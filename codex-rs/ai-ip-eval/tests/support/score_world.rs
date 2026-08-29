use std::collections::BTreeMap;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;

use anyhow::Context;
use anyhow::Result;
use codex_utils_cargo_bin::cargo_bin;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use tempfile::TempDir;

const REVIEW_SUBMISSION_DOMAIN: &[u8] = b"AI-IP-REVIEW-SUBMISSION-V1\0";

pub struct ScoreWorld {
    _temp: TempDir,
    score_binary: PathBuf,
    frozen_codex_binary: PathBuf,
    fixture_root: PathBuf,
    private_root: PathBuf,
    frozen_context: PathBuf,
}

impl ScoreWorld {
    pub fn new() -> Result<Self> {
        Self::build(/*copy_codex_binary*/ false)
    }

    pub fn with_copied_codex_binary() -> Result<Self> {
        Self::build(/*copy_codex_binary*/ true)
    }

    fn build(copy_codex_binary: bool) -> Result<Self> {
        let score_binary = cargo_bin("codex-ai-ip-eval")?;
        let fixture_set =
            codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json")?;
        let fixture_root = fixture_set
            .parent()
            .context("fixture-set parent")?
            .canonicalize()?;
        let temp = TempDir::new()?;
        let frozen_codex_binary = if copy_codex_binary {
            let copy = temp
                .path()
                .join(format!("frozen-codex{}", std::env::consts::EXE_SUFFIX));
            fs::copy(&score_binary, &copy)?;
            let mut permissions = fs::metadata(&copy)?.permissions();
            #[cfg(unix)]
            permissions.set_mode(permissions.mode() | 0o200);
            #[cfg(not(unix))]
            permissions.set_readonly(false);
            fs::set_permissions(&copy, permissions)?;
            copy.canonicalize()?
        } else {
            score_binary.clone()
        };
        let private_root = temp.path().join("private");
        fs::create_dir(&private_root)?;
        #[cfg(unix)]
        fs::set_permissions(&private_root, fs::Permissions::from_mode(/*mode*/ 0o700))?;
        let private_root = private_root.canonicalize()?;
        let frozen_context = private_root.join("frozen-run-context.json");

        let freeze = Command::new(&score_binary)
            .args(["freeze-run-context", "replay", "--repo-root"])
            .arg(&fixture_root)
            .args(["--fork-sha", "synthetic-replay-fork", "--private-root"])
            .arg(&private_root)
            .arg("--codex-bin")
            .arg(&frozen_codex_binary)
            .arg("--case")
            .arg(fixture_root.join("replay-case.json"))
            .arg("--transcript")
            .arg(fixture_root.join("replay-transcript.jsonl"))
            .arg("--fixture-set-manifest")
            .arg(&fixture_set)
            .arg("--output")
            .arg(&frozen_context)
            .output()?;
        assert_success("freeze-run-context replay", &freeze);
        let replay = Command::new(&score_binary)
            .args(["replay-pair", "--frozen-run-context"])
            .arg(&frozen_context)
            .output()?;
        assert_success("replay-pair", &replay);
        let blind = blind_command(&score_binary, &frozen_context).output()?;
        assert_success("blind-pack", &blind);

        Ok(Self {
            _temp: temp,
            score_binary,
            frozen_codex_binary,
            fixture_root,
            private_root,
            frozen_context,
        })
    }

    pub fn install_reviews(&self) -> Result<()> {
        for ordinal in 1..=3 {
            fs::copy(
                self.fixture_root
                    .join(format!("replay-review-{ordinal}.json")),
                self.review_path(&format!("reviewer-{ordinal}")),
            )?;
        }
        Ok(())
    }

    pub fn mutate_review(
        &self,
        reviewer_id: &str,
        mutation: impl FnOnce(&mut Value),
    ) -> Result<()> {
        let path = self.review_path(reviewer_id);
        let mut review: Value = serde_json::from_slice(&fs::read(&path)?)?;
        mutation(&mut review);
        fs::write(path, resign_submission(review)?)?;
        Ok(())
    }

