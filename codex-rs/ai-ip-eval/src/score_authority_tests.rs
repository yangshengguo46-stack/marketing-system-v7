use std::path::Path;
use std::path::PathBuf;

use pretty_assertions::assert_eq;
use sha2::Digest;

use crate::ExecutionMode;
use crate::FrozenContracts;
use crate::ScoreArgs;
use crate::blind_bundle_model::BlindPackReceipt;
use crate::blind_bundle_model::ReviewerMappingCommitment;
use crate::score_authority::ReceiptAuthorityBinding;

fn score_root() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    #[cfg(unix)]
    let root = temp.path().canonicalize().unwrap();
    #[cfg(windows)]
    let root = {
        let root = temp.path().join("private");
        crate::secure_fs::create_owner_only_dir_new(&root).unwrap();
        root.canonicalize().unwrap()
    };
    for relative in ["coordinator", "coordinator/mappings", "reviews"] {
        crate::secure_fs::create_owner_only_dir_new(&root.join(relative)).unwrap();
    }
    crate::secure_fs::write_owner_only_new(&root.join("frozen-run-context.json"), b"frozen")
        .unwrap();
    (temp, root)
}

fn args(root: &Path) -> ScoreArgs {
    ScoreArgs {
        mapping_dir: PathBuf::from("coordinator/mappings"),
        reviews_dir: PathBuf::from("reviews"),
        output: PathBuf::from("coordinator/decision.private.json"),
        frozen_run_context: root.join("frozen-run-context.json"),
    }
}

