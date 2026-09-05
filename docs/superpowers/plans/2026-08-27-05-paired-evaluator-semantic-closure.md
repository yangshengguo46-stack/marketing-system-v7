> **RETIRED — RETIRED_NOT_PASSED (2026-09-05).** This evaluator/proof document is historical and is not executable. All commands, prerequisite gates and successor authorizations below are retired by the approved business-first cleanup; prior results and failures remain historical claims only. Do not resume Phase 0A, 06A, 06B, 07A, 07B or LH1 from this record. Retirement grants no business PASS, model qualification, spending, publication, customer-data access or customer-release authority. Current authority: `docs/superpowers/specs/2026-09-05-business-first-cleanup-design.md` and `docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md`.

# Paired Evaluator Semantic Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the five load-bearing semantic defects left by Plan 04 so the provider-free paired evaluator permits treatment effects, observes both arms' Skill use, cannot label mock evidence G2-eligible, binds the real additional context, and gives Replay truthful first-root request evidence.

**Architecture:** Keep Plan 04's proxy, App Server, broker, atomic ledger, isolated Home, and receipt machinery unchanged. Add explicit mock evidence typing; make per-arm business output and Skill-use evidence independent; then extract the already-reviewed live first-root canonicalization into one pure function reused by live and by two frozen Replay request fixtures. The plan remains provider-free: native synthetic App Server and loopback mocks only.

**Tech Stack:** Rust 1.95, Tokio, serde/serde_json, SHA-256, Codex App Server V2 typed protocol, existing `codex-ai-ip-domain`, `codex-ai-ip-runtime`, `codex-responses-api-proxy`, Cargo nextest through `just`, Bazel 9.0.0.

**Spec:** `docs/superpowers/specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md`

## Global Constraints

- Base commit is `47eda2c3239bfbf68b8d0158362c9b7d50df1bc4`; do not rewrite prior reviewed history.
- The root Thread remains the only Lead. Do not add a fixed agent roster, second orchestrator, product agent-management surface, UI, provider gateway, registry, billing, publishing, or persistence.
- Request bodies, response bodies, `ContentPackage` bodies, and private materials may exist only in bounded private processing. Ledger, manifests, receipts, and verification files contain commitments and typed metadata only.
- `providerMode=not-run`; authorized provider cost `0`; paid provider cost `0`; no API key, credential reader, real provider, real external endpoint, or external network.
- The two parked WP8 same-UID ancestor-swap findings remain parked and must stay explicit. This plan does not claim live/G2/G0-G2/`PASS_TO_PHASE_0B` readiness.
- Authority lines 19 and 46 require the two arms to share inputs/config/model/limits while allowing output bodies to differ. Authority line 1319 explicitly permits output and output-derived metric differences.
- Every behavior change follows TDD: focused behavioral RED, minimal implementation, identical focused GREEN. Environment/compile failures that do not reach the asserted behavior are not valid RED evidence.
- Final verification order is evaluator tests, proxy tests, lock update, required Bazel targets, lock check, diff check, then `just fmt` and scoped `just fix` last. Run no tests after final format/fix.

---

## File Structure

- `codex-rs/ai-ip-eval/src/model.rs` — typed manifest execution mode and request-evidence fields; G2 eligibility.
- `codex-rs/ai-ip-eval/src/evidence.rs` — one target-Skill observer used by both treatment arms.
- `codex-rs/ai-ip-eval/src/runner.rs` — live/mock per-arm validation, distinct output commitments, actual additional-context binding, shared request canonicalizer, Replay request proof.
- `codex-rs/ai-ip-eval/src/eval_tests.rs` — focused behavior and executable native/replay integration tests.
- `codex-rs/ai-ip-eval/src/tree_tests.rs` — target-Skill required/forbidden observer unit tests.
- `codex-rs/ai-ip-eval/tests/fixtures/replay-transcript.jsonl` — valid generic output package.
- `codex-rs/ai-ip-eval/tests/fixtures/replay-candidate-transcript.jsonl` — distinct valid candidate output plus canonical Skill read.
- `codex-rs/ai-ip-eval/tests/fixtures/replay-generic-request.json` — frozen generic first-root Responses request.
- `codex-rs/ai-ip-eval/tests/fixtures/replay-candidate-request.json` — same request plus exactly one canonical target-Skill treatment fragment.
- `codex-rs/ai-ip-eval/tests/fixtures/replay-fixture-set.json` — exact SHA-256 inventory including both request fixtures and changed transcripts.

