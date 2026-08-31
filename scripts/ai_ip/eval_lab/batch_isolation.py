import hashlib
import os
import re
import shutil
import stat
import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Mapping

try:
    from .batch_contracts import BatchContractError, validate_named_contract
    from .batch_plan import BatchPlanError, TreeSnapshot, snapshot_tree
except ImportError:
    from batch_contracts import BatchContractError, validate_named_contract
    from batch_plan import BatchPlanError, TreeSnapshot, snapshot_tree


_REPO_ROOT = Path(__file__).resolve().parents[3]
_IDENTIFIER = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}\Z")
_ENVIRONMENT_NAME = re.compile(r"[A-Z_][A-Z0-9_]*\Z")
_LOCALE_NAME = re.compile(r"LC_[A-Z0-9_]+\Z")
_SECRET_NAME = re.compile(
    r"(?:API_?KEY|AUTH|BEARER|COOKIE|CREDENTIAL|PASS(?:WORD|WD)?|PRIVATE_?KEY|SECRET|TOKEN)"
)
_PROXY_NAMES = {"ALL_PROXY", "HTTP_PROXY", "HTTPS_PROXY", "FTP_PROXY"}
_CONTROLLED_ENVIRONMENT = {
    "HOME",
    "CODEX_HOME",
    "TMPDIR",
    "PROMPTFOO_CACHE_PATH",
    "PROMPTFOO_CONFIG_DIR",
    "PROMPTFOO_OUTPUT_PATH",
    "PROMPTFOO_CACHE_ENABLED",
    "FORCE_COLOR",
    "NO_PROXY",
}
_DIRECTORY_FIELDS = ("home", "workspace", "cache", "temp", "logs", "promptfoo")
_RECEIPT_NAME = ".receipts-sealed"
_RECEIPT_PAYLOAD = b"receipts-sealed-v1\n"
_REPARSE_POINT = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
_NOFOLLOW = getattr(os, "O_NOFOLLOW", 0)
_BINARY = getattr(os, "O_BINARY", 0)


class IsolationError(ValueError):
    pass


class _FrozenEnvironment(dict[str, str]):
    def _reject_mutation(self, *args: object, **kwargs: object) -> None:
        raise TypeError("candidate environment is immutable")

    __setitem__ = _reject_mutation
    __delitem__ = _reject_mutation
    __ior__ = _reject_mutation
    clear = _reject_mutation
    pop = _reject_mutation
    popitem = _reject_mutation
    setdefault = _reject_mutation
    update = _reject_mutation


@dataclass(frozen=True)
class AttemptCell:
    root: Path
    home: Path
    workspace: Path
    cache: Path
    temp: Path
    logs: Path
    promptfoo: Path
    environment: dict[str, str]
    receipt_marker: Path
    _root_identity: tuple[int, int] = field(repr=False, compare=False)


def _identity(metadata: os.stat_result) -> tuple[int, int]:
    return metadata.st_dev, metadata.st_ino


def _is_link_or_reparse(metadata: os.stat_result) -> bool:
    return stat.S_ISLNK(metadata.st_mode) or bool(
        getattr(metadata, "st_file_attributes", 0) & _REPARSE_POINT
    )


def _same_path(left: Path, right: Path) -> bool:
    return os.path.normcase(os.path.abspath(left)) == os.path.normcase(
        os.path.abspath(right)
    )


def _is_within(path: Path, directory: Path) -> bool:
    candidate = os.path.normcase(os.path.abspath(path))
    parent = os.path.normcase(os.path.abspath(directory))
    try:
        return os.path.commonpath((candidate, parent)) == parent
    except ValueError:
        return False


def _reject_link_ancestors(path: Path) -> None:
    current = Path(path.anchor)
    for part in path.parts[1:]:
        current /= part
        try:
            metadata = current.lstat()
        except FileNotFoundError:
            continue
        except OSError as error:
            raise IsolationError(f"cannot inspect path ancestor: {current}") from error
        if _is_link_or_reparse(metadata):
            raise IsolationError(
                f"symlink or reparse path component is forbidden: {current}"
            )


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
    worktrees = []
    for record in result.stdout.split(b"\0"):
        if record.startswith(b"worktree "):
            worktrees.append(Path(os.fsdecode(record[len(b"worktree ") :])).resolve())
    if not worktrees or not any(_same_path(_REPO_ROOT, path) for path in worktrees):
        raise IsolationError("Git worktree listing omitted the current repository")
    return tuple(worktrees)


