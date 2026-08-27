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
use sha2::Digest;
use sha2::Sha256;

use crate::EvaluationCondition;

/// A frozen context whose current bytes were re-read and hashed successfully.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct VerifiedFrozenContext {
    pub canonical_path: PathBuf,
    pub sha256: String,
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
    let bytes = fs::read(&canonical_path)
        .with_context(|| format!("read frozen context {}", canonical_path.display()))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).context("parse frozen context JSON")?;
    if value
        .get("executionMode")
        .and_then(serde_json::Value::as_str)
        != Some("live")
    {
        bail!("arm order requires a frozen live context");
    }
    Ok(VerifiedFrozenContext {
        canonical_path,
        sha256: sha256(&bytes),
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
        frozen_run_context_sha256: frozen.sha256.clone(),
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
            let bytes =
                fs::read(&canonical_path).with_context(|| format!("read {name} artifact"))?;
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
            let bytes = fs::read(&frozen.canonical_path)
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
