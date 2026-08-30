use anyhow::Result;
use anyhow::bail;

use crate::ArmReceipt;
use crate::ExecutionMode;
use crate::ModeEvidence;
use crate::PairReceipt;
use crate::RunManifest;
use crate::blind_verify::ExactDocument;
use crate::proof_ledger::VerifiedLedgerArm;
use crate::proof_ledger::VerifiedNativeLedger;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct CostArmProjection {
    pub(crate) manifest: RunManifest,
    pub(crate) manifest_bytes: Vec<u8>,
    pub(crate) manifest_sha256: String,
    pub(crate) mode_evidence: ModeEvidence,
    pub(crate) ledger: VerifiedLedgerArm,
    pub(crate) arm_receipt: ArmReceipt,
    pub(crate) arm_receipt_sha256: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct CostPairProjection {
    pub(crate) execution_mode: ExecutionMode,
    pub(crate) execution_context_sha256: String,
    pub(crate) started_at: String,
    pub(crate) deadline: String,
    pub(crate) arm_order_commitment: String,
    pub(crate) provider_endpoint_commitment: Option<String>,
    pub(crate) pair_receipt: PairReceipt,
    pub(crate) pair_receipt_sha256: String,
    pub(crate) finished_at: String,
    pub(crate) attempt_ledger_sha256: String,
    pub(crate) attempt_index_root_sha256: String,
    pub(crate) arms: [CostArmProjection; 2],
}

pub(crate) struct NativeCostProjectionInput<'a> {
    pub(crate) execution_mode: ExecutionMode,
    pub(crate) execution_context_sha256: &'a str,
    pub(crate) started_at: &'a str,
    pub(crate) deadline: &'a str,
    pub(crate) arm_order_commitment: &'a str,
    pub(crate) manifests: &'a [ExactDocument<RunManifest>; 2],
    pub(crate) ledger: VerifiedNativeLedger,
}

pub(crate) fn project_verified_native_cost(
    input: NativeCostProjectionInput<'_>,
) -> Result<CostPairProjection> {
    let NativeCostProjectionInput {
        execution_mode,
        execution_context_sha256,
        started_at,
        deadline,
        arm_order_commitment,
        manifests,
        ledger,
    } = input;
    let provider_endpoint_commitment = match (
        execution_mode,
        &manifests[0].typed.mode_evidence,
        &manifests[1].typed.mode_evidence,
    ) {
        (
            ExecutionMode::Mock,
            first @ ModeEvidence::Mock { .. },
            second @ ModeEvidence::Mock { .. },
        ) if first == second => None,
        (
            ExecutionMode::Live,
            first @ ModeEvidence::Live {
                provider_endpoint_commitment,
                ..
            },
            second @ ModeEvidence::Live { .. },
        ) if first == second => Some(provider_endpoint_commitment.clone()),
        (ExecutionMode::Replay, _, _) => bail!("Replay has no Native cost projection"),
        (ExecutionMode::Mock | ExecutionMode::Live, _, _) => {
            bail!("Native cost projection mode evidence is inconsistent")
        }
    };
    let VerifiedNativeLedger {
        attempt_ledger,
        arm_receipts,
        pair_receipt,
        pair_receipt_sha256,
    } = ledger;
    let crate::proof_ledger::VerifiedAttemptLedger {
        attempt_index_sha256,
        attempt_index_root_sha256,
        arms: [first_ledger, second_ledger],
        ..
    } = attempt_ledger;
    let [first_receipt, second_receipt] = arm_receipts;
    let [first_manifest, second_manifest] = manifests;
    let project_arm =
        |manifest: &ExactDocument<RunManifest>, ledger, arm_receipt: ArmReceipt| {
            CostArmProjection {
                manifest: manifest.typed.clone(),
                manifest_bytes: manifest.raw_bytes.clone(),
                manifest_sha256: manifest.sha256.clone(),
                mode_evidence: manifest.typed.mode_evidence.clone(),
                ledger,
                arm_receipt_sha256: arm_receipt.receipt_sha256.clone(),
                arm_receipt,
            }
        };
    let finished_at = pair_receipt.finished_at.clone();

    Ok(CostPairProjection {
        execution_mode,
        execution_context_sha256: execution_context_sha256.to_string(),
        started_at: started_at.to_string(),
        deadline: deadline.to_string(),
        arm_order_commitment: arm_order_commitment.to_string(),
        provider_endpoint_commitment,
        pair_receipt,
        pair_receipt_sha256,
        finished_at,
        attempt_ledger_sha256: attempt_index_sha256,
        attempt_index_root_sha256,
        arms: [
            project_arm(first_manifest, first_ledger, first_receipt),
            project_arm(second_manifest, second_ledger, second_receipt),
        ],
    })
}
