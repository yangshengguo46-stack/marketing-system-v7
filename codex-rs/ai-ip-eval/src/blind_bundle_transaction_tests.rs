use std::fs;

use chrono::DateTime;
use pretty_assertions::assert_eq;
use rand::TryRngCore;
use sha2::Digest;
use sha2::Sha256;

use super::*;
use crate::blind_bundle_model::PreparedBlindBundles;
use crate::blind_bundle_transaction::BlindBundleCheckpoint;
use crate::blind_bundle_transaction::BlindBundleTree;

#[test]
fn blind_bundle_transaction_replay_publishes_exact_prepared_bytes_in_prefix_order() {
    let fixture = replay_fixture_with_material();
    let replay = run_frozen_replay_pair_from(fixture.path()).unwrap();
    let snapshot =
        crate::blind::read_context_snapshot(&replay.private_root.join("frozen-run-context.json"))
            .unwrap();
    let core = crate::blind_verify::verify_pair_evidence_core(&snapshot).unwrap();
    let pair = crate::blind_finalize::finalize_blind_pair(core).unwrap();
    let prepared = crate::blind_bundle::prepare_replay_blind_bundles(
        &pair,
        &["one", "two", "three"].map(str::to_string),
    )
    .unwrap();
    let mut observed = Vec::new();
    let mut clock = || Ok(DateTime::parse_from_rfc3339("2026-08-29T12:34:56.789Z")?.to_utc());
    let root = replay.private_root;
    let mut hook = |checkpoint| {
        match checkpoint {
            BlindBundleCheckpoint::AfterTreeRenameBeforeInventoryAppend(_)
            | BlindBundleCheckpoint::AfterReceiptCreateBeforeInventoryAppend => {
                assert!(crate::private_inventory::verify_private_inventory(&root).is_err());
            }
            BlindBundleCheckpoint::AfterReviewerPublish
            | BlindBundleCheckpoint::AfterMappingsPublish
            | BlindBundleCheckpoint::AfterReviewsPublish => {
                crate::private_inventory::verify_private_inventory(&root).unwrap();
            }
            BlindBundleCheckpoint::BeforeFirstPublish
            | BlindBundleCheckpoint::BeforeReceiptCreate => {}
            BlindBundleCheckpoint::AfterSeedsPublish => {
                panic!("Replay transaction emitted a seed checkpoint")
            }
        }
        observed.push(checkpoint);
        Ok(())
    };
    crate::blind_bundle_transaction::commit_blind_bundles_for_test(
        &pair, &prepared, &mut clock, &mut hook,
    )
    .unwrap();
    assert_published_matches_prepared(&prepared, "2026-08-29T12:34:56.789Z");

    assert_eq!(
        observed,
        vec![
            BlindBundleCheckpoint::BeforeFirstPublish,
            BlindBundleCheckpoint::AfterTreeRenameBeforeInventoryAppend(BlindBundleTree::Reviewer,),
            BlindBundleCheckpoint::AfterReviewerPublish,
            BlindBundleCheckpoint::AfterTreeRenameBeforeInventoryAppend(BlindBundleTree::Mappings,),
            BlindBundleCheckpoint::AfterMappingsPublish,
            BlindBundleCheckpoint::AfterTreeRenameBeforeInventoryAppend(BlindBundleTree::Reviews,),
            BlindBundleCheckpoint::AfterReviewsPublish,
            BlindBundleCheckpoint::BeforeReceiptCreate,
            BlindBundleCheckpoint::AfterReceiptCreateBeforeInventoryAppend,
        ]
    );
    assert!(!root.join("coordinator/blind-seeds").exists());
    assert!(fs::read_dir(root.join("reviews")).unwrap().next().is_none());
}

