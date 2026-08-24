# Durable Provider Attempt and Supplier Reconciliation Addendum Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Ark Responses and Web Search recoverable across the paid-response-before-database-commit crash window by journaling every logical call before send, importing complete official supplier evidence, and batch-finalizing each exhausted task exactly once.

**Architecture:** Add an append-oriented provider-attempt journal and strict supplier-evidence tables inside Plan 02's existing `0002_intelligence` schema. The intelligence Worker durably prepares and marks a stable logical call before HTTP, never resends an unresolved sent call, and leaves exhausted tasks in Plan 01's `reconciliation_required` state; an independently deployed scanner implements Plan 01's two evidence ports and calls the one frozen `TaskReconciliationService` batch finalizer. Production composition remains fail closed before secrets or HTTP unless both this runtime attestation and Plan 02's SHA-256-pinned production crash-PoC attestation pass.

**Tech Stack:** Python 3.12, Pydantic 2.x strict models, SQLAlchemy 2.0 synchronous `Session`, PostgreSQL 16, Alembic, FastAPI worker composition, TOS private objects, RocketMQ/VKE operational jobs, Pytest, `httpx`, and SHA-256 canonical evidence.

---

## Scope, authority, and frozen contracts

This document is the mandatory Plan 02a safety addendum named by `2026-08-24-00-ip-saas-master-roadmap.md` and `2026-08-24-02-ip-content-intelligence.md`. It is part of business subproject 2, not a seventh business subproject. It changes no customer workflow, endpoint, frontend schema, reseller rule, media rule, or product entitlement. It exists only to close the provider-call durability gap before real Ark Responses or Web Search can be enabled.

Merge the greenfield schema and provider-domain work into Plan 02 before `0002_intelligence` is deployed. Execute the production composition, real-limit integration, and paid release Tasks only after Plan 05 has installed `ip_saas.modules.billing.composition.build_billing_service`; this ordering does not change migration ownership or create a seventh project.

Plan 01 remains the sole authority for task state, holds, provider-cost rows, and retry-exhaustion finalization. Import these contracts; do not fork them:

```python
from ip_saas.modules.billing.reconciliation import (
    NoProviderCallEvidence,
    NoProviderCallEvidencePort,
    ProviderCostManifestPort,
    ProviderRequestIdentity,
    TaskReconciliationService,
)
from ip_saas.modules.billing.service import BillingMode, ProviderCostInput
from ip_saas.modules.tasks.models import TaskRecord
from ip_saas.modules.tasks.service import TaskSubmissionService
```

The frozen state and accounting rules are:

- A `TaskRecord` owns the authoritative `attempt_no`, `lease_expires_at`, `request_fingerprint`, `billing_mode`, hold, and status. `reconciliation_required` is non-terminal and no ordinary Worker may execute it.
- `start(session, task_id, lease_seconds=300, max_attempts=8)` reclaims an expired `running` task by incrementing `attempt_no`; `heartbeat`, `succeed`, and `fail` require the exact captured attempt and an unexpired lease. This addendum does not weaken those compare-and-set rules.
- `TaskReconciliationService.finalize_no_provider_call(...)` is the only zero-call release path. Its evidence SHA-256 is part of Plan 01's reconciliation fingerprint, so a changed proof cannot replay under the same key.
- `TaskReconciliationService.finalize_provider_costs(...)` is the only exhausted-task paid-call finalizer. It receives one non-empty tuple covering the complete supplier request manifest, writes every cost and changes the task in one transaction, settles customer work at `actual_amount=0`, and settles platform-internal work at the exact sum of all `amount_fen` values.
- Every non-zero production `ProviderCostInput` has `task_id` equal to the exact `TaskRecord.id`, `reconciliation_status=ReconciliationStatus.MATCHED`, and a provider request ID whose stripped length is 1 through 200. A request is one cost row; different supplier request IDs are never aggregated into one row.
- Exact replay of `(provider, provider_request_id)` with identical fields is idempotent. Reusing that identity with any changed task, model, capability, quantity, unit, currency, or amount raises `Conflict`.
- The scanner calls the batch finalizer once per task. It never loops over costs and calls `settle_generation` piecemeal, never changes `TaskRecord` directly, and never releases a hold from a database anti-join alone.

All ordinary tests remove provider credentials, use `httpx.MockTransport` only where request serialization must be inspected, make no DNS or external HTTP call, and incur zero supplier spend. The paid SIGKILL exercise is a separately acknowledged operational step with both a provider hard cap and a real internal-limit version.

## Migration decision

The frozen primary chain remains:

```text
0001_foundation -> 0002_intelligence -> 0003_media -> 0004_publication -> 0005_resellers -> 0006_governance
```

When Plan 02 has not been applied to any shared, staging, or production database, execute this addendum in the same Plan 02 branch and add its tables to the existing `0002_intelligence.py`. There is still one `0002_intelligence` revision and one Alembic head.

If any environment has already applied `0002_intelligence` or a later revision, stop before editing or running that migration. Open a controlled migration-chain change, choose the next revision only after updating the master roadmap and every affected `down_revision`, rehearse upgrade and rollback against a restored copy, and obtain migration review. Silently changing an applied `0002_intelligence` is forbidden. Task 1 makes this branch decision mechanical from a checked deployment inventory; this addendum itself does not invent a second Alembic head.

## File map

```text
.env.example                                                     # disabled Plan 02a release settings
backend/
├── migrations/versions/0002_intelligence.py                     # same greenfield Plan 02 revision only
├── scripts/
│   ├── check_plan02a_migration_mode.py                           # blocks mutation after 0002 deployment
│   ├── reconcile_intelligence_provider_costs.py                  # independent scanner process
│   └── run_intelligence_provider_crash_poc.py                    # extend Plan 02 production fault harness
├── src/ip_saas/
│   ├── config.py                                                 # pinned runtime-attestation settings
│   ├── db/base.py                                                # model discovery
│   └── modules/intelligence/
│       ├── provider_attempts.py                                  # SQLAlchemy journal/evidence records
│       ├── provider_identity.py                                  # canonical logical-call/request identity
│       ├── provider_journal.py                                   # prepared/send/response fenced transitions
│       ├── provider_evidence.py                                  # strict official-export ingestion
│       ├── provider_reconciliation.py                            # Plan 01 ports and task scanner
│       ├── reconciliation_composition.py                         # production scanner dependencies
│       ├── provider_release.py                                   # deployed runtime attestation
│       ├── worker.py                                             # pre-send commit and fail-closed composition
│       └── providers/
│           ├── ark_responses.py                                  # stable logical call and exact response ID
│           └── ark_web_search.py                                 # stable logical call and exact response ID
└── tests/
    ├── conftest.py
    ├── support/provider_reconciliation_fixtures.py
    ├── contract/intelligence/test_plan02a_source_contract.py
    ├── integration/intelligence/
    │   ├── test_provider_crash_recovery.py
    │   ├── test_provider_attempt_journal.py
    │   ├── test_supplier_evidence_ingestion.py
    │   └── test_provider_reconciliation_scanner.py
    └── unit/intelligence/
        ├── test_ark_responses.py
        ├── test_ark_web_search.py
        ├── test_plan02a_migration_policy.py
        ├── test_provider_identity.py
        ├── test_provider_release.py
        ├── test_provider_release_runtime.py
        ├── test_reconciliation_cli.py
        ├── test_worker_leases.py
        └── test_worker_provider_boundary.py
docs/runbooks/
├── intelligence-provider-crash-poc.md                            # extend existing Plan 02 runbook
└── intelligence-provider-reconciliation.md                      # import/scanner/recovery procedure
```

The addendum creates no event type and no outbox event. The independent scanner polls `TaskRecord.status == reconciliation_required` with PostgreSQL locking. This avoids a second delivery truth and therefore requires no new `contracts/events/*.json` file.

### Task 1: Freeze the migration branch and canonical provider identity

**Files:**
- Create: `backend/scripts/check_plan02a_migration_mode.py`
- Test: `backend/tests/unit/intelligence/test_plan02a_migration_policy.py`
- Create: `backend/src/ip_saas/modules/intelligence/provider_identity.py`
- Test: `backend/tests/unit/intelligence/test_provider_identity.py`

- [ ] **Step 1: Write the failing migration-policy and identity tests**

Create both test files with these cases:

```python
# backend/tests/unit/intelligence/test_plan02a_migration_policy.py
from scripts.check_plan02a_migration_mode import (
    DeploymentObservation,
    MigrationDecision,
    decide,
)


def test_only_unapplied_or_foundation_environments_allow_same_0002_edit() -> None:
    observations = (
        DeploymentObservation(environment="local", revision=None),
        DeploymentObservation(environment="ci", revision="0001_foundation"),
    )
    assert decide(observations) == MigrationDecision.MERGE_INTO_0002


def test_any_applied_or_unknown_revision_blocks_same_0002_edit() -> None:
    deployed = (DeploymentObservation(environment="staging", revision="0002_intelligence"),)
    unknown = (DeploymentObservation(environment="prod", revision="vendor_branch_17"),)
    assert decide(deployed) == MigrationDecision.CONTROLLED_NEW_REVISION_REQUIRED
    assert decide(unknown) == MigrationDecision.CONTROLLED_NEW_REVISION_REQUIRED
```

```python
# backend/tests/unit/intelligence/test_provider_identity.py
from uuid import UUID

import pytest

from ip_saas.common.errors import Conflict
from ip_saas.modules.intelligence.provider_identity import (
    canonical_request_sha256,
    logical_call_identity,
    normalize_provider_request_id,
)


def test_logical_call_identity_is_stable_across_worker_attempts() -> None:
    first = logical_call_identity(
        task_id=UUID(int=1),
        request_fingerprint="a" * 64,
        adapter="responses",
        operation="generate_marketing_frame",
        logical_call_id="frame:primary",
        request_payload={"input": [{"role": "user", "content": "gold gift"}]},
    )
    second = logical_call_identity(
        task_id=UUID(int=1),
        request_fingerprint="a" * 64,
        adapter="responses",
        operation="generate_marketing_frame",
        logical_call_id="frame:primary",
        request_payload={"input": [{"content": "gold gift", "role": "user"}]},
    )
    assert first == second
    assert first.request_payload_sha256 == canonical_request_sha256(
        {"input": [{"content": "gold gift", "role": "user"}]}
    )


@pytest.mark.parametrize(
    "value",
    [
        None,
        123,
        "",
        "   ",
        "None",
        "NONE",
        "nOnE",
        "x" * 201,
        " id-with-padding ",
    ],
)
def test_provider_request_identity_rejects_empty_oversize_or_changed_whitespace(
    value: object,
) -> None:
    with pytest.raises(Conflict):
        normalize_provider_request_id(value)


def test_provider_request_identity_accepts_exact_one_to_two_hundred_characters() -> None:
    assert normalize_provider_request_id("r") == "r"
    assert normalize_provider_request_id("r" * 200) == "r" * 200
```

- [ ] **Step 2: Run the focused tests and verify both modules are missing**

Run: `cd backend && uv run pytest tests/unit/intelligence/test_plan02a_migration_policy.py tests/unit/intelligence/test_provider_identity.py -q`

Expected: collection fails with `ModuleNotFoundError` for the two new modules; no network fixture is initialized.

- [ ] **Step 3: Implement the migration decision and canonical identities**

Create the migration checker as a strict CLI. The input JSON is an array produced from an actual `alembic current` observation for every deployment environment; an empty inventory is an error, not implicit greenfield approval.

```python
# backend/scripts/check_plan02a_migration_mode.py
from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path


@dataclass(frozen=True)
class DeploymentObservation:
    environment: str
    revision: str | None


class MigrationDecision(StrEnum):
    MERGE_INTO_0002 = "merge_into_0002"
    CONTROLLED_NEW_REVISION_REQUIRED = "controlled_new_revision_required"


def decide(observations: tuple[DeploymentObservation, ...]) -> MigrationDecision:
    if not observations:
        raise ValueError("deployment inventory must name every environment")
    if all(item.revision in {None, "0001_foundation"} for item in observations):
        return MigrationDecision.MERGE_INTO_0002
    return MigrationDecision.CONTROLLED_NEW_REVISION_REQUIRED


def load_inventory(path: Path) -> tuple[DeploymentObservation, ...]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, list):
        raise ValueError("deployment inventory must be a JSON array")
    return tuple(
        DeploymentObservation(
            environment=str(item["environment"]).strip(),
            revision=None if item["revision"] is None else str(item["revision"]).strip(),
        )
        for item in payload
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--inventory", type=Path, required=True)
    args = parser.parse_args()
    decision = decide(load_inventory(args.inventory))
    print(decision.value)
    return 0 if decision == MigrationDecision.MERGE_INTO_0002 else 78


if __name__ == "__main__":
    raise SystemExit(main())
```

Create the identity helper. Its stable UUID contains no lease attempt, clock value, process value, or random input.

```python
# backend/src/ip_saas/modules/intelligence/provider_identity.py
from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from typing import Mapping
from uuid import NAMESPACE_URL, UUID, uuid5

from ip_saas.common.errors import Conflict
from ip_saas.modules.billing.service import canonical_provider_request_id


JSONScalar = str | int | float | bool | None
JSONValue = JSONScalar | list["JSONValue"] | Mapping[str, "JSONValue"]


@dataclass(frozen=True)
class LogicalCallIdentity:
    logical_call_key: str
    client_attempt_id: UUID
    request_payload_sha256: str


def canonical_request_sha256(payload: Mapping[str, JSONValue]) -> str:
    body = json.dumps(
        payload,
        ensure_ascii=False,
        allow_nan=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    return hashlib.sha256(body).hexdigest()


def logical_call_identity(
    *,
    task_id: UUID,
    request_fingerprint: str,
    adapter: str,
    operation: str,
    logical_call_id: str,
    request_payload: Mapping[str, JSONValue],
) -> LogicalCallIdentity:
    payload_sha256 = canonical_request_sha256(request_payload)
    normalized_call_id = logical_call_id.strip()
    if logical_call_id != normalized_call_id or not normalized_call_id:
        raise Conflict("logical_call_id must be non-empty and trimmed")
    material = ":".join(
        (
            str(task_id),
            request_fingerprint,
            adapter.strip(),
            operation.strip(),
            normalized_call_id,
        )
    )
    logical_key = hashlib.sha256(material.encode("utf-8")).hexdigest()
    return LogicalCallIdentity(
        logical_call_key=logical_key,
        client_attempt_id=uuid5(NAMESPACE_URL, f"ip-saas:plan02a:{logical_key}"),
        request_payload_sha256=payload_sha256,
    )


def normalize_provider_request_id(value: object) -> str:
    try:
        normalized = canonical_provider_request_id(value, allow_missing=False)
    except ValueError as error:
        raise Conflict(str(error)) from error
    if normalized is None:
        raise Conflict("provider_request_id is required")
    if value != normalized:
        raise Conflict("provider_request_id must be trimmed and 1 through 200 characters")
    return normalized
```

- [ ] **Step 4: Run the tests and exercise both CLI exit paths**

Run:

```bash
cd backend
uv run pytest tests/unit/intelligence/test_plan02a_migration_policy.py tests/unit/intelligence/test_provider_identity.py -q
printf '[{"environment":"ci","revision":"0001_foundation"}]' > /tmp/ip-saas-plan02a-greenfield.json
uv run python scripts/check_plan02a_migration_mode.py --inventory /tmp/ip-saas-plan02a-greenfield.json
printf '[{"environment":"staging","revision":"0002_intelligence"}]' > /tmp/ip-saas-plan02a-deployed.json
test "$(uv run python scripts/check_plan02a_migration_mode.py --inventory /tmp/ip-saas-plan02a-deployed.json; printf ':%s' "$?")" = $'controlled_new_revision_required\n:78'
```

Expected: Pytest reports all tests passed; the greenfield command prints `merge_into_0002` and exits 0; the deployed command prints `controlled_new_revision_required` and exits 78.

- [ ] **Step 5: Commit the branch guard and identity contract**

```bash
git add backend/scripts/check_plan02a_migration_mode.py backend/src/ip_saas/modules/intelligence/provider_identity.py backend/tests/unit/intelligence/test_plan02a_migration_policy.py backend/tests/unit/intelligence/test_provider_identity.py
git commit -m "feat: freeze durable provider identity and migration mode"
```

### Task 2: Add the pre-send journal and supplier-evidence schema to `0002_intelligence`

**Files:**
- Create: `backend/src/ip_saas/modules/intelligence/provider_attempts.py`
- Modify: `backend/src/ip_saas/db/base.py`
- Modify: `backend/migrations/versions/0002_intelligence.py`
- Test: `backend/tests/integration/intelligence/test_provider_attempt_journal.py`

- [ ] **Step 1: Write the failing PostgreSQL schema and constraint test**

Use only the inherited `db_session` fixture from `backend/tests/conftest.py`; it is a synchronous PostgreSQL `Session`. Add this test first:

