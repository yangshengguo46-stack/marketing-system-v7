> **RETIRED — RETIRED_NOT_PASSED (2026-09-05).** This evaluator/proof document is historical and is not executable. All commands, prerequisite gates and successor authorizations below are retired by the approved business-first cleanup; prior results and failures remain historical claims only. Do not resume Phase 0A, 06A, 06B, 07A, 07B or LH1 from this record. Retirement grants no business PASS, model qualification, spending, publication, customer-data access or customer-release authority. Current authority: `docs/superpowers/specs/2026-09-05-business-first-cleanup-design.md` and `docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md`.

# AI IP Proof Commitment and Cost Binding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give every future live business proof a private-key-derived public identity and two immutable, manifest-bound cost receipts plus sidecar bindings, without calling a provider or emitting a public business report.

**Architecture:** Extend the existing frozen Native proof boundary instead of adding a billing service or product gateway. Freeze strict CNY launch-input contracts, generate an owner-only 32-byte commitment key while the live-shaped context is frozen, derive `publicRunId` with the normative HMAC framing, and reuse the already verified pair/ledger/manifests to calculate one private cost receipt per arm. Keep execution manifests immutable and attach cost only through `cost-binding.json`; report, public verification, publishing, checkpoint finalization, and retention deletion remain later children.

**Tech Stack:** Rust 1.95, Clap 4, serde/serde_json, `jsonschema` 0.49.9 with external resolution disabled, `serde_json_canonicalizer` 0.3.2, HMAC-SHA256, SHA-256, rand 0.9 `OsRng`, chrono 0.4.43, existing secure filesystem/private inventory/proof-ledger code, Python 3 standard-library cross-check under local pytest 9.0.2, Cargo nextest through `just`, Bazel 9.0.0.

**Spec:** `docs/superpowers/specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md` Work Package 6 Contract 4 and the commitment primitives in Contract 5.

**Execution annex:** `docs/superpowers/plans/2026-08-30-06b1-cost-authority-execution-annex.md` contains Tasks 5A–7 and is normative only together with this plan.

## Global Constraints

- Base implementation tip is `8a11b52d0`; governance closure is `eb64199a6`; prior reviewed history is immutable.
- This is Work Package 6B-1 only. 06B-2 owns body-free report/index plus `verify-report` and `verify-live-proof`; 06B-3 owns crash-safe repository publication, shipping-boundary enforcement, and checkpoint finalization; 06C owns retention closeout.
- 06B-3 may implement and synthetically test the checkpoint finalizer, but its real invocation remains Work Package 9 after G0/G1/G2 evidence exists.
- 06B-2 must first reconcile two frozen-wire conflicts: `INVALID_PROOF` has `BlindDecision.metrics=null` while the current business-report schema requires numeric metrics, and the current report-index schema is smaller than the Work Package 8 append-only attempt wire. 06B-1 must not fill unknown metrics with zero or silently amend either public contract.
- `providerMode=not-run`; authorized provider cost `0`; paid provider cost `0`; no API key, credential reader, real provider, real endpoint, external network, real held-out case, or human review is authorized.
- Real CLI tests may create Replay and Native/Mock synthetic evidence only; both modes must be rejected by cost commands with no output. Positive receipt/binding transactions use a test-only synthetic `VerifiedLiveCostAuthority` and never make the current Mock producer claim Live status. The production success path remains dormant until Work Package 8 supplies a verified Live core after both parked blockers close.
- Business quality remains primary. Cost/token/latency inside an approved cap cannot override a valid private business decision; cost excess makes the proof invalid later and never rewrites 06A scoring.
- Launch cost inputs are exact CNY contracts. `fx-policy` is the explicit `notApplicable` CNY policy (`CNY -> CNY`, `1/1`) for this child. A future non-CNY provider needs a separately reviewed schema amendment; silently applying floating-point FX is forbidden.
- The HMAC domain is the exact byte string `b"AI-IP-PROOF-V1\0"`. Sensitive commitments use `domain || u32be(label_len) || label_utf8 || u64be(value_len) || canonical_value_bytes`; outputs are lowercase hex.
- `publicRunId` is `HMAC-SHA256(commitmentKey, framed("publicRunId", JCS(pairId)))`. It is created during the common Replay/Native context freeze, before any future bearer read, and is not derived from provider/model labels or wall-clock time.
- Replay and Native freeze both create the key and derive `publicRunId`; this preserves the Work Package 6 Contract 4 common freeze boundary. The 32-byte key exists only at `<privateRoot>/coordinator/commitment-key.bin`, owner-only, `create_new`, and inventory-covered. The key and any raw/salted copy of it never enter a manifest, receipt, log, Git, or public artifact.
- Execution manifests remain immutable. A cost receipt binds their bytes; `annotate-cost` creates a separate exact sidecar and never edits a manifest.
- All JSON readers reject duplicate keys, unknown fields, invalid framing, trailing bytes, NaN/Infinity, unsafe paths, symlinks/reparse points, hardlinks, replacement races, and files over their declared cap.
- Every production behavior follows executable RED -> GREEN. No unresolved-symbol RED; CLI REDs must compile and reach the existing unimplemented command or the wrong current behavior.
- RED is observed only in the working tree and is never committed. Every actual commit must compile and leave its registered focused tests GREEN; size-driven splits may contain only already-GREEN vertical behavior or compile-neutral/unregistered fixture support.
- Every commit, including fixture/mechanical splits, stays below 800 added+deleted lines. Before every commit, run `git diff --cached --numstat | awk '$1 == "-" || $2 == "-" { bad=1 } $1 ~ /^[0-9]+$/ { changed += $1 + $2 } END { print changed; exit(bad || changed >= 800) }'`; split the staged slice if it fails. Every new production module stays below 500 lines excluding sibling tests. Do not add cost logic to the 5,000-line `runner.rs`; only narrow call-site wiring is permitted there.
- Preserve the two parked Work Package 8 blockers: same-UID managed-case/eval-tree ancestor swap and actual executable/kernel-loaded-vnode ancestor swap. This child neither fixes nor weakens them.
- Required test runner is the locked `cargo-nextest 0.9.103`; do not run `cargo test`. Use `just test -p codex-ai-ip-eval` and package-scoped Bazel targets only.
- Final order is tests/build/lock/scope review, then `just fix -p codex-ai-ip-eval`, then `just fmt`. Run no test after final fix/format.

