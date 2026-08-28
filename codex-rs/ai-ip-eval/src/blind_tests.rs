use std::ffi::OsString;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;

use pretty_assertions::assert_eq;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;
use tempfile::TempDir;

use crate::BlindPackArgs;

fn args_for_mode(execution_mode: &str) -> (TempDir, PathBuf, BlindPackArgs) {
    let temp = TempDir::new().unwrap();
    let private_root = temp.path().join("private");
    fs::create_dir(&private_root).unwrap();
    #[cfg(unix)]
    fs::set_permissions(&private_root, fs::Permissions::from_mode(0o700)).unwrap();
    let private_root = private_root.canonicalize().unwrap();
    let frozen_run_context = private_root.join("frozen-run-context.json");
    fs::write(
        &frozen_run_context,
        serde_json::to_vec(&json!({
            "executionMode": execution_mode,
            "privateRoot": private_root,
        }))
        .unwrap(),
    )
    .unwrap();
    #[cfg(unix)]
    fs::set_permissions(&frozen_run_context, fs::Permissions::from_mode(0o600)).unwrap();
    let args = BlindPackArgs {
        reviewer_root: PathBuf::from("reviewer"),
        mapping_dir: PathBuf::from("coordinator/mappings"),
        seed_dir: None,
        replay_seeds: vec!["one".into(), "two".into(), "three".into()],
        frozen_run_context,
    };
    (temp, private_root, args)
}

fn replay_args() -> (TempDir, PathBuf, BlindPackArgs) {
    args_for_mode("replay")
}

#[test]
fn blind_pack_rejects_nonexact_destination_paths_before_verifier() {
    let mutations = [
        (
            "reviewer-root-absolute",
            PathBuf::from("/reviewer"),
            PathBuf::from("coordinator/mappings"),
            None,
        ),
        (
            "reviewer-root-parent",
            PathBuf::from("../reviewer"),
            PathBuf::from("coordinator/mappings"),
            None,
        ),
        (
            "mapping-dir-wrong",
            PathBuf::from("reviewer"),
            PathBuf::from("mappings"),
            None,
        ),
        (
            "seed-dir-absolute",
            PathBuf::from("reviewer"),
            PathBuf::from("coordinator/mappings"),
            Some(PathBuf::from("/coordinator/blind-seeds")),
        ),
    ];

    for (name, reviewer_root, mapping_dir, seed_dir) in mutations {
        let (_temp, private_root, mut args) = replay_args();
        args.reviewer_root = reviewer_root;
        args.mapping_dir = mapping_dir;
        args.seed_dir = seed_dir;

        let error = crate::blind::run_blind_pack(args).unwrap_err();
        assert_eq!(
            error.to_string(),
            "blind-pack destinations must use the exact private-root-relative path table",
            "mutation {name}"
        );
        assert_no_outputs(&private_root);
    }
}

#[test]
fn blind_pack_rejects_lexical_destination_aliases_before_verifier() {
    let mutations = [
        (
            "reviewer-trailing-separator",
            PathBuf::from("reviewer/"),
            PathBuf::from("coordinator/mappings"),
            None,
        ),
        (
            "reviewer-dot-component",
            PathBuf::from("reviewer/./"),
            PathBuf::from("coordinator/mappings"),
            None,
        ),
        (
            "mapping-repeated-separator",
            PathBuf::from("reviewer"),
            PathBuf::from("coordinator//mappings"),
            None,
        ),
        (
            "mapping-dot-component",
            PathBuf::from("reviewer"),
            PathBuf::from("coordinator/./mappings"),
            None,
        ),
        (
            "seed-repeated-separator",
            PathBuf::from("reviewer"),
            PathBuf::from("coordinator/mappings"),
            Some(PathBuf::from("coordinator//blind-seeds")),
        ),
        (
            "seed-dot-component",
            PathBuf::from("reviewer"),
            PathBuf::from("coordinator/mappings"),
            Some(PathBuf::from("coordinator/./blind-seeds")),
        ),
    ];

    for (name, reviewer_root, mapping_dir, seed_dir) in mutations {
        let (_temp, private_root, mut args) = replay_args();
        args.reviewer_root = reviewer_root;
        args.mapping_dir = mapping_dir;
        args.seed_dir = seed_dir;

        let error = crate::blind::run_blind_pack(args).unwrap_err();
        assert_eq!(
            error.to_string(),
            "blind-pack destinations must use the exact private-root-relative path table",
            "mutation {name}"
        );
        assert_no_outputs(&private_root);
    }
}

