#!/usr/bin/env python3
import argparse
import ctypes
import hashlib
import io
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import threading
import uuid
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import BinaryIO, Callable, Mapping


class EvidenceError(ValueError):
    pass


CHUNK_SIZE = 1024 * 1024
SHA_PATTERN = re.compile(r"[0-9a-f]{40}")
CHILD_ENV_ALLOWLIST = {
    "PATH",
    "HOME",
    "TMPDIR",
    "TMP",
    "TEMP",
    "SystemRoot",
    "ComSpec",
    "PATHEXT",
    "CARGO_HOME",
    "CARGO_TARGET_DIR",
    "npm_config_store_dir",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
}
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
LOCK_PATHS = {
    "cargoLockSha256": "codex-rs/Cargo.lock",
    "pnpmLockSha256": "pnpm-lock.yaml",
    "bazelLockSha256": "MODULE.bazel.lock",
}
COMMAND_MANIFEST_KEYS = {
    "schemaVersion",
    "commandId",
    "phase",
    "argv",
    "expectedExit",
    "exitCode",
    "status",
    "startedAt",
    "endedAt",
    "platform",
    "architecture",
    "testedGitSha",
    "toolsGitSha",
    "recorderSha256",
    "matrixSha256",
    "locks",
    "stdout",
    "stderr",
    "selection",
}
SELECTION_ROOT_KEYS = {"rust-build-meta", "test-count", "rust-suites"}
REQUIRED_TOOL_NAMES = (
    "python",
    "uv",
    "git",
    "just",
    "dotslash",
    "rustc",
    "cargo",
    "cargo-nextest",
    "cargo-deny",
    "node",
    "pnpm",
    "bazelisk",
    "bazel",
)


@dataclass(frozen=True)
class CommandSpec:
    id: str
    platforms: tuple[str, ...]
    phase: str
    argv: tuple[str, ...]
    expected_exit: int


@dataclass(frozen=True)
class Matrix:
    schema_version: int
    commands: tuple[CommandSpec, ...]

    def required_for(self, platform_name: str, mode: str) -> tuple[CommandSpec, ...]:
        if mode not in {"baseline", "post"}:
            raise EvidenceError(f"unknown evidence mode: {mode}")
        phases = (
            {"baselineAndPost"}
            if mode == "baseline"
            else {"baselineAndPost", "postOnly"}
        )
        return tuple(
            command
            for command in self.commands
            if platform_name in command.platforms and command.phase in phases
        )


@dataclass(frozen=True)
class StreamReceipt:
    path: str
    sha256: str
    bytes: int


@dataclass(frozen=True)
class ProcessReceipt:
    argv: tuple[str, ...]
    exit_code: int
    started_at: str
    ended_at: str
    stdout: StreamReceipt
    stderr: StreamReceipt


@dataclass(frozen=True)
class _PendingProcess:
    receipt: ProcessReceipt
    stdout_temp: Path
    stderr_temp: Path


def canonical_json_bytes(value: object) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
        allow_nan=False,
    ).encode("utf-8")


def _reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise EvidenceError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _require_exact_keys(
    value: object, expected: set[str], description: str
) -> dict[str, object]:
    if not isinstance(value, dict):
        raise EvidenceError(f"{description} must be an object")
    keys = set(value)
    if keys != expected:
        unknown = keys - expected
        if unknown:
            raise EvidenceError(
                f"unknown {description} keys: {', '.join(sorted(unknown))}"
            )
        raise EvidenceError(f"invalid {description} keys")
    return value


def load_matrix(path: Path) -> Matrix:
    try:
        parsed = json.loads(
            path.read_text(encoding="utf-8"), object_pairs_hook=_reject_duplicate_keys
        )
    except json.JSONDecodeError as error:
        raise EvidenceError(f"invalid matrix JSON: {error}") from error
    root = _require_exact_keys(parsed, {"schemaVersion", "commands"}, "matrix")
    schema_version = root["schemaVersion"]
    if (
        not isinstance(schema_version, int)
        or isinstance(schema_version, bool)
        or schema_version != 1
    ):
        raise EvidenceError("schemaVersion must be integer 1")
    entries = root["commands"]
    if not isinstance(entries, list):
        raise EvidenceError("commands must be an array")
    commands: list[CommandSpec] = []
    ids: set[str] = set()
    valid_platforms = {"macos-x86_64", "windows-11-x64"}
    valid_phases = {"baselineAndPost", "postOnly"}
    for entry in entries:
        command = _require_exact_keys(
            entry, {"id", "platforms", "phase", "argv", "expectedExit"}, "command"
        )
        command_id = command["id"]
        if not isinstance(command_id, str) or not command_id:
            raise EvidenceError("id must be a non-empty string")
        if command_id in ids:
            raise EvidenceError(f"duplicate command id: {command_id}")
        platforms = command["platforms"]
        if (
            not isinstance(platforms, list)
            or not platforms
            or any(not isinstance(item, str) for item in platforms)
        ):
            raise EvidenceError("platforms must be a non-empty array of strings")
        if any(item not in valid_platforms for item in platforms):
            raise EvidenceError("unknown platform")
        if len(set(platforms)) != len(platforms):
            raise EvidenceError("duplicate platform")
        phase = command["phase"]
        if not isinstance(phase, str) or phase not in valid_phases:
            raise EvidenceError("unknown phase")
        argv = command["argv"]
        if not isinstance(argv, list) or not argv:
            raise EvidenceError("argv must be a non-empty array")
        if any(not isinstance(argument, str) or not argument for argument in argv):
            raise EvidenceError("argv arguments must be non-empty strings")
        expected_exit = command["expectedExit"]
        if not isinstance(expected_exit, int) or isinstance(expected_exit, bool):
            raise EvidenceError("expectedExit must be an integer")
        if expected_exit != 0:
            raise EvidenceError("expectedExit must be zero")
        ids.add(command_id)
        commands.append(
            CommandSpec(command_id, tuple(platforms), phase, tuple(argv), expected_exit)
        )
    return Matrix(schema_version, tuple(commands))