---

### Task 0: Authorize 06B-1 without authorizing runtime behavior

**Files:**
- Create: `docs/superpowers/plans/2026-08-30-06b1-proof-commitment-and-cost-binding.md`
- Create: `docs/superpowers/plans/2026-08-30-06b1-cost-authority-execution-annex.md`
- Modify: `docs/superpowers/plans/README.md`
- Modify: `docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md`
- Modify: `docs/architecture/codex-fork-patch-ledger.md`

**Interfaces:**
- Consumes: F-0006A GREEN closure and two independent read-only plan reviews.
- Produces: the only current executable child pointer; no Rust, schema, provider, private evidence, or public report change.

- [x] **Step 1: Complete plan self-review and two independent reviews**

Require Critical 0 / Important 0 from one Work Package 6 specification review and one existing-code executability review across both documents. Self-review checks Contract 4 coverage, no placeholder language, exact type/path consistency, line/commit caps, provider not-run/cost zero, and the 06B-2/06B-3/06C exclusions. Keep each reviewed new document below 800 added lines. Commit the non-authorizing reviewed documents independently before pointer activation:

```bash
git add docs/superpowers/plans/2026-08-30-06b1-cost-authority-execution-annex.md
git diff --cached --check
git commit -m "docs: freeze private proof cost execution contract"
git add docs/superpowers/plans/2026-08-30-06b1-proof-commitment-and-cost-binding.md
git diff --cached --check
git commit -m "docs: plan private proof cost binding"
```

- [x] **Step 2: Switch all authority pointers cohesively**

Update the plan index and roadmap §9 to say 06A is complete and 06B-1 is the only executable child. Append F-0007 authorization with plan path, reviewed scope, provider/cost zero, Mock refusal, both parked blockers, `G0/G1/G2=OPEN`, `PASS_TO_PHASE_0B=false`, sync risk, and rollback. F-0006/F-0006A remain byte-preserved history.

```bash
git add docs/superpowers/plans/2026-08-30-06b1-proof-commitment-and-cost-binding.md \
  docs/superpowers/plans/README.md \
  docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md \
  docs/architecture/codex-fork-patch-ledger.md
git diff --cached --check
git commit -m "docs: authorize private proof cost binding"
```

No implementation task may start before this commit exists.

---

## File Structure

- `ai-ip-evals/schemas/provider-rate-card.schema.json` — exact CNY per-million-token launch rate card.
- `ai-ip-evals/schemas/billing-policy.schema.json` — exact charging and reasoning-token policy.
- `ai-ip-evals/schemas/fx-policy.schema.json` — exact `notApplicable` CNY-to-CNY policy for the launch route.
- `ai-ip-evals/schemas/provider-budget-evidence.schema.json` — exact private hard-limit evidence.
- `ai-ip-evals/schemas/supplier-statement.schema.json` — optional per-arm actual supplier charge.
- `ai-ip-evals/schemas/cost-receipt.schema.json` — amended exact receipt including pair/execution/ledger, currency, effective-time, and ceiling bindings.
- `codex-rs/ai-ip-eval/src/cost_contracts.rs` — embedded schemas, exact wire types, and duplicate-key-safe validators for the five inputs and receipt.
- `codex-rs/ai-ip-eval/src/cost_inputs.rs` — bounded retained filesystem authority for the five inputs and the optional supplier statement; kept separate so both production modules remain below 500 lines.
- `codex-rs/ai-ip-eval/src/proof_commitment.rs` — secret key type, normative HMAC framing, public-run derivation, and Merkle primitive used by later 06B-2.
- `codex-rs/ai-ip-eval/src/cost.rs` — checked CNY token-cost arithmetic and private receipt model; no filesystem access.
- `codex-rs/ai-ip-eval/src/cost_authority.rs` — frozen-context/pair/ledger verification and one-arm receipt transaction.
- `codex-rs/ai-ip-eval/src/cost_binding.rs` — immutable-manifest sidecar verification and transaction.
- `codex-rs/ai-ip-eval/src/*_tests.rs` — sibling unit and mutation matrices for the matching production module.
- `codex-rs/ai-ip-eval/tests/cost_cli.rs` — real binary/Clap coverage for both commands and exact Replay/Native-Mock refusal.
- `codex-rs/ai-ip-eval/tests/fixtures/contracts/06b1/**` — canonical and one-field-negative private pricing/budget/statement/receipt fixtures plus shared HMAC/Merkle vectors.
- `scripts/ai_ip/foundation/verify_evidence.py` / `test_verify_evidence.py` — public-vector-only Python cross-check; no private-root/key input.
- `codex-rs/ai-ip-eval/src/model.rs`, `lib.rs`, `runner.rs` — narrow typed CLI, module wiring, and key/public-ID freeze hook.
- `codex-rs/ai-ip-eval/BUILD.bazel` — exact compile-time inputs for embedded canonical cost fixtures; test fixture runfiles remain separate.
- `docs/architecture/codex-fork-patch-ledger.md` — append-only implementation result after complete review.

## Frozen Private Wire Contracts

