use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::process::Output;

use anyhow::Result;
use codex_utils_cargo_bin::cargo_bin;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

#[test]
fn blind_cli_accepts_authoritative_replay_surface() -> Result<()> {
    let binary = cargo_bin("codex-ai-ip-eval")?;
    let fixture_set =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json")?;
    let fixture_root = fixture_set.parent().unwrap().canonicalize()?;
    let temp = TempDir::new()?;
    let private_root = temp.path().join("private");
    fs::create_dir(&private_root)?;
    #[cfg(unix)]
    fs::set_permissions(&private_root, fs::Permissions::from_mode(0o700))?;
    let private_root = private_root.canonicalize()?;
    let frozen_context = private_root.join("frozen-run-context.json");

    let freeze = Command::new(&binary)
        .args(["freeze-run-context", "replay", "--repo-root"])
        .arg(&fixture_root)
        .args(["--fork-sha", "synthetic-replay-fork", "--private-root"])
        .arg(&private_root)
        .arg("--codex-bin")
        .arg(&binary)
        .arg("--case")
        .arg(fixture_root.join("replay-case.json"))
        .arg("--transcript")
        .arg(fixture_root.join("replay-transcript.jsonl"))
        .arg("--fixture-set-manifest")
        .arg(&fixture_set)
        .arg("--output")
        .arg(&frozen_context)
        .output()?;
    assert_success("freeze-run-context replay", &freeze);

    let replay = Command::new(&binary)
        .args(["replay-pair", "--frozen-run-context"])
        .arg(&frozen_context)
        .output()?;
    assert_success("replay-pair", &replay);
    assert!(
        private_root
            .join("replay-coordinator/replay-pair-verification.json")
            .is_file()
    );

    let conflict = blind_command(&binary, &frozen_context)
        .args(["--seed-dir", "coordinator/blind-seeds"])
        .output()?;
    assert!(!conflict.status.success());
    assert_eq!(
        String::from_utf8(conflict.stderr)?.trim_end(),
        "Error: Replay blind-pack requires exactly three distinct nonempty replay seeds and no seed directory"
    );
    assert_no_outputs(&private_root);

    let output = blind_command(&binary, &frozen_context).output()?;
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
        "Error: PairEvidenceCoreStageNotInstalled"
    );
    assert_no_outputs(&private_root);
    Ok(())
}

fn blind_command(binary: &Path, frozen_context: &Path) -> Command {
    let mut command = Command::new(binary);
    command
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
        .arg(frozen_context);
    command
}

fn assert_success(stage: &str, output: &Output) {
    assert!(
        output.status.success(),
        "{stage} failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_no_outputs(private_root: &Path) {
    for relative in [
        "reviewer",
        "coordinator/mappings",
        "coordinator/blind-seeds",
        "reviews",
        "coordinator/blind-pack-receipt.json",
    ] {
        match fs::symlink_metadata(private_root.join(relative)) {
            Err(error) => {
                assert_eq!(
                    error.kind(),
                    std::io::ErrorKind::NotFound,
                    "found {relative}"
                );
            }
            Ok(metadata) => panic!("found {relative} with type {:?}", metadata.file_type()),
        }
    }
}
