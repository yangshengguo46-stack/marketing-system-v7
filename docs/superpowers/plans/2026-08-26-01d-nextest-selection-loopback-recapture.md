> **RETIRED — RETIRED_NOT_PASSED (2026-09-05).** This evaluator/proof document is historical and is not executable. All commands, prerequisite gates and successor authorizations below are retired by the approved business-first cleanup; prior results and failures remain historical claims only. Do not resume Phase 0A, 06A, 06B, 07A, 07B or LH1 from this record. Retirement grants no business PASS, model qualification, spending, publication, customer-data access or customer-release authority. Current authority: `docs/superpowers/specs/2026-09-05-business-first-cleanup-design.md` and `docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md`.

# Phase 0A.2 Task 7R — Nextest Selection and Loopback Recapture Repair

> **Execution:** Use `superpowers:systematic-debugging`, `superpowers:test-driven-development`, and `superpowers:subagent-driven-development`. This is a bounded repair of the frozen native-evidence child, not a business/runtime change.

**Goal:** Correct two reproduced recorder-environment defects, seal a new tools SHA, and recapture the unchanged macOS x86_64 baseline without editing or deleting the invalid evidence bytes.

**Base:** `d7911f421a33b2f63a81e49ffe04a5de190bb59a`

**Provider boundary:** `providerMode=not-run`; `paidProviderCost=0`; do not locate or read an API key.

**Audit authorities:** The ignored SDD root is `.superpowers/sdd/2026-08-26-01d-nextest-selection-loopback-recapture/`. Its authoritative files are `progress.md`, `task-0-report.md`, `task-0-review.md`, `task-1-report.md`, `task-1-review.md`, `task-2-report.md`, `task-2-review.md`, `task-3-report.md`, `task-3-review.md`, and `task-4-report.md`. Every report records command, exit, relevant SHA/path, and disposition before its task is marked complete.

## Reproduced facts

1. Nine exact-name nextest selections ran from the repository top level. Cargo therefore could not find `Cargo.toml`, because the pinned workspace root is `<repo>/codex-rs`. Each selection became `BLOCKED_SELECTION` before its real test ran.
2. Those Cargo errors printed the Task 0 Rust toolchain under `/Users/yangyucheng`, so the public verifier correctly returned exit `1` with `INVALID_FORBIDDEN_CONTENT`; no valid baseline summary was created.
3. With the same pinned tree, toolchain, target cache, and exact test name, running nextest selection from `<repo>/codex-rs` succeeds.
4. cargo-nextest `0.9.103` JSON keeps root `test-count` as the entire selected test-binary population even when `-E 'test(=...)'` is applied. Human output proves the filter is applied, while JSON `rust-suites.*.testcases` contains the exact test key once. The old assumption that root `test-count == 1` was false for the locked tool.
5. The host has an HTTP/HTTPS/SOCKS proxy at `127.0.0.1:7890`. The sanitized child environment did not set loopback proxy bypass. The full `codex-app-server-transport` run routed local mock-server requests through the proxy, producing HTTP 502/timeouts. Re-running one failed exact test with fixed `NO_PROXY=no_proxy=127.0.0.1,localhost,::1` passed in 0.49 seconds.

These facts identify recorder/capture-environment defects. They do not authorize changes to `codex-rs/**`, the command matrix, test names, expected exits, or provider/runtime behavior.

## Contract repair

### Selection working directory

- Main frozen commands still run from the tested Git top level exactly as recorded in the matrix.
- Only generated `cargo nextest list` preflights run from the canonical real directory `<tested-repo>/codex-rs`.
- The recorder must prove that directory is a non-symlink directory contained immediately beneath the tested root and that `codex-rs/Cargo.toml` is the tracked stage-0 regular blob already covered by tested-tree identity.

### Exact-name count on nextest 0.9.103 JSON