#[test]
fn blind_bundle_transaction_native_publishes_exact_private_seeds_in_order() {
    let native = run_native_mock_pair_with_marker(
        /*marker*/ None, /*max_total_tokens_per_run*/ 10,
    );
    native.result.as_ref().unwrap();
    let root = native.live_root.canonicalize().unwrap();
    let snapshot =
        crate::blind::read_context_snapshot(&root.join("frozen-run-context.json")).unwrap();
    let core = crate::blind_verify::verify_pair_evidence_core(&snapshot).unwrap();
    let pair = crate::blind_finalize::finalize_blind_pair(core).unwrap();
    let mut rng = FixedRng(seed_set());
    let prepared = crate::blind_bundle::prepare_native_blind_bundles(&pair, &mut rng).unwrap();

    let mut mutated = prepared.clone();
    mutated.reviewers[0].native_seed.as_mut().unwrap()[0] ^= 1;
    let mut clock = fixed_clock;
    let error = crate::blind_bundle_transaction::commit_blind_bundles_for_test(
        &pair,
        &mutated,
        &mut clock,
        &mut |_| Ok(()),
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "prepared Native seed differs from its mapping commitment"
    );
    assert!(!root.join("reviewer").exists());

    let mut observed = Vec::new();
    crate::blind_bundle_transaction::commit_blind_bundles_for_test(
        &pair,
        &prepared,
        &mut clock,
        &mut |checkpoint| {
            observed.push(checkpoint);
            Ok(())
        },
    )
    .unwrap();
    assert_published_matches_prepared(&prepared, "2026-08-29T12:34:56.789Z");
    assert_eq!(
        observed,
        vec![
            BlindBundleCheckpoint::BeforeFirstPublish,
            BlindBundleCheckpoint::AfterTreeRenameBeforeInventoryAppend(BlindBundleTree::Reviewer,),
            BlindBundleCheckpoint::AfterReviewerPublish,
            BlindBundleCheckpoint::AfterTreeRenameBeforeInventoryAppend(BlindBundleTree::Mappings,),
            BlindBundleCheckpoint::AfterMappingsPublish,
            BlindBundleCheckpoint::AfterTreeRenameBeforeInventoryAppend(BlindBundleTree::Seeds),
            BlindBundleCheckpoint::AfterSeedsPublish,
            BlindBundleCheckpoint::AfterTreeRenameBeforeInventoryAppend(BlindBundleTree::Reviews,),
            BlindBundleCheckpoint::AfterReviewsPublish,
            BlindBundleCheckpoint::BeforeReceiptCreate,
            BlindBundleCheckpoint::AfterReceiptCreateBeforeInventoryAppend,
        ]
    );
    for reviewer in &prepared.reviewers {
        let expected = reviewer.native_seed.unwrap();
        let path = root
            .join("coordinator/blind-seeds")
            .join(format!("{}.seed", reviewer.reviewer_id));
        assert_eq!(fs::read(&path).unwrap(), expected);
        let mapping: serde_json::Value = serde_json::from_slice(&reviewer.mapping_bytes).unwrap();
        let orientation = crate::blind_bundle::orientation(&expected);
        assert_eq!(mapping["a"], serde_json::to_value(orientation[0]).unwrap());
        assert_eq!(mapping["b"], serde_json::to_value(orientation[1]).unwrap());
        assert_eq!(
            mapping["seedCommitment"],
            crate::blind_bundle::seed_commitment(&expected)
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
    let seed_names = fs::read_dir(root.join("coordinator/blind-seeds"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        seed_names,
        prepared
            .reviewers
            .iter()
            .map(|reviewer| format!("{}.seed", reviewer.reviewer_id).into())
            .collect()
    );
    let receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("coordinator/blind-pack-receipt.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        receipt["pairReceiptSha256"],
        prepared.pair_receipt_sha256.as_deref().unwrap()
    );
    crate::private_inventory::verify_private_inventory(&root).unwrap();
}

#[test]
fn blind_bundle_transaction_rejects_visible_and_contract_prepared_mutation_before_output() {
    let fixture = replay_fixture_with_material();
    let replay = run_frozen_replay_pair_from(fixture.path()).unwrap();
    let snapshot =
        crate::blind::read_context_snapshot(&replay.private_root.join("frozen-run-context.json"))
            .unwrap();
    let core = crate::blind_verify::verify_pair_evidence_core(&snapshot).unwrap();
    let pair = crate::blind_finalize::finalize_blind_pair(core).unwrap();
    let prepared = crate::blind_bundle::prepare_replay_blind_bundles(
        &pair,
        &["one", "two", "three"].map(str::to_string),
    )
    .unwrap();
    let mut mutations = Vec::new();
    let mut case = prepared.clone();
    case.case_bytes.push(b' ');
    mutations.push(case);
    let mut material = prepared.clone();
    material.materials[0].bytes.push(b' ');
    mutations.push(material);
    let mut contract = prepared;
    contract.rubric_bytes = b"{}";
    mutations.push(contract);
    for mutated in mutations {
        let mut clock = fixed_clock;
        let error = crate::blind_bundle_transaction::commit_blind_bundles_for_test(
            &pair,
            &mutated,
            &mut clock,
            &mut |_| Ok(()),
        )
        .unwrap_err();
        assert!(error.to_string().starts_with("prepared blind"));
        assert_no_transaction_outputs(&replay.private_root);
    }
}

fn assert_published_matches_prepared(prepared: &PreparedBlindBundles, generated_at: &str) {
    let root = &prepared.private_root;
    for reviewer in &prepared.reviewers {
        let reviewer_root = root.join("reviewer").join(&reviewer.reviewer_id);
        for (relative, expected) in [
            ("A.json", reviewer.a_bytes.as_slice()),
            ("B.json", reviewer.b_bytes.as_slice()),
            ("case.json", prepared.case_bytes.as_slice()),
            (
                "materials-manifest.json",
                prepared.materials_manifest_bytes.as_slice(),
            ),
            (
                "review-bundle.json",
                reviewer.review_bundle_bytes.as_slice(),
            ),
            ("rubric.json", prepared.rubric_bytes),
            (
                "reviewer-submission.schema.json",
                prepared.reviewer_submission_schema_bytes,
            ),
        ] {
            assert_eq!(fs::read(reviewer_root.join(relative)).unwrap(), expected);
        }
        for material in &prepared.materials {
            assert_eq!(
                fs::read(
                    reviewer_root
                        .join("materials")
                        .join(&material.relative_path)
                )
                .unwrap(),
                material.bytes
            );
        }
        assert_eq!(
            fs::read(
                root.join("coordinator/mappings")
                    .join(format!("{}.json", reviewer.reviewer_id))
            )
            .unwrap(),
            reviewer.mapping_bytes
        );
        for bytes in [&reviewer.review_bundle_bytes, &reviewer.mapping_bytes] {
            assert!(!bytes.ends_with(b"\n"));
            assert_eq!(
                crate::jcs::canonicalize_value(&crate::jcs::parse_json(bytes).unwrap()).unwrap(),
                *bytes
            );
        }
    }

    let receipt_path = root.join("coordinator/blind-pack-receipt.json");
    let receipt_bytes = fs::read(&receipt_path).unwrap();
    let receipt: serde_json::Value = serde_json::from_slice(&receipt_bytes).unwrap();
    let inventory = fs::read(root.join("coordinator/private-inventory.jsonl")).unwrap();
    let body = &inventory[..inventory.len() - 1];
    let final_line_start = body.iter().rposition(|byte| *byte == b'\n').unwrap() + 1;
    let prefix = &inventory[..final_line_start];
    let expected = serde_json::json!({
        "schemaVersion": 1,
        "pairId": prepared.pair_id,
        "frozenRunContextSha256": prepared.frozen_run_context_sha256,
        "pairReceiptSha256": prepared.pair_receipt_sha256,
        "pairVerificationSha256": prepared.pair_verification_sha256,
        "rubricSha256": prepared.rubric_sha256,
        "decisionPolicySha256": prepared.decision_policy_sha256,
        "reviewerSubmissionSchemaSha256": prepared.reviewer_submission_schema_sha256,
        "reviewerMappings": prepared.reviewers.iter().map(|reviewer| serde_json::json!({
            "reviewerId": reviewer.reviewer_id,
            "reviewBundleSha256": reviewer.review_bundle_sha256,
            "mappingSha256": reviewer.mapping_sha256,
            "seedCommitment": reviewer.seed_commitment,
        })).collect::<Vec<_>>(),
        "inventoryRootSha256": sha256(prefix),
        "reviewsDropSha256": sha256(br#"{"entries":[]}"#),
        "generatedAt": generated_at,
    });
    assert_eq!(receipt, expected);
    assert!(!receipt_bytes.ends_with(b"\n"));
    assert_eq!(
        crate::jcs::canonicalize_value(&receipt).unwrap(),
        receipt_bytes
    );
    let final_record: serde_json::Value =
        serde_json::from_slice(&body[final_line_start..]).unwrap();
    assert_eq!(
        final_record["relativePath"],
        "coordinator/blind-pack-receipt.json"
    );
    assert_eq!(final_record["sha256"], sha256(&receipt_bytes));
    crate::private_inventory::verify_private_inventory(root).unwrap();
}

fn assert_no_transaction_outputs(root: &std::path::Path) {
    for relative in [
        "reviewer",
        "coordinator/mappings",
        "coordinator/blind-seeds",
        "reviews",
        "coordinator/blind-pack-receipt.json",
    ] {
        assert!(
            fs::symlink_metadata(root.join(relative)).is_err(),
            "found {relative}"
        );
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn fixed_clock() -> anyhow::Result<chrono::DateTime<chrono::Utc>> {
    Ok(DateTime::parse_from_rfc3339("2026-08-29T12:34:56.789Z")?.to_utc())
}

fn seed_set() -> [u8; 96] {
    let values = [
        "5b870385a70b814e565cbf1f117ee04c7216447c304d4ae4d5a280b4533a755c",
        "80d9cee288d6e1f1a72d6a57bd9d16f01d820cd665347baf7029fac64c49c56b",
        "b876a96c066ae8f767fce34fd556c4b4c9d5544a4399f2c41c0c6fab376ebb84",
    ];
    let mut raw = [0_u8; 96];
    for (slot, value) in values.into_iter().enumerate() {
        for index in 0..32 {
            raw[slot * 32 + index] =
                u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).unwrap();
        }
    }
    raw
}

struct FixedRng([u8; 96]);

impl TryRngCore for FixedRng {
    type Error = std::convert::Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        unreachable!("blind preparation requests one complete seed set")
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        unreachable!("blind preparation requests one complete seed set")
    }

    fn try_fill_bytes(&mut self, destination: &mut [u8]) -> Result<(), Self::Error> {
        assert_eq!(destination.len(), self.0.len());
        destination.copy_from_slice(&self.0);
        Ok(())
    }
}
