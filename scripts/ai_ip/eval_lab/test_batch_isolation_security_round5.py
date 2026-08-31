import importlib.util
import ast
import os
import sys
import types
from pathlib import Path

import pytest


class _FakeWin32Error(OSError):
    pass


class _FakeFilesystem:
    def __init__(self) -> None:
        self.base_fd = 1
        self.root_fd = 10
        self.root_name = "root"
        self.root_identity = (7, 10)
        self.marker_handle = None
        self.directories: dict[str, int] = {}


class _CleanupWorld:
    def __init__(self) -> None:
        self.identities = {1: (7, 1), 10: (7, 10)}
        self.opened: list[int] = []
        self.next_open: list[int] = []
        self.pending: set[int] = set()
        self.fail_identity_for: set[int] = set()
        self.fail_close_for: set[int] = set()
        self.reused: set[int] = set()
        self.unsafe_mutations: list[int] = []
        self.observed: list[list[tuple[int | None, object, str | None]]] = []
        self.journal = None

    def duplicate(self, handle: int) -> int:
        duplicate = handle + 1_000
        self.identities[duplicate] = self.identities[handle]
        return duplicate

    def identity(self, handle: int) -> tuple[int, int]:
        source = handle - 1_000 if handle >= 1_000 else handle
        if source in self.fail_identity_for:
            self.observed.append(
                [
                    (
                        item.handle,
                        item.identity,
                        getattr(item, "state", None),
                    )
                    for item in self.journal.pending
                ]
            )
            error = _FakeWin32Error("injected transient identity query failure")
            error.winerror = 5
            raise error
        if handle not in self.identities:
            error = _FakeWin32Error("invalid handle")
            error.winerror = 6
            raise error
        return self.identities[handle]

    def open_child(self, parent: int, name: str, *, directory: bool, **kwargs) -> int:
        del parent, name, directory, kwargs
        handle = self.next_open.pop(0)
        self.opened.append(handle)
        self.identities[handle] = (7, 10)
        return handle

    def mark_delete(self, handle: int) -> None:
        if handle in self.reused:
            self.unsafe_mutations.append(handle)
            raise _FakeWin32Error("deleted through a reused handle value")
        self.pending.add(handle)

    def delete_pending(self, handle: int) -> bool:
        return handle in self.pending

    def close(self, handle: int) -> None:
        if handle in self.reused:
            self.unsafe_mutations.append(handle)
            raise _FakeWin32Error("closed through a reused handle value")
        if handle in self.fail_close_for:
            self.fail_close_for.remove(handle)
            self.identities[handle] = (99, 99)
            raise _FakeWin32Error("injected uncertain close")
        self.identities.pop(handle, None)


def _load_cleanup(monkeypatch: pytest.MonkeyPatch, world: _CleanupWorld):
    fake = types.ModuleType("batch_isolation_win32")
    fake.Win32SecurityError = _FakeWin32Error
    fake.duplicate = world.duplicate
    fake.identity = world.identity
    fake.open_child = world.open_child
    fake.mark_delete = world.mark_delete
    fake.delete_pending = world.delete_pending
    fake.close = world.close
    monkeypatch.setitem(sys.modules, "batch_isolation_win32", fake)
    path = Path(__file__).with_name("batch_isolation_windows_cleanup.py")
    name = f"_round5_windows_cleanup_{id(world)}"
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    monkeypatch.setitem(sys.modules, name, module)
    spec.loader.exec_module(module)
    return module


def _assert_unknown_owned(
    observed: list[list[tuple[int | None, object, str | None]]], handle: int
) -> None:
    assert observed
    assert observed[-1] == [(handle, None, "identity_unknown")]


@pytest.mark.parametrize("action", ["close", "dispose"])
def test_cleanup_records_existing_handle_before_identity_query(
    action: str, monkeypatch: pytest.MonkeyPatch
) -> None:
    world = _CleanupWorld()
    world.identities[40] = (7, 40)
    world.fail_identity_for.add(40)
    cleanup = _load_cleanup(monkeypatch, world)
    filesystem = _FakeFilesystem()
    journal = cleanup.WindowsCleanupJournal()
    world.journal = journal

    with pytest.raises(OSError, match="identity|uncertain|query"):
        getattr(journal, action)(filesystem, 40)

    _assert_unknown_owned(world.observed, 40)
    assert journal.creator_pid == os.getpid()
    assert journal.pending[0].handle == 40


