# AI IP SaaS Master Delivery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the approved AI IP content and generative-production SaaS through six independently testable subprojects without turning the system into a single unreviewable build.

**Architecture:** Use one modular Python monolith for transactional business rules, separate asynchronous worker entrypoints for Agent and media jobs, and one Next.js web application with C-user, reseller, and platform routes. PostgreSQL remains the source of truth; Redis, RocketMQ, TOS, and Volcengine model services are adapters behind explicit ports so tests run with deterministic fakes.

**Tech Stack:** Python 3.12, FastAPI 0.136.x, Pydantic 2.x, SQLAlchemy 2.0.x, Alembic, PostgreSQL, Redis, RocketMQ, TOS, FFmpeg, Pytest, Node.js 24, pnpm 10, Next.js 16 App Router, React 19, TypeScript 5.x, Playwright, VKE, VCI, APIG, TLS, CDN, VMP.

---

## 1. Why this is a plan set

The approved specification contains six systems with different failure modes. They must not be implemented as one giant branch or one giant checklist. This roadmap freezes their shared contracts; the following six plans contain the executable TDD steps:

1. `2026-08-24-01-platform-foundation-ledger.md`
2. `2026-08-24-02-ip-content-intelligence.md`
3. `2026-08-24-03-generative-media-studio.md`
4. `2026-08-24-04-publication-learning-loop.md`
5. `2026-08-24-05-reseller-operations.md`
6. `2026-08-24-06-digital-human-public-beta.md`

Each plan must pass its own tests with fake external providers. Real Volcengine credentials are introduced only in controlled integration and PoC stages; no unit or ordinary integration test may require a paid model call.

## 2. Fixed repository structure

```text
ip-agent-saas/
├── .env.example                         # documented local configuration only
├── .github/workflows/ci.yml             # lint, type, unit, integration and web checks
├── Makefile                             # stable developer commands
├── README.md                            # local setup and plan execution order
├── docker-compose.yml                   # local PostgreSQL, Redis, RocketMQ and MinIO-compatible storage
├── contracts/
│   ├── openapi.json                     # exported FastAPI contract
│   └── events/                          # versioned JSON event schemas
├── backend/
│   ├── pyproject.toml                   # locked Python dependency and tool configuration
│   ├── alembic.ini
│   ├── migrations/                      # forward and downgrade database migrations
│   ├── src/ip_saas/
│   │   ├── api.py                       # FastAPI composition root
│   │   ├── config.py                    # validated environment configuration
│   │   ├── db/                          # session, base, transaction and migration helpers
│   │   ├── common/                      # IDs, clocks, errors, idempotency, audit and outbox
│   │   ├── modules/
│   │   │   ├── accounts/                # principals, roles, sessions and tenancy
│   │   │   ├── projects/                # IP projects, profiles and owner boundaries
│   │   │   ├── billing/                 # credit and internal-cost ledgers
│   │   │   ├── intelligence/            # five questions through platform variants
│   │   │   ├── media/                   # character, shot, provider and QC workflows
│   │   │   ├── publication/             # prediction, metrics, comments, retro and memory
│   │   │   ├── resellers/               # fixed reseller tree, offline orders and support grants
│   │   │   └── governance/              # consent, labels, retention, incidents and launch gates
│   │   ├── providers/                    # Volcengine, TOS, RocketMQ and local fake adapters
│   │   └── workers/                      # independently runnable worker entrypoints
│   └── tests/
│       ├── unit/                         # pure domain behavior
│       ├── integration/                  # PostgreSQL and API behavior
│       ├── contract/                     # provider and event schemas
│       ├── fixtures/                     # five fixed project cases and fake provider responses
│       └── security/                     # tenancy, prompt-injection and permission cases
├── frontend/
│   ├── package.json
│   ├── playwright.config.ts
│   ├── src/app/                          # Next.js route groups
│   ├── src/features/                     # feature-owned UI and state
│   ├── src/lib/api/                      # generated types and one API client
│   └── tests/e2e/                        # critical user journeys
└── infra/
    ├── docker/                           # production container definitions
    ├── k8s/                              # VKE workloads, workers, jobs and policies
    ├── apig/                             # route, authentication and rate-limit declarations
    ├── observability/                    # VMP metrics and alert rules
    └── runbooks/                         # deploy, rollback, incident and recovery procedures
```