def host_id(sysctl_command: str = "sysctl") -> str:
    system, machine = platform.system(), platform.machine()
    if (system, machine) == ("Darwin", "x86_64"):
        try:
            translated = subprocess.run(
                [sysctl_command, "-n", "sysctl.proc_translated"],
                capture_output=True,
                text=True,
                check=False,
            )
        except OSError as error:
            raise EvidenceError("unable to characterize macOS host") from error
        if (
            translated.returncode == 0
            and translated.stdout == "0\n"
            and not translated.stderr
        ):
            return "macos-x86_64"
        if (
            translated.returncode == 1
            and not translated.stdout
            and translated.stderr == "sysctl: unknown oid 'sysctl.proc_translated'\n"
        ):
            return "macos-x86_64"
        raise EvidenceError("unsupported macOS translation state")
    if system == "Windows" and machine in {"AMD64", "x86_64"}:
        version = platform.version()
        if platform.release() == "11" or (
            version.split(".")[-1].isdigit() and int(version.split(".")[-1]) >= 22000
        ):
            return "windows-11-x64"
    raise EvidenceError(f"unsupported evidence host: {system}/{machine}")


def selection_argv(command: CommandSpec) -> tuple[str, ...] | None:
    if command.argv[:2] != ("just", "test") or "-E" not in command.argv:
        return None
    return ("cargo", "nextest", "list", "--message-format", "json", *command.argv[2:])


def canonical_rfc3339_utc_now() -> str:
    return (
        datetime.now(timezone.utc)
        .isoformat(timespec="microseconds")
        .replace("+00:00", "Z")
    )


def require_absolute_directory(raw: str, label: str) -> Path:
    lexical = Path(raw)
    if not lexical.is_absolute():
        raise EvidenceError(f"{label} must be absolute")
    try:
        resolved = lexical.resolve(strict=True)
    except OSError as error:
        raise EvidenceError(f"{label} must be a real directory") from error
    if not resolved.is_dir() or _path_has_symlink_component(lexical):
        raise EvidenceError(f"{label} must be a real directory")
    return resolved


def _require_absolute_file(raw: str, label: str) -> Path:
    lexical = Path(raw)
    if not lexical.is_absolute():
        raise EvidenceError(f"{label} must be absolute")
    try:
        resolved = lexical.resolve(strict=True)
    except OSError as error:
        raise EvidenceError(f"{label} must be a real file") from error
    if not resolved.is_file() or _path_has_symlink_component(lexical):
        raise EvidenceError(f"{label} must be a real file")
    return resolved


def _path_has_symlink_component(path: Path) -> bool:
    current = Path(path.anchor)
    for part in path.parts[1:]:
        current /= part
        if current.is_symlink():
            return True
    return False


def paths_are_disjoint(left: Path, right: Path) -> bool:
    return left != right and left not in right.parents and right not in left.parents


def _sanitized_child_environment(
    ambient: Mapping[str, str], *, platform_name: str | None = None
) -> dict[str, str]:
    target = os.name if platform_name is None else platform_name
    required = (
        {"PATH", "HOME"}
        if target != "nt"
        else {
            "PATH",
            "SystemRoot",
            "ComSpec",
            "PATHEXT",
        }
    )
    missing = sorted(key for key in required if not ambient.get(key))
    if missing:
        raise EvidenceError(f"missing required environment: {', '.join(missing)}")
    child: dict[str, str] = {}
    for key in CHILD_ENV_ALLOWLIST:
        value = ambient.get(key)
        if value is None:
            continue
        if "\0" in key or "\0" in value:
            raise EvidenceError(f"environment variable {key} contains NUL")
        child[key] = value
    child.update(HARDENED_GIT_ENV)
    return child


def _run_git(
    repo: Path, env: Mapping[str, str], *args: str
) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        ["git", "-C", str(repo), *args],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
        env=dict(env),
    )


