use std::fs;
use std::path::Path;
use std::path::PathBuf;

use pretty_assertions::assert_eq;
use serde_json::json;

use super::*;
use crate::blind::FrozenInputToken;
use crate::blind_verify::ExecutionContext;
use crate::blind_verify::PairVerification;
use crate::blind_verify::parse_pair_evidence;

fn replay_blind_args(private_root: &Path) -> crate::BlindPackArgs {
    crate::BlindPackArgs {
        reviewer_root: PathBuf::from("reviewer"),
        mapping_dir: PathBuf::from("coordinator/mappings"),
        seed_dir: None,
        replay_seeds: ["one", "two", "three"].map(str::to_string).to_vec(),
        frozen_run_context: private_root.join("frozen-run-context.json"),
    }
}

fn native_blind_args(private_root: &Path) -> crate::BlindPackArgs {
    let private_root = private_root.canonicalize().unwrap();
    crate::BlindPackArgs {
        reviewer_root: PathBuf::from("reviewer"),
        mapping_dir: PathBuf::from("coordinator/mappings"),
        seed_dir: Some(PathBuf::from("coordinator/blind-seeds")),
        replay_seeds: Vec::new(),
        frozen_run_context: private_root.join("frozen-run-context.json"),
    }
}

fn assert_no_blind_outputs(private_root: &Path) {
    for relative in [
        "reviewer",
        "coordinator/mappings",
        "coordinator/blind-seeds",
        "reviews",
        "coordinator/blind-pack-receipt.json",
    ] {
        let error = fs::symlink_metadata(private_root.join(relative)).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound, "{relative}");
    }
}

fn assert_current_stage(args: crate::BlindPackArgs, private_root: &Path) {
    let error = crate::blind::run_blind_pack(args).unwrap_err();
    assert_eq!(error.to_string(), "PairEvidenceCoreStageNotInstalled");
    assert_no_blind_outputs(private_root);
}

fn replace_with_same_owner_only_bytes(path: &Path) {
    let path = path.canonicalize().unwrap();
    let bytes = fs::read(&path).unwrap();
    let replacement = path.with_extension("blind-replacement");
    let displaced = path
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .join("blind-displaced-frozen-context.json");
    crate::secure_fs::write_owner_only_new(&replacement, &bytes).unwrap();
    fs::rename(&path, displaced).unwrap();
    fs::rename(replacement, path).unwrap();
}

#[test]
fn blind_snapshot_rejects_same_bytes_new_inode_before_pair_verification() {
    let replay = run_frozen_replay_pair().unwrap();
    let args = replay_blind_args(&replay.private_root);
    let snapshot = crate::blind::read_context_snapshot(&args.frozen_run_context).unwrap();
    replace_with_same_owner_only_bytes(&args.frozen_run_context);

    let error = crate::blind::verify_blind_pair_stage(&args, &snapshot).unwrap_err();
    assert_eq!(
        error.to_string(),
        "reverify retained blind-pack frozen context identity"
    );
    assert_no_blind_outputs(&replay.private_root);
}

fn rebuild_native_inventory(private_root: &Path) {
    let private_root = private_root.canonicalize().unwrap();
    for relative in [
        "coordinator/pair-marker.json",
        "coordinator/private-inventory.jsonl",
    ] {
        fs::remove_file(private_root.join(relative)).unwrap();
    }
    let frozen = fs::read(private_root.join("frozen-run-context.json")).unwrap();
    let context: serde_json::Value = serde_json::from_slice(&frozen).unwrap();
    crate::private_inventory::bootstrap_private_inventory(
        &private_root,
        context["pairId"].as_str().unwrap(),
        &test_sha256(&frozen),
        "2026-08-29T00:00:00Z",
    )
    .unwrap();
}

