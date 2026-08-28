use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use pretty_assertions::assert_eq;
use rand::TryRngCore;
use sha2::Digest;
use sha2::Sha256;

use super::*;
use crate::blind_bundle_model::BlindArmProjection;
use crate::blind_bundle_model::BlindMaterialProjection;
use crate::blind_bundle_model::BlindPairBundleProjection;
use crate::blind_bundle_model::BlindReviewerProjection;

const ONE_SEED: &str = "5b870385a70b814e565cbf1f117ee04c7216447c304d4ae4d5a280b4533a755c";
const TWO_SEED: &str = "80d9cee288d6e1f1a72d6a57bd9d16f01d820cd665347baf7029fac64c49c56b";
const THREE_SEED: &str = "b876a96c066ae8f767fce34fd556c4b4c9d5544a4399f2c41c0c6fab376ebb84";
const RUBRIC_BYTES: &[u8] =
    include_bytes!("../../../ai-ip-evals/rubrics/content-package-blind-review.json");
const POLICY_BYTES: &[u8] =
    include_bytes!("../../../ai-ip-evals/rubrics/blind-review-decision-policy.json");
const SUBMISSION_SCHEMA_BYTES: &[u8] =
    include_bytes!("../../../ai-ip-evals/rubrics/reviewer-submission.schema.json");

struct ProjectionFixture {
    private_root: PathBuf,
    reviewers: [(String, String, bool, String, String); 3],
    case_bytes: Vec<u8>,
    manifest: Vec<codex_ai_ip_domain::MissionMaterial>,
    manifest_bytes: Vec<u8>,
    material_bytes: Vec<u8>,
    first_package: Vec<u8>,
    second_package: Vec<u8>,
}

impl ProjectionFixture {
    fn new(mode: crate::ExecutionMode) -> Self {
        let manifest: Vec<codex_ai_ip_domain::MissionMaterial> =
            serde_json::from_value(serde_json::json!([{
                "materialId": "evidence-1",
                "relativePath": "notes/evidence.txt",
                "sha256": sha256(b"source evidence"),
                "materialKind": "evidence"
            }]))
            .unwrap();
        let private_root = match mode {
            crate::ExecutionMode::Replay => PathBuf::from("/private/replay-root"),
            crate::ExecutionMode::Mock | crate::ExecutionMode::Live => {
                PathBuf::from("/private/native-root")
            }
        };
        Self {
            private_root,
            reviewers: [
                reviewer("reviewer-z", "operator", true, '1'),
                reviewer("reviewer-a", "director", true, '2'),
                reviewer("reviewer-m", "specialist", false, '3'),
            ],
            case_bytes: br#"{"caseId":"case-1","objective":"Create a launch plan"}"#.to_vec(),
            manifest_bytes: serde_json::to_vec(&manifest).unwrap(),
            manifest,
            material_bytes: b"source evidence".to_vec(),
            first_package: br#"{"title":"alpha package"}"#.to_vec(),
            second_package: br#"{"title":"beta package"}"#.to_vec(),
        }
    }

    fn projection(&self, mode: crate::ExecutionMode) -> BlindPairBundleProjection<'_> {
        BlindPairBundleProjection {
            mode,
            private_root: &self.private_root,
            pair_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            frozen_run_context_sha256: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            pair_verification_sha256: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            pair_receipt_sha256: (mode != crate::ExecutionMode::Replay)
                .then_some("dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"),
            reviewers: self
                .reviewers
                .each_ref()
                .map(|reviewer| BlindReviewerProjection {
                    reviewer_id: &reviewer.0,
                    qualification_class: &reviewer.1,
                    experienced_operator_or_director: reviewer.2,
                    attestation_signed_payload_sha256: &reviewer.3,
                    attestation_signature_evidence_sha256: &reviewer.4,
                }),
            case_bytes: &self.case_bytes,
            materials_manifest: &self.manifest,
            materials_manifest_bytes: &self.manifest_bytes,
            materials: vec![BlindMaterialProjection {
                material_id: &self.manifest[0].material_id,
                relative_path: &self.manifest[0].relative_path,
                sha256: &self.manifest[0].sha256,
                bytes: &self.material_bytes,
            }],
            arms: [
                BlindArmProjection {
                    condition: crate::EvaluationCondition::Candidate,
                    package_bytes: &self.first_package,
                },
                BlindArmProjection {
                    condition: crate::EvaluationCondition::Generic,
                    package_bytes: &self.second_package,
                },
            ],
            forbidden_visible_markers: vec![
                "BLIND_TREATMENT_ALPHA_7D92".to_string(),
                self.private_root.to_string_lossy().into_owned(),
            ],
        }
    }
}

