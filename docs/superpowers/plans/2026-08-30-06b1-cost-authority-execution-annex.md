# AI IP Proof Cost Authority Execution Annex

> **Non-authorizing companion:** This annex is normative only when the parent plan is the current executable child. It cannot activate provider execution, Live claims, or implementation by itself.

**Parent plan:** `docs/superpowers/plans/2026-08-30-06b1-proof-commitment-and-cost-binding.md`

**Inherited constraints:** All parent-plan Global Constraints, frozen wire contracts, TDD rules, line/commit caps, provider-not-run/cost-zero boundary, and 06B-2/06B-3/06C exclusions apply unchanged. Tasks 1–4 in the parent must complete before these dependency-ordered tasks.

---

### Task 5A: Retain a single-oracle cost projection

**Files:**
- Create: `codex-rs/ai-ip-eval/src/cost_authority.rs`
- Create: `codex-rs/ai-ip-eval/src/cost_authority_tests.rs`
- Modify: `codex-rs/ai-ip-eval/src/blind_verify_semantics.rs`
- Modify: `codex-rs/ai-ip-eval/src/blind_verify_semantics_tests.rs`
- Modify: `codex-rs/ai-ip-eval/src/lib.rs`

**Interfaces:**
- Consumes: the `VerifiedNativeLedger` returned by the one existing `verify_native_proof_ledger` call plus already retained exact execution context/manifests.
- Produces: a Native-only projection mechanically constructed after existing ledger/archive/multiset verification; Replay has `None`.

- [ ] **Step 1: Characterize the complete projection with a behavioral RED**

Add `cost_projection_retains_exact_verified_native_evidence` and `cost_projection_is_absent_for_replay`. The Native row deep-equals typed and raw fields; the initial RED reaches an explicit `cost projection not implemented` scaffold, never an unresolved symbol.

```rust
pub(crate) struct CostArmProjection {
    pub(crate) manifest: RunManifest,
    pub(crate) manifest_bytes: Vec<u8>,
    pub(crate) manifest_sha256: String,
    pub(crate) mode_evidence: ModeEvidence,
    pub(crate) ledger: VerifiedLedgerArm,
    pub(crate) arm_receipt: ArmReceipt,
    pub(crate) arm_receipt_sha256: String,
}

pub(crate) struct CostPairProjection {
    pub(crate) execution_mode: ExecutionMode,
    pub(crate) execution_context_sha256: String,
    pub(crate) started_at: String,
    pub(crate) deadline: String,
    pub(crate) arm_order_commitment: String,
    pub(crate) provider_endpoint_commitment: Option<String>,
    pub(crate) pair_receipt: PairReceipt,
    pub(crate) pair_receipt_sha256: String,
    pub(crate) finished_at: String,
    pub(crate) attempt_ledger_sha256: String,
    pub(crate) attempt_index_root_sha256: String,
    pub(crate) arms: [CostArmProjection; 2],
}
```

Each arm's `brokerReceiptSha256` comes from its `arm_receipt_sha256`; it is never substituted with `pair_receipt_sha256`. Each selected supplier time compares against `arm_receipt.sealed_at`. Manifest byte snapshots come from the retained `ExactDocument`, not reserialization. Preserve each complete typed `ModeEvidence`; current Native/Mock therefore has `provider_endpoint_commitment=None`, while the later Live projection extracts `Some` only from equal verified Live evidence on both arms. Build this projection immediately after `verify_native_proof_ledger` succeeds and before values are discarded; do not reopen or independently reinterpret attempt JSONL, receipts, or manifests.

Run:

```bash
just test -p codex-ai-ip-eval -E 'test(cost_projection_)'
```

Expected: RED at the scaffold, then GREEN after the mechanical projection. Review and commit:

```bash
git add codex-rs/ai-ip-eval/src/cost_authority.rs \
  codex-rs/ai-ip-eval/src/cost_authority_tests.rs \
  codex-rs/ai-ip-eval/src/blind_verify_semantics.rs \
  codex-rs/ai-ip-eval/src/blind_verify_semantics_tests.rs \
  codex-rs/ai-ip-eval/src/lib.rs
git commit -m "refactor(ai-ip-eval): retain verified proof cost projection"
```

### Task 5B: Install exact typed CLI refusal before a Live producer exists

