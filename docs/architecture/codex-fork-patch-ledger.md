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

## F-0002B — Repair and reauthorize native evidence directory pairing

- Owner: Phase 0A.2 Task 3 review repair under `superpowers:receiving-code-review` and `superpowers:subagent-driven-development`.
- Upstream base: `4ef1d4b89bd419c976b04fefa0fd36844e898340`.
- Repair base: reviewed Task 3 implementation tip `7d537e2c72edf3f76ec3d3f431e9117e507f7b7f`; this entry and the repaired plan land as one governance commit before verifier repair resumes.
- Historical relationship: F-0002 and F-0002A remain byte-preserved append-only history; F-0002B reauthorizes only the same Phase 0A.2 evidence-layout contract and changes no product, provider, model, or business scope.
- Classification: **modify** only the Phase 0A.2 plan's baseline/post evidence-directory layout; **preserve** the matrix, recorder, verifier implementation and tests, runtime, `codex-rs/**`, provider rules, gates, and business contracts.
- Root cause: independent Task 3 review recomputed that trustworthy Post failure pairing requires an immutable sibling Baseline leaf, while Tasks 6–8 still created and verified the macOS Baseline directly at the platform root; the plan therefore lacked a unique baseline/post pairing authority.
- Repair: baseline/post CLI receives an exact `<foundation>/<platform>/baseline|post` leaf, frozen-final receives the foundation directory and resolves each platform's `post` leaf, every Post pairs only with its exact sibling `baseline`, Tasks 6–8 use `docs/evidence/foundation/macos-x86_64/baseline/`, and the future independent Windows child uses the same leaf convention without claiming Windows evidence exists.
- Regression/review proof: exact documentation RED exposed all missing contracts and platform-root executable paths; the same plan-fence assertions must pass after repair, together with upstream-lock verification, `git diff --check`, `codex-rs` zero diff, append-only ledger-prefix proof, and an independent review before implementation resumes.
- Provider disposition: `providerMode=not-run`; `paidProviderCost=0`; no API key was located or read.
- Gate disposition: Phase 0A.2 remains the only current executable child; `G0=OPEN`, `G1=OPEN`, `G2=OPEN`, `PASS_TO_PHASE_0B=false`.
- Upstream-sync risk: evidence tooling or future plans that assume evidence lives directly at a platform root must migrate explicitly to the exact sibling-leaf contract and recapture affected native evidence; no implicit migration or copied Baseline summary is authorized.
- Rollback: locate the cohesive F-0002B governance commit by this exact section title, review its two-document diff, and revert that commit only; never rewrite imported ancestry or remove F-0002/F-0002A history.

## F-0004 — Expose a gated Responses proxy server

- Owner: Phase 0A Work Package 5 Task 1 under `docs/superpowers/plans/2026-08-27-04-native-atomic-paired-evaluator.md`. F-0003 remains unissued because the earlier native baseline child stopped without its planned F-0003 handoff.
- Pinned fork base: `de86eb6938a88b348e28884346dcf1ab0c7cafad`.
- Exact implementation commit: `de67d411a25ad89b793404717bb0c6f744fa2b5d`.
- Classification: **directly modify** the upstream `codex-responses-api-proxy` library/CLI seam; **add** a synchronous gateable server surface, bounded request transform, body-free observer metadata, deadline-bound permit, explicit lifecycle handle, SSE metadata parser, and focused tests; **preserve** the legacy stdin credential path and CLI flags.
- Touched seams: `codex-rs/responses-api-proxy/**` and its existing Cargo lock entry only. No App Server, `codex-core`, provider adapter, product UI, customer persistence, business domain, Lead Skill, or automatic factual guard changed.
- Business reason: make the later generic/candidate Lead-Skill comparison atomic and measurable without forcing a creative workflow. The evaluator can authorize each attempt before forwarding, inject the real frozen token limit, account for completed Responses metadata, and close in-flight work before sealing an arm.
- Credential/body boundary: the locked header remains readable only from hardened stdin and is passed as a non-cloneable redacted wrapper; the upstream client disables redirects and environment proxies; gate and observer APIs receive no request or response body; transformed request, JSON string fields, and SSE event buffers are cleared after their short-lived use. Synthetic loopback tests use only the literal `test-token`.
- Regression proof: TDD RED failed on the unresolved gate/observer/transform API; final scoped Cargo verification passed `22/22`; Bazel passed both `responses-api-proxy-unit-tests` and `codex-responses-api-proxy-bin-unit-tests`; `just bazel-lock-check`, `just fmt`, and `just fix -p codex-responses-api-proxy` exited 0. Per repository rule, tests were not rerun after final `fmt`/`fix`.
- Provider disposition: `providerMode=not-run`; `paidProviderCost=0`; no real API key, provider endpoint, customer material, or external network was used.
- Gate disposition: this is only the first Work Package 5 mechanical GREEN boundary. Work Package 6 human review, Work Package 8 approved live pair, G0–G2, `PASS_TO_PHASE_0B`, and product Plan 02 remain blocked.
- Upstream-sync risk: upstream changes to the synchronous proxy loop, stdin hardening, dump behavior, tiny-http lifecycle, reqwest redirect/proxy defaults, Responses headers, or SSE completion shape require replaying the 22-test Cargo suite and both Bazel targets before rebasing this seam.
- Rollback: first revert the later ledger append commit containing this entry, then revert `de67d411a25ad89b793404717bb0c6f744fa2b5d`. Any later evaluator commit that imports the gateable API must be reverted first; never keep a compatibility shim or silently route around the proxy.
