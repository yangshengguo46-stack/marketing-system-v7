//! Trusted filesystem and sealed-evidence authority for `score`.

use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;

use crate::ScoreArgs;
use crate::secure_fs::resolve_private_relative;

const MAPPING_DIR: &str = "coordinator/mappings";
const REVIEWS_DIR: &str = "reviews";
const OUTPUT: &str = "coordinator/decision.private.json";
const STAGING_OUTPUT: &str = "coordinator/.decision.private.json.staging";
const FROZEN_CONTEXT: &str = "frozen-run-context.json";

pub(crate) struct ResolvedScorePaths {
    mapping_dir: PathBuf,
    reviews_dir: PathBuf,
    output: PathBuf,
    staging_output: PathBuf,
    frozen_run_context: PathBuf,
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
