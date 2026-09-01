import hashlib
import json
import shutil
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest


sys.path.insert(0, str(Path(__file__).resolve().parent))
import batch_controller  # noqa: E402
import batch_controller_lifecycle as lifecycle  # noqa: E402
import batch_controller_process_lease as leases  # noqa: E402
import batch_controller_supervision as supervision  # noqa: E402
import batch_orphan_authority as orphan  # noqa: E402
import test_batch_controller_review_round5 as r5  # noqa: E402
from batch_controller import BatchControllerError  # noqa: E402
from batch_controller_lifecycle import PairLifecycle, PairLifecycleError  # noqa: E402
from batch_receipt_offline import OfflineEvidence  # noqa: E402
from batch_receipt_storage import SecureStorageError  # noqa: E402
from test_batch_controller import World, world  # noqa: E402


def _lease() -> leases.ProcessLifecycleLease:
    return leases.ProcessLifecycleLease("a" * 64, "stock", "b" * 64, "c" * 64)


def _canonical(value: object) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        allow_nan=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode()


def _write_record(path: Path, record: dict[str, object], *, recommit: bool) -> None:
    if recommit:
        semantic = dict(record)
        semantic.pop("orphanSha256")
        record["orphanSha256"] = hashlib.sha256(_canonical(semantic)).hexdigest()
    path.write_bytes(_canonical(record) + b"\n")


def _fail(
    world,
    monkeypatch,
    *,
    seed: bytes,
    failures: int = 1,
    lost: bool = True,
    root: Path | None = None,
    pre: bool = False,
    sealer=None,
):
    plan, bindings = r5._with_timeout(world, 1)
    root = world.private_root if root is None else root
    launches = r5._launches(World(plan, bindings, root), r5._BOUND_CHILD)
    process_module = batch_controller._process
    real_owned = process_module.OwnedProcess
    real_terminate = supervision._terminate_process
    processes: list[object] = []
    failed: set[int] = set()

    def fail_popen(*args, **kwargs):
        raise OSError("injected pre-Popen failure")

    def owned(*args, **kwargs):
        processes.append(args[0])
        if len(failed) < failures:
            failed.add(args[0].pid)
            raise RuntimeError("injected OwnedProcess failure")
        return real_owned(*args, **kwargs)

    def terminate(process, group_id, deadline):
        confirmed = real_terminate(process, group_id, deadline)
        return False if lost and process.pid in failed else confirmed

    with monkeypatch.context() as patch:
        if pre:
            patch.setattr(process_module, "_Popen", fail_popen)
        patch.setattr(process_module, "OwnedProcess", owned)
        patch.setattr(supervision, "_terminate_process", terminate)
        if sealer is not None:
            patch.setattr(lifecycle, "seal_orphan_authority", sealer)
        try:
            with pytest.raises(BatchControllerError) as caught:
                batch_controller.run_candidate_pair(
                    plan, bindings, launches, root, seed=seed
                )
        finally:
            r5._kill_test_processes(processes)
    pair = next((root / "pairs").iterdir()).name
    return SimpleNamespace(
        cells=Path(bindings["attemptBase"]),
        error=caught.value,
        pair=pair,
        pids=tuple(process.pid for process in processes),
        records=sorted(root.glob(f"orphan-{pair}-*.json")),
        root=root,
    )


def test_process_lease_declared_paths_and_immutable_orphan():
    lease = _lease()
    lease.cancel_before_start()
    assert lease.state is leases.ProcessLeaseState.CANCELLED_BEFORE_START
    for promote in (False, True):
        lease = _lease()
        lease.attach_raw_process(123, 123)
        if promote:
            lease.promote(object())
        lease.confirm_stopped()
        assert lease.state is leases.ProcessLeaseState.STOP_CONFIRMED
    lease = _lease()
    lease.attach_raw_process(123, 123)
    lease.promote(object())
    orphaned = lease.mark_orphaned(RuntimeError("unconfirmed"))
    assert orphaned == leases.OrphanedProcess(
        "a" * 64, "stock", "b" * 64, 123, 123, "c" * 64, "RuntimeError"
    )
    assert lease.mark_orphaned(ValueError("later")) == orphaned
    assert lease.state is leases.ProcessLeaseState.ORPHANED
    with pytest.raises(leases.ProcessLeaseError, match="transition"):
        lease.confirm_stopped()


