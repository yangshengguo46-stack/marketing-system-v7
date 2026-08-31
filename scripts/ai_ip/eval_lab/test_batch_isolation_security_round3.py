import ctypes
import gc
import hashlib
import importlib
import importlib.util
import os
import sys
import types
import weakref
from dataclasses import replace
from pathlib import Path, PureWindowsPath

import pytest


sys.path.insert(0, str(Path(__file__).resolve().parent))
import batch_isolation  # noqa: E402
import batch_isolation_posix  # noqa: E402
from batch_isolation import (  # noqa: E402
    IsolationError,
    cleanup_attempt_cell,
    create_attempt_cell,
    mark_receipts_sealed,
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
    return f"round3-{digest}"


def _cell(
    base: Path,
    label: str,
    profile: dict[str, object],
    source_environment: dict[str, str],
):
    return create_attempt_cell(
        base,
        "pair-round3",
        _attempt_id(base, label),
        FIXTURES / "stock-seed",
        FIXTURES / "workspace",
        profile,
        source_environment,
    )


def test_state_strongly_retains_only_the_exact_public_capability(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    cell = _cell(tmp_path, "strong-public-object", profile, source_environment)
    reference = weakref.ref(cell)
    replay = replace(cell)

    del cell
    gc.collect()
    original = reference()
    assert original is not None, "live state must prevent public-object ID reuse"
    with pytest.raises(IsolationError, match="public.*capability|public object"):
        mark_receipts_sealed(replay)

    mark_receipts_sealed(original)
    cleanup_attempt_cell(original)
    del original
    gc.collect()
    assert reference() is None, "proven cleanup must release the public capability"


def test_allocator_replay_cannot_replace_the_exact_public_capability(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
) -> None:
    cell = _cell(tmp_path, "allocator-replay", profile, source_environment)
    replay = replace(cell)
    state = batch_isolation._CELLS[cell._capability]
    state.binding.public_object_id = id(replay)

    with pytest.raises(IsolationError, match="public.*capability|public object"):
        mark_receipts_sealed(replay)
    mark_receipts_sealed(cell)
    cleanup_attempt_cell(cell)


@pytest.mark.skipif(os.name == "nt", reason="POSIX terminal-scan bound probe")
def test_terminal_base_scan_is_entry_bounded_and_resumable(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    cell = _cell(tmp_path, "terminal-entry-bound", profile, source_environment)
    for index in range(12):
        (tmp_path / f"sibling-{index:02d}").mkdir(mode=0o700)
    mark_receipts_sealed(cell)
    monkeypatch.setattr(batch_isolation_posix, "_MAX_TERMINAL_SCAN_ENTRIES", 3)

    with pytest.raises(IsolationError, match="bounded|progress|terminal"):
        cleanup_attempt_cell(cell)
    state = batch_isolation._CELLS[cell._capability]
    assert state.lifecycle == "cleaning"
    assert state.filesystem.terminal_unlinked is True
    assert not cell.root.exists()

    monkeypatch.setattr(batch_isolation_posix, "_MAX_TERMINAL_SCAN_ENTRIES", 100)
    cleanup_attempt_cell(cell)


@pytest.mark.skipif(os.name == "nt", reason="POSIX terminal-scan time probe")
def test_terminal_base_scan_is_time_bounded_and_resumable(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    cell = _cell(tmp_path, "terminal-time-bound", profile, source_environment)
    (tmp_path / "sibling").mkdir(mode=0o700)
    mark_receipts_sealed(cell)
    monkeypatch.setattr(batch_isolation_posix, "_MAX_TERMINAL_SCAN_SECONDS", -1.0)

    with pytest.raises(IsolationError, match="bounded|progress|terminal"):
        cleanup_attempt_cell(cell)
    assert batch_isolation._CELLS[cell._capability].filesystem.terminal_unlinked

    monkeypatch.setattr(batch_isolation_posix, "_MAX_TERMINAL_SCAN_SECONDS", 5.0)
    cleanup_attempt_cell(cell)


def test_win32_standard_information_uses_native_boolean_layout() -> None:
    policy = importlib.import_module("batch_isolation_win32_policy")
    structure = policy.StandardInformation

    assert ctypes.sizeof(structure) == 24
    assert structure.DeletePending.offset == 20
    assert structure.Directory.offset == 21
    raw = bytearray(ctypes.sizeof(structure))
    raw[20] = 1
    raw[21] = 0
    value = structure.from_buffer_copy(raw)
    assert (value.DeletePending, value.Directory) == (1, 0)


@pytest.mark.parametrize(
    "sddl,directory,valid",
    [
        ("O:S-1-5-21-7D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;S-1-5-21-7)", True, True),
        ("O:S-1-5-21-7D:P(A;;FA;;;SY)(A;;FA;;;S-1-5-21-7)", False, True),
        ("O:S-1-5-21-7D:P(A;ID;FA;;;SY)(A;ID;FA;;;S-1-5-21-7)", False, False),
        ("O:S-1-5-21-7D:P(D;;FA;;;SY)(A;;FA;;;S-1-5-21-7)", False, False),
        ("O:S-1-5-21-7D:P(A;;FR;;;SY)(A;;FA;;;S-1-5-21-7)", False, False),
        (
            "O:S-1-5-21-7D:P(A;;FA;;;SY)(A;;FA;;;S-1-5-21-7)(A;;FA;;;BA)",
            False,
            False,
        ),
        ("O:SYD:P(A;;FA;;;SY)(A;;FA;;;S-1-5-21-7)", False, False),
        ("O:S-1-5-21-7D:(A;;FA;;;SY)(A;;FA;;;S-1-5-21-7)", False, False),
    ],
)
def test_win32_private_dacl_policy_is_exact(
    sddl: str, directory: bool, valid: bool
) -> None:
    policy = importlib.import_module("batch_isolation_win32_policy")
    assert policy.is_exact_private_sddl(sddl, "S-1-5-21-7", directory) is valid


def _load_windows_backend_with_fakes(monkeypatch: pytest.MonkeyPatch):
    calls: list[tuple[str, bool]] = []
    fake_win32 = types.ModuleType("batch_isolation_win32")

    class FakeError(OSError):
        pass

    fake_win32.Win32SecurityError = FakeError
    fake_win32.open_path = lambda path, *, directory, deletable: (
        calls.append((str(path), deletable)) or len(calls)
    )
    fake_win32.open_child = lambda handle, name, *, directory, deletable: (
        calls.append((str(name), deletable)) or len(calls)
    )
    fake_win32.close = lambda handle: None
    fake_tree = types.ModuleType("batch_isolation_windows_tree")
    fake_tree.WindowsTreeError = FakeError
    fake_tree.delete_tree = lambda *args, **kwargs: None
    fake_tree.scan_regular_identities = lambda *args, **kwargs: frozenset()
    monkeypatch.setitem(sys.modules, "batch_isolation_win32", fake_win32)
    monkeypatch.setitem(sys.modules, "batch_isolation_windows_tree", fake_tree)
    path = Path(__file__).with_name("batch_isolation_windows.py")
    name = "_round3_windows_backend"
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    monkeypatch.setitem(sys.modules, name, module)
    spec.loader.exec_module(module)
    monkeypatch.setattr(module, "Path", PureWindowsPath)
    return module, calls


def test_win32_ordinary_ancestors_never_request_delete_access(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    windows, calls = _load_windows_backend_with_fakes(monkeypatch)
    windows.open_directory(PureWindowsPath("C:/authority/base"))
    assert calls
    assert all(deletable is False for _, deletable in calls)


class _JournalOperations:
    def __init__(self) -> None:
        self.pending: set[int] = set()
        self.closed: list[int] = []
        self.live = {(10, "root"): (1, 20), (20, "child"): (1, 30)}
        self.fail_mark: set[int] = {30}

    def identity(self, handle: int) -> tuple[int, int]:
        return (1, handle)

    def mark_delete(self, handle: int) -> None:
        if handle in self.fail_mark:
            raise OSError("injected disposition failure")
        self.pending.add(handle)

    def delete_pending(self, handle: int) -> bool:
        return handle in self.pending

    def close(self, handle: int) -> None:
        self.closed.append(handle)
        for key, identity in tuple(self.live.items()):
            if identity == (1, handle) and handle in self.pending:
                del self.live[key]

    def live_identity(
        self, parent: int, name: str, *, directory: bool
    ) -> tuple[int, int] | None:
        return self.live.get((parent, name))


def test_windows_construction_journal_retains_failed_rollback_for_retry() -> None:
    journal_module = importlib.import_module("batch_isolation_windows_journal")
    operations = _JournalOperations()
    journal = journal_module.WindowsConstructionJournal(operations, 10)
    journal.record(20, 10, "root", directory=True)
    journal.record(30, 20, "child", directory=True)

    with pytest.raises(journal_module.JournalRollbackError, match="rollback"):
        journal.rollback()
    assert journal.state == "orphan"
    assert 30 not in operations.closed

    operations.fail_mark.clear()
    journal.retry_cleanup()
    assert journal.state == "cleaned"
    assert operations.live == {}
    assert operations.closed == [30, 20, 10]


def test_public_construction_retains_unproven_orphan_and_reservation(
    tmp_path: Path,
    profile: dict[str, object],
    source_environment: dict[str, str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    class Orphan:
        def __init__(self) -> None:
            self.retried = 0
            self.closed = False

        def retry_cleanup(self) -> None:
            self.retried += 1
            if self.retried == 1:
                raise OSError("still pending")

        def close(self) -> None:
            self.closed = True

    orphan = Orphan()

    class FailedFilesystem:
        @classmethod
        def create(cls, base: Path, root_name: str, base_fd: int):
            error = OSError("construction rollback is unproven")
            error.orphan_resource = orphan
            raise error

    monkeypatch.setattr(batch_isolation, "CellFilesystem", FailedFilesystem)
    before = set(batch_isolation._CELLS)
    with pytest.raises(IsolationError, match="orphan|cleanup.*pending"):
        _cell(tmp_path, "public-orphan", profile, source_environment)
    assert set(batch_isolation._CELLS) == before
    assert len(batch_isolation._ORPHANS) == 1

    with pytest.raises(IsolationError, match="orphan|cleanup.*pending"):
        batch_isolation._retry_orphans()
    assert len(batch_isolation._ORPHANS) == 1
    batch_isolation._retry_orphans()
    assert batch_isolation._ORPHANS == []
    assert orphan.closed


@pytest.mark.skipif(os.name != "nt", reason="native Windows journal retry")
def test_native_windows_construction_rollback_failure_is_retryable(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    import batch_isolation_windows

    base = tmp_path / "native-journal-base"
    base.mkdir()
    base_handle = batch_isolation_windows.open_directory(base)
    original_create = batch_isolation_windows.win32.create_directory
    original_mark = batch_isolation_windows.win32.mark_delete
    calls = 0

    def fail_third_create(parent: int, name: str) -> int:
        nonlocal calls
        calls += 1
        if calls == 3:
            raise batch_isolation_windows.win32.Win32SecurityError(
                "injected construction failure"
            )
        return original_create(parent, name)

    def fail_disposition(handle: int) -> None:
        raise batch_isolation_windows.win32.Win32SecurityError(
            "injected disposition failure"
        )

    monkeypatch.setattr(
        batch_isolation_windows.win32, "create_directory", fail_third_create
    )
    monkeypatch.setattr(batch_isolation_windows.win32, "mark_delete", fail_disposition)
    with pytest.raises(
        batch_isolation_windows.SecureFilesystemError,
        match="cleanup is pending",
    ) as caught:
        batch_isolation_windows.WindowsCellFilesystem.create(
            base, "native-partial", base_handle
        )
    orphan = caught.value.orphan_resource

    monkeypatch.setattr(batch_isolation_windows.win32, "mark_delete", original_mark)
    orphan.retry_cleanup()
    assert orphan.state == "cleaned"
    assert not (base / "native-partial").exists()
