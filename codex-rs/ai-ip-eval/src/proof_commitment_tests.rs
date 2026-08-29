use std::collections::BTreeMap;
#[cfg(unix)]
use std::fs;
use std::path::PathBuf;

fn fixture() -> serde_json::Value {
    let resource = "tests/fixtures/contracts/06b1/proof-commitment-vectors.json";
    let bytes = std::fs::read(codex_utils_cargo_bin::find_resource!(resource).unwrap()).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn key_from_fixture(vector: &serde_json::Value) -> crate::proof_commitment::ProofCommitmentKey {
    let bytes = hex_bytes(vector["keyHex"].as_str().unwrap());
    crate::proof_commitment::ProofCommitmentKey::from_test_bytes(bytes.try_into().unwrap())
}

fn hex_bytes(input: &str) -> Vec<u8> {
    assert!(input.len() % 2 == 0);
    (0..input.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&input[index..index + 2], 16).unwrap())
        .collect()
}

#[test]
fn proof_commitment_vectors_match_normative_framing() {
    let vector = fixture();
    let key = key_from_fixture(&vector);
    for commitment in vector["commitments"].as_array().unwrap() {
        let value = commitment["value"].clone();
        let canonical = crate::jcs::canonicalize_value(&value).unwrap();
        assert_eq!(
            canonical,
            commitment["canonicalValue"].as_str().unwrap().as_bytes()
        );
        assert_eq!(
            crate::proof_commitment::proof_commitment(
                &key,
                commitment["label"].as_str().unwrap(),
                &value,
            )
            .unwrap(),
            commitment["expectedHex"].as_str().unwrap(),
        );
    }
    assert_eq!(
        crate::proof_commitment::derive_public_run_id(&key, vector["pairId"].as_str().unwrap())
            .unwrap(),
        vector["commitments"][0]["expectedHex"].as_str().unwrap(),
    );

    let merkle = &vector["merkle"];
    let single = merkle["singleLeaf"].as_object().unwrap();
    let single_leaves = BTreeMap::from([(
        single["name"].as_str().unwrap().to_owned(),
        single["value"].as_str().unwrap().to_owned(),
    )]);
    assert_eq!(
        crate::proof_commitment::proof_merkle_root(&single_leaves).unwrap(),
        single["expectedHex"].as_str().unwrap(),
    );
    let even_node = merkle["evenNode"].as_object().unwrap();
    assert_eq!(
        crate::proof_commitment::proof_merkle_root(&single_leaves).unwrap(),
        even_node["leftHex"].as_str().unwrap(),
    );
    let right_single_leaves = BTreeMap::from([(
        "é".to_owned(),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
    )]);
    assert_eq!(
        crate::proof_commitment::proof_merkle_root(&right_single_leaves).unwrap(),
        even_node["rightHex"].as_str().unwrap(),
    );
    let even_leaves = BTreeMap::from([
        ("z".to_owned(), single["value"].as_str().unwrap().to_owned()),
        (
            "é".to_owned(),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
        ),
    ]);
    assert_eq!(
        crate::proof_commitment::proof_merkle_root(&even_leaves).unwrap(),
        even_node["expectedHex"].as_str().unwrap(),
    );
    let odd_leaves = merkle["oddDuplicationLeaves"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, value)| (name.clone(), value.as_str().unwrap().to_owned()))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        crate::proof_commitment::proof_merkle_root(&odd_leaves).unwrap(),
        merkle["oddDuplicationRootHex"].as_str().unwrap(),
    );
    let full_leaves = merkle["fullLeaves"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, value)| (name.clone(), value.as_str().unwrap().to_owned()))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        crate::proof_commitment::proof_merkle_root(&full_leaves).unwrap(),
        merkle["fullRootHex"].as_str().unwrap(),
    );
}

#[test]
fn proof_commitment_rejects_invalid_public_inputs_and_preserves_length_prefixes() {
    let vector = fixture();
    let key = key_from_fixture(&vector);
    assert!(crate::proof_commitment::derive_public_run_id(&key, &"A".repeat(64)).is_err());
    assert!(crate::proof_commitment::derive_public_run_id(&key, "a").is_err());
    assert!(crate::proof_commitment::proof_merkle_root(&BTreeMap::new()).is_err());
    assert!(
        crate::proof_commitment::proof_merkle_root(&BTreeMap::from([(
            "leaf".to_owned(),
            "A".repeat(64),
        )]))
        .is_err()
    );
    assert!(
        crate::proof_commitment::proof_merkle_root(&BTreeMap::from([(
            "proofRootSha256".to_owned(),
            "a".repeat(64),
        )]))
        .is_err()
    );

    let long_label = "λ".repeat(32_768);
    let long_value = serde_json::Value::String("x".repeat(65_536));
    assert!(crate::proof_commitment::proof_commitment(&key, &long_label, &long_value).is_ok());
}

fn private_root() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("private");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    }
    #[cfg(windows)]
    crate::secure_fs::create_owner_only_dir_new(&root).unwrap();
    (temp, root.canonicalize().unwrap())
}

#[test]
fn proof_commitment_key_uses_create_new_owner_only_exact_single_link_files() {
    let vector = fixture();
    let expected = vector["commitments"][0]["expectedHex"].as_str().unwrap();
    let (_temp, root) = private_root();
    let path = root.join("commitment-key.bin");
    let key = key_from_fixture(&vector);
    key.publish_owner_only_new(&path).unwrap();
    assert!(key.publish_owner_only_new(&path).is_err());
    let read = crate::proof_commitment::ProofCommitmentKey::read_exact_owner_only(&path).unwrap();
    assert_eq!(
        crate::proof_commitment::derive_public_run_id(&read, vector["pairId"].as_str().unwrap())
            .unwrap(),
        expected,
    );

    let generated = crate::proof_commitment::ProofCommitmentKey::generate().unwrap();
    let generated_path = root.join("generated-key.bin");
    generated.publish_owner_only_new(&generated_path).unwrap();
    crate::proof_commitment::ProofCommitmentKey::read_exact_owner_only(&generated_path).unwrap();

    let short_path = root.join("short-key.bin");
    crate::secure_fs::write_owner_only_new(&short_path, &[0_u8; 31]).unwrap();
    assert!(
        crate::proof_commitment::ProofCommitmentKey::read_exact_owner_only(&short_path).is_err()
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(crate::proof_commitment::ProofCommitmentKey::read_exact_owner_only(&path).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        let hardlink = root.join("hardlink-key.bin");
        fs::hard_link(&path, &hardlink).unwrap();
        assert!(crate::proof_commitment::ProofCommitmentKey::read_exact_owner_only(&path).is_err());
        fs::remove_file(&hardlink).unwrap();

        let symlink = root.join("symlink-key.bin");
        std::os::unix::fs::symlink(&path, &symlink).unwrap();
        assert!(
            crate::proof_commitment::ProofCommitmentKey::read_exact_owner_only(&symlink).is_err()
        );
    }
}