#[test]
fn blind_bundle_preparation_replay_wire_is_exact_and_slot_bound() {
    let fixture = ProjectionFixture::new(crate::ExecutionMode::Replay);
    let projection = fixture.projection(crate::ExecutionMode::Replay);
    let seeds = ["one", "two", "three"].map(str::to_string);
    let first = crate::blind_bundle::test_prepare_replay_projection(&projection, &seeds).unwrap();
    let second = crate::blind_bundle::test_prepare_replay_projection(&projection, &seeds).unwrap();
    assert_eq!(first, second);
    let mut other_root = ProjectionFixture::new(crate::ExecutionMode::Replay);
    other_root.private_root = PathBuf::from("/different/private/root");
    let other = crate::blind_bundle::test_prepare_replay_projection(
        &other_root.projection(crate::ExecutionMode::Replay),
        &seeds,
    )
    .unwrap();
    assert_eq!(
        first.reviewers.each_ref().map(|reviewer| (
            reviewer.review_bundle_bytes.as_slice(),
            reviewer.mapping_bytes.as_slice(),
        )),
        other.reviewers.each_ref().map(|reviewer| (
            reviewer.review_bundle_bytes.as_slice(),
            reviewer.mapping_bytes.as_slice(),
        ))
    );
    assert_eq!(
        first
            .reviewers
            .each_ref()
            .map(|reviewer| reviewer.reviewer_id.as_str()),
        ["reviewer-z", "reviewer-a", "reviewer-m"]
    );
    assert_eq!(
        first
            .reviewers
            .each_ref()
            .map(|reviewer| reviewer.mapping.a),
        [
            crate::EvaluationCondition::Candidate,
            crate::EvaluationCondition::Generic,
            crate::EvaluationCondition::Generic,
        ]
    );
    assert_eq!(
        first.reviewers[0].seed_commitment,
        "b68fa55556ad75f292841d3146222f2446d2b2a9a04aad0111beaf5f8b998384"
    );
    for reviewer in &first.reviewers {
        assert_eq!(reviewer.native_seed, None);
        assert_eq!(
            reviewer.review_bundle_sha256,
            sha256(&reviewer.review_bundle_bytes)
        );
        assert_eq!(reviewer.mapping_sha256, sha256(&reviewer.mapping_bytes));
        assert_eq!(
            reviewer.mapping.review_bundle_sha256,
            reviewer.review_bundle_sha256
        );
        assert_eq!(reviewer.mapping.seed_commitment, reviewer.seed_commitment);
        assert!(!reviewer.review_bundle_bytes.ends_with(b"\n"));
        assert!(!reviewer.mapping_bytes.ends_with(b"\n"));
        assert_eq!(
            crate::jcs::canonicalize_value(
                &crate::jcs::parse_json(&reviewer.review_bundle_bytes).unwrap()
            )
            .unwrap(),
            reviewer.review_bundle_bytes
        );
        assert_eq!(
            crate::jcs::canonicalize_value(
                &crate::jcs::parse_json(&reviewer.mapping_bytes).unwrap()
            )
            .unwrap(),
            reviewer.mapping_bytes
        );
        let visible = String::from_utf8(reviewer.review_bundle_bytes.clone()).unwrap();
        for forbidden in ["candidate", "generic", "/private/", "signatureEvidence\""] {
            assert!(
                !visible
                    .to_ascii_lowercase()
                    .contains(&forbidden.to_ascii_lowercase())
            );
        }
    }
    assert_eq!(first.reviewers[0].a_bytes, fixture.first_package);
    assert_eq!(first.reviewers[0].b_bytes, fixture.second_package);
    assert_eq!(first.reviewers[1].a_bytes, fixture.second_package);
    assert_eq!(first.reviewers[1].b_bytes, fixture.first_package);
    assert_eq!(first.materials_manifest_bytes, fixture.manifest_bytes);
    assert_eq!(first.materials[0].bytes, fixture.material_bytes);
    let rubric_sha = crate::jcs::commitment(b"", RUBRIC_BYTES).unwrap().sha256;
    let policy_sha = crate::jcs::commitment(b"", POLICY_BYTES).unwrap().sha256;
    let schema_sha = sha256(SUBMISSION_SCHEMA_BYTES);
    assert_ne!(sha256(first.rubric_bytes), rubric_sha);
    assert_eq!(first.rubric_sha256, rubric_sha);
    assert_eq!(first.decision_policy_sha256, policy_sha);
    assert_eq!(first.reviewer_submission_schema_sha256, schema_sha);
    let expected_manifest = serde_json::json!({
        "schemaVersion": 1,
        "pairId": first.pair_id,
        "reviewerId": "reviewer-z",
        "qualification": {
            "qualificationClass": "operator",
            "experiencedOperatorOrDirector": true,
            "attestationSignedPayloadSha256": "1".repeat(64),
            "attestationSignatureEvidenceSha256": format!("e{}", "1".repeat(63)),
        },
        "caseSha256": sha256(&fixture.case_bytes),
        "materialsManifestSha256": sha256(&fixture.manifest_bytes),
        "sourceMaterialsSha256": sha256(&serde_json::to_vec(&fixture.manifest).unwrap()),
        "aSha256": sha256(&fixture.first_package),
        "bSha256": sha256(&fixture.second_package),
        "rubricSha256": first.rubric_sha256,
        "reviewerSubmissionSchemaSha256": first.reviewer_submission_schema_sha256,
    });
    assert_eq!(
        crate::jcs::parse_json(&first.reviewers[0].review_bundle_bytes).unwrap(),
        expected_manifest
    );
    assert_eq!(
        crate::jcs::parse_json(&first.reviewers[0].mapping_bytes).unwrap(),
        serde_json::json!({
            "schemaVersion": 1,
            "pairId": first.pair_id,
            "reviewerId": "reviewer-z",
            "reviewBundleSha256": first.reviewers[0].review_bundle_sha256,
            "seedCommitment": first.reviewers[0].seed_commitment,
            "a": "candidate",
            "b": "generic",
        })
    );
}

