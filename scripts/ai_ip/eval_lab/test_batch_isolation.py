import os
import hashlib
import stat
import sys
from dataclasses import replace
from pathlib import Path

import pytest


sys.path.insert(0, str(Path(__file__).resolve().parent))
from batch_isolation import (  # noqa: E402
    IsolationError,
    cleanup_attempt_cell,
    create_attempt_cell,
    mark_receipts_sealed,
    verify_attempt_cells_disjoint,
)


REPO_ROOT = Path(__file__).resolve().parents[3]
FIXTURE_ROOT = REPO_ROOT / "ai-ip-evals" / "lab" / "fixtures" / "batch-runner"


@pytest.fixture
def profile() -> dict[str, object]:
    return {
        "schemaVersion": 1,
        "sandboxMode": "workspace-write",
        "approvalPolicy": "never",
        "networkMode": "offline",
        "networkAllowlist": [],
        "writablePaths": ["workspace"],
        "externalTools": [],
        "environmentAllowlist": ["LANG", "AWS_PROFILE"],
        "maxWallClockSeconds": 300,
        "maxOutputBytes": 1_048_576,
    }


@pytest.fixture
def source_environment() -> dict[str, str]:
    return {
        "PATH": "/safe/bin",
        "SHELL": "/bin/sh",
        "LANG": "en_US.UTF-8",
        "LC_CTYPE": "en_US.UTF-8",
        "AWS_PROFILE": "offline-profile",
        "UNDECLARED": "must-not-leak",
        "HOME": "/real/home",
        "CODEX_HOME": "/real/codex-home",
        "TMPDIR": "/real/temp",
        "HTTPS_PROXY": "https://proxy.example.invalid:8443",
        "VOLCENGINE_API_KEY": "credential-must-not-leak",
        "PROMPTFOO_CONFIG_DIR": "/real/promptfoo",
    }


def _create_cell(
    tmp_path: Path,
    attempt_id: str,
    profile: dict[str, object],
    source_environment: dict[str, str],
):
    unique_attempt_id = _unique_attempt_id(tmp_path, attempt_id)
    return create_attempt_cell(
        tmp_path,
        "pair-1",
        unique_attempt_id,
        FIXTURE_ROOT / "stock-seed",
        FIXTURE_ROOT / "workspace",
        profile,
        source_environment,
    )


def _unique_attempt_id(tmp_path: Path, attempt_id: str) -> str:
    return (
        hashlib.sha256(str(tmp_path).encode("utf-8")).hexdigest()[:12]
        + "-"
        + attempt_id
    )


