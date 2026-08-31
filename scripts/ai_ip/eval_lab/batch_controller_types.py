"""Public, capability-bearing executor types for the paired controller."""

import os
from dataclasses import dataclass
from pathlib import Path

try:
    from .batch_controller_snapshots import FrozenTree
except ImportError:
    from batch_controller_snapshots import FrozenTree


_EXECUTOR_CAPABILITY = object()
_HANDLE_CAPABILITY = object()
_SOURCE_CAPABILITY = object()


class AttemptByteSource:
    """A bounded-read byte source owned by an audited execution adapter."""

    def __init__(self) -> None:
        self.__capability = _SOURCE_CAPABILITY
        self.__creator_pid = os.getpid()

    def read(self, maximum: int) -> bytes:
        raise NotImplementedError

    def _validate_capability(self) -> None:
        if (
            self.__capability is not _SOURCE_CAPABILITY
            or self.__creator_pid != os.getpid()
        ):
            raise ValueError("attempt byte source capability is invalid")


class MemoryAttemptByteSource(AttemptByteSource):
    """Small in-memory source for audited adapters and deterministic tests."""

    def __init__(self, payload: bytes) -> None:
        if type(payload) is not bytes:
            raise TypeError("memory attempt source requires bytes")
        super().__init__()
        self._payload = payload
        self._offset = 0

    def read(self, maximum: int) -> bytes:
        self._validate_capability()
        if type(maximum) is not int or maximum < 1:
            raise ValueError("bounded source read size must be positive")
        chunk = self._payload[self._offset : self._offset + maximum]
        self._offset += len(chunk)
        return chunk


@dataclass(frozen=True)
class AttemptRequest:
    cell: object
    binary_path: Path
    case_bundle_json: bytes
    case_answer_schema_json: bytes
    model_route_json: bytes
    execution_profile_json: bytes
    effective_config: bytes
    timeout_seconds: int
    token_budget: int
    request_budget: int
    cost_budget_cny: int
    binary_sha256: str
    codex_home_seed_sha256: str
    effective_config_sha256: str
    workspace_seed_sha256: str
    execution_profile_sha256: str
    model_route_sha256: str
    app_server_protocol_schema: bytes
    app_server_protocol_schema_sha256: str
    promptfoo_config: bytes
    promptfoo_config_sha256: str
    codex_home_seed_snapshot: FrozenTree
    workspace_seed_snapshot: FrozenTree


@dataclass(frozen=True)
class AttemptAttestation:
    binary_sha256: str
    codex_home_seed_sha256: str
    effective_config_sha256: str
    workspace_seed_sha256: str
    execution_profile_sha256: str
    model_route_sha256: str
    app_server_protocol_schema_sha256: str
    promptfoo_config_sha256: str

    @classmethod
    def measure(cls, request: AttemptRequest) -> "AttemptAttestation":
        try:
            from .batch_controller_identity import measure_request_identity
        except ImportError:
            from batch_controller_identity import measure_request_identity

        return cls(**measure_request_identity(request))


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
    output: AttemptByteSource
    metadata: AttemptByteSource
    stdout: AttemptByteSource
    stderr: AttemptByteSource
    attestation: AttemptAttestation | None = None


class RunningAttempt:
    """Audited nonblocking handle whose termination positively confirms stop."""

    def __init__(self) -> None:
        self.__capability = _HANDLE_CAPABILITY
        self.__creator_pid = os.getpid()

    def poll(self) -> RawAttemptResult | None:
        raise NotImplementedError

    def telemetry(self) -> AttemptTelemetry:
        raise NotImplementedError

    def terminate_and_wait(self, deadline: float) -> bool:
        raise NotImplementedError

    def _validate_capability(self) -> None:
        if (
            self.__capability is not _HANDLE_CAPABILITY
            or self.__creator_pid != os.getpid()
        ):
            raise ValueError("running-attempt capability is invalid")


class CandidateExecutor:
    """Audited adapter that starts one bounded, supervisor-owned attempt."""

    def __init__(self) -> None:
        self.__capability = _EXECUTOR_CAPABILITY
        self.__creator_pid = os.getpid()

    def start(self, request: AttemptRequest) -> RunningAttempt:
        raise NotImplementedError

    def _validate_capability(self) -> None:
        if (
            self.__capability is not _EXECUTOR_CAPABILITY
            or self.__creator_pid != os.getpid()
        ):
            raise ValueError("audited executor capability is invalid")


def validate_executor(executor: object) -> CandidateExecutor:
    if not isinstance(executor, CandidateExecutor):
        raise ValueError("controller requires an audited supervisor capability")
    executor._validate_capability()
    return executor


def validate_handle(handle: object) -> RunningAttempt:
    if not isinstance(handle, RunningAttempt):
        raise ValueError("executor returned an unaudited supervisor handle")
    handle._validate_capability()
    return handle


def validate_source(source: object) -> AttemptByteSource:
    if not isinstance(source, AttemptByteSource):
        raise ValueError("executor returned an unaudited byte source")
    source._validate_capability()
    return source
