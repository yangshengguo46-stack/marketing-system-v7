use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;

use pretty_assertions::assert_eq;
use sha2::Digest;

use crate::private_inventory::InventoryKind;
use crate::private_inventory::InventoryRecord;
use crate::private_inventory::batch::ExpectedInventoryEntry;
use crate::private_inventory::batch::RetainedPendingReview;
use crate::private_inventory::batch::RetainedScoreDecisionStaging;
use crate::private_inventory::batch::append_private_inventory_batch;
use crate::private_inventory::batch::append_private_inventory_batch_from_root;
use crate::private_inventory::batch::append_private_inventory_from_root;
use crate::private_inventory::batch::reverify_score_inventory_with_staging;
use crate::private_inventory::batch::verify_pending_cost_binding_inventory;
use crate::private_inventory::batch::verify_pending_supplier_inventory;
use crate::private_inventory::batch::verify_private_inventory_continuation;

const PAIR_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const FROZEN_SHA: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const CREATED_AT: &str = "2026-08-28T12:00:00Z";
const INVENTORY: &str = "coordinator/private-inventory.jsonl";
const BLIND_RECEIPT: &str = "coordinator/blind-pack-receipt.json";

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}

fn owner_root() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
    #[cfg(unix)]
    let root = temp.path().canonicalize().unwrap();
    #[cfg(windows)]
    let root = {
        let root = temp.path().join("private");
        crate::secure_fs::create_owner_only_dir_new(&root).unwrap();
        root.canonicalize().unwrap()
    };
    (temp, root)
}

fn sealed_root() -> (tempfile::TempDir, PathBuf) {
    let (temp, root) = owner_root();
    crate::secure_fs::write_owner_only_new(&root.join("proof.bin"), b"proof").unwrap();
    crate::private_inventory::bootstrap_private_inventory(&root, PAIR_ID, FROZEN_SHA, CREATED_AT)
        .unwrap();
    (temp, root)
}

fn pending_supplier_root() -> (tempfile::TempDir, PathBuf, ExpectedInventoryEntry) {
    let (temp, root) = owner_root();
    crate::secure_fs::create_owner_only_dir_new(&root.join("inputs")).unwrap();
    crate::secure_fs::create_owner_only_dir_new(&root.join("inputs/supplier-statements")).unwrap();
    crate::private_inventory::bootstrap_private_inventory(&root, PAIR_ID, FROZEN_SHA, CREATED_AT)
        .unwrap();
    let relative = "inputs/supplier-statements/candidate.json";
    crate::secure_fs::write_owner_only_new(&root.join(relative), b"supplier").unwrap();
    (temp, root, file(relative, b"supplier"))
}

fn pending_binding_root(
    relative: &str,
) -> (tempfile::TempDir, PathBuf, ExpectedInventoryEntry) {
    let (temp, root) = owner_root();
    crate::secure_fs::create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    crate::secure_fs::create_owner_only_dir_new(&root.join("coordinator/cost")).unwrap();
    crate::private_inventory::bootstrap_private_inventory(&root, PAIR_ID, FROZEN_SHA, CREATED_AT)
        .unwrap();
    crate::secure_fs::write_owner_only_new(&root.join(relative), b"binding").unwrap();
    (temp, root, file(relative, b"binding"))
}

fn pending_binding_error(
    setup: impl FnOnce(&Path, &mut ExpectedInventoryEntry),
    expected_message: &str,
) {
    let (_temp, root, mut expected) =
        pending_binding_root("coordinator/cost/candidate-binding.json");
    setup(&root, &mut expected);
    let error = verify_pending_cost_binding_inventory(&root, &expected)
        .err()
        .expect("pending binding inventory must be rejected");
    assert!(error.to_string().contains(expected_message), "{error:#}");
}

fn pending_supplier_error(
    setup: impl FnOnce(&Path, &mut ExpectedInventoryEntry),
    expected_message: &str,
) {
    let (_temp, root, mut expected) = pending_supplier_root();
    setup(&root, &mut expected);
    let error = verify_pending_supplier_inventory(&root, &expected)
        .err()
        .expect("pending supplier inventory must be rejected");
    assert!(error.to_string().contains(expected_message), "{error:#}");
}

