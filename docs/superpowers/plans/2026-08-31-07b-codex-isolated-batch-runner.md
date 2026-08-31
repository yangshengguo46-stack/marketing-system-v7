# 07B Codex Isolated Batch Runner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a provider-free-first runner that sends one sealed marketing case to fully isolated stock and modified Codex App Server agents, seals private evidence, imports anonymous outputs into 07A, and then performs one separately authorized GLM smoke pair through a bounded local credential proxy.

**Architecture:** A Python controller owns typed plans, isolation, state transitions, receipts, and 07A import. Promptfoo 0.122.0 is an exact-pinned, replaceable App Server subprocess adapter only; a proof-only Rust bounded proxy holds the real provider credential outside candidate environments for the final GLM smoke.

**Tech Stack:** Python 3.11, `jsonschema==4.25.1`, `pytest==9.0.2`, Promptfoo 0.122.0, Node.js `>=22.22.0`, pnpm 10.34.5, Codex App Server JSON-RPC, Rust 1.95, existing `codex-responses-api-proxy`, Bazel 9.0.0, SHA-256 canonical JSON.

**Spec:** `docs/superpowers/specs/2026-08-31-codex-isolated-batch-runner-design.md`

## Global Constraints

- The main comparison is one sealed question executed by fully isolated Stock and Modified candidate agents; ordinary collaboration subagents are not evidence-bearing candidates.
- Stock is pinned upstream commit `4ef1d4b89`; Modified is the selected fork commit at plan sealing time. Both binary SHA-256 values are recomputed at preflight.
- Promptfoo is pinned exactly to `0.122.0`; do not use `latest`, a range, a global install, or `pnpm dlx`.
- Use Node.js `>=22.22.0` and repository-declared pnpm `10.34.5`; invoke `npx --yes pnpm@10.34.5` where Corepack is absent.
- Use `uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1` for every Python test command.
- Production Python modules remain below 500 lines. Do not add 07B logic to `codex-rs/ai-ip-eval/src/runner.rs`, `app_server.rs`, or `codex-core`.
- Promptfoo never sees rubric, reference dossier, outcome packet, real arm mapping, or release authority. Ignore its assertions and scores.
- Candidate App Servers use fresh `CODEX_HOME`, workspace, thread, cache, temp, log, and Promptfoo roots. Set `reuse_server=false`, `inherit_process_env=false`, cache off, concurrency one.
- Candidate processes receive a loopback dummy bearer only. A real Volcengine key may exist only in the controller and proof-only proxy process.
- No paid call occurs before provider-free gates pass and the user approves one exact authorization showing model, route, request count, token/time and CNY ceilings.
- First live route is `glm-5-2-260617` at `https://ark.cn-beijing.volces.com/api/v3/responses`; it is not an independent cross-provider proof.
- A mechanical failure invalidates the pair. Failed attempts are immutable; retry requires a new attempt ID and pre-sealed policy.
- Do not run direct `cargo test`; use `just test -p codex-ai-ip-eval` and `just test -p codex-responses-api-proxy`. Ask before a complete workspace `just test`.
- Keep each non-mechanical commit below 800 changed lines. Run tests before `just fix`/`just fmt`; do not test after formatter/fixer commands.

## File Map

| Path | Responsibility |
|---|---|
| `scripts/ai_ip/eval_lab/batch_contracts.py` | Schema loading, canonical commitments, validation |
| `scripts/ai_ip/eval_lab/batch_plan.py` | Binary/treatment verification, tree hashing, plan sealing |
| `scripts/ai_ip/eval_lab/batch_isolation.py` | Fresh candidate cells, safe copies, environment allowlist |
| `scripts/ai_ip/eval_lab/batch_receipts.py` | Attempt/pair receipts and failure classification |
| `scripts/ai_ip/eval_lab/batch_controller.py` | Randomized two-arm state machine |
| `scripts/ai_ip/eval_lab/batch_promptfoo.py` | Static Promptfoo config, invocation, result parsing |
| `scripts/ai_ip/eval_lab/batch_loopback.py` | Provider-free Responses SSE fixture |
| `scripts/ai_ip/eval_lab/batch_import.py` | One-time private import and 07A handoff |
| `scripts/ai_ip/eval_lab/batch_live.py` | One-shot authorization and bounded proxy lifecycle |
| `scripts/ai_ip/eval_lab/batch_cli.py` | Non-echoing 07B CLI |
| `codex-rs/ai-ip-eval/src/bounded_proxy.rs` | Proof-only request/token/time gate |
| `codex-rs/ai-ip-eval/src/bin/ai_ip_bounded_responses_proxy/main.rs` | Stdin-credential proxy binary |
| `ai-ip-evals/lab/schemas/*run*.schema.json` | Frozen wire contracts |
| `ai-ip-evals/lab/fixtures/batch-runner/**` | Positive and malicious fixtures |
| `ai-ip-evals/lab/promptfoo/package.json` | Exact Promptfoo dependency |

---

### Task 0: Authorize the executable child

**Files:**
- Modify: `docs/architecture/codex-fork-patch-ledger.md`

