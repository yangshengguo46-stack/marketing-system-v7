import json
import os
import shutil
import sys
from pathlib import Path

import pytest


sys.path.insert(0, str(Path(__file__).resolve().parent))
import batch_controller  # noqa: E402
import batch_controller_lifecycle  # noqa: E402
import batch_controller_supervision  # noqa: E402
from batch_controller import BatchControllerError  # noqa: E402
from batch_orphan_authority import (  # noqa: E402
    OrphanAuthorityError,
    orphan_authority_name,
    verify_orphan_authority,
)
from batch_controller_process_lease import (  # noqa: E402
    OrphanedProcess,
    ProcessLeaseError,
    ProcessLeaseState,
    ProcessLifecycleLease,
)
from batch_controller_support import derive, identities, pair_seed  # noqa: E402
from batch_launch_spec import launch_spec_identity  # noqa: E402
from batch_receipt_offline import OfflineEvidence  # noqa: E402
from batch_receipt_storage import SecureStorageError  # noqa: E402
from contracts import canonical_json_bytes  # noqa: E402
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


def _run_post_popen_failure(
    world: World,
    monkeypatch: pytest.MonkeyPatch,
    *,
    seed: bytes,
    owned_failures: int,
    report_unconfirmed: bool,
    private_root: Path | None = None,
    authority_sealing_fails: bool = False,
) -> dict[str, object]:
    plan, bindings = _with_timeout(world, 1)
    active_root = world.private_root if private_root is None else private_root
    child_world = World(plan, bindings, active_root)
    launches = _launches(child_world, _BOUND_CHILD)
    validated = batch_controller.validate_bindings(plan, bindings)
    frozen_launches = batch_controller.validate_launch_set(launches, validated)
    active_seed = pair_seed(seed, active_root, plan["planSha256"])
    pair_id, stock_attempt_id, modified_attempt_id = identities(active_seed)
    order = (
        ("stock", "modified")
        if derive(active_seed, b"execution-order")[0] % 2 == 0
        else ("modified", "stock")
    )
    process_module = batch_controller._process
    real_popen = process_module._Popen
    real_owned_process = process_module.OwnedProcess
    real_terminate = batch_controller_supervision._terminate_process
    processes: list[object] = []
    failed_process_ids: set[int] = set()
    state = {"owned_failures": 0}

    def tracking_popen(*args, **kwargs):
        process = real_popen(*args, **kwargs)
        processes.append(process)
        return process

    def owned_process(*args, **kwargs):
        if state["owned_failures"] < owned_failures:
            state["owned_failures"] += 1
            failed_process_ids.add(args[0].pid)
            raise RuntimeError("injected owned process construction failure")
        return real_owned_process(*args, **kwargs)

    def terminate_with_declared_disposition(process, group_id, deadline):
        confirmed = real_terminate(process, group_id, deadline)
        if report_unconfirmed and process.pid in failed_process_ids:
            return False
        return confirmed

    with monkeypatch.context() as patch:
        patch.setattr(process_module, "_Popen", tracking_popen)
        patch.setattr(process_module, "OwnedProcess", owned_process)
        patch.setattr(
            batch_controller_supervision,
            "_terminate_process",
            terminate_with_declared_disposition,
        )
        if authority_sealing_fails:
            patch.setattr(
                batch_controller_lifecycle,
                "seal_orphan_authority",
                lambda *args, **kwargs: (_ for _ in ()).throw(
                    OSError("injected authority write failure")
                ),
            )
        error_pattern = (
            "orphan|evidence|authority"
            if authority_sealing_fails
            else "launch|process|lifecycle|supervisor"
        )
        with pytest.raises(BatchControllerError, match=error_pattern) as caught:
            batch_controller.run_candidate_pair(
                plan,
                bindings,
                launches,
                active_root,
                seed=seed,
            )

    attempt_ids = {
        "stock": stock_attempt_id,
        "modified": modified_attempt_id,
    }
    launch_digests = {
        "stock": launch_spec_identity(frozen_launches.stock)[1],
        "modified": launch_spec_identity(frozen_launches.modified)[1],
    }
    return {
        "attemptIds": attempt_ids,
        "error": str(caught.value),
        "launchDigests": launch_digests,
        "order": order,
        "pairId": pair_id,
        "privateRoot": active_root,
        "processes": processes,
    }


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


