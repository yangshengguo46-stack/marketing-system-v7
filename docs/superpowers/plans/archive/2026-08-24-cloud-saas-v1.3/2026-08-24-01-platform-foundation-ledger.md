# Platform Foundation and Trusted Ledgers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the runnable SaaS foundation with authentication, fixed account ownership, isolated C-user/platform IP projects, append-only credit and internal-CNY ledgers, atomic holds/tasks/outbox/audit, model registration, private-object storage contracts, OpenAPI, and the first role-aware web shell.

**Architecture:** Implement one Python modular monolith with synchronous FastAPI routes and SQLAlchemy 2.0 synchronous `Session`; long-running work is represented by persisted tasks and dispatched from a transactional outbox to later Workers. PostgreSQL is the only transactional source of truth, while authentication, tenant access, billing, tasking, audit, model registry, and private object storage are separated behind small application services and ports. The Next.js application consumes a generated OpenAPI type file and exposes separate C-user, reseller, and platform route shells without adding team collaboration or online payment.

**Tech Stack:** Python 3.12, FastAPI 0.136.x, Pydantic 2.x, SQLAlchemy 2.0.x synchronous ORM, Alembic, PostgreSQL 16, psycopg 3, PyJWT, Argon2, Pytest, Hypothesis, Node.js 24, pnpm 10, Next.js 16 App Router, React 19, TypeScript 5.x, Vitest, Playwright, RocketMQ contract adapter, Volcengine TOS Python SDK, Docker Compose.

---

## Scope and frozen contracts

This plan implements subproject 1 from the master roadmap. It deliberately stops before interview logic, content Agents, real Seedream/Seedance/TTS calls, media processing, publication metrics, reseller cash-order workflows, support grants, and digital-human governance. It does create the stable account, project, billing, task, event, model, and storage contracts those later plans import.

The following import paths and symbols are public and must not be renamed by later tasks:

```text
ip_saas.modules.accounts.context.ActorContext
ip_saas.modules.accounts.context.ActorKind
ip_saas.db.base.Base
ip_saas.db.session.session_scope
ip_saas.common.clock.Clock
ip_saas.common.clock.SystemClock
ip_saas.common.audit.AuditWriter
ip_saas.common.outbox.OutboxWriter
ip_saas.common.outbox.EventEnvelope
ip_saas.modules.projects.models.IPProject
ip_saas.modules.projects.access.ProjectAccessService
ip_saas.modules.billing.service.BillingService
ip_saas.modules.billing.service.BillingContext
ip_saas.modules.billing.service.BillingMode
ip_saas.modules.billing.service.ProviderCostInput
ip_saas.modules.billing.service.canonical_provider_request_id
ip_saas.modules.billing.ports.CreditHoldPort
ip_saas.modules.billing.ports.InternalBudgetHoldPort
ip_saas.modules.billing.ports.GenerationLimitPort
ip_saas.modules.tasks.reconciliation.NoProviderCallEvidence
ip_saas.modules.tasks.reconciliation.NoProviderCallEvidencePort
ip_saas.modules.tasks.reconciliation.ProviderRequestIdentity
ip_saas.modules.tasks.reconciliation.ProviderCostManifestPort
ip_saas.modules.tasks.reconciliation.TaskReconciliationService
ip_saas.workers.task_consumer.TaskHandlerDisposition
ip_saas.common.errors.DomainError
ip_saas.common.errors.Conflict
ip_saas.common.errors.Forbidden
ip_saas.common.errors.NotFound
```

The retry-exhaustion API is frozen as:

```python
class TaskReconciliationService:
    def finalize_no_provider_call(
        self,
        session: Session,
        task_id: UUID,
        attempt_no: int,
        reason: str,
        idempotency_key: str,
    ) -> TaskRecord: ...

    def finalize_provider_costs(
        self,
        session: Session,
        task_id: UUID,
        attempt_no: int,
        actual_amount: int,
        provider_costs: tuple[ProviderCostInput, ...],
        reason: str,
        idempotency_key: str,
    ) -> TaskRecord: ...


class BillingService:
    def settle_generation_batch(
        self,
        session: Session,
        context: BillingContext,
        actual_amount: int,
        provider_costs: tuple[ProviderCostInput, ...],
        idempotency_key: str,
    ) -> None: ...
```

Later plans implement `NoProviderCallEvidencePort` and `ProviderCostManifestPort` over their own durable provider-attempt journals, but they may not replace either with a boolean/no-op adapter or create a second retry-exhaustion finalizer. `BillingService.settle_generation_batch(...)` is an additive batch primitive used by this finalizer; the four previously frozen reserve/settle/release signatures remain source-compatible.

The database rules frozen by this plan are:

- `0001_foundation` is the only Alembic revision created here; it has `down_revision = None`.
- UUID4 values are generated in application code, timestamps are timezone-aware UTC, points use integer `credit_units`, and internal money uses integer `amount_fen`.
- SQLAlchemy uses synchronous `Session` everywhere. FastAPI ordinary routes are synchronous `def`; provider work is represented by tasks for Workers. No `AsyncSession` appears in the repository.
- All transaction-semantics tests run against PostgreSQL. SQLite is not a fallback test database.
- A C-user project belongs to exactly one C-user account. A platform project belongs to the platform account and never enters the reseller tree.
- Platform operators do not receive implicit access to C-user project content. Foundation APIs have no support-access bypass.
- User/reseller credit and platform-internal CNY cost are separate ledgers and separate holds. A task references exactly one billing mode.
- Credit postings balance to zero per transaction. User-facing wallets never become negative. Only named system clearing wallets may allow a negative posted balance.
- Ledger entries, provider costs, audit records, and emitted outbox payloads are append-only. Corrections use reversing records.
- A high-cost task is created only in the same PostgreSQL transaction that reserves its hold, writes its audit event, and writes its outbox event.
- A lease owner may heartbeat or write a terminal result only while its captured `attempt_no` still matches and its lease is unexpired. Retry exhaustion enters non-terminal `reconciliation_required`; it never proves zero supplier usage and never releases a hold by itself.

## File map

```text
.editorconfig
.env.example
.gitignore
.github/workflows/ci.yml
Makefile
README.md
docker-compose.yml
contracts/openapi.json
contracts/events/event-envelope.v1.json
contracts/events/generation.task.requested.v1.json
infra/docker/postgres/init-test-db.sql
backend/pyproject.toml
backend/uv.lock
backend/alembic.ini
backend/migrations/env.py
backend/migrations/script.py.mako
backend/migrations/versions/0001_foundation.py
backend/src/ip_saas/__init__.py
backend/src/ip_saas/api.py
backend/src/ip_saas/config.py
backend/src/ip_saas/db/base.py
backend/src/ip_saas/db/session.py
backend/src/ip_saas/common/clock.py
backend/src/ip_saas/common/errors.py
backend/src/ip_saas/common/audit.py
backend/src/ip_saas/common/outbox.py
backend/src/ip_saas/common/tasking.py
backend/src/ip_saas/modules/accounts/context.py
backend/src/ip_saas/modules/accounts/models.py
backend/src/ip_saas/modules/accounts/security.py
backend/src/ip_saas/modules/accounts/service.py
backend/src/ip_saas/modules/accounts/dependencies.py
backend/src/ip_saas/modules/accounts/router.py
backend/src/ip_saas/modules/projects/models.py
backend/src/ip_saas/modules/projects/access.py
backend/src/ip_saas/modules/projects/service.py
backend/src/ip_saas/modules/projects/router.py
backend/src/ip_saas/modules/billing/domain.py
backend/src/ip_saas/modules/billing/models.py
backend/src/ip_saas/modules/billing/ports.py
backend/src/ip_saas/modules/billing/repository.py
backend/src/ip_saas/modules/billing/adapters.py
backend/src/ip_saas/modules/billing/service.py
backend/src/ip_saas/modules/billing/router.py
backend/src/ip_saas/modules/model_registry/models.py
backend/src/ip_saas/modules/model_registry/service.py
backend/src/ip_saas/modules/model_registry/router.py
backend/src/ip_saas/modules/tasks/models.py
backend/src/ip_saas/modules/tasks/events.py
backend/src/ip_saas/modules/tasks/reconciliation.py
backend/src/ip_saas/modules/tasks/service.py
backend/src/ip_saas/modules/tasks/router.py
backend/src/ip_saas/providers/object_store.py
backend/src/ip_saas/providers/fake_object_store.py
backend/src/ip_saas/providers/tos_object_store.py
backend/src/ip_saas/providers/rocketmq.py
backend/src/ip_saas/scripts/export_contracts.py
backend/src/ip_saas/workers/outbox_dispatcher.py
backend/src/ip_saas/workers/task_consumer.py
backend/tests/conftest.py
backend/tests/unit/common/test_clock_and_errors.py
backend/tests/unit/accounts/test_security.py
backend/tests/unit/billing/test_credit_planner.py
backend/tests/unit/billing/test_randomized_ledgers.py
backend/tests/unit/tasks/test_task_consumer.py
backend/tests/integration/test_health_and_migrations.py
backend/tests/integration/accounts/test_auth.py
backend/tests/integration/accounts/test_hierarchy_and_roles.py
backend/tests/integration/projects/test_project_isolation.py
backend/tests/integration/common/test_audit_outbox.py
backend/tests/integration/billing/test_credit_ledger.py
backend/tests/integration/billing/test_credit_concurrency.py
backend/tests/integration/billing/test_internal_costs.py
backend/tests/integration/model_registry/test_model_registry.py
backend/tests/integration/tasks/test_task_submission.py
backend/tests/integration/tasks/test_task_lease_concurrency.py
backend/tests/contract/test_event_contracts.py
backend/tests/contract/test_openapi.py
backend/tests/security/test_cross_tenant_access.py
backend/tests/providers/test_object_store_contract.py
backend/tests/providers/test_rocketmq_contract.py
frontend/package.json
frontend/pnpm-lock.yaml
frontend/next.config.ts
frontend/tsconfig.json
frontend/vitest.config.ts
frontend/playwright.config.ts
frontend/src/app/globals.css
frontend/src/app/layout.tsx
frontend/src/app/page.tsx
frontend/src/app/login/page.tsx
frontend/src/app/(c-user)/projects/page.tsx
frontend/src/app/(reseller)/reseller/page.tsx
frontend/src/app/(platform)/platform/page.tsx
frontend/src/features/auth/LoginForm.tsx
frontend/src/features/shell/AppShell.tsx
frontend/src/features/shell/role-home.ts
frontend/src/features/shell/role-home.test.ts
frontend/src/lib/api/client.ts
frontend/src/lib/api/schema.d.ts
frontend/tests/e2e/role-shells.spec.ts
```

Every Python package directory in the map also receives an empty `__init__.py`; package marker creation is included in Task 1 and is not repeated in later file lists.

### Task 1: Scaffold reproducible Python, web, and local-service workspaces

**Files:**
- Create: `.editorconfig`
- Create: `.env.example`
- Create: `.gitignore`
- Create: `Makefile`
- Create: `docker-compose.yml`
- Create: `infra/docker/postgres/init-test-db.sql`
- Create: `backend/pyproject.toml`
- Create: `backend/src/ip_saas/__init__.py`
- Create: all package-marker `__init__.py` files listed in the file map
- Create: `frontend/package.json`
- Create: `frontend/tsconfig.json`
- Create: `frontend/next.config.ts`

- [ ] **Step 1: Create the root ignore and editor rules**

Create `.editorconfig` and `.gitignore` with these exact contents:

```ini
# .editorconfig
root = true

[*]
charset = utf-8
end_of_line = lf
insert_final_newline = true
indent_style = space
indent_size = 2

[*.py]
indent_size = 4
```

```gitignore
# .gitignore
.env
.venv/
__pycache__/
.pytest_cache/
.mypy_cache/
.ruff_cache/
*.pyc
backend/.coverage
backend/htmlcov/
frontend/node_modules/
frontend/.next/
frontend/playwright-report/
frontend/test-results/
```

- [ ] **Step 2: Create local configuration without secrets**

Create `.env.example`:

```dotenv
APP_ENV=development
DATABASE_URL=postgresql+psycopg://ip_saas:ip_saas@localhost:5432/ip_saas
TEST_DATABASE_URL=postgresql+psycopg://ip_saas:ip_saas@localhost:5432/ip_saas_test
JWT_SECRET=replace-with-at-least-32-random-characters
JWT_ISSUER=ip-saas
ACCESS_TOKEN_MINUTES=15
REFRESH_TOKEN_DAYS=30
TOS_ENDPOINT=tos-cn-beijing.volces.com
TOS_REGION=cn-beijing
TOS_BUCKET=ip-saas-private
TOS_ACCESS_KEY=
TOS_SECRET_KEY=
ROCKETMQ_ENDPOINT=127.0.0.1:8081
ROCKETMQ_TOPIC=ip-saas-generation
ROCKETMQ_CONSUMER_GROUP=ip-saas-workers
ROCKETMQ_ACCESS_KEY=
ROCKETMQ_SECRET_KEY=
NEXT_PUBLIC_API_BASE_URL=/api
BACKEND_INTERNAL_URL=http://127.0.0.1:8000
```

- [ ] **Step 3: Create PostgreSQL-first local services**

Create `infra/docker/postgres/init-test-db.sql`:

```sql
CREATE DATABASE ip_saas_test OWNER ip_saas;
```

Create `docker-compose.yml`:

```yaml
services:
  postgres:
    image: postgres:16.4
    environment:
      POSTGRES_USER: ip_saas
      POSTGRES_PASSWORD: ip_saas
      POSTGRES_DB: ip_saas
    ports: ["5432:5432"]
    volumes:
      - pg_data:/var/lib/postgresql/data
      - ./infra/docker/postgres/init-test-db.sql:/docker-entrypoint-initdb.d/01-test-db.sql:ro
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ip_saas -d ip_saas"]
      interval: 2s
      timeout: 2s
      retries: 20
  redis:
    image: redis:7.4-alpine
    ports: ["6379:6379"]
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 2s
      timeout: 2s
      retries: 20
  rocketmq-nameserver:
    image: apache/rocketmq:5.3.2
    command: sh mqnamesrv
    ports: ["9876:9876"]
  rocketmq-broker:
    image: apache/rocketmq:5.3.2
    command: sh mqbroker -n rocketmq-nameserver:9876
    depends_on: [rocketmq-nameserver]
    environment:
      NAMESRV_ADDR: rocketmq-nameserver:9876
    ports: ["10909:10909", "10911:10911", "10912:10912"]
  rocketmq-proxy:
    image: apache/rocketmq:5.3.2
    command: sh mqproxy
    depends_on: [rocketmq-nameserver, rocketmq-broker]
    environment:
      NAMESRV_ADDR: rocketmq-nameserver:9876
    ports: ["8080:8080", "8081:8081"]
volumes:
  pg_data:
```

- [ ] **Step 4: Create the Python dependency and tool configuration**

Create `backend/pyproject.toml`:

```toml
[project]
name = "ip-agent-saas"
version = "0.1.0"
requires-python = ">=3.12,<3.13"
dependencies = [
  "alembic>=1.16,<2",
  "argon2-cffi>=25,<26",
  "fastapi>=0.136,<0.137",
  "httpx>=0.28,<0.29",
  "psycopg[binary]>=3.2,<4",
  "pydantic-settings>=2.10,<3",
  "pyjwt>=2.10,<3",
  "rocketmq-python-client>=5.1.1,<5.2",
  "sqlalchemy>=2.0.43,<2.1",
  "tos>=2.8,<3",
  "uvicorn[standard]>=0.35,<0.36",
  "volcengine-python-sdk[ark]>=5.0.46,<6",
]

[dependency-groups]
dev = [
  "hypothesis>=6.138,<7",
  "mypy>=1.17,<2",
  "pytest>=8.4,<9",
  "pytest-cov>=6.2,<7",
  "pytest-repeat>=0.9,<1",
  "ruff>=0.12,<0.13",
]

[build-system]
requires = ["hatchling>=1.27,<2"]
build-backend = "hatchling.build"

[tool.hatch.build.targets.wheel]
packages = ["src/ip_saas"]

[tool.pytest.ini_options]
testpaths = ["tests"]
addopts = "-ra --strict-markers"
markers = ["integration: requires local PostgreSQL", "contract: verifies public schemas"]

[tool.ruff]
line-length = 100
target-version = "py312"

[tool.ruff.lint]
select = ["E", "F", "I", "B", "UP", "SIM"]
ignore = ["E501"]

[tool.mypy]
python_version = "3.12"
strict = true
plugins = ["pydantic.mypy"]
```

- [ ] **Step 5: Create the frontend dependency and proxy configuration**

Create `frontend/package.json`, `frontend/tsconfig.json`, and `frontend/next.config.ts`:

```json
{
  "name": "ip-agent-saas-web",
  "private": true,
  "packageManager": "pnpm@10.15.0",
  "scripts": {
    "dev": "next dev",
    "build": "next build",
    "lint": "eslint .",
    "typecheck": "tsc --noEmit",
    "test": "vitest run",
    "test:e2e": "playwright test",
    "generate:api": "openapi-typescript ../contracts/openapi.json -o src/lib/api/schema.d.ts"
  },
  "dependencies": {
    "next": "16.0.0",
    "react": "19.1.1",
    "react-dom": "19.1.1"
  },
  "devDependencies": {
    "@playwright/test": "1.55.0",
    "@testing-library/jest-dom": "6.8.0",
    "@testing-library/react": "16.3.0",
    "@types/node": "24.3.0",
    "@types/react": "19.1.10",
    "@types/react-dom": "19.1.7",
    "eslint": "9.34.0",
    "eslint-config-next": "16.0.0",
    "jsdom": "26.1.0",
    "openapi-typescript": "7.9.1",
    "typescript": "5.9.2",
    "vitest": "3.2.4"
  }
}
```

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["dom", "dom.iterable", "esnext"],
    "allowJs": false,
    "skipLibCheck": true,
    "strict": true,
    "noEmit": true,
    "esModuleInterop": true,
    "module": "esnext",
    "moduleResolution": "bundler",
    "resolveJsonModule": true,
    "jsx": "react-jsx",
    "incremental": true,
    "plugins": [{"name": "next"}],
    "paths": {"@/*": ["./src/*"]}
  },
  "include": ["next-env.d.ts", "**/*.ts", "**/*.tsx", ".next/types/**/*.ts"],
  "exclude": ["node_modules"]
}
```

```typescript
// frontend/next.config.ts
import type {NextConfig} from "next";

const backend = process.env.BACKEND_INTERNAL_URL ?? "http://127.0.0.1:8000";

const config: NextConfig = {
  async rewrites() {
    return [{source: "/api/:path*", destination: `${backend}/:path*`}];
  },
};

export default config;
```

- [ ] **Step 6: Create package markers and lock dependency graphs**

Run:

```bash
mkdir -p backend/src/ip_saas/{db,common,providers,scripts,workers} \
  backend/src/ip_saas/modules/{accounts,projects,billing,model_registry,tasks} \
  backend/tests/{unit,integration,contract,security,providers} \
  frontend/src/lib/api
find backend/src/ip_saas backend/tests -type d -exec touch {}/__init__.py \;
cd backend && uv lock
cd ../frontend && pnpm install --lockfile-only
```

Expected: `backend/uv.lock` and `frontend/pnpm-lock.yaml` are created; neither command asks for provider credentials.

- [ ] **Step 7: Add stable root commands**

Create `Makefile`:

```make
.PHONY: bootstrap services-up services-down lint test-unit test-integration test-e2e test export-contracts

bootstrap:
	cd backend && uv sync --all-groups
	cd frontend && pnpm install --frozen-lockfile
	cd frontend && pnpm exec playwright install chromium

services-up:
	docker compose up -d postgres redis rocketmq-nameserver rocketmq-broker rocketmq-proxy

services-down:
	docker compose down

lint:
	cd backend && uv run ruff check src tests && uv run mypy src
	cd frontend && pnpm lint && pnpm typecheck

test-unit:
	cd backend && uv run pytest tests/unit -v
	cd frontend && pnpm test

test-integration:
	cd backend && uv run pytest tests/integration tests/contract tests/security tests/providers -v

test-e2e:
	cd frontend && pnpm test:e2e

test: test-unit test-integration test-e2e

export-contracts:
	cd backend && uv run python -m ip_saas.scripts.export_contracts --check
```

- [ ] **Step 8: Verify bootstrap**

Run: `make bootstrap`

Expected: exit code 0; Python and pnpm dependencies install; no Volcengine, TOS, SMS, payment, or publishing credential is read.

- [ ] **Step 9: Commit the scaffold**

```bash
git add .editorconfig .env.example .gitignore Makefile docker-compose.yml infra/docker/postgres/init-test-db.sql backend/pyproject.toml backend/uv.lock backend/src frontend/package.json frontend/pnpm-lock.yaml frontend/tsconfig.json frontend/next.config.ts
git commit -m "chore: scaffold platform foundation"
```

### Task 2: Add validated configuration, synchronous database sessions, clock, and domain errors

**Files:**
- Create: `backend/src/ip_saas/config.py`
- Create: `backend/src/ip_saas/db/base.py`
- Create: `backend/src/ip_saas/db/session.py`
- Create: `backend/src/ip_saas/common/clock.py`
- Create: `backend/src/ip_saas/common/errors.py`
- Create: `backend/tests/unit/common/test_clock_and_errors.py`
- Create: `backend/tests/conftest.py`
- Create: `backend/tests/integration/test_health_and_migrations.py`
- Create: `backend/src/ip_saas/api.py`

- [ ] **Step 1: Write the failing clock and error test**

Create `backend/tests/unit/common/test_clock_and_errors.py`:

```python
import json
from datetime import UTC, datetime

from ip_saas.common.clock import Clock
from ip_saas.common.errors import Conflict, DomainError, Forbidden, NotFound


class FixedClock:
    def now(self) -> datetime:
        return datetime(2026, 8, 24, 8, 0, tzinfo=UTC)


def test_clock_contract_is_timezone_aware() -> None:
    clock: Clock = FixedClock()
    assert clock.now().utcoffset() is not None


def test_domain_errors_have_stable_codes() -> None:
    errors: list[DomainError] = [Conflict("duplicate"), Forbidden(), NotFound("project")]
    assert [error.code for error in errors] == ["conflict", "forbidden", "not_found"]
```

- [ ] **Step 2: Run the unit test and verify the missing-module failure**

Run: `cd backend && uv run pytest tests/unit/common/test_clock_and_errors.py -v`

Expected: FAIL during collection with `ModuleNotFoundError: No module named 'ip_saas.common.clock'`.

- [ ] **Step 3: Implement clock and error contracts**

Create `backend/src/ip_saas/common/clock.py`:

```python
from datetime import UTC, datetime
from typing import Protocol


class Clock(Protocol):
    def now(self) -> datetime: ...


class SystemClock:
    def now(self) -> datetime:
        return datetime.now(UTC)
```

Create `backend/src/ip_saas/common/errors.py`:

```python
class DomainError(Exception):
    code = "domain_error"

    def __init__(self, message: str = "domain operation failed") -> None:
        self.message = message
        super().__init__(message)


class Conflict(DomainError):
    code = "conflict"


class Forbidden(DomainError):
    code = "forbidden"

    def __init__(self, message: str = "operation is not allowed") -> None:
        super().__init__(message)


class NotFound(DomainError):
    code = "not_found"

    def __init__(self, resource: str = "resource") -> None:
        super().__init__(f"{resource} not found")
```

- [ ] **Step 4: Run the unit test and verify it passes**

Run: `cd backend && uv run pytest tests/unit/common/test_clock_and_errors.py -v`

Expected: PASS, 2 tests.

- [ ] **Step 5: Add the PostgreSQL-only test fixtures**

Create `backend/tests/conftest.py`:

```python
import os
from collections.abc import Iterator

import pytest
from sqlalchemy import Engine
from sqlalchemy.orm import Session

from ip_saas.db.base import Base
from ip_saas.db.session import build_engine


@pytest.fixture(scope="session")
def test_database_url() -> str:
    value = os.environ.get(
        "TEST_DATABASE_URL",
        "postgresql+psycopg://ip_saas:ip_saas@localhost:5432/ip_saas_test",
    )
    if not value.startswith("postgresql+"):
        raise RuntimeError("integration tests require PostgreSQL")
    return value


@pytest.fixture(scope="session")
def pg_engine(test_database_url: str) -> Iterator[Engine]:
    engine = build_engine(test_database_url)
    yield engine
    engine.dispose()


@pytest.fixture
def db_session(pg_engine: Engine) -> Iterator[Session]:
    Base.metadata.create_all(pg_engine)
    connection = pg_engine.connect()
    transaction = connection.begin()
    session = Session(bind=connection, expire_on_commit=False)
    try:
        yield session
    finally:
        session.close()
        transaction.rollback()
        connection.close()
```

- [ ] **Step 6: Write the failing synchronous-session health test**

Create `backend/tests/integration/test_health_and_migrations.py`:

```python
from fastapi.testclient import TestClient
from sqlalchemy import text
from sqlalchemy.orm import Session

from ip_saas.api import create_app
from ip_saas.db.session import build_engine, build_session_factory


def test_session_factory_uses_postgresql(test_database_url: str) -> None:
    engine = build_engine(test_database_url)
    factory = build_session_factory(engine)
    with factory() as session:
        assert isinstance(session, Session)
        assert session.scalar(text("select current_database()")) == "ip_saas_test"


def test_health_endpoint() -> None:
    response = TestClient(create_app()).get("/healthz")
    assert response.status_code == 200
    assert response.json() == {"status": "ok"}
```

- [ ] **Step 7: Run the integration test and verify the missing-function failure**

Run: `docker compose up -d postgres && cd backend && TEST_DATABASE_URL=postgresql+psycopg://ip_saas:ip_saas@localhost:5432/ip_saas_test uv run pytest tests/integration/test_health_and_migrations.py -v`

Expected: FAIL during collection because `create_app` or `build_engine` is not defined.

- [ ] **Step 8: Implement settings, Base, and synchronous session management**

Create `backend/src/ip_saas/config.py`:

```python
from functools import lru_cache

from pydantic import SecretStr
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", extra="ignore")

    app_env: str = "development"
    database_url: str = "postgresql+psycopg://ip_saas:ip_saas@localhost:5432/ip_saas"
    test_database_url: str = "postgresql+psycopg://ip_saas:ip_saas@localhost:5432/ip_saas_test"
    jwt_secret: SecretStr = SecretStr("development-only-secret-change-me")
    jwt_issuer: str = "ip-saas"
    access_token_minutes: int = 15
    refresh_token_days: int = 30
    tos_endpoint: str = "tos-cn-beijing.volces.com"
    tos_region: str = "cn-beijing"
    tos_bucket: str = "ip-saas-private"
    tos_access_key: SecretStr = SecretStr("")
    tos_secret_key: SecretStr = SecretStr("")
    rocketmq_endpoint: str = "127.0.0.1:8081"
    rocketmq_topic: str = "ip-saas-generation"
    rocketmq_consumer_group: str = "ip-saas-workers"
    rocketmq_access_key: SecretStr = SecretStr("")
    rocketmq_secret_key: SecretStr = SecretStr("")


@lru_cache
def get_settings() -> Settings:
    return Settings()
```

Create `backend/src/ip_saas/db/base.py`:

```python
from datetime import datetime
from uuid import UUID, uuid4

from sqlalchemy import DateTime
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column


class Base(DeclarativeBase):
    type_annotation_map = {datetime: DateTime(timezone=True)}


class UUIDPrimaryKeyMixin:
    id: Mapped[UUID] = mapped_column(primary_key=True, default=uuid4)


class TimestampMixin:
    created_at: Mapped[datetime]
```

Create `backend/src/ip_saas/db/session.py`:

```python
from collections.abc import Iterator
from contextlib import contextmanager

from sqlalchemy import Engine, create_engine
from sqlalchemy.orm import Session, sessionmaker

from ip_saas.config import get_settings


def build_engine(database_url: str) -> Engine:
    if not database_url.startswith("postgresql+"):
        raise ValueError("PostgreSQL is required")
    return create_engine(database_url, pool_pre_ping=True)


def build_session_factory(engine: Engine) -> sessionmaker[Session]:
    return sessionmaker(bind=engine, class_=Session, expire_on_commit=False)


ENGINE = build_engine(get_settings().database_url)
SessionFactory = build_session_factory(ENGINE)


@contextmanager
def session_scope(factory: sessionmaker[Session] = SessionFactory) -> Iterator[Session]:
    with factory() as session:
        try:
            with session.begin():
                yield session
        except Exception:
            session.rollback()
            raise
```

- [ ] **Step 9: Implement the synchronous FastAPI composition root and error mapping**

Create `backend/src/ip_saas/api.py`:

```python
from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse

from ip_saas.common.errors import Conflict, DomainError, Forbidden, NotFound


STATUS_BY_ERROR = {Conflict: 409, Forbidden: 403, NotFound: 404}


def create_app() -> FastAPI:
    app = FastAPI(title="AI IP SaaS API", version="0.1.0")

    @app.exception_handler(DomainError)
    def handle_domain_error(_request: Request, error: DomainError) -> JSONResponse:
        status = next(
            (value for error_type, value in STATUS_BY_ERROR.items() if isinstance(error, error_type)),
            400,
        )
        return JSONResponse(status_code=status, content={"code": error.code, "message": error.message})

    @app.get("/healthz", tags=["system"])
    def health() -> dict[str, str]:
        return {"status": "ok"}

    return app


app = create_app()
```

- [ ] **Step 10: Run unit and PostgreSQL integration tests**

Run: `cd backend && TEST_DATABASE_URL=postgresql+psycopg://ip_saas:ip_saas@localhost:5432/ip_saas_test uv run pytest tests/unit/common/test_clock_and_errors.py tests/integration/test_health_and_migrations.py -v`

Expected: PASS, 4 tests; the session assertion reports database `ip_saas_test`.

- [ ] **Step 11: Commit the shared runtime contracts**

```bash
git add backend/src/ip_saas/config.py backend/src/ip_saas/db backend/src/ip_saas/common/clock.py backend/src/ip_saas/common/errors.py backend/src/ip_saas/api.py backend/tests/conftest.py backend/tests/unit/common backend/tests/integration/test_health_and_migrations.py
git commit -m "feat: add synchronous application foundation"
```

### Task 3: Model account kinds, fixed reseller ownership, platform roles, and actor context

**Files:**
- Create: `backend/src/ip_saas/modules/accounts/context.py`
- Create: `backend/src/ip_saas/modules/accounts/models.py`
- Create: `backend/src/ip_saas/modules/accounts/service.py`
- Create: `backend/tests/integration/accounts/test_hierarchy_and_roles.py`
- Modify: `backend/src/ip_saas/db/base.py`

- [ ] **Step 1: Give persisted rows application-generated UTC timestamps**

Modify `TimestampMixin` in `backend/src/ip_saas/db/base.py`:

```python
from datetime import UTC, datetime


class TimestampMixin:
    created_at: Mapped[datetime] = mapped_column(default=lambda: datetime.now(UTC))
```

- [ ] **Step 2: Write the failing hierarchy and platform-role tests**

Create `backend/tests/integration/accounts/test_hierarchy_and_roles.py`:

```python
from uuid import uuid4

import pytest
from sqlalchemy.orm import Session

from ip_saas.common.errors import Forbidden
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import Account, AccountKind, PlatformRole, Principal
from ip_saas.modules.accounts.service import AccountService


def platform_actor(account: Account, principal: Principal) -> ActorContext:
    return ActorContext(
        actor_id=principal.id,
        account_id=account.id,
        kind=ActorKind.PLATFORM_ADMIN,
        platform_roles=frozenset({PlatformRole.ADMIN}),
    )


def test_fixed_account_tree_and_no_third_reseller_level(db_session: Session) -> None:
    service = AccountService()
    platform = service.bootstrap_platform(db_session, "Platform")
    admin = service.add_principal(db_session, platform.id, "admin", "hash")
    actor = platform_actor(platform, admin)
    l1 = service.create_child_account(db_session, actor, platform.id, AccountKind.RESELLER_L1, "L1")
    l1_actor = ActorContext(uuid4(), l1.id, ActorKind.RESELLER_L1)
    l2 = service.create_child_account(db_session, l1_actor, l1.id, AccountKind.RESELLER_L2, "L2")
    c_direct = service.create_child_account(db_session, l1_actor, l1.id, AccountKind.C_USER, "C1")
    l2_actor = ActorContext(uuid4(), l2.id, ActorKind.RESELLER_L2)
    c_nested = service.create_child_account(db_session, l2_actor, l2.id, AccountKind.C_USER, "C2")

    assert {c_direct.kind, c_nested.kind} == {AccountKind.C_USER}
    with pytest.raises(Forbidden):
        service.create_child_account(db_session, l2_actor, l2.id, AccountKind.RESELLER_L2, "L3")


def test_platform_role_cannot_be_granted_to_reseller(db_session: Session) -> None:
    service = AccountService()
    platform = service.bootstrap_platform(db_session, "Platform")
    admin = service.add_principal(db_session, platform.id, "admin", "hash")
    actor = platform_actor(platform, admin)
    l1 = service.create_child_account(db_session, actor, platform.id, AccountKind.RESELLER_L1, "L1")
    reseller_principal = service.add_principal(db_session, l1.id, "reseller", "hash")

    with pytest.raises(Forbidden):
        service.grant_platform_role(db_session, actor, reseller_principal.id, PlatformRole.OPERATOR)
```

- [ ] **Step 3: Run the tests and verify the missing-account-contract failure**

Run: `cd backend && uv run pytest tests/integration/accounts/test_hierarchy_and_roles.py -v`

Expected: FAIL during collection because `ip_saas.modules.accounts.context` is missing.

- [ ] **Step 4: Implement the frozen actor context**

Create `backend/src/ip_saas/modules/accounts/context.py`:

```python
from dataclasses import dataclass, field
from enum import StrEnum
from uuid import UUID, uuid4


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
    platform_roles: frozenset[str] = field(default_factory=frozenset)
```

- [ ] **Step 5: Implement account, principal, ownership, session, and platform-role tables**

Create `backend/src/ip_saas/modules/accounts/models.py`:

```python
from datetime import datetime
from enum import StrEnum
from uuid import UUID

from sqlalchemy import Boolean, CheckConstraint, ForeignKey, Index, String, UniqueConstraint
from sqlalchemy.orm import Mapped, mapped_column

from ip_saas.db.base import Base, TimestampMixin, UUIDPrimaryKeyMixin


class AccountKind(StrEnum):
    PLATFORM = "platform"
    RESELLER_L1 = "reseller_l1"
    RESELLER_L2 = "reseller_l2"
    C_USER = "c_user"


class AccountStatus(StrEnum):
    ACTIVE = "active"
    SUSPENDED = "suspended"


class PrincipalStatus(StrEnum):
    ACTIVE = "active"
    DISABLED = "disabled"


class PlatformRole(StrEnum):
    ADMIN = "admin"
    OPERATOR = "operator"
    REVIEWER = "reviewer"
    BUDGET_APPROVER = "budget_approver"


class Account(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "accounts"
    kind: Mapped[str] = mapped_column(String(32), index=True)
    display_name: Mapped[str] = mapped_column(String(120))
    status: Mapped[str] = mapped_column(String(20), default=AccountStatus.ACTIVE)


class ResellerRelation(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "reseller_relations"
    __table_args__ = (
        UniqueConstraint("child_account_id", name="uq_reseller_relation_child"),
        CheckConstraint("parent_account_id <> child_account_id", name="ck_reseller_not_self"),
    )
    parent_account_id: Mapped[UUID] = mapped_column(ForeignKey("accounts.id"), index=True)
    child_account_id: Mapped[UUID] = mapped_column(ForeignKey("accounts.id"), index=True)
    created_by_principal_id: Mapped[UUID]
    active: Mapped[bool] = mapped_column(Boolean, default=True)


class Principal(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "principals"
    __table_args__ = (UniqueConstraint("login_name", name="uq_principal_login"),)
    account_id: Mapped[UUID] = mapped_column(ForeignKey("accounts.id"), index=True)
    login_name: Mapped[str] = mapped_column(String(190))
    password_hash: Mapped[str] = mapped_column(String(255))
    status: Mapped[str] = mapped_column(String(20), default=PrincipalStatus.ACTIVE)


class AuthSession(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "auth_sessions"
    __table_args__ = (Index("ix_auth_session_refresh_hash", "refresh_token_hash", unique=True),)
    principal_id: Mapped[UUID] = mapped_column(ForeignKey("principals.id"), index=True)
    refresh_token_hash: Mapped[str] = mapped_column(String(64))
    expires_at: Mapped[datetime]
    revoked_at: Mapped[datetime | None]


class PlatformRoleGrant(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "platform_role_grants"
    __table_args__ = (UniqueConstraint("principal_id", "role", name="uq_platform_role_grant"),)
    principal_id: Mapped[UUID] = mapped_column(ForeignKey("principals.id"), index=True)
    role: Mapped[str] = mapped_column(String(32))
    granted_by_principal_id: Mapped[UUID]
    active: Mapped[bool] = mapped_column(Boolean, default=True)
```

