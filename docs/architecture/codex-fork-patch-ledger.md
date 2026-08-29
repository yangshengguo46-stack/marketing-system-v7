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

## F-0005 — Emit descendant thread starts to App Server clients

- Owner: Phase 0A Work Package 5 Task 3 under `docs/superpowers/plans/2026-08-27-04-native-atomic-paired-evaluator.md`.
- Pinned fork base: `8125cc7fb9ebcd32b76bd3bda29d005cdaffe42b`.
- Exact implementation commit: the cohesive commit titled `test: cover descendant raw response usage` that contains this append-only entry.
- Classification: **directly modify** the upstream App Server `thread_processor.rs::try_attach_thread_listener` seam; **add** descendant raw-response/parent-edge and strict-output integration coverage; **preserve** `codex-core`, provider adapters, evaluator accounting, prompts, Skills, product UI, and customer persistence.
- Touched seam and contract: when core creates a subagent thread, App Server now builds its typed thread snapshot and sends `thread/started` only to the nonempty initialized-connection set that is auto-attached to the descendant. Existing listener attachment, descendant `rawResponse/completed`, typed `thread/read`, and terminal notifications remain authoritative and unchanged.
- Business reason: the paired evaluator must account for the complete native root/child/grandchild/sibling tree and cross-check every parent edge from both pushed `thread/started.thread.parentThreadId` and typed `thread/read`; reconstructing descendants from raw responses alone would weaken the frozen evidence contract.
- Characterization and regression proof: before the repair, the exact focused test completed all six synthetic response routes and observed all descendant raw/terminal events but timed out after receiving only the root `thread/started`. After the repair, both exact focused tests passed twice, including all six raw usage records, four parent-edge notifications, typed read cross-checks, terminal ordering, and the provider-visible strict `ContentPackage` Schema. `just bazel-lock-update` and `just bazel-lock-check` exited 0. The package-wide Cargo run also passed both new tests but was not globally green because unrelated existing tests lacked prebuilt helper binaries or observed host/plugin and timing-dependent failures. The required Bazel target did not reach test execution in the bounded local attempt; the controller's prior attempt identified the external `rusty_v8_libcxxabi` fetch from `chromium.googlesource.com` as the blocker.
- Provider disposition: `providerMode=not-run`; `paidProviderCost=0`; tests used only synthetic loopback Responses fixtures, and no API key, real provider, customer material, or live model was located or read.
- Upstream-sync risk: changes to core thread-created timing, App Server connection targeting, listener attachment, thread snapshot construction, raw-response translation, or descendant lifecycle ordering require replaying both exact focused tests and the full Bazel App Server target before rebasing this seam.
- Rollback: revert the cohesive `test: cover descendant raw response usage` commit. Do not retain the tests while weakening them to infer parent edges only from raw responses, and do not move missing descendant accounting into the evaluator.

## F-0005A — Authorize provider-free blind review and score child

- Owner: Phase 0A Work Package 6A plan authoring under `superpowers:writing-plans`; execution requires `superpowers:subagent-driven-development`, per-task TDD, and two-stage independent review.
- Pinned fork base: `eea75f7771bcfafea5bb9107d329a10d5b4b6429`.
- Authorization record: the cohesive documentation commit titled `docs: authorize provider-free blind review scoring`; its tree contains `docs/superpowers/plans/2026-08-28-06a-blind-review-and-score.md` plus the matching master-roadmap and plan-index pointers.
- Historical relationship: Plans 04 and 05 remain byte-preserved mechanical/semantic evidence. This entry advances only the current executable child pointer; it does not reclassify their replay/mock evidence as live or business PASS.
- Classification: **add** the Work Package 6A executable child; **modify** only plan navigation and this append-only ledger; **preserve** runtime, `codex-rs/**`, evaluator outputs, private evidence, provider configuration, credentials, G0–G2, and product Plans 02–15 at authorization time.
- Business reason: convert the first Work Package 6 boundary into small TDD/review units that freeze the six-dimension business rubric, hide treatment from three independent human reviewers, and compute the exact PASS/ITERATE/INVALID decision before any paid or product work can outrun business proof.
- Scope fence: 06A may add static rubric/schema assets, private typed transcript/catalog/config/tree sealing, fail-closed pair re-verification, three independent reviewer bundles/mappings/seeds, and private deterministic score. It may not implement or claim cost/report publication, retention completion, provider reachability, live quality, G2, `PASS_TO_PHASE_0B`, or product Plan 02.
- Provider disposition: `providerMode=not-run`; authorized provider cost `0`; paid provider cost `0`; no API key, provider endpoint, customer material, or external network is authorized by this entry.
- Parked blockers: Work Package 8 still requires closing both same-UID ancestor-swap findings before a real endpoint is reachable: managed case/eval-tree ancestors and the actual executable/kernel-loaded vnode ancestry. 06A must not weaken, relabel, or silently bypass either blocker.
- Regression/review proof: plan self-review covers Work Package 6 Contracts 1–3, exact type consistency, and forbidden-marker scan; an independent read-only plan review must report no Critical/Important finding before this authorization commit is created. Implementation evidence belongs to later F-0006 and cannot be inferred from this authorization.
- Upstream-sync risk: limited at authorization time to plan/governance files. During implementation, any change to App Server notification serialization, evaluator manifest fields, catalog/config/tree evidence, Bazel runfiles, or private-root filesystem semantics requires replaying 06A's focused and full Cargo/Bazel matrix.
- Rollback: revert only the cohesive authorization commit after confirming no later 06A implementation commit depends on it; if implementation exists, revert F-0006 and its owned commits first. Never rewrite imported ancestry or remove earlier append-only entries.

