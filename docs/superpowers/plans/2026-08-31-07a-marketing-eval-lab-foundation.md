> **RETIRED — RETIRED_NOT_PASSED (2026-09-05).** This evaluator/proof document is historical and is not executable. All commands, prerequisite gates and successor authorizations below are retired by the approved business-first cleanup; prior results and failures remain historical claims only. Do not resume Phase 0A, 06A, 06B, 07A, 07B or LH1 from this record. Retirement grants no business PASS, model qualification, spending, publication, customer-data access or customer-release authority. Current authority: `docs/superpowers/specs/2026-09-05-business-first-cleanup-design.md` and `docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md`.

# Marketing Evaluation Lab Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a provider-free, offline Marketing Capability Evaluation Lab foundation that compiles the frozen golden-gift evidence into physically separated exam packets, qualifies reviewers with calibration evidence, creates position-swapped blind assignments, enforces non-compensatory severe failures, and unlocks only a diagnostic pilot decision after blind statistics are sealed.

**Architecture:** Add a small Python 3.11 evaluation-control package under `scripts/ai_ip/eval_lab/` and versioned public contracts under `ai-ip-evals/lab/`. The package remains outside the Codex product answer path and invokes neither promptfoo nor Label Studio in this child. Existing `codex-rs/ai-ip-eval` remains unchanged and is used only by a later promotion bridge.

**Tech Stack:** Python 3.11, standard library, `jsonschema==4.25.1`, `pytest==9.0.2`, Bazel filegroups, immutable JSON with duplicate-key rejection and float-free canonical hashing.

**Spec:** `docs/superpowers/specs/2026-08-31-marketing-capability-evaluation-lab-design.md`

## Global Constraints

- Run all Python tests with `uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1`.
- Do not call a model provider, inspect an API key, access live Douyin, start MediaKit cloud work, install promptfoo, or start Label Studio in this child.
- Read only the three exact legacy source files listed in Task 1; never search legacy `.env`, browser profiles, Cookie stores, StorageState, or credential directories.
- Private roots must be absolute, outside every Git worktree, newly created with owner-only permissions, and free of symlinks and hardlinks.
- Canonical hashed JSON permits only `null`, booleans, integers, strings, arrays, and objects; reject floats and duplicate keys.
- No Python production module may exceed 500 lines. Keep every non-mechanical commit below 800 changed lines.
- No fixture, statistic, or decision in this child may emit `PASS_TO_PHASE_0B`, `G2=PASS`, a business-quality PASS, or `promotionEligible=true`.
- The only permitted terminal pilot dispositions are `fixtureValid`, `iterate`, and `invalid`; every decision must carry `diagnosticOnly=true` and `providerMode=not-run`.
- Do not modify `codex-rs/ai-ip-eval/**`, the existing 06A rubric/schema, product runtime crates, App Server protocol, provider code, or credentials.
- Finish code validation in this order: focused/full Python tests, Bazel asset build, scope/line checks, `just fmt` from `codex-rs`, then `git diff --check`; do not rerun tests after formatting.

## File Structure

| Path | Responsibility |
| --- | --- |
| `ai-ip-evals/lab/BUILD.bazel` | Public lab asset filegroup only |
| `ai-ip-evals/lab/schemas/*.schema.json` | Versioned exact public contracts |
| `ai-ip-evals/lab/rubrics/golden-gift-l1-l2-rubric.json` | L1–L2 dimensions and severe flags |
| `ai-ip-evals/lab/rubrics/fixture-qualification-policy.json` | Mechanical fixture-only reviewer policy |
| `ai-ip-evals/lab/fixtures/golden-gift-li-culture-v1/*.json` | Body-bounded source manifest and case blueprint |
| `ai-ip-evals/lab/fixtures/synthetic/*.json` | Explicitly synthetic candidate arms; tests construct reviewer values locally |
| `scripts/ai_ip/eval_lab/contracts.py` | Duplicate-safe JSON, schema validation, canonical bytes and hashes |
| `scripts/ai_ip/eval_lab/private_fs.py` | Owner-only private-root and create-new file primitives |
| `scripts/ai_ip/eval_lab/case_factory.py` | Pinned legacy import and three-packet compilation |
| `scripts/ai_ip/eval_lab/reviewer_academy.py` | Reviewer qualification from a versioned policy |
| `scripts/ai_ip/eval_lab/blind_controller.py` | Opaque arms, position swaps, mappings and reviewer bundles |
| `scripts/ai_ip/eval_lab/decision.py` | Submission validation, arbitration and blind statistics |
| `scripts/ai_ip/eval_lab/unlock.py` | Post-seal arm/outcome unlock and diagnostic decision |
| `scripts/ai_ip/eval_lab/cli.py` | Narrow command-line entry points and fixture-only E2E |
| `scripts/ai_ip/eval_lab/test_*.py` | Focused RED/GREEN tests; no live dependencies |

---
### Task 0: Authorize the Lab Foundation child

**Files:**
- Modify: `docs/superpowers/plans/README.md`
- Modify: `docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md`
- Modify: `docs/architecture/codex-fork-patch-ledger.md`

