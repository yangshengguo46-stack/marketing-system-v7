#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
import re
import stat
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Mapping

import capture_command as capture


EvidenceError = capture.EvidenceError
CHUNK_SIZE = 1024 * 1024
SHA_256_PATTERN = re.compile(r"[0-9a-f]{64}")
SHA_1_PATTERN = re.compile(r"[0-9a-f]{40}")
SAFE_BASENAME_PATTERN = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,254}")
FORBIDDEN_BYTE_RULES = {
    "unix-user-home": rb"/(?:Users|home)/[^/\x00-\x20]+/",
    "windows-user-home": rb"(?i:[A-Z]:\\Users\\[^\\\x00-\x20]+\\)",
    "authorization-header": rb"(?i:authorization\s*:\s*[^\s]+)",
    "bearer-token": rb"(?i:\bbearer\s+[A-Za-z0-9._~+/=-]{8,})",
    "credential-assignment": rb"(?i:(?:api[_-]?key|access[_-]?token|secret[_-]?key)\s*[:=]\s*[^\s,;]+)",
    "codex-auth-path": rb"(?i:(?:\.codex[/\\])?auth\.json)",
    "private-json-body-key": rb'"(?:prompt|responseBody|caseBody|reviewerMapping|privateCase)"\s*:',
}
LOCK_PATHS = {
    "cargoLockSha256": "codex-rs/Cargo.lock",
    "pnpmLockSha256": "pnpm-lock.yaml",
    "bazelLockSha256": "MODULE.bazel.lock",
}
PLATFORM_ARCHITECTURES = {
    "macos-x86_64": "x86_64",
    "windows-11-x64": "AMD64",
}
STREAM_KEYS = {"path", "sha256", "bytes"}
CONTEXT_KEYS = {
    "platform",
    "architecture",
    "testedGitSha",
    "toolsGitSha",
    "recorderSha256",
    "matrixSha256",
    "locks",
}
BOOTSTRAP_KEYS = {
    "schemaVersion",
    *CONTEXT_KEYS,
    "osVersion",
    "osBuild",
    "tools",
    "dependencyInstall",
    "createdAt",
    "administratorToken",
}
TOOL_KEYS = {
    "versionArgv",
    "versionExitCode",
    "versionStdout",
    "versionStderr",
    "executableSha256",
    "executableBytes",
}
DEPENDENCY_KEYS = {
    "argv",
    "exitCode",
    "startedAt",
    "endedAt",
    "stdout",
    "stderr",
}
COMMAND_KEYS = {
    "schemaVersion",
    "commandId",
    "phase",
    "argv",
    "expectedExit",
    "exitCode",
    "status",
    "startedAt",
    "endedAt",
    *CONTEXT_KEYS,
    "stdout",
    "stderr",
    "selection",
}
SELECTION_KEYS = {"argv", "exitCode", "testCount", "stdout", "stderr"}
SUMMARY_KEYS = {
    "schemaVersion",
    "platform",
    "mode",
    "testedGitSha",
    "toolsGitSha",
    "matrixSha256",
    "commandIds",
    "blockedIds",
    "status",
}
CAPABILITY_STATUS_KEYS = {
    "codePresent",
    "mechanicalContracts",
    "liveProviderReachable",
    "businessBlindReview",
    "publicationRetro",
}
REPORT_KEYS = {
    "schemaVersion",
    "publicRunId",
    "decision",
    "forkSha",
    "codexBinarySha256",
    "evaluatorBinarySha256",
    "brokerComponentSha256",
    "modelLabel",
    "providerLabel",
    "providerCompatibilityName",
    "providerRole",
    "frozenRunContextCommitment",
    "executionContextCommitment",
    "attemptIndexRootCommitment",
    "providerEndpointCommitment",
    "privateCaseCommitment",
    "privateMaterialsCommitment",
    "sharedConfigSha256",
    "promptSha256",
    "additionalContextCommitment",
    "outputSchemaSha256",
    "normalizedThreadStartSha256",
    "normalizedTurnStartCommitment",
    "genericCatalogSha256",
    "candidateCatalogSha256",
    "candidateSkillUseVerified",
    "skillUseEvidenceCommitment",
    "pairManifestsCommitment",
    "attestationCommitment",
    "armOrderCommitment",
    "rateCardSha256",
    "reviewSubmissionsCommitment",
    "proofRootSha256",
    "rubricSha256",
    "reviewerCount",
    "experiencedOperatorOrDirectorCount",
    "candidatePreferenceCount",
    "medianGenericScore",
    "medianCandidateScore",
    "medianPairedDelta",
    "candidateSevereFailureCount",
    "genericUsage",
    "candidateUsage",
    "genericCostFen",
    "candidateCostFen",
    "providerRequestAttemptCounts",
    "providerCompletedResponseCounts",
    "usageScope",
    "privateEvidenceRetentionDeadline",
    "capabilityStatus",
    "retentionStatus",
    "sourceMaterialRetention",
    "retentionCloseoutReceiptPath",
    "generatedAt",
}
INDEX_KEYS = {"schemaVersion", "attempts"}
ATTEMPT_KEYS = {
    "attemptOrdinal",
    "publicRunId",
    "reportPath",
    "reportSha256",
    "candidateSha",
    "decision",
    "supersedes",
    "candidateDiffSha256",
    "caseCommitment",
    "materialsCommitment",
    "selectedForCheckpoint",
}
BUSINESS_RECEIPT_KEYS = {
    "schemaVersion",
    "publicRunId",
    "candidateSha",
    "reportPath",
    "reportSha256",
    "reportIndexSha256",
    "selectedAttemptOrdinal",
    "proofRootSha256",
    "frozenRunContextCommitment",
    "executionContextCommitment",
    "attemptIndexRootCommitment",
    "brokerReceiptCommitment",
    "costReceiptsCommitment",
    "candidateAllowedDiffSha256",
    "G2",
    "capabilityStatus",
    "verifiedAt",
    "verifiedBeforeRetentionDeadline",
}
FINAL_OUTPUT_KEYS = {
    "schemaVersion",
    "publicRunId",
    "candidateSha",
    "reportPath",
    "reportSha256",
    "reportIndexSha256",
    "selectedAttemptOrdinal",
    "proofRootSha256",
    "frozenRunContextCommitment",
    "executionContextCommitment",
    "attemptIndexRootCommitment",
    "brokerReceiptCommitment",
    "costReceiptsCommitment",
    "requiredMatrixSha256",
    "macPostEvidenceCommitment",
    "windowsPostEvidenceCommitment",
    "candidateAllowedDiffSha256",
    "liveProofVerificationSha256",
    "G0",
    "G1",
    "G2",
    "capabilityStatus",
    "foundationDecision",
    "verifierSha256",
    "verifiedAt",
    "verifiedBeforeRetentionDeadline",
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


@dataclass(frozen=True, order=True)
class ForbiddenMatch:
    path: str
    rule_id: str


class ForbiddenEvidenceError(EvidenceError):
    def __init__(self, matches: tuple[ForbiddenMatch, ...]) -> None:
        super().__init__("forbidden public evidence content")
        self.matches = matches


@dataclass(frozen=True)
class EvidenceDisposition:
    platform: str
    mode: str
    tested_git_sha: str
    tools_git_sha: str
    matrix_sha256: str
    command_ids: tuple[str, ...]
    blocked_ids: tuple[str, ...]

    @property
    def status(self) -> str:
        return "PASS" if not self.blocked_ids else "BLOCKED_BASELINE"


@dataclass(frozen=True)
class VerificationRequest:
    repo_root: Path
    matrix_path: Path
    evidence_root: Path
    platform: str
    mode: str


@dataclass(frozen=True)
class FinalVerificationRequest:
    repo_root: Path
    matrix_path: Path
    mac_post_root: Path
    windows_post_root: Path
    candidate_sha: str
    selected_report: Path
    report_index: Path
    business_verification_receipt: Path
    verification_output: Path


@dataclass(frozen=True)
class _GitBindings:
    tested_sha: str
    tools_sha: str
    recorder_sha256: str
    matrix_raw_sha256: str
    matrix_semantic_sha256: str
    locks: Mapping[str, str]


def canonical_json_bytes(value: object) -> bytes:
    try:
        return json.dumps(
            value,
            ensure_ascii=False,
            sort_keys=True,
            separators=(",", ":"),
            allow_nan=False,
        ).encode("utf-8")
    except (TypeError, ValueError) as error:
        raise EvidenceError("value is not canonical JSON") from error


def _reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise EvidenceError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _reject_json_constant(value: str) -> object:
    raise EvidenceError(f"non-finite JSON number: {value}")


def _load_json(path: Path, label: str, *, canonical: bool = True) -> dict[str, object]:
    try:
        payload = _stable_file_bytes(path)
        parsed = json.loads(
            payload,
            object_pairs_hook=_reject_duplicate_keys,
            parse_constant=_reject_json_constant,
        )
    except (json.JSONDecodeError, UnicodeDecodeError) as error:
        raise EvidenceError(f"invalid {label} JSON") from error
    if not isinstance(parsed, dict):
        raise EvidenceError(f"{label} must be an object")
    if canonical and payload != canonical_json_bytes(parsed) + b"\n":
        raise EvidenceError(f"{label} must use canonical JSON plus LF")
    return parsed


def _exact_object(value: object, keys: set[str], label: str) -> dict[str, object]:
    if not isinstance(value, dict):
        raise EvidenceError(f"{label} must be an object")
    actual = set(value)
    if actual != keys:
        raise EvidenceError(f"{label} keys differ")
    return value


def _integer(value: object, label: str, *, minimum: int | None = None) -> int:
    if type(value) is not int:
        raise EvidenceError(f"{label} must be an integer")
    if minimum is not None and value < minimum:
        raise EvidenceError(f"{label} is below its minimum")
    return value


def _string(value: object, label: str) -> str:
    if not isinstance(value, str) or not value:
        raise EvidenceError(f"{label} must be a non-empty string")
    return value


def _digest(value: object, label: str) -> str:
    text = _string(value, label)
    if SHA_256_PATTERN.fullmatch(text) is None:
        raise EvidenceError(f"{label} must be lowercase 64-hex")
    return text


def _git_sha(value: object, label: str) -> str:
    text = _string(value, label)
    if SHA_1_PATTERN.fullmatch(text) is None:
        raise EvidenceError(f"{label} must be lowercase 40-hex")
    return text


def _safe_basename(value: object, label: str) -> str:
    name = _string(value, label)
    if (
        SAFE_BASENAME_PATTERN.fullmatch(name) is None
        or name in {".", ".."}
        or Path(name).name != name
        or "\\" in name
        or ":" in name
    ):
        raise EvidenceError(f"{label} must be a safe basename")
    return name


def _safe_relative_path(value: object, label: str) -> str:
    text = _string(value, label)
    path = Path(text)
    if path.is_absolute() or "\\" in text or ":" in text or path.as_posix() != text:
        raise EvidenceError(f"{label} must be a safe relative path")
    if not path.parts or any(
        part in {".", ".."} or SAFE_BASENAME_PATTERN.fullmatch(part) is None
        for part in path.parts
    ):
        raise EvidenceError(f"{label} must be a safe relative path")
    return text


def _rfc3339_utc(value: object, label: str) -> datetime:
    text = _string(value, label)
    if not text.endswith("Z"):
        raise EvidenceError(f"{label} must be RFC3339 UTC")
    try:
        parsed = datetime.fromisoformat(text[:-1] + "+00:00")
    except ValueError as error:
        raise EvidenceError(f"{label} must be RFC3339 UTC") from error
    if parsed.tzinfo is None or parsed.utcoffset() != timezone.utc.utcoffset(parsed):
        raise EvidenceError(f"{label} must be RFC3339 UTC")
    return parsed


def _path_has_symlink_component(path: Path) -> bool:
    current = Path(path.anchor)
    for part in path.parts[1:]:
        current /= part
        if current.is_symlink():
            return True
    return False


def _absolute_directory(path: Path, label: str) -> Path:
    lexical = Path(path)
    if not lexical.is_absolute() or _path_has_symlink_component(lexical):
        raise EvidenceError(f"{label} must be an absolute non-symlink directory")
    try:
        resolved = lexical.resolve(strict=True)
    except OSError as error:
        raise EvidenceError(f"{label} must exist") from error
    if not resolved.is_dir():
        raise EvidenceError(f"{label} must be a directory")
    return resolved


def _absolute_file(path: Path, label: str) -> Path:
    lexical = Path(path)
    if not lexical.is_absolute() or _path_has_symlink_component(lexical):
        raise EvidenceError(f"{label} must be an absolute non-symlink file")
    try:
        resolved = lexical.resolve(strict=True)
    except OSError as error:
        raise EvidenceError(f"{label} must exist") from error
    if not resolved.is_file():
        raise EvidenceError(f"{label} must be a file")
    return resolved


def _same_file_state(left: os.stat_result, right: os.stat_result) -> bool:
    return (
        left.st_dev,
        left.st_ino,
        left.st_mode,
        left.st_nlink,
        left.st_size,
        left.st_mtime_ns,
    ) == (
        right.st_dev,
        right.st_ino,
        right.st_mode,
        right.st_nlink,
        right.st_size,
        right.st_mtime_ns,
    )


def _open_stable_regular(path: Path) -> tuple[int, os.stat_result]:
    try:
        before = path.stat(follow_symlinks=False)
    except OSError as error:
        raise EvidenceError(
            f"unable to inspect public evidence file: {path.name}"
        ) from error
    if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
        raise EvidenceError(f"unsafe public evidence file: {path.name}")
    if getattr(before, "st_file_attributes", 0) & 0x400:
        raise EvidenceError(f"reparse public evidence file: {path.name}")
    flags = os.O_RDONLY | getattr(os, "O_BINARY", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError as error:
        raise EvidenceError(
            f"unable to open public evidence file: {path.name}"
        ) from error
    opened = os.fstat(descriptor)
    if not _same_file_state(before, opened):
        os.close(descriptor)
        raise EvidenceError(f"public evidence file changed before scan: {path.name}")
    return descriptor, opened


def _stable_file_bytes(path: Path) -> bytes:
    descriptor, before = _open_stable_regular(path)
    chunks: list[bytes] = []
    try:
        while True:
            chunk = os.read(descriptor, CHUNK_SIZE)
            if not chunk:
                break
            chunks.append(chunk)
        after = os.fstat(descriptor)
    finally:
        os.close(descriptor)
    try:
        final = path.stat(follow_symlinks=False)
    except OSError as error:
        raise EvidenceError(
            f"public evidence file changed after read: {path.name}"
        ) from error
    if not _same_file_state(before, after) or not _same_file_state(before, final):
        raise EvidenceError(f"public evidence file changed while read: {path.name}")
    payload = b"".join(chunks)
    if len(payload) != before.st_size:
        raise EvidenceError(
            f"public evidence file size changed while read: {path.name}"
        )
    return payload


def _scan_one(path: Path, display_path: str) -> tuple[ForbiddenMatch, ...]:
    descriptor, before = _open_stable_regular(path)
    found: set[ForbiddenMatch] = set()
    overlap = b""
    total = 0
    try:
        while True:
            chunk = os.read(descriptor, CHUNK_SIZE)
            if not chunk:
                break
            total += len(chunk)
            window = overlap + chunk
            for rule_id, pattern in FORBIDDEN_BYTE_RULES.items():
                if re.search(pattern, window) is not None:
                    found.add(ForbiddenMatch(display_path, rule_id))
            overlap = window[-CHUNK_SIZE:]
        after = os.fstat(descriptor)
    finally:
        os.close(descriptor)
    try:
        final = path.stat(follow_symlinks=False)
    except OSError as error:
        raise EvidenceError(
            f"public evidence file changed after scan: {display_path}"
        ) from error
    if (
        total != before.st_size
        or not _same_file_state(before, after)
        or not _same_file_state(before, final)
    ):
        raise EvidenceError(
            f"public evidence file changed while scanned: {display_path}"
        )
    return tuple(sorted(found))


def scan_forbidden_evidence(root: Path) -> tuple[ForbiddenMatch, ...]:
    canonical = _absolute_directory(Path(root), "evidence root")
    matches: set[ForbiddenMatch] = set()
    pending = [canonical]
    while pending:
        directory = pending.pop()
        try:
            entries = sorted(os.scandir(directory), key=lambda entry: entry.name)
        except OSError as error:
            raise EvidenceError("unable to enumerate public evidence") from error
        for entry in entries:
            relative = Path(entry.path).relative_to(canonical)
            display = relative.as_posix()
            for part in relative.parts:
                _safe_basename(part, "public evidence path component")
            try:
                metadata = entry.stat(follow_symlinks=False)
            except OSError as error:
                raise EvidenceError(
                    f"unable to inspect public evidence: {display}"
                ) from error
            if (
                stat.S_ISLNK(metadata.st_mode)
                or getattr(metadata, "st_file_attributes", 0) & 0x400
            ):
                raise EvidenceError(f"unsafe public evidence entry: {display}")
            if stat.S_ISDIR(metadata.st_mode):
                pending.append(Path(entry.path))
            elif stat.S_ISREG(metadata.st_mode):
                if metadata.st_nlink != 1:
                    raise EvidenceError(f"hardlinked public evidence file: {display}")
                matches.update(_scan_one(Path(entry.path), display))
            else:
                raise EvidenceError(f"special public evidence file: {display}")
    return tuple(sorted(matches))


def _scan_consumed_file(path: Path, display: str) -> None:
    matches = _scan_one(path, display)
    if matches:
        raise ForbiddenEvidenceError(matches)


def _git_environment() -> dict[str, str]:
    environment = {
        key: value
        for key, value in os.environ.items()
        if not key.upper().startswith("GIT_")
    }
    environment.update(HARDENED_GIT_ENV)
    return environment


def _run_git(repo: Path, *args: str) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        ["git", "-C", str(repo), *args],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=_git_environment(),
    )


def _git_bytes(repo: Path, *args: str) -> bytes:
    completed = _run_git(repo, *args)
    if completed.returncode != 0:
        raise EvidenceError(f"git {' '.join(args)} failed")
    return completed.stdout


def _git_text(repo: Path, *args: str) -> str:
    try:
        return _git_bytes(repo, *args).decode("utf-8").strip()
    except UnicodeDecodeError as error:
        raise EvidenceError("Git returned non-UTF-8 metadata") from error


def _git_root(path: Path, label: str) -> Path:
    root = Path(_git_text(path, "rev-parse", "--show-toplevel")).resolve(strict=True)
    if not root.is_dir():
        raise EvidenceError(f"{label} Git root is invalid")
    return root


def _reject_git_substitution(repo: Path) -> None:
    if _git_text(repo, "rev-parse", "--is-shallow-repository") != "false":
        raise EvidenceError("shallow Git repositories are forbidden")
    if _git_text(repo, "for-each-ref", "--format=%(refname)", "refs/replace/"):
        raise EvidenceError("Git replace refs are forbidden")
    git_dir = Path(_git_text(repo, "rev-parse", "--path-format=absolute", "--git-dir"))
    common = Path(
        _git_text(repo, "rev-parse", "--path-format=absolute", "--git-common-dir")
    )
    git_path = Path(
        _git_text(
            repo,
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "info/grafts",
        )
    )
    if any(
        os.path.lexists(path)
        for path in {git_dir / "info/grafts", common / "info/grafts", git_path}
    ):
        raise EvidenceError("Git grafts are forbidden")
    promisor = _run_git(
        repo,
        "config",
        "--get-regexp",
        r"^(remote\..*\.promisor|extensions\.partialClone)$",
    )
    if promisor.returncode not in {0, 1} or (
        promisor.returncode == 0 and promisor.stdout.strip()
    ):
        raise EvidenceError("Git promisor state is forbidden")


def _require_commit(repo: Path, sha: object, label: str) -> str:
    value = _git_sha(sha, label)
    if _git_text(repo, "cat-file", "-t", value) != "commit":
        raise EvidenceError(f"{label} is not a real commit")
    return value


def _commit_blob(repo: Path, sha: str, relative: str, label: str) -> bytes:
    _safe_relative_path(relative, label)
    entry = _git_text(repo, "ls-tree", sha, "--", relative).split()
    if len(entry) < 4 or entry[0] != "100644" or entry[1] != "blob":
        raise EvidenceError(f"{label} is not a regular committed blob")
    return _git_bytes(repo, "cat-file", "blob", f"{sha}:{relative}")


def _sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def _bindings(
    repo_root: Path,
    matrix_path: Path,
    tested_sha_value: object,
    tools_sha_value: object,
) -> tuple[_GitBindings, capture.Matrix]:
    repo = _absolute_directory(repo_root, "repo root")
    matrix = _absolute_file(matrix_path, "matrix")
    tested_repo = _git_root(repo, "tested")
    if tested_repo != repo:
        raise EvidenceError("repo root must be a Git top-level")
    tools_repo = _git_root(matrix.parent, "tools")
    _reject_git_substitution(tested_repo)
    if tools_repo != tested_repo:
        _reject_git_substitution(tools_repo)
    tested_sha = _require_commit(tested_repo, tested_sha_value, "testedGitSha")
    tools_sha = _require_commit(tools_repo, tools_sha_value, "toolsGitSha")
    try:
        matrix_relative = matrix.relative_to(tools_repo).as_posix()
    except ValueError as error:
        raise EvidenceError("matrix is outside its tools repository") from error
    matrix_blob = _commit_blob(tools_repo, tools_sha, matrix_relative, "matrix")
    current_matrix = _stable_file_bytes(matrix)
    if current_matrix != matrix_blob:
        raise EvidenceError("matrix differs from the tools commit")
    recorder_blob = _commit_blob(
        tools_repo,
        tools_sha,
        "scripts/ai_ip/foundation/capture_command.py",
        "recorder",
    )
    loaded_matrix = capture.load_matrix(matrix)
    parsed = json.loads(
        matrix_blob,
        object_pairs_hook=_reject_duplicate_keys,
        parse_constant=_reject_json_constant,
    )
    semantic_sha = _sha256(canonical_json_bytes(parsed))
    locks = {
        key: _sha256(_commit_blob(tested_repo, tested_sha, relative, relative))
        for key, relative in LOCK_PATHS.items()
    }
    return (
        _GitBindings(
            tested_sha,
            tools_sha,
            _sha256(recorder_blob),
            _sha256(matrix_blob),
            semantic_sha,
            locks,
        ),
        loaded_matrix,
    )


def _validate_stream(
    root: Path,
    value: object,
    expected_name: str,
    expected_files: set[str],
    label: str,
) -> None:
    stream = _exact_object(value, STREAM_KEYS, label)
    name = _safe_basename(stream["path"], f"{label} path")
    if name != expected_name:
        raise EvidenceError(f"{label} path differs")
    digest = _digest(stream["sha256"], f"{label} sha256")
    byte_count = _integer(stream["bytes"], f"{label} bytes", minimum=0)
    path = root / name
    payload = _stable_file_bytes(path)
    if len(payload) != byte_count or _sha256(payload) != digest:
        raise EvidenceError(f"{label} receipt differs from log")
    expected_files.add(name)


def _validate_context(
    value: Mapping[str, object], bindings: _GitBindings, platform_id: str, label: str
) -> None:
    if value["platform"] != platform_id:
        raise EvidenceError(f"{label} platform differs")
    if value["architecture"] != PLATFORM_ARCHITECTURES[platform_id]:
        raise EvidenceError(f"{label} architecture differs")
    if value["testedGitSha"] != bindings.tested_sha:
        raise EvidenceError(f"{label} testedGitSha differs")
    if value["toolsGitSha"] != bindings.tools_sha:
        raise EvidenceError(f"{label} toolsGitSha differs")
    if value["recorderSha256"] != bindings.recorder_sha256:
        raise EvidenceError(f"{label} recorderSha256 differs")
    if value["matrixSha256"] != bindings.matrix_raw_sha256:
        raise EvidenceError(f"{label} matrixSha256 differs")
    locks = _exact_object(value["locks"], set(LOCK_PATHS), f"{label} locks")
    if locks != dict(bindings.locks):
        raise EvidenceError(f"{label} locks differ")


def _validate_bootstrap(
    root: Path,
    value: dict[str, object],
    bindings: _GitBindings,
    platform_id: str,
    expected_files: set[str],
) -> None:
    bootstrap = _exact_object(value, BOOTSTRAP_KEYS, "bootstrap manifest")
    if bootstrap["schemaVersion"] != 1:
        raise EvidenceError("bootstrap schemaVersion differs")
    _validate_context(bootstrap, bindings, platform_id, "bootstrap")
    _string(bootstrap["osVersion"], "bootstrap osVersion")
    _string(bootstrap["osBuild"], "bootstrap osBuild")
    _rfc3339_utc(bootstrap["createdAt"], "bootstrap createdAt")
    if platform_id == "macos-x86_64" and bootstrap["administratorToken"] is not None:
        raise EvidenceError("macOS administratorToken must be null")
    if (
        platform_id == "windows-11-x64"
        and type(bootstrap["administratorToken"]) is not bool
    ):
        raise EvidenceError("Windows administratorToken must be boolean")
    tools = _exact_object(
        bootstrap["tools"], set(capture.REQUIRED_TOOL_NAMES), "bootstrap tools"
    )
    for tool_name in capture.REQUIRED_TOOL_NAMES:
        tool = _exact_object(tools[tool_name], TOOL_KEYS, f"{tool_name} tool")
        argv = tool["versionArgv"]
        if (
            not isinstance(argv, list)
            or not argv
            or any(not isinstance(item, str) or not item for item in argv)
        ):
            raise EvidenceError(f"{tool_name} versionArgv is invalid")
        if tool["versionExitCode"] != 0:
            raise EvidenceError(f"{tool_name} version probe did not pass")
        _digest(tool["executableSha256"], f"{tool_name} executableSha256")
        _integer(tool["executableBytes"], f"{tool_name} executableBytes", minimum=1)
        _validate_stream(
            root,
            tool["versionStdout"],
            f"{tool_name}.version.stdout.log",
            expected_files,
            f"{tool_name} version stdout",
        )
        _validate_stream(
            root,
            tool["versionStderr"],
            f"{tool_name}.version.stderr.log",
            expected_files,
            f"{tool_name} version stderr",
        )
    dependency = _exact_object(
        bootstrap["dependencyInstall"], DEPENDENCY_KEYS, "dependency install"
    )
    if dependency["argv"] != ["pnpm", "install", "--frozen-lockfile"]:
        raise EvidenceError("dependency install argv differs")
    if dependency["exitCode"] != 0:
        raise EvidenceError("dependency install did not pass")
    started = _rfc3339_utc(dependency["startedAt"], "dependency startedAt")
    ended = _rfc3339_utc(dependency["endedAt"], "dependency endedAt")
    if started > ended:
        raise EvidenceError("dependency timestamps are inverted")
    _validate_stream(
        root,
        dependency["stdout"],
        "dependency-install.stdout.log",
        expected_files,
        "dependency stdout",
    )
    _validate_stream(
        root,
        dependency["stderr"],
        "dependency-install.stderr.log",
        expected_files,
        "dependency stderr",
    )


def _validate_command(
    root: Path,
    manifest: dict[str, object],
    command: capture.CommandSpec,
    bindings: _GitBindings,
    platform_id: str,
    expected_files: set[str],
) -> bool:
    value = _exact_object(manifest, COMMAND_KEYS, f"{command.id} manifest")
    if value["schemaVersion"] != 1:
        raise EvidenceError(f"{command.id} schemaVersion differs")
    if value["commandId"] != command.id or value["phase"] != command.phase:
        raise EvidenceError(f"{command.id} matrix identity differs")
    if (
        value["argv"] != list(command.argv)
        or value["expectedExit"] != command.expected_exit
    ):
        raise EvidenceError(f"{command.id} matrix command differs")
    _validate_context(value, bindings, platform_id, command.id)
    started = _rfc3339_utc(value["startedAt"], f"{command.id} startedAt")
    ended = _rfc3339_utc(value["endedAt"], f"{command.id} endedAt")
    if started > ended:
        raise EvidenceError(f"{command.id} timestamps are inverted")
    exit_code = _integer(value["exitCode"], f"{command.id} exitCode")
    _validate_stream(
        root,
        value["stdout"],
        f"{command.id}.stdout.log",
        expected_files,
        f"{command.id} stdout",
    )
    _validate_stream(
        root,
        value["stderr"],
        f"{command.id}.stderr.log",
        expected_files,
        f"{command.id} stderr",
    )
    expected_selection = capture.selection_argv(command)
    selection_valid = True
    if expected_selection is None:
        if value["selection"] is not None:
            raise EvidenceError(f"{command.id} has unexpected selection evidence")
    else:
        selection = _exact_object(
            value["selection"], SELECTION_KEYS, f"{command.id} selection"
        )
        if selection["argv"] != list(expected_selection):
            raise EvidenceError(f"{command.id} selection argv differs")
        selection_exit = _integer(
            selection["exitCode"], f"{command.id} selection exitCode"
        )
        test_count = selection["testCount"]
        if test_count is not None:
            _integer(test_count, f"{command.id} selection testCount", minimum=0)
        selection_valid = selection_exit == 0 and test_count == 1
        _validate_stream(
            root,
            selection["stdout"],
            f"{command.id}.selection.stdout.log",
            expected_files,
            f"{command.id} selection stdout",
        )
        _validate_stream(
            root,
            selection["stderr"],
            f"{command.id}.selection.stderr.log",
            expected_files,
            f"{command.id} selection stderr",
        )
    status_value = value["status"]
    if status_value == "PASS":
        if exit_code != command.expected_exit or not selection_valid:
            raise EvidenceError(f"{command.id} PASS status is inconsistent")
        return False
    if status_value == "BLOCKED_BASELINE":
        if exit_code == command.expected_exit or not selection_valid:
            raise EvidenceError(f"{command.id} blocked status is inconsistent")
        return True
    if status_value == "BLOCKED_SELECTION":
        if expected_selection is None or selection_valid or exit_code != 1:
            raise EvidenceError(f"{command.id} selection status is inconsistent")
        return True
    raise EvidenceError(f"{command.id} has unknown status")


def disposition_dict(disposition: EvidenceDisposition) -> dict[str, object]:
    return {
        "schemaVersion": 1,
        "platform": disposition.platform,
        "mode": disposition.mode,
        "testedGitSha": disposition.tested_git_sha,
        "toolsGitSha": disposition.tools_git_sha,
        "matrixSha256": disposition.matrix_sha256,
        "commandIds": list(disposition.command_ids),
        "blockedIds": list(disposition.blocked_ids),
        "status": disposition.status,
    }


def _load_baseline_disposition(
    request: VerificationRequest, disposition: EvidenceDisposition
) -> set[str]:
    candidates = [
        request.evidence_root / "baseline-summary.json",
        request.evidence_root.parent / "baseline/baseline-summary.json",
        request.repo_root
        / "docs/evidence/foundation"
        / request.platform
        / "baseline-summary.json",
    ]
    existing: list[Path] = []
    for candidate in candidates:
        try:
            resolved = candidate.resolve(strict=True)
        except OSError:
            continue
        if resolved not in existing:
            existing.append(resolved)
    if not existing:
        return set()
    values = [_load_json(path, "baseline summary") for path in existing]
    first = values[0]
    if any(value != first for value in values[1:]):
        raise EvidenceError("multiple baseline summaries differ")
    summary = _exact_object(first, SUMMARY_KEYS, "baseline summary")
    if (
        summary["schemaVersion"] != 1
        or summary["platform"] != disposition.platform
        or summary["mode"] != "baseline"
        or summary["testedGitSha"] != disposition.tested_git_sha
        or summary["toolsGitSha"] != disposition.tools_git_sha
        or summary["matrixSha256"] != disposition.matrix_sha256
    ):
        raise EvidenceError("baseline summary binding differs")
    command_ids = summary["commandIds"]
    blocked_ids = summary["blockedIds"]
    if not isinstance(command_ids, list) or not isinstance(blocked_ids, list):
        raise EvidenceError("baseline summary IDs must be arrays")
    if any(not isinstance(item, str) for item in command_ids + blocked_ids):
        raise EvidenceError("baseline summary IDs must be strings")
    if len(set(command_ids)) != len(command_ids) or len(set(blocked_ids)) != len(
        blocked_ids
    ):
        raise EvidenceError("baseline summary IDs must be unique")
    if not set(blocked_ids).issubset(set(command_ids)):
        raise EvidenceError("baseline blocked IDs are unknown")
    expected_status = "PASS" if not blocked_ids else "BLOCKED_BASELINE"
    if summary["status"] != expected_status:
        raise EvidenceError("baseline summary status differs")
    return set(blocked_ids)


def verify_evidence(request: VerificationRequest) -> EvidenceDisposition:
    if request.platform not in PLATFORM_ARCHITECTURES:
        raise EvidenceError("unknown platform")
    if request.mode not in {"baseline", "post"}:
        raise EvidenceError("unknown evidence mode")
    root = _absolute_directory(request.evidence_root, "evidence root")
    try:
        matches = scan_forbidden_evidence(root)
    except ForbiddenEvidenceError:
        raise
    except EvidenceError as error:
        raise ForbiddenEvidenceError(()) from error
    if matches:
        raise ForbiddenEvidenceError(matches)
    bootstrap_path = root / "host-bootstrap.manifest.json"
    bootstrap = _load_json(bootstrap_path, "bootstrap manifest")
    tested_sha = bootstrap.get("testedGitSha")
    tools_sha = bootstrap.get("toolsGitSha")
    bindings, matrix = _bindings(
        request.repo_root, request.matrix_path, tested_sha, tools_sha
    )
    commands = matrix.required_for(request.platform, request.mode)
    if not commands:
        raise EvidenceError("matrix has no required commands")
    expected_files = {"host-bootstrap.manifest.json"}
    _validate_bootstrap(root, bootstrap, bindings, request.platform, expected_files)
    blocked: list[str] = []
    for command in commands:
        name = f"{command.id}.manifest.json"
        expected_files.add(name)
        manifest = _load_json(root / name, f"{command.id} manifest")
        if _validate_command(
            root, manifest, command, bindings, request.platform, expected_files
        ):
            blocked.append(command.id)
    disposition = EvidenceDisposition(
        request.platform,
        request.mode,
        bindings.tested_sha,
        bindings.tools_sha,
        bindings.matrix_semantic_sha256,
        tuple(command.id for command in commands),
        tuple(blocked),
    )
    if request.mode == "post" and blocked:
        baseline_blocked = _load_baseline_disposition(request, disposition)
        if any(command_id not in baseline_blocked for command_id in blocked):
            raise EvidenceError("post evidence contains an unpaired failure")
    allowed_summaries = {f"{request.mode}-summary.json"}
    if request.mode == "post":
        allowed_summaries.add("baseline-summary.json")
    actual_files = {
        path.relative_to(root).as_posix() for path in root.rglob("*") if path.is_file()
    }
    unexpected = actual_files - expected_files - allowed_summaries
    missing = expected_files - actual_files
    if missing or unexpected:
        raise EvidenceError("evidence completeness differs from the matrix")
    summary_path = root / f"{request.mode}-summary.json"
    if summary_path.exists():
        expected_summary = canonical_json_bytes(disposition_dict(disposition)) + b"\n"
        if _stable_file_bytes(summary_path) != expected_summary:
            raise EvidenceError("existing summary differs from recomputed disposition")
    return disposition


def _safe_public_value(value: object, label: str) -> None:
    if isinstance(value, dict):
        for key, item in value.items():
            if not isinstance(key, str):
                raise EvidenceError(f"{label} contains a non-string key")
            _safe_public_value(item, f"{label}.{key}")
        return
    if isinstance(value, list):
        for index, item in enumerate(value):
            _safe_public_value(item, f"{label}[{index}]")
        return
    if isinstance(value, float) and not (value == value and abs(value) != float("inf")):
        raise EvidenceError(f"{label} contains a non-finite number")
    if not isinstance(value, str):
        return
    encoded = value.encode("utf-8")
    for pattern in FORBIDDEN_BYTE_RULES.values():
        if re.search(pattern, encoded) is not None:
            raise EvidenceError(f"{label} contains forbidden content")
    if value.startswith("/") or re.match(r"(?i)^[A-Z]:[\\/]", value):
        raise EvidenceError(f"{label} contains an absolute path")


def _validate_capability_status(value: object) -> dict[str, object]:
    capability = _exact_object(value, CAPABILITY_STATUS_KEYS, "capabilityStatus")
    if (
        type(capability["codePresent"]) is not bool
        or type(capability["liveProviderReachable"]) is not bool
    ):
        raise EvidenceError("capabilityStatus booleans are invalid")
    for key in ("mechanicalContracts", "businessBlindReview", "publicationRetro"):
        _string(capability[key], f"capabilityStatus.{key}")
    return capability


def _validate_report(path: Path, repo: Path, candidate_sha: str) -> dict[str, object]:
    report = _exact_object(
        _load_json(path, "selected report"), REPORT_KEYS, "selected report"
    )
    _safe_public_value(report, "selected report")
    if report["schemaVersion"] != 1 or report["forkSha"] != candidate_sha:
        raise EvidenceError("selected report candidate binding differs")
    if report["decision"] != "BUSINESS_SIGNAL_PASS_PENDING_FOUNDATION":
        raise EvidenceError("selected report is not foundation-pending PASS")
    public_run_id = _safe_basename(report["publicRunId"], "publicRunId")
    expected = repo / f"docs/evidence/business-proof/{public_run_id}/report.json"
    if path != expected:
        raise EvidenceError("selected report path differs from publicRunId")
    _digest(report["proofRootSha256"], "proofRootSha256")
    for key, value in report.items():
        if key.endswith("Sha256") or key.endswith("Commitment"):
            _digest(value, f"selected report {key}")
    for key in (
        "modelLabel",
        "providerLabel",
        "providerCompatibilityName",
        "providerRole",
        "usageScope",
        "retentionStatus",
        "sourceMaterialRetention",
    ):
        _string(report[key], f"selected report {key}")
    _safe_basename(
        report["retentionCloseoutReceiptPath"], "retentionCloseoutReceiptPath"
    )
    for key in (
        "reviewerCount",
        "experiencedOperatorOrDirectorCount",
        "candidatePreferenceCount",
        "medianGenericScore",
        "medianCandidateScore",
        "medianPairedDelta",
        "candidateSevereFailureCount",
        "genericCostFen",
        "candidateCostFen",
    ):
        minimum = None if key.startswith("median") else 0
        _integer(report[key], f"selected report {key}", minimum=minimum)
    if type(report["candidateSkillUseVerified"]) is not bool:
        raise EvidenceError("candidateSkillUseVerified must be boolean")
    for key in ("genericUsage", "candidateUsage"):
        if not isinstance(report[key], dict):
            raise EvidenceError(f"selected report {key} must be an object")
    for key in ("providerRequestAttemptCounts", "providerCompletedResponseCounts"):
        counts = report[key]
        if (
            not isinstance(counts, list)
            or len(counts) != 2
            or any(type(item) is not int or item < 0 for item in counts)
        ):
            raise EvidenceError(f"selected report {key} is invalid")
    _validate_capability_status(report["capabilityStatus"])
    generated = _rfc3339_utc(report["generatedAt"], "report generatedAt")
    deadline = _rfc3339_utc(
        report["privateEvidenceRetentionDeadline"], "retention deadline"
    )
    if generated >= deadline:
        raise EvidenceError("report retention timeline is invalid")
    return report


def _validate_index(
    path: Path, report: Mapping[str, object], candidate_sha: str, report_sha: str
) -> tuple[dict[str, object], dict[str, object]]:
    index = _exact_object(_load_json(path, "report index"), INDEX_KEYS, "report index")
    if index["schemaVersion"] != 1:
        raise EvidenceError("report index schemaVersion differs")
    attempts = index["attempts"]
    if not isinstance(attempts, list) or not attempts or len(attempts) > 3:
        raise EvidenceError("report index attempts are invalid")
    selected: list[dict[str, object]] = []
    public_ids: set[str] = set()
    case_commitments: set[str] = set()
    material_commitments: set[str] = set()
    for ordinal, raw_attempt in enumerate(attempts, 1):
        attempt = _exact_object(raw_attempt, ATTEMPT_KEYS, "report index attempt")
        _safe_public_value(attempt, "report index attempt")
        if attempt["attemptOrdinal"] != ordinal:
            raise EvidenceError("report index ordinal is not append-only")
        public_id = _safe_basename(attempt["publicRunId"], "attempt publicRunId")
        report_path = _safe_relative_path(attempt["reportPath"], "attempt reportPath")
        _digest(attempt["reportSha256"], "attempt reportSha256")
        _git_sha(attempt["candidateSha"], "attempt candidateSha")
        _digest(attempt["candidateDiffSha256"], "attempt candidateDiffSha256")
        case = _digest(attempt["caseCommitment"], "attempt caseCommitment")
        materials = _digest(
            attempt["materialsCommitment"], "attempt materialsCommitment"
        )
        if type(attempt["selectedForCheckpoint"]) is not bool:
            raise EvidenceError("selectedForCheckpoint must be boolean")
        if (
            public_id in public_ids
            or case in case_commitments
            or materials in material_commitments
        ):
            raise EvidenceError("report index entries are not unique")
        public_ids.add(public_id)
        case_commitments.add(case)
        material_commitments.add(materials)
        expected_supersedes = (
            None if ordinal == 1 else attempts[ordinal - 2]["publicRunId"]
        )
        if attempt["supersedes"] != expected_supersedes:
            raise EvidenceError("report index supersedes chain differs")
        if attempt["selectedForCheckpoint"]:
            selected.append(attempt)
        if report_path != f"docs/evidence/business-proof/{public_id}/report.json":
            raise EvidenceError("report index path differs from publicRunId")
    if len(selected) != 1:
        raise EvidenceError("report index must select exactly one report")
    chosen = selected[0]
    if (
        chosen["publicRunId"] != report["publicRunId"]
        or chosen["candidateSha"] != candidate_sha
        or chosen["reportSha256"] != report_sha
        or chosen["decision"] != report["decision"]
    ):
        raise EvidenceError("selected report/index binding differs")
    return index, chosen


def _validate_receipt(
    path: Path,
    report: Mapping[str, object],
    chosen: Mapping[str, object],
    candidate_sha: str,
    report_sha: str,
    index_sha: str,
) -> dict[str, object]:
    receipt = _exact_object(
        _load_json(path, "business verification receipt"),
        BUSINESS_RECEIPT_KEYS,
        "business verification receipt",
    )
    _safe_public_value(receipt, "business verification receipt")
    if receipt["schemaVersion"] != 1:
        raise EvidenceError("business receipt schemaVersion differs")
    expected = {
        "publicRunId": report["publicRunId"],
        "candidateSha": candidate_sha,
        "reportPath": chosen["reportPath"],
        "reportSha256": report_sha,
        "reportIndexSha256": index_sha,
        "selectedAttemptOrdinal": chosen["attemptOrdinal"],
        "proofRootSha256": report["proofRootSha256"],
        "frozenRunContextCommitment": report["frozenRunContextCommitment"],
        "executionContextCommitment": report["executionContextCommitment"],
        "attemptIndexRootCommitment": report["attemptIndexRootCommitment"],
        "candidateAllowedDiffSha256": chosen["candidateDiffSha256"],
        "capabilityStatus": report["capabilityStatus"],
    }
    for key, value in expected.items():
        if receipt[key] != value:
            raise EvidenceError(f"business receipt {key} binding differs")
    for key in (
        "reportSha256",
        "reportIndexSha256",
        "proofRootSha256",
        "frozenRunContextCommitment",
        "executionContextCommitment",
        "attemptIndexRootCommitment",
        "brokerReceiptCommitment",
        "costReceiptsCommitment",
        "candidateAllowedDiffSha256",
    ):
        _digest(receipt[key], f"business receipt {key}")
    if receipt["G2"] != "PASS":
        raise EvidenceError("business receipt G2 did not pass")
    _validate_capability_status(receipt["capabilityStatus"])
    verified = _rfc3339_utc(receipt["verifiedAt"], "business receipt verifiedAt")
    deadline = _rfc3339_utc(
        report["privateEvidenceRetentionDeadline"], "retention deadline"
    )
    if verified >= deadline or receipt["verifiedBeforeRetentionDeadline"] is not True:
        raise EvidenceError(
            "business receipt was not verified before retention deadline"
        )
    return receipt


def _publish_create_new(path: Path, payload: bytes, repo: Path) -> None:
    if not path.is_absolute():
        raise EvidenceError("verification output must be absolute")
    if os.path.lexists(path):
        raise EvidenceError("verification output already exists")
    parent = _absolute_directory(path.parent, "verification output parent")
    if parent == repo or repo in parent.parents:
        raise EvidenceError("verification output must be outside the repository")
    _safe_basename(path.name, "verification output name")
    try:
        temp = capture._write_temp_bytes(path, payload)
    except OSError as error:
        raise EvidenceError("unable to create final output temporary file") from error
    try:
        capture._publish_create_new(temp, path)
    except BaseException as error:
        try:
            temp.unlink()
        except FileNotFoundError:
            pass
        if isinstance(error, EvidenceError):
            raise
        if isinstance(error, OSError):
            raise EvidenceError("unsupported final output filesystem") from error
        raise


def verify_frozen_final(request: FinalVerificationRequest) -> dict[str, object]:
    repo = _absolute_directory(request.repo_root, "repo root")
    candidate_sha = _require_commit(repo, request.candidate_sha, "candidateSha")
    mac = verify_evidence(
        VerificationRequest(
            repo,
            request.matrix_path,
            request.mac_post_root,
            "macos-x86_64",
            "post",
        )
    )
    windows = verify_evidence(
        VerificationRequest(
            repo,
            request.matrix_path,
            request.windows_post_root,
            "windows-11-x64",
            "post",
        )
    )
    if mac.status != "PASS" or windows.status != "PASS":
        raise EvidenceError("both native post evidence dispositions must pass")
    if (
        mac.tested_git_sha != candidate_sha
        or windows.tested_git_sha != candidate_sha
        or mac.tools_git_sha != windows.tools_git_sha
        or mac.matrix_sha256 != windows.matrix_sha256
    ):
        raise EvidenceError("native post evidence bindings differ")
    report_path = _absolute_file(request.selected_report, "selected report")
    index_path = _absolute_file(request.report_index, "report index")
    if index_path != repo / "docs/evidence/business-proof/index.json":
        raise EvidenceError("report index path differs")
    receipt_path = _absolute_file(
        request.business_verification_receipt, "business verification receipt"
    )
    try:
        _scan_consumed_file(report_path, "selected-report.json")
        _scan_consumed_file(index_path, "report-index.json")
        _scan_consumed_file(receipt_path, "business-verification-receipt.json")
    except ForbiddenEvidenceError:
        raise
    except EvidenceError as error:
        raise ForbiddenEvidenceError(()) from error
    report_payload = _stable_file_bytes(report_path)
    index_payload = _stable_file_bytes(index_path)
    receipt_payload = _stable_file_bytes(receipt_path)
    report = _validate_report(report_path, repo, candidate_sha)
    report_sha = _sha256(report_payload)
    index, chosen = _validate_index(index_path, report, candidate_sha, report_sha)
    del index
    index_sha = _sha256(index_payload)
    receipt = _validate_receipt(
        receipt_path, report, chosen, candidate_sha, report_sha, index_sha
    )
    disposition_payload = lambda item: (
        canonical_json_bytes(disposition_dict(item)) + b"\n"
    )
    output = {
        key: receipt[key]
        for key in BUSINESS_RECEIPT_KEYS
        if key
        not in {
            "candidateAllowedDiffSha256",
            "schemaVersion",
        }
    }
    output.update(
        {
            "schemaVersion": 1,
            "requiredMatrixSha256": mac.matrix_sha256,
            "macPostEvidenceCommitment": _sha256(disposition_payload(mac)),
            "windowsPostEvidenceCommitment": _sha256(disposition_payload(windows)),
            "candidateAllowedDiffSha256": receipt["candidateAllowedDiffSha256"],
            "liveProofVerificationSha256": _sha256(receipt_payload),
            "G0": "PASS",
            "G1": "PASS",
            "foundationDecision": "PASS_TO_PHASE_0B",
            "verifierSha256": _sha256(_stable_file_bytes(Path(__file__).resolve())),
        }
    )
    if set(output) != FINAL_OUTPUT_KEYS:
        raise EvidenceError("internal final output allowlist differs")
    _safe_public_value(output, "final verification output")
    payload = canonical_json_bytes(output) + b"\n"
    _publish_create_new(request.verification_output, payload, repo)
    return output


def _forbidden_output(error: ForbiddenEvidenceError) -> bytes:
    return (
        canonical_json_bytes(
            {
                "schemaVersion": 1,
                "status": "INVALID_FORBIDDEN_CONTENT",
                "matches": [
                    {"path": match.path, "ruleId": match.rule_id}
                    for match in error.matches
                ],
            }
        )
        + b"\n"
    )


def _write_summary(path: Path, payload: bytes, root: Path, mode: str) -> None:
    expected = root / f"{mode}-summary.json"
    if path != expected:
        raise EvidenceError("summary output path differs from evidence mode")
    if os.path.lexists(path):
        raise EvidenceError("summary output already exists")
    temp = capture._write_temp_bytes(path, payload)
    try:
        capture._publish_create_new(temp, path)
    except BaseException:
        try:
            temp.unlink()
        except FileNotFoundError:
            pass
        raise


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root")
    parser.add_argument("--matrix")
    parser.add_argument("--evidence-root")
    parser.add_argument("--platform", choices=sorted(PLATFORM_ARCHITECTURES))
    parser.add_argument("--mode", choices=["baseline", "post"])
    parser.add_argument("--summary-output")
    parser.add_argument("--candidate-sha")
    parser.add_argument("--selected-report")
    parser.add_argument("--report-index")
    parser.add_argument("--business-verification-receipt")
    parser.add_argument("--verification-output")
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = _parser()
    arguments = parser.parse_args(argv)
    final_names = (
        "candidate_sha",
        "selected_report",
        "report_index",
        "business_verification_receipt",
        "verification_output",
    )
    final_values = [getattr(arguments, name) for name in final_names]
    if any(final_values) and not all(final_values):
        parser.error("the five frozen-final arguments are required together")
    try:
        if all(final_values):
            repo = (
                Path(arguments.repo_root).resolve()
                if arguments.repo_root
                else Path.cwd().resolve()
            )
            matrix = (
                Path(arguments.matrix).resolve()
                if arguments.matrix
                else repo / "scripts/ai_ip/foundation/required_command_matrix.json"
            )
            foundation = (
                Path(arguments.evidence_root).resolve()
                if arguments.evidence_root
                else repo / "docs/evidence/foundation"
            )
            verify_frozen_final(
                FinalVerificationRequest(
                    repo,
                    matrix,
                    foundation / "macos-x86_64",
                    foundation / "windows-11-x64",
                    arguments.candidate_sha,
                    Path(arguments.selected_report),
                    Path(arguments.report_index),
                    Path(arguments.business_verification_receipt),
                    Path(arguments.verification_output),
                )
            )
            return 0
        required = {
            "repo-root": arguments.repo_root,
            "matrix": arguments.matrix,
            "evidence-root": arguments.evidence_root,
            "platform": arguments.platform,
            "mode": arguments.mode,
        }
        missing = [name for name, value in required.items() if value is None]
        if missing:
            parser.error(
                f"baseline/post arguments are required together: {', '.join(missing)}"
            )
        request = VerificationRequest(
            Path(arguments.repo_root),
            Path(arguments.matrix),
            Path(arguments.evidence_root),
            arguments.platform,
            arguments.mode,
        )
        disposition = verify_evidence(request)
        payload = canonical_json_bytes(disposition_dict(disposition)) + b"\n"
        if arguments.summary_output is not None:
            _write_summary(
                Path(arguments.summary_output),
                payload,
                request.evidence_root.resolve(),
                request.mode,
            )
        sys.stdout.buffer.write(payload)
        return 0 if disposition.status == "PASS" else 2
    except ForbiddenEvidenceError as error:
        sys.stdout.buffer.write(_forbidden_output(error))
        return 1
    except EvidenceError as error:
        print(f"verify_evidence: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