Files are split by business responsibility, not by controllers/models/services as three global layers. A module owns its domain, API router, repository, schemas and tests; other modules call its public application service or consume its versioned event.

## 3. Frozen cross-plan rules

### 3.1 Identifiers, time and versions

- Persist UUID values as PostgreSQL `uuid`; generate UUID4 in application code.
- Persist all timestamps as timezone-aware UTC and render Asia/Shanghai only at the presentation boundary.
- Immutable business artifacts use `version_no`, `created_at`, `created_by` and `supersedes_id`.
- Frozen strategy, persona, prediction and publication records are never updated in place.
- API payloads expose lowercase string enums; database migrations create check constraints for critical state values.

### 3.2 Tenant and owner boundary

Every private row must resolve to exactly one tenant boundary through `account_id` or `project_id`. `IPProject.owner_type` is `c_user` or `platform`; a platform-owned project cannot enter a reseller tree, and a C-user project cannot become platform-owned.

Repository methods accept an explicit `ActorContext`; they do not query by object ID alone:

```python
from dataclasses import dataclass
from enum import StrEnum
from uuid import UUID


class ActorKind(StrEnum):
    PLATFORM_ADMIN = "platform_admin"
    PLATFORM_OPERATOR = "platform_operator"
    PLATFORM_REVIEWER = "platform_reviewer"
    RESELLER_L1 = "reseller_l1"
    RESELLER_L2 = "reseller_l2"
    C_USER = "c_user"


@dataclass(frozen=True)
class ActorContext:
    actor_id: UUID
    account_id: UUID
    kind: ActorKind
```

The absence of `ActorContext` is a programming error except inside migrations and background jobs. Background jobs use a persisted `initiated_by_actor_id` and a constrained system-worker role.

### 3.3 Two money systems, never one polymorphic balance

Customer and reseller work uses integer `credit_units`. Platform self-marketing uses integer CNY `amount_fen`. They have separate ledgers and separate holds.

```python
from dataclasses import dataclass
from enum import StrEnum
from uuid import UUID


class BillingMode(StrEnum):
    CUSTOMER_CREDIT = "customer_credit"
    INTERNAL_COST = "internal_cost"


@dataclass(frozen=True)
class BillingContext:
    mode: BillingMode
    hold_id: UUID

    def __post_init__(self) -> None:
        if self.hold_id.int == 0:
            raise ValueError("hold_id must be non-zero")
```

A generation job references one `BillingContext`. It must never reference both `GenerationHold` and `InternalBudgetHold`. Ledger entries are append-only, transfers are double-entry, and retries use account-scoped client idempotency keys plus server-generated globally unique event keys; one customer's client key can never block another customer.

Plan 01 freezes a required generation-limit port in the `BillingService` constructor and supplies a production `UnconfiguredGenerationLimits` adapter that denies all reservations until commercial limits exist. There is no permissive default. Focused tests must pass an explicitly named allow fake. Plan 05 later replaces the deny adapter in every production API/Worker composition with its versioned per-task/day/month `GenerationLimitService`; integration and acceptance fixtures then create actual customer-account or internal-cost-center limit versions. A repository contract test rejects any production construction that omits the dependency.

### 3.4 Transaction and asynchronous boundary

The request transaction writes domain changes, the billing hold, the generation task and an outbox record together. A dispatcher publishes the outbox record to RocketMQ. Workers may receive messages repeatedly or out of order, so state transitions use compare-and-set rules and provider callbacks use `(provider, provider_task_id)` uniqueness.

```python
from datetime import datetime
from typing import Any, Literal
from uuid import UUID
from pydantic import BaseModel, ConfigDict


class EventEnvelope(BaseModel):
    model_config = ConfigDict(extra="forbid")

    event_id: UUID
    event_type: str
    schema_version: Literal[1]
    aggregate_id: UUID
    occurred_at: datetime
    initiated_by_actor_id: UUID
    idempotency_key: str
    payload: dict[str, Any]
```

Every event schema is exported to `contracts/events/<event_type>.v1.json` and verified by contract tests before a producer or consumer is merged.

`TaskRecord` is the only durable top-level work record. It carries `attempt_no` and `lease_expires_at`; `start(session, task_id, lease_seconds=300, max_attempts=8)` atomically claims queued work or an expired lease and increments the attempt number. A terminal replay returns the unchanged task and is acknowledged, an active lease is retried later without acknowledgement, and an expired lease may be reclaimed. `heartbeat`, `succeed`, and `fail` all compare `status=running`, the captured `attempt_no`, and `lease_expires_at > now`; a stale or already-expired worker may not renew itself, call a provider, write domain state, change billing, publish a delivery asset, or acknowledge success.