**Interfaces:**
- Consumes: approved design commit `c08a8da70` plus correction `83c52f238` and this plan's final commit.
- Produces: one append-only `F-0008` authorization that makes this file the only executable child and explicitly parks 06B-2.

- [ ] **Step 1: Record the expected governance assertions before editing**
Run:

```bash
rg -n "current and only executable child|06B-2|F-0007A" \
  docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md \
  docs/architecture/codex-fork-patch-ledger.md
```

Expected: the roadmap still names 06B-1 and the ledger ends with F-0007A; no F-0008 exists.

- [ ] **Step 2: Add the exact governance decision**
Update the plan index and roadmap so the current child is this plan. Append F-0008 with these exact dispositions:

```text
Owner: Marketing Evaluation Lab Foundation 07A
Classification: add external provider-free evaluation lab contracts and Python control plane
Preserve: codex-rs/ai-ip-eval, 06A/06B-1 private proof, App Server and product runtime
Park: 06B-2, live provider, promptfoo, Label Studio, real reviewers, G2 and Phase 0B
Provider mode: not-run
Gate: MARKETING_EVAL_LAB_07A=AUTHORIZED_NOT_IMPLEMENTED
Rollback: revert later F-0008A implementation commits first, then the F-0008 authorization commit; never rewrite F-0006 through F-0007A
```

Capture the plan commit with `git log -1 --format=%H -- docs/superpowers/plans/2026-08-31-07a-marketing-eval-lab-foundation.md` and write that full SHA into the entry; do not abbreviate it or write a stand-in token.

- [ ] **Step 3: Verify the governance scope**
Run:

```bash
git diff --check
git diff --name-only
rg -n "MARKETING_EVAL_LAB_07A=AUTHORIZED_NOT_IMPLEMENTED|06B-2.*park" \
  docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md \
  docs/architecture/codex-fork-patch-ledger.md
```

Expected: only the three governance documents changed and both decisions are discoverable.

- [ ] **Step 4: Commit the authorization**
```bash
git add docs/superpowers/plans/README.md \
  docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md \
  docs/architecture/codex-fork-patch-ledger.md
git commit -m "docs: authorize marketing eval lab foundation"
```

---
### Task 1: Freeze the public contracts and golden-gift source manifest

**Files:**
- Create: `ai-ip-evals/lab/BUILD.bazel`
- Create: `ai-ip-evals/lab/schemas/source-import.schema.json`
- Create: `ai-ip-evals/lab/schemas/case-bundle.schema.json`
- Create: `ai-ip-evals/lab/schemas/case-answer.schema.json`
- Create: `ai-ip-evals/lab/schemas/reviewer.schema.json`
- Create: `ai-ip-evals/lab/schemas/blind-review.schema.json`
- Create: `ai-ip-evals/lab/schemas/batch.schema.json`
- Create: `ai-ip-evals/lab/rubrics/golden-gift-l1-l2-rubric.json`
- Create: `ai-ip-evals/lab/rubrics/fixture-qualification-policy.json`
- Create: `ai-ip-evals/lab/fixtures/golden-gift-li-culture-v1/source-import.json`
- Create: `ai-ip-evals/lab/fixtures/golden-gift-li-culture-v1/case-blueprint.json`
- Create: `ai-ip-evals/lab/fixtures/synthetic/arm-strong.json`
- Create: `ai-ip-evals/lab/fixtures/synthetic/arm-known-failure.json`
- Create: `scripts/ai_ip/eval_lab/test_contract_assets.py`

**Interfaces:**
- Consumes: JSON Schema Draft 2020-12 and the three exact legacy SHA-256 values below.
- Produces: six root schemas keyed by `objectKind`, two versioned policies, one source manifest, one public blueprint and two explicitly synthetic `CaseAnswer` fixtures.

- [ ] **Step 1: Write failing asset tests**
Create `test_contract_assets.py` with a table that requires every asset, parses it with duplicate-key rejection, and asserts:

