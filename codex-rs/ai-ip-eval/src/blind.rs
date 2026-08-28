use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;

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
pub struct BlindPairVerifierStageNotInstalled;

impl fmt::Display for BlindPairVerifierStageNotInstalled {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BlindPairVerifierStageNotInstalled")
    }
}

impl std::error::Error for BlindPairVerifierStageNotInstalled {}

pub(crate) fn run_blind_pack(args: BlindPackArgs) -> Result<()> {
    validate_destination_arguments(&args)?;
    let context = read_context_header(&args.frozen_run_context)?;
    validate_seed_arguments(&args, context.execution_mode)?;
    ensure_destinations_absent(&context.private_root)?;
    Err(BlindPairVerifierStageNotInstalled.into())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlindContextHeader {
    execution_mode: ExecutionMode,
    private_root: PathBuf,
}

fn read_context_header(path: &Path) -> Result<BlindContextHeader> {
    if !path.is_absolute() {
        bail!("blind-pack frozen context must be an absolute canonical path");
    }
    let canonical = path
        .canonicalize()
        .context("canonicalize blind-pack frozen context")?;
    if canonical != path {
        bail!("blind-pack frozen context must be an absolute canonical path");
    }
    let bytes =
        read_single_link_regular(path).context("read retained blind-pack frozen context")?;
    let context: BlindContextHeader =
        serde_json::from_slice(&bytes).context("parse blind-pack context header")?;
    if context.private_root.join("frozen-run-context.json") != path {
        bail!("blind-pack frozen context is outside its declared private root");
    }
    Ok(context)
}

fn validate_destination_arguments(args: &BlindPackArgs) -> Result<()> {
    if args.reviewer_root != Path::new(REVIEWER_ROOT)
        || args.mapping_dir != Path::new(MAPPING_DIR)
        || args
            .seed_dir
            .as_deref()
            .is_some_and(|path| path != Path::new(SEED_DIR))
    {
        bail!("blind-pack destinations must use the exact private-root-relative path table");
    }
    Ok(())
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
            if args.seed_dir.as_deref() != Some(Path::new(SEED_DIR))
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