**Files:**
- Modify: `codex-rs/ai-ip-eval/src/model.rs`
- Modify: `codex-rs/ai-ip-eval/src/lib.rs`
- Create: `codex-rs/ai-ip-eval/tests/cost_cli.rs`
- Create: `codex-rs/ai-ip-eval/tests/support/cost_world.rs`
- Modify: `codex-rs/ai-ip-eval/tests/support/mod.rs`

- [ ] **Step 1: Replace the generic placeholder and prove refusal for the right reason**

```rust
#[derive(Debug, clap::Args)]
pub struct MakeCostReceiptArgs {
    #[arg(long, value_enum)]
    pub condition: EvaluationCondition,
    #[arg(long)]
    pub supplier_statement: Option<PathBuf>,
    #[arg(long)]
    pub frozen_run_context: PathBuf,
}
```

Derive `ValueEnum` with exact values `generic|candidate`. Add exact integration functions `cost_cli_requires_one_condition`, `cost_cli_rejects_output_override`, `cost_cli_refuses_replay_evidence`, `cost_cli_refuses_native_mock_evidence`, and `cost_cli_rejects_noncanonical_supplier_path`. Replay must report `make-cost-receipt requires verified Native Live evidence; Replay is refused`; Mock must report `make-cost-receipt requires executionMode=live; mock evidence is refused`; both leave receipt/binding absent and inventory byte-identical. Those exact assertions fail against today's generic `selected evaluator workflow is not implemented` branch, so this is not a false RED. The wrapper-injected `--frozen-run-context` remains mandatory and no arbitrary output option exists.

`tests/support/cost_world.rs` owns only real Replay and Native/Mock CLI refusal worlds and reuses the existing fixture producer. It never receives crate-private authority. Run:

```bash
just test -p codex-ai-ip-eval -E 'test(cost_cli_)'
```

Then review and commit this small CLI slice:

```bash
git add codex-rs/ai-ip-eval/src/model.rs \
  codex-rs/ai-ip-eval/src/lib.rs \
  codex-rs/ai-ip-eval/tests/cost_cli.rs \
  codex-rs/ai-ip-eval/tests/support/cost_world.rs \
  codex-rs/ai-ip-eval/tests/support/mod.rs
git commit -m "feat(ai-ip-eval): refuse unverified proof cost commands"
```

### Task 5C: Prepare and calculate from a synthetic verified-Live authority

**Files:**
- Modify: `codex-rs/ai-ip-eval/src/cost_authority.rs`
- Modify: `codex-rs/ai-ip-eval/src/cost_authority_tests.rs`
- Create: `codex-rs/ai-ip-eval/src/blind_verify_cost_projection_tests.rs`
- Modify: `codex-rs/ai-ip-eval/src/blind_verify_semantics_tests.rs`
- Modify: `codex-rs/ai-ip-eval/src/lib.rs`

Before the Task 5C behavioral RED, make one test-only GREEN migration commit: move the two Task 5A real Native/Replay projection tests from `cost_authority_tests.rs` into the new `blind_verify_cost_projection_tests.rs`, switch the existing child mount in `blind_verify_semantics_tests.rs` to that file, and mount the now-available `cost_authority_tests.rs` from `lib.rs` as the sibling unit-test module required below. Preserve the two test names and assertions byte-for-byte except for imports required by the module move. This corrects the previously omitted mount-migration file authority; it may not change production behavior or weaken Task 5A coverage. Commit the independently GREEN migration as `test(ai-ip-eval): preserve verified cost projection coverage`, then begin the uncommitted Task 5C scaffold RED from that commit.

**Interfaces:**

```rust
fn run_make_cost_receipt(args: MakeCostReceiptArgs) -> anyhow::Result<()>;
fn prepare_live_cost_authority(core: PairEvidenceCore) -> anyhow::Result<VerifiedLiveCostAuthority>;
trait CostClock {
    fn now(&self) -> anyhow::Result<DateTime<Utc>>;
}
fn commit_cost_receipt(
    authority: &VerifiedLiveCostAuthority,
    condition: EvaluationCondition,
    statement: Option<&RetainedSupplierStatement>,
    clock: &dyn CostClock,
) -> anyhow::Result<CostReceiptV1>;
```

