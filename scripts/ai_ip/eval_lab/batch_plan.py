import hashlib
import os
import re
import stat
import tomllib
from dataclasses import dataclass
from pathlib import Path

try:
    from .batch_contracts import (
        BatchContractError,
        seal_self_commitment,
        validate_named_contract,
        verify_self_commitment,
    )
    from .contracts import sha256_json
except ImportError:
    from batch_contracts import (
        BatchContractError,
        seal_self_commitment,
        validate_named_contract,
        verify_self_commitment,
    )
    from contracts import sha256_json


_REPO_ROOT = Path(__file__).resolve().parents[3]
_LEAD_SKILL_NAME = "deliver-ai-ip-content-package"
_LEAD_SKILL_PATH = (
    _REPO_ROOT / "ai-ip-assets" / "skills" / _LEAD_SKILL_NAME / "SKILL.md"
)
_MAX_FILE_BYTES = 8 * 1024 * 1024
_MAX_TREE_BYTES = 64 * 1024 * 1024
_CHUNK_BYTES = 1024 * 1024
_FORBIDDEN_SEED_TERMS = (
    b"goldengift",
    b"rubric",
    b"caseanswer",
    b"referencedossier",
    b"outcomepacket",
    b"scores",
    b"hiddeninstructions",
)

PARITY_FIELDS = (
    "caseBundleSha256",
    "caseAnswerSchemaId",
    "modelRouteRef",
    "executionProfileRef",
    "workspaceTemplateSha256",
    "promptfooPackageVersion",
    "promptfooLockSha256",
    "timeoutBudget",
    "tokenBudget",
    "requestBudget",
    "costBudgetCny",
    "networkPolicy",
    "permissionPolicy",
    "toolPolicy",
)


class BatchPlanError(ValueError):
    pass


def _file_state(metadata: os.stat_result) -> tuple[int, int, int, int, int, int, int]:
    return (
        metadata.st_dev,
        metadata.st_ino,
        metadata.st_mode,
        metadata.st_nlink,
        metadata.st_size,
        metadata.st_mtime_ns,
        metadata.st_ctime_ns,
    )


def _require_regular_file(metadata: os.stat_result, display: str) -> None:
    if stat.S_ISLNK(metadata.st_mode):
        raise BatchPlanError(f"symbolic link is not allowed: {display}")
    if not stat.S_ISREG(metadata.st_mode):
        raise BatchPlanError(f"regular file required: {display}")
    if metadata.st_nlink != 1:
        raise BatchPlanError(f"hard link is not allowed: {display}")
    if metadata.st_size > _MAX_FILE_BYTES:
        raise BatchPlanError(f"file exceeds 8 MiB: {display}")


def _stable_file(path: Path) -> tuple[str, int, int, bytes]:
    target = Path(path)
    try:
        before = target.lstat()
        _require_regular_file(before, str(target))
        flags = os.O_RDONLY | getattr(os, "O_BINARY", 0) | getattr(os, "O_NOFOLLOW", 0)
        descriptor = os.open(target, flags)
    except BatchPlanError:
        raise
    except OSError as error:
        raise BatchPlanError(f"cannot read file: {target}") from error

    try:
        opened = os.fstat(descriptor)
        _require_regular_file(opened, str(target))
        if _file_state(before) != _file_state(opened):
            raise BatchPlanError(f"file changed while opening: {target}")
        digest = hashlib.sha256()
        chunks: list[bytes] = []
        remaining = before.st_size
        while remaining:
            chunk = os.read(descriptor, min(_CHUNK_BYTES, remaining))
            if not chunk or len(chunk) > remaining:
                raise BatchPlanError(f"file changed while read: {target}")
            digest.update(chunk)
            chunks.append(chunk)
            remaining -= len(chunk)
        if os.read(descriptor, 1):
            raise BatchPlanError(f"file changed while read: {target}")
        after = os.fstat(descriptor)
    except OSError as error:
        raise BatchPlanError(f"cannot read file: {target}") from error
    finally:
        os.close(descriptor)

    try:
        final = target.lstat()
    except OSError as error:
        raise BatchPlanError(f"file changed while read: {target}") from error
    if _file_state(before) != _file_state(after) or _file_state(before) != _file_state(
        final
    ):
        raise BatchPlanError(f"file changed while read: {target}")
    return (
        digest.hexdigest(),
        before.st_size,
        stat.S_IMODE(before.st_mode),
        b"".join(chunks),
    )


def sha256_file(path: Path) -> str:
    """Return the digest of one bounded, stable regular file."""
    return _stable_file(Path(path))[0]


