use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use chrono::DateTime;
use chrono::SecondsFormat;
use chrono::Utc;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::Digest;
use sha2::Sha256;

use crate::ArmReceipt;
use crate::PairReceipt;

use super::NativeLedgerBinding;
use super::VerifiedAttemptLedger;
use super::VerifiedLedgerArm;

const RECEIPT_CAP: u64 = 1024 * 1024;
const FIRST_ARM_RECEIPT: &str = "coordinator/receipts/arm-1-receipt.json";
const SECOND_ARM_RECEIPT: &str = "coordinator/receipts/arm-2-receipt.json";
const PAIR_RECEIPT: &str = "coordinator/receipts/pair-receipt.json";
const POISON: &str = "coordinator/receipts/poison.json";
const REPLAY_LEDGER: &str = "coordinator/attempt-index.jsonl";
const REPLAY_RECEIPTS: &str = "coordinator/receipts";

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct VerifiedNativeLedger {
    pub(crate) attempt_ledger: VerifiedAttemptLedger,
    pub(crate) arm_receipts: [ArmReceipt; 2],
    pub(crate) pair_receipt: PairReceipt,
    pub(crate) pair_receipt_sha256: String,
}

struct DecodedReceipt<T> {
    typed: T,
    raw_sha256: String,
}

pub(crate) fn verify_native_proof_ledger(
    private_root: &Path,
    binding: &NativeLedgerBinding<'_>,
) -> Result<VerifiedNativeLedger> {
    ensure_absent(private_root, Path::new(POISON), "poison receipt is present")?;
    let attempt_ledger = super::derive_native_attempt_ledger(private_root, binding)?;
    let mut first = read_receipt::<ArmReceipt>(private_root, Path::new(FIRST_ARM_RECEIPT))?;
    let mut second = read_receipt::<ArmReceipt>(private_root, Path::new(SECOND_ARM_RECEIPT))?;
    let pair = read_receipt::<PairReceipt>(private_root, Path::new(PAIR_RECEIPT))?;

    verify_arm_receipt(
        &first.typed,
        &attempt_ledger.arms[0],
        binding,
        /*run_ordinal*/ 1,
        &attempt_ledger.arms,
        None,
    )?;
    verify_arm_receipt(
        &second.typed,
        &attempt_ledger.arms[1],
        binding,
        /*run_ordinal*/ 2,
        &attempt_ledger.arms,
        Some(&first.raw_sha256),
    )?;
    verify_receipt_timestamps(&first.typed, &second.typed, &pair.typed)?;
    verify_pair_receipt(
        &pair.typed,
        &attempt_ledger,
        binding,
        [&first.raw_sha256, &second.raw_sha256],
    )?;
    ensure_absent(private_root, Path::new(POISON), "poison receipt is present")?;

    first.typed.receipt_sha256 = first.raw_sha256;
    second.typed.receipt_sha256 = second.raw_sha256;
    Ok(VerifiedNativeLedger {
        attempt_ledger,
        arm_receipts: [first.typed, second.typed],
        pair_receipt: pair.typed,
        pair_receipt_sha256: pair.raw_sha256,
    })
}

pub(crate) fn verify_replay_proof_ledger_absence(private_root: &Path) -> Result<()> {
    for relative in [REPLAY_LEDGER, REPLAY_RECEIPTS] {
        ensure_absent(
            private_root,
            Path::new(relative),
            "replay native proof artifact is present",
        )?;
    }
    Ok(())
}

fn read_receipt<T: DeserializeOwned + Serialize>(
    private_root: &Path,
    relative: &Path,
) -> Result<DecodedReceipt<T>> {
    let path = crate::secure_fs::resolve_private_relative(private_root, relative)?;
    let raw = crate::secure_fs::read_single_link_regular_bounded(&path, RECEIPT_CAP)?;
    let body = raw
        .strip_suffix(b"\n")
        .context("receipt framing is invalid")?;
    if body.is_empty() || body.contains(&b'\n') || body.contains(&b'\r') {
        bail!("receipt framing is invalid");
    }
    let value = crate::jcs::parse_json(body).context("parse receipt JSON")?;
    let typed: T = serde_json::from_value(value).context("decode typed receipt JSON")?;
    if serde_json::to_vec(&typed)? != body {
        bail!("receipt is not exact compact typed JSON");
    }
    Ok(DecodedReceipt {
        typed,
        raw_sha256: sha256(&raw),
    })
}