**Interfaces:**
- Consumes: design commits `1c0cd8177` and `ee478e96c`.
- Produces: append-only `F-0009`, `MARKETING_EVAL_LAB_07B=AUTHORIZED_NOT_IMPLEMENTED`.

- [ ] **Step 1: Append the authority record**

Record owner 07B; add-only Python lab modules, schemas, Promptfoo workspace, proof-only bounded proxy, tests, fixtures and docs; preserve product App Server/core and 06A/06B1 behavior; provider-free first; live requires one-shot authorization; no business-quality, G2, or Phase 0B claim.

- [ ] **Step 2: Verify and commit**

```bash
rg -n "F-0009|MARKETING_EVAL_LAB_07B=AUTHORIZED_NOT_IMPLEMENTED|PASS_TO_PHASE_0B=false" docs/architecture/codex-fork-patch-ledger.md
git diff --check
git add docs/architecture/codex-fork-patch-ledger.md
git commit -m "docs: authorize isolated Codex batch runner"
```

### Task 1: Freeze the wire contracts

**Files:**
- Create: `ai-ip-evals/lab/schemas/binary-manifest.schema.json`
- Create: `ai-ip-evals/lab/schemas/treatment-manifest.schema.json`
- Create: `ai-ip-evals/lab/schemas/execution-profile.schema.json`
- Create: `ai-ip-evals/lab/schemas/candidate-run-plan.schema.json`
- Create: `ai-ip-evals/lab/schemas/arm-attempt-receipt.schema.json`
- Create: `ai-ip-evals/lab/schemas/paired-run-receipt.schema.json`
- Create: `ai-ip-evals/lab/schemas/candidate-pair-import-receipt.schema.json`
- Create: `ai-ip-evals/lab/schemas/live-run-authorization.schema.json`
- Create: `ai-ip-evals/lab/fixtures/batch-runner/valid-contracts.json`
- Create: `scripts/ai_ip/eval_lab/batch_contracts.py`
- Create: `scripts/ai_ip/eval_lab/test_batch_contracts.py`
- Modify: `ai-ip-evals/lab/BUILD.bazel`

**Interfaces:**
- Consumes: `contracts.load_exact_json`, `validate_contract`, `sha256_json`.
- Produces: `load_named_contract(name, path)`, `validate_named_contract(name, value)`, `seal_self_commitment(value, field)`, `verify_self_commitment(value, field)`.

- [ ] **Step 1: Write failing tests**

```python
CONTRACT_NAMES = (
    "binary-manifest", "treatment-manifest", "execution-profile",
    "candidate-run-plan", "arm-attempt-receipt", "paired-run-receipt",
    "candidate-pair-import-receipt", "live-run-authorization",
)

@pytest.mark.parametrize("name", CONTRACT_NAMES)
def test_valid_contract_fixture(name):
    fixture = load_exact_json(FIXTURE_PATH)[name]
    assert validate_named_contract(name, fixture) == fixture

def test_self_commitment_detects_mutation():
    value = {"schemaVersion": 1, "planId": "plan-1", "planSha256": "0" * 64}
    sealed = seal_self_commitment(value, "planSha256")
    verify_self_commitment(sealed, "planSha256")
    sealed["planId"] = "plan-2"
    with pytest.raises(BatchContractError, match="commitment mismatch"):
        verify_self_commitment(sealed, "planSha256")
```

- [ ] **Step 2: Observe RED**

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_contracts.py
```

- [ ] **Step 3: Implement strict schemas and validation**

Every schema uses Draft 2020-12, `additionalProperties:false`, integer-only budgets, RFC3339 timestamps and lowercase 64-hex hashes. Copy the exact field sets from design sections 8.1–8.8. Enforce these enum sets:

```json
{
  "batchKind": ["smoke", "development_blind", "diagnostic"],
  "armClass": ["stock", "modified"],
  "networkMode": ["offline", "allowlist"],
  "pairValidity": ["valid", "invalid"],
  "outputRelation": ["distinct", "canonicallyIdentical"],
  "blindDisposition": ["readyForBlindReview", "identicalTieNoReview"],
  "authorizationStatus": ["issued", "consumed"],
  "exitClassification": ["completed", "executionFailure", "schemaFailure", "budgetFailure", "evidenceFailure"]
}
```

Self-commitment copies the object, removes exactly its own commitment field, hashes canonical JSON and returns a new object. Reject absent fields, non-dicts, uppercase hashes, floats, duplicate keys and unknown names.

Use JSON Schema conditionals: a valid pair requires `outputRelation`; an invalid pair forbids it. A distinct pair requires an anonymous mapping commitment, while a canonically identical pair forbids one. Candidate import requires matching pairs of `outputRelation`/`blindDisposition`: distinct maps only to `readyForBlindReview`, identical only to `identicalTieNoReview`.

- [ ] **Step 4: Register assets, run GREEN, commit**

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_contracts.py scripts/ai_ip/eval_lab/test_contract_assets.py
bazel build --offline //ai-ip-evals/lab:lab-assets
git add ai-ip-evals/lab scripts/ai_ip/eval_lab/batch_contracts.py scripts/ai_ip/eval_lab/test_batch_contracts.py
git commit -m "test: freeze isolated batch runner contracts"
```

