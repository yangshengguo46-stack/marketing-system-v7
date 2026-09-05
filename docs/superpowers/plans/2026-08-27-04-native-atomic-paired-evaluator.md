> **RETIRED — RETIRED_NOT_PASSED (2026-09-05).** This evaluator/proof document is historical and is not executable. All commands, prerequisite gates and successor authorizations below are retired by the approved business-first cleanup; prior results and failures remain historical claims only. Do not resume Phase 0A, 06A, 06B, 07A, 07B or LH1 from this record. Retirement grants no business PASS, model qualification, spending, publication, customer-data access or customer-release authority. Current authority: `docs/superpowers/specs/2026-09-05-business-first-cleanup-design.md` and `docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md`.

# Phase 0A Work Package 5 — Native Atomic Paired Evaluator

> **Execution discipline:** Follow the approved superpowers test-driven, SDD, independent-review, and verification-before-completion practices; preserve one independently reviewable GREEN commit per task and stop on any scope or evidence mismatch.

**Goal:** Turn the pinned Codex Responses proxy and App Server into a replayable, mock-testable, atomic two-arm evaluator that compares the same Mission with and without the removable Lead Skill while accounting for every provider attempt and every descendant thread.

**Authority:** `docs/superpowers/specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md`, Work Package 5. Supporting authorities are `docs/superpowers/specs/2026-08-24-ip-agent-saas-design.md`, `docs/superpowers/specs/2026-08-25-domestic-model-adaptation-design.md`, `docs/architecture/2026-08-25-legacy-six-version-decision-memory.md`, `docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md`, `docs/superpowers/plans/2026-08-26-03-native-lead-skill-runtime.md`, and the pinned Codex source at this child base.

**Base:** `de86eb6938a88b348e28884346dcf1ab0c7cafad`

**Provider boundary:** This child implements replay and mock-upstream behavior only. `providerMode=not-run`; `paidProviderCost=0`; do not locate, read, print, transform, or transmit an API key. The later approved real-pair execution is a separate Work Package 8 action with a frozen attestation and budget.

## Why this is business-enabling rather than a fixed business workflow

- The evaluator freezes experimental parity, not creative method. It neither adds a questionnaire nor prescribes topic, script structure, agent roster, CTA, publication flow, or number of strategies.
- Generic and candidate arms receive the same bounded Mission, materials, prompt, additional context, output Schema, model route, limits, and effective config. The only intended treatment difference is availability and verified use of `deliver-ai-ip-content-package`.
- One Lead still owns the result and may use tools or create descendants only when useful. Tree accounting observes those choices; it does not require them.
- The output remains the existing `ContentPackage`. WP5 does not add an automatic factual guard, Evidence Store persistence, knowledge-base UI, publishing state, or product shell.
- Replay proves evaluator mechanics only. Mock Responses proves wire behavior only. Neither is factual-fidelity acceptance, business acceptance, live-model quality, G2, or `PASS_TO_PHASE_0B`.

## Global constraints

- Keep the four commits below separate and reviewable; do not squash them into one giant change.
- Do not modify `codex-core`. Modify `thread_processor.rs` only if the focused pinned App Server characterization reproduces the listener race described by the authority spec.
- New Rust test modules live in sibling `*_tests.rs` files. Public traits include role, call-order, concurrency, and body-retention documentation.
- No observer, receipt, log, dump, or public evidence may receive request/response bodies, API keys, private source material, or final package text. Tests use synthetic fixtures only.
- All injected context and request bodies are bounded. The proxy rejects an oversized or malformed JSON body before forwarding.
- No contract test depends on cwd, `target/` accidents, user Home, current credentials, network access, or a real provider. Resolve fixtures and the Skill through Bazel-compatible runfiles.
- Use exact typed App Server protocol structures and deep equality. Do not duplicate a shadow wire protocol or scrape debug output.
- Preserve the current CLI's `--server-info`, `--http-shutdown`, `--dump-dir`, stdin credential path, and default forwarding behavior.
- Rust verification uses `just test`, never direct `cargo test`. Ask before a complete workspace test if a shared crate change requires one. Do not rerun tests after final `just fmt` / scoped `just fix` for a task.
- A valid RED is a contract failure caused by the not-yet-implemented seam. Dependency installation, unrelated compilation failures, stale locks, or deliberately breaking an existing seam are not valid RED evidence.

## Task 1 — Extract the Responses proxy into a gateable library service

**Files:**

