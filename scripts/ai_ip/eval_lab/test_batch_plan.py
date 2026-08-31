import hashlib
import json
import os
import stat
import sys
import tomllib
from pathlib import Path

import pytest


sys.path.insert(0, str(Path(__file__).resolve().parent))
import batch_plan  # noqa: E402
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


def _binary_manifest(binary: Path, binary_id: str = "binary-1") -> dict[str, object]:
    return {
        "schemaVersion": 1,
        "binaryId": binary_id,
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


def _sealed_treatment(treatment_id: str) -> dict[str, object]:
    treatment = {
        "schemaVersion": 1,
        "treatmentId": treatment_id,
        "binaryManifestRef": "binary-1",
        "codexHomeSeedSha256": "a" * 64,
        "systemInstructionSha256": "b" * 64,
        "capabilityBundleSha256": "c" * 64,
        "effectiveCodexConfigSha256": "d" * 64,
        "declaredCapabilities": [],
        "prohibitedCaseSpecificMaterial": ["case answers", "review rubrics"],
        "treatmentManifestSha256": "0" * 64,
    }
    treatment["treatmentManifestSha256"] = sha256_json(
        {
            key: value
            for key, value in treatment.items()
            if key != "treatmentManifestSha256"
        }
    )
    return treatment


@pytest.fixture
def treatments() -> tuple[dict[str, object], dict[str, object]]:
    return _sealed_treatment("stock-treatment"), _sealed_treatment("modified-treatment")


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


def test_file_hash_rejects_hardlinks(tmp_path: Path) -> None:
    """Catches a file hasher that accepts aliases or non-regular filesystem nodes."""
    target = tmp_path / "target"
    target.write_bytes(b"safe")
    unsafe = tmp_path / "hardlink"
    os.link(target, unsafe)

    with pytest.raises(BatchPlanError, match="regular|hard link"):
        sha256_file(unsafe)


@pytest.mark.skipif(not hasattr(os, "mkfifo"), reason="FIFO creation is unavailable")
def test_file_hash_rejects_fifo_when_supported(tmp_path: Path) -> None:
    """Catches a file hasher that opens a named pipe as a regular input."""
    unsafe = tmp_path / "fifo"
    os.mkfifo(unsafe)
    with pytest.raises(BatchPlanError, match="regular"):
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
    manifest = _binary_manifest(binary, "candidate-1")

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
    binary = tmp_path / "codex"
    binary.write_bytes(b"candidate binary")
    binary_manifest = _binary_manifest(binary)
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
            binary_manifest=binary_manifest,
            binary_path=binary,
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
            binary_manifest=binary_manifest,
            binary_path=binary,
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
    binary = tmp_path / "codex"
    binary.write_bytes(b"candidate binary")
    binary_manifest = _binary_manifest(binary)
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
            binary_manifest=binary_manifest,
            binary_path=binary,
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


def test_mutated_plan_is_rejected(
    valid_plan: dict[str, object], treatments: tuple[dict[str, object], dict[str, object]]
) -> None:
    """Catches a plan verifier that does not bind token budget changes."""
    stock_treatment, modified_treatment = treatments
    sealed = seal_candidate_run_plan(
        valid_plan, stock_treatment=stock_treatment, modified_treatment=modified_treatment
    )
    assert sealed is not valid_plan
    verify_candidate_run_plan(
        sealed, stock_treatment=stock_treatment, modified_treatment=modified_treatment
    )
    sealed["tokenBudget"] = 4097
    with pytest.raises(BatchPlanError, match="plan commitment"):
        verify_candidate_run_plan(
            sealed, stock_treatment=stock_treatment, modified_treatment=modified_treatment
        )


def test_plan_sealer_rejects_invalid_wire_shape(
    valid_plan: dict[str, object], treatments: tuple[dict[str, object], dict[str, object]]
) -> None:
    """Catches a plan sealer that commits a schema-invalid candidate plan."""
    valid_plan["tokenBudget"] = True
    with pytest.raises(BatchPlanError, match="contract validation"):
        seal_candidate_run_plan(
            valid_plan, stock_treatment=treatments[0], modified_treatment=treatments[1]
        )


def test_effective_condition_parity_compares_only_runtime_fields(
    valid_plan: dict[str, object],
) -> None:
    """Catches parity checks that miss a budget or reject opaque arm-specific values."""
    stock = dict(valid_plan)
    modified = dict(valid_plan)
    modified.update(
        {
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


def test_tree_hash_rejects_root_replaced_with_symlink_after_open(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Catches traversal that follows a root replaced after its initial inspection."""
    root = tmp_path / "root"
    outside = tmp_path / "outside"
    detached = tmp_path / "detached"
    root.mkdir()
    outside.mkdir()
    (root / "safe").write_bytes(b"safe")
    (outside / "outside").write_bytes(b"outside")
    scandir = batch_plan.os.scandir
    swapped = False

    def scan_then_swap(path: object):
        nonlocal swapped
        entries = list(scandir(path))
        if not swapped:
            swapped = True
            root.rename(detached)
            root.symlink_to(outside, target_is_directory=True)
        return iter(entries)

    monkeypatch.setattr(batch_plan.os, "scandir", scan_then_swap)
    with pytest.raises(BatchPlanError, match="tree changed|symbolic link"):
        sha256_tree(root)


def test_tree_hash_rejects_subdirectory_replaced_with_symlink_before_open(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Catches traversal that reopens a queued child through a replacement link."""
    root = tmp_path / "root"
    nested = root / "nested"
    outside = tmp_path / "outside"
    detached = tmp_path / "detached"
    nested.mkdir(parents=True)
    outside.mkdir()
    (nested / "safe").write_bytes(b"safe")
    (outside / "outside").write_bytes(b"outside")
    scandir = batch_plan.os.scandir
    swapped = False

    def scan_then_swap(path: object):
        nonlocal swapped
        entries = list(scandir(path))
        if not swapped:
            swapped = True
            nested.rename(detached)
            nested.symlink_to(outside, target_is_directory=True)
        return iter(entries)

    monkeypatch.setattr(batch_plan.os, "scandir", scan_then_swap)
    with pytest.raises(BatchPlanError, match="symbolic link|tree changed"):
        sha256_tree(root)


@pytest.mark.parametrize(
    "name",
    [
        "OutcomePacket.json",
        "ReferenceDossier.json",
        "scores.json",
        "golden gift.md",
        "hidden-instructions.md",
    ],
)
def test_treatment_manifest_rejects_normalized_prohibited_seed_names(
    tmp_path: Path, name: str
) -> None:
    """Catches hygiene checks that miss documented filename spelling variants."""
    seed = tmp_path / "seed"
    bundle = tmp_path / "bundle"
    seed.mkdir()
    bundle.mkdir()
    (seed / "config.toml").write_text("offline = true\n", encoding="utf-8")
    (seed / name).write_text("placeholder\n", encoding="utf-8")
    (bundle / "SKILL.md").write_text("content package\n", encoding="utf-8")
    system_instruction = tmp_path / "system.md"
    effective_config = tmp_path / "effective.toml"
    binary = tmp_path / "codex"
    system_instruction.write_text("Return JSON only.\n", encoding="utf-8")
    effective_config.write_text("offline = true\n", encoding="utf-8")
    binary.write_bytes(b"candidate binary")
    manifest = _treatment_manifest(seed, system_instruction, bundle, effective_config)

    with pytest.raises(BatchPlanError, match="case-specific"):
        verify_treatment_manifest(
            manifest,
            codex_home_seed=seed,
            system_instruction=system_instruction,
            capability_bundle=bundle,
            effective_config=effective_config,
            binary_manifest=_binary_manifest(binary),
            binary_path=binary,
        )


def test_stock_treatment_rejects_lead_skill_under_a_renamed_path(tmp_path: Path) -> None:
    """Catches a stock seed that hides the Lead Skill under a noncanonical name."""
    seed = tmp_path / "seed"
    bundle = tmp_path / "bundle"
    seed.mkdir()
    bundle.mkdir()
    (seed / "config.toml").write_text("offline = true\n", encoding="utf-8")
    renamed = seed / "renamed.md"
    renamed.write_bytes(ASSET_SKILL.read_bytes())
    (bundle / "SKILL.md").write_text("content package\n", encoding="utf-8")
    system_instruction = tmp_path / "system.md"
    effective_config = tmp_path / "effective.toml"
    binary = tmp_path / "codex"
    system_instruction.write_text("Return JSON only.\n", encoding="utf-8")
    effective_config.write_text("offline = true\n", encoding="utf-8")
    binary.write_bytes(b"candidate binary")
    manifest = _treatment_manifest(seed, system_instruction, bundle, effective_config)

    with pytest.raises(BatchPlanError, match="Lead Skill"):
        verify_treatment_manifest(
            manifest,
            codex_home_seed=seed,
            system_instruction=system_instruction,
            capability_bundle=bundle,
            effective_config=effective_config,
            binary_manifest=_binary_manifest(binary),
            binary_path=binary,
        )


@pytest.mark.parametrize(
    ("path", "digest"),
    [
        ("skills/wrong/SKILL.md", sha256_file(ASSET_SKILL)),
        ("skills/deliver-ai-ip-content-package/SKILL.md", "0" * 64),
    ],
)
def test_modified_treatment_requires_exact_configured_lead_skill(
    tmp_path: Path, path: str, digest: str
) -> None:
    """Catches a modified config that does not bind the canonical Lead Skill."""
    seed = tmp_path / "seed"
    bundle = tmp_path / "bundle"
    skill = seed / "skills" / "deliver-ai-ip-content-package" / "SKILL.md"
    skill.parent.mkdir(parents=True)
    bundle.mkdir()
    skill.write_bytes(ASSET_SKILL.read_bytes())
    (seed / "config.toml").write_text(
        f'[lead_skill]\npath = "{path}"\nsha256 = "{digest}"\n', encoding="utf-8"
    )
    (bundle / "SKILL.md").write_text("content package\n", encoding="utf-8")
    system_instruction = tmp_path / "system.md"
    effective_config = tmp_path / "effective.toml"
    binary = tmp_path / "codex"
    system_instruction.write_text("Return JSON only.\n", encoding="utf-8")
    effective_config.write_text("offline = true\n", encoding="utf-8")
    binary.write_bytes(b"candidate binary")
    manifest = _treatment_manifest(seed, system_instruction, bundle, effective_config)
    manifest["declaredCapabilities"] = ["deliver-ai-ip-content-package"]
    manifest["treatmentManifestSha256"] = sha256_json(
        {
            key: value
            for key, value in manifest.items()
            if key != "treatmentManifestSha256"
        }
    )

    with pytest.raises(BatchPlanError, match="Lead Skill"):
        verify_treatment_manifest(
            manifest,
            codex_home_seed=seed,
            system_instruction=system_instruction,
            capability_bundle=bundle,
            effective_config=effective_config,
            binary_manifest=_binary_manifest(binary),
            binary_path=binary,
        )


def test_treatment_manifest_binds_the_verified_binary_reference(tmp_path: Path) -> None:
    """Catches a treatment whose opaque binary reference names a different binary."""
    seed = tmp_path / "seed"
    bundle = tmp_path / "bundle"
    seed.mkdir()
    bundle.mkdir()
    (seed / "config.toml").write_text("offline = true\n", encoding="utf-8")
    (bundle / "SKILL.md").write_text("content package\n", encoding="utf-8")
    system_instruction = tmp_path / "system.md"
    effective_config = tmp_path / "effective.toml"
    binary = tmp_path / "codex"
    system_instruction.write_text("Return JSON only.\n", encoding="utf-8")
    effective_config.write_text("offline = true\n", encoding="utf-8")
    binary.write_bytes(b"candidate binary")
    manifest = _treatment_manifest(seed, system_instruction, bundle, effective_config)
    manifest["binaryManifestRef"] = "other-binary"
    manifest["treatmentManifestSha256"] = sha256_json(
        {
            key: value
            for key, value in manifest.items()
            if key != "treatmentManifestSha256"
        }
    )

    with pytest.raises(BatchPlanError, match="binary manifest reference"):
        verify_treatment_manifest(
            manifest,
            codex_home_seed=seed,
            system_instruction=system_instruction,
            capability_bundle=bundle,
            effective_config=effective_config,
            binary_manifest=_binary_manifest(binary),
            binary_path=binary,
        )


def test_treatment_verification_uses_one_seed_snapshot(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Catches verification that hashes a seed then rescans different mutable bytes."""
    seed = tmp_path / "seed"
    bundle = tmp_path / "bundle"
    seed.mkdir()
    bundle.mkdir()
    config = seed / "config.toml"
    config.write_text("offline = true\n", encoding="utf-8")
    (bundle / "SKILL.md").write_text("content package\n", encoding="utf-8")
    system_instruction = tmp_path / "system.md"
    effective_config = tmp_path / "effective.toml"
    binary = tmp_path / "codex"
    system_instruction.write_text("Return JSON only.\n", encoding="utf-8")
    effective_config.write_text("offline = true\n", encoding="utf-8")
    binary.write_bytes(b"candidate binary")
    manifest = _treatment_manifest(seed, system_instruction, bundle, effective_config)
    snapshot_tree = batch_plan.snapshot_tree

    def snapshot_then_mutate(path: Path):
        snapshot = snapshot_tree(path)
        if path == seed:
            config.write_text("offline = false\n", encoding="utf-8")
        return snapshot

    monkeypatch.setattr(batch_plan, "snapshot_tree", snapshot_then_mutate)
    assert (
        verify_treatment_manifest(
            manifest,
            codex_home_seed=seed,
            system_instruction=system_instruction,
            capability_bundle=bundle,
            effective_config=effective_config,
            binary_manifest=_binary_manifest(binary),
            binary_path=binary,
        )
        is None
    )


def test_plan_refs_bind_verified_treatment_manifests(
    valid_plan: dict[str, object], treatments: tuple[dict[str, object], dict[str, object]]
) -> None:
    """Catches a plan that seals valid strings referring to the wrong treatment."""
    stock_treatment, modified_treatment = treatments
    valid_plan["stockTreatmentRef"] = "wrong-treatment"
    with pytest.raises(BatchPlanError, match="stock treatment reference"):
        seal_candidate_run_plan(
            valid_plan, stock_treatment=stock_treatment, modified_treatment=modified_treatment
        )


@pytest.mark.parametrize("field", ["caseAnswerSchemaSha256", "replicationCount", "retryPolicy"])
def test_effective_condition_parity_rejects_undeclared_runtime_differences(
    valid_plan: dict[str, object], field: str
) -> None:
    """Catches parity that ignores material plan differences outside its commitment."""
    modified = dict(valid_plan)
    modified[field] = "e" * 64 if field == "caseAnswerSchemaSha256" else (
        2 if field == "replicationCount" else {"maxRetries": 1}
    )
    with pytest.raises(BatchPlanError, match="undeclared condition difference"):
        verify_effective_condition_parity(valid_plan, modified)


def test_tree_hash_rejects_depth_two_directory_replaced_after_parent_scan(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Catches a late replacement of an already-open depth-one child directory."""
    root = tmp_path / "root"
    deep = root / "one" / "two"
    outside = tmp_path / "outside"
    detached = tmp_path / "detached"
    deep.mkdir(parents=True)
    outside.mkdir()
    (deep / "safe").write_bytes(b"safe")
    scans = 0
    scandir = batch_plan.os.scandir

    def scan_then_replace(path: object):
        nonlocal scans
        entries = list(scandir(path))
        scans += 1
        if scans == 3:
            deep.rename(detached)
            deep.symlink_to(outside, target_is_directory=True)
        return iter(entries)

    monkeypatch.setattr(batch_plan.os, "scandir", scan_then_replace)
    with pytest.raises(BatchPlanError, match="tree changed|symbolic link"):
        sha256_tree(root)


def test_tree_hash_fallback_backend_handles_a_regular_tree(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Catches a platform fallback that cannot hash ordinary non-linked trees."""
    root = tmp_path / "root"
    root.mkdir()
    (root / "safe").write_bytes(b"safe")
    monkeypatch.setattr(batch_plan, "_descriptor_traversal_available", lambda: False)
    assert sha256_tree(root) == sha256_json(
        {"entries": [{"mode": stat.S_IMODE((root / "safe").stat().st_mode), "path": "safe", "sha256": hashlib.sha256(b"safe").hexdigest(), "size": 4}]}
    )


def test_effective_condition_parity_rejects_missing_key_and_plan_id_mutation(
    valid_plan: dict[str, object]
) -> None:
    """Catches missing-vs-null equality and unbound plan identity changes."""
    modified = dict(valid_plan)
    modified["forbidden"] = None
    with pytest.raises(BatchPlanError, match="undeclared condition difference"):
        verify_effective_condition_parity(valid_plan, modified)
    modified = dict(valid_plan)
    modified["planId"] = "other-plan"
    with pytest.raises(BatchPlanError, match="undeclared condition difference"):
        verify_effective_condition_parity(valid_plan, modified)


def test_fallback_rejects_root_replaced_during_first_enumeration(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Catches fallback traversal that adopts a root swapped after scan begins."""
    root, detached = tmp_path / "root", tmp_path / "detached"
    root.mkdir()
    (root / "old").write_bytes(b"old")
    scandir = batch_plan.os.scandir

    def scan_then_replace(path: Path):
        entries = list(scandir(path))
        root.rename(detached)
        root.mkdir()
        (root / "new").write_bytes(b"new")
        return iter(entries)

    monkeypatch.setattr(batch_plan, "_descriptor_traversal_available", lambda: False)
    monkeypatch.setattr(batch_plan.os, "scandir", scan_then_replace)
    with pytest.raises(BatchPlanError, match="tree changed"):
        sha256_tree(root)


@pytest.mark.parametrize("root_kind", ["missing", "file", "symlink"])
def test_fallback_rejects_unsafe_root_before_enumeration(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, root_kind: str
) -> None:
    """Catches fallback roots that are missing, non-directories, or links."""
    root = tmp_path / "root"
    if root_kind == "file":
        root.write_bytes(b"file")
    elif root_kind == "symlink":
        target = tmp_path / "target"
        target.mkdir()
        root.symlink_to(target, target_is_directory=True)
    monkeypatch.setattr(batch_plan, "_descriptor_traversal_available", lambda: False)
    with pytest.raises(BatchPlanError, match="tree root"):
        sha256_tree(root)