def test_process_lease_rejects_invalid_identity_pid_and_reversal():
    valid: dict[str, object] = dict(
        pair_id="a" * 64,
        arm_class="stock",
        attempt_id="b" * 64,
        launch_spec_sha256="c" * 64,
    )
    invalid = {
        "pair_id": ("A" * 64, "a" * 63, "g" * 64),
        "attempt_id": ("", 1),
        "launch_spec_sha256": ("c" * 65,),
        "arm_class": ("control", 1),
    }
    for field, values in invalid.items():
        for value in values:
            with pytest.raises(leases.ProcessLeaseError):
                leases.ProcessLifecycleLease(**(valid | {field: value}))
    for pid, pgid in ((0, 1), (-1, 1), (True, 1), (1, 0), (1, -1), (1, False)):
        with pytest.raises(leases.ProcessLeaseError):
            _lease().attach_raw_process(pid, pgid)
    lease = _lease()
    lease.attach_raw_process(1, 1)
    with pytest.raises(leases.ProcessLeaseError, match="state"):
        lease.attach_raw_process(2, 2)
    lease.promote(object())
    with pytest.raises(leases.ProcessLeaseError, match="state"):
        lease.promote(object())


def test_guard_orphan_seals_exact_verified_authority(world, monkeypatch):
    run = _fail(world, monkeypatch, seed=b"l" * 32)
    path = run.records[0]
    record = orphan.verify_orphan_authority(
        run.root, run.pair, path.stem.rsplit("-", 1)[1]
    )
    assert record == json.loads(path.read_bytes())
    assert (record["status"], record["stopDisposition"]) == ("orphaned", "unconfirmed")
    assert (record["processId"], record["processGroupId"]) == (run.pids[0],) * 2
    assert len(run.pids) == 2 and len(list(run.cells.iterdir())) == 2
    assert len(list(run.root.glob("attempts/*/failure.json"))) == 2
    assert list(run.root.glob("pairs/*/failure.json"))
    assert not list(run.root.glob("pairs/*/receipt.json"))
    assert not (run.root / f"orphan-{run.pair}.json").exists()


def test_two_guard_orphans_verify_independently(world, monkeypatch):
    run = _fail(world, monkeypatch, seed=b"m" * 32, failures=2)
    records = [
        orphan.verify_orphan_authority(run.root, run.pair, path.stem.rsplit("-", 1)[1])
        for path in run.records
    ]
    assert {record["armClass"] for record in records} == {"stock", "modified"}
    assert {record["processId"] for record in records} == set(run.pids)
    assert not (run.root / f"orphan-{run.pair}.json").exists()


@pytest.mark.parametrize(("failures", "pre_popen"), [(2, False), (0, True)])
def test_no_orphan_after_stop(world, monkeypatch, failures, pre_popen):
    run = _fail(
        world, monkeypatch, seed=b"n" * 32, failures=failures, lost=False, pre=pre_popen
    )
    assert not run.records and not list(run.cells.iterdir())
    assert bool(run.pids) is not pre_popen


def test_orphan_tamper_stale_and_recommitted(world, monkeypatch):
    run = _fail(world, monkeypatch, seed=b"p" * 32)
    path = run.records[0]
    valid = json.loads(path.read_bytes())
    attempt = valid["attemptId"]
    stale = dict.fromkeys("processId processGroupId".split(), 1)
    identities = "pairId attemptId launchSpecSha256 orphanSha256"
    stale.update(dict.fromkeys(identities.split(), "f" * 64))
    stale.update(status="stopped", stopDisposition="confirmed", armClass="control")
    for field, value in stale.items():
        changed = dict(valid)
        changed[field] = value
        _write_record(path, changed, recommit=False)
        with pytest.raises(orphan.OrphanAuthorityError):
            orphan.verify_orphan_authority(run.root, run.pair, attempt)
    semantic = {
        "armClass": ("control",),
        "launchSpecSha256": ("d" * 64,),
        "schemaVersion": (2, "1"),
        "status": ("stopped",),
        "stopDisposition": ("confirmed",),
        "recordedAt": ("invalid",),
        "errorType": ("", None),
        "processId": (0, False),
        "processGroupId": (0, False),
        "pairId": ("f" * 64,),
        "attemptId": ("e" * 64,),
        "extraField": ("extra",),
    }
    for field, values in semantic.items():
        for value in values:
            changed = dict(valid)
            changed.pop(field) if value is None else changed.__setitem__(field, value)
            _write_record(path, changed, recommit=True)
            with pytest.raises(orphan.OrphanAuthorityError):
                orphan.verify_orphan_authority(run.root, run.pair, attempt)