```python
SOURCE_DIGESTS = {
    "douyin-community-mcp-a113-2026-08-20.json":
        "f3171fbbad16cd5fc2d3907d619132fcf873a16225fa29eb78e59be0986ff3f9",
    "a115-golden-gift-real-e2e-2026-08-20.json":
        "4fb16c9fe54a313f51a479d60e51e7361726a36c8dd2a1a7a6a5cabcf1398c7e",
    "a116-vertical-incubation-skill-live-2026-08-20.json":
        "6b304738309195216b203dc846ee7491e430a396d69fb4001cef93f870af889c",
}

assert source_manifest["caseFamilyId"] == "golden-gift-li-culture-v1"
assert source_manifest["sources"] == [
    {
        "sourceId": "v6-a113-community-acceptance",
        "relativePath": "douyin-community-mcp-a113-2026-08-20.json",
        "sha256": SOURCE_DIGESTS["douyin-community-mcp-a113-2026-08-20.json"],
        "packetRoles": ["contentEvidence", "referenceEvidence"],
    },
    {
        "sourceId": "v6-a115-known-failure",
        "relativePath": "a115-golden-gift-real-e2e-2026-08-20.json",
        "sha256": SOURCE_DIGESTS["a115-golden-gift-real-e2e-2026-08-20.json"],
        "packetRoles": ["outcomeEvidence", "referenceEvidence"],
    },
    {
        "sourceId": "v6-a116-pollution-diagnostic",
        "relativePath": "a116-vertical-incubation-skill-live-2026-08-20.json",
        "sha256": SOURCE_DIGESTS["a116-vertical-incubation-skill-live-2026-08-20.json"],
        "packetRoles": ["outcomeEvidence", "referenceEvidence"],
    },
]
assert blueprint["taskLevels"] == ["L1", "L2"]
assert blueprint["diagnosticOnly"] is True
assert all(arm["synthetic"] is True for arm in synthetic_arms)
```
- [ ] **Step 2: Run the focused test and observe RED**
Run:

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_contract_assets.py
```

Expected: FAIL because the lab assets do not exist.

- [ ] **Step 3: Write the six exact schema roots**
Each schema must use `additionalProperties: false`, `schemaVersion: {"const": 1}`, and a required `objectKind`. Freeze these root kinds and responsibilities:

```text
source-import.schema.json:
  SourceImportManifest {caseFamilyId, sources[{sourceId,relativePath,sha256,packetRoles}]}

case-bundle.schema.json oneOf:
  CaseBlueprint {caseFamilyId,taskLevels,mission,businessSubject,directionFamilies,knownUnknowns,diagnosticOnly}
  ContentPacket {caseId,predictionTime,mission,businessSubject,knownEvidence,unknowns,requestedOutput,sourceRefs}
  OutcomePacket {caseId,revealAfter,observations,knownFailures,sourceRefs}
  ReferenceDossier {caseId,directionFamilies,counterexamples,nonCopyBoundaries,disputes,sourceRefs}
  CaseCompilationReceipt {caseId,contentPacketSha256,outcomePacketSha256,referenceDossierSha256,sourceManifestSha256}

case-answer.schema.json:
  CaseAnswer {caseId,taskLevels,subject,audiences,desiredActions,evidenceGaps,directionOptions,recommendedDirectionId,claims,readiness}

reviewer.schema.json oneOf:
  ReviewerProfile {reviewerId,capabilityDomains,conflictDisclosure,requestedQualifications}
  CalibrationAttempt {reviewerId,policyId,anchorCorrect,severeMisses,repeatAgreement,swapAgreement,completedAt}
  QualificationReceipt {reviewerId,policyId,qualifiedDomains,status,attemptSha256,expiresAt,diagnosticOnly}

blind-review.schema.json oneOf:
  BlindAssignment {assignmentId,batchId,reviewerId,position,sequenceSlot,releaseCondition,contentPacketSha256,armAOutputSha256,armBOutputSha256,swapOf}
  ReviewSubmission {assignmentId,reviewerId,eligibility,preference,dimensionsByArm,severeFlagsByArm,reasons,evidenceRefs,submittedAt}
  BlindPackReceipt {batchId,batchManifestSha256,armKeySha256,assignmentSha256s,mappingSha256s,createdAt}

batch.schema.json oneOf:
  EvaluationBatch {batchId,caseId,rubricSha256,armOutputSha256s,reviewerIds,analysisFrozenAt,diagnosticOnly}
  BlindStatistics {batchId,reviewCount,qualifiedReviewerCount,swapConsistentCount,preferenceCounts,severeByOutputSha256,arbitrationRequired,arbitrationCompleted,sealedAt}
  BatchDecision {batchId,stockOutputSha256,modifiedOutputSha256,disposition,diagnosticOnly,providerMode,promotionEligible,blindStatisticsSha256,outcomePacketSha256,unlockedAt}