- Modify: `codex-rs/responses-api-proxy/src/lib.rs`
- Create: `codex-rs/responses-api-proxy/src/broker.rs`
- Create: `codex-rs/responses-api-proxy/src/broker_tests.rs`
- Modify only as required by the frozen credential lifecycle: `codex-rs/responses-api-proxy/src/read_api_key.rs`
- Modify: `codex-rs/responses-api-proxy/Cargo.toml`
- Modify mechanically: `codex-rs/responses-api-proxy/BUILD.bazel`, `codex-rs/Cargo.lock`, `MODULE.bazel.lock`

### RED

Add sibling tests for the exact Contract 3 behaviors:

1. only `POST /v1/responses` may reach upstream;
2. caller Authorization and Host are removed before the locked bearer and computed Host are injected;
3. gate rejection performs zero upstream requests;
4. upstream failure records an attempt but no completion;
5. SSE observation extracts only `response.completed` ID, usage, model, and deployment/fingerprint metadata and retains no event body;
6. transform accepts a bounded JSON object, rejects caller-provided/non-integer `max_output_tokens`, inserts the frozen `u64`, rebuilds Content-Length, and exposes only digest/length/typed evidence;
7. redirects are disabled, permit deadlines set a per-request timeout, shutdown waits boundedly for in-flight work, and `wait()` does not initiate shutdown;
8. the legacy CLI flags and stdin credential behavior remain covered.

Run:

```bash
just test -p codex-responses-api-proxy
```

Expected: contract tests fail because the library service, traits, and transform do not exist.

### GREEN

Implement only the frozen Contract 4 surface in `broker.rs`: `ProxyConfig`, bounded `RequestTransformConfig`, body-free request/response metadata, documented `RequestInspector`, `RequestGate`, `ExchangeObserver`, `RequestPermit`, `ForwardResult`, `RunningProxy`, `bind`, and `activate`.

Keep `run_main` as an adapter over this service. The legacy CLI may use no-op gate/observer implementations and no transform. It must continue reading the credential exactly once through the existing hardened stdin path.

Verification and first GREEN commit:

```bash
just test -p codex-responses-api-proxy
just bazel-lock-update
bazel test //codex-rs/responses-api-proxy:responses-api-proxy-unit-tests //codex-rs/responses-api-proxy:codex-responses-api-proxy-bin-unit-tests
just bazel-lock-check
just fmt
just fix -p codex-responses-api-proxy
git add codex-rs/responses-api-proxy codex-rs/Cargo.lock MODULE.bazel.lock
git commit -m "refactor: expose gateable Responses proxy"
```

Do not run tests after `fmt`/`fix`. Inspect the staged diff before commit and verify no credential or body entered an observer/log fixture.

## Task 2 — Add the replayable typed evaluator, config audit, and tree collector

**Files:**

- Create: `codex-rs/ai-ip-eval/Cargo.toml`
- Create: `codex-rs/ai-ip-eval/BUILD.bazel`
- Create: `codex-rs/ai-ip-eval/src/{lib.rs,main.rs,model.rs,app_server.rs,catalog.rs,evidence.rs,eval_tests.rs}`
- Create: `codex-rs/ai-ip-eval/tests/fixtures/{replay-fixture-set.json,replay-case.json,replay-transcript.jsonl,replay-attestation.json,replay-review-1.json,replay-review-2.json,replay-review-3.json}`
- Modify: `codex-rs/Cargo.toml`
- Modify mechanically: `codex-rs/Cargo.lock`, `MODULE.bazel.lock`

### RED

Register the crate and write the frozen Contract 1/2/5/6/7 tests before implementation. At minimum they prove:

- every replay line deserializes as pinned `ServerNotification`; final JSON deserializes and validates as the real `ContentPackage`;
- usage deduplicates by `(threadId, turnId, responseId)`, rejects conflicting duplicates, null/negative/internally inconsistent/overflow values, unknown threads, missing terminals, multiple finals, and item/turn text mismatch;
- `ModeEvidence` makes Replay and Live disjoint; Replay cannot carry approval/provider/budget fields and cannot be promoted to G2;
- `ProofBrokerCompatibilityName` serializes only as exact `"OpenAI"` and cannot replace actual provider identity or provider role;
- typed `ThreadStartParams` / `TurnStartParams` requests deep-equal their frozen projections, including one byte-identical untrusted additional-context entry and the strict output Schema;
- config audit rejects profiles, auth files, MCP/plugins, extra instructions, secrets, retries, websocket transport, unsafe permission drift, and Guardian enablement;
- catalog normalization proves generic has no target Skill, candidate has exactly one canonical target Skill, and removing it yields the same normalized base catalog;
- candidate use evidence requires a successful local read of the canonical complete `SKILL.md` before the final package;
- root, child, grandchild, and sibling are discovered through paginated typed reads; every turn reaches terminal; two complete quiet scans match; raw App Server and broker completions match one-to-one with equal usage;
- replay case-boundary tests create their own temporary Git repository and do not treat committed fixtures as private live evidence.