def _validate_root_location(root: Path, worktrees: tuple[Path, ...]) -> None:
    if any(_is_within(root, worktree) for worktree in worktrees):
        raise IsolationError("attempt root must be outside every Git worktree")


def _validate_base(base_root: Path) -> tuple[Path, tuple[Path, ...]]:
    candidate = Path(base_root)
    if not candidate.is_absolute():
        raise IsolationError("attempt base must be absolute")
    _reject_link_ancestors(candidate)
    try:
        base = candidate.resolve(strict=True)
        metadata = base.lstat()
    except OSError as error:
        raise IsolationError("attempt base must be an existing directory") from error
    if _is_link_or_reparse(metadata) or not stat.S_ISDIR(metadata.st_mode):
        raise IsolationError("attempt base must be a non-reparse directory")
    worktrees = _git_worktrees()
    _validate_root_location(base, worktrees)
    return base, worktrees


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
    return profile


def _allowed_profile_names(profile: dict[str, object]) -> tuple[str, ...]:
    names = profile["environmentAllowlist"]
    assert isinstance(names, list)
    validated = []
    for name in names:
        if type(name) is not str or not _ENVIRONMENT_NAME.fullmatch(name):
            raise IsolationError(
                "profile environment name must be uppercase and portable"
            )
        if name in _PROXY_NAMES or name.endswith("_PROXY"):
            raise IsolationError(f"proxy environment name is forbidden: {name}")
        if _SECRET_NAME.search(name):
            raise IsolationError(f"secret-like environment name is forbidden: {name}")
        if name in _CONTROLLED_ENVIRONMENT:
            raise IsolationError(
                f"candidate-controlled environment name is reserved: {name}"
            )
        validated.append(name)
    return tuple(validated)


def _environment(
    cell_paths: dict[str, Path],
    profile: dict[str, object],
    source: Mapping[str, str],
) -> dict[str, str]:
    if not isinstance(source, Mapping):
        raise IsolationError("source environment must be a mapping")
    profile_names = _allowed_profile_names(profile)
    copied_names = {"PATH", "SHELL", "LANG", "LANGUAGE", *profile_names}
    copied_names.update(
        name for name in source if type(name) is str and _LOCALE_NAME.fullmatch(name)
    )
    environment: dict[str, str] = {}
    for name in copied_names:
        if name not in source:
            continue
        value = source[name]
        if type(value) is not str or "\0" in value:
            raise IsolationError(f"invalid environment value: {name}")
        environment[name] = value
    environment.update(
        {
            "HOME": str(cell_paths["home"]),
            "CODEX_HOME": str(cell_paths["home"]),
            "TMPDIR": str(cell_paths["temp"]),
            "PROMPTFOO_CACHE_PATH": str(cell_paths["cache"] / "promptfoo"),
            "PROMPTFOO_CONFIG_DIR": str(cell_paths["promptfoo"]),
            "PROMPTFOO_OUTPUT_PATH": str(cell_paths["promptfoo"] / "output.json"),
            "PROMPTFOO_CACHE_ENABLED": "false",
            "FORCE_COLOR": "0",
            "NO_PROXY": "127.0.0.1,localhost,::1",
        }
    )
    return _FrozenEnvironment(environment)


def _make_private_directory(path: Path) -> None:
    try:
        path.mkdir(mode=0o700)
        os.chmod(path, 0o700)
    except OSError as error:
        raise IsolationError(f"cannot create private directory: {path.name}") from error


def _write_private_file(path: Path, payload: bytes) -> None:
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | _NOFOLLOW | _BINARY
    descriptor = None
    try:
        descriptor = os.open(path, flags, 0o600)
        os.chmod(path, 0o600)
        view = memoryview(payload)
        while view:
            written = os.write(descriptor, view)
            if written <= 0:
                raise IsolationError(f"short write: {path.name}")
            view = view[written:]
        os.fsync(descriptor)
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1:
            raise IsolationError(f"private file is not independent: {path.name}")
    except IsolationError:
        raise
    except OSError as error:
        raise IsolationError(f"cannot write private file: {path.name}") from error
    finally:
        if descriptor is not None:
            os.close(descriptor)