def _git_bytes(repo: Path, env: Mapping[str, str], *args: str) -> bytes:
    completed = _run_git(repo, env, *args)
    if completed.returncode != 0:
        detail = completed.stderr.decode("utf-8", errors="replace").strip()
        raise EvidenceError(
            f"git {' '.join(args)} failed ({completed.returncode}): {detail}"
        )
    return completed.stdout


def _git_text(repo: Path, env: Mapping[str, str], *args: str) -> str:
    return _git_bytes(repo, env, *args).decode("utf-8").strip()


def _require_tracked_blob(
    repo: Path, relative_path: str, env: Mapping[str, str], label: str
) -> None:
    entries = [
        entry
        for entry in _git_bytes(
            repo, env, "ls-files", "--stage", "-z", "--", relative_path
        ).split(b"\0")
        if entry
    ]
    if len(entries) != 1:
        raise EvidenceError(f"{label} must be a single stage-0 100644 blob")
    metadata, separator, indexed_path = entries[0].partition(b"\t")
    fields = metadata.split(b" ")
    if (
        separator != b"\t"
        or indexed_path != relative_path.encode("utf-8")
        or len(fields) != 3
        or fields[0] != b"100644"
        or fields[2] != b"0"
    ):
        raise EvidenceError(f"{label} must be a single stage-0 100644 blob")
    try:
        object_id = fields[1].decode("ascii")
    except UnicodeDecodeError as error:
        raise EvidenceError(f"{label} must be a single stage-0 100644 blob") from error
    if (
        SHA_PATTERN.fullmatch(object_id) is None
        or _git_text(repo, env, "cat-file", "-t", object_id) != "blob"
    ):
        raise EvidenceError(f"{label} must be a single stage-0 100644 blob")


def _require_safe_git_repository(
    repo: Path,
    env: Mapping[str, str],
    *,
    require_locks: bool,
    label: str,
) -> str:
    top_level = Path(_git_text(repo, env, "rev-parse", "--show-toplevel")).resolve()
    if top_level != repo:
        raise EvidenceError(f"{label} must be a Git top-level")
    head = _git_text(repo, env, "rev-parse", "HEAD")
    if SHA_PATTERN.fullmatch(head) is None:
        raise EvidenceError(f"{label} HEAD must be lowercase 40-hex")
    symbolic_head = _run_git(repo, env, "symbolic-ref", "-q", "HEAD")
    if symbolic_head.returncode == 0:
        raise EvidenceError(f"{label} must be detached")
    if symbolic_head.returncode != 1:
        raise EvidenceError(f"unable to inspect {label} detached state")
    if _git_text(repo, env, "rev-parse", "--is-shallow-repository") != "false":
        raise EvidenceError(f"{label} must not be shallow")
    replace_refs = _git_text(
        repo, env, "for-each-ref", "--format=%(refname)", "refs/replace/"
    )
    if replace_refs:
        raise EvidenceError(f"{label} has forbidden replace refs")
    git_dir = Path(
        _git_text(repo, env, "rev-parse", "--path-format=absolute", "--git-dir")
    )
    common_dir = Path(
        _git_text(repo, env, "rev-parse", "--path-format=absolute", "--git-common-dir")
    )
    git_grafts = Path(
        _git_text(
            repo,
            env,
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "info/grafts",
        )
    )
    if any(
        os.path.lexists(path)
        for path in {git_dir / "info/grafts", common_dir / "info/grafts", git_grafts}
    ):
        raise EvidenceError(f"{label} has forbidden graft state")
    promisor = _run_git(
        repo,
        env,
        "config",
        "--get-regexp",
        r"^(remote\..*\.promisor|extensions\.partialClone)$",
    )
    if promisor.returncode not in {0, 1}:
        raise EvidenceError(f"unable to inspect {label} promisor state")
    if promisor.returncode == 0 and promisor.stdout.strip():
        raise EvidenceError(f"{label} has forbidden promisor state")
    status = _git_text(repo, env, "status", "--porcelain=v1", "--untracked-files=all")
    if status:
        raise EvidenceError(f"{label} is not clean")
    if require_locks:
        for relative_path in LOCK_PATHS.values():
            path = repo / relative_path
            if not path.is_file() or path.is_symlink():
                raise EvidenceError(f"missing exact lock file: {relative_path}")
            _require_tracked_blob(repo, relative_path, env, relative_path)
    return head


def _require_clean_after(repo: Path, env: Mapping[str, str], label: str) -> None:
    status = _git_text(repo, env, "status", "--porcelain=v1", "--untracked-files=all")
    if status:
        raise EvidenceError(f"{label} became dirty")


def _sha256_file(path: Path) -> tuple[str, int]:
    digest = hashlib.sha256()
    count = 0
    with path.open("rb") as source:
        while True:
            chunk = source.read(CHUNK_SIZE)
            if not chunk:
                break
            digest.update(chunk)
            count += len(chunk)
    return digest.hexdigest(), count


def _stream_reader(reader: BinaryIO, temp_path: Path) -> tuple[str, int]:
    digest = hashlib.sha256()
    count = 0
    descriptor = os.open(temp_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "wb") as output:
        while True:
            chunk = reader.read(CHUNK_SIZE)
            if not chunk:
                break
            output.write(chunk)
            digest.update(chunk)
            count += len(chunk)
        output.flush()
        os.fsync(output.fileno())
    return digest.hexdigest(), count