`VerifiedLiveCostAuthority` owns the verified core/projection and retained frozen cost inputs needed for reverify. Its production constructor requires core/context/both manifests to say exact Live, both typed Live mode-evidence values to agree on the endpoint/order/rate/billing/FX/budget commitments, and `provider_endpoint_commitment=Some(exact_live_value)`. Mock/Replay never fabricate one. A `#[cfg(test)] pub(crate)` constructor inside `cost_authority.rs` is the only synthetic Live seam; `cost_authority_tests.rs` is mounted by `lib.rs` as a sibling unit-test module. No integration crate can see it. `cost_binding` in Task 6 consumes this same authority and never creates a second verifier.

Task 5C review-repair authority correction: the committed arm array preserves either CSPRNG-committed permutation and must contain exactly one `generic` and one `candidate`; it must not hard-code `generic` first. Bind the top-level complete attempt-ledger SHA to the final ordered arm prefix. For this cohesive security-critical verifier, auditability takes precedence over the original soft 500-line target: `cost_authority.rs` may remain below 620 lines while the other new production modules remain below 500. Use named predicates/helpers and avoid audit-hostile long-line compression; do not add another production file for this repair.

Task 5C rejects a supplier statement whose valid wire condition, pair, provider, or model does not match the selected arm authority. Before publishing a generated exact-JCS `CostReceiptV1`, it rejects every nonnegative emitted integer above `9_007_199_254_740_991`; Rust/source values remain `u64` and are never rounded.

- [ ] **Step 1: Freeze the authority and hard-gate RED matrix**

Use these exact test names, each failing before authority/receipt publication with destinations absent and inventory bytes unchanged:

```text
cost_authority_rejects_broker_app_server_response_usage_multiset_drift
cost_authority_rejects_provider_endpoint_commitment_drift
cost_authority_rejects_arm_order_commitment_drift
cost_authority_rejects_future_attestation_signed_at
cost_authority_rejects_inverted_or_expired_retention_timeline
cost_authority_rejects_candidate_fork_sha_drift
cost_authority_rejects_provider_role_or_target_evidence_drift
cost_authority_rejects_fixed_max_output_tokens_drift
cost_authority_rejects_attempt_cap_or_deadline_hard_gate_violation
cost_authority_rejects_provider_budget_above_user_approved_total
```

The first row mutates and fully re-signs one side of the broker/App Server `(responseId, usage)` multiset. Endpoint/order rows rebuild immediate documents so they reach their dedicated equality rules. The timeline is `candidateFrozenAt < caseSelectedAt <= signedAt <= startedAt < finishedAt <= calculatedAt < retentionDeadline`; require `rateCard.effectiveAt == attestation.rateEffectiveAt`, rate/billing/FX `effectiveAt <= startedAt`, rate expiry absent or strictly `> finishedAt`, and budget `validFrom <= startedAt && validUntil > finishedAt`. Supplier satisfies `selected ArmReceipt.sealedAt <= issuedAt <= calculatedAt` and `issuedAt < retentionDeadline`. Add named effective-rate/attestation mismatch, post-start policy, rate-expiry-equals-finish, expired-rate-at-run, invalid-budget-window, calculated-before-finish, calculated-at/after-retention, and supplier-after-calculation REDs. `commit_cost_receipt` calls the fallible clock once, normalizes its ordinary possibly sub-millisecond result to one UTC-millisecond string/value, uses the selected arm receipt SHA, and emits canonical JCS that passes the strict receipt validator.

Add the explicit compile seam first with `prepare_live_cost_authority` and `commit_cost_receipt` returning exact `cost authority not implemented`; run `just test -p codex-ai-ip-eval -E 'test(cost_authority_)'` for behavioral RED, implement, rerun GREEN, and review. The RED stays uncommitted. Measure the staged slice before commit; if it approaches 800 changed lines, split only already-GREEN hard-gate vertical behavior or compile-neutral unregistered support, never failing scaffold/tests. The final production commit is:

```bash
git add codex-rs/ai-ip-eval/src/cost_authority.rs \
  codex-rs/ai-ip-eval/src/cost_authority_tests.rs \
  codex-rs/ai-ip-eval/src/lib.rs
git commit -m "feat(ai-ip-eval): verify private proof cost authority"
```

### Task 5D: Publish one exact receipt through expected-SHA inventory append

