# Generative Media Studio Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a production-safe media studio in which human production packages, Seedream images, Seedance video, standard Doubao TTS, and mixed productions share one scene/shot/job workflow from storyboard through low-resolution approval, high-resolution generation, private storage, finishing, AI labeling, quality control, and per-shot recovery.

**Architecture:** Extend the modular monolith with a `media` domain and synchronous SQLAlchemy 2.0 services, while independent synchronous workers submit and poll external asynchronous tasks. API requests only validate, reserve exactly one billing context for each billable job, persist the job, and write an outbox event in one PostgreSQL transaction. Provider, temporary-object, private-storage, FFmpeg, and semantic-QC boundaries are ports with deterministic no-credential fakes; real Volcengine adapters are selected through the model registry and never leak temporary provider URLs to users.

**Tech Stack:** Python 3.12, FastAPI 0.136.x, Pydantic 2.x, SQLAlchemy 2.0 synchronous `Session`, Alembic, PostgreSQL, RocketMQ outbox delivery, HTTPX, Volcengine Ark Seedream and Seedance APIs, Doubao Speech standard TTS V3, TOS private object storage, FFmpeg/FFprobe, Pytest, Hypothesis, Next.js 16 App Router, React 19, TypeScript 5.x, Vitest, Playwright.

---

## Prerequisites, frozen seams, and file map

Execute Plans 01 and 02 first. This plan does not create another creative-master model: `MediaProduction.content_version_id` references the frozen master fields on `ip_saas.modules.intelligence.models.ContentVersion`, and `MediaProduction.platform_variant_id` references `ip_saas.modules.intelligence.models.PlatformVariant`.

Use these already-frozen imports and signatures exactly:

```python
from ip_saas.common.audit import AuditWriter
from ip_saas.common.clock import Clock
from ip_saas.common.errors import Conflict, DomainError, Forbidden, NotFound
from ip_saas.common.outbox import EventEnvelope, OutboxWriter
from ip_saas.db.base import Base
from ip_saas.db.session import session_scope
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.billing.ports import CreditHoldPort, InternalBudgetHoldPort
from ip_saas.modules.billing.service import (
    BillingContext,
    BillingMode,
    BillingService,
    ProviderCostInput,
)
from ip_saas.modules.intelligence.models import ContentVersion, PlatformVariant
from ip_saas.modules.projects.access import ProjectAccessService
from ip_saas.modules.projects.models import IPProject
from ip_saas.modules.tasks.models import TaskRecord
from ip_saas.modules.tasks.reconciliation import TaskReconciliationService
from ip_saas.modules.tasks.service import TaskSubmissionService
```

```text
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
BillingService.reserve_customer_generation(
    session,
    account_id,
    credit_units,
    idempotency_key,
    actor,
) -> BillingContext
BillingService.reserve_internal_generation(
    session,
    cost_center_id,
    amount_fen,
    idempotency_key,
    actor,
) -> BillingContext
BillingService.settle_generation(
    session,
    context,
    actual_amount,
    provider_cost,
    idempotency_key,
) -> None
BillingService.release_generation(
    session,
    context,
    reason,
    idempotency_key,
) -> None
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
    session,
    task_id,
    lease_seconds=300,
    max_attempts=8,
) -> TaskRecord
TaskSubmissionService.heartbeat(
    session,
    task_id,
    attempt_no,
    lease_seconds=300,
) -> TaskRecord
TaskSubmissionService.succeed(
    session,
    task_id,
    attempt_no,
    result_payload,
) -> TaskRecord
TaskSubmissionService.fail(
    session,
    task_id,
    attempt_no,
    error_code,
    error_message,
) -> TaskRecord
TaskReconciliationService.finalize_no_provider_call(
    session,
    task_id,
    attempt_no,
    reason,
    idempotency_key,
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
```

`ProviderCostInput` has the frozen fields `provider`, `provider_request_id`, `capability`, `model_id`, `model_version`, `native_quantity`, `native_unit`, `supplier_amount_minor`, `supplier_currency`, `amount_fen`, `reconciliation_status`, and `task_id: UUID | None = None`. Every real Seedream, Seedance, or TTS completion sets `task_id` to the current `TaskRecord.id` and preserves its exact provider request identity as canonical trimmed text of 1–200 characters; a null, non-string, blank, literal `None`, or 201-character supplier identity is rejected before evidence or ledger persistence. `provider_task_id` is a separate nullable polling locator and is never substituted into `provider_request_id`. A proven zero-call path creates no `ProviderCostInput` or `ProviderCostEntry`; a deterministic fake creates cost evidence only when it supplies an explicit canonical fake request identity and exact test task lineage. Distinct supplier requests never collapse into one cost row. `actual_amount` means customer `credit_units` for `CUSTOMER_CREDIT` and integer CNY `amount_fen` for `INTERNAL_COST`. `TaskRecord` is the only top-level task truth and owns `billing_mode`, `billing_hold_id`, `attempt_no`, `lease_expires_at`, input/result/error payloads, and lifecycle CAS. `GenerationJob` has a unique `task_record_id` and keeps only provider/copy/finishing/QC substate. Code reconstructs billing context only as `BillingContext(mode=BillingMode(task.billing_mode), hold_id=task.billing_hold_id)`; it never adds media-specific hold columns.

`start()` atomically claims a queued or expired-running task and increments `attempt_no`, raises `Conflict` for an active lease, and returns an already-terminal task unchanged. `heartbeat()`, `succeed()`, and `fail()` compare-and-set `status="running"`, the captured `attempt_no`, and an unexpired lease. After any lease/CAS loss, the Worker re-reads `TaskRecord`: it ACKs only if the task is terminal or `task.attempt_no` is strictly greater than the captured attempt, and otherwise RETRYs. Therefore a missing row, same-attempt expiry, active same attempt, or same-attempt `reconciliation_required` is never swallowed; none may write a domain row, ledger entry, delivery link, or terminal task result. When `max_attempts` is exhausted before supplier usage is proven, Plan 01 moves the task to non-terminal `reconciliation_required` and keeps the hold active. Every media handler must return `RETRY` without provider, domain, billing, QC, or delivery work until the independent reconciliation scanner proves zero calls or groups every matched supplier line by TaskRecord and invokes Plan 01 `TaskReconciliationService.finalize_provider_costs` exactly once for that task. The batch records distinct task-linked `(provider, provider_request_id)` costs with `ReconciliationStatus.MATCHED`, releases customer credit with `actual_amount=0` or settles internal CNY with `actual_amount=sum(cost.amount_fen)`, and moves the task once to FAILED in the same transaction. Workers preserve the `attempt_no` returned by `start()` through every provider, copy, finishing, QC, settlement, and terminal call.

All application and worker code in this plan uses a synchronous `Session`. Do not import `AsyncSession`; a request transaction never performs a paid HTTP call or polls a provider. The main migration is fixed to `revision = "0003_media"` and `down_revision = "0002_intelligence"`.

Plan 01 already freezes `GenerationLimitPort` as the mandatory third `BillingService` constructor argument. Before Plan 05, production composition uses `ip_saas.modules.billing.limits.UnconfiguredGenerationLimits`, so paid creation fails closed, while every Plan 03 unit or integration fixture explicitly imports `tests.support.generation_limits.AllowAllGenerationLimits`; there is no omitted/default bypass. Plan 03 must not import Plan 05's database service or `tests.fixtures.generation_limits` while the database is only at `0003_media`. After Plan 05 merges, its composition replaces the deny-all port with the real database-backed limit service, integration scenarios create effective account/internal limit versions through `allow_customer()` or `allow_internal()`, and the complete Plan 01 task/billing, Plan 03 media, and Plan 05 reseller/limit suites run together.

The formal cloned-digital-human, cloned-voice, real-person enrollment, consent withdrawal/deletion, and production-account permission program belongs to Plan 06. Plan 03 rejects `cloned_voice`, ordinary real-face reference uploads, `omnihuman`, `chimera`, and audio-driven avatar capabilities. The only standard voice source here is an allow-listed Doubao standard TTS voice; an uploaded human recording may be used as already-produced source audio. Seedance real-person `asset://` references stay disabled unless Plan 06 later supplies the rights-and-capability gate.

**Files created or modified by this plan:**

```text
backend/migrations/versions/0003_media.py
backend/src/ip_saas/api.py
backend/src/ip_saas/config.py
backend/src/ip_saas/modules/media/__init__.py
backend/src/ip_saas/modules/media/enums.py
backend/src/ip_saas/modules/media/schemas.py
backend/src/ip_saas/modules/media/state.py
backend/src/ip_saas/modules/media/models/__init__.py
backend/src/ip_saas/modules/media/models/identity.py
backend/src/ip_saas/modules/media/models/production.py
backend/src/ip_saas/modules/media/models/jobs.py
backend/src/ip_saas/modules/media/models/provider_ops.py
backend/src/ip_saas/modules/media/rights.py
backend/src/ip_saas/modules/media/routing.py
backend/src/ip_saas/modules/media/storyboard.py
backend/src/ip_saas/modules/media/service.py
backend/src/ip_saas/modules/media/router.py
backend/src/ip_saas/modules/media/events.py
backend/src/ip_saas/modules/media/dependencies.py
backend/src/ip_saas/modules/media/delivery.py
backend/src/ip_saas/modules/media/ffmpeg.py
backend/src/ip_saas/modules/media/captions.py
backend/src/ip_saas/modules/media/labels.py
backend/src/ip_saas/modules/media/qc.py
backend/src/ip_saas/modules/media/provider_poc.py
backend/src/ip_saas/modules/media/provider_reconciliation.py
backend/src/ip_saas/modules/media/provider_release.py
backend/src/ip_saas/providers/media/__init__.py
backend/src/ip_saas/providers/media/base.py
backend/src/ip_saas/providers/media/composition.py
backend/src/ip_saas/providers/media/fake.py
backend/src/ip_saas/providers/media/temporary.py
backend/src/ip_saas/providers/media/tos.py
backend/src/ip_saas/providers/volcengine/__init__.py
backend/src/ip_saas/providers/volcengine/auth.py
backend/src/ip_saas/providers/volcengine/seedream.py
backend/src/ip_saas/providers/volcengine/seedance.py
backend/src/ip_saas/providers/volcengine/tts.py
backend/src/ip_saas/workers/media_submit.py
backend/src/ip_saas/workers/media_poll.py
backend/src/ip_saas/workers/media_copy.py
backend/src/ip_saas/workers/media_finish.py
backend/src/ip_saas/workers/media_lease.py
backend/src/ip_saas/workers/media_local.py
backend/src/ip_saas/workers/media_qc.py
backend/src/ip_saas/workers/media_recovery.py
backend/scripts/run_media_provider_poc.py
backend/tests/unit/media/
backend/tests/integration/media/
backend/tests/contract/media/
backend/tests/security/test_media_tenant_isolation.py
backend/tests/fixtures/media/
contracts/events/media.copy.requested.v1.json
contracts/events/media.job.failed.v1.json
contracts/events/media.job.poll.requested.v1.json
contracts/events/media.qc.requested.v1.json
contracts/events/media.human_review.requested.v1.json
contracts/events/media.redo.requested.v1.json
frontend/src/app/(c-user)/projects/[projectId]/content/[contentVersionId]/studio/page.tsx
frontend/src/features/media/api.ts
frontend/src/features/media/types.ts
frontend/src/features/media/MediaStudio.tsx
frontend/src/features/media/media-studio.test.tsx
frontend/tests/e2e/media-studio.spec.ts
docs/runbooks/media-provider-crash-poc.md
Makefile
```

### Task 1: Freeze media vocabulary and strict request contracts

**Files:**
- Create: `backend/src/ip_saas/modules/media/__init__.py`
- Create: `backend/src/ip_saas/modules/media/enums.py`
- Create: `backend/src/ip_saas/modules/media/schemas.py`
- Test: `backend/tests/unit/media/test_schemas.py`

- [ ] **Step 1: Write the failing contract tests**

```python
from uuid import uuid4

import pytest
from pydantic import ValidationError

from ip_saas.modules.media.enums import ProductionMode, ShotSource, VoiceKind
from ip_saas.modules.media.schemas import (
    CostApprovalRequest,
    ProductionCreate,
    ProviderCallback,
    RedoShotRequest,
    ShotDraft,
)


def test_production_requires_the_frozen_content_and_variant_lineage() -> None:
    value = ProductionCreate(
        content_version_id=uuid4(),
        platform_variant_id=uuid4(),
        mode=ProductionMode.MIXED,
        output_aspect_ratio="9:16",
    )
    assert value.mode is ProductionMode.MIXED


def test_plan_03_rejects_cloned_voice_and_real_person_asset_references() -> None:
    with pytest.raises(ValidationError, match="standard_tts"):
        ShotDraft(
            sequence=1,
            title="送礼后的沉默",
            source=ShotSource.SEEDANCE,
            duration_ms=5000,
            prompt="客户把礼盒放在桌上，镜头缓慢推近",
            spoken_text="有些礼，送出去才知道分量。",
            voice_kind="cloned_voice",
        )
    with pytest.raises(ValidationError, match="asset://"):
        ShotDraft(
            sequence=1,
            title="真人参考",
            source=ShotSource.SEEDANCE,
            duration_ms=5000,
            prompt="asset://unverified-real-person walks into frame",
            voice_kind=VoiceKind.STANDARD_TTS,
        )


def test_cost_approval_accepts_exactly_one_quote_dimension() -> None:
    with pytest.raises(ValidationError, match="exactly one"):
        CostApprovalRequest(credit_units=30, amount_fen=900)
    with pytest.raises(ValidationError, match="exactly one"):
        CostApprovalRequest()
    assert CostApprovalRequest(credit_units=30).credit_units == 30


def test_redo_reason_and_provider_callback_are_strict() -> None:
    request = RedoShotRequest(reason="右手多出一根手指", user_requested=True)
    assert request.user_requested is True
    with pytest.raises(ValidationError):
        ProviderCallback.model_validate(
            {
                "provider": "seedance",
                "provider_task_id": "task-1",
                "status": "succeeded",
                "unknown": "must fail",
            }
        )
```

- [ ] **Step 2: Run the tests and verify the missing-module failure**

Run: `cd backend && uv run pytest tests/unit/media/test_schemas.py -v`

Expected: FAIL during collection with `ModuleNotFoundError: No module named 'ip_saas.modules.media'`.

- [ ] **Step 3: Create the complete enum vocabulary**

```python
from enum import StrEnum


class ProductionMode(StrEnum):
    HUMAN = "human"
    AI = "ai"
    MIXED = "mixed"


class ProductionStatus(StrEnum):
    DRAFT = "draft"
    STORYBOARD_READY = "storyboard_ready"
    STORYBOARD_APPROVED = "storyboard_approved"
    LOW_RES_RUNNING = "low_res_running"
    LOW_RES_READY = "low_res_ready"
    LOW_RES_APPROVED = "low_res_approved"
    HIGH_RES_RUNNING = "high_res_running"
    FINISHING = "finishing"
    QC_PENDING = "qc_pending"
    PARTIAL_REDO = "partial_redo"
    READY = "ready"
    FAILED = "failed"


class ShotSource(StrEnum):
    HUMAN_CAPTURE = "human_capture"
    SEEDREAM = "seedream"
    SEEDANCE = "seedance"
    UPLOADED_MEDIA = "uploaded_media"


class MediaKind(StrEnum):
    IMAGE = "image"
    VIDEO = "video"
    AUDIO = "audio"
    DOCUMENT = "document"


class VoiceKind(StrEnum):
    STANDARD_TTS = "standard_tts"
    UPLOADED_RECORDING = "uploaded_recording"


class Capability(StrEnum):
    HUMAN_PACKAGE = "human_package"
    STORYBOARD_IMAGE = "storyboard_image"
    LOW_RES_VIDEO = "low_res_video"
    HIGH_RES_VIDEO = "high_res_video"
    STANDARD_TTS = "standard_tts"
    FINISH = "finish"
    QC = "qc"


class JobStatus(StrEnum):
    DRAFT = "draft"
    ESTIMATED = "estimated"
    HELD = "held"
    SUBMITTED = "submitted"
    QUEUED = "queued"
    RUNNING = "running"
    SUCCEEDED_PENDING_COPY = "succeeded_pending_copy"
    COPIED_PENDING_QC = "copied_pending_qc"
    PASSED = "passed"
    REDO = "redo"
    FAILED = "failed"


class SettlementStatus(StrEnum):
    PENDING = "pending"
    SETTLED = "settled"
    RELEASED = "released"


class ProviderPhase(StrEnum):
    QUEUED = "queued"
    RUNNING = "running"
    SUCCEEDED = "succeeded"
    FAILED = "failed"


class RightsType(StrEnum):
    PORTRAIT = "portrait"
    VOICE = "voice"
    SOURCE_MEDIA = "source_media"
    PRODUCT = "product"


class AssetKind(StrEnum):
    CHARACTER_REFERENCE = "character_reference"
    PRODUCT_REFERENCE = "product_reference"
    SOURCE_MEDIA = "source_media"
    STORYBOARD = "storyboard"
    LOW_RES_PREVIEW = "low_res_preview"
    GENERATED_IMAGE = "generated_image"
    GENERATED_VIDEO = "generated_video"
    GENERATED_AUDIO = "generated_audio"
    CAPTION = "caption"
    HUMAN_PACKAGE = "human_package"
    FINAL_VIDEO = "final_video"


class AiDisclosure(StrEnum):
    NONE = "none"
    ASSISTED = "assisted"
    GENERATED = "generated"


class QcDecision(StrEnum):
    PASS = "pass"
    REDO = "redo"
    FAIL = "fail"


class FinishingStatus(StrEnum):
    NOT_REQUIRED = "not_required"
    PENDING = "pending"
    RUNNING = "running"
    SUCCEEDED = "succeeded"
    FAILED = "failed"


class HumanReviewDecision(StrEnum):
    PASS = "pass"
    REDO = "redo"
    FAIL = "fail"


class RedoOrigin(StrEnum):
    SYSTEM = "system"
    USER = "user"
```

Save this as `backend/src/ip_saas/modules/media/enums.py`. Create an empty `backend/src/ip_saas/modules/media/__init__.py`.

- [ ] **Step 4: Create strict Pydantic request and callback contracts**

```python
from datetime import datetime
from typing import Any, Literal
from uuid import UUID

from pydantic import (
    BaseModel,
    ConfigDict,
    Field,
    HttpUrl,
    model_validator,
)

from .enums import (
    AiDisclosure,
    JobStatus,
    HumanReviewDecision,
    ProductionMode,
    ProviderPhase,
    QcDecision,
    ShotSource,
    VoiceKind,
)


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class ProductionCreate(StrictModel):
    content_version_id: UUID
    platform_variant_id: UUID
    mode: ProductionMode
    output_aspect_ratio: Literal["9:16", "3:4", "1:1", "16:9"]


class SceneDraft(StrictModel):
    sequence: int = Field(ge=1, le=100)
    title: str = Field(min_length=1, max_length=120)
    purpose: str = Field(min_length=1, max_length=500)
    location: str = Field(min_length=1, max_length=200)


class ShotDraft(StrictModel):
    sequence: int = Field(ge=1, le=500)
    title: str = Field(min_length=1, max_length=120)
    source: ShotSource
    duration_ms: int = Field(ge=500, le=30_000)
    prompt: str = Field(min_length=1, max_length=4_000)
    spoken_text: str | None = Field(default=None, max_length=2_000)
    character_id: UUID | None = None
    voice_id: UUID | None = None
    voice_kind: VoiceKind | None = None
    product_reference_asset_id: UUID | None = None

    @model_validator(mode="after")
    def keep_beta_identity_inputs_out_of_plan_03(self) -> "ShotDraft":
        if "asset://" in self.prompt.lower():
            raise ValueError("asset:// real-person references are gated until Plan 06")
        if self.voice_kind is not None and self.voice_kind not in {
            VoiceKind.STANDARD_TTS,
            VoiceKind.UPLOADED_RECORDING,
        }:
            raise ValueError("voice_kind must be standard_tts or uploaded_recording")
        return self


class StoryboardCreate(StrictModel):
    scenes: list[SceneDraft] = Field(min_length=1, max_length=100)
    shots: dict[int, list[ShotDraft]]


class CostApprovalRequest(StrictModel):
    credit_units: int | None = Field(default=None, ge=1)
    amount_fen: int | None = Field(default=None, ge=1)
    quote_version: str = Field(default="v1", min_length=1, max_length=80)

    @model_validator(mode="after")
    def require_exactly_one_money_dimension(self) -> "CostApprovalRequest":
        if (self.credit_units is None) == (self.amount_fen is None):
            raise ValueError("exactly one of credit_units or amount_fen is required")
        return self


class RedoShotRequest(StrictModel):
    reason: str = Field(min_length=1, max_length=500)
    user_requested: bool


class ProviderCallback(StrictModel):
    provider: str = Field(min_length=1, max_length=60)
    provider_task_id: str = Field(min_length=1, max_length=300)
    provider_request_id: str = Field(min_length=1, max_length=200)
    status: ProviderPhase
    temporary_url: HttpUrl | None = None
    expires_at: datetime | None = None
    model_id: str | None = Field(default=None, max_length=200)
    model_version: str | None = Field(default=None, max_length=100)
    native_quantity: int | None = Field(default=None, ge=0)
    native_unit: str | None = Field(default=None, max_length=60)
    supplier_amount_minor: int | None = Field(default=None, ge=0)
    supplier_currency: str | None = Field(default=None, pattern=r"^[A-Z]{3}$")
    raw: dict[str, Any] = Field(default_factory=dict)

    @model_validator(mode="after")
    def successful_callbacks_require_copyable_output(self) -> "ProviderCallback":
        if (
            self.provider_request_id != self.provider_request_id.strip()
            or self.provider_request_id.casefold() in {"none", "missing"}
        ):
            raise ValueError("provider_request_id is not canonical supplier text")
        if self.status is ProviderPhase.SUCCEEDED:
            if self.temporary_url is None or self.expires_at is None:
                raise ValueError("successful callbacks require temporary_url and expires_at")
        return self


class QcIssueView(StrictModel):
    code: str
    message: str
    logical_shot_id: UUID | None = None


class JobView(StrictModel):
    id: UUID
    task_id: UUID
    shot_id: UUID | None
    status: JobStatus
    progress_percent: int = Field(ge=0, le=100)


class QcReportView(StrictModel):
    decision: QcDecision
    issues: list[QcIssueView]


class QcHumanReviewRequest(StrictModel):
    decision: HumanReviewDecision
    note: str = Field(min_length=1, max_length=2_000)
```

Save this as `backend/src/ip_saas/modules/media/schemas.py`.

- [ ] **Step 5: Run the contract tests**

Run: `cd backend && uv run pytest tests/unit/media/test_schemas.py -v`

Expected: 4 passed.

- [ ] **Step 6: Commit the vocabulary and contracts**

```bash
git add backend/src/ip_saas/modules/media backend/tests/unit/media/test_schemas.py
git commit -m "feat: define media studio contracts"
```

### Task 2: Make job and production transitions explicit and replay-safe

**Files:**
- Create: `backend/src/ip_saas/modules/media/state.py`
- Test: `backend/tests/unit/media/test_state.py`

- [ ] **Step 1: Write failing tests for legal, duplicate, and out-of-order transitions**

```python
import pytest

from ip_saas.common.errors import Conflict
from ip_saas.modules.media.enums import JobStatus, ProductionStatus
from ip_saas.modules.media.state import advance_job, advance_production


def test_billable_job_must_cross_held_before_submit() -> None:
    with pytest.raises(Conflict, match="illegal media job transition"):
        advance_job(JobStatus.ESTIMATED, JobStatus.SUBMITTED, billable=True)
    result = advance_job(JobStatus.ESTIMATED, JobStatus.HELD, billable=True)
    assert result.changed is True
    assert result.status is JobStatus.HELD


def test_non_billable_local_job_skips_hold() -> None:
    result = advance_job(JobStatus.ESTIMATED, JobStatus.SUBMITTED, billable=False)
    assert result.status is JobStatus.SUBMITTED


def test_duplicate_callback_is_a_no_op_and_regression_is_rejected() -> None:
    duplicate = advance_job(JobStatus.RUNNING, JobStatus.RUNNING, billable=True)
    assert duplicate.changed is False
    with pytest.raises(Conflict, match="illegal media job transition"):
        advance_job(JobStatus.SUCCEEDED_PENDING_COPY, JobStatus.RUNNING, billable=True)


def test_production_cannot_start_high_resolution_before_preview_approval() -> None:
    with pytest.raises(Conflict, match="illegal media production transition"):
        advance_production(
            ProductionStatus.LOW_RES_READY,
            ProductionStatus.HIGH_RES_RUNNING,
        )
    assert advance_production(
        ProductionStatus.LOW_RES_READY,
        ProductionStatus.LOW_RES_APPROVED,
    ).changed is True
```

- [ ] **Step 2: Run the tests and verify the missing-module failure**

Run: `cd backend && uv run pytest tests/unit/media/test_state.py -v`

Expected: FAIL during collection with `ModuleNotFoundError: No module named 'ip_saas.modules.media.state'`.

- [ ] **Step 3: Implement the complete transition policy**

```python
from dataclasses import dataclass

from ip_saas.common.errors import Conflict

from .enums import JobStatus, ProductionStatus


@dataclass(frozen=True)
class JobTransition:
    status: JobStatus
    changed: bool


@dataclass(frozen=True)
class ProductionTransition:
    status: ProductionStatus
    changed: bool


_BILLABLE_JOB_TRANSITIONS: dict[JobStatus, frozenset[JobStatus]] = {
    JobStatus.DRAFT: frozenset({JobStatus.ESTIMATED}),
    JobStatus.ESTIMATED: frozenset({JobStatus.HELD, JobStatus.FAILED}),
    JobStatus.HELD: frozenset({JobStatus.SUBMITTED, JobStatus.FAILED}),
    JobStatus.SUBMITTED: frozenset(
        {
            JobStatus.QUEUED,
            JobStatus.RUNNING,
            JobStatus.SUCCEEDED_PENDING_COPY,
            JobStatus.FAILED,
        }
    ),
    JobStatus.QUEUED: frozenset(
        {JobStatus.RUNNING, JobStatus.SUCCEEDED_PENDING_COPY, JobStatus.FAILED}
    ),
    JobStatus.RUNNING: frozenset(
        {JobStatus.SUCCEEDED_PENDING_COPY, JobStatus.FAILED}
    ),
    JobStatus.SUCCEEDED_PENDING_COPY: frozenset(
        {JobStatus.COPIED_PENDING_QC, JobStatus.FAILED}
    ),
    JobStatus.COPIED_PENDING_QC: frozenset(
        {JobStatus.PASSED, JobStatus.REDO, JobStatus.FAILED}
    ),
    JobStatus.PASSED: frozenset(),
    JobStatus.REDO: frozenset(),
    JobStatus.FAILED: frozenset(),
}

_NON_BILLABLE_JOB_TRANSITIONS = {
    **_BILLABLE_JOB_TRANSITIONS,
    JobStatus.ESTIMATED: frozenset({JobStatus.SUBMITTED, JobStatus.FAILED}),
}

_PRODUCTION_TRANSITIONS: dict[ProductionStatus, frozenset[ProductionStatus]] = {
    ProductionStatus.DRAFT: frozenset(
        {ProductionStatus.STORYBOARD_READY, ProductionStatus.FAILED}
    ),
    ProductionStatus.STORYBOARD_READY: frozenset(
        {ProductionStatus.STORYBOARD_APPROVED, ProductionStatus.FAILED}
    ),
    ProductionStatus.STORYBOARD_APPROVED: frozenset(
        {
            ProductionStatus.LOW_RES_RUNNING,
            ProductionStatus.LOW_RES_READY,
            ProductionStatus.FAILED,
        }
    ),
    ProductionStatus.LOW_RES_RUNNING: frozenset(
        {ProductionStatus.LOW_RES_READY, ProductionStatus.PARTIAL_REDO, ProductionStatus.FAILED}
    ),
    ProductionStatus.LOW_RES_READY: frozenset(
        {ProductionStatus.LOW_RES_APPROVED, ProductionStatus.PARTIAL_REDO, ProductionStatus.FAILED}
    ),
    ProductionStatus.LOW_RES_APPROVED: frozenset(
        {ProductionStatus.HIGH_RES_RUNNING, ProductionStatus.FINISHING, ProductionStatus.FAILED}
    ),
    ProductionStatus.HIGH_RES_RUNNING: frozenset(
        {ProductionStatus.FINISHING, ProductionStatus.PARTIAL_REDO, ProductionStatus.FAILED}
    ),
    ProductionStatus.FINISHING: frozenset(
        {ProductionStatus.QC_PENDING, ProductionStatus.PARTIAL_REDO, ProductionStatus.FAILED}
    ),
    ProductionStatus.QC_PENDING: frozenset(
        {ProductionStatus.READY, ProductionStatus.PARTIAL_REDO, ProductionStatus.FAILED}
    ),
    ProductionStatus.PARTIAL_REDO: frozenset(
        {
            ProductionStatus.LOW_RES_RUNNING,
            ProductionStatus.HIGH_RES_RUNNING,
            ProductionStatus.FINISHING,
            ProductionStatus.FAILED,
        }
    ),
    ProductionStatus.READY: frozenset({ProductionStatus.PARTIAL_REDO}),
    ProductionStatus.FAILED: frozenset({ProductionStatus.PARTIAL_REDO}),
}


def advance_job(
    current: JobStatus,
    target: JobStatus,
    *,
    billable: bool,
) -> JobTransition:
    if current is target:
        return JobTransition(status=current, changed=False)
    transitions = (
        _BILLABLE_JOB_TRANSITIONS if billable else _NON_BILLABLE_JOB_TRANSITIONS
    )
    if target not in transitions[current]:
        raise Conflict(
            f"illegal media job transition: {current.value} -> {target.value}"
        )
    return JobTransition(status=target, changed=True)


def advance_production(
    current: ProductionStatus,
    target: ProductionStatus,
) -> ProductionTransition:
    if current is target:
        return ProductionTransition(status=current, changed=False)
    if target not in _PRODUCTION_TRANSITIONS[current]:
        raise Conflict(
            "illegal media production transition: "
            f"{current.value} -> {target.value}"
        )
    return ProductionTransition(status=target, changed=True)
```

Save this as `backend/src/ip_saas/modules/media/state.py`.

- [ ] **Step 4: Run the transition tests**

Run: `cd backend && uv run pytest tests/unit/media/test_state.py -v`

Expected: 4 passed.

- [ ] **Step 5: Commit the transition policy**

```bash
git add backend/src/ip_saas/modules/media/state.py backend/tests/unit/media/test_state.py
git commit -m "feat: enforce replay-safe media states"
```

### Task 3: Persist characters, standard voices, and separate rights grants

**Files:**
- Create: `backend/src/ip_saas/modules/media/models/identity.py`
- Create: `backend/src/ip_saas/modules/media/rights.py`
- Test: `backend/tests/unit/media/test_rights.py`

- [ ] **Step 1: Write failing rights-policy tests**

```python
from datetime import UTC, datetime, timedelta
from types import SimpleNamespace
from uuid import uuid4

import pytest

from ip_saas.common.errors import Forbidden
from ip_saas.modules.media.enums import RightsType, ShotSource, VoiceKind
from ip_saas.modules.media.models.identity import CharacterProfile, MediaRightsGrant, VoiceAsset
from ip_saas.modules.media.rights import RightsPolicy


class FixedClock:
    def now(self) -> datetime:
        return datetime(2026, 8, 24, tzinfo=UTC)


def active_grant(rights_type: RightsType, subject_id) -> MediaRightsGrant:
    now = FixedClock().now()
    return MediaRightsGrant(
        id=uuid4(),
        account_id=uuid4(),
        project_id=uuid4(),
        rights_type=rights_type.value,
        subject_id=subject_id,
        grantor_name="授权人",
        platform_scopes=["douyin", "xiaohongshu"],
        territory="CN",
        valid_from=now - timedelta(days=1),
        valid_until=now + timedelta(days=30),
        commercial_use=True,
        ai_processing_allowed=False,
        provider_transfer_allowed=False,
        guardian_verified=False,
    )


def test_real_person_is_allowed_in_human_package_with_portrait_grant() -> None:
    character = CharacterProfile(
        id=uuid4(),
        account_id=uuid4(),
        project_id=uuid4(),
        name="客户本人",
        description="真人出镜",
        is_real_person=True,
        is_minor=False,
    )
    policy = RightsPolicy(FixedClock())
    policy.require_shot_rights(
        character=character,
        voice=None,
        grants=[active_grant(RightsType.PORTRAIT, character.id)],
        source=ShotSource.HUMAN_CAPTURE,
        platform="douyin",
    )


def test_real_person_ai_generation_is_deferred_to_plan_06() -> None:
    character = SimpleNamespace(id=uuid4(), is_real_person=True, is_minor=False)
    with pytest.raises(Forbidden, match="Plan 06"):
        RightsPolicy(FixedClock()).require_shot_rights(
            character=character,
            voice=None,
            grants=[active_grant(RightsType.PORTRAIT, character.id)],
            source=ShotSource.SEEDANCE,
            platform="douyin",
        )


def test_uploaded_human_recording_requires_an_active_voice_grant() -> None:
    voice = VoiceAsset(
        id=uuid4(),
        account_id=uuid4(),
        project_id=uuid4(),
        name="原声旁白",
        kind=VoiceKind.UPLOADED_RECORDING.value,
        provider_voice_type=None,
        source_asset_id=uuid4(),
        enabled=True,
    )
    with pytest.raises(Forbidden, match="voice grant"):
        RightsPolicy(FixedClock()).require_shot_rights(
            character=None,
            voice=voice,
            grants=[],
            source=ShotSource.UPLOADED_MEDIA,
            platform="douyin",
        )


def test_standard_tts_requires_an_enabled_allowlisted_voice() -> None:
    voice = VoiceAsset(
        id=uuid4(),
        account_id=uuid4(),
        project_id=uuid4(),
        name="标准女声",
        kind=VoiceKind.STANDARD_TTS.value,
        provider_voice_type="zh_female_wanqudashu_moon_bigtts",
        source_asset_id=None,
        enabled=False,
    )
    with pytest.raises(Forbidden, match="disabled"):
        RightsPolicy(FixedClock()).require_shot_rights(
            character=None,
            voice=voice,
            grants=[],
            source=ShotSource.SEEDANCE,
            platform="douyin",
        )
```

- [ ] **Step 2: Run the tests and verify the missing model failure**

Run: `cd backend && uv run pytest tests/unit/media/test_rights.py -v`

Expected: FAIL during collection because `ip_saas.modules.media.models.identity` does not exist.

- [ ] **Step 3: Create the complete identity and grant models**

```python
from datetime import datetime
from uuid import UUID, uuid4

from sqlalchemy import Boolean, CheckConstraint, DateTime, ForeignKey, Index, String
from sqlalchemy.dialects.postgresql import ARRAY, UUID as PGUUID
from sqlalchemy.orm import Mapped, mapped_column

from ip_saas.db.base import Base


class CharacterProfile(Base):
    __tablename__ = "character_profiles"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    account_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("accounts.id"), nullable=False, index=True
    )
    project_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("ip_projects.id"), nullable=False, index=True
    )
    name: Mapped[str] = mapped_column(String(120), nullable=False)
    description: Mapped[str] = mapped_column(String(2_000), nullable=False)
    is_real_person: Mapped[bool] = mapped_column(Boolean, nullable=False, default=False)
    is_minor: Mapped[bool] = mapped_column(Boolean, nullable=False, default=False)
    disabled_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))

    __table_args__ = (
        Index("ix_character_profiles_project_name", "project_id", "name"),
    )


class VoiceAsset(Base):
    __tablename__ = "voice_assets"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    account_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("accounts.id"), nullable=False, index=True
    )
    project_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("ip_projects.id"), nullable=False, index=True
    )
    name: Mapped[str] = mapped_column(String(120), nullable=False)
    kind: Mapped[str] = mapped_column(String(40), nullable=False)
    provider_voice_type: Mapped[str | None] = mapped_column(String(200))
    source_asset_id: Mapped[UUID | None] = mapped_column(PGUUID(as_uuid=True))
    enabled: Mapped[bool] = mapped_column(Boolean, nullable=False, default=True)
    disabled_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))

    __table_args__ = (
        CheckConstraint(
            "(kind = 'standard_tts' AND provider_voice_type IS NOT NULL "
            "AND source_asset_id IS NULL) OR "
            "(kind = 'uploaded_recording' AND provider_voice_type IS NULL "
            "AND source_asset_id IS NOT NULL)",
            name="ck_voice_assets_plan03_kind",
        ),
        Index("ix_voice_assets_project_name", "project_id", "name"),
    )


class MediaRightsGrant(Base):
    __tablename__ = "media_rights_grants"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    account_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("accounts.id"), nullable=False, index=True
    )
    project_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("ip_projects.id"), nullable=False, index=True
    )
    rights_type: Mapped[str] = mapped_column(String(40), nullable=False)
    subject_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    grantor_name: Mapped[str] = mapped_column(String(200), nullable=False)
    platform_scopes: Mapped[list[str]] = mapped_column(ARRAY(String(40)), nullable=False)
    territory: Mapped[str] = mapped_column(String(80), nullable=False)
    valid_from: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    valid_until: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    commercial_use: Mapped[bool] = mapped_column(Boolean, nullable=False)
    ai_processing_allowed: Mapped[bool] = mapped_column(Boolean, nullable=False)
    provider_transfer_allowed: Mapped[bool] = mapped_column(Boolean, nullable=False)
    guardian_verified: Mapped[bool] = mapped_column(Boolean, nullable=False, default=False)
    evidence_asset_id: Mapped[UUID | None] = mapped_column(PGUUID(as_uuid=True))
    revoked_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))

    __table_args__ = (
        CheckConstraint("valid_until > valid_from", name="ck_media_rights_valid_window"),
        Index(
            "ix_media_rights_active_subject",
            "project_id",
            "rights_type",
            "subject_id",
            "valid_until",
        ),
    )
```

Save this as `backend/src/ip_saas/modules/media/models/identity.py`.

- [ ] **Step 4: Implement the Plan 03 rights policy**

```python
from collections.abc import Iterable

from ip_saas.common.clock import Clock
from ip_saas.common.errors import Forbidden

from .enums import RightsType, ShotSource, VoiceKind
from .models.identity import CharacterProfile, MediaRightsGrant, VoiceAsset


class RightsPolicy:
    def __init__(self, clock: Clock) -> None:
        self.clock = clock

    def require_shot_rights(
        self,
        *,
        character: CharacterProfile | None,
        voice: VoiceAsset | None,
        grants: Iterable[MediaRightsGrant],
        source: ShotSource,
        platform: str,
    ) -> None:
        grant_list = list(grants)
        if character is not None:
            if character.disabled_at is not None:
                raise Forbidden("character is disabled")
            if character.is_real_person and source in {
                ShotSource.SEEDREAM,
                ShotSource.SEEDANCE,
            }:
                raise Forbidden(
                    "real-person AI generation is disabled until the Plan 06 gate"
                )
            if character.is_real_person:
                portrait = self._active_grant(
                    grant_list,
                    RightsType.PORTRAIT,
                    character.id,
                    platform,
                )
                if portrait is None:
                    raise Forbidden("active commercial portrait grant is required")
                if character.is_minor and not portrait.guardian_verified:
                    raise Forbidden("minor portrait requires verified guardian authorization")

        if voice is None:
            return
        if not voice.enabled or voice.disabled_at is not None:
            raise Forbidden("voice asset is disabled")
        if voice.kind == VoiceKind.STANDARD_TTS.value:
            if not voice.provider_voice_type:
                raise Forbidden("standard TTS voice is not allow-listed")
            return
        if voice.kind != VoiceKind.UPLOADED_RECORDING.value:
            raise Forbidden("cloned voice is disabled until the Plan 06 gate")
        grant = self._active_grant(
            grant_list,
            RightsType.VOICE,
            voice.id,
            platform,
        )
        if grant is None:
            raise Forbidden("active commercial voice grant is required")

    def _active_grant(
        self,
        grants: list[MediaRightsGrant],
        rights_type: RightsType,
        subject_id: object,
        platform: str,
    ) -> MediaRightsGrant | None:
        now = self.clock.now()
        for grant in grants:
            if (
                grant.rights_type == rights_type.value
                and grant.subject_id == subject_id
                and grant.revoked_at is None
                and grant.valid_from <= now < grant.valid_until
                and grant.commercial_use
                and platform in grant.platform_scopes
            ):
                return grant
        return None
```

Save this as `backend/src/ip_saas/modules/media/rights.py`.

- [ ] **Step 5: Run the rights-policy tests**

Run: `cd backend && uv run pytest tests/unit/media/test_rights.py -v`

Expected: 4 passed.

- [ ] **Step 6: Commit identity and rights policy**

```bash
git add backend/src/ip_saas/modules/media/models/identity.py backend/src/ip_saas/modules/media/rights.py backend/tests/unit/media/test_rights.py
git commit -m "feat: gate media identity and voice rights"
```

### Task 4: Model productions, scenes, immutable shot revisions, and human packages

**Files:**
- Create: `backend/src/ip_saas/modules/media/models/production.py`
- Test: `backend/tests/unit/media/test_production_models.py`

- [ ] **Step 1: Write failing model-shape tests**

```python
from uuid import uuid4

from ip_saas.modules.media.enums import ProductionMode, ProductionStatus, ShotSource
from ip_saas.modules.media.models.production import MediaProduction, Shot


def test_media_production_references_but_does_not_duplicate_the_creative_master() -> None:
    columns = set(MediaProduction.__table__.columns.keys())
    assert {"content_version_id", "platform_variant_id"} <= columns
    assert {
        "core_question",
        "script",
        "audience_change",
        "business_connection",
    }.isdisjoint(columns)


def test_a_shot_revision_keeps_a_stable_logical_identity() -> None:
    logical_id = uuid4()
    first_id = uuid4()
    replacement = Shot(
        id=uuid4(),
        account_id=uuid4(),
        project_id=uuid4(),
        production_id=uuid4(),
        scene_id=uuid4(),
        logical_shot_id=logical_id,
        revision=2,
        supersedes_id=first_id,
        is_current=True,
        sequence=1,
        title="修正手部",
        source=ShotSource.SEEDANCE.value,
        duration_ms=5000,
        prompt="手持礼盒，五指自然可见",
        spoken_text=None,
    )
    assert replacement.logical_shot_id == logical_id
    assert replacement.supersedes_id == first_id
    assert replacement.revision == 2


def test_production_defaults_to_draft_without_copying_master_text() -> None:
    production = MediaProduction(
        id=uuid4(),
        account_id=uuid4(),
        project_id=uuid4(),
        content_version_id=uuid4(),
        platform_variant_id=uuid4(),
        mode=ProductionMode.MIXED.value,
        output_aspect_ratio="9:16",
        idempotency_key="production:create:123",
        initiated_by_actor_id=uuid4(),
    )
    assert production.status is None or production.status == ProductionStatus.DRAFT.value
```

- [ ] **Step 2: Run the tests and verify the missing production model failure**

Run: `cd backend && uv run pytest tests/unit/media/test_production_models.py -v`

Expected: FAIL during collection because `ip_saas.modules.media.models.production` does not exist.

- [ ] **Step 3: Create the complete production model file**

```python
from datetime import datetime
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import (
    Boolean,
    CheckConstraint,
    DateTime,
    ForeignKey,
    Index,
    Integer,
    String,
    Text,
    UniqueConstraint,
    func,
)
from sqlalchemy.dialects.postgresql import JSONB, UUID as PGUUID
from sqlalchemy.orm import Mapped, mapped_column

from ip_saas.db.base import Base

from ..enums import ProductionStatus


class MediaProduction(Base):
    __tablename__ = "media_productions"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    account_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("accounts.id"), nullable=False, index=True
    )
    project_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("ip_projects.id"), nullable=False, index=True
    )
    content_version_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("content_versions.id"), nullable=False
    )
    platform_variant_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("platform_variants.id"), nullable=False
    )
    mode: Mapped[str] = mapped_column(String(20), nullable=False)
    output_aspect_ratio: Mapped[str] = mapped_column(String(10), nullable=False)
    status: Mapped[str] = mapped_column(
        String(40), nullable=False, default=ProductionStatus.DRAFT.value
    )
    idempotency_key: Mapped[str] = mapped_column(String(200), nullable=False)
    initiated_by_actor_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    storyboard_approved_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    low_res_approved_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    final_asset_id: Mapped[UUID | None] = mapped_column(PGUUID(as_uuid=True))
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now(), onupdate=func.now()
    )

    __table_args__ = (
        UniqueConstraint(
            "project_id", "idempotency_key",
            name="uq_media_production_project_idempotency",
        ),
        CheckConstraint("mode IN ('human', 'ai', 'mixed')", name="ck_media_production_mode"),
        CheckConstraint(
            "output_aspect_ratio IN ('9:16', '3:4', '1:1', '16:9')",
            name="ck_media_production_aspect_ratio",
        ),
        Index(
            "ix_media_productions_lineage",
            "project_id",
            "content_version_id",
            "platform_variant_id",
        ),
    )


class Scene(Base):
    __tablename__ = "media_scenes"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    account_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False, index=True)
    project_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False, index=True)
    production_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_productions.id"), nullable=False
    )
    sequence: Mapped[int] = mapped_column(Integer, nullable=False)
    title: Mapped[str] = mapped_column(String(120), nullable=False)
    purpose: Mapped[str] = mapped_column(String(500), nullable=False)
    location: Mapped[str] = mapped_column(String(200), nullable=False)

    __table_args__ = (
        UniqueConstraint("production_id", "sequence", name="uq_media_scene_sequence"),
        CheckConstraint("sequence > 0", name="ck_media_scene_sequence_positive"),
    )


class Shot(Base):
    __tablename__ = "media_shots"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    account_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False, index=True)
    project_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False, index=True)
    production_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_productions.id"), nullable=False
    )
    scene_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_scenes.id"), nullable=False
    )
    logical_shot_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    revision: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    supersedes_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_shots.id")
    )
    is_current: Mapped[bool] = mapped_column(Boolean, nullable=False, default=True)
    sequence: Mapped[int] = mapped_column(Integer, nullable=False)
    title: Mapped[str] = mapped_column(String(120), nullable=False)
    source: Mapped[str] = mapped_column(String(40), nullable=False)
    duration_ms: Mapped[int] = mapped_column(Integer, nullable=False)
    prompt: Mapped[str] = mapped_column(Text, nullable=False)
    spoken_text: Mapped[str | None] = mapped_column(Text)
    character_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("character_profiles.id")
    )
    voice_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("voice_assets.id")
    )
    product_reference_asset_id: Mapped[UUID | None] = mapped_column(PGUUID(as_uuid=True))
    selected_asset_id: Mapped[UUID | None] = mapped_column(PGUUID(as_uuid=True))
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )

    __table_args__ = (
        UniqueConstraint(
            "production_id",
            "logical_shot_id",
            "revision",
            name="uq_media_shot_revision",
        ),
        Index(
            "uq_media_shot_current",
            "production_id",
            "logical_shot_id",
            unique=True,
            postgresql_where=(is_current.is_(True)),
        ),
        CheckConstraint("revision > 0", name="ck_media_shot_revision_positive"),
        CheckConstraint(
            "duration_ms BETWEEN 500 AND 30000",
            name="ck_media_shot_duration",
        ),
    )


class HumanProductionPackage(Base):
    __tablename__ = "human_production_packages"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    account_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False, index=True)
    project_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False, index=True)
    production_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_productions.id"), nullable=False
    )
    revision: Mapped[int] = mapped_column(Integer, nullable=False, default=1)
    call_sheet: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False)
    shot_list: Mapped[list[dict[str, Any]]] = mapped_column(JSONB, nullable=False)
    performance_notes: Mapped[list[dict[str, Any]]] = mapped_column(JSONB, nullable=False)
    sound_notes: Mapped[list[dict[str, Any]]] = mapped_column(JSONB, nullable=False)
    props_and_wardrobe: Mapped[list[dict[str, Any]]] = mapped_column(JSONB, nullable=False)
    low_cost_alternatives: Mapped[list[dict[str, Any]]] = mapped_column(JSONB, nullable=False)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )

    __table_args__ = (
        UniqueConstraint("production_id", "revision", name="uq_human_package_revision"),
        CheckConstraint("revision > 0", name="ck_human_package_revision_positive"),
    )
```

Save this as `backend/src/ip_saas/modules/media/models/production.py`.

- [ ] **Step 4: Run the model-shape tests**

Run: `cd backend && uv run pytest tests/unit/media/test_production_models.py -v`

Expected: 3 passed.

- [ ] **Step 5: Commit production persistence models**

```bash
git add backend/src/ip_saas/modules/media/models/production.py backend/tests/unit/media/test_production_models.py
git commit -m "feat: model unified media productions and shots"
```

### Task 5: Model jobs, private assets, cost evidence, and QC reports

**Files:**
- Create: `backend/src/ip_saas/modules/media/models/jobs.py`
- Create: `backend/src/ip_saas/modules/media/models/__init__.py`
- Test: `backend/tests/unit/media/test_job_models.py`

- [ ] **Step 1: Write failing tests for shared-task ownership and URL-storage shape**

```python
from ip_saas.modules.media.models.jobs import GenerationJob, MediaAsset


def test_job_references_one_shared_task_and_does_not_duplicate_hold_state() -> None:
    columns = set(GenerationJob.__table__.columns.keys())
    assert "task_record_id" in columns
    assert "request_fingerprint" in columns
    assert "provider_request_id" in columns
    assert {"provider_task_id", "provider_request_id"} <= columns
    assert GenerationJob.__table__.columns.provider_task_id.type.length == 300
    assert GenerationJob.__table__.columns.provider_request_id.type.length == 200
    assert {"finishing_status", "finishing_attempt_count", "finished_asset_id"} <= columns
    assert "generation_hold_id" not in columns
    assert "internal_budget_hold_id" not in columns
    assert "settlement_status" not in columns


def test_provider_url_is_encrypted_and_never_part_of_a_deliverable_asset() -> None:
    job_columns = set(GenerationJob.__table__.columns.keys())
    asset_columns = set(MediaAsset.__table__.columns.keys())
    assert "temporary_url_ciphertext" in job_columns
    assert "temporary_url" not in job_columns
    assert "temporary_url" not in asset_columns
    assert {"tos_bucket", "tos_object_key", "sha256"} <= asset_columns


def test_non_billable_jobs_are_limited_to_local_capabilities() -> None:
    constraint = next(
        constraint
        for constraint in GenerationJob.__table__.constraints
        if constraint.name == "ck_generation_job_non_billable_capability"
    )
    sql = str(constraint.sqltext)
    assert "human_package" in sql
    assert "finish" in sql
    assert "qc" in sql
```

- [ ] **Step 2: Run the tests and verify the missing jobs model failure**

Run: `cd backend && uv run pytest tests/unit/media/test_job_models.py -v`

Expected: FAIL during collection because `ip_saas.modules.media.models.jobs` does not exist.

- [ ] **Step 3: Create the complete job, asset, and QC models**

```python
from datetime import datetime
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import (
    Boolean,
    CheckConstraint,
    DateTime,
    ForeignKey,
    Index,
    Integer,
    LargeBinary,
    Numeric,
    String,
    UniqueConstraint,
    func,
)
from sqlalchemy.dialects.postgresql import JSONB, UUID as PGUUID
from sqlalchemy.orm import Mapped, mapped_column

from ip_saas.db.base import Base

from ..enums import JobStatus


class GenerationJob(Base):
    __tablename__ = "generation_jobs"

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
    production_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_productions.id"), nullable=False
    )
    shot_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_shots.id")
    )
    capability: Mapped[str] = mapped_column(String(60), nullable=False)
    provider: Mapped[str] = mapped_column(String(60), nullable=False)
    model_id: Mapped[str] = mapped_column(String(200), nullable=False)
    model_version: Mapped[str] = mapped_column(String(100), nullable=False)
    billable: Mapped[bool] = mapped_column(Boolean, nullable=False)
    provider_status: Mapped[str] = mapped_column(
        String(40), nullable=False, default=JobStatus.DRAFT.value
    )
    idempotency_key: Mapped[str] = mapped_column(String(240), nullable=False)
    request_fingerprint: Mapped[str] = mapped_column(String(64), nullable=False)
    request_payload: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False)
    initiated_by_actor_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    provider_task_id: Mapped[str | None] = mapped_column(String(300))
    provider_request_id: Mapped[str | None] = mapped_column(String(200))
    provider_phase_rank: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    temporary_url_ciphertext: Mapped[bytes | None] = mapped_column(LargeBinary)
    temporary_url_expires_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    provider_payload_sha256: Mapped[str | None] = mapped_column(String(64))
    native_quantity: Mapped[int | None] = mapped_column(Integer)
    native_unit: Mapped[str | None] = mapped_column(String(60))
    supplier_amount_minor: Mapped[int | None] = mapped_column(Integer)
    supplier_currency: Mapped[str | None] = mapped_column(String(3))
    provider_cost_amount_fen: Mapped[int | None] = mapped_column(Integer)
    reconciliation_status: Mapped[str | None] = mapped_column(String(40))
    attempt_count: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    poll_count: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    copy_attempt_count: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    finishing_status: Mapped[str] = mapped_column(
        String(24), nullable=False, default="not_required"
    )
    finishing_attempt_count: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    finished_asset_id: Mapped[UUID | None] = mapped_column(PGUUID(as_uuid=True))
    next_attempt_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    last_error_code: Mapped[str | None] = mapped_column(String(100))
    last_error_message: Mapped[str | None] = mapped_column(String(500))
    replaces_job_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("generation_jobs.id")
    )
    redo_origin: Mapped[str | None] = mapped_column(String(20))
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now(), onupdate=func.now()
    )

    __table_args__ = (
        UniqueConstraint(
            "project_id", "idempotency_key",
            name="uq_generation_job_project_idempotency",
        ),
        CheckConstraint(
            "billable OR capability IN ('human_package', 'finish', 'qc')",
            name="ck_generation_job_non_billable_capability",
        ),
        CheckConstraint(
            "attempt_count >= 0 AND poll_count >= 0 AND copy_attempt_count >= 0 "
            "AND finishing_attempt_count >= 0",
            name="ck_generation_job_attempt_counts",
        ),
        CheckConstraint(
            "length(request_fingerprint) = 64",
            name="ck_generation_job_request_fingerprint",
        ),
        CheckConstraint(
            "provider_request_id IS NULL OR "
            "(provider_request_id = btrim(provider_request_id) AND "
            "lower(provider_request_id) NOT IN ('none', 'missing') AND "
            "length(provider_request_id) BETWEEN 1 AND 200)",
            name="ck_generation_job_provider_request_id",
        ),
        Index(
            "uq_generation_job_provider_task",
            "provider",
            "provider_task_id",
            unique=True,
            postgresql_where=(provider_task_id.is_not(None)),
        ),
        Index(
            "uq_generation_job_provider_request",
            "provider",
            "provider_request_id",
            unique=True,
            postgresql_where=(provider_request_id.is_not(None)),
        ),
        Index("ix_generation_job_due", "provider_status", "next_attempt_at"),
    )


class MediaAsset(Base):
    __tablename__ = "media_assets"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    account_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("accounts.id"), nullable=False, index=True
    )
    project_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("ip_projects.id"), nullable=False, index=True
    )
    production_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_productions.id")
    )
    shot_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_shots.id")
    )
    job_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("generation_jobs.id")
    )
    kind: Mapped[str] = mapped_column(String(60), nullable=False)
    tos_bucket: Mapped[str] = mapped_column(String(120), nullable=False)
    tos_object_key: Mapped[str] = mapped_column(String(1_024), nullable=False)
    tos_version_id: Mapped[str | None] = mapped_column(String(300))
    sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    content_type: Mapped[str] = mapped_column(String(120), nullable=False)
    size_bytes: Mapped[int] = mapped_column(Numeric(20, 0), nullable=False)
    width: Mapped[int | None] = mapped_column(Integer)
    height: Mapped[int | None] = mapped_column(Integer)
    duration_ms: Mapped[int | None] = mapped_column(Integer)
    ai_disclosure: Mapped[str] = mapped_column(String(30), nullable=False)
    provenance: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False)
    label_evidence: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), nullable=False, server_default=func.now()
    )

    __table_args__ = (
        UniqueConstraint("tos_bucket", "tos_object_key", name="uq_media_asset_tos_key"),
        CheckConstraint("size_bytes > 0", name="ck_media_asset_size_positive"),
        CheckConstraint("length(sha256) = 64", name="ck_media_asset_sha256"),
        Index("ix_media_asset_production_kind", "production_id", "kind"),
    )


class QCReport(Base):
    __tablename__ = "media_qc_reports"

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    account_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False, index=True)
    project_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False, index=True)
    production_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_productions.id"), nullable=False
    )
    job_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("generation_jobs.id"), nullable=False
    )
    asset_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("media_assets.id"), nullable=False
    )
    decision: Mapped[str] = mapped_column(String(20), nullable=False)
    technical_checks: Mapped[list[dict[str, Any]]] = mapped_column(JSONB, nullable=False)
    semantic_checks: Mapped[list[dict[str, Any]]] = mapped_column(JSONB, nullable=False)
    rights_checks: Mapped[list[dict[str, Any]]] = mapped_column(JSONB, nullable=False)
    disclosure_checks: Mapped[list[dict[str, Any]]] = mapped_column(JSONB, nullable=False)
    product_truth_checks: Mapped[list[dict[str, Any]]] = mapped_column(JSONB, nullable=False)
    continuity_checks: Mapped[list[dict[str, Any]]] = mapped_column(JSONB, nullable=False)
    redo_logical_shot_ids: Mapped[list[str]] = mapped_column(JSONB, nullable=False)
    checked_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    human_decision: Mapped[str | None] = mapped_column(String(20))
    human_review_note: Mapped[str | None] = mapped_column(String(2_000))
    human_reviewed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    human_reviewed_by: Mapped[UUID | None] = mapped_column(PGUUID(as_uuid=True))

    __table_args__ = (
        UniqueConstraint("job_id", "asset_id", name="uq_media_qc_job_asset"),
        CheckConstraint("decision IN ('pass', 'redo', 'fail')", name="ck_media_qc_decision"),
        CheckConstraint(
            "human_decision IS NULL OR human_decision IN ('pass', 'redo', 'fail')",
            name="ck_media_qc_human_decision",
        ),
    )
```

Save this as `backend/src/ip_saas/modules/media/models/jobs.py`.

- [ ] **Step 4: Export every mapped class so Alembic metadata sees it**

```python
from .identity import CharacterProfile, MediaRightsGrant, VoiceAsset
from .jobs import GenerationJob, MediaAsset, QCReport
from .production import HumanProductionPackage, MediaProduction, Scene, Shot

__all__ = [
    "CharacterProfile",
    "GenerationJob",
    "HumanProductionPackage",
    "MediaAsset",
    "MediaProduction",
    "MediaRightsGrant",
    "QCReport",
    "Scene",
    "Shot",
    "VoiceAsset",
]
```

Save this as `backend/src/ip_saas/modules/media/models/__init__.py`.

- [ ] **Step 5: Run the model-shape tests**

Run: `cd backend && uv run pytest tests/unit/media/test_job_models.py -v`

Expected: 3 passed.

- [ ] **Step 6: Commit jobs, assets, and QC persistence**

```bash
git add backend/src/ip_saas/modules/media/models backend/tests/unit/media/test_job_models.py
git commit -m "feat: persist media jobs assets and qc evidence"
```

### Task 6: Add the fixed `0003_media` migration and database invariants

**Files:**
- Create: `backend/migrations/versions/0003_media.py`
- Test: `backend/tests/integration/media/test_media_migration.py`

- [ ] **Step 1: Write the failing migration inspection test**

```python
from sqlalchemy import inspect
from sqlalchemy.engine import Engine


def test_media_migration_has_required_constraints(pg_engine: Engine) -> None:
    inspector = inspect(pg_engine)
    tables = set(inspector.get_table_names())
    assert {
        "media_productions",
        "media_scenes",
        "media_shots",
        "generation_jobs",
        "media_assets",
        "media_qc_reports",
    } <= tables
    column_rows = {
        row["name"]: row for row in inspector.get_columns("generation_jobs")
    }
    columns = set(column_rows)
    assert "task_record_id" in columns
    assert "request_fingerprint" in columns
    assert "provider_request_id" in columns
    assert column_rows["provider_task_id"]["type"].length == 300
    assert column_rows["provider_request_id"]["type"].length == 200
    assert "generation_hold_id" not in columns
    assert "internal_budget_hold_id" not in columns
    indexes = {
        row["name"]: row
        for row in inspector.get_indexes("generation_jobs")
    }
    assert indexes["uq_generation_job_provider_task"]["unique"] is True
    assert indexes["uq_generation_job_provider_request"]["unique"] is True
    assert "ck_generation_job_provider_request_id" in {
        row["name"] for row in inspector.get_check_constraints("generation_jobs")
    }
    job_uniques = {
        row["name"]: row["column_names"]
        for row in inspector.get_unique_constraints("generation_jobs")
    }
    production_uniques = {
        row["name"]: row["column_names"]
        for row in inspector.get_unique_constraints("media_productions")
    }
    assert job_uniques["uq_generation_job_project_idempotency"] == [
        "project_id", "idempotency_key",
    ]
    assert production_uniques["uq_media_production_project_idempotency"] == [
        "project_id", "idempotency_key",
    ]
    qc_columns = {row["name"] for row in inspector.get_columns("media_qc_reports")}
    assert {
        "product_truth_checks", "continuity_checks", "human_decision",
        "human_review_note", "human_reviewed_at", "human_reviewed_by",
    } <= qc_columns
```

- [ ] **Step 2: Run the test and verify that the migration is absent**

Run: `cd backend && uv run alembic upgrade 0002_intelligence && uv run pytest tests/integration/media/test_media_migration.py -v`

Expected: FAIL because `media_productions` and `generation_jobs` are absent.

- [ ] **Step 3: Create the complete fixed migration**

```python
from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects import postgresql

revision: str = "0003_media"
down_revision: str | None = "0002_intelligence"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def upgrade() -> None:
    op.create_table(
        "media_productions",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("account_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("accounts.id"), nullable=False),
        sa.Column("project_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("ip_projects.id"), nullable=False),
        sa.Column("content_version_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("content_versions.id"), nullable=False),
        sa.Column("platform_variant_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("platform_variants.id"), nullable=False),
        sa.Column("mode", sa.String(20), nullable=False),
        sa.Column("output_aspect_ratio", sa.String(10), nullable=False),
        sa.Column("status", sa.String(40), nullable=False),
        sa.Column("idempotency_key", sa.String(200), nullable=False),
        sa.Column("initiated_by_actor_id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("storyboard_approved_at", sa.DateTime(timezone=True)),
        sa.Column("low_res_approved_at", sa.DateTime(timezone=True)),
        sa.Column("final_asset_id", postgresql.UUID(as_uuid=True)),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column("updated_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.CheckConstraint("mode IN ('human', 'ai', 'mixed')", name="ck_media_production_mode"),
        sa.CheckConstraint(
            "output_aspect_ratio IN ('9:16', '3:4', '1:1', '16:9')",
            name="ck_media_production_aspect_ratio",
        ),
        sa.UniqueConstraint(
            "project_id", "idempotency_key",
            name="uq_media_production_project_idempotency",
        ),
    )
    op.create_index("ix_media_productions_account_id", "media_productions", ["account_id"])
    op.create_index("ix_media_productions_project_id", "media_productions", ["project_id"])
    op.create_index(
        "ix_media_productions_lineage",
        "media_productions",
        ["project_id", "content_version_id", "platform_variant_id"],
    )

    op.create_table(
        "character_profiles",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("account_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("accounts.id"), nullable=False),
        sa.Column("project_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("ip_projects.id"), nullable=False),
        sa.Column("name", sa.String(120), nullable=False),
        sa.Column("description", sa.String(2_000), nullable=False),
        sa.Column("is_real_person", sa.Boolean(), nullable=False, server_default=sa.false()),
        sa.Column("is_minor", sa.Boolean(), nullable=False, server_default=sa.false()),
        sa.Column("disabled_at", sa.DateTime(timezone=True)),
    )
    op.create_index("ix_character_profiles_account_id", "character_profiles", ["account_id"])
    op.create_index("ix_character_profiles_project_id", "character_profiles", ["project_id"])
    op.create_index("ix_character_profiles_project_name", "character_profiles", ["project_id", "name"])

    op.create_table(
        "voice_assets",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("account_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("accounts.id"), nullable=False),
        sa.Column("project_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("ip_projects.id"), nullable=False),
        sa.Column("name", sa.String(120), nullable=False),
        sa.Column("kind", sa.String(40), nullable=False),
        sa.Column("provider_voice_type", sa.String(200)),
        sa.Column("source_asset_id", postgresql.UUID(as_uuid=True)),
        sa.Column("enabled", sa.Boolean(), nullable=False, server_default=sa.true()),
        sa.Column("disabled_at", sa.DateTime(timezone=True)),
        sa.CheckConstraint(
            "(kind = 'standard_tts' AND provider_voice_type IS NOT NULL AND source_asset_id IS NULL) OR "
            "(kind = 'uploaded_recording' AND provider_voice_type IS NULL AND source_asset_id IS NOT NULL)",
            name="ck_voice_assets_plan03_kind",
        ),
    )
    op.create_index("ix_voice_assets_account_id", "voice_assets", ["account_id"])
    op.create_index("ix_voice_assets_project_id", "voice_assets", ["project_id"])
    op.create_index("ix_voice_assets_project_name", "voice_assets", ["project_id", "name"])

    op.create_table(
        "media_rights_grants",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("account_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("accounts.id"), nullable=False),
        sa.Column("project_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("ip_projects.id"), nullable=False),
        sa.Column("rights_type", sa.String(40), nullable=False),
        sa.Column("subject_id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("grantor_name", sa.String(200), nullable=False),
        sa.Column("platform_scopes", postgresql.ARRAY(sa.String(40)), nullable=False),
        sa.Column("territory", sa.String(80), nullable=False),
        sa.Column("valid_from", sa.DateTime(timezone=True), nullable=False),
        sa.Column("valid_until", sa.DateTime(timezone=True), nullable=False),
        sa.Column("commercial_use", sa.Boolean(), nullable=False),
        sa.Column("ai_processing_allowed", sa.Boolean(), nullable=False),
        sa.Column("provider_transfer_allowed", sa.Boolean(), nullable=False),
        sa.Column("guardian_verified", sa.Boolean(), nullable=False, server_default=sa.false()),
        sa.Column("evidence_asset_id", postgresql.UUID(as_uuid=True)),
        sa.Column("revoked_at", sa.DateTime(timezone=True)),
        sa.CheckConstraint("valid_until > valid_from", name="ck_media_rights_valid_window"),
    )
    op.create_index("ix_media_rights_grants_account_id", "media_rights_grants", ["account_id"])
    op.create_index("ix_media_rights_grants_project_id", "media_rights_grants", ["project_id"])
    op.create_index(
        "ix_media_rights_active_subject",
        "media_rights_grants",
        ["project_id", "rights_type", "subject_id", "valid_until"],
    )

    op.create_table(
        "media_scenes",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("account_id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("project_id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("production_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("media_productions.id"), nullable=False),
        sa.Column("sequence", sa.Integer(), nullable=False),
        sa.Column("title", sa.String(120), nullable=False),
        sa.Column("purpose", sa.String(500), nullable=False),
        sa.Column("location", sa.String(200), nullable=False),
        sa.UniqueConstraint("production_id", "sequence", name="uq_media_scene_sequence"),
        sa.CheckConstraint("sequence > 0", name="ck_media_scene_sequence_positive"),
    )
    op.create_index("ix_media_scenes_account_id", "media_scenes", ["account_id"])
    op.create_index("ix_media_scenes_project_id", "media_scenes", ["project_id"])

    op.create_table(
        "media_shots",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("account_id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("project_id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("production_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("media_productions.id"), nullable=False),
        sa.Column("scene_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("media_scenes.id"), nullable=False),
        sa.Column("logical_shot_id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("revision", sa.Integer(), nullable=False),
        sa.Column("supersedes_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("media_shots.id")),
        sa.Column("is_current", sa.Boolean(), nullable=False, server_default=sa.true()),
        sa.Column("sequence", sa.Integer(), nullable=False),
        sa.Column("title", sa.String(120), nullable=False),
        sa.Column("source", sa.String(40), nullable=False),
        sa.Column("duration_ms", sa.Integer(), nullable=False),
        sa.Column("prompt", sa.Text(), nullable=False),
        sa.Column("spoken_text", sa.Text()),
        sa.Column("character_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("character_profiles.id")),
        sa.Column("voice_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("voice_assets.id")),
        sa.Column("product_reference_asset_id", postgresql.UUID(as_uuid=True)),
        sa.Column("selected_asset_id", postgresql.UUID(as_uuid=True)),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.UniqueConstraint("production_id", "logical_shot_id", "revision", name="uq_media_shot_revision"),
        sa.CheckConstraint("revision > 0", name="ck_media_shot_revision_positive"),
        sa.CheckConstraint("duration_ms BETWEEN 500 AND 30000", name="ck_media_shot_duration"),
    )
    op.create_index("ix_media_shots_account_id", "media_shots", ["account_id"])
    op.create_index("ix_media_shots_project_id", "media_shots", ["project_id"])
    op.create_index(
        "uq_media_shot_current",
        "media_shots",
        ["production_id", "logical_shot_id"],
        unique=True,
        postgresql_where=sa.text("is_current"),
    )

    op.create_table(
        "human_production_packages",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("account_id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("project_id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("production_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("media_productions.id"), nullable=False),
        sa.Column("revision", sa.Integer(), nullable=False),
        sa.Column("call_sheet", postgresql.JSONB(), nullable=False),
        sa.Column("shot_list", postgresql.JSONB(), nullable=False),
        sa.Column("performance_notes", postgresql.JSONB(), nullable=False),
        sa.Column("sound_notes", postgresql.JSONB(), nullable=False),
        sa.Column("props_and_wardrobe", postgresql.JSONB(), nullable=False),
        sa.Column("low_cost_alternatives", postgresql.JSONB(), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.UniqueConstraint("production_id", "revision", name="uq_human_package_revision"),
        sa.CheckConstraint("revision > 0", name="ck_human_package_revision_positive"),
    )
    op.create_index("ix_human_production_packages_account_id", "human_production_packages", ["account_id"])
    op.create_index("ix_human_production_packages_project_id", "human_production_packages", ["project_id"])

    op.create_table(
        "generation_jobs",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("task_record_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("task_records.id"), nullable=False, unique=True),
        sa.Column("account_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("accounts.id"), nullable=False),
        sa.Column("project_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("ip_projects.id"), nullable=False),
        sa.Column("production_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("media_productions.id"), nullable=False),
        sa.Column("shot_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("media_shots.id")),
        sa.Column("capability", sa.String(60), nullable=False),
        sa.Column("provider", sa.String(60), nullable=False),
        sa.Column("model_id", sa.String(200), nullable=False),
        sa.Column("model_version", sa.String(100), nullable=False),
        sa.Column("billable", sa.Boolean(), nullable=False),
        sa.Column("provider_status", sa.String(40), nullable=False),
        sa.Column("idempotency_key", sa.String(240), nullable=False),
        sa.Column("request_fingerprint", sa.String(64), nullable=False),
        sa.Column("request_payload", postgresql.JSONB(), nullable=False),
        sa.Column("initiated_by_actor_id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("provider_task_id", sa.String(300)),
        sa.Column("provider_request_id", sa.String(200)),
        sa.Column("provider_phase_rank", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("temporary_url_ciphertext", sa.LargeBinary()),
        sa.Column("temporary_url_expires_at", sa.DateTime(timezone=True)),
        sa.Column("provider_payload_sha256", sa.String(64)),
        sa.Column("native_quantity", sa.Integer()),
        sa.Column("native_unit", sa.String(60)),
        sa.Column("supplier_amount_minor", sa.Integer()),
        sa.Column("supplier_currency", sa.String(3)),
        sa.Column("provider_cost_amount_fen", sa.Integer()),
        sa.Column("reconciliation_status", sa.String(40)),
        sa.Column("attempt_count", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("poll_count", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("copy_attempt_count", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("finishing_status", sa.String(24), nullable=False, server_default="not_required"),
        sa.Column("finishing_attempt_count", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("finished_asset_id", postgresql.UUID(as_uuid=True)),
        sa.Column("next_attempt_at", sa.DateTime(timezone=True)),
        sa.Column("last_error_code", sa.String(100)),
        sa.Column("last_error_message", sa.String(500)),
        sa.Column("replaces_job_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("generation_jobs.id")),
        sa.Column("redo_origin", sa.String(20)),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.Column("updated_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.CheckConstraint(
            "billable OR capability IN ('human_package', 'finish', 'qc')",
            name="ck_generation_job_non_billable_capability",
        ),
        sa.CheckConstraint(
            "attempt_count >= 0 AND poll_count >= 0 AND copy_attempt_count >= 0 "
            "AND finishing_attempt_count >= 0",
            name="ck_generation_job_attempt_counts",
        ),
        sa.CheckConstraint(
            "length(request_fingerprint) = 64",
            name="ck_generation_job_request_fingerprint",
        ),
        sa.CheckConstraint(
            "provider_request_id IS NULL OR "
            "(provider_request_id = btrim(provider_request_id) AND "
            "lower(provider_request_id) NOT IN ('none', 'missing') AND "
            "length(provider_request_id) BETWEEN 1 AND 200)",
            name="ck_generation_job_provider_request_id",
        ),
        sa.UniqueConstraint(
            "project_id", "idempotency_key",
            name="uq_generation_job_project_idempotency",
        ),
    )
    op.create_index("ix_generation_jobs_account_id", "generation_jobs", ["account_id"])
    op.create_index("ix_generation_jobs_project_id", "generation_jobs", ["project_id"])
    op.create_index("ix_generation_job_due", "generation_jobs", ["provider_status", "next_attempt_at"])
    op.create_index(
        "uq_generation_job_provider_task",
        "generation_jobs",
        ["provider", "provider_task_id"],
        unique=True,
        postgresql_where=sa.text("provider_task_id IS NOT NULL"),
    )
    op.create_index(
        "uq_generation_job_provider_request",
        "generation_jobs",
        ["provider", "provider_request_id"],
        unique=True,
        postgresql_where=sa.text("provider_request_id IS NOT NULL"),
    )

    op.create_table(
        "media_assets",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("account_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("accounts.id"), nullable=False),
        sa.Column("project_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("ip_projects.id"), nullable=False),
        sa.Column("production_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("media_productions.id")),
        sa.Column("shot_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("media_shots.id")),
        sa.Column("job_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("generation_jobs.id")),
        sa.Column("kind", sa.String(60), nullable=False),
        sa.Column("tos_bucket", sa.String(120), nullable=False),
        sa.Column("tos_object_key", sa.String(1_024), nullable=False),
        sa.Column("tos_version_id", sa.String(300)),
        sa.Column("sha256", sa.String(64), nullable=False),
        sa.Column("content_type", sa.String(120), nullable=False),
        sa.Column("size_bytes", sa.Numeric(20, 0), nullable=False),
        sa.Column("width", sa.Integer()),
        sa.Column("height", sa.Integer()),
        sa.Column("duration_ms", sa.Integer()),
        sa.Column("ai_disclosure", sa.String(30), nullable=False),
        sa.Column("provenance", postgresql.JSONB(), nullable=False),
        sa.Column("label_evidence", postgresql.JSONB(), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
        sa.UniqueConstraint("tos_bucket", "tos_object_key", name="uq_media_asset_tos_key"),
        sa.CheckConstraint("size_bytes > 0", name="ck_media_asset_size_positive"),
        sa.CheckConstraint("length(sha256) = 64", name="ck_media_asset_sha256"),
    )
    op.create_index("ix_media_assets_account_id", "media_assets", ["account_id"])
    op.create_index("ix_media_assets_project_id", "media_assets", ["project_id"])
    op.create_index("ix_media_asset_production_kind", "media_assets", ["production_id", "kind"])

    op.create_table(
        "media_qc_reports",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("account_id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("project_id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("production_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("media_productions.id"), nullable=False),
        sa.Column("job_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("generation_jobs.id"), nullable=False),
        sa.Column("asset_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("media_assets.id"), nullable=False),
        sa.Column("decision", sa.String(20), nullable=False),
        sa.Column("technical_checks", postgresql.JSONB(), nullable=False),
        sa.Column("semantic_checks", postgresql.JSONB(), nullable=False),
        sa.Column("rights_checks", postgresql.JSONB(), nullable=False),
        sa.Column("disclosure_checks", postgresql.JSONB(), nullable=False),
        sa.Column("product_truth_checks", postgresql.JSONB(), nullable=False),
        sa.Column("continuity_checks", postgresql.JSONB(), nullable=False),
        sa.Column("redo_logical_shot_ids", postgresql.JSONB(), nullable=False),
        sa.Column("checked_at", sa.DateTime(timezone=True), nullable=False),
        sa.Column("human_decision", sa.String(20)),
        sa.Column("human_review_note", sa.String(2_000)),
        sa.Column("human_reviewed_at", sa.DateTime(timezone=True)),
        sa.Column("human_reviewed_by", postgresql.UUID(as_uuid=True)),
        sa.UniqueConstraint("job_id", "asset_id", name="uq_media_qc_job_asset"),
        sa.CheckConstraint("decision IN ('pass', 'redo', 'fail')", name="ck_media_qc_decision"),
        sa.CheckConstraint(
            "human_decision IS NULL OR human_decision IN ('pass', 'redo', 'fail')",
            name="ck_media_qc_human_decision",
        ),
    )
    op.create_index("ix_media_qc_reports_account_id", "media_qc_reports", ["account_id"])
    op.create_index("ix_media_qc_reports_project_id", "media_qc_reports", ["project_id"])


def downgrade() -> None:
    op.drop_index("ix_media_qc_reports_project_id", table_name="media_qc_reports")
    op.drop_index("ix_media_qc_reports_account_id", table_name="media_qc_reports")
    op.drop_table("media_qc_reports")
    op.drop_index("ix_media_asset_production_kind", table_name="media_assets")
    op.drop_index("ix_media_assets_project_id", table_name="media_assets")
    op.drop_index("ix_media_assets_account_id", table_name="media_assets")
    op.drop_table("media_assets")
    op.drop_index("uq_generation_job_provider_request", table_name="generation_jobs")
    op.drop_index("uq_generation_job_provider_task", table_name="generation_jobs")
    op.drop_index("ix_generation_job_due", table_name="generation_jobs")
    op.drop_index("ix_generation_jobs_project_id", table_name="generation_jobs")
    op.drop_index("ix_generation_jobs_account_id", table_name="generation_jobs")
    op.drop_table("generation_jobs")
    op.drop_index("ix_human_production_packages_project_id", table_name="human_production_packages")
    op.drop_index("ix_human_production_packages_account_id", table_name="human_production_packages")
    op.drop_table("human_production_packages")
    op.drop_index("uq_media_shot_current", table_name="media_shots")
    op.drop_index("ix_media_shots_project_id", table_name="media_shots")
    op.drop_index("ix_media_shots_account_id", table_name="media_shots")
    op.drop_table("media_shots")
    op.drop_index("ix_media_scenes_project_id", table_name="media_scenes")
    op.drop_index("ix_media_scenes_account_id", table_name="media_scenes")
    op.drop_table("media_scenes")
    op.drop_index("ix_media_rights_active_subject", table_name="media_rights_grants")
    op.drop_index("ix_media_rights_grants_project_id", table_name="media_rights_grants")
    op.drop_index("ix_media_rights_grants_account_id", table_name="media_rights_grants")
    op.drop_table("media_rights_grants")
    op.drop_index("ix_voice_assets_project_name", table_name="voice_assets")
    op.drop_index("ix_voice_assets_project_id", table_name="voice_assets")
    op.drop_index("ix_voice_assets_account_id", table_name="voice_assets")
    op.drop_table("voice_assets")
    op.drop_index("ix_character_profiles_project_name", table_name="character_profiles")
    op.drop_index("ix_character_profiles_project_id", table_name="character_profiles")
    op.drop_index("ix_character_profiles_account_id", table_name="character_profiles")
    op.drop_table("character_profiles")
    op.drop_index("ix_media_productions_lineage", table_name="media_productions")
    op.drop_index("ix_media_productions_project_id", table_name="media_productions")
    op.drop_index("ix_media_productions_account_id", table_name="media_productions")
    op.drop_table("media_productions")
```

Save this as `backend/migrations/versions/0003_media.py`.

- [ ] **Step 4: Upgrade, inspect, downgrade, and upgrade the migration**

Run: `cd backend && uv run alembic upgrade 0003_media && uv run pytest tests/integration/media/test_media_migration.py -v && uv run alembic downgrade 0002_intelligence && uv run alembic upgrade 0003_media`

Expected: migration commands exit 0 and the inspection test reports 1 passed.

- [ ] **Step 5: Verify model metadata and migration have no diff**

Run: `cd backend && uv run alembic check`

Expected: `No new upgrade operations detected.`

- [ ] **Step 6: Commit the fixed media migration**

```bash
git add backend/migrations/versions/0003_media.py backend/tests/integration/media/test_media_migration.py
git commit -m "feat: add media studio schema"
```

### Task 7: Route human, AI, and mixed shots and build the human production package

**Files:**
- Create: `backend/src/ip_saas/modules/media/routing.py`
- Create: `backend/src/ip_saas/modules/media/storyboard.py`
- Test: `backend/tests/unit/media/test_routing_storyboard.py`

- [ ] **Step 1: Write failing routing and package tests**

```python
import pytest

from ip_saas.common.errors import Conflict
from ip_saas.modules.media.enums import Capability, ProductionMode, ShotSource, VoiceKind
from ip_saas.modules.media.routing import route_shots
from ip_saas.modules.media.schemas import SceneDraft, ShotDraft
from ip_saas.modules.media.storyboard import build_human_package


def shot(sequence: int, source: ShotSource, spoken: bool = False) -> ShotDraft:
    return ShotDraft(
        sequence=sequence,
        title=f"镜头 {sequence}",
        source=source,
        duration_ms=5000,
        prompt="固定机位，人物把礼盒放到桌面",
        spoken_text="礼不是价格，是关系。" if spoken else None,
        voice_kind=VoiceKind.STANDARD_TTS if spoken else None,
    )


def test_human_route_creates_only_a_non_billable_package_job() -> None:
    routes = route_shots(ProductionMode.HUMAN, [shot(1, ShotSource.HUMAN_CAPTURE)])
    assert [route.capability for route in routes] == [Capability.HUMAN_PACKAGE]
    assert routes[0].billable is False


def test_ai_route_creates_storyboard_video_and_tts_work() -> None:
    routes = route_shots(ProductionMode.AI, [shot(1, ShotSource.SEEDANCE, spoken=True)])
    assert [route.capability for route in routes] == [
        Capability.STORYBOARD_IMAGE,
        Capability.LOW_RES_VIDEO,
        Capability.STANDARD_TTS,
    ]
    assert all(route.billable for route in routes)


def test_mixed_route_requires_both_human_and_ai_sources() -> None:
    with pytest.raises(Conflict, match="both human and AI"):
        route_shots(ProductionMode.MIXED, [shot(1, ShotSource.SEEDANCE)])


def test_human_package_contains_execution_notes_for_each_human_shot() -> None:
    scenes = [SceneDraft(sequence=1, title="客厅", purpose="建立人情压力", location="客户家中")]
    shots = {1: [shot(1, ShotSource.HUMAN_CAPTURE)]}
    package = build_human_package(scenes, shots)
    assert package["call_sheet"]["locations"] == ["客户家中"]
    assert package["shot_list"][0]["sequence"] == 1
    assert package["performance_notes"][0]["objective"]
    assert package["low_cost_alternatives"][0]["alternative"]
```

- [ ] **Step 2: Run the tests and verify both modules are missing**

Run: `cd backend && uv run pytest tests/unit/media/test_routing_storyboard.py -v`

Expected: FAIL during collection with a missing `routing` or `storyboard` module.

- [ ] **Step 3: Implement the complete deterministic route planner**

```python
from dataclasses import dataclass

from ip_saas.common.errors import Conflict

from .enums import Capability, ProductionMode, ShotSource, VoiceKind
from .schemas import ShotDraft


@dataclass(frozen=True)
class ShotRoute:
    shot_sequence: int
    capability: Capability
    provider: str
    billable: bool


_HUMAN_SOURCES = frozenset({ShotSource.HUMAN_CAPTURE, ShotSource.UPLOADED_MEDIA})
_AI_SOURCES = frozenset({ShotSource.SEEDREAM, ShotSource.SEEDANCE})


def route_shots(mode: ProductionMode, shots: list[ShotDraft]) -> list[ShotRoute]:
    if not shots:
        raise Conflict("a production requires at least one shot")
    sources = {shot.source for shot in shots}
    if mode is ProductionMode.HUMAN and not sources <= _HUMAN_SOURCES:
        raise Conflict("human mode cannot contain AI-generated shots")
    if mode is ProductionMode.AI and not sources <= _AI_SOURCES:
        raise Conflict("AI mode cannot contain human-capture shots")
    if mode is ProductionMode.MIXED and not (
        sources & _HUMAN_SOURCES and sources & _AI_SOURCES
    ):
        raise Conflict("mixed mode requires both human and AI shot sources")

    routes: list[ShotRoute] = []
    for item in sorted(shots, key=lambda value: value.sequence):
        if item.source in _HUMAN_SOURCES:
            routes.append(
                ShotRoute(
                    shot_sequence=item.sequence,
                    capability=Capability.HUMAN_PACKAGE,
                    provider="local",
                    billable=False,
                )
            )
        else:
            routes.append(
                ShotRoute(
                    shot_sequence=item.sequence,
                    capability=Capability.STORYBOARD_IMAGE,
                    provider="seedream",
                    billable=True,
                )
            )
            if item.source is ShotSource.SEEDANCE:
                routes.append(
                    ShotRoute(
                        shot_sequence=item.sequence,
                        capability=Capability.LOW_RES_VIDEO,
                        provider="seedance",
                        billable=True,
                    )
                )
        if item.spoken_text and item.voice_kind is VoiceKind.STANDARD_TTS:
            routes.append(
                ShotRoute(
                    shot_sequence=item.sequence,
                    capability=Capability.STANDARD_TTS,
                    provider="doubao_tts",
                    billable=True,
                )
            )
    return routes
```

Save this as `backend/src/ip_saas/modules/media/routing.py`.

- [ ] **Step 4: Implement the complete human production-package builder**

```python
from typing import Any

from .enums import ShotSource
from .schemas import SceneDraft, ShotDraft


def build_human_package(
    scenes: list[SceneDraft],
    shots_by_scene: dict[int, list[ShotDraft]],
) -> dict[str, Any]:
    human_shots = [
        shot
        for scene in sorted(scenes, key=lambda item: item.sequence)
        for shot in sorted(shots_by_scene.get(scene.sequence, []), key=lambda item: item.sequence)
        if shot.source in {ShotSource.HUMAN_CAPTURE, ShotSource.UPLOADED_MEDIA}
    ]
    locations = list(dict.fromkeys(scene.location for scene in scenes))
    return {
        "call_sheet": {
            "locations": locations,
            "shot_count": len(human_shots),
            "estimated_minutes": max(30, len(human_shots) * 15),
            "safety": [
                "拍摄前确认场地和人物授权仍有效",
                "画面不得暴露住址、车牌、学校或其他非必要个人信息",
            ],
        },
        "shot_list": [
            {
                "sequence": shot.sequence,
                "title": shot.title,
                "duration_ms": shot.duration_ms,
                "action": shot.prompt,
                "dialogue": shot.spoken_text,
            }
            for shot in human_shots
        ],
        "performance_notes": [
            {
                "sequence": shot.sequence,
                "objective": "完成镜头中的具体动作并让台词像当下真实反应",
                "obstacle": "避免只背台词或直接讲产品卖点",
                "action": shot.prompt,
            }
            for shot in human_shots
        ],
        "sound_notes": [
            {
                "sequence": shot.sequence,
                "instruction": "先录十秒环境底噪，再录两遍干净对白",
            }
            for shot in human_shots
        ],
        "props_and_wardrobe": [
            {
                "sequence": shot.sequence,
                "instruction": "只使用真实商品、真实包装和已经确认的服装道具",
            }
            for shot in human_shots
        ],
        "low_cost_alternatives": [
            {
                "sequence": shot.sequence,
                "alternative": "在同一场地用固定机位和自然窗光完成，不新增场景",
            }
            for shot in human_shots
        ],
    }
```

Save this as `backend/src/ip_saas/modules/media/storyboard.py`.

- [ ] **Step 5: Run the route and package tests**

Run: `cd backend && uv run pytest tests/unit/media/test_routing_storyboard.py -v`

Expected: 4 passed.

- [ ] **Step 6: Commit route planning and human production output**

```bash
git add backend/src/ip_saas/modules/media/routing.py backend/src/ip_saas/modules/media/storyboard.py backend/tests/unit/media/test_routing_storyboard.py
git commit -m "feat: route human ai and mixed productions"
```

### Task 8: Define the provider boundary and a zero-credential deterministic fake

**Files:**
- Create: `backend/src/ip_saas/providers/media/__init__.py`
- Create: `backend/src/ip_saas/providers/media/base.py`
- Create: `backend/src/ip_saas/providers/media/fake.py`
- Test: `backend/tests/unit/media/test_fake_provider.py`

- [ ] **Step 1: Write failing fake-provider tests, including a clean environment**

```python
from datetime import UTC, datetime

import pytest

from ip_saas.modules.media.enums import Capability, ProviderPhase
from ip_saas.providers.media.base import (
    InvalidProviderRequestIdentity,
    ProviderPollRequest,
    ProviderRequest,
    canonical_provider_request_id,
)
from ip_saas.providers.media.fake import FakeTemporaryStore, ScriptedFakeMediaProvider


def request() -> ProviderRequest:
    return ProviderRequest(
        idempotency_key="job:one",
        capability=Capability.LOW_RES_VIDEO,
        model_id="fake-seedance-model",
        model_version="2026-08-24",
        prompt="榴莲摊主打开仓库门",
        reference_urls=(),
        parameters={"duration": 5, "ratio": "9:16", "resolution": "480p"},
    )


def test_fake_runs_without_any_paid_provider_secret(monkeypatch) -> None:
    for name in (
        "ARK_API_KEY",
        "DOUBAO_TTS_APP_ID",
        "DOUBAO_TTS_ACCESS_KEY",
        "TOS_ACCESS_KEY",
        "TOS_SECRET_KEY",
    ):
        monkeypatch.delenv(name, raising=False)
    store = FakeTemporaryStore(datetime(2026, 8, 24, tzinfo=UTC))
    provider = ScriptedFakeMediaProvider(store, success_after_polls=1)
    submitted = provider.submit(request())
    assert submitted.phase is ProviderPhase.QUEUED
    completed = provider.poll(
        ProviderPollRequest(
            submitted.provider_task_id,
            submitted.model_id,
            submitted.model_version,
        )
    )
    assert completed.phase is ProviderPhase.SUCCEEDED
    assert completed.artifact is not None
    assert completed.provider_request_id != completed.provider_task_id
    assert store.read(completed.artifact.temporary_url).startswith(b"FAKE_MEDIA")


def test_submit_is_idempotent_and_does_not_create_a_second_task() -> None:
    store = FakeTemporaryStore(datetime(2026, 8, 24, tzinfo=UTC))
    provider = ScriptedFakeMediaProvider(store, success_after_polls=0)
    first = provider.submit(request())
    second = provider.submit(request())
    assert first.provider_task_id == second.provider_task_id
    assert provider.submit_count == 1


@pytest.mark.parametrize(
    "value", [None, 7, "", "   ", "nOnE", "MISSING", "x" * 201]
)
def test_provider_request_identity_rejects_invalid_values(value: object) -> None:
    with pytest.raises(InvalidProviderRequestIdentity):
        canonical_provider_request_id(value)
```

- [ ] **Step 2: Run the tests and verify the missing provider package failure**

Run: `cd backend && env -u ARK_API_KEY -u DOUBAO_TTS_APP_ID -u DOUBAO_TTS_ACCESS_KEY -u TOS_ACCESS_KEY -u TOS_SECRET_KEY uv run pytest tests/unit/media/test_fake_provider.py -v`

Expected: FAIL during collection because `ip_saas.providers.media` does not exist.

- [ ] **Step 3: Create the complete provider protocol and normalized result types**

```python
from dataclasses import dataclass
from datetime import datetime
from typing import Any, Protocol

from ip_saas.modules.billing.models import ReconciliationStatus
from ip_saas.modules.media.enums import Capability, ProviderPhase


class InvalidProviderRequestIdentity(ValueError):
    pass


def canonical_provider_request_id(value: object) -> str:
    if not isinstance(value, str):
        raise InvalidProviderRequestIdentity("provider request id must be a string")
    normalized = value.strip()
    if (
        not 1 <= len(normalized) <= 200
        or normalized.casefold() in {"none", "missing"}
    ):
        raise InvalidProviderRequestIdentity(
            "provider request id must be canonical 1-200 supplier text"
        )
    return normalized


@dataclass(frozen=True)
class ProviderRequest:
    idempotency_key: str
    capability: Capability
    model_id: str
    model_version: str
    prompt: str
    reference_urls: tuple[str, ...]
    parameters: dict[str, Any]


@dataclass(frozen=True)
class ProviderArtifact:
    temporary_url: str
    expires_at: datetime
    content_type: str


@dataclass(frozen=True)
class ProviderPollRequest:
    provider_task_id: str
    model_id: str
    model_version: str


@dataclass(frozen=True)
class ProviderUsage:
    native_quantity: int
    native_unit: str
    supplier_amount_minor: int
    supplier_currency: str
    amount_fen: int
    reconciliation_status: ReconciliationStatus


@dataclass(frozen=True)
class ProviderResult:
    phase: ProviderPhase
    provider_task_id: str | None
    provider_request_id: str
    model_id: str
    model_version: str
    artifact: ProviderArtifact | None
    usage: ProviderUsage | None
    retry_after_seconds: int | None = None
    error_code: str | None = None
    error_message: str | None = None

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "provider_request_id",
            canonical_provider_request_id(self.provider_request_id),
        )
        if (
            self.phase in {ProviderPhase.QUEUED, ProviderPhase.RUNNING}
            and not self.provider_task_id
        ):
            raise ValueError("pollable provider result requires provider_task_id")


class MediaProvider(Protocol):
    provider_name: str

    def submit(self, request: ProviderRequest) -> ProviderResult:
        raise NotImplementedError

    def poll(self, request: ProviderPollRequest) -> ProviderResult:
        raise NotImplementedError
```

Save this as `backend/src/ip_saas/providers/media/base.py`. Export `InvalidProviderRequestIdentity` and `canonical_provider_request_id` from `backend/src/ip_saas/providers/media/__init__.py`. `ProviderResult.provider_request_id` is the canonical supplier request/correlation identity used by attempt evidence and billing; `provider_task_id` is nullable and exists only as an asynchronous resource locator for polling. The two fields are never derived from each other. A real adapter that cannot return a valid request ID raises `InvalidProviderRequestIdentity` after the send marker, so the Worker persists no result/usage/cost and retries until supplier reconciliation; it never hashes an idempotency key, invents a UUID, uses the task ID, or writes the string `None` as fallback.

- [ ] **Step 4: Create the complete deterministic fake and temporary store**

```python
from dataclasses import dataclass
from datetime import datetime, timedelta
from hashlib import sha256

from ip_saas.modules.billing.models import ReconciliationStatus
from ip_saas.modules.media.enums import Capability, ProviderPhase

from .base import (
    MediaProvider,
    ProviderArtifact,
    ProviderPollRequest,
    ProviderRequest,
    ProviderResult,
    ProviderUsage,
)


@dataclass
class _Task:
    request: ProviderRequest
    polls: int
    result: ProviderResult | None


class FakeTemporaryStore:
    def __init__(self, now: datetime) -> None:
        self.now = now
        self._objects: dict[str, tuple[bytes, datetime, str]] = {}

    def put(self, key: str, body: bytes, content_type: str) -> ProviderArtifact:
        expires_at = self.now + timedelta(hours=1)
        url = f"fake://temporary/{key}"
        self._objects[url] = (body, expires_at, content_type)
        return ProviderArtifact(url, expires_at, content_type)

    def read(self, url: str) -> bytes:
        body, expires_at, _ = self._objects[url]
        if self.now >= expires_at:
            raise TimeoutError("fake temporary URL expired")
        return body

    def metadata(self, url: str) -> tuple[datetime, str]:
        _, expires_at, content_type = self._objects[url]
        return expires_at, content_type

    def advance(self, seconds: int) -> None:
        self.now += timedelta(seconds=seconds)


class ScriptedFakeMediaProvider(MediaProvider):
    provider_name = "fake_media"

    def __init__(
        self,
        store: FakeTemporaryStore,
        *,
        success_after_polls: int,
        fail_code: str | None = None,
    ) -> None:
        self.store = store
        self.success_after_polls = success_after_polls
        self.fail_code = fail_code
        self._tasks: dict[str, _Task] = {}
        self.submit_count = 0

    def submit(self, request: ProviderRequest) -> ProviderResult:
        task_id = "fake-" + sha256(request.idempotency_key.encode()).hexdigest()[:24]
        existing = self._tasks.get(task_id)
        if existing is not None:
            if existing.result is not None:
                return existing.result
            return self._queued(task_id, request)
        self.submit_count += 1
        self._tasks[task_id] = _Task(request=request, polls=0, result=None)
        if self.success_after_polls == 0:
            result = self._complete(task_id, request)
            self._tasks[task_id].result = result
            return result
        return self._queued(task_id, request)

    def poll(self, request: ProviderPollRequest) -> ProviderResult:
        provider_task_id = request.provider_task_id
        task = self._tasks[provider_task_id]
        if task.result is not None:
            return task.result
        task.polls += 1
        if task.polls < self.success_after_polls:
            return self._running(provider_task_id, task.request)
        result = self._complete(provider_task_id, task.request)
        task.result = result
        return result

    def _complete(self, task_id: str, request: ProviderRequest) -> ProviderResult:
        if self.fail_code is not None:
            return ProviderResult(
                phase=ProviderPhase.FAILED,
                provider_task_id=task_id,
                provider_request_id=f"fake-request-failed-{task_id}",
                model_id=request.model_id,
                model_version=request.model_version,
                artifact=None,
                usage=ProviderUsage(
                    1, "request", 2, "CNY", 2,
                    ReconciliationStatus.UNRECONCILED,
                ),
                error_code=self.fail_code,
                error_message="deterministic fake failure",
            )
        content_type = {
            Capability.STORYBOARD_IMAGE: "image/png",
            Capability.LOW_RES_VIDEO: "video/mp4",
            Capability.HIGH_RES_VIDEO: "video/mp4",
            Capability.STANDARD_TTS: "audio/mpeg",
        }.get(request.capability, "application/octet-stream")
        body = (
            b"FAKE_MEDIA\n"
            + request.capability.value.encode()
            + b"\n"
            + request.prompt.encode("utf-8")
        )
        artifact = self.store.put(task_id, body, content_type)
        return ProviderResult(
            phase=ProviderPhase.SUCCEEDED,
            provider_task_id=task_id,
            provider_request_id=f"fake-request-succeeded-{task_id}",
            model_id=request.model_id,
            model_version=request.model_version,
            artifact=artifact,
            usage=ProviderUsage(
                1, "generation", 8, "CNY", 8,
                ReconciliationStatus.UNRECONCILED,
            ),
        )

    @staticmethod
    def _queued(task_id: str, request: ProviderRequest) -> ProviderResult:
        return ProviderResult(
            phase=ProviderPhase.QUEUED,
            provider_task_id=task_id,
            provider_request_id=f"fake-request-queued-{task_id}",
            model_id=request.model_id,
            model_version=request.model_version,
            artifact=None,
            usage=None,
            retry_after_seconds=1,
        )

    @staticmethod
    def _running(task_id: str, request: ProviderRequest) -> ProviderResult:
        return ProviderResult(
            phase=ProviderPhase.RUNNING,
            provider_task_id=task_id,
            provider_request_id=f"fake-request-running-{task_id}",
            model_id=request.model_id,
            model_version=request.model_version,
            artifact=None,
            usage=None,
            retry_after_seconds=1,
        )
```

Save this as `backend/src/ip_saas/providers/media/fake.py`. Save the following as `backend/src/ip_saas/providers/media/__init__.py`:

```python
from .base import MediaProvider, ProviderPollRequest, ProviderRequest, ProviderResult

__all__ = ["MediaProvider", "ProviderPollRequest", "ProviderRequest", "ProviderResult"]
```

- [ ] **Step 5: Run the fake-provider tests with every paid secret removed**

Run: `cd backend && env -u ARK_API_KEY -u DOUBAO_TTS_APP_ID -u DOUBAO_TTS_ACCESS_KEY -u TOS_ACCESS_KEY -u TOS_SECRET_KEY uv run pytest tests/unit/media/test_fake_provider.py -v`

Expected: 9 passed and no network request; the seven invalid identity variants fail before any fake result can reach evidence or billing.

- [ ] **Step 6: Commit provider normalization and fake execution**

```bash
git add backend/src/ip_saas/providers/media backend/tests/unit/media/test_fake_provider.py
git commit -m "feat: add deterministic media provider fake"
```

### Task 9: Add contract-tested Seedream, Seedance, and standard TTS adapters

**Files:**
- Create: `backend/src/ip_saas/providers/volcengine/__init__.py`
- Create: `backend/src/ip_saas/providers/volcengine/auth.py`
- Create: `backend/src/ip_saas/providers/volcengine/seedream.py`
- Create: `backend/src/ip_saas/providers/volcengine/seedance.py`
- Create: `backend/src/ip_saas/providers/volcengine/tts.py`
- Create: `backend/src/ip_saas/providers/media/temporary.py`
- Test: `backend/tests/contract/media/test_volcengine_adapters.py`

Do not hardcode a floating marketing name such as `seedance-latest`. The Plan 01 model registry resolves an enabled capability to a full `model_id` and `model_version`; `GenerationJob` freezes both before outbox publication. Administrators discover account-visible resources through Ark `ListModelActivations` and verify versions through `GetFoundationModelVersion`, run the fixed regression set, and only then switch the registry entry. These adapters only execute the pinned values received in `ProviderRequest`.

- [ ] **Step 1: Write failing adapter contract tests with `httpx.MockTransport`**

```python
import json
from datetime import UTC, datetime

import httpx
import pytest

from ip_saas.modules.media.enums import Capability, ProviderPhase
from ip_saas.providers.media.base import (
    InvalidProviderRequestIdentity,
    ProviderPollRequest,
    ProviderRequest,
)
from ip_saas.providers.media.temporary import MemoryArtifactSink
from ip_saas.providers.volcengine.auth import ArkCredentials, SpeechCredentials
from ip_saas.providers.volcengine.seedance import SeedanceAdapter
from ip_saas.providers.volcengine.seedream import SeedreamAdapter
from ip_saas.providers.volcengine.tts import StandardTtsAdapter


class FixedClock:
    def now(self) -> datetime:
        return datetime(2026, 8, 24, tzinfo=UTC)


def media_request(capability: Capability) -> ProviderRequest:
    return ProviderRequest(
        idempotency_key="contract-job-1",
        capability=capability,
        model_id="ep-20260824-media",
        model_version="2026-08-24",
        prompt="礼盒放在木桌上，暖色侧光",
        reference_urls=(),
        parameters={"ratio": "9:16", "resolution": "480p", "duration": 5},
    )


def test_seedream_uses_pinned_model_and_returns_a_temporary_artifact() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        payload = json.loads(request.content)
        assert request.headers["Authorization"] == "Bearer test-ark-key"
        assert payload["model"] == "ep-20260824-media"
        assert payload["watermark"] is True
        return httpx.Response(
            200,
            headers={"x-request-id": "image-request-1"},
            json={"data": [{"url": "https://provider.example/image.png"}]},
        )

    client = httpx.Client(transport=httpx.MockTransport(handler))
    result = SeedreamAdapter(client, ArkCredentials("test-ark-key"), FixedClock()).submit(
        media_request(Capability.STORYBOARD_IMAGE)
    )
    assert result.phase is ProviderPhase.SUCCEEDED
    assert result.artifact is not None
    assert result.provider_request_id == "image-request-1"
    assert result.provider_task_id is None
    assert result.provider_request_id != result.provider_task_id
    assert result.artifact.temporary_url.endswith("image.png")


def test_seedance_submit_then_poll_normalizes_async_status() -> None:
    calls = 0

    def handler(request: httpx.Request) -> httpx.Response:
        nonlocal calls
        calls += 1
        if request.method == "POST":
            return httpx.Response(
                200,
                headers={"x-request-id": "video-submit-request-1"},
                json={"id": "video-task-1", "status": "queued"},
            )
        return httpx.Response(
            200,
            headers={"x-request-id": "video-poll-request-1"},
            json={
                "id": "video-task-1",
                "status": "succeeded",
                "model": "ep-20260824-media",
                "content": {"video_url": "https://provider.example/video.mp4"},
                "usage": {"completion_tokens": 300},
            },
        )

    adapter = SeedanceAdapter(
        httpx.Client(transport=httpx.MockTransport(handler)),
        ArkCredentials("test-ark-key"),
        FixedClock(),
    )
    submitted = adapter.submit(media_request(Capability.LOW_RES_VIDEO))
    completed = adapter.poll(
        ProviderPollRequest(
            submitted.provider_task_id,
            submitted.model_id,
            submitted.model_version,
        )
    )
    assert submitted.phase is ProviderPhase.QUEUED
    assert submitted.provider_request_id == "video-submit-request-1"
    assert submitted.provider_request_id != submitted.provider_task_id
    assert completed.phase is ProviderPhase.SUCCEEDED
    assert completed.provider_request_id == "video-poll-request-1"
    assert completed.provider_request_id != completed.provider_task_id
    assert completed.artifact is not None
    assert calls == 2


def test_standard_tts_uses_speech_headers_and_stages_binary_audio() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        assert request.headers["X-Api-App-Id"] == "test-app"
        assert request.headers["X-Api-Resource-Id"] == "test-resource"
        return httpx.Response(
            200,
            headers={
                "content-type": "audio/mpeg",
                "x-tt-logid": "tts-request-1",
            },
            content=b"ID3-test-audio",
        )

    sink = MemoryArtifactSink(FixedClock())
    adapter = StandardTtsAdapter(
        httpx.Client(transport=httpx.MockTransport(handler)),
        SpeechCredentials("test-app", "test-access", "test-resource"),
        sink,
    )
    request = ProviderRequest(
        idempotency_key="tts-1",
        capability=Capability.STANDARD_TTS,
        model_id="doubao-standard-tts",
        model_version="v3",
        prompt="礼不是价格，是关系。",
        reference_urls=(),
        parameters={"speaker": "zh_female_wanqudashu_moon_bigtts"},
    )
    result = adapter.submit(request)
    assert result.phase is ProviderPhase.SUCCEEDED
    assert result.provider_request_id == "tts-request-1"
    assert result.provider_task_id is None
    assert result.provider_request_id != result.provider_task_id
    assert result.artifact is not None
    assert sink.read(result.artifact.temporary_url) == b"ID3-test-audio"


@pytest.mark.parametrize("adapter_name", ["seedream", "seedance", "standard_tts"])
@pytest.mark.parametrize("raw", [None, "", "   ", "nOnE", "x" * 201])
def test_adapter_rejects_missing_or_noncanonical_supplier_request_id(
    adapter_name: str,
    raw: str | None,
) -> None:
    header_name = "x-tt-logid" if adapter_name == "standard_tts" else "x-request-id"
    headers = {} if raw is None else {header_name: raw}

    def handler(_request: httpx.Request) -> httpx.Response:
        if adapter_name == "seedream":
            return httpx.Response(
                200, headers=headers,
                json={"data": [{"url": "https://provider.example/image.png"}]},
            )
        if adapter_name == "seedance":
            return httpx.Response(
                200, headers=headers,
                json={"id": "video-task-1", "status": "queued"},
            )
        return httpx.Response(
            200,
            headers={**headers, "content-type": "audio/mpeg"},
            content=b"ID3-test-audio",
        )

    client = httpx.Client(transport=httpx.MockTransport(handler))
    if adapter_name == "seedream":
        adapter = SeedreamAdapter(client, ArkCredentials("test-ark-key"), FixedClock())
        request = media_request(Capability.STORYBOARD_IMAGE)
    elif adapter_name == "seedance":
        adapter = SeedanceAdapter(client, ArkCredentials("test-ark-key"), FixedClock())
        request = media_request(Capability.LOW_RES_VIDEO)
    else:
        adapter = StandardTtsAdapter(
            client,
            SpeechCredentials("test-app", "test-access", "test-resource"),
            MemoryArtifactSink(FixedClock()),
        )
        request = ProviderRequest(
            idempotency_key="tts-invalid-request-id",
            capability=Capability.STANDARD_TTS,
            model_id="doubao-standard-tts",
            model_version="v3",
            prompt="礼不是价格，是关系。",
            reference_urls=(),
            parameters={"speaker": "zh_female_wanqudashu_moon_bigtts"},
        )
    with pytest.raises(InvalidProviderRequestIdentity):
        adapter.submit(request)
```

- [ ] **Step 2: Run the contract tests and verify the adapters are missing**

Run: `cd backend && env -u ARK_API_KEY -u DOUBAO_TTS_APP_ID -u DOUBAO_TTS_ACCESS_KEY uv run pytest tests/contract/media/test_volcengine_adapters.py -v`

Expected: FAIL during collection because the Volcengine adapter modules do not exist.

- [ ] **Step 3: Create immutable credential values and a staging sink contract**

```python
from dataclasses import dataclass


@dataclass(frozen=True)
class ArkCredentials:
    api_key: str


@dataclass(frozen=True)
class SpeechCredentials:
    app_id: str
    access_key: str
    resource_id: str
```

Save this as `backend/src/ip_saas/providers/volcengine/auth.py`.

```python
from datetime import timedelta
from hashlib import sha256
from typing import Protocol

from ip_saas.common.clock import Clock

from .base import ProviderArtifact


class TemporaryArtifactSink(Protocol):
    def put(self, key: str, body: bytes, content_type: str) -> ProviderArtifact:
        raise NotImplementedError


class MemoryArtifactSink:
    def __init__(self, clock: Clock) -> None:
        self.clock = clock
        self._objects: dict[str, bytes] = {}

    def put(self, key: str, body: bytes, content_type: str) -> ProviderArtifact:
        digest = sha256(body).hexdigest()
        url = f"memory://tts-stage/{key}/{digest}"
        self._objects[url] = body
        return ProviderArtifact(
            temporary_url=url,
            expires_at=self.clock.now() + timedelta(hours=1),
            content_type=content_type,
        )

    def read(self, url: str) -> bytes:
        return self._objects[url]
```

Save this as `backend/src/ip_saas/providers/media/temporary.py`. In production wiring, bind `TemporaryArtifactSink` to the private TOS staging prefix configured with a one-hour lifecycle; never emit its signed staging URL to the browser.

- [ ] **Step 4: Implement the synchronous Seedream image adapter**

```python
from datetime import timedelta
import httpx

from ip_saas.common.clock import Clock
from ip_saas.modules.billing.models import ReconciliationStatus
from ip_saas.modules.media.enums import ProviderPhase
from ip_saas.providers.media.base import (
    MediaProvider,
    ProviderArtifact,
    ProviderPollRequest,
    ProviderRequest,
    ProviderResult,
    ProviderUsage,
    canonical_provider_request_id,
)

from .auth import ArkCredentials


class SeedreamAdapter(MediaProvider):
    provider_name = "seedream"

    def __init__(
        self,
        client: httpx.Client,
        credentials: ArkCredentials,
        clock: Clock,
        *,
        endpoint: str = "https://ark.cn-beijing.volces.com/api/v3/images/generations",
    ) -> None:
        self.client = client
        self.credentials = credentials
        self.clock = clock
        self.endpoint = endpoint

    def submit(self, request: ProviderRequest) -> ProviderResult:
        payload: dict[str, object] = {
            "model": request.model_id,
            "prompt": request.prompt,
            "size": request.parameters.get("size", "1024x1792"),
            "response_format": "url",
            "watermark": True,
        }
        if request.reference_urls:
            payload["image"] = list(request.reference_urls)
        response = self.client.post(
            self.endpoint,
            headers={
                "Authorization": f"Bearer {self.credentials.api_key}",
                "X-Client-Request-Id": request.idempotency_key,
            },
            json=payload,
            timeout=120,
        )
        response.raise_for_status()
        data = response.json()
        url = str(data["data"][0]["url"])
        provider_request_id = canonical_provider_request_id(
            response.headers.get("x-request-id")
        )
        return ProviderResult(
            phase=ProviderPhase.SUCCEEDED,
            provider_task_id=None,
            provider_request_id=provider_request_id,
            model_id=request.model_id,
            model_version=request.model_version,
            artifact=ProviderArtifact(
                temporary_url=url,
                expires_at=self.clock.now() + timedelta(hours=23),
                content_type="image/png",
            ),
            usage=ProviderUsage(
                1, "image", 0, "CNY", 0,
                ReconciliationStatus.UNRECONCILED,
            ),
        )

    def poll(self, request: ProviderPollRequest) -> ProviderResult:
        raise RuntimeError(
            f"Seedream task {request.provider_task_id} is synchronous and cannot be polled"
        )
```

Save this as `backend/src/ip_saas/providers/volcengine/seedream.py`.

- [ ] **Step 5: Implement the asynchronous Seedance task adapter**

```python
from datetime import timedelta

import httpx

from ip_saas.common.clock import Clock
from ip_saas.modules.billing.models import ReconciliationStatus
from ip_saas.modules.media.enums import ProviderPhase
from ip_saas.providers.media.base import (
    MediaProvider,
    ProviderArtifact,
    ProviderPollRequest,
    ProviderRequest,
    ProviderResult,
    ProviderUsage,
    canonical_provider_request_id,
)

from .auth import ArkCredentials


_PHASES = {
    "queued": ProviderPhase.QUEUED,
    "running": ProviderPhase.RUNNING,
    "succeeded": ProviderPhase.SUCCEEDED,
    "failed": ProviderPhase.FAILED,
}


class SeedanceAdapter(MediaProvider):
    provider_name = "seedance"

    def __init__(
        self,
        client: httpx.Client,
        credentials: ArkCredentials,
        clock: Clock,
        *,
        endpoint: str = "https://ark.cn-beijing.volces.com/api/v3/contents/generations/tasks",
    ) -> None:
        self.client = client
        self.credentials = credentials
        self.clock = clock
        self.endpoint = endpoint

    def submit(self, request: ProviderRequest) -> ProviderResult:
        content: list[dict[str, object]] = [{"type": "text", "text": request.prompt}]
        content.extend(
            {"type": "image_url", "image_url": {"url": url}}
            for url in request.reference_urls
        )
        payload = {
            "model": request.model_id,
            "content": content,
            "ratio": request.parameters["ratio"],
            "resolution": request.parameters["resolution"],
            "duration": request.parameters["duration"],
            "watermark": True,
        }
        response = self.client.post(
            self.endpoint,
            headers={
                "Authorization": f"Bearer {self.credentials.api_key}",
                "X-Client-Request-Id": request.idempotency_key,
            },
            json=payload,
            timeout=120,
        )
        response.raise_for_status()
        data = response.json()
        task_id = str(data["id"])
        provider_request_id = canonical_provider_request_id(
            response.headers.get("x-request-id")
        )
        phase = _PHASES.get(str(data.get("status", "queued")), ProviderPhase.QUEUED)
        return ProviderResult(
            phase=phase,
            provider_task_id=task_id,
            provider_request_id=provider_request_id,
            model_id=request.model_id,
            model_version=request.model_version,
            artifact=None,
            usage=None,
            retry_after_seconds=5,
        )

    def poll(self, request: ProviderPollRequest) -> ProviderResult:
        provider_task_id = request.provider_task_id
        response = self.client.get(
            f"{self.endpoint}/{provider_task_id}",
            headers={"Authorization": f"Bearer {self.credentials.api_key}"},
            timeout=60,
        )
        response.raise_for_status()
        data = response.json()
        provider_request_id = canonical_provider_request_id(
            response.headers.get("x-request-id")
        )
        phase = _PHASES[str(data["status"])]
        artifact = None
        usage = None
        if phase is ProviderPhase.SUCCEEDED:
            artifact = ProviderArtifact(
                temporary_url=str(data["content"]["video_url"]),
                expires_at=self.clock.now() + timedelta(hours=23),
                content_type="video/mp4",
            )
            native_quantity = int(data.get("usage", {}).get("completion_tokens", 0))
            usage = ProviderUsage(
                native_quantity,
                "completion_token",
                0,
                "CNY",
                0,
                ReconciliationStatus.UNRECONCILED,
            )
        return ProviderResult(
            phase=phase,
            provider_task_id=provider_task_id,
            provider_request_id=provider_request_id,
            model_id=request.model_id,
            model_version=request.model_version,
            artifact=artifact,
            usage=usage,
            retry_after_seconds=5 if phase in {ProviderPhase.QUEUED, ProviderPhase.RUNNING} else None,
            error_code=str(data.get("error", {}).get("code")) if phase is ProviderPhase.FAILED else None,
            error_message=str(data.get("error", {}).get("message")) if phase is ProviderPhase.FAILED else None,
        )
```

Save this as `backend/src/ip_saas/providers/volcengine/seedance.py`.

- [ ] **Step 6: Implement standard, non-cloned Doubao TTS V3**

```python
from hashlib import sha256
import httpx

from ip_saas.modules.billing.models import ReconciliationStatus
from ip_saas.modules.media.enums import ProviderPhase
from ip_saas.providers.media.base import (
    MediaProvider,
    ProviderPollRequest,
    ProviderRequest,
    ProviderResult,
    ProviderUsage,
    canonical_provider_request_id,
)
from ip_saas.providers.media.temporary import TemporaryArtifactSink

from .auth import SpeechCredentials


class StandardTtsAdapter(MediaProvider):
    provider_name = "doubao_tts"

    def __init__(
        self,
        client: httpx.Client,
        credentials: SpeechCredentials,
        sink: TemporaryArtifactSink,
        *,
        endpoint: str = "https://openspeech.bytedance.com/api/v3/tts/unidirectional",
    ) -> None:
        self.client = client
        self.credentials = credentials
        self.sink = sink
        self.endpoint = endpoint

    def submit(self, request: ProviderRequest) -> ProviderResult:
        response = self.client.post(
            self.endpoint,
            headers={
                "X-Api-App-Id": self.credentials.app_id,
                "X-Api-Access-Key": self.credentials.access_key,
                "X-Api-Resource-Id": self.credentials.resource_id,
                "X-Api-Request-Id": request.idempotency_key,
                "Content-Type": "application/json",
            },
            json={
                "user": {"uid": "ip-saas-worker"},
                "req_params": {
                    "text": request.prompt,
                    "speaker": request.parameters["speaker"],
                    "audio_params": {
                        "format": "mp3",
                        "sample_rate": 24000,
                        "speech_rate": int(request.parameters.get("speech_rate", 0)),
                    },
                },
            },
            timeout=120,
        )
        response.raise_for_status()
        provider_request_id = canonical_provider_request_id(
            response.headers.get("x-tt-logid")
        )
        artifact = self.sink.put(
            "tts-" + sha256(request.idempotency_key.encode()).hexdigest()[:24],
            response.content,
            response.headers.get("content-type", "audio/mpeg").split(";", 1)[0],
        )
        return ProviderResult(
            phase=ProviderPhase.SUCCEEDED,
            provider_task_id=None,
            provider_request_id=provider_request_id,
            model_id=request.model_id,
            model_version=request.model_version,
            artifact=artifact,
            usage=ProviderUsage(
                len(request.prompt), "character", 0, "CNY", 0,
                ReconciliationStatus.UNRECONCILED,
            ),
        )

    def poll(self, request: ProviderPollRequest) -> ProviderResult:
        raise RuntimeError(
            f"standard TTS task {request.provider_task_id} is synchronous and cannot be polled"
        )
```

Save this as `backend/src/ip_saas/providers/volcengine/tts.py`. Save this as `backend/src/ip_saas/providers/volcengine/__init__.py`:

```python
from .seedance import SeedanceAdapter
from .seedream import SeedreamAdapter
from .tts import StandardTtsAdapter

__all__ = ["SeedanceAdapter", "SeedreamAdapter", "StandardTtsAdapter"]
```

- [ ] **Step 7: Run adapter contracts without live credentials or network**

Run: `cd backend && env -u ARK_API_KEY -u DOUBAO_TTS_APP_ID -u DOUBAO_TTS_ACCESS_KEY uv run pytest tests/contract/media/test_volcengine_adapters.py -v`

Expected: 18 passed; `httpx.MockTransport` handled every request. Each adapter proves the supplier request ID is distinct from the polling task ID, and missing, blank, literal `None`, or 201-character response identities fail closed without a generated fallback.

- [ ] **Step 8: Commit the real adapters and pinned-model boundary**

```bash
git add backend/src/ip_saas/providers/volcengine backend/src/ip_saas/providers/media/temporary.py backend/tests/contract/media/test_volcengine_adapters.py
git commit -m "feat: add volcengine media adapters"
```

Official contract references used while maintaining these adapters: [Seedream image generation](https://www.volcengine.com/docs/82379/1824121?lang=zh), [Seedance task creation](https://www.volcengine.com/docs/82379/1520757?lang=zh), [Seedance task query](https://www.volcengine.com/docs/82379/1521309?lang=zh), [standard TTS V3](https://www.volcengine.com/docs/6561/2228192?lang=zh), [ListModelActivations](https://api.volcengine.com/api-docs/view?action=ListModelActivations&serviceCode=ark&version=2024-01-01), and [GetFoundationModelVersion](https://api.volcengine.com/api-docs/view?action=GetFoundationModelVersion&serviceCode=ark&version=2024-01-01). Task 17 defines the only live entry points: the three separately budget-capped `make poc-seedream`, `make poc-seedance`, and `make poc-standard-tts` targets. Ordinary tests never invoke those targets or call a paid endpoint.

### Task 10: Reserve exactly one hold and create each job plus outbox atomically

**Files:**
- Create: `backend/src/ip_saas/modules/media/pricing.py`
- Create: `backend/src/ip_saas/modules/media/service.py`
- Test: `backend/tests/unit/media/test_job_service.py`
- Test: `backend/tests/integration/media/test_job_hold_transaction.py`

- [ ] **Step 1: Write a failing service test for customer-credit creation**

```python
from types import SimpleNamespace
from unittest.mock import Mock
from uuid import uuid4

from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.media.enums import Capability, JobStatus
from ip_saas.modules.media.pricing import BillingRoute, MediaQuote
from ip_saas.modules.media.service import JobCreateCommand, MediaJobService


def test_customer_job_delegates_hold_and_outbox_to_shared_task_without_provider_call() -> None:
    account_id = uuid4()
    project_id = uuid4()
    production_id = uuid4()
    shot_id = uuid4()
    hold_id = uuid4()
    actor = ActorContext(uuid4(), account_id, ActorKind.C_USER)
    session = Mock()
    session.scalar.side_effect = [
        None,
        SimpleNamespace(id=production_id, project_id=project_id, account_id=account_id),
        SimpleNamespace(id=shot_id, project_id=project_id, production_id=production_id),
    ]
    access = Mock()
    access.require_editor.return_value = SimpleNamespace(id=project_id)
    tasks = Mock()
    tasks.submit_customer.return_value = SimpleNamespace(
        id=uuid4(),
        billing_mode="customer_credit",
        billing_hold_id=hold_id,
    )
    audit = Mock()
    service = MediaJobService(access, tasks, audit)
    command = JobCreateCommand(
        project_id=project_id,
        production_id=production_id,
        shot_id=shot_id,
        capability=Capability.LOW_RES_VIDEO,
        quote=MediaQuote(
            provider="seedance",
            model_id="ep-pinned",
            model_version="2026-08-24",
            credit_units=40,
            amount_fen=800,
            quote_version="price-v3",
        ),
        billing_route=BillingRoute.customer(account_id),
        approved_credit_units=40,
        approved_amount_fen=None,
        request_payload={"prompt": "镜头推进", "resolution": "480p"},
        idempotency_key="preview:shot-1:v1",
    )
    job = service.create(session, actor, command)
    assert job.provider_status == JobStatus.HELD.value
    assert job.task_record_id == tasks.submit_customer.return_value.id
    assert not hasattr(job, "generation_hold_id")
    assert not hasattr(job, "internal_budget_hold_id")
    tasks.submit_customer.assert_called_once()
    assert not hasattr(service, "provider")
```

- [ ] **Step 2: Run the unit test and verify the missing service failure**

Run: `cd backend && uv run pytest tests/unit/media/test_job_service.py -v`

Expected: FAIL during collection because `pricing.py` and `service.py` do not exist.

- [ ] **Step 3: Create exact quote and trusted billing-route values**

```python
from dataclasses import dataclass
from typing import Protocol
from uuid import UUID

from ip_saas.modules.billing.service import BillingMode


@dataclass(frozen=True)
class MediaQuote:
    provider: str
    model_id: str
    model_version: str
    credit_units: int
    amount_fen: int
    quote_version: str

    def __post_init__(self) -> None:
        if self.credit_units <= 0 or self.amount_fen <= 0:
            raise ValueError("media quotes require positive credit and CNY dimensions")


@dataclass(frozen=True)
class BillingRoute:
    mode: BillingMode
    account_id: UUID | None
    cost_center_id: UUID | None

    @classmethod
    def customer(cls, account_id: UUID) -> "BillingRoute":
        return cls(BillingMode.CUSTOMER_CREDIT, account_id, None)

    @classmethod
    def internal(cls, cost_center_id: UUID) -> "BillingRoute":
        return cls(BillingMode.INTERNAL_COST, None, cost_center_id)

    def __post_init__(self) -> None:
        if self.mode is BillingMode.CUSTOMER_CREDIT:
            if self.account_id is None or self.cost_center_id is not None:
                raise ValueError("customer route requires only account_id")
        elif self.cost_center_id is None or self.account_id is not None:
            raise ValueError("internal route requires only cost_center_id")
```

Save this as `backend/src/ip_saas/modules/media/pricing.py`. The trusted Plan 01 project-billing resolver derives this route from the persisted `IPProject` owner type; no HTTP request may choose `BillingMode` or a cost center.

- [ ] **Step 4: Implement transactional, idempotent job creation**

```python
import json
from dataclasses import dataclass
from hashlib import sha256
from uuid import UUID, uuid4

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.common.errors import Conflict, NotFound
from ip_saas.modules.accounts.context import ActorContext
from ip_saas.modules.billing.service import BillingMode
from ip_saas.modules.projects.access import ProjectAccessService
from ip_saas.modules.tasks.models import TaskRecord
from ip_saas.modules.tasks.service import TaskSubmissionService
from .enums import Capability, JobStatus
from .models.jobs import GenerationJob
from .models.production import MediaProduction, Shot
from .pricing import BillingRoute, MediaQuote


@dataclass(frozen=True)
class JobCreateCommand:
    project_id: UUID
    production_id: UUID
    shot_id: UUID | None
    capability: Capability
    quote: MediaQuote
    billing_route: BillingRoute
    approved_credit_units: int | None
    approved_amount_fen: int | None
    request_payload: dict[str, object]
    idempotency_key: str


class MediaJobService:
    def __init__(
        self,
        access: ProjectAccessService,
        task_submission: TaskSubmissionService,
        audit: AuditWriter,
    ) -> None:
        self.access = access
        self.task_submission = task_submission
        self.audit = audit

    def create(
        self,
        session: Session,
        actor: ActorContext,
        command: JobCreateCommand,
    ) -> GenerationJob:
        self.access.require_editor(session, actor, command.project_id)
        request_fingerprint = self._job_request_fingerprint(actor, command)
        existing = session.scalar(
            select(GenerationJob).where(
                GenerationJob.project_id == command.project_id,
                GenerationJob.idempotency_key == command.idempotency_key
            )
        )
        if existing is not None:
            if existing.request_fingerprint != request_fingerprint:
                raise Conflict("media job idempotency key has different parameters")
            return existing
        production = session.scalar(
            select(MediaProduction).where(MediaProduction.id == command.production_id)
        )
        if production is None or production.project_id != command.project_id:
            raise NotFound("media production not found")
        shot = None
        if command.shot_id is not None:
            shot = session.scalar(select(Shot).where(Shot.id == command.shot_id))
            if (
                shot is None
                or shot.project_id != command.project_id
                or shot.production_id != command.production_id
            ):
                raise NotFound("media shot not found")

        provider_paid = command.capability not in {
            Capability.HUMAN_PACKAGE,
            Capability.FINISH,
            Capability.QC,
        }
        quote = command.quote
        job_id = uuid4()
        task = self._submit_task(session, actor, command, job_id)
        job = GenerationJob(
            id=job_id,
            task_record_id=task.id,
            account_id=production.account_id,
            project_id=command.project_id,
            production_id=production.id,
            shot_id=shot.id if shot is not None else None,
            capability=command.capability.value,
            provider=quote.provider,
            model_id=quote.model_id,
            model_version=quote.model_version,
            billable=provider_paid,
            provider_status=JobStatus.HELD.value,
            idempotency_key=command.idempotency_key,
            request_fingerprint=request_fingerprint,
            request_payload={
                **command.request_payload,
                "_billing": {
                    "credit_units": quote.credit_units,
                    "amount_fen": quote.amount_fen,
                    "quote_version": quote.quote_version,
                },
            },
            initiated_by_actor_id=actor.actor_id,
        )
        session.add(job)
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="media.job.created",
            target_type="generation_job",
            target_id=job.id,
            project_id=job.project_id,
            metadata={"capability": job.capability, "provider_paid": provider_paid},
        )
        return job


    @staticmethod
    def _job_request_fingerprint(
        actor: ActorContext, command: JobCreateCommand,
    ) -> str:
        route = command.billing_route
        canonical = {
            "actor_account_id": str(actor.account_id),
            "project_id": str(command.project_id),
            "production_id": str(command.production_id),
            "shot_id": (
                str(command.shot_id) if command.shot_id is not None else None
            ),
            "capability": command.capability.value,
            "provider": command.quote.provider,
            "model_id": command.quote.model_id,
            "model_version": command.quote.model_version,
            "credit_units": command.quote.credit_units,
            "amount_fen": command.quote.amount_fen,
            "quote_version": command.quote.quote_version,
            "billing_mode": route.mode.value,
            "billing_account_id": (
                str(route.account_id) if route.account_id is not None else None
            ),
            "cost_center_id": (
                str(route.cost_center_id)
                if route.cost_center_id is not None else None
            ),
            "approved_credit_units": command.approved_credit_units,
            "approved_amount_fen": command.approved_amount_fen,
            "request_payload": command.request_payload,
        }
        encoded = json.dumps(
            canonical, ensure_ascii=False, allow_nan=False,
            separators=(",", ":"), sort_keys=True,
        ).encode("utf-8")
        return sha256(encoded).hexdigest()

    def _submit_task(
        self,
        session: Session,
        actor: ActorContext,
        command: JobCreateCommand,
        job_id: UUID,
    ) -> TaskRecord:
        quote = command.quote
        route = command.billing_route
        input_payload = {
            **command.request_payload,
            "media_job_id": str(job_id),
            "production_id": str(command.production_id),
            "shot_id": str(command.shot_id) if command.shot_id is not None else None,
            "provider": quote.provider,
            "model_id": quote.model_id,
            "model_version": quote.model_version,
            "quote_version": quote.quote_version,
        }
        if route.mode is BillingMode.CUSTOMER_CREDIT:
            if command.approved_credit_units != quote.credit_units or command.approved_amount_fen is not None:
                raise Conflict("customer approval does not match the frozen credit quote")
            return self.task_submission.submit_customer(
                session,
                actor,
                command.project_id,
                command.capability.value,
                quote.credit_units,
                command.idempotency_key,
                input_payload,
            )
        if command.approved_amount_fen != quote.amount_fen or command.approved_credit_units is not None:
            raise Conflict("internal approval does not match the frozen CNY quote")
        if route.cost_center_id is None:
            raise Conflict("internal billing route has no cost center")
        return self.task_submission.submit_internal(
            session,
            actor,
            command.project_id,
            command.capability.value,
            route.cost_center_id,
            quote.amount_fen,
            command.idempotency_key,
            input_payload,
        )
```

Save this as `backend/src/ip_saas/modules/media/service.py`.

- [ ] **Step 5: Run the unit service test**

Run: `cd backend && uv run pytest tests/unit/media/test_job_service.py -v`

Expected: 1 passed.

- [ ] **Step 6: Add the rollback integration test**

```python
import pytest
from sqlalchemy import select

from ip_saas.common.errors import Conflict
from ip_saas.modules.media.models.jobs import GenerationJob
from ip_saas.modules.tasks.models import TaskRecord


def test_hold_job_and_outbox_roll_back_together(
    db_session,
) -> None:
    from tests.integration.media.harness import MediaIntegrationHarness

    case = MediaIntegrationHarness(db_session).job_transaction_case()
    customer_media_command = case.customer_media_command
    actor = case.actor
    media_job_service = case.media_job_service
    failing_task_submission = case.failing_task_submission
    media_job_service.task_submission = failing_task_submission
    with pytest.raises(RuntimeError, match="task outbox unavailable"):
        with db_session.begin():
            media_job_service.create(db_session, actor, customer_media_command)
    assert db_session.scalar(
        select(GenerationJob).where(
            GenerationJob.idempotency_key == customer_media_command.idempotency_key
        )
    ) is None
    assert db_session.scalar(
        select(TaskRecord).where(
            TaskRecord.idempotency_key == customer_media_command.idempotency_key
        )
    ) is None


def test_two_accounts_may_reuse_the_same_client_idempotency_key(db_session) -> None:
    from tests.integration.media.harness import MediaIntegrationHarness

    case = MediaIntegrationHarness(db_session).two_account_job_case(
        "client-retry-key"
    )
    first = case.first_service.create(
        db_session, case.first_actor, case.first_command
    )
    second = case.second_service.create(
        db_session, case.second_actor, case.second_command
    )
    assert first.project_id != second.project_id
    assert first.id != second.id
    assert first.task_record_id != second.task_record_id


@pytest.mark.parametrize(
    "change",
    ["request_payload", "quote", "billing_route", "shot_id", "capability"],
)
def test_same_project_same_key_with_different_parameters_conflicts(
    db_session, change
) -> None:
    from tests.integration.media.harness import MediaIntegrationHarness

    case = MediaIntegrationHarness(db_session).job_parameter_conflict_case(
        "same-project-key", change
    )
    case.service.create(db_session, case.actor, case.first_command)
    with pytest.raises(Conflict, match="different parameters"):
        case.service.create(db_session, case.actor, case.changed_command)
```

Add this test to `backend/tests/integration/media/test_job_hold_transaction.py`; its fixtures use the Plan 01 `TaskSubmissionService` with a failing `OutboxWriter` and deliberately raise after shared TaskRecord/hold persistence. The fixture teardown opens a fresh transaction before checking both tables, so the assertion proves database rollback rather than mock reset. Plan 01's TaskRecord integration test independently asserts that an absent TaskRecord also means its hold and outbox rows rolled back.

- [ ] **Step 7: Run the transaction test twice to verify idempotent fixture cleanup**

Run: `cd backend && uv run pytest tests/integration/media/test_job_hold_transaction.py -v --count=2`

Expected: all cases pass twice; rollback leaves no hold/job/outbox row, two accounts may reuse a client key without collision, and same-project parameter drift conflicts.

- [ ] **Step 8: Commit the atomic hold/job/outbox boundary**

```bash
git add backend/src/ip_saas/modules/media/pricing.py backend/src/ip_saas/modules/media/service.py backend/tests/unit/media/test_job_service.py backend/tests/integration/media/test_job_hold_transaction.py
git commit -m "feat: create held media jobs transactionally"
```

### Task 11: Copy every temporary result into private TOS before QC

**Files:**
- Modify: `backend/src/ip_saas/providers/media/temporary.py`
- Create: `backend/src/ip_saas/providers/media/tos.py`
- Create: `backend/src/ip_saas/workers/media_copy.py`
- Test: `backend/tests/unit/media/test_temporary_copy.py`
- Test: `backend/tests/integration/media/test_copy_worker.py`

- [ ] **Step 1: Write failing tests for expiry, checksum, and URL removal**

```python
from datetime import UTC, datetime, timedelta
from pathlib import Path
from uuid import uuid4

import pytest

from ip_saas.modules.media.enums import AiDisclosure, AssetKind
from ip_saas.providers.media.temporary import AesGcmUrlCipher, MemoryObjectFetcher
from ip_saas.providers.media.tos import MemoryPrivateStorage
from ip_saas.workers.media_copy import CopyCommand, copy_provider_result


class FixedClock:
    def __init__(self) -> None:
        self.value = datetime(2026, 8, 24, tzinfo=UTC)

    def now(self) -> datetime:
        return self.value


def test_copy_hashes_to_private_storage_and_drops_the_provider_url(tmp_path: Path) -> None:
    clock = FixedClock()
    cipher = AesGcmUrlCipher(b"k" * 32)
    url = "https://provider.example/result.mp4"
    fetcher = MemoryObjectFetcher({url: (b"valid-fake-video", "video/mp4")})
    storage = MemoryPrivateStorage("private-media")
    command = CopyCommand(
        job_id=uuid4(),
        account_id=uuid4(),
        project_id=uuid4(),
        production_id=uuid4(),
        shot_id=uuid4(),
        kind=AssetKind.LOW_RES_PREVIEW,
        ai_disclosure=AiDisclosure.GENERATED,
        temporary_url_ciphertext=cipher.seal(url, b"job-1"),
        url_aad=b"job-1",
        expires_at=clock.now() + timedelta(hours=1),
        object_key="accounts/a/projects/p/jobs/j/result.mp4",
        provenance={"provider": "seedance", "model_id": "ep-pinned"},
    )
    result = copy_provider_result(
        command,
        cipher,
        fetcher,
        storage,
        clock,
        tmp_path,
        max_bytes=1024,
    )
    assert result.stored.bucket == "private-media"
    assert result.stored.sha256 == "2896f6f5a3b5d47e003677d6ab3f342108b8a7c17a690fa924a82266c377057e"
    assert result.clear_temporary_url is True
    assert storage.read(result.stored.object_key) == b"valid-fake-video"


def test_copy_refuses_a_url_inside_the_safety_margin(tmp_path: Path) -> None:
    clock = FixedClock()
    cipher = AesGcmUrlCipher(b"k" * 32)
    command = CopyCommand(
        job_id=uuid4(),
        account_id=uuid4(),
        project_id=uuid4(),
        production_id=uuid4(),
        shot_id=None,
        kind=AssetKind.GENERATED_AUDIO,
        ai_disclosure=AiDisclosure.GENERATED,
        temporary_url_ciphertext=cipher.seal("https://provider.example/a.mp3", b"job-2"),
        url_aad=b"job-2",
        expires_at=clock.now() + timedelta(minutes=4),
        object_key="safe/audio.mp3",
        provenance={},
    )
    with pytest.raises(TimeoutError, match="safety margin"):
        copy_provider_result(
            command,
            cipher,
            MemoryObjectFetcher({}),
            MemoryPrivateStorage("private-media"),
            clock,
            tmp_path,
            max_bytes=1024,
        )
```

- [ ] **Step 2: Run the tests and verify copy primitives are missing**

Run: `cd backend && uv run pytest tests/unit/media/test_temporary_copy.py -v`

Expected: FAIL during collection because `AesGcmUrlCipher`, storage, and copy worker do not exist.

- [ ] **Step 3: Add encrypted URL handling and bounded download ports**

Append this complete block to `backend/src/ip_saas/providers/media/temporary.py`:

```python
from dataclasses import dataclass
from os import urandom
from pathlib import Path
from typing import Protocol

import httpx
from cryptography.hazmat.primitives.ciphers.aead import AESGCM


class UrlCipher(Protocol):
    def seal(self, url: str, aad: bytes) -> bytes:
        raise NotImplementedError

    def open(self, ciphertext: bytes, aad: bytes) -> str:
        raise NotImplementedError


class AesGcmUrlCipher:
    def __init__(self, key: bytes) -> None:
        if len(key) != 32:
            raise ValueError("media URL encryption key must be exactly 32 bytes")
        self._aes = AESGCM(key)

    def seal(self, url: str, aad: bytes) -> bytes:
        nonce = urandom(12)
        return nonce + self._aes.encrypt(nonce, url.encode(), aad)

    def open(self, ciphertext: bytes, aad: bytes) -> str:
        if len(ciphertext) < 29:
            raise ValueError("invalid encrypted media URL")
        return self._aes.decrypt(ciphertext[:12], ciphertext[12:], aad).decode()


@dataclass(frozen=True)
class DownloadedObject:
    content_type: str
    size_bytes: int
    sha256: str


class TemporaryObjectFetcher(Protocol):
    def download(self, url: str, destination: Path, max_bytes: int) -> DownloadedObject:
        raise NotImplementedError


class HttpTemporaryObjectFetcher:
    def __init__(self, client: httpx.Client, allowed_hosts: frozenset[str]) -> None:
        self.client = client
        self.allowed_hosts = allowed_hosts

    def download(self, url: str, destination: Path, max_bytes: int) -> DownloadedObject:
        from hashlib import sha256
        from urllib.parse import urlparse

        parsed = urlparse(url)
        if parsed.scheme != "https" or parsed.hostname not in self.allowed_hosts:
            raise ValueError("provider result URL host is not allow-listed")
        digest = sha256()
        size = 0
        with self.client.stream("GET", url, follow_redirects=False, timeout=120) as response:
            response.raise_for_status()
            content_type = response.headers.get("content-type", "application/octet-stream").split(";", 1)[0]
            with destination.open("xb") as output:
                for chunk in response.iter_bytes(1024 * 1024):
                    size += len(chunk)
                    if size > max_bytes:
                        raise ValueError("provider result exceeds configured byte limit")
                    digest.update(chunk)
                    output.write(chunk)
        if size == 0:
            raise ValueError("provider returned an empty object")
        return DownloadedObject(content_type, size, digest.hexdigest())


class MemoryObjectFetcher:
    def __init__(self, objects: dict[str, tuple[bytes, str]]) -> None:
        self.objects = objects

    def download(self, url: str, destination: Path, max_bytes: int) -> DownloadedObject:
        from hashlib import sha256

        body, content_type = self.objects[url]
        if not body or len(body) > max_bytes:
            raise ValueError("invalid fake provider object size")
        destination.write_bytes(body)
        return DownloadedObject(content_type, len(body), sha256(body).hexdigest())
```

- [ ] **Step 4: Implement private-storage ports and the TOS adapter**

```python
from dataclasses import dataclass
from pathlib import Path
from typing import Protocol


@dataclass(frozen=True)
class StoredPrivateObject:
    bucket: str
    object_key: str
    version_id: str | None
    size_bytes: int
    sha256: str
    content_type: str


class PrivateStorage(Protocol):
    def put_file(
        self,
        object_key: str,
        path: Path,
        content_type: str,
        sha256: str,
    ) -> StoredPrivateObject:
        raise NotImplementedError

    def sign_get(self, object_key: str, expires_seconds: int) -> str:
        raise NotImplementedError


class MemoryPrivateStorage:
    def __init__(self, bucket: str) -> None:
        self.bucket = bucket
        self._objects: dict[str, bytes] = {}

    def put_file(
        self,
        object_key: str,
        path: Path,
        content_type: str,
        sha256: str,
    ) -> StoredPrivateObject:
        body = path.read_bytes()
        self._objects[object_key] = body
        return StoredPrivateObject(
            self.bucket,
            object_key,
            "memory-v1",
            len(body),
            sha256,
            content_type,
        )

    def sign_get(self, object_key: str, expires_seconds: int) -> str:
        if object_key not in self._objects:
            raise KeyError(object_key)
        return f"memory://private/{object_key}?expires={expires_seconds}"

    def read(self, object_key: str) -> bytes:
        return self._objects[object_key]


class TosPrivateStorage:
    def __init__(self, client, bucket: str) -> None:
        self.client = client
        self.bucket = bucket

    def put_file(
        self,
        object_key: str,
        path: Path,
        content_type: str,
        sha256: str,
    ) -> StoredPrivateObject:
        with path.open("rb") as source:
            response = self.client.put_object(
                self.bucket,
                object_key,
                content=source,
                content_type=content_type,
                meta={"sha256": sha256},
            )
        return StoredPrivateObject(
            self.bucket,
            object_key,
            getattr(response, "version_id", None),
            path.stat().st_size,
            sha256,
            content_type,
        )

    def sign_get(self, object_key: str, expires_seconds: int) -> str:
        import tos

        result = self.client.pre_signed_url(
            tos.HttpMethodType.Http_Method_Get,
            self.bucket,
            object_key,
            expires=expires_seconds,
        )
        return str(result.signed_url)
```

Save this as `backend/src/ip_saas/providers/media/tos.py`. Production buckets are private with versioning enabled. API responses contain only an authorized short-lived `sign_get` URL, never `temporary_url_ciphertext` or the provider URL.

- [ ] **Step 5: Implement the pure copy operation and synchronous DB worker**

```python
from dataclasses import dataclass
from datetime import datetime, timedelta
from pathlib import Path
from tempfile import NamedTemporaryFile
from uuid import UUID, uuid4

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.clock import Clock
from ip_saas.common.errors import Conflict, NotFound
from ip_saas.common.outbox import EventEnvelope, OutboxWriter
from ip_saas.db.session import session_scope
from ip_saas.modules.media.enums import AiDisclosure, AssetKind, JobStatus
from ip_saas.modules.media.models.jobs import GenerationJob, MediaAsset
from ip_saas.providers.media.temporary import TemporaryObjectFetcher, UrlCipher
from ip_saas.providers.media.tos import PrivateStorage, StoredPrivateObject


@dataclass(frozen=True)
class CopyCommand:
    job_id: UUID
    account_id: UUID
    project_id: UUID
    production_id: UUID
    shot_id: UUID | None
    kind: AssetKind
    ai_disclosure: AiDisclosure
    temporary_url_ciphertext: bytes
    url_aad: bytes
    expires_at: datetime
    object_key: str
    provenance: dict[str, object]


@dataclass(frozen=True)
class CopyResult:
    stored: StoredPrivateObject
    clear_temporary_url: bool


def copy_provider_result(
    command: CopyCommand,
    cipher: UrlCipher,
    fetcher: TemporaryObjectFetcher,
    storage: PrivateStorage,
    clock: Clock,
    temp_root: Path,
    *,
    max_bytes: int,
) -> CopyResult:
    if clock.now() + timedelta(minutes=5) >= command.expires_at:
        raise TimeoutError("provider result URL is inside the five-minute safety margin")
    url = cipher.open(command.temporary_url_ciphertext, command.url_aad)
    with NamedTemporaryFile(dir=temp_root, prefix="media-copy-", delete=False) as handle:
        path = Path(handle.name)
    path.unlink()
    try:
        downloaded = fetcher.download(url, path, max_bytes)
        stored = storage.put_file(
            command.object_key,
            path,
            downloaded.content_type,
            downloaded.sha256,
        )
        return CopyResult(stored=stored, clear_temporary_url=True)
    finally:
        path.unlink(missing_ok=True)


class MediaCopyService:
    def __init__(
        self,
        cipher: UrlCipher,
        fetcher: TemporaryObjectFetcher,
        storage: PrivateStorage,
        outbox: OutboxWriter,
        clock: Clock,
        temp_root: Path,
        max_bytes: int,
    ) -> None:
        self.cipher = cipher
        self.fetcher = fetcher
        self.storage = storage
        self.outbox = outbox
        self.clock = clock
        self.temp_root = temp_root
        self.max_bytes = max_bytes

    def run(self, job_id: UUID) -> UUID:
        with session_scope() as session:
            return self._run_in_session(session, job_id)

    def _run_in_session(self, session: Session, job_id: UUID) -> UUID:
        job = session.scalar(
            select(GenerationJob).where(GenerationJob.id == job_id).with_for_update()
        )
        if job is None:
            raise NotFound("media job not found")
        existing = session.scalar(select(MediaAsset).where(MediaAsset.job_id == job.id))
        if existing is not None:
            return existing.id
        if job.provider_status != JobStatus.SUCCEEDED_PENDING_COPY.value:
            raise Conflict("media job is not ready for private copy")
        if job.temporary_url_ciphertext is None or job.temporary_url_expires_at is None:
            raise Conflict("successful media job has no protected provider result")
        aad = str(job.id).encode()
        result = copy_provider_result(
            CopyCommand(
                job_id=job.id,
                account_id=job.account_id,
                project_id=job.project_id,
                production_id=job.production_id,
                shot_id=job.shot_id,
                kind=_asset_kind(job.capability),
                ai_disclosure=AiDisclosure.GENERATED,
                temporary_url_ciphertext=job.temporary_url_ciphertext,
                url_aad=aad,
                expires_at=job.temporary_url_expires_at,
                object_key=_object_key(job),
                provenance={
                    "provider": job.provider,
                    "model_id": job.model_id,
                    "model_version": job.model_version,
                    "request_sha256": job.provider_payload_sha256,
                },
            ),
            self.cipher,
            self.fetcher,
            self.storage,
            self.clock,
            self.temp_root,
            max_bytes=self.max_bytes,
        )
        asset = MediaAsset(
            account_id=job.account_id,
            project_id=job.project_id,
            production_id=job.production_id,
            shot_id=job.shot_id,
            job_id=job.id,
            kind=_asset_kind(job.capability).value,
            tos_bucket=result.stored.bucket,
            tos_object_key=result.stored.object_key,
            tos_version_id=result.stored.version_id,
            sha256=result.stored.sha256,
            content_type=result.stored.content_type,
            size_bytes=result.stored.size_bytes,
            ai_disclosure=AiDisclosure.GENERATED.value,
            provenance={
                "provider": job.provider,
                "model_id": job.model_id,
                "model_version": job.model_version,
            },
            label_evidence={},
        )
        session.add(asset)
        session.flush()
        job.temporary_url_ciphertext = None
        job.temporary_url_expires_at = None
        job.provider_status = JobStatus.COPIED_PENDING_QC.value
        self.outbox.add(
            session,
            EventEnvelope(
                event_id=uuid4(),
                event_type="media.qc.requested",
                schema_version=1,
                aggregate_id=job.id,
                occurred_at=self.clock.now(),
                initiated_by_actor_id=job.initiated_by_actor_id,
                idempotency_key=f"media:job:{job.id}:qc",
                payload={"job_id": str(job.id), "asset_id": str(asset.id)},
            ),
        )
        return asset.id


def _asset_kind(capability: str) -> AssetKind:
    return {
        "storyboard_image": AssetKind.STORYBOARD,
        "low_res_video": AssetKind.LOW_RES_PREVIEW,
        "high_res_video": AssetKind.GENERATED_VIDEO,
        "standard_tts": AssetKind.GENERATED_AUDIO,
    }[capability]


def _object_key(job: GenerationJob) -> str:
    suffix = {
        "storyboard_image": "png",
        "low_res_video": "mp4",
        "high_res_video": "mp4",
        "standard_tts": "mp3",
    }[job.capability]
    return (
        f"accounts/{job.account_id}/projects/{job.project_id}/"
        f"productions/{job.production_id}/jobs/{job.id}/result.{suffix}"
    )
```

Save this as `backend/src/ip_saas/workers/media_copy.py`.

- [ ] **Step 6: Correct the checksum fixture and run the copy unit tests**

Run: `cd backend && uv run python -c "import hashlib; print(hashlib.sha256(b'valid-fake-video').hexdigest())"`

Expected: `2896f6f5a3b5d47e003677d6ab3f342108b8a7c17a690fa924a82266c377057e`.

Run: `cd backend && uv run pytest tests/unit/media/test_temporary_copy.py -v`

Expected: 2 passed.

- [ ] **Step 7: Test duplicate copy delivery against PostgreSQL and memory storage**

Add `backend/tests/integration/media/test_copy_worker.py` with one seeded `succeeded_pending_copy` job and invoke `MediaCopyService._run_in_session` twice. Assert both calls return the same `MediaAsset.id`, exactly one asset row exists, `temporary_url_ciphertext IS NULL`, the job is `copied_pending_qc`, and exactly one `media.qc.requested` outbox row exists. Task 15 wraps this pure persistence service in the lease-aware broker Worker; do not register `MediaCopyService` directly as a consumer.

Run: `cd backend && uv run pytest tests/integration/media/test_copy_worker.py -v`

Expected: 1 passed.

- [ ] **Step 8: Commit encrypted temporary-result copying**

```bash
git add backend/src/ip_saas/providers/media/temporary.py backend/src/ip_saas/providers/media/tos.py backend/src/ip_saas/workers/media_copy.py backend/tests/unit/media/test_temporary_copy.py backend/tests/integration/media/test_copy_worker.py
git commit -m "feat: copy temporary media into private tos"
```

### Task 12: Submit and poll paid providers with synchronous, replay-safe workers

**Files:**
- Create: `backend/src/ip_saas/workers/media_submit.py`
- Create: `backend/src/ip_saas/workers/media_poll.py`
- Test: `backend/tests/unit/media/test_provider_worker_state.py`
- Test: `backend/tests/integration/media/test_provider_worker_idempotency.py`

- [ ] **Step 1: Write failing phase-order tests**

```python
import pytest

from ip_saas.common.errors import Conflict
from ip_saas.modules.media.enums import JobStatus, ProviderPhase
from ip_saas.workers.media_submit import target_status


def test_provider_phase_maps_to_media_substate() -> None:
    assert target_status(ProviderPhase.QUEUED) is JobStatus.QUEUED
    assert target_status(ProviderPhase.RUNNING) is JobStatus.RUNNING
    assert target_status(ProviderPhase.SUCCEEDED) is JobStatus.SUCCEEDED_PENDING_COPY
    assert target_status(ProviderPhase.FAILED) is JobStatus.FAILED


def test_out_of_order_provider_phase_is_ignored_by_rank() -> None:
    assert target_status(ProviderPhase.QUEUED, current_rank=20) is None
    assert target_status(ProviderPhase.RUNNING, current_rank=20) is JobStatus.RUNNING


def test_success_without_artifact_is_rejected() -> None:
    with pytest.raises(Conflict, match="artifact"):
        target_status(ProviderPhase.SUCCEEDED, has_artifact=False)
```

- [ ] **Step 2: Run the tests and verify the worker module is missing**

Run: `cd backend && uv run pytest tests/unit/media/test_provider_worker_state.py -v`

Expected: FAIL during collection because `ip_saas.workers.media_submit` does not exist.

- [ ] **Step 3: Implement the complete submit worker and normalized result application**

```python
from dataclasses import dataclass
from datetime import timedelta
from enum import StrEnum
from hashlib import sha256
from json import dumps
from uuid import UUID, uuid4

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.clock import Clock
from ip_saas.common.errors import Conflict, NotFound
from ip_saas.common.outbox import EventEnvelope, OutboxWriter
from ip_saas.common.tasking import TaskStatus
from ip_saas.db.session import session_scope
from ip_saas.modules.billing.service import BillingContext, BillingMode, BillingService
from ip_saas.modules.media.enums import Capability, JobStatus, ProviderPhase
from ip_saas.modules.media.models.jobs import GenerationJob
from ip_saas.modules.tasks.models import TaskRecord
from ip_saas.modules.tasks.service import TaskSubmissionService
from ip_saas.providers.media.base import (
    MediaProvider,
    ProviderRequest,
    ProviderResult,
)
from ip_saas.providers.media.temporary import UrlCipher


_PHASE_RANK = {
    ProviderPhase.QUEUED: 10,
    ProviderPhase.RUNNING: 20,
    ProviderPhase.SUCCEEDED: 30,
    ProviderPhase.FAILED: 30,
}


@dataclass(frozen=True)
class ProviderJobSnapshot:
    job_id: UUID
    task_record_id: UUID
    attempt_no: int
    provider: str
    provider_task_id: str | None
    request: ProviderRequest


class ProcessDisposition(StrEnum):
    ACK = "ack"
    RETRY = "retry"


def target_status(
    phase: ProviderPhase,
    *,
    current_rank: int = 0,
    has_artifact: bool = True,
) -> JobStatus | None:
    rank = _PHASE_RANK[phase]
    if rank < current_rank:
        return None
    if phase is ProviderPhase.SUCCEEDED and not has_artifact:
        raise Conflict("successful provider result has no artifact")
    return {
        ProviderPhase.QUEUED: JobStatus.QUEUED,
        ProviderPhase.RUNNING: JobStatus.RUNNING,
        ProviderPhase.SUCCEEDED: JobStatus.SUCCEEDED_PENDING_COPY,
        ProviderPhase.FAILED: JobStatus.FAILED,
    }[phase]


class MediaProviderRegistry:
    def __init__(self, providers: dict[str, MediaProvider]) -> None:
        self.providers = dict(providers)

    def get(self, provider: str) -> MediaProvider:
        try:
            return self.providers[provider]
        except KeyError as error:
            raise NotFound(f"media provider is not configured: {provider}") from error


class MediaSubmitWorker:
    def __init__(
        self,
        providers: MediaProviderRegistry,
        task_submission: TaskSubmissionService,
        billing: BillingService,
        outbox: OutboxWriter,
        cipher: UrlCipher,
        clock: Clock,
    ) -> None:
        self.providers = providers
        self.task_submission = task_submission
        self.billing = billing
        self.outbox = outbox
        self.cipher = cipher
        self.clock = clock

    def run(self, job_id: UUID) -> ProcessDisposition:
        disposition, snapshot = self._claim(job_id)
        if snapshot is None:
            return disposition
        provider = self.providers.get(snapshot.provider)
        try:
            result = provider.submit(snapshot.request)
        except Exception as error:
            self._record_transport_failure(snapshot, error)
            return ProcessDisposition.RETRY
        return self._apply(snapshot, result)

    def _claim(
        self, job_id: UUID,
    ) -> tuple[ProcessDisposition, ProviderJobSnapshot | None]:
        with session_scope() as session:
            job = _locked_job(session, job_id)
            if not job.billable:
                raise Conflict("local media jobs do not enter the provider submit worker")
            try:
                claimed = self.task_submission.start(
                    session, job.task_record_id, lease_seconds=300, max_attempts=8
                )
            except Conflict:
                return ProcessDisposition.RETRY, None
            if claimed.status == TaskStatus.RECONCILIATION_REQUIRED:
                return ProcessDisposition.RETRY, None
            if claimed.status in {
                TaskStatus.SUCCEEDED, TaskStatus.FAILED, TaskStatus.CANCELLED,
            }:
                if (
                    claimed.status in {TaskStatus.FAILED, TaskStatus.CANCELLED}
                    and job.provider_status not in {
                        JobStatus.PASSED.value, JobStatus.REDO.value, JobStatus.FAILED.value,
                    }
                ):
                    job.provider_status = JobStatus.FAILED.value
                    job.last_error_code = claimed.error_code or "task_cancelled"
                    job.last_error_message = (
                        claimed.error_message or "media task ended before delivery"
                    )[:500]
                return ProcessDisposition.ACK, None
            if job.provider_task_id is not None or job.provider_status in {
                JobStatus.QUEUED.value, JobStatus.RUNNING.value,
                JobStatus.SUCCEEDED_PENDING_COPY.value,
                JobStatus.COPIED_PENDING_QC.value,
            }:
                return ProcessDisposition.ACK, None
            job.provider_status = JobStatus.SUBMITTED.value
            job.attempt_count += 1
            return ProcessDisposition.ACK, _snapshot(job, claimed.attempt_no)

    def _apply(
        self, snapshot: ProviderJobSnapshot, result: ProviderResult,
    ) -> ProcessDisposition:
        with session_scope() as session:
            try:
                self.task_submission.heartbeat(
                    session, snapshot.task_record_id, snapshot.attempt_no,
                    lease_seconds=300,
                )
            except Conflict:
                return ProcessDisposition.ACK
            job = _locked_job(session, snapshot.job_id)
            task = _locked_task(session, snapshot.task_record_id)
            if (task.status != TaskStatus.RUNNING
                    or task.attempt_no != snapshot.attempt_no):
                return ProcessDisposition.ACK
            new_status = target_status(
                result.phase,
                current_rank=job.provider_phase_rank,
                has_artifact=result.artifact is not None,
            )
            if new_status is None:
                return ProcessDisposition.ACK
            if result.provider_task_id is not None:
                if job.provider_task_id not in {None, result.provider_task_id}:
                    raise Conflict("provider task id changed for an existing media job")
                job.provider_task_id = result.provider_task_id
            job.provider_request_id = result.provider_request_id
            job.provider_phase_rank = _PHASE_RANK[result.phase]
            job.provider_status = new_status.value
            if result.usage is not None:
                job.native_quantity = result.usage.native_quantity
                job.native_unit = result.usage.native_unit
                job.supplier_amount_minor = result.usage.supplier_amount_minor
                job.supplier_currency = result.usage.supplier_currency
                job.provider_cost_amount_fen = result.usage.amount_fen
                job.reconciliation_status = result.usage.reconciliation_status.value
            normalized = {
                "phase": result.phase.value,
                "provider_task_id": result.provider_task_id,
                "provider_request_id": result.provider_request_id,
                "model_id": result.model_id,
                "model_version": result.model_version,
            }
            job.provider_payload_sha256 = sha256(
                dumps(normalized, sort_keys=True).encode()
            ).hexdigest()
            if result.phase is ProviderPhase.SUCCEEDED:
                artifact = result.artifact
                if artifact is None:
                    raise Conflict("successful provider result has no artifact")
                if result.usage is None:
                    self._defer_missing_usage(
                        session, job, snapshot.attempt_no,
                        "provider succeeded without auditable terminal usage",
                    )
                    return ProcessDisposition.RETRY
                job.temporary_url_ciphertext = self.cipher.seal(
                    artifact.temporary_url,
                    str(job.id).encode(),
                )
                job.temporary_url_expires_at = artifact.expires_at
                self._emit(
                    session, job, snapshot.attempt_no,
                    "media.copy.requested", "copy",
                )
            elif result.phase in {ProviderPhase.QUEUED, ProviderPhase.RUNNING}:
                delay = result.retry_after_seconds or 5
                job.next_attempt_at = self.clock.now() + timedelta(seconds=delay)
                self._emit(
                    session, job, snapshot.attempt_no,
                    "media.job.poll.requested", f"poll:{job.poll_count + 1}",
                )
            else:
                job.last_error_code = result.error_code or "provider_failed"
                job.last_error_message = "provider generation failed"
                if result.usage is None:
                    self._defer_missing_usage(
                        session, job, snapshot.attempt_no,
                        "provider terminal response has no auditable usage",
                    )
                    return ProcessDisposition.RETRY
                self._fail_locked(
                    session, job, task, snapshot.attempt_no,
                    job.last_error_code, job.last_error_message,
                )
            return ProcessDisposition.ACK

    def _defer_missing_usage(
        self,
        session: Session,
        job: GenerationJob,
        attempt_no: int,
        message: str,
    ) -> None:
        job.provider_status = JobStatus.RUNNING.value
        job.last_error_code = "provider_usage_missing"
        job.last_error_message = message[:500]
        job.next_attempt_at = self.clock.now() + timedelta(seconds=30)
        self._emit(
            session, job, attempt_no,
            "media.job.poll.requested", "usage-recovery",
        )

    def _record_transport_failure(
        self, snapshot: ProviderJobSnapshot, _error: Exception,
    ) -> None:
        with session_scope() as session:
            job = _locked_job(session, snapshot.job_id)
            task = _locked_task(session, snapshot.task_record_id)
            if (task.status != TaskStatus.RUNNING
                    or task.attempt_no != snapshot.attempt_no):
                return
            job.last_error_code = "provider_transport_error"
            job.last_error_message = "provider transport failed; retry scheduled"
            job.next_attempt_at = self.clock.now() + timedelta(seconds=30)

    def _fail_locked(
        self,
        session: Session,
        job: GenerationJob,
        task: TaskRecord,
        attempt_no: int,
        error_code: str,
        error_message: str,
    ) -> None:
        context = BillingContext(
            mode=BillingMode(task.billing_mode), hold_id=task.billing_hold_id,
        )
        self.billing.release_generation(
            session, context, error_code,
            f"release:media:{task.id}:{error_code}",
        )
        self.task_submission.fail(
            session, task.id, attempt_no, error_code, error_message,
        )
        job.provider_status = JobStatus.FAILED.value
        job.last_error_code = error_code
        job.last_error_message = error_message[:500]
        self._emit(
            session, job, attempt_no, "media.job.failed", "failed",
            {"error_code": error_code},
        )

    def _emit(
        self,
        session: Session,
        job: GenerationJob,
        attempt_no: int,
        event_type: str,
        suffix: str,
        extra_payload: dict[str, object] | None = None,
    ) -> None:
        self.outbox.add(
            session,
            EventEnvelope(
                event_id=uuid4(),
                event_type=event_type,
                schema_version=1,
                aggregate_id=job.id,
                occurred_at=self.clock.now(),
                initiated_by_actor_id=job.initiated_by_actor_id,
                idempotency_key=f"media:job:{job.id}:attempt:{attempt_no}:{suffix}",
                payload={
                    "job_id": str(job.id),
                    "task_id": str(job.task_record_id),
                    "attempt_no": attempt_no,
                    **(extra_payload or {}),
                },
            ),
        )


def _locked_job(session: Session, job_id: UUID) -> GenerationJob:
    job = session.scalar(
        select(GenerationJob).where(GenerationJob.id == job_id).with_for_update()
    )
    if job is None:
        raise NotFound("media job not found")
    return job


def _locked_task(session: Session, task_id: UUID) -> TaskRecord:
    task = session.scalar(
        select(TaskRecord).where(TaskRecord.id == task_id).with_for_update()
    )
    if task is None:
        raise NotFound("media task record not found")
    return task


def _snapshot(job: GenerationJob, attempt_no: int) -> ProviderJobSnapshot:
    return ProviderJobSnapshot(
        job_id=job.id,
        task_record_id=job.task_record_id,
        attempt_no=attempt_no,
        provider=job.provider,
        provider_task_id=job.provider_task_id,
        request=_provider_request(job),
    )


def _provider_request(job: GenerationJob) -> ProviderRequest:
    return ProviderRequest(
        idempotency_key=f"media-job:{job.id}",
        capability=Capability(job.capability),
        model_id=job.model_id,
        model_version=job.model_version,
        prompt=str(job.request_payload["prompt"]),
        reference_urls=tuple(job.request_payload.get("reference_urls", [])),
        parameters=dict(job.request_payload.get("parameters", {})),
    )


class MediaGenerationTaskHandler:
    PROVIDER_CAPABILITIES = {
        Capability.STORYBOARD_IMAGE.value,
        Capability.LOW_RES_VIDEO.value,
        Capability.HIGH_RES_VIDEO.value,
        Capability.STANDARD_TTS.value,
    }

    def __init__(self, worker: MediaSubmitWorker) -> None:
        self.worker = worker

    def __call__(self, task_id: UUID) -> None:
        with session_scope() as session:
            job = session.scalar(select(GenerationJob).where(
                GenerationJob.task_record_id == task_id
            ))
            if job is None:
                raise NotFound("media job for task not found")
            if job.capability not in self.PROVIDER_CAPABILITIES:
                raise Conflict("media task is not a paid provider capability")
            job_id = job.id
        disposition = self.worker.run(job_id)
        if disposition is ProcessDisposition.RETRY:
            raise RuntimeError("media provider submission requested retry")
```

Save this as `backend/src/ip_saas/workers/media_submit.py`. Register one `MediaGenerationTaskHandler` instance under `storyboard_image`, `low_res_video`, `high_res_video`, and `standard_tts` in Plan 01's `RocketMQTaskConsumer` handler map. The shared consumer validates the entire `GenerationTaskRequestedV1`, including `request_fingerprint`, before passing only `task_id`; the handler resolves `GenerationJob` by its unique `task_record_id`. `RETRY` raises so RocketMQ does not ACK, while a terminal/duplicate `ACK` returns normally.

- [ ] **Step 4: Implement polling as a separate synchronous worker turn**

```python
from uuid import UUID

from ip_saas.common.errors import Conflict
from ip_saas.db.session import session_scope
from ip_saas.modules.media.enums import JobStatus
from ip_saas.providers.media.base import ProviderPollRequest

from .media_submit import (
    MediaSubmitWorker, ProcessDisposition, ProviderJobSnapshot, _locked_job,
    _provider_request,
)


class MediaPollWorker:
    def __init__(self, submit_worker: MediaSubmitWorker) -> None:
        self.submit_worker = submit_worker

    def run(self, job_id: UUID, attempt_no: int) -> ProcessDisposition:
        with session_scope() as session:
            job = _locked_job(session, job_id)
            if job.provider_status not in {JobStatus.QUEUED.value, JobStatus.RUNNING.value}:
                return ProcessDisposition.ACK
            if job.provider_task_id is None:
                raise ValueError("pollable media job has no provider_task_id")
            try:
                self.submit_worker.task_submission.heartbeat(
                    session, job.task_record_id, attempt_no, lease_seconds=300
                )
            except Conflict:
                return ProcessDisposition.ACK
            provider = self.submit_worker.providers.get(job.provider)
            poll_request = ProviderPollRequest(
                job.provider_task_id,
                job.model_id,
                job.model_version,
            )
            job.poll_count += 1
            snapshot = ProviderJobSnapshot(
                job_id=job.id, task_record_id=job.task_record_id,
                attempt_no=attempt_no, provider=job.provider,
                provider_task_id=job.provider_task_id,
                request=_provider_request(job),
            )
        try:
            result = provider.poll(poll_request)
        except Exception as error:
            self.submit_worker._record_transport_failure(snapshot, error)
            return ProcessDisposition.RETRY
        return self.submit_worker._apply(snapshot, result)
```

Save this as `backend/src/ip_saas/workers/media_poll.py`. Submit and poll both use `_provider_request(job)` so the same frozen request shape is preserved. The scheduler emits a poll message with `job_id` and `attempt_no` when `next_attempt_at <= clock.now()`; it never sleeps inside a Worker or holds a database transaction open during HTTP. Task 15 replaces the provisional CAS-loss return with `disposition_after_lease_loss`: a poll owned by a strictly older attempt or an already-terminal task ACKs without writes, while a missing task or the same expired/reconciliation attempt RETRYs.

- [ ] **Step 5: Run phase-order unit tests**

Run: `cd backend && uv run pytest tests/unit/media/test_provider_worker_state.py -v`

Expected: 3 passed.

- [ ] **Step 6: Add and run duplicate/out-of-order integration scenarios**

In `backend/tests/integration/media/test_provider_worker_idempotency.py`, seed one held media job with the deterministic fake and deliver Plan 01's strict `GenerationTaskRequestedV1` envelope twice. Assert its payload contains the TaskRecord's 64-character `request_fingerprint`, route it to `MediaSubmitWorker` only when `input_payload.media_job_id` names this job, then deliver `running`, duplicate `running`, stale `queued`, and duplicate `succeeded` results. Assert one provider task ID, one encrypted temporary URL, one copy outbox event, one `TaskRecord.start` transition, and zero billing settlement calls before QC. Do not emit or consume a second `media.job.requested` event.

Run: `cd backend && env -u ARK_API_KEY uv run pytest tests/integration/media/test_provider_worker_idempotency.py -v`

Expected: all scenarios pass and `ScriptedFakeMediaProvider.submit_count == 1`.

- [ ] **Step 7: Commit synchronous provider submit and polling workers**

```bash
git add backend/src/ip_saas/workers/media_submit.py backend/src/ip_saas/workers/media_poll.py backend/tests/unit/media/test_provider_worker_state.py backend/tests/integration/media/test_provider_worker_idempotency.py
git commit -m "feat: run replay-safe media provider workers"
```

### Task 13: Gate high resolution behind preview approval and redo one shot at a time

**Files:**
- Modify: `backend/src/ip_saas/modules/media/service.py`
- Test: `backend/tests/unit/media/test_preview_redo.py`
- Test: `backend/tests/integration/media/test_per_shot_redo.py`

- [ ] **Step 1: Write failing tests for preview approval and redo funding**

```python
from types import SimpleNamespace
from uuid import uuid4

import pytest

from ip_saas.common.errors import Conflict
from ip_saas.modules.billing.service import BillingMode
from ip_saas.modules.media.enums import RedoOrigin, ShotSource
from ip_saas.modules.media.pricing import BillingRoute
from ip_saas.modules.media.service import choose_redo_funding, require_preview_assets


def test_high_resolution_requires_every_ai_shot_preview() -> None:
    shots = [
        SimpleNamespace(id=uuid4(), source=ShotSource.SEEDANCE.value),
        SimpleNamespace(id=uuid4(), source=ShotSource.HUMAN_CAPTURE.value),
    ]
    with pytest.raises(Conflict, match="preview"):
        require_preview_assets(shots, selected_asset_by_shot={})
    require_preview_assets(shots, selected_asset_by_shot={shots[0].id: uuid4()})


def test_user_redo_uses_a_new_customer_hold() -> None:
    customer_route = BillingRoute.customer(uuid4())
    selected = choose_redo_funding(
        RedoOrigin.USER,
        customer_route,
        recovery_cost_center_id=uuid4(),
    )
    assert selected.mode is BillingMode.CUSTOMER_CREDIT


def test_system_redo_moves_cost_to_internal_budget() -> None:
    cost_center_id = uuid4()
    selected = choose_redo_funding(
        RedoOrigin.SYSTEM,
        BillingRoute.customer(uuid4()),
        recovery_cost_center_id=cost_center_id,
    )
    assert selected == BillingRoute.internal(cost_center_id)
```

- [ ] **Step 2: Run the tests and verify the review functions are missing**

Run: `cd backend && uv run pytest tests/unit/media/test_preview_redo.py -v`

Expected: FAIL during collection because the review functions are absent.

- [ ] **Step 3: Append the complete preview and redo policy to `service.py`**

```python
from collections.abc import Iterable

from ip_saas.common.clock import Clock

from .enums import ProductionStatus, RedoOrigin, ShotSource


def require_preview_assets(
    shots: Iterable[Shot],
    selected_asset_by_shot: dict[UUID, UUID],
) -> None:
    missing = [
        shot.id
        for shot in shots
        if shot.source in {ShotSource.SEEDREAM.value, ShotSource.SEEDANCE.value}
        and shot.id not in selected_asset_by_shot
    ]
    if missing:
        raise Conflict(f"AI shots have no selected low-resolution preview: {missing}")


def choose_redo_funding(
    origin: RedoOrigin,
    project_route: BillingRoute,
    *,
    recovery_cost_center_id: UUID,
) -> BillingRoute:
    if origin is RedoOrigin.SYSTEM:
        return BillingRoute.internal(recovery_cost_center_id)
    return project_route


class MediaReviewService:
    def __init__(
        self,
        access: ProjectAccessService,
        audit: AuditWriter,
        clock: Clock,
    ) -> None:
        self.access = access
        self.audit = audit
        self.clock = clock

    def approve_low_resolution(
        self,
        session: Session,
        actor: ActorContext,
        production_id: UUID,
        selected_asset_by_shot: dict[UUID, UUID],
    ) -> MediaProduction:
        production = session.scalar(
            select(MediaProduction)
            .where(MediaProduction.id == production_id)
            .with_for_update()
        )
        if production is None:
            raise NotFound("media production not found")
        self.access.require_editor(session, actor, production.project_id)
        if production.status != ProductionStatus.LOW_RES_READY.value:
            raise Conflict("production is not ready for low-resolution approval")
        shots = list(
            session.scalars(
                select(Shot).where(
                    Shot.production_id == production.id,
                    Shot.is_current.is_(True),
                )
            )
        )
        require_preview_assets(shots, selected_asset_by_shot)
        for shot in shots:
            selected = selected_asset_by_shot.get(shot.id)
            if selected is not None:
                shot.selected_asset_id = selected
        production.low_res_approved_at = self.clock.now()
        production.status = ProductionStatus.LOW_RES_APPROVED.value
        self.audit.write(
            session,
            actor=actor,
            action="media.preview.approved",
            target_type="media_production",
            target_id=production.id,
            project_id=production.project_id,
            metadata={"selected_shot_count": len(selected_asset_by_shot)},
        )
        return production

    def redo_shot(
        self,
        session: Session,
        actor: ActorContext,
        shot_id: UUID,
        *,
        reason: str,
        origin: RedoOrigin,
    ) -> Shot:
        old = session.scalar(select(Shot).where(Shot.id == shot_id).with_for_update())
        if old is None:
            raise NotFound("media shot not found")
        self.access.require_editor(session, actor, old.project_id)
        if not old.is_current:
            raise Conflict("only the current shot revision can be redone")
        production = session.scalar(
            select(MediaProduction)
            .where(MediaProduction.id == old.production_id)
            .with_for_update()
        )
        if production is None:
            raise NotFound("media production not found")
        old.is_current = False
        replacement = Shot(
            account_id=old.account_id,
            project_id=old.project_id,
            production_id=old.production_id,
            scene_id=old.scene_id,
            logical_shot_id=old.logical_shot_id,
            revision=old.revision + 1,
            supersedes_id=old.id,
            is_current=True,
            sequence=old.sequence,
            title=old.title,
            source=old.source,
            duration_ms=old.duration_ms,
            prompt=old.prompt,
            spoken_text=old.spoken_text,
            character_id=old.character_id,
            voice_id=old.voice_id,
            product_reference_asset_id=old.product_reference_asset_id,
            selected_asset_id=None,
        )
        session.add(replacement)
        session.flush()
        production.status = ProductionStatus.PARTIAL_REDO.value
        self.audit.write(
            session,
            actor=actor,
            action="media.shot.redo.created",
            target_type="media_shot",
            target_id=replacement.id,
            project_id=old.project_id,
            metadata={
                "supersedes_id": str(old.id),
                "origin": origin.value,
                "reason": reason,
            },
        )
        return replacement
```

`MediaReviewService` changes only the requested logical shot. The caller schedules its replacement through `MediaJobService`: user-originated work submits a new customer TaskRecord/hold; system-originated recovery submits an internal TaskRecord/hold. The prior customer task is terminalized only through the Task 15 known-cost path or the Plan 01 reconciliation scanner—never by an unconditional release—so supplier cost is retained while the customer is charged zero for system defects.

- [ ] **Step 4: Run the preview and funding tests**

Run: `cd backend && uv run pytest tests/unit/media/test_preview_redo.py -v`

Expected: 3 passed.

- [ ] **Step 5: Verify one-shot revision behavior in PostgreSQL**

In `backend/tests/integration/media/test_per_shot_redo.py`, create three current shots, redo the middle logical shot, and schedule only its replacement. Assert the first and third shot IDs and selected assets are unchanged; the old middle revision is retained with `is_current=false`; the replacement has revision 2; there is one current row per logical ID; user redo gets a new customer TaskRecord; system redo gets an internal TaskRecord; neither path reuses a settled hold.

Run: `cd backend && uv run pytest tests/integration/media/test_per_shot_redo.py -v`

Expected: 2 passed, one for each redo origin.

- [ ] **Step 6: Commit preview approval and per-shot redo**

```bash
git add backend/src/ip_saas/modules/media/service.py backend/tests/unit/media/test_preview_redo.py backend/tests/integration/media/test_per_shot_redo.py
git commit -m "feat: approve previews and redo individual shots"
```

### Task 14: Finish media with controlled FFmpeg args, captions, and mandatory AI disclosure

**Files:**
- Create: `backend/src/ip_saas/modules/media/captions.py`
- Create: `backend/src/ip_saas/modules/media/labels.py`
- Create: `backend/src/ip_saas/modules/media/ffmpeg.py`
- Create: `backend/src/ip_saas/workers/media_finish.py`
- Test: `backend/tests/unit/media/test_ffmpeg_safety.py`
- Test: `backend/tests/integration/media/test_media_finishing.py`

- [ ] **Step 1: Write failing safety and disclosure tests**

```python
from pathlib import Path
from uuid import uuid4

import pytest

from ip_saas.modules.media.captions import CaptionCue, render_srt
from ip_saas.modules.media.enums import AiDisclosure
from ip_saas.modules.media.ffmpeg import FinishSpec, build_ffmpeg_args
from ip_saas.modules.media.labels import disclosure_for_sources


def test_generated_source_always_requires_visible_and_metadata_labels() -> None:
    policy = disclosure_for_sources([AiDisclosure.NONE, AiDisclosure.GENERATED])
    assert policy.disclosure is AiDisclosure.GENERATED
    assert policy.visible_text == "AI生成内容"
    assert policy.metadata["ai_generated"] == "true"


def test_ffmpeg_is_an_argument_vector_with_no_shell_or_user_filter(tmp_path: Path) -> None:
    manifest = tmp_path / "concat.txt"
    manifest.write_text("file 'shot-1.mp4'\n", encoding="utf-8")
    captions = tmp_path / "captions.srt"
    captions.write_text("1\n00:00:00,000 --> 00:00:01,000\n测试\n", encoding="utf-8")
    font = tmp_path / "label.ttf"
    font.write_bytes(b"font-fixture")
    output = tmp_path / "final.mp4"
    args = build_ffmpeg_args(
        manifest,
        output,
        FinishSpec(1080, 1920, 30, 180, captions, font, uuid4(), True),
        tmp_path,
    )
    assert isinstance(args, list)
    assert args[0] == "ffmpeg"
    assert "-nostdin" in args
    assert "-metadata" in args
    assert all(";" not in value and "$(" not in value for value in args)


def test_paths_outside_the_job_workspace_are_rejected(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="workspace"):
        build_ffmpeg_args(
            Path("/tmp/attacker-concat.txt"),
            tmp_path / "final.mp4",
            FinishSpec(1080, 1920, 30, 180, None, tmp_path / "label.ttf", uuid4(), True),
            tmp_path,
        )


def test_caption_renderer_has_stable_timestamps_and_plain_text() -> None:
    rendered = render_srt([CaptionCue(0, 1500, "礼不是价格，<b>是关系</b>。")])
    assert "00:00:00,000 --> 00:00:01,500" in rendered
    assert "<b>" not in rendered
```

- [ ] **Step 2: Run the tests and verify finishing modules are missing**

Run: `cd backend && uv run pytest tests/unit/media/test_ffmpeg_safety.py -v`

Expected: FAIL during collection because caption, label, and FFmpeg modules do not exist.

- [ ] **Step 3: Implement plain-text SRT rendering**

```python
from dataclasses import dataclass
from html import unescape
from re import sub


@dataclass(frozen=True)
class CaptionCue:
    start_ms: int
    end_ms: int
    text: str

    def __post_init__(self) -> None:
        if self.start_ms < 0 or self.end_ms <= self.start_ms:
            raise ValueError("caption cue requires an increasing non-negative interval")
        if self.end_ms - self.start_ms > 15_000:
            raise ValueError("caption cue exceeds fifteen seconds")


def render_srt(cues: list[CaptionCue]) -> str:
    previous_end = 0
    blocks: list[str] = []
    for index, cue in enumerate(cues, start=1):
        if cue.start_ms < previous_end:
            raise ValueError("caption cues overlap")
        text = _plain_text(cue.text)
        if not text:
            raise ValueError("caption text is empty")
        blocks.append(
            f"{index}\n{_timestamp(cue.start_ms)} --> {_timestamp(cue.end_ms)}\n{text}"
        )
        previous_end = cue.end_ms
    return "\n\n".join(blocks) + ("\n" if blocks else "")


def _plain_text(value: str) -> str:
    without_tags = sub(r"<[^>]*>", "", value)
    return unescape(without_tags).replace("\r", " ").strip()


def _timestamp(milliseconds: int) -> str:
    hours, remainder = divmod(milliseconds, 3_600_000)
    minutes, remainder = divmod(remainder, 60_000)
    seconds, millis = divmod(remainder, 1_000)
    return f"{hours:02d}:{minutes:02d}:{seconds:02d},{millis:03d}"
```

Save this as `backend/src/ip_saas/modules/media/captions.py`.

- [ ] **Step 4: Implement the non-removable Plan 03 disclosure policy**

```python
from dataclasses import dataclass

from .enums import AiDisclosure


@dataclass(frozen=True)
class DisclosurePolicy:
    disclosure: AiDisclosure
    visible_text: str | None
    metadata: dict[str, str]


def disclosure_for_sources(sources: list[AiDisclosure]) -> DisclosurePolicy:
    if AiDisclosure.GENERATED in sources:
        return DisclosurePolicy(
            AiDisclosure.GENERATED,
            "AI生成内容",
            {"ai_generated": "true", "ai_assisted": "true"},
        )
    if AiDisclosure.ASSISTED in sources:
        return DisclosurePolicy(
            AiDisclosure.ASSISTED,
            "AI辅助创作",
            {"ai_generated": "false", "ai_assisted": "true"},
        )
    return DisclosurePolicy(
        AiDisclosure.NONE,
        None,
        {"ai_generated": "false", "ai_assisted": "false"},
    )
```

Save this as `backend/src/ip_saas/modules/media/labels.py`. Plan 03 exposes no switch, role, or exception that removes a required visible label.

- [ ] **Step 5: Implement a controlled FFmpeg/FFprobe boundary**

```python
from dataclasses import dataclass
from pathlib import Path
from re import fullmatch
from subprocess import CompletedProcess, run
from uuid import UUID


@dataclass(frozen=True)
class FinishSpec:
    width: int
    height: int
    fps: int
    max_duration_seconds: int
    captions_path: Path | None
    label_font_path: Path
    content_id: UUID
    ai_generated: bool

    def __post_init__(self) -> None:
        if (self.width, self.height) not in {(1080, 1920), (1080, 1440), (1080, 1080), (1920, 1080)}:
            raise ValueError("unsupported export resolution")
        if self.fps not in {24, 25, 30}:
            raise ValueError("unsupported export frame rate")
        if not 1 <= self.max_duration_seconds <= 600:
            raise ValueError("unsupported export duration")


def build_ffmpeg_args(
    concat_manifest: Path,
    output_path: Path,
    spec: FinishSpec,
    workspace: Path,
) -> list[str]:
    root = workspace.resolve(strict=True)
    manifest = _safe_file(concat_manifest, root, must_exist=True)
    output = _safe_file(output_path, root, must_exist=False)
    font = _safe_file(spec.label_font_path, root, must_exist=True)
    filters = [
        f"scale={spec.width}:{spec.height}:force_original_aspect_ratio=decrease",
        f"pad={spec.width}:{spec.height}:(ow-iw)/2:(oh-ih)/2:black",
        f"fps={spec.fps}",
        "format=yuv420p",
    ]
    if spec.captions_path is not None:
        captions = _safe_file(spec.captions_path, root, must_exist=True)
        _safe_basename(captions.name)
        filters.append(
            "subtitles=" + captions.name + ":force_style='FontSize=18,Outline=2,Alignment=2,MarginV=90'"
        )
    if spec.ai_generated:
        _safe_basename(font.name)
        filters.extend(
            [
                "drawbox=x=w-tw-42:y=36:w=tw+28:h=44:color=black@0.65:t=fill",
                "drawtext="
                + f"fontfile={font.name}:text='AI生成内容':fontcolor=white:fontsize=26:"
                + "x=w-tw-28:y=44",
            ]
        )
    args = [
        "ffmpeg",
        "-nostdin",
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "concat",
        "-safe",
        "1",
        "-i",
        manifest.name,
        "-vf",
        ",".join(filters),
        "-map",
        "0:v:0",
        "-map",
        "0:a:0?",
        "-c:v",
        "libx264",
        "-profile:v",
        "high",
        "-level:v",
        "4.1",
        "-pix_fmt",
        "yuv420p",
        "-preset",
        "medium",
        "-crf",
        "20",
        "-c:a",
        "aac",
        "-b:a",
        "192k",
        "-ar",
        "48000",
        "-ac",
        "2",
        "-af",
        "loudnorm=I=-16:TP=-1.5:LRA=11",
        "-t",
        str(spec.max_duration_seconds),
        "-movflags",
        "+faststart",
        "-metadata",
        f"content_id={spec.content_id}",
        "-metadata",
        f"ai_generated={'true' if spec.ai_generated else 'false'}",
        "-threads",
        "2",
        "-y",
        output.name,
    ]
    return args


def run_ffmpeg(args: list[str], workspace: Path, timeout_seconds: int = 1800) -> CompletedProcess[str]:
    if not args or args[0] != "ffmpeg":
        raise ValueError("only the configured FFmpeg executable is allowed")
    return run(
        args,
        cwd=workspace,
        shell=False,
        check=True,
        capture_output=True,
        text=True,
        timeout=timeout_seconds,
    )


def write_concat_manifest(paths: list[Path], destination: Path, workspace: Path) -> None:
    root = workspace.resolve(strict=True)
    safe_names: list[str] = []
    for path in paths:
        safe = _safe_file(path, root, must_exist=True)
        _safe_basename(safe.name)
        safe_names.append(safe.name)
    destination.write_text(
        "".join(f"file '{name}'\n" for name in safe_names),
        encoding="utf-8",
    )


def _safe_file(path: Path, root: Path, *, must_exist: bool) -> Path:
    resolved = path.resolve(strict=must_exist)
    if resolved.parent != root:
        raise ValueError("media path must be directly inside the job workspace")
    _safe_basename(resolved.name)
    return resolved


def _safe_basename(name: str) -> None:
    if fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", name) is None:
        raise ValueError("unsafe media workspace filename")
```

Save this as `backend/src/ip_saas/modules/media/ffmpeg.py`.

- [ ] **Step 6: Add a synchronous finishing runner**

```python
from pathlib import Path
from uuid import UUID

from ip_saas.modules.media.captions import CaptionCue, render_srt
from ip_saas.modules.media.enums import AiDisclosure
from ip_saas.modules.media.ffmpeg import (
    FinishSpec,
    build_ffmpeg_args,
    run_ffmpeg,
    write_concat_manifest,
)
from ip_saas.modules.media.labels import disclosure_for_sources


def finish_in_workspace(
    workspace: Path,
    shot_files: list[Path],
    cues: list[CaptionCue],
    source_disclosures: list[AiDisclosure],
    content_id: UUID,
    font_file: Path,
    *,
    width: int,
    height: int,
    fps: int,
    max_duration_seconds: int,
) -> Path:
    if not shot_files:
        raise ValueError("finishing requires at least one selected shot")
    manifest = workspace / "concat.txt"
    captions = workspace / "captions.srt"
    output = workspace / "final.mp4"
    write_concat_manifest(shot_files, manifest, workspace)
    captions.write_text(render_srt(cues), encoding="utf-8")
    disclosure = disclosure_for_sources(source_disclosures)
    args = build_ffmpeg_args(
        manifest,
        output,
        FinishSpec(
            width,
            height,
            fps,
            max_duration_seconds,
            captions if cues else None,
            font_file,
            content_id,
            disclosure.disclosure is not AiDisclosure.NONE,
        ),
        workspace,
    )
    run_ffmpeg(args, workspace)
    if not output.is_file() or output.stat().st_size == 0:
        raise RuntimeError("FFmpeg completed without a final media file")
    return output
```

Save this as `backend/src/ip_saas/workers/media_finish.py`. The worker downloads only the selected private TOS assets into a newly created per-job directory, copies the configured font to `label.ttf`, calls `finish_in_workspace`, uploads `final.mp4` to private TOS, creates a `FINAL_VIDEO` asset, and emits `media.qc.requested`. It deletes the workspace in `finally` and never accepts a user-supplied path, filter, codec, executable, or FFmpeg option.

- [ ] **Step 7: Run unit safety tests**

Run: `cd backend && uv run pytest tests/unit/media/test_ffmpeg_safety.py -v`

Expected: 4 passed.

- [ ] **Step 8: Run a real local FFmpeg finishing integration test**

Create two one-second color clips and a sine-wave audio track inside Pytest's temporary directory using controlled FFmpeg args, finish them with captions and the AI label, then probe the result. Assert H.264/yuv420p, AAC/48 kHz stereo, expected dimensions, duration under three seconds, `ai_generated=true`, fast-start readability, and a non-empty upper-right label region on frames at 0.2 and 1.2 seconds.

Run: `cd backend && uv run pytest tests/integration/media/test_media_finishing.py -v`

Expected: 1 passed; skip only when the developer bootstrap check explicitly reports FFmpeg unavailable.

- [ ] **Step 9: Commit controlled finishing, captions, and disclosure**

```bash
git add backend/src/ip_saas/modules/media/captions.py backend/src/ip_saas/modules/media/labels.py backend/src/ip_saas/modules/media/ffmpeg.py backend/src/ip_saas/workers/media_finish.py backend/tests/unit/media/test_ffmpeg_safety.py backend/tests/integration/media/test_media_finishing.py
git commit -m "feat: finish media with captions and ai labels"
```

### Task 15: Run copy, finishing, QC, billing, and recovery as one lease-safe delivery pipeline

**Files:**
- Modify: `backend/src/ip_saas/modules/media/qc.py`
- Modify: `backend/src/ip_saas/modules/media/service.py`
- Modify: `backend/src/ip_saas/workers/media_submit.py`
- Modify: `backend/src/ip_saas/workers/media_poll.py`
- Modify: `backend/src/ip_saas/workers/media_copy.py`
- Modify: `backend/src/ip_saas/workers/media_finish.py`
- Create: `backend/src/ip_saas/workers/media_lease.py`
- Create: `backend/src/ip_saas/workers/media_local.py`
- Create: `backend/src/ip_saas/workers/media_qc.py`
- Create: `backend/src/ip_saas/workers/media_recovery.py`
- Create: `backend/tests/integration/media/conftest.py`
- Create: `backend/tests/integration/media/harness.py`
- Test: `backend/tests/unit/media/test_qc_port.py`
- Test: `backend/tests/integration/media/test_media_terminal_billing.py`
- Test: `backend/tests/integration/media/test_media_lease_recovery.py`
- Test: `backend/tests/integration/media/test_media_human_review.py`
- Test: `backend/tests/integration/media/test_media_local.py`
- Test: `backend/tests/integration/media/test_provider_worker_idempotency.py`

- [ ] **Step 1: Write failing QC taxonomy and replaceable-fake tests**

```python
# backend/tests/unit/media/test_qc_port.py
from uuid import uuid4

from ip_saas.modules.media.enums import QcDecision
from ip_saas.modules.media.qc import (
    CompositeMediaQCPort,
    DeterministicMediaQCFake,
    QCCheck,
    QCRequest,
    QCSeverity,
)


def check(code: str, passed: bool, *, global_blocker: bool = False) -> QCCheck:
    return QCCheck(
        code=code, passed=passed,
        severity=QCSeverity.BLOCKER if not passed else QCSeverity.INFO,
        message=code, logical_shot_id=None,
        global_blocker=global_blocker,
    )


def complete_request() -> QCRequest:
    return QCRequest(
        job_id=uuid4(), asset_id=uuid4(), project_id=uuid4(),
        technical_checks=(check("decode_ok", True), check("codec_ok", True),
                          check("subtitle_safe", True), check("loudness_ok", True)),
        semantic_checks=(check("shot_intent_match", True), check("face_hand_lipsync_ok", True)),
        rights_checks=(check("rights_active", True), check("consent_snapshot_present", True)),
        product_truth_checks=(check("product_reference_match", True),),
        continuity_checks=(check("ip_character_continuity", True),),
        disclosure_checks=(check("ai_label_visible", True),
                           check("ai_label_metadata", True)),
    )


def test_composite_qc_requires_every_evidence_family() -> None:
    outcome = CompositeMediaQCPort().inspect(complete_request())
    assert outcome.decision is QcDecision.PASS
    assert {item.code for item in outcome.disclosure_checks} == {
        "ai_label_visible", "ai_label_metadata",
    }


def test_global_rights_or_disclosure_failure_blocks_delivery() -> None:
    request = complete_request()
    request = request.replace(
        rights_checks=(
            check("rights_active", False, global_blocker=True),
            check("consent_snapshot_present", True),
        ),
    )
    assert CompositeMediaQCPort().inspect(request).decision is QcDecision.FAIL


def test_localized_semantic_defect_requests_only_its_logical_shot() -> None:
    shot_id = uuid4()
    defect = QCCheck(
        code="extra_finger", passed=False, severity=QCSeverity.ERROR,
        message="hand anatomy mismatch", logical_shot_id=shot_id,
        global_blocker=False,
    )
    outcome = CompositeMediaQCPort().inspect(
        complete_request().replace(semantic_checks=(defect,))
    )
    assert outcome.decision is QcDecision.REDO
    assert outcome.redo_logical_shot_ids == (shot_id,)


def test_deterministic_fake_needs_no_provider_credential(monkeypatch) -> None:
    for key in ("ARK_API_KEY", "DOUBAO_TTS_ACCESS_KEY", "TOS_SECRET_KEY"):
        monkeypatch.delenv(key, raising=False)
    request = complete_request()
    fake = DeterministicMediaQCFake({request.asset_id: CompositeMediaQCPort().inspect(request)})
    assert fake.inspect(request).decision is QcDecision.PASS
    assert fake.call_count == 1
```

- [ ] **Step 2: Write failing task lease, terminal billing, and human-review tests**

```python
# backend/tests/integration/media/test_media_terminal_billing.py
import pytest
from sqlalchemy import select

from ip_saas.common.tasking import TaskStatus
from ip_saas.modules.billing.models import ProviderCostEntry
from ip_saas.modules.media.enums import QcDecision
from ip_saas.modules.tasks.models import TaskRecord
from ip_saas.workers.media_lease import (
    ProcessDisposition,
    disposition_after_lease_loss,
)


@pytest.mark.parametrize("provider", ["seedream", "seedance", "doubao_tts"])
def test_real_provider_pass_settles_then_succeeds_with_task_cost_lineage(
    db_session, passed_qc_case, provider
) -> None:
    case = passed_qc_case(provider)
    assert case.worker.run(case.job.id, case.asset.id, case.attempt_no) == ProcessDisposition.ACK
    db_session.expire_all()
    task = db_session.get(TaskRecord, case.job.task_record_id)
    cost = db_session.scalar(select(ProviderCostEntry).where(
        ProviderCostEntry.task_id == task.id
    ))
    assert task.status == TaskStatus.SUCCEEDED
    assert cost is not None and cost.task_id == task.id
    assert case.hold_status() == "settled"


@pytest.mark.parametrize("decision", [QcDecision.REDO, QcDecision.FAIL])
def test_real_provider_non_deliverable_qc_records_cost_but_charges_zero(
    db_session, non_deliverable_qc_case, decision
) -> None:
    case = non_deliverable_qc_case(decision)
    assert case.worker.run(case.job.id, case.asset.id, case.attempt_no) == ProcessDisposition.ACK
    assert case.task_status() == "succeeded"
    assert case.customer_debit() == 0
    assert case.provider_cost_task_ids() == {case.task.id}
    assert case.hold_status() == "released"


def test_billed_provider_failure_records_task_cost_and_absorbs_customer_charge(
    billed_provider_failure_case,
) -> None:
    case = billed_provider_failure_case
    assert case.worker.run(case.job.id) == ProcessDisposition.ACK
    assert case.task_status() == "failed"
    assert case.customer_debit() == 0
    assert case.provider_cost_task_ids() == {case.task.id}
    assert case.hold_status() == "released"


def test_provider_cost_is_absorbed_before_downstream_retry_exhaustion(
    downstream_exhausted_case,
) -> None:
    case = downstream_exhausted_case
    assert case.recovery.run_once() == 1
    assert case.task_status() == "failed"
    assert case.customer_debit() == 0
    assert case.provider_cost_task_ids() == {case.task.id}
    assert case.hold_status() == "released"


def test_provider_success_without_terminal_usage_is_not_deliverable(missing_usage_case) -> None:
    case = missing_usage_case
    assert case.worker.run(case.job.id) == ProcessDisposition.RETRY
    assert case.task_error_code() == "provider_usage_missing"
    assert case.delivery_link_count() == 0
    assert case.release_count() == 0
    assert case.provider_cost_count() == 0
    assert case.hold_status() == "active"
    case.exhaust_to_reconciliation_required()
    assert case.task_status() == "reconciliation_required"
    assert case.hold_status() == "active"
```

```python
# backend/tests/integration/media/test_media_lease_recovery.py
import pytest

from ip_saas.workers.media_lease import ProcessDisposition


def test_terminal_delivery_is_acked_without_stage_call(terminal_media_case) -> None:
    assert terminal_media_case.worker.run(terminal_media_case.job.id) == ProcessDisposition.ACK
    assert terminal_media_case.stage_call_count() == 0


def test_active_lease_returns_retry(active_media_case) -> None:
    assert active_media_case.worker.run(active_media_case.job.id) == ProcessDisposition.RETRY
    assert active_media_case.stage_call_count() == 0


def test_reconciliation_required_retries_without_any_stage_call(
    reconciliation_media_case,
) -> None:
    assert (
        reconciliation_media_case.worker.run(reconciliation_media_case.job.id)
        == ProcessDisposition.RETRY
    )
    assert reconciliation_media_case.stage_call_count() == 0


def test_expired_lease_is_reclaimed_with_a_new_attempt(expired_media_case) -> None:
    before = expired_media_case.attempt_no()
    assert expired_media_case.recovery.run_once() == 1
    assert expired_media_case.attempt_no() == before + 1


def test_max_attempt_expiry_uses_start_and_keeps_hold_for_scanner(
    exhausted_media_case,
) -> None:
    assert exhausted_media_case.recovery.run_once() == 1
    assert exhausted_media_case.task_status() == "reconciliation_required"
    assert exhausted_media_case.hold_status() == "active"
    assert exhausted_media_case.stage_call_count() == 0
    assert exhausted_media_case.terminal_failure_count() == 0


@pytest.mark.parametrize("turn", ["submit", "poll"])
def test_lease_expiring_immediately_before_provider_turn_calls_no_provider(
    expired_before_provider_case,
    turn: str,
) -> None:
    case = expired_before_provider_case(turn)
    case.expire_immediately_before_context_enter()
    assert case.worker.run(case.job.id) == ProcessDisposition.RETRY
    assert case.provider_call_count() == 0
    assert case.domain_write_count() == 0
    assert case.ledger_write_count() == 0


def test_same_attempt_expiring_after_provider_response_retries_without_persist(
    expired_after_provider_case,
) -> None:
    case = expired_after_provider_case
    case.expire_after_provider_response()
    assert case.worker.run(case.job.id) == ProcessDisposition.RETRY
    assert case.provider_call_count() == 1
    assert case.domain_write_count() == 0
    assert case.ledger_write_count() == 0
    assert case.durable_attempt_is_same() is True


def test_stale_copy_or_qc_worker_cannot_write_any_terminal_side_effect(stale_media_case) -> None:
    stale_media_case.reclaim_with_new_worker()
    assert stale_media_case.finish_old_worker() == ProcessDisposition.ACK
    assert stale_media_case.domain_write_count_for_old_attempt() == 0
    assert stale_media_case.ledger_write_count_for_old_attempt() == 0
    assert stale_media_case.delivery_link_count_for_old_attempt() == 0


@pytest.mark.parametrize(
    "state,expected",
    [
        ("missing", ProcessDisposition.RETRY),
        ("same_attempt_expired", ProcessDisposition.RETRY),
        ("reconciliation_required", ProcessDisposition.RETRY),
        ("terminal", ProcessDisposition.ACK),
        ("newer_attempt", ProcessDisposition.ACK),
    ],
)
def test_lease_loss_ack_requires_terminal_or_strictly_newer_attempt(
    lease_loss_disposition_case,
    state: str,
    expected: ProcessDisposition,
) -> None:
    case = lease_loss_disposition_case(state)
    assert disposition_after_lease_loss(
        case.task_id, case.captured_attempt_no
    ) is expected
```

```python
# backend/tests/integration/media/test_media_human_review.py
import pytest

from ip_saas.common.errors import Conflict
from ip_saas.modules.media.enums import HumanReviewDecision


def test_machine_pass_still_needs_explicit_human_pass_before_delivery(human_review_case) -> None:
    assert human_review_case.delivery_allowed() is False
    human_review_case.review(HumanReviewDecision.PASS, "画面、商品与字幕均已核对")
    assert human_review_case.delivery_allowed() is True


def test_machine_rights_failure_cannot_be_overridden_to_pass(human_review_case) -> None:
    human_review_case.machine_fail("rights_active")
    with pytest.raises(Conflict, match="blocking QC"):
        human_review_case.review(HumanReviewDecision.PASS, "强行通过")


def test_human_redo_keeps_old_asset_and_targets_only_reported_shot(human_review_case) -> None:
    old_asset_id = human_review_case.asset.id
    redo_job = human_review_case.review(
        HumanReviewDecision.REDO, "只重做手部异常镜头"
    )
    assert human_review_case.asset_still_exists(old_asset_id)
    assert redo_job.shot_id == human_review_case.reported_shot_id
```

```python
# backend/tests/integration/media/test_media_local.py
def test_human_package_uses_no_provider_and_releases_before_succeed(
    local_human_case,
) -> None:
    case = local_human_case
    case.handler(case.task.id)
    assert case.provider_call_count() == 0
    assert case.package_count() == 1
    assert case.hold_status() == "released"
    assert case.task_status() == "succeeded"
    assert case.transition_order() == ["release", "succeed"]
    case.handler(case.task.id)
    assert case.package_count() == 1
```

- [ ] **Step 3: Define every custom integration fixture through one explicit harness**

Create `backend/tests/integration/media/harness.py` as the single scenario builder. It must use the production `MediaQCWorker`, `MediaSubmitWorker`, `MediaCopyWorker`, `MediaFinishWorker`, `MediaRecoveryWorker`, `MediaHumanReviewService`, `TaskSubmissionService`, and `BillingService`; provider, TOS, clock, QC, and the explicit pre-Plan-05 generation-limit port are the only deterministic fakes. Its public methods and return shape are frozen here so no test relies on an implicit fixture side effect:

```python
from dataclasses import dataclass
from typing import Callable
from uuid import UUID

from sqlalchemy.orm import Session

from ip_saas.modules.media.enums import QcDecision
from tests.support.generation_limits import AllowAllGenerationLimits


@dataclass(frozen=True)
class ProviderScenario:
    worker: object
    job: object
    task: object
    asset: object | None
    attempt_no: int
    task_status: Callable[[], str]
    task_error_code: Callable[[], str | None]
    hold_status: Callable[[], str]
    customer_debit: Callable[[], int]
    provider_cost_task_ids: Callable[[], set[UUID]]
    provider_cost_count: Callable[[], int]
    release_count: Callable[[], int]
    delivery_link_count: Callable[[], int]
    exhaust_to_reconciliation_required: Callable[[], int]


@dataclass(frozen=True)
class DownstreamExhaustedScenario:
    recovery: object
    task: object
    task_status: Callable[[], str]
    hold_status: Callable[[], str]
    customer_debit: Callable[[], int]
    provider_cost_task_ids: Callable[[], set[UUID]]


@dataclass(frozen=True)
class LeaseScenario:
    worker: object
    job: object
    recovery: object
    stage_call_count: Callable[[], int]
    attempt_no: Callable[[], int]
    reclaim_with_new_worker: Callable[[], None]
    finish_old_worker: Callable[[], object]
    domain_write_count_for_old_attempt: Callable[[], int]
    ledger_write_count_for_old_attempt: Callable[[], int]
    delivery_link_count_for_old_attempt: Callable[[], int]
    task_status: Callable[[], str]
    hold_status: Callable[[], str]
    terminal_failure_count: Callable[[], int]


@dataclass(frozen=True)
class PreCallLeaseScenario:
    worker: object
    job: object
    expire_immediately_before_context_enter: Callable[[], None]
    provider_call_count: Callable[[], int]
    domain_write_count: Callable[[], int]
    ledger_write_count: Callable[[], int]


@dataclass(frozen=True)
class PostCallLeaseScenario:
    worker: object
    job: object
    expire_after_provider_response: Callable[[], None]
    provider_call_count: Callable[[], int]
    domain_write_count: Callable[[], int]
    ledger_write_count: Callable[[], int]
    durable_attempt_is_same: Callable[[], bool]


@dataclass(frozen=True)
class LeaseDispositionScenario:
    task_id: UUID
    captured_attempt_no: int


@dataclass(frozen=True)
class HumanReviewScenario:
    asset: object
    reported_shot_id: UUID
    delivery_allowed: Callable[[], bool]
    review: Callable[[object, str], object]
    machine_fail: Callable[[str], None]
    asset_still_exists: Callable[[UUID], bool]


@dataclass(frozen=True)
class LocalHumanScenario:
    handler: Callable[[UUID], None]
    task: object
    provider_call_count: Callable[[], int]
    package_count: Callable[[], int]
    hold_status: Callable[[], str]
    task_status: Callable[[], str]
    transition_order: Callable[[], list[str]]


class MediaIntegrationHarness:
    def __init__(self, session: Session) -> None:
        self.session = session
        self.generation_limits = AllowAllGenerationLimits()

    def passed_qc(self, provider: str) -> ProviderScenario:
        return self._seed_provider_completion(provider, QcDecision.PASS, usage=True)

    def non_deliverable_qc(self, decision: QcDecision) -> ProviderScenario:
        return self._seed_provider_completion("seedance", decision, usage=True)

    def billed_provider_failure(self) -> ProviderScenario:
        return self._seed_provider_failure(usage=True)

    def downstream_exhausted(self) -> DownstreamExhaustedScenario:
        return self._seed_downstream_exhausted_case()

    def missing_usage(self) -> ProviderScenario:
        return self._seed_provider_completion("seedance", QcDecision.PASS, usage=False)

    def terminal(self) -> LeaseScenario:
        return self._seed_lease_case("succeeded")

    def active(self) -> LeaseScenario:
        return self._seed_lease_case("active")

    def reconciliation_required(self) -> LeaseScenario:
        return self._seed_lease_case("reconciliation_required")

    def expired(self) -> LeaseScenario:
        return self._seed_lease_case("expired")

    def exhausted(self) -> LeaseScenario:
        return self._seed_lease_case("exhausted")

    def stale(self) -> LeaseScenario:
        return self._seed_lease_case("stale")

    def expired_before_provider(self, turn: str) -> PreCallLeaseScenario:
        if turn not in {"submit", "poll"}:
            raise ValueError("turn must be submit or poll")
        return self._seed_pre_call_lease_case(turn)

    def expired_after_provider(self) -> PostCallLeaseScenario:
        return self._seed_post_call_lease_case()

    def lease_loss_disposition(self, state: str) -> LeaseDispositionScenario:
        if state not in {
            "missing", "same_attempt_expired", "reconciliation_required",
            "terminal", "newer_attempt",
        }:
            raise ValueError("unknown lease disposition state")
        return self._seed_lease_disposition_case(state)

    def human_review(self) -> HumanReviewScenario:
        return self._seed_human_review_case()

    def local_human(self) -> LocalHumanScenario:
        return self._seed_local_human_case()

    def job_transaction_case(self):
        return self._seed_job_transaction_case()

    def two_account_job_case(self, key: str):
        return self._seed_two_account_job_case(key)

    def job_parameter_conflict_case(self, key: str, change: str):
        return self._seed_job_parameter_conflict_case(key, change)
```

All private seed methods are implemented in the same file, not imported from hidden test utilities. Each builds a real account, principal, project, active model registry row, `ContentVersion`, `PlatformVariant`, production, shot, shared TaskRecord with `request_fingerprint="a" * 64`, one hold appropriate to `billing_mode`, job, and optional asset/QC row in `db_session`. At the `0003_media` stage, every scenario constructs `BillingService(credit_holds, internal_holds, AllowAllGenerationLimits())` explicitly. Only after Plan 05 merges do customer scenarios call `allow_customer(db_session, admin, account_id)` and internal scenarios call `allow_internal(db_session, admin, cost_center_id)`, then pass the real database `GenerationLimitService` as the third constructor argument. The harness wires SQL credit/internal hold adapters and asserts that a real `ProviderCostEntry` query—not a mock call—backs `provider_cost_task_ids()`. It patches no production module and performs no network call. Lease cases set `attempt_no`, `lease_expires_at`, and the exact active/expired/reconciliation/terminal state, then use the frozen `start/heartbeat/succeed/fail` API; the reconciliation fixture proves that no media stage runs and the original hold remains active. `_seed_pre_call_lease_case()` uses the fake clock hook to expire the captured attempt after the Worker builds its snapshot but immediately before `LeaseHeartbeat.__enter__`; the synchronous heartbeat CAS must fail, the Worker returns RETRY, and the counting submit/poll fake remains at zero calls. `_seed_post_call_lease_case()` advances the fake clock after the provider fake returns but before `heartbeat.ensure_owned()` performs its synchronous CAS; the task retains the same attempt number, the helper returns RETRY, and no provider result, cost, outbox, or delivery state persists. `_seed_lease_disposition_case()` returns a nonexistent UUID for `missing`; a same-attempt expired row; a same-attempt `RECONCILIATION_REQUIRED` row; a terminal row; or a RUNNING row whose durable `attempt_no` is exactly one higher than the captured value. This makes the ACK/RETRY table a real database test, not an exception-message heuristic. Idempotency cases create two authorized projects/accounts with the same client key, and mutate one of payload, quote, billing route, shot, or capability inside the same project. Human review returns a case object whose methods are exactly those invoked in `test_media_human_review.py`. This construction is executed inside Plan 01's rollback-isolated `db_session`, so every scenario is independent.

Create `backend/tests/integration/media/conftest.py` with all named fixtures:

```python
import pytest
from sqlalchemy.orm import Session

from .harness import MediaIntegrationHarness


@pytest.fixture
def media_integration_harness(db_session: Session) -> MediaIntegrationHarness:
    return MediaIntegrationHarness(db_session)


@pytest.fixture
def passed_qc_case(media_integration_harness):
    return media_integration_harness.passed_qc


@pytest.fixture
def non_deliverable_qc_case(media_integration_harness):
    return media_integration_harness.non_deliverable_qc


@pytest.fixture
def billed_provider_failure_case(media_integration_harness):
    return media_integration_harness.billed_provider_failure()


@pytest.fixture
def downstream_exhausted_case(media_integration_harness):
    return media_integration_harness.downstream_exhausted()


@pytest.fixture
def missing_usage_case(media_integration_harness):
    return media_integration_harness.missing_usage()


@pytest.fixture
def terminal_media_case(media_integration_harness):
    return media_integration_harness.terminal()


@pytest.fixture
def active_media_case(media_integration_harness):
    return media_integration_harness.active()


@pytest.fixture
def reconciliation_media_case(media_integration_harness):
    return media_integration_harness.reconciliation_required()


@pytest.fixture
def expired_media_case(media_integration_harness):
    return media_integration_harness.expired()


@pytest.fixture
def exhausted_media_case(media_integration_harness):
    return media_integration_harness.exhausted()


@pytest.fixture
def stale_media_case(media_integration_harness):
    return media_integration_harness.stale()


@pytest.fixture
def expired_before_provider_case(media_integration_harness):
    return media_integration_harness.expired_before_provider


@pytest.fixture
def expired_after_provider_case(media_integration_harness):
    return media_integration_harness.expired_after_provider()


@pytest.fixture
def lease_loss_disposition_case(media_integration_harness):
    return media_integration_harness.lease_loss_disposition


@pytest.fixture
def human_review_case(media_integration_harness):
    return media_integration_harness.human_review()


@pytest.fixture
def local_human_case(media_integration_harness):
    return media_integration_harness.local_human()
```

Move the Task 10 rollback test's `actor`, `customer_media_command`, `media_job_service`, and `failing_task_submission` setup into ordinary local variables built by `MediaIntegrationHarness.job_transaction_case()` so that test's only parameter is the shared `db_session`. `provider` and `decision` in Task 15 are supplied by their visible `pytest.mark.parametrize` decorators; `event_type` in Task 16 is similarly parametrized. No other custom test parameter remains.

- [ ] **Step 4: Run the new tests and verify the missing pipeline failure**

Run: `cd backend && uv run pytest tests/unit/media/test_qc_port.py tests/integration/media/test_media_terminal_billing.py tests/integration/media/test_media_lease_recovery.py tests/integration/media/test_media_human_review.py tests/integration/media/test_media_local.py -q`

Expected: collection fails because the QC port, lease coordinator, QC Worker, and human-review service are not implemented.

- [ ] **Step 5: Implement the replaceable QC contract, deterministic fake, and complete policy**

```python
# backend/src/ip_saas/modules/media/qc.py
from __future__ import annotations

from dataclasses import dataclass, replace
from enum import StrEnum
from typing import Protocol
from uuid import UUID

from ip_saas.common.errors import Conflict

from .enums import QcDecision


class QCSeverity(StrEnum):
    INFO = "info"
    WARNING = "warning"
    ERROR = "error"
    BLOCKER = "blocker"


@dataclass(frozen=True)
class QCCheck:
    code: str
    passed: bool
    severity: QCSeverity
    message: str
    logical_shot_id: UUID | None
    global_blocker: bool

    def __post_init__(self) -> None:
        if not self.code or not self.message:
            raise ValueError("QC checks require code and message")
        if self.passed and self.severity in {QCSeverity.ERROR, QCSeverity.BLOCKER}:
            raise ValueError("a passed QC check cannot have error severity")


@dataclass(frozen=True)
class QCRequest:
    job_id: UUID
    asset_id: UUID
    project_id: UUID
    technical_checks: tuple[QCCheck, ...]
    semantic_checks: tuple[QCCheck, ...]
    rights_checks: tuple[QCCheck, ...]
    product_truth_checks: tuple[QCCheck, ...]
    continuity_checks: tuple[QCCheck, ...]
    disclosure_checks: tuple[QCCheck, ...]

    def replace(self, **changes: object) -> "QCRequest":
        return replace(self, **changes)


@dataclass(frozen=True)
class QCOutcome:
    decision: QcDecision
    technical_checks: tuple[QCCheck, ...]
    semantic_checks: tuple[QCCheck, ...]
    rights_checks: tuple[QCCheck, ...]
    product_truth_checks: tuple[QCCheck, ...]
    continuity_checks: tuple[QCCheck, ...]
    disclosure_checks: tuple[QCCheck, ...]
    redo_logical_shot_ids: tuple[UUID, ...]


class MediaQCPort(Protocol):
    def inspect(self, request: QCRequest) -> QCOutcome:
        raise NotImplementedError


class CompositeMediaQCPort:
    REQUIRED_CODES = {
        "technical": {"decode_ok", "codec_ok", "subtitle_safe", "loudness_ok"},
        "semantic": {"shot_intent_match", "face_hand_lipsync_ok"},
        "rights": {"rights_active", "consent_snapshot_present"},
        "product_truth": {"product_reference_match"},
        "continuity": {"ip_character_continuity"},
        "disclosure": {"ai_label_visible", "ai_label_metadata"},
    }

    def inspect(self, request: QCRequest) -> QCOutcome:
        families = {
            "technical": request.technical_checks,
            "semantic": request.semantic_checks,
            "rights": request.rights_checks,
            "product_truth": request.product_truth_checks,
            "continuity": request.continuity_checks,
            "disclosure": request.disclosure_checks,
        }
        for name, required in self.REQUIRED_CODES.items():
            present = {item.code for item in families[name]}
            missing = required - present
            if missing:
                raise Conflict(f"QC family {name} lacks required checks: {sorted(missing)}")
        failed = [item for checks in families.values() for item in checks if not item.passed]
        if any(item.global_blocker or item.severity is QCSeverity.BLOCKER for item in failed):
            decision = QcDecision.FAIL
        elif failed:
            decision = QcDecision.REDO
        else:
            decision = QcDecision.PASS
        redo_ids = tuple(sorted(
            {item.logical_shot_id for item in failed if item.logical_shot_id is not None},
            key=str,
        ))
        if decision is QcDecision.REDO and not redo_ids:
            raise Conflict("redo QC must identify at least one logical shot")
        return QCOutcome(
            decision=decision,
            technical_checks=request.technical_checks,
            semantic_checks=request.semantic_checks,
            rights_checks=request.rights_checks,
            product_truth_checks=request.product_truth_checks,
            continuity_checks=request.continuity_checks,
            disclosure_checks=request.disclosure_checks,
            redo_logical_shot_ids=redo_ids,
        )


class DeterministicMediaQCFake:
    def __init__(self, outcomes: dict[UUID, QCOutcome]) -> None:
        self.outcomes = dict(outcomes)
        self.call_count = 0

    def inspect(self, request: QCRequest) -> QCOutcome:
        self.call_count += 1
        return self.outcomes[request.asset_id]


def checks_json(checks: tuple[QCCheck, ...]) -> list[dict[str, object]]:
    return [{
        "code": item.code, "passed": item.passed,
        "severity": item.severity.value, "message": item.message,
        "logical_shot_id": str(item.logical_shot_id)
        if item.logical_shot_id is not None else None,
        "global_blocker": item.global_blocker,
    } for item in checks]
```

The production `QCRequest` builder obtains technical checks from a controlled FFprobe/decode/loudness/subtitle inspector, semantic and face/hand/lip-sync checks from the configured replaceable visual inspector, rights and consent hashes from the current `MediaRightsGrant` snapshot, product checks against `product_reference_asset_id`, continuity checks against the frozen IP/character reference set, and disclosure checks from both visible-frame sampling and container metadata. A paid visual inspector, if enabled later, must be submitted as its own internal `Capability.QC` TaskRecord; it may not hide a second paid call inside this local port.

- [ ] **Step 6: Implement reusable attempt leases and background heartbeats**

```python
# backend/src/ip_saas/workers/media_lease.py
from enum import StrEnum
from threading import Event, Thread
from types import TracebackType
from uuid import UUID

from ip_saas.common.errors import Conflict, NotFound
from ip_saas.common.tasking import TaskStatus
from ip_saas.db.session import session_scope
from ip_saas.modules.tasks.models import TaskRecord
from ip_saas.modules.tasks.service import TaskSubmissionService


class ProcessDisposition(StrEnum):
    ACK = "ack"
    RETRY = "retry"


def disposition_after_lease_loss(
    task_id: UUID,
    captured_attempt_no: int,
) -> ProcessDisposition:
    with session_scope() as session:
        task = session.get(TaskRecord, task_id)
        if task is None:
            return ProcessDisposition.RETRY
        if task.status in {
            TaskStatus.SUCCEEDED, TaskStatus.FAILED, TaskStatus.CANCELLED,
        }:
            return ProcessDisposition.ACK
        if task.attempt_no > captured_attempt_no:
            return ProcessDisposition.ACK
        return ProcessDisposition.RETRY


class LeaseLost(Conflict):
    pass


class MediaLeaseCoordinator:
    def __init__(self, tasks: TaskSubmissionService) -> None:
        self.tasks = tasks

    def claim(self, task_id: UUID) -> tuple[ProcessDisposition, int | None]:
        try:
            with session_scope() as session:
                task = self.tasks.start(
                    session, task_id, lease_seconds=300, max_attempts=8
                )
                if task.status == TaskStatus.RECONCILIATION_REQUIRED:
                    return ProcessDisposition.RETRY, None
                if task.status in {
                    TaskStatus.SUCCEEDED, TaskStatus.FAILED, TaskStatus.CANCELLED,
                }:
                    return ProcessDisposition.ACK, None
                return ProcessDisposition.ACK, task.attempt_no
        except Conflict:
            return ProcessDisposition.RETRY, None

    def continue_attempt(
        self, task_id: UUID, attempt_no: int,
    ) -> ProcessDisposition:
        try:
            with session_scope() as session:
                self.tasks.heartbeat(
                    session, task_id, attempt_no, lease_seconds=300
                )
            return ProcessDisposition.ACK
        except Conflict:
            return disposition_after_lease_loss(task_id, attempt_no)


class LeaseHeartbeat:
    def __init__(self, tasks: TaskSubmissionService, task_id: UUID,
                 attempt_no: int, interval_seconds: int = 60) -> None:
        self.tasks = tasks
        self.task_id = task_id
        self.attempt_no = attempt_no
        self.interval_seconds = interval_seconds
        self.stop = Event()
        self.error: Exception | None = None
        self.thread = Thread(target=self._run, daemon=True)

    def _heartbeat_once(self) -> None:
        with session_scope() as session:
            self.tasks.heartbeat(
                session, self.task_id, self.attempt_no, lease_seconds=300
            )

    def _run(self) -> None:
        while not self.stop.wait(self.interval_seconds):
            try:
                self._heartbeat_once()
            except Exception as exc:
                self.error = exc
                self.stop.set()

    def __enter__(self) -> "LeaseHeartbeat":
        try:
            self._heartbeat_once()
        except Exception as exc:
            raise LeaseLost("media task lease was lost before provider call") from exc
        self.thread.start()
        return self

    def ensure_owned(self) -> None:
        if self.error is not None:
            raise LeaseLost("media task lease was lost after provider call") from self.error
        try:
            self._heartbeat_once()
        except Exception as exc:
            raise LeaseLost("media task lease was lost after provider call") from exc

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        traceback: TracebackType | None,
    ) -> bool:
        self.stop.set()
        self.thread.join(timeout=5)
        if exc_type is None:
            self.ensure_owned()
        return False
```

Replace the provider submit/poll call sites so long Volcengine calls keep the same attempt alive and a reclaimed stale result is discarded before persistence:

```diff
# backend/src/ip_saas/workers/media_submit.py
-from enum import StrEnum
 from hashlib import sha256
@@
-class ProcessDisposition(StrEnum):
-    ACK = "ack"
-    RETRY = "retry"
+from .media_lease import (
+    LeaseHeartbeat, LeaseLost, ProcessDisposition,
+    disposition_after_lease_loss,
+)
@@
         provider = self.providers.get(snapshot.provider)
         try:
-            result = provider.submit(snapshot.request)
+            with LeaseHeartbeat(
+                self.task_submission,
+                snapshot.task_record_id,
+                snapshot.attempt_no,
+            ) as heartbeat:
+                result = provider.submit(snapshot.request)
+                heartbeat.ensure_owned()
+        except LeaseLost:
+            return disposition_after_lease_loss(
+                snapshot.task_record_id, snapshot.attempt_no
+            )
         except Exception as error:
             self._record_transport_failure(snapshot, error)
             return ProcessDisposition.RETRY
         return self._apply(snapshot, result)
@@
             except Conflict:
-                return ProcessDisposition.ACK
+                return disposition_after_lease_loss(
+                    snapshot.task_record_id, snapshot.attempt_no
+                )

# backend/src/ip_saas/workers/media_poll.py
 from .media_submit import (
-    MediaSubmitWorker, ProcessDisposition, ProviderJobSnapshot, _locked_job,
+    MediaSubmitWorker, ProviderJobSnapshot, _locked_job,
     _provider_request,
 )
+from .media_lease import (
+    LeaseHeartbeat, LeaseLost, ProcessDisposition,
+    disposition_after_lease_loss,
+)
@@
         try:
-            result = provider.poll(poll_request)
+            with LeaseHeartbeat(
+                self.submit_worker.task_submission,
+                snapshot.task_record_id,
+                snapshot.attempt_no,
+            ) as heartbeat:
+                result = provider.poll(poll_request)
+                heartbeat.ensure_owned()
+        except LeaseLost:
+            return disposition_after_lease_loss(
+                snapshot.task_record_id, snapshot.attempt_no
+            )
         except Exception as error:
             self.submit_worker._record_transport_failure(snapshot, error)
             return ProcessDisposition.RETRY
         return self.submit_worker._apply(snapshot, result)
@@
             except Conflict:
-                return ProcessDisposition.ACK
+                return disposition_after_lease_loss(
+                    job.task_record_id, attempt_no
+                )
```

`claim()` is used only for a new/expired attempt. Broker continuation events carry their captured `attempt_no` and call `heartbeat()`. On CAS loss they use `disposition_after_lease_loss`: terminal or strictly newer ownership ACKs, while a missing task row, the same attempt with an expired lease, or same-attempt `RECONCILIATION_REQUIRED` RETRYs. The stage-specific Worker still locks `TaskRecord` and checks `status=running AND attempt_no=<captured>` immediately before every domain, billing, outbox, or delivery-authorizing write.

- [ ] **Step 7: Implement QC persistence, task-linked cost settlement, and attempt-scoped terminal writes**

```python
# backend/src/ip_saas/workers/media_qc.py
from datetime import timedelta
from decimal import Decimal
from uuid import UUID, uuid4

from sqlalchemy import select

from ip_saas.common.clock import Clock
from ip_saas.common.errors import Conflict, NotFound
from ip_saas.common.outbox import EventEnvelope, OutboxWriter
from ip_saas.common.tasking import TaskStatus
from ip_saas.db.session import session_scope
from ip_saas.modules.billing.models import ReconciliationStatus
from ip_saas.modules.billing.service import (
    BillingContext, BillingMode, BillingService, ProviderCostInput,
)
from ip_saas.modules.media.enums import JobStatus, ProductionStatus, QcDecision
from ip_saas.modules.media.models.jobs import GenerationJob, MediaAsset, QCReport
from ip_saas.modules.media.models.production import MediaProduction
from ip_saas.providers.media.base import canonical_provider_request_id
from ip_saas.modules.media.qc import MediaQCPort, QCOutcome, QCRequest, checks_json
from ip_saas.modules.tasks.models import TaskRecord
from ip_saas.modules.tasks.service import TaskSubmissionService

from .media_lease import (
    LeaseHeartbeat, LeaseLost, MediaLeaseCoordinator,
    MediaTerminalFailureService, ProcessDisposition,
    disposition_after_lease_loss,
)


class MediaQCRequestBuilder:
    def build(self, job: GenerationJob, asset: MediaAsset) -> QCRequest:
        raise NotImplementedError


class MediaQCWorker:
    def __init__(self, *, tasks: TaskSubmissionService, billing: BillingService,
                 qc: MediaQCPort, requests: MediaQCRequestBuilder,
                 terminal: MediaTerminalFailureService,
                 outbox: OutboxWriter, clock: Clock) -> None:
        self.tasks = tasks
        self.billing = billing
        self.qc = qc
        self.requests = requests
        self.terminal = terminal
        self.outbox = outbox
        self.clock = clock

    def run(self, job_id: UUID, asset_id: UUID,
            attempt_no: int) -> ProcessDisposition:
        with session_scope() as session:
            job = _job(session, job_id)
            task = _task(session, job.task_record_id)
            if task.status == TaskStatus.RECONCILIATION_REQUIRED:
                return ProcessDisposition.RETRY
            if task.status in {
                TaskStatus.SUCCEEDED, TaskStatus.FAILED, TaskStatus.CANCELLED,
            }:
                return ProcessDisposition.ACK
            if task.status != TaskStatus.RUNNING or task.attempt_no != attempt_no:
                return ProcessDisposition.ACK
            asset = session.scalar(select(MediaAsset).where(
                MediaAsset.id == asset_id,
                MediaAsset.job_id == job.id,
                MediaAsset.project_id == job.project_id,
            ))
            if asset is None:
                raise NotFound("QC asset not found")
            existing = session.scalar(select(QCReport).where(
                QCReport.job_id == job.id, QCReport.asset_id == asset.id,
            ))
            if existing is not None:
                return ProcessDisposition.ACK
            request = self.requests.build(job, asset)
        try:
            with LeaseHeartbeat(self.tasks, task.id, attempt_no) as heartbeat:
                outcome = self.qc.inspect(request)
                heartbeat.ensure_owned()
        except LeaseLost:
            return disposition_after_lease_loss(task.id, attempt_no)
        with session_scope() as session:
            try:
                self.tasks.heartbeat(
                    session, task.id, attempt_no, lease_seconds=300
                )
            except Conflict:
                return disposition_after_lease_loss(task.id, attempt_no)
            locked_task = _task(session, task.id, for_update=True)
            job = _job(session, job_id, for_update=True)
            asset = session.get(MediaAsset, asset_id)
            if (locked_task.status != TaskStatus.RUNNING
                    or locked_task.attempt_no != attempt_no
                    or asset is None or asset.project_id != job.project_id):
                return ProcessDisposition.ACK
            existing = session.scalar(select(QCReport).where(
                QCReport.job_id == job.id, QCReport.asset_id == asset.id,
            ))
            if existing is not None:
                return ProcessDisposition.ACK
            provider_cost = None
            if job.billable:
                try:
                    provider_cost = _provider_cost(job, locked_task)
                except Conflict:
                    job.last_error_code = "provider_usage_missing"
                    job.last_error_message = (
                        "provider completed without auditable terminal usage"
                    )
                    job.next_attempt_at = self.clock.now() + timedelta(seconds=30)
                    return ProcessDisposition.RETRY
            report = self._persist_report(session, job, asset, outcome)
            if job.billable:
                assert provider_cost is not None
                actual_amount = _approved_amount(job, locked_task, outcome.decision)
                context = BillingContext(
                    mode=BillingMode(locked_task.billing_mode),
                    hold_id=locked_task.billing_hold_id,
                )
                self.billing.settle_generation(
                    session, context, actual_amount, provider_cost,
                    f"settle:media:{locked_task.id}",
                )
            else:
                context = BillingContext(
                    mode=BillingMode(locked_task.billing_mode),
                    hold_id=locked_task.billing_hold_id,
                )
                self.billing.release_generation(
                    session, context, "local_media_stage_completed",
                    f"release:media:{locked_task.id}:local-success",
                )
            self.tasks.succeed(
                session, locked_task.id, attempt_no,
                {"job_id": str(job.id), "asset_id": str(asset.id),
                 "qc_report_id": str(report.id),
                 "qc_decision": outcome.decision.value,
                 "deliverable": False, "human_review_required": True},
            )
            self.outbox.add(session, EventEnvelope(
                event_id=uuid4(), event_type="media.human_review.requested",
                schema_version=1, aggregate_id=report.id,
                occurred_at=self.clock.now(),
                initiated_by_actor_id=job.initiated_by_actor_id,
                idempotency_key=f"media:human-review:{report.id}",
                payload={"project_id": str(job.project_id),
                         "production_id": str(job.production_id),
                         "job_id": str(job.id),
                         "task_id": str(locked_task.id),
                         "attempt_no": attempt_no,
                         "asset_id": str(asset.id),
                         "qc_report_id": str(report.id)},
            ))
        return ProcessDisposition.ACK

    def run_reclaimed(self, job_id: UUID) -> ProcessDisposition:
        with session_scope() as session:
            job = _job(session, job_id)
            asset = session.scalar(select(MediaAsset).where(
                MediaAsset.job_id == job.id,
                MediaAsset.project_id == job.project_id,
            ).order_by(MediaAsset.created_at.desc()).limit(1))
            if asset is None:
                raise NotFound("reclaimed QC job has no private asset")
            task_id, asset_id = job.task_record_id, asset.id
        disposition, attempt_no = MediaLeaseCoordinator(self.tasks).claim(task_id)
        return (
            disposition if attempt_no is None
            else self.run(job_id, asset_id, attempt_no)
        )

    def _persist_report(self, session, job: GenerationJob, asset: MediaAsset,
                        outcome: QCOutcome) -> QCReport:
        report = QCReport(
            account_id=job.account_id, project_id=job.project_id,
            production_id=job.production_id, job_id=job.id, asset_id=asset.id,
            decision=outcome.decision.value,
            technical_checks=checks_json(outcome.technical_checks),
            semantic_checks=checks_json(outcome.semantic_checks),
            rights_checks=checks_json(outcome.rights_checks),
            disclosure_checks=checks_json(outcome.disclosure_checks),
            product_truth_checks=checks_json(outcome.product_truth_checks),
            continuity_checks=checks_json(outcome.continuity_checks),
            redo_logical_shot_ids=[str(item) for item in outcome.redo_logical_shot_ids],
            checked_at=self.clock.now(), human_decision=None,
            human_review_note=None, human_reviewed_at=None, human_reviewed_by=None,
        )
        session.add(report)
        session.flush()
        job.provider_status = {
            QcDecision.PASS: JobStatus.PASSED,
            QcDecision.REDO: JobStatus.REDO,
            QcDecision.FAIL: JobStatus.FAILED,
        }[outcome.decision].value
        production = session.get(MediaProduction, job.production_id)
        if production is None or production.project_id != job.project_id:
            raise Conflict("media job lost its production")
        production.status = ProductionStatus.QC_PENDING.value
        return report


def _provider_cost(job: GenerationJob, task: TaskRecord) -> ProviderCostInput:
    values = (
        job.provider_request_id,
        job.native_quantity, job.native_unit, job.supplier_amount_minor,
        job.supplier_currency, job.provider_cost_amount_fen,
        job.reconciliation_status,
    )
    if any(item is None for item in values):
        raise Conflict("provider terminal usage is incomplete; asset is not deliverable")
    assert job.provider_request_id is not None
    assert job.native_quantity is not None
    assert job.native_unit is not None
    assert job.supplier_amount_minor is not None
    assert job.supplier_currency is not None
    assert job.provider_cost_amount_fen is not None
    assert job.reconciliation_status is not None
    return ProviderCostInput(
        provider=job.provider,
        provider_request_id=canonical_provider_request_id(job.provider_request_id),
        capability=job.capability,
        model_id=job.model_id, model_version=job.model_version,
        native_quantity=Decimal(job.native_quantity),
        native_unit=job.native_unit,
        supplier_amount_minor=job.supplier_amount_minor,
        supplier_currency=job.supplier_currency,
        amount_fen=job.provider_cost_amount_fen,
        reconciliation_status=ReconciliationStatus(job.reconciliation_status),
        task_id=task.id,
    )


def _approved_amount(job: GenerationJob, task: TaskRecord,
                     decision: QcDecision) -> int:
    if decision is not QcDecision.PASS:
        return 0
    billing = job.request_payload.get("_billing")
    if not isinstance(billing, dict):
        raise Conflict("media job lost its frozen billing quote")
    key = "credit_units" if BillingMode(task.billing_mode) is BillingMode.CUSTOMER_CREDIT else "amount_fen"
    value = billing.get(key)
    if not isinstance(value, int) or value <= 0:
        raise Conflict("media job has an invalid frozen approved amount")
    return value


def _job(session, job_id: UUID, *, for_update: bool = False) -> GenerationJob:
    statement = select(GenerationJob).where(GenerationJob.id == job_id)
    if for_update:
        statement = statement.with_for_update()
    job = session.scalar(statement)
    if job is None:
        raise NotFound("media job not found")
    return job


def _task(session, task_id: UUID, *, for_update: bool = False) -> TaskRecord:
    statement = select(TaskRecord).where(TaskRecord.id == task_id)
    if for_update:
        statement = statement.with_for_update()
    task = session.scalar(statement)
    if task is None:
        raise NotFound("media task not found")
    return task
```

For a real provider result, `PASS` settles the frozen customer-credit or internal-CNY amount. Machine `REDO`/`FAIL` with complete usage and a canonical `GenerationJob.provider_request_id` calls the same `settle_generation()` with `actual_amount=0`; Plan 01 releases the customer hold but still writes the supplier `ProviderCostEntry(task_id=..., provider_request_id=...)`. `provider_task_id` is never copied into that ledger field. Thus every known real-provider completion is reconciled exactly once, while a non-deliverable result charges the customer zero. A failure with no complete usage or no canonical supplier request identity RETRYs and preserves the hold regardless of what the caller believes happened; only after `start()` enters `RECONCILIATION_REQUIRED` may the independent scanner release using Task 17's durable zero-send evidence. A send-marked failure with missing terminal usage/request identity is never treated as zero-call and remains non-deliverable until complete supplier evidence exists.

- [ ] **Step 8: Execute a human production package as a local leased task**

```python
# backend/src/ip_saas/workers/media_local.py
from uuid import UUID

from sqlalchemy import func, select

from ip_saas.common.errors import Conflict, NotFound
from ip_saas.common.tasking import TaskStatus
from ip_saas.db.session import session_scope
from ip_saas.modules.billing.service import BillingContext, BillingMode, BillingService
from ip_saas.modules.media.enums import JobStatus
from ip_saas.modules.media.models.jobs import GenerationJob
from ip_saas.modules.media.models.production import (
    HumanProductionPackage, MediaProduction, Scene, Shot,
)
from ip_saas.modules.media.schemas import SceneDraft, ShotDraft
from ip_saas.modules.media.storyboard import build_human_package
from ip_saas.modules.tasks.service import TaskSubmissionService

from .media_lease import ProcessDisposition


class HumanPackageTaskHandler:
    def __init__(self, tasks: TaskSubmissionService,
                 billing: BillingService) -> None:
        self.tasks = tasks
        self.billing = billing

    def __call__(self, task_id: UUID) -> None:
        disposition = self.run(task_id)
        if disposition is ProcessDisposition.RETRY:
            raise RuntimeError("human package task requested retry")

    def run(self, task_id: UUID) -> ProcessDisposition:
        with session_scope() as session:
            job = session.scalar(select(GenerationJob).where(
                GenerationJob.task_record_id == task_id
            ).with_for_update())
            if job is None:
                raise NotFound("human package job not found")
            if job.capability != "human_package" or job.billable:
                raise Conflict("job is not a local human package")
            try:
                task = self.tasks.start(
                    session, task_id, lease_seconds=300, max_attempts=8
                )
            except Conflict:
                return ProcessDisposition.RETRY
            if task.status == TaskStatus.RECONCILIATION_REQUIRED:
                return ProcessDisposition.RETRY
            if task.status in {
                TaskStatus.SUCCEEDED, TaskStatus.FAILED, TaskStatus.CANCELLED,
            }:
                return ProcessDisposition.ACK
            attempt_no = task.attempt_no
            production = session.get(MediaProduction, job.production_id)
            if production is None or production.project_id != job.project_id:
                raise Conflict("human package lost production lineage")
            scenes = list(session.scalars(select(Scene).where(
                Scene.production_id == production.id
            ).order_by(Scene.sequence)))
            shots = list(session.scalars(select(Shot).where(
                Shot.production_id == production.id,
                Shot.is_current.is_(True),
            ).order_by(Shot.sequence)))
            scene_drafts = [SceneDraft(
                sequence=row.sequence, title=row.title,
                purpose=row.purpose, location=row.location,
            ) for row in scenes]
            shots_by_scene = {
                scene.sequence: [ShotDraft(
                    sequence=row.sequence, title=row.title,
                    source=row.source, duration_ms=row.duration_ms,
                    prompt=row.prompt, spoken_text=row.spoken_text,
                    character_id=row.character_id, voice_id=row.voice_id,
                    product_reference_asset_id=row.product_reference_asset_id,
                ) for row in shots if row.scene_id == scene.id]
                for scene in scenes
            }
            payload = build_human_package(scene_drafts, shots_by_scene)
            next_revision = int(session.scalar(select(func.coalesce(
                func.max(HumanProductionPackage.revision), 0
            )).where(
                HumanProductionPackage.production_id == production.id
            )) or 0) + 1
            package = HumanProductionPackage(
                account_id=job.account_id, project_id=job.project_id,
                production_id=production.id, revision=next_revision,
                call_sheet=payload["call_sheet"],
                shot_list=payload["shot_list"],
                performance_notes=payload["performance_notes"],
                sound_notes=payload["sound_notes"],
                props_and_wardrobe=payload["props_and_wardrobe"],
                low_cost_alternatives=payload["low_cost_alternatives"],
            )
            session.add(package)
            session.flush()
            context = BillingContext(
                mode=BillingMode(task.billing_mode),
                hold_id=task.billing_hold_id,
            )
            self.billing.release_generation(
                session, context, "local_human_package_completed",
                f"release:media:{task.id}:human-package",
            )
            job.provider_status = JobStatus.PASSED.value
            self.tasks.succeed(
                session, task.id, attempt_no,
                {"job_id": str(job.id), "human_package_id": str(package.id)},
            )
            return ProcessDisposition.ACK
```

Register `HumanPackageTaskHandler` under `human_package` in Plan 01's shared `RocketMQTaskConsumer` map. It uses no model, network, TOS, or supplier cost; the local package, hold release, job projection, and attempt-scoped `succeed()` commit atomically. A duplicate terminal task ACKs and cannot create revision 2. Human, AI, and mixed modes therefore enter the same TaskRecord surface without routing local work into a paid provider adapter.

- [ ] **Step 9: Wrap copy and finishing in heartbeats and durable three-phase writes**

Add these concrete Worker rules while retaining `copy_provider_result()` and `finish_in_workspace()` as pure executors:

```python
# backend/src/ip_saas/workers/media_copy.py (replace broker-facing class)
from .media_lease import (
    LeaseHeartbeat, LeaseLost, MediaLeaseCoordinator, ProcessDisposition,
    disposition_after_lease_loss,
)


class MediaCopyWorker:
    def __init__(self, *, tasks, service: MediaCopyService) -> None:
        self.tasks = tasks
        self.service = service

    def run(self, job_id: UUID, attempt_no: int) -> ProcessDisposition:
        with session_scope() as session:
            prepared = self.service.prepare(session, job_id, attempt_no)
            if prepared is None:
                return ProcessDisposition.ACK
        try:
            with LeaseHeartbeat(self.tasks, prepared.task_record_id, attempt_no) as heartbeat:
                result = self.service.execute(prepared)
                heartbeat.ensure_owned()
        except LeaseLost:
            return disposition_after_lease_loss(
                prepared.task_record_id, attempt_no
            )
        except Exception:
            self.service.record_retry(job_id, attempt_no, "private_copy_failed")
            return ProcessDisposition.RETRY
        with session_scope() as session:
            try:
                self.tasks.heartbeat(
                    session, prepared.task_record_id, attempt_no, lease_seconds=300
                )
            except Conflict:
                return disposition_after_lease_loss(
                    prepared.task_record_id, attempt_no
                )
            self.service.persist(session, prepared, result, attempt_no)
        return ProcessDisposition.ACK

    def run_reclaimed(self, job_id: UUID) -> ProcessDisposition:
        task_id = self.service.task_id(job_id)
        disposition, attempt_no = MediaLeaseCoordinator(self.tasks).claim(task_id)
        return disposition if attempt_no is None else self.run(job_id, attempt_no)
```

`MediaCopyService.prepare()` locks the project-owned `GenerationJob` and current `TaskRecord`, requires `status=running` and the same `attempt_no`, returns an immutable `CopyCommand`, and returns `None` when the one `MediaAsset(job_id=...)` already exists. `execute()` calls the bounded allow-listed fetch plus private versioned TOS upload outside every database transaction. `persist()` repeats the task CAS check under row locks, inserts the single asset, clears the encrypted temporary URL, changes the job to `copied_pending_qc`, and emits `media.qc.requested` with `job_id`, `asset_id`, and `attempt_no`. A stale execution may leave an unreferenced private TOS version for lifecycle cleanup, but cannot create a database asset, ledger row, outbox delivery event, or browser URL.

```python
# backend/src/ip_saas/workers/media_finish.py (append the broker-facing class)
from .media_lease import (
    LeaseHeartbeat, LeaseLost, MediaLeaseCoordinator, ProcessDisposition,
    disposition_after_lease_loss,
)


class MediaFinishWorker:
    def __init__(self, *, tasks, service, executor, outbox, clock) -> None:
        self.tasks = tasks
        self.service = service
        self.executor = executor
        self.outbox = outbox
        self.clock = clock

    def run(self, job_id: UUID, attempt_no: int) -> ProcessDisposition:
        task_id = self.service.task_id(job_id)
        with session_scope() as session:
            try:
                self.tasks.heartbeat(
                    session, task_id, attempt_no,
                    lease_seconds=300,
                )
            except Conflict:
                return disposition_after_lease_loss(task_id, attempt_no)
            plan = self.service.prepare_finish(session, job_id, attempt_no)
        try:
            with LeaseHeartbeat(self.tasks, plan.task_record_id, attempt_no) as heartbeat:
                stored = self.executor.finish_and_store(plan)
                heartbeat.ensure_owned()
        except LeaseLost:
            return disposition_after_lease_loss(plan.task_record_id, attempt_no)
        except Exception:
            self.service.record_finish_retry(job_id, attempt_no, "finishing_failed")
            return ProcessDisposition.RETRY
        with session_scope() as session:
            try:
                self.tasks.heartbeat(
                    session, plan.task_record_id, attempt_no, lease_seconds=300
                )
            except Conflict:
                return disposition_after_lease_loss(
                    plan.task_record_id, attempt_no
                )
            asset = self.service.persist_finished_asset(
                session, plan, stored, attempt_no
            )
            self.outbox.add(session, EventEnvelope(
                event_id=uuid4(), event_type="media.qc.requested",
                schema_version=1, aggregate_id=job_id,
                occurred_at=self.clock.now(),
                initiated_by_actor_id=plan.initiated_by_actor_id,
                idempotency_key=f"media:finish:{job_id}:attempt:{attempt_no}:qc",
                payload={"job_id": str(job_id),
                         "task_id": str(plan.task_record_id),
                         "asset_id": str(asset.id),
                         "attempt_no": attempt_no},
            ))
        return ProcessDisposition.ACK

    def run_reclaimed(self, job_id: UUID) -> ProcessDisposition:
        disposition, attempt_no = MediaLeaseCoordinator(self.tasks).claim(
            self.service.task_id(job_id)
        )
        return disposition if attempt_no is None else self.run(job_id, attempt_no)
```

`prepare_finish()` atomically changes `finishing_status` from `pending|failed` to `running` and increments `finishing_attempt_count`; `finish_and_store()` uses a fresh `mkdtemp` workspace, controlled TOS downloads, the Task 14 FFmpeg vector, private TOS upload, and unconditional workspace deletion; `persist_finished_asset()` requires the current attempt, inserts one `FINAL_VIDEO`, sets `finishing_status=succeeded`, `finished_asset_id`, and `provider_status=copied_pending_qc`. A failure sets durable `finishing_status=failed`, `last_error_code`, and `next_attempt_at` only if the same attempt still owns the task. Continuation messages call `run(job_id, attempt_no)`; only the expired-lease scanner calls `run_reclaimed()`. The Worker does not mark the task terminal while another retry is allowed; QC owns a successful terminal transition and the terminal failure service below owns exhausted/cancelled transitions.

- [ ] **Step 10: Implement explicit human pass/redo/fail without mutating QC evidence**

```python
# backend/src/ip_saas/modules/media/service.py (append)
from uuid import UUID, uuid4

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.common.clock import Clock
from ip_saas.common.errors import Conflict, NotFound
from ip_saas.common.outbox import EventEnvelope, OutboxWriter
from ip_saas.modules.accounts.context import ActorContext
from ip_saas.modules.projects.access import ProjectAccessService

from .enums import (
    AssetKind, HumanReviewDecision, JobStatus, ProductionStatus, QcDecision,
)
from .models.jobs import GenerationJob, MediaAsset, QCReport
from .models.production import MediaProduction
from .schemas import QcHumanReviewRequest


class MediaHumanReviewService:
    BLOCKING_CODES = {
        "decode_ok", "rights_active", "consent_snapshot_present",
        "product_reference_match", "ai_label_visible", "ai_label_metadata",
    }

    def __init__(self, access: ProjectAccessService, clock: Clock,
                 audit: AuditWriter, outbox: OutboxWriter) -> None:
        self.access = access
        self.clock = clock
        self.audit = audit
        self.outbox = outbox

    def review(self, session: Session, actor: ActorContext, project_id: UUID,
               report_id: UUID, command: QcHumanReviewRequest) -> QCReport:
        self.access.require_editor(session, actor, project_id)
        report = session.scalar(select(QCReport).where(
            QCReport.id == report_id, QCReport.project_id == project_id,
        ).with_for_update())
        if report is None:
            raise NotFound("QC report not found")
        if report.human_decision is not None:
            if (report.human_decision == command.decision.value
                    and report.human_review_note == command.note):
                return report
            raise Conflict("QC report already has a different human decision")
        failed_codes = {
            item["code"]
            for family in (
                report.technical_checks, report.semantic_checks,
                report.rights_checks, report.product_truth_checks,
                report.continuity_checks, report.disclosure_checks,
            )
            for item in family if not item["passed"]
        }
        if (command.decision is HumanReviewDecision.PASS
                and (report.decision != QcDecision.PASS.value
                     or failed_codes.intersection(self.BLOCKING_CODES))):
            raise Conflict("blocking QC evidence cannot be overridden to pass")
        job = session.get(GenerationJob, report.job_id)
        production = session.get(MediaProduction, report.production_id)
        asset = session.get(MediaAsset, report.asset_id)
        if (job is None or production is None or asset is None
                or job.project_id != project_id or production.project_id != project_id
                or asset.project_id != project_id):
            raise Conflict("QC report lost its media lineage")
        report.human_decision = command.decision.value
        report.human_review_note = command.note
        report.human_reviewed_at = self.clock.now()
        report.human_reviewed_by = actor.actor_id
        if command.decision is HumanReviewDecision.PASS:
            job.provider_status = JobStatus.PASSED.value
            if asset.kind == AssetKind.FINAL_VIDEO.value:
                production.final_asset_id = asset.id
                production.status = ProductionStatus.READY.value
            elif asset.kind == AssetKind.LOW_RES_PREVIEW.value:
                production.status = ProductionStatus.LOW_RES_READY.value
        elif command.decision is HumanReviewDecision.REDO:
            job.provider_status = JobStatus.REDO.value
            production.status = ProductionStatus.PARTIAL_REDO.value
            self.outbox.add(session, EventEnvelope(
                event_id=uuid4(), event_type="media.redo.requested",
                schema_version=1, aggregate_id=report.id,
                occurred_at=self.clock.now(), initiated_by_actor_id=actor.actor_id,
                idempotency_key=f"media:human-redo:{report.id}",
                payload={"project_id": str(project_id),
                         "production_id": str(production.id),
                         "qc_report_id": str(report.id),
                         "logical_shot_ids": report.redo_logical_shot_ids},
            ))
        else:
            job.provider_status = JobStatus.FAILED.value
            production.status = ProductionStatus.FAILED.value
        self.audit.write(
            session, actor=actor, action="media.qc.human_reviewed",
            target_type="media_qc_report", target_id=report.id,
            project_id=project_id,
            metadata={"decision": command.decision.value},
        )
        return report
```

This is an update only to the mutable human-decision fields on a machine-evidence report; technical, semantic, rights/consent, product-truth, continuity, disclosure, and redo-shot evidence never changes. A redo creates a new shot revision and new TaskRecord through Task 13; it never rewinds or reuses the completed attempt.

- [ ] **Step 11: Reconcile supplier cost before downstream exhaustion or cancellation fails the task**

Append this exact terminal helper to `backend/src/ip_saas/workers/media_lease.py` and inject it into copy, finishing, QC, cancellation, and recovery workers:

```python
from decimal import Decimal
from uuid import uuid4

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.clock import Clock
from ip_saas.common.outbox import EventEnvelope, OutboxWriter
from ip_saas.modules.billing.models import ReconciliationStatus
from ip_saas.modules.billing.service import (
    BillingContext, BillingMode, BillingService, ProviderCostInput,
)
from ip_saas.modules.media.enums import JobStatus, ProductionStatus
from ip_saas.modules.media.models.jobs import GenerationJob
from ip_saas.modules.media.models.production import MediaProduction
from ip_saas.providers.media.base import canonical_provider_request_id


class MediaTerminalFailureService:
    def __init__(self, tasks: TaskSubmissionService,
                 billing: BillingService, outbox: OutboxWriter,
                 clock: Clock) -> None:
        self.tasks = tasks
        self.billing = billing
        self.outbox = outbox
        self.clock = clock

    def fail_owned(
        self, session: Session, job: GenerationJob, task: TaskRecord,
        attempt_no: int, error_code: str, error_message: str,
    ) -> None:
        locked_task = session.scalar(select(TaskRecord).where(
            TaskRecord.id == task.id
        ).with_for_update())
        locked_job = session.scalar(select(GenerationJob).where(
            GenerationJob.id == job.id
        ).with_for_update())
        if (locked_task is None or locked_job is None
                or locked_task.status != TaskStatus.RUNNING
                or locked_task.attempt_no != attempt_no):
            raise LeaseLost("stale media attempt cannot fail or reconcile")
        context = BillingContext(
            mode=BillingMode(locked_task.billing_mode),
            hold_id=locked_task.billing_hold_id,
        )
        incurred = all(value is not None for value in (
            locked_job.provider_request_id,
            locked_job.native_quantity, locked_job.native_unit,
            locked_job.supplier_amount_minor, locked_job.supplier_currency,
            locked_job.provider_cost_amount_fen,
            locked_job.reconciliation_status,
        ))
        if incurred:
            assert locked_job.provider_request_id is not None
            assert locked_job.native_quantity is not None
            assert locked_job.native_unit is not None
            assert locked_job.supplier_amount_minor is not None
            assert locked_job.supplier_currency is not None
            assert locked_job.provider_cost_amount_fen is not None
            assert locked_job.reconciliation_status is not None
            provider_cost = ProviderCostInput(
                provider=locked_job.provider,
                provider_request_id=canonical_provider_request_id(
                    locked_job.provider_request_id
                ),
                capability=locked_job.capability,
                model_id=locked_job.model_id,
                model_version=locked_job.model_version,
                native_quantity=Decimal(locked_job.native_quantity),
                native_unit=locked_job.native_unit,
                supplier_amount_minor=locked_job.supplier_amount_minor,
                supplier_currency=locked_job.supplier_currency,
                amount_fen=locked_job.provider_cost_amount_fen,
                reconciliation_status=ReconciliationStatus(
                    locked_job.reconciliation_status
                ),
                task_id=locked_task.id,
            )
            self.billing.settle_generation(
                session, context, 0, provider_cost,
                f"settle:media:{locked_task.id}:terminal-failure",
            )
        else:
            raise Conflict(
                "supplier usage is unresolved; keep the hold and retry for reconciliation"
            )
        locked_job.provider_status = JobStatus.FAILED.value
        locked_job.last_error_code = error_code
        locked_job.last_error_message = error_message[:500]
        production = session.get(MediaProduction, locked_job.production_id)
        if production is not None and production.project_id == locked_job.project_id:
            production.status = (
                ProductionStatus.PARTIAL_REDO.value
                if locked_job.shot_id is not None
                else ProductionStatus.FAILED.value
            )
        self.tasks.fail(
            session, locked_task.id, attempt_no, error_code, error_message[:500]
        )
        self.outbox.add(session, EventEnvelope(
            event_id=uuid4(), event_type="media.job.failed",
            schema_version=1, aggregate_id=locked_job.id,
            occurred_at=self.clock.now(),
            initiated_by_actor_id=locked_job.initiated_by_actor_id,
            idempotency_key=(
                f"media:job:{locked_job.id}:attempt:{attempt_no}:failed"
            ),
            payload={"job_id": str(locked_job.id),
                     "task_id": str(locked_task.id),
                     "attempt_no": attempt_no,
                     "error_code": error_code},
        ))
```

Inject this same terminal helper into `MediaSubmitWorker` and replace its earlier release-only provider-failure method. This covers both synchronous Seedream/TTS terminal responses and asynchronous Seedance poll responses. The caller invokes it only when normalized supplier usage is complete. A response with absent or partial usage returns RETRY before this helper and keeps the hold active; retry exhaustion later enters `RECONCILIATION_REQUIRED`, where only Plan 01's scanner may release after Task 17's journal proves zero provider sends or may batch-record the complete supplier cost set:

```diff
# backend/src/ip_saas/workers/media_submit.py
+from .media_lease import MediaTerminalFailureService
@@
     def __init__(
         self,
         providers: MediaProviderRegistry,
         task_submission: TaskSubmissionService,
         billing: BillingService,
+        terminal: MediaTerminalFailureService,
         outbox: OutboxWriter,
         cipher: UrlCipher,
         clock: Clock,
@@
         self.task_submission = task_submission
         self.billing = billing
+        self.terminal = terminal
@@
     def _fail_locked(
         self,
         session: Session,
         job: GenerationJob,
         task: TaskRecord,
         attempt_no: int,
         error_code: str,
         error_message: str,
     ) -> None:
-        context = BillingContext(
-            mode=BillingMode(task.billing_mode), hold_id=task.billing_hold_id,
-        )
-        self.billing.release_generation(
-            session, context, error_code,
-            f"release:media:{task.id}:{error_code}",
-        )
-        self.task_submission.fail(
-            session, task.id, attempt_no, error_code, error_message,
-        )
-        job.provider_status = JobStatus.FAILED.value
-        job.last_error_code = error_code
-        job.last_error_message = error_message[:500]
-        self._emit(
-            session, job, attempt_no, "media.job.failed", "failed",
-            {"error_code": error_code},
+        self.terminal.fail_owned(
+            session,
+            job,
+            task,
+            attempt_no,
+            error_code,
+            error_message,
         )
```

Production composition and the integration harness construct one `MediaTerminalFailureService(tasks, billing, outbox, clock)` and pass that exact instance into submit, copy, finishing, QC, cancellation, and recovery paths. No Worker keeps the earlier release-only branch. A provider-returned failure without complete usage uses the same RETRY/reconciliation path as a provider-returned success without complete usage. This helper never releases an unproven zero-call path; Task 17's feature-owned attempt journal and the central Plan 01 scanner own that decision.

`record_retry()` for copy and finishing increments its durable stage counter only under the current task/attempt lock. When the configured stage maximum is reached and supplier usage plus canonical request identity are already complete, it calls `fail_owned()` in that same transaction instead of scheduling again. Explicit cancellation may do the same while the lease owner is current and complete usage/request identity are present. If Seedream, Seedance, or TTS returned complete usage and a real `provider_request_id` before copy, finishing, persistence, or QC later exhausted, `settle_generation(actual_amount=0, ProviderCostInput(task_id=task.id, provider_request_id=job.provider_request_id))` records that known supplier cost, releases the customer's remaining hold, and only then `fail()` CASes the task. `provider_task_id` remains a polling locator and is never substituted. With incomplete usage or request identity—including a true zero-send path not yet proven from the full journal—`fail_owned()` refuses to release; the caller RETRYs until Plan 01 enters `RECONCILIATION_REQUIRED`, and the scanner alone chooses zero-send release or complete supplier-cost finalization.

- [ ] **Step 12: Add expired-stage recovery and cancellation projection**

```python
# backend/src/ip_saas/workers/media_recovery.py
from sqlalchemy import select

from ip_saas.common.clock import Clock
from ip_saas.common.tasking import TaskStatus
from ip_saas.db.session import session_scope
from ip_saas.modules.media.enums import JobStatus
from ip_saas.modules.media.models.jobs import GenerationJob
from ip_saas.modules.tasks.models import TaskRecord

from .media_lease import ProcessDisposition


class MediaRecoveryWorker:
    def __init__(self, *, clock: Clock, submit, copy, finish, qc) -> None:
        self.clock = clock
        self.submit = submit
        self.copy = copy
        self.finish = finish
        self.qc = qc

    def run_once(self, limit: int = 50) -> int:
        with session_scope() as session:
            ids = list(session.execute(
                select(GenerationJob.id, GenerationJob.provider_status,
                       GenerationJob.finishing_status,
                       TaskRecord.id)
                .join(TaskRecord, TaskRecord.id == GenerationJob.task_record_id)
                .where(TaskRecord.status == TaskStatus.RUNNING,
                       TaskRecord.lease_expires_at <= self.clock.now())
                .order_by(TaskRecord.lease_expires_at)
                .limit(limit)
            ))
        handled = 0
        for job_id, status, finishing_status, task_id in ids:
            if finishing_status in {"pending", "running", "failed"}:
                disposition = self.finish.run_reclaimed(job_id)
            elif status in {JobStatus.HELD.value, JobStatus.SUBMITTED.value,
                            JobStatus.QUEUED.value, JobStatus.RUNNING.value}:
                disposition = self.submit.run(job_id)
            elif status == JobStatus.SUCCEEDED_PENDING_COPY.value:
                disposition = self.copy.run_reclaimed(job_id)
            elif status == JobStatus.COPIED_PENDING_QC.value:
                disposition = self.qc.run_reclaimed(job_id)
            else:
                continue
            if (
                disposition == ProcessDisposition.ACK
                or self._reconciliation_required(task_id)
            ):
                handled += 1
        return handled

    @staticmethod
    def _reconciliation_required(task_id) -> bool:
        with session_scope() as session:
            task = session.get(TaskRecord, task_id)
            return (
                task is not None
                and task.status == TaskStatus.RECONCILIATION_REQUIRED
            )
```

Every `run_reclaimed()` calls `MediaLeaseCoordinator.claim()` first; there is no max-attempt read or `fail_owned()` branch before Plan 01 `start()`. At the retry limit, `start()` alone moves the task to `RECONCILIATION_REQUIRED`, clears its lease, and keeps the original hold ACTIVE. The selected stage observes that state and returns RETRY without provider, domain, billing, QC, event, or delivery work. Only the independent Plan 01 scanner may later invoke the media no-call evidence port or the Task 17 supplier-cost manifest and central batch finalizer. Stage-specific exhaustion or an explicit cancellation may still use `MediaTerminalFailureService` while a live attempt is owned and supplier usage is already unambiguous; it is not a substitute for task retry-exhaustion reconciliation. Active leases are not selected by this scanner, and an ordinary duplicate consumer returns RETRY from `claim()`.

- [ ] **Step 13: Run lifecycle, QC, and finishing recovery tests**

Run: `cd backend && env -u ARK_API_KEY -u DOUBAO_TTS_APP_ID -u DOUBAO_TTS_ACCESS_KEY -u TOS_ACCESS_KEY -u TOS_SECRET_KEY uv run pytest tests/unit/media/test_qc_port.py tests/integration/media/test_media_terminal_billing.py tests/integration/media/test_media_lease_recovery.py tests/integration/media/test_media_human_review.py tests/integration/media/test_media_local.py tests/integration/media/test_provider_worker_idempotency.py tests/integration/media/test_copy_worker.py tests/integration/media/test_media_finishing.py -q`

Expected: all tests pass with zero network calls; every complete single-call provider result has its distinct task-linked cost, pass settles the approved amount, and machine redo/fail charges zero while preserving supplier cost. A sent result with missing/partial usage RETRYs with the hold active; Task 17 later proves the only zero-send release path. Active, same-expired, reconciliation, terminal, and strictly newer attempt dispositions match the frozen table; finishing state survives restart; and delivery remains blocked until human pass.

- [ ] **Step 14: Commit the lease-safe QC and terminal lifecycle**

```bash
git add backend/src/ip_saas/modules/media/qc.py backend/src/ip_saas/modules/media/service.py backend/src/ip_saas/workers/media_submit.py backend/src/ip_saas/workers/media_poll.py backend/src/ip_saas/workers/media_copy.py backend/src/ip_saas/workers/media_finish.py backend/src/ip_saas/workers/media_lease.py backend/src/ip_saas/workers/media_local.py backend/src/ip_saas/workers/media_qc.py backend/src/ip_saas/workers/media_recovery.py backend/tests/unit/media/test_qc_port.py backend/tests/integration/media/conftest.py backend/tests/integration/media/harness.py backend/tests/integration/media/test_media_terminal_billing.py backend/tests/integration/media/test_media_lease_recovery.py backend/tests/integration/media/test_media_human_review.py backend/tests/integration/media/test_media_local.py backend/tests/integration/media/test_provider_worker_idempotency.py backend/tests/integration/media/test_copy_worker.py backend/tests/integration/media/test_media_finishing.py
git commit -m "feat: close media qc billing and recovery lifecycle"
```

### Task 16: Expose the synchronous Studio API, strict media events, private delivery, and C-user UI

**Files:**
- Modify: `backend/src/ip_saas/modules/media/schemas.py`
- Modify: `backend/src/ip_saas/modules/media/service.py`
- Create: `backend/src/ip_saas/modules/media/events.py`
- Create: `backend/src/ip_saas/modules/media/dependencies.py`
- Create: `backend/src/ip_saas/modules/media/delivery.py`
- Create: `backend/src/ip_saas/modules/media/router.py`
- Modify: `backend/src/ip_saas/api.py`
- Modify: `backend/src/ip_saas/scripts/export_contracts.py`
- Create: `backend/tests/unit/media/test_delivery_policy.py`
- Create: `backend/tests/integration/media/test_media_router.py`
- Create: `backend/tests/contract/media/test_media_events.py`
- Create: `contracts/events/media.copy.requested.v1.json`
- Create: `contracts/events/media.job.failed.v1.json`
- Create: `contracts/events/media.job.poll.requested.v1.json`
- Create: `contracts/events/media.qc.requested.v1.json`
- Create: `contracts/events/media.human_review.requested.v1.json`
- Create: `contracts/events/media.redo.requested.v1.json`
- Modify: `frontend/src/lib/api/schema.d.ts`
- Create: `frontend/src/features/media/types.ts`
- Create: `frontend/src/features/media/api.ts`
- Create: `frontend/src/features/media/MediaStudio.tsx`
- Create: `frontend/src/app/(c-user)/projects/[projectId]/content/[contentVersionId]/studio/page.tsx`
- Create: `frontend/src/features/media/media-studio.test.tsx`
- Create: `frontend/tests/e2e/media-studio.spec.ts`

- [ ] **Step 1: Write failing delivery-policy, synchronous-route, and tenant tests**

```python
# backend/tests/unit/media/test_delivery_policy.py
from types import SimpleNamespace
from unittest.mock import Mock
from uuid import uuid4

import pytest

from ip_saas.common.errors import Conflict, NotFound
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.media.delivery import MediaDeliveryService


def actor(account_id):
    return ActorContext(uuid4(), account_id, ActorKind.C_USER)


def delivery_case(*, human_decision="pass", final=True):
    account_id, project_id, asset_id, production_id = (
        uuid4(), uuid4(), uuid4(), uuid4()
    )
    session = Mock()
    asset = SimpleNamespace(
        id=asset_id, project_id=project_id, production_id=production_id,
        tos_object_key="accounts/a/projects/p/final.mp4",
    )
    production = SimpleNamespace(
        id=production_id, project_id=project_id,
        final_asset_id=asset_id if final else uuid4(),
    )
    report = SimpleNamespace(
        asset_id=asset_id, project_id=project_id, decision="pass",
        human_decision=human_decision,
        technical_checks=[{"passed": True}],
        rights_checks=[{"passed": True}],
        product_truth_checks=[{"passed": True}],
        disclosure_checks=[{"passed": True}],
    )
    session.get.side_effect = lambda model, key: {
        asset_id: asset, production_id: production,
    }.get(key)
    session.scalar.return_value = report
    access = Mock()
    storage = Mock()
    storage.sign_get.return_value = "https://tos.example/signed?ttl=300"
    rights = Mock()
    service = MediaDeliveryService(access, storage, rights)
    return service, session, actor(account_id), project_id, asset_id, storage, rights


def test_delivery_requires_machine_and_human_pass_on_the_final_asset() -> None:
    service, session, current_actor, project_id, asset_id, storage, _rights = delivery_case(
        human_decision=None
    )
    with pytest.raises(Conflict, match="human pass"):
        service.issue(session, current_actor, project_id, asset_id)
    storage.sign_get.assert_not_called()


def test_delivery_returns_only_a_five_minute_signed_url() -> None:
    service, session, current_actor, project_id, asset_id, storage, rights = delivery_case()
    result = service.issue(session, current_actor, project_id, asset_id)
    assert result.expires_in_seconds == 300
    assert result.url.startswith("https://tos.example/signed")
    storage.sign_get.assert_called_once_with(
        "accounts/a/projects/p/final.mp4", expires_seconds=300
    )
    rights.require_current.assert_called_once()


def test_cross_project_asset_is_indistinguishable_from_missing() -> None:
    service, session, current_actor, project_id, asset_id, _storage, _rights = delivery_case()
    session.get.return_value = SimpleNamespace(id=asset_id, project_id=uuid4())
    session.get.side_effect = None
    with pytest.raises(NotFound, match="media asset"):
        service.issue(session, current_actor, project_id, asset_id)
```

```python
# backend/tests/integration/media/test_media_router.py
from time import monotonic
from unittest.mock import Mock
from uuid import uuid4

from fastapi import FastAPI
from fastapi.testclient import TestClient

from ip_saas.db.session import get_session
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.dependencies import get_actor
from ip_saas.modules.media.dependencies import MediaApiServices, get_media_services
from ip_saas.modules.media.router import router


def build_test_client(services: MediaApiServices) -> TestClient:
    app = FastAPI()
    app.include_router(router)
    actor = ActorContext(uuid4(), uuid4(), ActorKind.C_USER)
    app.dependency_overrides[get_session] = lambda: Mock()
    app.dependency_overrides[get_actor] = lambda: actor
    app.dependency_overrides[get_media_services] = lambda: services
    return TestClient(app)


def test_job_request_returns_202_without_calling_a_provider() -> None:
    orchestrator = Mock()
    orchestrator.enqueue.return_value = Mock(
        id=uuid4(), task_record_id=uuid4(), provider_status="held"
    )
    services = MediaApiServices.for_test(job_orchestrator=orchestrator)
    client = build_test_client(services)
    project_id, production_id, shot_id = uuid4(), uuid4(), uuid4()
    started = monotonic()
    response = client.post(
        f"/v1/projects/{project_id}/media/productions/{production_id}/jobs",
        json={
            "shot_id": str(shot_id), "capability": "low_res_video",
            "cost_approval": {"credit_units": 40, "quote_version": "price-v3"},
            "idempotency_key": "preview:shot-1:v1",
        },
    )
    assert response.status_code == 202
    assert monotonic() - started < 3
    assert response.json()["status"] == "queued"
    assert services.provider_call_count() == 0


def test_delivery_route_never_serializes_bucket_key_or_provider_url() -> None:
    delivery = Mock()
    delivery.issue.return_value = Mock(
        url="https://tos.example/private-signed", expires_in_seconds=300
    )
    client = build_test_client(MediaApiServices.for_test(delivery=delivery))
    project_id, asset_id = uuid4(), uuid4()
    response = client.get(
        f"/v1/projects/{project_id}/media/assets/{asset_id}/delivery-url"
    )
    assert response.status_code == 200
    assert set(response.json()) == {"url", "expires_in_seconds"}
    assert "bucket" not in response.text and "object_key" not in response.text


def test_unknown_fields_and_plan_06_identity_inputs_are_rejected() -> None:
    client = build_test_client(MediaApiServices.for_test())
    project_id, production_id = uuid4(), uuid4()
    response = client.post(
        f"/v1/projects/{project_id}/media/productions/{production_id}/jobs",
        json={
            "shot_id": str(uuid4()), "capability": "cloned_voice",
            "cost_approval": {"credit_units": 1},
            "idempotency_key": "forbidden-identity", "unknown": True,
        },
    )
    assert response.status_code == 422
```

The route test helper has no database or provider fixture: it overrides the two published FastAPI dependencies with explicit fakes in the same file. `db_session` and `pg_engine` used elsewhere are the Plan 01 fixtures in `backend/tests/conftest.py`; Pytest itself supplies `tmp_path` and `monkeypatch`.

- [ ] **Step 2: Run the backend tests and verify that delivery, dependencies, and router are absent**

Run: `cd backend && uv run pytest tests/unit/media/test_delivery_policy.py tests/integration/media/test_media_router.py -q`

Expected: collection fails because `media.delivery`, `media.dependencies`, and `media.router` do not exist.

- [ ] **Step 3: Add strict Studio request and response schemas**

Append to `backend/src/ip_saas/modules/media/schemas.py`:

```python
class SelectedPreviewRequest(StrictModel):
    selected_asset_by_shot: dict[UUID, UUID] = Field(min_length=1, max_length=500)


class MediaJobCreateRequest(StrictModel):
    shot_id: UUID | None = None
    capability: Literal[
        "storyboard_image", "low_res_video", "high_res_video", "standard_tts",
        "human_package",
    ]
    cost_approval: CostApprovalRequest
    idempotency_key: str = Field(min_length=8, max_length=160)


class MediaJobAccepted(StrictModel):
    job_id: UUID
    task_id: UUID
    status: Literal["queued"] = "queued"


class RedoShotJobRequest(StrictModel):
    reason: str = Field(min_length=1, max_length=500)
    capability: Literal["low_res_video", "high_res_video"]
    cost_approval: CostApprovalRequest
    idempotency_key: str = Field(min_length=8, max_length=160)


class RedoShotAccepted(MediaJobAccepted):
    shot_id: UUID


class ProductionView(StrictModel):
    id: UUID
    project_id: UUID
    content_version_id: UUID
    platform_variant_id: UUID
    mode: ProductionMode
    output_aspect_ratio: str
    status: str
    final_asset_id: UUID | None


class SceneView(StrictModel):
    id: UUID
    sequence: int
    title: str
    purpose: str
    location: str


class ShotView(StrictModel):
    id: UUID
    scene_id: UUID
    logical_shot_id: UUID
    revision: int
    sequence: int
    title: str
    source: ShotSource
    selected_asset_id: UUID | None


class AssetView(StrictModel):
    id: UUID
    job_id: UUID | None
    shot_id: UUID | None
    kind: str
    ai_disclosure: AiDisclosure
    content_type: str


class QcEvidenceView(StrictModel):
    id: UUID
    asset_id: UUID
    decision: QcDecision
    human_decision: HumanReviewDecision | None
    technical_checks: list[dict[str, Any]]
    semantic_checks: list[dict[str, Any]]
    rights_checks: list[dict[str, Any]]
    product_truth_checks: list[dict[str, Any]]
    continuity_checks: list[dict[str, Any]]
    disclosure_checks: list[dict[str, Any]]
    redo_logical_shot_ids: list[UUID]


class IdentityReadinessView(StrictModel):
    shot_id: UUID
    character_id: UUID | None
    voice_id: UUID | None
    voice_kind: VoiceKind | None
    ready: bool
    message: str


class StudioView(StrictModel):
    production: ProductionView
    scenes: list[SceneView]
    shots: list[ShotView]
    jobs: list[JobView]
    assets: list[AssetView]
    qc_reports: list[QcEvidenceView]
    identity_readiness: list[IdentityReadinessView]


class DeliveryUrlResponse(StrictModel):
    url: HttpUrl
    expires_in_seconds: Literal[300] = 300
```

Add `AiDisclosure` to the existing enum import in this file. These views intentionally omit TOS bucket/key/version, encrypted provider URL, raw provider payload, billing hold ID, credentials, and QC-internal prompts.

- [ ] **Step 4: Implement production/storyboard persistence and trusted job orchestration**

Append this complete block to `backend/src/ip_saas/modules/media/service.py`:

```python
from typing import Protocol

from ip_saas.modules.intelligence.models import ContentVersion, PlatformVariant

from .models.production import Scene
from .schemas import (
    CostApprovalRequest, MediaJobCreateRequest, ProductionCreate,
    RedoShotJobRequest, StoryboardCreate,
)


class MediaQuoteCatalog(Protocol):
    def quote(self, session: Session, project: IPProject,
              capability: Capability) -> MediaQuote:
        raise NotImplementedError


class ProjectBillingRouteResolver(Protocol):
    def resolve(self, session: Session, project: IPProject) -> BillingRoute:
        raise NotImplementedError


class MediaStudioService:
    def __init__(self, access: ProjectAccessService, audit: AuditWriter,
                 clock: Clock) -> None:
        self.access = access
        self.audit = audit
        self.clock = clock

    def create_production(
        self, session: Session, actor: ActorContext, project_id: UUID,
        command: ProductionCreate, idempotency_key: str,
    ) -> MediaProduction:
        project = self.access.require_editor(session, actor, project_id)
        existing = session.scalar(select(MediaProduction).where(
            MediaProduction.project_id == project_id,
            MediaProduction.idempotency_key == idempotency_key
        ))
        if existing is not None:
            if (existing.project_id != project_id
                    or existing.content_version_id != command.content_version_id
                    or existing.platform_variant_id != command.platform_variant_id
                    or existing.mode != command.mode.value
                    or existing.output_aspect_ratio != command.output_aspect_ratio):
                raise Conflict("production idempotency key has different parameters")
            return existing
        content = session.get(ContentVersion, command.content_version_id)
        variant = session.get(PlatformVariant, command.platform_variant_id)
        if (content is None or variant is None
                or content.ip_project_id != project_id
                or variant.ip_project_id != project_id
                or variant.content_version_id != content.id):
            raise NotFound("frozen content version or platform variant not found")
        production = MediaProduction(
            account_id=project.owner_account_id, project_id=project_id,
            content_version_id=content.id, platform_variant_id=variant.id,
            mode=command.mode.value,
            output_aspect_ratio=command.output_aspect_ratio,
            status=ProductionStatus.DRAFT.value,
            idempotency_key=idempotency_key,
            initiated_by_actor_id=actor.actor_id,
        )
        session.add(production)
        session.flush()
        self.audit.write(
            session, actor=actor, action="media.production.created",
            target_type="media_production", target_id=production.id,
            project_id=project_id,
            metadata={"content_version_id": str(content.id),
                      "platform_variant_id": str(variant.id)},
        )
        return production

    def create_storyboard(
        self, session: Session, actor: ActorContext, project_id: UUID,
        production_id: UUID, command: StoryboardCreate,
    ) -> MediaProduction:
        self.access.require_editor(session, actor, project_id)
        production = session.scalar(select(MediaProduction).where(
            MediaProduction.id == production_id,
            MediaProduction.project_id == project_id,
        ).with_for_update())
        if production is None:
            raise NotFound("media production not found")
        if production.status != ProductionStatus.DRAFT.value:
            raise Conflict("storyboard can only be created from draft")
        declared = {scene.sequence for scene in command.scenes}
        if set(command.shots) != declared:
            raise Conflict("shots must be grouped by every declared scene sequence")
        for scene_command in sorted(command.scenes, key=lambda item: item.sequence):
            scene = Scene(
                account_id=production.account_id, project_id=project_id,
                production_id=production.id, sequence=scene_command.sequence,
                title=scene_command.title, purpose=scene_command.purpose,
                location=scene_command.location,
            )
            session.add(scene)
            session.flush()
            for shot_command in command.shots[scene_command.sequence]:
                shot = Shot(
                    account_id=production.account_id, project_id=project_id,
                    production_id=production.id, scene_id=scene.id,
                    logical_shot_id=uuid4(), revision=1, supersedes_id=None,
                    is_current=True, sequence=shot_command.sequence,
                    title=shot_command.title, source=shot_command.source.value,
                    duration_ms=shot_command.duration_ms,
                    prompt=shot_command.prompt, spoken_text=shot_command.spoken_text,
                    character_id=shot_command.character_id,
                    voice_id=shot_command.voice_id,
                    product_reference_asset_id=shot_command.product_reference_asset_id,
                    selected_asset_id=None,
                )
                session.add(shot)
        production.status = ProductionStatus.STORYBOARD_READY.value
        self.audit.write(
            session, actor=actor, action="media.storyboard.created",
            target_type="media_production", target_id=production.id,
            project_id=project_id,
            metadata={"scene_count": len(command.scenes),
                      "shot_count": sum(map(len, command.shots.values()))},
        )
        return production

    def approve_storyboard(
        self, session: Session, actor: ActorContext, project_id: UUID,
        production_id: UUID,
    ) -> MediaProduction:
        self.access.require_editor(session, actor, project_id)
        production = session.scalar(select(MediaProduction).where(
            MediaProduction.id == production_id,
            MediaProduction.project_id == project_id,
        ).with_for_update())
        if production is None:
            raise NotFound("media production not found")
        if production.status == ProductionStatus.STORYBOARD_APPROVED.value:
            return production
        if production.status != ProductionStatus.STORYBOARD_READY.value:
            raise Conflict("storyboard is not ready for approval")
        production.status = ProductionStatus.STORYBOARD_APPROVED.value
        production.storyboard_approved_at = self.clock.now()
        self.audit.write(
            session, actor=actor, action="media.storyboard.approved",
            target_type="media_production", target_id=production.id,
            project_id=project_id,
        )
        return production


class MediaJobOrchestrator:
    def __init__(self, access: ProjectAccessService, jobs: MediaJobService,
                 quotes: MediaQuoteCatalog,
                 billing_routes: ProjectBillingRouteResolver,
                 review: MediaReviewService) -> None:
        self.access = access
        self.jobs = jobs
        self.quotes = quotes
        self.billing_routes = billing_routes
        self.review = review

    def enqueue(
        self, session: Session, actor: ActorContext, project_id: UUID,
        production_id: UUID, request: MediaJobCreateRequest,
    ) -> GenerationJob:
        project = self.access.require_editor(session, actor, project_id)
        capability = Capability(request.capability)
        quote = self.quotes.quote(session, project, capability)
        approval: CostApprovalRequest = request.cost_approval
        return self.jobs.create(session, actor, JobCreateCommand(
            project_id=project_id, production_id=production_id,
            shot_id=request.shot_id, capability=capability, quote=quote,
            billing_route=self.billing_routes.resolve(session, project),
            approved_credit_units=approval.credit_units,
            approved_amount_fen=approval.amount_fen,
            request_payload={"stage": capability.value},
            idempotency_key=request.idempotency_key,
        ))

    def redo(
        self, session: Session, actor: ActorContext, project_id: UUID,
        shot_id: UUID, request: RedoShotJobRequest,
    ) -> tuple[Shot, GenerationJob]:
        current = session.get(Shot, shot_id)
        if current is None or current.project_id != project_id:
            raise NotFound("media shot not found")
        replacement = self.review.redo_shot(
            session, actor, shot_id, reason=request.reason,
            origin=RedoOrigin.USER,
        )
        job = self.enqueue(
            session, actor, project_id, replacement.production_id,
            MediaJobCreateRequest(
                shot_id=replacement.id, capability=request.capability,
                cost_approval=request.cost_approval,
                idempotency_key=request.idempotency_key,
            ),
        )
        return replacement, job
```

`MediaQuoteCatalog` reads only an active, regression-passed registry entry and a server-side price version. `ProjectBillingRouteResolver` maps a C-user project to customer credit and the platform self-marketing project to its configured internal cost center. Neither value is accepted from JSON. `enqueue()` makes no provider, TOS, FFmpeg, or QC call; the shared `TaskSubmissionService` creates the hold, TaskRecord, `GenerationTaskRequestedV1` outbox row, and its `request_fingerprint` atomically.

- [ ] **Step 5: Implement the authorized private-delivery service**

```python
# backend/src/ip_saas/modules/media/delivery.py
from dataclasses import dataclass
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.errors import Conflict, NotFound
from ip_saas.modules.accounts.context import ActorContext
from ip_saas.modules.projects.access import ProjectAccessService
from ip_saas.providers.media.tos import PrivateStorage

from ip_saas.common.clock import Clock
from ip_saas.modules.intelligence.models import PlatformVariant

from .enums import HumanReviewDecision, QcDecision, RightsType, ShotSource
from .models.identity import CharacterProfile, MediaRightsGrant, VoiceAsset
from .models.jobs import MediaAsset, QCReport
from .models.production import MediaProduction, Shot
from .rights import RightsPolicy


@dataclass(frozen=True)
class DeliveryGrant:
    url: str
    expires_in_seconds: int = 300


class DeliveryRightsPort(Protocol):
    def require_current(self, session: Session,
                        production: MediaProduction) -> None:
        raise NotImplementedError


class CurrentDeliveryRightsService:
    def __init__(self, clock: Clock) -> None:
        self.clock = clock

    def require_current(self, session: Session,
                        production: MediaProduction) -> None:
        variant = session.get(PlatformVariant, production.platform_variant_id)
        if variant is None:
            raise Conflict("delivery lost its platform variant")
        grants = list(session.scalars(select(MediaRightsGrant).where(
            MediaRightsGrant.project_id == production.project_id
        )))
        shots = list(session.scalars(select(Shot).where(
            Shot.production_id == production.id,
            Shot.is_current.is_(True),
        )))
        policy = RightsPolicy(self.clock)
        for shot in shots:
            character = session.get(CharacterProfile, shot.character_id)
            voice = session.get(VoiceAsset, shot.voice_id)
            policy.require_shot_rights(
                character=character, voice=voice, grants=grants,
                source=ShotSource(shot.source), platform=variant.platform,
            )
            if shot.product_reference_asset_id is not None:
                active_product = policy._active_grant(
                    grants, RightsType.PRODUCT,
                    shot.product_reference_asset_id, variant.platform,
                )
                if active_product is None:
                    raise Conflict("active commercial product grant is required")


class MediaDeliveryService:
    def __init__(self, access: ProjectAccessService,
                 storage: PrivateStorage,
                 rights: DeliveryRightsPort) -> None:
        self.access = access
        self.storage = storage
        self.rights = rights

    def issue(self, session: Session, actor: ActorContext,
              project_id: UUID, asset_id: UUID) -> DeliveryGrant:
        self.access.require_viewer(session, actor, project_id)
        asset = session.get(MediaAsset, asset_id)
        if asset is None or asset.project_id != project_id:
            raise NotFound("media asset not found")
        production = session.get(MediaProduction, asset.production_id)
        if (production is None or production.project_id != project_id
                or production.final_asset_id != asset.id):
            raise Conflict("only the approved final asset can be delivered")
        report = session.scalar(select(QCReport).where(
            QCReport.project_id == project_id,
            QCReport.asset_id == asset.id,
        ).order_by(QCReport.checked_at.desc()).limit(1))
        if (report is None or report.decision != QcDecision.PASS.value
                or report.human_decision != HumanReviewDecision.PASS.value):
            raise Conflict("delivery requires machine pass and explicit human pass")
        families = (
            report.technical_checks, report.rights_checks,
            report.product_truth_checks, report.disclosure_checks,
        )
        if any(not bool(item.get("passed")) for family in families for item in family):
            raise Conflict("delivery evidence contains a blocking failure")
        self.rights.require_current(session, production)
        return DeliveryGrant(
            self.storage.sign_get(asset.tos_object_key, expires_seconds=300), 300
        )
```

The signed URL is generated only after authorization and is never persisted or placed in an outbox event. TOS stays private and versioned. Every issuance revalidates current character, uploaded-voice, and product grants, so an expiry or revocation blocks the next link even when an older QC report passed. Plan 06 later extends this gate with per-identity rights snapshot hashes without renaming this service.

- [ ] **Step 6: Add one injectable service bundle and complete synchronous FastAPI routes**

```python
# backend/src/ip_saas/modules/media/dependencies.py
from dataclasses import dataclass
from typing import Annotated

from fastapi import Depends, Request

from .delivery import CurrentDeliveryRightsService, MediaDeliveryService
from .service import (
    MediaHumanReviewService, MediaJobOrchestrator, MediaReviewService,
    MediaStudioService,
)


class UnconfiguredMediaService:
    def __getattr__(self, name: str):
        raise RuntimeError(f"media service is not configured: {name}")


@dataclass(frozen=True)
class MediaApiServices:
    studio: MediaStudioService | object
    review: MediaReviewService | object
    human_review: MediaHumanReviewService | object
    job_orchestrator: MediaJobOrchestrator | object
    delivery: MediaDeliveryService | object

    @classmethod
    def for_test(cls, **overrides: object) -> "MediaApiServices":
        unavailable = UnconfiguredMediaService()
        return cls(
            studio=overrides.get("studio", unavailable),
            review=overrides.get("review", unavailable),
            human_review=overrides.get("human_review", unavailable),
            job_orchestrator=overrides.get("job_orchestrator", unavailable),
            delivery=overrides.get("delivery", unavailable),
        )

    @staticmethod
    def provider_call_count() -> int:
        return 0


def get_media_services(request: Request) -> MediaApiServices:
    services = getattr(request.app.state, "media_services", None)
    if not isinstance(services, MediaApiServices):
        raise RuntimeError("media service bundle is not configured")
    return services


MediaServicesDep = Annotated[MediaApiServices, Depends(get_media_services)]


def compose_media_api_services(
    *, task_submission, quote_catalog, billing_routes, storage, clock,
) -> MediaApiServices:
    from ip_saas.common.audit import AuditWriter
    from ip_saas.common.outbox import OutboxWriter
    from ip_saas.modules.projects.access import ProjectAccessService
    from ip_saas.modules.media.service import MediaJobService

    access = ProjectAccessService()
    audit = AuditWriter(clock)
    outbox = OutboxWriter(clock)
    jobs = MediaJobService(access, task_submission, audit)
    review = MediaReviewService(access, audit, clock)
    return MediaApiServices(
        studio=MediaStudioService(access, audit, clock),
        review=review,
        human_review=MediaHumanReviewService(access, clock, audit, outbox),
        job_orchestrator=MediaJobOrchestrator(
            access, jobs, quote_catalog, billing_routes, review
        ),
        delivery=MediaDeliveryService(
            access, storage, CurrentDeliveryRightsService(clock)
        ),
    )
```

`MediaApiServices` contains only synchronous domain services; no provider adapter is reachable from a request handler. In the composition root, call `compose_media_api_services()` once with the same `TaskSubmissionService` used by Plan 01, the trusted active-model quote catalog, the owner-derived billing-route resolver, and configured private `TosPrivateStorage`, then pass the result to `create_app(media_services=...)`. The worker registry separately chooses either the real pinned adapters or `ScriptedFakeMediaProvider`; it is never stored in this API bundle.

```python
# backend/src/ip_saas/modules/media/router.py
from typing import Annotated
from uuid import UUID

from fastapi import APIRouter, Depends, Header, Query, status
from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.errors import Conflict, NotFound
from ip_saas.db.session import get_session
from ip_saas.modules.accounts.context import ActorContext
from ip_saas.modules.accounts.dependencies import get_actor

from .dependencies import MediaServicesDep
from .models.jobs import GenerationJob, MediaAsset, QCReport
from .models.production import MediaProduction, Scene, Shot
from .schemas import (
    AssetView, DeliveryUrlResponse, IdentityReadinessView, JobView, MediaJobAccepted,
    MediaJobCreateRequest, ProductionCreate, ProductionView,
    QcEvidenceView, QcHumanReviewRequest, SceneView,
    SelectedPreviewRequest, ShotView, StoryboardCreate, StudioView,
    RedoShotAccepted, RedoShotJobRequest,
)

router = APIRouter(prefix="/v1/projects/{project_id}/media", tags=["media"])
SessionDep = Annotated[Session, Depends(get_session)]
ActorDep = Annotated[ActorContext, Depends(get_actor)]


def production_view(row: MediaProduction) -> ProductionView:
    return ProductionView(
        id=row.id, project_id=row.project_id,
        content_version_id=row.content_version_id,
        platform_variant_id=row.platform_variant_id, mode=row.mode,
        output_aspect_ratio=row.output_aspect_ratio, status=row.status,
        final_asset_id=row.final_asset_id,
    )


@router.post("/productions", response_model=ProductionView, status_code=201)
def create_production(
    project_id: UUID, payload: ProductionCreate,
    idempotency_key: Annotated[str, Header(alias="Idempotency-Key",
                                           min_length=8, max_length=160)],
    session: SessionDep, actor: ActorDep, services: MediaServicesDep,
) -> ProductionView:
    return production_view(services.studio.create_production(
        session, actor, project_id, payload, idempotency_key
    ))


@router.post("/productions/{production_id}/storyboard",
             response_model=ProductionView, status_code=201)
def create_storyboard(
    project_id: UUID, production_id: UUID, payload: StoryboardCreate,
    session: SessionDep, actor: ActorDep, services: MediaServicesDep,
) -> ProductionView:
    return production_view(services.studio.create_storyboard(
        session, actor, project_id, production_id, payload
    ))


@router.post("/productions/{production_id}/storyboard/approve",
             response_model=ProductionView)
def approve_storyboard(
    project_id: UUID, production_id: UUID, session: SessionDep,
    actor: ActorDep, services: MediaServicesDep,
) -> ProductionView:
    return production_view(services.studio.approve_storyboard(
        session, actor, project_id, production_id
    ))


@router.post("/productions/{production_id}/previews/approve",
             response_model=ProductionView)
def approve_previews(
    project_id: UUID, production_id: UUID, payload: SelectedPreviewRequest,
    session: SessionDep, actor: ActorDep, services: MediaServicesDep,
) -> ProductionView:
    production = services.review.approve_low_resolution(
        session, actor, production_id, payload.selected_asset_by_shot
    )
    if production.project_id != project_id:
        raise RuntimeError("project-scoped review returned another project")
    return production_view(production)


@router.post("/productions/{production_id}/jobs",
             response_model=MediaJobAccepted, status_code=status.HTTP_202_ACCEPTED)
def create_job(
    project_id: UUID, production_id: UUID, payload: MediaJobCreateRequest,
    session: SessionDep, actor: ActorDep, services: MediaServicesDep,
) -> MediaJobAccepted:
    job = services.job_orchestrator.enqueue(
        session, actor, project_id, production_id, payload
    )
    return MediaJobAccepted(job_id=job.id, task_id=job.task_record_id)


@router.post("/shots/{shot_id}/redo",
             response_model=RedoShotAccepted,
             status_code=status.HTTP_202_ACCEPTED)
def redo_shot(
    project_id: UUID, shot_id: UUID, payload: RedoShotJobRequest,
    session: SessionDep, actor: ActorDep, services: MediaServicesDep,
) -> RedoShotAccepted:
    shot, job = services.job_orchestrator.redo(
        session, actor, project_id, shot_id, payload,
    )
    return RedoShotAccepted(
        shot_id=shot.id, job_id=job.id, task_id=job.task_record_id,
    )


@router.post("/qc-reports/{report_id}/review", response_model=QcEvidenceView)
def review_qc(
    project_id: UUID, report_id: UUID, payload: QcHumanReviewRequest,
    session: SessionDep, actor: ActorDep, services: MediaServicesDep,
) -> QcEvidenceView:
    return qc_view(services.human_review.review(
        session, actor, project_id, report_id, payload
    ))


@router.get("/assets/{asset_id}/delivery-url",
            response_model=DeliveryUrlResponse)
def delivery_url(
    project_id: UUID, asset_id: UUID, session: SessionDep,
    actor: ActorDep, services: MediaServicesDep,
) -> DeliveryUrlResponse:
    grant = services.delivery.issue(session, actor, project_id, asset_id)
    return DeliveryUrlResponse(
        url=grant.url, expires_in_seconds=grant.expires_in_seconds
    )


@router.get("/studio", response_model=StudioView)
def get_studio(
    project_id: UUID,
    content_version_id: Annotated[UUID, Query()],
    session: SessionDep, actor: ActorDep, services: MediaServicesDep,
) -> StudioView:
    services.studio.access.require_viewer(session, actor, project_id)
    production = session.scalar(select(MediaProduction).where(
        MediaProduction.project_id == project_id,
        MediaProduction.content_version_id == content_version_id,
    ).order_by(MediaProduction.created_at.desc()).limit(1))
    if production is None:
        raise NotFound("media production not found")
    scenes = list(session.scalars(select(Scene).where(
        Scene.production_id == production.id
    ).order_by(Scene.sequence)))
    shots = list(session.scalars(select(Shot).where(
        Shot.production_id == production.id, Shot.is_current.is_(True)
    ).order_by(Shot.sequence)))
    jobs = list(session.scalars(select(GenerationJob).where(
        GenerationJob.production_id == production.id
    ).order_by(GenerationJob.created_at)))
    assets = list(session.scalars(select(MediaAsset).where(
        MediaAsset.production_id == production.id
    ).order_by(MediaAsset.created_at)))
    reports = list(session.scalars(select(QCReport).where(
        QCReport.production_id == production.id
    ).order_by(QCReport.checked_at)))
    return StudioView(
        production=production_view(production),
        scenes=[SceneView.model_validate(row, from_attributes=True) for row in scenes],
        shots=[ShotView.model_validate(row, from_attributes=True) for row in shots],
        jobs=[JobView(id=row.id, task_id=row.task_record_id, shot_id=row.shot_id,
                      status=row.provider_status,
                      progress_percent=job_progress(row)) for row in jobs],
        assets=[AssetView.model_validate(row, from_attributes=True) for row in assets],
        qc_reports=[qc_view(row) for row in reports],
        identity_readiness=identity_views(
            session, services.studio.clock, production, shots
        ),
    )


def qc_view(row: QCReport) -> QcEvidenceView:
    return QcEvidenceView(
        id=row.id, asset_id=row.asset_id, decision=row.decision,
        human_decision=row.human_decision,
        technical_checks=row.technical_checks,
        semantic_checks=row.semantic_checks,
        rights_checks=row.rights_checks,
        product_truth_checks=row.product_truth_checks,
        continuity_checks=row.continuity_checks,
        disclosure_checks=row.disclosure_checks,
        redo_logical_shot_ids=[UUID(value) for value in row.redo_logical_shot_ids],
    )


def job_progress(row: GenerationJob) -> int:
    return {
        "held": 5, "submitted": 10, "queued": 20, "running": 45,
        "succeeded_pending_copy": 65, "copied_pending_qc": 80,
        "passed": 100, "redo": 100, "failed": 100,
    }.get(row.provider_status, 0)


def identity_views(session: Session, clock, production: MediaProduction,
                   shots: list[Shot]) -> list[IdentityReadinessView]:
    from ip_saas.common.errors import Forbidden
    from ip_saas.modules.intelligence.models import PlatformVariant
    from ip_saas.modules.media.enums import ShotSource, VoiceKind
    from ip_saas.modules.media.models.identity import (
        CharacterProfile, MediaRightsGrant, VoiceAsset,
    )
    from ip_saas.modules.media.rights import RightsPolicy

    variant = session.get(PlatformVariant, production.platform_variant_id)
    if variant is None:
        raise Conflict("production lost its platform variant")
    grants = list(session.scalars(select(MediaRightsGrant).where(
        MediaRightsGrant.project_id == production.project_id
    )))
    policy = RightsPolicy(clock)
    result: list[IdentityReadinessView] = []
    for shot in shots:
        character = session.get(CharacterProfile, shot.character_id)
        voice = session.get(VoiceAsset, shot.voice_id)
        ready, message = True, "授权与标准音色检查通过"
        try:
            policy.require_shot_rights(
                character=character, voice=voice, grants=grants,
                source=ShotSource(shot.source), platform=variant.platform,
            )
        except Forbidden as error:
            ready, message = False, error.message
        result.append(IdentityReadinessView(
            shot_id=shot.id, character_id=shot.character_id,
            voice_id=shot.voice_id,
            voice_kind=VoiceKind(voice.kind) if voice is not None else None,
            ready=ready, message=message,
        ))
    return result
```

All handler paths are backend `/v1/...`. FastAPI commits through Plan 01's synchronous `get_session`; handlers only read/write PostgreSQL and outbox state. The browser will reach them at `/api/v1/...` solely through the shared client proxy.

- [ ] **Step 7: Define strict typed models for every media event that is actually emitted**

```python
# backend/src/ip_saas/modules/media/events.py
from typing import Literal
from uuid import UUID

from pydantic import BaseModel, ConfigDict, Field, model_validator

from ip_saas.common.outbox import EventEnvelope


class StrictPayload(BaseModel):
    model_config = ConfigDict(extra="forbid")


class AttemptPayload(StrictPayload):
    job_id: UUID
    task_id: UUID
    attempt_no: int = Field(ge=1)


class MediaPollRequestedV1(EventEnvelope):
    event_type: Literal["media.job.poll.requested"] = "media.job.poll.requested"
    payload: AttemptPayload


class MediaCopyRequestedV1(EventEnvelope):
    event_type: Literal["media.copy.requested"] = "media.copy.requested"
    payload: AttemptPayload


class MediaQcRequestedPayload(AttemptPayload):
    asset_id: UUID


class MediaQcRequestedV1(EventEnvelope):
    event_type: Literal["media.qc.requested"] = "media.qc.requested"
    payload: MediaQcRequestedPayload


class MediaJobFailedPayload(AttemptPayload):
    error_code: str = Field(min_length=1, max_length=100)


class MediaJobFailedV1(EventEnvelope):
    event_type: Literal["media.job.failed"] = "media.job.failed"
    payload: MediaJobFailedPayload


class MediaHumanReviewRequestedPayload(AttemptPayload):
    project_id: UUID
    production_id: UUID
    asset_id: UUID
    qc_report_id: UUID


class MediaHumanReviewRequestedV1(EventEnvelope):
    event_type: Literal["media.human_review.requested"] = (
        "media.human_review.requested"
    )
    payload: MediaHumanReviewRequestedPayload


class MediaRedoRequestedPayload(StrictPayload):
    project_id: UUID
    production_id: UUID
    qc_report_id: UUID
    logical_shot_ids: list[UUID] = Field(min_length=1, max_length=500)


class MediaRedoRequestedV1(EventEnvelope):
    event_type: Literal["media.redo.requested"] = "media.redo.requested"
    payload: MediaRedoRequestedPayload

    @model_validator(mode="after")
    def aggregate_is_report(self) -> "MediaRedoRequestedV1":
        if self.aggregate_id != self.payload.qc_report_id:
            raise ValueError("redo aggregate must be the QC report")
        return self


MEDIA_EVENT_TYPES = (
    MediaCopyRequestedV1, MediaJobFailedV1, MediaPollRequestedV1,
    MediaQcRequestedV1, MediaHumanReviewRequestedV1, MediaRedoRequestedV1,
)
```

Replace the generic envelope construction at every emitting site with the matching class above. `media.copy.requested`, `media.job.poll.requested`, and `media.job.failed` include `job_id`, `task_id=job.task_record_id`, and the captured `attempt_no`; failure additionally includes `error_code`. Both copy and finish emit `MediaQcRequestedV1` with the same fields plus `asset_id`. QC emits `MediaHumanReviewRequestedV1` with all lineage fields. Human redo emits `MediaRedoRequestedV1`. Delete any unconsumed `media.finish.requested` or `media.job.requested` name instead of checking in an unused schema; job entry remains Plan 01's `GenerationTaskRequestedV1`, including the 64-character `request_fingerprint`.

- [ ] **Step 8: Export each event schema and prove payload drift is impossible**

```python
# backend/tests/contract/media/test_media_events.py
import json
from pathlib import Path

import pytest
from pydantic import ValidationError

from ip_saas.modules.media.events import MEDIA_EVENT_TYPES

ROOT = Path(__file__).parents[4]


@pytest.mark.parametrize("event_type", MEDIA_EVENT_TYPES)
def test_media_event_schema_matches_checked_in_contract(event_type) -> None:
    path = ROOT / "contracts/events" / f"{event_type.model_fields['event_type'].default}.v1.json"
    assert json.loads(path.read_text()) == event_type.model_json_schema()


@pytest.mark.parametrize("event_type", MEDIA_EVENT_TYPES)
def test_media_event_payload_rejects_unknown_fields(event_type) -> None:
    schema = event_type.model_json_schema()
    payload_schema = schema["$defs"][schema["properties"]["payload"]["$ref"].split("/")[-1]]
    assert payload_schema["additionalProperties"] is False


def test_attempt_event_rejects_zero_attempt() -> None:
    from uuid import UUID
    from ip_saas.modules.media.events import MediaCopyRequestedV1

    with pytest.raises(ValidationError, match="greater than or equal to 1"):
        MediaCopyRequestedV1.model_validate({
            "event_id": str(UUID(int=1)), "event_type": "media.copy.requested",
            "schema_version": 1, "aggregate_id": str(UUID(int=2)),
            "occurred_at": "2026-08-24T00:00:00Z",
            "initiated_by_actor_id": str(UUID(int=3)),
            "idempotency_key": "bad-attempt",
            "payload": {"job_id": str(UUID(int=2)), "task_id": str(UUID(int=4)),
                        "attempt_no": 0},
        })
```

Extend `backend/src/ip_saas/scripts/export_contracts.py` with this exact mapping and use the script's existing deterministic `write_or_check()` helper:

```python
from ip_saas.modules.media.events import MEDIA_EVENT_TYPES

for event_model in MEDIA_EVENT_TYPES:
    event_name = event_model.model_fields["event_type"].default
    write_or_check(
        ROOT / "contracts/events" / f"{event_name}.v1.json",
        rendered(event_model.model_json_schema()),
        args.check,
    )
```

Run: `cd backend && uv run python -m ip_saas.scripts.export_contracts && uv run pytest tests/contract/media/test_media_events.py -q`

Expected: six event JSON files are written and 13 checks pass. `generation.task.requested.v1.json` remains owned and exported by Plan 01; no media task-entry duplicate is produced.

- [ ] **Step 9: Register the router once and export the OpenAPI contract to the one frontend type file**

Modify the ordered router block in `backend/src/ip_saas/api.py`:

```python
from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse

from ip_saas.common.errors import Conflict, DomainError, Forbidden, NotFound
from ip_saas.modules.accounts.router import router as accounts_router
from ip_saas.modules.billing.router import router as billing_router
from ip_saas.modules.media.dependencies import MediaApiServices
from ip_saas.modules.media.router import router as media_router
from ip_saas.modules.model_registry.router import router as model_registry_router
from ip_saas.modules.projects.router import router as projects_router
from ip_saas.modules.tasks.router import router as tasks_router


STATUS_BY_ERROR = {Conflict: 409, Forbidden: 403, NotFound: 404}


def create_app(media_services: MediaApiServices | None = None) -> FastAPI:
    app = FastAPI(title="AI IP SaaS API", version="0.1.0")
    if media_services is not None:
        app.state.media_services = media_services

    @app.exception_handler(DomainError)
    def handle_domain_error(_request: Request, error: DomainError) -> JSONResponse:
        response_status = next(
            (value for kind, value in STATUS_BY_ERROR.items()
             if isinstance(error, kind)),
            400,
        )
        return JSONResponse(
            status_code=response_status,
            content={"code": error.code, "message": error.message},
        )

    @app.get("/healthz", tags=["system"])
    def health() -> dict[str, str]:
        return {"status": "ok"}

    app.include_router(accounts_router)
    app.include_router(projects_router)
    app.include_router(billing_router)
    app.include_router(model_registry_router)
    app.include_router(tasks_router)
    app.include_router(media_router)
    return app
```

The deployed ASGI factory passes a fully constructed `MediaApiServices` to `create_app`; contract export may call `create_app()` because OpenAPI generation never resolves request dependencies. There is one media-router inclusion and no `/api` backend prefix.

Add this assertion to `backend/tests/contract/test_openapi.py`:

```python
def test_media_routes_are_backend_v1_only() -> None:
    paths = create_app().openapi()["paths"]
    required = {
        "/v1/projects/{project_id}/media/productions",
        "/v1/projects/{project_id}/media/productions/{production_id}/storyboard",
        "/v1/projects/{project_id}/media/productions/{production_id}/jobs",
        "/v1/projects/{project_id}/media/qc-reports/{report_id}/review",
        "/v1/projects/{project_id}/media/assets/{asset_id}/delivery-url",
        "/v1/projects/{project_id}/media/studio",
    }
    assert required <= set(paths)
    assert not any(path.startswith("/api/") for path in paths)
```

Run: `make export-contracts && cd frontend && pnpm exec openapi-typescript ../contracts/openapi.json -o src/lib/api/schema.d.ts`

Expected: `contracts/openapi.json` and the existing `frontend/src/lib/api/schema.d.ts` are updated. No `generated.ts`, feature-local OpenAPI file, or second API client is created.

- [ ] **Step 10: Implement the Studio feature through the shared API client**

```typescript
// frontend/src/features/media/types.ts
import type {components} from "@/lib/api/schema";

export type StudioView = components["schemas"]["StudioView"];
export type MediaJobAccepted = components["schemas"]["MediaJobAccepted"];
export type TaskView = components["schemas"]["TaskResponse"];
export type QcDecision = components["schemas"]["HumanReviewDecision"];
export type DeliveryUrl = components["schemas"]["DeliveryUrlResponse"];
```

```typescript
// frontend/src/features/media/api.ts
import {apiRequest} from "@/lib/api/client";
import type {DeliveryUrl, MediaJobAccepted, QcDecision, StudioView, TaskView} from "./types";

const root = (projectId: string) =>
  `/v1/projects/${encodeURIComponent(projectId)}/media`;

export function loadStudio(projectId: string, contentVersionId: string) {
  return apiRequest<StudioView>(
    `${root(projectId)}/studio?content_version_id=${encodeURIComponent(contentVersionId)}`,
  );
}

export function approveStoryboard(projectId: string, productionId: string) {
  return apiRequest(`${root(projectId)}/productions/${productionId}/storyboard/approve`, {
    method: "POST",
  });
}

export function approvePreviews(
  projectId: string,
  productionId: string,
  selectedAssetByShot: Record<string, string>,
) {
  return apiRequest(`${root(projectId)}/productions/${productionId}/previews/approve`, {
    method: "POST",
    body: {selected_asset_by_shot: selectedAssetByShot},
  });
}

export function createJob(
  projectId: string,
  productionId: string,
  body: {
    shot_id: string | null;
    capability: "low_res_video" | "high_res_video" | "standard_tts" | "finish";
    cost_approval: {credit_units: number; quote_version: string};
    idempotency_key: string;
  },
) {
  return apiRequest<MediaJobAccepted>(
    `${root(projectId)}/productions/${productionId}/jobs`,
    {method: "POST", body},
  );
}

export function loadTask(taskId: string) {
  return apiRequest<TaskView>(`/v1/tasks/${encodeURIComponent(taskId)}`);
}

export function reviewQc(
  projectId: string, reportId: string, decision: QcDecision, note: string,
) {
  return apiRequest(`${root(projectId)}/qc-reports/${reportId}/review`, {
    method: "POST", body: {decision, note},
  });
}

export function redoShot(
  projectId: string, shotId: string, reason: string, creditUnits: number,
  idempotencyKey: string,
) {
  return apiRequest(`${root(projectId)}/shots/${shotId}/redo`, {
    method: "POST", body: {
      reason, capability: "high_res_video",
      cost_approval: {credit_units: creditUnits, quote_version: "current"},
      idempotency_key: idempotencyKey,
    },
  });
}

export function requestDelivery(projectId: string, assetId: string) {
  return apiRequest<DeliveryUrl>(`${root(projectId)}/assets/${assetId}/delivery-url`);
}
```

Every application request above passes a backend `/v1/...` path to Plan 01's `apiRequest`; that client alone adds the browser `/api` prefix. The feature never passes `/api/v1`, never imports native `fetch`, and never creates another token store or error decoder.

```tsx
// frontend/src/features/media/MediaStudio.tsx
"use client";

import {useCallback, useEffect, useMemo, useState} from "react";
import {
  approvePreviews, approveStoryboard, createJob, loadStudio, loadTask,
  redoShot, requestDelivery, reviewQc,
} from "./api";
import type {QcDecision, StudioView} from "./types";

type Props = {projectId: string; contentVersionId: string};

const LABELS: Record<string, string> = {
  human: "真人生产包", ai: "AI 生产", mixed: "真人 + AI 混合",
};

export function MediaStudio({projectId, contentVersionId}: Props) {
  const [studio, setStudio] = useState<StudioView | null>(null);
  const [selected, setSelected] = useState<Record<string, string>>({});
  const [credits, setCredits] = useState(40);
  const [deliveryUrl, setDeliveryUrl] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setStudio(await loadStudio(projectId, contentVersionId));
  }, [projectId, contentVersionId]);

  useEffect(() => { void refresh().catch((value: Error) => setError(value.message)); }, [refresh]);

  useEffect(() => {
    if (!studio?.jobs.some((job) => !["passed", "redo", "failed"].includes(job.status))) return;
    const timer = window.setInterval(async () => {
      await Promise.all(studio.jobs.map((job) => loadTask(job.task_id)));
      await refresh();
    }, 2500);
    return () => window.clearInterval(timer);
  }, [studio?.jobs, refresh]);

  const previewsByShot = useMemo(() => {
    const grouped: Record<string, StudioView["assets"]> = {};
    for (const asset of studio?.assets ?? []) {
      if (asset.kind !== "low_res_preview" || !asset.shot_id) continue;
      (grouped[asset.shot_id] ??= []).push(asset);
    }
    return grouped;
  }, [studio?.assets]);

  async function act(key: string, action: () => Promise<unknown>) {
    setBusy(key); setError(null);
    try { await action(); await refresh(); }
    catch (value) { setError(value instanceof Error ? value.message : "操作失败"); }
    finally { setBusy(null); }
  }

  if (error && !studio) return <main role="alert">{error}</main>;
  if (!studio) return <main aria-busy="true">正在加载媒体工作室…</main>;
  const {production} = studio;
  const finalAsset = studio.assets.find((asset) => asset.id === production.final_asset_id);

  return (
    <main aria-label="媒体工作室">
      <header>
        <h1>一站式媒体工作室</h1>
        <p>生产路线：{LABELS[production.mode]}</p>
        <p>当前阶段：{production.status}</p>
        <p>AI 标识：成片可见标识与文件元数据均为强制项，不提供移除入口。</p>
      </header>
      {error && <p role="alert">{error}</p>}

      <section aria-labelledby="identity-title">
        <h2 id="identity-title">角色、声音与授权</h2>
        {studio.identity_readiness.map((item) => (
          <article key={item.shot_id}>
            <strong>镜头 {item.shot_id.slice(0, 8)}</strong>
            <span>{item.voice_kind === "standard_tts" ? "标准音色" : "已上传录音/无配音"}</span>
            <span aria-label={item.ready ? "授权通过" : "授权阻断"}>{item.message}</span>
          </article>
        ))}
      </section>

      <section aria-labelledby="storyboard-title">
        <h2 id="storyboard-title">故事板与镜头</h2>
        {production.status === "storyboard_ready" && (
          <button disabled={busy !== null} onClick={() => void act(
            "approve-storyboard", () => approveStoryboard(projectId, production.id),
          )}>确认故事板</button>
        )}
        {studio.shots.map((shot) => (
          <article key={shot.id}>
            <h3>{shot.sequence}. {shot.title}</h3>
            <p>来源：{shot.source} · 修订：{shot.revision}</p>
            {(previewsByShot[shot.id] ?? []).length > 0 && (
              <label>低清方案
                <select value={selected[shot.id] ?? ""} onChange={(event) => setSelected({
                  ...selected, [shot.id]: event.target.value,
                })}>
                  <option value="">请选择</option>
                  {previewsByShot[shot.id].map((asset) => (
                    <option key={asset.id} value={asset.id}>{asset.id.slice(0, 8)}</option>
                  ))}
                </select>
              </label>
            )}
            <button disabled={busy !== null} onClick={() => void act(
              `redo-${shot.id}`,
              () => redoShot(
                projectId, shot.id, "用户要求单镜重做", credits,
                `user-redo:${shot.logical_shot_id}:r${shot.revision + 1}`,
              ),
            )}>只重做这一镜</button>
          </article>
        ))}
        {production.status === "low_res_ready" && (
          <button disabled={busy !== null || Object.keys(selected).length === 0}
            onClick={() => void act("approve-previews", () => approvePreviews(
              projectId, production.id, selected,
            ))}>确认所选低清方案</button>
        )}
      </section>

      {production.status === "low_res_approved" && (
        <section aria-labelledby="cost-title">
          <h2 id="cost-title">高清成本确认</h2>
          <label>本次最高积分
            <input type="number" min={1} value={credits}
              onChange={(event) => setCredits(Number(event.target.value))} />
          </label>
          <button disabled={busy !== null || credits < 1} onClick={() => void act(
            "high-res", () => createJob(projectId, production.id, {
              shot_id: null, capability: "high_res_video",
              cost_approval: {credit_units: credits, quote_version: "current"},
              idempotency_key: `high-res:${production.id}:v1`,
            }),
          )}>确认成本并开始高清生成</button>
        </section>
      )}

      <section aria-labelledby="jobs-title">
        <h2 id="jobs-title">任务进度</h2>
        {studio.jobs.map((job) => (
          <div key={job.id}>
            <span>{job.status}</span>
            <progress max={100} value={job.progress_percent}>{job.progress_percent}%</progress>
          </div>
        ))}
      </section>

      <section aria-labelledby="qc-title">
        <h2 id="qc-title">媒体质检与人工确认</h2>
        {studio.qc_reports.map((report) => (
          <article key={report.id}>
            <h3>机器结论：{report.decision}</h3>
            {([
              ["技术完整性", report.technical_checks],
              ["语义与表演", report.semantic_checks],
              ["授权与同意", report.rights_checks],
              ["商品真实性", report.product_truth_checks],
              ["IP 连续性", report.continuity_checks],
              ["显式/隐式 AI 标识", report.disclosure_checks],
            ] as const).map(([label, checks]) => (
              <details key={label}><summary>{label}</summary>
                <ul>{checks.map((check, index) => (
                  <li key={`${String(check.code)}-${index}`}>{check.passed ? "通过" : "阻断"}：{String(check.message)}</li>
                ))}</ul>
              </details>
            ))}
            {!report.human_decision && (["pass", "redo", "fail"] as QcDecision[]).map((decision) => (
              <button key={decision} disabled={busy !== null} onClick={() => void act(
                `${decision}-${report.id}`,
                () => reviewQc(projectId, report.id, decision, `人工复核：${decision}`),
              )}>{decision === "pass" ? "人工通过" : decision === "redo" ? "退回单镜重做" : "人工驳回"}</button>
            ))}
          </article>
        ))}
      </section>

      {finalAsset && studio.qc_reports.some((report) =>
        report.asset_id === finalAsset.id && report.human_decision === "pass") && (
        <section aria-labelledby="delivery-title">
          <h2 id="delivery-title">私有交付</h2>
          <button disabled={busy !== null} onClick={() => void act("delivery", async () => {
            const grant = await requestDelivery(projectId, finalAsset.id);
            setDeliveryUrl(grant.url);
          })}>生成 5 分钟下载链接</button>
          {deliveryUrl && <a href={deliveryUrl} rel="noreferrer">下载审核通过的成片</a>}
        </section>
      )}
    </main>
  );
}
```

```tsx
// frontend/src/app/(c-user)/projects/[projectId]/content/[contentVersionId]/studio/page.tsx
import {MediaStudio} from "@/features/media/MediaStudio";

export default async function StudioPage({params}: {
  params: Promise<{projectId: string; contentVersionId: string}>;
}) {
  const {projectId, contentVersionId} = await params;
  return <MediaStudio projectId={projectId} contentVersionId={contentVersionId} />;
}
```

- [ ] **Step 11: Add Vitest and Playwright coverage for the complete C-user path**

```tsx
// frontend/src/features/media/media-studio.test.tsx
import {render, screen, waitFor} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {beforeEach, describe, expect, it, vi} from "vitest";
import {MediaStudio} from "./MediaStudio";
import * as api from "./api";
import type {StudioView} from "./types";

vi.mock("./api");

const ids = {
  project: "10000000-0000-4000-8000-000000000001",
  content: "10000000-0000-4000-8000-000000000002",
  production: "10000000-0000-4000-8000-000000000003",
  scene: "10000000-0000-4000-8000-000000000004",
  shot: "10000000-0000-4000-8000-000000000005",
  logical: "10000000-0000-4000-8000-000000000006",
  task: "10000000-0000-4000-8000-000000000007",
  job: "10000000-0000-4000-8000-000000000008",
  asset: "10000000-0000-4000-8000-000000000009",
  report: "10000000-0000-4000-8000-000000000010",
};

function studio(): StudioView {
  const passed = [{code: "decode_ok", passed: true, message: "通过"}];
  return {
    production: {
      id: ids.production, project_id: ids.project,
      content_version_id: ids.content,
      platform_variant_id: "10000000-0000-4000-8000-000000000011",
      mode: "mixed", output_aspect_ratio: "9:16", status: "qc_pending",
      final_asset_id: ids.asset,
    },
    scenes: [{id: ids.scene, sequence: 1, title: "送礼", purpose: "人物选择", location: "客厅"}],
    shots: [{
      id: ids.shot, scene_id: ids.scene, logical_shot_id: ids.logical,
      revision: 1, sequence: 1, title: "递出礼盒", source: "seedance",
      selected_asset_id: ids.asset,
    }],
    jobs: [{id: ids.job, task_id: ids.task, shot_id: ids.shot,
            status: "passed", progress_percent: 100}],
    assets: [{id: ids.asset, job_id: ids.job, shot_id: ids.shot,
              kind: "final_video", ai_disclosure: "generated", content_type: "video/mp4"}],
    qc_reports: [{
      id: ids.report, asset_id: ids.asset, decision: "pass", human_decision: null,
      technical_checks: passed, semantic_checks: passed, rights_checks: passed,
      product_truth_checks: passed, continuity_checks: passed,
      disclosure_checks: passed, redo_logical_shot_ids: [],
    }],
    identity_readiness: [{
      shot_id: ids.shot, character_id: null,
      voice_id: "10000000-0000-4000-8000-000000000012",
      voice_kind: "standard_tts", ready: true,
      message: "授权与标准音色检查通过",
    }],
  };
}

describe("MediaStudio", () => {
  beforeEach(() => {
    vi.mocked(api.loadStudio).mockResolvedValue(studio());
    vi.mocked(api.reviewQc).mockResolvedValue({});
  });

  it("shows mixed routing, identity readiness, every QC family, and mandatory labels", async () => {
    render(<MediaStudio projectId={ids.project} contentVersionId={ids.content} />);
    expect(await screen.findByText("真人 + AI 混合")).toBeInTheDocument();
    expect(screen.getByText("授权与标准音色检查通过")).toBeInTheDocument();
    expect(screen.getByText(/不提供移除入口/)).toBeInTheDocument();
    for (const family of ["技术完整性", "语义与表演", "授权与同意", "商品真实性", "IP 连续性", "显式/隐式 AI 标识"]) {
      expect(screen.getByText(family)).toBeInTheDocument();
    }
  });

  it("persists an explicit human decision before delivery appears", async () => {
    const user = userEvent.setup();
    render(<MediaStudio projectId={ids.project} contentVersionId={ids.content} />);
    await user.click(await screen.findByRole("button", {name: "人工通过"}));
    await waitFor(() => expect(api.reviewQc).toHaveBeenCalledWith(
      ids.project, ids.report, "pass", "人工复核：pass",
    ));
  });
});
```

```typescript
// frontend/tests/e2e/media-studio.spec.ts
import {expect, test} from "@playwright/test";

test("preview, task, QC, redo, and private delivery stay on the shared API", async ({page}) => {
  let humanPassed = false;
  const ids = {
    project: "10000000-0000-4000-8000-000000000001",
    content: "10000000-0000-4000-8000-000000000002",
    production: "10000000-0000-4000-8000-000000000003",
    scene: "10000000-0000-4000-8000-000000000004",
    shot: "10000000-0000-4000-8000-000000000005",
    logical: "10000000-0000-4000-8000-000000000006",
    task: "10000000-0000-4000-8000-000000000007",
    job: "10000000-0000-4000-8000-000000000008",
    asset: "10000000-0000-4000-8000-000000000009",
    report: "10000000-0000-4000-8000-000000000010",
  };
  await page.addInitScript(() => sessionStorage.setItem("access_token", "e2e-token"));
  await page.route("**/api/v1/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith(`/qc-reports/${ids.report}/review`)) {
      humanPassed = true;
      return route.fulfill({status: 200, json: {}});
    }
    if (path.endsWith(`/assets/${ids.asset}/delivery-url`)) {
      return route.fulfill({status: 200, json: {
        url: "https://tos.example/signed-final", expires_in_seconds: 300,
      }});
    }
    if (path.endsWith("/studio")) {
      const passed = [{code: "decode_ok", passed: true, message: "通过"}];
      return route.fulfill({status: 200, json: {
        production: {id: ids.production, project_id: ids.project,
          content_version_id: ids.content,
          platform_variant_id: "10000000-0000-4000-8000-000000000011",
          mode: "mixed", output_aspect_ratio: "9:16", status: "qc_pending",
          final_asset_id: ids.asset},
        scenes: [{id: ids.scene, sequence: 1, title: "送礼", purpose: "人物选择", location: "客厅"}],
        shots: [{id: ids.shot, scene_id: ids.scene, logical_shot_id: ids.logical,
          revision: 1, sequence: 1, title: "递出礼盒", source: "seedance",
          selected_asset_id: ids.asset}],
        jobs: [{id: ids.job, task_id: ids.task, shot_id: ids.shot,
          status: "passed", progress_percent: 100}],
        assets: [{id: ids.asset, job_id: ids.job, shot_id: ids.shot,
          kind: "final_video", ai_disclosure: "generated", content_type: "video/mp4"}],
        qc_reports: [{id: ids.report, asset_id: ids.asset, decision: "pass",
          human_decision: humanPassed ? "pass" : null,
          technical_checks: passed, semantic_checks: passed, rights_checks: passed,
          product_truth_checks: passed, continuity_checks: passed,
          disclosure_checks: passed, redo_logical_shot_ids: []}],
        identity_readiness: [{shot_id: ids.shot, character_id: null,
          voice_id: null, voice_kind: "standard_tts", ready: true,
          message: "授权与标准音色检查通过"}],
      }});
    }
    return route.fulfill({status: 404, json: {message: "unhandled test API"}});
  });

  await page.goto(`/projects/${ids.project}/content/${ids.content}/studio`);
  await expect(page.getByText("真人 + AI 混合")).toBeVisible();
  await expect(page.getByText(/不提供移除入口/)).toBeVisible();
  await page.getByRole("button", {name: "人工通过"}).click();
  await expect(page.getByRole("button", {name: "生成 5 分钟下载链接"})).toBeVisible();
  await page.getByRole("button", {name: "生成 5 分钟下载链接"}).click();
  await expect(page.getByRole("link", {name: "下载审核通过的成片"}))
    .toHaveAttribute("href", "https://tos.example/signed-final");
  expect(await page.locator("text=移除 AI 标识").count()).toBe(0);
});
```

- [ ] **Step 12: Run backend, contract, frontend, and browser acceptance tests**

Run: `cd backend && env -u ARK_API_KEY -u DOUBAO_TTS_APP_ID -u DOUBAO_TTS_ACCESS_KEY -u TOS_ACCESS_KEY -u TOS_SECRET_KEY uv run pytest tests/unit/media/test_delivery_policy.py tests/integration/media/test_media_router.py tests/contract/media/test_media_events.py tests/security/test_media_tenant_isolation.py -q`

Expected: all tests pass without network or paid credentials; request latency tests stay under three seconds and no provider method is reachable from a handler.

Run: `cd frontend && pnpm test -- --run src/features/media/media-studio.test.tsx && pnpm exec playwright test tests/e2e/media-studio.spec.ts`

Expected: Vitest and Playwright pass; every application request observed by Playwright begins `/api/v1/`, while feature source contains only backend `/v1/` arguments to `apiRequest`.

Run: `make export-contracts && git diff --exit-code contracts/openapi.json contracts/events frontend/src/lib/api/schema.d.ts`

Expected: no OpenAPI, event-schema, or sole frontend-type-file drift remains.

- [ ] **Step 13: Commit the complete API, event, delivery, and Studio surface**

```bash
git add backend/src/ip_saas/modules/media backend/src/ip_saas/api.py backend/src/ip_saas/scripts/export_contracts.py backend/tests/unit/media/test_delivery_policy.py backend/tests/integration/media/test_media_router.py backend/tests/contract/media/test_media_events.py backend/tests/security/test_media_tenant_isolation.py contracts/events contracts/openapi.json frontend/src/lib/api/schema.d.ts frontend/src/features/media frontend/src/app/'(c-user)'/projects/'[projectId]'/content/'[contentVersionId]'/studio/page.tsx frontend/tests/e2e/media-studio.spec.ts
git commit -m "feat: deliver the media studio end to end"
```

### Task 17: Prove paid-provider crash reconciliation before enabling any real media capability

**Files:**
- Modify: `Makefile`
- Modify: `backend/migrations/versions/0003_media.py`
- Modify: `backend/src/ip_saas/config.py`
- Modify: `backend/src/ip_saas/modules/media/models/__init__.py`
- Create: `backend/src/ip_saas/modules/media/models/provider_ops.py`
- Create: `backend/src/ip_saas/modules/media/provider_poc.py`
- Create: `backend/src/ip_saas/modules/media/provider_reconciliation.py`
- Create: `backend/src/ip_saas/modules/media/provider_release.py`
- Create: `backend/src/ip_saas/providers/media/composition.py`
- Modify: `backend/src/ip_saas/providers/volcengine/seedream.py`
- Modify: `backend/src/ip_saas/providers/volcengine/seedance.py`
- Modify: `backend/src/ip_saas/providers/volcengine/tts.py`
- Modify: `backend/src/ip_saas/workers/media_submit.py`
- Modify: `backend/src/ip_saas/workers/media_poll.py`
- Create: `backend/scripts/run_media_provider_poc.py`
- Modify: `backend/tests/integration/media/harness.py`
- Test: `backend/tests/unit/media/test_provider_release.py`
- Test: `backend/tests/unit/media/test_provider_attempt_models.py`
- Test: `backend/tests/integration/media/test_provider_attempt_reconciliation.py`
- Test: `backend/tests/integration/media/test_provider_release_composition.py`
- Test: `backend/tests/contract/media/test_provider_poc_cli.py`
- Create: `docs/runbooks/media-provider-crash-poc.md`

This task does not create a second task, hold, ledger, or migration head. `TaskRecord` remains the task and billing owner, `ProviderCostEntry` remains the supplier-cost ledger, and the two provider-operations tables below are evidence and lineage only. Add both tables to the existing `0003_media` migration and keep `down_revision = "0002_intelligence"`.

- [ ] **Step 1: Write failing strict-evidence and provider-attempt model tests**

Create `backend/tests/unit/media/test_provider_release.py`:

```python
from datetime import UTC, datetime
from hashlib import sha256
import json
from pathlib import Path
from uuid import UUID

import pytest
from pydantic import ValidationError

from ip_saas.common.errors import Forbidden
from ip_saas.modules.media.provider_release import (
    CrashPoint,
    MediaAdapterKind,
    MediaProviderReleaseEvidence,
    ProviderReleaseGate,
    RecoveryProof,
)


TASK_BEFORE = UUID("10000000-0000-4000-8000-000000000001")
TASK_AFTER = UUID("10000000-0000-4000-8000-000000000002")
ATTEMPT_BEFORE = UUID("20000000-0000-4000-8000-000000000001")
ATTEMPT_AFTER = UUID("20000000-0000-4000-8000-000000000002")
COST_AFTER = UUID("30000000-0000-4000-8000-000000000001")


def evidence_payload() -> dict[str, object]:
    return {
        "schema_version": 1,
        "provider": "volcengine",
        "adapter": "seedream",
        "capabilities": ["storyboard_image"],
        "model_id": "ep-seedream-pinned",
        "model_version": "2026-08-24",
        "region": "cn-beijing",
        "production_account_fingerprint": "a" * 64,
        "run_id": "40000000-0000-4000-8000-000000000001",
        "official_identity_contract_url": (
            "https://www.volcengine.com/docs/82379/1824121"
        ),
        "official_identity_contract_sha256": "b" * 64,
        "recovery_proof": "supplier_statement_reconciliation",
        "official_request_identity_observation_sha256": None,
        "statement_reconciliation_implementation_sha256": "c" * 64,
        "provider_request_log_sha256": "d" * 64,
        "supplier_statement_sha256": "e" * 64,
        "supplier_statement_final": True,
        "attempt_journal_sha256": "f" * 64,
        "crash_transcript_sha256": "1" * 64,
        "reconciliation_report_sha256": "2" * 64,
        "budget_cap_receipt_sha256": "3" * 64,
        "max_total_fen": 100,
        "attempts": [
            {
                "fault_point": "before_http",
                "task_id": str(TASK_BEFORE),
                "client_attempt_id": str(ATTEMPT_BEFORE),
                "http_send_observed": False,
                "paid_response_observed": False,
                "job_result_persisted_before_kill": False,
                "cost_persisted_before_kill": False,
                "provider_request_ids": [],
                "request_log_request_ids": [],
                "statement_request_ids": [],
                "provider_cost_entry_ids": [],
                "provider_cost_task_ids": [],
                "supplier_amount_fen": 0,
                "ledger_amount_fen": 0,
            },
            {
                "fault_point": "after_paid_response_before_persistence",
                "task_id": str(TASK_AFTER),
                "client_attempt_id": str(ATTEMPT_AFTER),
                "http_send_observed": True,
                "paid_response_observed": True,
                "job_result_persisted_before_kill": False,
                "cost_persisted_before_kill": False,
                "provider_request_ids": ["supplier-request-1"],
                "request_log_request_ids": ["supplier-request-1"],
                "statement_request_ids": ["supplier-request-1"],
                "provider_cost_entry_ids": [str(COST_AFTER)],
                "provider_cost_task_ids": [str(TASK_AFTER)],
                "supplier_amount_fen": 4,
                "ledger_amount_fen": 4,
            },
        ],
        "reviewed_by_actor_id": "50000000-0000-4000-8000-000000000001",
        "review_ticket": "MEDIA-RELEASE-2026-001",
        "reviewed_at": "2026-08-25T00:00:00Z",
        "passed": True,
    }


def test_database_only_or_wrong_window_cannot_be_release_evidence() -> None:
    payload = evidence_payload()
    payload["attempts"][1]["statement_request_ids"] = []  # type: ignore[index]
    with pytest.raises(ValidationError, match="supplier-side request sets"):
        MediaProviderReleaseEvidence.model_validate(payload)

    payload = evidence_payload()
    payload["attempts"][1]["cost_persisted_before_kill"] = True  # type: ignore[index]
    with pytest.raises(ValidationError, match="exact crash checkpoint"):
        MediaProviderReleaseEvidence.model_validate(payload)


def test_statement_recovery_requires_the_deployed_reconciler_hash() -> None:
    payload = evidence_payload()
    payload["statement_reconciliation_implementation_sha256"] = None
    with pytest.raises(ValidationError, match="reconciler hash"):
        MediaProviderReleaseEvidence.model_validate(payload)


def test_hash_pinned_gate_matches_adapter_model_version_and_region(
    tmp_path: Path,
) -> None:
    path = tmp_path / "seedream-release-evidence.json"
    path.write_text(
        json.dumps(evidence_payload(), sort_keys=True, separators=(",", ":")),
        encoding="utf-8",
    )
    digest = sha256(path.read_bytes()).hexdigest()
    gate = ProviderReleaseGate.load(path, digest)
    gate.require(
        adapter=MediaAdapterKind.SEEDREAM,
        capability="storyboard_image",
        model_id="ep-seedream-pinned",
        model_version="2026-08-24",
        region="cn-beijing",
        reconciler_sha256="c" * 64,
    )
    with pytest.raises(Forbidden, match="does not match active binding"):
        gate.require(
            adapter=MediaAdapterKind.SEEDREAM,
            capability="storyboard_image",
            model_id="ep-different",
            model_version="2026-08-24",
            region="cn-beijing",
            reconciler_sha256="c" * 64,
        )
    with pytest.raises(Forbidden, match="digest mismatch"):
        ProviderReleaseGate.load(path, "9" * 64)


def test_missing_evidence_is_disabled() -> None:
    with pytest.raises(Forbidden, match="release evidence is not configured"):
        ProviderReleaseGate.disabled().require(
            adapter=MediaAdapterKind.SEEDANCE,
            capability="low_res_video",
            model_id="ep-seedance-pinned",
            model_version="2026-08-24",
            region="cn-beijing",
            reconciler_sha256="c" * 64,
        )
```

Create `backend/tests/unit/media/test_provider_attempt_models.py`:

```python
from ip_saas.modules.media.models.provider_ops import (
    ProviderRequestAttempt,
    ProviderSupplierReconciliation,
)


def test_attempt_has_stable_request_and_shared_task_lineage_only() -> None:
    columns = set(ProviderRequestAttempt.__table__.columns.keys())
    assert {
        "task_id", "generation_job_id", "client_attempt_id",
        "request_fingerprint", "request_payload_sha256", "worker_attempt_no",
        "expects_supplier_charge",
        "send_started_at", "supplier_request_log_run_id",
        "supplier_request_log_sha256",
    } <= columns
    assert "generation_hold_id" not in columns
    assert "internal_budget_hold_id" not in columns
    unique_names = {
        item.name for item in ProviderRequestAttempt.__table__.constraints
        if item.name is not None
    }
    assert "uq_media_provider_attempt_client_id" in unique_names
    assert "uq_media_provider_attempt_logical_call" in unique_names


def test_reconciliation_links_supplier_evidence_to_existing_cost_ledger() -> None:
    columns = set(ProviderSupplierReconciliation.__table__.columns.keys())
    assert {
        "provider_request_attempt_id", "task_id", "provider_request_id",
        "provider_request_log_sha256", "supplier_statement_sha256",
        "provider_cost_entry_id", "supplier_amount_minor", "amount_fen",
        "idempotency_key", "input_fingerprint",
    } <= columns
    assert "customer_charge" not in columns
```

- [ ] **Step 2: Run the tests and verify the new evidence boundary is absent**

Run:

```bash
cd backend
env -u ARK_API_KEY -u DOUBAO_TTS_APP_ID -u DOUBAO_TTS_ACCESS_KEY \
  uv run pytest \
  tests/unit/media/test_provider_release.py \
  tests/unit/media/test_provider_attempt_models.py -q
```

Expected: collection fails because `provider_release.py` and `models/provider_ops.py` do not exist. No credential is read and no network call occurs.

- [ ] **Step 3: Implement strict release evidence and a hash-pinned per-binding gate**

Create `backend/src/ip_saas/modules/media/provider_release.py`:

```python
from __future__ import annotations

from datetime import datetime
from enum import StrEnum
from hashlib import sha256
from pathlib import Path
from typing import Annotated, Literal
from uuid import UUID

from pydantic import (
    AfterValidator,
    BaseModel,
    ConfigDict,
    Field,
    HttpUrl,
    StringConstraints,
    model_validator,
)

from ip_saas.common.errors import Forbidden


Sha256Hex = Annotated[str, Field(pattern=r"^[0-9a-f]{64}$")]
NonBlank = Annotated[str, Field(min_length=1, max_length=500)]


def reject_provider_request_sentinel(value: str) -> str:
    if value.casefold() in {"none", "missing"}:
        raise ValueError("provider request id sentinel is invalid")
    return value


ProviderRequestId = Annotated[
    str,
    StringConstraints(strip_whitespace=True, min_length=1, max_length=200),
    AfterValidator(reject_provider_request_sentinel),
]


class StrictEvidenceModel(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)


class MediaAdapterKind(StrEnum):
    SEEDREAM = "seedream"
    SEEDANCE = "seedance"
    STANDARD_TTS = "standard_tts"


class CrashPoint(StrEnum):
    BEFORE_HTTP = "before_http"
    AFTER_PAID_RESPONSE_BEFORE_PERSISTENCE = (
        "after_paid_response_before_persistence"
    )


class RecoveryProof(StrEnum):
    OFFICIAL_QUERY = "official_query"
    OFFICIAL_IDEMPOTENCY = "official_idempotency"
    SUPPLIER_STATEMENT_RECONCILIATION = "supplier_statement_reconciliation"


class CrashAttemptEvidence(StrictEvidenceModel):
    fault_point: CrashPoint
    task_id: UUID
    client_attempt_id: UUID
    http_send_observed: bool
    paid_response_observed: bool
    job_result_persisted_before_kill: bool
    cost_persisted_before_kill: bool
    provider_request_ids: tuple[ProviderRequestId, ...]
    request_log_request_ids: tuple[ProviderRequestId, ...]
    statement_request_ids: tuple[ProviderRequestId, ...]
    provider_cost_entry_ids: tuple[UUID, ...]
    provider_cost_task_ids: tuple[UUID, ...]
    supplier_amount_fen: Annotated[int, Field(ge=0)]
    ledger_amount_fen: Annotated[int, Field(ge=0)]

    @model_validator(mode="after")
    def exact_checkpoint_and_supplier_mapping(self) -> "CrashAttemptEvidence":
        if self.fault_point is CrashPoint.BEFORE_HTTP:
            checkpoint = (
                self.http_send_observed,
                self.paid_response_observed,
                self.job_result_persisted_before_kill,
                self.cost_persisted_before_kill,
            )
            if checkpoint != (False, False, False, False):
                raise ValueError("before-http control has an invalid checkpoint")
            collections = (
                self.provider_request_ids,
                self.request_log_request_ids,
                self.statement_request_ids,
                self.provider_cost_entry_ids,
                self.provider_cost_task_ids,
            )
            if any(collections) or self.supplier_amount_fen or self.ledger_amount_fen:
                raise ValueError("before-http control cannot contain supplier cost")
            return self

        checkpoint = (
            self.http_send_observed,
            self.paid_response_observed,
            self.job_result_persisted_before_kill,
            self.cost_persisted_before_kill,
        )
        if checkpoint != (True, True, False, False):
            raise ValueError("exact crash checkpoint was not demonstrated")
        supplier_sets = (
            self.provider_request_ids,
            self.request_log_request_ids,
            self.statement_request_ids,
        )
        if any(len(values) != 1 for values in supplier_sets):
            raise ValueError("supplier-side request sets must contain one request")
        if any(values != self.provider_request_ids for values in supplier_sets):
            raise ValueError("supplier-side request sets identify different requests")
        if len(self.provider_cost_entry_ids) != 1:
            raise ValueError("one billed request needs one provider cost entry")
        if self.provider_cost_task_ids != (self.task_id,):
            raise ValueError("provider cost entry is not linked to the exercised task")
        if self.supplier_amount_fen <= 0:
            raise ValueError("paid crash cell needs positive supplier evidence")
        if self.ledger_amount_fen != self.supplier_amount_fen:
            raise ValueError("supplier and provider-cost fen differ")
        return self


class MediaProviderReleaseEvidence(StrictEvidenceModel):
    schema_version: Literal[1] = 1
    provider: Literal["volcengine"] = "volcengine"
    adapter: MediaAdapterKind
    capabilities: Annotated[tuple[NonBlank, ...], Field(min_length=1)]
    model_id: NonBlank
    model_version: NonBlank
    region: NonBlank
    production_account_fingerprint: Sha256Hex
    run_id: UUID
    official_identity_contract_url: HttpUrl
    official_identity_contract_sha256: Sha256Hex
    recovery_proof: RecoveryProof
    official_request_identity_observation_sha256: Sha256Hex | None = None
    statement_reconciliation_implementation_sha256: Sha256Hex | None = None
    provider_request_log_sha256: Sha256Hex
    supplier_statement_sha256: Sha256Hex
    supplier_statement_final: Literal[True]
    attempt_journal_sha256: Sha256Hex
    crash_transcript_sha256: Sha256Hex
    reconciliation_report_sha256: Sha256Hex
    budget_cap_receipt_sha256: Sha256Hex
    max_total_fen: Annotated[int, Field(gt=0, le=100)]
    attempts: Annotated[tuple[CrashAttemptEvidence, ...], Field(min_length=2)]
    reviewed_by_actor_id: UUID
    review_ticket: NonBlank
    reviewed_at: datetime
    passed: Literal[True]

    @model_validator(mode="after")
    def complete_release_proof(self) -> "MediaProviderReleaseEvidence":
        required = {CrashPoint.BEFORE_HTTP,
                    CrashPoint.AFTER_PAID_RESPONSE_BEFORE_PERSISTENCE}
        actual = {attempt.fault_point for attempt in self.attempts}
        if actual != required or len(self.attempts) != len(required):
            raise ValueError("evidence needs exactly the two required crash cells")
        if len({attempt.client_attempt_id for attempt in self.attempts}) != 2:
            raise ValueError("each crash cell needs a distinct durable attempt ID")
        if sum(item.supplier_amount_fen for item in self.attempts) > self.max_total_fen:
            raise ValueError("supplier spend exceeded the hard PoC budget")
        if (
            self.recovery_proof
            is RecoveryProof.SUPPLIER_STATEMENT_RECONCILIATION
            and self.statement_reconciliation_implementation_sha256 is None
        ):
            raise ValueError("statement recovery needs the deployed reconciler hash")
        if (
            self.recovery_proof
            in {RecoveryProof.OFFICIAL_QUERY, RecoveryProof.OFFICIAL_IDEMPOTENCY}
            and self.official_request_identity_observation_sha256 is None
        ):
            raise ValueError("official recovery needs an observed identity-proof hash")
        return self


class ProviderReleaseGate:
    def __init__(self, evidence: MediaProviderReleaseEvidence | None) -> None:
        self._evidence = evidence

    @classmethod
    def disabled(cls) -> "ProviderReleaseGate":
        return cls(None)

    @classmethod
    def load(cls, path: Path, expected_sha256: str) -> "ProviderReleaseGate":
        raw = path.read_bytes()
        if sha256(raw).hexdigest() != expected_sha256:
            raise Forbidden("media provider release evidence digest mismatch")
        return cls(MediaProviderReleaseEvidence.model_validate_json(raw))

    def require(
        self,
        *,
        adapter: MediaAdapterKind,
        capability: str,
        model_id: str,
        model_version: str,
        region: str,
        reconciler_sha256: str,
    ) -> None:
        evidence = self._evidence
        if evidence is None:
            raise Forbidden("media provider release evidence is not configured")
        actual = (
            evidence.adapter,
            capability in evidence.capabilities,
            evidence.model_id,
            evidence.model_version,
            evidence.region,
        )
        expected = (adapter, True, model_id, model_version, region)
        if actual != expected:
            raise Forbidden("media provider evidence does not match active binding")
        if (
            evidence.recovery_proof
            is RecoveryProof.SUPPLIER_STATEMENT_RECONCILIATION
            and evidence.statement_reconciliation_implementation_sha256
            != reconciler_sha256
        ):
            raise Forbidden("deployed supplier reconciler digest does not match evidence")
        if evidence.recovery_proof not in {
            RecoveryProof.OFFICIAL_QUERY,
            RecoveryProof.OFFICIAL_IDEMPOTENCY,
            RecoveryProof.SUPPLIER_STATEMENT_RECONCILIATION,
        }:
            raise Forbidden("provider recovery proof is not supported")
```

The synthetic manifest in the unit test proves only strict parsing and digest enforcement. It is never installed under a production evidence path and cannot activate an ordinary test process.

- [ ] **Step 4: Add durable provider-attempt and supplier-reconciliation lineage to `0003_media`**

Create `backend/src/ip_saas/modules/media/models/provider_ops.py`:

```python
from datetime import datetime
from uuid import UUID, uuid4

from sqlalchemy import (
    Boolean,
    CheckConstraint,
    DateTime,
    ForeignKey,
    Index,
    Integer,
    String,
    UniqueConstraint,
    event,
    inspect,
)
from sqlalchemy.dialects.postgresql import UUID as PGUUID
from sqlalchemy.orm import Mapped, mapped_column

from ip_saas.db.base import Base


class ProviderRequestAttempt(Base):
    __tablename__ = "media_provider_request_attempts"

    id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), primary_key=True, default=uuid4
    )
    task_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("task_records.id"), nullable=False, index=True
    )
    generation_job_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("generation_jobs.id"), nullable=False
    )
    provider: Mapped[str] = mapped_column(String(60), nullable=False)
    adapter: Mapped[str] = mapped_column(String(40), nullable=False)
    capability: Mapped[str] = mapped_column(String(60), nullable=False)
    model_id: Mapped[str] = mapped_column(String(200), nullable=False)
    model_version: Mapped[str] = mapped_column(String(100), nullable=False)
    logical_call_key: Mapped[str] = mapped_column(String(120), nullable=False)
    client_attempt_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), nullable=False
    )
    worker_attempt_no: Mapped[int] = mapped_column(Integer, nullable=False)
    expects_supplier_charge: Mapped[bool] = mapped_column(
        Boolean, nullable=False
    )
    request_fingerprint: Mapped[str] = mapped_column(String(64), nullable=False)
    request_payload_sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    status: Mapped[str] = mapped_column(
        String(32), nullable=False, default="prepared"
    )
    provider_request_id: Mapped[str | None] = mapped_column(String(200))
    response_payload_sha256: Mapped[str | None] = mapped_column(String(64))
    send_started_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
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
            name="uq_media_provider_attempt_client_id",
        ),
        UniqueConstraint(
            "generation_job_id", "logical_call_key",
            name="uq_media_provider_attempt_logical_call",
        ),
        UniqueConstraint(
            "provider", "provider_request_id",
            name="uq_media_provider_attempt_request_identity",
        ),
        CheckConstraint(
            "status IN ('prepared', 'response_observed', "
            "'supplier_matched', 'reconciled')",
            name="ck_media_provider_attempt_status",
        ),
        CheckConstraint(
            "worker_attempt_no > 0",
            name="ck_media_provider_attempt_worker_attempt",
        ),
        CheckConstraint(
            "length(request_fingerprint) = 64",
            name="ck_media_provider_attempt_request_fingerprint",
        ),
        CheckConstraint(
            "length(request_payload_sha256) = 64",
            name="ck_media_provider_attempt_request_sha",
        ),
        CheckConstraint(
            "supplier_request_log_sha256 IS NULL "
            "OR length(supplier_request_log_sha256) = 64",
            name="ck_media_provider_attempt_supplier_log_sha",
        ),
        CheckConstraint(
            "provider_request_id IS NULL OR "
            "(provider_request_id = btrim(provider_request_id) AND "
            "lower(provider_request_id) NOT IN ('none', 'missing') AND "
            "length(provider_request_id) BETWEEN 1 AND 200)",
            name="ck_media_provider_attempt_request_id",
        ),
        Index("ix_media_provider_attempt_task_status", "task_id", "status"),
    )


class ProviderSupplierReconciliation(Base):
    __tablename__ = "media_provider_supplier_reconciliations"

    id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), primary_key=True, default=uuid4
    )
    provider_request_attempt_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True),
        ForeignKey("media_provider_request_attempts.id"),
        nullable=False,
        unique=True,
    )
    task_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True), ForeignKey("task_records.id"), nullable=False, index=True
    )
    provider: Mapped[str] = mapped_column(String(60), nullable=False)
    provider_request_id: Mapped[str] = mapped_column(String(200), nullable=False)
    supplier_statement_line_id: Mapped[str] = mapped_column(
        String(300), nullable=False
    )
    provider_request_log_sha256: Mapped[str] = mapped_column(
        String(64), nullable=False
    )
    supplier_statement_sha256: Mapped[str] = mapped_column(
        String(64), nullable=False
    )
    provider_cost_entry_id: Mapped[UUID] = mapped_column(
        PGUUID(as_uuid=True),
        ForeignKey("provider_cost_entries.id"),
        nullable=False,
        unique=True,
    )
    supplier_amount_minor: Mapped[int] = mapped_column(Integer, nullable=False)
    supplier_currency: Mapped[str] = mapped_column(String(3), nullable=False)
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
            "provider", "provider_request_id",
            name="uq_media_supplier_reconciliation_provider_request",
        ),
        CheckConstraint(
            "supplier_amount_minor >= 0 AND amount_fen >= 0",
            name="ck_media_supplier_reconciliation_amounts",
        ),
        CheckConstraint(
            "length(provider_request_log_sha256) = 64 "
            "AND length(supplier_statement_sha256) = 64 "
            "AND length(input_fingerprint) = 64",
            name="ck_media_supplier_reconciliation_hashes",
        ),
        CheckConstraint(
            "provider_request_id = btrim(provider_request_id) AND "
            "lower(provider_request_id) NOT IN ('none', 'missing') AND "
            "length(provider_request_id) BETWEEN 1 AND 200",
            name="ck_media_supplier_reconciliation_request_id",
        ),
    )


@event.listens_for(ProviderRequestAttempt, "before_update")
def _attempt_identity_is_immutable(
    _mapper: object, _connection: object, target: ProviderRequestAttempt,
) -> None:
    state = inspect(target)
    immutable = (
        "task_id", "generation_job_id", "provider", "adapter", "capability",
        "model_id", "model_version", "logical_call_key", "client_attempt_id",
        "expects_supplier_charge", "request_fingerprint",
        "request_payload_sha256", "prepared_at",
    )
    if any(state.attrs[name].history.has_changes() for name in immutable):
        raise RuntimeError("provider request-attempt identity is immutable")
    send_history = state.attrs.send_started_at.history
    if send_history.has_changes() and (
        not send_history.added
        or send_history.added[0] is None
        or any(value is not None for value in send_history.deleted)
    ):
        raise RuntimeError("provider send marker is monotonic")


@event.listens_for(ProviderSupplierReconciliation, "before_update")
@event.listens_for(ProviderSupplierReconciliation, "before_delete")
def _supplier_reconciliation_is_append_only(*_args: object) -> None:
    raise RuntimeError("supplier reconciliation evidence is append-only")
```

Export both classes from `backend/src/ip_saas/modules/media/models/__init__.py`. Modify the single `backend/migrations/versions/0003_media.py`: create `media_provider_request_attempts` after `generation_jobs`, then create `media_provider_supplier_reconciliations`; use the same columns, foreign keys, named checks, named unique constraints, and index as the ORM. Both `provider_request_id` columns are exactly `sa.String(200)`. The attempt column is nullable only while no supplier identity has been observed and uses `ck_media_provider_attempt_request_id`; any non-null value must already equal `btrim(value)` and have length 1–200. The reconciliation column is non-null and uses `ck_media_supplier_reconciliation_request_id` with the same canonical constraint. In `downgrade()`, drop the reconciliation table first and the attempt table second, before dropping `generation_jobs`. Do not create another revision or Alembic head.

Add these assertions to `backend/tests/integration/media/test_media_migration.py`:

```python
def test_media_migration_contains_provider_crash_lineage(pg_engine) -> None:
    inspector = inspect(pg_engine)
    assert {
        "media_provider_request_attempts",
        "media_provider_supplier_reconciliations",
    } <= set(inspector.get_table_names())
    attempt_uniques = {
        row["name"] for row in inspector.get_unique_constraints(
            "media_provider_request_attempts"
        )
    }
    assert {
        "uq_media_provider_attempt_client_id",
        "uq_media_provider_attempt_logical_call",
        "uq_media_provider_attempt_request_identity",
    } <= attempt_uniques
    attempt_columns = {
        row["name"]: row for row in inspector.get_columns(
            "media_provider_request_attempts"
        )
    }
    reconciliation_columns = {
        row["name"]: row for row in inspector.get_columns(
            "media_provider_supplier_reconciliations"
        )
    }
    assert attempt_columns["provider_request_id"]["type"].length == 200
    assert attempt_columns["provider_request_id"]["nullable"] is True
    assert reconciliation_columns["provider_request_id"]["type"].length == 200
    assert reconciliation_columns["provider_request_id"]["nullable"] is False
    attempt_checks = {
        row["name"] for row in inspector.get_check_constraints(
            "media_provider_request_attempts"
        )
    }
    reconciliation_checks = {
        row["name"] for row in inspector.get_check_constraints(
            "media_provider_supplier_reconciliations"
        )
    }
    assert "ck_media_provider_attempt_request_id" in attempt_checks
    assert "ck_media_supplier_reconciliation_request_id" in reconciliation_checks
```

Run:

```bash
cd backend
uv run alembic downgrade 0002_intelligence
uv run alembic upgrade 0003_media
uv run pytest \
  tests/unit/media/test_provider_release.py \
  tests/unit/media/test_provider_attempt_models.py \
  tests/integration/media/test_media_migration.py -q
uv run alembic downgrade 0002_intelligence
uv run alembic upgrade 0003_media
uv run alembic check
```

Expected: tests pass; both upgrade cycles pass; Alembic reports `No new upgrade operations detected.`; there is still exactly one head, `0003_media`.

- [ ] **Step 5: Write failing exact-crash, supplier-first, idempotency, and stale-fence tests**

Create `backend/tests/integration/media/test_provider_attempt_reconciliation.py`:

```python
from uuid import uuid4

import pytest
from pydantic import ValidationError
from sqlalchemy import func, select

from ip_saas.common.errors import Conflict
from ip_saas.common.tasking import TaskStatus
from ip_saas.modules.billing.models import ProviderCostEntry
from ip_saas.modules.media.models.provider_ops import (
    ProviderRequestAttempt,
    ProviderSupplierReconciliation,
)
from ip_saas.modules.media.provider_poc import InjectedProviderCrash
from ip_saas.modules.media.provider_reconciliation import SupplierRequestLogLine
from ip_saas.workers.media_lease import ProcessDisposition
from tests.integration.media.harness import MediaIntegrationHarness


def test_provider_request_id_is_trimmed_before_reconciliation() -> None:
    line = SupplierRequestLogLine(
        provider_request_id="  supplier-request-1  ",
        client_attempt_id=uuid4(),
        provider="volcengine",
        adapter="seedream",
        model_id="seedream-model",
        model_version="v1",
        billable=True,
    )
    assert line.provider_request_id == "supplier-request-1"


@pytest.mark.parametrize(
    "raw", [None, "", "   ", "nOnE", "MISSING", "x" * 201]
)
def test_provider_request_id_rejects_null_blank_and_201(raw: object) -> None:
    with pytest.raises(ValidationError):
        SupplierRequestLogLine(
            provider_request_id=raw,
            client_attempt_id=uuid4(),
            provider="volcengine",
            adapter="seedream",
            model_id="seedream-model",
            model_version="v1",
            billable=True,
        )


@pytest.mark.parametrize("adapter", ["seedream", "seedance", "standard_tts"])
@pytest.mark.parametrize(
    "raw", [None, 7, "", "   ", "nOnE", "MISSING", "x" * 201]
)
def test_invalid_real_response_identity_retries_to_supplier_reconciliation(
    db_session,
    adapter: str,
    raw: object,
) -> None:
    case = MediaIntegrationHarness(db_session).provider_crash(adapter)
    assert case.run_invalid_provider_request_identity(raw) is ProcessDisposition.RETRY
    assert case.provider_call_count() == 1
    assert (
        case.job.provider_request_id
        == case.expected_provider_request_id_before_crash
    )
    assert case.provider_cost_count() == 0
    assert case.hold_status() == "active"
    case.exhaust_to_reconciliation_required()
    assert case.task_status() == TaskStatus.RECONCILIATION_REQUIRED
    assert case.provider_call_count() == 1
    assert case.provider_cost_count() == 0


def test_seedance_poll_request_identity_never_replaces_billable_submit_identity(
    db_session,
) -> None:
    case = MediaIntegrationHarness(db_session).provider_crash("seedance")
    case.run_success_to_settlement()
    assert case.job.provider_task_id == case.expected_provider_task_id
    assert case.job.provider_request_id == case.billable_provider_request_id
    assert case.terminal_http_request_id != case.expected_provider_task_id
    assert case.terminal_http_request_id != case.billable_provider_request_id
    assert case.provider_request_ids_by_chargeability() == {
        (case.billable_provider_request_id, True),
        (case.terminal_http_request_id, False),
    }
    assert case.provider_cost_request_ids() == {
        case.billable_provider_request_id
    }


@pytest.mark.parametrize("adapter", ["seedream", "seedance", "standard_tts"])
def test_exact_paid_response_crash_is_reconciled_from_supplier_to_task_cost(
    db_session,
    adapter: str,
) -> None:
    case = MediaIntegrationHarness(db_session).provider_crash(adapter)
    with pytest.raises(InjectedProviderCrash, match="paid response persisted nowhere"):
        case.run_exact_crash()

    db_session.expire_all()
    attempt = db_session.get(
        ProviderRequestAttempt, case.provider_request_attempt_id
    )
    assert attempt is not None
    assert attempt.client_attempt_id == case.client_attempt_id
    assert attempt.status == "prepared"
    assert attempt.send_started_at is not None
    assert attempt.provider_request_id is None
    assert case.job.provider_task_id == case.expected_provider_task_id
    assert case.job.provider_request_id is None
    assert db_session.scalar(select(func.count()).select_from(ProviderCostEntry)) == 0

    case.exhaust_to_reconciliation_required()
    assert case.task_status() == TaskStatus.RECONCILIATION_REQUIRED
    assert case.hold_status() == "active"
    links = case.reconciler.reconcile(
        db_session,
        request_log=case.request_log_export(),
        statement=case.final_statement_export(),
        provider_request_log_sha256="a" * 64,
        supplier_statement_sha256="b" * 64,
        idempotency_key=f"supplier-final:{case.run_id}",
    )
    db_session.flush()
    assert len(links) == 1
    cost = db_session.get(ProviderCostEntry, links[0].provider_cost_entry_id)
    assert cost is not None and cost.task_id == case.task.id
    assert links[0].task_id == case.task.id
    assert case.task_status() == TaskStatus.FAILED
    assert case.internal_spend_fen() == case.supplier_amount_fen


def test_customer_lost_output_records_supplier_cost_but_charges_zero(db_session) -> None:
    case = MediaIntegrationHarness(db_session).provider_crash(
        "seedream", billing_mode="customer_credit"
    )
    with pytest.raises(InjectedProviderCrash):
        case.run_exact_crash()
    case.exhaust_to_reconciliation_required()
    case.reconciler.reconcile(
        db_session,
        request_log=case.request_log_export(),
        statement=case.final_statement_export(),
        provider_request_log_sha256="c" * 64,
        supplier_statement_sha256="d" * 64,
        idempotency_key=f"supplier-final:{case.run_id}",
    )
    assert case.customer_debit() == 0
    assert case.hold_status() == "released"
    assert case.provider_cost_task_ids() == {case.task.id}


def test_supplier_statement_is_the_starting_set_and_cannot_use_a_db_antijoin(
    db_session,
) -> None:
    case = MediaIntegrationHarness(db_session).provider_crash("seedance")
    with pytest.raises(InjectedProviderCrash):
        case.run_exact_crash()
    case.exhaust_to_reconciliation_required()
    statement = case.final_statement_export(provider_request_id="unmapped-request")
    with pytest.raises(Conflict, match="request-log and final-statement sets differ"):
        case.reconciler.reconcile(
            db_session,
            request_log=case.request_log_export(),
            statement=statement,
            provider_request_log_sha256="e" * 64,
            supplier_statement_sha256="f" * 64,
            idempotency_key=f"supplier-final:{case.run_id}",
        )
    assert db_session.scalar(select(func.count()).select_from(ProviderCostEntry)) == 0


def test_two_supplier_costs_for_one_task_use_one_central_finalizer(db_session) -> None:
    case = MediaIntegrationHarness(db_session).provider_crash(
        "standard_tts", billable_calls=2
    )
    case.run_all_exact_crashes()
    case.exhaust_to_reconciliation_required()
    arguments = {
        "request_log": case.request_log_export(),
        "statement": case.final_statement_export(),
        "provider_request_log_sha256": "1" * 64,
        "supplier_statement_sha256": "2" * 64,
        "idempotency_key": f"supplier-final:{case.run_id}",
    }
    first = case.reconciler.reconcile(db_session, **arguments)
    assert len(first) == 2
    assert db_session.scalar(select(func.count()).select_from(ProviderCostEntry)) == 2
    assert db_session.scalar(
        select(func.count()).select_from(ProviderSupplierReconciliation)
    ) == 2
    assert case.task_finalizer_call_count() == 1
    assert case.hold_mutation_count() == 1
    assert case.internal_spend_fen() == case.supplier_amount_fen


def test_missing_supplier_line_rolls_back_whole_task_batch(db_session) -> None:
    case = MediaIntegrationHarness(db_session).provider_crash(
        "seedance", billable_calls=2
    )
    case.run_all_exact_crashes()
    case.exhaust_to_reconciliation_required()
    with pytest.raises(Conflict, match="request-log and final-statement sets differ"):
        case.reconciler.reconcile(
            db_session,
            request_log=case.request_log_export(),
            statement=case.final_statement_export(omit_last=True),
            provider_request_log_sha256="5" * 64,
            supplier_statement_sha256="6" * 64,
            idempotency_key=f"supplier-final:{case.run_id}",
        )
    assert case.provider_cost_count() == 0
    assert case.task_finalizer_call_count() == 0
    assert case.hold_status() == "active"
    assert case.task_status() == TaskStatus.RECONCILIATION_REQUIRED


def test_jointly_omitted_sent_request_is_rejected_by_durable_manifest(db_session) -> None:
    case = MediaIntegrationHarness(db_session).provider_crash(
        "standard_tts", billable_calls=2
    )
    case.run_all_exact_crashes()
    case.exhaust_to_reconciliation_required()
    with pytest.raises(Conflict, match="manifest"):
        case.reconciler.reconcile(
            db_session,
            request_log=case.request_log_export(omit_last=True),
            statement=case.final_statement_export(omit_last=True),
            provider_request_log_sha256="9" * 64,
            supplier_statement_sha256="0" * 64,
            idempotency_key=f"supplier-final:{case.run_id}",
        )
    assert case.provider_cost_count() == 0
    assert case.task_finalizer_call_count() == 0
    assert case.hold_status() == "active"
    assert case.task_status() == TaskStatus.RECONCILIATION_REQUIRED


def test_reversed_supplier_order_replays_same_two_costs(db_session) -> None:
    case = MediaIntegrationHarness(db_session).provider_crash(
        "seedream", billable_calls=2
    )
    case.run_all_exact_crashes()
    case.exhaust_to_reconciliation_required()
    arguments = {
        "request_log": case.request_log_export(),
        "statement": case.final_statement_export(),
        "provider_request_log_sha256": "7" * 64,
        "supplier_statement_sha256": "8" * 64,
        "idempotency_key": f"supplier-final:{case.run_id}",
    }
    first = case.reconciler.reconcile(db_session, **arguments)
    second = case.reconciler.reconcile(
        db_session,
        **{**arguments, "statement": case.final_statement_export(reverse=True)},
    )
    assert [item.id for item in first] == [item.id for item in second]
    assert case.provider_cost_count() == 2
    assert case.task_finalizer_call_count() == 1
    assert case.hold_mutation_count() == 1


def test_stale_attempt_cannot_write_supplier_cost_or_terminal_state(db_session) -> None:
    case = MediaIntegrationHarness(db_session).provider_crash("seedream")
    with pytest.raises(InjectedProviderCrash):
        case.run_exact_crash()
    attempt_no = case.exhaust_to_reconciliation_required()
    attempt = db_session.get(
        ProviderRequestAttempt, case.provider_request_attempt_id
    )
    assert attempt is not None
    attempt.worker_attempt_no = attempt_no + 1
    db_session.flush()
    with pytest.raises(Conflict, match="newer than the exhausted task attempt"):
        case.reconciler.reconcile(
            db_session,
            request_log=case.request_log_export(),
            statement=case.final_statement_export(),
            provider_request_log_sha256="3" * 64,
            supplier_statement_sha256="4" * 64,
            idempotency_key=f"supplier-final:{case.run_id}",
        )
    assert db_session.scalar(select(func.count()).select_from(ProviderCostEntry)) == 0
    assert case.task_status() == TaskStatus.RECONCILIATION_REQUIRED
    assert case.hold_status() == "active"


def test_poc_provider_turn_expired_at_enter_retries_without_http_and_releases_only_after_proof(
    db_session,
) -> None:
    case = MediaIntegrationHarness(db_session).provider_crash("seedream")
    case.expire_immediately_before_provider_turn()
    assert case.run_once() == ProcessDisposition.RETRY
    assert case.provider_call_count() == 0
    assert case.provider_cost_count() == 0
    assert case.domain_write_count() == 0
    assert case.hold_status() == "active"
    attempt_no = case.exhaust_to_reconciliation_required()
    failed = case.finalize_no_provider_call(attempt_no)
    assert failed.status == TaskStatus.FAILED
    assert case.hold_status() == "released"
    assert case.provider_cost_count() == 0
```

Extend `backend/tests/integration/media/harness.py` with this explicit public return shape; do not add another fixture:

```python
@dataclass(frozen=True)
class ProviderCrashScenario:
    worker: object
    reconciler: object
    job: object
    task: object
    run_id: UUID
    client_attempt_id: UUID
    provider_request_attempt_id: UUID
    expected_provider_task_id: str | None
    expected_provider_request_id_before_crash: str | None
    billable_provider_request_id: str
    terminal_http_request_id: str
    supplier_amount_fen: int
    run_exact_crash: Callable[[], None]
    run_all_exact_crashes: Callable[[], None]
    run_invalid_provider_request_identity: Callable[[object], object]
    run_success_to_settlement: Callable[[], None]
    run_once: Callable[[], object]
    exhaust_to_reconciliation_required: Callable[[], int]
    finalize_no_provider_call: Callable[[int], object]
    expire_immediately_before_provider_turn: Callable[[], None]
    request_log_export: Callable[..., object]
    final_statement_export: Callable[..., object]
    task_status: Callable[[], str]
    hold_status: Callable[[], str]
    customer_debit: Callable[[], int]
    internal_spend_fen: Callable[[], int]
    provider_cost_task_ids: Callable[[], set[UUID]]
    provider_cost_request_ids: Callable[[], set[str]]
    provider_request_ids_by_chargeability: Callable[[], set[tuple[str, bool]]]
    provider_cost_count: Callable[[], int]
    provider_call_count: Callable[[], int]
    domain_write_count: Callable[[], int]
    task_finalizer_call_count: Callable[[], int]
    hold_mutation_count: Callable[[], int]


class MediaIntegrationHarness:
    def provider_crash(
        self,
        adapter: str,
        *,
        billing_mode: str = "internal_cost",
        billable_calls: int = 1,
    ) -> ProviderCrashScenario:
        capability = {
            "seedream": "storyboard_image",
            "seedance": "low_res_video",
            "standard_tts": "standard_tts",
        }[adapter]
        return self._seed_provider_crash_case(
            adapter, capability, billing_mode, billable_calls
        )
```

Implement `_seed_provider_crash_case` in the same harness file, using the existing real `TaskSubmissionService`, `BillingService(..., AllowAllGenerationLimits())`, one real Plan 01 `TaskReconciliationService` configured with the media attempt-journal evidence ports from Step 7, `GenerationJob`, `MediaSubmitWorker`/`MediaPollWorker`, and SQL hold adapters. It creates one server-owned job/task/hold, `billable_calls` distinct logical request-attempt rows under that same task, scripted terminal results with positive complete usage, and the exact injected-crash hook from Step 6. Each scripted real response has a supplier `provider_request_id` that is deliberately different from its `provider_task_id`; Seedream and standard TTS expose `expected_provider_task_id=None` and no prior request identity, while Seedance exposes the independent polling task ID plus the canonical billable submit request identity already committed before the terminal-poll crash. `expected_provider_request_id_before_crash` captures exactly that durable pre-crash state; it never stands in for the terminal HTTP response identity that the injected crash prevented from persisting. For Seedance, `billable_provider_request_id`, `terminal_http_request_id`, and the two SQL-backed set readers prove the chargeable submit identity and non-billable poll correlation remain distinct; `run_success_to_settlement()` exercises the real submit/poll/apply/settlement path and creates a cost only for the former. `run_invalid_provider_request_identity(raw)` executes the selected adapter through its real normalization boundary after one committed send marker, substitutes only the raw supplier request identity, and returns the Worker disposition; it must leave `GenerationJob.provider_request_id`, response evidence, domain rows, and provider-cost rows untouched. `run_all_exact_crashes()` catches the expected injected exception for every logical call. `request_log_export(omit_last=False)` and `final_statement_export(reverse=False, omit_last=False, provider_request_id=None)` return the strict Step 7 classes; reversing changes only line order, omitting removes exactly one billed line, and overriding the ID changes only one statement line. `exhaust_to_reconciliation_required()` advances the harness clock beyond each lease and invokes real `TaskSubmissionService.start(..., max_attempts=8)` until it returns `RECONCILIATION_REQUIRED`, asserting the original hold remains active and no provider is called again. `finalize_no_provider_call(attempt_no)` calls that same service's frozen `finalize_no_provider_call(session, task.id, attempt_no, "media provider HTTP send never began", f"media:no-provider-call:{task.id}")`; the real `MediaNoProviderCallEvidence` must reject an empty journal or any send marker. A spy counts calls to the one injected central finalizer, while hold counts and costs come from SQL rows, not mocks. `expire_immediately_before_provider_turn()` advances the clock after snapshot/attempt preparation but immediately before `LeaseHeartbeat.__enter__`, so its synchronous heartbeat rejects the call. No TaskRecord is constructed by hand, no production module is patched, and no network call occurs.

- [ ] **Step 6: Persist one retry-stable request-attempt before HTTP and inject the exact crash window**

Add this request journal to `backend/src/ip_saas/modules/media/provider_reconciliation.py`:

```python
from collections.abc import Callable, Mapping
from datetime import datetime
from hashlib import sha256
from json import dumps
from uuid import UUID, uuid5

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.errors import Conflict
from ip_saas.common.tasking import TaskStatus
from ip_saas.modules.media.models.jobs import GenerationJob
from ip_saas.modules.media.models.provider_ops import ProviderRequestAttempt
from ip_saas.modules.tasks.models import TaskRecord
from ip_saas.providers.media.base import (
    ProviderResult,
    canonical_provider_request_id,
)


MEDIA_PROVIDER_ATTEMPT_NAMESPACE = UUID("6f122d6d-df21-4f42-94d4-88e76736cc41")


def canonical_sha256(value: Mapping[str, object]) -> str:
    encoded = dumps(
        dict(value), sort_keys=True, separators=(",", ":"), default=str
    ).encode("utf-8")
    return sha256(encoded).hexdigest()


class ProviderAttemptJournal:
    def __init__(self, now: Callable[[], datetime]) -> None:
        self.now = now

    def prepare(
        self,
        session: Session,
        *,
        job: GenerationJob,
        task: TaskRecord,
        worker_attempt_no: int,
        adapter: str,
        logical_call_key: str,
        expects_supplier_charge: bool,
    ) -> ProviderRequestAttempt:
        if task.status != TaskStatus.RUNNING or task.attempt_no != worker_attempt_no:
            raise Conflict("stale media worker cannot prepare provider request")
        client_attempt_id = uuid5(
            MEDIA_PROVIDER_ATTEMPT_NAMESPACE,
            f"{task.id}:{job.id}:{job.provider}:{adapter}:{logical_call_key}",
        )
        request_payload_sha256 = canonical_sha256(job.request_payload)
        existing = session.scalar(select(ProviderRequestAttempt).where(
            ProviderRequestAttempt.provider == job.provider,
            ProviderRequestAttempt.client_attempt_id == client_attempt_id,
        ).with_for_update())
        expected = (
            task.id, job.id, job.capability, job.model_id, job.model_version,
            logical_call_key, expects_supplier_charge,
            task.request_fingerprint, request_payload_sha256,
        )
        if existing is not None:
            actual = (
                existing.task_id, existing.generation_job_id,
                existing.capability, existing.model_id, existing.model_version,
                existing.logical_call_key, existing.expects_supplier_charge,
                existing.request_fingerprint,
                existing.request_payload_sha256,
            )
            if actual != expected:
                raise Conflict("provider attempt ID was reused with different input")
            existing.worker_attempt_no = worker_attempt_no
            return existing
        attempt = ProviderRequestAttempt(
            task_id=task.id,
            generation_job_id=job.id,
            provider=job.provider,
            adapter=adapter,
            capability=job.capability,
            model_id=job.model_id,
            model_version=job.model_version,
            logical_call_key=logical_call_key,
            client_attempt_id=client_attempt_id,
            worker_attempt_no=worker_attempt_no,
            expects_supplier_charge=expects_supplier_charge,
            request_fingerprint=task.request_fingerprint,
            request_payload_sha256=request_payload_sha256,
            status="prepared",
            prepared_at=self.now(),
        )
        session.add(attempt)
        session.flush()
        return attempt

    def mark_send_started(
        self,
        session: Session,
        *,
        attempt_id: UUID,
        task_id: UUID,
        worker_attempt_no: int,
    ) -> ProviderRequestAttempt:
        task = session.scalar(select(TaskRecord).where(
            TaskRecord.id == task_id
        ).with_for_update())
        attempt = session.scalar(select(ProviderRequestAttempt).where(
            ProviderRequestAttempt.id == attempt_id
        ).with_for_update())
        if (
            task is None
            or attempt is None
            or task.status != TaskStatus.RUNNING
            or task.attempt_no != worker_attempt_no
            or attempt.worker_attempt_no != worker_attempt_no
        ):
            raise Conflict("stale media worker cannot mark provider send")
        if attempt.send_started_at is not None:
            raise Conflict("unknown provider send requires reconciliation")
        attempt.send_started_at = self.now()
        session.flush()
        return attempt

    def record_response(
        self,
        session: Session,
        *,
        attempt_id: UUID,
        task_id: UUID,
        worker_attempt_no: int,
        result: ProviderResult,
    ) -> ProviderRequestAttempt:
        task = session.scalar(select(TaskRecord).where(
            TaskRecord.id == task_id
        ).with_for_update())
        attempt = session.scalar(select(ProviderRequestAttempt).where(
            ProviderRequestAttempt.id == attempt_id
        ).with_for_update())
        if (
            task is None
            or attempt is None
            or task.status != TaskStatus.RUNNING
            or task.attempt_no != worker_attempt_no
            or attempt.worker_attempt_no != worker_attempt_no
            or attempt.send_started_at is None
        ):
            raise Conflict("stale media worker cannot record provider response")
        response = {
            "phase": result.phase.value,
            "provider_task_id": result.provider_task_id,
            "provider_request_id": result.provider_request_id,
            "model_id": result.model_id,
            "model_version": result.model_version,
            "usage": None if result.usage is None else {
                "native_quantity": result.usage.native_quantity,
                "native_unit": result.usage.native_unit,
                "supplier_amount_minor": result.usage.supplier_amount_minor,
                "supplier_currency": result.usage.supplier_currency,
                "amount_fen": result.usage.amount_fen,
            },
        }
        attempt.provider_request_id = canonical_provider_request_id(
            result.provider_request_id
        )
        attempt.response_payload_sha256 = canonical_sha256(response)
        attempt.response_observed_at = self.now()
        attempt.status = "response_observed"
        return attempt
```

The identity algorithm uses server-created task/job IDs and a logical call key, never the customer idempotency key, Worker `attempt_no`, wall-clock time, or randomness. Generation submits and any other operation the supplier can charge call `prepare(..., expects_supplier_charge=True)`; a proven non-billable status lookup uses `False`. The flag is immutable request identity, supplier reconciliation accepts only `True` rows, and the provider-cost manifest requires every sent `True` row while ignoring `False` lookups for cost-set equality. Both kinds still count as provider sends, so `MediaNoProviderCallEvidence` rejects either kind after `send_started_at` is set.

Create the injectable boundary in `backend/src/ip_saas/modules/media/provider_poc.py`:

```python
from typing import Protocol

from ip_saas.providers.media.base import ProviderResult


class InjectedProviderCrash(RuntimeError):
    pass


class ProviderFaultInjector(Protocol):
    def before_http(self, snapshot: object) -> None:
        raise NotImplementedError

    def after_paid_response_before_persistence(
        self, snapshot: object, result: ProviderResult,
    ) -> None:
        raise NotImplementedError


class NoProviderFaults:
    def before_http(self, snapshot: object) -> None:
        return None

    def after_paid_response_before_persistence(
        self, snapshot: object, result: ProviderResult,
    ) -> None:
        return None


class RaiseAfterPaidResponse:
    def before_http(self, snapshot: object) -> None:
        return None

    def after_paid_response_before_persistence(
        self, snapshot: object, result: ProviderResult,
    ) -> None:
        raise InjectedProviderCrash("paid response persisted nowhere")
```

Modify `ProviderJobSnapshot` to carry `provider_request_attempt_id: UUID`, `client_attempt_id: UUID`, and `send_started_at: datetime | None`. Inject `ProviderAttemptJournal` plus `ProviderFaultInjector` into `MediaSubmitWorker` and `MediaPollWorker`. During `_claim`, call `journal.prepare(..., expects_supplier_charge=True)` for every supplier-chargeable submit/poll after `TaskSubmissionService.start()` and before returning the snapshot; a status-only lookup proven non-billable passes `False`. Because `_claim` exits `session_scope()` before the provider turn, the attempt ID and charge expectation are committed before HTTP. Rebuild the immutable `ProviderRequest` with `idempotency_key=str(attempt.client_attempt_id)`. If a reclaimed snapshot has `send_started_at` but no response evidence, return RETRY without issuing another HTTP call; repeated zero-new-call reclaims eventually let Plan 01 `start()` move the task to `RECONCILIATION_REQUIRED` with its hold active.

The provider turn order is frozen as follows. `LeaseHeartbeat.__enter__` performs the synchronous attempt/expiry CAS before the fault hook or send marker; a just-expired or stale attempt therefore returns RETRY with zero provider calls. `mark_send_started()` then commits in its own short transaction before HTTP, so a crash after that point is conservatively supplier-reconciled rather than resent:

```python
from .media_lease import (
    LeaseHeartbeat, LeaseLost, ProcessDisposition,
    disposition_after_lease_loss,
)
from ip_saas.providers.media.base import InvalidProviderRequestIdentity


try:
    with LeaseHeartbeat(
        self.task_submission,
        snapshot.task_record_id,
        snapshot.attempt_no,
    ) as heartbeat:
        self.faults.before_http(snapshot)
        with session_scope() as session:
            self.journal.mark_send_started(
                session,
                attempt_id=snapshot.provider_request_attempt_id,
                task_id=snapshot.task_record_id,
                worker_attempt_no=snapshot.attempt_no,
            )
        result = provider.submit(snapshot.request)
        if result.usage is not None:
            self.faults.after_paid_response_before_persistence(snapshot, result)
        heartbeat.ensure_owned()
except LeaseLost:
    return disposition_after_lease_loss(
        snapshot.task_record_id, snapshot.attempt_no
    )
except Conflict:
    return ProcessDisposition.RETRY
except InvalidProviderRequestIdentity:
    return ProcessDisposition.RETRY
return self._apply(snapshot, result)
```

For Seedance, use the same `LeaseHeartbeat` entry and send-marker check around each provider operation, then run the second hook in `MediaPollWorker` immediately after the first terminal result containing usage and before opening the `_apply` transaction. The chargeable creation submit passes `expects_supplier_charge=True`; an independently correlated, proven non-billable status lookup passes `False`. Both synchronously heartbeat before HTTP and carry separate stable logical-call attempts. In both workers, `_apply` first reacquires and heartbeats the same task/`attempt_no`, then calls `attempt = journal.record_response(...)` in the same transaction as updating `GenerationJob`. `record_response` persists the current HTTP result's canonical `result.provider_request_id` in that exact attempt and stores `result.provider_task_id` only inside hashed polling metadata; it never derives either from the other. Replace Task 12's provisional unconditional assignment with this final job-identity invariant:

```python
if attempt.expects_supplier_charge:
    if job.provider_request_id not in {None, attempt.provider_request_id}:
        raise Conflict("billable provider request identity changed")
    job.provider_request_id = attempt.provider_request_id
elif job.provider_request_id is None and result.usage is not None:
    raise Conflict("terminal usage has no durable billable request identity")
```

Thus a Seedance poll correlation is preserved in its non-billable attempt journal but never overwrites the chargeable submit identity used by `GenerationJob` and success/failure `ProviderCostInput`; the supplier manifest ignores the poll for cost-set equality. Seedream and standard TTS each have one chargeable synchronous request, so their job, attempt, and cost identities are the same canonical supplier request ID. If any current response ID is absent or noncanonical, `InvalidProviderRequestIdentity` occurs after the durable send marker, returns RETRY with no response/job/usage/cost write, and subsequent unknown-send reclaims make zero new calls until `RECONCILIATION_REQUIRED`. A stale hook return or stale Worker can therefore write neither response evidence nor job/cost state. The Task 17 pre-call-expiry test exercises this exact PoC boundary in addition to Task 15's submit and poll cases.

Seedream and Seedance continue sending the stable value as `X-Client-Request-Id`. Change standard TTS to send the same stable UUID string as `X-Api-Request-Id` instead of calling `uuid4()`. These headers are correlation attempts only: they do not qualify as official idempotency or query guarantees. Each real adapter stays disabled until the release evidence proves an official guarantee or the supplier-statement path below.

- [ ] **Step 7: Implement supplier-first final-statement reconciliation into the existing billing ledger**

Append these strict export types and the reconciler to `backend/src/ip_saas/modules/media/provider_reconciliation.py`:

```python
from datetime import datetime
from decimal import Decimal
from typing import Annotated, Literal

from pydantic import (
    AfterValidator,
    BaseModel,
    ConfigDict,
    Field,
    StringConstraints,
    model_validator,
)

from ip_saas.modules.billing.models import ProviderCostEntry, ReconciliationStatus
from ip_saas.modules.billing.service import (
    BillingMode,
    ProviderCostInput,
)
from ip_saas.modules.media.models.provider_ops import (
    ProviderSupplierReconciliation,
)
from ip_saas.modules.tasks.reconciliation import (
    NoProviderCallEvidence,
    NoProviderCallEvidencePort,
    ProviderCostManifestPort,
    ProviderRequestIdentity,
    TaskReconciliationService,
)


class StrictSupplierModel(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)


def reject_provider_request_sentinel(value: str) -> str:
    if value.casefold() in {"none", "missing"}:
        raise ValueError("provider request id sentinel is invalid")
    return value


ProviderRequestId = Annotated[
    str,
    StringConstraints(strip_whitespace=True, min_length=1, max_length=200),
    AfterValidator(reject_provider_request_sentinel),
]


class SupplierRequestLogLine(StrictSupplierModel):
    provider_request_id: ProviderRequestId
    client_attempt_id: UUID
    provider: Literal["volcengine"]
    adapter: str = Field(min_length=1, max_length=40)
    model_id: str = Field(min_length=1, max_length=200)
    model_version: str = Field(min_length=1, max_length=100)
    billable: bool


class SupplierRequestLogExport(StrictSupplierModel):
    schema_version: Literal[1] = 1
    run_id: UUID
    account_fingerprint: str = Field(pattern=r"^[0-9a-f]{64}$")
    complete: Literal[True]
    exported_at: datetime
    lines: tuple[SupplierRequestLogLine, ...]


class SupplierStatementLine(StrictSupplierModel):
    statement_line_id: str = Field(min_length=1, max_length=300)
    provider_request_id: ProviderRequestId
    native_quantity: Decimal = Field(gt=0)
    native_unit: str = Field(min_length=1, max_length=40)
    supplier_amount_minor: int = Field(gt=0)
    supplier_currency: Literal["CNY"]
    amount_fen: int = Field(gt=0)


class FinalSupplierStatement(StrictSupplierModel):
    schema_version: Literal[1] = 1
    run_id: UUID
    account_fingerprint: str = Field(pattern=r"^[0-9a-f]{64}$")
    final: Literal[True]
    finalized_at: datetime
    lines: tuple[SupplierStatementLine, ...] = Field(min_length=1)


class MediaNoProviderCallEvidence(NoProviderCallEvidencePort):
    def require_no_provider_call(
        self,
        session: Session,
        task_id: UUID,
        through_attempt_no: int,
    ) -> NoProviderCallEvidence:
        rows = tuple(session.scalars(select(ProviderRequestAttempt).where(
            ProviderRequestAttempt.task_id == task_id,
            ProviderRequestAttempt.worker_attempt_no <= through_attempt_no,
        ).order_by(ProviderRequestAttempt.client_attempt_id)))
        if not rows or any(row.send_started_at is not None for row in rows):
            raise Conflict("media attempt journal cannot prove zero provider calls")
        encoded = dumps([
            {
                "attempt_id": str(row.id),
                "client_attempt_id": str(row.client_attempt_id),
                "worker_attempt_no": row.worker_attempt_no,
                "request_fingerprint": row.request_fingerprint,
                "request_payload_sha256": row.request_payload_sha256,
                "send_started_at": None,
            }
            for row in rows
        ], sort_keys=True, separators=(",", ":")).encode("utf-8")
        return NoProviderCallEvidence(
            task_id=task_id,
            through_attempt_no=through_attempt_no,
            evidence_ref=f"media-provider-attempts:{task_id}:{through_attempt_no}",
            evidence_sha256=sha256(encoded).hexdigest(),
        )


class MediaProviderCostManifest(ProviderCostManifestPort):
    def require_complete_provider_requests(
        self,
        session: Session,
        task_id: UUID,
        through_attempt_no: int,
    ) -> tuple[ProviderRequestIdentity, ...]:
        all_rows = tuple(session.scalars(select(ProviderRequestAttempt).where(
            ProviderRequestAttempt.task_id == task_id,
            ProviderRequestAttempt.worker_attempt_no <= through_attempt_no,
        ).order_by(
            ProviderRequestAttempt.provider,
            ProviderRequestAttempt.client_attempt_id,
        )))
        sent_rows = tuple(
            row for row in all_rows
            if row.send_started_at is not None and row.expects_supplier_charge
        )
        if not sent_rows:
            raise Conflict("provider-cost finalization has no sent media request")
        if any(
            row.status not in {"supplier_matched", "reconciled"}
            or row.provider_request_id is None
            or row.supplier_request_log_run_id is None
            or row.supplier_request_log_sha256 is None
            for row in sent_rows
        ):
            raise Conflict("complete supplier-backed media request manifest is missing")
        rows = tuple(sorted(
            sent_rows,
            key=lambda row: (
                row.provider,
                row.provider_request_id or "",
            ),
        ))
        identities = tuple(
            ProviderRequestIdentity(row.provider, row.provider_request_id or "")
            for row in rows
        )
        if len(set(identities)) != len(identities):
            raise Conflict("media provider request manifest is duplicated")
        return identities


class SupplierReconciliationService:
    def __init__(
        self,
        task_reconciliation: TaskReconciliationService,
        now: Callable[[], datetime],
    ) -> None:
        self.task_reconciliation = task_reconciliation
        self.now = now

    @staticmethod
    def _fingerprint(
        *,
        request_log: SupplierRequestLogExport,
        statement: FinalSupplierStatement,
        provider_request_log_sha256: str,
        supplier_statement_sha256: str,
        task_attempts: Mapping[UUID, int],
    ) -> str:
        payload = {
            "run_id": str(request_log.run_id),
            "account_fingerprint": request_log.account_fingerprint,
            "provider_request_log_sha256": provider_request_log_sha256,
            "supplier_statement_sha256": supplier_statement_sha256,
            "request_lines": sorted(
                (
                    line.model_dump(mode="json")
                    for line in request_log.lines if line.billable
                ),
                key=lambda item: str(item["provider_request_id"]),
            ),
            "statement_lines": sorted(
                (line.model_dump(mode="json") for line in statement.lines),
                key=lambda item: str(item["provider_request_id"]),
            ),
            "task_attempts": {
                str(task_id): attempt_no
                for task_id, attempt_no in sorted(
                    task_attempts.items(), key=lambda item: str(item[0])
                )
            },
        }
        return sha256(dumps(
            payload,
            default=str,
            ensure_ascii=False,
            allow_nan=False,
            sort_keys=True,
            separators=(",", ":"),
        ).encode("utf-8")).hexdigest()

    def reconcile(
        self,
        session: Session,
        *,
        request_log: SupplierRequestLogExport,
        statement: FinalSupplierStatement,
        provider_request_log_sha256: str,
        supplier_statement_sha256: str,
        idempotency_key: str,
    ) -> list[ProviderSupplierReconciliation]:
        if request_log.run_id != statement.run_id:
            raise Conflict("supplier exports refer to different PoC runs")
        if request_log.account_fingerprint != statement.account_fingerprint:
            raise Conflict("supplier exports refer to different accounts")
        log_lines = tuple(line for line in request_log.lines if line.billable)
        log_ids = tuple(line.provider_request_id for line in log_lines)
        statement_ids = tuple(line.provider_request_id for line in statement.lines)
        if len(set(log_ids)) != len(log_ids) or len(set(statement_ids)) != len(
            statement_ids
        ):
            raise Conflict("supplier request identity is duplicated")
        if set(log_ids) != set(statement_ids):
            raise Conflict("request-log and final-statement sets differ")
        log_by_request = {line.provider_request_id: line for line in log_lines}
        statement_by_request = {
            line.provider_request_id: line for line in statement.lines
        }
        attempts_by_request: dict[str, ProviderRequestAttempt] = {}
        for provider_request_id in sorted(statement_by_request):
            request_line = log_by_request[provider_request_id]
            attempt = session.scalar(select(ProviderRequestAttempt).where(
                ProviderRequestAttempt.provider == request_line.provider,
                ProviderRequestAttempt.client_attempt_id
                == request_line.client_attempt_id,
            ).with_for_update())
            if attempt is None:
                raise Conflict("supplier request has no durable client attempt")
            if (
                attempt.adapter != request_line.adapter
                or attempt.model_id != request_line.model_id
                or attempt.model_version != request_line.model_version
                or not attempt.expects_supplier_charge
                or not request_line.billable
            ):
                raise Conflict("supplier request does not match the frozen binding")
            if attempt.send_started_at is None:
                raise Conflict("billable supplier line has no durable send marker")
            if attempt.provider_request_id not in {None, provider_request_id}:
                raise Conflict("attempt has another provider request identity")
            attempts_by_request[provider_request_id] = attempt

        tasks_by_id: dict[UUID, TaskRecord] = {}
        for task_id in sorted(
            {attempt.task_id for attempt in attempts_by_request.values()}, key=str
        ):
            task = session.scalar(select(TaskRecord).where(
                TaskRecord.id == task_id
            ).with_for_update())
            if task is None:
                raise Conflict("supplier attempt has no TaskRecord")
            task_attempts = tuple(
                attempt.worker_attempt_no
                for attempt in attempts_by_request.values()
                if attempt.task_id == task_id
            )
            if any(number > task.attempt_no for number in task_attempts):
                raise Conflict(
                    "supplier evidence is newer than the exhausted task attempt"
                )
            tasks_by_id[task_id] = task

        durable_task_attempts = {
            task_id: task.attempt_no for task_id, task in tasks_by_id.items()
        }
        fingerprint = self._fingerprint(
            request_log=request_log,
            statement=statement,
            provider_request_log_sha256=provider_request_log_sha256,
            supplier_statement_sha256=supplier_statement_sha256,
            task_attempts=durable_task_attempts,
        )
        existing = tuple(session.scalars(
            select(ProviderSupplierReconciliation).where(
                ProviderSupplierReconciliation.provider_request_log_sha256
                == provider_request_log_sha256,
                ProviderSupplierReconciliation.supplier_statement_sha256
                == supplier_statement_sha256,
            ).order_by(
                ProviderSupplierReconciliation.provider,
                ProviderSupplierReconciliation.provider_request_id,
            )
        ))
        if existing:
            if (
                len(existing) != len(statement_ids)
                or {row.provider_request_id for row in existing} != set(statement_ids)
                or {row.task_id for row in existing} != set(tasks_by_id)
                or any(row.idempotency_key != idempotency_key for row in existing)
                or any(row.input_fingerprint != fingerprint for row in existing)
            ):
                raise Conflict("supplier reconciliation replay changed input")
            return list(existing)

        if any(
            task.status != TaskStatus.RECONCILIATION_REQUIRED
            for task in tasks_by_id.values()
        ):
            raise Conflict("task is not waiting for supplier reconciliation")

        costs_by_task: dict[UUID, list[ProviderCostInput]] = {}
        for provider_request_id in sorted(statement_by_request):
            billed = statement_by_request[provider_request_id]
            attempt = attempts_by_request[provider_request_id]
            attempt.provider_request_id = provider_request_id
            attempt.supplier_request_log_run_id = request_log.run_id
            attempt.supplier_request_log_sha256 = provider_request_log_sha256
            attempt.status = "supplier_matched"
            costs_by_task.setdefault(attempt.task_id, []).append(
                ProviderCostInput(
                    provider=attempt.provider,
                    provider_request_id=provider_request_id,
                    capability=attempt.capability,
                    model_id=attempt.model_id,
                    model_version=attempt.model_version,
                    native_quantity=billed.native_quantity,
                    native_unit=billed.native_unit,
                    supplier_amount_minor=billed.supplier_amount_minor,
                    supplier_currency=billed.supplier_currency,
                    amount_fen=billed.amount_fen,
                    reconciliation_status=ReconciliationStatus.MATCHED,
                    task_id=attempt.task_id,
                )
            )
        session.flush()

        for task_id in sorted(costs_by_task, key=str):
            task = tasks_by_id[task_id]
            task_costs = tuple(costs_by_task[task_id])
            actual_amount = (
                0
                if BillingMode(task.billing_mode) is BillingMode.CUSTOMER_CREDIT
                else sum(cost.amount_fen for cost in task_costs)
            )
            self.task_reconciliation.finalize_provider_costs(
                session,
                task_id=task.id,
                attempt_no=task.attempt_no,
                actual_amount=actual_amount,
                provider_costs=task_costs,
                reason="provider response was lost and supplier cost was reconciled",
                idempotency_key=f"{idempotency_key}:{task.id}",
            )

        links: list[ProviderSupplierReconciliation] = []
        for provider_request_id in sorted(statement_by_request):
            billed = statement_by_request[provider_request_id]
            attempt = attempts_by_request[provider_request_id]
            cost = session.scalar(select(ProviderCostEntry).where(
                ProviderCostEntry.provider == attempt.provider,
                ProviderCostEntry.provider_request_id == provider_request_id,
            ))
            if cost is None or cost.task_id != attempt.task_id:
                raise Conflict("supplier cost was not linked to the task")
            link = ProviderSupplierReconciliation(
                provider_request_attempt_id=attempt.id,
                task_id=attempt.task_id,
                provider=attempt.provider,
                provider_request_id=provider_request_id,
                supplier_statement_line_id=billed.statement_line_id,
                provider_request_log_sha256=provider_request_log_sha256,
                supplier_statement_sha256=supplier_statement_sha256,
                provider_cost_entry_id=cost.id,
                supplier_amount_minor=billed.supplier_amount_minor,
                supplier_currency=billed.supplier_currency,
                amount_fen=billed.amount_fen,
                idempotency_key=idempotency_key,
                input_fingerprint=fingerprint,
                statement_finalized_at=statement.finalized_at,
                reconciled_at=self.now(),
            )
            session.add(link)
            attempt.status = "reconciled"
            links.append(link)
        session.flush()
        return links
```

Add the exact imports for `Callable`, `Mapping`, `UUID`, `Session`, `select`, `Conflict`, `TaskStatus`, and `TaskRecord`. Construct the one production `TaskReconciliationService` with `MediaNoProviderCallEvidence` and `MediaProviderCostManifest`; neither port has a permissive default. Reconciliation starts from every billed line in the supplier's finalized statement, joins it to the complete supplier request log and durable attempts, groups all lines by TaskRecord, and derives the only valid `attempt_no` from each locked TaskRecord plus its persisted journal rows. The public reconciler accepts no caller-provided attempt map. It rejects journal evidence whose Worker attempt is newer than the exhausted task, then calls Plan 01's batch finalizer once per task with the derived attempt. The central finalizer alone writes every `ReconciliationStatus.MATCHED` cost, releases customer credit at zero actual or settles the sum of internal CNY, and moves `RECONCILIATION_REQUIRED` to FAILED. Only then are append-only supplier links written in the same transaction. Missing/extra lines roll back the whole task batch; line order is canonicalized; exact replay returns the same sorted links; changed evidence, cost, key, or persisted attempt conflicts. It never calls `BillingService.settle_generation` or `TaskSubmissionService.fail` in a per-line loop and never starts with a database anti-join.

- [ ] **Step 8: Run the zero-paid crash and supplier reconciliation suite**

Run:

```bash
cd backend
env -u ARK_API_KEY -u DOUBAO_TTS_APP_ID -u DOUBAO_TTS_ACCESS_KEY \
  -u TOS_ACCESS_KEY -u TOS_SECRET_KEY \
  uv run pytest \
  tests/unit/media/test_provider_release.py \
  tests/unit/media/test_provider_attempt_models.py \
  tests/integration/media/test_provider_attempt_reconciliation.py \
  tests/integration/media/test_provider_worker_idempotency.py \
  tests/integration/media/test_media_lease_recovery.py -q
```

Expected: all tests pass with deterministic fakes and zero network. Each adapter has a committed client attempt and send marker before the simulated paid response; the exact crash leaves job and cost state absent; retry exhaustion enters `RECONCILIATION_REQUIRED` with the hold active; supplier-first reconciliation writes the complete one- or two-line task-linked cost batch through one Plan 01 finalizer call; a missing line rolls back everything; reversed-order replay returns the same links; a pre-call lease expiry makes zero provider calls; and stale reconciliation writes nothing.

- [ ] **Step 9: Write failing production-composition and CLI/Make contract tests**

Create `backend/tests/integration/media/test_provider_release_composition.py`:

```python
from dataclasses import dataclass

import pytest

from ip_saas.common.errors import Forbidden
from ip_saas.modules.media.provider_release import ProviderReleaseGate
from ip_saas.providers.media.composition import (
    ActiveMediaBinding,
    ProductionMediaProviderFactory,
)


@dataclass
class SecretSpy:
    ark_calls: int = 0
    speech_calls: int = 0

    def ark(self):
        self.ark_calls += 1
        raise AssertionError("gate must run before Ark secret access")

    def speech(self):
        self.speech_calls += 1
        raise AssertionError("gate must run before Speech secret access")


@pytest.mark.parametrize(
    "adapter,capability",
    [
        ("seedream", "storyboard_image"),
        ("seedance", "low_res_video"),
        ("standard_tts", "standard_tts"),
    ],
)
def test_production_binding_without_evidence_fails_before_secret_or_http(
    adapter: str,
    capability: str,
) -> None:
    secrets = SecretSpy()
    http_calls: list[str] = []
    factory = ProductionMediaProviderFactory(
        gates={adapter: ProviderReleaseGate.disabled()},
        secrets=secrets,
        http_factory=lambda name: http_calls.append(name),
        clock=object(),
        tts_sink=object(),
        reconciler_sha256="0" * 64,
    )
    binding = ActiveMediaBinding(
        adapter=adapter,
        capability=capability,
        model_id=f"pinned-{adapter}",
        model_version="2026-08-24",
        region="cn-beijing",
    )
    with pytest.raises(Forbidden, match="release evidence is not configured"):
        factory.build(binding)
    assert secrets.ark_calls == 0
    assert secrets.speech_calls == 0
    assert http_calls == []
```

Create `backend/tests/contract/media/test_provider_poc_cli.py`:

```python
from pathlib import Path

from scripts.run_media_provider_poc import build_parser


def test_cli_has_explicit_exercise_and_reconcile_actions() -> None:
    parser = build_parser()
    exercise = parser.parse_args([
        "exercise", "--adapter", "seedream", "--evidence-dir", "/tmp/e",
        "--max-total-fen", "100", "--max-billable-completions", "1",
    ])
    reconcile = parser.parse_args([
        "reconcile", "--adapter", "seedream", "--evidence-dir", "/tmp/e",
        "--max-total-fen", "100", "--max-billable-completions", "1",
    ])
    assert exercise.action == "exercise"
    assert reconcile.action == "reconcile"


def test_makefile_has_three_fixed_budget_media_targets() -> None:
    root = Path(__file__).parents[4]
    text = (root / "Makefile").read_text(encoding="utf-8")
    for target in ("poc-seedream", "poc-seedance", "poc-standard-tts"):
        block = text.split(f"{target}:", 1)[1].split("\n\n", 1)[0]
        assert "--max-total-fen 100" in block
        assert "--max-billable-completions 1" in block
        assert "POC_ACTION" in block
    declared = {
        line[:-1] for line in text.splitlines()
        if line.startswith("poc-") and line.endswith(":")
    }
    assert {"poc-seedream", "poc-seedance", "poc-standard-tts"} <= declared
```

- [ ] **Step 10: Make real provider construction fail closed per exact active binding**

Create `backend/src/ip_saas/providers/media/composition.py`:

```python
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import Protocol

import httpx

from ip_saas.common.clock import Clock
from ip_saas.common.errors import Forbidden
from ip_saas.modules.media.provider_release import (
    MediaAdapterKind,
    ProviderReleaseGate,
)
from ip_saas.providers.media.base import MediaProvider
from ip_saas.providers.media.temporary import TemporaryArtifactSink
from ip_saas.providers.volcengine.auth import ArkCredentials, SpeechCredentials
from ip_saas.providers.volcengine.seedance import SeedanceAdapter
from ip_saas.providers.volcengine.seedream import SeedreamAdapter
from ip_saas.providers.volcengine.tts import StandardTtsAdapter


@dataclass(frozen=True)
class ActiveMediaBinding:
    adapter: str
    capability: str
    model_id: str
    model_version: str
    region: str


class MediaSecretLoader(Protocol):
    def ark(self) -> ArkCredentials:
        raise NotImplementedError

    def speech(self) -> SpeechCredentials:
        raise NotImplementedError


class ProductionMediaProviderFactory:
    def __init__(
        self,
        *,
        gates: Mapping[str, ProviderReleaseGate],
        secrets: MediaSecretLoader,
        http_factory: Callable[[str], httpx.Client],
        clock: Clock,
        tts_sink: TemporaryArtifactSink,
        reconciler_sha256: str,
    ) -> None:
        self.gates = dict(gates)
        self.secrets = secrets
        self.http_factory = http_factory
        self.clock = clock
        self.tts_sink = tts_sink
        self.reconciler_sha256 = reconciler_sha256

    def build(self, binding: ActiveMediaBinding) -> MediaProvider:
        try:
            adapter = MediaAdapterKind(binding.adapter)
            gate = self.gates[binding.adapter]
        except (ValueError, KeyError) as error:
            raise Forbidden("media adapter has no release gate") from error
        gate.require(
            adapter=adapter,
            capability=binding.capability,
            model_id=binding.model_id,
            model_version=binding.model_version,
            region=binding.region,
            reconciler_sha256=self.reconciler_sha256,
        )
        if adapter is MediaAdapterKind.SEEDREAM:
            return SeedreamAdapter(
                self.http_factory("seedream"), self.secrets.ark(), self.clock
            )
        if adapter is MediaAdapterKind.SEEDANCE:
            return SeedanceAdapter(
                self.http_factory("seedance"), self.secrets.ark(), self.clock
            )
        return StandardTtsAdapter(
            self.http_factory("standard_tts"),
            self.secrets.speech(),
            self.tts_sink,
        )


def load_release_gate(
    path: Path | None,
    expected_sha256: str,
) -> ProviderReleaseGate:
    if path is None and not expected_sha256:
        return ProviderReleaseGate.disabled()
    if path is None or not expected_sha256:
        raise Forbidden("media release evidence path and digest must be paired")
    return ProviderReleaseGate.load(path, expected_sha256)
```

Add these production-default-empty settings to `backend/src/ip_saas/config.py`:

```python
class Settings(BaseSettings):
    media_seedream_release_evidence_path: Path | None = None
    media_seedream_release_evidence_sha256: str = ""
    media_seedance_release_evidence_path: Path | None = None
    media_seedance_release_evidence_sha256: str = ""
    media_standard_tts_release_evidence_path: Path | None = None
    media_standard_tts_release_evidence_sha256: str = ""
```

Import `Path` from `pathlib`. Worker composition computes `sha256(Path(provider_reconciliation.__file__).read_bytes()).hexdigest()` from the deployed module and passes it as `reconciler_sha256`; a statement-based evidence file cannot survive an unreviewed reconciler change. Composition calls `load_release_gate()` for all three adapters, then creates `ProductionMediaProviderFactory`; the factory sees only the full `model_id`, `model_version`, capability, and region read from the active Plan 01 registry row. It invokes `gate.require()` before reading credentials, constructing `httpx.Client`, creating TOS staging, or sending HTTP. Missing evidence, one-sided path/digest configuration, digest mismatch, parser failure, wrong adapter/capability/model/version/region, a non-final statement, an incorrect crash checkpoint, absent recovery proof, or reconciler hash mismatch leaves only that adapter disabled. A passing Seedream document never enables Seedance or standard TTS.

The deterministic `ScriptedFakeMediaProvider` composition remains a separate test-only function and never accepts or manufactures production release evidence. Production uses `UnconfiguredGenerationLimits` until Plan 05 provides real effective limits; the release gate does not bypass the mandatory billing limit port.

Run:

```bash
cd backend
env -u ARK_API_KEY -u DOUBAO_TTS_APP_ID -u DOUBAO_TTS_ACCESS_KEY \
  uv run pytest \
  tests/unit/media/test_provider_release.py \
  tests/integration/media/test_provider_release_composition.py -q
```

Expected: tests pass without secrets or network; all three missing-evidence bindings fail before secret and HTTP factories are touched.

- [ ] **Step 11: Implement the SIGKILL runner, strict supplier inputs, and evidence exporter**

In `backend/src/ip_saas/modules/media/provider_poc.py`, add the production-only fault injector utilities:

```python
from hashlib import sha256
import json
import os
from pathlib import Path
import signal
from uuid import UUID

from ip_saas.modules.media.provider_release import CrashPoint


def append_durable(path: Path, payload: dict[str, object]) -> None:
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    body = (json.dumps(payload, sort_keys=True) + "\n").encode("utf-8")
    descriptor = os.open(
        path,
        os.O_APPEND | os.O_CREAT | os.O_WRONLY | getattr(os, "O_DSYNC", 0),
        0o600,
    )
    try:
        os.write(descriptor, body)
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


class SigkillFaultInjector:
    def __init__(
        self,
        fault_point: CrashPoint,
        transcript_path: Path,
    ) -> None:
        self.fault_point = fault_point
        self.transcript_path = transcript_path

    def before_http(self, snapshot: object) -> None:
        if self.fault_point is CrashPoint.BEFORE_HTTP:
            append_durable(self.transcript_path, {
                "fault_point": self.fault_point.value,
                "task_id": str(snapshot.task_record_id),
                "client_attempt_id": str(snapshot.client_attempt_id),
                "http_send_observed": False,
                "paid_response_observed": False,
                "job_result_persisted_before_kill": False,
                "cost_persisted_before_kill": False,
            })
            os.kill(os.getpid(), signal.SIGKILL)

    def after_paid_response_before_persistence(
        self, snapshot: object, result: ProviderResult,
    ) -> None:
        if (
            self.fault_point
            is CrashPoint.AFTER_PAID_RESPONSE_BEFORE_PERSISTENCE
        ):
            append_durable(self.transcript_path, {
                "fault_point": self.fault_point.value,
                "task_id": str(snapshot.task_record_id),
                "client_attempt_id": str(snapshot.client_attempt_id),
                "http_send_observed": True,
                "paid_response_observed": True,
                "job_result_persisted_before_kill": False,
                "cost_persisted_before_kill": False,
                "response_id": result.provider_request_id,
                "response_sha256": canonical_sha256({
                    "phase": result.phase.value,
                    "provider_task_id": result.provider_task_id,
                    "provider_request_id": result.provider_request_id,
                    "model_id": result.model_id,
                    "model_version": result.model_version,
                }),
            })
            os.kill(os.getpid(), signal.SIGKILL)
```

Add exact imports for `ProviderResult` and `canonical_sha256`. `append_durable` stores no request body, prompt, signed URL, credential, or response body. The transcript only proves the checkpoint and correlation IDs; the supplier request log and statement remain the billing truth.

Implement `MediaProviderPocRunner` in the same file. Its `exercise()` method creates two isolated internal-cost TaskRecords through `TaskSubmissionService.submit_internal`, one for each `CrashPoint`; uses the real pinned production adapter and production Worker; commits the `ProviderRequestAttempt`; forks one child per cell; injects `SigkillFaultInjector`; and requires the parent to observe `SIGKILL`. `LeaseHeartbeat.__enter__` must complete its synchronous heartbeat CAS before either fault hook. The exact post-response cell must show a committed send marker but no `GenerationJob.provider_task_id`, terminal usage fields, or `ProviderCostEntry` after child death. The parent then invokes only zero-new-provider-call Worker reclaims until Plan 01 returns `RECONCILIATION_REQUIRED` for each task and verifies both original holds remain active; it never calls `fail_owned` before `start`. It stops before HTTP unless all of these are true:

1. `IP_SAAS_MEDIA_POC_ACK=PRODUCTION_LIKE_ACCOUNT_MAX_100_FEN`;
2. the adapter-specific credential and `DATABASE_URL` are present;
3. the production account fingerprint matches the signed budget-cap receipt;
4. the provider-side cap is enabled and at most 100 fen;
5. the server-owned internal task limit and hold are at most 100 fen;
6. the frozen price upper bound fits the remaining cap; and
7. the run has made fewer than one billable completion.

Its `reconcile()` method reads the raw supplier request-log export and finalized supplier statement from the capability evidence directory, verifies strict schemas and SHA-256 values, starts from the statement IDs, and calls `SupplierReconciliationService`, which groups all billed lines by task and delegates each group once to Plan 01 `TaskReconciliationService.finalize_provider_costs`. It separately calls `finalize_no_provider_call` for the before-HTTP control using the immutable attempt journal. It writes a reconciliation report and builds `MediaProviderReleaseEvidence` only after both tasks are FAILED and both holds are terminal. It exits without a passing document when the official contract does not prove query/deduplication and the deployed statement reconciler cannot map every billed request to exactly one durable attempt and task-linked cost. It writes the canonical evidence JSON with mode `0600` and a sibling `.sha256`; it never changes the model registry or deployment settings.

Create `backend/scripts/run_media_provider_poc.py`:

```python
from __future__ import annotations

import argparse
from pathlib import Path
from typing import Sequence

from ip_saas.providers.media.composition import compose_media_poc_runner


ADAPTERS = ("seedream", "seedance", "standard_tts")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="budgeted media provider crash PoC")
    subparsers = parser.add_subparsers(dest="action", required=True)
    for action in ("exercise", "reconcile"):
        command = subparsers.add_parser(action)
        command.add_argument("--adapter", choices=ADAPTERS, required=True)
        command.add_argument("--evidence-dir", type=Path, required=True)
        command.add_argument("--max-total-fen", type=int, required=True)
        command.add_argument("--max-billable-completions", type=int, required=True)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    if args.max_total_fen != 100 or args.max_billable_completions != 1:
        raise SystemExit("live media PoC requires the fixed 100-fen/one-completion cap")
    runner = compose_media_poc_runner(
        adapter=args.adapter,
        evidence_dir=args.evidence_dir,
        max_total_fen=args.max_total_fen,
        max_billable_completions=args.max_billable_completions,
    )
    if args.action == "exercise":
        runner.exercise()
    else:
        runner.reconcile()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

Add `compose_media_poc_runner()` to `backend/src/ip_saas/providers/media/composition.py`. It uses the production secret loader, private TOS staging, real SQL billing adapters, mandatory generation-limit service, active pinned model binding, `ProviderAttemptJournal`, and exact production Worker. It refuses a C-user account, a customer-credit billing route, a floating model alias, an unconfigured limit, a non-private evidence directory, or an already-used run ID. The normal production registry remains release-gated; only this explicit PoC composition may construct a real adapter before release evidence exists.

Create `docs/runbooks/media-provider-crash-poc.md` with the exact account owner, region, activated resource/endpoint, model ID/version, standard voice ID, quota, budget-cap receipt export, request-log export, final-statement export, retention location, reviewer role, exit codes, and rollback steps. Raw evidence is encrypted under restricted audit retention; only the strict reviewed JSON and its digest are mounted into the Worker.

- [ ] **Step 12: Add three separate hard-budget Make targets**

Append to `Makefile`:

```make
.PHONY: poc-seedream poc-seedance poc-standard-tts

POC_EVIDENCE_ROOT ?= .artifacts/media-provider-poc

poc-seedream:
	@cd backend && uv run python scripts/run_media_provider_poc.py "$${POC_ACTION:?set POC_ACTION=exercise or reconcile}" --adapter seedream --evidence-dir "../$(POC_EVIDENCE_ROOT)/seedream" --max-total-fen 100 --max-billable-completions 1

poc-seedance:
	@cd backend && uv run python scripts/run_media_provider_poc.py "$${POC_ACTION:?set POC_ACTION=exercise or reconcile}" --adapter seedance --evidence-dir "../$(POC_EVIDENCE_ROOT)/seedance" --max-total-fen 100 --max-billable-completions 1

poc-standard-tts:
	@cd backend && uv run python scripts/run_media_provider_poc.py "$${POC_ACTION:?set POC_ACTION=exercise or reconcile}" --adapter standard_tts --evidence-dir "../$(POC_EVIDENCE_ROOT)/standard_tts" --max-total-fen 100 --max-billable-completions 1
```

Do not add these targets as dependencies of `test`, `ci`, `lint`, `all`, or `media-check`. The fixed numeric cap and completion count are inside each recipe and cannot be widened through a Make variable.

Run the non-paid contract only:

```bash
cd backend
env -u ARK_API_KEY -u DOUBAO_TTS_APP_ID -u DOUBAO_TTS_ACCESS_KEY \
  uv run pytest \
  tests/contract/media/test_provider_poc_cli.py \
  tests/integration/media/test_provider_release_composition.py -q
cd ..
POC_ACTION=exercise make -n poc-seedream poc-seedance poc-standard-tts
```

Expected: tests pass without HTTP; dry-run output contains three different adapter values, `--max-total-fen 100`, and `--max-billable-completions 1`, but executes no recipe.

- [ ] **Step 13: Execute and reconcile each separately approved live PoC**

First run the entire no-credential suite from Final Verification. Then an authorized operator configures a dedicated production-like account, provider-side hard cap, internal cost center/limit, exact pinned resource, and restricted evidence directory. Exercise one adapter at a time:

```bash
export IP_SAAS_MEDIA_POC_ACK=PRODUCTION_LIKE_ACCOUNT_MAX_100_FEN
export POC_EVIDENCE_ROOT=.artifacts/media-provider-poc
POC_ACTION=exercise make poc-seedream
POC_ACTION=exercise make poc-seedance
POC_ACTION=exercise make poc-standard-tts
```

Expected for each target: the before-HTTP child and exact post-response child both die by `SIGKILL`; the parent survives; the exact cell has one durable client attempt but no provider task/result/cost row; no target exceeds one billable completion or 100 fen. A budget or account precondition exits before HTTP.

After the official provider request log and finalized supplier billing statement are exported into each target's restricted evidence directory, reconcile one adapter at a time:

```bash
POC_ACTION=reconcile make poc-seedream
POC_ACTION=reconcile make poc-seedance
POC_ACTION=reconcile make poc-standard-tts
sha256sum .artifacts/media-provider-poc/*/media-provider-release-evidence.json
```

Expected: every billed supplier statement ID first maps to the official request log, then to one durable client attempt, then to one `TaskRecord`, then to exactly one `ProviderCostEntry.task_id` with equal fen. Exit 75 means the statement is not final. Exit 78 means the official lookup/idempotency proof is insufficient and the implemented statement reconciliation did not close every supplier line. Either exit keeps that adapter disabled. A database-only report, synthetic export, missing supplier ID, duplicate billed request, mismatched model/version/account, or digest mismatch cannot emit `passed=true`.

For each passing target, a different authorized reviewer verifies the raw bundle, request identity contract, model/resource activation, URL lifetime, response schema, observed amount, task-cost mapping, and reconciler source hash before signing the strict evidence. Deployment mounts only that reviewed file and exact SHA-256. It does not copy test manifests or automatically activate a registry entry.

- [ ] **Step 14: Commit the durable attempt, supplier reconciliation, and release gate**

```bash
git add Makefile backend/migrations/versions/0003_media.py backend/src/ip_saas/config.py backend/src/ip_saas/modules/media/models/__init__.py backend/src/ip_saas/modules/media/models/provider_ops.py backend/src/ip_saas/modules/media/provider_poc.py backend/src/ip_saas/modules/media/provider_reconciliation.py backend/src/ip_saas/modules/media/provider_release.py backend/src/ip_saas/providers/media/composition.py backend/src/ip_saas/providers/volcengine/seedream.py backend/src/ip_saas/providers/volcengine/seedance.py backend/src/ip_saas/providers/volcengine/tts.py backend/src/ip_saas/workers/media_submit.py backend/src/ip_saas/workers/media_poll.py backend/scripts/run_media_provider_poc.py backend/tests/unit/media/test_provider_release.py backend/tests/unit/media/test_provider_attempt_models.py backend/tests/integration/media/harness.py backend/tests/integration/media/test_provider_attempt_reconciliation.py backend/tests/integration/media/test_provider_release_composition.py backend/tests/contract/media/test_provider_poc_cli.py backend/tests/integration/media/test_media_migration.py docs/runbooks/media-provider-crash-poc.md
git commit -m "feat: gate media providers on supplier crash reconciliation"
```

## Final verification and execution handoff

Run Tasks 1–17 in order with `superpowers:subagent-driven-development` or `superpowers:executing-plans`. A task is complete only after its stated failing test, minimal implementation, passing test, and commit have all occurred. Do not combine the provider HTTP turn with the request transaction, do not introduce `AsyncSession`, and do not initialize a real adapter in the default test container.

- [ ] Run: `rg -n "AsyncSession|generation_hold_id|internal_budget_hold_id" backend/src/ip_saas/modules/media backend/src/ip_saas/workers`

  Expected: no matches. Billing ownership exists only on `TaskRecord.billing_mode` and `TaskRecord.billing_hold_id`.

- [ ] Run: `cd backend && env -u ARK_API_KEY -u DOUBAO_TTS_APP_ID -u DOUBAO_TTS_ACCESS_KEY -u TOS_ACCESS_KEY -u TOS_SECRET_KEY uv run pytest tests/unit/media tests/contract/media tests/integration/media tests/security/test_media_tenant_isolation.py -v`

  Expected: all tests pass, no paid endpoint is contacted, and every real-adapter contract request is intercepted by `httpx.MockTransport`.

- [ ] Run: `cd backend && uv run alembic downgrade 0002_intelligence && uv run alembic upgrade 0003_media && uv run alembic check`

  Expected: upgrade succeeds, both provider-operations evidence tables exist, downgrade removes them before `generation_jobs`, and Alembic reports `No new upgrade operations detected.`

- [ ] Run: `cd backend && env -u ARK_API_KEY -u DOUBAO_TTS_APP_ID -u DOUBAO_TTS_ACCESS_KEY uv run pytest tests/unit/media/test_provider_release.py tests/unit/media/test_provider_attempt_models.py tests/integration/media/test_provider_attempt_reconciliation.py tests/integration/media/test_provider_release_composition.py tests/contract/media/test_provider_poc_cli.py -v`

  Expected: all Task 17 tests pass with no credential or network; the exact crash leaves only precommitted attempt/send evidence; Seedance retains its canonical billable submit request ID on GenerationJob/cost while its distinct non-billable poll request ID remains journal-only and its provider task ID remains polling-only; supplier-first reconciliation writes the complete one- or two-line task-linked cost set through one central finalizer/hold mutation per task; joint omission and stale journal evidence are rejected; reversed input replays identically; the proven no-send path releases without a cost; and all three production adapters fail before secret or HTTP construction without matching evidence.

- [ ] Run: `POC_ACTION=exercise make -n poc-seedream poc-seedance poc-standard-tts`

  Expected: only commands are printed; all three show their distinct adapter plus the fixed `--max-total-fen 100 --max-billable-completions 1`; no paid process starts.

- [ ] Run: `cd backend && uv run pytest tests/integration/media/test_provider_worker_idempotency.py tests/integration/media/test_copy_worker.py tests/integration/media/test_per_shot_redo.py -v --count=5`

  Expected: repeated and out-of-order delivery creates one provider task association, one private asset, one downstream event per stage, and no duplicate TaskRecord transition or charge.

- [ ] Run: `cd backend && uv run pytest tests/integration/media/test_media_terminal_billing.py tests/integration/media/test_media_lease_recovery.py tests/integration/media/test_media_human_review.py -v --count=3`

  Expected: provider-incurred downstream failures write the complete distinct `ProviderCostEntry(task_id=...)` set—one row per supplier request—settle customer/internal actual amount to zero, release the hold, and then fail; active/same-expired/reconciliation attempts RETRY, expired scanner claims reclaim, only terminal or strictly newer attempts ACK, and stale attempts create no domain, ledger, outbox, or delivery write.

- [ ] Run: `cd backend && uv run pytest tests/integration/media/test_media_finishing.py -v`

  Expected: the final file decodes, uses approved codecs/resolution/loudness, contains captions, retains explicit AI labeling plus `ai_generated` metadata, and is ready for a new QC pass after the transcode.

- [ ] Run: `cd frontend && pnpm test -- --run src/features/media/media-studio.test.tsx && pnpm exec playwright test tests/e2e/media-studio.spec.ts`

  Expected: the studio supports production-mode display, character/standard-voice rights status, storyboard review, selected low-resolution previews, frozen cost approval, per-shot redo, job progress, QC issue display, and authorized private delivery without exposing provider URLs or credentials.

- [ ] Run: `make export-contracts && make lint && make test`

  Expected: the six actually emitted media event schemas, shared generation-task schema, OpenAPI, and sole frontend schema file are current; lint passes; and the full no-credential suite passes.

- [ ] After Plan 05 merges, run: `cd backend && uv run pytest tests/integration/billing tests/integration/tasks tests/unit/media tests/integration/media tests/contract/media tests/integration/resellers -q`

  Expected: every `BillingService` fixture supplies an explicit `GenerationLimitService`; configured account/internal limits allow in-cap media reservations, missing or exceeded limits fail before hold/job/outbox creation, and media terminal settlement/release behavior remains unchanged.

- [ ] Run: `rg -n "apiRequest\(\s*[\"']\/api|generated\.ts|from [\"']\.\/generated|new .*ApiClient" frontend/src/features/media frontend/src/app/'(c-user)'/projects/'[projectId]'/content/'[contentVersionId]'/studio`

  Expected: no matches. Feature code passes only backend `/v1/...` paths to Plan 01's one `apiRequest`; the shared client alone adds browser `/api`.

- [ ] Run:

~~~bash
python3 - <<'PY'
from pathlib import Path
import ast
import re

path = Path('docs/superpowers/plans/2026-08-24-03-generative-media-studio.md')
text = path.read_text()
fence = chr(96) * 3
fence_lines = [line for line in text.splitlines() if line.startswith(fence)]
assert len(fence_lines) % 2 == 0
for index, block in enumerate(
    re.findall(rf'{fence}python\n(.*?){fence}', text, re.S), 1
):
    ast.parse(block, filename=f'plan03-python-block-{index}')
print('all Python fences parse and all Markdown fences are paired')
PY
~~~

  Expected: the script prints the success line and exits 0.

- [ ] Run:

~~~bash
python3 - <<'PY'
from pathlib import Path
import ast
import re

text = Path('docs/superpowers/plans/2026-08-24-03-generative-media-studio.md').read_text()
fence = chr(96) * 3
allowed = {'db_session', 'pg_engine', 'tmp_path', 'monkeypatch', 'request'}
parametrized = set()
custom = set()
fixture_names = set()


def decorator_is_fixture(decorator):
    target = decorator.func if isinstance(decorator, ast.Call) else decorator
    return (
        isinstance(target, ast.Name) and target.id == 'fixture'
    ) or (
        isinstance(target, ast.Attribute) and target.attr == 'fixture'
    )


for block in re.findall(rf'{fence}python\n(.*?){fence}', text, re.S):
    tree = ast.parse(block)
    for node in ast.walk(tree):
        if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            continue
        if any(decorator_is_fixture(item) for item in node.decorator_list):
            fixture_names.add(node.name)
        if node.name.startswith('test_'):
            for decorator in node.decorator_list:
                if (
                    isinstance(decorator, ast.Call)
                    and isinstance(decorator.func, ast.Attribute)
                    and decorator.func.attr == 'parametrize'
                    and decorator.args
                    and isinstance(decorator.args[0], ast.Constant)
                ):
                    parametrized.update(
                        item.strip()
                        for item in str(decorator.args[0].value).split(',')
                    )
            custom.update(argument.arg for argument in node.args.args)
missing = custom - allowed - parametrized - fixture_names
assert not missing, sorted(missing)
print('all custom media test parameters have explicit fixtures; missing=0')
PY
~~~

  Expected: the script prints the fixture-closure success line with `missing=0`. Only decorated fixtures count; ordinary helper and test functions cannot hide a missing fixture. Task 15's named scenarios resolve through `backend/tests/integration/media/conftest.py`; Plan 01 owns `db_session` and `pg_engine`; Pytest owns `tmp_path`, `monkeypatch`, and `request`.

- [ ] Run: `git diff --check && rg -n "T[B]D|T[O]DO|F[I]XME|place[h]older|implement l[a]ter|fill i[n]" docs/superpowers/plans/2026-08-24-03-generative-media-studio.md`

  Expected: `git diff --check` exits 0 and the unfinished-marker scan returns no matches.

The implementation is not accepted until all of the following are demonstrated together:

1. A human package, an AI production, and a mixed production reference the same frozen `ContentVersion`/`PlatformVariant` lineage and use the same Scene/Shot revision model.
2. Every media job references exactly one shared `TaskRecord`; customer work uses its customer-credit hold, platform self-marketing uses its internal CNY hold, and a retry or duplicate message cannot create another hold.
3. Seedream, Seedance, and standard TTS run through pinned model IDs; all ordinary tests use deterministic fakes without paid credentials. Cloned voice, unverified real-person `asset://`, OmniHuman, Chimera, and cloned digital-human routes remain rejected for Plan 06.
4. Seedance polling, callback replay, lost-callback polling, provider timeouts, near-expiry copy priority, and TOS copy retries resume from the latest durable substate.
5. Provider URLs are encrypted while pending, copied into private/versioned TOS with SHA-256 and decodability evidence, cleared after copy, and never returned to the browser.
6. Storyboard and low-resolution approval occur before high-resolution cost; a user redo creates only one new shot revision and a new customer task, while a system-fault redo uses internal recovery budget and does not charge the customer.
7. FFmpeg runs with `shell=False` and controlled argument vectors, captions and AI labels survive finishing, and every transcode/export triggers another disclosure and media-QC check.
8. QC reports retain technical, semantic, rights, product-truth, continuity, face/hand/lip-sync, subtitle, loudness, and disclosure results; localized defects point to logical shot IDs, while global rights or decode failures block delivery.
9. Passing QC on the normal single-call route settles the shared billing context once with its task-linked `ProviderCostInput` before `succeed`. A task with several supplier requests uses Plan 01's batch finalizer with the complete distinct cost tuple and still mutates the hold once. Machine redo/fail and downstream copy/finishing/QC exhaustion after real provider success settle customer actual amount zero with the same complete supplier-cost lineage, release the remaining hold, and only then fail or return a non-deliverable terminal result. A journal-proven zero-send task releases only through Plan 01's scanner and invents no cost. Missing or partial usage keeps the hold active in `RECONCILIATION_REQUIRED`. Supplier request identity, native units, supplier currency, CNY conversion, price version, and reconciliation state remain auditable.
10. Every Worker turn carries the captured `attempt_no`: after lease loss, only terminal tasks or a strictly higher durable attempt ACK; missing tasks, active/same-expired attempts, and same-attempt `reconciliation_required` RETRY. Expired scanner claims reclaim, synchronous pre-call and post-call heartbeats fence provider/copy/finishing/QC work, and a stale Worker cannot write domain state, billing, outbox, task terminal state, or delivery eligibility. Reconciliation-pending handlers perform no provider or downstream work; Plan 01's scanner alone finalizes evidence.
11. The C-user studio shows truthful human/AI/mixed routing, character and standard-voice readiness, storyboard and preview gates, task progress, single-shot redo, all six QC families, and recovery instructions. It issues only authorized five-minute TOS URLs and never offers an AI-label removal or a Plan 06-only identity capability.
12. Backend paths start `/v1`; only Plan 01's shared browser client adds `/api`. The OpenAPI export updates only `frontend/src/lib/api/schema.d.ts`, and each emitted media event has a strict checked-in V1 schema while task entry reuses `GenerationTaskRequestedV1.request_fingerprint`.
13. Every paid HTTP logical call has one server-derived durable `ProviderRequestAttempt` committed before send. `provider_task_id` is only the nullable asynchronous polling locator; every result, GenerationJob terminal identity, attempt response, supplier statement line, and success/failure cost instead carries the supplier's distinct canonical 1–200-character `provider_request_id`. Null, non-string, blank, `None`, `missing`, and 201-character identities make all three adapters fail closed into reconciliation without a fabricated hash/UUID/task-ID fallback. The live child is killed by `SIGKILL` immediately after a paid response and before job/result/cost persistence; the finalized supplier statement is the starting reconciliation set and every billed request maps through the official request log to exactly one `ProviderCostEntry.task_id` with equal fen.
14. Seedream, Seedance, and standard TTS have separate 100-fen/one-completion targets and separate hash-pinned release evidence. Production checks the exact adapter/capability/model/version/region before reading secrets or constructing HTTP. A capability without proven official query/idempotency or a passing deployed supplier-statement reconciliation stays disabled; evidence for one adapter cannot enable another.

The live commands are only Task 17 Step 13 and run after the complete no-credential suite. A passing PoC records the exact activated resource, pinned model ID/version, region, quota, response schema, temporary-URL lifetime, supplier amount, failure behavior, reviewer, and evidence digest. Digital-human and cloned-voice PoCs are not part of Plan 03; Plan 06 owns their permission, authorization, deletion, and release gates.

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-24-03-generative-media-studio.md`. Choose one execution mode before changing application code:

1. **Subagent-Driven (this session):** dispatch one fresh implementation subagent per task, review the stated tests and diff after every task, and preserve all 17 commit checkpoints.
2. **Inline Execution (separate session):** open a fresh implementation session with `superpowers:executing-plans`, execute Tasks 1–17 in order, and stop at each verification/commit checkpoint for review.

Do not run the paid Task 17 Step 13 PoCs in either mode until the full no-credential suite passes and an authorized operator has supplied the dedicated account, hard budget, evidence directory, and explicit acknowledgement.
