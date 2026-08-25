# Reseller Operations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver an auditable reseller-operations layer that keeps offline cash separate from creation credits, enforces the fixed platform→L1→L2→C/platform→L1→C hierarchy, posts allocations and approved returns atomically, freezes order prices, grants narrowly scoped support access, migrates accounts without rewriting history, and exposes privacy-safe operations and reports.

**Architecture:** Extend the Plan 01 modular monolith with one `resellers` module and one Alembic revision, while reusing its synchronous `Session`, account, project-access, audit, and append-only credit-ledger contracts. Cash evidence, cash confirmation, and credit movement are separate records: a confirmed cash order authorizes a later allocation but never invokes the ledger itself. Reseller support reads go through an explicit scoped-access service; ordinary `ProjectAccessService.require_viewer()` remains default-deny for resellers.

**Tech Stack:** Python 3.12, FastAPI 0.136.x, Pydantic 2.x with `extra="forbid"`, SQLAlchemy 2.0 synchronous ORM, Alembic, PostgreSQL 16, Plan 01 private TOS adapter, Pytest, Hypothesis, Next.js 16 App Router, React 19, TypeScript 5.x, Playwright.

---

## Scope, ordering, and frozen dependencies

Execute Plans 01–04 first. This plan creates exactly one migration:

```python
revision = "0005_resellers"
down_revision = "0004_publication"
```

Do not create another task table, wallet, ledger, hold, billing wrapper, project-access bypass, asynchronous application service, payment provider, or automatic settlement path. This plan does not submit model work, so it does not call `TaskSubmissionService` or the four generation methods on `BillingService`. Customer generation continues to use Plan 01's exact task/billing flow; reseller allocation and return are commercial double-entry movements through `CreditLedgerService`.

Use these frozen imports and signatures:

```python
from ip_saas.db.base import Base
from ip_saas.db.session import get_session, session_scope
from ip_saas.common.clock import Clock, SystemClock
from ip_saas.common.audit import AuditWriter
from ip_saas.common.errors import Conflict, Forbidden, NotFound
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import (
    Account,
    AccountKind,
    AccountStatus,
    AuthSession,
    Principal,
    ResellerRelation,
)
from ip_saas.modules.accounts.service import AccountService
from ip_saas.modules.projects.models import IPProject, ProjectOwnerType
from ip_saas.modules.projects.access import ProjectAccessService
from ip_saas.modules.billing.repository import CreditLedgerRepository
from ip_saas.modules.billing.service import CreditLedgerService
```

`AuditWriter.write` is always called with Plan 01's exact keyword-only contract:

```python
audit.write(
    session,
    actor=actor,
    action="stable.action.name",
    target_type="stable_target_type",
    target_id=target_id,
    project_id=project_id_or_none,
    metadata={"non_sensitive": "value"},
)
```

FastAPI handlers are synchronous `def` functions. `get_session()` already enters Plan 01's `session_scope()`, so handlers call one application-service method directly and never wrap the injected session in another `session.begin()`. A raised exception rolls back the cash/request/audit/ledger work together.

## File map

```text
backend/migrations/versions/0005_resellers.py
backend/src/ip_saas/db/base.py
backend/src/ip_saas/modules/accounts/service.py
backend/src/ip_saas/modules/projects/access.py
backend/src/ip_saas/modules/billing/adapters.py
backend/src/ip_saas/modules/billing/composition.py
backend/src/ip_saas/modules/billing/models.py
backend/src/ip_saas/modules/billing/limits.py
backend/src/ip_saas/modules/billing/service.py
backend/src/ip_saas/modules/resellers/__init__.py
backend/src/ip_saas/modules/resellers/enums.py
backend/src/ip_saas/modules/resellers/models.py
backend/src/ip_saas/modules/resellers/schemas.py
backend/src/ip_saas/modules/resellers/access.py
backend/src/ip_saas/modules/resellers/pricing.py
backend/src/ip_saas/modules/resellers/evidence.py
backend/src/ip_saas/modules/resellers/ports.py
backend/src/ip_saas/modules/resellers/adapters.py
backend/src/ip_saas/modules/resellers/service.py
backend/src/ip_saas/modules/resellers/support.py
backend/src/ip_saas/modules/resellers/lifecycle.py
backend/src/ip_saas/modules/resellers/reporting.py
backend/src/ip_saas/modules/resellers/router.py
backend/src/ip_saas/api.py
backend/tests/unit/resellers/test_schemas.py
backend/tests/unit/resellers/test_transfer_properties.py
backend/tests/integration/resellers/test_access_and_pricing.py
backend/tests/integration/resellers/test_cash_orders.py
backend/tests/integration/resellers/test_credit_movements.py
backend/tests/integration/resellers/test_lifecycle.py
backend/tests/integration/resellers/test_reports.py
backend/tests/contract/billing/test_generation_limit_composition.py
backend/tests/integration/billing/test_generation_limits.py
backend/tests/integration/billing/test_generation_limit_concurrency.py
backend/tests/integration/billing/test_internal_costs.py
backend/tests/integration/tasks/test_task_submission.py
backend/tests/integration/intelligence/conftest.py
backend/tests/integration/intelligence/test_api_workflow.py
backend/tests/integration/intelligence/test_self_marketing_isolation.py
backend/tests/integration/media/conftest.py
backend/tests/integration/media/harness.py
backend/tests/support/publication_fixtures.py
backend/tests/integration/publication/test_billed_failure_recovery.py
backend/tests/fixtures/__init__.py
backend/tests/fixtures/generation_limits.py
backend/tests/security/test_reseller_privacy.py
backend/tests/integration/test_health_and_migrations.py
contracts/openapi.json
frontend/src/app/(reseller)/reseller/customers/page.tsx
frontend/src/app/(reseller)/reseller/orders/page.tsx
frontend/src/app/(reseller)/reseller/credits/page.tsx
frontend/src/app/(reseller)/reseller/prices/page.tsx
frontend/src/app/(reseller)/reseller/reports/page.tsx
frontend/src/app/(reseller)/reseller/migration/page.tsx
frontend/src/app/(c-user)/projects/[projectId]/support/page.tsx
frontend/src/app/(platform)/platform/resellers/page.tsx
frontend/src/features/resellers/api.ts
frontend/src/features/resellers/CustomerTable.tsx
frontend/src/features/resellers/OfflineOrderForm.tsx
frontend/src/features/resellers/CreditMovementPanel.tsx
frontend/src/features/resellers/PriceForm.tsx
frontend/src/features/resellers/SupportGrantPanel.tsx
frontend/src/features/resellers/MigrationRequestForm.tsx
frontend/src/features/resellers/LifecyclePanel.tsx
frontend/src/features/resellers/ReportTable.tsx
frontend/src/features/resellers/GenerationLimitStatusCard.tsx
frontend/tests/e2e/reseller-operations.spec.ts
README.md
```

## Frozen business decisions

- The only active commercial edges are platform→L1, L1→L2, L1→C, and L2→C. A direct relation is required for every cash order and credit allocation. L1 reports may include C accounts one extra hop below their direct L2; no query walks deeper than two reseller edges.
- For a cash order, `payer_account_id` is the lower account and `payee_account_id` is its direct parent. For the later allocation, credits flow in the opposite direction: source is the payee/parent and destination is the payer/child.
- `reported_amount_fen` is a payer-supplied real amount. If it is absent, `estimated_amount_fen` is displayed explicitly as an estimate. The payee must enter `received_amount_fen` when confirming. Only that confirmed amount validates the later allocation.
- Uploading evidence, recording a cash order, and confirming receipt never call a credit method. A screenshot is evidence only.
- A platform→L1 allocation issues credits from Plan 01's `credit_treasury` system wallet. All L1/L2/C allocations and returns use Plan 01's locked, balanced `transfer()`. A return to the platform transfers credits back into `credit_treasury`; it never deletes or edits an earlier entry.
- One confirmed cash order can fund one posted allocation. Returns have their own request and approval shape and never require a cash-order ID.
- Every cash order stores one exact `ResellerPriceVersion.id`. Later price versions affect only orders recorded at or after their effective time.
- Plan 01 `PricingVersion` remains the source for capability consumption prices. `ResellerPriceVersion` covers reseller package wholesale/retail and platform activity prices only.
- A support grant is created and revoked only by the C-user project owner, lasts at most seven days, names explicit scopes, and opens no access through `ProjectAccessService.require_viewer()`. Every granted read writes an audit event.
- Suspending an account revokes its sessions and blocks that account's login, child creation, price changes, cash confirmation, credit movements, and new customer generation holds. Active descendant C users retain their own project access.
- A C account may move to an active L1 or L2. An active L2 may move only to an active L1. The subject account requests, a platform admin approves and executes, and the existing unique relation row is locked and repointed; `AccountMigrationRequest` plus audit preserves old/new ownership history. Credits never move automatically.
- Reports show actual cash, estimated cash, credit allocations/returns, credit consumption, balances, support state, and migration exceptions as separate fields. They never infer cash revenue from credits and never join interview, intelligence, content, media, publication-detail, voice, or consent tables.

### Task 1: Freeze strict request schemas and reseller persistence models

**Files:**
- Create: `backend/src/ip_saas/modules/resellers/__init__.py`
- Create: `backend/src/ip_saas/modules/resellers/enums.py`
- Create: `backend/src/ip_saas/modules/resellers/schemas.py`
- Create: `backend/src/ip_saas/modules/resellers/models.py`
- Test: `backend/tests/unit/resellers/test_schemas.py`

- [ ] **Step 1: Write the failing strict-schema tests**

Create `backend/tests/unit/resellers/test_schemas.py`:

```python
from datetime import UTC, datetime, timedelta
from uuid import uuid4

import pytest
from pydantic import ValidationError
from sqlalchemy import UniqueConstraint

from ip_saas.modules.accounts.models import AccountKind
from ip_saas.modules.resellers.enums import SupportScope
from ip_saas.modules.resellers.models import (
    CreditMovementRequest,
    OfflineCashOrder,
    SupportGrant,
)
from ip_saas.modules.resellers.schemas import (
    CashConfirmation,
    CreditAllocationCreate,
    CreditReturnCreate,
    OfflineCashOrderCreate,
    ResellerPriceCreate,
    SupportGrantCreate,
)


def test_cash_order_accepts_real_or_estimated_amount_but_not_both() -> None:
    common = {
        "payer_account_id": uuid4(),
        "payee_account_id": uuid4(),
        "evidence_asset_id": uuid4(),
        "pricing_version_id": uuid4(),
        "note": "bank transfer",
        "idempotency_key": "cash-order-0001",
    }
    assert OfflineCashOrderCreate(**common, reported_amount_fen=10_000).estimated_amount_fen is None
    assert OfflineCashOrderCreate(**common, estimated_amount_fen=10_000).reported_amount_fen is None
    with pytest.raises(ValidationError):
        OfflineCashOrderCreate(**common)
    with pytest.raises(ValidationError):
        OfflineCashOrderCreate(
            **common,
            reported_amount_fen=10_000,
            estimated_amount_fen=9_500,
        )


def test_confirmation_requires_received_cash() -> None:
    with pytest.raises(ValidationError):
        CashConfirmation(received_amount_fen=0)


def test_allocation_and_return_are_distinct_contracts() -> None:
    allocation = CreditAllocationCreate(
        cash_order_id=uuid4(),
        credit_units=1_000,
        idempotency_key="allocate-0001",
    )
    returned = CreditReturnCreate(
        source_account_id=uuid4(),
        credit_units=100,
        reason="unused balance",
        idempotency_key="return-request-0001",
    )
    assert "cash_order_id" in allocation.model_fields
    assert "cash_order_id" not in returned.model_fields
    assert "destination_account_id" not in returned.model_fields


def test_support_and_price_payloads_forbid_extra_fields() -> None:
    now = datetime(2026, 8, 24, 8, 0, tzinfo=UTC)
    SupportGrantCreate(
        project_id=uuid4(),
        reseller_account_id=uuid4(),
        scopes=[SupportScope.PROFILE],
        expires_at=now + timedelta(days=1),
        reason="help correct the profile",
        idempotency_key="support-grant-0001",
    )
    with pytest.raises(ValidationError):
        ResellerPriceCreate(
            seller_account_id=uuid4(),
            buyer_kind=AccountKind.C_USER,
            product_code="credits.standard",
            package_credit_units=1_000,
            package_price_fen=10_000,
            minimum_downstream_price_fen=None,
            effective_at=now,
            invented=True,
        )


def test_client_idempotency_keys_are_scoped_to_an_account_boundary() -> None:
    def unique_columns(model: type) -> set[frozenset[str]]:
        return {
            frozenset(column.name for column in constraint.columns)
            for constraint in model.__table__.constraints
            if isinstance(constraint, UniqueConstraint)
        }

    assert frozenset({"recorded_by_account_id", "idempotency_key"}) in unique_columns(
        OfflineCashOrder
    )
    assert frozenset({"source_account_id", "idempotency_key"}) in unique_columns(
        CreditMovementRequest
    )
    assert frozenset({"customer_account_id", "idempotency_key"}) in unique_columns(
        SupportGrant
    )
```

- [ ] **Step 2: Run the tests and verify the missing-module failure**

Run: `cd backend && uv run pytest tests/unit/resellers/test_schemas.py -v`

Expected: FAIL during collection with `ModuleNotFoundError: No module named 'ip_saas.modules.resellers'`.

- [ ] **Step 3: Create the closed enums**

Create `backend/src/ip_saas/modules/resellers/enums.py`:

```python
from enum import StrEnum


class CashOrderStatus(StrEnum):
    RECORDED = "recorded"
    CONFIRMED = "confirmed"
    CANCELLED = "cancelled"


class CreditMovementKind(StrEnum):
    ALLOCATION = "allocation"
    RETURN = "return"


class CreditMovementStatus(StrEnum):
    REQUESTED = "requested"
    REJECTED = "rejected"
    POSTED = "posted"


class MigrationStatus(StrEnum):
    REQUESTED = "requested"
    APPROVED = "approved"
    COMPLETED = "completed"
    REJECTED = "rejected"


class SupportScope(StrEnum):
    PROFILE = "profile"
    STRATEGY = "strategy"
    CONTENT = "content"
    MEDIA = "media"
    PUBLICATION = "publication"
```

Create an empty `backend/src/ip_saas/modules/resellers/__init__.py`.

- [ ] **Step 4: Create strict commands**

Create `backend/src/ip_saas/modules/resellers/schemas.py`:

```python
from datetime import datetime
from uuid import UUID

from pydantic import BaseModel, ConfigDict, Field, model_validator

from ip_saas.modules.accounts.models import AccountKind

from .enums import SupportScope


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class OfflineCashOrderCreate(StrictModel):
    payer_account_id: UUID
    payee_account_id: UUID
    reported_amount_fen: int | None = Field(default=None, gt=0)
    estimated_amount_fen: int | None = Field(default=None, gt=0)
    evidence_asset_id: UUID
    pricing_version_id: UUID
    note: str = Field(default="", max_length=500)
    idempotency_key: str = Field(min_length=8, max_length=160)

    @model_validator(mode="after")
    def validate_parties_and_amount(self) -> "OfflineCashOrderCreate":
        if self.payer_account_id == self.payee_account_id:
            raise ValueError("payer and payee must differ")
        if (self.reported_amount_fen is None) == (self.estimated_amount_fen is None):
            raise ValueError("provide exactly one real or estimated amount")
        return self


class CashConfirmation(StrictModel):
    received_amount_fen: int = Field(gt=0)


class CreditAllocationCreate(StrictModel):
    cash_order_id: UUID
    credit_units: int = Field(gt=0)
    idempotency_key: str = Field(min_length=8, max_length=160)


class CreditReturnCreate(StrictModel):
    source_account_id: UUID
    credit_units: int = Field(gt=0)
    reason: str = Field(min_length=1, max_length=500)
    idempotency_key: str = Field(min_length=8, max_length=160)


class ReturnRejection(StrictModel):
    reason: str = Field(min_length=1, max_length=500)


class ResellerPriceCreate(StrictModel):
    seller_account_id: UUID
    buyer_kind: AccountKind
    product_code: str = Field(pattern=r"^[a-z0-9_.-]+$", max_length=80)
    package_credit_units: int = Field(gt=0)
    package_price_fen: int = Field(gt=0)
    minimum_downstream_price_fen: int | None = Field(default=None, gt=0)
    effective_at: datetime
    promotion_code: str | None = Field(default=None, pattern=r"^[A-Z0-9_-]+$", max_length=40)

    @model_validator(mode="after")
    def c_user_is_terminal(self) -> "ResellerPriceCreate":
        if self.buyer_kind == AccountKind.C_USER and self.minimum_downstream_price_fen is not None:
            raise ValueError("a C-user price has no downstream floor")
        if self.buyer_kind != AccountKind.C_USER and self.minimum_downstream_price_fen is None:
            raise ValueError("a reseller price requires a downstream floor")
        return self


class SupportGrantCreate(StrictModel):
    project_id: UUID
    reseller_account_id: UUID
    scopes: list[SupportScope] = Field(min_length=1, max_length=5)
    expires_at: datetime
    reason: str = Field(min_length=1, max_length=500)
    idempotency_key: str = Field(min_length=8, max_length=160)


class SupportRevocationCreate(StrictModel):
    reason: str = Field(min_length=1, max_length=500)


class MigrationCreate(StrictModel):
    subject_account_id: UUID
    new_parent_account_id: UUID
    reason: str = Field(min_length=1, max_length=500)


class MigrationRejection(StrictModel):
    reason: str = Field(min_length=1, max_length=500)
```

- [ ] **Step 5: Create the Plan 05 ORM rows**

Create `backend/src/ip_saas/modules/resellers/models.py`:

```python
from datetime import datetime
from uuid import UUID

from sqlalchemy import (
    BigInteger,
    CheckConstraint,
    DateTime,
    ForeignKey,
    Index,
    String,
    Text,
    UniqueConstraint,
    text,
)
from sqlalchemy.orm import Mapped, mapped_column
from sqlalchemy.dialects.postgresql import JSONB

from ip_saas.db.base import Base, TimestampMixin, UUIDPrimaryKeyMixin


class CommercialEvidenceAsset(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "commercial_evidence_assets"
    owner_account_id: Mapped[UUID] = mapped_column(ForeignKey("accounts.id"), index=True)
    object_key: Mapped[str] = mapped_column(String(500), unique=True)
    sha256: Mapped[str] = mapped_column(String(64))
    content_type: Mapped[str] = mapped_column(String(100))
    size_bytes: Mapped[int] = mapped_column(BigInteger)
    uploaded_by: Mapped[UUID]


class ResellerPriceVersion(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "reseller_price_versions"
    __table_args__ = (
        UniqueConstraint(
            "seller_account_id",
            "buyer_kind",
            "product_code",
            "version_no",
            name="uq_reseller_price_version",
        ),
        CheckConstraint("package_credit_units > 0", name="ck_reseller_price_credits_positive"),
        CheckConstraint("package_price_fen > 0", name="ck_reseller_price_fen_positive"),
    )
    seller_account_id: Mapped[UUID] = mapped_column(ForeignKey("accounts.id"), index=True)
    buyer_kind: Mapped[str] = mapped_column(String(32), index=True)
    product_code: Mapped[str] = mapped_column(String(80), index=True)
    version_no: Mapped[int]
    package_credit_units: Mapped[int] = mapped_column(BigInteger)
    package_price_fen: Mapped[int] = mapped_column(BigInteger)
    minimum_downstream_price_fen: Mapped[int | None] = mapped_column(BigInteger)
    platform_floor_fen: Mapped[int] = mapped_column(BigInteger)
    effective_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), index=True)
    promotion_code: Mapped[str | None] = mapped_column(String(40))
    created_by: Mapped[UUID]


class OfflineCashOrder(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "offline_cash_orders"
    __table_args__ = (
        UniqueConstraint(
            "recorded_by_account_id",
            "idempotency_key",
            name="uq_cash_order_account_idempotency",
        ),
        CheckConstraint(
            "(reported_amount_fen is null) <> (estimated_amount_fen is null)",
            name="ck_cash_order_one_reported_amount",
        ),
        CheckConstraint(
            "confirmed_amount_fen is null or confirmed_amount_fen > 0",
            name="ck_cash_order_confirmed_amount",
        ),
    )
    payer_account_id: Mapped[UUID] = mapped_column(ForeignKey("accounts.id"), index=True)
    payee_account_id: Mapped[UUID] = mapped_column(ForeignKey("accounts.id"), index=True)
    reported_amount_fen: Mapped[int | None] = mapped_column(BigInteger)
    estimated_amount_fen: Mapped[int | None] = mapped_column(BigInteger)
    confirmed_amount_fen: Mapped[int | None] = mapped_column(BigInteger)
    evidence_asset_id: Mapped[UUID] = mapped_column(ForeignKey("commercial_evidence_assets.id"))
    pricing_version_id: Mapped[UUID] = mapped_column(ForeignKey("reseller_price_versions.id"))
    note: Mapped[str] = mapped_column(Text)
    status: Mapped[str] = mapped_column(String(20), index=True)
    idempotency_key: Mapped[str] = mapped_column(String(160))
    recorded_by_account_id: Mapped[UUID] = mapped_column(
        ForeignKey("accounts.id"), index=True
    )
    recorded_by: Mapped[UUID]
    confirmed_by: Mapped[UUID | None]
    confirmed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))


class CreditMovementRequest(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "credit_movement_requests"
    __table_args__ = (
        UniqueConstraint(
            "source_account_id",
            "idempotency_key",
            name="uq_credit_movement_account_idempotency",
        ),
        Index(
            "uq_posted_allocation_cash_order",
            "cash_order_id",
            unique=True,
            postgresql_where=text("kind = 'allocation' AND status = 'posted'"),
        ),
        CheckConstraint("credit_units > 0", name="ck_credit_movement_positive"),
    )
    kind: Mapped[str] = mapped_column(String(20), index=True)
    source_account_id: Mapped[UUID] = mapped_column(ForeignKey("accounts.id"), index=True)
    destination_account_id: Mapped[UUID] = mapped_column(ForeignKey("accounts.id"), index=True)
    credit_units: Mapped[int] = mapped_column(BigInteger)
    cash_order_id: Mapped[UUID | None] = mapped_column(ForeignKey("offline_cash_orders.id"))
    pricing_version_id: Mapped[UUID | None] = mapped_column(
        ForeignKey("reseller_price_versions.id")
    )
    idempotency_key: Mapped[str] = mapped_column(String(160))
    status: Mapped[str] = mapped_column(String(20), index=True)
    reason: Mapped[str | None] = mapped_column(Text)
    requested_by: Mapped[UUID]
    decided_by: Mapped[UUID | None]
    decided_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    ledger_transaction_id: Mapped[UUID | None]


class SupportGrant(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "support_grants"
    __table_args__ = (
        UniqueConstraint(
            "customer_account_id",
            "idempotency_key",
            name="uq_support_grant_account_idempotency",
        ),
    )
    project_id: Mapped[UUID] = mapped_column(ForeignKey("ip_projects.id"), index=True)
    customer_account_id: Mapped[UUID] = mapped_column(ForeignKey("accounts.id"), index=True)
    reseller_account_id: Mapped[UUID] = mapped_column(ForeignKey("accounts.id"), index=True)
    scopes: Mapped[list[str]] = mapped_column(JSONB)
    reason: Mapped[str] = mapped_column(Text)
    expires_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), index=True)
    granted_by: Mapped[UUID]
    idempotency_key: Mapped[str] = mapped_column(String(160))


class SupportGrantRevocation(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "support_grant_revocations"
    __table_args__ = (UniqueConstraint("grant_id", name="uq_support_grant_revocation"),)
    grant_id: Mapped[UUID] = mapped_column(ForeignKey("support_grants.id"), index=True)
    revoked_by: Mapped[UUID]
    reason: Mapped[str] = mapped_column(Text)


class AccountMigrationRequest(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "account_migration_requests"
    __table_args__ = (
        Index(
            "uq_open_account_migration",
            "subject_account_id",
            unique=True,
            postgresql_where=text("status IN ('requested', 'approved')"),
        ),
        CheckConstraint(
            "old_parent_account_id <> new_parent_account_id",
            name="ck_migration_parent_changes",
        ),
    )
    subject_account_id: Mapped[UUID] = mapped_column(ForeignKey("accounts.id"), index=True)
    old_parent_account_id: Mapped[UUID] = mapped_column(ForeignKey("accounts.id"))
    new_parent_account_id: Mapped[UUID] = mapped_column(ForeignKey("accounts.id"))
    reason: Mapped[str] = mapped_column(Text)
    status: Mapped[str] = mapped_column(String(20), index=True)
    requested_by: Mapped[UUID]
    approved_by: Mapped[UUID | None]
    approved_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    rejected_by: Mapped[UUID | None]
    rejection_reason: Mapped[str | None] = mapped_column(Text)
    completed_by: Mapped[UUID | None]
    completed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
```

- [ ] **Step 6: Run schema and account-scoped idempotency tests**

Run: `cd backend && uv run pytest tests/unit/resellers/test_schemas.py -v`

Expected: PASS, 5 tests; malformed or extra request fields are rejected, and unrelated accounts may reuse the same client-generated idempotency key without sharing a uniqueness boundary.

- [ ] **Step 7: Commit strict contracts and models**

```bash
git add backend/src/ip_saas/modules/resellers backend/tests/unit/resellers/test_schemas.py
git commit -m "feat: define reseller operation contracts"
```

### Task 2: Enforce the fixed tree and hierarchical price selection

**Files:**
- Create: `backend/src/ip_saas/modules/resellers/access.py`
- Create: `backend/src/ip_saas/modules/resellers/pricing.py`
- Test: `backend/tests/integration/resellers/test_access_and_pricing.py`

- [ ] **Step 1: Write failing depth and price-boundary tests**

Create `backend/tests/integration/resellers/test_access_and_pricing.py`:

```python
from datetime import UTC, datetime, timedelta
from uuid import uuid4

import pytest
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.common.errors import Conflict, Forbidden
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import AccountKind
from ip_saas.modules.accounts.service import AccountService
from ip_saas.modules.resellers.access import ResellerTreeAccess
from ip_saas.modules.resellers.pricing import PricingService
from ip_saas.modules.resellers.schemas import ResellerPriceCreate


class FixedClock:
    def now(self) -> datetime:
        return datetime(2026, 8, 24, 8, 0, tzinfo=UTC)


def test_tree_rejects_a_third_reseller_level(db_session: Session) -> None:
    accounts = AccountService()
    platform = accounts.bootstrap_platform(db_session, "Platform")
    admin_principal = accounts.add_principal(db_session, platform.id, "depth-admin", "hash")
    admin = ActorContext(admin_principal.id, platform.id, ActorKind.PLATFORM_ADMIN)
    l1 = accounts.create_child_account(
        db_session, admin, platform.id, AccountKind.RESELLER_L1, "L1"
    )
    l1_actor = ActorContext(uuid4(), l1.id, ActorKind.RESELLER_L1)
    l2 = accounts.create_child_account(
        db_session, l1_actor, l1.id, AccountKind.RESELLER_L2, "L2"
    )
    with pytest.raises(Forbidden):
        accounts.create_child_account(
            db_session,
            ActorContext(uuid4(), l2.id, ActorKind.RESELLER_L2),
            l2.id,
            AccountKind.RESELLER_L2,
            "L3",
        )


def test_agent_price_cannot_cross_platform_floor(db_session: Session) -> None:
    accounts = AccountService()
    platform = accounts.bootstrap_platform(db_session, "Platform")
    admin_principal = accounts.add_principal(db_session, platform.id, "price-admin", "hash")
    admin = ActorContext(admin_principal.id, platform.id, ActorKind.PLATFORM_ADMIN)
    l1 = accounts.create_child_account(
        db_session, admin, platform.id, AccountKind.RESELLER_L1, "Price L1"
    )
    service = PricingService(ResellerTreeAccess(), AuditWriter(FixedClock()), FixedClock())
    first = service.create_version(
        db_session,
        admin,
        ResellerPriceCreate(
            seller_account_id=platform.id,
            buyer_kind=AccountKind.RESELLER_L1,
            product_code="credits.standard",
            package_credit_units=1_000,
            package_price_fen=8_000,
            minimum_downstream_price_fen=9_000,
            effective_at=FixedClock().now(),
        ),
    )
    with pytest.raises(Conflict, match="platform floor"):
        service.create_version(
            db_session,
            ActorContext(uuid4(), l1.id, ActorKind.RESELLER_L1),
            ResellerPriceCreate(
                seller_account_id=l1.id,
                buyer_kind=AccountKind.C_USER,
                product_code="credits.standard",
                package_credit_units=1_000,
                package_price_fen=8_999,
                minimum_downstream_price_fen=None,
                effective_at=FixedClock().now() + timedelta(minutes=1),
            ),
        )
    assert first.version_no == 1
    assert first.package_price_fen == 8_000
```

- [ ] **Step 2: Run the tests and verify missing services**

Run: `cd backend && uv run pytest tests/integration/resellers/test_access_and_pricing.py -v`

Expected: FAIL during collection because `ResellerTreeAccess` and `PricingService` are missing.

- [ ] **Step 3: Implement a bounded, non-recursive tree access service**

Create `backend/src/ip_saas/modules/resellers/access.py`:

