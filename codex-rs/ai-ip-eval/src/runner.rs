use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use rand::TryRngCore;
use rand::rngs::OsRng;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

use crate::AppServerClient;
use crate::ArmActivation;
use crate::BrokerGateConfig;
use crate::BrokerRuntimeConfig;
use crate::CatalogRoots;
use crate::CatalogSnapshot;
use crate::CollectedReplay;
use crate::ConfigAuditEvidence;
use crate::EvaluationCondition;
use crate::FrozenContracts;
use crate::NativeHeldOutAttestation;
use crate::PairCoordinator;
use crate::ReplayCollector;
use crate::SkillUseTracker;
use crate::ThreadLifecycle;
use crate::TreeEvidence;
use crate::audit_frozen_config;
use crate::build_shared_config;
use crate::build_thread_start;
use crate::build_turn_start;
use crate::jcs::canonicalize_value;
use crate::model::LiveFreezeArgs;
use crate::model::MockProviderMode;
use crate::model::ModeEvidence;
use crate::model::ProofBrokerCompatibilityName;
use crate::model::ProviderRole;
use crate::model::ReplayFreezeArgs;
use crate::model::ReplayPairArgs;
use crate::model::RunManifest;

/// A frozen context whose current bytes were re-read and hashed successfully.
#[derive(Debug, Clone)]
pub struct VerifiedFrozenContext {
    canonical_path: PathBuf,
    sha256: String,
    raw_bytes: Vec<u8>,
    context: FrozenRunContext,
    frozen_file: ArtifactCommitment,
    artifacts: ArtifactCommitments,
    commitment_key: crate::proof_commitment::RetainedProofCommitmentKey,
}

