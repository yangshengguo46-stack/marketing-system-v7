"""Race-safe persistent authority for one sealed plan's replication set."""

import fcntl
import os
import stat
from dataclasses import dataclass
from pathlib import Path

try:
    from .contracts import canonical_json_bytes, load_exact_json
except ImportError:
    from contracts import canonical_json_bytes, load_exact_json


class PlanAuthorityError(ValueError):
    pass


@dataclass(frozen=True)
class PlanReservation:
    path: Path
    payload: bytes
    root_identity: tuple[int, int]


def _secure_directory(path: Path, *, create: bool = False) -> os.stat_result:
    target = Path(path)
    if not target.is_absolute():
        raise PlanAuthorityError("private root must be absolute")
    try:
        if create:
            target.mkdir(parents=True, mode=0o700, exist_ok=True)
        state = target.lstat()
    except OSError as error:
        raise PlanAuthorityError("private authority directory is unavailable") from error
    if not stat.S_ISDIR(state.st_mode) or stat.S_ISLNK(state.st_mode):
        raise PlanAuthorityError("private authority directory must not be a link")
    if stat.S_IMODE(state.st_mode) != 0o700 or state.st_uid != os.getuid():
        raise PlanAuthorityError("private authority directory ownership or mode is unsafe")
    return state


def reserve_plan(
    private_root: Path,
    plan_sha256: str,
    replication_count: int,
    pair_ids: tuple[str, ...],
) -> PlanReservation:
    root = Path(private_root)
    root_state = _secure_directory(root, create=True)
    authority = root / "plan-authority"
    _secure_directory(authority, create=True)
    lock_path = authority / f"{plan_sha256}.lock"
    try:
        lock_fd = os.open(
            lock_path,
            os.O_RDWR | os.O_CREAT | getattr(os, "O_NOFOLLOW", 0),
            0o600,
        )
    except OSError as error:
        raise PlanAuthorityError("plan authority lock is unavailable") from error
    state_path = authority / f"{plan_sha256}.json"
    payload = canonical_json_bytes(
        {
            "pairIds": list(pair_ids),
            "planSha256": plan_sha256,
            "replicationCount": replication_count,
        }
    ) + b"\n"
    try:
        fcntl.flock(lock_fd, fcntl.LOCK_EX)
        if state_path.exists():
            existing = load_exact_json(state_path)
            if type(existing) is not dict:
                raise PlanAuthorityError("sealed replication authority is invalid")
            raise PlanAuthorityError("sealed replication set is already reserved or complete")
        state_fd = os.open(
            state_path,
            os.O_WRONLY
            | os.O_CREAT
            | os.O_EXCL
            | getattr(os, "O_NOFOLLOW", 0),
            0o600,
        )
        try:
            os.write(state_fd, payload)
            os.fsync(state_fd)
        finally:
            os.close(state_fd)
    except (OSError, ValueError) as error:
        if isinstance(error, PlanAuthorityError):
            raise
        raise PlanAuthorityError("cannot reserve sealed replication set") from error
    finally:
        fcntl.flock(lock_fd, fcntl.LOCK_UN)
        os.close(lock_fd)
    return PlanReservation(
        state_path, payload, (root_state.st_dev, root_state.st_ino)
    )


def release_unstarted(reservation: PlanReservation) -> None:
    """Rollback a reservation only when its controller made zero executor calls."""
    try:
        root = reservation.path.parent.parent
        state = _secure_directory(root)
        if (state.st_dev, state.st_ino) != reservation.root_identity:
            raise PlanAuthorityError("private authority root was replaced")
        if reservation.path.read_bytes() != reservation.payload:
            raise PlanAuthorityError("plan reservation changed before rollback")
        reservation.path.unlink()
    except OSError as error:
        raise PlanAuthorityError("cannot roll back unstarted reservation") from error
