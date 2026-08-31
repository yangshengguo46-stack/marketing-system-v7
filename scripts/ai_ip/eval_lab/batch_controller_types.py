"""Public executor value types for the sealed paired controller."""

from dataclasses import dataclass
from pathlib import Path
from typing import Mapping, Protocol

try:
    from .batch_isolation import AttemptCell
except ImportError:
    from batch_isolation import AttemptCell


@dataclass(frozen=True)
class AttemptRequest:
    cell: AttemptCell
    binary_path: Path
    case_bundle: object
    case_answer_schema: object
    model_route: Mapping[str, object]
    execution_profile: object
    timeout_seconds: int
    token_budget: int
    request_budget: int
    cost_budget_cny: int
    codex_home_seed_sha256: str
    effective_config_sha256: str
    workspace_seed_sha256: str
    execution_profile_sha256: str
    model_route_sha256: str
    app_server_protocol_schema: bytes
    app_server_protocol_schema_sha256: str
    promptfoo_config: bytes
    promptfoo_config_sha256: str


@dataclass(frozen=True)
class AttemptAttestation:
    binary_sha256: str
    codex_home_seed_sha256: str
    effective_config_sha256: str
    execution_profile_sha256: str
    model_route_sha256: str
    app_server_protocol_schema_sha256: str
    promptfoo_config_sha256: str

    @classmethod
    def from_request(cls, request: AttemptRequest) -> "AttemptAttestation":
        import hashlib

        return cls(
            hashlib.sha256(request.binary_path.read_bytes()).hexdigest(),
            request.codex_home_seed_sha256,
            request.effective_config_sha256,
            request.execution_profile_sha256,
            request.model_route_sha256,
            hashlib.sha256(request.app_server_protocol_schema).hexdigest(),
            hashlib.sha256(request.promptfoo_config).hexdigest(),
        )


@dataclass(frozen=True)
class AttemptTelemetry:
    request_count: int
    input_tokens: int
    output_tokens: int
    cost_cny: int


@dataclass(frozen=True)
class RawAttemptResult:
    exit_code: int
    started_at: str
    finished_at: str
    output: object | None
    metadata: object | None
    stdout: bytes
    stderr: bytes
    attestation: AttemptAttestation | None = None


class RunningAttempt(Protocol):
    """Trusted supervisor handle; poll is nonblocking and terminate confirms stop."""

    def poll(self) -> RawAttemptResult | None: ...

    def telemetry(self) -> AttemptTelemetry: ...

    def terminate(self) -> bool: ...


class CandidateExecutor(Protocol):
    """Trusted adapter whose start is nonblocking and whose telemetry is supervisor-owned."""

    def start(self, request: AttemptRequest) -> RunningAttempt: ...