**Files:**
- Modify: `codex-rs/ai-ip-eval/src/cost_inputs.rs`
- Modify: `codex-rs/ai-ip-eval/src/cost_inputs_tests.rs`
- Modify: `codex-rs/ai-ip-eval/src/cost_authority.rs`
- Modify: `codex-rs/ai-ip-eval/src/cost_authority_tests.rs`
- Modify: `codex-rs/ai-ip-eval/src/private_inventory_batch.rs`
- Modify: `codex-rs/ai-ip-eval/src/private_inventory_batch_tests.rs`
- Modify: `codex-rs/ai-ip-eval/src/lib.rs`
- Modify: `codex-rs/ai-ip-eval/tests/cost_cli.rs`
- Modify: `codex-rs/ai-ip-eval/tests/support/cost_world.rs`

Task 5D controller authority correction: the pending supplier proof needs the private-inventory module's internal `verified_state` and trusted allowed-unrecorded projection; neither is accessible from the sibling `cost_inputs` module, and the existing score continuation is fixed to three reviewer leaves plus a blind-receipt tail. Authorize one specialized exact-one pending-entry projection in `private_inventory_batch.rs` with focused tests in `private_inventory_batch_tests.rs`. Commit that fully GREEN, CLI-dormant support slice before the transaction RED as `feat(ai-ip-eval): verify pending supplier inventory`; do not generalize it into an arbitrary public bypass. The transaction remains in `cost_inputs.rs`; `cost_authority.rs` may add only the minimal owned-token/rebuild/receipt-reverification interface and must remain below 700 lines with the Task 5C auditability improvements preserved.

Task 5D review-fix authority correction: the independent review requires production-path Replay and Native/Mock rows for an omitted selected supplier leaf that exists but is not yet inventory-recorded. Authorize only the two existing test files `tests/cost_cli.rs` and `tests/support/cost_world.rs` for those real CLI worlds and their minimum reusable pending-leaf helper. They may not add a Live success seam, alter production dispatch, weaken exact refusal strings, touch provider/network/key behavior, or introduce Task 6 `annotate-cost` behavior. No new tracked file is authorized.

- [ ] **Step 1: Record the pending-supplier/output transaction RED**

Add this exact entry point with an unconditional `anyhow::bail!("cost receipt transaction not implemented")` compile scaffold, plus a `#[cfg(test)] CostReceiptCheckpoint` hook enum that has no production effect:

```rust
fn publish_cost_receipt(
    authority: VerifiedLiveCostAuthority,
    condition: EvaluationCondition,
    supplier_path: Option<&Path>,
    clock: &dyn CostClock,
) -> anyhow::Result<()>;
```

Before implementing any transaction branch, add the `cost_receipt_transaction_` happy, two-state supplier, omitted-statement undercharge, expected-SHA replacement, and before/after-append fault rows. Run `just test -p codex-ai-ip-eval -E 'test(cost_receipt_transaction_)'`; expected behavioral RED is the exact scaffold error with no receipt write. Do not commit the RED.

- [ ] **Step 2: Implement the pending-supplier and output transaction**

The exact optional path is `inputs/supplier-statements/{generic|candidate}.json`. Resolve exactly two resumable initial states: (A) the retained leaf is absent from inventory but matches the sole pending fixed path/SHA, or (B) the retained leaf is already inventory-covered at that exact path/SHA/bytes after a prior successful supplier append. Any missing file with an inventory entry, changed bytes, different condition leaf, duplicate/extra pending leaf, or other state fails closed. For state A, use the existing score continuation pattern:

1. retain the exact `64 * 1024`-bounded supplier leaf as `RetainedPendingSupplierStatement` before ordinary pair verification;
2. build a pending inventory projection that permits only that fixed leaf and expected raw SHA, then call `verify_pair_evidence_core_from(&snapshot, inputs, pending_inventory)`;
3. prepare the Live authority, validate condition/provider/model/timeline and calculate prospective exact receipt bytes without writing;
4. reverify every retained token, then call `append_private_inventory_batch_from_root(old_root, [ExpectedInventoryEntry { exact supplier path, File, expected SHA }])`;
5. compare the returned root with a fresh `verify_private_inventory_state`, discard the now-stale pre-append core/authority, rebuild the sealed pair from normal fresh inventory, reconstruct a fresh Live authority, and deep-equal the precomputed statement/cost inputs;
6. create the fixed owner-only canonical JCS `coordinator/cost/{condition}-receipt.json` with `write_owner_only_new`, whose pre-created parent is already inventory-covered;
7. before append, reverify the current authority/input identities; append that receipt using `append_private_inventory_batch_from_root` and an `ExpectedInventoryEntry` containing the precomputed receipt SHA; compare the returned root with a fresh inventory verification; discard the stale authority and rebuild a fresh sealed pair/Live authority from the new root; then deep-equal the precomputed receipt bytes/hash and all source commitments.

