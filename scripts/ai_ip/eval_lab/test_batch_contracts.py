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


def test_self_commitment_preserves_other_commitments():
    """Catches a sealer that removes fields beyond the named commitment."""
    value = {
        "schemaVersion": 1,
        "receiptSha256": "a" * 64,
        "planSha256": "0" * 64,
    }
    sealed = seal_self_commitment(value, "planSha256")
    assert sealed["receiptSha256"] == "a" * 64
    assert sealed["planSha256"] != "0" * 64


def test_named_contract_rejects_unknown_name():
    """Catches an API that silently maps an unknown contract to a schema."""
    with pytest.raises(BatchContractError, match="unknown contract"):
        load_named_contract("not-a-contract", LAB_ROOT)


def test_pair_conditions_require_only_meaningful_mapping_fields():
    """Catches pair schemas that accept invalid/identical pairs with comparison data."""
    fixture = load_exact_json(FIXTURE_PATH)["paired-run-receipt"]

    invalid = dict(fixture)
    invalid["pairValidity"] = "invalid"
    invalid["invalidReason"] = "executionFailure"
    with pytest.raises(BatchContractError):
        validate_named_contract("paired-run-receipt", invalid)

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


def test_validation_rejects_uppercase_hash_and_float_budget():
    """Catches permissive wire validation for hashes and integer-only budgets."""
    plan = dict(load_exact_json(FIXTURE_PATH)["candidate-run-plan"])
    plan["planSha256"] = "A" * 64
    with pytest.raises(BatchContractError):
        validate_named_contract("candidate-run-plan", plan)

    plan = dict(load_exact_json(FIXTURE_PATH)["candidate-run-plan"])
    plan["tokenBudget"] = 1.0
    with pytest.raises((BatchContractError, LabContractError)):
        validate_named_contract("candidate-run-plan", plan)
