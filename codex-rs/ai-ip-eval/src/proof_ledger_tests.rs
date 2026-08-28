#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use codex_responses_api_proxy::ExchangeObserver;
use codex_responses_api_proxy::ForwardResult;
use codex_responses_api_proxy::ObservedUsage;
use codex_responses_api_proxy::RequestGate;
use codex_responses_api_proxy::RequestMetadata;
use codex_responses_api_proxy::ResponseCompletedMetadata;
use codex_responses_api_proxy::TransformedRequestEvidence;
use codex_responses_api_proxy::TransformedRequestMetadata;
use pretty_assertions::assert_eq;
use sha2::Digest;
use sha2::Sha256;

use crate::ArmActivation;
use crate::BrokerGateConfig;
use crate::BrokerRuntimeConfig;
use crate::EvaluationCondition;
use crate::ExecutionMode;
use crate::MockProviderMode;
use crate::ModeEvidence;
use crate::PairCoordinator;
use crate::ProofBrokerCompatibilityName;
use crate::RunManifest;
use crate::Usage;
use crate::proof_ledger::BoundRunManifest;
use crate::proof_ledger::NativeLedgerBinding;
use crate::proof_ledger::ParsedAttempt;
use crate::proof_ledger::ParsedAttemptLedger;
use crate::proof_ledger::ParsedLedgerArm;
use crate::proof_ledger::RequestRecord;
use crate::proof_ledger::TerminalRecord;
use crate::proof_ledger::parse_native_attempt_ledger;

const PAIR_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const FROZEN_SHA: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const EXECUTION_SHA: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const ORDER_SHA: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const DEADLINE: &str = "2099-08-28T12:00:00Z";
const MAX_OUTPUT_TOKENS: u64 = 321;
const MAX_ATTEMPTS_PER_ARM: u64 = 2;
const MAX_TOTAL_TOKENS: u64 = 100;

struct ProducerFixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    manifests: [RunManifest; 2],
    manifest_sha256: [String; 2],
    attempts_per_arm: [u64; 2],
}

impl ProducerFixture {
    fn binding(&self) -> NativeLedgerBinding<'_> {
        NativeLedgerBinding {
            pair_id: PAIR_ID,
            frozen_run_context_sha256: FROZEN_SHA,
            execution_context_sha256: EXECUTION_SHA,
            arm_order_commitment: ORDER_SHA,
            deadline: DEADLINE,
            max_output_tokens: MAX_OUTPUT_TOKENS,
            max_attempts_per_arm: MAX_ATTEMPTS_PER_ARM,
            max_total_tokens: MAX_TOTAL_TOKENS,
            manifests: [
                BoundRunManifest {
                    manifest: &self.manifests[0],
                    raw_sha256: &self.manifest_sha256[0],
                },
                BoundRunManifest {
                    manifest: &self.manifests[1],
                    raw_sha256: &self.manifest_sha256[1],
                },
            ],
        }
    }
}

fn producer_fixture(first: EvaluationCondition, second: EvaluationCondition) -> ProducerFixture {
    producer_fixture_with_attempts(first, second, [1, 1])
}

