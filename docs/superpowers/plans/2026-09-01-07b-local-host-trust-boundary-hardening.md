> **RETIRED — RETIRED_NOT_PASSED (2026-09-05).** This evaluator/proof document is historical and is not executable. All commands, prerequisite gates and successor authorizations below are retired by the approved business-first cleanup; prior results and failures remain historical claims only. Do not resume Phase 0A, 06A, 06B, 07A, 07B or LH1 from this record. Retirement grants no business PASS, model qualification, spending, publication, customer-data access or customer-release authority. Current authority: `docs/superpowers/specs/2026-09-05-business-first-cleanup-design.md` and `docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md`.

# 07B Local-Host Trust Boundary Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the remaining candidate-controlled runtime and evidence defects without turning the local 07B marketing evaluator into an operating-system or malware-isolation product.

**Architecture:** The operating system and the owner-managed login account are trusted host prerequisites. The candidate Agent, its descendants, its workspace, Promptfoo output, network activity, and evaluation evidence remain untrusted. Promptfoo is reconstructed under a runner-owned private temporary root that is not passed to the candidate, verified from the bytes actually written, made read-only, measured immediately before execution and again after execution; any drift invalidates the attempt.

**Tech Stack:** Python 3.11, Node.js 24.19.0, Promptfoo 0.122.0, pnpm 10.34.5, pytest 9.0.2, jsonschema 4.25.1, SHA-256, existing Task 4 `LaunchSpec` and process supervision.

**Spec:** `docs/superpowers/specs/2026-08-31-codex-isolated-batch-runner-design.md`

## Global Constraints

- Defend against candidate Agents, their tools and descendants, untrusted workspaces/results, unauthorized network access, secret exposure, cross-arm contamination, and evidence replacement.
- Treat the kernel, filesystem, and owner-managed current login account as trusted host prerequisites; an unrelated malicious process with equivalent same-user OS authority is out of scope.
- A detected runtime-tree mutation is a host-integrity failure: reject the attempt and rerun on a clean host.
- Promptfoo remains exactly `0.122.0`, Node remains exactly the committed `v24.19.0` and at least `22.22.0`, and pnpm remains exactly `10.34.5`.
- Keep Promptfoo assertions, scores, success fields, real provider credentials, arm labels, rubric, reference and release authority outside normalized evidence.
- Task 4 remains the sole owner of host process launch, process group, descriptors, deadlines, termination, stream budgets, metering and launch attestation.
- Keep every production module below 500 lines and every non-mechanical commit below 800 changed lines.
- Do not run Task 4/controller process tests while the twelve recorded macOS `UEs` processes remain; do not signal or delete them or their authenticated records.
- Run tests before the final `just fmt`; do not run tests after the formatter.

---

## File Structure

- `scripts/ai_ip/eval_lab/batch_promptfoo_runner.js`: sealed runtime reconstruction, exact writes, child-only environment, private runtime root, pre/post runtime verification and result/telemetry framing.
- `scripts/ai_ip/eval_lab/batch_promptfoo_result.js`: strict result parsing and shared UTF-8 canonical evidence encoding; also hosts small shared helpers moved out of the nearly-500-line runner when necessary.
- `scripts/ai_ip/eval_lab/batch_promptfoo_archive.py`: descriptor ownership and ignored-link policy for seal creation.
- `scripts/ai_ip/eval_lab/batch_promptfoo_runner.py`: Python oracle with the same output/evidence equality and canonicalization rules as production JavaScript.
- `scripts/ai_ip/eval_lab/batch_promptfoo.py`: static candidate environment templates, including a candidate-only temporary directory distinct from the runner runtime root.
- `scripts/ai_ip/eval_lab/test_batch_promptfoo.py`: Python/JavaScript result, environment, cwd and retained-evidence attacks.
- `scripts/ai_ip/eval_lab/test_batch_promptfoo_archive_hardening.py`: archive descriptor lifecycle, no-follow, exact-write and reconstructed-runtime mutation attacks.
- `ai-ip-evals/lab/promptfoo/README.md`: documented local-host trust boundary and runtime verification behavior.
- `.superpowers/sdd/2026-08-31-07b-codex-isolated-batch-runner/progress.md`: ignored decision/test ledger; append-only R15 and exact verification evidence.

---

### Task 1: Make every extracted and acquired byte exact

