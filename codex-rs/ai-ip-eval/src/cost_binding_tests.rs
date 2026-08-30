use std::fs;

use chrono::DateTime;
use chrono::Utc;
use pretty_assertions::assert_eq;
use sha2::Digest;

use crate::EvaluationCondition;

fn binding_path(world: &crate::cost_authority_tests::CostTransactionWorld) -> std::path::PathBuf {
    binding_path_for(world, EvaluationCondition::Candidate)
}

fn binding_path_for(
    world: &crate::cost_authority_tests::CostTransactionWorld,
    condition: EvaluationCondition,
) -> std::path::PathBuf {
    world.root().join(match condition {
        EvaluationCondition::Generic => "coordinator/cost/generic-binding.json",
        EvaluationCondition::Candidate => "coordinator/cost/candidate-binding.json",
    })
}

fn inventory_bytes(world: &crate::cost_authority_tests::CostTransactionWorld) -> Vec<u8> {
    fs::read(world.root().join("coordinator/private-inventory.jsonl")).unwrap()
}

fn write_new_owner_only(path: &std::path::Path, bytes: &[u8]) {
    crate::secure_fs::write_owner_only_new(path, bytes).unwrap();
}

fn assert_error_contains(error: anyhow::Error, expected: &str) {
    assert!(error.to_string().contains(expected), "{error:#}");
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
fn cost_binding_transaction_publishes_generic_inventory_covered_sidecar() {
    let world = crate::cost_authority_tests::generic_cost_binding_world();
    let receipt_before = fs::read(world.receipt_path()).unwrap();
    let manifests_before = manifest_paths(&world).map(|path| fs::read(path).unwrap());
    let clock = crate::cost_authority_tests::cost_binding_clock();
    crate::cost_binding::publish_cost_binding(
        crate::cost_authority_tests::cost_binding_authority(world.root()),
        EvaluationCondition::Generic,
        &clock,
    )
    .unwrap();

    let binding_bytes = fs::read(binding_path_for(&world, EvaluationCondition::Generic)).unwrap();
    let binding: crate::cost_binding::CostBindingV1 = serde_json::from_slice(&binding_bytes).unwrap();
    assert_eq!(binding.condition, EvaluationCondition::Generic);
    assert_eq!(binding.cost_receipt_sha256, format!("{:x}", sha2::Sha256::digest(&receipt_before)));
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
    let authority = crate::cost_authority_tests::cost_binding_authority(world.root());
    let bytes = fs::read(world.receipt_path()).unwrap();
    fs::write(world.receipt_path(), vec![b' '; bytes.len()]).unwrap();
    let error = crate::cost_binding::publish_cost_binding(
        authority,
        EvaluationCondition::Candidate,
        &crate::cost_authority_tests::cost_binding_clock(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("parse unique-key JSON"));
    assert!(!binding_path(&world).exists());
}

#[test]
fn cost_binding_transaction_rejects_receipt_commitment_and_shape_mutations() {
    let cases: [(&str, Box<dyn Fn(&mut serde_json::Value)>, &str); 3] = [
        (
            "wrong condition",
            Box::new(|value| value["condition"] = "generic".into()),
            "retained cost receipt condition differs from binding condition",
        ),
        (
            "execution manifest drift",
            Box::new(|value| value["executionManifestSha256"] = "0".repeat(64).into()),
            "sealed execution manifest SHA-256 differs from retained cost receipt commitment",
        ),
        (
            "broker receipt drift",
            Box::new(|value| value["brokerReceiptSha256"] = "0".repeat(64).into()),
            "private tree differs from its inventory records",
        ),
    ];
    for (name, edit, expected) in cases {
        let world = crate::cost_authority_tests::cost_binding_world(false);
        let authority = crate::cost_authority_tests::cost_binding_authority(world.root());
        rewrite_receipt(&world, edit);
        let error = crate::cost_binding::publish_cost_binding(
            authority,
            EvaluationCondition::Candidate,
            &crate::cost_authority_tests::cost_binding_clock(),
        )
        .unwrap_err();
        assert!(!binding_path(&world).exists(), "{name}");
        assert_error_contains(error, expected);
    }
}

#[test]
fn cost_binding_transaction_rejects_absent_and_noncanonical_receipts() {
    let absent = crate::cost_authority_tests::cost_binding_world(false);
    let absent_authority = crate::cost_authority_tests::cost_binding_authority(absent.root());
    fs::remove_file(absent.receipt_path()).unwrap();
    let error = crate::cost_binding::publish_cost_binding(
        absent_authority,
        EvaluationCondition::Candidate,
        &crate::cost_authority_tests::cost_binding_clock(),
    )
    .unwrap_err();
    assert_error_contains(error, "open retained private file");
    assert!(!binding_path(&absent).exists());

    let malformed = crate::cost_authority_tests::cost_binding_world(false);
    let malformed_authority = crate::cost_authority_tests::cost_binding_authority(malformed.root());
    let receipt = fs::read(malformed.receipt_path()).unwrap();
    fs::write(malformed.receipt_path(), [b" ".as_slice(), receipt.as_slice()].concat()).unwrap();
    let error = crate::cost_binding::publish_cost_binding(
        malformed_authority,
        EvaluationCondition::Candidate,
        &crate::cost_authority_tests::cost_binding_clock(),
    )
    .unwrap_err();
    assert_error_contains(error, "generated cost receipt is not exact JCS");
    assert!(!binding_path(&malformed).exists());
}

#[test]
fn cost_binding_transaction_rejects_manifest_bytes_drifting_from_receipt_commitment() {
    let world = crate::cost_authority_tests::cost_binding_world(false);
    let authority = crate::cost_authority_tests::cost_binding_authority(world.root());
    fs::write(&manifest_paths(&world)[1], b"manifest changed").unwrap();
    let error = crate::cost_binding::publish_cost_binding(
        authority,
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
fn cost_binding_transaction_rejects_supplier_replacement_after_retain_before_append() {
    let world = crate::cost_authority_tests::cost_binding_world(true);
    let receipt_before = fs::read(world.receipt_path()).unwrap();
    let manifests_before = manifest_paths(&world).map(|path| fs::read(path).unwrap());
    let inventory_before = inventory_bytes(&world);
    let clock = crate::cost_authority_tests::cost_binding_clock();
    let error = crate::cost_binding::publish_cost_binding_for_test(
        crate::cost_authority_tests::cost_binding_authority(world.root()),
        EvaluationCondition::Candidate,
        &clock,
        &mut |checkpoint| {
            if checkpoint == crate::cost_binding::CostBindingCheckpoint::AfterCreate {
                let path = world.root().join("inputs/supplier-statements/candidate.json");
                fs::remove_file(&path)?;
                crate::secure_fs::write_owner_only_new(&path, b"replacement")?;
            }
            Ok(())
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("retained private file"), "{error:#}");
    assert!(binding_path(&world).exists());
    assert_eq!(inventory_bytes(&world), inventory_before);
    assert_eq!(fs::read(world.receipt_path()).unwrap(), receipt_before);
    assert_eq!(manifest_paths(&world).map(|path| fs::read(path).unwrap()), manifests_before);
    assert_eq!(clock.calls(), 1);
}

#[test]
fn cost_binding_transaction_rejects_changed_binding_before_expected_sha_append() {
    let world = crate::cost_authority_tests::cost_binding_world(false);
    let receipt_before = fs::read(world.receipt_path()).unwrap();
    let manifests_before = manifest_paths(&world).map(|path| fs::read(path).unwrap());
    let inventory_before = inventory_bytes(&world);
    let clock = crate::cost_authority_tests::cost_binding_clock();
    let error = crate::cost_binding::publish_cost_binding_for_test(
        crate::cost_authority_tests::cost_binding_authority(world.root()),
        EvaluationCondition::Candidate,
        &clock,
        &mut |checkpoint| {
            if checkpoint == crate::cost_binding::CostBindingCheckpoint::BeforeAppend {
                let path = binding_path(&world);
                fs::remove_file(&path)?;
                write_new_owner_only(&path, b"changed binding");
            }
            Ok(())
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("inventory append target is absent, recorded, or mismatched"), "{error:#}");
    assert_eq!(inventory_bytes(&world), inventory_before);
    assert_eq!(fs::read(world.receipt_path()).unwrap(), receipt_before);
    assert_eq!(manifest_paths(&world).map(|path| fs::read(path).unwrap()), manifests_before);
    assert_eq!(clock.calls(), 1);
}

#[test]
fn cost_binding_transaction_rejects_extra_pending_leaf_and_retained_binding_bytes_change() {
    for (name, inject, expected) in [
        (
            "extra leaf",
            Box::new(|world: &crate::cost_authority_tests::CostTransactionWorld| {
                write_new_owner_only(&world.root().join("coordinator/cost/unexpected.json"), b"extra");
            }) as Box<dyn Fn(&crate::cost_authority_tests::CostTransactionWorld)>,
            "pending cost binding inventory contains an unexpected path or changed state",
        ),
        (
            "changed binding bytes",
            Box::new(|world: &crate::cost_authority_tests::CostTransactionWorld| {
                let path = binding_path(world);
                fs::remove_file(&path).unwrap();
                write_new_owner_only(&path, b"different binding bytes");
            }),
            "pending cost binding filesystem SHA-256 differs",
        ),
    ] {
        let world = crate::cost_authority_tests::cost_binding_world(false);
        let receipt_before = fs::read(world.receipt_path()).unwrap();
        let manifests_before = manifest_paths(&world).map(|path| fs::read(path).unwrap());
        let inventory_before = inventory_bytes(&world);
        let clock = crate::cost_authority_tests::cost_binding_clock();
        let error = crate::cost_binding::publish_cost_binding_for_test(
            crate::cost_authority_tests::cost_binding_authority(world.root()),
            EvaluationCondition::Candidate,
            &clock,
            &mut |checkpoint| {
                if checkpoint == crate::cost_binding::CostBindingCheckpoint::AfterCreate {
                    inject(&world);
                }
                Ok(())
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains(expected), "{name}: {error:#}");
        assert_eq!(inventory_bytes(&world), inventory_before, "{name}");
        assert_eq!(fs::read(world.receipt_path()).unwrap(), receipt_before, "{name}");
        assert_eq!(manifest_paths(&world).map(|path| fs::read(path).unwrap()), manifests_before, "{name}");
        assert_eq!(clock.calls(), 1, "{name}");
    }
}

#[test]
fn cost_binding_transaction_rejects_after_create_failure_without_rerun_overwrite() {
    let world = crate::cost_authority_tests::cost_binding_world(false);
    let rerun_authority = crate::cost_authority_tests::cost_binding_authority(world.root());
    let inventory_before = inventory_bytes(&world);
    let first_clock = crate::cost_authority_tests::cost_binding_clock();
    let error = crate::cost_binding::publish_cost_binding_for_test(
        crate::cost_authority_tests::cost_binding_authority(world.root()),
        EvaluationCondition::Candidate,
        &first_clock,
        &mut |checkpoint| match checkpoint {
            crate::cost_binding::CostBindingCheckpoint::AfterCreate => anyhow::bail!("injected after-create failure"),
            _ => Ok(()),
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("injected after-create failure"), "{error:#}");
    let binding_before = fs::read(binding_path(&world)).unwrap();
    let rerun_clock = crate::cost_authority_tests::cost_binding_clock();
    let rerun = crate::cost_binding::publish_cost_binding(
        rerun_authority,
        EvaluationCondition::Candidate,
        &rerun_clock,
    )
    .unwrap_err();
    assert!(rerun.to_string().contains("cost binding destination already exists"), "{rerun:#}");
    assert_eq!(fs::read(binding_path(&world)).unwrap(), binding_before);
    assert_eq!(inventory_bytes(&world), inventory_before);
    assert_eq!(first_clock.calls(), 1);
    assert_eq!(rerun_clock.calls(), 0);
}

#[test]
fn cost_binding_transaction_rejects_manifest_mutation_after_fresh_rebuild() {
    let world = crate::cost_authority_tests::cost_binding_world(false);
    let receipt_before = fs::read(world.receipt_path()).unwrap();
    let inventory_before = inventory_bytes(&world);
    let clock = crate::cost_authority_tests::cost_binding_clock();
    let error = crate::cost_binding::publish_cost_binding_for_test(
        crate::cost_authority_tests::cost_binding_authority(world.root()),
        EvaluationCondition::Candidate,
        &clock,
        &mut |checkpoint| {
            if checkpoint == crate::cost_binding::CostBindingCheckpoint::AfterFreshInventory {
                fs::write(&manifest_paths(&world)[0], b"changed manifest")?;
            }
            Ok(())
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("run manifest bytes changed during cost binding transaction"), "{error:#}");
    assert!(binding_path(&world).exists());
    assert_ne!(inventory_bytes(&world), inventory_before);
    assert_eq!(fs::read(world.receipt_path()).unwrap(), receipt_before);
    assert_eq!(clock.calls(), 1);
}

#[test]
fn cost_binding_transaction_rejects_receipt_mutation_after_fresh_rebuild() {
    let world = crate::cost_authority_tests::cost_binding_world(false);
    let manifests_before = manifest_paths(&world).map(|path| fs::read(path).unwrap());
    let clock = crate::cost_authority_tests::cost_binding_clock();
    let error = crate::cost_binding::publish_cost_binding_for_test(
        crate::cost_authority_tests::cost_binding_authority(world.root()),
        EvaluationCondition::Candidate,
        &clock,
        &mut |checkpoint| {
            if checkpoint == crate::cost_binding::CostBindingCheckpoint::AfterFreshInventory {
                let receipt = fs::read(world.receipt_path())?;
                fs::write(world.receipt_path(), vec![b' '; receipt.len()])?;
            }
            Ok(())
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("retained private file"), "{error:#}");
    assert!(binding_path(&world).exists());
    assert_eq!(manifest_paths(&world).map(|path| fs::read(path).unwrap()), manifests_before);
    crate::private_inventory::verify_private_inventory(world.root()).unwrap_err();
    assert_eq!(clock.calls(), 1);
}

#[test]
fn cost_binding_transaction_rejects_binding_replacement_after_fresh_rebuild() {
    let world = crate::cost_authority_tests::cost_binding_world(false);
    let receipt_before = fs::read(world.receipt_path()).unwrap();
    let manifests_before = manifest_paths(&world).map(|path| fs::read(path).unwrap());
    let inventory_before = inventory_bytes(&world);
    let clock = crate::cost_authority_tests::cost_binding_clock();
    let replacement = b"post-fresh binding replacement";
    let error = crate::cost_binding::publish_cost_binding_for_test(
        crate::cost_authority_tests::cost_binding_authority(world.root()),
        EvaluationCondition::Candidate,
        &clock,
        &mut |checkpoint| {
            if checkpoint == crate::cost_binding::CostBindingCheckpoint::AfterFreshInventory {
                let path = binding_path(&world);
                fs::remove_file(&path)?;
                crate::secure_fs::write_owner_only_new(&path, replacement)?;
            }
            Ok(())
        },
    )
    .unwrap_err();
    assert_error_contains(error, "retained private file");
    assert_eq!(fs::read(binding_path(&world)).unwrap(), replacement);
    assert_ne!(inventory_bytes(&world), inventory_before);
    assert_eq!(fs::read(world.receipt_path()).unwrap(), receipt_before);
    assert_eq!(manifest_paths(&world).map(|path| fs::read(path).unwrap()), manifests_before);
    crate::private_inventory::verify_private_inventory(world.root()).unwrap_err();
    assert_eq!(clock.calls(), 1);
}

#[test]
fn cost_binding_transaction_rejects_supplier_replacement_after_fresh_rebuild() {
    let world = crate::cost_authority_tests::cost_binding_world(true);
    let receipt_before = fs::read(world.receipt_path()).unwrap();
    let manifests_before = manifest_paths(&world).map(|path| fs::read(path).unwrap());
    let inventory_before = inventory_bytes(&world);
    let clock = crate::cost_authority_tests::cost_binding_clock();
    let replacement = b"post-fresh supplier replacement";
    let error = crate::cost_binding::publish_cost_binding_for_test(
        crate::cost_authority_tests::cost_binding_authority(world.root()),
        EvaluationCondition::Candidate,
        &clock,
        &mut |checkpoint| {
            if checkpoint == crate::cost_binding::CostBindingCheckpoint::AfterFreshInventory {
                let path = world.root().join("inputs/supplier-statements/candidate.json");
                fs::remove_file(&path)?;
                crate::secure_fs::write_owner_only_new(&path, replacement)?;
            }
            Ok(())
        },
    )
    .unwrap_err();
    assert_error_contains(error, "retained private file");
    assert!(binding_path(&world).exists());
    assert_eq!(fs::read(world.root().join("inputs/supplier-statements/candidate.json")).unwrap(), replacement);
    assert_ne!(inventory_bytes(&world), inventory_before);
    assert_eq!(fs::read(world.receipt_path()).unwrap(), receipt_before);
    assert_eq!(manifest_paths(&world).map(|path| fs::read(path).unwrap()), manifests_before);
    crate::private_inventory::verify_private_inventory(world.root()).unwrap_err();
    assert_eq!(clock.calls(), 1);
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
        ("2026-08-30T09:59:59.999Z", "calculation is before finish"),
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
    let unknown_authority = crate::cost_authority_tests::cost_binding_authority(unknown.root());
    rewrite_receipt(&unknown, |value| value["unknown"] = true.into());
    let error = crate::cost_binding::publish_cost_binding(
        unknown_authority,
        EvaluationCondition::Candidate,
        &crate::cost_authority_tests::cost_binding_clock(),
    )
    .unwrap_err();
    assert_error_contains(error, "schema");
    assert!(!binding_path(&unknown).exists());

    let duplicate = crate::cost_authority_tests::cost_binding_world(false);
    let duplicate_authority = crate::cost_authority_tests::cost_binding_authority(duplicate.root());
    let mut duplicate_bytes = fs::read(duplicate.receipt_path()).unwrap();
    assert_eq!(duplicate_bytes.pop(), Some(b'}'));
    duplicate_bytes.extend_from_slice(b",\"condition\":\"candidate\"}");
    fs::write(duplicate.receipt_path(), duplicate_bytes).unwrap();
    let error = crate::cost_binding::publish_cost_binding(
        duplicate_authority,
        EvaluationCondition::Candidate,
        &crate::cost_authority_tests::cost_binding_clock(),
    )
    .unwrap_err();
    assert_error_contains(error, "parse unique-key JSON");
    assert!(!binding_path(&duplicate).exists());

    let trailing = crate::cost_authority_tests::cost_binding_world(false);
    let trailing_authority = crate::cost_authority_tests::cost_binding_authority(trailing.root());
    let mut trailing_bytes = fs::read(trailing.receipt_path()).unwrap();
    trailing_bytes.extend_from_slice(b" trailing");
    fs::write(trailing.receipt_path(), trailing_bytes).unwrap();
    let error = crate::cost_binding::publish_cost_binding(
        trailing_authority,
        EvaluationCondition::Candidate,
        &crate::cost_authority_tests::cost_binding_clock(),
    )
    .unwrap_err();
    assert_error_contains(error, "reject trailing JSON");
    assert!(!binding_path(&trailing).exists());
}