- Keep the generated argv as `cargo nextest list --message-format json` followed by the original `just test` arguments after the first two tokens.
- Require exactly one separate `-E` argument pair, reject `--filter-expr`, a second `-E`, a missing value, or any other selection expression before a child command runs. The sole value must full-match `test(=<name>)`, where `<name>` matches `[A-Za-z0-9_]+(?:::[A-Za-z0-9_]+)*`; compound, negated, wildcard, `all()`, suffix-bearing, and extra-closing syntax is forbidden.
- Strictly parse the nextest JSON root while rejecting duplicate keys at every depth. `rust-suites` must be an object; every suite must be an object containing a `testcases` object; and every visited testcase leaf must have the locked typed shape needed by this contract. Root `test-count` is population metadata and must be a non-boolean nonnegative integer, but it is not the exact-filter count.
- Count raw exact key equality with the extracted test name across every suite's `testcases` object. The count must be exactly one, and that sole leaf must have `ignored is false` and exact `filter-match == {"status":"matches"}`. The manifest's existing `selection.testCount` records this runnable exact-match count and must equal `1` before the real command runs.
- Ignored or mismatched exact leaves, malformed leaves, repeated suite/testcase JSON keys, zero matches, and the same exact name in multiple suites remain `BLOCKED_SELECTION` and must not run the real command.

### Fixed loopback proxy bypass

- `_sanitized_child_environment` sets both `NO_PROXY` and `no_proxy` to the literal `127.0.0.1,localhost,::1` after reading its fixed allowlist.
- Ambient proxy/bypass variables are never copied as authority and cannot override the fixed value.
- No general external-network access is enabled; only loopback names/addresses bypass host proxy discovery.

## Task 0 — Quarantine the invalid capture before implementation

1. The exact source is the complete current `docs/evidence/foundation/macos-x86_64/baseline/` tree. Before moving it or mutating any worktree/ref/bundle, write `task-0-report.md` with the verifier exit/reasons and a deterministic inventory sorted by UTF-8 relative-path bytes. Each inventory row contains relative path, regular-file type, link count, byte count, and SHA-256; the aggregate digest is SHA-256 of the canonical newline-terminated rows.
2. Create a new destination beneath `/private/var/folders/vm/g05xtp4x72q6c6365sw3d2sm0000gn/T/ai-ip-native-macos.QWBiDf/quarantine/` named `task-7-invalid-<UTC-basic-timestamp>-<aggregate-digest-prefix12>`. The destination and final basename must not already exist.
3. Move the whole source leaf once. Recompute the same inventory at the destination and require identical relative-path set, regular-file type, link count, byte count, per-file SHA-256, total count, and aggregate digest. On mismatch, stop without recreating the product evidence leaf and without deleting either surviving copy/artifact.
4. Never edit or delete the quarantine. Independent `task-0-review.md` must pass before Task 1.

## Task 1 — TDD and implement the recorder repair

**Files:**

- Modify `scripts/ai_ip/foundation/capture_command.py`
- Modify `scripts/ai_ip/foundation/test_capture_command.py`

1. Add RED integration tests proving a root-level selection fails while a nested `codex-rs` fake workspace is the required selection cwd; the real command remains at repository root.
2. Add RED parser tests using nextest-0.9.103-shaped JSON where root `test-count` is greater than one but exactly one runnable leaf has the exact key. Cover exact-but-mismatched, ignored, zero, the same exact name across two suites, duplicate JSON keys at every parsed depth, malformed suite/testcase/leaf shapes, compound/negated/`all()`/wildcard/suffix/extra-closing expressions, a second `-E`, `--filter-expr`, and missing values.
3. Add RED environment tests proving hostile ambient `NO_PROXY`, `no_proxy`, and uppercase/lowercase HTTP/HTTPS/SOCKS proxy values cannot replace the fixed loopback bypass.
4. Implement the minimum production changes. Do not change the matrix, manifest schema, verifier, wrapper, or any `codex-rs/**` file.
5. Run focused `test_capture_command.py`, all Python foundation tests once, `just fmt`, py_compile, upstream lock, `codex-rs` zero diff, and clean tracked diff checks. Commit only the two authorized Python files.
6. Independent task review must find no Critical or Important issue before resealing.

## Task 2 — Preserve invalid evidence and reseal without discarding caches