#[cfg(windows)]
#[test]
fn blind_pack_rejects_windows_alternate_destination_separators_before_verifier() {
    for (reviewer_root, mapping_dir, seed_dir) in [
        (
            PathBuf::from("reviewer\\"),
            PathBuf::from("coordinator/mappings"),
            None,
        ),
        (
            PathBuf::from("reviewer"),
            PathBuf::from("coordinator\\mappings"),
            None,
        ),
        (
            PathBuf::from("reviewer"),
            PathBuf::from("coordinator/mappings"),
            Some(PathBuf::from("coordinator\\blind-seeds")),
        ),
    ] {
        let (_temp, private_root, mut args) = replay_args();
        args.reviewer_root = reviewer_root;
        args.mapping_dir = mapping_dir;
        args.seed_dir = seed_dir;

        assert_eq!(
            crate::blind::run_blind_pack(args).unwrap_err().to_string(),
            "blind-pack destinations must use the exact private-root-relative path table"
        );
        assert_no_outputs(&private_root);
    }
}

#[test]
fn blind_pack_enforces_mode_disjoint_seed_contract_before_verifier() {
    let replay_mutations = [
        ("two", vec!["one", "two"], None),
        ("duplicate", vec!["one", "two", "one"], None),
        ("empty", vec!["one", "", "three"], None),
        (
            "seed-directory",
            vec!["one", "two", "three"],
            Some(PathBuf::from("coordinator/blind-seeds")),
        ),
    ];
    for (name, seeds, seed_dir) in replay_mutations {
        let (_temp, private_root, mut args) = replay_args();
        args.replay_seeds = seeds.into_iter().map(str::to_owned).collect();
        args.seed_dir = seed_dir;

        let error = crate::blind::run_blind_pack(args).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Replay blind-pack requires exactly three distinct nonempty replay seeds and no seed directory",
            "mutation {name}"
        );
        assert_no_outputs(&private_root);
    }

    let (_temp, private_root, mut missing_seed_dir) = args_for_mode("live");
    missing_seed_dir.replay_seeds.clear();
    let error = crate::blind::run_blind_pack(missing_seed_dir).unwrap_err();
    assert_eq!(
        error.to_string(),
        "Mock/Live blind-pack requires the exact seed directory and no replay seeds"
    );
    assert_no_outputs(&private_root);

    let (_temp, private_root, mut replay_seed) = args_for_mode("live");
    replay_seed.seed_dir = Some(PathBuf::from("coordinator/blind-seeds"));
    let error = crate::blind::run_blind_pack(replay_seed).unwrap_err();
    assert_eq!(
        error.to_string(),
        "Mock/Live blind-pack requires the exact seed directory and no replay seeds"
    );
    assert_no_outputs(&private_root);

    let (_temp, private_root, mut valid) = args_for_mode("live");
    valid.seed_dir = Some(PathBuf::from("coordinator/blind-seeds"));
    valid.replay_seeds.clear();
    let error = crate::blind::run_blind_pack(valid).unwrap_err();
    assert_eq!(error.to_string(), "validate frozen native context contract");
    assert_no_outputs(&private_root);
}

