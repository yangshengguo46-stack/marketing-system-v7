use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use chrono::DateTime;
use pretty_assertions::assert_eq;

use super::*;
use crate::blind_bundle_model::PreparedBlindBundles;
use crate::blind_bundle_transaction::BlindBundleCheckpoint;
use crate::blind_bundle_transaction::BlindBundleTree;
use crate::blind_finalize::VerifiedBlindPair;

#[test]
fn blind_bundle_transaction_rejects_replaced_receipt_without_inventory_append() {
    let (_fixture, replay, pair, prepared) = replay_transaction();
    let root = replay.private_root;
    let inventory_path = root.join("coordinator/private-inventory.jsonl");
    let receipt_path = root.join("coordinator/blind-pack-receipt.json");
    let mut inventory_prefix = Vec::new();
    let mut original_receipt = Vec::new();
    let error = crate::blind_bundle_transaction::commit_blind_bundles_for_test(
        &pair,
        &prepared,
        &mut fixed_clock,
        &mut |checkpoint| {
            if checkpoint == BlindBundleCheckpoint::AfterReceiptCreateBeforeInventoryAppend {
                inventory_prefix = fs::read(&inventory_path)?;
                original_receipt = fs::read(&receipt_path)?;
                let replacement = root.join("coordinator/.replacement-receipt");
                let displaced = root.join("coordinator/.displaced-receipt");
                crate::secure_fs::write_owner_only_new(&replacement, b"{}")?;
                fs::rename(&receipt_path, &displaced)?;
                fs::rename(&replacement, &receipt_path)?;
                fs::remove_file(displaced)?;
                crate::secure_fs::fsync_directory(&root.join("coordinator"))?;
            }
            Ok(())
        },
    )
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "inventory append target is absent, recorded, or mismatched"
    );
    assert_eq!(fs::read(&inventory_path).unwrap(), inventory_prefix);
    assert_eq!(fs::read(&receipt_path).unwrap(), b"{}");
    assert_ne!(fs::read(&receipt_path).unwrap(), original_receipt);
    assert!(crate::private_inventory::verify_private_inventory(&root).is_err());
    assert_transaction_paths(
        &root,
        &[
            "reviewer",
            "coordinator/mappings",
            "reviews",
            "coordinator/blind-pack-receipt.json",
        ],
    );
    assert_failed_rerun_is_read_only(&root);
}

