import ctypes
import hashlib
import importlib.util
import os
import sys
import types
from dataclasses import dataclass
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
        "environmentAllowlist": ["LANG"],
        "maxWallClockSeconds": 300,
        "maxOutputBytes": 1_048_576,
    }


def _cell(base: Path, label: str, profile: dict[str, object]):
    digest = hashlib.sha256(f"{base}:{label}".encode()).hexdigest()[:20]
    return create_attempt_cell(
        base,
        "pair-round4",
        f"round4-{digest}",
        FIXTURES / "stock-seed",
        FIXTURES / "workspace",
        profile,
        {"PATH": "/safe/bin", "SHELL": "/bin/sh", "LANG": "en_US.UTF-8"},
    )


def _cleanup_until_complete(cell, *, limit: int = 32) -> int:
    bounded_failures = 0
    for _ in range(limit):
        try:
            cleanup_attempt_cell(cell)
        except IsolationError as error:
            assert "bounded" in str(error) or "changed" in str(error)
            bounded_failures += 1
        else:
            return bounded_failures
    pytest.fail("terminal cleanup did not make bounded cross-retry progress")


@pytest.mark.skipif(os.name == "nt", reason="POSIX terminal cursor probe")
def test_terminal_scan_completes_with_the_same_small_limit_across_retries(
    tmp_path: Path,
    profile: dict[str, object],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    cell = _cell(tmp_path, "same-limit-progress", profile)
    for index in range(11):
        (tmp_path / f"sibling-{index:02d}").mkdir(mode=0o700)
    mark_receipts_sealed(cell)
    monkeypatch.setattr(batch_isolation_posix, "_MAX_TERMINAL_SCAN_ENTRIES", 3)

    bounded_failures = _cleanup_until_complete(cell)

    assert bounded_failures >= 3
    assert not cell.root.exists()


@pytest.mark.skipif(os.name == "nt", reason="POSIX terminal cursor probe")
def test_terminal_scan_restarts_if_the_base_changes_between_retries(
    tmp_path: Path,
    profile: dict[str, object],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    cell = _cell(tmp_path, "stable-base-restart", profile)
    for index in range(7):
        (tmp_path / f"before-{index:02d}").mkdir(mode=0o700)
    mark_receipts_sealed(cell)
    monkeypatch.setattr(batch_isolation_posix, "_MAX_TERMINAL_SCAN_ENTRIES", 2)

    with pytest.raises(IsolationError, match="bounded"):
        cleanup_attempt_cell(cell)
    (tmp_path / "mutation-after-partial-scan").mkdir(mode=0o700)
    with pytest.raises(IsolationError, match="changed|stable|restart"):
        cleanup_attempt_cell(cell)

    assert _cleanup_until_complete(cell) >= 3
    assert not cell.root.exists()


class _FakeFunction:
    def __init__(self) -> None:
        self.restype = None
        self.argtypes = None

    def __call__(self, *args):
        return 1


class _FakeDll:
    def __init__(self) -> None:
        self._functions: dict[str, _FakeFunction] = {}

    def __getattr__(self, name: str) -> _FakeFunction:
        return self._functions.setdefault(name, _FakeFunction())


def _load_win32_abi(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setattr(ctypes, "WinDLL", lambda *args, **kwargs: _FakeDll(), raising=False)
    path = Path(__file__).with_name("batch_isolation_win32_abi.py")
    spec = importlib.util.spec_from_file_location("_round4_win32_abi", path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_win32_disposition_uses_native_one_byte_boolean(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    abi = _load_win32_abi(monkeypatch)

    assert ctypes.sizeof(abi.Disposition) == 1
    assert abi.Disposition.DeleteFile.offset == 0
    assert abi.Disposition(1).DeleteFile == 1


def test_win32_delete_api_receives_one_byte_disposition(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    abi = _load_win32_abi(monkeypatch)
    calls: list[tuple[int, int]] = []
    sid_text = ctypes.c_wchar_p("S-1-5-21-7")

    def open_process_token(process, access, output) -> int:
        ctypes.cast(output, ctypes.POINTER(abi.wintypes.HANDLE)).contents.value = 91
        return 1

    def get_token_information(token, kind, buffer, length, needed) -> int:
        ctypes.cast(needed, ctypes.POINTER(abi.wintypes.DWORD)).contents.value = (
            ctypes.sizeof(abi.SidAndAttributes)
        )
        if not buffer:
            return 0
        value = abi.SidAndAttributes.from_buffer(buffer)
        value.Sid = 1234
        return 1

    def convert_sid(sid, output) -> int:
        ctypes.cast(output, ctypes.POINTER(abi.wintypes.LPWSTR))[0] = sid_text
        return 1

    def set_information(handle, kind, value, size) -> int:
        disposition = ctypes.cast(value, ctypes.POINTER(abi.Disposition)).contents
        calls.append((disposition.DeleteFile, size))
        return 1

    def get_information(handle, kind, value, size) -> int:
        standard = ctypes.cast(
            value, ctypes.POINTER(abi.StandardInformation)
        ).contents
        standard.DeletePending = 1
        return 1

    abi.kernel32.GetCurrentProcess = lambda: 1
    abi.kernel32.CloseHandle = lambda handle: 1
    abi.kernel32.LocalFree = lambda value: None
    abi.kernel32.SetFileInformationByHandle = set_information
    abi.kernel32.GetFileInformationByHandleEx = get_information
    abi.advapi32.OpenProcessToken = open_process_token
    abi.advapi32.GetTokenInformation = get_token_information
    abi.advapi32.ConvertSidToStringSidW = convert_sid
    monkeypatch.setitem(sys.modules, "batch_isolation_win32_abi", abi)
    path = Path(__file__).with_name("batch_isolation_win32.py")
    spec = importlib.util.spec_from_file_location("_round4_win32", path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)

    module.mark_delete(77)

    assert calls == [(1, 1)]


@dataclass(frozen=True)
class _Entry:
    name: str
    is_directory: bool
    size: int = 0
    is_reparse: bool = False


class _FakeWin32Error(OSError):
    pass


class _WindowsWorld:
    def __init__(self, kind: str) -> None:
        self.kind = kind
        self.root_present = True
        self.child_present = True
        self.root_removal_enabled = True
        self.first_close_failed = False
        self.next_handle = 200
        self.identities = {1: (7, 1), 10: (7, 10), 20: (7, 20)}
        self.pending: set[int] = set()
        self.unsafe_mutations: list[int] = []
        self._reused: set[int] = set()

    def allocate(self, identity: tuple[int, int]) -> int:
        self.next_handle += 1
        self.identities[self.next_handle] = identity
        return self.next_handle

    def identity(self, handle: int) -> tuple[int, int]:
        if handle not in self.identities:
            raise _FakeWin32Error("invalid handle")
        return self.identities[handle]

    def iter_directory(self, handle: int):
        identity = self.identity(handle)
        if identity == (7, 10) and self.child_present:
            yield _Entry("home" if self.kind == "retained" else "victim", self.kind == "retained")

    def open_path(self, path, *, directory: bool, **kwargs) -> int:
        name = path[-1] if isinstance(path, tuple) else str(path)
        if name == "root":
            if not self.root_present:
                error = _FakeWin32Error("missing root")
                error.winerror = 2
                raise error
            return self.allocate((7, 10))
        if not self.child_present:
            error = _FakeWin32Error("missing child")
            error.winerror = 2
            raise error
        return self.allocate((7, 20 if self.kind == "retained" else 30))

    def open_child(self, parent: int, name: str, *, directory: bool, **kwargs) -> int:
        return self.open_path((parent, name), directory=directory, **kwargs)

    def child_path(self, parent: int, name: str):
        return (parent, name)

    def mark_delete(self, handle: int) -> None:
        if handle in self._reused:
            self.unsafe_mutations.append(handle)
            raise _FakeWin32Error("mutated a reused handle")
        self.pending.add(handle)

    def delete_pending(self, handle: int) -> bool:
        return handle in self.pending

    def close(self, handle: int) -> None:
        if handle in self._reused:
            self.unsafe_mutations.append(handle)
            raise _FakeWin32Error("closed a reused handle")
        identity = self.identity(handle)
        target = 20 if self.kind == "retained" else 30
        if identity == (7, target) and handle in self.pending and not self.first_close_failed:
            self.first_close_failed = True
            self.identities[handle] = (99, 99)
            self._reused.add(handle)
            raise _FakeWin32Error("injected uncertain close")
        if handle in self.pending:
            if identity == (7, target):
                self.child_present = False
            elif identity == (7, 10) and self.root_removal_enabled:
                self.root_present = False
        self.identities.pop(handle, None)


class _FakeWindowsFilesystem:
    def __init__(self, world: _WindowsWorld) -> None:
        self.world = world
        self.base_fd = 1
        self.root_fd = 10
        self.base_identity = (7, 1)
        self.root_identity = (7, 10)
        self.root_name = "root"
        self.directories = {"home": 20} if world.kind == "retained" else {}
        self.marker_handle = None
        self.cleanup_started = False

    def validate(self, *, cleanup: bool = False) -> None:
        return None

    def _open_relative(self, parts: tuple[str, ...], *, deletable: bool = False) -> int:
        if not parts:
            return self.world.allocate((7, 10))
        return self.world.allocate((7, 20))


def _load_windows_tree(monkeypatch: pytest.MonkeyPatch, world: _WindowsWorld):
    fake = types.ModuleType("batch_isolation_win32")
    fake.Win32SecurityError = _FakeWin32Error
    fake.iter_directory = world.iter_directory
    fake.open_path = world.open_path
    fake.open_child = world.open_child
    fake.child_path = world.child_path
    fake.identity = world.identity
    fake.mark_delete = world.mark_delete
    fake.delete_pending = world.delete_pending
    fake.close = world.close
    monkeypatch.setitem(sys.modules, "batch_isolation_win32", fake)
    monkeypatch.delitem(sys.modules, "batch_isolation_windows_cleanup", raising=False)
    path = Path(__file__).with_name("batch_isolation_windows_tree.py")
    name = f"_round4_windows_tree_{world.kind}"
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    monkeypatch.setitem(sys.modules, name, module)
    spec.loader.exec_module(module)
    return module


@pytest.mark.parametrize("kind", ["dynamic", "retained"])
def test_windows_cleanup_retains_close_uncertainty_and_safely_retries(
    kind: str, monkeypatch: pytest.MonkeyPatch
) -> None:
    world = _WindowsWorld(kind)
    tree = _load_windows_tree(monkeypatch, world)
    filesystem = _FakeWindowsFilesystem(world)

    with pytest.raises(tree.WindowsTreeError, match="close|cleanup|uncertain"):
        tree.delete_tree(filesystem)
    journal = filesystem.cleanup_journal
    assert journal.creator_pid == os.getpid()
    assert journal.pending

    tree.delete_tree(filesystem)

    assert world.unsafe_mutations == []
    assert not world.child_present
    assert not world.root_present
    assert filesystem.root_fd == -1
    assert filesystem.base_fd == -1


def test_windows_cleanup_does_not_succeed_until_root_entry_is_proven_absent(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    world = _WindowsWorld("dynamic")
    world.child_present = False
    world.first_close_failed = True
    world.root_removal_enabled = False
    tree = _load_windows_tree(monkeypatch, world)
    filesystem = _FakeWindowsFilesystem(world)

    with pytest.raises(tree.WindowsTreeError, match="root|removal|absent|proven"):
        tree.delete_tree(filesystem)
    assert world.root_present
    assert filesystem.cleanup_journal.creator_pid == os.getpid()

    world.root_removal_enabled = True
    tree.delete_tree(filesystem)
    assert not world.root_present


def test_windows_cleanup_retains_transient_handle_validation_uncertainty(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    world = _WindowsWorld("dynamic")
    tree = _load_windows_tree(monkeypatch, world)
    filesystem = _FakeWindowsFilesystem(world)
    with pytest.raises(tree.WindowsTreeError):
        tree.delete_tree(filesystem)
    item = filesystem.cleanup_journal.pending[0]
    world._reused.remove(item.handle)
    world.identities[item.handle] = item.identity
    original_identity = world.identity
    failed = False

    def fail_query_once(handle: int) -> tuple[int, int]:
        nonlocal failed
        if handle == item.handle and not failed:
            failed = True
            error = _FakeWin32Error("transient identity query failure")
            error.winerror = 5
            raise error
        return original_identity(handle)

    tree.win32.identity = fail_query_once
    with pytest.raises(tree.WindowsTreeError, match="uncertain|identity|cleanup"):
        tree.delete_tree(filesystem)
    assert filesystem.cleanup_journal.pending == [item]

    tree.win32.identity = original_identity
    tree.delete_tree(filesystem)
    assert world.unsafe_mutations == []


def test_public_construction_retains_filesystem_after_cleanup_uncertainty(
    tmp_path: Path,
    profile: dict[str, object],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    instances = []

    class UncertainFilesystem:
        def __init__(self, base_fd: int) -> None:
            self.base_fd = base_fd
            self.root_fd = base_fd
            self.root_identity = (1, 2)
            self.retries = 0
            self.closed = False

        @classmethod
        def create(cls, base: Path, root_name: str, base_fd: int):
            instance = cls(base_fd)
            instances.append(instance)
            return instance

        def write_snapshot(self, files, target: str) -> None:
            raise OSError("stop after construction")

        def delete_exact(self) -> None:
            raise OSError("uncertain retained handle close")

        def retry_cleanup(self) -> None:
            self.retries += 1
            if self.retries == 1:
                raise OSError("still uncertain")

        def close(self) -> None:
            self.closed = True

    monkeypatch.setattr(batch_isolation, "CellFilesystem", UncertainFilesystem)
    with pytest.raises(IsolationError, match="orphan|cleanup.*pending"):
        _cell(tmp_path, "public-windows-close-orphan", profile)
    assert len(batch_isolation._ORPHANS) == 1

    with pytest.raises(IsolationError, match="orphan|cleanup.*pending"):
        batch_isolation._retry_orphans()
    batch_isolation._retry_orphans()
    assert instances[0].closed
    assert batch_isolation._ORPHANS == []
