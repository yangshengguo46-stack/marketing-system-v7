"""Private preflight, derivation, and classification helpers for Task 4."""

import hashlib
import hmac
import json
import secrets
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Mapping

try:
    from .batch_contracts import BatchContractError, validate_named_contract
    from .batch_plan import (
        BatchPlanError,
        sha256_file,
        sha256_tree,
        verify_treatment_manifest,
        verify_binary_manifest,
        verify_candidate_run_plan,
        verify_effective_condition_parity,
    )
    from .batch_controller_snapshots import FrozenFile, FrozenJson, FrozenTree, SnapshotError
    from .contracts import (
        LabContractError,
        canonical_json_bytes,
        load_exact_json,
        sha256_json,
        validate_contract,
    )
except ImportError:
    from batch_contracts import BatchContractError, validate_named_contract
    from batch_plan import (
        BatchPlanError,
        sha256_file,
        sha256_tree,
        verify_treatment_manifest,
        verify_binary_manifest,
        verify_candidate_run_plan,
        verify_effective_condition_parity,
    )
    from batch_controller_snapshots import FrozenFile, FrozenJson, FrozenTree, SnapshotError
    from contracts import (
        LabContractError,
        canonical_json_bytes,
        load_exact_json,
        sha256_json,
        validate_contract,
    )


class ControllerSupportError(ValueError):
    pass


@dataclass(frozen=True)
class ArmBinding:
    private_arm_id: str
    arm_class: str
    treatment_manifest: dict[str, object]
    binary_manifest: dict[str, object]
    binary_manifest_sha256: str
    binary: FrozenFile
    codex_home_seed: FrozenTree
    effective_config_sha256: str
    effective_conditions: dict[str, object]


@dataclass(frozen=True)
class ValidatedBindings:
    plan: dict[str, object]
    stock: ArmBinding
    modified: ArmBinding
    attempt_base: Path
    workspace_seed: FrozenTree
    execution_profile: dict[str, object]
    source_environment: dict[str, str]
    case_bundle: object
    case_answer_schema: object
    case_answer_schema_file: FrozenFile
    model_route: Mapping[str, object]
    protocol: FrozenFile
    protocol_sha256: str
    promptfoo_config: FrozenFile
    promptfoo_config_sha256: str


def require_mapping(value: object, name: str) -> dict[str, object]:
    if type(value) is not dict:
        raise ControllerSupportError(f"{name} must be an object")
    return value


def _text(value: object, name: str) -> str:
    if type(value) is not str or not value:
        raise ControllerSupportError(f"{name} must be a non-empty string")
    return value


def _sha256(value: object, name: str) -> str:
    text = _text(value, name)
    if len(text) != 64 or any(
        character not in "0123456789abcdef" for character in text
    ):
        raise ControllerSupportError(f"{name} must be a lowercase SHA-256")
    return text


def _path(value: object, name: str) -> Path:
    if not isinstance(value, Path):
        raise ControllerSupportError(f"{name} must be a Path")
    return value


def _arm_binding(value: object, name: str, arm_class: str) -> ArmBinding:
    arm = require_mapping(value, f"{name} binding")
    private_arm_id = _text(arm.get("privateArmId"), "private arm ID")
    if any(
        token in private_arm_id.casefold() for token in ("stock", "modified", "/", "\\")
    ):
        raise ControllerSupportError("private arm ID must be opaque")
    treatment = freeze_json(require_mapping(arm.get("treatmentManifest"), "treatment manifest"))
    binary_manifest = freeze_json(require_mapping(arm.get("binaryManifest"), "binary manifest"))
    conditions = freeze_json(require_mapping(arm.get("effectiveConditions"), "effective conditions"))
    binary_path = _path(arm.get("binaryPath"), "binary path")
    seed_path = _path(arm.get("codexHomeSeed"), "Codex home seed")
    system_path = _path(arm.get("systemInstruction"), "system instruction")
    capability_path = _path(arm.get("capabilityBundle"), "capability bundle")
    config_path = _path(arm.get("effectiveConfig"), "effective config")
    try:
        verify_binary_manifest(binary_manifest, binary_path)
        verify_treatment_manifest(
            treatment,
            codex_home_seed=seed_path,
            system_instruction=system_path,
            capability_bundle=capability_path,
            effective_config=config_path,
            binary_manifest=binary_manifest,
            binary_path=binary_path,
        )
        binary = FrozenFile.capture(binary_path)
        seed = FrozenTree.capture(seed_path)
    except (BatchPlanError, SnapshotError) as error:
        raise ControllerSupportError(str(error)) from error
    return ArmBinding(
        private_arm_id,
        arm_class,
        treatment,
        binary_manifest,
        _sha256(arm.get("binaryManifestSha256"), "binary manifest SHA-256"),
        binary,
        seed,
        _sha256(arm.get("effectiveConfigSha256"), "effective config SHA-256"),
        conditions,
    )