def _write_snapshot(snapshot: TreeSnapshot, destination: Path) -> None:
    directories: set[Path] = set()
    entries: list[tuple[Path, bytes]] = []
    for relative_text, (_, payload) in snapshot.files.items():
        relative = Path(relative_text)
        if relative.is_absolute() or any(
            part in {"", ".", ".."} for part in relative.parts
        ):
            raise IsolationError("snapshot contains an unsafe relative path")
        entries.append((relative, payload))
        directories.update(parent for parent in relative.parents if parent != Path("."))
    for relative in sorted(directories, key=lambda path: (len(path.parts), path.parts)):
        _make_private_directory(destination / relative)
    for relative, payload in sorted(entries, key=lambda entry: entry[0].parts):
        _write_private_file(destination / relative, payload)


def _reserve_attempt_id(base: Path, attempt_id: str, pair_id: str) -> Path:
    registry = base / ".attempt-ids"
    if not registry.exists():
        try:
            _make_private_directory(registry)
        except IsolationError:
            if not registry.exists():
                raise
    metadata = registry.lstat()
    if _is_link_or_reparse(metadata) or not stat.S_ISDIR(metadata.st_mode):
        raise IsolationError("attempt ID registry is unsafe")
    if os.name != "nt" and stat.S_IMODE(metadata.st_mode) != 0o700:
        raise IsolationError("attempt ID registry must have mode 0700")
    digest = hashlib.sha256(attempt_id.encode("utf-8")).hexdigest()
    reservation = registry / digest
    try:
        _write_private_file(reservation, f"{pair_id}\n{attempt_id}\n".encode("utf-8"))
    except IsolationError as error:
        if reservation.exists():
            raise IsolationError("attempt ID was already used") from error
        raise
    return reservation


def _remove_failed_creation(root: Path, root_identity: tuple[int, int] | None) -> None:
    if root_identity is None:
        return
    try:
        metadata = root.lstat()
    except FileNotFoundError:
        return
    if _is_link_or_reparse(metadata) or _identity(metadata) != root_identity:
        return
    shutil.rmtree(root)


def create_attempt_cell(
    base_root: Path,
    pair_id: str,
    attempt_id: str,
    codex_home_seed: Path,
    workspace_seed: Path,
    profile: object,
    source_environment: Mapping[str, str],
) -> AttemptCell:
    """Create one candidate attempt with independent bytes and runtime state."""
    pair = _validate_identifier("pair ID", pair_id)
    attempt = _validate_identifier("attempt ID", attempt_id)
    execution_profile = _validate_profile(profile)
    _allowed_profile_names(execution_profile)
    try:
        home_snapshot = snapshot_tree(Path(codex_home_seed))
        workspace_snapshot = snapshot_tree(Path(workspace_seed))
    except BatchPlanError as error:
        raise IsolationError(f"unsafe seed: {error}") from error
    base, worktrees = _validate_base(Path(base_root))
    root_name = hashlib.sha256(f"{pair}\0{attempt}".encode("utf-8")).hexdigest()
    root = base / f"cell-{root_name}"
    _validate_root_location(root, worktrees)
    reservation = _reserve_attempt_id(base, attempt, pair)
    root_identity = None
    try:
        _make_private_directory(root)
        root_identity = _identity(root.lstat())
        paths = {name: root / name for name in _DIRECTORY_FIELDS}
        for path in paths.values():
            _make_private_directory(path)
        _make_private_directory(paths["cache"] / "promptfoo")
        _write_snapshot(home_snapshot, paths["home"])
        _write_snapshot(workspace_snapshot, paths["workspace"])
        environment = _environment(paths, execution_profile, source_environment)
        cell = AttemptCell(
            root=root,
            home=paths["home"],
            workspace=paths["workspace"],
            cache=paths["cache"],
            temp=paths["temp"],
            logs=paths["logs"],
            promptfoo=paths["promptfoo"],
            environment=environment,
            receipt_marker=root / _RECEIPT_NAME,
            _root_identity=root_identity,
        )
        _validate_cell(cell)
        return cell
    except BaseException:
        _remove_failed_creation(root, root_identity)
        try:
            reservation.unlink()
        except OSError:
            pass
        raise


