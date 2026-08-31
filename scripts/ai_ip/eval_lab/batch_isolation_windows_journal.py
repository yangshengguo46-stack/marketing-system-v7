"""Retryable handle-bound rollback journal for Windows cell construction."""

import os
from dataclasses import dataclass
from typing import Protocol


class JournalRollbackError(OSError):
    pass


class JournalOperations(Protocol):
    def identity(self, handle: int) -> tuple[int, int]: ...

    def mark_delete(self, handle: int) -> None: ...

    def delete_pending(self, handle: int) -> bool: ...

    def close(self, handle: int) -> None: ...

    def live_identity(
        self, parent: int, name: str, *, directory: bool
    ) -> tuple[int, int] | None: ...


@dataclass
class JournalEntry:
    handle: int | None
    parent: int
    name: str
    directory: bool
    identity: tuple[int, int] | None


class WindowsConstructionJournal:
    def __init__(self, operations: JournalOperations, base_handle: int) -> None:
        self.operations = operations
        self.base_handle = base_handle
        self.creator_pid = os.getpid()
        self.entries: list[JournalEntry] = []
        self.state = "constructing"

    def record(
        self, handle: int, parent: int, name: str, *, directory: bool
    ) -> None:
        self._require_owner()
        entry = JournalEntry(handle, parent, name, directory, None)
        self.entries.append(entry)
        entry.identity = self.operations.identity(handle)

    def complete(self) -> None:
        self._require_owner()
        self.entries.clear()
        self.base_handle = -1
        self.state = "complete"

    def _require_owner(self) -> None:
        if self.creator_pid != os.getpid():
            raise JournalRollbackError("construction journal belongs to another process")

    def rollback(self) -> None:
        self._require_owner()
        try:
            while self.entries:
                entry = self.entries[-1]
                if entry.handle is not None:
                    if entry.identity is not None and (
                        self.operations.identity(entry.handle) != entry.identity
                    ):
                        raise JournalRollbackError("rollback handle identity changed")
                    self.operations.mark_delete(entry.handle)
                    if not self.operations.delete_pending(entry.handle):
                        raise JournalRollbackError("rollback disposition was not pending")
                    self.operations.close(entry.handle)
                    entry.handle = None
                live = self.operations.live_identity(
                    entry.parent, entry.name, directory=entry.directory
                )
                if live is not None:
                    raise JournalRollbackError("rollback removal is not proven")
                self.entries.pop()
            self.operations.close(self.base_handle)
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
        for entry in reversed(self.entries):
            if entry.handle is not None:
                try:
                    self.operations.close(entry.handle)
                except OSError:
                    pass
                entry.handle = None
        if self.base_handle >= 0:
            try:
                self.operations.close(self.base_handle)
            except OSError:
                pass
            self.base_handle = -1