fn inventory_bytes(root: &Path) -> Vec<u8> {
    fs::read(root.join(INVENTORY)).unwrap()
}

fn directory(path: &str) -> ExpectedInventoryEntry {
    ExpectedInventoryEntry {
        relative_path: path.to_string(),
        kind: InventoryKind::Directory,
        sha256: None,
    }
}

fn file(path: &str, bytes: &[u8]) -> ExpectedInventoryEntry {
    ExpectedInventoryEntry {
        relative_path: path.to_string(),
        kind: InventoryKind::File,
        sha256: Some(digest(bytes)),
    }
}

fn inventory(root: &Path) -> (Vec<Vec<u8>>, Vec<InventoryRecord>) {
    let bytes = inventory_bytes(root);
    let lines = bytes[..bytes.len() - 1]
        .split(|byte| *byte == b'\n')
        .map(<[u8]>::to_vec)
        .collect::<Vec<_>>();
    let records = lines
        .iter()
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    (lines, records)
}

fn write_review_leaf(path: &Path, bytes: &[u8]) {
    #[cfg(unix)]
    fs::write(path, bytes).unwrap();
    #[cfg(windows)]
    crate::secure_fs::write_owner_only_new(path, bytes).unwrap();
}

fn append_recorded_file_for_test(root: &Path, relative_path: &str, bytes: &[u8]) {
    crate::secure_fs::write_owner_only_new(&root.join(relative_path), bytes).unwrap();
    let (lines, records) = inventory(root);
    let record = InventoryRecord {
        schema_version: 1,
        sequence: u64::try_from(records.len() + 1).unwrap(),
        relative_path: relative_path.to_string(),
        kind: InventoryKind::File,
        sha256: Some(digest(bytes)),
        previous_record_sha256: lines.last().map(|line| digest(line)),
    };
    let mut encoded =
        crate::jcs::canonicalize_value(&serde_json::to_value(record).unwrap()).unwrap();
    encoded.push(b'\n');
    let mut inventory_file = fs::OpenOptions::new()
        .append(true)
        .open(root.join(INVENTORY))
        .unwrap();
    inventory_file.write_all(&encoded).unwrap();
    inventory_file.sync_all().unwrap();
}

fn reject_without_append(setup: impl FnOnce(&Path), expected: Vec<ExpectedInventoryEntry>) {
    let (_temp, root) = sealed_root();
    let prefix = inventory_bytes(&root);
    setup(&root);
    assert!(append_private_inventory_batch(&root, &expected).is_err());
    assert_eq!(inventory_bytes(&root), prefix);
}