All five input documents are compact or pretty JSON values accepted by duplicate-key-safe parse + JSON Schema + typed deep equality. Their original file bytes/SHA remain the authority bound by the held-out attestation and frozen context. Each input and optional supplier statement has a `64 * 1024` byte cap; a generated cost receipt has a `128 * 1024` cap and a generated binding has a `16 * 1024` cap. Generated receipt/binding bytes must equal RFC 8785/JCS exactly; validators reject a semantically equal pretty or differently ordered encoding.

Rust and source-input arithmetic may retain `u64`, but every nonnegative integer emitted on the exact-JCS `CostReceiptV1` wire is restricted to `0..=9_007_199_254_740_991`. A larger calculated value fails before receipt publication; it is never rounded or silently changed for JCS.

Every provider/model/revision label uses the exact ASCII schema pattern `^[A-Za-z0-9][A-Za-z0-9._:+/@() -]{0,199}$`; no controls, line breaks, leading whitespace, or non-ASCII lookalikes are allowed. Every timestamp in this child (`effectiveAt`, `expiresAt`, `validFrom`, `validUntil`, `issuedAt`, `calculatedAt`, and `boundAt`) is UTC with exactly millisecond precision, matching `^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}\\.\\d{3}Z$`, and must round-trip through `DateTime::parse_from_rfc3339` plus `to_rfc3339_opts(SecondsFormat::Millis, true)` unchanged.

```text
ProviderRateCardV1
  schemaVersion=1
  providerLabel: exact label pattern above
  modelLabel: exact label pattern above
  currency="CNY"
  rateUnit="fenPerMillionTokens"
  effectiveAt: exact UTC-millisecond timestamp
  expiresAt: exact UTC-millisecond timestamp|null
  uncachedInputFenPerMillion: u64
  cachedInputFenPerMillion: u64
  cacheWriteInputFenPerMillion: u64
  outputFenPerMillion: u64

BillingPolicyV1
  schemaVersion=1
  providerLabel: exact match to rate card/context
  currency="CNY"
  effectiveAt: exact UTC-millisecond timestamp
  sourceCommitment: lowercase SHA-256 of the frozen private source evidence
  reasoningTokensBilledSeparately=false
  supplierActualPrecedence="maxEstimatedOrSupplierActual"
  rounding="ceilingToFen"

FxPolicyV1
  schemaVersion=1
  mode="notApplicable"
  sourceCurrency="CNY"
  targetCurrency="CNY"
  numerator=1
  denominator=1
  effectiveAt: exact UTC-millisecond timestamp

ProviderBudgetEvidenceV1
  schemaVersion=1
  providerLabel: exact match to context
  approvalId: exact match to held-out attestation
  currency="CNY"
  accountScopeCommitment: lowercase SHA-256
  prepaidOrHardLimitFen: u64, <= approvedTotalFen
  validFrom: exact UTC-millisecond timestamp
  validUntil: exact UTC-millisecond timestamp

SupplierStatementV1
  schemaVersion=1
  pairId: lowercase SHA-256
  condition="generic"|"candidate"
  providerLabel: exact manifest provider
  actualModelRevision: exact manifest revision
  currency="CNY"
  actualFen: u64
  issuedAt: exact UTC-millisecond timestamp, >= arm completion and < retention deadline
  statementReferenceCommitment: lowercase SHA-256
```

The supplier-condition-mismatch fixture is a concrete syntactically valid opposite-arm statement. Task 1 loads it as an exact wire fixture; Task 5C alone rejects it against the expected selected arm/pair/provider/model authority.

`CostReceiptV1` is the exact amended `cost-receipt.schema.json` wire:

```text
schemaVersion=1
pairId, frozenRunContextSha256, executionContextSha256,
executionManifestSha256, brokerReceiptSha256, pairReceiptSha256,
attemptLedgerSha256,
condition, runOrdinal, executionMode="live",
attemptIndexRootSha256,
attemptRange={startInclusive,endExclusive},
providerLabel, actualModelRevision,
rateCardSha256, billingPolicyCommitment, fxPolicySha256,
providerBudgetEvidenceSha256,
providerRequestAttemptCount, providerCompletedResponseCount,
  usageScope="rootSessionTree", usage,
currency="CNY", rateEffectiveAt,
fx={mode:"notApplicable",numerator:1,denominator:1},
ceilings={approvedPerRunFen,approvedTotalFen,prepaidOrHardLimitFen,
          maxProviderRequestAttempts,maxTotalTokens,maxElapsedSeconds},
calculatedAt,
calculation={rateUnit:"fenPerMillionTokens",rounding:"ceilingToFen",
             reasoningTokensBilledSeparately:false},
estimatedFen,
supplierStatementSha256:null|lowercase SHA-256,
supplierActualFen:null|u64,
chargedFen,
withinCeilings
```

`billingPolicyCommitment` is exactly the lowercase SHA-256 of the complete private `billing-policy.json` bytes; `BillingPolicyV1.sourceCommitment` separately binds the underlying provider/contract source. The launch FX document always says `notApplicable` because both rate and charge are already CNY.

The existing immutable execution manifest remains `usageScope="completeNativeThreadTree"`. Only after `verify_native_proof_ledger` proves the manifest, archive, broker/App Server response/usage multiset, and ledger-arm usage are exactly equal may the cost wire project that already verified whole root-session tree as the Contract 4 literal `usageScope="rootSessionTree"`; it does not change or reinterpret manifest bytes.

Hard gates and post-run validity ceilings are disjoint. Attempt cap, request deadline, per-request fixed `maxOutputTokens`, arm order, endpoint commitment, and `prepaidOrHardLimitFen <= approvedTotalFen` are pre-run constraints; sealed evidence contradicting any of them is fail-closed and produces no receipt. Only per-arm `usage.totalTokens <= maxTotalTokens` and `chargedFen <= approvedPerRunFen` feed `withinCeilings`; a valid overage produces an immutable receipt with `withinCeilings=false`. The later 06B-2 verifier computes the two-receipt total against `approvedTotalFen`; a single receipt never claims pair-total validity.

