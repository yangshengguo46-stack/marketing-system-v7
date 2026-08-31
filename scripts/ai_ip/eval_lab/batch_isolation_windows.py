"""Capability-bound Windows filesystem backend for isolated candidate cells."""

import os
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

try:
    from . import batch_isolation_win32 as win32
    from .batch_isolation_windows_tree import WindowsTreeError
    from .batch_isolation_windows_tree import delete_tree, scan_regular_identities
except ImportError:
    import batch_isolation_win32 as win32
    from batch_isolation_windows_tree import WindowsTreeError
    from batch_isolation_windows_tree import delete_tree, scan_regular_identities


class SecureFilesystemError(OSError):
    pass


_LAYOUT = ("home", "workspace", "cache", "temp", "logs", "promptfoo")


def _translate(action: str, operation):
    try:
        return operation()
    except win32.Win32SecurityError as error:
        raise SecureFilesystemError(action) from error


def handle_identity(handle: int) -> tuple[int, int]:
    return _translate("cannot query Windows identity", lambda: win32.identity(handle))


def close_handle(handle: int) -> None:
    _translate("cannot close Windows capability", lambda: win32.close(handle))


def open_directory(path: Path, *, deletable: bool = True) -> int:
    candidate = Path(path)
    if not candidate.is_absolute():
        raise SecureFilesystemError("absolute Windows directory path required")
    current = _translate(
        "cannot open Windows path anchor",
        lambda: win32.open_path(
            Path(candidate.anchor), directory=True, deletable=deletable
        ),
    )
    try:
        for part in candidate.parts[1:]:
            child = _translate(
                "cannot securely open Windows directory",
                lambda part=part: win32.open_child(
                    current, part, directory=True, deletable=deletable
                ),
            )
            close_handle(current)
            current = child
        result = current
        current = -1
        return result
    finally:
        if current >= 0:
            close_handle(current)


def ancestor_identities(handle: int) -> frozenset[tuple[int, int]]:
    path = _translate(
        "cannot resolve Windows ancestry", lambda: win32.final_path(handle)
    )
    found: set[tuple[int, int]] = set()
    for depth, candidate in enumerate((path, *path.parents)):
        if depth > 2_048:
            raise SecureFilesystemError("Windows ancestry exceeds safety bound")
        parent = open_directory(candidate, deletable=False)
        try:
            found.add(handle_identity(parent))
        finally:
            close_handle(parent)
    return frozenset(found)


def _windows_path_has_private_acl(path: Path) -> bool:
    try:
        handle = (
            open_directory(path, deletable=False)
            if Path(path).is_dir()
            else _translate(
                "cannot open protected Windows file",
                lambda: win32.open_path(Path(path), directory=False, deletable=False),
            )
        )
        try:
            win32.require_private_acl(handle)
            return True
        finally:
            close_handle(handle)
    except (OSError, SecureFilesystemError, win32.Win32SecurityError):
        return False


def create_private_directory(path: Path) -> int:
    parent = open_directory(Path(path).parent, deletable=False)
    try:
        return _translate(
            "cannot create protected Windows directory",
            lambda: win32.create_directory(parent, Path(path).name),
        )
    finally:
        close_handle(parent)


def create_private_file_handle(path: Path, payload: bytes) -> int:
    parent = open_directory(Path(path).parent, deletable=False)
    try:
        return _translate(
            "cannot create protected Windows file",
            lambda: win32.create_file(parent, Path(path).name, payload),
        )
    finally:
        close_handle(parent)


def read_handle(handle: int, limit: int) -> bytes:
    return _translate(
        "cannot read protected Windows file", lambda: win32.read_file(handle, limit)
    )


def _require_live_child(
    parent: int, name: str, retained: int, expected: tuple[int, int]
) -> None:
    if handle_identity(retained) != expected:
        raise SecureFilesystemError(f"required layout {name} capability changed")
    _translate(
        "required layout is not private", lambda: win32.require_private_acl(retained)
    )
    live = _translate(
        "required layout is unavailable",
        lambda: win32.open_child(parent, name, directory=True, deletable=False),
    )
    try:
        if handle_identity(live) != expected:
            raise SecureFilesystemError(f"required layout {name} identity changed")
    finally:
        close_handle(live)


