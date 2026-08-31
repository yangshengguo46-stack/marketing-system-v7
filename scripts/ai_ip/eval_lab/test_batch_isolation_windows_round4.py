import hashlib
import os
import sys
from pathlib import Path

import pytest


sys.path.insert(0, str(Path(__file__).resolve().parent))
import batch_isolation  # noqa: E402
from batch_isolation import (  # noqa: E402
    IsolationError,
    cleanup_attempt_cell,
    create_attempt_cell,
    mark_receipts_sealed,
)


REPO_ROOT = Path(__file__).resolve().parents[3]
FIXTURES = REPO_ROOT / "ai-ip-evals" / "lab" / "fixtures" / "batch-runner"


def _profile() -> dict[str, object]:
    return {
        "schemaVersion": 1,
        "sandboxMode": "workspace-write",
        "approvalPolicy": "never",
        "networkMode": "offline",
        "networkAllowlist": [],
        "writablePaths": ["workspace"],
        "externalTools": [],
        "environmentAllowlist": ["LANG"],
        "maxWallClockSeconds": 300,
        "maxOutputBytes": 1_048_576,
    }


def _cell(base: Path, label: str):
    attempt = hashlib.sha256(f"{base}:{label}".encode()).hexdigest()[:20]
    return create_attempt_cell(
        base,
        "pair-native-round4",
        f"native4-{attempt}",
        FIXTURES / "stock-seed",
        FIXTURES / "workspace",
        _profile(),
        {"PATH": "C:\\Windows\\System32", "LANG": "en_US.UTF-8"},
    )


@pytest.mark.skipif(os.name != "nt", reason="native Windows cleanup retry")
@pytest.mark.parametrize("target", ["dynamic", "retained"])
def test_native_windows_close_uncertainty_retries_without_handle_reuse(
    tmp_path: Path, target: str, monkeypatch: pytest.MonkeyPatch
) -> None:
    import batch_isolation_windows

    cell = _cell(tmp_path, f"close-{target}")
    state = batch_isolation._CELLS[cell._capability]
    if target == "dynamic":
        candidate = cell.workspace / "dynamic.txt"
        candidate.write_text("dynamic", encoding="utf-8")
        handle = batch_isolation_windows.win32.open_path(
            candidate, directory=False, deletable=False
        )
        try:
            target_identity = batch_isolation_windows.win32.identity(handle)
        finally:
            batch_isolation_windows.win32.close(handle)
    else:
        target_identity = state.filesystem.directory_identities["home"]
    original_close = batch_isolation_windows.win32.close
    failed = False

    def fail_once(handle: int) -> None:
        nonlocal failed
        try:
            matches = batch_isolation_windows.win32.identity(
                handle
            ) == target_identity and batch_isolation_windows.win32.delete_pending(
                handle
            )
        except batch_isolation_windows.win32.Win32SecurityError:
            matches = False
        if matches and not failed:
            failed = True
            raise batch_isolation_windows.win32.Win32SecurityError(
                "injected close uncertainty"
            )
        original_close(handle)

    monkeypatch.setattr(batch_isolation_windows.win32, "close", fail_once)
    mark_receipts_sealed(cell)
    with pytest.raises(IsolationError, match="cleanup|uncertain|close"):
        cleanup_attempt_cell(cell)
    assert batch_isolation._CELLS[cell._capability].filesystem.cleanup_journal.pending

    monkeypatch.setattr(batch_isolation_windows.win32, "close", original_close)
    cleanup_attempt_cell(cell)
    assert not cell.root.exists()


@pytest.mark.skipif(os.name != "nt", reason="native Windows root absence proof")
def test_native_windows_success_proves_the_exact_root_absent(
    tmp_path: Path,
) -> None:
    cell = _cell(tmp_path, "root-absence")
    mark_receipts_sealed(cell)
    cleanup_attempt_cell(cell)
    assert not cell.root.exists()


@pytest.mark.skipif(os.name != "nt", reason="native Windows public orphan retry")
def test_native_public_rollback_retains_close_uncertainty(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    import batch_isolation_windows

    original_close = batch_isolation_windows.win32.close
    failed = False

    def fail_pending_once(handle: int) -> None:
        nonlocal failed
        try:
            pending = batch_isolation_windows.win32.delete_pending(handle)
        except batch_isolation_windows.win32.Win32SecurityError:
            pending = False
        if pending and not failed:
            failed = True
            raise batch_isolation_windows.win32.Win32SecurityError(
                "injected rollback close uncertainty"
            )
        original_close(handle)

    monkeypatch.setattr(
        batch_isolation,
        "_write_snapshot",
        lambda *args: (_ for _ in ()).throw(OSError("stop after construction")),
    )
    monkeypatch.setattr(batch_isolation_windows.win32, "close", fail_pending_once)
    with pytest.raises(IsolationError, match="orphan|cleanup.*pending"):
        _cell(tmp_path, "public-orphan")
    assert batch_isolation._ORPHANS
    orphan_root = (
        batch_isolation._ORPHANS[-1].resource.base_path
        / batch_isolation._ORPHANS[-1].resource.root_name
    )

    monkeypatch.setattr(batch_isolation_windows.win32, "close", original_close)
    batch_isolation._retry_orphans()
    assert not orphan_root.exists()
