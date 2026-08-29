//! Trusted filesystem and sealed-evidence authority for `score`.

use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use chrono::DateTime;
use chrono::SecondsFormat;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::Digest;
use sha2::Sha256;

use crate::ExecutionMode;
use crate::FrozenContracts;
use crate::ScoreArgs;
use crate::blind::FrozenContextSnapshot;
use crate::blind_bundle_model::BlindPackReceipt;
use crate::blind_verify::PairEvidenceCore;
use crate::private_inventory::batch::RetainedPendingReview;
use crate::secure_fs::resolve_private_relative;
use crate::secure_fs_retain::RetainedBoundedFile;
use crate::secure_fs_retain::RetainedLeafPermissions;
use crate::secure_fs_retain::RetainedPrivateRoot;

const MAPPING_DIR: &str = "coordinator/mappings";
const REVIEWS_DIR: &str = "reviews";
const OUTPUT: &str = "coordinator/decision.private.json";
const STAGING_OUTPUT: &str = "coordinator/.decision.private.json.staging";
const FROZEN_CONTEXT: &str = "frozen-run-context.json";
const RECEIPT: &str = "coordinator/blind-pack-receipt.json";
const RECEIPT_CAP: u64 = 64 * 1024;
const VALIDATION_SENTINEL: &str = "score review validation stage is not installed";

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ResolvedScorePaths {
    mapping_dir: PathBuf,
    reviews_dir: PathBuf,
    output: PathBuf,
    staging_output: PathBuf,
    frozen_run_context: PathBuf,
}

struct RetainedBlindPackReceipt {
    typed: BlindPackReceipt,
    retained: RetainedBoundedFile,
}

pub(crate) struct ReceiptAuthorityBinding<'a> {
    pub(crate) mode: ExecutionMode,
    pub(crate) pair_id: &'a str,
    pub(crate) frozen_run_context_sha256: &'a str,
    pub(crate) pair_receipt_sha256: Option<&'a str>,
    pub(crate) pair_verification_sha256: &'a str,
    pub(crate) reviewer_ids: [&'a str; 3],
}

struct VerifiedScoreAuthority {
    snapshot: FrozenContextSnapshot,
    pair: PairEvidenceCore,
    receipt: RetainedBlindPackReceipt,
    reviews: [RetainedPendingReview; 3],
    private_root: RetainedPrivateRoot,
    paths: ResolvedScorePaths,
}

impl ResolvedScorePaths {
    pub(crate) fn mapping_dir(&self) -> &Path {
        &self.mapping_dir
    }

    pub(crate) fn reviews_dir(&self) -> &Path {
        &self.reviews_dir
    }

    pub(crate) fn output(&self) -> &Path {
        &self.output
    }

    pub(crate) fn staging_output(&self) -> &Path {
        &self.staging_output
    }

    pub(crate) fn frozen_run_context(&self) -> &Path {
        &self.frozen_run_context
    }
}

impl RetainedBlindPackReceipt {
    fn retain(private_root: &Path) -> Result<Self> {
        let path = resolve_private_relative(private_root, Path::new(RECEIPT))?;
        let retained = RetainedBoundedFile::retain(
            &path,
            RECEIPT_CAP,
            RetainedLeafPermissions::RequireOwnerOnly,
        )?;
        let typed = exact_jcs(retained.raw_bytes(), "blind pack receipt")?;
        retained.reverify_unchanged()?;
        Ok(Self { typed, retained })
    }

    fn raw_sha256(&self) -> String {
        sha256(self.retained.raw_bytes())
    }

    fn reverify_unchanged(&self) -> Result<()> {
        self.retained.reverify_unchanged()
    }
}

impl VerifiedScoreAuthority {
    fn reverify_unchanged(&self, args: &ScoreArgs) -> Result<()> {
        for review in &self.reviews {
            review.reverify_unchanged()?;
        }
        self.snapshot.reverify_unchanged()?;
        self.receipt.reverify_unchanged()?;
        crate::blind_finalize::reverify_sealed_pair_authority(&self.pair)?;
        self.private_root.reverify_unchanged()?;
        if resolve_score_paths(args, &self.pair.private_root)? != self.paths {
            bail!("score paths changed after authority verification");
        }
        self.private_root.reverify_unchanged()?;
        crate::blind_finalize::reverify_sealed_pair_authority(&self.pair)?;
        self.receipt.reverify_unchanged()?;
        self.snapshot.reverify_unchanged()?;
        for review in &self.reviews {
            review.reverify_unchanged()?;
        }
        Ok(())
    }
}

