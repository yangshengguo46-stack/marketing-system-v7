import hashlib
import os
import stat
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
    b"golden-gift",
    b"rubric",
    b"case answer",
    b"review rubric",
    b"reference dossier",
    b"outcome packet",
    b"case-specific hidden",
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
    if _file_state(before) != _file_state(after) or _file_state(before) != _file_state(final):
        raise BatchPlanError(f"file changed while read: {target}")
    return digest.hexdigest(), before.st_size, stat.S_IMODE(before.st_mode), b"".join(chunks)


def sha256_file(path: Path) -> str:
    """Return the digest of one bounded, stable regular file."""
    return _stable_file(Path(path))[0]


def _safe_relative_path(relative: Path) -> str:
    if relative.is_absolute() or not relative.parts:
        raise BatchPlanError("tree entry path must be a non-empty relative path")
    if any(part in {".", "..", ""} for part in relative.parts):
        raise BatchPlanError("tree entry path must not contain dot components")
    return relative.as_posix()


def sha256_tree(root: Path) -> str:
    """Hash a bounded tree of regular files with canonical relative entries."""
    tree_root = Path(root)
    try:
        root_metadata = tree_root.lstat()
    except OSError as error:
        raise BatchPlanError(f"cannot inspect tree root: {tree_root}") from error
    if stat.S_ISLNK(root_metadata.st_mode):
        raise BatchPlanError(f"symbolic link is not allowed: {tree_root}")
    if not stat.S_ISDIR(root_metadata.st_mode):
        raise BatchPlanError(f"tree root must be a directory: {tree_root}")

    entries: list[dict[str, object]] = []
    pending = [tree_root]
    total_bytes = 0
    while pending:
        directory = pending.pop()
        try:
            children = sorted(os.scandir(directory), key=lambda entry: entry.name)
        except OSError as error:
            raise BatchPlanError(f"cannot enumerate tree: {directory}") from error
        for child in children:
            path = Path(child.path)
            relative = path.relative_to(tree_root)
            display = _safe_relative_path(relative)
            try:
                metadata = path.lstat()
            except OSError as error:
                raise BatchPlanError(f"cannot inspect tree entry: {display}") from error
            if stat.S_ISLNK(metadata.st_mode):
                raise BatchPlanError(f"symbolic link is not allowed: {display}")
            if stat.S_ISDIR(metadata.st_mode):
                pending.append(path)
                continue
            _require_regular_file(metadata, display)
            digest, size, mode, _ = _stable_file(path)
            total_bytes += size
            if total_bytes > _MAX_TREE_BYTES:
                raise BatchPlanError("tree exceeds 64 MiB")
            entries.append(
                {"mode": mode, "path": display, "sha256": digest, "size": size}
            )
    entries.sort(key=lambda entry: str(entry["path"]))
    return sha256_json({"entries": entries})


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


def _verify_seed_material(seed: Path, declared_capabilities: object) -> None:
    if type(declared_capabilities) is not list:
        raise BatchPlanError("declared capabilities must be a list")
    lead_skill = seed / "skills" / _LEAD_SKILL_NAME / "SKILL.md"
    declares_lead_skill = _LEAD_SKILL_NAME in declared_capabilities
    if declares_lead_skill:
        if not lead_skill.is_file() or lead_skill.is_symlink():
            raise BatchPlanError("declared Lead Skill is missing from seed")
        if _stable_file(lead_skill)[3] != _stable_file(_LEAD_SKILL_PATH)[3]:
            raise BatchPlanError("Lead Skill differs from repository asset")
    elif lead_skill.exists() or lead_skill.is_symlink():
        raise BatchPlanError("undeclared Lead Skill is present in seed")

    pending = [seed]
    while pending:
        directory = pending.pop()
        for child in sorted(directory.iterdir(), key=lambda path: path.name):
            lower_name = child.name.lower().encode("utf-8")
            if any(term in lower_name for term in _FORBIDDEN_SEED_TERMS):
                raise BatchPlanError("case-specific material is present in seed")
            metadata = child.lstat()
            if stat.S_ISLNK(metadata.st_mode):
                raise BatchPlanError("case-specific seed scan found a symbolic link")
            if stat.S_ISDIR(metadata.st_mode):
                pending.append(child)
            else:
                _, _, _, payload = _stable_file(child)
                if any(term in payload.lower() for term in _FORBIDDEN_SEED_TERMS):
                    raise BatchPlanError("case-specific material is present in seed")


def verify_treatment_manifest(
    manifest: object,
    *,
    codex_home_seed: Path,
    system_instruction: Path,
    capability_bundle: Path,
    effective_config: Path,
) -> None:
    """Verify every treatment artifact, its commitment, and seed hygiene."""
    value = _validate_contract("treatment-manifest", manifest)
    try:
        verify_self_commitment(value, "treatmentManifestSha256")
    except BatchContractError as error:
        raise BatchPlanError("treatment commitment mismatch") from error
    expected_artifacts = (
        ("codexHomeSeedSha256", sha256_tree(codex_home_seed), "codex home seed"),
        ("systemInstructionSha256", sha256_file(system_instruction), "system instruction"),
        ("capabilityBundleSha256", sha256_tree(capability_bundle), "capability bundle"),
        ("effectiveCodexConfigSha256", sha256_file(effective_config), "effective config"),
    )
    for field, actual, label in expected_artifacts:
        if actual != value[field]:
            raise BatchPlanError(f"{label} SHA-256 differs from manifest")
    _verify_seed_material(Path(codex_home_seed), value["declaredCapabilities"])


def seal_candidate_run_plan(plan: object) -> dict[str, object]:
    """Return a schema-valid CandidateRunPlan with a fresh self-commitment."""
    value = _validate_contract("candidate-run-plan", plan)
    try:
        sealed = seal_self_commitment(value, "planSha256")
    except BatchContractError as error:
        raise BatchPlanError("cannot seal plan commitment") from error
    _validate_contract("candidate-run-plan", sealed)
    return sealed


def verify_candidate_run_plan(plan: object) -> None:
    """Require a schema-valid CandidateRunPlan whose full payload is committed."""
    value = _validate_contract("candidate-run-plan", plan)
    try:
        verify_self_commitment(value, "planSha256")
    except BatchContractError as error:
        raise BatchPlanError("plan commitment mismatch") from error


def verify_effective_condition_parity(
    stock: object, modified: object
) -> str:
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
    return sha256_json(parity)
