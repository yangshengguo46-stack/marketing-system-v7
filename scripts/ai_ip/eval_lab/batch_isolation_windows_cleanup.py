"""Retry journal for uncertain Windows cleanup operations."""

import os
from dataclasses import dataclass

try:
    from . import batch_isolation_win32 as win32
    from .batch_isolation_windows_handles import HandleOwnershipError
    from .batch_isolation_windows_handles import bind_identity
    from .batch_isolation_windows_handles import close_owned
    from .batch_isolation_windows_handles import primary_is_owned
except ImportError:
    import batch_isolation_win32 as win32
    from batch_isolation_windows_handles import HandleOwnershipError
    from batch_isolation_windows_handles import bind_identity
    from batch_isolation_windows_handles import close_owned
    from batch_isolation_windows_handles import primary_is_owned


class WindowsCleanupError(OSError):
    pass


class _HandleOperations:
    @staticmethod
    def duplicate(handle: int) -> int:
        return getattr(win32, "duplicate_raw", win32.duplicate)(handle)

    @staticmethod
    def identity(handle: int) -> tuple[int, int]:
        return win32.identity(handle)

    @staticmethod
    def close(handle: int) -> None:
        getattr(win32, "close_raw", win32.close)(handle)


_HANDLE_OPERATIONS = _HandleOperations()


@dataclass
class PendingHandle:
    handle: int | None
    identity: tuple[int, int] | None
    delete: bool
    slot: str | None
    expected_identity: tuple[int, int] | None = None
    proof_handle: int | None = None
    state: str = "identity_unknown"
    proof_close_uncertain: bool = False
    handle_close_uncertain: bool = False


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
        return any(
            item.handle == handle or item.proof_handle == handle for item in self.pending
        )

    def _clear_slot(
        self, filesystem: object, item: PendingHandle, handle: int | None = None
    ) -> None:
        owned_handle = item.handle if handle is None else handle
        if item.slot == "marker" and filesystem.marker_handle == owned_handle:
            filesystem.marker_handle = None
        elif item.slot == "root" and filesystem.root_fd == owned_handle:
            filesystem.root_fd = -1
        elif item.slot == "base" and filesystem.base_fd == owned_handle:
            filesystem.base_fd = -1
        elif item.slot and item.slot.startswith("directory:"):
            name = item.slot.partition(":")[2]
            if filesystem.directories.get(name) == owned_handle:
                del filesystem.directories[name]

    def _bind(self, item: PendingHandle) -> tuple[int, int]:
        try:
            return bind_identity(
                item,
                _HANDLE_OPERATIONS,
                self.creator_pid,
                expected_identity=item.expected_identity,
            )
        except (OSError, win32.Win32SecurityError) as error:
            if isinstance(error, HandleOwnershipError) and "substituted" in str(error):
                item.delete = False
                try:
                    close_owned(item, _HANDLE_OPERATIONS, self.creator_pid)
                finally:
                    if item.handle is None and item.proof_handle is None:
                        if item in self.pending:
                            self.pending.remove(item)
                raise WindowsCleanupError(
                    "Windows cleanup acquired a substituted handle"
                ) from error
            raise WindowsCleanupError(
                "Windows cleanup handle identity is uncertain"
            ) from error

    def _record(
        self,
        handle: int,
        *,
        delete: bool,
        slot: str | None,
        expected_identity: tuple[int, int] | None = None,
    ) -> PendingHandle:
        self._require_owner()
        item = PendingHandle(handle, None, delete, slot, expected_identity)
        self.pending.append(item)
        handle_ledger = getattr(win32, "handle_ledger", None)
        if handle_ledger is not None:
            transferred_identity = handle_ledger().transfer(handle)
            if item.expected_identity is None:
                item.expected_identity = transferred_identity
        self._bind(item)
        return item

    def _finish(self, filesystem: object, item: PendingHandle) -> None:
        self._bind(item)
        owned_handle = item.handle
        if not primary_is_owned(item, _HANDLE_OPERATIONS, self.creator_pid):
            self._clear_slot(filesystem, item, owned_handle)
            self.pending.remove(item)
            return
        assert item.handle is not None
        if item.delete:
            win32.mark_delete(item.handle)
            if not win32.delete_pending(item.handle):
                raise WindowsCleanupError("Windows deletion disposition is uncertain")
        close_owned(item, _HANDLE_OPERATIONS, self.creator_pid)
        self._clear_slot(filesystem, item, owned_handle)
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
            open_child = getattr(win32, "open_child_raw", win32.open_child)
            item.handle = open_child(
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
        item.identity = None
        item.expected_identity = filesystem.root_identity
        item.proof_handle = None
        item.state = "identity_unknown"
        item.proof_close_uncertain = False
        item.handle_close_uncertain = False
        live_identity = self._bind(item)
        if live_identity != filesystem.root_identity:
            raise WindowsCleanupError("Windows cleanup root identity was substituted")
        assert item.handle is not None
        filesystem.root_fd = item.handle
        return False

    def retry(self, filesystem: object) -> None:
        self._require_owner()
        for item in tuple(self.pending):
            self._bind(item)
            owned_handle = item.handle
            if not primary_is_owned(item, _HANDLE_OPERATIONS, self.creator_pid):
                if item.slot == "root" and not self._recover_root(filesystem, item):
                    pass
                else:
                    self._clear_slot(filesystem, item, owned_handle)
                    if item in self.pending:
                        self.pending.remove(item)
                    continue
            self._finish(filesystem, item)

    def prove_root_absent(self, filesystem: object) -> None:
        self._require_owner()
        try:
            open_child = getattr(win32, "open_child_raw", win32.open_child)
            handle = open_child(
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
        item = self._record(
            handle,
            delete=True,
            slot="root",
            expected_identity=filesystem.root_identity,
        )
        actual = item.identity
        if actual != filesystem.root_identity:
            raise WindowsCleanupError("Windows cleanup root identity was substituted")
        filesystem.root_fd = handle
        self._finish(filesystem, item)
        try:
            proof = open_child(
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
        proof_item = self._record(
            proof,
            delete=False,
            slot=None,
            expected_identity=filesystem.root_identity,
        )
        actual = proof_item.identity
        self._finish(filesystem, proof_item)
        if actual != filesystem.root_identity:
            raise WindowsCleanupError("Windows cleanup root identity was substituted")
        raise WindowsCleanupError("Windows root removal is not yet proven")