---

### Task 1: Freeze exact pricing, budget, statement, and receipt contracts

**Files:**
- Create: `ai-ip-evals/schemas/provider-rate-card.schema.json`
- Create: `ai-ip-evals/schemas/billing-policy.schema.json`
- Create: `ai-ip-evals/schemas/fx-policy.schema.json`
- Create: `ai-ip-evals/schemas/provider-budget-evidence.schema.json`
- Create: `ai-ip-evals/schemas/supplier-statement.schema.json`
- Modify: `ai-ip-evals/schemas/cost-receipt.schema.json`
- Modify: `ai-ip-evals/schemas/BUILD.bazel`
- Modify: `codex-rs/ai-ip-eval/BUILD.bazel`
- Create: `codex-rs/ai-ip-eval/src/cost_contracts.rs`
- Create: `codex-rs/ai-ip-eval/src/cost_contracts_tests.rs`
- Create: `codex-rs/ai-ip-eval/tests/fixtures/contracts/06b1/**`
- Modify: `codex-rs/ai-ip-eval/src/lib.rs`
- Modify: `codex-rs/ai-ip-eval/src/contracts_tests.rs`

**Interfaces:**
- Consumes: `jcs::parse_json`, offline `jsonschema::Validator`, the wire table above, existing `Usage` and `EvaluationCondition`.
- Produces: `FrozenCostContracts::load()`, `VerifiedCostInputs`, `CostReceiptV1`, and `validate_cost_receipt(&[u8]) -> Result<CostReceiptV1>`.

- [ ] **Step 1: Write resource/shape RED tests without missing Rust symbols**

Add `cost_contract_assets_expose_exact_launch_shapes` to `contracts_tests.rs` using `find_resource!` and `serde_json::Value`. It must require all five new files, `additionalProperties=false`, exact constants shown above, and the amended receipt fields `pairId`, `executionContextSha256`, `attemptLedgerSha256`, `executionMode`, `currency`, `fx`, and `ceilings`.

Run:

```bash
just test -p codex-ai-ip-eval -E 'test(cost_contract_assets_expose_exact_launch_shapes)'
```

Expected: executable FAIL because the five assets and amended receipt fields do not exist.

- [ ] **Step 2A: Add exact schemas, fixtures, and compile/runfile data**

Create exact canonical fixtures plus one unknown-field, missing-field, wrong-type, byte-cap, label-pattern, timestamp-format/order, provider mismatch, non-CNY, non-`notApplicable` FX, supplier-condition mismatch, non-JCS generated receipt, and receipt-null-pair negative. Update the schema filegroup and resource-shape test, rerun the Step 1 selector and `bazel build //ai-ip-evals/schemas:schemas`, then review and commit only assets/fixtures:

```bash
git add ai-ip-evals/schemas/provider-rate-card.schema.json \
  ai-ip-evals/schemas/billing-policy.schema.json \
  ai-ip-evals/schemas/fx-policy.schema.json \
  ai-ip-evals/schemas/provider-budget-evidence.schema.json \
  ai-ip-evals/schemas/supplier-statement.schema.json \
  ai-ip-evals/schemas/cost-receipt.schema.json \
  ai-ip-evals/schemas/BUILD.bazel \
  codex-rs/ai-ip-eval/src/contracts_tests.rs \
  codex-rs/ai-ip-eval/tests/fixtures/contracts/06b1 \
  codex-rs/ai-ip-eval/tests/fixtures/contracts/later/cost-receipt.canonical.json \
  codex-rs/ai-ip-eval/tests/fixtures/contracts/later/cost-receipt.missing.json \
  codex-rs/ai-ip-eval/tests/fixtures/contracts/later/cost-receipt.unknown.json \
  codex-rs/ai-ip-eval/tests/fixtures/contracts/later/cost-receipt.wrong-type.json
git diff --cached --check
git commit -m "test(ai-ip-eval): freeze private proof cost assets"
```

- [ ] **Step 2B: Add the strict typed loader**

First add the exact types/signatures below, mount `cost_contracts_tests.rs`, and make every loader/validator return `anyhow::bail!("cost contracts not implemented")`. Add behavioral tests for duplicate keys, typed deep-equality mismatch, byte caps, semantic cross-field checks, and non-JCS generated receipts. Run `just test -p codex-ai-ip-eval -E 'test(executable_06b1_cost_contracts_accept_only_exact_shapes)'`; expected RED is the exact scaffold error, not an import/compile failure. Do not commit this RED.

Then implement types with `#[serde(deny_unknown_fields, rename_all = "camelCase")]`. `FrozenCostContracts::load()` compiles every schema with external resolution disabled and fails if a canonical fixture does not deep-equal its typed decode. Source inputs keep their original pretty/compact bytes; only `validate_receipt` additionally requires byte equality with `jcs::canonicalize_value(serde_json::to_value(&typed)?)`.

```rust
pub(crate) struct FrozenCostContracts { /* validators + schema shas */ }

pub(crate) struct VerifiedCostInputs {
    pub(crate) rate_card: ProviderRateCardV1,
    pub(crate) rate_card_sha256: String,
    pub(crate) billing_policy: BillingPolicyV1,
    pub(crate) billing_policy_commitment: String,
    pub(crate) fx_policy: FxPolicyV1,
    pub(crate) fx_policy_sha256: String,
    pub(crate) budget: ProviderBudgetEvidenceV1,
    pub(crate) provider_budget_evidence_sha256: String,
}

impl FrozenCostContracts {
    pub(crate) fn load() -> anyhow::Result<Self>;
    pub(crate) fn validate_inputs(
        &self,
        rate_card: &[u8],
        billing_policy: &[u8],
        fx_policy: &[u8],
        budget: &[u8],
    ) -> anyhow::Result<VerifiedCostInputs>;
    pub(crate) fn validate_supplier_statement(
        &self,
        bytes: &[u8],
    ) -> anyhow::Result<SupplierStatementV1>;
    pub(crate) fn validate_receipt(&self, bytes: &[u8]) -> anyhow::Result<CostReceiptV1>;
}
```