#[test]
fn blind_bundle_transaction_rejects_all_fixed_staging_paths_before_mutation() {
    let (_fixture, replay, pair, prepared) = replay_transaction();
    let root = replay.private_root;
    for relative in [
        ".blind-pack-staging-reviewer",
        "coordinator/.blind-pack-staging-mappings",
        "coordinator/.blind-pack-staging-seeds",
        ".blind-pack-staging-reviews",
    ] {
        let path = root.join(relative);
        crate::secure_fs::create_owner_only_dir_new(&path).unwrap();
        let before = tree_snapshot(&root);
        let error = crate::blind_bundle_transaction::commit_blind_bundles_for_test(
            &pair,
            &prepared,
            &mut fixed_clock,
            &mut |_| Ok(()),
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("blind-pack transaction path already exists: {relative}")
        );
        assert_eq!(tree_snapshot(&root), before);
        fs::remove_dir(path).unwrap();
        crate::private_inventory::verify_private_inventory(&root).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn blind_bundle_transaction_rejects_same_bytes_new_private_root_inode() {
    use std::os::unix::fs::MetadataExt;

    let (_fixture, replay, pair, prepared) = replay_transaction();
    let root = replay.private_root;
    let parent = root.parent().unwrap();
    let replacement = parent.join("replacement-private-root");
    let displaced = parent.join("displaced-private-root");
    let mut expected = None;
    let mut original_inode = None;
    let error = crate::blind_bundle_transaction::commit_blind_bundles_for_test(
        &pair,
        &prepared,
        &mut fixed_clock,
        &mut |checkpoint| {
            if checkpoint == BlindBundleCheckpoint::AfterReviewerPublish {
                crate::private_inventory::verify_private_inventory(&root)?;
                copy_private_tree(&root, &replacement)?;
                expected = Some(tree_snapshot(&root));
                original_inode = Some(fs::metadata(&root)?.ino());
                fs::rename(&root, &displaced)?;
                fs::rename(&replacement, &root)?;
            }
            Ok(())
        },
    )
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "private root identity changed during blind bundle transaction"
    );
    assert_eq!(tree_snapshot(&root), expected.unwrap());
    assert_ne!(fs::metadata(&root).unwrap().ino(), original_inode.unwrap());
    crate::private_inventory::verify_private_inventory(&root).unwrap();
    assert_transaction_paths(&root, &["reviewer"]);
    assert_failed_rerun_is_read_only(&root);
}

#[test]
fn blind_bundle_native_failure_after_seeds_rename_is_unrecorded() {
    assert_native_seed_checkpoint_failure(
        BlindBundleCheckpoint::AfterTreeRenameBeforeInventoryAppend(BlindBundleTree::Seeds),
        false,
    );
}

#[test]
fn blind_bundle_native_failure_after_seeds_publish_is_recorded() {
    assert_native_seed_checkpoint_failure(BlindBundleCheckpoint::AfterSeedsPublish, true);
}

macro_rules! replay_fault_case {
    ($name:ident, $checkpoint:expr, $inventory_valid:expr, [$($present:literal),*], [$($absent:literal),*]) => {
        #[test]
        fn $name() {
            assert_replay_checkpoint_failure(
                $checkpoint,
                $inventory_valid,
                &[$($present),*],
                &[$($absent),*],
            );
        }
    };
}

replay_fault_case!(
    blind_bundle_failure_before_first_publish_leaves_only_staging,
    BlindBundleCheckpoint::BeforeFirstPublish,
    false,
    [".blind-pack-staging-reviewer"],
    [
        "reviewer",
        "coordinator/mappings",
        "reviews",
        "coordinator/blind-pack-receipt.json"
    ]
);
replay_fault_case!(
    blind_bundle_failure_after_reviewer_rename_is_unrecorded,
    BlindBundleCheckpoint::AfterTreeRenameBeforeInventoryAppend(BlindBundleTree::Reviewer),
    false,
    ["reviewer"],
    [
        ".blind-pack-staging-reviewer",
        "coordinator/mappings",
        "reviews",
        "coordinator/blind-pack-receipt.json"
    ]
);
replay_fault_case!(
    blind_bundle_failure_after_reviewer_publish_is_recorded,
    BlindBundleCheckpoint::AfterReviewerPublish,
    true,
    ["reviewer"],
    [
        "coordinator/.blind-pack-staging-mappings",
        "coordinator/mappings",
        "reviews",
        "coordinator/blind-pack-receipt.json"
    ]
);
replay_fault_case!(
    blind_bundle_failure_after_mappings_rename_is_unrecorded,
    BlindBundleCheckpoint::AfterTreeRenameBeforeInventoryAppend(BlindBundleTree::Mappings),
    false,
    ["reviewer", "coordinator/mappings"],
    [
        "coordinator/.blind-pack-staging-mappings",
        "reviews",
        "coordinator/blind-pack-receipt.json"
    ]
);
replay_fault_case!(
    blind_bundle_failure_after_mappings_publish_is_recorded,
    BlindBundleCheckpoint::AfterMappingsPublish,
    true,
    ["reviewer", "coordinator/mappings"],
    [
        ".blind-pack-staging-reviews",
        "reviews",
        "coordinator/blind-pack-receipt.json"
    ]
);
replay_fault_case!(
    blind_bundle_failure_after_reviews_rename_is_unrecorded,
    BlindBundleCheckpoint::AfterTreeRenameBeforeInventoryAppend(BlindBundleTree::Reviews),
    false,
    ["reviewer", "coordinator/mappings", "reviews"],
    [
        ".blind-pack-staging-reviews",
        "coordinator/blind-pack-receipt.json"
    ]
);
replay_fault_case!(
    blind_bundle_failure_after_reviews_publish_is_recorded,
    BlindBundleCheckpoint::AfterReviewsPublish,
    true,
    ["reviewer", "coordinator/mappings", "reviews"],
    ["coordinator/blind-pack-receipt.json"]
);
replay_fault_case!(
    blind_bundle_failure_before_receipt_is_recorded_without_success,
    BlindBundleCheckpoint::BeforeReceiptCreate,
    true,
    ["reviewer", "coordinator/mappings", "reviews"],
    ["coordinator/blind-pack-receipt.json"]
);

#[test]
fn blind_bundle_clock_failure_keeps_complete_recorded_trees_without_receipt() {
    let (_fixture, replay, pair, prepared) = replay_transaction();
    let root = replay.private_root;
    let error = crate::blind_bundle_transaction::commit_blind_bundles_for_test(
        &pair,
        &prepared,
        &mut || anyhow::bail!("injected clock failure"),
        &mut |_| Ok(()),
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "injected clock failure");
    crate::private_inventory::verify_private_inventory(&root).unwrap();
    for relative in ["reviewer", "coordinator/mappings", "reviews"] {
        assert!(root.join(relative).is_dir());
    }
    assert_transaction_paths(&root, &["reviewer", "coordinator/mappings", "reviews"]);
    assert_failed_rerun_is_read_only(&root);
}

fn assert_replay_checkpoint_failure(
    target: BlindBundleCheckpoint,
    inventory_valid: bool,
    present: &[&str],
    absent: &[&str],
) {
    let (_fixture, replay, pair, prepared) = replay_transaction();
    let root = replay.private_root;
    let mut reached = false;
    let mut inventory_at_fault = None;
    let error = crate::blind_bundle_transaction::commit_blind_bundles_for_test(
        &pair,
        &prepared,
        &mut fixed_clock,
        &mut |checkpoint| {
            if checkpoint == target {
                reached = true;
                inventory_at_fault =
                    Some(fs::read(root.join("coordinator/private-inventory.jsonl"))?);
                anyhow::bail!("injected blind bundle failure");
            }
            Ok(())
        },
    )
    .unwrap_err();
    assert!(reached);
    assert_eq!(error.to_string(), "injected blind bundle failure");
    assert_eq!(
        fs::read(root.join("coordinator/private-inventory.jsonl")).unwrap(),
        inventory_at_fault.unwrap()
    );
    assert_eq!(
        crate::private_inventory::verify_private_inventory(&root).is_ok(),
        inventory_valid
    );
    for relative in present {
        assert!(
            fs::symlink_metadata(root.join(relative)).is_ok(),
            "missing {relative}"
        );
    }
    for relative in absent {
        assert!(
            fs::symlink_metadata(root.join(relative)).is_err(),
            "found {relative}"
        );
    }
    assert_transaction_paths(&root, present);
    assert_failed_rerun_is_read_only(&root);
}

fn assert_native_seed_checkpoint_failure(target: BlindBundleCheckpoint, inventory_valid: bool) {
    let native = run_native_mock_pair_with_marker(
        /*marker*/ None, /*max_total_tokens_per_run*/ 10,
    );
    native.result.as_ref().unwrap();
    let root = native.live_root.canonicalize().unwrap();
    let snapshot =
        crate::blind::read_context_snapshot(&root.join("frozen-run-context.json")).unwrap();
    let core = crate::blind_verify::verify_pair_evidence_core(&snapshot).unwrap();
    let pair = crate::blind_finalize::finalize_blind_pair(core).unwrap();
    let prepared =
        crate::blind_bundle::prepare_native_blind_bundles(&pair, &mut rand::rngs::OsRng).unwrap();
    let mut inventory_at_fault = None;
    let error = crate::blind_bundle_transaction::commit_blind_bundles_for_test(
        &pair,
        &prepared,
        &mut fixed_clock,
        &mut |checkpoint| {
            if checkpoint == target {
                inventory_at_fault =
                    Some(fs::read(root.join("coordinator/private-inventory.jsonl"))?);
                anyhow::bail!("injected native seed failure");
            }
            Ok(())
        },
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "injected native seed failure");
    assert_eq!(
        fs::read(root.join("coordinator/private-inventory.jsonl")).unwrap(),
        inventory_at_fault.unwrap()
    );
    assert_eq!(
        crate::private_inventory::verify_private_inventory(&root).is_ok(),
        inventory_valid
    );
    assert_transaction_paths(
        &root,
        &[
            "reviewer",
            "coordinator/mappings",
            "coordinator/blind-seeds",
        ],
    );
    let before = tree_snapshot(&root);
    let error = crate::blind::run_blind_pack(native_args(&root)).unwrap_err();
    assert!(!error.to_string().is_empty());
    assert_eq!(tree_snapshot(&root), before);
}

fn assert_transaction_paths(root: &Path, present: &[&str]) {
    for relative in [
        ".blind-pack-staging-reviewer",
        "reviewer",
        "coordinator/.blind-pack-staging-mappings",
        "coordinator/mappings",
        "coordinator/.blind-pack-staging-seeds",
        "coordinator/blind-seeds",
        ".blind-pack-staging-reviews",
        "reviews",
        "coordinator/blind-pack-receipt.json",
    ] {
        assert_eq!(
            fs::symlink_metadata(root.join(relative)).is_ok(),
            present.contains(&relative),
            "unexpected transaction path state: {relative}"
        );
    }
}

fn assert_failed_rerun_is_read_only(root: &Path) {
    let before = tree_snapshot(root);
    let error = crate::blind::run_blind_pack(replay_args(root)).unwrap_err();
    assert!(!error.to_string().is_empty());
    assert_eq!(tree_snapshot(root), before);
}

fn replay_args(root: &Path) -> crate::BlindPackArgs {
    crate::BlindPackArgs {
        reviewer_root: PathBuf::from("reviewer"),
        mapping_dir: PathBuf::from("coordinator/mappings"),
        seed_dir: None,
        replay_seeds: ["one", "two", "three"].map(str::to_string).to_vec(),
        frozen_run_context: root.join("frozen-run-context.json"),
    }
}

fn native_args(root: &Path) -> crate::BlindPackArgs {
    crate::BlindPackArgs {
        reviewer_root: PathBuf::from("reviewer"),
        mapping_dir: PathBuf::from("coordinator/mappings"),
        seed_dir: Some(PathBuf::from("coordinator/blind-seeds")),
        replay_seeds: Vec::new(),
        frozen_run_context: root.join("frozen-run-context.json"),
    }
}

fn tree_snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn visit(root: &Path, current: &Path, output: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        let mut entries = fs::read_dir(current)
            .unwrap()
            .collect::<std::io::Result<Vec<_>>>()
            .unwrap();
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            if entry.file_type().unwrap().is_dir() {
                output.insert(relative, None);
                visit(root, &path, output);
            } else {
                output.insert(relative, Some(fs::read(path).unwrap()));
            }
        }
    }
    let mut output = BTreeMap::new();
    visit(root, root, &mut output);
    output
}