fn producer_fixture_with_attempts(
    first: EvaluationCondition,
    second: EvaluationCondition,
    attempts_per_arm: [u64; 2],
) -> ProducerFixture {
    let temp = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    let root = {
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        temp.path().canonicalize().unwrap()
    };
    #[cfg(windows)]
    let root = {
        let root = temp.path().join("private");
        crate::secure_fs::create_owner_only_dir_new(&root).unwrap();
        root.canonicalize().unwrap()
    };
    crate::secure_fs::create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    let gate = PairCoordinator::create(BrokerGateConfig {
        ledger_path: root.join("coordinator/attempt-index.jsonl"),
        receipt_dir: root.join("coordinator/receipts"),
        pair_id: PAIR_ID.to_string(),
        frozen_run_context_sha256: FROZEN_SHA.to_string(),
        execution_context_sha256: EXECUTION_SHA.to_string(),
        arm_order_commitment: ORDER_SHA.to_string(),
        require_proof_bindings: true,
        runtime: Arc::new(
            BrokerRuntimeConfig::with_run_limits(
                MAX_ATTEMPTS_PER_ARM,
                MAX_OUTPUT_TOKENS,
                1024 * 1024,
                MAX_TOTAL_TOKENS,
            )
            .unwrap(),
        ),
    })
    .unwrap();
    gate.commit_order_for_test(first, second).unwrap();

    let mut manifests = Vec::new();
    let mut manifest_sha256 = Vec::new();
    for (ordinal, condition) in [(1_u8, first), (2_u8, second)] {
        let attempt_count = attempts_per_arm[usize::from(ordinal - 1)];
        let root_thread_id = if ordinal == 1 {
            "0198f5aa-0000-7000-8000-000000000002"
        } else {
            "0198f5aa-0000-7000-8000-000000000003"
        };
        gate.activate_arm(ArmActivation {
            run_ordinal: ordinal,
            condition,
            root_thread_id: root_thread_id.to_string(),
            deadline: Instant::now() + Duration::from_secs(30),
            deadline_rfc3339: DEADLINE.to_string(),
        })
        .unwrap();
        for attempt in 0..attempt_count {
            let request_byte = ordinal + u8::try_from(attempt * 2).unwrap();
            let request_sha = [request_byte; 32];
            let treatment = (condition == EvaluationCondition::Candidate).then_some([9_u8; 32]);
            let transformed = TransformedRequestMetadata {
                content_length: 23,
                sha256: request_sha,
                evidence: TransformedRequestEvidence {
                    raw_sha256: request_sha,
                    normalized_sha256: [request_byte + 2; 32],
                    normalized_base_commitment: [7; 32],
                    treatment_diff_commitment: treatment,
                },
            };
            let permit = gate
                .before_forward(
                    &RequestMetadata {
                        method: "POST".to_string(),
                        path: "/v1/responses".to_string(),
                        window_id: Some(format!("{root_thread_id}:{attempt}")),
                        parent_thread_id: None,
                        is_subagent: false,
                    },
                    &transformed,
                )
                .unwrap();
            gate.response_completed(
                &permit,
                &ResponseCompletedMetadata {
                    response_id: format!("response-{ordinal}-{attempt}"),
                    usage: Some(ObservedUsage {
                        total_tokens: 3,
                        input_tokens: 2,
                        cached_input_tokens: 0,
                        cache_write_input_tokens: 0,
                        output_tokens: 1,
                        reasoning_output_tokens: 0,
                    }),
                    actual_model: Some("mock-revision".to_string()),
                    deployment_or_fingerprint: Some("mock-deployment".to_string()),
                },
            );
            gate.after_forward(permit, &ForwardResult::Completed { status: 200 });
        }
        let proof = gate.active_arm_proof_snapshot().unwrap();
        let manifest = manifest(ordinal, condition, root_thread_id, attempt_count, &proof);
        let raw = serde_json::to_vec_pretty(&manifest).unwrap();
        let raw_sha256 = sha256(&raw);
        gate.bind_active_run_manifest(raw_sha256.clone()).unwrap();
        gate.seal_arm().unwrap();
        manifests.push(manifest);
        manifest_sha256.push(raw_sha256);
    }
    gate.finish().unwrap();
    ProducerFixture {
        _temp: temp,
        root,
        manifests: manifests.try_into().unwrap(),
        manifest_sha256: manifest_sha256.try_into().unwrap(),
        attempts_per_arm,
    }
}

