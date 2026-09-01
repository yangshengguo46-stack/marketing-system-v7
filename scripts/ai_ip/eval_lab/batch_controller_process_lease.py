from dataclasses import dataclass
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


class ProcessLifecycleLease:
    def __init__(
        self, pair_id: str, arm_class: str, attempt_id: str, launch_spec_sha256: str
    ) -> None:
        identities = {
            "pair_id": pair_id,
            "attempt_id": attempt_id,
            "launch_spec_sha256": launch_spec_sha256,
        }
        for name, value in identities.items():
            if type(value) is not str or len(value) != 64 or set(value) - _HEX:
                raise ProcessLeaseError(f"{name} must be exact lower-case 64-hex")
        if arm_class not in {"stock", "modified"}:
            raise ProcessLeaseError("arm_class must be stock or modified")
        self._identity = pair_id, arm_class, attempt_id, launch_spec_sha256
        self._state = ProcessLeaseState.RESERVED
        self._process_id = self._process_group_id = None
        self._owned_process = self._orphaned_process = None

    pair_id = property(lambda self: self._identity[0])
    arm_class = property(lambda self: self._identity[1])
    attempt_id = property(lambda self: self._identity[2])
    launch_spec_sha256 = property(lambda self: self._identity[3])

    state = property(lambda self: self._state)
    orphaned_process = property(lambda self: self._orphaned_process)

    def _require_state(self, *allowed: ProcessLeaseState) -> None:
        if self._state not in allowed:
            raise ProcessLeaseError("invalid process lease state transition")

    def attach_raw_process(self, process_id: int, process_group_id: int) -> None:
        self._require_state(ProcessLeaseState.RESERVED)
        identities = (process_id, process_group_id)
        if not all(type(value) is int and value > 0 for value in identities):
            raise ProcessLeaseError("process identity must use positive integers")
        self._process_id, self._process_group_id = process_id, process_group_id
        self._state = ProcessLeaseState.ATTACHED_RAW_PROCESS

    def promote(self, owned_process: object) -> None:
        self._require_state(ProcessLeaseState.ATTACHED_RAW_PROCESS)
        self._owned_process = owned_process
        self._state = ProcessLeaseState.PROMOTED_OWNED_PROCESS

    def cancel_before_start(self) -> None:
        self._require_state(ProcessLeaseState.RESERVED)
        self._state = ProcessLeaseState.CANCELLED_BEFORE_START

    def confirm_stopped(self) -> None:
        self._require_state(
            ProcessLeaseState.ATTACHED_RAW_PROCESS,
            ProcessLeaseState.PROMOTED_OWNED_PROCESS,
        )
        self._state = ProcessLeaseState.STOP_CONFIRMED

    def mark_orphaned(self, error: BaseException) -> OrphanedProcess:
        if self._orphaned_process is not None:
            return self._orphaned_process
        self._require_state(
            ProcessLeaseState.ATTACHED_RAW_PROCESS,
            ProcessLeaseState.PROMOTED_OWNED_PROCESS,
        )
        assert self._process_id is not None and self._process_group_id is not None
        identity = self.pair_id, self.arm_class, self.attempt_id
        process = self._process_id, self._process_group_id
        spec, kind = self.launch_spec_sha256, type(error).__name__
        self._orphaned_process = OrphanedProcess(*identity, *process, spec, kind)
        self._state = ProcessLeaseState.ORPHANED
        return self._orphaned_process
