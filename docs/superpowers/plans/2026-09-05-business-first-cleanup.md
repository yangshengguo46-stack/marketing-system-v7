# Business-first Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan. Steps use checkbox syntax for tracking.

**Goal:** Retire the heavy evaluator and unblock internal marketing development without deleting user materials or replacing the removed machinery.

**Architecture:** Keep the native Codex loop and existing business primitives. Delete evaluator implementations and restore evaluator-only upstream proxy modifications. Keep historical business references outside executable evaluation assets.

**Tech Stack:** Rust/Cargo/Bazel, Python, Markdown.

**Spec:** `docs/superpowers/specs/2026-09-05-business-first-cleanup-design.md` (records the user's already-approved deletion scope).

## Global Constraints

- No replacement evaluator, security system, or new business feature.
- Preserve native Codex core/state/model/login/sandbox behavior and useful App Server changes.
- Preserve all untracked/ignored user files, credentials, external material, other worktrees, and caches.
- Historical references are not production prompt material or proof of marketing quality.
- Retired gates are `RETIRED_NOT_PASSED`; retirement is not release approval.
- All local edits/deletions use `apply_patch`; generated lockfiles and mechanical bulk deletion patches may be generated from exact Git-tracked path lists.
- No `git reset --hard`, `git clean`, broad recursive deletion, merge, push, or provider call.
- Run Rust tests through `just test`, not `cargo test`; no workspace-wide suite. Run `just bazel-lock-update` after Cargo changes and `just fmt` after final tests/fixes. Do not rerun tests after final formatting.
- This is predominantly mechanical deletion/restoration. Do not add negative tests asserting removed implementation remains absent, or tests asserting static prose. Keep tests that cover retained behavior. New behavior, if genuinely necessary, must have a failing behavioral test first.

## Task 1: Retire evaluator code, dependencies, and execution authority

**Files:**
- Delete all tracked files in `codex-rs/ai-ip-eval/`, `scripts/ai_ip/eval_lab/`, and `ai-ip-evals/` after preserving the four references below.
- Delete exactly the seven foundation evaluation files listed in the spec; retain `verify_upstream_lock.py` and `test_verify_upstream_lock.py`.
- Restore `codex-rs/responses-api-proxy/` to locked upstream `4ef1d4b89bd419c976b04fefa0fd36844e898340` using a Git-derived `apply_patch`, including removal of files added after upstream.
- Modify `codex-rs/Cargo.toml`, regenerate `codex-rs/Cargo.lock` and `MODULE.bazel.lock` as needed; remove eval-only visibility in `ai-ip-assets/skills/deliver-ai-ip-content-package/BUILD.bazel`.
- Modify `ai-ip-assets/skills/deliver-ai-ip-content-package/SKILL.md` only by removing the repeated ledger/entailment/atomic-proposition audit paragraphs. Preserve the other instructions and name.
- Create `docs/architecture/business-reference/README.md` and preserve unchanged:
  - `ai-ip-evals/lab/fixtures/golden-gift-li-culture-v1/case-blueprint.json` → `docs/architecture/business-reference/golden-gift-case-blueprint.json`.
  - `ai-ip-evals/lab/fixtures/golden-gift-li-culture-v1/source-import.json` → `docs/architecture/business-reference/golden-gift-source-import.json`.
  - `ai-ip-evals/lab/rubrics/golden-gift-l1-l2-rubric.json` → `docs/architecture/business-reference/golden-gift-rubric.json`.
  - `ai-ip-evals/rubrics/content-package-blind-review.json` → `docs/architecture/business-reference/content-package-rubric.json`.
- Update `docs/superpowers/plans/README.md`, the master roadmap, main product spec and `docs/evidence/foundation/README.md`; append a retirement entry to `docs/architecture/codex-fork-patch-ledger.md`. Mark evaluator-specific historical plans/specs RETIRED at their top without deleting history. Existing source-provenance and business-primitive plans can be historical completed records; no old child remains executable.

**Interfaces:**
- Consumes: approved spec, recovery commit, locked upstream tree, existing business schema/API tests.
- Produces: build graph without `codex-ai-ip-eval`, original native proxy interface, four preserved reference files, and current business-first development authority without proof-lab prerequisites.

- [ ] Verify branch/recovery point and inspect exact `git ls-files` targets. Read this spec and relevant local `AGENTS` overrides. No directory outside the checkout is a deletion target.
- [ ] Preserve the four reference files using `apply_patch` and compare their bytes with `git show codex/pre-business-cleanup-20260905:<old-path>` before deletion.
- [ ] Generate one mechanical deletion patch from the validated tracked file list. Each entry is `*** Delete File: <absolute-path>`. Apply it; never recursively remove directories containing ignored files.
- [ ] Remove the workspace member and dependency named `codex-ai-ip-eval`, eval-only Bazel visibility, and restore proxy files from upstream using `apply_patch`. Inspect inbound references with `rg -n 'codex[-_]ai[-_]ip[-_]eval|ai-ip-evals|scripts/ai_ip/eval_lab' codex-rs scripts ai-ip-assets` and resolve executable/build references, not historical citations.
- [ ] Remove only the five long repeated audit bullets between material reading and facts/inferences guidance from the Lead Skill. No new methodology or output schema is introduced.
- [ ] Update current docs to the approved scope. Historical plans may retain exact prior claims under an explicit retirement notice; current authority must not require their completion or label them passed. State the next outcome as a real marketing business slice, not a new proof subsystem.
- [ ] Regenerate lockfiles and run available retained checks using the toolchain already installed:

```sh
export PATH=/Users/yangyucheng/.cache/ai-ip-tools/bazelisk-1.28.1/node_modules/.bin:/Users/yangyucheng/.cache/ai-ip-tools/cargo-nextest-0.9.103/bin:/Users/yangyucheng/.rustup/toolchains/1.95.0-x86_64-apple-darwin/bin:/usr/local/bin:/usr/bin:/bin
export CARGO_HOME=/Users/yangyucheng/.cache/ai-ip-tools/cargo-nextest-0.9.103
cargo metadata --manifest-path codex-rs/Cargo.toml --offline --format-version 1 --no-deps
just bazel-lock-update
just test -p codex-ai-ip-domain -p codex-ai-ip-runtime -p codex-responses-api-proxy --offline
uv run --python 3.11 --with pytest==9.0.2 pytest -q scripts/ai_ip/foundation/test_verify_upstream_lock.py
```

The controller owns the already-started pre-change business-crate baseline. Do not run a competing duplicate baseline. If an offline cache is unavailable, report it; ordinary public dependency fetch is permitted, but provider/credential work is not.

- [ ] Run the retained App Server strict-output test if the installed build environment supports it: `just test -p codex-app-server --offline -E 'test(ai_ip_strict_output)'`. Report baseline host/dependency limitations without modifying unrelated runtime code. Do not launch removed evaluator/process/attestation tests.
- [ ] Run `just fix -p codex-responses-api-proxy` only if restoration produced an actual novel Rust change needing lint fixes; a byte-for-byte upstream restoration does not require rewriting upstream style. Run `just fmt` after all tests. Inspect formatting-only drift and restore unrelated files through `apply_patch`.
- [ ] Inspect `git diff --check`, compare native proxy bytes to upstream and preserved references to recovery, and confirm protected core/state/model/login/sandbox paths have no cleanup diff. These are one-time removal checks, not new permanent tests.
- [ ] Commit coherent mechanical removal and documentation changes. Write report with exact commits, removed/preserved counts, commands/results, remaining limits, and recovery reference. Do not claim business quality, live-model reachability, or customer readiness.