def _safe_relative_path(relative: Path) -> str:
    if relative.is_absolute() or not relative.parts:
        raise BatchPlanError("tree entry path must be a non-empty relative path")
    if any(part in {".", "..", ""} for part in relative.parts):
        raise BatchPlanError("tree entry path must not contain dot components")
    return relative.as_posix()


@dataclass(frozen=True)
class TreeSnapshot:
    digest: str
    files: dict[str, tuple[str, bytes]]


def _descriptor_traversal_available() -> bool:
    return os.open in os.supports_dir_fd and os.stat in os.supports_dir_fd


def _fallback_snapshot_tree(root: Path) -> TreeSnapshot:
    """Capture a Windows-compatible tree with complete before/after identities."""
    try:
        root_state = root.lstat()
    except OSError as error:
        raise BatchPlanError("tree root is unavailable") from error
    if (
        stat.S_ISLNK(root_state.st_mode)
        or not stat.S_ISDIR(root_state.st_mode)
        or getattr(root_state, "st_file_attributes", 0) & 0x400
    ):
        raise BatchPlanError("tree root must be a non-reparse directory")
    states: dict[Path, os.stat_result] = {root: root_state}
    file_states: dict[Path, os.stat_result] = {}
    entries, files = [], {}
    total = 0
    pending = [root]
    try:
        while pending:
            current = pending.pop()
            for child in sorted(os.scandir(current), key=lambda entry: entry.name):
                name, path = child.name, current / child.name
                metadata = path.lstat()
                relative = _safe_relative_path(path.relative_to(root))
                if (
                    stat.S_ISLNK(metadata.st_mode)
                    or getattr(metadata, "st_file_attributes", 0) & 0x400
                ):
                    raise BatchPlanError(
                        f"symbolic link or reparse point is not allowed: {relative}"
                    )
                if stat.S_ISDIR(metadata.st_mode):
                    states[path] = metadata
                    pending.append(path)
                    continue
                _require_regular_file(metadata, relative)
                digest, size, mode, payload = _stable_file(path)
                file_states[path] = metadata
                total += size
                if total > _MAX_TREE_BYTES:
                    raise BatchPlanError("tree exceeds 64 MiB")
                files[relative] = (digest, payload)
                entries.append(
                    {"mode": mode, "path": relative, "sha256": digest, "size": size}
                )
    except OSError as error:
        raise BatchPlanError("tree changed during traversal") from error
    if any(
        _file_state(before) != _file_state(path.lstat())
        for path, before in states.items()
    ) or any(
        _file_state(before) != _file_state(path.lstat())
        for path, before in file_states.items()
    ):
        raise BatchPlanError("tree changed during traversal")
    return TreeSnapshot(
        sha256_json({"entries": sorted(entries, key=lambda entry: str(entry["path"]))}),
        files,
    )