### Task 2: Seal manifests and run plans

**Files:**
- Create: `scripts/ai_ip/eval_lab/batch_plan.py`
- Create: `scripts/ai_ip/eval_lab/test_batch_plan.py`
- Create: `ai-ip-evals/lab/fixtures/batch-runner/stock-seed/config.toml`
- Create: `ai-ip-evals/lab/fixtures/batch-runner/modified-seed/config.toml`
- Create: `ai-ip-evals/lab/fixtures/batch-runner/workspace/case-input.json`
- Modify: `ai-ip-evals/lab/BUILD.bazel`

**Interfaces:**
- Consumes: Task 1.
- Produces: `sha256_file`, `sha256_tree`, `verify_binary_manifest`, `verify_treatment_manifest`, `seal_candidate_run_plan`, `verify_candidate_run_plan`, `verify_effective_condition_parity`.

- [ ] **Step 1: Write failing hashing and mutation tests**

```python
def test_tree_hash_rejects_links_and_is_order_independent(tmp_path):
    left = tmp_path / "left"; right = tmp_path / "right"
    left.mkdir(); right.mkdir()
    (left / "b").write_bytes(b"two"); (left / "a").write_bytes(b"one")
    (right / "a").write_bytes(b"one"); (right / "b").write_bytes(b"two")
    assert sha256_tree(left) == sha256_tree(right)
    (right / "link").symlink_to(right / "a")
    with pytest.raises(BatchPlanError, match="symbolic link"):
        sha256_tree(right)

def test_mutated_plan_is_rejected(valid_plan):
    sealed = seal_candidate_run_plan(valid_plan)
    sealed["tokenBudget"] += 1
    with pytest.raises(BatchPlanError, match="plan commitment"):
        verify_candidate_run_plan(sealed)
```

- [ ] **Step 2: Run RED**

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_plan.py
```

- [ ] **Step 3: Implement bounded hashing and parity**

Hash sorted canonical entries shaped as:

```python
{"entries": [{"mode": 0o600, "path": "config.toml", "sha256": "0000000000000000000000000000000000000000000000000000000000000000", "size": 123}]}
```

Reject absolute/dot paths, symlinks, sockets, devices, FIFOs, `st_nlink != 1`, files over 8 MiB, trees over 64 MiB and files changed while read. Runtime parity compares exactly:

```python
PARITY_FIELDS = (
    "caseBundleSha256", "caseAnswerSchemaId", "modelRouteRef",
    "executionProfileRef", "workspaceTemplateSha256", "promptfooPackageVersion",
    "promptfooLockSha256", "timeoutBudget", "tokenBudget", "requestBudget",
    "costBudgetCny", "networkPolicy", "permissionPolicy", "toolPolicy",
)
```

Only treatment refs, opaque IDs, order and private paths differ.

The Stock seed contains no `deliver-ai-ip-content-package` Skill. The Modified seed contains exactly `ai-ip-assets/skills/deliver-ai-ip-content-package/SKILL.md` at `skills/deliver-ai-ip-content-package/SKILL.md`; its bytes and SHA-256 must match the repository asset. Neither seed may contain golden-gift wording, rubric, outcome/reference packets, scores, or case-specific hidden instructions.

- [ ] **Step 4: Run GREEN and commit**

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_plan.py
bazel build --offline //ai-ip-evals/lab:lab-assets
git add scripts/ai_ip/eval_lab/batch_plan.py scripts/ai_ip/eval_lab/test_batch_plan.py ai-ip-evals/lab
git commit -m "feat: seal candidate run plans"
```

### Task 3: Create truly isolated candidate cells

**Files:**
- Create: `scripts/ai_ip/eval_lab/batch_isolation.py`
- Create: `scripts/ai_ip/eval_lab/test_batch_isolation.py`

**Interfaces:**
- Consumes: Task 2.
- Produces: `AttemptCell`, `create_attempt_cell`, `verify_attempt_cells_disjoint`, `mark_receipts_sealed`, `cleanup_attempt_cell`.

- [ ] **Step 1: Write isolation attacks**

```python
def test_cells_share_no_state(tmp_path, seeds, profile):
    left = create_attempt_cell(tmp_path, "pair-1", "opaque-1", seeds.stock, seeds.workspace, profile, os.environ)
    right = create_attempt_cell(tmp_path, "pair-1", "opaque-2", seeds.modified, seeds.workspace, profile, os.environ)
    verify_attempt_cells_disjoint(left, right)
    (left.workspace / "sentinel").write_text("left")
    assert not (right.workspace / "sentinel").exists()
    assert left.environment["HOME"] == str(left.home)
    assert left.environment["CODEX_HOME"] == str(left.home)
    assert "VOLCENGINE_API_KEY" not in left.environment

def test_cleanup_before_receipt_seal_is_rejected(cell):
    with pytest.raises(IsolationError, match="receipts are not sealed"):
        cleanup_attempt_cell(cell)
```

Also test overlapping roots, symlink/hardlink seeds, roots inside repo/worktrees, reused IDs, shared cache/temp/log paths and leaked key/proxy variables.

