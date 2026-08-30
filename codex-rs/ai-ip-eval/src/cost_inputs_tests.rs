use std::fs;
use std::path::Path;

use sha2::Digest;
use sha2::Sha256;

use crate::EvaluationCondition;
use crate::cost_inputs::RetainedPendingSupplierStatement;

fn owner_root() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
    }
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

fn supplier_bytes() -> Vec<u8> {
    fs::read(
        codex_utils_cargo_bin::find_resource!(
            "tests/fixtures/contracts/06b1/supplier-statement.canonical.json"
        )
        .unwrap(),
    )
    .unwrap()
}

fn supplier_leaf(root: &Path, bytes: &[u8]) -> std::path::PathBuf {
    let directory = root.join("inputs/supplier-statements");
    crate::secure_fs::create_owner_only_dir_new(&root.join("inputs")).unwrap();
    crate::secure_fs::create_owner_only_dir_new(&directory).unwrap();
    let path = directory.join("candidate.json");
    crate::secure_fs::write_owner_only_new(&path, bytes).unwrap();
    path
}

#[cfg(unix)]
#[test]
fn retained_exact_private_input_binds_raw_bytes_type_sha_and_identity() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let root = temp.path().canonicalize().unwrap();
    let path = root.join("rate-card.json");
    let fixture = codex_utils_cargo_bin::find_resource!(
        "tests/fixtures/contracts/06b1/provider-rate-card.canonical.json"
    )
    .unwrap();
    let bytes = fs::read(fixture).unwrap();
    fs::write(&path, &bytes).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let contracts = crate::cost_contracts::FrozenCostContracts::load().unwrap();
    let retained = crate::cost_inputs::RetainedExactPrivateInput::retain(&path, &path, |raw| {
        contracts.validate_rate_card(raw)
    })
    .unwrap();

    assert_eq!(retained.raw_bytes(), bytes);
    assert_eq!(retained.typed().provider_label, "approved-provider");
    assert_eq!(retained.sha256(), format!("{:x}", Sha256::digest(&bytes)));

    let displaced = root.join("displaced-rate-card.json");
    fs::rename(&path, displaced).unwrap();
    fs::write(&path, &bytes).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(retained.reverify_unchanged().is_err());
}

#[test]
fn retained_pending_supplier_binds_condition_root_bytes_type_and_sha() {
    let (_temp, root) = owner_root();
    let bytes = supplier_bytes();
    let path = supplier_leaf(&root, &bytes);
    let retained = RetainedPendingSupplierStatement::retain(
        &root,
        EvaluationCondition::Candidate,
        &path,
    )
    .unwrap();

    assert_eq!(retained.relative_path(), "inputs/supplier-statements/candidate.json");
    assert_eq!(retained.raw_bytes(), bytes);
    assert_eq!(retained.typed().condition, EvaluationCondition::Candidate);
    assert_eq!(retained.sha256(), format!("{:x}", Sha256::digest(&bytes)));
    assert_eq!(retained.inventory_entry().sha256.as_deref(), Some(retained.sha256()));
    assert!(retained.is_pending());
    retained.reverify_unchanged().unwrap();

    let wrong_leaf = root.join("inputs/supplier-statements/generic.json");
    crate::secure_fs::write_owner_only_new(&wrong_leaf, &bytes).unwrap();
    let error = RetainedPendingSupplierStatement::retain(
        &root,
        EvaluationCondition::Candidate,
        &wrong_leaf,
    )
    .err()
    .expect("wrong condition leaf must be rejected");
    assert!(error.to_string().contains("condition-bound supplier"), "{error:#}");

    let (_other_temp, other_root) = owner_root();
    let error = RetainedPendingSupplierStatement::retain(
        &other_root,
        EvaluationCondition::Candidate,
        &path,
    )
    .err()
    .expect("supplier retained from another root must be rejected");
    assert!(error.to_string().contains("condition-bound supplier"), "{error:#}");
}

#[test]
fn retained_pending_supplier_reverify_rejects_same_length_replacement() {
    let (_temp, root) = owner_root();
    let bytes = supplier_bytes();
    let path = supplier_leaf(&root, &bytes);
    let retained = RetainedPendingSupplierStatement::retain(
        &root,
        EvaluationCondition::Candidate,
        &path,
    )
    .unwrap();

    let replacement = vec![b' '; bytes.len()];
    fs::write(&path, replacement).unwrap();
    assert!(retained.reverify_unchanged().is_err());
}
