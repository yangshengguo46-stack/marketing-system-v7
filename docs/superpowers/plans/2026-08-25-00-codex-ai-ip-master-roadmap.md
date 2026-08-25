# Codex AI IP 1.1 Master Delivery Roadmap

> **Document status:** 本文件是交付路线图和门禁台账，不是可直接执行的 implementation plan。执行者只能运行 §9 明确列出的 child plan。

**Goal:** Deliver a business-first, local-first AI IP product by proving content value before building the local product shell, commercial control plane, media studio, partner system, or release infrastructure.

**Architecture:** Preserve this repository's decision history while merging the exact OpenAI Codex Git ancestor, then turn Codex App Server into the single local AI IP Business Server. Customer project truth remains encrypted on the device; a thin cloud plane owns identity, credits, provider operations, and encrypted backup only; partner and internal management remain separate products.

**Tech Stack:** OpenAI Codex commit `4ef1d4b89bd419c976b04fefa0fd36844e898340`, Rust 2024 workspace, Codex App Server v2, React/TypeScript/Vite local UI, SQLCipher and envelope-encrypted blobs, Axum/SQLx/PostgreSQL cloud services, RocketMQ, TOS, Volcengine Responses/Search/Seedream/Seedance/TTS, FFmpeg, Windows 11.

---

## 1. Authority and current repository state

This roadmap is the only active implementation ledger for AI IP 1.1. It implements the approved specification at `docs/superpowers/specs/2026-08-24-ip-agent-saas-design.md` and the retained lessons at `docs/architecture/2026-08-25-legacy-six-version-decision-memory.md`.

At roadmap authoring time, commit `58baf3b` contains documentation only:

- no Codex source tree;
- no `Cargo.toml`, `package.json`, executable test, or `AGENTS.md`;
- no configured Git remote;
- eight older implementation-plan documents based on Python/FastAPI, cloud customer projects, and the superseded DeerFlow/SaaS architecture.

Those older plans remain historical evidence and are not executable. This roadmap supersedes:

1. `archive/2026-08-24-cloud-saas-v1.3/2026-08-24-00-ip-saas-master-roadmap.md`
2. `archive/2026-08-24-cloud-saas-v1.3/2026-08-24-01-platform-foundation-ledger.md`
3. `archive/2026-08-24-cloud-saas-v1.3/2026-08-24-02-ip-content-intelligence.md`
4. `archive/2026-08-24-cloud-saas-v1.3/2026-08-24-02a-durable-provider-attempt-supplier-reconciliation.md`
5. `archive/2026-08-24-cloud-saas-v1.3/2026-08-24-03-generative-media-studio.md`
6. `archive/2026-08-24-cloud-saas-v1.3/2026-08-24-04-publication-learning-loop.md`
7. `archive/2026-08-24-cloud-saas-v1.3/2026-08-24-05-reseller-operations.md`
8. `archive/2026-08-24-cloud-saas-v1.3/2026-08-24-06-digital-human-public-beta.md`

Plan 01 is the Phase 0A program gate covering G0–G2, not one directly executable giant plan. Its normative integration contract is `docs/superpowers/specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md`. Today only the Phase 0A.1 child plan listed in §9 reaches `superpowers:writing-plans` execution depth; each later child plan is authored only after its predecessor exposes and tests the real source seams it depends on. Plans 02–15 remain frozen boundaries and entry gates, preventing speculative files or infrastructure from outrunning business proof.

## 2. Non-negotiable delivery order

```text
01 Phase 0A program: Codex source + minimum business proof (G0–G2)
  └─ PASS → 02 Mission / Lead / Artifact kernel
     └─ 03 Content capability and business evaluation (G6)
        └─ business blind-review PASS → 04 Local Business Server entry (G3)
           └─ 05 Encrypted local project runtime (G5)
              └─ 06 Volcengine internal-provider vertical (G4a)
                 ├─ internal budget → 09 Generative media studio → 10 Publication and learning loop
                 └─ commercial track → 07 Cloud identity and credit ledger
                    └─ 08 Capability operation and reconciliation (G4b + G7)
                       ├─ customer-credit activation for 09
                       └─ 13 Backup, support, and disaster recovery (G8)

07 + 08 + 10 → 11 Internal console and 12 Partner management
04–06 + 09 + 10 → 14 internal/invite Windows package gate (G9)
04–10, including 07–08 → 14 external customer Windows release gate (G4b + G9)
11, 12 → their own independent release gates in 14; neither blocks desktop
09 + applicable desktop/backup gate → 15 Digital identity and digital-human Beta
15 → six-week closed-Beta release gate
```

