import hashlib
import json
import os
import stat
import sys
import tomllib
from pathlib import Path

import pytest


sys.path.insert(0, str(Path(__file__).resolve().parent))
from batch_plan import (  # noqa: E402
    PARITY_FIELDS,
    BatchPlanError,
    seal_candidate_run_plan,
    sha256_file,
    sha256_tree,
    verify_binary_manifest,
    verify_candidate_run_plan,
    verify_effective_condition_parity,
    verify_treatment_manifest,
)
from contracts import sha256_json  # noqa: E402


REPO_ROOT = Path(__file__).resolve().parents[3]
LAB_ROOT = REPO_ROOT / "ai-ip-evals" / "lab"
ASSET_SKILL = (
    REPO_ROOT
    / "ai-ip-assets"
    / "skills"
    / "deliver-ai-ip-content-package"
    / "SKILL.md"
)


@pytest.fixture
def valid_plan() -> dict[str, object]:
    return {
        "schemaVersion": 1,
        "planId": "plan-1",
        "batchId": "batch-1",
        "batchKind": "smoke",
        "caseBundleId": "case-1",
        "caseBundleSha256": "a" * 64,
        "caseAnswerSchemaId": "case-answer-v1",
        "caseAnswerSchemaSha256": "b" * 64,
        "stockTreatmentRef": "stock-treatment",
        "modifiedTreatmentRef": "modified-treatment",
        "modelRouteRef": "loopback-model",
        "executionProfileRef": "offline-profile",
        "workspaceTemplateSha256": "c" * 64,
        "promptfooPackageVersion": "0.122.0",
        "promptfooLockSha256": "d" * 64,
        "replicationCount": 1,
        "retryPolicy": {"maxRetries": 0},
        "timeoutBudget": 300,
        "tokenBudget": 4096,
        "requestBudget": 2,
        "costBudgetCny": 0,
        "networkPolicy": "offline",
        "permissionPolicy": "never",
        "toolPolicy": "minimal",
        "createdAt": "2026-08-31T12:00:00Z",
        "sealedAt": "2026-08-31T12:01:00Z",
        "planSha256": "0" * 64,
    }


def _treatment_manifest(
    seed: Path,
    system_instruction: Path,
    capability_bundle: Path,
    effective_config: Path,
) -> dict[str, object]:
    manifest = {
        "schemaVersion": 1,
        "treatmentId": "treatment-1",
        "binaryManifestRef": "binary-1",
        "codexHomeSeedSha256": sha256_tree(seed),
        "systemInstructionSha256": sha256_file(system_instruction),
        "capabilityBundleSha256": sha256_tree(capability_bundle),
        "effectiveCodexConfigSha256": sha256_file(effective_config),
        "declaredCapabilities": ["content-package"],
        "prohibitedCaseSpecificMaterial": ["case answers", "review rubrics"],
        "treatmentManifestSha256": "0" * 64,
    }
    manifest["treatmentManifestSha256"] = sha256_json(
        {key: value for key, value in manifest.items() if key != "treatmentManifestSha256"}
    )
    return manifest


def test_tree_hash_rejects_links_and_is_order_independent(tmp_path: Path) -> None:
    """Catches a tree hasher that follows links or depends on directory order."""
    left = tmp_path / "left"
    right = tmp_path / "right"
    left.mkdir()
    right.mkdir()
    (left / "b").write_bytes(b"two")
    (left / "a").write_bytes(b"one")
    (right / "a").write_bytes(b"one")
    (right / "b").write_bytes(b"two")

    assert sha256_tree(left) == sha256_tree(right)

    (right / "link").symlink_to(right / "a")
    with pytest.raises(BatchPlanError, match="symbolic link"):
        sha256_tree(right)


def test_tree_hash_commits_canonical_relative_entries(tmp_path: Path) -> None:
    """Catches a tree hash that omits path, mode, size, or nested file bytes."""
    root = tmp_path / "seed"
    nested = root / "nested"
    nested.mkdir(parents=True)
    config = root / "config.toml"
    config.write_bytes(b"mode = 'offline'\n")
    os.chmod(config, 0o600)
    skill = nested / "SKILL.md"
    skill.write_bytes(b"skill\n")

    expected = sha256_json(
        {
            "entries": [
                {
                    "mode": stat.S_IMODE(config.stat().st_mode),
                    "path": "config.toml",
                    "sha256": hashlib.sha256(config.read_bytes()).hexdigest(),
                    "size": len(config.read_bytes()),
                },
                {
                    "mode": stat.S_IMODE(skill.stat().st_mode),
                    "path": "nested/SKILL.md",
                    "sha256": hashlib.sha256(skill.read_bytes()).hexdigest(),
                    "size": len(skill.read_bytes()),
                },
            ]
        }
    )
    assert sha256_tree(root) == expected