#[test]
fn private_inventory_batch_appends_exact_tree_order_as_one_chain_extension() {
    let (_temp, root) = sealed_root();
    let prefix = inventory_bytes(&root);
    for relative in [
        ".batch-staging",
        ".batch-staging/r1",
        ".batch-staging/r1/materials",
    ] {
        crate::secure_fs::create_owner_only_dir_new(&root.join(relative)).unwrap();
    }
    crate::secure_fs::write_owner_only_new(&root.join(".batch-staging/r1/A.json"), b"A").unwrap();
    crate::secure_fs::write_owner_only_new(
        &root.join(".batch-staging/r1/materials/source.txt"),
        b"source",
    )
    .unwrap();
    crate::secure_fs_publish::publish_private_tree_no_replace(
        &root,
        Path::new(".batch-staging"),
        Path::new("reviewer"),
    )
    .unwrap();
    let expected = vec![
        directory("reviewer"),
        directory("reviewer/r1"),
        file("reviewer/r1/A.json", b"A"),
        directory("reviewer/r1/materials"),
        file("reviewer/r1/materials/source.txt", b"source"),
    ];

    let root_sha = append_private_inventory_batch(&root, &expected).unwrap();
    let complete = inventory_bytes(&root);
    assert!(complete.starts_with(&prefix));
    assert_eq!(root_sha, digest(&complete));
    assert_eq!(
        crate::private_inventory::verify_private_inventory(&root).unwrap(),
        root_sha
    );
    let (lines, records) = inventory(&root);
    let appended = &records[records.len() - expected.len()..];
    assert_eq!(
        appended
            .iter()
            .map(|record| (
                record.relative_path.as_str(),
                record.kind,
                record.sha256.as_deref(),
            ))
            .collect::<Vec<_>>(),
        expected
            .iter()
            .map(|entry| (
                entry.relative_path.as_str(),
                entry.kind,
                entry.sha256.as_deref(),
            ))
            .collect::<Vec<_>>()
    );
    for (index, (line, record)) in lines.iter().zip(&records).enumerate() {
        assert_eq!(
            *line,
            crate::jcs::canonicalize_value(&crate::jcs::parse_json(line).unwrap()).unwrap()
        );
        assert_eq!(record.sequence, u64::try_from(index + 1).unwrap());
        assert_eq!(
            record.previous_record_sha256,
            index.checked_sub(1).map(|prior| digest(&lines[prior]))
        );
    }

    crate::secure_fs::write_owner_only_new(&root.join("later.bin"), b"later").unwrap();
    crate::private_inventory::append_private_inventory(&root, Path::new("later.bin")).unwrap();
    crate::private_inventory::verify_private_inventory(&root).unwrap();
}

#[test]
fn private_inventory_checked_appends_require_the_retained_old_root() {
    let (_temp, root) = sealed_root();
    let old_root = crate::private_inventory::verify_private_inventory(&root).unwrap();
    crate::secure_fs::create_owner_only_dir_new(&root.join("batch")).unwrap();
    let before = inventory_bytes(&root);
    let wrong_root = "f".repeat(64);
    let error = append_private_inventory_batch_from_root(
        &root,
        &wrong_root,
        &[directory("batch")],
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "private inventory cursor changed before batch append");
    assert_eq!(inventory_bytes(&root), before);

    let next_root =
        append_private_inventory_batch_from_root(&root, &old_root, &[directory("batch")])
            .unwrap();
    crate::secure_fs::write_owner_only_new(&root.join("receipt.json"), b"receipt").unwrap();
    let before_receipt = inventory_bytes(&root);
    let error = append_private_inventory_from_root(
        &root,
        &old_root,
        Path::new("receipt.json"),
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "private inventory cursor changed before append");
    assert_eq!(inventory_bytes(&root), before_receipt);
    append_private_inventory_from_root(&root, &next_root, Path::new("receipt.json")).unwrap();
}

fn pending_review_inventory(
    add_record_after_receipt: bool,
) -> (
    tempfile::TempDir,
    PathBuf,
    String,
    Vec<u8>,
    [RetainedPendingReview; 3],
) {
    let (temp, root) = sealed_root();
    crate::secure_fs::create_owner_only_dir_new(&root.join("reviews")).unwrap();
    let receipt_prefix =
        crate::private_inventory::append_private_inventory(&root, Path::new("reviews")).unwrap();
    let receipt_bytes = br#"{"schemaVersion":1}"#.to_vec();
    crate::secure_fs::write_owner_only_new(
        &root.join("coordinator/blind-pack-receipt.json"),
        &receipt_bytes,
    )
    .unwrap();
    let receipt_root =
        append_private_inventory_from_root(&root, &receipt_prefix, Path::new(BLIND_RECEIPT))
            .unwrap();
    if add_record_after_receipt {
        crate::secure_fs::write_owner_only_new(&root.join("later.bin"), b"later").unwrap();
        append_private_inventory_from_root(&root, &receipt_root, Path::new("later.bin")).unwrap();
    }
    let reviews = [
        ("reviewer-1", b"one".as_slice()),
        ("reviewer-2", b"two".as_slice()),
        ("reviewer-3", b"three".as_slice()),
    ];
    for (reviewer, bytes) in reviews {
        let path = root.join(format!("reviews/{reviewer}.json"));
        write_review_leaf(&path, bytes);
    }
    let pending = ["reviewer-1", "reviewer-2", "reviewer-3"]
        .map(|reviewer| RetainedPendingReview::retain(&root, reviewer).unwrap());
    (temp, root, receipt_prefix, receipt_bytes, pending)
}

