# 07B-LH1-F1 Bound-Cell Cleanup Proof Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the foreign-lease regression prove that a binding-authority failure never cleans a bound attempt cell.

**Architecture:** Keep the already-reviewed production implementation unchanged. Strengthen the existing foreign-lease integration test with one real bound sentinel cell, then use a temporary production mutation to prove the assertion kills an implementation that enters cleanup before restoring the exact production bytes.

**Tech Stack:** Python 3.11, pytest 9.0.2, jsonschema 4.25.1, uv, existing eval-lab lifecycle harness.

**Spec:** `docs/superpowers/specs/2026-09-01-07b-lh1-authenticated-orphan-authority-design.md`

## Global Constraints

- Final committed scope changes only `scripts/ai_ip/eval_lab/test_batch_controller_lifecycle_hardening.py`; production files must be byte-identical to base `67d8a982934e7a975a0feceb885966f6fedd4c5b`.
- The test must bind a sentinel through `PairLifecycle.bind_cell` before `abort`, retain the existing cleanup spy, and prove the sentinel is never passed to cleanup.
- Mutation RED must temporarily make the real abort path call `cleanup_attempt_cell` for bound cells before the binding-authority failure; the strengthened test must fail specifically because the sentinel was cleaned. Restore the production file using `apply_patch` before GREEN.
- Preserve the foreign-lease state assertions: expected lease is `ORPHANED`, foreign lease is `RESERVED`, and the public causal chain identifies binding authority.
- Full `4dbb448ea8957076e14476f2aca31fcaaf2e798f..HEAD` non-mechanical change must remain below 800 lines; production changed lines and every production module must remain below 500.
- Run all Python verification before the final successful `just fmt`; do not run tests afterward.
- No network, provider, Promptfoo, App Server, paid model, media, secrets, publication, push, PR, merge, or release operation is authorized during implementation and review.
- All hand edits use `apply_patch`; never use `git restore`, `checkout`, `reset`, `clean`, shell writes, or Python writes.

---

### Task 1: Bind a real cell and prove the cleanup spy is branch-distinguishing

**Files:**
- Modify: `scripts/ai_ip/eval_lab/test_batch_controller_lifecycle_hardening.py:273-288`
- Temporary mutation only, restored before commit: `scripts/ai_ip/eval_lab/batch_controller_lifecycle.py:172-190`

**Interfaces:**
- Consumes: `PairLifecycle.bind_cell(cell: object) -> None`, `PairLifecycle.abort(error: BaseException) -> None`, and the existing `cleanup_attempt_cell` spy seam.
- Produces: a foreign-lease regression whose bound sentinel makes accidental cleanup observable.

- [ ] **Step 1: Strengthen the existing test**

Create one sentinel with a valid `attempt_id`, bind it through `pair_lifecycle.bind_cell(sentinel)` before `abort`, and keep the final deep assertion that `cleaned == []` together with the expected/foreign lease states.

- [ ] **Step 2: Prove mutation RED**

Using `apply_patch`, temporarily insert an erroneous loop that invokes `cleanup_attempt_cell(cell)` for every bound cell before the binding-authority error is raised. Run:

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 pytest -q scripts/ai_ip/eval_lab/test_batch_controller_lifecycle_hardening.py -k foreign_lease
```

Expected: FAIL because `cleaned` contains the exact sentinel. A failure for syntax, fixture, or exception-message reasons does not count.

- [ ] **Step 3: Restore production and prove GREEN**

Restore the temporary production mutation with `apply_patch`, verify the production file is byte-identical to base, and rerun the same command.

Expected: `1 passed` with the remaining tests deselected.

- [ ] **Step 4: Run compatibility verification before formatting**

Run the complete LH1 test file, LH1 plus all five historical Task 4 review files, and the exact 18-file combined command recorded in the parent LH1 plan. Expected floors: LH1 `22 passed`; six-file `98 passed`; exact suite `272 passed, 10 skipped`, with only the existing native-Windows guards skipped.

- [ ] **Step 5: Format, restore formatter-only drift, and verify scope**

Run `just fmt` from `codex-rs` with the stable toolchain on `PATH`; do not run tests afterward. If the known 16 unrelated paths drift, restore only those formatter-created changes through explicit `apply_patch`. Verify production byte identity to base, full-range `<800`, production `<500`, every production module `<500`, `git diff --check`, and a final diff containing only the test file.

- [ ] **Step 6: Commit**

```bash
git add scripts/ai_ip/eval_lab/test_batch_controller_lifecycle_hardening.py
git commit -m "test(eval): prove foreign lease retains bound cells"
```