Retry exhaustion is not proof that no supplier call occurred. When the ceiling is reached before usage is proven, `start` moves the task to durable non-terminal `reconciliation_required`, clears the lease, preserves the original active hold, and performs no release or failure write. Every feature handler returns `RETRY` without provider/domain/billing work for this state. An independent reconciliation scanner may finalize it only through Plan 01's `TaskReconciliationService`: a signed complete zero-call proof releases the hold, or a complete supplier request manifest plus every supplier-matched `(provider, provider_request_id)` cost atomically inserts all task-linked cost rows, charges customer actual zero or internal actual total CNY once, and then marks the task failed. Exact replay is idempotent; incomplete, duplicate, stale, or changed evidence remains blocked.

The RocketMQ consumer acknowledges only after its handler has committed the durable outcome. Unknown event versions and handler failures are not acknowledged and therefore follow broker retry/dead-letter policy. Business truth and retry exhaustion stay in PostgreSQL; RocketMQ transports the frozen envelope but never becomes a second task-state database.

Every fake or real provider completion that reaches settlement records `ProviderCostInput.task_id` equal to the current `TaskRecord.id` and preserves a canonical 1–200-character `provider_request_id`; neither lineage field may be `None`. Multiple completions may aggregate the customer/internal charge calculation but never collapse distinct supplier request identities into one cost row. Success settles the original hold, records every provider cost, and succeeds the task in one transaction. Terminal failure first proves ownership of the exact unexpired attempt by row lock or compare-and-set. A proven zero-call outcome constructs neither `ProviderCostInput` nor `ProviderCostEntry`; it releases the hold and fails the task in one transaction only through the central zero-call evidence gate. If provider usage was incurred before parsing, persistence, validation, QC, or downstream work failed, the same transaction must preserve every task-linked supplier-cost row: customer work settles at `actual_amount=0` and releases the unused customer hold while the platform absorbs supplier spend; platform-internal work settles the actually incurred CNY amount against its internal hold. Only then may the task become failed. A stale attempt may perform none of these writes. The code may perform the fenced task write before release or settlement inside that uncommitted transaction when this is necessary to prevent a stale worker from changing a newer attempt's hold. All settlement and cleanup paths are idempotent.

Database anti-joins prove lineage only for completions the application observed; they cannot prove that a process did not die after a paid HTTP response but before cost persistence. Before any real synchronous provider adapter is enabled, a production-account fault PoC must kill the Worker in that exact window and reconcile the provider request transcript or supplier statement against durable client/request-attempt IDs and `ProviderCostEntry.task_id`. Activation requires documented and observed provider lookup/idempotency or an implemented supplier-statement reconciliation path that exposes every orphan as an exception. If neither proof exists, that capability remains disabled; a fake-provider test or a hard budget cap does not waive this gate.

### 3.5 Provider boundary

Business modules depend on provider protocols, never Volcengine SDK response dictionaries. Each real adapter has a deterministic fake with the same contract. Provider results preserve model ID, model version, task ID, native usage, temporary URL expiry and raw error category. Secrets never enter database fields, logs, fixtures or browser code.

### 3.6 Media storage boundary

Provider URLs are temporary. A successful provider task transitions to `succeeded_pending_copy`; the storage worker streams the object into private TOS, calculates SHA-256, stores metadata and only then moves the job to `copied_pending_qc`. User delivery uses short-lived signed URLs issued after authorization.

### 3.7 Content lineage

Every `TopicCard`, `MarketingFrame`, `ContentVersion`, `PlatformVariant`, `Prediction`, `Publication` and `Retro` stores the exact upstream IDs it used. A later persona or strategy revision creates a new branch; it never rewrites historical content lineage.

### 3.8 Self-marketing strategy rule

The platform self-marketing project creates three project-level strategy candidates once. Every candidate must map to L1 reseller, L2 reseller and C-user tracks. The platform operator chooses one total IP strategy, then freezes one `AudienceTrackPlanVersion` per track. A track requiring a different identity or account promise becomes a separate platform IP project.

### 3.9 Frozen foundation import paths

All feature plans use these Plan 01 contracts without renaming or parallel replacements:

```text
ip_saas.db.session.get_session
ip_saas.modules.accounts.dependencies.get_actor
ip_saas.modules.accounts.context.ActorContext
ip_saas.modules.projects.access.ProjectAccessService
ip_saas.common.audit.AuditWriter
ip_saas.common.outbox.OutboxWriter
ip_saas.common.outbox.EventEnvelope
ip_saas.modules.billing.service.BillingService
ip_saas.modules.billing.service.BillingContext
ip_saas.modules.billing.service.BillingMode
ip_saas.modules.billing.service.ProviderCostInput
ip_saas.modules.billing.ports.GenerationLimitPort
ip_saas.modules.billing.limits.UnconfiguredGenerationLimits
ip_saas.modules.tasks.models.TaskRecord
ip_saas.modules.tasks.service.TaskSubmissionService
ip_saas.modules.tasks.reconciliation.TaskReconciliationService
```

Every paid or potentially long-running Agent/media operation calls `TaskSubmissionService.submit_customer(...)` or `submit_internal(...)`; feature modules do not create competing top-level task, hold or outbox truth. Workers reconstruct billing from `TaskRecord.billing_mode` and `TaskRecord.billing_hold_id`, then use `start`, `succeed` or `fail` compare-and-set transitions.

### 3.10 One browser API boundary

`frontend/src/lib/api/schema.d.ts` is the only generated OpenAPI type file. Feature code passes backend paths beginning with `/v1/` to Plan 01's shared `apiRequest`; that client alone adds the browser `/api` reverse-proxy prefix. A feature may use direct `fetch` against the application only for a non-JSON stream/upload that the shared client intentionally does not support; that request applies `/api` exactly once and reuses `requireAccessToken()`. A direct upload to a server-issued pre-signed TOS URL uses that external URL unchanged and must not prepend `/api` or attach the application's bearer token. No plan creates `generated.ts`, a second JSON client, or a browser request to a bare backend `/v1` URL.

## 4. Baseline developer commands

All detailed plans must preserve these commands:

```bash
make bootstrap          # install Python and Node dependencies without paid calls
make lint               # Ruff, mypy, ESLint and TypeScript checks
make test-unit          # pure Python and frontend unit tests
make test-integration   # PostgreSQL/API tests with local adapters
make test-e2e           # Playwright critical journeys
make test               # all non-PoC checks
make export-contracts   # OpenAPI and event JSON schemas; fails on uncommitted drift
```

`make test` must not require Volcengine, TOS, SMS, platform publishing or payment credentials. Live provider checks use explicit `make poc-*` targets and a production-like test account with budget caps.

## 5. Dependency and execution order

```text
01 Platform foundation and trusted ledgers
├── 02 IP content intelligence ─→ 03 Generative media studio ─→ 04 Publication and learning loop
└── 05 Reseller operations ────────────────────────────────────┐
                                                               ├─→ 06 Digital-human Beta and public-launch gates
03 Generative media studio ────────────────────────────────────┤
04 Publication and learning loop ──────────────────────────────┘
```

Plans 02 and 05 domain work may begin after Plan 01 freezes database, auth, audit, billing and event contracts. Merge order is nevertheless linear because the frozen Alembic chain is `0001_foundation → 0002_intelligence → 0003_media → 0004_publication → 0005_resellers → 0006_governance`: Plan 03 cannot merge until Plan 02; Plan 04 cannot merge until Plan 03; and Plan 05's integration/migration branch cannot merge until Plan 04 even if its pure domain work was prepared earlier. Plans 02–04 use Plan 01's explicit test-only limit fake and the production deny adapter. When Plan 05 merges, it installs real versioned limits and must rerun Plans 01–04 billing/task/Agent/media/publication suites with explicit effective limit versions. Plan 06 begins only after Plans 01–05 pass their acceptance gates. The Plan 02a durable-provider reconciliation addendum is not a seventh product subproject; it is mandatory release hardening inside the Plan 02 boundary and must be reviewed, deployed, and exercised before Checkpoint 6 may enable real Ark Responses or Web Search.

## 6. Plan-level acceptance gates

### Plan 01 — Platform foundation and trusted ledgers

