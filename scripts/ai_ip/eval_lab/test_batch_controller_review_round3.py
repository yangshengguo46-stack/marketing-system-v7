import copy
import json
import shutil
import sys
from pathlib import Path

import pytest


sys.path.insert(0, str(Path(__file__).resolve().parent))
import batch_controller  # noqa: E402
import batch_controller_capture  # noqa: E402
import batch_isolation  # noqa: E402
from batch_contracts import seal_self_commitment  # noqa: E402
from batch_controller import (  # noqa: E402
    AttemptAttestation,
    AttemptByteSource,
    AttemptTelemetry,
    BatchControllerError,
    RawAttemptResult,
    RunningAttempt,
    run_candidate_pair,
)
from batch_plan import PARITY_FIELDS, seal_candidate_run_plan, sha256_tree  # noqa: E402
from batch_receipts import BatchReceiptError, verify_paired_run_receipt  # noqa: E402
from contracts import canonical_json_bytes, load_exact_json, sha256_json  # noqa: E402
from test_batch_controller import (  # noqa: E402
    ScriptedExecutor,
    World,
    _metadata,
    _result,
    world,
)


def _write_json(path: Path, value: object) -> None:
    path.write_bytes(canonical_json_bytes(value) + b"\n")


def _reseal(value: dict[str, object]) -> dict[str, object]:
    value["receiptSha256"] = "0" * 64
    return seal_self_commitment(value, "receiptSha256")


def _one_second_plan(world: World) -> tuple[dict[str, object], dict[str, object]]:
    bindings = copy.deepcopy(world.bindings)
    profile = dict(bindings["executionProfile"])
    profile["maxWallClockSeconds"] = 1
    bindings["executionProfile"] = profile
    _write_json(bindings["executionProfilePath"], profile)
    plan = dict(world.plan)
    plan["timeoutBudget"] = 1
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


def test_request_json_identity_is_deeply_immutable(world: World) -> None:
    class MutatingAdapter(ScriptedExecutor):
        def __post_init__(self):
            super().__post_init__()
            self.rejected_mutations = 0

        def start(self, request):
            for value, key in (
                (request.model_route_json, 0),
                (request.execution_profile_json, 0),
                (request.case_bundle_json, 0),
                (request.case_answer_schema_json, 0),
            ):
                try:
                    value[key] = "substituted"
                except (AttributeError, TypeError):
                    self.rejected_mutations += 1
            return super().start(request)

    executor = MutatingAdapter([_result("left"), _result("right")])

    receipt = run_candidate_pair(
        world.plan, world.bindings, executor, world.private_root, seed=b"a" * 32
    )

    assert receipt["pairValidity"] == "valid"
    assert executor.rejected_mutations == 8


def test_staged_seed_replacement_is_remeasured_before_any_start(
    world: World, monkeypatch: pytest.MonkeyPatch
) -> None:
    real_create = batch_controller.create_attempt_cell

    def replace_staged_config(*args, **kwargs):
        seed = Path(args[3])
        if "seed-a" in seed.name:
            (seed / "config.toml").write_text(
                "model = 'substituted-after-snapshot'\n", encoding="utf-8"
            )
        return real_create(*args, **kwargs)

    monkeypatch.setattr(batch_controller, "create_attempt_cell", replace_staged_config)
    executor = ScriptedExecutor([_result("left"), _result("right")])

    with pytest.raises(BatchControllerError, match="identity|seed|config|cell"):
        run_candidate_pair(
            world.plan, world.bindings, executor, world.private_root, seed=b"b" * 32
        )

    assert executor.calls == 0


def test_only_audited_supervisor_capability_is_admitted(world: World) -> None:
    class StartShaped:
        starts = 0

        def start(self, request):
            self.starts += 1
            raise AssertionError("untrusted start must not be called")

    executor = StartShaped()

    with pytest.raises(BatchControllerError, match="audited|supervisor|capability"):
        run_candidate_pair(
            world.plan, world.bindings, executor, world.private_root, seed=b"c" * 32
        )

    assert executor.starts == 0
    assert not world.private_root.exists()


def test_both_arms_start_before_poll_and_unconfirmed_stop_is_fatal(
    world: World,
) -> None:
    plan, bindings = _one_second_plan(world)
    events: list[str] = []

    class RefusingStopExecutor(ScriptedExecutor):
        def start(self, request):
            index = len(self.requests) + 1
            events.append(f"start-{index}")
            if index == 1:
                self.requests.append(request)

                class LiveHandle(RunningAttempt):
                    def __init__(self):
                        RunningAttempt.__init__(self)

                    def poll(self):
                        events.append("poll-1")
                        return None

                    def telemetry(self):
                        return AttemptTelemetry(1, 1, 1, 0)

                    def terminate_and_wait(self, deadline):
                        events.append("terminate-and-wait")
                        return False

                return LiveHandle()
            return super().start(request)

    executor = RefusingStopExecutor([_result("unused"), _result("second")])

    with pytest.raises(BatchControllerError, match="fatal|stop|orphan|supervisor"):
        run_candidate_pair(plan, bindings, executor, world.private_root, seed=b"d" * 32)

    assert executor.calls == 2
    assert events[:2] == ["start-1", "start-2"]
    assert list(world.private_root.glob("pairs/*/failure.json"))