const NATIVE_PROVIDER_LABEL: &str = "synthetic-loopback-mock";

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct NativeMaterialInput {
    pub(crate) material_id: String,
    pub(crate) relative_path: String,
    pub(crate) sha256: String,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct NativeContentInputs {
    pub(crate) mission_case: codex_ai_ip_domain::HeldOutMissionCase,
    pub(crate) case_bytes: Vec<u8>,
    pub(crate) attestation: NativeHeldOutAttestation,
    pub(crate) reviewers: [crate::ReviewerDeclaration; 3],
    pub(crate) attestation_bytes: Vec<u8>,
    pub(crate) materials_manifest: Vec<codex_ai_ip_domain::MissionMaterial>,
    pub(crate) materials_manifest_bytes: Vec<u8>,
    pub(crate) materials: Vec<NativeMaterialInput>,
    pub(crate) prompt_bytes: Vec<u8>,
    pub(crate) additional_context_bytes: Vec<u8>,
    pub(crate) schema_bytes: Vec<u8>,
    pub(crate) thread_start_bytes: Vec<u8>,
    pub(crate) turn_start_bytes: Vec<u8>,
    pub(crate) skill_bytes: Vec<u8>,
    pub(crate) codex_binary_sha256: String,
    pub(crate) evaluator_binary_sha256: String,
    pub(crate) broker_component_sha256: String,
    pub(crate) model_label: String,
    pub(crate) provider_mode: String,
    pub(crate) provider_label: String,
    pub(crate) provider_compatibility_name: ProofBrokerCompatibilityName,
    pub(crate) max_output_tokens: u64,
    pub(crate) max_provider_request_attempts: u64,
    pub(crate) max_total_tokens_per_run: u64,
    pub(crate) max_elapsed_seconds_per_run: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct VerifiedNativeContentInputs {
    inputs: NativeContentInputs,
    context: FrozenRunContext,
    artifacts: ArtifactCommitments,
}

impl VerifiedNativeContentInputs {
    pub(crate) fn projection(&self) -> &NativeContentInputs {
        &self.inputs
    }

    pub(crate) fn reverify_unchanged(&self) -> Result<()> {
        self.artifacts.verify()?;
        let observed = collect_native_content_inputs(&self.context, &self.artifacts)?;
        if observed != self.inputs {
            bail!("verified Native content inputs changed after verification");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FrozenArtifactReference {
    path: PathBuf,
    sha256: String,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FrozenRunContext {
    schema_version: u32,
    execution_mode: String,
    provider_mode: String,
    pair_id: String,
    public_run_id: String,
    commitment_key_sha256: String,
    candidate_sha: String,
    private_root: PathBuf,
    repo_root: PathBuf,
    repo_head: String,
    provider_upstream_url: String,
    model_label: String,
    max_output_tokens: u64,
    max_attempts_per_arm: u64,
    max_total_tokens: u64,
    max_elapsed_seconds: u64,
    supplier_statement_policy: SupplierStatementPolicy,
    artifacts: BTreeMap<String, FrozenArtifactReference>,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SupplierStatementPolicy {
    directory: String,
    allowed_leaves: [String; 2],
    nested_entries_allowed: bool,
}

impl VerifiedFrozenContext {
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }

    pub fn pair_id(&self) -> &str {
        &self.context.pair_id
    }

    pub fn max_output_tokens(&self) -> u64 {
        self.context.max_output_tokens
    }

    pub fn max_attempts_per_arm(&self) -> u64 {
        self.context.max_attempts_per_arm
    }

    pub fn provider_upstream_url(&self) -> &str {
        &self.context.provider_upstream_url
    }

    fn execution_guard(&self) -> Result<FrozenExecutionGuard> {
        FrozenExecutionGuard::from_commitments(self.artifacts.clone(), &self.context.repo_root)
    }

    pub(crate) fn raw_bytes(&self) -> Result<Vec<u8>> {
        Ok(self.raw_bytes.clone())
    }

    pub(crate) fn private_root(&self) -> &Path {
        &self.context.private_root
    }

    pub(crate) fn fork_sha(&self) -> &str {
        &self.context.repo_head
    }

    pub(crate) fn model_label(&self) -> &str {
        &self.context.model_label
    }

    pub(crate) fn provider_mode(&self) -> &str {
        &self.context.provider_mode
    }

    pub(crate) fn max_total_tokens_per_run(&self) -> u64 {
        self.context.max_total_tokens
    }

    pub(crate) fn max_elapsed_seconds_per_run(&self) -> u64 {
        self.context.max_elapsed_seconds
    }

    pub(crate) fn artifact_bytes(&self, name: &str) -> Result<Vec<u8>> {
        self.artifacts.read_verified(name)
    }

    pub(crate) fn artifact_sha256(&self, name: &str) -> Result<&str> {
        self.artifacts
            .sha256(name)
            .with_context(|| format!("missing frozen artifact {name}"))
    }

    pub(crate) fn reverify_all(&self) -> Result<()> {
        self.frozen_file
            .verify_context_bytes(&self.raw_bytes)
            .context("reverify native frozen context identity")?;
        if sha256(
            format!("{}:{}", self.context.repo_head, self.context.model_label).as_bytes(),
        ) != self.context.pair_id
        {
            bail!("native pair ID changed from its frozen repository and model inputs");
        }
        self.commitment_key.reverify_binding(
            &self.context.commitment_key_sha256,
            &self.context.pair_id,
            &self.context.public_run_id,
        )?;
        self.artifacts.verify()?;
        validate_managed_source_artifacts(self).map(drop)
    }

    pub(crate) fn native_content_inputs(&self) -> Result<VerifiedNativeContentInputs> {
        validate_managed_source_artifacts(self)
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ReplayFrozenContext {
    schema_version: u32,
    execution_mode: String,
    provider_mode: String,
    pair_id: String,
    public_run_id: String,
    commitment_key_sha256: String,
    repo_root: PathBuf,
    fork_sha: String,
    private_root: PathBuf,
    codex_binary: FrozenArtifactReference,
    evaluator_binary: FrozenArtifactReference,
    fixture_set_manifest: FrozenArtifactReference,
    fixtures: BTreeMap<String, FrozenArtifactReference>,
}

#[derive(Debug, Clone)]
struct VerifiedReplayReference {
    commitment: ArtifactCommitment,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ReplayMaterialInput {
    pub(crate) material_id: String,
    pub(crate) relative_path: String,
    pub(crate) sha256: String,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
struct VerifiedReplayMaterial {
    input: ReplayMaterialInput,
    commitment: ArtifactCommitment,
}

#[derive(Debug, Clone)]
enum ReplayBinaryCommitments {
    Shared(ArtifactCommitment),
    Distinct {
        codex: ArtifactCommitment,
        evaluator: ArtifactCommitment,
    },
}

impl ReplayBinaryCommitments {
    fn retain(
        codex_reference: &FrozenArtifactReference,
        evaluator_reference: &FrozenArtifactReference,
    ) -> Result<Self> {
        let codex_path = canonical_replay_reference(codex_reference, "Codex binary")?;
        let evaluator_path = canonical_replay_reference(evaluator_reference, "evaluator binary")?;
        if codex_reference.path == evaluator_reference.path
            && codex_reference.sha256 == evaluator_reference.sha256
        {
            return Ok(Self::Shared(retain_canonical_replay_reference(
                &codex_path,
                codex_reference,
                "Codex binary",
            )?));
        }
        Ok(Self::Distinct {
            codex: retain_canonical_replay_reference(
                &codex_path,
                codex_reference,
                "Codex binary",
            )?,
            evaluator: retain_canonical_replay_reference(
                &evaluator_path,
                evaluator_reference,
                "evaluator binary",
            )?,
        })
    }

    fn codex_sha256(&self) -> &str {
        match self {
            Self::Shared(commitment) => &commitment.sha256,
            Self::Distinct { codex, .. } => &codex.sha256,
        }
    }

    fn evaluator_sha256(&self) -> &str {
        match self {
            Self::Shared(commitment) => &commitment.sha256,
            Self::Distinct { evaluator, .. } => &evaluator.sha256,
        }
    }

    fn reverify_all(&self) -> Result<()> {
        match self {
            Self::Shared(commitment) => {
                commitment.read_verified()?;
            }
            Self::Distinct { codex, evaluator } => {
                codex.read_verified()?;
                evaluator.read_verified()?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct VerifiedReplayFrozenContext {
    canonical_path: PathBuf,
    raw_bytes: Vec<u8>,
    raw_sha256: String,
    frozen_file: ArtifactCommitment,
    context: ReplayFrozenContext,
    commitment_key: crate::proof_commitment::RetainedProofCommitmentKey,
    fixture_set: ArtifactCommitment,
    fixtures: BTreeMap<String, VerifiedReplayReference>,
    materials: Vec<VerifiedReplayMaterial>,
    binaries: ReplayBinaryCommitments,
    mission_case: codex_ai_ip_domain::HeldOutMissionCase,
    attestation: crate::ReplayReviewAttestation,
    materials_manifest_bytes: Vec<u8>,
    prompt_bytes: Vec<u8>,
    additional_context_bytes: Vec<u8>,
    schema_bytes: Vec<u8>,
    thread_start_bytes: Vec<u8>,
    turn_start_bytes: Vec<u8>,
}

pub(crate) struct ReplayFrozenInputProjection<'a> {
    pub(crate) canonical_path: &'a Path,
    pub(crate) raw_bytes: &'a [u8],
    pub(crate) raw_sha256: &'a str,
    pub(crate) private_root: &'a Path,
    pub(crate) pair_id: &'a str,
    pub(crate) fork_sha: &'a str,
    pub(crate) fixture_set_sha256: &'a str,
    pub(crate) mission_case: &'a codex_ai_ip_domain::HeldOutMissionCase,
    pub(crate) materials: Vec<ReplayMaterialInput>,
    pub(crate) attestation: &'a crate::ReplayReviewAttestation,
    pub(crate) materials_manifest_bytes: &'a [u8],
    pub(crate) prompt_bytes: &'a [u8],
    pub(crate) additional_context_bytes: &'a [u8],
    pub(crate) schema_bytes: &'a [u8],
    pub(crate) thread_start_bytes: &'a [u8],
    pub(crate) turn_start_bytes: &'a [u8],
    pub(crate) codex_binary_sha256: &'a str,
    pub(crate) evaluator_binary_sha256: &'a str,
    pub(crate) broker_component_sha256: String,
    pub(crate) model_label: &'static str,
    pub(crate) provider_label: &'static str,
    pub(crate) max_provider_request_attempts: u64,
    pub(crate) max_total_tokens: u64,
    pub(crate) max_elapsed_seconds: u64,
}

impl VerifiedReplayFrozenContext {
    pub(crate) fn projection(&self) -> ReplayFrozenInputProjection<'_> {
        ReplayFrozenInputProjection {
            canonical_path: &self.canonical_path,
            raw_bytes: &self.raw_bytes,
            raw_sha256: &self.raw_sha256,
            private_root: &self.context.private_root,
            pair_id: &self.context.pair_id,
            fork_sha: &self.context.fork_sha,
            fixture_set_sha256: &self.fixture_set.sha256,
            mission_case: &self.mission_case,
            materials: self
                .materials
                .iter()
                .map(|material| material.input.clone())
                .collect(),
            attestation: &self.attestation,
            materials_manifest_bytes: &self.materials_manifest_bytes,
            prompt_bytes: &self.prompt_bytes,
            additional_context_bytes: &self.additional_context_bytes,
            schema_bytes: &self.schema_bytes,
            thread_start_bytes: &self.thread_start_bytes,
            turn_start_bytes: &self.turn_start_bytes,
            codex_binary_sha256: self.binaries.codex_sha256(),
            evaluator_binary_sha256: self.binaries.evaluator_sha256(),
            broker_component_sha256: sha256(b"replay:no-broker-component"),
            model_label: "replay-fixture",
            provider_label: "not-run",
            max_provider_request_attempts: 0,
            max_total_tokens: 0,
            max_elapsed_seconds: 0,
        }
    }

    pub(crate) fn fixture_bytes(&self, name: &str) -> Result<&[u8]> {
        Ok(&self
            .fixtures
            .get(name)
            .with_context(|| format!("missing verified replay fixture {name}"))?
            .bytes)
    }

    pub(crate) fn reverify_all(&self) -> Result<()> {
        self.frozen_file
            .verify_context_bytes(&self.raw_bytes)
            .context("reverify Replay frozen context identity")?;
        if replay_pair_id(&self.context.fork_sha, &self.context.fixtures)? != self.context.pair_id {
            bail!("Replay pair ID changed from its frozen execution inputs");
        }
        self.commitment_key.reverify_binding(
            &self.context.commitment_key_sha256,
            &self.context.pair_id,
            &self.context.public_run_id,
        )?;
        self.fixture_set.read_verified()?;
        self.binaries.reverify_all()?;
        for (name, fixture) in &self.fixtures {
            if fixture.commitment.read_verified()? != fixture.bytes {
                bail!("verified replay fixture changed after verification: {name}");
            }
        }
        for material in &self.materials {
            let material_id = &material.input.material_id;
            let bytes = material
                .commitment
                .read_verified()
                .with_context(|| format!("reverify Replay material {material_id}"))?;
            if bytes != material.input.bytes {
                bail!("verified Replay material changed after verification: {material_id}");
            }
        }
        Ok(())
    }

    fn context(&self) -> &ReplayFrozenContext {
        &self.context
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ReplayFixtureSet {
    schema_version: u32,
    execution_mode: String,
    fixtures: Vec<ReplayFixtureEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ReplayFixtureEntry {
    name: String,
    path: PathBuf,
    sha256: String,
}

pub(crate) struct ImportedSourceProof {
    pub(crate) mission_case: codex_ai_ip_domain::HeldOutMissionCase,
    pub(crate) case_path: PathBuf,
    pub(crate) attestation_path: PathBuf,
    pub(crate) materials_manifest_path: PathBuf,
    pub(crate) material_paths: BTreeMap<String, PathBuf>,
    case_bytes: Vec<u8>,
    attestation_bytes: Vec<u8>,
    attestation: NativeHeldOutAttestation,
    material_bytes: BTreeMap<String, Vec<u8>>,
}

pub(crate) struct RetainedLiveInputDirectory {
    private_root: PathBuf,
    private_handle: File,
    inputs_handle: File,
    cost_inputs: crate::cost_inputs::RetainedPrivateCostInputs,
}

impl RetainedLiveInputDirectory {
    pub(crate) fn retain(
        private_root: &Path,
        attestation: &Path,
        budget: &Path,
        rate_card: &Path,
        billing_policy: &Path,
        fx_policy: &Path,
    ) -> Result<Self> {
        let private_handle = open_anchored_directory(private_root)?;
        let inputs_handle = open_directory_at(&private_handle, OsStr::new("inputs"))
            .context("open retention-managed private inputs")?;
        require_exact_directory_entries(
            &inputs_handle,
            [
                "held-out-attestation.json",
                "provider-budget-evidence.json",
                "rate-card.json",
                "billing-policy.json",
                "fx-policy.json",
            ],
        )?;
        let cost_inputs = crate::cost_inputs::RetainedPrivateCostInputs::retain(
            private_root,
            attestation,
            budget,
            rate_card,
            billing_policy,
            fx_policy,
        )?;
        let retained = Self {
            private_root: private_root.to_path_buf(),
            private_handle,
            inputs_handle,
            cost_inputs,
        };
        retained.reverify_unchanged()?;
        Ok(retained)
    }

    pub(crate) fn reverify_unchanged(&self) -> Result<()> {
        self.cost_inputs.reverify_unchanged()?;
        reverify_directory_identity(&self.private_root, &self.private_handle)?;
        reverify_child_directory_identity(
            &self.private_handle,
            OsStr::new("inputs"),
            &self.inputs_handle,
        )
    }
}

struct RetainedInitialCostNamespaces {
    private_root: PathBuf,
    private: File,
    inputs: File,
    supplier_statements: File,
    coordinator: File,
    cost: File,
}

impl RetainedInitialCostNamespaces {
    fn retain(private_root: &Path) -> Result<Self> {
        let private = open_anchored_directory(private_root)?;
        let inputs = open_directory_at(&private, OsStr::new("inputs"))?;
        let supplier_statements = open_directory_at(&inputs, OsStr::new("supplier-statements"))?;
        let coordinator = open_directory_at(&private, OsStr::new("coordinator"))?;
        let cost = open_directory_at(&coordinator, OsStr::new("cost"))?;
        Ok(Self {
            private_root: private_root.to_path_buf(),
            private,
            inputs,
            supplier_statements,
            coordinator,
            cost,
        })
    }

    fn reverify_empty(&self) -> Result<()> {
        reverify_directory_identity(&self.private_root, &self.private)?;
        reverify_child_directory_identity(&self.private, OsStr::new("inputs"), &self.inputs)?;
        reverify_child_directory_identity(
            &self.inputs,
            OsStr::new("supplier-statements"),
            &self.supplier_statements,
        )?;
        reverify_child_directory_identity(
            &self.private,
            OsStr::new("coordinator"),
            &self.coordinator,
        )?;
        reverify_child_directory_identity(&self.coordinator, OsStr::new("cost"), &self.cost)?;
        require_exact_directory_entries(&self.supplier_statements, std::iter::empty::<&str>())?;
        require_exact_directory_entries(&self.cost, std::iter::empty::<&str>())
    }
}

pub(crate) fn import_live_source_proof_into_existing_inputs(
    case_path: &Path,
    material_root: &Path,
    retained_inputs: &RetainedLiveInputDirectory,
) -> Result<ImportedSourceProof> {
    import_live_source_proof_into_existing_inputs_inner(
        case_path,
        material_root,
        retained_inputs,
        |_| Ok(()),
    )
}

#[cfg(test)]
pub(crate) fn import_live_source_proof_into_existing_inputs_with_hook(
    case_path: &Path,
    material_root: &Path,
    retained_inputs: &RetainedLiveInputDirectory,
    hook: impl FnOnce(&Path) -> Result<()>,
) -> Result<ImportedSourceProof> {
    import_live_source_proof_into_existing_inputs_inner(
        case_path,
        material_root,
        retained_inputs,
        hook,
    )
}

fn import_live_source_proof_into_existing_inputs_inner(
    case_path: &Path,
    material_root: &Path,
    retained_inputs: &RetainedLiveInputDirectory,
    hook: impl FnOnce(&Path) -> Result<()>,
) -> Result<ImportedSourceProof> {
    let private_root = &retained_inputs.private_root;
    let private_handle = &retained_inputs.private_handle;
    let inputs_handle = &retained_inputs.inputs_handle;
    let attestation = &retained_inputs.cost_inputs.attestation;
    let case_bytes = read_supplied_regular(case_path).context("read held-out case")?;
    let mission_case: codex_ai_ip_domain::HeldOutMissionCase =
        serde_json::from_slice(&case_bytes).context("parse held-out case")?;
    mission_case.validate().context("validate held-out case")?;
    let private_root = private_root
        .canonicalize()
        .context("canonicalize managed proof private root")?;
    let material_root = material_root
        .canonicalize()
        .context("canonicalize source material root")?;
    let root_metadata = fs::symlink_metadata(&material_root)?;
    if !root_metadata.file_type().is_dir() || root_metadata.file_type().is_symlink() {
        bail!("source material root must be a non-symlink directory");
    }
    let mut relative_paths = HashSet::new();
    let mut validated_materials = Vec::new();
    for material in &mission_case.materials {
        if material.relative_path == "case.json"
            || !relative_paths.insert(material.relative_path.as_str())
        {
            bail!("declared material path is duplicated or reserved");
        }
        let supplied = material_root.join(&material.relative_path);
        let metadata = fs::symlink_metadata(&supplied)
            .with_context(|| format!("stat declared material {}", material.relative_path))?;
        if !metadata.file_type().is_file()
            || metadata.file_type().is_symlink()
            || has_multiple_links(&metadata)
        {
            bail!("declared material is not a single-link regular file");
        }
        let canonical = supplied
            .canonicalize()
            .with_context(|| format!("canonicalize material {}", material.relative_path))?;
        if canonical != supplied || !canonical.starts_with(&material_root) {
            bail!("declared material path traverses a link or escapes its root");
        }
        let bytes = read_handle(&open_anchored_regular(&canonical)?)?;
        if sha256(&bytes) != material.sha256 {
            bail!(
                "declared material digest mismatch for {}",
                material.material_id
            );
        }
        validated_materials.push((material, bytes));
    }
    let materials_manifest_bytes = serde_json::to_vec(&mission_case.materials)?;
    validate_native_attestation_source_binding(
        attestation.typed(),
        &case_bytes,
        &materials_manifest_bytes,
        &private_root,
    )?;
    let case_handle = create_fresh_directory_at(&inputs_handle, OsStr::new("case"))
        .context("create fresh managed case tree")?;
    let inputs = private_root.join("inputs");
    let imported_case_root = inputs.join("case");
    let imported_case = imported_case_root.join("case.json");
    let imported_manifest = inputs.join("materials-manifest.json");
    let mut directory_handles = BTreeMap::<PathBuf, Arc<File>>::new();
    directory_handles.insert(PathBuf::new(), Arc::new(case_handle));
    for (material, _) in &validated_materials {
        let relative = Path::new(&material.relative_path);
        let mut parent = PathBuf::new();
        let components = relative.components().collect::<Vec<_>>();
        for component in &components[..components.len().saturating_sub(1)] {
            let std::path::Component::Normal(name) = component else {
                bail!("declared material path is not normalized");
            };
            let next = parent.join(name);
            if !directory_handles.contains_key(&next) {
                let parent_handle = directory_handles
                    .get(&parent)
                    .context("missing retained material parent directory")?;
                let created =
                    create_fresh_directory_at(parent_handle, name).with_context(|| {
                        format!("create managed material directory {}", next.display())
                    })?;
                directory_handles.insert(next.clone(), Arc::new(created));
            }
            parent = next;
        }
    }
    hook(&imported_case_root)?;
    create_owner_only_file_at(
        directory_handles[&PathBuf::new()].as_ref(),
        OsStr::new("case.json"),
        &case_bytes,
    )?;
    create_owner_only_file_at(
        &inputs_handle,
        OsStr::new("materials-manifest.json"),
        &materials_manifest_bytes,
    )?;
    let mut material_paths = BTreeMap::new();
    let mut material_bytes = BTreeMap::new();
    for (material, bytes) in validated_materials {
        let relative = Path::new(&material.relative_path);
        let parent = relative.parent().unwrap_or_else(|| Path::new(""));
        let leaf = relative
            .file_name()
            .context("declared material path has no file name")?;
        create_owner_only_file_at(
            directory_handles
                .get(parent)
                .context("missing retained material destination directory")?,
            leaf,
            &bytes,
        )?;
        let destination = imported_case_root.join(&material.relative_path);
        material_paths.insert(material.material_id.clone(), destination);
        material_bytes.insert(material.material_id.clone(), bytes);
    }
    let mut directory_paths = directory_handles.keys().collect::<Vec<_>>();
    directory_paths.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for path in directory_paths {
        directory_handles[path]
            .sync_all()
            .context("fsync managed material directory")?;
    }
    inputs_handle
        .sync_all()
        .context("fsync managed input directory")?;
    private_handle
        .sync_all()
        .context("fsync managed private root")?;
    let imported = ImportedSourceProof {
        mission_case,
        case_path: imported_case,
        attestation_path: attestation.path().to_path_buf(),
        materials_manifest_path: imported_manifest,
        material_paths,
        case_bytes,
        attestation_bytes: attestation.raw_bytes().to_vec(),
        attestation: attestation.typed().clone(),
        material_bytes,
    };
    validate_imported_source_proof(&imported)?;
    Ok(imported)
}

fn read_supplied_regular(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("stat supplied regular file {}", path.display()))?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || has_multiple_links(&metadata)
    {
        bail!("supplied path is not a single-link regular file");
    }
    let canonical = path
        .canonicalize()
        .with_context(|| format!("canonicalize supplied file {}", path.display()))?;
    read_handle(&open_anchored_regular(&canonical)?)
}

fn validate_native_attestation_source_binding(
    attestation: &NativeHeldOutAttestation,
    case_bytes: &[u8],
    materials_manifest_bytes: &[u8],
    private_root: &Path,
) -> Result<()> {
    if attestation.case_sha256 != sha256(case_bytes)
        || attestation.source_materials_sha256 != sha256(materials_manifest_bytes)
        || Path::new(&attestation.private_root) != private_root
    {
        bail!("native attestation does not bind the case, materials, and canonical private root");
    }
    let signed_at = chrono::DateTime::parse_from_rfc3339(&attestation.signed_at)?;
    let retention_deadline = chrono::DateTime::parse_from_rfc3339(&attestation.retention_deadline)?;
    let run_start = chrono::Utc::now();
    if signed_at.with_timezone(&chrono::Utc) > run_start
        || run_start >= retention_deadline.with_timezone(&chrono::Utc)
    {
        bail!("native attestation is not signed and retained for this run boundary");
    }
    Ok(())
}

fn provider_role_wire(role: ProviderRole) -> &'static str {
    match role {
        ProviderRole::TargetVolcengine => "targetVolcengine",
        ProviderRole::ApprovedReference => "approvedReference",
    }
}

fn validate_native_attestation_freeze_binding(
    attestation: &NativeHeldOutAttestation,
    args: &LiveFreezeArgs,
    private_root: &Path,
    cost_inputs: &crate::cost_inputs::RetainedPrivateCostInputs,
) -> Result<()> {
    if attestation.candidate_sha != args.fork_sha
        || Path::new(&attestation.private_root) != private_root
        || attestation.provider_role != provider_role_wire(args.provider_role)
        || attestation.approved_total_fen != args.authorized_total_cost_fen
        || attestation.approved_per_run_fen != args.authorized_per_run_cost_fen
        || attestation.max_provider_request_attempts_per_run
            != args.max_provider_request_attempts_per_run
        || attestation.max_total_tokens_per_run != args.max_total_tokens_per_run
        || attestation.max_elapsed_seconds_per_run != args.max_elapsed_seconds_per_run
        || attestation.max_output_tokens_per_request != args.max_output_tokens_per_request
        || attestation.provider_budget_evidence_sha256 != cost_inputs.budget.sha256()
        || attestation.rate_card_sha256 != cost_inputs.rate_card.sha256()
        || attestation.billing_policy_commitment != cost_inputs.billing_policy.sha256()
        || attestation.fx_policy_sha256 != cost_inputs.fx_policy.sha256()
    {
        bail!("native attestation differs from the live freeze inputs");
    }
    Ok(())
}

fn validate_native_attestation_context_binding(
    attestation: &NativeHeldOutAttestation,
    context: &FrozenRunContext,
) -> Result<()> {
    if attestation.candidate_sha != context.candidate_sha
        || Path::new(&attestation.private_root) != context.private_root
        || attestation.provider_mode != context.provider_mode
        || attestation.approved_total_fen != 0
        || attestation.approved_per_run_fen != 0
        || attestation.max_provider_request_attempts_per_run != context.max_attempts_per_arm
        || attestation.max_total_tokens_per_run != context.max_total_tokens
        || attestation.max_elapsed_seconds_per_run != context.max_elapsed_seconds
        || attestation.max_output_tokens_per_request != context.max_output_tokens
        || attestation.provider_budget_evidence_sha256
            != context.artifacts["providerBudgetEvidence"].sha256
        || attestation.rate_card_sha256 != context.artifacts["rateCard"].sha256
        || attestation.billing_policy_commitment != context.artifacts["billingPolicy"].sha256
        || attestation.fx_policy_sha256 != context.artifacts["fxPolicy"].sha256
    {
        bail!("native attestation differs from the frozen live context");
    }
    Ok(())
}

pub(crate) fn validate_imported_source_proof(imported: &ImportedSourceProof) -> Result<()> {
    let private_root = imported
        .case_path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .context("managed case path has no private root")?;
    validate_managed_source_tree(
        private_root,
        &imported.case_bytes,
        &imported.attestation_bytes,
        &serde_json::to_vec(&imported.mission_case.materials)?,
        &imported
            .mission_case
            .materials
            .iter()
            .map(|material| {
                Ok((
                    PathBuf::from(&material.relative_path),
                    imported
                        .material_bytes
                        .get(&material.material_id)
                        .context("missing imported material bytes")?
                        .clone(),
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?,
        false,
    )
    .map(drop)
}

fn validate_managed_source_artifacts(
    frozen: &VerifiedFrozenContext,
) -> Result<VerifiedNativeContentInputs> {
    Ok(VerifiedNativeContentInputs {
        inputs: collect_native_content_inputs(&frozen.context, &frozen.artifacts)?,
        context: frozen.context.clone(),
        artifacts: frozen.artifacts.clone(),
    })
}

fn collect_native_content_inputs(
    context: &FrozenRunContext,
    artifacts: &ArtifactCommitments,
) -> Result<NativeContentInputs> {
    let case_bytes = artifacts.read_verified("source")?;
    let mission_case: codex_ai_ip_domain::HeldOutMissionCase =
        serde_json::from_slice(&case_bytes).context("parse managed held-out case")?;
    mission_case
        .validate()
        .context("validate managed held-out case")?;
    let materials_manifest_bytes = artifacts.read_verified("materials")?;
    if materials_manifest_bytes != serde_json::to_vec(&mission_case.materials)? {
        bail!("managed materials manifest differs from the case declaration");
    }
    let attestation_bytes = artifacts.read_verified("attestation")?;
    let mut material_bytes = BTreeMap::new();
    let mut materials = Vec::with_capacity(mission_case.materials.len());
    for material in &mission_case.materials {
        let bytes = artifacts.read_verified(&format!("material:{}", material.material_id))?;
        material_bytes.insert(PathBuf::from(&material.relative_path), bytes.clone());
        materials.push(NativeMaterialInput {
            material_id: material.material_id.clone(),
            relative_path: material.relative_path.clone(),
            sha256: material.sha256.clone(),
            bytes,
        });
    }
    let attestation = validate_managed_source_tree(
        &context.private_root,
        &case_bytes,
        &attestation_bytes,
        &materials_manifest_bytes,
        &material_bytes,
        true,
    )?;
    validate_native_attestation_context_binding(&attestation, context)?;
    let reviewers: [crate::ReviewerDeclaration; 3] = attestation
        .reviewers
        .clone()
        .try_into()
        .map_err(|reviewers: Vec<_>| {
            anyhow!("expected three reviewers, got {}", reviewers.len())
        })?;
    let artifact_sha = |name: &str| -> Result<String> {
        Ok(artifacts
            .sha256(name)
            .with_context(|| format!("missing frozen artifact {name}"))?
            .to_string())
    };
    Ok(NativeContentInputs {
        materials_manifest: mission_case.materials.clone(),
        mission_case,
        case_bytes,
        reviewers,
        attestation,
        attestation_bytes,
        materials_manifest_bytes,
        materials,
        prompt_bytes: artifacts.read_verified("prompt")?,
        additional_context_bytes: artifacts.read_verified("additionalContext")?,
        schema_bytes: artifacts.read_verified("schema")?,
        thread_start_bytes: artifacts.read_verified("threadStartRequest")?,
        turn_start_bytes: artifacts.read_verified("turnStartRequest")?,
        skill_bytes: artifacts.read_verified("skill")?,
        codex_binary_sha256: artifact_sha("codexBinary")?,
        evaluator_binary_sha256: artifact_sha("evaluatorBinary")?,
        broker_component_sha256: artifact_sha("brokerSource")?,
        model_label: context.model_label.clone(),
        provider_mode: context.provider_mode.clone(),
        provider_label: NATIVE_PROVIDER_LABEL.to_string(),
        provider_compatibility_name: ProofBrokerCompatibilityName::OpenAi,
        max_output_tokens: context.max_output_tokens,
        max_provider_request_attempts: context.max_attempts_per_arm,
        max_total_tokens_per_run: context.max_total_tokens,
        max_elapsed_seconds_per_run: context.max_elapsed_seconds,
    })
}

pub(crate) fn validate_managed_source_before_arm(
    frozen: &VerifiedFrozenContext,
    gate: &PairCoordinator,
    pair_deadline: Instant,
) -> Result<()> {
    let validation = run_sync_before_deadline(pair_deadline, || {
        validate_managed_source_artifacts(frozen).map(drop)
    });
    poison_on_error(
        gate,
        "validate exact managed source tree before arm",
        validation,
    )
}

#[cfg(test)]
pub(crate) fn validate_imported_source_before_arm(
    imported: &ImportedSourceProof,
    gate: &PairCoordinator,
    pair_deadline: Instant,
) -> Result<()> {
    let validation =
        run_sync_before_deadline(pair_deadline, || validate_imported_source_proof(imported));
    poison_on_error(
        gate,
        "validate exact managed source tree before arm",
        validation,
    )
}

fn validate_managed_source_tree(
    private_root: &Path,
    case_bytes: &[u8],
    attestation_bytes: &[u8],
    manifest_bytes: &[u8],
    material_bytes: &BTreeMap<PathBuf, Vec<u8>>,
    supplier_statements_required: bool,
) -> Result<NativeHeldOutAttestation> {
    let attestation = FrozenContracts::load()?
        .validate_native_attestation(attestation_bytes)
        .context("validate strict managed native attestation")?;
    validate_native_attestation_source_binding(
        &attestation,
        case_bytes,
        manifest_bytes,
        private_root,
    )?;
    let private = open_anchored_directory(private_root)?;
    let inputs = open_directory_at(&private, OsStr::new("inputs"))?;
    let mut expected_inputs = BTreeSet::from([
        std::ffi::OsString::from("case"),
        std::ffi::OsString::from("held-out-attestation.json"),
        std::ffi::OsString::from("materials-manifest.json"),
        std::ffi::OsString::from("provider-budget-evidence.json"),
        std::ffi::OsString::from("rate-card.json"),
        std::ffi::OsString::from("billing-policy.json"),
        std::ffi::OsString::from("fx-policy.json"),
    ]);
    if supplier_statements_required {
        expected_inputs.insert(std::ffi::OsString::from("supplier-statements"));
    }
    let actual_inputs = list_directory_entries(&inputs)?;
    if actual_inputs != expected_inputs {
        bail!("managed input directory contains missing or undeclared entries");
    }
    require_exact_file_at(
        &inputs,
        OsStr::new("held-out-attestation.json"),
        attestation_bytes,
    )?;
    require_exact_file_at(
        &inputs,
        OsStr::new("materials-manifest.json"),
        manifest_bytes,
    )?;
    for leaf in [
        "provider-budget-evidence.json",
        "rate-card.json",
        "billing-policy.json",
        "fx-policy.json",
    ] {
        require_owner_only_file(&private_root.join("inputs").join(leaf))?;
    }
    if supplier_statements_required {
        let statements = open_directory_at(&inputs, OsStr::new("supplier-statements"))?;
        let entries = list_directory_entries(&statements)?;
        if entries.iter().any(|entry| {
            entry != OsStr::new("generic.json") && entry != OsStr::new("candidate.json")
        }) {
            bail!("supplier statement directory contains an undeclared entry");
        }
        for entry in entries {
            require_owner_only_file(&private_root.join("inputs/supplier-statements").join(entry))?;
        }
    }
    let case = open_directory_at(&inputs, OsStr::new("case"))?;
    let mut expected = BTreeMap::from([(PathBuf::from("case.json"), case_bytes.to_vec())]);
    expected.extend(material_bytes.clone());
    validate_exact_relative_tree(&case, Path::new(""), &expected)?;
    Ok(attestation)
}

fn validate_exact_relative_tree(
    directory: &File,
    prefix: &Path,
    expected_files: &BTreeMap<PathBuf, Vec<u8>>,
) -> Result<()> {
    let mut expected_entries = BTreeSet::new();
    let mut child_directories = BTreeSet::new();
    for path in expected_files.keys() {
        if !prefix.as_os_str().is_empty() && !path.starts_with(prefix) {
            continue;
        }
        let relative = path
            .strip_prefix(prefix)
            .context("managed source expectation escaped its tree")?;
        let mut components = relative.components();
        let Some(std::path::Component::Normal(first)) = components.next() else {
            bail!("managed source expectation is not normalized");
        };
        expected_entries.insert(first.to_os_string());
        if components.next().is_some() {
            child_directories.insert(first.to_os_string());
        }
    }
    let actual = list_directory_entries(directory)?;
    if actual != expected_entries {
        bail!("managed source tree contains missing or undeclared entries");
    }
    for name in child_directories {
        let child = open_directory_at(directory, &name)?;
        validate_exact_relative_tree(&child, &prefix.join(&name), expected_files)?;
    }
    for (path, bytes) in expected_files {
        if path.parent().unwrap_or_else(|| Path::new("")) == prefix {
            require_exact_file_at(
                directory,
                path.file_name()
                    .context("managed source file has no name")?,
                bytes,
            )?;
        }
    }
    directory
        .sync_all()
        .context("fsync checked managed directory")?;
    Ok(())
}

fn replay_pair_id(
    fork_sha: &str,
    fixtures: &BTreeMap<String, FrozenArtifactReference>,
) -> Result<String> {
    const PAIR_INPUT_NAMES: [&str; 6] = [
        "case",
        "genericRequest",
        "candidateRequest",
        "genericTranscript",
        "candidateTranscript",
        "leadSkill",
    ];
    let commitments = PAIR_INPUT_NAMES
        .iter()
        .map(|name| {
            let reference = fixtures
                .get(*name)
                .with_context(|| format!("missing replay pair input {name}"))?;
            Ok(serde_json::json!({"name": name, "sha256": reference.sha256}))
        })
        .collect::<Result<Vec<_>>>()?;
    let canonical_commitments = canonicalize_value(&serde_json::Value::Array(commitments))?;
    let mut hasher = Sha256::new();
    hasher.update(b"AI-IP-REPLAY-PAIR-V2\0");
    hasher.update(fork_sha.as_bytes());
    hasher.update(canonical_commitments);
    Ok(format!("{:x}", hasher.finalize()))
}

struct VerifiedReplayFixture {
    reference: FrozenArtifactReference,
    bytes: Vec<u8>,
}

/// Writes a typed replay freeze record with no provider-capable fields.
pub fn freeze_replay_context(args: ReplayFreezeArgs) -> Result<()> {
    freeze_replay_context_inner(args, || Ok(()))
}

fn freeze_replay_context_inner(
    args: ReplayFreezeArgs,
    hook: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let repo_root = args
        .repo_root
        .canonicalize()
        .context("canonicalize replay repository root")?;
    let private_root = args
        .private_root
        .canonicalize()
        .context("canonicalize replay private root")?;
    require_owner_only_directory(&private_root)?;
    if args.output.file_name() != Some(OsStr::new("frozen-run-context.json"))
        || args
            .output
            .parent()
            .context("replay frozen context output has no parent")?
            .canonicalize()?
            != private_root
    {
        bail!("replay frozen context must use the canonical private-root filename");
    }
    let fixture_set_path = args
        .fixture_set_manifest
        .canonicalize()
        .context("canonicalize replay fixture-set manifest")?;
    let fixture_set_bytes = read_regular_file_no_follow(&fixture_set_path)?;
    let fixture_set: ReplayFixtureSet =
        serde_json::from_slice(&fixture_set_bytes).context("parse replay fixture-set manifest")?;
    if fixture_set.schema_version != 1 || fixture_set.execution_mode != "replay" {
        bail!("fixture-set manifest is not the pinned replay schema");
    }
    let fixture_root = fixture_set_path
        .parent()
        .context("fixture-set manifest has no parent")?;
    let mut verified_fixtures = BTreeMap::new();
    for entry in fixture_set.fixtures {
        validate_leaf_name(&entry.name)?;
        if entry.path.components().count() != 1
            || !matches!(
                entry.path.components().next(),
                Some(std::path::Component::Normal(_))
            )
            || !is_lower_hex(&entry.sha256, 64)
        {
            bail!("fixture-set contains an invalid path or SHA-256");
        }
        let path = fixture_root
            .join(&entry.path)
            .canonicalize()
            .with_context(|| format!("canonicalize replay fixture {}", entry.name))?;
        if path.parent() != Some(fixture_root) {
            bail!("replay fixture escapes the fixture-set directory");
        }
        let reference = FrozenArtifactReference {
            path,
            sha256: entry.sha256,
        };
        let bytes = read_verified_replay_reference(&reference, &entry.name)?;
        if verified_fixtures
            .insert(entry.name, VerifiedReplayFixture { reference, bytes })
            .is_some()
        {
            bail!("fixture-set contains a duplicate semantic fixture name");
        }
    }
    let required = BTreeSet::from([
        "case",
        "genericRequest",
        "candidateRequest",
        "genericTranscript",
        "candidateTranscript",
        "leadSkill",
        "attestation",
    ]);
    if verified_fixtures
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != required
    {
        bail!("fixture-set does not contain the exact replay fixture roles");
    }
    if args.case.canonicalize()? != verified_fixtures["case"].reference.path
        || args.transcript.canonicalize()? != verified_fixtures["genericTranscript"].reference.path
    {
        bail!("explicit replay case/transcript differ from the fixture-set roles");
    }
    hook()?;
    let fixtures = verified_fixtures
        .iter()
        .map(|(name, fixture)| (name.clone(), fixture.reference.clone()))
        .collect();
    let codex_path = args
        .codex_bin
        .canonicalize()
        .context("canonicalize frozen replay Codex binary")?;
    let evaluator_path = std::env::current_exe()?
        .canonicalize()
        .context("canonicalize replay evaluator binary")?;
    let codex_binary = FrozenArtifactReference {
        sha256: sha256(&read_regular_file_no_follow(&codex_path)?),
        path: codex_path,
    };
    let evaluator_binary = FrozenArtifactReference {
        sha256: sha256(&read_regular_file_no_follow(&evaluator_path)?),
        path: evaluator_path,
    };
    let pair_id = replay_pair_id(&args.fork_sha, &fixtures)?;
    let commitment_key =
        crate::proof_commitment::RetainedProofCommitmentKey::create_fixed(&private_root)?;
    let commitment_key_sha256 = commitment_key.key_sha256().to_string();
    let public_run_id = commitment_key.derive_public_run_id(&pair_id)?;
    let case_bytes = &verified_fixtures["case"].bytes;
    let mission: codex_ai_ip_domain::HeldOutMissionCase =
        serde_json::from_slice(case_bytes).context("parse frozen replay case")?;
    mission.validate().context("validate frozen replay case")?;
    let primary_fixture_paths = verified_fixtures
        .values()
        .map(|fixture| fixture.reference.path.clone())
        .collect::<BTreeSet<_>>();
    retain_replay_materials(
        &mission,
        &verified_fixtures["case"].reference.path,
        &fixture_set_path,
        &primary_fixture_paths,
    )?;
    let attestation = FrozenContracts::load()?
        .validate_replay_attestation(&verified_fixtures["attestation"].bytes)?;
    if attestation.pair_id != pair_id
        || attestation.case_sha256 != sha256(case_bytes)
        || attestation.source_materials_sha256 != sha256(&serde_json::to_vec(&mission.materials)?)
    {
        bail!("replay attestation does not bind the frozen pair and declared case materials");
    }
    let record = ReplayFrozenContext {
        schema_version: 1,
        execution_mode: "replay".to_string(),
        provider_mode: "not-run".to_string(),
        pair_id,
        public_run_id,
        commitment_key_sha256,
        repo_root,
        fork_sha: args.fork_sha,
        private_root,
        codex_binary,
        evaluator_binary,
        fixture_set_manifest: FrozenArtifactReference {
            path: fixture_set_path,
            sha256: sha256(&fixture_set_bytes),
        },
        fixtures,
    };
    let record_bytes = serde_json::to_vec_pretty(&record)?;
    FrozenContracts::load()?.validate_replay_context(&record_bytes)?;
    commitment_key.reverify_binding(
        &record.commitment_key_sha256,
        &record.pair_id,
        &record.public_run_id,
    )?;
    write_owner_only_new(&args.output, &record_bytes)
}

#[cfg(test)]
pub(crate) fn freeze_replay_context_with_hook(
    args: ReplayFreezeArgs,
    hook: impl FnOnce() -> Result<()>,
) -> Result<()> {
    freeze_replay_context_inner(args, hook)
}

struct ReplayArmResult {
    condition: EvaluationCondition,
    catalog: CatalogSnapshot,
    catalog_response: codex_app_server_protocol::SkillsListResponse,
    catalog_roots: crate::proof_archive::CatalogRootsSidecar,
    collected: CollectedReplay,
    successful_read_observed: bool,
    skill_use_evidence_sha256: Option<String>,
    transcript_sha256: String,
    notifications: Vec<u8>,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PairOutputCommitments {
    generic_content_package_sha256: String,
    candidate_content_package_sha256: String,
}

const REPLAY_GENERIC_REQUEST_THREAD_ID: &str = "0198f5aa-0000-7000-8000-000000000002";
const REPLAY_CANDIDATE_REQUEST_THREAD_ID: &str = "0198f5aa-0000-7000-8000-000000000003";
const REPLAY_REQUEST_DEADLINE_RFC3339: &str = "2026-08-27T12:01:00Z";
const FROZEN_CONTEXT_CAP: u64 = 1024 * 1024;

/// Executes a fully frozen replay pair without constructing any provider,
/// broker, credential, App Server, network, budget, or paid-execution surface.
pub fn run_replay_pair(args: ReplayPairArgs) -> Result<()> {
    let verified = load_verified_replay_frozen_context(&args.frozen_run_context)?;
    let context = verified.context();
    let projection = verified.projection();
    let context_bytes = projection.raw_bytes;
    let mission = projection.mission_case;
    let skill_bytes = verified.fixture_bytes("leadSkill")?;

    let (homes, candidate_skill_path, replay_config) =
        prepare_replay_homes(&context.private_root, skill_bytes)?;
    let generic_request =
        canonicalize_replay_request(&verified, EvaluationCondition::Generic, &homes)?;
    let candidate_request =
        canonicalize_replay_request(&verified, EvaluationCondition::Candidate, &homes)?;
    let request_verification =
        build_pair_request_verification(&generic_request, &candidate_request)?;
    let coordinator = context.private_root.join("replay-coordinator");
    create_owner_only_dir(&coordinator)?;
    let case_dir = context.fixtures["case"]
        .path
        .parent()
        .context("replay case has no fixture directory")?
        .to_path_buf();
    let generic_roots = crate::proof_archive::CatalogRootsSidecar {
        codex_home: homes.generic_codex_home.clone(),
        host_home: homes.generic_home.clone(),
        case_dir: case_dir.clone(),
    };
    let candidate_roots = crate::proof_archive::CatalogRootsSidecar {
        codex_home: homes.candidate_codex_home.clone(),
        host_home: homes.candidate_home.clone(),
        case_dir,
    };
    let (generic_catalog_response, generic_catalog) = replay_catalog(
        &CatalogRoots {
            codex_home: generic_roots.codex_home.clone(),
            host_home: generic_roots.host_home.clone(),
            case_dir: generic_roots.case_dir.clone(),
        },
        None,
    )?;
    let (candidate_catalog_response, candidate_catalog) = replay_catalog(
        &CatalogRoots {
            codex_home: candidate_roots.codex_home.clone(),
            host_home: candidate_roots.host_home.clone(),
            case_dir: candidate_roots.case_dir.clone(),
        },
        Some(&candidate_skill_path),
    )?;
    let catalog_parity = crate::compare_catalogs(
        &generic_catalog,
        &candidate_catalog,
        &candidate_skill_path,
        skill_bytes,
    )?;
    crate::validate_stable_catalog(&generic_catalog, &generic_catalog)?;
    crate::validate_stable_catalog(&candidate_catalog, &candidate_catalog)?;
    let generic = collect_replay_arm(
        EvaluationCondition::Generic,
        verified.fixture_bytes("genericTranscript")?,
        mission,
        &homes.generic_codex_home,
        skill_bytes,
        generic_catalog_response,
        generic_roots,
        generic_catalog,
    )?;
    let candidate = collect_replay_arm(
        EvaluationCondition::Candidate,
        verified.fixture_bytes("candidateTranscript")?,
        mission,
        &homes.candidate_codex_home,
        skill_bytes,
        candidate_catalog_response,
        candidate_roots,
        candidate_catalog,
    )?;
    if generic.catalog.normalized_base_catalog_sha256()
        != catalog_parity.normalized_base_catalog_sha256
        || candidate.catalog.normalized_base_catalog_sha256()
            != catalog_parity.normalized_base_catalog_sha256
        || generic.successful_read_observed
        || generic.skill_use_evidence_sha256.is_some()
        || !candidate.successful_read_observed
        || candidate.skill_use_evidence_sha256.is_none()
    {
        bail!("typed replay pair differs outside the canonical Lead Skill treatment");
    }
    let execution_context = to_exact_pretty_json(serde_json::json!({
        "schemaVersion": 1,
        "executionMode": "replay",
        "providerMode": "not-run",
        "paidProviderCostFen": 0,
        "pairId": context.pair_id,
        "frozenRunContextSha256": sha256(context_bytes),
        "fixtureSetSha256": context.fixture_set_manifest.sha256,
        "arms": ["generic", "candidate"]
    }))?;
    write_owner_only_new(
        &coordinator.join("execution-context.json"),
        &execution_context,
    )?;
    let execution_context_sha256 = sha256(&execution_context);
    let mut generic_manifest_sha256 = None;
    let mut candidate_manifest_sha256 = None;
    for (ordinal, arm, request) in [
        (1_u8, &generic, &generic_request),
        (2_u8, &candidate, &candidate_request),
    ] {
        let codex_home = match arm.condition {
            EvaluationCondition::Generic => &homes.generic_codex_home,
            EvaluationCondition::Candidate => &homes.candidate_codex_home,
        };
        let (config_sidecar, config_evidence) = replay_config_sidecar(codex_home, &replay_config)?;
        let capture = crate::proof_archive::ArmPostprocessCapture {
            pair_id: context.pair_id.clone(),
            run_ordinal: ordinal,
            condition: arm.condition,
            evidence_source: crate::ArchiveEvidenceSource::ReplaySynthetic,
            notifications: arm.notifications.clone(),
            start: replay_start_sidecar(&arm.catalog_roots.case_dir)?,
            config: config_sidecar,
            pre_catalog: crate::proof_archive::CatalogSidecar {
                schema_version: 1,
                roots: arm.catalog_roots.clone(),
                response: arm.catalog_response.clone(),
            },
            post_catalog: crate::proof_archive::CatalogSidecar {
                schema_version: 1,
                roots: arm.catalog_roots.clone(),
                response: arm.catalog_response.clone(),
            },
            quiet_tree: replay_quiet_tree(&arm.catalog_roots.case_dir)?,
            broker_snapshot: replay_broker_snapshot(&context.pair_id, ordinal, arm)?,
        };
        let postprocess_sha256 = crate::proof_archive_seal::seal_postprocess_archive(
            &context.private_root,
            Path::new("replay-coordinator"),
            capture,
        )?;
        let package_sha256 = sha256(&serde_json::to_vec(&arm.collected.content_package)?);
        let manifest = build_replay_manifest(
            context,
            context_bytes,
            mission,
            &execution_context_sha256,
            ordinal,
            arm,
            &config_evidence,
            &sha256(&replay_config.bytes),
            &postprocess_sha256,
            &catalog_parity,
            &package_sha256,
            request,
        )?;
        crate::verify_postprocess_archive(&context.private_root, &manifest, mission, skill_bytes)?;
        let bytes = serde_json::to_vec_pretty(&manifest)?;
        write_owner_only_new(
            &coordinator.join(format!("run-{ordinal}-manifest.json")),
            &bytes,
        )?;
        match arm.condition {
            EvaluationCondition::Generic => generic_manifest_sha256 = Some(sha256(&bytes)),
            EvaluationCondition::Candidate => candidate_manifest_sha256 = Some(sha256(&bytes)),
        }
    }
    crate::private_inventory::bootstrap_private_inventory(
        &context.private_root,
        &context.pair_id,
        &sha256(context_bytes),
        &chrono::Utc::now().to_rfc3339(),
    )?;
    let mut verification = serde_json::json!({
        "schemaVersion": 1,
        "executionMode": "replay",
        "providerMode": "not-run",
        "paidProviderCostFen": 0,
        "pairId": context.pair_id,
        "frozenRunContextSha256": sha256(context_bytes),
        "fixtureSetSha256": context.fixture_set_manifest.sha256,
        "genericRunManifestSha256": generic_manifest_sha256.context("missing generic replay manifest")?,
        "candidateRunManifestSha256": candidate_manifest_sha256.context("missing candidate replay manifest")?,
        "normalizedBaseCatalogSha256": catalog_parity.normalized_base_catalog_sha256,
        "candidateSkillSha256": catalog_parity.candidate_skill_sha256,
        "requestParity": request_verification
    });
    append_pair_output_commitments(
        &mut verification,
        &sha256(&serde_json::to_vec(&generic.collected.content_package)?),
        &sha256(&serde_json::to_vec(&candidate.collected.content_package)?),
    )?;
    let verification = to_exact_pretty_json(verification)?;
    write_owner_only_new(
        &coordinator.join("replay-pair-verification.json"),
        &verification,
    )?;
    crate::private_inventory::append_private_inventory(
        &context.private_root,
        Path::new("replay-coordinator/replay-pair-verification.json"),
    )?;
    crate::private_inventory::verify_private_inventory(&context.private_root).map(drop)
}

pub(crate) fn verify_replay_frozen_context(
    canonical_path: &Path,
    raw_context_bytes: &[u8],
) -> Result<VerifiedReplayFrozenContext> {
    if !canonical_path.is_absolute() || canonical_path.canonicalize()? != canonical_path {
        bail!("Replay frozen context path is not canonical");
    }
    let (frozen_file, retained_bytes) = ArtifactCommitment::freeze_context(canonical_path)?;
    if retained_bytes != raw_context_bytes {
        bail!("Replay frozen context differs from its retained file handle");
    }
    verify_retained_replay_frozen_context(canonical_path, raw_context_bytes, frozen_file)
}

fn verify_retained_replay_frozen_context(
    canonical_path: &Path,
    raw_context_bytes: &[u8],
    frozen_file: ArtifactCommitment,
) -> Result<VerifiedReplayFrozenContext> {
    let contracts = FrozenContracts::load()?;
    contracts.validate_replay_context(raw_context_bytes)?;
    let context: ReplayFrozenContext =
        serde_json::from_value(crate::jcs::parse_json(raw_context_bytes)?)?;
    if serde_json::to_vec_pretty(&context)? != raw_context_bytes
        || context.schema_version != 1
        || context.execution_mode != "replay"
        || context.provider_mode != "not-run"
        || !is_lower_hex(&context.pair_id, 64)
        || !is_lower_hex(&context.public_run_id, 64)
        || !is_lower_hex(&context.commitment_key_sha256, 64)
        || canonical_path != context.private_root.join("frozen-run-context.json")
    {
        bail!("frozen replay context violates its exact typed contract");
    }
    require_owner_only_directory(&context.private_root)?;
    let commitment_key =
        crate::proof_commitment::RetainedProofCommitmentKey::read_fixed(&context.private_root)?;
    commitment_key.reverify_binding(
        &context.commitment_key_sha256,
        &context.pair_id,
        &context.public_run_id,
    )?;
    let binaries = ReplayBinaryCommitments::retain(
        &context.codex_binary,
        &context.evaluator_binary,
    )?;
    if std::env::current_exe()?.canonicalize()? != context.evaluator_binary.path {
        bail!("replay evaluator executable differs from the frozen binary");
    }
    let fixture_set =
        retain_replay_reference(&context.fixture_set_manifest, "fixture-set manifest")?;
    let fixture_set_bytes = fixture_set.read_verified()?;
    let fixture_set_typed: ReplayFixtureSet =
        serde_json::from_value(crate::jcs::parse_json(&fixture_set_bytes)?)?;
    if fixture_set_typed.schema_version != 1 || fixture_set_typed.execution_mode != "replay" {
        bail!("frozen fixture-set mode changed before replay");
    }
    let fixture_root = context
        .fixture_set_manifest
        .path
        .parent()
        .context("fixture-set manifest has no parent")?;
    let mut listed = BTreeMap::new();
    for entry in fixture_set_typed.fixtures {
        if listed
            .insert(entry.name, (entry.path, entry.sha256))
            .is_some()
        {
            bail!("frozen fixture-set contains a duplicate role");
        }
    }
    let required = BTreeSet::from([
        "case",
        "genericRequest",
        "candidateRequest",
        "genericTranscript",
        "candidateTranscript",
        "leadSkill",
        "attestation",
    ]);
    if listed.keys().map(String::as_str).collect::<BTreeSet<_>>() != required
        || context
            .fixtures
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            != required
    {
        bail!("frozen fixture-set roles changed before replay");
    }
    let mut fixtures = BTreeMap::new();
    for (name, reference) in &context.fixtures {
        let (relative, listed_sha256) = &listed[name];
        if relative.components().count() != 1
            || fixture_root.join(relative).canonicalize()? != reference.path
            || listed_sha256 != &reference.sha256
        {
            bail!("fixture-set reference changed for {name}");
        }
        let commitment = retain_replay_reference(reference, name)?;
        let bytes = commitment.read_verified()?;
        fixtures.insert(name.clone(), VerifiedReplayReference { commitment, bytes });
    }
    if replay_pair_id(&context.fork_sha, &context.fixtures)? != context.pair_id {
        bail!("frozen replay pair ID differs from its six execution inputs");
    }
    let case_bytes = &fixtures["case"].bytes;
    let mission_case: codex_ai_ip_domain::HeldOutMissionCase = serde_json::from_slice(case_bytes)?;
    mission_case
        .validate()
        .context("validate frozen replay case")?;
    let primary_fixture_paths = context
        .fixtures
        .values()
        .map(|reference| reference.path.clone())
        .collect::<BTreeSet<_>>();
    let materials = retain_replay_materials(
        &mission_case,
        &context.fixtures["case"].path,
        &context.fixture_set_manifest.path,
        &primary_fixture_paths,
    )?;
    let attestation = contracts.validate_replay_attestation(&fixtures["attestation"].bytes)?;
    let materials_manifest_bytes = serde_json::to_vec(&mission_case.materials)?;
    if attestation.pair_id != context.pair_id
        || attestation.case_sha256 != sha256(case_bytes)
        || attestation.source_materials_sha256 != sha256(&materials_manifest_bytes)
    {
        bail!("replay attestation does not bind the frozen pair and declared case materials");
    }
    if !std::str::from_utf8(&fixtures["leadSkill"].bytes)?
        .contains(&format!("name: {}", codex_ai_ip_runtime::LEAD_SKILL_NAME))
    {
        bail!("replay Lead Skill fixture has the wrong canonical name");
    }
    let case_dir = context.fixtures["case"]
        .path
        .parent()
        .context("replay case has no fixture directory")?
        .to_path_buf();
    let prompt_bytes = codex_ai_ip_runtime::root_prompt().as_bytes().to_vec();
    let additional_context_bytes =
        codex_ai_ip_runtime::evaluation_context(&mission_case)?.into_bytes();
    let schema_bytes = serde_json::to_vec(&codex_ai_ip_runtime::content_package_schema()?)?;
    let thread_start_bytes = serde_json::to_vec(&build_thread_start(
        "replay-fixture",
        "replay-not-run",
        &case_dir,
    )?)?;
    let turn_start_bytes = serde_json::to_vec(&build_turn_start("root-thread", &mission_case)?)?;
    Ok(VerifiedReplayFrozenContext {
        canonical_path: canonical_path.to_path_buf(),
        raw_bytes: raw_context_bytes.to_vec(),
        raw_sha256: sha256(raw_context_bytes),
        frozen_file,
        context,
        commitment_key,
        fixture_set,
        fixtures,
        materials,
        binaries,
        prompt_bytes,
        additional_context_bytes,
        schema_bytes,
        thread_start_bytes,
        turn_start_bytes,
        mission_case,
        attestation,
        materials_manifest_bytes,
    })
}

fn load_verified_replay_frozen_context(path: &Path) -> Result<VerifiedReplayFrozenContext> {
    let canonical_path = path
        .canonicalize()
        .context("canonicalize replay frozen context")?;
    let (frozen_file, bytes) = ArtifactCommitment::freeze_context(&canonical_path)?;
    verify_retained_replay_frozen_context(&canonical_path, &bytes, frozen_file)
}

fn retain_replay_reference(
    reference: &FrozenArtifactReference,
    label: &str,
) -> Result<ArtifactCommitment> {
    let canonical = canonical_replay_reference(reference, label)?;
    retain_canonical_replay_reference(&canonical, reference, label)
}

fn canonical_replay_reference(
    reference: &FrozenArtifactReference,
    label: &str,
) -> Result<PathBuf> {
    if !reference.path.is_absolute() || !is_lower_hex(&reference.sha256, 64) {
        bail!("frozen replay {label} reference is not canonical");
    }
    let canonical = reference
        .path
        .canonicalize()
        .with_context(|| format!("canonicalize frozen replay {label}"))?;
    if canonical != reference.path {
        bail!("frozen replay {label} bytes or identity changed");
    }
    Ok(canonical)
}

fn retain_canonical_replay_reference(
    canonical: &Path,
    reference: &FrozenArtifactReference,
    label: &str,
) -> Result<ArtifactCommitment> {
    let commitment = ArtifactCommitment::freeze(&canonical)?;
    if commitment.sha256 != reference.sha256 {
        bail!("frozen replay {label} bytes or identity changed");
    }
    Ok(commitment)
}

fn retain_replay_materials(
    mission: &codex_ai_ip_domain::HeldOutMissionCase,
    case_path: &Path,
    fixture_set_path: &Path,
    primary_fixture_paths: &BTreeSet<PathBuf>,
) -> Result<Vec<VerifiedReplayMaterial>> {
    let material_root = case_path
        .parent()
        .context("frozen Replay case has no material root")?;
    if material_root.canonicalize()? != material_root {
        bail!("frozen Replay material root is not canonical");
    }
    let mut reserved = primary_fixture_paths.clone();
    reserved.insert(fixture_set_path.to_path_buf());
    let mut relative_paths = BTreeSet::new();
    let mut retained = Vec::with_capacity(mission.materials.len());
    for material in &mission.materials {
        let relative = Path::new(&material.relative_path);
        if !relative
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
        {
            bail!("Replay material path is not a normalized relative path");
        }
        if !relative_paths.insert(material.relative_path.as_str()) {
            bail!("Replay case declares a duplicate material path");
        }
        let supplied = material_root.join(relative);
        if reserved.contains(&supplied) {
            bail!("Replay material collides with a frozen Replay fixture");
        }
        let canonical = supplied
            .canonicalize()
            .with_context(|| format!("canonicalize Replay material {}", material.material_id))?;
        if canonical != supplied || !canonical.starts_with(material_root) {
            bail!("Replay material path traverses a link or escapes its root");
        }
        if reserved.contains(&canonical) {
            bail!("Replay material collides with a frozen Replay fixture");
        }
        let commitment = ArtifactCommitment::freeze(&canonical)
            .with_context(|| format!("retain Replay material {}", material.material_id))?;
        let bytes = commitment
            .read_verified()
            .with_context(|| format!("read retained Replay material {}", material.material_id))?;
        if commitment.sha256 != material.sha256 {
            bail!(
                "Replay material digest mismatch for {}",
                material.material_id
            );
        }
        retained.push(VerifiedReplayMaterial {
            input: ReplayMaterialInput {
                material_id: material.material_id.clone(),
                relative_path: material.relative_path.clone(),
                sha256: material.sha256.clone(),
                bytes,
            },
            commitment,
        });
    }
    Ok(retained)
}

fn read_verified_replay_reference(
    reference: &FrozenArtifactReference,
    label: &str,
) -> Result<Vec<u8>> {
    retain_replay_reference(reference, label)?.read_verified()
}

#[cfg(test)]
static ARTIFACT_READ_OBSERVER: Mutex<Option<Arc<Mutex<Vec<PathBuf>>>>> = Mutex::new(None);

#[cfg(test)]
static ARTIFACT_READ_OBSERVER_GATE: Mutex<()> = Mutex::new(());

#[cfg(test)]
fn observe_artifact_read(path: &Path) {
    let observer = ARTIFACT_READ_OBSERVER.lock().unwrap().clone();
    if let Some(observer) = observer {
        observer.lock().unwrap().push(path.to_path_buf());
    }
}

#[cfg(test)]
pub(crate) fn observe_artifact_reads<T>(
    operation: impl FnOnce() -> Result<T>,
) -> (Result<T>, Vec<PathBuf>) {
    let _gate = ARTIFACT_READ_OBSERVER_GATE.lock().unwrap();
    let observed = Arc::new(Mutex::new(Vec::new()));
    *ARTIFACT_READ_OBSERVER.lock().unwrap() = Some(Arc::clone(&observed));
    let result = operation();
    *ARTIFACT_READ_OBSERVER.lock().unwrap() = None;
    let reads = std::mem::take(&mut *observed.lock().unwrap());
    (result, reads)
}

#[cfg(test)]
pub(crate) fn retain_and_reverify_replay_binaries_with_hook(
    codex_path: &Path,
    codex_sha256: &str,
    evaluator_path: &Path,
    evaluator_sha256: &str,
    hook: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let binaries = ReplayBinaryCommitments::retain(
        &FrozenArtifactReference {
            path: codex_path.to_path_buf(),
            sha256: codex_sha256.to_string(),
        },
        &FrozenArtifactReference {
            path: evaluator_path.to_path_buf(),
            sha256: evaluator_sha256.to_string(),
        },
    )?;
    hook()?;
    binaries.reverify_all()
}

#[cfg(test)]
pub(crate) fn read_replay_reference_with_hook(
    path: &Path,
    expected_sha256: &str,
    hook: impl FnOnce(&Path) -> Result<()>,
) -> Result<Vec<u8>> {
    let reference = FrozenArtifactReference {
        path: path.to_path_buf(),
        sha256: expected_sha256.to_string(),
    };
    let bytes = read_verified_replay_reference(&reference, "test fixture")?;
    hook(path)?;
    Ok(bytes)
}

pub(crate) fn canonicalize_replay_request(
    verified: &VerifiedReplayFrozenContext,
    condition: EvaluationCondition,
    homes: &IsolatedHomes,
) -> Result<codex_responses_api_proxy::TransformedRequestEvidence> {
    let (fixture_name, token, codex_home, committed_thread_id) = match condition {
        EvaluationCondition::Generic => (
            "genericRequest",
            "$GENERIC_CODEX_HOME",
            &homes.generic_codex_home,
            REPLAY_GENERIC_REQUEST_THREAD_ID,
        ),
        EvaluationCondition::Candidate => (
            "candidateRequest",
            "$CANDIDATE_CODEX_HOME",
            &homes.candidate_codex_home,
            REPLAY_CANDIDATE_REQUEST_THREAD_ID,
        ),
    };
    let mut body: serde_json::Value = serde_json::from_slice(verified.fixture_bytes(fixture_name)?)
        .with_context(|| format!("parse frozen replay request fixture {fixture_name}"))?;
    materialize_replay_request_home(&mut body, token, codex_home)?;
    canonicalize_first_root_request(
        &body,
        &RequestCanonicalizationContext {
            condition,
            known_thread_ids: &HashSet::from([committed_thread_id.to_string()]),
            generic_codex_home: &homes.generic_codex_home,
            candidate_codex_home: &homes.candidate_codex_home,
            deadline_rfc3339: REPLAY_REQUEST_DEADLINE_RFC3339,
        },
    )
}

fn materialize_replay_request_home(
    value: &mut serde_json::Value,
    committed_token: &str,
    codex_home: &Path,
) -> Result<()> {
    match value {
        serde_json::Value::Object(object) => {
            for value in object.values_mut() {
                materialize_replay_request_home(value, committed_token, codex_home)?;
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                materialize_replay_request_home(value, committed_token, codex_home)?;
            }
        }
        serde_json::Value::String(value)
            if value.contains("$GENERIC_CODEX_HOME") || value.contains("$CANDIDATE_CODEX_HOME") =>
        {
            let suffix = value
                .strip_prefix(committed_token)
                .context("replay request contains an uncommitted Home token")?;
            let materialized = if suffix.is_empty() {
                codex_home.to_path_buf()
            } else {
                let relative = suffix
                    .strip_prefix('/')
                    .context("replay request Home token is not in a normalized path position")?;
                if relative.is_empty()
                    || Path::new(relative)
                        .components()
                        .any(|component| !matches!(component, std::path::Component::Normal(_)))
                {
                    bail!("replay request Home token path is not normalized");
                }
                codex_home.join(relative)
            };
            *value = materialized
                .to_str()
                .context("replay request CODEX_HOME is not UTF-8")?
                .to_string();
        }
        _ => {}
    }
    Ok(())
}

fn prepare_replay_homes(
    private_root: &Path,
    skill_bytes: &[u8],
) -> Result<(IsolatedHomes, PathBuf, crate::FrozenSharedConfig)> {
    let generic_home = private_root.join("replay-generic-home");
    let generic_codex_home = generic_home.join(".codex");
    let candidate_home = private_root.join("replay-candidate-home");
    let candidate_codex_home = candidate_home.join(".codex");
    for path in [
        &generic_home,
        &generic_codex_home,
        &generic_codex_home.join("skills"),
        &candidate_home,
        &candidate_codex_home,
        &candidate_codex_home.join("skills"),
    ] {
        create_owner_only_dir(path)?;
    }
    let candidate_skill_dir = candidate_codex_home
        .join("skills")
        .join(codex_ai_ip_runtime::LEAD_SKILL_NAME);
    create_owner_only_dir(&candidate_skill_dir)?;
    let candidate_skill_path = candidate_skill_dir.join("SKILL.md");
    write_owner_only_new(&candidate_skill_path, skill_bytes)?;
    let shared_config = build_shared_config("replay-fixture", 1)?;
    for path in [
        generic_codex_home.join("config.toml"),
        candidate_codex_home.join("config.toml"),
    ] {
        crate::secure_fs::write_owner_only_new(&path, &shared_config.bytes)?;
    }
    Ok((
        IsolatedHomes {
            generic_home,
            generic_codex_home,
            candidate_home,
            candidate_codex_home,
        },
        candidate_skill_path,
        shared_config,
    ))
}

fn replay_catalog(
    roots: &CatalogRoots,
    target: Option<&Path>,
) -> Result<(
    codex_app_server_protocol::SkillsListResponse,
    CatalogSnapshot,
)> {
    let skills = target
        .map(|path| {
            vec![serde_json::json!({
                "name": codex_ai_ip_runtime::LEAD_SKILL_NAME,
                "description": "Synthetic replay fixture for the canonical AI IP delivery Skill.",
                "shortDescription": "Synthetic replay Lead Skill",
                "interface": null,
                "dependencies": {"tools": []},
                "path": path,
                "scope": "user",
                "enabled": true
            })]
        })
        .unwrap_or_default();
    let response: codex_app_server_protocol::SkillsListResponse =
        serde_json::from_value(serde_json::json!({
            "data": [{"cwd": roots.case_dir, "skills": skills, "errors": []}]
        }))?;
    let snapshot = crate::normalize_catalog(&response, roots)?;
    Ok((response, snapshot))
}

fn collect_replay_arm(
    condition: EvaluationCondition,
    transcript_bytes: &[u8],
    mission: &codex_ai_ip_domain::HeldOutMissionCase,
    codex_home: &Path,
    skill_bytes: &[u8],
    catalog_response: codex_app_server_protocol::SkillsListResponse,
    catalog_roots: crate::proof_archive::CatalogRootsSidecar,
    catalog: CatalogSnapshot,
) -> Result<ReplayArmResult> {
    let mut collector = ReplayCollector::new(
        "root-thread",
        "root-turn",
        HashSet::from(["root-thread".to_string()]),
    );
    let target_skill_path = codex_home
        .join("skills")
        .join(codex_ai_ip_runtime::LEAD_SKILL_NAME)
        .join("SKILL.md");
    let mut skill_use = match condition {
        EvaluationCondition::Generic => {
            SkillUseTracker::new_forbidden(&target_skill_path, skill_bytes)?
        }
        EvaluationCondition::Candidate => {
            SkillUseTracker::new_required(&target_skill_path, skill_bytes)?
        }
    };
    let text = std::str::from_utf8(transcript_bytes).context("replay transcript is not UTF-8")?;
    if text.is_empty() || !text.ends_with('\n') {
        bail!("replay transcript must be non-empty LF-terminated JSONL");
    }
    let mut recorder = crate::evidence::NotificationRecorder::default();
    let turn_started: codex_app_server_protocol::ServerNotification =
        serde_json::from_value(serde_json::json!({
            "method": "turn/started",
            "params": {
                "threadId": "root-thread",
                "turn": replay_turn("root-turn", "inProgress")
            }
        }))?;
    recorder.record(&turn_started)?;
    collector.ingest(turn_started)?;
    for line in text.lines() {
        let mut value: serde_json::Value = serde_json::from_str(line)?;
        materialize_replay_codex_home(&mut value, codex_home)?;
        let notification: codex_app_server_protocol::ServerNotification =
            serde_json::from_value(value)?;
        recorder.record(&notification)?;
        skill_use.ingest(&notification)?;
        collector.ingest(notification)?;
    }
    let collected = collector.finish(mission)?;
    let skill_use = skill_use.finish()?;
    let notifications = recorder.finish();
    Ok(ReplayArmResult {
        condition,
        catalog,
        catalog_response,
        catalog_roots,
        collected,
        successful_read_observed: skill_use.successful_read_observed,
        skill_use_evidence_sha256: skill_use.evidence_sha256,
        transcript_sha256: sha256(&notifications),
        notifications,
    })
}

fn materialize_replay_codex_home(value: &mut serde_json::Value, codex_home: &Path) -> Result<()> {
    match value {
        serde_json::Value::Object(object) => {
            for value in object.values_mut() {
                materialize_replay_codex_home(value, codex_home)?;
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                materialize_replay_codex_home(value, codex_home)?;
            }
        }
        serde_json::Value::String(value) if value.contains("$CODEX_HOME") => {
            let codex_home = codex_home
                .to_str()
                .context("replay CODEX_HOME is not UTF-8")?;
            *value = value.replace("$CODEX_HOME", codex_home);
        }
        _ => {}
    }
    Ok(())
}

fn replay_turn(id: &str, status: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "items": [],
        "itemsView": "full",
        "status": status,
        "error": null,
        "startedAt": null,
        "completedAt": null,
        "durationMs": null
    })
}

fn replay_root_thread(case_dir: &Path) -> Result<codex_app_server_protocol::Thread> {
    Ok(serde_json::from_value(serde_json::json!({
        "id": "root-thread",
        "extra": null,
        "sessionId": "replay-fixture-session",
        "forkedFromId": null,
        "parentThreadId": null,
        "preview": "synthetic replay",
        "ephemeral": false,
        "section": null,
        "sectionEnteredAt": null,
        "projectId": null,
        "historyMode": "legacy",
        "modelProvider": "replay-not-run",
        "createdAt": 1,
        "updatedAt": 1,
        "recencyAt": null,
        "status": {"type": "idle"},
        "path": null,
        "cwd": case_dir,
        "cliVersion": "synthetic",
        "source": "appServer",
        "canAcceptDirectInput": true,
        "threadSource": "user",
        "agentNickname": null,
        "agentRole": null,
        "gitInfo": null,
        "name": null,
        "turns": []
    }))?)
}

fn replay_start_sidecar(case_dir: &Path) -> Result<crate::proof_archive::StartSidecar> {
    let root = replay_root_thread(case_dir)?;
    Ok(crate::proof_archive::StartSidecar {
        schema_version: 1,
        thread: serde_json::from_value(serde_json::json!({
            "thread": root,
            "model": "replay-fixture",
            "modelProvider": "replay-not-run",
            "serviceTier": null,
            "cwd": case_dir,
            "runtimeWorkspaceRoots": [],
            "instructionSources": [],
            "approvalPolicy": "never",
            "approvalsReviewer": "user",
            "sandbox": {"type": "dangerFullAccess"},
            "activePermissionProfile": {"id": "ai-ip-eval", "extends": null},
            "reasoningEffort": null,
            "multiAgentMode": "explicitRequestOnly"
        }))?,
        turn: serde_json::from_value(serde_json::json!({
            "turn": replay_turn("root-turn", "inProgress")
        }))?,
    })
}

fn replay_config_sidecar(
    codex_home: &Path,
    shared: &crate::FrozenSharedConfig,
) -> Result<(crate::proof_archive::ConfigSidecar, ConfigAuditEvidence)> {
    let config_path = codex_home.join("config.toml").canonicalize()?;
    let mut effective = shared.layer_json.clone();
    effective["allow_login_shell"] = serde_json::Value::Bool(true);
    let response = serde_json::from_value(serde_json::json!({
        "config": effective,
        "origins": {},
        "layers": [{
            "name": {"type": "user", "file": config_path, "profile": null},
            "version": "v1",
            "config": shared.layer_json
        }]
    }))?;
    let requirements =
        codex_app_server_protocol::ConfigRequirementsReadResponse { requirements: None };
    let evidence = audit_frozen_config(
        &response,
        &requirements,
        config_path.clone(),
        shared.bytes.clone(),
        shared.layer_json.clone(),
    )?;
    Ok((
        crate::proof_archive::ConfigSidecar {
            schema_version: 1,
            response,
            requirements,
            canonical_config_path: config_path,
            expected_config_utf8: String::from_utf8(shared.bytes.clone())?,
            expected_layer_config: shared.layer_json.clone(),
        },
        evidence,
    ))
}

fn replay_quiet_tree(case_dir: &Path) -> Result<crate::proof_archive::QuietTreeSidecar> {
    let root = replay_root_thread(case_dir)?;
    let scan = |root: &codex_app_server_protocol::Thread| crate::proof_archive::TreeScanSidecar {
        ancestor_pages: vec![codex_app_server_protocol::ThreadListResponse {
            data: Vec::new(),
            next_cursor: None,
            backwards_cursor: None,
        }],
        loaded_pages: vec![codex_app_server_protocol::ThreadLoadedListResponse {
            data: vec![root.id.clone()],
            next_cursor: None,
        }],
        loaded_reads: vec![codex_app_server_protocol::ThreadReadResponse {
            thread: root.clone(),
        }],
    };
    Ok(crate::proof_archive::QuietTreeSidecar {
        schema_version: 1,
        first: scan(&root),
        second: scan(&root),
        root,
    })
}

fn replay_broker_snapshot(
    pair_id: &str,
    ordinal: u8,
    arm: &ReplayArmResult,
) -> Result<crate::proof_archive::BrokerSnapshotSidecar> {
    let completions = crate::evidence::parse_notification_archive(&arm.notifications)?
        .into_iter()
        .filter_map(|notification| {
            let codex_app_server_protocol::ServerNotification::RawResponseCompleted(raw) =
                notification
            else {
                return None;
            };
            Some(crate::proof_archive::BrokerCompletionSidecar {
                response_id: raw.response_id,
                usage: raw.usage.map(|usage| crate::Usage {
                    total_tokens: usage.total_tokens,
                    input_tokens: usage.input_tokens,
                    cached_input_tokens: usage.cached_input_tokens,
                    cache_write_input_tokens: usage.cache_write_input_tokens,
                    output_tokens: usage.output_tokens,
                    reasoning_output_tokens: usage.reasoning_output_tokens,
                }),
                actual_model: Some("replay-fixture-recording".to_string()),
                deployment_or_fingerprint: None,
            })
        })
        .collect();
    Ok(crate::proof_archive::BrokerSnapshotSidecar {
        schema_version: 1,
        pair_id: pair_id.to_string(),
        run_ordinal: ordinal,
        condition: arm.condition,
        completions,
        in_flight: 0,
        attempt_index_file_sha256: sha256(b"replay:no-broker-ledger"),
        attempt_index_merkle_root: sha256(b"replay:no-provider-attempts"),
        global_attempt_start_inclusive: 0,
        global_attempt_end_exclusive: 0,
    })
}

fn build_replay_manifest(
    context: &ReplayFrozenContext,
    context_bytes: &[u8],
    mission: &codex_ai_ip_domain::HeldOutMissionCase,
    execution_context_sha256: &str,
    run_ordinal: u8,
    arm: &ReplayArmResult,
    config: &ConfigAuditEvidence,
    shared_config_sha256: &str,
    postprocess_evidence_index_sha256: &str,
    catalog_parity: &crate::CatalogParity,
    package_sha256: &str,
    request: &codex_responses_api_proxy::TransformedRequestEvidence,
) -> Result<RunManifest> {
    let prompt_sha256 = sha256(codex_ai_ip_runtime::root_prompt().as_bytes());
    let additional_context_sha256 =
        sha256(codex_ai_ip_runtime::evaluation_context(mission)?.as_bytes());
    let schema_sha256 = sha256(&serde_json::to_vec(
        &codex_ai_ip_runtime::content_package_schema()?,
    )?);
    let case_dir = context.fixtures["case"]
        .path
        .parent()
        .context("replay case has no parent")?;
    let thread_start_sha256 = sha256(&serde_json::to_vec(&build_thread_start(
        "replay-fixture",
        "replay-not-run",
        case_dir,
    )?)?);
    let turn_start_sha256 = sha256(&serde_json::to_vec(&build_turn_start(
        "root-thread",
        mission,
    )?)?);
    let native_skill_sha256 = (arm.condition == EvaluationCondition::Candidate)
        .then(|| context.fixtures["leadSkill"].sha256.clone());
    Ok(RunManifest {
        schema_version: 1,
        pair_id: context.pair_id.clone(),
        frozen_run_context_sha256: sha256(context_bytes),
        execution_context_sha256: execution_context_sha256.to_string(),
        run_ordinal,
        condition: arm.condition,
        fork_sha: context.fork_sha.clone(),
        case_sha256: context.fixtures["case"].sha256.clone(),
        source_materials_sha256: sha256(&serde_json::to_vec(&mission.materials)?),
        prompt_sha256,
        additional_context_sha256,
        output_schema_sha256: schema_sha256,
        thread_start_request_sha256: thread_start_sha256,
        turn_start_request_sha256: turn_start_sha256,
        shared_config_sha256: shared_config_sha256.to_string(),
        effective_config_sha256: config.effective_config_sha256.clone(),
        config_layers_sha256: config.config_layers_sha256.clone(),
        native_skill_sha256,
        pre_skill_catalog_sha256: arm.catalog.sha256.clone(),
        post_skill_catalog_sha256: arm.catalog.sha256.clone(),
        normalized_base_catalog_sha256: catalog_parity.normalized_base_catalog_sha256.clone(),
        skill_use_evidence_sha256: arm.skill_use_evidence_sha256.clone(),
        codex_binary_sha256: context.codex_binary.sha256.clone(),
        evaluator_binary_sha256: context.evaluator_binary.sha256.clone(),
        broker_component_sha256: sha256(b"replay:no-broker-component"),
        first_root_provider_request_commitment: digest_hex(request.raw_sha256),
        normalized_first_root_request_commitment: digest_hex(request.normalized_sha256),
        normalized_first_root_base_commitment: digest_hex(request.normalized_base_commitment),
        first_root_treatment_diff_commitment: request.treatment_diff_commitment.map(digest_hex),
        app_server_transcript_sha256: arm.transcript_sha256.clone(),
        broker_attempt_ledger_sha256: sha256(b"replay:no-broker-ledger"),
        attempt_index_root_sha256: sha256(b"replay:no-provider-attempts"),
        postprocess_evidence_index_sha256: postprocess_evidence_index_sha256.to_string(),
        content_package_sha256: package_sha256.to_string(),
        root_thread_id: "root-thread".to_string(),
        root_turn_id: "root-turn".to_string(),
        session_id: "replay-fixture-session".to_string(),
        provider_request_attempt_count: 0,
        provider_completed_response_count: 0,
        raw_response_count: arm.collected.raw_response_count,
        usage_scope: "completeTypedReplayTranscript".to_string(),
        usage: arm.collected.usage.clone(),
        model_label: "replay-fixture".to_string(),
        actual_model_revision: "replay-fixture-recording".to_string(),
        deployment_or_fingerprint_commitment: None,
        provider_label: "not-run".to_string(),
        provider_compatibility_name: ProofBrokerCompatibilityName::OpenAi,
        authorized_evaluation_run_cost_fen: 0,
        max_provider_request_attempts: 0,
        max_total_tokens: 0,
        max_elapsed_seconds: 0,
        elapsed_ms: 0,
        tree_closed: true,
        execution_mode: crate::ExecutionMode::Replay,
        mode_evidence: ModeEvidence::Replay {
            fixture_set_sha256: context.fixture_set_manifest.sha256.clone(),
        },
    })
}

/// Freezes the strict local-mock live context consumed by [`run_local_mock_pair`].
pub fn freeze_live_context(args: LiveFreezeArgs) -> Result<()> {
    freeze_live_context_inner(args, |_: LiveFreezeHookPoint| Ok(()))
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum LiveFreezeHookPoint {
    BeforeArtifactFreeze,
    AfterArtifactFreeze,
}

#[cfg(test)]
pub(crate) fn freeze_live_context_with_hook(
    args: LiveFreezeArgs,
    hook: impl FnMut(LiveFreezeHookPoint) -> Result<()>,
) -> Result<()> {
    freeze_live_context_inner(args, hook)
}

fn freeze_live_context_inner(
    args: LiveFreezeArgs,
    mut hook: impl FnMut(LiveFreezeHookPoint) -> Result<()>,
) -> Result<()> {
    validate_local_mock_upstream(&args.provider_upstream_url)?;
    if args.authorized_total_cost_fen != 0 || args.authorized_per_run_cost_fen != 0 {
        bail!("local mock evaluation must have zero authorized cost");
    }
    let repo_root = args
        .repo_root
        .canonicalize()
        .context("canonicalize live repo")?;
    let repo = GitWorktreeCommitment::freeze(&repo_root)?;
    if repo.head() != args.fork_sha {
        bail!("live freeze fork SHA does not match clean worktree HEAD");
    }
    let canonical_private_root = args
        .private_root
        .canonicalize()
        .context("canonicalize evaluation private root")?;
    if args.output.file_name() != Some(OsStr::new("frozen-run-context.json"))
        || args
            .output
            .parent()
            .context("live frozen context output has no parent")?
            .canonicalize()?
            != canonical_private_root
    {
        bail!("live frozen context must use the canonical private-root filename");
    }
    let pair_material = format!("{}:{}", args.fork_sha, args.model_label);
    let pair_id = sha256(pair_material.as_bytes());
    let commitment_key = if canonical_private_root.join("coordinator").try_exists()? {
        Some(crate::proof_commitment::RetainedProofCommitmentKey::create_fixed(
            &canonical_private_root,
        )?)
    } else {
        None
    };
    let retained_inputs = RetainedLiveInputDirectory::retain(
        &canonical_private_root,
        &args.attestation,
        &args.provider_budget_evidence,
        &args.rate_card,
        &args.billing_policy,
        &args.fx_policy,
    )?;
    let commitment_key = match commitment_key {
        Some(commitment_key) => commitment_key,
        None => crate::proof_commitment::RetainedProofCommitmentKey::create_fixed(
            &canonical_private_root,
        )?,
    };
    let commitment_key_sha256 = commitment_key.key_sha256().to_string();
    let public_run_id = commitment_key.derive_public_run_id(&pair_id)?;
    let coordinator = open_directory_at(&retained_inputs.private_handle, OsStr::new("coordinator"))
        .context("open proof commitment coordinator directory")?;
    let source_proof = import_live_source_proof_into_existing_inputs(
        &args.case,
        &args.material_root,
        &retained_inputs,
    )?;
    validate_native_attestation_freeze_binding(
        &source_proof.attestation,
        &args,
        &canonical_private_root,
        &retained_inputs.cost_inputs,
    )?;
    let supplier_statements = create_fresh_directory_at(
        &retained_inputs.inputs_handle,
        OsStr::new("supplier-statements"),
    )
    .context("create supplier statement policy directory")?;
    supplier_statements.sync_all()?;
    let cost_directory = create_fresh_directory_at(&coordinator, OsStr::new("cost"))
        .context("create coordinator cost directory")?;
    cost_directory.sync_all()?;
    coordinator.sync_all()?;
    let generated_dir = canonical_private_root.join("frozen-inputs");
    create_owner_only_dir(&generated_dir)?;
    let skill_path = generated_dir.join("lead-skill.md");
    write_owner_only_new(&skill_path, &read_regular_file_no_follow(&args.lead_skill)?)?;
    let mission_case = source_proof.mission_case;
    let schema_path = generated_dir.join("content-package-schema.json");
    write_owner_only_new(
        &schema_path,
        &serde_json::to_vec_pretty(&codex_ai_ip_runtime::content_package_schema()?)?,
    )?;
    let prompt_path = generated_dir.join("root-prompt.txt");
    write_owner_only_new(&prompt_path, codex_ai_ip_runtime::root_prompt().as_bytes())?;
    let additional_context_path = generated_dir.join("additional-context.txt");
    write_owner_only_new(
        &additional_context_path,
        codex_ai_ip_runtime::evaluation_context(&mission_case)?.as_bytes(),
    )?;
    let canonical_eval_tree = source_proof
        .case_path
        .parent()
        .context("imported case has no proof-copy root")?
        .to_path_buf();
    let thread_projection = build_thread_start(
        &args.model_label,
        "ai-ip-proof-broker",
        &canonical_eval_tree,
    )?;
    let thread_path = generated_dir.join("thread-start-request.json");
    write_owner_only_new(
        &thread_path,
        &serde_json::to_vec_pretty(&thread_projection)?,
    )?;
    let turn_projection = build_turn_start("00000000-0000-7000-8000-000000000000", &mission_case)?;
    let turn_path = generated_dir.join("turn-start-request.json");
    write_owner_only_new(&turn_path, &serde_json::to_vec_pretty(&turn_projection)?)?;
    let evaluator_binary = evaluator_binary_for_freeze(&generated_dir)?;
    let broker_source = repo_root.join("codex-rs/responses-api-proxy/src/broker.rs");
    let mut named = BTreeMap::from([
        ("source".to_string(), source_proof.case_path),
        (
            "materials".to_string(),
            source_proof.materials_manifest_path,
        ),
        ("codexBinary".to_string(), args.codex_bin),
        ("evaluatorBinary".to_string(), evaluator_binary),
        ("brokerSource".to_string(), broker_source),
        ("schema".to_string(), schema_path),
        ("prompt".to_string(), prompt_path),
        ("additionalContext".to_string(), additional_context_path),
        ("skill".to_string(), skill_path),
        ("threadStartRequest".to_string(), thread_path),
        ("turnStartRequest".to_string(), turn_path),
    ]);
    for (material_id, path) in source_proof.material_paths {
        named.insert(format!("material:{material_id}"), path);
    }
    hook(LiveFreezeHookPoint::BeforeArtifactFreeze)?;
    let frozen_artifacts = ArtifactCommitments::freeze(named)?;
    let retained_reverify = retained_inputs.reverify_unchanged();
    let after_hook = hook(LiveFreezeHookPoint::AfterArtifactFreeze);
    retained_reverify?;
    after_hook?;
    let mut artifacts = BTreeMap::new();
    for (name, artifact) in frozen_artifacts.artifacts {
        artifacts.insert(
            name,
            FrozenArtifactReference {
                path: artifact.canonical_path,
                sha256: artifact.sha256,
            },
        );
    }
    if source_proof.attestation_path != *retained_inputs.cost_inputs.attestation.path() {
        bail!("imported attestation path differs from its retained authority");
    }
    let retained_reference = |path: &Path, sha256: &str, leaf: &str| -> Result<_> {
        let expected_path = canonical_private_root.join("inputs").join(leaf);
        if path != expected_path {
            bail!("retained private input path is not its canonical fixed leaf");
        }
        if !is_lower_hex(sha256, 64) {
            bail!("retained private input commitment is not a SHA-256 digest");
        }
        Ok(FrozenArtifactReference {
            path: path.to_path_buf(),
            sha256: sha256.to_string(),
        })
    };
    let retained_artifacts = BTreeMap::from([
        (
            "attestation".to_string(),
            retained_reference(
                retained_inputs.cost_inputs.attestation.path(),
                retained_inputs.cost_inputs.attestation.sha256(),
                "held-out-attestation.json",
            )?,
        ),
        (
            "providerBudgetEvidence".to_string(),
            retained_reference(
                retained_inputs.cost_inputs.budget.path(),
                retained_inputs.cost_inputs.budget.sha256(),
                "provider-budget-evidence.json",
            )?,
        ),
        (
            "rateCard".to_string(),
            retained_reference(
                retained_inputs.cost_inputs.rate_card.path(),
                retained_inputs.cost_inputs.rate_card.sha256(),
                "rate-card.json",
            )?,
        ),
        (
            "billingPolicy".to_string(),
            retained_reference(
                retained_inputs.cost_inputs.billing_policy.path(),
                retained_inputs.cost_inputs.billing_policy.sha256(),
                "billing-policy.json",
            )?,
        ),
        (
            "fxPolicy".to_string(),
            retained_reference(
                retained_inputs.cost_inputs.fx_policy.path(),
                retained_inputs.cost_inputs.fx_policy.sha256(),
                "fx-policy.json",
            )?,
        ),
    ]);
    for (name, reference) in &retained_artifacts {
        if artifacts.insert(name.clone(), reference.clone()).is_some() {
            bail!("retained private input artifact name is duplicated");
        }
    }
    if retained_artifacts
        .iter()
        .any(|(name, reference)| artifacts.get(name) != Some(reference))
    {
        bail!("frozen context did not preserve exact retained private input references");
    }
    let context = FrozenRunContext {
        schema_version: 1,
        execution_mode: "live".to_string(),
        provider_mode: "not-run".to_string(),
        pair_id,
        public_run_id,
        commitment_key_sha256,
        candidate_sha: args.fork_sha.clone(),
        private_root: canonical_private_root.clone(),
        repo_root,
        repo_head: args.fork_sha,
        provider_upstream_url: args.provider_upstream_url,
        model_label: args.model_label,
        max_output_tokens: args.max_output_tokens_per_request,
        max_attempts_per_arm: args.max_provider_request_attempts_per_run,
        max_total_tokens: args.max_total_tokens_per_run,
        max_elapsed_seconds: args.max_elapsed_seconds_per_run,
        supplier_statement_policy: SupplierStatementPolicy {
            directory: "inputs/supplier-statements".to_string(),
            allowed_leaves: ["generic.json".to_string(), "candidate.json".to_string()],
            nested_entries_allowed: false,
        },
        artifacts,
    };
    validate_frozen_context(&context)?;
    let context_bytes = serde_json::to_vec_pretty(&context)?;
    FrozenContracts::load()?.validate_native_context(&context_bytes)?;
    commitment_key.reverify_binding(
        &context.commitment_key_sha256,
        &context.pair_id,
        &context.public_run_id,
    )?;
    retained_inputs.reverify_unchanged()?;
    reverify_child_directory_identity(
        &retained_inputs.inputs_handle,
        OsStr::new("supplier-statements"),
        &supplier_statements,
    )?;
    reverify_child_directory_identity(
        &retained_inputs.private_handle,
        OsStr::new("coordinator"),
        &coordinator,
    )?;
    reverify_child_directory_identity(&coordinator, OsStr::new("cost"), &cost_directory)?;
    write_owner_only_new(&args.output, &context_bytes)
}

pub(crate) struct PairRequestInspector {
    gate: Arc<PairCoordinator>,
    homes: IsolatedHomes,
    pair_deadline_rfc3339: String,
    state: Mutex<PairRequestInspectorState>,
}

#[derive(Default)]
struct PairRequestInspectorState {
    generic: Option<codex_responses_api_proxy::TransformedRequestEvidence>,
    candidate: Option<codex_responses_api_proxy::TransformedRequestEvidence>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PairRequestVerification {
    schema_version: u32,
    normalized_base_commitment: String,
    generic_raw_commitment: String,
    generic_normalized_commitment: String,
    candidate_raw_commitment: String,
    candidate_normalized_commitment: String,
    candidate_treatment_diff_commitment: String,
}

pub(crate) struct RequestCanonicalizationContext<'a> {
    pub(crate) condition: EvaluationCondition,
    pub(crate) known_thread_ids: &'a HashSet<String>,
    pub(crate) generic_codex_home: &'a Path,
    pub(crate) candidate_codex_home: &'a Path,
    pub(crate) deadline_rfc3339: &'a str,
}

pub(crate) fn canonicalize_first_root_request(
    body: &serde_json::Value,
    context: &RequestCanonicalizationContext<'_>,
) -> Result<codex_responses_api_proxy::TransformedRequestEvidence> {
    let codex_home = match context.condition {
        EvaluationCondition::Generic => context.generic_codex_home,
        EvaluationCondition::Candidate => context.candidate_codex_home,
    };
    let home = codex_home
        .parent()
        .context("isolated CODEX_HOME has no Home parent")?;
    let raw_sha256 = Sha256::digest(serde_json::to_vec(body)?).into();
    let mut normalized = normalize_committed_request_values(
        body,
        context.known_thread_ids,
        home,
        codex_home,
        context.deadline_rfc3339,
    )?;
    let normalized_sha256 = Sha256::digest(serde_json::to_vec(&normalized)?).into();
    let canonical_treatment = canonical_target_skill_treatment(Path::new("$CODEX_HOME"));
    let input = normalized
        .get_mut("input")
        .and_then(serde_json::Value::as_array_mut)
        .context("Responses request input must be an array")?;
    let matching = input
        .iter()
        .enumerate()
        .filter_map(|(index, item)| (item == &canonical_treatment).then_some(index))
        .collect::<Vec<_>>();
    let treatment_diff_commitment = match context.condition {
        EvaluationCondition::Generic if matching.is_empty() => None,
        EvaluationCondition::Generic => {
            bail!("generic request contains the target Skill treatment fragment")
        }
        EvaluationCondition::Candidate if matching.len() == 1 => {
            input.remove(matching[0]);
            Some(Sha256::digest(serde_json::to_vec(&canonical_treatment)?).into())
        }
        EvaluationCondition::Candidate => {
            bail!(
                "candidate request must contain exactly one canonical target Skill treatment fragment"
            )
        }
    };
    let normalized_base_commitment = Sha256::digest(serde_json::to_vec(&normalized)?).into();
    Ok(codex_responses_api_proxy::TransformedRequestEvidence {
        raw_sha256,
        normalized_sha256,
        normalized_base_commitment,
        treatment_diff_commitment,
    })
}

impl PairRequestInspector {
    pub(crate) fn new(
        gate: Arc<PairCoordinator>,
        homes: IsolatedHomes,
        pair_deadline_rfc3339: String,
    ) -> Self {
        Self {
            gate,
            homes,
            pair_deadline_rfc3339,
            state: Mutex::new(PairRequestInspectorState::default()),
        }
    }

    fn inspect_inner(
        &self,
        body: &serde_json::Value,
    ) -> Result<codex_responses_api_proxy::TransformedRequestEvidence> {
        let (condition, arm_attempt_count, known_threads) =
            self.gate.active_inspection_context()?;
        let evidence = canonicalize_first_root_request(
            body,
            &RequestCanonicalizationContext {
                condition,
                known_thread_ids: &known_threads,
                generic_codex_home: &self.homes.generic_codex_home,
                candidate_codex_home: &self.homes.candidate_codex_home,
                deadline_rfc3339: &self.pair_deadline_rfc3339,
            },
        )?;
        if arm_attempt_count == 0 {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow!("pair request inspector state lock poisoned"))?;
            let (slot_occupied, other_base) = match condition {
                EvaluationCondition::Generic => (
                    state.generic.is_some(),
                    state
                        .candidate
                        .as_ref()
                        .map(|other| other.normalized_base_commitment),
                ),
                EvaluationCondition::Candidate => (
                    state.candidate.is_some(),
                    state
                        .generic
                        .as_ref()
                        .map(|other| other.normalized_base_commitment),
                ),
            };
            if slot_occupied {
                bail!("first-root request was inspected more than once before authorization");
            }
            if other_base.is_some_and(|other| other != evidence.normalized_base_commitment) {
                bail!(
                    "first-root requests differ outside the one canonical target Skill treatment"
                );
            }
            match condition {
                EvaluationCondition::Generic => state.generic = Some(evidence.clone()),
                EvaluationCondition::Candidate => state.candidate = Some(evidence.clone()),
            }
        }
        Ok(evidence)
    }

    fn verified_pair(&self) -> Result<PairRequestVerification> {
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow!("pair request inspector state lock poisoned"))?;
        let generic = state
            .generic
            .as_ref()
            .context("generic first-root request was not inspected")?;
        let candidate = state
            .candidate
            .as_ref()
            .context("candidate first-root request was not inspected")?;
        build_pair_request_verification(generic, candidate)
    }
}

fn build_pair_request_verification(
    generic: &codex_responses_api_proxy::TransformedRequestEvidence,
    candidate: &codex_responses_api_proxy::TransformedRequestEvidence,
) -> Result<PairRequestVerification> {
    if generic.normalized_base_commitment != candidate.normalized_base_commitment {
        bail!("first-root requests differ outside the one canonical target Skill treatment");
    }
    if generic.treatment_diff_commitment.is_some() {
        bail!("generic request parity evidence contains a treatment commitment");
    }
    let candidate_treatment = candidate
        .treatment_diff_commitment
        .context("candidate treatment commitment is missing")?;
    Ok(PairRequestVerification {
        schema_version: 1,
        normalized_base_commitment: digest_hex(generic.normalized_base_commitment),
        generic_raw_commitment: digest_hex(generic.raw_sha256),
        generic_normalized_commitment: digest_hex(generic.normalized_sha256),
        candidate_raw_commitment: digest_hex(candidate.raw_sha256),
        candidate_normalized_commitment: digest_hex(candidate.normalized_sha256),
        candidate_treatment_diff_commitment: digest_hex(candidate_treatment),
    })
}

impl codex_responses_api_proxy::RequestInspector for PairRequestInspector {
    fn inspect(
        &self,
        body: &serde_json::Value,
    ) -> Result<codex_responses_api_proxy::TransformedRequestEvidence> {
        self.inspect_inner(body).map_err(|error| {
            let _ = self
                .gate
                .poison_permanently(&format!("request parity inspection failed: {error:#}"));
            error
        })
    }
}

fn normalize_committed_request_values(
    body: &serde_json::Value,
    known_threads: &HashSet<String>,
    home: &Path,
    codex_home: &Path,
    pair_deadline_rfc3339: &str,
) -> Result<serde_json::Value> {
    fn normalize(
        value: &serde_json::Value,
        key: Option<&str>,
        known_threads: &HashSet<String>,
        home: &Path,
        codex_home: &Path,
        pair_deadline_rfc3339: &str,
    ) -> Result<serde_json::Value> {
        match value {
            serde_json::Value::Object(object) => Ok(serde_json::Value::Object(
                object
                    .iter()
                    .map(|(key, value)| {
                        Ok((
                            key.clone(),
                            normalize(
                                value,
                                Some(key),
                                known_threads,
                                home,
                                codex_home,
                                pair_deadline_rfc3339,
                            )?,
                        ))
                    })
                    .collect::<Result<serde_json::Map<_, _>>>()?,
            )),
            serde_json::Value::Array(values) => Ok(serde_json::Value::Array(
                values
                    .iter()
                    .map(|value| {
                        normalize(
                            value,
                            None,
                            known_threads,
                            home,
                            codex_home,
                            pair_deadline_rfc3339,
                        )
                    })
                    .collect::<Result<Vec<_>>>()?,
            )),
            serde_json::Value::String(value) if key == Some("aiIpThreadId") => {
                if !known_threads.contains(value) {
                    bail!("request contains an uncommitted dynamic thread ID");
                }
                Ok(serde_json::Value::String("$THREAD_ID".to_string()))
            }
            serde_json::Value::String(value) if key == Some("aiIpGeneratedAt") => {
                if value != pair_deadline_rfc3339 {
                    bail!("request contains an uncommitted dynamic time value");
                }
                Ok(serde_json::Value::String("$PAIR_DEADLINE".to_string()))
            }
            serde_json::Value::String(value) => Ok(serde_json::Value::String(
                normalize_home_prefix(value, codex_home, "$CODEX_HOME")
                    .or_else(|| normalize_home_prefix(value, home, "$HOME"))
                    .unwrap_or_else(|| value.clone()),
            )),
            value => Ok(value.clone()),
        }
    }
    normalize(
        body,
        None,
        known_threads,
        home,
        codex_home,
        pair_deadline_rfc3339,
    )
}

fn normalize_home_prefix(value: &str, prefix: &Path, replacement: &str) -> Option<String> {
    let prefix = prefix.to_str()?;
    if value == prefix {
        return Some(replacement.to_string());
    }
    value
        .strip_prefix(prefix)
        .filter(|suffix| suffix.starts_with(std::path::MAIN_SEPARATOR))
        .map(|suffix| format!("{replacement}{suffix}"))
}

fn digest_hex(digest: [u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn canonical_target_skill_treatment(codex_home: &Path) -> serde_json::Value {
    serde_json::json!({
        "role": "developer",
        "content": [{
            "type": "input_text",
            "text": "Use the exact native Skill named in treatment metadata."
        }],
        "metadata": {
            "treatmentKind": "nativeSkill",
            "skillName": codex_ai_ip_runtime::LEAD_SKILL_NAME,
            "skillPath": codex_home
                .join("skills")
                .join(codex_ai_ip_runtime::LEAD_SKILL_NAME)
                .join("SKILL.md")
        }
    })
}

pub(crate) fn native_pair_total_token_cap(max_total_tokens_per_run: u64) -> Result<u64> {
    max_total_tokens_per_run
        .checked_mul(2)
        .context("native pair token ceiling is not representable")
}

/// Executes exactly two arms against the frozen loopback mock upstream.
pub fn run_local_mock_pair(path: &Path) -> Result<()> {
    // Binding is deliberately first. `bind` creates the loopback listener but
    // cannot accept or forward until the fully validated gate is activated.
    let placeholder_config = codex_responses_api_proxy::ProxyConfig {
        listen_port: None,
        upstream_url: reqwest::Url::parse("http://127.0.0.1:1/v1/responses")?,
        dump_dir: None,
        http_shutdown: false,
        default_request_timeout: None,
        request_transform: None,
    };
    let bound = codex_responses_api_proxy::bind(&placeholder_config)?;
    let pair_started_at_instant = Instant::now();
    let started_at = chrono::Utc::now();
    let pair_deadline = establish_pair_deadline(path, pair_started_at_instant)?;
    let broker_port = bound.addr().port();
    let frozen = run_sync_before_deadline(pair_deadline, || verify_frozen_context(path))?;
    let initial_cost_namespaces = run_sync_before_deadline(pair_deadline, || {
        RetainedInitialCostNamespaces::retain(&frozen.context.private_root)
    })?;
    let max_total_tokens_per_pair = run_sync_before_deadline(pair_deadline, || {
        validate_local_mock_upstream(&frozen.context.provider_upstream_url)?;
        let verified_deadline = pair_started_at_instant
            .checked_add(Duration::from_secs(frozen.context.max_elapsed_seconds))
            .context("verified pair deadline overflow")?;
        if verified_deadline != pair_deadline {
            bail!("frozen pair deadline changed during context verification");
        }
        native_pair_total_token_cap(frozen.context.max_total_tokens)
    })?;
    let frozen_path = run_sync_before_deadline(pair_deadline, || {
        std::env::var("PATH").context("pinned App Server PATH is unavailable")
    })?;
    let mut guard = run_sync_before_deadline(pair_deadline, || frozen.execution_guard())?;
    let coordinator_dir = frozen.context.private_root.join("coordinator");
    let skill_bytes = run_sync_before_deadline(pair_deadline, || frozen.artifact_bytes("skill"))?;
    let runtime = run_sync_before_deadline(pair_deadline, || {
        Ok(Arc::new(BrokerRuntimeConfig::with_run_limits(
            frozen.max_attempts_per_arm(),
            frozen.max_output_tokens(),
            1024 * 1024,
            max_total_tokens_per_pair,
        )?))
    })?;
    let shared_config = run_sync_before_deadline(pair_deadline, || {
        build_shared_config(&frozen.context.model_label, broker_port)
    })?;
    let homes = run_sync_before_deadline(pair_deadline, || {
        prepare_isolated_homes(
            &frozen.context.private_root,
            &shared_config.bytes,
            codex_ai_ip_runtime::LEAD_SKILL_NAME,
            &skill_bytes,
        )
    })?;
    run_sync_before_deadline(pair_deadline, || {
        verify_isolated_home_parity(&homes, codex_ai_ip_runtime::LEAD_SKILL_NAME, &skill_bytes)
    })?;
    let mut generated =
        run_sync_before_deadline(pair_deadline, || GeneratedExecutionArtifacts::new(&homes))?;
    run_sync_before_deadline(pair_deadline, || {
        guard.advance(ExecutionBoundary::ContextFrozen)
    })?;
    run_sync_before_deadline(pair_deadline, || {
        generated.advance(ExecutionBoundary::ContextFrozen, &coordinator_dir)
    })?;

    let order = run_sync_before_deadline(pair_deadline, || {
        commit_arm_order(&frozen, &coordinator_dir)
    })?;
    let first = order.first();
    let second = order.second();
    let deadline = run_sync_before_deadline(pair_deadline, || {
        started_at
            .checked_add_signed(chrono::Duration::seconds(i64::try_from(
                frozen.context.max_elapsed_seconds,
            )?))
            .context("pair deadline is out of range")
    })?;
    let execution_bytes = run_sync_before_deadline(pair_deadline, || {
        to_exact_pretty_json(serde_json::json!({
            "schemaVersion": 1,
            "providerMode": "not-run",
            "frozenRunContextSha256": frozen.sha256(),
            "armOrderCommitment": order.seed_commitment(),
            "firstCondition": first,
            "secondCondition": second,
            "startedAt": started_at.to_rfc3339(),
            "deadline": deadline.to_rfc3339(),
            "broker": {"host": "127.0.0.1", "port": broker_port, "path": "/v1/responses"},
            "modelLabel": frozen.context.model_label,
            "maxOutputTokensPerRequest": frozen.context.max_output_tokens,
            "maxProviderRequestAttemptsPerRun": frozen.context.max_attempts_per_arm,
            "maxTotalTokensPerRun": frozen.context.max_total_tokens,
            "maxElapsedSecondsPerRun": frozen.context.max_elapsed_seconds,
            "genericHome": homes.generic_home,
            "genericCodexHome": homes.generic_codex_home,
            "candidateHome": homes.candidate_home,
            "candidateCodexHome": homes.candidate_codex_home,
            "sharedConfigSha256": sha256(&shared_config.bytes),
            "pathSha256": sha256(frozen_path.as_bytes()),
        }))
    })?;
    let execution_context_sha256 =
        run_sync_before_deadline(pair_deadline, || Ok(sha256(&execution_bytes)))?;
    run_sync_before_deadline(pair_deadline, || {
        write_owner_only_new(
            &coordinator_dir.join("execution-context.json"),
            &execution_bytes,
        )
    })?;
    let gate = run_sync_before_deadline(pair_deadline, || {
        Ok(Arc::new(PairCoordinator::create(BrokerGateConfig {
            ledger_path: coordinator_dir.join("attempt-index.jsonl"),
            receipt_dir: coordinator_dir.join("receipts"),
            pair_id: frozen.pair_id().to_string(),
            frozen_run_context_sha256: frozen.sha256().to_string(),
            execution_context_sha256: execution_context_sha256.clone(),
            arm_order_commitment: order.seed_commitment().to_string(),
            require_proof_bindings: true,
            runtime: runtime.clone(),
        })?))
    })?;
    let request_inspector = Arc::new(PairRequestInspector::new(
        gate.clone(),
        homes.clone(),
        deadline.to_rfc3339(),
    ));
    let proxy_config = run_sync_before_deadline(pair_deadline, || {
        Ok(codex_responses_api_proxy::ProxyConfig {
            listen_port: None,
            upstream_url: reqwest::Url::parse(frozen.provider_upstream_url())?,
            dump_dir: None,
            http_shutdown: false,
            default_request_timeout: None,
            request_transform: Some(runtime.request_transform(request_inspector.clone())),
        })
    })?;
    let order_commit = run_sync_before_deadline(pair_deadline, || gate.commit_order(order.clone()));
    poison_on_error(&gate, "commit arm order", order_commit)?;
    let order_guard = run_sync_before_deadline(pair_deadline, || {
        guard.advance(ExecutionBoundary::OrderCommitted)
    });
    poison_on_error(&gate, "rehash after arm-order commitment", order_guard)?;
    let generated_order = run_sync_before_deadline(pair_deadline, || {
        generated.advance(ExecutionBoundary::OrderCommitted, &coordinator_dir)
    });
    poison_on_error(
        &gate,
        "verify generated execution context after arm order",
        generated_order,
    )?;
    let deadline_check = remaining_pair_duration(pair_deadline);
    poison_on_error(
        &gate,
        "check deadline before broker activation",
        deadline_check,
    )?;
    let proxy_result = run_sync_before_deadline(pair_deadline, || {
        codex_responses_api_proxy::activate(
            bound,
            proxy_config,
            codex_responses_api_proxy::local_mock_auth_header(),
            gate.clone(),
            gate.clone(),
        )
    });
    let proxy = poison_on_error(&gate, "activate bound proof broker", proxy_result)?;
    let run_result = (|| -> Result<()> {
        let tokio_runtime =
            run_sync_before_deadline(pair_deadline, || Ok(tokio::runtime::Runtime::new()?))?;
        let mut outcomes = Vec::with_capacity(2);
        let mut manifests = Vec::with_capacity(2);
        for (index, condition) in [(1_u8, first), (2_u8, second)] {
            let pre = if index == 1 {
                ExecutionBoundary::Arm1Pre
            } else {
                ExecutionBoundary::Arm2Pre
            };
            run_sync_before_deadline(pair_deadline, || guard.advance(pre))?;
            run_sync_before_deadline(pair_deadline, || generated.advance(pre, &coordinator_dir))?;
            let verified_config_bytes =
                run_sync_before_deadline(pair_deadline, || generated.config_bytes(condition))?;
            remaining_pair_duration(pair_deadline)?;
            let outcome = tokio_runtime.block_on(run_app_server_arm(
                &frozen,
                &homes,
                &shared_config,
                &gate,
                index,
                condition,
                &coordinator_dir,
                &verified_config_bytes,
                &frozen_path,
                pair_deadline,
                &deadline.to_rfc3339(),
            ))?;
            remaining_pair_duration(pair_deadline)?;
            let proof = gate.active_arm_proof_snapshot()?;
            let completions = gate.active_completions()?;
            let mut manifest = build_live_run_manifest(
                &frozen,
                &outcome,
                &proof,
                &completions,
                &shared_config,
                &execution_context_sha256,
                &order,
            )?;
            let postprocess_evidence_index_sha256 =
                crate::proof_archive_seal::seal_postprocess_archive(
                    &frozen.context.private_root,
                    Path::new("coordinator"),
                    native_postprocess_capture(&frozen, &outcome, &proof, &completions)?,
                )?;
            manifest.postprocess_evidence_index_sha256 = postprocess_evidence_index_sha256;
            crate::verify_postprocess_archive(
                &frozen.context.private_root,
                &manifest,
                &serde_json::from_slice(&frozen.artifact_bytes("source")?)?,
                &frozen.artifact_bytes("skill")?,
            )?;
            let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
            write_owner_only_new(
                &coordinator_dir.join(format!("run-{index}-manifest.json")),
                &manifest_bytes,
            )?;
            let manifest_sha256 = sha256(&manifest_bytes);
            gate.bind_active_run_manifest(manifest_sha256.clone())?;
            outcomes.push(outcome);
            manifests.push((manifest, manifest_sha256));
            if outcomes.len() == 2 {
                verify_live_arm_business_parity(&frozen, &homes, &outcomes)?;
                initial_cost_namespaces.reverify_empty()?;
                crate::private_inventory::bootstrap_private_inventory(
                    &frozen.context.private_root,
                    frozen.pair_id(),
                    frozen.sha256(),
                    &chrono::Utc::now().to_rfc3339(),
                )?;
                write_live_pair_verification(
                    &frozen,
                    &execution_context_sha256,
                    &request_inspector,
                    &manifests,
                    &coordinator_dir,
                )?;
                crate::private_inventory::append_private_inventory(
                    &frozen.context.private_root,
                    Path::new("coordinator/pair-verification.json"),
                )?;
            }
            let post = if index == 1 {
                ExecutionBoundary::Arm1Post
            } else {
                ExecutionBoundary::Arm2Post
            };
            run_sync_before_deadline(pair_deadline, || guard.advance(post))?;
            run_sync_before_deadline(pair_deadline, || generated.advance(post, &coordinator_dir))?;
            run_sync_before_deadline(pair_deadline, || gate.seal_arm())?;
            if index == 2 {
                crate::private_inventory::append_private_inventory(
                    &frozen.context.private_root,
                    Path::new("coordinator/receipts/arm-2-receipt.json"),
                )?;
            }
        }
        Ok(())
    })();
    match run_result {
        Err(error) => {
            let _ = gate.poison_permanently(&format!("paired evaluator failed: {error:#}"));
            let shutdown_result = proxy.shutdown_with_timeout(
                remaining_pair_duration(pair_deadline).unwrap_or(Duration::from_millis(1)),
            );
            if let Err(shutdown_error) = shutdown_result {
                let _ =
                    gate.poison_permanently(&format!("broker teardown failed: {shutdown_error:#}"));
            }
            Err(error)
        }
        Ok(()) => finish_pair_after_teardown(
            &gate,
            pair_deadline,
            |timeout| proxy.shutdown_with_timeout(timeout),
            || {
                run_sync_before_deadline(pair_deadline, || {
                    guard.advance(ExecutionBoundary::Finished)
                })?;
                run_sync_before_deadline(pair_deadline, || {
                    generated.advance(ExecutionBoundary::Finished, &coordinator_dir)
                })
            },
        )
        .and_then(|receipt| {
            crate::private_inventory::append_private_inventory(
                &frozen.context.private_root,
                Path::new("coordinator/receipts/pair-receipt.json"),
            )?;
            crate::private_inventory::verify_private_inventory(&frozen.context.private_root)?;
            Ok(receipt)
        })
        .map(drop),
    }
}

pub(crate) fn finish_pair_after_teardown(
    gate: &PairCoordinator,
    pair_deadline: Instant,
    teardown: impl FnOnce(Duration) -> Result<()>,
    final_rehash: impl FnOnce() -> Result<()>,
) -> Result<crate::PairReceipt> {
    let timeout = poison_on_error(
        gate,
        "check deadline before broker teardown",
        remaining_pair_duration(pair_deadline),
    )?;
    poison_on_error(gate, "broker teardown", teardown(timeout))?;
    poison_on_error(
        gate,
        "check deadline after broker teardown",
        remaining_pair_duration(pair_deadline),
    )?;
    poison_on_error(
        gate,
        "check deadline before final rehash",
        remaining_pair_duration(pair_deadline),
    )?;
    poison_on_error(gate, "final rehash", final_rehash())?;
    poison_on_error(
        gate,
        "check deadline after final rehash",
        remaining_pair_duration(pair_deadline),
    )?;
    gate.finish()
}

fn poison_on_error<T>(gate: &PairCoordinator, step: &str, result: Result<T>) -> Result<T> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            let _ = gate.poison_permanently(&format!("{step} failed: {error:#}"));
            Err(error)
        }
    }
}

struct ArmRunEvidence {
    run_ordinal: u8,
    condition: EvaluationCondition,
    root_thread_id: String,
    root_turn_id: String,
    session_id: String,
    config: ConfigAuditEvidence,
    start: crate::proof_archive::StartSidecar,
    config_sidecar: crate::proof_archive::ConfigSidecar,
    pre_catalog: CapturedCatalog,
    post_catalog: CapturedCatalog,
    quiet_tree: crate::proof_archive::QuietTreeSidecar,
    notifications: Vec<u8>,
    collected: CollectedReplay,
    successful_read_observed: bool,
    skill_use_evidence_sha256: Option<String>,
    tree: TreeEvidence,
    app_server_transcript_sha256: String,
    elapsed_ms: u128,
}

fn build_live_run_manifest(
    frozen: &VerifiedFrozenContext,
    outcome: &ArmRunEvidence,
    proof: &crate::broker_gate::ActiveArmProofSnapshot,
    completions: &[codex_responses_api_proxy::ResponseCompletedMetadata],
    shared_config: &crate::FrozenSharedConfig,
    execution_context_sha256: &str,
    order: &CommittedArmOrder,
) -> Result<RunManifest> {
    if proof.run_ordinal != outcome.run_ordinal
        || proof.condition != outcome.condition
        || proof.completion_count != u64::try_from(completions.len())?
        || outcome.tree.usage != outcome.collected.usage
        || outcome.tree.raw_response_count != outcome.collected.raw_response_count
        || !outcome.tree.tree_closed
    {
        bail!("native arm evidence is internally inconsistent");
    }
    let actual_models = completions
        .iter()
        .map(|completion| {
            completion
                .actual_model
                .clone()
                .context("completed response is missing actual model revision")
        })
        .collect::<Result<BTreeSet<_>>>()?;
    if actual_models.len() != 1 {
        bail!("completed responses do not share one actual model revision");
    }
    let actual_model_revision = actual_models
        .into_iter()
        .next()
        .context("native arm has no completed provider response")?;
    let deployments = completions
        .iter()
        .filter_map(|completion| completion.deployment_or_fingerprint.as_ref())
        .collect::<BTreeSet<_>>();
    if deployments.len() > 1
        || (!deployments.is_empty()
            && completions
                .iter()
                .any(|completion| completion.deployment_or_fingerprint.is_none()))
    {
        bail!("completed responses do not share one deployment commitment");
    }
    let deployment_or_fingerprint_commitment = deployments
        .into_iter()
        .next()
        .map(|deployment| sha256(deployment.as_bytes()));
    let artifact_sha = |name: &str| {
        frozen
            .artifacts
            .sha256(name)
            .map(str::to_string)
            .with_context(|| format!("missing frozen artifact {name}"))
    };
    let content_package_sha256 = sha256(&serde_json::to_vec(&outcome.collected.content_package)?);
    let native_skill_sha256 = (outcome.condition == EvaluationCondition::Candidate)
        .then(|| artifact_sha("skill"))
        .transpose()?;
    Ok(RunManifest {
        schema_version: 1,
        pair_id: frozen.context.pair_id.clone(),
        frozen_run_context_sha256: frozen.sha256.clone(),
        execution_context_sha256: execution_context_sha256.to_string(),
        run_ordinal: outcome.run_ordinal,
        condition: outcome.condition,
        fork_sha: frozen.context.candidate_sha.clone(),
        case_sha256: artifact_sha("source")?,
        source_materials_sha256: artifact_sha("materials")?,
        prompt_sha256: artifact_sha("prompt")?,
        additional_context_sha256: artifact_sha("additionalContext")?,
        output_schema_sha256: artifact_sha("schema")?,
        thread_start_request_sha256: artifact_sha("threadStartRequest")?,
        turn_start_request_sha256: artifact_sha("turnStartRequest")?,
        shared_config_sha256: sha256(&shared_config.bytes),
        effective_config_sha256: outcome.config.effective_config_sha256.clone(),
        config_layers_sha256: outcome.config.config_layers_sha256.clone(),
        native_skill_sha256,
        pre_skill_catalog_sha256: outcome.pre_catalog.snapshot.sha256.clone(),
        post_skill_catalog_sha256: outcome.post_catalog.snapshot.sha256.clone(),
        normalized_base_catalog_sha256: outcome
            .pre_catalog
            .snapshot
            .normalized_base_catalog_sha256(),
        skill_use_evidence_sha256: outcome.skill_use_evidence_sha256.clone(),
        codex_binary_sha256: artifact_sha("codexBinary")?,
        evaluator_binary_sha256: artifact_sha("evaluatorBinary")?,
        broker_component_sha256: artifact_sha("brokerSource")?,
        first_root_provider_request_commitment: proof.first_root_request.raw_commitment.clone(),
        normalized_first_root_request_commitment: proof
            .first_root_request
            .normalized_commitment
            .clone(),
        normalized_first_root_base_commitment: proof
            .first_root_request
            .normalized_base_commitment
            .clone(),
        first_root_treatment_diff_commitment: proof
            .first_root_request
            .treatment_diff_commitment
            .clone(),
        app_server_transcript_sha256: outcome.app_server_transcript_sha256.clone(),
        broker_attempt_ledger_sha256: proof.attempt_index_file_sha256.clone(),
        attempt_index_root_sha256: proof.attempt_index_merkle_root.clone(),
        postprocess_evidence_index_sha256: String::new(),
        content_package_sha256,
        root_thread_id: outcome.root_thread_id.clone(),
        root_turn_id: outcome.root_turn_id.clone(),
        session_id: outcome.session_id.clone(),
        provider_request_attempt_count: proof.arm_attempt_count,
        provider_completed_response_count: proof.completion_count,
        raw_response_count: outcome.tree.raw_response_count,
        usage_scope: "completeNativeThreadTree".to_string(),
        usage: outcome.tree.usage.clone(),
        model_label: frozen.context.model_label.clone(),
        actual_model_revision,
        deployment_or_fingerprint_commitment,
        provider_label: NATIVE_PROVIDER_LABEL.to_string(),
        provider_compatibility_name: ProofBrokerCompatibilityName::OpenAi,
        authorized_evaluation_run_cost_fen: 0,
        max_provider_request_attempts: frozen.context.max_attempts_per_arm,
        max_total_tokens: i64::try_from(frozen.context.max_total_tokens)?,
        max_elapsed_seconds: frozen.context.max_elapsed_seconds,
        elapsed_ms: outcome.elapsed_ms,
        tree_closed: true,
        execution_mode: crate::ExecutionMode::Mock,
        mode_evidence: ModeEvidence::Mock {
            provider_mode: MockProviderMode::NotRun,
            synthetic_fixture_sha256: artifact_sha("attestation")?,
            arm_order_commitment: order.seed_commitment().to_string(),
        },
    })
}

fn native_postprocess_capture(
    frozen: &VerifiedFrozenContext,
    outcome: &ArmRunEvidence,
    proof: &crate::broker_gate::ActiveArmProofSnapshot,
    completions: &[codex_responses_api_proxy::ResponseCompletedMetadata],
) -> Result<crate::proof_archive::ArmPostprocessCapture> {
    let completions = completions
        .iter()
        .map(|completion| crate::proof_archive::BrokerCompletionSidecar {
            response_id: completion.response_id.clone(),
            usage: completion.usage.as_ref().map(|usage| crate::Usage {
                total_tokens: usage.total_tokens,
                input_tokens: usage.input_tokens,
                cached_input_tokens: usage.cached_input_tokens,
                cache_write_input_tokens: usage.cache_write_input_tokens,
                output_tokens: usage.output_tokens,
                reasoning_output_tokens: usage.reasoning_output_tokens,
            }),
            actual_model: completion.actual_model.clone(),
            deployment_or_fingerprint: completion.deployment_or_fingerprint.clone(),
        })
        .collect();
    Ok(crate::proof_archive::ArmPostprocessCapture {
        pair_id: frozen.pair_id().to_string(),
        run_ordinal: outcome.run_ordinal,
        condition: outcome.condition,
        evidence_source: crate::ArchiveEvidenceSource::NativeRecorded,
        notifications: outcome.notifications.clone(),
        start: outcome.start.clone(),
        config: outcome.config_sidecar.clone(),
        pre_catalog: crate::proof_archive::CatalogSidecar {
            schema_version: 1,
            roots: outcome.pre_catalog.roots.clone(),
            response: outcome.pre_catalog.response.clone(),
        },
        post_catalog: crate::proof_archive::CatalogSidecar {
            schema_version: 1,
            roots: outcome.post_catalog.roots.clone(),
            response: outcome.post_catalog.response.clone(),
        },
        quiet_tree: outcome.quiet_tree.clone(),
        broker_snapshot: crate::proof_archive::BrokerSnapshotSidecar {
            schema_version: 1,
            pair_id: frozen.pair_id().to_string(),
            run_ordinal: outcome.run_ordinal,
            condition: outcome.condition,
            completions,
            in_flight: 0,
            attempt_index_file_sha256: proof.attempt_index_file_sha256.clone(),
            attempt_index_merkle_root: proof.attempt_index_merkle_root.clone(),
            global_attempt_start_inclusive: proof.global_attempt_start_inclusive,
            global_attempt_end_exclusive: proof.global_attempt_end_exclusive,
        },
    })
}

fn verify_live_arm_business_parity(
    frozen: &VerifiedFrozenContext,
    homes: &IsolatedHomes,
    outcomes: &[ArmRunEvidence],
) -> Result<()> {
    let generic = outcomes
        .iter()
        .find(|outcome| outcome.condition == EvaluationCondition::Generic)
        .context("paired result is missing the generic arm")?;
    let candidate = outcomes
        .iter()
        .find(|outcome| outcome.condition == EvaluationCondition::Candidate)
        .context("paired result is missing the candidate arm")?;
    if outcomes.len() != 2
        || generic.successful_read_observed
        || generic.skill_use_evidence_sha256.is_some()
        || !candidate.successful_read_observed
        || candidate.skill_use_evidence_sha256.is_none()
    {
        bail!("paired Skill-use evidence differs from the assigned treatment");
    }
    let candidate_skill_path = homes
        .candidate_codex_home
        .join("skills")
        .join(codex_ai_ip_runtime::LEAD_SKILL_NAME)
        .join("SKILL.md");
    let skill_bytes = frozen.artifact_bytes("skill")?;
    let pre = crate::compare_catalogs(
        &generic.pre_catalog.snapshot,
        &candidate.pre_catalog.snapshot,
        &candidate_skill_path,
        &skill_bytes,
    )?;
    let post = crate::compare_catalogs(
        &generic.post_catalog.snapshot,
        &candidate.post_catalog.snapshot,
        &candidate_skill_path,
        &skill_bytes,
    )?;
    if pre != post {
        bail!("paired pre/post catalog parity evidence differs");
    }
    if generic.config != candidate.config {
        bail!(
            "paired config audit evidence differs: generic={:?}, candidate={:?}",
            generic.config,
            candidate.config
        );
    }
    Ok(())
}

fn write_live_pair_verification(
    frozen: &VerifiedFrozenContext,
    execution_context_sha256: &str,
    request_inspector: &PairRequestInspector,
    manifests: &[(RunManifest, String)],
    coordinator_dir: &Path,
) -> Result<()> {
    if manifests.len() != 2 {
        bail!("final pair verification requires exactly two run manifests");
    }
    let generic = manifests
        .iter()
        .find(|(manifest, _)| manifest.condition == EvaluationCondition::Generic)
        .context("final verification is missing the generic manifest")?;
    let candidate = manifests
        .iter()
        .find(|(manifest, _)| manifest.condition == EvaluationCondition::Candidate)
        .context("final verification is missing the candidate manifest")?;
    let request = request_inspector.verified_pair()?;
    if generic.0.first_root_provider_request_commitment != request.generic_raw_commitment
        || generic.0.normalized_first_root_request_commitment
            != request.generic_normalized_commitment
        || generic.0.normalized_first_root_base_commitment != request.normalized_base_commitment
        || generic.0.first_root_treatment_diff_commitment.is_some()
        || candidate.0.first_root_provider_request_commitment != request.candidate_raw_commitment
        || candidate.0.normalized_first_root_request_commitment
            != request.candidate_normalized_commitment
        || candidate.0.normalized_first_root_base_commitment != request.normalized_base_commitment
        || candidate.0.first_root_treatment_diff_commitment.as_deref()
            != Some(request.candidate_treatment_diff_commitment.as_str())
        || generic.0.normalized_base_catalog_sha256 != candidate.0.normalized_base_catalog_sha256
        || generic.0.effective_config_sha256 != candidate.0.effective_config_sha256
        || generic.0.config_layers_sha256 != candidate.0.config_layers_sha256
    {
        bail!("run manifests do not bind the verified pair evidence");
    }
    let mut verification = serde_json::json!({
        "schemaVersion": 1,
        "pairId": frozen.context.pair_id,
        "frozenRunContextSha256": frozen.sha256,
        "executionContextSha256": execution_context_sha256,
        "genericRunManifestSha256": generic.1,
        "candidateRunManifestSha256": candidate.1,
        "normalizedBaseCatalogSha256": generic.0.normalized_base_catalog_sha256,
        "effectiveConfigSha256": generic.0.effective_config_sha256,
        "configLayersSha256": generic.0.config_layers_sha256,
        "requestParity": request
    });
    append_pair_output_commitments(
        &mut verification,
        &generic.0.content_package_sha256,
        &candidate.0.content_package_sha256,
    )?;
    let bytes = to_exact_pretty_json(verification)?;
    write_owner_only_new(&coordinator_dir.join("pair-verification.json"), &bytes)
}

pub(crate) fn to_exact_pretty_json(mut value: serde_json::Value) -> Result<Vec<u8>> {
    value.sort_all_objects();
    Ok(serde_json::to_vec_pretty(&value)?)
}

fn append_pair_output_commitments(
    verification: &mut serde_json::Value,
    generic_content_package_sha256: &str,
    candidate_content_package_sha256: &str,
) -> Result<()> {
    let fields = serde_json::to_value(PairOutputCommitments {
        generic_content_package_sha256: generic_content_package_sha256.to_string(),
        candidate_content_package_sha256: candidate_content_package_sha256.to_string(),
    })?;
    verification
        .as_object_mut()
        .context("pair verification is not an object")?
        .extend(
            fields
                .as_object()
                .context("pair output commitments are not an object")?
                .clone(),
        );
    Ok(())
}

#[derive(Clone)]
struct CapturedCatalog {
    response: codex_app_server_protocol::SkillsListResponse,
    roots: crate::proof_archive::CatalogRootsSidecar,
    snapshot: CatalogSnapshot,
}

async fn read_skill_catalog(
    app_server: &mut AppServerClient,
    roots: &CatalogRoots,
    pair_deadline: Instant,
) -> Result<CapturedCatalog> {
    let params = codex_app_server_protocol::SkillsListParams {
        cwds: vec![roots.case_dir.clone()],
        force_reload: true,
    };
    let timeout = remaining_pair_duration(pair_deadline)?;
    let response: codex_app_server_protocol::SkillsListResponse = {
        let protocol = app_server.protocol_mut()?;
        run_before_deadline(
            pair_deadline,
            protocol.request("skills/list", Some(&params), timeout),
        )
        .await?
    };
    let snapshot =
        run_sync_before_deadline(pair_deadline, || crate::normalize_catalog(&response, roots))?;
    Ok(CapturedCatalog {
        response,
        roots: crate::proof_archive::CatalogRootsSidecar {
            codex_home: roots.codex_home.clone(),
            host_home: roots.host_home.clone(),
            case_dir: roots.case_dir.clone(),
        },
        snapshot,
    })
}

async fn run_app_server_arm(
    frozen: &VerifiedFrozenContext,
    homes: &IsolatedHomes,
    shared_config: &crate::FrozenSharedConfig,
    gate: &Arc<PairCoordinator>,
    run_ordinal: u8,
    condition: EvaluationCondition,
    coordinator_dir: &Path,
    verified_config_bytes: &[u8],
    frozen_path: &str,
    pair_deadline: Instant,
    pair_deadline_rfc3339: &str,
) -> Result<ArmRunEvidence> {
    use codex_app_server_protocol::ApprovalsReviewer;
    use codex_app_server_protocol::AskForApproval;
    use codex_app_server_protocol::ThreadStartResponse;
    use codex_app_server_protocol::TurnStartResponse;
    use codex_app_server_protocol::TurnStatus;

    let arm_started = Instant::now();
    validate_managed_source_before_arm(frozen, gate, pair_deadline)?;
    let (home, codex_home) = match condition {
        EvaluationCondition::Generic => (&homes.generic_home, &homes.generic_codex_home),
        EvaluationCondition::Candidate => (&homes.candidate_home, &homes.candidate_codex_home),
    };
    let temporary = home.join("tmp");
    let environment = run_sync_before_deadline(pair_deadline, || {
        ChildEnvironment::from_environment(
            &BTreeMap::from([("PATH".to_string(), frozen_path.to_string())]),
            home.to_str().context("non-UTF-8 isolated Home")?,
            codex_home
                .to_str()
                .context("non-UTF-8 isolated CODEX_HOME")?,
            temporary.to_str().context("non-UTF-8 isolated temp")?,
        )
    })?;
    let codex_binary = run_sync_before_deadline(pair_deadline, || {
        frozen
            .artifacts
            .artifacts
            .get("codexBinary")
            .context("missing codex binary commitment")
    })?;
    let retained_executable_descriptor =
        run_sync_before_deadline(pair_deadline, || codex_binary.verified_exec_descriptor())?;
    let mut app_server = run_before_deadline(
        pair_deadline,
        AppServerClient::spawn_verified(
            &codex_binary.canonical_path,
            retained_executable_descriptor,
            &coordinator_dir.join(format!("app-server-{run_ordinal}.stderr")),
            &environment,
        ),
    )
    .await
    .context("spawn pinned App Server before the pair deadline")?;
    let canonical_eval_tree = run_sync_before_deadline(pair_deadline, || {
        frozen.artifacts.artifacts["source"]
            .canonical_path
            .parent()
            .context("managed held-out case has no proof-copy root")
    })?;
    let handshake_timeout = remaining_pair_duration(pair_deadline)?;
    let handshake = run_before_deadline(
        pair_deadline,
        app_server.handshake(codex_home, canonical_eval_tree, handshake_timeout),
    )
    .await?;
    let canonical_config_path = run_sync_before_deadline(pair_deadline, || {
        Ok(codex_home.join("config.toml").canonicalize()?)
    })?;
    let config_evidence = run_sync_before_deadline(pair_deadline, || {
        audit_frozen_config(
            &handshake.config,
            &handshake.requirements,
            canonical_config_path.clone(),
            verified_config_bytes.to_vec(),
            shared_config.layer_json.clone(),
        )
    })?;
    let config_sidecar = crate::proof_archive::ConfigSidecar {
        schema_version: 1,
        response: handshake.config,
        requirements: handshake.requirements,
        canonical_config_path,
        expected_config_utf8: String::from_utf8(verified_config_bytes.to_vec())?,
        expected_layer_config: shared_config.layer_json.clone(),
    };
    let catalog_roots = CatalogRoots {
        codex_home: codex_home.clone(),
        host_home: home.clone(),
        case_dir: canonical_eval_tree.to_path_buf(),
    };
    let pre_catalog = read_skill_catalog(&mut app_server, &catalog_roots, pair_deadline).await?;
    let thread_params = run_sync_before_deadline(pair_deadline, || {
        let params = build_thread_start(
            &frozen.context.model_label,
            "ai-ip-proof-broker",
            canonical_eval_tree,
        )?;
        if serde_json::to_vec_pretty(&params)? != frozen.artifact_bytes("threadStartRequest")? {
            bail!("runtime thread/start request differs from frozen projection");
        }
        Ok(params)
    })?;
    let request_timeout = remaining_pair_duration(pair_deadline)?;
    let started: ThreadStartResponse = {
        remaining_pair_duration(pair_deadline)?;
        let protocol = app_server.protocol_mut()?;
        run_before_deadline(
            pair_deadline,
            protocol.request("thread/start", Some(&thread_params), request_timeout),
        )
        .await?
    };
    let mut tree = run_sync_before_deadline(pair_deadline, || {
        if started.model != frozen.context.model_label
            || started.model_provider != "ai-ip-proof-broker"
            || started.cwd.as_path() != canonical_eval_tree
            || started.approval_policy != AskForApproval::Never
            || started.approvals_reviewer != ApprovalsReviewer::User
            || started.service_tier.is_some()
            || started
                .active_permission_profile
                .as_ref()
                .is_none_or(|profile| {
                    profile.id != crate::app_server::EVALUATION_PERMISSION_PROFILE
                })
        {
            bail!("thread/start response differs from the frozen execution controls");
        }
        crate::TreeEventCollector::new(&started.thread)
    })?;
    run_sync_before_deadline(pair_deadline, || {
        gate.activate_arm(ArmActivation {
            run_ordinal,
            condition,
            root_thread_id: started.thread.id.clone(),
            deadline: pair_deadline,
            deadline_rfc3339: pair_deadline_rfc3339.to_string(),
        })
    })?;
    let mission_case: codex_ai_ip_domain::HeldOutMissionCase =
        run_sync_before_deadline(pair_deadline, || {
            Ok(serde_json::from_slice(&frozen.artifact_bytes("source")?)?)
        })?;
    let turn_params = run_sync_before_deadline(pair_deadline, || {
        let params = build_turn_start(&started.thread.id, &mission_case)?;
        let mut normalized = params.clone();
        normalized.thread_id = "00000000-0000-7000-8000-000000000000".to_string();
        if serde_json::to_vec_pretty(&normalized)? != frozen.artifact_bytes("turnStartRequest")? {
            bail!("runtime turn/start request differs from frozen projection");
        }
        Ok(params)
    })?;
    let request_timeout = remaining_pair_duration(pair_deadline)?;
    let started_turn: TurnStartResponse = {
        remaining_pair_duration(pair_deadline)?;
        let protocol = app_server.protocol_mut()?;
        run_before_deadline(
            pair_deadline,
            protocol.request("turn/start", Some(&turn_params), request_timeout),
        )
        .await?
    };
    run_sync_before_deadline(pair_deadline, || {
        if started_turn.turn.status != TurnStatus::InProgress || !started_turn.turn.items.is_empty()
        {
            bail!("turn/start response is not a fresh in-progress turn");
        }
        Ok(())
    })?;
    let mut replay = ReplayCollector::new(
        started.thread.id.clone(),
        started_turn.turn.id.clone(),
        HashSet::from([started.thread.id.clone()]),
    );
    let candidate_skill_path = codex_home
        .join("skills")
        .join(codex_ai_ip_runtime::LEAD_SKILL_NAME)
        .join("SKILL.md");
    let mut skill_use = run_sync_before_deadline(pair_deadline, || match condition {
        EvaluationCondition::Generic => {
            SkillUseTracker::new_forbidden(&candidate_skill_path, &frozen.artifact_bytes("skill")?)
        }
        EvaluationCondition::Candidate => {
            SkillUseTracker::new_required(&candidate_skill_path, &frozen.artifact_bytes("skill")?)
        }
    })?;
    let mut notifications = crate::evidence::NotificationRecorder::default();
    let completion_timeout = remaining_pair_duration(pair_deadline)?;
    let completed = {
        let protocol = app_server.protocol_mut()?;
        run_before_deadline(
            pair_deadline,
            protocol.wait_for_turn_completion(
                &started.thread.id,
                &started_turn.turn.id,
                completion_timeout,
                |notification| {
                    observe_captured_notification(
                        gate,
                        &started.thread.id,
                        &mut tree,
                        &mut replay,
                        &mut skill_use,
                        &mut notifications,
                        notification,
                    )?;
                    Ok(())
                },
            ),
        )
        .await
        .context("wait for App Server turn completion")?
    };
    run_sync_before_deadline(pair_deadline, || {
        if completed.turn.status != TurnStatus::Completed {
            bail!("App Server turn did not complete successfully");
        }
        Ok(())
    })?;
    let first_scan = complete_quiet_tree_scan(
        app_server.protocol_mut()?,
        &started.thread,
        pair_deadline,
        |notification| {
            observe_captured_notification(
                gate,
                &started.thread.id,
                &mut tree,
                &mut replay,
                &mut skill_use,
                &mut notifications,
                notification,
            )
            .map(drop)
        },
    )
    .await
    .context("collect first complete tree scan")?;
    app_server
        .protocol_mut()?
        .observe_until_quiet(Duration::from_secs(2), pair_deadline, |notification| {
            observe_captured_notification(
                gate,
                &started.thread.id,
                &mut tree,
                &mut replay,
                &mut skill_use,
                &mut notifications,
                notification,
            )
        })
        .await
        .context("observe full App Server quiet window")?;
    let second_scan = complete_quiet_tree_scan(
        app_server.protocol_mut()?,
        &started.thread,
        pair_deadline,
        |notification| {
            if observe_captured_notification(
                gate,
                &started.thread.id,
                &mut tree,
                &mut replay,
                &mut skill_use,
                &mut notifications,
                notification,
            )? {
                bail!("late relevant App Server event arrived during the final tree scan");
            }
            Ok(())
        },
    )
    .await
    .context("collect final complete tree scan")?;
    let post_catalog = read_skill_catalog(&mut app_server, &catalog_roots, pair_deadline).await?;
    run_sync_before_deadline(pair_deadline, || {
        crate::validate_stable_catalog(&pre_catalog.snapshot, &post_catalog.snapshot)
    })?;
    let status = run_before_deadline(
        pair_deadline,
        app_server.close_observing(pair_deadline, |notification| {
            if observe_captured_notification(
                gate,
                &started.thread.id,
                &mut tree,
                &mut replay,
                &mut skill_use,
                &mut notifications,
                notification,
            )? {
                bail!("late relevant App Server event arrived after the final tree scan");
            }
            Ok(())
        }),
    )
    .await
    .context("close App Server while observing late notifications")?;
    if !status.success() {
        bail!("pinned App Server exited unsuccessfully");
    }
    run_sync_before_deadline(pair_deadline, || {
        first_scan
            .scan
            .verify_broker_thread_ids(&gate.active_thread_ids()?)
    })?;
    let tree_evidence = run_sync_before_deadline(pair_deadline, || {
        tree.close(
            &first_scan.scan,
            &second_scan.scan,
            &gate.active_completions()?,
            gate.in_flight_count(),
        )
    })?;
    let collected = run_sync_before_deadline(pair_deadline, || replay.finish(&mission_case))?;
    let skill_use = run_sync_before_deadline(pair_deadline, || skill_use.finish())?;
    let notification_bytes = notifications.finish();
    Ok(ArmRunEvidence {
        run_ordinal,
        condition,
        root_thread_id: started.thread.id.clone(),
        root_turn_id: started_turn.turn.id.clone(),
        session_id: started.thread.session_id.clone(),
        config: config_evidence,
        start: crate::proof_archive::StartSidecar {
            schema_version: 1,
            thread: started.clone(),
            turn: started_turn.clone(),
        },
        config_sidecar,
        pre_catalog,
        post_catalog,
        quiet_tree: crate::proof_archive::QuietTreeSidecar {
            schema_version: 1,
            root: started.thread.clone(),
            first: first_scan.raw,
            second: second_scan.raw,
        },
        app_server_transcript_sha256: sha256(&notification_bytes),
        notifications: notification_bytes,
        collected,
        successful_read_observed: skill_use.successful_read_observed,
        skill_use_evidence_sha256: skill_use.evidence_sha256,
        tree: tree_evidence,
        elapsed_ms: arm_started.elapsed().as_millis(),
    })
}

struct CapturedTreeScan {
    scan: crate::TreeScan,
    raw: crate::proof_archive::TreeScanSidecar,
}

async fn complete_quiet_tree_scan<R, W>(
    client: &mut crate::JsonLineClient<R, W>,
    root: &codex_app_server_protocol::Thread,
    deadline: Instant,
    mut observe: impl FnMut(&codex_app_server_protocol::ServerNotification) -> Result<()>,
) -> Result<CapturedTreeScan>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    use codex_app_server_protocol::ThreadListParams;
    use codex_app_server_protocol::ThreadListResponse;
    use codex_app_server_protocol::ThreadLoadedListParams;
    use codex_app_server_protocol::ThreadLoadedListResponse;
    use codex_app_server_protocol::ThreadReadParams;
    use codex_app_server_protocol::ThreadReadResponse;
    use codex_app_server_protocol::ThreadSourceKind;

    let mut ancestor_pages = Vec::new();
    let mut cursor = None;
    loop {
        if ancestor_pages.len() >= 100 {
            bail!("thread/list pagination exceeded the frozen bound");
        }
        let params = ThreadListParams {
            cursor: cursor.clone(),
            limit: Some(100),
            sort_key: None,
            sort_direction: None,
            model_providers: Some(vec!["ai-ip-proof-broker".to_string()]),
            source_kinds: Some(vec![
                ThreadSourceKind::SubAgent,
                ThreadSourceKind::SubAgentThreadSpawn,
                ThreadSourceKind::SubAgentOther,
            ]),
            archived: Some(false),
            section_id: None,
            project_id: None,
            cwd: None,
            use_state_db_only: false,
            search_term: None,
            parent_thread_id: None,
            ancestor_thread_id: Some(root.id.clone()),
        };
        let page: ThreadListResponse = client
            .request(
                "thread/list",
                Some(&params),
                remaining_pair_duration(deadline)?,
            )
            .await?;
        client.observe_queued_notifications(|notification| {
            observe(notification)?;
            Ok(is_relevant_tree_notification(notification))
        })?;
        cursor = page.next_cursor.clone();
        ancestor_pages.push(page);
        if cursor.is_none() {
            break;
        }
    }

    let mut loaded_pages = Vec::new();
    let mut cursor = None;
    loop {
        if loaded_pages.len() >= 100 {
            bail!("thread/loaded/list pagination exceeded the frozen bound");
        }
        let params = ThreadLoadedListParams {
            cursor: cursor.clone(),
            limit: Some(100),
        };
        let page: ThreadLoadedListResponse = client
            .request(
                "thread/loaded/list",
                Some(&params),
                remaining_pair_duration(deadline)?,
            )
            .await?;
        client.observe_queued_notifications(|notification| {
            observe(notification)?;
            Ok(is_relevant_tree_notification(notification))
        })?;
        cursor = page.next_cursor.clone();
        loaded_pages.push(page);
        if cursor.is_none() {
            break;
        }
    }

    let mut read_ids = BTreeSet::from([root.id.clone()]);
    read_ids.extend(
        ancestor_pages
            .iter()
            .flat_map(|page| &page.data)
            .map(|thread| thread.id.clone()),
    );
    read_ids.extend(loaded_pages.iter().flat_map(|page| page.data.clone()));
    let mut loaded_reads = Vec::new();
    for thread_id in read_ids {
        let response: ThreadReadResponse = client
            .request(
                "thread/read",
                Some(&ThreadReadParams {
                    thread_id,
                    include_turns: true,
                }),
                remaining_pair_duration(deadline)?,
            )
            .await?;
        loaded_reads.push(response);
        client.observe_queued_notifications(|notification| {
            observe(notification)?;
            Ok(is_relevant_tree_notification(notification))
        })?;
    }
    remaining_pair_duration(deadline)?;
    let scan =
        crate::TreeScan::from_typed_pages(root, &ancestor_pages, &loaded_pages, &loaded_reads)?;
    remaining_pair_duration(deadline)?;
    Ok(CapturedTreeScan {
        scan,
        raw: crate::proof_archive::TreeScanSidecar {
            ancestor_pages,
            loaded_pages,
            loaded_reads,
        },
    })
}

fn observe_tree_notification(
    gate: &PairCoordinator,
    root_thread_id: &str,
    tree: &mut crate::TreeEventCollector,
    notification: &codex_app_server_protocol::ServerNotification,
) -> Result<bool> {
    observe_app_server_lifecycle(gate, root_thread_id, notification)?;
    tree.ingest(notification.clone())?;
    Ok(is_relevant_tree_notification(notification))
}

fn observe_post_completion_notification(
    gate: &PairCoordinator,
    root_thread_id: &str,
    tree: &mut crate::TreeEventCollector,
    skill_use: &mut SkillUseTracker,
    notification: &codex_app_server_protocol::ServerNotification,
) -> Result<bool> {
    skill_use.ingest(notification)?;
    observe_tree_notification(gate, root_thread_id, tree, notification)
}

fn observe_captured_notification(
    gate: &PairCoordinator,
    root_thread_id: &str,
    tree: &mut crate::TreeEventCollector,
    replay: &mut ReplayCollector,
    skill_use: &mut SkillUseTracker,
    recorder: &mut crate::evidence::NotificationRecorder,
    notification: &codex_app_server_protocol::ServerNotification,
) -> Result<bool> {
    recorder.record(notification)?;
    replay.ingest(notification.clone())?;
    observe_post_completion_notification(gate, root_thread_id, tree, skill_use, notification)
}

fn is_relevant_tree_notification(
    notification: &codex_app_server_protocol::ServerNotification,
) -> bool {
    use codex_app_server_protocol::ServerNotification;

    matches!(
        notification,
        ServerNotification::ThreadStarted(_)
            | ServerNotification::TurnStarted(_)
            | ServerNotification::TurnCompleted(_)
            | ServerNotification::ThreadStatusChanged(_)
            | ServerNotification::RawResponseCompleted(_)
            | ServerNotification::ItemGuardianApprovalReviewStarted(_)
            | ServerNotification::ItemGuardianApprovalReviewCompleted(_)
            | ServerNotification::GuardianWarning(_)
    )
}

fn remaining_pair_duration(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .context("frozen absolute pair deadline expired")
}

pub(crate) fn establish_pair_deadline(path: &Path, started_at: Instant) -> Result<Instant> {
    let supplied_metadata = fs::symlink_metadata(path)
        .with_context(|| format!("stat frozen context for pair deadline {}", path.display()))?;
    if !supplied_metadata.file_type().is_file()
        || supplied_metadata.file_type().is_symlink()
        || has_multiple_links(&supplied_metadata)
    {
        bail!("pair deadline requires a single-link regular frozen context");
    }
    let bytes = read_regular_file_no_follow(path)?;
    let context: FrozenRunContext =
        serde_json::from_slice(&bytes).context("parse frozen context for pair deadline")?;
    if context.max_elapsed_seconds == 0 {
        bail!("frozen pair duration must be positive");
    }
    started_at
        .checked_add(Duration::from_secs(context.max_elapsed_seconds))
        .context("pair deadline overflow")
}

pub(crate) async fn run_before_deadline<T>(
    deadline: Instant,
    future: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    tokio::time::timeout(remaining_pair_duration(deadline)?, future)
        .await
        .context("frozen absolute pair deadline expired")?
}

pub(crate) fn run_sync_before_deadline<T>(
    deadline: Instant,
    operation: impl FnOnce() -> Result<T>,
) -> Result<T> {
    remaining_pair_duration(deadline)?;
    let result = operation();
    remaining_pair_duration(deadline)?;
    result
}

fn observe_app_server_lifecycle(
    gate: &PairCoordinator,
    root_thread_id: &str,
    notification: &codex_app_server_protocol::ServerNotification,
) -> Result<()> {
    use codex_app_server_protocol::ServerNotification;
    use codex_app_server_protocol::ThreadSource;

    let ServerNotification::ThreadStarted(started) = notification else {
        return Ok(());
    };
    if started.thread.id == root_thread_id && started.thread.parent_thread_id.is_none() {
        return Ok(());
    }
    match (
        started.thread.thread_source.as_ref(),
        started.thread.parent_thread_id.as_ref(),
    ) {
        (Some(ThreadSource::GuardianReview), _) => {
            gate.observe_lifecycle(ThreadLifecycle::GuardianReview)
        }
        (Some(ThreadSource::Subagent), Some(parent_thread_id)) => {
            gate.observe_lifecycle(ThreadLifecycle::Subagent {
                thread_id: started.thread.id.clone(),
                parent_thread_id: parent_thread_id.clone(),
            })
        }
        _ => gate.observe_lifecycle(ThreadLifecycle::InvalidThread),
    }
}

fn validate_local_mock_upstream(raw: &str) -> Result<()> {
    let upstream = reqwest::Url::parse(raw).context("parse local mock upstream URL")?;
    if upstream.scheme() != "http"
        || !upstream.host_str().is_some_and(is_loopback_host)
        || upstream.port().is_none_or(|port| port == 0)
        || upstream.path() != "/v1/responses"
        || upstream.query().is_some()
        || upstream.fragment().is_some()
        || !upstream.username().is_empty()
        || upstream.password().is_some()
    {
        bail!("Phase 0A accepts only an explicit-port HTTP loopback /v1/responses upstream");
    }
    Ok(())
}

fn evaluator_binary_for_freeze(_generated_dir: &Path) -> Result<PathBuf> {
    let current = std::env::current_exe()
        .context("resolve evaluator executable")?
        .canonicalize()
        .context("canonicalize evaluator executable")?;
    #[cfg(test)]
    if has_multiple_links(&fs::metadata(&current)?) {
        let copied = _generated_dir.join("evaluator-binary-under-test");
        write_owner_only_new(&copied, &fs::read(&current)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&copied, fs::Permissions::from_mode(0o700))?;
        }
        return copied
            .canonicalize()
            .context("canonicalize copied evaluator test binary");
    }
    Ok(current)
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// Re-reads the private frozen context and returns its bytes commitment.
pub fn verify_frozen_context(path: &Path) -> Result<VerifiedFrozenContext> {
    let supplied_metadata = fs::symlink_metadata(path)
        .with_context(|| format!("stat supplied frozen context {}", path.display()))?;
    if supplied_metadata.file_type().is_symlink() || has_multiple_links(&supplied_metadata) {
        bail!("frozen context path must not be a symlink or hardlink");
    }
    let canonical_path = path
        .canonicalize()
        .with_context(|| format!("canonicalize frozen context {}", path.display()))?;
    let metadata = fs::symlink_metadata(&canonical_path)
        .with_context(|| format!("stat frozen context {}", canonical_path.display()))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        bail!("frozen context must be a regular non-symlink file");
    }
    let (frozen_file, bytes) = ArtifactCommitment::freeze_context(&canonical_path)?;
    FrozenContracts::load()?
        .validate_native_context(&bytes)
        .context("validate frozen native context contract")?;
    let context: FrozenRunContext =
        serde_json::from_slice(&bytes).context("parse strict frozen context JSON")?;
    if serde_json::to_vec_pretty(&context)? != bytes {
        bail!("native frozen context is not exact producer-order typed JSON");
    }
    if canonical_path != context.private_root.join("frozen-run-context.json") {
        bail!("frozen context is outside its canonical private-root location");
    }
    require_owner_only_file(&canonical_path)?;
    validate_frozen_context(&context)?;
    let commitment_key =
        crate::proof_commitment::RetainedProofCommitmentKey::read_fixed(&context.private_root)?;
    commitment_key.reverify_binding(
        &context.commitment_key_sha256,
        &context.pair_id,
        &context.public_run_id,
    )?;
    let artifacts = ArtifactCommitments::from_references(&context.artifacts)?;
    Ok(VerifiedFrozenContext {
        canonical_path,
        sha256: sha256(&bytes),
        raw_bytes: bytes,
        context,
        frozen_file,
        artifacts,
        commitment_key,
    })
}

#[cfg(test)]
pub(crate) fn encode_native_context_for_test(raw: &[u8]) -> Result<Vec<u8>> {
    FrozenContracts::load()?
        .validate_native_context(raw)
        .context("validate synthetic native context contract")?;
    let context: FrozenRunContext = serde_json::from_value(crate::jcs::parse_json(raw)?)
        .context("parse synthetic native context")?;
    Ok(serde_json::to_vec_pretty(&context)?)
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CommittedArmOrder {
    first: EvaluationCondition,
    second: EvaluationCondition,
    seed_commitment: String,
    frozen_run_context_sha256: String,
}

impl CommittedArmOrder {
    pub fn first(&self) -> EvaluationCondition {
        self.first
    }

    pub fn second(&self) -> EvaluationCondition {
        self.second
    }

    pub fn seed_commitment(&self) -> &str {
        &self.seed_commitment
    }

    pub fn frozen_run_context_sha256(&self) -> &str {
        &self.frozen_run_context_sha256
    }
}

/// Commits an unbiased arm order using only the operating system CSPRNG.
///
/// The verified frozen context is required by type, so arm order cannot be
/// selected before context freeze. No seed or order is accepted from callers.
pub fn commit_arm_order(
    frozen: &VerifiedFrozenContext,
    coordinator_directory: &Path,
) -> Result<CommittedArmOrder> {
    frozen.frozen_file.verify_context_bytes(&frozen.raw_bytes)?;
    if sha256(&frozen.raw_bytes) != frozen.sha256() {
        bail!("frozen run context changed before arm-order commitment");
    }
    prepare_pair_coordinator(coordinator_directory)?;
    let mut seed = [0u8; 32];
    OsRng
        .try_fill_bytes(&mut seed)
        .map_err(|error| anyhow!("operating system CSPRNG failed: {error}"))?;
    let path = coordinator_directory.join("arm-order-seed.bin");
    write_owner_only_new(&path, &seed)?;
    let seed_commitment = sha256(&seed);
    let first = if seed[0] & 1 == 0 {
        EvaluationCondition::Generic
    } else {
        EvaluationCondition::Candidate
    };
    let second = match first {
        EvaluationCondition::Generic => EvaluationCondition::Candidate,
        EvaluationCondition::Candidate => EvaluationCondition::Generic,
    };
    Ok(CommittedArmOrder {
        first,
        second,
        seed_commitment,
        frozen_run_context_sha256: frozen.sha256().to_string(),
    })
}

pub(crate) fn prepare_pair_coordinator(path: &Path) -> Result<()> {
    if path.try_exists()? {
        crate::secure_fs::fsync_directory(path)
    } else {
        crate::secure_fs::create_owner_only_dir_new(path)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct IsolatedHomes {
    pub generic_home: PathBuf,
    pub generic_codex_home: PathBuf,
    pub candidate_home: PathBuf,
    pub candidate_codex_home: PathBuf,
}

/// Creates two owner-only Homes from one shared config byte vector.
///
/// The generic Skill catalog is empty. The candidate catalog differs only by
/// the one supplied `SKILL.md`; no authentication file is created.
pub fn prepare_isolated_homes(
    private_root: &Path,
    shared_config_bytes: &[u8],
    skill_name: &str,
    skill_bytes: &[u8],
) -> Result<IsolatedHomes> {
    validate_leaf_name(skill_name)?;
    create_owner_only_dir(private_root)?;
    let generic_home = private_root.join("generic-home");
    let candidate_home = private_root.join("candidate-home");
    for path in [&generic_home, &candidate_home] {
        if path.exists() {
            bail!("isolated Home already exists: {}", path.display());
        }
    }
    let generic_codex_home = generic_home.join(".codex");
    let candidate_codex_home = candidate_home.join(".codex");
    for path in [
        &generic_home,
        &candidate_home,
        &generic_codex_home,
        &candidate_codex_home,
        &generic_codex_home.join("skills"),
        &candidate_codex_home.join("skills"),
        &generic_home.join("tmp"),
        &candidate_home.join("tmp"),
    ] {
        create_owner_only_dir(path)?;
    }
    write_owner_only_new(&generic_codex_home.join("config.toml"), shared_config_bytes)?;
    write_owner_only_new(
        &candidate_codex_home.join("config.toml"),
        shared_config_bytes,
    )?;
    let candidate_skill = candidate_codex_home.join("skills").join(skill_name);
    create_owner_only_dir(&candidate_skill)?;
    write_owner_only_new(&candidate_skill.join("SKILL.md"), skill_bytes)?;
    Ok(IsolatedHomes {
        generic_home,
        generic_codex_home,
        candidate_home,
        candidate_codex_home,
    })
}

/// Verifies complete relative-tree parity, allowing only the target Skill in
/// the candidate Home.
pub fn verify_isolated_home_parity(
    homes: &IsolatedHomes,
    skill_name: &str,
    expected_skill_bytes: &[u8],
) -> Result<()> {
    validate_leaf_name(skill_name)?;
    let generic = collect_tree(&homes.generic_home)?;
    let candidate = collect_tree(&homes.candidate_home)?;
    let mut expected_candidate = generic;
    expected_candidate.insert(format!(".codex/skills/{skill_name}"), None);
    expected_candidate.insert(
        format!(".codex/skills/{skill_name}/SKILL.md"),
        Some(expected_skill_bytes.to_vec()),
    );
    if candidate != expected_candidate {
        bail!("isolated Home trees differ outside the target Skill");
    }
    Ok(())
}

/// Explicit allowlist applied to an evaluator child after `env_clear()`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ChildEnvironment {
    variables: BTreeMap<String, String>,
}

impl ChildEnvironment {
    pub fn from_environment(
        host: &BTreeMap<String, String>,
        home: &str,
        codex_home: &str,
        temp: &str,
    ) -> Result<Self> {
        let path = host
            .get("PATH")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("frozen PATH is required"))?;
        let variables = BTreeMap::from([
            ("PATH".to_string(), path.clone()),
            ("HOME".to_string(), home.to_string()),
            ("CODEX_HOME".to_string(), codex_home.to_string()),
            ("TMP".to_string(), temp.to_string()),
            ("TEMP".to_string(), temp.to_string()),
            ("TMPDIR".to_string(), temp.to_string()),
        ]);
        #[cfg(windows)]
        let variables = {
            let mut variables = variables;
            variables.insert("USERPROFILE".to_string(), home.to_string());
            for name in ["SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT"] {
                if let Some(value) = host.get(name) {
                    variables.insert(name.to_string(), value.clone());
                }
            }
            variables
        };
        reject_secret_names(&variables)?;
        Ok(Self { variables })
    }

    pub fn variables(&self) -> &BTreeMap<String, String> {
        &self.variables
    }

    /// Builds a process with no inherited environment and only the allowlist.
    pub fn command(&self, program: impl AsRef<OsStr>) -> Command {
        let mut command = Command::new(program);
        command.env_clear();
        command.envs(&self.variables);
        command
    }

    /// Applies the same clear-then-allowlist policy to a Tokio child command.
    pub fn apply_tokio(&self, command: &mut tokio::process::Command) {
        command.env_clear();
        command.envs(&self.variables);
    }
}

#[derive(Debug, Clone)]
struct ArtifactCommitment {
    canonical_path: PathBuf,
    sha256: String,
    handle: Arc<File>,
}

#[derive(Debug, Clone)]
pub(crate) struct RetainedFrozenContextFile {
    commitment: ArtifactCommitment,
    raw_bytes: Vec<u8>,
}

impl RetainedFrozenContextFile {
    pub(crate) fn retain(path: &Path) -> Result<Self> {
        let (commitment, raw_bytes) = ArtifactCommitment::freeze_context(path)?;
        Ok(Self {
            commitment,
            raw_bytes,
        })
    }

    pub(crate) fn raw_bytes(&self) -> &[u8] {
        &self.raw_bytes
    }

    pub(crate) fn sha256(&self) -> &str {
        &self.commitment.sha256
    }

    pub(crate) fn reverify_unchanged(&self) -> Result<()> {
        self.commitment.verify_context_bytes(&self.raw_bytes)
    }
}

/// Named byte commitments revalidated at coordinator boundaries.
///
/// Callers use explicit semantic names (source, materials, binaries, config,
/// Schema, prompt, Skill, request projections, receipts) so an error identifies
/// the exact mutable input. Verification always re-opens each canonical path.
#[derive(Debug, Clone)]
pub struct ArtifactCommitments {
    artifacts: BTreeMap<String, ArtifactCommitment>,
}

pub const REQUIRED_EXECUTION_ARTIFACTS: &[&str] = &[
    "source",
    "attestation",
    "materials",
    "codexBinary",
    "evaluatorBinary",
    "brokerSource",
    "schema",
    "prompt",
    "additionalContext",
    "skill",
    "threadStartRequest",
    "turnStartRequest",
    "providerBudgetEvidence",
    "rateCard",
    "billingPolicy",
    "fxPolicy",
];

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ExecutionBoundary {
    ContextFrozen,
    OrderCommitted,
    Arm1Pre,
    Arm1Post,
    Arm2Pre,
    Arm2Post,
    Finished,
}

struct GeneratedExecutionArtifacts {
    immutable: BTreeMap<String, ArtifactCommitment>,
}

impl GeneratedExecutionArtifacts {
    fn new(homes: &IsolatedHomes) -> Result<Self> {
        let generic = ArtifactCommitment::freeze(&homes.generic_codex_home.join("config.toml"))?;
        let candidate =
            ArtifactCommitment::freeze(&homes.candidate_codex_home.join("config.toml"))?;
        if generic.sha256 != candidate.sha256 {
            bail!("isolated generated config commitments differ");
        }
        Ok(Self {
            immutable: BTreeMap::from([
                ("genericConfig".to_string(), generic),
                ("candidateConfig".to_string(), candidate),
            ]),
        })
    }

    fn advance(&mut self, boundary: ExecutionBoundary, coordinator_dir: &Path) -> Result<()> {
        for (name, artifact) in &self.immutable {
            artifact
                .read_verified()
                .with_context(|| format!("verify generated {name}"))?;
        }
        if boundary != ExecutionBoundary::ContextFrozen {
            self.capture_once(
                "executionContext",
                &coordinator_dir.join("execution-context.json"),
            )?;
        }
        Ok(())
    }

    fn config_bytes(&self, condition: EvaluationCondition) -> Result<Vec<u8>> {
        let name = match condition {
            EvaluationCondition::Generic => "genericConfig",
            EvaluationCondition::Candidate => "candidateConfig",
        };
        self.immutable
            .get(name)
            .context("missing generated config commitment")?
            .read_verified()
    }

    fn capture_once(&mut self, name: &str, path: &Path) -> Result<()> {
        if let Some(existing) = self.immutable.get(name) {
            existing.read_verified()?;
        } else {
            self.immutable
                .insert(name.to_string(), ArtifactCommitment::freeze(path)?);
        }
        Ok(())
    }
}

/// Coordinator-owned immutable inputs and the only valid rehash sequence.
pub struct FrozenExecutionGuard {
    artifacts: ArtifactCommitments,
    worktree: GitWorktreeCommitment,
    next: usize,
}

impl FrozenExecutionGuard {
    pub fn create(paths: BTreeMap<String, PathBuf>, repository: &Path) -> Result<Self> {
        require_artifact_names(paths.keys().map(String::as_str))?;
        Ok(Self {
            artifacts: ArtifactCommitments::freeze(paths)?,
            worktree: GitWorktreeCommitment::freeze(repository)?,
            next: 0,
        })
    }

    fn from_commitments(artifacts: ArtifactCommitments, repository: &Path) -> Result<Self> {
        require_artifact_names(artifacts.artifacts.keys().map(String::as_str))?;
        Ok(Self {
            artifacts,
            worktree: GitWorktreeCommitment::freeze(repository)?,
            next: 0,
        })
    }

    pub fn advance(&mut self, boundary: ExecutionBoundary) -> Result<()> {
        const ORDER: [ExecutionBoundary; 7] = [
            ExecutionBoundary::ContextFrozen,
            ExecutionBoundary::OrderCommitted,
            ExecutionBoundary::Arm1Pre,
            ExecutionBoundary::Arm1Post,
            ExecutionBoundary::Arm2Pre,
            ExecutionBoundary::Arm2Post,
            ExecutionBoundary::Finished,
        ];
        if ORDER.get(self.next) != Some(&boundary) {
            bail!("execution rehash boundary is skipped, duplicated, or out of order");
        }
        self.artifacts.verify()?;
        self.worktree.verify()?;
        self.next += 1;
        Ok(())
    }
}

/// Exact Git HEAD and cleanliness commitment for a frozen source worktree.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct GitWorktreeCommitment {
    canonical_root: PathBuf,
    head: String,
}

impl GitWorktreeCommitment {
    pub fn freeze(repository: &Path) -> Result<Self> {
        let canonical_root = repository
            .canonicalize()
            .with_context(|| format!("canonicalize worktree {}", repository.display()))?;
        let head = git_stdout(&canonical_root, &["rev-parse", "HEAD"])?;
        let status = git_stdout(
            &canonical_root,
            &["status", "--porcelain=v1", "--untracked-files=all"],
        )?;
        if !status.is_empty() {
            bail!("frozen worktree is not clean");
        }
        Ok(Self {
            canonical_root,
            head,
        })
    }

    pub fn verify(&self) -> Result<()> {
        let metadata =
            fs::symlink_metadata(&self.canonical_root).context("re-stat frozen worktree")?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            bail!("frozen worktree root type changed");
        }
        let head = git_stdout(&self.canonical_root, &["rev-parse", "HEAD"])?;
        if head != self.head {
            bail!("frozen worktree HEAD changed");
        }
        let status = git_stdout(
            &self.canonical_root,
            &["status", "--porcelain=v1", "--untracked-files=all"],
        )?;
        if !status.is_empty() {
            bail!("frozen worktree is not clean");
        }
        Ok(())
    }

    pub fn head(&self) -> &str {
        &self.head
    }
}

impl ArtifactCommitments {
    pub fn freeze(paths: BTreeMap<String, PathBuf>) -> Result<Self> {
        if paths.is_empty() {
            bail!("at least one artifact commitment is required");
        }
        let mut artifacts = BTreeMap::new();
        for (name, path) in paths {
            if name.is_empty() {
                bail!("artifact commitment name is empty");
            }
            let supplied_metadata = fs::symlink_metadata(&path)
                .with_context(|| format!("stat supplied {name} artifact {}", path.display()))?;
            if supplied_metadata.file_type().is_symlink() || has_multiple_links(&supplied_metadata)
            {
                bail!("{name} artifact path is a symlink or hardlink");
            }
            let canonical_path = path
                .canonicalize()
                .with_context(|| format!("canonicalize {name} artifact {}", path.display()))?;
            artifacts.insert(name, ArtifactCommitment::freeze(&canonical_path)?);
        }
        Ok(Self { artifacts })
    }

    fn from_references(references: &BTreeMap<String, FrozenArtifactReference>) -> Result<Self> {
        let mut artifacts = BTreeMap::new();
        for (name, reference) in references {
            let canonical_path = reference
                .path
                .canonicalize()
                .with_context(|| format!("canonicalize frozen {name} artifact"))?;
            if canonical_path != reference.path {
                bail!("frozen {name} artifact path is not canonical");
            }
            let artifact = ArtifactCommitment::freeze(&reference.path)
                .with_context(|| format!("open frozen {name} artifact"))?;
            if artifact.sha256 != reference.sha256 {
                bail!("frozen {name} artifact commitment mismatch");
            }
            artifacts.insert(name.clone(), artifact);
        }
        Ok(Self { artifacts })
    }

    pub fn verify(&self) -> Result<()> {
        for (name, frozen) in &self.artifacts {
            frozen
                .read_verified()
                .with_context(|| format!("re-read {name} artifact"))?;
        }
        Ok(())
    }

    pub fn read_verified(&self, name: &str) -> Result<Vec<u8>> {
        self.artifacts
            .get(name)
            .ok_or_else(|| anyhow!("missing frozen artifact {name}"))?
            .read_verified()
    }

    pub fn sha256(&self, name: &str) -> Option<&str> {
        self.artifacts
            .get(name)
            .map(|artifact| artifact.sha256.as_str())
    }
}

impl ArtifactCommitment {
    fn freeze(path: &Path) -> Result<Self> {
        let canonical_path = path
            .canonicalize()
            .with_context(|| format!("canonicalize artifact {}", path.display()))?;
        let handle = Arc::new(open_anchored_regular(&canonical_path)?);
        #[cfg(test)]
        observe_artifact_read(&canonical_path);
        let bytes = read_handle(&handle)?;
        Ok(Self {
            canonical_path,
            sha256: sha256(&bytes),
            handle,
        })
    }

    fn freeze_context(path: &Path) -> Result<(Self, Vec<u8>)> {
        let canonical_path = path
            .canonicalize()
            .with_context(|| format!("canonicalize frozen context {}", path.display()))?;
        if canonical_path != path {
            bail!("frozen context path is not canonical");
        }
        require_owner_only_file(&canonical_path)?;
        let path_metadata = fs::symlink_metadata(&canonical_path)?;
        let handle = Arc::new(open_anchored_regular(&canonical_path)?);
        if !same_file(&path_metadata, &handle.metadata()?) {
            bail!("frozen context identity changed while opening retained handle");
        }
        let bytes = read_context_handle_bounded(&handle)?;
        let commitment = Self {
            canonical_path,
            sha256: sha256(&bytes),
            handle,
        };
        Ok((commitment, bytes))
    }

    fn read_verified(&self) -> Result<Vec<u8>> {
        let current = open_anchored_regular(&self.canonical_path)?;
        if !same_file(&self.handle.metadata()?, &current.metadata()?) {
            bail!("artifact path identity changed after freeze");
        }
        #[cfg(test)]
        observe_artifact_read(&self.canonical_path);
        let bytes = read_handle(&self.handle)?;
        if sha256(&bytes) != self.sha256 {
            bail!("artifact bytes changed after freeze");
        }
        Ok(bytes)
    }

    fn verify_context_bytes(&self, expected: &[u8]) -> Result<()> {
        require_owner_only_file(&self.canonical_path)?;
        let current = open_anchored_regular(&self.canonical_path)?;
        if !same_file(&self.handle.metadata()?, &current.metadata()?) {
            bail!("frozen context path identity changed after verification");
        }
        let bytes = read_context_handle_bounded(&self.handle)?;
        if bytes != expected || sha256(&bytes) != self.sha256 {
            bail!("frozen context bytes changed after verification");
        }
        Ok(())
    }

    #[cfg(unix)]
    fn verified_exec_descriptor(&self) -> Result<crate::app_server::VerifiedExecutable> {
        self.read_verified()?;
        let read_descriptor = open_anchored_regular(&self.canonical_path)?;
        if !same_file(&self.handle.metadata()?, &read_descriptor.metadata()?) {
            bail!("verified executable path identity changed before launch");
        }
        let executable = crate::app_server::VerifiedExecutable::from_retained(
            read_descriptor,
            &self.canonical_path,
            self.sha256.clone(),
        )?;
        self.read_verified()?;
        Ok(executable)
    }

    #[cfg(not(unix))]
    fn verified_exec_descriptor(&self) -> Result<crate::app_server::VerifiedExecutable> {
        bail!("descriptor-backed executable launch requires Unix fexec semantics")
    }
}

#[cfg(unix)]
fn has_multiple_links(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;

    metadata.nlink() != 1
}

#[cfg(not(unix))]
fn has_multiple_links(_metadata: &fs::Metadata) -> bool {
    false
}

fn reject_secret_names(variables: &BTreeMap<String, String>) -> Result<()> {
    for name in variables.keys() {
        let upper = name.to_ascii_uppercase();
        if upper.contains("KEY")
            || upper.contains("TOKEN")
            || upper.contains("SECRET")
            || upper.contains("PROXY")
            || upper.starts_with("AWS_")
            || upper.starts_with("AZURE_")
            || upper.starts_with("GOOGLE_")
        {
            bail!("secret or proxy environment variable is forbidden: {name}");
        }
    }
    Ok(())
}

fn git_stdout(repository: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .output()
        .with_context(|| format!("run git in {}", repository.display()))?;
    if !output.status.success() {
        bail!("git command failed in frozen worktree");
    }
    String::from_utf8(output.stdout)
        .context("git output is not UTF-8")
        .map(|output| output.trim_end_matches(['\r', '\n']).to_string())
}

fn validate_leaf_name(name: &str) -> Result<()> {
    let path = Path::new(name);
    if name.is_empty()
        || path.components().count() != 1
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
    {
        bail!("Skill name must be one safe path component");
    }
    Ok(())
}

fn require_artifact_names<'a>(names: impl Iterator<Item = &'a str>) -> Result<()> {
    let names: std::collections::HashSet<&str> = names.collect();
    if REQUIRED_EXECUTION_ARTIFACTS
        .iter()
        .any(|required| !names.contains(required))
        || names.iter().any(|name| {
            !REQUIRED_EXECUTION_ARTIFACTS.contains(name) && !name.starts_with("material:")
        })
    {
        bail!("frozen execution artifact set is incomplete or has extras");
    }
    Ok(())
}

fn validate_frozen_context(context: &FrozenRunContext) -> Result<()> {
    if context.schema_version != 1
        || context.execution_mode != "live"
        || context.provider_mode != "not-run"
        || !is_lower_hex(&context.pair_id, 64)
        || !is_lower_hex(&context.public_run_id, 64)
        || !is_lower_hex(&context.commitment_key_sha256, 64)
        || context.candidate_sha.is_empty()
        || context.max_output_tokens == 0
        || context.max_attempts_per_arm == 0
        || context.max_total_tokens == 0
        || context.max_elapsed_seconds == 0
        || context.model_label.is_empty()
    {
        bail!("strict live frozen context has invalid required fields");
    }
    if context.supplier_statement_policy
        != (SupplierStatementPolicy {
            directory: "inputs/supplier-statements".to_string(),
            allowed_leaves: ["generic.json".to_string(), "candidate.json".to_string()],
            nested_entries_allowed: false,
        })
    {
        bail!("strict live frozen context has invalid supplier statement policy");
    }
    if context.candidate_sha != context.repo_head {
        bail!("candidate SHA must equal the frozen repository HEAD");
    }
    let expected_pair_id = sha256(
        format!("{}:{}", context.repo_head, context.model_label).as_bytes(),
    );
    if context.pair_id != expected_pair_id {
        bail!("strict live pair ID differs from its frozen repository and model inputs");
    }
    validate_local_mock_upstream(&context.provider_upstream_url)?;
    require_artifact_names(context.artifacts.keys().map(String::as_str))?;
    let current_exe = std::env::current_exe()
        .context("resolve current evaluator executable")?
        .canonicalize()
        .context("canonicalize current evaluator executable")?;
    let evaluator_matches = context.artifacts["evaluatorBinary"].path == current_exe;
    #[cfg(test)]
    let evaluator_matches = evaluator_matches
        || (context.artifacts["evaluatorBinary"].path
            == context
                .private_root
                .join("frozen-inputs/evaluator-binary-under-test")
            && context.artifacts["evaluatorBinary"].sha256 == sha256(&fs::read(&current_exe)?));
    if !evaluator_matches {
        bail!("frozen evaluator binary is not the running evaluator");
    }
    let expected_broker = context
        .repo_root
        .join("codex-rs/responses-api-proxy/src/broker.rs")
        .canonicalize()
        .context("canonicalize frozen broker source")?;
    if context.artifacts["brokerSource"].path != expected_broker {
        bail!("frozen broker source is not the committed broker component");
    }
    let canonical_repo = context
        .repo_root
        .canonicalize()
        .context("canonicalize frozen repository root")?;
    if canonical_repo != context.repo_root {
        bail!("frozen repository root path is not canonical");
    }
    let worktree = GitWorktreeCommitment::freeze(&context.repo_root)?;
    if worktree.head() != context.repo_head {
        bail!("frozen worktree HEAD does not match context");
    }
    let canonical_private_root = context
        .private_root
        .canonicalize()
        .context("canonicalize private root")?;
    if canonical_private_root != context.private_root {
        bail!("private root path must be canonical");
    }
    let private_metadata = fs::symlink_metadata(&context.private_root)?;
    if !private_metadata.is_dir() || private_metadata.file_type().is_symlink() {
        bail!("private root must be a non-symlink directory");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if private_metadata.permissions().mode() & 0o077 != 0 {
            bail!("private root must be owner-only");
        }
    }
    let frozen_inputs = context.private_root.join("frozen-inputs");
    require_owner_only_directory(&frozen_inputs)?;
    let expected_private_inputs = [
        ("skill", "lead-skill.md"),
        ("schema", "content-package-schema.json"),
        ("prompt", "root-prompt.txt"),
        ("additionalContext", "additional-context.txt"),
        ("threadStartRequest", "thread-start-request.json"),
        ("turnStartRequest", "turn-start-request.json"),
    ];
    for (name, leaf) in expected_private_inputs {
        let expected = frozen_inputs
            .join(leaf)
            .canonicalize()
            .with_context(|| format!("canonicalize private frozen input {name}"))?;
        if context.artifacts[name].path != expected {
            bail!("frozen {name} reference is outside its canonical private input slot");
        }
        require_owner_only_file(&expected)?;
    }
    let imported_inputs = context.private_root.join("inputs");
    let case_root = imported_inputs.join("case");
    require_owner_only_directory(&imported_inputs)?;
    require_owner_only_directory(&case_root)?;
    require_owner_only_directory(&imported_inputs.join("supplier-statements"))?;
    require_owner_only_directory(&context.private_root.join("coordinator/cost"))?;
    for (name, leaf) in [
        ("providerBudgetEvidence", "provider-budget-evidence.json"),
        ("rateCard", "rate-card.json"),
        ("billingPolicy", "billing-policy.json"),
        ("fxPolicy", "fx-policy.json"),
    ] {
        let expected = imported_inputs.join(leaf).canonicalize()?;
        if context.artifacts[name].path != expected {
            bail!("frozen {name} reference is outside its fixed private input slot");
        }
        require_owner_only_file(&expected)?;
    }
    for (name, path) in [
        ("source", case_root.join("case.json")),
        (
            "attestation",
            imported_inputs.join("held-out-attestation.json"),
        ),
        ("materials", imported_inputs.join("materials-manifest.json")),
    ] {
        let expected = path
            .canonicalize()
            .with_context(|| format!("canonicalize imported proof input {name}"))?;
        if context.artifacts[name].path != expected {
            bail!("frozen {name} reference is outside its canonical proof-copy slot");
        }
        require_owner_only_file(&expected)?;
    }
    let case_bytes = read_regular_file_no_follow(&context.artifacts["source"].path)?;
    let mission_case: codex_ai_ip_domain::HeldOutMissionCase =
        serde_json::from_slice(&case_bytes).context("parse frozen held-out case")?;
    mission_case
        .validate()
        .context("validate frozen held-out case")?;
    let expected_additional_context = codex_ai_ip_runtime::evaluation_context(&mission_case)?;
    if read_regular_file_no_follow(&context.artifacts["additionalContext"].path)?
        != expected_additional_context.as_bytes()
    {
        bail!("frozen additional context differs from the mission evaluation context");
    }
    let material_manifest_bytes = serde_json::to_vec(&mission_case.materials)?;
    if read_regular_file_no_follow(&context.artifacts["materials"].path)? != material_manifest_bytes
    {
        bail!("frozen material manifest differs from the held-out case declaration");
    }
    let attestation_bytes = read_regular_file_no_follow(&context.artifacts["attestation"].path)?;
    let attestation = FrozenContracts::load()?
        .validate_native_attestation(&attestation_bytes)
        .context("validate frozen complete native attestation")?;
    validate_native_attestation_source_binding(
        &attestation,
        &case_bytes,
        &material_manifest_bytes,
        &context.private_root,
    )?;
    validate_native_attestation_context_binding(&attestation, context)?;
    let declared_material_names: HashSet<String> = mission_case
        .materials
        .iter()
        .map(|material| format!("material:{}", material.material_id))
        .collect();
    let frozen_material_names: HashSet<String> = context
        .artifacts
        .keys()
        .filter(|name| name.starts_with("material:"))
        .cloned()
        .collect();
    if frozen_material_names != declared_material_names {
        bail!("frozen material artifact set differs from the held-out case declaration");
    }
    for material in &mission_case.materials {
        let name = format!("material:{}", material.material_id);
        let expected = case_root
            .join(&material.relative_path)
            .canonicalize()
            .with_context(|| format!("canonicalize managed material {name}"))?;
        if !expected.starts_with(&case_root)
            || context.artifacts[&name].path != expected
            || context.artifacts[&name].sha256 != material.sha256
        {
            bail!("frozen material commitment differs from the managed proof copy");
        }
        require_owner_only_file(&expected)?;
    }
    Ok(())
}

fn require_owner_only_file(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || has_multiple_links(&metadata)
    {
        bail!(
            "private input must be a single-link regular file: {}",
            path.display()
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            bail!("private input must be owner-only: {}", path.display());
        }
    }
    Ok(())
}

fn require_owner_only_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        bail!("private input root must be a non-symlink directory");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            bail!("private input root must be owner-only");
        }
    }
    Ok(())
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn collect_tree(root: &Path) -> Result<BTreeMap<String, Option<Vec<u8>>>> {
    fn walk(
        root: &Path,
        current: &Path,
        output: &mut BTreeMap<String, Option<Vec<u8>>>,
    ) -> Result<()> {
        for entry in fs::read_dir(current)
            .with_context(|| format!("read isolated Home directory {}", current.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            let relative = path
                .strip_prefix(root)
                .context("isolated Home entry escaped root")?
                .to_string_lossy()
                .replace('\\', "/");
            if metadata.file_type().is_symlink()
                || (metadata.is_file() && has_multiple_links(&metadata))
            {
                bail!("isolated Home contains a link: {relative}");
            }
            if metadata.is_dir() {
                output.insert(relative, None);
                walk(root, &path, output)?;
            } else if metadata.is_file() {
                output.insert(relative, Some(read_regular_file_no_follow(&path)?));
            } else {
                bail!("isolated Home contains a non-regular entry");
            }
        }
        Ok(())
    }

    let mut output = BTreeMap::new();
    walk(root, root, &mut output)?;
    Ok(output)
}

fn read_regular_file_no_follow(path: &Path) -> Result<Vec<u8>> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("canonicalize regular file {}", path.display()))?;
    read_handle(&open_anchored_regular(&canonical)?)
}

#[cfg(unix)]
pub(crate) fn read_private_existing_no_follow(path: &Path) -> Result<Vec<u8>> {
    read_handle(&open_anchored_regular(path)?)
}
#[cfg(unix)]
pub(crate) fn validate_private_existing_directory_no_follow(path: &Path) -> Result<()> {
    open_anchored(path, true).map(drop)
}
#[cfg(any(windows, test))]
fn validate_windows_stream_snapshot(
    buffer: &[u8],
    name_offset: usize,
    name_bytes: usize,
    next_offset: u32,
    stream_size: i64,
    directory: bool,
) -> Result<()> {
    const DEFAULT: &[u16] = &[58, 58, 36, 68, 65, 84, 65];
    let end = name_offset
        .checked_add(name_bytes)
        .context("Windows stream name bounds overflow")?;
    if !name_bytes.is_multiple_of(2) || end > buffer.len() || next_offset != 0 {
        bail!("private entry has malformed or multiple data streams");
    }
    let name = buffer[name_offset..end]
        .chunks_exact(2)
        .map(|unit| u16::from_le_bytes([unit[0], unit[1]]))
        .collect::<Vec<_>>();
    if name != DEFAULT || directory && stream_size != 0 {
        bail!("private entry has a named or disallowed data stream");
    }
    Ok(())
}
#[cfg(test)]
pub(crate) fn validate_windows_stream_snapshot_for_test(
    buffer: &[u8],
    name_offset: usize,
    name_bytes: usize,
    next_offset: u32,
    stream_size: i64,
    directory: bool,
) -> Result<()> {
    validate_windows_stream_snapshot(
        buffer,
        name_offset,
        name_bytes,
        next_offset,
        stream_size,
        directory,
    )
}
#[cfg(windows)]
pub(crate) mod private_existing_windows {
    use super::*;
    use std::io::Read;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::Storage::FileSystem::*;
    fn streams(file: &File, directory: bool) -> Result<()> {
        let mut words = [0u64; 1024];
        if unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle() as HANDLE,
                FileStreamInfo,
                words.as_mut_ptr().cast(),
                u32::try_from(std::mem::size_of_val(&words))?,
            )
        } == 0
        {
            let code = unsafe { GetLastError() };
            if directory && code == ERROR_HANDLE_EOF {
                return Ok(());
            }
            return Err(std::io::Error::from_raw_os_error(i32::try_from(code)?))
                .context("enumerate private tree data streams");
        }
        let entry = unsafe { &*words.as_ptr().cast::<FILE_STREAM_INFO>() };
        let buffer = unsafe {
            std::slice::from_raw_parts(words.as_ptr().cast::<u8>(), std::mem::size_of_val(&words))
        };
        validate_windows_stream_snapshot(
            buffer,
            std::mem::offset_of!(FILE_STREAM_INFO, StreamName),
            usize::try_from(entry.StreamNameLength)?,
            entry.NextEntryOffset,
            entry.StreamSize,
            directory,
        )
    }
    pub(crate) fn info(file: &File, directory: bool) -> Result<BY_HANDLE_FILE_INFORMATION> {
        let mut info = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &mut info) } == 0
            || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory
            || !directory && info.nNumberOfLinks != 1
        {
            bail!("private tree entry is a reparse point, hardlink, or wrong type");
        }
        streams(file, directory)?;
        Ok(info)
    }
    pub(crate) fn identity(value: &BY_HANDLE_FILE_INFORMATION) -> u128 {
        (u128::from(value.dwVolumeSerialNumber) << 64)
            | (u128::from(value.nFileIndexHigh) << 32)
            | u128::from(value.nFileIndexLow)
    }
    pub(crate) fn open(
        path: &Path,
        directory: bool,
        access: u32,
    ) -> Result<(File, BY_HANDLE_FILE_INFORMATION)> {
        if !path.is_absolute() || path.components().any(|component| {
            matches!(component, std::path::Component::Normal(name) if name.to_string_lossy().contains(':'))
                || matches!(component, std::path::Component::CurDir | std::path::Component::ParentDir)
        }) { bail!("private tree path is not normalized"); }
        let mut options = OpenOptions::new();
        options
            .access_mode(access)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS);
        let file = options.open(path)?;
        let info = info(&file, directory)?;
        Ok((file, info))
    }
    pub(crate) fn validate_directory(path: &Path) -> Result<()> {
        open(path, true, FILE_READ_ATTRIBUTES).map(drop)
    }
    pub(crate) fn read(path: &Path) -> Result<Vec<u8>> {
        let (mut file, before) = open(path, false, FILE_GENERIC_READ)?;
        let before_len = file.metadata()?.len();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let after = info(&file, false)?;
        if identity(&before) != identity(&after)
            || before_len != file.metadata()?.len()
            || before_len != u64::try_from(bytes.len())?
        {
            bail!("private tree file changed during retained read");
        }
        Ok(bytes)
    }
}
#[cfg(windows)]
pub(crate) use private_existing_windows::read as read_private_existing_no_follow;
#[cfg(windows)]
pub(crate) use private_existing_windows::validate_directory as validate_private_existing_directory_no_follow;
#[cfg(unix)]
fn open_anchored(path: &Path, directory: bool) -> Result<File> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::fd::FromRawFd;
    use std::path::Component;

    if !path.is_absolute() {
        bail!("anchored artifact path must be absolute");
    }
    let root = CString::new("/")?;
    let root_fd = unsafe {
        libc::open(
            root.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if root_fd < 0 {
        return Err(std::io::Error::last_os_error()).context("open filesystem root");
    }
    let mut current = unsafe { File::from_raw_fd(root_fd) };
    let components: Vec<_> = path.components().collect();
    let normal_count = components
        .iter()
        .filter(|component| matches!(component, Component::Normal(_)))
        .count();
    let mut seen = 0_usize;
    for component in components {
        match component {
            Component::RootDir => continue,
            Component::Normal(name) => {
                seen += 1;
                let name = CString::new(name.as_encoded_bytes())?;
                let final_component = seen == normal_count;
                let flags = libc::O_RDONLY
                    | libc::O_CLOEXEC
                    | libc::O_NOFOLLOW
                    | libc::O_NONBLOCK
                    | if !final_component || directory {
                        libc::O_DIRECTORY
                    } else {
                        0
                    };
                let fd = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(), flags) };
                if fd < 0 {
                    return Err(std::io::Error::last_os_error())
                        .with_context(|| format!("open anchored component in {}", path.display()));
                }
                current = unsafe { File::from_raw_fd(fd) };
            }
            _ => bail!("artifact path contains unsupported components"),
        }
    }
    let metadata = current.metadata()?;
    if directory && !metadata.is_dir()
        || !directory && (!metadata.is_file() || has_multiple_links(&metadata))
    {
        bail!("anchored artifact has the wrong type or multiple links");
    }
    Ok(current)
}

#[cfg(unix)]
fn open_anchored_regular(path: &Path) -> Result<File> {
    open_anchored(path, false)
}
#[cfg(not(unix))]
fn open_anchored_regular(_path: &Path) -> Result<File> {
    bail!("live artifact access requires descriptor-anchored Unix openat support")
}

#[cfg(unix)]
fn read_handle(file: &File) -> Result<Vec<u8>> {
    use std::os::unix::fs::FileExt;

    let before = file.metadata()?;
    let mut bytes = Vec::new();
    let mut offset = 0_u64;
    loop {
        let mut chunk = [0_u8; 8192];
        let read = file.read_at(&mut chunk, offset)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
        offset = offset
            .checked_add(u64::try_from(read)?)
            .context("artifact length overflow")?;
    }
    let after = file.metadata()?;
    if !same_file(&before, &after) || before.len() != after.len() || after.len() != offset {
        bail!("artifact changed while reading verified handle");
    }
    Ok(bytes)
}

#[cfg(unix)]
fn read_context_handle_bounded(file: &File) -> Result<Vec<u8>> {
    use std::os::unix::fs::FileExt;

    let before = file.metadata()?;
    if before.len() > FROZEN_CONTEXT_CAP {
        bail!("frozen context exceeds its byte cap");
    }
    let mut bytes = vec![0_u8; usize::try_from(before.len())?];
    file.read_exact_at(&mut bytes, 0)?;
    let mut eof_probe = [0_u8; 1];
    if file.read_at(&mut eof_probe, before.len())? != 0 {
        bail!("frozen context grew during bounded retained read");
    }
    let after = file.metadata()?;
    if !same_file(&before, &after) || before.len() != after.len() || has_multiple_links(&after) {
        bail!("frozen context changed during bounded retained read");
    }
    Ok(bytes)
}

#[cfg(not(unix))]
fn read_handle(_file: &File) -> Result<Vec<u8>> {
    bail!("live artifact access requires descriptor-anchored Unix reads")
}

#[cfg(not(unix))]
fn read_context_handle_bounded(_file: &File) -> Result<Vec<u8>> {
    bail!("live artifact access requires descriptor-anchored Unix reads")
}

#[cfg(unix)]
fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(not(unix))]
fn same_file(_left: &fs::Metadata, _right: &fs::Metadata) -> bool {
    false
}

