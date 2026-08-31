"""Bounded streaming traversal for the Windows isolated-cell backend."""

import time

try:
    from . import batch_isolation_win32 as win32
    from .batch_isolation_windows_cleanup import WindowsCleanupJournal
except ImportError:
    import batch_isolation_win32 as win32
    from batch_isolation_windows_cleanup import WindowsCleanupJournal


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
    journal = getattr(filesystem, "cleanup_journal", None)
    if journal is None:
        journal = WindowsCleanupJournal()
        filesystem.cleanup_journal = journal
    try:
        journal.retry(filesystem)
        if filesystem.base_fd < 0:
            return
        if filesystem.root_fd < 0:
            journal.prove_root_absent(filesystem)
            journal.close(filesystem, filesystem.base_fd, slot="base")
            return
        if not filesystem.cleanup_started:
            filesystem.validate(cleanup=True)
            filesystem.cleanup_started = True
        if filesystem.marker_handle is not None:
            journal.dispose(
                filesystem, filesystem.marker_handle, slot="marker"
            )
        _delete_contents(filesystem, journal)
        for name, handle in tuple(filesystem.directories.items()):
            journal.close(filesystem, handle, slot=f"directory:{name}")
        journal.dispose(filesystem, filesystem.root_fd, slot="root")
        journal.prove_root_absent(filesystem)
        journal.close(filesystem, filesystem.base_fd, slot="base")
    except (OSError, win32.Win32SecurityError) as error:
        if isinstance(error, WindowsTreeError):
            raise
        raise WindowsTreeError(f"Windows cleanup is uncertain: {error}") from error


def _delete_contents(filesystem: object, journal: WindowsCleanupJournal) -> None:
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
                journal.dispose(filesystem, handle)
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
            retained_name = "/".join(relative)
            retained = filesystem.directories.get(retained_name)
            disposed_current = bool(relative) and retained is None
            try:
                if relative:
                    journal.dispose(
                        filesystem,
                        retained if retained is not None else current,
                        slot=(
                            f"directory:{retained_name}"
                            if retained is not None
                            else None
                        ),
                    )
            finally:
                if not disposed_current:
                    journal.close(filesystem, current)
    except BaseException as original_error:
        close_error: OSError | None = None
        for _, handle in stack:
            if not journal.has_handle(handle):
                try:
                    journal.close(filesystem, handle)
                except (OSError, win32.Win32SecurityError) as error:
                    close_error = close_error or error
        if close_error is not None:
            raise WindowsTreeError(
                "Windows traversal-handle close is pending"
            ) from close_error
        raise original_error
