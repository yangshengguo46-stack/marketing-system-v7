"""Retryable handle-bound rollback journal for Windows cell construction."""

import os
from dataclasses import dataclass
from typing import Protocol

try:
    from .batch_isolation_windows_handles import HandleOwnership
    from .batch_isolation_windows_handles import HandleOwnershipError
    from .batch_isolation_windows_handles import bind_identity
    from .batch_isolation_windows_handles import close_owned
    from .batch_isolation_windows_handles import primary_is_owned
except ImportError:
    from batch_isolation_windows_handles import HandleOwnership
    from batch_isolation_windows_handles import HandleOwnershipError
    from batch_isolation_windows_handles import bind_identity
    from batch_isolation_windows_handles import close_owned
    from batch_isolation_windows_handles import primary_is_owned


class JournalRollbackError(OSError):
    pass


class JournalOperations(Protocol):
    def duplicate(self, handle: int) -> int: ...

    def identity(self, handle: int) -> tuple[int, int]: ...

    def mark_delete(self, handle: int) -> None: ...

    def delete_pending(self, handle: int) -> bool: ...

    def close(self, handle: int) -> None: ...

    def open_live(self, parent: int, name: str, *, directory: bool) -> int | None: ...


@dataclass
class JournalEntry:
    handle: int | None
    parent: int
    name: str
    directory: bool
    identity: tuple[int, int] | None
    expected_identity: tuple[int, int] | None = None
    proof_handle: int | None = None
    state: str = "identity_unknown"
    proof_close_uncertain: bool = False
    handle_close_uncertain: bool = False


class WindowsConstructionJournal:
    def __init__(self, operations: JournalOperations, base_handle: int) -> None:
        self.operations = operations
        self.base_handle = base_handle
        self.base_ownership = HandleOwnership(base_handle)
        self.creator_pid = os.getpid()
        self.entries: list[JournalEntry] = []
        self.probes: list[JournalEntry] = []
        self.state = "constructing"

    def record(self, handle: int, parent: int, name: str, *, directory: bool) -> None:
        self._require_owner()
        entry = JournalEntry(handle, parent, name, directory, None)
        self.entries.append(entry)
        self._bind(entry)

    def complete(self) -> None:
        self._require_owner()
        bind_identity(self.base_ownership, self.operations, self.creator_pid)
        self.base_ownership.handle = None
        self.base_ownership.state = "transferred"
        self.entries.clear()
        self.probes.clear()
        self.base_handle = -1
        self.state = "complete"

    def _require_owner(self) -> None:
        if self.creator_pid != os.getpid():
            raise JournalRollbackError(
                "construction journal belongs to another process"
            )

    def _bind(self, entry: JournalEntry) -> tuple[int, int]:
        try:
            return bind_identity(
                entry,
                self.operations,
                self.creator_pid,
                expected_identity=entry.expected_identity,
            )
        except OSError as error:
            raise JournalRollbackError(
                "construction handle identity is unproven"
            ) from error

    def _close_entry(self, entry: JournalEntry) -> None:
        try:
            close_owned(entry, self.operations, self.creator_pid)
        except OSError as error:
            raise JournalRollbackError(
                "construction handle close is unproven"
            ) from error

    def _retry_probes(self) -> None:
        for probe in tuple(self.probes):
            try:
                self._bind(probe)
            except JournalRollbackError as error:
                if not isinstance(error.__cause__, HandleOwnershipError) or (
                    "substituted" not in str(error.__cause__)
                ):
                    raise
            self._close_entry(probe)
            self.probes.remove(probe)

    def _live_identity(self, entry: JournalEntry) -> tuple[int, int] | None:
        handle = self.operations.open_live(
            entry.parent, entry.name, directory=entry.directory
        )
        if handle is None:
            return None
        probe = JournalEntry(
            handle,
            entry.parent,
            entry.name,
            entry.directory,
            None,
            entry.identity,
        )
        self.probes.append(probe)
        actual = self._bind(probe)
        self._close_entry(probe)
        self.probes.remove(probe)
        return actual

    def rollback(self) -> None:
        self._require_owner()
        try:
            self._retry_probes()
            while self.entries:
                entry = self.entries[-1]
                if entry.handle is not None:
                    self._bind(entry)
                    if not primary_is_owned(entry, self.operations, self.creator_pid):
                        raise JournalRollbackError("rollback handle identity changed")
                    self.operations.mark_delete(entry.handle)
                    if not self.operations.delete_pending(entry.handle):
                        raise JournalRollbackError(
                            "rollback disposition was not pending"
                        )
                    self._close_entry(entry)
                live = self._live_identity(entry)
                if live is not None:
                    raise JournalRollbackError("rollback removal is not proven")
                self.entries.pop()
            bind_identity(self.base_ownership, self.operations, self.creator_pid)
            close_owned(self.base_ownership, self.operations, self.creator_pid)
            self.base_handle = -1
            self.state = "cleaned"
        except OSError as error:
            self.state = "orphan"
            if isinstance(error, JournalRollbackError):
                raise
            raise JournalRollbackError("construction rollback is unproven") from error

    def retry_cleanup(self) -> None:
        if self.state == "cleaned":
            return
        self.rollback()

    def close(self) -> None:
        for probe in reversed(self.probes):
            if probe.handle is not None:
                try:
                    close_owned(probe, self.operations, self.creator_pid)
                except OSError:
                    pass
        self.probes.clear()
        for entry in reversed(self.entries):
            if entry.handle is not None:
                try:
                    close_owned(entry, self.operations, self.creator_pid)
                except OSError:
                    pass
        if self.base_handle >= 0:
            try:
                close_owned(self.base_ownership, self.operations, self.creator_pid)
            except OSError:
                pass
            self.base_handle = -1