Hard ordering rules:

1. Plans 02–15 do not start until Plan 01 records `PASS_TO_PHASE_0B` from a new real, non-frozen business case.
2. Plans 02–03 deepen Mission/Lead/Artifact and content business ability before UI, encrypted storage, provider commercialization, SaaS, reseller operations, or customer billing.
3. Plan 06 is an internal-budget minimum provider vertical, not permission to build full SaaS, reseller operations, or customer credit billing.
4. Plans 04–15 do not start until Plan 03 passes the approved business blind-review standard. Fake and recorded providers may continue improving business capability at any time.
5. Internal and partner management products cannot block the real Mission → media → manual publication → learning path.
6. Plans 09–10 may use Plan 06's approved internal budget while customer credits remain disabled; Plans 07–08 are the later customer-paid activation gate, not a prerequisite for the internal real loop.
7. Desktop, partner, and internal products release independently. Plan 14 verifies each applicable gate and the compatibility matrix; it never waits for all three merely to ship one. An internal/invite desktop package may use Plan 06's bounded internal credential, while any external customer desktop release must additionally pass Plans 07–08 and G4b customer activation.
8. A page opening, a thread chatting, a database migrating, or a provider returning 200 is never accepted as business proof.

## 3. Plan ledger

| Plan | Independently working outcome | Entry gate | Exit evidence | Specification ownership |
|---|---|---|---|---|
| **01 — Phase 0A Codex business proof** | Pinned Codex ancestry, upstream baseline, provider-independent evaluation harness, minimum Lead instructions, structured ContentPackage, and blind comparison on one new real business | Approved v1.4 specification | `PASS_TO_PHASE_0B`, or an explicit iteration record; no product shell, cloud, or commercial infrastructure | §§1, 17.1–17.3, 18/0A, 20.1, 20.5, 21/G0–G2, 22 |
| **02 — Mission, Lead, and Artifact kernel** | Subject, Mission, dynamic MissionPlan, BusinessTask/RuntimeAttempt, Artifact graph, BusinessContext, InfluenceRelation, ActionFunnel, optional Persona, and one accountable Lead | Plan 01 PASS | Tests prove skip/backtrack/parallel execution, 0/1/N candidates, one critical follow-up, and no fixed questionnaire/three-strategy/Agent roster | §§5, 7, 8.1–8.3, 8.5, 10.1–10.3, 10.5–10.6, 18/subproject 1, 20.5 |
| **03 — Content capability and business evaluation** | Research, evidence, strategy, topics, scripts, Douyin/Xiaohongshu variants, ContentPackage, project memory, frozen scenarios, unseen cases, and human blind review | Plan 02 | Approved 70% paired-win rule and zero-offset severe-failure rules pass; before Checkpoint 03, the full frozen-plus-new-unseen suite also runs through the target Volcengine model via the isolated evaluation broker | §§3, 7.2–7.5, 8.3–8.5, 11, 17.1–17.3, 18/subproject 1, 20, 21/G6 |
| **04 — Local Business Server entry** | One Codex App Server process serves random-loopback HTTP/WS, static UI, upload/download, bootstrap session, CSRF/Origin/Host protection, single instance, and fencing; fake provider completes Mission → ContentPackage in the browser | Plan 03 PASS | Security tests plus browser vertical; no Electron, sidecar, or second C-end backend | §§6.1–6.4, 15.4, 15.6, 17.4, 21/G3 |
| **05 — Encrypted local project runtime** | Product root, SQLCipher, project DEK and AEAD blobs, immutable Artifacts, encrypted rollout/receipts, atomic commits, offline editing, crash reconstruction, recycle bin, permanent delete, and side-by-side migration | Plan 04 | Kill-process recovery and plaintext/orphan scan show no lost committed Artifact or duplicate side effect | §§6.5–6.6, 10, 16.2–16.6, 17.4, 21/G5 |
| **06 — Volcengine internal-provider vertical** | An internal evaluation credential reaches the platform Responses endpoint through the product provider seam; Responses streaming/tools/multimodal/cancel/usage and Search/Seedream/Seedance/TTS contracts are proven under an approved internal budget | Plan 05 | G4a passes: Plan 03's authorized regression suite plus a newly selected unseen case rerun without business-quality regression; customer device auth and customer-credit mode remain disabled | §§6.4, 9.2, 10.6–10.7, 13.3, 15.1–15.3, 20.4, 21/G4a |
| **07 — Cloud identity and credit ledger** | IAM, device registration, Account hierarchy, wallet, append-only ledger, PricingVersion, MissionSpendGrant, internal cost centers, audiences, and allowlists; no project-content tables | Plans 03 and 06 PASS | At least 10,000 randomized ledger sequences preserve all invariants | §§4.1–4.2, 10 cloud objects, 13, 15.4–15.5, 17.4 |
| **08 — Capability operations and reconciliation** | Local operation/outbox → cloud hold/operation/audit/outbox transaction → ProviderAttempt → signed receipt → settlement, including global ID/digest rules, 72-hour resolution, callbacks, retention, customer device token, and customer-credit activation | Plans 06 and 07 | G4b and G7 pass: customer build has no OpenAI login/BYOK bypass; point-by-point process-kill tests prove one user settlement, no duplicate effective supplier task, and no lost supplier cost | §§10.4–10.7, 13.3, 14.1, 15.1–15.3, 16.2, 16.7, 17.3–17.4, 20.4, 21/G4b/G7 |
| **09 — Generative media studio** | Seedream, Seedance, standard TTS, storyboard, Shot, optional preview, high-resolution generation, FFmpeg, captions, AI labels, QC, local delivery, and per-shot redo | Plans 03, 05, and 06; approved internal budget, customer credits disabled | One real media artifact lands locally; execution/delivery/internal-cost states remain orthogonal; product and rights tests pass. Plan 08 is required only before customer-paid activation | §§9, 10.4, 14.2–14.6, 15.3–15.4, 17.1/8, 18/subproject 3 |
| **10 — Publication and learning loop** | Immutable pre-publication Prediction, actual version, manual URL/screenshot collection, metric confirmation, comment insight, Retro, MemoryRule, and next Mission | Plans 03 and 09 | At least one real manual publication and one real data-return Retro complete | §§12, 17.5, 18/subproject 4 |
| **11 — Internal console** | Independent model registry, provider/cost reconciliation, exceptional operations, separated internal roles, compliance, release controls, and redacted diagnostics | Plans 07, 08, and closed business loop | Independent build and allowlist tests; no customer project browser or content fields | §§4.3, 6.3, 13.3, 15.1, 15.5, 17.3–17.4 |
| **12 — Partner management** | Independent offline receipts, two-level hierarchy, transfers, price permissions, refunds, device migration, support grants, and privacy-safe reports | Plans 07, 08, and 10 | Independent build/rollback and schemas proving content fields are absent | §§4.1–4.2, 6.3, 13, 18/subproject 5 |
| **13 — E2E backup, support, and DR** | Default-off ProjectBundle encryption, blocks-before-manifest upload, resume, explicit restore branch, SupportBundle, deletion receipt, and cloud recovery drill | Plans 05 and 08 | New-device checksum match; platform cannot decrypt backup or browse support-excluded content | §§4.2, 6.5, 14.1, 16.3, 16.6–16.7, 17.1/11, 21/G8 |
| **14 — Independent release gates and Windows desktop** | Signed Windows installer/update, distributable SBOM/NOTICE, provenance, N/N-1 APIs, independent desktop/partner/internal build and rollback, compatibility matrix, and upstream-sync rehearsal | Internal/invite desktop package: Plans 04–06, 09–10 and applicable recovery; external customer desktop: additionally Plans 07–08/G4b; partner/internal: their own completed plans | Each product may pass and release on its own evidence. Desktop proves Windows 11 install/restart/offline/update/readonly/rollback; an internal credential never authorizes an external customer release; partner and internal prove their own build/rollback; the compatibility matrix does not impose synchronous release | §§6.3, 15.6, 17.4, 21/G4b/G9, 22.2–22.3 |
| **15 — Digital identity and digital-human Beta** | Separate face/voice grants, withdrawal, deletion, voice clone and digital-human switches, supplier/contract/quality/label gates | Plans 09, 13, and 14 | Missing permission keeps only the high-risk feature disabled; ordinary product remains usable | §§9.3–9.4, 14.2–14.5, 18/subproject 6, 19 |

