import hashlib
import os
import shutil
import sys
from dataclasses import replace
from pathlib import Path
from types import MappingProxyType

import pytest


sys.path.insert(0, str(Path(__file__).resolve().parent))
import batch_isolation  # noqa: E402
from batch_isolation import (  # noqa: E402
    IsolationError,
    cleanup_attempt_cell,
    create_attempt_cell,
    mark_receipts_sealed,
    verify_attempt_cells_disjoint,
)


REPO_ROOT = Path(__file__).resolve().parents[3]
FIXTURES = REPO_ROOT / "ai-ip-evals" / "lab" / "fixtures" / "batch-runner"


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
        "AWS_PROFILE": "offline-profile",
    }


def _attempt_id(base: Path, label: str) -> str:
    prefix = hashlib.sha256(str(base).encode("utf-8")).hexdigest()[:16]
    return f"{prefix}-{label}"


def _cell(
    base: Path,
    label: str,
    profile: dict[str, object],
    source_environment: dict[str, str],
):
    return create_attempt_cell(
        base,
        "pair-security",
        _attempt_id(base.parent, label),
        FIXTURES / "stock-seed",
        FIXTURES / "workspace",
        profile,
        source_environment,
    )


def _case_alias(path: Path) -> Path:
    parts = list(path.parts)
    for index, part in enumerate(parts):
        if not any(character.isalpha() for character in part):
            continue
        swapped = part.swapcase()
        if swapped == part:
            continue
        candidate = Path(*parts[:index], swapped, *parts[index + 1 :])
        try:
            if candidate.exists() and os.path.samefile(candidate, path):
                return candidate
        except OSError:
            continue
    pytest.skip("filesystem exposes no case-insensitive alias")


