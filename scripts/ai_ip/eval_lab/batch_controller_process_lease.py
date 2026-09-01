"""One-way process lifecycle authority with immutable orphan facts."""

import re
from dataclasses import dataclass, field
from enum import Enum


_LOWER_HEX_64 = re.compile(r"[0-9a-f]{64}")
_ARM_CLASSES = frozenset({"stock", "modified"})


class ProcessLeaseError(ValueError):
    pass


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
    _state: ProcessLeaseState = field(
        init=False, default=ProcessLeaseState.RESERVED, repr=False
    )
    _process_id: int | None = field(init=False, default=None, repr=False)
    _process_group_id: int | None = field(init=False, default=None, repr=False)
    _owned_process: object | None = field(init=False, default=None, repr=False)
    _orphaned_process: OrphanedProcess | None = field(
        init=False, default=None, repr=False
    )

    def __post_init__(self) -> None:
        for name, value in (
            ("pair_id", self.pair_id),
            ("attempt_id", self.attempt_id),
            ("launch_spec_sha256", self.launch_spec_sha256),
        ):
            if type(value) is not str or _LOWER_HEX_64.fullmatch(value) is None:
                raise ProcessLeaseError(f"{name} must be exact lower-case 64-hex")
        if self.arm_class not in _ARM_CLASSES:
            raise ProcessLeaseError("arm_class must be stock or modified")

    @property
    def state(self) -> ProcessLeaseState:
        return self._state

    @property
    def orphaned_process(self) -> OrphanedProcess | None:
        return self._orphaned_process

    def _require_state(self, *allowed: ProcessLeaseState) -> None:
        if self._state not in allowed:
            raise ProcessLeaseError(
                f"process lease state {self._state.value} does not allow transition"
            )

    def attach_raw_process(self, process_id: int, process_group_id: int) -> None:
        self._require_state(ProcessLeaseState.RESERVED)
        if type(process_id) is not int or process_id <= 0:
            raise ProcessLeaseError("process_id must be a positive integer")
        if type(process_group_id) is not int or process_group_id <= 0:
            raise ProcessLeaseError("process_group_id must be a positive integer")
        self._process_id = process_id
        self._process_group_id = process_group_id
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
        if self._state is ProcessLeaseState.ORPHANED:
            assert self._orphaned_process is not None
            return self._orphaned_process
        self._require_state(
            ProcessLeaseState.ATTACHED_RAW_PROCESS,
            ProcessLeaseState.PROMOTED_OWNED_PROCESS,
        )
        assert self._process_id is not None
        assert self._process_group_id is not None
        orphan = OrphanedProcess(
            pair_id=self.pair_id,
            arm_class=self.arm_class,
            attempt_id=self.attempt_id,
            process_id=self._process_id,
            process_group_id=self._process_group_id,
            launch_spec_sha256=self.launch_spec_sha256,
            error_type=type(error).__name__,
        )
        self._orphaned_process = orphan
        self._state = ProcessLeaseState.ORPHANED
        return orphan
