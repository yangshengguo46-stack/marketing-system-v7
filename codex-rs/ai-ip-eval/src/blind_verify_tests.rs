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

pub(super) fn assert_current_stage(args: crate::BlindPackArgs, private_root: &Path) {
    let error = crate::blind::run_blind_pack(args).unwrap_err();
    assert_eq!(error.to_string(), "BlindPairFinalizationStageNotInstalled");
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

fn rewrite_typed_field<T>(path: &Path, field: &str, value: serde_json::Value) -> Vec<u8>
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let mut document: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    document[field] = value;
    let typed: T = serde_json::from_value(document).unwrap();
    serde_json::to_vec_pretty(&typed).unwrap()
}

fn assert_mutated_file_rejected(
    snapshot: &crate::blind::FrozenContextSnapshot,
    private_root: &Path,
    path: &Path,
    mutation: &[u8],
    expected: &str,
) {
    let original = fs::read(path).unwrap();
    fs::write(path, mutation).unwrap();
    let error = parse_pair_evidence(snapshot).unwrap_err();
    let report = format!("{error:#}");
    assert!(
        report.contains(expected),
        "expected {expected:?} in {report}"
    );
    assert_no_blind_outputs(private_root);
    fs::write(path, original).unwrap();
}

fn assert_missing_file_rejected(
    snapshot: &crate::blind::FrozenContextSnapshot,
    private_root: &Path,
    path: &Path,
) {
    let backup = path.with_extension("blind-missing-backup");
    fs::rename(path, &backup).unwrap();
    let error = parse_pair_evidence(snapshot).unwrap_err();
    assert!(format!("{error:#}").contains("read bounded"));
    assert_no_blind_outputs(private_root);
    fs::rename(backup, path).unwrap();
}

fn assert_resigned_execution_rejected<T>(
    snapshot: &crate::blind::FrozenContextSnapshot,
    private_root: &Path,
    coordinator: &Path,
    execution_bytes: &[u8],
    pair_name: &str,
    expected: &str,
) where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let execution_path = coordinator.join("execution-context.json");
    let arm_paths = [
        coordinator.join("run-1-manifest.json"),
        coordinator.join("run-2-manifest.json"),
    ];
    let pair_path = coordinator.join(pair_name);
    let original_execution = fs::read(&execution_path).unwrap();
    let original_arms = arm_paths.each_ref().map(|path| fs::read(path).unwrap());
    let original_pair = fs::read(&pair_path).unwrap();

    fs::write(&execution_path, execution_bytes).unwrap();
    let execution_sha = test_sha256(execution_bytes);
    let mut arm_links = Vec::new();
    for path in &arm_paths {
        let mut arm: crate::RunManifest = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        arm.execution_context_sha256.clone_from(&execution_sha);
        let bytes = serde_json::to_vec_pretty(&arm).unwrap();
        fs::write(path, &bytes).unwrap();
        arm_links.push((arm.condition, test_sha256(&bytes)));
    }
    let mut pair: serde_json::Value = serde_json::from_slice(&original_pair).unwrap();
    for (condition, sha) in arm_links {
        let field = match condition {
            crate::EvaluationCondition::Generic => "genericRunManifestSha256",
            crate::EvaluationCondition::Candidate => "candidateRunManifestSha256",
        };
        pair[field] = json!(sha);
    }
    if pair.get("executionContextSha256").is_some() {
        pair["executionContextSha256"] = json!(execution_sha);
    }
    let typed_pair: T = serde_json::from_value(pair).unwrap();
    fs::write(&pair_path, serde_json::to_vec_pretty(&typed_pair).unwrap()).unwrap();

    let error = parse_pair_evidence(snapshot).unwrap_err();
    let report = format!("{error:#}");
    assert!(
        report.contains(expected),
        "expected {expected:?} in {report}"
    );
    assert_no_blind_outputs(private_root);

    fs::write(execution_path, original_execution).unwrap();
    for (path, bytes) in arm_paths.iter().zip(original_arms) {
        fs::write(path, bytes).unwrap();
    }
    fs::write(pair_path, original_pair).unwrap();
}