```python
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.errors import Forbidden, NotFound
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import Account, AccountKind, AccountStatus, ResellerRelation


class ResellerTreeAccess:
    def require_active_account(self, session: Session, account_id: UUID) -> Account:
        account = session.get(Account, account_id)
        if account is None:
            raise NotFound("account")
        if account.status != AccountStatus.ACTIVE:
            raise Forbidden("account is suspended")
        return account

    def require_active_actor(self, session: Session, actor: ActorContext) -> Account:
        return self.require_active_account(session, actor.account_id)

    def require_direct_relation(
        self,
        session: Session,
        parent_account_id: UUID,
        child_account_id: UUID,
        *,
        lock: bool = False,
    ) -> ResellerRelation:
        statement = select(ResellerRelation).where(
            ResellerRelation.parent_account_id == parent_account_id,
            ResellerRelation.child_account_id == child_account_id,
            ResellerRelation.active.is_(True),
        )
        if lock:
            statement = statement.with_for_update()
        relation = session.scalar(statement)
        if relation is None:
            raise Forbidden("accounts are not in a direct active commercial relation")
        self.require_active_account(session, parent_account_id)
        self.require_active_account(session, child_account_id)
        return relation

    def require_actor_controls_source(
        self,
        session: Session,
        actor: ActorContext,
        source_account_id: UUID,
    ) -> Account:
        account = self.require_active_actor(session, actor)
        if actor.account_id != source_account_id:
            raise Forbidden("only the source account can move credits")
        if account.kind == AccountKind.PLATFORM and actor.kind != ActorKind.PLATFORM_ADMIN:
            raise Forbidden("platform admin is required")
        return account

    def parent_of(
        self,
        session: Session,
        child_account_id: UUID,
        *,
        lock: bool = False,
    ) -> Account:
        statement = select(ResellerRelation).where(
            ResellerRelation.child_account_id == child_account_id,
            ResellerRelation.active.is_(True),
        )
        if lock:
            statement = statement.with_for_update()
        relation = session.scalar(statement)
        if relation is None:
            raise NotFound("active parent relation")
        return self.require_active_account(session, relation.parent_account_id)

    def is_ancestor(
        self,
        session: Session,
        ancestor_account_id: UUID,
        child_account_id: UUID,
    ) -> bool:
        direct = session.scalar(
            select(ResellerRelation.id).where(
                ResellerRelation.parent_account_id == ancestor_account_id,
                ResellerRelation.child_account_id == child_account_id,
                ResellerRelation.active.is_(True),
            )
        )
        if direct is not None:
            return True
        middle_ids = list(
            session.scalars(
                select(ResellerRelation.child_account_id).where(
                    ResellerRelation.parent_account_id == ancestor_account_id,
                    ResellerRelation.active.is_(True),
                )
            )
        )
        if not middle_ids:
            return False
        return (
            session.scalar(
                select(ResellerRelation.id).where(
                    ResellerRelation.parent_account_id.in_(middle_ids),
                    ResellerRelation.child_account_id == child_account_id,
                    ResellerRelation.active.is_(True),
                )
            )
            is not None
        )

    def visible_descendant_ids(self, session: Session, actor: ActorContext) -> set[UUID]:
        account = self.require_active_actor(session, actor)
        direct = set(
            session.scalars(
                select(ResellerRelation.child_account_id).where(
                    ResellerRelation.parent_account_id == actor.account_id,
                    ResellerRelation.active.is_(True),
                )
            )
        )
        if account.kind != AccountKind.RESELLER_L1:
            return direct
        l2_ids = set(
            session.scalars(
                select(Account.id).where(
                    Account.id.in_(direct),
                    Account.kind == AccountKind.RESELLER_L2,
                )
            )
        )
        indirect = (
            set(
                session.scalars(
                    select(ResellerRelation.child_account_id).where(
                        ResellerRelation.parent_account_id.in_(l2_ids),
                        ResellerRelation.active.is_(True),
                    )
                )
            )
            if l2_ids
            else set()
        )
        return direct | indirect
```

- [ ] **Step 4: Implement authority, floor inheritance, advisory locking, and lookup**

Create `backend/src/ip_saas/modules/resellers/pricing.py`:

```python
from datetime import datetime
from uuid import UUID

from sqlalchemy import func, select, text
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.common.clock import Clock
from ip_saas.common.errors import Conflict, Forbidden, NotFound
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import Account, AccountKind

from .access import ResellerTreeAccess
from .models import ResellerPriceVersion
from .schemas import ResellerPriceCreate


ALLOWED_PRICE_BUYERS = {
    AccountKind.PLATFORM: {AccountKind.RESELLER_L1},
    AccountKind.RESELLER_L1: {AccountKind.RESELLER_L2, AccountKind.C_USER},
    AccountKind.RESELLER_L2: {AccountKind.C_USER},
    AccountKind.C_USER: set(),
}


class PricingService:
    def __init__(self, tree: ResellerTreeAccess, audit: AuditWriter, clock: Clock) -> None:
        self.tree = tree
        self.audit = audit
        self.clock = clock

    def create_version(
        self,
        session: Session,
        actor: ActorContext,
        command: ResellerPriceCreate,
    ) -> ResellerPriceVersion:
        seller = self.tree.require_active_actor(session, actor)
        if seller.id != command.seller_account_id:
            raise Forbidden("seller must be the actor account")
        seller_kind = AccountKind(seller.kind)
        if command.buyer_kind not in ALLOWED_PRICE_BUYERS[seller_kind]:
            raise Forbidden("seller cannot price for that buyer kind")
        if seller_kind == AccountKind.PLATFORM and actor.kind != ActorKind.PLATFORM_ADMIN:
            raise Forbidden("platform admin is required")
        if command.promotion_code is not None and seller_kind != AccountKind.PLATFORM:
            raise Forbidden("only the platform can create activity prices")

        floor = (
            command.minimum_downstream_price_fen
            if seller_kind == AccountKind.PLATFORM
            else self._platform_floor(session, command.product_code, command.effective_at)
        )
        if floor is None:
            raise Conflict("platform price requires a downstream floor")
        if command.package_price_fen < floor:
            raise Conflict("package price is below the platform floor")
        if (
            command.minimum_downstream_price_fen is not None
            and command.minimum_downstream_price_fen < floor
        ):
            raise Conflict("downstream floor is below the platform floor")

        lock_key = f"reseller-price:{seller.id}:{command.buyer_kind}:{command.product_code}"
        session.execute(
            text("select pg_advisory_xact_lock(hashtext(:lock_key))"),
            {"lock_key": lock_key},
        )
        latest = session.scalar(
            select(func.coalesce(func.max(ResellerPriceVersion.version_no), 0)).where(
                ResellerPriceVersion.seller_account_id == seller.id,
                ResellerPriceVersion.buyer_kind == command.buyer_kind,
                ResellerPriceVersion.product_code == command.product_code,
            )
        )
        row = ResellerPriceVersion(
            seller_account_id=seller.id,
            buyer_kind=command.buyer_kind,
            product_code=command.product_code,
            version_no=int(latest or 0) + 1,
            package_credit_units=command.package_credit_units,
            package_price_fen=command.package_price_fen,
            minimum_downstream_price_fen=command.minimum_downstream_price_fen,
            platform_floor_fen=floor,
            effective_at=command.effective_at,
            promotion_code=command.promotion_code,
            created_by=actor.actor_id,
        )
        session.add(row)
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="reseller_price.version_created",
            target_type="reseller_price_version",
            target_id=row.id,
            metadata={
                "product_code": row.product_code,
                "version_no": row.version_no,
                "buyer_kind": row.buyer_kind,
            },
        )
        return row

    def _platform_floor(
        self,
        session: Session,
        product_code: str,
        at: datetime,
    ) -> int:
        platform_id = session.scalar(
            select(Account.id).where(Account.kind == AccountKind.PLATFORM)
        )
        if platform_id is None:
            raise NotFound("platform account")
        row = self.effective(
            session,
            platform_id,
            AccountKind.RESELLER_L1,
            product_code,
            at,
        )
        return row.minimum_downstream_price_fen or row.package_price_fen

    def effective(
        self,
        session: Session,
        seller_account_id: UUID,
        buyer_kind: AccountKind,
        product_code: str,
        at: datetime,
    ) -> ResellerPriceVersion:
        row = session.scalar(
            select(ResellerPriceVersion)
            .where(
                ResellerPriceVersion.seller_account_id == seller_account_id,
                ResellerPriceVersion.buyer_kind == buyer_kind,
                ResellerPriceVersion.product_code == product_code,
                ResellerPriceVersion.effective_at <= at,
            )
            .order_by(
                ResellerPriceVersion.effective_at.desc(),
                ResellerPriceVersion.version_no.desc(),
            )
            .limit(1)
        )
        if row is None:
            raise NotFound("effective reseller price")
        return row

    def require_order_version(
        self,
        session: Session,
        price_id: UUID,
        seller: Account,
        buyer: Account,
    ) -> ResellerPriceVersion:
        row = session.get(ResellerPriceVersion, price_id)
        if row is None:
            raise NotFound("reseller price version")
        if row.seller_account_id != seller.id or row.buyer_kind != buyer.kind:
            raise Conflict("price version does not match the cash-order parties")
        effective = self.effective(
            session,
            seller.id,
            AccountKind(buyer.kind),
            row.product_code,
            self.clock.now(),
        )
        if effective.id != row.id:
            raise Conflict("new cash orders must use the current effective price")
        return row
```

- [ ] **Step 5: Run depth and pricing tests**

Run: `cd backend && uv run pytest tests/integration/resellers/test_access_and_pricing.py -v`

Expected: PASS, 2 tests; L2→L2 remains forbidden and the inherited floor is enforced.

- [ ] **Step 6: Commit fixed-tree access and price versions**

```bash
git add backend/src/ip_saas/modules/resellers/access.py backend/src/ip_saas/modules/resellers/pricing.py backend/tests/integration/resellers/test_access_and_pricing.py
git commit -m "feat: enforce reseller tree and prices"
```

### Task 3: Store commercial evidence and record cash without touching credits

**Files:**
- Create: `backend/src/ip_saas/modules/resellers/evidence.py`
- Create: `backend/src/ip_saas/modules/resellers/service.py`
- Test: `backend/tests/integration/resellers/test_cash_orders.py`

- [ ] **Step 1: Write the failing cash/credit separation test**

Create `backend/tests/integration/resellers/test_cash_orders.py`:

```python
from datetime import UTC, datetime, timedelta
from uuid import uuid4

from sqlalchemy import func, select
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import AccountKind
from ip_saas.modules.accounts.service import AccountService
from ip_saas.modules.billing.models import CreditLedgerEntry
from ip_saas.modules.resellers.access import ResellerTreeAccess
from ip_saas.modules.resellers.models import CommercialEvidenceAsset, ResellerPriceVersion
from ip_saas.modules.resellers.pricing import PricingService
from ip_saas.modules.resellers.schemas import CashConfirmation, OfflineCashOrderCreate
from ip_saas.modules.resellers.service import CashOrderService


class FixedClock:
    def now(self) -> datetime:
        return datetime(2026, 8, 24, 8, 0, tzinfo=UTC)


def test_record_and_confirm_cash_never_post_credit_entries(db_session: Session) -> None:
    accounts = AccountService()
    platform = accounts.bootstrap_platform(db_session, "Platform")
    admin_principal = accounts.add_principal(db_session, platform.id, "cash-admin", "hash")
    admin = ActorContext(admin_principal.id, platform.id, ActorKind.PLATFORM_ADMIN)
    l1 = accounts.create_child_account(
        db_session, admin, platform.id, AccountKind.RESELLER_L1, "Cash L1"
    )
    l1_actor = ActorContext(uuid4(), l1.id, ActorKind.RESELLER_L1)
    price = ResellerPriceVersion(
        seller_account_id=platform.id,
        buyer_kind=AccountKind.RESELLER_L1,
        product_code="credits.standard",
        version_no=1,
        package_credit_units=1_000,
        package_price_fen=10_000,
        minimum_downstream_price_fen=10_000,
        platform_floor_fen=10_000,
        effective_at=FixedClock().now(),
        promotion_code=None,
        created_by=admin.actor_id,
    )
    evidence = CommercialEvidenceAsset(
        owner_account_id=l1.id,
        object_key=f"accounts/{l1.id}/commercial/{uuid4()}/receipt.png",
        sha256="a" * 64,
        content_type="image/png",
        size_bytes=100,
        uploaded_by=l1_actor.actor_id,
    )
    db_session.add_all([price, evidence])
    db_session.flush()
    service = CashOrderService(
        ResellerTreeAccess(),
        PricingService(ResellerTreeAccess(), AuditWriter(FixedClock()), FixedClock()),
        AuditWriter(FixedClock()),
        FixedClock(),
    )
    command = OfflineCashOrderCreate(
        payer_account_id=l1.id,
        payee_account_id=platform.id,
        reported_amount_fen=10_000,
        evidence_asset_id=evidence.id,
        pricing_version_id=price.id,
        note="bank transfer",
        idempotency_key="cash-separation-1",
    )
    before = db_session.scalar(select(func.count()).select_from(CreditLedgerEntry))
    order = service.record(db_session, l1_actor, command)
    service.confirm(
        db_session,
        admin,
        order.id,
        CashConfirmation(received_amount_fen=10_000),
    )
    after = db_session.scalar(select(func.count()).select_from(CreditLedgerEntry))

    assert order.status == "confirmed"
    assert order.confirmed_amount_fen == 10_000
    assert before == after == 0
```

- [ ] **Step 2: Run the test and verify the missing-service failure**

Run: `cd backend && uv run pytest tests/integration/resellers/test_cash_orders.py -v`

Expected: FAIL during collection because `CashOrderService` is missing.

- [ ] **Step 3: Implement account-scoped evidence upload and five-minute reads**

Create `backend/src/ip_saas/modules/resellers/evidence.py`:

```python
from pathlib import PurePosixPath
from uuid import UUID, uuid4

from sqlalchemy import or_, select
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.common.errors import Forbidden, NotFound
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.providers.object_store import PrivateObjectStore

from .models import CommercialEvidenceAsset, OfflineCashOrder


ALLOWED_EVIDENCE_TYPES = {"image/jpeg", "image/png", "application/pdf"}
MAX_EVIDENCE_BYTES = 10 * 1024 * 1024


class CommercialEvidenceService:
    def __init__(self, store: PrivateObjectStore, audit: AuditWriter) -> None:
        self.store = store
        self.audit = audit

    def upload(
        self,
        session: Session,
        actor: ActorContext,
        filename: str,
        content_type: str,
        body: bytes,
    ) -> CommercialEvidenceAsset:
        if (
            not filename
            or filename != PurePosixPath(filename).name
            or "\\" in filename
        ):
            raise Forbidden("filename must be a plain basename")
        if content_type not in ALLOWED_EVIDENCE_TYPES:
            raise Forbidden("unsupported evidence type")
        if not body or len(body) > MAX_EVIDENCE_BYTES:
            raise Forbidden("evidence must contain 1 to 10485760 bytes")
        asset_id = uuid4()
        key = f"accounts/{actor.account_id}/commercial/{asset_id}/{filename}"
        stored = self.store.put_bytes(key, body, content_type)
        asset = CommercialEvidenceAsset(
            id=asset_id,
            owner_account_id=actor.account_id,
            object_key=stored.key,
            sha256=stored.sha256,
            content_type=stored.content_type,
            size_bytes=stored.size_bytes,
            uploaded_by=actor.actor_id,
        )
        session.add(asset)
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="commercial_evidence.uploaded",
            target_type="commercial_evidence_asset",
            target_id=asset.id,
            metadata={"content_type": content_type, "size_bytes": len(body)},
        )
        return asset

    def signed_read(
        self,
        session: Session,
        actor: ActorContext,
        asset_id: UUID,
    ) -> str:
        asset = session.get(CommercialEvidenceAsset, asset_id)
        if asset is None:
            raise NotFound("commercial evidence")
        party_order_id = session.scalar(
            select(OfflineCashOrder.id).where(
                OfflineCashOrder.evidence_asset_id == asset.id,
                or_(
                    OfflineCashOrder.payer_account_id == actor.account_id,
                    OfflineCashOrder.payee_account_id == actor.account_id,
                ),
            )
        )
        if (
            asset.owner_account_id != actor.account_id
            and party_order_id is None
            and actor.kind != ActorKind.PLATFORM_ADMIN
        ):
            raise NotFound("commercial evidence")
        self.audit.write(
            session,
            actor=actor,
            action="commercial_evidence.read",
            target_type="commercial_evidence_asset",
            target_id=asset.id,
        )
        return self.store.signed_get(asset.object_key, expires_seconds=300)
```

- [ ] **Step 4: Implement idempotent recording and payee-only confirmation**

Create `backend/src/ip_saas/modules/resellers/service.py` initially with:

```python
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.common.clock import Clock
from ip_saas.common.errors import Conflict, Forbidden, NotFound
from ip_saas.modules.accounts.context import ActorContext

from .access import ResellerTreeAccess
from .enums import CashOrderStatus
from .models import CommercialEvidenceAsset, OfflineCashOrder
from .pricing import PricingService
from .schemas import CashConfirmation, OfflineCashOrderCreate


class CashOrderService:
    def __init__(
        self,
        tree: ResellerTreeAccess,
        prices: PricingService,
        audit: AuditWriter,
        clock: Clock,
    ) -> None:
        self.tree = tree
        self.prices = prices
        self.audit = audit
        self.clock = clock

    def record(
        self,
        session: Session,
        actor: ActorContext,
        command: OfflineCashOrderCreate,
    ) -> OfflineCashOrder:
        self.tree.require_active_actor(session, actor)
        payer = self.tree.require_active_account(session, command.payer_account_id)
        payee = self.tree.require_active_account(session, command.payee_account_id)
        self.tree.require_direct_relation(session, payee.id, payer.id)
        if actor.account_id not in {payer.id, payee.id}:
            raise Forbidden("cash-order actor must be payer or payee")
        existing = session.scalar(
            select(OfflineCashOrder).where(
                OfflineCashOrder.recorded_by_account_id == actor.account_id,
                OfflineCashOrder.idempotency_key == command.idempotency_key
            )
        )
        if existing is not None:
            expected = (
                command.payer_account_id,
                command.payee_account_id,
                command.reported_amount_fen,
                command.estimated_amount_fen,
                command.evidence_asset_id,
                command.pricing_version_id,
                command.note,
            )
            actual = (
                existing.payer_account_id,
                existing.payee_account_id,
                existing.reported_amount_fen,
                existing.estimated_amount_fen,
                existing.evidence_asset_id,
                existing.pricing_version_id,
                existing.note,
            )
            if actual != expected:
                raise Conflict("cash-order idempotency key has different input")
            return existing

        evidence = session.get(CommercialEvidenceAsset, command.evidence_asset_id)
        if evidence is None or evidence.owner_account_id != actor.account_id:
            raise Forbidden("cash evidence must belong to the recording party")
        self.prices.require_order_version(
            session,
            command.pricing_version_id,
            payee,
            payer,
        )
        order = OfflineCashOrder(
            payer_account_id=payer.id,
            payee_account_id=payee.id,
            reported_amount_fen=command.reported_amount_fen,
            estimated_amount_fen=command.estimated_amount_fen,
            confirmed_amount_fen=None,
            evidence_asset_id=evidence.id,
            pricing_version_id=command.pricing_version_id,
            note=command.note,
            status=CashOrderStatus.RECORDED,
            idempotency_key=command.idempotency_key,
            recorded_by_account_id=actor.account_id,
            recorded_by=actor.actor_id,
            confirmed_by=None,
            confirmed_at=None,
        )
        session.add(order)
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="cash_order.recorded",
            target_type="offline_cash_order",
            target_id=order.id,
            metadata={"amount_kind": "real" if order.reported_amount_fen else "estimate"},
        )
        return order

    def confirm(
        self,
        session: Session,
        actor: ActorContext,
        order_id: UUID,
        command: CashConfirmation,
    ) -> OfflineCashOrder:
        order = session.scalar(
            select(OfflineCashOrder)
            .where(OfflineCashOrder.id == order_id)
            .with_for_update()
        )
        if order is None:
            raise NotFound("cash order")
        self.tree.require_active_actor(session, actor)
        if actor.account_id != order.payee_account_id:
            raise Forbidden("only the payee confirms cash receipt")
        if order.status == CashOrderStatus.CONFIRMED:
            if order.confirmed_amount_fen != command.received_amount_fen:
                raise Conflict("confirmed cash amount is immutable")
            return order
        if order.status != CashOrderStatus.RECORDED:
            raise Conflict("cash order cannot be confirmed")
        order.status = CashOrderStatus.CONFIRMED
        order.confirmed_amount_fen = command.received_amount_fen
        order.confirmed_by = actor.actor_id
        order.confirmed_at = self.clock.now()
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="cash_order.confirmed",
            target_type="offline_cash_order",
            target_id=order.id,
            metadata={"received_amount_fen": command.received_amount_fen},
        )
        return order
```

The cash methods deliberately have no import from `ip_saas.modules.billing`.

- [ ] **Step 5: Run cash separation tests**

Run: `cd backend && uv run pytest tests/integration/resellers/test_cash_orders.py -v`

Expected: PASS, 1 test; recording and confirmation leave `credit_ledger_entries` empty.

- [ ] **Step 6: Verify the cash service has no billing or task dependency**

Run: `sed -n '1,/^class CreditMovementService:/p' backend/src/ip_saas/modules/resellers/service.py | rg -n 'modules\.billing|BillingService|TaskSubmissionService'`

Expected: no output.

- [ ] **Step 7: Commit evidence and cash-only workflows**

```bash
git add backend/src/ip_saas/modules/resellers/evidence.py backend/src/ip_saas/modules/resellers/service.py backend/tests/integration/resellers/test_cash_orders.py
git commit -m "feat: separate offline cash from credits"
```

### Task 4: Post allocations and approved returns through the Plan 01 ledger

**Files:**
- Create: `backend/src/ip_saas/modules/resellers/ports.py`
- Create: `backend/src/ip_saas/modules/resellers/adapters.py`
- Modify: `backend/src/ip_saas/modules/resellers/service.py`
- Test: `backend/tests/integration/resellers/test_credit_movements.py`

- [ ] **Step 1: Write failing direction, idempotency, and rollback tests**

Create `backend/tests/integration/resellers/test_credit_movements.py`:

```python
from datetime import UTC, datetime, timedelta
from uuid import UUID, uuid4

import pytest
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.common.errors import Conflict
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import Account, AccountKind, ResellerRelation
from ip_saas.modules.billing.models import CreditLedgerEntry
from ip_saas.modules.resellers.access import ResellerTreeAccess
from ip_saas.modules.resellers.enums import CashOrderStatus
from ip_saas.modules.resellers.models import (
    CommercialEvidenceAsset,
    CreditMovementRequest,
    OfflineCashOrder,
    ResellerPriceVersion,
)
from ip_saas.modules.resellers.schemas import CreditAllocationCreate
from ip_saas.modules.resellers.service import CreditMovementService


class FixedClock:
    def now(self) -> datetime:
        return datetime(2026, 8, 24, 8, 0, tzinfo=UTC)


class RecordingMovementPort:
    def __init__(self, fail: bool = False) -> None:
        self.fail = fail
        self.calls: list[tuple[UUID, UUID, int, str]] = []

    def post(
        self,
        session: Session,
        *,
        source_account_id: UUID,
        destination_account_id: UUID,
        credit_units: int,
        idempotency_key: str,
        actor: ActorContext,
    ) -> UUID:
        del session, actor
        self.calls.append(
            (source_account_id, destination_account_id, credit_units, idempotency_key)
        )
        if self.fail:
            raise Conflict("injected ledger failure")
        return uuid4()

    def available(self, session: Session, account_id: UUID) -> int:
        del session, account_id
        return 10_000


def allocation_rows(
    session: Session,
) -> tuple[Account, Account, ActorContext, OfflineCashOrder]:
    parent = Account(kind=AccountKind.RESELLER_L1, display_name="L1")
    child = Account(kind=AccountKind.C_USER, display_name="C")
    session.add_all([parent, child])
    session.flush()
    session.add(
        ResellerRelation(
            parent_account_id=parent.id,
            child_account_id=child.id,
            created_by_principal_id=uuid4(),
            active=True,
        )
    )
    price = ResellerPriceVersion(
        seller_account_id=parent.id,
        buyer_kind=AccountKind.C_USER,
        product_code="credits.standard",
        version_no=1,
        package_credit_units=1_000,
        package_price_fen=10_000,
        minimum_downstream_price_fen=None,
        platform_floor_fen=9_000,
        effective_at=FixedClock().now(),
        promotion_code=None,
        created_by=uuid4(),
    )
    evidence = CommercialEvidenceAsset(
        owner_account_id=child.id,
        object_key=f"accounts/{child.id}/commercial/{uuid4()}/proof.png",
        sha256="b" * 64,
        content_type="image/png",
        size_bytes=10,
        uploaded_by=uuid4(),
    )
    session.add_all([price, evidence])
    session.flush()
    order = OfflineCashOrder(
        payer_account_id=child.id,
        payee_account_id=parent.id,
        reported_amount_fen=10_000,
        estimated_amount_fen=None,
        confirmed_amount_fen=10_000,
        evidence_asset_id=evidence.id,
        pricing_version_id=price.id,
        note="",
        status=CashOrderStatus.CONFIRMED,
        idempotency_key=f"cash-{uuid4()}",
        recorded_by_account_id=child.id,
        recorded_by=uuid4(),
        confirmed_by=uuid4(),
        confirmed_at=FixedClock().now(),
    )
    session.add(order)
    session.flush()
    return parent, child, ActorContext(uuid4(), parent.id, ActorKind.RESELLER_L1), order


def test_allocation_flows_from_payee_parent_to_payer_child_once(db_session: Session) -> None:
    parent, child, actor, order = allocation_rows(db_session)
    port = RecordingMovementPort()
    service = CreditMovementService(
        ResellerTreeAccess(), port, AuditWriter(FixedClock()), FixedClock()
    )
    command = CreditAllocationCreate(
        cash_order_id=order.id,
        credit_units=1_000,
        idempotency_key="allocation-direction-1",
    )
    first = service.allocate(db_session, actor, command)
    second = service.allocate(db_session, actor, command)
    assert first.id == second.id
    assert port.calls == [
        (parent.id, child.id, 1_000, f"movement:{first.id}:ledger")
    ]


def test_two_source_accounts_reuse_client_key_with_distinct_server_ledger_keys(
    db_session: Session,
) -> None:
    first_parent, first_child, first_actor, first_order = allocation_rows(db_session)
    second_parent, second_child, second_actor, second_order = allocation_rows(db_session)
    port = RecordingMovementPort()
    service = CreditMovementService(
        ResellerTreeAccess(), port, AuditWriter(FixedClock()), FixedClock()
    )
    shared_client_key = "allocation-shared-account-key"

    first = service.allocate(
        db_session,
        first_actor,
        CreditAllocationCreate(
            cash_order_id=first_order.id,
            credit_units=1_000,
            idempotency_key=shared_client_key,
        ),
    )
    second = service.allocate(
        db_session,
        second_actor,
        CreditAllocationCreate(
            cash_order_id=second_order.id,
            credit_units=1_000,
            idempotency_key=shared_client_key,
        ),
    )

    assert first.source_account_id == first_parent.id
    assert second.source_account_id == second_parent.id
    assert first.id != second.id
    assert db_session.scalar(
        select(func.count()).select_from(CreditMovementRequest).where(
            CreditMovementRequest.idempotency_key == shared_client_key
        )
    ) == 2
    assert port.calls == [
        (first_parent.id, first_child.id, 1_000, f"movement:{first.id}:ledger"),
        (second_parent.id, second_child.id, 1_000, f"movement:{second.id}:ledger"),
    ]
    assert port.calls[0][3] != port.calls[1][3]


def test_ledger_failure_rolls_back_request_in_caller_transaction(db_session: Session) -> None:
    _parent, _child, actor, order = allocation_rows(db_session)
    service = CreditMovementService(
        ResellerTreeAccess(),
        RecordingMovementPort(fail=True),
        AuditWriter(FixedClock()),
        FixedClock(),
    )
    with pytest.raises(Conflict, match="injected ledger failure"):
        with db_session.begin_nested():
            service.allocate(
                db_session,
                actor,
                CreditAllocationCreate(
                    cash_order_id=order.id,
                    credit_units=1_000,
                    idempotency_key="allocation-rollback-1",
                ),
            )
    assert (
        db_session.scalar(
            select(func.count())
            .select_from(CreditMovementRequest)
            .where(CreditMovementRequest.idempotency_key == "allocation-rollback-1")
        )
        == 0
    )
    assert db_session.scalar(select(func.count()).select_from(CreditLedgerEntry)) == 0
```

- [ ] **Step 2: Run the tests and verify missing port/service behavior**

Run: `cd backend && uv run pytest tests/integration/resellers/test_credit_movements.py -v`

Expected: FAIL during collection because `CreditMovementService` is missing.

- [ ] **Step 3: Define the narrow movement port**

Create `backend/src/ip_saas/modules/resellers/ports.py`:

```python
from typing import Protocol
from uuid import UUID

from sqlalchemy.orm import Session

from ip_saas.modules.accounts.context import ActorContext


class CreditMovementPort(Protocol):
    def post(
        self,
        session: Session,
        *,
        source_account_id: UUID,
        destination_account_id: UUID,
        credit_units: int,
        idempotency_key: str,
        actor: ActorContext,
    ) -> UUID:
        raise NotImplementedError

    def available(self, session: Session, account_id: UUID) -> int:
        raise NotImplementedError
```

- [ ] **Step 4: Adapt allocations and returns to Plan 01's exact double-entry service**

Create `backend/src/ip_saas/modules/resellers/adapters.py`:

```python
from uuid import UUID

from sqlalchemy.orm import Session

from ip_saas.common.errors import NotFound
from ip_saas.modules.accounts.context import ActorContext
from ip_saas.modules.accounts.models import Account, AccountKind
from ip_saas.modules.billing.repository import CreditLedgerRepository
from ip_saas.modules.billing.service import CreditLedgerService


class FoundationCreditMovementAdapter:
    def __init__(self) -> None:
        self.ledger = CreditLedgerService(CreditLedgerRepository())

    def post(
        self,
        session: Session,
        *,
        source_account_id: UUID,
        destination_account_id: UUID,
        credit_units: int,
        idempotency_key: str,
        actor: ActorContext,
    ) -> UUID:
        source = session.get(Account, source_account_id)
        destination = session.get(Account, destination_account_id)
        if source is None or destination is None:
            raise NotFound("credit movement account")
        treasury = self.ledger.ensure_system_wallet(
            session,
            "credit_treasury",
            allow_negative=True,
        )
        if source.kind == AccountKind.PLATFORM:
            destination_wallet = self.ledger.ensure_account_wallet(session, destination.id)
            transaction = self.ledger.issue(
                session,
                treasury.id,
                destination_wallet.id,
                credit_units,
                actor.actor_id,
                idempotency_key,
            )
        elif destination.kind == AccountKind.PLATFORM:
            source_wallet = self.ledger.ensure_account_wallet(session, source.id)
            transaction = self.ledger.transfer(
                session,
                source_wallet.id,
                treasury.id,
                credit_units,
                actor.actor_id,
                idempotency_key,
            )
        else:
            source_wallet = self.ledger.ensure_account_wallet(session, source.id)
            destination_wallet = self.ledger.ensure_account_wallet(session, destination.id)
            transaction = self.ledger.transfer(
                session,
                source_wallet.id,
                destination_wallet.id,
                credit_units,
                actor.actor_id,
                idempotency_key,
            )
        return transaction.id

    def available(self, session: Session, account_id: UUID) -> int:
        account = session.get(Account, account_id)
        if account is None:
            raise NotFound("account")
        if account.kind == AccountKind.PLATFORM:
            raise ValueError("platform treasury does not expose a reseller balance")
        wallet = self.ledger.ensure_account_wallet(session, account_id)
        return self.ledger.available_balance(session, wallet.id)
```

- [ ] **Step 5: Append atomic allocation and return services**

Append these imports and the class to `backend/src/ip_saas/modules/resellers/service.py`:

```python
from .enums import CreditMovementKind, CreditMovementStatus
from .models import CreditMovementRequest, ResellerPriceVersion
from .ports import CreditMovementPort
from .schemas import CreditAllocationCreate, CreditReturnCreate


class CreditMovementService:
    def __init__(
        self,
        tree: ResellerTreeAccess,
        movement: CreditMovementPort,
        audit: AuditWriter,
        clock: Clock,
    ) -> None:
        self.tree = tree
        self.movement = movement
        self.audit = audit
        self.clock = clock

    def _existing(
        self,
        session: Session,
        idempotency_key: str,
        expected: tuple[str, UUID, UUID, int, UUID | None, str | None],
    ) -> CreditMovementRequest | None:
        row = session.scalar(
            select(CreditMovementRequest).where(
                CreditMovementRequest.source_account_id == expected[1],
                CreditMovementRequest.idempotency_key == idempotency_key
            )
        )
        if row is None:
            return None
        actual = (
            row.kind,
            row.source_account_id,
            row.destination_account_id,
            row.credit_units,
            row.cash_order_id,
            row.reason,
        )
        if actual != expected:
            raise Conflict("credit-movement idempotency key has different input")
        return row

    def allocate(
        self,
        session: Session,
        actor: ActorContext,
        command: CreditAllocationCreate,
    ) -> CreditMovementRequest:
        order = session.scalar(
            select(OfflineCashOrder)
            .where(OfflineCashOrder.id == command.cash_order_id)
            .with_for_update()
        )
        if order is None:
            raise NotFound("cash order")
        expected = (
            CreditMovementKind.ALLOCATION,
            order.payee_account_id,
            order.payer_account_id,
            command.credit_units,
            order.id,
            None,
        )
        self.tree.require_actor_controls_source(session, actor, order.payee_account_id)
        existing = self._existing(session, command.idempotency_key, expected)
        if existing is not None:
            return existing
        self.tree.require_direct_relation(
            session,
            order.payee_account_id,
            order.payer_account_id,
            lock=True,
        )
        if order.status != CashOrderStatus.CONFIRMED or order.confirmed_amount_fen is None:
            raise Conflict("cash must be confirmed before credit allocation")
        prior = session.scalar(
            select(CreditMovementRequest.id).where(
                CreditMovementRequest.cash_order_id == order.id,
                CreditMovementRequest.kind == CreditMovementKind.ALLOCATION,
                CreditMovementRequest.status == CreditMovementStatus.POSTED,
            )
        )
        if prior is not None:
            raise Conflict("cash order already funded one allocation")
        price = session.get(ResellerPriceVersion, order.pricing_version_id)
        if price is None:
            raise NotFound("reseller price version")
        if (
            order.confirmed_amount_fen * price.package_credit_units
            != command.credit_units * price.package_price_fen
        ):
            raise Conflict("credit units do not match confirmed cash and frozen price")
        row = CreditMovementRequest(
            kind=CreditMovementKind.ALLOCATION,
            source_account_id=order.payee_account_id,
            destination_account_id=order.payer_account_id,
            credit_units=command.credit_units,
            cash_order_id=order.id,
            pricing_version_id=price.id,
            idempotency_key=command.idempotency_key,
            status=CreditMovementStatus.REQUESTED,
            reason=None,
            requested_by=actor.actor_id,
            decided_by=None,
            decided_at=None,
            ledger_transaction_id=None,
        )
        session.add(row)
        session.flush()
        row.ledger_transaction_id = self.movement.post(
            session,
            source_account_id=row.source_account_id,
            destination_account_id=row.destination_account_id,
            credit_units=row.credit_units,
            idempotency_key=f"movement:{row.id}:ledger",
            actor=actor,
        )
        row.status = CreditMovementStatus.POSTED
        row.decided_by = actor.actor_id
        row.decided_at = self.clock.now()
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="credit_allocation.posted",
            target_type="credit_movement_request",
            target_id=row.id,
            metadata={
                "cash_order_id": str(order.id),
                "credit_units": row.credit_units,
                "pricing_version_id": str(price.id),
            },
        )
        return row

    def request_return(
        self,
        session: Session,
        actor: ActorContext,
        command: CreditReturnCreate,
    ) -> CreditMovementRequest:
        self.tree.require_actor_controls_source(session, actor, command.source_account_id)
        parent = self.tree.parent_of(session, command.source_account_id)
        expected = (
            CreditMovementKind.RETURN,
            command.source_account_id,
            parent.id,
            command.credit_units,
            None,
            command.reason,
        )
        existing = self._existing(session, command.idempotency_key, expected)
        if existing is not None:
            return existing
        row = CreditMovementRequest(
            kind=CreditMovementKind.RETURN,
            source_account_id=command.source_account_id,
            destination_account_id=parent.id,
            credit_units=command.credit_units,
            cash_order_id=None,
            pricing_version_id=None,
            idempotency_key=command.idempotency_key,
            status=CreditMovementStatus.REQUESTED,
            reason=command.reason,
            requested_by=actor.actor_id,
            decided_by=None,
            decided_at=None,
            ledger_transaction_id=None,
        )
        session.add(row)
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="credit_return.requested",
            target_type="credit_movement_request",
            target_id=row.id,
            metadata={"credit_units": row.credit_units},
        )
        return row

    def approve_return(
        self,
        session: Session,
        actor: ActorContext,
        request_id: UUID,
    ) -> CreditMovementRequest:
        row = session.scalar(
            select(CreditMovementRequest)
            .where(CreditMovementRequest.id == request_id)
            .with_for_update()
        )
        if row is None:
            raise NotFound("credit return")
        if row.kind != CreditMovementKind.RETURN:
            raise Conflict("only return requests can be approved")
        if actor.account_id != row.destination_account_id:
            raise Forbidden("only the direct parent approves a return")
        self.tree.require_active_actor(session, actor)
        self.tree.require_direct_relation(
            session,
            row.destination_account_id,
            row.source_account_id,
            lock=True,
        )
        if row.status == CreditMovementStatus.POSTED:
            return row
        if row.status != CreditMovementStatus.REQUESTED:
            raise Conflict("return request is not pending")
        if self.movement.available(session, row.source_account_id) < row.credit_units:
            raise Conflict("insufficient available credits for return")
        row.ledger_transaction_id = self.movement.post(
            session,
            source_account_id=row.source_account_id,
            destination_account_id=row.destination_account_id,
            credit_units=row.credit_units,
            idempotency_key=f"movement:{row.id}:ledger",
            actor=actor,
        )
        row.status = CreditMovementStatus.POSTED
        row.decided_by = actor.actor_id
        row.decided_at = self.clock.now()
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="credit_return.posted",
            target_type="credit_movement_request",
            target_id=row.id,
            metadata={"credit_units": row.credit_units},
        )
        return row

    def reject_return(
        self,
        session: Session,
        actor: ActorContext,
        request_id: UUID,
        reason: str,
    ) -> CreditMovementRequest:
        row = session.scalar(
            select(CreditMovementRequest)
            .where(CreditMovementRequest.id == request_id)
            .with_for_update()
        )
        if row is None:
            raise NotFound("credit return")
        if (
            row.kind != CreditMovementKind.RETURN
            or row.status != CreditMovementStatus.REQUESTED
        ):
            raise Conflict("return request is not pending")
        if actor.account_id != row.destination_account_id:
            raise Forbidden("only the direct parent rejects a return")
        row.status = CreditMovementStatus.REJECTED
        row.reason = reason
        row.decided_by = actor.actor_id
        row.decided_at = self.clock.now()
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="credit_return.rejected",
            target_type="credit_movement_request",
            target_id=row.id,
            metadata={"reason": reason},
        )
        return row
```

- [ ] **Step 6: Run service tests and Plan 01 ledger regressions**

Run: `cd backend && uv run pytest tests/integration/resellers/test_credit_movements.py tests/integration/billing/test_credit_ledger.py tests/integration/billing/test_credit_concurrency.py -v`

Expected: PASS; an exact allocation replay posts once using `movement:{server_id}:ledger`, two unrelated source accounts persist the same client key under separate PostgreSQL uniqueness scopes and receive distinct server ledger keys, failure rolls back the request, and Plan 01 wallet locks still prevent overdraw.

- [ ] **Step 7: Commit atomic credit movements**

```bash
git add backend/src/ip_saas/modules/resellers/ports.py backend/src/ip_saas/modules/resellers/adapters.py backend/src/ip_saas/modules/resellers/service.py backend/tests/integration/resellers/test_credit_movements.py
git commit -m "feat: post atomic reseller credit movements"
```

### Task 5: Add customer-controlled, audited support access

**Files:**
- Create: `backend/src/ip_saas/modules/resellers/support.py`
- Test: `backend/tests/security/test_reseller_privacy.py`

- [ ] **Step 1: Write failing default-deny, scope, expiry, and revocation tests**

Create `backend/tests/security/test_reseller_privacy.py`:

```python
from datetime import UTC, datetime, timedelta
from uuid import uuid4

import pytest
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditEvent, AuditWriter
from ip_saas.common.errors import Forbidden, NotFound
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import Account, AccountKind, ResellerRelation
from ip_saas.modules.projects.access import ProjectAccessService
from ip_saas.modules.projects.service import ProjectService
from ip_saas.modules.resellers.access import ResellerTreeAccess
from ip_saas.modules.resellers.enums import SupportScope
from ip_saas.modules.resellers.schemas import SupportGrantCreate
from ip_saas.modules.resellers.support import SupportGrantService, SupportProjectAccessService


class MutableClock:
    def __init__(self) -> None:
        self.value = datetime(2026, 8, 24, 8, 0, tzinfo=UTC)

    def now(self) -> datetime:
        return self.value


def support_scenario(session: Session):
    reseller = Account(kind=AccountKind.RESELLER_L1, display_name="L1")
    customer = Account(kind=AccountKind.C_USER, display_name="Customer")
    session.add_all([reseller, customer])
    session.flush()
    session.add(
        ResellerRelation(
            parent_account_id=reseller.id,
            child_account_id=customer.id,
            created_by_principal_id=uuid4(),
            active=True,
        )
    )
    customer_actor = ActorContext(uuid4(), customer.id, ActorKind.C_USER)
    reseller_actor = ActorContext(uuid4(), reseller.id, ActorKind.RESELLER_L1)
    project = ProjectService(ProjectAccessService()).create(
        session,
        customer_actor,
        "Private project",
    )
    return reseller_actor, customer_actor, project


def test_support_is_default_deny_scope_bound_and_audited(db_session: Session) -> None:
    reseller_actor, customer_actor, project = support_scenario(db_session)
    clock = MutableClock()
    audit = AuditWriter(clock)
    grants = SupportGrantService(
        ProjectAccessService(), ResellerTreeAccess(), clock, audit
    )
    access = SupportProjectAccessService(ResellerTreeAccess(), clock, audit)
    with pytest.raises(NotFound):
        ProjectAccessService().require_viewer(db_session, reseller_actor, project.id)
    with pytest.raises(Forbidden):
        access.require_scope(
            db_session,
            reseller_actor,
            project.id,
            SupportScope.PROFILE,
        )
    grant = grants.create(
        db_session,
        customer_actor,
        SupportGrantCreate(
            project_id=project.id,
            reseller_account_id=reseller_actor.account_id,
            scopes=[SupportScope.PROFILE],
            expires_at=clock.now() + timedelta(days=1),
            reason="profile correction",
            idempotency_key="support-scope-1",
        ),
    )
    assert access.require_scope(
        db_session,
        reseller_actor,
        project.id,
        SupportScope.PROFILE,
    ).id == project.id
    with pytest.raises(Forbidden):
        access.require_scope(
            db_session,
            reseller_actor,
            project.id,
            SupportScope.MEDIA,
        )
    assert (
        db_session.scalar(
            select(func.count())
            .select_from(AuditEvent)
            .where(AuditEvent.action == "support_access.used")
        )
        == 1
    )
    grants.revoke(db_session, customer_actor, grant.id, "support complete")
    with pytest.raises(Forbidden):
        access.require_scope(
            db_session,
            reseller_actor,
            project.id,
            SupportScope.PROFILE,
        )


def test_expiry_closes_support_without_mutating_grant(db_session: Session) -> None:
    reseller_actor, customer_actor, project = support_scenario(db_session)
    clock = MutableClock()
    audit = AuditWriter(clock)
    grant = SupportGrantService(
        ProjectAccessService(), ResellerTreeAccess(), clock, audit
    ).create(
        db_session,
        customer_actor,
        SupportGrantCreate(
            project_id=project.id,
            reseller_account_id=reseller_actor.account_id,
            scopes=[SupportScope.CONTENT],
            expires_at=clock.now() + timedelta(hours=1),
            reason="script review",
            idempotency_key="support-expiry-1",
        ),
    )
    frozen_expiry = grant.expires_at
    clock.value += timedelta(hours=2)
    with pytest.raises(Forbidden):
        SupportProjectAccessService(
            ResellerTreeAccess(), clock, audit
        ).require_scope(
            db_session,
            reseller_actor,
            project.id,
            SupportScope.CONTENT,
        )
    assert grant.expires_at == frozen_expiry
```

- [ ] **Step 2: Run security tests and verify missing support services**

Run: `cd backend && uv run pytest tests/security/test_reseller_privacy.py -v`

Expected: FAIL during collection because `SupportGrantService` is missing.

- [ ] **Step 3: Implement grants, separate revocations, and explicit scoped reads**

Create `backend/src/ip_saas/modules/resellers/support.py`:

```python
from datetime import timedelta
from uuid import UUID

from sqlalchemy import exists, select
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.common.clock import Clock
from ip_saas.common.errors import Conflict, Forbidden, NotFound
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import AccountKind
from ip_saas.modules.projects.access import ProjectAccessService
from ip_saas.modules.projects.models import IPProject, ProjectOwnerType

from .access import ResellerTreeAccess
from .enums import SupportScope
from .models import SupportGrant, SupportGrantRevocation
from .schemas import SupportGrantCreate


MAX_SUPPORT_DURATION = timedelta(days=7)


class SupportGrantService:
    def __init__(
        self,
        projects: ProjectAccessService,
        tree: ResellerTreeAccess,
        clock: Clock,
        audit: AuditWriter,
    ) -> None:
        self.projects = projects
        self.tree = tree
        self.clock = clock
        self.audit = audit

    def create(
        self,
        session: Session,
        actor: ActorContext,
        command: SupportGrantCreate,
    ) -> SupportGrant:
        project = self.projects.require_editor(session, actor, command.project_id)
        if actor.kind != ActorKind.C_USER or project.owner_type != ProjectOwnerType.C_USER:
            raise Forbidden("only a C-user project owner grants support")
        existing = session.scalar(
            select(SupportGrant).where(
                SupportGrant.customer_account_id == actor.account_id,
                SupportGrant.idempotency_key == command.idempotency_key
            )
        )
        expected_scopes = sorted({scope.value for scope in command.scopes})
        if existing is not None:
            if (
                existing.project_id != command.project_id
                or existing.reseller_account_id != command.reseller_account_id
                or existing.scopes != expected_scopes
                or existing.expires_at != command.expires_at
                or existing.reason != command.reason
            ):
                raise Conflict("support-grant idempotency key has different input")
            return existing
        reseller = self.tree.require_active_account(
            session,
            command.reseller_account_id,
        )
        if reseller.kind not in {AccountKind.RESELLER_L1, AccountKind.RESELLER_L2}:
            raise Forbidden("support can be granted only to a reseller")
        if not self.tree.is_ancestor(
            session,
            reseller.id,
            actor.account_id,
        ):
            raise Forbidden("support reseller must be in the customer chain")
        now = self.clock.now()
        if command.expires_at <= now or command.expires_at > now + MAX_SUPPORT_DURATION:
            raise Conflict("support must expire within seven days")
        grant = SupportGrant(
            project_id=project.id,
            customer_account_id=actor.account_id,
            reseller_account_id=reseller.id,
            scopes=expected_scopes,
            reason=command.reason,
            expires_at=command.expires_at,
            granted_by=actor.actor_id,
            idempotency_key=command.idempotency_key,
        )
        session.add(grant)
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="support_grant.created",
            target_type="support_grant",
            target_id=grant.id,
            project_id=project.id,
            metadata={
                "reseller_account_id": str(reseller.id),
                "scopes": grant.scopes,
                "expires_at": grant.expires_at.isoformat(),
            },
        )
        return grant

    def revoke(
        self,
        session: Session,
        actor: ActorContext,
        grant_id: UUID,
        reason: str,
    ) -> SupportGrantRevocation:
        grant = session.get(SupportGrant, grant_id)
        if grant is None:
            raise NotFound("support grant")
        if actor.kind != ActorKind.C_USER or actor.account_id != grant.customer_account_id:
            raise Forbidden("only the customer revokes support")
        existing = session.scalar(
            select(SupportGrantRevocation).where(
                SupportGrantRevocation.grant_id == grant.id
            )
        )
        if existing is not None:
            return existing
        revocation = SupportGrantRevocation(
            grant_id=grant.id,
            revoked_by=actor.actor_id,
            reason=reason,
        )
        session.add(revocation)
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="support_grant.revoked",
            target_type="support_grant",
            target_id=grant.id,
            project_id=grant.project_id,
            metadata={"reason": reason},
        )
        return revocation


class SupportProjectAccessService:
    def __init__(
        self,
        tree: ResellerTreeAccess,
        clock: Clock,
        audit: AuditWriter,
    ) -> None:
        self.tree = tree
        self.clock = clock
        self.audit = audit

    def require_scope(
        self,
        session: Session,
        actor: ActorContext,
        project_id: UUID,
        scope: SupportScope,
    ) -> IPProject:
        if actor.kind not in {ActorKind.RESELLER_L1, ActorKind.RESELLER_L2}:
            raise Forbidden("reseller support role is required")
        self.tree.require_active_actor(session, actor)
        project = session.get(IPProject, project_id)
        if project is None or project.owner_type != ProjectOwnerType.C_USER:
            raise NotFound("project")
        if not self.tree.is_ancestor(
            session,
            actor.account_id,
            project.owner_account_id,
        ):
            raise Forbidden("project is outside the reseller chain")
        grant = session.scalar(
            select(SupportGrant).where(
                SupportGrant.project_id == project.id,
                SupportGrant.customer_account_id == project.owner_account_id,
                SupportGrant.reseller_account_id == actor.account_id,
                SupportGrant.expires_at > self.clock.now(),
                SupportGrant.scopes.contains([scope.value]),
                ~exists().where(SupportGrantRevocation.grant_id == SupportGrant.id),
            )
        )
        if grant is None:
            raise Forbidden("active support scope is required")
        self.audit.write(
            session,
            actor=actor,
            action="support_access.used",
            target_type="ip_project",
            target_id=project.id,
            project_id=project.id,
            metadata={"grant_id": str(grant.id), "scope": scope.value},
        )
        return project
```

No code is added to `ProjectAccessService.require_viewer` or `require_editor` for reseller support. Future content endpoints must call `SupportProjectAccessService.require_scope` explicitly and return only the resource family named by that scope.

- [ ] **Step 4: Run privacy and audit tests**

Run: `cd backend && uv run pytest tests/security/test_reseller_privacy.py tests/security/test_cross_tenant_access.py -v`

Expected: PASS; default project access remains `NotFound`, the named scope opens only that scope, and expiry/revocation closes it.

- [ ] **Step 5: Commit customer-controlled support**

```bash
git add backend/src/ip_saas/modules/resellers/support.py backend/tests/security/test_reseller_privacy.py
git commit -m "feat: add scoped reseller support access"
```

### Task 6: Suspend accounts and migrate C/L2 relationships without moving balances

**Files:**
- Create: `backend/src/ip_saas/modules/resellers/lifecycle.py`
- Modify: `backend/src/ip_saas/modules/accounts/service.py`
- Modify: `backend/src/ip_saas/modules/projects/access.py`
- Modify: `backend/src/ip_saas/modules/billing/adapters.py`
- Test: `backend/tests/integration/resellers/test_lifecycle.py`

- [ ] **Step 1: Write failing suspension and unique-relation migration tests**

Create `backend/tests/integration/resellers/test_lifecycle.py`:

```python
from datetime import UTC, datetime
from uuid import uuid4

import pytest
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.common.errors import Forbidden
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import (
    Account,
    AccountKind,
    AccountStatus,
    ResellerRelation,
)
from ip_saas.modules.billing.repository import CreditLedgerRepository
from ip_saas.modules.billing.service import CreditLedgerService
from ip_saas.modules.resellers.access import ResellerTreeAccess
from ip_saas.modules.resellers.lifecycle import ResellerLifecycleService
from ip_saas.modules.resellers.schemas import MigrationCreate


class FixedClock:
    def now(self) -> datetime:
        return datetime(2026, 8, 24, 8, 0, tzinfo=UTC)


def test_suspended_actor_cannot_start_commercial_work(db_session: Session) -> None:
    platform = Account(kind=AccountKind.PLATFORM, display_name="Platform")
    l1 = Account(kind=AccountKind.RESELLER_L1, display_name="L1")
    db_session.add_all([platform, l1])
    db_session.flush()
    db_session.add(
        ResellerRelation(
            parent_account_id=platform.id,
            child_account_id=l1.id,
            created_by_principal_id=uuid4(),
            active=True,
        )
    )
    admin = ActorContext(uuid4(), platform.id, ActorKind.PLATFORM_ADMIN)
    l1_actor = ActorContext(uuid4(), l1.id, ActorKind.RESELLER_L1)
    service = ResellerLifecycleService(
        ResellerTreeAccess(), FixedClock(), AuditWriter(FixedClock())
    )
    service.suspend(db_session, admin, l1.id, "contract ended")
    assert l1.status == AccountStatus.SUSPENDED
    with pytest.raises(Forbidden, match="suspended"):
        ResellerTreeAccess().require_active_actor(db_session, l1_actor)


def test_migration_repoints_one_relation_and_preserves_credit_balance(
    db_session: Session,
) -> None:
    old_l1 = Account(kind=AccountKind.RESELLER_L1, display_name="Old L1")
    new_l1 = Account(kind=AccountKind.RESELLER_L1, display_name="New L1")
    customer = Account(kind=AccountKind.C_USER, display_name="C")
    db_session.add_all([old_l1, new_l1, customer])
    db_session.flush()
    relation = ResellerRelation(
        parent_account_id=old_l1.id,
        child_account_id=customer.id,
        created_by_principal_id=uuid4(),
        active=True,
    )
    db_session.add(relation)
    db_session.flush()
    ledger = CreditLedgerService(CreditLedgerRepository())
    wallet = ledger.ensure_account_wallet(db_session, customer.id)
    wallet.posted_balance = 321
    customer_actor = ActorContext(uuid4(), customer.id, ActorKind.C_USER)
    admin = ActorContext(uuid4(), uuid4(), ActorKind.PLATFORM_ADMIN)
    service = ResellerLifecycleService(
        ResellerTreeAccess(), FixedClock(), AuditWriter(FixedClock())
    )
    request = service.request_migration(
        db_session,
        customer_actor,
        MigrationCreate(
            subject_account_id=customer.id,
            new_parent_account_id=new_l1.id,
            reason="old reseller stopped operating",
        ),
    )
    service.approve_migration(db_session, admin, request.id)
    service.execute_migration(db_session, admin, request.id)

    assert relation.parent_account_id == new_l1.id
    assert wallet.posted_balance == 321
    assert (
        db_session.scalar(
            select(func.count())
            .select_from(ResellerRelation)
            .where(ResellerRelation.child_account_id == customer.id)
        )
        == 1
    )
    assert request.old_parent_account_id == old_l1.id
    assert request.new_parent_account_id == new_l1.id
    assert request.status == "completed"
```

- [ ] **Step 2: Run tests and verify missing lifecycle service**

Run: `cd backend && uv run pytest tests/integration/resellers/test_lifecycle.py -v`

Expected: FAIL during collection because `ResellerLifecycleService` is missing.

- [ ] **Step 3: Reject suspended accounts at Plan 01 authentication and child creation**

In `backend/src/ip_saas/modules/accounts/service.py`, include `AccountStatus` in the existing model imports and add these exact guards:

```python
# In AccountService.create_child_account(), immediately after loading parent:
if parent.status != AccountStatus.ACTIVE:
    raise Forbidden("parent account is suspended")

# In _actor_for(), immediately after loading account:
if account is None or account.status != AccountStatus.ACTIVE:
    raise Unauthenticated("account unavailable")
```

This keeps the existing `create_child_account(session, actor, parent_account_id, child_kind, display_name)` signature unchanged.

- [ ] **Step 4: Reject stale suspended actors at project and credit-hold boundaries**

Add `Account` and `AccountStatus` imports to `backend/src/ip_saas/modules/projects/access.py`. In `ProjectAccessService.require_viewer`, after the existing project-not-found check and before owner checks, add:

```python
actor_account = session.get(Account, actor.account_id)
if actor_account is None or actor_account.status != AccountStatus.ACTIVE:
    raise Forbidden("account is suspended")
```

Add `Account`, `AccountStatus`, and `Forbidden` imports to `backend/src/ip_saas/modules/billing/adapters.py`. At the start of `SqlCreditHoldPort.reserve`, before wallet creation, add:

```python
account = session.get(Account, account_id)
if account is None or account.status != AccountStatus.ACTIVE:
    raise Forbidden("account is suspended")
```

The frozen signatures of `ProjectAccessService.require_viewer`, `require_editor`, and all four `BillingService` generation methods do not change.

- [ ] **Step 5: Implement suspension, subject consent, approval, and atomic relation repointing**

Create `backend/src/ip_saas/modules/resellers/lifecycle.py`:

```python
from uuid import UUID

from sqlalchemy import select, update
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.common.clock import Clock
from ip_saas.common.errors import Conflict, Forbidden, NotFound
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import (
    Account,
    AccountKind,
    AccountStatus,
    AuthSession,
    Principal,
    ResellerRelation,
)
from ip_saas.modules.projects.models import IPProject

from .access import ResellerTreeAccess
from .enums import MigrationStatus
from .models import (
    AccountMigrationRequest,
    SupportGrant,
    SupportGrantRevocation,
)
from .schemas import MigrationCreate


ALLOWED_MIGRATION_PARENTS = {
    AccountKind.RESELLER_L2: {AccountKind.RESELLER_L1},
    AccountKind.C_USER: {AccountKind.RESELLER_L1, AccountKind.RESELLER_L2},
}


class ResellerLifecycleService:
    def __init__(
        self,
        tree: ResellerTreeAccess,
        clock: Clock,
        audit: AuditWriter,
    ) -> None:
        self.tree = tree
        self.clock = clock
        self.audit = audit

    def suspend(
        self,
        session: Session,
        actor: ActorContext,
        account_id: UUID,
        reason: str,
    ) -> Account:
        account = session.scalar(
            select(Account).where(Account.id == account_id).with_for_update()
        )
        if account is None:
            raise NotFound("account")
        if account.kind == AccountKind.PLATFORM:
            raise Forbidden("platform account cannot be suspended here")
        if actor.kind != ActorKind.PLATFORM_ADMIN:
            self.tree.require_direct_relation(session, actor.account_id, account.id)
            self.tree.require_active_actor(session, actor)
        if account.status == AccountStatus.SUSPENDED:
            return account
        account.status = AccountStatus.SUSPENDED
        principal_ids = select(Principal.id).where(Principal.account_id == account.id)
        session.execute(
            update(AuthSession)
            .where(
                AuthSession.principal_id.in_(principal_ids),
                AuthSession.revoked_at.is_(None),
            )
            .values(revoked_at=self.clock.now())
        )
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="account.suspended",
            target_type="account",
            target_id=account.id,
            metadata={"reason": reason},
        )
        return account

    def request_migration(
        self,
        session: Session,
        actor: ActorContext,
        command: MigrationCreate,
    ) -> AccountMigrationRequest:
        subject = self.tree.require_active_actor(session, actor)
        if subject.id != command.subject_account_id:
            raise Forbidden("the subject account must request its own migration")
        subject_kind = AccountKind(subject.kind)
        if subject_kind not in ALLOWED_MIGRATION_PARENTS:
            raise Forbidden("only C or L2 accounts can migrate")
        old_parent = self.tree.parent_of(session, subject.id)
        new_parent = self.tree.require_active_account(
            session,
            command.new_parent_account_id,
        )
        if AccountKind(new_parent.kind) not in ALLOWED_MIGRATION_PARENTS[subject_kind]:
            raise Forbidden("new parent would violate the fixed hierarchy")
        if old_parent.id == new_parent.id:
            raise Conflict("new parent must differ from old parent")
        request = AccountMigrationRequest(
            subject_account_id=subject.id,
            old_parent_account_id=old_parent.id,
            new_parent_account_id=new_parent.id,
            reason=command.reason,
            status=MigrationStatus.REQUESTED,
            requested_by=actor.actor_id,
            approved_by=None,
            approved_at=None,
            rejected_by=None,
            rejection_reason=None,
            completed_by=None,
            completed_at=None,
        )
        session.add(request)
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="account_migration.requested",
            target_type="account_migration_request",
            target_id=request.id,
            metadata={
                "old_parent_account_id": str(old_parent.id),
                "new_parent_account_id": str(new_parent.id),
            },
        )
        return request

    def approve_migration(
        self,
        session: Session,
        actor: ActorContext,
        request_id: UUID,
    ) -> AccountMigrationRequest:
        if actor.kind != ActorKind.PLATFORM_ADMIN:
            raise Forbidden("platform admin is required")
        request = session.scalar(
            select(AccountMigrationRequest)
            .where(AccountMigrationRequest.id == request_id)
            .with_for_update()
        )
        if request is None:
            raise NotFound("account migration")
        if request.status == MigrationStatus.APPROVED:
            return request
        if request.status != MigrationStatus.REQUESTED:
            raise Conflict("migration is not awaiting approval")
        request.status = MigrationStatus.APPROVED
        request.approved_by = actor.actor_id
        request.approved_at = self.clock.now()
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="account_migration.approved",
            target_type="account_migration_request",
            target_id=request.id,
        )
        return request

    def execute_migration(
        self,
        session: Session,
        actor: ActorContext,
        request_id: UUID,
    ) -> AccountMigrationRequest:
        if actor.kind != ActorKind.PLATFORM_ADMIN:
            raise Forbidden("platform admin is required")
        request = session.scalar(
            select(AccountMigrationRequest)
            .where(AccountMigrationRequest.id == request_id)
            .with_for_update()
        )
        if request is None:
            raise NotFound("account migration")
        if request.status == MigrationStatus.COMPLETED:
            return request
        if request.status != MigrationStatus.APPROVED:
            raise Conflict("platform approval is required")
        subject = self.tree.require_active_account(session, request.subject_account_id)
        new_parent = self.tree.require_active_account(
            session,
            request.new_parent_account_id,
        )
        allowed = ALLOWED_MIGRATION_PARENTS[AccountKind(subject.kind)]
        if AccountKind(new_parent.kind) not in allowed:
            raise Conflict("new parent no longer satisfies the fixed hierarchy")
        relation = session.scalar(
            select(ResellerRelation)
            .where(
                ResellerRelation.child_account_id == subject.id,
                ResellerRelation.active.is_(True),
            )
            .with_for_update()
        )
        if relation is None or relation.parent_account_id != request.old_parent_account_id:
            raise Conflict("commercial relation changed after the request")
        relation.parent_account_id = new_parent.id
        self._revoke_old_parent_grants(
            session,
            actor,
            subject,
            request.old_parent_account_id,
        )
        request.status = MigrationStatus.COMPLETED
        request.completed_by = actor.actor_id
        request.completed_at = self.clock.now()
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="account_migration.completed",
            target_type="account_migration_request",
            target_id=request.id,
            metadata={
                "subject_account_id": str(subject.id),
                "credits_moved": 0,
            },
        )
        return request

    def _revoke_old_parent_grants(
        self,
        session: Session,
        actor: ActorContext,
        subject: Account,
        old_parent_account_id: UUID,
    ) -> None:
        if subject.kind == AccountKind.C_USER:
            customer_ids = [subject.id]
        else:
            customer_ids = list(
                session.scalars(
                    select(ResellerRelation.child_account_id)
                    .join(Account, Account.id == ResellerRelation.child_account_id)
                    .where(
                        ResellerRelation.parent_account_id == subject.id,
                        ResellerRelation.active.is_(True),
                        Account.kind == AccountKind.C_USER,
                    )
                )
            )
        grants = list(
            session.scalars(
                select(SupportGrant).where(
                    SupportGrant.customer_account_id.in_(customer_ids),
                    SupportGrant.reseller_account_id == old_parent_account_id,
                    ~select(SupportGrantRevocation.id)
                    .where(SupportGrantRevocation.grant_id == SupportGrant.id)
                    .exists(),
                )
            )
        )
        for grant in grants:
            session.add(
                SupportGrantRevocation(
                    grant_id=grant.id,
                    revoked_by=actor.actor_id,
                    reason="commercial relationship migrated",
                )
            )
```

The migration updates the one existing relation row because Plan 01 freezes a unique constraint on `ResellerRelation.child_account_id`. It does not deactivate-and-insert a second row. Old/new ownership remains immutable in `AccountMigrationRequest` and audit; cash orders, price versions, projects, credit entries, and support-grant rows retain original IDs.

- [ ] **Step 6: Run lifecycle, hierarchy, auth, project, and billing regressions**

Run: `cd backend && uv run pytest tests/integration/resellers/test_lifecycle.py tests/integration/accounts/test_hierarchy_and_roles.py tests/integration/accounts/test_auth.py tests/integration/projects/test_project_isolation.py tests/integration/billing/test_internal_costs.py -v`

Expected: PASS; suspended accounts cannot reauthenticate or reserve new customer holds, descendants remain unchanged, and migration retains one relation and the same wallet balance.

- [ ] **Step 7: Commit suspension and migration**

```bash
git add backend/src/ip_saas/modules/resellers/lifecycle.py backend/src/ip_saas/modules/accounts/service.py backend/src/ip_saas/modules/projects/access.py backend/src/ip_saas/modules/billing/adapters.py backend/tests/integration/resellers/test_lifecycle.py
git commit -m "feat: suspend and migrate reseller accounts"
```

### Task 7: Enforce versioned per-task, daily, and monthly generation limits

**Files:**
- Modify: `backend/src/ip_saas/modules/billing/models.py`
- Modify: `backend/src/ip_saas/modules/billing/limits.py`
- Modify: `backend/src/ip_saas/modules/billing/service.py`
- Create: `backend/src/ip_saas/modules/billing/composition.py`
- Modify: `backend/src/ip_saas/api.py`
- Create: `backend/tests/integration/billing/test_generation_limits.py`
- Create: `backend/tests/integration/billing/test_generation_limit_concurrency.py`
- Create: `backend/tests/contract/billing/test_generation_limit_composition.py`
- Create: `backend/tests/fixtures/__init__.py`
- Create: `backend/tests/fixtures/generation_limits.py`
- Modify: `backend/tests/integration/billing/test_internal_costs.py`
- Modify: `backend/tests/integration/tasks/test_task_submission.py`
- Modify: `backend/tests/integration/intelligence/conftest.py`
- Modify: `backend/tests/integration/intelligence/test_api_workflow.py`
- Modify: `backend/tests/integration/intelligence/test_self_marketing_isolation.py`
- Modify: `backend/tests/integration/media/conftest.py`
- Modify: `backend/tests/integration/media/harness.py`
- Modify: `backend/tests/support/publication_fixtures.py`
- Modify: `backend/tests/integration/publication/test_billed_failure_recovery.py`

- [ ] **Step 1: Write deterministic failing boundary tests**

Create `backend/tests/integration/billing/test_generation_limits.py`:

```python
from datetime import UTC, datetime, timedelta
from uuid import UUID, uuid4

import pytest
from sqlalchemy.orm import Session

from ip_saas.common.errors import Conflict
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import Account, AccountKind
from ip_saas.modules.billing.adapters import SqlCreditHoldPort
from ip_saas.modules.billing.limits import GenerationLimitService
from ip_saas.modules.billing.models import (
    CreditWallet,
    GenerationHold,
    HoldStatus,
    InternalBudgetHold,
    InternalCostCenter,
)
from ip_saas.modules.billing.service import BillingService


class FixedClock:
    def __init__(self, instant: datetime | None = None) -> None:
        self.instant = instant or datetime(2026, 8, 24, 8, 0, tzinfo=UTC)

    def now(self) -> datetime:
        return self.instant


class RecordingCreditHolds:
    def __init__(self) -> None:
        self.calls: list[int] = []

    def reserve(
        self,
        session: Session,
        account_id: UUID,
        credit_units: int,
        idempotency_key: str,
        actor: ActorContext,
    ) -> UUID:
        del session, account_id, idempotency_key, actor
        self.calls.append(credit_units)
        return uuid4()

    def settle(
        self,
        session: Session,
        hold_id: UUID,
        actual_units: int,
        idempotency_key: str,
        actor: ActorContext | None,
    ) -> None:
        del session, hold_id, actual_units, idempotency_key, actor

    def release(
        self,
        session: Session,
        hold_id: UUID,
        reason: str,
        idempotency_key: str,
    ) -> None:
        del session, hold_id, reason, idempotency_key


class UnusedInternalHolds:
    def reserve(
        self,
        session: Session,
        cost_center_id: UUID,
        amount_fen: int,
        idempotency_key: str,
        actor: ActorContext,
    ) -> UUID:
        del session, cost_center_id, amount_fen, idempotency_key, actor
        raise AssertionError("internal reserve was not expected")

    def settle(
        self,
        session: Session,
        hold_id: UUID,
        actual_fen: int,
        idempotency_key: str,
    ) -> None:
        del session, hold_id, actual_fen, idempotency_key

    def release(
        self,
        session: Session,
        hold_id: UUID,
        reason: str,
        idempotency_key: str,
    ) -> None:
        del session, hold_id, reason, idempotency_key


def test_customer_limits_reject_before_a_hold_is_created(db_session: Session) -> None:
    account = Account(kind=AccountKind.C_USER, display_name="Limited C")
    db_session.add(account)
    db_session.flush()
    wallet = CreditWallet(
        account_id=account.id,
        system_code=None,
        allow_negative=False,
        posted_balance=10_000,
    )
    db_session.add(wallet)
    db_session.flush()
    admin = ActorContext(uuid4(), uuid4(), ActorKind.PLATFORM_ADMIN)
    actor = ActorContext(uuid4(), account.id, ActorKind.C_USER)
    limits = GenerationLimitService(FixedClock())
    limits.create_account_version(
        db_session,
        admin,
        account.id,
        per_task_units=100,
        per_day_units=150,
        per_month_units=250,
        effective_at=FixedClock().now(),
    )
    db_session.add(
        GenerationHold(
            wallet_id=wallet.id,
            initiated_by_actor_id=actor.actor_id,
            pricing_version_id=None,
            amount_units=60,
            settled_units=None,
            status=HoldStatus.ACTIVE,
            idempotency_key="existing-limit-hold",
            settled_transaction_id=None,
            release_reason=None,
        )
    )
    db_session.flush()
    credit_holds = RecordingCreditHolds()
    billing = BillingService(credit_holds, UnusedInternalHolds(), limits)

    with pytest.raises(Conflict, match="daily generation limit"):
        billing.reserve_customer_generation(
            db_session,
            account.id,
            100,
            "blocked-by-daily-limit",
            actor,
        )
    assert credit_holds.calls == []


def test_missing_limit_configuration_fails_closed(db_session: Session) -> None:
    account = Account(kind=AccountKind.RESELLER_L1, display_name="Unconfigured L1")
    db_session.add(account)
    db_session.flush()
    actor = ActorContext(uuid4(), account.id, ActorKind.RESELLER_L1)
    billing = BillingService(
        RecordingCreditHolds(),
        UnusedInternalHolds(),
        GenerationLimitService(FixedClock()),
    )
    with pytest.raises(Conflict, match="not configured"):
        billing.reserve_customer_generation(
            db_session,
            account.id,
            1,
            "missing-limit",
            actor,
        )


def test_missing_internal_limit_configuration_fails_before_hold(
    db_session: Session,
) -> None:
    platform = Account(kind=AccountKind.PLATFORM, display_name="Unconfigured platform")
    db_session.add(platform)
    db_session.flush()
    center = InternalCostCenter(
        platform_account_id=platform.id,
        code="unconfigured_internal",
        display_name="Unconfigured internal",
        active=True,
    )
    db_session.add(center)
    db_session.flush()
    actor = ActorContext(uuid4(), platform.id, ActorKind.PLATFORM_OPERATOR)
    billing = BillingService(
        RecordingCreditHolds(),
        UnusedInternalHolds(),
        GenerationLimitService(FixedClock()),
    )
    with pytest.raises(Conflict, match="not configured"):
        billing.reserve_internal_generation(
            db_session,
            center.id,
            1,
            "missing-internal-limit",
            actor,
        )


def test_single_task_and_monthly_limits_have_distinct_errors(db_session: Session) -> None:
    account = Account(kind=AccountKind.C_USER, display_name="Boundary C")
    db_session.add(account)
    db_session.flush()
    wallet = CreditWallet(
        account_id=account.id,
        system_code=None,
        allow_negative=False,
        posted_balance=10_000,
    )
    db_session.add(wallet)
    db_session.flush()
    admin = ActorContext(uuid4(), uuid4(), ActorKind.PLATFORM_ADMIN)
    limits = GenerationLimitService(FixedClock())
    limits.create_account_version(
        db_session,
        admin,
        account.id,
        per_task_units=100,
        per_day_units=200,
        per_month_units=300,
        effective_at=FixedClock().now(),
    )
    with pytest.raises(Conflict, match="single-task"):
        limits.require_customer_reservation(db_session, account.id, 101)
    db_session.add(
        GenerationHold(
            wallet_id=wallet.id,
            initiated_by_actor_id=uuid4(),
            pricing_version_id=None,
            amount_units=250,
            settled_units=250,
            status=HoldStatus.SETTLED,
            idempotency_key="prior-day-settled",
            settled_transaction_id=None,
            release_reason=None,
            created_at=datetime(2026, 8, 23, 8, 0, tzinfo=UTC),
        )
    )
    db_session.flush()
    with pytest.raises(Conflict, match="monthly generation limit"):
        limits.require_customer_reservation(db_session, account.id, 60)


def test_internal_cost_center_daily_limit_is_in_fen(db_session: Session) -> None:
    platform = Account(kind=AccountKind.PLATFORM, display_name="Platform limits")
    db_session.add(platform)
    db_session.flush()
    center = InternalCostCenter(
        platform_account_id=platform.id,
        code="self_marketing_limits",
        display_name="Self marketing limits",
        active=True,
    )
    db_session.add(center)
    db_session.flush()
    admin = ActorContext(uuid4(), platform.id, ActorKind.PLATFORM_ADMIN)
    limits = GenerationLimitService(FixedClock())
    limits.create_internal_version(
        db_session,
        admin,
        center.id,
        per_task_fen=1_000,
        per_day_fen=2_000,
        per_month_fen=3_000,
        effective_at=FixedClock().now(),
    )
    db_session.add(
        InternalBudgetHold(
            cost_center_id=center.id,
            initiated_by_actor_id=admin.actor_id,
            pricing_version_id=None,
            amount_fen=1_500,
            settled_fen=None,
            status=HoldStatus.ACTIVE,
            idempotency_key="internal-existing-hold",
            release_reason=None,
        )
    )
    db_session.flush()
    with pytest.raises(Conflict, match="daily generation limit"):
        limits.require_internal_reservation(db_session, center.id, 600)


def test_exact_customer_cap_creates_hold_and_cap_plus_one_is_rejected(
    db_session: Session,
) -> None:
    account = Account(kind=AccountKind.C_USER, display_name="Exact-cap C")
    db_session.add(account)
    db_session.flush()
    wallet = CreditWallet(
        account_id=account.id,
        system_code=None,
        allow_negative=False,
        posted_balance=10_000,
    )
    db_session.add(wallet)
    db_session.flush()
    admin = ActorContext(uuid4(), account.id, ActorKind.PLATFORM_ADMIN)
    actor = ActorContext(uuid4(), account.id, ActorKind.C_USER)
    limits = GenerationLimitService(FixedClock())
    limits.create_account_version(
        db_session,
        admin,
        account.id,
        per_task_units=100,
        per_day_units=150,
        per_month_units=250,
        effective_at=FixedClock().now(),
    )
    db_session.add(GenerationHold(
        wallet_id=wallet.id,
        initiated_by_actor_id=actor.actor_id,
        pricing_version_id=None,
        amount_units=50,
        settled_units=None,
        status=HoldStatus.ACTIVE,
        idempotency_key="exact-cap-existing",
        settled_transaction_id=None,
        release_reason=None,
    ))
    db_session.flush()
    billing = BillingService(
        SqlCreditHoldPort(), UnusedInternalHolds(), limits
    )
    billing.reserve_customer_generation(
        db_session, account.id, 100, "exact-cap-new", actor
    )
    with pytest.raises(Conflict, match="daily generation limit"):
        billing.reserve_customer_generation(
            db_session, account.id, 1, "cap-plus-one", actor
        )


def test_exposure_uses_active_reservation_and_settled_actual_only(
    db_session: Session,
) -> None:
    instant = datetime(2026, 8, 31, 16, 30, tzinfo=UTC)
    clock = FixedClock(instant)
    account = Account(kind=AccountKind.C_USER, display_name="Shanghai boundary")
    db_session.add(account)
    db_session.flush()
    wallet = CreditWallet(
        account_id=account.id,
        system_code=None,
        allow_negative=False,
        posted_balance=10_000,
    )
    db_session.add(wallet)
    db_session.flush()
    limits = GenerationLimitService(clock)
    limits.create_account_version(
        db_session,
        ActorContext(uuid4(), account.id, ActorKind.PLATFORM_ADMIN),
        account.id,
        per_task_units=100,
        per_day_units=100,
        per_month_units=300,
        effective_at=instant - timedelta(days=30),
    )
    for key, created_at, status, reserved, settled in (
        ("before-shanghai-month", instant - timedelta(minutes=31),
         HoldStatus.SETTLED, 500, 500),
        ("active-in-period", instant - timedelta(minutes=20),
         HoldStatus.ACTIVE, 40, None),
        ("settled-in-period", instant - timedelta(minutes=10),
         HoldStatus.SETTLED, 90, 30),
        ("released-in-period", instant - timedelta(minutes=5),
         HoldStatus.RELEASED, 999, None),
    ):
        db_session.add(GenerationHold(
            wallet_id=wallet.id,
            initiated_by_actor_id=uuid4(),
            pricing_version_id=None,
            amount_units=reserved,
            settled_units=settled,
            status=status,
            idempotency_key=key,
            settled_transaction_id=None,
            release_reason="released" if status == HoldStatus.RELEASED else None,
            created_at=created_at,
        ))
    db_session.flush()
    status = limits.customer_status(db_session, account.id)
    assert status.used_day_amount == 70
    assert status.used_month_amount == 70
    limits.require_customer_reservation(db_session, account.id, 30)
    with pytest.raises(Conflict, match="daily generation limit"):
        limits.require_customer_reservation(db_session, account.id, 31)


def test_internal_exposure_is_active_plus_settled_actual_and_not_released(
    db_session: Session,
) -> None:
    platform = Account(kind=AccountKind.PLATFORM, display_name="Internal exposure")
    db_session.add(platform)
    db_session.flush()
    center = InternalCostCenter(
        platform_account_id=platform.id,
        code="internal_exposure",
        display_name="Internal exposure",
        active=True,
    )
    db_session.add(center)
    db_session.flush()
    limits = GenerationLimitService(FixedClock())
    limits.create_internal_version(
        db_session,
        ActorContext(uuid4(), platform.id, ActorKind.PLATFORM_ADMIN),
        center.id,
        per_task_fen=100,
        per_day_fen=100,
        per_month_fen=300,
        effective_at=FixedClock().now(),
    )
    for key, state, reserved, settled in (
        ("internal-active", HoldStatus.ACTIVE, 40, None),
        ("internal-settled", HoldStatus.SETTLED, 80, 20),
        ("internal-released", HoldStatus.RELEASED, 900, None),
    ):
        db_session.add(InternalBudgetHold(
            cost_center_id=center.id,
            initiated_by_actor_id=uuid4(),
            pricing_version_id=None,
            amount_fen=reserved,
            settled_fen=settled,
            status=state,
            idempotency_key=key,
            release_reason="released" if state == HoldStatus.RELEASED else None,
        ))
    db_session.flush()
    status = limits.internal_status(db_session, center.id)
    assert status.used_day_amount == 60
    assert status.used_month_amount == 60
    limits.require_internal_reservation(db_session, center.id, 40)
    with pytest.raises(Conflict, match="daily generation limit"):
        limits.require_internal_reservation(db_session, center.id, 41)
```

- [ ] **Step 2: Run tests and verify the database-backed service is not implemented yet**

Run: `cd backend && uv run pytest tests/integration/billing/test_generation_limits.py -v`

Expected: FAIL during collection because Plan 01's existing `ip_saas.modules.billing.limits` exports only `UnconfiguredGenerationLimits`; `GenerationLimitService` and `GenerationLimitVersion` do not exist yet. The failure must not claim that the module itself is missing.

- [ ] **Step 3: Add immutable account/cost-center limit versions**

Append to `backend/src/ip_saas/modules/billing/models.py`:

```python
class GenerationLimitVersion(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "generation_limit_versions"
    __table_args__ = (
        CheckConstraint(
            "(account_id is not null and cost_center_id is null and billing_mode = 'customer_credit') "
            "or (account_id is null and cost_center_id is not null and billing_mode = 'internal_cost')",
            name="ck_generation_limit_one_subject",
        ),
        CheckConstraint(
            "per_task_amount > 0 and per_day_amount >= per_task_amount "
            "and per_month_amount >= per_day_amount",
            name="ck_generation_limit_ordered_caps",
        ),
        Index(
            "uq_generation_limit_account_version",
            "account_id",
            "version_no",
            unique=True,
            postgresql_where=text("account_id is not null"),
        ),
        Index(
            "uq_generation_limit_center_version",
            "cost_center_id",
            "version_no",
            unique=True,
            postgresql_where=text("cost_center_id is not null"),
        ),
    )
    account_id: Mapped[UUID | None] = mapped_column(ForeignKey("accounts.id"), index=True)
    cost_center_id: Mapped[UUID | None] = mapped_column(
        ForeignKey("internal_cost_centers.id"),
        index=True,
    )
    billing_mode: Mapped[str] = mapped_column(String(24))
    version_no: Mapped[int]
    per_task_amount: Mapped[int] = mapped_column(BigInteger)
    per_day_amount: Mapped[int] = mapped_column(BigInteger)
    per_month_amount: Mapped[int] = mapped_column(BigInteger)
    effective_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), index=True)
    created_by_principal_id: Mapped[UUID]
```

Add `DateTime` and `text` to the existing SQLAlchemy imports in that file. Amounts are credit units for `customer_credit` and人民币分 for `internal_cost`; a row can never mix the two subjects or units.

- [ ] **Step 4: Extend Plan 01's fail-closed module with locked database limits**

Modify `backend/src/ip_saas/modules/billing/limits.py`. Preserve the existing public `UnconfiguredGenerationLimits` class exactly; extend the same module rather than recreating or overwriting it:

```python
from dataclasses import dataclass
from datetime import UTC, datetime
from uuid import UUID
from zoneinfo import ZoneInfo

from sqlalchemy import case, func, select, text
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.common.clock import Clock
from ip_saas.common.errors import Conflict, Forbidden
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.billing.models import (
    BillingMode,
    CreditWallet,
    GenerationHold,
    GenerationLimitVersion,
    HoldStatus,
    InternalBudgetHold,
)


SHANGHAI = ZoneInfo("Asia/Shanghai")


class UnconfiguredGenerationLimits:
    def require_customer_reservation(
        self, session: Session, account_id: UUID, requested_units: int
    ) -> object:
        del session, account_id, requested_units
        raise Conflict("generation limit is not configured")

    def require_internal_reservation(
        self, session: Session, cost_center_id: UUID, requested_fen: int
    ) -> object:
        del session, cost_center_id, requested_fen
        raise Conflict("generation limit is not configured")


@dataclass(frozen=True)
class GenerationLimitStatus:
    version_id: UUID
    billing_mode: str
    per_task_amount: int
    per_day_amount: int
    per_month_amount: int
    used_day_amount: int
    used_month_amount: int


class GenerationLimitService:
    def __init__(self, clock: Clock) -> None:
        self.clock = clock
        self.audit = AuditWriter(clock)

    def create_account_version(
        self,
        session: Session,
        actor: ActorContext,
        account_id: UUID,
        *,
        per_task_units: int,
        per_day_units: int,
        per_month_units: int,
        effective_at: datetime,
    ) -> GenerationLimitVersion:
        return self._create(
            session,
            actor,
            account_id=account_id,
            cost_center_id=None,
            billing_mode=BillingMode.CUSTOMER_CREDIT,
            per_task=per_task_units,
            per_day=per_day_units,
            per_month=per_month_units,
            effective_at=effective_at,
        )

    def create_internal_version(
        self,
        session: Session,
        actor: ActorContext,
        cost_center_id: UUID,
        *,
        per_task_fen: int,
        per_day_fen: int,
        per_month_fen: int,
        effective_at: datetime,
    ) -> GenerationLimitVersion:
        return self._create(
            session,
            actor,
            account_id=None,
            cost_center_id=cost_center_id,
            billing_mode=BillingMode.INTERNAL_COST,
            per_task=per_task_fen,
            per_day=per_day_fen,
            per_month=per_month_fen,
            effective_at=effective_at,
        )

    def _create(
        self,
        session: Session,
        actor: ActorContext,
        *,
        account_id: UUID | None,
        cost_center_id: UUID | None,
        billing_mode: BillingMode,
        per_task: int,
        per_day: int,
        per_month: int,
        effective_at: datetime,
    ) -> GenerationLimitVersion:
        if actor.kind != ActorKind.PLATFORM_ADMIN:
            raise Forbidden("platform admin is required")
        if per_task <= 0 or per_day < per_task or per_month < per_day:
            raise Conflict("generation limits must satisfy task <= day <= month")
        subject = account_id or cost_center_id
        if subject is None:
            raise Conflict("generation limit subject is required")
        self._lock_reservation_subject(session, billing_mode, subject)
        subject_filter = (
            GenerationLimitVersion.account_id == account_id
            if account_id is not None
            else GenerationLimitVersion.cost_center_id == cost_center_id
        )
        latest = session.scalar(
            select(func.coalesce(func.max(GenerationLimitVersion.version_no), 0)).where(
                subject_filter
            )
        )
        row = GenerationLimitVersion(
            account_id=account_id,
            cost_center_id=cost_center_id,
            billing_mode=billing_mode,
            version_no=int(latest or 0) + 1,
            per_task_amount=per_task,
            per_day_amount=per_day,
            per_month_amount=per_month,
            effective_at=effective_at,
            created_by_principal_id=actor.actor_id,
        )
        session.add(row)
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="generation_limit.version_created",
            target_type="generation_limit_version",
            target_id=row.id,
            metadata={
                "billing_mode": row.billing_mode,
                "version_no": row.version_no,
                "per_task_amount": row.per_task_amount,
                "per_day_amount": row.per_day_amount,
                "per_month_amount": row.per_month_amount,
            },
        )
        return row

    def require_customer_reservation(
        self,
        session: Session,
        account_id: UUID,
        requested_units: int,
    ) -> GenerationLimitStatus:
        self._lock_reservation_subject(
            session, BillingMode.CUSTOMER_CREDIT, account_id
        )
        row = self._current(session, account_id=account_id, cost_center_id=None)
        wallet_id = session.scalar(
            select(CreditWallet.id).where(CreditWallet.account_id == account_id)
        )
        day_start, month_start = self._period_starts()
        used_day = self._customer_exposure(session, wallet_id, day_start)
        used_month = self._customer_exposure(session, wallet_id, month_start)
        return self._check(row, requested_units, used_day, used_month)

    def customer_status(
        self,
        session: Session,
        account_id: UUID,
    ) -> GenerationLimitStatus:
        row = self._current(session, account_id=account_id, cost_center_id=None)
        wallet_id = session.scalar(
            select(CreditWallet.id).where(CreditWallet.account_id == account_id)
        )
        day_start, month_start = self._period_starts()
        return self._status(
            row,
            self._customer_exposure(session, wallet_id, day_start),
            self._customer_exposure(session, wallet_id, month_start),
        )

    def require_internal_reservation(
        self,
        session: Session,
        cost_center_id: UUID,
        requested_fen: int,
    ) -> GenerationLimitStatus:
        self._lock_reservation_subject(
            session, BillingMode.INTERNAL_COST, cost_center_id
        )
        row = self._current(
            session,
            account_id=None,
            cost_center_id=cost_center_id,
        )
        day_start, month_start = self._period_starts()
        used_day = self._internal_exposure(session, cost_center_id, day_start)
        used_month = self._internal_exposure(session, cost_center_id, month_start)
        return self._check(row, requested_fen, used_day, used_month)

    def internal_status(
        self,
        session: Session,
        cost_center_id: UUID,
    ) -> GenerationLimitStatus:
        row = self._current(
            session,
            account_id=None,
            cost_center_id=cost_center_id,
        )
        day_start, month_start = self._period_starts()
        return self._status(
            row,
            self._internal_exposure(session, cost_center_id, day_start),
            self._internal_exposure(session, cost_center_id, month_start),
        )

    @staticmethod
    def _lock_reservation_subject(
        session: Session,
        billing_mode: BillingMode,
        subject_id: UUID,
    ) -> None:
        session.execute(
            text(
                "select pg_advisory_xact_lock("
                "hashtextextended(:lock_key, 0))"
            ),
            {
                "lock_key": (
                    f"generation-reservation:{billing_mode.value}:{subject_id}"
                )
            },
        )

    def _current(
        self,
        session: Session,
        *,
        account_id: UUID | None,
        cost_center_id: UUID | None,
    ) -> GenerationLimitVersion:
        subject_filter = (
            GenerationLimitVersion.account_id == account_id
            if account_id is not None
            else GenerationLimitVersion.cost_center_id == cost_center_id
        )
        row = session.scalar(
            select(GenerationLimitVersion)
            .where(
                subject_filter,
                GenerationLimitVersion.effective_at <= self.clock.now(),
            )
            .order_by(
                GenerationLimitVersion.effective_at.desc(),
                GenerationLimitVersion.version_no.desc(),
            )
            .limit(1)
        )
        if row is None:
            raise Conflict("generation limit is not configured")
        return row

    def _period_starts(self) -> tuple[datetime, datetime]:
        local = self.clock.now().astimezone(SHANGHAI)
        day = local.replace(hour=0, minute=0, second=0, microsecond=0).astimezone(UTC)
        month = local.replace(
            day=1,
            hour=0,
            minute=0,
            second=0,
            microsecond=0,
        ).astimezone(UTC)
        return day, month

    def _customer_exposure(
        self,
        session: Session,
        wallet_id: UUID | None,
        since: datetime,
    ) -> int:
        if wallet_id is None:
            return 0
        amount = case(
            (GenerationHold.status == HoldStatus.ACTIVE, GenerationHold.amount_units),
            (
                GenerationHold.status == HoldStatus.SETTLED,
                func.coalesce(
                    GenerationHold.settled_units,
                    GenerationHold.amount_units,
                ),
            ),
            else_=0,
        )
        return int(
            session.scalar(
                select(func.coalesce(func.sum(amount), 0)).where(
                    GenerationHold.wallet_id == wallet_id,
                    GenerationHold.created_at >= since,
                    GenerationHold.status.in_([
                        HoldStatus.ACTIVE,
                        HoldStatus.SETTLED,
                    ]),
                )
            )
            or 0
        )

    def _internal_exposure(
        self,
        session: Session,
        cost_center_id: UUID,
        since: datetime,
    ) -> int:
        amount = case(
            (
                InternalBudgetHold.status == HoldStatus.ACTIVE,
                InternalBudgetHold.amount_fen,
            ),
            (
                InternalBudgetHold.status == HoldStatus.SETTLED,
                func.coalesce(
                    InternalBudgetHold.settled_fen,
                    InternalBudgetHold.amount_fen,
                ),
            ),
            else_=0,
        )
        return int(
            session.scalar(
                select(func.coalesce(func.sum(amount), 0)).where(
                    InternalBudgetHold.cost_center_id == cost_center_id,
                    InternalBudgetHold.created_at >= since,
                    InternalBudgetHold.status.in_([
                        HoldStatus.ACTIVE,
                        HoldStatus.SETTLED,
                    ]),
                )
            )
            or 0
        )

    @staticmethod
    def _check(
        row: GenerationLimitVersion,
        requested: int,
        used_day: int,
        used_month: int,
    ) -> GenerationLimitStatus:
        if requested > row.per_task_amount:
            raise Conflict("single-task generation limit exceeded")
        if used_day + requested > row.per_day_amount:
            raise Conflict("daily generation limit exceeded")
        if used_month + requested > row.per_month_amount:
            raise Conflict("monthly generation limit exceeded")
        return GenerationLimitService._status(row, used_day, used_month)

    @staticmethod
    def _status(
        row: GenerationLimitVersion,
        used_day: int,
        used_month: int,
    ) -> GenerationLimitStatus:
        return GenerationLimitStatus(
            version_id=row.id,
            billing_mode=row.billing_mode,
            per_task_amount=row.per_task_amount,
            per_day_amount=row.per_day_amount,
            per_month_amount=row.per_month_amount,
            used_day_amount=used_day,
            used_month_amount=used_month,
        )
```