def test_orphan_authority_name_is_exact_and_rejects_unbound_identifiers() -> None:
    assert orphan_authority_name("a" * 64, "b" * 64) == (
        "orphan-" + "a" * 64 + "-" + "b" * 64 + ".json"
    )
    with pytest.raises(OrphanAuthorityError):
        orphan_authority_name("A" * 64, "b" * 64)
    with pytest.raises(OrphanAuthorityError):
        orphan_authority_name("a" * 64, "../attempt")


def test_guard_orphan_seals_authenticated_per_attempt_authority(
    world: World, monkeypatch: pytest.MonkeyPatch
) -> None:
    result = _run_post_popen_failure(
        world,
        monkeypatch,
        seed=b"l" * 32,
        owned_failures=1,
        report_unconfirmed=True,
    )
    processes = result["processes"]
    try:
        assert len(processes) == 2
        first_arm = result["order"][0]
        pair_id = result["pairId"]
        attempt_id = result["attemptIds"][first_arm]
        record_path = world.private_root / (f"orphan-{pair_id}-{attempt_id}.json")
        record = verify_orphan_authority(world.private_root, pair_id, attempt_id)
        assert record == json.loads(record_path.read_bytes())
        assert record == {
            "armClass": first_arm,
            "attemptId": attempt_id,
            "errorType": "RuntimeError",
            "launchSpecSha256": result["launchDigests"][first_arm],
            "orphanSha256": record["orphanSha256"],
            "pairId": pair_id,
            "processGroupId": processes[0].pid,
            "processId": processes[0].pid,
            "recordedAt": record["recordedAt"],
            "schemaVersion": 1,
            "status": "orphaned",
            "stopDisposition": "unconfirmed",
        }
        assert record["status"] == "orphaned"
        assert record["stopDisposition"] == "unconfirmed"
        assert len(list(world.private_root.glob("orphan-*.json"))) == 1
        assert not list(world.private_root.glob("pairs/*/receipt.json"))
        assert len(list(world.private_root.glob("pairs/*/failure.json"))) == 1
        assert len(list(world.private_root.glob("attempts/*/failure.json"))) == 2
        assert len(list(Path(world.bindings["attemptBase"]).iterdir())) == 2
    finally:
        _kill_test_processes(processes)


def test_two_guard_orphans_seal_distinct_independently_verified_authorities(
    world: World, monkeypatch: pytest.MonkeyPatch
) -> None:
    result = _run_post_popen_failure(
        world,
        monkeypatch,
        seed=b"m" * 32,
        owned_failures=2,
        report_unconfirmed=True,
    )
    processes = result["processes"]
    try:
        assert len(processes) == 2
        records = []
        for arm in ("stock", "modified"):
            attempt_id = result["attemptIds"][arm]
            record = verify_orphan_authority(
                world.private_root, result["pairId"], attempt_id
            )
            records.append(record)
            assert record["attemptId"] == attempt_id
            assert record["armClass"] == arm
            assert record["launchSpecSha256"] == result["launchDigests"][arm]
            assert record["processId"] in {process.pid for process in processes}
        assert records[0]["processId"] != records[1]["processId"]
        assert len(list(world.private_root.glob("orphan-*.json"))) == 2
        assert not list(world.private_root.glob("pairs/*/receipt.json"))
        assert len(list(world.private_root.glob("attempts/*/failure.json"))) == 2
        assert len(list(Path(world.bindings["attemptBase"]).iterdir())) == 2
    finally:
        _kill_test_processes(processes)