```python
# backend/tests/integration/intelligence/test_provider_attempt_journal.py
from sqlalchemy import inspect
from sqlalchemy.orm import Session


def test_plan02a_tables_and_request_identity_constraints(db_session: Session) -> None:
    names = set(inspect(db_session.get_bind()).get_table_names())
    assert {
        "intelligence_provider_attempts",
        "intelligence_supplier_evidence_bundles",
        "intelligence_supplier_request_matches",
    } <= names
    attempt_checks = {
        item["name"]
        for item in inspect(db_session.get_bind()).get_check_constraints(
            "intelligence_provider_attempts"
        )
    }
    assert "ck_intelligence_attempt_response_id_length" in attempt_checks
```

- [ ] **Step 2: Run the schema test and verify the tables are absent**

Run: `cd backend && uv run pytest tests/integration/intelligence/test_provider_attempt_journal.py::test_plan02a_tables_and_request_identity_constraints -q`

Expected: FAIL because `intelligence_provider_attempts` is not present.

- [ ] **Step 3: Create the three focused SQLAlchemy records**

Create `provider_attempts.py` with these records. `IntelligenceProviderAttempt` has mutable lifecycle columns but immutable ownership and request-identity columns; the two supplier records are append-only and corrections require a new `supplier_run_id`.

```python
# backend/src/ip_saas/modules/intelligence/provider_attempts.py
from __future__ import annotations

import enum
from datetime import datetime
from decimal import Decimal
from uuid import UUID, uuid4

from sqlalchemy import Boolean, CheckConstraint, DateTime, Enum, ForeignKey, Integer, Numeric, String, UniqueConstraint
from sqlalchemy.dialects.postgresql import UUID as PGUUID
from sqlalchemy.orm import Mapped, mapped_column

from ip_saas.db.base import Base


class ProviderAttemptStatus(str, enum.Enum):
    PREPARED = "prepared"
    SEND_STARTED = "send_started"
    RESPONSE_OBSERVED = "response_observed"
    SUPPLIER_MATCHED = "supplier_matched"
    SUPPLIER_NO_CALL = "supplier_no_call"


class IntelligenceProviderAttempt(Base):
    __tablename__ = "intelligence_provider_attempts"
    __table_args__ = (
        UniqueConstraint("task_id", "logical_call_key", name="uq_intelligence_attempt_task_logical_call"),
        UniqueConstraint("provider", "client_attempt_id", name="uq_intelligence_attempt_provider_client"),
        UniqueConstraint("provider", "provider_request_id", name="uq_intelligence_attempt_provider_response"),
        CheckConstraint("char_length(logical_call_key) = 64", name="ck_intelligence_attempt_logical_key"),
        CheckConstraint("char_length(provider_account_fingerprint) = 64", name="ck_intelligence_attempt_provider_account"),
        CheckConstraint("char_length(request_fingerprint) = 64", name="ck_intelligence_attempt_request_fingerprint"),
        CheckConstraint("char_length(request_payload_sha256) = 64", name="ck_intelligence_attempt_payload_sha"),
        CheckConstraint("response_sha256 IS NULL OR char_length(response_sha256) = 64", name="ck_intelligence_attempt_response_sha"),
        CheckConstraint(
            "provider_request_id IS NULL OR (char_length(provider_request_id) BETWEEN 1 AND 200 AND provider_request_id = btrim(provider_request_id))",
            name="ck_intelligence_attempt_response_id_length",
        ),
        CheckConstraint(
            "(status = 'prepared' AND send_worker_attempt_no IS NULL AND send_started_at IS NULL AND provider_request_id IS NULL AND supplier_evidence_bundle_id IS NULL) OR "
            "(status = 'send_started' AND send_worker_attempt_no IS NOT NULL AND send_started_at IS NOT NULL AND provider_request_id IS NULL AND supplier_evidence_bundle_id IS NULL) OR "
            "(status = 'response_observed' AND send_worker_attempt_no IS NOT NULL AND send_started_at IS NOT NULL AND provider_request_id IS NOT NULL AND response_sha256 IS NOT NULL AND supplier_evidence_bundle_id IS NULL) OR "
            "(status = 'supplier_matched' AND send_worker_attempt_no IS NOT NULL AND send_started_at IS NOT NULL AND provider_request_id IS NOT NULL AND supplier_evidence_bundle_id IS NOT NULL) OR "
            "(status = 'supplier_no_call' AND send_worker_attempt_no IS NOT NULL AND send_started_at IS NOT NULL AND provider_request_id IS NULL AND supplier_evidence_bundle_id IS NOT NULL)",
            name="ck_intelligence_attempt_lifecycle",
        ),
    )

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    account_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), ForeignKey("accounts.id", ondelete="RESTRICT"), nullable=False, index=True)
    task_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), ForeignKey("task_records.id", ondelete="RESTRICT"), nullable=False, index=True)
    model_registry_entry_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), ForeignKey("model_registry_entries.id", ondelete="RESTRICT"), nullable=False)
    provider: Mapped[str] = mapped_column(String(50), nullable=False)
    provider_account_fingerprint: Mapped[str] = mapped_column(String(64), nullable=False)
    adapter: Mapped[str] = mapped_column(String(30), nullable=False)
    operation: Mapped[str] = mapped_column(String(100), nullable=False)
    capability: Mapped[str] = mapped_column(String(100), nullable=False)
    model_id: Mapped[str] = mapped_column(String(200), nullable=False)
    model_version: Mapped[str] = mapped_column(String(100), nullable=False)
    logical_call_key: Mapped[str] = mapped_column(String(64), nullable=False)
    client_attempt_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    request_fingerprint: Mapped[str] = mapped_column(String(64), nullable=False)
    request_payload_sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    first_worker_attempt_no: Mapped[int] = mapped_column(Integer, nullable=False)
    send_worker_attempt_no: Mapped[int | None] = mapped_column(Integer)
    status: Mapped[ProviderAttemptStatus] = mapped_column(
        Enum(
            ProviderAttemptStatus,
            name="provider_attempt_status",
            values_callable=lambda enum_type: [item.value for item in enum_type],
        ),
        nullable=False,
    )
    prepared_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    send_started_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    response_observed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True))
    provider_request_id: Mapped[str | None] = mapped_column(String(200))
    response_sha256: Mapped[str | None] = mapped_column(String(64))
    supplier_evidence_bundle_id: Mapped[UUID | None] = mapped_column(
        PGUUID(as_uuid=True),
        ForeignKey("intelligence_supplier_evidence_bundles.id", ondelete="RESTRICT"),
    )


class IntelligenceSupplierEvidenceBundle(Base):
    __tablename__ = "intelligence_supplier_evidence_bundles"
    __table_args__ = (
        UniqueConstraint("account_id", "provider", "supplier_run_id", name="uq_intelligence_supplier_bundle_scope"),
        CheckConstraint("char_length(request_log_sha256) = 64", name="ck_intelligence_bundle_request_log_sha"),
        CheckConstraint("char_length(statement_sha256) = 64", name="ck_intelligence_bundle_statement_sha"),
        CheckConstraint("is_complete AND is_final", name="ck_intelligence_bundle_complete_final"),
    )

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    account_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), ForeignKey("accounts.id", ondelete="RESTRICT"), nullable=False, index=True)
    provider: Mapped[str] = mapped_column(String(50), nullable=False)
    provider_account_fingerprint: Mapped[str] = mapped_column(String(64), nullable=False)
    supplier_run_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    requested_from: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    requested_through: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)
    request_log_object_key: Mapped[str] = mapped_column(String(1024), nullable=False)
    request_log_sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    statement_object_key: Mapped[str] = mapped_column(String(1024), nullable=False)
    statement_sha256: Mapped[str] = mapped_column(String(64), nullable=False)
    is_complete: Mapped[bool] = mapped_column(Boolean, nullable=False)
    is_final: Mapped[bool] = mapped_column(Boolean, nullable=False)
    imported_by: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), nullable=False)
    imported_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), nullable=False)


class IntelligenceSupplierRequestMatch(Base):
    __tablename__ = "intelligence_supplier_request_matches"
    __table_args__ = (
        UniqueConstraint("attempt_id", name="uq_intelligence_supplier_match_attempt"),
        UniqueConstraint("provider", "provider_request_id", name="uq_intelligence_supplier_match_request"),
        CheckConstraint("char_length(provider_request_id) BETWEEN 1 AND 200 AND provider_request_id = btrim(provider_request_id)", name="ck_intelligence_match_request_id"),
        CheckConstraint("amount_fen >= 0 AND supplier_amount_minor >= 0", name="ck_intelligence_match_nonnegative_cost"),
    )

    id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), primary_key=True, default=uuid4)
    bundle_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), ForeignKey("intelligence_supplier_evidence_bundles.id", ondelete="RESTRICT"), nullable=False)
    attempt_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), ForeignKey("intelligence_provider_attempts.id", ondelete="RESTRICT"), nullable=False)
    account_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), ForeignKey("accounts.id", ondelete="RESTRICT"), nullable=False, index=True)
    task_id: Mapped[UUID] = mapped_column(PGUUID(as_uuid=True), ForeignKey("task_records.id", ondelete="RESTRICT"), nullable=False, index=True)
    provider: Mapped[str] = mapped_column(String(50), nullable=False)
    provider_request_id: Mapped[str] = mapped_column(String(200), nullable=False)
    capability: Mapped[str] = mapped_column(String(100), nullable=False)
    model_id: Mapped[str] = mapped_column(String(200), nullable=False)
    model_version: Mapped[str] = mapped_column(String(100), nullable=False)
    native_quantity: Mapped[Decimal] = mapped_column(Numeric(30, 9), nullable=False)
    native_unit: Mapped[str] = mapped_column(String(50), nullable=False)
    supplier_amount_minor: Mapped[int] = mapped_column(Integer, nullable=False)
    supplier_currency: Mapped[str] = mapped_column(String(3), nullable=False)
    amount_fen: Mapped[int] = mapped_column(Integer, nullable=False)
    request_fingerprint: Mapped[str] = mapped_column(String(64), nullable=False)
```

- [ ] **Step 4: Register the models and extend the existing greenfield migration**

Import `ip_saas.modules.intelligence.provider_attempts` inside `load_all_models()` in `backend/src/ip_saas/db/base.py`. In `backend/migrations/versions/0002_intelligence.py`, add the three tables with the exact columns, foreign keys, enum, unique constraints, and checks from the SQLAlchemy metadata above; create the evidence-bundle table first, the attempt table second, and the request-match table third after `task_records` exists and before any Plan 02 data seed. Drop matches, attempts, and bundles in that reverse dependency order at the beginning of `downgrade()`, then drop `provider_attempt_status`.

Before editing the migration, run the Task 1 checker against the signed deployment inventory. Continue only when it exits 0.

Run:

```bash
cd backend
uv run python scripts/check_plan02a_migration_mode.py --inventory .artifacts/deployment-revisions.json
uv run alembic downgrade 0001_foundation
uv run alembic upgrade 0002_intelligence
uv run alembic check
uv run alembic heads
uv run pytest tests/integration/intelligence/test_provider_attempt_journal.py::test_plan02a_tables_and_request_identity_constraints -q
```

Expected: the checker prints `merge_into_0002`; Alembic reports only `0001_foundation -> 0002_intelligence`, `check` reports no new operations, `heads` prints exactly `0002_intelligence (head)`, and Pytest reports one pass. If the checker exits 78, stop this addendum execution and follow the controlled-chain branch described above.

- [ ] **Step 5: Commit the same-head schema**

```bash
git add backend/src/ip_saas/modules/intelligence/provider_attempts.py backend/src/ip_saas/db/base.py backend/migrations/versions/0002_intelligence.py backend/tests/integration/intelligence/test_provider_attempt_journal.py
git commit -m "feat: add durable intelligence provider journal schema"
```

### Task 3: Implement fenced prepare, send-marker, and response-observed transitions

**Files:**
- Create: `backend/tests/support/provider_reconciliation_fixtures.py`
- Modify: `backend/tests/conftest.py`
- Create: `backend/src/ip_saas/modules/intelligence/provider_journal.py`
- Modify: `backend/tests/integration/intelligence/test_provider_attempt_journal.py`

- [ ] **Step 1: Add an explicit PostgreSQL fixture and failing lifecycle tests**

Create this support fixture and register it once from `backend/tests/conftest.py` with `pytest_plugins = ("tests.support.provider_reconciliation_fixtures",)`. If `pytest_plugins` already exists, append this module to that tuple rather than assigning a second variable.

```python
# backend/tests/support/provider_reconciliation_fixtures.py
from datetime import timedelta
from uuid import UUID

import pytest
from sqlalchemy.orm import Session

from ip_saas.common.clock import SystemClock
from ip_saas.common.tasking import TaskStatus
from ip_saas.modules.accounts.models import Account, AccountKind
from ip_saas.modules.billing.service import BillingMode
from ip_saas.modules.model_registry.models import ModelLifecycle, ModelRegistryEntry
from ip_saas.modules.projects.models import IPProject, ProjectOwnerType, ProjectStatus
from ip_saas.modules.tasks.models import TaskRecord


@pytest.fixture
def running_intelligence_task(db_session: Session) -> TaskRecord:
    account = Account(id=UUID(int=2201), kind=AccountKind.C_USER, display_name="Plan 02a owner")
    project = IPProject(
        id=UUID(int=2202),
        owner_type=ProjectOwnerType.C_USER,
        owner_account_id=account.id,
        name="Plan 02a project",
        status=ProjectStatus.ACTIVE,
        created_by_principal_id=UUID(int=2203),
    )
    model = ModelRegistryEntry(
        id=UUID(int=2204),
        capability="intelligence.diagnose",
        provider="volcengine",
        model_id="doubao-seed-2-0-lite-260215",
        model_version="260215",
        input_modalities=["text"],
        output_modalities=["text"],
        parameter_schema={},
        pricing_version_id=None,
        safety_version="plan02a-v1",
        lifecycle=ModelLifecycle.ACTIVE,
        regression_passed=True,
        regression_report_ref="tests://plan02a",
        created_by_principal_id=UUID(int=2203),
    )
    task = TaskRecord(
        id=UUID(int=2205),
        project_id=project.id,
        initiated_by_actor_id=UUID(int=2203),
        initiated_by_account_id=account.id,
        capability=model.capability,
        model_registry_entry_id=model.id,
        billing_mode=BillingMode.CUSTOMER_CREDIT,
        billing_hold_id=UUID(int=2206),
        request_fingerprint="a" * 64,
        status=TaskStatus.RUNNING,
        attempt_no=1,
        lease_expires_at=SystemClock().now() + timedelta(minutes=5),
        idempotency_key="plan02a-task",
        input_payload={"topic": "gold gifts"},
        result_payload=None,
        error_code=None,
        error_message=None,
        reconciliation_idempotency_key=None,
        reconciliation_fingerprint=None,
    )
    db_session.add_all((account, project, model, task))
    db_session.flush()
    return task
```

Append these tests to `test_provider_attempt_journal.py`:

```python
from datetime import timedelta

import pytest
from sqlalchemy.orm import Session

from ip_saas.common.clock import SystemClock
from ip_saas.common.errors import Conflict
from ip_saas.modules.intelligence.provider_attempts import ProviderAttemptStatus
from ip_saas.modules.intelligence.provider_journal import ProviderAttemptJournal
from ip_saas.modules.tasks.models import TaskRecord


def request_payload(script: str = "gold gift") -> dict[str, object]:
    return {"input": [{"role": "user", "content": script}]}


def test_prepare_is_exactly_replayable_and_changed_input_conflicts(
    db_session: Session,
    running_intelligence_task: TaskRecord,
) -> None:
    journal = ProviderAttemptJournal(SystemClock())
    first = journal.prepare(
        db_session,
        task_id=running_intelligence_task.id,
        attempt_no=1,
        adapter="responses",
        operation="diagnose",
        logical_call_id="diagnose:primary",
        provider_account_fingerprint="f" * 64,
        request_payload=request_payload(),
        model_id="doubao-seed-2-0-lite-260215",
        model_version="260215",
    )
    replay = journal.prepare(
        db_session,
        task_id=running_intelligence_task.id,
        attempt_no=1,
        adapter="responses",
        operation="diagnose",
        logical_call_id="diagnose:primary",
        provider_account_fingerprint="f" * 64,
        request_payload=request_payload(),
        model_id="doubao-seed-2-0-lite-260215",
        model_version="260215",
    )
    assert replay.id == first.id
    assert replay.client_attempt_id == first.client_attempt_id
    with pytest.raises(Conflict, match="logical call input changed"):
        journal.prepare(
            db_session,
            task_id=running_intelligence_task.id,
            attempt_no=1,
            adapter="responses",
            operation="diagnose",
            logical_call_id="diagnose:primary",
            provider_account_fingerprint="f" * 64,
            request_payload=request_payload("changed script"),
            model_id="doubao-seed-2-0-lite-260215",
            model_version="260215",
        )


def test_send_marker_is_durable_before_response_and_stale_attempt_cannot_write(
    db_session: Session,
    running_intelligence_task: TaskRecord,
) -> None:
    journal = ProviderAttemptJournal(SystemClock())
    attempt = journal.prepare(
        db_session,
        task_id=running_intelligence_task.id,
        attempt_no=1,
        adapter="web_search",
        operation="research",
        logical_call_id="research:durian-history",
        provider_account_fingerprint="f" * 64,
        request_payload={"queries": ["durian history"]},
        model_id="doubao-seed-2-0-lite-260215",
        model_version="260215",
    )
    marked = journal.mark_send_started(db_session, attempt.id, task_attempt_no=1)
    assert marked.status == ProviderAttemptStatus.SEND_STARTED
    assert marked.provider_request_id is None
    running_intelligence_task.lease_expires_at = SystemClock().now() - timedelta(seconds=1)
    db_session.flush()
    with pytest.raises(Conflict, match="task lease was lost"):
        journal.record_response(
            db_session,
            attempt.id,
            task_attempt_no=1,
            provider_request_id="response-1",
            response_sha256="b" * 64,
        )


def test_response_identity_exact_replay_is_idempotent_and_changed_replay_conflicts(
    db_session: Session,
    running_intelligence_task: TaskRecord,
) -> None:
    journal = ProviderAttemptJournal(SystemClock())
    attempt = journal.prepare(
        db_session,
        task_id=running_intelligence_task.id,
        attempt_no=1,
        adapter="responses",
        operation="diagnose",
        logical_call_id="diagnose:primary",
        provider_account_fingerprint="f" * 64,
        request_payload=request_payload(),
        model_id="doubao-seed-2-0-lite-260215",
        model_version="260215",
    )
    journal.mark_send_started(db_session, attempt.id, task_attempt_no=1)
    first = journal.record_response(
        db_session,
        attempt.id,
        task_attempt_no=1,
        provider_request_id="response-1",
        response_sha256="c" * 64,
    )
    replay = journal.record_response(
        db_session,
        attempt.id,
        task_attempt_no=1,
        provider_request_id="response-1",
        response_sha256="c" * 64,
    )
    assert replay.id == first.id
    with pytest.raises(Conflict, match="provider response changed"):
        journal.record_response(
            db_session,
            attempt.id,
            task_attempt_no=1,
            provider_request_id="response-2",
            response_sha256="d" * 64,
        )
```

- [ ] **Step 2: Run the lifecycle tests and verify the journal service is missing**

Run: `cd backend && uv run pytest tests/integration/intelligence/test_provider_attempt_journal.py -q`

Expected: collection fails because `provider_journal.py` does not exist.

- [ ] **Step 3: Implement the journal with task-row fencing**

Create this service. The caller owns commit boundaries; `flush()` is not a durability claim.

```python
# backend/src/ip_saas/modules/intelligence/provider_journal.py
from __future__ import annotations

import re
from typing import Mapping
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.clock import Clock
from ip_saas.common.errors import Conflict
from ip_saas.common.tasking import TaskStatus
from ip_saas.modules.intelligence.provider_attempts import (
    IntelligenceProviderAttempt,
    ProviderAttemptStatus,
)
from ip_saas.modules.intelligence.provider_identity import JSONValue, logical_call_identity, normalize_provider_request_id
from ip_saas.modules.tasks.models import TaskRecord


class ProviderAttemptJournal:
    def __init__(self, clock: Clock) -> None:
        self._clock = clock

    def _lock_fenced_task(
        self,
        session: Session,
        task_id: UUID,
        attempt_no: int,
    ) -> TaskRecord:
        task = session.scalar(select(TaskRecord).where(TaskRecord.id == task_id).with_for_update())
        now = self._clock.now()
        if (
            task is None
            or TaskStatus(task.status) != TaskStatus.RUNNING
            or task.attempt_no != attempt_no
            or task.lease_expires_at is None
            or task.lease_expires_at <= now
        ):
            raise Conflict("task lease was lost")
        return task

    def prepare(
        self,
        session: Session,
        *,
        task_id: UUID,
        attempt_no: int,
        adapter: str,
        operation: str,
        logical_call_id: str,
        provider_account_fingerprint: str,
        request_payload: Mapping[str, JSONValue],
        model_id: str,
        model_version: str,
    ) -> IntelligenceProviderAttempt:
        if re.fullmatch(r"[0-9a-f]{64}", provider_account_fingerprint) is None:
            raise Conflict("provider account fingerprint must be lowercase SHA-256")
        task = self._lock_fenced_task(session, task_id, attempt_no)
        identity = logical_call_identity(
            task_id=task.id,
            request_fingerprint=task.request_fingerprint,
            adapter=adapter,
            operation=operation,
            logical_call_id=logical_call_id,
            request_payload=request_payload,
        )
        existing = session.scalar(
            select(IntelligenceProviderAttempt)
            .where(
                IntelligenceProviderAttempt.task_id == task.id,
                IntelligenceProviderAttempt.logical_call_key == identity.logical_call_key,
            )
            .with_for_update()
        )
        immutable = (
            task.initiated_by_account_id,
            task.model_registry_entry_id,
            "volcengine",
            provider_account_fingerprint,
            adapter,
            operation,
            task.capability,
            model_id,
            model_version,
            task.request_fingerprint,
            identity.request_payload_sha256,
        )
        if existing is not None:
            recorded = (
                existing.account_id,
                existing.model_registry_entry_id,
                existing.provider,
                existing.provider_account_fingerprint,
                existing.adapter,
                existing.operation,
                existing.capability,
                existing.model_id,
                existing.model_version,
                existing.request_fingerprint,
                existing.request_payload_sha256,
            )
            if recorded != immutable:
                raise Conflict("logical call input changed")
            return existing
        attempt = IntelligenceProviderAttempt(
            account_id=task.initiated_by_account_id,
            task_id=task.id,
            model_registry_entry_id=task.model_registry_entry_id,
            provider="volcengine",
            provider_account_fingerprint=provider_account_fingerprint,
            adapter=adapter,
            operation=operation,
            capability=task.capability,
            model_id=model_id,
            model_version=model_version,
            logical_call_key=identity.logical_call_key,
            client_attempt_id=identity.client_attempt_id,
            request_fingerprint=task.request_fingerprint,
            request_payload_sha256=identity.request_payload_sha256,
            first_worker_attempt_no=attempt_no,
            send_worker_attempt_no=None,
            status=ProviderAttemptStatus.PREPARED,
            prepared_at=self._clock.now(),
            send_started_at=None,
            response_observed_at=None,
            provider_request_id=None,
            response_sha256=None,
        )
        session.add(attempt)
        session.flush()
        return attempt

    def mark_send_started(
        self,
        session: Session,
        attempt_id: UUID,
        *,
        task_attempt_no: int,
    ) -> IntelligenceProviderAttempt:
        attempt = session.scalar(
            select(IntelligenceProviderAttempt)
            .where(IntelligenceProviderAttempt.id == attempt_id)
            .with_for_update()
        )
        if attempt is None:
            raise Conflict("provider attempt does not exist")
        self._lock_fenced_task(session, attempt.task_id, task_attempt_no)
        if attempt.status != ProviderAttemptStatus.PREPARED:
            raise Conflict("sent logical call cannot be sent again")
        attempt.status = ProviderAttemptStatus.SEND_STARTED
        attempt.send_worker_attempt_no = task_attempt_no
        attempt.send_started_at = self._clock.now()
        session.flush()
        return attempt

    def record_response(
        self,
        session: Session,
        attempt_id: UUID,
        *,
        task_attempt_no: int,
        provider_request_id: str,
        response_sha256: str,
    ) -> IntelligenceProviderAttempt:
        request_id = normalize_provider_request_id(provider_request_id)
        attempt = session.scalar(
            select(IntelligenceProviderAttempt)
            .where(IntelligenceProviderAttempt.id == attempt_id)
            .with_for_update()
        )
        if attempt is None:
            raise Conflict("provider attempt does not exist")
        self._lock_fenced_task(session, attempt.task_id, task_attempt_no)
        if attempt.status == ProviderAttemptStatus.RESPONSE_OBSERVED:
            if (attempt.provider_request_id, attempt.response_sha256) == (request_id, response_sha256):
                return attempt
            raise Conflict("provider response changed for an existing identity")
        if attempt.status != ProviderAttemptStatus.SEND_STARTED:
            raise Conflict("provider response has no durable send marker")
        if re.fullmatch(r"[0-9a-f]{64}", response_sha256) is None:
            raise Conflict("response_sha256 must be a lowercase SHA-256")
        attempt.status = ProviderAttemptStatus.RESPONSE_OBSERVED
        attempt.provider_request_id = request_id
        attempt.response_sha256 = response_sha256
        attempt.response_observed_at = self._clock.now()
        session.flush()
        return attempt
```

- [ ] **Step 4: Prove flush/commit boundaries and stale-attempt rejection**

Run: `cd backend && uv run pytest tests/integration/intelligence/test_provider_attempt_journal.py -q`

Expected: all tests pass against PostgreSQL. Add one two-session assertion in the same file: a row is invisible to a second session before the first commit and visible after commit; the test passes only when the Worker-facing integration in Task 7 uses `session_scope` around `prepare` and `mark_send_started` before HTTP.

- [ ] **Step 5: Commit the fenced lifecycle**

```bash
git add backend/tests/conftest.py backend/tests/support/provider_reconciliation_fixtures.py backend/src/ip_saas/modules/intelligence/provider_journal.py backend/tests/integration/intelligence/test_provider_attempt_journal.py
git commit -m "feat: journal provider calls before send"
```

### Task 4: Parse and ingest complete official supplier exports strictly

**Files:**
- Create: `backend/src/ip_saas/modules/intelligence/provider_evidence.py`
- Create: `backend/tests/integration/intelligence/test_supplier_evidence_ingestion.py`

- [ ] **Step 1: Write failing strict-schema and adversarial-ingestion tests**

Create the test with local builder functions rather than hidden fixtures. It must cover all of these exact mutations of one valid two-request bundle:

```python
# backend/tests/integration/intelligence/test_supplier_evidence_ingestion.py
from copy import deepcopy

import pytest
from pydantic import ValidationError

from ip_saas.common.errors import Conflict
from ip_saas.modules.intelligence.provider_evidence import (
    FinalSupplierStatementV1,
    OfficialRequestLogV1,
    validate_evidence_pair,
)


def request_log() -> dict[str, object]:
    return {
        "schema_version": 1,
        "provider": "volcengine",
        "provider_account_fingerprint": "a" * 64,
        "supplier_run_id": "00000000-0000-0000-0000-000000002401",
        "requested_from": "2026-08-25T00:00:00Z",
        "requested_through": "2026-08-25T01:00:00Z",
        "complete": True,
        "requests": [
            {
                "client_attempt_id": "00000000-0000-0000-0000-000000002411",
                "provider_request_id": "ark-request-1",
                "application_account_id": "00000000-0000-0000-0000-000000002201",
                "task_id": "00000000-0000-0000-0000-000000002205",
                "adapter": "responses",
                "capability": "intelligence.diagnose",
                "model_id": "doubao-seed-2-0-lite-260215",
                "model_version": "260215",
                "billable": True,
            },
            {
                "client_attempt_id": "00000000-0000-0000-0000-000000002412",
                "provider_request_id": "ark-request-2",
                "application_account_id": "00000000-0000-0000-0000-000000002201",
                "task_id": "00000000-0000-0000-0000-000000002205",
                "adapter": "web_search",
                "capability": "intelligence.diagnose",
                "model_id": "doubao-seed-2-0-lite-260215",
                "model_version": "260215",
                "billable": True,
            },
        ],
    }


def statement() -> dict[str, object]:
    return {
        "schema_version": 1,
        "provider": "volcengine",
        "provider_account_fingerprint": "a" * 64,
        "supplier_run_id": "00000000-0000-0000-0000-000000002401",
        "final": True,
        "charges": [
            {
                "provider_request_id": "ark-request-1",
                "native_quantity": "12",
                "native_unit": "token",
                "supplier_amount_minor": 2,
                "supplier_currency": "CNY",
                "amount_fen": 2,
            },
            {
                "provider_request_id": "ark-request-2",
                "native_quantity": "1",
                "native_unit": "request",
                "supplier_amount_minor": 3,
                "supplier_currency": "CNY",
                "amount_fen": 3,
            },
        ],
    }


def test_missing_partial_duplicate_extra_and_nonfinal_exports_are_rejected() -> None:
    missing = statement()
    missing["charges"] = missing["charges"][:1]  # type: ignore[index]
    with pytest.raises(Conflict, match="exactly match"):
        validate_evidence_pair(
OfficialRequestLogV1.model_validate(request_log()),
            FinalSupplierStatementV1.model_validate(missing),
        )

    duplicate = statement()
    duplicate["charges"] = [duplicate["charges"][0], duplicate["charges"][0]]  # type: ignore[index]
    with pytest.raises(ValidationError, match="duplicated"):
        FinalSupplierStatementV1.model_validate(duplicate)

    extra = statement()
    extra["charges"].append({**extra["charges"][0], "provider_request_id": "extra"})  # type: ignore[index,union-attr]
    with pytest.raises(Conflict, match="exactly match"):
        validate_evidence_pair(
            OfficialRequestLogV1.model_validate(request_log()),
            FinalSupplierStatementV1.model_validate(extra),
        )

    partial_log = deepcopy(request_log())
    partial_log["complete"] = False
    with pytest.raises(ValidationError):
        OfficialRequestLogV1.model_validate(partial_log)

    nonfinal = statement()
    nonfinal["final"] = False
    with pytest.raises(ValidationError):
        FinalSupplierStatementV1.model_validate(nonfinal)


def test_cross_account_run_and_trimmed_request_id_are_rejected() -> None:
    other_account = statement()
    other_account["provider_account_fingerprint"] = "b" * 64
    with pytest.raises(Conflict, match="same provider account and run"):
        validate_evidence_pair(
            OfficialRequestLogV1.model_validate(request_log()),
            FinalSupplierStatementV1.model_validate(other_account),
        )
    padded = request_log()
    padded["requests"][0]["provider_request_id"] = " padded "  # type: ignore[index]
    with pytest.raises(ValidationError):
        OfficialRequestLogV1.model_validate(padded)
```

The same test file must add a PostgreSQL ingestion case using `db_session`, `running_intelligence_task`, `ProviderAttemptJournal`, and `FakePrivateObjectStore`. Prepare two sent attempts whose stable client IDs are substituted into the valid builders. Assert: exact re-import returns the same bundle and creates no new matches; changed bytes under the same `(account_id, provider, supplier_run_id)` raise `Conflict`; a different account, task, model, capability, adapter, unsent attempt, stale `first_worker_attempt_no > task.attempt_no`, or unknown client ID creates neither a bundle nor a match.

- [ ] **Step 2: Run the evidence tests and verify the strict models are missing**

Run: `cd backend && env -u ARK_API_KEY uv run pytest tests/integration/intelligence/test_supplier_evidence_ingestion.py -q`

Expected: collection fails because `provider_evidence.py` does not exist; the environment contains no provider credential.

- [ ] **Step 3: Implement strict official-log and finalized-statement models**

Create these strict Pydantic contracts and pair validator:

```python
# backend/src/ip_saas/modules/intelligence/provider_evidence.py
from __future__ import annotations

import hashlib
from datetime import datetime
from decimal import Decimal
from typing import Annotated, Literal
from uuid import UUID

from pydantic import BaseModel, ConfigDict, Field, StringConstraints, model_validator

from ip_saas.common.errors import Conflict


TrimmedRequestId = Annotated[str, StringConstraints(strip_whitespace=False, min_length=1, max_length=200, pattern=r"^\S(?:.*\S)?$")]
Sha256Hex = Annotated[str, StringConstraints(pattern=r"^[0-9a-f]{64}$")]


class StrictEvidenceModel(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)


class OfficialRequestLineV1(StrictEvidenceModel):
    client_attempt_id: UUID
    provider_request_id: TrimmedRequestId
    application_account_id: UUID
    task_id: UUID
    adapter: Literal["responses", "web_search"]
    capability: str
    model_id: str
    model_version: str
    billable: Literal[True]


class OfficialRequestLogV1(StrictEvidenceModel):
    schema_version: Literal[1]
    provider: Literal["volcengine"]
    provider_account_fingerprint: Sha256Hex
    supplier_run_id: UUID
    requested_from: datetime
    requested_through: datetime
    complete: Literal[True]
    requests: tuple[OfficialRequestLineV1, ...]

    @model_validator(mode="after")
    def unique_requests(self) -> "OfficialRequestLogV1":
        clients = [item.client_attempt_id for item in self.requests]
        requests = [item.provider_request_id for item in self.requests]
        if len(set(clients)) != len(clients) or len(set(requests)) != len(requests):
            raise ValueError("official request identity is duplicated")
        if (
            self.requested_from.tzinfo is None
            or self.requested_through.tzinfo is None
            or self.requested_from >= self.requested_through
            or (self.requested_through - self.requested_from).total_seconds() > 86_400
        ):
            raise ValueError("official request window must be timezone-aware and at most 24 hours")
        return self


class FinalChargeLineV1(StrictEvidenceModel):
    provider_request_id: TrimmedRequestId
    native_quantity: Annotated[Decimal, Field(ge=0)]
    native_unit: str
    supplier_amount_minor: Annotated[int, Field(ge=0)]
    supplier_currency: Literal["CNY"]
    amount_fen: Annotated[int, Field(ge=0)]

    @model_validator(mode="after")
    def exact_cny_minor_units(self) -> "FinalChargeLineV1":
        if self.supplier_amount_minor != self.amount_fen:
            raise ValueError("CNY supplier minor units must equal amount_fen")
        return self


class FinalSupplierStatementV1(StrictEvidenceModel):
    schema_version: Literal[1]
    provider: Literal["volcengine"]
    provider_account_fingerprint: Sha256Hex
    supplier_run_id: UUID
    final: Literal[True]
    charges: tuple[FinalChargeLineV1, ...]

    @model_validator(mode="after")
    def unique_charges(self) -> "FinalSupplierStatementV1":
        requests = [item.provider_request_id for item in self.charges]
        if len(set(requests)) != len(requests):
            raise ValueError("supplier charge identity is duplicated")
        return self


def validate_evidence_pair(
    request_log: OfficialRequestLogV1,
    statement: FinalSupplierStatementV1,
) -> None:
    if (
        request_log.provider,
        request_log.provider_account_fingerprint,
        request_log.supplier_run_id,
    ) != (
        statement.provider,
        statement.provider_account_fingerprint,
        statement.supplier_run_id,
    ):
        raise Conflict("exports must cover the same provider account and run")
    request_ids = {item.provider_request_id for item in request_log.requests}
    charge_ids = {item.provider_request_id for item in statement.charges}
    if request_ids != charge_ids:
        raise Conflict("official request log and final charges must exactly match")


def evidence_sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()
```

- [ ] **Step 4: Implement scoped immutable ingestion and request matching**

In the same module, add `SupplierEvidenceIngestor.ingest(session, actor, account_id, request_log_raw, statement_raw) -> IntelligenceSupplierEvidenceBundle`. The method must execute this exact order:

1. Require `actor.kind == ActorKind.PLATFORM_REVIEWER`; otherwise raise `Forbidden`.
2. Parse both raw byte strings with `model_validate_json` and call `validate_evidence_pair` before any object-store write.
3. Lock any bundle by `(account_id, provider, supplier_run_id)`. Exact request/statement hashes return it; changed hashes raise `Conflict`.
4. Require `actor.account_id == account_id`; here `account_id` is the platform account that owns the dedicated Ark provider contract, not a customer account. Lock every attempt whose provider-account fingerprint and `send_started_at` fall inside the official complete window. Require each official `client_attempt_id` to map to exactly one of those rows, require `attempt.account_id == line.application_account_id`, `attempt.task_id == line.task_id`, and require the locked `TaskRecord` to carry that same initiated account. Its status must be `send_started` or `response_observed`, `first_worker_attempt_no <= TaskRecord.attempt_no`, and adapter/capability/model ID/model version must equal the official line. Unknown, cross-account, cross-task, cross-model, stale, or out-of-window records raise `Conflict` before inserting a bundle. The dedicated Ark sub-account contains no unrelated traffic, so an official log line without a journal row is an unexplained extra call and rejects the whole bundle.
5. If a journal row already contains `provider_request_id`, require exact equality; if it is absent because the crash preceded response persistence, copy the exact official request ID into the journal row. For every official request, join the final charge by request ID, construct one `IntelligenceSupplierRequestMatch`, set `supplier_evidence_bundle_id`, and set the attempt to `supplier_matched`. For each window-covered send marker absent from the complete request log, require `provider_request_id is None`, set `supplier_evidence_bundle_id`, and set status `supplier_no_call`; this is the positive supplier-side zero-call result. A complete empty request log plus final empty statement is valid only when every covered send marker becomes `supplier_no_call`.
6. Write the validated raw bytes through the existing `PrivateObjectStore.put_bytes` to server-generated, content-addressed keys `provider-evidence/{account_id}/{provider}/{supplier_run_id}/{sha256}/request-log.json` and `.../statement.json`. Never use a client key as an object or ledger identity.
7. Insert one bundle and all matches, attach the bundle to every resolved attempt, and flush once. An exact transaction retry finds the scoped bundle before changing status or writing another match.

Implement the service as one public method with private `_lock_attempts` and `_build_matches` helpers; do not add an HTTP upload endpoint. Official exports enter only through the restricted scanner import command in Task 6.

Append this concrete implementation to the same module, adding the shown imports. It deliberately writes content-addressed TOS objects only after all cross-scope validation and before the database flush; an interrupted object write can leave only an unreferenced immutable object, while a database row can never reference missing bytes returned by `put_bytes`.

```python
import json

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.clock import Clock
from ip_saas.common.errors import Forbidden
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import Account, AccountKind
from ip_saas.modules.intelligence.provider_attempts import (
    IntelligenceProviderAttempt,
    IntelligenceSupplierEvidenceBundle,
    IntelligenceSupplierRequestMatch,
    ProviderAttemptStatus,
)
from ip_saas.modules.tasks.models import TaskRecord
from ip_saas.providers.object_store import PrivateObjectStore


class SupplierEvidenceIngestor:
    def __init__(self, *, store: PrivateObjectStore, clock: Clock) -> None:
        self._store = store
        self._clock = clock

    @staticmethod
    def _match_fingerprint(payload: dict[str, object]) -> str:
        raw = json.dumps(payload, default=str, separators=(",", ":"), sort_keys=True).encode("utf-8")
        return hashlib.sha256(raw).hexdigest()

    def ingest(
        self,
        session: Session,
        *,
        actor: ActorContext,
        account_id: UUID,
        request_log_raw: bytes,
        statement_raw: bytes,
    ) -> IntelligenceSupplierEvidenceBundle:
        if actor.kind != ActorKind.PLATFORM_REVIEWER or actor.account_id != account_id:
            raise Forbidden("platform reviewer for the provider-owner account is required")
        owner = session.get(Account, account_id)
        if owner is None or owner.kind != AccountKind.PLATFORM:
            raise Forbidden("provider evidence owner must be the platform account")
        request_log = OfficialRequestLogV1.model_validate_json(request_log_raw)
        statement = FinalSupplierStatementV1.model_validate_json(statement_raw)
        validate_evidence_pair(request_log, statement)
        request_log_sha = evidence_sha256(request_log_raw)
        statement_sha = evidence_sha256(statement_raw)
        existing = session.scalar(
            select(IntelligenceSupplierEvidenceBundle)
            .where(
                IntelligenceSupplierEvidenceBundle.account_id == account_id,
                IntelligenceSupplierEvidenceBundle.provider == request_log.provider,
                IntelligenceSupplierEvidenceBundle.supplier_run_id == request_log.supplier_run_id,
            )
            .with_for_update()
        )
        if existing is not None:
            if (existing.request_log_sha256, existing.statement_sha256) == (
                request_log_sha,
                statement_sha,
            ):
                return existing
            raise Conflict("supplier run was replayed with changed evidence")
        attempts = tuple(
            session.scalars(
                select(IntelligenceProviderAttempt)
                .where(
                    IntelligenceProviderAttempt.provider == request_log.provider,
                    IntelligenceProviderAttempt.provider_account_fingerprint
                    == request_log.provider_account_fingerprint,
                    IntelligenceProviderAttempt.send_started_at >= request_log.requested_from,
                    IntelligenceProviderAttempt.send_started_at < request_log.requested_through,
                    IntelligenceProviderAttempt.status.in_(
                        (
                            ProviderAttemptStatus.SEND_STARTED,
                            ProviderAttemptStatus.RESPONSE_OBSERVED,
                        )
                    ),
                )
                .order_by(IntelligenceProviderAttempt.client_attempt_id)
                .with_for_update()
            )
        )
        if not attempts:
            raise Conflict("official evidence window covers no unresolved send marker")
        by_client = {item.client_attempt_id: item for item in attempts}
        lines = {item.client_attempt_id: item for item in request_log.requests}
        if not set(lines) <= set(by_client):
            raise Conflict("official request log contains an unexplained extra call")
        charges = {item.provider_request_id: item for item in statement.charges}
        for attempt in attempts:
            task = session.get(TaskRecord, attempt.task_id)
            if task is None:
                raise Conflict("supplier evidence references a missing task")
            if attempt.first_worker_attempt_no > task.attempt_no:
                raise Conflict("supplier evidence references a stale future attempt")
            line = lines.get(attempt.client_attempt_id)
            if line is None:
                if attempt.provider_request_id is not None:
                    raise Conflict("complete log omits an already observed provider request")
                continue
            if (
                line.application_account_id != attempt.account_id
                or line.task_id != attempt.task_id
                or task.initiated_by_account_id != line.application_account_id
                or line.adapter != attempt.adapter
                or line.capability != attempt.capability
                or line.model_id != attempt.model_id
                or line.model_version != attempt.model_version
            ):
                raise Conflict("official request line crosses task, account, adapter, or model scope")
            if attempt.provider_request_id not in {None, line.provider_request_id}:
                raise Conflict("observed response identity differs from official request log")
            if line.provider_request_id not in charges:
                raise Conflict("official request has no finalized charge line")
        request_key = (
            f"provider-evidence/{account_id}/{request_log.provider}/"
            f"{request_log.supplier_run_id}/{request_log_sha}/request-log.json"
        )
        statement_key = (
            f"provider-evidence/{account_id}/{request_log.provider}/"
            f"{request_log.supplier_run_id}/{statement_sha}/statement.json"
        )
        request_object = self._store.put_bytes(
            request_key, request_log_raw, "application/json"
        )
        statement_object = self._store.put_bytes(
            statement_key, statement_raw, "application/json"
        )
        if (
            request_object.sha256 != request_log_sha
            or statement_object.sha256 != statement_sha
        ):
            raise Conflict("private evidence store returned a changed object digest")
        bundle = IntelligenceSupplierEvidenceBundle(
            account_id=account_id,
            provider=request_log.provider,
            provider_account_fingerprint=request_log.provider_account_fingerprint,
            supplier_run_id=request_log.supplier_run_id,
            requested_from=request_log.requested_from,
            requested_through=request_log.requested_through,
            request_log_object_key=request_object.key,
            request_log_sha256=request_log_sha,
            statement_object_key=statement_object.key,
            statement_sha256=statement_sha,
            is_complete=True,
            is_final=True,
            imported_by=actor.actor_id,
            imported_at=self._clock.now(),
        )
        session.add(bundle)
        session.flush()
        for attempt in attempts:
            line = lines.get(attempt.client_attempt_id)
            attempt.supplier_evidence_bundle_id = bundle.id
            if line is None:
                attempt.status = ProviderAttemptStatus.SUPPLIER_NO_CALL
                continue
            charge = charges[line.provider_request_id]
            attempt.provider_request_id = line.provider_request_id
            attempt.status = ProviderAttemptStatus.SUPPLIER_MATCHED
            payload = {
                "attempt_id": attempt.id,
                "task_id": attempt.task_id,
                "provider": attempt.provider,
                "provider_request_id": line.provider_request_id,
                "capability": attempt.capability,
                "model_id": attempt.model_id,
                "model_version": attempt.model_version,
                "native_quantity": charge.native_quantity,
                "native_unit": charge.native_unit,
                "supplier_amount_minor": charge.supplier_amount_minor,
                "supplier_currency": charge.supplier_currency,
                "amount_fen": charge.amount_fen,
            }
            session.add(
                IntelligenceSupplierRequestMatch(
                    bundle_id=bundle.id,
                    attempt_id=attempt.id,
                    account_id=attempt.account_id,
                    task_id=attempt.task_id,
                    provider=attempt.provider,
                    provider_request_id=line.provider_request_id,
                    capability=attempt.capability,
                    model_id=attempt.model_id,
                    model_version=attempt.model_version,
                    native_quantity=charge.native_quantity,
                    native_unit=charge.native_unit,
                    supplier_amount_minor=charge.supplier_amount_minor,
                    supplier_currency=charge.supplier_currency,
                    amount_fen=charge.amount_fen,
                    request_fingerprint=self._match_fingerprint(payload),
                )
            )
        session.flush()
        return bundle
```

- [ ] **Step 5: Run all strict-ingestion cases and commit**

Run: `cd backend && env -u ARK_API_KEY uv run pytest tests/integration/intelligence/test_supplier_evidence_ingestion.py -q`

Expected: all cases pass against PostgreSQL, the fake object store contains exactly two content-addressed objects after an exact replay, rejected cases leave bundle/match counts unchanged, and no network call occurs.

```bash
git add backend/src/ip_saas/modules/intelligence/provider_evidence.py backend/tests/integration/intelligence/test_supplier_evidence_ingestion.py
git commit -m "feat: ingest complete supplier evidence strictly"
```

### Task 5: Implement Plan 01 evidence ports and the once-per-task batch scanner

**Files:**
- Create: `backend/src/ip_saas/modules/intelligence/provider_reconciliation.py`
- Create: `backend/tests/integration/intelligence/test_provider_reconciliation_scanner.py`

- [ ] **Step 1: Write failing port and scanner tests**

Create the integration file with explicit factories imported from `tests.support.provider_reconciliation_fixtures`. Its first group must assert:

```python
# backend/tests/integration/intelligence/test_provider_reconciliation_scanner.py
from unittest.mock import Mock

import pytest
from sqlalchemy.orm import Session

from ip_saas.common.errors import Conflict
from ip_saas.modules.intelligence.provider_reconciliation import (
    JournalNoProviderCallEvidencePort,
    JournalProviderCostManifestPort,
    ProviderReconciliationScanner,
)
from ip_saas.modules.tasks.models import TaskRecord


def test_no_call_proof_requires_pinned_egress_attestation_and_no_send_marker(
    db_session: Session,
    running_intelligence_task: TaskRecord,
) -> None:
    missing_pin = JournalNoProviderCallEvidencePort(
        runtime_evidence_ref="",
        runtime_evidence_sha256="",
    )
    with pytest.raises(Conflict, match="runtime attestation"):
        missing_pin.require_no_provider_call(
            db_session,
            running_intelligence_task.id,
            running_intelligence_task.attempt_no,
        )


def test_manifest_rejects_any_sent_attempt_without_exact_final_supplier_match(
    db_session: Session,
    running_intelligence_task: TaskRecord,
) -> None:
    manifest = JournalProviderCostManifestPort()
    with pytest.raises(Conflict, match="complete supplier match"):
        manifest.require_complete_provider_requests(
            db_session,
            running_intelligence_task.id,
            running_intelligence_task.attempt_no,
        )


def test_scanner_calls_one_batch_finalizer_for_two_supplier_requests() -> None:
    finalizer = Mock()
    scanner = ProviderReconciliationScanner(finalizer=finalizer)
    task = Mock(
        id="00000000-0000-0000-0000-000000002501",
        attempt_no=8,
        billing_mode="internal_cost",
    )
    first = Mock(amount_fen=2)
    second = Mock(amount_fen=3)
    scanner._finalize_paid_task(Mock(), task, (first, second))
    finalizer.finalize_provider_costs.assert_called_once()
    call = finalizer.finalize_provider_costs.call_args.kwargs
    assert call["provider_costs"] == (first, second)
    assert call["actual_amount"] == 5


def test_scanner_customer_failure_uses_zero_actual_amount() -> None:
    finalizer = Mock()
    scanner = ProviderReconciliationScanner(finalizer=finalizer)
    task = Mock(
        id="00000000-0000-0000-0000-000000002502",
        attempt_no=8,
        billing_mode="customer_credit",
    )
    scanner._finalize_paid_task(Mock(), task, (Mock(amount_fen=7),))
    assert finalizer.finalize_provider_costs.call_args.kwargs["actual_amount"] == 0
```

Add real PostgreSQL cases in the same file that create two sent attempts and a valid ingested bundle using Task 4. Move the task to `TaskStatus.RECONCILIATION_REQUIRED`, clear its lease, and use the real Plan 01 `TaskReconciliationService`, hold adapters, and Plan 05 `BillingService`. Seed an effective customer or internal limit with the existing `real_customer_limits` or `real_internal_limits` factory before constructing the task and assert the resolved limit-version ID; no allow fake or production deny adapter is permitted in this cross-module integration. Assert one scan transaction produces exactly two `ProviderCostEntry` rows with `task_id == task.id`, one failed task, one terminal hold transition, and two unchanged `supplier_matched` attempt dispositions. Repeat the scan and assert every count, disposition, and balance is unchanged. Add negative parameter cases for one missing match, an extra match from another task, a match from `first_worker_attempt_no > task.attempt_no`, and a non-final bundle inserted through a direct corruption fixture; each case must raise `Conflict` and leave the task and hold unchanged.

- [ ] **Step 2: Run the scanner tests and verify the module is missing**