This is the final contents of the existing module: `UnconfiguredGenerationLimits` appears exactly once and remains import-compatible for the focused fail-closed foundation test. Both reservation methods acquire the subject-specific PostgreSQL transaction advisory lock before `_current()` or either exposure query. `BillingService` performs the corresponding hold-port `reserve()` before returning and never commits, rolls back, or opens a nested transaction, so the lock remains held through hold insertion and is released only with the caller's surrounding task/hold transaction. Status-only reads do not take the lock. Exposure is based on the hold's `created_at`: ACTIVE contributes its reserved amount, SETTLED contributes actual settled amount and fails closed to reserved amount if a corrupt legacy row lacks the actual value, and RELEASED contributes zero.

Plan 01 initially defines `BillingMode` and `BillingContext` in `service.py`. To avoid a service↔limits import cycle, move these exact definitions to `backend/src/ip_saas/modules/billing/models.py` (add the `dataclass` import; `StrEnum` and `UUID` are already imported there):

```python
from dataclasses import dataclass


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

Delete only those two original definitions from `service.py`, then add this re-exporting import to `service.py`:

```python
# backend/src/ip_saas/modules/billing/service.py
from ip_saas.modules.billing.models import BillingContext, BillingMode
```

All frozen imports from `ip_saas.modules.billing.service` remain valid. Do not move or alter `ProviderCostInput`.

- [ ] **Step 5: Keep Plan 01's mandatory port and install the database-backed limit implementation**

Modify `BillingService` in `backend/src/ip_saas/modules/billing/service.py`:

```python
from ip_saas.modules.billing.ports import GenerationLimitPort


class BillingService:
    def __init__(
        self,
        credit_holds: CreditHoldPort,
        internal_holds: InternalBudgetHoldPort,
        generation_limits: GenerationLimitPort,
    ) -> None:
        self.credit_holds = credit_holds
        self.internal_holds = internal_holds
        self.generation_limits = generation_limits

    def reserve_customer_generation(
        self,
        session: Session,
        account_id: UUID,
        credit_units: int,
        idempotency_key: str,
        actor: ActorContext,
    ) -> BillingContext:
        self.generation_limits.require_customer_reservation(
            session,
            account_id,
            credit_units,
        )
        hold_id = self.credit_holds.reserve(
            session,
            account_id,
            credit_units,
            idempotency_key,
            actor,
        )
        return BillingContext(BillingMode.CUSTOMER_CREDIT, hold_id)

    def reserve_internal_generation(
        self,
        session: Session,
        cost_center_id: UUID,
        amount_fen: int,
        idempotency_key: str,
        actor: ActorContext,
    ) -> BillingContext:
        self.generation_limits.require_internal_reservation(
            session,
            cost_center_id,
            amount_fen,
        )
        hold_id = self.internal_holds.reserve(
            session,
            cost_center_id,
            amount_fen,
            idempotency_key,
            actor,
        )
        return BillingContext(BillingMode.INTERNAL_COST, hold_id)
```

Create `backend/src/ip_saas/modules/billing/composition.py` as the only production constructor:

```python
from ip_saas.common.clock import Clock
from ip_saas.modules.billing.adapters import (
    SqlCreditHoldPort,
    SqlInternalBudgetHoldPort,
)
from ip_saas.modules.billing.limits import GenerationLimitService
from ip_saas.modules.billing.service import BillingService


def build_billing_service(clock: Clock) -> BillingService:
    return BillingService(
        SqlCreditHoldPort(),
        SqlInternalBudgetHoldPort(),
        GenerationLimitService(clock),
    )
```

Modify `create_app()` in `backend/src/ip_saas/api.py` without creating a second `BillingService`. Replace Plan 02's temporary fail-closed `app.state.billing_service = build_foundation_billing_service()` assignment and remove that helper from the `api.py` import; do not add a second assignment:

```python
from ip_saas.common.clock import SystemClock
from ip_saas.modules.billing.composition import build_billing_service

app.state.billing_service = build_billing_service(SystemClock())
```

Keep the replacement before Plan 02 calls `build_intelligence_dependencies(settings, app.state.billing_service, app.state.private_object_store)`. The already-composed intelligence task submitter therefore receives this exact database-backed instance rather than retaining the earlier deny adapter. Retain the exception handlers, registered routers, health route, and single return unchanged.

Also modify `backend/src/ip_saas/modules/intelligence/composition.py`: delete the entire `build_foundation_billing_service()` function, remove its `SqlCreditHoldPort`, `SqlInternalBudgetHoldPort`, and `UnconfiguredGenerationLimits` imports, and keep the `BillingService` import only because `build_intelligence_dependencies(..., billing_service: BillingService, ...)` still types the injected production instance. Do not leave an unused alias or compatibility wrapper. After this deletion, `backend/src/ip_saas/modules/billing/limits.py` is the only production-source file allowed to define or mention `UnconfiguredGenerationLimits`, exactly as the Step 6 AST contract requires.

Application services and Worker entrypoints receive this composed instance through their existing constructor/dependency boundary. They must not instantiate `UnconfiguredGenerationLimits`, `GenerationLimitService`, or another BillingService themselves.

Plan 01 already made this constructor dependency mandatory and used a fail-closed `UnconfiguredGenerationLimits` production adapter. Do not introduce `None`, a permissive default, an environment switch, or an implicit unlimited mode here. Replace the deny adapter in every application/Worker production composition root with `GenerationLimitService`; focused unit tests may pass Plan 01's explicitly named deterministic fake. A two-argument `BillingService(...)` construction remains a type/test failure, so feature modules cannot silently bypass caps.

Keep the frozen `settle_generation(session, context, actual_amount, provider_cost, idempotency_key)` and `release_generation(session, context, reason, idempotency_key)` signatures and ordinary single-cost behavior source-compatible. Plan 01's additive `settle_generation_batch(...)` remains restricted to retry-exhaustion reconciliation and does not change reservation callers. The limit check occurs before either hold port, so a rejected task creates no hold, task, audit, or outbox row and incurs no supplier cost.

- [ ] **Step 6: Prove production composition cannot omit the limit dependency**

Create `backend/tests/contract/billing/test_generation_limit_composition.py`:

```python
import ast
import inspect
from pathlib import Path
from textwrap import dedent

from ip_saas.modules.billing.limits import GenerationLimitService
from ip_saas.modules.billing.service import BillingService


def test_generation_limits_have_no_constructor_default() -> None:
    parameter = inspect.signature(BillingService).parameters["generation_limits"]
    assert parameter.default is inspect.Parameter.empty


def test_every_production_billing_composition_passes_limits_explicitly() -> None:
    backend = Path(__file__).parents[3]
    violations: list[str] = []
    for path in sorted((backend / "src").rglob("*.py")):
        tree = ast.parse(path.read_text())
        for node in ast.walk(tree):
            if not isinstance(node, ast.Call):
                continue
            name = (
                node.func.id if isinstance(node.func, ast.Name)
                else node.func.attr if isinstance(node.func, ast.Attribute)
                else None
            )
            if name != "BillingService":
                continue
            has_keyword = any(
                keyword.arg == "generation_limits" for keyword in node.keywords
            )
            if len(node.args) < 3 and not has_keyword:
                violations.append(f"{path.relative_to(backend)}:{node.lineno}")
    assert violations == []


def test_no_production_composition_keeps_the_foundation_deny_adapter() -> None:
    backend = Path(__file__).parents[3]
    definition = Path("src/ip_saas/modules/billing/limits.py")
    violations: list[str] = []
    for path in sorted((backend / "src").rglob("*.py")):
        if path.relative_to(backend) == definition:
            continue
        tree = ast.parse(path.read_text())
        for node in ast.walk(tree):
            if isinstance(node, ast.ImportFrom) and any(
                alias.name == "UnconfiguredGenerationLimits"
                for alias in node.names
            ):
                violations.append(f"{path.relative_to(backend)}:{node.lineno}")
                continue
            if not isinstance(node, ast.Call):
                continue
            name = (
                node.func.id if isinstance(node.func, ast.Name)
                else node.func.attr if isinstance(node.func, ast.Attribute)
                else None
            )
            if name == "UnconfiguredGenerationLimits":
                violations.append(f"{path.relative_to(backend)}:{node.lineno}")
    assert violations == []


def _ordered_self_calls(method) -> list[str]:
    tree = ast.parse(dedent(inspect.getsource(method)))
    calls = [
        (node.lineno, node.col_offset, node.func.attr)
        for node in ast.walk(tree)
        if isinstance(node, ast.Call)
        and isinstance(node.func, ast.Attribute)
        and isinstance(node.func.value, ast.Name)
        and node.func.value.id == "self"
    ]
    return [name for _line, _column, name in sorted(calls)]


def test_both_limit_checks_lock_before_resolution_and_exposure() -> None:
    for method, exposure in (
        (GenerationLimitService.require_customer_reservation,
         "_customer_exposure"),
        (GenerationLimitService.require_internal_reservation,
         "_internal_exposure"),
    ):
        calls = _ordered_self_calls(method)
        lock_at = calls.index("_lock_reservation_subject")
        assert lock_at < calls.index("_current")
        assert lock_at < calls.index(exposure)


def test_cross_module_integration_never_uses_the_early_allow_fake() -> None:
    backend = Path(__file__).parents[3]
    roots = (
        "tests/integration/billing",
        "tests/integration/tasks",
        "tests/integration/intelligence",
        "tests/integration/media",
        "tests/integration/publication",
    )
    candidates = [
        path
        for root in roots
        for path in (backend / root).rglob("*.py")
    ]
    candidates.append(backend / "tests/support/publication_fixtures.py")
    violations = [
        str(path.relative_to(backend))
        for path in candidates
        if "AllowAllGenerationLimits" in path.read_text(encoding="utf-8")
    ]
    assert violations == []


def test_api_composes_the_real_database_limit_service() -> None:
    backend = Path(__file__).parents[3]
    composition = ast.parse(
        (backend / "src/ip_saas/modules/billing/composition.py").read_text()
    )
    api = ast.parse((backend / "src/ip_saas/api.py").read_text())
    composition_calls = {
        node.func.id
        for node in ast.walk(composition)
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
    }
    api_calls = {
        node.func.id
        for node in ast.walk(api)
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
    }
    assert "GenerationLimitService" in composition_calls
    assert "build_billing_service" in api_calls
```

Run: `cd backend && uv run pytest tests/contract/billing/test_generation_limit_composition.py -v`

Expected: PASS, 6 tests; removing the constructor argument, omitting an explicit limit implementation, leaving Plan 01's staged deny adapter in production, moving either reservation lock after resolution/exposure, or retaining the early allow fake in a cross-module integration harness fails before feature tests can hide the mistake.

- [ ] **Step 7: Write a real two-session PostgreSQL concurrency regression**

Create `backend/tests/integration/billing/test_generation_limit_concurrency.py`:

```python
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from datetime import UTC, datetime
from threading import Event, Lock
from uuid import UUID, uuid4

import pytest
from sqlalchemy import Engine, delete, func, select
from sqlalchemy.orm import Session

from ip_saas.common.errors import Conflict
from ip_saas.db.base import Base
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import Account, AccountKind
from ip_saas.modules.billing.adapters import (
    SqlCreditHoldPort,
    SqlInternalBudgetHoldPort,
)
from ip_saas.modules.billing.limits import GenerationLimitService
from ip_saas.modules.billing.models import (
    CreditWallet,
    GenerationHold,
    GenerationLimitVersion,
    HoldStatus,
    InternalBudgetHold,
    InternalCostCenter,
    InternalCostEntry,
    InternalCostKind,
)
from ip_saas.modules.billing.service import BillingService


class FixedClock:
    def now(self) -> datetime:
        return datetime(2026, 8, 24, 8, 0, tzinfo=UTC)


@dataclass(frozen=True)
class ConcurrentSubject:
    mode: str
    account_id: UUID
    subject_id: UUID
    wallet_id: UUID | None
    actor: ActorContext


class LockProbe:
    def __init__(self) -> None:
        self.first_snapshot_read = Event()
        self.second_attempted = Event()
        self.second_finished = Event()
        self.release_first = Event()
        self._mutex = Lock()
        self._calls = 0

    def next_call(self) -> int:
        with self._mutex:
            self._calls += 1
            return self._calls


class CoordinatedGenerationLimitService(GenerationLimitService):
    def __init__(self, clock: FixedClock, probe: LockProbe) -> None:
        super().__init__(clock)
        self.probe = probe
        self.ordinal = 0

    def _lock_reservation_subject(
        self, session: Session, billing_mode, subject_id: UUID,
    ) -> None:
        self.ordinal = self.probe.next_call()
        if self.ordinal == 2:
            self.probe.second_attempted.set()
        super()._lock_reservation_subject(session, billing_mode, subject_id)

    def _check(self, row, requested, used_day, used_month):
        if self.ordinal == 1:
            self.probe.first_snapshot_read.set()
            if not self.probe.release_first.wait(timeout=10):
                raise RuntimeError("concurrency probe did not release first transaction")
        return super()._check(row, requested, used_day, used_month)


def seed_subject(pg_engine: Engine, mode: str) -> ConcurrentSubject:
    Base.metadata.create_all(pg_engine)
    account_id = uuid4()
    actor_id = uuid4()
    with Session(pg_engine) as session:
        with session.begin():
            account = Account(
                id=account_id,
                kind=(AccountKind.C_USER if mode == "customer"
                      else AccountKind.PLATFORM),
                display_name=f"concurrent-{mode}",
            )
            session.add(account)
            session.flush()
            admin = ActorContext(uuid4(), account.id, ActorKind.PLATFORM_ADMIN)
            limits = GenerationLimitService(FixedClock())
            if mode == "customer":
                wallet = CreditWallet(
                    account_id=account.id,
                    system_code=None,
                    allow_negative=False,
                    posted_balance=10_000,
                )
                session.add(wallet)
                session.flush()
                limits.create_account_version(
                    session, admin, account.id,
                    per_task_units=600,
                    per_day_units=1_000,
                    per_month_units=1_000,
                    effective_at=FixedClock().now(),
                )
                return ConcurrentSubject(
                    mode, account.id, account.id, wallet.id,
                    ActorContext(actor_id, account.id, ActorKind.C_USER),
                )
            center = InternalCostCenter(
                platform_account_id=account.id,
                code=f"concurrent-{uuid4().hex}",
                display_name="Concurrent internal",
                active=True,
            )
            session.add(center)
            session.flush()
            session.add(InternalCostEntry(
                cost_center_id=center.id,
                kind=InternalCostKind.ALLOCATION,
                amount_fen=10_000,
                idempotency_key=f"concurrent-allocation-{center.id}",
                created_by_principal_id=admin.actor_id,
                reversal_of_id=None,
            ))
            limits.create_internal_version(
                session, admin, center.id,
                per_task_fen=600,
                per_day_fen=1_000,
                per_month_fen=1_000,
                effective_at=FixedClock().now(),
            )
            return ConcurrentSubject(
                mode, account.id, center.id, None,
                ActorContext(actor_id, account.id, ActorKind.PLATFORM_OPERATOR),
            )


def reserve_once(
    pg_engine: Engine,
    subject: ConcurrentSubject,
    probe: LockProbe,
    index: int,
) -> str:
    try:
        with Session(pg_engine) as session:
            with session.begin():
                billing = BillingService(
                    SqlCreditHoldPort(),
                    SqlInternalBudgetHoldPort(),
                    CoordinatedGenerationLimitService(FixedClock(), probe),
                )
                if subject.mode == "customer":
                    billing.reserve_customer_generation(
                        session, subject.subject_id, 600,
                        f"concurrent-customer-{subject.account_id}-{index}",
                        subject.actor,
                    )
                else:
                    billing.reserve_internal_generation(
                        session, subject.subject_id, 600,
                        f"concurrent-internal-{subject.account_id}-{index}",
                        subject.actor,
                    )
        outcome = "reserved"
    except Conflict:
        outcome = "rejected"
    finally:
        if index == 2:
            probe.second_finished.set()
    return outcome


def cleanup(pg_engine: Engine, subject: ConcurrentSubject) -> None:
    with Session(pg_engine) as session:
        with session.begin():
            if subject.mode == "customer":
                session.execute(delete(GenerationHold).where(
                    GenerationHold.wallet_id == subject.wallet_id
                ))
                session.execute(delete(GenerationLimitVersion).where(
                    GenerationLimitVersion.account_id == subject.subject_id
                ))
                session.execute(delete(CreditWallet).where(
                    CreditWallet.id == subject.wallet_id
                ))
            else:
                session.execute(delete(InternalBudgetHold).where(
                    InternalBudgetHold.cost_center_id == subject.subject_id
                ))
                session.execute(delete(InternalCostEntry).where(
                    InternalCostEntry.cost_center_id == subject.subject_id
                ))
                session.execute(delete(GenerationLimitVersion).where(
                    GenerationLimitVersion.cost_center_id == subject.subject_id
                ))
                session.execute(delete(InternalCostCenter).where(
                    InternalCostCenter.id == subject.subject_id
                ))
            session.execute(delete(Account).where(Account.id == subject.account_id))


@pytest.mark.parametrize("mode", ["customer", "internal"])
def test_same_subject_concurrent_reservations_cannot_cross_cap(
    pg_engine: Engine,
    mode: str,
) -> None:
    subject = seed_subject(pg_engine, mode)
    try:
        probe = LockProbe()
        with ThreadPoolExecutor(max_workers=2) as pool:
            first = pool.submit(reserve_once, pg_engine, subject, probe, 1)
            assert probe.first_snapshot_read.wait(timeout=5)
            second = pool.submit(reserve_once, pg_engine, subject, probe, 2)
            assert probe.second_attempted.wait(timeout=5)
            finished_before_release = probe.second_finished.wait(timeout=2)
            probe.release_first.set()
            outcomes = [first.result(timeout=10), second.result(timeout=10)]
        assert not finished_before_release
        assert sorted(outcomes) == ["rejected", "reserved"]
        with Session(pg_engine) as verification:
            if mode == "customer":
                count = verification.scalar(
                    select(func.count()).select_from(GenerationHold).where(
                        GenerationHold.wallet_id == subject.wallet_id,
                        GenerationHold.status == HoldStatus.ACTIVE,
                    )
                )
            else:
                count = verification.scalar(
                    select(func.count()).select_from(InternalBudgetHold).where(
                        InternalBudgetHold.cost_center_id == subject.subject_id,
                        InternalBudgetHold.status == HoldStatus.ACTIVE,
                    )
                )
            assert count == 1
    finally:
        cleanup(pg_engine, subject)
```

This suite uses Plan 01's real `pg_engine`, two independent SQLAlchemy Sessions and transactions, real SQL hold ports, and two simultaneous 600-unit reservations against a 1,000 cap. A single shared Session, SQLite, a mock port, sleeps, or a precomputed outcome does not satisfy the test.

- [ ] **Step 8: Prove the concurrency test turns red when either reservation lock is removed**

Temporarily replace only the `session.execute(...)` body of `_lock_reservation_subject()` with `del session, billing_mode, subject_id`; keep both caller methods and the concurrency probe otherwise unchanged. Do not commit this mutation. Run:

```bash
cd backend && uv run pytest tests/integration/billing/test_generation_limit_concurrency.py -v
```

Expected: FAIL for both parameters: the second transaction finishes before the first snapshot is released and both return `reserved`, demonstrating that wallet/cost-center row locking occurs too late to protect the earlier limit snapshots. Restore the exact `pg_advisory_xact_lock(hashtextextended(...))` body from Step 4 with `apply_patch`.

- [ ] **Step 9: Run the concurrency test with the reservation locks restored**

Run:

```bash
cd backend && uv run pytest tests/integration/billing/test_generation_limit_concurrency.py -v
```

Expected: PASS, 2 tests; for both customer credits and internal人民币分, one transaction commits one ACTIVE hold and the other receives `Conflict` after it acquires the same subject lock and observes the committed exposure.

- [ ] **Step 10: Create real effective-limit helpers for cross-plan tests**

Create an empty `backend/tests/fixtures/__init__.py`, then create `backend/tests/fixtures/generation_limits.py`:

```python
from uuid import UUID

from sqlalchemy.orm import Session

from ip_saas.common.clock import Clock
from ip_saas.modules.accounts.context import ActorContext
from ip_saas.modules.billing.limits import GenerationLimitService


def allow_customer(
    session: Session,
    admin: ActorContext,
    account_id: UUID,
    clock: Clock,
) -> GenerationLimitService:
    service = GenerationLimitService(clock)
    service.create_account_version(
        session, admin, account_id,
        per_task_units=1_000_000,
        per_day_units=10_000_000,
        per_month_units=100_000_000,
        effective_at=clock.now(),
    )
    return service


def allow_internal(
    session: Session,
    admin: ActorContext,
    cost_center_id: UUID,
    clock: Clock,
) -> GenerationLimitService:
    service = GenerationLimitService(clock)
    service.create_internal_version(
        session, admin, cost_center_id,
        per_task_fen=1_000_000,
        per_day_fen=10_000_000,
        per_month_fen=100_000_000,
        effective_at=clock.now(),
    )
    return service
```

These helpers insert real `GenerationLimitVersion` rows effective at the exact injected clock and return the database-backed service. They are not unlimited fakes: lowering a persisted cap must make the consuming integration test fail.

- [ ] **Step 11: Retrofit every Plan 01–04 integration composition explicitly**

Make these exact changes; no other test directory is implicitly covered:

1. In `backend/tests/integration/billing/test_internal_costs.py`, keep only `test_unconfigured_generation_limits_fail_before_any_hold` on `UnconfiguredGenerationLimits`. Replace every `AllowAllGenerationLimits` import/call in ordinary customer/internal cases with `allow_customer(..., FixedClock())` or `allow_internal(..., FixedClock())` after the account/cost center exists.
2. In `backend/tests/integration/tasks/test_task_submission.py`, remove `AllowAllGenerationLimits`. Before the first BillingService construction, persist the first customer's limit and pass that returned service. Before the second-account submission, call `allow_customer` for the second account so the already-composed service can resolve its row. In the platform test, persist an internal version after center creation and pass its returned service. Assert the stored version ID through `billing.generation_limits.customer_status()` or `internal_status()` so a rowless fake cannot pass.
3. In `backend/tests/integration/intelligence/conftest.py`, remove `SystemClock` and define one `IntelligenceScenarioClock` whose `now()` returns `datetime(2026, 8, 24, 8, 0, tzinfo=UTC)`. Publish it as the named `scenario_clock: Clock` fixture; inject that fixture into the existing `audit_writer` and every other conftest fixture that previously constructed `SystemClock()`. Add the following two explicit factories. Modify `test_api_workflow.py` and `test_self_marketing_isolation.py` so their cross-plan reservation cases use real TaskSubmissionService/BillingService composition: create the account or cost center, invoke the matching factory, assert its status resolves a persisted effective version, then submit. Keep a direct `Mock(spec=TaskSubmissionService)` only in interaction tests that never construct BillingService or reserve a hold.

```python
from datetime import UTC, datetime

import pytest

from ip_saas.common.clock import Clock
from tests.fixtures.generation_limits import allow_customer, allow_internal


class IntelligenceScenarioClock:
    def now(self) -> datetime:
        return datetime(2026, 8, 24, 8, 0, tzinfo=UTC)


@pytest.fixture
def scenario_clock() -> Clock:
    return IntelligenceScenarioClock()


@pytest.fixture
def real_customer_limits(db_session, scenario_clock):
    def create(admin, account_id):
        return allow_customer(db_session, admin, account_id, scenario_clock)
    return create


@pytest.fixture
def real_internal_limits(db_session, scenario_clock):
    def create(admin, cost_center_id):
        return allow_internal(db_session, admin, cost_center_id, scenario_clock)
    return create
```

4. In `backend/tests/integration/media/harness.py`, remove the `AllowAllGenerationLimits` import and instance. Add this exact shared clock, change `MediaIntegrationHarness.__init__(session, clock)` to save it as `self.clock`, and make every seed path use it. Each seed creates a `limit_admin` `ActorContext` with `ActorKind.PLATFORM_ADMIN` for its persisted platform account. For a customer seed, call `allow_customer(self.session, limit_admin, account.id, self.clock)` after the customer account exists and before BillingService/task submission; for an internal seed, call `allow_internal(self.session, limit_admin, center.id, self.clock)` after the cost center/allocation exists. Query `customer_status()` or `internal_status()` and assert that `session.get(GenerationLimitVersion, status.version_id).effective_at <= self.clock.now()` before submission.

```python
from datetime import UTC, datetime, timedelta


class MediaScenarioClock:
    def __init__(self) -> None:
        self.instant = datetime(2026, 8, 24, 8, 0, tzinfo=UTC)

    def now(self) -> datetime:
        return self.instant

    def advance(self, seconds: int) -> None:
        self.instant += timedelta(seconds=seconds)
```

In `backend/tests/integration/media/conftest.py`, add a named `media_scenario_clock` fixture and change only the existing harness fixture as follows; no autouse fixture is permitted:

```python
from .harness import MediaIntegrationHarness, MediaScenarioClock


@pytest.fixture
def media_scenario_clock() -> MediaScenarioClock:
    return MediaScenarioClock()


@pytest.fixture
def media_integration_harness(
    db_session: Session,
    media_scenario_clock: MediaScenarioClock,
) -> MediaIntegrationHarness:
    return MediaIntegrationHarness(db_session, media_scenario_clock)