---

### Task 1: Correct per-arm output, Skill-use, mock eligibility, and additional-context evidence

**Files:**
- Modify: `codex-rs/ai-ip-eval/src/model.rs`
- Modify: `codex-rs/ai-ip-eval/src/evidence.rs`
- Modify: `codex-rs/ai-ip-eval/src/runner.rs`
- Modify: `codex-rs/ai-ip-eval/src/eval_tests.rs`
- Modify: `codex-rs/ai-ip-eval/src/tree_tests.rs`
- Modify: `codex-rs/ai-ip-eval/tests/fixtures/replay-transcript.jsonl`
- Modify: `codex-rs/ai-ip-eval/tests/fixtures/replay-candidate-transcript.jsonl`
- Modify: `codex-rs/ai-ip-eval/tests/fixtures/replay-fixture-set.json`

**Interfaces:**
- Consumes: `ReplayCollector::finish(&HeldOutMissionCase) -> Result<CollectedReplay>`, `SkillUseTracker`, `RunManifest`, `ModeEvidence`, `codex_ai_ip_runtime::evaluation_context`, existing pre/post catalog evidence and receipt-manifest binding.
- Produces: `ExecutionMode::Mock`, `ModeEvidence::Mock`, `SkillUseExpectation`, observed Skill-use results for both arms, separate generic/candidate output commitments, and an actual `additionalContextSha256` commitment.

- [ ] **Step 1: Write failing tests for distinct valid arm outputs**

Add tests that validate each arm internally while permitting a treatment effect:

```rust
#[test]
fn paired_outputs_are_independently_valid_and_may_differ() {
    let result = run_synthetic_native_pair_with_distinct_valid_packages();
    assert!(result.is_ok());
    let generic = read_manifest(EvaluationCondition::Generic);
    let candidate = read_manifest(EvaluationCondition::Candidate);
    assert_ne!(generic.content_package_sha256, candidate.content_package_sha256);
    let verification = read_pair_verification();
    assert_eq!(verification["genericContentPackageSha256"], generic.content_package_sha256);
    assert_eq!(verification["candidateContentPackageSha256"], candidate.content_package_sha256);
    assert!(verification.get("contentPackageSha256").is_none());
}
```

Update the committed Replay candidate transcript so its valid `ContentPackage` differs from the generic package in at least `publishableContent.title` and `publishableContent.body`. Keep each transcript's `item/completed` text byte-identical to its own `turn/completed` final item.

- [ ] **Step 2: Run the focused distinct-output test and confirm behavioral RED**

Run:

```bash
just test -p codex-ai-ip-eval -E 'test(paired_outputs_are_independently_valid_and_may_differ) | test(replay_pair_executes_frozen_typed_pair_without_live_or_provider_surfaces)'
```

Expected: FAIL because `verify_live_arm_business_parity` and Replay currently reject different packages or because pair verification exposes only one package commitment. A fixture hash mismatch alone is not the accepted RED; update the inventory only after observing the semantic failure.

- [ ] **Step 3: Remove cross-arm output equality and bind both outputs independently**

Keep `ReplayCollector`'s same-arm item/turn byte equality. Remove cross-arm `ContentPackage` equality from live/mock and Replay parity. Change pair verification to exact separate fields:

```rust
#[derive(Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PairOutputCommitments {
    generic_content_package_sha256: String,
    candidate_content_package_sha256: String,
}
```

Build each manifest with the hash of its own validated package. In `write_live_pair_verification` compare each manifest only with its matching arm and emit both fields. Replay must do the same.

