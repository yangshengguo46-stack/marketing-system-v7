"""Immutable byte snapshots used between controller preflight and execution."""

import hashlib
import json
import os
import shutil
import stat
from dataclasses import dataclass
from pathlib import Path

try:
    from .batch_plan import BatchPlanError, snapshot_tree
    from .contracts import canonical_json_bytes
except ImportError:
    from batch_plan import BatchPlanError, snapshot_tree
    from contracts import canonical_json_bytes


class SnapshotError(ValueError):
    pass


@dataclass(frozen=True)
class FrozenJson:
    payload: bytes
    sha256: str

    @classmethod
    def capture(cls, value: object) -> "FrozenJson":
        payload = canonical_json_bytes(value)
        return cls(payload, hashlib.sha256(payload).hexdigest())

    def thaw(self) -> object:
        return json.loads(self.payload)


@dataclass(frozen=True)
class FrozenFile:
    payload: bytes
    sha256: str

    @classmethod
    def capture(cls, path: Path, *, limit: int = 64 * 1024 * 1024) -> "FrozenFile":
        target = Path(path)
        try:
            before = target.lstat()
            if not stat.S_ISREG(before.st_mode) or stat.S_ISLNK(before.st_mode):
                raise SnapshotError("sealed file must be a regular non-link file")
            if before.st_size > limit:
                raise SnapshotError("sealed file exceeds snapshot bound")
            descriptor = os.open(target, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
            try:
                if os.fstat(descriptor) != before:
                    raise SnapshotError("sealed file changed while opening")
                payload = os.read(descriptor, before.st_size + 1)
                if len(payload) != before.st_size or os.fstat(descriptor) != before:
                    raise SnapshotError("sealed file changed while reading")
            finally:
                os.close(descriptor)
        except OSError as error:
            raise SnapshotError("sealed file is unavailable") from error
        return cls(payload, hashlib.sha256(payload).hexdigest())


@dataclass(frozen=True)
class FrozenTree:
    digest: str
    files: tuple[tuple[str, int, bytes], ...]

    @classmethod
    def capture(cls, path: Path) -> "FrozenTree":
        root = Path(path)
        try:
            first = snapshot_tree(root)
            records = tuple(
                (
                    relative,
                    stat.S_IMODE((root / relative).lstat().st_mode),
                    first.files[relative][1],
                )
                for relative in sorted(first.files)
            )
            second = snapshot_tree(root)
        except (BatchPlanError, OSError) as error:
            raise SnapshotError("sealed tree cannot be snapshotted") from error
        if first != second:
            raise SnapshotError("sealed tree changed during snapshot")
        return cls(first.digest, records)

    def materialize(self, destination: Path) -> Path:
        target = Path(destination)
        try:
            target.mkdir(mode=0o700)
            for relative, mode, payload in self.files:
                output = target / relative
                output.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
                descriptor = os.open(
                    output,
                    os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0),
                    mode,
                )
                try:
                    os.write(descriptor, payload)
                    os.fchmod(descriptor, mode)
                    os.fsync(descriptor)
                finally:
                    os.close(descriptor)
        except BaseException:
            shutil.rmtree(target, ignore_errors=True)
            raise
        try:
            if snapshot_tree(target).digest != self.digest:
                raise SnapshotError("materialized tree identity differs from snapshot")
        except BaseException:
            shutil.rmtree(target, ignore_errors=True)
            raise
        return target

    def cell_digest(self) -> str:
        entries = [
            {
                "mode": 0o600,
                "path": relative,
                "sha256": hashlib.sha256(payload).hexdigest(),
                "size": len(payload),
            }
            for relative, _, payload in self.files
        ]
        return FrozenJson.capture({"entries": entries}).sha256


def materialize_file(destination: Path, snapshot: FrozenFile, mode: int) -> Path:
    target = Path(destination)
    target.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    descriptor = os.open(
        target,
        os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0),
        mode,
    )
    try:
        os.write(descriptor, snapshot.payload)
        os.fchmod(descriptor, mode)
        os.fsync(descriptor)
    finally:
        os.close(descriptor)
    return target