#[test]
fn blind_pack_requires_absolute_canonical_context_bound_to_private_root() {
    let (_temp, private_root, mut relative) = replay_args();
    relative.frozen_run_context = PathBuf::from("frozen-run-context.json");
    assert_eq!(
        crate::blind::run_blind_pack(relative)
            .unwrap_err()
            .to_string(),
        "blind-pack frozen context must be an absolute canonical path"
    );
    assert_no_outputs(&private_root);

    let (_temp, private_root, mut noncanonical) = replay_args();
    let intermediate = private_root.join("intermediate");
    fs::create_dir(&intermediate).unwrap();
    #[cfg(unix)]
    fs::set_permissions(&intermediate, fs::Permissions::from_mode(0o700)).unwrap();
    noncanonical.frozen_run_context = intermediate.join("../frozen-run-context.json");
    assert_eq!(
        crate::blind::run_blind_pack(noncanonical)
            .unwrap_err()
            .to_string(),
        "blind-pack frozen context must be an absolute canonical path"
    );
    assert_no_outputs(&private_root);

    let (temp, private_root, mismatched) = replay_args();
    let other_root = temp.path().join("other-private");
    fs::create_dir(&other_root).unwrap();
    #[cfg(unix)]
    fs::set_permissions(&other_root, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(
        &mismatched.frozen_run_context,
        serde_json::to_vec(&json!({
            "executionMode": "replay",
            "privateRoot": other_root.canonicalize().unwrap(),
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        crate::blind::run_blind_pack(mismatched)
            .unwrap_err()
            .to_string(),
        "blind-pack frozen context is outside its declared private root"
    );
    assert_no_outputs(&private_root);
}

#[test]
fn blind_pack_rejects_lexical_context_and_private_root_aliases() {
    let separator = std::path::MAIN_SEPARATOR;
    for suffix in [
        format!("{separator}{separator}frozen-run-context.json"),
        format!("{separator}.{separator}frozen-run-context.json"),
        format!("{separator}frozen-run-context.json{separator}"),
    ] {
        let (_temp, private_root, mut args) = replay_args();
        let mut alias = OsString::from(private_root.as_os_str());
        alias.push(suffix);
        args.frozen_run_context = PathBuf::from(alias);

        assert_eq!(
            crate::blind::run_blind_pack(args).unwrap_err().to_string(),
            "blind-pack frozen context must be an absolute canonical path"
        );
        assert_no_outputs(&private_root);
    }

    for suffix in [
        format!("{separator}."),
        format!("{separator}{separator}"),
        separator.to_string(),
    ] {
        let (_temp, private_root, args) = replay_args();
        let mut alias = OsString::from(private_root.as_os_str());
        alias.push(suffix);
        fs::write(
            &args.frozen_run_context,
            serde_json::to_vec(&json!({
                "executionMode": "replay",
                "privateRoot": PathBuf::from(alias),
            }))
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            crate::blind::run_blind_pack(args).unwrap_err().to_string(),
            "blind-pack frozen context is outside its declared private root"
        );
        assert_no_outputs(&private_root);
    }
}

#[cfg(windows)]
#[test]
fn blind_pack_rejects_windows_alternate_context_spelling() {
    let (_temp, private_root, mut args) = replay_args();
    args.frozen_run_context =
        PathBuf::from(args.frozen_run_context.to_string_lossy().replace('\\', "/"));
    assert_eq!(
        crate::blind::run_blind_pack(args).unwrap_err().to_string(),
        "blind-pack frozen context must be an absolute canonical path"
    );
    assert_no_outputs(&private_root);

    let (_temp, private_root, args) = replay_args();
    fs::write(
        &args.frozen_run_context,
        serde_json::to_vec(&json!({
            "executionMode": "replay",
            "privateRoot": private_root.to_string_lossy().replace('\\', "/"),
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        crate::blind::run_blind_pack(args).unwrap_err().to_string(),
        "blind-pack frozen context is outside its declared private root"
    );
    assert_no_outputs(&private_root);
}

#[test]
fn blind_pack_rejects_all_existing_output_destinations_before_verifier() {
    for relative in [
        "reviewer",
        "coordinator/mappings",
        "coordinator/blind-seeds",
        "reviews",
        "coordinator/blind-pack-receipt.json",
    ] {
        let (_temp, private_root, args) = replay_args();
        let destination = private_root.join(relative);
        let parent = destination.parent().unwrap();
        if parent != private_root {
            fs::create_dir_all(parent).unwrap();
            #[cfg(unix)]
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).unwrap();
        }
        if relative.ends_with(".json") {
            fs::write(&destination, b"occupied").unwrap();
            #[cfg(unix)]
            fs::set_permissions(&destination, fs::Permissions::from_mode(0o600)).unwrap();
        } else {
            fs::create_dir(&destination).unwrap();
            #[cfg(unix)]
            fs::set_permissions(&destination, fs::Permissions::from_mode(0o700)).unwrap();
        }

        let before = output_states(&private_root);
        let error = crate::blind::run_blind_pack(args).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("blind-pack output already exists: {relative}")
        );
        assert_eq!(output_states(&private_root), before);
    }
}

#[cfg(unix)]
#[test]
fn blind_pack_rejects_linked_output_ancestors_and_hardlinks_before_verifier() {
    use std::os::unix::fs::symlink;

    let (temp, private_root, args) = replay_args();
    let external = temp.path().join("external");
    fs::create_dir(&external).unwrap();
    fs::set_permissions(&external, fs::Permissions::from_mode(0o700)).unwrap();
    symlink(&external, private_root.join("coordinator")).unwrap();
    let before = output_states(&private_root);
    let error = crate::blind::run_blind_pack(args).unwrap_err();
    assert!(format!("{error:#}").contains("link"));
    assert_eq!(output_states(&private_root), before);
    assert_eq!(
        fs::read_link(private_root.join("coordinator")).unwrap(),
        external
    );

    let (_temp, private_root, args) = replay_args();
    let coordinator = private_root.join("coordinator");
    fs::create_dir(&coordinator).unwrap();
    fs::set_permissions(&coordinator, fs::Permissions::from_mode(0o700)).unwrap();
    let original = private_root.join("occupied");
    fs::write(&original, b"occupied").unwrap();
    fs::set_permissions(&original, fs::Permissions::from_mode(0o600)).unwrap();
    fs::hard_link(&original, coordinator.join("blind-pack-receipt.json")).unwrap();
    let before = output_states(&private_root);
    let error = crate::blind::run_blind_pack(args).unwrap_err();
    assert!(format!("{error:#}").contains("link"));
    assert_eq!(output_states(&private_root), before);
    assert_eq!(fs::read(original).unwrap(), b"occupied");
}

#[cfg(unix)]
#[test]
fn blind_pack_reads_frozen_context_through_single_link_secure_fs() {
    let (_temp, private_root, args) = replay_args();
    let retained = args.frozen_run_context.with_file_name("retained-context");
    fs::rename(&args.frozen_run_context, &retained).unwrap();
    fs::hard_link(&retained, &args.frozen_run_context).unwrap();

    let error = crate::blind::run_blind_pack(args).unwrap_err();
    assert!(format!("{error:#}").contains("link"));
    assert_no_outputs(&private_root);
}

#[test]
fn frozen_context_snapshot_retains_exact_bytes_digest_and_placeholder_input() {
    let (_temp, private_root, args) = replay_args();
    let initial_raw = fs::read(&args.frozen_run_context).unwrap();
    let initial = crate::blind::read_context_snapshot(&args.frozen_run_context).unwrap();
    assert_eq!(initial.raw_bytes(), initial_raw.as_slice());
    assert_eq!(
        initial.sha256(),
        format!("{:x}", Sha256::digest(&initial_raw))
    );
    assert_eq!(initial.canonical_path(), args.frozen_run_context.as_path());

    fs::write(
        &args.frozen_run_context,
        serde_json::to_vec(&json!({
            "executionMode": "replay",
            "privateRoot": private_root,
            "snapshotNonce": 1,
        }))
        .unwrap(),
    )
    .unwrap();
    let changed = crate::blind::read_context_snapshot(&args.frozen_run_context).unwrap();
    assert_ne!(changed.raw_bytes(), initial.raw_bytes());
    assert_ne!(changed.sha256(), initial.sha256());

    assert_eq!(
        crate::blind::verify_blind_pair_stage(&args, &initial)
            .unwrap_err()
            .to_string(),
        "reverify retained blind-pack frozen context identity"
    );
    assert_no_outputs(&private_root);
}

fn assert_no_outputs(private_root: &Path) {
    assert_eq!(output_states(private_root), vec![OutputState::Missing; 5]);
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum OutputState {
    Missing,
    File(Vec<u8>),
    Directory(Vec<OsString>),
    Symlink(PathBuf),
}

fn output_states(private_root: &Path) -> Vec<OutputState> {
    [
        "reviewer",
        "coordinator/mappings",
        "coordinator/blind-seeds",
        "reviews",
        "coordinator/blind-pack-receipt.json",
    ]
    .into_iter()
    .map(|relative| {
        let path = private_root.join(relative);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => OutputState::Missing,
            Err(error) => panic!("inspect {relative}: {error}"),
            Ok(metadata) if metadata.file_type().is_symlink() => {
                OutputState::Symlink(fs::read_link(path).unwrap())
            }
            Ok(metadata) if metadata.is_file() => OutputState::File(fs::read(path).unwrap()),
            Ok(metadata) if metadata.is_dir() => {
                let mut entries = fs::read_dir(path)
                    .unwrap()
                    .map(|entry| entry.unwrap().file_name())
                    .collect::<Vec<_>>();
                entries.sort();
                OutputState::Directory(entries)
            }
            Ok(_) => panic!("unexpected output type at {relative}"),
        }
    })
    .collect()
}
