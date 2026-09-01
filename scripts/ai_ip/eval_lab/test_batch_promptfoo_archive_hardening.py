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


def test_ignored_bin_links_are_never_followed(monkeypatch: pytest.MonkeyPatch) -> None:
    """Catches ignored deployment links consulting any target type or location."""
    archive = _archive()
    short_root = Path(tempfile.mkdtemp(prefix="07b-pf-", dir="/tmp"))
    endpoint = socket.socket(socket.AF_UNIX)
    try:
        runtime = _runtime(short_root)
        bin_dir = runtime / "node_modules/.bin"
        bin_dir.mkdir()
        outside_file = short_root / "outside-file"
        outside_fifo = short_root / "outside-fifo"
        outside_socket = short_root / "outside-socket"
        outside_file.write_bytes(b"must not be archived")
        os.mkfifo(outside_fifo)
        endpoint.bind(str(outside_socket))
        (bin_dir / "dangling").symlink_to(short_root / "missing")
        (bin_dir / "external").symlink_to(outside_file)
        (bin_dir / "fifo").symlink_to(outside_fifo)
        (bin_dir / "socket").symlink_to(outside_socket)
        stat_entry = archive.os.stat

        def reject_target_stat(*args: object, **kwargs: object):
            if kwargs.get("dir_fd") is not None and kwargs.get("follow_symlinks") is True:
                raise AssertionError("ignored .bin target was followed")
            return stat_entry(*args, **kwargs)

        monkeypatch.setattr(archive.os, "stat", reject_target_stat)

        built = archive.build_runtime_archive(_canonical(runtime))

        assert built.file_count == 2
    finally:
        endpoint.close()
        shutil.rmtree(short_root)


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


@pytest.mark.parametrize("failed_open", [0, 1], ids=["root", "child"])
def test_new_directory_descriptor_is_owned_before_fstat(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, failed_open: int
) -> None:
    """Catches root or child capabilities leaking when immediate fstat fails."""
    archive = _archive()
    runtime = _runtime(tmp_path)
    opened: list[int] = []
    closed: list[int] = []
    open_file = archive.os.open
    close_file = archive.os.close
    fstat_file = archive.os.fstat

    def record_open(*args: object, **kwargs: object) -> int:
        descriptor = open_file(*args, **kwargs)
        opened.append(descriptor)
        return descriptor

    def fail_selected_fstat(descriptor: int):
        if len(opened) > failed_open and descriptor == opened[failed_open]:
            raise OSError("injected immediate fstat failure")
        return fstat_file(descriptor)

    def record_close(descriptor: int) -> None:
        closed.append(descriptor)
        close_file(descriptor)

    monkeypatch.setattr(archive.os, "open", record_open)
    monkeypatch.setattr(archive.os, "fstat", fail_selected_fstat)
    monkeypatch.setattr(archive.os, "close", record_close)

    with pytest.raises(archive.PromptfooFilesystemError, match="capability|changed"):
        archive.build_runtime_archive(_canonical(runtime))

    assert sorted(opened) == sorted(closed)
    assert len(closed) == len(set(closed))