```

5. Modify Plan 04's single registered fixture source `backend/tests/support/publication_fixtures.py`; do not create `backend/tests/integration/publication/conftest.py` or alter the existing root `backend/tests/conftest.py` plugin registration. Remove its `AllowAllGenerationLimits` import and instance. Add `datetime`, `GenerationLimitService`, and `GenerationLimitStatus` imports and name the mutable frozen clock already required by its builders `PublicationScenarioClock`; it has the same `now()`/`advance(seconds)` implementation as `MediaScenarioClock` above. In `publication_harness`, instantiate that clock once, pass the same object into `PublicationScenarioBuilders`, `AuditWriter`, task Workers, and `GenerationLimitService`, then construct the harness's one shared BillingService with that real service. `build_lineage_values` stores its platform-admin context on `self.platform_admin` before any later builder runs. Every builder path that will submit a customer task must call `allow_customer(self.session, self.platform_admin, customer_account.id, self.clock)` immediately after creating that account; every internal path calls `allow_internal(self.session, self.platform_admin, center.id, self.clock)` immediately after creating its center/allocation. Both calls happen before the first submission, and the shared real service sees those rows in the same Session.

Add `limit_status: Callable[[], GenerationLimitStatus]` and `clock_now: Callable[[], datetime]` to `BilledFailureCase`. `build_billed_failure` sets `limit_status` to a zero-argument wrapper around `generation_limits.customer_status(session, account.id)` or `generation_limits.internal_status(session, center.id)`, matching the selected mode, and sets `clock_now=scenario_clock.now`. In `test_billed_failure_recovery.py`, import `GenerationLimitVersion` and assert for both parameterized modes:

```python
status = case.limit_status()
version = session.get(GenerationLimitVersion, status.version_id)
assert version is not None
assert version.effective_at <= case.clock_now()
```

This one registered Plan 04 harness remains the fixture truth for metric, comment, retro, failure, idempotency, router, and security scenarios. Every customer/internal cross-module case now queries the just-created version before submission. The contract test from Step 6 proves no `AllowAllGenerationLimits` reference remains anywhere under the five integration roots or the registered publication support fixture.

- [ ] **Step 12: Run the contract and focused limit suites after the retrofit**

Run:

```bash
cd backend && uv run pytest \
  tests/contract/billing/test_generation_limit_composition.py \
  tests/integration/billing/test_generation_limits.py \
  tests/integration/billing/test_generation_limit_concurrency.py \
  tests/integration/billing/test_internal_costs.py \
  tests/integration/tasks/test_task_submission.py -v
```

Expected: PASS. Exact cap succeeds, cap+1 and missing configuration fail before hold creation; Shanghai day/month boundaries are correct; ACTIVE plus SETTLED-actual counts, RELEASED does not; the two-session race admits exactly one hold; task/hold/audit/outbox atomicity is unchanged.

- [ ] **Step 13: Run the explicit Plans 01–04 cross-module regressions**

Run:

```bash
cd backend && uv run pytest \
  tests/integration/billing/test_internal_costs.py \
  tests/integration/tasks/test_task_submission.py \
  tests/integration/intelligence/test_api_workflow.py \
  tests/integration/intelligence/test_self_marketing_isolation.py \
  tests/integration/media \
  tests/integration/publication -v
cd backend && uv run pytest tests/contract/billing/test_generation_limit_composition.py -v
```

Expected: PASS; Plan 01 customer/internal tasks, Plan 02 customer/self-marketing submissions, Plan 03 media customer/internal scenarios, and Plan 04 screenshot/comment/retro submissions all use persisted effective versions and the real GenerationLimitService. The contract run reports no production deny adapter, no missing third constructor argument, and no early allow fake in cross-module integration.

- [ ] **Step 14: Commit generation limit enforcement and every retrofit path**

```bash
git add backend/src/ip_saas/modules/billing/models.py backend/src/ip_saas/modules/billing/limits.py backend/src/ip_saas/modules/billing/service.py backend/src/ip_saas/modules/billing/composition.py backend/src/ip_saas/api.py backend/tests/contract/billing/test_generation_limit_composition.py backend/tests/integration/billing/test_generation_limits.py backend/tests/integration/billing/test_generation_limit_concurrency.py backend/tests/integration/billing/test_internal_costs.py backend/tests/integration/tasks/test_task_submission.py backend/tests/fixtures/__init__.py backend/tests/fixtures/generation_limits.py backend/tests/integration/intelligence/conftest.py backend/tests/integration/intelligence/test_api_workflow.py backend/tests/integration/intelligence/test_self_marketing_isolation.py backend/tests/integration/media/conftest.py backend/tests/integration/media/harness.py backend/tests/support/publication_fixtures.py backend/tests/integration/publication/test_billed_failure_recovery.py
git commit -m "feat: enforce concurrent generation spending limits"
```

### Task 8: Build privacy-safe customer and cash/credit reconciliation reports

**Files:**
- Create: `backend/src/ip_saas/modules/resellers/reporting.py`
- Test: `backend/tests/integration/resellers/test_reports.py`

- [ ] **Step 1: Write a failing report-boundary test**

Create `backend/tests/integration/resellers/test_reports.py`:

```python
from datetime import UTC, datetime
from uuid import uuid4

from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import Account, AccountKind, ResellerRelation
from ip_saas.modules.resellers.access import ResellerTreeAccess
from ip_saas.modules.resellers.reporting import ResellerReportService


class FixedClock:
    def now(self) -> datetime:
        return datetime(2026, 8, 24, 8, 0, tzinfo=UTC)


def test_report_contains_separate_commercial_totals_and_no_content(
    db_session: Session,
) -> None:
    l1 = Account(kind=AccountKind.RESELLER_L1, display_name="L1")
    customer = Account(kind=AccountKind.C_USER, display_name="Customer")
    db_session.add_all([l1, customer])
    db_session.flush()
    db_session.add(
        ResellerRelation(
            parent_account_id=l1.id,
            child_account_id=customer.id,
            created_by_principal_id=uuid4(),
            active=True,
        )
    )
    report = ResellerReportService(
        ResellerTreeAccess(), FixedClock(), AuditWriter(FixedClock())
    ).build(
        db_session,
        ActorContext(uuid4(), l1.id, ActorKind.RESELLER_L1),
    )
    payload = report.model_dump(mode="json")
    assert {
        "actual_cash_fen",
        "estimated_cash_fen",
        "allocated_credit_units",
        "returned_credit_units",
        "generation_spend_units",
    } <= payload["customers"][0].keys()
    serialized = str(payload).lower()
    for forbidden in (
        "interview",
        "script",
        "voice_asset",
        "media_asset",
        "source_asset",
        "content_version",
        "publication_url",
        "consent",
    ):
        assert forbidden not in serialized
```

- [ ] **Step 2: Run the test and verify the missing report service**

Run: `cd backend && uv run pytest tests/integration/resellers/test_reports.py -v`

Expected: FAIL during collection because `ResellerReportService` is missing.

- [ ] **Step 3: Implement report DTOs and constrained ledger/cash queries**

Create `backend/src/ip_saas/modules/resellers/reporting.py`:

```python
from datetime import datetime
from uuid import UUID

from pydantic import BaseModel, ConfigDict
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.common.clock import Clock
from ip_saas.common.errors import Forbidden
from ip_saas.modules.accounts.context import ActorContext
from ip_saas.modules.accounts.context import ActorKind
from ip_saas.modules.accounts.models import Account, ResellerRelation
from ip_saas.modules.billing.models import (
    CreditLedgerEntry,
    CreditTransaction,
    CreditTransactionKind,
    CreditWallet,
)

from .access import ResellerTreeAccess
from .enums import CashOrderStatus, CreditMovementKind, CreditMovementStatus
from .models import (
    CreditMovementRequest,
    OfflineCashOrder,
    SupportGrant,
    SupportGrantRevocation,
)


class ReportModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class CustomerCommercialSummary(ReportModel):
    account_id: UUID
    display_name: str
    account_kind: str
    account_status: str
    direct_parent_account_id: UUID
    credit_balance: int
    actual_cash_fen: int
    estimated_cash_fen: int
    allocated_credit_units: int
    returned_credit_units: int
    generation_spend_units: int
    pending_return_count: int
    active_support_grant_count: int


class CashCreditReconciliationRow(ReportModel):
    cash_order_id: UUID
    payer_account_id: UUID
    payee_account_id: UUID
    amount_kind: str
    display_amount_fen: int
    confirmed_amount_fen: int | None
    pricing_version_id: UUID
    allocation_request_id: UUID | None
    allocated_credit_units: int
    reconciliation_state: str


class ResellerReport(ReportModel):
    generated_at: datetime
    customers: list[CustomerCommercialSummary]
    cash_credit_rows: list[CashCreditReconciliationRow]


class ResellerReportService:
    def __init__(
        self,
        tree: ResellerTreeAccess,
        clock: Clock,
        audit: AuditWriter,
    ) -> None:
        self.tree = tree
        self.clock = clock
        self.audit = audit

    def build(self, session: Session, actor: ActorContext) -> ResellerReport:
        if actor.kind not in {ActorKind.RESELLER_L1, ActorKind.RESELLER_L2}:
            raise Forbidden("reseller account is required")
        visible_ids = self.tree.visible_descendant_ids(session, actor)
        accounts = list(
            session.scalars(
                select(Account)
                .where(Account.id.in_(visible_ids))
                .order_by(Account.created_at, Account.id)
            )
        )
        parent_by_child = dict(
            session.execute(
                select(
                    ResellerRelation.child_account_id,
                    ResellerRelation.parent_account_id,
                ).where(
                    ResellerRelation.child_account_id.in_(visible_ids),
                    ResellerRelation.active.is_(True),
                )
            ).all()
        )
        customers = [
            self._customer(
                session,
                actor,
                account,
                parent_by_child[account.id],
            )
            for account in accounts
        ]
        orders = list(
            session.scalars(
                select(OfflineCashOrder)
                .where(OfflineCashOrder.payer_account_id.in_(visible_ids))
                .order_by(OfflineCashOrder.created_at, OfflineCashOrder.id)
            )
        )
        report = ResellerReport(
            generated_at=self.clock.now(),
            customers=customers,
            cash_credit_rows=[self._reconcile(session, order) for order in orders],
        )
        self.audit.write(
            session,
            actor=actor,
            action="reseller_report.viewed",
            target_type="account",
            target_id=actor.account_id,
            metadata={"descendant_count": len(visible_ids)},
        )
        return report

    def _customer(
        self,
        session: Session,
        actor: ActorContext,
        account: Account,
        parent_account_id: UUID,
    ) -> CustomerCommercialSummary:
        wallet = session.scalar(
            select(CreditWallet).where(CreditWallet.account_id == account.id)
        )
        actual_cash = int(
            session.scalar(
                select(func.coalesce(func.sum(OfflineCashOrder.confirmed_amount_fen), 0)).where(
                    OfflineCashOrder.payer_account_id == account.id,
                    OfflineCashOrder.status == CashOrderStatus.CONFIRMED,
                )
            )
            or 0
        )
        estimated_cash = int(
            session.scalar(
                select(func.coalesce(func.sum(OfflineCashOrder.estimated_amount_fen), 0)).where(
                    OfflineCashOrder.payer_account_id == account.id,
                    OfflineCashOrder.status == CashOrderStatus.RECORDED,
                    OfflineCashOrder.reported_amount_fen.is_(None),
                )
            )
            or 0
        )
        allocated = self._movement_total(
            session,
            account.id,
            CreditMovementKind.ALLOCATION,
            destination=True,
        )
        returned = self._movement_total(
            session,
            account.id,
            CreditMovementKind.RETURN,
            destination=False,
        )
        spend = 0
        if wallet is not None:
            spend = int(
                -(
                    session.scalar(
                        select(
                            func.coalesce(
                                func.sum(CreditLedgerEntry.amount_units),
                                0,
                            )
                        )
                        .join(
                            CreditTransaction,
                            CreditTransaction.id == CreditLedgerEntry.transaction_id,
                        )
                        .where(
                            CreditLedgerEntry.wallet_id == wallet.id,
                            CreditLedgerEntry.amount_units < 0,
                            CreditTransaction.kind == CreditTransactionKind.CONSUME,
                        )
                    )
                    or 0
                )
            )
        pending_returns = int(
            session.scalar(
                select(func.count())
                .select_from(CreditMovementRequest)
                .where(
                    CreditMovementRequest.source_account_id == account.id,
                    CreditMovementRequest.kind == CreditMovementKind.RETURN,
                    CreditMovementRequest.status == CreditMovementStatus.REQUESTED,
                )
            )
            or 0
        )
        active_grants = int(
            session.scalar(
                select(func.count())
                .select_from(SupportGrant)
                .where(
                    SupportGrant.customer_account_id == account.id,
                    SupportGrant.reseller_account_id == actor.account_id,
                    SupportGrant.expires_at > self.clock.now(),
                    ~select(SupportGrantRevocation.id)
                    .where(SupportGrantRevocation.grant_id == SupportGrant.id)
                    .exists(),
                )
            )
            or 0
        )
        return CustomerCommercialSummary(
            account_id=account.id,
            display_name=account.display_name,
            account_kind=account.kind,
            account_status=account.status,
            direct_parent_account_id=parent_account_id,
            credit_balance=wallet.posted_balance if wallet is not None else 0,
            actual_cash_fen=actual_cash,
            estimated_cash_fen=estimated_cash,
            allocated_credit_units=allocated,
            returned_credit_units=returned,
            generation_spend_units=spend,
            pending_return_count=pending_returns,
            active_support_grant_count=active_grants,
        )

    @staticmethod
    def _movement_total(
        session: Session,
        account_id: UUID,
        kind: CreditMovementKind,
        *,
        destination: bool,
    ) -> int:
        account_column = (
            CreditMovementRequest.destination_account_id
            if destination
            else CreditMovementRequest.source_account_id
        )
        return int(
            session.scalar(
                select(
                    func.coalesce(
                        func.sum(CreditMovementRequest.credit_units),
                        0,
                    )
                ).where(
                    account_column == account_id,
                    CreditMovementRequest.kind == kind,
                    CreditMovementRequest.status == CreditMovementStatus.POSTED,
                )
            )
            or 0
        )

    @staticmethod
    def _reconcile(
        session: Session,
        order: OfflineCashOrder,
    ) -> CashCreditReconciliationRow:
        movement = session.scalar(
            select(CreditMovementRequest).where(
                CreditMovementRequest.cash_order_id == order.id,
                CreditMovementRequest.kind == CreditMovementKind.ALLOCATION,
                CreditMovementRequest.status == CreditMovementStatus.POSTED,
            )
        )
        amount_kind = "actual" if order.reported_amount_fen is not None else "estimate"
        display_amount = order.reported_amount_fen or order.estimated_amount_fen
        assert display_amount is not None
        if order.status != CashOrderStatus.CONFIRMED:
            state = "cash_unconfirmed"
        elif movement is None:
            state = "cash_confirmed_credit_unposted"
        else:
            state = "matched"
        return CashCreditReconciliationRow(
            cash_order_id=order.id,
            payer_account_id=order.payer_account_id,
            payee_account_id=order.payee_account_id,
            amount_kind=amount_kind,
            display_amount_fen=display_amount,
            confirmed_amount_fen=order.confirmed_amount_fen,
            pricing_version_id=order.pricing_version_id,
            allocation_request_id=movement.id if movement else None,
            allocated_credit_units=movement.credit_units if movement else 0,
            reconciliation_state=state,
        )