def test_confirmed_post_popen_stop_has_no_orphan_authority_and_cleans_cells(
    world: World, monkeypatch: pytest.MonkeyPatch
) -> None:
    result = _run_post_popen_failure(
        world,
        monkeypatch,
        seed=b"n" * 32,
        owned_failures=2,
        report_unconfirmed=False,
    )
    processes = result["processes"]
    try:
        assert len(processes) == 2
        assert not list(world.private_root.glob("orphan-*.json"))
        assert not list(Path(world.bindings["attemptBase"]).iterdir())
        assert not list(world.private_root.glob("pairs/*/receipt.json"))
    finally:
        _kill_test_processes(processes)


def test_pre_popen_failure_has_no_process_or_orphan_authority_and_cleans_cells(
    world: World, monkeypatch: pytest.MonkeyPatch
) -> None:
    plan, bindings = _with_timeout(world, 1)
    launches = _launches(World(plan, bindings, world.private_root), _BOUND_CHILD)
    orphan_snapshots: list[tuple[OrphanedProcess, ...]] = []
    real_abort = batch_controller_lifecycle.PairLifecycle.abort

    def fail_before_process(*args, **kwargs):
        raise OSError("injected pre-Popen failure")

    def capture_abort(lifecycle, error):
        try:
            return real_abort(lifecycle, error)
        finally:
            orphan_snapshots.append(lifecycle.orphaned_processes)

    monkeypatch.setattr(batch_controller._process, "_Popen", fail_before_process)
    monkeypatch.setattr(
        batch_controller_lifecycle.PairLifecycle, "abort", capture_abort
    )

    with pytest.raises(BatchControllerError, match="launch|process|lifecycle"):
        batch_controller.run_candidate_pair(
            plan,
            bindings,
            launches,
            world.private_root,
            seed=b"o" * 32,
        )

    assert orphan_snapshots == [()]
    assert not list(world.private_root.glob("orphan-*.json"))
    assert not list(Path(bindings["attemptBase"]).iterdir())
    assert not list(world.private_root.glob("attempts/*/launch-record.json"))


@pytest.mark.parametrize(
    ("field", "replacement"),
    [
        ("processId", 1),
        ("processGroupId", 1),
        ("pairId", "f" * 64),
        ("attemptId", "e" * 64),
        ("launchSpecSha256", "d" * 64),
        ("status", "stopped"),
        ("stopDisposition", "confirmed"),
        ("armClass", "modified"),
        ("orphanSha256", "0" * 64),
    ],
)
def test_orphan_authority_verifier_rejects_tamper(
    world: World,
    monkeypatch: pytest.MonkeyPatch,
    field: str,
    replacement: object,
) -> None:
    result = _run_post_popen_failure(
        world,
        monkeypatch,
        seed=b"p" * 32,
        owned_failures=1,
        report_unconfirmed=True,
    )
    processes = result["processes"]
    try:
        arm = result["order"][0]
        attempt_id = result["attemptIds"][arm]
        record_path = world.private_root / (
            f"orphan-{result['pairId']}-{attempt_id}.json"
        )
        record = json.loads(record_path.read_bytes())
        if field == "armClass" and record[field] == replacement:
            replacement = "stock"
        if field in {"processId", "processGroupId"} and record[field] == replacement:
            replacement = 2
        record[field] = replacement
        record_path.write_bytes(canonical_json_bytes(record) + b"\n")

        with pytest.raises(OrphanAuthorityError):
            verify_orphan_authority(world.private_root, result["pairId"], attempt_id)
    finally:
        _kill_test_processes(processes)


def test_orphan_authority_verifier_rejects_attempt_filename_replay(
    world: World, monkeypatch: pytest.MonkeyPatch
) -> None:
    result = _run_post_popen_failure(
        world,
        monkeypatch,
        seed=b"q" * 32,
        owned_failures=1,
        report_unconfirmed=True,
    )
    processes = result["processes"]
    try:
        orphan_arm = result["order"][0]
        other_arm = result["order"][1]
        orphan_attempt = result["attemptIds"][orphan_arm]
        other_attempt = result["attemptIds"][other_arm]
        source = world.private_root / (
            f"orphan-{result['pairId']}-{orphan_attempt}.json"
        )
        replay = world.private_root / (
            f"orphan-{result['pairId']}-{other_attempt}.json"
        )
        shutil.copyfile(source, replay)

        with pytest.raises(OrphanAuthorityError):
            verify_orphan_authority(world.private_root, result["pairId"], other_attempt)
    finally:
        _kill_test_processes(processes)


