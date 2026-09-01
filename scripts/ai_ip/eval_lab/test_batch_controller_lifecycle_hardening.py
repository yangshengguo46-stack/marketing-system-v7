import sys
from pathlib import Path

import pytest


sys.path.insert(0, str(Path(__file__).resolve().parent))
import batch_controller  # noqa: E402
import batch_controller_lifecycle  # noqa: E402
import batch_controller_supervision  # noqa: E402
from batch_controller import BatchControllerError  # noqa: E402
from batch_controller_process_lease import (  # noqa: E402
    OrphanedProcess,
    ProcessLeaseError,
    ProcessLeaseState,
    ProcessLifecycleLease,
)
from batch_controller_support import derive, identities, pair_seed  # noqa: E402
from batch_launch_spec import launch_spec_identity  # noqa: E402
from test_batch_controller import World, world  # noqa: E402
from test_batch_controller_review_round5 import (  # noqa: E402
    _BOUND_CHILD,
    _kill_test_processes,
    _launches,
    _with_timeout,
)


def _lease() -> ProcessLifecycleLease:
    return ProcessLifecycleLease(
        pair_id="a" * 64,
        arm_class="stock",
        attempt_id="b" * 64,
        launch_spec_sha256="c" * 64,
    )


def test_process_lease_rejects_duplicate_attach_and_terminal_reversal() -> None:
    lease = _lease()
    lease.attach_raw_process(123, 123)
    with pytest.raises(ProcessLeaseError, match="state"):
        lease.attach_raw_process(124, 124)
    orphan = lease.mark_orphaned(RuntimeError("unconfirmed"))
    assert orphan == OrphanedProcess(
        pair_id="a" * 64,
        arm_class="stock",
        attempt_id="b" * 64,
        process_id=123,
        process_group_id=123,
        launch_spec_sha256="c" * 64,
        error_type="RuntimeError",
    )
    with pytest.raises(ProcessLeaseError, match="state"):
        lease.confirm_stopped()


@pytest.mark.parametrize(
    ("path", "expected_state"),
    [
        ("cancel", ProcessLeaseState.CANCELLED_BEFORE_START),
        ("attach-stop", ProcessLeaseState.STOP_CONFIRMED),
        ("promote-stop", ProcessLeaseState.STOP_CONFIRMED),
        ("promote-orphan", ProcessLeaseState.ORPHANED),
    ],
)
def test_process_lease_allows_only_declared_forward_terminal_paths(
    path: str, expected_state: ProcessLeaseState
) -> None:
    lease = _lease()
    owned = object()

    if path == "cancel":
        lease.cancel_before_start()
    else:
        lease.attach_raw_process(123, 123)
        if path.startswith("promote"):
            lease.promote(owned)
        if path.endswith("stop"):
            lease.confirm_stopped()
        else:
            orphan = lease.mark_orphaned(RuntimeError("unconfirmed"))
            assert orphan == OrphanedProcess(
                pair_id="a" * 64,
                arm_class="stock",
                attempt_id="b" * 64,
                process_id=123,
                process_group_id=123,
                launch_spec_sha256="c" * 64,
                error_type="RuntimeError",
            )
            assert lease.mark_orphaned(ValueError("later")) == orphan

    assert lease.state is expected_state


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("pair_id", "A" * 64),
        ("pair_id", "a" * 63),
        ("pair_id", "g" * 64),
        ("attempt_id", ""),
        ("attempt_id", 1),
        ("launch_spec_sha256", "c" * 65),
        ("arm_class", "control"),
        ("arm_class", 1),
    ],
)
def test_process_lease_rejects_malformed_authority_identity(
    field: str, value: object
) -> None:
    values: dict[str, object] = {
        "pair_id": "a" * 64,
        "arm_class": "stock",
        "attempt_id": "b" * 64,
        "launch_spec_sha256": "c" * 64,
    }
    values[field] = value

    with pytest.raises(ProcessLeaseError):
        ProcessLifecycleLease(**values)