def _validate_private_directory(path: Path, description: str) -> None:
    try:
        metadata = path.lstat()
    except OSError as error:
        raise IsolationError(f"{description} directory is unavailable") from error
    if _is_link_or_reparse(metadata) or not stat.S_ISDIR(metadata.st_mode):
        raise IsolationError(f"{description} path must be a private directory")
    if os.name != "nt" and stat.S_IMODE(metadata.st_mode) != 0o700:
        raise IsolationError(f"{description} directory must have mode 0700")


def _validate_cell(cell: AttemptCell) -> None:
    if not isinstance(cell, AttemptCell):
        raise IsolationError("attempt cell is required")
    if not cell.root.is_absolute():
        raise IsolationError("attempt root must be absolute")
    _reject_link_ancestors(cell.root)
    _validate_private_directory(cell.root, "root")
    if _identity(cell.root.lstat()) != cell._root_identity:
        raise IsolationError("attempt root identity changed")
    _validate_root_location(cell.root, _git_worktrees())
    for name in _DIRECTORY_FIELDS:
        actual = getattr(cell, name)
        expected = cell.root / name
        if not _same_path(actual, expected):
            raise IsolationError(f"{name} path differs from the isolated layout")
        _validate_private_directory(actual, name)
    if not _same_path(cell.receipt_marker, cell.root / _RECEIPT_NAME):
        raise IsolationError("receipt marker path differs from the isolated layout")
    expected_environment = {
        "HOME": str(cell.home),
        "CODEX_HOME": str(cell.home),
        "TMPDIR": str(cell.temp),
        "PROMPTFOO_CACHE_PATH": str(cell.cache / "promptfoo"),
        "PROMPTFOO_CONFIG_DIR": str(cell.promptfoo),
        "PROMPTFOO_OUTPUT_PATH": str(cell.promptfoo / "output.json"),
        "PROMPTFOO_CACHE_ENABLED": "false",
        "FORCE_COLOR": "0",
        "NO_PROXY": "127.0.0.1,localhost,::1",
    }
    for name, expected in expected_environment.items():
        if cell.environment.get(name) != expected:
            raise IsolationError(f"candidate environment changed: {name}")
    try:
        snapshot_tree(cell.root)
    except BatchPlanError as error:
        raise IsolationError(
            f"attempt cell contains a link or unstable entry: {error}"
        ) from error


def verify_attempt_cells_disjoint(left: AttemptCell, right: AttemptCell) -> None:
    """Fail unless two candidate cells have distinct roots and runtime paths."""
    _validate_cell(left)
    _validate_cell(right)
    if _is_within(left.root, right.root) or _is_within(right.root, left.root):
        raise IsolationError("attempt cell roots overlap")
    for name in _DIRECTORY_FIELDS:
        left_path = getattr(left, name)
        right_path = getattr(right, name)
        if _is_within(left_path, right_path) or _is_within(right_path, left_path):
            raise IsolationError(f"{name} path is shared between attempt cells")


def mark_receipts_sealed(cell: AttemptCell) -> None:
    """Seal the cleanup gate after all attempt receipts have been persisted."""
    _validate_cell(cell)
    if cell.receipt_marker.exists():
        raise IsolationError("receipts are already sealed")
    _write_private_file(cell.receipt_marker, _RECEIPT_PAYLOAD)


def cleanup_attempt_cell(cell: AttemptCell) -> None:
    """Delete exactly one identity-bound cell after its receipt gate is sealed."""
    _validate_cell(cell)
    try:
        marker = cell.receipt_marker.lstat()
    except FileNotFoundError as error:
        raise IsolationError("receipts are not sealed") from error
    except OSError as error:
        raise IsolationError("receipt seal is unavailable") from error
    if (
        _is_link_or_reparse(marker)
        or not stat.S_ISREG(marker.st_mode)
        or marker.st_nlink != 1
        or (os.name != "nt" and stat.S_IMODE(marker.st_mode) != 0o600)
    ):
        raise IsolationError("receipt seal is unsafe")
    try:
        payload = cell.receipt_marker.read_bytes()
    except OSError as error:
        raise IsolationError("receipt seal is unavailable") from error
    if payload != _RECEIPT_PAYLOAD:
        raise IsolationError("receipts are not sealed")
    if _identity(cell.root.lstat()) != cell._root_identity:
        raise IsolationError("attempt root identity changed")
    try:
        shutil.rmtree(cell.root)
    except OSError as error:
        raise IsolationError("cannot clean the exact attempt cell") from error
    if cell.root.exists():
        raise IsolationError("attempt cell cleanup was incomplete")