Run:

```bash
just test -p codex-ai-ip-eval
```

Expected: failure because the model, typed client, config audit, catalog normalizer, and collector are unresolved.

### GREEN

Implement the minimum replay path and typed in-memory collector. Keep live-only fields structurally present but unreachable from replay. Preserve the same pre-main hardening as the proxy binary. Do not add the live-pair coordinator or broker state machine in this task.

Verification and second GREEN commit:

```bash
just bazel-lock-update
just test -p codex-ai-ip-eval
bazel test //codex-rs/ai-ip-eval:ai-ip-eval-unit-tests //codex-rs/ai-ip-eval:codex-ai-ip-eval-bin-unit-tests
just bazel-lock-check
just fmt
just fix -p codex-ai-ip-eval
git add codex-rs/Cargo.toml codex-rs/Cargo.lock codex-rs/ai-ip-eval MODULE.bazel.lock
git commit -m "test: add typed App Server evaluation client"
```

## Task 3 — Characterize the pinned App Server descendant and strict-output seams

**Files:**

- Create: `codex-rs/app-server/tests/suite/v2/raw_response_subagents.rs`
- Create: `codex-rs/app-server/tests/suite/v2/ai_ip_strict_output.rs`
- Modify: `codex-rs/app-server/tests/suite/v2/mod.rs`
- Modify mechanically: `codex-rs/app-server/Cargo.toml`, `codex-rs/app-server/BUILD.bazel`, `codex-rs/Cargo.lock`, `MODULE.bazel.lock`
- Conditional only after a valid seam RED: `codex-rs/app-server/src/request_processors/thread_processor.rs`
- Conditional with a production seam change: `docs/architecture/codex-fork-patch-ledger.md`

### Characterization first

Use the existing `TestAppServer` and mock Responses server. The descendant test creates root → child → grandchild plus a root sibling, gives each thread a distinct mocked `response.completed` usage, and proves the external connection receives every typed raw completion and parent edge before the root terminal. The strict-output test sends the real Work Package 4 Schema through V2 `turn/start`, inspects the actual provider-visible request, deep-compares the Schema, and proves `strict=true`.

Run only the two focused tests first:

```bash
just test -p codex-app-server --test all -E 'test(=suite::v2::raw_response_subagents::raw_events_cover_root_child_grandchild_and_sibling)'
just test -p codex-app-server --test all -E 'test(=suite::v2::ai_ip_strict_output::turn_start_sends_strict_content_package_schema)'
```

If both are GREEN, record a test-only seam and do not alter production App Server code. If and only if a test reproduces descendant listener attachment loss, parent-edge drift, or strict wire drift, preserve that RED and make the smallest repair. Test registration or dependency failure is not a valid production-seam RED.

Verification and third GREEN commit:

```bash
just bazel-lock-update
just test -p codex-app-server --test all -E 'test(=suite::v2::raw_response_subagents::raw_events_cover_root_child_grandchild_and_sibling)'
just test -p codex-app-server --test all -E 'test(=suite::v2::ai_ip_strict_output::turn_start_sends_strict_content_package_schema)'
if ! git diff --quiet -- codex-rs/app-server/src/request_processors/thread_processor.rs; then just test -p codex-app-server; fi
bazel test //codex-rs/app-server:app-server-all-test
just bazel-lock-check
just fmt
just fix -p codex-app-server
git add codex-rs/Cargo.lock MODULE.bazel.lock codex-rs/app-server/Cargo.toml codex-rs/app-server/BUILD.bazel codex-rs/app-server/tests/suite/v2/mod.rs codex-rs/app-server/tests/suite/v2/raw_response_subagents.rs codex-rs/app-server/tests/suite/v2/ai_ip_strict_output.rs codex-rs/app-server/src/request_processors/thread_processor.rs docs/architecture/codex-fork-patch-ledger.md
git commit -m "test: cover descendant raw response usage"
```

## Task 4 — Add the atomic two-arm coordinator and append-only broker ledger

**Files:**

- Create: `codex-rs/ai-ip-eval/src/{broker_gate.rs,runner.rs}`
- Modify: `codex-rs/ai-ip-eval/src/{lib.rs,main.rs,model.rs,evidence.rs,eval_tests.rs}`
- Modify mechanically: `codex-rs/ai-ip-eval/BUILD.bazel`, `codex-rs/Cargo.lock`, `MODULE.bazel.lock`
- Modify only if frozen public surfaces require build wiring: `codex-rs/responses-api-proxy/BUILD.bazel`
- Append only when a direct fork seam changes: `docs/architecture/codex-fork-patch-ledger.md`