## F-0006 — Implement provider-free blind review and private scoring

- Owner: Phase 0A Work Package 6A Tasks 1–7 under `docs/superpowers/plans/2026-08-28-06a-blind-review-and-score.md`, using per-slice TDD, `superpowers:subagent-driven-development`, and independent contract/quality reviews.
- Pinned fork base: `eea75f7771bcfafea5bb9107d329a10d5b4b6429`.
- Exact pre-ledger implementation range: `1b7d5e6fa^..a8d52f4be`; exact reviewed repair tip: `a8d52f4be`. This entry lands in the cohesive documentation commit titled `docs: record provider-free blind review implementation`.
- Disposition: the provider-free implementation is present through a mechanically exercisable immutable private decision, but the Work Package 6A regression gate remains **OPEN** because the real Native test harness still exceeds the repository's 60-second watchdog in a clean serial run. This entry is not a GREEN, business-PASS, or Phase 0B handoff claim.
- Classification: **directly modify** the fork evaluator's private evidence, filesystem, runner, blind-review, and score seams; **add** frozen rubric/schema assets, sealed private evidence types, deterministic bundle/mapping/submission/decision contracts, and private CLI coverage; **preserve** upstream App Server public protocol, provider adapters, product UI, customer persistence, credentials, public reporting, and all Work Package 8 live-provider blockers.
- Touched paths: `ai-ip-evals/rubrics/**`, `ai-ip-evals/schemas/**`, `codex-rs/ai-ip-eval/{src,tests,BUILD.bazel,Cargo.toml}`, root `MODULE.bazel.lock`, `codex-rs/{Cargo.toml,Cargo.lock}`, and the 06A plan/specification documents. The exact range changes 116 paths. The evaluator-local `codex-rs/ai-ip-eval/src/app_server.rs` client seam is included; no upstream `codex-rs/app-server*/**`, `codex-core`, provider-adapter, or product path is part of F-0006.

### Exact Task 1–7 commit register

The IDs below are chronological inside each bucket. Rollback uses the reverse task order and reverses each bucket.

- Task 1 runtime/test: `1b7d5e6fa`.
- Task 2 runtime/test: `981d0bb02`.
- Task 3 runtime/test: `ef34734e7`, `f66132ebb`, `e916fda3a`, `e06bffb61`.
- Task 4 authority/docs: `8bd3329ee`, `64f00cddc`. Runtime/test: `4cc55883d`, `ee3f74db7`, `024c8f98b`, `14d022f62`, `46be053f6`, `d1a0126cc`, `bfa29fc56`.
- Task 5 authority/docs: `baf8ada98`, `aed84de9b`, `def5331f3`, `f6ca07d23`, `866e98d62`, `f6e6671fe`, `129e5c822`, `991d267d7`, `7e9e23f61`, `8070e1516`, `d9bab5b41`, `997ff7331`, `f3cff449e`, `7faff5b39`, `66b92f279`, `9a488eff2`, `23b195b3d`, `ad7d26f1f`, `ec3acc319`, `76da7cf58`, `d7f559c9c`, `c42dfde4e`, `d03912200`, `c2db67aae`. Runtime/test: `c777ff4da`, `1fb2139bf`, `7b904b14f`, `035fbd437`, `ba6565946`, `5dfab6df0`, `10c44d5cb`, `305b27b59`, `805edbb26`, `8be761822`, `a980cecb3`, `8ddd4c994`, `80af9f456`, `83b2c965c`, `ffc9d53b3`, `c26c3ff3f`, `5ceee3f92`, `03b058696`, `c36ca5bfc`, `a5be2ac97`, `1fd8ddd58`, `51a8bdfa2`, `260928ec7`, `cf01fdcaa`.
- Task 6 authority/docs: `62c5dc628`, `4b6d88a29`, `1c330450c`, `766d618c9`, `8bafa3019`, `73171bd95`. Runtime/test: `effcaedaa`, `507e4772d`, `74bead584`, `2d1eea678`, `1b2a16fe0`, `d7891be0b`, `2788487a2`, `286d60a94`, `25c5b375f`, `86c518f0c`.
- Task 7 authority/docs: `7640ab856`, `f9b6b6a34`, `16387f518`, `a22a3050f`. Runtime/test: `8adaa1477`, `5c5d7ce7a`, `354f3b9bb`, `45464191e`, `23b696e47`, `f7fc563f7`, `9e71b5336`, `7a62b54ec`, `31a59eaf0`, `c83b1138d`, `03d282c17`, `d3100259f`, `0ab769af8`, `59f8d0568`. Task 7E regression-fixture repair: `a8d52f4be`.