pub(crate) fn run_score_authority(args: ScoreArgs) -> Result<()> {
    let snapshot = crate::blind::read_context_snapshot(&args.frozen_run_context)?;
    let paths = resolve_score_paths(&args, snapshot.private_root())?;
    if paths.frozen_run_context() != snapshot.canonical_path() {
        bail!("score frozen context differs from its retained snapshot");
    }
    let inputs = snapshot.verified_inputs()?;
    let projection = inputs.score_projection()?;
    if projection.private_root != snapshot.private_root()
        || projection.frozen_run_context_sha256 != snapshot.sha256()
        || (projection.mode == ExecutionMode::Replay)
            != (snapshot.execution_mode() == ExecutionMode::Replay)
    {
        bail!("score frozen input projection differs from its retained snapshot");
    }
    let private_root_path = projection.private_root.to_path_buf();
    let pair_id = projection.pair_id.to_string();
    let frozen_sha256 = projection.frozen_run_context_sha256.to_string();
    let reviewer_ids = projection
        .reviewers
        .map(|reviewer| reviewer.reviewer_id.clone());
    let private_root = RetainedPrivateRoot::retain(&private_root_path)?;
    let reviews = reviewer_ids
        .each_ref()
        .map(|reviewer_id| RetainedPendingReview::retain(&private_root_path, reviewer_id));
    let reviews = reviews.into_iter().collect::<Result<Vec<_>>>()?;
    let reviews: [RetainedPendingReview; 3] = reviews
        .try_into()
        .map_err(|_| anyhow::anyhow!("score requires exactly three retained reviews"))?;
    let receipt = RetainedBlindPackReceipt::retain(&private_root_path)?;
    let receipt_sha256 = receipt.raw_sha256();
    let continuation = crate::private_inventory::batch::verify_private_inventory_continuation(
        &private_root_path,
        reviews,
        &receipt_sha256,
        &receipt.typed.inventory_root_sha256,
    )?;
    continuation.verify_binding(&pair_id, &frozen_sha256, &private_root_path)?;
    continuation.reverify_unchanged()?;
    let (inventory, reviews) = continuation.into_parts();
    let pair = crate::blind_verify::verify_pair_evidence_core_from(&snapshot, inputs, inventory)?;
    verify_receipt(&receipt.typed, &receipt_binding(&pair))?;
    let authority = VerifiedScoreAuthority {
        snapshot,
        pair,
        receipt,
        reviews,
        private_root,
        paths,
    };
    authority.reverify_unchanged(&args)?;
    bail!(VALIDATION_SENTINEL)
}

pub(crate) fn resolve_score_paths(
    args: &ScoreArgs,
    private_root: &Path,
) -> Result<ResolvedScorePaths> {
    let expected_reviews = private_root.join(REVIEWS_DIR);
    let expected_output = private_root.join(OUTPUT);
    let expected_frozen = private_root.join(FROZEN_CONTEXT);
    if args.mapping_dir.as_os_str() != OsStr::new(MAPPING_DIR)
        || !one_of_exact_paths(&args.reviews_dir, Path::new(REVIEWS_DIR), &expected_reviews)
        || !one_of_exact_paths(&args.output, Path::new(OUTPUT), &expected_output)
        || args.frozen_run_context.as_os_str() != expected_frozen.as_os_str()
    {
        bail!("score paths must use the exact frozen private-root path table");
    }

    let mapping_dir = resolve_private_relative(private_root, Path::new(MAPPING_DIR))?;
    let reviews_dir = resolve_private_relative(private_root, Path::new(REVIEWS_DIR))?;
    let output = resolve_private_relative(private_root, Path::new(OUTPUT))?;
    let staging_output = resolve_private_relative(private_root, Path::new(STAGING_OUTPUT))?;
    let frozen_run_context = resolve_private_relative(private_root, Path::new(FROZEN_CONTEXT))?;
    crate::runner::validate_private_existing_directory_no_follow(&mapping_dir)
        .context("validate score mapping directory")?;
    crate::runner::validate_private_existing_directory_no_follow(&reviews_dir)
        .context("validate score reviews directory")?;
    crate::runner::validate_private_existing_directory_no_follow(
        output.parent().context("score output has no parent")?,
    )
    .context("validate score output directory")?;
    let frozen_metadata = std::fs::symlink_metadata(&frozen_run_context)
        .context("inspect exact score frozen context leaf")?;
    if !frozen_metadata.is_file() || frozen_metadata.file_type().is_symlink() {
        bail!("score frozen context is not an existing regular private file");
    }
    ensure_absent(
        &staging_output,
        /* message */ "score staging output already exists",
    )?;
    ensure_absent(
        &output,
        /* message */ "score final output already exists",
    )?;
    Ok(ResolvedScorePaths {
        mapping_dir,
        reviews_dir,
        output,
        staging_output,
        frozen_run_context,
    })
}

