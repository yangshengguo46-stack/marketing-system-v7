# Phase 0A.2 Task 7R — Nextest Selection and Loopback Recapture Repair

> **Execution:** Use `superpowers:systematic-debugging`, `superpowers:test-driven-development`, and `superpowers:subagent-driven-development`. This is a bounded repair of the frozen native-evidence child, not a business/runtime change.

**Goal:** Correct two reproduced recorder-environment defects, seal a new tools SHA, and recapture the unchanged macOS x86_64 baseline without editing or deleting the invalid evidence bytes.

**Base:** `d7911f421a33b2f63a81e49ffe04a5de190bb59a`

**Provider boundary:** `providerMode=not-run`; `paidProviderCost=0`; do not locate or read an API key.

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
- Require exactly one `-E` argument pair whose expression is exactly `test(=<nonempty-name>)`; reject any other selection expression before a child command runs.
- Strictly parse the nextest JSON root and dynamic `rust-suites` objects. Root `test-count` is population metadata and must be a non-boolean nonnegative integer, but it is not the exact-filter count.
- Count exact key equality with the extracted test name across every suite's `testcases` object. The manifest's existing `selection.testCount` field records that exact-match count and must equal `1` before the real command runs.
- Duplicate JSON keys, malformed suite/testcase objects, zero matches, and duplicate exact matches remain `BLOCKED_SELECTION`.

### Fixed loopback proxy bypass

- `_sanitized_child_environment` sets both `NO_PROXY` and `no_proxy` to the literal `127.0.0.1,localhost,::1` after reading its fixed allowlist.
- Ambient proxy/bypass variables are never copied as authority and cannot override the fixed value.
- No general external-network access is enabled; only loopback names/addresses bypass host proxy discovery.

## Task 1 — TDD and implement the recorder repair

**Files:**

- Modify `scripts/ai_ip/foundation/capture_command.py`
- Modify `scripts/ai_ip/foundation/test_capture_command.py`

1. Add RED integration tests proving a root-level selection fails while a nested `codex-rs` fake workspace is the required selection cwd; the real command remains at repository root.
2. Add RED parser tests using nextest-0.9.103-shaped JSON where root `test-count` is greater than one but the exact test key occurs once. Add zero/duplicate/malformed cases.
3. Add RED environment tests proving hostile ambient `NO_PROXY`, `no_proxy`, and proxy values cannot replace the fixed loopback bypass.
4. Implement the minimum production changes. Do not change the matrix, manifest schema, verifier, wrapper, or any `codex-rs/**` file.
5. Run focused `test_capture_command.py`, all Python foundation tests once, `just fmt`, py_compile, upstream lock, `codex-rs` zero diff, and clean tracked diff checks. Commit only the two authorized Python files.
6. Independent task review must find no Critical or Important issue before resealing.

## Task 2 — Preserve invalid evidence and reseal without discarding caches

1. Before moving anything, compute an inventory digest for the complete invalid evidence directory and record the verifier exit/reasons in the ignored repair report.
2. Move the complete directory, without modifying its contents, to a create-new canonical external quarantine directory beneath the existing native root. Prove every relative file, byte count, and SHA-256 is identical after the move. Do not delete the quarantine.
3. The passing Task 1 commit becomes the new `AI_IP_TOOLS_SHA`. Run the complete Python foundation suite and `just fmt-check` against that exact SHA; no duplicate run is required if Task 1's final complete suite used the same committed tree.
4. Preserve the old tools worktree and old bundle for audit. Add a new detached `tools-v2` worktree in the existing native root. Reuse the unchanged pinned-Codex worktree and existing external `cargo-target`, `cargo-home`, and pnpm store so the expensive compiled cache is not discarded.
5. Compare-and-swap only `refs/heads/ai-ip-transfer/foundation-tools` from old tools SHA `d7911f421a33b2f63a81e49ffe04a5de190bb59a` to the new SHA; the pinned ref remains exact. Create and verify a fresh external two-ref bundle/checksum. Do not overwrite or delete the old bundle.
6. Record canonical paths, old/new tools SHA, refs, old/new bundle digests, cache reuse, quarantine path/digest, and zero-provider claims in the ignored repair report. Independent review must pass before recapture.

## Task 3 — Recapture bootstrap and the 20-command baseline

1. Recreate `docs/evidence/foundation/macos-x86_64/baseline/` once and run bootstrap from `tools-v2` against the unchanged pinned tree with the existing external caches.
2. Verify the new bootstrap manifest binds the new tools/recorder SHA and all 29 bootstrap artifacts pass strict hashes and forbidden-content scanning.
3. Run the exact original 20 matrix IDs, in order, only through the new recorder. Do not alter expected exits or retry a nonzero capture in place.
4. Run the baseline verifier and create `baseline-summary.json`. Accept only exit `0/PASS` or exit `2/BLOCKED_BASELINE`; exit `1` remains a capture/tool blocker.
5. If the verifier reaches exit 0 or 2, ask the user once whether to run the optional complete workspace `just test`; do not infer approval. Then run the forbidden-content rescan.
6. Independent evidence review must verify new SHA bindings, exact inventory, all manifests/log hashes, summary disposition, no forbidden content, and clean detached worktrees.

## Task 4 — Resume original Task 8 boundary

- On macOS focused `PASS`, resume original Task 8 with the new tools SHA, new bundle digest, new summary digest, and the user's optional-suite disposition.
- On honest `BLOCKED_BASELINE`, follow original Task 8 Step 1 and stop; do not authorize Work Package 3.
- In either case retain the invalid quarantine, old tools worktree, and old bundle outside Git until the Phase 0A.2 handoff is accepted.

## Verification boundary

- No `codex-rs/**` diff from `4ef1d4b89bd419c976b04fefa0fd36844e898340`.
- No command-matrix or expected-exit change.
- No evidence byte edited in place; invalid capture is preserved externally, valid recapture is create-new.
- No provider call, key read, prompt/response body, customer content, or paid cost.
- This repair can make the baseline trustworthy; it cannot predetermine that the unchanged pinned Codex baseline will pass.