#[test]
fn blind_envelope_rejects_nonexact_document_encodings() {
    let replay = run_frozen_replay_pair().unwrap();
    let snapshot = crate::blind::read_context_snapshot(
        &replay_blind_args(&replay.private_root).frozen_run_context,
    )
    .unwrap();
    let coordinator = replay.private_root.join("replay-coordinator");
    let manifest = coordinator.join("run-1-manifest.json");
    let execution = coordinator.join("execution-context.json");
    let pair = coordinator.join("replay-pair-verification.json");

    let original = fs::read(&manifest).unwrap();
    let mut duplicate = b"{\"schemaVersion\":1,".to_vec();
    duplicate.extend_from_slice(&original[1..]);
    assert_mutated_file_rejected(
        &snapshot,
        &replay.private_root,
        &manifest,
        &duplicate,
        "duplicate object key",
    );

    let mut unknown: serde_json::Value =
        serde_json::from_slice(&fs::read(&execution).unwrap()).unwrap();
    unknown["unexpectedEnvelopeField"] = json!(true);
    assert_mutated_file_rejected(
        &snapshot,
        &replay.private_root,
        &execution,
        &serde_json::to_vec_pretty(&unknown).unwrap(),
        "unknown field",
    );

    let pair_value: serde_json::Value = serde_json::from_slice(&fs::read(&pair).unwrap()).unwrap();
    assert_mutated_file_rejected(
        &snapshot,
        &replay.private_root,
        &pair,
        &serde_json::to_vec(&pair_value).unwrap(),
        "not exact producer-order typed JSON without trailing bytes",
    );

    let mut trailing = fs::read(&manifest).unwrap();
    trailing.push(b'\n');
    assert_mutated_file_rejected(
        &snapshot,
        &replay.private_root,
        &manifest,
        &trailing,
        "not exact producer-order typed JSON without trailing bytes",
    );

    assert_mutated_file_rejected(
        &snapshot,
        &replay.private_root,
        &pair,
        &vec![b' '; 1024 * 1024 + 1],
        "private file exceeds its byte cap",
    );
}

#[test]
fn blind_envelope_rejects_missing_and_linked_fixed_paths() {
    let replay = run_frozen_replay_pair().unwrap();
    let snapshot = crate::blind::read_context_snapshot(
        &replay_blind_args(&replay.private_root).frozen_run_context,
    )
    .unwrap();
    let coordinator = replay.private_root.join("replay-coordinator");
    assert_missing_file_rejected(
        &snapshot,
        &replay.private_root,
        &coordinator.join("execution-context.json"),
    );

    let pair = coordinator.join("replay-pair-verification.json");
    let pair_backup = pair.with_extension("blind-hardlink-backup");
    fs::rename(&pair, &pair_backup).unwrap();
    fs::hard_link(&pair_backup, &pair).unwrap();
    let error = parse_pair_evidence(&snapshot).unwrap_err();
    let report = format!("{error:#}");
    assert!(
        report.contains("unsafe type, links, or permissions")
            || report.contains("reparse point or hardlink"),
        "{report}"
    );
    assert_no_blind_outputs(&replay.private_root);
    fs::remove_file(&pair).unwrap();
    fs::rename(pair_backup, pair).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let manifest = coordinator.join("run-1-manifest.json");
        let manifest_backup = manifest.with_extension("blind-symlink-backup");
        fs::rename(&manifest, &manifest_backup).unwrap();
        symlink(&manifest_backup, &manifest).unwrap();
        let error = parse_pair_evidence(&snapshot).unwrap_err();
        let report = format!("{error:#}");
        assert!(
            report.contains("open no-follow path")
                || report.contains("unsafe type, links, or permissions")
                || report.contains("private path contains a link"),
            "{report}"
        );
        assert_no_blind_outputs(&replay.private_root);
        fs::remove_file(&manifest).unwrap();
        fs::rename(manifest_backup, manifest).unwrap();
    }
}

