"""Explicit test-only compiler from legacy scripted results to launch specs.

This module never participates in production preflight.  It lets the historical
controller tests exercise the concrete process boundary while they migrate away
from callback-shaped fixtures.
"""

import base64
import hashlib
import json
import os
import sys
from pathlib import Path

try:
    import batch_controller as _controller
    from batch_controller_support import derive, identities, pair_seed, replication_seeds
    from batch_launch_spec import CandidateLaunchSet, LaunchArtifact, LaunchSpec
    from batch_plan import sha256_file
    from contracts import canonical_json_bytes
except ImportError:
    from . import batch_controller as _controller
    from .batch_controller_support import derive, identities, pair_seed, replication_seeds
    from .batch_launch_spec import CandidateLaunchSet, LaunchArtifact, LaunchSpec
    from .batch_plan import sha256_file
    from .contracts import canonical_json_bytes


_RUNNER = b"""\
import base64
import hashlib
import json
import os
import sys
import time

cases = json.load(open(sys.argv[1], encoding="utf-8"))
case = cases[os.environ["AI_IP_ATTEMPT_ID"]]
time.sleep(case["sleepSeconds"])
decode = lambda name: base64.b64decode(case[name])
envelope = b'{"output":' + decode("output") + b',"metadata":' + decode("metadata") + b'}'
os.write(int(os.environ["AI_IP_RESULT_FD"]), envelope)
os.write(int(os.environ["AI_IP_TELEMETRY_FD"]), canonical := json.dumps(
    case["telemetry"], sort_keys=True, separators=(",", ":")
).encode())
os.write(1, decode("stdout"))
os.write(2, decode("stderr"))
sys.exit(case["exitCode"])
"""


def _source_bytes(source: object, limit: int) -> bytes:
    chunks: list[bytes] = []
    size = 0
    while size <= limit:
        chunk = source.read(min(64 * 1024, limit + 1 - size))
        if type(chunk) is not bytes:
            return b"null"
        if not chunk:
            break
        chunks.append(chunk)
        size += len(chunk)
    return b"".join(chunks)


def _telemetry(metadata: bytes) -> dict[str, int]:
    try:
        value = json.loads(metadata)
    except (UnicodeDecodeError, json.JSONDecodeError):
        value = {}
    if type(value) is not dict:
        value = {}
    usage = value.get("usage")
    cost = value.get("costEvidence")
    return {
        "requestCount": int(value.get("requestCount", 0)),
        "inputTokens": int(usage.get("inputTokens", 0)) if type(usage) is dict else 0,
        "outputTokens": int(usage.get("outputTokens", 0)) if type(usage) is dict else 0,
        "costCny": int(cost.get("costCny", 0)) if type(cost) is dict else 0,
    }


def _case(
    result: object,
    limit: int,
    telemetry_override: dict[str, int] | None,
    sleep_seconds: int,
) -> dict[str, object]:
    output = _source_bytes(result.output, limit)
    metadata = _source_bytes(result.metadata, limit)
    return {
        "exitCode": result.exit_code if type(result.exit_code) is int else 255,
        "output": base64.b64encode(output).decode(),
        "metadata": base64.b64encode(metadata).decode(),
        "stdout": base64.b64encode(_source_bytes(result.stdout, limit)).decode(),
        "stderr": base64.b64encode(_source_bytes(result.stderr, limit)).decode(),
        "telemetry": (
            _telemetry(metadata) if telemetry_override is None else telemetry_override
        ),
        "sleepSeconds": sleep_seconds,
    }


def _active_seeds(
    plan: dict[str, object], private_root: Path, seed: bytes, batch: bool
) -> tuple[bytes, ...]:
    if batch:
        return replication_seeds(plan, private_root, seed)
    return (pair_seed(seed, private_root, plan["planSha256"]),)


def _launches(
    plan: dict[str, object],
    bindings: dict[str, object],
    executor: object,
    private_root: Path,
    seed: bytes,
    batch: bool,
) -> CandidateLaunchSet:
    limit = min(int(bindings["executionProfile"]["maxOutputBytes"]), 16 * 1024 * 1024)
    cases: dict[str, object] = {}
    results = executor.results
    for pair_index, active_seed in enumerate(
        _active_seeds(plan, private_root, seed, batch)
    ):
        _, stock_id, modified_id = identities(active_seed)
        order = (
            ("stock", "modified")
            if derive(active_seed, b"execution-order")[0] % 2 == 0
            else ("modified", "stock")
        )
        attempt_ids = {"stock": stock_id, "modified": modified_id}
        for offset, arm in enumerate(order):
            result_index = pair_index * 2 + offset
            overrides = getattr(executor, "test_telemetry", None)
            override = overrides[result_index] if type(overrides) is list else None
            sleeps = getattr(executor, "test_sleep", None)
            sleep_seconds = sleeps[result_index] if type(sleeps) is list else 0
            cases[attempt_ids[arm]] = _case(
                results[result_index], limit, override, sleep_seconds
            )
    case_bytes = canonical_json_bytes(cases)
    artifacts = (
        LaunchArtifact("runner.py", _RUNNER, hashlib.sha256(_RUNNER).hexdigest()),
        LaunchArtifact("cases.json", case_bytes, hashlib.sha256(case_bytes).hexdigest()),
    )
    executable = Path(sys.executable).resolve()

    def spec(arm: str) -> LaunchSpec:
        binding = bindings[arm]
        return LaunchSpec(
            executable,
            sha256_file(executable),
            ("{artifact:runner.py}", "{artifact:cases.json}"),
            (),
            artifacts,
            Path(binding["effectiveConfig"]).read_bytes(),
            canonical_json_bytes(bindings["executionProfile"]),
            canonical_json_bytes(bindings["modelRoute"]),
            Path(bindings["appServerProtocolSchemaPath"]).read_bytes(),
            Path(bindings["promptfooConfigPath"]).read_bytes(),
        )

    return CandidateLaunchSet(spec("stock"), spec("modified"))


def _run(
    method: object,
    plan: dict[str, object],
    bindings: dict[str, object],
    executor: object,
    private_root: Path,
    seed: bytes | None,
    batch: bool,
):
    active_seed = os.urandom(32) if seed is None else seed
    launches = _launches(plan, bindings, executor, Path(private_root), active_seed, batch)
    original = _controller._process.prepare_process

    def observe(request, spec):
        prepared = original(request, spec)
        executor.start(request)
        return prepared

    _controller._process.prepare_process = observe
    try:
        return method(plan, bindings, launches, private_root, seed=active_seed)
    finally:
        _controller._process.prepare_process = original


def run_candidate_pair(
    plan, bindings, executor, private_root: Path, *, seed: bytes | None = None
):
    return _run(
        _controller.run_candidate_pair,
        plan,
        bindings,
        executor,
        private_root,
        seed,
        False,
    )


def run_candidate_batch(
    plan, bindings, executor, private_root: Path, *, seed: bytes | None = None
):
    return _run(
        _controller.run_candidate_batch,
        plan,
        bindings,
        executor,
        private_root,
        seed,
        True,
    )
