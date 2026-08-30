use std::{
    fs,
    path::{Path, PathBuf},
};

use pretty_assertions::assert_eq;
use sha2::{Digest, Sha256};

use super::*;

#[path = "blind_verify_cost_projection_tests.rs"]
mod blind_verify_cost_projection_tests;

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
    let snapshot = crate::blind::read_context_snapshot(&args.frozen_run_context).unwrap();
    crate::blind::verify_blind_pair_stage(&args, &snapshot).unwrap();
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
    assert_eq!(core.materials.len(), core.materials_manifest.len());
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
            assert_eq!(core.materials.len(), projection.materials.len());
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
            assert_eq!(core.materials.len(), projection.materials.len());
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

#[derive(Clone, Copy)]
enum NativeIdentityAttack {
    Model,
    Deployment,
}

fn replace_json_value(record: &mut String, key: &str, replacement: &str) {
    let marker = format!("\"{key}\":");
    let start = record.find(&marker).unwrap() + marker.len();
    let raw = record.as_bytes();
    let end = match raw[start] {
        b'"' => start + 1 + record[start + 1..].find('"').unwrap() + 1,
        _ => {
            start
                + record[start..]
                    .find([',', '}'])
                    .unwrap_or(record.len() - start)
        }
    };
    record.replace_range(start..end, replacement);
}

fn write_receipt<T: serde::Serialize>(path: &Path, receipt: &T) -> String {
    let mut bytes = serde_json::to_vec(receipt).unwrap();
    bytes.push(b'\n');
    fs::write(path, &bytes).unwrap();
    test_sha256(&bytes)
}

