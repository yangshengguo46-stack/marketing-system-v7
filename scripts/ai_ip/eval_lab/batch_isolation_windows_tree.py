"""Bounded streaming traversal for the Windows isolated-cell backend."""

import time

try:
    from . import batch_isolation_win32 as win32
except ImportError:
    import batch_isolation_win32 as win32


MAX_ENTRIES = 100_000
MAX_DEPTH = 2_048
MAX_BYTES = 1024 * 1024 * 1024
MAX_SECONDS = 10.0


class WindowsTreeError(OSError):
    pass


def _check_bound(entries: int, total_bytes: int, started: float, action: str) -> None:
    if (
        entries > MAX_ENTRIES
        or total_bytes > MAX_BYTES
        or time.monotonic() - started > MAX_SECONDS
    ):
        raise WindowsTreeError(
            f"{action} reached its bounded progress limit; retry is required"
        )


def scan_regular_identities(filesystem: object) -> frozenset[tuple[int, int]]:
    pending: list[tuple[str, ...]] = [()]
    found: set[tuple[int, int]] = set()
    entries = 0
    total_bytes = 0
    started = time.monotonic()
    while pending:
        relative = pending.pop()
        if len(relative) > MAX_DEPTH:
            raise WindowsTreeError("Windows traversal exceeds depth bound")
        directory = filesystem._open_relative(relative)
        try:
            for entry in win32.iter_directory(directory):
                entries += 1
                total_bytes += entry.size
                _check_bound(entries, total_bytes, started, "Windows traversal")
                if entry.is_reparse:
                    raise WindowsTreeError("cell contains a reparse point")
                if entry.is_directory:
                    pending.append((*relative, entry.name))
                else:
                    handle = win32.open_child(directory, entry.name, directory=False)
                    try:
                        if win32.information(handle).nNumberOfLinks != 1:
                            raise WindowsTreeError("cell contains a hard link")
                        found.add(win32.identity(handle))
                    finally:
                        win32.close(handle)
        finally:
            win32.close(directory)
    return frozenset(found)


def delete_tree(filesystem: object) -> None:
    filesystem.validate(cleanup=True)
    filesystem.cleanup_started = True
    if filesystem.marker_handle is not None:
        win32.mark_delete(filesystem.marker_handle)
        win32.close(filesystem.marker_handle)
        filesystem.marker_handle = None
    entries = 0
    total_bytes = 0
    started = time.monotonic()
    directory = filesystem._open_relative((), deletable=True)
    stack: list[tuple[tuple[str, ...], int]] = [((), directory)]
    try:
        while stack:
            relative, current = stack[-1]
            child_directory: tuple[str, ...] | None = None
            for entry in win32.iter_directory(current):
                entries += 1
                total_bytes += entry.size
                _check_bound(entries, total_bytes, started, "cleanup")
                if entry.is_directory and not entry.is_reparse:
                    child_directory = (*relative, entry.name)
                    break
                handle = win32.open_path(
                    win32.child_path(current, entry.name),
                    directory=entry.is_directory,
                    allow_reparse=True,
                    deletable=True,
                )
                try:
                    win32.mark_delete(handle)
                finally:
                    win32.close(handle)
            if child_directory is not None:
                if len(child_directory) > MAX_DEPTH:
                    raise WindowsTreeError("cleanup exceeds Windows depth bound")
                stack.append(
                    (
                        child_directory,
                        filesystem._open_relative(child_directory, deletable=True),
                    )
                )
                continue
            stack.pop()
            if relative:
                retained_name = "/".join(relative)
                retained = filesystem.directories.get(retained_name)
                win32.mark_delete(retained if retained is not None else current)
                if retained is not None:
                    win32.close(retained)
                    del filesystem.directories[retained_name]
            win32.close(current)
    except BaseException:
        for _, handle in stack:
            try:
                win32.close(handle)
            except win32.Win32SecurityError:
                pass
        raise
    for handle in filesystem.directories.values():
        try:
            win32.close(handle)
        except win32.Win32SecurityError:
            pass
    filesystem.directories.clear()
    win32.mark_delete(filesystem.root_fd)
    win32.close(filesystem.root_fd)
    win32.close(filesystem.base_fd)
    filesystem.root_fd = -1
    filesystem.base_fd = -1