def validate_bindings(plan: dict[str, object], value: object) -> ValidatedBindings:
    plan = freeze_json(plan)
    assert type(plan) is dict
    bindings = require_mapping(value, "candidate bindings")
    if bindings.get("retryPolicy") != {"maxAttemptsPerArm": 1}:
        raise ControllerSupportError("controller requires maxAttemptsPerArm=1")
    if plan.get("retryPolicy") != {"maxRetries": 0}:
        raise ControllerSupportError("sealed plan must prohibit retries")
    stock = _arm_binding(bindings.get("stock"), "stock", "stock")
    modified = _arm_binding(bindings.get("modified"), "modified", "modified")
    if stock.private_arm_id == modified.private_arm_id:
        raise ControllerSupportError("private arm IDs must differ")
    try:
        verify_candidate_run_plan(
            plan,
            stock_treatment=stock.treatment_manifest,
            modified_treatment=modified.treatment_manifest,
        )
        verify_effective_condition_parity(
            stock.effective_conditions, modified.effective_conditions
        )
        for arm, arm_class in ((stock, "stock"), (modified, "modified")):
            if arm.binary_manifest_sha256 != sha256_json(arm.binary_manifest):
                raise BatchPlanError("binary manifest commitment mismatch")
            if arm.binary_manifest.get("armClass") != arm_class:
                raise BatchPlanError("binary manifest arm class mismatch")
            if arm.binary.sha256 != arm.binary_manifest["binarySha256"]:
                raise BatchPlanError("binary snapshot commitment mismatch")
            if arm.codex_home_seed.digest != arm.treatment_manifest["codexHomeSeedSha256"]:
                raise BatchPlanError("seed snapshot commitment mismatch")
            config_path = _path(bindings[arm_class].get("effectiveConfig"), "effective config")
            if arm.effective_config_sha256 != sha256_file(config_path):
                raise BatchPlanError("effective config commitment mismatch")
    except BatchPlanError as error:
        raise ControllerSupportError(str(error)) from error
    if sha256_json(bindings.get("caseBundle")) != plan["caseBundleSha256"]:
        raise ControllerSupportError("case bundle commitment mismatch")
    schema_path = _path(bindings.get("caseAnswerSchemaPath"), "CaseAnswer schema path")
    if sha256_file(schema_path) != plan["caseAnswerSchemaSha256"]:
        raise ControllerSupportError("CaseAnswer schema commitment mismatch")
    case_answer_schema = freeze_json(bindings.get("caseAnswerSchema"))
    if case_answer_schema != load_exact_json(schema_path):
        raise ControllerSupportError("CaseAnswer schema bytes were substituted")
    try:
        workspace_seed = FrozenTree.capture(_path(bindings.get("workspaceSeed"), "workspace seed"))
        schema_file = FrozenFile.capture(schema_path)
    except SnapshotError as error:
        raise ControllerSupportError(str(error)) from error
    if workspace_seed.digest != plan["workspaceTemplateSha256"]:
        raise ControllerSupportError("workspace template commitment mismatch")
    profile = freeze_json(require_mapping(bindings.get("executionProfile"), "execution profile"))
    assert type(profile) is dict
    profile_path = _path(bindings.get("executionProfilePath"), "execution profile path")
    try:
        validate_named_contract("execution-profile", profile)
    except BatchContractError as error:
        raise ControllerSupportError(
            f"execution profile is invalid: {error}"
        ) from error
    if load_exact_json(profile_path) != profile:
        raise ControllerSupportError("execution profile bytes were substituted")
    if sha256_json(profile) != plan["executionProfileRef"]:
        raise ControllerSupportError("execution profile reference mismatch")
    if profile["maxWallClockSeconds"] != plan["timeoutBudget"]:
        raise ControllerSupportError(
            "execution profile wall clock differs from plan timeout"
        )
    source_environment = bindings.get("sourceEnvironment")
    model_route = freeze_json(bindings.get("modelRoute"))
    if not isinstance(source_environment, Mapping) or type(model_route) is not dict:
        raise ControllerSupportError(
            "source environment and model route must be mappings"
        )
    model_route_path = _path(bindings.get("modelRoutePath"), "model route path")
    if load_exact_json(model_route_path) != model_route:
        raise ControllerSupportError("model route bytes were substituted")
    if sha256_json(model_route) != plan["modelRouteRef"]:
        raise ControllerSupportError("model route reference mismatch")
    protocol_path = _path(
        bindings.get("appServerProtocolSchemaPath"), "protocol schema path"
    )
    protocol_sha256 = _sha256(
        bindings.get("appServerProtocolSchemaSha256"), "protocol schema SHA-256"
    )
    try:
        protocol = FrozenFile.capture(protocol_path)
    except SnapshotError as error:
        raise ControllerSupportError(str(error)) from error
    if protocol.sha256 != protocol_sha256:
        raise ControllerSupportError("protocol schema bytes were substituted")
    promptfoo_path = _path(bindings.get("promptfooConfigPath"), "Promptfoo config path")
    promptfoo_sha256 = _sha256(
        bindings.get("promptfooConfigSha256"), "Promptfoo config SHA-256"
    )
    try:
        promptfoo = FrozenFile.capture(promptfoo_path)
    except SnapshotError as error:
        raise ControllerSupportError(str(error)) from error
    if promptfoo.sha256 != promptfoo_sha256:
        raise ControllerSupportError("Promptfoo config bytes were substituted")
    return ValidatedBindings(
        plan,
        stock,
        modified,
        _path(bindings.get("attemptBase"), "attempt base"),
        workspace_seed,
        profile,
        {str(key): str(item) for key, item in source_environment.items()},
        freeze_json(bindings.get("caseBundle")),
        case_answer_schema,
        schema_file,
        model_route,
        protocol,
        protocol_sha256,
        promptfoo,
        promptfoo_sha256,
    )