@pytest.mark.parametrize("pair_state", ["missing-failure", "receipt"])
def test_orphan_requires_failure_without_receipt(world, monkeypatch, pair_state):
    run = _fail(world, monkeypatch, seed=b"q" * 32)
    record = json.loads(run.records[0].read_bytes())
    pair = run.root / "pairs" / run.pair
    (pair / "failure.json").unlink() if pair_state == "missing-failure" else (
        pair / "receipt.json"
    ).write_bytes(b"{}\n")
    with pytest.raises(orphan.OrphanAuthorityError):
        orphan.verify_orphan_authority(run.root, run.pair, record["attemptId"])


def test_orphan_replay_attempt_and_pair(world, monkeypatch):
    first = _fail(world, monkeypatch, seed=b"r" * 32)
    source = first.records[0]
    record = json.loads(source.read_bytes())
    attempts = (first.root / "attempts").iterdir()
    other_attempt = next(
        path.name for path in attempts if path.name != record["attemptId"]
    )
    target = first.root / orphan.orphan_authority_name(first.pair, other_attempt)
    shutil.copyfile(source, target)
    with pytest.raises(orphan.OrphanAuthorityError):
        orphan.verify_orphan_authority(first.root, first.pair, other_attempt)
    replay_root = world.private_root.parent / "private-replay"
    second = _fail(world, monkeypatch, seed=b"s" * 32, root=replay_root)
    second.records[0].write_bytes(source.read_bytes())
    replay = json.loads(second.records[0].read_bytes())
    with pytest.raises(orphan.OrphanAuthorityError):
        orphan.verify_orphan_authority(second.root, second.pair, replay["attemptId"])


@pytest.mark.parametrize("replacement", ["root", "pair"])
def test_orphan_authority_descriptor_replacement(world, monkeypatch, replacement):
    run = _fail(world, monkeypatch, seed=b"t" * 32)
    record = json.loads(run.records[0].read_bytes())
    if replacement == "root":
        evidence = OfflineEvidence(run.root, run.pair)
        moved = run.root.with_name("private-original")
        run.root.rename(moved)
        run.root.mkdir(mode=0o700)
        try:
            with pytest.raises(SecureStorageError, match="identity"):
                evidence.load_root(run.records[0].name)
        finally:
            evidence.close()
            run.root.rmdir()
            moved.rename(run.root)
    else:
        pair = run.root / "pairs" / run.pair
        moved = pair.with_name(pair.name + ".original")
        pair.rename(moved)
        shutil.copytree(moved, pair)
        pair.chmod(0o700)
        with pytest.raises(orphan.OrphanAuthorityError):
            orphan.verify_orphan_authority(run.root, run.pair, record["attemptId"])


def test_orphan_sealer_attempts_all_after_error(world, monkeypatch):
    real_sealer = lifecycle.seal_orphan_authority
    first_error = OSError("first authority write failure")
    attempts: list[str] = []

    def selective(root, orphan):
        attempts.append(orphan.attempt_id)
        if len(attempts) == 1:
            raise first_error
        return real_sealer(root, orphan, recorded_at="2026-09-01T00:00:00Z")

    run = _fail(world, monkeypatch, seed=b"u" * 32, failures=2, sealer=selective)
    assert len(attempts) == 2 and len(set(attempts)) == 2 and len(run.records) == 1
    assert run.error.__cause__.__cause__ is first_error
    assert "authority" in str(run.error)
    assert len(list(run.cells.iterdir())) == 2
    assert not list(run.root.glob("pairs/*/receipt.json"))


def test_legacy_orphan_name_is_disjoint(world, monkeypatch):
    world.private_root.mkdir(mode=0o700)
    pair = "a" * 64
    pair_lifecycle = PairLifecycle(world.private_root, pair)
    pair_lifecycle.bind_cell(SimpleNamespace(attempt_id="b" * 64))
    monkeypatch.setattr(
        lifecycle,
        "cleanup_attempt_cell",
        lambda cell: (_ for _ in ()).throw(OSError("cleanup failed")),
    )
    with pytest.raises(PairLifecycleError, match="cleanup"):
        pair_lifecycle.complete()
    assert (world.private_root / f"orphan-{pair}.json").is_file()
    assert not list(world.private_root.glob(f"orphan-{pair}-*.json"))
