"""Bounded terminal identity proof for an unlinked POSIX cell root."""

import os
import time


class TerminalProofError(OSError):
    pass


def identity(metadata: os.stat_result) -> tuple[int, int]:
    return metadata.st_dev, metadata.st_ino


def handle_identity(fd: int) -> tuple[int, int]:
    return identity(os.fstat(fd))


def require_parent(
    root_fd: int, base_identity: tuple[int, int], directory_flags: int
) -> None:
    parent_fd = os.open("..", directory_flags, dir_fd=root_fd)
    try:
        if handle_identity(parent_fd) != base_identity:
            raise TerminalProofError("cleanup root parent identity changed")
    finally:
        os.close(parent_fd)


def prove_unlinked(
    base_fd: int,
    root_fd: int,
    base_identity: tuple[int, int],
    root_identity: tuple[int, int],
    directory_flags: int,
    max_entries: int,
    max_seconds: float,
) -> None:
    require_parent(root_fd, base_identity, directory_flags)
    entries_seen = 0
    started = time.monotonic()
    with os.scandir(base_fd) as entries:
        for entry in entries:
            entries_seen += 1
            if (
                entries_seen > max_entries
                or time.monotonic() - started > max_seconds
            ):
                raise TerminalProofError(
                    "terminal cleanup proof reached its bounded progress limit"
                )
            metadata = os.stat(
                entry.name, dir_fd=base_fd, follow_symlinks=False
            )
            if identity(metadata) == root_identity:
                raise TerminalProofError("cleanup did not remove the original root")
