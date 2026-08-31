"""PID-bound retention for construction failures with unproven cleanup."""

import os
from dataclasses import dataclass
from typing import Protocol


class OrphanCleanupError(OSError):
    pass


class CleanupResource(Protocol):
    def close(self) -> None: ...


class ReservationResource(Protocol):
    def close(self) -> None: ...


@dataclass
class RetainedOrphan:
    resource: CleanupResource
    reservation: ReservationResource
    creator_pid: int


_ORPHANS: list[RetainedOrphan] = []


def retain_orphan(resource: CleanupResource, reservation: ReservationResource) -> None:
    _ORPHANS.append(RetainedOrphan(resource, reservation, os.getpid()))


def retry_orphans() -> None:
    failures = 0
    for orphan in tuple(_ORPHANS):
        if orphan.creator_pid != os.getpid():
            failures += 1
            continue
        try:
            retry = getattr(orphan.resource, "retry_cleanup", None)
            if retry is None:
                retry = getattr(orphan.resource, "delete_exact")
            retry()
        except OSError:
            failures += 1
            continue
        orphan.resource.close()
        orphan.reservation.close()
        _ORPHANS.remove(orphan)
    if failures:
        raise OrphanCleanupError("isolated cell orphan cleanup is still pending")


def close_orphans_after_fork() -> None:
    for orphan in _ORPHANS:
        try:
            orphan.resource.close()
        finally:
            orphan.reservation.close()
    _ORPHANS.clear()