- [ ] **Step 6: Implement fixed account-tree and platform-role rules**

Create `backend/src/ip_saas/modules/accounts/service.py` with these account methods; authentication methods are added in Task 4:

```python
from datetime import datetime
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.errors import Conflict, Forbidden, NotFound
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import (
    Account,
    AccountKind,
    PlatformRole,
    PlatformRoleGrant,
    Principal,
    ResellerRelation,
)


ALLOWED_CHILDREN = {
    AccountKind.PLATFORM: {AccountKind.RESELLER_L1},
    AccountKind.RESELLER_L1: {AccountKind.RESELLER_L2, AccountKind.C_USER},
    AccountKind.RESELLER_L2: {AccountKind.C_USER},
    AccountKind.C_USER: set(),
}


class AccountService:
    def bootstrap_platform(self, session: Session, display_name: str) -> Account:
        existing = session.scalar(select(Account).where(Account.kind == AccountKind.PLATFORM))
        if existing is not None:
            return existing
        account = Account(kind=AccountKind.PLATFORM, display_name=display_name)
        session.add(account)
        session.flush()
        return account

    def add_principal(
        self, session: Session, account_id: UUID, login_name: str, password_hash: str
    ) -> Principal:
        account = session.get(Account, account_id)
        if account is None:
            raise NotFound("account")
        if account.kind != AccountKind.PLATFORM:
            existing = session.scalar(select(Principal).where(Principal.account_id == account_id))
            if existing is not None:
                raise Conflict("team accounts are not enabled")
        principal = Principal(
            account_id=account_id,
            login_name=login_name.strip().lower(),
            password_hash=password_hash,
        )
        session.add(principal)
        session.flush()
        return principal

    def create_child_account(
        self,
        session: Session,
        actor: ActorContext,
        parent_account_id: UUID,
        child_kind: AccountKind,
        display_name: str,
    ) -> Account:
        parent = session.get(Account, parent_account_id)
        if parent is None:
            raise NotFound("parent account")
        if actor.account_id != parent.id or child_kind not in ALLOWED_CHILDREN[AccountKind(parent.kind)]:
            raise Forbidden("invalid account hierarchy")
        if parent.kind == AccountKind.PLATFORM and actor.kind != ActorKind.PLATFORM_ADMIN:
            raise Forbidden("platform admin is required")
        child = Account(kind=child_kind, display_name=display_name)
        session.add(child)
        session.flush()
        session.add(
            ResellerRelation(
                parent_account_id=parent.id,
                child_account_id=child.id,
                created_by_principal_id=actor.actor_id,
            )
        )
        session.flush()
        return child

    def grant_platform_role(
        self,
        session: Session,
        actor: ActorContext,
        principal_id: UUID,
        role: PlatformRole,
    ) -> PlatformRoleGrant:
        if actor.kind != ActorKind.PLATFORM_ADMIN:
            raise Forbidden("platform admin is required")
        principal = session.get(Principal, principal_id)
        if principal is None:
            raise NotFound("principal")
        account = session.get(Account, principal.account_id)
        if account is None or account.kind != AccountKind.PLATFORM:
            raise Forbidden("platform roles require a platform principal")
        grant = PlatformRoleGrant(
            principal_id=principal.id,
            role=role,
            granted_by_principal_id=actor.actor_id,
        )
        session.add(grant)
        session.flush()
        return grant
```

- [ ] **Step 7: Run hierarchy tests**

Run: `cd backend && uv run pytest tests/integration/accounts/test_hierarchy_and_roles.py -v`

Expected: PASS, 2 tests; the L2→L2 attempt raises `Forbidden` and the reseller role grant is rejected.

- [ ] **Step 8: Commit account ownership and platform roles**

```bash
git add backend/src/ip_saas/db/base.py backend/src/ip_saas/modules/accounts backend/tests/integration/accounts/test_hierarchy_and_roles.py
git commit -m "feat: enforce account hierarchy and platform roles"
```

### Task 4: Implement password authentication, refresh sessions, and frozen FastAPI dependencies

**Files:**
- Create: `backend/src/ip_saas/modules/accounts/security.py`
- Create: `backend/src/ip_saas/modules/accounts/dependencies.py`
- Create: `backend/src/ip_saas/modules/accounts/router.py`
- Create: `backend/tests/unit/accounts/test_security.py`
- Create: `backend/tests/integration/accounts/test_auth.py`
- Modify: `backend/src/ip_saas/common/errors.py`
- Modify: `backend/src/ip_saas/db/session.py`
- Modify: `backend/src/ip_saas/modules/accounts/service.py`
- Modify: `backend/src/ip_saas/api.py`

- [ ] **Step 1: Write the failing password and token unit tests**

Create `backend/tests/unit/accounts/test_security.py`:

```python
from datetime import UTC, datetime
from uuid import uuid4

import pytest

from ip_saas.common.errors import Unauthenticated
from ip_saas.modules.accounts.context import ActorKind
from ip_saas.modules.accounts.security import Passwords, TokenClaims, Tokens


def test_argon2_hash_never_contains_plaintext() -> None:
    passwords = Passwords()
    encoded = passwords.hash("correct horse battery staple")
    assert "correct horse" not in encoded
    assert passwords.verify(encoded, "correct horse battery staple") is True
    assert passwords.verify(encoded, "wrong") is False


def test_access_token_rejects_wrong_issuer() -> None:
    now = datetime(2026, 8, 24, 8, 0, tzinfo=UTC)
    tokens = Tokens(secret="a" * 32, issuer="ip-saas", access_minutes=15)
    claims = TokenClaims(uuid4(), uuid4(), uuid4(), ActorKind.C_USER, ())
    encoded = tokens.issue_access(claims, now)
    with pytest.raises(Unauthenticated):
        Tokens(secret="a" * 32, issuer="other", access_minutes=15).decode_access(encoded, now)


def test_refresh_hash_is_sha256() -> None:
    raw, digest = Tokens.new_refresh_token()
    assert raw != digest
    assert len(digest) == 64
```

- [ ] **Step 2: Run the unit tests and verify the missing-security failure**

Run: `cd backend && uv run pytest tests/unit/accounts/test_security.py -v`

Expected: FAIL during collection because `ip_saas.modules.accounts.security` is missing.

- [ ] **Step 3: Add the unauthenticated domain error**

Append to `backend/src/ip_saas/common/errors.py`:

```python
class Unauthenticated(DomainError):
    code = "unauthenticated"

    def __init__(self, message: str = "authentication required") -> None:
        super().__init__(message)
```

Add `Unauthenticated: 401` to `STATUS_BY_ERROR` in `backend/src/ip_saas/api.py` and import it beside the existing domain errors.

- [ ] **Step 4: Implement Argon2 password hashing and JWT/refresh primitives**

Create `backend/src/ip_saas/modules/accounts/security.py`:

```python
from dataclasses import dataclass
from datetime import datetime, timedelta
from hashlib import sha256
from secrets import token_urlsafe
from uuid import UUID

import jwt
from argon2 import PasswordHasher
from argon2.exceptions import VerifyMismatchError

from ip_saas.common.errors import Unauthenticated
from ip_saas.modules.accounts.context import ActorKind


class Passwords:
    def __init__(self) -> None:
        self._hasher = PasswordHasher()

    def hash(self, raw: str) -> str:
        if len(raw) < 12:
            raise ValueError("password must contain at least 12 characters")
        return self._hasher.hash(raw)

    def verify(self, encoded: str, raw: str) -> bool:
        try:
            return self._hasher.verify(encoded, raw)
        except VerifyMismatchError:
            return False


@dataclass(frozen=True)
class TokenClaims:
    principal_id: UUID
    session_id: UUID
    account_id: UUID
    actor_kind: ActorKind
    roles: tuple[str, ...]


class Tokens:
    def __init__(self, secret: str, issuer: str, access_minutes: int) -> None:
        self._secret = secret
        self._issuer = issuer
        self._access_minutes = access_minutes

    def issue_access(self, claims: TokenClaims, now: datetime) -> str:
        payload = {
            "sub": str(claims.principal_id),
            "sid": str(claims.session_id),
            "aid": str(claims.account_id),
            "kind": claims.actor_kind,
            "roles": list(claims.roles),
            "iss": self._issuer,
            "iat": now,
            "exp": now + timedelta(minutes=self._access_minutes),
        }
        return jwt.encode(payload, self._secret, algorithm="HS256")

    def decode_access(self, encoded: str, now: datetime) -> TokenClaims:
        try:
            payload = jwt.decode(
                encoded,
                self._secret,
                algorithms=["HS256"],
                issuer=self._issuer,
                options={
                    "require": ["sub", "sid", "aid", "kind", "exp", "iat"],
                    "verify_exp": False,
                },
            )
        except jwt.PyJWTError as error:
            raise Unauthenticated("invalid access token") from error
        if datetime.fromtimestamp(payload["exp"], tz=now.tzinfo) <= now:
            raise Unauthenticated("access token expired")
        return TokenClaims(
            principal_id=UUID(payload["sub"]),
            session_id=UUID(payload["sid"]),
            account_id=UUID(payload["aid"]),
            actor_kind=ActorKind(payload["kind"]),
            roles=tuple(payload.get("roles", [])),
        )

    @staticmethod
    def new_refresh_token() -> tuple[str, str]:
        raw = token_urlsafe(48)
        return raw, sha256(raw.encode()).hexdigest()

    @staticmethod
    def hash_refresh(raw: str) -> str:
        return sha256(raw.encode()).hexdigest()
```

- [ ] **Step 5: Run the security unit tests**

Run: `cd backend && uv run pytest tests/unit/accounts/test_security.py -v`

Expected: PASS, 3 tests.

- [ ] **Step 6: Write the failing authentication-service integration test**

Create `backend/tests/integration/accounts/test_auth.py`:

```python
from datetime import UTC, datetime

import pytest
from sqlalchemy.orm import Session

from ip_saas.common.errors import Unauthenticated
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import PlatformRole
from ip_saas.modules.accounts.security import Passwords, Tokens
from ip_saas.modules.accounts.service import AccountService, AuthService


class FixedClock:
    def now(self) -> datetime:
        return datetime(2026, 8, 24, 8, 0, tzinfo=UTC)


def test_login_creates_server_side_refresh_session(db_session: Session) -> None:
    accounts = AccountService()
    platform = accounts.bootstrap_platform(db_session, "Platform")
    principal = accounts.add_principal(
        db_session, platform.id, "admin@example.com", Passwords().hash("strong-password-123")
    )
    bootstrap_actor = ActorContext(
        principal.id,
        platform.id,
        ActorKind.PLATFORM_ADMIN,
        frozenset({PlatformRole.ADMIN}),
    )
    accounts.grant_platform_role(db_session, bootstrap_actor, principal.id, PlatformRole.ADMIN)
    auth = AuthService(Passwords(), Tokens("a" * 32, "ip-saas", 15), FixedClock(), 30)

    result = auth.login(db_session, "admin@example.com", "strong-password-123")

    assert result.actor.account_id == platform.id
    assert result.access_token
    assert result.refresh_token
    assert result.actor.kind.value == "platform_admin"


def test_wrong_password_does_not_create_session(db_session: Session) -> None:
    accounts = AccountService()
    platform = accounts.bootstrap_platform(db_session, "Platform")
    accounts.add_principal(
        db_session, platform.id, "admin@example.com", Passwords().hash("strong-password-123")
    )
    auth = AuthService(Passwords(), Tokens("a" * 32, "ip-saas", 15), FixedClock(), 30)
    with pytest.raises(Unauthenticated):
        auth.login(db_session, "admin@example.com", "wrong-password")


def test_refresh_rotates_once_and_logout_revokes_the_new_session(db_session: Session) -> None:
    accounts = AccountService()
    platform = accounts.bootstrap_platform(db_session, "Platform")
    principal = accounts.add_principal(
        db_session, platform.id, "refresh@example.com", Passwords().hash("strong-password-123")
    )
    accounts.grant_platform_role(
        db_session,
        ActorContext(principal.id, platform.id, ActorKind.PLATFORM_ADMIN,
                     frozenset({PlatformRole.ADMIN})),
        principal.id,
        PlatformRole.ADMIN,
    )
    auth = AuthService(Passwords(), Tokens("a" * 32, "ip-saas", 15), FixedClock(), 30)
    first = auth.login(db_session, principal.login_name, "strong-password-123")
    second = auth.refresh(db_session, first.refresh_token)
    assert second.refresh_token != first.refresh_token
    with pytest.raises(Unauthenticated, match="refresh session"):
        auth.refresh(db_session, first.refresh_token)
    auth.logout(db_session, second.refresh_token)
    with pytest.raises(Unauthenticated, match="session unavailable"):
        auth.actor_from_access(db_session, second.access_token)
```

- [ ] **Step 7: Run the integration test and verify `AuthService` is missing**

Run: `cd backend && uv run pytest tests/integration/accounts/test_auth.py -v`

Expected: FAIL during collection with `ImportError: cannot import name 'AuthService'`.

- [ ] **Step 8: Add account-to-actor resolution and authentication service**

Append the following definitions and methods to `backend/src/ip_saas/modules/accounts/service.py`:

```python
from dataclasses import dataclass
from datetime import timedelta

from ip_saas.common.clock import Clock
from ip_saas.common.errors import Unauthenticated
from ip_saas.modules.accounts.models import AccountStatus, AuthSession, PrincipalStatus
from ip_saas.modules.accounts.security import Passwords, TokenClaims, Tokens


ROLE_KIND = {
    PlatformRole.ADMIN: ActorKind.PLATFORM_ADMIN,
    PlatformRole.REVIEWER: ActorKind.PLATFORM_REVIEWER,
    PlatformRole.OPERATOR: ActorKind.PLATFORM_OPERATOR,
    PlatformRole.BUDGET_APPROVER: ActorKind.PLATFORM_OPERATOR,
}


def _non_platform_kind(account_kind: str) -> ActorKind:
    return ActorKind(account_kind)


@dataclass(frozen=True)
class LoginResult:
    access_token: str
    refresh_token: str
    actor: ActorContext


def _roles_for(session: Session, principal_id: UUID) -> frozenset[str]:
    values = session.scalars(
        select(PlatformRoleGrant.role).where(
            PlatformRoleGrant.principal_id == principal_id,
            PlatformRoleGrant.active.is_(True),
        )
    ).all()
    return frozenset(values)


def _actor_for(session: Session, principal: Principal) -> ActorContext:
    account = session.get(Account, principal.account_id)
    if account is None or account.status != AccountStatus.ACTIVE:
        raise Unauthenticated("account unavailable")
    roles = _roles_for(session, principal.id)
    if account.kind == AccountKind.PLATFORM:
        if PlatformRole.ADMIN in roles:
            kind = ActorKind.PLATFORM_ADMIN
        elif PlatformRole.REVIEWER in roles:
            kind = ActorKind.PLATFORM_REVIEWER
        else:
            kind = ActorKind.PLATFORM_OPERATOR
    else:
        kind = _non_platform_kind(account.kind)
    return ActorContext(principal.id, account.id, kind, roles)


class AuthService:
    def __init__(
        self,
        passwords: Passwords,
        tokens: Tokens,
        clock: Clock,
        refresh_days: int,
    ) -> None:
        self.passwords = passwords
        self.tokens = tokens
        self.clock = clock
        self.refresh_days = refresh_days

    def login(self, session: Session, login_name: str, password: str) -> LoginResult:
        principal = session.scalar(
            select(Principal).where(Principal.login_name == login_name.strip().lower())
        )
        if (
            principal is None
            or principal.status != PrincipalStatus.ACTIVE
            or not self.passwords.verify(principal.password_hash, password)
        ):
            raise Unauthenticated("invalid credentials")
        return self._issue_session(session, principal)

    def _issue_session(self, session: Session, principal: Principal) -> LoginResult:
        raw_refresh, refresh_hash = self.tokens.new_refresh_token()
        auth_session = AuthSession(
            principal_id=principal.id,
            refresh_token_hash=refresh_hash,
            expires_at=self.clock.now() + timedelta(days=self.refresh_days),
            revoked_at=None,
        )
        session.add(auth_session)
        session.flush()
        actor = _actor_for(session, principal)
        access = self.tokens.issue_access(
            TokenClaims(actor.actor_id, auth_session.id, actor.account_id, actor.kind, tuple(actor.platform_roles)),
            self.clock.now(),
        )
        return LoginResult(access, raw_refresh, actor)

    def refresh(self, session: Session, raw_refresh: str) -> LoginResult:
        now = self.clock.now()
        current = session.scalar(
            select(AuthSession)
            .where(
                AuthSession.refresh_token_hash == self.tokens.hash_refresh(raw_refresh),
                AuthSession.revoked_at.is_(None),
                AuthSession.expires_at > now,
            )
            .with_for_update()
        )
        # The row lock makes refresh rotation single-use even when two requests race.
        if current is None:
            raise Unauthenticated("refresh session unavailable")
        principal = session.get(Principal, current.principal_id)
        if principal is None or principal.status != PrincipalStatus.ACTIVE:
            raise Unauthenticated("refresh session unavailable")
        current.revoked_at = now
        session.flush()
        return self._issue_session(session, principal)

    def logout(self, session: Session, raw_refresh: str) -> None:
        current = session.scalar(
            select(AuthSession).where(
                AuthSession.refresh_token_hash == self.tokens.hash_refresh(raw_refresh)
            )
        )
        if current is not None and current.revoked_at is None:
            current.revoked_at = self.clock.now()
            session.flush()

    def actor_from_access(self, session: Session, encoded: str) -> ActorContext:
        claims = self.tokens.decode_access(encoded, self.clock.now())
        auth_session = session.get(AuthSession, claims.session_id)
        principal = session.get(Principal, claims.principal_id)
        if (
            auth_session is None
            or auth_session.revoked_at is not None
            or auth_session.expires_at <= self.clock.now()
            or principal is None
            or principal.status != PrincipalStatus.ACTIVE
        ):
            raise Unauthenticated("session unavailable")
        return _actor_for(session, principal)
```

- [ ] **Step 9: Run the authentication integration tests**

Run: `cd backend && uv run pytest tests/integration/accounts/test_auth.py -v`

Expected: PASS, 3 tests; a refresh token is single-use and logout revokes the replacement session.

- [ ] **Step 10: Add the frozen synchronous dependencies**

Append to `backend/src/ip_saas/db/session.py`:

```python
def get_session() -> Iterator[Session]:
    with session_scope() as session:
        yield session
```

Create `backend/src/ip_saas/modules/accounts/dependencies.py`:

```python
from typing import Annotated

from fastapi import Depends
from fastapi.security import HTTPAuthorizationCredentials, HTTPBearer
from sqlalchemy.orm import Session

from ip_saas.common.clock import SystemClock
from ip_saas.config import Settings, get_settings
from ip_saas.db.session import get_session
from ip_saas.common.errors import Forbidden
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.security import Passwords, Tokens
from ip_saas.modules.accounts.service import AuthService

bearer_scheme = HTTPBearer(auto_error=True)


def get_actor(
    credentials: Annotated[HTTPAuthorizationCredentials, Depends(bearer_scheme)],
    session: Annotated[Session, Depends(get_session)],
    settings: Annotated[Settings, Depends(get_settings)],
) -> ActorContext:
    service = AuthService(
        Passwords(),
        Tokens(
            settings.jwt_secret.get_secret_value(),
            settings.jwt_issuer,
            settings.access_token_minutes,
        ),
        SystemClock(),
        settings.refresh_token_days,
    )
    return service.actor_from_access(session, credentials.credentials)
```

- [ ] **Step 11: Add login, one-time refresh rotation, logout, and current-actor routes**

Create `backend/src/ip_saas/modules/accounts/router.py`:

```python
from typing import Annotated

from fastapi import APIRouter, Cookie, Depends, Response, status
from pydantic import BaseModel, ConfigDict
from sqlalchemy.orm import Session

from ip_saas.common.clock import SystemClock
from ip_saas.common.errors import Unauthenticated
from ip_saas.config import Settings, get_settings
from ip_saas.db.session import get_session
from ip_saas.modules.accounts.context import ActorContext
from ip_saas.modules.accounts.dependencies import get_actor
from ip_saas.modules.accounts.security import Passwords, Tokens
from ip_saas.modules.accounts.service import AuthService

router = APIRouter(prefix="/v1/auth", tags=["auth"])


class LoginRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    login_name: str
    password: str


class AccessResponse(BaseModel):
    access_token: str
    token_type: str = "bearer"


class ActorResponse(BaseModel):
    actor_id: str
    account_id: str
    kind: str
    platform_roles: list[str]


def _auth_service(settings: Settings) -> AuthService:
    return AuthService(
        Passwords(),
        Tokens(settings.jwt_secret.get_secret_value(), settings.jwt_issuer,
               settings.access_token_minutes),
        SystemClock(),
        settings.refresh_token_days,
    )


def _set_refresh_cookie(response: Response, token: str, settings: Settings) -> None:
    response.set_cookie(
        "refresh_token", token, httponly=True,
        secure=settings.app_env != "development", samesite="lax",
        # The browser reaches FastAPI through Next.js at /api/v1/auth.  A root path
        # keeps the cookie valid through that reverse proxy and for direct API tests.
        max_age=settings.refresh_token_days * 86400, path="/",
    )


@router.post("/login", response_model=AccessResponse)
def login(
    payload: LoginRequest,
    response: Response,
    session: Annotated[Session, Depends(get_session)],
    settings: Annotated[Settings, Depends(get_settings)],
) -> AccessResponse:
    service = _auth_service(settings)
    result = service.login(session, payload.login_name, payload.password)
    _set_refresh_cookie(response, result.refresh_token, settings)
    return AccessResponse(access_token=result.access_token)


@router.post("/refresh", response_model=AccessResponse)
def refresh(
    response: Response,
    session: Annotated[Session, Depends(get_session)],
    settings: Annotated[Settings, Depends(get_settings)],
    refresh_token: Annotated[str | None, Cookie()] = None,
) -> AccessResponse:
    if refresh_token is None:
        raise Unauthenticated("refresh session unavailable")
    result = _auth_service(settings).refresh(session, refresh_token)
    _set_refresh_cookie(response, result.refresh_token, settings)
    return AccessResponse(access_token=result.access_token)


@router.post("/logout", status_code=status.HTTP_204_NO_CONTENT)
def logout(
    response: Response,
    session: Annotated[Session, Depends(get_session)],
    settings: Annotated[Settings, Depends(get_settings)],
    refresh_token: Annotated[str | None, Cookie()] = None,
) -> None:
    if refresh_token is not None:
        _auth_service(settings).logout(session, refresh_token)
    response.delete_cookie("refresh_token", path="/")


@router.get("/me", response_model=ActorResponse)
def me(actor: Annotated[ActorContext, Depends(get_actor)]) -> ActorResponse:
    return ActorResponse(
        actor_id=str(actor.actor_id),
        account_id=str(actor.account_id),
        kind=actor.kind,
        platform_roles=sorted(actor.platform_roles),
    )
```

Import and include this router in `create_app()` in `backend/src/ip_saas/api.py`:

```python
from ip_saas.modules.accounts.router import router as accounts_router

# inside create_app(), after FastAPI construction
app.include_router(accounts_router)
```

- [ ] **Step 12: Run all authentication tests and the API schema smoke test**

Run: `cd backend && uv run pytest tests/unit/accounts/test_security.py tests/integration/accounts/test_auth.py tests/integration/test_health_and_migrations.py -v`

Expected: PASS, 8 tests; `create_app().openapi()` includes `/v1/auth/login`, `/v1/auth/refresh`, `/v1/auth/logout`, and `/v1/auth/me`.

- [ ] **Step 13: Commit authentication**

```bash
git add backend/src/ip_saas/common/errors.py backend/src/ip_saas/db/session.py backend/src/ip_saas/modules/accounts backend/src/ip_saas/api.py backend/tests/unit/accounts backend/tests/integration/accounts/test_auth.py
git commit -m "feat: add password and token authentication"
```

### Task 5: Enforce project ownership and tenant-isolated project access

**Files:**
- Create: `backend/src/ip_saas/modules/projects/models.py`
- Create: `backend/src/ip_saas/modules/projects/access.py`
- Create: `backend/src/ip_saas/modules/projects/service.py`
- Create: `backend/src/ip_saas/modules/projects/router.py`
- Create: `backend/tests/integration/projects/test_project_isolation.py`
- Create: `backend/tests/security/test_cross_tenant_access.py`
- Modify: `backend/src/ip_saas/api.py`

- [ ] **Step 1: Write the failing project ownership tests**

Create `backend/tests/integration/projects/test_project_isolation.py`:

```python
import pytest
from sqlalchemy.orm import Session

from ip_saas.common.errors import Forbidden, NotFound
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import Account, AccountKind, Principal
from ip_saas.modules.projects.access import ProjectAccessService
from ip_saas.modules.projects.models import ProjectOwnerType
from ip_saas.modules.projects.service import ProjectService


def add_account(session: Session, kind: AccountKind, name: str) -> tuple[Account, Principal]:
    account = Account(kind=kind, display_name=name)
    session.add(account)
    session.flush()
    principal = Principal(account_id=account.id, login_name=f"{name}@example.com", password_hash="hash")
    session.add(principal)
    session.flush()
    return account, principal


def actor(account: Account, principal: Principal, kind: ActorKind) -> ActorContext:
    return ActorContext(principal.id, account.id, kind)


def test_c_user_can_create_multiple_projects_but_only_view_own(db_session: Session) -> None:
    first, first_principal = add_account(db_session, AccountKind.C_USER, "first")
    second, second_principal = add_account(db_session, AccountKind.C_USER, "second")
    service = ProjectService(ProjectAccessService())
    first_actor = actor(first, first_principal, ActorKind.C_USER)
    project_a = service.create(db_session, first_actor, "Gold gifts")
    project_b = service.create(db_session, first_actor, "Fruit")

    assert {project_a.owner_type, project_b.owner_type} == {ProjectOwnerType.C_USER}
    with pytest.raises(NotFound):
        ProjectAccessService().require_viewer(
            db_session, actor(second, second_principal, ActorKind.C_USER), project_a.id
        )


def test_platform_project_cannot_be_created_by_c_user(db_session: Session) -> None:
    account, principal = add_account(db_session, AccountKind.C_USER, "customer")
    with pytest.raises(Forbidden):
        ProjectService(ProjectAccessService()).create_platform_project(
            db_session, actor(account, principal, ActorKind.C_USER), "Self marketing"
        )
```

- [ ] **Step 2: Run the project tests and verify the missing-model failure**

Run: `cd backend && uv run pytest tests/integration/projects/test_project_isolation.py -v`

Expected: FAIL during collection because `ip_saas.modules.projects.models` is missing.

- [ ] **Step 3: Implement immutable project ownership fields**

Create `backend/src/ip_saas/modules/projects/models.py`:

```python
from enum import StrEnum
from uuid import UUID

from sqlalchemy import CheckConstraint, ForeignKey, String
from sqlalchemy.orm import Mapped, mapped_column

from ip_saas.db.base import Base, TimestampMixin, UUIDPrimaryKeyMixin


class ProjectOwnerType(StrEnum):
    C_USER = "c_user"
    PLATFORM = "platform"


class ProjectStatus(StrEnum):
    ONBOARDING = "onboarding"
    NEEDS_INFORMATION = "needs_information"
    AWAITING_STRATEGY = "awaiting_strategy"
    ACTIVE = "active"
    REPOSITIONING = "repositioning"
    PAUSED = "paused"
    ARCHIVED = "archived"


class MarketingRisk(StrEnum):
    RESTRAINED = "restrained"
    BALANCED = "balanced"
    AGGRESSIVE = "aggressive"


class IPProject(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "ip_projects"
    __table_args__ = (
        CheckConstraint("owner_type in ('c_user', 'platform')", name="ck_project_owner_type"),
    )
    owner_type: Mapped[str] = mapped_column(String(20), index=True)
    owner_account_id: Mapped[UUID] = mapped_column(ForeignKey("accounts.id"), index=True)
    name: Mapped[str] = mapped_column(String(160))
    status: Mapped[str] = mapped_column(String(32), default=ProjectStatus.ONBOARDING)
    marketing_risk: Mapped[str] = mapped_column(String(20), default=MarketingRisk.BALANCED)
    created_by_principal_id: Mapped[UUID]
```

- [ ] **Step 4: Implement the frozen project access signatures**

Create `backend/src/ip_saas/modules/projects/access.py`:

```python
from uuid import UUID

from sqlalchemy.orm import Session

from ip_saas.common.errors import Forbidden, NotFound
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.projects.models import IPProject, ProjectOwnerType, ProjectStatus


class ProjectAccessService:
    def require_viewer(
        self, session: Session, actor: ActorContext, project_id: UUID
    ) -> IPProject:
        project = session.get(IPProject, project_id)
        if project is None:
            raise NotFound("project")
        if project.owner_account_id != actor.account_id:
            raise NotFound("project")
        if project.owner_type == ProjectOwnerType.C_USER and actor.kind != ActorKind.C_USER:
            raise NotFound("project")
        if project.owner_type == ProjectOwnerType.PLATFORM and actor.kind not in {
            ActorKind.PLATFORM_ADMIN,
            ActorKind.PLATFORM_OPERATOR,
            ActorKind.PLATFORM_REVIEWER,
        }:
            raise NotFound("project")
        return project

    def require_editor(
        self, session: Session, actor: ActorContext, project_id: UUID
    ) -> IPProject:
        project = self.require_viewer(session, actor, project_id)
        if project.status == ProjectStatus.ARCHIVED:
            raise Forbidden("archived project is read-only")
        if actor.kind == ActorKind.PLATFORM_REVIEWER:
            raise Forbidden("platform reviewer is read-only")
        return project
```

- [ ] **Step 5: Implement project creation and listing through explicit owner filters**

Create `backend/src/ip_saas/modules/projects/service.py`:

```python
from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.errors import Forbidden
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import Account, AccountKind
from ip_saas.modules.projects.access import ProjectAccessService
from ip_saas.modules.projects.models import IPProject, MarketingRisk, ProjectOwnerType


class ProjectService:
    def __init__(self, access: ProjectAccessService) -> None:
        self.access = access

    def create(
        self,
        session: Session,
        actor: ActorContext,
        name: str,
        marketing_risk: MarketingRisk = MarketingRisk.BALANCED,
    ) -> IPProject:
        account = session.get(Account, actor.account_id)
        if account is None or account.kind != AccountKind.C_USER or actor.kind != ActorKind.C_USER:
            raise Forbidden("C-user account is required")
        return self._insert(session, actor, name, ProjectOwnerType.C_USER, marketing_risk)

    def create_platform_project(
        self,
        session: Session,
        actor: ActorContext,
        name: str,
        marketing_risk: MarketingRisk = MarketingRisk.BALANCED,
    ) -> IPProject:
        if actor.kind not in {ActorKind.PLATFORM_ADMIN, ActorKind.PLATFORM_OPERATOR}:
            raise Forbidden("platform operator is required")
        return self._insert(session, actor, name, ProjectOwnerType.PLATFORM, marketing_risk)

    def _insert(
        self,
        session: Session,
        actor: ActorContext,
        name: str,
        owner_type: ProjectOwnerType,
        marketing_risk: MarketingRisk,
    ) -> IPProject:
        project = IPProject(
            owner_type=owner_type,
            owner_account_id=actor.account_id,
            name=name.strip(),
            marketing_risk=marketing_risk,
            created_by_principal_id=actor.actor_id,
        )
        session.add(project)
        session.flush()
        return project

    def list_owned(self, session: Session, actor: ActorContext) -> list[IPProject]:
        owner_type = (
            ProjectOwnerType.C_USER if actor.kind == ActorKind.C_USER else ProjectOwnerType.PLATFORM
        )
        return list(
            session.scalars(
                select(IPProject)
                .where(
                    IPProject.owner_account_id == actor.account_id,
                    IPProject.owner_type == owner_type,
                )
                .order_by(IPProject.created_at.desc())
            )
        )
```

- [ ] **Step 6: Run project isolation tests**

Run: `cd backend && uv run pytest tests/integration/projects/test_project_isolation.py -v`

Expected: PASS, 2 tests.

- [ ] **Step 7: Write the cross-role tenant-security matrix**

Create `backend/tests/security/test_cross_tenant_access.py`:

```python
import pytest
from sqlalchemy.orm import Session

from ip_saas.common.errors import NotFound
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import Account, AccountKind, Principal
from ip_saas.modules.projects.access import ProjectAccessService
from ip_saas.modules.projects.service import ProjectService


@pytest.mark.integration
@pytest.mark.parametrize(
    "intruder_kind,intruder_account_kind",
    [
        (ActorKind.RESELLER_L1, AccountKind.RESELLER_L1),
        (ActorKind.RESELLER_L2, AccountKind.RESELLER_L2),
        (ActorKind.C_USER, AccountKind.C_USER),
        (ActorKind.PLATFORM_ADMIN, AccountKind.PLATFORM),
        (ActorKind.PLATFORM_OPERATOR, AccountKind.PLATFORM),
    ],
)
def test_object_id_never_bypasses_owner_boundary(
    db_session: Session,
    intruder_kind: ActorKind,
    intruder_account_kind: AccountKind,
) -> None:
    owner_account = Account(kind=AccountKind.C_USER, display_name="owner")
    intruder_account = Account(kind=intruder_account_kind, display_name="intruder")
    db_session.add_all([owner_account, intruder_account])
    db_session.flush()
    owner = Principal(account_id=owner_account.id, login_name="owner@example.com", password_hash="hash")
    intruder = Principal(
        account_id=intruder_account.id,
        login_name=f"{intruder_kind}@example.com",
        password_hash="hash",
    )
    db_session.add_all([owner, intruder])
    db_session.flush()
    project = ProjectService(ProjectAccessService()).create(
        db_session,
        ActorContext(owner.id, owner_account.id, ActorKind.C_USER),
        "private",
    )

    with pytest.raises(NotFound):
        ProjectAccessService().require_viewer(
            db_session,
            ActorContext(intruder.id, intruder_account.id, intruder_kind),
            project.id,
        )
```

- [ ] **Step 8: Run the security matrix**

Run: `cd backend && uv run pytest tests/security/test_cross_tenant_access.py -v -m integration`

Expected: PASS, 5 parametrized cases; all unauthorized requests are indistinguishable from a missing project.

- [ ] **Step 9: Add project API schemas and synchronous routes**

Create `backend/src/ip_saas/modules/projects/router.py`:

