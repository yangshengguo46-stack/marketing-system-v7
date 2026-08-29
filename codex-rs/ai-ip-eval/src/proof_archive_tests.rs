use pretty_assertions::assert_eq;
use serde_json::Value;
use sha2::Digest;
use std::collections::HashSet;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

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

struct NativeArchiveRun {
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

fn execute_native_archive_pair() -> anyhow::Result<NativeArchiveRun> {
    use serde_json::json;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;

    let temp = tempfile::tempdir()?;
    #[cfg(unix)]
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700))?;
    let repository = temp.path().join("repo");
    fs::create_dir(&repository)?;
    fs::write(repository.join("source"), b"committed source\n")?;
    fs::create_dir_all(repository.join("codex-rs/responses-api-proxy/src"))?;
    fs::write(
        repository.join("codex-rs/responses-api-proxy/src/broker.rs"),
        b"// committed synthetic broker\n",
    )?;
    let git = |args: &[&str]| -> anyhow::Result<Vec<u8>> {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&repository)
            .args(args)
            .output()?;
        anyhow::ensure!(output.status.success(), "synthetic git command failed");
        Ok(output.stdout)
    };
    git(&["init", "--quiet"])?;
    git(&["add", "."])?;
    git(&[
        "-c",
        "user.name=Synthetic",
        "-c",
        "user.email=synthetic@example.invalid",
        "commit",
        "--quiet",
        "-m",
        "native archive",
    ])?;
    let head = String::from_utf8(git(&["rev-parse", "HEAD"])?)?
        .trim()
        .to_string();

    let inputs = temp.path().join("inputs");
    fs::create_dir(&inputs)?;
    let private_root = temp.path().join("private");
    fs::create_dir(&private_root)?;
    #[cfg(unix)]
    fs::set_permissions(&private_root, fs::Permissions::from_mode(0o700))?;
    let private_root = private_root.canonicalize()?;
    let retained_inputs = private_root.join("inputs");
    fs::create_dir(&retained_inputs)?;
    #[cfg(unix)]
    fs::set_permissions(&retained_inputs, fs::Permissions::from_mode(0o700))?;
    let mission: codex_ai_ip_domain::HeldOutMissionCase = serde_json::from_value(json!({
        "caseId": "native-archive-case",
        "objective": "Produce one native archive pair",
        "subjectKind": "brand",
        "constraints": [],
        "materials": []
    }))?;
    let case_bytes = serde_json::to_vec_pretty(&mission)?;
    let materials_bytes = serde_json::to_vec(&mission.materials)?;
    let case = inputs.join("case.json");
    let materials = inputs.join("materials");
    fs::write(&case, &case_bytes)?;
    fs::create_dir(&materials)?;

    let named_input = |name: &str, bytes: &[u8]| -> anyhow::Result<PathBuf> {
        let path = inputs.join(name);
        fs::write(&path, bytes)?;
        #[cfg(unix)]
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        Ok(path)
    };
    let retained_cost_input = |leaf: &str, fixture: &str| -> anyhow::Result<PathBuf> {
        let resource = format!("tests/fixtures/contracts/06b1/{fixture}");
        let path = retained_inputs.join(leaf);
        fs::copy(codex_utils_cargo_bin::find_resource!(resource)?, &path)?;
        #[cfg(unix)]
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        Ok(path)
    };
    let provider_budget = retained_cost_input(
        "provider-budget-evidence.json",
        "provider-budget-evidence.canonical.json",
    )?;
    let rate_card = retained_cost_input("rate-card.json", "provider-rate-card.canonical.json")?;
    let billing_policy =
        retained_cost_input("billing-policy.json", "billing-policy.canonical.json")?;
    let fx_policy = retained_cost_input("fx-policy.json", "fx-policy.canonical.json")?;
    let skill = named_input("lead-skill.md", b"synthetic skill\n")?;
    let mut attestation: Value =
        serde_json::from_slice(&fs::read(codex_utils_cargo_bin::find_resource!(
            "tests/fixtures/contracts/06a/canonical-native-attestation.json"
        )?)?)?;
    attestation["candidateSha"] = json!(head);
    attestation["candidateFrozenAt"] = json!("2026-08-27T06:00:00Z");
    attestation["caseSelectedAt"] = json!("2026-08-27T07:00:00Z");
    attestation["caseSha256"] = json!(sha256(&case_bytes));
    attestation["sourceMaterialsSha256"] = json!(sha256(&materials_bytes));
    attestation["privateRoot"] = json!(private_root.canonicalize()?);
    attestation["providerRole"] = json!("approvedReference");
    attestation["approvedTotalFen"] = json!(0);
    attestation["approvedPerRunFen"] = json!(0);
    attestation["maxProviderRequestAttemptsPerRun"] = json!(2);
    attestation["maxTotalTokensPerRun"] = json!(10);
    attestation["maxElapsedSecondsPerRun"] = json!(180);
    attestation["maxOutputTokensPerRequest"] = json!(17);
    attestation["signedAt"] = json!("2026-08-27T08:00:00Z");
    attestation["retentionDeadline"] = json!("2099-09-04T08:00:00Z");
    attestation["rateEffectiveAt"] = json!("2026-08-27T00:00:00Z");
    for (field, path) in [
        ("providerBudgetEvidenceSha256", &provider_budget),
        ("rateCardSha256", &rate_card),
        ("billingPolicyCommitment", &billing_policy),
        ("fxPolicySha256", &fx_policy),
    ] {
        attestation[field] = json!(sha256(&fs::read(path)?));
    }
    for (index, reviewer) in attestation["reviewers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        reviewer["declaredAt"] = json!(format!("2026-08-27T07:2{index}:00Z"));
        let mut payload = reviewer.clone();
        payload
            .as_object_mut()
            .unwrap()
            .remove("signedPayloadSha256");
        payload
            .as_object_mut()
            .unwrap()
            .remove("signatureEvidenceSha256");
        reviewer["signedPayloadSha256"] = json!(
            crate::jcs::commitment(
                b"AI-IP-REVIEWER-QUALIFICATION-V1\0",
                &serde_json::to_vec(&payload)?,
            )?
            .sha256
        );
    }
    let attestation_path = retained_inputs.join("held-out-attestation.json");
    fs::write(&attestation_path, serde_json::to_vec_pretty(&attestation)?)?;
    #[cfg(unix)]
    fs::set_permissions(&attestation_path, fs::Permissions::from_mode(0o600))?;

    let mock_codex = crate::native_app_server_fixture::copy_into(temp.path())?;
    let upstream = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let upstream_addr = upstream.server_addr().to_ip().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let worker = std::thread::spawn(move || {
        let mut index = 0_u64;
        while !worker_stop.load(Ordering::SeqCst) {
            let Some(request) = upstream.recv_timeout(Duration::from_millis(100)).unwrap() else {
                continue;
            };
            let body = format!(
                "data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"archive-response-{index}\",\"model\":\"mock-revision\",\"usage\":{{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}}}}\n\n"
            );
            request
                .respond(tiny_http::Response::from_string(body).with_header(
                    tiny_http::Header::from_bytes("content-type", "text/event-stream").unwrap(),
                ))
                .unwrap();
            index += 1;
        }
    });
    let frozen = private_root.join("frozen-run-context.json");
    crate::freeze_live_context(crate::model::LiveFreezeArgs {
        repo_root: repository.clone(),
        evidence_repo_root: repository,
        fork_sha: head,
        private_root: private_root.clone(),
        codex_bin: mock_codex,
        case,
        material_root: materials,
        attestation: attestation_path,
        provider_budget_evidence: provider_budget,
        rate_card,
        billing_policy,
        fx_policy,
        lead_skill: skill.clone(),
        model_label: "local-mock".to_string(),
        provider_label: "local-mock".to_string(),
        provider_role: crate::ProviderRole::ApprovedReference,
        provider_upstream_url: format!("http://{upstream_addr}/v1/responses"),
        authorized_total_cost_fen: 0,
        authorized_per_run_cost_fen: 0,
        max_provider_request_attempts_per_run: 2,
        max_total_tokens_per_run: 10,
        max_elapsed_seconds_per_run: 180,
        max_output_tokens_per_request: 17,
        output: frozen.clone(),
    })?;
    let result = crate::run_local_mock_pair(&frozen);
    stop.store(true, Ordering::SeqCst);
    worker.join().unwrap();
    result?;
    Ok(NativeArchiveRun {
        _temp: temp,
        private_root,
        mission,
        skill_bytes: fs::read(skill)?,
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

fn coordinator_for(root: &Path, mode: crate::ExecutionMode) -> PathBuf {
    root.join(if mode == crate::ExecutionMode::Replay {
        "replay-coordinator"
    } else {
        "coordinator"
    })
}

fn manifest_path_for(root: &Path, mode: crate::ExecutionMode, ordinal: u8) -> PathBuf {
    coordinator_for(root, mode).join(format!("run-{ordinal}-manifest.json"))
}

fn retire_run_one_archive(
    root: &Path,
    mode: crate::ExecutionMode,
) -> anyhow::Result<tempfile::TempDir> {
    let retired = tempfile::tempdir()?;
    let coordinator = coordinator_for(root, mode);
    for leaf in std::iter::once("manifest.json")
        .chain(std::iter::once("postprocess-index.json"))
        .chain(SIDECAR_KINDS.map(|(_, leaf)| leaf))
    {
        fs::rename(
            coordinator.join(format!("run-1-{leaf}")),
            retired.path().join(leaf),
        )?;
    }
    Ok(retired)
}

fn verify_sequential_run_two_after_retiring_run_one(
    root: &Path,
    mode: crate::ExecutionMode,
    mission: &codex_ai_ip_domain::HeldOutMissionCase,
    skill_bytes: &[u8],
) -> anyhow::Result<(
    crate::proof_archive::VerifiedPostprocessSummary,
    crate::proof_archive::VerifiedPostprocessSummary,
)> {
    let run_one: crate::RunManifest =
        serde_json::from_slice(&fs::read(manifest_path_for(root, mode, 1))?)?;
    let run_two: crate::RunManifest =
        serde_json::from_slice(&fs::read(manifest_path_for(root, mode, 2))?)?;
    let first = crate::proof_archive::verify_postprocess_archive_summary(
        root,
        &run_one,
        mission,
        skill_bytes,
    )?;
    let expected_first_end = run_one.provider_request_attempt_count;
    assert_eq!(first.broker_global_attempt_start_inclusive, 0);
    assert_eq!(
        first.broker_global_attempt_end_exclusive,
        expected_first_end
    );
    let _retired = retire_run_one_archive(root, mode)?;
    let second = crate::proof_archive::verify_sequential_postprocess_archive_summary(
        root,
        &run_two,
        mission,
        skill_bytes,
        &first,
    )?;
    let expected_second_start = if mode == crate::ExecutionMode::Replay {
        0
    } else {
        expected_first_end
    };
    assert_eq!(
        second.broker_global_attempt_start_inclusive,
        expected_second_start
    );
    assert_eq!(
        second.broker_global_attempt_end_exclusive,
        expected_second_start + run_two.provider_request_attempt_count
    );
    Ok((first, second))
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
fn postprocess_summary_returns_the_complete_same_pass_recomputation() {
    let run = execute_existing_frozen_replay_pair().unwrap();
    let manifest: crate::RunManifest =
        serde_json::from_slice(&fs::read(manifest_path(&run.private_root, 1)).unwrap()).unwrap();
    let index: crate::ArmPostprocessIndex =
        serde_json::from_value(raw_archive_is_bound(&run.private_root, 1).unwrap()).unwrap();
    let notifications =
        fs::read(coordinator(&run.private_root).join("run-1-notifications.jsonl")).unwrap();
    let mut collector = crate::ReplayCollector::new(
        manifest.root_thread_id.clone(),
        manifest.root_turn_id.clone(),
        HashSet::from([manifest.root_thread_id.clone()]),
    );
    for notification in crate::evidence::parse_notification_archive(&notifications).unwrap() {
        collector.ingest(notification).unwrap();
    }
    let collected = collector.finish(&run.mission).unwrap();
    let expected = crate::proof_archive::VerifiedPostprocessSummary {
        index,
        content_package_bytes: serde_json::to_vec(&collected.content_package).unwrap(),
        content_package: collected.content_package,
        usage: collected.usage,
        raw_response_count: collected.raw_response_count,
        tree_closed: true,
        skill_use: crate::SkillUseOutcome {
            successful_read_observed: false,
            evidence_sha256: None,
        },
        normalized_base_catalog_sha256: manifest.normalized_base_catalog_sha256.clone(),
        broker_global_attempt_start_inclusive: 0,
        broker_global_attempt_end_exclusive: 0,
    };
    assert_eq!(
        crate::proof_archive::verify_postprocess_archive_summary(
            &run.private_root,
            &manifest,
            &run.mission,
            &run.skill_bytes,
        )
        .unwrap(),
        expected
    );
}

#[test]
fn sequential_replay_archive_summary_never_reopens_run_one() {
    let run = execute_existing_frozen_replay_pair().unwrap();
    verify_sequential_run_two_after_retiring_run_one(
        &run.private_root,
        crate::ExecutionMode::Replay,
        &run.mission,
        &run.skill_bytes,
    )
    .unwrap();
}

#[test]
fn sequential_native_archive_summary_never_reopens_run_one() {
    let run = execute_native_archive_pair().unwrap();
    verify_sequential_run_two_after_retiring_run_one(
        &run.private_root,
        crate::ExecutionMode::Mock,
        &run.mission,
        &run.skill_bytes,
    )
    .unwrap();
}

#[test]
fn sequential_archive_summary_rejects_untrusted_prior_identity_and_ranges() {
    let run = execute_existing_frozen_replay_pair().unwrap();
    let run_one: crate::RunManifest = serde_json::from_slice(
        &fs::read(manifest_path_for(
            &run.private_root,
            crate::ExecutionMode::Replay,
            1,
        ))
        .unwrap(),
    )
    .unwrap();
    let run_two: crate::RunManifest = serde_json::from_slice(
        &fs::read(manifest_path_for(
            &run.private_root,
            crate::ExecutionMode::Replay,
            2,
        ))
        .unwrap(),
    )
    .unwrap();
    let baseline = crate::proof_archive::verify_postprocess_archive_summary(
        &run.private_root,
        &run_one,
        &run.mission,
        &run.skill_bytes,
    )
    .unwrap();
    for mutation in ["pair", "ordinal", "condition", "source", "start", "end"] {
        let mut prior = baseline.clone();
        match mutation {
            "pair" => prior.index.pair_id = "wrong-pair".to_string(),
            "ordinal" => prior.index.run_ordinal = 2,
            "condition" => prior.index.condition = run_two.condition,
            "source" => prior.index.evidence_source = crate::ArchiveEvidenceSource::NativeRecorded,
            "start" => prior.broker_global_attempt_start_inclusive = 1,
            "end" => prior.broker_global_attempt_end_exclusive = 1,
            _ => unreachable!(),
        }
        let error = crate::proof_archive::verify_sequential_postprocess_archive_summary(
            &run.private_root,
            &run_two,
            &run.mission,
            &run.skill_bytes,
            &prior,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "prior summary identity does not authorize the sequential attempt range",
            "wrong rejection for {mutation}"
        );
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