- [ ] **Step 4: Run the focused distinct-output tests and confirm GREEN**

Run the Step 2 command unchanged.

Expected: PASS; generic/candidate output hashes differ, both packages validate, and no public evidence contains either body.

- [ ] **Step 5: Write failing tests that observe generic target-Skill reads**

Add a policy-level observer test and native integration rejection:

```rust
#[test]
fn generic_successful_target_skill_read_is_observed_and_rejected() {
    let mut tracker = SkillUseTracker::new_forbidden(
        &absolute_generic_target_skill_path(),
        complete_skill_bytes(),
    ).unwrap();
    tracker.ingest(&successful_exact_skill_read()).unwrap();
    assert!(tracker.finish().unwrap_err().to_string().contains("generic arm read"));
}

#[test]
fn generic_target_skill_read_poisons_without_pair_receipt() {
    let result = run_native_pair_with_generic_skill_read();
    assert!(result.unwrap_err().to_string().contains("generic arm read"));
    assert!(poison_marker().is_file());
    assert!(!pair_receipt().exists());
}
```

- [ ] **Step 6: Run the generic-read tests and confirm behavioral RED**

Run:

```bash
just test -p codex-ai-ip-eval -E 'test(generic_successful_target_skill_read_is_observed_and_rejected) | test(generic_target_skill_read_poisons_without_pair_receipt)'
```

Expected: FAIL because the generic live arm constructs no tracker and the current `None` evidence is tautological.

- [ ] **Step 7: Implement one required/forbidden observer for both arms**

Add exact types in `evidence.rs`:

```rust
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SkillUseExpectation {
    RequiredBeforeFinal,
    Forbidden,
}

pub struct SkillUseOutcome {
    pub successful_read_observed: bool,
    pub evidence_sha256: Option<String>,
}
```

Provide `SkillUseTracker::new_required` for the existing candidate file and `SkillUseTracker::new_forbidden` for the absolute generic target path even when that file does not exist. Both trackers ingest every typed item. A successful exact-path, exit-0 read returning the complete Skill bytes causes `Forbidden` to fail; `RequiredBeforeFinal` still requires the read before the final package. `finish` returns `SkillUseOutcome` rather than manufacturing generic `None` without observation.

Create a tracker for both arms in `run_app_server_arm`. Replay must use the same policy rather than a command-substring special case. `ArmRunEvidence` retains `successful_read_observed` plus optional body-free evidence hash; parity requires generic `false`, candidate `true`.

- [ ] **Step 8: Run generic and candidate Skill-use tests and confirm GREEN**

Run:

```bash
just test -p codex-ai-ip-eval -E 'test(generic_successful_target_skill_read_is_observed_and_rejected) | test(generic_target_skill_read_poisons_without_pair_receipt) | test(candidate_skill_use_requires_the_exact_complete_skill_before_final_package)'
```

Expected: PASS.

- [ ] **Step 9: Write failing tests for mock G2 eligibility and actual additional-context binding**

```rust
#[test]
fn local_mock_manifests_are_typed_mock_and_never_g2_eligible() {
    let manifests = run_and_read_native_mock_manifests();
    assert!(manifests.iter().all(|manifest| manifest.execution_mode == ExecutionMode::Mock));
    assert!(manifests.iter().all(|manifest| matches!(manifest.mode_evidence, ModeEvidence::Mock { .. })));
    assert!(manifests.iter().all(|manifest| !manifest.is_g2_eligible()));
}

#[test]
fn mock_manifest_binds_actual_additional_context_not_turn_request() {
    let manifest = run_and_read_native_mock_manifests().remove(0);
    let mission = frozen_mission();
    assert_eq!(
        manifest.additional_context_sha256,
        sha256(codex_ai_ip_runtime::evaluation_context(&mission).unwrap().as_bytes())
    );
    assert_ne!(manifest.additional_context_sha256, manifest.turn_start_request_sha256);
}
```

- [ ] **Step 10: Run mock-mode/additional-context tests and confirm behavioral RED**