```

Use enums exactly as follows:

```json
{
  "eligibility": ["both", "aOnly", "bOnly", "neither"],
  "preference": ["A", "B", "nearTie", "abstain"],
  "readiness": ["readyForHumanReview", "blockedByMissingEvidence", "notUsable"],
  "qualificationStatus": ["qualified", "notQualified", "expired"],
  "pilotDisposition": ["fixtureValid", "iterate", "invalid"]
}
```
- [ ] **Step 4: Write the policies and fixtures**
The L1–L2 rubric must keep dimensions separate:

```json
{
  "schemaVersion": 1,
  "rubricId": "golden-gift-l1-l2-v1",
  "dimensions": [
    "businessSubjectClarity",
    "audienceActionFit",
    "directionBreadthAndTradeoffs",
    "evidenceAndUnknownDiscipline",
    "sustainableIpPotential",
    "commercialConnectionWithoutForcedSelling"
  ],
  "severeFlags": [
    "fabricatedExistingExperience",
    "wrongBusinessSubject",
    "wrongDesiredAction",
    "singlePathPresentedAsProvenTruth",
    "notActuallyUsable",
    "rightsOrPrivacyViolation"
  ],
  "preferenceOptions": ["A", "B", "nearTie", "abstain"],
  "eligibilityOptions": ["both", "aOnly", "bOnly", "neither"]
}
```

The fixture qualification policy must be explicitly non-production:

```json
{
  "schemaVersion": 1,
  "policyId": "fixture-reviewer-qualification-v1",
  "diagnosticOnly": true,
  "minimumAnchorCorrect": 4,
  "maximumSevereMisses": 0,
  "minimumRepeatAgreementPermille": 1000,
  "minimumSwapAgreementPermille": 1000,
  "qualificationLifetimeDays": 7
}
```

The case blueprint must list the three acceptable direction families from the spec and state that none is a mandatory root. `arm-known-failure.json` must seed an unsupported existing-experience claim and `readiness=notUsable`; it must not fail merely because it differs from A116's hard-coded content root.

- [ ] **Step 5: Add the Bazel filegroup and make the asset test GREEN**
`BUILD.bazel` must enumerate every schema, rubric and fixture explicitly:

```starlark
filegroup(
    name = "lab-assets",
    srcs = [
        "fixtures/golden-gift-li-culture-v1/case-blueprint.json",
        "fixtures/golden-gift-li-culture-v1/source-import.json",
        "fixtures/synthetic/arm-known-failure.json",
        "fixtures/synthetic/arm-strong.json",
        "rubrics/fixture-qualification-policy.json",
        "rubrics/golden-gift-l1-l2-rubric.json",
        "schemas/batch.schema.json",
        "schemas/blind-review.schema.json",
        "schemas/case-answer.schema.json",
        "schemas/case-bundle.schema.json",
        "schemas/reviewer.schema.json",
        "schemas/source-import.schema.json",
    ],
)
```

Run the focused pytest command from Step 2 and:

```bash
bazel build //ai-ip-evals/lab:lab-assets
```

Expected: both PASS.

- [ ] **Step 6: Commit the frozen contracts**
```bash
git add ai-ip-evals/lab scripts/ai_ip/eval_lab/test_contract_assets.py
git commit -m "test: freeze marketing eval lab contracts"
```

---
### Task 2: Implement exact JSON and private filesystem primitives

**Files:**
- Create: `scripts/ai_ip/eval_lab/contracts.py`
- Create: `scripts/ai_ip/eval_lab/private_fs.py`
- Create: `scripts/ai_ip/eval_lab/test_contracts.py`
- Create: `scripts/ai_ip/eval_lab/test_private_fs.py`

**Interfaces:**
- Produces: `JsonObject = dict[str, object]` plus aliases `CaseCompilationReceipt`, `QualificationReceipt`, `BlindPackReceipt`, `BlindStatistics`, and `BatchDecision`; `load_exact_json(path) -> object`, `validate_contract(value, schema_path) -> None`, `canonical_json_bytes(value) -> bytes`, `sha256_json(value) -> str`; and `PrivateRoot.create_new(path)`, `.open_existing(path)`, `.create_dir(relative)`, `.write_new_json(relative, value)`, `.read_json(relative)`.
- Consumes: Task 1 schemas and float-free wire values.

- [ ] **Step 1: Write failing contract tests**
Cover exact valid round-trip and these required failures:

```python
@pytest.mark.parametrize("payload", [
    b'{"a":1,"a":2}',
    b'{"number":1.25}',
    b'{"number":NaN}',
])
def test_load_exact_json_rejects_ambiguous_numbers_and_keys(payload, tmp_path):
    path = tmp_path / "input.json"
    path.write_bytes(payload)
    with pytest.raises(LabContractError):
        load_exact_json(path)

def test_canonical_json_is_order_independent_and_utf8():
    assert canonical_json_bytes({"礼": "人情", "a": 1}) == (
        '{"a":1,"礼":"人情"}'.encode("utf-8")
    )
```

- [ ] **Step 2: Write failing private-root tests**
Tests must reject relative roots, existing roots, roots inside the repository, symlink ancestors, path traversal, duplicate create, a hardlinked retained file and file modes broader than `0600`. A successful root must be `0700`, and every JSON object must be created once at `0600`.

- [ ] **Step 3: Run focused tests and observe RED**
```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_contracts.py \
    scripts/ai_ip/eval_lab/test_private_fs.py
```

Expected: FAIL because both modules are absent.

- [ ] **Step 4: Implement minimal exact contracts**
Use an `object_pairs_hook` and `parse_float`/`parse_constant` rejection:

```python
def load_exact_json(path: Path) -> object:
    def reject_pairs(pairs: list[tuple[str, object]]) -> dict[str, object]:
        result: dict[str, object] = {}
        for key, value in pairs:
            if key in result:
                raise LabContractError(f"duplicate JSON key: {key}")
            result[key] = value
        return result

    def reject_number(token: str) -> NoReturn:
        raise LabContractError(f"non-integer JSON number: {token}")

    return json.loads(
        path.read_bytes().decode("utf-8", errors="strict"),
        object_pairs_hook=reject_pairs,
        parse_float=reject_number,
        parse_constant=reject_number,
    )
