"""Remeasure immutable execution identity at the cell boundary."""

import hashlib
import os
import stat
from pathlib import Path

try:
    from .batch_controller_snapshots import FrozenTree
    from .batch_controller_types import AttemptRequest
except ImportError:
    from batch_controller_snapshots import FrozenTree
    from batch_controller_types import AttemptRequest


class ExecutionIdentityError(ValueError):
    pass


def _hash_regular(path: Path) -> str:
    target = Path(path)
    descriptor = -1
    try:
        before = target.lstat()
        if not stat.S_ISREG(before.st_mode) or stat.S_ISLNK(before.st_mode):
            raise ExecutionIdentityError("execution identity file is not regular")
        descriptor = os.open(target, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
        opened = os.fstat(descriptor)
        if (opened.st_dev, opened.st_ino) != (before.st_dev, before.st_ino):
            raise ExecutionIdentityError("execution identity file was replaced")
        digest = hashlib.sha256()
        while True:
            chunk = os.read(descriptor, 64 * 1024)
            if not chunk:
                break
            digest.update(chunk)
        after = os.fstat(descriptor)
        if (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns) != (
            opened.st_dev,
            opened.st_ino,
            opened.st_size,
            opened.st_mtime_ns,
        ):
            raise ExecutionIdentityError("execution identity file changed")
        return digest.hexdigest()
    except OSError as error:
        raise ExecutionIdentityError("execution identity file is unavailable") from error
    finally:
        if descriptor >= 0:
            os.close(descriptor)


def _verify_tree(root: Path, expected: FrozenTree, *, allow_runtime: bool) -> str:
    expected_files = {relative: payload for relative, _, payload in expected.files}
    found: set[str] = set()
    try:
        for path in Path(root).rglob("*"):
            relative = path.relative_to(root).as_posix()
            if allow_runtime and (
                relative == ".runtime" or relative.startswith(".runtime/")
            ):
                continue
            state = path.lstat()
            if stat.S_ISLNK(state.st_mode):
                raise ExecutionIdentityError("materialized cell tree contains a link")
            if path.is_dir():
                continue
            if relative not in expected_files or not stat.S_ISREG(state.st_mode):
                raise ExecutionIdentityError("materialized cell tree contains extra data")
            if path.read_bytes() != expected_files[relative]:
                raise ExecutionIdentityError("materialized cell tree bytes changed")
            if stat.S_IMODE(state.st_mode) != 0o600:
                raise ExecutionIdentityError("materialized cell file mode changed")
            found.add(relative)
    except OSError as error:
        raise ExecutionIdentityError("materialized cell tree is unavailable") from error
    if found != set(expected_files):
        raise ExecutionIdentityError("materialized cell tree is incomplete")
    return expected.digest if allow_runtime else expected.cell_digest()


def measure_request_identity(request: AttemptRequest) -> dict[str, str]:
    measured = {
        "binary_sha256": _hash_regular(request.binary_path),
        "codex_home_seed_sha256": _verify_tree(
            request.cell.home, request.codex_home_seed_snapshot, allow_runtime=True
        ),
        "effective_config_sha256": hashlib.sha256(request.effective_config).hexdigest(),
        "workspace_seed_sha256": _verify_tree(
            request.cell.workspace, request.workspace_seed_snapshot, allow_runtime=False
        ),
        "execution_profile_sha256": hashlib.sha256(
            request.execution_profile_json
        ).hexdigest(),
        "model_route_sha256": hashlib.sha256(request.model_route_json).hexdigest(),
        "app_server_protocol_schema_sha256": hashlib.sha256(
            request.app_server_protocol_schema
        ).hexdigest(),
        "promptfoo_config_sha256": hashlib.sha256(request.promptfoo_config).hexdigest(),
    }
    expected = {
        "binary_sha256": request.binary_sha256,
        "codex_home_seed_sha256": request.codex_home_seed_sha256,
        "effective_config_sha256": request.effective_config_sha256,
        "workspace_seed_sha256": request.workspace_seed_snapshot.cell_digest(),
        "execution_profile_sha256": request.execution_profile_sha256,
        "model_route_sha256": request.model_route_sha256,
        "app_server_protocol_schema_sha256": request.app_server_protocol_schema_sha256,
        "promptfoo_config_sha256": request.promptfoo_config_sha256,
    }
    if measured != expected:
        raise ExecutionIdentityError("actual cell execution identity differs from seal")
    return measured