def require_seed(value: bytes | None) -> bytes:
    generated = secrets.token_bytes(32) if value is None else value
    if type(generated) is not bytes or len(generated) != 32:
        raise ControllerSupportError("seed must contain exactly 32 bytes")
    return generated


def derive(seed: bytes, label: bytes) -> bytes:
    return hmac.new(seed, b"07b\0" + label, hashlib.sha256).digest()


def pair_seed(seed: bytes, private_root: Path, plan_sha256: object) -> bytes:
    context = (
        str(Path(private_root).resolve()).encode() + b"\0" + str(plan_sha256).encode()
    )
    return derive(seed, b"private-context\0" + context)


def identities(seed: bytes) -> tuple[str, str, str]:
    return (
        derive(seed, b"pair-id").hex(),
        derive(seed, b"stock-attempt-id").hex(),
        derive(seed, b"modified-attempt-id").hex(),
    )


def freeze_json(value: object) -> object:
    return json.loads(canonical_json_bytes(value))


def classify_attempt_result(
    exit_code: object,
    output: object,
    metadata: object,
    plan: dict[str, object],
    schema_path: Path,
    *,
    started_at: object,
    finished_at: object,
    request_count: object,
    artifact_sizes: tuple[int, ...],
    max_output_bytes: int,
    forced_evidence_failure: bool = False,
) -> tuple[str, str | None]:
    if forced_evidence_failure:
        return "evidenceFailure", "candidate raw evidence is malformed"
    if type(metadata) is dict and metadata.get("supervisorTimedOut") is True:
        return "budgetFailure", "candidate exceeded the supervisor deadline"
    if type(exit_code) is not int or exit_code != 0:
        return "executionFailure", "candidate process did not exit successfully"
    if type(metadata) is dict and metadata.get("timedOut") is True:
        return "executionFailure", "candidate execution timed out"
    try:
        validate_contract(output, schema_path)
    except LabContractError:
        return "schemaFailure", "candidate output failed CaseAnswer validation"
    assert type(output) is dict
    if output.get("caseId") != plan["caseBundleId"]:
        return (
            "schemaFailure",
            "candidate output case ID does not match the sealed case",
        )
    if type(metadata) is not dict:
        return "evidenceFailure", "candidate metadata is missing"
    try:
        if type(started_at) is not str or type(finished_at) is not str:
            raise ValueError
        started = datetime.fromisoformat(started_at.replace("Z", "+00:00"))
        finished = datetime.fromisoformat(finished_at.replace("Z", "+00:00"))
        if started.tzinfo is None or finished.tzinfo is None or finished < started:
            raise ValueError
    except ValueError:
        return "evidenceFailure", "candidate timestamps are invalid"
    if (finished - started).total_seconds() > plan["timeoutBudget"]:
        return "budgetFailure", "candidate exceeded the sealed wall-clock budget"
    if (
        type(metadata.get("threadId")) is not str
        or type(metadata.get("turnId")) is not str
    ):
        return "evidenceFailure", "thread or turn evidence is missing"
    trajectory = metadata.get("trajectory")
    if type(trajectory) not in (dict, list) or not trajectory:
        return "evidenceFailure", "trajectory evidence is missing"
    usage, cost = metadata.get("usage"), metadata.get("costEvidence")
    if type(usage) is not dict or type(cost) is not dict:
        return "evidenceFailure", "usage or cost evidence is missing"
    values = (
        usage.get("inputTokens"),
        usage.get("outputTokens"),
        usage.get("totalTokens"),
    )
    if (
        any(type(item) is not int or item < 0 for item in values)
        or values[2] != values[0] + values[1]
    ):
        return "evidenceFailure", "usage evidence is invalid"
    if type(cost.get("costCny")) is not int or cost["costCny"] < 0:
        return "evidenceFailure", "cost evidence is invalid"
    if type(request_count) is not int or request_count < 0:
        return "evidenceFailure", "request-count evidence is invalid"
    if request_count > plan["requestBudget"]:
        return "budgetFailure", "candidate exceeded the sealed request budget"
    if any(size > max_output_bytes for size in artifact_sizes):
        return "budgetFailure", "candidate exceeded the sealed output budget"
    if values[2] > plan["tokenBudget"] or cost["costCny"] > plan["costBudgetCny"]:
        return "budgetFailure", "candidate exceeded a sealed budget"
    return "completed", None