Run: `cd backend && uv run pytest tests/integration/intelligence/test_provider_reconciliation_scanner.py -q`

Expected: collection fails because `provider_reconciliation.py` does not exist.

- [ ] **Step 3: Implement the two frozen Plan 01 evidence ports**

Create `provider_reconciliation.py`. The zero-call proof combines the complete journal snapshot with the reviewed runtime/egress attestation; database absence by itself is never sufficient.

```python
# backend/src/ip_saas/modules/intelligence/provider_reconciliation.py
from __future__ import annotations

import hashlib
import json
import re
from dataclasses import dataclass
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.errors import Conflict
from ip_saas.common.tasking import TaskStatus
from ip_saas.modules.billing.models import ReconciliationStatus
from ip_saas.modules.billing.reconciliation import (
    NoProviderCallEvidence,
    ProviderRequestIdentity,
    TaskReconciliationService,
)
from ip_saas.modules.billing.service import BillingMode, ProviderCostInput
from ip_saas.modules.intelligence.provider_attempts import (
    IntelligenceProviderAttempt,
    IntelligenceSupplierEvidenceBundle,
    IntelligenceSupplierRequestMatch,
    ProviderAttemptStatus,
)
from ip_saas.modules.tasks.models import TaskRecord


SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
SENT_STATUSES = {
    ProviderAttemptStatus.SEND_STARTED,
    ProviderAttemptStatus.RESPONSE_OBSERVED,
    ProviderAttemptStatus.SUPPLIER_MATCHED,
    ProviderAttemptStatus.SUPPLIER_NO_CALL,
}


class JournalNoProviderCallEvidencePort:
    def __init__(self, *, runtime_evidence_ref: str, runtime_evidence_sha256: str) -> None:
        self._runtime_ref = runtime_evidence_ref
        self._runtime_sha256 = runtime_evidence_sha256

    def require_no_provider_call(
        self,
        session: Session,
        task_id: UUID,
        through_attempt_no: int,
    ) -> NoProviderCallEvidence:
        if not self._runtime_ref.strip() or SHA256_RE.fullmatch(self._runtime_sha256) is None:
            raise Conflict("pinned runtime attestation is required for zero-call evidence")
        attempts = tuple(
            session.scalars(
                select(IntelligenceProviderAttempt)
                .where(
                    IntelligenceProviderAttempt.task_id == task_id,
                )
                .order_by(IntelligenceProviderAttempt.logical_call_key)
                .with_for_update()
            )
        )
        if any(
            item.first_worker_attempt_no > through_attempt_no
            or (item.send_worker_attempt_no or 0) > through_attempt_no
            for item in attempts
        ):
            raise Conflict("provider journal contains a stale future task attempt")
        unresolved = [item for item in attempts if item.status in {ProviderAttemptStatus.SEND_STARTED, ProviderAttemptStatus.RESPONSE_OBSERVED, ProviderAttemptStatus.SUPPLIER_MATCHED}]
        if unresolved:
            raise Conflict("every send marker needs a final supplier disposition")
        no_call = [item for item in attempts if item.status == ProviderAttemptStatus.SUPPLIER_NO_CALL]
        bundle_ids = {item.supplier_evidence_bundle_id for item in no_call}
        if None in bundle_ids:
            raise Conflict("supplier no-call disposition is missing its evidence bundle")
        bundles = tuple(
            session.scalars(
                select(IntelligenceSupplierEvidenceBundle)
                .where(IntelligenceSupplierEvidenceBundle.id.in_(bundle_ids))
                .with_for_update()
            )
        ) if bundle_ids else ()
        if len(bundles) != len(bundle_ids) or any(not item.is_complete or not item.is_final for item in bundles):
            raise Conflict("supplier no-call evidence is incomplete or non-final")
        snapshot = {
            "task_id": str(task_id),
            "through_attempt_no": through_attempt_no,
            "runtime_evidence_ref": self._runtime_ref,
            "runtime_evidence_sha256": self._runtime_sha256,
            "prepared_calls": [
                {
                    "id": str(item.id),
                    "logical_call_key": item.logical_call_key,
                    "client_attempt_id": str(item.client_attempt_id),
                    "status": item.status,
                    "first_worker_attempt_no": item.first_worker_attempt_no,
                }
                for item in attempts
            ],
            "supplier_zero_call_bundles": [
                {
                    "id": str(item.id),
                    "request_log_sha256": item.request_log_sha256,
                    "statement_sha256": item.statement_sha256,
                }
                for item in sorted(bundles, key=lambda value: str(value.id))
            ],
        }
        canonical = json.dumps(snapshot, separators=(",", ":"), sort_keys=True).encode("utf-8")
        return NoProviderCallEvidence(
            task_id=task_id,
            through_attempt_no=through_attempt_no,
            evidence_ref=f"{self._runtime_ref}#task={task_id}&attempt={through_attempt_no}",
            evidence_sha256=hashlib.sha256(canonical).hexdigest(),
        )


class JournalProviderCostManifestPort:
    def require_complete_provider_requests(
        self,
        session: Session,
        task_id: UUID,
        through_attempt_no: int,
    ) -> tuple[ProviderRequestIdentity, ...]:
        sent = tuple(
            session.scalars(
                select(IntelligenceProviderAttempt)
                .where(
                    IntelligenceProviderAttempt.task_id == task_id,
                    IntelligenceProviderAttempt.status.in_(SENT_STATUSES),
                )
                .order_by(IntelligenceProviderAttempt.logical_call_key)
                .with_for_update()
            )
        )
        if any(
            item.first_worker_attempt_no > through_attempt_no
            or (item.send_worker_attempt_no or 0) > through_attempt_no
            for item in sent
        ):
            raise Conflict("provider manifest contains a stale future task attempt")
        if not sent:
            raise Conflict("provider request manifest is empty")
        matches = tuple(
            session.scalars(
                select(IntelligenceSupplierRequestMatch)
                .join(
                    IntelligenceSupplierEvidenceBundle,
                    IntelligenceSupplierEvidenceBundle.id == IntelligenceSupplierRequestMatch.bundle_id,
                )
                .where(
                    IntelligenceSupplierRequestMatch.task_id == task_id,
                    IntelligenceSupplierEvidenceBundle.is_complete.is_(True),
                    IntelligenceSupplierEvidenceBundle.is_final.is_(True),
                )
                .with_for_update()
            )
        )
        by_attempt = {item.attempt_id: item for item in matches}
        paid_attempts = {item.id for item in sent if item.status == ProviderAttemptStatus.SUPPLIER_MATCHED}
        no_call_attempts = [item for item in sent if item.status == ProviderAttemptStatus.SUPPLIER_NO_CALL]
        if len(by_attempt) != len(matches) or set(by_attempt) != paid_attempts:
            raise Conflict("every billed attempt requires one complete supplier match")
        if any(item.supplier_evidence_bundle_id is None for item in no_call_attempts):
            raise Conflict("every non-billed send marker needs final supplier no-call evidence")
        if paid_attempts | {item.id for item in no_call_attempts} != {item.id for item in sent}:
            raise Conflict("every sent attempt needs a final supplier disposition")
        identities = tuple(
            sorted(
                (
                    ProviderRequestIdentity(item.provider, item.provider_request_id)
                    for item in matches
                ),
                key=lambda item: (item.provider, item.provider_request_id),
            )
        )
        if len(set(identities)) != len(identities):
            raise Conflict("supplier request manifest is duplicated")
        return identities
```

- [ ] **Step 4: Implement one locked task batch per scanner transaction**

Append this scanner to the same module:

```python
@dataclass(frozen=True)
class ScanResult:
    selected: int
    finalized: int


class ProviderReconciliationScanner:
    def __init__(self, *, finalizer: TaskReconciliationService) -> None:
        self._finalizer = finalizer

    @staticmethod
    def _costs(session: Session, task: TaskRecord) -> tuple[ProviderCostInput, ...]:
        matches = tuple(
            session.scalars(
                select(IntelligenceSupplierRequestMatch)
                .where(IntelligenceSupplierRequestMatch.task_id == task.id)
                .order_by(
                    IntelligenceSupplierRequestMatch.provider,
                    IntelligenceSupplierRequestMatch.provider_request_id,
                )
                .with_for_update()
            )
        )
        return tuple(
            ProviderCostInput(
                provider=item.provider,
                capability=item.capability,
                model_id=item.model_id,
                model_version=item.model_version,
                native_quantity=item.native_quantity,
                native_unit=item.native_unit,
                supplier_amount_minor=item.supplier_amount_minor,
                supplier_currency=item.supplier_currency,
                amount_fen=item.amount_fen,
                reconciliation_status=ReconciliationStatus.MATCHED,
                task_id=task.id,
                provider_request_id=item.provider_request_id,
            )
            for item in matches
        )

    def _finalize_paid_task(
        self,
        session: Session,
        task: TaskRecord,
        costs: tuple[ProviderCostInput, ...],
    ) -> None:
        actual = 0 if BillingMode(task.billing_mode) == BillingMode.CUSTOMER_CREDIT else sum(item.amount_fen for item in costs)
        self._finalizer.finalize_provider_costs(
            session=session,
            task_id=task.id,
            attempt_no=task.attempt_no,
            actual_amount=actual,
            provider_costs=costs,
            reason="intelligence provider attempts exhausted after supplier reconciliation",
            idempotency_key=f"task-reconciliation:{task.id}:{task.attempt_no}:supplier-manifest",
        )

    def scan_one_batch(self, session: Session, *, batch_size: int = 25) -> ScanResult:
        tasks = tuple(
            session.scalars(
                select(TaskRecord)
                .where(
                    TaskRecord.status == TaskStatus.RECONCILIATION_REQUIRED,
                    TaskRecord.capability.like("intelligence.%"),
                )
                .order_by(TaskRecord.created_at, TaskRecord.id)
                .limit(batch_size)
                .with_for_update(skip_locked=True)
            )
        )
        finalized = 0
        for task in tasks:
            sent = session.scalar(
                select(IntelligenceProviderAttempt.id).where(
                    IntelligenceProviderAttempt.task_id == task.id,
                    IntelligenceProviderAttempt.status.in_(SENT_STATUSES),
                ).limit(1)
            )
            costs = self._costs(session, task)
            if sent is None or not costs:
                self._finalizer.finalize_no_provider_call(
                    session=session,
                    task_id=task.id,
                    attempt_no=task.attempt_no,
                    reason="intelligence provider attempts exhausted before any send",
                    idempotency_key=f"task-reconciliation:{task.id}:{task.attempt_no}:no-send",
                )
            else:
                self._finalize_paid_task(session, task, costs)
            finalized += 1
        session.flush()
        return ScanResult(selected=len(tasks), finalized=finalized)
```

The independent process wraps each call to `scan_one_batch` in one `session_scope`. Any missing or conflicting evidence rolls back the task, hold, costs, matches, and attempt status together. A bad task does not get skipped inside that transaction; the process logs its server task ID and exits non-zero so operations can correct evidence before continuing.

- [ ] **Step 5: Run real batch accounting, replay, and rejection tests**

Run:

```bash
cd backend
uv run pytest tests/integration/intelligence/test_provider_reconciliation_scanner.py -q
uv run pytest tests/integration/tasks/test_task_transaction.py tests/integration/billing/test_internal_budget.py -q
```

Expected: all scanner cases pass; two supplier calls create two task-linked cost rows and one terminal accounting transition; exact scanner replay is a no-op; the Plan 01 transaction and internal-budget regressions pass.

- [ ] **Step 6: Commit the single batch finalizer path**

```bash
git add backend/src/ip_saas/modules/intelligence/provider_reconciliation.py backend/tests/integration/intelligence/test_provider_reconciliation_scanner.py
git commit -m "feat: reconcile exhausted intelligence tasks in one batch"
```

### Task 6: Build the restricted supplier-import and independent scanner process

**Files:**
- Create: `backend/scripts/reconcile_intelligence_provider_costs.py`
- Create: `backend/src/ip_saas/modules/intelligence/reconciliation_composition.py`
- Modify: `backend/src/ip_saas/config.py`
- Modify: `.env.example`
- Create: `docs/runbooks/intelligence-provider-reconciliation.md`
- Test: `backend/tests/unit/intelligence/test_reconciliation_cli.py`

- [ ] **Step 1: Write failing CLI tests with injected dependencies**

Create `test_reconciliation_cli.py` with a temporary request log and statement built from Task 4. Patch only `build_runtime`, then assert:

```python
# backend/tests/unit/intelligence/test_reconciliation_cli.py
from pathlib import Path
from unittest.mock import Mock

from scripts import reconcile_intelligence_provider_costs as cli


def test_import_command_requires_both_official_exports(tmp_path: Path) -> None:
    args = cli.parser().parse_args(
        [
            "import",
            "--provider-owner-account-id", "00000000-0000-0000-0000-000000002601",
            "--reviewer-actor-id", "00000000-0000-0000-0000-000000002602",
            "--request-log", str(tmp_path / "missing-request-log.json"),
            "--final-statement", str(tmp_path / "missing-statement.json"),
        ]
    )
    assert cli.run(args, runtime=Mock()) == 66


def test_scan_command_commits_each_batch_separately() -> None:
    runtime = Mock()
    runtime.scan_batch.side_effect = [(3, 3), (0, 0)]
    args = cli.parser().parse_args(["scan", "--batch-size", "3", "--max-batches", "2"])
    assert cli.run(args, runtime=runtime) == 0
    assert runtime.scan_batch.call_count == 2
```

- [ ] **Step 2: Run the CLI tests and verify the entry point is missing**

Run: `cd backend && uv run pytest tests/unit/intelligence/test_reconciliation_cli.py -q`

Expected: collection fails because the script does not exist.

- [ ] **Step 3: Implement the synchronous restricted command**

Create the script with `import` and `scan` subcommands. `build_runtime()` composes `session_scope`, `TosPrivateObjectStore`, `SupplierEvidenceIngestor`, both Task 5 evidence ports, the existing Plan 01 `BillingService` with the production `GenerationLimitService`, and `TaskReconciliationService`; it must not use `AllowAllGenerationLimits`, `UnconfiguredGenerationLimits`, or a deny adapter. The module-level `run` function stays dependency-injectable and contains no network client construction.

```python
# backend/scripts/reconcile_intelligence_provider_costs.py
from __future__ import annotations

import argparse
from pathlib import Path
from typing import Protocol
from uuid import UUID


class ReconciliationRuntime(Protocol):
    def import_evidence(
        self,
        *,
        provider_owner_account_id: UUID,
        reviewer_actor_id: UUID,
        request_log_raw: bytes,
        statement_raw: bytes,
    ) -> UUID:
        raise NotImplementedError

    def scan_batch(self, batch_size: int) -> tuple[int, int]:
        raise NotImplementedError


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser()
    commands = root.add_subparsers(dest="command", required=True)
    ingest = commands.add_parser("import")
    ingest.add_argument("--provider-owner-account-id", type=UUID, required=True)
    ingest.add_argument("--reviewer-actor-id", type=UUID, required=True)
    ingest.add_argument("--request-log", type=Path, required=True)
    ingest.add_argument("--final-statement", type=Path, required=True)
    scan = commands.add_parser("scan")
    scan.add_argument("--batch-size", type=int, default=25)
    scan.add_argument("--max-batches", type=int, default=40)
    return root


def run(args: argparse.Namespace, *, runtime: ReconciliationRuntime) -> int:
    if args.command == "import":
        if not args.request_log.is_file() or not args.final_statement.is_file():
            return 66
        bundle_id = runtime.import_evidence(
            provider_owner_account_id=args.provider_owner_account_id,
            reviewer_actor_id=args.reviewer_actor_id,
            request_log_raw=args.request_log.read_bytes(),
            statement_raw=args.final_statement.read_bytes(),
        )
        print(f"imported_bundle={bundle_id}")
        return 0
    if not 1 <= args.batch_size <= 100 or not 1 <= args.max_batches <= 1000:
        return 64
    for _ in range(args.max_batches):
        selected, finalized = runtime.scan_batch(args.batch_size)
        print(f"selected={selected} finalized={finalized}")
        if selected == 0:
            return 0
    return 75


def build_runtime() -> ReconciliationRuntime:
    from ip_saas.modules.intelligence.reconciliation_composition import (
        build_provider_reconciliation_runtime,
    )

    return build_provider_reconciliation_runtime()


def main() -> int:
    return run(parser().parse_args(), runtime=build_runtime())


if __name__ == "__main__":
    raise SystemExit(main())
```

Add these exact settings to `Settings` and disabled defaults to `.env.example`:

```python
    ark_plan02a_runtime_evidence_ref: str = ""
    ark_plan02a_runtime_evidence_sha256: str = ""
```

```bash
ARK_PLAN02A_RUNTIME_EVIDENCE_REF=
ARK_PLAN02A_RUNTIME_EVIDENCE_SHA256=
```

Create `backend/src/ip_saas/modules/intelligence/reconciliation_composition.py`. Its concrete runtime opens a fresh `session_scope` for every import or batch, and its builder uses Plan 05's only production billing constructor:

```python
from __future__ import annotations

import re
from uuid import UUID

from ip_saas.common.clock import SystemClock
from ip_saas.common.errors import Forbidden
from ip_saas.config import get_settings
from ip_saas.db.session import session_scope
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.billing.composition import build_billing_service
from ip_saas.modules.billing.reconciliation import TaskReconciliationService
from ip_saas.modules.intelligence.provider_evidence import SupplierEvidenceIngestor
from ip_saas.modules.intelligence.provider_reconciliation import (
    JournalNoProviderCallEvidencePort,
    JournalProviderCostManifestPort,
    ProviderReconciliationScanner,
)
from ip_saas.providers.object_store import PrivateObjectStore


class SynchronousProviderReconciliationRuntime:
    def __init__(
        self,
        *,
        store: PrivateObjectStore,
        ingestor: SupplierEvidenceIngestor,
        scanner: ProviderReconciliationScanner,
    ) -> None:
        self._store = store
        self._ingestor = ingestor
        self._scanner = scanner

    def import_evidence(
        self,
        *,
        provider_owner_account_id: UUID,
        reviewer_actor_id: UUID,
        request_log_raw: bytes,
        statement_raw: bytes,
    ) -> UUID:
        actor = ActorContext(
            actor_id=reviewer_actor_id,
            account_id=provider_owner_account_id,
            kind=ActorKind.PLATFORM_REVIEWER,
        )
        with session_scope() as session:
            bundle = self._ingestor.ingest(
                session,
                actor=actor,
                account_id=provider_owner_account_id,
                request_log_raw=request_log_raw,
                statement_raw=statement_raw,
            )
            return bundle.id

    def scan_batch(self, batch_size: int) -> tuple[int, int]:
        with session_scope() as session:
            result = self._scanner.scan_one_batch(session, batch_size=batch_size)
            return result.selected, result.finalized


def build_provider_reconciliation_runtime() -> SynchronousProviderReconciliationRuntime:
    from ip_saas.api import build_private_object_store

    settings = get_settings()
    if (
        not settings.ark_plan02a_runtime_evidence_ref.strip()
        or re.fullmatch(r"[0-9a-f]{64}", settings.ark_plan02a_runtime_evidence_sha256) is None
    ):
        raise Forbidden("Plan 02a runtime evidence is not pinned")
    store = build_private_object_store(settings)
    if store is None:
        raise Forbidden("private TOS evidence store is not configured")
    clock = SystemClock()
    billing = build_billing_service(clock)
    no_call = JournalNoProviderCallEvidencePort(
        runtime_evidence_ref=settings.ark_plan02a_runtime_evidence_ref,
        runtime_evidence_sha256=settings.ark_plan02a_runtime_evidence_sha256,
    )
    manifest = JournalProviderCostManifestPort()
    finalizer = TaskReconciliationService(billing, no_call, manifest)
    return SynchronousProviderReconciliationRuntime(
        store=store,
        ingestor=SupplierEvidenceIngestor(store=store, clock=clock),
        scanner=ProviderReconciliationScanner(finalizer=finalizer),
    )
```

`build_billing_service(clock)` supplies the mandatory real `GenerationLimitService`. Unit tests inject an explicit runtime and never call this production builder. No production deny adapter or allow fake remains in this composition.

- [ ] **Step 4: Write the operational runbook with exact exits and transaction behavior**

Document these commands in `docs/runbooks/intelligence-provider-reconciliation.md`:

```bash
cd backend
env -u ARK_API_KEY uv run python scripts/reconcile_intelligence_provider_costs.py import \
  --provider-owner-account-id "${IP_SAAS_PROVIDER_OWNER_ACCOUNT_ID}" \
  --reviewer-actor-id "${IP_SAAS_PLATFORM_REVIEWER_ID}" \
  --request-log /restricted/provider/request-log.complete.json \
  --final-statement /restricted/provider/statement.final.json
env -u ARK_API_KEY uv run python scripts/reconcile_intelligence_provider_costs.py scan \
  --batch-size 25 --max-batches 40
```

The runbook states: exit 66 means a file is absent; exit 64 means unsafe arguments; exit 75 means work remains and the VKE Job is retried; any validation/`Conflict` exit keeps the task and hold unchanged. The Job has no Ark API key, has read access only to the restricted import mount, private TOS write access only under `provider-evidence/`, database rights only for the application schema, one active instance by deployment policy, and a 60-second termination grace period. PostgreSQL `SKIP LOCKED` remains the concurrency safety mechanism if two instances overlap during rollout.

- [ ] **Step 5: Run the CLI tests and a zero-work database scan**

Run:

```bash
cd backend
env -u ARK_API_KEY uv run pytest tests/unit/intelligence/test_reconciliation_cli.py tests/integration/intelligence/test_provider_reconciliation_scanner.py -q
env -u ARK_API_KEY uv run python scripts/reconcile_intelligence_provider_costs.py scan --batch-size 1 --max-batches 1
```

Expected: tests pass with no external call; against an empty local queue the command prints `selected=0 finalized=0` and exits 0.

- [ ] **Step 6: Commit the independent process and runbook**

```bash
git add .env.example backend/src/ip_saas/config.py backend/src/ip_saas/modules/intelligence/reconciliation_composition.py backend/scripts/reconcile_intelligence_provider_costs.py backend/tests/unit/intelligence/test_reconciliation_cli.py docs/runbooks/intelligence-provider-reconciliation.md
git commit -m "feat: add independent provider reconciliation scanner"
```

### Task 7: Put the durable boundary in front of both Ark adapters

**Files:**
- Modify: `backend/src/ip_saas/modules/intelligence/provider_journal.py`
- Modify: `backend/src/ip_saas/modules/intelligence/providers/ark_responses.py`
- Modify: `backend/src/ip_saas/modules/intelligence/providers/ark_web_search.py`
- Modify: `backend/src/ip_saas/modules/intelligence/worker.py`
- Create: `backend/tests/unit/intelligence/test_worker_provider_boundary.py`
- Modify: `backend/tests/unit/intelligence/test_ark_responses.py`
- Modify: `backend/tests/unit/intelligence/test_ark_web_search.py`
- Modify: `backend/tests/unit/intelligence/test_worker_leases.py`

- [ ] **Step 1: Write failing no-resend, exact-ID, and crash-window tests**

Create `test_worker_provider_boundary.py` with an injected synchronous session-scope factory, real Task 3 journal, and a `Mock` sender. Add these named cases:

```python
from hashlib import sha256
from unittest.mock import Mock

import pytest

from ip_saas.common.errors import Conflict
from ip_saas.modules.intelligence.provider_journal import (
    DurableProviderCallBoundary,
    ProviderReconciliationPending,
    ProviderResponseEnvelope,
)


class SimulatedPostPaidSigkill(BaseException):
    pass


def test_paid_response_crash_before_response_persist_is_never_resent(boundary_fixture) -> None:
    boundary, call, sender = boundary_fixture
    sender.return_value = ProviderResponseEnvelope(
        provider_request_id="ark-paid-1",
        raw_body=b'{"id":"ark-paid-1","usage":{"total_tokens":1}}',
        value={"answer": "recorded only after the crash point"},
    )

    def kill_after_paid_response() -> None:
        raise SimulatedPostPaidSigkill

    with pytest.raises(SimulatedPostPaidSigkill):
        boundary.execute(call, sender=sender, after_paid_response=kill_after_paid_response)
    with pytest.raises(ProviderReconciliationPending):
        boundary.execute(call, sender=sender)
    sender.assert_called_once()


def test_missing_padded_or_oversize_completion_id_leaves_sent_call_for_reconciliation(
    boundary_fixture,
) -> None:
    boundary, call, sender = boundary_fixture
    sender.return_value = ProviderResponseEnvelope(
        provider_request_id=" padded ",
        raw_body=b"{}",
        value={},
    )
    with pytest.raises(Conflict, match="trimmed"):
        boundary.execute(call, sender=sender)
    with pytest.raises(ProviderReconciliationPending):
        boundary.execute(call, sender=sender)
    sender.assert_called_once()


def test_response_hash_and_provider_identity_are_persisted_exactly(boundary_fixture) -> None:
    boundary, call, sender = boundary_fixture
    raw = b'{"id":"ark-2","model":"doubao-seed-2-0-lite-260215"}'
    sender.return_value = ProviderResponseEnvelope(
        provider_request_id="ark-2",
        raw_body=raw,
        value={"answer": "ok"},
    )
    assert boundary.execute(call, sender=sender) == {"answer": "ok"}
    attempt = boundary_fixture.persisted_attempt()
    assert attempt.provider_request_id == "ark-2"
    assert attempt.response_sha256 == sha256(raw).hexdigest()
```

Define `boundary_fixture` in this same test module with `@pytest.fixture`; it explicitly creates `ProviderCall` from `running_intelligence_task`, supplies a `session_scope_factory` that commits on normal exit and rolls back on exceptions, and exposes a second-session `persisted_attempt()` reader. Do not add an undeclared fixture argument.

Append adapter assertions to the two existing adapter test files:

- The outbound request to the controlled APIG egress origin contains `X-IP-SaaS-Logical-Call-ID` equal to the stable `client_attempt_id`. APIG records and strips this header; Ark never interprets it as idempotency.
- The response body `id` is passed byte-for-byte through `normalize_provider_request_id`, which delegates validity to Plan 01's sole canonical identity guard and then adds the stricter no-padding rule. Null, non-string, padded, empty, case-insensitive reserved `None`, or longer-than-200 IDs raise `Conflict` after the call and never synthesize an ID from a timestamp, task ID, body hash, or APIG request ID.
- `StructuredCompletion.provider_request_id` and `ProviderCostInput.provider_request_id` are identical; any mismatch raises `Conflict` before settlement.
- Both adapters preserve the exact task-pinned model ID/version and the exact current `TaskRecord.id` in the cost input.

Append Worker tests proving `ProviderReconciliationPending` returns `WorkerDisposition.RETRY` without calling `settle`, `settle_failure`, `release_generation`, `fail`, or a provider sender. After `TaskSubmissionService.start(..., max_attempts=8)` returns `reconciliation_required`, the Worker also returns `RETRY` without executing application code; RocketMQ leaves the delivery unacknowledged while the independent scanner finalizes it.

- [ ] **Step 2: Run the boundary suite and verify old direct-call behavior fails**

Run:

```bash
cd backend
env -u ARK_API_KEY uv run pytest \
  tests/unit/intelligence/test_worker_provider_boundary.py \
  tests/unit/intelligence/test_ark_responses.py \
  tests/unit/intelligence/test_ark_web_search.py \
  tests/unit/intelligence/test_worker_leases.py -q
```

Expected: tests fail because the adapters can still send without a committed journal marker and the Worker has no no-resend exception path; no external connection occurs.

- [ ] **Step 3: Add the transaction-separated durable call boundary**

Append these public types and service to `provider_journal.py`:

```python
from contextlib import AbstractContextManager
from dataclasses import dataclass
from hashlib import sha256
from typing import Callable, Generic, TypeVar


ResultT = TypeVar("ResultT")
SessionScopeFactory = Callable[[], AbstractContextManager[Session]]


class ProviderReconciliationPending(Conflict):
    pass


@dataclass(frozen=True)
class ProviderCall:
    task_id: UUID
    task_attempt_no: int
    adapter: str
    operation: str
    logical_call_id: str
    provider_account_fingerprint: str
    request_payload: Mapping[str, JSONValue]
    model_id: str
    model_version: str


@dataclass(frozen=True)
class ProviderResponseEnvelope(Generic[ResultT]):
    provider_request_id: str
    raw_body: bytes
    value: ResultT


class DurableProviderCallBoundary:
    def __init__(
        self,
        *,
        journal: ProviderAttemptJournal,
        session_scope_factory: SessionScopeFactory,
    ) -> None:
        self._journal = journal
        self._session_scope_factory = session_scope_factory

    def execute(
        self,
        call: ProviderCall,
        *,
        sender: Callable[[UUID], ProviderResponseEnvelope[ResultT]],
        after_paid_response: Callable[[], None] = lambda: None,
    ) -> ResultT:
        with self._session_scope_factory() as session:
            attempt = self._journal.prepare(
                session,
                task_id=call.task_id,
                attempt_no=call.task_attempt_no,
                adapter=call.adapter,
                operation=call.operation,
                logical_call_id=call.logical_call_id,
                provider_account_fingerprint=call.provider_account_fingerprint,
                request_payload=call.request_payload,
                model_id=call.model_id,
                model_version=call.model_version,
            )
            attempt_id = attempt.id
            client_attempt_id = attempt.client_attempt_id
            status = ProviderAttemptStatus(attempt.status)
        if status != ProviderAttemptStatus.PREPARED:
            raise ProviderReconciliationPending(
                f"logical provider call {attempt_id} already crossed the send boundary"
            )
        try:
            with self._session_scope_factory() as session:
                self._journal.mark_send_started(
                    session,
                    attempt_id,
                    task_attempt_no=call.task_attempt_no,
                )
        except Conflict as error:
            raise ProviderReconciliationPending(str(error)) from error
        try:
            response = sender(client_attempt_id)
            request_id = normalize_provider_request_id(response.provider_request_id)
        except Exception as error:
            raise ProviderReconciliationPending(
                f"provider send outcome requires supplier reconciliation: {type(error).__name__}"
            ) from error
        after_paid_response()
        try:
            with self._session_scope_factory() as session:
                self._journal.record_response(
                    session,
                    attempt_id,
                    task_attempt_no=call.task_attempt_no,
                    provider_request_id=request_id,
                    response_sha256=sha256(response.raw_body).hexdigest(),
                )
        except Conflict as error:
            raise ProviderReconciliationPending(str(error)) from error
        return response.value
```

`prepare` commits before `mark_send_started`; `mark_send_started` commits before `sender`; `record_response` commits after the response. `BaseException` still escapes for real process termination. Every ordinary sender, parse, response-identity, concurrent-marker, or post-send lease exception is converted to `ProviderReconciliationPending`, so it bypasses ordinary failure settlement while preserving the durable send marker. The next delivery sees that marker and cannot resend. The scanner later resolves the marker from a complete supplier export as either `supplier_matched` or `supplier_no_call`.

- [ ] **Step 4: Route both adapters through the boundary and controlled egress**

Change the two production adapters so their HTTP method is called only inside a `sender(client_attempt_id)` closure passed to `DurableProviderCallBoundary.execute`. Build the controlled-gateway request as follows in both adapters:

```python
def provider_egress_headers(
    *,
    api_key: str,
    client_attempt_id: UUID,
) -> dict[str, str]:
    return {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json",
        "X-IP-SaaS-Logical-Call-ID": str(client_attempt_id),
    }
```

The target origin comes only from the reviewed Plan 02a runtime and is an APIG/VKE egress route. Its policy logs the stable header, removes it before forwarding upstream, denies every destination except the exact Ark endpoints, and captures the official gateway request ID plus the provider response identity needed for the supplier-log join. Direct `ark.cn-beijing.volces.com` egress from the Worker namespace is denied. This header is correlation metadata, not a retry or idempotency promise; the no-resend rule comes from the committed send marker.

For Responses, pass one `ProviderCall` per `StructuredCall.idempotency_key`. For Web Search, use one call per canonical ordered query batch. The operation and request payload passed to the boundary must be the exact payload sent after removing only the gateway correlation header. Timeouts remain below the task lease and a heartbeat occurs before `prepare`.

After reading response bytes, decode JSON once, require the exact registry model, validate the response ID with `normalize_provider_request_id`, and construct `ProviderResponseEnvelope`. Inject the Plan 02 crash harness callback immediately before `record_response`; ordinary composition supplies the default no-op.

- [ ] **Step 5: Make unresolved sends bypass every ordinary failure settlement path**

In `IntelligenceWorker.process`, add this exception branch before the existing domain/provider failure branch:

```diff
+        except ProviderReconciliationPending:
+            return WorkerDisposition.RETRY
```

At the start of the claimed-task branch, keep this terminal-state decision:

```python
        if TaskStatus(snapshot.status) == TaskStatus.RECONCILIATION_REQUIRED:
            return WorkerDisposition.RETRY
```

The RocketMQ consumer leaves the exhausted ordinary delivery unacknowledged while the independent scanner owns recovery. It does not emit a new provider task and does not call the provider. `reconciliation_required` and active or same-attempt unresolved work return `RETRY`; after the scanner atomically moves the task to terminal `failed`, the next broker delivery follows Plan 01's terminal replay path and ACKs. Before exhaustion, lease reclaim may advance the task attempt, but `DurableProviderCallBoundary` still finds the stable sent logical call and blocks resending it.

- [ ] **Step 6: Run zero-network tests and commit**

Run:

```bash
cd backend
env -u ARK_API_KEY uv run pytest \
  tests/unit/intelligence/test_worker_provider_boundary.py \
  tests/unit/intelligence/test_ark_responses.py \
  tests/unit/intelligence/test_ark_web_search.py \
  tests/unit/intelligence/test_worker_leases.py \
  tests/integration/intelligence/test_provider_attempt_journal.py -q
```