#[cfg(target_os = "macos")]
fn open_anchored_directory(path: &Path) -> Result<File> {
    use std::ffi::CString;
    use std::os::fd::FromRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Component;

    if !path.is_absolute() {
        bail!("anchored managed directory path must be absolute");
    }
    let root = CString::new("/")?;
    let root_fd = unsafe {
        libc::open(
            root.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if root_fd < 0 {
        return Err(std::io::Error::last_os_error()).context("open filesystem root");
    }
    let mut current = unsafe { File::from_raw_fd(root_fd) };
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => {
                let name = CString::new(name.as_bytes())?;
                let fd = unsafe {
                    libc::openat(
                        std::os::fd::AsRawFd::as_raw_fd(&current),
                        name.as_ptr(),
                        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                    )
                };
                if fd < 0 {
                    return Err(std::io::Error::last_os_error()).with_context(|| {
                        format!("open retained managed directory {}", path.display())
                    });
                }
                current = unsafe { File::from_raw_fd(fd) };
            }
            _ => bail!("managed directory path is not normalized"),
        }
    }
    require_owner_only_directory_handle(&current)?;
    Ok(current)
}

#[cfg(not(target_os = "macos"))]
fn open_anchored_directory(_path: &Path) -> Result<File> {
    bail!("descriptor-relative managed input trees require Darwin openat semantics")
}

