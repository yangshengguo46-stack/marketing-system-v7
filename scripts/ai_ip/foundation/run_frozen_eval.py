#!/usr/bin/env python3
from collections.abc import Sequence
from dataclasses import dataclass
from datetime import datetime
import hashlib
import json
import os
from pathlib import Path, PurePosixPath, PureWindowsPath
import platform
import re
import stat
import subprocess
import sys
from typing import NoReturn


class FrozenEvalError(ValueError):
    pass


FROZEN_EVALUATOR_KEYS = {
    "gitSha",
    "worktree",
    "macosX8664Binary",
    "macosX8664BinarySha256",
    "windows11X64Binary",
    "windows11X64BinarySha256",
}
FORBIDDEN_CHILD_FLAGS = {
    "--frozen-run-context",
    "--model",
    "--model-id",
    "--provider",
    "--provider-role",
    "--endpoint",
    "--case",
    "--case-fixture",
    "--binary",
    "--evaluator",
    "--codex-bin",
    "--budget",
    "--budget-fen",
    "--limit",
    "--attempt-limit",
    "--timeout",
    "--max-output-tokens",
}
SHA256_PATTERN = re.compile(r"[0-9a-f]{64}")
GIT_SHA_PATTERN = re.compile(r"[0-9a-f]{40}")
RFC3339_PATTERN = re.compile(
    r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}"
    r"(?:\.[0-9]+)?(?:Z|[+-][0-9]{2}:[0-9]{2})"
)
HARDENED_GIT_ENV = {
    "GIT_CONFIG_COUNT": "1",
    "GIT_CONFIG_GLOBAL": os.devnull,
    "GIT_CONFIG_KEY_0": "core.fsmonitor",
    "GIT_CONFIG_NOSYSTEM": "1",
    "GIT_CONFIG_VALUE_0": "false",
    "GIT_NO_LAZY_FETCH": "1",
    "GIT_NO_REPLACE_OBJECTS": "1",
    "GIT_OPTIONAL_LOCKS": "0",
    "GIT_TERMINAL_PROMPT": "0",
}
CHILD_ENVIRONMENT = {"PATH": os.defpath}
TRUSTED_GIT_EXECUTABLE = (
    Path("/usr/bin/git")
    if sys.platform == "darwin"
    else Path(r"C:\Program Files\Git\cmd\git.exe")
)


@dataclass(frozen=True)
class VerifiedExecutable:
    canonical_path: Path
    descriptor: int
    device: int
    inode: int
    size: int
    mtime_ns: int


def _reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise FrozenEvalError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise FrozenEvalError(message)


def _require_string(value: object, label: str) -> str:
    _require(
        isinstance(value, str) and bool(value), f"{label} must be a non-empty string"
    )
    assert isinstance(value, str)
    return value


def _require_sha256(value: object, label: str) -> str:
    digest = _require_string(value, label)
    _require(
        SHA256_PATTERN.fullmatch(digest) is not None,
        f"{label} must be lowercase SHA-256",
    )
    return digest


def _require_git_sha(value: object, label: str) -> str:
    digest = _require_string(value, label)
    _require(
        GIT_SHA_PATTERN.fullmatch(digest) is not None,
        f"{label} must be lowercase Git SHA",
    )
    return digest


def _canonical_existing_path(raw: object, label: str, *, directory: bool) -> Path:
    text = _require_string(raw, label)
    path = Path(text)
    _require(path.is_absolute(), f"{label} must be absolute")
    try:
        resolved = path.resolve(strict=True)
        path_stat = path.lstat()
    except OSError as error:
        raise FrozenEvalError(f"cannot access {label}: {error}") from error
    _require(path == resolved, f"{label} must be canonical and not use symlinks")
    if directory:
        _require(stat.S_ISDIR(path_stat.st_mode), f"{label} must be a directory")
    else:
        _require(stat.S_ISREG(path_stat.st_mode), f"{label} must be a regular file")
    return resolved


def _absolute_posix_frozen_string(raw: object, label: str) -> str:
    text = _require_string(raw, label)
    _require(
        PurePosixPath(text).is_absolute(), f"{label} must be an absolute frozen path"
    )
    return text


def _absolute_windows_frozen_string(raw: object, label: str) -> str:
    text = _require_string(raw, label)
    _require(
        PureWindowsPath(text).is_absolute(),
        f"{label} must be an absolute frozen path",
    )
    return text


def _is_within(child: Path, parent: Path) -> bool:
    try:
        child.relative_to(parent)
    except ValueError:
        return False
    return True


