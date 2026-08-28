use crate::private_inventory::InventoryRecord;
use crate::private_inventory::PairMarker;
use pretty_assertions::assert_eq;
use sha2::Digest;
use std::collections::BTreeSet;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
const PAIR_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const FROZEN_SHA: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const CREATED_AT: &str = "2026-08-28T12:00:00Z";
const INVENTORY: &str = "coordinator/private-inventory.jsonl";
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
    crate::secure_fs::create_owner_only_dir_new(&root.join("nested")).unwrap();
    crate::secure_fs::write_owner_only_new(&root.join("nested/evidence.bin"), b"evidence").unwrap();
    bootstrap(&root).unwrap();
    (temp, root)
}
fn bootstrap(root: &Path) -> anyhow::Result<String> {
    crate::private_inventory::bootstrap_private_inventory(root, PAIR_ID, FROZEN_SHA, CREATED_AT)
}
fn append(root: &Path, relative: &str) -> anyhow::Result<String> {
    crate::private_inventory::append_private_inventory(root, Path::new(relative))
}
fn tamper(edit: impl FnOnce(&Path)) {
    let (_temp, root) = sealed_root();
    edit(&root);
    assert!(crate::private_inventory::verify_private_inventory(&root).is_err());
}
#[cfg(unix)]
fn rejects_before_seal(setup: impl FnOnce(&Path)) {
    let (_temp, root) = owner_root();
    setup(&root);
    assert!(bootstrap(&root).is_err());
    assert!(!root.join("coordinator").exists());
}
fn frozen_replay_pair() -> anyhow::Result<(tempfile::TempDir, PathBuf)> {
    let fixture_set =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json")?;
    let fixture_root = fixture_set.parent().unwrap();
    let (temp, private_root) = owner_root();
    let codex_binary = private_root.join("synthetic-codex");
    crate::secure_fs::write_owner_only_new(&codex_binary, b"synthetic replay binary\n")?;
    let frozen = private_root.join("frozen-run-context.json");
    crate::freeze_replay_context(crate::model::ReplayFreezeArgs {
        repo_root: fixture_root.to_path_buf(),
        fork_sha: "synthetic-replay-fork".to_string(),
        private_root: private_root.clone(),
        codex_bin: codex_binary,
        case: fixture_root.join("replay-case.json"),
        transcript: fixture_root.join("replay-transcript.jsonl"),
        fixture_set_manifest: fixture_set,
        output: frozen.clone(),
    })?;
    crate::run_replay_pair(crate::ReplayPairArgs {
        frozen_run_context: frozen,
    })?;
    Ok((temp, private_root))
}
fn inventory_path(root: &Path) -> PathBuf {
    root.join(INVENTORY)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}