Run:

```bash
just test -p codex-ai-ip-eval -E 'test(local_mock_manifests_are_typed_mock_and_never_g2_eligible) | test(mock_manifest_binds_actual_additional_context_not_turn_request)'
```

Expected: FAIL because native loopback manifests are `Live/Live`, `is_g2_eligible()` is true, and `additional_context_sha256` equals the turn-start request artifact.

- [ ] **Step 11: Add explicit Mock typing and freeze the actual additional context**

Extend the exact typed surface:

```rust
pub enum ExecutionMode {
    Replay,
    Mock,
    Live,
}

pub enum ModeEvidence {
    Replay { fixture_set_sha256: String },
    Mock {
        provider_mode: MockProviderMode,
        synthetic_fixture_sha256: String,
        arm_order_commitment: String,
    },
    Live { /* existing exact fields unchanged */ },
}

pub enum MockProviderMode {
    NotRun,
}
```

`validate_execution_mode` accepts only matching Replay/Mock/Live pairs. `is_g2_eligible()` remains true only for `ExecutionMode::Live` with `ModeEvidence::Live`; Mock and Replay are always false.

During live-context freezing for the local mock boundary, write owner-only `additional-context.txt` from `evaluation_context(&mission)` and add the exact `additionalContext` frozen artifact. Include it in required artifact commitments and boundary rehashes. Build local native manifests as `Mock/Mock` and set `additional_context_sha256` from `artifact_sha("additionalContext")`. Do not change the frozen 180-second deadline or either parked WP8 blocker.

- [ ] **Step 12: Run all Task 1 focused tests and confirm GREEN**

Run:

```bash
just test -p codex-ai-ip-eval -E 'test(paired_outputs_are_independently_valid_and_may_differ) | test(replay_pair_executes_frozen_typed_pair_without_live_or_provider_surfaces) | test(generic_successful_target_skill_read_is_observed_and_rejected) | test(generic_target_skill_read_poisons_without_pair_receipt) | test(local_mock_manifests_are_typed_mock_and_never_g2_eligible) | test(mock_manifest_binds_actual_additional_context_not_turn_request)'
```

Expected: PASS with no provider, credential, external network, G2, or live-readiness claim.

- [ ] **Step 13: Run Task 1 package regression**

Run:

```bash
just test -p codex-ai-ip-eval
```

Expected: all evaluator tests PASS.

- [ ] **Step 14: Commit Task 1**

```bash
git add codex-rs/ai-ip-eval/src/model.rs \
  codex-rs/ai-ip-eval/src/evidence.rs \
  codex-rs/ai-ip-eval/src/runner.rs \
  codex-rs/ai-ip-eval/src/eval_tests.rs \
  codex-rs/ai-ip-eval/src/tree_tests.rs \
  codex-rs/ai-ip-eval/tests/fixtures/replay-transcript.jsonl \
  codex-rs/ai-ip-eval/tests/fixtures/replay-candidate-transcript.jsonl \
  codex-rs/ai-ip-eval/tests/fixtures/replay-fixture-set.json
git commit -m "fix(ai-ip-eval): preserve paired treatment effects"
```

---

### Task 2: Give Replay truthful first-root request evidence through the shared canonicalizer

**Files:**
- Modify: `codex-rs/ai-ip-eval/src/runner.rs`
- Modify: `codex-rs/ai-ip-eval/src/eval_tests.rs`
- Create: `codex-rs/ai-ip-eval/tests/fixtures/replay-generic-request.json`
- Create: `codex-rs/ai-ip-eval/tests/fixtures/replay-candidate-request.json`
- Modify: `codex-rs/ai-ip-eval/tests/fixtures/replay-fixture-set.json`

**Interfaces:**
- Consumes: reviewed `PairRequestInspector` normalization rules, `TransformedRequestEvidence`, Task 1's distinct per-arm manifest/output fields and Replay execution.
- Produces: `canonicalize_first_root_request(...) -> Result<TransformedRequestEvidence>` shared by live/mock and Replay; frozen generic/candidate request fixtures whose evidence truthfully populates all four request-proof fields.