def _host_platform() -> str:
    machine = platform.machine().lower()
    if sys.platform == "darwin" and machine == "x86_64":
        return "macos-x86_64"
    if sys.platform == "win32":
        _require(
            machine in {"amd64", "x86_64"},
            f"unsupported evaluator host: {sys.platform}/{machine}",
        )
        windows_version = sys.getwindowsversion()
        _require(
            windows_version.major == 10
            and windows_version.build >= 22000
            and windows_version.product_type == 1,
            "frozen evaluator requires Windows 11 x64 workstation",
        )
        return "windows-11-x64"
    raise FrozenEvalError(f"unsupported evaluator host: {sys.platform}/{machine}")


def _git_environment() -> dict[str, str]:
    return dict(HARDENED_GIT_ENV)


def _trusted_git_path() -> Path:
    path = _canonical_existing_path(
        str(TRUSTED_GIT_EXECUTABLE), "trusted Git executable", directory=False
    )
    _require(os.access(path, os.X_OK), "trusted Git executable is not executable")
    return path


def _run_git(worktree: Path, *arguments: str) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        [str(_trusted_git_path()), "-C", str(worktree), *arguments],
        check=False,
        capture_output=True,
        env=_git_environment(),
    )


def _git_text(worktree: Path, *arguments: str) -> str:
    completed = _run_git(worktree, *arguments)
    if completed.returncode != 0:
        detail = completed.stderr.decode("utf-8", errors="replace").strip()
        raise FrozenEvalError(
            f"git {' '.join(arguments)} failed ({completed.returncode}): {detail}"
        )
    return completed.stdout.decode("utf-8", errors="strict").strip()


def _verify_git_worktree(worktree: Path, candidate_sha: str) -> None:
    _require(
        _git_text(worktree, "rev-parse", "--is-inside-work-tree") == "true",
        "frozen evaluator worktree is not a Git worktree",
    )
    symbolic_head = _run_git(worktree, "symbolic-ref", "-q", "HEAD")
    _require(
        symbolic_head.returncode == 1, "frozen evaluator worktree must be detached"
    )
    _require(
        _git_text(worktree, "rev-parse", "HEAD") == candidate_sha,
        "frozen evaluator worktree HEAD differs from candidateSha",
    )
    _require(
        not _git_text(worktree, "status", "--porcelain=v1", "--untracked-files=all"),
        "frozen evaluator worktree is dirty",
    )
    _require(
        _git_text(worktree, "rev-parse", "--is-shallow-repository") == "false",
        "shallow frozen evaluator worktree is forbidden",
    )
    _require(
        not _git_text(worktree, "for-each-ref", "--format=%(refname)", "refs/replace/"),
        "Git replace refs are forbidden",
    )
    git_dir = Path(
        _git_text(worktree, "rev-parse", "--path-format=absolute", "--git-dir")
    )
    common_dir = Path(
        _git_text(worktree, "rev-parse", "--path-format=absolute", "--git-common-dir")
    )
    grafts_path = Path(
        _git_text(
            worktree,
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "info/grafts",
        )
    )
    _require(
        all(path.is_absolute() for path in (git_dir, common_dir, grafts_path)),
        "Git metadata paths must be absolute",
    )
    forbidden_grafts = [
        path
        for path in {git_dir / "info/grafts", common_dir / "info/grafts", grafts_path}
        if os.path.lexists(path)
    ]
    _require(not forbidden_grafts, "Git info/grafts entries are forbidden")


def _open_verified_executable(path: Path, expected_digest: str) -> VerifiedExecutable:
    flags = os.O_RDONLY
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        descriptor = os.open(path, flags)
    except OSError as error:
        raise FrozenEvalError(
            f"cannot open frozen evaluator binary: {error}"
        ) from error
    digest = hashlib.sha256()
    try:
        file_stat = os.fstat(descriptor)
        _require(
            stat.S_ISREG(file_stat.st_mode),
            "frozen evaluator binary must be a regular file",
        )
        while chunk := os.read(descriptor, 1024 * 1024):
            digest.update(chunk)
    except FrozenEvalError:
        os.close(descriptor)
        raise
    except OSError as error:
        os.close(descriptor)
        raise FrozenEvalError(
            f"cannot hash frozen evaluator binary: {error}"
        ) from error
    if digest.hexdigest() != expected_digest:
        os.close(descriptor)
        raise FrozenEvalError("frozen evaluator binary SHA-256 differs from context")
    return VerifiedExecutable(
        canonical_path=path,
        descriptor=descriptor,
        device=file_stat.st_dev,
        inode=file_stat.st_ino,
        size=file_stat.st_size,
        mtime_ns=file_stat.st_mtime_ns,
    )