@pytest.mark.parametrize("unsafe_kind", ["hardlink", "fifo"])
def test_file_hash_rejects_unsafe_files(tmp_path: Path, unsafe_kind: str) -> None:
    """Catches a file hasher that accepts aliases or non-regular filesystem nodes."""
    target = tmp_path / "target"
    target.write_bytes(b"safe")
    unsafe = tmp_path / unsafe_kind
    if unsafe_kind == "hardlink":
        os.link(target, unsafe)
    else:
        os.mkfifo(unsafe)

    with pytest.raises(BatchPlanError, match="regular|hard link"):
        sha256_file(unsafe)


def test_tree_hash_rejects_files_larger_than_limit(tmp_path: Path) -> None:
    """Catches a bounded tree hasher that allows an oversized seed file."""
    root = tmp_path / "seed"
    root.mkdir()
    (root / "oversized").write_bytes(b"x" * (8 * 1024 * 1024 + 1))

    with pytest.raises(BatchPlanError, match="8 MiB"):
        sha256_tree(root)


def test_binary_manifest_binds_recomputed_binary_bytes(tmp_path: Path) -> None:
    """Catches binary preflight that trusts a manifest digest instead of the binary."""
    binary = tmp_path / "codex"
    binary.write_bytes(b"candidate binary")
    manifest = {
        "schemaVersion": 1,
        "binaryId": "candidate-1",
        "armClass": "stock",
        "sourceRepository": "https://example.invalid/codex",
        "sourceCommit": "4ef1d4b89",
        "targetTriple": "aarch64-apple-darwin",
        "buildProfile": "debug",
        "toolchainVersions": {"rust": "1.95.0"},
        "buildReceiptSha256": "a" * 64,
        "binarySha256": sha256_file(binary),
        "declaredCapabilities": [],
    }

    assert verify_binary_manifest(manifest, binary) is None
    binary.write_bytes(b"mutated candidate binary")
    with pytest.raises(BatchPlanError, match="binary SHA-256"):
        verify_binary_manifest(manifest, binary)


def test_treatment_manifest_binds_each_sealed_artifact(tmp_path: Path) -> None:
    """Catches treatment verification that skips a seed or effective-config digest."""
    seed = tmp_path / "seed"
    capability_bundle = tmp_path / "bundle"
    seed.mkdir()
    capability_bundle.mkdir()
    (seed / "config.toml").write_text("offline = true\n", encoding="utf-8")
    (capability_bundle / "SKILL.md").write_text("content package\n", encoding="utf-8")
    system_instruction = tmp_path / "system.md"
    system_instruction.write_text("Return JSON only.\n", encoding="utf-8")
    effective_config = tmp_path / "effective.toml"
    effective_config.write_text("offline = true\n", encoding="utf-8")
    manifest = _treatment_manifest(
        seed, system_instruction, capability_bundle, effective_config
    )

    assert (
        verify_treatment_manifest(
            manifest,
            codex_home_seed=seed,
            system_instruction=system_instruction,
            capability_bundle=capability_bundle,
            effective_config=effective_config,
        )
        is None
    )
    effective_config.write_text("offline = false\n", encoding="utf-8")
    with pytest.raises(BatchPlanError, match="effective config SHA-256"):
        verify_treatment_manifest(
            manifest,
            codex_home_seed=seed,
            system_instruction=system_instruction,
            capability_bundle=capability_bundle,
            effective_config=effective_config,
        )