**Files:**
- Modify: `scripts/ai_ip/eval_lab/batch_promptfoo_runner.js`
- Modify: `scripts/ai_ip/eval_lab/batch_promptfoo_archive.py`
- Modify: `scripts/ai_ip/eval_lab/test_batch_promptfoo_archive_hardening.py`

**Interfaces:**
- Consumes: current `ArchiveReader`, Python `BoundRoot`/bounded descriptor readers, committed runtime manifest and 29 archive chunks.
- Produces: `writeAllAt(fd, buffer, offset, writer)`, exact extracted-file verification, immediate descriptor ownership, close-error propagation and target-free `.bin` skipping.

- [ ] **Step 1: Add mutation-sensitive extractor and descriptor lifecycle tests**

Add focused cases that inject a partial regular-file write, `EINTR`, zero progress, root/child `fstat` failure, regular-file final-verification failure, `close` failure, and `.bin` links whose targets are dangling, external, FIFO, socket and a trap that fails if target-following stat occurs.

```python
def test_ignored_bin_link_is_never_followed(monkeypatch, runtime_tree):
    link = runtime_tree / "node_modules/.bin/promptfoo"
    link.symlink_to("/outside/target")
    monkeypatch.setattr(os, "stat", lambda *args, **kwargs: (_ for _ in ()).throw(AssertionError("target followed")))
    assert archive.runtime_entries(runtime_tree) == expected_without_bin_link
```

- [ ] **Step 2: Run the new tests and preserve the RED transcript**

Run:

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_promptfoo_archive_hardening.py
```

Expected: the new cases fail because archive-file `writeSync` can short-write, ownership begins after `fstat`, close uncertainty is swallowed, and ignored `.bin` links follow their targets.

- [ ] **Step 3: Implement exact extraction writes and verify the actual descriptor**

Use a loop that advances by the returned count, retries `EINTR`, rejects zero progress, and hashes only bytes confirmed written. Before closing each output descriptor, require exact size, regular-file type, single link, expected mode and a second digest read from that same descriptor.

```javascript
function writeAllAt(fd, payload, writer = fs.writeSync) {
  let offset = 0;
  while (offset < payload.length) {
    let written;
    try {
      written = writer(fd, payload, offset, payload.length - offset, offset);
    } catch (error) {
      if (error?.code === "EINTR") continue;
      throw error;
    }
    if (!Number.isSafeInteger(written) || written < 1) {
      throw new RunnerError("runtime file write made no progress");
    }
    offset += written;
  }
}
```

- [ ] **Step 4: Register every descriptor immediately and surface close uncertainty**

Append each successful `open` result to the ownership stack before `fstat` or other fallible validation. Release through one `finally` path; collect the first close error and raise `PromptfooArchiveError` after attempting all remaining closes. Decide ignored `.bin` status from the directory entry/lstat only and never call target-following `stat`.

- [ ] **Step 5: Run the focused archive tests and commit**

Expected: all archive-hardening tests pass and no Task 4 test starts.

```bash
git add scripts/ai_ip/eval_lab/batch_promptfoo_runner.js \
  scripts/ai_ip/eval_lab/batch_promptfoo_archive.py \
  scripts/ai_ip/eval_lab/test_batch_promptfoo_archive_hardening.py
git commit -m "fix: close Promptfoo runtime byte gaps"
```

---

### Task 2: Bind the marketing answer to retained evidence

**Files:**
- Modify: `scripts/ai_ip/eval_lab/batch_promptfoo_result.js`
- Modify: `scripts/ai_ip/eval_lab/batch_promptfoo_runner.py`
- Modify: `scripts/ai_ip/eval_lab/test_batch_promptfoo.py`

**Interfaces:**
- Consumes: exact Promptfoo 0.122.0 invariant that `response.output`, `raw.output` and `raw.finalResponse` originate from the same provider result.
- Produces: identical Python/JavaScript acceptance, exact answer equality and one UTF-8-byte-sorted canonical trajectory digest.

- [ ] **Step 1: Add pairwise output-mismatch and canonicalization parity tests**

Mutate each of the three output strings independently and require both parsers to reject it. Add one shared corpus with BMP/non-BMP keys, escaped equivalents, unpaired surrogates, `-0`, `1.0`, exponent values, duplicate keys, `NaN`, `Infinity`, and integers outside `[-9007199254740991, 9007199254740991]`.

```python
@pytest.mark.parametrize("field", ["response", "rawOutput", "rawFinalResponse"])
def test_answer_must_equal_retained_final_response(field, promptfoo_payload):
    mutated = mutate_one_answer_field(promptfoo_payload, field)
    with pytest.raises(PromptfooAdapterError, match="final response differs"):
        parse_promptfoo_result(mutated)