    pub fn candidate_arm(&self, reviewer_id: &str) -> Result<&'static str> {
        let mapping: Value = serde_json::from_slice(&fs::read(
            self.private_root
                .join("coordinator/mappings")
                .join(format!("{reviewer_id}.json")),
        )?)?;
        match (mapping["a"].as_str(), mapping["b"].as_str()) {
            (Some("candidate"), Some("generic")) => Ok("A"),
            (Some("generic"), Some("candidate")) => Ok("B"),
            _ => anyhow::bail!("mapping does not contain one candidate arm"),
        }
    }

    pub fn score(&self) -> Result<Output> {
        self.score_with(&self.score_binary)
    }

    pub fn score_from_copied_evaluator(&self) -> Result<Output> {
        let copied = self._temp.path().join(format!(
            "current-evaluator-copy{}",
            std::env::consts::EXE_SUFFIX
        ));
        fs::copy(&self.score_binary, &copied)?;
        self.score_with(&copied)
    }

    fn score_with(&self, binary: &Path) -> Result<Output> {
        Ok(score_command(binary, &self.private_root, &self.frozen_context).output()?)
    }

    pub fn decision_bytes(&self) -> Result<Vec<u8>> {
        Ok(fs::read(
            self.private_root.join("coordinator/decision.private.json"),
        )?)
    }

    pub fn mutate_frozen_codex_binary(&self) -> Result<()> {
        if self.frozen_codex_binary == self.score_binary {
            anyhow::bail!("test world does not use an isolated frozen Codex binary");
        }
        fs::write(&self.frozen_codex_binary, b"changed frozen Codex bytes\n")?;
        Ok(())
    }

    pub fn expected_commitments(&self) -> Result<Value> {
        let reviewer_ids = ["reviewer-1", "reviewer-2", "reviewer-3"];
        let mapping_entries = reviewer_ids.map(|reviewer_id| {
            Ok((
                reviewer_id.to_string(),
                fs::read(
                    self.private_root
                        .join("coordinator/mappings")
                        .join(format!("{reviewer_id}.json")),
                )?,
            ))
        });
        let review_entries = reviewer_ids.map(|reviewer_id| {
            Ok((
                reviewer_id.to_string(),
                fs::read(self.review_path(reviewer_id))?,
            ))
        });
        let mapping_entries = mapping_entries.into_iter().collect::<Result<Vec<_>>>()?;
        let review_entries = review_entries.into_iter().collect::<Result<Vec<_>>>()?;
        let receipt = fs::read(
            self.private_root
                .join("coordinator/blind-pack-receipt.json"),
        )?;
        let attestation: Value = serde_json::from_slice(&fs::read(
            self.fixture_root.join("replay-attestation.json"),
        )?)?;
        let policy = fs::read(codex_utils_cargo_bin::find_resource!(
            "../../ai-ip-evals/rubrics/blind-review-decision-policy.json"
        )?)?;
        Ok(serde_json::json!({
            "pairId": attestation["pairId"],
            "frozenRunContextSha256": sha256(&fs::read(&self.frozen_context)?),
            "blindPackReceiptSha256": sha256(&receipt),
            "reviewerMappingsSha256": raw_set_commitment(
                b"AI-IP-BLIND-MAPPING-SET-V1\0", &mapping_entries)?,
            "reviewSubmissionsSha256": raw_set_commitment(
                b"AI-IP-BLIND-REVIEW-SET-V1\0", &review_entries)?,
            "rubricSha256": "63614f7ceaaebd785cc00c552a3c5156d3ecc70634958c87e2814f195591e56d",
            "decisionPolicySha256": jcs_sha(&policy)?,
        }))
    }

    pub fn tree_snapshot(&self) -> Result<BTreeMap<PathBuf, Option<Vec<u8>>>> {
        tree_snapshot(&self.private_root)
    }

    pub fn assert_decision_paths_absent(&self) {
        self.assert_staging_absent();
        assert_path_absent(&self.private_root, "coordinator/decision.private.json");
    }

    pub fn assert_staging_absent(&self) {
        assert_path_absent(
            &self.private_root,
            "coordinator/.decision.private.json.staging",
        );
    }

    fn review_path(&self, reviewer_id: &str) -> PathBuf {
        self.private_root
            .join("reviews")
            .join(format!("{reviewer_id}.json"))
    }
}

pub fn assert_success(stage: &str, output: &Output) {
    assert!(
        output.status.success(),
        "{stage} failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
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

fn resign_submission(mut value: Value) -> Result<Vec<u8>> {
    let evidence = value["signatureEvidence"]
        .as_str()
        .context("review signature evidence")?
        .to_owned();
    value["signatureEvidenceSha256"] = Value::String(sha256(evidence.as_bytes()));
    let mut signed = value.clone();
    let signed = signed.as_object_mut().context("review object")?;
    signed.remove("signedPayloadSha256");
    signed.remove("signatureEvidenceSha256");
    let canonical_signed = serde_json_canonicalizer::to_vec(&signed)?;
    let mut digest = Sha256::new();
    digest.update(REVIEW_SUBMISSION_DOMAIN);
    digest.update(canonical_signed);
    value["signedPayloadSha256"] = Value::String(format!("{:x}", digest.finalize()));
    Ok(serde_json_canonicalizer::to_vec(&value)?)
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn jcs_sha(bytes: &[u8]) -> Result<String> {
    Ok(sha256(&serde_json_canonicalizer::to_vec(
        &serde_json::from_slice::<Value>(bytes)?,
    )?))
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
