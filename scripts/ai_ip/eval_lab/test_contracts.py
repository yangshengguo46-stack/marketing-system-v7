from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from contracts import (
    LabContractError,
    canonical_json_bytes,
    load_exact_json,
    sha256_json,
    validate_contract,
)


REPO_ROOT = Path(__file__).resolve().parents[3]
LAB_ROOT = REPO_ROOT / "ai-ip-evals" / "lab"


def test_load_exact_json_round_trips_exact_wire_values(tmp_path):
    path = tmp_path / "input.json"
    path.write_bytes(
        '{"text":"礼与人情","items":[null,true,false,-7,0,12],"nested":{"a":1}}'.encode()
    )

    assert load_exact_json(path) == {
        "text": "礼与人情",
        "items": [None, True, False, -7, 0, 12],
        "nested": {"a": 1},
    }


@pytest.mark.parametrize(
    "payload",
    [
        b'{"a":1,"a":2}',
        b'{"number":1.25}',
        b'{"number":NaN}',
    ],
)
def test_load_exact_json_rejects_ambiguous_numbers_and_keys(payload, tmp_path):
    path = tmp_path / "input.json"
    path.write_bytes(payload)
    with pytest.raises(LabContractError):
        load_exact_json(path)


def test_canonical_json_is_order_independent_and_utf8():
    assert canonical_json_bytes({"礼": "人情", "a": 1}) == (
        '{"a":1,"礼":"人情"}'.encode("utf-8")
    )
    assert canonical_json_bytes({"a": 1, "礼": "人情"}) == canonical_json_bytes(
        {"礼": "人情", "a": 1}
    )


@pytest.mark.parametrize(
    "value",
    [1.0, float("nan"), {1: "non-string key"}, ("tuple",), b"bytes", object()],
)
def test_canonical_json_rejects_non_wire_python_values(value):
    with pytest.raises(LabContractError):
        canonical_json_bytes(value)


def test_sha256_json_hashes_only_canonical_bytes():
    assert sha256_json({"a": 1}) == (
        "015abd7f5cc57a2dd94b7590f04ad8084273905ee33ec5cebeae62276a97f862"
    )


def test_validate_contract_accepts_fixture_and_rejects_schema_violation():
    schema = LAB_ROOT / "schemas" / "case-answer.schema.json"
    value = load_exact_json(LAB_ROOT / "fixtures" / "synthetic" / "arm-strong.json")
    validate_contract(value, schema)

    invalid = dict(value)
    invalid["unexpected"] = "rejected"
    with pytest.raises(LabContractError):
        validate_contract(invalid, schema)


def test_validate_contract_rejects_float_even_when_schema_allows_number(tmp_path):
    schema = tmp_path / "number.schema.json"
    schema.write_bytes(
        b'{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"number"}'
    )

    with pytest.raises(LabContractError):
        validate_contract(1.0, schema)