def test_regular_descriptor_close_failure_is_not_swallowed(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Catches an fstat error hiding uncertainty from closing the acquired file."""
    archive = _archive()
    control = _canonical(tmp_path) / "control"
    control.write_bytes(b"safe")
    control_fd = -1
    open_file = archive.os.open
    close_file = archive.os.close
    fstat_file = archive.os.fstat

    def record_open(name: object, flags: int, *args: object, **kwargs: object) -> int:
        nonlocal control_fd
        descriptor = open_file(name, flags, *args, **kwargs)
        if name == control.name and kwargs.get("dir_fd") is not None:
            control_fd = descriptor
        return descriptor

    def fail_control_fstat(descriptor: int):
        if descriptor == control_fd:
            raise OSError("injected regular-file fstat failure")
        return fstat_file(descriptor)

    def fail_control_close(descriptor: int) -> None:
        close_file(descriptor)
        if descriptor == control_fd:
            raise OSError("injected regular-file close failure")

    monkeypatch.setattr(archive.os, "open", record_open)
    monkeypatch.setattr(archive.os, "fstat", fail_control_fstat)
    monkeypatch.setattr(archive.os, "close", fail_control_close)

    with pytest.raises(archive.PromptfooFilesystemError, match="descriptor close failed"):
        archive.read_bounded_regular(control, 64, "control")


def test_descriptor_stack_reports_first_close_error_after_closing_all(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Catches later close failures replacing the first uncertain release."""
    archive = _archive()
    runtime = _runtime(tmp_path)
    owner = archive._DirectoryCapability(_canonical(runtime))
    owner.__enter__()
    expected_attempts = list(reversed(owner.descriptors))
    attempts: list[int] = []
    close_file = archive.os.close

    def fail_close(descriptor: int) -> None:
        attempts.append(descriptor)
        close_file(descriptor)
        raise OSError(f"close {descriptor}")

    monkeypatch.setattr(archive.os, "close", fail_close)

    with pytest.raises(archive.PromptfooFilesystemError) as raised:
        owner.__exit__(None, None, None)

    assert attempts == expected_attempts
    assert str(raised.value.__cause__) == f"close {expected_attempts[0]}"


_EXTRACTOR_PROBE = r"""
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const runnerPath = process.argv[1];
const root = process.argv[2];
const scenario = process.argv[3];
const runnerSource = fs.readFileSync(runnerPath, "utf8");
const loaded = {exports: {}};
new Function("require", "module", "exports", "__filename", "__dirname",
  `${runnerSource}\nmodule.exports.ArchiveReader = RecordExtractor;`
)(require, loaded, loaded.exports, runnerPath, path.dirname(runnerPath));
const payload = Buffer.from("abcdef");
const header = Buffer.from(JSON.stringify({
  mode: 384,
  path: "file.txt",
  sha256: crypto.createHash("sha256").update(payload).digest("hex"),
  size: payload.length,
}));
const length = Buffer.alloc(4);
length.writeUInt32BE(header.length);
const terminator = Buffer.alloc(4);
const record = Buffer.concat([length, header, payload, terminator]);
const manifest = {
  fileCount: 1,
  maximumFileBytes: 1024,
  maximumUnpackedBytes: 1024,
  treeSha256: crypto.createHash("sha256").update(record).digest("hex"),
  unpackedBytes: payload.length,
};
const originalWrite = fs.writeSync;
const originalFstat = fs.fstatSync;
let attempts = 0;
let targetFd = -1;
fs.fstatSync = (fd) => {
  const state = originalFstat(fd);
  if (scenario === "type" && fd === targetFd) state.isFile = () => false;
  return state;
};
fs.writeSync = (fd, buffer, offset = 0, requested = buffer.length - offset, position = null) => {
  attempts += 1;
  targetFd = fd;
  if (scenario === "partial") {
    if (attempts === 2) { const error = new Error("interrupt"); error.code = "EINTR"; throw error; }
    return originalWrite(fd, buffer, offset, Math.min(attempts === 1 ? 2 : requested, requested), position);
  }
  let written;
  if (scenario === "corrupt") {
    const changed = Buffer.from(buffer.subarray(offset, offset + requested));
    changed[0] ^= 0xff;
    written = originalWrite(fd, changed, 0, changed.length, position);
  } else {
    written = originalWrite(fd, buffer, offset, requested, position);
  }
  if (scenario === "size") originalWrite(fd, Buffer.from("!"), 0, 1, position + requested);
  if (scenario === "link") fs.linkSync(path.join(root, "file.txt"), path.join(root, "alias.txt"));
  if (scenario === "special-mode") fs.fchmodSync(fd, 0o4600);
  return written;
};
let error = null;
try {
  const reader = new loaded.exports.ArchiveReader(root, manifest);
  reader.consume(record);
  reader.finish();
} catch (caught) {
  error = caught.message;
} finally {
  fs.writeSync = originalWrite;
  fs.fstatSync = originalFstat;
}
const extracted = fs.readFileSync(path.join(root, "file.txt")).toString("hex");
process.stdout.write(JSON.stringify({attempts, error, extracted}));
"""


def _extractor_probe(tmp_path: Path, scenario: str) -> dict[str, object]:
    completed = subprocess.run(
        [
            shutil.which("node") or "node",
            "-e",
            _EXTRACTOR_PROBE,
            str(MODULE_ROOT / "batch_promptfoo_runner.js"),
            str(tmp_path),
            scenario,
        ],
        capture_output=True,
        check=False,
    )
    assert completed.returncode == 0, completed.stderr.decode(errors="replace")
    return json.loads(completed.stdout)


def test_extractor_retries_partial_write_and_eintr(tmp_path: Path) -> None:
    """Catches an archive payload being hashed despite only a short write."""
    assert _extractor_probe(tmp_path, "partial") == {
        "attempts": 3,
        "error": None,
        "extracted": b"abcdef".hex(),
    }


def test_extractor_rejects_bytes_changed_by_the_writer(tmp_path: Path) -> None:
    """Catches trusting source bytes instead of rereading the output descriptor."""
    observed = _extractor_probe(tmp_path, "corrupt")

    assert observed["attempts"] == 1
    assert "digest" in str(observed["error"])


@pytest.mark.parametrize(
    "scenario", ["type", "size", "link", "special-mode"]
)
def test_extractor_rejects_each_invalid_final_file_invariant(
    tmp_path: Path, scenario: str
) -> None:
    """Catches removing any final type, size, link, or complete-mode check."""
    observed = _extractor_probe(tmp_path, scenario)

    assert observed["attempts"] == 1
    assert "differs" in str(observed["error"])


def test_write_all_at_rejects_zero_progress() -> None:
    """Catches an archive writer accepting a successful zero-byte write."""
    source = r"""
const runner = require(process.argv[1]);
let error = null;
try { runner.writeAllAt(9, Buffer.from("x"), 0, () => 0); }
catch (caught) { error = caught.message; }
process.stdout.write(JSON.stringify(error));
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
    assert json.loads(completed.stdout) == "runtime file write made no progress"


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