State B begins from ordinary inventory verification and consumes the already covered exact statement without appending it again. A no-supplier call first proves the selected condition's fixed statement leaf is absent from both filesystem and inventory; if an exact pending or already covered statement exists, omitting `--supplier-statement` is a hard error so `supplierActualFen=null` cannot understate cost. Only after that absence proof may it skip pending-supplier retain/projection, supplier inventory append, and its post-append rebuild; it still performs ordinary pair verification, prepares/reverifies Live authority, calculates, creates the receipt, and performs expected-SHA append plus fresh-authority rebuild. Add REDs for omitted argument with pending and already inventoried statements. Never use the plain append API that learns current bytes after creation. Replacement between create and append must fail against the expected SHA. A crash after supplier append but before receipt creation is resumable through state B. Any error before receipt create leaves no receipt. A failure after receipt create but before its inventory append remains an explicit incomplete private transaction; rerun fails closed on the existing uninventoryed receipt rather than overwriting it. Recovery for that later window is not invented in 06B-1.

Implement only enough to make the recorded rows GREEN. The fault matrix covers before/after supplier append, receipt create, and receipt append, including same-length replacement. Run `just test -p codex-ai-ip-eval -E 'test(cost_receipt_transaction_) | test(cost_cli_)'`; synthetic verified-Live unit rows pass, real Replay/Mock still refuse, and no row emits a report or public decision.

Review and measure the staged slice. If it approaches 800 changed lines, split only compile-neutral unregistered fault-injection support or fully GREEN transaction behavior; never commit a failing RED. The final production commit is:

```bash
git add codex-rs/ai-ip-eval/src/cost_inputs.rs \
  codex-rs/ai-ip-eval/src/cost_inputs_tests.rs \
  codex-rs/ai-ip-eval/src/cost_authority.rs \
  codex-rs/ai-ip-eval/src/cost_authority_tests.rs \
  codex-rs/ai-ip-eval/src/private_inventory_batch.rs \
  codex-rs/ai-ip-eval/src/private_inventory_batch_tests.rs \
  codex-rs/ai-ip-eval/src/lib.rs \
  codex-rs/ai-ip-eval/tests/cost_cli.rs \
  codex-rs/ai-ip-eval/tests/support/cost_world.rs
git commit -m "feat(ai-ip-eval): seal private proof cost receipts"
```

---

### Task 6: Implement immutable `annotate-cost` sidecars

**Files:**
- Create: `codex-rs/ai-ip-eval/src/cost_binding.rs`
- Create: `codex-rs/ai-ip-eval/src/cost_binding_tests.rs`
- Modify: `codex-rs/ai-ip-eval/src/model.rs`
- Modify: `codex-rs/ai-ip-eval/src/lib.rs`
- Modify: `codex-rs/ai-ip-eval/tests/cost_cli.rs`

**Interfaces:**
- Consumes: the same `VerifiedLiveCostAuthority`, exact selected receipt/manifest/arm receipt, `CostClock`, and current private inventory.
- Produces: `AnnotateCostArgs` and a dormant Live-only transaction for `coordinator/cost/{generic|candidate}-binding.json`.

- [ ] **Step 1: Add CLI and sidecar mutation REDs**

Define:

```rust
#[derive(Debug, clap::Args)]
pub struct AnnotateCostArgs {
    #[arg(long, value_enum)]
    pub condition: EvaluationCondition,
    #[arg(long)]
    pub frozen_run_context: PathBuf,
}
```

Freeze exact compact typed sidecar fields:

```text
executionManifestSha256,costReceiptSha256,brokerReceiptSha256,
condition,boundAt
```

Before recording RED, add and register `cost_binding.rs`/`cost_binding_tests.rs` with the exact typed `CostBindingV1` fields above and the exact signature `commit_cost_binding(authority: &VerifiedLiveCostAuthority, condition: EvaluationCondition, clock: &dyn CostClock) -> Result<CostBindingV1>`; its compile scaffold unconditionally returns `anyhow::bail!("cost binding not implemented")`. Add the typed CLI args/dispatch scaffold so integration tests compile and reach the same exact error. Do not add any success branch and do not commit this RED.