```

Before canonical encoding, recurse through the value and reject non-string keys, floats and unknown Python types. Encode with `ensure_ascii=False`, `sort_keys=True`, `separators=(",", ":")`, and `allow_nan=False`.

- [ ] **Step 5: Implement owner-only create-new storage**
`PrivateRoot.create_new()` must resolve and record the repository root, reject containment in any Git worktree, create the root with mode `0700`, and keep all writes relative. `open_existing()` repeats containment, mode, symlink and ownership checks. `create_dir()` creates one relative `0700` directory without traversal. `write_new_json()` must use `os.open` with `O_CREAT|O_EXCL|O_WRONLY` and `O_NOFOLLOW` when available, write canonical bytes plus one LF, `fsync`, close, then re-open and verify regular file, `nlink == 1`, mode `0600` and expected SHA-256.

- [ ] **Step 6: Run focused tests and commit**
Run the command from Step 3. Expected: PASS.

```bash
git add scripts/ai_ip/eval_lab/contracts.py \
  scripts/ai_ip/eval_lab/private_fs.py \
  scripts/ai_ip/eval_lab/test_contracts.py \
  scripts/ai_ip/eval_lab/test_private_fs.py
git commit -m "feat: add exact lab storage primitives"
```

---
### Task 3: Compile the frozen golden-gift three-packet case

**Files:**
- Create: `scripts/ai_ip/eval_lab/case_factory.py`
- Create: `scripts/ai_ip/eval_lab/test_case_factory.py`

**Interfaces:**
- Consumes: `SourceImportManifest`, case blueprint, exact legacy source root and `PrivateRoot`.
- Produces: `compile_golden_gift_case(source_root, blueprint_path, source_manifest_path, private_root, compiled_at) -> CaseCompilationReceipt`.

- [ ] **Step 1: Write the failing success test**
Create minimal A113/A115/A116-shaped JSON values under `tmp_path/source`, generate a test-local manifest from their computed SHA-256 values, run `compile_golden_gift_case`, and assert the exact private layout:

```text
cases/golden-gift-li-culture-v1/content/content-packet.json
cases/golden-gift-li-culture-v1/outcome/outcome-packet.json
cases/golden-gift-li-culture-v1/reference/reference-dossier.json
cases/golden-gift-li-culture-v1/coordinator/case-compilation-receipt.json
```

Assert that the content packet includes the mission, A113 bounded public-evidence availability and named unknowns, but recursively contains none of:

```python
FORBIDDEN_CONTENT_KEYS = {
    "thread_id", "project_id", "runs", "performance", "human_review_passed",
    "required_path", "default_root", "content_root", "knownFailures",
}
```

Assert the outcome packet retains the A115/A116 known failure classes and the Reference Dossier contains three non-mandatory direction families.

- [ ] **Step 2: Write failing mutation tests**
Parameterize wrong SHA, missing source, extra source, source symlink, A113 `credential_values_recorded=true`, A115/A116 `secrets_included=true`, changed `caseFamilyId`, and `compiledAt` earlier than a source record. Every mutation must raise `CaseFactoryError` without creating a partial case tree.

- [ ] **Step 3: Run the focused test and observe RED**
```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_case_factory.py
```

Expected: FAIL because `case_factory.py` is absent.

- [ ] **Step 4: Implement manifest-first compilation**
The compiler must:

1. validate the manifest and blueprint before reading a legacy source;
2. join only each manifest `relativePath` below the explicit `source_root`;
3. require a regular non-symlink file and exact SHA-256;
4. validate the expected top-level identity (`A113`, `A115`, `A116`) and no-recorded-secrets flags;
5. build all four values in memory and validate them before the first output write;
6. create four disjoint directories and write packets with `PrivateRoot.write_new_json`;
7. write the receipt last with the three packet SHA-256 values and source-manifest SHA-256.

Do not copy the full legacy JSON into `ContentPacket`. The `knownEvidence` projection must be bounded to A113's provider revision, local-experiment license status, query, result count, payload size and explicit limitations.

- [ ] **Step 5: Run the focused test and commit**
Run the command from Step 3. Expected: PASS.

```bash
git add scripts/ai_ip/eval_lab/case_factory.py \
  scripts/ai_ip/eval_lab/test_case_factory.py
git commit -m "feat: compile golden gift exam packets"
```

---
### Task 4: Qualify reviewers with explicit calibration evidence

**Files:**
- Create: `scripts/ai_ip/eval_lab/reviewer_academy.py`
- Create: `scripts/ai_ip/eval_lab/test_reviewer_academy.py`

**Interfaces:**
- Consumes: `ReviewerProfile`, `CalibrationAttempt`, fixture qualification policy and evaluation time.
- Produces: `evaluate_calibration(profile, attempt, policy, evaluated_at) -> QualificationReceipt`.

- [ ] **Step 1: Write failing qualification tests**
Use whole-object equality for a passing receipt and cover these failures: reviewer mismatch, policy mismatch, conflict disclosure not accepted, anchor count below 4, any severe miss, repeat/swap permille below 1000, requested domain absent from the attempt, expired attempt and unknown fields.

The expected passing receipt shape is:

```python
{
    "schemaVersion": 1,
    "objectKind": "QualificationReceipt",
    "reviewerId": "reviewer-business-1",
    "policyId": "fixture-reviewer-qualification-v1",
    "qualifiedDomains": ["businessIpJudgment", "evidenceIntegrity"],
    "status": "qualified",
    "attemptSha256": sha256_json(attempt),
    "expiresAt": "2026-09-07T00:00:00Z",
    "diagnosticOnly": True,
}
```
- [ ] **Step 2: Run the focused test and observe RED**
```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_reviewer_academy.py
```
- [ ] **Step 3: Implement policy-driven qualification**
Implement RFC3339 UTC parsing with `datetime.fromisoformat(value.replace("Z", "+00:00"))`, reject non-UTC outputs, compute expiry from `qualificationLifetimeDays`, and emit `notQualified` rather than silently weakening the policy. Do not hard-code professional qualification beyond the passed versioned policy.

- [ ] **Step 4: Run the focused test and commit**
Run Step 2. Expected: PASS.

```bash
git add scripts/ai_ip/eval_lab/reviewer_academy.py \
  scripts/ai_ip/eval_lab/test_reviewer_academy.py
