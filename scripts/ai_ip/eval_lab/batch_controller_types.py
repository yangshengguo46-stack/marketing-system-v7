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


@dataclass(frozen=True)
class RawAttemptResult:
    exit_code: int
    started_at: str
    finished_at: str
    output: object | None
    metadata: object | None
    stdout: bytes
    stderr: bytes


class CandidateExecutor(Protocol):
    """Execute one isolated request exactly once and return its complete raw evidence."""

    def execute(self, request: AttemptRequest) -> RawAttemptResult: ...