- [ ] **Step 2: Run RED**

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_isolation.py
```

- [ ] **Step 3: Implement the cell and allowlist**

```python
@dataclass(frozen=True)
class AttemptCell:
    root: Path
    home: Path
    workspace: Path
    cache: Path
    temp: Path
    logs: Path
    promptfoo: Path
    environment: dict[str, str]
    receipt_marker: Path
```

Allow only PATH, SHELL, locale, isolated HOME/CODEX_HOME/TMPDIR, isolated Promptfoo paths, `PROMPTFOO_CACHE_ENABLED=false`, `FORCE_COLOR=0` and loopback `NO_PROXY`, plus profile-declared non-secret names. Copy separate bytes with directories 0700/files 0600 and no links.

- [ ] **Step 4: Run GREEN and commit**

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_isolation.py
git add scripts/ai_ip/eval_lab/batch_isolation.py scripts/ai_ip/eval_lab/test_batch_isolation.py
git commit -m "feat: isolate Codex candidate cells"
```

### Task 4: Implement paired state and receipts

**Files:**
- Create: `scripts/ai_ip/eval_lab/batch_receipts.py`
- Create: `scripts/ai_ip/eval_lab/batch_controller.py`
- Create: `scripts/ai_ip/eval_lab/test_batch_controller.py`

**Interfaces:**
- Consumes: Tasks 1–3.
- Produces: `CandidateExecutor`, `AttemptRequest`, `RawAttemptResult`, `run_candidate_pair`, `run_candidate_batch`, `verify_arm_attempt_receipt`, `verify_paired_run_receipt`.

- [ ] **Step 1: Freeze executor and failure semantics**

```python
class CandidateExecutor(Protocol):
    def execute(self, request: AttemptRequest) -> RawAttemptResult:
        pass

@dataclass(frozen=True)
class RawAttemptResult:
    exit_code: int
    started_at: str
    finished_at: str
    output: object | None
    metadata: object | None
    stdout: bytes
    stderr: bytes

def test_one_arm_failure_invalidates_pair(world):
    executor = ScriptedExecutor(stock=valid_result(), modified=timeout_result())
    receipt = run_candidate_pair(world.plan, world.bindings, executor, world.private_root, seed=b"x" * 32)
    assert receipt["pairValidity"] == "invalid"
    assert receipt["invalidReason"] == "executionFailure"
    assert executor.calls == 2

def test_identical_outputs_are_a_valid_tie(world):
    executor = ScriptedExecutor(stock=valid_result("same"), modified=valid_result("same"))
    receipt = run_candidate_pair(world.plan, world.bindings, executor, world.private_root, seed=b"x" * 32)
    assert receipt["pairValidity"] == "valid"
    assert receipt["outputRelation"] == "canonicallyIdentical"

def test_batch_executes_exactly_the_sealed_replication_count(world):
    plan = world.sealed_plan(replication_count=3)
    receipts = run_candidate_batch(plan, world.bindings, world.executor, world.private_root, seed=b"x" * 32)
    assert len(receipts) == 3
    assert len({receipt["pairId"] for receipt in receipts}) == 3
    assert world.executor.calls == 6
```

Test both orders, wrong case, invalid CaseAnswer, missing trajectory, timeout, nonzero exit, post-receipt mutation, duplicate attempt and existing pair. Verify `replicationCount=3` cannot stop after an early result or append a fourth pair.

- [ ] **Step 2: Run RED**

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_controller.py
```

- [ ] **Step 3: Implement fail-closed pairing**

Use 32 random bytes for order and opaque IDs. Retain both attempts unless global preflight prevents any call. Validate CaseAnswer and case ID; seal stdout/stderr/metadata privately; hash workspace before/after; seal each arm, reverify both, then seal the pair. Compare canonical CaseAnswer bytes and hashes: equality produces a valid `canonicallyIdentical` pair, never a mechanical failure. `run_candidate_batch` executes exactly the pre-sealed `replicationCount`, deriving independent pair/attempt IDs and order randomness without adaptive stopping. Accept only `{"maxAttemptsPerArm":1}`.

- [ ] **Step 4: Run GREEN and commit**

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_controller.py
git add scripts/ai_ip/eval_lab/batch_receipts.py scripts/ai_ip/eval_lab/batch_controller.py scripts/ai_ip/eval_lab/test_batch_controller.py
git commit -m "feat: seal paired candidate attempts"
```

### Task 5: Add the exact-pinned Promptfoo adapter

**Files:**
- Create: `ai-ip-evals/lab/promptfoo/package.json`
- Create: `ai-ip-evals/lab/promptfoo/README.md`
- Modify: `pnpm-workspace.yaml`
- Modify mechanically: `pnpm-lock.yaml`
- Create: `scripts/ai_ip/eval_lab/batch_promptfoo.py`
- Create: `scripts/ai_ip/eval_lab/test_batch_promptfoo.py`
- Create: `ai-ip-evals/lab/fixtures/batch-runner/promptfoo-result.json`

