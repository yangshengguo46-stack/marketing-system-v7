> **RETIRED — RETIRED_NOT_PASSED (2026-09-05).** This evaluator/proof document is historical and is not executable. All commands, prerequisite gates and successor authorizations below are retired by the approved business-first cleanup; prior results and failures remain historical claims only. Do not resume Phase 0A, 06A, 06B, 07A, 07B or LH1 from this record. Retirement grants no business PASS, model qualification, spending, publication, customer-data access or customer-release authority. Current authority: `docs/superpowers/specs/2026-09-05-business-first-cleanup-design.md` and `docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md`.

# AI IP Blind Review and Score Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn a sealed generic/candidate pair into three condition-hidden reviewer bundles and one deterministic private PASS/ITERATE/INVALID decision without calling a provider or exposing private bodies publicly.

**Architecture:** Preserve Plan 04/05's atomic pair and immutable manifests, but add one manifest-bound post-processing archive containing the typed evidence that blind verification must recompute. Compile frozen rubric/schema bytes into the evaluator, keep reviewer-visible rubric separate from coordinator-only decision thresholds, resolve all CLI paths from the injected frozen context, bind every private mapping in a blind-pack receipt, and score only reviewer-specific mappings. This is Work Package 6's first independent boundary; cost/report/verifiers are 06B and retention closeout is 06C.

**Tech Stack:** Rust 1.95, Clap 4, serde/serde_json, `jsonschema` 0.49.9 with external resolution disabled, `serde_json_canonicalizer` 0.3.2, SHA-256, rand 0.9 (`OsRng`, `StdRng`), Codex App Server V2 typed protocol, existing `codex-ai-ip-domain`/`codex-ai-ip-runtime`, Cargo nextest through `just`, Bazel 9.0.0.

**Spec:** `docs/superpowers/specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md`

## Global Constraints

- Base commit is `eea75f7771bcfafea5bb9107d329a10d5b4b6429`; prior reviewed history is immutable.
- `providerMode=not-run`; authorized provider cost `0`; paid provider cost `0`; no API key, credential reader, real provider, external endpoint, or external network.
- Business quality is primary. Cost/token/latency inside an approved cap cannot veto business PASS; this child does not calculate cost.
- Exactly three independent human reviewers are required; at least two must have matching, canonically bound private declarations with `experiencedOperatorOrDirector=true`.
- PASS requires candidate preference by at least `2/3`, median paired score delta at least `3`, candidate median total at least `18`, candidate ready-for-human-review from at least `2/3`, and zero candidate severe flags.
- A valid proof with any candidate `fabricatedFactualClaim=true` is `ITERATE_SMALLEST_LEAD_CHANGE`, never `INVALID_PROOF`. Unsafe/unparseable authority inputs stop with no output; a trusted frozen pair/blind receipt followed by invalid review evidence produces immutable `INVALID_PROOF`.
- Reviewer-visible files never contain candidate/generic condition labels, decision thresholds, mapping, seed, Home, Skill, token, cost, duration, thread/response IDs, logs, or treatment-only markers.
- All generated reviewer, mapping, seed, reviews, receipt, archive, and inventory paths resolve below canonical `privateRoot`; no Work Package 6A private output is written to Git or an arbitrary external root.
- The authoritative WP7/WP8 CLI forms are immutable: `blind-pack --reviewer-root reviewer --mapping-dir coordinator/mappings [seed args]` and `score --mapping-dir coordinator/mappings --reviews-dir reviews --output coordinator/decision.private.json`; `run_frozen_eval.py` injects `--frozen-run-context`.
- Live/Mock seeds come from OS CSPRNG. Replay accepts exactly three nonempty distinct explicit seed strings and derives 32 bytes with domain-separated SHA-256. All-three-identical A/B orientation is resampled for Live/Mock and rejected for Replay; two matching orientations are valid.
- Every production behavior follows RED→GREEN. The first RED in every task must compile and execute real code; it may fail by missing runtime file, rejected real CLI argv, missing output, or wrong result, but not by an unresolved Rust symbol.
- Keep each non-mechanical implementation stage below 500 changed lines and every total stage below 800 changed lines, excluding generated lockfile churn. Keep each new production module below 500 lines excluding tests. Static schema fixtures are mechanical; every complex Rust boundary gets its own commit and review gate.
- The parked Work Package 8 same-UID managed-case/eval-tree ancestor-swap and executable/kernel-loaded-vnode ancestor-swap findings remain blocked; 06A cannot weaken or relabel them.
- Final order: evaluator tests, proxy tests, Bazel lock update, required Bazel tests/builds, lock check, diff/scope checks, then `just fmt` and `just fix -p codex-ai-ip-eval` last. No tests after final format/fix.

---

## File Structure

- `ai-ip-evals/rubrics/content-package-blind-review.json` — reviewer-visible six-dimension rubric only.
- `ai-ip-evals/rubrics/blind-review-decision-policy.json` — coordinator-only exact decision thresholds.
- `ai-ip-evals/rubrics/reviewer-submission.schema.json` — exact private review submission schema.
- `ai-ip-evals/rubrics/BUILD.bazel` — evaluator-only rubric filegroup.
- `ai-ip-evals/schemas/*.schema.json` — strict Work Package 6 schemas, including separate native/replay attestation and frozen-context branches.
- `ai-ip-evals/schemas/BUILD.bazel` — evaluator-only explicit schema filegroup.
- `codex-rs/ai-ip-eval/src/contracts.rs` — `include_bytes!` production contract loader and semantic validators.
- `codex-rs/ai-ip-eval/src/jcs.rs` — duplicate-key-rejecting RFC 8785 canonicalization and domain-separated commitment helper.
- `codex-rs/ai-ip-eval/src/proof_archive.rs` — typed, bounded per-arm post-processing archive and verifier.
- `codex-rs/ai-ip-eval/src/private_inventory.rs` — append+fsync hash-chain inventory and pair marker.
- `codex-rs/ai-ip-eval/src/secure_fs.rs` — contained owner-only create/read helpers for Unix and Windows.
- `codex-rs/ai-ip-eval/src/blind.rs` / `blind_bundle.rs` — pair recomputation, marker scan, seeds, mappings, bundles, reviews drop, receipt.
- `codex-rs/ai-ip-eval/src/score.rs` — exact submission validation, unblinding, medians, and immutable decision.
- `codex-rs/ai-ip-eval/src/*_tests.rs` — focused unit tests kept out of production modules.
- `codex-rs/ai-ip-eval/tests/blind_cli.rs` / `score_cli.rs` — real binary/Clap integration tests using `codex_utils_cargo_bin::cargo_bin`.
- `codex-rs/ai-ip-eval/tests/fixtures/contracts/**` — canonical positive and one-field-negative synthetic schema fixtures.
- `codex-rs/ai-ip-eval/tests/fixtures/blind/**` — synthetic pair/reviewer mutation fixtures only.
- `codex-rs/ai-ip-eval/src/model.rs`, `lib.rs`, `runner.rs`, `BUILD.bazel` — narrow wiring, archive hook, manifest commitment, compile/runfile data.
- `docs/architecture/codex-fork-patch-ledger.md` — append-only completion record after implementation and review.

---

### Task 1: Freeze executable reviewer, attestation, and context contracts

**Files:**
- Create: `ai-ip-evals/rubrics/content-package-blind-review.json`
- Create: `ai-ip-evals/rubrics/blind-review-decision-policy.json`
- Create: `ai-ip-evals/rubrics/reviewer-submission.schema.json`
- Create: `ai-ip-evals/rubrics/BUILD.bazel`
- Create: `ai-ip-evals/schemas/held-out-attestation.schema.json`
- Create: `ai-ip-evals/schemas/frozen-run-context.schema.json`
- Create: `ai-ip-evals/schemas/BUILD.bazel`
- Create: `codex-rs/ai-ip-eval/src/contracts.rs`
- Create: `codex-rs/ai-ip-eval/src/contracts_tests.rs`
- Create: `codex-rs/ai-ip-eval/src/jcs.rs`
- Create: `codex-rs/ai-ip-eval/src/jcs_tests.rs`
- Create: `codex-rs/ai-ip-eval/tests/fixtures/contracts/06a/**`
- Modify: `codex-rs/Cargo.toml`
- Modify: `codex-rs/ai-ip-eval/Cargo.toml`
- Modify: `codex-rs/Cargo.lock`
- Modify: `MODULE.bazel.lock`
- Modify: `codex-rs/ai-ip-eval/src/lib.rs`
- Modify: `codex-rs/ai-ip-eval/src/runner.rs`
- Modify: `codex-rs/ai-ip-eval/src/eval_tests.rs`
- Modify: `codex-rs/ai-ip-eval/BUILD.bazel`
- Modify: `codex-rs/ai-ip-eval/tests/fixtures/replay-attestation.json`
- Modify: `codex-rs/ai-ip-eval/tests/fixtures/replay-fixture-set.json`

**Interfaces:**
- Consumes: Work Package 6 Contracts 1–3; serde/serde_json; offline Draft 2020-12 validation; RFC 8785; test-only `find_resource!`.
- Produces: `FrozenContracts`, `ReviewerSubmission`, `NativeHeldOutAttestation`, `ReplayReviewAttestation`, `validate_reviewer_submission`, and compile-time embedded rubric/schema bytes.

- [ ] **Step 1: Write runtime RED tests against missing assets**

Use `find_resource!` only in `contracts_tests.rs`; parse with `serde_json::Value` so the test compiles before production loaders exist:

```rust
#[test]
fn reviewer_contract_assets_expose_exact_behavior() {
    let path = codex_utils_cargo_bin::find_resource!(
        "../../ai-ip-evals/rubrics/content-package-blind-review.json"
    );
    let rubric = std::fs::read(path).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&rubric).unwrap();
    assert_eq!(value["dimensions"].as_array().unwrap().len(), 6);
    assert!(value.get("pass").is_none());
    assert!(!String::from_utf8(rubric).unwrap().to_ascii_lowercase().contains("candidate"));
}
```

Add only resource-presence/shape tests in this first RED. They use existing serde/serde_json and contain no references to either new dependency or a missing Rust symbol.

- [ ] **Step 2: Run the exact RED**

```bash
just test -p codex-ai-ip-eval -E 'test(reviewer_contract_assets_expose_exact_behavior)'
```

Expected: tests compile and execute, then FAIL because the resource files do not exist.

- [ ] **Step 3: Add dependency-only plumbing, compile stubs, and behavioral validator/JCS REDs**

Add the exact workspace/package Cargo dependencies and resolve `Cargo.lock`; this is build plumbing, not production behavior. Write table-driven positive/unknown/missing/wrong-type tests for submission, native/replay attestation, and native/replay context, plus RFC 8785 vectors. After the tests are authored, add only enough compile scaffolding for the RED to execute: `FrozenContracts::load()` and the `jcs.rs` commitment entry point return an unconditional typed `NotImplemented` error and have no success branch. Run the tests and observe behavioral failure from those stubs, not an unresolved symbol or missing crate.

```bash
just bazel-lock-update
just test -p codex-ai-ip-eval -E 'test(executable_06a_schemas_accept_only_the_frozen_shapes) | test(rfc_8785_commitments_match_official_vectors)'
```

- [ ] **Step 4: Create condition-free reviewer rubric and private decision policy**

`content-package-blind-review.json` contains only `schemaVersion`, scale `0..4`, six full Chinese definitions, four severe-flag definitions, and reviewer instructions. Its dimension IDs are:

```text
businessOutcomeClarity
subjectAudienceActionFit
strategicJudgment
publishableUsability
evidenceIntegrity
measurementUsefulness
```

Its severe flags are:

```text
fabricatedFactualClaim
wrongSubjectOrDesiredAction
notActuallyUsable
rightsOrPrivacyViolation
```

The reviewer-visible file contains neither `candidate` nor `generic` in any case. Coordinator-only `blind-review-decision-policy.json` contains exact values:

```json
{
  "schemaVersion": 1,
  "candidatePreferenceCount": 2,
  "medianPairedDelta": 3,
  "medianCandidateTotal": 18,
  "candidateReadyForHumanReviewCount": 2,
  "candidateSevereFailureCount": 0
}
```

- [ ] **Step 5: Freeze nested submission and reviewer declaration semantics**

Every object uses `additionalProperties:false`; every Rust struct uses `#[serde(deny_unknown_fields, rename_all="camelCase")]`. The exact review submission is:

```rust
struct ReviewerSubmission {
    schema_version: u32,
    reviewer_id: String,
    review_bundle_sha256: String,
    rubric_sha256: String,
    qualification: ReviewerQualificationBinding,
    preferred: PreferredArm,
    arms: ReviewerArms,
    signed_at: String,
    signed_payload_sha256: String,
    signature_evidence: String,
    signature_evidence_sha256: String,
}

struct ReviewerQualificationBinding {
    qualification_class: String,
    experienced_operator_or_director: bool,
    attestation_signed_payload_sha256: String,
    attestation_signature_evidence_sha256: String,
}

struct ArmReview {
    scores: DimensionScores,
    ready_for_human_review: bool,
    reasons: Vec<String>,
    severe_flags: SevereFlags,
}
```

`reviewerId` matches `^[A-Za-z0-9][A-Za-z0-9._-]{2,63}$`; digests are lowercase 64-hex; `preferred=A|B|tie`; each score is integer `0..4`; reasons are `1..3` nonempty strings, each at most 500 Unicode scalar values; timestamps parse RFC3339.

Attestation reviewer declarations have exact fields:

```rust
struct ReviewerDeclaration {
    reviewer_id: String,
    qualification_class: String,
    experienced_operator_or_director: bool,
    declared_at: String,
    signed_payload_sha256: String,
    signature_evidence: String,
    signature_evidence_sha256: String,
}
```

`signedPayloadSha256` is recomputed from RFC 8785/JCS bytes of the containing payload without `signedPayloadSha256` and `signatureEvidenceSha256` but including private `signatureEvidence`, prefixed by domain `AI-IP-REVIEWER-QUALIFICATION-V1\0` for attestation and `AI-IP-REVIEW-SUBMISSION-V1\0` for review. `jcs.rs` first parses raw JSON through a recursive serde visitor that rejects duplicate object keys at every depth, then calls `serde_json_canonicalizer::to_vec`; NaN, infinity, invalid UTF-8/surrogates, and duplicate keys fail before hashing. Tests include the RFC 8785 Appendix B numeric samples plus Unicode, UTF-16 property ordering, escaping, and equivalent-whitespace vectors, and assert the exact canonical bytes and digest. `signatureEvidenceSha256` must equal SHA-256 of the exact UTF-8 `signatureEvidence`; for native evidence the attestation copy is frozen before provider use. This is an integrity binding to private human-signature evidence, not a claim that SHA-256 authenticates identity. Score requires attestation/submission reviewer ID sets equal, qualification fields equal, and both payload/evidence commitments to recompute exactly; copying a valid commitment then flipping any covered field is invalid.

- [ ] **Step 6: Freeze native/replay attestation and frozen-context branches**

`held-out-attestation.schema.json` is `oneOf` two exact discriminated shapes:

```text
native/live:
schemaVersion,executionMode,providerMode,candidateSha,candidateFrozenAt,
caseSelectedAt,caseSha256,sourceMaterialsSha256,privateRoot,caseClass,
notOneOfFiveFrozenClasses,materialAuthorizationScope,providerDisclosure,
providerRole,targetProviderModelEvidenceCommitment,reviewers,approvalId,
approvedTotalFen,approvedPerRunFen,maxProviderRequestAttemptsPerRun,
maxTotalTokensPerRun,maxElapsedSecondsPerRun,maxOutputTokensPerRequest,
signedAt,retentionDeadline,providerBudgetEvidenceSha256,rateCardSha256,
billingPolicyCommitment,fxPolicySha256,rateCurrency,rateEffectiveAt

replay:
schemaVersion,executionMode="replay",providerMode="not-run",
paidProviderCostFen=0,syntheticOnly=true,pairId,caseSha256,
sourceMaterialsSha256,reviewers,signedAt
```

`frozen-run-context.schema.json` is `oneOf` the existing exact native and Replay `FrozenRunContext` wire shapes. Add `postprocessEvidenceIndexSha256` to each arm's later manifest, not to frozen context. Canonical fixtures exercise every nested field, and single-field fixtures cover unknown, missing, wrong enum/type/pattern/range, duplicate reviewer ID, fewer/more than three reviewers, and copied commitment with flipped experience/class/ID.

Every schema declares `"$schema":"https://json-schema.org/draft/2020-12/schema"` and contains no external `$ref`. Add `jsonschema = { version = "0.49.9", default-features = false }` and `serde_json_canonicalizer = "0.3.2"` to workspace dependencies, then consume both through `workspace = true` in `codex-ai-ip-eval`. `FrozenContracts` parses each embedded schema, requires `jsonschema::draft202012::meta::validate` success, builds it with `jsonschema::draft202012::options().should_validate_formats(true).build`, and uses the resulting validator for every positive and negative fixture before typed serde/semantic validation. No validator path may resolve network or filesystem references.

Remove `review1`, `review2`, and `review3` from `freeze-replay-context`'s exact required fixture roles. Define Replay `pairId` as SHA-256 over domain `AI-IP-REPLAY-PAIR-V2\0`, `forkSha`, and the canonical ordered `{name,sha256}` commitments for exactly `case`, `genericRequest`, `candidateRequest`, `genericTranscript`, `candidateTranscript`, and `leadSkill`; the attestation and review submissions are not pair-ID inputs. Keep `attestation` as a required frozen reference, parse it through `ReplayReviewAttestation`, require its `pairId` to equal the recomputed value, and compute `sourceMaterialsSha256` from the declared case materials rather than from fixture-set bytes. Update `runner.rs`, its Replay tests, attestation, and fixture-set in this task so the new typed parser is live immediately and no fixture hash depends on a value that itself includes that fixture hash.

- [ ] **Step 7: Embed production assets correctly and wire Bazel**

Production code uses compile-time bytes, never `find_resource!`:

```rust
const REVIEW_RUBRIC_BYTES: &[u8] =
    include_bytes!("../../../ai-ip-evals/rubrics/content-package-blind-review.json");
const DECISION_POLICY_BYTES: &[u8] =
    include_bytes!("../../../ai-ip-evals/rubrics/blind-review-decision-policy.json");
```

`BUILD.bazel` adds both filegroups to library `compile_data` and retains test runfiles:

```starlark
compile_data = [
    "//ai-ip-evals/rubrics:rubrics",
    "//ai-ip-evals/schemas:schemas",
],
test_data_extra = glob(["tests/fixtures/**"]) + [
    "//ai-ip-assets/skills/deliver-ai-ip-content-package:skill",
    "//ai-ip-evals/rubrics:rubrics",
    "//ai-ip-evals/schemas:schemas",
]
```

Do not move `codex-utils-cargo-bin` from dev-dependencies. Run `just bazel-lock-update` after Cargo resolves the two exact dependencies so the Task 1 Bazel test uses the committed lock state.

- [ ] **Step 8: Run GREEN and commit exact files**

```bash
just test -p codex-ai-ip-eval -E 'test(reviewer_contract_assets_expose_exact_behavior) | test(executable_06a_schemas_accept_only_the_frozen_shapes) | test(rfc_8785_commitments_match_official_vectors)'
just bazel-lock-check
bazel build //ai-ip-evals/rubrics:rubrics //ai-ip-evals/schemas:schemas
bazel test //codex-rs/ai-ip-eval:ai-ip-eval-unit-tests
git add ai-ip-evals/rubrics/content-package-blind-review.json \
  ai-ip-evals/rubrics/blind-review-decision-policy.json \
  ai-ip-evals/rubrics/reviewer-submission.schema.json ai-ip-evals/rubrics/BUILD.bazel \
  ai-ip-evals/schemas/held-out-attestation.schema.json \
  ai-ip-evals/schemas/frozen-run-context.schema.json ai-ip-evals/schemas/BUILD.bazel \
  codex-rs/Cargo.toml codex-rs/ai-ip-eval/Cargo.toml codex-rs/Cargo.lock \
  MODULE.bazel.lock codex-rs/ai-ip-eval/src/contracts.rs \
  codex-rs/ai-ip-eval/src/contracts_tests.rs codex-rs/ai-ip-eval/src/jcs.rs \
  codex-rs/ai-ip-eval/src/jcs_tests.rs codex-rs/ai-ip-eval/src/lib.rs \
  codex-rs/ai-ip-eval/src/runner.rs codex-rs/ai-ip-eval/src/eval_tests.rs \
  codex-rs/ai-ip-eval/BUILD.bazel \
  codex-rs/ai-ip-eval/tests/fixtures/contracts/06a/canonical-review.json \
  codex-rs/ai-ip-eval/tests/fixtures/contracts/06a/canonical-native-attestation.json \
  codex-rs/ai-ip-eval/tests/fixtures/contracts/06a/canonical-replay-attestation.json \
  codex-rs/ai-ip-eval/tests/fixtures/contracts/06a/canonical-native-context.json \
  codex-rs/ai-ip-eval/tests/fixtures/contracts/06a/canonical-replay-context.json \
  codex-rs/ai-ip-eval/tests/fixtures/contracts/06a/negative-cases.json \
  codex-rs/ai-ip-eval/tests/fixtures/replay-attestation.json \
  codex-rs/ai-ip-eval/tests/fixtures/replay-fixture-set.json
git commit -m "test(ai-ip-eval): freeze executable blind review contracts"
```

Expected: Cargo/Bazel pass and production resources compile without a runtime resolver.

---

### Task 2: Freeze remaining Work Package 6 schema assets without executing them

**Files:**
- Create: `ai-ip-evals/schemas/cost-receipt.schema.json`
- Create: `ai-ip-evals/schemas/business-report.schema.json`
- Create: `ai-ip-evals/schemas/report-index.schema.json`
- Create: `ai-ip-evals/schemas/attempt-index.schema.json`
- Create: `ai-ip-evals/schemas/verification.schema.json`
- Create: `ai-ip-evals/schemas/retention-closeout.schema.json`
- Modify: `ai-ip-evals/schemas/BUILD.bazel`
- Create: `codex-rs/ai-ip-eval/tests/fixtures/contracts/later/**`
- Modify: `codex-rs/ai-ip-eval/src/contracts_tests.rs`

**Interfaces:**
- Consumes: Work Package 6 Contract 3's exact filegroup obligation and Contracts 4–7's static wire definitions.
- Produces: strict, self-validating later-only schemas and positive/unknown/missing/wrong-type fixtures; no cost/report/publish/retention command behavior.

- [ ] **Step 1: Write runtime RED schema-fixture tests**

For each of the six schema names, load schema and fixtures through test-only `find_resource!`; assert the canonical fixture has exactly the schema's required top-level/nested fields and each one-field mutation is rejected by the test validator. The test must execute and fail on absent files.

```bash
just test -p codex-ai-ip-eval -E 'test(later_work_package_schemas_have_strict_positive_and_negative_fixtures)'
```

- [ ] **Step 2: Create exact schemas from Contracts 4–7**

Use Draft 2020-12, stable `$id`, recursive `additionalProperties:false`, explicit null unions rather than omission, numeric minima/maxima, lowercase SHA patterns, enum constraints, and `if/then` status consistency. Freeze these exact top-level contracts:

```text
cost-receipt:
schemaVersion,frozenRunContextSha256,executionManifestSha256,
brokerReceiptSha256,condition,runOrdinal,attemptIndexRootSha256,attemptRange,
providerLabel,actualModelRevision,rateCardSha256,billingPolicyCommitment,
fxPolicySha256,providerBudgetEvidenceSha256,providerRequestAttemptCount,
providerCompletedResponseCount,usageScope,usage,calculatedAt,calculation,
estimatedFen,supplierStatementSha256,supplierActualFen,chargedFen,withinCeilings

business-report:
all fields in Contract 5's report JSON, including nested capabilityStatus,
genericUsage/candidateUsage and exact retention/source-material enums

report-index:
schemaVersion,attempts; exact attempt fields publicRunId,reportPath,
reportSha256,decision,selected,generatedAt

attempt-index:
the schema validates one exact `attempt-index.jsonl` line, with top-level
`oneOf(requestRecord,terminalRecord)`. The request branch freezes Contract 9's
exact request fields including Plan 05's normalizedRequestCommitment,
normalizedBaseCommitment and explicit-null treatmentDiffCommitment; the terminal
branch freezes Contract 9's exact terminal fields and explicit nullable values.
Consumers validate every nonempty line independently; request/terminal pairing,
gap/uniqueness checks, exact-file SHA and Merkle root remain Rust/receipt duties.

verification:
schemaVersion,verificationKind,frozenRunContextSha256,subjectSha256,
nextIndexSha256,publicRunId,verifierBinarySha256,verifiedAt,valid

retention-closeout:
schemaVersion,pairCommitment,reportCommitment,proofRootSha256,
inventoryCommitment,scheduledAt,deletedAt,status,operator,
proofCopiesDeleted,externalUserSourcesRetained,method,
physicalSecureErasureGuaranteed,failureReasons
```

`retention-closeout.method` is literal `logical-filesystem-delete`; `physicalSecureErasureGuaranteed` is literal `false`; success requires deleted timestamp, `proofCopiesDeleted=true`, and empty failure reasons; failure requires `proofCopiesDeleted=false` and at least one reason.

`BUSINESS_SIGNAL_PASS_PENDING_FOUNDATION` additionally requires
`experiencedOperatorOrDirectorCount>=2`, `candidatePreferenceCount>=2`,
`candidateReadyForHumanReviewCount>=2`,
`medianCandidateScore>=18`, `medianPairedDelta>=3`,
`candidateSkillUseVerified=true`, `candidateSevereFailureCount=0`, and
`capabilityStatus.liveProviderReachable=true` with
`businessBlindReview="passed"`. Tests recursively bind each canonical object to
the corresponding schema `required`/`properties` field set, prove object closure,
mutate every required field, and explicitly exercise supplier, decision,
verification-kind, ordinal, and retention condition branches. The committed 24
fixture files remain the minimum canonical/unknown/missing/wrong-type corpus;
additional exhaustive mutations may be generated in memory.

The business report always includes bounded
`candidateReadyForHumanReviewCount` rather than inferring readiness from
preference or score. Attempt terminal status is consistent: `completed` requires
a response commitment, non-null usage, and `failureClass=null`; `failed` and
`timeout` require a non-null failure class while preserving their other explicit
nullable fields. All JSON integer fields representing Rust `u64` or nonnegative
`i64` values freeze the matching maximum as well as the minimum. Tests bind the
terminal and terminal-usage field sets through independent hard-coded arrays,
not only by reflecting the schema under test.

- [ ] **Step 3: Make filegroup explicit, run GREEN, commit**

`schemas` lists all eight files explicitly with visibility `//codex-rs/ai-ip-eval:__pkg__`. Load every schema with Task 1's offline `jsonschema::draft202012` validator, meta-validate it, then run all positive and negative instances through the compiled validator; field-set assertions are supplementary only.

```bash
just test -p codex-ai-ip-eval -E 'test(later_work_package_schemas_have_strict_positive_and_negative_fixtures)'
bazel build //ai-ip-evals/schemas:schemas
git add ai-ip-evals/schemas/{cost-receipt,business-report,report-index,attempt-index,verification,retention-closeout}.schema.json \
  ai-ip-evals/schemas/BUILD.bazel codex-rs/ai-ip-eval/src/contracts_tests.rs \
  codex-rs/ai-ip-eval/tests/fixtures/contracts/later/{cost-receipt,business-report,report-index,attempt-index,verification,retention-closeout}.{canonical,unknown,missing,wrong-type}.json
git commit -m "test(ai-ip-eval): freeze remaining proof schemas"
```

Expected: static schema behavior is frozen; no later CLI becomes implemented or authorized.

---

### Task 3: Add contained private filesystem and append-only inventory

**Files:**
- Create: `codex-rs/ai-ip-eval/src/private_inventory.rs`
- Create: `codex-rs/ai-ip-eval/src/private_inventory_tests.rs`
- Create: `codex-rs/ai-ip-eval/src/secure_fs.rs`
- Create: `codex-rs/ai-ip-eval/src/secure_fs_tests.rs`
- Modify: `codex-rs/ai-ip-eval/src/lib.rs`
- Modify: `codex-rs/ai-ip-eval/src/runner.rs`

**Interfaces:**
- Consumes: canonical privateRoot from frozen context and existing owner-only file semantics.
- Produces: safe contained create/read/fsync APIs, `PairMarker`, append-only `InventoryRecord`, bootstrap enumeration, and final inventory-root commitment.

**Review-boundary amendment (2026-08-28):** The qualified Task 3 RED proved
the missing marker/inventory behavior, but the complete cross-platform safe
filesystem plus inventory draft cannot fit the global 800-line review boundary
without deleting required checks. Preserve the exact functional scope and execute
it as two dependency-ordered commits with independent specification and quality
reviews. Task 3A owns only `secure_fs.rs`, `secure_fs_tests.rs`, the `lib.rs`
module seam, and this plan amendment; its commit is
`feat(ai-ip-eval): add contained private filesystem`. Task 3B begins only after
3A is Ready Yes and owns `private_inventory.rs`,
`private_inventory_tests.rs`, the narrow `runner.rs` hook and `lib.rs` wiring;
its commit remains `feat(ai-ip-eval): add private proof inventory`. Each commit
must independently remain below 800 changed lines, each new production module
below 500 lines, and Task 4 remains blocked until 3B is Ready Yes.

- [ ] **Step 1: Write runtime RED filesystem and inventory tests**

Use test-local filesystem operations and the existing evaluator binary; assert the current code accepts no secure inventory workflow and produces no marker. The tests compile without new production symbols by exercising `replay-pair`, then inspecting its actual private tree:

```rust
#[test]
fn replay_pair_creates_a_complete_hash_chained_private_inventory() {
    let run = execute_existing_frozen_replay_pair();
    assert!(run.path("coordinator/pair-marker.json").is_file());
    assert!(run.path("coordinator/private-inventory.jsonl").is_file());
    assert_eq!(inventory_paths(), actual_private_tree_paths_without_inventory());
}
```

Add macOS behavior tests for mode, parent escape, symlink, hardlink, existing destination, and chain tamper. Add `#[cfg(windows)]` tests for inherited broad ACL, reparse/junction, hardlink, and non-current-user ACE. The first run fails at runtime because marker/inventory outputs are absent.

- [ ] **Step 2: Run RED**

```bash
just test -p codex-ai-ip-eval -E 'test(replay_pair_creates_a_complete_hash_chained_private_inventory) | test(private_inventory_rejects_path_and_chain_tamper)'
```

- [ ] **Step 3: Implement cross-platform contained private filesystem seam**

Expose only these operations to later modules:

```rust
pub(crate) fn resolve_private_relative(root: &Path, relative: &Path) -> Result<PathBuf>;
pub(crate) fn create_owner_only_dir_new(path: &Path) -> Result<()>;
pub(crate) fn write_owner_only_new(path: &Path, bytes: &[u8]) -> Result<()>;
pub(crate) fn read_single_link_regular(path: &Path) -> Result<Vec<u8>>;
pub(crate) fn fsync_directory(path: &Path) -> Result<()>;
```

Reject absolute/parent/prefix components, symlink/reparse components, junctions, and multi-link files. Unix creates `0700/0600` and uses no-follow descriptor-relative operations. Windows opens with `FILE_FLAG_OPEN_REPARSE_POINT`, rejects reparse/hardlink identity, and applies/verifies a current-user-only protected DACL before returning. Add `#[cfg(windows)]` tests for inherited broad ACL, junction/reparse, and hardlink rejection; macOS tests cover mode, symlink, hardlink, containment, and `create_new`.

- [ ] **Step 3A: Verify and review the contained filesystem commit**

Run the secure-filesystem focused tests plus the evaluator regression, lock and
scope checks, then final format/fix. Commit only the Task 3A boundary:

```bash
just test -p codex-ai-ip-eval -E 'test(secure_fs_)'
just test -p codex-ai-ip-eval
git add docs/superpowers/plans/2026-08-28-06a-blind-review-and-score.md \
  codex-rs/ai-ip-eval/src/secure_fs.rs \
  codex-rs/ai-ip-eval/src/secure_fs_tests.rs codex-rs/ai-ip-eval/src/lib.rs
git commit -m "feat(ai-ip-eval): add contained private filesystem"
```

Fresh specification and quality reviews must return Ready Yes before restoring
or implementing Task 3B inventory changes.

- [ ] **Step 4: Freeze and bootstrap the exact inventory wire**

`pair-marker.json` is exact `{schemaVersion,pairId,frozenRunContextSha256,privateRoot,inventoryRelativePath,createdAt}`. `private-inventory.jsonl` records `{schemaVersion,sequence,relativePath,kind,sha256|null,previousRecordSha256}`. The inventory file itself is the one reserved path excluded from ordinary records and from `actual_private_tree_paths_without_inventory()`; it is committed by `inventoryRootSha256 = SHA-256(exact complete JSONL bytes)` rather than by an impossible self-record. On initial pair sealing, enumerate the complete existing privateRoot tree with no-follow reads, excluding only the inventory file, and record every file/directory plus the marker. Every later append verifies the prior chain, writes one LF line with append+fsync, fsyncs coordinator, and returns the new exact-file root. Paths are privateRoot-relative only. A receipt's `inventoryRootSha256` denotes the verified prefix immediately before the receipt's own record; verifiers then validate the receipt record and recompute the later final root, avoiding a receipt/inventory cycle.

- [ ] **Step 5: Run inventory GREEN, regression, commit Task 3B**

```bash
just test -p codex-ai-ip-eval -E 'test(replay_pair_creates_a_complete_hash_chained_private_inventory) | test(private_inventory_rejects_path_and_chain_tamper)'
just test -p codex-ai-ip-eval
git add codex-rs/ai-ip-eval/src/private_inventory.rs \
  codex-rs/ai-ip-eval/src/private_inventory_tests.rs \
  codex-rs/ai-ip-eval/src/lib.rs codex-rs/ai-ip-eval/src/runner.rs
git commit -m "feat(ai-ip-eval): add private proof inventory"
```

---

### Task 4: Seal manifest-bound post-processing evidence

**Files:**
- Create: `codex-rs/ai-ip-eval/src/proof_archive.rs`
- Create: `codex-rs/ai-ip-eval/src/proof_archive_seal.rs` (owner-only create-new persistence for the seven sidecars and canonical index)
- Create: `codex-rs/ai-ip-eval/src/proof_archive_tests.rs`
- Modify: `codex-rs/ai-ip-eval/src/runner.rs`
- Modify: `codex-rs/ai-ip-eval/src/model.rs`
- Modify: `codex-rs/ai-ip-eval/src/lib.rs`
- Modify: `codex-rs/ai-ip-eval/src/evidence.rs`
- Modify: `codex-rs/ai-ip-eval/src/app_server.rs`
- Modify: `codex-rs/ai-ip-eval/src/broker_gate.rs` (read-only authoritative attempt-range fields on the existing proof snapshot only)

**Interfaces:**
- Consumes: Task 3 secure files/inventory; Replay/Mock pair execution; App Server handshake/start/notifications/quiet-scan inputs; broker completions; catalogs/config audit.
- Produces: `ArmPostprocessIndex`, `RunManifest.postprocess_evidence_index_sha256`, exact sidecars, and `verify_postprocess_archive`.

- [ ] **Step 1: Write binary-level archive RED and mutation tests**

Run an existing frozen Replay pair and inspect JSON/files without referring to new Rust symbols:

```rust
#[test]
fn replay_pair_seals_manifest_bound_postprocess_archives() {
    let run = execute_existing_frozen_replay_pair();
    for ordinal in [1, 2] {
        let manifest = run.read_manifest(ordinal);
        let index_bytes = run.read_bytes(format!("coordinator/run-{ordinal}-postprocess-index.json"));
        let index: serde_json::Value = serde_json::from_slice(&index_bytes).unwrap();
        assert_eq!(index_bytes, canonical_bytes(&index));
        assert_eq!(sha256(&index_bytes), manifest["postprocessEvidenceIndexSha256"]);
    }
}
```

The RED uses only existing `serde_json::Value` plus Task 1's canonical-byte helper; it does not name `ArmPostprocessIndex`. After GREEN defines the typed wire, add a second assertion that the same raw bytes deserialize as `ArmPostprocessIndex` and reserialize identically.

Add table mutations for transcript/start/config/pre/post catalog/quiet tree/broker snapshot/index. The first run compiles/executes and fails because archive outputs are absent.

- [ ] **Step 2: Run RED**

```bash
just test -p codex-ai-ip-eval -E 'test(replay_pair_seals_manifest_bound_postprocess_archives) | test(any_postprocess_archive_tamper_is_rejected)'
```

- [ ] **Step 3: Freeze exact archive wires**

Each arm writes these private files before its immutable manifest:

```text
run-N-notifications.jsonl
run-N-start.json                 # ThreadStartResponse + TurnStartResponse
run-N-config.json                # ConfigReadResponse + requirements
run-N-pre-catalog.json           # raw SkillsListResponse
run-N-post-catalog.json          # raw SkillsListResponse
run-N-quiet-tree.json            # root + both raw list/loaded/read page sets
run-N-broker-snapshot.json       # completions, inFlight=0, attempt root/range
run-N-postprocess-index.json      # exact path/SHA inventory above
```

Native recording covers turn completion, both quiet scans, quiet window, and close, bounded to `10_000` notifications and `64 MiB`. Replay deterministically emits equivalent synthetic start/config/catalog/tree/broker sidecars from already frozen fixture bytes; it never claims live evidence. `RunManifest` adds one lowercase 64-hex `postprocessEvidenceIndexSha256`; the index binds exact bytes of every sidecar, while existing manifest fields bind transcript/package/config/catalog/tree/usage outcomes.

The broker snapshot's global attempt range comes from the existing private `ActiveArm.global_start` plus its checked arm attempt count, exposed only through two read-only fields on `ActiveArmProofSnapshot`. The runner must not reconstruct a second range oracle by independently accumulating arm counts, and this seam must not change broker behavior.

Append all sidecars/index/manifests to Task 3's inventory before final pair verification/receipt.

- [ ] **Step 4: Recompute archive evidence instead of trusting summaries**

`verify_postprocess_archive` hashes the exact raw index bytes and compares that digest to the manifest before parsing, then requires those bytes to equal the canonical serialization of the parsed typed index. It re-parses start responses, re-runs `audit_frozen_config`, normalizes raw pre/post catalogs, rebuilds first/second `TreeScan`, replays notification lifecycle/Skill use/`ReplayCollector`, matches broker completions and in-flight state, and compares every result with the manifest. Any sidecar byte mutation fails its index entry; any index whitespace/key-order/content mutation fails the manifest's raw-byte digest before semantic use.

- [ ] **Step 5: Run GREEN, package regression, commit**

```bash
just test -p codex-ai-ip-eval -E 'test(replay_pair_seals_manifest_bound_postprocess_archives) | test(any_postprocess_archive_tamper_is_rejected)'
just test -p codex-ai-ip-eval
git add codex-rs/ai-ip-eval/src/proof_archive.rs \
  codex-rs/ai-ip-eval/src/proof_archive_tests.rs \
  codex-rs/ai-ip-eval/src/runner.rs codex-rs/ai-ip-eval/src/model.rs \
  codex-rs/ai-ip-eval/src/lib.rs codex-rs/ai-ip-eval/src/evidence.rs \
  codex-rs/ai-ip-eval/src/app_server.rs
git commit -m "feat(ai-ip-eval): seal postprocess proof archives"
```

---

### Task 5: Add the exact blind-pack CLI and fail-closed pair verifier

**Files:**
- Create: `codex-rs/ai-ip-eval/src/blind.rs`
- Create: `codex-rs/ai-ip-eval/src/blind_tests.rs`
- Create: `codex-rs/ai-ip-eval/src/blind_verify.rs`
- Create: `codex-rs/ai-ip-eval/src/blind_verify_tests.rs`
- Create: `codex-rs/ai-ip-eval/src/proof_ledger.rs`
- Create: `codex-rs/ai-ip-eval/src/proof_ledger_tests.rs`
- Create: `codex-rs/ai-ip-eval/tests/blind_cli.rs`
- Modify: `codex-rs/ai-ip-eval/src/model.rs`
- Modify: `codex-rs/ai-ip-eval/src/lib.rs`
- Modify: `codex-rs/ai-ip-eval/src/proof_archive.rs` (read-only verified-summary projection only)
- Modify: `codex-rs/ai-ip-eval/src/runner.rs` (crate-private frozen-context accessors only)
- Modify: `codex-rs/ai-ip-eval/BUILD.bazel` (treatment-marker compile data only)
- Modify: `codex-rs/ai-ip-eval/tests/fixtures/replay-transcript.jsonl`
- Modify: `codex-rs/ai-ip-eval/tests/fixtures/replay-candidate-transcript.jsonl`
- Modify: `codex-rs/ai-ip-eval/tests/fixtures/replay-attestation.json`
- Modify: `codex-rs/ai-ip-eval/tests/fixtures/replay-fixture-set.json`
- Create: `codex-rs/ai-ip-eval/tests/fixtures/blind/treatment-markers.json`
- Modify: `codex-rs/ai-ip-eval/src/eval_tests.rs`

**Interfaces:**
- Consumes: Tasks 1–4 contracts/archive/inventory/secure-fs; exact WP7/WP8 argv.
- Produces: `BlindPackArgs` and `VerifiedBlindPair`; no reviewer-visible output or receipt yet.

**Preflight authority amendment (2026-08-28):** Task 5's original single `blind.rs` file list conflicts with the repository's under-500-line production-module rule. Read-only seam inspection estimates 650–900 production lines if context, archive, ledger/receipt, parity, marker scanning, path rules, and CLI orchestration are combined, and it would duplicate already reviewed verification logic. The authorized split is therefore:

- `blind.rs` owns typed CLI destinations/seed rules, no-output preflight, the `VerifiedBlindPair` handoff, and the exact `BlindBundleStageNotInstalled` sentinel; target 180–260 lines.
- `blind_verify.rs` owns frozen context/pair/manifest/archive parity and treatment-marker orchestration; target 320–430 lines.
- `proof_ledger.rs` owns strict offline attempt-index plus arm/pair-receipt recomputation; target 230–320 lines and must not grow the existing 1,500+ line `broker_gate.rs`.
- `proof_archive.rs` may expose only the already recomputed package/usage/Skill outcome as a read-only summary; it must not duplicate archive verification. `runner.rs` may expose only crate-private accessors for an already verified frozen context; it must not receive new blind verification logic.
- The three production modules receive dedicated sibling test modules. The exact treatment-marker JSON is compile data so Cargo and Bazel execute identical bytes.

Implementation is split into coherent independently reviewable commits below 800 changed lines: (A) real CLI RED plus typed path/seed surface, (B1a) structural native attempt-ledger parsing and derivation, (B1b) raw attempt semantic verification, (B2) arm/pair receipt semantics plus poison/Replay-absence checks, (C) frozen pair/archive verifier and summary seams, and (D) treatment-free fixtures plus complete real-CLI sentinel/mutation matrix. The final feature commit retains the planned title. No Task 6 output transaction, provider request, credential read, scoring, or reviewer-visible file is authorized by this amendment.

**Measured Task 5B split amendment (2026-08-28):** the real `PairCoordinator` fixture plus complete raw JSONL framing/schema/order/identity/prefix/fold/range/usage derivation and re-signed semantic mutations measures 827 source/test lines before arm receipt, pair receipt, poison, or Replay-absence semantics. One B1 commit would therefore exceed the repository's 800-line nonmechanical commit ceiling even before integration wiring. B1a exposes only a crate-private `ParsedAttemptLedger`/`parse_native_attempt_ledger` structural core: safe bounded JSONL framing, embedded-schema validation, exact compact typed JSON, sequence/order, manifest prefix/fold/range recomputation, and derived raw records. It must not use `Verified*` naming or claim semantic verification. B1b consumes that parsed core, installs crate-private `VerifiedAttemptLedger`/`derive_native_attempt_ledger`, and proves request-record binding, completed terminal identity, usage arithmetic/caps, model/deployment identity, and first-record commitments with re-signed mutation RED/GREEN. B2 consumes the verified attempt core, installs the final receipt/poison/Replay-absence verifier, and adds its own mutation RED/GREEN. Every commit must remain below 800 changed lines; the final `proof_ledger.rs` production module must remain below 500 lines. This is a review-boundary split only, not reduced verification scope.

**B1a bounded-read review-fix authority (2026-08-28):** independent reviews of `7b904b14f` found that a pathname metadata precheck followed by the existing unbounded retained-handle read can allocate an attacker-grown file before the parser rejects its cap, and that collecting every newline slice before checking the record count permits newline-bomb amplification. One narrow follow-up may therefore modify only `secure_fs.rs`, `secure_fs_tests.rs`, `proof_ledger.rs`, and `proof_ledger_tests.rs`: add a crate-private cross-platform `read_single_link_regular_bounded(path, cap)` seam that checks the retained handle before allocation, reads no more than the accepted size plus an EOF probe, and preserves the existing identity/link/DACL rechecks; consume that seam from B1a; and stop JSONL line iteration as soon as the checked record bound is exceeded. The correction also adds isolated RED/GREEN coverage for exact/over-cap reads and for CRLF, file/line caps, odd/alternating records, exact compact encoding, global/arm sequence and second-arm reset, post-arm records, manifest prefix/fold/count/range, plus a multi-attempt range fixture. This review-fix commit must stay below 800 changed lines. Because the reviewed cross-platform `secure_fs.rs` is already 494 lines, this narrowly authorized seam may take it above the preferred 500-line target but must keep it below 560; a broad filesystem refactor is not authorized. No B1b semantic verification, B2 receipt/poison/Replay logic, provider behavior, CLI change, or output write is authorized.

**B1b semantic review-fix authority (2026-08-28):** independent reviews of `ba6565946` found four semantic boundary defects. The attested `maxTotalTokensPerRun` is a per-arm validity ceiling, not a sum-of-both-arms ceiling; both arms may therefore each remain at or below the frozen limit even when their sum exceeds it. Terminal-to-manifest equality is insufficient unless both arms also have one exact actual model revision and one exact optional deployment/fingerprint identity. A caller-supplied lowercase manifest hash is not verified evidence unless the same binding supplies exact raw manifest bytes whose SHA-256 is recomputed and whose duplicate-key-safe typed value deep-equals the bound manifest. Finally, each arm's first request must bind `threadCommitment == SHA256(manifest.rootThreadId)` and `parentThreadCommitment == null`, in addition to the four provider-request commitments. Because `proof_ledger.rs` is already 499 lines, the follow-up may add one nested production module `proof_ledger_semantics.rs` and move B1b-only types/logic there; both production modules must remain below 500 lines. The review fix may modify only those two production files and `proof_ledger_tests.rs`, must remain below 800 changed lines, and must add runtime RED/GREEN for a valid per-arm-over-pair-sum case, cross-arm model and deployment drift with terminal+manifest+ledger/raw-manifest re-signing, forged lowercase raw-manifest SHA, typed/raw manifest mismatch, and re-signed first-root thread/parent mutations. The verified core must retain the single cross-arm model/deployment identity for later consumers. No receipt, poison, Replay-absence, provider call, CLI, runner, or broker behavior is authorized in this correction.

**B2 receipt-verifier seam authority (2026-08-28):** after B1b `5dfab6df0` passed fresh specification and quality review with no Critical or Important findings, read-only sizing measured 260–320 production lines for exact arm/pair receipt parsing, poison rejection, and Replay absence. This must not be forced into the 313-line structural parser or the 247-line semantic verifier. B2 may therefore add one nested `proof_ledger_receipts.rs` module and may modify only `proof_ledger.rs`, `proof_ledger_semantics.rs`, `proof_ledger_receipts.rs`, and `proof_ledger_tests.rs`. The B1 parsed and verified arm cores may each gain the recomputed per-arm attempt-index prefix SHA-256 and fold root so B2 never treats a producer manifest summary as an independent expectation. Every production module remains below 500 lines and the independent B2 commit remains below 800 changed lines.

B2 reads the three fixed receipt paths through the existing contained retained-handle bounded-read seam with a one-MiB cap per complete file. Each file must be duplicate-key-safe JSON, exact producer-order compact typed JSON plus exactly one final LF, and its SHA-256 is recomputed over those complete bytes including LF. `VerifiedNativeLedger` retains the B1 verified attempt core, both typed arm receipts with their recomputed raw-file SHA values, the typed pair receipt, and the recomputed pair-receipt SHA. Arm receipts are checked only against B1 verified ranges/counts/per-arm prefix SHA/root/raw-manifest SHA and the independent binding; the first arm has no previous receipt and the second links the recomputed first receipt SHA. Pair totals use checked sums from the B1 verified arms, both receipt and manifest links are exact, failures/timeouts remain zero, and the final fold root and arm-order commitment are independently bound. Receipt timestamps must use the producer's exact UTC-millisecond-`Z` form and satisfy `arm1.sealedAt <= arm2.sealedAt <= pair.finishedAt`; no verifier-wall-clock or monotonic-deadline claim is added. Mock/Live proof verification rejects `poison.json` on both sides of receipt reads. Replay proves that both the exact attempt-index path and the entire receipts path are absent; any file, directory, link, or empty receipts directory fails. Unknown extra native receipt-directory leaves are deferred to Task 5C's complete private inventory/tree verification; B2's exact negative sentinel is `poison.json`.

B2 TDD starts with an arm-1 prefix/full confusion whose arm file, arm-2 previous link, and pair arm links are fully re-signed: B1b must still deep-equal its valid core, while the final verifier must reach and reject the dedicated receipt rule. The table-driven matrix then covers exact encoding/duplicate/unknown/framing, both arm snapshots/ranges/counts/order/previous/raw-manifest links/timestamps, pair arm swaps/order/root/totals/timestamp, poison coexistence, and Replay absence/presence. No provider call, credential access, output transaction, reviewer-visible output, CLI, runner, broker, scoring, or producer token-cap correction is authorized in B2.

The same review exposed a pre-existing producer-side contract mismatch: `runner.rs` passes the attested per-run token ceiling into `BrokerRuntimeConfig::max_total_tokens_per_pair`. That over-restrictive native runtime behavior is not silently changed inside B1b; it is a separately tracked business-capability correction that must receive its own TDD authority before native execution can be called fully usable.

**Producer token-ceiling mapping correction authority (2026-08-28):** B2 is complete and fresh reviews returned no Critical or Important findings. Before 5C, one narrow correction may modify only `runner.rs` and `eval_tests.rs`: map the frozen `maxTotalTokensPerRun` into the existing fixed-two-arm broker aggregate as `checked_mul(2)`, and fail with a dedicated configuration error before any provider request, coordinator output, or evidence output if that arithmetic cannot be represented. This is a bug correction inside the existing 5,000+ line runner, not new runner functionality; no broad runner refactor is authorized. The code commit remains below 800 changed lines.

The runtime RED/GREEN must use the real local native pair path with two responses per arm and set the per-run ceiling equal to each arm's exact aggregate, proving that both legal arms finish even though the pair sum exceeds one arm's ceiling. A focused arithmetic negative proves checked overflow rather than saturation. Existing broker coverage continues to prove that a pair aggregate beyond its configured cap writes `maxTotalTokensExceeded` and poisons. The correction must not change the B1/B2 per-arm proof-validity rule, `BrokerRuntimeConfig` wire/state behavior, attempt caps, money caps, manifests, receipts, provider endpoints, credentials, or public CLI.

**Task 5C measured seam/core split authority (2026-08-28):** the producer token-ceiling correction is complete at `305b27b59` and fresh specification/quality reviews found no Critical or Important issue. Read-only sizing and three independent preflight reviews found that the original C commit would require about 1,000–1,200 changed lines and would otherwise create duplicate Replay, archive, or inventory verification truth. C is therefore split into C1 and C2; both are independent commits below 800 changed lines.

- C1, `refactor(ai-ip-eval): expose verified blind inputs`, may modify only `runner.rs`, `proof_archive.rs`, `proof_archive_tests.rs`, `private_inventory.rs`, `private_inventory_tests.rs`, and `eval_tests.rs`. It is a read-only seam/refactor commit: it creates no blind output, performs no marker scan, makes no provider request, and adds no blind policy to the runner.
- In C1, extract the existing Replay frozen-context verification into one crate-private function that consumes the canonical context path plus caller-retained exact raw context bytes and returns `VerifiedReplayFrozenContext`; add a producer wrapper that obtains those bytes through the existing retained-handle bounded-read policy. The existing Replay producer must call that same function after its single read, while blind verification will later pass `FrozenContextSnapshot.raw_bytes` to it. This shared verifier, and the existing Native verifier in the same commit, own unique-key/schema/typed parsing, exact `serde_json::to_vec_pretty(typed) == raw bytes`, raw SHA, path/identity, and semantic verification; C2 may only compare its retained snapshot path/raw/SHA and consume the returned verified token, never parse frozen context again. The result retains exact content fixture/source/material/Skill/request bytes and exposes the verified mode/path/raw SHA, private root, pair/fork, case and attestation, prompt/additional/schema/thread/turn, Skill, binary/broker commitments, model/provider/limits, and a read-only `reverify_all` operation. Large Codex/evaluator binaries and broker source stay as retained identity/SHA commitments rather than duplicated in-memory byte buffers. Native `VerifiedFrozenContext` receives the equivalent crate-private projections and `reverify_all`; its existing producer remains the source of truth.
- In C1, `verify_postprocess_archive` must construct a same-pass `VerifiedPostprocessSummary`; it may not parse or verify the archive a second time. The same-pass archive reader uses the existing retained-handle bounded seam: notification JSONL keeps its existing legal 64-MiB/10,000-record cap, while the index and each other typed sidecar use an explicit one-MiB cap. The summary retains the verified typed index, `ContentPackage`, the exact producer-order compact typed `ContentPackage` bytes whose SHA-256 equals the manifest commitment, `Usage`, raw-response count, tree-closed result, Skill-use outcome, and normalized base catalog. The existing public/internal return may project its prior index view from this result so old callers keep one verifier truth.
- In C1, private inventory verification must likewise construct a same-pass crate-private `VerifiedPrivateInventory` retaining the verified initial digest/root and typed pair marker, including its pair/frozen-context/private-root binding, plus a retained-handle-style `reverify_unchanged` check. The existing digest-returning API projects from this verified state. `private_inventory.rs` must remain below 500 production lines; no second marker parser or second inventory oracle is authorized.
- C1 TDD covers both the real Replay producer and an existing real Native fixture reaching successful `reverify_all`, plus one external-input identity/byte mutation in each mode; exact raw/path/SHA/reference mutations; deep equality of the complete recomputed archive summary; wrong external pair/frozen/private-root inventory binding and begin/end unchanged drift. The first RED must compile and execute real code, then fail behaviorally at a minimal stub. Finish with the focused evaluator tests, `just fix -p codex-ai-ip-eval`, and scoped format; do not rerun tests after fix/format.