- Authentication, fixed account ownership, C-user multi-project isolation and platform internal roles work.
- Credit and internal-cost ledgers are separate, append-only and reconstruct balances exactly.
- 10,000 randomized transaction sequences preserve non-negative balances and double-entry equality.
- Outbox, idempotency, audit, model registry, task state and private-object adapters have integration tests.
- Expired leases cannot renew or finish; retry exhaustion preserves the hold in `reconciliation_required` until complete zero-call or supplier-cost evidence is atomically finalized.

### Plan 02 — IP content intelligence

- The exact five questions—`我是谁？`, `我的产品或服务要卖给谁？`, `对方为什么会买，又为什么可能不买？`, `对方为什么相信我、选择我，而不是别人？`, and `什么内容能让对方从陌生走到行动？`—plus purchase roles, versioned personas, three strategies and track plans are structurally enforced.
- Fruit, gold gifts, TikTok live guild, platform self-marketing and under-specified mother cases pass scenario tests; the real content-world service expands every ready diagnosis across object taxonomy, history/culture, geography/environment, production/distribution, people/relationships/choice/cost, and buyer/use/decision as research questions—not invented facts—before topic generation.
- A generic label or selling-method input triggers clarification instead of a preselected content niche.
- Research claims, topics, marketing narrative, scripts and Douyin/Xiaohongshu variants preserve evidence and lineage.
- After selecting a topic, an editor may add or revise only that topic's server-derived subject-risk declaration while the project remains active: the declaration binds both the immutable `TopicCard.id` and server-derived subject key, so two different topics about the same subject never share confirmation. The new immutable profile version copies business identity, offer, audience context, constraints and strategy exactly, requires private evidence and restoration truths where applicable, and never forces a full repositioning workflow. Narrative generation stays disabled until the currently selected topic has that exact confirmation.
- Serious delayed accusations against an identifiable real person are mechanically blocked from server-owned subject declarations, private evidence and research claims even when model advisory flags lie; de-identified or fictional reconstruction requires disclosure and full truth restoration.
- The fixed dog regression must block an identifiable owner's sensationalized accusation even when the model returns safe/pass: the full facts are terminal illness, ongoing pain, and euthanasia chosen to relieve suffering. A de-identified or disclosed fictional reconstruction passes only when all three truths are restored before the end.
- The production composition root—not a caller-provided builder—must bind SQL subject-risk resolution, verified private-source lookup, project state, immutable intelligence events, and the six-lens content-world service for both HTTP and Worker paths; a composition test proves none can be omitted or replaced by advisory model output.
- Real Ark/Search activation requires both the Plan 02a durable attempt/supplier-statement runtime and a budget-capped crash-window supplier-transcript reconciliation PoC; official identity documentation or fake-provider tests alone cannot open the adapter.

### Plan 03 — Generative media studio

- A customer is never required to appear on camera: the approved script may route to a human production package, ordinary AI scenes, a mixed production, or—only behind Plan 06's independent consent and Beta gates—the customer's authorized digital human.
- Human production packages, Seedream, Seedance, standard TTS and mixed productions use one shot/job model.
- Paid calls require a hold; retries never double charge; successful temporary URLs are copied before expiry.
- Storyboard preview, per-shot redo, FFmpeg finishing, AI labels and media QC are testable with fakes.
- Seedream, Seedance and standard TTS each remain disabled until a separate hard-budget SIGKILL PoC reconciles the finalized supplier statement to durable request attempts and `ProviderCostEntry.task_id`.

### Plan 04 — Publication and learning loop

- Predictions freeze before manual publication and cannot be edited later.
- T+1/T+3/T+7 metrics retain original screenshots and user-confirmed extraction.
- Comments, retros and memory rules cannot silently rewrite strategy or leak between projects.
- Platform self-marketing tracks have independent baselines and business-action attribution.
- A billed completion that later fails still preserves task-linked supplier cost; customer charge is zero, internal CNY is reconciled, and stale attempts write nothing.

### Plan 05 — Reseller operations

- Only platform→L1→L2→C and platform→L1→C relationships are possible.
- Offline cash records never trigger credits automatically.
- Transfers, returns, prices, support grants, reports and account migration preserve audit and privacy.
- Versioned per-task/day/month limits serialize concurrent reservations, fail closed when unconfigured, and replace every staged allow/deny seam in cross-module integration and production composition.

### Plan 06 — Digital-human Beta and public launch

- Voice cloning, authorized real-person assets and cloned digital humans remain independent feature flags.
- Permission, consent, revocation, deletion, AI labels, complaints and provider PoCs gate every Beta capability.
- Load, security, backup restore and disaster recovery exercises produce signed evidence.
- Public Beta cannot open until the four external projects and separate platform self-marketing acceptance rules pass.

