mod support;

use std::path::Path;
use std::process::Command;

use anyhow::Result;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

use support::cost_world::CostWorld;
use support::cost_world::eval_binary;

#[test]
fn cost_cli_requires_one_condition() -> Result<()> {
    let binary = eval_binary()?;
    let missing_context = Path::new("/definitely/missing/frozen-run-context.json");

    let no_condition = Command::new(&binary)
        .args(["make-cost-receipt", "--frozen-run-context"])
        .arg(missing_context)
        .output()?;
    assert_clap_rejection(&no_condition, "--condition <CONDITION>");

    let duplicate = Command::new(&binary)
        .args(["make-cost-receipt", "--condition", "generic"])
        .args(["--condition", "candidate", "--frozen-run-context"])
        .arg(missing_context)
        .output()?;
    assert_clap_rejection(&duplicate, "cannot be used multiple times");

    for condition in ["generic", "candidate"] {
        let accepted = Command::new(&binary)
            .args(["make-cost-receipt", "--condition", condition])
            .arg("--frozen-run-context")
            .arg(missing_context)
            .output()?;
        assert_eq!(accepted.status.code(), Some(1));
        assert!(accepted.stdout.is_empty());
        assert!(accepted.stderr.starts_with(b"Error: "));
        assert!(!String::from_utf8_lossy(&accepted.stderr).contains("invalid value"));
    }

    for condition in ["Generic", "CANDIDATE"] {
        let rejected = Command::new(&binary)
            .args(["make-cost-receipt", "--condition", condition])
            .arg("--frozen-run-context")
            .arg(missing_context)
            .output()?;
        assert_clap_rejection(&rejected, "invalid value");
    }

    let no_context = Command::new(binary)
        .args(["make-cost-receipt", "--condition", "generic"])
        .output()?;
    assert_clap_rejection(&no_context, "--frozen-run-context <FROZEN_RUN_CONTEXT>");
    Ok(())
}

#[test]
fn cost_cli_rejects_output_override() -> Result<()> {
    let binary = eval_binary()?;
    let temp = TempDir::new()?;
    let output_path = temp.path().join("arbitrary-receipt.json");
    let output = Command::new(binary)
        .args(["make-cost-receipt", "--condition", "generic"])
        .args(["--frozen-run-context", "/definitely/missing/context.json"])
        .arg("--output")
        .arg(&output_path)
        .output()?;

    assert_clap_rejection(&output, "unexpected argument '--output'");
    assert!(!output_path.exists());
    Ok(())
}

#[test]
fn cost_cli_refuses_replay_evidence() -> Result<()> {
    let world = CostWorld::replay()?;
    let missing_but_canonical = world.supplier_statement("generic");
    world.assert_refusal(
        "generic",
        Some(&missing_but_canonical),
        b"Error: make-cost-receipt requires verified Native Live evidence; Replay is refused\n",
    )
}

#[test]
fn cost_cli_refuses_native_mock_evidence() -> Result<()> {
    let world = CostWorld::native_mock()?;
    world.assert_refusal(
        "candidate",
        None,
        b"Error: make-cost-receipt requires executionMode=live; mock evidence is refused\n",
    )
}

#[test]
fn cost_cli_annotate_refuses_replay_evidence() -> Result<()> {
    let world = CostWorld::replay()?;
    let output = Command::new(eval_binary()?)
        .args([
            "annotate-cost",
            "--condition",
            "generic",
            "--frozen-run-context",
        ])
        .arg(world.private_root().join("frozen-run-context.json"))
        .output()?;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"Error: annotate-cost requires verified Native Live evidence; Replay is refused\n"
    );
    assert!(
        !world
            .private_root()
            .join("coordinator/cost/generic-binding.json")
            .exists()
    );
    Ok(())
}

#[test]
fn cost_cli_annotate_refuses_native_mock_evidence() -> Result<()> {
    let world = CostWorld::native_mock()?;
    let output = Command::new(eval_binary()?)
        .args([
            "annotate-cost",
            "--condition",
            "candidate",
            "--frozen-run-context",
        ])
        .arg(world.private_root().join("frozen-run-context.json"))
        .output()?;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"Error: annotate-cost requires executionMode=live; mock evidence is refused\n"
    );
    assert!(
        !world
            .private_root()
            .join("coordinator/cost/candidate-binding.json")
            .exists()
    );
    Ok(())
}

#[test]
fn cost_cli_refuses_omitted_pending_supplier_in_replay() -> Result<()> {
    let world = CostWorld::replay()?;
    world.add_pending_candidate_supplier()?;
    world.assert_refusal(
        "candidate",
        None,
        b"Error: make-cost-receipt requires verified Native Live evidence; Replay is refused\n",
    )
}

#[test]
fn cost_cli_refuses_omitted_pending_supplier_in_native_mock() -> Result<()> {
    let world = CostWorld::native_mock()?;
    world.add_pending_candidate_supplier()?;
    world.assert_refusal(
        "candidate",
        None,
        b"Error: make-cost-receipt requires executionMode=live; mock evidence is refused\n",
    )
}

#[test]
fn cost_cli_rejects_noncanonical_supplier_path() -> Result<()> {
    let world = CostWorld::replay()?;
    let private_root = world.private_root();
    let expected = world.supplier_statement("generic");
    let aliases = [
        Path::new("inputs/supplier-statements/generic.json").to_path_buf(),
        private_root.join("inputs/supplier-statements/../supplier-statements/generic.json"),
        world.supplier_statement("candidate"),
        private_root.join("inputs/generic.json"),
        private_root
            .parent()
            .unwrap()
            .join("external-supplier-statement.json"),
    ];
    assert!(expected.is_absolute());
    for path in aliases {
        world.assert_refusal(
            "generic",
            Some(&path),
            b"Error: make-cost-receipt supplier statement must use the exact condition-bound private input path\n",
        )?;
    }
    Ok(())
}

fn assert_clap_rejection(output: &std::process::Output, needle: &str) {
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(needle),
        "stderr did not contain {needle:?}:\n{stderr}"
    );
}