**Measured C1 commit-boundary split (2026-08-28):** the complete C1 implementation and tests measured 741 changed lines before required fix/format, but repository-supported Clippy fixes and formatting raised the exact six-path delta to 808 changed lines. No behavior or coverage may be removed to fit the ceiling. C1 is therefore committed and reviewed as two dependency-ordered slices: C1a, `refactor(ai-ip-eval): share verified frozen contexts`, modifies only `runner.rs` and `eval_tests.rs` and owns the strict Replay/Native verified-context token, producer reuse, projections, and `reverify_all`; C1b, `refactor(ai-ip-eval): expose verified evidence summaries`, modifies only `proof_archive.rs`, `proof_archive_tests.rs`, `private_inventory.rs`, and `private_inventory_tests.rs` and owns the capped same-pass archive summary plus same-pass private-inventory marker/root token, binding, and `reverify_unchanged`. Each commit is independently below 800 changed lines and preserves the already observed C1 RED/GREEN evidence. This is a review/commit split only; the C1 scope, behavior, test matrix, and C2 dependency remain unchanged.

**C1a retained-context identity review-fix authority (2026-08-28):** independent quality review accepted C1b but found that the Replay token retained context path/raw bytes without the initial file handle, so a same-path, same-bytes inode replacement after verification could pass `reverify_all`. One narrow follow-up may modify only `runner.rs` and `eval_tests.rs`: retain an `ArtifactCommitment` for the exact Replay frozen-context handle used by the one-MiB capped load; make Replay `reverify_all` compare that retained handle's identity and bytes rather than accepting a fresh pathname read; and make Native frozen-context loading use the same capped commitment constructor while preserving its existing retained identity. The general artifact reader and non-context limits are unchanged. A runtime RED replaces each mode's frozen-context path with a new owner-only single-link file containing byte-identical content and proves Replay currently accepts the replacement while Native already rejects it; GREEN requires both modes to reject. The correction remains below 800 changed lines, performs no output/provider/blind behavior, and finishes with fix/format without a later test rerun.

The exact review fix at `a980cecb3` closes retained identity but quality re-review found two remaining Native consumers. `commit_arm_order` still calls the general unbounded artifact reader; the same narrow follow-up under the two-file scope must replace that call with the retained context-only bounded verifier and cached raw bytes/SHA. Native `reverify_all` must also stop rebuilding a second verified context from the current pathname: it first verifies the original retained context handle/bytes, then the original retained artifact commitments and managed sources, without re-parsing or re-freezing pathname inputs. The runtime RED grows the same retained context inode beyond one MiB after initial verification, requires the dedicated cap failure, and proves neither coordinator nor seed was created; the existing same-bytes inode replacement test must lock the retained-identity error. The correction must not change general artifact reads, arm selection, CSPRNG, provider behavior, or output semantics for a valid context.

**C1 verified-content projection amendment (2026-08-29):** C2 preflight found two values that cannot be recreated without violating C1's single-verifier boundary: Replay's frozen fixture-set SHA is not projected, and neither mode exposes one typed, already verified case/attestation/material input token. C1 receives two dependency-ordered follow-ups, each modifying only `runner.rs` and `eval_tests.rs` and staying below 800 changed lines. C1c, `fix(ai-ip-eval): retain replay frozen material inputs`, projects the retained fixture-set SHA and installs a typed Replay material token. The authoritative Replay material root is the canonical parent of the already verified case fixture; each case-declared normalized relative material path must be unique, remain canonically contained below that root, avoid every primary fixture and the fixture-set manifest, resolve through retained single-link regular-file handling, and have exact bytes whose SHA-256 equals the case declaration. Freeze-time and shared run-time verification use the same material rule, the token retains each material ID/relative path/SHA/exact bytes plus its commitment, and `reverify_all` covers every material. Pair-ID and frozen wire/schema remain unchanged because the bound case bytes already commit the declarations and digests. A real nonempty-material Replay RED mutates or removes a material after freeze and proves failure before any Replay coordinator output; GREEN also covers happy projection, digest, path escape/collision, byte and same-byte-inode drift, and exact fixture-set SHA projection.

C1d, `refactor(ai-ip-eval): expose verified native content inputs`, adds one crate-private typed Native content projection produced by the same managed-source verifier that currently checks and discards those values. It retains the typed case and exact case bytes, typed native attestation and three reviewer declarations, exact materials-manifest bytes, every declared material ID/relative path/SHA/exact bytes, prompt/additional/schema/thread/turn/Skill bytes, Codex/evaluator/broker commitments, model/provider and all frozen limits. Existing before-arm and `reverify_all` checks reuse the same collector and compare/revalidate the retained commitments; C2 may consume this token but may not parse Native case/attestation/materials again. The runtime RED uses an existing real Native producer fixture, reaches a compiling projection stub, and then deep-equals the complete projection; a material identity/byte drift remains rejected. Neither C1c nor C1d adds blind policy, provider calls, new output, schema/fixture roles, or a material-count restriction.

**Measured C2 structural/semantic split (2026-08-29):** after C1c/C1d, the complete C2 production and required two-mode mutation matrix still measures about 790–945 changed lines and would place 520–660 production lines in one module. C2 is therefore split without reducing scope. C2a, `feat(ai-ip-eval): parse sealed blind pair envelopes`, may modify `blind.rs`, `blind_tests.rs`, `blind_verify.rs`, `blind_verify_tests.rs`, `lib.rs`, and only the test-module mount in `eval_tests.rs`. It returns crate-private `ParsedPairEvidence`, never `PairEvidenceCore`: a mode-specific C1 token, one same-pass `VerifiedPrivateInventory` token and binding, exact typed/raw/SHA execution context, both exact typed/raw/SHA manifests, and exact typed/raw/SHA pair verification. It makes the blind snapshot read one-MiB bounded; derives fixed paths from the verified token; uses duplicate-key-safe exact producer-order typed pretty JSON without an added LF; proves mode/pair/frozen/ordinal/condition/arm-set/direct SHA links; and then leaves the valid real pair at `PairEvidenceCoreStageNotInstalled` with all five outputs absent. It does not call archive or B2 verification and makes no semantic-core claim.

C2b, `feat(ai-ip-eval): verify sealed blind pair cores`, may add `blind_verify_semantics.rs` and `blind_verify_semantics_tests.rs` and narrowly wire them from `blind_verify.rs`, `blind.rs`, `lib.rs`, and the already mounted test module. It consumes `ParsedPairEvidence` without re-reading or re-parsing its files, calls each C1 archive summary exactly once, consumes the retained inventory token, invokes B2 Native ledger/receipts or Replay absence, proves all context/execution/manifest/pair links and the authorized cross-arm semantic parity, and only then constructs crate-private `PairEvidenceCore` with every field listed above. `blind_verify.rs` and `blind_verify_semantics.rs` each remain below 500 production lines; C2a and C2b each remain below 800 changed lines. `blind_verify_tests.rs` is mounted as a child of `eval_tests.rs` solely to reuse the existing private real Replay and Native producer fixtures rather than copying hundreds of harness lines. C2a owns exact encoding/framing/path/link/marker tests; C2b owns complete Replay/Native expected projections, B2 reachability, re-signed model/deployment/config/catalog/request-base and archive-semantic mutations with rebuilt inventories, and five-destination absence for every failure and current-stage sentinel. Existing two-field blind header tests remain header/path/seed tests and are not treated as C2 happy-pair evidence.

**Measured C2a test-matrix split (2026-08-29):** the first readable implementation measured exactly 800 changed lines before the required legacy-sentinel adaptations and before formatting, so one C2a commit cannot honestly remain below 800 without deleting coverage or compressing the code for the metric. C2a is therefore delivered in two independently reviewable commits that share the already recorded real freeze-to-Replay behavioral RED. C2a1 retains the exact title `feat(ai-ip-eval): parse sealed blind pair envelopes` and may contain the complete production parser plus only the real Replay/Native happy projection, current-stage sentinel, retained-token recheck, and five-output-absence tests needed to prove the new interface. C2a2, `test(ai-ip-eval): harden sealed blind pair envelopes`, may modify only `blind_verify_tests.rs` and adds the duplicate/unknown/exact-pretty/trailing-byte/cap, fixed-path/link, ordinal/condition/mode, pair/frozen/direct-link, and independently valid wrong-marker attack matrix. C2a2 is test hardening for the same feature RED, not a claim that every attack case was separately red. If any C2a2 attack exposes a production defect, it must stop and receive narrow correction authority rather than silently changing production under the test-only title. Both commits remain below 800 changed lines; the complete matrix and all five-output-absence assertions remain mandatory, and `blind_verify.rs` remains below 500 production lines after formatting.

C2a1 may also modify only the stale sentinel expectations in `blind_tests.rs` and `tests/blind_cli.rs` from `BlindPairVerifierStageNotInstalled` to `PairEvidenceCoreStageNotInstalled`; this is required compatibility wiring for the same real CLI stage transition and authorizes no other integration-test or CLI change.

The first C2a1 compatibility run proved that the real CLI fixture reaches `PairEvidenceCoreStageNotInstalled`, while two intentionally minimal `blind_tests.rs` header fixtures now fail earlier at the newly installed C1 boundary. Those two unit tests may receive test-only semantic expectation updates: the mode-disjoint seed test must prove it passed seed validation and then failed the Native frozen-context contract, and the retained-snapshot identity test must prove that replacing the context pathname after the snapshot is rejected by the Replay retained-handle identity check. Both continue to assert all five outputs absent. Production bypasses, relaxed context parsing, new fixtures, and any other test change are not authorized.

**C2a1 review-fix authority (2026-08-29):** fresh reviews of `ffc9d53b3` found two Important binding defects. First, `FrozenContextSnapshot` retains only bytes/SHA/header and later reopens the pathname to create its C1 token, so a same-byte new-inode replacement after the snapshot is accepted; the existing compatibility test changed bytes on the original inode and did not prove the authorized identity rule. Second, the Native manifest check accepts each arm independently as either Mock or Live, allowing a fully re-signed mixed-mode or all-Live pair even though the current C1 Native producer identity is the fixed local `synthetic-loopback-mock / OpenAi` producer and its manifests are Mock. A narrow correction may modify only `runner.rs`, `blind.rs`, `blind_tests.rs`, `blind_verify.rs`, and `blind_verify_tests.rs`. It may expose one crate-private retained frozen-context file token backed by the existing one-MiB-bounded `ArtifactCommitment::freeze_context`; `FrozenContextSnapshot` must own that first token, and C1-token creation must bracket its second open with retained identity/byte rechecks. It must derive the one expected manifest mode from the verified mode-specific C1 producer token (Replay to Replay; the current synthetic Native producer to Mock) and require both manifests to equal it. It must add runtime RED/GREEN for a same-byte new-inode context replacement and for a fully re-signed Native single-arm mode mutation with rebuilt pair link and inventory. The correction may deepen the existing Native happy projection/inventory assertions and remove the now-unreachable `BlindPairVerifierStageNotInstalled` fallthrough, but it may not add archive/B2 semantics, provider behavior, output, marker scanning, or the remaining C2a2 attack matrix. `blind_verify.rs` remains below 500 production lines and the correction commit remains below 800 changed lines. C2a2 stays blocked until both fresh reviews accept the correction; its remaining matrix need not duplicate the mode attack that this fix owns.

**C2a2 cross-platform assertion correction (2026-08-29):** fresh review of `5ceee3f92` found that the real hardlink attack is cross-platform but its expected error text names only the Unix secure-filesystem diagnostic. One test-only follow-up may modify only `blind_verify_tests.rs` so that this hardlink case accepts exactly the Unix `unsafe type, links, or permissions` diagnostic or the Windows `reparse point or hardlink` diagnostic, while still rejecting arbitrary errors and asserting all five outputs absent. No production change or other matrix rewrite is authorized. The execution-resigning helper's lack of an inventory rebuild remains a disclosed non-blocking test-depth Minor because C2a structural validation intentionally precedes inventory and C2b owns below-tree fully re-signed semantic mutations.

**C2a2 full-regression fixture correction authority (2026-08-29):** Task 7E's first full evaluator regression proved that the stricter private-inventory verifier installed after C2a2 now correctly preempts four older parser-mutation expectations: file mutations and fully re-signed execution mutations change inventory-covered bytes, while missing/link attacks fail at the inventory/tree or anchored retained-reader boundary. One narrow test-only follow-up may modify only `blind_verify_tests.rs` and this plan. Its shared rebuild helper must retain the original pair marker `createdAt`, rebuild inventory after each byte mutation and after restoration, and do the same after fully re-signing and restoring execution-linked documents. Missing and Unix symlink attacks must assert the current exact inventory/tree diagnostics. The hardlink attack may accept only the four current cross-platform secure-reader fragments: Unix `unsafe type, links, or permissions`, legacy Windows `reparse point or hardlink`, current Windows `reparse point, hardlink, or wrong type`, or macOS/Unix anchored `anchored artifact has the wrong type or multiple links`; all five outputs must remain absent. No production, provider, output, score, timeout, nextest-profile, or other matrix change is authorized. Native harness timeouts remain a Task 7E regression-gate failure and may not be reported as GREEN or solved by silently widening the watchdog.

**C1 sequential archive-summary handoff authority (2026-08-29):** C2b preflight found that the existing Native run-2 archive verifier recursively calls the complete run-1 archive verifier to derive its global attempt start. Calling the two C1 summary entry points once would therefore still read and verify run 1 twice, violating C2's one-summary-pass boundary and creating a second archive oracle. One dependency-ordered seam correction, `refactor(ai-ip-eval): hand off sequential archive summaries`, may modify only `proof_archive.rs` and `proof_archive_tests.rs`. `VerifiedPostprocessSummary` must retain the already verified broker global attempt start/end. A new crate-private run-2 summary entry point consumes the retained run-1 summary, proves the prior pair/ordinal/opposite-condition/evidence-source and range identity, derives Replay start zero or Native start from the verified prior end, and verifies only run 2. It must not reopen or reparse any run-1 manifest, index, transcript, or sidecar. The existing public single-archive API keeps its current compatibility behavior; C2b uses the ordinary run-1 summary followed by the sequential run-2 entry point. RED/GREEN must cover real Replay and Native pairs, deep-equal the retained ranges, and prove run 2 still succeeds after the already verified run-1 manifest/archive pathnames become unavailable. This seam changes no producer bytes, provider behavior, blind policy, output, marker scan, ledger/receipt verification, or final re-verification, and the commit remains below 800 changed lines with `proof_archive.rs` below 500 production lines.

**Measured C2b production/matrix split (2026-08-29):** after C2a and the sequential archive-summary seam, the honest semantic core plus the required fully re-signed mutation matrix measures about 860–1,030 changed lines. C2b is therefore delivered as two review boundaries without reducing scope. C2b1 retains the exact title `feat(ai-ip-eval): verify sealed blind pair cores` and may contain the complete `blind_verify_semantics.rs` production verifier, narrow wiring, real Replay/Native deep-equal happy cores, proof that Native reaches and consumes B2, and the current-stage transition. After constructing and consuming `PairEvidenceCore`, the installed blind flow must return the new exact `BlindPairFinalizationStageNotInstalled` sentinel with all five destinations absent; `PairEvidenceCoreStageNotInstalled` is removed and the older ambiguous sentinel is not revived. C2b1 may also modify only the stale exact sentinel expectations in `blind_verify_tests.rs` and `tests/blind_cli.rs` from `PairEvidenceCoreStageNotInstalled` to `BlindPairFinalizationStageNotInstalled`; no other test or CLI change is authorized in those files. C2b2, `test(ai-ip-eval): harden sealed blind pair semantics`, may modify only `blind_verify_semantics_tests.rs` and adds the complete fully re-signed model/deployment/config/catalog/request-base/archive mutation matrix, rebuilt inventories, and Replay forbidden-ledger/receipt cases. If C2b2 exposes a production defect, implementation stops for narrow correction authority rather than changing production under a test-only title. Each commit remains below 800 changed lines; both semantic production modules remain below 500 lines. D alone installs decoded treatment-marker scanning, candidate Skill payload equality, final input/inventory re-verification, five-destination preflight, and `VerifiedBlindPair`.

**C2b1 semantic-binding module amendment (2026-08-29):** the first honest GREEN implementation of the complete C1/manifest/execution/pair binding plus archive/B2 orchestration measured 562 production lines in `blind_verify_semantics.rs`, above the 500-line module ceiling before test hardening. C2b1 may therefore also add `blind_verify_semantics_bindings.rs` as a private child of `blind_verify`: it owns only the immutable C1 content projection and manifest/execution/pair/request-parity validation, while `blind_verify_semantics.rs` owns archive sequencing, B2 ledger/receipt comparison, and `PairEvidenceCore` construction. Neither module may re-read C2a documents or perform D/Task 6 work; both remain below 500 lines, the C2b1 commit remains below 800 changed lines, and the required two-mode happy/B2 reachability scope is unchanged.

