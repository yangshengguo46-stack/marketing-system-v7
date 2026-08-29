use std::fs;

use sha2::Digest;
use sha2::Sha256;

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
