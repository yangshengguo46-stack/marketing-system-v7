"""Retry journal for uncertain Windows cleanup operations."""

import os
from dataclasses import dataclass

try:
    from . import batch_isolation_win32 as win32
except ImportError:
    import batch_isolation_win32 as win32


class WindowsCleanupError(OSError):
    pass


@dataclass
class PendingHandle:
    handle: int
    identity: tuple[int, int]
    delete: bool
    slot: str | None


class WindowsCleanupJournal:
    def __init__(self) -> None:
        self.creator_pid = os.getpid()
        self.pending: list[PendingHandle] = []

    def _require_owner(self) -> None:
        if self.creator_pid != os.getpid():
            raise WindowsCleanupError(
                "Windows cleanup journal belongs to another process"
            )

    def has_handle(self, handle: int) -> bool:
        return any(item.handle == handle for item in self.pending)

    def _clear_slot(self, filesystem: object, item: PendingHandle) -> None:
        if item.slot == "marker" and filesystem.marker_handle == item.handle:
            filesystem.marker_handle = None
        elif item.slot == "root" and filesystem.root_fd == item.handle:
            filesystem.root_fd = -1
        elif item.slot == "base" and filesystem.base_fd == item.handle:
            filesystem.base_fd = -1
        elif item.slot and item.slot.startswith("directory:"):
            name = item.slot.partition(":")[2]
            if filesystem.directories.get(name) == item.handle:
                del filesystem.directories[name]

    def _record(self, handle: int, *, delete: bool, slot: str | None) -> PendingHandle:
        self._require_owner()
        item = PendingHandle(handle, win32.identity(handle), delete, slot)
        self.pending.append(item)
        return item

    def _finish(self, filesystem: object, item: PendingHandle) -> None:
        if item.delete:
            win32.mark_delete(item.handle)
            if not win32.delete_pending(item.handle):
                raise WindowsCleanupError("Windows deletion disposition is uncertain")
        win32.close(item.handle)
        self._clear_slot(filesystem, item)
        self.pending.remove(item)

    def dispose(
        self, filesystem: object, handle: int, *, slot: str | None = None
    ) -> None:
        item = self._record(handle, delete=True, slot=slot)
        self._finish(filesystem, item)

    def close(
        self, filesystem: object, handle: int, *, slot: str | None = None
    ) -> None:
        item = self._record(handle, delete=False, slot=slot)
        self._finish(filesystem, item)

    def _recover_root(self, filesystem: object, item: PendingHandle) -> bool:
        try:
            handle = win32.open_child(
                filesystem.base_fd,
                filesystem.root_name,
                directory=True,
                deletable=True,
            )
        except win32.Win32SecurityError as error:
            code = getattr(error, "winerror", None) or getattr(error, "errno", None)
            if code in {2, 3}:
                self._clear_slot(filesystem, item)
                self.pending.remove(item)
                return True
            raise
        live_identity = win32.identity(handle)
        if live_identity != filesystem.root_identity:
            self.close(filesystem, handle)
            raise WindowsCleanupError("Windows cleanup root identity was substituted")
        item.handle = handle
        item.identity = live_identity
        filesystem.root_fd = handle
        return False

    def retry(self, filesystem: object) -> None:
        self._require_owner()
        for item in tuple(self.pending):
            try:
                actual = win32.identity(item.handle)
            except win32.Win32SecurityError as error:
                code = getattr(error, "winerror", None) or getattr(error, "errno", None)
                if code != 6:
                    raise WindowsCleanupError(
                        "Windows cleanup handle identity is uncertain"
                    ) from error
                actual = None
            if actual != item.identity:
                if item.slot == "root" and not self._recover_root(filesystem, item):
                    pass
                else:
                    self._clear_slot(filesystem, item)
                    if item in self.pending:
                        self.pending.remove(item)
                    continue
            self._finish(filesystem, item)

    def prove_root_absent(self, filesystem: object) -> None:
        self._require_owner()
        try:
            handle = win32.open_child(
                filesystem.base_fd,
                filesystem.root_name,
                directory=True,
                deletable=True,
            )
        except win32.Win32SecurityError as error:
            code = getattr(error, "winerror", None) or getattr(error, "errno", None)
            if code in {2, 3}:
                return
            raise WindowsCleanupError("Windows root absence is unproven") from error
        actual = win32.identity(handle)
        if actual != filesystem.root_identity:
            self.close(filesystem, handle)
            raise WindowsCleanupError("Windows cleanup root identity was substituted")
        filesystem.root_fd = handle
        self.dispose(filesystem, handle, slot="root")
        try:
            proof = win32.open_child(
                filesystem.base_fd,
                filesystem.root_name,
                directory=True,
                deletable=True,
            )
        except win32.Win32SecurityError as error:
            code = getattr(error, "winerror", None) or getattr(error, "errno", None)
            if code in {2, 3}:
                return
            raise WindowsCleanupError("Windows root absence is unproven") from error
        actual = win32.identity(proof)
        self.close(filesystem, proof)
        if actual != filesystem.root_identity:
            raise WindowsCleanupError("Windows cleanup root identity was substituted")
        raise WindowsCleanupError("Windows root removal is not yet proven")
