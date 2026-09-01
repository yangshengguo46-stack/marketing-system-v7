"""Canonical values shared by Promptfoo runtime attestation stages."""

import hashlib
import struct
from dataclasses import dataclass


_MAX_DEPTH = 128
_MAX_FILE_BYTES = 384 * 1024 * 1024
_MAX_PATH_BYTES = 4096
_RECORD_FIELDS = {"mode", "path", "sha256", "size"}


class PromptfooFilesystemError(ValueError):
    pass


@dataclass(frozen=True, slots=True)
class ArchiveBuild:
    chunks: tuple[bytes, ...]
    tree_sha256: str
    inventory_sha256: str
    file_count: int
    unpacked_bytes: int


def _canonical_path(value: object) -> bytes:
    if type(value) is not str:
        raise PromptfooFilesystemError("Promptfoo inventory path is invalid")
    parts = value.split("/")
    if not parts or len(parts) > _MAX_DEPTH:
        raise PromptfooFilesystemError("Promptfoo inventory path is invalid")
    for part in parts:
        if (
            not part
            or part in {".", ".."}
            or "\\" in part
            or any(ord(character) < 0x20 or ord(character) == 0x7F for character in part)
        ):
            raise PromptfooFilesystemError("Promptfoo inventory path is invalid")
    try:
        encoded = value.encode("utf-8", errors="strict")
    except UnicodeEncodeError as error:
        raise PromptfooFilesystemError("Promptfoo inventory path is invalid") from error
    if len(encoded) > _MAX_PATH_BYTES:
        raise PromptfooFilesystemError("Promptfoo inventory path is invalid")
    return encoded


def inventory_sha256(records: object) -> str:
    """Commit one canonical ordered view of locked regular-file records."""
    if type(records) not in {list, tuple}:
        raise PromptfooFilesystemError("Promptfoo inventory is invalid")
    digest = hashlib.sha256(b"ai-ip-promptfoo-inventory/v1\0")
    previous: bytes | None = None
    for item in records:
        if type(item) is not dict or set(item) != _RECORD_FIELDS:
            raise PromptfooFilesystemError("Promptfoo inventory record is invalid")
        path = _canonical_path(item["path"])
        mode, file_digest, size = item["mode"], item["sha256"], item["size"]
        if (
            previous is not None
            and path <= previous
            or type(mode) is not int
            or mode not in {0o400, 0o500}
            or type(size) is not int
            or not 0 <= size <= _MAX_FILE_BYTES
            or type(file_digest) is not str
            or len(file_digest) != 64
            or any(character not in "0123456789abcdef" for character in file_digest)
        ):
            raise PromptfooFilesystemError("Promptfoo inventory record is invalid")
        digest.update(b"file\0")
        values = path, str(mode).encode("ascii"), bytes.fromhex(file_digest), str(size).encode("ascii")
        for value in values:
            digest.update(struct.pack(">I", len(value)))
            digest.update(value)
        previous = path
    digest.update(b"end\0")
    return digest.hexdigest()