Run:

```bash
just test -p codex-ai-ip-eval -E 'test(executable_06b1_cost_contracts_accept_only_exact_shapes)'
bazel build //ai-ip-evals/schemas:schemas
```

Expected: PASS; existing later cost fixtures are migrated to the amended exact wire, not left as a second contract.

- [ ] **Step 3: Review and commit the Rust validator slice**

Run `git diff --check`; verify no report/index/retention schema changed. Request specification and code review; fix every Critical/Important finding. If the staged validator/tests approach 800 changed lines, split only already-GREEN validator behavior or compile-neutral unregistered fixture support; never commit the observed scaffold RED. Every resulting commit must pass the global pre-commit gate and its registered focused tests.

```bash
git add codex-rs/ai-ip-eval/src/cost_contracts.rs \
  codex-rs/ai-ip-eval/src/cost_contracts_tests.rs \
  codex-rs/ai-ip-eval/src/lib.rs
git commit -m "feat(ai-ip-eval): validate private proof cost contracts"
```

---

### Task 2: Implement normative HMAC/Merkle proof commitments

**Files:**
- Create: `codex-rs/ai-ip-eval/src/proof_commitment.rs`
- Create: `codex-rs/ai-ip-eval/src/proof_commitment_tests.rs`
- Create: `codex-rs/ai-ip-eval/tests/fixtures/contracts/06b1/proof-commitment-vectors.json`
- Modify: `codex-rs/ai-ip-eval/src/lib.rs`
- Modify: `scripts/ai_ip/foundation/verify_evidence.py`
- Modify: `scripts/ai_ip/foundation/test_verify_evidence.py`

**Interfaces:**
- Consumes: `hmac 0.12.1`, SHA-256, `jcs::canonicalize_value`, exact Contract 5 framing.
- Produces: `ProofCommitmentKey`, `proof_commitment`, `derive_public_run_id`, and `proof_merkle_root` for later 06B-2.

- [ ] **Step 1: Freeze cross-language RED vectors**

The vector file contains one 32-byte key hex, one pair ID, at least three label/JCS-value commitments, one leaf, one even node, one odd-duplication root, and one full root. Write Rust and Python tests that compare every expected lowercase hex. The Python test imports only pure helper functions and passes vector bytes; it never accepts a private-root path or real key file.

Before the recorded RED, add only compile scaffolding with the exact Task 2 signatures: every Rust function returns `anyhow::bail!("proof commitment not implemented")`, and the two Python vector helpers raise `NotImplementedError("proof commitment not implemented")`. Wire the Rust test module in `lib.rs`. There is no success branch, HMAC, or filesystem behavior in this scaffold.

Run:

```bash
just test -p codex-ai-ip-eval -E 'test(proof_commitment_vectors_match_normative_framing)'
python3 -m pytest -q -p no:cacheprovider \
  scripts/ai_ip/foundation/test_verify_evidence.py -k proof_commitment_vectors
```

Expected: both tests compile/import and fail because their scaffolds return the exact `proof commitment not implemented` error. Do not use unresolved imports as the recorded RED.

- [ ] **Step 2: Implement the exact primitives**

Use a non-`Serialize` key wrapper with no `Debug`, `Display`, or method returning key hex. Do not claim reliable memory erasure in this child because no `zeroize` dependency is added. The crate-private test constructor accepts `[u8; 32]`. Production key creation uses `OsRng.try_fill_bytes` and errors on RNG failure.

```rust
pub(crate) struct ProofCommitmentKey([u8; 32]);

impl ProofCommitmentKey {
    pub(crate) fn generate() -> anyhow::Result<Self>;
    pub(crate) fn publish_owner_only_new(&self, path: &Path) -> anyhow::Result<()>;
    pub(crate) fn read_exact_owner_only(path: &Path) -> anyhow::Result<Self>;
}

pub(crate) fn proof_commitment(
    key: &ProofCommitmentKey,
    label: &str,
    value: &serde_json::Value,
) -> anyhow::Result<String>;

pub(crate) fn derive_public_run_id(
    key: &ProofCommitmentKey,
    pair_id: &str,
) -> anyhow::Result<String>;

pub(crate) fn proof_merkle_root(
    leaves: &std::collections::BTreeMap<String, String>,
) -> anyhow::Result<String>;
```

`publish_owner_only_new` delegates to `crate::secure_fs::write_owner_only_new` and the exact reader uses the retained owner-only/single-link bounded path with a 32-byte cap; do not use the similarly named runner helper. `proof_commitment` canonicalizes `value` internally with `crate::jcs::canonicalize_value` and takes the framed length from those final UTF-8 bytes; callers cannot supply allegedly canonical raw bytes. `derive_public_run_id` validates lowercase 64-hex `pair_id`, constructs `Value::String(pair_id)`, and calls that primitive with label `publicRunId`. Enforce label length to `u32`, value length to `u64`, nonempty leaf set, lowercase 64-hex leaf values, UTF-8 byte ordering, odd-node duplication, and rejection of a `proofRootSha256` input leaf.

Run both commands from Step 1. Expected: PASS with the same fixture in both languages.

- [ ] **Step 3: Review and commit the primitive slice**