git commit -m "feat: calibrate marketing reviewers"
```

---
### Task 5: Create opaque position-swapped blind assignments

**Files:**
- Create: `scripts/ai_ip/eval_lab/blind_controller.py`
- Create: `scripts/ai_ip/eval_lab/test_blind_controller.py`

**Interfaces:**
- Consumes: case receipt, two `CaseAnswer` files, rubric, two base qualification receipts, an optional arbitrator qualification receipt, private root and `seed_source(byte_count) -> bytes`.
- Produces: `prepare_blind_batch(...) -> BlindPackReceipt` plus private batch manifest, arm key, per-reviewer assignments, A/B answers and private mappings.

- [ ] **Step 1: Write the failing deterministic-layout test**
Inject two fixed 32-byte test seeds and assert:

```text
batches/golden-gift-fixture-batch-1/coordinator/batch-manifest.json
batches/golden-gift-fixture-batch-1/coordinator/arm-key.json
batches/golden-gift-fixture-batch-1/coordinator/mappings/reviewer-business-1-primary.json
batches/golden-gift-fixture-batch-1/reviewer/reviewer-business-1/reviewer-business-1-primary/assignment.json
batches/golden-gift-fixture-batch-1/reviewer/reviewer-business-1/reviewer-business-1-primary/A.json
batches/golden-gift-fixture-batch-1/reviewer/reviewer-business-1/reviewer-business-1-primary/B.json
batches/golden-gift-fixture-batch-1/reviewer/reviewer-business-1/reviewer-business-1-primary/content-packet.json
batches/golden-gift-fixture-batch-1/reviewer/reviewer-business-1/reviewer-business-1-primary/rubric.json
```

Each of two base reviewers must receive a primary and non-adjacent swapped assignment. When an arbitrator receipt is supplied, prepare its primary/swap assignments but mark them `releaseCondition=arbitrationRequired`. `arm-key.json` and `mappings/` must not be below `reviewer/`; reviewer bundles must contain no treatment ID, binary path, provider, Skill name, cost or source-root path.

- [ ] **Step 2: Write failing authority tests**
Reject: identical arm bytes, case ID mismatch, unqualified/expired reviewer, duplicate reviewer ID, a base reviewer count other than two, an arbitrator who duplicates a base reviewer, reused seed, wrong rubric, pre-existing destination, candidate answer with additional fields, and any candidate output containing a reserved opaque arm nonce supplied by the controller.

- [ ] **Step 3: Run the focused test and observe RED**
```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_blind_controller.py
```
- [ ] **Step 4: Implement the blind transaction**
Expose one production function:

```python
def prepare_blind_batch(
    *,
    private_root: PrivateRoot,
    case_receipt_path: Path,
    stock_answer_path: Path,
    modified_answer_path: Path,
    rubric_path: Path,
    base_qualification_receipt_paths: tuple[Path, Path],
    arbitrator_qualification_receipt_path: Path | None,
    batch_id: str,
    analysis_frozen_at: str,
    seed_source: Callable[[int], bytes] = secrets.token_bytes,
) -> BlindPackReceipt:
    ...
```

Call `seed_source(32)` once per assignment. Prepare and validate every value in memory, stage under a private create-new directory, publish reviewer and coordinator trees only after all files verify, and write `blind-pack-receipt.json` last. The treatment-to-output mapping exists only in `arm-key.json`; assignment mappings refer only to output SHA-256 values.

- [ ] **Step 5: Run the focused test and commit**
Run Step 3. Expected: PASS.

```bash
git add scripts/ai_ip/eval_lab/blind_controller.py \
  scripts/ai_ip/eval_lab/test_blind_controller.py
