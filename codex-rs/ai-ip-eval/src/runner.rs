use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

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

use crate::ArmActivation;
use crate::BrokerGateConfig;
use crate::BrokerRuntimeConfig;
use crate::EvaluationCondition;
use crate::PairCoordinator;
use crate::model::LiveFreezeArgs;
use crate::model::ReplayFreezeArgs;

/// A frozen context whose current bytes were re-read and hashed successfully.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct VerifiedFrozenContext {
    canonical_path: PathBuf,
    sha256: String,
    context: FrozenRunContext,
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
    candidate_sha: String,
    private_root: PathBuf,
    repo_root: PathBuf,
    repo_head: String,
    provider_upstream_url: String,
    max_output_tokens: u64,
    max_attempts_per_arm: u64,
    artifacts: BTreeMap<String, FrozenArtifactReference>,
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
        FrozenExecutionGuard::create(
            self.context
                .artifacts
                .iter()
                .map(|(name, artifact)| (name.clone(), artifact.path.clone()))
                .collect(),
            &self.context.repo_root,
        )
    }

    fn artifact_bytes(&self, name: &str) -> Result<Vec<u8>> {
        let artifact = self
            .context
            .artifacts
            .get(name)
            .ok_or_else(|| anyhow!("missing frozen artifact {name}"))?;
        read_regular_file_no_follow(&artifact.path)
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ReplayFrozenContext {
    schema_version: u32,
    execution_mode: &'static str,
    provider_mode: &'static str,
    repo_root: PathBuf,
    fork_sha: String,
    private_root: PathBuf,
    codex_binary_sha256: String,
    case_sha256: String,
    transcript_sha256: String,
    fixture_set_manifest_sha256: String,
}

/// Writes a typed replay freeze record with no provider-capable fields.
pub fn freeze_replay_context(args: ReplayFreezeArgs) -> Result<()> {
    let record = ReplayFrozenContext {
        schema_version: 1,
        execution_mode: "replay",
        provider_mode: "not-run",
        repo_root: args
            .repo_root
            .canonicalize()
            .context("canonicalize replay repo")?,
        fork_sha: args.fork_sha,
        private_root: args.private_root,
        codex_binary_sha256: sha256(&read_regular_file_no_follow(&args.codex_bin)?),
        case_sha256: sha256(&read_regular_file_no_follow(&args.case)?),
        transcript_sha256: sha256(&read_regular_file_no_follow(&args.transcript)?),
        fixture_set_manifest_sha256: sha256(&read_regular_file_no_follow(
            &args.fixture_set_manifest,
        )?),
    };
    write_owner_only_new(&args.output, &serde_json::to_vec_pretty(&record)?)
}

/// Freezes the strict local-mock live context consumed by [`run_local_mock_pair`].
pub fn freeze_live_context(args: LiveFreezeArgs) -> Result<()> {
    let upstream = reqwest::Url::parse(&args.provider_upstream_url)
        .context("parse local mock upstream URL")?;
    if upstream.scheme() != "http" || !upstream.host_str().is_some_and(is_loopback_host) {
        bail!("Phase 0A live freeze accepts only an HTTP loopback mock upstream");
    }
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
    let named = BTreeMap::from([
        ("source".to_string(), args.case),
        ("materials".to_string(), args.attestation),
        ("codexBinary".to_string(), args.codex_bin.clone()),
        ("evaluatorBinary".to_string(), args.codex_bin),
        (
            "brokerSource".to_string(),
            args.provider_budget_evidence.clone(),
        ),
        ("config".to_string(), args.rate_card),
        ("schema".to_string(), args.billing_policy),
        ("prompt".to_string(), args.fx_policy),
        ("skill".to_string(), args.lead_skill),
        (
            "threadStartRequest".to_string(),
            args.provider_budget_evidence.clone(),
        ),
        (
            "turnStartRequest".to_string(),
            args.provider_budget_evidence.clone(),
        ),
        ("receipt".to_string(), args.provider_budget_evidence),
    ]);
    let frozen_artifacts = ArtifactCommitments::freeze(named)?;
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
    let pair_material = format!("{}:{}", args.fork_sha, args.model_label);
    let context = FrozenRunContext {
        schema_version: 1,
        execution_mode: "live".to_string(),
        provider_mode: "not-run".to_string(),
        pair_id: sha256(pair_material.as_bytes()),
        public_run_id: sha256(args.provider_label.as_bytes()),
        candidate_sha: args.fork_sha.clone(),
        private_root: args
            .private_root
            .canonicalize()
            .context("canonicalize private root")?,
        repo_root,
        repo_head: args.fork_sha,
        provider_upstream_url: args.provider_upstream_url,
        max_output_tokens: args.max_output_tokens_per_request,
        max_attempts_per_arm: args.max_provider_request_attempts_per_run,
        artifacts,
    };
    validate_frozen_context(&context)?;
    write_owner_only_new(&args.output, &serde_json::to_vec_pretty(&context)?)
}

struct RuntimeInspector;

impl codex_responses_api_proxy::RequestInspector for RuntimeInspector {
    fn inspect(
        &self,
        body: &serde_json::Value,
    ) -> Result<codex_responses_api_proxy::TransformedRequestEvidence> {
        let digest: [u8; 32] = Sha256::digest(serde_json::to_vec(body)?).into();
        Ok(codex_responses_api_proxy::TransformedRequestEvidence {
            raw_sha256: digest,
            normalized_sha256: digest,
            normalized_base_commitment: digest,
            treatment_diff_commitment: None,
        })
    }
}

/// Executes exactly two arms against the frozen loopback mock upstream.
pub fn run_local_mock_pair(path: &Path) -> Result<()> {
    use std::sync::Arc;
    use std::time::Duration;
    use std::time::Instant;

    let frozen = verify_frozen_context(path)?;
    let mut guard = frozen.execution_guard()?;
    guard.advance(ExecutionBoundary::ContextFrozen)?;
    let coordinator_dir = frozen.context.private_root.join("coordinator");
    let order = commit_arm_order(&frozen, &coordinator_dir)?;
    guard.advance(ExecutionBoundary::OrderCommitted)?;
    let execution_bytes = serde_json::to_vec(&serde_json::json!({
        "frozenRunContextSha256": frozen.sha256(),
        "armOrderCommitment": order.seed_commitment(),
    }))?;
    let execution_context_sha256 = sha256(&execution_bytes);
    write_owner_only_new(
        &coordinator_dir.join("execution-context.json"),
        &execution_bytes,
    )?;

    let config_bytes = frozen.artifact_bytes("config")?;
    let skill_bytes = frozen.artifact_bytes("skill")?;
    let homes = prepare_isolated_homes(
        &frozen.context.private_root,
        &config_bytes,
        "candidate-skill",
        &skill_bytes,
    )?;
    verify_isolated_home_parity(&homes, "candidate-skill", &skill_bytes)?;
    let runtime = Arc::new(BrokerRuntimeConfig::new(
        frozen.max_attempts_per_arm(),
        frozen.max_output_tokens(),
        1024 * 1024,
    )?);
    let first = order.first();
    let second = order.second();
    let gate = Arc::new(PairCoordinator::create(BrokerGateConfig {
        ledger_path: coordinator_dir.join("attempt-index.jsonl"),
        receipt_dir: coordinator_dir.join("receipts"),
        pair_id: frozen.pair_id().to_string(),
        frozen_run_context_sha256: frozen.sha256().to_string(),
        execution_context_sha256,
        arm_order_commitment: order.seed_commitment().to_string(),
        runtime: runtime.clone(),
    })?);
    gate.commit_order(order)?;
    let proxy_config = codex_responses_api_proxy::ProxyConfig {
        listen_port: None,
        upstream_url: reqwest::Url::parse(frozen.provider_upstream_url())?,
        dump_dir: None,
        http_shutdown: false,
        default_request_timeout: None,
        request_transform: Some(runtime.request_transform(Arc::new(RuntimeInspector))),
    };
    let bound = codex_responses_api_proxy::bind(&proxy_config)?;
    let proxy = codex_responses_api_proxy::activate(
        bound,
        proxy_config,
        codex_responses_api_proxy::local_mock_auth_header(),
        gate.clone(),
        gate.clone(),
    )?;
    let client = reqwest::blocking::Client::builder().no_proxy().build()?;
    let proxy_url = format!("http://{}/v1/responses", proxy.addr());
    let run_result = (|| -> Result<()> {
        for (index, condition) in [(1_u8, first), (2_u8, second)] {
            guard.advance(if index == 1 {
                ExecutionBoundary::Arm1Pre
            } else {
                ExecutionBoundary::Arm2Pre
            })?;
            let root = codex_protocol::ThreadId::new().to_string();
            gate.activate_arm(ArmActivation {
                run_ordinal: index,
                condition,
                root_thread_id: root.clone(),
                deadline: Instant::now() + Duration::from_secs(5),
                deadline_rfc3339: chrono::Utc::now().to_rfc3339(),
            })?;
            let response = client
                .post(&proxy_url)
                .header("x-codex-window-id", format!("{root}:0"))
                .json(&serde_json::json!({"model":"local-mock","input":[]}))
                .send()?;
            if !response.status().is_success() {
                bail!("local mock arm request failed with {}", response.status());
            }
            let _ = response.bytes()?;
            gate.seal_arm()?;
            guard.advance(if index == 1 {
                ExecutionBoundary::Arm1Post
            } else {
                ExecutionBoundary::Arm2Post
            })?;
        }
        gate.finish()?;
        guard.advance(ExecutionBoundary::Finished)
    })();
    let shutdown_result = proxy.shutdown_with_timeout(Duration::from_secs(2));
    run_result?;
    shutdown_result
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
    let bytes = read_regular_file_no_follow(&canonical_path)?;
    let context: FrozenRunContext =
        serde_json::from_slice(&bytes).context("parse strict frozen context JSON")?;
    validate_frozen_context(&context)?;
    Ok(VerifiedFrozenContext {
        canonical_path,
        sha256: sha256(&bytes),
        context,
    })
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
    let revalidated = verify_frozen_context(frozen.canonical_path())?;
    if revalidated.sha256() != frozen.sha256() {
        bail!("frozen run context changed before arm-order commitment");
    }
    create_owner_only_dir(coordinator_directory)?;
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

#[derive(Debug, Clone, Eq, PartialEq)]
struct ArtifactCommitment {
    canonical_path: PathBuf,
    sha256: String,
}

/// Named byte commitments revalidated at coordinator boundaries.
///
/// Callers use explicit semantic names (source, materials, binaries, config,
/// Schema, prompt, Skill, request projections, receipts) so an error identifies
/// the exact mutable input. Verification always re-opens each canonical path.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ArtifactCommitments {
    artifacts: BTreeMap<String, ArtifactCommitment>,
}

pub const REQUIRED_EXECUTION_ARTIFACTS: &[&str] = &[
    "source",
    "materials",
    "codexBinary",
    "evaluatorBinary",
    "brokerSource",
    "config",
    "schema",
    "prompt",
    "skill",
    "threadStartRequest",
    "turnStartRequest",
    "receipt",
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
            let metadata = fs::symlink_metadata(&canonical_path)
                .with_context(|| format!("stat {name} artifact"))?;
            if !metadata.file_type().is_file()
                || metadata.file_type().is_symlink()
                || has_multiple_links(&metadata)
            {
                bail!("{name} artifact is not a regular non-symlink file");
            }
            let bytes = read_regular_file_no_follow(&canonical_path)
                .with_context(|| format!("read {name} artifact"))?;
            artifacts.insert(
                name,
                ArtifactCommitment {
                    canonical_path,
                    sha256: sha256(&bytes),
                },
            );
        }
        Ok(Self { artifacts })
    }

    pub fn verify(&self) -> Result<()> {
        for (name, frozen) in &self.artifacts {
            let metadata = fs::symlink_metadata(&frozen.canonical_path)
                .with_context(|| format!("re-stat {name} artifact"))?;
            if !metadata.file_type().is_file()
                || metadata.file_type().is_symlink()
                || has_multiple_links(&metadata)
            {
                bail!("{name} artifact link/type changed after freeze");
            }
            let bytes = read_regular_file_no_follow(&frozen.canonical_path)
                .with_context(|| format!("re-read {name} artifact"))?;
            let actual = sha256(&bytes);
            if actual != frozen.sha256 {
                bail!("{name} artifact changed after freeze");
            }
        }
        Ok(())
    }

    pub fn sha256(&self, name: &str) -> Option<&str> {
        self.artifacts
            .get(name)
            .map(|artifact| artifact.sha256.as_str())
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
    if names.len() != REQUIRED_EXECUTION_ARTIFACTS.len()
        || REQUIRED_EXECUTION_ARTIFACTS
            .iter()
            .any(|required| !names.contains(required))
    {
        bail!("frozen execution artifact set is incomplete or has extras");
    }
    Ok(())
}

