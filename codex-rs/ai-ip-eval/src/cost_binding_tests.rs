use std::fs;

use chrono::DateTime;
use chrono::Utc;
use pretty_assertions::assert_eq;
use sha2::Digest;

use crate::EvaluationCondition;

fn binding_path(world: &crate::cost_authority_tests::CostTransactionWorld) -> std::path::PathBuf {
    world.root().join("coordinator/cost/candidate-binding.json")
}

fn manifest_paths(
    world: &crate::cost_authority_tests::CostTransactionWorld,
) -> [std::path::PathBuf; 2] {
    [
        world.root().join("coordinator/run-1-manifest.json"),
        world.root().join("coordinator/run-2-manifest.json"),
    ]
}

fn rewrite_receipt(
    world: &crate::cost_authority_tests::CostTransactionWorld,
    edit: impl FnOnce(&mut serde_json::Value),
) {
    let mut value = crate::jcs::parse_json(&fs::read(world.receipt_path()).unwrap()).unwrap();
    edit(&mut value);
    fs::write(
        world.receipt_path(),
        crate::jcs::canonicalize_value(&value).unwrap(),
    )
    .unwrap();
}

struct BindingClock(DateTime<Utc>);

impl crate::cost_authority::CostClock for BindingClock {
    fn now(&self) -> anyhow::Result<DateTime<Utc>> {
        Ok(self.0)
    }
}

fn binding_clock(value: &str) -> BindingClock {
    BindingClock(DateTime::parse_from_rfc3339(value).unwrap().with_timezone(&Utc))
}

#[test]
fn cost_binding_transaction_publishes_exact_inventory_covered_sidecar() {
    let world = crate::cost_authority_tests::cost_binding_world(false);
    let receipt_before = fs::read(world.receipt_path()).unwrap();
    let manifests_before = manifest_paths(&world).map(|path| fs::read(path).unwrap());
    let clock = crate::cost_authority_tests::cost_binding_clock();
    crate::cost_binding::publish_cost_binding(
        crate::cost_authority_tests::cost_binding_authority(world.root()),
        EvaluationCondition::Candidate,
        &clock,
    )
    .unwrap();

    let binding_bytes = fs::read(binding_path(&world)).unwrap();
    let binding: crate::cost_binding::CostBindingV1 = serde_json::from_slice(&binding_bytes).unwrap();
    let receipt = crate::cost_contracts::validate_cost_receipt(&receipt_before).unwrap();
    assert_eq!(binding.execution_manifest_sha256, receipt.execution_manifest_sha256);
    assert_eq!(binding.broker_receipt_sha256, receipt.broker_receipt_sha256);
    assert_eq!(binding.cost_receipt_sha256, format!("{:x}", sha2::Sha256::digest(&receipt_before)));
    assert_eq!(binding.condition, EvaluationCondition::Candidate);
    assert_eq!(binding.bound_at, "2026-08-30T10:01:00.000Z");
    assert_eq!(binding_bytes, crate::jcs::canonicalize_value(&crate::jcs::parse_json(&binding_bytes).unwrap()).unwrap());
    assert_eq!(fs::read(world.receipt_path()).unwrap(), receipt_before);
    assert_eq!(manifest_paths(&world).map(|path| fs::read(path).unwrap()), manifests_before);
    crate::private_inventory::verify_private_inventory(world.root()).unwrap();
    assert_eq!(clock.calls(), 1);
}

#[test]
fn cost_binding_transaction_refuses_existing_sidecar_without_overwriting() {
    let world = crate::cost_authority_tests::cost_binding_world(false);
    let clock = crate::cost_authority_tests::cost_binding_clock();
    crate::cost_binding::publish_cost_binding(
        crate::cost_authority_tests::cost_binding_authority(world.root()),
        EvaluationCondition::Candidate,
        &clock,
    )
    .unwrap();
    let before = fs::read(binding_path(&world)).unwrap();
    let rejected_clock = crate::cost_authority_tests::cost_binding_clock();
    let error = crate::cost_binding::publish_cost_binding(
        crate::cost_authority_tests::cost_binding_authority(world.root()),
        EvaluationCondition::Candidate,
        &rejected_clock,
    )
    .unwrap_err();
    assert!(error.to_string().contains("cost binding destination already exists"));
    assert_eq!(fs::read(binding_path(&world)).unwrap(), before);
    assert_eq!(rejected_clock.calls(), 0);
}