#[test]
fn blind_bundle_preparation_replay_rejects_bad_sets_and_uses_utf8_byte_length() {
    let fixture = ProjectionFixture::new(crate::ExecutionMode::Replay);
    let projection = fixture.projection(crate::ExecutionMode::Replay);
    for seeds in [
        vec!["one", "two"],
        vec!["one", "two", "three", "four"],
        vec!["one", "one", "two"],
        vec!["", "two", "three"],
    ] {
        assert!(
            crate::blind_bundle::test_prepare_replay_projection(
                &projection,
                &seeds.into_iter().map(str::to_string).collect::<Vec<_>>()
            )
            .is_err()
        );
    }
    let all_same = ["a", "b", "c"].map(str::to_string);
    let error =
        crate::blind_bundle::test_prepare_replay_projection(&projection, &all_same).unwrap_err();
    assert_eq!(
        error.to_string(),
        "Replay seed set has duplicate bytes or one orientation for all reviewers"
    );
    let unicode = ["é", "two", "one"].map(str::to_string);
    let prepared =
        crate::blind_bundle::test_prepare_replay_projection(&projection, &unicode).unwrap();
    assert_eq!(
        prepared.reviewers[0].seed_commitment,
        "e3750dacbbc06905ab81dcd8fc9ff28d71572343a5a1a3783a9ccd18361329ea"
    );

    let mut leaked = ProjectionFixture::new(crate::ExecutionMode::Replay);
    leaked.case_bytes = br#"{"objective":"candidate creators use generic formats"}"#.to_vec();
    crate::blind_bundle::test_prepare_replay_projection(
        &leaked.projection(crate::ExecutionMode::Replay),
        &["one", "two", "three"].map(str::to_string),
    )
    .unwrap();
    leaked.case_bytes = br#"{"objective":"BLIND_TREATMENT_ALPHA_7D9\u0032"}"#.to_vec();
    let error = crate::blind_bundle::test_prepare_replay_projection(
        &leaked.projection(crate::ExecutionMode::Replay),
        &["one", "two", "three"].map(str::to_string),
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "reviewer-visible case.json contains a forbidden treatment marker"
    );
}

