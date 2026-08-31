"""Descriptor-retained materialization for every candidate launch input."""

import hashlib
import os
import stat
import sys
from dataclasses import dataclass
from pathlib import Path

try:
    from .batch_controller_identity import measure_request_identity
    from .batch_launch_spec import FrozenLaunchSpec
except ImportError:
    from batch_controller_identity import measure_request_identity
    from batch_launch_spec import FrozenLaunchSpec


_CHUNK_BYTES = 64 * 1024
_MAX_INPUT_BYTES = 128 * 1024 * 1024


class LaunchMaterializationError(RuntimeError):
    pass


def _read_descriptor(descriptor: int, size: int) -> bytes:
    if size < 0 or size > _MAX_INPUT_BYTES:
        raise LaunchMaterializationError("launch input exceeds the byte bound")
    chunks: list[bytes] = []
    offset = 0
    while offset < size:
        chunk = os.pread(descriptor, min(_CHUNK_BYTES, size - offset), offset)
        if not chunk:
            raise LaunchMaterializationError("launch input ended before its bound")
        chunks.append(chunk)
        offset += len(chunk)
    if os.pread(descriptor, 1, offset):
        raise LaunchMaterializationError("launch input exceeds its stable size")
    return b"".join(chunks)


def _identity(state: os.stat_result) -> tuple[int, int, int, int, int]:
    return (
        state.st_dev,
        state.st_ino,
        stat.S_IMODE(state.st_mode),
        state.st_size,
        state.st_mtime_ns,
    )


@dataclass
class BoundLaunchInput:
    """One retained regular-file descriptor and its measured immutable bytes."""

    name: str
    path: Path
    descriptor: int
    identity: tuple[int, int, int, int, int]
    sha256: str
    closed: bool = False

    @property
    def child_path(self) -> str:
        if not Path("/dev/fd").is_dir():
            raise LaunchMaterializationError(
                "platform has no inherited descriptor path mechanism"
            )
        return f"/dev/fd/{self.descriptor}"

    def verify(self, *, pathname: bool = False) -> bytes:
        if self.closed:
            raise LaunchMaterializationError("launch input descriptor is closed")
        before = os.fstat(self.descriptor)
        if not stat.S_ISREG(before.st_mode) or _identity(before) != self.identity:
            raise LaunchMaterializationError("launch input descriptor identity changed")
        payload = _read_descriptor(self.descriptor, before.st_size)
        after = os.fstat(self.descriptor)
        if _identity(after) != self.identity:
            raise LaunchMaterializationError("launch input changed while measured")
        if hashlib.sha256(payload).hexdigest() != self.sha256:
            raise LaunchMaterializationError("launch input digest changed")
        if pathname:
            try:
                path_state = self.path.lstat()
            except OSError as error:
                raise LaunchMaterializationError(
                    "launch executable pathname became unavailable"
                ) from error
            if _identity(path_state) != self.identity or stat.S_ISLNK(
                path_state.st_mode
            ):
                raise LaunchMaterializationError(
                    "launch executable pathname identity changed"
                )
        return payload

    def evidence(self) -> dict[str, object]:
        payload = self.verify()
        device, inode, mode, size, _ = self.identity
        return {
            "childPath": self.child_path,
            "device": device,
            "inode": inode,
            "mode": mode,
            "sha256": hashlib.sha256(payload).hexdigest(),
            "size": size,
        }

    def close(self) -> None:
        if not self.closed:
            os.close(self.descriptor)
            self.closed = True


def _open_bound(name: str, path: Path, expected: str) -> BoundLaunchInput:
    descriptor = -1
    try:
        before = path.lstat()
        if (
            not stat.S_ISREG(before.st_mode)
            or stat.S_ISLNK(before.st_mode)
            or before.st_size > _MAX_INPUT_BYTES
        ):
            raise LaunchMaterializationError("launch input is not a bounded regular file")
        descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
        opened = os.fstat(descriptor)
        if (opened.st_dev, opened.st_ino) != (before.st_dev, before.st_ino):
            raise LaunchMaterializationError("launch input was replaced while opened")
        payload = _read_descriptor(descriptor, opened.st_size)
        after = os.fstat(descriptor)
        digest = hashlib.sha256(payload).hexdigest()
        if _identity(opened) != _identity(after) or digest != expected:
            raise LaunchMaterializationError("launch input identity differs")
        return BoundLaunchInput(
            name,
            path,
            descriptor,
            _identity(opened),
            digest,
        )
    except OSError as error:
        raise LaunchMaterializationError("launch input could not be opened") from error
    except BaseException:
        if descriptor >= 0:
            os.close(descriptor)
        raise