#[test]
fn cost_binding_transaction_rejects_changed_receipt_without_sidecar() {
    let world = crate::cost_authority_tests::cost_binding_world(false);
    let bytes = fs::read(world.receipt_path()).unwrap();
    fs::write(world.receipt_path(), vec![b' '; bytes.len()]).unwrap();
    let error = crate::cost_binding::publish_cost_binding(
        crate::cost_authority_tests::cost_binding_authority(world.root()),
        EvaluationCondition::Candidate,
        &crate::cost_authority_tests::cost_binding_clock(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("parse unique-key JSON"));
    assert!(!binding_path(&world).exists());
}

#[test]
fn cost_binding_transaction_rejects_receipt_commitment_and_shape_mutations() {
    let cases: [(&str, Box<dyn Fn(&mut serde_json::Value)>); 3] = [
        (
            "wrong condition",
            Box::new(|value| value["condition"] = "generic".into()),
        ),
        (
            "execution manifest drift",
            Box::new(|value| value["executionManifestSha256"] = "0".repeat(64).into()),
        ),
        (
            "broker receipt drift",
            Box::new(|value| value["brokerReceiptSha256"] = "0".repeat(64).into()),
        ),
    ];
    for (name, edit) in cases {
        let world = crate::cost_authority_tests::cost_binding_world(false);
        rewrite_receipt(&world, edit);
        let error = crate::cost_binding::publish_cost_binding(
            crate::cost_authority_tests::cost_binding_authority(world.root()),
            EvaluationCondition::Candidate,
            &crate::cost_authority_tests::cost_binding_clock(),
        )
        .unwrap_err();
        assert!(!binding_path(&world).exists(), "{name}");
        assert!(!error.to_string().is_empty(), "{name}");
    }
}

#[test]
fn cost_binding_transaction_rejects_absent_and_noncanonical_receipts() {
    let absent = crate::cost_authority_tests::cost_binding_world(false);
    fs::remove_file(absent.receipt_path()).unwrap();
    assert!(crate::cost_binding::publish_cost_binding(
        crate::cost_authority_tests::cost_binding_authority(absent.root()),
        EvaluationCondition::Candidate,
        &crate::cost_authority_tests::cost_binding_clock(),
    )
    .is_err());
    assert!(!binding_path(&absent).exists());

    let malformed = crate::cost_authority_tests::cost_binding_world(false);
    let receipt = fs::read(malformed.receipt_path()).unwrap();
    fs::write(malformed.receipt_path(), [b" ".as_slice(), receipt.as_slice()].concat()).unwrap();
    assert!(crate::cost_binding::publish_cost_binding(
        crate::cost_authority_tests::cost_binding_authority(malformed.root()),
        EvaluationCondition::Candidate,
        &crate::cost_authority_tests::cost_binding_clock(),
    )
    .is_err());
    assert!(!binding_path(&malformed).exists());
}

#[test]
fn cost_binding_transaction_rejects_manifest_bytes_drifting_from_receipt_commitment() {
    let world = crate::cost_authority_tests::cost_binding_world(false);
    fs::write(&manifest_paths(&world)[1], b"manifest changed").unwrap();
    let error = crate::cost_binding::publish_cost_binding(
        crate::cost_authority_tests::cost_binding_authority(world.root()),
        EvaluationCondition::Candidate,
        &crate::cost_authority_tests::cost_binding_clock(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("execution manifest SHA-256"), "{error:#}");
    assert!(!binding_path(&world).exists());
}

#[test]
fn cost_binding_transaction_binds_over_ceiling_supplier_receipt_without_rewriting_it() {
    let world = crate::cost_authority_tests::cost_binding_world(true);
    let receipt_before = fs::read(world.receipt_path()).unwrap();
    let receipt = crate::cost_contracts::validate_cost_receipt(&receipt_before).unwrap();
    assert!(!receipt.within_ceilings);
    assert!(receipt.supplier_statement_sha256.is_some());
    crate::cost_binding::publish_cost_binding(
        crate::cost_authority_tests::cost_binding_authority(world.root()),
        EvaluationCondition::Candidate,
        &crate::cost_authority_tests::cost_binding_clock(),
    )
    .unwrap();
    assert_eq!(fs::read(world.receipt_path()).unwrap(), receipt_before);
    assert!(binding_path(&world).exists());
    crate::private_inventory::verify_private_inventory(world.root()).unwrap();
}

#[test]
fn cost_binding_calculation_uses_one_clock_value_without_publication() {
    let world = crate::cost_authority_tests::cost_binding_world(false);
    let clock = crate::cost_authority_tests::cost_binding_clock();
    let binding = crate::cost_binding::commit_cost_binding(
        &crate::cost_authority_tests::cost_binding_authority(world.root()),
        EvaluationCondition::Candidate,
        &clock,
    )
    .unwrap();
    assert_eq!(binding.condition, EvaluationCondition::Candidate);
    assert_eq!(binding.bound_at, "2026-08-30T10:01:00.000Z");
    assert_eq!(clock.calls(), 1);
    assert!(!binding_path(&world).exists());
}

#[test]
fn cost_binding_calculation_rejects_invalid_bound_timeline_without_publication() {
    let cases = [
        ("2026-08-30T10:00:59.999Z", "before retained receipt calculation"),
        ("2026-08-30T11:00:00.000Z", "retention"),
    ];
    for (time, expected) in cases {
        let world = crate::cost_authority_tests::cost_binding_world(false);
        let error = crate::cost_binding::commit_cost_binding(
            &crate::cost_authority_tests::cost_binding_authority(world.root()),
            EvaluationCondition::Candidate,
            &binding_clock(time),
        )
        .unwrap_err();
        assert!(error.to_string().contains(expected), "{error:#}");
        assert!(!binding_path(&world).exists());
    }
}

#[test]
fn cost_binding_transaction_rejects_unknown_duplicate_and_trailing_receipt_json() {
    let unknown = crate::cost_authority_tests::cost_binding_world(false);
    rewrite_receipt(&unknown, |value| value["unknown"] = true.into());
    assert!(crate::cost_binding::publish_cost_binding(
        crate::cost_authority_tests::cost_binding_authority(unknown.root()),
        EvaluationCondition::Candidate,
        &crate::cost_authority_tests::cost_binding_clock(),
    )
    .is_err());
    assert!(!binding_path(&unknown).exists());

    let duplicate = crate::cost_authority_tests::cost_binding_world(false);
    let mut duplicate_bytes = fs::read(duplicate.receipt_path()).unwrap();
    assert_eq!(duplicate_bytes.pop(), Some(b'}'));
    duplicate_bytes.extend_from_slice(b",\"condition\":\"candidate\"}");
    fs::write(duplicate.receipt_path(), duplicate_bytes).unwrap();
    assert!(crate::cost_binding::publish_cost_binding(
        crate::cost_authority_tests::cost_binding_authority(duplicate.root()),
        EvaluationCondition::Candidate,
        &crate::cost_authority_tests::cost_binding_clock(),
    )
    .is_err());
    assert!(!binding_path(&duplicate).exists());

    let trailing = crate::cost_authority_tests::cost_binding_world(false);
    let mut trailing_bytes = fs::read(trailing.receipt_path()).unwrap();
    trailing_bytes.extend_from_slice(b" trailing");
    fs::write(trailing.receipt_path(), trailing_bytes).unwrap();
    assert!(crate::cost_binding::publish_cost_binding(
        crate::cost_authority_tests::cost_binding_authority(trailing.root()),
        EvaluationCondition::Candidate,
        &crate::cost_authority_tests::cost_binding_clock(),
    )
    .is_err());
    assert!(!binding_path(&trailing).exists());
}