def test_cleanup_unknown_identity_retry_never_acts_on_reused_raw_value(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    world = _CleanupWorld()
    world.identities[40] = (7, 40)
    world.fail_identity_for.add(40)
    cleanup = _load_cleanup(monkeypatch, world)
    filesystem = _FakeFilesystem()
    journal = cleanup.WindowsCleanupJournal()
    world.journal = journal
    with pytest.raises(OSError, match="identity|uncertain|query"):
        journal.dispose(filesystem, 40)
    assert journal.pending
    assert journal.pending[0].proof_handle == 1_040
    world.fail_identity_for.clear()
    world.identities[40] = (99, 99)
    world.reused.add(40)

    journal.retry(filesystem)

    assert world.unsafe_mutations == []
    assert journal.pending == []


def test_cleanup_records_reopened_root_before_identity_query(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    world = _CleanupWorld()
    world.identities[40] = (7, 10)
    cleanup = _load_cleanup(monkeypatch, world)
    filesystem = _FakeFilesystem()
    journal = cleanup.WindowsCleanupJournal()
    world.journal = journal
    world.fail_close_for.add(40)
    with pytest.raises(OSError, match="close|uncertain"):
        journal.dispose(filesystem, 40, slot="root")
    world.next_open.append(41)
    world.fail_identity_for.add(41)

    with pytest.raises(OSError, match="identity|uncertain|query"):
        journal.retry(filesystem)

    _assert_unknown_owned(world.observed, 41)
    assert journal.pending[0].handle == 41


def test_root_absence_records_first_probe_before_identity_query(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    world = _CleanupWorld()
    world.next_open.append(51)
    world.fail_identity_for.add(51)
    cleanup = _load_cleanup(monkeypatch, world)
    filesystem = _FakeFilesystem()
    journal = cleanup.WindowsCleanupJournal()
    world.journal = journal

    with pytest.raises(OSError, match="identity|uncertain|query"):
        journal.prove_root_absent(filesystem)

    _assert_unknown_owned(world.observed, 51)
    assert journal.pending[0].handle == 51


def test_root_absence_records_final_proof_before_identity_query(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    world = _CleanupWorld()
    world.next_open.extend([61, 62])
    world.fail_identity_for.add(62)
    cleanup = _load_cleanup(monkeypatch, world)
    filesystem = _FakeFilesystem()
    journal = cleanup.WindowsCleanupJournal()
    world.journal = journal

    with pytest.raises(OSError, match="identity|uncertain|query"):
        journal.prove_root_absent(filesystem)

    _assert_unknown_owned(world.observed, 62)
    assert journal.pending[0].handle == 62


class _ConstructionOperations:
    def __init__(self) -> None:
        self.identities = {10: (7, 10), 20: (7, 20)}
        self.pending: set[int] = set()
        self.observed: list[list[tuple[int | None, object, str | None]]] = []
        self.journal = None

    def duplicate(self, handle: int) -> int:
        duplicate = handle + 1_000
        self.identities[duplicate] = self.identities[handle]
        return duplicate

    def identity(self, handle: int) -> tuple[int, int]:
        source = handle - 1_000 if handle >= 1_000 else handle
        if source == 30:
            self.observed.append(
                [
                    (
                        item.handle,
                        item.identity,
                        getattr(item, "state", None),
                    )
                    for item in getattr(self.journal, "probes", [])
                ]
            )
            error = OSError("injected live-probe identity failure")
            error.winerror = 5
            raise error
        return self.identities[handle]

    def mark_delete(self, handle: int) -> None:
        self.pending.add(handle)

    def delete_pending(self, handle: int) -> bool:
        return handle in self.pending

    def close(self, handle: int) -> None:
        self.identities.pop(handle, None)

    def open_live(self, parent: int, name: str, *, directory: bool) -> int:
        del parent, name, directory
        self.identities[30] = (7, 20)
        return 30

    def live_identity(
        self, parent: int, name: str, *, directory: bool
    ) -> tuple[int, int] | None:
        handle = self.open_live(parent, name, directory=directory)
        try:
            return self.identity(handle)
        finally:
            self.close(handle)


def test_construction_records_live_probe_before_identity_query() -> None:
    from batch_isolation_windows_journal import WindowsConstructionJournal

    operations = _ConstructionOperations()
    journal = WindowsConstructionJournal(operations, 10)
    operations.journal = journal
    journal.record(20, 10, "root", directory=True)

    with pytest.raises(OSError, match="rollback|live-probe|unproven"):
        journal.rollback()

    _assert_unknown_owned(operations.observed, 30)
    assert journal.creator_pid == os.getpid()
    assert journal.probes[0].handle == 30


class _LedgerOperations:
    def __init__(self) -> None:
        self.identities = {40: (7, 40)}
        self.observed = []
        self.closed = []
        self.ledger = None

    def duplicate(self, handle: int) -> int:
        duplicate = handle + 1_000
        self.identities[duplicate] = self.identities[handle]
        return duplicate

    def identity(self, handle: int) -> tuple[int, int]:
        source = handle - 1_000 if handle >= 1_000 else handle
        if source == 40 and not self.observed:
            self.observed.append(
                [
                    (record.handle, record.identity, record.state)
                    for record in self.ledger.records
                ]
            )
        return self.identities[handle]

    def close(self, handle: int) -> None:
        self.closed.append(handle)
        self.identities.pop(handle, None)


def test_handle_ledger_registers_raw_and_duplicate_before_identity_query() -> None:
    from batch_isolation_windows_handle_ledger import WindowsHandleLedger

    operations = _LedgerOperations()
    ledger = WindowsHandleLedger(operations)
    operations.ledger = ledger

    ledger.acquire(40)

    assert operations.observed == [[(40, None, "identity_unknown")]]
    assert ledger.records[0].identity == (7, 40)
    assert ledger.records[0].proof_handle is None


def _called_attributes(function: ast.FunctionDef) -> list[str]:
    return [
        node.func.attr
        for node in ast.walk(function)
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
    ]


@pytest.mark.parametrize("function_name", ["open_path", "duplicate"])
def test_win32_public_acquisitions_route_through_the_handle_ledger(
    function_name: str,
) -> None:
    path = Path(__file__).with_name("batch_isolation_win32.py")
    tree = ast.parse(path.read_text(encoding="utf-8"))
    function = next(
        node
        for node in tree.body
        if isinstance(node, ast.FunctionDef) and node.name == function_name
    )

    assert "acquire" in _called_attributes(function)


def test_win32_duplicate_queries_source_before_acquiring_new_raw_value() -> None:
    path = Path(__file__).with_name("batch_isolation_win32.py")
    tree = ast.parse(path.read_text(encoding="utf-8"))
    function = next(
        node
        for node in tree.body
        if isinstance(node, ast.FunctionDef) and node.name == "duplicate"
    )
    positions = {
        node.func.id: (node.lineno, node.col_offset)
        for node in ast.walk(function)
        if isinstance(node, ast.Call)
        and isinstance(node.func, ast.Name)
        and node.func.id in {"identity", "duplicate_raw"}
    }

    assert positions["identity"] < positions["duplicate_raw"]


class _RotationWin32Error(OSError):
    pass


def _load_windows_for_rotation(monkeypatch: pytest.MonkeyPatch):
    closes = []
    fake = types.ModuleType("batch_isolation_win32")
    fake.Win32SecurityError = _RotationWin32Error
    fake.open_path = lambda *args, **kwargs: 1
    fake.open_child = lambda *args, **kwargs: 2
    fake.duplicate = lambda handle: 1

    def close(handle: int) -> None:
        closes.append(handle)
        if handle == 1 and closes.count(1) == 1:
            raise _RotationWin32Error("injected old-handle close uncertainty")

    fake.close = close
    fake.identity = lambda handle: (7, handle)
    fake.final_path = lambda handle: Path("C:/base")
    fake.require_private_acl = lambda handle: None
    monkeypatch.setitem(sys.modules, "batch_isolation_win32", fake)
    path = Path(__file__).with_name("batch_isolation_windows.py")
    name = f"_round5_windows_rotation_{id(closes)}"
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    monkeypatch.setitem(sys.modules, name, module)
    spec.loader.exec_module(module)
    return module, closes


def test_open_directory_closes_new_child_when_old_close_is_uncertain(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    windows, closes = _load_windows_for_rotation(monkeypatch)

    with pytest.raises(windows.SecureFilesystemError, match="close"):
        windows.open_directory(Path("/base/child"))

    assert 2 in closes


def test_open_relative_closes_new_child_when_old_close_is_uncertain(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    windows, closes = _load_windows_for_rotation(monkeypatch)
    filesystem = object.__new__(windows.WindowsCellFilesystem)
    filesystem.root_fd = 10
    filesystem.directories = {}

    with pytest.raises(windows.SecureFilesystemError, match="close"):
        filesystem._open_relative(("child",))

    assert 2 in closes
