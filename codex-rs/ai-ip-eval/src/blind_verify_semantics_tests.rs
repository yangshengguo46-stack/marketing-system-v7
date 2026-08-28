use std::{
    fs,
    path::{Path, PathBuf},
};

use pretty_assertions::assert_eq;

use super::*;

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

fn assert_complete_core(
    core: crate::blind_verify::PairEvidenceCore,
    manifests: &[crate::RunManifest],
    coordinator: &Path,
    native: bool,
) {
    assert_eq!(
        core.private_root,
        coordinator.parent().unwrap().canonicalize().unwrap()
    );
    assert_eq!(core.pair_id, manifests[0].pair_id);
    assert_eq!(core.fork_sha, manifests[0].fork_sha);
    assert_eq!(
        core.frozen_run_context_sha256,
        manifests[0].frozen_run_context_sha256
    );
    assert_eq!(
        core.initial_inventory_root_sha256,
        core.inventory.inventory_root_sha256()
    );
    let parsed_case = serde_json::from_slice(&core.case_bytes).unwrap();
    let encoded_materials = serde_json::to_vec(&core.materials_manifest).unwrap();
    assert_eq!(core.mission_case, parsed_case);
    assert_eq!(core.materials_manifest, core.mission_case.materials);
    assert_eq!(core.materials_manifest_bytes, encoded_materials);
    assert!(!core.materials.is_empty());
    for (material, declaration) in core.materials.iter().zip(&core.materials_manifest) {
        assert_eq!(material.material_id, declaration.material_id);
        assert_eq!(material.relative_path, declaration.relative_path);
        assert_eq!(material.sha256, declaration.sha256);
        assert_eq!(material.sha256, test_sha256(&material.bytes));
    }
    let expected_mode = [crate::ExecutionMode::Replay, crate::ExecutionMode::Mock][native as usize];
    assert_eq!(core.mode, expected_mode);
    match &core.inputs {
        crate::blind::FrozenInputToken::Replay(inputs) => {
            assert!(!native);
            assert_eq!(
                core.reviewers.as_slice(),
                inputs.projection().attestation.reviewers.as_slice()
            );
        }
        crate::blind::FrozenInputToken::Native { content, .. } => {
            assert!(native);
            assert_eq!(core.reviewers, content.projection().reviewers);
        }
    }
    for (arm, manifest) in core.arms.iter().zip(manifests) {
        assert_eq!(arm.run_ordinal, manifest.run_ordinal);
        assert_eq!(arm.condition, manifest.condition);
        assert_eq!(arm.usage, manifest.usage);
        assert_eq!(arm.raw_response_count, manifest.raw_response_count);
        assert_eq!(
            arm.content_package_bytes,
            serde_json::to_vec(&arm.content_package).unwrap()
        );
        assert_eq!(
            test_sha256(&arm.content_package_bytes),
            manifest.content_package_sha256
        );
        assert_eq!(
            arm.skill_use.evidence_sha256,
            manifest.skill_use_evidence_sha256
        );
        assert_eq!(
            arm.run_manifest_raw_sha256,
            test_sha256(
                &fs::read(coordinator.join(format!("run-{}-manifest.json", arm.run_ordinal)))
                    .unwrap()
            )
        );
    }
    let pair_name = ["replay-pair-verification.json", "pair-verification.json"][native as usize];
    assert_eq!(
        core.pair_verification_raw_sha256,
        test_sha256(&fs::read(coordinator.join(pair_name)).unwrap())
    );
    let expected_receipt = native
        .then(|| test_sha256(&fs::read(coordinator.join("receipts/pair-receipt.json")).unwrap()));
    assert_eq!(core.native_pair_receipt_raw_sha256, expected_receipt);
}

fn semantic_core(args: &crate::BlindPackArgs) -> crate::blind_verify::PairEvidenceCore {
    let snapshot = crate::blind::read_context_snapshot(&args.frozen_run_context).unwrap();
    crate::blind_verify::verify_pair_evidence_core(&snapshot).unwrap()
}

#[test]
fn sealed_replay_pair_reaches_blind_finalization_stage() {
    let fixture = replay_fixture_with_material();
    let replay = run_frozen_replay_pair_from(fixture.path()).unwrap();
    let args = replay_blind_args(&replay.private_root);
    assert_complete_core(
        semantic_core(&args),
        &replay.manifests,
        &replay.private_root.join("replay-coordinator"),
        false,
    );
    super::blind_verify_tests::assert_current_stage(args, &replay.private_root);
}

#[test]
fn sealed_native_pair_reaches_blind_finalization_stage() {
    let native = run_native_mock_pair_with_marker(None, 10);
    native.result.as_ref().unwrap();
    let args = native_blind_args(&native.live_root);
    let manifests = read_native_mock_manifests(&native.live_root);
    assert_complete_core(
        semantic_core(&args),
        &manifests,
        &native.live_root.join("coordinator"),
        true,
    );
    super::blind_verify_tests::assert_current_stage(args, &native.live_root);
}

#[test]
fn sealed_native_pair_reaches_b2_receipt_semantics() {
    let native = run_native_mock_pair_with_marker(None, 10);
    native.result.as_ref().unwrap();
    let receipt_path = native
        .live_root
        .join("coordinator/receipts/pair-receipt.json");
    let raw = fs::read(&receipt_path).unwrap();
    let mut receipt: crate::PairReceipt =
        serde_json::from_slice(raw.strip_suffix(b"\n").unwrap()).unwrap();
    receipt.total_attempt_count += 1;
    let mut mutated = serde_json::to_vec(&receipt).unwrap();
    mutated.push(b'\n');
    fs::write(receipt_path, mutated).unwrap();
    for relative in [
        "coordinator/pair-marker.json",
        "coordinator/private-inventory.jsonl",
    ] {
        fs::remove_file(native.live_root.join(relative)).unwrap();
    }
    let frozen = fs::read(native.live_root.join("frozen-run-context.json")).unwrap();
    let context: serde_json::Value = serde_json::from_slice(&frozen).unwrap();
    crate::private_inventory::bootstrap_private_inventory(
        &native.live_root.canonicalize().unwrap(),
        context["pairId"].as_str().unwrap(),
        &test_sha256(&frozen),
        "2026-08-29T00:00:00Z",
    )
    .unwrap();
    let error = crate::blind::run_blind_pack(native_blind_args(&native.live_root)).unwrap_err();
    assert_eq!(error.to_string(), "pair receipt totals are invalid");
    assert_no_blind_outputs(&native.live_root);
}