#[test]
fn blind_envelope_rejects_wrong_arm_shape_and_replay_modes() {
    let replay = run_frozen_replay_pair().unwrap();
    let snapshot = crate::blind::read_context_snapshot(
        &replay_blind_args(&replay.private_root).frozen_run_context,
    )
    .unwrap();
    let coordinator = replay.private_root.join("replay-coordinator");
    let first = coordinator.join("run-1-manifest.json");
    let second = coordinator.join("run-2-manifest.json");

    assert_mutated_file_rejected(
        &snapshot,
        &replay.private_root,
        &first,
        &rewrite_typed_field::<crate::RunManifest>(&first, "runOrdinal", json!(2)),
        "exact two-arm ordinal and condition set",
    );
    let second_value: serde_json::Value =
        serde_json::from_slice(&fs::read(&second).unwrap()).unwrap();
    assert_mutated_file_rejected(
        &snapshot,
        &replay.private_root,
        &first,
        &rewrite_typed_field::<crate::RunManifest>(
            &first,
            "condition",
            second_value["condition"].clone(),
        ),
        "exact two-arm ordinal and condition set",
    );

    let mut typed: crate::RunManifest = serde_json::from_slice(&fs::read(&first).unwrap()).unwrap();
    typed.execution_mode = crate::ExecutionMode::Mock;
    typed.mode_evidence = crate::ModeEvidence::Mock {
        provider_mode: crate::MockProviderMode::NotRun,
        synthetic_fixture_sha256: "a".repeat(64),
        arm_order_commitment: "b".repeat(64),
    };
    assert_mutated_file_rejected(
        &snapshot,
        &replay.private_root,
        &first,
        &serde_json::to_vec_pretty(&typed).unwrap(),
        "run manifest mode differs from verified C1 producer mode",
    );

    let execution = coordinator.join("execution-context.json");
    assert_resigned_execution_rejected::<crate::blind_verify::ReplayPairVerification>(
        &snapshot,
        &replay.private_root,
        &coordinator,
        &rewrite_typed_field::<crate::blind_verify::ReplayExecutionContext>(
            &execution,
            "executionMode",
            json!("mock"),
        ),
        "replay-pair-verification.json",
        "Replay envelopes differ from the retained pair binding",
    );
    let pair = coordinator.join("replay-pair-verification.json");
    assert_mutated_file_rejected(
        &snapshot,
        &replay.private_root,
        &pair,
        &rewrite_typed_field::<crate::blind_verify::ReplayPairVerification>(
            &pair,
            "executionMode",
            json!("mock"),
        ),
        "Replay envelopes differ from the retained pair binding",
    );
}

#[test]
fn blind_envelope_rejects_replay_binding_and_direct_link_drift() {
    let replay = run_frozen_replay_pair().unwrap();
    let snapshot = crate::blind::read_context_snapshot(
        &replay_blind_args(&replay.private_root).frozen_run_context,
    )
    .unwrap();
    let coordinator = replay.private_root.join("replay-coordinator");
    let manifest = coordinator.join("run-1-manifest.json");
    let execution = coordinator.join("execution-context.json");
    let pair = coordinator.join("replay-pair-verification.json");
    let wrong = json!("f".repeat(64));

    for field in ["pairId", "frozenRunContextSha256", "executionContextSha256"] {
        assert_mutated_file_rejected(
            &snapshot,
            &replay.private_root,
            &manifest,
            &rewrite_typed_field::<crate::RunManifest>(&manifest, field, wrong.clone()),
            "run manifest differs from the retained pair binding",
        );
    }
    for field in ["pairId", "frozenRunContextSha256"] {
        assert_resigned_execution_rejected::<crate::blind_verify::ReplayPairVerification>(
            &snapshot,
            &replay.private_root,
            &coordinator,
            &rewrite_typed_field::<crate::blind_verify::ReplayExecutionContext>(
                &execution,
                field,
                wrong.clone(),
            ),
            "replay-pair-verification.json",
            "Replay envelopes differ from the retained pair binding",
        );
    }
    for field in ["pairId", "frozenRunContextSha256"] {
        assert_mutated_file_rejected(
            &snapshot,
            &replay.private_root,
            &pair,
            &rewrite_typed_field::<crate::blind_verify::ReplayPairVerification>(
                &pair,
                field,
                wrong.clone(),
            ),
            "Replay envelopes differ from the retained pair binding",
        );
    }
    for field in ["genericRunManifestSha256", "genericContentPackageSha256"] {
        assert_mutated_file_rejected(
            &snapshot,
            &replay.private_root,
            &pair,
            &rewrite_typed_field::<crate::blind_verify::ReplayPairVerification>(
                &pair,
                field,
                wrong.clone(),
            ),
            "pair verification direct SHA links do not match the sealed arm evidence",
        );
    }
}