```

These queries name only account, relation, cash, credit-movement, wallet/ledger, and support tables. They never import an intelligence, media, publication-detail, project-profile, source-asset, voice, or consent model.

- [ ] **Step 4: Run reporting and privacy tests**

Run: `cd backend && uv run pytest tests/integration/resellers/test_reports.py tests/security/test_reseller_privacy.py -v`

Expected: PASS; cash estimates and confirmed cash remain separate from credit and content fields cannot appear in strict DTOs.

- [ ] **Step 5: Commit privacy-safe reporting**

```bash
git add backend/src/ip_saas/modules/resellers/reporting.py backend/tests/integration/resellers/test_reports.py
git commit -m "feat: add reseller reconciliation reports"
```

### Task 9: Create the single reversible 0005 migration and register metadata

**Files:**
- Create: `backend/migrations/versions/0005_resellers.py`
- Modify: `backend/src/ip_saas/db/base.py`
- Modify: `backend/tests/integration/test_health_and_migrations.py`

- [ ] **Step 1: Register the new models with Plan 01 metadata**

Inside `load_all_models()` in `backend/src/ip_saas/db/base.py`, add these imports:

```python
from ip_saas.modules.billing.models import GenerationLimitVersion
from ip_saas.modules.resellers.models import (
    AccountMigrationRequest,
    CommercialEvidenceAsset,
    CreditMovementRequest,
    OfflineCashOrder,
    ResellerPriceVersion,
    SupportGrant,
    SupportGrantRevocation,
)
```

In the existing model tuple passed to `assert all`, insert these exact entries after `ProviderCostEntry` and before the closing parenthesis; retain every Plan 01–04 entry:

```python
GenerationLimitVersion,
AccountMigrationRequest,
CommercialEvidenceAsset,
CreditMovementRequest,
OfflineCashOrder,
ResellerPriceVersion,
SupportGrant,
SupportGrantRevocation,
```

- [ ] **Step 2: Write the complete migration**

Create `backend/migrations/versions/0005_resellers.py`:

```python
"""reseller operations, privacy grants, migration, and generation limits

Revision ID: 0005_resellers
Revises: 0004_publication
"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects import postgresql

revision: str = "0005_resellers"
down_revision: str | None = "0004_publication"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None


def identity_columns() -> tuple[sa.Column, sa.Column]:
    return (
        sa.Column("id", sa.UUID(), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
    )


def upgrade() -> None:
    op.create_table(
        "generation_limit_versions",
        *identity_columns(),
        sa.Column("account_id", sa.UUID(), sa.ForeignKey("accounts.id"), nullable=True),
        sa.Column(
            "cost_center_id",
            sa.UUID(),
            sa.ForeignKey("internal_cost_centers.id"),
            nullable=True,
        ),
        sa.Column("billing_mode", sa.String(24), nullable=False),
        sa.Column("version_no", sa.Integer(), nullable=False),
        sa.Column("per_task_amount", sa.BigInteger(), nullable=False),
        sa.Column("per_day_amount", sa.BigInteger(), nullable=False),
        sa.Column("per_month_amount", sa.BigInteger(), nullable=False),
        sa.Column("effective_at", sa.DateTime(timezone=True), nullable=False),
        sa.Column("created_by_principal_id", sa.UUID(), nullable=False),
        sa.CheckConstraint(
            "(account_id is not null and cost_center_id is null "
            "and billing_mode = 'customer_credit') or "
            "(account_id is null and cost_center_id is not null "
            "and billing_mode = 'internal_cost')",
            name="ck_generation_limit_one_subject",
        ),
        sa.CheckConstraint(
            "per_task_amount > 0 and per_day_amount >= per_task_amount "
            "and per_month_amount >= per_day_amount",
            name="ck_generation_limit_ordered_caps",
        ),
        sa.PrimaryKeyConstraint("id"),
    )
    op.create_index(
        "ix_generation_limit_versions_account_id",
        "generation_limit_versions",
        ["account_id"],
    )
    op.create_index(
        "ix_generation_limit_versions_cost_center_id",
        "generation_limit_versions",
        ["cost_center_id"],
    )
    op.create_index(
        "ix_generation_limit_versions_effective_at",
        "generation_limit_versions",
        ["effective_at"],
    )
    op.create_index(
        "uq_generation_limit_account_version",
        "generation_limit_versions",
        ["account_id", "version_no"],
        unique=True,
        postgresql_where=sa.text("account_id is not null"),
    )
    op.create_index(
        "uq_generation_limit_center_version",
        "generation_limit_versions",
        ["cost_center_id", "version_no"],
        unique=True,
        postgresql_where=sa.text("cost_center_id is not null"),
    )

    op.create_table(
        "commercial_evidence_assets",
        *identity_columns(),
        sa.Column("owner_account_id", sa.UUID(), sa.ForeignKey("accounts.id"), nullable=False),
        sa.Column("object_key", sa.String(500), nullable=False),
        sa.Column("sha256", sa.String(64), nullable=False),
        sa.Column("content_type", sa.String(100), nullable=False),
        sa.Column("size_bytes", sa.BigInteger(), nullable=False),
        sa.Column("uploaded_by", sa.UUID(), nullable=False),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint("object_key"),
    )
    op.create_index(
        "ix_commercial_evidence_assets_owner_account_id",
        "commercial_evidence_assets",
        ["owner_account_id"],
    )

    op.create_table(
        "reseller_price_versions",
        *identity_columns(),
        sa.Column("seller_account_id", sa.UUID(), sa.ForeignKey("accounts.id"), nullable=False),
        sa.Column("buyer_kind", sa.String(32), nullable=False),
        sa.Column("product_code", sa.String(80), nullable=False),
        sa.Column("version_no", sa.Integer(), nullable=False),
        sa.Column("package_credit_units", sa.BigInteger(), nullable=False),
        sa.Column("package_price_fen", sa.BigInteger(), nullable=False),
        sa.Column("minimum_downstream_price_fen", sa.BigInteger(), nullable=True),
        sa.Column("platform_floor_fen", sa.BigInteger(), nullable=False),
        sa.Column("effective_at", sa.DateTime(timezone=True), nullable=False),
        sa.Column("promotion_code", sa.String(40), nullable=True),
        sa.Column("created_by", sa.UUID(), nullable=False),
        sa.CheckConstraint(
            "package_credit_units > 0",
            name="ck_reseller_price_credits_positive",
        ),
        sa.CheckConstraint(
            "package_price_fen > 0",
            name="ck_reseller_price_fen_positive",
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "seller_account_id",
            "buyer_kind",
            "product_code",
            "version_no",
            name="uq_reseller_price_version",
        ),
    )
    op.create_index(
        "ix_reseller_price_versions_seller_account_id",
        "reseller_price_versions",
        ["seller_account_id"],
    )
    op.create_index(
        "ix_reseller_price_versions_buyer_kind",
        "reseller_price_versions",
        ["buyer_kind"],
    )
    op.create_index(
        "ix_reseller_price_versions_product_code",
        "reseller_price_versions",
        ["product_code"],
    )
    op.create_index(
        "ix_reseller_price_versions_effective_at",
        "reseller_price_versions",
        ["effective_at"],
    )

    op.create_table(
        "offline_cash_orders",
        *identity_columns(),
        sa.Column("payer_account_id", sa.UUID(), sa.ForeignKey("accounts.id"), nullable=False),
        sa.Column("payee_account_id", sa.UUID(), sa.ForeignKey("accounts.id"), nullable=False),
        sa.Column("reported_amount_fen", sa.BigInteger(), nullable=True),
        sa.Column("estimated_amount_fen", sa.BigInteger(), nullable=True),
        sa.Column("confirmed_amount_fen", sa.BigInteger(), nullable=True),
        sa.Column(
            "evidence_asset_id",
            sa.UUID(),
            sa.ForeignKey("commercial_evidence_assets.id"),
            nullable=False,
        ),
        sa.Column(
            "pricing_version_id",
            sa.UUID(),
            sa.ForeignKey("reseller_price_versions.id"),
            nullable=False,
        ),
        sa.Column("note", sa.Text(), nullable=False),
        sa.Column("status", sa.String(20), nullable=False),
        sa.Column("idempotency_key", sa.String(160), nullable=False),
        sa.Column(
            "recorded_by_account_id",
            sa.UUID(),
            sa.ForeignKey("accounts.id"),
            nullable=False,
        ),
        sa.Column("recorded_by", sa.UUID(), nullable=False),
        sa.Column("confirmed_by", sa.UUID(), nullable=True),
        sa.Column("confirmed_at", sa.DateTime(timezone=True), nullable=True),
        sa.CheckConstraint(
            "(reported_amount_fen is null) <> (estimated_amount_fen is null)",
            name="ck_cash_order_one_reported_amount",
        ),
        sa.CheckConstraint(
            "confirmed_amount_fen is null or confirmed_amount_fen > 0",
            name="ck_cash_order_confirmed_amount",
        ),
        sa.CheckConstraint(
            "status in ('recorded', 'confirmed', 'cancelled')",
            name="ck_cash_order_status",
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "recorded_by_account_id",
            "idempotency_key",
            name="uq_cash_order_account_idempotency",
        ),
    )
    op.create_index(
        "ix_offline_cash_orders_payer_account_id",
        "offline_cash_orders",
        ["payer_account_id"],
    )
    op.create_index(
        "ix_offline_cash_orders_payee_account_id",
        "offline_cash_orders",
        ["payee_account_id"],
    )
    op.create_index(
        "ix_offline_cash_orders_recorded_by_account_id",
        "offline_cash_orders",
        ["recorded_by_account_id"],
    )
    op.create_index("ix_offline_cash_orders_status", "offline_cash_orders", ["status"])

    op.create_table(
        "credit_movement_requests",
        *identity_columns(),
        sa.Column("kind", sa.String(20), nullable=False),
        sa.Column("source_account_id", sa.UUID(), sa.ForeignKey("accounts.id"), nullable=False),
        sa.Column(
            "destination_account_id",
            sa.UUID(),
            sa.ForeignKey("accounts.id"),
            nullable=False,
        ),
        sa.Column("credit_units", sa.BigInteger(), nullable=False),
        sa.Column(
            "cash_order_id",
            sa.UUID(),
            sa.ForeignKey("offline_cash_orders.id"),
            nullable=True,
        ),
        sa.Column(
            "pricing_version_id",
            sa.UUID(),
            sa.ForeignKey("reseller_price_versions.id"),
            nullable=True,
        ),
        sa.Column("idempotency_key", sa.String(160), nullable=False),
        sa.Column("status", sa.String(20), nullable=False),
        sa.Column("reason", sa.Text(), nullable=True),
        sa.Column("requested_by", sa.UUID(), nullable=False),
        sa.Column("decided_by", sa.UUID(), nullable=True),
        sa.Column("decided_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column("ledger_transaction_id", sa.UUID(), nullable=True),
        sa.CheckConstraint("credit_units > 0", name="ck_credit_movement_positive"),
        sa.CheckConstraint(
            "kind in ('allocation', 'return')",
            name="ck_credit_movement_kind",
        ),
        sa.CheckConstraint(
            "status in ('requested', 'rejected', 'posted')",
            name="ck_credit_movement_status",
        ),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "source_account_id",
            "idempotency_key",
            name="uq_credit_movement_account_idempotency",
        ),
    )
    op.create_index(
        "ix_credit_movement_requests_source_account_id",
        "credit_movement_requests",
        ["source_account_id"],
    )
    op.create_index(
        "ix_credit_movement_requests_destination_account_id",
        "credit_movement_requests",
        ["destination_account_id"],
    )
    op.create_index(
        "uq_posted_allocation_cash_order",
        "credit_movement_requests",
        ["cash_order_id"],
        unique=True,
        postgresql_where=sa.text("kind = 'allocation' AND status = 'posted'"),
    )

    op.create_table(
        "support_grants",
        *identity_columns(),
        sa.Column("project_id", sa.UUID(), sa.ForeignKey("ip_projects.id"), nullable=False),
        sa.Column(
            "customer_account_id",
            sa.UUID(),
            sa.ForeignKey("accounts.id"),
            nullable=False,
        ),
        sa.Column(
            "reseller_account_id",
            sa.UUID(),
            sa.ForeignKey("accounts.id"),
            nullable=False,
        ),
        sa.Column("scopes", postgresql.JSONB(astext_type=sa.Text()), nullable=False),
        sa.Column("reason", sa.Text(), nullable=False),
        sa.Column("expires_at", sa.DateTime(timezone=True), nullable=False),
        sa.Column("granted_by", sa.UUID(), nullable=False),
        sa.Column("idempotency_key", sa.String(160), nullable=False),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint(
            "customer_account_id",
            "idempotency_key",
            name="uq_support_grant_account_idempotency",
        ),
    )
    op.create_index("ix_support_grants_project_id", "support_grants", ["project_id"])
    op.create_index(
        "ix_support_grants_customer_account_id",
        "support_grants",
        ["customer_account_id"],
    )
    op.create_index(
        "ix_support_grants_reseller_account_id",
        "support_grants",
        ["reseller_account_id"],
    )
    op.create_index("ix_support_grants_expires_at", "support_grants", ["expires_at"])

    op.create_table(
        "support_grant_revocations",
        *identity_columns(),
        sa.Column("grant_id", sa.UUID(), sa.ForeignKey("support_grants.id"), nullable=False),
        sa.Column("revoked_by", sa.UUID(), nullable=False),
        sa.Column("reason", sa.Text(), nullable=False),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint("grant_id", name="uq_support_grant_revocation"),
    )
    op.create_index(
        "ix_support_grant_revocations_grant_id",
        "support_grant_revocations",
        ["grant_id"],
    )

    op.create_table(
        "account_migration_requests",
        *identity_columns(),
        sa.Column(
            "subject_account_id",
            sa.UUID(),
            sa.ForeignKey("accounts.id"),
            nullable=False,
        ),
        sa.Column(
            "old_parent_account_id",
            sa.UUID(),
            sa.ForeignKey("accounts.id"),
            nullable=False,
        ),
        sa.Column(
            "new_parent_account_id",
            sa.UUID(),
            sa.ForeignKey("accounts.id"),
            nullable=False,
        ),
        sa.Column("reason", sa.Text(), nullable=False),
        sa.Column("status", sa.String(20), nullable=False),
        sa.Column("requested_by", sa.UUID(), nullable=False),
        sa.Column("approved_by", sa.UUID(), nullable=True),
        sa.Column("approved_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column("rejected_by", sa.UUID(), nullable=True),
        sa.Column("rejection_reason", sa.Text(), nullable=True),
        sa.Column("completed_by", sa.UUID(), nullable=True),
        sa.Column("completed_at", sa.DateTime(timezone=True), nullable=True),
        sa.CheckConstraint(
            "old_parent_account_id <> new_parent_account_id",
            name="ck_migration_parent_changes",
        ),
        sa.CheckConstraint(
            "status in ('requested', 'approved', 'completed', 'rejected')",
            name="ck_account_migration_status",
        ),
        sa.PrimaryKeyConstraint("id"),
    )
    op.create_index(
        "ix_account_migration_requests_subject_account_id",
        "account_migration_requests",
        ["subject_account_id"],
    )
    op.create_index(
        "uq_open_account_migration",
        "account_migration_requests",
        ["subject_account_id"],
        unique=True,
        postgresql_where=sa.text("status IN ('requested', 'approved')"),
    )


def downgrade() -> None:
    op.drop_table("account_migration_requests")
    op.drop_table("support_grant_revocations")
    op.drop_table("support_grants")
    op.drop_table("credit_movement_requests")
    op.drop_table("offline_cash_orders")
    op.drop_table("reseller_price_versions")
    op.drop_table("commercial_evidence_assets")
    op.drop_table("generation_limit_versions")
```

- [ ] **Step 3: Add exact upgrade/parity/downgrade coverage**

Append to `backend/tests/integration/test_health_and_migrations.py`:

```python
def test_reseller_migration_is_0005_over_0004_and_reversible(
    pg_engine: Engine,
    test_database_url: str,
) -> None:
    config = alembic_config(test_database_url)
    command.downgrade(config, "0004_publication")
    before = set(inspect(pg_engine).get_table_names())
    command.upgrade(config, "0005_resellers")
    after = set(inspect(pg_engine).get_table_names())
    assert after - before == {
        "generation_limit_versions",
        "commercial_evidence_assets",
        "reseller_price_versions",
        "offline_cash_orders",
        "credit_movement_requests",
        "support_grants",
        "support_grant_revocations",
        "account_migration_requests",
    }
    command.check(config)
    command.downgrade(config, "0004_publication")
    assert set(inspect(pg_engine).get_table_names()) == before
    command.upgrade(config, "0005_resellers")
```

- [ ] **Step 4: Run migration and metadata checks**

Run:

```bash
cd backend
DATABASE_URL=postgresql+psycopg://ip_saas:ip_saas@localhost:5432/ip_saas_test \
TEST_DATABASE_URL=postgresql+psycopg://ip_saas:ip_saas@localhost:5432/ip_saas_test \
uv run pytest tests/integration/test_health_and_migrations.py -v
uv run alembic heads
```

Expected: tests PASS, `alembic check` reports no operations, and `alembic heads` prints exactly `0005_resellers (head)`.

- [ ] **Step 5: Verify no conflicting migration number exists**

Run: `rg -n 'revision.*0005|down_revision.*0004' backend/migrations/versions`

Expected: only `0005_resellers.py` declares revision `0005_resellers` and down revision `0004_publication`.

- [ ] **Step 6: Commit the single migration**

```bash
git add backend/migrations/versions/0005_resellers.py backend/src/ip_saas/db/base.py backend/tests/integration/test_health_and_migrations.py
git commit -m "feat: add reseller operations migration"
```

### Task 10: Expose synchronous APIs, limit status, and a frozen OpenAPI contract

**Files:**
- Create: `backend/src/ip_saas/modules/resellers/router.py`
- Modify: `backend/src/ip_saas/api.py`
- Modify: `backend/tests/contract/test_openapi.py`
- Modify: `contracts/openapi.json`

- [ ] **Step 1: Extend the failing OpenAPI route assertion**

Add this test to `backend/tests/contract/test_openapi.py`:

```python
def test_reseller_routes_are_present() -> None:
    paths = create_app().openapi()["paths"]
    assert {
        "/v1/reseller/customers",
        "/v1/reseller/evidence",
        "/v1/reseller/evidence/{asset_id}",
        "/v1/reseller/cash-orders",
        "/v1/reseller/cash-orders/{order_id}/confirm",
        "/v1/reseller/credit-allocations",
        "/v1/reseller/return-requests",
        "/v1/reseller/return-requests/{request_id}/approve",
        "/v1/reseller/return-requests/{request_id}/reject",
        "/v1/reseller/prices",
        "/v1/reseller/report",
        "/v1/projects/{project_id}/support-grants",
        "/v1/projects/{project_id}/support-grants/{grant_id}/revoke",
        "/v1/account-migrations",
        "/v1/platform/account-migrations/{request_id}/approve",
        "/v1/platform/account-migrations/{request_id}/execute",
        "/v1/platform/accounts/{account_id}/suspend",
        "/v1/billing/generation-limits/me",
        "/v1/platform/generation-limits/accounts/{account_id}",
        "/v1/platform/generation-limits/cost-centers/{cost_center_id}",
        "/v1/platform/generation-limits/cost-centers/{cost_center_id}/status",
    } <= set(paths)
```

- [ ] **Step 2: Run the contract test and verify missing routes**

Run: `cd backend && uv run pytest tests/contract/test_openapi.py::test_reseller_routes_are_present -v`

Expected: FAIL because the reseller routes are absent.

- [ ] **Step 3: Create strict route DTOs, dependency factories, and handlers**

Create `backend/src/ip_saas/modules/resellers/router.py`:

```python
from datetime import datetime
from typing import Annotated
from uuid import UUID

import tos
from fastapi import APIRouter, Body, Depends, Header, Query
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.common.clock import SystemClock
from ip_saas.common.errors import DomainError, Forbidden
from ip_saas.config import Settings, get_settings
from ip_saas.db.session import get_session
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.dependencies import get_actor
from ip_saas.modules.accounts.models import AccountKind
from ip_saas.modules.accounts.security import Passwords
from ip_saas.modules.accounts.service import AccountService
from ip_saas.modules.billing.limits import GenerationLimitService, GenerationLimitStatus
from ip_saas.modules.projects.access import ProjectAccessService
from ip_saas.providers.object_store import PrivateObjectStore
from ip_saas.providers.tos_object_store import TosPrivateObjectStore

from .access import ResellerTreeAccess
from .adapters import FoundationCreditMovementAdapter
from .evidence import CommercialEvidenceService
from .lifecycle import ResellerLifecycleService
from .models import (
    AccountMigrationRequest,
    CommercialEvidenceAsset,
    CreditMovementRequest,
    OfflineCashOrder,
    ResellerPriceVersion,
    SupportGrant,
    SupportGrantRevocation,
)
from .pricing import PricingService
from .reporting import CustomerCommercialSummary, ResellerReport, ResellerReportService
from .schemas import (
    CashConfirmation,
    CreditAllocationCreate,
    CreditReturnCreate,
    MigrationCreate,
    OfflineCashOrderCreate,
    ResellerPriceCreate,
    ReturnRejection,
    SupportGrantCreate,
    SupportRevocationCreate,
)
from .service import CashOrderService, CreditMovementService
from .support import SupportGrantService


reseller_router = APIRouter(prefix="/v1/reseller", tags=["reseller"])
support_router = APIRouter(prefix="/v1/projects", tags=["project-support"])
migration_router = APIRouter(prefix="/v1", tags=["account-migrations"])
limit_router = APIRouter(prefix="/v1", tags=["generation-limits"])


class ApiModel(BaseModel):
    model_config = ConfigDict(extra="forbid", from_attributes=True)


class ChildAccountCreate(ApiModel):
    child_kind: AccountKind
    display_name: str = Field(min_length=1, max_length=120)
    login_name: str = Field(min_length=3, max_length=190)
    initial_password: str = Field(min_length=12, max_length=200)


class ChildAccountResponse(ApiModel):
    account_id: UUID
    kind: str
    display_name: str


class EvidenceResponse(ApiModel):
    id: UUID
    content_type: str
    size_bytes: int
    sha256: str


class SignedEvidenceResponse(ApiModel):
    url: str
    expires_seconds: int


class CashOrderResponse(ApiModel):
    id: UUID
    payer_account_id: UUID
    payee_account_id: UUID
    reported_amount_fen: int | None
    estimated_amount_fen: int | None
    confirmed_amount_fen: int | None
    pricing_version_id: UUID
    status: str


class MovementResponse(ApiModel):
    id: UUID
    kind: str
    source_account_id: UUID
    destination_account_id: UUID
    credit_units: int
    status: str
    ledger_transaction_id: UUID | None


class PriceResponse(ApiModel):
    id: UUID
    seller_account_id: UUID
    buyer_kind: str
    product_code: str
    version_no: int
    package_credit_units: int
    package_price_fen: int
    minimum_downstream_price_fen: int | None
    platform_floor_fen: int
    effective_at: datetime


class SupportGrantResponse(ApiModel):
    id: UUID
    project_id: UUID
    reseller_account_id: UUID
    scopes: list[str]
    expires_at: datetime


class MigrationResponse(ApiModel):
    id: UUID
    subject_account_id: UUID
    old_parent_account_id: UUID
    new_parent_account_id: UUID
    status: str


class SuspendCreate(ApiModel):
    reason: str = Field(min_length=1, max_length=500)


class LimitCreate(ApiModel):
    per_task_amount: int = Field(gt=0)
    per_day_amount: int = Field(gt=0)
    per_month_amount: int = Field(gt=0)
    effective_at: datetime


class LimitStatusResponse(ApiModel):
    version_id: UUID
    billing_mode: str
    per_task_amount: int
    per_day_amount: int
    per_month_amount: int
    used_day_amount: int
    used_month_amount: int


def audit() -> AuditWriter:
    return AuditWriter(SystemClock())


def tree() -> ResellerTreeAccess:
    return ResellerTreeAccess()


def prices() -> PricingService:
    return PricingService(tree(), audit(), SystemClock())


def cash_orders() -> CashOrderService:
    return CashOrderService(tree(), prices(), audit(), SystemClock())


def movements() -> CreditMovementService:
    return CreditMovementService(
        tree(),
        FoundationCreditMovementAdapter(),
        audit(),
        SystemClock(),
    )


def lifecycle() -> ResellerLifecycleService:
    return ResellerLifecycleService(tree(), SystemClock(), audit())


def private_store(
    settings: Annotated[Settings, Depends(get_settings)],
) -> PrivateObjectStore:
    access_key = settings.tos_access_key.get_secret_value()
    secret_key = settings.tos_secret_key.get_secret_value()
    if not access_key or not secret_key:
        raise DomainError("private object storage is not configured")
    client = tos.TosClientV2(
        access_key,
        secret_key,
        settings.tos_endpoint,
        settings.tos_region,
    )
    return TosPrivateObjectStore(client, settings.tos_bucket)


def limit_response(value: GenerationLimitStatus) -> LimitStatusResponse:
    return LimitStatusResponse(
        version_id=value.version_id,
        billing_mode=value.billing_mode,
        per_task_amount=value.per_task_amount,
        per_day_amount=value.per_day_amount,
        per_month_amount=value.per_month_amount,
        used_day_amount=value.used_day_amount,
        used_month_amount=value.used_month_amount,
    )


@reseller_router.post("/customers", response_model=ChildAccountResponse, status_code=201)
def create_child_account(
    payload: ChildAccountCreate,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> ChildAccountResponse:
    account = AccountService().create_child_account(
        session,
        actor,
        actor.account_id,
        payload.child_kind,
        payload.display_name,
    )
    AccountService().add_principal(
        session,
        account.id,
        payload.login_name,
        Passwords().hash(payload.initial_password),
    )
    audit().write(
        session,
        actor=actor,
        action="child_account.created",
        target_type="account",
        target_id=account.id,
        metadata={"kind": account.kind},
    )
    return ChildAccountResponse(
        account_id=account.id,
        kind=account.kind,
        display_name=account.display_name,
    )


@reseller_router.post("/evidence", response_model=EvidenceResponse, status_code=201)
def upload_evidence(
    filename: Annotated[str, Query(min_length=1, max_length=180)],
    content_type: Annotated[str, Header(alias="content-type")],
    body: Annotated[bytes, Body(media_type="application/octet-stream")],
    store: Annotated[PrivateObjectStore, Depends(private_store)],
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> EvidenceResponse:
    row = CommercialEvidenceService(store, audit()).upload(
        session,
        actor,
        filename,
        content_type,
        body,
    )
    return EvidenceResponse.model_validate(row)


@reseller_router.get("/evidence/{asset_id}", response_model=SignedEvidenceResponse)
def read_evidence(
    asset_id: UUID,
    store: Annotated[PrivateObjectStore, Depends(private_store)],
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> SignedEvidenceResponse:
    url = CommercialEvidenceService(store, audit()).signed_read(
        session,
        actor,
        asset_id,
    )
    return SignedEvidenceResponse(url=url, expires_seconds=300)


@reseller_router.post("/cash-orders", response_model=CashOrderResponse, status_code=201)
def record_cash_order(
    payload: OfflineCashOrderCreate,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> CashOrderResponse:
    return CashOrderResponse.model_validate(cash_orders().record(session, actor, payload))


@reseller_router.post(
    "/cash-orders/{order_id}/confirm",
    response_model=CashOrderResponse,
)
def confirm_cash_order(
    order_id: UUID,
    payload: CashConfirmation,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> CashOrderResponse:
    return CashOrderResponse.model_validate(
        cash_orders().confirm(session, actor, order_id, payload)
    )


@reseller_router.post(
    "/credit-allocations",
    response_model=MovementResponse,
    status_code=201,
)
def allocate_credits(
    payload: CreditAllocationCreate,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> MovementResponse:
    return MovementResponse.model_validate(movements().allocate(session, actor, payload))


@reseller_router.post(
    "/return-requests",
    response_model=MovementResponse,
    status_code=201,
)
def request_return(
    payload: CreditReturnCreate,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> MovementResponse:
    return MovementResponse.model_validate(
        movements().request_return(session, actor, payload)
    )


@reseller_router.post(
    "/return-requests/{request_id}/approve",
    response_model=MovementResponse,
)
def approve_return(
    request_id: UUID,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> MovementResponse:
    return MovementResponse.model_validate(
        movements().approve_return(session, actor, request_id)
    )


@reseller_router.post(
    "/return-requests/{request_id}/reject",
    response_model=MovementResponse,
)
def reject_return(
    request_id: UUID,
    payload: ReturnRejection,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> MovementResponse:
    return MovementResponse.model_validate(
        movements().reject_return(session, actor, request_id, payload.reason)
    )


@reseller_router.post("/prices", response_model=PriceResponse, status_code=201)
def create_price(
    payload: ResellerPriceCreate,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> PriceResponse:
    return PriceResponse.model_validate(prices().create_version(session, actor, payload))


@reseller_router.get("/customers", response_model=list[CustomerCommercialSummary])
def list_customers(
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> list[CustomerCommercialSummary]:
    return ResellerReportService(
        tree(), SystemClock(), audit()
    ).build(session, actor).customers


@reseller_router.get("/report", response_model=ResellerReport)
def reseller_report(
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> ResellerReport:
    return ResellerReportService(
        tree(), SystemClock(), audit()
    ).build(session, actor)


@support_router.post(
    "/{project_id}/support-grants",
    response_model=SupportGrantResponse,
    status_code=201,
)
def create_support_grant(
    project_id: UUID,
    payload: SupportGrantCreate,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> SupportGrantResponse:
    if payload.project_id != project_id:
        raise Forbidden("path and payload project IDs must match")
    row = SupportGrantService(
        ProjectAccessService(), tree(), SystemClock(), audit()
    ).create(session, actor, payload)
    return SupportGrantResponse.model_validate(row)


@support_router.post(
    "/{project_id}/support-grants/{grant_id}/revoke",
    status_code=204,
)
def revoke_support_grant(
    project_id: UUID,
    grant_id: UUID,
    payload: SupportRevocationCreate,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> None:
    ProjectAccessService().require_editor(session, actor, project_id)
    SupportGrantService(
        ProjectAccessService(), tree(), SystemClock(), audit()
    ).revoke(session, actor, grant_id, payload.reason)


@migration_router.post(
    "/account-migrations",
    response_model=MigrationResponse,
    status_code=201,
)
def request_account_migration(
    payload: MigrationCreate,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> MigrationResponse:
    return MigrationResponse.model_validate(
        lifecycle().request_migration(session, actor, payload)
    )


@migration_router.post(
    "/platform/account-migrations/{request_id}/approve",
    response_model=MigrationResponse,
)
def approve_account_migration(
    request_id: UUID,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> MigrationResponse:
    return MigrationResponse.model_validate(
        lifecycle().approve_migration(session, actor, request_id)
    )


@migration_router.post(
    "/platform/account-migrations/{request_id}/execute",
    response_model=MigrationResponse,
)
def execute_account_migration(
    request_id: UUID,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> MigrationResponse:
    return MigrationResponse.model_validate(
        lifecycle().execute_migration(session, actor, request_id)
    )


@migration_router.post("/platform/accounts/{account_id}/suspend", status_code=204)
def suspend_account(
    account_id: UUID,
    payload: SuspendCreate,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> None:
    lifecycle().suspend(session, actor, account_id, payload.reason)


@limit_router.get(
    "/billing/generation-limits/me",
    response_model=LimitStatusResponse,
)
def my_generation_limits(
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> LimitStatusResponse:
    return limit_response(
        GenerationLimitService(SystemClock()).customer_status(
            session,
            actor.account_id,
        )
    )


@limit_router.post(
    "/platform/generation-limits/accounts/{account_id}",
    response_model=LimitStatusResponse,
    status_code=201,
)
def create_account_limits(
    account_id: UUID,
    payload: LimitCreate,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> LimitStatusResponse:
    service = GenerationLimitService(SystemClock())
    service.create_account_version(
        session,
        actor,
        account_id,
        per_task_units=payload.per_task_amount,
        per_day_units=payload.per_day_amount,
        per_month_units=payload.per_month_amount,
        effective_at=payload.effective_at,
    )
    return limit_response(service.customer_status(session, account_id))


@limit_router.post(
    "/platform/generation-limits/cost-centers/{cost_center_id}",
    response_model=LimitStatusResponse,
    status_code=201,
)
def create_internal_limits(
    cost_center_id: UUID,
    payload: LimitCreate,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> LimitStatusResponse:
    service = GenerationLimitService(SystemClock())
    service.create_internal_version(
        session,
        actor,
        cost_center_id,
        per_task_fen=payload.per_task_amount,
        per_day_fen=payload.per_day_amount,
        per_month_fen=payload.per_month_amount,
        effective_at=payload.effective_at,
    )
    return limit_response(service.internal_status(session, cost_center_id))


@limit_router.get(
    "/platform/generation-limits/cost-centers/{cost_center_id}/status",
    response_model=LimitStatusResponse,
)
def internal_limit_status(
    cost_center_id: UUID,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> LimitStatusResponse:
    if actor.kind != ActorKind.PLATFORM_ADMIN:
        raise Forbidden("platform admin is required")
    return limit_response(
        GenerationLimitService(SystemClock()).internal_status(
            session,
            cost_center_id,
        )
    )
```

No handler uses `with session.begin()`. The Plan 01 dependency commits or rolls back the request exactly once.

- [ ] **Step 4: Compose routers once**

Add to `backend/src/ip_saas/api.py`:

```python
from ip_saas.modules.resellers.router import (
    limit_router,
    migration_router,
    reseller_router,
    support_router,
)

# In create_app(), after the Plan 04 routers:
app.include_router(reseller_router)
app.include_router(support_router)
app.include_router(migration_router)
app.include_router(limit_router)
```

- [ ] **Step 5: Export and verify OpenAPI**

Run:

```bash
cd backend
uv run python -m ip_saas.scripts.export_contracts
uv run pytest tests/contract/test_openapi.py -v
uv run python -m ip_saas.scripts.export_contracts --check
```

Expected: PASS; all listed routes exist, every request schema rejects extra fields, and contract drift mode exits 0.

- [ ] **Step 6: Run a transaction-boundary source check**

Run: `rg -n 'AsyncSession|async def|session\.begin|session\.commit|session\.rollback' backend/src/ip_saas/modules/resellers backend/src/ip_saas/modules/billing/limits.py`

Expected: no output.

- [ ] **Step 7: Commit API and contract**

```bash
git add backend/src/ip_saas/modules/resellers/router.py backend/src/ip_saas/api.py backend/tests/contract/test_openapi.py contracts/openapi.json
git commit -m "feat: expose reseller operations APIs"
```

### Task 11: Build reseller, C-user support, platform migration, and limit-status UI

**Files:**
- Create: `frontend/src/features/resellers/api.ts`
- Create: `frontend/src/features/resellers/CustomerTable.tsx`
- Create: `frontend/src/features/resellers/OfflineOrderForm.tsx`
- Create: `frontend/src/features/resellers/CreditMovementPanel.tsx`
- Create: `frontend/src/features/resellers/PriceForm.tsx`
- Create: `frontend/src/features/resellers/SupportGrantPanel.tsx`
- Create: `frontend/src/features/resellers/MigrationRequestForm.tsx`
- Create: `frontend/src/features/resellers/LifecyclePanel.tsx`
- Create: `frontend/src/features/resellers/ReportTable.tsx`
- Create: `frontend/src/features/resellers/GenerationLimitStatusCard.tsx`
- Create: reseller/C-user/platform pages listed in the file map
- Modify: `frontend/src/lib/api/schema.d.ts`
- Test: `frontend/tests/e2e/reseller-operations.spec.ts`

- [ ] **Step 1: Regenerate API types and write the client**

Run: `cd frontend && pnpm generate:api`

Expected: `src/lib/api/schema.d.ts` contains `CashOrderResponse`, `MovementResponse`, `ResellerReport`, `SupportGrantResponse`, and `LimitStatusResponse`.

Create `frontend/src/features/resellers/api.ts`:

```typescript
import type {components} from "@/lib/api/schema";
import {apiRequest, requireAccessToken} from "@/lib/api/client";

export type CustomerSummary = components["schemas"]["CustomerCommercialSummary"];
export type ResellerReport = components["schemas"]["ResellerReport"];
export type CashOrder = components["schemas"]["CashOrderResponse"];
export type Movement = components["schemas"]["MovementResponse"];
export type LimitStatus = components["schemas"]["LimitStatusResponse"];
export type Price = components["schemas"]["PriceResponse"];
export type SupportGrant = components["schemas"]["SupportGrantResponse"];
export type Migration = components["schemas"]["MigrationResponse"];

export const resellerApi = {
  customers: () => apiRequest<CustomerSummary[]>("/v1/reseller/customers"),
  report: () => apiRequest<ResellerReport>("/v1/reseller/report"),
  myLimits: () => apiRequest<LimitStatus>("/v1/billing/generation-limits/me"),
  internalLimits: (costCenterId: string) =>
    apiRequest<LimitStatus>(
      "/v1/platform/generation-limits/cost-centers/" + costCenterId + "/status",
    ),
  uploadEvidence: async (file: File) => {
    const response = await fetch(
      "/api/v1/reseller/evidence?filename=" + encodeURIComponent(file.name),
      {
        method: "POST",
        credentials: "include",
        headers: {
          authorization: "Bearer " + requireAccessToken(),
          "content-type": file.type,
        },
        body: file,
      },
    );
    if (!response.ok) throw new Error("付款凭证上传失败");
    return response.json() as Promise<{id: string}>;
  },
  recordCash: (payload: object) =>
    apiRequest<CashOrder>("/v1/reseller/cash-orders", {
      method: "POST",
      body: payload,
    }),
  confirmCash: (orderId: string, receivedAmountFen: number) =>
    apiRequest<CashOrder>("/v1/reseller/cash-orders/" + orderId + "/confirm", {
      method: "POST",
      body: {received_amount_fen: receivedAmountFen},
    }),
  allocate: (payload: object) =>
    apiRequest<Movement>("/v1/reseller/credit-allocations", {
      method: "POST",
      body: payload,
    }),
  requestReturn: (payload: object) =>
    apiRequest<Movement>("/v1/reseller/return-requests", {
      method: "POST",
      body: payload,
    }),
  approveReturn: (requestId: string) =>
    apiRequest<Movement>("/v1/reseller/return-requests/" + requestId + "/approve", {
      method: "POST",
    }),
  rejectReturn: (requestId: string, reason: string) =>
    apiRequest<Movement>("/v1/reseller/return-requests/" + requestId + "/reject", {
      method: "POST",
      body: {reason},
    }),
  createPrice: (payload: object) =>
    apiRequest<Price>("/v1/reseller/prices", {
      method: "POST",
      body: payload,
    }),
  grantSupport: (projectId: string, payload: object) =>
    apiRequest<SupportGrant>("/v1/projects/" + projectId + "/support-grants", {
      method: "POST",
      body: payload,
    }),
  revokeSupport: (projectId: string, grantId: string, reason: string) =>
    apiRequest<void>(
      "/v1/projects/" + projectId + "/support-grants/" + grantId + "/revoke",
      {method: "POST", body: {reason}},
    ),
  requestMigration: (payload: object) =>
    apiRequest<Migration>("/v1/account-migrations", {
      method: "POST",
      body: payload,
    }),
  approveMigration: (requestId: string) =>
    apiRequest<Migration>("/v1/platform/account-migrations/" + requestId + "/approve", {
      method: "POST",
    }),
  executeMigration: (requestId: string) =>
    apiRequest<Migration>("/v1/platform/account-migrations/" + requestId + "/execute", {
      method: "POST",
    }),
  suspend: (accountId: string, reason: string) =>
    apiRequest<void>("/v1/platform/accounts/" + accountId + "/suspend", {
      method: "POST",
      body: {reason},
    }),
  setAccountLimits: (accountId: string, payload: object) =>
    apiRequest<LimitStatus>("/v1/platform/generation-limits/accounts/" + accountId, {
      method: "POST",
      body: payload,
    }),
  setInternalLimits: (costCenterId: string, payload: object) =>
    apiRequest<LimitStatus>(
      "/v1/platform/generation-limits/cost-centers/" + costCenterId,
      {method: "POST", body: payload},
    ),
};
```

- [ ] **Step 2: Build the limit card and privacy-safe customer table**

Create `frontend/src/features/resellers/GenerationLimitStatusCard.tsx`:

```tsx
"use client";

import {useEffect, useState} from "react";
import {resellerApi, type LimitStatus} from "./api";

export function GenerationLimitStatusCard({costCenterId}: {costCenterId?: string}) {
  const [status, setStatus] = useState<LimitStatus | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    const load = costCenterId
      ? resellerApi.internalLimits(costCenterId)
      : resellerApi.myLimits();
    load.then(setStatus).catch((reason: Error) => setError(reason.message));
  }, [costCenterId]);
  if (error) return <p role="alert">付费任务已暂停：{error}</p>;
  if (!status) return <p aria-busy="true">读取创作额度上限…</p>;
  const unit = status.billing_mode === "internal_cost" ? "人民币分" : "创作点";
  return (
    <section aria-label="生成费用上限">
      <h2>生成费用上限</h2>
      <dl>
        <dt>单任务</dt><dd>{status.per_task_amount} {unit}</dd>
        <dt>今日</dt><dd>{status.used_day_amount} / {status.per_day_amount} {unit}</dd>
        <dt>本月</dt><dd>{status.used_month_amount} / {status.per_month_amount} {unit}</dd>
      </dl>
      <p>达到任一上限后，新付费任务会被拒绝，不会继续产生供应商费用。</p>
    </section>
  );
}
```

Create `frontend/src/features/resellers/CustomerTable.tsx`:

```tsx
"use client";

import {useEffect, useState} from "react";
import {resellerApi, type CustomerSummary} from "./api";

export function CustomerTable() {
  const [rows, setRows] = useState<CustomerSummary[]>([]);
  const [error, setError] = useState("");
  useEffect(() => {
    resellerApi.customers().then(setRows).catch((reason: Error) => setError(reason.message));
  }, []);
  if (error) return <p role="alert">{error}</p>;
  return (
    <table>
      <caption>下级账户商业汇总，不含访谈、脚本、人物、声音或视频</caption>
      <thead>
        <tr>
          <th>账户</th><th>层级</th><th>状态</th><th>余额</th>
          <th>实际现金</th><th>估算现金</th><th>生成消耗</th><th>支持授权</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => (
          <tr key={row.account_id}>
            <td>{row.display_name}</td><td>{row.account_kind}</td>
            <td>{row.account_status}</td><td>{row.credit_balance}</td>
            <td>{row.actual_cash_fen} 分</td><td>{row.estimated_cash_fen} 分（估算）</td>
            <td>{row.generation_spend_units} 点</td>
            <td>{row.active_support_grant_count} 个有效范围</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
```

- [ ] **Step 3: Build separate cash-recording and credit-movement controls**

Create `frontend/src/features/resellers/OfflineOrderForm.tsx`:

```tsx
"use client";

import {useState, type FormEvent} from "react";
import {resellerApi} from "./api";

export function OfflineOrderForm() {
  const [message, setMessage] = useState("");
  const [orderId, setOrderId] = useState("");
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const file = data.get("evidence");
    if (!(file instanceof File)) return;
    const evidence = await resellerApi.uploadEvidence(file);
    const amountFen = Math.round(Number(data.get("amount_yuan")) * 100);
    const order = await resellerApi.recordCash({
      payer_account_id: String(data.get("payer_account_id")),
      payee_account_id: String(data.get("payee_account_id")),
      reported_amount_fen: data.get("is_estimate") ? null : amountFen,
      estimated_amount_fen: data.get("is_estimate") ? amountFen : null,
      evidence_asset_id: evidence.id,
      pricing_version_id: String(data.get("pricing_version_id")),
      note: String(data.get("note")),
      idempotency_key: crypto.randomUUID(),
    });
    setOrderId(order.id);
    setMessage("已记录，尚未充值。收款方确认后仍需单独划拨创作点。");
  }
  async function confirm(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    await resellerApi.confirmCash(
      String(data.get("order_id")),
      Math.round(Number(data.get("received_yuan")) * 100),
    );
    setMessage("现金已确认，创作点仍未划拨。");
  }
  return (
    <>
      <form onSubmit={submit}>
        <label>付款账户 ID<input name="payer_account_id" required /></label>
        <label>收款账户 ID<input name="payee_account_id" required /></label>
        <label>线下付款金额<input name="amount_yuan" inputMode="decimal" required /></label>
        <label><input name="is_estimate" type="checkbox" />金额仅为估算</label>
        <label>价格版本 ID<input name="pricing_version_id" required /></label>
        <label>付款凭证<input name="evidence" type="file" accept="image/png,image/jpeg,application/pdf" required /></label>
        <label>备注<input name="note" maxLength={500} /></label>
        <button type="submit">记录付款凭证</button>
      </form>
      <form onSubmit={confirm}>
        <label>现金订单 ID<input name="order_id" defaultValue={orderId} required /></label>
        <label>确认到账金额<input name="received_yuan" inputMode="decimal" required /></label>
        <button type="submit">确认线下到账</button>
      </form>
      {message ? <p role="status">{message}</p> : null}
    </>
  );
}
```

Create `frontend/src/features/resellers/CreditMovementPanel.tsx`:

```tsx
"use client";

import {useState, type FormEvent} from "react";
import {resellerApi} from "./api";

export function CreditMovementPanel() {
  const [message, setMessage] = useState("");
  async function allocate(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    await resellerApi.allocate({
      cash_order_id: String(data.get("cash_order_id")),
      credit_units: Number(data.get("credit_units")),
      idempotency_key: crypto.randomUUID(),
    });
    setMessage("划拨成功：已追加一笔双向平衡分录。");
  }
  async function requestReturn(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    await resellerApi.requestReturn({
      source_account_id: String(data.get("source_account_id")),
      credit_units: Number(data.get("return_units")),
      reason: String(data.get("reason")),
      idempotency_key: crypto.randomUUID(),
    });
    setMessage("返还申请已提交，直属上级批准前余额不变。");
  }
  async function decideReturn(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const requestId = String(data.get("request_id"));
    if (data.get("decision") === "approve") {
      await resellerApi.approveReturn(requestId);
      setMessage("返还已批准：已原子追加双向平衡分录。");
      return;
    }
    await resellerApi.rejectReturn(requestId, String(data.get("rejection_reason")));
    setMessage("返还已拒绝，双方余额未变。");
  }
  return (
    <>
      <form onSubmit={allocate}>
        <label>已确认现金订单 ID<input name="cash_order_id" required /></label>
        <label>划拨创作点<input name="credit_units" type="number" min={1} required /></label>
        <button type="submit">确认划拨</button>
      </form>
      <form onSubmit={requestReturn}>
        <label>返还来源账户 ID<input name="source_account_id" required /></label>
        <label>返还创作点<input name="return_units" type="number" min={1} required /></label>
        <label>返还原因<input name="reason" required maxLength={500} /></label>
        <button type="submit">提交返还申请</button>
      </form>
      <form onSubmit={decideReturn}>
        <label>返还申请 ID<input name="request_id" required /></label>
        <label>审批结果<select name="decision"><option value="approve">批准</option><option value="reject">拒绝</option></select></label>
        <label>拒绝原因<input name="rejection_reason" maxLength={500} /></label>
        <button type="submit">提交审批</button>
      </form>
      {message ? <p role="status">{message}</p> : null}
    </>
  );
}
```

- [ ] **Step 4: Build price and customer-owned support controls**

Create `frontend/src/features/resellers/PriceForm.tsx`:

```tsx
"use client";

import {useState, type FormEvent} from "react";
import {resellerApi} from "./api";

export function PriceForm() {
  const [message, setMessage] = useState("");
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const buyerKind = String(data.get("buyer_kind"));
    const row = await resellerApi.createPrice({
      seller_account_id: String(data.get("seller_account_id")),
      buyer_kind: buyerKind,
      product_code: String(data.get("product_code")),
      package_credit_units: Number(data.get("package_credit_units")),
      package_price_fen: Math.round(Number(data.get("package_price_yuan")) * 100),
      minimum_downstream_price_fen:
        buyerKind === "c_user"
          ? null
          : Math.round(Number(data.get("minimum_yuan")) * 100),
      effective_at: new Date(String(data.get("effective_at"))).toISOString(),
      promotion_code: null,
    });
    setMessage("价格版本 " + row.version_no + " 已冻结，仅用于未来订单。");
  }
  return (
    <form onSubmit={submit}>
      <label>卖方账户 ID<input name="seller_account_id" required /></label>
      <label>买方层级<select name="buyer_kind"><option value="reseller_l1">一级代理</option><option value="reseller_l2">二级代理</option><option value="c_user">C 端</option></select></label>
      <label>产品代码<input name="product_code" defaultValue="credits.standard" required /></label>
      <label>套餐创作点<input name="package_credit_units" type="number" min={1} required /></label>
      <label>套餐售价（元）<input name="package_price_yuan" inputMode="decimal" required /></label>
      <label>下游最低售价（元）<input name="minimum_yuan" inputMode="decimal" /></label>
      <label>生效时间<input name="effective_at" type="datetime-local" required /></label>
      <button type="submit">创建价格版本</button>
      {message ? <p role="status">{message}</p> : null}
    </form>
  );
}
```

Create `frontend/src/features/resellers/SupportGrantPanel.tsx`:

```tsx
"use client";

import {useState, type FormEvent} from "react";
import {resellerApi} from "./api";

export function SupportGrantPanel({projectId}: {projectId: string}) {
  const [message, setMessage] = useState("");
  async function grant(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const scopes = data.getAll("scopes").map(String);
    const grant = await resellerApi.grantSupport(projectId, {
      project_id: projectId,
      reseller_account_id: String(data.get("reseller_account_id")),
      scopes,
      expires_at: new Date(String(data.get("expires_at"))).toISOString(),
      reason: String(data.get("reason")),
      idempotency_key: crypto.randomUUID(),
    });
    setMessage("授权已创建：" + grant.scopes.join("、") + "，到期后自动失效。");
  }
  async function revoke(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    await resellerApi.revokeSupport(
      projectId,
      String(data.get("grant_id")),
      String(data.get("reason")),
    );
    setMessage("授权已立即撤销，后续读取将被拒绝。");
  }
  return (
    <>
      <form onSubmit={grant}>
        <p>代理默认看不到项目内容；这里只开放勾选范围，最长七天，所有读取都会审计。</p>
        <label>代理账户 ID<input name="reseller_account_id" required /></label>
        {["profile", "strategy", "content", "media", "publication"].map((scope) => (
          <label key={scope}><input type="checkbox" name="scopes" value={scope} />{scope}</label>
        ))}
        <label>到期时间<input name="expires_at" type="datetime-local" required /></label>
        <label>协助原因<input name="reason" required maxLength={500} /></label>
        <button type="submit">创建限时支持授权</button>
      </form>
      <form onSubmit={revoke}>
        <label>授权 ID<input name="grant_id" required /></label>
        <label>撤销原因<input name="reason" required maxLength={500} /></label>
        <button type="submit">立即撤销支持授权</button>
      </form>
      {message ? <p role="status">{message}</p> : null}
    </>
  );
}
```

Create `frontend/src/features/resellers/MigrationRequestForm.tsx`:

```tsx
"use client";

import {useState, type FormEvent} from "react";
import {resellerApi} from "./api";

export function MigrationRequestForm() {
  const [message, setMessage] = useState("");
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const request = await resellerApi.requestMigration({
      subject_account_id: String(data.get("subject_account_id")),
      new_parent_account_id: String(data.get("new_parent_account_id")),
      reason: String(data.get("reason")),
    });
    setMessage("迁移申请已提交：" + request.id + "。批准前层级不变。");
  }
  return (
    <form onSubmit={submit}>
      <h2>申请更换直属代理</h2>
      <label>待迁移账户 ID<input name="subject_account_id" required /></label>
      <label>新直属上级 ID<input name="new_parent_account_id" required /></label>
      <label>迁移原因<input name="reason" required maxLength={500} /></label>
      <button type="submit">提交迁移申请</button>
      {message ? <p role="status">{message}</p> : null}
    </form>
  );
}
```

- [ ] **Step 5: Build lifecycle, platform-limit, and reconciliation components**

Create `frontend/src/features/resellers/LifecyclePanel.tsx`:

```tsx
"use client";

import {useState, type FormEvent} from "react";
import {resellerApi} from "./api";
import {GenerationLimitStatusCard} from "./GenerationLimitStatusCard";

function limitPayload(data: FormData) {
  return {
    per_task_amount: Number(data.get("per_task")),
    per_day_amount: Number(data.get("per_day")),
    per_month_amount: Number(data.get("per_month")),
    effective_at: new Date(String(data.get("effective_at"))).toISOString(),
  };
}

export function LifecyclePanel() {
  const [message, setMessage] = useState("");
  const [costCenterId, setCostCenterId] = useState("");
  async function suspend(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    await resellerApi.suspend(String(data.get("account_id")), String(data.get("reason")));
    setMessage("账户已停用；历史项目、现金、价格、点数分录和审计仍保留。");
  }
  async function progress(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const id = String(data.get("request_id"));
    await resellerApi.approveMigration(id);
    await resellerApi.executeMigration(id);
    setMessage("迁移完成；余额未自动移动，旧代理支持授权已撤销。");
  }
  async function accountLimits(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    await resellerApi.setAccountLimits(String(data.get("subject_id")), limitPayload(data));
    setMessage("账户生成上限版本已生效。");
  }
  async function internalLimits(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const id = String(data.get("subject_id"));
    await resellerApi.setInternalLimits(id, limitPayload(data));
    setCostCenterId(id);
    setMessage("平台内部成本中心预算上限版本已生效。");
  }
  return (
    <>
      <form onSubmit={suspend}>
        <label>停用账户 ID<input name="account_id" required /></label>
        <label>停用原因<input name="reason" required /></label>
        <button type="submit">停用账户</button>
      </form>
      <form onSubmit={progress}>
        <label>迁移申请 ID<input name="request_id" required /></label>
        <button type="submit">批准并执行迁移</button>
      </form>
      <form onSubmit={accountLimits}>
        <h2>平台/代理/C 端创作点上限</h2>
        <LimitFields />
        <button type="submit">冻结账户上限版本</button>
      </form>
      <form onSubmit={internalLimits}>
        <h2>平台自营销内部人民币预算上限</h2>
        <LimitFields />
        <button type="submit">冻结成本中心上限版本</button>
      </form>
      {costCenterId ? <GenerationLimitStatusCard costCenterId={costCenterId} /> : null}
      {message ? <p role="status">{message}</p> : null}
    </>
  );
}

function LimitFields() {
  return (
    <>
      <label>账户或成本中心 ID<input name="subject_id" required /></label>
      <label>单任务上限<input name="per_task" type="number" min={1} required /></label>
      <label>单日上限<input name="per_day" type="number" min={1} required /></label>
      <label>单月上限<input name="per_month" type="number" min={1} required /></label>
      <label>生效时间<input name="effective_at" type="datetime-local" required /></label>
    </>
  );
}
```

Create `frontend/src/features/resellers/ReportTable.tsx`:

```tsx
"use client";

import {useEffect, useState} from "react";
import {resellerApi, type ResellerReport} from "./api";

export function ReportTable() {
  const [report, setReport] = useState<ResellerReport | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    resellerApi.report().then(setReport).catch((reason: Error) => setError(reason.message));
  }, []);
  if (error) return <p role="alert">{error}</p>;
  if (!report) return <p aria-busy="true">生成对账报表…</p>;
  return (
    <table>
      <caption>现金与创作点分账对照；估算金额不计作实际收款</caption>
      <thead>
        <tr><th>现金订单</th><th>金额性质</th><th>展示金额</th><th>确认金额</th><th>创作点</th><th>状态</th></tr>
      </thead>
      <tbody>
        {report.cash_credit_rows.map((row) => (
          <tr key={row.cash_order_id}>
            <td>{row.cash_order_id}</td>
            <td>{row.amount_kind === "estimate" ? "估算" : "实际申报"}</td>
            <td>{row.display_amount_fen} 分</td>
            <td>{row.confirmed_amount_fen ?? "未确认"}</td>
            <td>{row.allocated_credit_units}</td>
            <td>{row.reconciliation_state}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
```

- [ ] **Step 6: Create all route pages**

Create the following files:

```tsx
// frontend/src/app/(reseller)/reseller/customers/page.tsx
import {AppShell} from "@/features/shell/AppShell";
import {CustomerTable} from "@/features/resellers/CustomerTable";
import {GenerationLimitStatusCard} from "@/features/resellers/GenerationLimitStatusCard";
export default function Page() {
  return <AppShell title="下级账户" allowedKinds={["reseller_l1", "reseller_l2"]}><GenerationLimitStatusCard /><CustomerTable /></AppShell>;
}
```

```tsx
// frontend/src/app/(reseller)/reseller/orders/page.tsx
import {AppShell} from "@/features/shell/AppShell";
import {OfflineOrderForm} from "@/features/resellers/OfflineOrderForm";
export default function Page() {
  return <AppShell title="线下现金订单" allowedKinds={["reseller_l1", "reseller_l2"]}><OfflineOrderForm /></AppShell>;
}
```

```tsx
// frontend/src/app/(reseller)/reseller/credits/page.tsx
import {AppShell} from "@/features/shell/AppShell";
import {CreditMovementPanel} from "@/features/resellers/CreditMovementPanel";
export default function Page() {
  return <AppShell title="创作点划拨与返还" allowedKinds={["reseller_l1", "reseller_l2"]}><CreditMovementPanel /></AppShell>;
}
```

```tsx
// frontend/src/app/(reseller)/reseller/prices/page.tsx
import {AppShell} from "@/features/shell/AppShell";
import {PriceForm} from "@/features/resellers/PriceForm";
export default function Page() {
  return <AppShell title="未来订单价格" allowedKinds={["reseller_l1", "reseller_l2"]}><PriceForm /></AppShell>;
}
```

```tsx
// frontend/src/app/(reseller)/reseller/reports/page.tsx
import {AppShell} from "@/features/shell/AppShell";
import {ReportTable} from "@/features/resellers/ReportTable";
export default function Page() {
  return <AppShell title="现金与创作点对账" allowedKinds={["reseller_l1", "reseller_l2"]}><ReportTable /></AppShell>;
}
```

```tsx
// frontend/src/app/(reseller)/reseller/migration/page.tsx
import {AppShell} from "@/features/shell/AppShell";
import {MigrationRequestForm} from "@/features/resellers/MigrationRequestForm";
export default function Page() {
  return <AppShell title="更换直属代理" allowedKinds={["reseller_l2"]}><MigrationRequestForm /></AppShell>;
}
```

```tsx
// frontend/src/app/(c-user)/projects/[projectId]/support/page.tsx
import {AppShell} from "@/features/shell/AppShell";
import {GenerationLimitStatusCard} from "@/features/resellers/GenerationLimitStatusCard";
import {MigrationRequestForm} from "@/features/resellers/MigrationRequestForm";
import {SupportGrantPanel} from "@/features/resellers/SupportGrantPanel";
export default async function Page({params}: {params: Promise<{projectId: string}>}) {
  const {projectId} = await params;
  return <AppShell title="代理支持授权" allowedKinds={["c_user"]}><GenerationLimitStatusCard /><SupportGrantPanel projectId={projectId} /><MigrationRequestForm /></AppShell>;
}
```

```tsx
// frontend/src/app/(platform)/platform/resellers/page.tsx
import {AppShell} from "@/features/shell/AppShell";
import {LifecyclePanel} from "@/features/resellers/LifecyclePanel";
import {PriceForm} from "@/features/resellers/PriceForm";
export default function Page() {
  return <AppShell title="代理停用、迁移与费用上限" allowedKinds={["platform_admin"]}><LifecyclePanel /><PriceForm /></AppShell>;
}
```

- [ ] **Step 7: Write end-to-end separation, privacy, and limit-state coverage**

Create `frontend/tests/e2e/reseller-operations.spec.ts`:

```typescript
import {expect, test} from "@playwright/test";

test.beforeEach(async ({page}) => {
  await page.addInitScript(() => sessionStorage.setItem("access_token", "test-token"));
  await page.route("**/api/v1/auth/me", (route) =>
    route.fulfill({json: {kind: "reseller_l1"}}),
  );
  await page.route("**/api/v1/billing/generation-limits/me", (route) =>
    route.fulfill({
      json: {
        version_id: "00000000-0000-0000-0000-000000000001",
        billing_mode: "customer_credit",
        per_task_amount: 1000,
        per_day_amount: 5000,
        per_month_amount: 50000,
        used_day_amount: 800,
        used_month_amount: 4000,
      },
    }),
  );
});

test("offline cash and credit allocation remain two actions", async ({page}) => {
  await page.route("**/api/v1/reseller/evidence?*", (route) =>
    route.fulfill({json: {id: "00000000-0000-0000-0000-000000000010"}}),
  );
  await page.route("**/api/v1/reseller/cash-orders", (route) =>
    route.fulfill({
      json: {
        id: "00000000-0000-0000-0000-000000000020",
        payer_account_id: "00000000-0000-0000-0000-000000000021",
        payee_account_id: "00000000-0000-0000-0000-000000000022",
        reported_amount_fen: 100000,
        estimated_amount_fen: null,
        confirmed_amount_fen: null,
        pricing_version_id: "00000000-0000-0000-0000-000000000023",
        status: "recorded",
      },
    }),
  );
  await page.goto("/reseller/orders");
  await page.getByLabel("付款账户 ID").fill("00000000-0000-0000-0000-000000000021");
  await page.getByLabel("收款账户 ID").fill("00000000-0000-0000-0000-000000000022");
  await page.getByLabel("线下付款金额").fill("1000");
  await page.getByLabel("价格版本 ID").fill("00000000-0000-0000-0000-000000000023");
  await page.getByLabel("付款凭证").setInputFiles({
    name: "proof.png",
    mimeType: "image/png",
    buffer: Buffer.from("proof"),
  });
  await page.getByRole("button", {name: "记录付款凭证"}).click();
  await expect(page.getByText("已记录，尚未充值")).toBeVisible();
  await expect(page.getByText("划拨成功")).toHaveCount(0);
});

test("customer list exposes aggregates and visible limits only", async ({page}) => {
  await page.route("**/api/v1/reseller/customers", (route) =>
    route.fulfill({
      json: [{
        account_id: "00000000-0000-0000-0000-000000000030",
        display_name: "客户",
        account_kind: "c_user",
        account_status: "active",
        direct_parent_account_id: "00000000-0000-0000-0000-000000000031",
        credit_balance: 900,
        actual_cash_fen: 10000,
        estimated_cash_fen: 0,
        allocated_credit_units: 1000,
        returned_credit_units: 0,
        generation_spend_units: 100,
        pending_return_count: 0,
        active_support_grant_count: 0,
      }],
    }),
  );
  await page.goto("/reseller/customers");
  await expect(page.getByText("800 / 5000 创作点")).toBeVisible();
  await expect(page.getByText("客户脚本")).toHaveCount(0);
  await expect(page.getByText("人物素材")).toHaveCount(0);
  await expect(page.getByText("声音")).toHaveCount(0);
});
```

- [ ] **Step 8: Run frontend checks**

Run: `cd frontend && pnpm test && pnpm typecheck && pnpm build && pnpm exec playwright test tests/e2e/reseller-operations.spec.ts`

Expected: all commands PASS; cash recording never displays allocation success, privacy-safe columns render, and agent limit consumption/caps are visible.

- [ ] **Step 9: Commit reseller operations UI**

```bash
git add frontend/src/app frontend/src/features/resellers frontend/src/lib/api/schema.d.ts frontend/tests/e2e/reseller-operations.spec.ts
git commit -m "feat: add reseller operations workspace"
```

### Task 12: Close randomized, permission, operational, and documentation acceptance

**Files:**
- Create: `backend/tests/unit/resellers/test_transfer_properties.py`
- Modify: `backend/tests/integration/resellers/test_access_and_pricing.py`
- Modify: `README.md`

- [ ] **Step 1: Add 10,000 randomized commercial-movement sequences**

Create `backend/tests/unit/resellers/test_transfer_properties.py`:

```python
from dataclasses import dataclass, field

from hypothesis import given, settings, strategies as st


@dataclass
class CommercialState:
    balances: dict[str, int] = field(
        default_factory=lambda: {
            "treasury": 0,
            "l1": 0,
            "l2": 0,
            "c": 0,
        }
    )
    posted_keys: dict[str, tuple[str, str, int]] = field(default_factory=dict)

    def post(self, source: str, destination: str, amount: int, key: str) -> None:
        payload = (source, destination, amount)
        if key in self.posted_keys:
            assert self.posted_keys[key] == payload
            return
        if source != "treasury" and self.balances[source] < amount:
            return
        self.balances[source] -= amount
        self.balances[destination] += amount
        self.posted_keys[key] = payload

    def assert_invariants(self) -> None:
        assert sum(self.balances.values()) == 0
        assert self.balances["l1"] >= 0
        assert self.balances["l2"] >= 0
        assert self.balances["c"] >= 0


operation = st.tuples(
    st.sampled_from(
        [
            "platform_l1",
            "l1_l2",
            "l1_c",
            "l2_c",
            "c_return",
            "l2_return",
            "l1_return",
            "cash_confirm_only",
        ]
    ),
    st.integers(min_value=1, max_value=10_000),
    st.integers(min_value=0, max_value=20),
)


@settings(max_examples=10_000, deadline=None)
@given(st.lists(operation, min_size=1, max_size=40))
def test_randomized_reseller_sequences_keep_cash_separate_and_credits_balanced(
    operations: list[tuple[str, int, int]],
) -> None:
    state = CommercialState()
    for index, (kind, amount, retry_group) in enumerate(operations):
        key = f"{kind}-{retry_group if retry_group < index else index}"
        before_cash_confirmation = dict(state.balances)
        if kind == "platform_l1":
            state.post("treasury", "l1", amount, key)
        elif kind == "l1_l2":
            state.post("l1", "l2", amount, key)
        elif kind == "l1_c":
            state.post("l1", "c", amount, key)
        elif kind == "l2_c":
            state.post("l2", "c", amount, key)
        elif kind == "c_return":
            destination = "l2" if state.balances["l2"] else "l1"
            state.post("c", destination, amount, key)
        elif kind == "l2_return":
            state.post("l2", "l1", amount, key)
        elif kind == "l1_return":
            state.post("l1", "treasury", amount, key)
        else:
            assert state.balances == before_cash_confirmation
        state.assert_invariants()
```

- [ ] **Step 2: Add the fixed hierarchy authority matrix**

Append to `backend/tests/integration/resellers/test_access_and_pricing.py`:

```python
from ip_saas.modules.accounts.service import ALLOWED_CHILDREN


@pytest.mark.parametrize(
    "parent,child,allowed",
    [
        (AccountKind.PLATFORM, AccountKind.RESELLER_L1, True),
        (AccountKind.PLATFORM, AccountKind.C_USER, False),
        (AccountKind.RESELLER_L1, AccountKind.RESELLER_L2, True),
        (AccountKind.RESELLER_L1, AccountKind.C_USER, True),
        (AccountKind.RESELLER_L2, AccountKind.C_USER, True),
        (AccountKind.RESELLER_L2, AccountKind.RESELLER_L2, False),
        (AccountKind.C_USER, AccountKind.C_USER, False),
    ],
)
def test_fixed_hierarchy_matrix(
    parent: AccountKind,
    child: AccountKind,
    allowed: bool,
) -> None:
    assert (child in ALLOWED_CHILDREN[parent]) is allowed
```

The authorization suite is the combined executable matrix:

```text
tests/integration/accounts/test_hierarchy_and_roles.py
tests/integration/resellers/test_access_and_pricing.py
tests/integration/resellers/test_cash_orders.py
tests/integration/resellers/test_credit_movements.py
tests/integration/resellers/test_lifecycle.py
tests/security/test_reseller_privacy.py
tests/integration/billing/test_generation_limits.py
```

It proves platform-only L1 creation and migration approval; L1 L2/direct-C creation; L2 direct-C creation; payee-only cash confirmation; source-only allocation; parent-only return decision; C-owner-only support grant/revocation; scoped and expiring reads; account/internal task/day/month limits; and no third reseller level.

- [ ] **Step 3: Run randomized and focused backend acceptance**

Run:

```bash
cd backend
uv run pytest tests/unit/resellers/test_transfer_properties.py -v --hypothesis-show-statistics
uv run pytest tests/unit/resellers tests/integration/resellers tests/security/test_reseller_privacy.py tests/integration/billing/test_generation_limits.py -v
```

Expected: PASS; Hypothesis reports 10,000 passing examples and all fixed permission, privacy, cash/credit, limit, support, migration, price, and report tests pass.

- [ ] **Step 4: Add the operational README section**

Append to `README.md`:

```markdown
## Reseller operations

The commercial tree is fixed to platform→L1→L2→C and platform→L1→C. L1 creates L2 or direct C accounts; L2 creates direct C accounts; there is no third reseller level.

Offline cash and creation credits are different records. Uploading a receipt and confirming the received cash amount do not change a wallet. A later, explicit allocation uses the cash order's frozen price version and posts one balanced credit transaction. Unused credits return through a lower-account request and direct-parent approval; a return appends a new transaction.

Agents can see descendant accounts, balances, credit movement, aggregate generation consumption, cash/support status, and reconciliation exceptions. They cannot see interviews, profiles, strategies, scripts, people, voices, media, or publication detail unless the C-user project owner grants a named scope for at most seven days. Every granted read is audited.

The platform configures versioned per-task, daily, and monthly creation-credit limits for platform, reseller, and C accounts. Platform self-marketing cost centers receive separate per-task, daily, and monthly limits in人民币分. Missing or exhausted limits reject a new paid task before any hold, task, outbox event, or supplier request is created.

Suspension revokes sessions and stops new commercial or paid work for that account without deleting history. A C account requests migration to an active L1/L2; an L2 requests migration to an active L1; a platform admin approves and executes. Migration changes the active parent atomically, revokes obsolete support grants, and never moves credit balances automatically.
```

- [ ] **Step 5: Run full repository gates**

Run:

```bash
make export-contracts
make lint
make test-unit
make test-integration
make test-e2e
```

Expected: every command exits 0; ordinary tests use fake storage and no payment, publishing, model, or paid-provider credential.

- [ ] **Step 6: Commit acceptance and operations documentation**

```bash
git add backend/tests/unit/resellers/test_transfer_properties.py backend/tests/integration/resellers/test_access_and_pricing.py README.md
git commit -m "test: close reseller operations acceptance"
```

## Plan 05 completion checklist

- [ ] The only valid active paths are platform→L1→L2→C and platform→L1→C; report traversal is bounded to those paths.
- [ ] Offline evidence, cash recording, and payee confirmation make zero credit-ledger calls and never imply online payment or automatic split settlement.
- [ ] Cash rows distinguish payer-reported real amounts, estimates, and payee-confirmed received amounts; estimates are never reported as actual revenue.
- [ ] A cash allocation derives parent→child direction from the frozen order, validates confirmed cash against the exact price version, and posts once.
- [ ] Returns are lower→parent requests, require parent approval, append one balanced transaction, and never edit an earlier ledger entry.
- [ ] PostgreSQL row locks, one caller-owned transaction, and account-scoped client idempotency preserve non-negative balances, allow unrelated accounts to reuse a key safely, and roll back request/audit/ledger state together; the underlying global ledger key is derived from the server-generated movement ID.
- [ ] Platform, L1, and L2 price authority follows the specification; platform floors and activity prices are versioned, and old orders keep old version IDs.
- [ ] Plan 01 capability-consumption `PricingVersion` remains the only capability-cost price object.
- [ ] Resellers receive account, balance, flow, aggregate consumption, support, and exception fields only; report code cannot join private content tables.
- [ ] Support is C-owner-created, reseller-chain-bound, scope-bound, at most seven days, separately revocable, default-deny, and audited on every read.
- [ ] Suspension blocks login, child creation, pricing, cash confirmation, credit movement, project work, and new holds for the suspended account while descendants retain their own content.
- [ ] C and L2 migrations require subject request plus platform approval, preserve one active relation, revoke obsolete support, retain old/new history, and move zero credits.
- [ ] Account and internal-cost limits are immutable versions; customer credits and internal人民币分 remain separate units.
- [ ] Single-task, Shanghai-calendar day, and Shanghai-calendar month caps are checked before both customer and internal holds; missing/exhausted limits fail closed before task/outbox/provider cost.
- [ ] Customer and internal reservation checks take a subject-scoped PostgreSQL transaction advisory lock before resolving exposure and retain it through hold creation; the real two-session race admits exactly one 600-unit hold against a 1,000-unit cap.
- [ ] Limit exposure counts ACTIVE reservations plus SETTLED actual usage, excludes RELEASED holds, and has passing exact-cap, cap+1, missing-config, Shanghai day/month, and lock-removal mutation coverage.
- [ ] `BillingService` has no optional or permissive limit dependency; a contract test scans every production composition and later feature tests provide explicit limit versions or a named unit-test fake.
- [ ] Plans 01–04 cross-module integration harnesses create persisted effective customer/internal limit versions; no production deny adapter or early allow fake remains in those acceptance paths.
- [ ] Agent/C pages show their own cap and usage state; platform pages configure account/internal versions and display internal cost-center state.
- [ ] Frontend types come only from `src/lib/api/schema.d.ts`; JSON calls reuse Plan 01 `apiRequest` with backend `/v1/...` paths, and the single evidence-upload fetch applies `/api` exactly once.
- [ ] `0005_resellers` is the only new revision, its down revision is `0004_publication`, metadata has no drift, and downgrade returns exactly to 0004.
- [ ] OpenAPI, generated TypeScript, backend tests, 10,000 randomized sequences, security tests, frontend tests, production build, and Playwright journeys pass.

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-24-05-reseller-operations.md`. Two execution options:

1. **Subagent-Driven (recommended)** — use `superpowers:subagent-driven-development`, dispatch a fresh worker per task, and review specification compliance and code quality between tasks.
2. **Inline Execution** — use `superpowers:executing-plans`, execute tasks in batches, and stop at the migration, API, and final acceptance checkpoints.