The six-week closed Beta is a release gate, not a software-plan substitute. It begins only after Plans 01–15 applicable to the enabled capability set have passed.

## 4. Upstream source decisions already verified

The exact upstream commit already contains several seams that the approved specification expected to use:

| Existing upstream seam | Exact source path | Decision |
|---|---|---|
| Rust workspace | `codex-rs/Cargo.toml` | **Preserve and extend.** New product crates join this workspace; no second Rust workspace for the C-end product. |
| App Server composition | `codex-rs/app-server/src/lib.rs`, `codex-rs/app-server/src/message_processor.rs` | **Directly modify.** This remains the single C-end composition root. |
| Transport crate | `codex-rs/app-server-transport/src/lib.rs`, `codex-rs/app-server-transport/src/transport/mod.rs` | **Directly extend.** The crate already exists; do not create a duplicate listener crate. |
| v2 protocol | `codex-rs/app-server-protocol/src/protocol/common.rs`, `codex-rs/app-server-protocol/src/protocol/v2/` | **Directly extend and regenerate.** New product RPC is v2 only. |
| Experimental generic `project/*` | `codex-rs/app-server-protocol/src/protocol/v2/project.rs`, `codex-rs/app-server/src/request_processors/projects.rs`, `codex-rs/state/src/runtime/projects.rs` | **Take over deliberately.** It currently groups workspace roots and threads; it is not the AI IP business Project. Plan 04 must replace the experimental wire meaning with `IPProject` while retaining roots/thread binding as subordinate fields or migration inputs. No second project truth is allowed. |
| State lifecycle | `codex-rs/state/src/lib.rs`, `codex-rs/state/src/paths.rs`, `codex-rs/state/src/migrations.rs` | **Extend, do not overload.** AI IP business state uses a separate `ai_ip_1.sqlite` managed by this lifecycle. |
| Auth/provider path | `codex-rs/login/`, `codex-rs/model-provider-info/`, `codex-rs/model-provider/`, `codex-rs/codex-api/` | **Replace in customer build.** Existing OpenAI/ChatGPT login, API-key, and provider override paths must become unreachable there. |
| Thread/Turn/Item and tools | `codex-rs/core/`, `codex-rs/protocol/`, `codex-rs/skills/`, `codex-rs/codex-mcp/` | **Preserve as execution substrate.** They never become Mission, Artifact, publication, or billing truth. |

