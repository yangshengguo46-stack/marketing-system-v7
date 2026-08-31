"""Private preflight, derivation, and classification helpers for Task 4."""

import hashlib
import hmac
import json
import secrets
from dataclasses import dataclass
from pathlib import Path
from typing import Mapping

try:
    from .batch_contracts import BatchContractError, validate_named_contract
    from .batch_plan import (
        BatchPlanError,
        sha256_file,
        sha256_tree,
        verify_candidate_run_plan,
        verify_effective_condition_parity,
    )
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
        verify_candidate_run_plan,
        verify_effective_condition_parity,
    )
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
    treatment_manifest: dict[str, object]
    binary_manifest_sha256: str
    binary_path: Path
    codex_home_seed: Path
    effective_config_sha256: str
    effective_conditions: dict[str, object]


@dataclass(frozen=True)
class ValidatedBindings:
    stock: ArmBinding
    modified: ArmBinding
    attempt_base: Path
    workspace_seed: Path
    execution_profile: dict[str, object]
    source_environment: Mapping[str, str]
    case_bundle: object
    case_answer_schema: object
    case_answer_schema_path: Path
    model_route: Mapping[str, object]
    protocol_sha256: str
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


def _arm_binding(value: object, name: str) -> ArmBinding:
    arm = require_mapping(value, f"{name} binding")
    private_arm_id = _text(arm.get("privateArmId"), "private arm ID")
    if any(
        token in private_arm_id.casefold() for token in ("stock", "modified", "/", "\\")
    ):
        raise ControllerSupportError("private arm ID must be opaque")
    return ArmBinding(
        private_arm_id,
        require_mapping(arm.get("treatmentManifest"), "treatment manifest"),
        _sha256(arm.get("binaryManifestSha256"), "binary manifest SHA-256"),
        _path(arm.get("binaryPath"), "binary path"),
        _path(arm.get("codexHomeSeed"), "Codex home seed"),
        _sha256(arm.get("effectiveConfigSha256"), "effective config SHA-256"),
        require_mapping(arm.get("effectiveConditions"), "effective conditions"),
    )


def validate_bindings(plan: dict[str, object], value: object) -> ValidatedBindings:
    bindings = require_mapping(value, "candidate bindings")
    if bindings.get("retryPolicy") != {"maxAttemptsPerArm": 1}:
        raise ControllerSupportError("controller requires maxAttemptsPerArm=1")
    if plan.get("retryPolicy") != {"maxRetries": 0}:
        raise ControllerSupportError("sealed plan must prohibit retries")
    stock = _arm_binding(bindings.get("stock"), "stock")
    modified = _arm_binding(bindings.get("modified"), "modified")
    try:
        verify_candidate_run_plan(
            plan,
            stock_treatment=stock.treatment_manifest,
            modified_treatment=modified.treatment_manifest,
        )
        verify_effective_condition_parity(
            stock.effective_conditions, modified.effective_conditions
        )
    except BatchPlanError as error:
        raise ControllerSupportError(str(error)) from error
    if sha256_json(bindings.get("caseBundle")) != plan["caseBundleSha256"]:
        raise ControllerSupportError("case bundle commitment mismatch")
    schema_path = _path(bindings.get("caseAnswerSchemaPath"), "CaseAnswer schema path")
    if sha256_file(schema_path) != plan["caseAnswerSchemaSha256"]:
        raise ControllerSupportError("CaseAnswer schema commitment mismatch")
    case_answer_schema = bindings.get("caseAnswerSchema")
    if case_answer_schema != load_exact_json(schema_path):
        raise ControllerSupportError("CaseAnswer schema bytes were substituted")
    workspace_seed = _path(bindings.get("workspaceSeed"), "workspace seed")
    if sha256_tree(workspace_seed) != plan["workspaceTemplateSha256"]:
        raise ControllerSupportError("workspace template commitment mismatch")
    profile = require_mapping(bindings.get("executionProfile"), "execution profile")
    try:
        validate_named_contract("execution-profile", profile)
    except BatchContractError as error:
        raise ControllerSupportError(
            f"execution profile is invalid: {error}"
        ) from error
    source_environment = bindings.get("sourceEnvironment")
    model_route = bindings.get("modelRoute")
    if not isinstance(source_environment, Mapping) or not isinstance(
        model_route, Mapping
    ):
        raise ControllerSupportError(
            "source environment and model route must be mappings"
        )
    return ValidatedBindings(
        stock,
        modified,
        _path(bindings.get("attemptBase"), "attempt base"),
        workspace_seed,
        profile,
        source_environment,
        bindings.get("caseBundle"),
        case_answer_schema,
        schema_path,
        model_route,
        _sha256(
            bindings.get("appServerProtocolSchemaSha256"), "protocol schema SHA-256"
        ),
        _sha256(bindings.get("promptfooConfigSha256"), "Promptfoo config SHA-256"),
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
) -> tuple[str, str | None]:
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
    if values[2] > plan["tokenBudget"] or cost["costCny"] > plan["costBudgetCny"]:
        return "budgetFailure", "candidate exceeded a sealed budget"
    return "completed", None
