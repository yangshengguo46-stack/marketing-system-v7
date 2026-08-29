use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;

use anyhow::Context;
use anyhow::Result;
use chrono::SecondsFormat;
use codex_utils_cargo_bin::cargo_bin;
use pretty_assertions::assert_eq;
use rand::SeedableRng;
use rand::seq::SliceRandom;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use tempfile::TempDir;

#[test]
fn blind_cli_accepts_authoritative_replay_surface() -> Result<()> {
    let binary = cargo_bin("codex-ai-ip-eval")?;
    let fixture_set =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json")?;
    let fixture_root = fixture_set.parent().unwrap().canonicalize()?;
    let temp = TempDir::new()?;
    let private_root = temp.path().join("private");
    fs::create_dir(&private_root)?;
    #[cfg(unix)]
    fs::set_permissions(&private_root, fs::Permissions::from_mode(/*mode*/ 0o700))?;
    let private_root = private_root.canonicalize()?;
    let frozen_context = run_replay_pair(&binary, &fixture_root, &fixture_set, &private_root)?;
    assert!(
        private_root
            .join("replay-coordinator/replay-pair-verification.json")
            .is_file()
    );

    let conflict = blind_command(&binary, &frozen_context)
        .args(["--seed-dir", "coordinator/blind-seeds"])
        .output()?;
    assert!(!conflict.status.success());
    assert_eq!(
        String::from_utf8(conflict.stderr)?.trim_end(),
        "Error: Replay blind-pack requires exactly three distinct nonempty replay seeds and no seed directory"
    );
    assert_no_outputs(&private_root);

    let output = blind_command(&binary, &frozen_context).output()?;
    assert_success("blind-pack", &output);
    assert_replay_outputs(&private_root, &fixture_root, &frozen_context)?;

    let before_rerun = tree_snapshot(&private_root)?;
    let rerun = blind_command(&binary, &frozen_context).output()?;
    assert!(!rerun.status.success());
    assert!(String::from_utf8(rerun.stderr)?.contains("blind-pack output already exists"));
    assert_eq!(tree_snapshot(&private_root)?, before_rerun);

    let second = TempDir::new()?;
    let second_root = second.path().join("private");
    fs::create_dir(&second_root)?;
    #[cfg(unix)]
    fs::set_permissions(&second_root, fs::Permissions::from_mode(0o700))?;
    let second_root = second_root.canonicalize()?;
    let second_context = run_replay_pair(&binary, &fixture_root, &fixture_set, &second_root)?;
    let second_output = blind_command(&binary, &second_context).output()?;
    assert_success("second blind-pack", &second_output);
    assert_eq!(
        deterministic_outputs(&private_root)?,
        deterministic_outputs(&second_root)?
    );
    for ordinal in 1..=3 {
        fs::copy(
            fixture_root.join(format!("replay-review-{ordinal}.json")),
            second_root
                .join("reviews")
                .join(format!("reviewer-{ordinal}.json")),
        )?;
    }
    fs::remove_file(second_root.join("coordinator/blind-pack-receipt.json"))?;
    let before_missing_receipt = tree_snapshot(&second_root)?;
    let missing_receipt = score_command(&binary, &second_root, &second_context).output()?;
    assert!(!missing_receipt.status.success());
    assert!(missing_receipt.stdout.is_empty());
    assert!(String::from_utf8(missing_receipt.stderr)?.contains("blind-pack receipt"));
    assert_path_absent(&second_root, "coordinator/.decision.private.json.staging");
    assert_path_absent(&second_root, "coordinator/decision.private.json");
    assert_eq!(tree_snapshot(&second_root)?, before_missing_receipt);
    Ok(())
}

