"""Secure per-cell materialization of arm-neutral executable identities."""

import hashlib
import os
import stat
from pathlib import Path


class ArtifactMaterializationError(ValueError):
    pass


def materialize_neutral_binary(
    cell_home: Path, source: Path, expected_sha256: str
) -> Path:
    """Copy verified bytes to the same non-arm-specific path inside every cell."""
    runtime = Path(cell_home) / ".runtime"
    try:
        runtime.mkdir(mode=0o700)
        runtime_state = runtime.lstat()
        if not stat.S_ISDIR(runtime_state.st_mode) or stat.S_ISLNK(
            runtime_state.st_mode
        ):
            raise ArtifactMaterializationError("neutral runtime directory is invalid")
        source_fd = os.open(source, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
        destination = runtime / "codex"
        destination_fd = os.open(
            destination,
            os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0),
            0o700,
        )
    except (OSError, ArtifactMaterializationError) as error:
        raise ArtifactMaterializationError(
            "cannot materialize neutral candidate binary"
        ) from error
    digest = hashlib.sha256()
    try:
        source_state = os.fstat(source_fd)
        if not stat.S_ISREG(source_state.st_mode):
            raise ArtifactMaterializationError("candidate binary is not a regular file")
        while True:
            chunk = os.read(source_fd, 1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
            view = memoryview(chunk)
            while view:
                written = os.write(destination_fd, view)
                if written <= 0:
                    raise ArtifactMaterializationError("candidate binary copy failed")
                view = view[written:]
        os.fsync(destination_fd)
        if os.fstat(source_fd) != source_state:
            raise ArtifactMaterializationError("candidate binary changed while copied")
    except OSError as error:
        raise ArtifactMaterializationError("candidate binary copy failed") from error
    finally:
        os.close(source_fd)
        os.close(destination_fd)
    if digest.hexdigest() != expected_sha256:
        raise ArtifactMaterializationError("candidate binary commitment mismatch")
    return destination
