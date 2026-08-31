"""Race-safe persistent authority for one sealed plan's replication set."""

import json
import os
import stat
from dataclasses import dataclass
from pathlib import Path

try:
    from .batch_receipt_storage import (
        child_directory,
        open_private_directory,
        private_directory,
        read_bounded,
    )
    from .contracts import canonical_json_bytes
except ImportError:
    from batch_receipt_storage import (
        child_directory,
        open_private_directory,
        private_directory,
        read_bounded,
    )
    from contracts import canonical_json_bytes

try:
    import fcntl
except ImportError:  # pragma: no cover - the public Windows gate runs first
    fcntl = None


class PlanAuthorityError(ValueError):
    pass


@dataclass(frozen=True)
class PlanReservation:
    path: Path
    payload: bytes
    root_identity: tuple[int, int]


def reserve_plan(
    private_root: Path,
    plan_sha256: str,
    replication_count: int,
    pair_ids: tuple[str, ...],
) -> PlanReservation:
    if fcntl is None:
        raise PlanAuthorityError("plan authority requires the supported POSIX backend")
    root = Path(private_root)
    private_directory(root, create=True)
    authority = child_directory(root, "plan-authority", exclusive=False)
    root_fd = -1
    authority_fd = -1
    lock_fd = -1
    lock_name = f"{plan_sha256}.lock"
    try:
        root_fd = open_private_directory(root)
        authority_fd = open_private_directory(authority)
        root_state = os.fstat(root_fd)
        lock_fd = os.open(
            lock_name,
            os.O_RDWR | os.O_CREAT | getattr(os, "O_NOFOLLOW", 0),
            0o600,
            dir_fd=authority_fd,
        )
        state = os.fstat(lock_fd)
        if not stat.S_ISREG(state.st_mode) or stat.S_IMODE(state.st_mode) != 0o600:
            raise PlanAuthorityError("plan authority lock is unsafe")
        state_name = f"{plan_sha256}.json"
        state_path = authority / state_name
        payload = (
            canonical_json_bytes(
                {
                    "pairIds": list(pair_ids),
                    "planSha256": plan_sha256,
                    "replicationCount": replication_count,
                }
            )
            + b"\n"
        )
        fcntl.flock(lock_fd, fcntl.LOCK_EX)
        try:
            existing_state = os.stat(
                state_name, dir_fd=authority_fd, follow_symlinks=False
            )
        except FileNotFoundError:
            existing_state = None
        if existing_state is not None:
            existing = json.loads(read_bounded(state_path, 1024 * 1024))
            if not stat.S_ISREG(existing_state.st_mode) or type(existing) is not dict:
                raise PlanAuthorityError("sealed replication authority is invalid")
            raise PlanAuthorityError(
                "sealed replication set is already reserved or complete"
            )
        state_fd = os.open(
            state_name,
            os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0),
            0o600,
            dir_fd=authority_fd,
        )
        try:
            view = memoryview(payload)
            while view:
                written = os.write(state_fd, view)
                if written <= 0:
                    raise PlanAuthorityError("plan reservation write failed")
                view = view[written:]
            os.fsync(state_fd)
        finally:
            os.close(state_fd)
    except (OSError, ValueError) as error:
        if isinstance(error, PlanAuthorityError):
            raise
        raise PlanAuthorityError("cannot reserve sealed replication set") from error
    finally:
        if lock_fd >= 0:
            fcntl.flock(lock_fd, fcntl.LOCK_UN)
            os.close(lock_fd)
        if authority_fd >= 0:
            os.close(authority_fd)
        if root_fd >= 0:
            os.close(root_fd)
    return PlanReservation(state_path, payload, (root_state.st_dev, root_state.st_ino))


def release_unstarted(reservation: PlanReservation) -> None:
    """Rollback a reservation only when its controller made zero executor calls."""
    try:
        root = reservation.path.parent.parent
        root_fd = open_private_directory(root)
        state = os.fstat(root_fd)
        if (state.st_dev, state.st_ino) != reservation.root_identity:
            raise PlanAuthorityError("private authority root was replaced")
        authority_fd = open_private_directory(reservation.path.parent)
        if read_bounded(reservation.path, 1024 * 1024) != reservation.payload:
            raise PlanAuthorityError("plan reservation changed before rollback")
        os.unlink(reservation.path.name, dir_fd=authority_fd)
    except OSError as error:
        raise PlanAuthorityError("cannot roll back unstarted reservation") from error
    finally:
        if "authority_fd" in locals():
            os.close(authority_fd)
        if "root_fd" in locals():
            os.close(root_fd)
