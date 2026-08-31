"""PID-bound ledger for public Win32 open and duplicate boundaries."""

import os
from types import SimpleNamespace

try:
    from .batch_isolation_windows_handles import HandleOwnership
    from .batch_isolation_windows_handles import bind_identity
    from .batch_isolation_windows_handles import close_owned
except ImportError:
    from batch_isolation_windows_handles import HandleOwnership
    from batch_isolation_windows_handles import bind_identity
    from batch_isolation_windows_handles import close_owned


class WindowsHandleLedgerError(OSError):
    pass


class WindowsHandleLedger:
    def __init__(self, operations: object) -> None:
        if isinstance(operations, tuple):
            duplicate, identity, close = operations
            operations = SimpleNamespace(
                duplicate=duplicate, identity=identity, close=close
            )
        self.operations = operations
        self.creator_pid = os.getpid()
        self.records: list[HandleOwnership] = []

    def _require_owner(self) -> None:
        if self.creator_pid != os.getpid():
            raise WindowsHandleLedgerError(
                "Windows handle ledger belongs to another process"
            )

    def _remove_if_closed(self, record: HandleOwnership) -> None:
        if record.handle is None and record.proof_handle is None:
            if record in self.records:
                self.records.remove(record)

    def _supersede_closed_generation(self, handle: int) -> None:
        for record in tuple(self.records):
            if record.handle != handle:
                continue
            if not record.close_requested:
                raise WindowsHandleLedgerError(
                    "Windows returned an already-owned handle value"
                )
            record.handle = None
            record.state = "stale_reused"
            self._remove_if_closed(record)

    def acquire(
        self,
        handle: int,
        *,
        expected_identity: tuple[int, int] | None = None,
    ) -> int:
        """Register raw ownership before binding it through a duplicate proof."""
        self._require_owner()
        self._supersede_closed_generation(handle)
        record = HandleOwnership(handle)
        self.records.append(record)
        try:
            bind_identity(
                record,
                self.operations,
                self.creator_pid,
                expected_identity=expected_identity,
            )
        except OSError as bind_error:
            try:
                close_owned(record, self.operations, self.creator_pid)
            except OSError:
                pass
            self._remove_if_closed(record)
            raise WindowsHandleLedgerError(
                "Windows handle acquisition identity is unproven"
            ) from bind_error
        return handle

    def close(self, handle: int) -> None:
        self._require_owner()
        record = next(
            (
                candidate
                for candidate in reversed(self.records)
                if candidate.handle == handle
            ),
            None,
        )
        if record is None:
            self.operations.close(handle)
            return
        try:
            close_owned(record, self.operations, self.creator_pid)
        finally:
            self._remove_if_closed(record)

    def transfer(self, handle: int) -> tuple[int, int] | None:
        """Move a live handle into another journal without an unowned interval."""
        self._require_owner()
        record = next(
            (
                candidate
                for candidate in reversed(self.records)
                if candidate.handle == handle
            ),
            None,
        )
        if record is None:
            return None
        if (
            record.close_requested
            or record.identity is None
            or record.proof_handle is not None
        ):
            raise WindowsHandleLedgerError(
                "Windows handle ownership transfer is unproven"
            )
        identity = record.identity
        record.handle = None
        record.state = "transferred"
        self.records.remove(record)
        return identity

    def retry_uncertain_closes(self) -> None:
        self._require_owner()
        failures = 0
        for record in tuple(self.records):
            if not record.close_requested:
                continue
            try:
                close_owned(record, self.operations, self.creator_pid)
            except OSError:
                failures += 1
            self._remove_if_closed(record)
        if failures:
            raise WindowsHandleLedgerError(
                "Windows handle ledger still has uncertain closes"
            )
