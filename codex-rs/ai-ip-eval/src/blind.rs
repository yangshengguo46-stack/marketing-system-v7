use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use sha2::Digest;
use sha2::Sha256;

use crate::ExecutionMode;
use crate::model::BlindPackArgs;
use crate::secure_fs::read_single_link_regular;
use crate::secure_fs::resolve_private_relative;

const REVIEWER_ROOT: &str = "reviewer";
const MAPPING_DIR: &str = "coordinator/mappings";
const SEED_DIR: &str = "coordinator/blind-seeds";
const REVIEWS_DIR: &str = "reviews";
const RECEIPT: &str = "coordinator/blind-pack-receipt.json";

/// Task 5A stops here until the complete pair verifier is installed.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct BlindPairVerifierStageNotInstalled;

impl fmt::Display for BlindPairVerifierStageNotInstalled {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BlindPairVerifierStageNotInstalled")
    }
}

impl std::error::Error for BlindPairVerifierStageNotInstalled {}

pub(crate) fn run_blind_pack(args: BlindPackArgs) -> Result<()> {
    validate_destination_arguments(&args)?;
    let snapshot = read_context_snapshot(&args.frozen_run_context)?;
    validate_seed_arguments(&args, snapshot.execution_mode)?;
    ensure_destinations_absent(&snapshot.private_root)?;
    verify_blind_pair_stage(&args, &snapshot)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlindContextHeader {
    execution_mode: ExecutionMode,
    private_root: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct FrozenContextSnapshot {
    canonical_path: PathBuf,
    raw_bytes: Vec<u8>,
    sha256: String,
    execution_mode: ExecutionMode,
    private_root: PathBuf,
}

impl FrozenContextSnapshot {
    pub(crate) fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }

    pub(crate) fn raw_bytes(&self) -> &[u8] {
        &self.raw_bytes
    }

    pub(crate) fn sha256(&self) -> &str {
        &self.sha256
    }
}

pub(crate) fn read_context_snapshot(path: &Path) -> Result<FrozenContextSnapshot> {
    let canonical = require_exact_canonical(
        path,
        "blind-pack frozen context must be an absolute canonical path",
    )?;
    let bytes =
        read_single_link_regular(&canonical).context("read retained blind-pack frozen context")?;
    let context: BlindContextHeader =
        serde_json::from_slice(&bytes).context("parse blind-pack context header")?;
    let canonical_private_root = require_exact_canonical(
        &context.private_root,
        "blind-pack frozen context is outside its declared private root",
    )?;
    if canonical_private_root
        .join("frozen-run-context.json")
        .as_os_str()
        != canonical.as_os_str()
    {
        bail!("blind-pack frozen context is outside its declared private root");
    }
    Ok(FrozenContextSnapshot {
        canonical_path: canonical,
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        raw_bytes: bytes,
        execution_mode: context.execution_mode,
        private_root: context.private_root,
    })
}

/// Task 5C must consume `snapshot.raw_bytes` directly. If a later verifier
/// re-reads the path, it must fail unless the new raw-byte SHA equals this
/// retained snapshot's SHA before returning a verified pair.
pub(crate) fn verify_blind_pair_stage(
    args: &BlindPackArgs,
    snapshot: &FrozenContextSnapshot,
) -> Result<()> {
    if args.frozen_run_context.as_os_str() != snapshot.canonical_path().as_os_str()
        || format!("{:x}", Sha256::digest(snapshot.raw_bytes())) != snapshot.sha256()
    {
        bail!("blind-pack frozen context snapshot changed before pair verification");
    }
    Err(BlindPairVerifierStageNotInstalled.into())
}

fn require_exact_canonical(path: &Path, error: &str) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("{error}");
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| anyhow::anyhow!("{error}"))?;
    if canonical.as_os_str() != path.as_os_str() {
        bail!("{error}");
    }
    Ok(canonical)
}

fn validate_destination_arguments(args: &BlindPackArgs) -> Result<()> {
    if !is_exact_path(&args.reviewer_root, REVIEWER_ROOT)
        || !is_exact_path(&args.mapping_dir, MAPPING_DIR)
        || args
            .seed_dir
            .as_deref()
            .is_some_and(|path| !is_exact_path(path, SEED_DIR))
    {
        bail!("blind-pack destinations must use the exact private-root-relative path table");
    }
    Ok(())
}

fn is_exact_path(path: &Path, expected: &str) -> bool {
    path.as_os_str() == OsStr::new(expected)
}

fn validate_seed_arguments(args: &BlindPackArgs, execution_mode: ExecutionMode) -> Result<()> {
    match execution_mode {
        ExecutionMode::Replay => {
            let distinct = args.replay_seeds.iter().collect::<BTreeSet<_>>();
            if args.seed_dir.is_some()
                || args.replay_seeds.len() != 3
                || args.replay_seeds.iter().any(String::is_empty)
                || distinct.len() != 3
            {
                bail!(
                    "Replay blind-pack requires exactly three distinct nonempty replay seeds and no seed directory"
                );
            }
        }
        ExecutionMode::Mock | ExecutionMode::Live => {
            if !args
                .seed_dir
                .as_deref()
                .is_some_and(|path| is_exact_path(path, SEED_DIR))
                || !args.replay_seeds.is_empty()
            {
                bail!("Mock/Live blind-pack requires the exact seed directory and no replay seeds");
            }
        }
    }
    Ok(())
}

fn ensure_destinations_absent(private_root: &Path) -> Result<()> {
    for relative in [REVIEWER_ROOT, MAPPING_DIR, SEED_DIR, REVIEWS_DIR, RECEIPT] {
        let path = resolve_private_relative(private_root, Path::new(relative))?;
        match std::fs::symlink_metadata(&path) {
            Ok(_) => bail!("blind-pack output already exists: {relative}"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("inspect blind-pack output absence"),
        }
    }
    Ok(())
}
