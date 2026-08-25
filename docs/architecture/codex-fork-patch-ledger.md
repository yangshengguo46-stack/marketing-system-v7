# Codex fork patch ledger

This append-only ledger explains why AI IP 1.1 diverges from the locked OpenAI Codex ancestor. A patch is not accepted merely because it compiles: it must identify the upstream seam, business reason, regression proof, sync risk, and removal or rollback path.

## Entry contract

Each later entry contains:

- stable entry ID and owning implementation-plan task;
- upstream base and exact fork commit;
- classification: **preserve**, **directly modify**, **replace**, or **add**;
- touched files and externally visible contracts;
- business result that requires the change;
- focused and integration tests with evidence locations;
- upstream-sync conflict surface;
- deletion or rollback procedure.

No entry may contain credentials, private cases, prompt or response bodies, reviewer mappings, or customer content.

## F-0001 — Establish source provenance

- Owner: Phase 0A.1 Codex Fork Provenance.
- Upstream base: `4ef1d4b89bd419c976b04fefa0fd36844e898340`.
- Product origin status: `unconfigured`.
- Classification: **preserve** upstream Git ancestry, `LICENSE`, root `AGENTS.md`, and upstream NOTICE; **modify** `.gitattributes` and `NOTICE`; **add** source-lock verification and fork governance.
- Touched seams: repository metadata and documentation only; `codex-rs/` is unchanged.
- Business reason: permit a deep business-first Codex fork without losing reproducible source attribution or silently falling back to DeerFlow.
- Regression proof: `scripts/ai_ip/foundation/test_verify_upstream_lock.py` and a clean-tree run of `scripts/ai_ip/foundation/verify_upstream_lock.py`.
- Upstream-sync risk: later upstream changes to `LICENSE`, `NOTICE`, root `AGENTS.md`, or `.gitattributes` require an explicit lock update and ledger entry; the verifier fails closed on silent drift.
- Rollback: remove only the AI IP provenance commit after first returning to the source-import merge commit; never rewrite or discard either imported ancestor.

## F-0002 — Authorize native evidence and macOS baseline child

- Owner: Phase 0A.2 plan authoring under `superpowers:writing-plans` and two independent read-only reviews.
- Upstream base: `4ef1d4b89bd419c976b04fefa0fd36844e898340`.
- Reviewed plan tip: `b90d1297f406cd8dd85aa916d4ba7c68b630ed2b`.
- Classification: **preserve** all runtime and `codex-rs/**`; **add** `docs/superpowers/plans/2026-08-25-01b-codex-native-evidence-macos-baseline.md`; **modify** only the master-roadmap and plan-index current-child pointers.
- Touched seams: implementation governance and documentation only; no App Server, provider, Skill, prompt, model, business contract, or evidence artifact is changed by this entry.
- Business reason: convert Work Package 2 into an executable, fail-closed child that seals common evidence tools and captures the unchanged native macOS x86_64 baseline before the first business change, while leaving Windows as an honest independent mandatory child on the same tools SHA.
- Regression proof: canonical matrix review digest `04deab6a2769e491b2b313a1f3b44bd11b1ce7f58b5f6893b12e8cb8105c140a`; Bash and Python plan-fence syntax checks; balanced Markdown fences; source-lock verifier; `25 passed` source-lock tests; two final reviewer verdicts `Ready: Yes` with no Critical or Important findings.
- Provider disposition: `providerMode=not-run`; `paidProviderCost=0`; no API key was located or read.
- Gate disposition: Phase 0A.1 is complete; Phase 0A.2 is the only current executable child; G0/G1/G2 remain open and `PASS_TO_PHASE_0B=false`.
- Upstream-sync risk: documentation-only. A change to the pinned source, Work Package 2 contract, matrix content, or reviewed plan tip invalidates this authorization and requires a new append-only entry and review.
- Rollback: revert plan-review commits `b90d1297f406cd8dd85aa916d4ba7c68b630ed2b`, `05f29df8ac56d5953dd18f471fab2d04cf11a6d0`, and `cfa71ea4f88207595c5d0b4baed05dccf9361ef0` in that order after reviewing each diff; never rewrite imported ancestry. F-0003 separately owns any later tool or baseline-evidence rollback.

## F-0002A — Repair and reauthorize native tool version probes

- Owner: Phase 0A.2 Task 0 plan-probe repair under `superpowers:systematic-debugging` and `superpowers:subagent-driven-development`.
- Upstream base: `4ef1d4b89bd419c976b04fefa0fd36844e898340`.
- Repaired and independently reviewed plan tip: `1dc50293df13f1c5ae574d750ca6f26538441d7e`.
- Historical relationship: F-0002 remains append-only history; F-0002A reauthorizes the same Phase 0A.2 child at the repaired plan tip and changes no product scope.
- Classification: **modify** only the Phase 0A.2 plan's uv, cargo-nextest, and Bazelisk output-shape contracts; **preserve** all versions, SHAs, matrix content, runtime, `codex-rs/**`, provider rules, gates, and business contracts.
- Root cause: exact locked artifacts produced `uv 0.11.3` plus build metadata, multi-line `cargo-nextest 0.9.103` metadata, and raw Bazelisk npm package version `v1.28.1`; the original assertions rejected those legitimate exact artifacts.
- Repair: authorize first-nonempty-line prefix-plus-exact-version-token checks for uv/nextest and exact raw `v1.28.1` for Bazelisk, with Task 0 and Task 6 kept consistent.
- Regression/review proof: all three repaired real-tool assertions exit 0; direct Bazel SHA remains `aa7e5fc364eaaba7f4f271dbf8c14172a5433f663cca6b130325df4b6569b3f0`; upstream-lock verifier and `codex-rs` zero-diff pass; independent task reviewer verdict is `Approved` with no Critical, Important, or Minor findings.
- Provider disposition: `providerMode=not-run`; `paidProviderCost=0`; no API key located or read.
- Gate disposition: Phase 0A.2 remains the only current executable child; `G0=OPEN`, `G1=OPEN`, `G2=OPEN`, `PASS_TO_PHASE_0B=false`.
- Upstream-sync risk: any later change to version-output shapes or repaired plan tip requires another append-only entry and review.
- Rollback: identify and review the later ledger-only commit through Git history, revert it, then revert plan repair commit `1dc50293df13f1c5ae574d750ca6f26538441d7e`; never rewrite imported ancestry.