#[test]
fn score_cli_publishes_immutable_pass_for_verified_replay_reviews() -> Result<()> {
    let binary = cargo_bin("codex-ai-ip-eval")?;
    let fixture_set =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json")?;
    let fixture_root = fixture_set.parent().unwrap().canonicalize()?;
    let temp = TempDir::new()?;
    let private_root = temp.path().join("private");
    fs::create_dir(&private_root)?;
    #[cfg(unix)]
    fs::set_permissions(&private_root, fs::Permissions::from_mode(/*mode*/ 0o700))?;
    let private_root = private_root.canonicalize()?;
    let frozen_context = run_replay_pair(&binary, &fixture_root, &fixture_set, &private_root)?;
    let blind = blind_command(&binary, &frozen_context).output()?;
    assert_success("blind-pack before score", &blind);
    for ordinal in 1..=3 {
        fs::copy(
            fixture_root.join(format!("replay-review-{ordinal}.json")),
            private_root
                .join("reviews")
                .join(format!("reviewer-{ordinal}.json")),
        )?;
    }
    let before_score = tree_snapshot(&private_root)?;
    let snapshot_file = |relative: &str| -> Result<Vec<u8>> {
        before_score
            .get(Path::new(relative))
            .and_then(Option::as_ref)
            .cloned()
            .with_context(|| format!("missing pre-score file {relative}"))
    };
    let reviewer_ids = ["reviewer-1", "reviewer-2", "reviewer-3"];
    let mapping_entries = reviewer_ids
        .map(|reviewer_id| {
            Ok((
                reviewer_id.to_string(),
                snapshot_file(&format!("coordinator/mappings/{reviewer_id}.json"))?,
            ))
        })
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    let review_entries = reviewer_ids
        .map(|reviewer_id| {
            Ok((
                reviewer_id.to_string(),
                snapshot_file(&format!("reviews/{reviewer_id}.json"))?,
            ))
        })
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    let inventory_before = snapshot_file("coordinator/private-inventory.jsonl")?;
    let receipt_bytes = snapshot_file("coordinator/blind-pack-receipt.json")?;
    let frozen_context_bytes = snapshot_file("frozen-run-context.json")?;
    let receipt: Value = serde_json::from_slice(&receipt_bytes)?;
    let attestation: Value =
        serde_json::from_slice(&fs::read(fixture_root.join("replay-attestation.json"))?)?;
    let rubric = fs::read(codex_utils_cargo_bin::find_resource!(
        "../../ai-ip-evals/rubrics/content-package-blind-review.json"
    )?)?;
    let policy = fs::read(codex_utils_cargo_bin::find_resource!(
        "../../ai-ip-evals/rubrics/blind-review-decision-policy.json"
    )?)?;

    let score = score_command(&binary, &private_root, &frozen_context).output()?;
    assert_success("score", &score);
    assert!(score.stdout.is_empty());
    assert!(score.stderr.is_empty());
    assert_path_absent(&private_root, "coordinator/.decision.private.json.staging");
    let decision_path = private_root.join("coordinator/decision.private.json");
    let decision_bytes = fs::read(&decision_path)?;
    assert_canonical(&decision_bytes)?;
    let decision: Value = serde_json::from_slice(&decision_bytes)?;
    let generated = chrono::DateTime::parse_from_rfc3339(
        decision["generatedAt"]
            .as_str()
            .context("decision generatedAt")?,
    )?;
    assert_eq!(
        generated
            .to_utc()
            .to_rfc3339_opts(SecondsFormat::Millis, true),
        decision["generatedAt"]
    );
    assert_eq!(
        decision,
        serde_json::json!({
            "schemaVersion": 1,
            "pairId": attestation["pairId"],
            "frozenRunContextSha256": sha256(&frozen_context_bytes),
            "blindPackReceiptSha256": sha256(&receipt_bytes),
            "reviewerMappingsSha256": raw_set_commitment(
                b"AI-IP-BLIND-MAPPING-SET-V1\0", &mapping_entries)?,
            "reviewSubmissionsSha256": raw_set_commitment(
                b"AI-IP-BLIND-REVIEW-SET-V1\0", &review_entries)?,
            "rubricSha256": jcs_sha(&rubric)?,
            "decisionPolicySha256": jcs_sha(&policy)?,
            "decision": "PASS",
            "metrics": {
                "reviewerCount": 3,
                "experiencedOperatorOrDirectorCount": 2,
                "candidatePreferenceCount": 2,
                "candidateReadyForHumanReviewCount": 2,
                "genericTotals": [15, 15, 15],
                "candidateTotals": [18, 18, 18],
                "pairedDeltas": [3, 3, 3],
                "medianGenericTotal": 15,
                "medianCandidateTotal": 18,
                "medianPairedDelta": 3,
                "candidateSevereFailureCount": 0,
                "candidateSevereFlags": {
                    "fabricatedFactualClaim": 0,
                    "wrongSubjectOrDesiredAction": 0,
                    "notActuallyUsable": 0,
                    "rightsOrPrivacyViolation": 0,
                },
            },
            "validationFailures": [],
            "generatedAt": generated.to_utc().to_rfc3339_opts(SecondsFormat::Millis, true),
        })
    );
    assert_eq!(receipt["pairId"], attestation["pairId"]);
    #[cfg(unix)]
    assert_eq!(
        fs::metadata(&decision_path)?.permissions().mode() & /*mask*/ 0o777,
        /*owner_read_write*/ 0o600
    );
    let decision_text = String::from_utf8(decision_bytes.clone())?;
    for forbidden in [
        "reviewer-1",
        "preferredArm",
        "qualificationClass",
        "signatureEvidence",
        "synthetic review evidence",
        "private review reason",
        "seedCommitment",
        "coordinator/mappings",
    ] {
        assert!(
            !decision_text.contains(forbidden),
            "decision leak: {forbidden}"
        );
    }
    let inventory_after = fs::read(private_root.join("coordinator/private-inventory.jsonl"))?;
    assert!(inventory_after.starts_with(&inventory_before));
    let appended_lines = inventory_after[inventory_before.len()..]
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let prefix_lines = inventory_before
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let expected_files = [
        (
            "coordinator/decision.private.json",
            decision_bytes.as_slice(),
        ),
        ("reviews/reviewer-1.json", review_entries[0].1.as_slice()),
        ("reviews/reviewer-2.json", review_entries[1].1.as_slice()),
        ("reviews/reviewer-3.json", review_entries[2].1.as_slice()),
    ];
    assert_eq!(appended_lines.len(), expected_files.len());
    let mut previous = prefix_lines.last().map(|line| sha256(line));
    for (index, (line, (relative_path, bytes))) in
        appended_lines.iter().zip(expected_files).enumerate()
    {
        assert_canonical(line)?;
        assert_eq!(
            serde_json::from_slice::<Value>(line)?,
            serde_json::json!({
                "schemaVersion": 1,
                "sequence": prefix_lines.len() + index + 1,
                "relativePath": relative_path,
                "kind": "file",
                "sha256": sha256(bytes),
                "previousRecordSha256": previous,
            })
        );
        previous = Some(sha256(line));
    }
    let mut expected_after_score = before_score;
    expected_after_score.insert(
        PathBuf::from("coordinator/decision.private.json"),
        Some(decision_bytes),
    );
    expected_after_score.insert(
        PathBuf::from("coordinator/private-inventory.jsonl"),
        Some(inventory_after),
    );
    assert_eq!(tree_snapshot(&private_root)?, expected_after_score);

    let before_rerun = tree_snapshot(&private_root)?;
    let rerun = score_command(&binary, &private_root, &frozen_context).output()?;
    assert!(!rerun.status.success());
    assert!(rerun.stdout.is_empty());
    assert_eq!(rerun.stderr, b"Error: score final output already exists\n");
    assert_eq!(tree_snapshot(&private_root)?, before_rerun);
    assert_path_absent(&private_root, "coordinator/.decision.private.json.staging");
    Ok(())
}

