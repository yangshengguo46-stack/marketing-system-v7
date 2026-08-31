import hashlib
import hmac
import os
import re
import secrets
import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from types import MappingProxyType
from typing import Mapping

try:
    from .batch_isolation_authority import AuthorityError, Reservation
    from .batch_isolation_authority import creator_signature, reserve_attempt
    from .batch_contracts import BatchContractError, validate_named_contract
    from .batch_plan import BatchPlanError, TreeSnapshot, snapshot_tree
except ImportError:
    from batch_isolation_authority import AuthorityError, Reservation
    from batch_isolation_authority import creator_signature, reserve_attempt
    from batch_contracts import BatchContractError, validate_named_contract
    from batch_plan import BatchPlanError, TreeSnapshot, snapshot_tree

if os.name == "nt":
    try:
        from .batch_isolation_windows import WindowsCellFilesystem as CellFilesystem
        from .batch_isolation_windows import (
            SecureFilesystemError,
            _windows_path_has_private_acl,
        )
        from .batch_isolation_windows import (
            ancestor_identities,
            close_handle,
            handle_identity,
            open_directory,
        )
    except ImportError:
        from batch_isolation_windows import WindowsCellFilesystem as CellFilesystem
        from batch_isolation_windows import (
            SecureFilesystemError,
            _windows_path_has_private_acl,
        )
        from batch_isolation_windows import (
            ancestor_identities,
            close_handle,
            handle_identity,
            open_directory,
        )
else:
    try:
        from .batch_isolation_posix import PosixCellFilesystem as CellFilesystem
        from .batch_isolation_posix import (
            SecureFilesystemError,
            ancestor_identities,
            close_handle,
            handle_identity,
            open_directory,
        )
    except ImportError:
        from batch_isolation_posix import PosixCellFilesystem as CellFilesystem
        from batch_isolation_posix import (
            SecureFilesystemError,
            ancestor_identities,
            close_handle,
            handle_identity,
            open_directory,
        )


_REPO_ROOT = Path(__file__).resolve().parents[3]
_IDENTIFIER = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}\Z")
_LOCALE_NAME = re.compile(r"(?:LANG(?:UAGE)?|LC_[A-Z0-9_]+)\Z")
_PROFILE_VALUE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}\Z")
_PROFILE_NAMES = {"AWS_PROFILE", "VOLCENGINE_PROFILE"}
_DIRECTORY_FIELDS = ("home", "workspace", "cache", "temp", "logs", "promptfoo")
_RECEIPT_NAME = ".receipts-sealed"


class IsolationError(ValueError):
    pass


@dataclass(frozen=True)
class AttemptCell:
    pair_id: str
    attempt_id: str
    root: Path
    home: Path
    workspace: Path
    cache: Path
    temp: Path
    logs: Path
    promptfoo: Path
    environment: Mapping[str, str]
    receipt_marker: Path
    _capability: object = field(repr=False, compare=False)


@dataclass
class _CellState:
    cell: AttemptCell
    filesystem: CellFilesystem
    reservation: Reservation
    environment: Mapping[str, str]
    nonce: bytes
    lifecycle: str = "active"
    receipt: bytes | None = None


_CELLS: dict[object, _CellState] = {}


def _validate_identifier(name: str, value: object) -> str:
    if type(value) is not str or not _IDENTIFIER.fullmatch(value):
        raise IsolationError(f"{name} must be an opaque identifier")
    return value


def _validate_profile(profile: object) -> dict[str, object]:
    if type(profile) is not dict:
        raise IsolationError("execution profile must be an object")
    try:
        validate_named_contract("execution-profile", profile)
    except BatchContractError as error:
        raise IsolationError(f"execution profile is invalid: {error}") from error
    names = profile["environmentAllowlist"]
    assert isinstance(names, list)
    for name in names:
        if type(name) is not str or not (
            name in _PROFILE_NAMES or _LOCALE_NAME.fullmatch(name)
        ):
            raise IsolationError(
                f"{name!s} is not an approved non-secret profile environment name; "
                "secret or proxy inheritance is forbidden"
            )
    return profile


def _git_worktrees() -> tuple[Path, ...]:
    try:
        result = subprocess.run(
            ["git", "-C", str(_REPO_ROOT), "worktree", "list", "--porcelain", "-z"],
            check=False,
            capture_output=True,
            timeout=10,
        )
    except (OSError, subprocess.SubprocessError) as error:
        raise IsolationError("cannot enumerate Git worktrees") from error
    if result.returncode != 0:
        raise IsolationError("cannot enumerate Git worktrees")
    worktrees = tuple(
        Path(os.fsdecode(record[9:]))
        for record in result.stdout.split(b"\0")
        if record.startswith(b"worktree ")
    )
    if not worktrees:
        raise IsolationError("Git worktree listing is empty")
    return worktrees