**C2b1 dual-review semantic correction authority (2026-08-29):** independent reviews of `a5be2ac97` found four Important authority defects before C2b2. One narrow correction, `fix(ai-ip-eval): bind sealed producer semantics`, may modify only `runner.rs`, `blind_verify.rs`, `blind_verify_semantics_bindings.rs`, `blind_verify_semantics_tests.rs`, and `blind_verify_tests.rs`; the last file may only revert the test-helper visibility widening from C2b1. The correction must consume the retained Replay `genericRequest` and `candidateRequest` bytes without reopening either fixture, rerun the exact producer canonicalizer with the derived Replay Homes and frozen producer constants, and bind every raw/normalized/base/treatment commitment to the manifests and pair verification. It must independently rebuild the deterministic shared config with `build_shared_config` using Replay port `1` or the sealed Native loopback broker port and bind its exact byte hash; the existing archive audit remains responsible for effective/layer semantics. Replay manifests must additionally prove the exact provider-not-run attempt/completion counts, ledger/root sentinels, usage scope, actual model revision, absent deployment/fingerprint, and zero elapsed producer constants. Native `pathSha256` must be canonical lowercase SHA-256 shape but must not be compared with the later blind-pack process's ambient `PATH`; the execution raw SHA, manifests, pair and B2 chain retain the historical commitment. Runtime RED/GREEN must cover a fully re-signed frozen-request commitment attack, a fully re-signed alternate valid shared config, a re-signed Replay attempt/range attack, and an isolated real-Native semantic check showing another well-formed historical PATH commitment is not compared with ambient state; every filesystem attack must rebuild inventory and prove all five outputs absent. The correction may add only a test-gated in-memory seam in `blind_verify.rs` to isolate that Native historical-field rule, may expose only the existing pure Replay canonicalizer crate-privately, and changes no producer bytes or provider behavior. It must not add the remaining C2b2 matrix, decoded treatment scanning, final re-verification, output, Task 6, or scoring. The correction commit remains below 800 changed lines, both semantic production modules remain below 500 lines, and finishes with the scoped crate test, `just fix -p codex-ai-ip-eval`, and scoped formatting without rerunning tests afterward.

- Across the measured C2a/C2b paths authorized above, C2 consumes C1 plus B2 and returns a crate-private `PairEvidenceCore`; that type is not re-exported from `lib.rs`, is not a Task 6 input, and must not use `VerifiedBlindPair` or other naming that claims treatment-marker/final-input verification has completed.
- C2 completes all sealed pair evidence verification except decoded treatment-marker scanning and the final input/inventory re-verification: exact mode/private-root/pair/fork/frozen-context SHA; exact case, attestation, materials manifest and every declared material path/byte/SHA; prompt/additional/schema/thread/turn; Skill and binary/broker commitments; model/provider/limits; fixed paths; typed duplicate-key-safe raw manifests and pair verification; B2 native ledger or exact Replay ledger absence; inventory marker/root; verified archives and manifest links; and shared semantic parity. C2 directly reads manifests, pair verification, and other non-archive complete proof files with the retained-handle bounded seam and a one-MiB cap per file; frozen context is consumed from C1's already retained snapshot/token, and archives are consumed only through C1's capped same-pass summary. Exact producer-order bytes, final LF rules, recomputed SHA-256, and typed deep equality are mandatory where the producer contract is exact.
- `PairEvidenceCore` privately owns the mode-specific C1 verified frozen-input token and the complete `VerifiedPrivateInventory` token so D can invoke their existing re-verification methods without parsing or rebuilding an oracle. It also projects and retains enough verified typed values and exact bytes for D and Task 6 without re-parsing: mode, private root, pair/fork/frozen SHA, initial inventory root, pair-verification raw SHA, optional native pair-receipt raw SHA, the three reviewer declarations, typed case plus exact bytes, typed materials manifest plus exact bytes and each declared material relative path/bytes/SHA, and both ordered arms' condition/ordinal/raw-manifest SHA/exact producer-order compact typed `ContentPackage` bytes and value/usage/raw-response count/Skill outcome.
- Shared semantic fields must be equal. The first-root request may differ only through the canonical normalized Skill treatment; its normalized non-treatment base must be equal. Thread/session identifiers, post-root trace, attempt ranges/counts, archive/ledger hashes, elapsed values, and package/usage may differ only where independently recomputed from the treatment or resulting output; these values do not enter a reviewer-visible parity assertion. This rule protects real business differentiation while rejecting uncommitted execution drift.
- C2 makes the context snapshot read in `blind.rs` bounded at one MiB. Its first runtime RED uses the real freeze-to-Replay producer and a compiling `PairEvidenceCore` stub, reaches that stub, and fails behaviorally. GREEN must deep-equal complete happy cores from both real Replay and real Native fixtures and prove the native ledger is actually consumed. The mutation matrix isolates exact manifest duplicate/unknown/framing/ordinal/condition, exact pair-verification encoding/link, marker binding, re-signed cross-arm model/deployment, config/catalog and normalized request-base drift, and a re-signed archive-semantic drift; Replay forbidden ledger and empty receipts-directory cases must reach B2. Mutations aimed below tree integrity must re-sign downstream links and rebuild a valid inventory. Every failure and both mode-valid current-stage sentinels/stubs use `symlink_metadata` to prove all five output destinations absent. `blind_verify.rs` remains below 500 production lines and the commit remains below 800 changed lines; if the authored RED plus production implementation cannot fit without dropping this matrix, C2 must receive a further review-boundary split rather than deleting coverage.

- D alone scans every decoded package string for treatment markers and, as a separate non-marker rule, verifies that the candidate Skill-use payload is complete/untruncated and byte-equal to the frozen Skill asset after protocol-wrapper removal; it then calls both frozen-context/external-input and inventory `reverify_unchanged`, proves the five output destinations still do not exist, and only then constructs the final `VerifiedBlindPair` consumed by Task 6 before reaching `BlindBundleStageNotInstalled`. D may add a nested marker/finalization module if needed to keep every production module below 500 lines. No C1/C2 change to provider/broker behavior, output transactions, reviewer bundles, scoring, treatment-marker fixtures, Bazel compile data, or public CLI is authorized.

- [ ] **Step 1: Write real CLI RED using authoritative argv**

`tests/blind_cli.rs` spawns `codex-ai-ip-eval` via `cargo_bin`, freezes/runs the synthetic Replay pair, then executes exactly:

```text
blind-pack
--reviewer-root reviewer
--mapping-dir coordinator/mappings
--replay-seed mechanical-reviewer-1
--replay-seed mechanical-reviewer-2
--replay-seed mechanical-reviewer-3
--frozen-run-context <absolute-context>
```

Assert the current real binary rejects the authoritative argv as unimplemented. Add mutation cases whose expected verifier-specific errors are not yet produced. The test compiles and executes real Clap/binary code, so this is behavioral RED.

- [ ] **Step 2: Add exact typed CLI and path table**

```rust
struct BlindPackArgs {
    reviewer_root: PathBuf,
    mapping_dir: PathBuf,
    seed_dir: Option<PathBuf>,
    replay_seeds: Vec<String>,
    frozen_run_context: PathBuf,
}
```

The only accepted resolved destinations are:

```text
reviewerRoot: privateRoot/reviewer
mappingDir:   privateRoot/coordinator/mappings
seedDir:      privateRoot/coordinator/blind-seeds (Mock/Live only)
reviews:      privateRoot/reviews
receipt:      privateRoot/coordinator/blind-pack-receipt.json
```

Relative CLI values above resolve against privateRoot; absolute values are rejected for blind-pack. Replay requires no seed-dir and exactly three distinct nonempty strings. Mock/Live require exact seed-dir and no replay seeds. Until Task 6 installs the bundle transaction, a valid verified pair reaches one exact typed `BlindBundleStageNotInstalled` error and creates no output; all verification failures must occur before that sentinel.

- [ ] **Step 3: Reverify all bound pair evidence before creating output**

From frozen context and pair marker, derive every receipt/verification/manifest/archive path. Re-read and recompute case/material aggregate, `root_prompt()`, `evaluation_context()`, schema, normalized thread/turn request, current Codex/evaluator binaries and broker component source, config/catalog/Skill treatment, pair order, broker index/root, model/provider/limits, typed package and complete tree usage. Only condition, committed ordinal, normalized target Skill treatment/use, output body, and output-derived usage may differ.

Recursively scan decoded package JSON string values and reject case-insensitive `candidate`, `generic`, canonical Skill name/path, privateRoot, both Home prefixes, and every synthetic marker in `tests/fixtures/blind/treatment-markers.json`. Update the committed Replay candidate title/body and native mock harness to different treatment-free synthetic text; recompute the execution-only pair ID, update only the Replay attestation's bound `pairId`, then update the attestation and execution fixture hashes. Reviewer declaration commitments remain byte-stable unless the reviewer declaration objects themselves change. Because pair ID excludes attestation, this order is finite and reproducible. Candidate Skill payload must be complete/untruncated and byte-equal to the asset after protocol wrapper removal.

- [ ] **Step 4: Add verifier matrix, run GREEN, commit**

Cover wrong relative/absolute path, seed/reviewer/mapping/reviews ancestry, symlink/reparse/hardlink, marker leakage, each bound-file mutation, and current evaluator/Codex binary or broker component source mismatch. All failures and the valid-pair sentinel assert that no reviewer, mapping, seed, reviews, or receipt path was created. Current binary or broker source mismatch is always a command error with no output, never evidence-derived `INVALID_PROOF`.

```bash
just test -p codex-ai-ip-eval -E 'test(blind_cli)'
just test -p codex-ai-ip-eval
git add codex-rs/ai-ip-eval/src/blind.rs \
  codex-rs/ai-ip-eval/src/blind_tests.rs \
  codex-rs/ai-ip-eval/src/blind_verify.rs \
  codex-rs/ai-ip-eval/src/blind_verify_tests.rs \
  codex-rs/ai-ip-eval/src/proof_ledger.rs \
  codex-rs/ai-ip-eval/src/proof_ledger_tests.rs \
  codex-rs/ai-ip-eval/src/model.rs codex-rs/ai-ip-eval/src/lib.rs \
  codex-rs/ai-ip-eval/src/proof_archive.rs codex-rs/ai-ip-eval/src/runner.rs \
  codex-rs/ai-ip-eval/src/eval_tests.rs codex-rs/ai-ip-eval/BUILD.bazel \
  codex-rs/ai-ip-eval/tests/blind_cli.rs \
  codex-rs/ai-ip-eval/tests/fixtures/blind/treatment-markers.json \
  codex-rs/ai-ip-eval/tests/fixtures/replay-transcript.jsonl \
  codex-rs/ai-ip-eval/tests/fixtures/replay-candidate-transcript.jsonl \
  codex-rs/ai-ip-eval/tests/fixtures/replay-attestation.json \
  codex-rs/ai-ip-eval/tests/fixtures/replay-fixture-set.json
git commit -m "feat(ai-ip-eval): verify sealed blind review pairs"
```

---

### Task 6: Create three independent blind bundles in a bounded transaction

**Task 6 exact-wire, transaction-seam, and review-boundary amendment (2026-08-29):** the original
single Task 6 commit is not an honest sub-800-line boundary. The existing secure filesystem has no
same-parent no-replace publish primitive, and the private inventory can admit only one new path at a
time, while publishing one complete reviewer tree introduces its directory and all descendants at
once. Task 6 is therefore delivered through the five independently reviewable commits below; no
matrix or platform behavior may be dropped to fit a commit.

- `feat(ai-ip-eval): publish private trees without replacement` may add
  `secure_fs_publish.rs` plus a sibling test module and a narrow mount in `secure_fs.rs`/`lib.rs`.
  It publishes one already durable owner-only staging entry to an absent final sibling without
  replacement. Unix uses an anchored same-parent no-replace rename (`renameat2(RENAME_NOREPLACE)`
  where available and `renameatx_np(RENAME_EXCL)` on Apple); Windows uses the equivalent
  no-replace handle/path operation. It rejects links/reparse points, a changed parent or source
  identity, cross-parent/cross-volume publication, and any existing destination, then fsyncs the
  final entry and parent. Production and test modules each remain below 500 lines.
- `feat(ai-ip-eval): append private inventory batches` may add
  `private_inventory_batch.rs` plus a sibling test module and narrow shared internals in
  `private_inventory.rs`/`lib.rs`. Its caller supplies a normalized, sorted, duplicate-free exact
  set of `(relativePath, kind, optionalSha256)` entries. Against one retained old inventory prefix,
  it requires the actual tree to equal the old tree plus exactly that set, appends every canonical
  chained record in tree order, fsyncs once, and returns the new root. It never discovers and
  blesses unexpected files. The existing one-entry append and verifier behavior remain unchanged.
- `feat(ai-ip-eval): prepare deterministic blind review bundles` owns typed preparation only: a
  narrow `VerifiedBlindPair::bundle_projection()` and `reverify_unchanged()` boundary,
  contract-commitment accessors, deterministic Replay/native seed-set preparation, typed bundle and
  mapping bytes, and no filesystem output. Task 6 must not consume or expose `PairEvidenceCore` or
  `VerifiedPrivateInventory` directly. If production approaches 500 lines, exact wire types move to
  `blind_bundle_model.rs` instead of growing one module.
- `feat(ai-ip-eval): create bound blind review bundles` owns the real output transaction, receipt,
  inventory integration, and real CLI GREEN. The transaction is fail-closed but does not claim
  cross-directory atomicity. Each complete staging tree is published and batch-recorded before the
  next staging tree is created; the receipt is created last and then recorded with the existing
  one-entry append.
- `test(ai-ip-eval): harden blind bundle transaction failures` adds the complete retry, entropy,
  partial-publication, path, and fault-boundary matrix. A production defect found here receives a
  separate narrow correction rather than weakening the matrix.

**Measured Task 6C commit-boundary split (2026-08-29):** the typed preparation implementation is
about 578 changed lines and its real Replay/Native, deterministic-seed, exact-wire, read-only, and
drift matrix is about 402 more. No production boundary or behavioral coverage may be deleted to fit
the ceiling. The preparation is therefore committed first as
`feat(ai-ip-eval): prepare deterministic blind review bundles`, followed by
`test(ai-ip-eval): harden deterministic blind bundle preparation`. The production commit also owns
the one-line `blind.rs` adapter needed to remove the old `PairEvidenceCore` escape without breaking
the crate; the test commit owns the narrow `eval_tests.rs` child mount needed to reuse the real pair
harness. This measured split supersedes the single Task 6C staging stanza below; each commit remains
independently below 800 changed lines.

**Task 6D inventory-cursor amendment (2026-08-29):** read-only seam review found that the existing
inventory append APIs accept any currently self-consistent prefix. A pathname-level verification
before calling them would leave a race between verification and the retained append handle. Before
the transaction commit, add checked batch and single-entry variants that compare an
`expectedOldInventoryRootSha256` against the exact old bytes read from the retained inventory
handle before any write. Add a narrow `VerifiedBlindPair::begin_bundle_transaction()` that
reverifies the sealed pair and returns only its initial inventory-root commitment; it must not
expose `PairEvidenceCore` or `VerifiedPrivateInventory`. The transaction then advances this scalar
cursor with every checked append. This prerequisite may modify `blind_finalize.rs`,
`private_inventory.rs`, and `private_inventory_batch.rs` in a separate sub-800-line commit named
`fix(ai-ip-eval): bind blind bundle inventory cursor`; focused tests belong in the existing sibling
inventory/finalizer test modules. This measured seam correction precedes and is consumed by the
Task 6D transaction commit.

**Measured Task 6D production/success-test split (2026-08-29):** the real transaction production
path, exact prepared-token re-binding, root-identity anchoring, and CLI wiring no longer fit honestly
beside the exact Replay/Native success oracles in one sub-800-line commit. Commit production first
as `feat(ai-ip-eval): create bound blind review bundles`, then commit the real CLI, Replay/Native
transaction, compatibility-success, exact-byte, receipt, inventory-prefix, privacy, determinism, and
rerun tests as `test(ai-ip-eval): verify bound blind review transactions`; the existing Task 6E
failure/fault matrix remains a third commit. The production commit may add
`blind_bundle_transaction_validate.rs` and a narrow retained private-root identity module if needed.
The success-test commit may modify `blind_finalize_tests.rs`, `blind_verify_tests.rs`,
`blind_verify_semantics_tests.rs`, and `eval_tests.rs` solely to replace the obsolete
`BlindBundleStageNotInstalled` success sentinel and mount the new sibling transaction tests. This
measured split supersedes the single Task 6D staging stanza below; no exact success assertion moves
to the Task 6E failure commit.

**Authoritative Replay seed correction (2026-08-29):** the earlier illustrative CLI seeds
`mechanical-reviewer-{1,2,3}` deterministically produce one orientation for all three reviewers and
therefore conflict with the later frozen anti-degeneracy rule. The authoritative Task 6/7 Replay
CLI vector is now exactly `one`, `two`, `three` in reviewer-slot order; the earlier three values are
superseded, not silently accepted. All three derived seeds and commitments remain unique, and this
vector produces both orientations.

**Task 6D retained-root and receipt-binding correction (2026-08-29):** the transaction must retain
an opaque identity for the canonical private root at `begin_bundle_transaction`, reverify that
identity across every mutation boundary, and reject same-path/same-bytes inode replacement. The
receipt inventory append must bind the SHA-256 of the exact JCS bytes originally generated and
written; it may not discover and bless whatever bytes happen to exist at the receipt pathname.
Use an exact one-entry checked batch or an equivalent expected-SHA single append. The success/fault
tests must include receipt replacement in the post-create/pre-inventory hook.

