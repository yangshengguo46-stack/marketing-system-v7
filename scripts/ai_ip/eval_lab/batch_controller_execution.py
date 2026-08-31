"""Audited two-arm start and supervision orchestration."""

from dataclasses import dataclass

try:
    from .batch_controller_attempt import (
        capture_arm,
        capture_arm_start_failure,
        start_arm,
    )
    from .batch_controller_identity import ExecutionIdentityError
    from .batch_controller_types import (
        AttemptRequest,
        CandidateExecutor,
        validate_executor,
    )
except ImportError:
    from batch_controller_attempt import (
        capture_arm,
        capture_arm_start_failure,
        start_arm,
    )
    from batch_controller_identity import ExecutionIdentityError
    from batch_controller_types import (
        AttemptRequest,
        CandidateExecutor,
        validate_executor,
    )


@dataclass
class CountingExecutor(CandidateExecutor):
    executor: CandidateExecutor
    calls: int = 0

    def __post_init__(self) -> None:
        CandidateExecutor.__init__(self)

    def start(self, request: AttemptRequest):
        self.calls += 1
        return self.executor.start(request)


def counting_executor(executor: object) -> CountingExecutor:
    return CountingExecutor(validate_executor(executor))


def start_and_capture_pair(
    executor: CandidateExecutor,
    prepared: dict[str, tuple[str, AttemptRequest]],
    order: tuple[str, str],
) -> dict[str, object]:
    """Start both arms once before observing either handle."""
    try:
        from .batch_controller_identity import measure_request_identity
    except ImportError:
        from batch_controller_identity import measure_request_identity

    for name in order:
        measure_request_identity(prepared[name][1])
    handles: dict[str, object] = {}
    errors: dict[str, BaseException] = {}
    for name in order:
        try:
            handles[name] = start_arm(executor, prepared[name][1])
        except BaseException as error:
            errors[name] = error
    identity_errors = [
        error for error in errors.values() if isinstance(error, ExecutionIdentityError)
    ]
    if identity_errors:
        raise identity_errors[0]
    return {
        name: (
            capture_arm_start_failure(errors[name])
            if name in errors
            else capture_arm(handles[name], prepared[name][1])
        )
        for name in order
    }