Expected: all tests pass; the post-paid exception leaves one send marker and zero response/cost rows; retry makes zero additional HTTP calls; every accepted completion identity is trimmed, 1 through 200 characters, and equal in completion, journal, and cost input.

```bash
git add backend/src/ip_saas/modules/intelligence/provider_journal.py backend/src/ip_saas/modules/intelligence/providers/ark_responses.py backend/src/ip_saas/modules/intelligence/providers/ark_web_search.py backend/src/ip_saas/modules/intelligence/worker.py backend/tests/unit/intelligence/test_worker_provider_boundary.py backend/tests/unit/intelligence/test_ark_responses.py backend/tests/unit/intelligence/test_ark_web_search.py backend/tests/unit/intelligence/test_worker_leases.py
git commit -m "feat: fence Ark calls behind durable send markers"
```

### Task 8: Replace the unavailable runtime only behind two independent attestations

**Files:**
- Modify: `backend/src/ip_saas/modules/intelligence/provider_release.py`
- Modify: `backend/src/ip_saas/modules/intelligence/worker.py`
- Modify: `backend/src/ip_saas/modules/intelligence/reconciliation_composition.py`
- Modify: `backend/src/ip_saas/config.py`
- Modify: `.env.example`
- Create: `backend/tests/unit/intelligence/test_provider_release_runtime.py`

- [ ] **Step 1: Write failing dual-gate and pre-secret tests**

Create `test_provider_release_runtime.py` using temporary JSON files and mocks. Cover this full matrix:

```python
from pathlib import Path
from unittest.mock import Mock

import pytest

from ip_saas.common.errors import Forbidden
from ip_saas.modules.intelligence import worker as worker_module


@pytest.mark.parametrize(
    ("runtime_present", "crash_poc_present"),
    [(False, False), (True, False), (False, True)],
)
def test_either_missing_gate_denies_before_secret_or_http(
    runtime_present: bool,
    crash_poc_present: bool,
    valid_runtime_evidence: tuple[Path, str],
    valid_crash_poc_evidence: tuple[Path, str],
) -> None:
    secret = Mock()
    http = Mock()
    runtime_path, runtime_sha = valid_runtime_evidence
    crash_path, crash_sha = valid_crash_poc_evidence
    with pytest.raises(Forbidden):
        worker_module.compose_production_intelligence_runtime_factory(
            runtime_evidence_path=runtime_path if runtime_present else None,
            runtime_evidence_sha256=runtime_sha if runtime_present else "",
            evidence_path=crash_path if crash_poc_present else None,
            evidence_sha256=crash_sha if crash_poc_present else "",
            read_ark_api_key=secret,
            build_http_client=http,
            clock=Mock(),
            private_object_store=None,
        )
    secret.assert_not_called()
    http.assert_not_called()
```

Define both named fixtures in the same file. `valid_runtime_evidence` writes the strict model below with both adapters, `migration_revision="0002_intelligence"`, `scanner_ready=true`, `direct_ark_egress_denied=true`, `egress_base_url="https://ark-egress.test.example.com"`, two non-empty route IDs, `passed=true`, and real 64-character test digests. `valid_crash_poc_evidence` uses Plan 02's existing strict six-cell fixture. Add mutations for missing adapter, wrong migration, stale scanner health, altered file hash, non-final supplier capability, direct egress allowed, direct Ark origin, missing route, and `passed=false`; all deny before the secret mock.

- [ ] **Step 2: Run the release tests and verify production still always uses unavailable**

Run: `cd backend && env -u ARK_API_KEY uv run pytest tests/unit/intelligence/test_provider_release_runtime.py -q`

Expected: tests fail because Plan 02 production composition still constructs `UnavailableDurableProviderReconciliation` unconditionally.

- [ ] **Step 3: Implement the strict hash-pinned runtime evidence**

Append this model and runtime to `provider_release.py`:

```python
from datetime import datetime, timedelta
from dataclasses import dataclass
from pathlib import Path
from typing import Literal
from urllib.parse import urlparse

from pydantic import BaseModel, ConfigDict, Field, model_validator


@dataclass(frozen=True)
class ProviderEgressBinding:
    adapter: AdapterKind
    base_url: str
    route_id: str
    provider_account_fingerprint: str


class Plan02aRuntimeEvidence(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)

    schema_version: Literal[1]
    provider: Literal["volcengine"]
    provider_account_fingerprint: str = Field(pattern=r"^[0-9a-f]{64}$")
    adapters: tuple[AdapterKind, ...]
    migration_revision: Literal["0002_intelligence"]
    journal_schema_sha256: str = Field(pattern=r"^[0-9a-f]{64}$")
    scanner_image_digest: str = Field(pattern=r"^sha256:[0-9a-f]{64}$")
    scanner_health_at: datetime
    scanner_ready: Literal[True]
    official_request_log_contract_sha256: str = Field(pattern=r"^[0-9a-f]{64}$")
    finalized_statement_contract_sha256: str = Field(pattern=r"^[0-9a-f]{64}$")
    apig_access_log_contract_sha256: str = Field(pattern=r"^[0-9a-f]{64}$")
    egress_base_url: str
    responses_route_id: str
    web_search_route_id: str
    direct_ark_egress_denied: Literal[True]
    stable_header_logged_and_stripped: Literal[True]
    supplier_identity_join_proven: Literal[True]
    passed: Literal[True]

    @model_validator(mode="after")
    def exact_adapters(self) -> "Plan02aRuntimeEvidence":
        if set(self.adapters) != {AdapterKind.RESPONSES, AdapterKind.WEB_SEARCH} or len(self.adapters) != 2:
            raise ValueError("runtime evidence must cover both adapters exactly once")
        parsed = urlparse(self.egress_base_url)
        if parsed.scheme != "https" or not parsed.hostname or parsed.hostname.endswith("ark.cn-beijing.volces.com"):
            raise ValueError("runtime evidence must name the controlled HTTPS egress origin")
        if not self.responses_route_id.strip() or not self.web_search_route_id.strip():
            raise ValueError("both controlled egress route IDs are required")
        return self


class DeployedDurableProviderReconciliation:
    def __init__(self, evidence: Plan02aRuntimeEvidence, *, now: datetime) -> None:
        self._evidence = evidence
        self._now = now

    def require_ready(self, adapters: tuple[AdapterKind, ...]) -> None:
        if set(adapters) != set(self._evidence.adapters):
            raise Forbidden("Plan 02a runtime adapter coverage differs")
        if self._evidence.scanner_health_at - self._now > timedelta(minutes=5):
            raise Forbidden("Plan 02a scanner health evidence is future-dated")
        if self._now - self._evidence.scanner_health_at > timedelta(hours=24):
            raise Forbidden("Plan 02a scanner health evidence is stale")

    def require_binding(self, adapter: AdapterKind) -> ProviderEgressBinding:
        self.require_ready((AdapterKind.RESPONSES, AdapterKind.WEB_SEARCH))
        route_id = (
            self._evidence.responses_route_id
            if adapter == AdapterKind.RESPONSES
            else self._evidence.web_search_route_id
        )
        return ProviderEgressBinding(
            adapter=adapter,
            base_url=self._evidence.egress_base_url,
            route_id=route_id,
            provider_account_fingerprint=self._evidence.provider_account_fingerprint,
        )


def build_durable_provider_reconciliation_runtime(
    *,
    evidence_path: Path | None,
    expected_sha256: str,
    now: datetime,
) -> DurableProviderReconciliationRuntime:
    if evidence_path is None or not expected_sha256:
        return UnavailableDurableProviderReconciliation()
    raw = evidence_path.read_bytes()
    if sha256(raw).hexdigest() != expected_sha256:
        raise Forbidden("Plan 02a runtime evidence digest mismatch")
    try:
        evidence = Plan02aRuntimeEvidence.model_validate_json(raw)
    except Exception as error:
        raise Forbidden("Plan 02a runtime evidence is invalid") from error
    return DeployedDurableProviderReconciliation(evidence, now=now)
```

Reject future-dated `scanner_health_at` by more than five minutes in `require_ready`. The runtime evidence is a reviewed, append-only artifact copied from restricted TOS to the deployment secret volume. `ark_plan02a_runtime_evidence_sha256` is also the exact pin passed to `JournalNoProviderCallEvidencePort`; changing the runtime or egress policy invalidates both production startup and every new zero-call proof.

- [ ] **Step 4: Change only the controlled production binding**

Add to `Settings` and `.env.example`:

```python
    ark_plan02a_runtime_evidence_path: Path | None = None
```

```bash
ARK_PLAN02A_RUNTIME_EVIDENCE_PATH=
```

Replace only the hardcoded line inside `compose_production_intelligence_runtime_factory`:

```python
    reconciliation_runtime = build_durable_provider_reconciliation_runtime(
        evidence_path=runtime_evidence_path,
        expected_sha256=runtime_evidence_sha256,
        now=clock.now(),
    )
```

Add `runtime_evidence_path: Path | None` and `runtime_evidence_sha256: str` to that function's keyword-only signature. Extend the additive Plan 02 protocol with `require_binding(adapter: AdapterKind) -> ProviderEgressBinding`; `UnavailableDurableProviderReconciliation.require_binding` raises the same fail-closed `Forbidden`. Preserve `_build_verified_production_intelligence_runtime_factory` call order: first `reconciliation_runtime.require_ready(...)`, then obtain both exact bindings through `require_binding`, then `release_gate.require_verified()`, then `read_ark_api_key()`, then `build_http_client()`, then `_VerifiedProductionIntelligenceRuntimeFactory(...)`. Pass the two bindings into that factory; it supplies the controlled base URL, route ID, and provider-account fingerprint to every Task 7 `ProviderCall`. There is still exactly one constructor call for the verified factory.

Production settings pass both independent pairs:

```python
runtime_evidence_path=settings.ark_plan02a_runtime_evidence_path,
runtime_evidence_sha256=settings.ark_plan02a_runtime_evidence_sha256,
evidence_path=settings.ark_provider_crash_evidence_path,
evidence_sha256=settings.ark_provider_crash_evidence_sha256,
```

The first pair proves the deployed journal, scanner, official-log contract, and network enforcement. The second pair is Plan 02's production-account six-cell crash result. Neither artifact can substitute for the other.

Modify `build_provider_reconciliation_runtime()` in `reconciliation_composition.py` to load the same path/hash with `build_durable_provider_reconciliation_runtime(...)`, call `require_ready((RESPONSES, WEB_SEARCH))`, and require both bindings before constructing the two evidence ports. The scanner process has no Ark credential and never uses the URLs, but this shared validation prevents a syntactically shaped hash from authorizing zero-call releases.

- [ ] **Step 5: Prove valid dual evidence reaches secrets only after both checks**

Add one positive test with both valid fixtures. Assert the secret loader is called once, the HTTP builder once, and the verified factory once. Add an AST assertion that the unique `_VerifiedProductionIntelligenceRuntimeFactory` call remains inside `_build_verified_production_intelligence_runtime_factory`, and that the controlled composition contains one call to `build_durable_provider_reconciliation_runtime` and no direct construction of `DeployedDurableProviderReconciliation`.

Run: `cd backend && env -u ARK_API_KEY uv run pytest tests/unit/intelligence/test_provider_release_runtime.py tests/unit/intelligence/test_provider_release.py -q`

Expected: all tests pass; every missing, stale, altered, partial, or wrong-account artifact denies before credentials or HTTP; two valid independently pinned files allow composition.

- [ ] **Step 6: Commit the dual production gate**

```bash
git add .env.example backend/src/ip_saas/config.py backend/src/ip_saas/modules/intelligence/provider_release.py backend/src/ip_saas/modules/intelligence/reconciliation_composition.py backend/src/ip_saas/modules/intelligence/worker.py backend/tests/unit/intelligence/test_provider_release_runtime.py
git commit -m "feat: require dual attestations for Ark production"
```

### Task 9: Exercise the real post-paid/pre-persist crash under a hard budget

**Files:**
- Modify: `backend/scripts/run_intelligence_provider_crash_poc.py`
- Modify: `docs/runbooks/intelligence-provider-crash-poc.md`
- Create: `backend/tests/integration/intelligence/test_provider_crash_recovery.py`
- Modify: `backend/tests/unit/intelligence/test_provider_release_runtime.py`

- [ ] **Step 1: Write a failing local SIGKILL recovery test**

Create `test_provider_crash_recovery.py`. It uses PostgreSQL and a local child process, but a deterministic fake egress server and zero provider credentials. The child receives a real task ID, executes the Task 7 boundary, writes the fake paid response into a pipe, and is killed by `os.kill(os.getpid(), signal.SIGKILL)` from the exact `after_paid_response` callback. The parent then asserts:

```python
def test_sigkill_after_paid_response_before_persist_never_resends_and_batch_recovers(
    pg_engine,
    running_intelligence_task,
) -> None:
    result = run_local_sigkill_case(
        pg_engine=pg_engine,
        task_id=running_intelligence_task.id,
        adapter="responses",
        provider_request_id="fake-paid-response-1",
    )
    assert result.child_returncode == -signal.SIGKILL
    assert result.sender_call_count == 1
    assert result.attempt_status_before_import == "send_started"
    assert result.provider_request_id_before_import is None
    assert result.cost_rows_before_import == 0
    assert result.retry_sender_call_count == 0
    assert result.task_status_after_scan == "failed"
    assert result.task_linked_cost_request_ids == ("fake-paid-response-1",)
    assert result.finalizer_calls == 1
```

Define `run_local_sigkill_case` and its frozen result dataclass in the same test module. It imports a complete fake official request log and final statement through `SupplierEvidenceIngestor`, exhausts the task into `reconciliation_required`, and runs the real scanner once. Add the same case for `web_search`, plus a pre-HTTP SIGKILL case whose complete empty official exports produce `supplier_no_call`, release the hold through `finalize_no_provider_call`, and create no cost row. The local fake is explicitly marked `environment="deterministic_test"` and cannot generate either production release artifact.

- [ ] **Step 2: Run the local crash test and verify the old harness cannot recover**

Run: `cd backend && env -u ARK_API_KEY uv run pytest tests/integration/intelligence/test_provider_crash_recovery.py -q`

Expected: FAIL because the existing Plan 02 harness has no committed Task 7 send marker, official-evidence importer, or scanner recovery hook; no external network call occurs.

- [ ] **Step 3: Extend the production runner with strict preflight and fault checkpoints**

Retain Plan 02's strict `ProviderCrashEvidence` schema and six-cell adapter/fault grid. Extend `run_intelligence_provider_crash_poc.py` with these commands:

```text
preflight-runtime  validate migration inventory, dual release gates, dedicated account, APIG log join, egress deny, real internal limits, and both hard caps; never read ARK_API_KEY
exercise           fork one child per fault cell, call the exact production adapter, SIGKILL the child, and leave raw restricted evidence
import             call the same SupplierEvidenceIngestor used by the independent scanner
scan               call the same ProviderReconciliationScanner in separate transactions
verify             compare journal, official request log, final statement, ProviderCostEntry, hold, task, APIG log, and budget proof; emit strict evidence
```

Use these exact fault points for both `responses` and `web_search`:

```python
class FaultPoint(StrEnum):
    AFTER_SEND_MARKER_BEFORE_HTTP = "after_send_marker_before_http"
    AFTER_PAID_RESPONSE_BEFORE_RESPONSE_PERSIST = "after_paid_response_before_response_persist"
    AFTER_COST_FLUSH_BEFORE_TASK_TERMINAL_COMMIT = "after_cost_flush_before_task_terminal_commit"
```

The second point calls `os.kill(os.getpid(), signal.SIGKILL)` from `DurableProviderCallBoundary.after_paid_response` after complete response bytes are present and before `record_response` opens its transaction. The parent must observe `send_started`, `provider_request_id is None`, and zero `ProviderCostEntry` rows before importing supplier evidence. On every restart, the parent executes the normal Worker delivery and records that the provider sender call count remains unchanged.

The first point kills after the committed send marker but before `httpx.Client.send`; its complete official APIG/request export and final statement contain zero matching requests, ingestion marks the attempt `supplier_no_call`, and the no-call proof includes both supplier artifact hashes plus the runtime/egress attestation hash. The third point kills after `ProviderCostEntry` has been flushed inside the uncommitted terminal transaction; PostgreSQL rollback removes that row, supplier import reconstructs the same exact request cost once, and batch finalization creates it once.

- [ ] **Step 4: Enforce two independent hard budgets before credential access**

The preflight reads these non-secret artifacts before `ARK_API_KEY`:

- A provider-console/API export for a dedicated Ark sub-account showing an enabled hard cap of at most 100 fen, its account fingerprint, effective time, and SHA-256. A warning threshold is not a hard cap and fails preflight.
- A real Plan 05 `GenerationLimitVersion` for the platform internal cost center with per-task, daily, and monthly remaining capacity each at most 100 fen. The production `GenerationLimitService` resolves it; no fake or unconfigured adapter is accepted.
- A run-local `max_total_fen=100` and `max_billable_completions=6`. Before each potentially billable request, the runner recomputes supplier spend from finalized statement lines plus the maximum next-call price and refuses if the sum could exceed 100 fen.
- The dedicated sub-account has no non-PoC traffic during the evidence window. The official complete log must therefore equal the journal-resolved call set; unrelated extra traffic fails verification.

Only after all four checks, the Plan 02a runtime attestation, and the existing Plan 02 crash-evidence gate preflight pass may the runner read `ARK_API_KEY` and build `httpx.Client`. For initial evidence generation, the crash-evidence gate is in a signed `exercise_authorization` mode that names the exact run ID and budget; it cannot enable application traffic. Normal production composition still requires the final `passed=true` crash artifact.

- [ ] **Step 5: Document and run the restricted production exercise**

Replace the corresponding runbook section with these commands:

```bash
cd backend
test "${IP_SAAS_ARK_CRASH_POC_ACK:?}" = "PRODUCTION_ACCOUNT_MAX_100_FEN"
env -u ARK_API_KEY uv run python scripts/run_intelligence_provider_crash_poc.py preflight-runtime \
  --deployment-inventory .artifacts/deployment-revisions.json \
  --runtime-evidence .artifacts/plan02a/runtime-evidence.json \
  --runtime-evidence-sha256 "${ARK_PLAN02A_RUNTIME_EVIDENCE_SHA256}" \
  --exercise-authorization .artifacts/plan02a/exercise-authorization.json \
  --provider-budget-proof .artifacts/plan02a/provider-hard-cap.json \
  --internal-cost-center-id "${IP_SAAS_ARK_POC_COST_CENTER_ID}" \
  --max-total-fen 100
test -n "${ARK_API_KEY:?}"
uv run python scripts/run_intelligence_provider_crash_poc.py exercise \
  --run-id "${IP_SAAS_ARK_POC_RUN_ID}" \
  --adapters responses,web_search \
  --fault-points after_send_marker_before_http,after_paid_response_before_response_persist,after_cost_flush_before_task_terminal_commit \
  --max-billable-completions 6 \
  --max-total-fen 100 \
  --evidence-dir .artifacts/plan02a/restricted
```

Expected: `preflight-runtime` exits 0 without reading a credential only when migration mode, runtime deployment, official identity join, egress deny, real internal limits, exercise authorization, and both caps pass. `exercise` forks six children; every intended crash returns `-SIGKILL`; no restart sends the same stable logical call again; measured supplier spend remains at or below 100 fen. Exit 78 means an invariant failed before or during provider use and leaves normal production disabled. Exit 75 means official exports are not yet complete/final and requires later evidence collection, not a retry of provider calls.

After the provider console/API marks the billing statement final, place the official complete APIG/Ark request log and finalized supplier statement in the restricted directory, then run:

```bash
cd backend
env -u ARK_API_KEY uv run python scripts/run_intelligence_provider_crash_poc.py import \
  --run-id "${IP_SAAS_ARK_POC_RUN_ID}" \
  --request-log .artifacts/plan02a/restricted/provider-request-log.complete.json \
  --final-statement .artifacts/plan02a/restricted/supplier-statement.final.json
env -u ARK_API_KEY uv run python scripts/run_intelligence_provider_crash_poc.py scan \
  --run-id "${IP_SAAS_ARK_POC_RUN_ID}" --batch-size 6
env -u ARK_API_KEY uv run python scripts/run_intelligence_provider_crash_poc.py verify \
  --run-id "${IP_SAAS_ARK_POC_RUN_ID}" \
  --evidence-dir .artifacts/plan02a/restricted \
  --output .artifacts/plan02a/provider-crash-evidence.json
sha256sum .artifacts/plan02a/provider-crash-evidence.json
```

Expected: `verify` exits 0 only when each complete official request maps to exactly one stable journal identity, one exact task ID, one 1–200-character provider request ID, and one cost row; every no-request send marker has complete/final zero-call coverage; customer actual usage is zero on failed work; internal actual amount equals the sum of every statement `amount_fen`; the same request replay is idempotent; all six cells pass; the two budget proofs and both evidence digests match. Missing, partial, duplicated, extra, cross-account, cross-model, cross-task, stale-attempt, or `final=false` inputs exit 78 and emit no release artifact.

- [ ] **Step 6: Run the local SIGKILL regression and strict evidence parser**

Run:

```bash
cd backend
env -u ARK_API_KEY uv run pytest \
  tests/integration/intelligence/test_provider_crash_recovery.py \
  tests/integration/intelligence/test_supplier_evidence_ingestion.py \
  tests/integration/intelligence/test_provider_reconciliation_scanner.py \
  tests/unit/intelligence/test_provider_release_runtime.py -q
```

Expected: all tests pass with zero network and zero paid calls; the local child really exits by SIGKILL and recovery writes exactly one task-linked cost.

- [ ] **Step 7: Commit the reproducible crash proof**

```bash
git add backend/scripts/run_intelligence_provider_crash_poc.py backend/tests/integration/intelligence/test_provider_crash_recovery.py backend/tests/unit/intelligence/test_provider_release_runtime.py docs/runbooks/intelligence-provider-crash-poc.md
git commit -m "test: prove supplier-first recovery after provider crash"
```

### Task 10: Freeze source contracts and run the complete acceptance gate

**Files:**
- Create: `backend/tests/contract/intelligence/test_plan02a_source_contract.py`
- Modify: `docs/runbooks/intelligence-provider-reconciliation.md`

- [ ] **Step 1: Write the failing source-contract test**

Create the contract test with repository-relative paths and these assertions:

```python
# backend/tests/contract/intelligence/test_plan02a_source_contract.py
import ast
from pathlib import Path


BACKEND = Path(__file__).resolve().parents[3]


def parsed(relative: str) -> ast.Module:
    return ast.parse((BACKEND / relative).read_text(encoding="utf-8"))


def calls_named(tree: ast.AST, name: str) -> list[ast.Call]:
    return [
        node
        for node in ast.walk(tree)
        if isinstance(node, ast.Call)
        and (
            isinstance(node.func, ast.Name) and node.func.id == name
            or isinstance(node.func, ast.Attribute) and node.func.attr == name
        )
    ]


def test_scanner_uses_only_the_once_per_task_plan01_finalizer() -> None:
    tree = parsed("src/ip_saas/modules/intelligence/provider_reconciliation.py")
    assert len(calls_named(tree, "finalize_provider_costs")) == 1
    assert len(calls_named(tree, "finalize_no_provider_call")) == 1
    assert calls_named(tree, "settle_generation") == []
    assert calls_named(tree, "settle_generation_batch") == []
    assert calls_named(tree, "release_generation") == []


def test_production_composition_builds_runtime_before_secret_and_http() -> None:
    tree = parsed("src/ip_saas/modules/intelligence/worker.py")
    compose = next(
        node
        for node in tree.body
        if isinstance(node, ast.FunctionDef)
        and node.name == "compose_production_intelligence_runtime_factory"
    )
    ordered = [
        node.func.id
        for node in ast.walk(compose)
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
    ]
    assert ordered.count("build_durable_provider_reconciliation_runtime") == 1
    source = ast.unparse(compose)
    assert source.index("build_durable_provider_reconciliation_runtime") < source.index("read_ark_api_key")
    assert source.index("read_ark_api_key") < source.index("build_http_client")


def test_request_identity_and_migration_chain_remain_single_truths() -> None:
    attempt_source = (BACKEND / "src/ip_saas/modules/intelligence/provider_attempts.py").read_text()
    migration_source = (BACKEND / "migrations/versions/0002_intelligence.py").read_text()
    assert "String(200)" in attempt_source
    assert 'revision: str = "0002_intelligence"' in migration_source
    assert 'down_revision: str | None = "0001_foundation"' in migration_source
    assert "0002a" not in migration_source


def test_reconciliation_composition_has_real_mandatory_limits() -> None:
    source = (BACKEND / "src/ip_saas/modules/intelligence/reconciliation_composition.py").read_text()
    assert "build_billing_service(clock)" in source
    assert "AllowAllGenerationLimits" not in source
    assert "UnconfiguredGenerationLimits" not in source
```

Add an AST assertion that `provider_journal.py` contains exactly one call each to `prepare`, `mark_send_started`, and `record_response` inside `DurableProviderCallBoundary.execute`, with `sender` textually between the committed marker scope and response-recording scope. Add a filesystem assertion that this addendum creates no JSON event contract and that `frontend/src/lib/api/schema.d.ts` remains the sole frontend OpenAPI type file; Plan 02a creates no frontend client or `/api/v1` path.

- [ ] **Step 2: Run the contract test and verify it catches incomplete wiring**

Run: `cd backend && uv run pytest tests/contract/intelligence/test_plan02a_source_contract.py -q`

Expected: FAIL until all Tasks 1–9 have the exact model, scanner, production composition, and migration wiring.

- [ ] **Step 3: Add the final operator decision table to the runbook**

Append this table to `docs/runbooks/intelligence-provider-reconciliation.md`:

| Observed state | Permitted action | Forbidden action |
|---|---|---|
| `prepared`, no send marker | Worker may mark and send while its lease is current | scanner finalization |
| `send_started` or `response_observed` | wait for complete official evidence; retry returns without send | resend, release, ordinary fail |
| `supplier_no_call` on every sent attempt | scanner calls `finalize_no_provider_call` once | database-only release |
| one or more `supplier_matched`, all other sends `supplier_no_call` | scanner passes the full matched tuple once | per-row settlement |
| missing/partial/non-final/conflicting evidence | keep `reconciliation_required`, return `RETRY`, and keep the hold active | treat delivery as terminal, fabricate zero cost |
| task already terminal with same reconciliation fingerprint | ACK exact replay | changed replay |

Also document restore behavior: database restore and TOS evidence restore must target the same recovery point; after restore, run a read-only manifest audit before starting the scanner. A bundle exists only when both content-addressed raw objects and their SHA-256 values are readable. A missing TOS object blocks finalization. Runtime, PoC, official-log, and statement artifacts follow the platform backup policy, and the Plan 06 RPO 1 hour/RTO 4 hour exercise must include one `reconciliation_required` task.

- [ ] **Step 4: Run migration, type, lint, fixture, ordinary, and contract gates**

Run:

```bash
docker compose up -d postgres
cd backend
env -u ARK_API_KEY uv run alembic downgrade 0001_foundation
env -u ARK_API_KEY uv run alembic upgrade 0002_intelligence
env -u ARK_API_KEY uv run alembic check
test "$(env -u ARK_API_KEY uv run alembic heads)" = "0002_intelligence (head)"
env -u ARK_API_KEY uv run ruff check src tests scripts
env -u ARK_API_KEY uv run mypy src
env -u ARK_API_KEY uv run pytest --collect-only -q
env -u ARK_API_KEY uv run pytest \
  tests/unit/intelligence \
  tests/integration/intelligence \
  tests/contract/intelligence/test_plan02a_source_contract.py \
  tests/integration/tasks/test_task_transaction.py \
  tests/integration/billing/test_internal_budget.py \
  tests/contract/billing/test_generation_limit_composition.py -q
```

Expected: migration downgrade/upgrade/check pass with exactly one `0002_intelligence` head; Ruff and Mypy exit 0; collection resolves every non-parametrized test argument from a local fixture, Plan 01 `backend/tests/conftest.py`, Plan 02 `backend/tests/integration/intelligence/conftest.py`, or the explicitly registered support fixture; all tests pass with no provider credential, external HTTP, DNS, or paid call.

- [ ] **Step 5: Run repository-wide release and forbidden-pattern checks**

Run:

```bash
cd backend
env -u ARK_API_KEY uv run pytest tests/unit/intelligence/test_provider_release.py tests/unit/intelligence/test_provider_release_runtime.py -q
test -z "$(rg -n 'AsyncSession|async def' src/ip_saas/modules/intelligence)"
test -z "$(rg -n 'apiRequest\("/api/v1|generated\.ts' ../frontend/src ../frontend/tests)"
test "$(find ../frontend/src -name 'schema.d.ts' -print | wc -l | tr -d ' ')" = "1"
test -z "$(find ../contracts/events -type f -newer tests/contract/intelligence/test_plan02a_source_contract.py -print)"
```

Expected: release tests pass; intelligence contains no async database/application path; frontend retains the one Plan 01 `schema.d.ts` and shared `/v1` client boundary; Plan 02a has introduced no event schema or second API client. Review the `find` output by the addendum commit range rather than wall-clock timestamp if the worktree contains unrelated newer user files.

- [ ] **Step 6: Commit the source contract and runbook closeout**

```bash
git add backend/tests/contract/intelligence/test_plan02a_source_contract.py docs/runbooks/intelligence-provider-reconciliation.md
git commit -m "test: freeze durable provider reconciliation contracts"
```

## Final acceptance checklist

- [ ] This remains Plan 02a safety hardening inside business subproject 2; the master roadmap still has six business subprojects.
- [ ] Migration inventory mechanically selects the greenfield branch before editing; the implemented greenfield branch has one `0002_intelligence` revision, `down_revision="0001_foundation"`, and no extra head. Any already-deployed `0002` blocks execution until a separately reviewed chain update is approved.
- [ ] Every logical Responses or Web Search call has a deterministic key and stable UUID derived only from task ID, the 64-character task request fingerprint, adapter, operation, and the upstream stable logical-call ID; canonical payload is a separately compared SHA-256, so changed input under the same logical call raises `Conflict` instead of creating another call.
- [ ] `prepare` commits before the send marker, the send marker commits before HTTP, and no unresolved send marker is ever resent after lease expiry, Worker restart, RocketMQ redelivery, or process SIGKILL.
- [ ] Provider request IDs are exact, trimmed, 1 through 200 characters, identical in response completion, journal, official log, manifest, `ProviderCostInput`, and `ProviderCostEntry`; exact replay is idempotent and changed replay raises `Conflict`.
- [ ] Official request logs require `complete=true`; statements require `final=true`; both are `extra="forbid"`, hash-pinned, scoped to the exact provider account/run, and stored in private content-addressed TOS objects.
- [ ] Missing, partial, duplicate, extra, non-final, cross-account, cross-model, cross-task, out-of-window, stale-attempt, mismatched-response, and changed-replay evidence is rejected without changing the task, hold, bundle, match, cost, or attempt disposition.
- [ ] Every sent marker resolves positively to either one supplier-matched request or complete/final supplier zero-call evidence. A database anti-join alone never releases a hold.
- [ ] `JournalNoProviderCallEvidencePort` includes the pinned egress/runtime digest and all supplier zero-call artifact hashes. `JournalProviderCostManifestPort` returns the complete, unique, sorted supplier request set for the exact task and exhausted attempt.
- [ ] The independent scanner selects `reconciliation_required` tasks with `FOR UPDATE SKIP LOCKED` and invokes exactly one Plan 01 batch finalizer per task. It never settles individual rows or directly changes task/hold truth.
- [ ] Failed customer work passes `actual_amount=0`; failed platform-internal work passes the exact sum of all supplier `amount_fen`; every non-zero cost has the exact task ID and one exact provider request ID.
- [ ] Production scanner composition explicitly uses Plan 05's real `GenerationLimitService`; production contains no allow fake, deny adapter, optional third billing dependency, or implicit unlimited path.
- [ ] Production Workers have no direct Ark egress. The controlled APIG/VKE route logs and strips the stable correlation header, and the signed runtime evidence proves the official request-log-to-provider-identity join for both adapters.
- [ ] Ark secrets and HTTP clients remain unreachable unless the Plan 02a runtime artifact and Plan 02 six-cell crash artifact both validate independently. Missing, changed, stale, or partial evidence fails before secret access.
- [ ] The real dedicated-account PoC uses a provider hard cap and real internal limits no higher than 100 fen, covers both adapters and all three fault points, and includes actual SIGKILL after paid response but before journal/cost persistence.
- [ ] The local SIGKILL suite, all ordinary tests, collection, lint, typing, migration, source-contract, Plan 01 task/accounting, and Plan 05 limit-composition regressions pass with `ARK_API_KEY` removed and zero network or paid calls.
- [ ] Plan 02a adds no endpoint, frontend API client, frontend generated schema, product role, ledger, task status, event type, or outbox delivery truth.

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-24-02a-durable-provider-attempt-supplier-reconciliation.md`. Two execution options:

1. **Subagent-Driven (recommended)** — use `superpowers:subagent-driven-development`, assign one fresh implementation worker per task, run specification and code-quality review after every task, and stop immediately if migration inventory, supplier export capability, provider hard cap, egress enforcement, or either production attestation fails.
2. **Inline Execution** — use `superpowers:executing-plans`, execute Tasks 1–10 in order in this session, run every named checkpoint, and keep Ark Responses/Web Search disabled until the independently budgeted production PoC emits a reviewed, SHA-256-pinned `passed=true` artifact.