**Narrow review-closure authority amendment (2026-08-27):** Task 4 may also modify only
the `#[cfg(test)]` modules `codex-rs/ai-ip-eval/src/app_server_tests.rs` and
`codex-rs/ai-ip-eval/src/tree_tests.rs` for the directly related Darwin retained-image,
canonical-protocol, and quiet-tree REDs. This exception grants no production App Server
authority and does not broaden Task 4 beyond those evaluator tests.

### RED

Add mock-only tests for Contracts 9–12:

- state transitions are exactly `Created → OrderCommitted → Active1 → Sealed1 → Active2 → Finished`; any early, inter-arm, late, duplicate-condition, or third-arm request poisons the pair;
- OS CSPRNG chooses the arm order after context freeze; live does not accept a caller seed/order;
- only the active root or a descendant of a known thread may forward; malformed IDs, Guardian lifecycle, parentless/unknown descendants, attempts over cap, expired deadlines, or non-Responses paths poison before upstream;
- every pre-forward request record is append + fsync before network; exactly one terminal record binds it; indices are gap-free and receipts chain arm 2 to arm 1;
- `max_output_tokens` is a real transformed upstream field and not only a manifest promise;
- shared config bytes are generated once and copied; both isolated Homes are owner-only, contain no credential, and differ only by the target Skill asset;
- child processes use an allowlisted `env_clear()` environment and cannot read a credential sentinel or inherited key/token/secret/proxy variables;
- `freeze-run-context` uses disjoint typed Replay/Live subcommands; `live-pair` accepts only the frozen context and never exposes a single live arm;
- source, materials, binaries, config, Schema, prompt, Skill, request projections, attempt index, receipts, and worktree cleanliness are rehashed at the required boundaries;
- every network test uses a local mock upstream; upstream request count remains zero after any gate rejection.

### GREEN

Implement only the frozen CLI surface and mock-testable coordinator. The code may read a bearer only inside `live-pair` after bind-without-accept, frozen-context revalidation, Home/config/catalog preflight, arm-order commitment, and execution-context fsync. This child must not execute that live path or access a real secret.

Verification and fourth GREEN commit:

```bash
just test -p codex-ai-ip-eval
just test -p codex-responses-api-proxy
just bazel-lock-update
bazel test //codex-rs/ai-ip-eval:ai-ip-eval-unit-tests //codex-rs/ai-ip-eval:codex-ai-ip-eval-bin-unit-tests
just bazel-lock-check
just fmt
just fix -p codex-ai-ip-eval
git add codex-rs/Cargo.toml codex-rs/Cargo.lock codex-rs/ai-ip-eval MODULE.bazel.lock docs/architecture/codex-fork-patch-ledger.md
git commit -m "test: add atomic Codex AI IP paired runner"
```

## Final child verification and review

After Task 4's commit, run no provider and read no secret. Inspect the complete diff from this child base and verify:

```bash
git diff --check de86eb6938a88b348e28884346dcf1ab0c7cafad..HEAD
git status --porcelain=v1 --untracked-files=all
rg -n 'PASS_TO_PHASE_0B|factual[- ]fidelity PASS|business acceptance|providerMode=live' codex-rs/ai-ip-eval codex-rs/responses-api-proxy codex-rs/app-server/tests/suite/v2
```

The review must report separately:

1. proxy behavior and credential/body-retention safety;
2. typed protocol/config/catalog/tree accounting correctness;
3. whether App Server production code changed and the exact valid RED that authorized it;
4. atomic two-arm state/ledger/receipt correctness;
5. scoped tests and Bazel results;
6. `providerMode=not-run`, paid cost `0`, and absence of API-key access;
7. remaining blockers: Work Package 6 source-visible human review, Work Package 8 approved live pair, G0–G2, `PASS_TO_PHASE_0B`, and product Plan 02.

## Stop conditions

Stop and amend this child before continuing if any of the following occurs:

- a required pinned App Server or proxy seam differs materially from the frozen authority;
- a change to `codex-core`, product UI, persistence, provider gateway, billing, publication, or automatic semantic guard appears necessary;
- a test would need a real API key, external network, paid provider, private customer material, or current user Home;
- request/response bodies or private material would have to enter an observer, receipt, public report, or committed fixture;
- one-arm live execution, caller-chosen arm order, incomplete descendant accounting, or non-atomic receipt production would become possible;
- the four-commit boundary can no longer remain independently reviewable.

At a stop, preserve the valid evidence, report the smallest blocking seam, and do not claim WP5, G2, Phase 0B, or business proof.