```

- [ ] **Step 2: Run the parity tests and preserve the RED transcript**

Run the named new tests from `test_batch_promptfoo.py`. Expected: output mismatches are accepted and non-BMP object-key ordering produces different trajectory digests.

- [ ] **Step 3: Enforce equality and one canonical byte contract**

Require all three output strings to be equal before decoding the CaseAnswer. Canonical object keys are ordered by their strict UTF-8 byte sequences in both languages; unpaired surrogates and non-integral JSON numbers are rejected in retained evidence. Scores, success and assertions remain excluded.

```python
def _utf8_key(value: str) -> bytes:
    return value.encode("utf-8", "strict")

def _canonical_mapping(value: dict[str, object]) -> bytes:
    return b"{" + b",".join(
        _canonical_string(key) + b":" + _canonical(value[key])
        for key in sorted(value, key=_utf8_key)
    ) + b"}"
```

- [ ] **Step 4: Run Python and sealed-JavaScript focused tests and commit**

Run:

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_promptfoo.py
```

Expected: both paths reject every mismatch and emit identical retained-evidence SHA-256 values for every accepted corpus item.

```bash
git add scripts/ai_ip/eval_lab/batch_promptfoo_result.js \
  scripts/ai_ip/eval_lab/batch_promptfoo_runner.py \
  scripts/ai_ip/eval_lab/test_batch_promptfoo.py
git commit -m "fix: bind Promptfoo answers to evidence"
```

---

### Task 3: Separate candidate scratch space and attest the reconstructed tree

**Files:**
- Modify: `scripts/ai_ip/eval_lab/batch_promptfoo.py`
- Modify: `scripts/ai_ip/eval_lab/batch_promptfoo_runner.js`
- Modify: `scripts/ai_ip/eval_lab/batch_promptfoo_result.js`
- Modify: `scripts/ai_ip/eval_lab/test_batch_promptfoo.py`
- Modify: `scripts/ai_ip/eval_lab/test_batch_promptfoo_archive_hardening.py`

**Interfaces:**
- Consumes: Task 4 starts the sealed runner with `cwd=AttemptCell.workspace`, an outer isolated `TMPDIR`, immutable artifacts and the fixed Promptfoo environment.
- Produces: candidate-only `AI_IP_CANDIDATE_TMP`, runner-private extraction root, in-memory extracted-tree inventory, read-only reconstructed runtime and pre/post execution tree identity checks.

- [ ] **Step 1: Add environment-separation and runtime-drift RED tests**

Assert the candidate receives a workspace-local temporary directory, never the outer runner `TMPDIR`. Use a fake Promptfoo entrypoint that modifies one extracted dependency after startup; the runner must reject the result even if the fake entrypoint exits zero and writes valid result/telemetry envelopes.

- [ ] **Step 2: Run only the new Task 5 tests and record the RED**

Expected: the candidate currently receives outer `TMPDIR`, and a post-extraction dependency mutation is not detected.

- [ ] **Step 3: Split scratch roots without changing shared config bytes across arms**

Render candidate `TMPDIR` as `{{ env.AI_IP_CANDIDATE_TMP }}`. The runner sets that value to a private `0700` directory inside `process.cwd()` while retaining the outer `TMPDIR` exclusively for Promptfoo reconstruction and state. Do not place the runtime path in `cli_env`, Promptfoo config, candidate argv or candidate-readable evidence.

- [ ] **Step 4: Retain and verify the extracted-tree inventory**

While extracting, retain `{path, sha256, size, mode}` for every regular file. After extraction, change files to `0400`/`0500` and directories to `0500`, verify exact path set, link count, size, mode and digest immediately before spawning Promptfoo, then repeat after Promptfoo exits and before emitting result/telemetry. Compare the extraction-root descriptor identity before every path scan; any drift raises `RunnerError("Promptfoo runtime changed after attestation")`.

This check defends the candidate boundary and detects host corruption. It does not claim to defeat an unrelated malicious process that already has the login user's equivalent OS authority; R15 classifies that as a compromised-host rerun condition.