class _LazyBytes(AttemptByteSource):
    def __init__(self, payload: bytes):
        AttemptByteSource.__init__(self)
        self.payload = payload
        self.offset = 0
        self.requests: list[int] = []

    def read(self, maximum: int) -> bytes:
        self._validate_capability()
        self.requests.append(maximum)
        chunk = self.payload[self.offset : self.offset + maximum]
        self.offset += len(chunk)
        return chunk


@pytest.mark.parametrize(
    "payload",
    [
        b'{"value":"' + b"x" * (1_048_576 + 1) + b'"}',
        (b"[" * 2_000) + b"0" + (b"]" * 2_000),
    ],
    ids=("oversized", "deep"),
)
def test_lazy_json_source_is_bounded_before_materialization(
    world: World, payload: bytes
) -> None:
    output = _LazyBytes(payload)
    metadata = _LazyBytes(canonical_json_bytes(_metadata("lazy")))
    stdout = _LazyBytes(b"stdout")
    stderr = _LazyBytes(b"")
    first = RawAttemptResult(
        0,
        "2026-08-31T12:00:00Z",
        "2026-08-31T12:00:01Z",
        output,
        metadata,
        stdout,
        stderr,
    )

    receipt = run_candidate_pair(
        world.plan,
        world.bindings,
        ScriptedExecutor([first, _result("other")]),
        world.private_root,
        seed=b"e" * 32,
    )

    assert receipt["pairValidity"] == "invalid"
    assert output.requests
    assert max(output.requests) <= 64 * 1024
    assert output.offset <= 1_048_577


def test_completed_pair_rejects_replaced_layout_even_if_record_is_rewritten(
    world: World,
) -> None:
    pair = run_candidate_pair(
        world.plan,
        world.bindings,
        ScriptedExecutor([_result("left"), _result("right")]),
        world.private_root,
        seed=b"f" * 32,
    )
    attempts = world.private_root / "attempts"
    held = world.private_root / "attempts-held"
    attempts.rename(held)
    shutil.copytree(held, attempts)
    layout_path = world.private_root / f"layout-{pair['pairId']}.json"
    layout = load_exact_json(layout_path)
    for entry in layout["entries"]:
        state = (world.private_root / entry["path"]).lstat()
        entry["device"], entry["inode"] = state.st_dev, state.st_ino
    _write_json(layout_path, layout)

    with pytest.raises(BatchReceiptError, match="layout|identity|commitment"):
        verify_paired_run_receipt(pair, world.private_root)


@pytest.mark.parametrize(
    "failure_point",
    ["second-prepare", "second-arm-seal", "pair-seal", "final-verify"],
)
def test_lifecycle_failures_terminalize_every_created_cell(
    world: World, monkeypatch: pytest.MonkeyPatch, failure_point: str
) -> None:
    created = []
    real_create = batch_controller.create_attempt_cell

    def record_create(*args, **kwargs):
        cell = real_create(*args, **kwargs)
        created.append(cell)
        return cell

    monkeypatch.setattr(batch_controller, "create_attempt_cell", record_create)
    if failure_point == "second-prepare":
        real = batch_controller.prepare_arm
        calls = 0

        def fail(*args, **kwargs):
            nonlocal calls
            calls += 1
            if calls == 2:
                raise RuntimeError("second prepare failed")
            return real(*args, **kwargs)

        monkeypatch.setattr(batch_controller, "prepare_arm", fail)
    elif failure_point == "second-arm-seal":
        real = batch_controller.seal_arm
        calls = 0

        def fail(*args, **kwargs):
            nonlocal calls
            calls += 1
            if calls == 2:
                raise RuntimeError("second arm seal failed")
            return real(*args, **kwargs)

        monkeypatch.setattr(batch_controller, "seal_arm", fail)
    elif failure_point == "pair-seal":
        monkeypatch.setattr(
            batch_controller,
            "seal_paired_run_receipt",
            lambda *args, **kwargs: (_ for _ in ()).throw(RuntimeError("pair seal")),
        )
    else:
        monkeypatch.setattr(
            batch_controller,
            "verify_paired_run_receipt",
            lambda *args, **kwargs: (_ for _ in ()).throw(RuntimeError("final verify")),
        )

    with pytest.raises(BatchControllerError):
        run_candidate_pair(
            world.plan,
            world.bindings,
            ScriptedExecutor([_result("left"), _result("right")]),
            world.private_root,
            seed=b"g" * 32,
        )

    assert len(created) == 2
    assert all(cell._capability not in batch_isolation._CELLS for cell in created)
    assert all(not cell.root.exists() for cell in created)
    assert list(world.private_root.glob("pairs/*/failure.json"))
    assert len(list(world.private_root.glob("attempts/*/*failure*.json"))) >= 2


def test_resealed_false_condition_parity_is_rejected(world: World) -> None:
    pair = run_candidate_pair(
        world.plan,
        world.bindings,
        ScriptedExecutor([_result("left"), _result("right")]),
        world.private_root,
        seed=b"h" * 32,
    )
    pair["conditionParitySha256"] = "f" * 64
    pair = _reseal(pair)
    pair_path = world.private_root / "pairs" / str(pair["pairId"]) / "receipt.json"
    _write_json(pair_path, pair)

    with pytest.raises(BatchReceiptError, match="parity|condition"):
        verify_paired_run_receipt(pair, world.private_root)