#[test]
fn blind_bundle_preparation_native_uses_complete_bounded_seed_sets() {
    let fixture = ProjectionFixture::new(crate::ExecutionMode::Mock);
    let projection = fixture.projection(crate::ExecutionMode::Mock);
    let valid = seed_set([ONE_SEED, TWO_SEED, THREE_SEED]);
    let mut rng = ScriptedRng::new(vec![Ok([0_u8; 96]), Ok(valid)]);
    let prepared =
        crate::blind_bundle::test_prepare_native_projection(&projection, &mut rng).unwrap();
    assert_eq!(rng.calls, 2);
    assert_eq!(
        prepared
            .reviewers
            .each_ref()
            .map(|reviewer| reviewer.native_seed.unwrap()),
        valid_chunks(valid)
    );

    let mut exhausted = ScriptedRng::new(
        std::iter::repeat_n(Ok([0_u8; 96]), 32)
            .chain([Ok(valid)])
            .collect(),
    );
    let error = crate::blind_bundle::test_prepare_native_projection(&projection, &mut exhausted)
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "native blind seed resampling exhausted after 32 complete sets"
    );
    assert_eq!(exhausted.calls, 32);

    let mut failed = ScriptedRng::new(vec![Err(TestEntropyError)]);
    let error =
        crate::blind_bundle::test_prepare_native_projection(&projection, &mut failed).unwrap_err();
    assert_eq!(error.to_string(), "generate native blind seed set");
}

#[test]
fn blind_bundle_preparation_real_pairs_are_read_only_and_reverified() {
    let fixture = replay_fixture_with_material();
    let replay = run_frozen_replay_pair_from(fixture.path()).unwrap();
    let before = tree_snapshot(&replay.private_root);
    let snapshot =
        crate::blind::read_context_snapshot(&replay.private_root.join("frozen-run-context.json"))
            .unwrap();
    let core = crate::blind_verify::verify_pair_evidence_core(&snapshot).unwrap();
    let expected = core
        .arms
        .iter()
        .map(|arm| (arm.condition, arm.content_package_bytes.clone()))
        .collect::<Vec<_>>();
    let signature_body = core.reviewers[0].signature_evidence.clone();
    let pair = crate::blind_finalize::finalize_blind_pair(core).unwrap();
    let prepared = crate::blind_bundle::prepare_replay_blind_bundles(
        &pair,
        &["one", "two", "three"].map(str::to_string),
    )
    .unwrap();
    assert_eq!(tree_snapshot(&replay.private_root), before);
    for reviewer in &prepared.reviewers {
        assert_eq!(
            reviewer.a_bytes.as_slice(),
            expected
                .iter()
                .find(|(condition, _)| *condition == reviewer.mapping.a)
                .unwrap()
                .1
                .as_slice()
        );
        assert_eq!(
            reviewer.b_bytes.as_slice(),
            expected
                .iter()
                .find(|(condition, _)| *condition == reviewer.mapping.b)
                .unwrap()
                .1
                .as_slice()
        );
        assert!(
            !reviewer
                .review_bundle_bytes
                .windows(signature_body.len())
                .any(|part| part == signature_body.as_bytes())
        );
    }
    fs::write(fixture.path().join("notes/evidence.txt"), b"late drift\n").unwrap();
    let error = crate::blind_bundle::prepare_replay_blind_bundles(
        &pair,
        &["one", "two", "three"].map(str::to_string),
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "reverify Replay material replay-evidence"
    );

    let native = run_native_mock_pair_with_marker(
        /*marker*/ None, /*max_total_tokens_per_run*/ 10,
    );
    native.result.as_ref().unwrap();
    let live_root = native.live_root.canonicalize().unwrap();
    let before = tree_snapshot(&live_root);
    let snapshot =
        crate::blind::read_context_snapshot(&live_root.join("frozen-run-context.json")).unwrap();
    let core = crate::blind_verify::verify_pair_evidence_core(&snapshot).unwrap();
    let pair = crate::blind_finalize::finalize_blind_pair(core).unwrap();
    let mut rng = ScriptedRng::new(vec![Ok(seed_set([ONE_SEED, TWO_SEED, THREE_SEED]))]);
    let prepared = crate::blind_bundle::prepare_native_blind_bundles(&pair, &mut rng).unwrap();
    assert_eq!(prepared.mode, crate::ExecutionMode::Mock);
    assert!(prepared.pair_receipt_sha256.is_some());
    assert!(
        prepared
            .reviewers
            .iter()
            .all(|reviewer| reviewer.native_seed.is_some())
    );
    assert_eq!(tree_snapshot(&live_root), before);
    fs::write(
        live_root
            .join("inputs/case")
            .join(&prepared.materials[0].relative_path),
        b"late native drift\n",
    )
    .unwrap();
    let mut retry_rng = ScriptedRng::new(vec![Ok(seed_set([ONE_SEED, TWO_SEED, THREE_SEED]))]);
    let error =
        crate::blind_bundle::prepare_native_blind_bundles(&pair, &mut retry_rng).unwrap_err();
    assert_eq!(error.to_string(), "re-read material:evidence-1 artifact");
    for relative in [
        "reviewer",
        "coordinator/mappings",
        "coordinator/blind-seeds",
        "reviews",
        "coordinator/blind-pack-receipt.json",
    ] {
        assert!(!live_root.join(relative).exists(), "unexpected {relative}");
    }
}