fn run_replay_pair(
    binary: &Path,
    fixture_root: &Path,
    fixture_set: &Path,
    private_root: &Path,
) -> Result<PathBuf> {
    let frozen_context = private_root.join("frozen-run-context.json");
    let freeze = Command::new(binary)
        .args(["freeze-run-context", "replay", "--repo-root"])
        .arg(fixture_root)
        .args(["--fork-sha", "synthetic-replay-fork", "--private-root"])
        .arg(private_root)
        .arg("--codex-bin")
        .arg(binary)
        .arg("--case")
        .arg(fixture_root.join("replay-case.json"))
        .arg("--transcript")
        .arg(fixture_root.join("replay-transcript.jsonl"))
        .arg("--fixture-set-manifest")
        .arg(fixture_set)
        .arg("--output")
        .arg(&frozen_context)
        .output()?;
    assert_success("freeze-run-context replay", &freeze);
    let replay = Command::new(binary)
        .args(["replay-pair", "--frozen-run-context"])
        .arg(&frozen_context)
        .output()?;
    assert_success("replay-pair", &replay);
    Ok(frozen_context)
}

fn deterministic_outputs(root: &Path) -> Result<BTreeMap<PathBuf, Option<Vec<u8>>>> {
    Ok(tree_snapshot(root)?
        .into_iter()
        .filter(|(path, _)| {
            path.starts_with("reviewer") || path.starts_with("coordinator/mappings")
        })
        .collect())
}

