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
    from .private_fs import PrivateRoot
except ImportError:
    from batch_receipt_storage import (
        child_directory,
        open_private_directory,
        private_directory,
        read_bounded,
    )
    from contracts import canonical_json_bytes
    from private_fs import PrivateRoot

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
    try:
        authenticated = (
            PrivateRoot.open_existing(root)
            if root.exists()
            else PrivateRoot.create_new(root)
        )
    except ValueError as error:
        raise PlanAuthorityError(
            "private authority root is not authenticated"
        ) from error
    authority = child_directory(root, "plan-authority", exclusive=False)
    root_fd = -1
    authority_fd = -1
    lock_fd = -1
    lock_name = f"{plan_sha256}.lock"
    try:
        root_fd = open_private_directory(root)
        authority_fd = authenticated._open_root()
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
        state_path = root / state_name
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
    """Reservations are intentionally irreversible once any lifecycle state exists."""
    raise PlanAuthorityError("sealed replication reservations cannot be released")
