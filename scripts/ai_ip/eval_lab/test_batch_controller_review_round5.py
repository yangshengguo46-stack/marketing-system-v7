import copy
import hashlib
import json
import os
import signal
import sys
import time
from pathlib import Path

import pytest


sys.path.insert(0, str(Path(__file__).resolve().parent))
import batch_controller  # noqa: E402
import batch_isolation  # noqa: E402
from batch_controller import BatchControllerError  # noqa: E402
from batch_plan import PARITY_FIELDS, seal_candidate_run_plan, sha256_file  # noqa: E402
from contracts import canonical_json_bytes, sha256_json  # noqa: E402
from test_batch_controller import World, world  # noqa: E402


_ANSWER_SOURCE = r"""
def answer():
    return {
        "schemaVersion": 1,
        "objectKind": "CaseAnswer",
        "caseId": "case-1",
        "taskLevels": ["L1", "L2"],
        "subject": "sealed subject",
        "audiences": ["audience"],
        "desiredActions": ["act"],
        "evidenceGaps": [],
        "directionOptions": [{
            "directionId": "direction",
            "directionFamily": "family",
            "rationale": "because",
            "tradeoffs": ["tradeoff"],
            "conditions": ["condition"],
            "evidenceRefs": [],
        }],
        "recommendedDirectionId": "direction",
        "claims": [],
        "readiness": "readyForHumanReview",
    }
"""

_BOUND_CHILD = (
    "import hashlib,json,os,sys\n"
    + _ANSWER_SOURCE
    + r"""
names = {
    "case": os.environ["AI_IP_CASE_PATH"],
    "codex": os.environ["AI_IP_CODEX_PATH"],
    "config": os.environ["AI_IP_CONFIG_PATH"],
    "profile": os.environ["AI_IP_PROFILE_PATH"],
    "route": os.environ["AI_IP_ROUTE_PATH"],
    "protocol": os.environ["AI_IP_PROTOCOL_PATH"],
    "promptfoo": os.environ["AI_IP_PROMPTFOO_PATH"],
    "schema": os.environ["AI_IP_SCHEMA_PATH"],
    "executable": sys.executable,
    "artifact:runner.py": sys.argv[0],
}
consumed = {
    name: hashlib.sha256(
        os.pread(
            descriptor := os.open(path, os.O_RDONLY),
            os.fstat(descriptor).st_size,
            0,
        )
    ).hexdigest()
    for name, path in names.items()
}
metadata = {
    "threadId": "thread",
    "turnId": "turn",
    "trajectory": [{"type": "turn.completed"}],
    "usage": {"inputTokens": 1, "outputTokens": 2, "totalTokens": 3},
    "costEvidence": {"costCny": 0, "sourceSha256": "e" * 64},
    "requestCount": 1,
}
result = json.dumps(
    {"output": answer(), "metadata": metadata},
    sort_keys=True,
    separators=(",", ":"),
).encode()
telemetry = json.dumps(
    {"requestCount": 1, "inputTokens": 1, "outputTokens": 2, "costCny": 0},
    sort_keys=True,
    separators=(",", ":"),
).encode()
os.write(int(os.environ["AI_IP_RESULT_FD"]), result)
os.write(int(os.environ["AI_IP_TELEMETRY_FD"]), telemetry)
os.write(2, json.dumps(consumed, sort_keys=True, separators=(",", ":")).encode())
"""
).encode()

_SUBSTITUTED_CHILD = _BOUND_CHILD + b"\n# substituted after descriptor preparation\n"

_DESCENDANT_CHILD = (
    "import json,os,signal,time\n"
    + _ANSWER_SOURCE
    + r"""
metadata = {
    "threadId": "thread",
    "turnId": "turn",
    "trajectory": [{"type": "turn.completed"}],
    "usage": {"inputTokens": 1, "outputTokens": 2, "totalTokens": 3},
    "costEvidence": {"costCny": 0, "sourceSha256": "e" * 64},
    "requestCount": 1,
}
result = json.dumps(
    {"output": answer(), "metadata": metadata},
    sort_keys=True,
    separators=(",", ":"),
).encode()
telemetry = b'{"costCny":0,"inputTokens":1,"outputTokens":2,"requestCount":1}'
pid = os.fork()
if pid == 0:
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    while True:
        os.write(1, b"descendant-alive\n")
        time.sleep(0.02)
os.write(int(os.environ["AI_IP_RESULT_FD"]), result)
os.write(int(os.environ["AI_IP_TELEMETRY_FD"]), telemetry)
pid_path = os.environ.get("AI_IP_DESCENDANT_PID_PATH")
if pid_path:
    open(pid_path, "w").write(str(pid))
os.write(2, (str(pid) + "\n").encode())
"""
).encode()