### Business and privacy boundary

- Business result implemented: one Lead remains responsible for the result while the evaluator freezes the business rubric and content contract, seals a generic/candidate pair, creates three independently oriented blind-review bundles, verifies three signed reviewer submissions, and computes a canonical body-free private `PASS`, `ITERATE`, or `INVALID_PROOF` decision. The implementation is provider-free plumbing for proving business quality; it does not replace the later real-review/live-provider decision.
- The private side retains case/evidence bindings, exact prompts and Skill commitments, pair execution evidence, reviewer bundles, mappings, seeds, reviewer submissions, validation failures, metrics, and the immutable decision. No prompt, response body, reviewer mapping, seed, customer material, private path, or review reason is copied into a public report.
- Replay can mechanically produce a private `PASS` to prove the scoring transaction. That result is synthetic regression evidence only. No 06A command emits `BUSINESS_SIGNAL_PASS_PENDING_FOUNDATION`, `PASS_TO_PHASE_0B`, or any equivalent public PASS.
- No knowledge-base claim is introduced. The sealed evidence/inventory is a bounded proof substrate, not a general retrieval store or an authorization to let historical memory override current business evidence.

### Regression and review evidence

- Evidence location: unless a repository path is named, the commands, counts, durations, and temporary diagnostics below are controller-terminal evidence in this Codex task and were not persisted as a standalone log. They are recorded here precisely so the OPEN result is not converted into an untraceable or false GREEN claim.
- Scoped score CLI regression: `just test -p codex-ai-ip-eval -E 'test(score_cli)'` completed `6/6` passing, including real private `PASS`, contract-valid `ITERATE`, semantic `INVALID_PROOF`, current-evaluator drift, and frozen-Codex-byte drift with no invalid side effects.
- First default-concurrency full evaluator run, `just test -p codex-ai-ip-eval`: `284` tests; `243` passed, `4` assertion failures, and `37` timed out. The four assertion failures were stale parser-mutation fixtures whose changed bytes were correctly preempted by the stricter inventory verifier; they were not production-verifier failures.
- Controlled serial full evaluator run, `just test -p codex-ai-ip-eval --test-threads=1`: with the narrow fixture repair present in the working tree but before commit `a8d52f4be`, `284` tests ran; `275` passed and `9` timed out, with no assertion failure. The four deterministic fixtures passed in full-order execution. The nine timeouts shared the real Native pair harness, which copies/hashes the approximately 103-MiB evaluator binary, freezes a Native context, establishes private executable images, launches one or both loopback App Server arms, scans reached trees, and waits for quiet windows; successful Native pairs launch both arms.
- Focused fixture regression after repair: `just test -p codex-ai-ip-eval -E 'test(blind_envelope_rejects_missing_and_linked_fixed_paths) | test(blind_envelope_rejects_nonexact_document_encodings) | test(blind_envelope_rejects_replay_binding_and_direct_link_drift) | test(blind_envelope_rejects_wrong_arm_shape_and_replay_modes) | test(blind_pair_parser_retains_exact_real_replay_and_native_envelopes)' --test-threads=1 --retries=0` completed `5/5` passing (`3` slow; slowest `56.508s`). After the current Windows retained-reader diagnostic was added, `just test -p codex-ai-ip-eval -E 'test(blind_envelope_rejects_missing_and_linked_fixed_paths)' --test-threads=1 --retries=0` completed `1/1` in `35.727s`. Independent final review reported `Critical=0`, `Important=0`, `Minor=0`, `Ready=Yes` for the narrow repair.
- Native characterization is not counted as GREEN. `just test -p codex-ai-ip-eval -E 'test(sealed_native_pair_reaches_blind_bundle_stage) | test(sealed_native_pair_rejects_fully_resigned_cross_arm_deployment_drift) | test(sealed_native_pair_rejects_fully_resigned_cross_arm_model_drift) | test(sealed_native_pair_treats_path_sha_as_historical_commitment)' --test-threads=1` completed `4/4` in `29.279s`–`36.678s`. `just test -p codex-ai-ip-eval -E 'test(blind_envelope_rejects_native_mode_and_execution_link_drift) | test(blind_native_pair_rejects_fully_resigned_live_manifest_mode) | test(blind_pair_parser_retains_exact_real_replay_and_native_envelopes)' --test-threads=1` completed the first two in `35.796s` and `33.564s`, while the combined exact-envelope case timed out twice at `60s`. `just test -p codex-ai-ip-eval -E 'test(blind_pair_parser_retains_exact_real_replay_and_native_envelopes)' --test-threads=1` then timed out twice in isolation. `just test -p codex-ai-ip-eval -E 'test(generic_target_skill_read_during_quiet_window_poisons_without_pair_receipt) | test(generic_target_skill_read_poisons_without_pair_receipt)' --test-threads=1 --retries=0` timed out both exact poison selectors at `60s`.
- Temporary uncommitted characterization, fully removed before `a8d52f4be`: the exact-envelope test was split only for diagnosis, its Native half was run with `just test -p codex-ai-ip-eval -E 'test(blind_pair_parser_retains_exact_real_native_envelope)' --no-capture --retries=0`, and a scoped temporary `slow-timeout = { period = "1m", terminate-after = 2 }` still terminated it at `120.175s`. Temporary stage prints located the delay inside `LivePair` before blind parsing. The split, prints, timeout override, and nextest cleanup experiment were all reverted and are not part of the registered implementation range.
- `just fix -p codex-ai-ip-eval` and `just fmt` exited `0`; formatter/Clippy changes outside the two authorized repair files were restored. Per repository rule, tests were not rerun after final fix/format.
- No full evaluator run may be summarized as `284/284`, and Work Package 6A/Task 7E remains open until the Native harness completes under an approved regression strategy and the required Task 8 Cargo/Bazel matrix runs.

