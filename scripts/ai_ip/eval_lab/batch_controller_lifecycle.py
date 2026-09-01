"""All-exit terminalization for one reserved candidate pair."""

import time
from pathlib import Path

try:
    from .batch_isolation import cleanup_attempt_cell, mark_receipts_sealed
    from .batch_orphan_authority import seal_orphan_authority
    from .batch_controller_process_lease import (
        OrphanedProcess,
        ProcessLeaseState,
        ProcessLifecycleLease,
    )
    from .batch_receipt_storage import (
        entry_exists,
        seal_failure_tombstone,
        write_exclusive,
    )
    from .contracts import canonical_json_bytes
except ImportError:
    from batch_isolation import cleanup_attempt_cell, mark_receipts_sealed
    from batch_orphan_authority import seal_orphan_authority
    from batch_controller_process_lease import (
        OrphanedProcess,
        ProcessLeaseState,
        ProcessLifecycleLease,
    )
    from batch_receipt_storage import (
        entry_exists,
        seal_failure_tombstone,
        write_exclusive,
    )
    from contracts import canonical_json_bytes


class PairLifecycle:
    """Tracks evidence and cells until verified pair completion or terminal failure."""

    def __init__(self, private_root: Path, pair_id: str) -> None:
        self.private_root = Path(private_root)
        self.pair_id = pair_id
        self.layout = None
        self.cells: list[object] = []
        self.processes: list[object] = []
        self._leases: dict[tuple[str, str], ProcessLifecycleLease] = {}
        self._process_leases: dict[int, ProcessLifecycleLease] = {}
        self.completed = False

    def __enter__(self) -> "PairLifecycle":
        return self

    def __exit__(self, error_type, error, traceback) -> bool:
        if error is None:
            self.complete()
            return False
        self.abort(error)
        raise PairLifecycleError(f"pair lifecycle failed: {error}") from error

    def bind_layout(self, layout: object) -> None:
        self.layout = layout

    def bind_cell(self, cell: object) -> None:
        self.cells.append(cell)

    def reserve_process(
        self, arm_class: str, attempt_id: str, launch_spec_sha256: str
    ) -> ProcessLifecycleLease:
        key = (arm_class, attempt_id)
        if key in self._leases:
            raise PairLifecycleError("process lease is already reserved")
        lease = ProcessLifecycleLease(
            self.pair_id, arm_class, attempt_id, launch_spec_sha256
        )
        self._leases[key] = lease
        return lease

    def promote_process(self, lease: ProcessLifecycleLease, process: object) -> None:
        if lease not in self._leases.values():
            raise PairLifecycleError("process lease does not belong to this pair")
        lease.promote(process)
        self.processes.append(process)
        self._process_leases[id(process)] = lease

    @property
    def orphaned_processes(self) -> tuple[OrphanedProcess, ...]:
        return tuple(
            orphan
            for lease in self._leases.values()
            if (orphan := lease.orphaned_process) is not None
        )

    def complete(self) -> None:
        unsafe_states = {
            ProcessLeaseState.RESERVED,
            ProcessLeaseState.ATTACHED_RAW_PROCESS,
            ProcessLeaseState.PROMOTED_OWNED_PROCESS,
            ProcessLeaseState.ORPHANED,
        }
        if any(lease.state in unsafe_states for lease in self._leases.values()):
            raise PairLifecycleError("pair process lease is not safely terminal")
        if any(not item.stopped for item in self.processes):
            raise PairLifecycleError("pair process is still live at completion")
        for cell in self.cells:
            try:
                cleanup_attempt_cell(cell)
            except BaseException as error:
                self._record_orphan(error)
                raise PairLifecycleError(
                    "sealed attempt cell cleanup failed"
                ) from error
        self.completed = True

    def _tombstone(self, directory: object, error: BaseException) -> None:
        path = directory.path
        if not entry_exists(path, "failure.json"):
            seal_failure_tombstone(directory, self.pair_id, "terminal", error)

    def _record_orphan(self, error: BaseException) -> None:
        payload = (
            canonical_json_bytes(
                {
                    "attemptIds": sorted(cell.attempt_id for cell in self.cells),
                    "errorType": type(error).__name__,
                    "pairId": self.pair_id,
                    "status": "orphaned",
                }
            )
            + b"\n"
        )
        write_exclusive(self.private_root / f"orphan-{self.pair_id}.json", payload)

    def abort(self, error: BaseException) -> None:
        if self.completed:
            return
        if self.layout is not None:
            for directory in (
                self.layout.pair_directory,
                *self.layout.attempt_directories,
            ):
                try:
                    self._tombstone(directory, error)
                except BaseException:
                    pass
        if self.processes:
            try:
                from .batch_controller_process import close_owned, terminate_and_wait
            except ImportError:
                from batch_controller_process import close_owned, terminate_and_wait

            for process in self.processes:
                lease = self._process_leases[id(process)]
                try:
                    if lease.state is ProcessLeaseState.STOP_CONFIRMED:
                        continue
                    try:
                        confirmed = terminate_and_wait(process, time.monotonic() + 1.0)
                    except BaseException as stop_error:
                        lease.mark_orphaned(stop_error)
                    else:
                        if confirmed:
                            if lease.state is ProcessLeaseState.PROMOTED_OWNED_PROCESS:
                                lease.confirm_stopped()
                        else:
                            lease.mark_orphaned(error)
                finally:
                    close_owned(process)
        orphaned_processes = self.orphaned_processes
        if orphaned_processes:
            sealing_error = None
            for orphan in orphaned_processes:
                try:
                    seal_orphan_authority(self.layout.root_directory, orphan)
                except BaseException as error:
                    if sealing_error is None:
                        sealing_error = error
            if sealing_error is not None:
                raise PairLifecycleError(
                    "authenticated orphan authority sealing failed"
                ) from sealing_error
            return
        for cell in self.cells:
            try:
                mark_receipts_sealed(cell)
            except BaseException:
                pass
            try:
                cleanup_attempt_cell(cell)
            except BaseException:
                try:
                    self._record_orphan(error)
                except BaseException:
                    pass


class PairLifecycleError(ValueError):
    pass