- [ ] **Step 5: Keep both sealed JavaScript artifacts below 500 lines**

Move existing generic strict-file/canonical helpers between `batch_promptfoo_runner.js` and `batch_promptfoo_result.js` instead of creating a third source artifact. Preserve the exact material equation: `29 runtime chunks + 1 runtime manifest + 2 JavaScript artifacts = 32`.

- [ ] **Step 6: Run focused Task 5 tests and commit**

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_promptfoo.py \
  scripts/ai_ip/eval_lab/test_batch_promptfoo_archive_hardening.py
git add scripts/ai_ip/eval_lab/batch_promptfoo.py \
  scripts/ai_ip/eval_lab/batch_promptfoo_runner.js \
  scripts/ai_ip/eval_lab/batch_promptfoo_result.js \
  scripts/ai_ip/eval_lab/test_batch_promptfoo.py \
  scripts/ai_ip/eval_lab/test_batch_promptfoo_archive_hardening.py
git commit -m "fix: isolate and attest Promptfoo runtime"
```

Expected: all Task 5 tests pass, the fake mutation fails closed, both arm configs remain byte-identical and every production module remains below 500 lines.

---

### Task 4: Verify, review and close the ledger honestly

**Files:**
- Modify: `ai-ip-evals/lab/promptfoo/README.md`
- Modify ignored ledger: `.superpowers/sdd/2026-08-31-07b-codex-isolated-batch-runner/progress.md`
- Create ignored reviews: `task-5-final-spec-review-round-3.md`, `task-5-final-security-review-round-3.md`

**Interfaces:**
- Consumes: Tasks 1–3 commits, committed Darwin runtime manifest and the existing clean-room runtime root.
- Produces: exact focused test evidence, unchanged 29-chunk seal proof, independent Critical 0/Important 0 reviews and a clearly separated post-reboot Task 4 gate.

- [ ] **Step 1: Re-run the Task 5 focused suite**

Run the exact two-file command from Task 3. Record the full count and duration in the ignored ledger.

- [ ] **Step 2: Reproduce the committed Darwin seal**

Call `seal_committed_promptfoo_runtime` against `/tmp/07b-promptfoo-full-hoisted-clean.DE9xxS/deploy` and the portable Node path. Require exactly 29 chunks, 53,634 files, 1,632,192,483 unpacked bytes and exact equality of Node, package, lock, tree, archive and every chunk digest. This is pure local file verification and must not start a candidate or contact a provider.

- [ ] **Step 3: Obtain two fresh independent reviews**

One reviewer checks Task 5 specification closure; another attacks runtime reconstruction, candidate scratch separation, evidence binding, descriptor lifecycle and changed-line/module caps. Required gate: `Critical 0 / Important 0`; Minors must be explicitly accepted or fixed.

- [ ] **Step 4: Update documentation and the ignored ledger**

Document the trusted-host prerequisite, candidate threat boundary, private runtime, pre/post verification and compromised-host rerun rule. Preserve the historical SIGTERM exit 143 as non-evidence and the twelve `UEs` processes as an unresolved host verification condition.

- [ ] **Step 5: After the host is clean, run the Task 4 timing/compatibility gate**

Only after a normal reboot or equivalent clean-host state removes the twelve recorded `UEs` processes, run:

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_controller_review_round4.py \
  scripts/ai_ip/eval_lab/test_batch_controller_review_round5.py \
  scripts/ai_ip/eval_lab/test_batch_promptfoo.py \
  scripts/ai_ip/eval_lab/test_batch_promptfoo_archive_hardening.py
```

Then run the complete Task 1–5 Python compatibility selection recorded in the ledger. Do not relax the production deadline or delete authenticated orphan evidence to make the tests pass.

- [ ] **Step 6: Run the required final formatter and do not retest afterward**

Run the repository's existing `just fmt` command from its required working directory. Inspect `git diff --check`, module line counts, commit changed-line counts and tracked worktree status. Do not run tests after this formatter.

- [ ] **Step 7: Commit the final documentation closure**

```bash
git add ai-ip-evals/lab/promptfoo/README.md
git commit -m "docs: close Promptfoo host boundary"
```

Do not mark Task 5 complete or merge locally until the post-reboot Task 4 gate and both independent reviews are green.
