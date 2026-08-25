# Digital-Human Beta and Public-Launch Gates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add independently gated voice cloning, Seedance authorized-real-person media, and traditional cloned-digital-human Beta workflows, then prove governance, infrastructure, recovery, security, and six-week business evidence before any public-Beta switch can open.

**Architecture:** Extend the existing modular monolith with one governance module that owns consent, provider entitlements, release gates, identity enrollments, complaints, export verification, Beta evidence, and release decisions. Reuse Plan 03 character, shot, GenerationJob, MediaAsset, QCReport, private-TOS, task, billing, outbox, and AI-disclosure seams; TaskRecord remains the only top-level task and hold truth. Long provider work runs in synchronous workers after PostgreSQL commits, while feature eligibility, consent, quota reservations, task creation, identity-use lineage, audit, and outbox writes remain atomic in one synchronous SQLAlchemy Session transaction.

**Tech Stack:** Python 3.12, FastAPI 0.136.x synchronous routes, Pydantic 2.x, SQLAlchemy 2.0 synchronous Session, Alembic, PostgreSQL 16, Redis, RocketMQ, TOS, FFmpeg/FFprobe, HTTPX, Pytest, Hypothesis, Locust, Next.js 16 App Router, React 19, TypeScript 5.x, Vitest, Playwright, VKE, VCI, APIG, VMP, KMS, Helm, kubeconform, Conftest, Promtool.

---

## Prerequisites, frozen seams, and non-goals

Execute Plans 01 through 05 and their acceptance commands before starting this plan. Do not merge this plan until migration 0005_resellers is present. This plan creates exactly one migration:

~~~python
revision: str = "0006_governance"
down_revision: str | None = "0005_resellers"
~~~

All Python data access remains synchronous. FastAPI request handlers use def, never AsyncSession. Paid, slow, or retryable calls execute in workers after a TaskRecord and outbox event exist. PostgreSQL, not Redis or RocketMQ, remains the source of truth.

Reuse these public paths without renaming or duplicating their responsibility:

~~~python
from ip_saas.common.audit import AuditWriter
from ip_saas.common.clock import Clock, SystemClock
from ip_saas.common.errors import Forbidden
from ip_saas.db.base import Base
from ip_saas.db.session import get_session, session_scope
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.dependencies import get_actor
from ip_saas.modules.billing.service import (
    BillingContext,
    BillingMode,
    BillingService,
    ProviderCostInput,
)
from ip_saas.modules.billing.limits import GenerationLimitService
from ip_saas.modules.billing.ports import (
    CreditHoldPort,
    GenerationLimitPort,
    InternalBudgetHoldPort,
)
from ip_saas.modules.media.enums import AiDisclosure, JobStatus, QcDecision
from ip_saas.modules.media.labels import disclosure_for_sources
from ip_saas.modules.media.models.identity import (
    CharacterProfile,
    MediaRightsGrant,
    VoiceAsset,
)
from ip_saas.modules.media.models.jobs import GenerationJob, MediaAsset, QCReport
from ip_saas.modules.media.models.production import (
    HumanProductionPackage,
    MediaProduction,
    Scene,
    Shot,
)
from ip_saas.modules.projects.access import ProjectAccessService
from ip_saas.modules.projects.models import IPProject
from ip_saas.common.tasking import TaskStatus
from ip_saas.modules.tasks.models import TaskRecord
from ip_saas.modules.tasks.reconciliation import TaskReconciliationService
from ip_saas.modules.tasks.service import TaskSubmissionService
~~~

The following contracts remain frozen:

~~~text
ProjectAccessService.require_viewer(session, actor, project_id) -> IPProject
ProjectAccessService.require_editor(session, actor, project_id) -> IPProject
AuditWriter.write(
    session,
    *,
    actor,
    action,
    target_type,
    target_id,
    project_id=None,
    metadata=None,
)
OutboxWriter.add(session, event) -> object
TaskSubmissionService.submit_customer(
    session,
    actor,
    project_id,
    capability,
    max_credit_units,
    idempotency_key,
    input_payload,
) -> TaskRecord
TaskSubmissionService.submit_internal(
    session,
    actor,
    project_id,
    capability,
    cost_center_id,
    max_amount_fen,
    idempotency_key,
    input_payload,
) -> TaskRecord
TaskSubmissionService.start(
    session, task_id, lease_seconds=300, max_attempts=8
) -> TaskRecord
TaskSubmissionService.heartbeat(
    session, task_id, attempt_no, lease_seconds=300
) -> TaskRecord
TaskSubmissionService.succeed(
    session, task_id, attempt_no, result_payload
) -> TaskRecord
TaskSubmissionService.fail(
    session, task_id, attempt_no, error_code, error_message
) -> TaskRecord
TaskReconciliationService.finalize_provider_costs(
    session,
    task_id,
    attempt_no,
    actual_amount,
    provider_costs,
    reason,
    idempotency_key,
) -> TaskRecord
~~~

TaskRecord has `attempt_no: int = 0`, `lease_expires_at: datetime | None`, and non-null `request_fingerprint: str`, the 64-character SHA-256 generated by TaskSubmissionService from the immutable request. `GenerationTaskRequestedV1` carries the same fingerprint. `start` atomically changes queued or expired-running work to running, increments attempt_no, and sets the lease. It returns an already-terminal task unchanged so the message can be acknowledged, rejects an active running task with Conflict, reclaims an expired running task under a new attempt, and, after eight exhausted attempts, moves it to the durable non-terminal `RECONCILIATION_REQUIRED` state without releasing its hold. Every heartbeat and terminal write is fenced by the exact attempt number. Every governance Worker that receives `RECONCILIATION_REQUIRED` returns RETRY without a provider call or domain, cost, event, quota, delivery, or task mutation; only the Plan 01 `TaskReconciliationService` may batch-finalize complete supplier costs (or separately prove no provider call) and move the task to FAILED. A stale worker receives Conflict and may not update governance state, settle/release billing, settle/release quota, create delivery assets, or acknowledge the message as successful. Plan 06 fixtures submit tasks through TaskSubmissionService; any narrowly isolated fixture that directly constructs a TaskRecord must set `request_fingerprint="0" * 64` or another exact 64-character test hash.

`ProviderCostInput` is also frozen with the Plan 01 fields `provider`, `capability`, `model_id`, `model_version`, `native_quantity`, `native_unit`, `supplier_amount_minor`, `supplier_currency`, `amount_fen`, `reconciliation_status`, `task_id: UUID | None = None`, and `provider_request_id: str | None = None`. `BillingService.settle_generation` persists those values into `ProviderCostEntry`, whose task foreign key targets `task_records.id`. Every Plan 06 completion that actually calls a provider must set `task_id` to the exact current `TaskRecord.id` and use the supplier's real canonical request identity; every observed supplier request identity is canonical trimmed text of 1–200 characters before journal/evidence/ledger persistence, and null, non-string, blank, the literal strings `None` or `missing`, or 201-character input fails closed. Every reconciliation cost additionally has non-empty unique `(provider, provider_request_id)` and `ReconciliationStatus.MATCHED`. A production path proven to have made zero supplier calls constructs no `ProviderCostInput` and inserts no `ProviderCostEntry`; only durable zero-call evidence may drive `finalize_no_provider_call()` and release the hold. A deterministic fake also may not fabricate a lineage-free zero-cost input: ordinary fake-only tests bypass settlement, while a test that deliberately exercises the cost ledger supplies an explicit canonical fake request ID and the exact TaskRecord ID. `TaskReconciliationService.finalize_provider_costs` validates the feature-owned complete request manifest, persists all real costs in one transaction, releases customer credit with `actual_amount=0` or settles internal CNY with `actual_amount=sum(cost.amount_fen)`, and transitions the task once to FAILED; Plan 06 does not loop the finalizer per supplier line or create another reconciliation service.

After Plan 05, every `BillingService` construction has exactly three required dependencies: `CreditHoldPort`, `InternalBudgetHoldPort`, and the frozen `GenerationLimitPort`. Plan 06 never relies on a default, `None`, or implicit unlimited mode. Production API and Worker composition pass the real `GenerationLimitService(SystemClock())` implementation. Integration fixtures and Beta evidence insert real effective `GenerationLimitVersion` rows for every customer account and internal cost center before constructing BillingService. A narrowly isolated unit test may explicitly pass Plan 01's `tests.support.generation_limits.AllowAllGenerationLimits`; that allow-fake is forbidden in integration, PoC, load, resilience, and Beta-evidence paths.

The frontend seam is frozen too: `frontend/src/lib/api/schema.d.ts` is the only generated OpenAPI type file, and every Plan 06 feature reuses `frontend/src/lib/api/client.ts`. Feature code passes `apiRequest` a backend path beginning with `/v1/`; that shared client alone adds the browser `/api` prefix. Plan 06 creates neither `generated.ts` nor a second fetch/API client and never passes `/api/v1/...` into `apiRequest`.

Plan 03 semantics are extended, not rewritten:

- VoiceAsset kind uploaded_recording continues to mean already-produced source audio. It is never reinterpreted as a cloned voice.
- Provider-enrolled cloned voices, authorized Seedance portraits, and cloned avatars live in ProviderIdentityAsset. A GenerationIdentityUse row connects them to a GenerationJob.
- GenerationJob keeps one unique task_record_id. No governance table adds customer-hold and internal-hold columns.
- Ordinary Plan 03 ShotDraft still rejects cloned_voice and raw asset:// text. Only the governed Plan 06 submission service may resolve an active local identity asset to a provider reference inside a worker.
- MediaAsset remains the deliverable object. Provider result URLs and provider identity references never reach API payloads, logs, audit metadata, outbox payloads, or browser code.
- Every required visible AI label remains non-removable. Every transcode and export creates a new ExportVerification before delivery.

This plan does not add team collaboration, online payment, a third reseller level, automatic publishing, TikTok API integration, overseas transfer of identity material, or a GA/SLA state for cloned digital humans. Public Beta is still Beta. Resellers cannot enroll, inspect, download, revoke, or delete a customer's identity material, even when they hold a Plan 05 media support grant.

## File map

~~~text
backend/migrations/versions/0006_governance.py
backend/src/ip_saas/api.py
backend/src/ip_saas/config.py
backend/src/ip_saas/modules/media/enums.py
backend/src/ip_saas/modules/media/ffmpeg.py
backend/src/ip_saas/modules/media/labels.py
backend/src/ip_saas/modules/media/models/__init__.py
backend/src/ip_saas/modules/media/models/jobs.py
backend/src/ip_saas/modules/media/qc.py
backend/src/ip_saas/modules/media/rights.py
backend/src/ip_saas/modules/governance/__init__.py
backend/src/ip_saas/modules/governance/enums.py
backend/src/ip_saas/modules/governance/events.py
backend/src/ip_saas/modules/governance/schemas.py
backend/src/ip_saas/modules/governance/models.py
backend/src/ip_saas/modules/governance/feature_flags.py
backend/src/ip_saas/modules/governance/consent.py
backend/src/ip_saas/modules/governance/eligibility.py
backend/src/ip_saas/modules/governance/enrollment.py
backend/src/ip_saas/modules/governance/generation.py
backend/src/ip_saas/modules/governance/provider_crash.py
backend/src/ip_saas/modules/governance/revocation.py
backend/src/ip_saas/modules/governance/deletion.py
backend/src/ip_saas/modules/governance/export_labels.py
backend/src/ip_saas/modules/governance/complaints.py
backend/src/ip_saas/modules/governance/self_marketing_review.py
backend/src/ip_saas/modules/governance/beta.py
backend/src/ip_saas/modules/governance/router.py
backend/src/ip_saas/modules/publication/service.py
backend/src/ip_saas/providers/digital_identity/__init__.py
backend/src/ip_saas/providers/digital_identity/base.py
backend/src/ip_saas/providers/digital_identity/composition.py
backend/src/ip_saas/providers/digital_identity/fake.py
backend/src/ip_saas/providers/volcengine/voice_clone.py
backend/src/ip_saas/providers/volcengine/seedance_portrait.py
backend/src/ip_saas/providers/volcengine/cloned_avatar.py
backend/src/ip_saas/workers/identity_enrollment.py
backend/src/ip_saas/workers/identity_deletion.py
backend/src/ip_saas/workers/media_submit.py
backend/src/ip_saas/workers/media_copy.py
backend/src/ip_saas/workers/media_finish.py
backend/src/ip_saas/workers/media_qc.py
backend/src/ip_saas/workers/recovery_marker.py
backend/src/ip_saas/scripts/provider_poc.py
backend/src/ip_saas/scripts/export_contracts.py
backend/src/ip_saas/scripts/dr_restore.py
backend/src/ip_saas/scripts/evaluate_structured_output.py
backend/src/ip_saas/scripts/evaluate_public_beta.py
backend/src/ip_saas/scripts/release_control.py
backend/src/ip_saas/scripts/rocketmq_poc.py
backend/tests/unit/governance/test_beta_evaluator.py
backend/tests/unit/governance/test_consent_policy.py
backend/tests/unit/governance/test_dr_thresholds.py
backend/tests/unit/governance/test_export_label_policy.py
backend/tests/unit/governance/test_feature_flags.py
backend/tests/unit/governance/test_model_shape.py
backend/tests/unit/governance/test_schemas.py
backend/tests/unit/governance/test_structured_output_gate.py
backend/tests/integration/governance/test_blind_review.py
backend/tests/integration/governance/test_complaint_takedown.py
backend/tests/integration/governance/test_consent_withdrawal.py
backend/tests/integration/governance/test_eligibility.py
backend/tests/integration/governance/test_enrollment_submission.py
backend/tests/integration/governance/test_export_label_survival.py
backend/tests/integration/governance/test_global_deletion.py
backend/tests/integration/governance/test_governed_generation.py
backend/tests/integration/governance/test_migration.py
backend/tests/integration/governance/test_provider_poc_evaluator.py
backend/tests/integration/governance/test_provider_crash_reconciliation.py
backend/tests/integration/governance/test_provider_release_composition.py
backend/tests/integration/governance/test_public_beta_evidence.py
backend/tests/integration/governance/test_release_control.py
backend/tests/integration/governance/test_self_marketing_review.py
backend/tests/contract/governance/test_event_schemas.py
backend/tests/contract/governance/test_event_consumers.py
backend/tests/contract/governance/test_fixture_closure.py
backend/tests/contract/governance/test_openapi.py
backend/tests/contract/governance/test_provider_adapters.py
backend/tests/contract/governance/test_rds_restore_actions.py
backend/tests/conftest.py
backend/tests/__init__.py
backend/tests/support/__init__.py
backend/tests/support/governance_fixtures.py
backend/tests/support/governance_rig.py
backend/tests/security/test_identity_boundaries.py
backend/tests/security/test_governance_ssrf.py
backend/tests/security/test_secret_and_log_redaction.py
backend/tests/load/locustfile.py
backend/tests/resilience/test_deleted_data_not_restored.py
backend/tests/resilience/test_governance_faults.py
backend/tests/fixtures/governance/public-beta-criteria.v1.json
backend/tests/fixtures/governance/structured-output-corpus.v1.jsonl
backend/tests/fixtures/governance/structured-output-corpus.v1.sha256
contracts/events/identity.enrollment.requested.v1.json
contracts/events/identity.deletion.requested.v1.json
contracts/events/governed.media.requested.v1.json
contracts/events/complaint.takedown.requested.v1.json
contracts/events/deletion.plan.confirmed.v1.json
contracts/events/capability.rollback.requested.v1.json
contracts/events/capability.release.executed.v1.json
contracts/openapi.json
frontend/src/app/(c-user)/projects/[projectId]/identity/page.tsx
frontend/src/app/(platform)/platform/governance/page.tsx
frontend/src/app/(platform)/platform/public-beta/page.tsx
frontend/src/features/governance/api.ts
frontend/src/features/governance/types.ts
frontend/src/features/governance/ConsentWizard.tsx
frontend/src/features/governance/IdentityAssetPanel.tsx
frontend/src/features/governance/CapabilityBadge.tsx
frontend/src/features/governance/ComplaintStatus.tsx
frontend/src/features/governance/ReleaseGateBoard.tsx
frontend/src/features/governance/__tests__/ConsentWizard.test.tsx
frontend/src/features/governance/__tests__/IdentityAssetPanel.test.tsx
frontend/src/lib/api/schema.d.ts
frontend/tests/e2e/digital-human-beta.spec.ts
frontend/tests/e2e/consent-withdrawal.spec.ts
frontend/tests/e2e/public-beta-release.spec.ts
infra/helm/ip-saas/Chart.yaml
infra/helm/ip-saas/values.schema.json
infra/helm/ip-saas/values-cn-beijing.yaml
infra/helm/ip-saas/templates/api.yaml
infra/helm/ip-saas/templates/workers.yaml
infra/helm/ip-saas/templates/service.yaml
infra/helm/ip-saas/templates/apig-ingress.yaml
infra/helm/ip-saas/templates/autoscaling.yaml
infra/helm/ip-saas/templates/network-policy.yaml
infra/helm/ip-saas/templates/vmp-monitoring.yaml
infra/helm/ip-saas/templates/recovery-jobs.yaml
infra/helm/ip-saas/tests/deployment_test.yaml
infra/observability/governance-alerts.yaml
infra/observability/governance-dashboard.json
infra/scripts/verify-production-secrets.sh
infra/scripts/preflight-production.sh
infra/scripts/deploy.sh
infra/scripts/rollback.sh
infra/scripts/restore-postgres.sh
infra/scripts/restore-tos-sample.sh
infra/scripts/verify-rpo-rto.sh
infra/runbooks/provider-poc.md
infra/runbooks/identity-incident.md
infra/runbooks/backup-restore.md
infra/runbooks/public-beta-release.md
Makefile
README.md
~~~

### Task 1: Freeze independent Beta vocabulary and strict request contracts

**Files:**
- Create: backend/src/ip_saas/modules/governance/__init__.py
- Create: backend/src/ip_saas/modules/governance/enums.py
- Create: backend/src/ip_saas/modules/governance/schemas.py
- Modify: backend/tests/conftest.py
- Create: backend/tests/__init__.py
- Create: backend/tests/support/__init__.py
- Create: backend/tests/support/governance_fixtures.py
- Create: backend/tests/support/governance_rig.py
- Test: backend/tests/unit/governance/test_schemas.py
- Test: backend/tests/contract/governance/test_fixture_closure.py

- [ ] **Step 1: Write the failing independence, consent, and minor request tests**

Create backend/tests/unit/governance/test_schemas.py:

~~~python
from datetime import UTC, datetime, timedelta
from uuid import uuid4

import pytest
from pydantic import ValidationError

from ip_saas.modules.governance.enums import (
    BetaCapability,
    CapabilityStage,
    ConsentKind,
    IdentityKind,
)
from ip_saas.modules.governance.schemas import (
    ConsentCreate,
    GovernedGenerationCreate,
    IdentityEnrollmentCreate,
)


def consent(kind: ConsentKind) -> ConsentCreate:
    now = datetime(2026, 8, 24, tzinfo=UTC)
    return ConsentCreate(
        subject_id=uuid4(),
        kind=kind,
        terms_version="identity-consent-cn-v1",
        purposes=["content_generation", "commercial_distribution"],
        platforms=["douyin", "xiaohongshu"],
        territory="CN",
        valid_from=now,
        valid_until=now + timedelta(days=180),
        usage_limit=120,
        commercial_use=True,
        modeling_allowed=True,
        reuse_allowed=False,
        provider_transfer_allowed=True,
        existing_output_policy="published_outputs_remain_withdrawal_notified",
        verification_method="direct_liveness",
        verification_evidence_asset_id=uuid4(),
    )


def test_three_capabilities_and_stages_are_independent() -> None:
    assert {item.value for item in BetaCapability} == {
        "voice_clone",
        "seedance_authorized_portrait",
        "traditional_digital_human",
    }
    assert CapabilityStage.GA.value not in {item.value for item in CapabilityStage}


def test_voice_and_portrait_are_separate_consents() -> None:
    voice = consent(ConsentKind.VOICE)
    portrait = consent(ConsentKind.PORTRAIT)
    assert voice.kind is ConsentKind.VOICE
    assert portrait.kind is ConsentKind.PORTRAIT
    with pytest.raises(ValidationError, match="one consent kind"):
        ConsentCreate.model_validate(
            {
                **voice.model_dump(),
                "kind": ["voice", "portrait"],
            }
        )


def test_identity_enrollment_cannot_claim_a_raw_provider_reference() -> None:
    with pytest.raises(ValidationError):
        IdentityEnrollmentCreate(
            identity_kind=IdentityKind.SEEDANCE_PORTRAIT,
            subject_id=uuid4(),
            source_media_asset_ids=[uuid4()],
            consent_ids=[uuid4()],
            provider_asset_id="asset-secret-that-must-not-enter-the-api",
            idempotency_key="portrait:enroll:1",
        )


def test_generation_accepts_local_identity_ids_only() -> None:
    identity_id = uuid4()
    request = GovernedGenerationCreate(
        production_id=uuid4(),
        shot_id=uuid4(),
        capability=BetaCapability.TRADITIONAL_DIGITAL_HUMAN,
        identity_asset_ids=[identity_id],
        script_text="真正的礼，不是越贵越好。",
        platform="douyin",
        approved_credit_units=80,
        approved_amount_fen=None,
        idempotency_key="avatar:shot:1:v1",
    )
    assert "provider_asset" not in request.model_dump()
    assert request.identity_asset_ids == [identity_id]
    assert request.capability is BetaCapability.TRADITIONAL_DIGITAL_HUMAN


def test_v1_generation_rejects_more_than_one_identity() -> None:
    with pytest.raises(ValidationError):
        GovernedGenerationCreate(
            production_id=uuid4(), shot_id=uuid4(),
            capability=BetaCapability.VOICE_CLONE,
            identity_asset_ids=[uuid4(), uuid4()],
            script_text="同一任务只允许一个受治理身份。", platform="douyin",
            approved_credit_units=1, approved_amount_fen=None,
            idempotency_key="voice:single-identity:v1",
        )


def test_voice_clone_requires_one_unique_governed_source() -> None:
    with pytest.raises(ValidationError, match="exactly one"):
        IdentityEnrollmentCreate(
            identity_kind=IdentityKind.VOICE_CLONE,
            subject_id=uuid4(), source_media_asset_ids=[uuid4(), uuid4()],
            provider_verification_receipt_asset_id=None,
            consent_ids=[uuid4(), uuid4()], idempotency_key="voice:sources:v1",
            approved_credit_units=1, approved_amount_fen=None,
        )
~~~

- [ ] **Step 2: Run the request tests and verify the missing-module failure**

Run:

~~~bash
cd backend && uv run pytest tests/unit/governance/test_schemas.py -v
~~~

Expected: FAIL during collection with ModuleNotFoundError for ip_saas.modules.governance.

- [ ] **Step 3: Create the complete governance enum vocabulary**

Create backend/src/ip_saas/modules/governance/enums.py:

~~~python
from enum import StrEnum


class BetaCapability(StrEnum):
    VOICE_CLONE = "voice_clone"
    SEEDANCE_AUTHORIZED_PORTRAIT = "seedance_authorized_portrait"
    TRADITIONAL_DIGITAL_HUMAN = "traditional_digital_human"


class CapabilityStage(StrEnum):
    OFF = "off"
    INTERNAL_POC = "internal_poc"
    CLOSED_BETA = "closed_beta"
    PUBLIC_BETA = "public_beta"


class GateKind(StrEnum):
    PRODUCTION_PERMISSION = "production_permission"
    PROVIDER_QUOTA = "provider_quota"
    WRITTEN_CONTRACT = "written_contract"
    CONSENT_REHEARSAL = "consent_rehearsal"
    BATCH_STABILITY = "batch_stability"
    COST_RECONCILIATION = "cost_reconciliation"
    FAILURE_RECOVERY = "failure_recovery"
    PROVIDER_CRASH_RECONCILIATION = "provider_crash_reconciliation"
    MEDIA_QC = "media_qc"
    AI_LABEL = "ai_label"
    REVOCATION = "revocation"
    DELETION = "deletion"
    COMPLAINT_TAKEDOWN = "complaint_takedown"
    STRUCTURED_OUTPUT = "structured_output"


class GateResult(StrEnum):
    PASS = "pass"
    FAIL = "fail"
    EXPIRED = "expired"


class ConsentKind(StrEnum):
    PORTRAIT = "portrait"
    VOICE = "voice"
    SENSITIVE_PERSONAL_INFO = "sensitive_personal_info"
    MINOR_SOURCE_MATERIAL = "minor_source_material"
    MINOR_MODELING = "minor_modeling"


class ConsentStatus(StrEnum):
    PENDING_VERIFICATION = "pending_verification"
    ACTIVE = "active"
    WITHDRAWN = "withdrawn"
    EXPIRED = "expired"


class SubjectAgeClass(StrEnum):
    ADULT = "adult"
    MINOR = "minor"
    UNKNOWN = "unknown"


class IdentityKind(StrEnum):
    VOICE_CLONE = "voice_clone"
    SEEDANCE_PORTRAIT = "seedance_portrait"
    TRADITIONAL_AVATAR = "traditional_avatar"


class IdentityAssetStatus(StrEnum):
    ENROLLING = "enrolling"
    ACTIVE = "active"
    FROZEN = "frozen"
    DELETION_PENDING = "deletion_pending"
    DELETED = "deleted"
    FAILED = "failed"


class EnrollmentStatus(StrEnum):
    QUEUED = "queued"
    RUNNING = "running"
    SUCCEEDED = "succeeded"
    FAILED = "failed"
    CANCELLED = "cancelled"


class ComplaintSeverity(StrEnum):
    CRITICAL = "critical"
    HIGH = "high"
    NORMAL = "normal"


class ComplaintStatus(StrEnum):
    RECEIVED = "received"
    IDENTITY_FROZEN = "identity_frozen"
    INVESTIGATING = "investigating"
    UPHELD = "upheld"
    REJECTED = "rejected"
    CLOSED = "closed"


class DeletionStatus(StrEnum):
    REQUESTED = "requested"
    PROVIDER_PENDING = "provider_pending"
    LIVE_DATA_DELETED = "live_data_deleted"
    VERIFIED = "verified"
    FAILED = "failed"


class RecycleTargetType(StrEnum):
    PROJECT = "project"
    CONTENT = "content"
    MEDIA_ASSET = "media_asset"
    IDENTITY_SUBJECT = "identity_subject"
    PROVIDER_IDENTITY_ASSET = "provider_identity_asset"


class DeletionPlanStatus(StrEnum):
    TRASHED = "trashed"
    RESTORED = "restored"
    PURGE_PENDING = "purge_pending"
    PURGING = "purging"
    PURGED = "purged"
    FAILED = "failed"


class DeletionSystem(StrEnum):
    DATABASE = "database"
    TOS = "tos"
    PROVIDER_MODEL = "provider_model"
    CACHE = "cache"
    BACKUP_RETENTION = "backup_retention"


class DeletionStepStatus(StrEnum):
    PENDING = "pending"
    RUNNING = "running"
    SUCCEEDED = "succeeded"
    FAILED = "failed"
    NATURAL_EXPIRY_PENDING = "natural_expiry_pending"


class BlindReviewWinner(StrEnum):
    SYSTEM = "system"
    GENERIC_AI = "generic_ai"
    TIE = "tie"
    DISQUALIFIED = "disqualified"


class SelfMarketingReviewDecision(StrEnum):
    APPROVED = "approved"
    REJECTED = "rejected"


class BetaProjectKind(StrEnum):
    CUSTOMER = "customer"
    PLATFORM_SELF_MARKETING = "platform_self_marketing"


class BetaCustomerCase(StrEnum):
    FRUIT = "fruit"
    GOLD_GIFTS = "gold_gifts"
    TIKTOK_LIVE_GUILD = "tiktok_live_guild"
    MOTHER_CREATOR = "mother_creator"


class ReleaseDecisionKind(StrEnum):
    HOLD = "hold"
    APPROVE_PUBLIC_BETA = "approve_public_beta"
    ROLLBACK = "rollback"
~~~

Create an empty backend/src/ip_saas/modules/governance/__init__.py.

- [ ] **Step 4: Create strict API schemas without provider identifiers**

Create backend/src/ip_saas/modules/governance/schemas.py:

~~~python
from datetime import datetime
from typing import Literal
from uuid import UUID, uuid4

from pydantic import BaseModel, ConfigDict, Field, model_validator

from .enums import (
    BetaCapability,
    CapabilityStage,
    ConsentKind,
    IdentityKind,
)


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class IdentitySubjectCreate(StrictModel):
    pseudonym: str = Field(min_length=1, max_length=120)
    age_class: Literal["adult", "minor"]
    guardian_subject_id: UUID | None = None
    age_verification_method: str = Field(min_length=3, max_length=80)
    age_verification_evidence_asset_id: UUID

    @model_validator(mode="after")
    def require_guardian_for_minor(self) -> "IdentitySubjectCreate":
        if self.age_class == "minor" and self.guardian_subject_id is None:
            raise ValueError("a minor subject requires a verified guardian subject")
        if self.age_class == "adult" and self.guardian_subject_id is not None:
            raise ValueError("an adult subject cannot have a guardian subject")
        return self


class ConsentCreate(StrictModel):
    subject_id: UUID
    kind: ConsentKind
    terms_version: str = Field(min_length=1, max_length=100)
    purposes: list[str] = Field(min_length=1, max_length=12)
    platforms: list[Literal["douyin", "xiaohongshu"]] = Field(min_length=1)
    territory: Literal["CN"]
    valid_from: datetime
    valid_until: datetime
    usage_limit: int = Field(ge=1, le=100_000)
    commercial_use: bool
    modeling_allowed: bool
    reuse_allowed: bool
    provider_transfer_allowed: bool
    existing_output_policy: Literal[
        "published_outputs_remain_withdrawal_notified",
        "request_platform_takedown",
    ]
    verification_method: Literal[
        "direct_liveness",
        "provider_real_person_verification",
        "guardian_direct_liveness",
    ]
    verification_evidence_asset_id: UUID

    @model_validator(mode="after")
    def validate_high_risk_scope(self) -> "ConsentCreate":
        if self.valid_until <= self.valid_from:
            raise ValueError("consent validity must increase")
        if self.kind is ConsentKind.MINOR_SOURCE_MATERIAL:
            if self.modeling_allowed or self.provider_transfer_allowed:
                raise ValueError("minor source-material consent cannot authorize modeling or provider transfer")
            return self
        if not self.commercial_use or not self.modeling_allowed:
            raise ValueError("identity enrollment requires commercial modeling consent")
        if not self.provider_transfer_allowed:
            raise ValueError("identity enrollment requires explicit provider transfer consent")
        return self


class ConsentWithdraw(StrictModel):
    reason: str = Field(min_length=1, max_length=500)
    request_provider_deletion: bool = True


class IdentityEnrollmentCreate(StrictModel):
    identity_kind: IdentityKind
    subject_id: UUID
    source_media_asset_ids: list[UUID] = Field(default_factory=list, max_length=10)
    provider_verification_receipt_asset_id: UUID | None = None
    consent_ids: list[UUID] = Field(min_length=1, max_length=6)
    idempotency_key: str = Field(min_length=8, max_length=160)
    approved_credit_units: int | None = Field(default=None, ge=1)
    approved_amount_fen: int | None = Field(default=None, ge=1)

    @model_validator(mode="after")
    def require_one_money_dimension(self) -> "IdentityEnrollmentCreate":
        if (self.approved_credit_units is None) == (self.approved_amount_fen is None):
            raise ValueError("exactly one enrollment approval amount is required")
        if self.identity_kind is IdentityKind.SEEDANCE_PORTRAIT:
            if self.source_media_asset_ids or self.provider_verification_receipt_asset_id is None:
                raise ValueError("Seedance portrait requires only a provider verification receipt")
        elif not self.source_media_asset_ids or self.provider_verification_receipt_asset_id is not None:
            raise ValueError("voice and traditional avatar enrollment require governed sources only")
        if self.identity_kind is IdentityKind.VOICE_CLONE and len(self.source_media_asset_ids) != 1:
            raise ValueError("voice clone requires exactly one governed voice source")
        if len(set(self.source_media_asset_ids)) != len(self.source_media_asset_ids):
            raise ValueError("source media asset ids must be unique")
        if len(set(self.consent_ids)) != len(self.consent_ids):
            raise ValueError("consent ids must be unique")
        return self


class GovernedGenerationCreate(StrictModel):
    production_id: UUID
    shot_id: UUID
    capability: BetaCapability
    identity_asset_ids: list[UUID] = Field(min_length=1, max_length=1)
    script_text: str = Field(min_length=1, max_length=4_000)
    platform: Literal["douyin", "xiaohongshu"]
    approved_credit_units: int | None = Field(default=None, ge=1)
    approved_amount_fen: int | None = Field(default=None, ge=1)
    idempotency_key: str = Field(min_length=8, max_length=160)

    @model_validator(mode="after")
    def require_one_generation_amount(self) -> "GovernedGenerationCreate":
        if (self.approved_credit_units is None) == (self.approved_amount_fen is None):
            raise ValueError("exactly one generation approval amount is required")
        return self


class ComplaintCreate(StrictModel):
    content_id: UUID | None = None
    identity_asset_id: UUID | None = None
    category: Literal[
        "unauthorized_identity",
        "impersonation",
        "minor_safety",
        "missing_ai_label",
        "other",
    ]
    description: str = Field(min_length=10, max_length=4_000)
    evidence_asset_ids: list[UUID] = Field(default_factory=list, max_length=10)
    contact_ciphertext: str = Field(min_length=20, max_length=4_000)

    @model_validator(mode="after")
    def require_a_target(self) -> "ComplaintCreate":
        if self.content_id is None and self.identity_asset_id is None:
            raise ValueError("a complaint requires content_id or identity_asset_id")
        return self
~~~

- [ ] **Step 5: Register every shared Plan 06 pytest fixture explicitly**

Create empty package markers backend/tests/__init__.py and backend/tests/support/__init__.py. Append this exact plugin registration to the Plan 01 backend/tests/conftest.py:

~~~python
pytest_plugins = ("tests.support.governance_fixtures",)
~~~

Create backend/tests/support/governance_fixtures.py. `db_session` and `pg_engine` are the PostgreSQL fixtures already frozen in Plan 01; every other non-parametrize argument used by Tasks 3–16 is defined here rather than assumed:

~~~python
from collections.abc import Iterator
from json import loads
from pathlib import Path

import pytest
from sqlalchemy import Engine
from sqlalchemy.orm import Session

from tests.support.governance_rig import GovernanceTestRig


@pytest.fixture
def governance_rig(db_session: Session, pg_engine: Engine) -> GovernanceTestRig:
    return GovernanceTestRig(db_session, pg_engine)


@pytest.fixture
def fake_clock(governance_rig): return governance_rig.fake_clock()


@pytest.fixture
def eligibility(governance_rig): return governance_rig.eligibility()


@pytest.fixture
def customer_actor(governance_rig): return governance_rig.customer_actor()


@pytest.fixture
def platform_operator(governance_rig): return governance_rig.platform_operator()


@pytest.fixture
def l1_actor_with_media_support(governance_rig):
    return governance_rig.l1_actor_with_media_support()


@pytest.fixture
def seeded_governance(governance_rig): return governance_rig.seeded_governance()


@pytest.fixture
def quota_service(governance_rig): return governance_rig.quota_service()


@pytest.fixture
def revocation_service(governance_rig): return governance_rig.revocation_service()


@pytest.fixture
def queued_governed_generation(governance_rig):
    return governance_rig.queued_governed_generation()


@pytest.fixture
def running_governed_generation(governance_rig):
    return governance_rig.running_governed_generation()


@pytest.fixture
def completed_governed_generation(governance_rig):
    return governance_rig.completed_governed_generation()


@pytest.fixture
def identity_repository(governance_rig): return governance_rig.identity_repository()


@pytest.fixture
def customer_identity(governance_rig): return governance_rig.customer_identity()


@pytest.fixture
def minor_source_material_validator(governance_rig):
    return governance_rig.minor_source_material_validator()


@pytest.fixture
def production_adapter_harness(governance_rig):
    return governance_rig.production_adapter_harness()


@pytest.fixture
def customer_enrollment_command(governance_rig):
    return governance_rig.customer_enrollment_command()


@pytest.fixture
def platform_enrollment_command(governance_rig):
    return governance_rig.platform_enrollment_command()


@pytest.fixture
def enrollment_service(governance_rig): return governance_rig.enrollment_service()


@pytest.fixture
def enrollment_service_with_failing_outbox(governance_rig):
    return governance_rig.enrollment_service_with_failing_outbox()


@pytest.fixture
def enrollment_quote(governance_rig): return governance_rig.enrollment_quote()


@pytest.fixture
def customer_billing_route(governance_rig):
    return governance_rig.customer_billing_route()


@pytest.fixture
def internal_billing_route(governance_rig):
    return governance_rig.internal_billing_route()


@pytest.fixture
def customer_project(governance_rig): return governance_rig.customer_project()


@pytest.fixture
def platform_project(governance_rig): return governance_rig.platform_project()


@pytest.fixture
def transactional_session(governance_rig) -> Iterator[Session]:
    with governance_rig.transactional_session() as session:
        yield session


@pytest.fixture
def enrollment_factory(governance_rig): return governance_rig.enrollment_factory()


@pytest.fixture
def governed_generation_cases(governance_rig):
    return governance_rig.governed_generation_cases()


@pytest.fixture
def governed_generation_with_failing_outbox(governance_rig):
    return governance_rig.governed_generation_with_failing_outbox()


@pytest.fixture
def race_harness(governance_rig): return governance_rig.race_harness()


@pytest.fixture
def governed_generation_factory(governance_rig):
    return governance_rig.governed_generation_factory()


@pytest.fixture
def export_harness(governance_rig): return governance_rig.export_harness()


@pytest.fixture
def complaint_service(governance_rig): return governance_rig.complaint_service()


@pytest.fixture
def unauthorized_identity_command(governance_rig):
    return governance_rig.unauthorized_identity_command()


@pytest.fixture
def command_factory(governance_rig): return governance_rig.command_factory()


@pytest.fixture
def deletion_harness(governance_rig): return governance_rig.deletion_harness()


@pytest.fixture
def real_restore_harness(governance_rig):
    return governance_rig.real_restore_harness()


@pytest.fixture
def self_marketing_harness(governance_rig):
    return governance_rig.self_marketing_harness()


@pytest.fixture
def app_openapi():
    from ip_saas.api import app
    return app.openapi()


@pytest.fixture
def event_schemas() -> dict[str, object]:
    root = Path(__file__).parents[3] / "contracts" / "events"
    return {
        path.name: loads(path.read_text(encoding="utf-8"))
        for path in sorted(root.glob("*.json"))
    }


@pytest.fixture
def boundary_harness(governance_rig): return governance_rig.boundary_harness()


@pytest.fixture
def redaction_harness(governance_rig): return governance_rig.redaction_harness()


@pytest.fixture
def governance_api(governance_rig): return governance_rig.governance_api()


@pytest.fixture
def governance_fault_harness(governance_rig):
    return governance_rig.governance_fault_harness()


@pytest.fixture
def passing_beta_input(governance_rig): return governance_rig.passing_beta_input()


@pytest.fixture
def evaluator():
    from ip_saas.modules.governance.beta import PublicBetaEvaluator
    return PublicBetaEvaluator()


@pytest.fixture
def release_harness(governance_rig): return governance_rig.release_harness()
~~~

Create backend/tests/contract/governance/test_fixture_closure.py so future edits cannot reintroduce a hidden fixture:

~~~python
import ast
from pathlib import Path

from ip_saas.modules.billing.limits import GenerationLimitService


PYTEST_BUILTINS = frozenset({
    "cache", "capfd", "capfdbinary", "caplog", "capsys", "capsysbinary",
    "doctest_namespace", "monkeypatch", "pytestconfig", "record_property",
    "record_testsuite_property", "record_xml_attribute", "recwarn", "request",
    "tmp_path", "tmp_path_factory", "tmpdir", "tmpdir_factory",
})
PLAN06_TEST_GLOBS = (
    "unit/governance/test_*.py",
    "integration/governance/test_*.py",
    "contract/governance/test_*.py",
    "security/test_identity_boundaries.py",
    "security/test_secret_and_log_redaction.py",
    "security/test_governance_ssrf.py",
    "resilience/test_governance_faults.py",
    "resilience/test_deleted_data_not_restored.py",
)


def _decorator_name(node: ast.expr) -> str:
    target = node.func if isinstance(node, ast.Call) else node
    parts: list[str] = []
    while isinstance(target, ast.Attribute):
        parts.append(target.attr)
        target = target.value
    if isinstance(target, ast.Name):
        parts.append(target.id)
    return ".".join(reversed(parts))


def _parametrize_names(node: ast.FunctionDef) -> set[str]:
    names: set[str] = set()
    for decorator in node.decorator_list:
        if not isinstance(decorator, ast.Call):
            continue
        if _decorator_name(decorator) != "pytest.mark.parametrize" or not decorator.args:
            continue
        value = decorator.args[0]
        if isinstance(value, ast.Constant) and isinstance(value.value, str):
            names.update(item.strip() for item in value.value.strip("()").split(","))
        elif isinstance(value, (ast.Tuple, ast.List)):
            names.update(
                item.value for item in value.elts
                if isinstance(item, ast.Constant) and isinstance(item.value, str)
            )
    return names


def test_every_test_argument_has_an_explicit_fixture_definition() -> None:
    tests_root = Path(__file__).parents[2]
    fixtures: set[str] = set()
    test_arguments: list[tuple[Path, int, str, set[str]]] = []
    paths = {tests_root / "conftest.py", tests_root / "support/governance_fixtures.py"}
    for pattern in PLAN06_TEST_GLOBS:
        paths.update(tests_root.glob(pattern))
    for path in sorted(paths):
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        for node in ast.walk(tree):
            if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                continue
            if any(_decorator_name(item) == "pytest.fixture" for item in node.decorator_list):
                fixtures.add(node.name)
            if node.name.startswith("test_"):
                arguments = {item.arg for item in node.args.args} - {"self"}
                arguments -= _parametrize_names(node)
                test_arguments.append((path, node.lineno, node.name, arguments))
    allowed = fixtures | PYTEST_BUILTINS
    missing = [
        f"{path.relative_to(tests_root)}:{line} {name}: {sorted(arguments - allowed)}"
        for path, line, name, arguments in test_arguments
        if arguments - allowed
    ]
    assert missing == [], "undeclared pytest fixtures:\n" + "\n".join(missing)


def test_every_billing_service_composition_passes_mandatory_limits() -> None:
    tests_root = Path(__file__).parents[2]
    backend_root = tests_root.parent
    paths = list((backend_root / "src/ip_saas").rglob("*.py"))
    paths.append(tests_root / "support/governance_rig.py")
    calls: list[tuple[Path, int, ast.Call]] = []
    for path in paths:
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        for node in ast.walk(tree):
            if isinstance(node, ast.Call) and _decorator_name(node.func) == "BillingService":
                calls.append((path, node.lineno, node))
    assert calls, "no BillingService composition was inspected"
    bad = []
    for path, line, call in calls:
        keyword = next(
            (item for item in call.keywords if item.arg == "generation_limits"),
            None,
        )
        has_third = len(call.args) >= 3 or keyword is not None
        explicit_none = (
            len(call.args) >= 3
            and isinstance(call.args[2], ast.Constant)
            and call.args[2].value is None
        ) or (
            keyword is not None
            and isinstance(keyword.value, ast.Constant)
            and keyword.value.value is None
        )
        if not has_third or explicit_none:
            bad.append(f"{path.relative_to(backend_root)}:{line}")
    assert bad == [], "BillingService missing explicit limits: " + ", ".join(bad)
    forbidden_allow_fake_paths = [
        *(backend_root / "src/ip_saas").rglob("*.py"),
        *(tests_root / "integration/governance").rglob("*.py"),
        *(tests_root / "load").rglob("*.py"),
        *(tests_root / "resilience").rglob("*.py"),
        tests_root / "support/governance_rig.py",
    ]
    leaked_allow_fake = [
        str(path.relative_to(backend_root))
        for path in forbidden_allow_fake_paths
        if path.exists()
        and "AllowAllGenerationLimits" in path.read_text(encoding="utf-8")
    ]
    assert leaked_allow_fake == [], (
        "Plan 01 allow fake escaped unit tests: " + ", ".join(leaked_allow_fake)
    )


def test_governance_rig_uses_persisted_customer_and_internal_limits(
    governance_rig,
) -> None:
    case = governance_rig.generation_limit_composition_case()
    assert isinstance(case.billing.generation_limits, GenerationLimitService)
    assert case.customer_limit.account_id == case.customer_account.id
    assert case.customer_limit.cost_center_id is None
    assert case.internal_limit.account_id is None
    assert case.internal_limit.cost_center_id == case.cost_center.id
    assert case.customer_limit.effective_at <= case.clock.now()
    assert case.internal_limit.effective_at <= case.clock.now()
~~~

Create backend/tests/support/governance_rig.py with a concrete `GovernanceTestRig` exposing every method called above. Each method builds committed models and the real service graph for its owning task; only Clock, provider transport, TOS, Redis, RocketMQ, and the intentionally exploding Outbox are fakes. The rig uses the injected PostgreSQL Session/Engine, unique account/project/idempotency values per scenario, TaskSubmissionService for ordinary tasks, and `request_fingerprint="0" * 64` for the few isolated TaskRecord constructor probes. It never returns `SimpleNamespace`, a fixture from an undeclared plugin, SQLite state, or a precomputed assertion result. Keep each scenario builder beside its returned typed harness in this one file so the fixture name, setup, behavior, and cleanup are copyable together.

Every rig scenario that may reserve billing must use this composition shape, with real persisted versions rather than an unlimited fake:

~~~python
self.generation_limits = GenerationLimitService(self.clock)
self.billing = BillingService(
    self.credit_holds,
    self.internal_budget_holds,
    self.generation_limits,
)


def seed_customer_limit(self, admin, account):
    return self.generation_limits.create_account_version(
        self.session, admin, account.id,
        per_task_units=10_000, per_day_units=100_000,
        per_month_units=1_000_000, effective_at=self.clock.now(),
    )


def seed_internal_limit(self, admin, cost_center):
    return self.generation_limits.create_internal_version(
        self.session, admin, cost_center.id,
        per_task_fen=1_000_000, per_day_fen=10_000_000,
        per_month_fen=100_000_000, effective_at=self.clock.now(),
    )
~~~

`generation_limit_composition_case()` creates an actual customer account, platform cost center, both current limit rows, and the three-argument BillingService, then returns those persisted objects for inspection. Enrollment, governed-generation, quota-idempotency, production-PoC, and Beta-evidence builders call the matching seed method before the first TaskSubmissionService call. A focused unit test may explicitly pass Plan 01's `AllowAllGenerationLimits()` as the third constructor argument; it is forbidden from integration, PoC, load, resilience, or Beta evidence builders.

The idempotency harness methods used later are also real PostgreSQL scenarios. `enrollment_factory.for_account`, `for_second_project`, and `as_actor` create distinct persisted accounts, projects, subjects, consents, rights, wallets, and current limit versions. `governed_generation_factory.replay_with` changes exactly one of script, identity ID, or server quote while preserving the client key. `quota_idempotency_harness` creates two accounts plus two authorized projects for account A. Each `durable_side_effect_counts()` performs fresh scoped SQL counts of TaskRecord, holds, reservations, GenerationIdentityUse, ConsentRecord.usage_count, and outbox rows; flags such as `foreign_existing_aggregate_was_returned` are set only by observing an actual returned object, not precomputed. These builders assert the server-created enrollment/job/task IDs inside outbox idempotency keys and never derive an outbox key directly from the client key.

Run the closure check:

~~~bash
cd backend && uv run pytest tests/contract/governance/test_fixture_closure.py -v
cd backend && uv run pytest --fixtures -q | rg "^(governance_rig|eligibility|production_adapter_harness|enrollment_factory|governed_generation_factory|deletion_harness|self_marketing_harness|boundary_harness|governance_fault_harness|release_harness)"
~~~

Expected: the AST closure test passes and all ten sampled names print exactly once. The final static audit in Task 16 reruns this file; it removes names supplied by `@pytest.mark.parametrize`, permits only core pytest built-ins, and fails if any remaining parameter lacks an explicit local or conftest fixture definition. Plan 01 `db_session` and `pg_engine` pass because they are visibly defined in backend/tests/conftest.py, not because they are allow-listed.

- [ ] **Step 6: Run the schema tests**

Run:

~~~bash
cd backend && uv run pytest tests/unit/governance/test_schemas.py -v
~~~

Expected: PASS, 6 tests, including the V1 single-identity and one-source voice boundaries.

- [ ] **Step 7: Commit the frozen governance vocabulary and fixture registry**

~~~bash
git add backend/src/ip_saas/modules/governance backend/tests/__init__.py backend/tests/conftest.py backend/tests/support backend/tests/unit/governance/test_schemas.py backend/tests/contract/governance/test_fixture_closure.py
git commit -m "feat: freeze digital identity beta contracts"
~~~

### Task 2: Persist immutable gate evidence and revocable identity state

**Files:**
- Create: backend/src/ip_saas/modules/governance/models.py
- Modify: backend/src/ip_saas/modules/governance/enums.py
- Modify: backend/src/ip_saas/modules/media/models/jobs.py
- Modify: backend/src/ip_saas/modules/media/models/__init__.py
- Test: backend/tests/unit/governance/test_model_shape.py

- [ ] **Step 1: Write the failing model-boundary tests**

Create backend/tests/unit/governance/test_model_shape.py:

~~~python
from ip_saas.db.base import Base
from ip_saas.modules.governance.models import (
    BetaBlindReviewResult,
    CapabilityGateAttestation,
    CapabilityFeatureFlag,
    ConsentRecord,
    DeletionExecutionStep,
    DeletionPlan,
    ExportVerification,
    GenerationIdentityUse,
    ProviderCapabilityEntitlement,
    ProviderIdentityAsset,
    ReleaseDecision,
    SelfMarketingRiskReview,
)
from ip_saas.modules.media.models.jobs import MediaAsset


def test_provider_entitlement_and_identity_asset_store_no_plain_secret() -> None:
    entitlement_columns = set(ProviderCapabilityEntitlement.__table__.columns.keys())
    identity_columns = set(ProviderIdentityAsset.__table__.columns.keys())
    forbidden = {
        "access_key",
        "secret_key",
        "api_key",
        "access_token",
        "provider_asset_id",
        "speaker_id",
    }
    assert entitlement_columns.isdisjoint(forbidden)
    assert identity_columns.isdisjoint(forbidden)
    assert {
        "provider_resource_ref_ciphertext",
        "provider_resource_ref_fingerprint",
    } <= identity_columns


def test_gate_and_release_decision_freeze_the_exact_entitlement_binding() -> None:
    binding_columns = {
        "entitlement_id", "provider_account_fingerprint", "region",
        "model_id", "model_version", "release_snapshot_sha256",
    }
    entitlement_columns = set(
        ProviderCapabilityEntitlement.__table__.columns.keys()
    )
    assert {
        "provider_account_fingerprint", "region", "model_id", "model_version",
        "release_snapshot_sha256",
    } <= entitlement_columns
    assert binding_columns <= set(
        CapabilityGateAttestation.__table__.columns.keys()
    )
    assert binding_columns <= set(ReleaseDecision.__table__.columns.keys())


def test_consent_is_one_subject_and_one_kind() -> None:
    columns = set(ConsentRecord.__table__.columns.keys())
    assert {"subject_id", "kind", "terms_snapshot_sha256", "withdrawn_at"} <= columns
    assert "consent_bundle" not in columns


def test_generation_identity_lineage_is_separate_from_the_media_job_hold() -> None:
    use_columns = set(GenerationIdentityUse.__table__.columns.keys())
    assert {
        "generation_job_id",
        "identity_asset_id",
        "consent_snapshot_sha256s",
        "rights_snapshot_sha256s",
    } <= use_columns
    assert "billing_hold_id" not in use_columns


def test_enrollment_and_quota_idempotency_are_scoped_to_account() -> None:
    expected = {
        "identity_enrollments": "uq_identity_enrollment_account_idempotency",
        "capability_quota_reservations": (
            "uq_capability_quota_account_idempotency"
        ),
    }
    for table_name, constraint_name in expected.items():
        table = Base.metadata.tables[table_name]
        matches = [
            constraint for constraint in table.constraints
            if constraint.name == constraint_name
        ]
        assert len(matches) == 1
        assert tuple(column.name for column in matches[0].columns) == (
            "account_id", "idempotency_key",
        )
        assert not any(
            constraint.name is None
            and tuple(column.name for column in constraint.columns)
            == ("idempotency_key",)
            for constraint in table.constraints
        )


def test_every_export_verifies_the_exact_asset_hash() -> None:
    columns = set(ExportVerification.__table__.columns.keys())
    assert {
        "media_asset_id",
        "asset_sha256",
        "explicit_label_passed",
        "implicit_label_passed",
        "overall_passed",
    } <= columns
    assert "label_removal_allowed" not in columns


def test_media_asset_can_be_quarantined_without_deleting_history() -> None:
    columns = set(MediaAsset.__table__.columns.keys())
    assert {
        "quarantined_at",
        "quarantine_reason",
        "deleted_at",
        "deletion_reason",
    } <= columns
    assert set(CapabilityFeatureFlag.__table__.columns.keys()) >= {
        "capability",
        "stage",
        "version_no",
    }


def test_global_delete_plan_covers_live_provider_cache_and_backup_state() -> None:
    plan_columns = set(DeletionPlan.__table__.columns.keys())
    step_columns = set(DeletionExecutionStep.__table__.columns.keys())
    assert {
        "target_type",
        "target_id",
        "restore_until",
        "permanent_confirmation_sha256",
        "backup_expire_by",
    } <= plan_columns
    assert {"system", "status", "attempt_no", "evidence_sha256"} <= step_columns


def test_blind_review_and_self_marketing_separation_are_persisted() -> None:
    blind_columns = set(BetaBlindReviewResult.__table__.columns.keys())
    review_columns = set(SelfMarketingRiskReview.__table__.columns.keys())
    assert {
        "operator_scores",
        "director_scores",
        "winner",
        "blinded_order_sha256",
    } <= blind_columns
    assert {
        "operator_actor_id",
        "reviewer_actor_id",
        "budget_approver_actor_id",
        "triggered_risks",
    } <= review_columns
~~~

- [ ] **Step 2: Run the model tests and verify the missing-model failure**

Run:

~~~bash
cd backend && uv run pytest tests/unit/governance/test_model_shape.py -v
~~~

Expected: FAIL during collection because governance.models does not exist.

- [ ] **Step 3: Add launch, drill, and review enums**

Append to backend/src/ip_saas/modules/governance/enums.py:

~~~python
class EntitlementStatus(StrEnum):
    UNVERIFIED = "unverified"
    ACTIVE = "active"
    SUSPENDED = "suspended"
    EXPIRED = "expired"


class QuotaReservationStatus(StrEnum):
    ACTIVE = "active"
    SETTLED = "settled"
    RELEASED = "released"


class OperationalDrillKind(StrEnum):
    DATABASE_RESTORE = "database_restore"
    TOS_RESTORE = "tos_restore"
    FULL_DISASTER_RECOVERY = "full_disaster_recovery"
    SECURITY = "security"
    LOAD = "load"
    FAULT_INJECTION = "fault_injection"


class OperationalResult(StrEnum):
    PASS = "pass"
    FAIL = "fail"


class LaunchAttestationTopic(StrEnum):
    APPLICATION_OR_ALGORITHM_FILING = "application_or_algorithm_filing"
    SECURITY_ASSESSMENT = "security_assessment"
    TELECOM_LICENSE = "telecom_license"
    ADVERTISING_OBLIGATIONS = "advertising_obligations"
    DIGITAL_IDENTITY_COMMERCIAL_RIGHTS = "digital_identity_commercial_rights"
    RESELLER_CONTRACT_TAX_CONSUMER = "reseller_contract_tax_consumer"
    MLPS_CLASSIFICATION = "mlps_classification"
    MINOR_BOUNDARY = "minor_boundary"


class BetaProgramStatus(StrEnum):
    DRAFT = "draft"
    FROZEN = "frozen"
    RUNNING = "running"
    EVALUATED = "evaluated"
    RELEASED = "released"
    HELD = "held"
    ROLLED_BACK = "rolled_back"


class ScriptRewriteClass(StrEnum):
    NONE = "none"
    MINOR = "minor"
    STRUCTURAL = "structural"
~~~

- [ ] **Step 4: Create feature, entitlement, contract, and gate models**

Create backend/src/ip_saas/modules/governance/models.py with the following imports and first model group:

~~~python
from datetime import datetime
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import (
    BigInteger,
    Boolean,
    CheckConstraint,
    DateTime,
    ForeignKey,
    Index,
    Integer,
    LargeBinary,
    String,
    Text,
    UniqueConstraint,
    event,
    func,
    text,
)
from sqlalchemy.dialects.postgresql import ARRAY, JSONB, UUID as PGUUID
from sqlalchemy.orm import Mapped, mapped_column

from ip_saas.db.base import Base

from .enums import (
    BetaProgramStatus,
    CapabilityStage,
    ComplaintStatus,
    ConsentStatus,
    DeletionStatus,
    EntitlementStatus,
    EnrollmentStatus,
    GateResult,
    IdentityAssetStatus,
    QuotaReservationStatus,
)


class CapabilityFeatureFlag(Base):
    __tablename__ = "capability_feature_flags"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    capability: Mapped[str] = mapped_column(String(60), nullable=False, unique=True)
    stage: Mapped[str] = mapped_column(
        String(30), nullable=False, default=CapabilityStage.OFF.value
    )
    version_no: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    reason: Mapped[str] = mapped_column(String(500), nullable=False)
    changed_by: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    changed_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)

    __table_args__ = (
        CheckConstraint(
            "stage IN ('off','internal_poc','closed_beta','public_beta')",
            name="ck_capability_feature_stage",
        ),
        CheckConstraint("version_no > 0", name="ck_capability_feature_version"),
    )


class CapabilityCohortGrant(Base):
    __tablename__ = "capability_cohort_grants"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    capability: Mapped[str] = mapped_column(String(60), nullable=False, index=True)
    account_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("accounts.id"), nullable=False, index=True
    )
    project_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("ip_projects.id"), nullable=False, index=True
    )
    allowed_until: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    granted_by: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    granted_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    revoked_by: Mapped[UUID | None] = mapped_column(PGUUID(as_uuid=True))
    revoked_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    reason: Mapped[str] = mapped_column(String(500), nullable=False)

    __table_args__ = (
        Index(
            "uq_active_capability_project_grant",
            "capability",
            "project_id",
            unique=True,
            postgresql_where=text("revoked_at IS NULL"),
        ),
    )


class CommercialContractAttestation(Base):
    __tablename__ = "commercial_contract_attestations"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    capability: Mapped[str] = mapped_column(String(60), nullable=False, index=True)
    contract_sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    evidence_asset_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id"), nullable=False
    )
    platform_may_generate: Mapped[bool] = mapped_column(Boolean, nullable=False)
    commercial_distribution_allowed: Mapped[bool] = mapped_column(Boolean, nullable=False)
    effective_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    expires_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    approved_by: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    independently_reviewed_by: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), nullable=False
    )
    approved_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)

    __table_args__ = (
        CheckConstraint("length(contract_sha256) = 64", name="ck_contract_hash"),
        CheckConstraint("expires_at > effective_at", name="ck_contract_validity"),
        CheckConstraint(
            "approved_by <> independently_reviewed_by",
            name="ck_contract_independent_review",
        ),
    )


class ProviderCapabilityEntitlement(Base):
    __tablename__ = "provider_capability_entitlements"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    capability: Mapped[str] = mapped_column(String(60), nullable=False, index=True)
    provider: Mapped[str] = mapped_column(String(60), nullable=False)
    region: Mapped[str] = mapped_column(String(40), nullable=False)
    provider_account_fingerprint: Mapped[str] = mapped_column(
        String(64), nullable=False
    )
    model_id: Mapped[str] = mapped_column(String(200), nullable=False)
    model_version: Mapped[str] = mapped_column(String(100), nullable=False)
    release_snapshot_sha256: Mapped[str] = mapped_column(
        String(64), nullable=False
    )
    status: Mapped[str] = mapped_column(
        String(30), nullable=False, default=EntitlementStatus.UNVERIFIED.value
    )
    quota_per_minute: Mapped[int] = mapped_column(Integer, nullable=False)
    quota_per_day: Mapped[int] = mapped_column(Integer, nullable=False)
    quota_per_month: Mapped[int] = mapped_column(Integer, nullable=False)
    contract_attestation_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True),
        ForeignKey("commercial_contract_attestations.id"),
        nullable=False,
    )
    evidence_asset_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id"), nullable=False
    )
    checked_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    expires_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    checked_by: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)

    __table_args__ = (
        UniqueConstraint(
            "capability",
            "provider",
            "region",
            "provider_account_fingerprint",
            "model_id",
            "model_version",
            "release_snapshot_sha256",
            name="uq_provider_capability_account",
        ),
        CheckConstraint(
            "quota_per_minute > 0 AND quota_per_day > 0 AND quota_per_month > 0",
            name="ck_provider_entitlement_quotas",
        ),
        CheckConstraint("expires_at > checked_at", name="ck_entitlement_validity"),
        CheckConstraint(
            "length(provider_account_fingerprint) = 64 AND "
            "length(release_snapshot_sha256) = 64",
            name="ck_entitlement_binding_hashes",
        ),
    )


class CapabilityGateAttestation(Base):
    __tablename__ = "capability_gate_attestations"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    capability: Mapped[str] = mapped_column(String(60), nullable=False, index=True)
    gate_kind: Mapped[str] = mapped_column(String(60), nullable=False, index=True)
    result: Mapped[str] = mapped_column(String(20), nullable=False)
    entitlement_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True),
        ForeignKey("provider_capability_entitlements.id"),
        nullable=False,
    )
    provider_account_fingerprint: Mapped[str] = mapped_column(
        String(64), nullable=False
    )
    region: Mapped[str] = mapped_column(String(40), nullable=False)
    model_id: Mapped[str] = mapped_column(String(200), nullable=False)
    model_version: Mapped[str] = mapped_column(String(100), nullable=False)
    release_snapshot_sha256: Mapped[str] = mapped_column(
        String(64), nullable=False
    )
    evidence_asset_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id"), nullable=False
    )
    evidence_sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    test_run_ref: Mapped[str] = mapped_column(String(200), nullable=False)
    measurements: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False)
    executed_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    valid_until: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    signed_by_operator: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    signed_by_reviewer: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)

    __table_args__ = (
        UniqueConstraint(
            "capability", "gate_kind", "test_run_ref", name="uq_capability_gate_run"
        ),
        CheckConstraint(
            "result IN ('pass','fail','expired')", name="ck_capability_gate_result"
        ),
        CheckConstraint("length(evidence_sha256) = 64", name="ck_gate_evidence_hash"),
        CheckConstraint(
            "length(provider_account_fingerprint) = 64 AND "
            "length(release_snapshot_sha256) = 64",
            name="ck_gate_binding_hashes",
        ),
        CheckConstraint("valid_until > executed_at", name="ck_gate_validity"),
        CheckConstraint(
            "signed_by_operator <> signed_by_reviewer",
            name="ck_gate_independent_signatures",
        ),
    )
~~~

- [ ] **Step 5: Add subject, consent, enrollment, and provider identity models**

Append to backend/src/ip_saas/modules/governance/models.py:

~~~python
class IdentitySubject(Base):
    __tablename__ = "identity_subjects"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    account_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("accounts.id"), nullable=False, index=True
    )
    project_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("ip_projects.id"), nullable=False, index=True
    )
    pseudonym: Mapped[str] = mapped_column(String(120), nullable=False)
    age_class: Mapped[str] = mapped_column(String(20), nullable=False)
    guardian_subject_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("identity_subjects.id")
    )
    age_verification_method: Mapped[str] = mapped_column(String(80), nullable=False)
    age_verification_evidence_asset_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id"), nullable=False
    )
    verified_by: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    verified_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    disabled_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))

    __table_args__ = (
        CheckConstraint(
            "age_class IN ('adult','minor','unknown')", name="ck_identity_subject_age"
        ),
        CheckConstraint(
            "(age_class = 'minor' AND guardian_subject_id IS NOT NULL) OR "
            "(age_class <> 'minor' AND guardian_subject_id IS NULL)",
            name="ck_identity_subject_guardian",
        ),
    )


class ConsentRecord(Base):
    __tablename__ = "consent_records"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    account_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("accounts.id"), nullable=False, index=True
    )
    project_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("ip_projects.id"), nullable=False, index=True
    )
    subject_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("identity_subjects.id"), nullable=False, index=True
    )
    kind: Mapped[str] = mapped_column(String(40), nullable=False)
    terms_version: Mapped[str] = mapped_column(String(100), nullable=False)
    terms_snapshot_sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    purposes: Mapped[list[str]] = mapped_column(ARRAY(String(80)), nullable=False)
    platforms: Mapped[list[str]] = mapped_column(ARRAY(String(40)), nullable=False)
    territory: Mapped[str] = mapped_column(String(40), nullable=False)
    valid_from: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    valid_until: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    usage_limit: Mapped[int] = mapped_column(Integer, nullable=False)
    usage_count: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    commercial_use: Mapped[bool] = mapped_column(Boolean, nullable=False)
    modeling_allowed: Mapped[bool] = mapped_column(Boolean, nullable=False)
    reuse_allowed: Mapped[bool] = mapped_column(Boolean, nullable=False)
    provider_transfer_allowed: Mapped[bool] = mapped_column(Boolean, nullable=False)
    existing_output_policy: Mapped[str] = mapped_column(String(80), nullable=False)
    verification_method: Mapped[str] = mapped_column(String(80), nullable=False)
    verification_evidence_asset_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id"), nullable=False
    )
    status: Mapped[str] = mapped_column(
        String(30), nullable=False, default=ConsentStatus.PENDING_VERIFICATION.value
    )
    granted_by: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    granted_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    withdrawn_by: Mapped[UUID | None] = mapped_column(PGUUID(as_uuid=True))
    withdrawn_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    withdrawal_reason: Mapped[str | None] = mapped_column(String(500))

    __table_args__ = (
        CheckConstraint("length(terms_snapshot_sha256) = 64", name="ck_consent_terms_hash"),
        CheckConstraint("valid_until > valid_from", name="ck_consent_validity"),
        CheckConstraint(
            "usage_limit > 0 AND usage_count >= 0 AND usage_count <= usage_limit",
            name="ck_consent_usage",
        ),
        Index(
            "uq_active_consent_subject_kind",
            "subject_id",
            "kind",
            unique=True,
            postgresql_where=text("status = 'active'"),
        ),
    )


class IdentityEnrollment(Base):
    __tablename__ = "identity_enrollments"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    task_record_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("task_records.id"), nullable=False, unique=True
    )
    account_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("accounts.id"), nullable=False, index=True
    )
    project_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("ip_projects.id"), nullable=False, index=True
    )
    subject_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("identity_subjects.id"), nullable=False
    )
    identity_kind: Mapped[str] = mapped_column(String(40), nullable=False)
    provider: Mapped[str] = mapped_column(String(60), nullable=False)
    source_media_asset_ids: Mapped[list[str]] = mapped_column(JSONB, nullable=False)
    rights_snapshot_sha256s: Mapped[list[str]] = mapped_column(JSONB, nullable=False)
    status: Mapped[str] = mapped_column(
        String(30), nullable=False, default=EnrollmentStatus.QUEUED.value
    )
    provider_task_id: Mapped[str | None] = mapped_column(String(300))
    idempotency_key: Mapped[str] = mapped_column(String(160), nullable=False)
    last_error_code: Mapped[str | None] = mapped_column(String(100))
    last_error_message: Mapped[str | None] = mapped_column(String(500))
    initiated_by_actor_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now(), onupdate=func.now()
    )

    __table_args__ = (
        UniqueConstraint(
            "account_id",
            "idempotency_key",
            name="uq_identity_enrollment_account_idempotency",
        ),
        Index(
            "uq_identity_enrollment_provider_task",
            "provider",
            "provider_task_id",
            unique=True,
            postgresql_where=text("provider_task_id IS NOT NULL"),
        ),
    )


class ProviderIdentityAsset(Base):
    __tablename__ = "provider_identity_assets"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    account_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("accounts.id"), nullable=False, index=True
    )
    project_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("ip_projects.id"), nullable=False, index=True
    )
    subject_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("identity_subjects.id"), nullable=False, index=True
    )
    identity_kind: Mapped[str] = mapped_column(String(40), nullable=False)
    provider: Mapped[str] = mapped_column(String(60), nullable=False)
    enrollment_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("identity_enrollments.id"), nullable=False, unique=True
    )
    status: Mapped[str] = mapped_column(
        String(30), nullable=False, default=IdentityAssetStatus.ENROLLING.value
    )
    provider_resource_ref_ciphertext: Mapped[bytes | None] = mapped_column(LargeBinary)
    provider_resource_ref_fingerprint: Mapped[str | None] = mapped_column(String(64))
    character_profile_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("character_profiles.id")
    )
    activated_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    frozen_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    freeze_reason: Mapped[str | None] = mapped_column(String(500))
    deleted_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )

    __table_args__ = (
        CheckConstraint(
            "(status = 'active' AND provider_resource_ref_ciphertext IS NOT NULL "
            "AND provider_resource_ref_fingerprint IS NOT NULL) OR "
            "(status <> 'active')",
            name="ck_identity_asset_active_reference",
        ),
    )


class IdentityAssetConsent(Base):
    __tablename__ = "identity_asset_consents"

    identity_asset_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True),
        ForeignKey("provider_identity_assets.id"),
        primary_key=True,
    )
    consent_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("consent_records.id"), primary_key=True
    )
    consent_snapshot_sha256: Mapped[str] = mapped_column(String(64), nullable=False)

    __table_args__ = (
        CheckConstraint(
            "length(consent_snapshot_sha256) = 64", name="ck_identity_consent_hash"
        ),
    )
~~~

- [ ] **Step 6: Add generation lineage, durable quota, deletion, complaint, and export models**

Append to backend/src/ip_saas/modules/governance/models.py:

~~~python
class GenerationIdentityUse(Base):
    __tablename__ = "generation_identity_uses"

    generation_job_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("generation_jobs.id"), primary_key=True
    )
    identity_asset_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True),
        ForeignKey("provider_identity_assets.id"),
        primary_key=True,
    )
    consent_snapshot_sha256s: Mapped[list[str]] = mapped_column(JSONB, nullable=False)
    rights_snapshot_sha256s: Mapped[list[str]] = mapped_column(JSONB, nullable=False)
    platform: Mapped[str] = mapped_column(String(40), nullable=False)
    used_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    delivery_blocked_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    delivery_block_reason: Mapped[str | None] = mapped_column(String(500))


class CapabilityQuotaReservation(Base):
    __tablename__ = "capability_quota_reservations"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    capability: Mapped[str] = mapped_column(String(60), nullable=False, index=True)
    account_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("accounts.id"), nullable=False, index=True
    )
    project_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("ip_projects.id"), nullable=False, index=True
    )
    task_record_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("task_records.id")
    )
    reserved_units: Mapped[int] = mapped_column(Integer, nullable=False)
    status: Mapped[str] = mapped_column(
        String(20), nullable=False, default=QuotaReservationStatus.ACTIVE.value
    )
    idempotency_key: Mapped[str] = mapped_column(String(160), nullable=False)
    reserved_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    expires_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    settled_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    released_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))

    __table_args__ = (
        UniqueConstraint(
            "account_id",
            "idempotency_key",
            name="uq_capability_quota_account_idempotency",
        ),
        CheckConstraint("reserved_units > 0", name="ck_capability_quota_units"),
        CheckConstraint("expires_at > reserved_at", name="ck_capability_quota_expiry"),
    )


class IdentityDeletionRequest(Base):
    __tablename__ = "identity_deletion_requests"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    account_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("accounts.id"), nullable=False, index=True
    )
    project_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("ip_projects.id"), nullable=False, index=True
    )
    subject_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("identity_subjects.id"), nullable=False
    )
    identity_asset_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("provider_identity_assets.id"), nullable=False
    )
    status: Mapped[str] = mapped_column(
        String(30), nullable=False, default=DeletionStatus.REQUESTED.value
    )
    requested_by: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    requested_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    provider_receipt_sha256: Mapped[str | None] = mapped_column(String(64))
    provider_receipt_evidence_asset_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id")
    )
    live_data_deleted_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    verified_by: Mapped[UUID | None] = mapped_column(PGUUID(as_uuid=True))
    verified_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    failure_message: Mapped[str | None] = mapped_column(String(500))

    __table_args__ = (
        Index(
            "uq_open_identity_deletion",
            "identity_asset_id",
            unique=True,
            postgresql_where=text(
                "status IN ('requested','provider_pending','live_data_deleted')"
            ),
        ),
    )


class DeletionPlan(Base):
    """Recoverable live-data deletion followed by independently confirmed purge."""

    __tablename__ = "deletion_plans"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    account_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("accounts.id"), nullable=False, index=True
    )
    project_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("ip_projects.id"), nullable=False, index=True
    )
    target_type: Mapped[str] = mapped_column(String(50), nullable=False)
    target_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    status: Mapped[str] = mapped_column(String(30), nullable=False, default="trashed")
    deletion_manifest: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False)
    manifest_sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    trashed_by: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    trashed_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    restore_until: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    restored_by: Mapped[UUID | None] = mapped_column(PGUUID(as_uuid=True))
    restored_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    permanent_confirmation_sha256: Mapped[str | None] = mapped_column(String(64))
    purge_requested_by: Mapped[UUID | None] = mapped_column(PGUUID(as_uuid=True))
    purge_requested_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    backup_expire_by: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    completed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    completion_evidence_asset_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id")
    )
    completion_evidence_sha256: Mapped[str | None] = mapped_column(String(64))
    failure_message: Mapped[str | None] = mapped_column(String(500))

    __table_args__ = (
        CheckConstraint("restore_until > trashed_at", name="ck_delete_restore_window"),
        CheckConstraint("backup_expire_by >= restore_until", name="ck_delete_backup_expiry"),
        CheckConstraint("length(manifest_sha256) = 64", name="ck_delete_manifest_hash"),
        Index(
            "uq_open_deletion_target",
            "target_type",
            "target_id",
            unique=True,
            postgresql_where=text(
                "status IN ('trashed','purge_pending','purging','failed')"
            ),
        ),
    )


class DeletionExecutionStep(Base):
    __tablename__ = "deletion_execution_steps"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    deletion_plan_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("deletion_plans.id"), nullable=False, index=True
    )
    system: Mapped[str] = mapped_column(String(40), nullable=False)
    status: Mapped[str] = mapped_column(String(40), nullable=False)
    attempt_no: Mapped[int] = mapped_column(Integer, nullable=False)
    started_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    completed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    evidence_asset_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id")
    )
    evidence_sha256: Mapped[str | None] = mapped_column(String(64))
    sanitized_error: Mapped[str | None] = mapped_column(String(500))

    __table_args__ = (
        UniqueConstraint(
            "deletion_plan_id", "system", "attempt_no", name="uq_deletion_step_attempt"
        ),
        CheckConstraint("attempt_no > 0", name="ck_deletion_step_attempt"),
    )


class ComplaintCase(Base):
    __tablename__ = "complaint_cases"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    public_reference: Mapped[str] = mapped_column(String(32), nullable=False, unique=True)
    project_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("ip_projects.id"), index=True
    )
    content_id: Mapped[UUID | None] = mapped_column(PGUUID(as_uuid=True), index=True)
    identity_asset_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("provider_identity_assets.id"), index=True
    )
    category: Mapped[str] = mapped_column(String(60), nullable=False)
    severity: Mapped[str] = mapped_column(String(20), nullable=False)
    status: Mapped[str] = mapped_column(
        String(30), nullable=False, default=ComplaintStatus.RECEIVED.value
    )
    description: Mapped[str] = mapped_column(Text, nullable=False)
    evidence_asset_ids: Mapped[list[str]] = mapped_column(JSONB, nullable=False)
    contact_ciphertext: Mapped[bytes] = mapped_column(LargeBinary, nullable=False)
    received_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    identity_frozen_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    assigned_reviewer_id: Mapped[UUID | None] = mapped_column(PGUUID(as_uuid=True))
    decision_reason: Mapped[str | None] = mapped_column(Text)
    decided_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    closed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))

    __table_args__ = (
        CheckConstraint(
            "content_id IS NOT NULL OR identity_asset_id IS NOT NULL",
            name="ck_complaint_target",
        ),
    )


class ExportVerification(Base):
    __tablename__ = "export_verifications"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    media_asset_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id"), nullable=False, index=True
    )
    asset_sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    export_stage: Mapped[str] = mapped_column(String(40), nullable=False)
    explicit_label_passed: Mapped[bool] = mapped_column(Boolean, nullable=False)
    implicit_label_passed: Mapped[bool] = mapped_column(Boolean, nullable=False)
    label_evidence: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False)
    verifier_version: Mapped[str] = mapped_column(String(100), nullable=False)
    overall_passed: Mapped[bool] = mapped_column(Boolean, nullable=False)
    checked_by_task_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("task_records.id"), nullable=False
    )
    checked_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)

    __table_args__ = (
        UniqueConstraint(
            "media_asset_id",
            "asset_sha256",
            "export_stage",
            "verifier_version",
            name="uq_export_verification",
        ),
        CheckConstraint("length(asset_sha256) = 64", name="ck_export_asset_hash"),
        CheckConstraint(
            "overall_passed = (explicit_label_passed AND implicit_label_passed)",
            name="ck_export_overall_result",
        ),
    )
~~~

- [ ] **Step 7: Add operational, launch, Beta, and release-decision models**

Append to backend/src/ip_saas/modules/governance/models.py:

~~~python
class DisasterRecoveryMarker(Base):
    __tablename__ = "disaster_recovery_markers"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    sequence_no: Mapped[int] = mapped_column(BigInteger, nullable=False, unique=True)
    emitted_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    marker_sha256: Mapped[str] = mapped_column(String(64), nullable=False)

    __table_args__ = (
        CheckConstraint("sequence_no > 0", name="ck_recovery_marker_sequence"),
        CheckConstraint("length(marker_sha256) = 64", name="ck_recovery_marker_hash"),
    )


class OperationalDrillEvidence(Base):
    __tablename__ = "operational_drill_evidence"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    drill_kind: Mapped[str] = mapped_column(String(50), nullable=False, index=True)
    environment: Mapped[str] = mapped_column(String(30), nullable=False)
    started_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    completed_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    source_restore_point_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    measured_rpo_seconds: Mapped[int | None] = mapped_column(Integer)
    measured_rto_seconds: Mapped[int | None] = mapped_column(Integer)
    result: Mapped[str] = mapped_column(String(20), nullable=False)
    measurements: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False)
    evidence_asset_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id"), nullable=False
    )
    evidence_sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    signed_by_operator: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    signed_by_reviewer: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)

    __table_args__ = (
        CheckConstraint("completed_at > started_at", name="ck_drill_duration"),
        CheckConstraint("length(evidence_sha256) = 64", name="ck_drill_hash"),
        CheckConstraint(
            "signed_by_operator <> signed_by_reviewer",
            name="ck_drill_independent_signatures",
        ),
    )


class LaunchExternalAttestation(Base):
    __tablename__ = "launch_external_attestations"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    topic: Mapped[str] = mapped_column(String(80), nullable=False, index=True)
    conclusion: Mapped[str] = mapped_column(String(30), nullable=False)
    conclusion_summary: Mapped[str] = mapped_column(Text, nullable=False)
    evidence_asset_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id"), nullable=False
    )
    evidence_sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    issuer_name: Mapped[str] = mapped_column(String(200), nullable=False)
    issued_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    valid_until: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    recorded_by: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)

    __table_args__ = (
        CheckConstraint(
            "conclusion IN ('approved','not_required','blocked')",
            name="ck_launch_attestation_conclusion",
        ),
        CheckConstraint("valid_until > issued_at", name="ck_launch_attestation_validity"),
        CheckConstraint("length(evidence_sha256) = 64", name="ck_launch_attestation_hash"),
    )


class BetaProgram(Base):
    __tablename__ = "beta_programs"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    criteria_version: Mapped[str] = mapped_column(String(80), nullable=False)
    criteria_sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    starts_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    ends_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    status: Mapped[str] = mapped_column(
        String(30), nullable=False, default=BetaProgramStatus.DRAFT.value
    )
    frozen_by: Mapped[UUID | None] = mapped_column(PGUUID(as_uuid=True))
    frozen_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    created_by: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)

    __table_args__ = (
        CheckConstraint(
            "ends_at >= starts_at + interval '42 days'", name="ck_beta_six_weeks"
        ),
        CheckConstraint("length(criteria_sha256) = 64", name="ck_beta_criteria_hash"),
    )


class BetaProjectEnrollment(Base):
    __tablename__ = "beta_project_enrollments"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    beta_program_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("beta_programs.id"), nullable=False, index=True
    )
    project_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("ip_projects.id"), nullable=False, index=True
    )
    project_kind: Mapped[str] = mapped_column(String(40), nullable=False)
    customer_case: Mapped[str | None] = mapped_column(String(40))
    baseline_eligible: Mapped[bool] = mapped_column(Boolean, nullable=False)
    enrolled_by: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    enrolled_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)

    __table_args__ = (
        UniqueConstraint("beta_program_id", "project_id", name="uq_beta_program_project"),
        CheckConstraint(
            "(project_kind = 'customer' AND customer_case IS NOT NULL) OR "
            "(project_kind = 'platform_self_marketing' AND customer_case IS NULL)",
            name="ck_beta_project_case",
        ),
    )


class BetaContentAssessment(Base):
    __tablename__ = "beta_content_assessments"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    beta_program_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("beta_programs.id"), nullable=False, index=True
    )
    publication_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("publications.id"), nullable=False
    )
    topic_card_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("topic_cards.id"), nullable=False
    )
    script_rewrite_class: Mapped[str] = mapped_column(String(30), nullable=False)
    false_claim: Mapped[bool] = mapped_column(Boolean, nullable=False)
    major_controversy: Mapped[bool] = mapped_column(Boolean, nullable=False)
    unsustainable_paid_traffic: Mapped[bool] = mapped_column(Boolean, nullable=False)
    evidence_asset_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id"), nullable=False
    )
    reviewed_by_operator: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    reviewed_by_reviewer: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    reviewed_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)

    __table_args__ = (
        UniqueConstraint(
            "beta_program_id", "publication_id", name="uq_beta_content_assessment"
        ),
        CheckConstraint(
            "reviewed_by_operator <> reviewed_by_reviewer",
            name="ck_beta_assessment_independent_review",
        ),
    )


class BetaBlindReviewResult(Base):
    __tablename__ = "beta_blind_review_results"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    beta_program_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("beta_programs.id"), nullable=False, index=True
    )
    project_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("ip_projects.id"), nullable=False, index=True
    )
    system_content_asset_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id"), nullable=False
    )
    generic_ai_content_asset_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id"), nullable=False
    )
    blinded_order_sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    operator_scores: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False)
    director_scores: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False)
    winner: Mapped[str] = mapped_column(String(30), nullable=False)
    reviewed_by_operator: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    reviewed_by_director: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    evidence_asset_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id"), nullable=False
    )
    reviewed_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)

    __table_args__ = (
        UniqueConstraint(
            "beta_program_id",
            "system_content_asset_id",
            "generic_ai_content_asset_id",
            name="uq_beta_blind_pair",
        ),
        CheckConstraint(
            "reviewed_by_operator <> reviewed_by_director",
            name="ck_blind_review_independent_reviewers",
        ),
        CheckConstraint(
            "winner IN ('system','generic_ai','tie','disqualified')",
            name="ck_blind_review_winner",
        ),
        CheckConstraint(
            "length(blinded_order_sha256) = 64", name="ck_blind_order_hash"
        ),
    )


class SelfMarketingBudgetApproval(Base):
    __tablename__ = "self_marketing_budget_approvals"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    project_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("ip_projects.id"), nullable=False, index=True
    )
    cost_center_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("internal_cost_centers.id"), nullable=False
    )
    request_fingerprint_sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    max_amount_fen: Mapped[int] = mapped_column(Integer, nullable=False)
    requested_by: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    approved_by: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    approved_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    valid_until: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    consumed_by_task_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("task_records.id"), unique=True
    )
    evidence_asset_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id"), nullable=False
    )

    __table_args__ = (
        CheckConstraint("max_amount_fen > 0", name="ck_self_marketing_budget_positive"),
        CheckConstraint("requested_by <> approved_by", name="ck_budget_approval_separation"),
        CheckConstraint("valid_until > approved_at", name="ck_budget_approval_validity"),
        CheckConstraint(
            "length(request_fingerprint_sha256) = 64",
            name="ck_budget_request_fingerprint",
        ),
    )


class SelfMarketingRiskReview(Base):
    __tablename__ = "self_marketing_risk_reviews"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    project_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("ip_projects.id"), nullable=False, index=True
    )
    prediction_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("predictions.id"), nullable=False, unique=True
    )
    media_asset_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id")
    )
    budget_approval_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("self_marketing_budget_approvals.id"),
        nullable=False,
    )
    triggered_risks: Mapped[list[str]] = mapped_column(JSONB, nullable=False)
    decision: Mapped[str] = mapped_column(String(30), nullable=False)
    decision_reason: Mapped[str] = mapped_column(Text, nullable=False)
    operator_actor_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    reviewer_actor_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    budget_approver_actor_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), nullable=False
    )
    evidence_asset_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id"), nullable=False
    )
    reviewed_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)

    __table_args__ = (
        CheckConstraint(
            "decision IN ('approved','rejected')", name="ck_self_marketing_review_decision"
        ),
        CheckConstraint(
            "operator_actor_id <> reviewer_actor_id AND "
            "operator_actor_id <> budget_approver_actor_id AND "
            "reviewer_actor_id <> budget_approver_actor_id",
            name="ck_self_marketing_three_party_separation",
        ),
        CheckConstraint(
            "jsonb_array_length(triggered_risks) > 0",
            name="ck_self_marketing_review_has_risk",
        ),
    )


class ReleaseDecision(Base):
    __tablename__ = "release_decisions"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    beta_program_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("beta_programs.id"), nullable=False, index=True
    )
    capability: Mapped[str] = mapped_column(String(60), nullable=False, index=True)
    entitlement_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True),
        ForeignKey("provider_capability_entitlements.id"),
        nullable=False,
    )
    provider_account_fingerprint: Mapped[str] = mapped_column(
        String(64), nullable=False
    )
    region: Mapped[str] = mapped_column(String(40), nullable=False)
    model_id: Mapped[str] = mapped_column(String(200), nullable=False)
    model_version: Mapped[str] = mapped_column(String(100), nullable=False)
    release_snapshot_sha256: Mapped[str] = mapped_column(
        String(64), nullable=False
    )
    decision: Mapped[str] = mapped_column(String(40), nullable=False)
    evaluator_version: Mapped[str] = mapped_column(String(80), nullable=False)
    input_snapshot_sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    evaluation: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False)
    evidence_asset_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id"), nullable=False
    )
    product_owner_approval_evidence_asset_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id")
    )
    decided_by: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    independently_reviewed_by: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), nullable=False
    )
    decided_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    executed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))

    __table_args__ = (
        CheckConstraint(
            "decision IN ('hold','approve_public_beta','rollback')",
            name="ck_release_decision",
        ),
        CheckConstraint(
            "length(input_snapshot_sha256) = 64", name="ck_release_snapshot_hash"
        ),
        CheckConstraint(
            "length(provider_account_fingerprint) = 64 AND "
            "length(release_snapshot_sha256) = 64",
            name="ck_release_binding_hashes",
        ),
        CheckConstraint(
            "decided_by <> independently_reviewed_by",
            name="ck_release_independent_review",
        ),
        CheckConstraint(
            "decision <> 'approve_public_beta' OR "
            "product_owner_approval_evidence_asset_id IS NOT NULL",
            name="ck_release_owner_approval",
        ),
    )
~~~

- [ ] **Step 8: Make signed gate, drill, launch, and release evidence append-only**

Append to backend/src/ip_saas/modules/governance/models.py:

~~~python
@event.listens_for(CapabilityGateAttestation, "before_update")
@event.listens_for(CapabilityGateAttestation, "before_delete")
@event.listens_for(DisasterRecoveryMarker, "before_update")
@event.listens_for(DisasterRecoveryMarker, "before_delete")
@event.listens_for(OperationalDrillEvidence, "before_update")
@event.listens_for(OperationalDrillEvidence, "before_delete")
@event.listens_for(LaunchExternalAttestation, "before_update")
@event.listens_for(LaunchExternalAttestation, "before_delete")
@event.listens_for(BetaBlindReviewResult, "before_update")
@event.listens_for(BetaBlindReviewResult, "before_delete")
@event.listens_for(SelfMarketingRiskReview, "before_update")
@event.listens_for(SelfMarketingRiskReview, "before_delete")
@event.listens_for(ReleaseDecision, "before_delete")
def _signed_evidence_is_append_only(*_args: object) -> None:
    raise RuntimeError("signed governance evidence is append-only")


@event.listens_for(ReleaseDecision, "before_update")
def _release_decision_only_allows_one_way_execution(_mapper, _connection, target) -> None:
    from sqlalchemy import inspect

    state = inspect(target)
    changed = {attribute.key for attribute in state.attrs
               if attribute.history.has_changes()}
    history = state.attrs.executed_at.history
    if (changed != {"executed_at"} or not history.added
            or history.added[0] is None or any(item is not None for item in history.deleted)):
        raise RuntimeError("release decision is immutable except first execution mark")
~~~

- [ ] **Step 9: Add quarantine fields to the existing media asset**

Append these columns to MediaAsset in backend/src/ip_saas/modules/media/models/jobs.py:

~~~python
quarantined_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
quarantine_reason: Mapped[str | None] = mapped_column(String(500))
deleted_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
deletion_reason: Mapped[str | None] = mapped_column(String(500))
~~~

Export governance models from backend/src/ip_saas/modules/governance/__init__.py:

~~~python
from .models import (
    BetaBlindReviewResult,
    BetaProgram,
    CapabilityFeatureFlag,
    ComplaintCase,
    ConsentRecord,
    DeletionPlan,
    ExportVerification,
    IdentitySubject,
    ProviderIdentityAsset,
    ReleaseDecision,
    SelfMarketingBudgetApproval,
    SelfMarketingRiskReview,
)

__all__ = [
    "BetaBlindReviewResult",
    "BetaProgram",
    "CapabilityFeatureFlag",
    "ComplaintCase",
    "ConsentRecord",
    "DeletionPlan",
    "ExportVerification",
    "IdentitySubject",
    "ProviderIdentityAsset",
    "ReleaseDecision",
    "SelfMarketingBudgetApproval",
    "SelfMarketingRiskReview",
]
~~~

Import every class from ip_saas.modules.governance.models in backend/src/ip_saas/modules/media/models/__init__.py only for Alembic metadata registration; do not re-export governance classes as media classes.

- [ ] **Step 10: Run the model-shape tests**

Run:

~~~bash
cd backend && uv run pytest tests/unit/governance/test_model_shape.py -v
~~~

Expected: PASS, 9 tests, including exact entitlement/gate/release snapshot binding.

- [ ] **Step 11: Commit governance persistence models**

~~~bash
git add backend/src/ip_saas/modules/governance backend/src/ip_saas/modules/media/models backend/tests/unit/governance/test_model_shape.py
git commit -m "feat: persist digital identity governance evidence"
~~~

### Task 3: Create and verify the frozen 0006_governance migration

**Files:**
- Create: backend/migrations/versions/0006_governance.py
- Create: backend/tests/integration/governance/test_migration.py

- [ ] **Step 1: Write the failing migration-chain and invariant test**

Create backend/tests/integration/governance/test_migration.py:

~~~python
from pathlib import Path

from alembic import command
from alembic.config import Config
from sqlalchemy import inspect


def alembic_config() -> Config:
    root = Path(__file__).parents[4]
    config = Config(str(root / "alembic.ini"))
    config.set_main_option("script_location", str(root / "migrations"))
    return config


def test_governance_migration_chain_and_constraints(pg_engine) -> None:
    inspector = inspect(pg_engine)
    assert {
        "capability_feature_flags",
        "provider_capability_entitlements",
        "consent_records",
        "provider_identity_assets",
        "generation_identity_uses",
        "deletion_plans",
        "deletion_execution_steps",
        "complaint_cases",
        "export_verifications",
        "disaster_recovery_markers",
        "beta_programs",
        "beta_blind_review_results",
        "self_marketing_budget_approvals",
        "self_marketing_risk_reviews",
        "release_decisions",
    } <= set(inspector.get_table_names())
    media_columns = {
        column["name"] for column in inspector.get_columns("media_assets")
    }
    assert {
        "quarantined_at",
        "quarantine_reason",
        "deleted_at",
        "deletion_reason",
    } <= media_columns
    assert "rights_snapshot_sha256s" in {
        column["name"]
        for column in inspector.get_columns("identity_enrollments")
    }
    assert "rights_snapshot_sha256s" in {
        column["name"]
        for column in inspector.get_columns("generation_identity_uses")
    }
    consent_indexes = {
        item["name"]: item for item in inspector.get_indexes("consent_records")
    }
    assert consent_indexes["uq_active_consent_subject_kind"]["unique"]
    enrollment_uniques = {
        item["name"]: tuple(item["column_names"])
        for item in inspector.get_unique_constraints("identity_enrollments")
    }
    quota_uniques = {
        item["name"]: tuple(item["column_names"])
        for item in inspector.get_unique_constraints(
            "capability_quota_reservations"
        )
    }
    assert enrollment_uniques["uq_identity_enrollment_account_idempotency"] == (
        "account_id", "idempotency_key",
    )
    assert quota_uniques["uq_capability_quota_account_idempotency"] == (
        "account_id", "idempotency_key",
    )
    assert ("idempotency_key",) not in enrollment_uniques.values()
    assert ("idempotency_key",) not in quota_uniques.values()
    release_checks = {
        item["name"] for item in inspector.get_check_constraints("release_decisions")
    }
    assert "ck_release_owner_approval" in release_checks
    assert "ck_release_binding_hashes" in release_checks
    binding_columns = {
        "entitlement_id", "provider_account_fingerprint", "region",
        "model_id", "model_version", "release_snapshot_sha256",
    }
    entitlement_columns = {
        item["name"]
        for item in inspector.get_columns("provider_capability_entitlements")
    }
    gate_columns = {
        item["name"]
        for item in inspector.get_columns("capability_gate_attestations")
    }
    release_columns = {
        item["name"] for item in inspector.get_columns("release_decisions")
    }
    assert binding_columns - {"entitlement_id"} <= entitlement_columns
    assert binding_columns <= gate_columns
    assert binding_columns | {"capability"} <= release_columns
    entitlement_uniques = {
        item["name"]: tuple(item["column_names"])
        for item in inspector.get_unique_constraints(
            "provider_capability_entitlements"
        )
    }
    assert entitlement_uniques["uq_provider_capability_account"] == (
        "capability", "provider", "region", "provider_account_fingerprint",
        "model_id", "model_version", "release_snapshot_sha256",
    )


def test_governance_downgrade_and_reupgrade(pg_engine) -> None:
    config = alembic_config()
    command.downgrade(config, "0005_resellers")
    assert "consent_records" not in set(inspect(pg_engine).get_table_names())
    assert "quarantined_at" not in {
        column["name"] for column in inspect(pg_engine).get_columns("media_assets")
    }
    command.upgrade(config, "0006_governance")
    assert "consent_records" in set(inspect(pg_engine).get_table_names())
~~~

- [ ] **Step 2: Run the migration test from the 0005 boundary**

Run:

~~~bash
cd backend && uv run alembic downgrade 0005_resellers && uv run pytest tests/integration/governance/test_migration.py -v
~~~

Expected: FAIL because revision 0006_governance and its tables do not exist.

- [ ] **Step 3: Create the migration header and shared column helpers**

Create backend/migrations/versions/0006_governance.py:

~~~python
from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects import postgresql

revision: str = "0006_governance"
down_revision: str | None = "0005_resellers"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def _id() -> sa.Column:
    return sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True)


def _utc(name: str, nullable: bool = False) -> sa.Column:
    return sa.Column(name, sa.DateTime(timezone=True), nullable=nullable)


def upgrade() -> None:
    op.add_column("media_assets", _utc("quarantined_at", nullable=True))
    op.add_column(
        "media_assets", sa.Column("quarantine_reason", sa.String(500), nullable=True)
    )
    op.add_column("media_assets", _utc("deleted_at", nullable=True))
    op.add_column(
        "media_assets", sa.Column("deletion_reason", sa.String(500), nullable=True)
    )
~~~

- [ ] **Step 4: Add release-control and provider-account tables to upgrade**

Append inside upgrade():

~~~text
    op.create_table(
        "capability_feature_flags",
        _id(),
        sa.Column("capability", sa.String(60), nullable=False, unique=True),
        sa.Column("stage", sa.String(30), nullable=False),
        sa.Column("version_no", sa.Integer(), nullable=False),
        sa.Column("reason", sa.String(500), nullable=False),
        sa.Column("changed_by", postgresql.UUID(as_uuid=True), nullable=False),
        _utc("changed_at"),
        sa.CheckConstraint(
            "stage IN ('off','internal_poc','closed_beta','public_beta')",
            name="ck_capability_feature_stage",
        ),
        sa.CheckConstraint("version_no > 0", name="ck_capability_feature_version"),
    )
    op.create_table(
        "capability_cohort_grants",
        _id(),
        sa.Column("capability", sa.String(60), nullable=False),
        sa.Column(
            "account_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("accounts.id"),
            nullable=False,
        ),
        sa.Column(
            "project_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("ip_projects.id"),
            nullable=False,
        ),
        _utc("allowed_until"),
        sa.Column("granted_by", postgresql.UUID(as_uuid=True), nullable=False),
        _utc("granted_at"),
        sa.Column("revoked_by", postgresql.UUID(as_uuid=True)),
        _utc("revoked_at", nullable=True),
        sa.Column("reason", sa.String(500), nullable=False),
    )
    op.create_index(
        "ix_capability_cohort_grants_account_id",
        "capability_cohort_grants",
        ["account_id"],
    )
    op.create_index(
        "ix_capability_cohort_grants_project_id",
        "capability_cohort_grants",
        ["project_id"],
    )
    op.create_index(
        "uq_active_capability_project_grant",
        "capability_cohort_grants",
        ["capability", "project_id"],
        unique=True,
        postgresql_where=sa.text("revoked_at IS NULL"),
    )
    op.create_table(
        "commercial_contract_attestations",
        _id(),
        sa.Column("capability", sa.String(60), nullable=False),
        sa.Column("contract_sha256", sa.String(64), nullable=False),
        sa.Column(
            "evidence_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
            nullable=False,
        ),
        sa.Column("platform_may_generate", sa.Boolean(), nullable=False),
        sa.Column("commercial_distribution_allowed", sa.Boolean(), nullable=False),
        _utc("effective_at"),
        _utc("expires_at"),
        sa.Column("approved_by", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column(
            "independently_reviewed_by",
            postgresql.UUID(as_uuid=True),
            nullable=False,
        ),
        _utc("approved_at"),
        sa.CheckConstraint("length(contract_sha256) = 64", name="ck_contract_hash"),
        sa.CheckConstraint("expires_at > effective_at", name="ck_contract_validity"),
        sa.CheckConstraint(
            "approved_by <> independently_reviewed_by",
            name="ck_contract_independent_review",
        ),
    )
    op.create_index(
        "ix_commercial_contract_attestations_capability",
        "commercial_contract_attestations",
        ["capability"],
    )
    op.create_table(
        "provider_capability_entitlements",
        _id(),
        sa.Column("capability", sa.String(60), nullable=False),
        sa.Column("provider", sa.String(60), nullable=False),
        sa.Column("region", sa.String(40), nullable=False),
        sa.Column("provider_account_fingerprint", sa.String(64), nullable=False),
        sa.Column("model_id", sa.String(200), nullable=False),
        sa.Column("model_version", sa.String(100), nullable=False),
        sa.Column("release_snapshot_sha256", sa.String(64), nullable=False),
        sa.Column("status", sa.String(30), nullable=False),
        sa.Column("quota_per_minute", sa.Integer(), nullable=False),
        sa.Column("quota_per_day", sa.Integer(), nullable=False),
        sa.Column("quota_per_month", sa.Integer(), nullable=False),
        sa.Column(
            "contract_attestation_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("commercial_contract_attestations.id"),
            nullable=False,
        ),
        sa.Column(
            "evidence_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
            nullable=False,
        ),
        _utc("checked_at"),
        _utc("expires_at"),
        sa.Column("checked_by", postgresql.UUID(as_uuid=True), nullable=False),
        sa.UniqueConstraint(
            "capability",
            "provider",
            "region",
            "provider_account_fingerprint",
            "model_id",
            "model_version",
            "release_snapshot_sha256",
            name="uq_provider_capability_account",
        ),
        sa.CheckConstraint(
            "quota_per_minute > 0 AND quota_per_day > 0 AND quota_per_month > 0",
            name="ck_provider_entitlement_quotas",
        ),
        sa.CheckConstraint("expires_at > checked_at", name="ck_entitlement_validity"),
        sa.CheckConstraint(
            "length(provider_account_fingerprint) = 64 AND "
            "length(release_snapshot_sha256) = 64",
            name="ck_entitlement_binding_hashes",
        ),
    )
    op.create_index(
        "ix_provider_capability_entitlements_capability",
        "provider_capability_entitlements",
        ["capability"],
    )
    op.create_table(
        "capability_gate_attestations",
        _id(),
        sa.Column("capability", sa.String(60), nullable=False),
        sa.Column("gate_kind", sa.String(60), nullable=False),
        sa.Column("result", sa.String(20), nullable=False),
        sa.Column(
            "entitlement_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("provider_capability_entitlements.id"),
            nullable=False,
        ),
        sa.Column("provider_account_fingerprint", sa.String(64), nullable=False),
        sa.Column("region", sa.String(40), nullable=False),
        sa.Column("model_id", sa.String(200), nullable=False),
        sa.Column("model_version", sa.String(100), nullable=False),
        sa.Column("release_snapshot_sha256", sa.String(64), nullable=False),
        sa.Column(
            "evidence_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
            nullable=False,
        ),
        sa.Column("evidence_sha256", sa.String(64), nullable=False),
        sa.Column("test_run_ref", sa.String(200), nullable=False),
        sa.Column("measurements", postgresql.JSONB(), nullable=False),
        _utc("executed_at"),
        _utc("valid_until"),
        sa.Column("signed_by_operator", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("signed_by_reviewer", postgresql.UUID(as_uuid=True), nullable=False),
        sa.UniqueConstraint(
            "capability", "gate_kind", "test_run_ref", name="uq_capability_gate_run"
        ),
        sa.CheckConstraint(
            "result IN ('pass','fail','expired')", name="ck_capability_gate_result"
        ),
        sa.CheckConstraint("length(evidence_sha256) = 64", name="ck_gate_evidence_hash"),
        sa.CheckConstraint(
            "length(provider_account_fingerprint) = 64 AND "
            "length(release_snapshot_sha256) = 64",
            name="ck_gate_binding_hashes",
        ),
        sa.CheckConstraint("valid_until > executed_at", name="ck_gate_validity"),
        sa.CheckConstraint(
            "signed_by_operator <> signed_by_reviewer",
            name="ck_gate_independent_signatures",
        ),
    )
    op.create_index(
        "ix_capability_gate_attestations_capability",
        "capability_gate_attestations",
        ["capability"],
    )
    op.create_index(
        "ix_capability_gate_attestations_gate_kind",
        "capability_gate_attestations",
        ["gate_kind"],
    )
~~~

- [ ] **Step 5: Add subject, consent, enrollment, and identity-asset tables to upgrade**

Append inside upgrade():

~~~text
    op.create_table(
        "identity_subjects",
        _id(),
        sa.Column(
            "account_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("accounts.id"),
            nullable=False,
        ),
        sa.Column(
            "project_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("ip_projects.id"),
            nullable=False,
        ),
        sa.Column("pseudonym", sa.String(120), nullable=False),
        sa.Column("age_class", sa.String(20), nullable=False),
        sa.Column(
            "guardian_subject_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("identity_subjects.id"),
        ),
        sa.Column("age_verification_method", sa.String(80), nullable=False),
        sa.Column(
            "age_verification_evidence_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
            nullable=False,
        ),
        sa.Column("verified_by", postgresql.UUID(as_uuid=True), nullable=False),
        _utc("verified_at"),
        _utc("disabled_at", nullable=True),
        sa.CheckConstraint(
            "age_class IN ('adult','minor','unknown')",
            name="ck_identity_subject_age",
        ),
        sa.CheckConstraint(
            "(age_class = 'minor' AND guardian_subject_id IS NOT NULL) OR "
            "(age_class <> 'minor' AND guardian_subject_id IS NULL)",
            name="ck_identity_subject_guardian",
        ),
    )
    op.create_index("ix_identity_subjects_account_id", "identity_subjects", ["account_id"])
    op.create_index("ix_identity_subjects_project_id", "identity_subjects", ["project_id"])
    op.create_table(
        "consent_records",
        _id(),
        sa.Column(
            "account_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("accounts.id"),
            nullable=False,
        ),
        sa.Column(
            "project_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("ip_projects.id"),
            nullable=False,
        ),
        sa.Column(
            "subject_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("identity_subjects.id"),
            nullable=False,
        ),
        sa.Column("kind", sa.String(40), nullable=False),
        sa.Column("terms_version", sa.String(100), nullable=False),
        sa.Column("terms_snapshot_sha256", sa.String(64), nullable=False),
        sa.Column("purposes", postgresql.ARRAY(sa.String(80)), nullable=False),
        sa.Column("platforms", postgresql.ARRAY(sa.String(40)), nullable=False),
        sa.Column("territory", sa.String(40), nullable=False),
        _utc("valid_from"),
        _utc("valid_until"),
        sa.Column("usage_limit", sa.Integer(), nullable=False),
        sa.Column("usage_count", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("commercial_use", sa.Boolean(), nullable=False),
        sa.Column("modeling_allowed", sa.Boolean(), nullable=False),
        sa.Column("reuse_allowed", sa.Boolean(), nullable=False),
        sa.Column("provider_transfer_allowed", sa.Boolean(), nullable=False),
        sa.Column("existing_output_policy", sa.String(80), nullable=False),
        sa.Column("verification_method", sa.String(80), nullable=False),
        sa.Column(
            "verification_evidence_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
            nullable=False,
        ),
        sa.Column("status", sa.String(30), nullable=False),
        sa.Column("granted_by", postgresql.UUID(as_uuid=True), nullable=False),
        _utc("granted_at"),
        sa.Column("withdrawn_by", postgresql.UUID(as_uuid=True)),
        _utc("withdrawn_at", nullable=True),
        sa.Column("withdrawal_reason", sa.String(500)),
        sa.CheckConstraint(
            "length(terms_snapshot_sha256) = 64", name="ck_consent_terms_hash"
        ),
        sa.CheckConstraint("valid_until > valid_from", name="ck_consent_validity"),
        sa.CheckConstraint(
            "usage_limit > 0 AND usage_count >= 0 AND usage_count <= usage_limit",
            name="ck_consent_usage",
        ),
    )
    op.create_index("ix_consent_records_account_id", "consent_records", ["account_id"])
    op.create_index("ix_consent_records_project_id", "consent_records", ["project_id"])
    op.create_index("ix_consent_records_subject_id", "consent_records", ["subject_id"])
    op.create_index(
        "uq_active_consent_subject_kind",
        "consent_records",
        ["subject_id", "kind"],
        unique=True,
        postgresql_where=sa.text("status = 'active'"),
    )
    op.create_table(
        "identity_enrollments",
        _id(),
        sa.Column(
            "task_record_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("task_records.id"),
            nullable=False,
            unique=True,
        ),
        sa.Column(
            "account_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("accounts.id"),
            nullable=False,
        ),
        sa.Column(
            "project_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("ip_projects.id"),
            nullable=False,
        ),
        sa.Column(
            "subject_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("identity_subjects.id"),
            nullable=False,
        ),
        sa.Column("identity_kind", sa.String(40), nullable=False),
        sa.Column("provider", sa.String(60), nullable=False),
        sa.Column("source_media_asset_ids", postgresql.JSONB(), nullable=False),
        sa.Column("rights_snapshot_sha256s", postgresql.JSONB(), nullable=False),
        sa.Column("status", sa.String(30), nullable=False),
        sa.Column("provider_task_id", sa.String(300)),
        sa.Column("idempotency_key", sa.String(160), nullable=False),
        sa.Column("last_error_code", sa.String(100)),
        sa.Column("last_error_message", sa.String(500)),
        sa.Column("initiated_by_actor_id", postgresql.UUID(as_uuid=True), nullable=False),
        _utc("created_at"),
        _utc("updated_at"),
        sa.UniqueConstraint(
            "account_id",
            "idempotency_key",
            name="uq_identity_enrollment_account_idempotency",
        ),
    )
    op.create_index("ix_identity_enrollments_account_id", "identity_enrollments", ["account_id"])
    op.create_index("ix_identity_enrollments_project_id", "identity_enrollments", ["project_id"])
    op.create_index(
        "uq_identity_enrollment_provider_task",
        "identity_enrollments",
        ["provider", "provider_task_id"],
        unique=True,
        postgresql_where=sa.text("provider_task_id IS NOT NULL"),
    )
    op.create_table(
        "provider_identity_assets",
        _id(),
        sa.Column(
            "account_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("accounts.id"),
            nullable=False,
        ),
        sa.Column(
            "project_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("ip_projects.id"),
            nullable=False,
        ),
        sa.Column(
            "subject_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("identity_subjects.id"),
            nullable=False,
        ),
        sa.Column("identity_kind", sa.String(40), nullable=False),
        sa.Column("provider", sa.String(60), nullable=False),
        sa.Column(
            "enrollment_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("identity_enrollments.id"),
            nullable=False,
            unique=True,
        ),
        sa.Column("status", sa.String(30), nullable=False),
        sa.Column("provider_resource_ref_ciphertext", sa.LargeBinary()),
        sa.Column("provider_resource_ref_fingerprint", sa.String(64)),
        sa.Column(
            "character_profile_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("character_profiles.id"),
        ),
        _utc("activated_at", nullable=True),
        _utc("frozen_at", nullable=True),
        sa.Column("freeze_reason", sa.String(500)),
        _utc("deleted_at", nullable=True),
        _utc("created_at"),
        sa.CheckConstraint(
            "(status = 'active' AND provider_resource_ref_ciphertext IS NOT NULL "
            "AND provider_resource_ref_fingerprint IS NOT NULL) OR "
            "(status <> 'active')",
            name="ck_identity_asset_active_reference",
        ),
    )
    op.create_index(
        "ix_provider_identity_assets_account_id",
        "provider_identity_assets",
        ["account_id"],
    )
    op.create_index(
        "ix_provider_identity_assets_project_id",
        "provider_identity_assets",
        ["project_id"],
    )
    op.create_index(
        "ix_provider_identity_assets_subject_id",
        "provider_identity_assets",
        ["subject_id"],
    )
    op.create_table(
        "identity_asset_consents",
        sa.Column(
            "identity_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("provider_identity_assets.id"),
            primary_key=True,
        ),
        sa.Column(
            "consent_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("consent_records.id"),
            primary_key=True,
        ),
        sa.Column("consent_snapshot_sha256", sa.String(64), nullable=False),
        sa.CheckConstraint(
            "length(consent_snapshot_sha256) = 64",
            name="ck_identity_consent_hash",
        ),
    )
~~~

- [ ] **Step 6: Add generation-use, quota, deletion, complaint, and export tables to upgrade**

Append inside upgrade():

~~~text
    op.create_table(
        "generation_identity_uses",
        sa.Column(
            "generation_job_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("generation_jobs.id"),
            primary_key=True,
        ),
        sa.Column(
            "identity_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("provider_identity_assets.id"),
            primary_key=True,
        ),
        sa.Column("consent_snapshot_sha256s", postgresql.JSONB(), nullable=False),
        sa.Column("rights_snapshot_sha256s", postgresql.JSONB(), nullable=False),
        sa.Column("platform", sa.String(40), nullable=False),
        _utc("used_at"),
        _utc("delivery_blocked_at", nullable=True),
        sa.Column("delivery_block_reason", sa.String(500)),
    )
    op.create_table(
        "capability_quota_reservations",
        _id(),
        sa.Column("capability", sa.String(60), nullable=False),
        sa.Column(
            "account_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("accounts.id"),
            nullable=False,
        ),
        sa.Column(
            "project_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("ip_projects.id"),
            nullable=False,
        ),
        sa.Column(
            "task_record_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("task_records.id"),
        ),
        sa.Column("reserved_units", sa.Integer(), nullable=False),
        sa.Column("status", sa.String(20), nullable=False),
        sa.Column("idempotency_key", sa.String(160), nullable=False),
        _utc("reserved_at"),
        _utc("expires_at"),
        _utc("settled_at", nullable=True),
        _utc("released_at", nullable=True),
        sa.UniqueConstraint(
            "account_id",
            "idempotency_key",
            name="uq_capability_quota_account_idempotency",
        ),
        sa.CheckConstraint("reserved_units > 0", name="ck_capability_quota_units"),
        sa.CheckConstraint("expires_at > reserved_at", name="ck_capability_quota_expiry"),
    )
    op.create_index(
        "ix_capability_quota_reservations_capability",
        "capability_quota_reservations",
        ["capability"],
    )
    op.create_index(
        "ix_capability_quota_reservations_account_id",
        "capability_quota_reservations",
        ["account_id"],
    )
    op.create_index(
        "ix_capability_quota_reservations_project_id",
        "capability_quota_reservations",
        ["project_id"],
    )
    op.create_table(
        "identity_deletion_requests",
        _id(),
        sa.Column(
            "account_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("accounts.id"),
            nullable=False,
        ),
        sa.Column(
            "project_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("ip_projects.id"),
            nullable=False,
        ),
        sa.Column(
            "subject_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("identity_subjects.id"),
            nullable=False,
        ),
        sa.Column(
            "identity_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("provider_identity_assets.id"),
            nullable=False,
        ),
        sa.Column("status", sa.String(30), nullable=False),
        sa.Column("requested_by", postgresql.UUID(as_uuid=True), nullable=False),
        _utc("requested_at"),
        sa.Column("provider_receipt_sha256", sa.String(64)),
        sa.Column(
            "provider_receipt_evidence_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
        ),
        _utc("live_data_deleted_at", nullable=True),
        sa.Column("verified_by", postgresql.UUID(as_uuid=True)),
        _utc("verified_at", nullable=True),
        sa.Column("failure_message", sa.String(500)),
    )
    op.create_index(
        "ix_identity_deletion_requests_account_id",
        "identity_deletion_requests",
        ["account_id"],
    )
    op.create_index(
        "ix_identity_deletion_requests_project_id",
        "identity_deletion_requests",
        ["project_id"],
    )
    op.create_index(
        "uq_open_identity_deletion",
        "identity_deletion_requests",
        ["identity_asset_id"],
        unique=True,
        postgresql_where=sa.text(
            "status IN ('requested','provider_pending','live_data_deleted')"
        ),
    )
    op.create_table(
        "deletion_plans",
        _id(),
        sa.Column(
            "account_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("accounts.id"),
            nullable=False,
        ),
        sa.Column(
            "project_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("ip_projects.id"),
            nullable=False,
        ),
        sa.Column("target_type", sa.String(50), nullable=False),
        sa.Column("target_id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("status", sa.String(30), nullable=False),
        sa.Column("deletion_manifest", postgresql.JSONB(), nullable=False),
        sa.Column("manifest_sha256", sa.String(64), nullable=False),
        sa.Column("trashed_by", postgresql.UUID(as_uuid=True), nullable=False),
        _utc("trashed_at"),
        _utc("restore_until"),
        sa.Column("restored_by", postgresql.UUID(as_uuid=True)),
        _utc("restored_at", nullable=True),
        sa.Column("permanent_confirmation_sha256", sa.String(64)),
        sa.Column("purge_requested_by", postgresql.UUID(as_uuid=True)),
        _utc("purge_requested_at", nullable=True),
        _utc("backup_expire_by"),
        _utc("completed_at", nullable=True),
        sa.Column(
            "completion_evidence_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
        ),
        sa.Column("completion_evidence_sha256", sa.String(64)),
        sa.Column("failure_message", sa.String(500)),
        sa.CheckConstraint(
            "restore_until > trashed_at", name="ck_delete_restore_window"
        ),
        sa.CheckConstraint(
            "backup_expire_by >= restore_until", name="ck_delete_backup_expiry"
        ),
        sa.CheckConstraint(
            "length(manifest_sha256) = 64", name="ck_delete_manifest_hash"
        ),
    )
    op.create_index("ix_deletion_plans_account_id", "deletion_plans", ["account_id"])
    op.create_index("ix_deletion_plans_project_id", "deletion_plans", ["project_id"])
    op.create_index(
        "uq_open_deletion_target",
        "deletion_plans",
        ["target_type", "target_id"],
        unique=True,
        postgresql_where=sa.text(
            "status IN ('trashed','purge_pending','purging','failed')"
        ),
    )
    op.create_table(
        "deletion_execution_steps",
        _id(),
        sa.Column(
            "deletion_plan_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("deletion_plans.id"),
            nullable=False,
        ),
        sa.Column("system", sa.String(40), nullable=False),
        sa.Column("status", sa.String(40), nullable=False),
        sa.Column("attempt_no", sa.Integer(), nullable=False),
        _utc("started_at", nullable=True),
        _utc("completed_at", nullable=True),
        sa.Column(
            "evidence_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
        ),
        sa.Column("evidence_sha256", sa.String(64)),
        sa.Column("sanitized_error", sa.String(500)),
        sa.UniqueConstraint(
            "deletion_plan_id", "system", "attempt_no", name="uq_deletion_step_attempt"
        ),
        sa.CheckConstraint("attempt_no > 0", name="ck_deletion_step_attempt"),
    )
    op.create_index(
        "ix_deletion_execution_steps_deletion_plan_id",
        "deletion_execution_steps",
        ["deletion_plan_id"],
    )
    op.create_table(
        "complaint_cases",
        _id(),
        sa.Column("public_reference", sa.String(32), nullable=False, unique=True),
        sa.Column(
            "project_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("ip_projects.id"),
        ),
        sa.Column("content_id", postgresql.UUID(as_uuid=True)),
        sa.Column(
            "identity_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("provider_identity_assets.id"),
        ),
        sa.Column("category", sa.String(60), nullable=False),
        sa.Column("severity", sa.String(20), nullable=False),
        sa.Column("status", sa.String(30), nullable=False),
        sa.Column("description", sa.Text(), nullable=False),
        sa.Column("evidence_asset_ids", postgresql.JSONB(), nullable=False),
        sa.Column("contact_ciphertext", sa.LargeBinary(), nullable=False),
        _utc("received_at"),
        _utc("identity_frozen_at", nullable=True),
        sa.Column("assigned_reviewer_id", postgresql.UUID(as_uuid=True)),
        sa.Column("decision_reason", sa.Text()),
        _utc("decided_at", nullable=True),
        _utc("closed_at", nullable=True),
        sa.CheckConstraint(
            "content_id IS NOT NULL OR identity_asset_id IS NOT NULL",
            name="ck_complaint_target",
        ),
    )
    op.create_index("ix_complaint_cases_project_id", "complaint_cases", ["project_id"])
    op.create_index("ix_complaint_cases_content_id", "complaint_cases", ["content_id"])
    op.create_index(
        "ix_complaint_cases_identity_asset_id",
        "complaint_cases",
        ["identity_asset_id"],
    )
    op.create_table(
        "export_verifications",
        _id(),
        sa.Column(
            "media_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
            nullable=False,
        ),
        sa.Column("asset_sha256", sa.String(64), nullable=False),
        sa.Column("export_stage", sa.String(40), nullable=False),
        sa.Column("explicit_label_passed", sa.Boolean(), nullable=False),
        sa.Column("implicit_label_passed", sa.Boolean(), nullable=False),
        sa.Column("label_evidence", postgresql.JSONB(), nullable=False),
        sa.Column("verifier_version", sa.String(100), nullable=False),
        sa.Column("overall_passed", sa.Boolean(), nullable=False),
        sa.Column(
            "checked_by_task_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("task_records.id"),
            nullable=False,
        ),
        _utc("checked_at"),
        sa.UniqueConstraint(
            "media_asset_id",
            "asset_sha256",
            "export_stage",
            "verifier_version",
            name="uq_export_verification",
        ),
        sa.CheckConstraint("length(asset_sha256) = 64", name="ck_export_asset_hash"),
        sa.CheckConstraint(
            "overall_passed = (explicit_label_passed AND implicit_label_passed)",
            name="ck_export_overall_result",
        ),
    )
    op.create_index(
        "ix_export_verifications_media_asset_id",
        "export_verifications",
        ["media_asset_id"],
    )
~~~

- [ ] **Step 7: Add drill, external-attestation, Beta, and release tables to upgrade**

Append inside upgrade():

~~~text
    op.create_table(
        "disaster_recovery_markers",
        _id(),
        sa.Column("sequence_no", sa.BigInteger(), nullable=False, unique=True),
        _utc("emitted_at"),
        sa.Column("marker_sha256", sa.String(64), nullable=False),
        sa.CheckConstraint(
            "sequence_no > 0", name="ck_recovery_marker_sequence"
        ),
        sa.CheckConstraint(
            "length(marker_sha256) = 64", name="ck_recovery_marker_hash"
        ),
    )
    op.create_table(
        "operational_drill_evidence",
        _id(),
        sa.Column("drill_kind", sa.String(50), nullable=False),
        sa.Column("environment", sa.String(30), nullable=False),
        _utc("started_at"),
        _utc("completed_at"),
        _utc("source_restore_point_at", nullable=True),
        sa.Column("measured_rpo_seconds", sa.Integer()),
        sa.Column("measured_rto_seconds", sa.Integer()),
        sa.Column("result", sa.String(20), nullable=False),
        sa.Column("measurements", postgresql.JSONB(), nullable=False),
        sa.Column(
            "evidence_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
            nullable=False,
        ),
        sa.Column("evidence_sha256", sa.String(64), nullable=False),
        sa.Column("signed_by_operator", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("signed_by_reviewer", postgresql.UUID(as_uuid=True), nullable=False),
        sa.CheckConstraint("completed_at > started_at", name="ck_drill_duration"),
        sa.CheckConstraint("length(evidence_sha256) = 64", name="ck_drill_hash"),
        sa.CheckConstraint(
            "signed_by_operator <> signed_by_reviewer",
            name="ck_drill_independent_signatures",
        ),
    )
    op.create_index(
        "ix_operational_drill_evidence_drill_kind",
        "operational_drill_evidence",
        ["drill_kind"],
    )
    op.create_table(
        "launch_external_attestations",
        _id(),
        sa.Column("topic", sa.String(80), nullable=False),
        sa.Column("conclusion", sa.String(30), nullable=False),
        sa.Column("conclusion_summary", sa.Text(), nullable=False),
        sa.Column(
            "evidence_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
            nullable=False,
        ),
        sa.Column("evidence_sha256", sa.String(64), nullable=False),
        sa.Column("issuer_name", sa.String(200), nullable=False),
        _utc("issued_at"),
        _utc("valid_until"),
        sa.Column("recorded_by", postgresql.UUID(as_uuid=True), nullable=False),
        sa.CheckConstraint(
            "conclusion IN ('approved','not_required','blocked')",
            name="ck_launch_attestation_conclusion",
        ),
        sa.CheckConstraint(
            "valid_until > issued_at", name="ck_launch_attestation_validity"
        ),
        sa.CheckConstraint(
            "length(evidence_sha256) = 64", name="ck_launch_attestation_hash"
        ),
    )
    op.create_index(
        "ix_launch_external_attestations_topic",
        "launch_external_attestations",
        ["topic"],
    )
    op.create_table(
        "beta_programs",
        _id(),
        sa.Column("criteria_version", sa.String(80), nullable=False),
        sa.Column("criteria_sha256", sa.String(64), nullable=False),
        _utc("starts_at"),
        _utc("ends_at"),
        sa.Column("status", sa.String(30), nullable=False),
        sa.Column("frozen_by", postgresql.UUID(as_uuid=True)),
        _utc("frozen_at", nullable=True),
        sa.Column("created_by", postgresql.UUID(as_uuid=True), nullable=False),
        _utc("created_at"),
        sa.CheckConstraint(
            "ends_at >= starts_at + interval '42 days'", name="ck_beta_six_weeks"
        ),
        sa.CheckConstraint(
            "length(criteria_sha256) = 64", name="ck_beta_criteria_hash"
        ),
    )
    op.create_table(
        "beta_project_enrollments",
        _id(),
        sa.Column(
            "beta_program_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("beta_programs.id"),
            nullable=False,
        ),
        sa.Column("capability", sa.String(60), nullable=False),
        sa.Column(
            "project_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("ip_projects.id"),
            nullable=False,
        ),
        sa.Column("project_kind", sa.String(40), nullable=False),
        sa.Column("customer_case", sa.String(40)),
        sa.Column("baseline_eligible", sa.Boolean(), nullable=False),
        sa.Column("enrolled_by", postgresql.UUID(as_uuid=True), nullable=False),
        _utc("enrolled_at"),
        sa.UniqueConstraint(
            "beta_program_id", "project_id", name="uq_beta_program_project"
        ),
        sa.CheckConstraint(
            "(project_kind = 'customer' AND customer_case IS NOT NULL) OR "
            "(project_kind = 'platform_self_marketing' AND customer_case IS NULL)",
            name="ck_beta_project_case",
        ),
    )
    op.create_index(
        "ix_beta_project_enrollments_beta_program_id",
        "beta_project_enrollments",
        ["beta_program_id"],
    )
    op.create_index(
        "ix_beta_project_enrollments_project_id",
        "beta_project_enrollments",
        ["project_id"],
    )
    op.create_table(
        "beta_content_assessments",
        _id(),
        sa.Column(
            "beta_program_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("beta_programs.id"),
            nullable=False,
        ),
        sa.Column(
            "publication_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("publications.id"),
            nullable=False,
        ),
        sa.Column(
            "topic_card_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("topic_cards.id"),
            nullable=False,
        ),
        sa.Column("script_rewrite_class", sa.String(30), nullable=False),
        sa.Column("false_claim", sa.Boolean(), nullable=False),
        sa.Column("major_controversy", sa.Boolean(), nullable=False),
        sa.Column("unsustainable_paid_traffic", sa.Boolean(), nullable=False),
        sa.Column(
            "evidence_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
            nullable=False,
        ),
        sa.Column("reviewed_by_operator", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("reviewed_by_reviewer", postgresql.UUID(as_uuid=True), nullable=False),
        _utc("reviewed_at"),
        sa.UniqueConstraint(
            "beta_program_id",
            "publication_id",
            name="uq_beta_content_assessment",
        ),
        sa.CheckConstraint(
            "reviewed_by_operator <> reviewed_by_reviewer",
            name="ck_beta_assessment_independent_review",
        ),
    )
    op.create_index(
        "ix_beta_content_assessments_beta_program_id",
        "beta_content_assessments",
        ["beta_program_id"],
    )
    op.create_table(
        "beta_blind_review_results",
        _id(),
        sa.Column(
            "beta_program_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("beta_programs.id"),
            nullable=False,
        ),
        sa.Column(
            "project_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("ip_projects.id"),
            nullable=False,
        ),
        sa.Column(
            "system_content_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
            nullable=False,
        ),
        sa.Column(
            "generic_ai_content_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
            nullable=False,
        ),
        sa.Column("blinded_order_sha256", sa.String(64), nullable=False),
        sa.Column("operator_scores", postgresql.JSONB(), nullable=False),
        sa.Column("director_scores", postgresql.JSONB(), nullable=False),
        sa.Column("winner", sa.String(30), nullable=False),
        sa.Column("reviewed_by_operator", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("reviewed_by_director", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column(
            "evidence_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
            nullable=False,
        ),
        _utc("reviewed_at"),
        sa.UniqueConstraint(
            "beta_program_id",
            "system_content_asset_id",
            "generic_ai_content_asset_id",
            name="uq_beta_blind_pair",
        ),
        sa.CheckConstraint(
            "reviewed_by_operator <> reviewed_by_director",
            name="ck_blind_review_independent_reviewers",
        ),
        sa.CheckConstraint(
            "winner IN ('system','generic_ai','tie','disqualified')",
            name="ck_blind_review_winner",
        ),
        sa.CheckConstraint(
            "length(blinded_order_sha256) = 64", name="ck_blind_order_hash"
        ),
    )
    op.create_index(
        "ix_beta_blind_review_results_beta_program_id",
        "beta_blind_review_results",
        ["beta_program_id"],
    )
    op.create_index(
        "ix_beta_blind_review_results_project_id",
        "beta_blind_review_results",
        ["project_id"],
    )
    op.create_table(
        "self_marketing_budget_approvals",
        _id(),
        sa.Column(
            "project_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("ip_projects.id"),
            nullable=False,
        ),
        sa.Column(
            "cost_center_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("internal_cost_centers.id"),
            nullable=False,
        ),
        sa.Column("request_fingerprint_sha256", sa.String(64), nullable=False),
        sa.Column("max_amount_fen", sa.Integer(), nullable=False),
        sa.Column("requested_by", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("approved_by", postgresql.UUID(as_uuid=True), nullable=False),
        _utc("approved_at"),
        _utc("valid_until"),
        sa.Column(
            "consumed_by_task_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("task_records.id"),
            unique=True,
        ),
        sa.Column(
            "evidence_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
            nullable=False,
        ),
        sa.CheckConstraint(
            "max_amount_fen > 0", name="ck_self_marketing_budget_positive"
        ),
        sa.CheckConstraint(
            "requested_by <> approved_by", name="ck_budget_approval_separation"
        ),
        sa.CheckConstraint(
            "valid_until > approved_at", name="ck_budget_approval_validity"
        ),
        sa.CheckConstraint(
            "length(request_fingerprint_sha256) = 64",
            name="ck_budget_request_fingerprint",
        ),
    )
    op.create_index(
        "ix_self_marketing_budget_approvals_project_id",
        "self_marketing_budget_approvals",
        ["project_id"],
    )
    op.create_table(
        "self_marketing_risk_reviews",
        _id(),
        sa.Column(
            "project_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("ip_projects.id"),
            nullable=False,
        ),
        sa.Column(
            "prediction_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("predictions.id"),
            nullable=False,
            unique=True,
        ),
        sa.Column(
            "media_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
        ),
        sa.Column(
            "budget_approval_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("self_marketing_budget_approvals.id"),
            nullable=False,
        ),
        sa.Column("triggered_risks", postgresql.JSONB(), nullable=False),
        sa.Column("decision", sa.String(30), nullable=False),
        sa.Column("decision_reason", sa.Text(), nullable=False),
        sa.Column("operator_actor_id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("reviewer_actor_id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column(
            "budget_approver_actor_id", postgresql.UUID(as_uuid=True), nullable=False
        ),
        sa.Column(
            "evidence_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
            nullable=False,
        ),
        _utc("reviewed_at"),
        sa.CheckConstraint(
            "decision IN ('approved','rejected')",
            name="ck_self_marketing_review_decision",
        ),
        sa.CheckConstraint(
            "operator_actor_id <> reviewer_actor_id AND "
            "operator_actor_id <> budget_approver_actor_id AND "
            "reviewer_actor_id <> budget_approver_actor_id",
            name="ck_self_marketing_three_party_separation",
        ),
        sa.CheckConstraint(
            "jsonb_array_length(triggered_risks) > 0",
            name="ck_self_marketing_review_has_risk",
        ),
    )
    op.create_index(
        "ix_self_marketing_risk_reviews_project_id",
        "self_marketing_risk_reviews",
        ["project_id"],
    )
    op.create_table(
        "release_decisions",
        _id(),
        sa.Column(
            "beta_program_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("beta_programs.id"),
            nullable=False,
        ),
        sa.Column("capability", sa.String(60), nullable=False),
        sa.Column(
            "entitlement_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("provider_capability_entitlements.id"),
            nullable=False,
        ),
        sa.Column("provider_account_fingerprint", sa.String(64), nullable=False),
        sa.Column("region", sa.String(40), nullable=False),
        sa.Column("model_id", sa.String(200), nullable=False),
        sa.Column("model_version", sa.String(100), nullable=False),
        sa.Column("release_snapshot_sha256", sa.String(64), nullable=False),
        sa.Column("decision", sa.String(40), nullable=False),
        sa.Column("evaluator_version", sa.String(80), nullable=False),
        sa.Column("input_snapshot_sha256", sa.String(64), nullable=False),
        sa.Column("evaluation", postgresql.JSONB(), nullable=False),
        sa.Column(
            "evidence_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
            nullable=False,
        ),
        sa.Column(
            "product_owner_approval_evidence_asset_id",
            postgresql.UUID(as_uuid=True),
            sa.ForeignKey("media_assets.id"),
        ),
        sa.Column("decided_by", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column(
            "independently_reviewed_by",
            postgresql.UUID(as_uuid=True),
            nullable=False,
        ),
        _utc("decided_at"),
        _utc("executed_at", nullable=True),
        sa.CheckConstraint(
            "decision IN ('hold','approve_public_beta','rollback')",
            name="ck_release_decision",
        ),
        sa.CheckConstraint(
            "length(input_snapshot_sha256) = 64", name="ck_release_snapshot_hash"
        ),
        sa.CheckConstraint(
            "length(provider_account_fingerprint) = 64 AND "
            "length(release_snapshot_sha256) = 64",
            name="ck_release_binding_hashes",
        ),
        sa.CheckConstraint(
            "decided_by <> independently_reviewed_by",
            name="ck_release_independent_review",
        ),
        sa.CheckConstraint(
            "decision <> 'approve_public_beta' OR "
            "product_owner_approval_evidence_asset_id IS NOT NULL",
            name="ck_release_owner_approval",
        ),
    )
    op.create_index(
        "ix_release_decisions_beta_program_id",
        "release_decisions",
        ["beta_program_id"],
    )
    op.create_index(
        "ix_release_decisions_capability",
        "release_decisions",
        ["capability"],
    )
~~~

- [ ] **Step 8: Add the exact reverse-order downgrade**

Append to backend/migrations/versions/0006_governance.py:

~~~python
def downgrade() -> None:
    op.drop_index("ix_release_decisions_capability", table_name="release_decisions")
    op.drop_index("ix_release_decisions_beta_program_id", table_name="release_decisions")
    op.drop_table("release_decisions")
    op.drop_index(
        "ix_self_marketing_risk_reviews_project_id",
        table_name="self_marketing_risk_reviews",
    )
    op.drop_table("self_marketing_risk_reviews")
    op.drop_index(
        "ix_self_marketing_budget_approvals_project_id",
        table_name="self_marketing_budget_approvals",
    )
    op.drop_table("self_marketing_budget_approvals")
    op.drop_index(
        "ix_beta_blind_review_results_project_id",
        table_name="beta_blind_review_results",
    )
    op.drop_index(
        "ix_beta_blind_review_results_beta_program_id",
        table_name="beta_blind_review_results",
    )
    op.drop_table("beta_blind_review_results")
    op.drop_index(
        "ix_beta_content_assessments_beta_program_id",
        table_name="beta_content_assessments",
    )
    op.drop_table("beta_content_assessments")
    op.drop_index(
        "ix_beta_project_enrollments_project_id",
        table_name="beta_project_enrollments",
    )
    op.drop_index(
        "ix_beta_project_enrollments_beta_program_id",
        table_name="beta_project_enrollments",
    )
    op.drop_table("beta_project_enrollments")
    op.drop_table("beta_programs")
    op.drop_index(
        "ix_launch_external_attestations_topic",
        table_name="launch_external_attestations",
    )
    op.drop_table("launch_external_attestations")
    op.drop_index(
        "ix_operational_drill_evidence_drill_kind",
        table_name="operational_drill_evidence",
    )
    op.drop_table("operational_drill_evidence")
    op.drop_table("disaster_recovery_markers")
    op.drop_index(
        "ix_export_verifications_media_asset_id",
        table_name="export_verifications",
    )
    op.drop_table("export_verifications")
    op.drop_index(
        "ix_complaint_cases_identity_asset_id", table_name="complaint_cases"
    )
    op.drop_index("ix_complaint_cases_content_id", table_name="complaint_cases")
    op.drop_index("ix_complaint_cases_project_id", table_name="complaint_cases")
    op.drop_table("complaint_cases")
    op.drop_index(
        "ix_deletion_execution_steps_deletion_plan_id",
        table_name="deletion_execution_steps",
    )
    op.drop_table("deletion_execution_steps")
    op.drop_index("uq_open_deletion_target", table_name="deletion_plans")
    op.drop_index("ix_deletion_plans_project_id", table_name="deletion_plans")
    op.drop_index("ix_deletion_plans_account_id", table_name="deletion_plans")
    op.drop_table("deletion_plans")
    op.drop_index(
        "uq_open_identity_deletion", table_name="identity_deletion_requests"
    )
    op.drop_index(
        "ix_identity_deletion_requests_project_id",
        table_name="identity_deletion_requests",
    )
    op.drop_index(
        "ix_identity_deletion_requests_account_id",
        table_name="identity_deletion_requests",
    )
    op.drop_table("identity_deletion_requests")
    op.drop_index(
        "ix_capability_quota_reservations_project_id",
        table_name="capability_quota_reservations",
    )
    op.drop_index(
        "ix_capability_quota_reservations_account_id",
        table_name="capability_quota_reservations",
    )
    op.drop_index(
        "ix_capability_quota_reservations_capability",
        table_name="capability_quota_reservations",
    )
    op.drop_table("capability_quota_reservations")
    op.drop_table("generation_identity_uses")
    op.drop_table("identity_asset_consents")
    op.drop_index(
        "ix_provider_identity_assets_subject_id",
        table_name="provider_identity_assets",
    )
    op.drop_index(
        "ix_provider_identity_assets_project_id",
        table_name="provider_identity_assets",
    )
    op.drop_index(
        "ix_provider_identity_assets_account_id",
        table_name="provider_identity_assets",
    )
    op.drop_table("provider_identity_assets")
    op.drop_index(
        "uq_identity_enrollment_provider_task", table_name="identity_enrollments"
    )
    op.drop_index(
        "ix_identity_enrollments_project_id", table_name="identity_enrollments"
    )
    op.drop_index(
        "ix_identity_enrollments_account_id", table_name="identity_enrollments"
    )
    op.drop_table("identity_enrollments")
    op.drop_index(
        "uq_active_consent_subject_kind", table_name="consent_records"
    )
    op.drop_index("ix_consent_records_subject_id", table_name="consent_records")
    op.drop_index("ix_consent_records_project_id", table_name="consent_records")
    op.drop_index("ix_consent_records_account_id", table_name="consent_records")
    op.drop_table("consent_records")
    op.drop_index("ix_identity_subjects_project_id", table_name="identity_subjects")
    op.drop_index("ix_identity_subjects_account_id", table_name="identity_subjects")
    op.drop_table("identity_subjects")
    op.drop_index(
        "ix_capability_gate_attestations_gate_kind",
        table_name="capability_gate_attestations",
    )
    op.drop_index(
        "ix_capability_gate_attestations_capability",
        table_name="capability_gate_attestations",
    )
    op.drop_table("capability_gate_attestations")
    op.drop_index(
        "ix_provider_capability_entitlements_capability",
        table_name="provider_capability_entitlements",
    )
    op.drop_table("provider_capability_entitlements")
    op.drop_index(
        "ix_commercial_contract_attestations_capability",
        table_name="commercial_contract_attestations",
    )
    op.drop_table("commercial_contract_attestations")
    op.drop_index(
        "uq_active_capability_project_grant",
        table_name="capability_cohort_grants",
    )
    op.drop_index(
        "ix_capability_cohort_grants_project_id",
        table_name="capability_cohort_grants",
    )
    op.drop_index(
        "ix_capability_cohort_grants_account_id",
        table_name="capability_cohort_grants",
    )
    op.drop_table("capability_cohort_grants")
    op.drop_table("capability_feature_flags")
    op.drop_column("media_assets", "deletion_reason")
    op.drop_column("media_assets", "deleted_at")
    op.drop_column("media_assets", "quarantine_reason")
    op.drop_column("media_assets", "quarantined_at")
~~~

- [ ] **Step 9: Run PostgreSQL migration, downgrade, and re-upgrade tests**

Run:

~~~bash
cd backend && uv run alembic upgrade 0006_governance && uv run pytest tests/integration/governance/test_migration.py -v
~~~

Expected: PASS, 2 tests; the revision chain is 0005_resellers to 0006_governance and downgrade removes only Plan 06 objects.

- [ ] **Step 10: Commit the frozen migration**

~~~bash
git add backend/migrations/versions/0006_governance.py backend/tests/integration/governance/test_migration.py
git commit -m "feat: add frozen governance migration"
~~~

### Task 4: Enforce independent feature flags, production entitlements, gates, and quotas

**Files:**
- Create: backend/src/ip_saas/modules/governance/feature_flags.py
- Create: backend/src/ip_saas/modules/governance/eligibility.py
- Create: backend/src/ip_saas/modules/governance/router.py
- Modify: backend/src/ip_saas/modules/governance/schemas.py
- Modify: backend/src/ip_saas/api.py
- Modify: backend/tests/support/governance_rig.py
- Test: backend/tests/unit/governance/test_feature_flags.py
- Test: backend/tests/integration/governance/test_eligibility.py

- [ ] **Step 1: Write the failing independent-flag tests**

Create backend/tests/unit/governance/test_feature_flags.py:

~~~python
from types import SimpleNamespace
from unittest.mock import Mock
from uuid import uuid4

import pytest

from ip_saas.common.errors import Conflict, Forbidden, NotFound
from ip_saas.common.outbox import OutboxWriter
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.governance.enums import BetaCapability, CapabilityStage
from ip_saas.modules.governance.feature_flags import CapabilityFeatureFlagService


def test_enabling_voice_clone_does_not_enable_other_capabilities() -> None:
    session = Mock()
    voice = SimpleNamespace(
        capability=BetaCapability.VOICE_CLONE.value,
        stage=CapabilityStage.INTERNAL_POC.value,
        version_no=1,
    )
    portrait = SimpleNamespace(
        capability=BetaCapability.SEEDANCE_AUTHORIZED_PORTRAIT.value,
        stage=CapabilityStage.OFF.value,
        version_no=1,
    )
    session.scalar.side_effect = [voice, portrait]
    gates = Mock()
    gates.require_stage_gates.return_value = None
    service = CapabilityFeatureFlagService(gates, Mock(), Mock())
    admin = ActorContext(uuid4(), uuid4(), ActorKind.PLATFORM_ADMIN)
    service.set_stage(
        session,
        admin,
        BetaCapability.VOICE_CLONE,
        CapabilityStage.CLOSED_BETA,
        reason="voice PoC passed",
        release_decision_id=None,
    )
    assert voice.stage == CapabilityStage.CLOSED_BETA.value
    assert portrait.stage == CapabilityStage.OFF.value


def test_off_cannot_jump_directly_to_public_beta() -> None:
    session = Mock()
    session.scalar.return_value = SimpleNamespace(
        capability=BetaCapability.VOICE_CLONE.value,
        stage=CapabilityStage.OFF.value,
        version_no=0,
    )
    gates = Mock()
    service = CapabilityFeatureFlagService(gates, Mock(), Mock())
    admin = ActorContext(uuid4(), uuid4(), ActorKind.PLATFORM_ADMIN)
    with pytest.raises(Conflict, match="illegal capability stage transition"):
        service.set_stage(
            session,
            admin,
            BetaCapability.VOICE_CLONE,
            CapabilityStage.PUBLIC_BETA,
            reason="invalid shortcut",
            release_decision_id=uuid4(),
        )
    gates.require_stage_gates.assert_not_called()


def test_reseller_cannot_change_a_capability_flag() -> None:
    service = CapabilityFeatureFlagService(Mock(), Mock(), Mock())
    reseller = ActorContext(uuid4(), uuid4(), ActorKind.RESELLER_L1)
    with pytest.raises(Forbidden, match="platform admin"):
        service.set_stage(
            Mock(),
            reseller,
            BetaCapability.TRADITIONAL_DIGITAL_HUMAN,
            CapabilityStage.INTERNAL_POC,
            reason="attempt",
            release_decision_id=None,
        )
~~~

- [ ] **Step 2: Run the flag tests and verify the missing-service failure**

Run:

~~~bash
cd backend && uv run pytest tests/unit/governance/test_feature_flags.py -v
~~~

Expected: FAIL during collection because governance.feature_flags does not exist.

- [ ] **Step 3: Implement gate requirements and versioned stage changes**

Create backend/src/ip_saas/modules/governance/feature_flags.py:

~~~python
from uuid import UUID

from sqlalchemy import func, select
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.common.clock import Clock
from ip_saas.common.errors import Conflict, Forbidden, NotFound
from ip_saas.modules.accounts.context import ActorContext, ActorKind

from .enums import BetaCapability, CapabilityStage, EntitlementStatus, GateKind
from .models import (
    CapabilityFeatureFlag,
    ProviderCapabilityEntitlement,
    ReleaseDecision,
)


POC_GATES = frozenset(
    {
        GateKind.PRODUCTION_PERMISSION,
        GateKind.PROVIDER_QUOTA,
        GateKind.WRITTEN_CONTRACT,
    }
)
FULL_BETA_GATES = frozenset(GateKind)
ALLOWED_STAGE_TRANSITIONS = {
    CapabilityStage.OFF: frozenset({CapabilityStage.INTERNAL_POC}),
    CapabilityStage.INTERNAL_POC: frozenset(
        {CapabilityStage.OFF, CapabilityStage.CLOSED_BETA}
    ),
    CapabilityStage.CLOSED_BETA: frozenset(
        {
            CapabilityStage.OFF,
            CapabilityStage.INTERNAL_POC,
            CapabilityStage.PUBLIC_BETA,
        }
    ),
    CapabilityStage.PUBLIC_BETA: frozenset(
        {CapabilityStage.OFF, CapabilityStage.CLOSED_BETA}
    ),
}


def lock_capability_governance(
    session: Session,
    capability: BetaCapability,
) -> None:
    lock_name = f"governance-capability:{capability.value}"
    session.execute(select(func.pg_advisory_xact_lock(
        func.hashtextextended(lock_name, 0)
    )))


class GateReader:
    def __init__(self, clock: Clock) -> None:
        self.clock = clock

    def require_stage_gates(
        self,
        session: Session,
        capability: BetaCapability,
        stage: CapabilityStage,
    ) -> ProviderCapabilityEntitlement:
        from .eligibility import CapabilityEligibilityService

        now = self.clock.now()
        entitlement = CapabilityEligibilityService.require_current_entitlement(
            session, capability, now=now, for_update=True
        )
        CapabilityEligibilityService.require_gate_set(
            session,
            capability,
            POC_GATES if stage is CapabilityStage.INTERNAL_POC else FULL_BETA_GATES,
            entitlement=entitlement,
            now=now,
        )
        return entitlement


class CapabilityFeatureFlagService:
    def __init__(
        self,
        gates: GateReader,
        audit: AuditWriter,
        clock: Clock,
    ) -> None:
        self.gates = gates
        self.audit = audit
        self.clock = clock

    def set_stage(
        self,
        session: Session,
        actor: ActorContext,
        capability: BetaCapability,
        stage: CapabilityStage,
        *,
        reason: str,
        release_decision_id: UUID | None,
    ) -> CapabilityFeatureFlag:
        if actor.kind != ActorKind.PLATFORM_ADMIN:
            raise Forbidden("platform admin is required")
        lock_capability_governance(session, capability)
        row = session.scalar(
            select(CapabilityFeatureFlag)
            .where(CapabilityFeatureFlag.capability == capability.value)
            .with_for_update()
        )
        if row is None:
            row = CapabilityFeatureFlag(
                capability=capability.value,
                stage=CapabilityStage.OFF.value,
                version_no=0,
                reason="initially disabled",
                changed_by=actor.actor_id,
                changed_at=self.clock.now(),
            )
            session.add(row)
            session.flush()
        old_stage = CapabilityStage(row.stage)
        if old_stage is stage:
            if release_decision_id is not None:
                raise Conflict("release decision cannot be consumed by a stage no-op")
            return row
        if stage not in ALLOWED_STAGE_TRANSITIONS[old_stage]:
            raise Conflict("illegal capability stage transition")
        if stage is CapabilityStage.OFF:
            if release_decision_id is not None:
                raise Conflict("rollback does not consume a release decision")
            entitlement = None
        else:
            entitlement = self.gates.require_stage_gates(session, capability, stage)
        if stage is CapabilityStage.PUBLIC_BETA:
            now = self.clock.now()
            if (
                entitlement.status != EntitlementStatus.ACTIVE.value
                or entitlement.expires_at <= now
            ):
                raise Conflict("current entitlement is inactive or expired")
            if release_decision_id is None:
                raise Conflict("public beta requires an approved release decision")
            decision = session.scalar(
                select(ReleaseDecision)
                .where(ReleaseDecision.id == release_decision_id)
                .with_for_update()
            )
            if (decision is None or decision.decision != "approve_public_beta"
                    or decision.capability != capability.value):
                raise Conflict("release decision does not approve public beta")
            if decision.executed_at is not None:
                raise Conflict("release decision was already executed")
            decision_binding = (
                decision.entitlement_id,
                decision.provider_account_fingerprint,
                decision.region,
                decision.model_id,
                decision.model_version,
                decision.release_snapshot_sha256,
            )
            current_binding = (
                entitlement.id,
                entitlement.provider_account_fingerprint,
                entitlement.region,
                entitlement.model_id,
                entitlement.model_version,
                entitlement.release_snapshot_sha256,
            )
            if decision_binding != current_binding:
                raise Conflict("release decision is bound to an old entitlement snapshot")
            decision.executed_at = self.clock.now()
        elif release_decision_id is not None:
            raise Conflict("only public beta promotion consumes a release decision")
        row.stage = stage.value
        row.version_no += 1
        row.reason = reason[:500]
        row.changed_by = actor.actor_id
        row.changed_at = self.clock.now()
        self.audit.write(
            session,
            actor=actor,
            action="capability.stage.changed",
            target_type="capability_feature_flag",
            target_id=row.id,
            metadata={
                "capability": capability.value,
                "from": old_stage.value,
                "to": stage.value,
                "version_no": row.version_no,
            },
        )
        return row

    def require_stage(
        self,
        session: Session,
        capability: BetaCapability,
    ) -> CapabilityStage:
        row = session.scalar(
            select(CapabilityFeatureFlag).where(
                CapabilityFeatureFlag.capability == capability.value
            )
        )
        if row is None or row.stage == CapabilityStage.OFF.value:
            raise Forbidden(f"{capability.value} is disabled")
        return CapabilityStage(row.stage)
~~~

- [ ] **Step 4: Run the independent-flag tests**

Run:

~~~bash
cd backend && uv run pytest tests/unit/governance/test_feature_flags.py -v
~~~

Expected: PASS, 3 tests; the legal state graph rejects OFF-to-PUBLIC_BETA before gate or decision access.

- [ ] **Step 5: Write failing eligibility, expiry, minor, and quota tests**

Create backend/tests/integration/governance/test_eligibility.py:

~~~python
from datetime import timedelta
from uuid import UUID

import pytest

from ip_saas.common.errors import Conflict, Forbidden, NotFound
from ip_saas.modules.governance.enums import BetaCapability


def test_expired_gate_blocks_only_its_capability(
    db_session,
    eligibility,
    customer_actor,
    seeded_governance,
    fake_clock,
) -> None:
    seeded_governance.expire_gate(BetaCapability.VOICE_CLONE, fake_clock.now())
    with pytest.raises(Forbidden, match="gate"):
        eligibility.require_project_capability(
            db_session,
            customer_actor,
            seeded_governance.project_id,
            BetaCapability.VOICE_CLONE,
        )
    eligibility.require_project_capability(
        db_session,
        customer_actor,
        seeded_governance.project_id,
        BetaCapability.SEEDANCE_AUTHORIZED_PORTRAIT,
    )


def test_old_entitlement_gate_set_cannot_authorize_a_rotated_snapshot(
    db_session,
    eligibility,
    customer_actor,
    seeded_governance,
) -> None:
    old = seeded_governance.current_entitlement(BetaCapability.VOICE_CLONE)
    current = seeded_governance.rotate_entitlement(
        BetaCapability.VOICE_CLONE,
        model_version="voice-v2",
        release_snapshot_sha256="2" * 64,
    )
    assert current.id != old.id
    with pytest.raises(Forbidden, match="gate"):
        eligibility.require_project_capability(
            db_session,
            customer_actor,
            seeded_governance.project_id,
            BetaCapability.VOICE_CLONE,
        )
    seeded_governance.pass_gate_set_for(current)
    assert eligibility.require_project_capability(
        db_session,
        customer_actor,
        seeded_governance.project_id,
        BetaCapability.VOICE_CLONE,
    ).id == current.id


def test_minor_subject_is_never_eligible_for_v1_modeling(
    db_session,
    eligibility,
    customer_actor,
    seeded_governance,
) -> None:
    with pytest.raises(Forbidden, match="minor identity modeling"):
        eligibility.require_identity_subject(
            db_session,
            customer_actor,
            seeded_governance.project_id,
            seeded_governance.minor_subject_id,
        )


def test_reseller_support_grant_never_opens_identity_capability(
    db_session,
    eligibility,
    l1_actor_with_media_support,
    seeded_governance,
) -> None:
    with pytest.raises(Forbidden, match="customer or platform operator"):
        eligibility.require_project_capability(
            db_session,
            l1_actor_with_media_support,
            seeded_governance.project_id,
            BetaCapability.VOICE_CLONE,
        )


def test_durable_quota_counts_active_and_settled_reservations(
    db_session,
    quota_service,
    customer_actor,
    seeded_governance,
    fake_clock,
) -> None:
    for index in range(seeded_governance.minute_quota):
        quota_service.reserve(
            db_session,
            customer_actor,
            seeded_governance.project_id,
            BetaCapability.VOICE_CLONE,
            units=1,
            idempotency_key=f"voice-quota-{index}",
        )
    with pytest.raises(Forbidden, match="per-minute"):
        quota_service.reserve(
            db_session,
            customer_actor,
            seeded_governance.project_id,
            BetaCapability.VOICE_CLONE,
            units=1,
            idempotency_key="voice-quota-overflow",
        )
    fake_clock.advance(timedelta(minutes=1, seconds=1))
    quota_service.reserve(
        db_session,
        customer_actor,
        seeded_governance.project_id,
        BetaCapability.VOICE_CLONE,
        units=1,
        idempotency_key="voice-quota-next-minute",
    )


def test_quota_idempotency_is_account_scoped_and_permission_first(
    governance_rig,
) -> None:
    case = governance_rig.quota_idempotency_harness()
    first = case.reserve(case.account_a_actor, case.account_a_project,
                         key="same-client-key", units=1)
    before_replay = case.durable_counts()
    replay = case.reserve(case.account_a_actor, case.account_a_project,
                          key="same-client-key", units=1)
    assert replay.id == first.id
    assert case.durable_counts() == before_replay

    second_account = case.reserve(
        case.account_b_actor, case.account_b_project,
        key="same-client-key", units=1,
    )
    assert second_account.id != first.id
    with pytest.raises(Conflict, match="different input"):
        case.reserve(case.account_a_actor, case.account_a_project,
                     key="same-client-key", units=2)
    with pytest.raises(Conflict, match="different input"):
        case.reserve(case.account_a_actor, case.account_a_second_project,
                     key="same-client-key", units=1)
    with pytest.raises((Forbidden, NotFound)):
        case.reserve(case.account_b_actor, case.account_a_project,
                     key="same-client-key", units=1)
    assert case.lookup_returned_foreign_reservation is False


@pytest.mark.parametrize("first_operation", ["promotion", "rotation"])
def test_postgres_capability_lock_serializes_entitlement_rotation_and_public_promotion(
    governance_rig,
    first_operation: str,
) -> None:
    race = governance_rig.entitlement_public_beta_race(
        BetaCapability.VOICE_CLONE
    )
    race.start_first(first_operation)
    race.wait_until_first_transaction_holds_capability_lock()
    race.start_second(
        "rotation" if first_operation == "promotion" else "promotion"
    )
    assert race.second_transaction_is_blocked_in_postgres()
    race.commit_first()
    second = race.await_second()

    assert race.distinct_sqlalchemy_session_count() == 2
    assert race.maximum_simultaneous_capability_critical_sections() == 1
    if first_operation == "promotion":
        assert second.is_conflict("demote capability before entitlement rotation")
        assert race.stage() == "public_beta"
        assert race.current_entitlement_binding() == race.decision_binding()
        assert race.decision_executed_at() is not None
    else:
        assert second.is_conflict("old entitlement snapshot")
        assert race.stage() == "closed_beta"
        assert race.current_entitlement_id() == race.rotated_entitlement_id()
        assert race.decision_executed_at() is None


def test_expired_entitlement_can_be_replaced_without_manual_database_edits(
    governance_rig,
) -> None:
    case = governance_rig.expired_entitlement_rotation_case(
        BetaCapability.VOICE_CLONE
    )
    expired = case.expired_entitlement()
    replacement = case.rotate(model_version="voice-v2")
    assert replacement.id != expired.id
    assert case.entitlement(expired.id).status == "suspended"
    assert case.current_entitlement_id() == replacement.id
    assert case.audit_action() == "provider.entitlement.rotated"


def test_initial_entitlement_provision_requires_off_stage_and_zero_history(
    governance_rig,
) -> None:
    case = governance_rig.initial_entitlement_provision_case(
        BetaCapability.TRADITIONAL_DIGITAL_HUMAN
    )
    entitlement = case.provision_initial()
    assert case.stage() == "off"
    assert case.entitlement_ids() == [entitlement.id]
    assert case.audit_action() == "provider.entitlement.provisioned"
    with pytest.raises(Conflict, match="already has entitlement history"):
        case.provision_initial()

    non_off = governance_rig.initial_entitlement_provision_case(
        BetaCapability.SEEDANCE_AUTHORIZED_PORTRAIT,
        stage="closed_beta",
    )
    with pytest.raises(Conflict, match="OFF stage"):
        non_off.provision_initial()


def test_authenticated_platform_route_is_the_only_initial_provision_path(
    governance_rig,
) -> None:
    case = governance_rig.initial_entitlement_provision_case(
        BetaCapability.TRADITIONAL_DIGITAL_HUMAN
    )
    response = case.platform_admin_post_initial()
    assert response.status_code == 201
    assert response.json()["capability"] == "traditional_digital_human"
    assert case.entitlement_ids() == [UUID(response.json()["id"])]
    assert case.direct_model_insert_count() == 0
    assert case.service_provision_initial_call_count() == 1
~~~

- [ ] **Step 6: Run eligibility tests and verify the missing-service failure**

Run:

~~~bash
cd backend && uv run pytest tests/integration/governance/test_eligibility.py -v
~~~

Expected: FAIL during collection because governance.eligibility does not exist.

- [ ] **Step 7: Implement valid-gate and active-entitlement checks**

Create backend/src/ip_saas/modules/governance/eligibility.py:

~~~python
from datetime import datetime, timedelta
from uuid import UUID

from sqlalchemy import func, select
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.common.clock import Clock
from ip_saas.common.errors import Conflict, Forbidden, NotFound
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.projects.access import ProjectAccessService

from .enums import (
    BetaCapability,
    CapabilityStage,
    ConsentStatus,
    EntitlementStatus,
    GateKind,
    GateResult,
    QuotaReservationStatus,
)
from .feature_flags import (
    FULL_BETA_GATES,
    POC_GATES,
    lock_capability_governance,
)
from .models import (
    CapabilityCohortGrant,
    CapabilityFeatureFlag,
    CapabilityGateAttestation,
    CapabilityQuotaReservation,
    IdentitySubject,
    ProviderCapabilityEntitlement,
)


def require_governance_project(
    access: ProjectAccessService,
    session: Session,
    actor: ActorContext,
    project_id: UUID,
):
    if actor.kind not in {
        ActorKind.C_USER,
        ActorKind.PLATFORM_ADMIN,
        ActorKind.PLATFORM_OPERATOR,
    }:
        raise Forbidden("customer or platform operator identity is required")
    project = access.require_editor(session, actor, project_id)
    if project.owner_account_id != actor.account_id:
        raise Forbidden("governed identity requires the owning account")
    return project


def lock_account_idempotency(
    session: Session,
    account_id: UUID,
    namespace: str,
    idempotency_key: str,
) -> None:
    lock_name = f"{namespace}:{account_id}:{idempotency_key}"
    session.execute(select(func.pg_advisory_xact_lock(
        func.hashtextextended(lock_name, 0)
    )))


class ProviderEntitlementRotationService:
    def __init__(self, audit: AuditWriter, clock: Clock) -> None:
        self.audit = audit
        self.clock = clock

    @staticmethod
    def _lock_history(
        session: Session,
        capability: BetaCapability,
    ) -> tuple[ProviderCapabilityEntitlement, ...]:
        return tuple(session.scalars(
            select(ProviderCapabilityEntitlement)
            .where(
                ProviderCapabilityEntitlement.capability == capability.value
            )
            .order_by(
                ProviderCapabilityEntitlement.checked_at.desc(),
                ProviderCapabilityEntitlement.id.desc(),
            )
            .with_for_update()
        ))

    def provision_initial(
        self,
        session: Session,
        actor: ActorContext,
        capability: BetaCapability,
        *,
        provider: str,
        region: str,
        provider_account_fingerprint: str,
        model_id: str,
        model_version: str,
        release_snapshot_sha256: str,
        quota_per_minute: int,
        quota_per_day: int,
        quota_per_month: int,
        contract_attestation_id: UUID,
        evidence_asset_id: UUID,
        expires_at: datetime,
    ) -> ProviderCapabilityEntitlement:
        if actor.kind != ActorKind.PLATFORM_ADMIN:
            raise Forbidden("platform admin is required")
        lock_capability_governance(session, capability)
        flag = session.scalar(
            select(CapabilityFeatureFlag)
            .where(CapabilityFeatureFlag.capability == capability.value)
            .with_for_update()
        )
        if flag is None or flag.stage != CapabilityStage.OFF.value:
            raise Conflict("initial entitlement requires the OFF stage")
        if self._lock_history(session, capability):
            raise Conflict(f"{capability.value} already has entitlement history")
        checked_at = self.clock.now()
        entitlement = ProviderCapabilityEntitlement(
            capability=capability.value,
            provider=provider,
            region=region,
            provider_account_fingerprint=provider_account_fingerprint,
            model_id=model_id,
            model_version=model_version,
            release_snapshot_sha256=release_snapshot_sha256,
            checked_at=checked_at,
            checked_by=actor.actor_id,
            status=EntitlementStatus.ACTIVE.value,
            quota_per_minute=quota_per_minute,
            quota_per_day=quota_per_day,
            quota_per_month=quota_per_month,
            contract_attestation_id=contract_attestation_id,
            evidence_asset_id=evidence_asset_id,
            expires_at=expires_at,
        )
        if entitlement.expires_at <= checked_at:
            raise Conflict("initial entitlement must expire in the future")
        session.add(entitlement)
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="provider.entitlement.provisioned",
            target_type="provider_capability_entitlement",
            target_id=entitlement.id,
            metadata={
                "capability": capability.value,
                "release_snapshot_sha256": entitlement.release_snapshot_sha256,
            },
        )
        return entitlement

    def rotate(
        self,
        session: Session,
        actor: ActorContext,
        capability: BetaCapability,
        *,
        provider: str,
        region: str,
        provider_account_fingerprint: str,
        model_id: str,
        model_version: str,
        release_snapshot_sha256: str,
        quota_per_minute: int,
        quota_per_day: int,
        quota_per_month: int,
        contract_attestation_id: UUID,
        evidence_asset_id: UUID,
        expires_at: datetime,
    ) -> ProviderCapabilityEntitlement:
        if actor.kind != ActorKind.PLATFORM_ADMIN:
            raise Forbidden("platform admin is required")
        lock_capability_governance(session, capability)
        flag = session.scalar(
            select(CapabilityFeatureFlag)
            .where(CapabilityFeatureFlag.capability == capability.value)
            .with_for_update()
        )
        if flag is not None and flag.stage == CapabilityStage.PUBLIC_BETA.value:
            raise Conflict(
                "demote capability before entitlement rotation"
            )
        checked_at = self.clock.now()
        history = self._lock_history(session, capability)
        if not history:
            raise Conflict(
                f"{capability.value} has no entitlement history; use provision_initial"
            )
        active_rows = tuple(
            row for row in history
            if row.status == EntitlementStatus.ACTIVE.value
        )
        if len(active_rows) > 1:
            raise Conflict(f"{capability.value} has multiple active entitlements")
        current = history[0]
        if active_rows and active_rows[0].id != current.id:
            raise Conflict("latest entitlement is not the active entitlement")
        if current.status == EntitlementStatus.ACTIVE.value:
            current.status = EntitlementStatus.SUSPENDED.value
        replacement = ProviderCapabilityEntitlement(
            capability=capability.value,
            provider=provider,
            region=region,
            provider_account_fingerprint=provider_account_fingerprint,
            model_id=model_id,
            model_version=model_version,
            release_snapshot_sha256=release_snapshot_sha256,
            status=EntitlementStatus.ACTIVE.value,
            quota_per_minute=quota_per_minute,
            quota_per_day=quota_per_day,
            quota_per_month=quota_per_month,
            contract_attestation_id=contract_attestation_id,
            evidence_asset_id=evidence_asset_id,
            checked_at=checked_at,
            expires_at=expires_at,
            checked_by=actor.actor_id,
        )
        if replacement.expires_at <= checked_at:
            raise Conflict("replacement entitlement must expire in the future")
        session.add(replacement)
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="provider.entitlement.rotated",
            target_type="provider_capability_entitlement",
            target_id=replacement.id,
            metadata={
                "capability": capability.value,
                "superseded_entitlement_id": str(current.id),
                "release_snapshot_sha256": release_snapshot_sha256,
            },
        )
        return replacement


class CapabilityEligibilityService:
    def __init__(
        self,
        access: ProjectAccessService,
        clock: Clock,
    ) -> None:
        self.access = access
        self.clock = clock

    @staticmethod
    def require_current_entitlement(
        session: Session,
        capability: BetaCapability,
        *,
        now: datetime,
        for_update: bool = False,
    ) -> ProviderCapabilityEntitlement:
        statement = (
            select(ProviderCapabilityEntitlement)
            .where(
                ProviderCapabilityEntitlement.capability == capability.value,
                ProviderCapabilityEntitlement.status == EntitlementStatus.ACTIVE.value,
                ProviderCapabilityEntitlement.expires_at > now,
            )
            .order_by(ProviderCapabilityEntitlement.checked_at.desc())
            .limit(2)
        )
        if for_update:
            statement = statement.with_for_update().execution_options(
                populate_existing=True
            )
        rows = tuple(session.scalars(statement))
        if not rows:
            raise Forbidden(f"{capability.value} has no active production entitlement")
        if len(rows) != 1:
            raise Conflict(f"{capability.value} has multiple active entitlements")
        return rows[0]

    @staticmethod
    def require_gate_set(
        session: Session,
        capability: BetaCapability,
        required: frozenset[GateKind],
        *,
        entitlement: ProviderCapabilityEntitlement,
        now,
    ) -> None:
        for gate_kind in sorted(required, key=lambda item: item.value):
            row = session.scalar(
                select(CapabilityGateAttestation)
                .where(
                    CapabilityGateAttestation.capability == capability.value,
                    CapabilityGateAttestation.gate_kind == gate_kind.value,
                    CapabilityGateAttestation.result == GateResult.PASS.value,
                    CapabilityGateAttestation.entitlement_id == entitlement.id,
                    CapabilityGateAttestation.provider_account_fingerprint
                    == entitlement.provider_account_fingerprint,
                    CapabilityGateAttestation.region == entitlement.region,
                    CapabilityGateAttestation.model_id == entitlement.model_id,
                    CapabilityGateAttestation.model_version
                    == entitlement.model_version,
                    CapabilityGateAttestation.release_snapshot_sha256
                    == entitlement.release_snapshot_sha256,
                    CapabilityGateAttestation.valid_until > now,
                )
                .order_by(CapabilityGateAttestation.executed_at.desc())
                .limit(1)
            )
            if row is None:
                raise Forbidden(
                    f"{capability.value} gate is not passed: {gate_kind.value}"
                )

    def require_project_capability(
        self,
        session: Session,
        actor: ActorContext,
        project_id: UUID,
        capability: BetaCapability,
    ) -> ProviderCapabilityEntitlement:
        project = require_governance_project(
            self.access, session, actor, project_id
        )
        now = self.clock.now()
        flag = session.scalar(
            select(CapabilityFeatureFlag).where(
                CapabilityFeatureFlag.capability == capability.value
            )
        )
        if flag is None or flag.stage == CapabilityStage.OFF.value:
            raise Forbidden(f"{capability.value} is disabled")
        grant = session.scalar(
            select(CapabilityCohortGrant).where(
                CapabilityCohortGrant.capability == capability.value,
                CapabilityCohortGrant.account_id == project.owner_account_id,
                CapabilityCohortGrant.project_id == project.id,
                CapabilityCohortGrant.revoked_at.is_(None),
                CapabilityCohortGrant.allowed_until > now,
            )
        )
        if grant is None:
            raise Forbidden(f"{capability.value} has no active project grant")
        entitlement = self.require_current_entitlement(
            session, capability, now=now
        )
        required = (
            POC_GATES
            if flag.stage == CapabilityStage.INTERNAL_POC.value
            else FULL_BETA_GATES
        )
        self.require_gate_set(
            session,
            capability,
            required,
            entitlement=entitlement,
            now=now,
        )
        return entitlement

    def require_identity_subject(
        self,
        session: Session,
        actor: ActorContext,
        project_id: UUID,
        subject_id: UUID,
    ) -> IdentitySubject:
        project = require_governance_project(
            self.access, session, actor, project_id
        )
        subject = session.scalar(
            select(IdentitySubject).where(
                IdentitySubject.id == subject_id,
                IdentitySubject.project_id == project.id,
                IdentitySubject.account_id == actor.account_id,
            )
        )
        if subject is None:
            raise NotFound("identity subject")
        if subject.age_class == "minor":
            raise Forbidden("minor identity modeling is disabled in V1 public beta")
        if subject.age_class != "adult" or subject.disabled_at is not None:
            raise Forbidden("an active, age-verified adult identity subject is required")
        return subject


class CapabilityQuotaService:
    def __init__(
        self,
        eligibility: CapabilityEligibilityService,
        audit: AuditWriter,
        clock: Clock,
    ) -> None:
        self.eligibility = eligibility
        self.audit = audit
        self.clock = clock

    def reserve(
        self,
        session: Session,
        actor: ActorContext,
        project_id: UUID,
        capability: BetaCapability,
        *,
        units: int,
        idempotency_key: str,
    ) -> CapabilityQuotaReservation:
        project = require_governance_project(
            self.eligibility.access, session, actor, project_id
        )
        lock_account_idempotency(
            session, actor.account_id, "capability-quota", idempotency_key
        )
        existing = session.scalar(
            select(CapabilityQuotaReservation).where(
                CapabilityQuotaReservation.account_id == actor.account_id,
                CapabilityQuotaReservation.idempotency_key == idempotency_key
            )
        )
        if existing is not None:
            if (
                existing.project_id != project_id
                or existing.capability != capability.value
                or existing.reserved_units != units
            ):
                raise Conflict("quota idempotency key has different input")
            return existing
        entitlement = self.eligibility.require_project_capability(
            session, actor, project_id, capability
        )
        now = self.clock.now()
        session.execute(
            select(ProviderCapabilityEntitlement)
            .where(ProviderCapabilityEntitlement.id == entitlement.id)
            .with_for_update()
        )
        self._require_limit(
            session,
            capability,
            now - timedelta(minutes=1),
            units,
            entitlement.quota_per_minute,
            "per-minute",
        )
        self._require_limit(
            session,
            capability,
            now - timedelta(days=1),
            units,
            entitlement.quota_per_day,
            "per-day",
        )
        month_start = now.replace(day=1, hour=0, minute=0, second=0, microsecond=0)
        self._require_limit(
            session,
            capability,
            month_start,
            units,
            entitlement.quota_per_month,
            "per-month",
        )
        row = CapabilityQuotaReservation(
            capability=capability.value,
            account_id=project.owner_account_id,
            project_id=project.id,
            task_record_id=None,
            reserved_units=units,
            status=QuotaReservationStatus.ACTIVE.value,
            idempotency_key=idempotency_key,
            reserved_at=now,
            expires_at=now + timedelta(hours=1),
        )
        session.add(row)
        session.flush()
        return row

    def mark_send_started(
        self,
        session: Session,
        *,
        task: TaskRecord,
        attempt: GovernedProviderRequestAttempt,
        worker_attempt_no: int,
    ) -> GovernedProviderRequestAttempt:
        locked = session.scalar(
            select(GovernedProviderRequestAttempt)
            .where(GovernedProviderRequestAttempt.id == attempt.id)
            .with_for_update()
        )
        if (
            locked is None
            or task.status != TaskStatus.RUNNING
            or task.attempt_no != worker_attempt_no
            or locked.worker_attempt_no != worker_attempt_no
        ):
            raise Conflict("stale worker cannot mark provider send")
        if locked.send_started_at is not None:
            raise Conflict("provider send outcome requires reconciliation")
        locked.send_started_at = self.now()
        session.flush()
        return locked

    @staticmethod
    def _require_limit(
        session: Session,
        capability: BetaCapability,
        since,
        requested: int,
        limit: int,
        label: str,
    ) -> None:
        used = session.scalar(
            select(func.coalesce(func.sum(CapabilityQuotaReservation.reserved_units), 0))
            .where(
                CapabilityQuotaReservation.capability == capability.value,
                CapabilityQuotaReservation.reserved_at >= since,
                CapabilityQuotaReservation.status.in_(
                    [
                        QuotaReservationStatus.ACTIVE.value,
                        QuotaReservationStatus.SETTLED.value,
                    ]
                ),
            )
        )
        if int(used or 0) + requested > limit:
            raise Forbidden(f"{capability.value} {label} quota exceeded")

    def attach_task(
        self,
        reservation: CapabilityQuotaReservation,
        task_id: UUID,
    ) -> None:
        reservation.task_record_id = task_id

    def settle(self, reservation: CapabilityQuotaReservation) -> None:
        if reservation.status == QuotaReservationStatus.ACTIVE.value:
            reservation.status = QuotaReservationStatus.SETTLED.value
            reservation.settled_at = self.clock.now()

    def release(self, reservation: CapabilityQuotaReservation) -> None:
        if reservation.status == QuotaReservationStatus.ACTIVE.value:
            reservation.status = QuotaReservationStatus.RELEASED.value
            reservation.released_at = self.clock.now()
~~~

Create the first narrow version of `backend/src/ip_saas/modules/governance/router.py` now, so a fresh production database never needs a hand-written entitlement row. Add strict `ProviderEntitlementWrite` and minimal `ProviderEntitlementResponse` schemas to `schemas.py`; the write schema contains exactly `provider`, `region`, `provider_account_fingerprint`, `model_id`, `model_version`, `release_snapshot_sha256`, the three positive quotas, `contract_attestation_id`, `evidence_asset_id`, and a timezone-aware future `expires_at`. The response exposes only entitlement ID, capability, status, model ID/version, checked time, and expiry—not provider-account fingerprints or evidence contents.

Expose two authenticated platform-admin routes: `POST /v1/platform/governance/capabilities/{capability}/entitlements/provision-initial` returns 201 and calls only `ProviderEntitlementRotationService.provision_initial`; `POST /v1/platform/governance/capabilities/{capability}/entitlements/rotate` returns 201 and calls only `rotate`. Both receive `Annotated[Session, Depends(get_session)]` and `Annotated[ActorContext, Depends(get_actor)]`, validate `BetaCapability`, and use one injected service built from a shared `SystemClock` plus `AuditWriter`. Include this router in `create_app()` in Task 4; Task 11 extends the same router and never creates a second entitlement write path. Neither route accepts a status, stage, checked time, actor ID, account ID, gate result, or arbitrary metadata, and neither writes `ProviderCapabilityEntitlement` directly.

Implement the three new `governance_rig` cases with real PostgreSQL rows and the real service/router. The expired case keeps the latest row `ACTIVE` but sets `expires_at < clock.now()`; rotation must lock it without applying the eligibility expiry filter, suspend it, and insert one future replacement. The zero-history case starts with the migration-seeded OFF feature flag and no entitlement rows. The route case overrides only session/actor/service dependencies, uses a real PLATFORM_ADMIN actor, and counts ORM inserts outside the service as forbidden.

`lock_capability_governance()` is the one transaction-level lock namespace for `CapabilityFeatureFlagService.set_stage()`, `ProviderEntitlementRotationService.provision_initial()`, and `rotate()`. Never add a second numeric or string lock key for any of the three paths. After acquiring it, `set_stage()` locks the flag row, and `GateReader` re-selects the single ACTIVE, unexpired entitlement with `FOR UPDATE` plus `populate_existing`; PUBLIC_BETA then rechecks status/expiry and the exact six-field `ReleaseDecision` binding before consuming the decision. Initial provision locks the OFF flag plus the complete zero-row entitlement history. Rotation locks the flag and all historical entitlement rows without filtering out an expired latest snapshot, refuses while the capability is already PUBLIC_BETA, suspends an ACTIVE latest row, inserts a new immutable future provider/account/region/model/version/release-snapshot binding, and copies no PASS gates. The expanded `uq_provider_capability_account` permits historical snapshots but rejects a duplicate exact snapshot; ordinary eligibility still uses `require_current_entitlement` and rejects zero, expired, or multiple active rows.

Implement `governance_rig.entitlement_public_beta_race()` in `backend/tests/support/governance_rig.py` with a real PostgreSQL engine and `sessionmaker(bind=engine, expire_on_commit=False)`. It seeds CLOSED_BETA, one current entitlement with a full valid gate set, and an unconsumed decision bound to that entitlement. `start_first()` and `start_second()` create two independent SQLAlchemy `Session` objects in two executor threads; never share a Session across threads. The first thread explicitly acquires `lock_capability_governance()` in its transaction, signals a `threading.Event`, then invokes the real promotion wrapper or `ProviderEntitlementRotationService.rotate()` and pauses before commit. In the rotation-first branch only, the harness inserts a freshly executed gate set bound to the replacement entitlement in that same first transaction; it never copies old gate rows, and this deliberate setup ensures the waiting promotion reaches the exact old-decision binding check after the lock releases. The second invokes the other real service and must block on the same PostgreSQL advisory transaction lock; before entering the service it records `SELECT pg_backend_pid()`, and `second_transaction_is_blocked_in_postgres()` requires its future to remain incomplete while a third observer connection finds `locktype='advisory' AND granted=false AND pid=<second_pid>` in `pg_locks`, not merely a sleep or mock flag. `commit_first()` releases the first transaction, then `await_second()` commits success or rolls back and returns the exact `Conflict`. If promotion wins, later rotation sees PUBLIC_BETA and must roll back; if rotation wins, promotion re-reads the replacement entitlement and rejects the old decision. The harness reports two distinct backend/session IDs and instruments entry after lock acquisition so the test proves a maximum of one capability critical section. Always join both threads and close all three connections in `finally`.

Extend `seeded_governance.rotate_entitlement(...)` and `pass_gate_set_for(entitlement)` to delegate to that production rotation service so the non-race test also proves old gate rows never authorize a new entitlement.

- [ ] **Step 8: Run focused eligibility and quota tests**

Run:

~~~bash
cd backend && uv run pytest tests/unit/governance/test_feature_flags.py tests/integration/governance/test_eligibility.py -v
~~~

Expected: PASS. Each capability can be disabled without changing the other two, direct OFF-to-PUBLIC_BETA is illegal, expired or old-entitlement evidence blocks, and the two real PostgreSQL Sessions prove rotation and promotion cannot interleave across the capability lock. A promotion winner makes rotation fail until demotion; a rotation winner makes the old decision fail without consumption. Minors are rejected, reseller support does not cross the identity boundary, and quotas are durable in PostgreSQL. Two accounts may reuse one client key; an exact same-account replay is side-effect free; changed input or another project conflicts; an unauthorized actor fails before the account-scoped existing-row query can return anything.

- [ ] **Step 9: Commit capability and quota gates**

~~~bash
git add backend/src/ip_saas/modules/governance/feature_flags.py backend/src/ip_saas/modules/governance/eligibility.py backend/src/ip_saas/modules/governance/router.py backend/src/ip_saas/modules/governance/schemas.py backend/src/ip_saas/api.py backend/tests/support/governance_rig.py backend/tests/unit/governance/test_feature_flags.py backend/tests/integration/governance/test_eligibility.py
git commit -m "feat: gate beta capabilities and provider quotas"
~~~

### Task 5: Implement separate consent, minor default-deny, withdrawal, and deletion

**Files:**
- Create: backend/src/ip_saas/modules/governance/consent.py
- Create: backend/src/ip_saas/modules/governance/events.py
- Create: backend/src/ip_saas/modules/governance/revocation.py
- Test: backend/tests/unit/governance/test_consent_policy.py
- Test: backend/tests/integration/governance/test_consent_withdrawal.py

- [ ] **Step 1: Write failing consent-policy tests**

Create backend/tests/unit/governance/test_consent_policy.py:

~~~python
from datetime import UTC, datetime, timedelta
from types import SimpleNamespace
from uuid import uuid4

import pytest

from ip_saas.common.errors import Forbidden
from ip_saas.modules.governance.consent import ConsentPolicy
from ip_saas.modules.governance.enums import ConsentKind, IdentityKind


NOW = datetime(2026, 8, 24, tzinfo=UTC)


def active(kind: ConsentKind):
    return SimpleNamespace(
        id=uuid4(),
        kind=kind.value,
        status="active",
        valid_from=NOW - timedelta(days=1),
        valid_until=NOW + timedelta(days=30),
        usage_count=0,
        usage_limit=10,
        commercial_use=True,
        modeling_allowed=True,
        provider_transfer_allowed=True,
        platforms=["douyin"],
        territory="CN",
        terms_snapshot_sha256="a" * 64,
    )


def test_voice_clone_requires_voice_and_sensitive_consents() -> None:
    with pytest.raises(Forbidden, match="sensitive_personal_info"):
        ConsentPolicy.require_identity_consents(
            IdentityKind.VOICE_CLONE,
            [active(ConsentKind.VOICE)],
            NOW,
        )
    rows = ConsentPolicy.require_identity_consents(
        IdentityKind.VOICE_CLONE,
        [
            active(ConsentKind.VOICE),
            active(ConsentKind.SENSITIVE_PERSONAL_INFO),
        ],
        NOW,
    )
    assert {row.kind for row in rows} == {"voice", "sensitive_personal_info"}


def test_portrait_and_voice_cannot_substitute_for_each_other() -> None:
    with pytest.raises(Forbidden, match="portrait"):
        ConsentPolicy.require_identity_consents(
            IdentityKind.SEEDANCE_PORTRAIT,
            [
                active(ConsentKind.VOICE),
                active(ConsentKind.SENSITIVE_PERSONAL_INFO),
            ],
            NOW,
        )


def test_traditional_avatar_requires_three_separate_consents() -> None:
    rows = ConsentPolicy.require_identity_consents(
        IdentityKind.TRADITIONAL_AVATAR,
        [
            active(ConsentKind.PORTRAIT),
            active(ConsentKind.VOICE),
            active(ConsentKind.SENSITIVE_PERSONAL_INFO),
        ],
        NOW,
    )
    assert len(rows) == 3


def test_generation_requires_exact_platform_and_cn_territory() -> None:
    records = [
        active(ConsentKind.VOICE),
        active(ConsentKind.SENSITIVE_PERSONAL_INFO),
    ]
    with pytest.raises(Forbidden, match="platform or territory"):
        ConsentPolicy.require_identity_consents(
            IdentityKind.VOICE_CLONE,
            records,
            NOW,
            platform="xiaohongshu",
            territory="CN",
        )


def test_duplicate_active_consent_kind_fails_closed() -> None:
    with pytest.raises(Forbidden, match="multiple active"):
        ConsentPolicy.require_identity_consents(
            IdentityKind.VOICE_CLONE,
            [
                active(ConsentKind.VOICE),
                active(ConsentKind.VOICE),
                active(ConsentKind.SENSITIVE_PERSONAL_INFO),
            ],
            NOW,
        )
~~~

- [ ] **Step 2: Run policy tests and verify the missing-module failure**

Run:

~~~bash
cd backend && uv run pytest tests/unit/governance/test_consent_policy.py -v
~~~

Expected: FAIL during collection because governance.consent does not exist.

- [ ] **Step 3: Implement exact consent requirements and terms hashing**

Create backend/src/ip_saas/modules/governance/consent.py:

~~~python
from collections.abc import Iterable, Mapping
from hashlib import sha256
from json import dumps
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.common.clock import Clock
from ip_saas.common.errors import Forbidden, NotFound
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.media.models.jobs import MediaAsset
from ip_saas.modules.projects.access import ProjectAccessService

from .enums import ConsentKind, ConsentStatus, IdentityKind
from .models import ConsentRecord, IdentitySubject
from .schemas import ConsentCreate, IdentitySubjectCreate


REQUIRED_CONSENTS: dict[IdentityKind, frozenset[ConsentKind]] = {
    IdentityKind.VOICE_CLONE: frozenset(
        {ConsentKind.VOICE, ConsentKind.SENSITIVE_PERSONAL_INFO}
    ),
    IdentityKind.SEEDANCE_PORTRAIT: frozenset(
        {ConsentKind.PORTRAIT, ConsentKind.SENSITIVE_PERSONAL_INFO}
    ),
    IdentityKind.TRADITIONAL_AVATAR: frozenset(
        {
            ConsentKind.PORTRAIT,
            ConsentKind.VOICE,
            ConsentKind.SENSITIVE_PERSONAL_INFO,
        }
    ),
}


class ConsentTermsRegistry:
    def __init__(self, terms: Mapping[str, str]) -> None:
        self.terms = dict(terms)

    def snapshot_hash(self, version: str) -> str:
        try:
            body = self.terms[version]
        except KeyError as error:
            raise Forbidden("consent terms version is not registered") from error
        return sha256(body.encode("utf-8")).hexdigest()


class ConsentPolicy:
    @staticmethod
    def require_identity_consents(
        identity_kind: IdentityKind,
        records: Iterable[ConsentRecord],
        now,
        *,
        platform: str | None = None,
        territory: str = "CN",
    ) -> list[ConsentRecord]:
        grouped: dict[ConsentKind, list[ConsentRecord]] = {}
        for row in records:
            grouped.setdefault(ConsentKind(row.kind), []).append(row)
        required = REQUIRED_CONSENTS[identity_kind]
        missing = sorted(kind.value for kind in required - set(grouped))
        if missing:
            raise Forbidden(f"required identity consents are missing: {missing}")
        ambiguous = sorted(
            kind.value for kind in required if len(grouped[kind]) != 1
        )
        if ambiguous:
            raise Forbidden(f"multiple active identity consents exist: {ambiguous}")
        selected: list[ConsentRecord] = []
        for kind in sorted(required, key=lambda item: item.value):
            row = grouped[kind][0]
            if (
                row.status != ConsentStatus.ACTIVE.value
                or not (row.valid_from <= now < row.valid_until)
                or row.usage_count >= row.usage_limit
                or not row.commercial_use
                or not row.modeling_allowed
                or not row.provider_transfer_allowed
            ):
                raise Forbidden(f"consent is not usable: {kind.value}")
            if row.territory != territory or (
                platform is not None and platform not in row.platforms
            ):
                raise Forbidden(
                    f"consent platform or territory is not usable: {kind.value}"
                )
            selected.append(row)
        return selected


class ConsentService:
    def __init__(
        self,
        access: ProjectAccessService,
        terms: ConsentTermsRegistry,
        audit: AuditWriter,
        clock: Clock,
    ) -> None:
        self.access = access
        self.terms = terms
        self.audit = audit
        self.clock = clock

    def create_subject(
        self,
        session: Session,
        actor: ActorContext,
        project_id: UUID,
        command: IdentitySubjectCreate,
    ) -> IdentitySubject:
        project = self.access.require_editor(session, actor, project_id)
        if actor.kind not in {
            ActorKind.C_USER,
            ActorKind.PLATFORM_OPERATOR,
            ActorKind.PLATFORM_ADMIN,
        }:
            raise Forbidden("project owner identity is required")
        evidence = session.get(MediaAsset, command.age_verification_evidence_asset_id)
        if (
            evidence is None
            or evidence.project_id != project.id
            or evidence.quarantined_at is not None
            or evidence.deleted_at is not None
        ):
            raise Forbidden("valid age-verification evidence is required")
        if command.age_class == "minor":
            guardian = session.scalar(
                select(IdentitySubject).where(
                    IdentitySubject.id == command.guardian_subject_id,
                    IdentitySubject.project_id == project.id,
                    IdentitySubject.age_class == "adult",
                    IdentitySubject.disabled_at.is_(None),
                )
            )
            if guardian is None:
                raise Forbidden("a verified adult guardian is required")
        row = IdentitySubject(
            account_id=project.owner_account_id,
            project_id=project.id,
            pseudonym=command.pseudonym,
            age_class=command.age_class,
            guardian_subject_id=command.guardian_subject_id,
            age_verification_method=command.age_verification_method,
            age_verification_evidence_asset_id=evidence.id,
            verified_by=actor.actor_id,
            verified_at=self.clock.now(),
        )
        session.add(row)
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="identity.subject.verified",
            target_type="identity_subject",
            target_id=row.id,
            project_id=project.id,
            metadata={"age_class": row.age_class},
        )
        return row

    def grant(
        self,
        session: Session,
        actor: ActorContext,
        project_id: UUID,
        command: ConsentCreate,
    ) -> ConsentRecord:
        project = self.access.require_editor(session, actor, project_id)
        subject = session.scalar(
            select(IdentitySubject).where(
                IdentitySubject.id == command.subject_id,
                IdentitySubject.project_id == project.id,
                IdentitySubject.account_id == project.owner_account_id,
            )
        )
        if subject is None or subject.disabled_at is not None:
            raise NotFound("active identity subject")
        active_same_kind = session.scalar(
            select(ConsentRecord)
            .where(
                ConsentRecord.subject_id == subject.id,
                ConsentRecord.kind == command.kind.value,
                ConsentRecord.status == ConsentStatus.ACTIVE.value,
            )
            .with_for_update()
        )
        if active_same_kind is not None:
            raise Forbidden("withdraw the existing active consent before replacement")
        evidence = session.get(MediaAsset, command.verification_evidence_asset_id)
        if (
            evidence is None
            or evidence.project_id != project.id
            or evidence.quarantined_at is not None
            or evidence.deleted_at is not None
        ):
            raise Forbidden("valid direct-verification evidence is required")
        if subject.age_class == "minor":
            if command.kind is ConsentKind.MINOR_MODELING:
                raise Forbidden("minor identity modeling is disabled in V1 public beta")
            if command.kind is not ConsentKind.MINOR_SOURCE_MATERIAL:
                raise Forbidden("a minor requires the separate source-material flow")
            if command.verification_method != "guardian_direct_liveness":
                raise Forbidden("guardian direct verification is required")
        elif command.kind in {
            ConsentKind.MINOR_SOURCE_MATERIAL,
            ConsentKind.MINOR_MODELING,
        }:
            raise Forbidden("minor consent kinds require a minor subject")
        now = self.clock.now()
        row = ConsentRecord(
            account_id=project.owner_account_id,
            project_id=project.id,
            subject_id=subject.id,
            kind=command.kind.value,
            terms_version=command.terms_version,
            terms_snapshot_sha256=self.terms.snapshot_hash(command.terms_version),
            purposes=sorted(set(command.purposes)),
            platforms=sorted(set(command.platforms)),
            territory=command.territory,
            valid_from=command.valid_from,
            valid_until=command.valid_until,
            usage_limit=command.usage_limit,
            usage_count=0,
            commercial_use=command.commercial_use,
            modeling_allowed=command.modeling_allowed,
            reuse_allowed=command.reuse_allowed,
            provider_transfer_allowed=command.provider_transfer_allowed,
            existing_output_policy=command.existing_output_policy,
            verification_method=command.verification_method,
            verification_evidence_asset_id=evidence.id,
            status=ConsentStatus.ACTIVE.value,
            granted_by=actor.actor_id,
            granted_at=now,
        )
        session.add(row)
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="identity.consent.granted",
            target_type="consent_record",
            target_id=row.id,
            project_id=project.id,
            metadata={
                "kind": row.kind,
                "terms_version": row.terms_version,
                "terms_snapshot_sha256": row.terms_snapshot_sha256,
            },
        )
        return row

    def lock_required(
        self,
        session: Session,
        project_id: UUID,
        subject_id: UUID,
        identity_kind: IdentityKind,
        *,
        platform: str | None = None,
        territory: str = "CN",
    ) -> list[ConsentRecord]:
        records = list(
            session.scalars(
                select(ConsentRecord)
                .where(
                    ConsentRecord.project_id == project_id,
                    ConsentRecord.subject_id == subject_id,
                    ConsentRecord.status == ConsentStatus.ACTIVE.value,
                    ConsentRecord.kind.in_(
                        [kind.value for kind in REQUIRED_CONSENTS[identity_kind]]
                    ),
                )
                .with_for_update()
            )
        )
        return ConsentPolicy.require_identity_consents(
            identity_kind,
            records,
            self.clock.now(),
            platform=platform,
            territory=territory,
        )

    @staticmethod
    def consume(records: list[ConsentRecord]) -> None:
        for row in records:
            row.usage_count += 1
~~~

The partial unique index `uq_active_consent_subject_kind` is the final concurrent-write guard. Map its PostgreSQL unique violation to `Conflict("an active consent of this kind already exists")`; never retry by inserting a second active row or overwrite the first authorization.

- [ ] **Step 4: Run the consent-policy tests**

Run:

~~~bash
cd backend && uv run pytest tests/unit/governance/test_consent_policy.py -v
~~~

Expected: PASS, 5 tests, including exact-platform scope and duplicate-active-consent denial.

- [ ] **Step 5: Write failing withdrawal and live-job tests**

Create backend/tests/integration/governance/test_consent_withdrawal.py:

~~~python
from sqlalchemy import select

from ip_saas.modules.governance.models import (
    GenerationIdentityUse,
    IdentityDeletionRequest,
    ProviderIdentityAsset,
)
from ip_saas.modules.media.models.jobs import GenerationJob, MediaAsset
from ip_saas.modules.tasks.models import TaskRecord


def test_withdrawal_freezes_identity_cancels_queued_job_and_releases_hold(
    db_session,
    revocation_service,
    customer_actor,
    queued_governed_generation,
) -> None:
    result = revocation_service.withdraw(
        db_session,
        customer_actor,
        queued_governed_generation.project_id,
        queued_governed_generation.consent_id,
        reason="subject withdrew authorization",
        request_provider_deletion=True,
    )
    asset = db_session.get(
        ProviderIdentityAsset, queued_governed_generation.identity_asset_id
    )
    task = db_session.get(TaskRecord, queued_governed_generation.task_id)
    job = db_session.get(GenerationJob, queued_governed_generation.job_id)
    assert result.status == "withdrawn"
    assert asset.status == "frozen"
    assert task.status == "cancelled"
    assert job.provider_status == "failed"
    assert queued_governed_generation.billing_spy.release_count == 1
    assert db_session.scalar(
        select(IdentityDeletionRequest).where(
            IdentityDeletionRequest.identity_asset_id == asset.id
        )
    ) is not None


def test_withdrawal_blocks_running_delivery_without_claiming_provider_cancel(
    db_session,
    revocation_service,
    customer_actor,
    running_governed_generation,
) -> None:
    revocation_service.withdraw(
        db_session,
        customer_actor,
        running_governed_generation.project_id,
        running_governed_generation.consent_id,
        reason="subject withdrew authorization",
        request_provider_deletion=True,
    )
    task = db_session.get(TaskRecord, running_governed_generation.task_id)
    identity_use = db_session.get(
        GenerationIdentityUse,
        (
            running_governed_generation.job_id,
            running_governed_generation.identity_asset_id,
        ),
    )
    assert task.status == "running"
    assert identity_use.delivery_blocked_at is not None
    assert running_governed_generation.billing_spy.release_count == 0


def test_withdrawal_quarantines_existing_outputs(
    db_session,
    revocation_service,
    customer_actor,
    completed_governed_generation,
) -> None:
    revocation_service.withdraw(
        db_session,
        customer_actor,
        completed_governed_generation.project_id,
        completed_governed_generation.consent_id,
        reason="subject withdrew authorization",
        request_provider_deletion=False,
    )
    media = db_session.get(MediaAsset, completed_governed_generation.media_asset_id)
    assert media.quarantined_at is not None
    assert media.quarantine_reason == "identity consent withdrawn"
~~~

- [ ] **Step 6: Run withdrawal tests and verify the missing-service failure**

Run:

~~~bash
cd backend && uv run pytest tests/integration/governance/test_consent_withdrawal.py -v
~~~

Expected: FAIL during collection because governance.revocation does not exist.

- [ ] **Step 7: Implement atomic withdrawal, delivery blocking, and deletion requests**

Create the initial producer helper in `backend/src/ip_saas/modules/governance/events.py` before importing it from revocation, enrollment, generation, complaint, or deletion services. Task 11 replaces this generic construction boundary with the seven strict typed envelopes and full-envelope consumer; this initial version exists so Tasks 5–10 remain independently runnable:

~~~python
from collections.abc import Mapping
from datetime import datetime
from uuid import UUID

from ip_saas.common.outbox import EventEnvelope


def event_envelope(
    *,
    event_type: str,
    event_id: UUID,
    aggregate_id: UUID,
    occurred_at: datetime,
    initiated_by_actor_id: UUID,
    idempotency_key: str,
    payload: Mapping[str, object],
) -> EventEnvelope:
    return EventEnvelope(
        event_id=event_id,
        event_type=event_type,
        schema_version=1,
        aggregate_id=aggregate_id,
        occurred_at=occurred_at,
        initiated_by_actor_id=initiated_by_actor_id,
        idempotency_key=idempotency_key,
        payload=dict(payload),
    )
~~~

Create backend/src/ip_saas/modules/governance/revocation.py:

~~~python
from uuid import UUID, uuid4

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.common.clock import Clock
from ip_saas.common.errors import Forbidden, NotFound
from ip_saas.common.outbox import OutboxWriter
from ip_saas.common.tasking import TaskStatus
from ip_saas.modules.accounts.context import ActorContext
from ip_saas.modules.billing.service import BillingContext, BillingMode, BillingService
from ip_saas.modules.media.enums import JobStatus
from ip_saas.modules.media.models.jobs import GenerationJob, MediaAsset
from ip_saas.modules.projects.access import ProjectAccessService
from ip_saas.modules.tasks.models import TaskRecord

from .enums import ConsentStatus, DeletionStatus, IdentityAssetStatus
from .events import event_envelope
from .models import (
    ConsentRecord,
    GenerationIdentityUse,
    IdentityAssetConsent,
    IdentityDeletionRequest,
    ProviderIdentityAsset,
)


class RevocationService:
    def __init__(
        self,
        access: ProjectAccessService,
        billing: BillingService,
        audit: AuditWriter,
        outbox: OutboxWriter,
        clock: Clock,
    ) -> None:
        self.access = access
        self.billing = billing
        self.audit = audit
        self.outbox = outbox
        self.clock = clock

    def withdraw(
        self,
        session: Session,
        actor: ActorContext,
        project_id: UUID,
        consent_id: UUID,
        *,
        reason: str,
        request_provider_deletion: bool,
    ) -> ConsentRecord:
        consent = session.scalar(
            select(ConsentRecord)
            .where(
                ConsentRecord.id == consent_id,
                ConsentRecord.project_id == project_id,
            )
            .with_for_update()
        )
        if consent is None:
            raise NotFound("consent")
        self.access.require_editor(session, actor, project_id)
        if consent.account_id != actor.account_id:
            raise Forbidden("only the project owner can withdraw identity consent")
        if consent.status == ConsentStatus.WITHDRAWN.value:
            return consent
        now = self.clock.now()
        consent.status = ConsentStatus.WITHDRAWN.value
        consent.withdrawn_by = actor.actor_id
        consent.withdrawn_at = now
        consent.withdrawal_reason = reason[:500]
        assets = list(
            session.scalars(
                select(ProviderIdentityAsset)
                .join(
                    IdentityAssetConsent,
                    IdentityAssetConsent.identity_asset_id
                    == ProviderIdentityAsset.id,
                )
                .where(IdentityAssetConsent.consent_id == consent.id)
                .with_for_update()
            )
        )
        for identity_asset in assets:
            self._freeze_asset_and_jobs(
                session,
                actor,
                identity_asset,
                now,
                request_provider_deletion=request_provider_deletion,
            )
        self.audit.write(
            session,
            actor=actor,
            action="identity.consent.withdrawn",
            target_type="consent_record",
            target_id=consent.id,
            project_id=consent.project_id,
            metadata={
                "kind": consent.kind,
                "identity_asset_count": len(assets),
                "provider_deletion_requested": request_provider_deletion,
            },
        )
        return consent

    def _freeze_asset_and_jobs(
        self,
        session: Session,
        actor: ActorContext,
        identity_asset: ProviderIdentityAsset,
        now,
        *,
        request_provider_deletion: bool,
    ) -> None:
        if identity_asset.status != IdentityAssetStatus.DELETED.value:
            identity_asset.status = IdentityAssetStatus.FROZEN.value
            identity_asset.frozen_at = now
            identity_asset.freeze_reason = "identity consent withdrawn"
        uses = list(
            session.scalars(
                select(GenerationIdentityUse)
                .where(
                    GenerationIdentityUse.identity_asset_id == identity_asset.id
                )
                .with_for_update()
            )
        )
        for use in uses:
            use.delivery_blocked_at = now
            use.delivery_block_reason = "identity consent withdrawn"
            job = session.scalar(
                select(GenerationJob)
                .where(GenerationJob.id == use.generation_job_id)
                .with_for_update()
            )
            if job is None:
                continue
            task = session.scalar(
                select(TaskRecord)
                .where(TaskRecord.id == job.task_record_id)
                .with_for_update()
            )
            if task is not None and task.status == TaskStatus.QUEUED.value:
                task.status = TaskStatus.CANCELLED.value
                task.error_code = "identity_consent_withdrawn"
                task.error_message = "Identity authorization was withdrawn before submission."
                job.provider_status = JobStatus.FAILED.value
                job.last_error_code = "identity_consent_withdrawn"
                job.last_error_message = "Identity authorization was withdrawn."
                self.billing.release_generation(
                    session,
                    BillingContext(
                        BillingMode(task.billing_mode),
                        task.billing_hold_id,
                    ),
                    "identity consent withdrawn before provider submission",
                    f"withdraw:{task.id}:release",
                )
            media_assets = list(
                session.scalars(
                    select(MediaAsset).where(MediaAsset.job_id == job.id).with_for_update()
                )
            )
            for media in media_assets:
                media.quarantined_at = now
                media.quarantine_reason = "identity consent withdrawn"
        if request_provider_deletion:
            deletion = session.scalar(
                select(IdentityDeletionRequest).where(
                    IdentityDeletionRequest.identity_asset_id == identity_asset.id,
                    IdentityDeletionRequest.status.in_(
                        [
                            DeletionStatus.REQUESTED.value,
                            DeletionStatus.PROVIDER_PENDING.value,
                            DeletionStatus.LIVE_DATA_DELETED.value,
                        ]
                    ),
                )
            )
            if deletion is None:
                deletion = IdentityDeletionRequest(
                    account_id=identity_asset.account_id,
                    project_id=identity_asset.project_id,
                    subject_id=identity_asset.subject_id,
                    identity_asset_id=identity_asset.id,
                    status=DeletionStatus.REQUESTED.value,
                    requested_by=actor.actor_id,
                    requested_at=now,
                )
                session.add(deletion)
                session.flush()
                self.outbox.add(
                    session,
                    event_envelope(
                        event_id=uuid4(),
                        event_type="identity.deletion.requested",
                        aggregate_id=deletion.id,
                        occurred_at=now,
                        initiated_by_actor_id=actor.actor_id,
                        idempotency_key=f"identity-delete:{identity_asset.id}",
                        payload={
                            "deletion_request_id": deletion.id,
                            "identity_asset_id": identity_asset.id,
                        },
                    ),
                )
~~~

- [ ] **Step 8: Run withdrawal integration tests**

Run:

~~~bash
cd backend && uv run pytest tests/integration/governance/test_consent_withdrawal.py -v
~~~

Expected: PASS, 3 tests. Queued work releases its hold once; running provider work is delivery-blocked but not falsely reported cancelled or free; completed media is quarantined.

- [ ] **Step 9: Add minor privacy and reseller-denial security cases**

Append to backend/tests/security/test_identity_boundaries.py:

~~~python
import pytest

from ip_saas.common.errors import Forbidden, NotFound


def test_reseller_cannot_read_consent_or_training_source(
    db_session,
    identity_repository,
    l1_actor_with_media_support,
    customer_identity,
) -> None:
    with pytest.raises((Forbidden, NotFound)):
        identity_repository.require_consent(
            db_session,
            l1_actor_with_media_support,
            customer_identity.consent_id,
        )
    with pytest.raises((Forbidden, NotFound)):
        identity_repository.sign_training_source(
            db_session,
            l1_actor_with_media_support,
            customer_identity.source_asset_id,
        )


def test_child_location_school_and_medical_metadata_are_rejected(
    minor_source_material_validator,
) -> None:
    with pytest.raises(Forbidden, match="minor sensitive location"):
        minor_source_material_validator.validate(
            {
                "school": "某某小学",
                "home_address": "北京市某街道",
                "medical_record": "diagnosis",
            }
        )
~~~

Implement the repository queries with account_id plus project_id filters and a MinorSourceMaterialValidator that rejects school, home_address, precise_location, vehicle_plate, routine_schedule, and medical_record keys before storage.

Run:

~~~bash
cd backend && uv run pytest tests/security/test_identity_boundaries.py -v
~~~

Expected: PASS; a Plan 05 support grant never opens identity records, and minor-sensitive fields never reach PostgreSQL or TOS.

- [ ] **Step 10: Commit consent and revocation controls**

~~~bash
git add backend/src/ip_saas/modules/governance/consent.py backend/src/ip_saas/modules/governance/events.py backend/src/ip_saas/modules/governance/revocation.py backend/tests/unit/governance/test_consent_policy.py backend/tests/integration/governance/test_consent_withdrawal.py backend/tests/security/test_identity_boundaries.py
git commit -m "feat: enforce identity consent withdrawal and deletion"
~~~

### Task 6: Prove three independent production-provider boundaries

**Files:**
- Modify: backend/migrations/versions/0006_governance.py
- Modify: backend/src/ip_saas/modules/governance/enums.py
- Modify: backend/src/ip_saas/modules/governance/models.py
- Create: backend/src/ip_saas/modules/governance/provider_crash.py
- Create: backend/src/ip_saas/providers/digital_identity/base.py
- Create: backend/src/ip_saas/providers/digital_identity/composition.py
- Create: backend/src/ip_saas/providers/digital_identity/fake.py
- Create: backend/src/ip_saas/providers/digital_identity/__init__.py
- Create: backend/src/ip_saas/providers/volcengine/voice_clone.py
- Create: backend/src/ip_saas/providers/volcengine/seedance_portrait.py
- Create: backend/src/ip_saas/providers/volcengine/cloned_avatar.py
- Create: backend/src/ip_saas/scripts/provider_poc.py
- Modify: Makefile
- Create: infra/runbooks/provider-poc.md
- Test: backend/tests/contract/governance/test_provider_adapters.py
- Test: backend/tests/integration/governance/test_migration.py
- Test: backend/tests/integration/governance/test_provider_poc_evaluator.py
- Test: backend/tests/integration/governance/test_provider_crash_reconciliation.py
- Test: backend/tests/integration/governance/test_provider_release_composition.py
- Modify: backend/tests/support/governance_rig.py

- [ ] **Step 1: Write failing provider-contract tests that cannot mix the three capabilities**

Create backend/tests/contract/governance/test_provider_adapters.py:

~~~python
from datetime import UTC, datetime

import httpx
import pytest

from ip_saas.modules.billing.models import ReconciliationStatus
from ip_saas.modules.governance.enums import BetaCapability, IdentityKind
from ip_saas.providers.digital_identity.base import (
    EnrollmentRequest,
    ProviderIdentityState,
    normalize_provider_request_id,
)
from ip_saas.providers.volcengine.seedance_portrait import SeedancePortraitAdapter
from ip_saas.providers.volcengine.voice_clone import VoiceCloneAdapter


def test_voice_clone_upload_has_watermark_resource_and_no_portrait_fields() -> None:
    captured: dict[str, object] = {}

    def handler(request: httpx.Request) -> httpx.Response:
        captured["path"] = request.url.path
        captured["resource"] = request.headers["Resource-Id"]
        captured["body"] = request.read()
        return httpx.Response(
            200,
            headers={"X-Tt-Logid": "voice-request-1"},
            json={"BaseResp": {"StatusCode": 0}, "Data": {}},
        )

    adapter = VoiceCloneAdapter(
        httpx.Client(transport=httpx.MockTransport(handler)),
        app_id="app",
        access_token="secret",
        resource_id="volc.megatts.voiceclone",
        usage_meter=object(),
    )
    result = adapter.submit_enrollment(
        EnrollmentRequest(
            capability=BetaCapability.VOICE_CLONE,
            identity_kind=IdentityKind.VOICE_CLONE,
            local_enrollment_id="enr-1",
            source_urls=("https://private.example/one-hour-source.wav",),
            provider_verified_asset_id=None,
            idempotency_key="voice-1",
        )
    )
    assert captured == {
        "path": "/api/v1/mega_tts/audio/upload",
        "resource": "volc.megatts.voiceclone",
        "body": captured["body"],
    }
    assert b"portrait" not in bytes(captured["body"])
    assert result.state is ProviderIdentityState.ENROLLING


def test_seedance_portrait_requires_provider_verification_receipt() -> None:
    adapter = SeedancePortraitAdapter(
        httpx.Client(), api_key="secret", receipt_reader=object()
    )
    request = EnrollmentRequest(
        capability=BetaCapability.SEEDANCE_AUTHORIZED_PORTRAIT,
        identity_kind=IdentityKind.SEEDANCE_PORTRAIT,
        local_enrollment_id="enr-2",
        source_urls=(),
        provider_verified_asset_id=None,
        idempotency_key="portrait-1",
    )
    with pytest.raises(ValueError, match="provider-authenticated real-person asset"):
        adapter.submit_enrollment(request)


def test_provider_result_never_contains_a_delivery_url() -> None:
    annotations = ProviderIdentityState.__members__
    assert "ACTIVE" in annotations
    from ip_saas.providers.digital_identity.base import EnrollmentResult

    assert "delivery_url" not in EnrollmentResult.__annotations__
    assert "provider_resource_ref" in EnrollmentResult.__annotations__


def test_provider_request_id_is_trimmed_to_plan01_ledger_limit() -> None:
    assert normalize_provider_request_id("  supplier-request-1  ") == (
        "supplier-request-1"
    )


@pytest.mark.parametrize(
    "raw", [None, 7, "", "   ", "nOnE", "MISSING", "x" * 201]
)
def test_invalid_provider_request_id_is_rejected(raw: object) -> None:
    with pytest.raises(ValueError, match="provider request id"):
        normalize_provider_request_id(raw)


@pytest.mark.parametrize("capability", list(BetaCapability))
def test_production_terminal_result_has_reconciled_usage(
    production_adapter_harness, capability,
) -> None:
    result = production_adapter_harness.terminal_result(capability)
    assert result.usage is not None
    assert result.usage.amount_fen >= 0
    assert result.usage.reconciliation_status in {
        ReconciliationStatus.UNRECONCILED,
        ReconciliationStatus.MATCHED,
    }
~~~

- [ ] **Step 2: Run the contract tests and confirm the provider package is missing**

Run:

~~~bash
cd backend && env -u ARK_API_KEY -u DOUBAO_SPEECH_ACCESS_TOKEN uv run pytest tests/contract/governance/test_provider_adapters.py -v
~~~

Expected: FAIL during collection because ip_saas.providers.digital_identity does not exist. The test process must not read production credentials.

- [ ] **Step 3: Define one provider protocol with capability-specific requests**

Create backend/src/ip_saas/providers/digital_identity/base.py:

~~~python
from dataclasses import dataclass
from decimal import Decimal
from enum import StrEnum
from typing import Mapping, Protocol

from ip_saas.modules.billing.models import ReconciliationStatus
from ip_saas.modules.governance.enums import BetaCapability, IdentityKind


JSONScalar = str | int | float | bool | None


def normalize_provider_request_id(value: object) -> str:
    if not isinstance(value, str):
        raise ValueError("provider request id must be a string")
    normalized = value.strip()
    if (
        not 1 <= len(normalized) <= 200
        or normalized.casefold() in {"none", "missing"}
    ):
        raise ValueError("provider request id must be 1-200 trimmed characters")
    return normalized


class ProviderIdentityState(StrEnum):
    ENROLLING = "enrolling"
    ACTIVE = "active"
    FAILED = "failed"
    DELETED = "deleted"


@dataclass(frozen=True)
class ProviderUsage:
    model_id: str
    model_version: str
    native_quantity: Decimal
    native_unit: str
    supplier_amount_minor: int
    supplier_currency: str
    amount_fen: int
    reconciliation_status: ReconciliationStatus


class ProviderUsageMeter(Protocol):
    def for_response(
        self,
        *,
        capability: BetaCapability,
        provider_request_id: str,
        response_payload: Mapping[str, object],
    ) -> ProviderUsage: ...


@dataclass(frozen=True)
class VerifiedProviderReceipt:
    provider_request_id: str
    provider_resource_ref: str
    usage: ProviderUsage

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "provider_request_id",
            normalize_provider_request_id(self.provider_request_id),
        )


class ProviderVerificationReceiptReader(Protocol):
    def require(self, local_receipt_asset_id: str) -> VerifiedProviderReceipt: ...


@dataclass(frozen=True)
class EnrollmentRequest:
    capability: BetaCapability
    identity_kind: IdentityKind
    local_enrollment_id: str
    source_urls: tuple[str, ...]
    provider_verified_asset_id: str | None
    idempotency_key: str


@dataclass(frozen=True)
class EnrollmentResult:
    state: ProviderIdentityState
    provider_task_id: str
    provider_resource_ref: str | None
    provider_request_id: str
    retry_after_seconds: int | None
    sanitized_error_code: str | None = None
    usage: ProviderUsage | None = None

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "provider_request_id",
            normalize_provider_request_id(self.provider_request_id),
        )


@dataclass(frozen=True)
class IdentityGenerationRequest:
    capability: BetaCapability
    provider_resource_ref: str
    prompt: str
    parameters: Mapping[str, JSONScalar]
    idempotency_key: str


@dataclass(frozen=True)
class IdentityGenerationResult:
    provider_task_id: str
    state: str
    temporary_result_url: str | None
    provider_request_id: str
    retry_after_seconds: int | None
    usage: ProviderUsage | None

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "provider_request_id",
            normalize_provider_request_id(self.provider_request_id),
        )


class DigitalIdentityProvider(Protocol):
    capability: BetaCapability

    def submit_enrollment(self, request: EnrollmentRequest) -> EnrollmentResult: ...
    def poll_enrollment(self, provider_task_id: str) -> EnrollmentResult: ...
    def generate(self, request: IdentityGenerationRequest) -> IdentityGenerationResult: ...
    def poll_generation(self, provider_task_id: str) -> IdentityGenerationResult: ...
    def delete_identity(self, provider_resource_ref: str, idempotency_key: str) -> str: ...
~~~

The provider_resource_ref exists only in worker memory. The worker encrypts it before persistence and stores only its SHA-256 fingerprint beside the ciphertext.

- [ ] **Step 4: Add a deterministic fake and a capability-keyed registry**

Create backend/src/ip_saas/providers/digital_identity/fake.py and backend/src/ip_saas/providers/digital_identity/__init__.py:

~~~python
from hashlib import sha256

from ip_saas.modules.governance.enums import BetaCapability

from .base import (
    DigitalIdentityProvider,
    EnrollmentRequest,
    EnrollmentResult,
    IdentityGenerationRequest,
    IdentityGenerationResult,
    ProviderIdentityState,
)


class FakeDigitalIdentityProvider:
    def __init__(self, capability: BetaCapability) -> None:
        self.capability = capability
        self.deleted: set[str] = set()

    def submit_enrollment(self, request: EnrollmentRequest) -> EnrollmentResult:
        if request.capability is not self.capability:
            raise ValueError("capability/provider mismatch")
        suffix = sha256(request.idempotency_key.encode()).hexdigest()[:20]
        return EnrollmentResult(
            ProviderIdentityState.ACTIVE,
            f"fake-task-{suffix}",
            f"fake-ref-{suffix}",
            f"fake-request-{suffix}",
            None,
        )

    def poll_enrollment(self, provider_task_id: str) -> EnrollmentResult:
        suffix = provider_task_id.removeprefix("fake-task-")
        return EnrollmentResult(
            ProviderIdentityState.ACTIVE,
            provider_task_id,
            f"fake-ref-{suffix}",
            f"fake-poll-{suffix}",
            None,
        )

    def generate(self, request: IdentityGenerationRequest) -> IdentityGenerationResult:
        suffix = sha256(request.idempotency_key.encode()).hexdigest()[:20]
        return IdentityGenerationResult(
            f"fake-generation-{suffix}",
            "succeeded",
            f"memory://provider-result/{suffix}",
            f"fake-request-{suffix}",
            None,
            None,
        )

    def poll_generation(self, provider_task_id: str) -> IdentityGenerationResult:
        return IdentityGenerationResult(
            provider_task_id, "succeeded", f"memory://provider-result/{provider_task_id}",
            f"fake-poll-{provider_task_id}", None, None
        )

    def delete_identity(self, provider_resource_ref: str, idempotency_key: str) -> str:
        self.deleted.add(provider_resource_ref)
        return "fake-delete-" + sha256(idempotency_key.encode()).hexdigest()[:20]


class DigitalIdentityProviderRegistry:
    def __init__(self, providers: tuple[DigitalIdentityProvider, ...]) -> None:
        self._providers = {provider.capability: provider for provider in providers}
        if len(self._providers) != len(providers):
            raise ValueError("one provider is allowed per capability")

    def require(self, capability: BetaCapability) -> DigitalIdentityProvider:
        try:
            return self._providers[capability]
        except KeyError as exc:
            raise RuntimeError(f"provider unavailable for {capability.value}") from exc
~~~

Export the provider protocol, `ProviderUsageMeter`, `ProviderVerificationReceiptReader`, registry, `normalize_provider_request_id`, and all value classes, including `ProviderUsage` and `VerifiedProviderReceipt`, from backend/src/ip_saas/providers/digital_identity/__init__.py. Deterministic fakes set usage to `None`; production adapters must populate terminal enrollment/generation usage from the provider response or verified-receipt evidence plus the frozen contracted CNY conversion and reconciliation state. A production adapter may not return terminal success with `usage=None`. The normalization helper is the single boundary for supplier request identities: it accepts `object`, rejects non-strings before `.strip()`, trims once, requires 1–200 characters to match Plan 01's ledger column, and rejects blanks plus the sentinel strings `None` and `missing` before any result, evidence, attempt-journal, or billing write. Every production terminal-result and verified-receipt constructor invokes this helper in `__post_init__`; adapters never bypass it, hash a client key, invent a UUID, use a fixed literal, or substitute a provider task ID.

- [ ] **Step 5: Implement the voice-clone adapter against the production speech endpoint**

Create backend/src/ip_saas/providers/volcengine/voice_clone.py. Use the Plan 03 synchronous httpx policy and this request core:

~~~python
from ip_saas.providers.digital_identity.base import normalize_provider_request_id


class VoiceCloneAdapter:
    capability = BetaCapability.VOICE_CLONE

    def __init__(self, client: httpx.Client, app_id: str, access_token: str,
                 usage_meter: ProviderUsageMeter,
                 resource_id: str = "volc.megatts.voiceclone") -> None:
        self.client = client
        self.app_id = app_id
        self.access_token = access_token
        self.resource_id = resource_id
        self.usage_meter = usage_meter
        self.endpoint = "https://openspeech.bytedance.com/api/v1/mega_tts"

    def submit_enrollment(self, request: EnrollmentRequest) -> EnrollmentResult:
        if request.capability is not self.capability or len(request.source_urls) != 1:
            raise ValueError("voice cloning requires exactly one governed voice source")
        response = self.client.post(
            f"{self.endpoint}/audio/upload",
            headers={
                "Authorization": f"Bearer;{self.access_token}",
                "Resource-Id": self.resource_id,
                "X-Client-Request-Id": request.idempotency_key,
            },
            json={
                "appid": self.app_id,
                "speaker_id": request.local_enrollment_id,
                "audios": [{"audio_url": request.source_urls[0]}],
                "source": 2,
                "language": 0,
            },
            timeout=120,
        )
        response.raise_for_status()
        body = response.json()
        provider_request_id = normalize_provider_request_id(
            response.headers.get("X-Tt-Logid", "")
        )
        if int(body["BaseResp"]["StatusCode"]) != 0:
            return EnrollmentResult(
                ProviderIdentityState.FAILED, request.local_enrollment_id, None,
                provider_request_id, None,
                str(body["BaseResp"]["StatusCode"]),
                self.usage_meter.for_response(
                    capability=self.capability,
                    provider_request_id=provider_request_id,
                    response_payload=body,
                ),
            )
        return EnrollmentResult(
            ProviderIdentityState.ENROLLING,
            request.local_enrollment_id,
            None,
            provider_request_id,
            10,
        )
~~~

Implement poll_enrollment with POST `/api/v1/mega_tts/status`, generate with the provider's speech synthesis voice type, and delete_identity with the production-account deletion operation discovered by the live entitlement probe. Inject the same `ProviderUsageMeter` into all three production adapters. It maps terminal response request ID, entitlement/model version, provider native units, contracted supplier currency, immutable FX/price version, and reconciliation state into `ProviderUsage`; even a provider-confirmed zero charge returns an explicit zero usage. `usage=None` is allowed only before any request was emitted or for a still-nonterminal poll. If the entitlement probe returns no supported deletion operation, return a hard failure and keep the local asset frozen; never claim provider deletion succeeded. Tests must mock the exact endpoint and response code and must not contain a real token.

- [ ] **Step 6: Implement Seedance verified-portrait and traditional-avatar adapters independently**

Create backend/src/ip_saas/providers/volcengine/seedance_portrait.py with this mandatory boundary:

~~~python
from ip_saas.providers.digital_identity.base import (
    ProviderVerificationReceiptReader,
    normalize_provider_request_id,
)


class SeedancePortraitAdapter:
    capability = BetaCapability.SEEDANCE_AUTHORIZED_PORTRAIT

    def __init__(self, client: httpx.Client, api_key: str,
                 receipt_reader: ProviderVerificationReceiptReader) -> None:
        self.client = client
        self.api_key = api_key
        self.receipt_reader = receipt_reader
        self.endpoint = "https://ark.cn-beijing.volces.com/api/v3"

    def submit_enrollment(self, request: EnrollmentRequest) -> EnrollmentResult:
        if request.capability is not self.capability:
            raise ValueError("capability/provider mismatch")
        if request.source_urls or not request.provider_verified_asset_id:
            raise ValueError("Seedance portrait requires provider-authenticated real-person asset")
        receipt = self.receipt_reader.require(request.provider_verified_asset_id)
        return EnrollmentResult(
            ProviderIdentityState.ACTIVE,
            request.local_enrollment_id,
            receipt.provider_resource_ref,
            normalize_provider_request_id(receipt.provider_request_id),
            None,
            usage=receipt.usage,
        )

    def generate(self, request: IdentityGenerationRequest) -> IdentityGenerationResult:
        content = [
            {"type": "text", "text": request.prompt},
            {"type": "image_url", "image_url": {"url": f"asset://{request.provider_resource_ref}"}},
        ]
        response = self.client.post(
            f"{self.endpoint}/contents/generations/tasks",
            headers={"Authorization": f"Bearer {self.api_key}",
                     "X-Client-Request-Id": request.idempotency_key},
            json={"model": request.parameters["model_id"], "content": content,
                  "watermark": True},
            timeout=120,
        )
        response.raise_for_status()
        body = response.json()
        provider_request_id = normalize_provider_request_id(
            response.headers.get("x-request-id", "")
        )
        return IdentityGenerationResult(
            str(body["id"]), str(body.get("status", "queued")), None,
            provider_request_id, 5, None,
        )
~~~

The browser and API never accept or return asset://. Only this worker adapter constructs it after decrypting an active local ProviderIdentityAsset.

Create backend/src/ip_saas/providers/volcengine/cloned_avatar.py. Its signed OpenAPI caller must use `service_code="cv"`, `version="2024-06-06"`, enrollment actions `RealmanAvatarTrainingTaskStreaming` and `GetRealmanAvatarTrainingTaskStreaming`, generation actions `RealmanAvatarCreationTask` and `GetRealmanAvatarCreationTask`, and the production endpoint `https://open.volcengineapi.com`. Every submit, poll, and deletion response passes the supplier correlation value through `normalize_provider_request_id` before it reaches a result, usage meter, attempt journal, or ledger row. Keep its provider resource reference encrypted under the same boundary. Add one contract test per action and assert a Seedance asset receipt is rejected by this adapter.

- [ ] **Step 7: Run provider contract tests**

Run:

~~~bash
cd backend && env -u ARK_API_KEY -u DOUBAO_SPEECH_ACCESS_TOKEN uv run pytest tests/contract/governance/test_provider_adapters.py -v
~~~

Expected: PASS, at least 9 tests: success, provider error, timeout, deletion unsupported, wrong capability, and secret-redaction paths are covered for the three independent adapters.

- [ ] **Step 8: Write failing durable-attempt, exact-crash, and supplier-first tests**

Create backend/tests/integration/governance/test_provider_crash_reconciliation.py:

~~~python
from uuid import uuid4

import pytest
from pydantic import ValidationError
from sqlalchemy import func, select

from ip_saas.common.tasking import TaskStatus
from ip_saas.common.errors import Conflict
from ip_saas.modules.billing.models import ProviderCostEntry
from ip_saas.modules.governance.enums import BetaCapability
from ip_saas.modules.governance.models import (
    CapabilitySupplierCostReconciliation,
    GovernedProviderRequestAttempt,
)
from ip_saas.modules.governance.provider_crash import (
    InjectedProviderCrash,
    SupplierRequestLine,
)
from ip_saas.workers.task_consumer import TaskHandlerDisposition


def test_supplier_evidence_trims_provider_request_id() -> None:
    line = SupplierRequestLine(
        provider_request_id="  provider-request-1  ",
        client_attempt_id=uuid4(),
        capability=BetaCapability.VOICE_CLONE,
        provider="volcengine",
        model_id="voice-model",
        model_version="v1",
        billable=True,
    )
    assert line.provider_request_id == "provider-request-1"


@pytest.mark.parametrize(
    "raw", [None, 7, "", "   ", "nOnE", "MISSING", "x" * 201]
)
def test_supplier_evidence_rejects_invalid_provider_request_id(raw: object) -> None:
    with pytest.raises(ValidationError):
        SupplierRequestLine(
            provider_request_id=raw,
            client_attempt_id=uuid4(),
            capability=BetaCapability.VOICE_CLONE,
            provider="volcengine",
            model_id="voice-model",
            model_version="v1",
            billable=True,
        )


@pytest.mark.parametrize("capability", list(BetaCapability))
@pytest.mark.parametrize(
    "raw", [None, 7, "", "   ", "nOnE", "MISSING", "x" * 201]
)
def test_invalid_terminal_request_identity_fails_closed_before_cost_input(
    governance_rig,
    capability: BetaCapability,
    raw: object,
) -> None:
    case = governance_rig.provider_crash_case(capability)
    assert (
        case.run_terminal_response_with_request_id(raw)
        is TaskHandlerDisposition.RETRY
    )
    assert case.provider_call_count() == 1
    assert case.provider_cost_input_count() == 0
    assert case.provider_cost_count() == 0
    assert case.persisted_provider_request_ids() == set()
    assert case.hold_status() == "active"
    case.exhaust_to_reconciliation_required()
    assert case.task_status() == TaskStatus.RECONCILIATION_REQUIRED
    assert case.provider_call_count() == 1
    assert case.provider_cost_input_count() == 0


@pytest.mark.parametrize("capability", list(BetaCapability))
def test_paid_response_sigkill_reconciles_supplier_line_to_exact_task(
    governance_rig,
    capability: BetaCapability,
) -> None:
    case = governance_rig.provider_crash_case(capability)
    with pytest.raises(InjectedProviderCrash, match="paid response before persistence"):
        case.run_fake_exact_crash()
    case.refresh()
    attempt = case.session.scalar(select(GovernedProviderRequestAttempt).where(
        GovernedProviderRequestAttempt.task_id == case.task.id
    ))
    assert attempt is not None
    assert attempt.client_attempt_id == case.client_attempt_id
    assert attempt.status == "prepared"
    assert attempt.send_started_at is not None
    assert case.provider_result_persisted() is False
    assert case.provider_cost_task_ids() == set()

    case.exhaust_to_reconciliation_required()
    assert case.task_status() == TaskStatus.RECONCILIATION_REQUIRED
    assert case.hold_status() == "active"
    calls_before = case.provider_call_count()
    assert case.consume_again() is TaskHandlerDisposition.RETRY
    assert case.provider_call_count() == calls_before

    links = case.reconciler.reconcile(
        case.session,
        request_log_bytes=case.request_log_bytes(),
        statement_bytes=case.final_statement_bytes(),
        idempotency_key=f"supplier-final:{case.run_id}",
    )
    assert len(links) == 1
    cost = case.session.get(ProviderCostEntry, links[0].provider_cost_entry_id)
    assert cost is not None and cost.task_id == case.task.id
    assert links[0].task_id == case.task.id
    assert case.internal_spend_fen() == case.supplier_amount_fen
    assert case.hold_status() == "settled"
    assert case.task_status() == TaskStatus.FAILED


def test_customer_crash_persists_supplier_cost_but_releases_all_credit(
    governance_rig,
) -> None:
    case = governance_rig.provider_crash_case(
        BetaCapability.SEEDANCE_AUTHORIZED_PORTRAIT,
        billing_mode="customer_credit",
    )
    with pytest.raises(InjectedProviderCrash):
        case.run_fake_exact_crash()
    case.exhaust_to_reconciliation_required()
    case.reconciler.reconcile(
        case.session,
        request_log_bytes=case.request_log_bytes(),
        statement_bytes=case.final_statement_bytes(),
        idempotency_key=f"supplier-final:{case.run_id}",
    )
    assert case.provider_cost_task_ids() == {case.task.id}
    assert case.customer_actual_amount() == 0
    assert case.hold_status() == "released"
    assert case.task_status() == TaskStatus.FAILED


def test_retry_exhaustion_with_durable_zero_send_proof_releases_without_cost(
    governance_rig,
) -> None:
    case = governance_rig.provider_crash_case(BetaCapability.VOICE_CLONE)
    attempt_no = case.fail_before_send_until_reconciliation_required()
    assert case.task_status() == TaskStatus.RECONCILIATION_REQUIRED
    assert case.hold_status() == "active"
    case.task_reconciliation.finalize_no_provider_call(
        case.session,
        task_id=case.task.id,
        attempt_no=attempt_no,
        reason="all durable governed attempts stopped before send marker",
        idempotency_key=f"zero-call:{case.task.id}:{attempt_no}",
    )
    assert case.provider_call_count() == 0
    assert case.provider_cost_input_count() == 0
    assert case.provider_cost_count() == 0
    assert case.hold_status() == "released"
    assert case.task_status() == TaskStatus.FAILED


def test_active_lease_retries_and_expired_lease_reclaims_without_second_send(
    governance_rig,
) -> None:
    case = governance_rig.provider_crash_case(
        BetaCapability.SEEDANCE_AUTHORIZED_PORTRAIT
    )
    with pytest.raises(InjectedProviderCrash):
        case.run_fake_exact_crash()
    original_attempt_no = case.task_attempt_no()
    calls_before = case.provider_call_count()
    assert case.consume_during_active_lease() is TaskHandlerDisposition.RETRY
    assert case.task_attempt_no() == original_attempt_no
    case.expire_current_lease()
    assert case.consume_again() is TaskHandlerDisposition.RETRY
    assert case.task_attempt_no() == original_attempt_no + 1
    assert case.provider_call_count() == calls_before
    assert case.provider_cost_count() == 0
    assert case.hold_status() == "active"


def test_two_supplier_lines_for_one_task_finalize_once_as_one_batch(
    governance_rig,
) -> None:
    case = governance_rig.provider_crash_case(
        BetaCapability.TRADITIONAL_DIGITAL_HUMAN,
        billable_calls=2,
    )
    with pytest.raises(InjectedProviderCrash):
        case.run_two_paid_calls_then_crash_before_persistence()
    case.exhaust_to_reconciliation_required()
    links = case.reconciler.reconcile(
        case.session,
        request_log_bytes=case.request_log_bytes(),
        statement_bytes=case.final_statement_bytes(),
        idempotency_key=f"supplier-final:{case.run_id}",
    )
    assert len(links) == 2
    assert case.provider_cost_count() == 2
    assert case.task_finalizer_call_count() == 1
    assert case.hold_mutation_count() == 1
    assert case.internal_spend_fen() == case.supplier_amount_fen
    assert case.task_status() == TaskStatus.FAILED


def test_database_rows_without_supplier_statement_never_pass(governance_rig) -> None:
    case = governance_rig.provider_crash_case(BetaCapability.VOICE_CLONE)
    with pytest.raises(InjectedProviderCrash):
        case.run_fake_exact_crash()
    case.exhaust_to_reconciliation_required()
    with pytest.raises(Conflict, match="supplier request sets differ"):
        case.reconciler.reconcile(
            case.session,
            request_log_bytes=case.request_log_bytes(),
            statement_bytes=case.final_statement_bytes(
                provider_request_id="supplier-request-not-in-log"
            ),
            idempotency_key=f"supplier-final:{case.run_id}",
        )
    assert case.session.scalar(
        select(func.count()).select_from(ProviderCostEntry)
    ) == 0


def test_reconciliation_replay_is_one_cost_and_one_link(governance_rig) -> None:
    case = governance_rig.provider_crash_case(
        BetaCapability.TRADITIONAL_DIGITAL_HUMAN
    )
    with pytest.raises(InjectedProviderCrash):
        case.run_fake_exact_crash()
    case.exhaust_to_reconciliation_required()
    arguments = {
        "request_log_bytes": case.request_log_bytes(),
        "statement_bytes": case.final_statement_bytes(),
        "idempotency_key": f"supplier-final:{case.run_id}",
    }
    first = case.reconciler.reconcile(case.session, **arguments)
    second = case.reconciler.reconcile(case.session, **arguments)
    assert [row.id for row in first] == [row.id for row in second]
    assert case.session.scalar(
        select(func.count()).select_from(ProviderCostEntry)
    ) == 1
    assert case.session.scalar(select(func.count()).select_from(
        CapabilitySupplierCostReconciliation
    )) == 1
    assert case.task_finalizer_call_count() == 1
    assert case.hold_mutation_count() == 1


def test_stale_reconciler_cannot_write_cost_gate_or_task_state(governance_rig) -> None:
    case = governance_rig.provider_crash_case(
        BetaCapability.SEEDANCE_AUTHORIZED_PORTRAIT
    )
    with pytest.raises(InjectedProviderCrash):
        case.run_fake_exact_crash()
    stale_worker = case.captured_worker_attempt()
    attempt_no = case.exhaust_to_reconciliation_required()
    with pytest.raises(Conflict, match="stale worker"):
        stale_worker.persist_result_cost_or_delivery()
    case.corrupt_journal_worker_attempt_no(attempt_no + 1)
    with pytest.raises(Conflict, match="newer than task"):
        case.reconciler.reconcile(
            case.session,
            request_log_bytes=case.request_log_bytes(),
            statement_bytes=case.final_statement_bytes(),
            idempotency_key=f"supplier-final:{case.run_id}",
        )
    assert case.provider_cost_task_ids() == set()
    assert case.gate_attestation_count() == 0
    assert case.task_status() == TaskStatus.RECONCILIATION_REQUIRED
    assert case.hold_status() == "active"
~~~

Extend `GovernanceTestRig` in backend/tests/support/governance_rig.py with `provider_crash_case(capability, billing_mode="internal_cost", billable_calls=1)`. It creates the account/project and real TaskRecord through `submit_customer` or `submit_internal`, an identity enrollment or governed GenerationJob appropriate to the capability, an explicit SQL hold, a `BillingService` with the already-frozen real integration `GenerationLimitService`, a counting deterministic terminal provider with positive complete usage, and the test-only injected crash hook. It exposes every method used above. `run_terminal_response_with_request_id(raw)` commits one send marker, makes exactly one selected-capability fake provider call, and routes a terminal result carrying only that raw identity through the same `EnrollmentResult` or `IdentityGenerationResult` constructor and Worker normalization used in production; `ValueError` or Pydantic failure maps to RETRY, persists no response/request identity, never constructs `ProviderCostInput`, and makes no second call on reclaim. A spy immediately around the Billing boundary implements `provider_cost_input_count()`, while `provider_cost_count()` reads SQL, so the invalid and zero-call tests distinguish “no cost object was ever built” from “a cost row happened not to commit.”

`exhaust_to_reconciliation_required()` advances the clock beyond each lease and calls the real `start(..., max_attempts=8)` until Plan 01 durably returns `RECONCILIATION_REQUIRED`; it asserts the hold is still active. `fail_before_send_until_reconciliation_required()` commits durable attempts but never their send marker, proving the Plan 01 no-call finalizer rather than using a boolean. That zero-call branch invokes `finalize_no_provider_call()` directly with evidence and must not call any provider-cost builder or Billing settlement method. `consume_again()` invokes the real capability handler, not a harness shortcut. `task_finalizer_call_count()` observes a spy around the one injected Plan 01 `TaskReconciliationService`, and `run_two_paid_calls_then_crash_before_persistence()` creates two stable canonical provider request identities for one TaskRecord so batching is exercised. `corrupt_journal_worker_attempt_no(value)` is confined to the stale-evidence negative test and directly changes the durable test row before invoking the real reconciler; production never accepts an attempt number from a caller. The existing `governance_rig` fixture is the only custom test parameter, no TaskRecord is constructed directly, and no network, credential, or paid provider is used.

Run:

~~~bash
cd backend && env -u ARK_API_KEY -u DOUBAO_SPEECH_ACCESS_TOKEN uv run pytest tests/integration/governance/test_provider_crash_reconciliation.py -v
~~~

Expected: FAIL because the provider-attempt tables, exact crash hook, feature manifest port, and supplier-first batch reconciler do not exist. The counting fake proves the reconciliation state never emits a second provider request.

- [ ] **Step 9: Persist capability provider attempts and supplier-cost mappings in `0006_governance`**

Append to backend/src/ip_saas/modules/governance/models.py:

~~~python
class GovernedProviderRequestAttempt(Base):
    __tablename__ = "governed_provider_request_attempts"

    id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), primary_key=True, default=uuid4
    )
    task_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("task_records.id"), nullable=False, index=True
    )
    aggregate_type: Mapped[str] = mapped_column(String(40), nullable=False)
    aggregate_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), nullable=False, index=True
    )
    capability: Mapped[str] = mapped_column(String(60), nullable=False, index=True)
    provider: Mapped[str] = mapped_column(String(60), nullable=False)
    model_id: Mapped[str] = mapped_column(String(200), nullable=False)
    model_version: Mapped[str] = mapped_column(String(100), nullable=False)
    logical_call_key: Mapped[str] = mapped_column(String(120), nullable=False)
    client_attempt_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), nullable=False
    )
    worker_attempt_no: Mapped[int] = mapped_column(Integer, nullable=False)
    request_fingerprint: Mapped[str] = mapped_column(String(64), nullable=False)
    request_payload_sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    status: Mapped[str] = mapped_column(
        String(32), nullable=False, default="prepared"
    )
    provider_request_id: Mapped[str | None] = mapped_column(String(200))
    response_sha256: Mapped[str | None] = mapped_column(String(64))
    send_started_at: Mapped[datetime | None] = mapped_column(
        DateTime(timezone=True)
    )
    supplier_request_log_run_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True)
    )
    supplier_request_log_sha256: Mapped[str | None] = mapped_column(String(64))
    prepared_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False
    )
    response_observed_at: Mapped[datetime | None] = mapped_column(
        DateTime(timezone=True)
    )

    __table_args__ = (
        UniqueConstraint(
            "provider", "client_attempt_id",
            name="uq_governed_provider_attempt_client",
        ),
        UniqueConstraint(
            "task_id", "logical_call_key",
            name="uq_governed_provider_attempt_logical_call",
        ),
        UniqueConstraint(
            "provider", "provider_request_id",
            name="uq_governed_provider_attempt_request_identity",
        ),
        CheckConstraint(
            "aggregate_type IN ('identity_enrollment','generation_job')",
            name="ck_governed_provider_attempt_aggregate",
        ),
        CheckConstraint(
            "status IN ('prepared','response_observed','supplier_matched','reconciled')",
            name="ck_governed_provider_attempt_status",
        ),
        CheckConstraint(
            "worker_attempt_no > 0 AND length(request_fingerprint) = 64 "
            "AND length(request_payload_sha256) = 64 "
            "AND (supplier_request_log_sha256 IS NULL "
            "OR length(supplier_request_log_sha256) = 64)",
            name="ck_governed_provider_attempt_identity",
        ),
        CheckConstraint(
            "provider_request_id IS NULL OR "
            "(provider_request_id = btrim(provider_request_id) AND "
            "lower(provider_request_id) NOT IN ('none', 'missing') AND "
            "length(provider_request_id) BETWEEN 1 AND 200)",
            name="ck_governed_provider_request_id",
        ),
    )


class CapabilitySupplierCostReconciliation(Base):
    __tablename__ = "capability_supplier_cost_reconciliations"

    id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), primary_key=True, default=uuid4
    )
    provider_request_attempt_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True),
        ForeignKey("governed_provider_request_attempts.id"),
        nullable=False,
        unique=True,
    )
    task_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("task_records.id"), nullable=False, index=True
    )
    capability: Mapped[str] = mapped_column(String(60), nullable=False, index=True)
    provider_request_id: Mapped[str] = mapped_column(String(200), nullable=False)
    supplier_statement_line_id: Mapped[str] = mapped_column(
        String(300), nullable=False
    )
    request_log_sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    statement_sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    provider_cost_entry_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True),
        ForeignKey("provider_cost_entries.id"),
        nullable=False,
        unique=True,
    )
    amount_fen: Mapped[int] = mapped_column(Integer, nullable=False)
    idempotency_key: Mapped[str] = mapped_column(String(160), nullable=False)
    input_fingerprint: Mapped[str] = mapped_column(String(64), nullable=False)
    statement_finalized_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False
    )
    reconciled_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False
    )

    __table_args__ = (
        UniqueConstraint(
            "capability", "provider_request_id",
            name="uq_capability_supplier_provider_request",
        ),
        CheckConstraint("amount_fen >= 0", name="ck_capability_supplier_amount"),
        CheckConstraint(
            "provider_request_id = btrim(provider_request_id) AND "
            "lower(provider_request_id) NOT IN ('none', 'missing') AND "
            "length(provider_request_id) BETWEEN 1 AND 200",
            name="ck_capability_supplier_provider_request_id",
        ),
        CheckConstraint(
            "length(request_log_sha256) = 64 AND length(statement_sha256) = 64 "
            "AND length(input_fingerprint) = 64",
            name="ck_capability_supplier_hashes",
        ),
    )
~~~

Make the request identity fields immutable, allow `send_started_at` only the monotonic NULL→timestamp transition, allow supplier identity/status only through the scanner's locked transition, and make the reconciliation row append-only with SQLAlchemy event listeners. Add both tables, constraints, indexes, and foreign keys to the existing backend/migrations/versions/0006_governance.py; `down_revision` remains `0005_resellers`. In downgrade, drop the reconciliation table before the attempt table and before any referenced governance aggregate. Do not create another migration revision.

Extend backend/tests/integration/governance/test_migration.py to assert both tables, every named unique/check constraint above (including stable client ID, logical call, the nullable and non-null 1–200-character trimmed provider request constraints, provider request identity, and append-only supplier link), both provider request columns have length 200, task/provider-cost foreign keys, and downgrade/upgrade. Run:

~~~bash
cd backend
uv run alembic downgrade 0005_resellers
uv run alembic upgrade 0006_governance
uv run pytest tests/integration/governance/test_migration.py -q
uv run alembic downgrade 0005_resellers
uv run alembic upgrade 0006_governance
uv run alembic check
~~~

Expected: both cycles pass, the two evidence tables disappear on downgrade, Alembic reports no drift, and `0006_governance` remains the only head added by this plan.

- [ ] **Step 10: Commit a stable attempt before HTTP and expose the exact SIGKILL hook**

Create backend/src/ip_saas/modules/governance/provider_crash.py with this request journal and fault boundary:

~~~python
from collections.abc import Callable, Mapping, Protocol
from datetime import datetime
from hashlib import sha256
from json import dumps
from uuid import UUID, uuid5

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.errors import Conflict
from ip_saas.common.tasking import TaskStatus
from ip_saas.modules.tasks.models import TaskRecord

from .models import GovernedProviderRequestAttempt


GOVERNED_ATTEMPT_NAMESPACE = UUID("df35a56f-d913-45a0-921a-c5f81f75b098")


def canonical_sha256(value: Mapping[str, object]) -> str:
    body = dumps(
        dict(value), sort_keys=True, separators=(",", ":"), default=str
    ).encode("utf-8")
    return sha256(body).hexdigest()


class GovernedProviderAttemptJournal:
    def __init__(self, now: Callable[[], datetime]) -> None:
        self.now = now

    def prepare(
        self,
        session: Session,
        *,
        task: TaskRecord,
        aggregate_type: str,
        aggregate_id: UUID,
        capability: str,
        provider: str,
        model_id: str,
        model_version: str,
        logical_call_key: str,
        request_payload: Mapping[str, object],
        worker_attempt_no: int,
    ) -> GovernedProviderRequestAttempt:
        if task.status != TaskStatus.RUNNING or task.attempt_no != worker_attempt_no:
            raise Conflict("stale worker cannot prepare governed provider request")
        client_attempt_id = uuid5(
            GOVERNED_ATTEMPT_NAMESPACE,
            f"{task.id}:{capability}:{aggregate_type}:{aggregate_id}:{logical_call_key}",
        )
        payload_sha = canonical_sha256(request_payload)
        existing = session.scalar(select(GovernedProviderRequestAttempt).where(
            GovernedProviderRequestAttempt.provider == provider,
            GovernedProviderRequestAttempt.client_attempt_id == client_attempt_id,
        ).with_for_update())
        if existing is not None:
            expected = (
                task.id, aggregate_type, aggregate_id, capability, model_id,
                model_version, logical_call_key, task.request_fingerprint, payload_sha,
            )
            actual = (
                existing.task_id, existing.aggregate_type, existing.aggregate_id,
                existing.capability, existing.model_id, existing.model_version,
                existing.logical_call_key, existing.request_fingerprint,
                existing.request_payload_sha256,
            )
            if actual != expected:
                raise Conflict("governed client attempt was reused with changed input")
            existing.worker_attempt_no = worker_attempt_no
            return existing
        row = GovernedProviderRequestAttempt(
            task_id=task.id,
            aggregate_type=aggregate_type,
            aggregate_id=aggregate_id,
            capability=capability,
            provider=provider,
            model_id=model_id,
            model_version=model_version,
            logical_call_key=logical_call_key,
            client_attempt_id=client_attempt_id,
            worker_attempt_no=worker_attempt_no,
            request_fingerprint=task.request_fingerprint,
            request_payload_sha256=payload_sha,
            status="prepared",
            prepared_at=self.now(),
        )
        session.add(row)
        session.flush()
        return row


class InjectedProviderCrash(RuntimeError):
    pass


class GovernedProviderFaults(Protocol):
    def before_http(self, attempt: GovernedProviderRequestAttempt) -> None:
        raise NotImplementedError

    def after_paid_response_before_persistence(
        self, attempt: GovernedProviderRequestAttempt, result: object,
    ) -> None:
        raise NotImplementedError


class NoGovernedProviderFaults:
    def before_http(self, attempt: GovernedProviderRequestAttempt) -> None:
        return None

    def after_paid_response_before_persistence(
        self, attempt: GovernedProviderRequestAttempt, result: object,
    ) -> None:
        return None
~~~

Expose these journal and fault operations as the provider-call boundary used by the Task 6 PoC runner and by both Task 7 Workers. The caller prepares the attempt inside the claim transaction and exits `session_scope()` before the provider turn. In a second short fenced transaction it calls `mark_send_started` and commits before any HTTP; then it replaces the request idempotency/correlation field with `str(attempt.client_attempt_id)`. A reclaimed caller that finds `send_started_at` already set but no persisted terminal response returns RETRY without HTTP, domain, billing, quota, event, or delivery writes; successive lease expiries lead Plan 01 `start(..., max_attempts=8)` to `RECONCILIATION_REQUIRED`, with the hold intact. It calls `faults.before_http(attempt)` immediately before send. For synchronous success or the first terminal poll carrying `ProviderUsage`, it calls `faults.after_paid_response_before_persistence(attempt, result)` immediately after the adapter returns and before opening the transaction that writes provider task/reference/result/usage or calls BillingService. Only after that hook returns may it heartbeat the captured `attempt_no`, record response SHA, update the domain aggregate, settle cost, and terminalize the TaskRecord in one fenced transaction. Task 7 is the first step that modifies `IdentityEnrollmentWorker` and the governed Plan 03 `MediaSubmitWorker`; Task 6's integration rig proves this generic boundary without referring to not-yet-created enrollment code.

The live PoC hook appends and fsyncs only task ID, stable attempt ID, provider response ID/hash, and four checkpoint booleans, then calls `os.kill(os.getpid(), signal.SIGKILL)`. It never records source media, provider identity references, prompt, signed URL, response body, or secret. The provider headers remain correlation attempts, not assumed idempotency: absent official query/deduplication proof, only the supplier-statement reconciler below can qualify the capability.

- [ ] **Step 11: Reconcile finalized supplier billing into `ProviderCostEntry.task_id` and strict gate evidence**

Append strict Pydantic supplier exports, `CapabilitySupplierReconciler`, and `CapabilityProviderCrashEvidenceV1` to provider_crash.py. The frozen core is:

~~~python
from decimal import Decimal
from enum import StrEnum
from typing import Annotated, Literal

from pydantic import (
    AfterValidator,
    BaseModel,
    ConfigDict,
    Field,
    HttpUrl,
    StringConstraints,
    model_validator,
)

from ip_saas.modules.billing.models import ProviderCostEntry, ReconciliationStatus
from ip_saas.modules.billing.service import BillingMode, ProviderCostInput
from ip_saas.modules.tasks.reconciliation import (
    NoProviderCallEvidence,
    NoProviderCallEvidencePort,
    ProviderCostManifestPort,
    ProviderRequestIdentity,
    TaskReconciliationService,
)

from .enums import BetaCapability, GateKind
from .models import (
    CapabilityGateAttestation,
    CapabilitySupplierCostReconciliation,
)


Sha256Hex = Annotated[str, Field(pattern=r"^[0-9a-f]{64}$")]


def reject_missing_provider_request_id(value: str) -> str:
    if value.casefold() in {"none", "missing"}:
        raise ValueError("provider request id sentinel is invalid")
    return value


ProviderRequestId = Annotated[
    str,
    StringConstraints(
        strip_whitespace=True,
        min_length=1,
        max_length=200,
    ),
    AfterValidator(reject_missing_provider_request_id),
]


class StrictProviderEvidence(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)


class RecoveryProof(StrEnum):
    OFFICIAL_QUERY = "official_query"
    OFFICIAL_IDEMPOTENCY = "official_idempotency"
    SUPPLIER_STATEMENT = "supplier_statement_reconciliation"


class SupplierRequestLine(StrictProviderEvidence):
    provider_request_id: ProviderRequestId
    client_attempt_id: UUID
    capability: BetaCapability
    provider: Literal["volcengine"]
    model_id: str = Field(min_length=1, max_length=200)
    model_version: str = Field(min_length=1, max_length=100)
    billable: bool


class SupplierRequestLog(StrictProviderEvidence):
    schema_version: Literal[1] = 1
    run_id: UUID
    account_fingerprint: Sha256Hex
    complete: Literal[True]
    exported_at: datetime
    lines: tuple[SupplierRequestLine, ...]


class FinalStatementLine(StrictProviderEvidence):
    line_id: str = Field(min_length=1, max_length=300)
    provider_request_id: ProviderRequestId
    native_quantity: Decimal = Field(gt=0)
    native_unit: str = Field(min_length=1, max_length=40)
    supplier_amount_minor: int = Field(gt=0)
    supplier_currency: Literal["CNY"]
    amount_fen: int = Field(gt=0)


class FinalSupplierStatement(StrictProviderEvidence):
    schema_version: Literal[1] = 1
    run_id: UUID
    account_fingerprint: Sha256Hex
    final: Literal[True]
    finalized_at: datetime
    lines: tuple[FinalStatementLine, ...] = Field(min_length=1)


class CrashCheckpointEvidence(StrictProviderEvidence):
    task_id: UUID
    client_attempt_id: UUID
    http_send_observed: Literal[True]
    paid_response_observed: Literal[True]
    provider_result_persisted_before_kill: Literal[False]
    provider_cost_persisted_before_kill: Literal[False]
    provider_request_id: ProviderRequestId
    provider_cost_entry_id: UUID
    provider_cost_task_id: UUID
    supplier_amount_fen: int = Field(gt=0)
    ledger_amount_fen: int = Field(gt=0)

    @model_validator(mode="after")
    def task_and_amount_match(self) -> "CrashCheckpointEvidence":
        if self.provider_cost_task_id != self.task_id:
            raise ValueError("crash cost is not linked to its TaskRecord")
        if self.supplier_amount_fen != self.ledger_amount_fen:
            raise ValueError("supplier and ledger amounts differ")
        return self


class CapabilityProviderCrashEvidenceV1(StrictProviderEvidence):
    schema_version: Literal[1] = 1
    capability: BetaCapability
    entitlement_id: UUID
    provider: Literal["volcengine"]
    provider_account_fingerprint: Sha256Hex
    region: str = Field(min_length=1, max_length=40)
    model_id: str = Field(min_length=1, max_length=200)
    model_version: str = Field(min_length=1, max_length=100)
    release_snapshot_sha256: Sha256Hex
    run_id: UUID
    submitted: int = Field(ge=30)
    completed: int = Field(ge=0)
    provider_completions: int = Field(ge=1)
    request_log_evidence_asset_id: UUID
    request_log_sha256: Sha256Hex
    final_statement_evidence_asset_id: UUID
    final_statement_sha256: Sha256Hex
    final_statement: Literal[True]
    reconciliation_report_evidence_asset_id: UUID
    reconciliation_report_sha256: Sha256Hex
    attempt_journal_evidence_asset_id: UUID
    attempt_journal_sha256: Sha256Hex
    sigkill_transcript_evidence_asset_id: UUID
    sigkill_transcript_sha256: Sha256Hex
    official_request_identity_url: HttpUrl
    official_request_identity_sha256: Sha256Hex
    observed_identity_proof_sha256: Sha256Hex | None = None
    recovery_proof: RecoveryProof
    reconciler_implementation_sha256: Sha256Hex | None = None
    deployed_image_digest: Annotated[
        str, Field(pattern=r"^sha256:[0-9a-f]{64}$")
    ]
    max_cost_fen: int = Field(gt=0)
    settled_amount_fen: int = Field(ge=0)
    crash: CrashCheckpointEvidence
    provider_cost_rows_with_task_id: int = Field(ge=0)
    provider_cost_task_mismatch: Literal[0]
    duplicate_charge: Literal[0]
    unauthorized_delivery: Literal[0]
    label_loss: Literal[0]
    batch_passed: Literal[True]
    reviewed_by_operator: UUID
    reviewed_by_independent_reviewer: UUID
    reviewed_at: datetime
    passed: Literal[True]

    @model_validator(mode="after")
    def complete_recovery_proof(self) -> "CapabilityProviderCrashEvidenceV1":
        if self.completed / self.submitted < 0.95:
            raise ValueError("batch completion rate is below 95 percent")
        if self.provider_completions != self.provider_cost_rows_with_task_id:
            raise ValueError("provider completion and task-cost counts differ")
        if self.settled_amount_fen > self.max_cost_fen:
            raise ValueError("provider PoC exceeded its hard cost cap")
        if self.reviewed_by_operator == self.reviewed_by_independent_reviewer:
            raise ValueError("provider PoC needs an independent reviewer")
        if (
            self.recovery_proof is RecoveryProof.SUPPLIER_STATEMENT
            and self.reconciler_implementation_sha256 is None
        ):
            raise ValueError("statement recovery needs the deployed reconciler hash")
        if (
            self.recovery_proof
            in {RecoveryProof.OFFICIAL_QUERY, RecoveryProof.OFFICIAL_IDEMPOTENCY}
            and self.observed_identity_proof_sha256 is None
        ):
            raise ValueError("official recovery needs observed query/deduplication proof")
        return self
~~~

Add the feature-owned manifest port and supplier-first batch reconciler below the evidence models. This is the only Plan 06 scanner finalizer; it delegates each task once to Plan 01 and never loops settlement or `fail()` per supplier line:

~~~python
from collections import defaultdict
import hashlib
import json

from sqlalchemy import select

from ip_saas.common.errors import Conflict
from ip_saas.common.tasking import TaskStatus
from ip_saas.modules.tasks.models import TaskRecord

from .models import GovernedProviderRequestAttempt


def evidence_sha256(body: bytes) -> str:
    return hashlib.sha256(body).hexdigest()


class CapabilityProviderCostManifest(ProviderCostManifestPort):
    def require_complete_provider_requests(
        self,
        session: Session,
        task_id: UUID,
        through_attempt_no: int,
    ) -> tuple[ProviderRequestIdentity, ...]:
        rows = tuple(session.scalars(
            select(GovernedProviderRequestAttempt)
            .where(
                GovernedProviderRequestAttempt.task_id == task_id,
                GovernedProviderRequestAttempt.worker_attempt_no <= through_attempt_no,
                GovernedProviderRequestAttempt.status.in_(
                    ("supplier_matched", "reconciled")
                ),
            )
            .order_by(
                GovernedProviderRequestAttempt.provider,
                GovernedProviderRequestAttempt.provider_request_id,
            )
        ))
        if not rows:
            raise Conflict("supplier-backed provider request manifest is empty")
        if any(
            row.provider_request_id is None
            or row.supplier_request_log_run_id is None
            or row.supplier_request_log_sha256 is None
            for row in rows
        ):
            raise Conflict("provider request manifest lacks supplier evidence")
        identities = tuple(
            ProviderRequestIdentity(row.provider, row.provider_request_id or "")
            for row in rows
        )
        if len(set(identities)) != len(identities):
            raise Conflict("provider request manifest is duplicated")
        return identities


class GovernedNoProviderCallEvidence(NoProviderCallEvidencePort):
    def require_no_provider_call(
        self,
        session: Session,
        task_id: UUID,
        through_attempt_no: int,
    ) -> NoProviderCallEvidence:
        rows = tuple(session.scalars(
            select(GovernedProviderRequestAttempt)
            .where(
                GovernedProviderRequestAttempt.task_id == task_id,
                GovernedProviderRequestAttempt.worker_attempt_no
                <= through_attempt_no,
            )
            .order_by(GovernedProviderRequestAttempt.client_attempt_id)
        ))
        if not rows or any(row.send_started_at is not None for row in rows):
            raise Conflict("durable attempt journal cannot prove zero provider calls")
        body = json.dumps(
            [
                {
                    "id": str(row.id),
                    "client_attempt_id": str(row.client_attempt_id),
                    "worker_attempt_no": row.worker_attempt_no,
                    "request_fingerprint": row.request_fingerprint,
                    "request_payload_sha256": row.request_payload_sha256,
                    "send_started_at": None,
                }
                for row in rows
            ],
            ensure_ascii=False,
            allow_nan=False,
            separators=(",", ":"),
            sort_keys=True,
        ).encode("utf-8")
        return NoProviderCallEvidence(
            task_id=task_id,
            through_attempt_no=through_attempt_no,
            evidence_ref=(
                f"governed-attempt-journal:{task_id}:{through_attempt_no}"
            ),
            evidence_sha256=evidence_sha256(body),
        )


class CapabilitySupplierReconciler:
    def __init__(
        self,
        task_reconciliation: TaskReconciliationService,
        now: Callable[[], datetime],
    ) -> None:
        self.task_reconciliation = task_reconciliation
        self.now = now

    @staticmethod
    def _parse_exports(
        request_log_bytes: bytes,
        statement_bytes: bytes,
    ) -> tuple[SupplierRequestLog, FinalSupplierStatement]:
        request_log = SupplierRequestLog.model_validate_json(request_log_bytes)
        statement = FinalSupplierStatement.model_validate_json(statement_bytes)
        if request_log.run_id != statement.run_id:
            raise Conflict("supplier run identifiers differ")
        if request_log.account_fingerprint != statement.account_fingerprint:
            raise Conflict("supplier account fingerprints differ")
        billable = tuple(line for line in request_log.lines if line.billable)
        log_ids = tuple(line.provider_request_id for line in billable)
        statement_ids = tuple(line.provider_request_id for line in statement.lines)
        if len(set(log_ids)) != len(log_ids) or len(set(statement_ids)) != len(
            statement_ids
        ):
            raise Conflict("supplier request identity is duplicated")
        if set(log_ids) != set(statement_ids):
            raise Conflict("supplier request sets differ")
        return request_log, statement

    def reconcile(
        self,
        session: Session,
        *,
        request_log_bytes: bytes,
        statement_bytes: bytes,
        idempotency_key: str,
    ) -> tuple[CapabilitySupplierCostReconciliation, ...]:
        request_log, statement = self._parse_exports(
            request_log_bytes, statement_bytes
        )
        request_log_hash = evidence_sha256(request_log_bytes)
        statement_hash = evidence_sha256(statement_bytes)
        billable_by_id = {
            line.provider_request_id: line
            for line in request_log.lines
            if line.billable
        }
        statement_by_id = {
            line.provider_request_id: line for line in statement.lines
        }
        costs_by_task: dict[UUID, list[ProviderCostInput]] = defaultdict(list)
        attempts_by_request: dict[str, GovernedProviderRequestAttempt] = {}
        for provider_request_id, supplier_line in billable_by_id.items():
            attempt = session.scalar(
                select(GovernedProviderRequestAttempt)
                .where(
                    GovernedProviderRequestAttempt.client_attempt_id
                    == supplier_line.client_attempt_id
                )
                .with_for_update()
            )
            if attempt is None:
                raise Conflict("supplier request has no durable client attempt")
            if (
                attempt.capability != supplier_line.capability.value
                or attempt.provider != supplier_line.provider
                or attempt.model_id != supplier_line.model_id
                or attempt.model_version != supplier_line.model_version
            ):
                raise Conflict("supplier request crosses capability or model identity")
            if attempt.provider_request_id not in {None, provider_request_id}:
                raise Conflict("durable attempt has another provider request identity")
            attempt.provider_request_id = provider_request_id
            attempt.supplier_request_log_run_id = request_log.run_id
            attempt.supplier_request_log_sha256 = request_log_hash
            attempt.status = "supplier_matched"
            statement_line = statement_by_id[provider_request_id]
            costs_by_task[attempt.task_id].append(ProviderCostInput(
                provider=attempt.provider,
                capability=attempt.capability,
                model_id=attempt.model_id,
                model_version=attempt.model_version,
                native_quantity=statement_line.native_quantity,
                native_unit=statement_line.native_unit,
                supplier_amount_minor=statement_line.supplier_amount_minor,
                supplier_currency=statement_line.supplier_currency,
                amount_fen=statement_line.amount_fen,
                reconciliation_status=ReconciliationStatus.MATCHED,
                task_id=attempt.task_id,
                provider_request_id=provider_request_id,
            ))
            attempts_by_request[provider_request_id] = attempt

        tasks_by_id: dict[UUID, TaskRecord] = {}
        for task_id in sorted(costs_by_task, key=str):
            task = session.scalar(
                select(TaskRecord).where(TaskRecord.id == task_id).with_for_update()
            )
            if task is None:
                raise Conflict("supplier cost has no TaskRecord")
            tasks_by_id[task_id] = task
        for attempt in attempts_by_request.values():
            task = tasks_by_id[attempt.task_id]
            if attempt.worker_attempt_no > task.attempt_no:
                raise Conflict("durable provider attempt is newer than task")
        input_fingerprint = evidence_sha256(json.dumps(
            {
                "request_log_sha256": request_log_hash,
                "statement_sha256": statement_hash,
                "durable_task_attempts": {
                    str(task_id): task.attempt_no
                    for task_id, task in sorted(
                        tasks_by_id.items(), key=lambda item: str(item[0])
                    )
                },
            },
            ensure_ascii=False,
            allow_nan=False,
            separators=(",", ":"),
            sort_keys=True,
        ).encode("utf-8"))
        existing = tuple(session.scalars(
            select(CapabilitySupplierCostReconciliation).where(
                CapabilitySupplierCostReconciliation.request_log_sha256
                == request_log_hash,
                CapabilitySupplierCostReconciliation.statement_sha256
                == statement_hash,
            )
        ))
        if existing:
            if (
                len(existing) != len(statement.lines)
                or any(row.idempotency_key != idempotency_key for row in existing)
                or any(row.input_fingerprint != input_fingerprint for row in existing)
                or {row.provider_request_id for row in existing}
                != {line.provider_request_id for line in statement.lines}
            ):
                raise Conflict("supplier reconciliation replay changed input")
            return existing
        session.flush()

        for task_id, task_costs in costs_by_task.items():
            task = tasks_by_id[task_id]
            status = TaskStatus(task.status)
            if status == TaskStatus.RECONCILIATION_REQUIRED:
                actual_amount = (
                    0
                    if BillingMode(task.billing_mode) == BillingMode.CUSTOMER_CREDIT
                    else sum(cost.amount_fen for cost in task_costs)
                )
                self.task_reconciliation.finalize_provider_costs(
                    session,
                    task_id=task.id,
                    attempt_no=task.attempt_no,
                    actual_amount=actual_amount,
                    provider_costs=tuple(task_costs),
                    reason="supplier-finalized cost after exhausted provider retry",
                    idempotency_key=f"{idempotency_key}:{task.id}",
                )
            elif status == TaskStatus.SUCCEEDED:
                for cost in task_costs:
                    row = session.scalar(select(ProviderCostEntry).where(
                        ProviderCostEntry.provider == cost.provider,
                        ProviderCostEntry.provider_request_id
                        == cost.provider_request_id,
                    ))
                    if row is None or row.task_id != task.id or row.amount_fen != cost.amount_fen:
                        raise Conflict("successful task cost differs from final statement")
            else:
                raise Conflict("task is not supplier-reconcilable")

        links: list[CapabilitySupplierCostReconciliation] = []
        for provider_request_id, statement_line in statement_by_id.items():
            attempt = attempts_by_request[provider_request_id]
            cost = session.scalar(select(ProviderCostEntry).where(
                ProviderCostEntry.provider == attempt.provider,
                ProviderCostEntry.provider_request_id == provider_request_id,
            ))
            if cost is None or cost.task_id != attempt.task_id:
                raise Conflict("task-linked provider cost was not persisted")
            link = CapabilitySupplierCostReconciliation(
                provider_request_attempt_id=attempt.id,
                task_id=attempt.task_id,
                capability=attempt.capability,
                provider_request_id=provider_request_id,
                supplier_statement_line_id=statement_line.line_id,
                request_log_sha256=request_log_hash,
                statement_sha256=statement_hash,
                provider_cost_entry_id=cost.id,
                amount_fen=statement_line.amount_fen,
                idempotency_key=idempotency_key,
                input_fingerprint=input_fingerprint,
                statement_finalized_at=statement.finalized_at,
                reconciled_at=self.now(),
            )
            attempt.status = "reconciled"
            session.add(link)
            links.append(link)
        session.flush()
        return tuple(links)
~~~

The production `TaskReconciliationService` injected above is constructed with `GovernedNoProviderCallEvidence` and `CapabilityProviderCostManifest`; neither has a permissive default. Zero-call finalization requires at least one immutable prepared attempt and proves every row through the exhausted attempt lacks the committed send marker; absence of a cost row alone cannot pass. The paid scanner starts from the complete official request log and finalized supplier statement, never from a database anti-join. It imports the supplier request identity into the already-durable stable attempt, groups every statement line by TaskRecord, and invokes `finalize_provider_costs` exactly once per `RECONCILIATION_REQUIRED` task. Therefore a customer task records every supplier cost but releases its credit hold with zero actual units, while an internal task settles the sum of the complete CNY batch. A normal SUCCEEDED batch gets only the append-only supplier link after its existing task-linked cost is matched. Missing/extra/duplicate supplier lines, a partial replay, wrong account/run/capability/model, a non-final statement, a stale task attempt, or a missing/mismatched existing cost rolls back the entire transaction.

`CapabilityGateAttestationService.record_provider_crash_pass()` first locks the one current entitlement and copies its `id`, provider-account fingerprint, region, model ID, model version, and release snapshot SHA-256 into both `CapabilityProviderCrashEvidenceV1` and the dedicated `CapabilityGateAttestation` columns; caller-supplied copies are rejected. It then stores the exact supplier request log, finalized statement, reconciliation report, attempt journal, and SIGKILL transcript as five private versioned TOS `MediaAsset` rows, re-downloads each version, verifies its SHA-256, and freezes those asset IDs/hashes in `CapabilityProviderCrashEvidenceV1`. It canonicalizes that strict evidence JSON, stores and re-downloads the sixth private object, verifies SHA-256, and inserts `CapabilityGateAttestation` with `gate_kind=GateKind.PROVIDER_CRASH_RECONCILIATION`, the outer evidence asset ID/hash, exact run ID, strict measurement summary, and different operator/reviewer IDs. It can insert PASS only after the reconciler succeeds. Every other PASS producer uses the same locked-entitlement copy routine, so no capability-global or nullable PASS exists. The attestation is therefore hash-pinned to the capability, entitlement, provider account fingerprint, region, model/version, release snapshot, supplier exports, crash transcript, task-cost links, deployed image, and reconciler hash. It never changes the feature flag.

- [ ] **Step 12: Write failing PoC-evaluator and fail-closed production-composition tests**

Create backend/tests/integration/governance/test_provider_poc_evaluator.py:

~~~python
import pytest

from ip_saas.modules.governance.enums import BetaCapability


@pytest.mark.parametrize("capability", list(BetaCapability))
def test_exact_provider_poc_boundary_passes(
    governance_rig,
    capability: BetaCapability,
) -> None:
    case = governance_rig.provider_release_case(capability)
    result = case.evaluator.evaluate(case.passing_measurement())
    assert result.passed
    assert result.failed_gates == ()


@pytest.mark.parametrize(
    ("mutation", "failed_gate"),
    [
        ("submitted_29", "batch_size"),
        ("completion_94_99_percent", "completion_rate"),
        ("cost_over_cap", "hard_cost_cap"),
        ("cost_variance_10_01_percent", "cost_variance"),
        ("missing_task_cost", "task_cost_lineage"),
        ("duplicate_charge", "duplicate_charge"),
        ("missing_exact_sigkill", "exact_crash_window"),
        ("request_log_incomplete", "supplier_request_log"),
        ("statement_not_final", "final_supplier_statement"),
        ("unmatched_statement_line", "supplier_request_set"),
        ("same_reviewer", "independent_review"),
    ],
)
def test_each_provider_poc_failure_is_fail_closed(
    governance_rig,
    mutation: str,
    failed_gate: str,
) -> None:
    case = governance_rig.provider_release_case(BetaCapability.VOICE_CLONE)
    result = case.evaluator.evaluate(case.passing_measurement().mutate(mutation))
    assert not result.passed
    assert failed_gate in result.failed_gates
    assert case.stage() == "internal_poc"


@pytest.mark.parametrize("capability", list(BetaCapability))
def test_no_official_identity_and_no_statement_reconciliation_stays_internal_poc(
    governance_rig,
    capability: BetaCapability,
) -> None:
    case = governance_rig.provider_release_case(capability)
    measurement = case.passing_measurement().without_official_recovery_paths()
    result = case.evaluator.evaluate(measurement)
    assert not result.passed
    assert "recoverable_request_identity" in result.failed_gates
    assert case.stage() == "internal_poc"
~~~

Create backend/tests/integration/governance/test_provider_release_composition.py:

~~~python
import pytest

from ip_saas.common.errors import Conflict, Forbidden
from ip_saas.modules.governance.enums import BetaCapability


@pytest.mark.parametrize("capability", list(BetaCapability))
def test_normal_worker_cannot_build_real_provider_in_internal_poc(
    governance_rig,
    capability: BetaCapability,
) -> None:
    case = governance_rig.provider_release_case(capability)
    with pytest.raises(Forbidden, match="production provider is not released"):
        case.build_production_provider()
    assert case.secret_read_count() == 0
    assert case.http_client_build_count() == 0


def test_poc_adapter_is_only_available_to_explicit_provider_poc_composition(
    governance_rig,
) -> None:
    case = governance_rig.provider_release_case(BetaCapability.VOICE_CLONE)
    adapter = case.build_provider_poc_adapter(change_ticket_id=case.change_ticket_id)
    assert adapter.capability is BetaCapability.VOICE_CLONE
    with pytest.raises(Forbidden, match="provider PoC composition"):
        case.build_provider_poc_adapter_from_normal_worker()


@pytest.mark.parametrize("capability", list(BetaCapability))
def test_hash_mismatch_or_wrong_capability_blocks_before_secret_or_http(
    governance_rig,
    capability: BetaCapability,
) -> None:
    case = governance_rig.provider_release_case(capability)
    case.install_passing_crash_attestation()
    case.promote_closed_beta()
    case.corrupt_attested_tos_object()
    with pytest.raises(Conflict, match="evidence hash"):
        case.build_production_provider()
    assert case.secret_read_count() == 0
    assert case.http_client_build_count() == 0

    case.restore_attested_tos_object()
    with pytest.raises(Forbidden, match="capability evidence mismatch"):
        case.build_production_provider(
            requested_capability=case.sibling_capability()
        )
    assert case.secret_read_count() == 0
    assert case.http_client_build_count() == 0


def test_linked_supplier_statement_hash_mismatch_is_fail_closed(
    governance_rig,
) -> None:
    case = governance_rig.provider_release_case(
        BetaCapability.TRADITIONAL_DIGITAL_HUMAN
    )
    case.install_passing_crash_attestation()
    case.promote_closed_beta()
    case.corrupt_linked_final_statement_object()
    with pytest.raises(Conflict, match="linked provider evidence hash"):
        case.build_production_provider()
    assert case.secret_read_count() == 0
    assert case.http_client_build_count() == 0


@pytest.mark.parametrize("capability", list(BetaCapability))
def test_exact_hash_pinned_attestation_unlocks_only_exact_release_tuple(
    governance_rig,
    capability: BetaCapability,
) -> None:
    case = governance_rig.provider_release_case(capability)
    case.install_passing_crash_attestation()
    case.promote_closed_beta()
    adapter = case.build_production_provider()
    assert adapter.capability is capability
    assert case.secret_read_count() == 1
    assert case.http_client_build_count() == 1
    with pytest.raises(Forbidden, match="model evidence mismatch"):
        case.build_production_provider(model_version="unattested-version")
~~~

Extend `GovernanceTestRig.provider_release_case(capability)` with the methods used in these two files. It uses private in-memory versioned object storage, secret/HTTP counting spies, one exact entitlement/account/region/model tuple, and the strict evidence models. It never creates a network client unless the production factory passes every check. The existing `governance_rig` fixture and the parametrized `capability`, `mutation`, and `failed_gate` names close every custom argument.

Run:

~~~bash
cd backend && env -u ARK_API_KEY -u DOUBAO_SPEECH_ACCESS_TOKEN uv run pytest tests/integration/governance/test_provider_poc_evaluator.py tests/integration/governance/test_provider_release_composition.py -v
~~~

Expected: FAIL because the immutable evaluator and production/PoC composition split do not exist. Secret and HTTP spy counts remain zero in every rejected case.

- [ ] **Step 13: Implement immutable PoC evaluation and hash-pinned release composition**

Append to backend/src/ip_saas/modules/governance/provider_crash.py:

~~~python
from decimal import Decimal


class CapabilityPocMeasurement(StrictProviderEvidence):
    capability: BetaCapability
    submitted: int = Field(ge=0)
    completed: int = Field(ge=0)
    max_cost_fen: int = Field(gt=0)
    settled_amount_fen: int = Field(ge=0)
    absolute_cost_variance: Decimal = Field(ge=0)
    provider_completions: int = Field(ge=0)
    provider_cost_rows_with_task_id: int = Field(ge=0)
    provider_cost_task_mismatch: int = Field(ge=0)
    duplicate_charge: int = Field(ge=0)
    unauthorized_delivery: int = Field(ge=0)
    label_loss: int = Field(ge=0)
    exact_sigkill_observed: bool
    result_absent_before_kill: bool
    cost_absent_before_kill: bool
    request_log_complete: bool
    final_statement_final: bool
    unmatched_supplier_lines: int = Field(ge=0)
    recovery_proof: RecoveryProof | None
    observed_identity_proof_sha256: Sha256Hex | None
    reconciler_implementation_sha256: Sha256Hex | None
    operator_id: UUID
    independent_reviewer_id: UUID

    def mutate(self, name: str) -> "CapabilityPocMeasurement":
        changes: dict[str, object] = {
            "submitted_29": {"submitted": 29, "completed": 29},
            "completion_94_99_percent": {"submitted": 10000, "completed": 9499},
            "cost_over_cap": {"settled_amount_fen": self.max_cost_fen + 1},
            "cost_variance_10_01_percent": {
                "absolute_cost_variance": Decimal("0.1001")
            },
            "missing_task_cost": {
                "provider_cost_rows_with_task_id": self.provider_completions - 1
            },
            "duplicate_charge": {"duplicate_charge": 1},
            "missing_exact_sigkill": {"exact_sigkill_observed": False},
            "request_log_incomplete": {"request_log_complete": False},
            "statement_not_final": {"final_statement_final": False},
            "unmatched_statement_line": {"unmatched_supplier_lines": 1},
            "same_reviewer": {"independent_reviewer_id": self.operator_id},
        }[name]
        return self.model_copy(update=changes)

    def without_official_recovery_paths(self) -> "CapabilityPocMeasurement":
        return self.model_copy(update={
            "recovery_proof": None,
            "observed_identity_proof_sha256": None,
            "reconciler_implementation_sha256": None,
            "final_statement_final": False,
        })


class ProviderPocEvaluation(StrictProviderEvidence):
    passed: bool
    failed_gates: tuple[str, ...]


class ProviderPocEvaluator:
    def evaluate(self, value: CapabilityPocMeasurement) -> ProviderPocEvaluation:
        failed: list[str] = []
        if value.submitted < 30:
            failed.append("batch_size")
        if value.submitted == 0 or Decimal(value.completed) / Decimal(value.submitted) < Decimal("0.95"):
            failed.append("completion_rate")
        if value.settled_amount_fen > value.max_cost_fen:
            failed.append("hard_cost_cap")
        if value.absolute_cost_variance > Decimal("0.10"):
            failed.append("cost_variance")
        if (
            value.provider_completions != value.provider_cost_rows_with_task_id
            or value.provider_cost_task_mismatch != 0
        ):
            failed.append("task_cost_lineage")
        if value.duplicate_charge != 0:
            failed.append("duplicate_charge")
        if not (
            value.exact_sigkill_observed
            and value.result_absent_before_kill
            and value.cost_absent_before_kill
        ):
            failed.append("exact_crash_window")
        if not value.request_log_complete:
            failed.append("supplier_request_log")
        if not value.final_statement_final:
            failed.append("final_supplier_statement")
        if value.unmatched_supplier_lines != 0:
            failed.append("supplier_request_set")
        if value.operator_id == value.independent_reviewer_id:
            failed.append("independent_review")
        if value.unauthorized_delivery != 0 or value.label_loss != 0:
            failed.append("zero_tolerance")
        official_recovery = (
            value.recovery_proof
            in {RecoveryProof.OFFICIAL_QUERY, RecoveryProof.OFFICIAL_IDEMPOTENCY}
            and value.observed_identity_proof_sha256 is not None
        )
        statement_recovery = (
            value.recovery_proof is RecoveryProof.SUPPLIER_STATEMENT
            and value.request_log_complete
            and value.final_statement_final
            and value.reconciler_implementation_sha256 is not None
            and value.unmatched_supplier_lines == 0
        )
        if not (official_recovery or statement_recovery):
            failed.append("recoverable_request_identity")
        return ProviderPocEvaluation(
            passed=not failed,
            failed_gates=tuple(dict.fromkeys(failed)),
        )
~~~

Create backend/src/ip_saas/providers/digital_identity/composition.py. `ProductionDigitalIdentityProviderFactory.build()` must execute these checks in this order: target feature stage is CLOSED_BETA or PUBLIC_BETA; entitlement is current and exact; current PASS `PROVIDER_CRASH_RECONCILIATION` attestation names that entitlement and is unexpired; its `MediaAsset` is private/versioned; re-downloaded bytes hash to `evidence_sha256`; `CapabilityProviderCrashEvidenceV1.model_validate_json()` accepts the bytes; capability, entitlement, provider-account fingerprint, region, model ID/version, run and provider exactly equal the requested tuple; the evidence's supplier/request/cost/crash invariants pass; and a supplier-statement proof carries the SHA-256 of the reconciler bytes in the running release image. Only after all checks may it read a secret, create an HTTP client, or construct one of the three real adapters.

Implement the core as:

~~~python
from collections.abc import Callable
from datetime import datetime
from hashlib import sha256
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.errors import Conflict, Forbidden
from ip_saas.modules.governance.enums import (
    BetaCapability,
    CapabilityStage,
    GateKind,
)
from ip_saas.modules.governance.models import (
    CapabilityFeatureFlag,
    CapabilityGateAttestation,
    ProviderCapabilityEntitlement,
)
from ip_saas.modules.governance.eligibility import CapabilityEligibilityService
from ip_saas.modules.governance.provider_crash import (
    CapabilityProviderCrashEvidenceV1,
    CapabilitySupplierReconciler,
    RecoveryProof,
)

from .base import DigitalIdentityProvider


class ProductionDigitalIdentityProviderFactory:
    def __init__(
        self,
        *,
        now: Callable[[], datetime],
        read_private_evidence: Callable[[UUID], bytes],
        read_provider_secret: Callable[[ProviderCapabilityEntitlement], object],
        build_adapter: Callable[
            [BetaCapability, ProviderCapabilityEntitlement, object],
            DigitalIdentityProvider,
        ],
        reconciler_implementation_sha256: str,
        deployed_image_digest: str,
    ) -> None:
        self.now = now
        self.read_private_evidence = read_private_evidence
        self.read_provider_secret = read_provider_secret
        self.build_adapter = build_adapter
        self.reconciler_implementation_sha256 = reconciler_implementation_sha256
        self.deployed_image_digest = deployed_image_digest

    def build(
        self,
        session: Session,
        *,
        capability: BetaCapability,
        entitlement_id: UUID,
        model_id: str,
        model_version: str,
    ) -> DigitalIdentityProvider:
        flag = session.scalar(select(CapabilityFeatureFlag).where(
            CapabilityFeatureFlag.capability == capability.value
        ))
        if flag is None or CapabilityStage(flag.stage) not in {
            CapabilityStage.CLOSED_BETA,
            CapabilityStage.PUBLIC_BETA,
        }:
            raise Forbidden("production provider is not released")
        check_at = self.now()
        entitlement = CapabilityEligibilityService.require_current_entitlement(
            session, capability, now=check_at
        )
        if entitlement.id != entitlement_id:
            raise Forbidden("provider entitlement is not current and exact")
        attestation = session.scalar(
            select(CapabilityGateAttestation)
            .where(
                CapabilityGateAttestation.capability == capability.value,
                CapabilityGateAttestation.gate_kind
                == GateKind.PROVIDER_CRASH_RECONCILIATION.value,
                CapabilityGateAttestation.result == "pass",
                CapabilityGateAttestation.entitlement_id == entitlement.id,
                CapabilityGateAttestation.provider_account_fingerprint
                == entitlement.provider_account_fingerprint,
                CapabilityGateAttestation.region == entitlement.region,
                CapabilityGateAttestation.model_id == entitlement.model_id,
                CapabilityGateAttestation.model_version
                == entitlement.model_version,
                CapabilityGateAttestation.release_snapshot_sha256
                == entitlement.release_snapshot_sha256,
                CapabilityGateAttestation.valid_until > check_at,
            )
            .order_by(CapabilityGateAttestation.executed_at.desc())
            .limit(1)
        )
        if attestation is None:
            raise Forbidden("current provider crash attestation is required")
        body = self.read_private_evidence(attestation.evidence_asset_id)
        if sha256(body).hexdigest() != attestation.evidence_sha256:
            raise Conflict("provider crash evidence hash mismatch")
        evidence = CapabilityProviderCrashEvidenceV1.model_validate_json(body)
        if (
            evidence.capability is not capability
            or evidence.entitlement_id != entitlement.id
            or evidence.provider_account_fingerprint
            != entitlement.provider_account_fingerprint
            or evidence.region != entitlement.region
            or evidence.model_id != entitlement.model_id
            or evidence.model_version != entitlement.model_version
            or evidence.release_snapshot_sha256
            != entitlement.release_snapshot_sha256
        ):
            raise Forbidden("capability evidence mismatch")
        if evidence.model_id != model_id or evidence.model_version != model_version:
            raise Forbidden("model evidence mismatch")
        if evidence.deployed_image_digest != self.deployed_image_digest:
            raise Forbidden("release image differs from attested evidence")
        linked_evidence = (
            (evidence.request_log_evidence_asset_id, evidence.request_log_sha256),
            (
                evidence.final_statement_evidence_asset_id,
                evidence.final_statement_sha256,
            ),
            (
                evidence.reconciliation_report_evidence_asset_id,
                evidence.reconciliation_report_sha256,
            ),
            (
                evidence.attempt_journal_evidence_asset_id,
                evidence.attempt_journal_sha256,
            ),
            (
                evidence.sigkill_transcript_evidence_asset_id,
                evidence.sigkill_transcript_sha256,
            ),
        )
        linked_bodies: list[bytes] = []
        for asset_id, expected_hash in linked_evidence:
            linked_body = self.read_private_evidence(asset_id)
            if sha256(linked_body).hexdigest() != expected_hash:
                raise Conflict("linked provider evidence hash mismatch")
            linked_bodies.append(linked_body)
        CapabilitySupplierReconciler._parse_exports(
            linked_bodies[0], linked_bodies[1]
        )
        if (
            evidence.recovery_proof is RecoveryProof.SUPPLIER_STATEMENT
            and evidence.reconciler_implementation_sha256
            != self.reconciler_implementation_sha256
        ):
            raise Forbidden("deployed reconciler differs from attested evidence")
        secret = self.read_provider_secret(entitlement)
        return self.build_adapter(capability, entitlement, secret)
~~~

The same module exposes a separate `ProviderPocDigitalIdentityProviderFactory` only to `ip_saas.scripts.provider_poc`. Its `build()` requires stage INTERNAL_POC, the exact current entitlement, an approved non-empty production change-ticket UUID, the real `GenerationLimitService`, and the hard capability budget. API and ordinary Worker composition do not receive this factory. It cannot produce a normal production dependency, set a feature flag, write a gate attestation, or bypass the supplier reconciliation step. Unit and integration composition use counting adapter builders; no credential is read and no paid request is possible.

- [ ] **Step 14: Implement the three hard-budget exercise/reconcile commands and runbook**

Create backend/src/ip_saas/scripts/provider_poc.py with two explicit subcommands. Freeze the capability policy in code:

~~~python
from dataclasses import dataclass

from ip_saas.modules.governance.enums import BetaCapability


@dataclass(frozen=True)
class PocCapabilityPolicy:
    maximum_cost_fen: int
    minimum_items: int = 30


POC_CAPABILITY_POLICY = {
    BetaCapability.VOICE_CLONE: PocCapabilityPolicy(maximum_cost_fen=30_000),
    BetaCapability.SEEDANCE_AUTHORIZED_PORTRAIT: PocCapabilityPolicy(
        maximum_cost_fen=60_000
    ),
    BetaCapability.TRADITIONAL_DIGITAL_HUMAN: PocCapabilityPolicy(
        maximum_cost_fen=90_000
    ),
}
~~~

`exercise` accepts exactly `--environment production`, `--capability`, `--project-id`, `--cost-center-id`, `--entitlement-id`, `--change-ticket-id`, and `--evidence-directory`. It verifies the isolated internal project/cost center, effective real `GenerationLimitVersion`, INTERNAL_POC stage, current entitlement/account/region/model, private directory permissions, and empty evidence directory before reading secrets. It reserves no more than the frozen capability cap and refuses a request when the maximum quoted next call exceeds the remaining hold. It runs at least 30 real items and records every stable attempt ID and sanitized provider request ID. For the exact crash item, the parent process launches a one-task child: the child commits the attempt and send marker, obtains the first paid terminal response/usage, appends and fsyncs the minimal checkpoint transcript, then receives `SIGKILL` before provider result, ProviderCostEntry, domain result, event, or delivery persistence. The parent requires signal 9, verifies the exact database absence, drives only zero-call lease retries until the task is `RECONCILIATION_REQUIRED`, verifies the hold remains active, exports the attempt journal, and exits 0 without creating a gate attestation.

`reconcile` accepts the same identity arguments plus `--supplier-request-log`, `--final-supplier-statement`, `--operator-id`, `--reviewer-id`, and `--evidence-tos-prefix`. It requires different reviewers, exact exercise manifest hashes, strict official complete request-log bytes, a finalized statement for the same run/account, and an unchanged deployed image/reconciler hash. It invokes `CapabilitySupplierReconciler` once; that service groups lines by task and calls Plan 01 `TaskReconciliationService.finalize_provider_costs` once per `RECONCILIATION_REQUIRED` task. It then evaluates the ≥30 batch, exact crash, ≤10% absolute cost variance, ≤hard cap, ≥95% completion, zero duplicate/unauthorized/label-loss/task-mismatch, and complete supplier mapping. Only a pass writes private versioned TOS evidence and the hash-pinned PASS attestation. A missing/non-final supplier statement exits 75; a capability with neither proven official lookup/idempotency nor implementable complete statement reconciliation exits 78. Both outcomes leave the stage INTERNAL_POC, hold unresolved when evidence is incomplete, and production composition disabled.

The CLI never accepts API keys or access tokens as command-line flags, never prints a provider resource reference or signed URL, and never interprets a local DB anti-join as proof of no supplier charge. Its live subprocess path is unreachable from imports and test composition; `main()` requires `--environment production`, the approved ticket, and a real entitlement. Ordinary tests exercise only `ProviderPocEvaluator`, deterministic fakes, and counting composition spies.

Add these targets to Makefile:

~~~makefile
POC_COMMON_ARGS = --environment production \
	--project-id "$(POC_PROJECT_ID)" \
	--cost-center-id "$(POC_COST_CENTER_ID)" \
	--entitlement-id "$(POC_ENTITLEMENT_ID)" \
	--change-ticket-id "$(POC_CHANGE_TICKET_ID)" \
	--evidence-directory "$(POC_EVIDENCE_DIRECTORY)"

define require-provider-poc-input
	@test -n "$(POC_PROJECT_ID)" -a -n "$(POC_COST_CENTER_ID)" \
		-a -n "$(POC_ENTITLEMENT_ID)" -a -n "$(POC_CHANGE_TICKET_ID)" \
		-a -n "$(POC_EVIDENCE_DIRECTORY)"
	@test "$(POC_ACTION)" = exercise -o "$(POC_ACTION)" = reconcile
	@if test "$(POC_ACTION)" = reconcile; then \
		test -n "$(POC_SUPPLIER_REQUEST_LOG)" \
			-a -n "$(POC_FINAL_SUPPLIER_STATEMENT)" \
			-a -n "$(POC_OPERATOR_ID)" -a -n "$(POC_REVIEWER_ID)" \
			-a -n "$(POC_EVIDENCE_TOS_PREFIX)"; \
	fi
endef

define run-provider-poc
	$(call require-provider-poc-input)
	cd backend && uv run python -m ip_saas.scripts.provider_poc $(POC_ACTION) \
		--capability "$(1)" $(POC_COMMON_ARGS) \
		$(if $(filter reconcile,$(POC_ACTION)),--supplier-request-log "$(POC_SUPPLIER_REQUEST_LOG)" --final-supplier-statement "$(POC_FINAL_SUPPLIER_STATEMENT)" --operator-id "$(POC_OPERATOR_ID)" --reviewer-id "$(POC_REVIEWER_ID)" --evidence-tos-prefix "$(POC_EVIDENCE_TOS_PREFIX)",)
endef

.PHONY: poc-voice-clone poc-seedance-portrait poc-traditional-avatar
poc-voice-clone:
	$(call run-provider-poc,voice_clone)

poc-seedance-portrait:
	$(call run-provider-poc,seedance_authorized_portrait)

poc-traditional-avatar:
	$(call run-provider-poc,traditional_digital_human)
~~~

Create infra/runbooks/provider-poc.md with: named operator/reviewer/change approver; account/project/cost-center/entitlement preflight; immutable model/version and price source; KMS/TOS/secret-manager checks; per-capability cap; one capability per evidence directory; exercise and reconcile commands; expected child SIGKILL and parent verification; how to obtain the provider's complete request log and finalized statement; exits 75/78; private evidence upload/hash verification; internal-hold reconciliation; incident/over-cap stop; and rollback to OFF. The runbook states that a console screenshot, current DB anti-join, provider request header alone, fake run, or a statement covering only selected request IDs cannot pass.

- [ ] **Step 15: Run non-paid tests, then execute each approved capability exercise/reconcile pair**

First run the complete no-credential Task 6 suite:

~~~bash
cd backend && env -u ARK_API_KEY -u DOUBAO_SPEECH_ACCESS_TOKEN uv run pytest tests/contract/governance/test_provider_adapters.py tests/integration/governance/test_provider_crash_reconciliation.py tests/integration/governance/test_provider_poc_evaluator.py tests/integration/governance/test_provider_release_composition.py -v
cd backend && uv run pytest tests/integration/governance/test_migration.py -v
git diff --check
~~~

Expected: PASS. The suite makes zero network calls, reads zero provider credentials, proves three capabilities, exact crash window, multi-line one-task batching, customer-zero/internal-CNY settlement, durable reconciliation state, hold retention, zero-call RETRY, stale fencing, evidence-hash rejection, and production fail-closed composition.

The following six commands are paid production-evidence operations. Execute them only inside the approved change window after exporting the runbook-required `POC_*` values. Each exercise command uses its code-frozen maximum and exits 0 only after the parent has verified the post-paid-response/pre-persistence SIGKILL checkpoint; it does not enable production. Run the matching reconcile command only after the official request log and final statement for that exercise are finalized:

~~~bash
POC_ACTION=exercise make poc-voice-clone
POC_ACTION=reconcile make poc-voice-clone
POC_ACTION=exercise make poc-seedance-portrait
POC_ACTION=reconcile make poc-seedance-portrait
POC_ACTION=exercise make poc-traditional-avatar
POC_ACTION=reconcile make poc-traditional-avatar
~~~

Expected for each reconcile command: exit 0, one private hash-verified `CapabilityProviderCrashEvidenceV1`, one PASS `PROVIDER_CRASH_RECONCILIATION` attestation for the exact capability/entitlement/account/region/model/release hash, complete `ProviderCostEntry.task_id` coverage, and no unresolved hold. Exit 75 or 78 is a successful fail-closed outcome, not permission to promote: retain INTERNAL_POC and resolve the supplier evidence path before rerunning.

- [ ] **Step 16: Commit the independent provider boundaries and evidence gate**

~~~bash
git add backend/migrations/versions/0006_governance.py backend/src/ip_saas/modules/governance/enums.py backend/src/ip_saas/modules/governance/models.py backend/src/ip_saas/modules/governance/provider_crash.py backend/src/ip_saas/providers/digital_identity backend/src/ip_saas/providers/volcengine/voice_clone.py backend/src/ip_saas/providers/volcengine/seedance_portrait.py backend/src/ip_saas/providers/volcengine/cloned_avatar.py backend/src/ip_saas/scripts/provider_poc.py backend/tests/contract/governance/test_provider_adapters.py backend/tests/integration/governance/test_migration.py backend/tests/integration/governance/test_provider_crash_reconciliation.py backend/tests/integration/governance/test_provider_poc_evaluator.py backend/tests/integration/governance/test_provider_release_composition.py backend/tests/support/governance_rig.py infra/runbooks/provider-poc.md Makefile
git commit -m "feat: gate digital identity providers with supplier reconciliation"
~~~

### Task 7: Submit enrollment and governed generation atomically

**Files:**
- Modify: backend/src/ip_saas/modules/media/enums.py
- Modify: backend/src/ip_saas/modules/media/rights.py
- Create: backend/src/ip_saas/modules/governance/enrollment.py
- Create: backend/src/ip_saas/modules/governance/generation.py
- Create: backend/src/ip_saas/workers/identity_enrollment.py
- Modify: backend/src/ip_saas/workers/media_submit.py
- Modify: backend/src/ip_saas/workers/media_copy.py
- Test: backend/tests/integration/governance/test_enrollment_submission.py
- Test: backend/tests/integration/governance/test_governed_generation.py

- [ ] **Step 1: Write failing atomic-enrollment tests for customer and internal projects**

Create backend/tests/integration/governance/test_enrollment_submission.py with these cases:

~~~python
from uuid import uuid4

import pytest

from ip_saas.common.errors import Conflict, Forbidden, NotFound


def test_customer_enrollment_reserves_credit_quota_and_consent_in_one_transaction(
    db_session, customer_actor, customer_enrollment_command, enrollment_service,
    enrollment_quote, customer_billing_route, customer_project,
) -> None:
    with db_session.begin():
        enrollment = enrollment_service.create(
            db_session, customer_actor, customer_project.id, customer_enrollment_command,
            enrollment_quote, customer_billing_route,
        )
    assert enrollment.task_record.billing_mode == "customer_credit"
    assert enrollment.identity_asset.status == "enrolling"
    assert enrollment.quota_reservation.task_record_id == enrollment.task_record.id
    assert len(enrollment.task_record.request_fingerprint) == 64
    assert enrollment.consent_snapshot_sha256s
    assert enrollment.enrollment.rights_snapshot_sha256s
    assert enrollment.outbox_event_type == "identity.enrollment.requested"


def test_platform_enrollment_uses_internal_budget_not_customer_credit(
    db_session, platform_operator, platform_enrollment_command, enrollment_service,
    enrollment_quote, internal_billing_route, platform_project,
) -> None:
    with db_session.begin():
        result = enrollment_service.create(
            db_session, platform_operator, platform_project.id, platform_enrollment_command,
            enrollment_quote, internal_billing_route,
        )
    assert result.task_record.billing_mode == "internal_cost"


def test_failed_outbox_rolls_back_hold_quota_consent_and_enrollment(
    transactional_session, enrollment_service_with_failing_outbox,
    customer_actor, customer_enrollment_command, enrollment_quote,
    customer_billing_route, customer_project,
) -> None:
    with pytest.raises(RuntimeError, match="outbox unavailable"):
        with transactional_session.begin():
            enrollment_service_with_failing_outbox.create(
                transactional_session, customer_actor, customer_project.id,
                customer_enrollment_command,
                enrollment_quote, customer_billing_route,
            )
    assert ledger_probe(transactional_session).new_holds == 0
    assert governance_probe(transactional_session).new_rows == 0


def test_same_key_same_payload_returns_same_enrollment_but_changed_payload_conflicts(
    enrollment_factory,
) -> None:
    first = enrollment_factory.submit()
    before_replay = enrollment_factory.durable_side_effect_counts()
    assert enrollment_factory.submit().id == first.id
    assert enrollment_factory.durable_side_effect_counts() == before_replay
    event = enrollment_factory.single_event("identity.enrollment.requested")
    assert event.idempotency_key == (
        f"identity-enrollment:{first.id}:requested"
    )
    assert event.aggregate_id == first.id
    assert event.payload["enrollment_id"] == str(first.id)
    with pytest.raises(Conflict, match="different enrollment input"):
        enrollment_factory.submit(source_media_asset_ids=[uuid4()])
    with pytest.raises(Conflict, match="different enrollment input"):
        enrollment_factory.submit(approved_credit_units=2)


def test_enrollment_idempotency_is_account_scoped_and_never_leaks_projects(
    enrollment_factory,
) -> None:
    account_a = enrollment_factory.for_account("account-a")
    account_b = enrollment_factory.for_account("account-b")
    first = account_a.submit(idempotency_key="shared-enrollment-key")
    second = account_b.submit(idempotency_key="shared-enrollment-key")
    assert first.id != second.id
    with pytest.raises(Conflict, match="already used in this account"):
        account_a.for_second_project().submit(
            idempotency_key="shared-enrollment-key"
        )
    with pytest.raises((Forbidden, NotFound)):
        account_b.as_actor().submit_to(
            account_a.project_id,
            idempotency_key="shared-enrollment-key",
        )
    assert account_b.foreign_existing_aggregate_was_returned is False


def test_source_rights_are_separate_complete_and_current(enrollment_factory) -> None:
    with pytest.raises(Forbidden, match="source rights"):
        enrollment_factory.voice_clone(grants=[]).submit()
    with pytest.raises(Forbidden, match="source rights"):
        enrollment_factory.traditional_avatar(grant_types={"portrait"}).submit()
    created = enrollment_factory.traditional_avatar(
        grant_types={"portrait", "voice"}
    ).submit()
    assert len(created.enrollment.rights_snapshot_sha256s) >= 2


def test_real_enrollment_cost_links_exact_task(enrollment_factory) -> None:
    completed = enrollment_factory.voice_clone().complete_real_provider()
    assert completed.provider_cost.task_id == completed.task_record.id
    assert completed.provider_cost_row_count == 1
~~~

- [ ] **Step 2: Run enrollment tests and verify the service is missing**

Run:

~~~bash
cd backend && uv run pytest tests/integration/governance/test_enrollment_submission.py -v
~~~

Expected: FAIL during collection because governance.enrollment does not exist.

- [ ] **Step 3: Extend Media Capability and the existing source-rights policy without changing VoiceKind semantics**

Append to Capability in backend/src/ip_saas/modules/media/enums.py:

~~~python
VOICE_CLONE = "voice_clone"
AUTHORIZED_PORTRAIT_VIDEO = "authorized_portrait_video"
CLONED_AVATAR_VIDEO = "cloned_avatar_video"
~~~

Do not change `VoiceKind.UPLOADED_RECORDING`. A cloned voice remains ProviderIdentityAsset(kind=voice_clone); it is not an uploaded recording.

In backend/src/ip_saas/modules/media/rights.py, retain `require_shot_rights` unchanged and add this Plan 06-specific method plus canonical evidence hash:

~~~diff
 from collections.abc import Iterable
+from hashlib import sha256
+from json import dumps
+from uuid import UUID

 class RightsPolicy:
+    def require_identity_source_rights(
+        self,
+        *,
+        source_asset_ids: Iterable[UUID],
+        grants: Iterable[MediaRightsGrant],
+        required_types: frozenset[RightsType],
+        platform: str | None,
+        territory: str = "CN",
+    ) -> list[MediaRightsGrant]:
+        source_ids = set(source_asset_ids)
+        if not source_ids and not required_types:
+            return []
+        now = self.clock.now()
+        selected = [
+            grant
+            for grant in grants
+            if grant.subject_id in source_ids
+            and grant.rights_type in {item.value for item in required_types}
+            and grant.revoked_at is None
+            and grant.valid_from <= now < grant.valid_until
+            and grant.commercial_use
+            and grant.ai_processing_allowed
+            and grant.provider_transfer_allowed
+            and grant.territory == territory
+            and grant.evidence_asset_id is not None
+            and bool(grant.platform_scopes)
+            and (platform is None or platform in grant.platform_scopes)
+        ]
+        uncovered = source_ids - {grant.subject_id for grant in selected}
+        missing_types = required_types - {
+            RightsType(grant.rights_type) for grant in selected
+        }
+        if uncovered or missing_types:
+            raise Forbidden(
+                "active commercial AI/provider-transfer source rights are required"
+            )
+        return sorted(selected, key=lambda grant: str(grant.id))


+def rights_grant_snapshot_sha256(grant: MediaRightsGrant) -> str:
+    payload = {
+        "id": str(grant.id),
+        "account_id": str(grant.account_id),
+        "project_id": str(grant.project_id),
+        "rights_type": grant.rights_type,
+        "subject_id": str(grant.subject_id),
+        "grantor_name": grant.grantor_name,
+        "platform_scopes": sorted(grant.platform_scopes),
+        "territory": grant.territory,
+        "valid_from": grant.valid_from.isoformat(),
+        "valid_until": grant.valid_until.isoformat(),
+        "commercial_use": grant.commercial_use,
+        "ai_processing_allowed": grant.ai_processing_allowed,
+        "provider_transfer_allowed": grant.provider_transfer_allowed,
+        "guardian_verified": grant.guardian_verified,
+        "evidence_asset_id": str(grant.evidence_asset_id),
+        "revoked_at": grant.revoked_at.isoformat() if grant.revoked_at else None,
+    }
+    body = dumps(payload, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
+    return sha256(body.encode("utf-8")).hexdigest()
~~~

`MediaRightsGrant.subject_id` denotes the governed source object's local ID for this method. Voice clone requires a voice grant for every supplied voice source. Traditional avatar requires at least one portrait grant, at least one voice grant, and coverage for every supplied source. Seedance verified portrait has no local source list, so its provider-verification receipt plus the separate portrait and sensitive-information consents are the authorization boundary. Generation repeats the rights check for the exact target platform; enrollment alone never grants a broader platform scope.

- [ ] **Step 4: Implement idempotent enrollment creation over Plan 01 task submission**

Create backend/src/ip_saas/modules/governance/enrollment.py:

~~~python
from dataclasses import dataclass
from uuid import UUID, uuid4

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.common.errors import Conflict, Forbidden, NotFound
from ip_saas.common.outbox import OutboxWriter
from ip_saas.modules.accounts.context import ActorContext
from ip_saas.modules.billing.service import BillingMode
from ip_saas.modules.media.enums import RightsType
from ip_saas.modules.media.models.identity import MediaRightsGrant
from ip_saas.modules.media.models.jobs import MediaAsset
from ip_saas.modules.media.pricing import BillingRoute, MediaQuote
from ip_saas.modules.media.rights import RightsPolicy, rights_grant_snapshot_sha256
from ip_saas.modules.projects.access import ProjectAccessService
from ip_saas.modules.tasks.models import TaskRecord
from ip_saas.modules.tasks.service import TaskSubmissionService

from .consent import ConsentService
from .eligibility import (
    CapabilityEligibilityService,
    CapabilityQuotaService,
    lock_account_idempotency,
    require_governance_project,
)
from .enums import BetaCapability, EnrollmentStatus, IdentityAssetStatus, IdentityKind
from .events import event_envelope
from .models import (
    CapabilityQuotaReservation,
    IdentityAssetConsent,
    IdentityEnrollment,
    ProviderIdentityAsset,
)
from .schemas import IdentityEnrollmentCreate


CAPABILITY_BY_IDENTITY = {
    IdentityKind.VOICE_CLONE: BetaCapability.VOICE_CLONE,
    IdentityKind.SEEDANCE_PORTRAIT: BetaCapability.SEEDANCE_AUTHORIZED_PORTRAIT,
    IdentityKind.TRADITIONAL_AVATAR: BetaCapability.TRADITIONAL_DIGITAL_HUMAN,
}
RIGHTS_BY_IDENTITY = {
    IdentityKind.VOICE_CLONE: frozenset({RightsType.VOICE}),
    IdentityKind.SEEDANCE_PORTRAIT: frozenset(),
    IdentityKind.TRADITIONAL_AVATAR: frozenset(
        {RightsType.PORTRAIT, RightsType.VOICE}
    ),
}


@dataclass(frozen=True)
class EnrollmentCreated:
    enrollment: IdentityEnrollment
    identity_asset: ProviderIdentityAsset
    task_record: TaskRecord
    quota_reservation: CapabilityQuotaReservation
    consent_snapshot_sha256s: tuple[str, ...]
    outbox_event_type: str


class EnrollmentService:
    def __init__(self, access: ProjectAccessService,
                 eligibility: CapabilityEligibilityService,
                 quota: CapabilityQuotaService, consents: ConsentService,
                 rights: RightsPolicy,
                 tasks: TaskSubmissionService, audit: AuditWriter,
                 outbox: OutboxWriter, clock) -> None:
        self.access = access
        self.eligibility = eligibility
        self.quota = quota
        self.consents = consents
        self.rights = rights
        self.tasks = tasks
        self.audit = audit
        self.outbox = outbox
        self.clock = clock

    def create(self, session: Session, actor: ActorContext, project_id: UUID,
               command: IdentityEnrollmentCreate, quote: MediaQuote,
               billing_route: BillingRoute) -> EnrollmentCreated:
        project = require_governance_project(
            self.access, session, actor, project_id
        )
        lock_account_idempotency(
            session, actor.account_id, "identity-enrollment",
            command.idempotency_key,
        )
        existing = session.scalar(select(IdentityEnrollment).where(
            IdentityEnrollment.account_id == actor.account_id,
            IdentityEnrollment.project_id == project.id,
            IdentityEnrollment.idempotency_key == command.idempotency_key
        ))
        if existing is not None:
            created = self._created(session, actor, project, existing)
            if (existing.subject_id != command.subject_id
                    or existing.identity_kind != command.identity_kind.value
                    or created.task_record.input_payload != self._input_payload(command)
                    or created.task_record.request_fingerprint
                    != self._expected_fingerprint(
                        actor, project.id, command, quote, billing_route
                    )):
                raise Conflict("idempotency key has different enrollment input")
            return created
        used_in_another_project = session.scalar(
            select(IdentityEnrollment.id).where(
                IdentityEnrollment.account_id == actor.account_id,
                IdentityEnrollment.idempotency_key == command.idempotency_key,
            ).limit(1)
        )
        if used_in_another_project is not None:
            raise Conflict("idempotency key is already used in this account")

        capability = CAPABILITY_BY_IDENTITY[command.identity_kind]
        entitlement = self.eligibility.require_project_capability(
            session, actor, project_id, capability
        )
        subject = self.eligibility.require_identity_subject(
            session, actor, project_id, command.subject_id
        )
        consent_rows = self.consents.lock_required(
            session, project_id, subject.id, command.identity_kind
        )
        if set(command.consent_ids) != {row.id for row in consent_rows}:
            raise Forbidden("request consent ids do not equal the locked required set")
        source_ids = list(command.source_media_asset_ids)
        sources = list(session.scalars(select(MediaAsset).where(
            MediaAsset.id.in_(source_ids), MediaAsset.project_id == project_id,
            MediaAsset.account_id == subject.account_id,
            MediaAsset.quarantined_at.is_(None), MediaAsset.deleted_at.is_(None),
        ))) if source_ids else []
        if {row.id for row in sources} != set(source_ids):
            raise NotFound("governed enrollment source")
        required_rights = RIGHTS_BY_IDENTITY[command.identity_kind]
        grants = list(session.scalars(select(MediaRightsGrant).where(
            MediaRightsGrant.project_id == project_id,
            MediaRightsGrant.account_id == subject.account_id,
            MediaRightsGrant.subject_id.in_(source_ids),
            MediaRightsGrant.rights_type.in_(
                [item.value for item in required_rights]
            ),
        ).with_for_update())) if source_ids else []
        selected_rights = self.rights.require_identity_source_rights(
            source_asset_ids=source_ids,
            grants=grants,
            required_types=required_rights,
            platform=None,
            territory="CN",
        )
        rights_hashes = sorted(
            rights_grant_snapshot_sha256(grant) for grant in selected_rights
        )

        task = self._submit_task(
            session, actor, project_id, command, quote, billing_route, capability
        )
        reservation = self.quota.reserve(
            session, actor, project_id, capability, units=1,
            idempotency_key=f"{command.idempotency_key}:quota",
        )
        reservation.task_record_id = task.id
        enrollment = IdentityEnrollment(
            task_record_id=task.id, account_id=subject.account_id,
            project_id=subject.project_id, subject_id=subject.id,
            identity_kind=command.identity_kind.value, provider=entitlement.provider,
            source_media_asset_ids=[str(item) for item in source_ids],
            rights_snapshot_sha256s=rights_hashes,
            status=EnrollmentStatus.QUEUED.value,
            idempotency_key=command.idempotency_key,
            initiated_by_actor_id=actor.actor_id,
        )
        session.add(enrollment)
        session.flush()
        identity = ProviderIdentityAsset(
            account_id=subject.account_id, project_id=subject.project_id,
            subject_id=subject.id, identity_kind=command.identity_kind.value,
            provider=entitlement.provider, enrollment_id=enrollment.id,
            status=IdentityAssetStatus.ENROLLING.value,
        )
        session.add(identity)
        session.flush()
        for consent in consent_rows:
            session.add(IdentityAssetConsent(
                identity_asset_id=identity.id, consent_id=consent.id,
                consent_snapshot_sha256=consent.terms_snapshot_sha256,
            ))
        self.consents.consume(consent_rows)
        event_type = "identity.enrollment.requested"
        self.outbox.add(session, event_envelope(
            event_id=uuid4(), event_type=event_type,
            aggregate_id=enrollment.id, occurred_at=self.clock.now(),
            initiated_by_actor_id=actor.actor_id,
            idempotency_key=f"identity-enrollment:{enrollment.id}:requested",
            payload={
                "enrollment_id": enrollment.id,
                "identity_asset_id": identity.id,
                "provider_verification_receipt_asset_id": (
                    command.provider_verification_receipt_asset_id
                ),
            },
        ))
        self.audit.write(session, actor=actor, action="identity.enrollment.created",
                         target_type="identity_enrollment", target_id=enrollment.id,
                         project_id=enrollment.project_id,
                         metadata={"identity_kind": enrollment.identity_kind,
                                   "rights_snapshot_sha256s": rights_hashes})
        return EnrollmentCreated(enrollment, identity, task, reservation,
                                 tuple(row.terms_snapshot_sha256 for row in consent_rows),
                                 event_type)

    def _created(self, session, actor, project, enrollment):
        identity = session.scalar(select(ProviderIdentityAsset).where(
            ProviderIdentityAsset.enrollment_id == enrollment.id,
            ProviderIdentityAsset.project_id == project.id,
            ProviderIdentityAsset.account_id == actor.account_id,
        ))
        task = session.scalar(select(TaskRecord).where(
            TaskRecord.id == enrollment.task_record_id,
            TaskRecord.project_id == project.id,
            TaskRecord.initiated_by_account_id == actor.account_id,
        ))
        reservation = session.scalar(select(CapabilityQuotaReservation).where(
            CapabilityQuotaReservation.task_record_id == enrollment.task_record_id,
            CapabilityQuotaReservation.project_id == project.id,
            CapabilityQuotaReservation.account_id == actor.account_id,
        ))
        links = list(session.scalars(select(IdentityAssetConsent).where(
            IdentityAssetConsent.identity_asset_id == identity.id
        ))) if identity is not None else []
        if identity is None or task is None or reservation is None:
            raise Conflict("existing enrollment aggregate is incomplete")
        return EnrollmentCreated(
            enrollment, identity, task, reservation,
            tuple(sorted(link.consent_snapshot_sha256 for link in links)),
            "identity.enrollment.requested",
        )

    def _expected_fingerprint(self, actor, project_id, command, quote, route):
        if route.mode is BillingMode.CUSTOMER_CREDIT:
            scope_id = actor.account_id
            maximum_amount = quote.credit_units
        else:
            if route.cost_center_id is None:
                raise Conflict("internal billing route has no cost center")
            scope_id = route.cost_center_id
            maximum_amount = quote.amount_fen
        return TaskSubmissionService._request_fingerprint(
            actor_account_id=actor.account_id,
            project_id=project_id,
            capability=CAPABILITY_BY_IDENTITY[command.identity_kind].value,
            billing_mode=route.mode,
            billing_scope_id=scope_id,
            maximum_amount=maximum_amount,
            input_payload=self._input_payload(command),
        )

    def _submit_task(self, session, actor, project_id, command, quote, route, capability):
        payload = self._input_payload(command)
        if route.mode is BillingMode.CUSTOMER_CREDIT:
            if command.approved_credit_units != quote.credit_units:
                raise Conflict("customer approval differs from frozen enrollment quote")
            return self.tasks.submit_customer(session, actor, project_id,
                capability.value, quote.credit_units, command.idempotency_key, payload)
        if command.approved_amount_fen != quote.amount_fen or route.cost_center_id is None:
            raise Conflict("internal approval differs from frozen enrollment quote")
        return self.tasks.submit_internal(session, actor, project_id,
            capability.value, route.cost_center_id, quote.amount_fen,
            command.idempotency_key, payload)

    @staticmethod
    def _input_payload(command):
        return {
            "subject_id": str(command.subject_id),
            "identity_kind": command.identity_kind.value,
            "source_media_asset_ids": [str(item) for item in command.source_media_asset_ids],
            "provider_verification_receipt_asset_id": (
                str(command.provider_verification_receipt_asset_id)
                if command.provider_verification_receipt_asset_id else None
            ),
            "consent_ids": sorted(str(item) for item in command.consent_ids),
            "_billing": {
                "credit_units": command.approved_credit_units,
                "amount_fen": command.approved_amount_fen,
            },
        }
~~~

Do not repeat a hold, quota reservation, consent consumption, audit row, or outbox event on retry.

- [ ] **Step 5: Run enrollment transaction tests**

Run:

~~~bash
cd backend && uv run pytest tests/integration/governance/test_enrollment_submission.py -v
~~~

Expected: PASS, at least 6 tests. Verify PostgreSQL row locks, source-right snapshots, exact provider-cost task lineage, and the real Plan 01 hold/outbox transaction; do not replace this suite with SQLite.

- [ ] **Step 6: Write failing governed-generation tests**

Create backend/tests/integration/governance/test_governed_generation.py with these assertions:

~~~python
import pytest

from ip_saas.common.errors import Conflict, Forbidden, NotFound


def test_each_capability_accepts_only_its_active_identity_kind(governed_generation_cases):
    for case in governed_generation_cases:
        job = case.submit()
        assert job.capability == case.media_capability.value
        task = case.task_record_for(job)
        assert len(task.request_fingerprint) == 64
        assert case.task_requested_event(job)["request_fingerprint"] == task.request_fingerprint
        assert [use.identity_asset_id for use in job.identity_uses] == [case.identity.id]
        assert all(use.consent_snapshot_sha256s for use in job.identity_uses)
        if case.capability.value != "seedance_authorized_portrait":
            assert all(use.rights_snapshot_sha256s for use in job.identity_uses)
        assert "provider_resource_ref" not in job.request_payload


def test_generation_rolls_back_task_hold_quota_and_identity_use_together(
    governed_generation_with_failing_outbox,
) -> None:
    with pytest.raises(RuntimeError, match="outbox unavailable"):
        governed_generation_with_failing_outbox.submit()
    assert governed_generation_with_failing_outbox.persisted_delta() == {
        "tasks": 0, "holds": 0, "quota": 0, "jobs": 0, "identity_uses": 0,
    }


def test_generation_exact_replay_has_zero_governance_side_effects(
    governed_generation_factory,
) -> None:
    case = governed_generation_factory.voice_clone()
    first = case.submit(idempotency_key="governed-replay-key")
    before = case.durable_side_effect_counts()
    replay = case.submit(idempotency_key="governed-replay-key")
    assert replay.id == first.id
    assert case.durable_side_effect_counts() == before
    assert before == {
        "tasks": 1, "holds": 1, "quota_reservations": 1,
        "identity_uses": 1, "consent_usage": 1,
        "task_outbox": 1, "governance_outbox": 1,
    }
    event = case.single_event("governed.media.requested")
    assert event.idempotency_key == f"governed-media:{first.id}:requested"
    assert event.aggregate_id == first.id
    assert event.payload["generation_job_id"] == str(first.id)


@pytest.mark.parametrize("changed", ["script", "identity", "quote"])
def test_generation_replay_with_changed_input_conflicts_without_side_effects(
    governed_generation_factory, changed,
) -> None:
    case = governed_generation_factory.voice_clone()
    case.submit(idempotency_key="governed-conflict-key")
    before = case.durable_side_effect_counts()
    with pytest.raises(Conflict, match="different governed generation input"):
        case.replay_with(changed, idempotency_key="governed-conflict-key")
    assert case.durable_side_effect_counts() == before


def test_generation_existing_lookup_requires_owned_project_and_account(
    governed_generation_factory,
) -> None:
    case = governed_generation_factory.voice_clone()
    case.submit(idempotency_key="governed-private-key")
    with pytest.raises((Forbidden, NotFound)):
        case.replay_as_other_account(idempotency_key="governed-private-key")
    assert case.foreign_job_or_task_was_returned is False


def test_withdrawal_race_loses_to_locked_consent_or_blocks_delivery(race_harness):
    result = race_harness.run_generation_against_withdrawal()
    assert result in {"withdrawal_first_generation_rejected",
                      "generation_first_delivery_blocked"}


def test_generation_rechecks_consent_and_source_rights_for_exact_platform(
    governed_generation_factory,
) -> None:
    case = governed_generation_factory.voice_clone(
        consent_platforms=["douyin"], rights_platforms=["douyin"]
    )
    with pytest.raises(Forbidden, match="platform"):
        case.submit(platform="xiaohongshu")


def test_real_generation_cost_links_exact_media_task(
    governed_generation_factory,
) -> None:
    completed = governed_generation_factory.voice_clone().complete_real_provider()
    assert completed.provider_cost.task_id == completed.job.task_record_id
    assert completed.provider_cost_row_count == 1
~~~

- [ ] **Step 7: Implement the governed MediaJobService wrapper**

Create backend/src/ip_saas/modules/governance/generation.py:

~~~python
from uuid import UUID, uuid4

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.errors import Conflict, Forbidden
from ip_saas.common.outbox import OutboxWriter
from ip_saas.modules.billing.service import BillingMode
from ip_saas.modules.media.enums import Capability
from ip_saas.modules.media.models.jobs import GenerationJob
from ip_saas.modules.media.models.identity import MediaRightsGrant
from ip_saas.modules.media.rights import RightsPolicy, rights_grant_snapshot_sha256
from ip_saas.modules.media.service import JobCreateCommand, MediaJobService
from ip_saas.modules.projects.access import ProjectAccessService
from ip_saas.modules.tasks.models import TaskRecord
from ip_saas.modules.tasks.service import TaskSubmissionService

from .consent import ConsentService
from .eligibility import (
    CapabilityQuotaService,
    lock_account_idempotency,
    require_governance_project,
)
from .enrollment import RIGHTS_BY_IDENTITY
from .enums import BetaCapability, IdentityAssetStatus, IdentityKind
from .events import event_envelope
from .models import (
    GenerationIdentityUse,
    IdentityEnrollment,
    ProviderIdentityAsset,
)


MEDIA_CAPABILITY = {
    BetaCapability.VOICE_CLONE: Capability.VOICE_CLONE,
    BetaCapability.SEEDANCE_AUTHORIZED_PORTRAIT: Capability.AUTHORIZED_PORTRAIT_VIDEO,
    BetaCapability.TRADITIONAL_DIGITAL_HUMAN: Capability.CLONED_AVATAR_VIDEO,
}
IDENTITY_KIND = {
    BetaCapability.VOICE_CLONE: IdentityKind.VOICE_CLONE,
    BetaCapability.SEEDANCE_AUTHORIZED_PORTRAIT: IdentityKind.SEEDANCE_PORTRAIT,
    BetaCapability.TRADITIONAL_DIGITAL_HUMAN: IdentityKind.TRADITIONAL_AVATAR,
}


class GovernedGenerationService:
    def __init__(self, access: ProjectAccessService,
                 media_jobs: MediaJobService, tasks: TaskSubmissionService,
                 quota: CapabilityQuotaService,
                 consents: ConsentService, rights: RightsPolicy,
                 outbox: OutboxWriter, clock) -> None:
        self.access = access
        self.media_jobs = media_jobs
        self.tasks = tasks
        self.quota = quota
        self.consents = consents
        self.rights = rights
        self.outbox = outbox
        self.clock = clock

    def create(self, session: Session, actor, project_id, command, quote,
               billing_route):
        if len(command.identity_asset_ids) != 1:
            raise Forbidden("V1 requires exactly one governed identity asset")
        project = require_governance_project(
            self.access, session, actor, project_id
        )
        lock_account_idempotency(
            session, actor.account_id, "governed-generation",
            command.idempotency_key,
        )
        existing = session.scalar(select(GenerationJob).where(
            GenerationJob.account_id == actor.account_id,
            GenerationJob.project_id == project.id,
            GenerationJob.idempotency_key == command.idempotency_key,
        ))
        if existing is not None:
            return self._existing(
                session, actor, project.id, command, quote,
                billing_route, existing,
            )
        account_task_id = session.scalar(select(TaskRecord.id).where(
            TaskRecord.initiated_by_account_id == actor.account_id,
            TaskRecord.idempotency_key == command.idempotency_key,
        ).limit(1))
        if account_task_id is not None:
            raise Conflict("idempotency key is already used in this account")
        identity = session.scalar(select(ProviderIdentityAsset).where(
            ProviderIdentityAsset.id == command.identity_asset_ids[0],
            ProviderIdentityAsset.project_id == project_id,
            ProviderIdentityAsset.account_id == actor.account_id,
        ).with_for_update())
        expected = IDENTITY_KIND[command.capability]
        if (identity is None or identity.identity_kind != expected.value
                or identity.status != IdentityAssetStatus.ACTIVE.value
                or identity.frozen_at is not None or identity.deleted_at is not None):
            raise Forbidden("active matching identity asset is required")
        consent_rows = self.consents.lock_required(
            session, project_id, identity.subject_id, expected,
            platform=command.platform, territory="CN",
        )
        enrollment = session.get(IdentityEnrollment, identity.enrollment_id)
        if enrollment is None:
            raise Forbidden("identity enrollment lineage is missing")
        source_ids = [UUID(value) for value in enrollment.source_media_asset_ids]
        required_rights = RIGHTS_BY_IDENTITY[expected]
        grants = list(session.scalars(select(MediaRightsGrant).where(
            MediaRightsGrant.project_id == project_id,
            MediaRightsGrant.account_id == identity.account_id,
            MediaRightsGrant.subject_id.in_(source_ids),
            MediaRightsGrant.rights_type.in_(
                [item.value for item in required_rights]
            ),
        ).with_for_update())) if source_ids else []
        selected_rights = self.rights.require_identity_source_rights(
            source_asset_ids=source_ids,
            grants=grants,
            required_types=required_rights,
            platform=command.platform,
            territory="CN",
        )
        rights_hashes = sorted(
            rights_grant_snapshot_sha256(grant) for grant in selected_rights
        )
        reservation = self.quota.reserve(
            session, actor, project_id, command.capability, units=1,
            idempotency_key=f"{command.idempotency_key}:identity-quota",
        )
        job = self.media_jobs.create(session, actor, JobCreateCommand(
            project_id=project_id, production_id=command.production_id,
            shot_id=command.shot_id, capability=MEDIA_CAPABILITY[command.capability],
            quote=quote, billing_route=billing_route,
            approved_credit_units=command.approved_credit_units,
            approved_amount_fen=command.approved_amount_fen,
            request_payload=self._request_payload(command),
            idempotency_key=command.idempotency_key,
        ))
        reservation.task_record_id = job.task_record_id
        use = GenerationIdentityUse(
            generation_job_id=job.id, identity_asset_id=identity.id,
            consent_snapshot_sha256s=sorted(
                row.terms_snapshot_sha256 for row in consent_rows
            ),
            rights_snapshot_sha256s=rights_hashes,
            platform=command.platform, used_at=self.clock.now(),
        )
        session.add(use)
        task = session.scalar(select(TaskRecord).where(
            TaskRecord.id == job.task_record_id,
            TaskRecord.project_id == project.id,
            TaskRecord.initiated_by_account_id == actor.account_id,
        ))
        if task is None:
            raise Forbidden("generation task lineage is missing")
        self.outbox.add(session, event_envelope(
            event_id=uuid4(), event_type="governed.media.requested",
            aggregate_id=job.id, occurred_at=self.clock.now(),
            initiated_by_actor_id=actor.actor_id,
            idempotency_key=f"governed-media:{job.id}:requested",
            payload={
                "task_id": task.id,
                "request_fingerprint": task.request_fingerprint,
                "generation_job_id": job.id,
                "identity_asset_ids": [identity.id],
            },
        ))
        self.consents.consume(consent_rows)
        return job

    def _existing(self, session, actor, project_id, command, quote, route, job):
        task = session.scalar(select(TaskRecord).where(
            TaskRecord.id == job.task_record_id,
            TaskRecord.project_id == project_id,
            TaskRecord.initiated_by_account_id == actor.account_id,
            TaskRecord.idempotency_key == command.idempotency_key,
        ))
        uses = list(session.scalars(select(GenerationIdentityUse).where(
            GenerationIdentityUse.generation_job_id == job.id
        )))
        request_payload = self._request_payload(command)
        expected_job_payload = {
            **request_payload,
            "_billing": {
                "credit_units": quote.credit_units,
                "amount_fen": quote.amount_fen,
                "quote_version": quote.quote_version,
            },
        }
        approved_matches = (
            route.mode is BillingMode.CUSTOMER_CREDIT
            and route.account_id == actor.account_id
            and command.approved_credit_units == quote.credit_units
            and command.approved_amount_fen is None
        ) or (
            route.mode is BillingMode.INTERNAL_COST
            and route.cost_center_id is not None
            and command.approved_amount_fen == quote.amount_fen
            and command.approved_credit_units is None
        )
        expected_task_payload = self._task_payload(job.id, command, quote)
        expected_fingerprint = self._task_fingerprint(
            actor, project_id, command, quote, route, expected_task_payload
        )
        if (
            task is None
            or task.capability != MEDIA_CAPABILITY[command.capability].value
            or task.billing_mode != route.mode.value
            or job.production_id != command.production_id
            or job.shot_id != command.shot_id
            or job.capability != MEDIA_CAPABILITY[command.capability].value
            or job.provider != quote.provider
            or job.model_id != quote.model_id
            or job.model_version != quote.model_version
            or job.request_payload != expected_job_payload
            or not approved_matches
            or task.input_payload != expected_task_payload
            or task.request_fingerprint != expected_fingerprint
            or len(uses) != 1
            or uses[0].identity_asset_id != command.identity_asset_ids[0]
            or uses[0].platform != command.platform
        ):
            raise Conflict("idempotency key has different governed generation input")
        return job

    @staticmethod
    def _request_payload(command):
        return {
            "identity_asset_ids": [str(item) for item in command.identity_asset_ids],
            "script_text": command.script_text,
            "platform": command.platform,
        }

    @classmethod
    def _task_payload(cls, job_id, command, quote):
        return {
            **cls._request_payload(command),
            "media_job_id": str(job_id),
            "production_id": str(command.production_id),
            "shot_id": str(command.shot_id) if command.shot_id else None,
            "provider": quote.provider,
            "model_id": quote.model_id,
            "model_version": quote.model_version,
            "quote_version": quote.quote_version,
        }

    @staticmethod
    def _task_fingerprint(actor, project_id, command, quote, route, payload):
        if route.mode is BillingMode.CUSTOMER_CREDIT:
            scope_id = actor.account_id
            maximum_amount = quote.credit_units
        else:
            if route.cost_center_id is None:
                raise Conflict("internal billing route has no cost center")
            scope_id = route.cost_center_id
            maximum_amount = quote.amount_fen
        return TaskSubmissionService._request_fingerprint(
            actor_account_id=actor.account_id,
            project_id=project_id,
            capability=MEDIA_CAPABILITY[command.capability].value,
            billing_mode=route.mode,
            billing_scope_id=scope_id,
            maximum_amount=maximum_amount,
            input_payload=payload,
        )
~~~

MediaJobService continues to create the only TaskRecord and billing hold. The governance wrapper creates only lineage and a provider-quota reservation tied to that TaskRecord. The account-scoped PostgreSQL advisory transaction lock serializes concurrent uses of the same client key. Exact replay performs only authorized reads and returns the existing job; it does not call `lock_required`, `quota.reserve`, `MediaJobService.create`, `consents.consume`, `session.add(GenerationIdentityUse)`, or `outbox.add`. The fingerprint comparison deliberately invokes Plan 01's canonical implementation instead of maintaining a second hash algorithm; its contract test fails if Plan 01 changes that implementation without updating this seam.

- [ ] **Step 8: Implement the enrollment worker without leaking provider references**

Create backend/src/ip_saas/workers/identity_enrollment.py. In one short database transaction claim queued or lease-expired work with `FOR UPDATE SKIP LOCKED`, call `claimed = TaskSubmissionService.start(session, task_id, lease_seconds=300, max_attempts=8)`. If the returned row is terminal, acknowledge without side effects. If it is `RECONCILIATION_REQUIRED`, return `TaskHandlerDisposition.RETRY` without reading provider secrets, creating signed source URLs, calling a provider, or mutating consent, quota, domain, billing, events, delivery, or task state. Otherwise require RUNNING and retain `attempt_no = claimed.attempt_no`; reload consent plus every `MediaRightsGrant` named by `IdentityEnrollment.source_media_asset_ids`; run `RightsPolicy.require_identity_source_rights(..., platform=None, territory="CN")`; compare its sorted canonical hashes to `IdentityEnrollment.rights_snapshot_sha256s`; and commit the running state only when they match. A missing, revoked, expired, broadened, narrowed, or otherwise changed grant fails closed and requires a new enrollment request. Outside the transaction use Task 6's durable attempt/send-marker protocol before creating one-hour private TOS signed source URLs, resolving a Seedance provider-verification receipt through the encrypted receipt reader, and calling the capability-keyed adapter. A send-marked attempt without a persisted response returns RETRY without another provider call. Heartbeat with `heartbeat(session, task_id, attempt_no, lease_seconds=300)` before and after each bounded provider wait. Import `normalize_provider_request_id` from `ip_saas.providers.digital_identity.base`, and use these exact helpers for a production terminal response:

~~~python
def provider_cost_for_enrollment(
    task, enrollment, usage, provider_request_id: str,
) -> ProviderCostInput:
    try:
        normalized_request_id = normalize_provider_request_id(provider_request_id)
    except ValueError as exc:
        raise Conflict("provider terminal request identity is invalid") from exc
    return ProviderCostInput(
        provider=enrollment.provider,
        capability=task.capability,
        model_id=usage.model_id,
        model_version=usage.model_version,
        native_quantity=usage.native_quantity,
        native_unit=usage.native_unit,
        supplier_amount_minor=usage.supplier_amount_minor,
        supplier_currency=usage.supplier_currency,
        amount_fen=usage.amount_fen,
        reconciliation_status=usage.reconciliation_status,
        task_id=task.id,
        provider_request_id=normalized_request_id,
    )


def enrollment_actual_amount(task, usage, succeeded: bool) -> int:
    mode = BillingMode(task.billing_mode)
    if mode is BillingMode.INTERNAL_COST:
        return usage.amount_fen
    if not succeeded:
        return 0
    billing = task.input_payload.get("_billing")
    if not isinstance(billing, dict):
        raise Conflict("enrollment task lost its frozen billing quote")
    credit_units = billing.get("credit_units")
    if not isinstance(credit_units, int) or credit_units <= 0:
        raise Conflict("enrollment task has invalid approved credit units")
    return credit_units
~~~

In a second transaction, re-lock TaskRecord, IdentityEnrollment, ProviderIdentityAsset, and CapabilityQuotaReservation. Call `heartbeat(session, task.id, attempt_no, lease_seconds=300)` first; reject a stale attempt before mutating any of those rows or billing. A production terminal success or failure without `result.usage` raises `Conflict("provider terminal usage is missing")`, remains non-deliverable, and enters reconciliation retry instead of fabricating zero cost. With complete usage, perform this exact order in one transaction:

~~~python
usage = result.usage
if usage is None:
    raise Conflict("provider terminal usage is missing")
succeeded = result.state is ProviderIdentityState.ACTIVE
if succeeded and not result.provider_resource_ref:
    raise Conflict("active provider identity has no resource reference")
provider_cost = provider_cost_for_enrollment(
    task, enrollment, usage, result.provider_request_id
)
billing.settle_generation(
    session,
    BillingContext(BillingMode(task.billing_mode), task.billing_hold_id),
    enrollment_actual_amount(task, usage, succeeded),
    provider_cost,
    f"settle:identity-enrollment:{task.id}",
)
quota.settle(reservation)
if succeeded:
    assert result.provider_resource_ref is not None
    identity.provider_resource_ref_ciphertext = ref_cipher.encrypt(
        result.provider_resource_ref.encode("utf-8")
    )
    identity.provider_resource_ref_fingerprint = sha256(
        result.provider_resource_ref.encode("utf-8")
    ).hexdigest()
    identity.status = IdentityAssetStatus.ACTIVE.value
    identity.activated_at = clock.now()
    enrollment.status = EnrollmentStatus.SUCCEEDED.value
    tasks.succeed(session, task.id, attempt_no, {
        "identity_asset_id": str(identity.id),
        "identity_kind": identity.identity_kind,
    })
else:
    enrollment.status = EnrollmentStatus.FAILED.value
    enrollment.last_error_code = result.sanitized_error_code or "provider_failed"
    enrollment.last_error_message = "identity enrollment provider failed"
    tasks.fail(session, task.id, attempt_no, enrollment.last_error_code,
               enrollment.last_error_message)
~~~

The settlement, provider request, and quota reservation each use their frozen server-derived idempotency key, so replay observes the same rows without a second mutation. Customer failure settles zero customer units while preserving the task-linked supplier-cost row; internal failure settles the actual CNY supplier cost. A deterministic non-production fake never creates `ProviderCostInput(task_id=None)`: fake-only unit tests bypass settlement, and a ledger-focused fake test uses an explicit canonical fake request ID plus the exact test TaskRecord ID. A path with durable proof that no provider request was emitted constructs no ProviderCostInput, releases billing and quota through the fenced no-call finalizer, then fails without a ProviderCostEntry. Any send-marked or real terminal result without an exact task ID, canonical supplier request ID, and complete usage keeps the hold active and enters reconciliation. Delete the TOS staging source immediately. Persist the sanitized provider request identity only in `ProviderCostEntry.provider_request_id`, the governed attempt journal, and restricted evidence; never expose it or the provider resource reference in TaskRecord.result_payload.

- [ ] **Step 9: Extend media submit/copy workers under the existing state machine**

In backend/src/ip_saas/workers/media_submit.py, branch only for the three new Capability values. Use the same `start`/heartbeat/attempt-number fence; a terminal return is ACK-only and an expired attempt cannot settle or advance GenerationJob. A returned `RECONCILIATION_REQUIRED` state is RETRY-only and makes zero provider, domain, cost, event, quota, or delivery calls. Lock GenerationIdentityUse, recheck consent/frozen/deleted state, re-run the exact-platform source-rights policy through the linked IdentityEnrollment, decrypt the provider reference into a local variable, invoke DigitalIdentityProviderRegistry under Task 6's committed attempt/send-marker boundary, and discard the variable before committing. A reclaimed send-marked unknown result is never sent again. Keep `GenerationJob.provider_status` transitions from Plan 03 unchanged. When the existing Plan 03 settlement path builds `ProviderCostInput` for any of these three real-provider jobs, it must set `task_id=job.task_record_id` and `provider_request_id` to the sanitized terminal supplier request identity; terminal production results with missing usage or a mismatched task ID fail reconciliation and stay undeliverable.

In backend/src/ip_saas/workers/media_copy.py, copy temporary provider output to private TOS before provider URL expiry, calculate SHA-256 while streaming, create MediaAsset with AiDisclosure.GENERATED, clear the temporary URL from memory, and enqueue the existing QC worker. On consent withdrawal, copy may finish for cost reconciliation but marks the asset quarantined and cannot create a delivery URL.

- [ ] **Step 10: Run governed generation and duplicate-delivery tests**

Run:

~~~bash
cd backend && uv run pytest tests/integration/governance/test_governed_generation.py tests/integration/media/test_worker_delivery.py -v
~~~

Expected: PASS. Exact API and RocketMQ replay returns the same job and hold with zero additional consent usage, quota reservation, GenerationIdentityUse, or outbox row; changed script, identity, quote, approval, project, or account conflicts before side effects. A capability/identity mismatch fails before hold creation; missing/revoked/wrong-platform source rights fail closed; each real-provider enrollment or generation has exactly one ProviderCostEntry whose task_id equals the aggregate TaskRecord; a withdrawn identity never reaches READY or a signed-delivery response. Enrollment and governed-media outbox idempotency keys contain their server aggregate UUID rather than the client key.

- [ ] **Step 11: Commit atomic enrollment and governed generation**

~~~bash
git add backend/src/ip_saas/modules/media/enums.py backend/src/ip_saas/modules/media/rights.py backend/src/ip_saas/modules/governance/enrollment.py backend/src/ip_saas/modules/governance/generation.py backend/src/ip_saas/workers/identity_enrollment.py backend/src/ip_saas/workers/media_submit.py backend/src/ip_saas/workers/media_copy.py backend/tests/integration/governance/test_enrollment_submission.py backend/tests/integration/governance/test_governed_generation.py
git commit -m "feat: submit governed identity media atomically"
~~~

### Task 8: Make explicit and implicit AI identity labels survive every export

**Files:**
- Modify: backend/src/ip_saas/modules/media/labels.py
- Modify: backend/src/ip_saas/modules/media/ffmpeg.py
- Modify: backend/src/ip_saas/modules/media/qc.py
- Create: backend/src/ip_saas/modules/governance/export_labels.py
- Modify: backend/src/ip_saas/workers/media_finish.py
- Modify: backend/src/ip_saas/workers/media_qc.py
- Test: backend/tests/unit/governance/test_export_label_policy.py
- Test: backend/tests/integration/governance/test_export_label_survival.py

- [ ] **Step 1: Write failing label-policy and exact-hash delivery tests**

Create backend/tests/unit/governance/test_export_label_policy.py:

~~~python
from ip_saas.modules.governance.export_labels import (
    IdentityDisclosureManifest,
    labels_pass,
)


def test_identity_manifest_contains_required_service_and_content_fields() -> None:
    manifest = IdentityDisclosureManifest(
        content_id="018f0000-0000-7000-8000-000000000001",
        capability="traditional_digital_human",
        service_name="IP Agent SaaS",
        model_id="local-model-alias",
        model_version="2026-08-24",
        asset_sha256="a" * 64,
    )
    assert manifest.visible_text == "AI生成/合成内容"
    assert manifest.ffmpeg_metadata()["ai_generated"] == "true"
    assert manifest.ffmpeg_metadata()["ai_synthetic_identity"] == "true"
    assert "content_id" in manifest.ffmpeg_metadata()
    assert "service_name" in manifest.ffmpeg_metadata()


def test_delivery_requires_both_labels_for_the_exact_current_bytes() -> None:
    assert labels_pass("a" * 64, "a" * 64, True, True)
    assert not labels_pass("a" * 64, "b" * 64, True, True)
    assert not labels_pass("a" * 64, "a" * 64, False, True)
    assert not labels_pass("a" * 64, "a" * 64, True, False)
~~~

- [ ] **Step 2: Run the policy tests and confirm the identity manifest is missing**

Run:

~~~bash
cd backend && uv run pytest tests/unit/governance/test_export_label_policy.py -v
~~~

Expected: FAIL during collection because governance.export_labels does not exist.

- [ ] **Step 3: Implement the canonical manifest and exact-byte predicate**

Create backend/src/ip_saas/modules/governance/export_labels.py:

~~~python
from dataclasses import asdict, dataclass
from hashlib import sha256
import json


@dataclass(frozen=True)
class IdentityDisclosureManifest:
    content_id: str
    capability: str
    service_name: str
    model_id: str
    model_version: str
    asset_sha256: str
    visible_text: str = "AI生成/合成内容"

    def canonical_json(self) -> str:
        return json.dumps(asdict(self), ensure_ascii=False,
                          sort_keys=True, separators=(",", ":"))

    def manifest_sha256(self) -> str:
        return sha256(self.canonical_json().encode("utf-8")).hexdigest()

    def ffmpeg_metadata(self) -> dict[str, str]:
        return {
            "ai_generated": "true",
            "ai_synthetic_identity": "true",
            "content_id": self.content_id,
            "service_name": self.service_name,
            "identity_capability": self.capability,
            "model_id": self.model_id,
            "model_version": self.model_version,
            "identity_manifest_sha256": self.manifest_sha256(),
        }


def labels_pass(current_asset_sha256: str, verified_asset_sha256: str,
                explicit_label_passed: bool,
                implicit_label_passed: bool) -> bool:
    return (current_asset_sha256 == verified_asset_sha256
            and explicit_label_passed and implicit_label_passed)
~~~

- [ ] **Step 4: Extend the controlled FFmpeg boundary with fixed metadata only**

Add `identity_manifest: IdentityDisclosureManifest | None` to Plan 03 FinishSpec. In `build_ffmpeg_args`, append each key/value from `identity_manifest.ffmpeg_metadata()` as a separate `-metadata`, `key=value` argument, sorted by key. When present, draw `identity_manifest.visible_text` in the existing fixed upper-right label box. Do not accept label text, position, font path, opacity, metadata keys, or removal flags from HTTP input.

Add to backend/src/ip_saas/modules/media/labels.py:

~~~python
def disclosure_for_identity_sources(sources: list[AiDisclosure],
                                    has_governed_identity: bool) -> DisclosurePolicy:
    ordinary = disclosure_for_sources(sources)
    if not has_governed_identity:
        return ordinary
    return DisclosurePolicy(
        AiDisclosure.GENERATED,
        "AI生成/合成内容",
        {**ordinary.metadata, "ai_generated": "true",
         "ai_assisted": "true", "ai_synthetic_identity": "true"},
    )
~~~

- [ ] **Step 5: Add a real export-survival integration test**

Create backend/tests/integration/governance/test_export_label_survival.py. Generate a two-second controlled clip, finish it with an identity manifest, then run two more real transcodes: one preserving the label/metadata and one deliberately stripping metadata. Probe frames at 0.2, 1.0, and 1.8 seconds and the container tags.

~~~python
def test_every_transcode_requires_a_new_exact_hash_verification(export_harness) -> None:
    original = export_harness.finish_identity_video()
    first = export_harness.verify(original, stage="finish")
    assert first.explicit_label_passed and first.implicit_label_passed

    captioned = export_harness.transcode_with_captions(original)
    assert captioned.sha256 != original.sha256
    assert not export_harness.can_deliver(captioned, verification=first)
    second = export_harness.verify(captioned, stage="caption")
    assert second.overall_passed

    stripped = export_harness.transcode_stripping_metadata(captioned)
    failed = export_harness.verify(stripped, stage="export")
    assert failed.explicit_label_passed
    assert not failed.implicit_label_passed
    assert not export_harness.can_deliver(stripped, verification=failed)
~~~

- [ ] **Step 6: Persist verification on every finish, caption, and export**

In backend/src/ip_saas/workers/media_finish.py, construct the manifest from local content/job/model identifiers before FFmpeg. In backend/src/ip_saas/workers/media_qc.py, run the controlled FFprobe tag check plus deterministic label-region frame probe and insert ExportVerification for the exact MediaAsset SHA-256 and stage.

Add this delivery guard to backend/src/ip_saas/modules/media/qc.py:

~~~python
def require_export_delivery(session, media_asset, stage: str) -> ExportVerification:
    verification = session.scalar(
        select(ExportVerification)
        .where(
            ExportVerification.media_asset_id == media_asset.id,
            ExportVerification.asset_sha256 == media_asset.sha256,
            ExportVerification.export_stage == stage,
        )
        .order_by(ExportVerification.checked_at.desc())
        .limit(1)
    )
    if (verification is None or not verification.overall_passed
            or media_asset.quarantined_at is not None
            or media_asset.deleted_at is not None):
        raise Forbidden("current export has not passed identity disclosure verification")
    blocked = session.scalar(select(GenerationIdentityUse).where(
        GenerationIdentityUse.generation_job_id == media_asset.job_id,
        GenerationIdentityUse.delivery_blocked_at.is_not(None),
    ))
    if blocked is not None:
        raise Forbidden("identity consent blocks delivery")
    return verification
~~~

Call this guard immediately before every TOS signed delivery URL is minted. The final export state remains non-deliverable if either the visible label or metadata is absent; platform administrators have no bypass.

- [ ] **Step 7: Run unit and real-media tests**

Run:

~~~bash
cd backend && uv run pytest tests/unit/governance/test_export_label_policy.py tests/integration/governance/test_export_label_survival.py tests/integration/media/test_media_finishing.py -v
~~~

Expected: PASS. The stripped-metadata fixture is rejected, the old verification cannot authorize new bytes, and all three sampled frames retain the explicit mark.

- [ ] **Step 8: Commit non-removable identity disclosure**

~~~bash
git add backend/src/ip_saas/modules/media/labels.py backend/src/ip_saas/modules/media/ffmpeg.py backend/src/ip_saas/modules/media/qc.py backend/src/ip_saas/modules/governance/export_labels.py backend/src/ip_saas/workers/media_finish.py backend/src/ip_saas/workers/media_qc.py backend/tests/unit/governance/test_export_label_policy.py backend/tests/integration/governance/test_export_label_survival.py
git commit -m "feat: verify identity disclosure after every export"
~~~

### Task 9: Takedown complaints and execute recoverable global deletion

**Files:**
- Modify: backend/src/ip_saas/modules/governance/schemas.py
- Create: backend/src/ip_saas/modules/governance/complaints.py
- Create: backend/src/ip_saas/modules/governance/deletion.py
- Modify: backend/src/ip_saas/workers/identity_deletion.py
- Test: backend/tests/integration/governance/test_complaint_takedown.py
- Test: backend/tests/integration/governance/test_global_deletion.py
- Test: backend/tests/resilience/test_deleted_data_not_restored.py

- [ ] **Step 1: Write failing critical-complaint tests**

Create backend/tests/integration/governance/test_complaint_takedown.py:

~~~python
def test_unauthorized_identity_complaint_freezes_first_and_returns_opaque_receipt(
    db_session, complaint_service, unauthorized_identity_command,
) -> None:
    with db_session.begin():
        receipt = complaint_service.receive(
            db_session, unauthorized_identity_command, source_ip="203.0.113.10"
        )
    assert receipt.public_reference.startswith("CMP-")
    assert receipt.status == "identity_frozen"
    assert receipt.identity_asset_id is None
    assert complaint_probe(db_session).identity.status == "frozen"
    assert complaint_probe(db_session).media_is_quarantined


def test_client_cannot_downgrade_critical_severity(complaint_service, command_factory) -> None:
    command = command_factory(category="minor_safety")
    assert complaint_service.classify(command.category) == "critical"


def test_public_complaint_rate_limit_does_not_drop_the_first_report(
    complaint_service, command_factory,
) -> None:
    complaint_service.receive_public(command_factory(), "203.0.113.12")
    for _ in range(4):
        complaint_service.receive_public(command_factory(), "203.0.113.12")
    with pytest.raises(TooManyRequests):
        complaint_service.receive_public(command_factory(), "203.0.113.12")
~~~

- [ ] **Step 2: Run complaint tests and verify the service is missing**

Run:

~~~bash
cd backend && uv run pytest tests/integration/governance/test_complaint_takedown.py -v
~~~

Expected: FAIL during collection because governance.complaints does not exist.

- [ ] **Step 3: Implement server-classified complaint intake and immediate takedown**

Create backend/src/ip_saas/modules/governance/complaints.py:

~~~python
from dataclasses import dataclass
from secrets import token_hex
from uuid import uuid4

from sqlalchemy import select

from ip_saas.common.errors import NotFound
from ip_saas.modules.governance.enums import ComplaintSeverity, ComplaintStatus
from ip_saas.modules.governance.events import event_envelope
from ip_saas.modules.governance.models import (
    ComplaintCase, GenerationIdentityUse, ProviderIdentityAsset,
)
from ip_saas.modules.media.models.jobs import GenerationJob, MediaAsset


CRITICAL_CATEGORIES = frozenset({
    "unauthorized_identity", "impersonation", "minor_safety",
})


@dataclass(frozen=True)
class PublicComplaintReceipt:
    public_reference: str
    status: str
    identity_asset_id: None = None


class ComplaintService:
    def __init__(self, session_factory, clock, contact_cipher, limiter, outbox,
                 audit, system_actor) -> None:
        self.session_factory = session_factory
        self.clock = clock
        self.contact_cipher = contact_cipher
        self.limiter = limiter
        self.outbox = outbox
        self.audit = audit
        self.system_actor = system_actor

    @staticmethod
    def classify(category: str) -> str:
        return (ComplaintSeverity.CRITICAL.value if category in CRITICAL_CATEGORIES
                else ComplaintSeverity.HIGH.value
                if category == "missing_ai_label" else ComplaintSeverity.NORMAL.value)

    def receive_public(self, command, source_ip: str):
        self.limiter.consume(f"complaint:{source_ip}", limit=5, window_seconds=3600)
        with self.session_factory.begin() as session:
            return self.receive(session, command, source_ip)

    def receive(self, session, command, source_ip: str) -> PublicComplaintReceipt:
        severity = self.classify(command.category)
        now = self.clock.now()
        identity_target = (session.get(ProviderIdentityAsset, command.identity_asset_id)
                           if command.identity_asset_id else None)
        content_target = (session.get(MediaAsset, command.content_id)
                          if command.content_id else None)
        project_ids = {row.project_id for row in (identity_target, content_target)
                       if row is not None}
        if not project_ids or len(project_ids) != 1:
            raise NotFound("complaint target")
        project_id = project_ids.pop()
        case = ComplaintCase(
            id=uuid4(), public_reference="CMP-" + token_hex(10).upper(),
            project_id=project_id,
            content_id=command.content_id, identity_asset_id=command.identity_asset_id,
            category=command.category, severity=severity,
            status=ComplaintStatus.RECEIVED.value, description=command.description,
            evidence_asset_ids=[str(item) for item in command.evidence_asset_ids],
            contact_ciphertext=self.contact_cipher.encrypt(command.contact_ciphertext.encode()),
            received_at=now,
        )
        session.add(case)
        session.flush()
        if severity == ComplaintSeverity.CRITICAL.value:
            identity_ids = {command.identity_asset_id} if command.identity_asset_id else set()
            if command.content_id:
                identity_ids.update(session.scalars(
                    select(GenerationIdentityUse.identity_asset_id)
                    .join(GenerationJob,
                          GenerationJob.id == GenerationIdentityUse.generation_job_id)
                    .join(MediaAsset, MediaAsset.job_id == GenerationJob.id)
                    .where(MediaAsset.id == command.content_id)
                ))
            for identity_id in identity_ids:
                identity = session.get(ProviderIdentityAsset, identity_id)
                if identity is None:
                    continue
                identity.status = "frozen"
                identity.frozen_at = now
                identity.freeze_reason = f"complaint:{case.public_reference}"
                for asset in session.scalars(
                    select(MediaAsset)
                    .join(GenerationJob, GenerationJob.id == MediaAsset.job_id)
                    .join(GenerationIdentityUse,
                          GenerationIdentityUse.generation_job_id == GenerationJob.id)
                    .where(GenerationIdentityUse.identity_asset_id == identity.id)
                ):
                    asset.quarantined_at = now
                    asset.quarantine_reason = f"complaint:{case.public_reference}"
            case.status = ComplaintStatus.IDENTITY_FROZEN.value
            case.identity_frozen_at = now
        self.outbox.add(session, event_envelope(
            event_id=uuid4(), event_type="complaint.takedown.requested",
            aggregate_id=case.id, occurred_at=now,
            initiated_by_actor_id=self.system_actor.actor_id,
            idempotency_key=f"complaint:{case.id}:takedown",
            payload={
                "complaint_id": case.id, "severity": severity,
            },
        ))
        self.audit.write(
            session, actor=self.system_actor, action="complaint.received",
            target_type="complaint_case", target_id=case.id, project_id=project_id,
            metadata={"severity": severity, "source_ip_hash": self.limiter.hash_ip(source_ip)},
        )
        return PublicComplaintReceipt(case.public_reference, case.status)
~~~

Bind the limiter to Redis with an atomic Lua increment/expiry. If Redis is unavailable, accept the first report into PostgreSQL with a database uniqueness window and fail closed only for subsequent flood traffic. Encrypt contact details before insert; never put them in audit or outbox. Reviewer resolution must be a separate PLATFORM_REVIEWER action with a reason and evidence; only an upheld/rejected decision can close a case, and reactivation after rejection requires a fresh current consent/QC/label check.

- [ ] **Step 4: Add strict trash, restore, and permanent-confirmation schemas**

Append to backend/src/ip_saas/modules/governance/schemas.py:

~~~python
class TrashCreate(StrictModel):
    target_type: Literal[
        "project", "content", "media_asset", "identity_subject",
        "provider_identity_asset",
    ]
    target_id: UUID
    reason: str = Field(min_length=1, max_length=500)


class PermanentDeletionPrepare(StrictModel):
    reason: str = Field(min_length=1, max_length=500)


class PermanentDeletionConfirm(StrictModel):
    confirmation_token: str = Field(min_length=40, max_length=200)
~~~

No request may supply retention days, backup expiry, TOS keys, cache keys, provider references, or a deletion manifest.

- [ ] **Step 5: Write failing global recycle-bin tests**

Create backend/tests/integration/governance/test_global_deletion.py:

~~~python
def test_normal_delete_is_hidden_but_recoverable_before_expiry(deletion_harness) -> None:
    plan = deletion_harness.trash("content")
    assert plan.status == "trashed"
    assert not deletion_harness.visible_to_user(plan.target_id)
    restored = deletion_harness.restore(plan.id)
    assert restored.status == "restored"
    assert deletion_harness.visible_to_user(plan.target_id)


def test_permanent_delete_requires_new_one_time_confirmation(deletion_harness) -> None:
    plan = deletion_harness.trash("project")
    challenge = deletion_harness.prepare_permanent(plan.id)
    with pytest.raises(Forbidden, match="confirmation"):
        deletion_harness.confirm_permanent(plan.id, "typed project name")
    pending = deletion_harness.confirm_permanent(plan.id, challenge.token)
    assert pending.status == "purge_pending"
    with pytest.raises(Conflict, match="already consumed"):
        deletion_harness.confirm_permanent(plan.id, challenge.token)


def test_purge_records_every_system_and_waits_for_backup_expiry(deletion_harness) -> None:
    plan = deletion_harness.confirmed_plan("provider_identity_asset")
    deletion_harness.run_worker(plan.id)
    assert deletion_harness.step_states(plan.id) == {
        "database": "succeeded",
        "tos": "succeeded",
        "provider_model": "succeeded",
        "cache": "succeeded",
        "backup_retention": "natural_expiry_pending",
    }
    assert deletion_harness.provider_reference_ciphertext(plan.target_id) is None
    assert deletion_harness.live_tos_versions(plan.target_id) == []
~~~

- [ ] **Step 6: Run deletion tests and confirm the service is missing**

Run:

~~~bash
cd backend && uv run pytest tests/integration/governance/test_global_deletion.py -v
~~~

Expected: FAIL during collection because governance.deletion does not exist.

- [ ] **Step 7: Implement deletion policy, target adapters, and second confirmation**

Create backend/src/ip_saas/modules/governance/deletion.py:

~~~python
from dataclasses import dataclass
from datetime import timedelta
from hashlib import sha256
import json
from secrets import token_urlsafe
from typing import Mapping, Protocol
from uuid import UUID, uuid4

from sqlalchemy import select

from ip_saas.common.errors import Conflict, Forbidden, NotFound
from ip_saas.modules.accounts.context import ActorKind

from .enums import DeletionPlanStatus
from .events import event_envelope
from .models import DeletionPlan


@dataclass(frozen=True)
class DeletionPolicy:
    recycle_days: int = 30
    backup_retention_days: int = 35

    def __post_init__(self) -> None:
        if self.recycle_days < 1 or self.backup_retention_days < self.recycle_days:
            raise ValueError("backup retention must cover the recycle window")


@dataclass(frozen=True)
class PermanentDeletionChallenge:
    plan_id: UUID
    token: str
    expires_at: object


class DeletionTargetAdapter(Protocol):
    target_type: str
    def build_manifest(self, session, project_id: UUID, target_id: UUID) -> dict: ...
    def trash_live(self, session, manifest: Mapping[str, object], now) -> None: ...
    def restore_live(self, session, manifest: Mapping[str, object], now) -> None: ...
    def purge_database(self, session, manifest: Mapping[str, object], now) -> dict: ...
    def purge_tos(self, manifest: Mapping[str, object]) -> dict: ...
    def purge_provider(self, manifest: Mapping[str, object]) -> dict: ...
    def purge_cache(self, manifest: Mapping[str, object]) -> dict: ...


def canonical_manifest(value: Mapping[str, object]) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True,
                      separators=(",", ":")).encode("utf-8")


class GlobalDeletionService:
    def __init__(self, access, adapters, policy, clock, outbox, audit) -> None:
        self.access = access
        self.adapters = adapters
        self.policy = policy
        self.clock = clock
        self.outbox = outbox
        self.audit = audit

    def trash(self, session, actor, project_id: UUID, command) -> DeletionPlan:
        if actor.kind not in {ActorKind.C_USER, ActorKind.PLATFORM_OPERATOR,
                              ActorKind.PLATFORM_ADMIN}:
            raise Forbidden("project owner identity is required")
        project = self.access.require_editor(session, actor, project_id)
        adapter = self.adapters.require(command.target_type)
        manifest = adapter.build_manifest(session, project.id, command.target_id)
        encoded = canonical_manifest(manifest)
        now = self.clock.now()
        plan = DeletionPlan(
            account_id=project.owner_account_id, project_id=project.id,
            target_type=command.target_type, target_id=command.target_id,
            status=DeletionPlanStatus.TRASHED.value,
            deletion_manifest=manifest, manifest_sha256=sha256(encoded).hexdigest(),
            trashed_by=actor.actor_id, trashed_at=now,
            restore_until=now + timedelta(days=self.policy.recycle_days),
            backup_expire_by=now + timedelta(days=self.policy.backup_retention_days),
        )
        session.add(plan)
        session.flush()
        adapter.trash_live(session, manifest, now)
        self.audit.write(session, actor=actor, action="deletion.trashed",
                         target_type="deletion_plan", target_id=plan.id,
                         project_id=project.id,
                         metadata={"target_type": command.target_type})
        return plan

    def restore(self, session, actor, plan_id: UUID) -> DeletionPlan:
        plan = self._lock_owned(session, actor, plan_id)
        now = self.clock.now()
        if plan.status != DeletionPlanStatus.TRASHED.value or now >= plan.restore_until:
            raise Conflict("recycle entry is no longer recoverable")
        self.adapters.require(plan.target_type).restore_live(
            session, plan.deletion_manifest, now
        )
        plan.status = DeletionPlanStatus.RESTORED.value
        plan.restored_by = actor.actor_id
        plan.restored_at = now
        return plan

    def prepare_permanent(self, session, actor, plan_id: UUID):
        plan = self._lock_owned(session, actor, plan_id)
        if plan.status != DeletionPlanStatus.TRASHED.value:
            raise Conflict("only a recycle entry can be permanently deleted")
        token = token_urlsafe(32)
        plan.permanent_confirmation_sha256 = sha256(token.encode()).hexdigest()
        plan.purge_requested_at = self.clock.now()
        return PermanentDeletionChallenge(
            plan.id, token, self.clock.now() + timedelta(minutes=15)
        )

    def confirm_permanent(self, session, actor, plan_id: UUID,
                          token: str) -> DeletionPlan:
        plan = self._lock_owned(session, actor, plan_id)
        now = self.clock.now()
        if plan.status != DeletionPlanStatus.TRASHED.value:
            raise Conflict("confirmation token was already consumed")
        if (plan.purge_requested_at is None
                or now > plan.purge_requested_at + timedelta(minutes=15)
                or plan.permanent_confirmation_sha256
                   != sha256(token.encode()).hexdigest()):
            raise Forbidden("permanent deletion confirmation is invalid or expired")
        plan.status = DeletionPlanStatus.PURGE_PENDING.value
        plan.purge_requested_by = actor.actor_id
        plan.permanent_confirmation_sha256 = None
        self.outbox.add(session, event_envelope(
            event_id=uuid4(), event_type="deletion.plan.confirmed",
            aggregate_id=plan.id, occurred_at=now,
            initiated_by_actor_id=actor.actor_id,
            idempotency_key=f"deletion:{plan.id}:confirmed",
            payload={
                "deletion_plan_id": plan.id,
            },
        ))
        return plan

    def _lock_owned(self, session, actor, plan_id):
        plan = session.scalar(select(DeletionPlan).where(
            DeletionPlan.id == plan_id,
            DeletionPlan.account_id == actor.account_id,
        ).with_for_update())
        if plan is None:
            raise NotFound("deletion plan")
        self.access.require_editor(session, actor, plan.project_id)
        return plan
~~~

Implement five adapters for project, content, media asset, identity subject, and provider identity asset. `build_manifest` resolves exact DB identifiers, all TOS bucket/key/version tuples, local provider-identity IDs, and versioned cache namespaces server-side. `trash_live` only soft-hides and freezes. `restore_live` reverses those markers only while consent and rights remain valid. `purge_database` redacts user content but retains the minimum immutable audit/tombstone keys; `purge_tos` lists and deletes every version plus delete marker; `purge_provider` invokes the capability adapter and verifies its receipt; `purge_cache` deletes versioned keys. No adapter accepts a path, object key, provider reference, or cache pattern supplied by the browser.

- [ ] **Step 8: Implement idempotent deletion execution and identity-withdrawal bridging**

Modify backend/src/ip_saas/workers/identity_deletion.py so an IdentityDeletionRequest first creates or reuses a trashed DeletionPlan and notifies the owner that permanent provider deletion requires the one-time second confirmation. It must not call the provider before confirmation.

The same worker consumes `deletion.plan.confirmed`. Claim with `FOR UPDATE SKIP LOCKED`, set PURGING by compare-and-swap, and run these exact step names in order: database, tos, provider_model, cache. For each system, create a DeletionExecutionStep attempt, store only a redacted receipt hash/evidence asset, and retry idempotently. After all four live systems succeed, set the plan PURGED, retain its tombstone, add a BACKUP_RETENTION step in NATURAL_EXPIRY_PENDING, and expose `backup_expire_by` to the owner. A daily verifier changes that step to SUCCEEDED only after the configured backup catalog proves the final containing snapshot expired. Never rewrite or prune the immutable audit trail.

- [ ] **Step 9: Prove restored backups cannot resurrect purged data**

Create backend/tests/resilience/test_deleted_data_not_restored.py:

~~~python
def test_restore_applies_persistent_deletion_tombstones(real_restore_harness) -> None:
    purge = real_restore_harness.purge_target_present_in_backup()
    restored = real_restore_harness.restore_snapshot_to_new_database()
    restored.apply_deletion_tombstones()
    assert not restored.contains_user_payload(purge.target_type, purge.target_id)
    assert restored.contains_tombstone(purge.target_type, purge.target_id)
    assert restored.cache_keys_for(purge.target_id) == []
    assert restored.tos_versions_for(purge.target_id) == []
~~~

The restore tool must run the tombstone reconciliation before opening traffic. A missing or mismatched DeletionPlan manifest hash aborts the restore; it never silently resurrects the payload.

- [ ] **Step 10: Run complaint, deletion, and resurrection tests**

Run:

~~~bash
cd backend && uv run pytest tests/integration/governance/test_complaint_takedown.py tests/integration/governance/test_global_deletion.py tests/resilience/test_deleted_data_not_restored.py -v
~~~

Expected: PASS. Ordinary delete restores; permanent delete rejects absent/expired/reused confirmation; provider ciphertext and all TOS versions disappear; backup content remains inaccessible and expires according to retention.

- [ ] **Step 11: Commit complaint, recovery, and permanent deletion controls**

~~~bash
git add backend/src/ip_saas/modules/governance/schemas.py backend/src/ip_saas/modules/governance/complaints.py backend/src/ip_saas/modules/governance/deletion.py backend/src/ip_saas/workers/identity_deletion.py backend/tests/integration/governance/test_complaint_takedown.py backend/tests/integration/governance/test_global_deletion.py backend/tests/resilience/test_deleted_data_not_restored.py
git commit -m "feat: add takedown and recoverable global deletion"
~~~

### Task 10: Gate high-risk platform self-marketing with independent review

**Files:**
- Create: backend/src/ip_saas/modules/governance/self_marketing_review.py
- Modify: backend/src/ip_saas/modules/publication/service.py
- Test: backend/tests/integration/governance/test_self_marketing_review.py

- [ ] **Step 1: Write failing normal-versus-high-risk publication tests**

Create backend/tests/integration/governance/test_self_marketing_review.py:

~~~python
def test_ordinary_self_marketing_needs_common_gates_but_not_reviewer(
    self_marketing_harness,
) -> None:
    candidate = self_marketing_harness.candidate(risks=[])
    self_marketing_harness.assert_common_gates_checked(candidate)
    self_marketing_harness.require_ready(candidate)
    assert self_marketing_harness.risk_review(candidate.prediction_id) is None


@pytest.mark.parametrize("risk", [
    "high_risk_marketing_claim", "authorized_real_person",
    "authorized_customer_case", "major_controversy",
])
def test_each_high_risk_signal_requires_independent_platform_reviewer(
    self_marketing_harness, risk,
) -> None:
    candidate = self_marketing_harness.candidate(risks=[risk])
    with pytest.raises(Forbidden, match="independent platform review"):
        self_marketing_harness.require_ready(candidate)
    self_marketing_harness.review(candidate, decision="approved")
    self_marketing_harness.require_ready(candidate)


def test_operator_reviewer_and_budget_approver_must_be_three_people(
    self_marketing_harness,
) -> None:
    candidate = self_marketing_harness.candidate(
        risks=["authorized_real_person"]
    )
    for duplicated_role in ("operator", "budget_approver"):
        with pytest.raises(Forbidden, match="three distinct principals"):
            self_marketing_harness.review(
                candidate, reviewer=self_marketing_harness.actor(duplicated_role)
            )


def test_platform_admin_cannot_bypass_fact_rights_qc_or_budget(self_marketing_harness) -> None:
    candidate = self_marketing_harness.candidate(
        risks=["high_risk_marketing_claim"], fact_pass=False, rights_pass=False,
        qc_pass=False, budget_pass=False,
    )
    with pytest.raises(Forbidden):
        self_marketing_harness.require_ready(
            candidate, actor=self_marketing_harness.platform_admin
        )
~~~

- [ ] **Step 2: Run the tests and verify the review service is missing**

Run:

~~~bash
cd backend && uv run pytest tests/integration/governance/test_self_marketing_review.py -v
~~~

Expected: FAIL during collection because governance.self_marketing_review does not exist.

- [ ] **Step 3: Implement separately signed internal budget approval**

Create backend/src/ip_saas/modules/governance/self_marketing_review.py with these value types and service methods:

~~~python
from dataclasses import dataclass
from datetime import timedelta
from hashlib import sha256
import json

from sqlalchemy import select

from ip_saas.common.errors import Conflict, Forbidden, NotFound
from ip_saas.modules.accounts.context import ActorKind
from ip_saas.modules.projects.models import IPProject

from .models import SelfMarketingBudgetApproval, SelfMarketingRiskReview


HIGH_RISKS = frozenset({
    "high_risk_marketing_claim", "authorized_real_person",
    "authorized_customer_case", "major_controversy",
})


def request_fingerprint(payload: dict[str, object]) -> str:
    encoded = json.dumps(payload, ensure_ascii=False, sort_keys=True,
                         separators=(",", ":")).encode("utf-8")
    return sha256(encoded).hexdigest()


class SelfMarketingBudgetApprovalService:
    def __init__(self, access, clock, audit) -> None:
        self.access = access
        self.clock = clock
        self.audit = audit

    def approve(self, session, approver, project_id, cost_center_id,
                requested_by, frozen_request, max_amount_fen,
                evidence_asset_id):
        project = self.access.require_editor(session, approver, project_id)
        if (project.owner_type != "platform"
                or "budget_approver" not in approver.platform_roles):
            raise Forbidden("platform budget approver is required")
        if requested_by == approver.actor_id:
            raise Forbidden("operator and budget approver must differ")
        now = self.clock.now()
        row = SelfMarketingBudgetApproval(
            project_id=project.id, cost_center_id=cost_center_id,
            request_fingerprint_sha256=request_fingerprint(frozen_request),
            max_amount_fen=max_amount_fen, requested_by=requested_by,
            approved_by=approver.actor_id, approved_at=now,
            valid_until=now + timedelta(days=7), evidence_asset_id=evidence_asset_id,
        )
        session.add(row)
        session.flush()
        self.audit.write(session, actor=approver,
            action="self_marketing.budget.approved",
            target_type="self_marketing_budget_approval", target_id=row.id,
            project_id=project.id,
            metadata={"max_amount_fen": max_amount_fen,
                      "request_fingerprint_sha256": row.request_fingerprint_sha256})
        return row

    def consume(self, session, approval_id, task, frozen_request,
                amount_fen, cost_center_id):
        row = session.scalar(select(SelfMarketingBudgetApproval).where(
            SelfMarketingBudgetApproval.id == approval_id
        ).with_for_update())
        if row is None:
            raise NotFound("self-marketing budget approval")
        if row.consumed_by_task_id is not None:
            if row.consumed_by_task_id == task.id:
                return row
            raise Conflict("budget approval was already consumed")
        if (self.clock.now() >= row.valid_until
                or row.request_fingerprint_sha256 != request_fingerprint(frozen_request)
                or amount_fen > row.max_amount_fen
                or row.cost_center_id != cost_center_id
                or row.project_id != task.project_id):
            raise Forbidden("budget approval does not match this task")
        row.consumed_by_task_id = task.id
        return row
~~~

The platform task orchestration calls `approve` before generation. In the same transaction as `TaskSubmissionService.submit_internal`, it calls `consume`; failure rolls back the InternalBudgetHold, TaskRecord, consumption marker, and outbox together. Customer tasks never accept this approval ID.

- [ ] **Step 4: Derive risks from persisted lineage and enforce the common gates**

Append to backend/src/ip_saas/modules/governance/self_marketing_review.py:

~~~python
@dataclass(frozen=True)
class SelfMarketingSignals:
    risks: frozenset[str]
    operator_actor_id: object
    budget_approval_id: object
    budget_approver_actor_id: object
    fact_passed: bool
    rights_passed: bool
    qc_passed: bool
    disclosure_passed: bool


class SelfMarketingReviewService:
    def __init__(self, repository, access, clock, audit) -> None:
        self.repository = repository
        self.access = access
        self.clock = clock
        self.audit = audit

    def signals(self, session, project_id, prediction_id, media_asset_id):
        return self.repository.derive_signals(
            session, project_id, prediction_id, media_asset_id
        )

    def review(self, session, reviewer, project_id, prediction_id,
               media_asset_id, decision, reason, evidence_asset_id):
        project = self.access.require_viewer(session, reviewer, project_id)
        if reviewer.kind is not ActorKind.PLATFORM_REVIEWER:
            raise Forbidden("independent PLATFORM_REVIEWER is required")
        if project.owner_type != "platform":
            raise Forbidden("self-marketing review requires a platform project")
        signals = self.signals(session, project_id, prediction_id, media_asset_id)
        risks = signals.risks & HIGH_RISKS
        if not risks:
            raise Conflict("ordinary content does not require a high-risk review")
        people = {signals.operator_actor_id, reviewer.actor_id,
                  signals.budget_approver_actor_id}
        if len(people) != 3:
            raise Forbidden("three distinct principals are required")
        row = SelfMarketingRiskReview(
            project_id=project_id, prediction_id=prediction_id,
            media_asset_id=media_asset_id,
            budget_approval_id=signals.budget_approval_id,
            triggered_risks=sorted(risks), decision=decision,
            decision_reason=reason, operator_actor_id=signals.operator_actor_id,
            reviewer_actor_id=reviewer.actor_id,
            budget_approver_actor_id=signals.budget_approver_actor_id,
            evidence_asset_id=evidence_asset_id, reviewed_at=self.clock.now(),
        )
        session.add(row)
        session.flush()
        self.audit.write(session, actor=reviewer,
            action="self_marketing.risk_reviewed",
            target_type="self_marketing_risk_review", target_id=row.id,
            project_id=project_id,
            metadata={"decision": decision, "triggered_risks": sorted(risks)})
        return row

    def require_ready(self, session, project_id, prediction_id, media_asset_id):
        signals = self.signals(session, project_id, prediction_id, media_asset_id)
        if not all((signals.fact_passed, signals.rights_passed,
                    signals.qc_passed, signals.disclosure_passed,
                    signals.budget_approval_id is not None)):
            raise Forbidden("fact, rights, QC, disclosure, and budget gates must pass")
        risks = signals.risks & HIGH_RISKS
        if risks:
            review = session.scalar(select(SelfMarketingRiskReview).where(
                SelfMarketingRiskReview.project_id == project_id,
                SelfMarketingRiskReview.prediction_id == prediction_id,
                SelfMarketingRiskReview.media_asset_id == media_asset_id,
                SelfMarketingRiskReview.decision == "approved",
            ))
            if review is None or set(review.triggered_risks) != risks:
                raise Forbidden("independent platform review is required")
        return signals
~~~

Implement `derive_signals` with project-scoped SQL joins only: Prediction to its frozen MarketingFrame truth_review_payload and TopicCard; optional MediaAsset to QCReport, ExportVerification, GenerationIdentityUse, IdentityAssetConsent, and MediaRightsGrant; TaskRecord to the consumed SelfMarketingBudgetApproval. Mark the four risks from server-owned evidence, never an HTTP boolean. A customer-case signal requires a current explicit case-use rights grant and public/evidence copy inside the platform project; it never reads a customer's private project.

- [ ] **Step 5: Put the gate before Plan 04 manual publication recording**

Modify backend/src/ip_saas/modules/publication/service.py. Immediately before creating Publication, load the persisted IPProject. For owner_type=platform call:

~~~python
self.self_marketing_review.require_ready(
    session,
    project_id=project.id,
    prediction_id=command.prediction_id,
    media_asset_id=command.final_media_asset_id,
)
~~~

For customer projects, retain the Plan 04 behavior. This is not automatic publishing: the operator still publishes manually and records the result. A high-risk platform candidate cannot enter the application's “待发布” view until the review passes; an ordinary candidate does not wait for PLATFORM_REVIEWER but still passes the common gates.

- [ ] **Step 6: Run separation and bypass tests**

Run:

~~~bash
cd backend && uv run pytest tests/integration/governance/test_self_marketing_review.py tests/integration/publication/test_publication_service.py -v
~~~

Expected: PASS. Four risk types require an independent reviewer; ordinary content does not; admin, operator, reviewer, or budget roles alone cannot bypass truth, rights, QC, disclosure, or internal-budget checks.

- [ ] **Step 7: Commit the self-marketing responsibility gate**

~~~bash
git add backend/src/ip_saas/modules/governance/self_marketing_review.py backend/src/ip_saas/modules/publication/service.py backend/tests/integration/governance/test_self_marketing_review.py
git commit -m "feat: require independent review for risky self marketing"
~~~

### Task 11: Extend synchronous APIs, event contracts, and explicit Beta UI

**Files:**
- Modify: backend/src/ip_saas/modules/governance/router.py
- Modify: backend/src/ip_saas/modules/governance/events.py
- Modify: backend/src/ip_saas/api.py
- Modify: backend/src/ip_saas/scripts/export_contracts.py
- Create: contracts/events/identity.enrollment.requested.v1.json
- Create: contracts/events/identity.deletion.requested.v1.json
- Create: contracts/events/governed.media.requested.v1.json
- Create: contracts/events/complaint.takedown.requested.v1.json
- Create: contracts/events/deletion.plan.confirmed.v1.json
- Create: contracts/events/capability.rollback.requested.v1.json
- Create: contracts/events/capability.release.executed.v1.json
- Modify: contracts/openapi.json
- Modify: frontend/src/lib/api/schema.d.ts
- Create: frontend/src/features/governance/types.ts
- Create: frontend/src/features/governance/api.ts
- Create: frontend/src/features/governance/ConsentWizard.tsx
- Create: frontend/src/features/governance/IdentityAssetPanel.tsx
- Create: frontend/src/features/governance/CapabilityBadge.tsx
- Create: frontend/src/features/governance/ComplaintStatus.tsx
- Create: frontend/src/features/governance/ReleaseGateBoard.tsx
- Create: frontend/src/app/(c-user)/projects/[projectId]/identity/page.tsx
- Create: frontend/src/app/(platform)/platform/governance/page.tsx
- Create: frontend/src/app/(platform)/platform/public-beta/page.tsx
- Test: backend/tests/contract/governance/test_openapi.py
- Test: backend/tests/contract/governance/test_event_schemas.py
- Test: backend/tests/contract/governance/test_event_consumers.py
- Test: frontend/src/features/governance/__tests__/ConsentWizard.test.tsx
- Test: frontend/src/features/governance/__tests__/IdentityAssetPanel.test.tsx

- [ ] **Step 1: Write failing OpenAPI and provider-reference leak tests**

Create backend/tests/contract/governance/test_openapi.py:

~~~python
FORBIDDEN_PUBLIC_TERMS = {
    "provider_resource_ref", "provider_asset_id", "speaker_id", "asset://",
    "access_token", "secret_key",
}


def test_governance_routes_are_sync_receipt_endpoints(app_openapi) -> None:
    paths = app_openapi["paths"]
    assert "/v1/projects/{project_id}/identity/enrollments" in paths
    assert "/v1/projects/{project_id}/identity/generations" in paths
    assert "/v1/public/identity-complaints" in paths
    assert paths["/v1/projects/{project_id}/identity/enrollments"]["post"][
        "responses"
    ]["202"]


def test_openapi_never_exposes_provider_identity_references(app_openapi) -> None:
    rendered = json.dumps(app_openapi, ensure_ascii=False)
    assert all(term not in rendered for term in FORBIDDEN_PUBLIC_TERMS)


def test_no_route_claims_general_availability(app_openapi) -> None:
    rendered = json.dumps(app_openapi, ensure_ascii=False).lower()
    assert '"ga"' not in rendered
    assert "service level agreement" not in rendered
~~~

- [ ] **Step 2: Run contract tests and verify routes are absent**

Run:

~~~bash
cd backend && uv run pytest tests/contract/governance/test_openapi.py tests/contract/governance/test_event_schemas.py -v
~~~

Expected: FAIL because governance routes and event schema files do not exist.

- [ ] **Step 3: Implement sync FastAPI routes using the frozen dependencies**

Create backend/src/ip_saas/modules/governance/router.py. All ordinary handlers use synchronous `def`, SQLAlchemy 2.0 `Session`, `ip_saas.db.session.get_session`, and `ip_saas.modules.accounts.dependencies.get_actor`; long work returns 202 and runs in RocketMQ workers.

~~~python
from uuid import UUID

from fastapi import APIRouter, Depends, Request, status
from sqlalchemy.orm import Session

from ip_saas.db.session import get_session
from ip_saas.modules.accounts.context import ActorContext
from ip_saas.modules.accounts.dependencies import get_actor

from .schemas import (
    ComplaintCreate, ConsentCreate, ConsentWithdraw, GovernedGenerationCreate,
    IdentityEnrollmentCreate, IdentitySubjectCreate, PermanentDeletionConfirm,
    PermanentDeletionPrepare, TrashCreate,
)

project_router = APIRouter(
    prefix="/v1/projects/{project_id}/identity", tags=["identity-public-beta"]
)
public_router = APIRouter(prefix="/v1/public", tags=["identity-safety"])
platform_router = APIRouter(prefix="/v1/platform/governance", tags=["governance"])


@project_router.post("/subjects", status_code=status.HTTP_201_CREATED)
def create_subject(project_id: UUID, body: IdentitySubjectCreate,
                   request: Request, session: Session = Depends(get_session),
                   actor: ActorContext = Depends(get_actor)):
    return request.app.state.consent_service.create_subject(
        session, actor, project_id, body
    )


@project_router.post("/consents", status_code=status.HTTP_201_CREATED)
def grant_consent(project_id: UUID, body: ConsentCreate, request: Request,
                  session: Session = Depends(get_session),
                  actor: ActorContext = Depends(get_actor)):
    return request.app.state.consent_service.grant(session, actor, project_id, body)


@project_router.post("/consents/{consent_id}/withdraw")
def withdraw_consent(project_id: UUID, consent_id: UUID, body: ConsentWithdraw,
                     request: Request, session: Session = Depends(get_session),
                     actor: ActorContext = Depends(get_actor)):
    return request.app.state.revocation_service.withdraw(
        session, actor, project_id, consent_id,
        reason=body.reason,
        request_provider_deletion=body.request_provider_deletion,
    )


@project_router.post("/enrollments", status_code=status.HTTP_202_ACCEPTED)
def enroll_identity(project_id: UUID, body: IdentityEnrollmentCreate,
                    request: Request, session: Session = Depends(get_session),
                    actor: ActorContext = Depends(get_actor)):
    quote, route = request.app.state.identity_quote_service.enrollment(
        session, actor, project_id, body
    )
    result = request.app.state.enrollment_service.create(
        session, actor, project_id, body, quote, route
    )
    return {"task_id": result.task_record.id,
            "identity_asset_id": result.identity_asset.id,
            "status": result.enrollment.status, "beta": True}


@project_router.post("/generations", status_code=status.HTTP_202_ACCEPTED)
def generate_identity_media(project_id: UUID, body: GovernedGenerationCreate,
                            request: Request,
                            session: Session = Depends(get_session),
                            actor: ActorContext = Depends(get_actor)):
    quote, route = request.app.state.identity_quote_service.generation(
        session, actor, project_id, body
    )
    job = request.app.state.governed_generation_service.create(
        session, actor, project_id, body, quote, route
    )
    return {"task_id": job.task_record_id, "generation_job_id": job.id,
            "status": job.provider_status, "beta": True}


@project_router.post("/trash", status_code=status.HTTP_201_CREATED)
def trash(project_id: UUID, body: TrashCreate, request: Request,
          session: Session = Depends(get_session),
          actor: ActorContext = Depends(get_actor)):
    return request.app.state.deletion_service.trash(
        session, actor, project_id, body
    )


@public_router.post("/identity-complaints", status_code=status.HTTP_202_ACCEPTED)
def complain(body: ComplaintCreate, request: Request):
    return request.app.state.complaint_service.receive_public(
        body, request.client.host if request.client else "unknown"
    )
~~~

Add authenticated GET list/detail routes with project/account filters, recycle-bin GET/restore/prepare-permanent/confirm-permanent routes, and the public opaque complaint-status route. Add PLATFORM_ADMIN read-only flag plus cohort/gate/entitlement evidence routes, PLATFORM_REVIEWER complaint and self-marketing review routes, and budget-approver routes. Task 10 exposes no generic `stage` setter and no repository-level flag mutation; only Task 16's capability-specific promote/rollback endpoints may change a flag, through its release-control wrapper and `CapabilityFeatureFlagService.set_stage`. Every mutation writes AuditWriter and uses optimistic version or `FOR UPDATE`; list responses omit consent evidence bytes, contact data, provider refs, source signed URLs, and deletion manifests.

- [ ] **Step 4: Register routers and prove no async Session was introduced**

Modify backend/src/ip_saas/api.py:

~~~python
from ip_saas.modules.governance.router import (
    platform_router as governance_platform_router,
    project_router as governance_project_router,
    public_router as governance_public_router,
)

app.include_router(governance_project_router)
app.include_router(governance_public_router)
app.include_router(governance_platform_router)
~~~

Run:

~~~bash
rg -n "AsyncSession|async def" backend/src/ip_saas/modules/governance backend/src/ip_saas/workers
~~~

Expected: no output. Provider calls and FFmpeg remain in workers, never in request handlers.

- [ ] **Step 5: Freeze seven typed event payloads, contracts, and consumers**

Replace the initial producer-only `backend/src/ip_saas/modules/governance/events.py` with the following complete contract boundary. These seven names are the entire new Plan 06 event surface; Plan 01 task and Plan 03 media events remain owned by those plans:

~~~python
from collections.abc import Callable, Mapping
from datetime import datetime
from typing import Annotated, Literal, TypeAlias, Union
from uuid import UUID

from pydantic import (
    BaseModel,
    ConfigDict,
    Field,
    TypeAdapter,
    model_validator,
)

from ip_saas.common.outbox import EventEnvelope

from .enums import BetaCapability


class StrictEventPayload(BaseModel):
    model_config = ConfigDict(extra="forbid")


class IdentityEnrollmentRequestedPayloadV1(StrictEventPayload):
    enrollment_id: UUID
    identity_asset_id: UUID
    provider_verification_receipt_asset_id: UUID | None = None


class GovernedMediaRequestedPayloadV1(StrictEventPayload):
    task_id: UUID
    request_fingerprint: str = Field(pattern=r"^[0-9a-f]{64}$")
    generation_job_id: UUID
    identity_asset_ids: list[UUID] = Field(min_length=1, max_length=1)


class IdentityDeletionRequestedPayloadV1(StrictEventPayload):
    deletion_request_id: UUID
    identity_asset_id: UUID


class ComplaintTakedownRequestedPayloadV1(StrictEventPayload):
    complaint_id: UUID
    severity: Literal["critical", "high", "normal"]


class DeletionPlanConfirmedPayloadV1(StrictEventPayload):
    deletion_plan_id: UUID


class CapabilityRollbackRequestedPayloadV1(StrictEventPayload):
    capability: BetaCapability
    flag_version: int = Field(ge=1)
    feature_flag_id: UUID


class CapabilityReleaseExecutedPayloadV1(StrictEventPayload):
    capability: BetaCapability
    flag_version: int = Field(ge=1)
    release_decision_id: UUID


class IdentityEnrollmentRequestedEventV1(EventEnvelope):
    event_type: Literal["identity.enrollment.requested"] = (
        "identity.enrollment.requested"
    )
    schema_version: Literal[1] = 1
    payload: IdentityEnrollmentRequestedPayloadV1

    @model_validator(mode="after")
    def aggregate_is_enrollment(self) -> "IdentityEnrollmentRequestedEventV1":
        if self.aggregate_id != self.payload.enrollment_id:
            raise ValueError("aggregate_id must equal enrollment_id")
        return self


class GovernedMediaRequestedEventV1(EventEnvelope):
    event_type: Literal["governed.media.requested"] = "governed.media.requested"
    schema_version: Literal[1] = 1
    payload: GovernedMediaRequestedPayloadV1

    @model_validator(mode="after")
    def aggregate_is_generation_job(self) -> "GovernedMediaRequestedEventV1":
        if self.aggregate_id != self.payload.generation_job_id:
            raise ValueError("aggregate_id must equal generation_job_id")
        return self


class IdentityDeletionRequestedEventV1(EventEnvelope):
    event_type: Literal["identity.deletion.requested"] = (
        "identity.deletion.requested"
    )
    schema_version: Literal[1] = 1
    payload: IdentityDeletionRequestedPayloadV1

    @model_validator(mode="after")
    def aggregate_is_deletion_request(self) -> "IdentityDeletionRequestedEventV1":
        if self.aggregate_id != self.payload.deletion_request_id:
            raise ValueError("aggregate_id must equal deletion_request_id")
        return self


class ComplaintTakedownRequestedEventV1(EventEnvelope):
    event_type: Literal["complaint.takedown.requested"] = (
        "complaint.takedown.requested"
    )
    schema_version: Literal[1] = 1
    payload: ComplaintTakedownRequestedPayloadV1

    @model_validator(mode="after")
    def aggregate_is_complaint(self) -> "ComplaintTakedownRequestedEventV1":
        if self.aggregate_id != self.payload.complaint_id:
            raise ValueError("aggregate_id must equal complaint_id")
        return self


class DeletionPlanConfirmedEventV1(EventEnvelope):
    event_type: Literal["deletion.plan.confirmed"] = "deletion.plan.confirmed"
    schema_version: Literal[1] = 1
    payload: DeletionPlanConfirmedPayloadV1

    @model_validator(mode="after")
    def aggregate_is_deletion_plan(self) -> "DeletionPlanConfirmedEventV1":
        if self.aggregate_id != self.payload.deletion_plan_id:
            raise ValueError("aggregate_id must equal deletion_plan_id")
        return self


class CapabilityRollbackRequestedEventV1(EventEnvelope):
    event_type: Literal["capability.rollback.requested"] = (
        "capability.rollback.requested"
    )
    schema_version: Literal[1] = 1
    payload: CapabilityRollbackRequestedPayloadV1

    @model_validator(mode="after")
    def aggregate_is_feature_flag(self) -> "CapabilityRollbackRequestedEventV1":
        if self.aggregate_id != self.payload.feature_flag_id:
            raise ValueError("aggregate_id must equal feature_flag_id")
        return self


class CapabilityReleaseExecutedEventV1(EventEnvelope):
    event_type: Literal["capability.release.executed"] = (
        "capability.release.executed"
    )
    schema_version: Literal[1] = 1
    payload: CapabilityReleaseExecutedPayloadV1

    @model_validator(mode="after")
    def aggregate_is_release_decision(self) -> "CapabilityReleaseExecutedEventV1":
        if self.aggregate_id != self.payload.release_decision_id:
            raise ValueError("aggregate_id must equal release_decision_id")
        return self


EVENT_PAYLOAD_MODELS: dict[str, type[StrictEventPayload]] = {
    "identity.enrollment.requested": IdentityEnrollmentRequestedPayloadV1,
    "governed.media.requested": GovernedMediaRequestedPayloadV1,
    "identity.deletion.requested": IdentityDeletionRequestedPayloadV1,
    "complaint.takedown.requested": ComplaintTakedownRequestedPayloadV1,
    "deletion.plan.confirmed": DeletionPlanConfirmedPayloadV1,
    "capability.rollback.requested": CapabilityRollbackRequestedPayloadV1,
    "capability.release.executed": CapabilityReleaseExecutedPayloadV1,
}
EVENT_ENVELOPE_MODELS: dict[str, type[EventEnvelope]] = {
    "identity.enrollment.requested": IdentityEnrollmentRequestedEventV1,
    "governed.media.requested": GovernedMediaRequestedEventV1,
    "identity.deletion.requested": IdentityDeletionRequestedEventV1,
    "complaint.takedown.requested": ComplaintTakedownRequestedEventV1,
    "deletion.plan.confirmed": DeletionPlanConfirmedEventV1,
    "capability.rollback.requested": CapabilityRollbackRequestedEventV1,
    "capability.release.executed": CapabilityReleaseExecutedEventV1,
}
EVENT_SCHEMA_FILES = {
    name: f"{name}.v1.json" for name in EVENT_ENVELOPE_MODELS
}
EVENT_CONSUMER_KEYS = {
    "identity.enrollment.requested": "identity_enrollment_worker",
    "governed.media.requested": "governance_lineage_projection",
    "identity.deletion.requested": "identity_deletion_worker",
    "complaint.takedown.requested": "complaint_takedown_worker",
    "deletion.plan.confirmed": "global_deletion_worker",
    "capability.rollback.requested": "capability_rollback_worker",
    "capability.release.executed": "release_evidence_projection",
}


GovernanceEventV1: TypeAlias = Annotated[
    Union[
        IdentityEnrollmentRequestedEventV1,
        GovernedMediaRequestedEventV1,
        IdentityDeletionRequestedEventV1,
        ComplaintTakedownRequestedEventV1,
        DeletionPlanConfirmedEventV1,
        CapabilityRollbackRequestedEventV1,
        CapabilityReleaseExecutedEventV1,
    ],
    Field(discriminator="event_type"),
]
GOVERNANCE_EVENT_ADAPTER = TypeAdapter(GovernanceEventV1)


def parse_governance_event(
    raw_envelope: Mapping[str, object] | str | bytes,
) -> GovernanceEventV1:
    if isinstance(raw_envelope, (str, bytes)):
        base = EventEnvelope.model_validate_json(raw_envelope)
    else:
        base = EventEnvelope.model_validate(raw_envelope)
    return GOVERNANCE_EVENT_ADAPTER.validate_python(
        base.model_dump(mode="python")
    )


def event_envelope(
    *,
    event_type: str,
    event_id: UUID,
    aggregate_id: UUID,
    occurred_at: datetime,
    initiated_by_actor_id: UUID,
    idempotency_key: str,
    payload: Mapping[str, object],
) -> GovernanceEventV1:
    return parse_governance_event({
        "event_id": event_id,
        "event_type": event_type,
        "schema_version": 1,
        "aggregate_id": aggregate_id,
        "occurred_at": occurred_at,
        "initiated_by_actor_id": initiated_by_actor_id,
        "idempotency_key": idempotency_key,
        "payload": dict(payload),
    })


class GovernanceEventDispatcher:
    def __init__(
        self,
        handlers: Mapping[str, Callable[[GovernanceEventV1], None]],
    ) -> None:
        if set(handlers) != set(EVENT_CONSUMER_KEYS):
            raise ValueError("exactly one handler per governance event is required")
        self.handlers = dict(handlers)

    def consume(self, raw_envelope: Mapping[str, object] | str | bytes) -> None:
        envelope = parse_governance_event(raw_envelope)
        self.handlers[envelope.event_type](envelope)
~~~

Each of the seven event-specific JSON files is the complete typed `EventEnvelope` structural schema, including strict outer fields, literal `event_type`, explicit literal `schema_version=1`, and the strict typed payload; it is not a payload-only schema and does not need composition with `event-envelope.v1.json`. Aggregate-to-domain-ID equality is a semantic invariant enforced by each typed envelope's `model_validator` and by consumer contract tests rather than an equality keyword in JSON Schema. `parse_governance_event` deliberately validates the raw message with Plan 01's strict `EventEnvelope` first, then validates the discriminated event-specific envelope, payload, and aggregate invariant. Every producer calls `event_envelope`; every consumer calls `GovernanceEventDispatcher.consume(raw_envelope)`. No producer or consumer accepts a separate trusted `event_type` plus a bare payload.

Modify Plan 01's sole deterministic exporter, `backend/src/ip_saas/scripts/export_contracts.py`, rather than adding a second event writer:

~~~python
from ip_saas.modules.governance.events import (
    EVENT_ENVELOPE_MODELS,
    EVENT_SCHEMA_FILES,
)


for event_type, envelope_type in EVENT_ENVELOPE_MODELS.items():
    complete_envelope_schema = {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        **envelope_type.model_json_schema(mode="validation"),
    }
    write_or_check(
        ROOT / "contracts" / "events" / EVENT_SCHEMA_FILES[event_type],
        rendered(complete_envelope_schema),
        args.check,
    )
~~~

Place this loop inside `main()` after the existing Plan 01/03/04 event exports. `EVENT_SCHEMA_FILES` is the only filename source for writing and checking the seven files; no script spells a second filename mapping.

Create backend/tests/contract/governance/test_event_schemas.py and test_event_consumers.py. Freeze valid payloads inline with deterministic UUIDs; do not add another fixture:

~~~python
import json
from pathlib import Path
from uuid import UUID

import pytest
from pydantic import ValidationError

from ip_saas.modules.governance.events import (
    EVENT_CONSUMER_KEYS,
    EVENT_ENVELOPE_MODELS,
    EVENT_PAYLOAD_MODELS,
    EVENT_SCHEMA_FILES,
    GovernanceEventDispatcher,
)


U1 = UUID("00000000-0000-4000-8000-000000000001")
U2 = UUID("00000000-0000-4000-8000-000000000002")
U3 = UUID("00000000-0000-4000-8000-000000000003")
VALID_PAYLOADS = {
    "identity.enrollment.requested": {
        "enrollment_id": U1, "identity_asset_id": U2,
        "provider_verification_receipt_asset_id": None,
    },
    "governed.media.requested": {
        "task_id": U1, "request_fingerprint": "0" * 64,
        "generation_job_id": U2, "identity_asset_ids": [U1],
    },
    "identity.deletion.requested": {
        "deletion_request_id": U1, "identity_asset_id": U2,
    },
    "complaint.takedown.requested": {"complaint_id": U1, "severity": "critical"},
    "deletion.plan.confirmed": {"deletion_plan_id": U1},
    "capability.rollback.requested": {
        "capability": "voice_clone", "flag_version": 1,
        "feature_flag_id": U2,
    },
    "capability.release.executed": {
        "capability": "voice_clone", "flag_version": 1, "release_decision_id": U2,
    },
}
AGGREGATE_FIELDS = {
    "identity.enrollment.requested": "enrollment_id",
    "governed.media.requested": "generation_job_id",
    "identity.deletion.requested": "deletion_request_id",
    "complaint.takedown.requested": "complaint_id",
    "deletion.plan.confirmed": "deletion_plan_id",
    "capability.rollback.requested": "feature_flag_id",
    "capability.release.executed": "release_decision_id",
}


def valid_envelope(event_type: str) -> dict[str, object]:
    payload = dict(VALID_PAYLOADS[event_type])
    aggregate_id = payload[AGGREGATE_FIELDS[event_type]]
    return {
        "event_id": U3,
        "event_type": event_type,
        "schema_version": 1,
        "aggregate_id": aggregate_id,
        "occurred_at": "2026-08-24T00:00:00Z",
        "initiated_by_actor_id": U3,
        "idempotency_key": f"governance:{event_type}:{aggregate_id}",
        "payload": payload,
    }


def test_typed_payloads_and_json_contracts_cannot_drift() -> None:
    root = Path(__file__).parents[4] / "contracts" / "events"
    assert (
        set(EVENT_PAYLOAD_MODELS)
        == set(EVENT_ENVELOPE_MODELS)
        == set(EVENT_CONSUMER_KEYS)
        == set(VALID_PAYLOADS)
    )
    assert EVENT_SCHEMA_FILES == {
        event_type: f"{event_type}.v1.json"
        for event_type in EVENT_ENVELOPE_MODELS
    }
    assert len(set(EVENT_SCHEMA_FILES.values())) == len(EVENT_SCHEMA_FILES)
    checked_in_names = {path.name for path in root.glob("*.json")}
    assert set(EVENT_SCHEMA_FILES.values()) <= checked_in_names
    legacy_hyphenated = {
        f"{event_type.replace('.', '-')}.v1.json"
        for event_type in EVENT_ENVELOPE_MODELS
    }
    assert legacy_hyphenated.isdisjoint(checked_in_names)
    for event_type, envelope_type in EVENT_ENVELOPE_MODELS.items():
        contract = json.loads((root / EVENT_SCHEMA_FILES[event_type]).read_text())
        expected = {
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            **envelope_type.model_json_schema(mode="validation"),
        }
        assert contract == expected
        assert contract["additionalProperties"] is False
        assert contract["properties"]["event_type"]["const"] == event_type
        assert contract["properties"]["schema_version"]["const"] == 1
        payload_ref = contract["properties"]["payload"]["$ref"].split("/")[-1]
        assert contract["$defs"][payload_ref]["additionalProperties"] is False
        with pytest.raises(ValidationError):
            envelope_type.model_validate({
                **valid_envelope(event_type),
                "payload": {
                    **VALID_PAYLOADS[event_type],
                    "unexpected": True,
                },
            })


def test_governance_event_consumers_validate_full_envelope_before_dispatch() -> None:
    seen: list[tuple[str, UUID]] = []
    handlers = {
        name: (
            lambda envelope, event_type=name: seen.append(
                (event_type, envelope.aggregate_id)
            )
        )
        for name in EVENT_CONSUMER_KEYS
    }
    dispatcher = GovernanceEventDispatcher(handlers)
    for event_type in VALID_PAYLOADS:
        dispatcher.consume(valid_envelope(event_type))
    assert [name for name, _aggregate in seen] == list(VALID_PAYLOADS)


def test_unknown_outer_field_is_rejected_by_plan01_envelope_first() -> None:
    seen: list[object] = []
    dispatcher = GovernanceEventDispatcher({
        name: seen.append for name in EVENT_CONSUMER_KEYS
    })
    raw = valid_envelope("identity.deletion.requested")
    raw["unexpected_outer"] = True
    raw["payload"] = {
        **VALID_PAYLOADS["identity.deletion.requested"],
        "provider_resource_ref": "leak",
    }
    with pytest.raises(ValidationError) as caught:
        dispatcher.consume(raw)
    assert any(
        error["loc"] == ("unexpected_outer",)
        for error in caught.value.errors()
    )
    assert seen == []


@pytest.mark.parametrize(("field", "value"), [
    ("event_type", "governance.unknown"),
    ("schema_version", 2),
])
def test_event_type_and_schema_version_are_rejected_before_dispatch(
    field: str,
    value: object,
) -> None:
    seen: list[object] = []
    dispatcher = GovernanceEventDispatcher({
        name: seen.append for name in EVENT_CONSUMER_KEYS
    })
    raw = valid_envelope("identity.enrollment.requested")
    raw[field] = value
    with pytest.raises(ValidationError):
        dispatcher.consume(raw)
    assert seen == []


def test_unknown_payload_field_is_rejected_after_outer_envelope_validation() -> None:
    seen: list[object] = []
    dispatcher = GovernanceEventDispatcher({
        name: seen.append for name in EVENT_CONSUMER_KEYS
    })
    raw = valid_envelope("identity.deletion.requested")
    raw["payload"] = {
        **VALID_PAYLOADS["identity.deletion.requested"],
        "provider_resource_ref": "leak",
    }
    with pytest.raises(ValidationError):
        dispatcher.consume(raw)
    assert seen == []


@pytest.mark.parametrize("event_type", tuple(VALID_PAYLOADS))
def test_aggregate_id_must_match_event_domain_identity(event_type: str) -> None:
    seen: list[object] = []
    dispatcher = GovernanceEventDispatcher({
        name: seen.append for name in EVENT_CONSUMER_KEYS
    })
    raw = valid_envelope(event_type)
    raw["aggregate_id"] = UUID("00000000-0000-4000-8000-000000000099")
    with pytest.raises(ValidationError, match="aggregate_id must equal"):
        dispatcher.consume(raw)
    assert seen == []


def test_governance_events_contain_local_ids_only() -> None:
    rendered = json.dumps(VALID_PAYLOADS, ensure_ascii=False, default=str)
    for forbidden in ("provider_resource_ref", "provider_asset_id", "asset://",
                      "temporary_result_url", "access_token"):
        assert forbidden not in rendered
~~~

Put the drift and local-ID tests in test_event_schemas.py. Put full-envelope dispatch, unknown outer field, wrong event type, wrong schema version, unknown payload field, and all seven aggregate mismatch cases in test_event_consumers.py. Duplicate `VALID_PAYLOADS`, `AGGREGATE_FIELDS`, and `valid_envelope` as immutable test data in those two files; do not import one test module from another. The consumer tests must call `dispatcher.consume(raw_envelope)` with the whole message and must never call a payload-only overload.

- [ ] **Step 6: Export and compare the OpenAPI snapshot**

Run:

~~~bash
cd backend && uv run python -m ip_saas.scripts.export_contracts
cd backend && uv run python -m ip_saas.scripts.export_contracts --check
cd backend && uv run pytest tests/contract/governance/test_openapi.py tests/contract/governance/test_event_schemas.py tests/contract/governance/test_event_consumers.py -v
cd frontend && pnpm generate:api
~~~

Expected: the single exporter writes and then verifies OpenAPI plus all complete-envelope event schemas without drift; contract tests pass; and the only generated OpenAPI type artifact is `frontend/src/lib/api/schema.d.ts`. Review the diff; only the routes and seven point-named schemas listed in this task may appear, and no `generated.ts`, second schema file, payload-only event file, or hyphenated governance event filename is created.

- [ ] **Step 7: Write failing UI tests for Beta, consent separation, and no label removal**

Create frontend/src/features/governance/__tests__/ConsentWizard.test.tsx and IdentityAssetPanel.test.tsx. Assert the UI:

~~~tsx
expect(screen.getByText("公开 Beta，不是正式 SLA 能力")).toBeVisible();
expect(screen.getByLabelText("声音授权")).not.toBeChecked();
expect(screen.getByLabelText("肖像授权")).not.toBeChecked();
expect(screen.getByText("未成年人仅可进入素材授权流程，不能建模")).toBeVisible();
expect(screen.queryByText(/去除.*AI.*标识/)).not.toBeInTheDocument();
expect(screen.queryByText(/供应商资产|speaker_id|asset:\/\//i)).not.toBeInTheDocument();
~~~

Mock `frontend/src/lib/api/client.ts` in these tests and assert each feature call receives a backend path beginning with `/v1/`. Add a negative assertion that no feature passes `/api/v1/`; Plan 01's shared client is solely responsible for adding the browser `/api` proxy prefix.

- [ ] **Step 8: Implement the customer and platform governance screens**

In frontend/src/features/governance/types.ts define only UI-state unions derived from the `paths` types imported from `frontend/src/lib/api/schema.d.ts`; do not copy OpenAPI request/response DTOs into a second handwritten type tree. In api.ts import `apiRequest` from `frontend/src/lib/api/client.ts`, pass only `/v1/...` backend paths, and validate unknown response properties at the feature boundary. Do not call `fetch` directly, create another API client, import `generated.ts`, or pass `/api/v1/...` to `apiRequest`.

ConsentWizard presents separate voice, portrait, sensitive-information, platform, territory, validity, commercial use, AI processing, provider transfer, reuse, withdrawal, existing-output policy, and direct-verification steps. Seedance portrait opens the provider verification invitation and submits only the returned local receipt asset ID. Traditional avatar requires separate portrait, voice, and sensitive-information records. No minor modeling option is rendered.

IdentityAssetPanel shows capability-specific stage, quota, task, freeze/revocation, recycle deadline, backup expiry, complaint status, and a permanent-delete dialog requiring the server one-time token. The C-user cannot see platform flags or evidence internals. Platform governance shows three independent flags/gate matrices and complaints. Public Beta shows evidence-backed thresholds, not “已正式上线”.

- [ ] **Step 9: Run frontend and full contract tests**

Run:

~~~bash
cd frontend && pnpm test -- --run src/features/governance/__tests__/ConsentWizard.test.tsx src/features/governance/__tests__/IdentityAssetPanel.test.tsx
cd frontend && pnpm lint && pnpm typecheck
cd backend && uv run pytest tests/contract/governance -v
if rg -n "generated\\.ts|apiRequest\\([\"']\\/api\\/v1|fetch\\(" frontend/src/features/governance; then exit 1; fi
~~~

Expected: all commands exit 0 and the final `rg` emits no matches. UI never offers team members, online payment, automatic publishing, a third reseller level, raw provider identifiers, or AI-label removal; all browser requests continue through the single Plan 01 client.

- [ ] **Step 10: Commit API contracts and Beta UI**

~~~bash
git add backend/src/ip_saas/modules/governance/router.py backend/src/ip_saas/modules/governance/events.py backend/src/ip_saas/api.py backend/src/ip_saas/scripts/export_contracts.py contracts/events contracts/openapi.json frontend/src/lib/api/schema.d.ts frontend/src/features/governance frontend/src/app/\(c-user\)/projects/\[projectId\]/identity/page.tsx frontend/src/app/\(platform\)/platform/governance/page.tsx frontend/src/app/\(platform\)/platform/public-beta/page.tsx backend/tests/contract/governance
git commit -m "feat: expose governed digital human beta workflows"
~~~

### Task 12: Deploy the Beta stack on VKE, APIG, managed data services, and VMP

**Files:**
- Modify: backend/src/ip_saas/config.py
- Create: infra/helm/ip-saas/Chart.yaml
- Create: infra/helm/ip-saas/values.schema.json
- Create: infra/helm/ip-saas/values-cn-beijing.yaml
- Create: infra/helm/ip-saas/templates/api.yaml
- Create: infra/helm/ip-saas/templates/workers.yaml
- Create: infra/helm/ip-saas/templates/service.yaml
- Create: infra/helm/ip-saas/templates/apig-ingress.yaml
- Create: infra/helm/ip-saas/templates/autoscaling.yaml
- Create: infra/helm/ip-saas/templates/network-policy.yaml
- Create: infra/helm/ip-saas/templates/vmp-monitoring.yaml
- Create: infra/observability/governance-alerts.yaml
- Create: infra/observability/governance-dashboard.json
- Create: infra/scripts/verify-production-secrets.sh
- Create: infra/scripts/preflight-production.sh
- Create: infra/scripts/deploy.sh
- Test: infra/helm/ip-saas/tests/deployment_test.yaml

- [ ] **Step 1: Write failing Helm schema and secret-leak checks**

Create infra/helm/ip-saas/tests/deployment_test.yaml with helm-unittest assertions for three API replicas, topology spread across zones, PDB minAvailable=2, non-root/read-only security context, separate CPU and media worker deployments, VCI runtime for media, no literal Secret data, HPA, NetworkPolicy, ServiceMonitor, and APIG Ingress.

Add this shell check to the test runner:

~~~bash
helm lint infra/helm/ip-saas -f infra/helm/ip-saas/values-cn-beijing.yaml
helm template ip-saas infra/helm/ip-saas -f infra/helm/ip-saas/values-cn-beijing.yaml > /tmp/ip-saas-rendered.yaml
if rg -n "(AKLT|access[_-]?key|secret[_-]?key|Bearer |password:)" /tmp/ip-saas-rendered.yaml; then
  echo "rendered manifests contain a credential-like value" >&2
  exit 1
fi
~~~

Expected: FAIL before implementation because Helm exits non-zero while Chart.yaml and templates do not exist.

- [ ] **Step 2: Freeze config as mounted-secret paths, never secret values in Helm**

Modify backend/src/ip_saas/config.py so production requires these mounted files:

~~~python
class ProductionSecretFiles(BaseModel):
    postgres_dsn: Path = Path("/var/run/ip-saas-secrets/postgres_dsn")
    redis_dsn: Path = Path("/var/run/ip-saas-secrets/redis_dsn")
    rocketmq_credentials: Path = Path("/var/run/ip-saas-secrets/rocketmq_credentials")
    ark_api_key: Path = Path("/var/run/ip-saas-secrets/ark_api_key")
    speech_access_token: Path = Path("/var/run/ip-saas-secrets/speech_access_token")
    cv_openapi_credentials: Path = Path("/var/run/ip-saas-secrets/cv_openapi_credentials")
    identity_ref_data_key: Path = Path("/var/run/ip-saas-secrets/identity_ref_data_key")
    complaint_contact_data_key: Path = Path("/var/run/ip-saas-secrets/complaint_contact_data_key")

    def read(self, field_name: str) -> str:
        path = getattr(self, field_name)
        value = path.read_text(encoding="utf-8").strip()
        if not value:
            raise RuntimeError(f"empty production secret file: {field_name}")
        return value
~~~

TOS access uses the VKE workload's least-privilege cloud identity. KMS wraps the two data keys; plaintext keys and decrypted provider references exist only in worker memory. Rotation loads the new mounted version for writes while retaining the previous key ID only for bounded re-encryption reads.

- [ ] **Step 3: Create a strict Helm values schema and non-secret Beijing values**

Create Chart.yaml with `apiVersion: v2`, chart version `0.1.0`, and appVersion `0.1.0`. In values.schema.json require:

~~~json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "additionalProperties": false,
  "required": ["image", "runtimeSecretName", "service", "managed", "observability"],
  "properties": {
    "image": {
      "type": "object",
      "additionalProperties": false,
      "required": ["repository", "digest"],
      "properties": {
        "repository": {"type": "string", "minLength": 1},
        "digest": {"type": "string", "pattern": "^sha256:[0-9a-f]{64}$"}
      }
    },
    "runtimeSecretName": {"const": "ip-saas-runtime"},
    "service": {"type": "object"},
    "managed": {"type": "object"},
    "observability": {"type": "object"}
  }
}
~~~

values-cn-beijing.yaml contains only resource names and endpoints: VPC-private PostgreSQL, Redis, RocketMQ, and TOS endpoint references; APIG ingress class `apig`; public host name; VMP label selectors; three zones; private evidence/source/deliverable bucket names; and image repository plus deploy-time digest. It contains no password, token, AK/SK, signed URL, KMS plaintext data key, or provider asset ID.

- [ ] **Step 4: Implement hardened multi-zone API and worker workloads**

In api.yaml and workers.yaml set:

~~~yaml
replicas: 3
template:
  spec:
    automountServiceAccountToken: false
    topologySpreadConstraints:
      - maxSkew: 1
        topologyKey: topology.kubernetes.io/zone
        whenUnsatisfiable: DoNotSchedule
        labelSelector:
          matchLabels:
            app.kubernetes.io/name: ip-saas
    containers:
      - name: api
        securityContext:
          allowPrivilegeEscalation: false
          capabilities: {drop: ["ALL"]}
          readOnlyRootFilesystem: true
          runAsNonRoot: true
          seccompProfile: {type: RuntimeDefault}
        volumeMounts:
          - {name: runtime-secrets, mountPath: /var/run/ip-saas-secrets, readOnly: true}
        readinessProbe: {httpGet: {path: /readyz, port: 8080}}
        livenessProbe: {httpGet: {path: /healthz, port: 8080}}
    volumes:
      - name: runtime-secrets
        secret: {secretName: ip-saas-runtime, defaultMode: 256}
~~~

Use separate deployments for API, outbox dispatcher, CPU worker, identity worker, and FFmpeg/media worker. API has no FFmpeg binary and cannot call paid providers. The media worker sets `runtimeClassName: vci`, an ephemeral emptyDir size limit, no public ingress, and private TOS egress only. Set PodDisruptionBudget minAvailable=2 for API and one available consumer per critical queue. HPA uses CPU plus RocketMQ backlog custom metrics; scale-to-zero is forbidden for consent withdrawal, deletion, and complaint consumers.

- [ ] **Step 5: Add APIG, network, and managed-service boundaries**

apig-ingress.yaml uses `ingressClassName: apig`, TLS only, and only routes `/v1`. Configure JWT validation and CORS at APIG, ordinary JSON body limit 1 MiB, 60 requests/min/account, 5 complaints/hour/source hash, and a three-second upstream timeout for long-task receipts. Media uploads use a short-lived scoped TOS upload policy returned after authentication, not APIG request bodies.

network-policy.yaml defaults deny and then allows: APIG to API; API to PostgreSQL/Redis/RocketMQ/TOS control endpoints; workers to their required Volcengine/TOS endpoints; VMP scrape; DNS. PostgreSQL, Redis, RocketMQ, and TOS use VPC-private endpoints and TLS. RocketMQ topics have retry and dead-letter queues; retries are at-least-once, so database idempotency remains mandatory. TOS enables versioning, server-side encryption, private ACL, lifecycle for one-hour staging, and asynchronous cross-region replication only for critical ledger/evidence and necessary media.

- [ ] **Step 6: Add VMP metrics, dashboards, and actionable alerts**

Create ServiceMonitor resources for `/metrics`. governance-alerts.yaml must include exact expressions and `for` periods for: API p95 >2s for 10m; task receipt p95 >3s for 5m; queue oldest age >120s; DLQ >0; consent withdrawal lag >60s; delivery blocked but URL minted >0; label verification failure >0; duplicate settlement >0; provider quota >80%; capability gate expiry <7d; complaint critical unassigned >15m; deletion live step failed >0; backup expiry overdue >1h; PostgreSQL replication/PITR lag >3600s; and multi-zone replica shortfall >5m.

- [ ] **Step 7: Implement secret and production preflight scripts**

Create infra/scripts/verify-production-secrets.sh with `set -euo pipefail`. It reads only Kubernetes Secret key names via jsonpath, compares the sorted names with the eight frozen mounted filenames, verifies the Secret is encrypted at rest by cluster policy, and prints names/status only. It must never use `kubectl get secret -o yaml` or base64-decode a value.

Create infra/scripts/preflight-production.sh. It exits non-zero unless all of these are observed: three schedulable zones; `RuntimeClass/vci`; APIG ingress class; VMP ServiceMonitor CRD; private PostgreSQL with continuous backup; Redis primary/standby; RocketMQ multi-zone endpoints, retry and DLQ topics; TOS private ACL/versioning/encryption/lifecycle and replication policy; KMS key enabled with rotation; exact Secret keys; image digest exists and has passed the security scan; all three ProviderCapabilityEntitlement rows are production and current; every enrolled customer account and platform internal cost center has a current persisted GenerationLimitVersion. Emit redacted JSON evidence to stdout for capture in the evidence bucket.

- [ ] **Step 8: Implement an atomic digest deployment**

Create infra/scripts/deploy.sh:

~~~bash
#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: deploy.sh <sha256:image-digest> <evidence-directory>" >&2
  exit 64
fi
deploy_digest="$1"
evidence_dir="$2"
[[ "$deploy_digest" =~ ^sha256:[0-9a-f]{64}$ ]] || exit 65
mkdir -p "$evidence_dir"
infra/scripts/preflight-production.sh > "$evidence_dir/preflight.json"
helm upgrade --install ip-saas infra/helm/ip-saas \
  --namespace ip-saas --create-namespace --atomic --wait --timeout 15m \
  -f infra/helm/ip-saas/values-cn-beijing.yaml \
  --set-string image.digest="$deploy_digest"
kubectl -n ip-saas rollout status deployment/ip-saas-api --timeout=5m
kubectl -n ip-saas get deploy,pod,pdb,hpa -o json > "$evidence_dir/workloads.json"
sha256sum "$evidence_dir"/*.json > "$evidence_dir/SHA256SUMS"
~~~

The caller supplies a newly created dedicated evidence directory. The script never deletes or overwrites an existing evidence bundle; add an early `[[ ! -e "$evidence_dir/SHA256SUMS" ]] || exit 73` guard.

- [ ] **Step 9: Render, scan, and dry-run the production release**

Run:

~~~bash
helm lint infra/helm/ip-saas -f infra/helm/ip-saas/values-cn-beijing.yaml
helm unittest infra/helm/ip-saas
kubeconform -strict -summary -ignore-missing-schemas <(helm template ip-saas infra/helm/ip-saas -f infra/helm/ip-saas/values-cn-beijing.yaml)
infra/scripts/verify-production-secrets.sh
infra/scripts/preflight-production.sh > work/governance-preflight.json
~~~

Expected: all commands exit 0; preflight JSON contains no secrets and reports VKE/APIG/TOS/Redis/RocketMQ/VMP/KMS/provider prerequisites as passed. A missing zone, private endpoint, DLQ, backup, rotation, entitlement, or exact secret key exits non-zero.

- [ ] **Step 10: Commit the production deployment boundary**

~~~bash
git add backend/src/ip_saas/config.py infra/helm infra/observability infra/scripts/verify-production-secrets.sh infra/scripts/preflight-production.sh infra/scripts/deploy.sh
git commit -m "infra: deploy governed beta on volcengine"
~~~

### Task 13: Perform real RPO-one-hour and RTO-four-hour restore drills

**Files:**
- Create: backend/src/ip_saas/workers/recovery_marker.py
- Create: backend/src/ip_saas/scripts/dr_restore.py
- Create: backend/tests/unit/governance/test_dr_thresholds.py
- Create: backend/tests/contract/governance/test_rds_restore_actions.py
- Create: infra/helm/ip-saas/templates/recovery-jobs.yaml
- Create: infra/scripts/restore-postgres.sh
- Create: infra/scripts/restore-tos-sample.sh
- Create: infra/scripts/verify-rpo-rto.sh
- Create: infra/runbooks/backup-restore.md

- [ ] **Step 1: Write failing threshold and OpenAPI-action tests**

Create backend/tests/unit/governance/test_dr_thresholds.py:

~~~python
from ip_saas.scripts.dr_restore import DrillMeasurements, passes_drill


def test_rpo_and_rto_boundaries_are_inclusive() -> None:
    assert passes_drill(DrillMeasurements(3600, 14400, True, True, True))
    assert not passes_drill(DrillMeasurements(3601, 14400, True, True, True))
    assert not passes_drill(DrillMeasurements(3600, 14401, True, True, True))
    assert not passes_drill(DrillMeasurements(1, 1, False, True, True))
~~~

Create backend/tests/contract/governance/test_rds_restore_actions.py. With a fake signed OpenAPI caller assert the script calls `DescribeRecoverableTime`, then `RestoreToNewInstance`, then polls `DescribeTasks`; uses service `rds_postgresql`, version `2022-01-01`, region `cn-beijing`; includes `SrcInstanceId`, `RestoreTime`, two-zone `NodeInfo`, `VpcId`, `SubnetId`, `ChargeInfo={"ChargeType":"PostPaid"}`, deletion protection, and a unique drill tag. Assert it never calls RestoreToExistedInstance or mutates the source instance.

- [ ] **Step 2: Run the tests and confirm the drill module is missing**

Run:

~~~bash
cd backend && uv run pytest tests/unit/governance/test_dr_thresholds.py tests/contract/governance/test_rds_restore_actions.py -v
~~~

Expected: FAIL during collection because ip_saas.scripts.dr_restore does not exist.

- [ ] **Step 3: Emit an append-only recovery marker every five minutes**

Create backend/src/ip_saas/workers/recovery_marker.py:

~~~python
from hashlib import sha256

from sqlalchemy import func, select, text

from ip_saas.db.session import session_scope
from ip_saas.modules.governance.models import DisasterRecoveryMarker


def emit_recovery_marker(clock) -> int:
    with session_scope() as session:
        session.execute(text("SELECT pg_advisory_xact_lock(62026)"))
        previous = session.scalar(select(func.max(DisasterRecoveryMarker.sequence_no))) or 0
        sequence = previous + 1
        emitted_at = clock.now()
        digest = sha256(f"{sequence}:{emitted_at.isoformat()}".encode()).hexdigest()
        session.add(DisasterRecoveryMarker(
            sequence_no=sequence, emitted_at=emitted_at, marker_sha256=digest
        ))
        return sequence
~~~

Schedule it every five minutes in recovery-jobs.yaml with concurrencyPolicy=Forbid and a 60-second deadline. Alert if no marker appears for fifteen minutes. The marker contains no customer data.

- [ ] **Step 4: Implement the restore orchestration and hard thresholds**

Create backend/src/ip_saas/scripts/dr_restore.py:

~~~python
from dataclasses import dataclass
from datetime import datetime


@dataclass(frozen=True)
class DrillMeasurements:
    rpo_seconds: int
    rto_seconds: int
    database_checksums_passed: bool
    tos_checksums_passed: bool
    deletion_tombstones_applied: bool


def passes_drill(value: DrillMeasurements) -> bool:
    return (
        0 <= value.rpo_seconds <= 3600
        and 0 <= value.rto_seconds <= 14400
        and value.database_checksums_passed
        and value.tos_checksums_passed
        and value.deletion_tombstones_applied
    )


class PostgresRestoreDrill:
    service = "rds_postgresql"
    version = "2022-01-01"
    region = "cn-beijing"

    def __init__(self, caller, clock) -> None:
        self.caller = caller
        self.clock = clock

    def start(self, source_instance_id: str, restore_time: datetime,
              network: dict[str, object]) -> dict[str, object]:
        window = self.caller.call(self.service, self.version,
            "DescribeRecoverableTime", {"InstanceId": source_instance_id})
        if not window_contains(window, restore_time):
            raise RuntimeError("requested restore point is outside the recoverable window")
        suffix = restore_time.strftime("%Y%m%d%H%M%S")
        return self.caller.call(self.service, self.version,
            "RestoreToNewInstance", {
                "SrcInstanceId": source_instance_id,
                "RestoreTime": restore_time.strftime("%Y-%m-%dT%H:%M:%SZ"),
                "NodeInfo": network["NodeInfo"],
                "StorageType": network["StorageType"],
                "StorageSpace": network["StorageSpace"],
                "VpcId": network["VpcId"],
                "SubnetId": network["SubnetId"],
                "AllowListIds": network["AllowListIds"],
                "InstanceName": f"ip-saas-drill-{suffix}",
                "ProjectName": "ip-saas-dr",
                "ChargeInfo": {"ChargeType": "PostPaid", "Number": 1},
                "DeletionProtection": "Enabled",
                "Tags": [{"Key": "purpose", "Value": "disaster-recovery-drill"},
                         {"Key": "restore-point", "Value": suffix}],
            })
~~~

Implement `window_contains`, signed caller construction from the mounted OpenAPI credential, `DescribeTasks` polling with a five-second minimum and 30-minute maximum interval, and redacted JSON evidence. The CLI reads non-secret network/spec values from `/etc/ip-saas/dr-postgres.json`, creates a new instance only, waits until Running, obtains its temporary connection secret from the restricted secret broker, runs schema/Alembic head, ledger invariants, latest recovery-marker hash, deletion-tombstone reconciliation, and critical-row checksum queries, and never redirects application traffic.

- [ ] **Step 5: Implement safe shell entrypoints and TOS sampling**

Create infra/scripts/restore-postgres.sh with `set -euo pipefail`. It accepts source instance ID, UTC restore time, and a new empty evidence directory; validates the instance prefix and RFC3339 time; calls `uv run python -m ip_saas.scripts.dr_restore postgres`; writes start/end monotonic timestamps, new instance ID, action request IDs, recovered marker, RPO, checksums, and tombstone report. It never prints a DSN. It leaves deletion protection enabled; cleanup is a separately approved runbook operation naming the exact drill instance.

Create infra/scripts/restore-tos-sample.sh. From the immutable backup catalog choose 100 deterministic objects across ledger evidence, governance evidence, source rights evidence, and necessary deliverables, including at least ten cross-region replicas and ten versioned prior objects. Restore archived versions when necessary, download to a fresh `mktemp -d` workspace, compare size and SHA-256 to the catalog, verify deleted-target tombstones remove them from the accessible restore set, record TOS request/version IDs, and clean only that validated temporary workspace in a trap.

- [ ] **Step 6: Compute RPO/RTO and persist independently signed evidence**

Create infra/scripts/verify-rpo-rto.sh. It accepts the PostgreSQL evidence JSON, TOS evidence JSON, operator UUID, reviewer UUID, and evidence TOS prefix. It rejects equal signer IDs. RPO is `restore_requested_at - latest_valid_marker.emitted_at`; RTO is `all_core_checks_completed_at - restore_requested_at`. It exits non-zero above 3600 or 14400 seconds, on a checksum mismatch, missing TOS class, missing tombstone reconciliation, or source-instance mutation. On success it uploads the immutable bundle, verifies the remote SHA-256, and inserts OperationalDrillEvidence through a restricted internal command.

- [ ] **Step 7: Add monthly and quarterly runbook procedures**

Create infra/runbooks/backup-restore.md with two executable schedules:

- Monthly: alternate PostgreSQL point-in-time restore and 100-object TOS restore; complete by the seventh calendar day; two independent signers; close every failed measurement before the next public-Beta capability promotion.
- Quarterly: create an isolated recovery namespace and new RDS instance, restore critical cross-region TOS, deploy the last known good image by digest, apply deletion tombstones before traffic, replay RocketMQ from a frozen offset into the isolated namespace, run API/task/ledger/identity-delivery smoke tests, and measure full RTO. No DNS or production consumer group is switched during the drill.

Include explicit failure/abort criteria, incident escalation, evidence locations, and the separately approved new-instance cleanup command. “Backup enabled” is never a pass result.

- [ ] **Step 8: Run local contract tests and one real production drill**

Run local tests:

~~~bash
cd backend && uv run pytest tests/unit/governance/test_dr_thresholds.py tests/contract/governance/test_rds_restore_actions.py tests/resilience/test_deleted_data_not_restored.py -v
~~~

Expected: PASS.

Run the approved production drill:

~~~bash
infra/scripts/restore-postgres.sh postgres-cn-beijing-source 2026-08-24T00:00:00Z work/drill-2026-08-postgres
infra/scripts/restore-tos-sample.sh work/drill-2026-08-tos
infra/scripts/verify-rpo-rto.sh work/drill-2026-08-postgres/result.json work/drill-2026-08-tos/result.json 018f0000-0000-7000-8000-000000000010 018f0000-0000-7000-8000-000000000011 governance/drills/2026-08
~~~

The example identifiers are test fixtures only. Before execution, the change ticket supplies the exact production source instance, current recoverable UTC point, empty evidence directories, and two real distinct signer UUIDs. Expected: all commands exit 0, measured RPO is at most 3600 seconds, RTO at most 14400 seconds, all checksums match, and the resulting signed OperationalDrillEvidence is current. If any condition fails, every capability remains below PUBLIC_BETA.

- [ ] **Step 9: Commit real restore tooling and runbook**

~~~bash
git add backend/src/ip_saas/workers/recovery_marker.py backend/src/ip_saas/scripts/dr_restore.py backend/tests/unit/governance/test_dr_thresholds.py backend/tests/contract/governance/test_rds_restore_actions.py infra/helm/ip-saas/templates/recovery-jobs.yaml infra/scripts/restore-postgres.sh infra/scripts/restore-tos-sample.sh infra/scripts/verify-rpo-rto.sh infra/runbooks/backup-restore.md
git commit -m "infra: verify production backup recovery objectives"
~~~

### Task 14: Pass security, normal-load, structured-output, and fault-injection gates

**Files:**
- Modify: backend/tests/security/test_identity_boundaries.py
- Create: backend/tests/security/test_secret_and_log_redaction.py
- Create: backend/tests/security/test_governance_ssrf.py
- Modify: backend/tests/load/locustfile.py
- Create: backend/tests/resilience/test_governance_faults.py
- Create: backend/tests/fixtures/governance/structured-output-corpus.v1.jsonl
- Create: backend/tests/fixtures/governance/structured-output-corpus.v1.sha256
- Create: backend/src/ip_saas/scripts/evaluate_structured_output.py
- Create: backend/src/ip_saas/scripts/rocketmq_poc.py
- Create: infra/runbooks/identity-incident.md
- Test: backend/tests/unit/governance/test_structured_output_gate.py

- [ ] **Step 1: Write failing zero-tolerance security tests**

Extend backend/tests/security/test_identity_boundaries.py and create test_secret_and_log_redaction.py and test_governance_ssrf.py. Cover this exact matrix:

~~~python
@pytest.mark.parametrize("attacker", [
    "other_customer", "reseller_l1", "reseller_l2", "platform_operator_other_account",
])
@pytest.mark.parametrize("resource", [
    "identity_subject", "consent", "source_asset", "provider_identity_asset",
    "generation_identity_use", "complaint", "deletion_plan",
])
def test_cross_tenant_and_reseller_reads_are_not_found(boundary_harness,
                                                        attacker, resource):
    response = boundary_harness.get(attacker, resource)
    assert response.status_code == 404


def test_logs_traces_audit_and_outbox_have_no_identity_secrets(redaction_harness):
    redaction_harness.exercise_all_provider_success_and_failure_paths()
    rendered = redaction_harness.all_text()
    for secret in redaction_harness.canaries([
        "portrait_source", "voice_source", "provider_reference", "id_number",
        "home_address", "contact", "access_token", "signed_url",
    ]):
        assert secret not in rendered


@pytest.mark.parametrize("url", [
    "http://127.0.0.1/admin", "http://169.254.169.254/latest/meta-data",
    "file:///etc/passwd", "https://user:pass@example.com/a",
])
def test_user_urls_never_reach_provider_or_tos(governance_api, url):
    response = governance_api.post_generation({"script_text": url})
    assert response.status_code in {202, 422}
    assert not governance_api.egress_observer.requested(url)
~~~

Add consent withdrawal versus generation race, expired consent, minor-modeling denial, unsupported provider deletion, complaint freeze, confirmation-token reuse, label stripping, delivery URL after quarantine, prompt injection attempting asset://, mass-assignment, SQL wildcard, oversized JSON, MIME mismatch, zip bomb, and audit-evidence mutation cases.

- [ ] **Step 2: Run security tests and observe the first unimplemented boundary**

Run:

~~~bash
cd backend && uv run pytest tests/security -v
~~~

Expected: FAIL before fixes because at least one new boundary test is unimplemented. Fix only the boundary named by the failure; do not weaken an assertion or add a test-only bypass.

- [ ] **Step 3: Freeze a versioned critical structured-output corpus and gate**

Create backend/tests/fixtures/governance/structured-output-corpus.v1.jsonl with exactly 1,000 immutable, de-identified inputs: 200 each for fruit, gold gifts, TikTok live guild, platform self-marketing, and mother creators. For each project type include 20 cases for each critical Plan 02 structured schema: diagnosis, persona, strategy directions, research claims, topic cards, marketing frame, script, platform variant, fact/rights review, and QC routing. Include missing data, contradictory evidence, sensitive claims, title-bait framing, malformed Unicode, long inputs, and prompt-injection strings. Store the corpus SHA-256 in the adjacent fixture manifest and review changes like source code.

Create backend/src/ip_saas/scripts/evaluate_structured_output.py and the unit gate:

~~~python
from dataclasses import dataclass


@dataclass(frozen=True)
class StructuredOutputResult:
    total: int
    valid_within_retry_limit: int
    unsafe_false_success: int
    retry_limit: int


def passes_structured_output_gate(result: StructuredOutputResult) -> bool:
    if result.total != 1000 or result.retry_limit != 3:
        return False
    return (
        result.valid_within_retry_limit / result.total >= 0.99
        and result.unsafe_false_success == 0
    )
~~~

The evaluator verifies the fixture manifest hash, calls the production StructuredGateway and the frozen active model registry entries, permits at most three total attempts per item, validates strict Pydantic output plus semantic invariants, and stores each input ID, schema/model/prompt/rule version, attempts, validation category, latency, token usage, and provider request ID. It never stores raw identity material. Any fabricated-fact, unsafe authorization, cross-project citation, or schema accepted after more than three attempts is a hard failure independent of aggregate rate.

- [ ] **Step 4: Test the gate and run the paid fixed-corpus evaluation**

Run deterministic boundary tests:

~~~bash
cd backend && uv run pytest tests/unit/governance/test_structured_output_gate.py -v
~~~

Expected: PASS for 990/1000 with zero unsafe false success; FAIL for 989/1000, 990/999, retry limit four, or one unsafe false success.

Run the approved production-model evaluation:

~~~bash
cd backend && uv run python -m ip_saas.scripts.evaluate_structured_output --corpus tests/fixtures/governance/structured-output-corpus.v1.jsonl --manifest tests/fixtures/governance/structured-output-corpus.v1.sha256 --max-attempts 3 --environment production --evidence-tos-prefix governance/structured-output/v1
~~~

Expected: exit 0 only when at least 990 of exactly 1,000 items validate within three attempts and unsafe_false_success is zero. Capture the immutable evidence hash as the STRUCTURED_OUTPUT CapabilityGateAttestation used by the release evaluator; a dry-run/fake-model result cannot satisfy this launch gate.

- [ ] **Step 5: Define and execute the normal-load profile**

Modify backend/tests/load/locustfile.py with three weighted user classes: 60% authenticated project browsing/listing, 30% long-task enrollment/generation receipt creation against deterministic fake providers, and 10% public complaint/status/recycle reads. Run 200 concurrent users, 20 new users/second, and 30 minutes after a five-minute warm-up. Use isolated performance accounts/projects and disposable credits/internal budgets; no production customer data or paid provider call.

~~~python
def assert_load_gate(stats) -> None:
    assert stats.non_ai_requests >= 100_000
    assert stats.long_task_receipts >= 10_000
    assert stats.non_ai_p95_ms <= 2_000
    assert stats.long_task_receipt_p95_ms <= 3_000
    assert stats.http_failure_rate <= 0.001
    assert stats.duplicate_hold_count == 0
    assert stats.false_success_count == 0
~~~

Run:

~~~bash
cd backend && uv run locust -f tests/load/locustfile.py --headless -u 200 -r 20 --run-time 35m --csv work/governance-load
cd backend && uv run python -m ip_saas.scripts.evaluate_load work/governance-load_stats.csv
~~~

Expected: both exit 0 and the thresholds above pass. Store Locust CSV, VMP query exports, image digest, dataset IDs, and hashes as signed LOAD OperationalDrillEvidence.

- [ ] **Step 6: Write deterministic duplicate, outage, and failover injections**

Create backend/tests/resilience/test_governance_faults.py:

~~~python
@pytest.mark.parametrize("fault", [
    "rocketmq_duplicate", "rocketmq_out_of_order", "rocketmq_redelivery_after_dlq",
    "provider_timeout_before_task_id", "provider_timeout_after_task_id",
    "tos_signed_source_expired", "tos_copy_interrupted", "redis_unavailable",
    "postgres_failover_during_hold", "worker_killed_after_provider_success",
    "expired_lease_stale_worker_finishes_after_reclaim",
    "zone_drained", "label_metadata_stripped",
])
def test_fault_never_double_charges_or_delivers_invalid_identity(
    governance_fault_harness, fault,
) -> None:
    result = governance_fault_harness.inject(fault)
    assert result.customer_net_charge in {0, result.one_settled_charge}
    assert result.internal_net_cost in {0, result.one_settled_cost}
    assert result.hold_count <= 1
    assert result.task_truth_matches_provider_observation
    assert result.stale_attempt_terminal_write_count == 0
    assert not result.delivered_without_current_consent
    assert not result.delivered_without_both_ai_labels
    assert result.can_resume_from_last_successful_step
~~~

Use PostgreSQL transaction termination, Toxiproxy for provider/TOS/Redis links, duplicate and reordered RocketMQ fixture messages, VKE pod deletion, and a controlled zone-node drain in the isolated resilience namespace. Never run destructive fault injection in the production namespace.

- [ ] **Step 7: Run resilience and supply-chain scans**

Run:

~~~bash
cd backend && uv run pytest tests/resilience/test_governance_faults.py -v
cd backend && uv run pip-audit
cd backend && uv run bandit -r src/ip_saas/modules/governance src/ip_saas/providers/digital_identity src/ip_saas/providers/volcengine
trivy image --exit-code 1 --severity HIGH,CRITICAL --ignore-unfixed ip-saas@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
~~~

The digest shown is a fixture. The release job substitutes the candidate's immutable CR digest and stores the signed report. Expected: tests pass; no unapproved HIGH/CRITICAL vulnerability, leaked secret, repeated charge/refund, false success, invalid delivery, or unrecoverable step.

- [ ] **Step 8: Run a non-destructive RocketMQ PoC on the real production instance**

Create backend/src/ip_saas/scripts/rocketmq_poc.py with this frozen result gate:

~~~python
from dataclasses import dataclass


@dataclass(frozen=True)
class RocketMqPocResult:
    unique_messages: int
    delivered_messages: int
    duplicate_deliveries: int
    out_of_order_deliveries: int
    reclaimed_expired_leases: int
    stale_attempt_conflicts: int
    duplicate_holds: int
    duplicate_terminal_writes: int
    dlq_before_redrive: int
    dlq_after_redrive: int
    unreconciled_holds: int
    provider_cost_rows: int


def passes_rocketmq_poc(value: RocketMqPocResult) -> bool:
    return (
        value.unique_messages == 1000
        and value.delivered_messages >= 1250
        and value.duplicate_deliveries >= 250
        and value.out_of_order_deliveries >= 100
        and value.reclaimed_expired_leases >= 50
        and value.stale_attempt_conflicts == value.reclaimed_expired_leases
        and value.duplicate_holds == 0
        and value.duplicate_terminal_writes == 0
        and value.dlq_before_redrive == 10
        and value.dlq_after_redrive == 0
        and value.unreconciled_holds == 0
        and value.provider_cost_rows == 0
    )
~~~

The CLI accepts only `--environment production`, the exact isolated topic, consumer group, platform project, internal cost center, and evidence prefix registered in preflight. It creates 1,000 no-provider `governance.rocketmq_poc` internal tasks, publishes all messages plus 250 deliberate duplicates, uses multiple queues to observe at least 100 out-of-order deliveries, pauses a consumer after 50 starts and waits beyond the 300-second lease so a second consumer reclaims them, then resumes the old consumer and records 50 Conflict outcomes from its stale terminal CAS. Ten signed poison messages must reach the isolated DLQ at the configured retry limit; after the handler is repaired, redrive them to success and prove the DLQ is empty. All internal holds are released and no ProviderCostEntry is created because this path proves that no supplier call occurred; therefore no `ProviderCostInput` is fabricated merely to carry `task_id=None`. The script refuses ordinary production topics and consumer groups, contains no customer or identity payload, does not drain or stop a production broker, and writes request/message/task/attempt IDs plus ledger reconciliation to immutable evidence.

Run:

~~~bash
cd backend && uv run python -m ip_saas.scripts.rocketmq_poc --environment production --topic ip-saas-governance-poc --consumer-group ip-saas-governance-poc-v1 --project-id 018f0000-0000-7000-8000-000000000020 --cost-center-id 018f0000-0000-7000-8000-000000000021 --evidence-tos-prefix governance/rocketmq/2026-08
~~~

The UUIDs shown are fixtures; the approved change ticket substitutes the preflight-registered isolated project and cost center. Expected: exit 0 only at the exact gate above. Insert one independently signed FAULT_INJECTION OperationalDrillEvidence and reference its evidence hash from each capability's FAILURE_RECOVERY attestation. Unit or local-broker tests cannot substitute for this production-instance PoC.

- [ ] **Step 9: Complete incident runbook and signed drill evidence**

Create infra/runbooks/identity-incident.md with exact triggers and owners for unauthorized identity, missing label, minor material, provider compromise, leaked signed URL/reference, mass complaint, stuck withdrawal/deletion, duplicate billing, and capability-wide provider degradation. The first response for an identity incident is: set only the affected capability OFF, freeze linked identities, quarantine deliverables, stop new signed URLs, preserve evidence, page PLATFORM_REVIEWER/security, reconcile running costs, notify affected owners, and invoke provider deletion if confirmed. It must include complaint acknowledgement and update targets, evidence preservation, regulator/legal escalation decision, recovery prerequisites, and a post-incident gate re-run.

Insert independently signed SECURITY and FAULT_INJECTION OperationalDrillEvidence rows from the immutable reports. A scan alone is not a security drill; a test-only fake is not a provider recovery drill.

- [ ] **Step 10: Commit engineering launch gates**

~~~bash
git add backend/tests/security backend/tests/load/locustfile.py backend/tests/resilience/test_governance_faults.py backend/tests/fixtures/governance backend/src/ip_saas/scripts/evaluate_structured_output.py backend/src/ip_saas/scripts/rocketmq_poc.py backend/tests/unit/governance/test_structured_output_gate.py infra/runbooks/identity-incident.md
git commit -m "test: enforce beta security load and recovery gates"
~~~

### Task 15: Evaluate the six-week Beta with immutable hard thresholds

**Files:**
- Create: backend/src/ip_saas/modules/governance/beta.py
- Create: backend/src/ip_saas/scripts/evaluate_public_beta.py
- Create: backend/tests/fixtures/governance/public-beta-criteria.v1.json
- Create: backend/tests/unit/governance/test_beta_evaluator.py
- Create: backend/tests/integration/governance/test_blind_review.py
- Create: backend/tests/integration/governance/test_public_beta_evidence.py
- Create: infra/runbooks/public-beta-release.md

- [ ] **Step 1: Freeze the criteria file before looking at outcomes**

Create backend/tests/fixtures/governance/public-beta-criteria.v1.json with these exact immutable values:

~~~json
{
  "criteria_version": "public-beta-v1",
  "minimum_days": 42,
  "customer_cases": ["fruit", "gold_gifts", "tiktok_live_guild", "mother_creator"],
  "platform_tracks": ["reseller_l1", "reseller_l2", "c_user"],
  "minimum_publications_per_project": 12,
  "minimum_total_publications": 60,
  "minimum_platform_publications_per_track": 3,
  "minimum_platform_verified_actions_per_track": 1,
  "minimum_customer_projects_with_uplift": 3,
  "minimum_median_standardized_uplift": "0.15",
  "minimum_topic_adoption_rate": "0.60",
  "minimum_non_structural_script_rate": "0.70",
  "minimum_blind_pairs": 100,
  "minimum_blind_pairs_per_case": 20,
  "minimum_system_blind_win_rate": "0.70",
  "minimum_structured_output_rate": "0.99",
  "structured_output_items": 1000,
  "structured_output_max_attempts": 3,
  "maximum_rpo_seconds": 3600,
  "maximum_rto_seconds": 14400,
  "require_generation_limit_versions": true,
  "require_provider_cost_task_lineage": true,
  "zero_tolerance_errors": 0
}
~~~

Hash the canonical JSON and store that hash in BetaProgram before setting status FROZEN. The service rejects criterion mutation, adding/removing enrolled projects, or changing start/end after frozen_at. A later criteria version creates a new BetaProgram; it never rewrites this run.

- [ ] **Step 2: Write failing threshold-boundary tests**

Create backend/tests/unit/governance/test_beta_evaluator.py. Start from one passing fixture and independently change each boundary:

~~~python
def test_exact_boundaries_pass(passing_beta_input, evaluator) -> None:
    result = evaluator.evaluate(passing_beta_input)
    assert result.passed
    assert result.failed_gates == ()


@pytest.mark.parametrize(("mutation", "gate"), [
    ("duration_41_days", "six_week_duration"),
    ("one_customer_has_11_publications", "project_publication_count"),
    ("total_59", "total_publication_count"),
    ("only_two_customer_projects_at_15_percent", "customer_uplift"),
    ("topic_adoption_59_99_percent", "topic_adoption"),
    ("non_structural_69_99_percent", "script_rewrite"),
    ("blind_system_wins_69_of_100", "blind_review"),
    ("structured_outputs_989_of_1000", "structured_output"),
    ("platform_l2_has_two_publications", "platform_track_publications"),
    ("platform_c_user_has_zero_actions", "platform_track_actions"),
    ("one_content_missing_t3", "complete_lineage"),
    ("one_account_or_cost_center_missing_limit", "generation_limits"),
    ("one_provider_cost_has_null_or_wrong_task", "provider_cost_task_lineage"),
    ("provider_crash_evidence_hash_mismatch", "provider_cost_task_lineage"),
    ("supplier_statement_omits_one_request", "provider_cost_task_lineage"),
    ("one_false_claim", "zero_tolerance"),
])
def test_each_failed_boundary_forces_hold(passing_beta_input, evaluator,
                                          mutation, gate) -> None:
    result = evaluator.evaluate(passing_beta_input.mutate(mutation))
    assert not result.passed
    assert gate in result.failed_gates
~~~

- [ ] **Step 3: Run evaluator tests and confirm beta.py is missing**

Run:

~~~bash
cd backend && uv run pytest tests/unit/governance/test_beta_evaluator.py -v
~~~

Expected: FAIL during collection because governance.beta does not exist.

- [ ] **Step 4: Implement a deterministic evaluator with no subjective override**

Create backend/src/ip_saas/modules/governance/beta.py:

~~~python
from dataclasses import dataclass
from decimal import Decimal


CUSTOMER_CASES = frozenset({
    "fruit", "gold_gifts", "tiktok_live_guild", "mother_creator",
})
PLATFORM_TRACKS = frozenset({"reseller_l1", "reseller_l2", "c_user"})


@dataclass(frozen=True)
class ProjectEvidence:
    project_id: str
    kind: str
    customer_case: str | None
    publication_count: int
    complete_lineage_count: int
    onboarding_and_persona_confirmed: bool
    median_standardized_uplift: Decimal | None
    cold_start: bool
    platform_track_publications: dict[str, int]
    platform_track_verified_actions: dict[str, int]
    platform_tracks_have_frozen_goal_and_baseline: bool


@dataclass(frozen=True)
class BetaEvidence:
    observed_days: int
    projects: tuple[ProjectEvidence, ...]
    topic_recommendations: int
    adopted_topics: int
    assessed_scripts: int
    scripts_without_structural_rewrite: int
    blind_pairs: int
    blind_system_wins: int
    blind_pairs_by_case: dict[str, int]
    structured_total: int
    structured_valid: int
    structured_max_attempts: int
    zero_tolerance_errors: int
    capability_gate_set_current: bool
    production_batch_poc_current: bool
    generation_limit_versions_complete: bool
    provider_cost_task_lineage_complete: bool
    external_attestations_current: bool
    operational_drills_current: bool
    rpo_seconds: int
    rto_seconds: int


@dataclass(frozen=True)
class BetaEvaluation:
    passed: bool
    failed_gates: tuple[str, ...]
    measurements: dict[str, object]


class PublicBetaEvaluator:
    def evaluate(self, value: BetaEvidence) -> BetaEvaluation:
        failures: list[str] = []
        customers = [row for row in value.projects if row.kind == "customer"]
        platforms = [row for row in value.projects if row.kind == "platform_self_marketing"]
        if value.observed_days < 42:
            failures.append("six_week_duration")
        if (len(customers) != 4 or len(platforms) != 1
                or {row.customer_case for row in customers} != CUSTOMER_CASES):
            failures.append("project_cohort")
        if any(row.publication_count < 12 for row in value.projects):
            failures.append("project_publication_count")
        total = sum(row.publication_count for row in value.projects)
        if total < 60:
            failures.append("total_publication_count")
        if any(row.complete_lineage_count != row.publication_count
               or not row.onboarding_and_persona_confirmed for row in value.projects):
            failures.append("complete_lineage")
        uplift_passes = sum(
            not row.cold_start and row.median_standardized_uplift is not None
            and row.median_standardized_uplift >= Decimal("0.15")
            for row in customers
        )
        if uplift_passes < 3:
            failures.append("customer_uplift")
        if (value.topic_recommendations <= 0
                or Decimal(value.adopted_topics) / value.topic_recommendations
                   < Decimal("0.60")):
            failures.append("topic_adoption")
        if (value.assessed_scripts <= 0
                or Decimal(value.scripts_without_structural_rewrite)
                   / value.assessed_scripts < Decimal("0.70")):
            failures.append("script_rewrite")
        platform = platforms[0] if len(platforms) == 1 else None
        if platform is not None:
            if (set(platform.platform_track_publications) != PLATFORM_TRACKS
                    or any(platform.platform_track_publications[item] < 3
                           for item in PLATFORM_TRACKS)):
                failures.append("platform_track_publications")
            if (set(platform.platform_track_verified_actions) != PLATFORM_TRACKS
                    or any(platform.platform_track_verified_actions[item] < 1
                           for item in PLATFORM_TRACKS)):
                failures.append("platform_track_actions")
            if not platform.platform_tracks_have_frozen_goal_and_baseline:
                failures.append("platform_track_baselines")
        if (value.blind_pairs < 100
                or any(value.blind_pairs_by_case.get(item, 0) < 20
                       for item in CUSTOMER_CASES | {"platform_self_marketing"})
                or Decimal(value.blind_system_wins) / max(value.blind_pairs, 1)
                   < Decimal("0.70")):
            failures.append("blind_review")
        if (value.structured_total != 1000 or value.structured_max_attempts != 3
                or Decimal(value.structured_valid) / max(value.structured_total, 1)
                   < Decimal("0.99")):
            failures.append("structured_output")
        if value.zero_tolerance_errors != 0:
            failures.append("zero_tolerance")
        if not value.capability_gate_set_current:
            failures.append("capability_gates")
        if not value.production_batch_poc_current:
            failures.append("production_batch_poc")
        if not value.generation_limit_versions_complete:
            failures.append("generation_limits")
        if not value.provider_cost_task_lineage_complete:
            failures.append("provider_cost_task_lineage")
        if not value.external_attestations_current:
            failures.append("external_attestations")
        if (not value.operational_drills_current or value.rpo_seconds > 3600
                or value.rto_seconds > 14400):
            failures.append("operations_and_recovery")
        unique_failures = tuple(sorted(set(failures)))
        return BetaEvaluation(not unique_failures, unique_failures,
                              {"total_publications": total,
                               "customer_uplift_passes": uplift_passes})
~~~

The five blind-review buckets require 20 pairs each, so the minimum is exactly 100. A system win counts only when both the operator and director, shown randomized A/B artifacts with source identity sealed, independently choose the system version and neither marks fabricated fact, unshootable result, or obvious persona mismatch. Reviewer disagreement, tie, generic-AI win, or disqualification is a non-win; dimension scores cannot offset a disqualifier.

- [ ] **Step 5: Implement blinded pair assignment and immutable review evidence**

Create backend/tests/integration/governance/test_blind_review.py, then add BlindReviewService to beta.py. It must create the generic AI comparator and system artifact from the same de-identified brief, platform, duration, and format; cryptographically randomize A/B; seal the mapping until two different reviewer IDs submit all nine dimensions plus title-bait risk; forbid reviewers involved in creating either artifact; derive winner server-side; store the immutable BetaBlindReviewResult and evidence hash; and reject a changed score or second submission. Tests cover hidden order, reviewer separation, disqualifier behavior, 69/100 failure, and 70/100 pass.

- [ ] **Step 6: Build evidence only from the five enrolled projects**

Create backend/src/ip_saas/scripts/evaluate_public_beta.py. The repository must:

1. Require exactly five BetaProjectEnrollment rows: one each of fruit, gold gifts, TikTok live guild, mother creator, and one platform self-marketing project. The approved cohort follows the five fixed validation projects in the specification; a sales method or identity-label-only counterexample is a fixture, not another live Beta project.
2. Query publications only inside BetaProgram starts_at/ends_at and require the program to be finished with at least 42 observed days.
3. For every publication verify frozen persona/strategy/track/topic/marketing/content/platform lineage, production version, Publication, confirmed T+1/T+3/T+7 MetricSnapshot, Retro, and BetaContentAssessment.
4. Reuse Plan 04 CustomerBetaReportService's matched project/platform/format/track baseline formula. Cold-start content is recorded but cannot claim uplift.
5. Reuse the self-marketing attribution service. Count only manually verified, source-deduplicated BusinessAction rows, keep L1/L2/C-user tracks separate, and never query a customer project from the platform report.
6. Calculate topic adoption from recommended TopicCard rows and persisted TopicSelection; calculate rewrite rate from BetaContentAssessment; count false claims, major controversy, and unsustainable paid traffic as zero-tolerance evidence.
7. Derive `generation_limit_versions_complete` from PostgreSQL: every customer account enrolled in the program has an effective account limit version, and every platform/internal task cost center has an effective internal limit version covering its submission time. Reject gaps, future-only versions, wrong billing scope, a submitted amount above its per-task limit, or any BillingService composition evidence that omitted the mandatory GenerationLimitService. Never accept this boolean from CLI input.
8. Load exactly the immutable structured-output run, blind results, current external attestations, current real restore/security/load/fault/RocketMQ evidence, and the target capability's current gate/production PoC set. Derive `provider_cost_task_lineage_complete` by requiring the target capability's current PASS `PROVIDER_CRASH_RECONCILIATION` attestation for the exact entitlement/account/region/model/release image, re-downloading its private outer `CapabilityProviderCrashEvidenceV1`, checking the attestation SHA-256, then re-downloading the five version-pinned supplier request-log/final-statement/reconciliation-report/attempt-journal/SIGKILL assets and checking every embedded hash. Strictly parse the two supplier exports, re-run the stable-attempt/request-set and task-cost mapping checks, require the attested reconciler hash to equal the running release, and verify every statement line maps to exactly one `ProviderCostEntry.task_id = IdentityEnrollment/GenerationJob.task_record_id`. Only then run the database anti-join as a supplemental current-state drift check over every real provider completion in the program. The anti-join alone, a CLI boolean/count, a screenshot, or absence of a current cost row is never positive supplier evidence.

Serialize the complete input in canonical sorted JSON, store it in private TOS, re-download and verify SHA-256, then evaluate. Never accept a manually supplied count or boolean.

- [ ] **Step 7: Create a capability-specific signed release decision**

If the evaluator passes, a PLATFORM_ADMIN proposes APPROVE_PUBLIC_BETA for exactly one BetaCapability. In the same transaction the evaluator locks and resolves exactly one current ACTIVE, unexpired `ProviderCapabilityEntitlement`; it rejects zero or multiple current rows. The canonical input snapshot includes that entitlement ID, provider-account fingerprint, region, model ID, model version, release snapshot SHA-256, and every required gate ID/evidence SHA-256. A different PLATFORM_REVIEWER verifies the input snapshot hash, criteria hash, immutable evidence asset, evaluator version, and exact entitlement binding. The product owner attaches signed approval evidence. Insert `ReleaseDecision` with its capability and copy the six entitlement-binding fields into their dedicated columns; any failed gate inserts HOLD with the complete `failed_gates` list and the same binding. There is no force-pass, waiver, manual score, or “mostly passed” branch. Rotating or expiring the entitlement after decision creation makes that decision unusable; the evaluator must run again and issue a new independently reviewed decision.

Run:

~~~bash
cd backend && uv run python -m ip_saas.scripts.evaluate_public_beta --program-id 018f0000-0000-7000-8000-000000000030 --capability voice_clone --criteria tests/fixtures/governance/public-beta-criteria.v1.json --evidence-tos-prefix governance/public-beta/voice-clone
~~~

The UUID is a fixture; the release ticket supplies the frozen program ID. Expected: exit 0 and an APPROVE_PUBLIC_BETA decision only if every hard threshold passes. Run separately for Seedance authorized portrait and traditional digital human; a failed provider-specific gate holds only that capability.

- [ ] **Step 8: Run all evaluator and evidence integration tests**

Run:

~~~bash
cd backend && uv run pytest tests/unit/governance/test_beta_evaluator.py tests/integration/governance/test_blind_review.py tests/integration/governance/test_public_beta_evidence.py tests/integration/publication/test_attribution_beta.py -v
~~~

Expected: PASS. Tests prove 42/12/60, four named customer cases plus isolated platform project, three track minima, three-of-four uplift, 60% adoption, 70% non-structural scripts, 70% dual blind wins, 99% structured output, complete T1/T3/T7 lineage, and every zero-tolerance failure.

- [ ] **Step 9: Commit immutable Beta evaluation**

~~~bash
git add backend/src/ip_saas/modules/governance/beta.py backend/src/ip_saas/scripts/evaluate_public_beta.py backend/tests/fixtures/governance/public-beta-criteria.v1.json backend/tests/unit/governance/test_beta_evaluator.py backend/tests/integration/governance/test_blind_review.py backend/tests/integration/governance/test_public_beta_evidence.py infra/runbooks/public-beta-release.md
git commit -m "feat: evaluate six week beta with hard evidence gates"
~~~

### Task 16: Release one Beta capability at a time and prove rollback

**Files:**
- Modify: backend/src/ip_saas/modules/governance/beta.py
- Modify: backend/src/ip_saas/modules/governance/router.py
- Modify: backend/src/ip_saas/modules/governance/schemas.py
- Create: backend/src/ip_saas/scripts/release_control.py
- Create: backend/tests/integration/governance/test_release_control.py
- Create: infra/scripts/rollback.sh
- Modify: infra/runbooks/public-beta-release.md
- Modify: Makefile
- Modify: README.md
- Test: frontend/tests/e2e/digital-human-beta.spec.ts
- Test: frontend/tests/e2e/consent-withdrawal.spec.ts
- Test: frontend/tests/e2e/public-beta-release.spec.ts

- [ ] **Step 1: Write failing capability-specific release and rollback tests**

Create backend/tests/integration/governance/test_release_control.py:

~~~python
import pytest

from ip_saas.common.errors import Conflict


def test_approved_voice_decision_cannot_promote_portrait(release_harness) -> None:
    decision = release_harness.approved_decision("voice_clone")
    with pytest.raises(Conflict, match="does not approve"):
        release_harness.promote("seedance_authorized_portrait", decision.id)


def test_release_decision_is_consumed_exactly_once(release_harness) -> None:
    decision = release_harness.approved_decision("voice_clone")
    release_harness.promote("voice_clone", decision.id)
    release_harness.return_to_closed_beta("voice_clone")
    before = release_harness.durable_release_counts()
    with pytest.raises(Conflict, match="already executed"):
        release_harness.promote("voice_clone", decision.id)
    assert release_harness.durable_release_counts() == before


def test_decision_for_old_entitlement_cannot_promote_current_snapshot(
    release_harness,
) -> None:
    decision = release_harness.approved_decision("voice_clone")
    old_binding = release_harness.decision_binding(decision.id)
    current = release_harness.rotate_entitlement_and_pass_fresh_gates(
        "voice_clone", model_version="voice-v2", release_sha256="3" * 64
    )
    assert current.id != old_binding.entitlement_id
    with pytest.raises(Conflict, match="old entitlement snapshot"):
        release_harness.promote("voice_clone", decision.id)
    assert release_harness.decision(decision.id).executed_at is None


def test_platform_release_route_delegates_to_control_wrapper(
    release_harness,
) -> None:
    decision = release_harness.approved_decision("voice_clone")
    response = release_harness.platform_post(
        "/v1/platform/governance/capabilities/voice_clone/promote",
        {"release_decision_id": str(decision.id)},
    )
    assert response.status_code == 200
    assert release_harness.control_wrapper_calls() == [
        ("promote", "voice_clone", decision.id)
    ]
    assert release_harness.direct_flag_repository_write_count() == 0
    generic = release_harness.platform_post(
        "/v1/platform/governance/capabilities/voice_clone/stage",
        {"stage": "public_beta"},
    )
    assert generic.status_code in {404, 405}


def test_promotion_emits_typed_release_event(release_harness) -> None:
    decision = release_harness.approved_decision("voice_clone")
    flag = release_harness.promote("voice_clone", decision.id)
    event = release_harness.single_event("capability.release.executed")
    assert release_harness.decision(decision.id).executed_at is not None
    assert event.aggregate_id == decision.id
    assert event.payload == {
        "capability": "voice_clone",
        "flag_version": flag.version_no,
        "release_decision_id": str(decision.id),
    }


def test_rollback_stops_only_target_capability_and_fences_old_workers(
    release_harness,
) -> None:
    release_harness.enable_all_public_beta()
    old_worker = release_harness.start("traditional_digital_human")
    release_harness.rollback("traditional_digital_human")
    assert release_harness.stage("traditional_digital_human") == "off"
    assert release_harness.stage("voice_clone") == "public_beta"
    assert release_harness.stage("seedance_authorized_portrait") == "public_beta"
    assert release_harness.new_submission_rejected("traditional_digital_human")
    with pytest.raises(Conflict, match="attempt"):
        old_worker.write_success_after_reclaim()


def test_queued_work_releases_once_and_running_work_reconciles(release_harness) -> None:
    queued, running = release_harness.create_queued_and_running_tasks()
    release_harness.rollback(queued.capability)
    assert release_harness.hold(queued).status == "released"
    assert release_harness.delivery_blocked(running)
    assert release_harness.provider_cost_reconciled(running)
    assert release_harness.provider_cost(running).task_id == running.id
    assert release_harness.hold_mutation_count(queued) == 1
~~~

- [ ] **Step 2: Run release tests and verify control code is missing**

Run:

~~~bash
cd backend && uv run pytest tests/integration/governance/test_release_control.py -v
~~~

Expected: FAIL because release_control.py does not exist or the capability-specific check is not wired.

- [ ] **Step 3: Implement one-way promotion and capability-local rollback**

Create backend/src/ip_saas/scripts/release_control.py and import `event_envelope` and the common errors there. Its `promote(session, admin, capability, release_decision_id, services)` implementation is capability-local and transactional:

~~~python
def promote(session, admin, capability, release_decision_id, services):
    flag = services.flags.set_stage(
        session, admin, capability, CapabilityStage.PUBLIC_BETA,
        reason="approved public beta release",
        release_decision_id=release_decision_id,
    )
    now = flag.changed_at
    services.audit.write(
        session, actor=admin, action="capability.release.executed",
        target_type="capability_feature_flag", target_id=flag.id,
        project_id=None,
        metadata={"capability": capability.value,
                  "flag_version": flag.version_no,
                  "release_decision_id": str(release_decision_id)},
    )
    services.outbox.add(session, event_envelope(
        event_id=uuid4(), event_type="capability.release.executed",
        aggregate_id=release_decision_id,
        occurred_at=now,
        initiated_by_actor_id=admin.actor_id,
        idempotency_key=(
            f"release:{capability.value}:{flag.version_no}:{release_decision_id}"
        ),
        payload={
            "capability": capability.value,
            "flag_version": flag.version_no,
            "release_decision_id": release_decision_id,
        },
    ))
    return flag
~~~

`CapabilityFeatureFlagService.set_stage` is the sole state-transition authority and the sole writer of `ReleaseDecision.executed_at`; the wrapper never pre-consumes or directly mutates a decision. Under Task 4's shared capability advisory transaction lock plus flag, current-entitlement, and decision row locks, it enforces the frozen transition graph, re-selects the ACTIVE/unexpired exact entitlement, verifies current gate and release-decision bindings, and consumes a decision once before the wrapper emits release audit/outbox records in the same transaction. Entitlement rotation uses the same capability lock, so neither path has a TOCTOU gap. It refuses OFF-to-PUBLIC_BETA, and a decision for one capability cannot promote another. It never promotes more than one capability per transaction. Implement rollback as one database transaction:

~~~python
def rollback_capability(session, admin, capability, reason, services):
    flag = services.flags.set_stage(
        session, admin, capability, CapabilityStage.OFF,
        reason=reason, release_decision_id=None,
    )
    queued = services.jobs.lock_queued_for_capability(session, capability)
    for task, quota in queued:
        services.jobs.cancel_queued(session, task, reason)
        services.billing.release_generation(
            session, services.billing.context_for(task), reason,
            f"rollback:{capability.value}:{task.id}:release",
        )
        services.quota.release(session, quota.id,
                               f"rollback:{capability.value}:{quota.id}")
    running = services.jobs.lock_running_for_capability(session, capability)
    for task, identity_use in running:
        identity_use.delivery_blocked_at = services.clock.now()
        identity_use.delivery_block_reason = f"capability rollback: {reason}"[:500]
        services.jobs.stop_heartbeats_after_current_provider_poll(task.id)
    services.outbox.add(session, event_envelope(
        event_id=uuid4(), event_type="capability.rollback.requested",
        aggregate_id=flag.id, occurred_at=services.clock.now(),
        initiated_by_actor_id=admin.actor_id,
        idempotency_key=f"rollback:{capability.value}:{flag.version_no}",
        payload={
            "capability": capability.value,
            "flag_version": flag.version_no,
            "feature_flag_id": flag.id,
        },
    ))
    return flag
~~~

`cancel_queued` uses a compare-and-swap and terminal semantics from Plan 01. Running provider tasks are never reported free or cancelled unless the provider confirms cancellation; their final cost is settled, their attempt fence is honored, and their outputs remain quarantined/delivery-blocked. Historical published media is not erased by rollback; complaints, withdrawal, and deletion policies continue to govern it.

Append strict platform-control request/response schemas to backend/src/ip_saas/modules/governance/schemas.py:

~~~python
class CapabilityPromotionRequest(StrictModel):
    release_decision_id: UUID


class CapabilityRollbackRequest(StrictModel):
    reason: str = Field(min_length=3, max_length=500)


class CapabilityStageView(StrictModel):
    capability: BetaCapability
    stage: CapabilityStage
    version_no: int = Field(ge=1)
~~~

Append only these two stage-mutation routes to backend/src/ip_saas/modules/governance/router.py; import the three schemas, `BetaCapability`, and the Task 16 `promote`/`rollback_capability` wrappers. There is no generic target-stage request and no repository setter exposed through FastAPI:

~~~python
@platform_router.post(
    "/capabilities/{capability}/promote",
    response_model=CapabilityStageView,
)
def promote_capability(
    capability: BetaCapability,
    body: CapabilityPromotionRequest,
    request: Request,
    session: Session = Depends(get_session),
    actor: ActorContext = Depends(get_actor),
) -> CapabilityStageView:
    flag = promote(
        session,
        actor,
        capability,
        body.release_decision_id,
        request.app.state.governance_release_services,
    )
    return CapabilityStageView(
        capability=capability,
        stage=CapabilityStage(flag.stage),
        version_no=flag.version_no,
    )


@platform_router.post(
    "/capabilities/{capability}/rollback",
    response_model=CapabilityStageView,
)
def rollback_capability_route(
    capability: BetaCapability,
    body: CapabilityRollbackRequest,
    request: Request,
    session: Session = Depends(get_session),
    actor: ActorContext = Depends(get_actor),
) -> CapabilityStageView:
    flag = rollback_capability(
        session,
        actor,
        capability,
        body.reason,
        request.app.state.governance_release_services,
    )
    return CapabilityStageView(
        capability=capability,
        stage=CapabilityStage(flag.stage),
        version_no=flag.version_no,
    )
~~~

The production app constructs one `governance_release_services` composition whose `flags` member is `CapabilityFeatureFlagService`; the CLI and both routes use the same wrapper. A contract test searches router source/OpenAPI for a generic stage-update path and fails if one exists. The integration harness spies at the wrapper boundary, proving the route cannot directly update `CapabilityFeatureFlag.stage` or `ReleaseDecision.executed_at`.

- [ ] **Step 4: Freeze automatic rollback triggers**

Add these triggers to infra/runbooks/public-beta-release.md and governance-alerts.yaml:

- Immediate affected-capability OFF: any tenant leak, unauthorized identity/minor use, duplicate charge/refund, false-success state, missing final AI label, provider reference/secret leak, or deletion/withdrawal delivery violation.
- OFF within fifteen minutes: provider entitlement suspended, commercial contract expired, quota evidence invalid, critical complaint not frozen within one minute, or a required gate/attestation expired.
- Return to CLOSED_BETA after two consecutive ten-minute windows: task-receipt p95 >3s, non-AI p95 >2s, provider failure rate >5%, queue oldest age >120s, or QC failure >10%, if not explained by an approved drill.

No trigger automatically deletes identities or source media. Re-enable requires incident closure, current consent/rights, cost reconciliation, a new passing gate attestation, independent review, and a new ReleaseDecision.

- [ ] **Step 5: Implement the safe infrastructure rollback wrapper**

Create infra/scripts/rollback.sh:

~~~bash
#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 4 ]]; then
  echo "usage: rollback.sh <capability> <sha256:last-good-digest> <reason-file> <evidence-directory>" >&2
  exit 64
fi
rollback_capability="$1"
last_good_digest="$2"
reason_file="$3"
evidence_dir="$4"
case "$rollback_capability" in
  voice_clone|seedance_authorized_portrait|traditional_digital_human) ;;
  *) exit 65 ;;
esac
[[ "$last_good_digest" =~ ^sha256:[0-9a-f]{64}$ ]] || exit 66
[[ -f "$reason_file" && ! -e "$evidence_dir/SHA256SUMS" ]] || exit 67
mkdir -p "$evidence_dir"
uv run python -m ip_saas.scripts.release_control rollback \
  --capability "$rollback_capability" --reason-file "$reason_file" \
  > "$evidence_dir/control.json"
helm upgrade ip-saas infra/helm/ip-saas --namespace ip-saas \
  --atomic --wait --timeout 15m \
  -f infra/helm/ip-saas/values-cn-beijing.yaml \
  --set-string image.digest="$last_good_digest"
uv run python -m ip_saas.scripts.release_control verify-rollback \
  --capability "$rollback_capability" > "$evidence_dir/verification.json"
sha256sum "$evidence_dir"/*.json > "$evidence_dir/SHA256SUMS"
~~~

The wrapper turns the feature off before the image rollback. It never runs Alembic downgrade in production; schema rollback is a reviewed forward migration. It accepts a single explicit capability, never `all` or a wildcard.

- [ ] **Step 6: Run end-to-end customer, withdrawal, and release scenarios**

Implement the three frontend Playwright files from the file map:

1. digital-human-beta: create an adult subject, grant separate consents, enroll each capability independently, receive a task in under three seconds, generate, observe Beta badge, QC and both AI labels, and download only after exact-hash verification.
2. consent-withdrawal: withdraw while queued, running, and completed; assert queued refund/release once, running delivery blocked with honest status/cost, completed media quarantined, recycle entry visible, provider deletion waits for second confirmation, and reseller access remains 404.
3. public-beta-release: failed evidence produces HOLD; one approved capability promotes without siblings; high-risk self-marketing waits for independent PLATFORM_REVIEWER; trigger rollback and verify new work stops while history/audit remain.

Run against the isolated staging namespace and deterministic fake providers:

~~~bash
cd frontend && pnpm playwright test tests/e2e/digital-human-beta.spec.ts tests/e2e/consent-withdrawal.spec.ts tests/e2e/public-beta-release.spec.ts
~~~

Expected: all three pass. The staging run validates workflow, not production entitlements or production provider/RocketMQ evidence.

- [ ] **Step 7: Add final Make targets and operator documentation**

Modify Makefile with `governance-unit`, `governance-integration`, `governance-contract`, `governance-security`, `governance-resilience`, `governance-helm`, `governance-e2e`, and `governance-check`. `governance-check` runs all non-paid, non-destructive suites; real provider, structured-output, RocketMQ, restore, load, security, and six-week evaluations remain separately approved evidence commands.

Update README.md with capability/stage meanings, exact Beta disclaimer, supported platforms Douyin/Xiaohongshu, roles, consent lifecycle, complaint path, recycle/permanent deletion, task lease semantics, deployment prerequisites, and links to all four runbooks. Explicitly list the non-goals: team collaboration, online payment, a third reseller level, auto publishing/TikTok API integration, overseas identity transfer, and digital-human GA/SLA.

- [ ] **Step 8: Run the full non-paid acceptance suite**

Run:

~~~bash
make governance-check
cd backend && uv run pytest tests/contract/governance/test_fixture_closure.py -v
cd backend && uv run alembic upgrade 0006_governance
cd backend && uv run alembic downgrade 0005_resellers && uv run alembic upgrade 0006_governance
git diff --check
~~~

Run the plan-document static audit from the repository root:

~~~bash
python3 - <<'PY'
from pathlib import Path
import ast
import re

path = Path("docs/superpowers/plans/2026-08-24-06-digital-human-public-beta.md")
text = path.read_text(encoding="utf-8")
fence = "~" * 3
fence_lines = [line for line in text.splitlines() if line.startswith(fence)]
assert len(fence_lines) % 2 == 0
blocks = re.findall(rf"{fence}python\n(.*?){fence}", text, re.S)
assert blocks
for index, block in enumerate(blocks, 1):
    ast.parse(block, filename=f"plan06-python-block-{index}")
print(f"paired fences; Python AST clean: {len(blocks)} blocks")
PY
rg -n "T[B]D|T[O]DO|F[I]XME|place[h]older|implement l[a]ter|fill i[n]" docs/superpowers/plans/2026-08-24-06-digital-human-public-beta.md
~~~

Expected: all acceptance commands exit 0 except the unfinished-marker `rg`, which exits 1 with no output; the static script reports paired fences and every Python block parses. Migration round-trip succeeds against PostgreSQL; the fixture-closure contract reports no missing fixture; no SQLite test substitutes for transaction semantics; OpenAPI/event snapshots are current; frontend lint/typecheck/tests pass; Helm render and security checks pass.

- [ ] **Step 9: Verify the real evidence packet before any PUBLIC_BETA flag**

The release reviewer checks one immutable manifest containing:

- target capability's production permission, actual quota, signed contract, direct consent rehearsal, ≥30-item batch PoC, cost, failure recovery, media QC, label, revocation, deletion, complaint, and structured-output gate evidence; persisted effective GenerationLimitVersion rows cover every customer account and internal cost center; the current hash-pinned provider-crash attestation re-verifies its official complete request log, finalized supplier statement, exact SIGKILL transcript, stable attempt manifest, deployed reconciler, and statement-line-to-cost mapping before the supplemental anti-join confirms every real provider completion has exactly one `ProviderCostEntry.task_id = TaskRecord.id` with a canonical real supplier request ID and no null or cross-task lineage; durable zero-call evidence proves zero provider calls, zero ProviderCostInput constructions, and zero cost rows before release;
- the PostgreSQL two-Session rotation/promotion race passes in both operation orders: the common capability advisory transaction lock serializes the critical section, the winner's entitlement/stage remains internally consistent, a rotated snapshot cannot consume an old decision, and a PUBLIC_BETA capability cannot rotate until demoted;
- production RocketMQ PoC; current security/load/fault evidence; a monthly restore and current quarterly full DR evidence with RPO≤3600/RTO≤14400; current external legal/commercial/minor/MLPS attestations;
- frozen five-project/six-week dataset; ≥12 publications each and ≥60 total; full T1/T3/T7/retro lineage; four customer-case results; isolated three-track self-marketing results; ≥100 dual blind pairs with ≥70% wins; exactly 1,000 structured cases with ≥99% within three attempts and zero unsafe false success;
- two independent release reviewers, product-owner approval evidence, candidate and last-good image digests, release/rollback commands, and verified evidence SHA-256.

Any missing, expired, unverifiable, manually edited, fake-only, or threshold-failing item yields HOLD. Public Beta remains labeled Beta after release.

- [ ] **Step 10: Commit release controls and final documentation**

~~~bash
git add backend/src/ip_saas/modules/governance/beta.py backend/src/ip_saas/modules/governance/router.py backend/src/ip_saas/modules/governance/schemas.py backend/src/ip_saas/scripts/release_control.py backend/tests/integration/governance/test_release_control.py infra/scripts/rollback.sh infra/runbooks/public-beta-release.md infra/observability/governance-alerts.yaml Makefile README.md frontend/tests/e2e
git commit -m "feat: release and rollback digital human public beta"
~~~

## Official implementation references

- Volcengine Ark Seedance task API and verified real-person asset documentation: use the production-account entitlement and asset verification flow current at execution time; save the exact documentation version and entitlement response in the PoC evidence.
- Volcengine Doubao speech voice-clone service: use the activated production service, paid voice slot, UsageMonitoring/QuotaMonitoring evidence, and current training/status API; save request IDs and service terms without tokens or training audio.
- Volcengine cloned digital-human OpenAPI: service code `cv`, version `2024-06-06`, and the four frozen training/creation actions in Task 6.
- [RDS for PostgreSQL RestoreToNewInstance](https://www.volcengine.com/docs/6438/1159934) and API version `2022-01-01`: restore drills create a new protected instance.
- [TOS versioning](https://www.volcengine.com/docs/6349/261963) and the TOS SDK versioned-object restore APIs: backup tests verify exact versions and SHA-256.
- [VKE prometheus-agent](https://www.volcengine.com/docs/6460/699875) and VMP ServiceMonitor support: production preflight verifies the installed compatible add-on rather than assuming it exists.
- [人工智能生成合成内容标识办法](https://www.cac.gov.cn/2025-03/14/c_1743654684782215.htm): the launch legal attestation records the applicable obligations and implementation evidence; engineers do not infer legal approval from code tests.

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-24-06-digital-human-public-beta.md`.

Two execution options:

1. **Subagent-driven (this session):** execute one task at a time, review after each task, and stop at every paid production PoC or external approval gate.
2. **Separate execution session:** open a new implementation session with the writing-plans execution workflow, run tasks in order, and checkpoint at the same production and approval gates.

In either mode, Tasks 1–16 are implemented in order. A test, fake provider, console setting, or subjective approval never substitutes for the production evidence explicitly required above.