def test_orphan_authority_verifier_rejects_cross_pair_replay(
    world: World, monkeypatch: pytest.MonkeyPatch
) -> None:
    first = _run_post_popen_failure(
        world,
        monkeypatch,
        seed=b"r" * 32,
        owned_failures=1,
        report_unconfirmed=True,
    )
    other_root = world.private_root.parent / "private-replay"
    second = _run_post_popen_failure(
        world,
        monkeypatch,
        seed=b"s" * 32,
        owned_failures=1,
        report_unconfirmed=True,
        private_root=other_root,
    )
    processes = first["processes"] + second["processes"]
    try:
        first_arm = first["order"][0]
        second_arm = second["order"][0]
        source = world.private_root / (
            f"orphan-{first['pairId']}-{first['attemptIds'][first_arm]}.json"
        )
        target = other_root / (
            f"orphan-{second['pairId']}-{second['attemptIds'][second_arm]}.json"
        )
        target.write_bytes(source.read_bytes())

        with pytest.raises(OrphanAuthorityError):
            verify_orphan_authority(
                other_root,
                second["pairId"],
                second["attemptIds"][second_arm],
            )
    finally:
        _kill_test_processes(processes)


@pytest.mark.parametrize("replacement", ["root", "pair"])
def test_orphan_authority_rejects_replaced_descriptor_identity(
    world: World,
    monkeypatch: pytest.MonkeyPatch,
    replacement: str,
) -> None:
    result = _run_post_popen_failure(
        world,
        monkeypatch,
        seed=b"t" * 32,
        owned_failures=1,
        report_unconfirmed=True,
    )
    processes = result["processes"]
    pair_id = result["pairId"]
    arm = result["order"][0]
    attempt_id = result["attemptIds"][arm]
    try:
        if replacement == "root":
            evidence = OfflineEvidence(world.private_root, pair_id)
            moved_root = world.private_root.with_name("private-original")
            world.private_root.rename(moved_root)
            world.private_root.mkdir(mode=0o700)
            try:
                with pytest.raises(SecureStorageError, match="identity"):
                    evidence.load_root(f"orphan-{pair_id}-{attempt_id}.json")
            finally:
                evidence.close()
                world.private_root.rmdir()
                moved_root.rename(world.private_root)
        else:
            pair_path = world.private_root / "pairs" / pair_id
            moved_pair = pair_path.with_name(pair_id + ".original")
            pair_path.rename(moved_pair)
            shutil.copytree(moved_pair, pair_path)
            pair_path.chmod(0o700)
            with pytest.raises(OrphanAuthorityError):
                verify_orphan_authority(world.private_root, pair_id, attempt_id)
    finally:
        _kill_test_processes(processes)


def test_orphan_authority_sealing_failure_retains_cells_and_blocks_receipt(
    world: World, monkeypatch: pytest.MonkeyPatch
) -> None:
    result = _run_post_popen_failure(
        world,
        monkeypatch,
        seed=b"u" * 32,
        owned_failures=1,
        report_unconfirmed=True,
        authority_sealing_fails=True,
    )
    processes = result["processes"]
    try:
        assert "orphan" in result["error"]
        assert "authority" in result["error"] or "evidence" in result["error"]
        assert len(list(Path(world.bindings["attemptBase"]).iterdir())) == 2
        assert not list(world.private_root.glob("pairs/*/receipt.json"))
        assert len(list(world.private_root.glob("pairs/*/failure.json"))) == 1
        assert not list(world.private_root.glob("orphan-*.json"))
    finally:
        _kill_test_processes(processes)