Each child plan must label every touched seam as **preserve**, **directly modify**, **replace**, or **add**, and must state the business result, regression proof, and removal or rollback path.

## 5. Frozen cross-plan business contracts

### 5.1 Business authority

- One stable Lead owns each Mission and may skip, reorder, parallelize, retry, or backtrack reversible work.
- Subject kinds are `PERSON`, `BRAND`, `PRODUCT`, `ORGANIZATION`, and `HYBRID`.
- `InfluenceRelation` and `ActionFunnel` are first-class; purchase is one relation subtype, not the universal model.
- Persona, Strategy, Topic, CreativeBrief, and media are optional typed Artifacts, not a mandatory foreign-key chain.
- Candidate counts are zero, one, or many according to actual uncertainty.
- Chat, Thread, Turn, RuntimeAttempt, and provider completion never equal an accepted business Artifact.

### 5.2 Live-guild brand-IP regression

The live guild is always evaluated as a `BRAND` or `ORGANIZATION` IP whose desired result is attracting suitable creators. The required funnel vocabulary is:

```text
creator sees → trusts → applies → passes review → joins → first live → continues live
```

Plan 02 must support this relationship generically. Plan 03 owns the isolated frozen regression proving it is not transformed into the boss's personal IP, a single host's follower-growth plan, or an industry SOP. Plan 10 owns T+14/T+30 and application/review/join/first-live/continued-live measurement. The case, answers, and keywords may not enter production prompts, Skills, retrieval context, or branching code.

