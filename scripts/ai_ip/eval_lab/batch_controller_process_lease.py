from dataclasses import dataclass, field
from enum import Enum

_HEX = set("0123456789abcdef")


class ProcessLeaseError(ValueError): ...


class ProcessLeaseState(Enum):
    RESERVED = "reserved"
    ATTACHED_RAW_PROCESS = "attachedRawProcess"
    PROMOTED_OWNED_PROCESS = "promotedOwnedProcess"
    STOP_CONFIRMED = "stopConfirmed"
    ORPHANED = "orphaned"
    CANCELLED_BEFORE_START = "cancelledBeforeStart"


@dataclass(frozen=True, slots=True)
class OrphanedProcess:
    pair_id: str
    arm_class: str
    attempt_id: str
    process_id: int
    process_group_id: int
    launch_spec_sha256: str
    error_type: str


@dataclass(slots=True, eq=False)
class ProcessLifecycleLease:
    pair_id: str
    arm_class: str
    attempt_id: str
    launch_spec_sha256: str
    state: ProcessLeaseState = field(init=False, default=ProcessLeaseState.RESERVED)
    _process_id: int | None = field(init=False, default=None)
    _process_group_id: int | None = field(init=False, default=None)
    orphaned_process: OrphanedProcess | None = field(init=False, default=None)

    def __post_init__(self) -> None:
        for name, value in (
            ("pair_id", self.pair_id),
            ("attempt_id", self.attempt_id),
            ("launch_spec_sha256", self.launch_spec_sha256),
        ):
            if type(value) is not str or len(value) != 64 or set(value) - _HEX:
                raise ProcessLeaseError(f"{name} must be exact lower-case 64-hex")
        if self.arm_class not in {"stock", "modified"}:
            raise ProcessLeaseError("arm_class must be stock or modified")

    def _require_state(self, *allowed: ProcessLeaseState) -> None:
        if self.state not in allowed:
            raise ProcessLeaseError("invalid process lease state transition")

    def attach_raw_process(self, process_id: int, process_group_id: int) -> None:
        self._require_state(ProcessLeaseState.RESERVED)
        identities = (process_id, process_group_id)
        if not all(type(value) is int and value > 0 for value in identities):
            raise ProcessLeaseError("process identity must use positive integers")
        self._process_id, self._process_group_id = process_id, process_group_id
        self.state = ProcessLeaseState.ATTACHED_RAW_PROCESS

    def promote(self, owned_process: object) -> None:
        self._require_state(ProcessLeaseState.ATTACHED_RAW_PROCESS)
        self.state = ProcessLeaseState.PROMOTED_OWNED_PROCESS

    def cancel_before_start(self) -> None:
        self._require_state(ProcessLeaseState.RESERVED)
        self.state = ProcessLeaseState.CANCELLED_BEFORE_START

    def confirm_stopped(self) -> None:
        self._require_state(
            ProcessLeaseState.ATTACHED_RAW_PROCESS,
            ProcessLeaseState.PROMOTED_OWNED_PROCESS,
        )
        self.state = ProcessLeaseState.STOP_CONFIRMED

    def mark_orphaned(self, error: BaseException) -> OrphanedProcess:
        if self.orphaned_process is not None:
            return self.orphaned_process
        self._require_state(
            ProcessLeaseState.ATTACHED_RAW_PROCESS,
            ProcessLeaseState.PROMOTED_OWNED_PROCESS,
        )
        assert self._process_id is not None and self._process_group_id is not None
        identity = self.pair_id, self.arm_class, self.attempt_id
        process = self._process_id, self._process_group_id
        spec, kind = self.launch_spec_sha256, type(error).__name__
        self.orphaned_process = OrphanedProcess(*identity, *process, spec, kind)
        self.state = ProcessLeaseState.ORPHANED
        return self.orphaned_process