def _require_rfc3339(value: object, label: str) -> str:
    timestamp = _require_string(value, label)
    _require(
        RFC3339_PATTERN.fullmatch(timestamp) is not None, f"{label} must be RFC3339"
    )
    normalized = timestamp[:-1] + "+00:00" if timestamp.endswith("Z") else timestamp
    try:
        parsed = datetime.fromisoformat(normalized)
    except ValueError as error:
        raise FrozenEvalError(f"{label} must be RFC3339") from error
    _require("T" in timestamp and parsed.tzinfo is not None, f"{label} must be RFC3339")
    return timestamp


def reject_authority_overrides(argv: tuple[str, ...]) -> None:
    for token in argv:
        name = token.split("=", 1)[0]
        if name in FORBIDDEN_CHILD_FLAGS:
            raise FrozenEvalError(f"child argv overrides frozen authority: {name}")


def _load_context(context: Path) -> dict[str, object]:
    try:
        value = json.loads(
            context.read_text(encoding="utf-8"),
            object_pairs_hook=_reject_duplicate_keys,
        )
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise FrozenEvalError(f"cannot parse frozen run context: {error}") from error
    _require(isinstance(value, dict), "frozen run context must be an object")
    return value


def _validate_context(context: Path) -> VerifiedExecutable:
    root = _load_context(context)
    for required_key in {
        "schemaVersion",
        "executionMode",
        "candidateSha",
        "privateRoot",
        "platform",
        "frozenEvaluator",
    }:
        _require(
            required_key in root, f"missing frozen run context field: {required_key}"
        )
    _require(
        isinstance(root["schemaVersion"], int)
        and not isinstance(root["schemaVersion"], bool)
        and root["schemaVersion"] == 1,
        "schemaVersion must be integer 1",
    )
    execution_mode = _require_string(root["executionMode"], "executionMode")
    _require(execution_mode in {"replay", "live"}, "unknown executionMode")
    candidate_sha = _require_git_sha(root["candidateSha"], "candidateSha")
    context_platform = _require_string(root["platform"], "platform")
    host_platform = _host_platform()
    _require(
        context_platform == host_platform,
        "frozen evaluator platform does not match host",
    )

    private_root = _canonical_existing_path(
        root["privateRoot"], "privateRoot", directory=True
    )
    _require(
        _is_within(context, private_root), "context must be contained by privateRoot"
    )

    if execution_mode == "replay":
        _require_sha256(root.get("fixtureSetSha256"), "fixtureSetSha256")
        for forbidden in {
            "providerRole",
            "providerBudgetEvidenceSha256",
            "retentionDeadline",
        }:
            _require(
                forbidden not in root, f"{forbidden} is forbidden in replay context"
            )
    else:
        _require(
            root.get("providerRole") in {"targetVolcengine", "approvedReference"},
            "providerRole must be targetVolcengine or approvedReference",
        )
        _require_sha256(
            root.get("providerBudgetEvidenceSha256"), "providerBudgetEvidenceSha256"
        )
        _require_rfc3339(root.get("retentionDeadline"), "retentionDeadline")
        _require(
            "fixtureSetSha256" not in root,
            "fixtureSetSha256 is forbidden in live context",
        )

    evaluator = root["frozenEvaluator"]
    _require(isinstance(evaluator, dict), "frozenEvaluator must be an object")
    _require(
        set(evaluator) == FROZEN_EVALUATOR_KEYS,
        "frozenEvaluator keys do not match frozen evaluator contract",
    )
    evaluator_sha = _require_git_sha(evaluator["gitSha"], "frozenEvaluator.gitSha")
    _require(
        evaluator_sha == candidate_sha,
        "frozenEvaluator.gitSha differs from candidateSha",
    )
    worktree = _canonical_existing_path(
        evaluator["worktree"], "frozenEvaluator.worktree", directory=True
    )
    _require(
        not _is_within(worktree, private_root)
        and not _is_within(private_root, worktree),
        "privateRoot and frozen evaluator worktree must be disjoint",
    )
    _verify_git_worktree(worktree, candidate_sha)

    macos_binary = _absolute_posix_frozen_string(
        evaluator["macosX8664Binary"], "frozenEvaluator.macosX8664Binary"
    )
    macos_digest = _require_sha256(
        evaluator["macosX8664BinarySha256"], "frozenEvaluator.macosX8664BinarySha256"
    )
    windows_binary = _absolute_windows_frozen_string(
        evaluator["windows11X64Binary"], "frozenEvaluator.windows11X64Binary"
    )
    windows_digest = _require_sha256(
        evaluator["windows11X64BinarySha256"],
        "frozenEvaluator.windows11X64BinarySha256",
    )
    _require(
        windows_binary.lower().endswith(".exe"), "Windows evaluator must end in .exe"
    )

    selected_binary_text, selected_digest = (
        (macos_binary, macos_digest)
        if host_platform == "macos-x86_64"
        else (windows_binary, windows_digest)
    )
    selected_binary = _canonical_existing_path(
        selected_binary_text, "frozen evaluator binary", directory=False
    )
    _require(
        _is_within(selected_binary, worktree),
        "frozen evaluator binary must be contained by evaluator worktree",
    )
    if host_platform != "windows-11-x64":
        _require(
            os.access(selected_binary, os.X_OK),
            "frozen evaluator binary is not executable",
        )
    return _open_verified_executable(selected_binary, selected_digest)