Confirm neither implementation prints the key or reads environment variables. Request specification and code review; fix every Critical/Important finding.

```bash
git add codex-rs/ai-ip-eval/src/proof_commitment.rs \
  codex-rs/ai-ip-eval/src/proof_commitment_tests.rs \
  codex-rs/ai-ip-eval/src/lib.rs \
  codex-rs/ai-ip-eval/tests/fixtures/contracts/06b1/proof-commitment-vectors.json \
  scripts/ai_ip/foundation/verify_evidence.py \
  scripts/ai_ip/foundation/test_verify_evidence.py
git commit -m "feat(ai-ip-eval): freeze proof commitment primitives"
```

---

### Task 3A: Normalize retention-managed private cost inputs

**Files:**
- Modify: `ai-ip-evals/schemas/frozen-run-context.schema.json`
- Modify: `ai-ip-evals/schemas/held-out-attestation.schema.json`
- Create: `codex-rs/ai-ip-eval/src/cost_inputs.rs`
- Create: `codex-rs/ai-ip-eval/src/cost_inputs_tests.rs`
- Modify: `codex-rs/ai-ip-eval/src/cost_contracts.rs`
- Modify: `codex-rs/ai-ip-eval/src/cost_contracts_tests.rs`
- Modify: `codex-rs/ai-ip-eval/src/lib.rs`
- Modify: `codex-rs/ai-ip-eval/src/runner.rs`
- Modify: `codex-rs/ai-ip-eval/src/eval_tests.rs`
- Modify: `codex-rs/ai-ip-eval/src/blind_verify_semantics_bindings.rs`
- Modify: `codex-rs/ai-ip-eval/src/blind_verify_semantics_tests.rs`
- Modify: `codex-rs/ai-ip-eval/src/proof_archive_tests.rs`
- Modify: `codex-rs/ai-ip-eval/tests/fixtures/contracts/06a/canonical-native-context.json`
- Modify: `codex-rs/ai-ip-eval/tests/fixtures/contracts/06a/canonical-native-attestation.json`

**Interfaces:**
- Consumes: Task 1 strict input validators, canonical Native `privateRoot`, existing retained-file/owner-only checks.
- Produces: cost artifacts whose only source of truth is the fixed, retention-managed `privateRoot/inputs` table.

- [ ] **Step 1: Write fixed-path and exact-input REDs**

The five `LiveFreezeArgs` paths remain explicit for auditability, but must equal these exact canonical paths:

```text
inputs/held-out-attestation.json
inputs/provider-budget-evidence.json
inputs/rate-card.json
inputs/billing-policy.json
inputs/fx-policy.json
```

Add executable tests that pass an equally encoded file from Home, repo, Downloads-like TempDir siblings, the old `frozen-inputs/` location, a symlink/reparse point, hardlink, wrong fixed leaf, and a post-open replacement. Each must fail before frozen context publication. Before freeze, `inputs/` must contain exactly the five fixed regular files above and no other file, directory, link, or nested entry; add unexpected `foo`, pre-existing `case/`, pre-existing `materials-manifest.json`, and pre-existing `supplier-statements/` REDs. Canonical typed files at the five fixed paths must pass, remain on the same inode, and be inventory-covered when the normal pair producer bootstraps inventory.

Freeze this exact typed Native-context field and schema shape: `supplierStatementPolicy={directory:"inputs/supplier-statements",allowedLeaves:["generic.json","candidate.json"],nestedEntriesAllowed:false}`. The ordered two-leaf array is exact; unknown fields, reversed/extra/missing leaves, another directory, or `nestedEntriesAllowed=true` fail typed/schema equality. Pre-create that owner-only empty directory and owner-only `coordinator/cost/` during freeze so the initial inventory covers both fixed parents. An absent statement is valid; this is the only post-freeze input namespace 06B-1 may append.

Also freeze the artifact-key rename `providerBudgetReceipt -> providerBudgetEvidence`; stale-key and both-key contexts/attestations fail. `rateCard`, `billingPolicy`, and `fxPolicy` remain exact. The cost inputs are not copied to a second location.

Run:

```bash
just test -p codex-ai-ip-eval -E 'test(native_freeze_requires_retention_managed_cost_inputs) | test(native_cost_input_artifacts_use_exact_names_and_bytes)'
```

Expected: executable FAIL because current code accepts arbitrary source paths, copies into `frozen-inputs`, and uses `providerBudgetReceipt`.

- [ ] **Step 2: Install one retained exact-input authority**

Use a focused `RetainedExactPrivateInput<T>` in `cost_inputs.rs` backed by `RetainedBoundedFile`; it performs a `64 * 1024` bounded retained read, duplicate-key-safe parsing, schema validation, typed deep equality, exact artifact SHA binding, and `reverify_unchanged()` before context publication. `cost_contracts.rs` exposes only the typed per-document validators required by that retained wrapper. Do not add another path reader to `runner.rs`.

`freeze_live_context` first enumerates the retained owner-only `inputs/` handle and deep-equals its entries to the exact five-file initial allowlist; it validates the five CLI paths against that same table, consumes those retained tokens, and places their exact canonical paths/SHA values in frozen artifacts. Generated prompt/schema/Skill files may remain below the existing generated directory; the five coordinator-provided run inputs remain only in `inputs/`, and the four cost inputs never enter `frozen-inputs/`. Replace the current `import_live_source_proof`/`_inner` ownership conflict with one narrow `import_live_source_proof_into_existing_inputs`: the importer consumes that already verified directory handle, creates only fresh `inputs/case/` plus the case/material proof-copy leaves and necessary manifest, and consumes the retained in-place `inputs/held-out-attestation.json` token instead of recreating `inputs/` or copying the attestation. Any pre-existing/extra entry, directory replacement, or attestation identity drift fails before context publication. Only after this initial allowlist proof may freeze create the exact `case/`, manifest, typed supplier-policy directory, and `coordinator/cost/`; it fabricates no statement or output. Update the held-out attestation and context contracts to the exact artifact name.