Then add mutation tests and run the selector to observe that exact behavioral FAIL. In GREEN, `commit_cost_binding` calls the clock once, normalizes it to the same UTC-millisecond string/value rule, and requires `boundAt >= calculatedAt`, `boundAt >= pair_receipt.finished_at`, and `boundAt < retentionDeadline`. Tests cover exact Replay/Mock refusal messages, absent receipt, wrong condition, manifest/receipt/arm-receipt hash drift, unknown/duplicate/trailing/non-JCS fields, existing output, bad bound timeline, receipt above ceiling (binding is allowed but later proof remains invalid), manifest byte mutation, and attempts to observe any manifest byte change. Positive rows consume only the sibling-test synthetic `VerifiedLiveCostAuthority`.

Name the integration rows `cost_cli_annotate_refuses_replay_evidence` and `cost_cli_annotate_refuses_native_mock_evidence` and assert the same typed refusal wording as Task 5B with command name `annotate-cost`; today's generic unimplemented error therefore cannot satisfy RED. Run `just test -p codex-ai-ip-eval -E 'test(cost_binding_) | test(cost_cli_annotate_)'`. Expected: executable FAIL for the exact wrong behavior, not merely any error.

- [ ] **Step 2: Verify and atomically create the sidecar**

Re-run the sealed cost authority, require exact Live mode, retain the exact `128 * 1024` inventory-covered receipt, require its bytes to be canonical JCS, and compare its execution-manifest, selected arm-receipt, pair-receipt, condition, and ledger hashes with the already verified projection. Snapshot both manifest byte arrays. Produce a `16 * 1024`-bounded canonical JCS sidecar, write the fixed owner-only leaf with `write_owner_only_new`, reverify the pre-append authority/receipt, and call `append_private_inventory_batch_from_root` with an `ExpectedInventoryEntry` containing the precomputed binding SHA. Compare the returned root with fresh `verify_private_inventory_state`, discard the stale pre-append authority, rebuild sealed pair/Live authority from the new root, deep-equal the precomputed binding bytes/hash and receipt/source commitments, and prove both manifest snapshots are unchanged. Never use a plain append and do not write a receipt SHA into either manifest.

A replacement between binding create and inventory append must fail against expected SHA. Fault injection after create leaves the same explicit incomplete fail-closed transaction policy as receipts; rerun never overwrites it.

Run the selector. Expected: both sidecars pass; all mutation rows fail with no new sidecar and unchanged manifests.

- [ ] **Step 3: Review and commit the binding slice**

```bash
git add codex-rs/ai-ip-eval/src/cost_binding.rs \
  codex-rs/ai-ip-eval/src/cost_binding_tests.rs \
  codex-rs/ai-ip-eval/src/model.rs \
  codex-rs/ai-ip-eval/src/lib.rs \
  codex-rs/ai-ip-eval/tests/cost_cli.rs
git commit -m "feat(ai-ip-eval): bind cost without manifest hash cycles"
```

---

### Task 7: Verify the complete provider-free 06B-1 boundary

**Files:**
- Modify only when generated by required tooling: `codex-rs/Cargo.lock`
- Modify only when generated by required tooling: `MODULE.bazel.lock`
- Modify after implementation/review: `docs/architecture/codex-fork-patch-ledger.md`

**Interfaces:**
- Consumes: Tasks 1–6 (including 3A/3B and 5A–5D) and existing evaluator/proxy contracts.
- Produces: reviewed 06B-1 tip; no provider spend, report, public publish, retention claim, G2, or Phase 0B handoff.

- [ ] **Step 1: Run focused and full Cargo/Python regressions**

First inspect `pgrep -fl 'cargo|rustc|cargo-nextest'`. If another Rust build/test is active, wait; do not kill it and do not start a competing regression. Use the exact locked toolchain and cached dependencies:

```bash
export CARGO_HOME=/Users/yangyucheng/.cache/ai-ip-tools/cargo-nextest-0.9.103
export PATH=/Users/yangyucheng/.cache/ai-ip-tools/bazelisk-1.28.1/node_modules/.bin:/Users/yangyucheng/.cache/ai-ip-tools/cargo-nextest-0.9.103/bin:/Users/yangyucheng/.rustup/toolchains/1.95.0-x86_64-apple-darwin/bin:/usr/local/bin:/usr/bin:/bin
CARGO_NET_OFFLINE=true just test -p codex-ai-ip-eval -E 'test(cost_) | test(proof_commitment_) | test(freeze_derives_public_run_id_from_private_commitment_key)' --test-threads=1 --retries=0
CARGO_NET_OFFLINE=true just test -p codex-ai-ip-eval --test-threads=1 --retries=0
CARGO_NET_OFFLINE=true just test -p codex-responses-api-proxy --test-threads=1 --retries=0
python3 -m pytest -q -p no:cacheprovider scripts/ai_ip/foundation/test_verify_evidence.py
```

Assert `cargo nextest --version` is `cargo-nextest 0.9.103`, `rustc --version` is `rustc 1.95.0`, and `python3 -m pytest --version` is `pytest 9.0.2` before the matrix. Preserve the repository's default 60-second per-test watchdog; do not add a slow-timeout override. Record exact counts, durations, retries zero, skips, timeouts, and failures. No full-workspace test is authorized.

- [ ] **Step 2: Run Bazel/resource/lock matrix**

```bash
export CARGO_HOME=/Users/yangyucheng/.cache/ai-ip-tools/cargo-nextest-0.9.103
export PATH=/Users/yangyucheng/.cache/ai-ip-tools/bazelisk-1.28.1/node_modules/.bin:/Users/yangyucheng/.cache/ai-ip-tools/cargo-nextest-0.9.103/bin:/Users/yangyucheng/.rustup/toolchains/1.95.0-x86_64-apple-darwin/bin:/usr/local/bin:/usr/bin:/bin
bazel --repository_disable_download mod deps --lockfile_mode=update
bazel --repository_disable_download test //codex-rs/ai-ip-eval:ai-ip-eval-unit-tests \
  //codex-rs/ai-ip-eval:codex-ai-ip-eval-bin-unit-tests \
  //codex-rs/ai-ip-eval:ai-ip-eval-cost_cli-test
bazel --repository_disable_download build //ai-ip-evals/rubrics:rubrics //ai-ip-evals/schemas:schemas
bazel --repository_disable_download mod deps --lockfile_mode=error
git diff --exit-code -- codex-rs/Cargo.lock MODULE.bazel.lock
```

`--repository_disable_download` is mandatory and a missing cache entry is a blocked prerequisite, not permission to fetch. If the generated integration-test label differs, discover it with `bazel --repository_disable_download query '//codex-rs/ai-ip-eval:*cost*'` and amend this plan before running; do not silently substitute an unrecorded target.

- [ ] **Step 3: Prove scope, privacy, and gate disposition**

```bash
git diff --check
git status --short
rg -n 'MakeCostReceipt\(PathInputArgs\)|AnnotateCost\(PathInputArgs\)|sha256\(args\.provider_label\.as_bytes\(\)\)' \
  codex-rs/ai-ip-eval/src
rg -n 'providerMode=not-run|paid provider cost|same-UID|ancestor swap|PASS_TO_PHASE_0B' \
  docs/architecture/codex-fork-patch-ledger.md
test -z "$(find . -path './.git' -prune -o -name 'commitment-key.bin' -print)"
if git diff --no-ext-diff eb64199a6..HEAD -- \
  codex-rs/ai-ip-eval ai-ip-evals/schemas scripts/ai_ip/foundation \
  ':!codex-rs/ai-ip-eval/tests/fixtures/contracts/06b1/proof-commitment-vectors.json' \
  | rg -n '(Bearer [A-Za-z0-9._-]{20,}|(?:api[_-]?key|access[_-]?token|secret[_-]?key)[[:space:]]*[:=][[:space:]]*[^[:space:],;]{12,})'; then
  echo 'credential-like value in 06B-1 diff' >&2
  false
fi
if git diff --name-only eb64199a6..HEAD | rg -n '(^|/)(report\.json|index\.json|verification\.json|retention-closeout\.json)$'; then
  echo 'unauthorized public/retention output path in 06B-1 diff' >&2
  false
fi
wc -l codex-rs/ai-ip-eval/src/{cost_contracts,cost_inputs,proof_commitment,cost,cost_binding,private_inventory_batch}.rs \
  | awk '$2 != "total" && $1 >= 500 { print; bad=1 } END { exit bad }'
test "$(wc -l < codex-rs/ai-ip-eval/src/cost_authority.rs)" -lt 700
for commit in $(git rev-list --reverse eb64199a6..HEAD); do
  git show --numstat --format= "$commit" \
    | awk -v commit="$commit" '$1 ~ /^[0-9]+$/ { changed += $1 + $2 } END { print commit, changed; exit(changed >= 800) }' || exit 1
done
```