fn one_of_exact_paths(actual: &Path, relative: &Path, absolute: &Path) -> bool {
    actual.as_os_str() == relative.as_os_str() || actual.as_os_str() == absolute.as_os_str()
}

fn ensure_absent(path: &Path, message: &str) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => bail!("{message}"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("inspect absent score path {}", path.display()))
        }
    }
}

fn receipt_binding(pair: &PairEvidenceCore) -> ReceiptAuthorityBinding<'_> {
    ReceiptAuthorityBinding {
        mode: pair.mode,
        pair_id: &pair.pair_id,
        frozen_run_context_sha256: &pair.frozen_run_context_sha256,
        pair_receipt_sha256: pair.native_pair_receipt_raw_sha256.as_deref(),
        pair_verification_sha256: &pair.pair_verification_raw_sha256,
        reviewer_ids: pair
            .reviewers
            .each_ref()
            .map(|reviewer| reviewer.reviewer_id.as_str()),
    }
}

pub(crate) fn verify_receipt(
    receipt: &BlindPackReceipt,
    binding: &ReceiptAuthorityBinding<'_>,
) -> Result<()> {
    let contracts = FrozenContracts::load()?.blind_review_contracts()?;
    let generated = DateTime::parse_from_rfc3339(&receipt.generated_at)?;
    let generated_is_canonical = generated
        .to_utc()
        .to_rfc3339_opts(SecondsFormat::Millis, /*use_z*/ true)
        == receipt.generated_at;
    let reviewers_match = receipt.reviewer_mappings.len() == binding.reviewer_ids.len()
        && receipt
            .reviewer_mappings
            .iter()
            .zip(binding.reviewer_ids)
            .all(|(mapping, reviewer_id)| {
                mapping.reviewer_id == reviewer_id
                    && lower_sha256(&mapping.review_bundle_sha256)
                    && lower_sha256(&mapping.mapping_sha256)
                    && lower_sha256(&mapping.seed_commitment)
            });
    let mapping_commitments_are_unique = match receipt.reviewer_mappings.as_slice() {
        [first, second, third] => {
            three_distinct(&first.reviewer_id, &second.reviewer_id, &third.reviewer_id)
                && three_distinct(
                    &first.review_bundle_sha256,
                    &second.review_bundle_sha256,
                    &third.review_bundle_sha256,
                )
                && three_distinct(
                    &first.mapping_sha256,
                    &second.mapping_sha256,
                    &third.mapping_sha256,
                )
                && three_distinct(
                    &first.seed_commitment,
                    &second.seed_commitment,
                    &third.seed_commitment,
                )
        }
        _ => false,
    };
    if receipt.schema_version != 1
        || receipt.pair_id != binding.pair_id
        || receipt.frozen_run_context_sha256 != binding.frozen_run_context_sha256
        || receipt.pair_receipt_sha256.as_deref() != binding.pair_receipt_sha256
        || receipt.pair_verification_sha256 != binding.pair_verification_sha256
        || receipt.rubric_sha256 != contracts.rubric_sha256
        || receipt.decision_policy_sha256 != contracts.decision_policy_sha256
        || receipt.reviewer_submission_schema_sha256 != contracts.reviewer_submission_schema_sha256
        || !lower_sha256(&receipt.inventory_root_sha256)
        || receipt.reviews_drop_sha256 != sha256(br#"{"entries":[]}"#)
        || !generated_is_canonical
        || !reviewers_match
        || !mapping_commitments_are_unique
        || (binding.mode == ExecutionMode::Replay) != receipt.pair_receipt_sha256.is_none()
    {
        bail!("blind pack receipt differs from the sealed score authority");
    }
    Ok(())
}

fn three_distinct(first: &str, second: &str, third: &str) -> bool {
    first != second && first != third && second != third
}

fn exact_jcs<T>(bytes: &[u8], label: &str) -> Result<T>
where
    T: DeserializeOwned + Serialize,
{
    let typed: T = serde_json::from_value(crate::jcs::parse_json(bytes)?)?;
    if crate::jcs::canonicalize_value(&serde_json::to_value(&typed)?)? != bytes {
        bail!("{label} is not exact canonical JCS");
    }
    Ok(typed)
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}