**Post-format Task 6D success-test split (2026-08-29):** required workspace formatting expands the
complete real-CLI oracle enough that combining it with the 368-line sibling transaction test would
exceed the 800-changed-line review ceiling. Commit the sibling Replay/Native transaction,
compatibility-success, exact prepared-byte, receipt, inventory, and semantic-mutation tests first as
`test(ai-ip-eval): verify bound blind review transactions`; then commit the real binary exact-tree,
source-byte, mapping/bundle/receipt, privacy, inventory-chain, rerun, and cross-root determinism
oracle as `test(ai-ip-eval): verify bound blind review CLI contract`. No success assertion moves to
Task 6E, and both commits remain independently below 800 changed lines.

**Task 6E failure sibling amendment (2026-08-29):** the formatted success transaction sibling is
already 368 lines. Keep the fault matrix independently reviewable in
`blind_bundle_transaction_failure_tests.rs`, mounted only from `eval_tests.rs`; it owns staging
preflight, injected checkpoint states, read-only rerun, receipt replacement, clock failure, and
retained-root replacement tests, while reusing the already verified transaction production seam.

The exact Task 6 wire and bytes are frozen as follows:

```rust
struct ReviewerQualificationCommitment {
    qualification_class: String,
    experienced_operator_or_director: bool,
    attestation_signed_payload_sha256: String,
    attestation_signature_evidence_sha256: String,
}

struct ReviewBundleManifest {
    schema_version: u32,
    pair_id: String,
    reviewer_id: String,
    qualification: ReviewerQualificationCommitment,
    case_sha256: String,
    materials_manifest_sha256: String,
    source_materials_sha256: String,
    a_sha256: String,
    b_sha256: String,
    rubric_sha256: String,
    reviewer_submission_schema_sha256: String,
}
```

- Reviewer roots are exactly `reviewer/<reviewerId>/`; mappings are exactly
  `coordinator/mappings/<reviewerId>.json`; Native seeds are exactly
  `coordinator/blind-seeds/<reviewerId>.seed`. The already validated reviewer-ID grammar makes each
  ID one safe path component. A Native seed file is exactly 32 raw bytes with owner-only mode;
  Replay never creates a seed directory.
- Reviewer slot order is the sealed attestation `reviewers` array order. Replay CLI seed `i` and
  Native 96-byte seed-set chunk `i` bind to reviewer slot `i`; reviewer preparation and
  `BlindPackReceipt.reviewerMappings` retain that order. Filesystem inventory enumeration remains
  lexical and does not redefine reviewer slot order.
- `A.json` and `B.json` are the exact retained compact producer-order package bytes selected by that
  reviewer's private mapping. `case.json`, `materials-manifest.json`, and every declared material
  retain their exact verified source bytes. `rubric.json` and
  `reviewer-submission.schema.json` retain their exact embedded bytes. All newly generated JSON
  (`review-bundle.json`, mappings, and receipt) is RFC 8785/JCS UTF-8 with no trailing LF.
- `caseSha256`, `materialsManifestSha256`, `aSha256`, `bSha256`,
  `reviewBundleSha256`, and `mappingSha256` hash the exact corresponding file bytes.
  `sourceMaterialsSha256` is recomputed as SHA-256 of the existing producer-order compact
  `serde_json::to_vec` encoding of the typed material manifest and must equal the sealed
  attestation commitment. `rubricSha256` is the already frozen rubric JCS commitment, while
  `reviewerSubmissionSchemaSha256` hashes the exact embedded schema bytes. The coordinator-only
  `decisionPolicySha256` is the policy JCS commitment; policy bytes never enter a visible bundle.
- A seed commitment is lowercase SHA-256 of
  `b"AI-IP-BLIND-SEED-COMMITMENT-V1\0" || seed32`. Replay seed bytes retain the planned
  `b"AI-IP-REPLAY-SEED-V1\0" || u64be(len) || seed_utf8` derivation. Orientation uses the
  workspace-locked rand 0.9 `StdRng::from_seed(seed32)` and `SliceRandom::shuffle` over the exact
  two-element `[Generic, Candidate]` array. The three seed bytes and commitments must be unique,
  and an all-three-identical orientation is invalid. Native samples a complete 96-byte set per
  attempt and accepts at most the first of 32 sets satisfying both rules; rejected sets are never
  persisted. Replay rejects rather than resamples.
- `review-bundle.json` includes the reviewer ID and exact qualification commitment, so bundle hashes
  remain reviewer-specific even when two mappings have the same orientation. Its
  `qualification` fields are copied from the sealed declaration; no signature-evidence body is
  reviewer-visible. Mapping, seed, condition names, private paths, logs, tokens, costs, durations,
  thread/turn/response identifiers, Skill data, and treatment markers never enter visible files.
- `BlindPackReceipt.generatedAt` is injected from a clock at commit time and must be canonical UTC
  RFC3339 with millisecond precision. Replay determinism applies to reviewer-visible bundles and
  mappings, not this private timestamp. `reviewsDropSha256` remains SHA-256 of JCS
  `{"entries":[]}`. `inventoryRootSha256` is the verified inventory prefix after all published
  directories are recorded and immediately before the receipt file is created.
- Fixed same-parent staging names are `.blind-pack-staging-reviewer`,
  `coordinator/.blind-pack-staging-mappings`,
  `coordinator/.blind-pack-staging-seeds`, and `.blind-pack-staging-reviews`; all must be absent in
  the initial preflight. A crash may leave one staging/final path, but the next invocation must
  detect inventory/output drift and reject without repair. Publication order is reviewer,
  mappings, optional Native seeds, reviews, then receipt. `AfterReviewerPublish`,
  `AfterMappingsPublish`, `AfterSeedsPublish`, and `AfterReviewsPublish` mean both the top-level
  no-replace rename and its exact expected-batch inventory append are durable; Replay never emits
  `AfterSeedsPublish`. Every tree also emits
  `AfterTreeRenameBeforeInventoryAppend(Reviewer|Mappings|Seeds|Reviews)` in the deliberately
  unrecorded-final-tree window. Receipt creation emits `AfterReceiptCreateBeforeInventoryAppend`
  after its exact file is durable and before the one-entry append. The remaining test-only
  boundaries are `BeforeFirstPublish` and `BeforeReceiptCreate`; production uses a no-op hook.
  Entropy and clock are likewise injectable only through crate-private/test seams, never through
  the CLI. Linux `renameat2` absence or `ENOSYS` fails closed before publication and must never
  fall back to replacement-capable `rename`.

**Files:**
- Create: `codex-rs/ai-ip-eval/src/secure_fs_publish.rs`
- Create: `codex-rs/ai-ip-eval/src/secure_fs_publish_tests.rs`
- Create: `codex-rs/ai-ip-eval/src/private_inventory_batch.rs`
- Create: `codex-rs/ai-ip-eval/src/private_inventory_batch_tests.rs`
- Create: `codex-rs/ai-ip-eval/src/blind_bundle.rs`
- Create: `codex-rs/ai-ip-eval/src/blind_bundle_tests.rs`
- Create: `codex-rs/ai-ip-eval/src/blind_bundle_model.rs`
- Create: `codex-rs/ai-ip-eval/src/blind_bundle_transaction.rs`
- Create: `codex-rs/ai-ip-eval/src/blind_bundle_transaction_tests.rs`
- Modify: `codex-rs/ai-ip-eval/src/secure_fs.rs`
- Modify: `codex-rs/ai-ip-eval/src/private_inventory.rs`
- Modify: `codex-rs/ai-ip-eval/src/contracts.rs`
- Modify: `codex-rs/ai-ip-eval/src/blind_finalize.rs`
- Modify: `codex-rs/ai-ip-eval/src/blind.rs`
- Modify: `codex-rs/ai-ip-eval/src/lib.rs`
- Modify: `codex-rs/ai-ip-eval/tests/blind_cli.rs`

**Interfaces:**
- Consumes: Task 5's `VerifiedBlindPair`, exact three reviewer declarations, Task 1 embedded contracts, Task 3 secure filesystem/inventory.
- Produces: three reviewer bundles, three private mappings, Replay/CSPRNG seeds, empty reviews drop, and manifest-bound `BlindPackReceipt`.

- [ ] **Step 1: Turn the valid-pair sentinel into transaction RED**

Extend the real binary test from Task 5 to assert exit `0`, exactly three reviewer
directories/mappings, exact receipt, and an empty `reviews/`. Before production changes it reaches
`BlindBundleStageNotInstalled`, so the test executes and fails behaviorally. The same real CLI test
must also:

- deep-equal each mapping's A/B assignment to the two exact retained compact package bytes and
  deep-equal the visible qualification, case, manifest, material, rubric, and submission-schema
  bytes to their frozen sources;
- recursively prove reviewer-visible bytes contain no condition name, seed, mapping, private path,
  Home/Skill/treatment marker, log, token, cost, duration, or thread/turn/response identifier;
- recompute every bundle, mapping, seed, contract, pair, frozen-context, and optional pair-receipt
  commitment in the receipt;
- hash the exact private-inventory prefix before its last receipt record and match
  `inventoryRootSha256`, then require final inventory verification and the receipt as the last
  record; and
- rerun the same fixture and Replay seeds below another private root and require all visible bundle
  and mapping bytes to be identical; rerun the original root and require rejection with every
  existing output and inventory byte unchanged.

- [ ] **Step 2: Generate seeds, mappings, and condition-hidden bundles**

Import `rand::TryRngCore`, `rand::SeedableRng`, and `rand::seq::SliceRandom`. Replay derives each `[u8;32]` as SHA-256 of `b"AI-IP-REPLAY-SEED-V1\0" || u64be(len) || seed_utf8`. Mock/Live CSPRNG samples a complete three-seed set in memory, with at most 32 attempts to avoid all-three-identical orientation; only the accepted set is persisted owner-only. CSPRNG failure/resample exhaustion fails before output. Replay rejects all-three-identical orientation; two matching orientations are valid.

Each reviewer directory contains exactly `A.json`, `B.json`, `case.json`, `materials/`, `materials-manifest.json`, reviewer-visible `rubric.json`, `reviewer-submission.schema.json`, and `review-bundle.json`. Copy only regular declared materials and recompute their aggregate digest. Reviewer IDs and qualification commitments come from the exact three typed attestation declarations. Replay bundle bytes are deterministic for the three explicit seeds and exclude wall clock, absolute path, and host identity.

```rust
struct ReviewerMapping {
    schema_version: u32,
    pair_id: String,
    reviewer_id: String,
    review_bundle_sha256: String,
    seed_commitment: String,
    a: EvaluationCondition,
    b: EvaluationCondition,
}
```

Mappings exist only in coordinator. Mapping/seed/condition never enters visible files.

- [ ] **Step 3: Bind the bounded transaction in an exact receipt**

Stage every complete directory below privateRoot, fsync it, rename to its exact final path, and append each final file/directory to the inventory. Create exact empty owner-only `reviews/`; define `reviewsDropSha256` as SHA-256 of JCS bytes `{"entries":[]}`. The operation is fail-closed and does not claim cross-directory atomicity: any pre-existing/partial final path makes rerun reject before further output.

```rust
struct BlindPackReceipt {
    schema_version: u32,
    pair_id: String,
    frozen_run_context_sha256: String,
    pair_receipt_sha256: Option<String>,
    pair_verification_sha256: String,
    rubric_sha256: String,
    decision_policy_sha256: String,
    reviewer_submission_schema_sha256: String,
    reviewer_mappings: Vec<ReviewerMappingCommitment>,
    inventory_root_sha256: String,
    reviews_drop_sha256: String,
    generated_at: String,
}

struct ReviewerMappingCommitment {
    reviewer_id: String,
    review_bundle_sha256: String,
    mapping_sha256: String,
    seed_commitment: String,
}
```

`inventoryRootSha256` is the verified inventory prefix immediately before the receipt record, as defined in Task 3. Replay has `pairReceiptSha256:null`; Mock/Live require a digest. Exactly three reviewer IDs, bundle hashes, mapping hashes, and seed commitments are unique where required. Score can detect an A/B swap by recomputing mapping SHA.

- [ ] **Step 4: Run the split transaction/retry matrix and commit**

Cover duplicate/two/four Replay seeds, Replay seed-dir, Mock/Live replay seeds, all-three Replay orientation, CSPRNG failure/exhaustion, existing destination, failure before first rename, failure between renames, failure before receipt, and rerun after partial/final output. Assert any partial result is detectable, never silently repaired, and no successful receipt exists until every bound output is durable.

The original single-commit block is superseded. Execute and stage the five review boundaries
exactly as follows; each production boundary runs its focused test and the scoped full crate before
the final `just fix -p codex-ai-ip-eval` and formatting sequence:

```bash
just test -p codex-ai-ip-eval -E 'test(secure_publish)'
just test -p codex-ai-ip-eval
git add codex-rs/ai-ip-eval/src/secure_fs_publish.rs \
  codex-rs/ai-ip-eval/src/secure_fs_publish_tests.rs \
  codex-rs/ai-ip-eval/src/secure_fs.rs codex-rs/ai-ip-eval/src/lib.rs
git commit -m "feat(ai-ip-eval): publish private trees without replacement"

just test -p codex-ai-ip-eval -E 'test(private_inventory_batch)'
just test -p codex-ai-ip-eval
git add codex-rs/ai-ip-eval/src/private_inventory_batch.rs \
  codex-rs/ai-ip-eval/src/private_inventory_batch_tests.rs \
  codex-rs/ai-ip-eval/src/private_inventory.rs codex-rs/ai-ip-eval/src/lib.rs
git commit -m "feat(ai-ip-eval): append private inventory batches"

just test -p codex-ai-ip-eval -E 'test(blind_bundle_preparation)'
just test -p codex-ai-ip-eval
git add codex-rs/ai-ip-eval/src/blind_bundle.rs \
  codex-rs/ai-ip-eval/src/blind_bundle_tests.rs \
  codex-rs/ai-ip-eval/src/blind_bundle_model.rs \
  codex-rs/ai-ip-eval/src/blind_finalize.rs \
  codex-rs/ai-ip-eval/src/contracts.rs codex-rs/ai-ip-eval/src/lib.rs
git commit -m "feat(ai-ip-eval): prepare deterministic blind review bundles"

just test -p codex-ai-ip-eval -E 'test(blind_cli) | test(blind_bundle_transaction)'
just test -p codex-ai-ip-eval
git add codex-rs/ai-ip-eval/src/blind_bundle_transaction.rs \
  codex-rs/ai-ip-eval/src/blind_bundle_transaction_tests.rs \
  codex-rs/ai-ip-eval/src/blind.rs codex-rs/ai-ip-eval/src/lib.rs \
  codex-rs/ai-ip-eval/tests/blind_cli.rs
git commit -m "feat(ai-ip-eval): create bound blind review bundles"

just test -p codex-ai-ip-eval -E 'test(blind_bundle_transaction) | test(blind_cli)'
just test -p codex-ai-ip-eval
git add codex-rs/ai-ip-eval/src/blind_bundle_tests.rs \
  codex-rs/ai-ip-eval/src/blind_bundle_transaction_tests.rs \
  codex-rs/ai-ip-eval/tests/blind_cli.rs
git commit -m "test(ai-ip-eval): harden blind bundle transaction failures"
```

---

### Task 7: Score exactly three reviews into an immutable private decision

> **Task 7 atomic implementation amendment (2026-08-29):** To keep every
> non-mechanical commit below the repository's 800 changed-line ceiling, Task 7
> is delivered as five behavior-complete slices: **7A** freezes the pure typed
> scoring table and body-free decision model; **7B** freezes the exact CLI/path
> and trusted-authority no-output boundary; **7C** verifies mappings,
> declarations, signed submissions, and immutable `INVALID_PROOF`; **7D** runs
> the real Replay CLI and the complete mutation table using the three committed
> review fixtures; **7E** appends F-0006 and runs the scoped/full regressions.
> This is a commit split only; Steps 1–5 and their exact classifications remain
> unchanged.

> **Task 7E interim OPEN ledger amendment (2026-08-29):** The first honest
> scoped/full regression found that the provider-free implementation and score
> selector are present, while the real Native test harness still exceeds the
> repository watchdog. To preserve that negative evidence append-only, one
> reviewed documentation commit may amend this plan and append F-0006 before
> GREEN only if F-0006 explicitly keeps Task 7E and Work Package 6A OPEN, gives
> exact commands/results/evidence location, makes no business or Phase 0B PASS
> claim, and changes no runtime, timeout, profile, provider, output, or private
> artifact. Step 5 remains incomplete. Its future closure must append F-0006A
> with the exact corrective commits and complete GREEN Cargo/Bazel evidence;
> F-0006 itself may never be rewritten to hide or reclassify these timeouts.