**Interfaces:**
- Consumes: `AttemptRequest`, `AttemptCell`.
- Produces: `render_promptfoo_config`, `parse_promptfoo_result`, `PromptfooExecutor.execute`.

- [ ] **Step 1: Freeze the dependency**

```json
{
  "name": "@openai/ai-ip-eval-lab-promptfoo",
  "version": "0.0.0",
  "private": true,
  "engines": {"node": ">=22.22.0"},
  "dependencies": {"promptfoo": "0.122.0"}
}
```

Add the package to `pnpm-workspace.yaml`, then run:

```bash
npx --yes pnpm@10.34.5 install --lockfile-only
npx --yes pnpm@10.34.5 --dir ai-ip-evals/lab/promptfoo exec promptfoo --version
```

Expected: `0.122.0`. Add no release-age exclusion.

- [ ] **Step 2: Write config and parser tests**

```python
def test_config_forces_isolation(request):
    config = render_promptfoo_config(request)
    provider = config["providers"][0]
    assert provider["id"] == "openai:codex-app-server"
    assert provider["config"]["reuse_server"] is False
    assert provider["config"]["inherit_process_env"] is False
    assert provider["config"]["include_raw_events"] is True
    assert provider["config"]["codex_path_override"] == str(request.binary_path)
    assert provider["config"]["model_provider"] == "openai"
    assert "assert" not in config

def test_parser_rejects_multiple_rows(frozen_result):
    frozen_result["results"]["results"].append(frozen_result["results"]["results"][0])
    with pytest.raises(PromptfooAdapterError, match="exactly one result row"):
        parse_promptfoo_result(frozen_result)
```

- [ ] **Step 3: Run RED**

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_promptfoo.py
```

- [ ] **Step 4: Implement static config and exact parsing**

Render one prompt/provider/test. Provider config sets `codex_path_override`, working directory, model, `model_provider="openai"` for the Ark/OpenAI-compatible Responses route, reasoning, output schema, sandbox, approval, loopback base URL, `ephemeral=true`, `reuse_server=false`, `inherit_process_env=false`, `include_raw_events=true`, cell `cli_env`, and bounded timeouts. Include no assertions, transforms, graders, JavaScript, rubric or reference data.

Invoke exactly:

```text
promptfoo eval --config cell/promptfoo/config.json --output cell/promptfoo/result.json --no-cache --no-progress-bar --no-table --no-share --no-write --max-concurrency 1
```

Parse one `results.results[0].response.output` and its `metadata.codexAppServer`. Reject missing thread/turn/usage, provider error, extra row, non-string output, oversized result or path escape.

- [ ] **Step 5: Run GREEN and commit**

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_promptfoo.py
npx --yes pnpm@10.34.5 --dir ai-ip-evals/lab/promptfoo exec promptfoo --version
git add ai-ip-evals/lab/promptfoo ai-ip-evals/lab/fixtures/batch-runner/promptfoo-result.json \
  pnpm-workspace.yaml pnpm-lock.yaml scripts/ai_ip/eval_lab/batch_promptfoo.py scripts/ai_ip/eval_lab/test_batch_promptfoo.py
git commit -m "feat: add locked Promptfoo App Server adapter"
```

### Task 6: Prove real App Server isolation locally

**Files:**
- Create: `scripts/ai_ip/eval_lab/batch_loopback.py`
- Create: `scripts/ai_ip/eval_lab/test_batch_promptfoo_e2e.py`

**Interfaces:**
- Consumes: Tasks 3–5 and a built Codex CLI.
- Produces: `LoopbackResponsesServer` with `.base_url`, `.enqueue_json_answer`, `.captured_requests`.

- [ ] **Step 1: Write the real-process test**

```python
def test_two_fresh_app_servers_finish_in_disjoint_cells(codex_binary, world):
    with LoopbackResponsesServer() as upstream:
        upstream.enqueue_json_answer(world.stock_answer)
        upstream.enqueue_json_answer(world.modified_answer)
        receipt = run_candidate_pair(
            world.plan_for(base_url=upstream.base_url),
            world.two_bindings(codex_binary, codex_binary),
            PromptfooExecutor(world.promptfoo_binary),
            world.private_root,
            seed=b"e" * 32,
        )
    assert receipt["pairValidity"] == "valid"
    assert len(upstream.captured_requests) == 2
    assert world.stock_cell.home != world.modified_cell.home
    assert world.stock_thread_id != world.modified_thread_id
```

The fixture emits `response.created`, one assistant `response.output_item.done`, and `response.completed` with integer usage; it accepts only dummy bearer and `/v1/responses`.

- [ ] **Step 2: Build and observe RED**

```bash
cargo build --locked --manifest-path codex-rs/Cargo.toml -p codex-cli -p codex-app-server
AI_IP_07B_CODEX_PATH="$PWD/codex-rs/target/debug/codex" \
  uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_promptfoo_e2e.py
```

- [ ] **Step 3: Implement the loopback fixture**

Use `ThreadingHTTPServer` on `127.0.0.1:0`, 1 MiB body maximum, canonical request hashes, five-second bounded shutdown and deterministic timeout/truncated-SSE/missing-usage/invalid-output failure modes.