```python
from typing import Annotated
from uuid import UUID

from fastapi import APIRouter, Depends
from pydantic import BaseModel, ConfigDict
from sqlalchemy.orm import Session

from ip_saas.db.session import get_session
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.dependencies import get_actor
from ip_saas.modules.projects.access import ProjectAccessService
from ip_saas.modules.projects.models import IPProject, MarketingRisk
from ip_saas.modules.projects.service import ProjectService

router = APIRouter(prefix="/v1/projects", tags=["projects"])


class ProjectCreate(BaseModel):
    model_config = ConfigDict(extra="forbid")
    name: str
    marketing_risk: MarketingRisk = MarketingRisk.BALANCED


class ProjectResponse(BaseModel):
    id: UUID
    name: str
    owner_type: str
    status: str
    marketing_risk: str


def present(project: IPProject) -> ProjectResponse:
    return ProjectResponse(
        id=project.id,
        name=project.name,
        owner_type=project.owner_type,
        status=project.status,
        marketing_risk=project.marketing_risk,
    )


@router.get("", response_model=list[ProjectResponse])
def list_projects(
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> list[ProjectResponse]:
    return [present(item) for item in ProjectService(ProjectAccessService()).list_owned(session, actor)]


@router.post("", response_model=ProjectResponse, status_code=201)
def create_project(
    payload: ProjectCreate,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> ProjectResponse:
    service = ProjectService(ProjectAccessService())
    project = (
        service.create(session, actor, payload.name, payload.marketing_risk)
        if actor.kind == ActorKind.C_USER
        else service.create_platform_project(session, actor, payload.name, payload.marketing_risk)
    )
    return present(project)
```

Import and include `projects.router` in `create_app()` immediately after the auth router.

- [ ] **Step 10: Run project, tenant, and API smoke tests**

Run: `cd backend && uv run pytest tests/integration/projects tests/security/test_cross_tenant_access.py tests/integration/test_health_and_migrations.py -v`

Expected: PASS, 9 tests; `/v1/projects` appears in the generated OpenAPI document.

- [ ] **Step 11: Commit tenant-isolated projects**

```bash
git add backend/src/ip_saas/modules/projects backend/tests/integration/projects backend/tests/security/test_cross_tenant_access.py backend/src/ip_saas/api.py
git commit -m "feat: isolate customer and platform projects"
```

### Task 6: Add append-only audit records and a transactional outbox

**Files:**
- Create: `backend/src/ip_saas/common/audit.py`
- Create: `backend/src/ip_saas/common/outbox.py`
- Create: `backend/src/ip_saas/providers/rocketmq.py`
- Create: `backend/src/ip_saas/workers/outbox_dispatcher.py`
- Create: `backend/tests/integration/common/test_audit_outbox.py`
- Create: `backend/tests/providers/test_rocketmq_contract.py`
- Create: `backend/tests/contract/test_event_contracts.py`
- Create: `contracts/events/event-envelope.v1.json`

- [ ] **Step 1: Write failing audit/outbox atomicity tests**

Create `backend/tests/integration/common/test_audit_outbox.py`:

```python
from datetime import UTC, datetime
from uuid import uuid4

import pytest
from sqlalchemy import func, select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditEvent, AuditWriter
from ip_saas.common.outbox import EventEnvelope, OutboxEvent, OutboxWriter
from ip_saas.modules.accounts.context import ActorContext, ActorKind


class FixedClock:
    def now(self) -> datetime:
        return datetime(2026, 8, 24, 8, 0, tzinfo=UTC)


def test_audit_and_outbox_rollback_with_business_transaction(db_session: Session) -> None:
    actor = ActorContext(uuid4(), uuid4(), ActorKind.C_USER)
    try:
        with db_session.begin_nested():
            AuditWriter(FixedClock()).write(
                db_session,
                actor=actor,
                action="project.created",
                target_type="ip_project",
                target_id=uuid4(),
                metadata={"source": "api"},
            )
            OutboxWriter(FixedClock()).add(
                db_session,
                EventEnvelope(
                    event_id=uuid4(),
                    event_type="generation.task.requested",
                    schema_version=1,
                    aggregate_id=uuid4(),
                    occurred_at=FixedClock().now(),
                    initiated_by_actor_id=actor.actor_id,
                    idempotency_key="task-1",
                    payload={"capability": "seedream"},
                ),
            )
            raise RuntimeError("force rollback")
    except RuntimeError:
        pass
    assert db_session.scalar(select(func.count()).select_from(AuditEvent)) == 0
    assert db_session.scalar(select(func.count()).select_from(OutboxEvent)) == 0


def test_outbox_event_id_is_idempotent(db_session: Session) -> None:
    event_id = uuid4()
    writer = OutboxWriter(FixedClock())
    envelope = EventEnvelope(
        event_id=event_id,
        event_type="generation.task.requested",
        schema_version=1,
        aggregate_id=uuid4(),
        occurred_at=FixedClock().now(),
        initiated_by_actor_id=uuid4(),
        idempotency_key="task-2",
        payload={},
    )
    writer.add(db_session, envelope)
    with pytest.raises(IntegrityError):
        writer.add(db_session, envelope)


def test_published_payload_cannot_be_rewritten(db_session: Session) -> None:
    record = OutboxWriter(FixedClock()).add(
        db_session,
        EventEnvelope(
            event_id=uuid4(), event_type="generation.task.requested", schema_version=1,
            aggregate_id=uuid4(), occurred_at=FixedClock().now(),
            initiated_by_actor_id=uuid4(), idempotency_key="task-immutable",
            payload={"capability": "seedream"},
        ),
    )
    record.payload = {"capability": "different"}
    with pytest.raises(RuntimeError, match="payload is immutable"):
        db_session.flush()
```

- [ ] **Step 2: Run the integration tests and verify missing contracts**

Run: `cd backend && uv run pytest tests/integration/common/test_audit_outbox.py -v`

Expected: FAIL during collection because `ip_saas.common.audit` and `ip_saas.common.outbox` are missing.

- [ ] **Step 3: Implement the frozen audit writer**

Create `backend/src/ip_saas/common/audit.py`:

```python
from collections.abc import Mapping
from typing import TypeAlias
from uuid import UUID

from sqlalchemy import JSON, String, event
from sqlalchemy.orm import Mapped, Session, mapped_column

from ip_saas.common.clock import Clock
from ip_saas.db.base import Base, TimestampMixin, UUIDPrimaryKeyMixin
from ip_saas.modules.accounts.context import ActorContext

JSONScalar: TypeAlias = str | int | float | bool | None
JSONValue: TypeAlias = JSONScalar | list["JSONValue"] | dict[str, "JSONValue"]


class AuditEvent(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "audit_events"
    actor_id: Mapped[UUID]
    actor_account_id: Mapped[UUID]
    actor_kind: Mapped[str] = mapped_column(String(32))
    action: Mapped[str] = mapped_column(String(120), index=True)
    target_type: Mapped[str] = mapped_column(String(80))
    target_id: Mapped[UUID] = mapped_column(index=True)
    project_id: Mapped[UUID | None] = mapped_column(index=True)
    metadata_json: Mapped[dict[str, JSONValue]] = mapped_column("metadata", JSON)


@event.listens_for(AuditEvent, "before_update")
@event.listens_for(AuditEvent, "before_delete")
def _audit_is_append_only(*_args: object) -> None:
    raise RuntimeError("audit events are append-only")


class AuditWriter:
    def __init__(self, clock: Clock) -> None:
        self.clock = clock

    def write(
        self,
        session: Session,
        *,
        actor: ActorContext,
        action: str,
        target_type: str,
        target_id: UUID,
        project_id: UUID | None = None,
        metadata: Mapping[str, JSONValue] | None = None,
    ) -> AuditEvent:
        item = AuditEvent(
            actor_id=actor.actor_id,
            actor_account_id=actor.account_id,
            actor_kind=actor.kind,
            action=action,
            target_type=target_type,
            target_id=target_id,
            project_id=project_id,
            metadata_json=dict(metadata or {}),
            created_at=self.clock.now(),
        )
        session.add(item)
        session.flush()
        return item
```

- [ ] **Step 4: Implement the frozen event envelope and outbox writer**

Create `backend/src/ip_saas/common/outbox.py`:

```python
from datetime import datetime
from enum import StrEnum
from typing import Any, Literal
from uuid import UUID

from pydantic import BaseModel, ConfigDict
from sqlalchemy import JSON, String, UniqueConstraint, event, inspect
from sqlalchemy.orm import Mapped, Session, mapped_column

from ip_saas.common.clock import Clock
from ip_saas.db.base import Base, TimestampMixin, UUIDPrimaryKeyMixin


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


class OutboxStatus(StrEnum):
    PENDING = "pending"
    PUBLISHED = "published"
    FAILED = "failed"


class OutboxEvent(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "outbox_events"
    __table_args__ = (UniqueConstraint("event_type", "idempotency_key", name="uq_outbox_idempotency"),)
    event_type: Mapped[str] = mapped_column(String(160), index=True)
    schema_version: Mapped[int]
    aggregate_id: Mapped[UUID] = mapped_column(index=True)
    occurred_at: Mapped[datetime]
    initiated_by_actor_id: Mapped[UUID]
    idempotency_key: Mapped[str] = mapped_column(String(160))
    payload: Mapped[dict[str, Any]] = mapped_column(JSON)
    status: Mapped[str] = mapped_column(String(20), default=OutboxStatus.PENDING, index=True)
    attempt_count: Mapped[int] = mapped_column(default=0)
    next_attempt_at: Mapped[datetime | None] = mapped_column(index=True)
    published_at: Mapped[datetime | None]
    published_message_id: Mapped[str | None] = mapped_column(String(160))
    last_error: Mapped[str | None] = mapped_column(String(500))


@event.listens_for(OutboxEvent, "before_delete")
def _outbox_is_not_deleted(*_args: object) -> None:
    raise RuntimeError("outbox events cannot be deleted")


@event.listens_for(OutboxEvent, "before_update")
def _outbox_payload_is_immutable(_mapper: object, _connection: object, target: OutboxEvent) -> None:
    state = inspect(target)
    immutable_fields = (
        "event_type", "schema_version", "aggregate_id", "occurred_at",
        "initiated_by_actor_id", "idempotency_key", "payload", "created_at",
    )
    if any(state.attrs[name].history.has_changes() for name in immutable_fields):
        raise RuntimeError("outbox event payload is immutable")


class OutboxWriter:
    def __init__(self, clock: Clock) -> None:
        self.clock = clock

    def add(self, session: Session, event: EventEnvelope) -> OutboxEvent:
        serialized = event.model_dump(mode="json")
        record = OutboxEvent(
            id=event.event_id,
            event_type=event.event_type,
            schema_version=event.schema_version,
            aggregate_id=event.aggregate_id,
            occurred_at=event.occurred_at,
            initiated_by_actor_id=event.initiated_by_actor_id,
            idempotency_key=event.idempotency_key,
            payload=serialized["payload"],
            created_at=self.clock.now(),
            next_attempt_at=None,
            published_at=None,
            published_message_id=None,
            last_error=None,
        )
        session.add(record)
        session.flush()
        return record
```

- [ ] **Step 5: Run audit/outbox tests**

Run: `cd backend && uv run pytest tests/integration/common/test_audit_outbox.py -v`

Expected: PASS, 3 tests; lifecycle fields may advance but the emitted envelope cannot be rewritten.

- [ ] **Step 6: Write failing dispatcher retry and dead-letter tests**

Append to `backend/tests/integration/common/test_audit_outbox.py`:

```python
from ip_saas.common.outbox import OutboxStatus
from ip_saas.workers.outbox_dispatcher import dispatch_batch


class MemoryPublisher:
    def __init__(self) -> None:
        self.events: list[EventEnvelope] = []

    def publish(self, envelope: EventEnvelope) -> str:
        self.events.append(envelope)
        return "mq-message-1"


class BrokenPublisher:
    def publish(self, _envelope: EventEnvelope) -> str:
        raise RuntimeError("broker token=must-not-leak")


def pending_event(writer: OutboxWriter, session: Session, key: str) -> OutboxEvent:
    return writer.add(session, EventEnvelope(
        event_id=uuid4(), event_type="generation.task.requested", schema_version=1,
        aggregate_id=uuid4(), occurred_at=FixedClock().now(),
        initiated_by_actor_id=uuid4(), idempotency_key=key, payload={"task_id": key},
    ))


def test_dispatch_records_the_broker_message_id(db_session: Session) -> None:
    row = pending_event(OutboxWriter(FixedClock()), db_session, "dispatch-success")
    publisher = MemoryPublisher()
    assert dispatch_batch(db_session, publisher, FixedClock()) == 1
    assert row.status == OutboxStatus.PUBLISHED
    assert row.published_message_id == "mq-message-1"
    assert [item.event_id for item in publisher.events] == [row.id]


def test_dispatch_failure_is_sanitized_and_dead_letters_at_the_cap(
    db_session: Session,
) -> None:
    row = pending_event(OutboxWriter(FixedClock()), db_session, "dispatch-failure")
    assert dispatch_batch(
        db_session, BrokenPublisher(), FixedClock(), max_attempts=1
    ) == 1
    assert row.status == OutboxStatus.FAILED
    assert row.attempt_count == 1
    assert row.next_attempt_at is None
    assert row.last_error == "broker token=[redacted]"
```

Run: `cd backend && uv run pytest tests/integration/common/test_audit_outbox.py -q`

Expected: FAIL because `dispatch_batch` does not yet persist broker receipts or dead-letter failures.

- [ ] **Step 7: Add the retry-safe outbox dispatcher port and one-batch worker**

Create `backend/src/ip_saas/workers/outbox_dispatcher.py`:

```python
import re
from datetime import timedelta
from typing import Protocol

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.clock import Clock
from ip_saas.common.outbox import EventEnvelope, OutboxEvent, OutboxStatus


class EventPublisher(Protocol):
    def publish(self, envelope: EventEnvelope) -> str: ...


def _safe_error(error: Exception) -> str:
    value = " ".join(str(error).split())
    value = re.sub(r"(?i)(password|secret|token)=\S+", r"\1=[redacted]", value)
    return value[:500]


def dispatch_batch(
    session: Session,
    publisher: EventPublisher,
    clock: Clock,
    batch_size: int = 50,
    max_attempts: int = 8,
) -> int:
    now = clock.now()
    records = list(
        session.scalars(
            select(OutboxEvent)
            .where(
                OutboxEvent.status == OutboxStatus.PENDING,
                (OutboxEvent.next_attempt_at.is_(None))
                | (OutboxEvent.next_attempt_at <= now),
            )
            .order_by(OutboxEvent.created_at)
            .limit(batch_size)
            .with_for_update(skip_locked=True)
        )
    )
    for record in records:
        try:
            message_id = publisher.publish(
                EventEnvelope(
                    event_id=record.id,
                    event_type=record.event_type,
                    schema_version=1,
                    aggregate_id=record.aggregate_id,
                    occurred_at=record.occurred_at,
                    initiated_by_actor_id=record.initiated_by_actor_id,
                    idempotency_key=record.idempotency_key,
                    payload=record.payload,
                )
            )
        except Exception as error:
            record.attempt_count += 1
            record.last_error = _safe_error(error)
            if record.attempt_count >= max_attempts:
                record.status = OutboxStatus.FAILED
                record.next_attempt_at = None
            else:
                delay_seconds = min(2 ** record.attempt_count, 300)
                record.next_attempt_at = now + timedelta(seconds=delay_seconds)
            continue
        record.status = OutboxStatus.PUBLISHED
        record.attempt_count += 1
        record.next_attempt_at = None
        record.published_at = now
        record.published_message_id = message_id
        record.last_error = None
    session.flush()
    return len(records)
```

- [ ] **Step 8: Write the failing RocketMQ adapter contract test**

Create `backend/tests/providers/test_rocketmq_contract.py`:

```python
import json
from datetime import UTC, datetime
from types import SimpleNamespace
from uuid import UUID

from ip_saas.common.outbox import EventEnvelope
from ip_saas.providers.rocketmq import RocketMQPublisher


class FakeMessage:
    def __init__(self) -> None:
        self.topic = ""
        self.body = b""
        self.tag = ""
        self.keys = ""
        self.properties: dict[str, str] = {}

    def add_property(self, key: str, value: str) -> None:
        self.properties[key] = value


class FakeProducer:
    def __init__(self) -> None:
        self.messages: list[FakeMessage] = []

    def send(self, message: FakeMessage) -> SimpleNamespace:
        self.messages.append(message)
        return SimpleNamespace(message_id="broker-id-77")


def test_rocketmq_publisher_preserves_the_frozen_envelope() -> None:
    producer = FakeProducer()
    publisher = RocketMQPublisher(
        producer=producer, topic="ip-saas-generation", message_factory=FakeMessage
    )
    envelope = EventEnvelope(
        event_id=UUID(int=71), event_type="generation.task.requested", schema_version=1,
        aggregate_id=UUID(int=72), occurred_at=datetime(2026, 8, 24, tzinfo=UTC),
        initiated_by_actor_id=UUID(int=73), idempotency_key="task:72", payload={"task_id": "72"},
    )
    assert publisher.publish(envelope) == "broker-id-77"
    message = producer.messages[0]
    body = json.loads(message.body)
    assert message.topic == "ip-saas-generation"
    assert message.tag == "generation.task.requested"
    assert message.keys == str(envelope.event_id)
    assert message.properties == {"schema_version": "1", "idempotency_key": "task:72"}
    assert body == envelope.model_dump(mode="json")
```

Run: `cd backend && uv run pytest tests/providers/test_rocketmq_contract.py -q`

Expected: FAIL because `ip_saas.providers.rocketmq` does not exist; the test never opens a socket or reads credentials.

- [ ] **Step 9: Implement the official RocketMQ 5.x producer adapter**

Create `backend/src/ip_saas/providers/rocketmq.py`:

```python
from __future__ import annotations

import json
from collections.abc import Callable
from typing import Any, Protocol

from rocketmq import ClientConfiguration, Credentials, Message, Producer  # type: ignore[import-untyped]

from ip_saas.common.outbox import EventEnvelope
from ip_saas.config import Settings


class ProducerPort(Protocol):
    def send(self, message: Any) -> Any: ...


class RocketMQPublisher:
    def __init__(
        self,
        *,
        producer: ProducerPort,
        topic: str,
        message_factory: Callable[[], Any] = Message,
    ) -> None:
        self._producer = producer
        self._topic = topic
        self._message_factory = message_factory

    def publish(self, envelope: EventEnvelope) -> str:
        message = self._message_factory()
        message.topic = self._topic
        message.body = json.dumps(
            envelope.model_dump(mode="json"),
            ensure_ascii=False,
            allow_nan=False,
            separators=(",", ":"),
            sort_keys=True,
        ).encode("utf-8")
        message.tag = envelope.event_type
        message.keys = str(envelope.event_id)
        message.add_property("schema_version", str(envelope.schema_version))
        message.add_property("idempotency_key", envelope.idempotency_key)
        receipt = self._producer.send(message)
        message_id = str(receipt.message_id).strip()
        if not message_id:
            raise RuntimeError("RocketMQ returned an empty message ID")
        return message_id


def rocketmq_configuration(settings: Settings) -> ClientConfiguration:
    access_key = settings.rocketmq_access_key.get_secret_value()
    secret_key = settings.rocketmq_secret_key.get_secret_value()
    if bool(access_key) != bool(secret_key):
        raise ValueError("RocketMQ access and secret keys must be supplied together")
    credentials = Credentials(access_key, secret_key) if access_key else Credentials()
    return ClientConfiguration(settings.rocketmq_endpoint, credentials)


def connect_rocketmq(settings: Settings) -> tuple[RocketMQPublisher, Producer]:
    configuration = rocketmq_configuration(settings)
    producer = Producer(configuration, (settings.rocketmq_topic,))
    producer.startup()
    return RocketMQPublisher(producer=producer, topic=settings.rocketmq_topic), producer
```

The worker composition root calls `connect_rocketmq()` once, keeps the returned producer alive, and calls `producer.shutdown()` during graceful termination. It never creates a producer per event. The local endpoint is the RocketMQ Proxy (`:8081`), not the NameServer (`:9876`). Managed Volcengine endpoint, namespace, AK/SK, topic, ACL, TLS, and network reachability are verified again in the Plan 06 production PoC before any public launch.

Run: `cd backend && uv run pytest tests/providers/test_rocketmq_contract.py tests/integration/common/test_audit_outbox.py -q && uv run mypy src/ip_saas/providers/rocketmq.py src/ip_saas/workers/outbox_dispatcher.py`

Expected: PASS; the fake proves byte-for-byte JSON-envelope preservation and the dispatcher proves broker failures retry with bounded backoff before entering `failed` dead-letter state.

- [ ] **Step 10: Write and export the event schema contract**

Create `backend/tests/contract/test_event_contracts.py`:

```python
import json
from pathlib import Path

import pytest

from ip_saas.common.outbox import EventEnvelope


@pytest.mark.contract
def test_event_envelope_schema_has_no_drift() -> None:
    expected = EventEnvelope.model_json_schema()
    path = Path(__file__).parents[3] / "contracts/events/event-envelope.v1.json"
    assert json.loads(path.read_text()) == expected
```

Generate `contracts/events/event-envelope.v1.json` once:

```bash
cd backend
uv run python -c 'import json,pathlib; from ip_saas.common.outbox import EventEnvelope; p=pathlib.Path("../contracts/events/event-envelope.v1.json"); p.parent.mkdir(parents=True,exist_ok=True); p.write_text(json.dumps(EventEnvelope.model_json_schema(),ensure_ascii=False,indent=2)+"\n")'
```

Expected: the JSON file is created and contains `schema_version`, `aggregate_id`, `initiated_by_actor_id`, and `idempotency_key` as required fields.

- [ ] **Step 11: Run integration and event contract tests**

Run: `cd backend && uv run pytest tests/integration/common/test_audit_outbox.py tests/providers/test_rocketmq_contract.py tests/contract/test_event_contracts.py -v`

Expected: PASS, 7 tests; broker delivery records a message ID, bounded failures enter dead-letter state, and no fake test opens a network connection.

- [ ] **Step 12: Commit audit and outbox contracts**

```bash
git add backend/src/ip_saas/common/audit.py backend/src/ip_saas/common/outbox.py backend/src/ip_saas/providers/rocketmq.py backend/src/ip_saas/workers/outbox_dispatcher.py backend/tests/integration/common backend/tests/providers/test_rocketmq_contract.py backend/tests/contract/test_event_contracts.py contracts/events/event-envelope.v1.json
git commit -m "feat: add audit and transactional outbox"
```

### Task 7: Define balanced credit postings, pricing versions, wallets, and generation holds

**Files:**
- Create: `backend/src/ip_saas/modules/billing/domain.py`
- Create: `backend/src/ip_saas/modules/billing/models.py`
- Create: `backend/tests/unit/billing/test_credit_planner.py`

- [ ] **Step 1: Write failing pure credit-planner tests**

Create `backend/tests/unit/billing/test_credit_planner.py`:

```python
from uuid import uuid4

import pytest

from ip_saas.common.errors import Conflict
from ip_saas.modules.billing.domain import CreditPosting, balanced_transfer, reverse_postings


def test_transfer_is_integer_and_balanced() -> None:
    source, destination = uuid4(), uuid4()
    postings = balanced_transfer(source, destination, 1250)
    assert postings == (
        CreditPosting(source, -1250),
        CreditPosting(destination, 1250),
    )
    assert sum(item.amount_units for item in postings) == 0


@pytest.mark.parametrize("amount", [0, -1])
def test_transfer_rejects_non_positive_amount(amount: int) -> None:
    with pytest.raises(Conflict):
        balanced_transfer(uuid4(), uuid4(), amount)


def test_reversal_negates_each_original_posting() -> None:
    postings = (CreditPosting(uuid4(), -80), CreditPosting(uuid4(), 80))
    reversed_postings = reverse_postings(postings)
    assert [item.amount_units for item in reversed_postings] == [80, -80]
    assert sum(item.amount_units for item in reversed_postings) == 0
```

- [ ] **Step 2: Run the planner tests and verify the missing-domain failure**

Run: `cd backend && uv run pytest tests/unit/billing/test_credit_planner.py -v`

Expected: FAIL during collection because `ip_saas.modules.billing.domain` is missing.

- [ ] **Step 3: Implement the pure balanced-posting planner**

Create `backend/src/ip_saas/modules/billing/domain.py`:

```python
from dataclasses import dataclass
from uuid import UUID

from ip_saas.common.errors import Conflict


@dataclass(frozen=True)
class CreditPosting:
    wallet_id: UUID
    amount_units: int


def balanced_transfer(source_wallet_id: UUID, destination_wallet_id: UUID, amount: int) -> tuple[CreditPosting, ...]:
    if amount <= 0:
        raise Conflict("credit amount must be positive")
    if source_wallet_id == destination_wallet_id:
        raise Conflict("source and destination wallets must differ")
    return (
        CreditPosting(source_wallet_id, -amount),
        CreditPosting(destination_wallet_id, amount),
    )


def reverse_postings(postings: tuple[CreditPosting, ...]) -> tuple[CreditPosting, ...]:
    if not postings or sum(item.amount_units for item in postings) != 0:
        raise Conflict("only a balanced transaction can be reversed")
    return tuple(CreditPosting(item.wallet_id, -item.amount_units) for item in postings)
```

- [ ] **Step 4: Run the planner tests**

Run: `cd backend && uv run pytest tests/unit/billing/test_credit_planner.py -v`

Expected: PASS, 4 tests.

- [ ] **Step 5: Define credit, price, hold, internal-cost, and provider-cost tables**

Create `backend/src/ip_saas/modules/billing/models.py`:

```python
from datetime import datetime
from decimal import Decimal
from enum import StrEnum
from uuid import UUID

from sqlalchemy import (
    BigInteger,
    Boolean,
    CheckConstraint,
    ForeignKey,
    Index,
    JSON,
    Numeric,
    String,
    UniqueConstraint,
    event,
)
from sqlalchemy.orm import Mapped, mapped_column

from ip_saas.db.base import Base, TimestampMixin, UUIDPrimaryKeyMixin


class CreditTransactionKind(StrEnum):
    ISSUE = "issue"
    TRANSFER = "transfer"
    CONSUME = "consume"
    REVERSAL = "reversal"
    ADJUSTMENT = "adjustment"


class HoldStatus(StrEnum):
    ACTIVE = "active"
    SETTLED = "settled"
    RELEASED = "released"


class PricingUnit(StrEnum):
    CREDIT = "credit"
    CNY_FEN = "cny_fen"


class InternalCostKind(StrEnum):
    ALLOCATION = "allocation"
    PROVIDER_SPEND = "provider_spend"
    REVERSAL = "reversal"
    ADJUSTMENT = "adjustment"


class ReconciliationStatus(StrEnum):
    UNRECONCILED = "unreconciled"
    MATCHED = "matched"
    DISPUTED = "disputed"


class CreditWallet(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "credit_wallets"
    __table_args__ = (
        CheckConstraint(
            "(account_id is not null and system_code is null) or "
            "(account_id is null and system_code is not null)",
            name="ck_wallet_single_owner",
        ),
        Index("uq_credit_wallet_account", "account_id", unique=True),
        Index("uq_credit_wallet_system", "system_code", unique=True),
    )
    account_id: Mapped[UUID | None] = mapped_column(ForeignKey("accounts.id"), index=True)
    system_code: Mapped[str | None] = mapped_column(String(40))
    allow_negative: Mapped[bool] = mapped_column(Boolean, default=False)
    posted_balance: Mapped[int] = mapped_column(BigInteger, default=0)


class CreditTransaction(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "credit_transactions"
    __table_args__ = (UniqueConstraint("idempotency_key", name="uq_credit_tx_idempotency"),)
    kind: Mapped[str] = mapped_column(String(24), index=True)
    idempotency_key: Mapped[str] = mapped_column(String(160))
    created_by_principal_id: Mapped[UUID]
    reversal_of_id: Mapped[UUID | None] = mapped_column(ForeignKey("credit_transactions.id"))
    metadata_json: Mapped[dict[str, str | int | bool | None]] = mapped_column("metadata", JSON)


class CreditLedgerEntry(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "credit_ledger_entries"
    __table_args__ = (CheckConstraint("amount_units <> 0", name="ck_credit_entry_nonzero"),)
    transaction_id: Mapped[UUID] = mapped_column(ForeignKey("credit_transactions.id"), index=True)
    wallet_id: Mapped[UUID] = mapped_column(ForeignKey("credit_wallets.id"), index=True)
    amount_units: Mapped[int] = mapped_column(BigInteger)


class PricingVersion(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "pricing_versions"
    __table_args__ = (UniqueConstraint("code", "version_no", name="uq_pricing_code_version"),)
    code: Mapped[str] = mapped_column(String(80), index=True)
    version_no: Mapped[int]
    unit: Mapped[str] = mapped_column(String(20))
    unit_amount: Mapped[int] = mapped_column(BigInteger)
    effective_at: Mapped[datetime]
    retired_at: Mapped[datetime | None]
    created_by_principal_id: Mapped[UUID]


class GenerationHold(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "generation_holds"
    __table_args__ = (UniqueConstraint("idempotency_key", name="uq_generation_hold_idempotency"),)
    wallet_id: Mapped[UUID] = mapped_column(ForeignKey("credit_wallets.id"), index=True)
    initiated_by_actor_id: Mapped[UUID]
    pricing_version_id: Mapped[UUID | None] = mapped_column(ForeignKey("pricing_versions.id"))
    amount_units: Mapped[int] = mapped_column(BigInteger)
    settled_units: Mapped[int | None] = mapped_column(BigInteger)
    status: Mapped[str] = mapped_column(String(20), default=HoldStatus.ACTIVE, index=True)
    idempotency_key: Mapped[str] = mapped_column(String(160))
    settled_transaction_id: Mapped[UUID | None] = mapped_column(ForeignKey("credit_transactions.id"))
    release_reason: Mapped[str | None] = mapped_column(String(240))


class InternalCostCenter(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "internal_cost_centers"
    __table_args__ = (UniqueConstraint("platform_account_id", "code", name="uq_cost_center_code"),)
    platform_account_id: Mapped[UUID] = mapped_column(ForeignKey("accounts.id"), index=True)
    code: Mapped[str] = mapped_column(String(80))
    display_name: Mapped[str] = mapped_column(String(120))
    active: Mapped[bool] = mapped_column(Boolean, default=True)


class InternalCostEntry(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "internal_cost_entries"
    __table_args__ = (
        UniqueConstraint("idempotency_key", name="uq_internal_cost_idempotency"),
        CheckConstraint("amount_fen <> 0", name="ck_internal_cost_nonzero"),
    )
    cost_center_id: Mapped[UUID] = mapped_column(ForeignKey("internal_cost_centers.id"), index=True)
    kind: Mapped[str] = mapped_column(String(24))
    amount_fen: Mapped[int] = mapped_column(BigInteger)
    idempotency_key: Mapped[str] = mapped_column(String(160))
    created_by_principal_id: Mapped[UUID]
    reversal_of_id: Mapped[UUID | None] = mapped_column(ForeignKey("internal_cost_entries.id"))


class InternalBudgetHold(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "internal_budget_holds"
    __table_args__ = (UniqueConstraint("idempotency_key", name="uq_internal_hold_idempotency"),)
    cost_center_id: Mapped[UUID] = mapped_column(ForeignKey("internal_cost_centers.id"), index=True)
    initiated_by_actor_id: Mapped[UUID]
    pricing_version_id: Mapped[UUID | None] = mapped_column(ForeignKey("pricing_versions.id"))
    amount_fen: Mapped[int] = mapped_column(BigInteger)
    settled_fen: Mapped[int | None] = mapped_column(BigInteger)
    status: Mapped[str] = mapped_column(String(20), default=HoldStatus.ACTIVE, index=True)
    idempotency_key: Mapped[str] = mapped_column(String(160))
    release_reason: Mapped[str | None] = mapped_column(String(240))


class ProviderCostEntry(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "provider_cost_entries"
    __table_args__ = (
        UniqueConstraint("idempotency_key", name="uq_provider_cost_idempotency"),
        UniqueConstraint(
            "provider",
            "provider_request_id",
            name="uq_provider_cost_request_identity",
        ),
    )
    task_id: Mapped[UUID | None] = mapped_column(ForeignKey("task_records.id"), index=True)
    internal_cost_center_id: Mapped[UUID | None] = mapped_column(
        ForeignKey("internal_cost_centers.id"), index=True
    )
    provider: Mapped[str] = mapped_column(String(60))
    provider_request_id: Mapped[str | None] = mapped_column(String(200))
    capability: Mapped[str] = mapped_column(String(80))
    model_id: Mapped[str] = mapped_column(String(160))
    model_version: Mapped[str] = mapped_column(String(80))
    native_quantity: Mapped[Decimal] = mapped_column(Numeric(24, 8))
    native_unit: Mapped[str] = mapped_column(String(40))
    supplier_amount_minor: Mapped[int] = mapped_column(BigInteger)
    supplier_currency: Mapped[str] = mapped_column(String(3))
    amount_fen: Mapped[int] = mapped_column(BigInteger)
    reconciliation_status: Mapped[str] = mapped_column(String(24))
    idempotency_key: Mapped[str] = mapped_column(String(160))


for append_only_class in (CreditLedgerEntry, InternalCostEntry, ProviderCostEntry):
    event.listen(
        append_only_class,
        "before_update",
        lambda *_args: (_ for _ in ()).throw(RuntimeError("ledger records are append-only")),
    )
    event.listen(
        append_only_class,
        "before_delete",
        lambda *_args: (_ for _ in ()).throw(RuntimeError("ledger records are append-only")),
    )
```

- [ ] **Step 6: Add table-constraint smoke assertions**

Append to `backend/tests/unit/billing/test_credit_planner.py`:

```python
from ip_saas.modules.billing.models import CreditWallet, GenerationHold, InternalBudgetHold


def test_foundation_billing_tables_use_distinct_hold_models() -> None:
    assert CreditWallet.__tablename__ == "credit_wallets"
    assert GenerationHold.__tablename__ == "generation_holds"
    assert InternalBudgetHold.__tablename__ == "internal_budget_holds"
```

- [ ] **Step 7: Run billing domain tests**

Run: `cd backend && uv run pytest tests/unit/billing/test_credit_planner.py -v`

Expected: PASS, 5 tests.

- [ ] **Step 8: Commit billing primitives**

```bash
git add backend/src/ip_saas/modules/billing/domain.py backend/src/ip_saas/modules/billing/models.py backend/tests/unit/billing/test_credit_planner.py
git commit -m "feat: define trusted billing records"
```

### Task 8: Implement PostgreSQL credit posting, transfer, hold, settlement, and reversal

**Files:**
- Create: `backend/src/ip_saas/modules/billing/repository.py`
- Create: `backend/tests/integration/billing/test_credit_ledger.py`
- Create: `backend/tests/integration/billing/test_credit_concurrency.py`
- Modify: `backend/src/ip_saas/modules/billing/service.py`

- [ ] **Step 1: Write failing credit-ledger integration tests**

Create `backend/tests/integration/billing/test_credit_ledger.py`:

```python
from uuid import uuid4

import pytest
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from ip_saas.common.errors import Conflict
from ip_saas.modules.accounts.models import Account, AccountKind
from ip_saas.modules.billing.models import CreditLedgerEntry, HoldStatus
from ip_saas.modules.billing.repository import CreditLedgerRepository
from ip_saas.modules.billing.service import CreditLedgerService


def setup_wallets(session: Session) -> tuple[CreditLedgerService, object, object, object]:
    account = Account(kind=AccountKind.C_USER, display_name="Customer")
    session.add(account)
    session.flush()
    service = CreditLedgerService(CreditLedgerRepository())
    treasury = service.ensure_system_wallet(session, "credit_treasury", allow_negative=True)
    revenue = service.ensure_system_wallet(session, "generation_revenue", allow_negative=False)
    customer = service.ensure_account_wallet(session, account.id)
    return service, treasury, revenue, customer


def test_hold_reduces_available_balance_and_settlement_releases_difference(
    db_session: Session,
) -> None:
    service, treasury, revenue, customer = setup_wallets(db_session)
    service.issue(db_session, treasury.id, customer.id, 1000, uuid4(), "issue-1")
    hold = service.reserve(db_session, customer.id, 800, "hold-1", uuid4())

    assert service.available_balance(db_session, customer.id) == 200
    with pytest.raises(Conflict):
        service.transfer(db_session, customer.id, revenue.id, 201, uuid4(), "transfer-too-much")

    service.settle(db_session, hold.id, revenue.id, 600, uuid4(), "settle-1")
    assert hold.status == HoldStatus.SETTLED
    assert customer.posted_balance == 400
    assert service.available_balance(db_session, customer.id) == 400


def test_duplicate_idempotency_key_never_posts_twice(db_session: Session) -> None:
    service, treasury, _revenue, customer = setup_wallets(db_session)
    first = service.issue(db_session, treasury.id, customer.id, 500, uuid4(), "issue-once")
    second = service.issue(db_session, treasury.id, customer.id, 500, uuid4(), "issue-once")
    assert first.id == second.id
    assert customer.posted_balance == 500
    assert db_session.scalar(select(func.sum(CreditLedgerEntry.amount_units))) == 0


def test_reversal_is_new_balanced_transaction(db_session: Session) -> None:
    service, treasury, _revenue, customer = setup_wallets(db_session)
    original = service.issue(db_session, treasury.id, customer.id, 300, uuid4(), "issue-reverse")
    reversal = service.reverse(db_session, original.id, uuid4(), "reverse-once")
    assert reversal.reversal_of_id == original.id
    assert customer.posted_balance == 0
    assert service.reconstruct_balance(db_session, customer.id) == 0
```

- [ ] **Step 2: Run the integration tests and verify missing repository/service behavior**

Run: `cd backend && uv run pytest tests/integration/billing/test_credit_ledger.py -v`

Expected: FAIL during collection because `CreditLedgerRepository` or `CreditLedgerService` is missing.

- [ ] **Step 3: Implement row locking, available-balance calculation, and append-only posting**

Create `backend/src/ip_saas/modules/billing/repository.py`:

```python
from collections.abc import Iterable
from uuid import UUID

from sqlalchemy import func, select
from sqlalchemy.orm import Session

from ip_saas.common.errors import Conflict, NotFound
from ip_saas.modules.billing.domain import CreditPosting
from ip_saas.modules.billing.models import (
    CreditLedgerEntry,
    CreditTransaction,
    CreditTransactionKind,
    CreditWallet,
    GenerationHold,
    HoldStatus,
)


class CreditLedgerRepository:
    def lock_wallets(self, session: Session, wallet_ids: Iterable[UUID]) -> dict[UUID, CreditWallet]:
        ordered = sorted(set(wallet_ids), key=str)
        wallets = list(
            session.scalars(
                select(CreditWallet)
                .where(CreditWallet.id.in_(ordered))
                .order_by(CreditWallet.id)
                .with_for_update()
            )
        )
        if len(wallets) != len(ordered):
            raise NotFound("wallet")
        return {wallet.id: wallet for wallet in wallets}

    def active_holds(self, session: Session, wallet_id: UUID) -> int:
        return int(
            session.scalar(
                select(func.coalesce(func.sum(GenerationHold.amount_units), 0)).where(
                    GenerationHold.wallet_id == wallet_id,
                    GenerationHold.status == HoldStatus.ACTIVE,
                )
            )
            or 0
        )

    def post(
        self,
        session: Session,
        *,
        kind: CreditTransactionKind,
        postings: tuple[CreditPosting, ...],
        actor_id: UUID,
        idempotency_key: str,
        reversal_of_id: UUID | None = None,
    ) -> CreditTransaction:
        existing = session.scalar(
            select(CreditTransaction).where(CreditTransaction.idempotency_key == idempotency_key)
        )
        if existing is not None:
            return existing
        if not postings or sum(item.amount_units for item in postings) != 0:
            raise Conflict("credit transaction must balance to zero")
        wallets = self.lock_wallets(session, (item.wallet_id for item in postings))
        for posting in postings:
            wallet = wallets[posting.wallet_id]
            next_balance = wallet.posted_balance + posting.amount_units
            if not wallet.allow_negative and next_balance - self.active_holds(session, wallet.id) < 0:
                raise Conflict("insufficient available credits")
        transaction = CreditTransaction(
            kind=kind,
            idempotency_key=idempotency_key,
            created_by_principal_id=actor_id,
            reversal_of_id=reversal_of_id,
            metadata_json={},
        )
        session.add(transaction)
        session.flush()
        for posting in postings:
            wallet = wallets[posting.wallet_id]
            wallet.posted_balance += posting.amount_units
            session.add(
                CreditLedgerEntry(
                    transaction_id=transaction.id,
                    wallet_id=wallet.id,
                    amount_units=posting.amount_units,
                )
            )
        session.flush()
        return transaction
```

- [ ] **Step 4: Implement the credit ledger application service**

Create `backend/src/ip_saas/modules/billing/service.py` initially with the following credit service; Task 10 adds the frozen cross-ledger `BillingService` below it:

```python
from uuid import UUID

from sqlalchemy import func, select
from sqlalchemy.orm import Session

from ip_saas.common.errors import Conflict, NotFound
from ip_saas.modules.billing.domain import CreditPosting, balanced_transfer, reverse_postings
from ip_saas.modules.billing.models import (
    CreditLedgerEntry,
    CreditTransaction,
    CreditTransactionKind,
    CreditWallet,
    GenerationHold,
    HoldStatus,
)
from ip_saas.modules.billing.repository import CreditLedgerRepository


class CreditLedgerService:
    def __init__(self, repository: CreditLedgerRepository) -> None:
        self.repository = repository

    def ensure_system_wallet(
        self, session: Session, code: str, *, allow_negative: bool
    ) -> CreditWallet:
        existing = session.scalar(select(CreditWallet).where(CreditWallet.system_code == code))
        if existing is not None:
            return existing
        wallet = CreditWallet(
            account_id=None,
            system_code=code,
            allow_negative=allow_negative,
            posted_balance=0,
        )
        session.add(wallet)
        session.flush()
        return wallet

    def ensure_account_wallet(self, session: Session, account_id: UUID) -> CreditWallet:
        existing = session.scalar(select(CreditWallet).where(CreditWallet.account_id == account_id))
        if existing is not None:
            return existing
        wallet = CreditWallet(
            account_id=account_id,
            system_code=None,
            allow_negative=False,
            posted_balance=0,
        )
        session.add(wallet)
        session.flush()
        return wallet

    def available_balance(self, session: Session, wallet_id: UUID) -> int:
        wallet = session.get(CreditWallet, wallet_id)
        if wallet is None:
            raise NotFound("wallet")
        return wallet.posted_balance - self.repository.active_holds(session, wallet_id)

    def issue(
        self,
        session: Session,
        treasury_wallet_id: UUID,
        recipient_wallet_id: UUID,
        units: int,
        actor_id: UUID,
        idempotency_key: str,
    ) -> CreditTransaction:
        return self.repository.post(
            session,
            kind=CreditTransactionKind.ISSUE,
            postings=balanced_transfer(treasury_wallet_id, recipient_wallet_id, units),
            actor_id=actor_id,
            idempotency_key=idempotency_key,
        )

    def transfer(
        self,
        session: Session,
        source_wallet_id: UUID,
        destination_wallet_id: UUID,
        units: int,
        actor_id: UUID,
        idempotency_key: str,
    ) -> CreditTransaction:
        return self.repository.post(
            session,
            kind=CreditTransactionKind.TRANSFER,
            postings=balanced_transfer(source_wallet_id, destination_wallet_id, units),
            actor_id=actor_id,
            idempotency_key=idempotency_key,
        )

    def reserve(
        self,
        session: Session,
        wallet_id: UUID,
        units: int,
        idempotency_key: str,
        initiated_by_actor_id: UUID,
    ) -> GenerationHold:
        existing = session.scalar(
            select(GenerationHold).where(GenerationHold.idempotency_key == idempotency_key)
        )
        if existing is not None:
            return existing
        if units <= 0:
            raise Conflict("hold amount must be positive")
        wallet = self.repository.lock_wallets(session, [wallet_id])[wallet_id]
        if wallet.posted_balance - self.repository.active_holds(session, wallet_id) < units:
            raise Conflict("insufficient available credits")
        hold = GenerationHold(
            wallet_id=wallet.id,
            initiated_by_actor_id=initiated_by_actor_id,
            pricing_version_id=None,
            amount_units=units,
            settled_units=None,
            idempotency_key=idempotency_key,
            settled_transaction_id=None,
            release_reason=None,
        )
        session.add(hold)
        session.flush()
        return hold

    def settle(
        self,
        session: Session,
        hold_id: UUID,
        revenue_wallet_id: UUID,
        actual_units: int,
        actor_id: UUID,
        idempotency_key: str,
    ) -> CreditTransaction:
        hold = session.scalar(
            select(GenerationHold).where(GenerationHold.id == hold_id).with_for_update()
        )
        if hold is None:
            raise NotFound("generation hold")
        if hold.status == HoldStatus.SETTLED and hold.settled_transaction_id is not None:
            transaction = session.get(CreditTransaction, hold.settled_transaction_id)
            if transaction is None:
                raise Conflict("settled hold has no transaction")
            return transaction
        if hold.status != HoldStatus.ACTIVE or actual_units < 0 or actual_units > hold.amount_units:
            raise Conflict("hold cannot be settled with this amount")
        hold.status = HoldStatus.SETTLED
        hold.settled_units = actual_units
        session.flush()
        if actual_units == 0:
            raise Conflict("zero-cost completion must release the hold")
        transaction = self.repository.post(
            session,
            kind=CreditTransactionKind.CONSUME,
            postings=balanced_transfer(hold.wallet_id, revenue_wallet_id, actual_units),
            actor_id=actor_id,
            idempotency_key=idempotency_key,
        )
        hold.settled_transaction_id = transaction.id
        session.flush()
        return transaction

    def release(self, session: Session, hold_id: UUID, reason: str) -> GenerationHold:
        hold = session.scalar(
            select(GenerationHold).where(GenerationHold.id == hold_id).with_for_update()
        )
        if hold is None:
            raise NotFound("generation hold")
        if hold.status in {HoldStatus.RELEASED, HoldStatus.SETTLED}:
            return hold
        hold.status = HoldStatus.RELEASED
        hold.release_reason = reason
        session.flush()
        return hold

    def reverse(
        self,
        session: Session,
        transaction_id: UUID,
        actor_id: UUID,
        idempotency_key: str,
    ) -> CreditTransaction:
        original = session.get(CreditTransaction, transaction_id)
        if original is None:
            raise NotFound("credit transaction")
        entries = tuple(
            CreditPosting(item.wallet_id, item.amount_units)
            for item in session.scalars(
                select(CreditLedgerEntry).where(CreditLedgerEntry.transaction_id == transaction_id)
            )
        )
        return self.repository.post(
            session,
            kind=CreditTransactionKind.REVERSAL,
            postings=reverse_postings(entries),
            actor_id=actor_id,
            idempotency_key=idempotency_key,
            reversal_of_id=original.id,
        )

    def reconstruct_balance(self, session: Session, wallet_id: UUID) -> int:
        return int(
            session.scalar(
                select(func.coalesce(func.sum(CreditLedgerEntry.amount_units), 0)).where(
                    CreditLedgerEntry.wallet_id == wallet_id
                )
            )
            or 0
        )
```

- [ ] **Step 5: Run credit ledger integration tests**

Run: `cd backend && uv run pytest tests/integration/billing/test_credit_ledger.py -v`

Expected: PASS, 3 tests; cached balances equal reconstructed entry sums and duplicate keys do not post twice.

- [ ] **Step 6: Write the failing simultaneous-hold test**

Create `backend/tests/integration/billing/test_credit_concurrency.py`:

```python
from concurrent.futures import ThreadPoolExecutor
from threading import Barrier
from uuid import uuid4

import pytest
from sqlalchemy import delete
from sqlalchemy.engine import Engine
from sqlalchemy.orm import Session, sessionmaker

from ip_saas.common.errors import Conflict
from ip_saas.db.base import Base
from ip_saas.modules.accounts.models import Account, AccountKind
from ip_saas.modules.billing.models import CreditWallet, GenerationHold
from ip_saas.modules.billing.repository import CreditLedgerRepository
from ip_saas.modules.billing.service import CreditLedgerService


@pytest.mark.integration
def test_two_concurrent_holds_cannot_overspend(pg_engine: Engine) -> None:
    Base.metadata.create_all(pg_engine)
    factory = sessionmaker(bind=pg_engine, class_=Session, expire_on_commit=False)
    with factory.begin() as session:
        account = Account(kind=AccountKind.C_USER, display_name=f"concurrent-{uuid4()}")
        session.add(account)
        session.flush()
        wallet = CreditWallet(
            account_id=account.id,
            system_code=None,
            allow_negative=False,
            posted_balance=1000,
        )
        session.add(wallet)
        session.flush()
        wallet_id = wallet.id

    barrier = Barrier(2)

    def reserve(key: str) -> str:
        try:
            with factory.begin() as session:
                barrier.wait()
                CreditLedgerService(CreditLedgerRepository()).reserve(
                    session, wallet_id, 800, key, uuid4()
                )
            return "reserved"
        except Conflict:
            return "rejected"

    with ThreadPoolExecutor(max_workers=2) as executor:
        outcomes = sorted(executor.map(reserve, ["concurrent-a", "concurrent-b"]))

    assert outcomes == ["rejected", "reserved"]
    with factory.begin() as session:
        session.execute(delete(GenerationHold).where(GenerationHold.wallet_id == wallet_id))
        session.execute(delete(CreditWallet).where(CreditWallet.id == wallet_id))
        session.execute(delete(Account).where(Account.id == account.id))
```

- [ ] **Step 7: Temporarily remove `with_for_update()` and verify the concurrency test fails**

Change the wallet query in `CreditLedgerRepository.lock_wallets()` to omit `.with_for_update()`, then run:

`cd backend && uv run pytest tests/integration/billing/test_credit_concurrency.py -v`

Expected: FAIL because both concurrent calls can report `reserved`, proving the test detects overspending. Restore `.with_for_update()` immediately after observing the failure.

- [ ] **Step 8: Run the concurrency test with row locking restored**

Run: `cd backend && uv run pytest tests/integration/billing/test_credit_concurrency.py -v`

Expected: PASS; exactly one hold is reserved and one is rejected.

- [ ] **Step 9: Run all credit tests**

Run: `cd backend && uv run pytest tests/unit/billing/test_credit_planner.py tests/integration/billing/test_credit_ledger.py tests/integration/billing/test_credit_concurrency.py -v`

Expected: PASS, 9 tests.

- [ ] **Step 10: Commit the PostgreSQL credit ledger**

```bash
git add backend/src/ip_saas/modules/billing/repository.py backend/src/ip_saas/modules/billing/service.py backend/tests/integration/billing
git commit -m "feat: implement atomic credit ledger"
```

### Task 9: Implement the separate internal-CNY ledger and freeze BillingService ports

**Files:**
- Create: `backend/src/ip_saas/modules/billing/ports.py`
- Create: `backend/src/ip_saas/modules/billing/limits.py`
- Create: `backend/src/ip_saas/modules/billing/adapters.py`
- Create: `backend/tests/support/__init__.py`
- Create: `backend/tests/support/generation_limits.py`
- Create: `backend/tests/integration/billing/test_internal_costs.py`
- Modify: `backend/src/ip_saas/modules/billing/models.py`
- Modify: `backend/src/ip_saas/modules/billing/repository.py`
- Modify: `backend/src/ip_saas/modules/billing/service.py`

- [ ] **Step 1: Write the failing internal-budget and billing-context tests**

Create `backend/tests/integration/billing/test_internal_costs.py`:

```python
from dataclasses import replace
from decimal import Decimal
from types import SimpleNamespace
from typing import cast
from uuid import UUID, uuid4

import pytest
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from ip_saas.common.errors import Conflict
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import Account, AccountKind
from ip_saas.modules.billing.models import (
    GenerationHold,
    HoldStatus,
    InternalBudgetHold,
    InternalCostEntry,
    InternalCostKind,
    ProviderCostEntry,
)
from ip_saas.modules.billing.adapters import SqlCreditHoldPort, SqlInternalBudgetHoldPort
from ip_saas.modules.billing.limits import UnconfiguredGenerationLimits
from ip_saas.modules.billing.service import (
    BillingContext,
    BillingMode,
    BillingService,
    InternalBudgetService,
    ProviderCostInput,
)
from tests.support.generation_limits import AllowAllGenerationLimits


def real_provider_cost(
    *, task_id: UUID | None, provider_request_id: str | None
) -> ProviderCostInput:
    return ProviderCostInput(
        provider="volcengine",
        capability="text_generation",
        model_id="doubao-test",
        model_version="test-v1",
        native_quantity=Decimal("1"),
        native_unit="request",
        supplier_amount_minor=1,
        supplier_currency="CNY",
        amount_fen=1,
        reconciliation_status="matched",
        task_id=task_id,
        provider_request_id=provider_request_id,
    )


def test_unconfigured_generation_limits_fail_before_any_hold(
    db_session: Session,
) -> None:
    actor = ActorContext(uuid4(), uuid4(), ActorKind.C_USER)
    billing = BillingService(
        SqlCreditHoldPort(),
        SqlInternalBudgetHoldPort(),
        UnconfiguredGenerationLimits(),
    )
    with pytest.raises(Conflict, match="generation limit is not configured"):
        billing.reserve_customer_generation(
            db_session, actor.account_id, 1, "must-fail-closed", actor
        )
    assert db_session.scalar(select(func.count()).select_from(GenerationHold)) == 0


def test_internal_budget_hold_never_uses_credit_wallet(db_session: Session) -> None:
    platform = Account(kind=AccountKind.PLATFORM, display_name="Platform")
    db_session.add(platform)
    db_session.flush()
    actor = ActorContext(uuid4(), platform.id, ActorKind.PLATFORM_ADMIN)
    internal = InternalBudgetService()
    center = internal.create_center(db_session, platform.id, "self_marketing", "Self marketing")
    internal.allocate(db_session, center.id, 10_000, actor.actor_id, "allocation-1")
    billing = BillingService(
        SqlCreditHoldPort(),
        SqlInternalBudgetHoldPort(internal),
        AllowAllGenerationLimits(),
    )

    context = billing.reserve_internal_generation(
        db_session, center.id, 8_000, "internal-hold-1", actor
    )
    with pytest.raises(Conflict):
        billing.reserve_internal_generation(db_session, center.id, 3_000, "too-much", actor)
    internal.settle(db_session, context.hold_id, 6_000, "internal-settle-1")

    assert context.mode == BillingMode.INTERNAL_COST
    assert internal.available_fen(db_session, center.id) == 4_000
    assert db_session.scalar(select(func.count()).select_from(ProviderCostEntry)) == 0
    assert db_session.scalar(
        select(func.sum(InternalCostEntry.amount_fen)).where(
            InternalCostEntry.cost_center_id == center.id
        )
    ) == 4_000


def test_billing_context_rejects_zero_hold_id() -> None:
    with pytest.raises(ValueError):
        BillingContext(BillingMode.CUSTOMER_CREDIT, UUID(int=0))


def test_proven_no_provider_call_releases_internal_hold_without_cost_row(
    db_session: Session,
) -> None:
    platform = Account(kind=AccountKind.PLATFORM, display_name="Platform zero")
    db_session.add(platform)
    db_session.flush()
    actor = ActorContext(uuid4(), platform.id, ActorKind.PLATFORM_ADMIN)
    internal = InternalBudgetService()
    center = internal.create_center(db_session, platform.id, "zero", "Zero-cost test")
    internal.allocate(db_session, center.id, 1_000, actor.actor_id, "zero-allocation")
    billing = BillingService(
        SqlCreditHoldPort(),
        SqlInternalBudgetHoldPort(internal),
        AllowAllGenerationLimits(),
    )
    context = billing.reserve_internal_generation(
        db_session, center.id, 500, "zero-hold", actor
    )
    billing.release_generation(
        db_session,
        context,
        "no_provider_call",
        "zero-call-release",
    )
    assert db_session.get(InternalBudgetHold, context.hold_id).status == HoldStatus.RELEASED
    assert internal.available_fen(db_session, center.id) == 1_000
    assert db_session.scalar(select(func.count()).select_from(ProviderCostEntry)) == 0


@pytest.mark.parametrize(
    "provider_request_id",
    ["   ", "x" * 201, "None", "NONE", "none", "nOnE"],
)
def test_provider_cost_value_rejects_blank_oversized_or_reserved_request_identity(
    provider_request_id: str,
) -> None:
    with pytest.raises(ValueError, match="provider_request_id"):
        real_provider_cost(task_id=uuid4(), provider_request_id=provider_request_id)


def test_provider_cost_value_rejects_non_string_request_identity() -> None:
    with pytest.raises(ValueError, match="provider_request_id"):
        real_provider_cost(
            task_id=uuid4(),
            provider_request_id=cast(str, 123),
        )


def test_provider_cost_value_canonicalizes_surrounding_whitespace() -> None:
    value = real_provider_cost(
        task_id=uuid4(),
        provider_request_id="  req-canonical-001  ",
    )
    assert value.provider_request_id == "req-canonical-001"


def test_real_provider_cost_rejects_missing_request_identity_at_settlement() -> None:
    with pytest.raises(Conflict, match="provider_request_id"):
        BillingService._require_provider_identity(
            real_provider_cost(task_id=uuid4(), provider_request_id=None)
        )


def test_real_provider_cost_rejects_missing_task_lineage() -> None:
    with pytest.raises(Conflict, match="task_id"):
        BillingService._require_provider_identity(
            real_provider_cost(task_id=None, provider_request_id="req-001")
        )


def test_settlement_replay_comparison_is_exact_before_any_database_write() -> None:
    task_id = uuid4()
    context = BillingContext(BillingMode.CUSTOMER_CREDIT, uuid4())
    cost = real_provider_cost(
        task_id=task_id,
        provider_request_id="provider-request-exact",
    )
    existing = SimpleNamespace(
        task_id=cost.task_id,
        billing_mode=context.mode,
        billing_hold_id=context.hold_id,
        settlement_amount=7,
        provider=cost.provider,
        provider_request_id=cost.provider_request_id,
        capability=cost.capability,
        model_id=cost.model_id,
        model_version=cost.model_version,
        native_quantity=cost.native_quantity,
        native_unit=cost.native_unit,
        supplier_amount_minor=cost.supplier_amount_minor,
        supplier_currency=cost.supplier_currency,
        amount_fen=cost.amount_fen,
        reconciliation_status=cost.reconciliation_status,
    )
    BillingService._require_exact_settlement_replay(existing, context, 7, cost)
    for changed_context, changed_amount, changed_cost in (
        (context, 8, cost),
        (BillingContext(context.mode, uuid4()), 7, cost),
        (BillingContext(BillingMode.INTERNAL_COST, context.hold_id), 7, cost),
        (context, 7, replace(cost, task_id=uuid4())),
        (context, 7, replace(cost, provider_request_id="provider-request-changed")),
        (context, 7, replace(cost, amount_fen=cost.amount_fen + 1)),
    ):
        with pytest.raises(Conflict, match="settlement replay changed input"):
            BillingService._require_exact_settlement_replay(
                existing,
                changed_context,
                changed_amount,
                changed_cost,
            )
```

Add `from uuid import UUID` to the imports at the top of this file.

- [ ] **Step 2: Run the internal-cost tests and verify missing ports/services**

Run: `cd backend && uv run pytest tests/integration/billing/test_internal_costs.py -v`

Expected: FAIL during collection because `SqlCreditHoldPort`, `BillingContext`, or `InternalBudgetService` is missing.

- [ ] **Step 3: Freeze the billing hold port protocols**

Create `backend/src/ip_saas/modules/billing/ports.py`:

```python
from typing import Protocol
from uuid import UUID

from sqlalchemy.orm import Session

from ip_saas.modules.accounts.context import ActorContext


class CreditHoldPort(Protocol):
    def reserve(
        self,
        session: Session,
        account_id: UUID,
        credit_units: int,
        idempotency_key: str,
        actor: ActorContext,
    ) -> UUID: ...

    def settle(
        self,
        session: Session,
        hold_id: UUID,
        actual_units: int,
        idempotency_key: str,
        actor: ActorContext | None,
    ) -> None: ...

    def release(
        self, session: Session, hold_id: UUID, reason: str, idempotency_key: str
    ) -> None: ...


class InternalBudgetHoldPort(Protocol):
    def reserve(
        self,
        session: Session,
        cost_center_id: UUID,
        amount_fen: int,
        idempotency_key: str,
        actor: ActorContext,
    ) -> UUID: ...

    def settle(
        self, session: Session, hold_id: UUID, actual_fen: int, idempotency_key: str
    ) -> None: ...

    def release(
        self, session: Session, hold_id: UUID, reason: str, idempotency_key: str
    ) -> None: ...


class GenerationLimitPort(Protocol):
    def require_customer_reservation(
        self, session: Session, account_id: UUID, requested_units: int
    ) -> object: ...

    def require_internal_reservation(
        self, session: Session, cost_center_id: UUID, requested_fen: int
    ) -> object: ...
```

Create `backend/src/ip_saas/modules/billing/limits.py` as the explicit fail-closed foundation implementation:

```python
from uuid import UUID

from sqlalchemy.orm import Session

from ip_saas.common.errors import Conflict


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
```

Create an empty `backend/tests/support/__init__.py` and create `backend/tests/support/generation_limits.py`:

```python
from uuid import UUID

from sqlalchemy.orm import Session


class AllowAllGenerationLimits:
    """Explicit unit/integration-test seam used only before Plan 05 adds real limits."""

    def __init__(self) -> None:
        self.customer_calls: list[tuple[UUID, int]] = []
        self.internal_calls: list[tuple[UUID, int]] = []

    def require_customer_reservation(
        self, session: Session, account_id: UUID, requested_units: int
    ) -> object:
        del session
        self.customer_calls.append((account_id, requested_units))
        return object()

    def require_internal_reservation(
        self, session: Session, cost_center_id: UUID, requested_fen: int
    ) -> object:
        del session
        self.internal_calls.append((cost_center_id, requested_fen))
        return object()
```

The production composition root uses `UnconfiguredGenerationLimits()` until Plan 05 installs the database-backed service, so paid task creation fails closed during staged development. Ordinary tests must import and pass `AllowAllGenerationLimits()` explicitly; no environment check or implicit unlimited default is permitted.

- [ ] **Step 4: Add billing context fields to provider costs**

Add these three fields to `ProviderCostEntry` in `backend/src/ip_saas/modules/billing/models.py`:

```python
billing_mode: Mapped[str] = mapped_column(String(24), index=True)
billing_hold_id: Mapped[UUID] = mapped_column(index=True)
settlement_amount: Mapped[int] = mapped_column(BigInteger)
```

These fields are immutable with the rest of the provider-cost row and allow every supplier charge to resolve back to exactly one customer hold or internal budget hold. `settlement_amount` stores the exact customer-credit or internal-fen amount supplied on that individual settlement call; later zero-increment rows therefore remain distinguishable from a changed replay after the hold itself has already settled.

- [ ] **Step 5: Implement the internal budget service**

Append to `backend/src/ip_saas/modules/billing/service.py`:

```python
import hashlib
import json
from dataclasses import asdict, dataclass
from decimal import Decimal
from enum import StrEnum

from ip_saas.modules.accounts.context import ActorContext
from ip_saas.modules.billing.models import (
    InternalBudgetHold,
    InternalCostCenter,
    InternalCostEntry,
    InternalCostKind,
    ProviderCostEntry,
    ReconciliationStatus,
)
from ip_saas.modules.billing.ports import (
    CreditHoldPort,
    GenerationLimitPort,
    InternalBudgetHoldPort,
)


RESERVED_PROVIDER_REQUEST_IDS = frozenset({"none"})


def canonical_provider_request_id(
    value: object,
    *,
    allow_missing: bool,
) -> str | None:
    if value is None:
        if allow_missing:
            return None
        raise ValueError("provider_request_id is required")
    if not isinstance(value, str):
        raise ValueError("provider_request_id must be a string")
    request_id = value.strip()
    if not request_id or len(request_id) > 200:
        raise ValueError("provider_request_id must contain 1-200 characters")
    if request_id.casefold() in RESERVED_PROVIDER_REQUEST_IDS:
        raise ValueError("provider_request_id uses a reserved sentinel")
    return request_id


class InternalBudgetService:
    def create_center(
        self,
        session: Session,
        platform_account_id: UUID,
        code: str,
        display_name: str,
    ) -> InternalCostCenter:
        existing = session.scalar(
            select(InternalCostCenter).where(
                InternalCostCenter.platform_account_id == platform_account_id,
                InternalCostCenter.code == code,
            )
        )
        if existing is not None:
            return existing
        center = InternalCostCenter(
            platform_account_id=platform_account_id,
            code=code,
            display_name=display_name,
        )
        session.add(center)
        session.flush()
        return center

    def allocate(
        self,
        session: Session,
        center_id: UUID,
        amount_fen: int,
        actor_id: UUID,
        idempotency_key: str,
    ) -> InternalCostEntry:
        if amount_fen <= 0:
            raise Conflict("allocation must be positive")
        existing = session.scalar(
            select(InternalCostEntry).where(InternalCostEntry.idempotency_key == idempotency_key)
        )
        if existing is not None:
            return existing
        center = session.scalar(
            select(InternalCostCenter).where(InternalCostCenter.id == center_id).with_for_update()
        )
        if center is None:
            raise NotFound("internal cost center")
        entry = InternalCostEntry(
            cost_center_id=center.id,
            kind=InternalCostKind.ALLOCATION,
            amount_fen=amount_fen,
            idempotency_key=idempotency_key,
            created_by_principal_id=actor_id,
            reversal_of_id=None,
        )
        session.add(entry)
        session.flush()
        return entry

    def posted_fen(self, session: Session, center_id: UUID) -> int:
        return int(
            session.scalar(
                select(func.coalesce(func.sum(InternalCostEntry.amount_fen), 0)).where(
                    InternalCostEntry.cost_center_id == center_id
                )
            )
            or 0
        )

    def active_holds_fen(self, session: Session, center_id: UUID) -> int:
        return int(
            session.scalar(
                select(func.coalesce(func.sum(InternalBudgetHold.amount_fen), 0)).where(
                    InternalBudgetHold.cost_center_id == center_id,
                    InternalBudgetHold.status == HoldStatus.ACTIVE,
                )
            )
            or 0
        )

    def available_fen(self, session: Session, center_id: UUID) -> int:
        return self.posted_fen(session, center_id) - self.active_holds_fen(session, center_id)

    def reserve(
        self,
        session: Session,
        center_id: UUID,
        amount_fen: int,
        idempotency_key: str,
        initiated_by_actor_id: UUID,
    ) -> InternalBudgetHold:
        existing = session.scalar(
            select(InternalBudgetHold).where(
                InternalBudgetHold.idempotency_key == idempotency_key
            )
        )
        if existing is not None:
            return existing
        center = session.scalar(
            select(InternalCostCenter).where(InternalCostCenter.id == center_id).with_for_update()
        )
        if center is None:
            raise NotFound("internal cost center")
        if amount_fen <= 0 or self.available_fen(session, center.id) < amount_fen:
            raise Conflict("insufficient internal budget")
        hold = InternalBudgetHold(
            cost_center_id=center.id,
            initiated_by_actor_id=initiated_by_actor_id,
            pricing_version_id=None,
            amount_fen=amount_fen,
            settled_fen=None,
            idempotency_key=idempotency_key,
            release_reason=None,
        )
        session.add(hold)
        session.flush()
        return hold

    def settle(
        self,
        session: Session,
        hold_id: UUID,
        actual_fen: int,
        idempotency_key: str,
    ) -> None:
        hold = session.scalar(
            select(InternalBudgetHold).where(InternalBudgetHold.id == hold_id).with_for_update()
        )
        if hold is None:
            raise NotFound("internal budget hold")
        if hold.status == HoldStatus.SETTLED:
            return
        if hold.status != HoldStatus.ACTIVE or actual_fen < 0 or actual_fen > hold.amount_fen:
            raise Conflict("internal hold cannot be settled with this amount")
        hold.status = HoldStatus.SETTLED
        hold.settled_fen = actual_fen
        if actual_fen:
            session.add(
                InternalCostEntry(
                    cost_center_id=hold.cost_center_id,
                    kind=InternalCostKind.PROVIDER_SPEND,
                    amount_fen=-actual_fen,
                    idempotency_key=idempotency_key,
                    created_by_principal_id=hold.initiated_by_actor_id,
                    reversal_of_id=None,
                )
            )
        session.flush()

    def release(self, session: Session, hold_id: UUID, reason: str) -> None:
        hold = session.scalar(
            select(InternalBudgetHold).where(InternalBudgetHold.id == hold_id).with_for_update()
        )
        if hold is None:
            raise NotFound("internal budget hold")
        if hold.status in {HoldStatus.RELEASED, HoldStatus.SETTLED}:
            return
        hold.status = HoldStatus.RELEASED
        hold.release_reason = reason
        session.flush()
```

- [ ] **Step 6: Implement concrete SQL hold adapters without creating a service/repository cycle**

Create `backend/src/ip_saas/modules/billing/adapters.py`:

```python
from uuid import UUID

from sqlalchemy.orm import Session

from ip_saas.common.errors import NotFound
from ip_saas.modules.accounts.context import ActorContext
from ip_saas.modules.billing.models import GenerationHold
from ip_saas.modules.billing.repository import CreditLedgerRepository
from ip_saas.modules.billing.service import CreditLedgerService, InternalBudgetService


class SqlCreditHoldPort:
    def reserve(
        self,
        session: Session,
        account_id: UUID,
        credit_units: int,
        idempotency_key: str,
        actor: ActorContext,
    ) -> UUID:
        ledger = CreditLedgerService(CreditLedgerRepository())
        wallet = ledger.ensure_account_wallet(session, account_id)
        return ledger.reserve(
            session, wallet.id, credit_units, idempotency_key, actor.actor_id
        ).id

    def settle(
        self,
        session: Session,
        hold_id: UUID,
        actual_units: int,
        idempotency_key: str,
        actor: ActorContext | None,
    ) -> None:
        ledger = CreditLedgerService(CreditLedgerRepository())
        revenue = ledger.ensure_system_wallet(session, "generation_revenue", allow_negative=False)
        hold = session.get(GenerationHold, hold_id)
        if hold is None:
            raise NotFound("generation hold")
        ledger.settle(
            session,
            hold_id,
            revenue.id,
            actual_units,
            actor.actor_id if actor else hold.initiated_by_actor_id,
            idempotency_key,
        )

    def release(
        self, session: Session, hold_id: UUID, reason: str, idempotency_key: str
    ) -> None:
        del idempotency_key
        CreditLedgerService(CreditLedgerRepository()).release(session, hold_id, reason)


class SqlInternalBudgetHoldPort:
    def __init__(self, internal: InternalBudgetService | None = None) -> None:
        self.internal = internal or InternalBudgetService()

    def reserve(
        self,
        session: Session,
        cost_center_id: UUID,
        amount_fen: int,
        idempotency_key: str,
        actor: ActorContext,
    ) -> UUID:
        return self.internal.reserve(
            session, cost_center_id, amount_fen, idempotency_key, actor.actor_id
        ).id

    def settle(
        self, session: Session, hold_id: UUID, actual_fen: int, idempotency_key: str
    ) -> None:
        self.internal.settle(session, hold_id, actual_fen, idempotency_key)

    def release(
        self, session: Session, hold_id: UUID, reason: str, idempotency_key: str
    ) -> None:
        del idempotency_key
        self.internal.release(session, hold_id, reason)
```