def snapshot_tree(root: Path) -> TreeSnapshot:
    """Capture a descriptor-bound regular-file tree without following links."""
    tree_root = Path(root)
    if not _descriptor_traversal_available():
        return _fallback_snapshot_tree(tree_root)
    try:
        before = tree_root.lstat()
        if stat.S_ISLNK(before.st_mode) or not stat.S_ISDIR(before.st_mode):
            raise BatchPlanError("tree root must be a non-symlink directory")
        flags = (
            os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_NOFOLLOW", 0)
        )
        root_fd = os.open(tree_root, flags)
    except BatchPlanError:
        raise
    except OSError as error:
        raise BatchPlanError("cannot securely open tree root") from error
    if _file_state(before) != _file_state(os.fstat(root_fd)):
        os.close(root_fd)
        raise BatchPlanError("tree changed while opening")
    pending = [(root_fd, (), before)]
    opened = [root_fd]
    directories = [(tree_root, before)]
    entries: list[dict[str, object]] = []
    files: dict[str, tuple[str, bytes]] = {}
    total = 0
    try:
        while pending:
            directory_fd, prefix, state = pending.pop()
            children = sorted(os.scandir(directory_fd), key=lambda entry: entry.name)
            for child in children:
                name = child.name
                relative = Path(*prefix, name)
                display = _safe_relative_path(relative)
                metadata = os.stat(name, dir_fd=directory_fd, follow_symlinks=False)
                if (
                    stat.S_ISLNK(metadata.st_mode)
                    or getattr(metadata, "st_file_attributes", 0) & 0x400
                ):
                    raise BatchPlanError(
                        f"symbolic link or reparse point is not allowed: {display}"
                    )
                if stat.S_ISDIR(metadata.st_mode):
                    flags = (
                        os.O_RDONLY
                        | getattr(os, "O_DIRECTORY", 0)
                        | getattr(os, "O_NOFOLLOW", 0)
                    )
                    child_fd = os.open(name, flags, dir_fd=directory_fd)
                    if _file_state(metadata) != _file_state(os.fstat(child_fd)):
                        os.close(child_fd)
                        raise BatchPlanError("tree changed while opening")
                    opened.append(child_fd)
                    directories.append((tree_root / relative, metadata))
                    pending.append((child_fd, (*prefix, name), metadata))
                    continue
                _require_regular_file(metadata, display)
                fd = os.open(
                    name,
                    os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0),
                    dir_fd=directory_fd,
                )
                try:
                    opened_file = os.fstat(fd)
                    if _file_state(metadata) != _file_state(opened_file):
                        raise BatchPlanError("file changed while opening")
                    remaining = metadata.st_size
                    chunks = []
                    while remaining:
                        chunk = os.read(fd, min(_CHUNK_BYTES, remaining))
                        if not chunk or len(chunk) > remaining:
                            raise BatchPlanError("file changed while read")
                        chunks.append(chunk)
                        remaining -= len(chunk)
                    if os.read(fd, 1):
                        raise BatchPlanError("file changed while read")
                    payload = b"".join(chunks)
                    after = os.fstat(fd)
                finally:
                    os.close(fd)
                if len(payload) != metadata.st_size or _file_state(
                    metadata
                ) != _file_state(after):
                    raise BatchPlanError("file changed while read")
                total += len(payload)
                if total > _MAX_TREE_BYTES:
                    raise BatchPlanError("tree exceeds 64 MiB")
                digest = hashlib.sha256(payload).hexdigest()
                files[display] = (digest, payload)
                entries.append(
                    {
                        "mode": stat.S_IMODE(metadata.st_mode),
                        "path": display,
                        "sha256": digest,
                        "size": len(payload),
                    }
                )
            if _file_state(state) != _file_state(os.fstat(directory_fd)):
                raise BatchPlanError("tree changed during traversal")
        if any(
            _file_state(state) != _file_state(path.lstat())
            for path, state in directories
        ):
            raise BatchPlanError("tree changed during traversal")
    except OSError as error:
        raise BatchPlanError("tree changed during traversal") from error
    finally:
        for fd in opened:
            os.close(fd)
    return TreeSnapshot(
        sha256_json({"entries": sorted(entries, key=lambda entry: str(entry["path"]))}),
        files,
    )


def sha256_tree(root: Path) -> str:
    return snapshot_tree(root).digest


def _validate_contract(name: str, value: object) -> dict[str, object]:
    if type(value) is not dict:
        raise BatchPlanError(f"{name} must be an object")
    try:
        validate_named_contract(name, value)
    except BatchContractError as error:
        raise BatchPlanError(f"{name} contract validation failed: {error}") from error
    return value


def verify_binary_manifest(manifest: object, binary_path: Path) -> None:
    """Verify a binary manifest against freshly hashed candidate bytes."""
    value = _validate_contract("binary-manifest", manifest)
    actual = sha256_file(binary_path)
    if actual != value["binarySha256"]:
        raise BatchPlanError("binary SHA-256 differs from manifest")