fn validate_frozen_context(context: &FrozenRunContext) -> Result<()> {
    if context.schema_version != 1
        || context.execution_mode != "live"
        || context.provider_mode != "not-run"
        || context.pair_id.is_empty()
        || context.public_run_id.is_empty()
        || context.candidate_sha.is_empty()
        || context.max_output_tokens == 0
        || context.max_attempts_per_arm == 0
    {
        bail!("strict live frozen context has invalid required fields");
    }
    require_artifact_names(context.artifacts.keys().map(String::as_str))?;
    for (name, artifact) in &context.artifacts {
        let bytes = read_regular_file_no_follow(&artifact.path)
            .with_context(|| format!("read frozen {name} artifact"))?;
        if sha256(&bytes) != artifact.sha256 {
            bail!("frozen {name} artifact commitment mismatch");
        }
    }
    let worktree = GitWorktreeCommitment::freeze(&context.repo_root)?;
    if worktree.head() != context.repo_head {
        bail!("frozen worktree HEAD does not match context");
    }
    if !context.private_root.is_absolute() {
        bail!("private root must be absolute");
    }
    Ok(())
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
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(path).with_context(|| {
        format!(
            "open regular file without following links {}",
            path.display()
        )
    })?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || has_multiple_links(&metadata) {
        bail!("file is not a single-link regular file: {}", path.display());
    }
    let mut bytes = Vec::new();
    use std::io::Read;
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
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