- [ ] **Step 4: Run GREEN and commit**

```bash
AI_IP_07B_CODEX_PATH="$PWD/codex-rs/target/debug/codex" \
  uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_promptfoo_e2e.py
git add scripts/ai_ip/eval_lab/batch_loopback.py scripts/ai_ip/eval_lab/test_batch_promptfoo_e2e.py
git commit -m "test: run isolated App Servers without a provider"
```

### Task 7: Import a sealed pair into 07A

**Files:**
- Create: `scripts/ai_ip/eval_lab/batch_import.py`
- Create: `scripts/ai_ip/eval_lab/test_batch_import.py`
- Modify: `scripts/ai_ip/eval_lab/blind_controller.py`

**Interfaces:**
- Consumes: valid PairedRunReceipt, private outputs, existing `prepare_blind_batch`.
- Produces: `import_candidate_pair`, `prepare_blind_batch_from_import`.

- [ ] **Step 1: Write once-only and opacity tests**

```python
def test_import_is_once_only_and_blind_pack_hides_arms(world):
    imported = import_candidate_pair(
        private_root=world.private_root,
        paired_receipt_path=world.pair_receipt,
        stock_output_path=world.stock_output,
        modified_output_path=world.modified_output,
        imported_at="2026-08-31T12:00:00Z",
    )
    with pytest.raises(BatchImportError, match="already exists"):
        import_candidate_pair(
            private_root=world.private_root,
            paired_receipt_path=world.pair_receipt,
            stock_output_path=world.stock_output,
            modified_output_path=world.modified_output,
            imported_at="2026-08-31T12:00:01Z",
        )
    pack = prepare_blind_batch_from_import(world.private_root, imported, world.review_inputs)
    reviewer_bytes = world.read_reviewer_tree(pack)
    assert b"stock" not in reviewer_bytes
    assert b"modified" not in reviewer_bytes

def test_identical_pair_is_recorded_without_a_fake_blind_pack(world):
    imported = import_candidate_pair(
        private_root=world.private_root,
        paired_receipt_path=world.identical_pair_receipt,
        stock_output_path=world.identical_stock_output,
        modified_output_path=world.identical_modified_output,
        imported_at="2026-08-31T12:00:00Z",
    )
    assert imported["blindDisposition"] == "identicalTieNoReview"
    with pytest.raises(BatchImportError, match="no blind review required"):
        prepare_blind_batch_from_import(world.private_root, imported, world.review_inputs)
```

Also reject invalid pair, changed source, mismatched case, import predating pair completion, target collision and interrupted staging.

- [ ] **Step 2: Run RED**

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_import.py
```

- [ ] **Step 3: Implement atomic private import**

Stage under `candidate-runs/.stage-{secrets.token_hex(16)}`, write private Stock/Modified outputs, pair receipt and import receipt, then atomically rename to `candidate-runs/{pair_id}`. Never alter PairedRunReceipt. For `distinct`, set `readyForBlindReview` and delegate anonymous A/B generation to existing 07A. For `canonicallyIdentical`, set `identicalTieNoReview`, retain the private evidence, and refuse to create a meaningless A/B pack.

- [ ] **Step 4: Run GREEN and commit**

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_import.py scripts/ai_ip/eval_lab/test_blind_controller.py
git add scripts/ai_ip/eval_lab/batch_import.py scripts/ai_ip/eval_lab/test_batch_import.py scripts/ai_ip/eval_lab/blind_controller.py
git commit -m "feat: import candidate pairs into blind review"
```

### Task 8: Add CLI and provider-free Stock/Modified acceptance

**Files:**
- Create: `scripts/ai_ip/eval_lab/batch_cli.py`
- Create: `scripts/ai_ip/eval_lab/test_batch_cli.py`
- Modify: `ai-ip-evals/lab/BUILD.bazel`
- Modify: `docs/superpowers/specs/2026-08-31-codex-isolated-batch-runner-design.md`

**Interfaces:**
- Consumes: Tasks 1–7.
- Produces: `plan`, `seal`, `preflight`, `run`, `verify`, `import`, `status`; no `run-live` yet.

- [ ] **Step 1: Write CLI non-echo/state tests and run RED**

Every command rejects abbreviated/unknown flags with exactly `invalid arguments\n`, never echoes values and prints one canonical JSON status on success.

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_cli.py
```

- [ ] **Step 2: Implement the seven provider-free commands**

`run` refuses non-loopback base URLs; `status` exposes only mechanical state/opaque IDs; `verify` recomputes all commitments; `import` accepts only a valid pair.

- [ ] **Step 3: Build exact binaries**

Use `superpowers:using-git-worktrees` to create a detached temporary Stock worktree at `4ef1d4b89`. Run:

```bash
stock_build_root="$(mktemp -d /tmp/ai-ip-07b-stock.XXXXXX)"
stock_worktree_path="$stock_build_root/source"
git worktree add --detach "$stock_worktree_path" 4ef1d4b89
CARGO_TARGET_DIR="$stock_build_root/target" \
  cargo build --locked --manifest-path "$stock_worktree_path/codex-rs/Cargo.toml" -p codex-cli -p codex-app-server
