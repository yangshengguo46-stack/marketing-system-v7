from pathlib import Path

try:
    from .contracts import (
        LabContractError,
        load_exact_json,
        sha256_json,
        validate_contract,
    )
except ImportError:
    from contracts import (
        LabContractError,
        load_exact_json,
        sha256_json,
        validate_contract,
    )


_REPO_ROOT = Path(__file__).resolve().parents[3]
_SCHEMA_ROOT = _REPO_ROOT / "ai-ip-evals" / "lab" / "schemas"
_CONTRACT_NAMES = frozenset(
    {
        "binary-manifest",
        "treatment-manifest",
        "execution-profile",
        "candidate-run-plan",
        "arm-attempt-receipt",
        "paired-run-receipt",
        "candidate-pair-import-receipt",
        "live-run-authorization",
    }
)
_SELF_COMMITMENT_FIELDS = frozenset(
    {
        "treatmentManifestSha256",
        "planSha256",
        "receiptSha256",
        "importReceiptSha256",
        "orphanSha256",
    }
)


class BatchContractError(ValueError):
    pass


def _require_named_contract(name: str) -> None:
    if name not in _CONTRACT_NAMES:
        raise BatchContractError(f"unknown contract: {name}")


def _require_commitment_field(field: str) -> None:
    if field not in _SELF_COMMITMENT_FIELDS:
        raise BatchContractError(f"unknown commitment field: {field}")


def _require_commitment_value(value: object, field: str) -> dict[str, object]:
    if type(value) is not dict:
        raise BatchContractError("self commitment value must be an object")
    if field not in value:
        raise BatchContractError(f"missing commitment field: {field}")
    commitment = value[field]
    if type(commitment) is not str or len(commitment) != 64:
        raise BatchContractError(f"invalid commitment field: {field}")
    if any(character not in "0123456789abcdef" for character in commitment):
        raise BatchContractError(f"invalid commitment field: {field}")
    return value


def _schema_path(name: str) -> Path:
    _require_named_contract(name)
    schema_path = _SCHEMA_ROOT / f"{name}.schema.json"
    if not schema_path.is_file():
        raise BatchContractError(f"contract schema is missing: {name}")
    return schema_path


def load_named_contract(name: str, path: Path) -> object:
    _require_named_contract(name)
    try:
        value = load_exact_json(path)
    except LabContractError as error:
        raise BatchContractError(str(error)) from error
    return validate_named_contract(name, value)


def validate_named_contract(name: str, value: object) -> object:
    schema_path = _schema_path(name)
    try:
        validate_contract(value, schema_path)
    except LabContractError as error:
        raise BatchContractError(str(error)) from error
    return value


def seal_self_commitment(value: object, field: str) -> dict[str, object]:
    _require_commitment_field(field)
    source = _require_commitment_value(value, field)
    sealed = dict(source)
    sealed.pop(field)
    try:
        sealed[field] = sha256_json(sealed)
    except LabContractError as error:
        raise BatchContractError(str(error)) from error
    return sealed


def verify_self_commitment(value: object, field: str) -> None:
    _require_commitment_field(field)
    source = _require_commitment_value(value, field)
    expected = source[field]
    committed = dict(source)
    committed.pop(field)
    try:
        actual = sha256_json(committed)
    except LabContractError as error:
        raise BatchContractError(str(error)) from error
    if actual != expected:
        raise BatchContractError("commitment mismatch")