def _temporary_path(final_path: Path) -> Path:
    return final_path.with_name(f"{final_path.name}.tmp-{uuid.uuid4().hex}")


def _run_process(
    argv: tuple[str, ...],
    repo_root: Path,
    evidence_dir: Path,
    env: Mapping[str, str],
    stdout_name: str,
    stderr_name: str,
) -> _PendingProcess:
    stdout_final = evidence_dir / stdout_name
    stderr_final = evidence_dir / stderr_name
    stdout_temp = _temporary_path(stdout_final)
    stderr_temp = _temporary_path(stderr_final)
    started_at = canonical_rfc3339_utc_now()
    try:
        process = subprocess.Popen(
            list(argv),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            shell=False,
            cwd=repo_root,
            env=dict(env),
        )
    except OSError:
        _stream_reader(io.BytesIO(b""), stdout_temp)
        _stream_reader(io.BytesIO(b"unable to start command\n"), stderr_temp)
        ended_at = canonical_rfc3339_utc_now()
        stdout_sha, stdout_bytes = _sha256_file(stdout_temp)
        stderr_sha, stderr_bytes = _sha256_file(stderr_temp)
        return _PendingProcess(
            ProcessReceipt(
                argv,
                127,
                started_at,
                ended_at,
                StreamReceipt(stdout_name, stdout_sha, stdout_bytes),
                StreamReceipt(stderr_name, stderr_sha, stderr_bytes),
            ),
            stdout_temp,
            stderr_temp,
        )
    assert process.stdout is not None and process.stderr is not None
    results: dict[str, tuple[str, int]] = {}
    errors: list[BaseException] = []

    def read_stream(name: str, reader: BinaryIO, path: Path) -> None:
        try:
            results[name] = _stream_reader(reader, path)
        except BaseException as error:
            errors.append(error)

    stdout_thread = threading.Thread(
        target=read_stream, args=("stdout", process.stdout, stdout_temp), daemon=True
    )
    stderr_thread = threading.Thread(
        target=read_stream, args=("stderr", process.stderr, stderr_temp), daemon=True
    )
    stdout_thread.start()
    stderr_thread.start()
    exit_code = process.wait()
    stdout_thread.join()
    stderr_thread.join()
    process.stdout.close()
    process.stderr.close()
    if errors:
        for path in (stdout_temp, stderr_temp):
            try:
                path.unlink()
            except FileNotFoundError:
                pass
        raise EvidenceError("unable to capture command stream") from errors[0]
    ended_at = canonical_rfc3339_utc_now()
    stdout_sha, stdout_bytes = results["stdout"]
    stderr_sha, stderr_bytes = results["stderr"]
    return _PendingProcess(
        ProcessReceipt(
            argv,
            exit_code,
            started_at,
            ended_at,
            StreamReceipt(stdout_name, stdout_sha, stdout_bytes),
            StreamReceipt(stderr_name, stderr_sha, stderr_bytes),
        ),
        stdout_temp,
        stderr_temp,
    )


def _publish_create_new(
    temp_path: Path,
    final_path: Path,
    *,
    platform_name: str | None = None,
    link: Callable[[Path, Path], None] | None = None,
    open_directory: Callable[[Path, int], int] | None = None,
    fsync: Callable[[int], None] | None = None,
    close: Callable[[int], None] | None = None,
    unlink: Callable[[Path], None] | None = None,
    move_file: Callable[[str, str, int], int] | None = None,
    get_last_error: Callable[[], int] | None = None,
) -> None:
    target = os.name if platform_name is None else platform_name
    if target == "nt":
        if move_file is None:
            move_file = ctypes.WinDLL("kernel32", use_last_error=True).MoveFileExW
        if get_last_error is None:
            get_last_error = ctypes.get_last_error
        if not move_file(str(temp_path), str(final_path), 0x8):
            error_code = get_last_error()
            if error_code in {80, 183}:
                raise EvidenceError(
                    f"evidence output already exists: {final_path.name}"
                )
            raise EvidenceError(
                f"create-new publication failed for {final_path.name}: error {error_code}"
            )
        return
    link = os.link if link is None else link
    open_directory = os.open if open_directory is None else open_directory
    fsync = os.fsync if fsync is None else fsync
    close = os.close if close is None else close
    unlink = os.unlink if unlink is None else unlink
    try:
        link(temp_path, final_path)
    except FileExistsError as error:
        raise EvidenceError(
            f"evidence output already exists: {final_path.name}"
        ) from error
    try:
        descriptor = open_directory(
            final_path.parent, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0)
        )
        try:
            fsync(descriptor)
        finally:
            close(descriptor)
    except BaseException:
        try:
            unlink(final_path)
        except OSError:
            pass
        raise
    unlink(temp_path)


def _stream_dict(receipt: StreamReceipt) -> dict[str, object]:
    return {
        "path": receipt.path,
        "sha256": receipt.sha256,
        "bytes": receipt.bytes,
    }