cargo build --locked --manifest-path codex-rs/Cargo.toml -p codex-cli -p codex-app-server
```

Register `$stock_build_root/target/debug/codex` and `$PWD/codex-rs/target/debug/codex` with their build receipts; do not commit binaries. Retain the temporary worktree through Task 11 so review can re-hash the Stock binary, then remove it through `superpowers:finishing-a-development-branch` only after the evidence handoff.

- [ ] **Step 4: Run a real provider-free pair and import**

Use golden-gift ContentPacket, CaseAnswer schema, local Responses fixture, one replication, offline profile, identical model/budgets and both registered binaries. Verify pair and import into a temporary 07A PrivateRoot.

- [ ] **Step 5: Run full provider-free regression**

```bash
AI_IP_V6_EVIDENCE_ROOT=/Users/yangyucheng/Documents/ChatGPT/第六版营销系统/docs/content-intelligence-v6/evidence \
  uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab
bazel build --offline //ai-ip-evals/lab:lab-assets
git diff --check
```

Expected: zero skip on approved host and no provider request.

- [ ] **Step 6: Record mechanical completion and commit**

Set status `IMPLEMENTATION_COMPLETE_LIVE_PENDING_AUTHORIZATION` with exact commands/counts, binary hashes and pair receipt hash.

```bash
git add scripts/ai_ip/eval_lab/batch_cli.py scripts/ai_ip/eval_lab/test_batch_cli.py ai-ip-evals/lab/BUILD.bazel \
  docs/superpowers/specs/2026-08-31-codex-isolated-batch-runner-design.md
git commit -m "feat: complete provider-free Codex batch runner"
```

### Task 9: Add a proof-only bounded credential proxy

**Files:**
- Create: `codex-rs/ai-ip-eval/src/bounded_proxy.rs`
- Create: `codex-rs/ai-ip-eval/src/bounded_proxy_tests.rs`
- Create: `codex-rs/ai-ip-eval/src/bin/ai_ip_bounded_responses_proxy/main.rs`
- Modify: `codex-rs/ai-ip-eval/src/lib.rs`
- Modify: `codex-rs/ai-ip-eval/Cargo.toml`
- Modify mechanically: `codex-rs/ai-ip-eval/BUILD.bazel`

**Interfaces:**
- Consumes: public `codex_responses_api_proxy` bind/activate, stdin auth, gate, observer and transform APIs.
- Produces: `ai-ip-bounded-responses-proxy` and canonical body-free `BoundedProxyReceipt`.

- [ ] **Step 1: Write bounded proxy tests first**

Cover: stdin credential reaches upstream but never dump/receipt/stderr; only `POST /v1/responses`; body cap; injected integer `max_output_tokens`; attempt cap; request deadline; redirect rejection; missing/duplicate completion; invalid usage; observed-token cap; retained failures; receipt only after bounded shutdown; concurrent reservations cannot exceed caps.

```rust
pub struct BoundedProxyConfig {
    pub max_request_attempts: u64,
    pub max_request_body_bytes: usize,
    pub max_output_tokens_per_request: u64,
    pub max_observed_total_tokens: u64,
    pub request_timeout: Duration,
    pub receipt_path: PathBuf,
}
```

- [ ] **Step 2: Run focused RED**

```bash
just test -p codex-ai-ip-eval
```

- [ ] **Step 3: Implement the proof-only gate**

Allocate monotonic attempt IDs before I/O; reserve request slots atomically; observe exactly one `response.completed` per successful exchange; aggregate integer usage; poison on any budget/contract violation. Receipt contains only digests, counts, usage, timestamps, model/deployment and classification—never bodies or Authorization.

The binary accepts exactly `--upstream-url`, `--server-info`, `--receipt`, `--max-request-attempts`, `--max-request-body-bytes`, `--max-output-tokens-per-request`, `--max-observed-total-tokens`, `--request-timeout-seconds`; auth is stdin only.

- [ ] **Step 4: Run Rust and Bazel tests**

```bash
just test -p codex-ai-ip-eval
just test -p codex-responses-api-proxy
bazel test //codex-rs/ai-ip-eval:ai-ip-eval-unit-tests //codex-rs/responses-api-proxy:responses-api-proxy-unit-tests
```

- [ ] **Step 5: Fix/format, do not retest, commit**

```bash
just fix -p codex-ai-ip-eval
just fmt
git diff --check
git add codex-rs/ai-ip-eval
git commit -m "feat(ai-ip-eval): add bounded live proxy"
```

### Task 10: Consume one-shot authorization and run GLM

**Files:**
- Create: `scripts/ai_ip/eval_lab/batch_live.py`
- Create: `scripts/ai_ip/eval_lab/test_batch_live.py`
- Modify: `scripts/ai_ip/eval_lab/batch_cli.py`
- Modify: `scripts/ai_ip/eval_lab/test_batch_cli.py`
- Modify: `docs/architecture/codex-fork-patch-ledger.md`
- Modify: `docs/superpowers/specs/2026-08-31-codex-isolated-batch-runner-design.md`

**Interfaces:**
- Consumes: sealed plan, one-shot authorization, owner-only credential outside repos/worktrees, Task 9 proxy.
- Produces: `consume_live_authorization`, `BoundedProxyProcess`, `run_live_pair`, CLI `run-live`, first real anonymous GLM pair.

- [ ] **Step 1: Write one-shot and secret tests**

Test expired/wrong plan, model or route; reused nonce; excessive request/token/time/CNY; credential inside repo, wrong owner/mode, symlink; secret in child env/argv/stdout/stderr/config/receipt; proxy crash; one-arm failure; authorization reuse after any attempted call.

```python
def test_authorization_consumed_before_provider_attempt(world):
    auth = world.issue_authorization(max_requests=2)
    world.proxy.fail_start = True
    with pytest.raises(LiveRunError):
        run_live_pair(world.plan, auth, world.credential_file, world.proxy)
    assert world.authorization_status(auth) == "consumed"
