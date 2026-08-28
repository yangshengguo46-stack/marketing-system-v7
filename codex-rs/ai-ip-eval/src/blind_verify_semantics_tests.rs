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

fn assert_current_stage(args: crate::BlindPackArgs, private_root: &Path) {
    let error = crate::blind::run_blind_pack(args).unwrap_err();
    assert_eq!(error.to_string(), "BlindPairFinalizationStageNotInstalled");
    assert_no_blind_outputs(private_root);
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
            let projection = inputs.projection();
            assert_eq!(
                core.reviewers.as_slice(),
                projection.attestation.reviewers.as_slice()
            );
            assert_eq!(core.case_bytes, inputs.fixture_bytes("case").unwrap());
            assert_eq!(
                core.materials_manifest_bytes,
                projection.materials_manifest_bytes
            );
            for (actual, expected) in core.materials.iter().zip(projection.materials) {
                assert_eq!(actual.bytes, expected.bytes);
            }
        }
        crate::blind::FrozenInputToken::Native { content, .. } => {
            assert!(native);
            let projection = content.projection();
            assert_eq!(core.reviewers, projection.reviewers);
            assert_eq!(core.case_bytes, projection.case_bytes);
            assert_eq!(
                core.materials_manifest_bytes,
                projection.materials_manifest_bytes
            );
            for (actual, expected) in core.materials.iter().zip(&projection.materials) {
                assert_eq!(actual.bytes, expected.bytes);
            }
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

fn rebuild_inventory(private_root: &Path) {
    for relative in [
        "coordinator/pair-marker.json",
        "coordinator/private-inventory.jsonl",
    ] {
        fs::remove_file(private_root.join(relative)).unwrap();
    }
    let frozen = fs::read(private_root.join("frozen-run-context.json")).unwrap();
    let context: serde_json::Value = serde_json::from_slice(&frozen).unwrap();
    crate::private_inventory::bootstrap_private_inventory(
        &private_root.canonicalize().unwrap(),
        context["pairId"].as_str().unwrap(),
        &test_sha256(&frozen),
        "2026-08-29T00:00:00Z",
    )
    .unwrap();
}

fn replay_coordinator(private_root: &Path) -> PathBuf {
    private_root.join("replay-coordinator")
}

fn replay_manifest_path(private_root: &Path, ordinal: u8) -> PathBuf {
    replay_coordinator(private_root).join(format!("run-{ordinal}-manifest.json"))
}

fn resign_replay_pair(private_root: &Path, edit_pair: impl FnOnce(&mut serde_json::Value)) {
    let coordinator = replay_coordinator(private_root);
    let pair_path = coordinator.join("replay-pair-verification.json");
    let mut pair: serde_json::Value =
        serde_json::from_slice(&fs::read(&pair_path).unwrap()).unwrap();
    edit_pair(&mut pair);
    for ordinal in [1_u8, 2] {
        let path = replay_manifest_path(private_root, ordinal);
        let bytes = fs::read(path).unwrap();
        let manifest: crate::RunManifest = serde_json::from_slice(&bytes).unwrap();
        let field = match manifest.condition {
            crate::EvaluationCondition::Generic => "genericRunManifestSha256",
            crate::EvaluationCondition::Candidate => "candidateRunManifestSha256",
        };
        pair[field] = serde_json::json!(test_sha256(&bytes));
    }
    let typed: crate::blind_verify::ReplayPairVerification = serde_json::from_value(pair).unwrap();
    fs::write(pair_path, serde_json::to_vec_pretty(&typed).unwrap()).unwrap();
    rebuild_inventory(private_root);
}

fn resign_replay_sidecar(
    private_root: &Path,
    ordinal: u8,
    leaf: &str,
    sidecar_bytes: &[u8],
    manifest: &mut crate::RunManifest,
) {
    let coordinator = replay_coordinator(private_root);
    let sidecar_path = coordinator.join(format!("run-{ordinal}-{leaf}"));
    fs::write(&sidecar_path, sidecar_bytes).unwrap();
    let index_path = coordinator.join(format!("run-{ordinal}-postprocess-index.json"));
    let mut index = crate::jcs::parse_json(&fs::read(&index_path).unwrap()).unwrap();
    let relative = format!("replay-coordinator/run-{ordinal}-{leaf}");
    let entry = index["sidecars"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["relativePath"] == relative)
        .unwrap();
    entry["sha256"] = serde_json::json!(test_sha256(sidecar_bytes));
    let index_bytes = crate::jcs::canonicalize_value(&index).unwrap();
    fs::write(index_path, &index_bytes).unwrap();
    manifest.postprocess_evidence_index_sha256 = test_sha256(&index_bytes);
    fs::write(
        replay_manifest_path(private_root, ordinal),
        serde_json::to_vec_pretty(manifest).unwrap(),
    )
    .unwrap();
}

fn assert_semantic_rejection(args: crate::BlindPackArgs, private_root: &Path, expected: &str) {
    let error = crate::blind::run_blind_pack(args).unwrap_err();
    assert_eq!(error.to_string(), expected);
    assert_no_blind_outputs(private_root);
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
    assert_current_stage(args, &replay.private_root);
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
    assert_current_stage(args, &native.live_root);
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

#[test]
fn sealed_replay_pair_rejects_fully_resigned_frozen_request_commitments() {
    let fixture = replay_fixture_with_material();
    let replay = run_frozen_replay_pair_from(fixture.path()).unwrap();
    let commitments = ["1", "2", "3", "4", "5", "6"].map(|digit| digit.repeat(64));
    for ordinal in [1_u8, 2] {
        let path = replay_manifest_path(&replay.private_root, ordinal);
        let mut manifest: crate::RunManifest =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let candidate = manifest.condition == crate::EvaluationCondition::Candidate;
        manifest.first_root_provider_request_commitment =
            commitments[usize::from(candidate)].clone();
        manifest.normalized_first_root_request_commitment =
            commitments[2 + usize::from(candidate)].clone();
        manifest.normalized_first_root_base_commitment = commitments[4].clone();
        manifest.first_root_treatment_diff_commitment = candidate.then(|| commitments[5].clone());
        fs::write(path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }
    resign_replay_pair(&replay.private_root, |pair| {
        pair["requestParity"] = serde_json::json!({
            "schemaVersion": 1,
            "candidateNormalizedCommitment": commitments[3],
            "candidateRawCommitment": commitments[1],
            "candidateTreatmentDiffCommitment": commitments[5],
            "genericNormalizedCommitment": commitments[2],
            "genericRawCommitment": commitments[0],
            "normalizedBaseCommitment": commitments[4],
        });
    });

    assert_semantic_rejection(
        replay_blind_args(&replay.private_root),
        &replay.private_root,
        "Replay request commitments differ from retained frozen fixtures",
    );
}

#[test]
fn sealed_replay_pair_rejects_fully_resigned_alternate_shared_config() {
    let fixture = replay_fixture_with_material();
    let replay = run_frozen_replay_pair_from(fixture.path()).unwrap();
    let alternate = crate::build_shared_config("replay-fixture", 2).unwrap();
    for home in ["replay-generic-home", "replay-candidate-home"] {
        fs::write(
            replay.private_root.join(home).join(".codex/config.toml"),
            &alternate.bytes,
        )
        .unwrap();
    }
    for ordinal in [1_u8, 2] {
        let config_path =
            replay_coordinator(&replay.private_root).join(format!("run-{ordinal}-config.json"));
        let config: crate::proof_archive::ConfigSidecar =
            serde_json::from_slice(&fs::read(config_path).unwrap()).unwrap();
        let mut value = serde_json::to_value(config).unwrap();
        let mut effective = alternate.layer_json.clone();
        effective["allow_login_shell"] = serde_json::json!(true);
        value["expectedConfigUtf8"] =
            serde_json::json!(String::from_utf8(alternate.bytes.clone()).unwrap());
        value["expectedLayerConfig"] = alternate.layer_json.clone();
        value["response"]["config"] = effective;
        value["response"]["layers"][0]["config"] = alternate.layer_json.clone();
        let config: crate::proof_archive::ConfigSidecar = serde_json::from_value(value).unwrap();
        let audit = crate::audit_frozen_config(
            &config.response,
            &config.requirements,
            config.canonical_config_path.clone(),
            config.expected_config_utf8.as_bytes().to_vec(),
            config.expected_layer_config.clone(),
        )
        .unwrap();
        let manifest_path = replay_manifest_path(&replay.private_root, ordinal);
        let mut manifest: crate::RunManifest =
            serde_json::from_slice(&fs::read(manifest_path).unwrap()).unwrap();
        manifest.shared_config_sha256 = test_sha256(&alternate.bytes);
        manifest.effective_config_sha256 = audit.effective_config_sha256;
        manifest.config_layers_sha256 = audit.config_layers_sha256;
        resign_replay_sidecar(
            &replay.private_root,
            ordinal,
            "config.json",
            &serde_json::to_vec(&config).unwrap(),
            &mut manifest,
        );
    }
    resign_replay_pair(&replay.private_root, |_| {});

    assert_semantic_rejection(
        replay_blind_args(&replay.private_root),
        &replay.private_root,
        "shared config bytes differ from deterministic producer config",
    );
}

#[test]
fn sealed_replay_pair_rejects_resigned_provider_attempt_range() {
    let fixture = replay_fixture_with_material();
    let replay = run_frozen_replay_pair_from(fixture.path()).unwrap();
    let ordinal = 2;
    let broker_path = replay_coordinator(&replay.private_root)
        .join(format!("run-{ordinal}-broker-snapshot.json"));
    let mut broker: crate::proof_archive::BrokerSnapshotSidecar =
        serde_json::from_slice(&fs::read(broker_path).unwrap()).unwrap();
    broker.global_attempt_end_exclusive = 1;
    let manifest_path = replay_manifest_path(&replay.private_root, ordinal);
    let mut manifest: crate::RunManifest =
        serde_json::from_slice(&fs::read(manifest_path).unwrap()).unwrap();
    manifest.provider_request_attempt_count = 1;
    resign_replay_sidecar(
        &replay.private_root,
        ordinal,
        "broker-snapshot.json",
        &serde_json::to_vec(&broker).unwrap(),
        &mut manifest,
    );
    resign_replay_pair(&replay.private_root, |_| {});

    assert_semantic_rejection(
        replay_blind_args(&replay.private_root),
        &replay.private_root,
        "Replay manifest violates provider-not-run semantics",
    );
}

#[test]
fn sealed_native_pair_treats_path_sha_as_historical_commitment() {
    let native = run_native_mock_pair_with_marker(None, 10);
    native.result.as_ref().unwrap();
    let args = native_blind_args(&native.live_root);
    let snapshot = crate::blind::read_context_snapshot(&args.frozen_run_context).unwrap();
    let parsed = crate::blind_verify::parse_pair_evidence(&snapshot).unwrap();
    let error =
        crate::blind_verify::verify_with_native_path_commitment_for_test(parsed, "F".repeat(64))
            .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Native execution context differs from verified inputs or manifests"
    );
    let snapshot = crate::blind::read_context_snapshot(&args.frozen_run_context).unwrap();
    let parsed = crate::blind_verify::parse_pair_evidence(&snapshot).unwrap();
    let core =
        crate::blind_verify::verify_with_native_path_commitment_for_test(parsed, "f".repeat(64))
            .unwrap();
    assert_eq!(core.mode, crate::ExecutionMode::Mock);
    assert_no_blind_outputs(&native.live_root);
}