def test_treatment_manifest_rejects_case_specific_seed_material(tmp_path: Path) -> None:
    """Catches treatment verification that permits rubric or answer material in a seed."""
    seed = tmp_path / "seed"
    capability_bundle = tmp_path / "bundle"
    seed.mkdir()
    capability_bundle.mkdir()
    (seed / "config.toml").write_text("offline = true\n", encoding="utf-8")
    (seed / "review-rubric.json").write_text("{}\n", encoding="utf-8")
    (capability_bundle / "SKILL.md").write_text("content package\n", encoding="utf-8")
    system_instruction = tmp_path / "system.md"
    system_instruction.write_text("Return JSON only.\n", encoding="utf-8")
    effective_config = tmp_path / "effective.toml"
    effective_config.write_text("offline = true\n", encoding="utf-8")
    manifest = _treatment_manifest(
        seed, system_instruction, capability_bundle, effective_config
    )

    with pytest.raises(BatchPlanError, match="case-specific"):
        verify_treatment_manifest(
            manifest,
            codex_home_seed=seed,
            system_instruction=system_instruction,
            capability_bundle=capability_bundle,
            effective_config=effective_config,
        )


def test_fixture_seeds_only_differ_by_the_declared_skill() -> None:
    """Catches fixture drift that gives stock the Lead Skill or changes its bytes."""
    stock_skill = (
        LAB_ROOT
        / "fixtures"
        / "batch-runner"
        / "stock-seed"
        / "skills"
        / "deliver-ai-ip-content-package"
        / "SKILL.md"
    )
    modified_skill = (
        LAB_ROOT
        / "fixtures"
        / "batch-runner"
        / "modified-seed"
        / "skills"
        / "deliver-ai-ip-content-package"
        / "SKILL.md"
    )

    assert not stock_skill.exists()
    assert modified_skill.read_bytes() == ASSET_SKILL.read_bytes()
    assert sha256_file(modified_skill) == sha256_file(ASSET_SKILL)
    stock_config = tomllib.loads(
        (LAB_ROOT / "fixtures" / "batch-runner" / "stock-seed" / "config.toml").read_text(
            encoding="utf-8"
        )
    )
    modified_config = tomllib.loads(
        (
            LAB_ROOT / "fixtures" / "batch-runner" / "modified-seed" / "config.toml"
        ).read_text(encoding="utf-8")
    )
    assert "lead_skill" not in stock_config
    assert modified_config["lead_skill"] == {
        "path": "skills/deliver-ai-ip-content-package/SKILL.md",
        "sha256": sha256_file(ASSET_SKILL),
    }


def test_mutated_plan_is_rejected(valid_plan: dict[str, object]) -> None:
    """Catches a plan verifier that does not bind token budget changes."""
    sealed = seal_candidate_run_plan(valid_plan)
    assert sealed is not valid_plan
    verify_candidate_run_plan(sealed)
    sealed["tokenBudget"] = 4097
    with pytest.raises(BatchPlanError, match="plan commitment"):
        verify_candidate_run_plan(sealed)


def test_plan_sealer_rejects_invalid_wire_shape(valid_plan: dict[str, object]) -> None:
    """Catches a plan sealer that commits a schema-invalid candidate plan."""
    valid_plan["tokenBudget"] = True
    with pytest.raises(BatchPlanError, match="contract validation"):
        seal_candidate_run_plan(valid_plan)


def test_effective_condition_parity_compares_only_runtime_fields(
    valid_plan: dict[str, object],
) -> None:
    """Catches parity checks that miss a budget or reject opaque arm-specific values."""
    stock = dict(valid_plan)
    modified = dict(valid_plan)
    modified.update(
        {
            "planId": "plan-2",
            "stockTreatmentRef": "other-stock-treatment",
            "modifiedTreatmentRef": "other-modified-treatment",
            "privateArmPath": "/private/modified",
            "executionOrder": 2,
        }
    )

    expected = sha256_json({field: stock[field] for field in PARITY_FIELDS})
    assert verify_effective_condition_parity(stock, modified) == expected

    modified["requestBudget"] = 3
    with pytest.raises(BatchPlanError, match="condition parity"):
        verify_effective_condition_parity(stock, modified)


def test_effective_condition_parity_requires_every_runtime_field(
    valid_plan: dict[str, object],
) -> None:
    """Catches parity checks that silently accept a missing runtime condition."""
    modified = dict(valid_plan)
    modified.pop("toolPolicy")

    with pytest.raises(BatchPlanError, match="missing parity field"):
        verify_effective_condition_parity(valid_plan, modified)
