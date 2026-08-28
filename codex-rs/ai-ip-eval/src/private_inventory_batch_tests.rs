use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;

use pretty_assertions::assert_eq;
use sha2::Digest;

use crate::private_inventory::InventoryKind;
use crate::private_inventory::InventoryRecord;
use crate::private_inventory::batch::ExpectedInventoryEntry;
use crate::private_inventory::batch::append_private_inventory_batch;
use crate::private_inventory::batch::append_private_inventory_batch_from_root;
use crate::private_inventory::batch::append_private_inventory_from_root;

const PAIR_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const FROZEN_SHA: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const CREATED_AT: &str = "2026-08-28T12:00:00Z";
const INVENTORY: &str = "coordinator/private-inventory.jsonl";

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