#[test]
fn blind_native_pair_rejects_fully_resigned_live_manifest_mode() {
    let native = run_native_mock_pair_with_marker(None, 10);
    native.result.as_ref().unwrap();
    let coordinator = native.live_root.join("coordinator");
    let manifest_path = coordinator.join("run-1-manifest.json");
    let mut manifest: crate::RunManifest =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest.execution_mode = crate::ExecutionMode::Live;
    manifest.mode_evidence = crate::ModeEvidence::Live {
        attestation_sha256: "1".repeat(64),
        provider_budget_evidence_sha256: "2".repeat(64),
        approval_commitment: "3".repeat(64),
        provider_endpoint_commitment: "4".repeat(64),
        provider_role: crate::ProviderRole::ApprovedReference,
        arm_order_commitment: "5".repeat(64),
        rate_card_sha256: "6".repeat(64),
        billing_policy_sha256: "7".repeat(64),
        fx_policy_sha256: None,
        authorized_pair_cost_fen: 0,
        retention_deadline: "2026-08-29T01:00:00Z".to_string(),
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();
    fs::write(&manifest_path, &manifest_bytes).unwrap();
    let pair_path = coordinator.join("pair-verification.json");
    let mut pair: serde_json::Value =
        serde_json::from_slice(&fs::read(&pair_path).unwrap()).unwrap();
    let link = match manifest.condition {
        crate::EvaluationCondition::Generic => "genericRunManifestSha256",
        crate::EvaluationCondition::Candidate => "candidateRunManifestSha256",
    };
    pair[link] = json!(test_sha256(&manifest_bytes));
    fs::write(&pair_path, serde_json::to_vec_pretty(&pair).unwrap()).unwrap();
    rebuild_native_inventory(&native.live_root);

    let args = native_blind_args(&native.live_root);
    let snapshot = crate::blind::read_context_snapshot(&args.frozen_run_context).unwrap();
    let error = parse_pair_evidence(&snapshot).unwrap_err();
    assert_eq!(
        error.to_string(),
        "run manifest mode differs from verified C1 producer mode"
    );
    assert_no_blind_outputs(&native.live_root);
}

#[test]
fn blind_pair_parser_retains_exact_real_replay_and_native_envelopes() {
    let replay = run_frozen_replay_pair().unwrap();
    let replay_args = replay_blind_args(&replay.private_root);
    let snapshot = crate::blind::read_context_snapshot(&replay_args.frozen_run_context).unwrap();
    let parsed = parse_pair_evidence(&snapshot).unwrap();
    match &parsed.inputs {
        FrozenInputToken::Replay(inputs) => inputs.reverify_all().unwrap(),
        FrozenInputToken::Native { .. } => panic!("Replay pair returned Native inputs"),
    }
    let replay_coordinator = replay.private_root.join("replay-coordinator");
    let execution_bytes = fs::read(replay_coordinator.join("execution-context.json")).unwrap();
    let execution: ExecutionContext = serde_json::from_slice(&execution_bytes).unwrap();
    assert_eq!(
        (
            &parsed.execution_context.typed,
            &parsed.execution_context.raw_bytes
        ),
        (&execution, &execution_bytes)
    );
    assert_eq!(
        parsed.arms.each_ref().map(|arm| &arm.typed),
        [&replay.manifests[0], &replay.manifests[1]]
    );
    let pair_bytes = fs::read(replay_coordinator.join("replay-pair-verification.json")).unwrap();
    let pair: PairVerification = serde_json::from_slice(&pair_bytes).unwrap();
    assert_eq!(
        (
            &parsed.pair_verification.typed,
            &parsed.pair_verification.raw_bytes
        ),
        (&pair, &pair_bytes)
    );
    parsed.inventory.reverify_unchanged().unwrap();
    assert_current_stage(replay_args, &replay.private_root);

    let native = run_native_mock_pair_with_marker(None, 10);
    native.result.as_ref().unwrap();
    let native_args = native_blind_args(&native.live_root);
    let snapshot = crate::blind::read_context_snapshot(&native_args.frozen_run_context).unwrap();
    let parsed = parse_pair_evidence(&snapshot).unwrap();
    match &parsed.inputs {
        FrozenInputToken::Native { frozen, content } => {
            frozen.reverify_all().unwrap();
            content.reverify_unchanged().unwrap();
        }
        FrozenInputToken::Replay(_) => panic!("Native pair returned Replay inputs"),
    }
    let coordinator = native.live_root.join("coordinator");
    for (document, path) in [
        (
            &parsed.execution_context.raw_bytes,
            coordinator.join("execution-context.json"),
        ),
        (
            &parsed.pair_verification.raw_bytes,
            coordinator.join("pair-verification.json"),
        ),
    ] {
        assert_eq!(document, &fs::read(path).unwrap());
    }
    for (ordinal, arm) in [1_u8, 2].into_iter().zip(&parsed.arms) {
        let bytes = fs::read(coordinator.join(format!("run-{ordinal}-manifest.json"))).unwrap();
        let typed: crate::RunManifest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            (&arm.typed, &arm.raw_bytes, &arm.sha256),
            (&typed, &bytes, &test_sha256(&bytes))
        );
    }
    let canonical_root = native.live_root.canonicalize().unwrap();
    let frozen_bytes = fs::read(canonical_root.join("frozen-run-context.json")).unwrap();
    let frozen: serde_json::Value = serde_json::from_slice(&frozen_bytes).unwrap();
    assert_eq!(
        parsed.inventory_binding,
        crate::blind_verify::PrivateInventoryBinding {
            private_root: canonical_root,
            pair_id: frozen["pairId"].as_str().unwrap().to_string(),
            frozen_run_context_sha256: test_sha256(&frozen_bytes),
            inventory_root_sha256: parsed.inventory.inventory_root_sha256().to_string(),
        }
    );
    parsed.inventory.reverify_unchanged().unwrap();
    assert_current_stage(native_args, &native.live_root);
}
