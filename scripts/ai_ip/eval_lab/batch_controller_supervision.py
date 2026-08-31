"""Owned process-group supervision and aggregate bounded stream acquisition."""

import hashlib
import os
import selectors
import signal
import subprocess
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone

try:
    from .batch_controller_capture import CapturedResult, FatalSupervisorError
    from .batch_controller_launch_materialization import expand_argument
    from .batch_controller_launch_record import build_launch_record
    from .batch_controller_process_capture import captured_result
except ImportError:
    from batch_controller_capture import CapturedResult, FatalSupervisorError
    from batch_controller_launch_materialization import expand_argument
    from batch_controller_launch_record import build_launch_record
    from batch_controller_process_capture import captured_result


_CHUNK_BYTES = 64 * 1024
_MAX_DRAIN_CHUNKS = 128
_STOP_SECONDS = 1.0
_DRAIN_SECONDS = 0.25


def _now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


@dataclass
class AggregateBudget:
    limit: int
    acquired: int = 0
    exhausted: bool = False

    @property
    def remaining(self) -> int:
        return max(0, self.limit - self.acquired)

    def consume(self, size: int) -> None:
        if size < 0 or size > self.remaining:
            raise FatalSupervisorError("aggregate acquisition budget was bypassed")
        self.acquired += size
        if self.acquired >= self.limit:
            self.exhausted = True


@dataclass
class StreamBuffer:
    payload: bytearray = field(default_factory=bytearray)
    digest: object = field(default_factory=hashlib.sha256)
    size: int = 0
    truncated: bool = False
    raw_complete: bool = True

    def add(self, chunk: bytes, *, store: bool) -> None:
        self.digest.update(chunk)
        self.size += len(chunk)
        if store:
            self.payload.extend(chunk)
        else:
            self.truncated = True

    def evidence(self) -> dict[str, object]:
        stored = bytes(self.payload)
        return {
            "rawComplete": self.raw_complete,
            "rawSha256": self.digest.hexdigest(),
            "rawSize": self.size,
            "storedSha256": hashlib.sha256(stored).hexdigest(),
            "storedSize": len(stored),
            "truncated": self.truncated,
        }


@dataclass
class OwnedProcess:
    process: subprocess.Popen
    prepared: object
    read_descriptors: dict[str, int]
    started_at: str
    started_monotonic: float
    deadline: float
    group_id: int
    launch_record: dict[str, object]
    buffers: dict[str, StreamBuffer]
    budget: AggregateBudget
    timed_out: bool = False
    stopped: bool = False
    drain_deadline: float | None = None
    drain_chunks: int = 0


