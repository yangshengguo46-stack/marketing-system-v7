"""Auditable ownership transitions for newly acquired Windows handles."""

import os
from dataclasses import dataclass
from typing import Protocol


class HandleOwnershipError(OSError):
    pass


class HandleOperations(Protocol):
    def duplicate(self, handle: int) -> int: ...

    def identity(self, handle: int) -> tuple[int, int]: ...

    def close(self, handle: int) -> None: ...


@dataclass
class HandleOwnership:
    handle: int | None
    identity: tuple[int, int] | None = None
    proof_handle: int | None = None
    state: str = "identity_unknown"
    proof_close_uncertain: bool = False
    handle_close_uncertain: bool = False
    close_requested: bool = False


def _error_code(error: OSError) -> int | None:
    return getattr(error, "winerror", None) or getattr(error, "errno", None)


def _require_owner(creator_pid: int) -> None:
    if creator_pid != os.getpid():
        raise HandleOwnershipError("Windows handle ownership belongs to another process")


def _close_proof(
    ownership: HandleOwnership, operations: HandleOperations, creator_pid: int
) -> None:
    _require_owner(creator_pid)
    proof = ownership.proof_handle
    if proof is None:
        return
    if ownership.identity is None:
        raise HandleOwnershipError("Windows proof handle identity is unknown")
    if ownership.proof_close_uncertain:
        try:
            actual = operations.identity(proof)
        except OSError as error:
            if _error_code(error) != 6:
                raise HandleOwnershipError(
                    "Windows proof-handle identity is uncertain"
                ) from error
            actual = None
        if actual != ownership.identity:
            ownership.proof_handle = None
            ownership.proof_close_uncertain = False
            ownership.state = "identity_bound"
            return
    try:
        operations.close(proof)
    except OSError as error:
        ownership.proof_close_uncertain = True
        ownership.state = "close_uncertain"
        raise HandleOwnershipError("Windows proof-handle close is uncertain") from error
    ownership.proof_handle = None
    ownership.proof_close_uncertain = False
    ownership.state = "identity_bound"


def bind_identity(
    ownership: HandleOwnership,
    operations: HandleOperations,
    creator_pid: int,
    *,
    expected_identity: tuple[int, int] | None = None,
) -> tuple[int, int]:
    """Bind through a duplicate so the raw value is never the sole identity proof."""
    _require_owner(creator_pid)
    if ownership.handle is None:
        raise HandleOwnershipError("Windows handle ownership was already released")
    if ownership.identity is None:
        if ownership.proof_handle is None:
            ownership.proof_handle = operations.duplicate(ownership.handle)
        ownership.identity = operations.identity(ownership.proof_handle)
        ownership.state = "identity_bound"
    _close_proof(ownership, operations, creator_pid)
    if expected_identity is not None and ownership.identity != expected_identity:
        raise HandleOwnershipError("Windows acquired handle identity was substituted")
    return ownership.identity


def primary_is_owned(
    ownership: HandleOwnership, operations: HandleOperations, creator_pid: int
) -> bool:
    """Validate a bound raw value before any close or mutation on a retry path."""
    _require_owner(creator_pid)
    if ownership.handle is None:
        return False
    if ownership.identity is None:
        raise HandleOwnershipError("Windows handle identity is not bound")
    try:
        actual = operations.identity(ownership.handle)
    except OSError as error:
        if _error_code(error) != 6:
            raise HandleOwnershipError("Windows handle identity is uncertain") from error
        actual = None
    if actual != ownership.identity:
        ownership.handle = None
        ownership.handle_close_uncertain = False
        ownership.state = "closed"
        return False
    return True


def close_owned(
    ownership: HandleOwnership, operations: HandleOperations, creator_pid: int
) -> None:
    """Close a bound handle, retaining uncertainty without reusing raw values."""
    _require_owner(creator_pid)
    ownership.close_requested = True
    if ownership.identity is None:
        bind_identity(ownership, operations, creator_pid)
    else:
        _close_proof(ownership, operations, creator_pid)
    if not primary_is_owned(ownership, operations, creator_pid):
        return
    assert ownership.handle is not None
    try:
        operations.close(ownership.handle)
    except OSError as error:
        ownership.handle_close_uncertain = True
        ownership.state = "close_uncertain"
        raise HandleOwnershipError("Windows handle close is uncertain") from error
    ownership.handle = None
    ownership.handle_close_uncertain = False
    ownership.state = "closed"