#[test]
fn blind_envelope_rejects_native_mode_and_execution_link_drift() {
    let native = run_native_mock_pair_with_marker(None, 10);
    native.result.as_ref().unwrap();
    let snapshot = crate::blind::read_context_snapshot(
        &native_blind_args(&native.live_root).frozen_run_context,
    )
    .unwrap();
    let coordinator = native.live_root.join("coordinator");
    let manifest = coordinator.join("run-1-manifest.json");
    let mut typed: crate::RunManifest =
        serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    typed.execution_mode = crate::ExecutionMode::Live;
    assert_mutated_file_rejected(
        &snapshot,
        &native.live_root,
        &manifest,
        &serde_json::to_vec_pretty(&typed).unwrap(),
        "executionMode does not match modeEvidence",
    );

    let execution = coordinator.join("execution-context.json");
    assert_resigned_execution_rejected::<crate::blind_verify::NativePairVerification>(
        &snapshot,
        &native.live_root,
        &coordinator,
        &rewrite_typed_field::<crate::blind_verify::NativeExecutionContext>(
            &execution,
            "frozenRunContextSha256",
            json!("f".repeat(64)),
        ),
        "pair-verification.json",
        "Native envelopes differ from the retained pair binding",
    );
    let pair = coordinator.join("pair-verification.json");
    for field in ["frozenRunContextSha256", "executionContextSha256"] {
        assert_mutated_file_rejected(
            &snapshot,
            &native.live_root,
            &pair,
            &rewrite_typed_field::<crate::blind_verify::NativePairVerification>(
                &pair,
                field,
                json!("f".repeat(64)),
            ),
            if field == "executionContextSha256" {
                "pair verification direct SHA links do not match the sealed arm evidence"
            } else {
                "Native envelopes differ from the retained pair binding"
            },
        );
    }
}

#[test]
fn blind_envelope_rejects_independently_valid_wrong_marker() {
    let replay = run_frozen_replay_pair().unwrap();
    let snapshot = crate::blind::read_context_snapshot(
        &replay_blind_args(&replay.private_root).frozen_run_context,
    )
    .unwrap();
    for relative in [
        "coordinator/pair-marker.json",
        "coordinator/private-inventory.jsonl",
    ] {
        fs::remove_file(replay.private_root.join(relative)).unwrap();
    }
    let frozen = fs::read(replay.private_root.join("frozen-run-context.json")).unwrap();
    crate::private_inventory::bootstrap_private_inventory(
        &replay.private_root,
        &"f".repeat(64),
        &test_sha256(&frozen),
        "2026-08-29T00:00:00Z",
    )
    .unwrap();

    let error = parse_pair_evidence(&snapshot).unwrap_err();
    assert_eq!(
        error.to_string(),
        "private inventory marker differs from the verified pair binding"
    );
    assert_no_blind_outputs(&replay.private_root);
}