fn resign_native_identity(private_root: &Path, attack: NativeIdentityAttack) {
    let coordinator = private_root.join("coordinator");
    let manifest_paths = [
        coordinator.join("run-1-manifest.json"),
        coordinator.join("run-2-manifest.json"),
    ];
    let mut manifests = manifest_paths.each_ref().map(|path| {
        serde_json::from_slice::<crate::RunManifest>(&fs::read(path).unwrap()).unwrap()
    });
    let forged_model = "forged-native-model";
    let raw_deployment = "forged-native-deployment";
    let deployment_commitment = test_sha256(raw_deployment.as_bytes());
    match attack {
        NativeIdentityAttack::Model => {
            manifests[0].actual_model_revision = forged_model.to_string()
        }
        NativeIdentityAttack::Deployment => {
            manifests[0].deployment_or_fingerprint_commitment = Some(deployment_commitment.clone())
        }
    }

    let ledger_path = coordinator.join("attempt-index.jsonl");
    let ledger = fs::read(&ledger_path).unwrap();
    let mut lines = std::str::from_utf8(ledger.strip_suffix(b"\n").unwrap())
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();
    for global in 0..manifests[0].provider_request_attempt_count {
        let terminal = usize::try_from(global * 2 + 1).unwrap();
        match attack {
            NativeIdentityAttack::Model => replace_json_value(
                &mut lines[terminal],
                "actualModelRevision",
                &format!("\"{forged_model}\""),
            ),
            NativeIdentityAttack::Deployment => replace_json_value(
                &mut lines[terminal],
                "deploymentCommitment",
                &format!("\"{deployment_commitment}\""),
            ),
        }
    }
    let ledger = format!("{}\n", lines.join("\n")).into_bytes();
    let arm_ends = [
        usize::try_from(manifests[0].provider_request_attempt_count * 2).unwrap(),
        lines.len(),
    ];
    let mut root = [0_u8; 32];
    let mut byte_end = 0_usize;
    for (index, line) in lines.iter().enumerate() {
        let leaf = Sha256::digest(line.as_bytes());
        let mut hasher = Sha256::new();
        hasher.update(root);
        hasher.update(leaf);
        root = hasher.finalize().into();
        byte_end += line.len() + 1;
        if let Some(arm) = arm_ends.iter().position(|end| index + 1 == *end) {
            manifests[arm].broker_attempt_ledger_sha256 = test_sha256(&ledger[..byte_end]);
            manifests[arm].attempt_index_root_sha256 =
                root.iter().map(|byte| format!("{byte:02x}")).collect();
        }
    }
    fs::write(ledger_path, ledger).unwrap();

    for ordinal in [1_u8, 2] {
        let index = usize::from(ordinal - 1);
        let sidecar_path = coordinator.join(format!("run-{ordinal}-broker-snapshot.json"));
        let mut sidecar: crate::proof_archive::BrokerSnapshotSidecar =
            serde_json::from_slice(&fs::read(&sidecar_path).unwrap()).unwrap();
        sidecar.attempt_index_file_sha256 = manifests[index].broker_attempt_ledger_sha256.clone();
        sidecar.attempt_index_merkle_root = manifests[index].attempt_index_root_sha256.clone();
        if ordinal == 1 {
            for completion in &mut sidecar.completions {
                match attack {
                    NativeIdentityAttack::Model => {
                        completion.actual_model = Some(forged_model.to_string())
                    }
                    NativeIdentityAttack::Deployment => {
                        completion.deployment_or_fingerprint = Some(raw_deployment.to_string())
                    }
                }
            }
        }
        let sidecar_bytes = serde_json::to_vec(&sidecar).unwrap();
        fs::write(sidecar_path, &sidecar_bytes).unwrap();
        let index_path = coordinator.join(format!("run-{ordinal}-postprocess-index.json"));
        let mut archive: crate::proof_archive::ArmPostprocessIndex =
            serde_json::from_slice(&fs::read(&index_path).unwrap()).unwrap();
        archive
            .sidecars
            .iter_mut()
            .find(|entry| {
                entry.kind == crate::proof_archive::PostprocessSidecarKind::BrokerSnapshot
            })
            .unwrap()
            .sha256 = test_sha256(&sidecar_bytes);
        let archive_bytes =
            crate::jcs::canonicalize_value(&serde_json::to_value(&archive).unwrap()).unwrap();
        fs::write(index_path, &archive_bytes).unwrap();
        manifests[index].postprocess_evidence_index_sha256 = test_sha256(&archive_bytes);
    }

    let manifest_bytes: [Vec<u8>; 2] = std::array::from_fn(|index| {
        let bytes = serde_json::to_vec_pretty(&manifests[index]).unwrap();
        fs::write(&manifest_paths[index], &bytes).unwrap();
        bytes
    });
    let manifest_shas = manifest_bytes.each_ref().map(|bytes| test_sha256(bytes));
    let receipt_dir = coordinator.join("receipts");
    let arm_paths = [
        receipt_dir.join("arm-1-receipt.json"),
        receipt_dir.join("arm-2-receipt.json"),
    ];
    let mut arm_receipts = arm_paths
        .each_ref()
        .map(|path| serde_json::from_slice::<crate::ArmReceipt>(&fs::read(path).unwrap()).unwrap());
    for index in 0..2 {
        arm_receipts[index].attempt_index_file_sha256 =
            manifests[index].broker_attempt_ledger_sha256.clone();
        arm_receipts[index].attempt_index_merkle_root =
            manifests[index].attempt_index_root_sha256.clone();
        arm_receipts[index].run_manifest_sha256 = Some(manifest_shas[index].clone());
    }
    let first_receipt_sha = write_receipt(&arm_paths[0], &arm_receipts[0]);
    arm_receipts[1].previous_arm_receipt_sha256 = Some(first_receipt_sha.clone());
    let second_receipt_sha = write_receipt(&arm_paths[1], &arm_receipts[1]);
    let pair_receipt_path = receipt_dir.join("pair-receipt.json");
    let mut pair_receipt: crate::PairReceipt =
        serde_json::from_slice(&fs::read(&pair_receipt_path).unwrap()).unwrap();
    pair_receipt.first_arm_receipt_sha256 = first_receipt_sha;
    pair_receipt.second_arm_receipt_sha256 = second_receipt_sha;
    pair_receipt.first_run_manifest_sha256 = Some(manifest_shas[0].clone());
    pair_receipt.second_run_manifest_sha256 = Some(manifest_shas[1].clone());
    pair_receipt.final_attempt_index_root = manifests[1].attempt_index_root_sha256.clone();
    write_receipt(&pair_receipt_path, &pair_receipt);

    let pair_path = coordinator.join("pair-verification.json");
    let mut pair: serde_json::Value =
        serde_json::from_slice(&fs::read(&pair_path).unwrap()).unwrap();
    for (manifest, sha) in manifests.iter().zip(manifest_shas) {
        let field = match manifest.condition {
            crate::EvaluationCondition::Generic => "genericRunManifestSha256",
            crate::EvaluationCondition::Candidate => "candidateRunManifestSha256",
        };
        pair[field] = serde_json::json!(sha);
    }
    let pair: crate::blind_verify::NativePairVerification = serde_json::from_value(pair).unwrap();
    fs::write(pair_path, serde_json::to_vec_pretty(&pair).unwrap()).unwrap();
    rebuild_inventory(private_root);
}

