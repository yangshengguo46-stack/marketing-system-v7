from pathlib import Path
import sys

import pytest


sys.path.insert(0, str(Path(__file__).resolve().parent))
from batch_contracts import (  # noqa: E402
    BatchContractError,
    load_named_contract,
    seal_self_commitment,
    validate_named_contract,
    verify_self_commitment,
)
from contracts import LabContractError, load_exact_json  # noqa: E402


REPO_ROOT = Path(__file__).resolve().parents[3]
LAB_ROOT = REPO_ROOT / "ai-ip-evals" / "lab"
FIXTURE_PATH = LAB_ROOT / "fixtures" / "batch-runner" / "valid-contracts.json"

CONTRACT_NAMES = (
    "binary-manifest",
    "treatment-manifest",
    "execution-profile",
    "candidate-run-plan",
    "arm-attempt-receipt",
    "paired-run-receipt",
    "candidate-pair-import-receipt",
    "live-run-authorization",
)


@pytest.mark.parametrize("name", CONTRACT_NAMES)
def test_valid_contract_fixture(name):
    """Catches a schema that rejects its documented minimal wire contract."""
    fixture = load_exact_json(FIXTURE_PATH)[name]
    assert validate_named_contract(name, fixture) == fixture


def test_self_commitment_detects_mutation():
    """Catches a verifier that fails to bind fields outside the commitment."""
    value = {"schemaVersion": 1, "planId": "plan-1", "planSha256": "0" * 64}
    sealed = seal_self_commitment(value, "planSha256")
    verify_self_commitment(sealed, "planSha256")
    sealed["planId"] = "plan-2"
    with pytest.raises(BatchContractError, match="commitment mismatch"):
        verify_self_commitment(sealed, "planSha256")


def test_self_commitment_preserves_input_and_returns_a_distinct_object():
    """Catches a sealer that mutates its input or removes another commitment."""
    value = {
        "schemaVersion": 1,
        "receiptSha256": "a" * 64,
        "planSha256": "0" * 64,
    }
    sealed = seal_self_commitment(value, "planSha256")
    assert sealed is not value
    assert value == {
        "schemaVersion": 1,
        "receiptSha256": "a" * 64,
        "planSha256": "0" * 64,
    }
    assert sealed["receiptSha256"] == "a" * 64
    assert sealed["planSha256"] != "0" * 64


def test_named_contract_rejects_unknown_name():
    """Catches an API that silently maps an unknown contract to a schema."""
    with pytest.raises(BatchContractError, match="unknown contract"):
        load_named_contract("not-a-contract", LAB_ROOT)


def test_pair_conditions_accept_a_valid_invalid_pair_shape():
    """Catches pair schemas that reject the receipt shape for a failed pair."""
    fixture = load_exact_json(FIXTURE_PATH)["paired-run-receipt"]

    invalid = dict(fixture)
    invalid["pairValidity"] = "invalid"
    invalid["invalidReason"] = "executionFailure"
    invalid.pop("outputRelation")
    invalid.pop("anonymousMappingCommitment")
    assert validate_named_contract("paired-run-receipt", invalid) == invalid


def test_pair_conditions_reject_missing_meaningful_fields():
    """Catches pairs that omit output relation or distinct anonymous mapping proof."""
    fixture = load_exact_json(FIXTURE_PATH)["paired-run-receipt"]

    missing_relation = dict(fixture)
    missing_relation.pop("outputRelation")
    with pytest.raises(BatchContractError):
        validate_named_contract("paired-run-receipt", missing_relation)

    missing_mapping = dict(fixture)
    missing_mapping.pop("anonymousMappingCommitment")
    with pytest.raises(BatchContractError):
        validate_named_contract("paired-run-receipt", missing_mapping)


def test_pair_conditions_reject_mapping_for_an_identical_pair():
    """Catches pair schemas that create an anonymous mapping for an identical tie."""
    fixture = load_exact_json(FIXTURE_PATH)["paired-run-receipt"]

    identical = dict(fixture)
    identical["outputRelation"] = "canonicallyIdentical"
    identical.pop("anonymousMappingCommitment")
    assert validate_named_contract("paired-run-receipt", identical) == identical

    with_mapping = dict(identical)
    with_mapping["anonymousMappingCommitment"] = "b" * 64
    with pytest.raises(BatchContractError):
        validate_named_contract("paired-run-receipt", with_mapping)


@pytest.mark.parametrize(
    ("output_relation", "blind_disposition"),
    [
        ("distinct", "identicalTieNoReview"),
        ("canonicallyIdentical", "readyForBlindReview"),
    ],
)
def test_import_requires_matching_blind_disposition(output_relation, blind_disposition):
    """Catches imports that can create a review for a canonical identical tie."""
    fixture = dict(load_exact_json(FIXTURE_PATH)["candidate-pair-import-receipt"])
    fixture["outputRelation"] = output_relation
    fixture["blindDisposition"] = blind_disposition
    with pytest.raises(BatchContractError):
        validate_named_contract("candidate-pair-import-receipt", fixture)


def test_validation_rejects_uppercase_hash_float_and_boolean_budget():
    """Catches permissive wire validation for hashes and integer-only budgets."""
    plan = dict(load_exact_json(FIXTURE_PATH)["candidate-run-plan"])
    plan["planSha256"] = "A" * 64
    with pytest.raises(BatchContractError):
        validate_named_contract("candidate-run-plan", plan)

    plan = dict(load_exact_json(FIXTURE_PATH)["candidate-run-plan"])
    plan["tokenBudget"] = 1.0
    with pytest.raises((BatchContractError, LabContractError)):
        validate_named_contract("candidate-run-plan", plan)

    plan = dict(load_exact_json(FIXTURE_PATH)["candidate-run-plan"])
    plan["tokenBudget"] = True
    with pytest.raises(BatchContractError):
        validate_named_contract("candidate-run-plan", plan)


def test_validation_rejects_malformed_rfc3339_timestamp_and_unknown_field():
    """Catches timestamp formats or undeclared wire fields that slip through validation."""
    plan = dict(load_exact_json(FIXTURE_PATH)["candidate-run-plan"])
    plan["createdAt"] = "not-a-timestamp"
    with pytest.raises(BatchContractError):
        validate_named_contract("candidate-run-plan", plan)

    plan = dict(load_exact_json(FIXTURE_PATH)["candidate-run-plan"])
    plan["unexpected"] = "rejected"
    with pytest.raises(BatchContractError):
        validate_named_contract("candidate-run-plan", plan)


def test_load_named_contract_rejects_duplicate_json_keys(tmp_path):
    """Catches a loader that permits ambiguous duplicate keys on the wire."""
    path = tmp_path / "duplicate.json"
    path.write_text('{"schemaVersion":1,"schemaVersion":1}', encoding="utf-8")
    with pytest.raises(BatchContractError, match="duplicate JSON key"):
        load_named_contract("binary-manifest", path)


@pytest.mark.parametrize(
    ("value", "field", "message"),
    [
        (None, "planSha256", "must be an object"),
        ({"schemaVersion": 1}, "planSha256", "missing commitment field"),
        ({"planSha256": "0" * 64}, "notACommitment", "unknown commitment field"),
    ],
)
def test_self_commitment_rejects_missing_or_invalid_boundary_inputs(
    value, field, message
):
    """Catches commitment APIs that accept non-objects or unrecognized fields."""
    with pytest.raises(BatchContractError, match=message):
        seal_self_commitment(value, field)
