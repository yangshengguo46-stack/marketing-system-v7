import hashlib
import importlib
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace

import pytest


MODULE_ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(MODULE_ROOT))


def _archive():
    try:
        return importlib.import_module("batch_promptfoo_archive")
    except ImportError as error:
        pytest.fail(f"descriptor-bound Promptfoo archive module is missing: {error}")


def _runtime(root: Path) -> Path:
    runtime = root / "runtime"
    entrypoint = runtime / "node_modules/promptfoo/dist/src/entrypoint.js"
    entrypoint.parent.mkdir(parents=True)
    entrypoint.write_bytes(b'console.log("0.122.0")\n')
    (entrypoint.parents[2] / "package.json").write_bytes(
        json.dumps(
            {
                "bin": {"promptfoo": "dist/src/entrypoint.js"},
                "name": "promptfoo",
                "version": "0.122.0",
            },
            sort_keys=True,
            separators=(",", ":"),
        ).encode()
    )
    return runtime


def _canonical(path: Path) -> Path:
    return Path(os.path.realpath(path))


def test_runtime_archive_rejects_retained_ancestor_replacement(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Catches reopening descendants after an absolute ancestor is replaced."""
    archive = _archive()
    container = tmp_path / "container"
    outside = tmp_path / "outside"
    detached = tmp_path / "detached"
    container.mkdir()
    outside.mkdir()
    runtime = _runtime(container)
    _runtime(outside)
    scandir = archive.os.scandir
    swapped = False

    def scan_then_swap(descriptor: int):
        nonlocal swapped
        entries = list(scandir(descriptor))
        if not swapped:
            swapped = True
            container.rename(detached)
            container.symlink_to(outside, target_is_directory=True)
        return iter(entries)

    monkeypatch.setattr(archive.os, "scandir", scan_then_swap)
    with pytest.raises(archive.PromptfooFilesystemError, match="changed|link"):
        archive.build_runtime_archive(_canonical(runtime))


def test_runtime_archive_rejects_queued_subdirectory_swap(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Catches a queued child changed to a symlink before descriptor open."""
    archive = _archive()
    runtime = _runtime(tmp_path)
    nested = runtime / "queued"
    outside = tmp_path / "outside"
    detached = tmp_path / "detached"
    nested.mkdir()
    outside.mkdir()
    (nested / "safe").write_bytes(b"safe")
    (outside / "outside").write_bytes(b"outside")
    scandir = archive.os.scandir
    swapped = False

    def scan_then_swap(descriptor: int):
        nonlocal swapped
        entries = list(scandir(descriptor))
        if not swapped:
            swapped = True
            nested.rename(detached)
            nested.symlink_to(outside, target_is_directory=True)
        return iter(entries)

    monkeypatch.setattr(archive.os, "scandir", scan_then_swap)
    with pytest.raises(archive.PromptfooFilesystemError, match="changed|link"):
        archive.build_runtime_archive(_canonical(runtime))


def test_runtime_archive_rejects_hardlinked_regular_file(tmp_path: Path) -> None:
    """Catches accepting two archive names backed by one mutable inode."""
    archive = _archive()
    runtime = _runtime(tmp_path)
    source = runtime / "source"
    source.write_bytes(b"same inode")
    os.link(source, runtime / "alias")

    with pytest.raises(archive.PromptfooFilesystemError, match="hard link"):
        archive.build_runtime_archive(_canonical(runtime))


def test_bin_file_link_is_ignored_but_bin_directory_link_is_rejected(
    tmp_path: Path,
) -> None:
    """Catches following deployment .bin leaves or accepting linked directories."""
    archive = _archive()
    runtime = _runtime(tmp_path)
    bin_dir = runtime / "node_modules/.bin"
    bin_dir.mkdir()
    outside_file = tmp_path / "outside-file"
    outside_file.write_bytes(b"must not be archived")
    (bin_dir / "tool").symlink_to(outside_file)

    built = archive.build_runtime_archive(_canonical(runtime))
    assert built.file_count == 2

    outside_dir = tmp_path / "outside-dir"
    outside_dir.mkdir()
    (bin_dir / "linked-dir").symlink_to(outside_dir, target_is_directory=True)
    with pytest.raises(archive.PromptfooFilesystemError, match="directory|link"):
        archive.build_runtime_archive(_canonical(runtime))


@pytest.mark.parametrize("kind", ["fifo", "socket"])
def test_runtime_archive_rejects_special_entries(tmp_path: Path, kind: str) -> None:
    """Catches special entries entering a runtime authority seal."""
    archive = _archive()
    short_root: Path | None = None
    if kind == "socket":
        short_root = Path(tempfile.mkdtemp(prefix="07b-pf-", dir="/tmp"))
    runtime = _runtime(short_root or tmp_path)
    unsafe = runtime / kind
    if kind == "fifo":
        os.mkfifo(unsafe)
    else:
        endpoint = socket.socket(socket.AF_UNIX)
        endpoint.bind(str(unsafe))
        endpoint.close()

    try:
        with pytest.raises(archive.PromptfooFilesystemError, match="regular|special"):
            archive.build_runtime_archive(_canonical(runtime))
    finally:
        if short_root is not None:
            shutil.rmtree(short_root)


def test_control_regular_to_fifo_race_is_nonblocking(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Catches a checked regular leaf swapped to a blocking FIFO before openat."""
    archive = _archive()
    control = _canonical(tmp_path) / "control.bin"
    control.write_bytes(b"safe")
    open_file = archive.os.open
    observed_flags = 0

    def swap_before_open(name: object, flags: int, *args: object, **kwargs: object):
        nonlocal observed_flags
        if name == control.name and kwargs.get("dir_fd") is not None:
            observed_flags = flags
            control.unlink()
            os.mkfifo(control)
        return open_file(name, flags, *args, **kwargs)

    monkeypatch.setattr(archive.os, "open", swap_before_open)
    with pytest.raises(archive.PromptfooFilesystemError, match="regular|changed"):
        archive.read_bounded_regular(control, 64, "control")
    assert observed_flags & os.O_NONBLOCK


@pytest.mark.parametrize("mutation", ["bytes", "mode"])
def test_runtime_archive_detects_same_descriptor_content_or_mode_race(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, mutation: str
) -> None:
    """Catches header, mode, and payload being derived from different file states."""
    archive = _archive()
    runtime = _runtime(tmp_path)
    target = runtime / "race"
    target.write_bytes(b"first")
    target_inode = target.stat().st_ino
    pread = archive.os.pread
    mutated = False

    def mutate_after_first_pass(fd: int, amount: int, offset: int) -> bytes:
        nonlocal mutated
        payload = pread(fd, amount, offset)
        if os.fstat(fd).st_ino == target_inode and offset == 5 and not mutated:
            mutated = True
            if mutation == "bytes":
                target.write_bytes(b"other")
            else:
                target.chmod(0o700)
        return payload

    monkeypatch.setattr(archive.os, "pread", mutate_after_first_pass)
    monkeypatch.setattr(Path, "stat", lambda self: pytest.fail("Path.stat escaped"))
    with pytest.raises(archive.PromptfooFilesystemError, match="changed|digest"):
        archive.build_runtime_archive(_canonical(runtime))


@pytest.mark.parametrize("kind", ["symlink", "hardlink", "fifo"])
def test_bounded_control_reader_rejects_unsafe_leaf(tmp_path: Path, kind: str) -> None:
    """Catches control inputs escaping the nofollow/single-link boundary."""
    archive = _archive()
    safe = _canonical(tmp_path) / "safe"
    unsafe = safe.parent / "unsafe"
    safe.write_bytes(b"safe")
    if kind == "symlink":
        unsafe.symlink_to(safe)
    elif kind == "hardlink":
        os.link(safe, unsafe)
    else:
        os.mkfifo(unsafe)

    with pytest.raises(archive.PromptfooFilesystemError, match="regular|link"):
        archive.read_bounded_regular(unsafe, 64, "control")


def test_bounded_control_reader_rejects_sparse_oversize_before_read(
    tmp_path: Path,
) -> None:
    """Catches package/lock/manifest/source inputs allocating beyond their cap."""
    archive = _archive()
    oversized = _canonical(tmp_path) / "oversized"
    with oversized.open("wb") as stream:
        stream.truncate(65)

    with pytest.raises(archive.PromptfooFilesystemError, match="bound"):
        archive.read_bounded_regular(oversized, 64, "control")


def test_measure_and_compile_have_no_path_read_bytes_escape(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Catches Task 5 package/lock/Node/JS control reads reverting to Path.read_bytes."""
    archive = _archive()
    bundle = importlib.import_module("batch_promptfoo_bundle")
    adapter = importlib.import_module("batch_promptfoo")
    runtime = _runtime(tmp_path)
    node = _canonical(tmp_path) / "node"
    node.write_bytes(b"portable node")
    monkeypatch.setattr(Path, "read_bytes", lambda self: pytest.fail(f"read_bytes: {self}"))

    seal = bundle.measure_promptfoo_runtime(_canonical(runtime), node)
    sealed = bundle.seal_promptfoo_runtime(_canonical(runtime), node, seal)
    monkeypatch.setattr(bundle, "seal_committed_promptfoo_runtime", lambda *_: sealed)
    request = SimpleNamespace(
        app_server_protocol_schema=b'{"version":2}',
        case_answer_schema_json=b'{"type":"object"}',
        effective_config=b"config",
        execution_profile_json=b'{"approvalPolicy":"never","maxWallClockSeconds":30,"sandboxMode":"workspace-write"}',
        model_route_json=b'{"baseUrl":"http://127.0.0.1:1/v1","model":"local"}',
        promptfoo_config=b"",
    )
    request.promptfoo_config = json.dumps(
        adapter.render_promptfoo_config(request), sort_keys=True, separators=(",", ":")
    ).encode()

    launches = adapter.compile_promptfoo_launch_set(
        request,
        request,
        promptfoo_runtime_root=_canonical(runtime),
        portable_node_path=node,
    )
    assert len(launches.stock.artifacts) == len(sealed.artifacts) + 2
    assert archive.read_bounded_regular(node, 64, "node") == b"portable node"


@pytest.mark.parametrize("name", ["bad\\name", "bad\x01name"])
def test_runtime_archive_rejects_noncanonical_names(tmp_path: Path, name: str) -> None:
    """Catches producer/consumer disagreement over backslash and control names."""
    archive = _archive()
    runtime = _runtime(tmp_path)
    (runtime / name).write_bytes(b"unsafe")

    with pytest.raises(archive.PromptfooFilesystemError, match="path|name"):
        archive.build_runtime_archive(_canonical(runtime))


def test_runtime_archive_accepts_strict_utf8_and_preserves_record_format(
    tmp_path: Path,
) -> None:
    """Catches changing the legacy record bytes while adding strict UTF-8 support."""
    archive = _archive()
    runtime = _runtime(tmp_path)
    (runtime / "礼.txt").write_bytes("礼".encode())

    built = archive.build_runtime_archive(_canonical(runtime))

    assert built.file_count == 3
    assert len(built.tree_sha256) == 64
    assert hashlib.sha256(b"".join(built.chunks)).hexdigest()


def test_missing_descriptor_primitive_fails_before_fallback(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Catches an unsupported host falling back to pathname reads or os.walk."""
    archive = _archive()
    control = _canonical(tmp_path) / "control"
    control.write_bytes(b"safe")
    monkeypatch.setattr(archive.os, "pread", None)
    monkeypatch.setattr(archive.os, "walk", lambda *_: pytest.fail("os.walk fallback"))
    monkeypatch.setattr(Path, "read_bytes", lambda self: pytest.fail("read_bytes fallback"))

    with pytest.raises(archive.PromptfooFilesystemError, match="descriptor"):
        archive.read_bounded_regular(control, 64, "control")


def test_all_descriptors_close_once_when_root_scan_fails(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Catches capability descriptors leaking or being double-closed on exceptions."""
    archive = _archive()
    runtime = _runtime(tmp_path)
    opened: list[int] = []
    closed: list[int] = []
    open_file = archive.os.open
    close_file = archive.os.close

    def record_open(*args: object, **kwargs: object) -> int:
        descriptor = open_file(*args, **kwargs)
        opened.append(descriptor)
        return descriptor

    def record_close(descriptor: int) -> None:
        closed.append(descriptor)
        close_file(descriptor)

    monkeypatch.setattr(archive.os, "open", record_open)
    monkeypatch.setattr(archive.os, "close", record_close)
    monkeypatch.setattr(archive.os, "scandir", lambda *_: (_ for _ in ()).throw(OSError("boom")))

    with pytest.raises(archive.PromptfooFilesystemError, match="unavailable|changed"):
        archive.build_runtime_archive(_canonical(runtime))
    assert sorted(opened) == sorted(closed)
    assert len(closed) == len(set(closed))


def test_write_all_retries_eintr_and_partial_writes_and_rejects_zero() -> None:
    """Catches valid controller frames being truncated by one writeSync call."""
    source = r"""
const runner = require(process.argv[1]);
const calls = [];
let attempt = 0;
runner.writeAll(9, Buffer.from("abcdef"), (fd, payload, offset, length) => {
  calls.push([fd, offset, length]);
  attempt += 1;
  if (attempt === 1) return 2;
  if (attempt === 2) { const error = new Error("interrupt"); error.code = "EINTR"; throw error; }
  return length;
});
let zero = "accepted";
try { runner.writeAll(9, Buffer.from("x"), () => 0); } catch (error) { zero = error.message; }
process.stdout.write(JSON.stringify({calls, zero}));
"""
    completed = subprocess.run(
        [
            shutil.which("node") or "node",
            "-e",
            source,
            str(MODULE_ROOT / "batch_promptfoo_runner.js"),
        ],
        capture_output=True,
        check=False,
    )

    assert completed.returncode == 0, completed.stderr.decode(errors="replace")
    assert json.loads(completed.stdout) == {
        "calls": [[9, 0, 6], [9, 2, 4], [9, 2, 4]],
        "zero": "pipe write made no progress",
    }