#[test]
fn private_inventory_continuation_allows_only_exact_pending_reviews_at_receipt_tail() {
    let (_temp, root, receipt_prefix, receipt_bytes, pending) =
        pending_review_inventory(/* add_record_after_receipt */ false);
    assert!(crate::private_inventory::verify_private_inventory(&root).is_err());
    let [reviewer_1, reviewer_2, reviewer_3] = pending;

    let continuation = verify_private_inventory_continuation(
        &root,
        [reviewer_3, reviewer_1, reviewer_2],
        &digest(&receipt_bytes),
        &receipt_prefix,
    )
    .unwrap();

    continuation
        .verify_binding(PAIR_ID, FROZEN_SHA, &root)
        .unwrap();
    continuation.reverify_unchanged().unwrap();
    assert_eq!(
        continuation.inventory_root_sha256(),
        digest(&inventory_bytes(&root))
    );
}

#[test]
fn pending_supplier_inventory_accepts_one_exact_unrecorded_leaf() {
    let (_temp, root, expected) = pending_supplier_root();
    let old_root = digest(&inventory_bytes(&root));
    assert!(crate::private_inventory::verify_private_inventory_state(&root).is_err());

    let verified = verify_pending_supplier_inventory(&root, &expected).unwrap();

    assert_eq!(verified.inventory_root_sha256(), old_root);
    verified.verify_binding(PAIR_ID, FROZEN_SHA, &root).unwrap();
    verified.reverify_unchanged().unwrap();
}

#[test]
fn pending_supplier_inventory_rejects_recorded_or_arbitrary_leaf() {
    pending_supplier_error(
        |root, expected| {
            let old_root = digest(&inventory_bytes(root));
            append_private_inventory_batch_from_root(root, &old_root, std::slice::from_ref(expected))
                .unwrap();
        },
        "already recorded",
    );
    pending_supplier_error(
        |root, expected| {
            crate::secure_fs::write_owner_only_new(&root.join("arbitrary.json"), b"supplier")
                .unwrap();
            expected.relative_path = "arbitrary.json".to_string();
        },
        "fixed supplier",
    );
}

#[test]
fn pending_supplier_inventory_rejects_wrong_kind_missing_or_wrong_sha() {
    pending_supplier_error(
        |_, expected| {
            expected.kind = InventoryKind::Directory;
            expected.sha256 = None;
        },
        "regular file",
    );
    pending_supplier_error(
        |_, expected| expected.sha256 = None,
        "lowercase SHA-256",
    );
    pending_supplier_error(
        |_, expected| expected.sha256 = Some("f".repeat(64)),
        "SHA-256",
    );
}

#[test]
fn pending_supplier_inventory_rejects_extra_unrecorded_entry() {
    pending_supplier_error(
        |root, _| {
            crate::secure_fs::write_owner_only_new(&root.join("unexpected.bin"), b"unexpected")
                .unwrap();
        },
        "unexpected path",
    );
}

#[test]
fn pending_supplier_inventory_reverify_rejects_replacement() {
    let (_temp, root, expected) = pending_supplier_root();
    let verified = verify_pending_supplier_inventory(&root, &expected).unwrap();
    fs::write(root.join(&expected.relative_path), b"replaced").unwrap();

    assert!(verified.reverify_unchanged().is_err());
}

#[test]
fn pending_cost_binding_inventory_accepts_only_each_fixed_unrecorded_binding_leaf() {
    for relative in [
        "coordinator/cost/generic-binding.json",
        "coordinator/cost/candidate-binding.json",
    ] {
        let (_temp, root, expected) = pending_binding_root(relative);
        let cursor = inventory_bytes(&root);
        let verified = verify_pending_cost_binding_inventory(&root, &expected).unwrap();
        assert_eq!(verified.inventory_root_sha256(), digest(&cursor));
        assert_eq!(inventory_bytes(&root), cursor);
        verified.verify_binding(PAIR_ID, FROZEN_SHA, &root).unwrap();
        verified.reverify_unchanged().unwrap();
        assert_eq!(inventory_bytes(&root), cursor);
    }
}