def _worktree_identities() -> frozenset[tuple[int, int]]:
    found: set[tuple[int, int]] = set()
    for worktree in _git_worktrees():
        try:
            fd = open_directory(worktree)
        except SecureFilesystemError:
            continue
        try:
            found.add(handle_identity(fd))
        finally:
            close_handle(fd)
    repo_fd = open_directory(_REPO_ROOT)
    try:
        if handle_identity(repo_fd) not in found:
            raise IsolationError("Git worktree listing omitted the current repository")
    finally:
        close_handle(repo_fd)
    return frozenset(found)


def _validate_base(base_root: Path) -> tuple[Path, int]:
    base = Path(base_root)
    if not base.is_absolute():
        raise IsolationError("attempt base must be absolute")
    fd = -1
    try:
        fd = open_directory(base)
        if ancestor_identities(fd) & _worktree_identities():
            raise IsolationError("attempt root must be outside every Git worktree")
        return base, fd
    except IsolationError:
        if fd >= 0:
            close_handle(fd)
        raise
    except (OSError, SecureFilesystemError) as error:
        if fd >= 0:
            close_handle(fd)
        raise IsolationError(
            "attempt base must be a non-symlink or non-reparse directory"
        ) from error


def _windows_environment_overrides(
    home: Path, cache: Path, temp: Path
) -> dict[str, str]:
    home_text = str(home)
    return {
        "USERPROFILE": home_text,
        "HOMEDRIVE": home.drive,
        "HOMEPATH": home_text[len(home.drive) :],
        "APPDATA": str(home / "AppData" / "Roaming"),
        "LOCALAPPDATA": str(cache),
        "TEMP": str(temp),
        "TMP": str(temp),
    }


def _environment(
    paths: dict[str, Path], profile: dict[str, object], source: Mapping[str, str]
) -> Mapping[str, str]:
    if not isinstance(source, Mapping):
        raise IsolationError("source environment must be a mapping")
    declared = profile["environmentAllowlist"]
    assert isinstance(declared, list)
    names = {"PATH", "SHELL", "LANG", "LANGUAGE", *declared}
    names.update(
        name for name in source if type(name) is str and name.startswith("LC_")
    )
    environment: dict[str, str] = {}
    for name in names:
        if name not in source:
            continue
        value = source[name]
        if type(value) is not str or "\0" in value:
            raise IsolationError(f"invalid environment value: {name}")
        if name in _PROFILE_NAMES and not _PROFILE_VALUE.fullmatch(value):
            raise IsolationError(
                f"profile environment value is not a safe name: {name}"
            )
        environment[name] = value
    environment.update(
        {
            "HOME": str(paths["home"]),
            "CODEX_HOME": str(paths["home"]),
            "TMPDIR": str(paths["temp"]),
            "PROMPTFOO_CACHE_PATH": str(paths["cache"] / "promptfoo"),
            "PROMPTFOO_CONFIG_DIR": str(paths["promptfoo"]),
            "PROMPTFOO_OUTPUT_PATH": str(paths["promptfoo"] / "output.json"),
            "PROMPTFOO_CACHE_ENABLED": "false",
            "FORCE_COLOR": "0",
            "NO_PROXY": "127.0.0.1,localhost,::1",
        }
    )
    if os.name == "nt":
        environment.update(
            _windows_environment_overrides(paths["home"], paths["cache"], paths["temp"])
        )
    return MappingProxyType(environment)


def _write_snapshot(
    filesystem: CellFilesystem, snapshot: TreeSnapshot, target: str
) -> None:
    filesystem.write_snapshot(
        ((relative, payload) for relative, (_, payload) in snapshot.files.items()),
        target,
    )


def _state_for(cell: AttemptCell, *, cleanup: bool = False) -> _CellState:
    if not isinstance(cell, AttemptCell):
        raise IsolationError("attempt cell is required")
    state = _CELLS.get(cell._capability)
    if state is None:
        raise IsolationError("attempt cell capability is invalid")
    if cell.environment is not state.environment:
        raise IsolationError("candidate environment binding changed")
    if cell.root != state.cell.root:
        raise IsolationError("attempt root identity binding changed")
    for name in _DIRECTORY_FIELDS:
        if getattr(cell, name) != getattr(state.cell, name):
            raise IsolationError(f"{name} path binding changed")
    if (
        cell.pair_id != state.cell.pair_id
        or cell.attempt_id != state.cell.attempt_id
        or cell.receipt_marker != state.cell.receipt_marker
    ):
        raise IsolationError("attempt identity or receipt binding changed")
    try:
        state.reservation.validate()
    except AuthorityError as error:
        raise IsolationError(str(error)) from error
    try:
        state.filesystem.validate(cleanup=cleanup)
    except (OSError, SecureFilesystemError) as error:
        raise IsolationError(str(error)) from error
    return state