def _materialize(
    name: str, path: Path, payload: bytes, mode: int
) -> BoundLaunchInput:
    writer = -1
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0)
    try:
        path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        writer = os.open(path, flags, mode)
        view = memoryview(payload)
        while view:
            written = os.write(writer, view)
            if written < 1:
                raise LaunchMaterializationError(
                    "launch artifact write made no progress"
                )
            view = view[written:]
        os.fchmod(writer, mode)
        state = os.fstat(writer)
        if not stat.S_ISREG(state.st_mode) or state.st_size != len(payload):
            raise LaunchMaterializationError("launch artifact was not written exactly")
    except OSError as error:
        raise LaunchMaterializationError(
            "launch artifact could not be materialized"
        ) from error
    finally:
        if writer >= 0:
            os.close(writer)
    return _open_bound(name, path, hashlib.sha256(payload).hexdigest())


@dataclass
class PreparedProcess:
    request: object
    spec: FrozenLaunchSpec
    request_identity: dict[str, str]
    inputs: dict[str, BoundLaunchInput]

    @property
    def launcher_path(self) -> Path:
        return self.inputs["executable"].path

    @property
    def artifact_paths(self) -> dict[str, Path]:
        return {
            name: item.path
            for name, item in self.inputs.items()
            if name not in {"codex", "executable"}
        }

    @property
    def written_sha256(self) -> dict[str, str]:
        return {
            name: item.sha256
            for name, item in self.inputs.items()
            if name != "codex"
        }

    @property
    def pass_fds(self) -> tuple[int, ...]:
        return tuple(item.descriptor for item in self.inputs.values())

    @property
    def executable_handoff_path(self) -> str:
        executable = self.inputs["executable"]
        if sys.platform.startswith("linux") and Path("/proc/self/fd").is_dir():
            return f"/proc/self/fd/{executable.descriptor}"
        if sys.platform == "darwin":
            executable.verify(pathname=True)
            return str(executable.path)
        raise LaunchMaterializationError(
            "platform cannot execute the retained launch descriptor safely"
        )

    def child_path(self, name: str) -> str:
        return self.inputs[name].child_path

    def verify_handoff(self) -> None:
        for item in self.inputs.values():
            item.verify(pathname=item.name == "executable" and sys.platform == "darwin")

    def close(self) -> None:
        for item in self.inputs.values():
            try:
                item.close()
            except OSError:
                pass


def prepare_process(request: object, spec: FrozenLaunchSpec) -> PreparedProcess:
    """Materialize and retain every launch byte until process consumption ends."""
    request_identity = measure_request_identity(request)
    runtime = request.cell.home / ".runtime"
    launch_root = runtime / "launch"
    launch_root.mkdir(mode=0o700)
    inputs: dict[str, BoundLaunchInput] = {}
    try:
        inputs["codex"] = _open_bound(
            "codex", request.binary_path, request_identity["binary_sha256"]
        )
        inputs["executable"] = _materialize(
            "executable", runtime / "launcher", spec.executable_payload, 0o700
        )
        fixed = {
            "config": ("effective-config.bin", spec.effective_config),
            "profile": ("execution-profile.json", spec.execution_profile_json),
            "route": ("model-route.json", spec.model_route_json),
            "protocol": ("app-server-protocol.json", spec.app_server_protocol_schema),
            "promptfoo": ("promptfoo-config.json", spec.promptfoo_config),
            "case": ("case-bundle.json", request.case_bundle_json),
            "schema": ("case-answer-schema.json", request.case_answer_schema_json),
        }
        for name, (relative, payload) in fixed.items():
            inputs[name] = _materialize(name, launch_root / relative, payload, 0o600)
        for artifact in spec.artifacts:
            name = f"artifact:{artifact.relative_path}"
            inputs[name] = _materialize(
                name,
                launch_root / "artifacts" / artifact.relative_path,
                artifact.payload,
                0o600,
            )
        prepared = PreparedProcess(request, spec, request_identity, inputs)
        if prepared.inputs["executable"].sha256 != spec.executable_sha256:
            raise LaunchMaterializationError(
                "copied launch executable identity differs"
            )
        prepared.verify_handoff()
        return prepared
    except BaseException:
        for item in inputs.values():
            try:
                item.close()
            except OSError:
                pass
        raise


def expand_argument(argument: str, prepared: PreparedProcess) -> str:
    values = {
        "{codex}": "codex",
        "{config}": "config",
        "{profile}": "profile",
        "{route}": "route",
        "{protocol}": "protocol",
        "{promptfoo}": "promptfoo",
        "{case}": "case",
        "{schema}": "schema",
    }
    values.update(
        {
            "{" + name + "}": name
            for name in prepared.inputs
            if name.startswith("artifact:")
        }
    )
    if argument in values:
        return prepared.child_path(values[argument])
    if "{" in argument or "}" in argument:
        raise LaunchMaterializationError("launch argv contains an unknown placeholder")
    return argument