#[test]
fn pending_cost_binding_inventory_rejects_recorded_arbitrary_or_extra_leaf() {
    pending_binding_error(
        |root, expected| {
            let cursor = digest(&inventory_bytes(root));
            append_private_inventory_batch_from_root(root, &cursor, std::slice::from_ref(expected))
                .unwrap();
        },
        "already recorded",
    );
    pending_binding_error(
        |root, expected| {
            crate::secure_fs::write_owner_only_new(&root.join("arbitrary.json"), b"binding")
                .unwrap();
            expected.relative_path = "arbitrary.json".to_string();
        },
        "fixed cost binding",
    );
    pending_binding_error(
        |root, _| {
            crate::secure_fs::write_owner_only_new(&root.join("unexpected.bin"), b"extra")
                .unwrap();
        },
        "unexpected path",
    );
}

#[test]
fn pending_cost_binding_inventory_rejects_invalid_or_replaced_expected_leaf() {
    pending_binding_error(
        |_, expected| {
            expected.kind = InventoryKind::Directory;
            expected.sha256 = None;
        },
        "regular file",
    );
    pending_binding_error(
        |_, expected| expected.sha256 = Some("F".repeat(64)),
        "lowercase SHA-256",
    );
    pending_binding_error(
        |_, expected| expected.sha256 = Some("f".repeat(64)),
        "SHA-256",
    );

    let (_temp, root, expected) = pending_binding_root("coordinator/cost/candidate-binding.json");
    let verified = verify_pending_cost_binding_inventory(&root, &expected).unwrap();
    fs::write(root.join(&expected.relative_path), b"replaced").unwrap();
    let error = verified.reverify_unchanged().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("inventory append target is absent, recorded, or mismatched"),
        "{error:#}"
    );
}