> **Task 7E lightweight Native App Server fixture corrective authority
> (2026-08-29):** Systematic diagnosis of the default-60-second Native
> failures found no business-logic deadlock. The test harness copies the
> approximately 103 MiB `codex-ai-ip-eval` unit-test executable and then uses
> that copy as the App Server input to every retained-handle, artifact-rehash,
> private-image, `fsync`, immutable-vnode, spawn, and close verification. A
> successful pair consequently performs about 3.1 GiB of equivalent reads and
> 318 MiB of writes before the fixed quiet scans complete; watchdog termination
> also bypasses normal temporary-image cleanup and makes later selectors
> progressively slower. Task 7E may correct only this test-infrastructure root
> cause by adding one dedicated small App Server fixture binary. The production
> LivePair path and all proof/security semantics remain unchanged.
>
> The fixture target is named exactly `ai-ip-native-app-server-fixture`. Its
> basename deliberately differs from the legacy
> `native-mock-app-server-test-harness` sentinel, so the unmodified App Server
> launcher must execute the normal production-shaped command
> `app-server --listen stdio:// --strict-config`, use stdin for requests and
> stdout JSONL for responses/notifications, and must not set or consume
> `AI_IP_NATIVE_APP_SERVER_FIXTURE`, libtest `--exact` arguments, or protocol
> fd 3. The fixture must fail closed on any other argv, record the exact argv in
> each isolated arm's `app-server-launch.json`, and preserve the existing
> initialize/config/Skill catalog/thread/raw-response/child-thread/quiet-window
> behavior. It may contact only the already frozen loopback proof broker from
> isolated `config.toml`; it adds no provider endpoint, key, paid request, or
> external network behavior.
>
> Cargo and Bazel must locate the same built target. Cargo/nextest uses
> `codex_utils_cargo_bin::cargo_bin("ai-ip-native-app-server-fixture")`.
> Bazel adds that same target to this package's `test_data_extra` and resolves
> the package-local runfile with `find_resource!` when runfiles are available;
> no shared Bazel macro may change. A crate-private `#[cfg(test)]` helper may
> locate and copy the fixture with owner-only executable mode. Its first real
> contract test must fail before the binary exists, then prove that the
> dedicated binary is a different file from the evaluator, is at most 64 MiB,
> rejects wrong argv, accepts the exact production argv, answers a real
> initialize request over stdout JSONL, and records that argv. The 64 MiB bound
> is a test-infrastructure I/O budget, not a product or business limit.
>
> The three Native execution copy points in `eval_tests.rs` and
> `proof_archive_tests.rs` must switch from `std::env::current_exe()` to the
> dedicated fixture. The old embedded `native_app_server_fixture` libtest may
> then be deleted in a separate mechanical commit. The actual evaluator
> commitment remains the current test executable; fixture and evaluator
> commitments stay distinct. Every Native test still performs an independent
> freeze, creates a fresh private executable image for each arm, retains and
> re-verifies its descriptor, validates the kernel-loaded vnode, runs both
> quiet scans, and re-reads ledger, manifests, arm/pair receipts, archive, and
> inventory. No pair result, private image, receipt, manifest, inventory, or
> verification result may be cached or shared across tests or arms.
>
> The first corrective round may modify only this plan plus
> `codex-rs/ai-ip-eval/Cargo.toml`, `codex-rs/ai-ip-eval/BUILD.bazel`, new
> sources below
> `codex-rs/ai-ip-eval/src/bin/ai_ip_native_app_server_fixture/`, one optional
> `codex-rs/ai-ip-eval/src/native_app_server_fixture.rs` test helper,
> `codex-rs/ai-ip-eval/src/lib.rs` solely to mount that helper,
> `codex-rs/ai-ip-eval/src/eval_tests.rs`, and
> `codex-rs/ai-ip-eval/src/proof_archive_tests.rs`. Cargo or Bazel lock files
> may change only when their own required tooling generates a necessary
> update. Every new source file remains below 500 lines and each independent
> non-mechanical commit remains below 800 changed lines. The required order is:
> (1) this authority-only commit; (2) a behavioral RED that fails because the
> dedicated target is absent; (3) the smallest fixture/Cargo/Bazel/helper
> GREEN; (4) switch all three execution copy points and remove the embedded
> fixture; (5) focused/default-60/full regression evidence; (6) append-only
> F-0006A if and only if all closure checks are GREEN.
>
> This authority explicitly forbids changes to `runner.rs`, `app_server.rs`,
> `.config/nextest.toml`, CLI/wire/schema/provider/output/security policy,
> deadlines, retries, test-thread settings, quiet windows, artifact rehashes,
> SHA checks, `fsync`, immutable/private-image/vnode validation, Replay
> fixtures, private evidence, and existing F-0006 bytes. If the dedicated
> production-argv/stdout fixture cannot be integrated without modifying either
> forbidden Rust module, stop and obtain a new reviewed authority rather than
> using the legacy test-only launcher branch. Moving the evaluator commitment
> to the fixture, stripping the old test executable, or widening the watchdog
> are not authorized fallbacks.
>
> F-0006A may close Task 7E only after clean serial `retries=0` runs under the
> unchanged default 60-second watchdog pass the formerly timing-out combined
> exact-envelope selector, immediate generic Skill poison, late quiet-window
> generic Skill poison, the full evaluator suite, the proxy suite, all four
> planned evaluator Bazel targets, schemas/rubrics builds, and lock checks.
> It records exact commands/counts/timings, corrective commits, fixture launch
> contract and size, rollback order, and `providerMode=not-run`, paid cost zero,
> loopback-only/no-key/no-customer-material disposition. F-0006A still leaves
> G0/G1/G2 and `PASS_TO_PHASE_0B` open unless their separate gates are met.

> **Task 7E dedicated-fixture weight corrective authority (2026-08-29):**
> Task 8's first full serial evaluator run after `d6c51eacb` is a new
> append-only RED, not closure evidence. Under locked `cargo-nextest 0.9.103`,
> the unchanged default 60-second watchdog, `--test-threads=1`, and
> `--retries=0`, run `57c6b74e-aa4a-4185-aa40-c960693de795` executed 284 tests
> in 3976.108 seconds: 258 passed (20 slow), 26 timed out, zero assertion
> failures, and zero LEAK labels. The proxy, Bazel, lock, fix, and format gates
> were correctly not started. Exact names, timings, first-failure sequence, and
> environment are retained in `task-8-closure-validation-report.md`.
>
> The dedicated fixture correction is directionally correct but not yet small
> enough for the complete suite. Its Cargo Mach-O is 19,870,672 bytes, including
> approximately 8.95 MiB of `__LINKEDIT`, and it links the full reqwest/TLS
> client only to call the already-authorized numeric loopback HTTP broker. A
> standalone strip diagnostic still measured 17,236,848 bytes when removing
> debug information and 15,941,160 bytes when also removing local symbols, so
> stripping alone is insufficient. The failed full run left 22 exact
> fixture-bearing TempDir roots totalling approximately 0.564 GiB; later Replay
> and score tests timed out only after the earlier Native timeout cascade. Disk
> capacity, sustained I/O saturation, and VM throttling were not observed.
> This supports, but does not uniquely prove, cumulative fixture materialization
> and timeout-residue pressure as the shared infrastructure cause. No business
> assertion failed.
>
> One second corrective round may reduce only the dedicated fixture's test I/O
> weight. It may modify this plan, a new
> `codex-rs/ai-ip-eval/build.rs`,
> `codex-rs/ai-ip-eval/BUILD.bazel`,
> `codex-rs/ai-ip-eval/src/bin/ai_ip_native_app_server_fixture/wire.rs`, and the
> existing direct fixture contract in
> `codex-rs/ai-ip-eval/src/eval_tests.rs`. `Cargo.toml` and its reqwest/toml
> dependencies remain unchanged because production evaluator code still uses
> them and Cargo auto-discovers `build.rs`. The round may not modify `server.rs`,
> `runner.rs`, `app_server.rs`, the locator/copy helper, shared Bazel macros,
> nextest configuration, any timeout/retry/thread/quiet-window value, any
> production binary, or any proof/security/provider/schema/output/business
> behavior. The evaluator and every non-fixture target retain their normal
> debug/link settings.
>
> Cargo may add one package build script which emits only
> `rustc-link-arg-bin=ai-ip-native-app-server-fixture=...` platform linker strip
> arguments: Darwin may omit debug and local symbols, GNU-family linkers may
> strip the dedicated fixture, and unsupported targets must safely no-op rather
> than guess a linker flag. Bazel must disable that build script, because this
> repository's rules_rust path intentionally drops per-bin build-script
> directives, and use the existing `binary_rustc_flags_extra` map to pass
> `-Cstrip=symbols` only to `ai-ip-native-app-server-fixture`. This follows the
> repository's existing `windows-sandbox-rs` per-binary Cargo/Bazel pattern.
> Removing symbols is acceptable only for this deterministic test fixture; it
> is not a security feature and may not apply to the evaluator, product CLI,
> library, unit-test binary, or integration-test binaries.
>
> The fixture may replace its reqwest call with a small `std::net::TcpStream`
> HTTP/1.1 client only for the exact frozen broker form
> `http://127.0.0.1:<u16>/v1`. It must reject any other scheme, host, missing
> port, base path, query/fragment, or header control character. It appends the
> exact `/responses` suffix to produce request target `/v1/responses`, preserves
> the exact JSON body and proof-relevant `x-codex-window-id` plus conditional
> `x-codex-parent-thread-id` / `x-openai-subagent` headers, and emits valid
> `Host`, `content-type`, `Content-Length`, and `Connection: close` transport
> headers. It must require a final HTTP/1.0 or HTTP/1.1 2xx status, reject
> ambiguous/conflicting content-length and
> transfer-encoding framing, and support the local broker's single valid
> content-length or chunked SSE response including chunk extensions/trailers.
> It may use neither a proxy nor TLS and may add no socket,
> product, or test deadline. The existing TOML/config, stdout JSONL
> synchronization, argv gate, App Server methods, package bytes, broker
> accounting, receipts, poison behavior, and quiet-window behavior remain
> byte/semantics compatible. This correction only prevents the dedicated
> fixture binary from linking unused HTTP/TLS machinery.
>
> TDD order is mandatory. First tighten the direct contract so the current
> macOS Cargo/Bazel fixture fails a local test-I/O bound of 12 MiB while all
> other platforms retain the reviewed 64 MiB cross-platform ceiling. Weight and
> raw-loopback behavior require independently selectable sibling REDs: one must
> fail only because the current fixture exceeds the macOS 12 MiB bound, and one
> must prove that the current reqwest fixture reaches a controlled host-local
> listener through a non-loopback/unspecified address when the expected new
> behavior is to reject before connect. The listener and address remain wholly
> host-local and the test must never attempt an external connection. Additional
> contract coverage freezes the exact request target, headers and body,
> content-length and chunked SSE framing, and malformed/conflicting framing
> rejection. No RED may fail merely because an API is missing or be masked by
> the other size/behavior RED. Then add the smallest
> build-script/Bazel/raw-loopback implementation. GREEN must prove the direct
> wrong-argv/initialize/broker contract, exact Cargo and Bazel fixture sizes,
> the production-shaped typed happy path, archive, immediate and quiet-window
> poison, combined Replay/Native envelope, and representative Native receipt,
> bundle, and finalizer paths under the unchanged watchdog. Each independent
> commit remains below 800 changed lines and every source remains below 500.
>
> Before the next full-suite attempt, and only with no Rust test process
> active, cleanup must first write an exact manifest containing every absolute
> source root, fixture SHA-256, byte size, owner, and intended Trash destination.
> Every source must be an immediate owned directory below the current user's
> resolved TMPDIR, not a symlink, and contain the expected regular fixture plus
> recognizable Task 7E test-root structure. Create one unique named Trash
> directory, move only manifest-listed roots one by one with collision failure,
> record every source-to-destination mapping and aggregate size, and verify every
> original path is absent. No glob, broad `find | mv`, recursive deletion, or
> unmanifested root is authorized. The corrective report and future F-0006A
> retain the manifest, recovery destination, cleanup receipt, exact fixture
> section/symbol diagnostics, strip measurements, and the commands/output used
> to observe disk, I/O, and VM state. The subsequent full evaluator run must
> start from that clean test-owned residue state and still use locked 0.9.103,
> serial execution, zero retries, and the default 60-second watchdog. F-0006A
> remains forbidden until that full run and every previously specified proxy,
> Bazel, resource, lock, scope, fix/format, and independent-review gate is
> GREEN. F-0006 itself remains immutable.

> **Task 7E exact-envelope selector budget corrective authority
> (2026-08-29):** After timeout-residue cleanup, locked serial run
> `a8f3d671` still timed out
> `blind_pair_parser_retains_exact_real_replay_and_native_envelopes` at
> 60.061 seconds. The selector serializes a complete real Replay pair and a
> complete real Native pair under one unchanged default-60-second watchdog;
> the retained Native evidence reached its second run manifest at the watchdog
> boundary without exposing an assertion or product failure. One narrow
> test-only correction may modify only `blind_verify_tests.rs` and this plan to
> split that selector into independently watched Replay-envelope and
> Native-envelope tests. Every existing assertion must remain byte-for-byte
> unchanged. No helper, producer, parser, runner, fixture, product, timeout,
> retry, nextest profile, thread setting, proof/security rule, or business
> behavior may change. Run `a8f3d671` remains the authoritative RED; GREEN
> requires a fresh dedicated-fixture build followed by separate locked serial,
> retries-zero runs of both new selectors under the unchanged watchdog.

> **Task 8 Cargo/Bazel exact-wire feature-invariance corrective authority
> (2026-08-30):** The clean Task 8 Cargo runs are GREEN (`292/292` evaluator
> and `22/22` proxy), but Bazel invocation
> `3a6eb425-d741-4dd4-ad8b-f8afd0e50fc3` is the authoritative cross-build
> RED: the binary unit target passed while evaluator unit tests reported
> `242 passed; 44 failed`, blind CLI `0/2`, and score CLI `0/4`. The first
> shared failure is `Replay execution context is not exact producer-order
> typed JSON without trailing bytes` or its Native equivalent. Read-only
> diagnosis proved both builds use `serde_json 1.0.149`, while this Cargo
> package graph enables only `default,std` and the workspace-generated Bazel
> crates repository additionally enables `indexmap,preserve_order`. The four
> exact typed documents still produced through `serde_json::Value` are Replay
> and Native execution context plus Replay and Native pair verification. In
> Cargo their object maps are already lexically ordered; in Bazel they retain
> the non-typed literal/append order. Native execution also contains a nested
> broker object with the same mismatch. Pretty formatting, trailing bytes,
> concurrency, fixture behavior, and serde_json version are not the cause.
>
> One narrow correction may modify only this plan,
> `codex-rs/ai-ip-eval/src/runner.rs`, and existing sibling tests in
> `eval_tests.rs` or `blind_verify_tests.rs`. It may add one local serializer
> which recursively sorts every object in an owned `serde_json::Value` before
> existing pretty serialization, and route exactly those four producer
> documents through it. This makes their already authoritative Cargo bytes
> independent of dependency feature unification, including nested objects and
> fields appended after literal construction. It may not change typed parser
> structs, exact byte equality, duplicate/unknown/trailing-byte rejection,
> schemas, hashes, inventory, provider/broker behavior, public output,
> deadlines, retries, threads, nextest/Bazel configuration, shared macros,
> serde dependency features, lock files, or any other producer. It may not
> disable Bazel `preserve_order` or enable it for Cargo as a substitute.
>
> TDD retains the actual Bazel failure above as behavioral RED and adds focused
> feature-invariance coverage for top-level, nested, and appended object keys;
> existing non-producer-order negative coverage must stay strict. GREEN first
> requires the two locked serial exact Replay/Native envelope selectors, then
> focused Bazel proof that the prior shared gate passes, followed by the exact
> four-target Task 8 Bazel command. Full Cargo suites need not be repeated
> because the correction is exercised by the focused real-pair selectors and
> those complete suites passed immediately before the RED. All later resource,
> lock, scope, final fix/format, independent-review, and F-0006A gates remain
> required and F-0006 remains immutable.

> **Task 8 Bazel harness completion corrective authority (2026-08-30):**
> After the exact-wire correction passed focused Cargo and Bazel Replay/Native
> evidence chains, combined Bazel invocation
> `58e72857-a771-4426-a121-db24e1d8ec80` removed every prior exact-wire
> failure. The binary unit target passed, blind CLI passed `2/2`, score CLI
> passed `3/4`, and the library unit log completed 216 tests without an
> assertion failure before Bazel killed the aggregate target at exactly the
> `long` 900-second outer budget. The sole score failure was independently
> reproduced by invocation `c01d94a8-a77d-4d22-b0eb-7672c0608b89` in 53.84
> seconds: `score_cli_rejects_frozen_codex_byte_drift_without_side_effects`
> returned `Permission denied`. Bazel materializes the evaluator runfile as
> mode `0555`; the test copies that executable into its private TempDir while
> preserving mode, then intentionally performs an in-place byte mutation, so
> the test helper cannot open its own isolated copy for writing. Cargo's source
> executable is owner-writable, which is why the same contract passed there.
> This is test-harness portability, not a product permission or proof failure.
>
> One test-only correction may additionally modify
> `codex-rs/ai-ip-eval/tests/support/score_world.rs` and
> `codex-rs/ai-ip-eval/BUILD.bazel`. Immediately after copying the exact
> evaluator into the test-owned TempDir, the helper may add only the owner
> write bit to that destination on Unix, and clear read-only only on non-Unix,
> before the frozen context is created. It must not chmod or rewrite the Bazel
> runfile, replace the destination pathname/inode, weaken the production
> retained-handle byte/identity check, or change any product path, permission,
> provider, security, scoring, or output behavior. The existing test must still
> mutate the same frozen inode and receive exactly `frozen replay Codex binary
> bytes or identity changed` with no decision side effect.
>
> The package-local Bazel `unit_test_timeout` attribute may change from `long`
> to `eternal`, while `unit_test_args = ["--test-threads=4"]` remains exact.
> The repository macro applies that one attribute to both the library-unit
> outer target and this package's generated binary-unit outer targets; this
> authority explicitly covers those aggregate Bazel wrappers, and the final
> four-target gate must still prove the evaluator binary-unit target passes.
> No Rust test body or binary product behavior receives this timeout. The
> change does not alter any product deadline, per-test watchdog, thread count,
> test body, retry, fixture, or assertion. Do not add `test_shard_counts`: the
> current workspace launcher
> treats the nonempty unit-test args as an ad-hoc filter and would run all 287
> tests once per shard, while the shared macro would also mark shards flaky and
> introduce default retries. Do not modify the shared macro, raise test
> concurrency, add retry, or use a repeated PASS as closure evidence.
>
> GREEN requires the focused Cargo and Bazel frozen-Codex drift selector, full
> Bazel score CLI `4/4`, isolated Bazel library unit `287/287` within its actual
> completion time, and then the same four-target Task 8 command including the
> generated evaluator binary-unit target. Every target
> must pass on its first attempt. Only then continue to resource, lock, scope,
> final fix/format, independent-review, and F-0006A gates.