def _parse_invocation(argv: Sequence[str]) -> tuple[Path, tuple[str, ...]]:
    try:
        separator_index = argv.index("--")
    except ValueError as error:
        raise FrozenEvalError(
            "expected -- before frozen evaluator arguments"
        ) from error
    prefix = tuple(argv[:separator_index])
    _require(
        len(prefix) == 2 and prefix[0] == "--context",
        "expected exactly --context ABS before --",
    )
    child_argv = tuple(argv[separator_index + 1 :])
    _require(child_argv, "frozen evaluator subcommand is required")
    subcommand = child_argv[0]
    _require(bool(subcommand), "frozen evaluator subcommand must be non-empty")
    _require(
        not subcommand.startswith("-"),
        "frozen evaluator subcommand must not start with -",
    )
    _require(
        "/" not in subcommand and "\\" not in subcommand,
        "frozen evaluator subcommand must not select a path",
    )
    _require(subcommand not in {".", ".."}, "frozen evaluator subcommand is unsafe")
    reject_authority_overrides(child_argv)
    return _canonical_existing_path(prefix[1], "context", directory=False), child_argv


def _exec(
    frozen_executable: VerifiedExecutable,
    child_argv: tuple[str, ...],
    context: Path,
) -> NoReturn:
    try:
        descriptor_stat = os.fstat(frozen_executable.descriptor)
        path_stat = frozen_executable.canonical_path.lstat()
    except OSError as error:
        os.close(frozen_executable.descriptor)
        raise FrozenEvalError(
            f"frozen evaluator changed after verification: {error}"
        ) from error
    expected_identity = (
        frozen_executable.device,
        frozen_executable.inode,
        frozen_executable.size,
        frozen_executable.mtime_ns,
    )
    descriptor_identity = (
        descriptor_stat.st_dev,
        descriptor_stat.st_ino,
        descriptor_stat.st_size,
        descriptor_stat.st_mtime_ns,
    )
    path_identity = (
        path_stat.st_dev,
        path_stat.st_ino,
        path_stat.st_size,
        path_stat.st_mtime_ns,
    )
    if (
        not stat.S_ISREG(path_stat.st_mode)
        or descriptor_identity != expected_identity
        or path_identity != expected_identity
    ):
        os.close(frozen_executable.descriptor)
        raise FrozenEvalError("frozen evaluator changed after verification")
    executable = (
        frozen_executable.descriptor
        if os.execve in os.supports_fd
        else frozen_executable.canonical_path
    )
    try:
        os.execve(
            executable,
            [
                str(frozen_executable.canonical_path),
                *child_argv,
                "--frozen-run-context",
                str(context),
            ],
            CHILD_ENVIRONMENT,
        )
    except OSError:
        os.close(frozen_executable.descriptor)
        raise
    raise AssertionError("os.execve unexpectedly returned")


def main(argv: Sequence[str] | None = None) -> int:
    arguments = tuple(sys.argv[1:] if argv is None else argv)
    try:
        context, child_argv = _parse_invocation(arguments)
        frozen_executable = _validate_context(context)
        _exec(frozen_executable, child_argv, context)
    except FrozenEvalError as error:
        print(f"run_frozen_eval: {error}", file=sys.stderr)
        return 1
    except OSError as error:
        print(f"run_frozen_eval: exec failed: {error}", file=sys.stderr)
        return 1
    raise AssertionError("unreachable after os.execve")


if __name__ == "__main__":
    raise SystemExit(main())