git commit -m "feat: prepare swapped marketing blind packs"
```

---
### Task 6: Validate reviews and seal blind statistics before unlock

**Files:**
- Create: `scripts/ai_ip/eval_lab/decision.py`
- Create: `scripts/ai_ip/eval_lab/test_decision.py`

**Interfaces:**
- Consumes: blind-pack receipt, assignments, two base qualification receipts, optional arbitrator receipt and review submissions.
- Produces: `seal_blind_statistics(...) -> BlindStatistics` without reading `arm-key.json` or any treatment identity.

- [ ] **Step 1: Write failing review-ingestion tests**
Cover exact submission validation, reviewer/assignment binding, qualification expiry, duplicate reviewer/assignment, missing swap, preference inversion under swap, evidence-ref membership and severe-flag arm binding.

Use these blind rules:

```text
same output wins in primary and swap -> one consistent reviewer preference
primary/swap disagree -> reviewer preference nearTie and arbitrationRequired=true
two reviewers choose different outputs -> arbitrationRequired=true
eligibility=neither -> not a tie; both outputs are ineligible for that reviewer
any severe flag -> recorded by output SHA; never averaged into dimension scores
```
- [ ] **Step 2: Write the known-failure non-compensation test**
Give the known-failure arm maximum dimension scores but flag `fabricatedExistingExperience=true` and `notActuallyUsable=true`. Assert the analysis keeps both severe flags and sets `arbitrationRequired=true`; no aggregate quality score may erase them.

- [ ] **Step 3: Run the focused test and observe RED**
```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_decision.py
```
- [ ] **Step 4: Implement blind-only sealing**
The public API must not accept an arm key:

```python
def seal_blind_statistics(
    *,
    private_root: PrivateRoot,
    blind_pack_receipt_path: Path,
    base_qualification_receipt_paths: tuple[Path, Path],
    arbitrator_qualification_receipt_path: Path | None,
    submission_paths: tuple[Path, ...],
    sealed_at: str,
) -> BlindStatistics:
    ...
```

Require four base submissions: primary and swap for each of two reviewers. If arbitration is required and the arbitrator pair is absent, raise `ArbitrationRequiredError` without writing `blind-statistics.json`. When the two designated arbitrator submissions are supplied, include them in the same function call, require `releaseCondition=arbitrationRequired`, then write the final `blind-statistics.json` create-new and return the validated object; its SHA-256 is `sha256_json(result)`.

- [ ] **Step 5: Run the focused test and commit**
Run Step 3. Expected: PASS.

```bash
git add scripts/ai_ip/eval_lab/decision.py scripts/ai_ip/eval_lab/test_decision.py
git commit -m "feat: seal blind marketing review statistics"
```

---
### Task 7: Unlock only a diagnostic pilot decision

**Files:**
- Create: `scripts/ai_ip/eval_lab/unlock.py`
- Create: `scripts/ai_ip/eval_lab/test_unlock.py`

**Interfaces:**
- Consumes: sealed blind statistics, arm key, case compilation receipt and OutcomePacket.
- Produces: `unlock_fixture_pilot(...) -> BatchDecision` with `diagnosticOnly=true`, `providerMode=not-run`, and `promotionEligible=false`.

- [ ] **Step 1: Write failing unlock-order tests**
Reject unlock when statistics are absent, have `arbitrationRequired=true` with `arbitrationCompleted=false`, changed after seal, refer to another batch, omit a review, have an arm-key mismatch, or bind a different OutcomePacket than the case receipt. Prove the function does not rewrite blind statistics after revealing outcomes.

- [ ] **Step 2: Write the exact diagnostic decision test**
For a valid known-failure pilot, expect:

```python
assert decision["disposition"] == "fixtureValid"
assert decision["diagnosticOnly"] is True
assert decision["providerMode"] == "not-run"
assert decision["promotionEligible"] is False
assert "PASS_TO_PHASE_0B" not in canonical_json_bytes(decision).decode("utf-8")
assert "G2" not in canonical_json_bytes(decision).decode("utf-8")
```

`fixtureValid` means only that the evaluation mechanism correctly retained and exposed the seeded failure after unlock; it does not mean the strong arm is a validated marketing answer.

- [ ] **Step 3: Run the focused test and observe RED**
```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_unlock.py
```
- [ ] **Step 4: Implement post-seal unlock**
Implement:

```python
def unlock_fixture_pilot(
    *,
    private_root: PrivateRoot,
    blind_statistics_path: Path,
    arm_key_path: Path,
    case_receipt_path: Path,
    outcome_packet_path: Path,
    unlocked_at: str,
) -> BatchDecision:
    ...