### 5.3 Freedom and irreversible boundaries

Default-allowed capabilities include network research, local files, Skills/MCP, scripts, Shell, FFmpeg, and authorized media generation. Ordinary uncertainty, controversy, quality problems, or reversible local locking produce provenance, readiness, warnings, and alternatives—not broad refusal.

Code hard-gates only:

- spend beyond the authorized Mission or single-call ceiling;
- actual external use of unlicensed face or voice;
- unauthorized sensitive material leaving the project;
- permanent deletion;
- actual publication or external send.

Every refusal at an irreversible boundary must return a usable alternative: anonymize, declare fiction/reconstruction, use a synthetic character, lower cost, or retain an unpublished draft.

### 5.4 Local and cloud truth

- Customer Project, Mission, Artifact, ContentPackage, rights, Prediction, Publication, Metric, and Memory truth is local and encrypted.
- Cloud truth is identity, device, entitlement, wallet, ledger, price, hold, provider operation, receipt, operation-scoped temporary objects, and client-encrypted backup metadata.
- Cloud databases, durable logs, partner management, and internal console contain no readable Prompt, project body, source person asset, or customer media library.
- All paid/model/provider work crosses `CapabilityOperation`; Skills, MCP, Shell, or browser code cannot obtain platform supplier credentials or bypass the outbox/hold path.

### 5.5 Evaluation isolation

- The five named cases are evaluation-only and never examples for the production Lead.
- Plan 01 uses a new real private business not belonging to those five classes and stores only sanitized hashes/scores in Git.
- Each durable Agent, hard semantic gate, or permanent context addition requires a held-out failure, unchanged baseline, smallest candidate, blind quality/fact comparison, token/latency/cost comparison, and deletion path.
- Capability status is reported separately as code-present, mechanically-tested, live-provider-reachable, business-blind-review-passed, and real-publication-retro-passed.

## 6. Testing and evidence rules

Every executable child plan must include:

1. a failing focused test before each behavior change; a pinned upstream seam characterization is run first and may already be GREEN, in which case it remains a test-only proof rather than forcing a fake failure;
2. the exact narrow test command and expected failure;
3. the minimum implementation;
4. the passing narrow test and relevant integration test;
5. formatting/lint commands required by the merged Codex `AGENTS.md`;
6. a commit at each coherent boundary;
7. fake/recorded provider coverage with no paid credentials in ordinary tests;
8. for any child that changes or exercises runtime behavior, an evidence manifest containing full upstream SHA, tested fork SHA, lock digests, test commands, provider mode, costs, known failures, and rollback method. A source-provenance-only child cannot embed its own final commit SHA without a hash cycle, so its exact source lock, patch ledger, clean-tree verifier/test output, and handoff statement collectively serve as the evidence record; that handoff must still state provider mode, paid cost, known failures, and rollback.

For Rust changes under `codex-rs`:

- run `just fmt` after code edits;
- use `just test -p <changed-crate>`, never direct `cargo test`;
- run `just fix -p <changed-crate>` before finalizing a large crate change;
- ask before the complete workspace `just test` run, as required by upstream `AGENTS.md`;
- update `BUILD.bazel` for new compile-time resources and run `just bazel-lock-update` when Rust dependencies change;
- v2 protocol changes run `just write-app-server-schema` and `just test -p codex-app-server-protocol`.