## 7. Specification-to-plan coverage matrix

This matrix is a completeness control, not a summary. A requirement is implemented only when the named plan task and its focused test both exist; a prose mention alone does not count.

| Approved specification section | Owning implementation tasks | Required proof |
|---|---|---|
| 1–2 Product definition, goals, and non-goals | Master rules; Plan 01 Tasks 1 and 16; Plan 06 Task 16 | README scope, CI gate, and release manifest preserve every explicit non-goal |
| 3 First validation projects | Plan 02 Tasks 3–10; Plan 04 Task 11; Plan 06 Task 15 | Fruit, gold gifts, TikTok live guild, platform self-marketing, and under-specified mother fixtures pass without customer leakage |
| 4 Users, hierarchy, privacy, and platform responsibility | Plan 01 Tasks 3–5; Plan 05 Tasks 2, 5, 6, and 8 | Permission matrix, tenant-isolation tests, fixed hierarchy tests, and privacy-safe reports |
| 5 Product operating flow | Plan 02 Tasks 3–10; Plan 03 Tasks 7–17; Plan 04 Tasks 3–10 | One selected topic proceeds through script, production, manual publication, metrics, and retro with immutable lineage |
| 6 Overall architecture | Plan 01 Tasks 1, 2, 6, 11, and 16 | Synchronous transaction boundaries, outbox/RocketMQ contracts, fake-provider tests, and CI |
| 7 Agent roles and constraints | Plan 02 Tasks 1 and 3–10; Plan 03 Tasks 7 and 14; Plan 04 Tasks 4, 6, and 7 | Strict structured outputs, source links, truth review, directing package, QC, and memory controls |
| 8 Core planning reasoning and five questions | Plan 02 Tasks 3, 4, 6, 7, 8, and 10 | Five exact questions, confirmed personas, three comparable strategies, one frozen choice, and per-track plans |
| 9 Content and media production | Plan 03 Tasks 1–17; Plan 06 Tasks 1 and 6–9 | Human/AI/mixed paths, preview approval, per-shot redo, private delivery, QC, rights, and durable labels |
| 10 Data objects and states | Plan 01 Tasks 3, 5, 7, 9, and 11; Plans 02–06 persistence and migration tasks | ORM/migration parity, database constraints, compare-and-set transitions, upgrade/downgrade tests |
| 11 Research, marketing, and truth balance | Plan 02 Tasks 5–8 | Untrusted-source isolation, claim provenance, truthful reveal, identifiable-person reputational-harm gating, risk grading, and no fabricated evidence |
| 12 Prediction and learning loop | Plan 04 Tasks 1–12 | Frozen prediction, actual publication version, T+1/T+3/T+7 evidence, comments, retro, approved memory promotion, billed-failure recovery, and strict publication events |
| 13 Credits and reseller settlement | Plan 01 Tasks 7–9 and 11; Plan 05 Tasks 2–4 and 7–8 | Separate cash/credit/CNY records, double-entry invariants, atomic holds, price versions, limits, and reconciliation |
| 14 Rights, privacy, and compliance | Plan 03 Tasks 3 and 14; Plan 05 Task 5; Plan 06 Tasks 1–5 and 8–10 | Consent and rights evidence, minor default-deny, revocation/deletion, complaints, labels, and independent review |
| 15 Volcengine models and infrastructure | Plan 01 Tasks 1, 10, and 12; Plan 03 Tasks 8–12 and 17; Plan 06 Tasks 6 and 12 | Capability registry, production-entitlement checks, provider contract tests, supplier-first crash reconciliation, TOS copy, and VKE/APIG/VMP evidence |
| 16 Failure recovery and audit | Plan 01 Tasks 6, 9, 11, 12, and 16; Plan 02 Task 14; Plan 03 Tasks 10–17; Plan 04 Tasks 11–12; Plan 06 Tasks 6–7, 9, 13, 14, and 16 | Lease expiry fencing, reconciliation-required recovery, idempotent settlement/release, task-linked supplier-cost preservation, supplier-statement fault tests, audit trail, recycle/deletion workflow, RPO/RTO, and rollback |
| 17 Testing and launch standards | Every plan's final acceptance task; Plan 06 Tasks 14–16 | Unit/integration/contract/security/E2E gates, fixed thresholds, immutable evidence manifest, and two-reviewer release decision |
| 18 Phased build and golden path | Master checkpoints; Plans 01→06 in dependency order | One gold-gifts end-to-end slice at Checkpoint 4, then controlled provider PoCs and six-week Beta |
| 19 Explicitly deferred items | Master rules; Plan 06 Tasks 1 and 16 | README and release checks reject team mode, online payment, third reseller tier, automatic publishing, overseas identity transfer, and digital-human GA/SLA |
| 20 Official basis index | Each plan's reference section | Exact document/version and production-account entitlement evidence are captured at implementation time |