#[cfg(target_os = "macos")]
fn create_fresh_directory_at(parent: &File, name: &OsStr) -> Result<File> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::fd::FromRawFd;
    use std::os::unix::ffi::OsStrExt;

    let name = CString::new(name.as_bytes())?;
    if unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
        return Err(std::io::Error::last_os_error()).context("mkdirat managed directory");
    }
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error()).context("open fresh managed directory");
    }
    let directory = unsafe { File::from_raw_fd(fd) };
    if unsafe { libc::fchmod(directory.as_raw_fd(), 0o700) } != 0 {
        return Err(std::io::Error::last_os_error()).context("chmod managed directory");
    }
    require_owner_only_directory_handle(&directory)?;
    Ok(directory)
}

#[cfg(not(target_os = "macos"))]
fn create_fresh_directory_at(_parent: &File, _name: &OsStr) -> Result<File> {
    bail!("descriptor-relative managed input creation is unsupported on this platform")
}

#[cfg(target_os = "macos")]
fn open_directory_at(parent: &File, name: &OsStr) -> Result<File> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::fd::FromRawFd;
    use std::os::unix::ffi::OsStrExt;

    let name = CString::new(name.as_bytes())?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error()).context("open retained managed subdirectory");
    }
    let directory = unsafe { File::from_raw_fd(fd) };
    require_owner_only_directory_handle(&directory)?;
    Ok(directory)
}