fn assert_replay_outputs(root: &Path, fixture_root: &Path, frozen_context: &Path) -> Result<()> {
    let reviewer_ids = ["reviewer-1", "reviewer-2", "reviewer-3"];
    assert_eq!(
        fs::read_dir(root.join("reviewer"))?
            .map(|entry| entry.unwrap().file_name())
            .collect::<BTreeSet<_>>(),
        reviewer_ids.map(Into::into).into_iter().collect()
    );
    assert!(fs::read_dir(root.join("reviews"))?.next().is_none());
    assert!(!root.join("coordinator/blind-seeds").exists());
    assert_eq!(
        fs::read_dir(root.join("coordinator/mappings"))?
            .map(|entry| entry.unwrap().file_name())
            .collect::<BTreeSet<_>>(),
        reviewer_ids
            .map(|id| format!("{id}.json").into())
            .into_iter()
            .collect()
    );

    let case_bytes = fs::read(fixture_root.join("replay-case.json"))?;
    let rubric_bytes = fs::read(codex_utils_cargo_bin::find_resource!(
        "../../ai-ip-evals/rubrics/content-package-blind-review.json"
    )?)?;
    let schema_bytes = fs::read(codex_utils_cargo_bin::find_resource!(
        "../../ai-ip-evals/rubrics/reviewer-submission.schema.json"
    )?)?;
    let policy_bytes = fs::read(codex_utils_cargo_bin::find_resource!(
        "../../ai-ip-evals/rubrics/blind-review-decision-policy.json"
    )?)?;
    let attestation: Value =
        serde_json::from_slice(&fs::read(fixture_root.join("replay-attestation.json"))?)?;
    let packages = [
        transcript_package(&fixture_root.join("replay-transcript.jsonl"))?,
        transcript_package(&fixture_root.join("replay-candidate-transcript.jsonl"))?,
    ];
    let mut receipt_mappings = Vec::new();
    let mut sensitive = vec![
        root.to_string_lossy().to_ascii_lowercase(),
        frozen_context.to_string_lossy().to_ascii_lowercase(),
        jcs_sha(&policy_bytes)?,
        "coordinator/mappings".into(),
        "coordinator/blind-seeds".into(),
        "$codex_home".into(),
        "skill.md".into(),
        "lead-skill-read".into(),
        "root-thread".into(),
        "root-turn".into(),
        "resp-1".into(),
    ];
    for ((reviewer_id, seed_text), declaration) in reviewer_ids
        .into_iter()
        .zip(["one", "two", "three"])
        .zip(attestation["reviewers"].as_array().unwrap())
    {
        let reviewer_root = root.join("reviewer").join(reviewer_id);
        assert_eq!(
            tree_snapshot(&reviewer_root)?
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>(),
            [
                "A.json",
                "B.json",
                "case.json",
                "materials",
                "materials-manifest.json",
                "review-bundle.json",
                "reviewer-submission.schema.json",
                "rubric.json",
            ]
            .map(PathBuf::from)
            .into_iter()
            .collect()
        );
        assert_eq!(fs::read(reviewer_root.join("case.json"))?, case_bytes);
        assert_eq!(
            fs::read(reviewer_root.join("materials-manifest.json"))?,
            b"[]"
        );
        assert_eq!(fs::read(reviewer_root.join("rubric.json"))?, rubric_bytes);
        assert_eq!(
            fs::read(reviewer_root.join("reviewer-submission.schema.json"))?,
            schema_bytes
        );

        let mapping_bytes = fs::read(
            root.join("coordinator/mappings")
                .join(format!("{reviewer_id}.json")),
        )?;
        assert_canonical(&mapping_bytes)?;
        let mapping: Value = serde_json::from_slice(&mapping_bytes)?;
        let seed = replay_seed(seed_text);
        let mut orientation = ["generic", "candidate"];
        orientation.shuffle(&mut rand::rngs::StdRng::from_seed(seed));
        let expected_a = &packages[usize::from(orientation[0] == "candidate")];
        let expected_b = &packages[usize::from(orientation[1] == "candidate")];
        assert_eq!(fs::read(reviewer_root.join("A.json"))?, *expected_a);
        assert_eq!(fs::read(reviewer_root.join("B.json"))?, *expected_b);

        let bundle_bytes = fs::read(reviewer_root.join("review-bundle.json"))?;
        assert_canonical(&bundle_bytes)?;
        let bundle: Value = serde_json::from_slice(&bundle_bytes)?;
        let qualification = serde_json::json!({
            "qualificationClass": declaration["qualificationClass"],
            "experiencedOperatorOrDirector": declaration["experiencedOperatorOrDirector"],
            "attestationSignedPayloadSha256": declaration["signedPayloadSha256"],
            "attestationSignatureEvidenceSha256": declaration["signatureEvidenceSha256"],
        });
        assert_eq!(
            bundle,
            serde_json::json!({
                "schemaVersion": 1, "pairId": attestation["pairId"], "reviewerId": reviewer_id,
                "qualification": qualification, "caseSha256": sha256(&case_bytes),
                "materialsManifestSha256": sha256(b"[]"), "sourceMaterialsSha256": sha256(b"[]"),
                "aSha256": sha256(expected_a), "bSha256": sha256(expected_b),
                "rubricSha256": jcs_sha(&rubric_bytes)?,
                "reviewerSubmissionSchemaSha256": sha256(&schema_bytes),
            })
        );
        let commitment = seed_commitment(&seed);
        assert_eq!(
            mapping,
            serde_json::json!({
                "schemaVersion": 1, "pairId": attestation["pairId"], "reviewerId": reviewer_id,
                "reviewBundleSha256": sha256(&bundle_bytes), "seedCommitment": commitment,
                "a": orientation[0], "b": orientation[1],
            })
        );
        sensitive.extend([
            commitment.clone(),
            sha256(&mapping_bytes),
            seed.iter().map(|byte| format!("{byte:02x}")).collect(),
        ]);
        receipt_mappings.push(serde_json::json!({
            "reviewerId": reviewer_id,
            "reviewBundleSha256": sha256(&bundle_bytes),
            "mappingSha256": sha256(&mapping_bytes),
            "seedCommitment": commitment,
        }));
    }

    let inventory_root = assert_private_inventory(root)?;
    let receipt_bytes = fs::read(root.join("coordinator/blind-pack-receipt.json"))?;
    assert_canonical(&receipt_bytes)?;
    let receipt: Value = serde_json::from_slice(&receipt_bytes)?;
    let pair_verification =
        fs::read(root.join("replay-coordinator/replay-pair-verification.json"))?;
    let generated = chrono::DateTime::parse_from_rfc3339(receipt["generatedAt"].as_str().unwrap())?;
    assert_eq!(
        generated
            .to_utc()
            .to_rfc3339_opts(SecondsFormat::Millis, true),
        receipt["generatedAt"]
    );
    assert_eq!(
        receipt,
        serde_json::json!({
            "schemaVersion": 1, "pairId": attestation["pairId"],
            "frozenRunContextSha256": sha256(&fs::read(frozen_context)?),
            "pairReceiptSha256": Value::Null, "pairVerificationSha256": sha256(&pair_verification),
            "rubricSha256": jcs_sha(&rubric_bytes)?, "decisionPolicySha256": jcs_sha(&policy_bytes)?,
            "reviewerSubmissionSchemaSha256": sha256(&schema_bytes),
            "reviewerMappings": receipt_mappings, "inventoryRootSha256": inventory_root,
            "reviewsDropSha256": sha256(br#"{"entries":[]}"#),
            "generatedAt": generated.to_utc().to_rfc3339_opts(SecondsFormat::Millis, true),
        })
    );

    let visible = tree_snapshot(&root.join("reviewer"))?;
    let visible_bytes = visible
        .into_iter()
        .flat_map(|(path, bytes)| {
            path.to_string_lossy()
                .as_bytes()
                .to_vec()
                .into_iter()
                .chain(bytes.unwrap_or_default())
        })
        .collect::<Vec<_>>();
    let visible_text = String::from_utf8_lossy(&visible_bytes).to_ascii_lowercase();
    for forbidden in [
        "generic",
        "candidate",
        "blind_treatment_alpha_7d92",
        "hidden_arm_signal_c4e1",
        "mechanical_review_seed_9b7a",
        "seedcommitment",
        "threadid",
        "turnid",
        "responseid",
        "totaltokens",
        "durationms",
    ] {
        assert!(
            !visible_text.contains(forbidden),
            "visible leak: {forbidden}"
        );
    }
    for forbidden in sensitive {
        assert!(
            !visible_text.contains(&forbidden),
            "visible leak: {forbidden}"
        );
    }
    Ok(())
}

fn assert_private_inventory(root: &Path) -> Result<String> {
    let bytes = fs::read(root.join("coordinator/private-inventory.jsonl"))?;
    assert!(bytes.ends_with(b"\n"));
    let lines = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let mut paths = BTreeSet::new();
    for (index, line) in lines.iter().enumerate() {
        assert_canonical(line)?;
        let record: Value = serde_json::from_slice(line)?;
        assert_eq!(record["sequence"], index + 1);
        assert_eq!(
            record["previousRecordSha256"],
            index
                .checked_sub(1)
                .map(|previous| Value::String(sha256(lines[previous])))
                .unwrap_or(Value::Null)
        );
        let relative = record["relativePath"].as_str().unwrap();
        assert!(paths.insert(relative.to_string()));
        let path = root.join(relative);
        if record["kind"] == "file" {
            assert_eq!(record["sha256"], sha256(&fs::read(path)?));
        } else {
            assert!(path.is_dir());
            assert_eq!(record["sha256"], Value::Null);
        }
    }
    let last = serde_json::from_slice::<Value>(lines.last().unwrap())?;
    assert_eq!(last["relativePath"], "coordinator/blind-pack-receipt.json");
    let last_start = bytes.len() - lines.last().unwrap().len() - 1;
    let receipt: Value =
        serde_json::from_slice(&fs::read(root.join("coordinator/blind-pack-receipt.json"))?)?;
    let inventory_root = sha256(&bytes[..last_start]);
    assert_eq!(receipt["inventoryRootSha256"], inventory_root);
    let expected = tree_snapshot(root)?
        .into_keys()
        .filter(|path| path != Path::new("coordinator/private-inventory.jsonl"))
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect();
    assert_eq!(paths, expected);
    Ok(inventory_root)
}

fn transcript_package(path: &Path) -> Result<Vec<u8>> {
    let mut package = None;
    for line in fs::read_to_string(path)?.lines() {
        let event: Value = serde_json::from_str(line)?;
        if event["method"] == "item/completed" && event["params"]["item"]["type"] == "agentMessage"
        {
            package = Some(
                event["params"]["item"]["text"]
                    .as_str()
                    .unwrap()
                    .as_bytes()
                    .to_vec(),
            );
        }
    }
    Ok(package.unwrap())
}

fn replay_seed(value: &str) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"AI-IP-REPLAY-SEED-V1\0");
    digest.update(u64::try_from(value.len()).unwrap().to_be_bytes());
    digest.update(value.as_bytes());
    digest.finalize().into()
}