- [ ] **Step 1: Write failing tests for truthful Replay request evidence**

Add a test that recomputes both fixture bodies through the shared semantics and rejects the existing unrelated substitutions:

```rust
#[test]
fn replay_request_evidence_is_derived_from_frozen_request_fixtures() {
    let result = execute_frozen_replay_pair();
    assert!(result.is_ok());
    let generic = read_replay_manifest(EvaluationCondition::Generic);
    let candidate = read_replay_manifest(EvaluationCondition::Candidate);
    let expected = canonicalize_replay_request_fixtures();
    assert_eq!(generic.first_root_provider_request_commitment, expected.generic_raw);
    assert_eq!(candidate.first_root_provider_request_commitment, expected.candidate_raw);
    assert_eq!(generic.normalized_first_root_base_commitment, expected.normalized_base);
    assert_eq!(candidate.normalized_first_root_base_commitment, expected.normalized_base);
    assert_eq!(generic.first_root_treatment_diff_commitment, None);
    assert_eq!(candidate.first_root_treatment_diff_commitment, Some(expected.treatment));
    assert_ne!(generic.first_root_provider_request_commitment, generic.app_server_transcript_sha256);
}
```

Add a mutation test that changes `model`, `tools`, or a non-treatment input item in exactly one Replay request fixture and expects failure before manifests are written.

- [ ] **Step 2: Run the Replay request tests and confirm behavioral RED**

Run:

```bash
just test -p codex-ai-ip-eval -E 'test(replay_request_evidence_is_derived_from_frozen_request_fixtures) | test(replay_request_fixture_drift_fails_before_manifests)'
```

Expected: FAIL because Replay currently uses transcript/package/Skill-use hashes for request-proof fields and the fixture set has no request bodies.

- [ ] **Step 3: Extract one pure request canonicalizer**

Refactor without changing the reviewed live behavior:

```rust
struct RequestCanonicalizationContext<'a> {
    condition: EvaluationCondition,
    known_thread_ids: &'a HashSet<String>,
    generic_codex_home: &'a Path,
    candidate_codex_home: &'a Path,
    deadline_rfc3339: &'a str,
}

fn canonicalize_first_root_request(
    body: &serde_json::Value,
    context: &RequestCanonicalizationContext<'_>,
) -> Result<codex_responses_api_proxy::TransformedRequestEvidence>;
```

The function preserves Plan 04 semantics exactly: hash raw JSON, normalize only committed thread IDs/Home prefixes/deadline, require zero target fragments for generic and exactly one for candidate, remove only that candidate fragment, and hash normalized/base/treatment evidence. `PairRequestInspector::inspect_inner` calls this pure function and keeps only pair ordering/state/poison responsibilities.

- [ ] **Step 4: Add exact frozen Replay request fixtures**

Create two strict JSON request bodies. They must have the same model, tools, metadata, non-treatment input, dynamic-ID shape, and deadline. The candidate adds exactly the canonical `canonical_target_skill_treatment` value; the generic has none. Use `$GENERIC_CODEX_HOME` and `$CANDIDATE_CODEX_HOME` tokens only in path positions normalized by the shared function.

Compute the exact digests first:

```bash
shasum -a 256 codex-rs/ai-ip-eval/tests/fixtures/replay-generic-request.json
shasum -a 256 codex-rs/ai-ip-eval/tests/fixtures/replay-candidate-request.json
```

Add `genericRequest` and `candidateRequest` entries to `replay-fixture-set.json` using the printed 64-character lowercase first column as each `sha256`. Parse the finished manifest with `serde_json` in the focused test and assert both frozen references equal freshly recomputed file digests before running Replay.

- [ ] **Step 5: Populate Replay manifests from actual request evidence**