#[cfg(not(target_os = "macos"))]
fn open_directory_at(_parent: &File, _name: &OsStr) -> Result<File> {
    bail!("descriptor-relative managed input validation is unsupported on this platform")
}

#[cfg(target_os = "macos")]
fn create_owner_only_file_at(parent: &File, name: &OsStr, bytes: &[u8]) -> Result<()> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::fd::FromRawFd;
    use std::os::unix::ffi::OsStrExt;

    let name = CString::new(name.as_bytes())?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error()).context("create managed input file");
    }
    let mut file = unsafe { File::from_raw_fd(fd) };
    if unsafe { libc::fchmod(file.as_raw_fd(), 0o600) } != 0 {
        return Err(std::io::Error::last_os_error()).context("chmod managed input file");
    }
    file.write_all(bytes).context("write managed input file")?;
    file.sync_all().context("fsync managed input file")
}

#[cfg(not(target_os = "macos"))]
fn create_owner_only_file_at(_parent: &File, _name: &OsStr, _bytes: &[u8]) -> Result<()> {
    bail!("descriptor-relative managed input creation is unsupported on this platform")
}

#[cfg(target_os = "macos")]
fn require_exact_file_at(parent: &File, name: &OsStr, expected: &[u8]) -> Result<()> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::fd::FromRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;

    let name = CString::new(name.as_bytes())?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error()).context("open retained managed input file");
    }
    let file = unsafe { File::from_raw_fd(fd) };
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || has_multiple_links(&metadata)
        || metadata.permissions().mode() & 0o077 != 0
        || read_handle(&file)? != expected
    {
        bail!("managed input file type, links, mode, or bytes differ");
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn require_exact_file_at(_parent: &File, _name: &OsStr, _expected: &[u8]) -> Result<()> {
    bail!("descriptor-relative managed input validation is unsupported on this platform")
}

#[cfg(target_os = "macos")]
fn require_owner_only_directory_handle(directory: &File) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = directory.metadata()?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        bail!("managed directory is not an owner-only directory");
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn require_owner_only_directory_handle(_directory: &File) -> Result<()> {
    bail!("descriptor-relative managed input validation is unsupported on this platform")
}

#[cfg(target_os = "macos")]
fn list_directory_entries(directory: &File) -> Result<BTreeSet<std::ffi::OsString>> {
    use std::ffi::CStr;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStringExt;

    let duplicate = unsafe { libc::fcntl(directory.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
    if duplicate < 0 {
        return Err(std::io::Error::last_os_error()).context("duplicate managed directory handle");
    }
    let stream = unsafe { libc::fdopendir(duplicate) };
    if stream.is_null() {
        unsafe { libc::close(duplicate) };
        return Err(std::io::Error::last_os_error()).context("open managed directory stream");
    }
    let mut entries = BTreeSet::new();
    loop {
        unsafe { *libc::__error() = 0 };
        let entry = unsafe { libc::readdir(stream) };
        if entry.is_null() {
            let error = std::io::Error::last_os_error();
            unsafe { libc::closedir(stream) };
            if error.raw_os_error().unwrap_or(0) != 0 {
                return Err(error).context("read managed directory entries");
            }
            break;
        }
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
        if name != b"." && name != b".." {
            entries.insert(std::ffi::OsString::from_vec(name.to_vec()));
        }
    }
    Ok(entries)
}

#[cfg(not(target_os = "macos"))]
fn list_directory_entries(_directory: &File) -> Result<BTreeSet<std::ffi::OsString>> {
    bail!("descriptor-relative managed input validation is unsupported on this platform")
}

fn require_exact_directory_entries<I, S>(directory: &File, expected: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let expected = expected
        .into_iter()
        .map(|name| name.as_ref().to_os_string())
        .collect::<BTreeSet<_>>();
    if list_directory_entries(directory)? != expected {
        bail!("managed input directory contains missing or undeclared entries");
    }
    Ok(())
}

fn reverify_directory_identity(path: &Path, retained: &File) -> Result<()> {
    let current = open_anchored_directory(path)?;
    if !same_file(&retained.metadata()?, &current.metadata()?) {
        bail!("retained private directory identity changed");
    }
    require_owner_only_directory_handle(retained)?;
    require_owner_only_directory_handle(&current)
}

fn reverify_child_directory_identity(parent: &File, name: &OsStr, retained: &File) -> Result<()> {
    let current = open_directory_at(parent, name)?;
    if !same_file(&retained.metadata()?, &current.metadata()?) {
        bail!("retained private child directory identity changed");
    }
    require_owner_only_directory_handle(retained)?;
    require_owner_only_directory_handle(&current)
}

fn create_owner_only_dir(path: &Path) -> Result<()> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path)
            .with_context(|| format!("stat directory {}", path.display()))?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            bail!("path is not a non-symlink directory: {}", path.display());
        }
    } else {
        fs::create_dir(path).with_context(|| format!("create directory {}", path.display()))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("set owner-only permissions on {}", path.display()))?;
    }
    Ok(())
}

fn write_owner_only_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("create private file {}", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("write private file {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("fsync private file {}", path.display()))
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