#[cfg(unix)]
#[test]
fn pending_cost_binding_inventory_rejects_unsafe_leaf_permissions() {
    let (_temp, root, expected) = pending_binding_root("coordinator/cost/candidate-binding.json");
    fs::set_permissions(
        root.join(&expected.relative_path),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let error = verify_pending_cost_binding_inventory(&root, &expected)
        .err()
        .expect("unsafe binding leaf must be rejected");
    assert!(error.to_string().contains("unsafe type, links, or permissions"), "{error:#}");
}

#[test]
fn private_inventory_score_staging_projection_allows_only_one_exact_decision_leaf() {
    let (_temp, root, receipt_prefix, receipt_bytes, pending) =
        pending_review_inventory(/* add_record_after_receipt */ false);
    let continuation = verify_private_inventory_continuation(
        &root,
        pending,
        &digest(&receipt_bytes),
        &receipt_prefix,
    )
    .unwrap();
    let (inventory, reviews) = continuation.into_parts();
    let decision = br#"{"decision":"INVALID_PROOF"}"#;
    crate::secure_fs::write_owner_only_new(
        &root.join("coordinator/.decision.private.json.staging"),
        decision,
    )
    .unwrap();
    let staging = RetainedScoreDecisionStaging::retain(&root, decision).unwrap();

    assert!(inventory.reverify_unchanged().is_err());
    reverify_score_inventory_with_staging(&inventory, &reviews, &staging).unwrap();

    crate::secure_fs::write_owner_only_new(&root.join("unexpected.bin"), b"unexpected").unwrap();
    assert!(reverify_score_inventory_with_staging(&inventory, &reviews, &staging).is_err());
}

#[test]
fn retained_score_staging_publishes_without_releasing_its_authority() {
    let (_temp, root) = owner_root();
    crate::secure_fs::create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    let decision = br#"{"decision":"INVALID_PROOF"}"#;
    crate::secure_fs::write_owner_only_new(
        &root.join("coordinator/.decision.private.json.staging"),
        decision,
    )
    .unwrap();
    let staging = RetainedScoreDecisionStaging::retain(&root, decision).unwrap();

    staging
        .publish_no_replace(Path::new("coordinator/decision.private.json"))
        .unwrap();

    assert_eq!(
        fs::read(root.join("coordinator/decision.private.json")).unwrap(),
        decision
    );
    assert!(fs::symlink_metadata(root.join("coordinator/.decision.private.json.staging")).is_err());
}

#[test]
fn private_inventory_continuation_rejects_set_tail_and_cursor_drift() {
    let (_temp, root, receipt_prefix, _receipt_bytes, pending) =
        pending_review_inventory(/* add_record_after_receipt */ false);
    assert!(
        verify_private_inventory_continuation(&root, pending, &digest(b"wrong"), &receipt_prefix,)
            .is_err()
    );

    let (_temp, root, _receipt_prefix, receipt_bytes, pending) =
        pending_review_inventory(/* add_record_after_receipt */ false);
    assert!(
        verify_private_inventory_continuation(
            &root,
            pending,
            &digest(&receipt_bytes),
            &"f".repeat(64),
        )
        .is_err()
    );

    let (_temp, root, receipt_prefix, receipt_bytes, pending) =
        pending_review_inventory(/* add_record_after_receipt */ false);
    write_review_leaf(
        &root.join("reviews/reviewer-4.json"),
        &vec![b'x'; 2 * 1024 * 1024],
    );
    assert_eq!(
        verify_private_inventory_continuation(
            &root,
            pending,
            &digest(&receipt_bytes),
            &receipt_prefix,
        )
        .err()
        .expect("unexpected review path must fail before content read")
        .to_string(),
        "trusted inventory continuation contains an unexpected path"
    );

    let (_temp, root, receipt_prefix, receipt_bytes, pending) =
        pending_review_inventory(/* add_record_after_receipt */ false);
    let continuation = verify_private_inventory_continuation(
        &root,
        pending,
        &digest(&receipt_bytes),
        &receipt_prefix,
    )
    .unwrap();
    #[cfg(unix)]
    {
        fs::write(root.join("reviews/reviewer-1.json"), b"changed").unwrap();
        assert!(continuation.reverify_unchanged().is_err());
    }
    #[cfg(windows)]
    {
        assert!(fs::write(root.join("reviews/reviewer-1.json"), b"changed").is_err());
        continuation.reverify_unchanged().unwrap();
    }

    let (_temp, root, receipt_prefix, receipt_bytes, pending) =
        pending_review_inventory(/* add_record_after_receipt */ true);
    assert!(
        verify_private_inventory_continuation(
            &root,
            pending,
            &digest(&receipt_bytes),
            &receipt_prefix,
        )
        .is_err()
    );
}

#[test]
fn private_inventory_continuation_retains_its_inventory_cursor_independently() {
    let (_temp, root, receipt_prefix, receipt_bytes, pending) =
        pending_review_inventory(/* add_record_after_receipt */ false);
    let continuation = verify_private_inventory_continuation(
        &root,
        pending,
        &digest(&receipt_bytes),
        &receipt_prefix,
    )
    .unwrap();
    append_recorded_file_for_test(&root, "later.bin", b"later");
    for review in continuation.reviews() {
        review.reverify_unchanged().unwrap();
    }

    assert_eq!(
        continuation.reverify_unchanged().unwrap_err().to_string(),
        "private inventory changed after initial verification"
    );
}

#[test]
fn private_inventory_continuation_rejects_pending_reviews_retained_from_another_root() {
    let (_temp_a, _root_a, _prefix_a, _receipt_a, pending_a) =
        pending_review_inventory(/* add_record_after_receipt */ false);
    let (_temp_b, root_b, prefix_b, receipt_b, _pending_b) =
        pending_review_inventory(/* add_record_after_receipt */ false);

    assert_eq!(
        verify_private_inventory_continuation(&root_b, pending_a, &digest(&receipt_b), &prefix_b,)
            .err()
            .expect("cross-root continuation must fail")
            .to_string(),
        "pending review belongs to a different private root"
    );
}

#[test]
fn private_inventory_batch_rejects_nonexact_caller_shapes_without_append() {
    let (_temp, root) = sealed_root();
    let prefix = inventory_bytes(&root);
    let invalid = [
        vec![],
        vec![directory("z"), directory("a")],
        vec![directory("duplicate"), directory("duplicate")],
        vec![directory("")],
        vec![directory("/absolute")],
        vec![directory(".")],
        vec![directory("../escape")],
        vec![file(INVENTORY, b"reserved")],
        vec![ExpectedInventoryEntry {
            relative_path: "file.bin".to_string(),
            kind: InventoryKind::File,
            sha256: None,
        }],
        vec![ExpectedInventoryEntry {
            relative_path: "directory".to_string(),
            kind: InventoryKind::Directory,
            sha256: Some(digest(b"forbidden")),
        }],
        vec![ExpectedInventoryEntry {
            relative_path: "uppercase.bin".to_string(),
            kind: InventoryKind::File,
            sha256: Some("A".repeat(64)),
        }],
        vec![ExpectedInventoryEntry {
            relative_path: "short.bin".to_string(),
            kind: InventoryKind::File,
            sha256: Some("a".repeat(63)),
        }],
        vec![ExpectedInventoryEntry {
            relative_path: "nonhex.bin".to_string(),
            kind: InventoryKind::File,
            sha256: Some("g".repeat(64)),
        }],
    ];

    for expected in invalid {
        assert!(append_private_inventory_batch(&root, &expected).is_err());
        assert_eq!(inventory_bytes(&root), prefix);
    }
}

#[test]
fn private_inventory_batch_rejects_unexpected_missing_mismatched_and_recorded_entries() {
    reject_without_append(
        |root| {
            crate::secure_fs::create_owner_only_dir_new(&root.join("batch")).unwrap();
            crate::secure_fs::write_owner_only_new(&root.join("batch/a.bin"), b"a").unwrap();
            crate::secure_fs::write_owner_only_new(&root.join("batch/extra.bin"), b"extra")
                .unwrap();
        },
        vec![directory("batch"), file("batch/a.bin", b"a")],
    );
    reject_without_append(
        |root| crate::secure_fs::create_owner_only_dir_new(&root.join("batch")).unwrap(),
        vec![directory("batch"), file("batch/missing.bin", b"missing")],
    );
    reject_without_append(
        |root| {
            crate::secure_fs::create_owner_only_dir_new(&root.join("batch")).unwrap();
            crate::secure_fs::write_owner_only_new(&root.join("batch/a.bin"), b"actual").unwrap();
        },
        vec![directory("batch"), file("batch/a.bin", b"wrong")],
    );
    reject_without_append(|_| {}, vec![file("proof.bin", b"proof")]);
    reject_without_append(
        |root| crate::secure_fs::create_owner_only_dir_new(&root.join("batch")).unwrap(),
        vec![file("batch", b"not-a-directory")],
    );
}

#[cfg(unix)]
#[test]
fn private_inventory_batch_rejects_links_without_append() {
    reject_without_append(
        |root| {
            crate::secure_fs::create_owner_only_dir_new(&root.join("batch")).unwrap();
            std::os::unix::fs::symlink("missing", root.join("batch/link")).unwrap();
        },
        vec![directory("batch"), file("batch/link", b"missing")],
    );
}

#[cfg(unix)]
#[test]
fn private_inventory_batch_preserves_legacy_single_append_path_rules() {
    let (_temp, root) = sealed_root();
    for relative in ["legacy:name.bin", "legacy\\name.bin"] {
        crate::secure_fs::write_owner_only_new(&root.join(relative), b"legacy").unwrap();
        crate::private_inventory::append_private_inventory(&root, Path::new(relative)).unwrap();
    }

    crate::private_inventory::verify_private_inventory(&root).unwrap();
}