> **Task 7A measured split amendment (2026-08-29):** The reviewed 7A diff is
> 919 changed lines, so 7A is committed as **7A1** (`score.rs`, its sibling
> scoring-table tests, and their `lib.rs` mount) and **7A2**
> (`score_decision.rs`, its sibling exact-wire tests, and their `lib.rs` mount).
> Each subcommit is independently green and below 800 changed lines; no test or
> readable contract is compressed to manufacture compliance.

> **Task 7 score authority and wire amendment (2026-08-29):** The following is
> the exact authority missing from the original prose. Task 7 may additionally
> create `score_authority.rs`, `score_decision.rs`, `score_transaction.rs` and
> sibling test modules, and may narrowly modify `contracts.rs`,
> `blind_bundle_model.rs`, `blind_bundle_transaction.rs`, `blind_verify.rs`,
> `private_inventory.rs`, and their sibling tests to share producer wire types
> and add the Score continuation verifier. No Task 6 producer bytes may change.
>
> `BlindDecision` is canonical JCS with no trailing LF and this exact closed
> producer-order-independent field set:
>
> ```text
> schemaVersion: 1
> pairId: string
> frozenRunContextSha256: lowercase SHA-256
> blindPackReceiptSha256: lowercase SHA-256 of exact receipt bytes
> reviewerMappingsSha256: lowercase mapping-set commitment
> reviewSubmissionsSha256: lowercase review-set commitment
> rubricSha256: lowercase SHA-256
> decisionPolicySha256: lowercase SHA-256
> decision: PASS | ITERATE_SMALLEST_LEAD_CHANGE | INVALID_PROOF
> metrics: DecisionMetrics | null
> validationFailures: BlindDecisionFailure[]
> generatedAt: canonical UTC RFC3339 milliseconds
> ```
>
> `DecisionMetrics` has exactly `reviewerCount`,
> `experiencedOperatorOrDirectorCount`, `candidatePreferenceCount`,
> `candidateReadyForHumanReviewCount`, ascending `genericTotals[3]`, ascending
> `candidateTotals[3]`, ascending signed `pairedDeltas[3]`,
> `medianGenericTotal`, `medianCandidateTotal`, `medianPairedDelta`,
> `candidateSevereFailureCount`, and `candidateSevereFlags`. The last object has
> exactly the four rubric flag names, each holding its candidate-side count.
> The paired-delta array is formed reviewer-by-reviewer before sorting; it is
> not the difference between two medians.
>
> `BlindDecisionFailure` has this declared order and exact wire spelling:
>
> ```text
> REVIEW_SUBMISSION_INVALID
> REVIEWER_ID_SET_MISMATCH
> REVIEWER_QUALIFICATION_MISMATCH
> REVIEW_MAPPING_COMMITMENT_MISMATCH
> REVIEW_BUNDLE_COMMITMENT_MISMATCH
> REVIEW_SEED_COMMITMENT_MISMATCH
> REVIEW_RUBRIC_COMMITMENT_MISMATCH
> INSUFFICIENT_EXPERIENCED_REVIEWERS
> ```
>
> Failures are deduplicated and emitted in that declared order. Valid PASS or
> ITERATE has non-null metrics and `[]`; INVALID has `metrics:null` and at least
> one failure. `reviewerMappingsSha256` and `reviewSubmissionsSha256` commit the
> actual raw bytes, including bytes later found semantically invalid. For each
> set, sort the three expected reviewer IDs by UTF-8 bytes and hash:
> `domain || Σ(u32be(id_len) || id_utf8 || SHA256(raw_file_bytes))`, using the
> domains `AI-IP-BLIND-MAPPING-SET-V1\0` and
> `AI-IP-BLIND-REVIEW-SET-V1\0` respectively. The NUL is one byte. The decision
> contains no reviewer ID/file name, qualification text, A/B value, reason,
> signature evidence, body, case/material/path, seed/raw mapping, private-root
> or Home, Skill, token/cost, thread/response, provider log, or public claim.
>
> Score path spellings are exact. `--mapping-dir` accepts only raw relative
> `coordinator/mappings`. `--reviews-dir` accepts raw relative `reviews` or the
> exact canonical absolute `privateRoot/reviews`. `--output` accepts raw
> relative `coordinator/decision.private.json` or the exact canonical
> coordinator directory plus the absent leaf `decision.private.json`.
> `--frozen-run-context` is the existing canonical absolute
> `privateRoot/frozen-run-context.json`. Dot segments, alternate separators,
> links/reparse points, hardlinks, non-UTF-8 ambiguity, and any other spelling
> are rejected.
>
> The Score continuation verifier first re-verifies the retained frozen input,
> current evaluator/Codex/broker sources, sealed pair, exact typed blind receipt,
> receipt-as-current-inventory-tail, and private-root identity. It then permits
> exactly three unrecorded leaves named `reviews/<attestedReviewerId>.json` in
> the already-recorded owner-only drop directory. Each leaf must be a
> single-link regular file no larger than 64 KiB; leaf mode need not be `0600`
> because the containing drop directory is owner-only. Missing/extra/special,
> unreadable, over-cap, or identity-changing review leaves are command errors
> with no decision. It retains the exact three file handles/bytes and the
> inventory cursor through validation and publication. It does not call the
> strict pre-review inventory verifier after the operator drop.
>
> Submission JSON/schema/digest/signature and all semantic bindings are checked
> only after that trusted continuation exists; their failures write immutable
> INVALID_PROOF. Command-authority errors always take precedence. The decision
> transaction writes an owner-only staging file at
> `coordinator/.decision.private.json.staging`, fsyncs it, re-verifies all
> retained authority, publishes no-replace to the exact final leaf, fsyncs the
> coordinator, then appends the three review leaves and decision leaf as one
> inventory batch and verifies the new root. Any pre-publish I/O/fsync failure
> leaves no final output. A failure after the no-replace publish boundary may
> leave a detectable unrecorded final file; it is not a valid decision and a
> rerun refuses without overwriting or repairing it. This narrow durable-partial
> rule supersedes the original physically impossible blanket “any I/O failure,
> no output” wording while preserving fail-closed business semantics.

> **Task 7 shared safety-seam amendment (2026-08-29):** Task 7 may narrowly
> modify `secure_fs.rs`, `secure_fs_retain.rs`, `secure_fs_publish.rs`, and their
> sibling tests rather than duplicate platform code in Score. The retained-read
> seam accepts a caller cap and a policy that may waive owner-only mode for a
> leaf inside an already verified owner-only directory; it still requires a
> no-follow single-link regular file, retains its handle and identity, and on
> reverify rejects path/parent replacement or byte drift. Score uses a 64 KiB
> cap and only the waived-leaf-mode policy for the three review leaves. The
> no-replace publish seam gains an exact regular-file variant that reuses the
> existing anchored parent/source identity and platform-exclusive rename
> machinery, proves the retained staging inode became the final inode, and
> fsyncs the parent. It never falls back to overwrite or copy. These shared
> seams and their cross-platform/failure sibling tests may be delivered as
> 7B0 commits before the Score continuation. Until 7C installs semantic review
> validation, a completed 7B authority check may return the exact transitional
> error `score review validation stage is not installed`; it creates neither
> staging nor final decision and is removed with the first 7C consumer.

> **Task 7B sealed-authority consuming-seam amendment (2026-08-29):** 7B may
> narrowly modify `blind.rs`, `blind_verify.rs`, `blind_finalize.rs`,
> `private_inventory_batch.rs`, and the existing `tests/blind_cli.rs` harness.
> These changes may only project one already verified frozen token, consume the
> already verified review-inventory continuation in the sealed-pair verifier,
> split sealed-pair revalidation from Task 6 destination absence, and prove the
> real Replay command reaches the exact no-output sentinel. They must not parse
> review semantics, classify `INVALID_PROOF`, write an output, or install any
> Task 7C behavior. Every production module remains below 500 lines and the
> independent 7B commit remains below 800 changed lines.

> **Task 7C measured semantic/transaction split (2026-08-29):** Read-only
> preflight measured 630–785 changed lines before the complete classification
> matrix and found that the required post-staging authority recheck cannot use
> the old inventory token: the exact staging file is intentionally present but
> not yet recorded. 7C is therefore delivered in two dependency-ordered commits.
> 7C1, `feat(ai-ip-eval): verify blind review semantics`, may add
> `score_validation.rs` and its sibling tests and narrowly add `Deserialize`
> support to the existing Task 6 mapping/bundle wire types. It is a pure
> consumer: it validates retained raw mapping, bundle, seed and submission
> evidence, returns either mapped reviews or the ordered INVALID failure set,
> and writes no file. 7C2, `feat(ai-ip-eval): publish immutable blind decisions`,
> may add `score_transaction.rs` and sibling tests and narrowly modify
> `score_authority.rs`, `blind_finalize.rs`, `private_inventory_batch.rs`,
> `lib.rs`, and the existing `tests/blind_cli.rs` smoke test. Its inventory seam
> may accept only one exact retained staging leaf and digest while rechecking
> every prior recorded entry and the three retained reviews; it must not expose
> a general ignored-path API. 7C2 removes the transitional sentinel and proves
> the current schema-invalid Replay placeholders produce an immutable private
> INVALID_PROOF. 7D alone replaces the committed review fixtures and owns the
> real valid Replay and complete mutation matrix. Neither 7C commit changes Task
> 6 producer bytes, provider behavior, public reporting, or F-0006. Every new
> production module remains below 500 lines and each commit below 800 changed
> lines.

**Files:**
- Create: `codex-rs/ai-ip-eval/src/score.rs`
- Create: `codex-rs/ai-ip-eval/src/score_tests.rs`
- Create: `codex-rs/ai-ip-eval/tests/score_cli.rs`
- Modify: `codex-rs/ai-ip-eval/src/model.rs`
- Modify: `codex-rs/ai-ip-eval/src/lib.rs`
- Modify: `codex-rs/ai-ip-eval/tests/fixtures/replay-review-1.json`
- Modify: `codex-rs/ai-ip-eval/tests/fixtures/replay-review-2.json`
- Modify: `codex-rs/ai-ip-eval/tests/fixtures/replay-review-3.json`
- Modify: `docs/architecture/codex-fork-patch-ledger.md`

**Interfaces:**
- Consumes: frozen context, attestation role, exact blind receipt, three committed mappings/bundles, exact reviews directory, Task 1 contracts/policy.
- Produces: `ScoreArgs`, `BusinessDecision`, body-free private `BlindDecision`, and F-0006 implementation disposition.

- [ ] **Step 1: Write exact real CLI RED and decision table**

After the Replay blind pack, copy the three full schema-valid synthetic submissions and run exactly:

```text
score
--mapping-dir coordinator/mappings
--reviews-dir <privateRoot/reviews>
--output <privateRoot/coordinator/decision.private.json>
--frozen-run-context <absolute-context>
```

Current code compiles/executes but rejects/unimplements the command. Add hand-derived decision rows for preference `1/2`, delta `2/3`, candidate median `17/18`, ready `1/2`, every severe flag, tie, duplicate reviewer, wrong bundle/rubric/mapping hash, flipped qualification field with copied signed digest, malformed review, and current evaluator mismatch. The mismatch row expects command error and no output.

- [ ] **Step 2: Add exact CLI and outcome classification**

```rust
struct ScoreArgs {
    mapping_dir: PathBuf,
    reviews_dir: PathBuf,
    output: PathBuf,
    frozen_run_context: PathBuf,
}

enum BusinessDecision {
    Pass,
    IterateSmallestLeadChange,
    InvalidProof,
}
```

Accept mapping dir only at `privateRoot/coordinator/mappings`. Accept reviews/output as either the exact relative spelling used in WP8 or the exact canonical absolute path used in WP7; both must resolve to `privateRoot/reviews` and `privateRoot/coordinator/decision.private.json`.

Freeze outcome classification:

```text
command error, no output:
- frozen context or pair marker unsafe/unparseable
- current evaluator/Codex binary or broker component source differs from the frozen commitment
- blind receipt unsafe/unparseable or not the derived exact file
- reviews directory not yet exactly three regular files
- output exists, unsafe path, or any I/O/fsync failure

immutable INVALID_PROOF:
- after trusted context+receipt, any submission JSON/schema/digest/signature invalid
- reviewer ID set/qualification/experience disclosure mismatch
- mapping/bundle/seed/rubric commitment mismatch
- fewer than two matching experienced reviewers

valid ITERATE:
- any candidate fabricatedFactualClaim=true
- or any other valid result below PASS thresholds
```

Tests assert one exact outcome per mutation; no `error || INVALID` assertions.

- [ ] **Step 3: Verify mappings, declarations, and signed payloads before unblinding**

Recompute each mapping file SHA and compare it to the receipt before using A/B. Require exact equality among attestation reviewer IDs, receipt reviewer IDs, mapping reviewer IDs, and submission reviewer IDs. Recompute attestation/submission domain-separated JCS signed payload digests and match frozen signature-evidence SHA. Compare qualification class and experience boolean field-for-field. A copied valid digest with changed ID/class/experience/bundle/time/arms is invalid.

- [ ] **Step 4: Apply exact scoring algorithm**

For each reviewer, map A/B to generic/candidate using only that reviewer's verified mapping. Checked-sum six dimensions. Sort three generic totals, candidate totals, and signed deltas; median is index `1`. Candidate preference count excludes ties. Count ready and each severe flag.

```rust
if proof_invalid {
    BusinessDecision::InvalidProof
} else if candidate_fabricated_count > 0 {
    BusinessDecision::IterateSmallestLeadChange
} else if candidate_preference_count >= 2
    && median_paired_delta >= 3
    && median_candidate_total >= 18
    && candidate_ready_count >= 2
    && candidate_severe_total == 0
{
    BusinessDecision::Pass
} else {
    BusinessDecision::IterateSmallestLeadChange
}
```

Write owner-only with `create_new`. `BlindDecision` contains commitments, decision, timestamp, `metrics: DecisionMetrics|null`, and exact body-free validation failure codes. PASS/ITERATE require non-null counts/totals/deltas/medians/severe metrics and an empty failure list; INVALID requires `metrics:null` and at least one enum failure code, so zero is never used as a fake score. It contains no reasons, bodies, paths, seeds, raw mappings, or public report. Replay may exercise private `PASS` mechanically, but no 06A command emits `BUSINESS_SIGNAL_PASS_PENDING_FOUNDATION` or `PASS_TO_PHASE_0B`.

- [x] **Step 5: Update Replay evidence, run GREEN, append ledger, commit**

Replace all three Replay review fixtures with full valid submissions bound to Task 6's deterministic bundle/mapping/rubric values. These reviewer submissions are post-pair inputs and are not roles in `replay-fixture-set.json`; do not mutate the frozen execution fixture-set.

```bash
just test -p codex-ai-ip-eval -E 'test(score_cli)'
just test -p codex-ai-ip-eval
git add codex-rs/ai-ip-eval/src/score.rs codex-rs/ai-ip-eval/src/score_tests.rs \
  codex-rs/ai-ip-eval/src/model.rs codex-rs/ai-ip-eval/src/lib.rs \
  codex-rs/ai-ip-eval/tests/score_cli.rs \
  codex-rs/ai-ip-eval/tests/fixtures/replay-review-1.json \
  codex-rs/ai-ip-eval/tests/fixtures/replay-review-2.json \
  codex-rs/ai-ip-eval/tests/fixtures/replay-review-3.json \
  docs/architecture/codex-fork-patch-ledger.md
git commit -m "test(ai-ip-eval): freeze private blind review decision"
```

F-0006 records exact Task 1–7 commits, private/public boundary, tests, provider not-run/cost zero, parked WP8 blockers, sync risk, and reverse rollback order. If the interim OPEN authority above is used, Step 5 remains unchecked until a later append-only F-0006A records complete GREEN evidence.

---

### Task 8: Verify the complete provider-free WP6A boundary

**Files:**
- Modify only when generated by required tooling: `codex-rs/Cargo.lock`
- Modify only when generated by required tooling: `MODULE.bazel.lock`

**Interfaces:**
- Consumes: Tasks 1–7 and existing proxy/evaluator contracts.
- Produces: clean reviewed WP6A tip; no provider spend or public business claim.

- [x] **Step 1: Run Cargo regressions**

```bash
cd codex-rs
just test -p codex-ai-ip-eval
just test -p codex-responses-api-proxy
cd ..
```

- [x] **Step 2: Run Bazel and resource matrix**

```bash
just bazel-lock-update
bazel test //codex-rs/ai-ip-eval:ai-ip-eval-unit-tests \
  //codex-rs/ai-ip-eval:codex-ai-ip-eval-bin-unit-tests \
  //codex-rs/ai-ip-eval:ai-ip-eval-blind_cli-test \
  //codex-rs/ai-ip-eval:ai-ip-eval-score_cli-test
bazel build //ai-ip-evals/rubrics:rubrics //ai-ip-evals/schemas:schemas
just bazel-lock-check
```

- [x] **Step 3: Verify scope/governance before formatting**

```bash
git diff --check
git status --short
git diff --name-only eea75f7771bcfafea5bb9107d329a10d5b4b6429..HEAD
rg -n 'providerMode=not-run|paidProviderCost=0|same-UID|ancestor-swap' \
  docs/architecture/codex-fork-patch-ledger.md
```

Expected: only planned paths; ledger explicitly preserves zero provider use/cost and both blockers. Inspect staged/untracked paths before every commit; never use a directory-wide `git add` that can capture unrelated user files.

- [x] **Step 4: Run formatter/fixer last**

```bash
cd codex-rs
just fix -p codex-ai-ip-eval
just fmt
cd ..
```

Run no test after this step.

- [x] **Step 5: Retain generated output only if present**

Stage only the exact modified formatter/lock files shown by `git status`; inspect `git diff --cached --check`, then commit:

```bash
git diff --cached --quiet || git commit -m "style(ai-ip-eval): retain blind review formatter output"
```

Expected: clean worktree. After two-stage task reviews and a broad final review with no open Critical/Important findings, mark 06A complete and authorize only reviewed 06B. Do not mark Work Package 6, G0–G2, `PASS_TO_PHASE_0B`, or product Plan 02 complete.