def _verify_seed_material(
    snapshot: TreeSnapshot, declared_capabilities: object
) -> None:
    if type(declared_capabilities) is not list:
        raise BatchPlanError("declared capabilities must be a list")
    lead_path = f"skills/{_LEAD_SKILL_NAME}/SKILL.md"
    lead_digest = sha256_file(_LEAD_SKILL_PATH)
    declares_lead_skill = _LEAD_SKILL_NAME in declared_capabilities
    if declares_lead_skill:
        try:
            config = tomllib.loads(snapshot.files["config.toml"][1].decode("utf-8"))[
                "lead_skill"
            ]
        except (KeyError, UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
            raise BatchPlanError("Lead Skill config is missing") from error
        if (
            config != {"path": lead_path, "sha256": lead_digest}
            or snapshot.files.get(lead_path, (None,))[0] != lead_digest
        ):
            raise BatchPlanError("declared Lead Skill is missing from seed")
    elif any(digest == lead_digest for digest, _ in snapshot.files.values()):
        raise BatchPlanError("undeclared Lead Skill is present in seed")
    for path, (_, payload) in snapshot.files.items():
        normalized = re.sub(rb"[^a-z0-9]+", b"", path.lower().encode())
        content = re.sub(rb"[^a-z0-9]+", b"", payload.lower())
        if any(term in normalized or term in content for term in _FORBIDDEN_SEED_TERMS):
            raise BatchPlanError("case-specific material is present in seed")


def verify_treatment_manifest(
    manifest: object,
    *,
    codex_home_seed: Path,
    system_instruction: Path,
    capability_bundle: Path,
    effective_config: Path,
    binary_manifest: object,
    binary_path: Path,
) -> None:
    """Verify every treatment artifact, its commitment, and seed hygiene."""
    value = _validate_contract("treatment-manifest", manifest)
    try:
        verify_self_commitment(value, "treatmentManifestSha256")
    except BatchContractError as error:
        raise BatchPlanError("treatment commitment mismatch") from error
    verify_binary_manifest(binary_manifest, binary_path)
    if value["binaryManifestRef"] != binary_manifest["binaryId"]:
        raise BatchPlanError("binary manifest reference differs from verified binary")
    seed_snapshot = snapshot_tree(codex_home_seed)
    expected_artifacts = (
        ("codexHomeSeedSha256", seed_snapshot.digest, "codex home seed"),
        (
            "systemInstructionSha256",
            sha256_file(system_instruction),
            "system instruction",
        ),
        ("capabilityBundleSha256", sha256_tree(capability_bundle), "capability bundle"),
        (
            "effectiveCodexConfigSha256",
            sha256_file(effective_config),
            "effective config",
        ),
    )
    for field, actual, label in expected_artifacts:
        if actual != value[field]:
            raise BatchPlanError(f"{label} SHA-256 differs from manifest")
    _verify_seed_material(seed_snapshot, value["declaredCapabilities"])


def _verify_plan_refs(
    plan: dict[str, object], stock_treatment: object, modified_treatment: object
) -> None:
    stock = _validate_contract("treatment-manifest", stock_treatment)
    modified = _validate_contract("treatment-manifest", modified_treatment)
    for treatment in (stock, modified):
        try:
            verify_self_commitment(treatment, "treatmentManifestSha256")
        except BatchContractError as error:
            raise BatchPlanError("treatment commitment mismatch") from error
    if plan["stockTreatmentRef"] != stock["treatmentId"]:
        raise BatchPlanError(
            "stock treatment reference differs from verified treatment"
        )
    if plan["modifiedTreatmentRef"] != modified["treatmentId"]:
        raise BatchPlanError(
            "modified treatment reference differs from verified treatment"
        )


def seal_candidate_run_plan(
    plan: object, *, stock_treatment: object, modified_treatment: object
) -> dict[str, object]:
    """Return a schema-valid CandidateRunPlan with a fresh self-commitment."""
    value = _validate_contract("candidate-run-plan", plan)
    _verify_plan_refs(value, stock_treatment, modified_treatment)
    try:
        sealed = seal_self_commitment(value, "planSha256")
    except BatchContractError as error:
        raise BatchPlanError("cannot seal plan commitment") from error
    _validate_contract("candidate-run-plan", sealed)
    return sealed


def verify_candidate_run_plan(
    plan: object, *, stock_treatment: object, modified_treatment: object
) -> None:
    """Require a schema-valid CandidateRunPlan whose full payload is committed."""
    value = _validate_contract("candidate-run-plan", plan)
    _verify_plan_refs(value, stock_treatment, modified_treatment)
    try:
        verify_self_commitment(value, "planSha256")
    except BatchContractError as error:
        raise BatchPlanError("plan commitment mismatch") from error


def verify_effective_condition_parity(stock: object, modified: object) -> str:
    """Commit the runtime fields that must be identical across both arms."""
    if type(stock) is not dict or type(modified) is not dict:
        raise BatchPlanError("effective conditions must be objects")
    parity: dict[str, object] = {}
    for field in PARITY_FIELDS:
        if field not in stock or field not in modified:
            raise BatchPlanError(f"missing parity field: {field}")
        if stock[field] != modified[field]:
            raise BatchPlanError(f"condition parity differs: {field}")
        parity[field] = stock[field]
    allowed = {
        "stockTreatmentRef",
        "modifiedTreatmentRef",
        "privateArmPath",
        "executionOrder",
    }
    for field in sorted(set(stock) | set(modified)):
        if (
            field not in PARITY_FIELDS
            and field not in allowed
            and (
                (field in stock) != (field in modified)
                or stock.get(field) != modified.get(field)
            )
        ):
            raise BatchPlanError(f"undeclared condition difference: {field}")
    return sha256_json(parity)