def _write_temp_bytes(final_path: Path, payload: bytes) -> Path:
    temp_path = _temporary_path(final_path)
    descriptor = os.open(temp_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "wb") as output:
        output.write(payload)
        output.flush()
        os.fsync(output.fileno())
    return temp_path


def _fsync_directory(directory: Path) -> None:
    if os.name == "nt":
        return
    descriptor = os.open(directory, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def _remove_temp_files(paths: list[Path]) -> None:
    for path in paths:
        try:
            path.unlink()
        except FileNotFoundError:
            pass


def _same_receipt(path: Path, receipt: StreamReceipt) -> bool:
    try:
        digest, size = _sha256_file(path)
    except OSError:
        return False
    return digest == receipt.sha256 and size == receipt.bytes


def _remove_published(path: Path) -> None:
    if os.name != "nt":
        path.unlink()
        return
    tombstone = _temporary_path(path)
    move_file = ctypes.WinDLL("kernel32", use_last_error=True).MoveFileExW
    if not move_file(str(path), str(tombstone), 0x8):
        raise EvidenceError(
            f"write-through cleanup failed for {path.name}: error {ctypes.get_last_error()}"
        )
    tombstone.unlink()


def _publish_transaction(
    pending: list[_PendingProcess], manifest_path: Path, manifest: dict[str, object]
) -> None:
    manifest_temp = _write_temp_bytes(
        manifest_path, canonical_json_bytes(manifest) + b"\n"
    )
    temporary_paths = [
        path for item in pending for path in (item.stdout_temp, item.stderr_temp)
    ] + [manifest_temp]
    entries = [
        (
            item.stdout_temp,
            manifest_path.parent / item.receipt.stdout.path,
            item.receipt.stdout,
        )
        for item in pending
    ] + [
        (
            item.stderr_temp,
            manifest_path.parent / item.receipt.stderr.path,
            item.receipt.stderr,
        )
        for item in pending
    ]
    entries.sort(key=lambda entry: entry[1].name)
    published: list[tuple[Path, StreamReceipt]] = []
    try:
        for temp_path, final_path, receipt in entries:
            _publish_create_new(temp_path, final_path)
            published.append((final_path, receipt))
        _publish_create_new(manifest_temp, manifest_path)
    except BaseException:
        for final_path, receipt in reversed(published):
            if _same_receipt(final_path, receipt):
                try:
                    _remove_published(final_path)
                except FileNotFoundError:
                    pass
        _fsync_directory(manifest_path.parent)
        _remove_temp_files(temporary_paths)
        raise


def _reject_preexisting(evidence_dir: Path, names: list[str]) -> None:
    for name in names:
        path = evidence_dir / name
        if os.path.lexists(path):
            raise EvidenceError(f"evidence output already exists: {name}")


def _hash_locks(repo_root: Path) -> dict[str, str]:
    return {
        key: _sha256_file(repo_root / relative)[0]
        for key, relative in LOCK_PATHS.items()
    }


def _common_manifest_context(
    repo_root: Path,
    matrix_path: Path,
    recorder_path: Path,
    tested_sha: str,
    tools_sha: str,
    platform_id: str,
) -> dict[str, object]:
    return {
        "platform": platform_id,
        "architecture": platform.machine(),
        "testedGitSha": tested_sha,
        "toolsGitSha": tools_sha,
        "recorderSha256": _sha256_file(recorder_path)[0],
        "matrixSha256": _sha256_file(matrix_path)[0],
        "locks": _hash_locks(repo_root),
    }


def _load_selection_count(path: Path) -> int:
    try:
        parsed = json.loads(path.read_bytes(), object_pairs_hook=_reject_duplicate_keys)
    except (json.JSONDecodeError, UnicodeDecodeError) as error:
        raise EvidenceError("invalid selection JSON") from error
    root = _require_exact_keys(parsed, SELECTION_ROOT_KEYS, "selection result")
    test_count = root["test-count"]
    if type(test_count) is not int:
        raise EvidenceError("selection test-count must be an integer")
    return test_count


def _empty_pending(
    argv: tuple[str, ...], evidence_dir: Path, stdout_name: str, stderr_name: str
) -> _PendingProcess:
    started = canonical_rfc3339_utc_now()
    stdout_temp = _temporary_path(evidence_dir / stdout_name)
    stderr_temp = _temporary_path(evidence_dir / stderr_name)
    stdout_sha, stdout_bytes = _stream_reader(io.BytesIO(b""), stdout_temp)
    stderr_sha, stderr_bytes = _stream_reader(io.BytesIO(b""), stderr_temp)
    ended = canonical_rfc3339_utc_now()
    return _PendingProcess(
        ProcessReceipt(
            argv,
            1,
            started,
            ended,
            StreamReceipt(stdout_name, stdout_sha, stdout_bytes),
            StreamReceipt(stderr_name, stderr_sha, stderr_bytes),
        ),
        stdout_temp,
        stderr_temp,
    )


def _capture_command(
    command: CommandSpec,
    repo_root: Path,
    evidence_dir: Path,
    matrix_path: Path,
    recorder_path: Path,
    env: Mapping[str, str],
    tested_sha: str,
    tools_sha: str,
    platform_id: str,
) -> int:
    selection_command = selection_argv(command)
    names = [
        f"{command.id}.stdout.log",
        f"{command.id}.stderr.log",
        f"{command.id}.manifest.json",
    ]
    if selection_command is not None:
        names.extend(
            [
                f"{command.id}.selection.stdout.log",
                f"{command.id}.selection.stderr.log",
            ]
        )
    _reject_preexisting(evidence_dir, names)
    pending: list[_PendingProcess] = []
    selection_value: dict[str, object] | None = None
    try:
        if selection_command is not None:
            selected = _run_process(
                selection_command,
                repo_root,
                evidence_dir,
                env,
                f"{command.id}.selection.stdout.log",
                f"{command.id}.selection.stderr.log",
            )
            pending.append(selected)
            test_count: int | None = None
            selection_valid = selected.receipt.exit_code == 0
            if selection_valid:
                try:
                    test_count = _load_selection_count(selected.stdout_temp)
                except EvidenceError:
                    selection_valid = False
            selection_value = {
                "argv": list(selected.receipt.argv),
                "exitCode": selected.receipt.exit_code,
                "testCount": test_count,
                "stdout": _stream_dict(selected.receipt.stdout),
                "stderr": _stream_dict(selected.receipt.stderr),
            }
            if not selection_valid or test_count != 1:
                empty = _empty_pending(
                    command.argv,
                    evidence_dir,
                    f"{command.id}.stdout.log",
                    f"{command.id}.stderr.log",
                )
                pending.append(empty)
                manifest = {
                    "schemaVersion": 1,
                    "commandId": command.id,
                    "phase": command.phase,
                    "argv": list(command.argv),
                    "expectedExit": command.expected_exit,
                    "exitCode": 1,
                    "status": "BLOCKED_SELECTION",
                    "startedAt": selected.receipt.started_at,
                    "endedAt": selected.receipt.ended_at,
                    **_common_manifest_context(
                        repo_root,
                        matrix_path,
                        recorder_path,
                        tested_sha,
                        tools_sha,
                        platform_id,
                    ),
                    "stdout": _stream_dict(empty.receipt.stdout),
                    "stderr": _stream_dict(empty.receipt.stderr),
                    "selection": selection_value,
                }
                _publish_transaction(
                    pending,
                    evidence_dir / f"{command.id}.manifest.json",
                    manifest,
                )
                return 1
        result = _run_process(
            command.argv,
            repo_root,
            evidence_dir,
            env,
            f"{command.id}.stdout.log",
            f"{command.id}.stderr.log",
        )
        pending.append(result)
        _require_clean_after(repo_root, env, "tested tree")
        manifest = {
            "schemaVersion": 1,
            "commandId": command.id,
            "phase": command.phase,
            "argv": list(command.argv),
            "expectedExit": command.expected_exit,
            "exitCode": result.receipt.exit_code,
            "status": (
                "PASS"
                if result.receipt.exit_code == command.expected_exit
                else "BLOCKED_BASELINE"
            ),
            "startedAt": result.receipt.started_at,
            "endedAt": result.receipt.ended_at,
            **_common_manifest_context(
                repo_root,
                matrix_path,
                recorder_path,
                tested_sha,
                tools_sha,
                platform_id,
            ),
            "stdout": _stream_dict(result.receipt.stdout),
            "stderr": _stream_dict(result.receipt.stderr),
            "selection": selection_value,
        }
        if set(manifest) != COMMAND_MANIFEST_KEYS:
            raise EvidenceError("internal command manifest key mismatch")
        _publish_transaction(
            pending, evidence_dir / f"{command.id}.manifest.json", manifest
        )
        return result.receipt.exit_code
    except BaseException:
        _remove_temp_files(
            [path for item in pending for path in (item.stdout_temp, item.stderr_temp)]
        )
        raise


@dataclass(frozen=True)
class _ToolProbe:
    argv: tuple[str, ...]
    style: str
    expected: str
    prefix: str | None = None


def _find_bazelisk_package_json(executable: Path) -> Path:
    for directory in (executable.parent, *executable.parents):
        candidate = directory / "package.json"
        if not candidate.is_file() or candidate.is_symlink():
            continue
        try:
            parsed = json.loads(candidate.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        if (
            isinstance(parsed, dict)
            and parsed.get("name") == "@bazel/bazelisk"
            and parsed.get("version") == "v1.28.1"
        ):
            return candidate.resolve(strict=True)
    raise EvidenceError("unable to locate exact Bazelisk package JSON")


def _tool_probes(resolved: Mapping[str, Path]) -> dict[str, _ToolProbe]:
    package_json = _find_bazelisk_package_json(resolved["bazelisk"])
    return {
        "python": _ToolProbe(("python", "--version"), "exact", "Python 3.11.15"),
        "uv": _ToolProbe(("uv", "--version"), "token", "0.11.3", "uv"),
        "git": _ToolProbe(("git", "--version"), "exact", "git version 2.54.0"),
        "just": _ToolProbe(("just", "--version"), "exact", "just 1.58.0"),
        "dotslash": _ToolProbe(("dotslash", "--version"), "exact", "DotSlash 0.5.8"),
        "rustc": _ToolProbe(("rustc", "--version"), "token", "1.95.0", "rustc"),
        "cargo": _ToolProbe(("cargo", "--version"), "token", "1.95.0", "cargo"),
        "cargo-nextest": _ToolProbe(
            ("cargo", "nextest", "--version"),
            "token",
            "0.9.103",
            "cargo-nextest",
        ),
        "cargo-deny": _ToolProbe(
            ("cargo", "deny", "--version"), "token", "0.20.2", "cargo-deny"
        ),
        "node": _ToolProbe(("node", "--version"), "exact", "v26.4.0"),
        "pnpm": _ToolProbe(("pnpm", "--version"), "exact", "10.34.5"),
        "bazelisk": _ToolProbe(
            (
                "node",
                "-p",
                f"require({json.dumps(str(package_json))}).version",
            ),
            "exact",
            "v1.28.1",
        ),
        "bazel": _ToolProbe(("bazel", "--version"), "exact", "bazel 9.0.0"),
    }


def _one_nonempty_output(pending: _PendingProcess) -> str:
    values: list[str] = []
    for path in (pending.stdout_temp, pending.stderr_temp):
        try:
            value = path.read_bytes().decode("ascii").strip()
        except UnicodeDecodeError as error:
            raise EvidenceError("tool version output must be ASCII") from error
        if value:
            values.append(value)
    if len(values) != 1:
        raise EvidenceError("tool version must use exactly one nonempty stream")
    return values[0]


def _validate_tool_probe(
    tool_id: str, probe: _ToolProbe, pending: _PendingProcess
) -> None:
    if pending.receipt.exit_code != 0:
        raise EvidenceError(f"{tool_id} version probe failed")
    output = _one_nonempty_output(pending)
    if probe.style == "exact":
        if output != probe.expected:
            raise EvidenceError(f"{tool_id} version mismatch")
        return
    first_line = next(
        (line.strip() for line in output.splitlines() if line.strip()), ""
    )
    tokens = first_line.split()
    if len(tokens) < 2 or tokens[0] != probe.prefix or tokens[1] != probe.expected:
        raise EvidenceError(f"{tool_id} version mismatch")


def _resolve_exact_tools(env: Mapping[str, str]) -> dict[str, Path]:
    resolved: dict[str, Path] = {}
    seen: set[str] = set()
    for tool in REQUIRED_TOOL_NAMES:
        found = shutil.which(tool, path=env["PATH"])
        if found is None:
            raise EvidenceError(f"required tool is missing: {tool}")
        lexical = Path(found)
        try:
            canonical = lexical.resolve(strict=True)
        except OSError as error:
            raise EvidenceError(f"required tool is invalid: {tool}") from error
        if not canonical.is_file() or (
            os.name != "nt" and not os.access(canonical, os.X_OK)
        ):
            raise EvidenceError(f"required tool is not executable: {tool}")
        identity = os.path.normcase(str(canonical))
        if identity in seen:
            raise EvidenceError(
                f"required tools resolve to duplicate executable: {tool}"
            )
        seen.add(identity)
        resolved[tool] = canonical
    return resolved


def _bootstrap(
    repo_root: Path,
    evidence_dir: Path,
    matrix_path: Path,
    recorder_path: Path,
    env: Mapping[str, str],
    tested_sha: str,
    tools_sha: str,
    platform_id: str,
) -> int:
    log_names = [
        f"{tool}.version.{stream}.log"
        for tool in REQUIRED_TOOL_NAMES
        for stream in ("stdout", "stderr")
    ] + [
        "dependency-install.stdout.log",
        "dependency-install.stderr.log",
        "host-bootstrap.manifest.json",
    ]
    _reject_preexisting(evidence_dir, log_names)
    resolved = _resolve_exact_tools(env)
    probes = _tool_probes(resolved)
    pending: list[_PendingProcess] = []
    tools: dict[str, object] = {}
    try:
        for tool_id in REQUIRED_TOOL_NAMES:
            probe = probes[tool_id]
            result = _run_process(
                probe.argv,
                repo_root,
                evidence_dir,
                env,
                f"{tool_id}.version.stdout.log",
                f"{tool_id}.version.stderr.log",
            )
            pending.append(result)
            _validate_tool_probe(tool_id, probe, result)
            executable_sha, executable_bytes = _sha256_file(resolved[tool_id])
            tools[tool_id] = {
                "versionArgv": list(probe.argv),
                "versionExitCode": result.receipt.exit_code,
                "versionStdout": _stream_dict(result.receipt.stdout),
                "versionStderr": _stream_dict(result.receipt.stderr),
                "executableSha256": executable_sha,
                "executableBytes": executable_bytes,
            }
        dependency = _run_process(
            ("pnpm", "install", "--frozen-lockfile"),
            repo_root,
            evidence_dir,
            env,
            "dependency-install.stdout.log",
            "dependency-install.stderr.log",
        )
        pending.append(dependency)
        if dependency.receipt.exit_code != 0:
            raise EvidenceError("dependency install failed")
        _require_clean_after(repo_root, env, "tested tree")
        administrator_token: bool | None = None
        if os.name == "nt":
            administrator_token = bool(ctypes.windll.shell32.IsUserAnAdmin())
        manifest = {
            "schemaVersion": 1,
            **_common_manifest_context(
                repo_root,
                matrix_path,
                recorder_path,
                tested_sha,
                tools_sha,
                platform_id,
            ),
            "osVersion": platform.mac_ver()[0] or platform.version(),
            "osBuild": platform.version(),
            "tools": tools,
            "dependencyInstall": {
                "argv": list(dependency.receipt.argv),
                "exitCode": dependency.receipt.exit_code,
                "startedAt": dependency.receipt.started_at,
                "endedAt": dependency.receipt.ended_at,
                "stdout": _stream_dict(dependency.receipt.stdout),
                "stderr": _stream_dict(dependency.receipt.stderr),
            },
            "createdAt": canonical_rfc3339_utc_now(),
            "administratorToken": administrator_token,
        }
        _publish_transaction(
            pending, evidence_dir / "host-bootstrap.manifest.json", manifest
        )
        return 0
    except BaseException:
        _remove_temp_files(
            [path for item in pending for path in (item.stdout_temp, item.stderr_temp)]
        )
        raise


def _resolve_request(
    repo_raw: str, evidence_raw: str, matrix_raw: str
) -> tuple[Path, Path, Path, Path, dict[str, str], str, str, str]:
    env = _sanitized_child_environment(os.environ)
    repo_root = require_absolute_directory(repo_raw, "repo-root")
    evidence_dir = require_absolute_directory(evidence_raw, "evidence-dir")
    matrix_path = _require_absolute_file(matrix_raw, "matrix")
    recorder_lexical = Path(__file__)
    if not recorder_lexical.is_absolute() or _path_has_symlink_component(
        recorder_lexical
    ):
        raise EvidenceError("recorder must be a non-symlink absolute file")
    recorder_path = recorder_lexical.resolve(strict=True)
    tools_root = Path(
        _git_text(
            recorder_path.parent,
            env,
            "rev-parse",
            "--show-toplevel",
        )
    ).resolve(strict=True)
    for left, right in (
        (repo_root, evidence_dir),
        (repo_root, tools_root),
        (evidence_dir, tools_root),
    ):
        if not paths_are_disjoint(left, right):
            raise EvidenceError("tested, tools, and evidence trees must be disjoint")
    try:
        matrix_relative = matrix_path.relative_to(tools_root).as_posix()
    except ValueError as error:
        raise EvidenceError("matrix must be inside the tools repository") from error
    tested_sha = _require_safe_git_repository(
        repo_root, env, require_locks=True, label="tested tree"
    )
    tools_sha = _require_safe_git_repository(
        tools_root, env, require_locks=False, label="tools tree"
    )
    _require_tracked_blob(tools_root, matrix_relative, env, "matrix")
    platform_id = host_id(
        "/usr/sbin/sysctl" if platform.system() == "Darwin" else "sysctl"
    )
    return (
        repo_root,
        evidence_dir,
        matrix_path,
        recorder_path,
        env,
        tested_sha,
        tools_sha,
        platform_id,
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", required=True)
    parser.add_argument("--evidence-dir", required=True)
    parser.add_argument("--matrix", required=True)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--name")
    mode.add_argument("--bootstrap", action="store_true")
    arguments = parser.parse_args(argv)
    try:
        (
            repo_root,
            evidence_dir,
            matrix_path,
            recorder_path,
            env,
            tested_sha,
            tools_sha,
            platform_id,
        ) = _resolve_request(
            arguments.repo_root, arguments.evidence_dir, arguments.matrix
        )
        matrix = load_matrix(matrix_path)
        if arguments.bootstrap:
            return _bootstrap(
                repo_root,
                evidence_dir,
                matrix_path,
                recorder_path,
                env,
                tested_sha,
                tools_sha,
                platform_id,
            )
        matching = [
            command for command in matrix.commands if command.id == arguments.name
        ]
        if len(matching) != 1:
            raise EvidenceError(f"unknown command: {arguments.name}")
        command = matching[0]
        if platform_id not in command.platforms:
            raise EvidenceError(f"command is not valid for {platform_id}: {command.id}")
        return _capture_command(
            command,
            repo_root,
            evidence_dir,
            matrix_path,
            recorder_path,
            env,
            tested_sha,
            tools_sha,
            platform_id,
        )
    except EvidenceError as error:
        print(f"capture_command: {error}", file=sys.stderr)
        return 1
    except OSError as error:
        print(
            f"capture_command: operating-system failure: {error.errno}", file=sys.stderr
        )
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
