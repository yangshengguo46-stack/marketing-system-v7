"""All-exit terminalization for one reserved candidate pair."""

import time
from pathlib import Path

try:
    from .batch_controller_capture import FatalSupervisorError
    from .batch_isolation import cleanup_attempt_cell, mark_receipts_sealed
    from .batch_receipt_storage import (
        entry_exists,
        seal_failure_tombstone,
        write_exclusive,
    )
    from .contracts import canonical_json_bytes
except ImportError:
    from batch_controller_capture import FatalSupervisorError
    from batch_isolation import cleanup_attempt_cell, mark_receipts_sealed
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

    def bind_process(self, process: object) -> None:
        """Own a concrete process immediately after the OS returns its handle."""
        self.processes.append(process)

    def complete(self) -> None:
        if any(item.process.poll() is None for item in self.processes):
            raise PairLifecycleError("pair process is still live at completion")
        for cell in self.cells:
            try:
                cleanup_attempt_cell(cell)
            except BaseException as error:
                self._record_orphan(error)
                raise PairLifecycleError("sealed attempt cell cleanup failed") from error
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
        orphaned = isinstance(error, FatalSupervisorError)
        if self.processes:
            try:
                from .batch_controller_process import terminate_and_wait
            except ImportError:
                from batch_controller_process import terminate_and_wait

            for process in self.processes:
                try:
                    if not terminate_and_wait(process, time.monotonic() + 1.0):
                        orphaned = True
                except BaseException:
                    orphaned = True
        if orphaned:
            try:
                self._record_orphan(error)
            except BaseException:
                pass
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