Run the focused selector plus existing Native freeze/verification selectors. Expected: PASS; no test writes a cost input outside the private root.

- [ ] **Step 3: Review and commit the private-input correction**

Request specification and code review focused on source-copy retention, replacement races, schema/typed equality, and artifact-name compatibility. If the staged diff approaches 800 lines, commit only compile-neutral fixture migrations that leave all existing tests GREEN under `test(ai-ip-eval): align private cost input fixtures`, then split fully GREEN retained-input vertical behavior as needed; never commit a failing scaffold. Every commit must pass the same global `<800` gate.

```bash
git add ai-ip-evals/schemas/frozen-run-context.schema.json \
  ai-ip-evals/schemas/held-out-attestation.schema.json \
  codex-rs/ai-ip-eval/src/cost_inputs.rs \
  codex-rs/ai-ip-eval/src/cost_inputs_tests.rs \
  codex-rs/ai-ip-eval/src/cost_contracts.rs \
  codex-rs/ai-ip-eval/src/cost_contracts_tests.rs \
  codex-rs/ai-ip-eval/src/lib.rs \
  codex-rs/ai-ip-eval/src/runner.rs \
  codex-rs/ai-ip-eval/src/eval_tests.rs \
  codex-rs/ai-ip-eval/src/blind_verify_semantics_bindings.rs \
  codex-rs/ai-ip-eval/src/blind_verify_semantics_tests.rs \
  codex-rs/ai-ip-eval/src/proof_archive_tests.rs \
  codex-rs/ai-ip-eval/tests/fixtures/contracts/06a/canonical-native-context.json \
  codex-rs/ai-ip-eval/tests/fixtures/contracts/06a/canonical-native-attestation.json
git commit -m "fix(ai-ip-eval): retain exact private cost inputs"
```

---

### Task 3B: Generate the common Replay/Native commitment key and correct `publicRunId`

**Files:**
- Modify: `codex-rs/ai-ip-eval/src/proof_commitment.rs`
- Modify: `codex-rs/ai-ip-eval/src/proof_commitment_tests.rs`
- Modify: `codex-rs/ai-ip-eval/src/runner.rs`
- Modify: `codex-rs/ai-ip-eval/src/eval_tests.rs`
- Modify: `codex-rs/ai-ip-eval/src/contracts_tests.rs`
- Modify: `ai-ip-evals/schemas/frozen-run-context.schema.json`
- Modify: `codex-rs/ai-ip-eval/tests/fixtures/contracts/06a/canonical-native-context.json`
- Modify: `codex-rs/ai-ip-eval/tests/fixtures/contracts/06a/canonical-replay-context.json`
- Modify: `codex-rs/ai-ip-eval/tests/fixtures/contracts/06a/negative-cases.json`

**Interfaces:**
- Consumes: Task 2 key/public-ID primitive, existing `write_owner_only_new`, canonical Native private root and later inventory bootstrap.
- Produces: `<privateRoot>/coordinator/commitment-key.bin`, a private `commitmentKeySha256` context binding, and Replay/Native frozen contexts whose `publicRunId` is reverified against that retained key and `pairId` on every C1 read.

- [ ] **Step 1: Write two current-behavior REDs**

Add tests proving:

1. two Replay freezes and two Native freezes with the same non-secret labels each receive different `publicRunId` values; and
2. a retained owner-only key reader reproduces the context's `commitmentKeySha256` and `publicRunId` from the fixed key path and `pairId`.

Also assert mode `0600` on Unix/current-user-only DACL on Windows, single link, exact 32 bytes, `create_new` collision refusal, no raw key bytes in frozen context/manifest/log text, and later private inventory coverage for both Replay and Native pair bootstraps. Add bit-mutation and same-length replacement rows for key, `commitmentKeySha256`, pair ID, and public ID; both `verify_replay_frozen_context` and `verify_frozen_context` must fail before returning their mode-specific token.

Run:

```bash
just test -p codex-ai-ip-eval -E 'test(freeze_derives_public_run_id_from_private_commitment_key) | test(freeze_refuses_existing_commitment_key)'
```

Expected: executable FAIL because current Replay/Native contexts use low-entropy public-ID inputs and no common key file exists.

- [ ] **Step 2: Add the narrow freeze hook**

Add one common helper that creates a missing owner-only `<privateRoot>/coordinator` with `crate::secure_fs::create_owner_only_dir_new` or validates the exact existing owner-only directory without following links; Replay currently has no coordinator and must not assume it exists. For both Replay and Native freeze, generate key bytes, write and `fsync` the exact fixed leaf with `create_new`, derive its SHA and the public ID from the already computed pair ID, then construct the mode-specific context. If context validation or publication fails after key creation, leave the private proof root for explicit audited cleanup; never overwrite/reuse the key.

Replace only:

```rust
public_run_id: sha256(args.provider_label.as_bytes()),
```

with a call into `proof_commitment`; update `ReplayFrozenContext`, its exact producer encoding, and its independent verification path analogously without collapsing the two existing types. Extend both branches of `frozen-run-context.schema.json` and both exact canonical fixtures with `publicRunId` and `commitmentKeySha256`, never a raw key or caller-selected key path. Add `RetainedProofCommitmentKey::read_fixed(private_root)` backed by the exact 32-byte secure retained reader. Both `verify_frozen_context` and `verify_replay_frozen_context` read that fixed file, require its SHA to equal `commitmentKeySha256`, recompute `derive_public_run_id(key, pairId)`, compare exact equality, and retain the key authority before returning. Both `VerifiedFrozenContext::reverify_all()` and `VerifiedReplayFrozenContext::reverify_all()` repeat key identity/permission/length, SHA, pair-ID, and HMAC equality checks.