## 8. Required execution checkpoints

- [ ] **Checkpoint 1:** Execute Plan 01 and freeze OpenAPI/event contracts before parallel feature work.
- [ ] **Checkpoint 2:** Execute Plan 02 and freeze the content-lineage contracts.
- [ ] **Checkpoint 3:** Execute Plan 03 against Plan 01's explicit limit seam and freeze media contracts.
- [ ] **Checkpoint 4:** Execute Plan 04; run all cross-module contracts and complete one fake-provider end-to-end gold-gifts slice.
- [ ] **Checkpoint 5:** Merge Plan 05 on `0004_publication`, install real generation limits, and rerun every Plan 01–04 billing/task/Agent/media/publication regression with configured account/internal limit versions.
- [ ] **Checkpoint 6:** Review and deploy the Plan 02a durable-provider reconciliation binding, then run all real-provider PoCs with hard budget caps, including the post-response/pre-cost-persistence crash window and supplier-transcript reconciliation. Execute Plan 06 governance, recovery and release gates only after those controls pass; do not enable real Ark/Search or public Beta before every applicable gate passes.
- [ ] **Checkpoint 7:** Run the six-week closed Beta and record each acceptance result without changing thresholds after seeing outcomes.

At each checkpoint, implementation stops for a user-readable acceptance report containing: what works, what is stubbed, tests run, costs incurred, known risks, rollback method and the explicit decision to continue or correct.

## 9. Definition of done for the complete plan set

The system is not complete merely because every endpoint exists. Completion requires all of the following:

- The approved specification has a corresponding task and test in one of the six detailed plans.
- All ordinary tests pass without paid provider credentials.
- Live provider PoCs pass using the production account and recorded model registry entries.
- No project, tenant, credit, cost, consent, strategy or prediction history can be silently rewritten.
- A C user can complete the gold-gifts path from onboarding through one real manual publication and retro.
- The platform can run its own self-marketing project without privileged content shortcuts or customer-data access.
- Customer-project Beta metrics and platform self-marketing metrics are calculated separately.
- The user receives a plain-language readiness report and explicitly approves public Beta.

## 10. Official implementation references checked for this plan

- [Next.js App Router installation and Node.js requirement](https://nextjs.org/docs/app/getting-started/installation)
- [Next.js deployment as a Node.js server or container](https://nextjs.org/docs/app/getting-started/deploying)
- [FastAPI version pinning guidance](https://fastapi.tiangolo.com/deployment/versions/)
- [SQLAlchemy 2.0 unified tutorial](https://docs.sqlalchemy.org/en/20/tutorial/index.html)
- [Alembic tutorial](https://alembic.sqlalchemy.org/en/latest/tutorial.html)
- [Playwright Test installation and browser setup](https://playwright.dev/docs/intro)
- [Apache RocketMQ 5.x Python client](https://github.com/apache/rocketmq-clients/tree/master/python)
- [Apache RocketMQ 5.x Docker Compose quick start with Proxy](https://rocketmq.apache.org/docs/quickStart/03quickstartWithDockercompose/)

The dependency manifests created by Plan 01 must pin exact resolved versions in `uv.lock` and `pnpm-lock.yaml`. Upgrades happen only through a dedicated dependency commit followed by the full test suite; production deployment never installs unpinned `latest` packages.

## 11. Execution handoff

Two execution options:

1. **Subagent-Driven (recommended)** — use `superpowers:subagent-driven-development`, execute one detailed-plan task with a fresh worker, review specification compliance and code quality, then stop at every numbered checkpoint for the Chinese business acceptance report.
2. **Inline Execution** — use `superpowers:executing-plans`, execute the six plans sequentially in this session, with the same checkpoint stops and no paid-provider or public-release action before its explicit gate.
