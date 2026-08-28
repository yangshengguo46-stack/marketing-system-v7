use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;

use pretty_assertions::assert_eq;
use serde_json::json;
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

    for execution_mode in ["mock", "live"] {
        let (_temp, private_root, mut missing_seed_dir) = args_for_mode(execution_mode);
        missing_seed_dir.replay_seeds.clear();
        let error = crate::blind::run_blind_pack(missing_seed_dir).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Mock/Live blind-pack requires the exact seed directory and no replay seeds"
        );
        assert_no_outputs(&private_root);

        let (_temp, private_root, mut replay_seed) = args_for_mode(execution_mode);
        replay_seed.seed_dir = Some(PathBuf::from("coordinator/blind-seeds"));
        let error = crate::blind::run_blind_pack(replay_seed).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Mock/Live blind-pack requires the exact seed directory and no replay seeds"
        );
        assert_no_outputs(&private_root);

        let (_temp, private_root, mut valid) = args_for_mode(execution_mode);
        valid.seed_dir = Some(PathBuf::from("coordinator/blind-seeds"));
        valid.replay_seeds.clear();
        let error = crate::blind::run_blind_pack(valid).unwrap_err();
        assert_eq!(error.to_string(), "BlindPairVerifierStageNotInstalled");
        assert_no_outputs(&private_root);
    }
}

#[test]
fn blind_pack_requires_absolute_canonical_context_bound_to_private_root() {
    let (_temp, _private_root, mut relative) = replay_args();
    relative.frozen_run_context = PathBuf::from("frozen-run-context.json");
    assert_eq!(
        crate::blind::run_blind_pack(relative)
            .unwrap_err()
            .to_string(),
        "blind-pack frozen context must be an absolute canonical path"
    );

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

        let error = crate::blind::run_blind_pack(args).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("blind-pack output already exists: {relative}")
        );
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
    let error = crate::blind::run_blind_pack(args).unwrap_err();
    assert!(format!("{error:#}").contains("link"));
    assert!(!private_root.join("reviewer").exists());
    assert!(!private_root.join("reviews").exists());

    let (_temp, private_root, args) = replay_args();
    let coordinator = private_root.join("coordinator");
    fs::create_dir(&coordinator).unwrap();
    fs::set_permissions(&coordinator, fs::Permissions::from_mode(0o700)).unwrap();
    let original = private_root.join("occupied");
    fs::write(&original, b"occupied").unwrap();
    fs::set_permissions(&original, fs::Permissions::from_mode(0o600)).unwrap();
    fs::hard_link(&original, coordinator.join("blind-pack-receipt.json")).unwrap();
    let error = crate::blind::run_blind_pack(args).unwrap_err();
    assert!(format!("{error:#}").contains("link"));
    assert!(!private_root.join("reviewer").exists());
    assert!(!private_root.join("reviews").exists());
}

#[cfg(unix)]
#[test]
fn blind_pack_reads_frozen_context_through_single_link_secure_fs() {
    let (_temp, _private_root, args) = replay_args();
    let retained = args.frozen_run_context.with_file_name("retained-context");
    fs::rename(&args.frozen_run_context, &retained).unwrap();
    fs::hard_link(&retained, &args.frozen_run_context).unwrap();

    let error = crate::blind::run_blind_pack(args).unwrap_err();
    assert!(format!("{error:#}").contains("link"));
}

fn assert_no_outputs(private_root: &Path) {
    for relative in [
        "reviewer",
        "coordinator/mappings",
        "coordinator/blind-seeds",
        "reviews",
        "coordinator/blind-pack-receipt.json",
    ] {
        assert!(!private_root.join(relative).exists(), "created {relative}");
    }
}
