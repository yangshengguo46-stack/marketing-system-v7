use std::fmt;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::Digest;
use sha2::Sha256;

use crate::EvaluationCondition;
use crate::ExecutionMode;
use crate::RunManifest;
use crate::blind::FrozenContextSnapshot;
use crate::blind::FrozenInputToken;
use crate::private_inventory::VerifiedPrivateInventory;
use crate::private_inventory::verify_private_inventory_state;
use crate::secure_fs::read_single_link_regular_bounded;
use crate::secure_fs::resolve_private_relative;

const EVIDENCE_FILE_CAP: u64 = 1024 * 1024;

#[derive(Debug)]
pub(crate) struct ParsedPairEvidence {
    pub(crate) inputs: FrozenInputToken,
    pub(crate) inventory: VerifiedPrivateInventory,
    pub(crate) inventory_binding: PrivateInventoryBinding,
    pub(crate) execution_context: ExactDocument<ExecutionContext>,
    pub(crate) arms: [ExactDocument<RunManifest>; 2],
    pub(crate) pair_verification: ExactDocument<PairVerification>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PrivateInventoryBinding {
    pub(crate) private_root: PathBuf,
    pub(crate) pair_id: String,
    pub(crate) frozen_run_context_sha256: String,
    pub(crate) inventory_root_sha256: String,
}

#[derive(Debug)]
pub(crate) struct ExactDocument<T> {
    pub(crate) typed: T,
    pub(crate) raw_bytes: Vec<u8>,
    pub(crate) sha256: String,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub(crate) enum ExecutionContext {
    Replay(ReplayExecutionContext),
    Native(NativeExecutionContext),
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub(crate) enum PairVerification {
    Replay(ReplayPairVerification),
    Native(NativePairVerification),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct PairEvidenceCoreStageNotInstalled;

impl fmt::Display for PairEvidenceCoreStageNotInstalled {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PairEvidenceCoreStageNotInstalled")
    }
}

impl std::error::Error for PairEvidenceCoreStageNotInstalled {}

pub(crate) fn verify_pair_evidence_core(
    snapshot: &FrozenContextSnapshot,
) -> Result<ParsedPairEvidence> {
    parse_pair_evidence(snapshot)?;
    Err(PairEvidenceCoreStageNotInstalled.into())
}

pub(crate) fn parse_pair_evidence(snapshot: &FrozenContextSnapshot) -> Result<ParsedPairEvidence> {
    let inputs = snapshot.verified_inputs()?;
    let binding = PairBinding::from_snapshot(snapshot, &inputs)?;
    let coordinator = match binding.mode {
        ExecutionMode::Replay => "replay-coordinator",
        ExecutionMode::Mock | ExecutionMode::Live => "coordinator",
    };
    let execution_path = evidence_path(
        &binding.private_root,
        &format!("{coordinator}/execution-context.json"),
    )?;
    let execution_context = match binding.mode {
        ExecutionMode::Replay => ExactDocument::read(&execution_path, "Replay execution context")?
            .map(ExecutionContext::Replay),
        ExecutionMode::Mock | ExecutionMode::Live => {
            ExactDocument::read(&execution_path, "Native execution context")?
                .map(ExecutionContext::Native)
        }
    };
    let arms = [1_u8, 2].map(|ordinal| {
        let path = evidence_path(
            &binding.private_root,
            &format!("{coordinator}/run-{ordinal}-manifest.json"),
        )?;
        ExactDocument::read(&path, "run manifest")
    });
    let arms = arms.into_iter().collect::<Result<Vec<_>>>()?;
    let arms: [ExactDocument<RunManifest>; 2] = arms
        .try_into()
        .map_err(|_| anyhow::anyhow!("sealed pair requires exactly two run manifests"))?;
    let pair_name = match binding.mode {
        ExecutionMode::Replay => "replay-pair-verification.json",
        ExecutionMode::Mock | ExecutionMode::Live => "pair-verification.json",
    };
    let pair_path = evidence_path(&binding.private_root, &format!("{coordinator}/{pair_name}"))?;
    let pair_verification = match binding.mode {
        ExecutionMode::Replay => ExactDocument::read(&pair_path, "Replay pair verification")?
            .map(PairVerification::Replay),
        ExecutionMode::Mock | ExecutionMode::Live => {
            ExactDocument::read(&pair_path, "Native pair verification")?
                .map(PairVerification::Native)
        }
    };
    validate_structural_links(&binding, &execution_context, &arms, &pair_verification)?;
    let inventory = verify_private_inventory_state(&binding.private_root)?;
    inventory.verify_binding(
        &binding.pair_id,
        &binding.frozen_run_context_sha256,
        &binding.private_root,
    )?;
    let inventory_binding = PrivateInventoryBinding {
        private_root: binding.private_root,
        pair_id: binding.pair_id,
        frozen_run_context_sha256: binding.frozen_run_context_sha256,
        inventory_root_sha256: inventory.inventory_root_sha256().to_string(),
    };
    Ok(ParsedPairEvidence {
        inputs,
        inventory,
        inventory_binding,
        execution_context,
        arms,
        pair_verification,
    })
}

struct PairBinding {
    mode: ExecutionMode,
    private_root: PathBuf,
    pair_id: String,
    fork_sha: String,
    frozen_run_context_sha256: String,
    fixture_set_sha256: Option<String>,
}

impl PairBinding {
    fn from_snapshot(snapshot: &FrozenContextSnapshot, inputs: &FrozenInputToken) -> Result<Self> {
        let (private_root, pair_id, fork_sha, frozen_sha, fixture_set_sha256) = match inputs {
            FrozenInputToken::Replay(verified) => {
                let projection = verified.projection();
                (
                    projection.private_root.to_path_buf(),
                    projection.pair_id.to_string(),
                    projection.fork_sha.to_string(),
                    projection.raw_sha256.to_string(),
                    Some(projection.fixture_set_sha256.to_string()),
                )
            }
            FrozenInputToken::Native { frozen, .. } => (
                frozen.private_root().to_path_buf(),
                frozen.pair_id().to_string(),
                frozen.fork_sha().to_string(),
                frozen.sha256().to_string(),
                None,
            ),
        };
        if snapshot.private_root() != private_root
            || snapshot.sha256() != frozen_sha
            || (snapshot.execution_mode() == ExecutionMode::Replay) != fixture_set_sha256.is_some()
        {
            bail!("blind-pack snapshot differs from its retained C1 token");
        }
        Ok(Self {
            mode: snapshot.execution_mode(),
            private_root,
            pair_id,
            fork_sha,
            frozen_run_context_sha256: frozen_sha,
            fixture_set_sha256,
        })
    }
}

impl<T> ExactDocument<T> {
    fn read(path: &Path, label: &str) -> Result<Self>
    where
        T: DeserializeOwned + Serialize,
    {
        let raw_bytes = read_single_link_regular_bounded(path, EVIDENCE_FILE_CAP)
            .with_context(|| format!("read bounded {label}"))?;
        let typed: T = serde_json::from_value(crate::jcs::parse_json(&raw_bytes)?)
            .with_context(|| format!("parse typed {label}"))?;
        if serde_json::to_vec_pretty(&typed)? != raw_bytes {
            bail!("{label} is not exact producer-order typed JSON without trailing bytes");
        }
        Ok(Self {
            sha256: sha256(&raw_bytes),
            raw_bytes,
            typed,
        })
    }

    fn map<U>(self, map: impl FnOnce(T) -> U) -> ExactDocument<U> {
        ExactDocument {
            typed: map(self.typed),
            raw_bytes: self.raw_bytes,
            sha256: self.sha256,
        }
    }
}

fn evidence_path(private_root: &Path, relative: &str) -> Result<PathBuf> {
    resolve_private_relative(private_root, Path::new(relative))
}

fn validate_structural_links(
    binding: &PairBinding,
    execution: &ExactDocument<ExecutionContext>,
    arms: &[ExactDocument<RunManifest>; 2],
    pair: &ExactDocument<PairVerification>,
) -> Result<()> {
    let conditions = arms.each_ref().map(|arm| arm.typed.condition);
    if arms[0].typed.run_ordinal != 1
        || arms[1].typed.run_ordinal != 2
        || conditions[0] == conditions[1]
        || !conditions.contains(&EvaluationCondition::Generic)
        || !conditions.contains(&EvaluationCondition::Candidate)
    {
        bail!("run manifests do not form the exact two-arm ordinal and condition set");
    }
    for arm in arms {
        let manifest = &arm.typed;
        let mode_matches = match binding.mode {
            ExecutionMode::Replay => manifest.execution_mode == ExecutionMode::Replay,
            ExecutionMode::Mock | ExecutionMode::Live => {
                matches!(
                    manifest.execution_mode,
                    ExecutionMode::Mock | ExecutionMode::Live
                )
            }
        };
        manifest.validate_execution_mode()?;
        if manifest.schema_version != 1
            || manifest.pair_id != binding.pair_id
            || manifest.frozen_run_context_sha256 != binding.frozen_run_context_sha256
            || manifest.execution_context_sha256 != execution.sha256
            || manifest.fork_sha != binding.fork_sha
            || !mode_matches
        {
            bail!("run manifest differs from the retained pair binding");
        }
    }
    match (&execution.typed, &pair.typed) {
        (ExecutionContext::Replay(execution), PairVerification::Replay(pair)) => {
            validate_replay_links(binding, execution, arms, pair)?
        }
        (ExecutionContext::Native(execution), PairVerification::Native(pair)) => {
            validate_native_links(binding, execution, arms, pair)?
        }
        _ => bail!("execution context and pair verification modes differ"),
    }
    Ok(())
}

fn validate_replay_links(
    binding: &PairBinding,
    execution: &ReplayExecutionContext,
    arms: &[ExactDocument<RunManifest>; 2],
    pair: &ReplayPairVerification,
) -> Result<()> {
    let fixture_sha = binding
        .fixture_set_sha256
        .as_deref()
        .context("missing retained Replay fixture-set commitment")?;
    if execution.schema_version != 1
        || execution.execution_mode != ExecutionMode::Replay
        || execution.provider_mode != "not-run"
        || execution.paid_provider_cost_fen != 0
        || execution.pair_id != binding.pair_id
        || execution.frozen_run_context_sha256 != binding.frozen_run_context_sha256
        || execution.fixture_set_sha256 != fixture_sha
        || execution.arms != [EvaluationCondition::Generic, EvaluationCondition::Candidate]
        || arms.each_ref().map(|arm| arm.typed.condition) != execution.arms
        || pair.schema_version != 1
        || pair.execution_mode != ExecutionMode::Replay
        || pair.provider_mode != "not-run"
        || pair.paid_provider_cost_fen != 0
        || pair.pair_id != binding.pair_id
        || pair.frozen_run_context_sha256 != binding.frozen_run_context_sha256
        || pair.fixture_set_sha256 != fixture_sha
    {
        bail!("Replay envelopes differ from the retained pair binding");
    }
    validate_direct_pair_links(arms, pair.direct_links())
}

fn validate_native_links(
    binding: &PairBinding,
    execution: &NativeExecutionContext,
    arms: &[ExactDocument<RunManifest>; 2],
    pair: &NativePairVerification,
) -> Result<()> {
    if execution.schema_version != 1
        || execution.provider_mode != "not-run"
        || execution.frozen_run_context_sha256 != binding.frozen_run_context_sha256
        || [execution.first_condition, execution.second_condition]
            != arms.each_ref().map(|arm| arm.typed.condition)
        || pair.schema_version != 1
        || pair.pair_id != binding.pair_id
        || pair.frozen_run_context_sha256 != binding.frozen_run_context_sha256
    {
        bail!("Native envelopes differ from the retained pair binding");
    }
    validate_direct_pair_links(arms, pair.direct_links())
}

struct DirectPairLinks<'a> {
    generic_manifest_sha256: &'a str,
    candidate_manifest_sha256: &'a str,
    generic_content_package_sha256: &'a str,
    candidate_content_package_sha256: &'a str,
    execution_context_sha256: Option<&'a str>,
}

fn validate_direct_pair_links(
    arms: &[ExactDocument<RunManifest>; 2],
    links: DirectPairLinks<'_>,
) -> Result<()> {
    let generic = arms
        .iter()
        .find(|arm| arm.typed.condition == EvaluationCondition::Generic)
        .context("missing generic manifest")?;
    let candidate = arms
        .iter()
        .find(|arm| arm.typed.condition == EvaluationCondition::Candidate)
        .context("missing candidate manifest")?;
    if links.generic_manifest_sha256 != generic.sha256
        || links.candidate_manifest_sha256 != candidate.sha256
        || links.generic_content_package_sha256 != generic.typed.content_package_sha256
        || links.candidate_content_package_sha256 != candidate.typed.content_package_sha256
        || links
            .execution_context_sha256
            .is_some_and(|sha| sha != generic.typed.execution_context_sha256)
    {
        bail!("pair verification direct SHA links do not match the sealed arm evidence");
    }
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct ReplayExecutionContext {
    arms: [EvaluationCondition; 2],
    execution_mode: ExecutionMode,
    fixture_set_sha256: String,
    frozen_run_context_sha256: String,
    paid_provider_cost_fen: u64,
    pair_id: String,
    provider_mode: String,
    schema_version: u32,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct NativeExecutionContext {
    arm_order_commitment: String,
    broker: BrokerEndpoint,
    candidate_codex_home: PathBuf,
    candidate_home: PathBuf,
    deadline: String,
    first_condition: EvaluationCondition,
    frozen_run_context_sha256: String,
    generic_codex_home: PathBuf,
    generic_home: PathBuf,
    max_elapsed_seconds_per_run: u64,
    max_output_tokens_per_request: u64,
    max_provider_request_attempts_per_run: u64,
    max_total_tokens_per_run: u64,
    model_label: String,
    path_sha256: String,
    provider_mode: String,
    schema_version: u32,
    second_condition: EvaluationCondition,
    shared_config_sha256: String,
    started_at: String,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct BrokerEndpoint {
    host: String,
    path: String,
    port: u16,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct ReplayPairVerification {
    candidate_content_package_sha256: String,
    candidate_run_manifest_sha256: String,
    candidate_skill_sha256: String,
    execution_mode: ExecutionMode,
    fixture_set_sha256: String,
    frozen_run_context_sha256: String,
    generic_content_package_sha256: String,
    generic_run_manifest_sha256: String,
    normalized_base_catalog_sha256: String,
    paid_provider_cost_fen: u64,
    pair_id: String,
    provider_mode: String,
    request_parity: PairRequestParity,
    schema_version: u32,
}

impl ReplayPairVerification {
    fn direct_links(&self) -> DirectPairLinks<'_> {
        DirectPairLinks {
            generic_manifest_sha256: &self.generic_run_manifest_sha256,
            candidate_manifest_sha256: &self.candidate_run_manifest_sha256,
            generic_content_package_sha256: &self.generic_content_package_sha256,
            candidate_content_package_sha256: &self.candidate_content_package_sha256,
            execution_context_sha256: None,
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct NativePairVerification {
    candidate_content_package_sha256: String,
    candidate_run_manifest_sha256: String,
    config_layers_sha256: String,
    effective_config_sha256: String,
    execution_context_sha256: String,
    frozen_run_context_sha256: String,
    generic_content_package_sha256: String,
    generic_run_manifest_sha256: String,
    normalized_base_catalog_sha256: String,
    pair_id: String,
    request_parity: PairRequestParity,
    schema_version: u32,
}

impl NativePairVerification {
    fn direct_links(&self) -> DirectPairLinks<'_> {
        DirectPairLinks {
            generic_manifest_sha256: &self.generic_run_manifest_sha256,
            candidate_manifest_sha256: &self.candidate_run_manifest_sha256,
            generic_content_package_sha256: &self.generic_content_package_sha256,
            candidate_content_package_sha256: &self.candidate_content_package_sha256,
            execution_context_sha256: Some(&self.execution_context_sha256),
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PairRequestParity {
    candidate_normalized_commitment: String,
    candidate_raw_commitment: String,
    candidate_treatment_diff_commitment: String,
    generic_normalized_commitment: String,
    generic_raw_commitment: String,
    normalized_base_commitment: String,
    schema_version: u32,
}