1. The passing Task 1 commit becomes the new `AI_IP_TOOLS_SHA`. Run the complete Python foundation suite and `just fmt-check` against that exact SHA; no duplicate run is required if Task 1's final complete suite used the same committed tree.
2. Preserve the old tools worktree and old bundle for audit. Add a new detached `tools-v2` worktree in the existing native root. Reuse the unchanged pinned-Codex worktree and existing external `cargo-target`, `cargo-home`, and pnpm store so the expensive compiled cache is not discarded.
3. Before mutation, revalidate and record `refs/heads/ai-ip-transfer/foundation-tools == d7911f421a33b2f63a81e49ffe04a5de190bb59a` and `refs/heads/ai-ip-transfer/pinned-codex == 4ef1d4b89bd419c976b04fefa0fd36844e898340`.
4. Treat resealing as a transaction. Compare-and-swap only the foundation-tools ref old-to-new with the old SHA as the expected value; never change the pinned ref. Create a fresh external bundle and checksum, then verify it advertises exactly the pinned ref at the pinned SHA and the foundation-tools ref at the new SHA, with no extra advertised head. Re-hash the old bundle and require its original digest `52ada4aac15d4490e78261414bd936849ae74634f5aa7d2e30dd8c0576008992`; never overwrite or delete it.
5. If any post-CAS step or `task-2-review.md` acceptance fails, roll back only with expected-new compare-and-swap (`git update-ref <foundation-ref> <old> <new>`), reverify both refs, and retain the quarantine, old artifacts, and every failed new artifact. A rollback race is a hard stop, not permission to force-update.
6. `task-2-review.md` PASS is the point of no return. Once Task 3 starts creating evidence bound to the new tools SHA, do not silently roll back that ref. A later capture/tool failure preserves the new capture in a new external quarantine and requires an explicit follow-up repair; an honest verifier exit `2/BLOCKED_BASELINE` is not a tooling rollback condition.
7. Record canonical paths, old/new tools SHA, refs, old/new bundle digests, cache reuse, quarantine path/digest, transaction result, and zero-provider claims in `task-2-report.md`. Independent `task-2-review.md` must pass before recapture.

## Task 3 — Recapture bootstrap and the 20-command baseline

1. Recreate `docs/evidence/foundation/macos-x86_64/baseline/` once and run bootstrap from `tools-v2` against the unchanged pinned tree with the existing external caches.
2. Verify the new bootstrap manifest binds the new tools/recorder SHA and all 29 bootstrap artifacts pass strict hashes and forbidden-content scanning.
3. Run the exact original 20 matrix IDs, in order, only through the new recorder. Do not alter expected exits or retry a nonzero capture in place.
4. Run the baseline verifier and create `baseline-summary.json`. Accept only exit `0/PASS` or exit `2/BLOCKED_BASELINE`; exit `1` remains a capture/tool blocker.
5. If the verifier reaches exit 0 or 2, ask the user once whether to run the optional complete workspace `just test`; do not infer approval. Then run the forbidden-content rescan.
6. Independent `task-3-review.md` must verify new SHA bindings, exact inventory, all manifests/log hashes, summary disposition, no forbidden content, and clean detached worktrees. A Task 3 failure follows Task 2's point-of-no-return rule and is documented in `task-3-report.md`.

## Task 4 — Resume original Task 8 boundary

- The original plan's root `test-count == 1` clauses (contract line 21 and Task 2 line 531 in `2026-08-25-01b-codex-native-evidence-macos-baseline.md`) are explicitly superseded by this repair plan beginning at commit `f75ab66be064a7bc41c41e9d102568dc3de73793`; the locked nextest runnable-leaf contract above is authoritative.
- For both `PASS` and `BLOCKED_BASELINE`, F-0003 must cite both plans, this supersession, the new tools SHA, old/new bundle digests, new summary digest, retained invalid-quarantine path/digest, and the user's optional-suite disposition.
- On macOS focused `PASS`, resume original Task 8 with those bindings. On honest `BLOCKED_BASELINE`, follow original Task 8 Step 1 with the same bindings and stop; do not authorize Work Package 3.
- In either case retain the invalid quarantine, old tools worktree, and old bundle outside Git until the Phase 0A.2 handoff is accepted.

## Verification boundary

- No `codex-rs/**` diff from `4ef1d4b89bd419c976b04fefa0fd36844e898340`.
- No command-matrix or expected-exit change.
- No evidence byte edited in place; invalid capture is preserved externally, valid recapture is create-new.
- No provider call, key read, prompt/response body, customer content, or paid cost.
- This repair can make the baseline trustworthy; it cannot predetermine that the unchanged pinned Codex baseline will pass.
