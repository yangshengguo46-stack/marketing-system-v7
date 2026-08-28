use pretty_assertions::assert_eq;
use serde_json::Value;
use sha2::Digest;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;

const SIDECAR_KINDS: [(&str, &str); 7] = [
    ("notifications", "notifications.jsonl"),
    ("start", "start.json"),
    ("config", "config.json"),
    ("preCatalog", "pre-catalog.json"),
    ("postCatalog", "post-catalog.json"),
    ("quietTree", "quiet-tree.json"),
    ("brokerSnapshot", "broker-snapshot.json"),
];

struct ReplayArchiveRun {
    _temp: tempfile::TempDir,
    private_root: PathBuf,
    mission: codex_ai_ip_domain::HeldOutMissionCase,
    skill_bytes: Vec<u8>,
}

fn execute_existing_frozen_replay_pair() -> anyhow::Result<ReplayArchiveRun> {
    let fixture_set =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json")?;
    let fixture_root = fixture_set.parent().unwrap().to_path_buf();
    let temp = tempfile::tempdir()?;
    #[cfg(unix)]
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700))?;
    #[cfg(unix)]
    let private_root = temp.path().canonicalize()?;
    #[cfg(windows)]
    let private_root = {
        let root = temp.path().join("private");
        crate::secure_fs::create_owner_only_dir_new(&root)?;
        root.canonicalize()?
    };
    let codex_binary = private_root.join("synthetic-codex");
    crate::secure_fs::write_owner_only_new(&codex_binary, b"synthetic replay binary\n")?;
    let frozen = private_root.join("frozen-run-context.json");
    crate::freeze_replay_context(crate::model::ReplayFreezeArgs {
        repo_root: fixture_root.clone(),
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
    let mission = serde_json::from_slice(&fs::read(fixture_root.join("replay-case.json"))?)?;
    let skill_bytes = fs::read(fixture_root.join("replay-lead-skill.md"))?;
    Ok(ReplayArchiveRun {
        _temp: temp,
        private_root,
        mission,
        skill_bytes,
    })
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}

fn coordinator(root: &Path) -> PathBuf {
    root.join("replay-coordinator")
}

fn manifest_path(root: &Path, ordinal: u8) -> PathBuf {
    coordinator(root).join(format!("run-{ordinal}-manifest.json"))
}

fn index_path(root: &Path, ordinal: u8) -> PathBuf {
    coordinator(root).join(format!("run-{ordinal}-postprocess-index.json"))
}

fn raw_archive_is_bound(root: &Path, ordinal: u8) -> anyhow::Result<Value> {
    let manifest: Value = serde_json::from_slice(&fs::read(manifest_path(root, ordinal))?)?;
    let index_bytes = fs::read(index_path(root, ordinal))?;
    anyhow::ensure!(
        manifest["postprocessEvidenceIndexSha256"].as_str() == Some(&sha256(&index_bytes)),
        "manifest does not bind the raw postprocess index"
    );
    let index = crate::jcs::parse_json(&index_bytes)?;
    anyhow::ensure!(
        index_bytes == crate::jcs::canonicalize_value(&index)?,
        "postprocess index is not canonical"
    );
    let sidecars = index["sidecars"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("postprocess index has no sidecars"))?;
    anyhow::ensure!(sidecars.len() == SIDECAR_KINDS.len());
    for (entry, (kind, leaf)) in sidecars.iter().zip(SIDECAR_KINDS) {
        let expected = format!("replay-coordinator/run-{ordinal}-{leaf}");
        anyhow::ensure!(entry["kind"].as_str() == Some(kind));
        anyhow::ensure!(entry["relativePath"].as_str() == Some(expected.as_str()));
        let bytes = fs::read(root.join(&expected))?;
        let expected_sha256 = sha256(&bytes);
        anyhow::ensure!(entry["sha256"].as_str() == Some(expected_sha256.as_str()));
    }
    Ok(index)
}

fn semantic_mutation(leaf: &str, bytes: &[u8]) -> Vec<u8> {
    if leaf == "notifications.jsonl" {
        let mut output = Vec::new();
        let mut changed = false;
        for line in bytes
            .strip_suffix(b"\n")
            .unwrap()
            .split(|byte| *byte == b'\n')
        {
            let mut value: Value = serde_json::from_slice(line).unwrap();
            if !changed && value["method"] == "rawResponse/completed" {
                value["params"]["usage"]["totalTokens"] = Value::from(151);
                changed = true;
            }
            output.extend(serde_json::to_vec(&value).unwrap());
            output.push(b'\n');
        }
        assert!(changed);
        return output;
    }
    let mut value: Value = serde_json::from_slice(bytes).unwrap();
    match leaf {
        "start.json" => value["thread"]["model"] = Value::from("wrong-model"),
        "config.json" => value["canonicalConfigPath"] = Value::from("/wrong/config.toml"),
        "pre-catalog.json" | "post-catalog.json" | "quiet-tree.json" => {
            value["schemaVersion"] = Value::from(2)
        }
        "broker-snapshot.json" => value["globalAttemptStartInclusive"] = Value::from(100),
        other => panic!("unexpected mutation leaf {other}"),
    }
    serde_json::to_vec(&value).unwrap()
}

fn resign_sidecar(
    root: &Path,
    ordinal: u8,
    leaf: &str,
    sidecar_bytes: &[u8],
    manifest: &crate::RunManifest,
) {
    let path = coordinator(root).join(format!("run-{ordinal}-{leaf}"));
    fs::write(&path, sidecar_bytes).unwrap();
    let mut index: Value =
        crate::jcs::parse_json(&fs::read(index_path(root, ordinal)).unwrap()).unwrap();
    let relative = format!("replay-coordinator/run-{ordinal}-{leaf}");
    let entry = index["sidecars"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["relativePath"] == relative)
        .unwrap();
    entry["sha256"] = Value::from(sha256(sidecar_bytes));
    let index_bytes = crate::jcs::canonicalize_value(&index).unwrap();
    fs::write(index_path(root, ordinal), &index_bytes).unwrap();
    let mut manifest = manifest.clone();
    manifest.postprocess_evidence_index_sha256 = sha256(&index_bytes);
    fs::write(
        manifest_path(root, ordinal),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}

#[test]
fn replay_pair_seals_manifest_bound_postprocess_archives() {
    let run = execute_existing_frozen_replay_pair().unwrap();
    for ordinal in [1, 2] {
        let index = raw_archive_is_bound(&run.private_root, ordinal).unwrap();
        assert_eq!(index["schemaVersion"], 1);
        assert_eq!(index["runOrdinal"], ordinal);
        assert_eq!(index["evidenceSource"], "replaySynthetic");
        let typed: crate::ArmPostprocessIndex = serde_json::from_value(index).unwrap();
        let raw = fs::read(index_path(&run.private_root, ordinal)).unwrap();
        assert_eq!(
            raw,
            crate::jcs::canonicalize_value(&serde_json::to_value(typed).unwrap()).unwrap()
        );
        let manifest: crate::RunManifest =
            serde_json::from_slice(&fs::read(manifest_path(&run.private_root, ordinal)).unwrap())
                .unwrap();
        crate::verify_postprocess_archive(
            &run.private_root,
            &manifest,
            &run.mission,
            &run.skill_bytes,
        )
        .unwrap();
    }
}

#[test]
fn any_postprocess_archive_tamper_is_rejected() {
    let run = execute_existing_frozen_replay_pair().unwrap();
    raw_archive_is_bound(&run.private_root, 1).unwrap();
    let manifest: crate::RunManifest =
        serde_json::from_slice(&fs::read(manifest_path(&run.private_root, 1)).unwrap()).unwrap();
    let coordinator = coordinator(&run.private_root);
    for leaf in [
        "notifications.jsonl",
        "start.json",
        "config.json",
        "pre-catalog.json",
        "post-catalog.json",
        "quiet-tree.json",
        "broker-snapshot.json",
        "postprocess-index.json",
    ] {
        let path = coordinator.join(format!("run-1-{leaf}"));
        let original = fs::read(&path).unwrap();
        let mut mutated = original.clone();
        mutated.push(b' ');
        fs::write(&path, mutated).unwrap();
        assert!(
            crate::verify_postprocess_archive(
                &run.private_root,
                &manifest,
                &run.mission,
                &run.skill_bytes,
            )
            .is_err(),
            "tamper was accepted for {leaf}"
        );
        fs::write(path, original).unwrap();
    }

    for leaf in [
        "notifications.jsonl",
        "start.json",
        "config.json",
        "pre-catalog.json",
        "post-catalog.json",
        "quiet-tree.json",
        "broker-snapshot.json",
    ] {
        let sidecar_path = coordinator.join(format!("run-1-{leaf}"));
        let original_sidecar = fs::read(&sidecar_path).unwrap();
        let original_index = fs::read(index_path(&run.private_root, 1)).unwrap();
        let original_manifest = fs::read(manifest_path(&run.private_root, 1)).unwrap();
        let mutated = semantic_mutation(leaf, &original_sidecar);
        resign_sidecar(&run.private_root, 1, leaf, &mutated, &manifest);
        let resigned_manifest: crate::RunManifest =
            serde_json::from_slice(&fs::read(manifest_path(&run.private_root, 1)).unwrap())
                .unwrap();
        assert!(
            crate::verify_postprocess_archive(
                &run.private_root,
                &resigned_manifest,
                &run.mission,
                &run.skill_bytes,
            )
            .is_err(),
            "re-signed semantic mutation was accepted for {leaf}"
        );
        fs::write(sidecar_path, original_sidecar).unwrap();
        fs::write(index_path(&run.private_root, 1), original_index).unwrap();
        fs::write(manifest_path(&run.private_root, 1), original_manifest).unwrap();
    }

    let original_index = fs::read(index_path(&run.private_root, 1)).unwrap();
    let original_manifest = fs::read(manifest_path(&run.private_root, 1)).unwrap();
    let mut index: Value = crate::jcs::parse_json(&original_index).unwrap();
    index["pairId"] = Value::from("wrong-pair");
    let index_bytes = crate::jcs::canonicalize_value(&index).unwrap();
    fs::write(index_path(&run.private_root, 1), &index_bytes).unwrap();
    let mut resigned_manifest = manifest;
    resigned_manifest.postprocess_evidence_index_sha256 = sha256(&index_bytes);
    fs::write(
        manifest_path(&run.private_root, 1),
        serde_json::to_vec_pretty(&resigned_manifest).unwrap(),
    )
    .unwrap();
    assert!(
        crate::verify_postprocess_archive(
            &run.private_root,
            &resigned_manifest,
            &run.mission,
            &run.skill_bytes,
        )
        .is_err(),
        "re-signed semantic index mutation was accepted"
    );
    fs::write(index_path(&run.private_root, 1), original_index).unwrap();
    fs::write(manifest_path(&run.private_root, 1), original_manifest).unwrap();
}