`run_replay_pair` must read both fixtures through the already frozen byte inventory, materialize only the committed Home tokens, call `canonicalize_first_root_request` for each condition, and require equal normalized bases. Pass the actual evidence into `build_replay_manifest`. Remove all substitutions of transcript, package, or Skill-use hashes into request-proof fields. Replay verification binds both raw/normalized commitments, one shared base, and the candidate treatment commitment.

- [ ] **Step 6: Run Replay request tests and confirm GREEN**

Run the Step 2 command unchanged.

Expected: PASS; unauthorized drift fails before any manifest, and no App Server/proxy/broker/provider/credential/network surface is constructed.

- [ ] **Step 7: Run all semantic-closure focused tests**

Run:

```bash
just test -p codex-ai-ip-eval -E 'test(paired_outputs_are_independently_valid_and_may_differ) | test(generic_target_skill_read_poisons_without_pair_receipt) | test(local_mock_manifests_are_typed_mock_and_never_g2_eligible) | test(mock_manifest_binds_actual_additional_context_not_turn_request) | test(replay_request_evidence_is_derived_from_frozen_request_fixtures) | test(replay_request_fixture_drift_fails_before_manifests)'
```

Expected: PASS.

- [ ] **Step 8: Run required verification before final formatting**

With the pinned offline PATH/Cargo/Bazel environment recorded in the Plan 04 Task 4 report, run in order:

```bash
just test -p codex-ai-ip-eval
just test -p codex-responses-api-proxy
just bazel-lock-update
bazel test //codex-rs/ai-ip-eval:ai-ip-eval-unit-tests //codex-rs/ai-ip-eval:codex-ai-ip-eval-bin-unit-tests
just bazel-lock-check
git diff --check 47eda2c3239bfbf68b8d0158362c9b7d50df1bc4..HEAD
```

Expected: evaluator/proxy/Bazel all PASS, lock update produces no diff, lock check and diff check exit 0.

- [ ] **Step 9: Run final format/fix boundary**

```bash
just fmt
just fix -p codex-ai-ip-eval
```

Expected: both exit 0. Run no test, build, formatter, fixer, or code generator afterward.

- [ ] **Step 10: Commit Task 2**

```bash
git add codex-rs/ai-ip-eval/src/runner.rs \
  codex-rs/ai-ip-eval/src/eval_tests.rs \
  codex-rs/ai-ip-eval/tests/fixtures/replay-generic-request.json \
  codex-rs/ai-ip-eval/tests/fixtures/replay-candidate-request.json \
  codex-rs/ai-ip-eval/tests/fixtures/replay-fixture-set.json
git commit -m "fix(ai-ip-eval): bind replay request evidence"
```

---

## Final Acceptance

- Both arms may emit different valid `ContentPackage` bodies; each arm's item and terminal copies remain byte-identical internally.
- Generic target-Skill non-use and candidate target-Skill use are both observed from typed items, not inferred from installation state.
- Native loopback manifests are explicitly Mock and never G2-eligible; Replay remains non-G2; no real Live manifest is produced by this plan.
- `additionalContextSha256` is the SHA-256 of `evaluation_context(mission)` bytes and differs from the turn-start request commitment.
- Replay request-proof fields come only from frozen request fixtures processed by the same canonicalizer used by the live/mock inspector.
- Pair verification contains separate output commitments and truthful request commitments without bodies.
- The two parked WP8 ancestor-swap findings remain explicit; real/paid execution stays forbidden.
- No deferred Minor is silently claimed fixed.

## Self-Review

- **Spec coverage:** Tasks map to authority lines 19/46 (same treatment inputs), 742 (manifest fields), 1258 (cross-arm input equality), 1317-1319 (same prompt/context/request with outputs permitted to differ), and the Plan 04 mock-only boundary.
- **Placeholder scan:** Every fixture digest is produced by an explicit command and checked by a focused Rust assertion. The plan contains no unresolved marker or unspecified error-handling step.
- **Type consistency:** Task 1 produces Mock mode, both-arm Skill-use outcomes, and separate output commitments. Task 2 consumes those unchanged and only replaces false Replay request values with actual shared-canonicalizer evidence.
