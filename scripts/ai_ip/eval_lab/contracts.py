import hashlib
import json
import re
from datetime import datetime
from pathlib import Path
from typing import NoReturn

from jsonschema import Draft202012Validator, FormatChecker
from jsonschema.exceptions import SchemaError, ValidationError


JsonObject = dict[str, object]
CaseCompilationReceipt = JsonObject
QualificationReceipt = JsonObject
BlindPackReceipt = JsonObject
BlindStatistics = JsonObject
BatchDecision = JsonObject


class LabContractError(ValueError):
    pass


_RFC3339_TIMESTAMP = re.compile(
    r"^\d{4}-\d{2}-\d{2}[Tt]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:[Zz]|[+-]\d{2}:\d{2})$"
)
_FORMAT_CHECKER = FormatChecker()


@_FORMAT_CHECKER.checks("date-time")
def _is_rfc3339_timestamp(value: object) -> bool:
    if type(value) is not str or _RFC3339_TIMESTAMP.fullmatch(value) is None:
        return False
    try:
        datetime.fromisoformat(value.replace("z", "+00:00").replace("Z", "+00:00"))
    except ValueError:
        return False
    return True


def _load_exact_json_bytes(payload: bytes) -> object:
    def reject_pairs(pairs: list[tuple[str, object]]) -> dict[str, object]:
        result: dict[str, object] = {}
        for key, value in pairs:
            if key in result:
                raise LabContractError(f"duplicate JSON key: {key}")
            result[key] = value
        return result

    def reject_number(token: str) -> NoReturn:
        raise LabContractError(f"non-integer JSON number: {token}")

    try:
        text = payload.decode("utf-8", errors="strict")
        return json.loads(
            text,
            object_pairs_hook=reject_pairs,
            parse_float=reject_number,
            parse_constant=reject_number,
        )
    except LabContractError:
        raise
    except (UnicodeDecodeError, json.JSONDecodeError, ValueError) as error:
        raise LabContractError("invalid exact JSON") from error


def load_exact_json(path: Path) -> object:
    return _load_exact_json_bytes(path.read_bytes())


def _require_wire_value(value: object, active: set[int] | None = None) -> None:
    if value is None or type(value) in (bool, int, str):
        return
    if active is None:
        active = set()
    if type(value) is list:
        identity = id(value)
        if identity in active:
            raise LabContractError("cyclic Python value is not JSON")
        active.add(identity)
        try:
            for item in value:
                _require_wire_value(item, active)
        finally:
            active.remove(identity)
        return
    if type(value) is dict:
        identity = id(value)
        if identity in active:
            raise LabContractError("cyclic Python value is not JSON")
        active.add(identity)
        try:
            for key, item in value.items():
                if type(key) is not str:
                    raise LabContractError("JSON object keys must be strings")
                _require_wire_value(item, active)
        finally:
            active.remove(identity)
        return
    raise LabContractError(f"non-wire Python value: {type(value).__name__}")


def canonical_json_bytes(value: object) -> bytes:
    _require_wire_value(value)
    try:
        return json.dumps(
            value,
            ensure_ascii=False,
            sort_keys=True,
            separators=(",", ":"),
            allow_nan=False,
        ).encode("utf-8")
    except (TypeError, ValueError, UnicodeEncodeError) as error:
        raise LabContractError("value is not canonical JSON") from error


def sha256_json(value: object) -> str:
    return hashlib.sha256(canonical_json_bytes(value)).hexdigest()


def validate_contract(value: object, schema_path: Path) -> None:
    _require_wire_value(value)
    schema = load_exact_json(schema_path)
    if type(schema) is not dict:
        raise LabContractError("contract schema must be a JSON object")
    try:
        Draft202012Validator.check_schema(schema)
        Draft202012Validator(schema, format_checker=_FORMAT_CHECKER).validate(value)
    except (SchemaError, ValidationError) as error:
        raise LabContractError(
            f"contract validation failed: {error.message}"
        ) from error