Expected: the one stale-code search is empty, while the separate ledger disposition search returns the required nonempty lines. Locks are unchanged because this child uses already locked dependencies/globs; `cost_authority.rs` is below its explicit 700-line auditability exception, every other new or transaction-modified production module is below 500 lines, and every commit is below 800 changed lines. The ledger records provider not run and cost zero, both blockers open, 06B-1 complete, 06B-2 next, `G0/G1/G2=OPEN`, and `PASS_TO_PHASE_0B=false`. The executable scans reject a real key file, credential-like diff, or unauthorized public/retention output. Existing contract fixtures remain permitted; the one fixed synthetic 32-byte key in `proof-commitment-vectors.json` is the explicit exception and its Rust/Python vector tests prove exact reviewed bytes.

- [ ] **Step 4: Run final review, fixer, and formatter**

Obtain independent specification and code reviews with Critical 0 / Important 0. Then:

```bash
cd codex-rs
just fix -p codex-ai-ip-eval
just fmt
cd ..
```

Run no test after this step. Restore unrelated formatter chaff precisely; do not discard user changes.

- [ ] **Step 5: Append the implementation ledger and commit exact final files**

First inspect `git status --short`. If fixer/formatter changed authorized files, stage only these exact possible paths and commit the mechanical delta:

```bash
git add codex-rs/ai-ip-eval/src/cost.rs \
  codex-rs/ai-ip-eval/src/cost_authority.rs \
  codex-rs/ai-ip-eval/src/cost_binding.rs \
  codex-rs/ai-ip-eval/src/cost_contracts.rs \
  codex-rs/ai-ip-eval/src/cost_inputs.rs \
  codex-rs/ai-ip-eval/src/proof_commitment.rs \
  codex-rs/ai-ip-eval/src/cost_contracts_tests.rs \
  codex-rs/ai-ip-eval/src/cost_inputs_tests.rs \
  codex-rs/ai-ip-eval/src/proof_commitment_tests.rs \
  codex-rs/ai-ip-eval/src/cost_tests.rs \
  codex-rs/ai-ip-eval/src/cost_authority_tests.rs \
  codex-rs/ai-ip-eval/src/cost_binding_tests.rs \
  codex-rs/ai-ip-eval/src/blind_verify_semantics.rs \
  codex-rs/ai-ip-eval/src/blind_verify_semantics_bindings.rs \
  codex-rs/ai-ip-eval/src/blind_verify_semantics_tests.rs \
  codex-rs/ai-ip-eval/src/contracts_tests.rs \
  codex-rs/ai-ip-eval/src/eval_tests.rs \
  codex-rs/ai-ip-eval/src/proof_archive_tests.rs \
  codex-rs/ai-ip-eval/src/model.rs \
  codex-rs/ai-ip-eval/src/lib.rs \
  codex-rs/ai-ip-eval/src/runner.rs \
  codex-rs/ai-ip-eval/tests/cost_cli.rs \
  codex-rs/ai-ip-eval/tests/support/cost_world.rs \
  codex-rs/ai-ip-eval/tests/support/mod.rs
git diff --cached --check
git diff --cached --quiet || git commit -m "style(ai-ip-eval): retain proof cost formatter output"
```

Restore any unrelated formatter chaff before staging. Then append exact `F-0007A` containing implementation commits, paths, test counts/IDs, provider/cost disposition, private/public boundary, incomplete-transaction behavior, both parked blockers, sync risk, and reverse rollback order, and commit it exactly:

```bash
git add docs/architecture/codex-fork-patch-ledger.md
git diff --cached --check
git commit -m "docs: record private proof cost binding implementation"
git status --short
```

Expected final state: clean worktree; `MakeCostReceipt` and `AnnotateCost` are implemented, all other later CLI placeholders remain fail-closed and unimplemented. Authorize 06B-2 only after its own plan and independent review.