fn reviewer(
    id: &str,
    class: &str,
    experienced: bool,
    marker: char,
) -> (String, String, bool, String, String) {
    (
        id.to_string(),
        class.to_string(),
        experienced,
        marker.to_string().repeat(64),
        format!("e{}", marker.to_string().repeat(63)),
    )
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn decode_hex(value: &str) -> [u8; 32] {
    std::array::from_fn(|index| u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).unwrap())
}

fn seed_set(values: [&str; 3]) -> [u8; 96] {
    let mut result = [0_u8; 96];
    for (index, value) in values.into_iter().enumerate() {
        result[index * 32..(index + 1) * 32].copy_from_slice(&decode_hex(value));
    }
    result
}

fn valid_chunks(value: [u8; 96]) -> [[u8; 32]; 3] {
    std::array::from_fn(|index| value[index * 32..(index + 1) * 32].try_into().unwrap())
}

#[derive(Debug, Clone, Copy)]
struct TestEntropyError;

impl fmt::Display for TestEntropyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("test entropy failure")
    }
}

struct ScriptedRng {
    sets: std::collections::VecDeque<Result<[u8; 96], TestEntropyError>>,
    calls: usize,
}

impl ScriptedRng {
    fn new(sets: Vec<Result<[u8; 96], TestEntropyError>>) -> Self {
        Self {
            sets: sets.into(),
            calls: 0,
        }
    }
}

impl TryRngCore for ScriptedRng {
    type Error = TestEntropyError;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        unreachable!("blind preparation must request one complete seed set")
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        unreachable!("blind preparation must request one complete seed set")
    }

    fn try_fill_bytes(&mut self, destination: &mut [u8]) -> Result<(), Self::Error> {
        assert_eq!(destination.len(), 96);
        self.calls += 1;
        let next = self.sets.pop_front().expect("unexpected entropy request")?;
        destination.copy_from_slice(&next);
        Ok(())
    }
}

fn tree_snapshot(root: &Path) -> BTreeMap<String, String> {
    fn visit(root: &Path, current: &Path, entries: &mut BTreeMap<String, String>) {
        let mut children = fs::read_dir(current)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        children.sort();
        for path in children {
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            let metadata = fs::symlink_metadata(&path).unwrap();
            if metadata.is_dir() {
                entries.insert(relative, "directory".to_string());
                visit(root, &path, entries);
            } else if metadata.is_file() {
                entries.insert(relative, sha256(&fs::read(path).unwrap()));
            } else {
                entries.insert(relative, "other".to_string());
            }
        }
    }
    let mut entries = BTreeMap::new();
    visit(root, root, &mut entries);
    entries
}