@pytest.mark.parametrize(
    ("process_id", "process_group_id"),
    [
        (0, 123),
        (-1, 123),
        (True, 123),
        (123, 0),
        (123, -1),
        (123, False),
    ],
)
def test_process_lease_rejects_non_positive_or_boolean_process_identity(
    process_id: object, process_group_id: object
) -> None:
    lease = _lease()

    with pytest.raises(ProcessLeaseError):
        lease.attach_raw_process(process_id, process_group_id)

    assert lease.state is ProcessLeaseState.RESERVED


def test_process_lease_rejects_promotion_with_a_second_owned_object() -> None:
    lease = _lease()
    first = object()
    lease.attach_raw_process(123, 123)
    lease.promote(first)

    with pytest.raises(ProcessLeaseError, match="state"):
        lease.promote(object())

    assert lease.state is ProcessLeaseState.PROMOTED_OWNED_PROCESS


def test_post_popen_aggregation_preserves_authenticated_orphan_authority(
    world: World, monkeypatch: pytest.MonkeyPatch
) -> None:
    plan, bindings = _with_timeout(world, 1)
    child_world = World(plan, bindings, world.private_root)
    launches = _launches(child_world, _BOUND_CHILD)
    validated = batch_controller.validate_bindings(plan, bindings)
    frozen_launches = batch_controller.validate_launch_set(launches, validated)
    active_seed = pair_seed(b"l" * 32, world.private_root, plan["planSha256"])
    pair_id, stock_attempt_id, modified_attempt_id = identities(active_seed)
    first_arm = (
        "stock" if derive(active_seed, b"execution-order")[0] % 2 == 0 else "modified"
    )
    expected_attempt_id = {
        "stock": stock_attempt_id,
        "modified": modified_attempt_id,
    }[first_arm]
    expected_launch_digest = launch_spec_identity(
        getattr(frozen_launches, first_arm)
    )[1]
    process_module = batch_controller._process
    real_popen = process_module._Popen
    real_owned_process = process_module.OwnedProcess
    real_terminate = batch_controller_supervision._terminate_process
    real_abort = batch_controller_lifecycle.PairLifecycle.abort
    processes: list[object] = []
    orphan_snapshots: list[tuple[OrphanedProcess, ...]] = []
    state = {"owned_failed": False, "unconfirmed_stop": False}

    def tracking_popen(*args, **kwargs):
        process = real_popen(*args, **kwargs)
        processes.append(process)
        return process

    def owned_process(*args, **kwargs):
        if not state["owned_failed"]:
            state["owned_failed"] = True
            raise RuntimeError("injected owned process construction failure")
        return real_owned_process(*args, **kwargs)

    def terminate_then_report_first_unconfirmed(process, group_id, deadline):
        confirmed = real_terminate(process, group_id, deadline)
        if not state["unconfirmed_stop"]:
            state["unconfirmed_stop"] = True
            return False
        return confirmed

    def capture_abort(lifecycle, error):
        try:
            return real_abort(lifecycle, error)
        finally:
            orphan_snapshots.append(lifecycle.orphaned_processes)

    monkeypatch.setattr(process_module, "_Popen", tracking_popen)
    monkeypatch.setattr(process_module, "OwnedProcess", owned_process)
    monkeypatch.setattr(
        batch_controller_supervision,
        "_terminate_process",
        terminate_then_report_first_unconfirmed,
    )
    monkeypatch.setattr(batch_controller_lifecycle.PairLifecycle, "abort", capture_abort)

    try:
        with pytest.raises(BatchControllerError, match="launch|process|lifecycle"):
            batch_controller.run_candidate_pair(
                plan,
                bindings,
                launches,
                world.private_root,
                seed=b"l" * 32,
            )

        assert len(processes) == 2
        assert not list(world.private_root.glob("pairs/*/receipt.json"))
        assert list(world.private_root.glob("pairs/*/failure.json"))
        assert len(list(Path(bindings["attemptBase"]).iterdir())) == 2
        assert orphan_snapshots == [
            (
                OrphanedProcess(
                    pair_id=pair_id,
                    arm_class=first_arm,
                    attempt_id=expected_attempt_id,
                    process_id=processes[0].pid,
                    process_group_id=processes[0].pid,
                    launch_spec_sha256=expected_launch_digest,
                    error_type="RuntimeError",
                ),
            )
        ]
    finally:
        _kill_test_processes(processes)
