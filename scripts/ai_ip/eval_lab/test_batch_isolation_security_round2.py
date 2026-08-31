import hashlib
import os
import stat
import sys
from dataclasses import replace
from pathlib import Path

import pytest


sys.path.insert(0, str(Path(__file__).resolve().parent))
import batch_isolation  # noqa: E402
import batch_isolation_posix  # noqa: E402
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
    digest = hashlib.sha256(f"{base}:{label}".encode()).hexdigest()[:20]
    return f"round2-{digest}"


def _cell(
    base: Path,
    label: str,
    profile: dict[str, object],
    source_environment: dict[str, str],
):
    return create_attempt_cell(
        base,
        "pair-round2",
        _attempt_id(base, label),
        FIXTURES / "stock-seed",
        FIXTURES / "workspace",
        profile,
        source_environment,
    )


def _seal_and_cleanup(cell: object) -> None:
    mark_receipts_sealed(cell)
    cleanup_attempt_cell(cell)


@pytest.mark.skipif(os.name == "nt", reason="POSIX deterministic rename probe")
def test_validated_base_capability_is_transferred_without_path_reopen(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Catches closing the proven base fd before backend construction."""
    base = tmp_path / "base"
    moved = tmp_path / "proven-base"
    replacement = tmp_path / "replacement-sentinel"
    base.mkdir(mode=0o700)
    original = batch_isolation.reserve_attempt

    def swap_after_validation(*args: object, **kwargs: object):
        reservation = original(*args, **kwargs)
        base.rename(moved)
        base.mkdir(mode=0o700)
        replacement.write_text("retain", encoding="utf-8")
        return reservation

    monkeypatch.setattr(batch_isolation, "reserve_attempt", swap_after_validation)
    with pytest.raises(IsolationError, match="base.*identity|validated.*base"):
        _cell(base, "base-handoff", profile, source_environment)

    assert list(base.iterdir()) == []
    assert replacement.read_text(encoding="utf-8") == "retain"
    assert not any(path.name.startswith("cell-") for path in moved.iterdir())


@pytest.mark.skipif(os.name == "nt", reason="POSIX quarantine race probe")
def test_cleanup_proves_original_quarantine_was_removed_before_success(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Catches rmdir of a substituted empty quarantine terminal name."""
    cell = _cell(tmp_path, "terminal-cleanup", profile, source_environment)
    mark_receipts_sealed(cell)
    original = batch_isolation_posix.PosixCellFilesystem._delete_contents
    moved = tmp_path / "moved-quarantine"

    def swap_after_contents(filesystem: object) -> None:
        original(filesystem)
        quarantine = tmp_path / filesystem.quarantine_name
        quarantine.rename(moved)
        quarantine.mkdir(mode=0o700)

    monkeypatch.setattr(
        batch_isolation_posix.PosixCellFilesystem,
        "_delete_contents",
        swap_after_contents,
    )
    with pytest.raises(IsolationError, match="cleanup|identity|original"):
        cleanup_attempt_cell(cell)

    assert moved.is_dir()
    assert not cell.root.exists()
    quarantine_name = batch_isolation._CELLS[
        cell._capability
    ].filesystem.quarantine_name
    quarantine = tmp_path / quarantine_name
    if quarantine.exists():
        quarantine.rmdir()
    moved.rename(quarantine)
    monkeypatch.setattr(
        batch_isolation_posix.PosixCellFilesystem, "_delete_contents", original
    )
    cleanup_attempt_cell(cell)
    assert not moved.exists()


def test_object_setattr_cannot_mutate_canonical_cell_binding(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches canonical state aliasing the caller-visible frozen dataclass."""
    cell = _cell(tmp_path, "object-setattr", profile, source_environment)
    outside = tmp_path / "outside-workspace"
    outside.mkdir(mode=0o700)
    object.__setattr__(cell, "workspace", outside)

    with pytest.raises(IsolationError, match="workspace.*binding"):
        mark_receipts_sealed(cell)
    assert not cell.receipt_marker.exists()


def test_dataclass_replacement_cannot_replay_public_cell_capability(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches accepting a copied public object solely because fields match."""
    cell = _cell(tmp_path, "dataclass-replay", profile, source_environment)
    replay = replace(cell)

    with pytest.raises(IsolationError, match="capability|binding|public object"):
        mark_receipts_sealed(replay)
    assert not cell.receipt_marker.exists()


@pytest.mark.skipif(os.name == "nt", reason="POSIX rename/mode probe")
@pytest.mark.parametrize("attack", ["replace", "mode"])
def test_lifecycle_rebinds_every_required_layout_entry(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
    attack: str,
) -> None:
    """Catches validation of retained handles without the live root entries."""
    cell = _cell(tmp_path, f"layout-{attack}", profile, source_environment)
    if attack == "replace":
        cell.workspace.rename(tmp_path / "moved-workspace")
        cell.workspace.mkdir(mode=0o700)
    else:
        os.chmod(cell.workspace, 0o755)

    with pytest.raises(IsolationError, match="workspace|required layout|private"):
        mark_receipts_sealed(cell)


def test_undeclared_nonstandard_locale_name_is_not_inherited(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches prefix-based locale inheritance admitting credential-like names."""
    source_environment["LC_API_KEY"] = "must-not-enter"
    cell = _cell(tmp_path, "locale-source", profile, source_environment)
    assert "LC_API_KEY" not in cell.environment


def test_nonstandard_locale_name_cannot_be_profile_declared(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches LC_ prefix being treated as a positive profile catalog."""
    unsafe = {**profile, "environmentAllowlist": ["LC_API_KEY"]}
    source_environment["LC_API_KEY"] = "must-not-enter"
    with pytest.raises(IsolationError, match="approved non-secret"):
        _cell(tmp_path, "locale-profile", unsafe, source_environment)


@pytest.mark.skipif(not Path("/dev/fd").is_dir(), reason="descriptor count unavailable")
def test_completed_attempts_close_reservation_and_cell_descriptors(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches one permanently leaked reservation fd per completed attempt."""
    before = len(list(Path("/dev/fd").iterdir()))
    for index in range(24):
        _seal_and_cleanup(
            _cell(tmp_path, f"descriptor-{index}", profile, source_environment)
        )
    after = len(list(Path("/dev/fd").iterdir()))
    assert after - before < 6


@pytest.mark.skipif(os.name == "nt", reason="POSIX failure resource probe")
def test_failed_public_construction_closes_resources_and_removes_state(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Catches failed construction retaining cell state, fds, or partial roots."""
    base = tmp_path / "base"
    base.mkdir(mode=0o700)
    before_fds = len(list(Path("/dev/fd").iterdir()))
    before_cells = set(batch_isolation._CELLS)
    original = batch_isolation._write_snapshot
    calls = 0

    def fail_second_copy(*args: object, **kwargs: object) -> None:
        nonlocal calls
        calls += 1
        if calls == 2:
            raise OSError("injected copy failure")
        original(*args, **kwargs)

    monkeypatch.setattr(batch_isolation, "_write_snapshot", fail_second_copy)
    with pytest.raises(OSError, match="injected copy failure"):
        _cell(base, "partial-create", profile, source_environment)

    assert set(batch_isolation._CELLS) == before_cells
    assert not any(
        path.name.startswith(("cell-", ".deleting-")) for path in base.iterdir()
    )
    assert len(list(Path("/dev/fd").iterdir())) - before_fds < 3


@pytest.mark.skipif(os.name == "nt", reason="POSIX backend rollback probe")
def test_posix_partial_layout_creation_rolls_back_nested_directory(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Catches closing cache handles before rolling back cache/promptfoo."""
    base_fd = batch_isolation_posix.open_directory(tmp_path)
    original = batch_isolation_posix._mkdir_child
    calls = 0

    def fail_after_nested_create(parent_fd: int, name: str) -> int:
        nonlocal calls
        calls += 1
        handle = original(parent_fd, name)
        if calls == 7:
            batch_isolation_posix.close_handle(handle)
            raise OSError("injected nested layout failure")
        return handle

    monkeypatch.setattr(batch_isolation_posix, "_mkdir_child", fail_after_nested_create)
    with pytest.raises(OSError, match="injected nested layout failure"):
        batch_isolation_posix.PosixCellFilesystem.create(
            tmp_path, "cell-partial-layout", base_fd
        )
    assert not (tmp_path / "cell-partial-layout").exists()


@pytest.mark.skipif(os.name == "nt", reason="POSIX streaming enumeration probe")
def test_disjoint_scan_does_not_materialize_directory_entries(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Catches os.listdir materializing attacker-sized directories before bounds."""
    left = _cell(tmp_path, "stream-left", profile, source_environment)
    right = _cell(tmp_path, "stream-right", profile, source_environment)

    def reject_listdir(*args: object, **kwargs: object) -> list[str]:
        raise AssertionError("materialized directory")

    monkeypatch.setattr(batch_isolation_posix.os, "listdir", reject_listdir)
    verify_attempt_cells_disjoint(left, right)


@pytest.mark.skipif(os.name == "nt", reason="POSIX bounded cleanup probe")
def test_bounded_high_fanout_cleanup_makes_resumable_progress(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Catches a hard bound that permanently strands a quarantined cell."""
    cell = _cell(tmp_path, "bounded-cleanup", profile, source_environment)
    for index in range(18):
        directory = cell.workspace / f"d-{index}"
        directory.mkdir()
        (directory / "value").write_bytes(b"x" * 32)
    mark_receipts_sealed(cell)
    monkeypatch.setattr(batch_isolation_posix, "_MAX_DELETE_ENTRIES", 7)

    failures = 0
    for _ in range(20):
        try:
            cleanup_attempt_cell(cell)
            break
        except IsolationError as error:
            assert "cleanup" in str(error)
            failures += 1
    else:
        pytest.fail("bounded cleanup never completed")
    assert failures > 0
    assert not cell.root.exists()


@pytest.mark.skipif(not hasattr(os, "fork"), reason="fork is unavailable")
def test_forked_child_cannot_use_parent_cell_capability(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches inherited creator keys, registries, and cell descriptors after fork."""
    cell = _cell(tmp_path, "fork-boundary", profile, source_environment)
    read_fd, write_fd = os.pipe()
    child = os.fork()
    if child == 0:
        os.close(read_fd)
        try:
            mark_receipts_sealed(cell)
        except IsolationError:
            os.write(write_fd, b"rejected")
        else:
            os.write(write_fd, b"accepted")
        finally:
            os.close(write_fd)
            os._exit(0)
    os.close(write_fd)
    outcome = os.read(read_fd, 32)
    os.close(read_fd)
    _, status = os.waitpid(child, 0)
    assert os.waitstatus_to_exitcode(status) == 0
    assert outcome == b"rejected"
    assert not cell.receipt_marker.exists()
    _seal_and_cleanup(cell)


@pytest.mark.skipif(os.name != "nt", reason="native Windows security chain")
def test_windows_backend_completes_private_lifecycle_without_safe_failure(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    """Catches treating unsupported/fail-closed Windows creation as test success."""
    cell = _cell(tmp_path, "windows-native-round2", profile, source_environment)
    verifier = batch_isolation._windows_path_has_private_acl
    for path in (
        cell.root,
        cell.home,
        cell.workspace,
        cell.cache,
        cell.temp,
        cell.logs,
        cell.promptfoo,
    ):
        assert verifier(path)
    with pytest.raises(PermissionError):
        cell.root.rename(tmp_path / "renamed-cell")
    assert (cell.home / "config.toml").is_file()
    _seal_and_cleanup(cell)
    assert not cell.root.exists()


def test_windows_backend_declares_handle_bound_streaming_security_primitives() -> None:
    """Keeps the Windows backend reviewable even when this suite runs on POSIX."""
    backend = (Path(__file__).parent / "batch_isolation_windows.py").read_text(
        encoding="utf-8"
    )
    win32 = (Path(__file__).parent / "batch_isolation_win32.py").read_text(
        encoding="utf-8"
    )
    win32 += (Path(__file__).parent / "batch_isolation_win32_abi.py").read_text(
        encoding="utf-8"
    )
    assert "os.walk(" not in backend
    assert "os.listdir(" not in backend
    assert "SetFilePointer(" not in win32
    for primitive in (
        "SetFilePointerEx",
        "FindFirstFileExW",
        "GetSecurityInfo",
        "GetFileInformationByHandleEx",
        "SetFileInformationByHandle",
        "NtCreateFile",
    ):
        assert primitive in win32
    assert "function.argtypes = argtypes" in win32
    assert "function.restype = restype" in win32


@pytest.mark.skipif(os.name != "nt", reason="native Windows base handoff probe")
def test_windows_validated_base_handle_prevents_swap_during_handoff(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    base = tmp_path / "base"
    moved = tmp_path / "moved"
    base.mkdir()
    original = batch_isolation.reserve_attempt

    def try_swap(*args: object, **kwargs: object):
        reservation = original(*args, **kwargs)
        with pytest.raises(PermissionError):
            base.rename(moved)
        return reservation

    monkeypatch.setattr(batch_isolation, "reserve_attempt", try_swap)
    cell = _cell(base, "windows-base-handoff", profile, source_environment)
    _seal_and_cleanup(cell)


@pytest.mark.skipif(os.name != "nt", reason="native Windows rollback probe")
def test_windows_partial_create_rolls_back_handles_and_directory(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    import batch_isolation_windows  # noqa: PLC0415

    base = tmp_path / "base"
    base.mkdir()
    base_handle = batch_isolation_windows.open_directory(base)
    original = batch_isolation_windows.win32.create_directory
    calls = 0

    def fail_child(parent: int, name: str) -> int:
        nonlocal calls
        calls += 1
        if calls == 3:
            raise batch_isolation_windows.SecureFilesystemError("injected")
        return original(parent, name)

    monkeypatch.setattr(batch_isolation_windows.win32, "create_directory", fail_child)
    with pytest.raises(batch_isolation_windows.SecureFilesystemError):
        batch_isolation_windows.WindowsCellFilesystem.create(
            base, "cell-partial", base_handle
        )
    assert not (base / "cell-partial").exists()