#[test]
fn sealed_replay_pair_reaches_blind_bundle_stage() {
    let fixture = replay_fixture_with_material();
    let replay = run_frozen_replay_pair_from(fixture.path()).unwrap();
    let args = replay_blind_args(&replay.private_root);
    assert_complete_core(
        semantic_core(&args),
        &replay.manifests,
        &replay.private_root.join("replay-coordinator"),
        /*native*/ false,
    );
    assert_current_stage(args, &replay.private_root);
}

fn run_native_identity_attack_pair() -> NativeMockPairTestRun {
    let max_total_tokens_per_run = 10;
    let native = run_native_mock_pair_with_marker(/*marker*/ None, max_total_tokens_per_run);
    native.result.as_ref().unwrap();
    native
}

#[test]
fn sealed_native_pair_reaches_blind_bundle_stage() {
    let native = run_native_identity_attack_pair();
    let args = native_blind_args(&native.live_root);
    let manifests = read_native_mock_manifests(&native.live_root);
    assert_complete_core(
        semantic_core(&args),
        &manifests,
        &native.live_root.join("coordinator"),
        /*native*/ true,
    );
    assert_current_stage(args, &native.live_root);
}

#[test]
fn sealed_native_pair_reaches_b2_receipt_semantics() {
    let native = run_native_identity_attack_pair();
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
    let alternate = crate::build_shared_config("replay-fixture", /*broker_port*/ 2).unwrap();
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
    let native = run_native_identity_attack_pair();
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

#[test]
fn sealed_replay_pair_rejects_resigned_cross_arm_request_base_drift() {
    let fixture = replay_fixture_with_material();
    let replay = run_frozen_replay_pair_from(fixture.path()).unwrap();
    let candidate = replay
        .manifests
        .iter()
        .find(|manifest| manifest.condition == crate::EvaluationCondition::Candidate)
        .unwrap();
    let path = replay_manifest_path(&replay.private_root, candidate.run_ordinal);
    let mut manifest: crate::RunManifest =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest.normalized_first_root_base_commitment = "f".repeat(64);
    fs::write(path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    resign_replay_pair(&replay.private_root, |_| {});

    assert_semantic_rejection(
        replay_blind_args(&replay.private_root),
        &replay.private_root,
        "sealed arms differ outside the authorized Skill treatment and outputs",
    );
}

#[test]
fn sealed_replay_pair_rejects_resigned_cross_arm_catalog_drift() {
    let fixture = replay_fixture_with_material();
    let replay = run_frozen_replay_pair_from(fixture.path()).unwrap();
    let candidate = replay
        .manifests
        .iter()
        .find(|manifest| manifest.condition == crate::EvaluationCondition::Candidate)
        .unwrap();
    let ordinal = candidate.run_ordinal;
    let base_dir = replay
        .private_root
        .join("replay-candidate-home/.codex/skills/forged-base");
    fs::create_dir_all(&base_dir).unwrap();
    let base_path = base_dir.join("SKILL.md");
    fs::write(
        &base_path,
        b"---\nname: forged-base\ndescription: forged\n---\n",
    )
    .unwrap();
    let base_path = base_path.canonicalize().unwrap();
    let mut snapshots = Vec::new();
    let mut sidecars = Vec::new();
    for leaf in ["pre-catalog.json", "post-catalog.json"] {
        let path = replay_coordinator(&replay.private_root).join(format!("run-{ordinal}-{leaf}"));
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        let skills = value["response"]["data"][0]["skills"]
            .as_array_mut()
            .unwrap();
        let mut forged = skills[0].clone();
        forged["name"] = serde_json::json!("forged-base");
        forged["description"] = serde_json::json!("forged base catalog entry");
        forged["path"] = serde_json::json!(base_path);
        skills.push(forged);
        let sidecar: crate::proof_archive::CatalogSidecar = serde_json::from_value(value).unwrap();
        let roots = crate::CatalogRoots {
            codex_home: sidecar.roots.codex_home.clone(),
            host_home: sidecar.roots.host_home.clone(),
            case_dir: sidecar.roots.case_dir.clone(),
        };
        snapshots.push(crate::normalize_catalog(&sidecar.response, &roots).unwrap());
        sidecars.push((leaf, serde_json::to_vec(&sidecar).unwrap()));
    }
    assert_eq!(snapshots[0], snapshots[1]);
    let manifest_path = replay_manifest_path(&replay.private_root, ordinal);
    let mut manifest: crate::RunManifest =
        serde_json::from_slice(&fs::read(manifest_path).unwrap()).unwrap();
    manifest.pre_skill_catalog_sha256 = snapshots[0].sha256.clone();
    manifest.post_skill_catalog_sha256 = snapshots[1].sha256.clone();
    manifest.normalized_base_catalog_sha256 = snapshots[0].normalized_base_catalog_sha256();
    for (leaf, bytes) in sidecars {
        resign_replay_sidecar(&replay.private_root, ordinal, leaf, &bytes, &mut manifest);
    }
    resign_replay_pair(&replay.private_root, |_| {});

    assert_semantic_rejection(
        replay_blind_args(&replay.private_root),
        &replay.private_root,
        "sealed arms differ outside the authorized Skill treatment and outputs",
    );
}

#[test]
fn sealed_replay_pair_rejects_resigned_archive_semantic_drift() {
    let fixture = replay_fixture_with_material();
    let replay = run_frozen_replay_pair_from(fixture.path()).unwrap();
    let ordinal = 1;
    let quiet_path =
        replay_coordinator(&replay.private_root).join(format!("run-{ordinal}-quiet-tree.json"));
    let mut quiet: crate::proof_archive::QuietTreeSidecar =
        serde_json::from_slice(&fs::read(quiet_path).unwrap()).unwrap();
    quiet.schema_version = 2;
    let manifest_path = replay_manifest_path(&replay.private_root, ordinal);
    let mut manifest: crate::RunManifest =
        serde_json::from_slice(&fs::read(manifest_path).unwrap()).unwrap();
    resign_replay_sidecar(
        &replay.private_root,
        ordinal,
        "quiet-tree.json",
        &serde_json::to_vec(&quiet).unwrap(),
        &mut manifest,
    );
    resign_replay_pair(&replay.private_root, |_| {});

    assert_semantic_rejection(
        replay_blind_args(&replay.private_root),
        &replay.private_root,
        "quiet tree evidence is invalid",
    );
}

#[test]
fn sealed_replay_pair_rejects_native_ledger_and_empty_receipts_path() {
    for artifact in ["ledger", "receipts"] {
        let fixture = replay_fixture_with_material();
        let replay = run_frozen_replay_pair_from(fixture.path()).unwrap();
        if artifact == "ledger" {
            crate::secure_fs::write_owner_only_new(
                &replay.private_root.join("coordinator/attempt-index.jsonl"),
                b"present",
            )
            .unwrap();
        } else {
            crate::secure_fs::create_owner_only_dir_new(
                &replay.private_root.join("coordinator/receipts"),
            )
            .unwrap();
        }
        rebuild_inventory(&replay.private_root);

        assert_semantic_rejection(
            replay_blind_args(&replay.private_root),
            &replay.private_root,
            "replay native proof artifact is present",
        );
    }
}

#[test]
fn sealed_native_pair_rejects_fully_resigned_cross_arm_model_drift() {
    let native = run_native_identity_attack_pair();
    resign_native_identity(&native.live_root, NativeIdentityAttack::Model);
    assert_semantic_rejection(
        native_blind_args(&native.live_root),
        &native.live_root,
        "sealed arms differ outside the authorized Skill treatment and outputs",
    );
}

#[test]
fn sealed_native_pair_rejects_fully_resigned_cross_arm_deployment_drift() {
    let native = run_native_identity_attack_pair();
    resign_native_identity(&native.live_root, NativeIdentityAttack::Deployment);
    assert_semantic_rejection(
        native_blind_args(&native.live_root),
        &native.live_root,
        "sealed arms differ outside the authorized Skill treatment and outputs",
    );
}
