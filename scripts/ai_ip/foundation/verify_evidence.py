#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
import re
import stat
import subprocess
import sys
from collections.abc import Iterator
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Mapping

import capture_command as capture


EvidenceError = capture.EvidenceError
CHUNK_SIZE = 1024 * 1024
MAX_PUBLIC_JSON_BYTES = 16 * 1024 * 1024
MAX_TOTAL_PUBLIC_JSON_BYTES = 64 * 1024 * 1024
SHA_256_PATTERN = re.compile(r"[0-9a-f]{64}")
SHA_1_PATTERN = re.compile(r"[0-9a-f]{40}")
RFC3339_UTC_PATTERN = re.compile(
    r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{6}Z"
)
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
PRIVATE_PUBLIC_KEYS = {
    "thread",
    "threadid",
    "response",
    "responseid",
    "responsebody",
    "reviewer",
    "reviewerid",
    "reviewermapping",
    "private",
    "privateroot",
    "privatecase",
    "casebody",
    "prompt",
    "promptbody",
    "output",
    "outputbody",
}
PRIVATE_CREDENTIAL_KEYS = {"apikey", "accesstoken", "secretkey", "secret"}
PRIVATE_PUBLIC_VALUE_PATTERN = re.compile(
    r"(?i)(?:\b(?:thread|response|reviewer)[-_ ]?id\b|"
    r"\b(?:thread|reviewer)[-_ ][A-Za-z0-9]{3,}\b|"
    r"\bresponse[-_ ](?!status\b)[A-Za-z0-9]{3,}\b|"
    r"\bprivate[-_ ]*(?:root|material|case)\b|"
    r"\b(?:prompt|output)[-_ ]*body\b)"
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


@dataclass(frozen=True, order=True)
class ForbiddenMatch:
    path: str
    rule_id: str


class ForbiddenEvidenceError(EvidenceError):
    def __init__(self, matches: tuple[ForbiddenMatch, ...]) -> None:
        super().__init__("forbidden public evidence content")
        self.matches = matches


class UnsafeEvidenceError(EvidenceError):
    pass


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
    matrix_snapshot: "_FileSnapshot"


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


def _load_json_bytes(
    payload: bytes, label: str, *, canonical: bool = True
) -> dict[str, object]:
    try:
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
    if RFC3339_UTC_PATTERN.fullmatch(text) is None:
        raise EvidenceError(f"{label} must be RFC3339 UTC")
    try:
        parsed = datetime.fromisoformat(text[:-1] + "+00:00")
    except ValueError as error:
        raise EvidenceError(f"{label} must be RFC3339 UTC") from error
    if parsed.tzinfo is None or parsed.utcoffset() != timezone.utc.utcoffset(parsed):
        raise EvidenceError(f"{label} must be RFC3339 UTC")
    return parsed


def _unsafe_path_component(path: Path) -> str | None:
    current = Path(path.anchor)
    for part in path.parts[1:]:
        current /= part
        try:
            metadata = current.lstat()
        except OSError as error:
            raise EvidenceError(
                f"unable to inspect path component: {current.name}"
            ) from error
        if stat.S_ISLNK(metadata.st_mode):
            return "symlink"
        if getattr(metadata, "st_file_attributes", 0) & 0x400 or getattr(
            metadata, "st_reparse_tag", 0
        ):
            return "reparse"
    return None


def _absolute_directory(path: Path, label: str) -> Path:
    lexical = Path(path)
    if not lexical.is_absolute():
        raise EvidenceError(f"{label} must be an absolute non-symlink directory")
    unsafe = _unsafe_path_component(lexical)
    if unsafe is not None:
        raise UnsafeEvidenceError(f"{label} contains a {unsafe} component")
    try:
        resolved = lexical.resolve(strict=True)
    except OSError as error:
        raise EvidenceError(f"{label} must exist") from error
    if not resolved.is_dir():
        raise UnsafeEvidenceError(f"{label} must be a directory")
    return resolved


def _absolute_file(path: Path, label: str) -> Path:
    lexical = Path(path)
    if not lexical.is_absolute():
        raise EvidenceError(f"{label} must be an absolute non-symlink file")
    unsafe = _unsafe_path_component(lexical)
    if unsafe is not None:
        raise UnsafeEvidenceError(f"{label} contains a {unsafe} component")
    try:
        resolved = lexical.resolve(strict=True)
    except OSError as error:
        raise EvidenceError(f"{label} must exist") from error
    if not resolved.is_file():
        raise UnsafeEvidenceError(f"{label} must be a file")
    return resolved


def _reparse_state(value: os.stat_result) -> tuple[int, int]:
    return (
        getattr(value, "st_file_attributes", 0) & 0x400,
        getattr(value, "st_reparse_tag", 0),
    )


def _same_file_state(left: os.stat_result, right: os.stat_result) -> bool:
    return (
        left.st_dev,
        left.st_ino,
        left.st_mode,
        left.st_nlink,
        left.st_size,
        left.st_mtime_ns,
        _reparse_state(left),
    ) == (
        right.st_dev,
        right.st_ino,
        right.st_mode,
        right.st_nlink,
        right.st_size,
        right.st_mtime_ns,
        _reparse_state(right),
    )


def _safe_fstat(descriptor: int, label: str) -> os.stat_result:
    try:
        return os.fstat(descriptor)
    except OSError as error:
        raise UnsafeEvidenceError(
            f"unable to inspect opened public evidence: {label}"
        ) from error


def _descriptor_relative_traversal_available() -> bool:
    return (
        os.open in os.supports_dir_fd
        and os.stat in os.supports_dir_fd
        and os.stat in os.supports_follow_symlinks
        and os.scandir in os.supports_fd
        and hasattr(os, "O_DIRECTORY")
        and hasattr(os, "O_NOFOLLOW")
    )


def _unsafe_reparse_state(value: os.stat_result) -> bool:
    return _reparse_state(value) != (0, 0)


def _require_safe_directory_state(value: os.stat_result, label: str) -> None:
    if stat.S_ISLNK(value.st_mode) or not stat.S_ISDIR(value.st_mode):
        raise UnsafeEvidenceError(f"unsafe public evidence directory: {label}")
    if _unsafe_reparse_state(value):
        raise UnsafeEvidenceError(f"reparse public evidence directory: {label}")


def _open_absolute_parent_nofollow(path: Path) -> int:
    lexical = Path(path)
    if not lexical.is_absolute():
        raise UnsafeEvidenceError("public evidence path must be absolute")
    anchor = Path(lexical.anchor)
    flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
    try:
        current_fd = os.open(anchor, flags)
    except OSError as error:
        raise UnsafeEvidenceError("unable to open public evidence anchor") from error
    try:
        anchor_state = _safe_fstat(current_fd, str(anchor))
        _require_safe_directory_state(anchor_state, str(anchor))
        current = anchor
        for component in lexical.parent.parts[1:]:
            current /= component
            try:
                before = os.stat(
                    component,
                    dir_fd=current_fd,
                    follow_symlinks=False,
                )
            except OSError as error:
                raise UnsafeEvidenceError(
                    f"unable to inspect public evidence directory: {current.name}"
                ) from error
            _require_safe_directory_state(before, current.name)
            try:
                next_fd = os.open(component, flags, dir_fd=current_fd)
            except OSError as error:
                raise UnsafeEvidenceError(
                    f"unable to open public evidence directory: {current.name}"
                ) from error
            try:
                opened = _safe_fstat(next_fd, current.name)
                _require_safe_directory_state(opened, current.name)
                if not _same_file_state(before, opened):
                    raise UnsafeEvidenceError(
                        "public evidence directory changed before traversal: "
                        f"{current.name}"
                    )
            except BaseException:
                os.close(next_fd)
                raise
            os.close(current_fd)
            current_fd = next_fd
        result = current_fd
        current_fd = -1
        return result
    finally:
        if current_fd >= 0:
            os.close(current_fd)


def _fallback_component_states(path: Path, label: str) -> tuple[os.stat_result, ...]:
    lexical = Path(path)
    if not lexical.is_absolute():
        raise UnsafeEvidenceError(f"public evidence path must be absolute: {label}")
    current = Path(lexical.anchor)
    components = [current]
    for component in lexical.parent.parts[1:]:
        current /= component
        components.append(current)
    states: list[os.stat_result] = []
    for component in components:
        try:
            value = component.lstat()
        except OSError as error:
            raise UnsafeEvidenceError(
                f"unable to inspect public evidence directory: {label}"
            ) from error
        _require_safe_directory_state(value, component.name or str(component))
        states.append(value)
    return tuple(states)


def _same_component_states(
    before: tuple[os.stat_result, ...], after: tuple[os.stat_result, ...]
) -> bool:
    return len(before) == len(after) and all(
        _same_file_state(left, right) for left, right in zip(before, after)
    )


def _absolute_leaf_state(path: Path, label: str) -> os.stat_result:
    lexical = Path(path)
    if _descriptor_relative_traversal_available():
        parent_fd = _open_absolute_parent_nofollow(lexical)
        try:
            if lexical == Path(lexical.anchor):
                return _safe_fstat(parent_fd, label)
            return _entry_state(lexical, parent_fd, lexical.name)
        finally:
            os.close(parent_fd)
    before = _fallback_component_states(lexical, label)
    try:
        leaf = lexical.lstat()
    except OSError as error:
        raise UnsafeEvidenceError(
            f"unable to inspect public evidence file: {label}"
        ) from error
    after = _fallback_component_states(lexical, label)
    if not _same_component_states(before, after):
        raise UnsafeEvidenceError(
            f"public evidence path changed while inspected: {label}"
        )
    return leaf


def _open_stable_directory(
    path: Path,
    *,
    parent_fd: int | None = None,
    name: str | None = None,
    expected_state: os.stat_result | None = None,
) -> tuple[int | None, os.stat_result]:
    anchored_parent_fd: int | None = None
    try:
        if parent_fd is not None and name is not None:
            before = os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
        elif _descriptor_relative_traversal_available():
            anchored_parent_fd = _open_absolute_parent_nofollow(path)
            if path == Path(path.anchor):
                before = _safe_fstat(anchored_parent_fd, path.name or str(path))
            else:
                before = os.stat(
                    path.name,
                    dir_fd=anchored_parent_fd,
                    follow_symlinks=False,
                )
        else:
            before_components = _fallback_component_states(path, path.name)
            before = path.lstat()
    except OSError as error:
        if anchored_parent_fd is not None:
            os.close(anchored_parent_fd)
        raise UnsafeEvidenceError(
            f"unable to inspect public evidence directory: {path.name}"
        ) from error
    if expected_state is not None and not _same_file_state(expected_state, before):
        if anchored_parent_fd is not None:
            os.close(anchored_parent_fd)
        raise UnsafeEvidenceError(
            f"public evidence directory changed before traversal: {path.name}"
        )
    try:
        _require_safe_directory_state(before, path.name or str(path))
    except BaseException:
        if anchored_parent_fd is not None:
            os.close(anchored_parent_fd)
        raise
    if not _descriptor_relative_traversal_available():
        after_components = _fallback_component_states(path, path.name)
        if not _same_component_states(before_components, after_components):
            raise UnsafeEvidenceError(
                f"public evidence directory changed before traversal: {path.name}"
            )
        return None, before
    flags = os.O_RDONLY | os.O_DIRECTORY | getattr(os, "O_NOFOLLOW", 0)
    try:
        if parent_fd is not None and name is not None:
            descriptor = os.open(name, flags, dir_fd=parent_fd)
        elif path == Path(path.anchor):
            if anchored_parent_fd is None:
                raise UnsafeEvidenceError("public evidence anchor is unavailable")
            descriptor = anchored_parent_fd
            anchored_parent_fd = None
        else:
            if anchored_parent_fd is None:
                raise UnsafeEvidenceError("public evidence parent is unavailable")
            descriptor = os.open(path.name, flags, dir_fd=anchored_parent_fd)
    except OSError as error:
        raise UnsafeEvidenceError(
            f"unable to open public evidence directory: {path.name}"
        ) from error
    finally:
        if anchored_parent_fd is not None:
            os.close(anchored_parent_fd)
    try:
        opened = _safe_fstat(descriptor, path.name or str(path))
        _require_safe_directory_state(opened, path.name or str(path))
    except BaseException:
        os.close(descriptor)
        raise
    if not _same_file_state(before, opened):
        os.close(descriptor)
        raise UnsafeEvidenceError(
            f"public evidence directory changed before traversal: {path.name}"
        )
    return descriptor, opened


def _entry_state(path: Path, dir_fd: int | None, name: str | None) -> os.stat_result:
    try:
        if dir_fd is not None and name is not None:
            return os.stat(name, dir_fd=dir_fd, follow_symlinks=False)
        return path.stat(follow_symlinks=False)
    except OSError as error:
        raise UnsafeEvidenceError(
            f"unable to inspect public evidence file: {path.name}"
        ) from error


def _open_stable_regular(
    path: Path,
    *,
    dir_fd: int | None = None,
    name: str | None = None,
    expected_state: os.stat_result | None = None,
) -> tuple[int, os.stat_result]:
    del dir_fd, name
    lexical = Path(path)
    parent_fd: int | None = None
    before_components: tuple[os.stat_result, ...] | None = None
    if _descriptor_relative_traversal_available():
        parent_fd = _open_absolute_parent_nofollow(lexical)
        try:
            before = _entry_state(lexical, parent_fd, lexical.name)
        except UnsafeEvidenceError:
            os.close(parent_fd)
            raise
    else:
        before_components = _fallback_component_states(lexical, path.name)
        before = _entry_state(lexical, None, None)
    if expected_state is not None and not _same_file_state(expected_state, before):
        if parent_fd is not None:
            os.close(parent_fd)
        raise UnsafeEvidenceError(
            f"public evidence file changed before scan: {path.name}"
        )
    if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
        if parent_fd is not None:
            os.close(parent_fd)
        raise UnsafeEvidenceError(f"unsafe public evidence file: {path.name}")
    if _unsafe_reparse_state(before):
        if parent_fd is not None:
            os.close(parent_fd)
        raise UnsafeEvidenceError(f"reparse public evidence file: {path.name}")
    flags = os.O_RDONLY | getattr(os, "O_BINARY", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        if parent_fd is not None:
            descriptor = os.open(lexical.name, flags, dir_fd=parent_fd)
        else:
            descriptor = os.open(lexical, flags)
    except OSError as error:
        raise UnsafeEvidenceError(
            f"unable to open public evidence file: {path.name}"
        ) from error
    finally:
        if parent_fd is not None:
            os.close(parent_fd)
    try:
        opened = _safe_fstat(descriptor, path.name)
    except BaseException:
        os.close(descriptor)
        raise
    if _unsafe_reparse_state(opened) or not _same_file_state(before, opened):
        os.close(descriptor)
        raise UnsafeEvidenceError(
            f"public evidence file changed before scan: {path.name}"
        )
    if before_components is not None:
        after_components = _fallback_component_states(lexical, path.name)
        if not _same_component_states(before_components, after_components):
            os.close(descriptor)
            raise UnsafeEvidenceError(
                f"public evidence path changed before scan: {path.name}"
            )
    return descriptor, opened


def _bounded_file_chunks(
    descriptor: int, captured_size: int, label: str
) -> Iterator[bytes]:
    remaining = captured_size
    while remaining:
        try:
            chunk = os.read(descriptor, min(CHUNK_SIZE, remaining))
        except OSError as error:
            raise UnsafeEvidenceError(
                f"unable to read public evidence: {label}"
            ) from error
        if not chunk or len(chunk) > remaining:
            raise UnsafeEvidenceError(
                f"public evidence file ended before captured size: {label}"
            )
        remaining -= len(chunk)
        yield chunk
    try:
        overrun = os.read(descriptor, 1)
    except OSError as error:
        raise UnsafeEvidenceError(
            f"unable to probe public evidence: {label}"
        ) from error
    if overrun:
        raise UnsafeEvidenceError(
            f"public evidence file exceeded captured size: {label}"
        )


def _stable_file_bytes(path: Path) -> bytes:
    descriptor, before = _open_stable_regular(path)
    chunks: list[bytes] = []
    try:
        for chunk in _bounded_file_chunks(descriptor, before.st_size, path.name):
            chunks.append(chunk)
        after = _safe_fstat(descriptor, path.name)
    finally:
        os.close(descriptor)
    try:
        final = _absolute_leaf_state(path, path.name)
    except UnsafeEvidenceError as error:
        raise UnsafeEvidenceError(
            f"public evidence file changed after read: {path.name}"
        ) from error
    if not _same_file_state(before, after) or not _same_file_state(before, final):
        raise UnsafeEvidenceError(
            f"public evidence file changed while read: {path.name}"
        )
    payload = b"".join(chunks)
    if len(payload) != before.st_size:
        raise UnsafeEvidenceError(
            f"public evidence file size changed while read: {path.name}"
        )
    return payload


def _scan_one(path: Path, display_path: str) -> tuple[ForbiddenMatch, ...]:
    descriptor, before = _open_stable_regular(path)
    found: set[ForbiddenMatch] = set()
    overlap = b""
    total = 0
    try:
        for chunk in _bounded_file_chunks(descriptor, before.st_size, display_path):
            total += len(chunk)
            window = overlap + chunk
            for rule_id, pattern in FORBIDDEN_BYTE_RULES.items():
                if re.search(pattern, window) is not None:
                    found.add(ForbiddenMatch(display_path, rule_id))
            overlap = window[-CHUNK_SIZE:]
        after = _safe_fstat(descriptor, display_path)
    finally:
        os.close(descriptor)
    try:
        final = _absolute_leaf_state(path, display_path)
    except UnsafeEvidenceError as error:
        raise UnsafeEvidenceError(
            f"public evidence file changed after scan: {display_path}"
        ) from error
    if (
        total != before.st_size
        or not _same_file_state(before, after)
        or not _same_file_state(before, final)
    ):
        raise UnsafeEvidenceError(
            f"public evidence file changed while scanned: {display_path}"
        )
    return tuple(sorted(found))


@dataclass(frozen=True)
class _FileSnapshot:
    path: Path
    display: str
    state: os.stat_result
    size: int
    sha256: str
    payload: bytes | None
    matches: tuple[ForbiddenMatch, ...]
    dir_fd: int | None = None
    relative_name: str | None = None

    @classmethod
    def capture(
        cls,
        path: Path,
        display: str,
        *,
        keep_payload: bool,
        dir_fd: int | None = None,
        relative_name: str | None = None,
        expected_state: os.stat_result | None = None,
        max_payload_bytes: int | None = None,
    ) -> "_FileSnapshot":
        descriptor, before = _open_stable_regular(
            path,
            dir_fd=dir_fd,
            name=relative_name,
            expected_state=expected_state,
        )
        payload_limit = MAX_PUBLIC_JSON_BYTES
        if max_payload_bytes is not None:
            payload_limit = min(payload_limit, max_payload_bytes)
        if keep_payload and before.st_size > payload_limit:
            os.close(descriptor)
            raise EvidenceError(f"public JSON exceeds retained limit: {display}")
        digest = hashlib.sha256()
        payload_chunks: list[bytes] | None = [] if keep_payload else None
        matches: set[ForbiddenMatch] = set()
        overlap = b""
        total = 0
        try:
            for chunk in _bounded_file_chunks(descriptor, before.st_size, display):
                total += len(chunk)
                digest.update(chunk)
                if payload_chunks is not None:
                    if total > payload_limit:
                        raise EvidenceError(
                            f"public JSON exceeds retained limit: {display}"
                        )
                    payload_chunks.append(chunk)
                window = overlap + chunk
                for rule_id, pattern in FORBIDDEN_BYTE_RULES.items():
                    if re.search(pattern, window) is not None:
                        matches.add(ForbiddenMatch(display, rule_id))
                overlap = window[-CHUNK_SIZE:]
            after = _safe_fstat(descriptor, display)
        finally:
            os.close(descriptor)
        try:
            final = _absolute_leaf_state(path, display)
        except UnsafeEvidenceError as error:
            raise UnsafeEvidenceError(
                f"public evidence file changed after read: {display}"
            ) from error
        if (
            total != before.st_size
            or not _same_file_state(before, after)
            or not _same_file_state(before, final)
        ):
            raise UnsafeEvidenceError(
                f"public evidence file changed while read: {display}"
            )
        payload = b"".join(payload_chunks) if payload_chunks is not None else None
        return cls(
            path,
            display,
            before,
            total,
            digest.hexdigest(),
            payload,
            tuple(sorted(matches)),
            dir_fd,
            relative_name,
        )

    def validate_final_state(self) -> None:
        descriptor, reopened = _open_stable_regular(
            self.path,
            dir_fd=self.dir_fd,
            name=self.relative_name,
            expected_state=self.state,
        )
        digest = hashlib.sha256()
        total = 0
        try:
            for chunk in _bounded_file_chunks(descriptor, self.size, self.display):
                total += len(chunk)
                digest.update(chunk)
            after = _safe_fstat(descriptor, self.display)
        finally:
            os.close(descriptor)
        try:
            final = _absolute_leaf_state(self.path, self.display)
        except UnsafeEvidenceError as error:
            raise UnsafeEvidenceError(
                f"public evidence file changed after consumption: {self.display}"
            ) from error
        if (
            total != self.size
            or digest.hexdigest() != self.sha256
            or not _same_file_state(self.state, reopened)
            or not _same_file_state(reopened, after)
            or not _same_file_state(reopened, final)
        ):
            raise UnsafeEvidenceError(
                f"public evidence file changed after consumption: {self.display}"
            )


class _EvidenceSnapshot:
    def __init__(self, root: Path) -> None:
        self.root = root
        self.files: dict[str, _FileSnapshot] = {}
        self.entry_states: dict[str, os.stat_result] = {}
        self.root_fd: int | None = None
        self.root_state: os.stat_result | None = None
        try:
            self.root_fd, self.root_state = _open_stable_directory(root)
            self._enumerate_names_and_types()
        except BaseException:
            self.close()
            raise

    def _scandir(self) -> list[os.DirEntry[str]]:
        target: int | Path = self.root_fd if self.root_fd is not None else self.root
        try:
            return sorted(os.scandir(target), key=lambda entry: entry.name)
        except OSError as error:
            raise UnsafeEvidenceError("unable to enumerate public evidence") from error

    def _root_unchanged(self) -> None:
        if self.root_state is None:
            raise EvidenceError("public evidence root snapshot is unavailable")
        try:
            lexical = _absolute_leaf_state(self.root, self.root.name)
            opened = (
                _safe_fstat(self.root_fd, self.root.name)
                if self.root_fd is not None
                else lexical
            )
            _require_safe_directory_state(lexical, self.root.name)
            _require_safe_directory_state(opened, self.root.name)
        except UnsafeEvidenceError as error:
            raise UnsafeEvidenceError("public evidence root changed") from error
        if not _same_file_state(self.root_state, lexical) or not _same_file_state(
            self.root_state, opened
        ):
            raise UnsafeEvidenceError("public evidence root changed")

    def _enumerate_names_and_types(self) -> None:
        entries = self._scandir()
        for entry in entries:
            name = _safe_basename(entry.name, "public evidence path component")
            path = self.root / name
            try:
                metadata = _entry_state(path, self.root_fd, name)
            except UnsafeEvidenceError as error:
                raise UnsafeEvidenceError(
                    f"unable to inspect public evidence: {name}"
                ) from error
            if stat.S_ISLNK(metadata.st_mode) or _unsafe_reparse_state(metadata):
                raise UnsafeEvidenceError(f"unsafe public evidence entry: {name}")
            if stat.S_ISREG(metadata.st_mode):
                if metadata.st_nlink != 1:
                    raise UnsafeEvidenceError(
                        f"hardlinked public evidence file: {name}"
                    )
            elif not stat.S_ISDIR(metadata.st_mode):
                raise UnsafeEvidenceError(f"special public evidence file: {name}")
            self.entry_states[name] = metadata
        self._root_unchanged()

    def require_exact(self, required_names: set[str], optional_names: set[str]) -> None:
        allowed = required_names | optional_names
        actual = set(self.entry_states)
        if required_names - actual or actual - allowed:
            raise EvidenceError("evidence completeness differs from the matrix")
        if any(not stat.S_ISREG(self.entry_states[name].st_mode) for name in actual):
            raise EvidenceError("evidence completeness differs from the matrix")
        retained = [name for name in actual if not name.endswith(".log")]
        if any(
            self.entry_states[name].st_size > MAX_PUBLIC_JSON_BYTES for name in retained
        ):
            raise EvidenceError("public JSON exceeds per-file limit")
        if sum(self.entry_states[name].st_size for name in retained) > (
            MAX_TOTAL_PUBLIC_JSON_BYTES
        ):
            raise EvidenceError("total public JSON exceeds snapshot limit")
        remaining_retained = MAX_TOTAL_PUBLIC_JSON_BYTES
        for name in sorted(actual):
            path = self.root / name
            keep_payload = not name.endswith(".log")
            self.files[name] = _FileSnapshot.capture(
                path,
                name,
                keep_payload=keep_payload,
                dir_fd=self.root_fd,
                relative_name=name if self.root_fd is not None else None,
                expected_state=self.entry_states[name],
                max_payload_bytes=remaining_retained if keep_payload else None,
            )
            if keep_payload:
                remaining_retained -= self.files[name].size
        self._root_unchanged()

    def validate_final_state(self) -> None:
        entries = self._scandir()
        if {entry.name for entry in entries} != set(self.entry_states):
            raise UnsafeEvidenceError("public evidence tree changed after enumeration")
        for name, before in self.entry_states.items():
            final = _absolute_leaf_state(self.root / name, name)
            if not _same_file_state(before, final):
                raise UnsafeEvidenceError(f"public evidence entry changed: {name}")
        for snapshot in self.files.values():
            snapshot.validate_final_state()
        self._root_unchanged()

    def close(self) -> None:
        if self.root_fd is not None:
            os.close(self.root_fd)
            self.root_fd = None

    def json(self, name: str, label: str) -> dict[str, object]:
        try:
            payload = self.files[name].payload
        except KeyError as error:
            raise EvidenceError(f"missing public evidence file: {name}") from error
        if payload is None:
            raise EvidenceError(f"JSON evidence cannot be a log: {name}")
        return _load_json_bytes(payload, label)

    def matches(self) -> tuple[ForbiddenMatch, ...]:
        return tuple(
            sorted(
                match for snapshot in self.files.values() for match in snapshot.matches
            )
        )


def scan_forbidden_evidence(root: Path) -> tuple[ForbiddenMatch, ...]:
    canonical = _absolute_directory(Path(root), "evidence root")
    directories: list[tuple[Path, str, int | None, os.stat_result, set[str]]] = []
    snapshots: list[_FileSnapshot] = []
    pending: list[tuple[Path, str, int | None, str | None, os.stat_result | None]] = [
        (canonical, "", None, None, None)
    ]
    try:
        while pending:
            path, display, parent_fd, name, expected = pending.pop()
            descriptor, state = _open_stable_directory(
                path,
                parent_fd=parent_fd,
                name=name,
                expected_state=expected,
            )
            target: int | Path = descriptor if descriptor is not None else path
            try:
                entries = sorted(os.scandir(target), key=lambda entry: entry.name)
            except OSError as error:
                if descriptor is not None:
                    os.close(descriptor)
                raise UnsafeEvidenceError(
                    "unable to enumerate public evidence"
                ) from error
            names: set[str] = set()
            directories.append((path, display, descriptor, state, names))
            for entry in entries:
                child_name = _safe_basename(
                    entry.name, "public evidence path component"
                )
                child_display = f"{display}/{child_name}" if display else child_name
                child = path / child_name
                metadata = _entry_state(child, descriptor, child_name)
                if stat.S_ISLNK(metadata.st_mode) or _unsafe_reparse_state(metadata):
                    raise UnsafeEvidenceError(
                        f"unsafe public evidence entry: {child_display}"
                    )
                names.add(child_name)
                if stat.S_ISDIR(metadata.st_mode):
                    pending.append(
                        (child, child_display, descriptor, child_name, metadata)
                    )
                elif stat.S_ISREG(metadata.st_mode):
                    if metadata.st_nlink != 1:
                        raise UnsafeEvidenceError(
                            f"hardlinked public evidence file: {child_display}"
                        )
                    snapshots.append(
                        _FileSnapshot.capture(
                            child,
                            child_display,
                            keep_payload=False,
                            dir_fd=descriptor,
                            relative_name=child_name
                            if descriptor is not None
                            else None,
                            expected_state=metadata,
                        )
                    )
                else:
                    raise UnsafeEvidenceError(
                        f"special public evidence file: {child_display}"
                    )
        for path, display, descriptor, state, names in directories:
            target = descriptor if descriptor is not None else path
            try:
                final_names = {entry.name for entry in os.scandir(target)}
                lexical = _absolute_leaf_state(path, display or canonical.name)
                opened = (
                    _safe_fstat(descriptor, display or canonical.name)
                    if descriptor is not None
                    else lexical
                )
                _require_safe_directory_state(lexical, display or canonical.name)
                _require_safe_directory_state(opened, display or canonical.name)
            except (OSError, UnsafeEvidenceError) as error:
                raise UnsafeEvidenceError(
                    "public evidence tree changed after scan"
                ) from error
            if (
                final_names != names
                or not _same_file_state(state, lexical)
                or not _same_file_state(state, opened)
            ):
                label = display or canonical.name
                raise UnsafeEvidenceError(
                    f"public evidence directory changed after scan: {label}"
                )
        for snapshot in snapshots:
            snapshot.validate_final_state()
        return tuple(
            sorted(match for snapshot in snapshots for match in snapshot.matches)
        )
    finally:
        for _, _, descriptor, _, _ in directories:
            if descriptor is not None:
                os.close(descriptor)


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


def _matrix_from_value(parsed: object) -> capture.Matrix:
    root = _exact_object(parsed, {"schemaVersion", "commands"}, "matrix")
    if _integer(root["schemaVersion"], "matrix schemaVersion") != 1:
        raise EvidenceError("matrix schemaVersion differs")
    entries = root["commands"]
    if not isinstance(entries, list):
        raise EvidenceError("matrix commands must be an array")
    commands: list[capture.CommandSpec] = []
    ids: set[str] = set()
    for raw in entries:
        command = _exact_object(
            raw, {"id", "platforms", "phase", "argv", "expectedExit"}, "matrix command"
        )
        command_id = _string(command["id"], "matrix command id")
        if capture.COMMAND_ID_PATTERN.fullmatch(command_id) is None:
            raise EvidenceError("matrix command id is unsafe")
        if command_id in ids:
            raise EvidenceError("duplicate matrix command id")
        platforms = command["platforms"]
        if (
            not isinstance(platforms, list)
            or not platforms
            or any(
                not isinstance(item, str) or item not in PLATFORM_ARCHITECTURES
                for item in platforms
            )
            or len(set(platforms)) != len(platforms)
        ):
            raise EvidenceError("matrix command platforms are invalid")
        phase = command["phase"]
        if not isinstance(phase, str) or phase not in {"baselineAndPost", "postOnly"}:
            raise EvidenceError("matrix command phase is invalid")
        argv = command["argv"]
        if (
            not isinstance(argv, list)
            or not argv
            or any(not isinstance(item, str) or not item for item in argv)
        ):
            raise EvidenceError("matrix command argv is invalid")
        expected_exit = _integer(command["expectedExit"], "matrix expectedExit")
        if expected_exit != 0:
            raise EvidenceError("matrix expectedExit must be zero")
        ids.add(command_id)
        commands.append(
            capture.CommandSpec(
                command_id, tuple(platforms), phase, tuple(argv), expected_exit
            )
        )
    return capture.Matrix(1, tuple(commands))


def _capture_matrix(
    matrix_path: Path,
) -> tuple[Path, _FileSnapshot, object, capture.Matrix]:
    try:
        matrix = _absolute_file(matrix_path, "matrix")
        snapshot = _FileSnapshot.capture(
            matrix, "required-command-matrix.json", keep_payload=True
        )
    except UnsafeEvidenceError as error:
        raise ForbiddenEvidenceError(()) from error
    if snapshot.matches:
        raise ForbiddenEvidenceError(snapshot.matches)
    if snapshot.payload is None:
        raise EvidenceError("internal matrix snapshot differs")
    try:
        parsed = json.loads(
            snapshot.payload,
            object_pairs_hook=_reject_duplicate_keys,
            parse_constant=_reject_json_constant,
        )
    except (json.JSONDecodeError, UnicodeDecodeError) as error:
        raise EvidenceError("invalid matrix JSON") from error
    try:
        return matrix, snapshot, parsed, _matrix_from_value(parsed)
    except BaseException:
        try:
            snapshot.validate_final_state()
        except UnsafeEvidenceError as error:
            raise ForbiddenEvidenceError(()) from error
        raise


def _bindings(
    repo_root: Path,
    matrix_path: Path,
    tested_sha_value: object,
    tools_sha_value: object,
    *,
    preloaded_matrix: tuple[Path, _FileSnapshot, object, capture.Matrix] | None = None,
) -> tuple[_GitBindings, capture.Matrix]:
    repo = _absolute_directory(repo_root, "repo root")
    if preloaded_matrix is None:
        matrix, matrix_snapshot, parsed, loaded_matrix = _capture_matrix(matrix_path)
    else:
        matrix, matrix_snapshot, parsed, loaded_matrix = preloaded_matrix
        if Path(matrix_path) != matrix:
            raise EvidenceError("preloaded matrix path differs")
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
    try:
        if matrix_snapshot.matches:
            raise ForbiddenEvidenceError(matrix_snapshot.matches)
        current_matrix = matrix_snapshot.payload
        if current_matrix is None:
            raise EvidenceError("internal matrix snapshot differs")
        matrix_blob = _commit_blob(tools_repo, tools_sha, matrix_relative, "matrix")
        if current_matrix != matrix_blob:
            raise EvidenceError("matrix differs from the tools commit")
        recorder_blob = _commit_blob(
            tools_repo,
            tools_sha,
            "scripts/ai_ip/foundation/capture_command.py",
            "recorder",
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
                matrix_snapshot,
            ),
            loaded_matrix,
        )
    except BaseException:
        matrix_snapshot.validate_final_state()
        raise


def _validate_stream(
    evidence: _EvidenceSnapshot,
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
    try:
        snapshot = evidence.files[name]
    except KeyError as error:
        raise EvidenceError(f"missing public evidence log: {name}") from error
    if snapshot.size != byte_count or snapshot.sha256 != digest:
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
    evidence: _EvidenceSnapshot,
    value: dict[str, object],
    bindings: _GitBindings,
    platform_id: str,
    expected_files: set[str],
) -> None:
    bootstrap = _exact_object(value, BOOTSTRAP_KEYS, "bootstrap manifest")
    if _integer(bootstrap["schemaVersion"], "bootstrap schemaVersion") != 1:
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
        if _integer(tool["versionExitCode"], f"{tool_name} versionExitCode") != 0:
            raise EvidenceError(f"{tool_name} version probe did not pass")
        _digest(tool["executableSha256"], f"{tool_name} executableSha256")
        _integer(tool["executableBytes"], f"{tool_name} executableBytes", minimum=1)
        _validate_stream(
            evidence,
            tool["versionStdout"],
            f"{tool_name}.version.stdout.log",
            expected_files,
            f"{tool_name} version stdout",
        )
        _validate_stream(
            evidence,
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
    if _integer(dependency["exitCode"], "dependency exitCode") != 0:
        raise EvidenceError("dependency install did not pass")
    started = _rfc3339_utc(dependency["startedAt"], "dependency startedAt")
    ended = _rfc3339_utc(dependency["endedAt"], "dependency endedAt")
    if started > ended:
        raise EvidenceError("dependency timestamps are inverted")
    _validate_stream(
        evidence,
        dependency["stdout"],
        "dependency-install.stdout.log",
        expected_files,
        "dependency stdout",
    )
    _validate_stream(
        evidence,
        dependency["stderr"],
        "dependency-install.stderr.log",
        expected_files,
        "dependency stderr",
    )


def _validate_command(
    evidence: _EvidenceSnapshot,
    manifest: dict[str, object],
    command: capture.CommandSpec,
    bindings: _GitBindings,
    platform_id: str,
    expected_files: set[str],
) -> bool:
    value = _exact_object(manifest, COMMAND_KEYS, f"{command.id} manifest")
    if _integer(value["schemaVersion"], f"{command.id} schemaVersion") != 1:
        raise EvidenceError(f"{command.id} schemaVersion differs")
    if value["commandId"] != command.id or value["phase"] != command.phase:
        raise EvidenceError(f"{command.id} matrix identity differs")
    expected_exit = _integer(value["expectedExit"], f"{command.id} expectedExit")
    if value["argv"] != list(command.argv) or expected_exit != command.expected_exit:
        raise EvidenceError(f"{command.id} matrix command differs")
    _validate_context(value, bindings, platform_id, command.id)
    started = _rfc3339_utc(value["startedAt"], f"{command.id} startedAt")
    ended = _rfc3339_utc(value["endedAt"], f"{command.id} endedAt")
    if started > ended:
        raise EvidenceError(f"{command.id} timestamps are inverted")
    exit_code = _integer(value["exitCode"], f"{command.id} exitCode")
    _validate_stream(
        evidence,
        value["stdout"],
        f"{command.id}.stdout.log",
        expected_files,
        f"{command.id} stdout",
    )
    _validate_stream(
        evidence,
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
            evidence,
            selection["stdout"],
            f"{command.id}.selection.stdout.log",
            expected_files,
            f"{command.id} selection stdout",
        )
        _validate_stream(
            evidence,
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


def _expected_evidence_names(
    commands: tuple[capture.CommandSpec, ...], mode: str
) -> tuple[set[str], set[str]]:
    required = {
        "host-bootstrap.manifest.json",
        "dependency-install.stdout.log",
        "dependency-install.stderr.log",
    }
    for tool_name in capture.REQUIRED_TOOL_NAMES:
        required.update(
            {
                f"{tool_name}.version.stdout.log",
                f"{tool_name}.version.stderr.log",
            }
        )
    for command in commands:
        required.update(
            {
                f"{command.id}.manifest.json",
                f"{command.id}.stdout.log",
                f"{command.id}.stderr.log",
            }
        )
        if capture.selection_argv(command) is not None:
            required.update(
                {
                    f"{command.id}.selection.stdout.log",
                    f"{command.id}.selection.stderr.log",
                }
            )
    return required, {f"{mode}-summary.json"}


def verify_evidence(request: VerificationRequest) -> EvidenceDisposition:
    return _verify_evidence(request, require_summary=False)


def _verify_evidence(
    request: VerificationRequest, *, require_summary: bool
) -> EvidenceDisposition:
    if request.platform not in PLATFORM_ARCHITECTURES:
        raise EvidenceError("unknown platform")
    if request.mode not in {"baseline", "post"}:
        raise EvidenceError("unknown evidence mode")
    lexical_root = Path(request.evidence_root)
    if (
        lexical_root.name != request.mode
        or lexical_root.parent.name != request.platform
    ):
        raise EvidenceError("evidence root layout differs from platform/mode")
    try:
        root = _absolute_directory(lexical_root, "evidence root")
    except UnsafeEvidenceError as error:
        raise ForbiddenEvidenceError(()) from error
    matrix_capture = _capture_matrix(request.matrix_path)
    matrix = matrix_capture[3]
    commands = matrix.required_for(request.platform, request.mode)
    if not commands:
        matrix_capture[1].validate_final_state()
        raise EvidenceError("matrix has no required commands")
    required_files, optional_files = _expected_evidence_names(commands, request.mode)
    evidence: _EvidenceSnapshot | None = None
    try:
        try:
            compatibility_matches = scan_forbidden_evidence(root)
        except ForbiddenEvidenceError:
            raise
        except EvidenceError as error:
            raise ForbiddenEvidenceError(()) from error
        try:
            evidence = _EvidenceSnapshot(root)
        except EvidenceError as error:
            raise ForbiddenEvidenceError(()) from error
        evidence.require_exact(required_files, optional_files)
        matches = tuple(sorted(set(compatibility_matches) | set(evidence.matches())))
        if matches:
            raise ForbiddenEvidenceError(matches)
        bootstrap = evidence.json("host-bootstrap.manifest.json", "bootstrap manifest")
        bindings, matrix = _bindings(
            request.repo_root,
            request.matrix_path,
            bootstrap.get("testedGitSha"),
            bootstrap.get("toolsGitSha"),
            preloaded_matrix=matrix_capture,
        )
        expected_files = {"host-bootstrap.manifest.json"}
        _validate_bootstrap(
            evidence, bootstrap, bindings, request.platform, expected_files
        )
        blocked: list[str] = []
        for command in commands:
            name = f"{command.id}.manifest.json"
            expected_files.add(name)
            manifest = evidence.json(name, f"{command.id} manifest")
            if _validate_command(
                evidence,
                manifest,
                command,
                bindings,
                request.platform,
                expected_files,
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
        summary_name = f"{request.mode}-summary.json"
        if expected_files != required_files:
            raise EvidenceError("evidence completeness differs from the matrix")
        expected_summary = canonical_json_bytes(disposition_dict(disposition)) + b"\n"
        if summary_name in evidence.files:
            if evidence.files[summary_name].payload != expected_summary:
                raise EvidenceError(
                    "existing summary differs from recomputed disposition"
                )
        elif require_summary:
            raise EvidenceError("required recomputed baseline summary is missing")
        if request.mode == "post":
            baseline = _verify_evidence(
                VerificationRequest(
                    request.repo_root,
                    request.matrix_path,
                    root.parent / "baseline",
                    request.platform,
                    "baseline",
                ),
                require_summary=True,
            )
            expected_baseline_ids = tuple(
                command.id
                for command in matrix.required_for(request.platform, "baseline")
            )
            if (
                baseline.platform != disposition.platform
                or baseline.tested_git_sha != disposition.tested_git_sha
                or baseline.tools_git_sha != disposition.tools_git_sha
                or baseline.matrix_sha256 != disposition.matrix_sha256
                or baseline.command_ids != expected_baseline_ids
            ):
                raise EvidenceError("sibling baseline evidence binding differs")
            baseline_blocked = set(baseline.blocked_ids)
            if any(command_id not in baseline_blocked for command_id in blocked):
                raise EvidenceError("post evidence contains an unpaired failure")
        return disposition
    finally:
        try:
            try:
                matrix_capture[1].validate_final_state()
            except UnsafeEvidenceError as error:
                raise ForbiddenEvidenceError(()) from error
        finally:
            try:
                if evidence is not None:
                    evidence.validate_final_state()
            except UnsafeEvidenceError as error:
                raise ForbiddenEvidenceError(()) from error
            finally:
                if evidence is not None:
                    evidence.close()


def _safe_public_value(value: object, label: str) -> None:
    if isinstance(value, dict):
        for key, item in value.items():
            if not isinstance(key, str):
                raise EvidenceError(f"{label} contains a non-string key")
            normalized_key = key.casefold().replace("_", "").replace("-", "")
            if (
                key.casefold() in PRIVATE_PUBLIC_KEYS
                or normalized_key in PRIVATE_CREDENTIAL_KEYS
            ):
                raise EvidenceError(f"{label} contains a private-material key")
            _safe_public_value(key, f"{label} key")
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
    if (
        value.startswith("/")
        or value.startswith("\\")
        or re.match(r"(?i)^[A-Z]:[\\/]", value)
    ):
        raise EvidenceError(f"{label} contains an absolute path")
    if PRIVATE_PUBLIC_VALUE_PATTERN.search(value) is not None:
        raise EvidenceError(f"{label} contains private material")


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


def _validate_report(
    path: Path, repo: Path, candidate_sha: str, payload: bytes
) -> dict[str, object]:
    report = _exact_object(
        _load_json_bytes(payload, "selected report"), REPORT_KEYS, "selected report"
    )
    _safe_public_value(report, "selected report")
    if (
        _integer(report["schemaVersion"], "selected report schemaVersion") != 1
        or report["forkSha"] != candidate_sha
    ):
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
    payload: bytes, report: Mapping[str, object], candidate_sha: str, report_sha: str
) -> tuple[dict[str, object], dict[str, object]]:
    index = _exact_object(
        _load_json_bytes(payload, "report index"), INDEX_KEYS, "report index"
    )
    if _integer(index["schemaVersion"], "report index schemaVersion") != 1:
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
        if _integer(attempt["attemptOrdinal"], "attemptOrdinal", minimum=1) != ordinal:
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
    payload: bytes,
    report: Mapping[str, object],
    chosen: Mapping[str, object],
    candidate_sha: str,
    report_sha: str,
    index_sha: str,
) -> dict[str, object]:
    receipt = _exact_object(
        _load_json_bytes(payload, "business verification receipt"),
        BUSINESS_RECEIPT_KEYS,
        "business verification receipt",
    )
    _safe_public_value(receipt, "business verification receipt")
    if _integer(receipt["schemaVersion"], "business receipt schemaVersion") != 1:
        raise EvidenceError("business receipt schemaVersion differs")
    selected_ordinal = _integer(
        receipt["selectedAttemptOrdinal"], "business receipt selectedAttemptOrdinal"
    )
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
        actual = selected_ordinal if key == "selectedAttemptOrdinal" else receipt[key]
        if actual != value:
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
    generated = _rfc3339_utc(report["generatedAt"], "report generatedAt")
    deadline = _rfc3339_utc(
        report["privateEvidenceRetentionDeadline"], "retention deadline"
    )
    if (
        generated > verified
        or verified >= deadline
        or receipt["verifiedBeforeRetentionDeadline"] is not True
    ):
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
    try:
        report_path = _absolute_file(request.selected_report, "selected report")
        index_path = _absolute_file(request.report_index, "report index")
        receipt_path = _absolute_file(
            request.business_verification_receipt,
            "business verification receipt",
        )
    except UnsafeEvidenceError as error:
        raise ForbiddenEvidenceError(()) from error
    if index_path != repo / "docs/evidence/business-proof/index.json":
        raise EvidenceError("report index path differs")
    try:
        consumed = (
            _FileSnapshot.capture(
                report_path, "selected-report.json", keep_payload=True
            ),
            _FileSnapshot.capture(index_path, "report-index.json", keep_payload=True),
            _FileSnapshot.capture(
                receipt_path,
                "business-verification-receipt.json",
                keep_payload=True,
            ),
        )
    except UnsafeEvidenceError as error:
        raise ForbiddenEvidenceError(()) from error
    matches = tuple(sorted(match for item in consumed for match in item.matches))
    if matches:
        raise ForbiddenEvidenceError(matches)
    report_payload, index_payload, receipt_payload = (item.payload for item in consumed)
    if report_payload is None or index_payload is None or receipt_payload is None:
        raise EvidenceError("internal public evidence snapshot differs")
    report = _validate_report(report_path, repo, candidate_sha, report_payload)
    report_sha = consumed[0].sha256
    index, chosen = _validate_index(index_payload, report, candidate_sha, report_sha)
    del index
    index_sha = consumed[1].sha256
    receipt = _validate_receipt(
        receipt_payload, report, chosen, candidate_sha, report_sha, index_sha
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
            "liveProofVerificationSha256": consumed[2].sha256,
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
    try:
        for item in consumed:
            item.validate_final_state()
    except UnsafeEvidenceError as error:
        raise ForbiddenEvidenceError(()) from error
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
    try:
        temp = capture._write_temp_bytes(path, payload)
    except OSError as error:
        raise EvidenceError("unable to reserve summary temporary file") from error
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
            raise EvidenceError("unable to publish summary output") from error
        raise


class _UniqueOption(argparse.Action):
    def __call__(
        self,
        parser: argparse.ArgumentParser,
        namespace: argparse.Namespace,
        values: object,
        option_string: str | None = None,
    ) -> None:
        if getattr(namespace, self.dest, None) is not None:
            parser.error(f"{option_string} may not be repeated")
        setattr(namespace, self.dest, values)


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", action=_UniqueOption)
    parser.add_argument("--matrix", action=_UniqueOption)
    parser.add_argument("--evidence-root", action=_UniqueOption)
    parser.add_argument(
        "--platform", choices=sorted(PLATFORM_ARCHITECTURES), action=_UniqueOption
    )
    parser.add_argument("--mode", choices=["baseline", "post"], action=_UniqueOption)
    parser.add_argument("--summary-output", action=_UniqueOption)
    parser.add_argument("--candidate-sha", action=_UniqueOption)
    parser.add_argument("--selected-report", action=_UniqueOption)
    parser.add_argument("--report-index", action=_UniqueOption)
    parser.add_argument("--business-verification-receipt", action=_UniqueOption)
    parser.add_argument("--verification-output", action=_UniqueOption)
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = _parser()
    raw_arguments = list(sys.argv[1:] if argv is None else argv)
    arguments = parser.parse_args(raw_arguments)
    final_names = (
        "candidate_sha",
        "selected_report",
        "report_index",
        "business_verification_receipt",
        "verification_output",
    )
    final_values = [getattr(arguments, name) for name in final_names]
    foundation_names = ("repo_root", "matrix", "evidence_root")
    foundation_values = [getattr(arguments, name) for name in foundation_names]
    baseline_options = {"--platform", "--mode", "--summary-output"}
    final_options = {
        "--candidate-sha",
        "--selected-report",
        "--report-index",
        "--business-verification-receipt",
        "--verification-output",
    }
    first_baseline = min(
        (
            raw_arguments.index(item)
            for item in baseline_options
            if item in raw_arguments
        ),
        default=len(raw_arguments) + 1,
    )
    first_final = min(
        (raw_arguments.index(item) for item in final_options if item in raw_arguments),
        default=len(raw_arguments) + 1,
    )
    if any(final_values) and first_baseline < first_final:
        parser.error("final-only arguments are invalid in baseline/post mode")
    if any(final_values):
        if any(
            value is not None
            for value in (arguments.platform, arguments.mode, arguments.summary_output)
        ):
            parser.error("baseline-only arguments are invalid in frozen-final mode")
        if not all(final_values):
            parser.error("the five frozen-final arguments are required together")
        if not all(foundation_values):
            parser.error("the three public foundation arguments are required")
    path_names = [
        "repo_root",
        "matrix",
        "evidence_root",
        "summary_output",
        "selected_report",
        "report_index",
        "business_verification_receipt",
        "verification_output",
    ]
    for name in path_names:
        value = getattr(arguments, name)
        if value is not None and not Path(value).is_absolute():
            parser.error(f"--{name.replace('_', '-')} must be absolute")
    try:
        if all(final_values):
            repo = Path(arguments.repo_root)
            matrix = Path(arguments.matrix)
            foundation = Path(arguments.evidence_root)
            verify_frozen_final(
                FinalVerificationRequest(
                    repo,
                    matrix,
                    foundation / "macos-x86_64/post",
                    foundation / "windows-11-x64/post",
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
