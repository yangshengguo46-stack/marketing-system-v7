"""Descriptor-retained offline view of one sealed pair layout."""

import json
import os
from pathlib import Path

try:
    from .batch_receipt_storage import (
        MAX_CONTEXT_BYTES,
        MAX_PRIVATE_FILE_BYTES,
        BoundDirectory,
        SecureStorageError,
        bind_child,
        bind_directory,
        read_entry,
    )
    from .contracts import canonical_json_bytes, sha256_json
    from .private_fs import PrivateRoot, _prepare_root_path
except ImportError:
    from batch_receipt_storage import (
        MAX_CONTEXT_BYTES,
        MAX_PRIVATE_FILE_BYTES,
        BoundDirectory,
        SecureStorageError,
        bind_child,
        bind_directory,
        read_entry,
    )
    from contracts import canonical_json_bytes, sha256_json
    from private_fs import PrivateRoot, _prepare_root_path


def _load(directory: BoundDirectory, name: str, limit: int) -> object:
    payload = read_entry(directory, name, limit)
    try:
        value = json.loads(payload)
    except (json.JSONDecodeError, UnicodeDecodeError) as error:
        raise SecureStorageError("private evidence is invalid") from error
    if canonical_json_bytes(value) + b"\n" != payload:
        raise SecureStorageError("private evidence bytes are not canonical")
    return value


class OfflineEvidence:
    """Open the authenticated root once and retain every descendant identity."""

    def __init__(self, private_root: Path, pair_id: str) -> None:
        root, repo_root, worktrees = _prepare_root_path(Path(private_root))
        authenticated = PrivateRoot(root, repo_root, worktrees)
        descriptor = authenticated._open_root()
        state = os.fstat(descriptor)
        self.root = BoundDirectory(root, descriptor, (state.st_dev, state.st_ino))
        self._entries: list[BoundDirectory] = [self.root]
        try:
            layout = _load(self.root, f"layout-{pair_id}.json", MAX_CONTEXT_BYTES)
            authority = _load(
                self.root, f"layout-authority-{pair_id}.json", MAX_CONTEXT_BYTES
            )
            if (
                type(layout) is not dict
                or type(authority) is not dict
                or authority.get("layout") != layout
                or authority.get("layoutSha256") != sha256_json(layout)
            ):
                raise SecureStorageError("sealed layout authority commitment mismatch")
            records = {
                entry["path"]: (entry["device"], entry["inode"])
                for entry in layout.get("entries", [])
                if type(entry) is dict
            }
            if records.get(".") != self.root.identity:
                raise SecureStorageError("private root layout identity differs")
            self.pairs = self._child(self.root, "pairs", records["pairs"])
            self.attempts = self._child(self.root, "attempts", records["attempts"])
            self.pair = self._child(self.pairs, pair_id, records[f"pairs/{pair_id}"])
            self._records = records
            self._attempt_caps: dict[str, BoundDirectory] = {}
            self.layout_authority = authority
        except BaseException:
            self.close()
            raise

    def _child(
        self, parent: BoundDirectory, name: str, identity: tuple[int, int]
    ) -> BoundDirectory:
        child = bind_child(parent, name, identity)
        self._entries.append(child)
        return child

    def attempt(self, attempt_id: str) -> BoundDirectory:
        if attempt_id not in self._attempt_caps:
            identity = self._records.get(f"attempts/{attempt_id}")
            if type(identity) is not tuple:
                raise SecureStorageError("attempt is absent from sealed layout")
            self._attempt_caps[attempt_id] = self._child(
                self.attempts, attempt_id, identity
            )
        return self._attempt_caps[attempt_id]

    def load_pair(self, name: str, limit: int = MAX_PRIVATE_FILE_BYTES) -> object:
        return _load(self.pair, name, limit)

    def load_root(self, name: str, limit: int = MAX_PRIVATE_FILE_BYTES) -> object:
        return _load(self.root, name, limit)

    def load_attempt(
        self, attempt_id: str, name: str, limit: int = MAX_PRIVATE_FILE_BYTES
    ) -> object:
        return _load(self.attempt(attempt_id), name, limit)

    def read_pair(self, name: str, limit: int) -> bytes:
        return read_entry(self.pair, name, limit)

    def has_pair(self, name: str) -> bool:
        self.pair.verify()
        try:
            os.stat(name, dir_fd=self.pair.descriptor, follow_symlinks=False)
        except FileNotFoundError:
            return False
        self.pair.verify()
        return True

    def read_attempt(self, attempt_id: str, name: str, limit: int) -> bytes:
        return read_entry(self.attempt(attempt_id), name, limit)

    def verify(self) -> None:
        for entry in self._entries:
            entry.verify()

    def close(self) -> None:
        for entry in reversed(self._entries):
            entry.close()