fn manifest(
    ordinal: u8,
    condition: EvaluationCondition,
    root_thread_id: &str,
    attempt_count: u64,
    proof: &crate::broker_gate::ActiveArmProofSnapshot,
) -> RunManifest {
    let hash = "e".repeat(64);
    RunManifest {
        schema_version: 1,
        pair_id: PAIR_ID.to_string(),
        frozen_run_context_sha256: FROZEN_SHA.to_string(),
        execution_context_sha256: EXECUTION_SHA.to_string(),
        run_ordinal: ordinal,
        condition,
        fork_sha: "fork".to_string(),
        case_sha256: hash.clone(),
        source_materials_sha256: hash.clone(),
        prompt_sha256: hash.clone(),
        additional_context_sha256: hash.clone(),
        output_schema_sha256: hash.clone(),
        thread_start_request_sha256: hash.clone(),
        turn_start_request_sha256: hash.clone(),
        shared_config_sha256: hash.clone(),
        effective_config_sha256: hash.clone(),
        config_layers_sha256: hash.clone(),
        native_skill_sha256: (condition == EvaluationCondition::Candidate).then(|| hash.clone()),
        pre_skill_catalog_sha256: hash.clone(),
        post_skill_catalog_sha256: hash.clone(),
        normalized_base_catalog_sha256: hash.clone(),
        skill_use_evidence_sha256: (condition == EvaluationCondition::Candidate)
            .then(|| hash.clone()),
        codex_binary_sha256: hash.clone(),
        evaluator_binary_sha256: hash.clone(),
        broker_component_sha256: hash.clone(),
        first_root_provider_request_commitment: proof.first_root_request.raw_commitment.clone(),
        normalized_first_root_request_commitment: proof
            .first_root_request
            .normalized_commitment
            .clone(),
        normalized_first_root_base_commitment: proof
            .first_root_request
            .normalized_base_commitment
            .clone(),
        first_root_treatment_diff_commitment: proof
            .first_root_request
            .treatment_diff_commitment
            .clone(),
        app_server_transcript_sha256: hash.clone(),
        broker_attempt_ledger_sha256: proof.attempt_index_file_sha256.clone(),
        attempt_index_root_sha256: proof.attempt_index_merkle_root.clone(),
        postprocess_evidence_index_sha256: hash.clone(),
        content_package_sha256: hash,
        root_thread_id: root_thread_id.to_string(),
        root_turn_id: format!("turn-{ordinal}"),
        session_id: "session".to_string(),
        provider_request_attempt_count: attempt_count,
        provider_completed_response_count: attempt_count,
        raw_response_count: attempt_count,
        usage_scope: "completeNativeThreadTree".to_string(),
        usage: Usage {
            total_tokens: i64::try_from(attempt_count * 3).unwrap(),
            input_tokens: i64::try_from(attempt_count * 2).unwrap(),
            cached_input_tokens: 0,
            cache_write_input_tokens: 0,
            output_tokens: i64::try_from(attempt_count).unwrap(),
            reasoning_output_tokens: 0,
        },
        model_label: "mock".to_string(),
        actual_model_revision: "mock-revision".to_string(),
        deployment_or_fingerprint_commitment: Some(sha256(b"mock-deployment")),
        provider_label: "synthetic-loopback-mock".to_string(),
        provider_compatibility_name: ProofBrokerCompatibilityName::OpenAi,
        authorized_evaluation_run_cost_fen: 0,
        max_provider_request_attempts: MAX_ATTEMPTS_PER_ARM,
        max_total_tokens: i64::try_from(MAX_TOTAL_TOKENS).unwrap(),
        max_elapsed_seconds: 30,
        elapsed_ms: 1,
        tree_closed: true,
        execution_mode: ExecutionMode::Mock,
        mode_evidence: ModeEvidence::Mock {
            provider_mode: MockProviderMode::NotRun,
            synthetic_fixture_sha256: "f".repeat(64),
            arm_order_commitment: ORDER_SHA.to_string(),
        },
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn expected_parsed(fixture: &ProducerFixture) -> ParsedAttemptLedger {
    let ledger = std::fs::read(fixture.root.join("coordinator/attempt-index.jsonl")).unwrap();
    let lines = ledger
        .strip_suffix(b"\n")
        .unwrap()
        .split(|byte| *byte == b'\n')
        .collect::<Vec<_>>();
    let mut fold = [0_u8; 32];
    for line in ledger
        .strip_suffix(b"\n")
        .unwrap()
        .split(|byte| *byte == b'\n')
    {
        let leaf = Sha256::digest(line);
        let mut hasher = Sha256::new();
        hasher.update(fold);
        hasher.update(leaf);
        fold = hasher.finalize().into();
    }
    let arm = |index: usize, start: u64| {
        let attempt_count = fixture.attempts_per_arm[index];
        let attempts = (start..start + attempt_count)
            .map(|global| {
                let request_index = usize::try_from(global * 2).unwrap();
                ParsedAttempt {
                    request_raw_sha256: sha256(lines[request_index]),
                    request: serde_json::from_slice::<RequestRecord>(lines[request_index]).unwrap(),
                    terminal: serde_json::from_slice::<TerminalRecord>(lines[request_index + 1])
                        .unwrap(),
                }
            })
            .collect();
        ParsedLedgerArm {
            condition: fixture.manifests[index].condition,
            global_start_inclusive: start,
            global_end_exclusive: start + attempt_count,
            attempt_count,
            attempts,
        }
    };
    let second_start = fixture.attempts_per_arm[0];
    ParsedAttemptLedger {
        attempt_index_sha256: sha256(&ledger),
        attempt_index_root_sha256: fold.iter().map(|byte| format!("{byte:02x}")).collect(),
        arms: [arm(0, 0), arm(1, second_start)],
    }
}

#[test]
fn proof_ledger_real_producer_orders_parse_complete_structure() {
    for (first, second) in [
        (EvaluationCondition::Generic, EvaluationCondition::Candidate),
        (EvaluationCondition::Candidate, EvaluationCondition::Generic),
    ] {
        let fixture = producer_fixture(first, second);
        assert_eq!(
            parse_native_attempt_ledger(&fixture.root, &fixture.binding()).unwrap(),
            expected_parsed(&fixture)
        );
    }
}

#[test]
fn proof_ledger_real_multi_attempt_pair_derives_exact_ranges() {
    let fixture = producer_fixture_with_attempts(
        EvaluationCondition::Generic,
        EvaluationCondition::Candidate,
        [2, 2],
    );

    assert_eq!(
        parse_native_attempt_ledger(&fixture.root, &fixture.binding()).unwrap(),
        expected_parsed(&fixture)
    );
}

#[test]
fn proof_ledger_rejects_structural_jsonl_mutations() {
    for case in [
        "missing-lf",
        "crlf",
        "blank-line",
        "duplicate-key",
        "unknown-field",
        "file-cap",
        "line-cap",
        "record-cap",
        "missing-terminal",
        "alternating-records",
        "whitespace",
        "key-order",
        "index-gap",
        "arm-index-gap",
        "second-arm-index-reset",
        "terminal-index",
        "ordinal-reversal",
        "post-arm-records",
        "arm-1-prefix-sha",
        "arm-1-prefix-root",
        "arm-2-full-sha",
        "arm-2-final-root",
        "manifest-count",
    ] {
        let mut fixture =
            producer_fixture(EvaluationCondition::Generic, EvaluationCondition::Candidate);
        let path = fixture.root.join("coordinator/attempt-index.jsonl");
        let ledger = std::fs::read(&path).unwrap();
        let mut lines = std::str::from_utf8(ledger.strip_suffix(b"\n").unwrap())
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut exact_bytes = None;
        let mut rewrite = true;
        let intended_error = match case {
            "missing-lf" => {
                exact_bytes = Some(lines.join("\n").into_bytes());
                "LF-framed JSONL"
            }
            "crlf" => {
                exact_bytes = Some(format!("{}\r\n", lines.join("\r\n")).into_bytes());
                "LF-framed JSONL"
            }
            "blank-line" => {
                lines.splice(0..0, [String::new(), String::new()]);
                "parse unique-key JSON"
            }
            "duplicate-key" => {
                lines[0].insert_str(1, "\"schemaVersion\":1,");
                "duplicate object key"
            }
            "unknown-field" => {
                lines[0].insert_str(1, "\"unknown\":null,");
                "schema validation failed"
            }
            "file-cap" => {
                let cap = MAX_ATTEMPTS_PER_ARM * 4 * (1024 * 1024 + 1);
                std::fs::OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_len(cap + 1)
                    .unwrap();
                rewrite = false;
                "exceeds its byte cap"
            }
            "line-cap" => {
                lines[0].insert_str(1, &format!("\"padding\":\"{}\",", "a".repeat(1024 * 1024)));
                "record exceeds its byte cap"
            }
            "record-cap" => {
                exact_bytes = Some(b"{}\n".repeat(9));
                "record count is invalid"
            }
            "missing-terminal" => {
                lines.pop();
                "record count is invalid"
            }
            "alternating-records" => {
                lines.swap(0, 1);
                "do not alternate"
            }
            "whitespace" => {
                lines[0].insert(1, ' ');
                "exact compact typed JSON"
            }
            "key-order" => {
                lines[0] = lines[0].replacen(
                    "{\"schemaVersion\":1,\"recordType\":\"request\"",
                    "{\"recordType\":\"request\",\"schemaVersion\":1",
                    1,
                );
                "exact compact typed JSON"
            }
            "index-gap" => {
                lines[0] =
                    lines[0].replacen("\"globalAttemptIndex\":0", "\"globalAttemptIndex\":1", 1);
                "identity or sequence"
            }
            "arm-index-gap" => {
                lines[0] = lines[0].replacen("\"armAttemptIndex\":0", "\"armAttemptIndex\":1", 1);
                "identity or sequence"
            }
            "second-arm-index-reset" => {
                lines[2] = lines[2].replacen("\"armAttemptIndex\":0", "\"armAttemptIndex\":1", 1);
                "identity or sequence"
            }
            "terminal-index" => {
                lines[1] =
                    lines[1].replacen("\"globalAttemptIndex\":0", "\"globalAttemptIndex\":1", 1);
                "identity or sequence"
            }
            "ordinal-reversal" => {
                lines[0] = lines[0].replacen("\"runOrdinal\":1", "\"runOrdinal\":2", 1);
                "missing an arm"
            }
            "post-arm-records" => {
                lines.extend_from_within(0..2);
                "after the second arm"
            }
            "arm-1-prefix-sha" => {
                fixture.manifests[0].broker_attempt_ledger_sha256 = "f".repeat(64);
                "summary is invalid"
            }
            "arm-1-prefix-root" => {
                fixture.manifests[0].attempt_index_root_sha256 = "f".repeat(64);
                "summary is invalid"
            }
            "arm-2-full-sha" => {
                fixture.manifests[1].broker_attempt_ledger_sha256 = "f".repeat(64);
                "summary is invalid"
            }
            "arm-2-final-root" => {
                fixture.manifests[1].attempt_index_root_sha256 = "f".repeat(64);
                "summary is invalid"
            }
            "manifest-count" => {
                fixture.manifests[0].provider_request_attempt_count = 2;
                "summary is invalid"
            }
            _ => unreachable!(),
        };
        if rewrite {
            let bytes =
                exact_bytes.unwrap_or_else(|| format!("{}\n", lines.join("\n")).into_bytes());
            std::fs::write(&path, bytes).unwrap();
        }
        let error = parse_native_attempt_ledger(&fixture.root, &fixture.binding())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(intended_error),
            "{case} reached the wrong rule: {error}"
        );
    }
}