fn inventory(root: &Path) -> (Vec<Vec<u8>>, Vec<InventoryRecord>) {
    let bytes = fs::read(inventory_path(root)).unwrap();
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
fn actual_paths(root: &Path) -> BTreeSet<String> {
    fn walk(root: &Path, directory: &Path, paths: &mut BTreeSet<String>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if relative == "coordinator/private-inventory.jsonl" {
                continue;
            }
            paths.insert(relative);
            if path.is_dir() {
                walk(root, &path, paths);
            }
        }
    }
    let mut paths = BTreeSet::new();
    walk(root, root, &mut paths);
    paths
}
#[test]
fn replay_pair_creates_a_complete_hash_chained_private_inventory() {
    let (_temp, private_root) = frozen_replay_pair().unwrap();
    let marker_bytes = fs::read(private_root.join("coordinator/pair-marker.json")).unwrap();
    let marker: PairMarker = serde_json::from_slice(&marker_bytes).unwrap();
    let frozen_bytes = fs::read(private_root.join("frozen-run-context.json")).unwrap();
    let frozen: serde_json::Value = serde_json::from_slice(&frozen_bytes).unwrap();
    assert_eq!(marker.schema_version, 1);
    assert_eq!(marker.pair_id, frozen["pairId"].as_str().unwrap());
    assert_eq!(marker.frozen_run_context_sha256, digest(&frozen_bytes));
    assert_eq!(marker.private_root, private_root.to_str().unwrap());
    assert_eq!(marker.inventory_relative_path, INVENTORY);
    chrono::DateTime::parse_from_rfc3339(&marker.created_at).unwrap();
    let marker_value = crate::jcs::parse_json(&marker_bytes).unwrap();
    assert_eq!(
        marker_bytes,
        crate::jcs::canonicalize_value(&marker_value).unwrap()
    );
    let (lines, records) = inventory(&private_root);
    let paths: BTreeSet<_> = records
        .iter()
        .map(|record| record.relative_path.clone())
        .collect();
    assert_eq!(paths, actual_paths(&private_root));
    assert!(
        records
            .windows(2)
            .all(|pair| pair[0].relative_path < pair[1].relative_path)
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
    let bytes = fs::read(inventory_path(&private_root)).unwrap();
    assert_eq!(
        crate::private_inventory::verify_private_inventory(&private_root).unwrap(),
        digest(&bytes)
    );
}
#[test]
fn private_inventory_append_preserves_exact_prefix_and_root() {
    let (_temp, root) = sealed_root();
    let prefix_root = crate::private_inventory::verify_private_inventory(&root).unwrap();
    crate::secure_fs::write_owner_only_new(&root.join("later.bin"), b"later").unwrap();
    let final_root = append(&root, "later.bin").unwrap();
    let final_bytes = fs::read(inventory_path(&root)).unwrap();
    let (lines, records) = inventory(&root);
    assert_eq!(final_root, digest(&final_bytes));
    assert_ne!(prefix_root, final_root);
    assert_eq!(
        records.last().unwrap().previous_record_sha256,
        Some(digest(&lines[lines.len() - 2]))
    );
    assert_eq!(records.last().unwrap().relative_path, "later.bin");
    assert!(append(&root, "later.bin").is_err());
    assert!(append(&root, INVENTORY).is_err());
}
#[test]
fn private_inventory_rejects_marker_chain_and_tree_tamper() {
    tamper(|root| {
        let marker = root.join("coordinator/pair-marker.json");
        let mut bytes = fs::read(&marker).unwrap();
        bytes.insert(0, b' ');
        fs::write(marker, bytes).unwrap();
    });
    tamper(|root| {
        let inventory = inventory_path(root);
        let mut bytes = fs::read(&inventory).unwrap();
        bytes[0] ^= 1;
        fs::write(inventory, bytes).unwrap();
    });
    tamper(|root| fs::write(root.join("proof.bin"), b"changed").unwrap());
    tamper(|root| fs::remove_file(root.join("proof.bin")).unwrap());
    tamper(|root| {
        crate::secure_fs::write_owner_only_new(&root.join("extra.bin"), b"extra").unwrap()
    });
}
#[cfg(unix)]
#[test]
fn private_inventory_rejects_links_without_outputs() {
    rejects_before_seal(|root| {
        crate::secure_fs::write_owner_only_new(&root.join("proof.bin"), b"proof").unwrap();
        fs::hard_link(root.join("proof.bin"), root.join("hardlink.bin")).unwrap();
    });
    rejects_before_seal(|root| std::os::unix::fs::symlink("missing", root.join("link")).unwrap());
}
#[test]
fn private_inventory_rejects_noncanonical_root_and_existing_destination() {
    let (_temp, root) = owner_root();
    let noncanonical = PathBuf::from(format!("{}/", root.display()));
    assert!(bootstrap(&noncanonical).is_err());
    assert!(!root.join("coordinator").exists());
    crate::secure_fs::create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    crate::secure_fs::write_owner_only_new(&inventory_path(&root), b"occupied").unwrap();
    assert!(bootstrap(&root).is_err());
    assert_eq!(fs::read(inventory_path(&root)).unwrap(), b"occupied");
    assert!(!root.join("coordinator/pair-marker.json").exists());
}
#[test]
fn private_inventory_windows_stream_snapshot_policy_is_exact() {
    let encoded = |name: &str| {
        name.encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
    };
    let default = encoded("::$DATA");
    let named = encoded(":hidden:$DATA");
    let validate = |buffer: &[u8], name_bytes, next_offset, stream_size, directory| {
        crate::runner::validate_windows_stream_snapshot_for_test(
            buffer,
            0,
            name_bytes,
            next_offset,
            stream_size,
            directory,
        )
    };
    assert!(validate(&default, default.len(), 0, 0, true).is_ok());
    assert!(validate(&default, default.len(), 0, 9, false).is_ok());
    assert!(validate(&default, default.len(), 0, 1, true).is_err());
    assert!(validate(&named, named.len(), 0, 0, true).is_err());
    assert!(validate(&named, named.len(), 0, 9, false).is_err());
    assert!(validate(&default, default.len(), 8, 0, false).is_err());
    assert!(validate(&default, default.len() - 1, 0, 0, false).is_err());
    assert!(
        crate::runner::validate_windows_stream_snapshot_for_test(
            &default,
            1,
            default.len(),
            0,
            0,
            false,
        )
        .is_err()
    );
}
#[cfg(windows)]
fn add_stream(path: &Path) -> PathBuf {
    let stream = PathBuf::from(format!("{}:hidden", path.display()));
    fs::write(&stream, b"hidden").unwrap();
    stream
}
#[cfg(windows)]
#[test]
fn private_inventory_windows_coordinator_append_and_stream_policy() {
    let (_temp, root) = owner_root();
    crate::secure_fs::write_owner_only_new(&root.join("proof.bin"), b"proof").unwrap();
    crate::runner::prepare_pair_coordinator(&root.join("coordinator")).unwrap();
    bootstrap(&root).unwrap();
    let prefix = fs::read(inventory_path(&root)).unwrap();
    crate::secure_fs::write_owner_only_new(&root.join("later.bin"), b"later").unwrap();
    let final_root = append(&root, "later.bin").unwrap();
    let complete = fs::read(inventory_path(&root)).unwrap();
    assert!(complete.starts_with(&prefix));
    assert_eq!(final_root, digest(&complete));
    assert_eq!(
        crate::private_inventory::verify_private_inventory(&root).unwrap(),
        final_root
    );

    let (_pre_temp, pre_root) = owner_root();
    crate::secure_fs::write_owner_only_new(&pre_root.join("proof.bin"), b"proof").unwrap();
    crate::secure_fs::create_owner_only_dir_new(&pre_root.join("nested")).unwrap();
    for relative in ["proof.bin", "nested"] {
        let stream = add_stream(&pre_root.join(relative));
        assert!(bootstrap(&pre_root).is_err());
        fs::remove_file(stream).unwrap();
    }
    let (_sealed_temp, sealed_root) = sealed_root();
    for relative in [
        "proof.bin",
        "nested",
        "coordinator/pair-marker.json",
        INVENTORY,
    ] {
        let stream = add_stream(&sealed_root.join(relative));
        assert!(crate::private_inventory::verify_private_inventory(&sealed_root).is_err());
        fs::remove_file(stream).unwrap();
        crate::private_inventory::verify_private_inventory(&sealed_root).unwrap();
    }
}