Business acceptance evidence is always shown separately from mechanical test evidence.

## 7. Specification coverage map

| Specification sections | Owning plans |
|---|---|
| §§1–2 product and non-goals | 01, 02, 04, 14 |
| §3 five fixed validation projects | 03; result windows in 10 |
| §4 users, privacy, and self-marketing responsibility | 07, 11, 12, 13 |
| §5 delegated product flow | 02, 03, 09, 10 |
| §6 architecture and three product faces | 04, 05, 07, 11, 12, 14 |
| §§7–8 Lead and planning reasoning | 01, 02, 03 |
| §9 content and media | 03, 09, 15 |
| §10 objects and truth boundaries | 02, 05, 07, 08, 09, 10 |
| §11 truth and marketing tension | 03, 09, 15 |
| §12 prediction and learning | 10 |
| §13 credits and partner settlement | 07, 08, 12 |
| §14 rights, privacy, and compliance | 09, 11, 13, 15 |
| §15 local product, Volcengine, and thin cloud | 04, 05, 06, 07, 08, 09, 11–14 |
| §16 recovery and audit | 05, 08, 10, 13, 14 |
| §17 test and launch standards | all authored child plans; closed-Beta gate after 15 |
| §18 phased construction | this roadmap and Plans 01–15 |
| §§19–20 deferrals and six-version guardrails | all child plans |
| §§21–22 Codex fork proof, ownership, sync, rollback | 01, 04–06, 14 |

## 8. Checkpoints

- [ ] **Checkpoint 01 / business permission:** Plan 01 records `PASS_TO_PHASE_0B`; otherwise iterate the smallest business instructions or prepare an explicit base-decision review after three logged minimal attempts. No silent DeerFlow fallback.
- [ ] **Checkpoint 03 / business moat:** Mission/Lead/Artifact kernel plus frozen and new unseen human blind review meets §17.2, including the target Volcengine model through the isolated evaluation broker. Only then may product shell, encrypted persistence, provider commercialization, or complete commercial infrastructure begin.
- [ ] **Checkpoint 06 / product foundation:** local entry, encrypted truth, and G4a Volcengine internal vertical work together; held-out business quality has not regressed. This authorizes only bounded internal/invite use.
- [ ] **Checkpoint 08 / paid-call truth:** G4b customer device-token/customer-credit activation passes provider compatibility, transaction, idempotency, crash-window, supplier-statement, and retention proof; the customer build has no OpenAI login/BYOK bypass.
- [ ] **Checkpoint 10 / real loop:** one new real project reaches manual publication, data return, and next-Mission learning without structural rework.
- [ ] **Checkpoint 14 / independent release engineering:** each release candidate passes only its applicable gate. Internal/invite Windows packaging may proceed after its bounded internal-provider gates; external customer Windows release additionally requires Plans 07–08/G4b. Desktop Windows is not blocked by partner/internal readiness; partner and internal are not blocked by desktop cadence. The shared compatibility matrix and N/N-1 contracts must pass for the products actually combined.
- [ ] **Checkpoint 15 / closed Beta:** at least four non-frozen customer projects plus the separate platform self-marketing project run six weeks; at least three customer projects meet their preregistered meaningful-result thresholds without severe failures.

At every checkpoint, stop for a user-readable report: business outcome, working capabilities, stubbed capabilities, tests, paid cost, data location, known risk, rollback, and explicit continue/correct decision.

## 9. Current executable child plan

Phase 0A's non-executable build and acceptance specification:

`docs/superpowers/specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md`

Execute only:

`docs/superpowers/plans/2026-08-25-01a-codex-fork-provenance.md`

Phase 0A.1 only establishes the verifiable source/fork boundary. It does not close G0–G2, produce `PASS_TO_PHASE_0B`, or authorize Plan 02. The remaining Work Packages in the build specification must each be converted into a new executable child plan with `superpowers:writing-plans` after the preceding child exposes real files, tests, and failure modes. Do not infer implementation details from the superseded 2026-08-24 documents.