Run the focused selector. Expected: PASS, including key inventory coverage after normal Replay and Native/Mock pair fixtures initialize their inventories.

- [ ] **Step 3: Review and commit the freeze correction**

Request specification and code review focused on key disclosure, partial-failure behavior, owner-only creation, and HMAC input framing. Fix every Critical/Important finding.

```bash
git add codex-rs/ai-ip-eval/src/proof_commitment.rs \
  codex-rs/ai-ip-eval/src/proof_commitment_tests.rs \
  codex-rs/ai-ip-eval/src/runner.rs \
  codex-rs/ai-ip-eval/src/eval_tests.rs \
  codex-rs/ai-ip-eval/src/contracts_tests.rs \
  ai-ip-evals/schemas/frozen-run-context.schema.json \
  codex-rs/ai-ip-eval/tests/fixtures/contracts/06a/canonical-native-context.json \
  codex-rs/ai-ip-eval/tests/fixtures/contracts/06a/canonical-replay-context.json \
  codex-rs/ai-ip-eval/tests/fixtures/contracts/06a/negative-cases.json
git commit -m "fix(ai-ip-eval): derive private proof public identity"
```

---

### Task 4: Implement checked CNY cost arithmetic

**Files:**
- Create: `codex-rs/ai-ip-eval/src/cost.rs`
- Create: `codex-rs/ai-ip-eval/src/cost_tests.rs`
- Modify: `codex-rs/ai-ip-eval/src/lib.rs`

**Interfaces:**
- Consumes: authority-verified `VerifiedCostInputs`, `Usage`, optional already validated supplier actual fen, and arm ceilings.
- Produces: `calculate_cost(&CostCalculationInput) -> Result<CalculatedCost>` with no filesystem, clock, identity, or timeline access.

Freeze the pure interface before RED:

```rust
pub(crate) struct CostCalculationInput<'a> {
    pub(crate) inputs: &'a VerifiedCostInputs,
    pub(crate) usage: &'a Usage,
    pub(crate) supplier_actual_fen: Option<u64>,
    pub(crate) approved_per_run_fen: u64,
    pub(crate) max_total_tokens: u64,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct CalculatedCost {
    pub(crate) estimated_fen: u64,
    pub(crate) supplier_actual_fen: Option<u64>,
    pub(crate) charged_fen: u64,
    pub(crate) within_ceilings: bool,
}

pub(crate) fn calculate_cost(input: &CostCalculationInput<'_>) -> anyhow::Result<CalculatedCost>;
```

Arithmetic and source inputs may retain `u64`; Task 5C rejects any nonnegative value above `9_007_199_254_740_991` before exact-JCS `CostReceiptV1` publication, without rounding.

- [ ] **Step 1: Add table-driven arithmetic REDs**

Cover exact million-token rates, one-token ceiling to one fen, cached/cache-write subtraction, all-zero usage, reasoning included in output without double charge, invalid totals, negative values, cached+write > input, checked multiply/add/round overflow, actual lower/equal/higher than estimate, and the two post-run validity ceilings: per-arm total tokens and per-arm charged cost. Supplier identity, actual execution time, policy/budget windows, attempt/deadline/max-output/account hard gates are authority errors in Task 5C, not arithmetic rows or `withinCeilings` inputs.

Before the recorded RED, add only the exact `CostCalculationInput`, `CalculatedCost`, and `calculate_cost` compile scaffolding above; `calculate_cost` unconditionally returns `anyhow::bail!("cost calculation not implemented")`. Mount `cost_tests.rs` from `lib.rs`. No arithmetic or clock branch exists in the scaffold.

Use this exact calculation:

```rust
uncached = input - cached - cache_write;
numerator = uncached * rate.uncached_input
          + cached * rate.cached_input
          + cache_write * rate.cache_write_input
          + output * rate.output;
estimated_fen = (numerator + 999_999) / 1_000_000;
charged_fen = max(estimated_fen, supplier_actual_fen.unwrap_or(0));
```

Run:

```bash
just test -p codex-ai-ip-eval -E 'test(cost_arithmetic_)'
```

Expected: behavioral FAIL from compile scaffolding returning `NotImplemented`, not an unresolved symbol.

- [ ] **Step 2: Implement minimal checked arithmetic and semantic validation**

Use `i64` validation before `u128` conversion and checked arithmetic throughout. `CalculatedCost` contains only derived numeric/boolean values; receipt construction remains Task 5C. `within_ceilings` is false, not an exception, for a valid calculation above a proof ceiling; malformed/contradictory inputs are command errors and produce no receipt.

Task 5C owns all identity/timeline validation and the injected clock, then passes only verified numeric inputs here. This prevents receipt wall-clock time from being mistaken for the provider execution interval.

Run the Task 4 selector. Expected: all rows PASS.

- [ ] **Step 3: Review and commit the pure cost engine**

Request review focused on token partition, rounding, actual-charge precedence, time comparisons, and business-priority semantics.

```bash
git add codex-rs/ai-ip-eval/src/cost.rs \
  codex-rs/ai-ip-eval/src/cost_tests.rs \
  codex-rs/ai-ip-eval/src/lib.rs
git commit -m "feat(ai-ip-eval): calculate bounded proof cost"
```

---

## Normative continuation

Tasks 5A–7 are frozen in `docs/superpowers/plans/2026-08-30-06b1-cost-authority-execution-annex.md`. The annex is part of this plan's reviewed authority and inherits every constraint above; it is not independently executable. No implementation may start until both documents are committed and Task 0 switches all authority pointers.