def test_cells_share_no_state_and_copy_independent_bytes(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches cells that reuse a root, seed file identity, or workspace bytes."""
    left = _create_cell(tmp_path, "opaque-1", profile, source_environment)
    right = create_attempt_cell(
        tmp_path,
        "pair-1",
        _unique_attempt_id(tmp_path, "opaque-2"),
        FIXTURE_ROOT / "modified-seed",
        FIXTURE_ROOT / "workspace",
        profile,
        source_environment,
    )

    verify_attempt_cells_disjoint(left, right)
    (left.workspace / "sentinel").write_text("left", encoding="utf-8")

    left_input = left.workspace / "case-input.json"
    right_input = right.workspace / "case-input.json"
    seed_input = FIXTURE_ROOT / "workspace" / "case-input.json"
    assert not (right.workspace / "sentinel").exists()
    assert (
        left_input.read_bytes() == right_input.read_bytes() == seed_input.read_bytes()
    )
    identities = {
        (path.stat().st_dev, path.stat().st_ino)
        for path in (left_input, right_input, seed_input)
    }
    assert len(identities) == 3
    assert left.root != right.root
    assert left.pair_id == right.pair_id == "pair-1"
    assert left.attempt_id != right.attempt_id


def test_cell_tree_is_private_and_contains_no_links(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches copying that preserves permissive modes or filesystem aliases."""
    cell = _create_cell(tmp_path, "opaque-modes", profile, source_environment)

    for path in (
        cell.root,
        cell.home,
        cell.workspace,
        cell.cache,
        cell.temp,
        cell.logs,
        cell.promptfoo,
    ):
        assert path.is_dir()
        if os.name != "nt":
            assert stat.S_IMODE(path.stat().st_mode) == 0o700
    for path in (cell.home / "config.toml", cell.workspace / "case-input.json"):
        metadata = path.lstat()
        assert stat.S_ISREG(metadata.st_mode)
        assert not stat.S_ISLNK(metadata.st_mode)
        assert metadata.st_nlink == 1
        if os.name != "nt":
            assert stat.S_IMODE(metadata.st_mode) == 0o600


def test_candidate_environment_is_an_exact_deny_by_default_allowlist(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches ambient credentials, proxies, homes, or undeclared values leaking in."""
    cell = _create_cell(tmp_path, "opaque-env", profile, source_environment)

    assert cell.environment == {
        "PATH": "/safe/bin",
        "SHELL": "/bin/sh",
        "LANG": "en_US.UTF-8",
        "LC_CTYPE": "en_US.UTF-8",
        "AWS_PROFILE": "offline-profile",
        "HOME": str(cell.home),
        "CODEX_HOME": str(cell.home),
        "TMPDIR": str(cell.temp),
        "PROMPTFOO_CACHE_PATH": str(cell.cache / "promptfoo"),
        "PROMPTFOO_CONFIG_DIR": str(cell.promptfoo),
        "PROMPTFOO_OUTPUT_PATH": str(cell.promptfoo / "output.json"),
        "PROMPTFOO_CACHE_ENABLED": "false",
        "FORCE_COLOR": "0",
        "NO_PROXY": "127.0.0.1,localhost,::1",
    }

    with pytest.raises(TypeError):
        cell.environment["VOLCENGINE_API_KEY"] = "late-secret"


@pytest.mark.parametrize(
    "forbidden_name",
    ["VOLCENGINE_API_KEY", "HTTPS_PROXY", "ALL_PROXY", "SERVICE_TOKEN", "PASSWORD"],
)
def test_profile_cannot_allow_credentials_or_proxies(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
    forbidden_name: str,
) -> None:
    """Catches a profile turning its extra allowlist into a secret escape hatch."""
    unsafe = {**profile, "environmentAllowlist": ["LANG", forbidden_name]}
    source_environment[forbidden_name] = "sensitive"

    with pytest.raises(IsolationError, match="secret|proxy"):
        _create_cell(
            tmp_path, f"opaque-secret-{forbidden_name}", unsafe, source_environment
        )


@pytest.mark.parametrize("link_kind", ["symbolic", "hard"])
def test_linked_seed_entries_are_rejected(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
    link_kind: str,
) -> None:
    """Catches a seed copier that follows a link or preserves shared file identity."""
    seed = tmp_path / f"{link_kind}-seed"
    workspace = tmp_path / f"{link_kind}-workspace"
    seed.mkdir()
    workspace.mkdir()
    target = seed / "target"
    target.write_bytes(b"seed")
    linked = seed / "linked"
    if link_kind == "symbolic":
        try:
            linked.symlink_to(target)
        except (NotImplementedError, OSError):
            pytest.skip("symbolic links are unavailable")
    else:
        try:
            os.link(target, linked)
        except (NotImplementedError, OSError):
            pytest.skip("hard links are unavailable")

    with pytest.raises(IsolationError, match="link"):
        create_attempt_cell(
            tmp_path,
            "pair-linked",
            f"attempt-{link_kind}",
            seed,
            workspace,
            profile,
            source_environment,
        )


def test_roots_inside_a_git_worktree_are_rejected(
    profile: dict[str, object], source_environment: dict[str, str]
) -> None:
    """Catches candidate state being placed in this repository or a worktree."""
    with pytest.raises(IsolationError, match="outside every Git worktree"):
        create_attempt_cell(
            REPO_ROOT,
            "pair-repo",
            "attempt-repo",
            FIXTURE_ROOT / "stock-seed",
            FIXTURE_ROOT / "workspace",
            profile,
            source_environment,
        )


def test_symlinked_cell_base_is_rejected(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches a root alias that could redirect writes outside the declared base."""
    real_base = tmp_path / "real"
    real_base.mkdir()
    linked_base = tmp_path / "linked"
    try:
        linked_base.symlink_to(real_base, target_is_directory=True)
    except (NotImplementedError, OSError):
        pytest.skip("symbolic links are unavailable")

    with pytest.raises(IsolationError, match="symlink|reparse"):
        _create_cell(linked_base, "attempt-linked-base", profile, source_environment)
    assert list(real_base.iterdir()) == []


def test_attempt_id_cannot_be_reused(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches ID reuse after either an active cell or a sealed cleanup."""
    attempt_id = _unique_attempt_id(tmp_path, "opaque-reused")
    cell = _create_cell(tmp_path, "opaque-reused", profile, source_environment)
    with pytest.raises(IsolationError, match="attempt ID was already used"):
        create_attempt_cell(
            tmp_path,
            "pair-2",
            attempt_id,
            FIXTURE_ROOT / "modified-seed",
            FIXTURE_ROOT / "workspace",
            profile,
            source_environment,
        )

    mark_receipts_sealed(cell)
    cleanup_attempt_cell(cell)
    with pytest.raises(IsolationError, match="attempt ID was already used"):
        _create_cell(tmp_path, "opaque-reused", profile, source_environment)


def test_disjoint_verifier_rejects_overlapping_roots(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches callers substituting one attempt's root for another's."""
    left = _create_cell(tmp_path, "opaque-left", profile, source_environment)
    right = _create_cell(tmp_path, "opaque-right", profile, source_environment)

    with pytest.raises(IsolationError, match="root|identity|overlap"):
        verify_attempt_cells_disjoint(left, replace(right, root=left.root))


@pytest.mark.parametrize("field", ["cache", "temp", "logs"])
def test_disjoint_verifier_rejects_shared_runtime_paths(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
    field: str,
) -> None:
    """Catches shared mutable cache, temporary, or log state between arms."""
    left = _create_cell(tmp_path, f"opaque-left-{field}", profile, source_environment)
    right = _create_cell(tmp_path, f"opaque-right-{field}", profile, source_environment)

    with pytest.raises(IsolationError, match=f"{field} path"):
        verify_attempt_cells_disjoint(
            right, replace(left, **{field: getattr(right, field)})
        )


def test_disjoint_verifier_rejects_cross_cell_hardlinks(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches independently named cells that still share a writable inode."""
    left = _create_cell(tmp_path, "opaque-hard-left", profile, source_environment)
    right = _create_cell(tmp_path, "opaque-hard-right", profile, source_environment)
    try:
        os.link(left.workspace / "case-input.json", right.workspace / "alias.json")
    except (NotImplementedError, OSError):
        pytest.skip("hard links are unavailable")

    with pytest.raises(IsolationError, match="link"):
        verify_attempt_cells_disjoint(left, right)


def test_cleanup_before_receipt_seal_is_rejected(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches deletion of evidence before its receipts are durably marked sealed."""
    cell = _create_cell(tmp_path, "opaque-unsealed", profile, source_environment)

    with pytest.raises(IsolationError, match="receipts are not sealed"):
        cleanup_attempt_cell(cell)
    assert cell.root.is_dir()


def test_cleanup_removes_only_the_exact_sealed_cell(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches cleanup that broadens from the cell root to its parent or sibling."""
    cell = _create_cell(tmp_path, "opaque-clean", profile, source_environment)
    outside = tmp_path / "outside-sentinel"
    outside.write_text("retain", encoding="utf-8")

    mark_receipts_sealed(cell)
    cleanup_attempt_cell(cell)

    assert not cell.root.exists()
    assert outside.read_text(encoding="utf-8") == "retain"


def test_cleanup_rejects_a_replaced_cell_root(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches cleanup deleting a different directory substituted at the same path."""
    cell = _create_cell(tmp_path, "opaque-replaced", profile, source_environment)
    mark_receipts_sealed(cell)
    original = tmp_path / "original-cell"
    cell.root.rename(original)
    cell.root.mkdir(mode=0o700)
    cell.receipt_marker.write_bytes(b"forged")
    os.chmod(cell.receipt_marker, 0o600)

    with pytest.raises(IsolationError, match="identity"):
        cleanup_attempt_cell(cell)
    assert cell.root.is_dir()
    assert original.is_dir()