def _group_alive(group_id: int) -> bool:
    try:
        os.killpg(group_id, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def _stopped(process: subprocess.Popen, group_id: int) -> bool:
    return process.poll() is not None and not _group_alive(group_id)


def _wait_stopped(process: subprocess.Popen, group_id: int, deadline: float) -> bool:
    while time.monotonic() < deadline:
        if _stopped(process, group_id):
            return True
        time.sleep(0.005)
    return _stopped(process, group_id)


def _signal_group(group_id: int, group_signal: int) -> None:
    try:
        os.killpg(group_id, group_signal)
    except ProcessLookupError:
        pass


def _terminate_process(
    process: subprocess.Popen, group_id: int, deadline: float
) -> bool:
    if _group_alive(group_id):
        _signal_group(group_id, signal.SIGTERM)
    remaining = max(0.0, deadline - time.monotonic())
    term_deadline = min(deadline, time.monotonic() + min(0.1, remaining / 2))
    if _wait_stopped(process, group_id, term_deadline):
        return True
    if _group_alive(group_id):
        _signal_group(group_id, signal.SIGKILL)
    return _wait_stopped(process, group_id, deadline)


def terminate_and_wait(owned: OwnedProcess, deadline: float) -> bool:
    """Terminate, kill, and positively observe both leader and process group."""
    owned.stopped = _terminate_process(owned.process, owned.group_id, deadline)
    return owned.stopped


def _close_descriptor(descriptor: int) -> None:
    try:
        os.close(descriptor)
    except OSError:
        pass


def close_owned(owned: OwnedProcess) -> None:
    for name, descriptor in list(owned.read_descriptors.items()):
        _close_read_descriptor(owned, name, descriptor)
    owned.read_descriptors.clear()
    owned.prepared.close()


def _close_process_stream(process: subprocess.Popen, name: str) -> int | None:
    stream = getattr(process, name, None)
    if stream is None:
        return None
    try:
        descriptor = stream.fileno()
    except (OSError, ValueError):
        descriptor = None
    try:
        stream.close()
    except OSError:
        pass
    return descriptor


def _close_read_descriptor(owned: OwnedProcess, name: str, descriptor: int) -> None:
    if name in {"stdout", "stderr"}:
        _close_process_stream(owned.process, name)
    else:
        _close_descriptor(descriptor)
    owned.read_descriptors.pop(name, None)


class ProcessOwnershipGuard:
    """Own every launch FD before Popen and the process at Popen assignment."""

    def __init__(self, prepared: object, descriptors: set[int]) -> None:
        self.prepared = prepared
        self.descriptors = descriptors
        self.process: subprocess.Popen | None = None
        self.group_id: int | None = None
        self.transferred = False

    def start(self, popen: object, argv: list[str], **kwargs) -> subprocess.Popen:
        self.process = popen(argv, **kwargs)
        self.group_id = self.process.pid
        return self.process

    def close_descriptor(self, descriptor: int) -> None:
        os.close(descriptor)
        self.descriptors.discard(descriptor)

    def transfer(self) -> None:
        self.transferred = True
        self.descriptors.clear()

    def abort(self, error: BaseException) -> None:
        confirmed = True
        if self.process is not None and self.group_id is not None:
            confirmed = _terminate_process(
                self.process, self.group_id, time.monotonic() + _STOP_SECONDS
            )
            for name in ("stdout", "stderr"):
                descriptor = _close_process_stream(self.process, name)
                if descriptor is not None:
                    self.descriptors.discard(descriptor)
        for descriptor in tuple(self.descriptors):
            _close_descriptor(descriptor)
        self.descriptors.clear()
        self.prepared.close()
        if not confirmed:
            raise FatalSupervisorError(
                "fatal supervisor orphan: post-launch stop was not confirmed"
            ) from error


def _pipes() -> tuple[int, int]:
    read_descriptor, write_descriptor = os.pipe()
    os.set_blocking(read_descriptor, False)
    return read_descriptor, write_descriptor


def _pipe_pair() -> tuple[int, int, int, int]:
    result_read = result_write = telemetry_read = telemetry_write = -1
    try:
        result_read, result_write = _pipes()
        telemetry_read, telemetry_write = _pipes()
        return result_read, result_write, telemetry_read, telemetry_write
    except BaseException:
        for descriptor in (
            result_read,
            result_write,
            telemetry_read,
            telemetry_write,
        ):
            if descriptor >= 0:
                _close_descriptor(descriptor)
        raise


def spawn(
    prepared: object,
    limit: int,
    lifecycle: object,
    *,
    popen: object,
    owned_type: type,
    launch_spec_identity_fn: object,
    sha256_json_fn: object,
) -> OwnedProcess:
    """Launch under a preexisting guard and atomically transfer lifecycle ownership."""
    prepared.verify_handoff()
    started_at = _now()
    started_monotonic = time.monotonic()
    executable = prepared.executable_handoff_path
    argv = [executable] + [
        expand_argument(argument, prepared) for argument in prepared.spec.argv
    ]
    environment = dict(prepared.request.cell.environment)
    environment.update(dict(prepared.spec.environment))
    environment.update(
        {
            "AI_IP_ATTEMPT_ID": prepared.request.cell.attempt_id,
            "AI_IP_CASE_PATH": prepared.child_path("case"),
            "AI_IP_CODEX_PATH": prepared.child_path("codex"),
            "AI_IP_CONFIG_PATH": prepared.child_path("config"),
            "AI_IP_PROFILE_PATH": prepared.child_path("profile"),
            "AI_IP_ROUTE_PATH": prepared.child_path("route"),
            "AI_IP_PROTOCOL_PATH": prepared.child_path("protocol"),
            "AI_IP_PROMPTFOO_PATH": prepared.child_path("promptfoo"),
            "AI_IP_SCHEMA_PATH": prepared.child_path("schema"),
        }
    )
    result_read, result_write, telemetry_read, telemetry_write = _pipe_pair()
    guard = ProcessOwnershipGuard(
        prepared,
        {result_read, result_write, telemetry_read, telemetry_write},
    )
    try:
        environment.update(
            {
                "AI_IP_RESULT_FD": str(result_write),
                "AI_IP_TELEMETRY_FD": str(telemetry_write),
            }
        )
        pass_fds = prepared.pass_fds + (result_write, telemetry_write)
        process = guard.start(
            popen,
            argv,
            executable=executable,
            cwd=prepared.request.cell.workspace,
            env=environment,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            pass_fds=pass_fds,
            start_new_session=True,
        )
        guard.close_descriptor(result_write)
        guard.close_descriptor(telemetry_write)
        if process.stdout is None or process.stderr is None:
            raise RuntimeError("candidate process streams are unavailable")
        stdout = process.stdout.fileno()
        stderr = process.stderr.fileno()
        guard.descriptors.update((stdout, stderr))
        os.set_blocking(stdout, False)
        os.set_blocking(stderr, False)
        try:
            process_group = os.getpgid(process.pid)
        except ProcessLookupError:
            process_group = process.pid
        if process_group != process.pid:
            raise RuntimeError("candidate process did not enter its owned session")
        prepared.verify_handoff()
        record = build_launch_record(
            prepared,
            argv=argv,
            environment=environment,
            pid=process.pid,
            started_at=started_at,
            launch_spec_identity_fn=launch_spec_identity_fn,
            sha256_json_fn=sha256_json_fn,
        )
        owned = owned_type(
            process,
            prepared,
            {
                "result": result_read,
                "telemetry": telemetry_read,
                "stdout": stdout,
                "stderr": stderr,
            },
            started_at,
            started_monotonic,
            started_monotonic + prepared.request.timeout_seconds,
            process.pid,
            record,
            {
                name: StreamBuffer()
                for name in ("result", "telemetry", "stdout", "stderr")
            },
            AggregateBudget(limit),
        )
        lifecycle.bind_process(owned)
        guard.transfer()
        return owned
    except BaseException as error:
        guard.abort(error)
        raise


def _descriptor_active(owned: OwnedProcess, owners: dict[int, object]) -> bool:
    return any(item is owned for item, _ in owners.values())


def _active(owned: OwnedProcess, owners: dict[int, object]) -> bool:
    return (
        owned.process.poll() is None
        or _group_alive(owned.group_id)
        or _descriptor_active(owned, owners)
    )


def _begin_terminal_drain(
    owned: OwnedProcess, *, timed_out: bool, stop_process: object
) -> None:
    if owned.drain_deadline is not None:
        return
    owned.timed_out = owned.timed_out or timed_out
    if not stop_process(owned, time.monotonic() + _STOP_SECONDS):
        raise FatalSupervisorError("fatal supervisor orphan: stop was not confirmed")
    owned.drain_deadline = time.monotonic() + _DRAIN_SECONDS


def supervise_pair(
    owned: dict[str, OwnedProcess], stop_process: object
) -> dict[str, CapturedResult]:
    """Apply each absolute deadline until leader, group, and streams are terminal."""
    selector = selectors.DefaultSelector()
    descriptor_owner: dict[int, tuple[OwnedProcess, str]] = {}
    for attempt in owned.values():
        for name, descriptor in attempt.read_descriptors.items():
            selector.register(descriptor, selectors.EVENT_READ)
            descriptor_owner[descriptor] = (attempt, name)
    try:
        while any(_active(item, descriptor_owner) for item in owned.values()):
            now = time.monotonic()
            for item in owned.values():
                if now >= item.deadline and _active(item, descriptor_owner):
                    _begin_terminal_drain(
                        item, timed_out=True, stop_process=stop_process
                    )
                if item.budget.exhausted:
                    _begin_terminal_drain(
                        item, timed_out=False, stop_process=stop_process
                    )
                if (
                    item.drain_deadline is not None
                    and now >= item.drain_deadline
                    and _descriptor_active(item, descriptor_owner)
                ):
                    for descriptor, (owner, _) in list(descriptor_owner.items()):
                        if owner is item:
                            selector.unregister(descriptor)
                            _, name = descriptor_owner[descriptor]
                            _close_read_descriptor(item, name, descriptor)
                            descriptor_owner.pop(descriptor)
                    for buffer in item.buffers.values():
                        buffer.raw_complete = False
            deadlines = [
                item.deadline
                for item in owned.values()
                if item.drain_deadline is None and _active(item, descriptor_owner)
            ] + [
                item.drain_deadline
                for item in owned.values()
                if item.drain_deadline is not None
                and _descriptor_active(item, descriptor_owner)
            ]
            timeout = min(0.05, max(0.0, min(deadlines) - now)) if deadlines else 0.0
            for key, _ in selector.select(timeout):
                descriptor = key.fd
                item, name = descriptor_owner[descriptor]
                draining = item.drain_deadline is not None
                maximum = (
                    _CHUNK_BYTES
                    if draining
                    else min(_CHUNK_BYTES, item.budget.remaining)
                )
                if maximum < 1:
                    _begin_terminal_drain(
                        item, timed_out=False, stop_process=stop_process
                    )
                    continue
                try:
                    chunk = os.read(descriptor, maximum)
                except BlockingIOError:
                    continue
                if not chunk:
                    selector.unregister(descriptor)
                    _close_read_descriptor(item, name, descriptor)
                    descriptor_owner.pop(descriptor)
                    continue
                if draining:
                    item.drain_chunks += 1
                    item.buffers[name].add(chunk, store=False)
                    if item.drain_chunks >= _MAX_DRAIN_CHUNKS:
                        item.drain_deadline = time.monotonic()
                else:
                    item.budget.consume(len(chunk))
                    item.buffers[name].add(chunk, store=True)
                    if item.budget.exhausted:
                        item.buffers[name].truncated = True
                        _begin_terminal_drain(
                            item, timed_out=False, stop_process=stop_process
                        )
            for item in owned.values():
                if _stopped(item.process, item.group_id):
                    item.stopped = True
        return {name: captured_result(item, _now()) for name, item in owned.items()}
    finally:
        selector.close()
        for descriptor, (item, name) in list(descriptor_owner.items()):
            _close_read_descriptor(item, name, descriptor)
        for item in owned.values():
            item.read_descriptors.clear()
            item.prepared.close()