_FOUR_STREAM_CHILD = b"""\
import os

descriptors = (
    int(os.environ["AI_IP_RESULT_FD"]),
    int(os.environ["AI_IP_TELEMETRY_FD"]),
    1,
    2,
)
for descriptor in descriptors:
    os.set_blocking(descriptor, False)
remaining = {descriptor: 512 * 1024 for descriptor in descriptors}
block = b"x" * 4096
while any(remaining.values()):
    for descriptor in descriptors:
        if remaining[descriptor] == 0:
            continue
        try:
            written = os.write(descriptor, block[:remaining[descriptor]])
        except BlockingIOError:
            continue
        remaining[descriptor] -= written
"""


def _with_timeout(
    world: World, seconds: int
) -> tuple[dict[str, object], dict[str, object]]:
    bindings = copy.deepcopy(world.bindings)
    profile = dict(bindings["executionProfile"])
    profile["maxWallClockSeconds"] = seconds
    bindings["executionProfile"] = profile
    Path(bindings["executionProfilePath"]).write_bytes(
        canonical_json_bytes(profile) + b"\n"
    )
    plan = dict(world.plan)
    plan["timeoutBudget"] = seconds
    plan["executionProfileRef"] = sha256_json(profile)
    plan["planSha256"] = "0" * 64
    plan = seal_candidate_run_plan(
        plan,
        stock_treatment=bindings["stock"]["treatmentManifest"],
        modified_treatment=bindings["modified"]["treatmentManifest"],
    )
    parity = {field: plan[field] for field in PARITY_FIELDS}
    bindings["stock"]["effectiveConditions"] = parity
    bindings["modified"]["effectiveConditions"] = parity
    return plan, bindings


def _launches(
    world: World,
    script_bytes: bytes,
    *,
    environment: tuple[tuple[str, str], ...] = (),
):
    artifact_type = batch_controller.LaunchArtifact
    spec_type = batch_controller.LaunchSpec
    launch_set_type = batch_controller.CandidateLaunchSet
    script = artifact_type(
        "runner.py", script_bytes, hashlib.sha256(script_bytes).hexdigest()
    )
    executable = Path(sys.executable).resolve()

    def spec(arm: str):
        binding = world.bindings[arm]
        return spec_type(
            executable_path=executable,
            executable_sha256=sha256_file(executable),
            argv=("{artifact:runner.py}",),
            environment=environment,
            artifacts=(script,),
            effective_config=Path(binding["effectiveConfig"]).read_bytes(),
            execution_profile_json=canonical_json_bytes(
                world.bindings["executionProfile"]
            ),
            model_route_json=canonical_json_bytes(world.bindings["modelRoute"]),
            app_server_protocol_schema=Path(
                world.bindings["appServerProtocolSchemaPath"]
            ).read_bytes(),
            promptfoo_config=Path(world.bindings["promptfooConfigPath"]).read_bytes(),
        )

    return launch_set_type(stock=spec("stock"), modified=spec("modified"))


def _replace(path: Path, payload: bytes) -> None:
    replacement = path.with_name(path.name + ".substituted")
    replacement.write_bytes(payload)
    replacement.chmod(path.stat().st_mode & 0o777)
    os.replace(replacement, path)


