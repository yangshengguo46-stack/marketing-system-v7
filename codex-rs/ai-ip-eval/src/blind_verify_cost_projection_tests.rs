use std::path::Path;

use pretty_assertions::assert_eq;
use serde::de::DeserializeOwned;

use super::*;

fn receipt<T: DeserializeOwned>(path: &Path) -> (T, Vec<u8>) {
    let raw = fs::read(path).unwrap();
    let body = raw.strip_suffix(b"\n").unwrap();
    (serde_json::from_slice(body).unwrap(), raw)
}

fn expected_native_projection(private_root: &Path) -> crate::cost_authority::CostPairProjection {
    let coordinator = private_root.join("coordinator");
    let execution_bytes = fs::read(coordinator.join("execution-context.json")).unwrap();
    let execution: serde_json::Value = serde_json::from_slice(&execution_bytes).unwrap();
    let (pair_receipt, pair_receipt_bytes): (crate::PairReceipt, _) =
        receipt(&coordinator.join("receipts/pair-receipt.json"));

    let arms = [1_u8, 2].map(|ordinal| {
        let manifest_bytes =
            fs::read(coordinator.join(format!("run-{ordinal}-manifest.json"))).unwrap();
        let manifest: crate::RunManifest = serde_json::from_slice(&manifest_bytes).unwrap();
        let (mut arm_receipt, arm_receipt_bytes): (crate::ArmReceipt, _) =
            receipt(&coordinator.join(format!("receipts/arm-{ordinal}-receipt.json")));
        let manifest_sha256 = test_sha256(&manifest_bytes);
        let arm_receipt_sha256 = test_sha256(&arm_receipt_bytes);
        arm_receipt.receipt_sha256 = arm_receipt_sha256.clone();
        let ledger = crate::proof_ledger::VerifiedLedgerArm {
            condition: manifest.condition,
            global_start_inclusive: arm_receipt.global_attempt_start_inclusive,
            global_end_exclusive: arm_receipt.global_attempt_end_exclusive,
            provider_request_attempt_count: manifest.provider_request_attempt_count,
            provider_completed_response_count: manifest.provider_completed_response_count,
            raw_response_count: manifest.raw_response_count,
            usage: manifest.usage.clone(),
            attempt_index_prefix_sha256: manifest.broker_attempt_ledger_sha256.clone(),
            attempt_index_prefix_root_sha256: manifest.attempt_index_root_sha256.clone(),
            run_manifest_sha256: manifest_sha256.clone(),
        };
        crate::cost_authority::CostArmProjection {
            mode_evidence: manifest.mode_evidence.clone(),
            manifest,
            manifest_bytes,
            manifest_sha256,
            ledger,
            arm_receipt,
            arm_receipt_sha256,
        }
    });

    crate::cost_authority::CostPairProjection {
        execution_mode: crate::ExecutionMode::Mock,
        execution_context_sha256: test_sha256(&execution_bytes),
        started_at: execution["startedAt"].as_str().unwrap().to_string(),
        deadline: execution["deadline"].as_str().unwrap().to_string(),
        arm_order_commitment: execution["armOrderCommitment"]
            .as_str()
            .unwrap()
            .to_string(),
        provider_endpoint_commitment: None,
        pair_receipt_sha256: test_sha256(&pair_receipt_bytes),
        finished_at: pair_receipt.finished_at.clone(),
        attempt_ledger_sha256: test_sha256(
            &fs::read(coordinator.join("attempt-index.jsonl")).unwrap(),
        ),
        attempt_index_root_sha256: pair_receipt.final_attempt_index_root.clone(),
        pair_receipt,
        arms,
    }
}

#[test]
fn cost_projection_retains_exact_verified_native_evidence() {
    let native = run_native_identity_attack_pair();
    native.result.as_ref().unwrap();
    let core = semantic_core(&native_blind_args(&native.live_root));
    let projection = core.cost_projection.as_ref().unwrap();
    let expected = expected_native_projection(&native.live_root);

    assert_eq!(projection, &expected);
    assert_eq!(projection.finished_at, projection.pair_receipt.finished_at);
    for (index, arm) in projection.arms.iter().enumerate() {
        assert_eq!(arm.manifest.run_ordinal, u8::try_from(index + 1).unwrap());
        assert_eq!(arm.manifest.condition, arm.ledger.condition);
        assert_eq!(arm.manifest.condition, arm.arm_receipt.condition);
        assert_eq!(arm.manifest.run_ordinal, arm.arm_receipt.run_ordinal);
        assert_eq!(arm.mode_evidence, arm.manifest.mode_evidence);
        assert_eq!(
            arm.arm_receipt.sealed_at,
            expected.arms[index].arm_receipt.sealed_at
        );
        assert_ne!(arm.arm_receipt_sha256, projection.pair_receipt_sha256);
    }
}

#[test]
fn cost_projection_is_absent_for_replay() {
    let fixture = replay_fixture_with_material();
    let replay = run_frozen_replay_pair_from(fixture.path()).unwrap();
    let core = semantic_core(&replay_blind_args(&replay.private_root));

    assert_eq!(core.cost_projection, None);
    assert_eq!(core.native_pair_receipt_raw_sha256, None);
}