def test_known_receipt_marker_bytes_cannot_authorize_cleanup(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches cleanup trusting a public marker instead of creator capability state."""
    cell = _cell(tmp_path, "forged-marker", profile, source_environment)
    cell.receipt_marker.write_bytes(b"receipts-sealed-v1\n")
    os.chmod(cell.receipt_marker, 0o600)

    with pytest.raises(IsolationError, match="receipts are not sealed|forged"):
        cleanup_attempt_cell(cell)
    assert cell.root.is_dir()


def test_base_dict_apis_cannot_mutate_candidate_environment(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches a dict subclass whose overrides are bypassed by dict.__setitem__."""
    cell = _cell(tmp_path, "base-dict", profile, source_environment)

    with pytest.raises((TypeError, AttributeError)):
        dict.__setitem__(cell.environment, "VOLCENGINE_API_KEY", "late-secret")
    assert "VOLCENGINE_API_KEY" not in cell.environment


def test_replaced_environment_mapping_is_rejected_by_lifecycle_gate(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches lifecycle checks that validate controlled values but allow extra keys."""
    cell = _cell(tmp_path, "replaced-env", profile, source_environment)
    forged = replace(
        cell,
        environment=MappingProxyType(
            {**dict(cell.environment), "VOLCENGINE_API_KEY": "late-secret"}
        ),
    )

    with pytest.raises(IsolationError, match="environment"):
        mark_receipts_sealed(forged)
    assert not cell.receipt_marker.exists()


@pytest.mark.parametrize(
    "unsafe_name",
    ["GITHUB_PAT", "AWS_ACCESS_KEY_ID", "CARGO_HOME", "UNKNOWN_SETTING"],
)
def test_profile_environment_catalog_rejects_unknown_or_shared_state_names(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
    unsafe_name: str,
) -> None:
    """Catches blacklist-based profile inheritance that fails open on new names."""
    unsafe = {**profile, "environmentAllowlist": [unsafe_name]}
    source_environment[unsafe_name] = "sensitive-or-shared"

    with pytest.raises(IsolationError, match="not an approved non-secret"):
        _cell(tmp_path, f"unsafe-{unsafe_name}", unsafe, source_environment)


def test_attempt_id_reservation_is_global_across_candidate_bases(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches caller-selected bases acting as independent uniqueness registries."""
    left_base = tmp_path / "left"
    right_base = tmp_path / "right"
    left_base.mkdir(mode=0o700)
    right_base.mkdir(mode=0o700)
    attempt_id = _attempt_id(tmp_path, "global-id")
    create_attempt_cell(
        left_base,
        "pair-left",
        attempt_id,
        FIXTURES / "stock-seed",
        FIXTURES / "workspace",
        profile,
        source_environment,
    )

    with pytest.raises(IsolationError, match="attempt ID was already used"):
        create_attempt_cell(
            right_base,
            "pair-right",
            attempt_id,
            FIXTURES / "modified-seed",
            FIXTURES / "workspace",
            profile,
            source_environment,
        )


def test_case_alias_of_worktree_is_rejected(
    profile: dict[str, object], source_environment: dict[str, str]
) -> None:
    """Catches case-sensitive lexical containment on a case-insensitive filesystem."""
    alias = _case_alias(REPO_ROOT)

    with pytest.raises(IsolationError, match="outside every Git worktree"):
        create_attempt_cell(
            alias,
            "pair-case-alias",
            "case-alias-worktree",
            FIXTURES / "stock-seed",
            FIXTURES / "workspace",
            profile,
            source_environment,
        )


def test_disjoint_verifier_rejects_same_directory_through_case_alias(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches same-file roots whose lexical spellings differ only by case."""
    cell = _cell(tmp_path, "case-disjoint", profile, source_environment)
    alias_root = _case_alias(cell.root)
    environment = {
        key: value.replace(str(cell.root), str(alias_root), 1)
        if value.startswith(str(cell.root))
        else value
        for key, value in cell.environment.items()
    }
    alias = replace(
        cell,
        root=alias_root,
        home=alias_root / "home",
        workspace=alias_root / "workspace",
        cache=alias_root / "cache",
        temp=alias_root / "temp",
        logs=alias_root / "logs",
        promptfoo=alias_root / "promptfoo",
        receipt_marker=alias_root / ".receipts-sealed",
        environment=MappingProxyType(environment),
    )

    with pytest.raises(IsolationError, match="same filesystem|overlap|environment"):
        verify_attempt_cells_disjoint(cell, alias)


def test_parent_replacement_cannot_redirect_seed_copy_or_failed_cleanup(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Catches path-based copy/chmod and failed cleanup after the base is swapped."""
    base = tmp_path / "base"
    outside = tmp_path / "outside"
    moved = tmp_path / "moved-base"
    base.mkdir(mode=0o700)
    outside.mkdir(mode=0o700)
    original = batch_isolation._write_snapshot
    attacked = False

    def swap_then_copy(*args: object, **kwargs: object) -> None:
        nonlocal attacked
        if not attacked:
            root_name = next(
                path.name for path in base.iterdir() if path.name.startswith("cell-")
            )
            target = outside / root_name
            for name in ("home", "workspace", "cache", "temp", "logs", "promptfoo"):
                (target / name).mkdir(parents=True, mode=0o700)
            (target / "home" / "outside-sentinel").write_text(
                "retain", encoding="utf-8"
            )
            try:
                base.rename(moved)
                base.symlink_to(outside, target_is_directory=True)
            except PermissionError:
                if os.name != "nt":
                    raise
            attacked = True
        original(*args, **kwargs)

    monkeypatch.setattr(batch_isolation, "_write_snapshot", swap_then_copy)
    attempt_id = _attempt_id(tmp_path, "copy-swap")
    if os.name == "nt":
        created = create_attempt_cell(
            base,
            "pair-copy-swap",
            attempt_id,
            FIXTURES / "stock-seed",
            FIXTURES / "workspace",
            profile,
            source_environment,
        )
        assert created.root.is_dir()
        mark_receipts_sealed(created)
        cleanup_attempt_cell(created)
    else:
        with pytest.raises(IsolationError):
            create_attempt_cell(
                base,
                "pair-copy-swap",
                attempt_id,
                FIXTURES / "stock-seed",
                FIXTURES / "workspace",
                profile,
                source_environment,
            )

    assert attacked
    if os.name != "nt":
        target = next(
            path for path in outside.iterdir() if path.name.startswith("cell-")
        )
        assert (target / "home" / "outside-sentinel").read_text(
            encoding="utf-8"
        ) == "retain"
        assert not (target / "home" / "config.toml").exists()

    retry_base = tmp_path / "retry"
    retry_base.mkdir(mode=0o700)
    with pytest.raises(IsolationError, match="attempt ID was already used"):
        create_attempt_cell(
            retry_base,
            "pair-retry",
            attempt_id,
            FIXTURES / "stock-seed",
            FIXTURES / "workspace",
            profile,
            source_environment,
        )


def test_cleanup_swap_never_deletes_substituted_directory(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Catches validation followed by path-based recursive deletion of a substitute."""
    cell = _cell(tmp_path, "cleanup-swap", profile, source_environment)
    mark_receipts_sealed(cell)
    moved = tmp_path / "moved-original"
    attacked = False

    if hasattr(batch_isolation, "_delete_cell_root"):
        original_delete = batch_isolation._delete_cell_root

        def attacked_delete(*args: object, **kwargs: object) -> None:
            nonlocal attacked
            try:
                cell.root.rename(moved)
                cell.root.mkdir(mode=0o700)
                (cell.root / "substitute-sentinel").write_text(
                    "retain", encoding="utf-8"
                )
            except PermissionError:
                if os.name != "nt":
                    raise
            attacked = True
            original_delete(*args, **kwargs)

        target = batch_isolation
        name = "_delete_cell_root"
        replacement = attacked_delete
    else:
        original_delete = shutil.rmtree

        def attacked_rmtree(path: Path) -> None:
            nonlocal attacked
            cell.root.rename(moved)
            cell.root.mkdir(mode=0o700)
            (cell.root / "substitute-sentinel").write_text("retain", encoding="utf-8")
            attacked = True
            original_delete(path)

        target = batch_isolation.shutil
        name = "rmtree"
        replacement = attacked_rmtree

    with monkeypatch.context() as context:
        context.setattr(target, name, replacement)
        if os.name == "nt":
            cleanup_attempt_cell(cell)
        else:
            with pytest.raises(IsolationError, match="identity|substituted|cleanup"):
                cleanup_attempt_cell(cell)

    assert attacked
    if os.name == "nt":
        assert not cell.root.exists()
    else:
        assert (cell.root / "substitute-sentinel").read_text(
            encoding="utf-8"
        ) == "retain"
        assert moved.is_dir()


def test_required_promptfoo_cache_layout_is_identity_bound(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches lifecycle validation that omits the required cache/promptfoo directory."""
    cell = _cell(tmp_path, "missing-cache", profile, source_environment)
    (cell.cache / "promptfoo").rmdir()

    with pytest.raises(IsolationError, match="cache/promptfoo|required layout"):
        mark_receipts_sealed(cell)
    assert not cell.receipt_marker.exists()


@pytest.mark.parametrize("hostile_kind", ["link", "oversized", "deep"])
def test_hostile_candidate_content_can_be_sealed_and_cleaned_safely(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
    hostile_kind: str,
) -> None:
    """Catches unbounded whole-cell validation making failure evidence undeletable."""
    cell = _cell(tmp_path, f"hostile-{hostile_kind}", profile, source_environment)
    outside = tmp_path / f"outside-{hostile_kind}"
    outside.write_text("retain", encoding="utf-8")
    if hostile_kind == "link":
        try:
            (cell.workspace / "hostile").symlink_to(outside)
        except (NotImplementedError, OSError):
            pytest.skip("symbolic links are unavailable")
    elif hostile_kind == "oversized":
        with (cell.workspace / "hostile").open("wb") as stream:
            stream.truncate(9 * 1024 * 1024)
    else:
        current = cell.workspace
        for _ in range(300):
            current /= "d"
            current.mkdir()
        (current / "leaf").write_bytes(b"leaf")

    mark_receipts_sealed(cell)
    cleanup_attempt_cell(cell)

    assert not cell.root.exists()
    assert outside.read_text(encoding="utf-8") == "retain"


def test_windows_environment_projection_has_no_real_profile_fallback(
    tmp_path: Path,
) -> None:
    """Catches a Windows candidate inheriting USERPROFILE, TEMP, APPDATA, or cache."""
    home = tmp_path / "home"
    cache = tmp_path / "cache"
    temp = tmp_path / "temp"
    projection = getattr(
        batch_isolation, "_windows_environment_overrides", lambda *_: {}
    )(home, cache, temp)

    assert projection == {
        "USERPROFILE": str(home),
        "HOMEDRIVE": home.drive,
        "HOMEPATH": str(home)[len(home.drive) :],
        "APPDATA": str(home / "AppData" / "Roaming"),
        "LOCALAPPDATA": str(cache),
        "TEMP": str(temp),
        "TMP": str(temp),
    }


@pytest.mark.skipif(os.name != "nt", reason="native Windows privacy assertion")
def test_windows_backend_uses_verified_private_acl_or_fails_closed(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches Windows success based only on chmod rather than a protected DACL."""
    try:
        cell = _cell(tmp_path, "windows-acl", profile, source_environment)
    except IsolationError as error:
        assert "secure Windows" in str(error) or "protected DACL" in str(error)
        return
    verifier = getattr(batch_isolation, "_windows_path_has_private_acl", None)
    assert verifier is not None
    for path in (
        cell.root,
        cell.home,
        cell.workspace,
        cell.cache,
        cell.temp,
        cell.logs,
    ):
        assert verifier(path)