def _process_is_absent(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return True
    return False


def _kill_test_processes(processes: list[object]) -> None:
    for process in processes:
        if process.poll() is None:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait(timeout=2)


def test_materialized_launch_inputs_are_consumed_from_bound_descriptors_or_fail_closed(
    world: World, monkeypatch: pytest.MonkeyPatch
) -> None:
    process_module = batch_controller._process
    original_spawn = process_module.spawn

    def substitute_paths(prepared, *args, **kwargs):
        for name, path in prepared.artifact_paths.items():
            payload = (
                _SUBSTITUTED_CHILD
                if name == "artifact:runner.py"
                else b"substituted:" + name.encode()
            )
            _replace(path, payload)
        return original_spawn(prepared, *args, **kwargs)

    monkeypatch.setattr(process_module, "spawn", substitute_paths)
    try:
        receipt = batch_controller.run_candidate_pair(
            world.plan,
            world.bindings,
            _launches(world, _BOUND_CHILD),
            world.private_root,
            seed=b"r" * 32,
        )
    except BatchControllerError:
        assert not list(world.private_root.glob("pairs/*/receipt.json"))
        return

    assert receipt["pairValidity"] != "valid" or all(
        json.loads(stderr.read_bytes())
        == {
            "artifact:runner.py": record["artifactSha256"]["artifact:runner.py"],
            "case": record["artifactSha256"]["case"],
            "codex": record["candidateBinarySha256"],
            "config": record["artifactSha256"]["config"],
            "profile": record["artifactSha256"]["profile"],
            "promptfoo": record["artifactSha256"]["promptfoo"],
            "protocol": record["artifactSha256"]["protocol"],
            "route": record["artifactSha256"]["route"],
            "schema": record["artifactSha256"]["schema"],
            "executable": record["executableSha256"],
        }
        for stderr, record in (
            (
                attempt / "stderr.bin",
                json.loads((attempt / "launch-record.json").read_bytes()),
            )
            for attempt in world.private_root.glob("attempts/*")
        )
    )


def test_absolute_deadline_stops_descendant_group_after_leader_exits(
    world: World,
) -> None:
    plan, bindings = _with_timeout(world, 1)
    child_world = World(plan, bindings, world.private_root)
    started = time.monotonic()
    pid_path = world.private_root.parent / "descendant.pid"
    descendant_absent_before_cleanup = False
    previous_handler = signal.getsignal(signal.SIGALRM)

    def controller_hung(signum, frame):
        raise TimeoutError("controller exceeded the bounded test deadline")

    signal.signal(signal.SIGALRM, controller_hung)
    signal.setitimer(signal.ITIMER_REAL, 4)
    try:
        receipt = batch_controller.run_candidate_pair(
            plan,
            bindings,
            _launches(
                child_world,
                _DESCENDANT_CHILD,
                environment=(("AI_IP_DESCENDANT_PID_PATH", str(pid_path)),),
            ),
            world.private_root,
            seed=b"s" * 32,
        )
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous_handler)
        if pid_path.exists():
            descendant_pid = int(pid_path.read_text())
            stop_deadline = time.monotonic() + 1
            while time.monotonic() < stop_deadline and not _process_is_absent(
                descendant_pid
            ):
                time.sleep(0.01)
            descendant_absent_before_cleanup = _process_is_absent(descendant_pid)
            if not descendant_absent_before_cleanup:
                try:
                    os.kill(descendant_pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass

    elapsed = time.monotonic() - started
    descendant_pids = [
        int(value)
        for value in (
            path.read_text().strip()
            for path in world.private_root.glob("attempts/*/stderr.bin")
        )
        if value
    ]
    assert elapsed < 3
    assert receipt["pairValidity"] == "invalid"
    assert receipt["invalidReason"] in {"budgetFailure", "evidenceFailure"}
    assert descendant_pids
    deadline = time.monotonic() + 1
    while time.monotonic() < deadline and not all(
        _process_is_absent(pid) for pid in descendant_pids
    ):
        time.sleep(0.01)
    assert descendant_absent_before_cleanup
    assert all(_process_is_absent(pid) for pid in descendant_pids)
    assert not list(world.private_root.glob("orphan-*.json"))


def test_four_streams_share_one_total_acquisition_and_storage_budget(
    world: World,
) -> None:
    plan, bindings = _with_timeout(world, 3)
    child_world = World(plan, bindings, world.private_root)

    receipt = batch_controller.run_candidate_pair(
        plan,
        bindings,
        _launches(child_world, _FOUR_STREAM_CHILD),
        world.private_root,
        seed=b"t" * 32,
    )

    assert receipt["pairValidity"] == "invalid"
    assert receipt["invalidReason"] in {"budgetFailure", "evidenceFailure"}
    for path in world.private_root.glob("attempts/*/evidence.json"):
        raw = json.loads(path.read_bytes())["rawEvidence"]
        streams = (
            raw["resultEnvelopeBytes"],
            raw["telemetryEnvelopeBytes"],
            raw["stdoutBytes"],
            raw["stderrBytes"],
        )
        assert sum(int(stream["storedSize"]) for stream in streams) <= 1_048_576
        assert raw["aggregateStreamBytes"]["acquiredSize"] <= 1_048_576
        assert raw["aggregateStreamBytes"]["truncated"] is True


def test_completed_process_stream_objects_relinquish_every_descriptor(
    world: World, monkeypatch: pytest.MonkeyPatch
) -> None:
    process_module = batch_controller._process
    real_popen = process_module._Popen
    processes: list[object] = []

    def tracking_popen(*args, **kwargs):
        process = real_popen(*args, **kwargs)
        processes.append(process)
        return process

    monkeypatch.setattr(process_module, "_Popen", tracking_popen)

    receipt = batch_controller.run_candidate_pair(
        world.plan,
        world.bindings,
        _launches(world, _BOUND_CHILD),
        world.private_root,
        seed=b"v" * 32,
    )

    assert receipt["pairValidity"] == "valid"
    assert processes
    assert all(process.stdout.closed and process.stderr.closed for process in processes)


@pytest.mark.parametrize(
    "checkpoint",
    [
        "closeChildWriters",
        "stdoutDescriptor",
        "setNonblocking",
        "launchSpecIdentity",
        "environmentDigest",
        "ownedProcess",
    ],
)
def test_every_post_popen_failure_remains_owned_until_stop(
    world: World,
    monkeypatch: pytest.MonkeyPatch,
    checkpoint: str,
) -> None:
    plan, bindings = _with_timeout(world, 1)
    child_world = World(plan, bindings, world.private_root)
    process_module = batch_controller._process
    real_popen = process_module._Popen
    processes: list[object] = []
    cells_before = set(batch_isolation._CELLS)
    state = {"returned": False, "failed": False, "write_fds": set()}

    def tracking_popen(*args, **kwargs):
        process = real_popen(*args, **kwargs)
        processes.append(process)
        state["write_fds"].update(kwargs["pass_fds"][-2:])
        state["returned"] = True
        return process

    monkeypatch.setattr(process_module, "_Popen", tracking_popen)

    def fail_once() -> None:
        if state["returned"] and not state["failed"]:
            state["failed"] = True
            raise RuntimeError(f"injected {checkpoint} failure")

    if checkpoint == "closeChildWriters":
        real_close = process_module.os.close

        def close(descriptor):
            if descriptor in state["write_fds"]:
                fail_once()
            return real_close(descriptor)

        monkeypatch.setattr(process_module.os, "close", close)
    elif checkpoint == "stdoutDescriptor":
        real_fileno = subprocess_pipe_fileno = None

        def popen_with_bad_stdout(*args, **kwargs):
            nonlocal real_fileno, subprocess_pipe_fileno
            process = tracking_popen(*args, **kwargs)
            real_fileno = process.stdout.fileno
            subprocess_pipe_fileno = process.stdout.fileno

            def fileno():
                fail_once()
                return subprocess_pipe_fileno()

            monkeypatch.setattr(process.stdout, "fileno", fileno)
            return process

        monkeypatch.setattr(process_module, "_Popen", popen_with_bad_stdout)
    elif checkpoint == "setNonblocking":
        real_set_blocking = process_module.os.set_blocking

        def set_blocking(descriptor, blocking):
            if state["returned"]:
                fail_once()
            return real_set_blocking(descriptor, blocking)

        monkeypatch.setattr(process_module.os, "set_blocking", set_blocking)
    elif checkpoint == "launchSpecIdentity":
        original = process_module.launch_spec_identity

        def launch_spec_identity(spec):
            fail_once()
            return original(spec)

        monkeypatch.setattr(
            process_module, "launch_spec_identity", launch_spec_identity
        )
    elif checkpoint == "environmentDigest":
        original = process_module.sha256_json

        def digest(value):
            fail_once()
            return original(value)

        monkeypatch.setattr(process_module, "sha256_json", digest)
    else:
        original = process_module.OwnedProcess

        def owned_process(*args, **kwargs):
            fail_once()
            return original(*args, **kwargs)

        monkeypatch.setattr(process_module, "OwnedProcess", owned_process)

    try:
        with pytest.raises(BatchControllerError, match="launch|process|lifecycle"):
            batch_controller.run_candidate_pair(
                plan,
                bindings,
                _launches(child_world, _BOUND_CHILD),
                world.private_root,
                seed=b"u" * 32,
            )

        assert state["failed"] is True
        assert processes
        assert all(process.poll() is not None for process in processes)
        assert not list(Path(bindings["attemptBase"]).glob("*"))
        assert not list(world.private_root.glob("orphan-*.json"))
        assert set(batch_isolation._CELLS) == cells_before
    finally:
        _kill_test_processes(processes)