fn assert_no_score_outputs(root: &Path) {
    for relative in [
        "coordinator/.decision.private.json.staging",
        "coordinator/decision.private.json",
    ] {
        assert_eq!(
            std::fs::symlink_metadata(root.join(relative))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }
}

#[test]
fn score_authority_accepts_native_receipt_binding_and_requires_its_pair_receipt() {
    let contracts = FrozenContracts::load()
        .unwrap()
        .blind_review_contracts()
        .unwrap();
    let sha = format!("{:x}", sha2::Sha256::digest(b"score authority test"));
    let reviewer_ids = [
        "native-reviewer-1",
        "native-reviewer-2",
        "native-reviewer-3",
    ];
    let commitment_shas = ["reviewer one", "reviewer two", "reviewer three"]
        .map(|label| format!("{:x}", sha2::Sha256::digest(label.as_bytes())));
    let receipt = BlindPackReceipt {
        schema_version: 1,
        pair_id: "native-pair".to_string(),
        frozen_run_context_sha256: sha.clone(),
        pair_receipt_sha256: Some(sha.clone()),
        pair_verification_sha256: sha.clone(),
        rubric_sha256: contracts.rubric_sha256,
        decision_policy_sha256: contracts.decision_policy_sha256,
        reviewer_submission_schema_sha256: contracts.reviewer_submission_schema_sha256,
        reviewer_mappings: reviewer_ids
            .iter()
            .zip(&commitment_shas)
            .map(|(reviewer_id, commitment_sha)| ReviewerMappingCommitment {
                reviewer_id: reviewer_id.to_string(),
                review_bundle_sha256: commitment_sha.clone(),
                mapping_sha256: commitment_sha.clone(),
                seed_commitment: commitment_sha.clone(),
            })
            .collect(),
        inventory_root_sha256: sha.clone(),
        reviews_drop_sha256: format!("{:x}", sha2::Sha256::digest(br#"{"entries":[]}"#)),
        generated_at: "2026-08-29T12:34:56.789Z".to_string(),
    };
    let binding = ReceiptAuthorityBinding {
        mode: ExecutionMode::Mock,
        pair_id: "native-pair",
        frozen_run_context_sha256: &sha,
        pair_receipt_sha256: Some(&sha),
        pair_verification_sha256: &sha,
        reviewer_ids,
    };
    crate::score_authority::verify_receipt(&receipt, &binding).unwrap();

    let mut duplicate_commitment = receipt.clone();
    duplicate_commitment.reviewer_mappings[1].review_bundle_sha256 = duplicate_commitment
        .reviewer_mappings[0]
        .review_bundle_sha256
        .clone();
    assert_eq!(
        crate::score_authority::verify_receipt(&duplicate_commitment, &binding)
            .unwrap_err()
            .to_string(),
        "blind pack receipt differs from the sealed score authority"
    );

    let without_native_pair_receipt = BlindPackReceipt {
        pair_receipt_sha256: None,
        ..receipt
    };
    assert_eq!(
        crate::score_authority::verify_receipt(&without_native_pair_receipt, &binding)
            .unwrap_err()
            .to_string(),
        "blind pack receipt differs from the sealed score authority"
    );
}

#[test]
fn score_authority_resolves_only_the_two_frozen_path_spellings() {
    let (_temp, root) = score_root();
    let relative = crate::score_authority::resolve_score_paths(&args(&root), &root).unwrap();
    assert_eq!(relative.mapping_dir(), root.join("coordinator/mappings"));
    assert_eq!(relative.reviews_dir(), root.join("reviews"));
    assert_eq!(
        relative.output(),
        root.join("coordinator/decision.private.json")
    );
    assert_eq!(
        relative.staging_output(),
        root.join("coordinator/.decision.private.json.staging")
    );
    assert_eq!(
        relative.frozen_run_context(),
        root.join("frozen-run-context.json")
    );
    assert_no_score_outputs(&root);

    let mut absolute = args(&root);
    absolute.reviews_dir = root.join("reviews");
    absolute.output = root.join("coordinator/decision.private.json");
    crate::score_authority::resolve_score_paths(&absolute, &root).unwrap();
    assert_no_score_outputs(&root);
}

#[test]
fn score_authority_rejects_aliases_and_paths_outside_the_private_root() {
    let (_temp, root) = score_root();
    let mutations = [
        (
            "mapping absolute",
            root.join("coordinator/mappings"),
            PathBuf::from("reviews"),
            PathBuf::from("coordinator/decision.private.json"),
            root.join("frozen-run-context.json"),
        ),
        (
            "mapping dot",
            PathBuf::from("coordinator/./mappings"),
            PathBuf::from("reviews"),
            PathBuf::from("coordinator/decision.private.json"),
            root.join("frozen-run-context.json"),
        ),
        (
            "reviews dot",
            PathBuf::from("coordinator/mappings"),
            PathBuf::from("./reviews"),
            PathBuf::from("coordinator/decision.private.json"),
            root.join("frozen-run-context.json"),
        ),
        (
            "reviews outside",
            PathBuf::from("coordinator/mappings"),
            root.parent().unwrap().join("reviews"),
            PathBuf::from("coordinator/decision.private.json"),
            root.join("frozen-run-context.json"),
        ),
        (
            "output dot",
            PathBuf::from("coordinator/mappings"),
            PathBuf::from("reviews"),
            PathBuf::from("coordinator/./decision.private.json"),
            root.join("frozen-run-context.json"),
        ),
        (
            "output outside",
            PathBuf::from("coordinator/mappings"),
            PathBuf::from("reviews"),
            root.parent().unwrap().join("decision.private.json"),
            root.join("frozen-run-context.json"),
        ),
        (
            "frozen alias",
            PathBuf::from("coordinator/mappings"),
            PathBuf::from("reviews"),
            PathBuf::from("coordinator/decision.private.json"),
            root.join("nested/../frozen-run-context.json"),
        ),
    ];
    for (label, mapping_dir, reviews_dir, output, frozen_run_context) in mutations {
        let mutated = ScoreArgs {
            mapping_dir,
            reviews_dir,
            output,
            frozen_run_context,
        };
        assert!(
            crate::score_authority::resolve_score_paths(&mutated, &root).is_err(),
            "accepted {label}"
        );
    }
    assert_no_score_outputs(&root);
}

#[test]
fn score_authority_requires_absent_staging_and_final_output() {
    for occupied in [
        "coordinator/decision.private.json",
        "coordinator/.decision.private.json.staging",
    ] {
        let (_temp, root) = score_root();
        crate::secure_fs::write_owner_only_new(&root.join(occupied), b"occupied").unwrap();
        assert!(crate::score_authority::resolve_score_paths(&args(&root), &root).is_err());
    }
}

#[test]
fn score_authority_requires_an_existing_regular_frozen_context_leaf() {
    let (_temp, root) = score_root();
    std::fs::remove_file(root.join("frozen-run-context.json")).unwrap();
    assert!(crate::score_authority::resolve_score_paths(&args(&root), &root).is_err());

    let (_temp, root) = score_root();
    std::fs::remove_file(root.join("frozen-run-context.json")).unwrap();
    crate::secure_fs::create_owner_only_dir_new(&root.join("frozen-run-context.json")).unwrap();
    assert!(crate::score_authority::resolve_score_paths(&args(&root), &root).is_err());
}

#[cfg(unix)]
#[test]
fn score_authority_rejects_an_exact_frozen_context_symlink() {
    let (_temp, root) = score_root();
    std::fs::rename(
        root.join("frozen-run-context.json"),
        root.join("retained-frozen-context.json"),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        "retained-frozen-context.json",
        root.join("frozen-run-context.json"),
    )
    .unwrap();

    assert!(crate::score_authority::resolve_score_paths(&args(&root), &root).is_err());
}

#[cfg(unix)]
#[test]
fn score_authority_rejects_replaced_input_directories() {
    let (_temp, root) = score_root();
    std::fs::rename(root.join("reviews"), root.join("retained-reviews")).unwrap();
    std::os::unix::fs::symlink("retained-reviews", root.join("reviews")).unwrap();

    assert!(crate::score_authority::resolve_score_paths(&args(&root), &root).is_err());
}