fn seed_commitment(seed: &[u8; 32]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"AI-IP-BLIND-SEED-COMMITMENT-V1\0");
    digest.update(seed);
    format!("{:x}", digest.finalize())
}

fn assert_canonical(bytes: &[u8]) -> Result<()> {
    assert!(!bytes.ends_with(b"\n"));
    assert_eq!(
        serde_json_canonicalizer::to_vec(&serde_json::from_slice::<Value>(bytes)?)?,
        bytes
    );
    Ok(())
}

fn jcs_sha(bytes: &[u8]) -> Result<String> {
    Ok(sha256(&serde_json_canonicalizer::to_vec(
        &serde_json::from_slice::<Value>(bytes)?,
    )?))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn raw_set_commitment(domain: &[u8], entries: &[(String, Vec<u8>)]) -> Result<String> {
    let mut entries = entries.iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let mut digest = Sha256::new();
    digest.update(domain);
    for (reviewer_id, raw) in entries {
        digest.update(u32::try_from(reviewer_id.len())?.to_be_bytes());
        digest.update(reviewer_id.as_bytes());
        digest.update(Sha256::digest(raw));
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn tree_snapshot(root: &Path) -> Result<BTreeMap<PathBuf, Option<Vec<u8>>>> {
    fn visit(
        root: &Path,
        current: &Path,
        snapshot: &mut BTreeMap<PathBuf, Option<Vec<u8>>>,
    ) -> Result<()> {
        let mut entries = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let relative = path.strip_prefix(root)?.to_path_buf();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                snapshot.insert(relative, None);
                visit(root, &path, snapshot)?;
            } else {
                assert!(kind.is_file(), "unexpected tree entry: {}", path.display());
                snapshot.insert(relative, Some(fs::read(path)?));
            }
        }
        Ok(())
    }

    let mut snapshot = BTreeMap::new();
    visit(root, root, &mut snapshot)?;
    Ok(snapshot)
}

fn blind_command(binary: &Path, frozen_context: &Path) -> Command {
    let mut command = Command::new(binary);
    command
        .args([
            "blind-pack",
            "--reviewer-root",
            "reviewer",
            "--mapping-dir",
            "coordinator/mappings",
            "--replay-seed",
            "one",
            "--replay-seed",
            "two",
            "--replay-seed",
            "three",
            "--frozen-run-context",
        ])
        .arg(frozen_context);
    command
}

fn score_command(binary: &Path, private_root: &Path, frozen_context: &Path) -> Command {
    let mut command = Command::new(binary);
    command
        .args(["score", "--mapping-dir", "coordinator/mappings"])
        .arg("--reviews-dir")
        .arg(private_root.join("reviews"))
        .arg("--output")
        .arg(private_root.join("coordinator/decision.private.json"))
        .arg("--frozen-run-context")
        .arg(frozen_context);
    command
}

fn assert_success(stage: &str, output: &Output) {
    assert!(
        output.status.success(),
        "{stage} failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_no_outputs(private_root: &Path) {
    for relative in [
        "reviewer",
        "coordinator/mappings",
        "coordinator/blind-seeds",
        "reviews",
        "coordinator/blind-pack-receipt.json",
    ] {
        assert_path_absent(private_root, relative);
    }
}

fn assert_path_absent(private_root: &Path, relative: &str) {
    match fs::symlink_metadata(private_root.join(relative)) {
        Err(error) => assert_eq!(
            error.kind(),
            std::io::ErrorKind::NotFound,
            "found {relative}"
        ),
        Ok(metadata) => panic!("found {relative} with type {:?}", metadata.file_type()),
    }
}