#[cfg(unix)]
fn copy_private_tree(source: &Path, destination: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir(destination)?;
    fs::set_permissions(destination, fs::Permissions::from_mode(0o700))?;
    fn copy(source: &Path, destination: &Path) -> anyhow::Result<()> {
        use std::os::unix::fs::MetadataExt;
        use std::os::unix::fs::PermissionsExt;

        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let source_path = entry.path();
            let destination_path = destination.join(entry.file_name());
            let kind = entry.file_type()?;
            if kind.is_dir() {
                fs::create_dir(&destination_path)?;
                fs::set_permissions(&destination_path, fs::Permissions::from_mode(0o700))?;
                copy(&source_path, &destination_path)?;
            } else if kind.is_file() {
                fs::write(&destination_path, fs::read(&source_path)?)?;
                fs::set_permissions(
                    &destination_path,
                    fs::Permissions::from_mode(fs::metadata(source_path)?.mode() & 0o700),
                )?;
            } else {
                anyhow::bail!("private root copy encountered a link or special entry");
            }
        }
        Ok(())
    }
    copy(source, destination)
}

fn replay_transaction() -> (
    tempfile::TempDir,
    ReplayPairTestRun,
    VerifiedBlindPair,
    PreparedBlindBundles,
) {
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
    (fixture, replay, pair, prepared)
}

fn fixed_clock() -> anyhow::Result<chrono::DateTime<chrono::Utc>> {
    Ok(DateTime::parse_from_rfc3339("2026-08-29T12:34:56.789Z")?.to_utc())
}