fn verify_arm_receipt(
    receipt: &ArmReceipt,
    arm: &VerifiedLedgerArm,
    binding: &NativeLedgerBinding<'_>,
    run_ordinal: u8,
    order: &[VerifiedLedgerArm; 2],
    previous_sha256: Option<&String>,
) -> Result<()> {
    if receipt.schema_version != 1
        || receipt.pair_id != binding.pair_id
        || receipt.frozen_run_context_sha256 != binding.frozen_run_context_sha256
        || receipt.execution_context_sha256 != binding.execution_context_sha256
        || receipt.run_ordinal != run_ordinal
        || receipt.condition != arm.condition
    {
        bail!("arm receipt identity is invalid");
    }
    if receipt.first_condition != order[0].condition
        || receipt.second_condition != order[1].condition
    {
        bail!("arm receipt order is invalid");
    }
    if receipt.attempt_index_file_sha256 != arm.attempt_index_prefix_sha256
        || receipt.attempt_index_merkle_root != arm.attempt_index_prefix_root_sha256
    {
        bail!("arm receipt ledger snapshot is invalid");
    }
    if receipt.global_attempt_start_inclusive != arm.global_start_inclusive
        || receipt.global_attempt_end_exclusive != arm.global_end_exclusive
        || receipt.attempt_count != arm.provider_request_attempt_count
        || receipt.completion_count != arm.provider_completed_response_count
        || receipt.failure_count != 0
        || receipt.timeout_count != 0
        || receipt.in_flight != 0
    {
        bail!("arm receipt attempt accounting is invalid");
    }
    if receipt.previous_arm_receipt_sha256.as_ref() != previous_sha256 {
        bail!("arm receipt previous link is invalid");
    }
    if receipt.run_manifest_sha256.as_deref() != Some(&arm.run_manifest_sha256) {
        bail!("arm receipt run manifest link is invalid");
    }
    Ok(())
}

fn verify_pair_receipt(
    pair: &PairReceipt,
    ledger: &VerifiedAttemptLedger,
    binding: &NativeLedgerBinding<'_>,
    arm_sha256: [&String; 2],
) -> Result<()> {
    if pair.schema_version != 1
        || pair.pair_id != binding.pair_id
        || pair.frozen_run_context_sha256 != binding.frozen_run_context_sha256
        || pair.execution_context_sha256 != binding.execution_context_sha256
    {
        bail!("pair receipt identity is invalid");
    }
    if pair.first_arm_receipt_sha256 != *arm_sha256[0]
        || pair.second_arm_receipt_sha256 != *arm_sha256[1]
    {
        bail!("pair receipt arm links are invalid");
    }
    if pair.first_run_manifest_sha256.as_deref() != Some(&ledger.arms[0].run_manifest_sha256)
        || pair.second_run_manifest_sha256.as_deref() != Some(&ledger.arms[1].run_manifest_sha256)
    {
        bail!("pair receipt manifest links are invalid");
    }
    let attempts = ledger.arms[0]
        .provider_request_attempt_count
        .checked_add(ledger.arms[1].provider_request_attempt_count)
        .context("verified attempt total overflow")?;
    let completions = ledger.arms[0]
        .provider_completed_response_count
        .checked_add(ledger.arms[1].provider_completed_response_count)
        .context("verified completion total overflow")?;
    if pair.total_attempt_count != attempts
        || pair.total_completion_count != completions
        || pair.total_failure_count != 0
        || pair.total_timeout_count != 0
    {
        bail!("pair receipt totals are invalid");
    }
    if pair.arm_order_commitment != binding.arm_order_commitment {
        bail!("pair receipt order is invalid");
    }
    if pair.final_attempt_index_root != ledger.attempt_index_root_sha256 {
        bail!("pair receipt final root is invalid");
    }
    Ok(())
}

fn verify_receipt_timestamps(
    first: &ArmReceipt,
    second: &ArmReceipt,
    pair: &PairReceipt,
) -> Result<()> {
    let first = exact_timestamp(&first.sealed_at)?;
    let second = exact_timestamp(&second.sealed_at)?;
    let finished = exact_timestamp(&pair.finished_at)?;
    if first > second || second > finished {
        bail!("receipt timestamp order is invalid");
    }
    Ok(())
}

fn exact_timestamp(value: &str) -> Result<DateTime<Utc>> {
    let timestamp = DateTime::parse_from_rfc3339(value)
        .context("receipt timestamp format is invalid")?
        .with_timezone(&Utc);
    if timestamp.to_rfc3339_opts(SecondsFormat::Millis, true) != value {
        bail!("receipt timestamp format is invalid");
    }
    Ok(timestamp)
}

fn ensure_absent(private_root: &Path, relative: &Path, message: &str) -> Result<()> {
    let path = crate::secure_fs::resolve_private_relative(private_root, relative)?;
    match std::fs::symlink_metadata(path) {
        Ok(_) => bail!("{message}"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("inspect native proof artifact absence"),
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
