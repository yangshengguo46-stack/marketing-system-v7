"""Bounded resumable identity proof for an unlinked POSIX cell root."""

import os
import time
from dataclasses import dataclass
from typing import Any


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


def _stable_version(fd: int) -> tuple[int, int, int, int, int, int]:
    metadata = os.fstat(fd)
    return (
        metadata.st_dev,
        metadata.st_ino,
        metadata.st_nlink,
        metadata.st_size,
        metadata.st_mtime_ns,
        metadata.st_ctime_ns,
    )


@dataclass
class TerminalProof:
    base_fd: int
    base_identity: tuple[int, int]
    root_identity: tuple[int, int]
    directory_flags: int
    scan_fd: int
    entries: Any
    stable_version: tuple[int, int, int, int, int, int]
    creator_pid: int

    @classmethod
    def start(
        cls,
        filesystem: object,
        directory_flags: int,
    ) -> "TerminalProof":
        base_fd = filesystem.base_fd
        root_fd = filesystem.root_fd
        base_identity = filesystem.base_identity
        root_identity = filesystem.root_identity
        require_parent(root_fd, base_identity, directory_flags)
        scan_fd = os.dup(base_fd)
        try:
            return cls(
                base_fd,
                base_identity,
                root_identity,
                directory_flags,
                scan_fd,
                os.scandir(scan_fd),
                _stable_version(base_fd),
                os.getpid(),
            )
        except BaseException:
            os.close(scan_fd)
            raise

    def _require_owner(self) -> None:
        if self.creator_pid != os.getpid():
            raise TerminalProofError("terminal proof belongs to another process")

    def _restart_after_change(self) -> None:
        self.close()
        raise TerminalProofError(
            "terminal cleanup base changed; stable proof must restart"
        )

    def advance(self, root_fd: int, max_entries: int, max_seconds: float) -> bool:
        self._require_owner()
        require_parent(root_fd, self.base_identity, self.directory_flags)
        if _stable_version(self.base_fd) != self.stable_version:
            self._restart_after_change()
        entries_seen = 0
        started = time.monotonic()
        while entries_seen < max_entries and time.monotonic() - started <= max_seconds:
            try:
                entry = next(self.entries)
            except StopIteration:
                require_parent(root_fd, self.base_identity, self.directory_flags)
                if _stable_version(self.base_fd) != self.stable_version:
                    self._restart_after_change()
                self.close()
                return True
            entries_seen += 1
            metadata = os.stat(
                entry.name, dir_fd=self.base_fd, follow_symlinks=False
            )
            if identity(metadata) == self.root_identity:
                self.close()
                raise TerminalProofError("cleanup did not remove the original root")
        if _stable_version(self.base_fd) != self.stable_version:
            self._restart_after_change()
        raise TerminalProofError(
            "terminal cleanup proof reached its bounded progress limit"
        )

    def close(self) -> None:
        entries, scan_fd = self.entries, self.scan_fd
        self.entries = iter(())
        self.scan_fd = -1
        try:
            entries.close()
        finally:
            if scan_fd >= 0:
                os.close(scan_fd)