def create_attempt_cell(
    base_root: Path,
    pair_id: str,
    attempt_id: str,
    codex_home_seed: Path,
    workspace_seed: Path,
    profile: object,
    source_environment: Mapping[str, str],
) -> AttemptCell:
    """Create one candidate attempt with independent bytes and retained capabilities."""
    pair = _validate_identifier("pair ID", pair_id)
    attempt = _validate_identifier("attempt ID", attempt_id)
    execution_profile = _validate_profile(profile)
    try:
        home_snapshot = snapshot_tree(Path(codex_home_seed))
        workspace_snapshot = snapshot_tree(Path(workspace_seed))
    except BatchPlanError as error:
        raise IsolationError(f"unsafe seed: {error}") from error
    base, inspected_fd = _validate_base(Path(base_root))
    close_handle(inspected_fd)
    nonce = secrets.token_bytes(32)
    try:
        reservation = reserve_attempt(pair, attempt, nonce)
    except AuthorityError as error:
        raise IsolationError(str(error)) from error
    root_name = (
        "cell-"
        + hashlib.sha256(pair.encode() + b"\0" + attempt.encode() + nonce).hexdigest()
    )
    filesystem: CellFilesystem | None = None
    try:
        filesystem = CellFilesystem.create(base, root_name)
        root = base / root_name
        paths = {name: root / name for name in _DIRECTORY_FIELDS}
        _write_snapshot(filesystem, home_snapshot, "home")
        _write_snapshot(filesystem, workspace_snapshot, "workspace")
        environment = _environment(paths, execution_profile, source_environment)
        capability = object()
        cell = AttemptCell(
            pair,
            attempt,
            root,
            paths["home"],
            paths["workspace"],
            paths["cache"],
            paths["temp"],
            paths["logs"],
            paths["promptfoo"],
            environment,
            root / _RECEIPT_NAME,
            capability,
        )
        state = _CellState(cell, filesystem, reservation, environment, nonce)
        _CELLS[capability] = state
        _state_for(cell)
        if not filesystem.base_is_still_bound():
            raise IsolationError("attempt base identity changed during creation")
        return cell
    except BaseException:
        if filesystem is not None:
            try:
                filesystem.delete_exact()
            except OSError:
                pass
        raise


def verify_attempt_cells_disjoint(left: AttemptCell, right: AttemptCell) -> None:
    """Fail unless two live cells have distinct identities and independent files."""
    left_state = _state_for(left)
    right_state = _state_for(right)
    if left_state is right_state:
        raise IsolationError("attempt cells use the same filesystem root")
    left_ancestors = ancestor_identities(left_state.filesystem.root_fd)
    right_ancestors = ancestor_identities(right_state.filesystem.root_fd)
    if (
        left_state.filesystem.root_identity in right_ancestors
        or right_state.filesystem.root_identity in left_ancestors
    ):
        raise IsolationError("attempt cell roots overlap")
    try:
        if (
            left_state.filesystem.scan_regular_identities()
            & right_state.filesystem.scan_regular_identities()
        ):
            raise IsolationError("attempt cells contain a shared hard link")
    except SecureFilesystemError as error:
        raise IsolationError(str(error)) from error


def _receipt_payload(state: _CellState) -> bytes:
    binding = (
        state.cell.pair_id.encode()
        + b"\0"
        + state.cell.attempt_id.encode()
        + b"\0"
        + repr(state.filesystem.root_identity).encode()
        + b"\0"
        + state.reservation.payload
        + state.nonce
    )
    signature = creator_signature(binding).hex().encode()
    return b"receipts-sealed-v2 " + signature + b"\n"


def mark_receipts_sealed(cell: AttemptCell) -> None:
    """Create an unforgeable cleanup gate bound to this live attempt."""
    state = _state_for(cell)
    if state.lifecycle != "active":
        raise IsolationError("receipts are already sealed")
    payload = _receipt_payload(state)
    try:
        state.filesystem.create_file(_RECEIPT_NAME, payload)
    except FileExistsError as error:
        raise IsolationError("forged receipt marker blocks sealing") from error
    except OSError as error:
        raise IsolationError("cannot seal attempt receipts") from error
    state.receipt = payload
    state.lifecycle = "sealed"


def _delete_cell_root(cell: AttemptCell) -> None:
    state = _CELLS[cell._capability]
    try:
        state.filesystem.delete_exact()
    except (OSError, SecureFilesystemError) as error:
        raise IsolationError(
            "cleanup refused a substituted or unstable root"
        ) from error


def cleanup_attempt_cell(cell: AttemptCell) -> None:
    """Delete exactly one handle-bound cell after its authenticated receipt seal."""
    state = _state_for(cell, cleanup=True)
    if state.lifecycle != "sealed" or state.receipt is None:
        raise IsolationError("receipts are not sealed")
    try:
        marker = state.filesystem.read_file(_RECEIPT_NAME, 256)
    except OSError as error:
        raise IsolationError("receipt seal is unavailable") from error
    if not hmac.compare_digest(marker, state.receipt):
        raise IsolationError("receipt seal is forged")
    _delete_cell_root(cell)
    state.lifecycle = "cleaned"
    del _CELLS[cell._capability]