The dependency direction is now unambiguous: `service.py` depends on the repository and protocols, while `adapters.py` composes those services for the frozen ports. Neither `service.py` nor `repository.py` imports `adapters.py`.

- [ ] **Step 7: Implement the frozen BillingService API and ProviderCostInput**

Append to `backend/src/ip_saas/modules/billing/service.py`:

```python
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


@dataclass(frozen=True)
class ProviderCostInput:
    provider: str
    capability: str
    model_id: str
    model_version: str
    native_quantity: Decimal
    native_unit: str
    supplier_amount_minor: int
    supplier_currency: str
    amount_fen: int
    reconciliation_status: str
    task_id: UUID | None = None
    provider_request_id: str | None = None

    def __post_init__(self) -> None:
        provider = self.provider.strip()
        capability = self.capability.strip()
        if not provider or not capability:
            raise ValueError("provider and capability must be non-blank")
        object.__setattr__(self, "provider", provider)
        object.__setattr__(self, "capability", capability)
        request_id = canonical_provider_request_id(
            self.provider_request_id,
            allow_missing=True,
        )
        if request_id is not None:
            object.__setattr__(self, "provider_request_id", request_id)


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

    @staticmethod
    def _require_provider_identity(provider_cost: ProviderCostInput) -> None:
        if provider_cost.task_id is None or provider_cost.task_id.int == 0:
            raise Conflict("task_id is required for every real provider settlement")
        try:
            request_id = canonical_provider_request_id(
                provider_cost.provider_request_id,
                allow_missing=False,
            )
        except ValueError as error:
            raise Conflict(
                "provider_request_id is required, canonical, non-reserved, and 1-200 characters"
            ) from error
        if request_id != provider_cost.provider_request_id:
            raise Conflict("provider_request_id must already be canonical")

    def reserve_customer_generation(
        self,
        session: Session,
        account_id: UUID,
        credit_units: int,
        idempotency_key: str,
        actor: ActorContext,
    ) -> BillingContext:
        self.generation_limits.require_customer_reservation(
            session, account_id, credit_units
        )
        hold_id = self.credit_holds.reserve(
            session, account_id, credit_units, idempotency_key, actor
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
            session, cost_center_id, amount_fen
        )
        hold_id = self.internal_holds.reserve(
            session, cost_center_id, amount_fen, idempotency_key, actor
        )
        return BillingContext(BillingMode.INTERNAL_COST, hold_id)

    @staticmethod
    def _require_exact_settlement_replay(
        existing: ProviderCostEntry,
        context: BillingContext,
        actual_amount: int,
        provider_cost: ProviderCostInput,
    ) -> None:
        expected = (
            provider_cost.task_id,
            context.mode,
            context.hold_id,
            actual_amount,
            provider_cost.provider,
            provider_cost.provider_request_id,
            provider_cost.capability,
            provider_cost.model_id,
            provider_cost.model_version,
            provider_cost.native_quantity,
            provider_cost.native_unit,
            provider_cost.supplier_amount_minor,
            provider_cost.supplier_currency,
            provider_cost.amount_fen,
            provider_cost.reconciliation_status,
        )
        actual = (
            existing.task_id,
            existing.billing_mode,
            existing.billing_hold_id,
            existing.settlement_amount,
            existing.provider,
            existing.provider_request_id,
            existing.capability,
            existing.model_id,
            existing.model_version,
            existing.native_quantity,
            existing.native_unit,
            existing.supplier_amount_minor,
            existing.supplier_currency,
            existing.amount_fen,
            existing.reconciliation_status,
        )
        if actual != expected:
            raise Conflict("provider cost settlement replay changed input")

    def settle_generation(
        self,
        session: Session,
        context: BillingContext,
        actual_amount: int,
        provider_cost: ProviderCostInput,
        idempotency_key: str,
    ) -> None:
        self._require_provider_identity(provider_cost)
        existing = session.scalar(
            select(ProviderCostEntry).where(ProviderCostEntry.idempotency_key == idempotency_key)
        )
        if existing is not None:
            self._require_exact_settlement_replay(
                existing,
                context,
                actual_amount,
                provider_cost,
            )
            return
        if actual_amount < 0 or provider_cost.amount_fen < 0:
            raise Conflict("cost amounts cannot be negative")
        if actual_amount == 0:
            self.release_generation(
                session, context, "zero_cost_completion", f"{idempotency_key}:zero-cost"
            )
            center_id = None
            if context.mode == BillingMode.INTERNAL_COST:
                hold = session.get(InternalBudgetHold, context.hold_id)
                center_id = hold.cost_center_id if hold else None
        elif context.mode == BillingMode.CUSTOMER_CREDIT:
            self.credit_holds.settle(
                session, context.hold_id, actual_amount, f"{idempotency_key}:credits", None
            )
            center_id = None
        else:
            self.internal_holds.settle(
                session, context.hold_id, actual_amount, f"{idempotency_key}:internal"
            )
            hold = session.get(InternalBudgetHold, context.hold_id)
            center_id = hold.cost_center_id if hold else None
        session.add(
            ProviderCostEntry(
                task_id=provider_cost.task_id,
                internal_cost_center_id=center_id,
                billing_mode=context.mode,
                billing_hold_id=context.hold_id,
                settlement_amount=actual_amount,
                provider=provider_cost.provider,
                provider_request_id=provider_cost.provider_request_id,
                capability=provider_cost.capability,
                model_id=provider_cost.model_id,
                model_version=provider_cost.model_version,
                native_quantity=provider_cost.native_quantity,
                native_unit=provider_cost.native_unit,
                supplier_amount_minor=provider_cost.supplier_amount_minor,
                supplier_currency=provider_cost.supplier_currency,
                amount_fen=provider_cost.amount_fen,
                reconciliation_status=provider_cost.reconciliation_status,
                idempotency_key=idempotency_key,
            )
        )
        session.flush()

    def settle_generation_batch(
        self,
        session: Session,
        context: BillingContext,
        actual_amount: int,
        provider_costs: tuple[ProviderCostInput, ...],
        idempotency_key: str,
    ) -> None:
        if not provider_costs:
            raise Conflict("provider cost batch cannot be empty")
        for provider_cost in provider_costs:
            self._require_provider_identity(provider_cost)
        if actual_amount < 0 or any(cost.amount_fen < 0 for cost in provider_costs):
            raise Conflict("cost amounts cannot be negative")
        if any(cost.task_id is None for cost in provider_costs):
            raise Conflict("batch provider costs require task lineage")
        if len({cost.task_id for cost in provider_costs}) != 1:
            raise Conflict("batch provider costs must reference one task")
        identities = tuple(
            (cost.provider, (cost.provider_request_id or "").strip())
            for cost in provider_costs
        )
        if any(not request_id for _provider, request_id in identities):
            raise Conflict("batch provider costs require provider request identity")
        if len(set(identities)) != len(identities):
            raise Conflict("provider request identity is duplicated in the batch")
        if any(
            cost.reconciliation_status != ReconciliationStatus.MATCHED
            for cost in provider_costs
        ):
            raise Conflict("batch provider costs must be supplier-reconciled")
        if context.mode == BillingMode.CUSTOMER_CREDIT and actual_amount != 0:
            raise Conflict("failed customer work settles zero customer credits")
        total_fen = sum(cost.amount_fen for cost in provider_costs)
        if context.mode == BillingMode.INTERNAL_COST and actual_amount != total_fen:
            raise Conflict("internal batch settlement must equal total supplier CNY cost")

        ordered = tuple(
            sorted(
                provider_costs,
                key=lambda cost: (cost.provider, cost.provider_request_id or ""),
            )
        )
        batch_fingerprint = hashlib.sha256(
            json.dumps(
                {
                    "idempotency_key": idempotency_key,
                    "actual_amount": actual_amount,
                    "provider_costs": [asdict(cost) for cost in ordered],
                },
                default=str,
                ensure_ascii=False,
                allow_nan=False,
                separators=(",", ":"),
                sort_keys=True,
            ).encode("utf-8")
        ).hexdigest()
        row_keys = tuple(
            f"provider-cost-batch:{batch_fingerprint}:{index}"
            for index in range(len(ordered))
        )
        existing_rows = tuple(
            session.scalar(
                select(ProviderCostEntry).where(
                    ProviderCostEntry.idempotency_key == row_key
                )
            )
            for row_key in row_keys
        )
        if any(row is not None for row in existing_rows):
            if not all(row is not None for row in existing_rows):
                raise Conflict("provider cost batch is only partially persisted")
            for index, (row, cost) in enumerate(
                zip(existing_rows, ordered, strict=True)
            ):
                assert row is not None
                expected = (
                    cost.task_id,
                    context.mode,
                    context.hold_id,
                    actual_amount if index == 0 else 0,
                    cost.provider,
                    cost.provider_request_id,
                    cost.capability,
                    cost.model_id,
                    cost.model_version,
                    cost.native_quantity,
                    cost.native_unit,
                    cost.supplier_amount_minor,
                    cost.supplier_currency,
                    cost.amount_fen,
                    cost.reconciliation_status,
                )
                actual = (
                    row.task_id,
                    row.billing_mode,
                    row.billing_hold_id,
                    row.settlement_amount,
                    row.provider,
                    row.provider_request_id,
                    row.capability,
                    row.model_id,
                    row.model_version,
                    row.native_quantity,
                    row.native_unit,
                    row.supplier_amount_minor,
                    row.supplier_currency,
                    row.amount_fen,
                    row.reconciliation_status,
                )
                if actual != expected:
                    raise Conflict("provider cost batch replay changed input")
            return

        for cost in ordered:
            identity_owner = session.scalar(
                select(ProviderCostEntry).where(
                    ProviderCostEntry.provider == cost.provider,
                    ProviderCostEntry.provider_request_id == cost.provider_request_id,
                )
            )
            if identity_owner is not None:
                raise Conflict("provider request identity already belongs to another cost row")

        if context.mode == BillingMode.CUSTOMER_CREDIT:
            self.release_generation(
                session,
                context,
                "reconciled_provider_failure",
                f"provider-cost-batch:{batch_fingerprint}:release",
            )
            center_id = None
        else:
            self.internal_holds.settle(
                session,
                context.hold_id,
                actual_amount,
                f"provider-cost-batch:{batch_fingerprint}:internal",
            )
            hold = session.get(InternalBudgetHold, context.hold_id)
            center_id = hold.cost_center_id if hold else None
        session.add_all(
            [
                ProviderCostEntry(
                    task_id=cost.task_id,
                    internal_cost_center_id=center_id,
                    billing_mode=context.mode,
                    billing_hold_id=context.hold_id,
                    settlement_amount=(actual_amount if index == 0 else 0),
                    provider=cost.provider,
                    provider_request_id=cost.provider_request_id,
                    capability=cost.capability,
                    model_id=cost.model_id,
                    model_version=cost.model_version,
                    native_quantity=cost.native_quantity,
                    native_unit=cost.native_unit,
                    supplier_amount_minor=cost.supplier_amount_minor,
                    supplier_currency=cost.supplier_currency,
                    amount_fen=cost.amount_fen,
                    reconciliation_status=cost.reconciliation_status,
                    idempotency_key=row_key,
                )
                for index, (cost, row_key) in enumerate(
                    zip(ordered, row_keys, strict=True)
                )
            ]
        )
        session.flush()

    def release_generation(
        self,
        session: Session,
        context: BillingContext,
        reason: str,
        idempotency_key: str,
    ) -> None:
        if context.mode == BillingMode.CUSTOMER_CREDIT:
            self.credit_holds.release(session, context.hold_id, reason, idempotency_key)
        else:
            self.internal_holds.release(session, context.hold_id, reason, idempotency_key)
```

- [ ] **Step 8: Run internal-cost and credit regression tests**

Run: `cd backend && uv run pytest tests/integration/billing/test_internal_costs.py tests/integration/billing/test_credit_ledger.py -v`

Expected: PASS, 18 collected test cases across both files; an unconfigured production limit seam creates no hold, no `CreditWallet` row is created for the platform internal cost center, a proven zero-call outcome releases its hold without creating a supplier-cost record, malformed or reserved provider identities are rejected before settlement, surrounding whitespace is canonicalized once, and exact replay comparison rejects every changed immutable input.

- [ ] **Step 9: Add an internal-ledger idempotency assertion without inventing supplier usage**

Append to `test_internal_budget_hold_never_uses_credit_wallet` after the first direct internal settlement call:

```python
internal.settle(db_session, context.hold_id, 6_000, "internal-settle-1")
assert db_session.scalar(select(func.count()).select_from(ProviderCostEntry)) == 0
assert db_session.scalar(
    select(func.count()).select_from(InternalCostEntry).where(
        InternalCostEntry.cost_center_id == center.id,
        InternalCostEntry.kind == InternalCostKind.PROVIDER_SPEND,
    )
) == 1
```

This task has not created `TaskRecord` yet, so it must not persist a non-zero supplier-cost row with `task_id=None`. Task 11 adds the non-zero provider-cost reconciliation test after a real task exists and requires `ProviderCostInput.task_id == TaskRecord.id`.

- [ ] **Step 10: Run the idempotency regression**

Run: `cd backend && uv run pytest tests/integration/billing/test_internal_costs.py -v`

Expected: PASS, 15 collected cases; the duplicate direct settlement leaves one internal spend entry and no provider-cost row, the proven zero-call path releases without constructing `ProviderCostInput` or `ProviderCostEntry`, every fake or real completion entering settlement carries the exact task lineage and a canonical request identity, malformed and case-insensitive reserved provider request identities fail before billing, canonical whitespace normalization and exact replay comparison pass, and the fail-closed limit regression remains green.

- [ ] **Step 11: Commit separate internal cost accounting and frozen billing ports**

```bash
git add backend/src/ip_saas/modules/billing backend/tests/support backend/tests/integration/billing/test_internal_costs.py
git commit -m "feat: separate customer credits and internal costs"
```

### Task 10: Add a gated model capability registry

**Files:**
- Create: `backend/src/ip_saas/modules/model_registry/models.py`
- Create: `backend/src/ip_saas/modules/model_registry/service.py`
- Create: `backend/src/ip_saas/modules/model_registry/router.py`
- Create: `backend/tests/integration/model_registry/test_model_registry.py`
- Modify: `backend/src/ip_saas/api.py`

- [ ] **Step 1: Write the failing model-activation tests**

Create `backend/tests/integration/model_registry/test_model_registry.py`:

```python
from uuid import uuid4

import pytest
from sqlalchemy.orm import Session

from ip_saas.common.errors import Conflict, Forbidden
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.model_registry.models import ModelLifecycle
from ip_saas.modules.model_registry.service import ModelRegistryService


def test_candidate_requires_passing_regression_before_activation(db_session: Session) -> None:
    actor = ActorContext(uuid4(), uuid4(), ActorKind.PLATFORM_ADMIN)
    service = ModelRegistryService()
    candidate = service.register_candidate(
        db_session,
        actor,
        capability="text_strategy",
        provider="volcengine",
        model_id="doubao-model-id",
        model_version="2026-08-01",
        input_modalities=["text"],
        output_modalities=["text"],
        parameter_schema={"temperature": {"type": "number"}},
        pricing_version_id=None,
        safety_version="safety-v1",
    )
    with pytest.raises(Conflict):
        service.activate(db_session, actor, candidate.id)
    service.record_regression(db_session, actor, candidate.id, passed=True, report_ref="run-001")
    service.activate(db_session, actor, candidate.id)
    assert candidate.lifecycle == ModelLifecycle.ACTIVE


def test_non_admin_cannot_register_model(db_session: Session) -> None:
    actor = ActorContext(uuid4(), uuid4(), ActorKind.C_USER)
    with pytest.raises(Forbidden):
        ModelRegistryService().register_candidate(
            db_session, actor, "image", "volcengine", "model", "v1", ["text"], ["image"], {}, None, "s1"
        )
```

- [ ] **Step 2: Run the model tests and verify missing registry behavior**

Run: `cd backend && uv run pytest tests/integration/model_registry/test_model_registry.py -v`

Expected: FAIL during collection because the model registry module is missing.

- [ ] **Step 3: Implement model registry persistence**

Create `backend/src/ip_saas/modules/model_registry/models.py`:

```python
from enum import StrEnum
from uuid import UUID

from sqlalchemy import Boolean, ForeignKey, Index, JSON, String, text
from sqlalchemy.orm import Mapped, mapped_column

from ip_saas.db.base import Base, TimestampMixin, UUIDPrimaryKeyMixin


class ModelLifecycle(StrEnum):
    CANDIDATE = "candidate"
    ACTIVE = "active"
    RETIRING = "retiring"
    SHUTDOWN = "shutdown"


class ModelRegistryEntry(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "model_registry_entries"
    __table_args__ = (
        Index(
            "uq_active_model_capability",
            "capability",
            unique=True,
            postgresql_where=text("lifecycle = 'active'"),
        ),
    )
    capability: Mapped[str] = mapped_column(String(80), index=True)
    provider: Mapped[str] = mapped_column(String(60))
    model_id: Mapped[str] = mapped_column(String(180))
    model_version: Mapped[str] = mapped_column(String(100))
    input_modalities: Mapped[list[str]] = mapped_column(JSON)
    output_modalities: Mapped[list[str]] = mapped_column(JSON)
    parameter_schema: Mapped[dict[str, object]] = mapped_column(JSON)
    pricing_version_id: Mapped[UUID | None] = mapped_column(ForeignKey("pricing_versions.id"))
    safety_version: Mapped[str] = mapped_column(String(100))
    lifecycle: Mapped[str] = mapped_column(String(20), default=ModelLifecycle.CANDIDATE)
    regression_passed: Mapped[bool] = mapped_column(Boolean, default=False)
    regression_report_ref: Mapped[str | None] = mapped_column(String(240))
    created_by_principal_id: Mapped[UUID]
```

- [ ] **Step 4: Implement registration, regression, activation, and retirement gates**

Create `backend/src/ip_saas/modules/model_registry/service.py`:

```python
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.errors import Conflict, Forbidden, NotFound
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.model_registry.models import ModelLifecycle, ModelRegistryEntry


class ModelRegistryService:
    @staticmethod
    def _require_admin(actor: ActorContext) -> None:
        if actor.kind != ActorKind.PLATFORM_ADMIN:
            raise Forbidden("platform admin is required")

    def register_candidate(
        self,
        session: Session,
        actor: ActorContext,
        capability: str,
        provider: str,
        model_id: str,
        model_version: str,
        input_modalities: list[str],
        output_modalities: list[str],
        parameter_schema: dict[str, object],
        pricing_version_id: UUID | None,
        safety_version: str,
    ) -> ModelRegistryEntry:
        self._require_admin(actor)
        entry = ModelRegistryEntry(
            capability=capability,
            provider=provider,
            model_id=model_id,
            model_version=model_version,
            input_modalities=input_modalities,
            output_modalities=output_modalities,
            parameter_schema=parameter_schema,
            pricing_version_id=pricing_version_id,
            safety_version=safety_version,
            created_by_principal_id=actor.actor_id,
            regression_report_ref=None,
        )
        session.add(entry)
        session.flush()
        return entry

    def record_regression(
        self,
        session: Session,
        actor: ActorContext,
        entry_id: UUID,
        *,
        passed: bool,
        report_ref: str,
    ) -> ModelRegistryEntry:
        self._require_admin(actor)
        entry = session.get(ModelRegistryEntry, entry_id)
        if entry is None:
            raise NotFound("model registry entry")
        if entry.lifecycle != ModelLifecycle.CANDIDATE:
            raise Conflict("only a candidate can receive a regression result")
        entry.regression_passed = passed
        entry.regression_report_ref = report_ref
        session.flush()
        return entry

    def activate(
        self, session: Session, actor: ActorContext, entry_id: UUID
    ) -> ModelRegistryEntry:
        self._require_admin(actor)
        entry = session.scalar(
            select(ModelRegistryEntry).where(ModelRegistryEntry.id == entry_id).with_for_update()
        )
        if entry is None:
            raise NotFound("model registry entry")
        if not entry.regression_passed:
            raise Conflict("regression suite must pass before activation")
        current = session.scalar(
            select(ModelRegistryEntry).where(
                ModelRegistryEntry.capability == entry.capability,
                ModelRegistryEntry.lifecycle == ModelLifecycle.ACTIVE,
            ).with_for_update()
        )
        if current is not None and current.id != entry.id:
            current.lifecycle = ModelLifecycle.RETIRING
        entry.lifecycle = ModelLifecycle.ACTIVE
        session.flush()
        return entry

    def require_active(self, session: Session, capability: str) -> ModelRegistryEntry:
        entry = session.scalar(
            select(ModelRegistryEntry).where(
                ModelRegistryEntry.capability == capability,
                ModelRegistryEntry.lifecycle == ModelLifecycle.ACTIVE,
            )
        )
        if entry is None:
            raise NotFound("active model capability")
        return entry
```

- [ ] **Step 5: Run the model registry tests**

Run: `cd backend && uv run pytest tests/integration/model_registry/test_model_registry.py -v`

Expected: PASS, 2 tests.

- [ ] **Step 6: Add the platform-admin model registry route**

Create `backend/src/ip_saas/modules/model_registry/router.py`:

```python
from typing import Annotated
from uuid import UUID

from fastapi import APIRouter, Depends
from pydantic import BaseModel, ConfigDict
from sqlalchemy.orm import Session

from ip_saas.db.session import get_session
from ip_saas.modules.accounts.context import ActorContext
from ip_saas.modules.accounts.dependencies import get_actor
from ip_saas.modules.model_registry.models import ModelRegistryEntry
from ip_saas.modules.model_registry.service import ModelRegistryService

router = APIRouter(prefix="/v1/platform/models", tags=["platform-models"])


class ModelCandidateCreate(BaseModel):
    model_config = ConfigDict(extra="forbid")
    capability: str
    provider: str
    model_id: str
    model_version: str
    input_modalities: list[str]
    output_modalities: list[str]
    parameter_schema: dict[str, object]
    pricing_version_id: UUID | None = None
    safety_version: str


class ModelResponse(BaseModel):
    id: UUID
    capability: str
    provider: str
    model_id: str
    model_version: str
    lifecycle: str


def present(entry: ModelRegistryEntry) -> ModelResponse:
    return ModelResponse(
        id=entry.id,
        capability=entry.capability,
        provider=entry.provider,
        model_id=entry.model_id,
        model_version=entry.model_version,
        lifecycle=entry.lifecycle,
    )


@router.post("", response_model=ModelResponse, status_code=201)
def register_model(
    payload: ModelCandidateCreate,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> ModelResponse:
    return present(
        ModelRegistryService().register_candidate(
            session,
            actor,
            payload.capability,
            payload.provider,
            payload.model_id,
            payload.model_version,
            payload.input_modalities,
            payload.output_modalities,
            payload.parameter_schema,
            payload.pricing_version_id,
            payload.safety_version,
        )
    )
```

Include the router in `create_app()` after the projects router.

- [ ] **Step 7: Verify route authorization and OpenAPI presence**

Run: `cd backend && uv run pytest tests/integration/model_registry/test_model_registry.py tests/integration/test_health_and_migrations.py -v`

Expected: PASS; `create_app().openapi()["paths"]` contains `/v1/platform/models`, and the service test proves a C user receives `Forbidden`.

- [ ] **Step 8: Commit the gated model registry**

```bash
git add backend/src/ip_saas/modules/model_registry backend/tests/integration/model_registry backend/src/ip_saas/api.py
git commit -m "feat: gate production model activation"
```

### Task 11: Atomically reserve billing, create a task, audit it, and enqueue its event

**Files:**
- Create: `backend/src/ip_saas/common/tasking.py`
- Create: `backend/src/ip_saas/modules/tasks/models.py`
- Create: `backend/src/ip_saas/modules/tasks/events.py`
- Create: `backend/src/ip_saas/modules/tasks/reconciliation.py`
- Create: `backend/src/ip_saas/modules/tasks/service.py`
- Create: `backend/src/ip_saas/modules/tasks/router.py`
- Create: `backend/src/ip_saas/workers/task_consumer.py`
- Create: `backend/tests/integration/tasks/test_task_submission.py`
- Create: `backend/tests/integration/tasks/test_task_lease_concurrency.py`
- Create: `backend/tests/unit/tasks/test_task_consumer.py`
- Modify: `backend/src/ip_saas/api.py`

- [ ] **Step 1: Write failing atomic task-submission and cross-session settlement-replay tests**

Create `backend/tests/integration/tasks/test_task_submission.py`:

```python
from dataclasses import replace
from datetime import UTC, datetime, timedelta
from decimal import Decimal
from uuid import UUID, uuid4

import pytest
from pydantic import ValidationError
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditEvent, AuditWriter
from ip_saas.common.errors import Conflict, NotFound
from ip_saas.common.outbox import OutboxEvent, OutboxWriter
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import Account, AccountKind, Principal
from ip_saas.modules.billing.adapters import SqlCreditHoldPort, SqlInternalBudgetHoldPort
from ip_saas.modules.billing.models import (
    GenerationHold,
    HoldStatus,
    InternalBudgetHold,
    ProviderCostEntry,
    ReconciliationStatus,
)
from ip_saas.modules.billing.repository import CreditLedgerRepository
from ip_saas.modules.billing.service import (
    BillingContext,
    BillingMode,
    BillingService,
    CreditLedgerService,
    InternalBudgetService,
    ProviderCostInput,
)
from ip_saas.modules.model_registry.service import ModelRegistryService
from ip_saas.modules.projects.access import ProjectAccessService
from ip_saas.modules.projects.service import ProjectService
from ip_saas.modules.tasks.events import GenerationTaskRequestedV1
from ip_saas.modules.tasks.models import TaskRecord
from ip_saas.modules.tasks.reconciliation import (
    NoProviderCallEvidence,
    ProviderRequestIdentity,
    TaskReconciliationService,
)
from ip_saas.modules.tasks.service import TaskSubmissionService
from tests.support.generation_limits import AllowAllGenerationLimits


class FixedClock:
    def now(self) -> datetime:
        return datetime(2026, 8, 24, 8, 0, tzinfo=UTC)


class StaticNoProviderCallEvidence:
    def require_no_provider_call(
        self,
        _session: Session,
        task_id: UUID,
        through_attempt_no: int,
    ) -> NoProviderCallEvidence:
        return NoProviderCallEvidence(
            task_id=task_id,
            through_attempt_no=through_attempt_no,
            evidence_ref=f"provider-journal://{task_id}/no-calls",
            evidence_sha256="e" * 64,
        )


class StaticProviderCostManifest:
    def __init__(self, identities: tuple[ProviderRequestIdentity, ...]) -> None:
        self.identities = identities

    def require_complete_provider_requests(
        self,
        _session: Session,
        _task_id: UUID,
        _through_attempt_no: int,
    ) -> tuple[ProviderRequestIdentity, ...]:
        return self.identities


def test_generation_task_event_rejects_extra_fields_and_identity_mismatch() -> None:
    payload = {
        "task_id": UUID(int=902),
        "project_id": UUID(int=904),
        "capability": "text_strategy",
        "model_registry_entry_id": UUID(int=905),
        "billing_mode": "customer_credit",
        "billing_hold_id": UUID(int=906),
        "request_fingerprint": "a" * 64,
        "input_payload": {"brief": "gold"},
    }
    base = {
        "event_id": UUID(int=901),
        "event_type": "generation.task.requested",
        "schema_version": 1,
        "aggregate_id": UUID(int=902),
        "occurred_at": datetime(2026, 8, 24, tzinfo=UTC),
        "initiated_by_actor_id": UUID(int=903),
        "idempotency_key": "task:902",
        "payload": payload,
    }
    assert GenerationTaskRequestedV1.model_validate(base).payload.task_id == UUID(int=902)
    with pytest.raises(ValidationError):
        GenerationTaskRequestedV1.model_validate(
            {**base, "payload": {**payload, "unexpected": True}}
        )
    with pytest.raises(ValidationError):
        GenerationTaskRequestedV1.model_validate(
            {**base, "payload": {**payload, "task_id": UUID(int=999)}}
        )


class ExplodingOutbox(OutboxWriter):
    def add(self, session: Session, event: object) -> object:
        raise RuntimeError("outbox unavailable")


def create_replayable_customer_task(
    db_session: Session,
) -> tuple[BillingService, TaskRecord, ProviderCostInput]:
    suffix = uuid4().hex
    account = Account(
        kind=AccountKind.C_USER,
        display_name=f"Replay customer {suffix[:8]}",
    )
    db_session.add(account)
    db_session.flush()
    principal = Principal(
        account_id=account.id,
        login_name=f"replay-{suffix}@example.com",
        password_hash="hash",
    )
    db_session.add(principal)
    db_session.flush()
    actor = ActorContext(principal.id, account.id, ActorKind.C_USER)
    project = ProjectService(ProjectAccessService()).create(
        db_session,
        actor,
        f"Replay project {suffix[:8]}",
    )
    ledger = CreditLedgerService(CreditLedgerRepository())
    treasury = ledger.ensure_system_wallet(
        db_session,
        f"replay_{suffix[:16]}",
        allow_negative=True,
    )
    wallet = ledger.ensure_account_wallet(db_session, account.id)
    ledger.issue(
        db_session,
        treasury.id,
        wallet.id,
        100,
        actor.actor_id,
        f"replay-credit-{suffix}",
    )
    registry = ModelRegistryService()
    admin = ActorContext(uuid4(), uuid4(), ActorKind.PLATFORM_ADMIN)
    candidate = registry.register_candidate(
        db_session,
        admin,
        f"replay-capability-{suffix}",
        "volcengine",
        f"replay-model-{suffix}",
        "v1",
        ["text"],
        ["text"],
        {},
        None,
        "safe-v1",
    )
    registry.record_regression(
        db_session,
        admin,
        candidate.id,
        passed=True,
        report_ref="replay-contract",
    )
    registry.activate(db_session, admin, candidate.id)
    billing = BillingService(
        SqlCreditHoldPort(),
        SqlInternalBudgetHoldPort(),
        AllowAllGenerationLimits(),
    )
    tasks = TaskSubmissionService(
        ProjectAccessService(),
        billing,
        registry,
        AuditWriter(FixedClock()),
        OutboxWriter(FixedClock()),
        FixedClock(),
    )
    task = tasks.submit_customer(
        db_session,
        actor,
        project.id,
        candidate.capability,
        10,
        f"replay-task-{suffix}",
        {"brief": "exact settlement replay"},
    )
    cost = ProviderCostInput(
        provider="volcengine",
        capability=candidate.capability,
        model_id=candidate.model_id,
        model_version=candidate.model_version,
        native_quantity=Decimal("10"),
        native_unit="token",
        supplier_amount_minor=3,
        supplier_currency="CNY",
        amount_fen=3,
        reconciliation_status=ReconciliationStatus.MATCHED,
        task_id=task.id,
        provider_request_id=f"provider-request-{suffix}",
    )
    db_session.flush()
    return billing, task, cost


def settle_in_independent_session(
    db_session: Session,
    billing: BillingService,
    task: TaskRecord,
    actual_amount: int,
    cost: ProviderCostInput,
    idempotency_key: str,
) -> None:
    with Session(
        bind=db_session.connection(),
        join_transaction_mode="create_savepoint",
        expire_on_commit=False,
    ) as isolated:
        with isolated.begin():
            billing.settle_generation(
                isolated,
                BillingContext(
                    BillingMode(task.billing_mode),
                    task.billing_hold_id,
                ),
                actual_amount,
                cost,
                idempotency_key,
            )


def test_exact_provider_settlement_replay_across_sessions_is_reused(
    db_session: Session,
) -> None:
    billing, task, cost = create_replayable_customer_task(db_session)
    key = f"provider-settlement-replay:{task.id}"
    settle_in_independent_session(db_session, billing, task, 7, cost, key)
    settle_in_independent_session(db_session, billing, task, 7, cost, key)
    assert db_session.scalar(
        select(func.count()).select_from(ProviderCostEntry).where(
            ProviderCostEntry.idempotency_key == key
        )
    ) == 1


@pytest.mark.parametrize(
    "changed_field",
    ["provider_request_id", "task_id", "actual_amount", "supplier_amount"],
)
def test_changed_provider_settlement_replay_across_sessions_conflicts(
    db_session: Session,
    changed_field: str,
) -> None:
    billing, task, original = create_replayable_customer_task(db_session)
    key = f"provider-settlement-changed:{task.id}"
    settle_in_independent_session(db_session, billing, task, 7, original, key)
    changed_cost = original
    changed_actual = 7
    if changed_field == "provider_request_id":
        changed_cost = replace(original, provider_request_id=f"changed-{task.id}")
    elif changed_field == "task_id":
        changed_cost = replace(original, task_id=uuid4())
    elif changed_field == "actual_amount":
        changed_actual = 8
    else:
        changed_cost = replace(
            original,
            supplier_amount_minor=4,
            amount_fen=4,
        )
    with pytest.raises(Conflict, match="settlement replay changed input"):
        settle_in_independent_session(
            db_session,
            billing,
            task,
            changed_actual,
            changed_cost,
            key,
        )


def test_hold_task_audit_and_outbox_rollback_together(db_session: Session) -> None:
    account = Account(kind=AccountKind.C_USER, display_name="Customer")
    db_session.add(account)
    db_session.flush()
    principal = Principal(account_id=account.id, login_name="customer@example.com", password_hash="hash")
    db_session.add(principal)
    db_session.flush()
    actor = ActorContext(principal.id, account.id, ActorKind.C_USER)
    project = ProjectService(ProjectAccessService()).create(db_session, actor, "Gold gifts")
    ledger = CreditLedgerService(CreditLedgerRepository())
    treasury = ledger.ensure_system_wallet(db_session, "credit_treasury", allow_negative=True)
    wallet = ledger.ensure_account_wallet(db_session, account.id)
    ledger.issue(db_session, treasury.id, wallet.id, 500, actor.actor_id, "seed-credit")
    service = TaskSubmissionService(
        ProjectAccessService(),
        BillingService(
            SqlCreditHoldPort(),
            SqlInternalBudgetHoldPort(),
            AllowAllGenerationLimits(),
        ),
        ModelRegistryService(),
        AuditWriter(FixedClock()),
        ExplodingOutbox(FixedClock()),
        FixedClock(),
    )

    with pytest.raises(RuntimeError):
        with db_session.begin_nested():
            service.submit_customer(
                db_session,
                actor,
                project.id,
                "text_strategy",
                100,
                "task-atomic",
                {"brief": "gold gifts"},
            )

    assert db_session.scalar(select(func.count()).select_from(TaskRecord)) == 0
    assert db_session.scalar(select(func.count()).select_from(AuditEvent)) == 0
    assert db_session.scalar(select(func.count()).select_from(OutboxEvent)) == 0
    assert ledger.available_balance(db_session, wallet.id) == 500
```

- [ ] **Step 2: Run the test and verify the missing task module failure**

Run: `cd backend && uv run pytest tests/integration/tasks/test_task_submission.py -v`

Expected: FAIL during collection because `ip_saas.modules.tasks.models` is missing.

- [ ] **Step 3: Define shared task states and legal transitions**

Create `backend/src/ip_saas/common/tasking.py`:

```python
from enum import StrEnum

from ip_saas.common.audit import JSONValue
from ip_saas.common.errors import Conflict


__all__ = ["JSONValue", "TaskStatus", "require_task_transition"]


class TaskStatus(StrEnum):
    QUEUED = "queued"
    RUNNING = "running"
    RECONCILIATION_REQUIRED = "reconciliation_required"
    SUCCEEDED = "succeeded"
    FAILED = "failed"
    CANCELLED = "cancelled"


ALLOWED_TASK_TRANSITIONS = {
    TaskStatus.QUEUED: {TaskStatus.RUNNING, TaskStatus.CANCELLED},
    TaskStatus.RUNNING: {
        TaskStatus.RECONCILIATION_REQUIRED,
        TaskStatus.SUCCEEDED,
        TaskStatus.FAILED,
    },
    TaskStatus.RECONCILIATION_REQUIRED: {TaskStatus.FAILED},
    TaskStatus.SUCCEEDED: set(),
    TaskStatus.FAILED: set(),
    TaskStatus.CANCELLED: set(),
}


def require_task_transition(current: TaskStatus, target: TaskStatus) -> None:
    if target not in ALLOWED_TASK_TRANSITIONS[current]:
        raise Conflict(f"invalid task transition: {current} -> {target}")
```