```

- [ ] **Step 2: Run RED**

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_live.py scripts/ai_ip/eval_lab/test_batch_cli.py -k live
```

- [ ] **Step 3: Implement live lifecycle without a live call**

`run-live` accepts paths to plan, authorization, credential file, bounded proxy, Stock/Modified binaries and PrivateRoot. Validate, atomically consume authorization, start one proxy per arm, write key bytes to proxy stdin and close it. Candidate receives only loopback `/v1` and dummy bearer. Kill candidates/proxies at deadline and retain failures.

Freeze the smoke limits: one attempt and at most one provider request per arm, 4096 output tokens per request, 1 MiB request body, five-minute pair wall time, no retry, offline tools, CNY ceiling from a frozen current Ark rate card and conservative maxima.

- [ ] **Step 4: Run all provider-free live-boundary tests**

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_live.py scripts/ai_ip/eval_lab/test_batch_cli.py
```

Expected: pass without a real key or non-loopback connection.

- [ ] **Step 5: STOP for exact paid-run authorization**

Show the user: `glm-5-2-260617`, Ark Responses route, two maximum requests, 4096 output tokens/request, 1 MiB body cap, five-minute deadline, frozen rate-card source/time, computed CNY ceiling, credential path class and plan hash. Do not continue without explicit approval of this exact run.

- [ ] **Step 6: After approval, run one real pair**

Read `VOLCENGINE_API_KEY` from the owner-only untracked fifth/sixth-version credential location without displaying/copying it. Run `batch_cli.py run-live`; verify both proxy receipts, pair receipt, consumed authorization and anonymous 07A import. Do not score or announce a winner.

- [ ] **Step 7: Run final regression before formatting**

```bash
AI_IP_V6_EVIDENCE_ROOT=/Users/yangyucheng/Documents/ChatGPT/第六版营销系统/docs/content-intelligence-v6/evidence \
  uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab
just test -p codex-ai-ip-eval
just test -p codex-responses-api-proxy
bazel build --offline //ai-ip-evals/lab:lab-assets
bazel test //codex-rs/ai-ip-eval:ai-ip-eval-unit-tests //codex-rs/responses-api-proxy:responses-api-proxy-unit-tests
```

- [ ] **Step 8: Format once and do not retest**

```bash
just fix -p codex-ai-ip-eval
just fmt
git diff --check
git status --short
```

Restore unrelated formatter churn with `apply_patch`; preserve user changes.

- [ ] **Step 9: Close ledger and commit**

Append `F-0009A` with exact test counts, hashes, authorization, labels, usage/cost and opacity evidence; state `G2=OPEN`, `PASS_TO_PHASE_0B=false`. Set design status `07B_COMPLETE`; claim no marketing superiority.

```bash
git add scripts/ai_ip/eval_lab/batch_live.py scripts/ai_ip/eval_lab/test_batch_live.py \
  scripts/ai_ip/eval_lab/batch_cli.py scripts/ai_ip/eval_lab/test_batch_cli.py \
  docs/architecture/codex-fork-patch-ledger.md \
  docs/superpowers/specs/2026-08-31-codex-isolated-batch-runner-design.md
git commit -m "feat: complete isolated Codex batch runner"
```

### Task 11: Independent final review

**Files:**
- Review only: all changes from F-0009 authority through Task 10.

**Interfaces:**
- Consumes: clean branch and verification records.
- Produces: independent spec and code/security verdicts with Critical/Important/Minor counts.

- [ ] **Step 1: Dispatch specification review**

Check design coverage, process isolation, treatment sealing, invalid-pair semantics, Promptfoo non-authority, 07A opacity, one-shot authorization and absence of false G2/business claims.

- [ ] **Step 2: Dispatch code/security review**

Check path races, links, environment leakage, process cleanup, secret lifetime, output redaction, budget concurrency, result parsing, commitments, size caps and line caps.

- [ ] **Step 3: Fix Critical/Important findings with focused TDD**

Each finding gets a failing regression, minimal fix, focused test and separate commit. Minor findings require written rationale if retained.

- [ ] **Step 4: Record final verdict**

Handoff requires both reviewers at Critical 0 / Important 0, a clean worktree and ledger truth matching actual provider/live state.