@dataclass
class WindowsCellFilesystem:
    base_path: Path
    root_name: str
    base_fd: int
    root_fd: int
    base_identity: tuple[int, int]
    root_identity: tuple[int, int]
    directories: dict[str, int]
    directory_identities: dict[str, tuple[int, int]]
    creator_pid: int
    marker_handle: int | None = None
    cleanup_started: bool = False

    @classmethod
    def create(
        cls, base: Path, root_name: str, base_fd: int
    ) -> "WindowsCellFilesystem":
        root_fd = -1
        directories: dict[str, int] = {}
        try:
            root_fd = _translate(
                "cannot create protected Windows root",
                lambda: win32.create_directory(base_fd, root_name),
            )
            for name in _LAYOUT:
                directories[name] = _translate(
                    f"cannot create required layout {name}",
                    lambda name=name: win32.create_directory(root_fd, name),
                )
            directories["cache/promptfoo"] = _translate(
                "cannot create required layout cache/promptfoo",
                lambda: win32.create_directory(directories["cache"], "promptfoo"),
            )
            return cls(
                Path(base),
                root_name,
                base_fd,
                root_fd,
                handle_identity(base_fd),
                handle_identity(root_fd),
                directories,
                {name: handle_identity(fd) for name, fd in directories.items()},
                os.getpid(),
            )
        except BaseException:
            for handle in reversed(tuple(directories.values())):
                try:
                    win32.mark_delete(handle)
                except win32.Win32SecurityError:
                    pass
                try:
                    win32.close(handle)
                except win32.Win32SecurityError:
                    pass
            if root_fd >= 0:
                try:
                    win32.mark_delete(root_fd)
                except win32.Win32SecurityError:
                    pass
                try:
                    win32.close(root_fd)
                except win32.Win32SecurityError:
                    pass
            try:
                win32.close(base_fd)
            except win32.Win32SecurityError:
                pass
            raise

    def _require_owner(self) -> None:
        if self.creator_pid != os.getpid():
            raise SecureFilesystemError("Windows cell belongs to another process")

    def validate(self, *, cleanup: bool = False) -> None:
        self._require_owner()
        if (
            handle_identity(self.base_fd) != self.base_identity
            or handle_identity(self.root_fd) != self.root_identity
        ):
            raise SecureFilesystemError("attempt root capability changed")
        _translate(
            "attempt root is not private",
            lambda: win32.require_private_acl(self.root_fd),
        )
        live_root = _translate(
            "attempt root is unavailable",
            lambda: win32.open_child(
                self.base_fd, self.root_name, directory=True, deletable=False
            ),
        )
        try:
            if handle_identity(live_root) != self.root_identity:
                raise SecureFilesystemError("attempt root identity was substituted")
        finally:
            close_handle(live_root)
        if cleanup and self.cleanup_started:
            return
        for name in _LAYOUT:
            _require_live_child(
                self.root_fd,
                name,
                self.directories[name],
                self.directory_identities[name],
            )
        _require_live_child(
            self.directories["cache"],
            "promptfoo",
            self.directories["cache/promptfoo"],
            self.directory_identities["cache/promptfoo"],
        )

    def base_is_still_bound(self) -> bool:
        self._require_owner()
        try:
            current = open_directory(self.base_path, deletable=False)
        except SecureFilesystemError:
            return False
        try:
            return handle_identity(current) == self.base_identity
        finally:
            close_handle(current)

    def _open_relative(self, parts: tuple[str, ...]) -> int:
        retained_name = "/".join(parts)
        retained = self.directories.get(retained_name)
        current = _translate(
            "cannot duplicate retained directory capability",
            lambda: win32.duplicate(retained if retained is not None else self.root_fd),
        )
        try:
            for part in () if retained is not None else parts:
                child = _translate(
                    "cannot open cell descendant",
                    lambda part=part: win32.open_child(current, part, directory=True),
                )
                close_handle(current)
                current = child
            result = current
            current = -1
            return result
        finally:
            if current >= 0:
                close_handle(current)

    def write_snapshot(self, files: Iterable[tuple[str, bytes]], target: str) -> None:
        self._require_owner()
        created: dict[tuple[str, ...], int] = {(): self.directories[target]}
        owned: list[int] = []
        try:
            for relative_text, payload in sorted(files):
                parts = Path(relative_text).parts
                if not parts or any(part in {"", ".", ".."} for part in parts):
                    raise SecureFilesystemError("snapshot contains unsafe path")
                parent: tuple[str, ...] = ()
                for part in parts[:-1]:
                    child = (*parent, part)
                    if child not in created:
                        try:
                            handle = win32.create_directory(created[parent], part)
                        except win32.Win32SecurityError:
                            handle = win32.open_child(
                                created[parent], part, directory=True
                            )
                        created[child] = handle
                        owned.append(handle)
                    parent = child
                handle = _translate(
                    "cannot copy independent snapshot bytes",
                    lambda: win32.create_file(created[parent], parts[-1], payload),
                )
                close_handle(handle)
        finally:
            for handle in reversed(owned):
                close_handle(handle)

    def create_file(self, name: str, payload: bytes) -> None:
        self._require_owner()
        if self.marker_handle is not None:
            raise FileExistsError(name)
        try:
            self.marker_handle = win32.create_file(self.root_fd, name, payload)
        except win32.Win32SecurityError as error:
            if win32.child_path(self.root_fd, name).exists():
                raise FileExistsError(name) from error
            raise SecureFilesystemError("cannot seal Windows receipt") from error

    def read_file(self, name: str, limit: int) -> bytes:
        self._require_owner()
        if self.marker_handle is None:
            raise FileNotFoundError(name)
        return read_handle(self.marker_handle, limit)

    def scan_regular_identities(self) -> frozenset[tuple[int, int]]:
        self._require_owner()
        try:
            return scan_regular_identities(self)
        except (WindowsTreeError, win32.Win32SecurityError) as error:
            raise SecureFilesystemError(str(error)) from error

    def delete_exact(self) -> None:
        try:
            delete_tree(self)
        except (WindowsTreeError, win32.Win32SecurityError) as error:
            raise SecureFilesystemError(str(error)) from error

    def close(self) -> None:
        handles = [*self.directories.values(), self.root_fd, self.base_fd]
        if self.marker_handle is not None:
            handles.append(self.marker_handle)
        self.directories.clear()
        self.marker_handle = None
        self.root_fd = -1
        self.base_fd = -1
        for handle in handles:
            if handle not in (-1, None):
                try:
                    win32.close(handle)
                except win32.Win32SecurityError:
                    pass