- [ ] **Step 4: Persist task identity, billing context, and lineage**

Create `backend/src/ip_saas/modules/tasks/models.py`:

```python
from datetime import datetime
from uuid import UUID

from sqlalchemy import CheckConstraint, DateTime, ForeignKey, String, UniqueConstraint
from sqlalchemy.dialects.postgresql import JSONB
from sqlalchemy.orm import Mapped, mapped_column

from ip_saas.common.tasking import TaskStatus
from ip_saas.db.base import Base, TimestampMixin, UUIDPrimaryKeyMixin


class TaskRecord(UUIDPrimaryKeyMixin, TimestampMixin, Base):
    __tablename__ = "task_records"
    __table_args__ = (
        UniqueConstraint(
            "initiated_by_account_id",
            "idempotency_key",
            name="uq_task_account_idempotency",
        ),
        CheckConstraint(
            "billing_mode in ('customer_credit', 'internal_cost')",
            name="ck_task_billing_mode",
        ),
        CheckConstraint(
            "length(request_fingerprint) = 64",
            name="ck_task_request_fingerprint",
        ),
        CheckConstraint(
            "status in ('queued', 'running', 'reconciliation_required', "
            "'succeeded', 'failed', 'cancelled')",
            name="ck_task_status",
        ),
        CheckConstraint(
            "(reconciliation_idempotency_key is null and "
            "reconciliation_fingerprint is null) or "
            "(reconciliation_idempotency_key is not null and "
            "length(reconciliation_fingerprint) = 64)",
            name="ck_task_reconciliation_identity",
        ),
    )
    project_id: Mapped[UUID] = mapped_column(ForeignKey("ip_projects.id"), index=True)
    initiated_by_actor_id: Mapped[UUID]
    initiated_by_account_id: Mapped[UUID]
    capability: Mapped[str] = mapped_column(String(80), index=True)
    model_registry_entry_id: Mapped[UUID] = mapped_column(
        ForeignKey("model_registry_entries.id")
    )
    billing_mode: Mapped[str] = mapped_column(String(24))
    billing_hold_id: Mapped[UUID] = mapped_column(index=True)
    request_fingerprint: Mapped[str] = mapped_column(String(64))
    status: Mapped[str] = mapped_column(String(32), default=TaskStatus.QUEUED, index=True)
    attempt_no: Mapped[int] = mapped_column(default=0)
    lease_expires_at: Mapped[datetime | None] = mapped_column(
        DateTime(timezone=True), index=True
    )
    idempotency_key: Mapped[str] = mapped_column(String(160))
    input_payload: Mapped[dict[str, object]] = mapped_column(JSONB)
    result_payload: Mapped[dict[str, object] | None] = mapped_column(JSONB)
    error_code: Mapped[str | None] = mapped_column(String(80))
    error_message: Mapped[str | None] = mapped_column(String(500))
    reconciliation_idempotency_key: Mapped[str | None] = mapped_column(String(160))
    reconciliation_fingerprint: Mapped[str | None] = mapped_column(String(64))
```

`RECONCILIATION_REQUIRED` is durable and non-terminal. It means retry ownership was exhausted before the system could prove whether a supplier call occurred. The original hold remains active, the ordinary generation delivery remains unacknowledged, and only `TaskReconciliationService` may move the task to `FAILED` after proving a zero-call path or persisting complete task-linked supplier cost. The 32-character column and `ck_task_status` constraint must be present in `0001_foundation`; later migrations do not widen or reinterpret this state.

Create `backend/src/ip_saas/modules/tasks/events.py` so a generic envelope cannot hide a malformed task payload:

```python
from typing import Literal
from uuid import UUID

from pydantic import BaseModel, ConfigDict, Field, model_validator

from ip_saas.common.audit import JSONValue
from ip_saas.common.outbox import EventEnvelope


class GenerationTaskRequestedPayload(BaseModel):
    model_config = ConfigDict(extra="forbid")

    task_id: UUID
    project_id: UUID
    capability: str = Field(min_length=1, max_length=80)
    model_registry_entry_id: UUID
    billing_mode: Literal["customer_credit", "internal_cost"]
    billing_hold_id: UUID
    request_fingerprint: str = Field(pattern=r"^[0-9a-f]{64}$")
    input_payload: dict[str, JSONValue]


class GenerationTaskRequestedV1(EventEnvelope):
    event_type: Literal["generation.task.requested"] = "generation.task.requested"
    payload: GenerationTaskRequestedPayload

    @model_validator(mode="after")
    def task_identity_matches_aggregate(self) -> "GenerationTaskRequestedV1":
        if self.aggregate_id != self.payload.task_id:
            raise ValueError("task payload must match aggregate_id")
        return self
```

- [ ] **Step 5: Write failing expired-lease and heartbeat/reclaim race tests**

Create `backend/tests/integration/tasks/test_task_lease_concurrency.py`:

```python
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from threading import Barrier
from unittest.mock import Mock
from uuid import UUID, uuid4

import pytest
from sqlalchemy import Engine, delete, select
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter
from ip_saas.common.errors import Conflict
from ip_saas.common.outbox import OutboxWriter
from ip_saas.common.tasking import TaskStatus
from ip_saas.db.base import Base
from ip_saas.modules.accounts.models import Account, AccountKind
from ip_saas.modules.billing.service import BillingService
from ip_saas.modules.model_registry.models import ModelLifecycle, ModelRegistryEntry
from ip_saas.modules.model_registry.service import ModelRegistryService
from ip_saas.modules.projects.access import ProjectAccessService
from ip_saas.modules.projects.models import IPProject, ProjectOwnerType
from ip_saas.modules.tasks.models import TaskRecord
from ip_saas.modules.tasks.service import TaskSubmissionService


class FrozenLeaseClock:
    def __init__(self) -> None:
        self.instant = datetime(2026, 8, 24, 8, 10, tzinfo=UTC)

    def now(self) -> datetime:
        return self.instant


@dataclass(frozen=True)
class LeaseRows:
    task_id: UUID
    project_id: UUID
    model_id: UUID
    account_id: UUID


def lease_service(clock: FrozenLeaseClock) -> TaskSubmissionService:
    return TaskSubmissionService(
        Mock(spec=ProjectAccessService),
        Mock(spec=BillingService),
        Mock(spec=ModelRegistryService),
        Mock(spec=AuditWriter),
        Mock(spec=OutboxWriter),
        clock,
    )


def seed_expired_task(pg_engine: Engine, clock: FrozenLeaseClock) -> LeaseRows:
    Base.metadata.create_all(pg_engine)
    capability = f"lease-race-{uuid4().hex}"
    with Session(pg_engine) as session:
        with session.begin():
            account = Account(kind=AccountKind.C_USER, display_name=capability)
            session.add(account)
            session.flush()
            project = IPProject(
                owner_type=ProjectOwnerType.C_USER,
                owner_account_id=account.id,
                name=capability,
                created_by_principal_id=uuid4(),
            )
            model = ModelRegistryEntry(
                capability=capability,
                provider="fake",
                model_id=capability,
                model_version="v1",
                input_modalities=["text"],
                output_modalities=["text"],
                parameter_schema={},
                pricing_version_id=None,
                safety_version="safe-v1",
                lifecycle=ModelLifecycle.ACTIVE,
                regression_passed=True,
                regression_report_ref="lease-test",
                created_by_principal_id=uuid4(),
            )
            session.add_all([project, model])
            session.flush()
            task = TaskRecord(
                project_id=project.id,
                initiated_by_actor_id=uuid4(),
                initiated_by_account_id=account.id,
                capability=capability,
                model_registry_entry_id=model.id,
                billing_mode="customer_credit",
                billing_hold_id=uuid4(),
                request_fingerprint="a" * 64,
                status=TaskStatus.RUNNING,
                attempt_no=1,
                lease_expires_at=clock.now() - timedelta(seconds=1),
                idempotency_key=f"expired-{uuid4().hex}",
                input_payload={"case": "expired"},
                result_payload=None,
                error_code=None,
                error_message=None,
                reconciliation_idempotency_key=None,
                reconciliation_fingerprint=None,
            )
            session.add(task)
            session.flush()
            return LeaseRows(task.id, project.id, model.id, account.id)


def cleanup_lease_rows(pg_engine: Engine, rows: LeaseRows) -> None:
    with Session(pg_engine) as session:
        with session.begin():
            session.execute(delete(TaskRecord).where(TaskRecord.id == rows.task_id))
            session.execute(delete(IPProject).where(IPProject.id == rows.project_id))
            session.execute(
                delete(ModelRegistryEntry).where(ModelRegistryEntry.id == rows.model_id)
            )
            session.execute(delete(Account).where(Account.id == rows.account_id))


def test_expired_unreclaimed_attempt_cannot_heartbeat_or_finish(
    pg_engine: Engine,
) -> None:
    clock = FrozenLeaseClock()
    rows = seed_expired_task(pg_engine, clock)
    try:
        with Session(pg_engine) as session:
            with session.begin():
                service = lease_service(clock)
                with pytest.raises(Conflict, match="lease was lost"):
                    service.heartbeat(session, rows.task_id, 1)
                with pytest.raises(Conflict, match="lease was lost"):
                    service.succeed(session, rows.task_id, 1, {"stale": True})
                with pytest.raises(Conflict, match="lease was lost"):
                    service.fail(session, rows.task_id, 1, "stale", "expired")
                stored = session.get(TaskRecord, rows.task_id)
                assert stored is not None
                assert stored.status == TaskStatus.RUNNING
                assert stored.attempt_no == 1
    finally:
        cleanup_lease_rows(pg_engine, rows)


def test_expired_heartbeat_loses_to_concurrent_reclaim(pg_engine: Engine) -> None:
    clock = FrozenLeaseClock()
    rows = seed_expired_task(pg_engine, clock)
    barrier = Barrier(2)

    def old_heartbeat() -> str:
        with Session(pg_engine) as session:
            with session.begin():
                barrier.wait(timeout=5)
                with pytest.raises(Conflict, match="lease was lost"):
                    lease_service(clock).heartbeat(session, rows.task_id, 1)
                return "heartbeat_conflict"

    def reclaim() -> str:
        with Session(pg_engine) as session:
            with session.begin():
                barrier.wait(timeout=5)
                claimed = lease_service(clock).start(session, rows.task_id)
                return f"reclaimed_{claimed.attempt_no}"

    try:
        with ThreadPoolExecutor(max_workers=2) as pool:
            outcomes = [pool.submit(old_heartbeat), pool.submit(reclaim)]
            assert sorted(future.result(timeout=10) for future in outcomes) == [
                "heartbeat_conflict",
                "reclaimed_2",
            ]
        with Session(pg_engine) as session:
            stored = session.scalar(select(TaskRecord).where(TaskRecord.id == rows.task_id))
            assert stored is not None
            assert stored.status == TaskStatus.RUNNING
            assert stored.attempt_no == 2
            assert stored.lease_expires_at == clock.now() + timedelta(seconds=300)
    finally:
        cleanup_lease_rows(pg_engine, rows)
```

This file uses Plan 01's real PostgreSQL `pg_engine`, two independent Sessions and transactions, and no BillingService fake or provider call. It directly seeds only the prerequisite account/project/model/task rows so Plan 05's later “no allow fake in integration” contract remains true.

- [ ] **Step 6: Run the new lease tests and observe the red result**

Run: `cd backend && uv run pytest tests/integration/tasks/test_task_lease_concurrency.py -v`

Expected: FAIL before the implementation change: the expired attempt can still heartbeat or finish because the CAS lacks `lease_expires_at > now`; the race can allow the old heartbeat to renew attempt 1 instead of deterministically reclaiming attempt 2.

- [ ] **Step 7: Implement atomic task submission, lease-fenced transitions, and explicit reconciliation**

Create `backend/src/ip_saas/modules/tasks/service.py`:

```python
import hashlib
import json
import re
from datetime import timedelta
from typing import Mapping
from uuid import UUID, uuid4

from sqlalchemy import select, update
from sqlalchemy.orm import Session

from ip_saas.common.audit import AuditWriter, JSONValue
from ip_saas.common.clock import Clock
from ip_saas.common.errors import Conflict
from ip_saas.common.outbox import OutboxWriter
from ip_saas.common.tasking import TaskStatus
from ip_saas.modules.accounts.context import ActorContext
from ip_saas.modules.billing.service import BillingMode, BillingService
from ip_saas.modules.model_registry.service import ModelRegistryService
from ip_saas.modules.projects.access import ProjectAccessService
from ip_saas.modules.tasks.events import GenerationTaskRequestedV1
from ip_saas.modules.tasks.models import TaskRecord


class TaskSubmissionService:
    def __init__(
        self,
        project_access: ProjectAccessService,
        billing: BillingService,
        models: ModelRegistryService,
        audit: AuditWriter,
        outbox: OutboxWriter,
        clock: Clock,
    ) -> None:
        self.project_access = project_access
        self.billing = billing
        self.models = models
        self.audit = audit
        self.outbox = outbox
        self.clock = clock

    @staticmethod
    def _request_fingerprint(
        *,
        actor_account_id: UUID,
        project_id: UUID,
        capability: str,
        billing_mode: BillingMode,
        billing_scope_id: UUID,
        maximum_amount: int,
        input_payload: Mapping[str, JSONValue],
    ) -> str:
        canonical = json.dumps(
            {
                "actor_account_id": str(actor_account_id),
                "project_id": str(project_id),
                "capability": capability,
                "billing_mode": billing_mode.value,
                "billing_scope_id": str(billing_scope_id),
                "maximum_amount": maximum_amount,
                "input_payload": dict(input_payload),
            },
            ensure_ascii=False,
            allow_nan=False,
            separators=(",", ":"),
            sort_keys=True,
        ).encode("utf-8")
        return hashlib.sha256(canonical).hexdigest()

    def submit_customer(
        self,
        session: Session,
        actor: ActorContext,
        project_id: UUID,
        capability: str,
        max_credit_units: int,
        idempotency_key: str,
        input_payload: Mapping[str, JSONValue],
    ) -> TaskRecord:
        project = self.project_access.require_viewer(session, actor, project_id)
        fingerprint = self._request_fingerprint(
            actor_account_id=actor.account_id,
            project_id=project.id,
            capability=capability,
            billing_mode=BillingMode.CUSTOMER_CREDIT,
            billing_scope_id=actor.account_id,
            maximum_amount=max_credit_units,
            input_payload=input_payload,
        )
        existing = session.scalar(
            select(TaskRecord).where(
                TaskRecord.initiated_by_account_id == actor.account_id,
                TaskRecord.idempotency_key == idempotency_key,
            )
        )
        if existing is not None:
            if existing.request_fingerprint != fingerprint:
                raise Conflict("idempotency key was already used with different task input")
            return existing
        project = self.project_access.require_editor(session, actor, project_id)
        model = self.models.require_active(session, capability)
        billing = self.billing.reserve_customer_generation(
            session,
            actor.account_id,
            max_credit_units,
            "task:"
            + str(actor.account_id)
            + ":"
            + hashlib.sha256(idempotency_key.encode("utf-8")).hexdigest()
            + ":hold",
            actor,
        )
        task = TaskRecord(
            project_id=project.id,
            initiated_by_actor_id=actor.actor_id,
            initiated_by_account_id=actor.account_id,
            capability=capability,
            model_registry_entry_id=model.id,
            billing_mode=billing.mode,
            billing_hold_id=billing.hold_id,
            request_fingerprint=fingerprint,
            idempotency_key=idempotency_key,
            input_payload=dict(input_payload),
            result_payload=None,
            error_code=None,
            error_message=None,
        )
        session.add(task)
        session.flush()
        event = GenerationTaskRequestedV1(
            event_id=uuid4(),
            event_type="generation.task.requested",
            schema_version=1,
            aggregate_id=task.id,
            occurred_at=self.clock.now(),
            initiated_by_actor_id=actor.actor_id,
            idempotency_key=f"task:{task.id}",
            payload={
                "task_id": str(task.id),
                "project_id": str(project.id),
                "capability": capability,
                "model_registry_entry_id": str(model.id),
                "billing_mode": billing.mode,
                "billing_hold_id": str(billing.hold_id),
                "request_fingerprint": fingerprint,
                "input_payload": dict(input_payload),
            },
        )
        self.audit.write(
            session,
            actor=actor,
            action="generation.task.created",
            target_type="task_record",
            target_id=task.id,
            project_id=project.id,
            metadata={"capability": capability},
        )
        self.outbox.add(session, event)
        return task

    def submit_internal(
        self,
        session: Session,
        actor: ActorContext,
        project_id: UUID,
        capability: str,
        cost_center_id: UUID,
        max_amount_fen: int,
        idempotency_key: str,
        input_payload: Mapping[str, JSONValue],
    ) -> TaskRecord:
        project = self.project_access.require_viewer(session, actor, project_id)
        if project.owner_type != "platform":
            raise Conflict("internal billing requires a platform project")
        fingerprint = self._request_fingerprint(
            actor_account_id=actor.account_id,
            project_id=project.id,
            capability=capability,
            billing_mode=BillingMode.INTERNAL_COST,
            billing_scope_id=cost_center_id,
            maximum_amount=max_amount_fen,
            input_payload=input_payload,
        )
        existing = session.scalar(
            select(TaskRecord).where(
                TaskRecord.initiated_by_account_id == actor.account_id,
                TaskRecord.idempotency_key == idempotency_key,
            )
        )
        if existing is not None:
            if existing.request_fingerprint != fingerprint:
                raise Conflict("idempotency key was already used with different task input")
            return existing
        project = self.project_access.require_editor(session, actor, project_id)
        model = self.models.require_active(session, capability)
        billing = self.billing.reserve_internal_generation(
            session,
            cost_center_id,
            max_amount_fen,
            "task:"
            + str(actor.account_id)
            + ":"
            + hashlib.sha256(idempotency_key.encode("utf-8")).hexdigest()
            + ":hold",
            actor,
        )
        task = TaskRecord(
            project_id=project.id,
            initiated_by_actor_id=actor.actor_id,
            initiated_by_account_id=actor.account_id,
            capability=capability,
            model_registry_entry_id=model.id,
            billing_mode=billing.mode,
            billing_hold_id=billing.hold_id,
            request_fingerprint=fingerprint,
            idempotency_key=idempotency_key,
            input_payload=dict(input_payload),
            result_payload=None,
            error_code=None,
            error_message=None,
        )
        session.add(task)
        session.flush()
        self.audit.write(
            session,
            actor=actor,
            action="generation.task.created",
            target_type="task_record",
            target_id=task.id,
            project_id=project.id,
            metadata={"capability": capability, "billing_mode": billing.mode},
        )
        self.outbox.add(
            session,
            GenerationTaskRequestedV1(
                event_id=uuid4(),
                event_type="generation.task.requested",
                schema_version=1,
                aggregate_id=task.id,
                occurred_at=self.clock.now(),
                initiated_by_actor_id=actor.actor_id,
                idempotency_key=f"task:{task.id}",
                payload={
                    "task_id": str(task.id),
                    "project_id": str(project.id),
                    "capability": capability,
                    "model_registry_entry_id": str(model.id),
                    "billing_mode": billing.mode,
                    "billing_hold_id": str(billing.hold_id),
                    "request_fingerprint": fingerprint,
                    "input_payload": dict(input_payload),
                },
            ),
        )
        return task

    def start(
        self,
        session: Session,
        task_id: UUID,
        lease_seconds: int = 300,
        max_attempts: int = 8,
    ) -> TaskRecord:
        if lease_seconds < 30 or max_attempts < 1:
            raise ValueError("invalid task lease policy")
        task = session.scalar(
            select(TaskRecord).where(TaskRecord.id == task_id).with_for_update()
        )
        if task is None:
            raise Conflict("task does not exist")
        current = TaskStatus(task.status)
        if current in {TaskStatus.SUCCEEDED, TaskStatus.FAILED, TaskStatus.CANCELLED}:
            return task
        if current == TaskStatus.RECONCILIATION_REQUIRED:
            return task
        now = self.clock.now()
        if (
            current == TaskStatus.RUNNING
            and task.lease_expires_at is not None
            and task.lease_expires_at > now
        ):
            raise Conflict("task lease is still active")
        if task.attempt_no >= max_attempts:
            task.status = TaskStatus.RECONCILIATION_REQUIRED
            task.lease_expires_at = None
            task.error_code = "task_retry_exhausted_reconciliation_required"
            task.error_message = "任务重试次数已耗尽，正在核对供应商调用和费用；原预留尚未释放。"
            session.flush()
            return task
        task.status = TaskStatus.RUNNING
        task.attempt_no += 1
        task.lease_expires_at = now + timedelta(seconds=lease_seconds)
        session.flush()
        return task

    def heartbeat(
        self,
        session: Session,
        task_id: UUID,
        attempt_no: int,
        lease_seconds: int = 300,
    ) -> TaskRecord:
        if lease_seconds < 30:
            raise ValueError("invalid task lease policy")
        now = self.clock.now()
        changed = session.execute(
            update(TaskRecord)
            .where(
                TaskRecord.id == task_id,
                TaskRecord.status == TaskStatus.RUNNING,
                TaskRecord.attempt_no == attempt_no,
                TaskRecord.lease_expires_at.is_not(None),
                TaskRecord.lease_expires_at > now,
            )
            .values(lease_expires_at=now + timedelta(seconds=lease_seconds))
        )
        if changed.rowcount != 1:
            raise Conflict("task lease was lost")
        task = session.get(TaskRecord, task_id)
        if task is None:
            raise Conflict("task does not exist")
        return task

    def succeed(
        self,
        session: Session,
        task_id: UUID,
        attempt_no: int,
        result_payload: Mapping[str, JSONValue],
    ) -> TaskRecord:
        now = self.clock.now()
        changed = session.execute(
            update(TaskRecord)
            .where(
                TaskRecord.id == task_id,
                TaskRecord.status == TaskStatus.RUNNING,
                TaskRecord.attempt_no == attempt_no,
                TaskRecord.lease_expires_at.is_not(None),
                TaskRecord.lease_expires_at > now,
            )
            .values(
                status=TaskStatus.SUCCEEDED,
                lease_expires_at=None,
                result_payload=dict(result_payload),
                error_code=None,
                error_message=None,
            )
        )
        if changed.rowcount != 1:
            raise Conflict("task lease was lost")
        task = session.get(TaskRecord, task_id)
        if task is None:
            raise Conflict("task does not exist")
        return task

    def fail(
        self,
        session: Session,
        task_id: UUID,
        attempt_no: int,
        error_code: str,
        error_message: str,
    ) -> TaskRecord:
        sanitized = " ".join(error_message.split())
        sanitized = re.sub(r"(?i)(bearer\s+)[A-Za-z0-9._-]+", r"\1[redacted]", sanitized)
        sanitized = re.sub(
            r"(?i)(password|secret|token)=\S+", r"\1=[redacted]", sanitized
        )[:500]
        now = self.clock.now()
        changed = session.execute(
            update(TaskRecord)
            .where(
                TaskRecord.id == task_id,
                TaskRecord.status == TaskStatus.RUNNING,
                TaskRecord.attempt_no == attempt_no,
                TaskRecord.lease_expires_at.is_not(None),
                TaskRecord.lease_expires_at > now,
            )
            .values(
                status=TaskStatus.FAILED,
                lease_expires_at=None,
                error_code=error_code[:80],
                error_message=sanitized,
            )
        )
        if changed.rowcount != 1:
            raise Conflict("task lease was lost")
        task = session.get(TaskRecord, task_id)
        if task is None:
            raise Conflict("task does not exist")
        return task
```

Create `backend/src/ip_saas/modules/tasks/reconciliation.py`. This is the only retry-exhaustion finalizer; ordinary feature Workers must not release or settle an exhausted task directly:

```python
import hashlib
import json
import re
from dataclasses import asdict, dataclass
from typing import Protocol
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.errors import Conflict
from ip_saas.common.tasking import TaskStatus
from ip_saas.modules.billing.models import ReconciliationStatus
from ip_saas.modules.billing.service import (
    BillingContext,
    BillingMode,
    BillingService,
    ProviderCostInput,
)
from ip_saas.modules.tasks.models import TaskRecord


@dataclass(frozen=True)
class NoProviderCallEvidence:
    task_id: UUID
    through_attempt_no: int
    evidence_ref: str
    evidence_sha256: str

    def __post_init__(self) -> None:
        if not self.evidence_ref.strip():
            raise ValueError("zero-call evidence_ref is required")
        if re.fullmatch(r"[0-9a-f]{64}", self.evidence_sha256) is None:
            raise ValueError("zero-call evidence_sha256 must be lowercase SHA-256")


class NoProviderCallEvidencePort(Protocol):
    def require_no_provider_call(
        self,
        session: Session,
        task_id: UUID,
        through_attempt_no: int,
    ) -> NoProviderCallEvidence: ...


@dataclass(frozen=True, order=True)
class ProviderRequestIdentity:
    provider: str
    provider_request_id: str

    def __post_init__(self) -> None:
        if not self.provider.strip() or not self.provider_request_id.strip():
            raise ValueError("provider request identity cannot be empty")


class ProviderCostManifestPort(Protocol):
    def require_complete_provider_requests(
        self,
        session: Session,
        task_id: UUID,
        through_attempt_no: int,
    ) -> tuple[ProviderRequestIdentity, ...]: ...


class TaskReconciliationService:
    def __init__(
        self,
        billing: BillingService,
        no_provider_call_evidence: NoProviderCallEvidencePort,
        provider_cost_manifest: ProviderCostManifestPort,
    ) -> None:
        self.billing = billing
        self.no_provider_call_evidence = no_provider_call_evidence
        self.provider_cost_manifest = provider_cost_manifest

    @staticmethod
    def _fingerprint(payload: dict[str, object]) -> str:
        canonical = json.dumps(
            payload,
            default=str,
            ensure_ascii=False,
            allow_nan=False,
            separators=(",", ":"),
            sort_keys=True,
        ).encode("utf-8")
        return hashlib.sha256(canonical).hexdigest()

    @staticmethod
    def _lock(session: Session, task_id: UUID) -> TaskRecord:
        task = session.scalar(
            select(TaskRecord).where(TaskRecord.id == task_id).with_for_update()
        )
        if task is None:
            raise Conflict("task does not exist")
        return task

    @staticmethod
    def _is_replay(
        task: TaskRecord,
        attempt_no: int,
        idempotency_key: str,
        fingerprint: str,
    ) -> bool:
        if task.attempt_no != attempt_no:
            raise Conflict("reconciliation attempt does not match task")
        status = TaskStatus(task.status)
        if status == TaskStatus.FAILED:
            if (
                task.reconciliation_idempotency_key == idempotency_key
                and task.reconciliation_fingerprint == fingerprint
            ):
                return True
            raise Conflict("task was finalized by a different reconciliation input")
        if status != TaskStatus.RECONCILIATION_REQUIRED:
            raise Conflict("task is not waiting for reconciliation")
        return False

    @staticmethod
    def _mark_failed(
        task: TaskRecord,
        reason: str,
        idempotency_key: str,
        fingerprint: str,
    ) -> None:
        task.status = TaskStatus.FAILED
        task.lease_expires_at = None
        task.error_code = "task_retry_exhausted"
        task.error_message = " ".join(reason.split())[:500]
        task.reconciliation_idempotency_key = idempotency_key
        task.reconciliation_fingerprint = fingerprint

    def finalize_no_provider_call(
        self,
        session: Session,
        task_id: UUID,
        attempt_no: int,
        reason: str,
        idempotency_key: str,
    ) -> TaskRecord:
        task = self._lock(session, task_id)
        proof = self.no_provider_call_evidence.require_no_provider_call(
            session, task.id, attempt_no
        )
        if proof.task_id != task.id or proof.through_attempt_no != attempt_no:
            raise Conflict("zero-call evidence does not cover this task attempt")
        fingerprint = self._fingerprint(
            {
                "kind": "no_provider_call",
                "task_id": task.id,
                "attempt_no": attempt_no,
                "reason": reason,
                "evidence_ref": proof.evidence_ref,
                "evidence_sha256": proof.evidence_sha256,
            }
        )
        if self._is_replay(task, attempt_no, idempotency_key, fingerprint):
            return task
        self.billing.release_generation(
            session,
            BillingContext(BillingMode(task.billing_mode), task.billing_hold_id),
            "retry_exhausted_no_provider_call",
            f"task-reconciliation:{task.id}:{fingerprint}:release",
        )
        self._mark_failed(task, reason, idempotency_key, fingerprint)
        session.flush()
        return task

    def finalize_provider_costs(
        self,
        session: Session,
        task_id: UUID,
        attempt_no: int,
        actual_amount: int,
        provider_costs: tuple[ProviderCostInput, ...],
        reason: str,
        idempotency_key: str,
    ) -> TaskRecord:
        task = self._lock(session, task_id)
        if not provider_costs:
            raise Conflict("provider cost reconciliation cannot be empty")
        if any(cost.task_id != task.id for cost in provider_costs):
            raise Conflict("every provider cost must reference the reconciled task")
        if any(
            cost.reconciliation_status != ReconciliationStatus.MATCHED
            for cost in provider_costs
        ):
            raise Conflict("every provider cost must be supplier-reconciled")
        if any(
            not cost.provider.strip() or not (cost.provider_request_id or "").strip()
            for cost in provider_costs
        ):
            raise Conflict("every provider cost must include provider request identity")
        provided_identities = tuple(
            ProviderRequestIdentity(
                provider=cost.provider,
                provider_request_id=cost.provider_request_id or "",
            )
            for cost in provider_costs
        )
        if len(set(provided_identities)) != len(provided_identities):
            raise Conflict("provider request identity is duplicated")
        expected_identities = self.provider_cost_manifest.require_complete_provider_requests(
            session, task.id, attempt_no
        )
        if not expected_identities or len(set(expected_identities)) != len(expected_identities):
            raise Conflict("provider request manifest is empty or duplicated")
        if set(provided_identities) != set(expected_identities):
            raise Conflict("provider cost set does not cover the complete request manifest")
        mode = BillingMode(task.billing_mode)
        if mode == BillingMode.CUSTOMER_CREDIT and actual_amount != 0:
            raise Conflict("failed customer work settles zero customer credits")
        total_fen = sum(cost.amount_fen for cost in provider_costs)
        if mode == BillingMode.INTERNAL_COST and actual_amount != total_fen:
            raise Conflict("failed internal work must settle the complete supplier CNY cost")
        ordered_costs = tuple(
            sorted(
                provider_costs,
                key=lambda cost: (cost.provider, cost.provider_request_id or ""),
            )
        )
        fingerprint = self._fingerprint(
            {
                "kind": "provider_costs",
                "task_id": task.id,
                "attempt_no": attempt_no,
                "actual_amount": actual_amount,
                "provider_costs": [asdict(cost) for cost in ordered_costs],
                "request_manifest": [asdict(identity) for identity in sorted(expected_identities)],
                "reason": reason,
            }
        )
        if self._is_replay(task, attempt_no, idempotency_key, fingerprint):
            return task
        self.billing.settle_generation_batch(
            session,
            BillingContext(mode, task.billing_hold_id),
            actual_amount,
            ordered_costs,
            f"task-reconciliation:{task.id}:{fingerprint}",
        )
        self._mark_failed(task, reason, idempotency_key, fingerprint)
        session.flush()
        return task
```

Neither evidence port has a permissive or production default. A feature adapter implements both over its durable provider-attempt journal and signed supplier evidence; an operator-supplied boolean, an empty journal created after the fact, or absence of `ProviderCostEntry` alone is not proof. `finalize_provider_costs` requires a non-empty tuple whose `(provider, provider_request_id)` set exactly equals the durable manifest through the exhausted attempt, so omitting or duplicating one paid call fails before billing. The batch billing method settles/releases the hold once and inserts every cost row in the same caller-owned transaction. Both finalizers store a reconciliation fingerprint: exact replay, including a differently ordered but identical cost tuple, returns the same failed task, while changed evidence, cost set, reason, or attempt conflicts.

- [ ] **Step 8: Seed one active fake model inside the task test**

Before constructing `TaskSubmissionService` in `test_hold_task_audit_and_outbox_rollback_together`, insert:

```python
platform_admin = ActorContext(uuid4(), uuid4(), ActorKind.PLATFORM_ADMIN)
registry = ModelRegistryService()
candidate = registry.register_candidate(
    db_session,
    platform_admin,
    "text_strategy",
    "fake",
    "fake-text",
    "v1",
    ["text"],
    ["text"],
    {},
    None,
    "safe-v1",
)
registry.record_regression(db_session, platform_admin, candidate.id, passed=True, report_ref="test")
registry.activate(db_session, platform_admin, candidate.id)
```

- [ ] **Step 9: Run atomic rollback and cross-session settlement-replay tests**

Run: `cd backend && uv run pytest tests/integration/tasks/test_task_submission.py -v`

Expected: PASS; task, audit, outbox, and active hold counts remain zero after the injected exception; an exact settlement replay in a fresh SQLAlchemy session reuses one row, while changed request ID, task ID, settlement amount, or supplier amount raises `Conflict`.

- [ ] **Step 10: Extend the existing task test with immutable idempotency and terminal results**

Append this code to `test_hold_task_audit_and_outbox_rollback_together` after its rollback assertions:

```python
working = TaskSubmissionService(
    ProjectAccessService(),
    BillingService(
        SqlCreditHoldPort(),
        SqlInternalBudgetHoldPort(),
        AllowAllGenerationLimits(),
    ),
    ModelRegistryService(),
    AuditWriter(FixedClock()),
    OutboxWriter(FixedClock()),
    FixedClock(),
)
first = working.submit_customer(
    db_session, actor, project.id, "text_strategy", 100, "task-once", {"brief": "gold"}
)
second = working.submit_customer(
    db_session, actor, project.id, "text_strategy", 100, "task-once", {"brief": "gold"}
)
assert first.id == second.id
assert len(first.request_fingerprint) == 64
assert db_session.scalar(select(func.count()).select_from(TaskRecord)) == 1
with pytest.raises(Conflict):
    working.submit_customer(
        db_session, actor, project.id, "text_strategy", 100, "task-once", {"brief": "changed"}
    )
with pytest.raises(Conflict):
    working.submit_customer(
        db_session, actor, project.id, "text_strategy", 101, "task-once", {"brief": "gold"}
    )

other_account = Account(kind=AccountKind.C_USER, display_name="Other customer")
db_session.add(other_account)
db_session.flush()
other_principal = Principal(
    account_id=other_account.id,
    login_name="other@example.com",
    password_hash="hash",
)
db_session.add(other_principal)
db_session.flush()
other_actor = ActorContext(other_principal.id, other_account.id, ActorKind.C_USER)
with pytest.raises(NotFound):
    working.submit_customer(
        db_session,
        other_actor,
        project.id,
        "text_strategy",
        100,
        "task-once",
        {"brief": "gold"},
    )
other_project = ProjectService(ProjectAccessService()).create(
    db_session, other_actor, "Other customer's fruit project"
)
other_wallet = ledger.ensure_account_wallet(db_session, other_account.id)
ledger.issue(
    db_session,
    treasury.id,
    other_wallet.id,
    500,
    actor.actor_id,
    "seed-credit-other-account",
)
other_task = working.submit_customer(
    db_session,
    other_actor,
    other_project.id,
    "text_strategy",
    100,
    "task-once",
    {"brief": "gold"},
)
assert other_task.id != first.id
assert other_task.initiated_by_account_id == other_account.id
assert db_session.scalar(select(func.count()).select_from(TaskRecord)) == 2
first_lease = working.start(db_session, first.id)
working.heartbeat(db_session, first.id, first_lease.attempt_no)
working.succeed(db_session, first.id, first_lease.attempt_no, {"asset_id": "result-1"})
assert first.result_payload == {"asset_id": "result-1"}
assert working.start(db_session, first.id).status == "succeeded"

failed = working.submit_customer(
    db_session, actor, project.id, "text_strategy", 100, "task-fail", {"brief": "test"}
)
failed_lease = working.start(db_session, failed.id)
working.fail(
    db_session,
    failed.id,
    failed_lease.attempt_no,
    "provider_error",
    "Bearer super-secret-token password=do-not-log " + "x" * 600,
)
assert "super-secret-token" not in (failed.error_message or "")
assert "do-not-log" not in (failed.error_message or "")
assert len(failed.error_message or "") <= 500

reclaimed = working.submit_customer(
    db_session, actor, project.id, "text_strategy", 100, "task-reclaim", {"brief": "lease"}
)
attempt_one = working.start(db_session, reclaimed.id)
with pytest.raises(Conflict, match="still active"):
    working.start(db_session, reclaimed.id)
attempt_one.lease_expires_at = FixedClock().now() - timedelta(seconds=1)
db_session.flush()
with pytest.raises(Conflict, match="lease was lost"):
    working.heartbeat(db_session, reclaimed.id, attempt_one.attempt_no)
with pytest.raises(Conflict, match="lease was lost"):
    working.succeed(db_session, reclaimed.id, attempt_one.attempt_no, {"expired": True})
with pytest.raises(Conflict, match="lease was lost"):
    working.fail(
        db_session, reclaimed.id, attempt_one.attempt_no, "expired", "expired lease"
    )
assert reclaimed.status == "running"
assert reclaimed.attempt_no == 1
attempt_two = working.start(db_session, reclaimed.id)
assert attempt_two.attempt_no == 2
with pytest.raises(Conflict, match="lease was lost"):
    working.succeed(db_session, reclaimed.id, 1, {"stale": True})
working.succeed(db_session, reclaimed.id, 2, {"fresh": True})

exhausted = working.submit_customer(
    db_session, actor, project.id, "text_strategy", 100, "task-exhaust", {"brief": "retry"}
)
last_attempt = working.start(db_session, exhausted.id, max_attempts=1)
last_attempt.lease_expires_at = FixedClock().now() - timedelta(seconds=1)
db_session.flush()
terminal = working.start(db_session, exhausted.id, max_attempts=1)
assert terminal.status == "reconciliation_required"
assert terminal.error_code == "task_retry_exhausted_reconciliation_required"
assert terminal.reconciliation_idempotency_key is None
assert terminal.reconciliation_fingerprint is None
assert db_session.get(GenerationHold, exhausted.billing_hold_id).status == HoldStatus.ACTIVE
reconciler = TaskReconciliationService(
    working.billing,
    StaticNoProviderCallEvidence(),
    StaticProviderCostManifest(()),
)
failed_after_proof = reconciler.finalize_no_provider_call(
    db_session,
    exhausted.id,
    terminal.attempt_no,
    "retry exhausted before any provider request",
    "reconcile-task-exhaust-no-call",
)
same_failed = reconciler.finalize_no_provider_call(
    db_session,
    exhausted.id,
    terminal.attempt_no,
    "retry exhausted before any provider request",
    "reconcile-task-exhaust-no-call",
)
assert same_failed.id == failed_after_proof.id
assert failed_after_proof.status == "failed"
assert db_session.get(GenerationHold, exhausted.billing_hold_id).status == HoldStatus.RELEASED
with pytest.raises(Conflict, match="different reconciliation input"):
    reconciler.finalize_no_provider_call(
        db_session,
        exhausted.id,
        terminal.attempt_no,
        "changed reason",
        "reconcile-task-exhaust-no-call",
    )
```

- [ ] **Step 11: Add the platform internal-task and non-zero cost reconciliation test**

Append this test to `backend/tests/integration/tasks/test_task_submission.py`:

```python
def test_platform_project_uses_internal_hold_and_same_task_shape(db_session: Session) -> None:
    platform = Account(kind=AccountKind.PLATFORM, display_name="Platform")
    db_session.add(platform)
    db_session.flush()
    principal = Principal(account_id=platform.id, login_name="operator@example.com", password_hash="hash")
    db_session.add(principal)
    db_session.flush()
    operator = ActorContext(principal.id, platform.id, ActorKind.PLATFORM_OPERATOR)
    admin = ActorContext(principal.id, platform.id, ActorKind.PLATFORM_ADMIN)
    project = ProjectService(ProjectAccessService()).create_platform_project(
        db_session, operator, "Self marketing"
    )
    internal = InternalBudgetService()
    center = internal.create_center(db_session, platform.id, "marketing", "Marketing")
    internal.allocate(db_session, center.id, 20_000, admin.actor_id, "budget-1")
    registry = ModelRegistryService()
    candidate = registry.register_candidate(
        db_session, admin, "text_strategy", "fake", "fake-text", "v1", ["text"], ["text"], {}, None, "s1"
    )
    registry.record_regression(db_session, admin, candidate.id, passed=True, report_ref="test")
    registry.activate(db_session, admin, candidate.id)
    billing = BillingService(
        SqlCreditHoldPort(),
        SqlInternalBudgetHoldPort(internal),
        AllowAllGenerationLimits(),
    )
    service = TaskSubmissionService(
        ProjectAccessService(),
        billing,
        registry,
        AuditWriter(FixedClock()),
        OutboxWriter(FixedClock()),
        FixedClock(),
    )

    task = service.submit_internal(
        db_session,
        operator,
        project.id,
        "text_strategy",
        center.id,
        5_000,
        "internal-task-1",
        {"brief": "market the platform"},
    )

    assert task.billing_mode == "internal_cost"
    assert task.input_payload == {"brief": "market the platform"}
    assert internal.available_fen(db_session, center.id) == 15_000

    first_claim = service.start(db_session, task.id, max_attempts=2)
    first_claim.lease_expires_at = FixedClock().now() - timedelta(seconds=1)
    db_session.flush()
    second_claim = service.start(db_session, task.id, max_attempts=2)
    assert second_claim.attempt_no == 2
    second_claim.lease_expires_at = FixedClock().now() - timedelta(seconds=1)
    db_session.flush()
    pending = service.start(db_session, task.id, max_attempts=2)
    assert pending.status == "reconciliation_required"
    assert db_session.get(InternalBudgetHold, task.billing_hold_id).status == HoldStatus.ACTIVE

    # PoC: two supplier responses were durably journaled across attempts, but the
    # Worker crashed after each response and before any ProviderCostEntry write.
    first_request = ProviderRequestIdentity("volcengine", "provider-request-1")
    second_request = ProviderRequestIdentity("volcengine", "provider-request-2")
    reconciler = TaskReconciliationService(
        billing,
        StaticNoProviderCallEvidence(),
        StaticProviderCostManifest((first_request, second_request)),
    )
    first_cost = ProviderCostInput(
        provider="volcengine",
        capability="text_strategy",
        model_id="fake-text",
        model_version="v1",
        native_quantity=Decimal("1"),
        native_unit="response",
        supplier_amount_minor=1_200,
        supplier_currency="CNY",
        amount_fen=1_200,
        reconciliation_status=ReconciliationStatus.MATCHED,
        task_id=task.id,
        provider_request_id=first_request.provider_request_id,
    )
    second_cost = ProviderCostInput(
        provider="volcengine",
        capability="text_strategy",
        model_id="fake-text",
        model_version="v1",
        native_quantity=Decimal("1"),
        native_unit="response",
        supplier_amount_minor=1_800,
        supplier_currency="CNY",
        amount_fen=1_800,
        reconciliation_status=ReconciliationStatus.MATCHED,
        task_id=task.id,
        provider_request_id=second_request.provider_request_id,
    )
    with pytest.raises(Conflict, match="reference the reconciled task"):
        reconciler.finalize_provider_costs(
            db_session,
            task.id,
            pending.attempt_no,
            3_000,
            (replace(first_cost, task_id=uuid4()), second_cost),
            "provider completed before retry exhaustion",
            "reconcile-wrong-task",
        )
    with pytest.raises(Conflict, match="supplier-reconciled"):
        reconciler.finalize_provider_costs(
            db_session,
            task.id,
            pending.attempt_no,
            3_000,
            (
                replace(
                    first_cost,
                    reconciliation_status=ReconciliationStatus.UNRECONCILED,
                ),
                second_cost,
            ),
            "provider completed before retry exhaustion",
            "reconcile-unmatched-cost",
        )
    with pytest.raises(Conflict, match="duplicated"):
        reconciler.finalize_provider_costs(
            db_session,
            task.id,
            pending.attempt_no,
            3_000,
            (
                first_cost,
                replace(second_cost, provider_request_id=first_request.provider_request_id),
            ),
            "provider completed before retry exhaustion",
            "reconcile-duplicate-request",
        )
    with pytest.raises(Conflict, match="complete request manifest"):
        reconciler.finalize_provider_costs(
            db_session,
            task.id,
            pending.attempt_no,
            1_200,
            (first_cost,),
            "provider completed before retry exhaustion",
            "reconcile-missing-provider-call",
        )
    with pytest.raises(Conflict, match="complete supplier CNY cost"):
        reconciler.finalize_provider_costs(
            db_session,
            task.id,
            pending.attempt_no,
            2_999,
            (first_cost, second_cost),
            "provider completed before retry exhaustion",
            "reconcile-wrong-total",
        )
    assert db_session.get(InternalBudgetHold, task.billing_hold_id).status == HoldStatus.ACTIVE
    assert db_session.scalar(
        select(func.count()).select_from(ProviderCostEntry).where(
            ProviderCostEntry.task_id == task.id
        )
    ) == 0
    failed = reconciler.finalize_provider_costs(
        db_session,
        task.id,
        pending.attempt_no,
        3_000,
        (first_cost, second_cost),
        "provider completed before retry exhaustion",
        "reconcile-internal-provider-cost",
    )
    same_failed = reconciler.finalize_provider_costs(
        db_session,
        task.id,
        pending.attempt_no,
        3_000,
        (second_cost, first_cost),
        "provider completed before retry exhaustion",
        "reconcile-internal-provider-cost",
    )
    stored_costs = tuple(
        db_session.scalars(
            select(ProviderCostEntry)
            .where(ProviderCostEntry.task_id == task.id)
            .order_by(ProviderCostEntry.provider_request_id)
        )
    )
    assert same_failed.id == failed.id
    assert failed.status == "failed"
    assert len(stored_costs) == 2
    assert sum(row.amount_fen for row in stored_costs) == 3_000
    assert {row.task_id for row in stored_costs} == {task.id}
    assert {row.provider_request_id for row in stored_costs} == {
        first_request.provider_request_id,
        second_request.provider_request_id,
    }
    assert db_session.get(InternalBudgetHold, task.billing_hold_id).status == HoldStatus.SETTLED
    assert internal.available_fen(db_session, center.id) == 17_000
```

The imports added in Step 1 already include `Decimal`, `InternalBudgetService`, `ProviderCostInput`, `ProviderCostEntry`, `InternalBudgetHold`, `ReconciliationStatus`, `NoProviderCallEvidence`, `ProviderRequestIdentity`, and `TaskReconciliationService`; do not add a second import block. This is the first non-zero supplier-cost test in the plan and the required post-response/pre-cost-persistence crash PoC: it models two supplier calls across the exhausted task, proves no cost row existed at crash recovery, rejects a missing call, and persists both rows with the real `TaskRecord.id` before settling the hold exactly once.

- [ ] **Step 12: Write failing at-least-once consumer tests**

Create `backend/tests/unit/tasks/test_task_consumer.py`:

```python
import json
from datetime import UTC, datetime
from types import SimpleNamespace
from uuid import UUID

from ip_saas.modules.tasks.events import GenerationTaskRequestedV1
from ip_saas.workers.task_consumer import RocketMQTaskConsumer, TaskHandlerDisposition


class MemoryConsumer:
    def __init__(self, bodies: list[bytes]) -> None:
        self.deliveries = [SimpleNamespace(body=body) for body in bodies]
        self.acked: list[object] = []

    def receive(self, _maximum: int, _invisible_seconds: int) -> list[object]:
        return self.deliveries

    def ack(self, delivery: object) -> None:
        self.acked.append(delivery)


def task_body(capability: str = "text_strategy") -> bytes:
    return GenerationTaskRequestedV1(
        event_id=UUID(int=801), event_type="generation.task.requested", schema_version=1,
        aggregate_id=UUID(int=802), occurred_at=datetime(2026, 8, 24, tzinfo=UTC),
        initiated_by_actor_id=UUID(int=803), idempotency_key="task:802",
        payload={
            "task_id": str(UUID(int=802)),
            "project_id": str(UUID(int=804)),
            "capability": capability,
            "model_registry_entry_id": str(UUID(int=805)),
            "billing_mode": "customer_credit",
            "billing_hold_id": str(UUID(int=806)),
            "request_fingerprint": "a" * 64,
            "input_payload": {"brief": "gold"},
        },
    ).model_dump_json().encode()


def test_consumer_acks_only_after_the_registered_handler_returns() -> None:
    consumer = MemoryConsumer([task_body()])
    handled: list[UUID] = []
    result = RocketMQTaskConsumer(consumer).consume_batch(
        {"text_strategy": handled.append}, maximum=8, invisible_seconds=600
    )
    assert handled == [UUID(int=802)]
    assert result.acked == 1 and result.retry == 0
    assert consumer.acked == consumer.deliveries


def test_handler_failure_or_unknown_capability_is_not_acked() -> None:
    consumer = MemoryConsumer([task_body(), task_body("unknown")])

    def broken(_task_id: UUID) -> None:
        raise RuntimeError("worker interrupted")

    result = RocketMQTaskConsumer(consumer).consume_batch(
        {"text_strategy": broken}, maximum=8, invisible_seconds=600
    )
    assert result.acked == 0 and result.retry == 2
    assert consumer.acked == []


def test_malformed_payload_is_not_acked() -> None:
    malformed = json.loads(task_body())
    malformed["payload"]["unexpected"] = "reject me"
    consumer = MemoryConsumer([json.dumps(malformed).encode()])

    result = RocketMQTaskConsumer(consumer).consume_batch(
        {"text_strategy": lambda _task_id: None}, maximum=8, invisible_seconds=600
    )

    assert result.acked == 0 and result.retry == 1
    assert consumer.acked == []


def test_reconciliation_required_handler_result_is_not_acked() -> None:
    consumer = MemoryConsumer([task_body()])
    result = RocketMQTaskConsumer(consumer).consume_batch(
        {"text_strategy": lambda _task_id: TaskHandlerDisposition.RETRY},
        maximum=8,
        invisible_seconds=600,
    )
    assert result.acked == 0 and result.retry == 1
    assert consumer.acked == []
```

Run: `cd backend && uv run pytest tests/unit/tasks/test_task_consumer.py -q`

Expected: FAIL because `ip_saas.workers.task_consumer` does not exist.

- [ ] **Step 13: Implement the synchronous RocketMQ task consumer boundary**

Create `backend/src/ip_saas/workers/task_consumer.py`:

```python
from __future__ import annotations

from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from enum import StrEnum
from typing import Any, Protocol
from uuid import UUID

from rocketmq import FilterExpression, SimpleConsumer  # type: ignore[import-untyped]

from ip_saas.config import Settings
from ip_saas.modules.tasks.events import GenerationTaskRequestedV1
from ip_saas.providers.rocketmq import rocketmq_configuration


class TaskHandlerDisposition(StrEnum):
    ACK = "ack"
    RETRY = "retry"


TaskHandler = Callable[[UUID], TaskHandlerDisposition | None]


class ConsumerPort(Protocol):
    def receive(self, maximum: int, invisible_seconds: int) -> Sequence[Any] | None: ...
    def ack(self, delivery: Any) -> None: ...


@dataclass(frozen=True)
class ConsumeBatchResult:
    received: int
    acked: int
    retry: int


class RocketMQTaskConsumer:
    def __init__(self, consumer: ConsumerPort) -> None:
        self._consumer = consumer

    def consume_batch(
        self,
        handlers: Mapping[str, TaskHandler],
        *,
        maximum: int = 16,
        invisible_seconds: int = 600,
    ) -> ConsumeBatchResult:
        if maximum < 1 or invisible_seconds < 30:
            raise ValueError("invalid consumer batch policy")
        deliveries = tuple(self._consumer.receive(maximum, invisible_seconds) or ())
        acked = 0
        for delivery in deliveries:
            try:
                envelope = GenerationTaskRequestedV1.model_validate_json(delivery.body)
                task_id = envelope.payload.task_id
                capability = envelope.payload.capability
                handler = handlers.get(capability)
                if handler is None:
                    raise ValueError("unregistered task capability")
                disposition = handler(task_id)
                if disposition == TaskHandlerDisposition.RETRY:
                    continue
            except Exception:
                # Do not ACK. RocketMQ redelivers and eventually routes the message to
                # its configured DLQ. The handler owns sanitized task/audit error detail.
                continue
            self._consumer.ack(delivery)
            acked += 1
        return ConsumeBatchResult(len(deliveries), acked, len(deliveries) - acked)


def connect_task_consumer(
    settings: Settings,
) -> tuple[RocketMQTaskConsumer, SimpleConsumer]:
    raw = SimpleConsumer(
        rocketmq_configuration(settings),
        settings.rocketmq_consumer_group,
        {settings.rocketmq_topic: FilterExpression()},
    )
    raw.startup()
    return RocketMQTaskConsumer(raw), raw
```

Each feature Worker registers only its exact capability strings. Its handler calls `TaskSubmissionService.start()`: an already terminal task returns `TaskHandlerDisposition.ACK`; `RUNNING` continues with the captured `attempt_no`; `RECONCILIATION_REQUIRED` returns `TaskHandlerDisposition.RETRY` without any provider, domain, billing, or delivery write; an active lease `Conflict` also retries. Returning `None` remains a backward-compatible ACK for handlers that completed successfully. A database reconciliation scanner invokes `TaskReconciliationService`; after it atomically moves the task to `FAILED`, the next delivery is terminal and can ACK. Heartbeats protect long stages, and the Worker process calls `raw.shutdown()` during graceful termination. Unknown, malformed, repeatedly failing, or reconciliation-pending deliveries are never acknowledged by application code; the managed RocketMQ subscription must have a finite retry/DLQ policy and a reconciliation scanner verified before a real provider is enabled.

- [ ] **Step 14: Freeze the task-specific event payload schema**

Append to `backend/tests/contract/test_event_contracts.py`:

```python
from ip_saas.modules.tasks.events import GenerationTaskRequestedV1


@pytest.mark.contract
def test_generation_task_requested_schema_has_no_drift() -> None:
    expected = GenerationTaskRequestedV1.model_json_schema()
    path = Path(__file__).parents[3] / "contracts/events/generation.task.requested.v1.json"
    assert json.loads(path.read_text()) == expected
```

Generate the typed contract once:

```bash
cd backend
uv run python -c 'import json,pathlib; from ip_saas.modules.tasks.events import GenerationTaskRequestedV1; p=pathlib.Path("../contracts/events/generation.task.requested.v1.json"); p.write_text(json.dumps(GenerationTaskRequestedV1.model_json_schema(),ensure_ascii=False,indent=2)+"\n")'
```

Expected: the schema requires every billing, model, project, fingerprint, and input field and rejects extra payload fields.

- [ ] **Step 15: Run lease, strict event, and consumer contract tests together**

Run: `cd backend && uv run pytest tests/unit/tasks/test_task_consumer.py tests/integration/tasks/test_task_submission.py tests/integration/tasks/test_task_lease_concurrency.py tests/contract/test_event_contracts.py -q && uv run mypy src/ip_saas/modules/tasks src/ip_saas/workers/task_consumer.py`

Expected: PASS; a valid handler return ACKs, an unknown/malformed/reconciliation-pending delivery retries, an expired lease cannot heartbeat or write before reclaim, the heartbeat/reclaim race always advances to attempt 2, retry exhaustion leaves the hold active until one explicit finalizer succeeds, and the typed event schema has no drift.

- [ ] **Step 16: Run task idempotency, lifecycle, and internal-budget tests**

Run: `cd backend && uv run pytest tests/integration/tasks/test_task_submission.py -v`

Expected: PASS, 8 collected cases; exact provider settlement replay succeeds across independent SQLAlchemy sessions, changed request ID, task ID, settlement amount, or supplier amount conflicts, duplicate task input freezes billing cap and payload, an idempotency retry cannot bypass project isolation, two accounts may independently reuse the same client key, terminal results persist, errors are redacted and bounded, an active lease cannot be stolen, an expired lease cannot write and advances `attempt_no` only through reclaim, exhausted retries enter `reconciliation_required`, zero-call evidence releases once, and a complete two-request matched supplier-cost set is persisted with the exact task ID and settled once before failure; omission, duplication, mismatch, and replay are covered.

- [ ] **Step 17: Run task and tenant regressions**

Run: `cd backend && uv run pytest tests/integration/tasks tests/security/test_cross_tenant_access.py -v`

Expected: PASS; task status, including `reconciliation_required`, is tenant-filtered; customer project access remains isolated; and no test persists non-zero provider cost without a TaskRecord lineage.

- [ ] **Step 18: Commit task submission contracts**

```bash
git add backend/src/ip_saas/common/tasking.py backend/src/ip_saas/modules/tasks backend/src/ip_saas/workers/task_consumer.py backend/tests/unit/tasks/test_task_consumer.py backend/tests/integration/tasks backend/tests/contract/test_event_contracts.py contracts/events/generation.task.requested.v1.json backend/src/ip_saas/api.py
git commit -m "feat: submit billed tasks atomically"
```

### Task 12: Define and test the private-object/TOS boundary

**Files:**
- Create: `backend/src/ip_saas/providers/object_store.py`
- Create: `backend/src/ip_saas/providers/fake_object_store.py`
- Create: `backend/src/ip_saas/providers/tos_object_store.py`
- Create: `backend/tests/providers/test_object_store_contract.py`

- [ ] **Step 1: Write the failing private-object contract test**

Create `backend/tests/providers/test_object_store_contract.py`:

```python
from hashlib import sha256
from uuid import uuid4

import pytest

from ip_saas.providers.fake_object_store import FakePrivateObjectStore
from ip_saas.providers.object_store import ObjectKeyFactory


@pytest.mark.integration
def test_private_object_round_trip_and_short_signed_url() -> None:
    store = FakePrivateObjectStore()
    account_id, project_id, object_id = uuid4(), uuid4(), uuid4()
    body = b"private-media"
    key = ObjectKeyFactory().for_project(account_id, project_id, object_id, "clip.mp4")
    result = store.put_bytes(key, body, "video/mp4")
    assert result.sha256 == sha256(body).hexdigest()
    assert store.read_for_test(key) == body
    assert "expires=300" in store.signed_get(key, expires_seconds=300)


@pytest.mark.parametrize("filename", ["../secret", "/etc/passwd", "a\\b.mp4", ""])
def test_object_key_rejects_path_traversal(filename: str) -> None:
    with pytest.raises(ValueError):
        ObjectKeyFactory().for_project(uuid4(), uuid4(), uuid4(), filename)
```

- [ ] **Step 2: Run the contract and verify missing object-store modules**

Run: `cd backend && uv run pytest tests/providers/test_object_store_contract.py -v`

Expected: FAIL during collection because the object-store modules are missing.

- [ ] **Step 3: Implement the provider-independent private-object protocol**

Create `backend/src/ip_saas/providers/object_store.py`:

```python
from dataclasses import dataclass
from pathlib import PurePosixPath
from typing import Protocol
from uuid import UUID


@dataclass(frozen=True)
class StoredObject:
    key: str
    sha256: str
    size_bytes: int
    content_type: str


class ObjectKeyFactory:
    def for_project(
        self,
        account_id: UUID,
        project_id: UUID,
        object_id: UUID,
        filename: str,
    ) -> str:
        if not filename or filename != PurePosixPath(filename).name or "\\" in filename:
            raise ValueError("filename must be a plain basename")
        return f"accounts/{account_id}/projects/{project_id}/objects/{object_id}/{filename}"


class PrivateObjectStore(Protocol):
    def put_bytes(self, key: str, body: bytes, content_type: str) -> StoredObject: ...
    def signed_get(self, key: str, expires_seconds: int) -> str: ...
    def delete(self, key: str) -> None: ...
```

- [ ] **Step 4: Implement the deterministic fake adapter**

Create `backend/src/ip_saas/providers/fake_object_store.py`:

```python
from hashlib import sha256

from ip_saas.providers.object_store import StoredObject


class FakePrivateObjectStore:
    def __init__(self) -> None:
        self._objects: dict[str, tuple[bytes, str]] = {}

    def put_bytes(self, key: str, body: bytes, content_type: str) -> StoredObject:
        self._objects[key] = (body, content_type)
        return StoredObject(key, sha256(body).hexdigest(), len(body), content_type)

    def signed_get(self, key: str, expires_seconds: int) -> str:
        if key not in self._objects:
            raise KeyError(key)
        if not 1 <= expires_seconds <= 3600:
            raise ValueError("signed URL lifetime must be between 1 and 3600 seconds")
        return f"https://fake.invalid/{key}?expires={expires_seconds}"

    def delete(self, key: str) -> None:
        self._objects.pop(key, None)

    def read_for_test(self, key: str) -> bytes:
        return self._objects[key][0]
```

- [ ] **Step 5: Implement the real TOS adapter without reading credentials in tests**

Create `backend/src/ip_saas/providers/tos_object_store.py`:

```python
from hashlib import sha256

import tos

from ip_saas.providers.object_store import StoredObject


class TosPrivateObjectStore:
    def __init__(self, client: tos.TosClientV2, bucket: str) -> None:
        self._client = client
        self._bucket = bucket

    def put_bytes(self, key: str, body: bytes, content_type: str) -> StoredObject:
        self._client.put_object(
            bucket=self._bucket,
            key=key,
            content=body,
            content_type=content_type,
            meta={"sha256": sha256(body).hexdigest()},
        )
        return StoredObject(key, sha256(body).hexdigest(), len(body), content_type)

    def signed_get(self, key: str, expires_seconds: int) -> str:
        if not 1 <= expires_seconds <= 3600:
            raise ValueError("signed URL lifetime must be between 1 and 3600 seconds")
        result = self._client.pre_signed_url(
            tos.HttpMethodType.Http_Method_Get,
            self._bucket,
            key,
            expires=expires_seconds,
        )
        return result.signed_url

    def delete(self, key: str) -> None:
        self._client.delete_object(bucket=self._bucket, key=key)
```

The application composition root constructs this adapter only when non-empty TOS credentials are explicitly supplied. Unit, integration, and contract tests instantiate `FakePrivateObjectStore` directly.

- [ ] **Step 6: Run object-store contract tests**

Run: `cd backend && uv run pytest tests/providers/test_object_store_contract.py -v`

Expected: PASS, 5 tests; no network connection or credential lookup occurs.

- [ ] **Step 7: Type-check the provider boundary**

Run: `cd backend && uv run mypy src/ip_saas/providers`

Expected: PASS; both adapters satisfy the method shapes of `PrivateObjectStore`.

- [ ] **Step 8: Commit private storage adapters**

```bash
git add backend/src/ip_saas/providers backend/tests/providers/test_object_store_contract.py
git commit -m "feat: add private TOS storage boundary"
```

### Task 13: Create and verify the frozen `0001_foundation` PostgreSQL migration

**Files:**
- Create: `backend/alembic.ini`
- Create: `backend/migrations/env.py`
- Create: `backend/migrations/script.py.mako`
- Create: `backend/migrations/versions/0001_foundation.py`
- Modify: `backend/src/ip_saas/db/base.py`
- Modify: `backend/tests/integration/test_health_and_migrations.py`

- [ ] **Step 1: Register every foundation model with SQLAlchemy metadata**

Append to `backend/src/ip_saas/db/base.py`:

```python
def load_all_models() -> None:
    from ip_saas.common.audit import AuditEvent
    from ip_saas.common.outbox import OutboxEvent
    from ip_saas.modules.accounts.models import (
        Account,
        AuthSession,
        PlatformRoleGrant,
        Principal,
        ResellerRelation,
    )
    from ip_saas.modules.billing.models import (
        CreditLedgerEntry,
        CreditTransaction,
        CreditWallet,
        GenerationHold,
        InternalBudgetHold,
        InternalCostCenter,
        InternalCostEntry,
        PricingVersion,
        ProviderCostEntry,
    )
    from ip_saas.modules.model_registry.models import ModelRegistryEntry
    from ip_saas.modules.projects.models import IPProject
    from ip_saas.modules.tasks.models import TaskRecord

    assert all(
        model.__table__.metadata is Base.metadata
        for model in (
            Account,
            AuthSession,
            PlatformRoleGrant,
            Principal,
            ResellerRelation,
            CreditLedgerEntry,
            CreditTransaction,
            CreditWallet,
            GenerationHold,
            InternalBudgetHold,
            InternalCostCenter,
            InternalCostEntry,
            PricingVersion,
            ProviderCostEntry,
            ModelRegistryEntry,
            IPProject,
            TaskRecord,
            AuditEvent,
            OutboxEvent,
        )
    )
```

Call `load_all_models()` once at the bottom of the module. This explicit list makes accidental omission visible in code review.

- [ ] **Step 2: Initialize Alembic and replace its environment with the synchronous configuration**

Run: `cd backend && uv run alembic init migrations`

Expected: `backend/alembic.ini`, `backend/migrations/env.py`, and `backend/migrations/script.py.mako` are created.

Replace `backend/migrations/env.py` with:

```python
from logging.config import fileConfig

from alembic import context
from sqlalchemy import engine_from_config, pool

from ip_saas.config import get_settings
from ip_saas.db.base import Base, load_all_models

config = context.config
if config.config_file_name is not None:
    fileConfig(config.config_file_name)
config.set_main_option("sqlalchemy.url", get_settings().database_url)
load_all_models()
target_metadata = Base.metadata


def run_migrations_offline() -> None:
    context.configure(
        url=config.get_main_option("sqlalchemy.url"),
        target_metadata=target_metadata,
        literal_binds=True,
        dialect_opts={"paramstyle": "named"},
        compare_type=True,
    )
    with context.begin_transaction():
        context.run_migrations()


def run_migrations_online() -> None:
    connectable = engine_from_config(
        config.get_section(config.config_ini_section, {}),
        prefix="sqlalchemy.",
        poolclass=pool.NullPool,
    )
    with connectable.connect() as connection:
        context.configure(connection=connection, target_metadata=target_metadata, compare_type=True)
        with context.begin_transaction():
            context.run_migrations()


run_migrations_offline() if context.is_offline_mode() else run_migrations_online()
```

- [ ] **Step 3: Generate exactly one foundation migration**

Run:

```bash
cd backend
DATABASE_URL=postgresql+psycopg://ip_saas:ip_saas@localhost:5432/ip_saas \
  uv run alembic revision --autogenerate --rev-id 0001_foundation -m foundation
```

Expected: `backend/migrations/versions/0001_foundation.py` is created with:

```python
revision = "0001_foundation"
down_revision = None
branch_labels = None
depends_on = None
```

Verify `upgrade()` creates every table registered by `load_all_models()` and `downgrade()` drops them in reverse foreign-key order. Do not split this schema into a second revision; Plan 02 starts at `0002_intelligence`.

Inspect the generated `task_records` definition before continuing. Its `status` column must be `sa.String(length=32)`, and `upgrade()` must create both `ck_task_status` containing the exact `reconciliation_required` value and `ck_task_reconciliation_identity`. The nullable `reconciliation_idempotency_key` and `reconciliation_fingerprint` columns belong in this same `0001_foundation` revision. If autogenerate emitted `String(20)`, omitted either constraint/column, or created another revision, fix `0001_foundation.py` before running parity tests.

Also inspect `provider_cost_entries`: `provider_request_id` must be a nullable `sa.String(length=200)` for source-compatible pre-response estimates and migration history, `settlement_amount` must be a non-null `sa.BigInteger()`, and `uq_provider_cost_request_identity` must uniquely cover `(provider, provider_request_id)`. `ProviderCostInput.__post_init__` canonicalizes surrounding whitespace and rejects blank, non-string, over-200-character, or case-insensitive reserved `None` sentinel values when an adapter already knows the identity; `BillingService._require_provider_identity` then rejects null identities and missing task lineage before every ordinary or batch settlement. Temporary in-memory cost estimates may exist before the response identity is received, but they cannot reach either ledger. A proven zero-call path constructs neither `ProviderCostInput` nor `ProviderCostEntry` and releases only through `TaskReconciliationService.finalize_no_provider_call`; a ledger-focused deterministic fake instead supplies its explicit canonical fake request ID and the exact `TaskRecord.id`. A supplier response, timeout, unknown outcome, or paid attempt may never masquerade as zero-call and instead remains held for reconciliation. `settlement_amount` freezes the per-row customer-credit/internal-fen input so an idempotency replay can compare every immutable argument even after the shared hold has already settled.

- [ ] **Step 4: Add database-level append-only protection to the migration**

At the end of `upgrade()` in `0001_foundation.py`, add:

```python
op.execute(
    """
    CREATE FUNCTION reject_append_only_change() RETURNS trigger AS $$
    BEGIN
      RAISE EXCEPTION '% is append-only', TG_TABLE_NAME;
    END;
    $$ LANGUAGE plpgsql;
    """
)
for table in ("audit_events", "credit_ledger_entries", "internal_cost_entries", "provider_cost_entries"):
    op.execute(
        f"CREATE TRIGGER {table}_append_only BEFORE UPDATE OR DELETE ON {table} "
        "FOR EACH ROW EXECUTE FUNCTION reject_append_only_change();"
    )
op.execute(
    """
    CREATE FUNCTION protect_outbox_envelope() RETURNS trigger AS $$
    BEGIN
      IF ROW(NEW.event_type, NEW.schema_version, NEW.aggregate_id, NEW.occurred_at,
             NEW.initiated_by_actor_id, NEW.idempotency_key, NEW.payload, NEW.created_at)
         IS DISTINCT FROM
         ROW(OLD.event_type, OLD.schema_version, OLD.aggregate_id, OLD.occurred_at,
             OLD.initiated_by_actor_id, OLD.idempotency_key, OLD.payload, OLD.created_at) THEN
        RAISE EXCEPTION 'outbox event payload is immutable';
      END IF;
      RETURN NEW;
    END;
    $$ LANGUAGE plpgsql;
    """
)
op.execute(
    "CREATE TRIGGER outbox_envelope_immutable BEFORE UPDATE ON outbox_events "
    "FOR EACH ROW EXECUTE FUNCTION protect_outbox_envelope();"
)
```

At the start of `downgrade()`, before dropping tables, add:

```python
op.execute("DROP TRIGGER outbox_envelope_immutable ON outbox_events;")
op.execute("DROP FUNCTION protect_outbox_envelope();")
for table in ("audit_events", "credit_ledger_entries", "internal_cost_entries", "provider_cost_entries"):
    op.execute(f"DROP TRIGGER {table}_append_only ON {table};")
op.execute("DROP FUNCTION reject_append_only_change();")
```

- [ ] **Step 5: Write migration upgrade, parity, and downgrade tests**

Append to `backend/tests/integration/test_health_and_migrations.py`:

```python
from alembic import command
from alembic.config import Config
from sqlalchemy import Engine, inspect

from ip_saas.db.base import Base, load_all_models


def alembic_config(test_database_url: str) -> Config:
    config = Config("alembic.ini")
    config.set_main_option("sqlalchemy.url", test_database_url)
    return config


def test_foundation_migration_is_complete_and_reversible(
    pg_engine: Engine, test_database_url: str
) -> None:
    config = alembic_config(test_database_url)
    command.downgrade(config, "base")
    command.upgrade(config, "head")
    load_all_models()
    actual = set(inspect(pg_engine).get_table_names()) - {"alembic_version"}
    assert actual == set(Base.metadata.tables)
    task_columns = {
        column["name"]: column for column in inspect(pg_engine).get_columns("task_records")
    }
    assert task_columns["status"]["type"].length == 32
    assert task_columns["reconciliation_idempotency_key"]["nullable"] is True
    assert task_columns["reconciliation_fingerprint"]["nullable"] is True
    task_checks = inspect(pg_engine).get_check_constraints("task_records")
    checks_by_name = {item["name"]: item["sqltext"] for item in task_checks}
    assert "reconciliation_required" in checks_by_name["ck_task_status"]
    assert "reconciliation_fingerprint" in checks_by_name[
        "ck_task_reconciliation_identity"
    ]
    provider_cost_columns = {
        column["name"]: column
        for column in inspect(pg_engine).get_columns("provider_cost_entries")
    }
    assert provider_cost_columns["provider_request_id"]["nullable"] is True
    assert provider_cost_columns["provider_request_id"]["type"].length == 200
    assert provider_cost_columns["settlement_amount"]["nullable"] is False
    assert provider_cost_columns["settlement_amount"]["type"].python_type is int
    provider_cost_uniques = {
        item["name"]: tuple(item["column_names"])
        for item in inspect(pg_engine).get_unique_constraints("provider_cost_entries")
    }
    assert provider_cost_uniques["uq_provider_cost_request_identity"] == (
        "provider",
        "provider_request_id",
    )
    command.check(config)
    command.downgrade(config, "base")
    assert set(inspect(pg_engine).get_table_names()) <= {"alembic_version"}
    command.upgrade(config, "head")
```

- [ ] **Step 6: Run the migration test against PostgreSQL**

Run: `cd backend && DATABASE_URL=postgresql+psycopg://ip_saas:ip_saas@localhost:5432/ip_saas_test TEST_DATABASE_URL=postgresql+psycopg://ip_saas:ip_saas@localhost:5432/ip_saas_test uv run pytest tests/integration/test_health_and_migrations.py -v`

Expected: PASS; `alembic check` prints `No new upgrade operations detected.` and the downgrade leaves no foundation tables.

- [ ] **Step 7: Run the entire integration suite on the migrated database**

Run: `make test-integration`

Expected: PASS; no test selects SQLite and no second Alembic revision exists.

- [ ] **Step 8: Commit the frozen first migration**

```bash
git add backend/alembic.ini backend/migrations backend/src/ip_saas/db/base.py backend/tests/integration/test_health_and_migrations.py
git commit -m "feat: add foundation database migration"
```

### Task 14: Freeze synchronous API composition and the OpenAPI contract

**Files:**
- Create: `backend/src/ip_saas/modules/billing/router.py`
- Create: `backend/src/ip_saas/scripts/export_contracts.py`
- Create: `backend/tests/contract/test_openapi.py`
- Create: `backend/tests/integration/billing/test_wallet_route.py`
- Create: `contracts/openapi.json`
- Modify: `backend/src/ip_saas/api.py`

- [ ] **Step 1: Write the failing OpenAPI drift test**

Create `backend/tests/contract/test_openapi.py`:

```python
import json
from pathlib import Path

import pytest

from ip_saas.api import create_app


@pytest.mark.contract
def test_checked_in_openapi_has_no_drift() -> None:
    expected = create_app().openapi()
    path = Path(__file__).parents[3] / "contracts/openapi.json"
    assert json.loads(path.read_text()) == expected


def test_foundation_routes_are_present() -> None:
    paths = create_app().openapi()["paths"]
    assert {
        "/healthz",
        "/v1/auth/login",
        "/v1/auth/me",
        "/v1/projects",
        "/v1/billing/wallet",
        "/v1/platform/models",
        "/v1/tasks/{task_id}",
    } <= set(paths)
```

Create `backend/tests/integration/billing/test_wallet_route.py`:

```python
from uuid import uuid4

import pytest
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from ip_saas.common.errors import Forbidden
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.models import Account, AccountKind
from ip_saas.modules.billing.models import CreditWallet
from ip_saas.modules.billing.router import wallet_summary


def test_platform_wallet_route_never_creates_a_credit_wallet(db_session: Session) -> None:
    actor = ActorContext(uuid4(), uuid4(), ActorKind.PLATFORM_OPERATOR)
    before = db_session.scalar(select(func.count()).select_from(CreditWallet))

    with pytest.raises(Forbidden):
        wallet_summary(db_session, actor)

    assert db_session.scalar(select(func.count()).select_from(CreditWallet)) == before


def test_customer_wallet_route_returns_credit_summary(db_session: Session) -> None:
    account = Account(kind=AccountKind.C_USER, display_name="Customer")
    db_session.add(account)
    db_session.flush()
    actor = ActorContext(uuid4(), account.id, ActorKind.C_USER)

    response = wallet_summary(db_session, actor)

    assert response.posted_balance == response.active_holds == response.available_balance == 0
```

- [ ] **Step 2: Run the contract test and verify the missing contract/route failure**

Run: `cd backend && uv run pytest tests/contract/test_openapi.py tests/integration/billing/test_wallet_route.py -v`

Expected: FAIL because `contracts/openapi.json` and `ip_saas.modules.billing.router` do not exist.

- [ ] **Step 3: Add the read-only wallet summary route**

Create `backend/src/ip_saas/modules/billing/router.py`:

```python
from typing import Annotated

from fastapi import APIRouter, Depends
from pydantic import BaseModel
from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.errors import Forbidden
from ip_saas.db.session import get_session
from ip_saas.modules.accounts.context import ActorContext, ActorKind
from ip_saas.modules.accounts.dependencies import get_actor
from ip_saas.modules.billing.models import CreditWallet
from ip_saas.modules.billing.repository import CreditLedgerRepository
from ip_saas.modules.billing.service import CreditLedgerService

router = APIRouter(prefix="/v1/billing", tags=["billing"])


class WalletSummary(BaseModel):
    posted_balance: int
    active_holds: int
    available_balance: int


@router.get("/wallet", response_model=WalletSummary)
def wallet_summary(
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> WalletSummary:
    if actor.kind in {
        ActorKind.PLATFORM_ADMIN,
        ActorKind.PLATFORM_OPERATOR,
        ActorKind.PLATFORM_REVIEWER,
    }:
        raise Forbidden("platform work uses internal cost centers, not credit wallets")
    ledger = CreditLedgerService(CreditLedgerRepository())
    wallet = session.scalar(select(CreditWallet).where(CreditWallet.account_id == actor.account_id))
    if wallet is None:
        wallet = ledger.ensure_account_wallet(session, actor.account_id)
    active = ledger.repository.active_holds(session, wallet.id)
    return WalletSummary(
        posted_balance=wallet.posted_balance,
        active_holds=active,
        available_balance=wallet.posted_balance - active,
    )
```

- [ ] **Step 4: Run the wallet boundary tests**

Run: `cd backend && uv run pytest tests/integration/billing/test_wallet_route.py -v`

Expected: PASS, 2 tests; customer/reseller credit-wallet behavior remains available while every platform role is rejected before a `CreditWallet` row can be created.

- [ ] **Step 5: Replace the task-route description with the exact implementation**

Create `backend/src/ip_saas/modules/tasks/router.py`:

```python
from typing import Annotated
from uuid import UUID

from fastapi import APIRouter, Depends
from pydantic import BaseModel
from sqlalchemy import select
from sqlalchemy.orm import Session

from ip_saas.common.errors import NotFound
from ip_saas.db.session import get_session
from ip_saas.modules.accounts.context import ActorContext
from ip_saas.modules.accounts.dependencies import get_actor
from ip_saas.modules.tasks.models import TaskRecord

router = APIRouter(prefix="/v1/tasks", tags=["tasks"])


class TaskResponse(BaseModel):
    id: UUID
    project_id: UUID
    capability: str
    status: str
    attempt_no: int
    billing_mode: str
    result_payload: dict[str, object] | None
    error_code: str | None
    error_message: str | None


@router.get("/{task_id}", response_model=TaskResponse)
def get_task(
    task_id: UUID,
    session: Annotated[Session, Depends(get_session)],
    actor: Annotated[ActorContext, Depends(get_actor)],
) -> TaskResponse:
    task = session.scalar(
        select(TaskRecord).where(
            TaskRecord.id == task_id,
            TaskRecord.initiated_by_account_id == actor.account_id,
        )
    )
    if task is None:
        raise NotFound("task")
    return TaskResponse(
        id=task.id,
        project_id=task.project_id,
        capability=task.capability,
        status=task.status,
        attempt_no=task.attempt_no,
        billing_mode=task.billing_mode,
        result_payload=task.result_payload,
        error_code=task.error_code,
        error_message=task.error_message,
    )
```

- [ ] **Step 6: Compose all foundation routers in one place**

Replace the earlier one-router import block in `backend/src/ip_saas/api.py` with this complete block; do not leave a second alias or a second accounts-router inclusion:

```python
from ip_saas.modules.accounts.router import router as accounts_router
from ip_saas.modules.billing.router import router as billing_router
from ip_saas.modules.model_registry.router import router as model_registry_router
from ip_saas.modules.projects.router import router as projects_router
from ip_saas.modules.tasks.router import router as tasks_router
```

Inside `create_app()`, replace the earlier `app.include_router(accounts_router)` line with this complete ordered block:

```python
app.include_router(accounts_router)
app.include_router(projects_router)
app.include_router(billing_router)
app.include_router(model_registry_router)
app.include_router(tasks_router)
```

- [ ] **Step 7: Implement deterministic OpenAPI and event export**

Create `backend/src/ip_saas/scripts/export_contracts.py`:

```python
import argparse
import json
from pathlib import Path

from ip_saas.api import create_app
from ip_saas.common.outbox import EventEnvelope
from ip_saas.modules.tasks.events import GenerationTaskRequestedV1

ROOT = Path(__file__).parents[4]


def rendered(value: object) -> str:
    return json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n"


def write_or_check(path: Path, content: str, check: bool) -> None:
    if check:
        if not path.exists() or path.read_text() != content:
            raise SystemExit(f"contract drift: {path}")
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    write_or_check(ROOT / "contracts/openapi.json", rendered(create_app().openapi()), args.check)
    write_or_check(
        ROOT / "contracts/events/event-envelope.v1.json",
        rendered(EventEnvelope.model_json_schema()),
        args.check,
    )
    write_or_check(
        ROOT / "contracts/events/generation.task.requested.v1.json",
        rendered(GenerationTaskRequestedV1.model_json_schema()),
        args.check,
    )


if __name__ == "__main__":
    main()
```

- [ ] **Step 8: Export the checked-in contracts**

Run: `cd backend && uv run python -m ip_saas.scripts.export_contracts`

Expected: `contracts/openapi.json` and the event contract are written with stable sorted keys.

- [ ] **Step 9: Run contract tests and drift mode**

Run: `cd backend && uv run pytest tests/contract -v && uv run python -m ip_saas.scripts.export_contracts --check`

Expected: PASS; drift mode exits 0 and changes no files.

- [ ] **Step 10: Commit the public API contracts**

```bash
git add backend/src/ip_saas/api.py backend/src/ip_saas/modules/billing/router.py backend/src/ip_saas/modules/tasks/router.py backend/src/ip_saas/scripts/export_contracts.py backend/tests/contract backend/tests/integration/billing/test_wallet_route.py contracts
git commit -m "feat: freeze foundation API contracts"
```

### Task 15: Build the role-aware Next.js foundation shell

**Files:**
- Create: `frontend/vitest.config.ts`
- Create: `frontend/playwright.config.ts`
- Create: `frontend/src/app/globals.css`
- Create: `frontend/src/app/layout.tsx`
- Create: `frontend/src/app/page.tsx`
- Create: `frontend/src/app/login/page.tsx`
- Create: `frontend/src/app/(c-user)/projects/page.tsx`
- Create: `frontend/src/app/(reseller)/reseller/page.tsx`
- Create: `frontend/src/app/(platform)/platform/page.tsx`
- Create: `frontend/src/features/auth/LoginForm.tsx`
- Create: `frontend/src/features/shell/AppShell.tsx`
- Create: `frontend/src/features/shell/role-home.ts`
- Create: `frontend/src/features/shell/role-home.test.ts`
- Create: `frontend/src/lib/api/client.ts`
- Create: `frontend/src/lib/api/client.test.ts`
- Create: `frontend/src/lib/api/schema.d.ts`
- Create: `frontend/tests/e2e/role-shells.spec.ts`

- [ ] **Step 1: Generate TypeScript API types from the frozen contract**

Run: `cd frontend && pnpm generate:api`

Expected: `frontend/src/lib/api/schema.d.ts` is created and contains paths for `/v1/auth/login`, `/v1/projects`, `/v1/billing/wallet`, and `/v1/tasks/{task_id}`.

- [ ] **Step 2: Write the failing role-home unit test**

Create `frontend/src/features/shell/role-home.test.ts`:

```typescript
import {describe, expect, it} from "vitest";
import {homeForActorKind} from "./role-home";

describe("homeForActorKind", () => {
  it.each([
    ["c_user", "/projects"],
    ["reseller_l1", "/reseller"],
    ["reseller_l2", "/reseller"],
    ["platform_admin", "/platform"],
    ["platform_operator", "/platform"],
    ["platform_reviewer", "/platform"],
  ])("maps %s to %s", (kind, expected) => {
    expect(homeForActorKind(kind)).toBe(expected);
  });
});
```

Create `frontend/src/lib/api/client.test.ts`:

```typescript
import {beforeEach, describe, expect, it, vi} from "vitest";
import {apiRequest} from "./client";

describe("apiRequest", () => {
  beforeEach(() => {
    sessionStorage.clear();
    vi.unstubAllGlobals();
  });

  it("adds the browser proxy prefix, bearer token, cookie, and JSON body", async () => {
    sessionStorage.setItem("access_token", "access-1");
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({status: "queued"}),
    });
    vi.stubGlobal("fetch", fetchMock);

    await apiRequest<{status: string}>("/v1/tasks/task-1", {
      method: "POST",
      body: {confirm: true},
    });

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/tasks/task-1",
      expect.objectContaining({
        method: "POST",
        credentials: "include",
        body: JSON.stringify({confirm: true}),
        headers: expect.objectContaining({
          authorization: "Bearer access-1",
          "content-type": "application/json",
        }),
      }),
    );
  });

  it("rejects before fetch when no access token exists", async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);

    await expect(apiRequest("/v1/tasks/task-1")).rejects.toThrow(
      "authentication required",
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("rejects a browser /api prefix supplied by a feature", async () => {
    sessionStorage.setItem("access_token", "access-1");
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);

    await expect(apiRequest("/api/v1/tasks/task-1")).rejects.toThrow(
      "API path must start with /v1/",
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 3: Run the unit test and verify the missing-module failure**

Run: `cd frontend && pnpm test -- role-home.test.ts client.test.ts`

Expected: FAIL because `./role-home` and `./client` are missing.

- [ ] **Step 4: Implement role routing and the typed API client**

Create `frontend/src/features/shell/role-home.ts`:

```typescript
export function homeForActorKind(kind: string): string {
  if (kind === "c_user") return "/projects";
  if (kind === "reseller_l1" || kind === "reseller_l2") return "/reseller";
  if (kind.startsWith("platform_")) return "/platform";
  throw new Error(`unsupported actor kind: ${kind}`);
}
```

Create `frontend/src/lib/api/client.ts`:

```typescript
import type {paths} from "./schema";

type LoginResponse = paths["/v1/auth/login"]["post"]["responses"]["200"]["content"]["application/json"];

type ApiRequestInit = Omit<RequestInit, "body" | "headers"> & {
  body?: unknown;
  headers?: Record<string, string>;
};

export function requireAccessToken(): string {
  const token = sessionStorage.getItem("access_token");
  if (!token) throw new Error("authentication required");
  return token;
}

export async function apiRequest<T>(
  path: string,
  init: ApiRequestInit = {},
): Promise<T> {
  if (!path.startsWith("/v1/")) throw new Error("API path must start with /v1/");
  const token = requireAccessToken();
  const {body, headers, ...rest} = init;
  const hasBody = body !== undefined;
  const response = await fetch(`/api${path}`, {
    ...rest,
    credentials: "include",
    headers: {
      authorization: `Bearer ${token}`,
      ...(hasBody ? {"content-type": "application/json"} : {}),
      ...headers,
    },
    body: hasBody ? JSON.stringify(body) : undefined,
  });
  if (!response.ok) {
    const error = (await response.json().catch(() => null)) as {message?: string} | null;
    throw new Error(error?.message ?? `request failed: ${response.status}`);
  }
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}

export async function login(loginName: string, password: string): Promise<LoginResponse> {
  const response = await fetch("/api/v1/auth/login", {
    method: "POST",
    credentials: "include",
    headers: {"content-type": "application/json"},
    body: JSON.stringify({login_name: loginName, password}),
  });
  if (!response.ok) throw new Error("登录失败，请检查账号和密码");
  const result = (await response.json()) as LoginResponse;
  sessionStorage.setItem("access_token", result.access_token);
  return result;
}

export async function getMe(): Promise<{kind: string}> {
  const token = requireAccessToken();
  const response = await fetch("/api/v1/auth/me", {
    headers: {authorization: `Bearer ${token}`},
    credentials: "include",
  });
  if (!response.ok) throw new Error("authentication required");
  return response.json() as Promise<{kind: string}>;
}
```

- [ ] **Step 5: Run the role and shared API-client unit tests**

Run: `cd frontend && pnpm test -- role-home.test.ts client.test.ts`

Expected: PASS, 9 cases; every feature can pass a backend `/v1/...` path, the shared client alone applies the browser `/api` proxy prefix, and a double prefix is rejected before fetch.

- [ ] **Step 6: Create the application shell and login form**

Create `frontend/src/features/shell/AppShell.tsx`:

```tsx
"use client";

import {useEffect, useState, type ReactNode} from "react";
import {useRouter} from "next/navigation";
import {getMe} from "@/lib/api/client";

export function AppShell({title, allowedKinds, children}: {
  title: string;
  allowedKinds: string[];
  children: ReactNode;
}) {
  const router = useRouter();
  const [ready, setReady] = useState(false);
  useEffect(() => {
    getMe().then((actor) => {
      if (!allowedKinds.includes(actor.kind)) router.replace("/login");
      else setReady(true);
    }).catch(() => router.replace("/login"));
  }, [allowedKinds, router]);
  if (!ready) return <main aria-busy="true">正在验证账号…</main>;
  return <main><header><h1>{title}</h1></header>{children}</main>;
}
```

Create `frontend/src/features/auth/LoginForm.tsx`:

```tsx
"use client";

import {useState, type FormEvent} from "react";
import {useRouter} from "next/navigation";
import {getMe, login} from "@/lib/api/client";
import {homeForActorKind} from "@/features/shell/role-home";

export function LoginForm() {
  const router = useRouter();
  const [error, setError] = useState("");
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    try {
      await login(String(data.get("login_name")), String(data.get("password")));
      router.replace(homeForActorKind((await getMe()).kind));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "登录失败");
    }
  }
  return (
    <form onSubmit={submit}>
      <label>账号<input name="login_name" autoComplete="username" required /></label>
      <label>密码<input name="password" type="password" autoComplete="current-password" required /></label>
      <button type="submit">登录</button>
      {error ? <p role="alert">{error}</p> : null}
    </form>
  );
}
```

- [ ] **Step 7: Create the layout, styles, and role route pages**

Create `frontend/src/app/layout.tsx`:

```tsx
import type {Metadata} from "next";
import type {ReactNode} from "react";
import "./globals.css";

export const metadata: Metadata = {title: "AI IP 工作台"};

export default function RootLayout({children}: {children: ReactNode}) {
  return <html lang="zh-CN"><body>{children}</body></html>;
}
```

Create `frontend/src/app/page.tsx`:

```tsx
import {redirect} from "next/navigation";
export default function Home() { redirect("/login"); }
```

Create `frontend/src/app/login/page.tsx`:

```tsx
import {LoginForm} from "@/features/auth/LoginForm";
export default function LoginPage() { return <main><h1>登录 AI IP 工作台</h1><LoginForm /></main>; }
```

Create the three role pages:

```tsx
// frontend/src/app/(c-user)/projects/page.tsx
import {AppShell} from "@/features/shell/AppShell";
export default function ProjectsPage() {
  return <AppShell title="我的 IP 项目" allowedKinds={["c_user"]}><p>创建和进入相互隔离的 IP 项目。</p></AppShell>;
}
```

```tsx
// frontend/src/app/(reseller)/reseller/page.tsx
import {AppShell} from "@/features/shell/AppShell";
export default function ResellerPage() {
  return <AppShell title="代理后台" allowedKinds={["reseller_l1", "reseller_l2"]}><p>查看账户与创作点汇总。</p></AppShell>;
}
```

```tsx
// frontend/src/app/(platform)/platform/page.tsx
import {AppShell} from "@/features/shell/AppShell";
export default function PlatformPage() {
  return <AppShell title="平台后台" allowedKinds={["platform_admin", "platform_operator", "platform_reviewer"]}><p>管理模型、预算和平台项目。</p></AppShell>;
}
```

Create `frontend/src/app/globals.css`:

```css
:root { color-scheme: light; font-family: system-ui, sans-serif; background: #f5f6f8; color: #18202a; }
body { margin: 0; }
main { width: min(960px, calc(100% - 32px)); margin: 48px auto; background: white; padding: 24px; border-radius: 16px; }
form { display: grid; gap: 16px; max-width: 360px; }
label { display: grid; gap: 6px; }
input, button { min-height: 40px; font: inherit; }
button { cursor: pointer; }
```

- [ ] **Step 8: Configure Vitest and Playwright**

Create `frontend/vitest.config.ts`:

```typescript
import {defineConfig} from "vitest/config";
export default defineConfig({test: {environment: "jsdom"}});
```

Create `frontend/playwright.config.ts`:

```typescript
import {defineConfig} from "@playwright/test";
export default defineConfig({
  testDir: "tests/e2e",
  use: {baseURL: "http://127.0.0.1:3000"},
  webServer: {command: "pnpm dev", url: "http://127.0.0.1:3000", reuseExistingServer: true},
});
```

- [ ] **Step 9: Write the role-shell browser test**

Create `frontend/tests/e2e/role-shells.spec.ts`:

```typescript
import {expect, test} from "@playwright/test";

test("C user is routed from login to the project shell", async ({page}) => {
  await page.route("**/api/v1/auth/login", (route) => route.fulfill({json: {access_token: "token", token_type: "bearer"}}));
  await page.route("**/api/v1/auth/me", (route) => route.fulfill({json: {kind: "c_user"}}));
  await page.goto("/login");
  await page.getByLabel("账号").fill("customer@example.com");
  await page.getByLabel("密码").fill("strong-password-123");
  await page.getByRole("button", {name: "登录"}).click();
  await expect(page).toHaveURL(/\/projects$/);
  await expect(page.getByRole("heading", {name: "我的 IP 项目"})).toBeVisible();
});
```

- [ ] **Step 10: Run frontend unit, type, build, and E2E checks**

Run: `cd frontend && pnpm test && pnpm typecheck && pnpm build && pnpm test:e2e`

Expected: all commands PASS; Playwright reaches `/projects` only after the mocked C-user identity is returned.

- [ ] **Step 11: Commit the web foundation**

```bash
git add frontend
git commit -m "feat: add role-aware web shell"
```

### Task 16: Prove 10,000 randomized ledger sequences and add CI/operational acceptance

**Files:**
- Create: `backend/tests/unit/billing/test_randomized_ledgers.py`
- Create: `.github/workflows/ci.yml`
- Create: `README.md`
- Modify: `Makefile`

- [ ] **Step 1: Write the 10,000-sequence property test**

Create `backend/tests/unit/billing/test_randomized_ledgers.py`:

```python
from dataclasses import dataclass, field
from uuid import UUID, uuid4

from hypothesis import given, settings, strategies as st

from ip_saas.modules.billing.domain import CreditPosting, balanced_transfer, reverse_postings


@dataclass
class LedgerState:
    treasury: UUID = field(default_factory=uuid4)
    customer_a: UUID = field(default_factory=uuid4)
    customer_b: UUID = field(default_factory=uuid4)
    revenue: UUID = field(default_factory=uuid4)
    balances: dict[UUID, int] = field(init=False)
    holds: dict[str, tuple[UUID, int]] = field(default_factory=dict)
    history: list[tuple[CreditPosting, ...]] = field(default_factory=list)

    def __post_init__(self) -> None:
        self.balances = {self.treasury: 0, self.customer_a: 0, self.customer_b: 0, self.revenue: 0}

    def active_holds(self, wallet: UUID) -> int:
        return sum(amount for owner, amount in self.holds.values() if owner == wallet)

    def available(self, wallet: UUID) -> int:
        return self.balances[wallet] - self.active_holds(wallet)

    def post(self, postings: tuple[CreditPosting, ...]) -> None:
        assert sum(item.amount_units for item in postings) == 0
        for item in postings:
            self.balances[item.wallet_id] += item.amount_units
        self.history.append(postings)

    def assert_invariants(self) -> None:
        assert sum(self.balances.values()) == 0
        assert self.balances[self.customer_a] >= 0
        assert self.balances[self.customer_b] >= 0
        assert self.balances[self.revenue] >= 0
        assert self.available(self.customer_a) >= 0
        assert self.available(self.customer_b) >= 0


operation = st.tuples(
    st.sampled_from(["issue", "transfer", "hold", "settle", "release", "refund"]),
    st.integers(min_value=1, max_value=10_000),
)


@settings(max_examples=10_000, deadline=None)
@given(st.lists(operation, min_size=1, max_size=40))
def test_random_credit_sequences_preserve_all_invariants(
    operations: list[tuple[str, int]],
) -> None:
    state = LedgerState()
    for index, (kind, amount) in enumerate(operations):
        if kind == "issue":
            state.post(balanced_transfer(state.treasury, state.customer_a, amount))
        elif kind == "transfer" and state.available(state.customer_a) >= amount:
            state.post(balanced_transfer(state.customer_a, state.customer_b, amount))
        elif kind == "hold" and state.available(state.customer_a) >= amount:
            state.holds[f"hold-{index}"] = (state.customer_a, amount)
        elif kind == "settle" and state.holds:
            key = next(iter(state.holds))
            wallet, reserved = state.holds.pop(key)
            actual = min(amount, reserved)
            state.post(balanced_transfer(wallet, state.revenue, actual))
        elif kind == "release" and state.holds:
            state.holds.pop(next(iter(state.holds)))
        elif kind == "refund" and state.history:
            original = state.history[-1]
            candidate = reverse_postings(original)
            debits_are_available = all(
                posting.amount_units >= 0 or state.available(posting.wallet_id) >= -posting.amount_units
                for posting in candidate
                if posting.wallet_id != state.treasury
            )
            if debits_are_available:
                state.post(candidate)
        state.assert_invariants()
```

- [ ] **Step 2: Run the randomized test and record its example count**

Run: `cd backend && uv run pytest tests/unit/billing/test_randomized_ledgers.py -v --hypothesis-show-statistics`

Expected: PASS with `10,000 passing examples`; every generated sequence checks zero-sum postings, non-negative customer/revenue balances, and active-hold availability after every action.

- [ ] **Step 3: Add a PostgreSQL reconstruction check after mixed real operations**

Append to `backend/tests/integration/billing/test_credit_ledger.py`:

```python
def test_every_wallet_cache_reconstructs_after_mixed_operations(db_session: Session) -> None:
    service, treasury, revenue, customer = setup_wallets(db_session)
    service.issue(db_session, treasury.id, customer.id, 1000, uuid4(), "mixed-issue")
    hold = service.reserve(db_session, customer.id, 500, "mixed-hold", uuid4())
    service.settle(db_session, hold.id, revenue.id, 350, uuid4(), "mixed-settle")
    for wallet in (treasury, revenue, customer):
        assert wallet.posted_balance == service.reconstruct_balance(db_session, wallet.id)
    assert db_session.scalar(select(func.sum(CreditLedgerEntry.amount_units))) == 0
```

- [ ] **Step 4: Run randomized and PostgreSQL ledger suites together**

Run: `cd backend && uv run pytest tests/unit/billing tests/integration/billing -v`

Expected: PASS; the 10,000 pure sequences and PostgreSQL append/locking tests agree on all invariants.

- [ ] **Step 5: Correct the integration command so it never filters unmarked PostgreSQL tests**

Replace the `test-integration` target in `Makefile` with:

```make
test-integration:
	cd backend && uv run pytest tests/integration tests/contract tests/security tests/providers -v
```

- [ ] **Step 6: Add CI with a real PostgreSQL service**

Create `.github/workflows/ci.yml`:

```yaml
name: ci
on:
  pull_request:
  push:
    branches: [main]
jobs:
  verify:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:16.4
        env:
          POSTGRES_USER: ip_saas
          POSTGRES_PASSWORD: ip_saas
          POSTGRES_DB: ip_saas_test
        ports: ["5432:5432"]
        options: >-
          --health-cmd "pg_isready -U ip_saas -d ip_saas_test"
          --health-interval 2s --health-timeout 2s --health-retries 20
    env:
      DATABASE_URL: postgresql+psycopg://ip_saas:ip_saas@localhost:5432/ip_saas_test
      TEST_DATABASE_URL: postgresql+psycopg://ip_saas:ip_saas@localhost:5432/ip_saas_test
      JWT_SECRET: ci-secret-with-at-least-thirty-two-characters
    steps:
      - uses: actions/checkout@v4
      - uses: astral-sh/setup-uv@v6
        with: {version: "0.8.13", python-version: "3.12"}
      - uses: pnpm/action-setup@v4
        with: {version: "10.15.0"}
      - uses: actions/setup-node@v4
        with: {node-version: "24", cache: "pnpm", cache-dependency-path: "frontend/pnpm-lock.yaml"}
      - run: make bootstrap
      - run: cd frontend && pnpm exec playwright install --with-deps chromium
      - run: make lint
      - run: make export-contracts
      - run: make test
      - run: cd frontend && pnpm build
```

- [ ] **Step 7: Add plain-language setup and acceptance instructions**

Create `README.md`:

```markdown
# AI IP SaaS

This repository implements the approved SaaS as six ordered subprojects. Plan 01 supplies the shared authentication, project isolation, billing, task, audit, outbox, model, storage, API, and web contracts.

## Local verification

1. Copy `.env.example` to `.env` and replace `JWT_SECRET` with at least 32 random characters.
2. Run `make services-up`.
3. Run `make bootstrap`.
4. Run `cd backend && uv run alembic upgrade head`.
5. Run `make lint`, `make export-contracts`, and `make test`.
6. Run `make test-e2e` after `cd frontend && pnpm exec playwright install chromium`.

Ordinary tests use fake external providers and never require paid Volcengine credentials. PostgreSQL is mandatory for integration tests because wallet locks, uniqueness, JSONB, partial indexes, and append-only triggers are part of the product behavior.

## Foundation acceptance

- `alembic current` prints `0001_foundation (head)`.
- `make export-contracts` exits without drift.
- `make test` passes, including 10,000 randomized ledger sequences and cross-tenant security cases.
- A C user can own multiple projects but cannot read another account's project by changing an ID.
- Platform projects use internal CNY holds; customer projects use credit holds.
- A task, its hold, audit event, and outbox event either all commit or all roll back.
- A retry-exhausted task retains its hold in `reconciliation_required` until durable zero-call evidence releases it or matched task-linked supplier cost settles it; pending reconciliation is never presented as final failure.
- No ordinary test makes a paid provider call.
```

- [ ] **Step 8: Run the full foundation gate**

Run:

```bash
make bootstrap
make lint
make export-contracts
make test
cd frontend && pnpm build
```

Expected: every command exits 0; OpenAPI/events show no drift; all integration tests identify PostgreSQL; the Hypothesis report contains 10,000 passing examples.

- [ ] **Step 9: Commit CI, property tests, and operator documentation**

```bash
git add backend/tests/unit/billing/test_randomized_ledgers.py backend/tests/integration/billing/test_credit_ledger.py .github/workflows/ci.yml Makefile README.md
git commit -m "test: gate platform foundation delivery"
```

## Plan 01 acceptance checklist

- [ ] Authentication issues short-lived access tokens and server-tracked refresh sessions; disabled/revoked principals cannot authenticate.
- [ ] Only platform→L1→L2→C and platform→L1→C ownership paths can be created; C-user accounts have one principal because team mode is excluded.
- [ ] `ProjectAccessService.require_viewer` and `require_editor` reject cross-account object IDs without disclosing project existence.
- [ ] Platform internal projects and customer projects use immutable owner types and cannot be converted into one another.
- [ ] Credit transfers are balanced, user wallets remain non-negative after active holds, duplicate idempotency keys do not post twice, and reversals append new entries.
- [ ] `BillingService` always receives an explicit generation-limit port; the pre-Plan-05 production adapter denies all reservations, while tests must name and pass their allow fake rather than relying on an unlimited default.
- [ ] Internal cost centers reconstruct integer-fen availability from append-only entries and separate holds; no platform internal task consumes reseller/customer credit.
- [ ] The four frozen `BillingService` methods and both hold ports are importable at the paths listed in this plan.
- [ ] Provider costs preserve provider, capability, model/version, native quantity/unit, supplier minor amount/currency, converted fen, billing context, and reconciliation status.
- [ ] Hold, task, audit, and outbox creation share one PostgreSQL transaction; injected failure leaves none of them behind.
- [ ] Task idempotency is unique within the initiating account, not globally across customers; a retry freezes project, account, billing mode/scope, maximum reservation, capability, and input payload in one SHA-256 fingerprint and cannot bypass tenant access.
- [ ] The RocketMQ 5.x adapter preserves the frozen envelope and strict task payload; send receipts are stored, failures back off, and exhausted events enter an inspectable dead-letter state without leaking secrets.
- [ ] Active task leases retry later, terminal replays ACK, expired leases cannot heartbeat or finish even before reclaim, expired reclaim increments `attempt_no`, and a stale Worker cannot write a result, settle/release billing, or acknowledge success.
- [ ] Retry exhaustion enters non-terminal `reconciliation_required` with the original hold untouched; its delivery retries until the explicit reconciliation service proves zero provider calls or persists matched cost tied to the current task, then atomically releases/settles and fails with fingerprinted idempotency.
- [ ] Every fake or real provider completion entering settlement carries the current `TaskRecord.id` and a canonical 1–200-character provider request identity; settlement/release and terminal task state commit together and remain idempotent. A proven zero-call path constructs no provider-cost value or row.
- [ ] Only regression-passed candidates become the active model for a capability; model IDs remain configuration data rather than business-code constants.
- [ ] Private objects use account/project-scoped keys and signed URLs of at most one hour; fake storage passes without credentials.
- [ ] `0001_foundation` has `down_revision = None`, upgrades cleanly, matches SQLAlchemy metadata, and downgrades cleanly.
- [ ] Checked-in OpenAPI and event schemas exactly match generated contracts.
- [ ] The C-user, reseller, and platform web shells route from authenticated actor kind and never expose team collaboration or online payment controls.
- [ ] At least 10,000 randomized ledger sequences, PostgreSQL concurrency tests, tenant-security tests, migration tests, contract tests, frontend tests, and the production build pass.

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-24-01-platform-foundation-ledger.md`. Two execution options:

1. **Subagent-Driven (recommended)** — dispatch a fresh subagent per task, review each task for spec and code quality, then continue.
2. **Inline Execution** — use `superpowers:executing-plans` in this session and execute tasks in batches with review checkpoints.
