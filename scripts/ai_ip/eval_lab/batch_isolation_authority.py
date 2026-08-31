"""Process-private authority for globally unique candidate attempt capabilities."""

import hashlib
import hmac
import os
import secrets
import stat
import tempfile
from dataclasses import dataclass
from pathlib import Path

if os.name == "nt":
    try:
        from .batch_isolation_windows import SecureFilesystemError
        from .batch_isolation_windows import _windows_path_has_private_acl
        from .batch_isolation_windows import close_handle, create_private_directory
        from .batch_isolation_windows import create_private_file_handle
        from .batch_isolation_windows import (
            handle_identity,
            open_directory,
            read_handle,
        )
    except ImportError:
        from batch_isolation_windows import SecureFilesystemError
        from batch_isolation_windows import _windows_path_has_private_acl
        from batch_isolation_windows import close_handle, create_private_directory
        from batch_isolation_windows import create_private_file_handle
        from batch_isolation_windows import handle_identity, open_directory, read_handle
else:
    try:
        from .batch_isolation_posix import close_handle, handle_identity, open_directory
    except ImportError:
        from batch_isolation_posix import close_handle, handle_identity, open_directory


class AuthorityError(ValueError):
    pass


_PROCESS_KEY = secrets.token_bytes(32)
_REGISTRY_FD: int | None = None
_REGISTRY_PATH: Path | None = None


@dataclass
class Reservation:
    fd: int
    identity: tuple[int, int]
    payload: bytes

    def validate(self) -> None:
        if os.name == "nt":
            if (
                handle_identity(self.fd) != self.identity
                or read_handle(self.fd, 64) != self.payload
            ):
                raise AuthorityError("attempt reservation identity or content changed")
            return
        metadata = os.fstat(self.fd)
        actual = metadata.st_dev, metadata.st_ino
        if (
            actual != self.identity
            or not stat.S_ISREG(metadata.st_mode)
            or metadata.st_nlink != 1
        ):
            raise AuthorityError("attempt reservation identity changed")
        os.lseek(self.fd, 0, os.SEEK_SET)
        if os.read(self.fd, len(self.payload) + 1) != self.payload:
            raise AuthorityError("attempt reservation content changed")


def creator_signature(binding: bytes) -> bytes:
    return hmac.digest(_PROCESS_KEY, binding, "sha256")


def _registry_fd() -> int:
    global _REGISTRY_FD, _REGISTRY_PATH
    if _REGISTRY_FD is not None:
        return _REGISTRY_FD
    owner = getattr(
        os,
        "getuid",
        lambda: hashlib.sha256(
            os.environ.get("USERNAME", "unknown-user").encode()
        ).hexdigest()[:16],
    )()
    authority = (
        Path(tempfile.gettempdir()).resolve(strict=True)
        / f"codex-isolated-cells-{owner}"
    )
    if os.name == "nt":
        if authority.exists():
            if not _windows_path_has_private_acl(authority):
                raise AuthorityError(
                    "secure Windows authority requires a protected DACL"
                )
            authority_fd = open_directory(authority)
        else:
            authority_fd = create_private_directory(authority)
        _REGISTRY_PATH = authority
        _REGISTRY_FD = authority_fd
        return authority_fd
    try:
        authority.mkdir(mode=0o700)
    except FileExistsError:
        pass
    authority_fd = open_directory(authority)
    metadata = os.fstat(authority_fd)
    if metadata.st_uid != os.getuid():
        close_handle(authority_fd)
        raise AuthorityError("attempt authority must be owned by the current user")
    os.fchmod(authority_fd, 0o700)
    _REGISTRY_FD = authority_fd
    return authority_fd


def reserve_attempt(pair_id: str, attempt_id: str, nonce: bytes) -> Reservation:
    digest = hashlib.sha256(attempt_id.encode()).hexdigest()
    payload = creator_signature(
        b"reservation\0" + pair_id.encode() + b"\0" + attempt_id.encode() + nonce
    )
    if os.name == "nt":
        _registry_fd()
        assert _REGISTRY_PATH is not None
        path = _REGISTRY_PATH / digest
        try:
            fd = create_private_file_handle(path, payload)
        except SecureFilesystemError as error:
            if path.exists():
                raise AuthorityError("attempt ID was already used") from error
            raise AuthorityError("cannot create secure Windows reservation") from error
        reservation = Reservation(fd, handle_identity(fd), payload)
        reservation.validate()
        return reservation
    flags = (
        os.O_RDWR
        | os.O_CREAT
        | os.O_EXCL
        | getattr(os, "O_NOFOLLOW", 0)
        | getattr(os, "O_CLOEXEC", 0)
    )
    try:
        fd = os.open(digest, flags, 0o600, dir_fd=_registry_fd())
    except FileExistsError as error:
        raise AuthorityError("attempt ID was already used") from error
    try:
        os.fchmod(fd, 0o600)
        os.write(fd, payload)
        os.fsync(fd)
        metadata = os.fstat(fd)
        reservation = Reservation(fd, (metadata.st_dev, metadata.st_ino), payload)
        reservation.validate()
        return reservation
    except BaseException:
        os.close(fd)
        raise
