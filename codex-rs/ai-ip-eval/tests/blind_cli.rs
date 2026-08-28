use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use anyhow::Result;
use codex_utils_cargo_bin::cargo_bin;
use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::TempDir;

#[test]
fn blind_cli_accepts_authoritative_replay_surface() -> Result<()> {
    let temp = TempDir::new()?;
    let private_root = temp.path().join("private");
    fs::create_dir(&private_root)?;
    #[cfg(unix)]
    {
        fs::set_permissions(&private_root, fs::Permissions::from_mode(0o700))?;
    }
    let private_root = private_root.canonicalize()?;
    let frozen_context = private_root.join("frozen-run-context.json");
    fs::write(
        &frozen_context,
        serde_json::to_vec(&json!({
            "executionMode": "replay",
            "privateRoot": private_root,
        }))?,
    )?;
    #[cfg(unix)]
    fs::set_permissions(&frozen_context, fs::Permissions::from_mode(0o600))?;

    let output = Command::new(cargo_bin("codex-ai-ip-eval")?)
        .args([
            "blind-pack",
            "--reviewer-root",
            "reviewer",
            "--mapping-dir",
            "coordinator/mappings",
            "--replay-seed",
            "mechanical-reviewer-1",
            "--replay-seed",
            "mechanical-reviewer-2",
            "--replay-seed",
            "mechanical-reviewer-3",
            "--frozen-run-context",
        ])
        .arg(&frozen_context)
        .output()?;

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr)?;
    for stale_error in [
        "unexpected argument",
        "Usage:",
        "selected evaluator workflow",
    ] {
        assert!(!stderr.contains(stale_error), "stale CLI error: {stderr}");
    }
    assert_eq!(
        stderr.trim_end(),
        "Error: BlindPairVerifierStageNotInstalled"
    );
    for relative in [
        "reviewer",
        "coordinator/mappings",
        "coordinator/blind-seeds",
        "reviews",
        "coordinator/blind-pack-receipt.json",
    ] {
        assert!(!private_root.join(relative).exists(), "created {relative}");
    }
    Ok(())
}