```

The function must verify every referenced SHA before reading mappings, map only output SHA-256 values to `stock`/`modified`, attach no answer body or reviewer identity, and write `batch-decision.json` once.

- [ ] **Step 5: Run the focused test and commit**
Run Step 3. Expected: PASS.

```bash
git add scripts/ai_ip/eval_lab/unlock.py scripts/ai_ip/eval_lab/test_unlock.py
git commit -m "feat: unlock diagnostic marketing pilot"
```

---
### Task 8: Add the CLI and complete the provider-free golden-gift fixture pilot

**Files:**
- Create: `scripts/ai_ip/eval_lab/cli.py`
- Create: `scripts/ai_ip/eval_lab/test_cli.py`
- Create: `scripts/ai_ip/eval_lab/test_offline_pilot.py`
- Modify: `docs/architecture/codex-fork-patch-ledger.md`

**Interfaces:**
- Consumes: Tasks 1–7.
- Produces: narrow CLI commands and append-only `F-0008A` evidence of the provider-free fixture mechanism.

- [ ] **Step 1: Write failing CLI tests**
Freeze these commands and reject unknown flags:

```text
compile-golden-gift --source-root --source-manifest --blueprint --private-root --compiled-at
qualify-reviewer --profile --attempt --policy --evaluated-at --output-root
prepare-blind-batch --private-root --case-receipt --stock-answer --modified-answer --rubric --base-qualification-receipt (exactly twice) --arbitrator-qualification-receipt (zero or once) --batch-id --analysis-frozen-at
seal-blind-statistics --private-root --blind-pack-receipt --base-qualification-receipt (exactly twice) --arbitrator-qualification-receipt (zero or once) --submission (four or six) --sealed-at
unlock-fixture-pilot --private-root --blind-statistics --arm-key --case-receipt --outcome-packet --unlocked-at
```

The CLI must emit only one compact JSON receipt on stdout and never emit packet bodies, mappings, source paths or reviewer identities.

- [ ] **Step 2: Write the failing end-to-end offline pilot**
The portable E2E test must use test-local A113/A115/A116-shaped values with a generated manifest, the two synthetic arms, two qualified synthetic reviewers plus one qualified arbitrator, position-swapped submissions, a forced disagreement, arbitration, blind seal and unlock. Assert:

```python
assert result == {
    "batchId": "golden-gift-fixture-batch-1",
    "diagnosticOnly": True,
    "disposition": "fixtureValid",
    "promotionEligible": False,
    "providerMode": "not-run",
}
```

The stdout projection may include only these five fields; full private receipts remain under pytest `tmp_path` and are removed by pytest cleanup.

- [ ] **Step 3: Run CLI/E2E tests and observe RED**
```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_cli.py \
    scripts/ai_ip/eval_lab/test_offline_pilot.py
```
- [ ] **Step 4: Implement the narrow CLI**
Use `argparse` subparsers with `allow_abbrev=False`. Convert all paths to absolute canonical paths before dispatch. Catch only `LabContractError`, `PrivateFsError`, `CaseFactoryError`, reviewer/blind/decision errors, print one generic error to stderr and exit 1; do not dump object bodies.

- [ ] **Step 5: Run the complete Python matrix**
```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab
```

Expected: all tests PASS, with no network, provider, credential or live platform operation.

- [ ] **Step 6: Run assets, scope and size checks**
Run the current-host legacy acceptance separately; the test reads `AI_IP_V6_EVIDENCE_ROOT`, uses `tempfile.TemporaryDirectory()` for its owner-only private root, and skips only when that variable is absent:

```bash
AI_IP_V6_EVIDENCE_ROOT=/Users/yangyucheng/Documents/ChatGPT/第六版营销系统/docs/content-intelligence-v6/evidence \
  uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_offline_pilot.py -k legacy_sources
```

Expected on the approved development host: PASS, not SKIP. The temporary private root is removed by `TemporaryDirectory` after receipt verification. Then run:

```bash
bazel build //ai-ip-evals/lab:lab-assets
git diff --check
git diff --name-only HEAD~1..HEAD
wc -l scripts/ai_ip/eval_lab/*.py
```

Expected: every production module is below 500 lines; no `codex-rs/ai-ip-eval`, App Server, provider or product runtime file changed.

- [ ] **Step 7: Run final formatter in required order**
```bash
cd codex-rs
just fmt
cd ..
git diff --check
```

Do not rerun tests after `just fmt`. Restore any unrelated formatter churn before continuing.

- [ ] **Step 8: Append F-0008A without upgrading a business gate**
Record exact commits, test counts, Bazel result, fixture source digests, zero provider cost, no credential lookup, no live Douyin, and:

```text
MARKETING_EVAL_LAB_07A=COMPLETE_PROVIDER_FREE_FIXTURE
G0=OPEN
G1=OPEN
G2=OPEN
PASS_TO_PHASE_0B=false
next child=07B Codex Batch Runner, separately reviewed and authorized
```
- [ ] **Step 9: Commit the CLI, tests and implementation ledger**
```bash
git add scripts/ai_ip/eval_lab/cli.py \
  scripts/ai_ip/eval_lab/test_cli.py \
  scripts/ai_ip/eval_lab/test_offline_pilot.py \
  docs/architecture/codex-fork-patch-ledger.md
git commit -m "test: close marketing eval lab foundation"
```

---
## Plan Self-Review Checklist

- Every design requirement owned by child 07A maps to Tasks 1–8: three-packet isolation, L1–L2 contracts, reviewer calibration, position swaps, arbitration, severe non-compensation, blind seal, post-seal unlock and provider-free diagnostic output.
- promptfoo, Label Studio, live community collection, MediaKit cloud, AI judge authority, historical outcome ranking, G2 and the 06A promotion bridge remain outside this child.
- All public function names and wire enum values are defined once and reused consistently.
- No step relies on a model, network endpoint, provider key, hidden legacy credential or unreviewed dynamic code.
- No implementation step contains an unspecified error-handling instruction or an unbound type name.