### Provider, gate, and risk disposition

- Provider disposition: `providerMode=not-run`; `paidProviderCost=0`; only deterministic Replay and `127.0.0.1` loopback mock traffic ran. No API key, real provider endpoint, external network, customer material, or paid model was located, read, or used.
- Parked Work Package 8 blockers remain unchanged and mandatory before any real endpoint: (1) same-UID ancestor swap of managed case/eval-tree paths; (2) same-UID ancestor swap of the actual executable/kernel-loaded vnode ancestry. Neither was weakened, relabeled, or bypassed.
- Gate disposition: `WORK_PACKAGE_6A_REGRESSION=OPEN`; `G0=OPEN`; `G1=OPEN`; `G2=OPEN`; `PASS_TO_PHASE_0B=false`. Product Plan 02, approved live pair, public cost/reporting, retention closeout, and live-quality claims remain blocked.
- Upstream-sync risk: high around evaluator private-root traversal/identity semantics, App Server notification and child-process lifecycle, manifest/pair wire types, Responses proxy broker evidence, private inventory append rules, secure no-replace publication, exact JCS/pretty-JSON encodings, CLI/Bazel compile-data wiring, and large test-binary/private-executable-image behavior. Before rebasing, preserve this entry's exact failure fingerprint; after the rebase and every conflict resolution, rerun the focused contract tests and complete provider-free Cargo/Bazel matrix. The current timeouts can never serve as evidence that an upstream sync succeeded.
- Reverse rollback: first revert the later F-0006 ledger commit; then revert Task 7E repair and Task 7 runtime/test and authority commits in reverse of the register above, followed by Tasks 6, 5, 4, 3, 2, and 1 in that order, reversing each listed bucket. Revert later dependents before their providers; do not keep compatibility shims, weaken exact evidence checks, delete append-only history, rewrite imported ancestry, or roll back F-0005A before all F-0006 implementation commits are gone.
