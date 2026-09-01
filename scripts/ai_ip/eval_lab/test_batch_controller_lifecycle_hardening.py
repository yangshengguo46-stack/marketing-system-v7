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

_name = orphan.orphan_authority_name
_verify = orphan.verify_orphan_authority
_Lease = leases.ProcessLifecycleLease
_LeaseError = leases.ProcessLeaseError
_State = leases.ProcessLeaseState
_StorageError = SecureStorageError


def _lease() -> _Lease:
    return _Lease("a" * 64, "stock", "b" * 64, "c" * 64)


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


def _reject(call, error=orphan.OrphanAuthorityError, match=None):
    with pytest.raises(error, match=match):
        call()


def _fail(
    world,
    monkeypatch,
    *,
    seed: bytes,
    fails: int = 1,
    lost: bool = True,
    root: Path | None = None,
    pre: bool = False,
    promote: bool = False,
    sealer=None,
):
    plan, bindings = r5._with_timeout(world, 1)
    root = world.private_root if root is None else root
    script = r5._BOUND_CHILD + (b"\nimport time;time.sleep(10)\n" if promote else b"")
    launches = r5._launches(World(plan, bindings, root), script)
    process_module = batch_controller._process
    real_owned = process_module.OwnedProcess
    real_terminate = supervision._terminate_process
    processes: list[object] = []
    failed: set[int] = set()
    failed_leases: list[_Lease] = []
    stops = 0

    def fail_popen(*args, **kwargs):
        raise OSError("injected pre-Popen failure")

    def owned(*args, **kwargs):
        processes.append(args[0])
        if len(failed) < fails:
            failed.add(args[0].pid)
            failed_leases.append(args[1])
            raise RuntimeError("injected OwnedProcess failure")
        return real_owned(*args, **kwargs)

    def terminate(process, group_id, deadline):
        nonlocal stops
        stops += 1
        confirmed = real_terminate(process, group_id, deadline)
        if promote and stops == 1:
            return False
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
    cells, error = Path(bindings["attemptBase"]), caught.value
    pids = tuple(process.pid for process in processes)
    records = sorted(root.glob(f"orphan-{pair}-*.json"))
    return SimpleNamespace(**locals())


class TestOrphanAuthority:
    def test_process_lease_declared_paths_and_immutable_orphan(self):
        lease = _lease()
        lease.cancel_before_start()
        assert lease.state is _State.CANCELLED_BEFORE_START
        for promote in (False, True):
            lease = _lease()
            lease.attach_raw_process(123, 123)
            if promote:
                lease.promote(object())
            lease.confirm_stopped()
            assert lease.state is _State.STOP_CONFIRMED
        lease = _lease()
        lease.attach_raw_process(123, 123)
        owned = object()
        lease.promote(owned)
        orphaned = lease.mark_orphaned(RuntimeError("unconfirmed"))
        assert orphaned == leases.OrphanedProcess(
            "a" * 64, "stock", "b" * 64, 123, 123, "c" * 64, "RuntimeError"
        )
        assert lease.mark_orphaned(ValueError("later")) == orphaned
        assert lease.state is _State.ORPHANED
        assert lease._owned_process is owned
        for name, value in (("state", _State.STOP_CONFIRMED), ("orphaned_process", None)):  # fmt: skip
            _reject(lambda: setattr(lease, name, value), AttributeError)
        assert lease.orphaned_process == orphaned
        _reject(lambda: lease.confirm_stopped(), _LeaseError, "transition")

    def test_process_lease_rejects_invalid_identity_pid_and_reversal(self):
        valid: dict[str, object] = dict(
            pair_id="a" * 64, arm_class="stock",
            attempt_id="b" * 64, launch_spec_sha256="c" * 64,
        )  # fmt: skip
        invalid = {
            "pair_id": ("A" * 64, "a" * 63, "g" * 64), "attempt_id": ("", 1),
            "launch_spec_sha256": ("c" * 65,), "arm_class": ("control", 1),
        }  # fmt: skip
        for field, values in invalid.items():
            for value in values:
                _reject(lambda: _Lease(**(valid | {field: value})), _LeaseError)
        for pid, pgid in ((0, 1), (-1, 1), (True, 1), (1, 0), (1, -1), (1, False)):
            _reject(lambda: _lease().attach_raw_process(pid, pgid), _LeaseError)
        lease = _lease()
        lease.attach_raw_process(1, 1)
        _reject(lambda: lease.attach_raw_process(2, 2), _LeaseError, "state")
        lease.promote(object())
        _reject(lambda: lease.promote(object()), _LeaseError, "state")

    def test_guard_orphan_seals_exact_verified_authority(self, world, monkeypatch):
        run = _fail(world, monkeypatch, seed=b"l" * 32)
        assert len(run.records) == 1
        path = run.records[0]
        record = _verify(run.root, run.pair, path.stem.rsplit("-", 1)[1])
        fact = run.failed_leases[0].orphaned_process
        assert record == {
            "schemaVersion": 1, "pairId": fact.pair_id, "armClass": fact.arm_class,
            "attemptId": fact.attempt_id, "processId": fact.process_id, "processGroupId": fact.process_group_id,
            "launchSpecSha256": fact.launch_spec_sha256, "status": "orphaned", "stopDisposition": "unconfirmed", "errorType": fact.error_type,
            "recordedAt": record["recordedAt"], "orphanSha256": record["orphanSha256"],
        }  # fmt: skip
        assert len(run.pids) == 2 and len(list(run.cells.iterdir())) == 2
        assert len(list(run.root.glob("attempts/*/failure.json"))) == 2
        assert list(run.root.glob("pairs/*/failure.json"))
        assert not list(run.root.glob("pairs/*/receipt.json"))
        assert not (run.root / f"orphan-{run.pair}.json").exists()

    def test_two_guard_orphans_verify_independently(self, world, monkeypatch):
        run = _fail(world, monkeypatch, seed=b"m" * 32, fails=2)
        assert len(run.records) == 2
        records = [
            _verify(run.root, run.pair, path.stem.rsplit("-", 1)[1])
            for path in run.records
        ]
        assert {record["armClass"] for record in records} == {"stock", "modified"}
        assert {record["processId"] for record in records} == set(run.pids)
        assert not (run.root / f"orphan-{run.pair}.json").exists()

    @pytest.mark.parametrize(("fails", "pre"), [(2, False), (0, True)])
    def test_no_orphan_after_stop(self, world, monkeypatch, fails, pre):
        run = _fail(
            world, monkeypatch, seed=b"n" * 32, fails=fails, lost=False, pre=pre
        )
        assert not run.records and not list(run.cells.iterdir())
        assert bool(run.pids) is not pre

    def test_promoted_orphan_survives_confirmed_abort_retry(self, world, monkeypatch):
        run = _fail(world, monkeypatch, seed=b"o" * 32, fails=0, promote=True)
        assert run.stops >= 2 and len(run.records) == 1
        path = run.records[0]
        _verify(run.root, run.pair, path.stem.rsplit("-", 1)[1])
        assert len(list(run.cells.iterdir())) == 2
        assert not list(run.root.glob("pairs/*/receipt.json"))

    def test_orphan_tamper_stale_and_recommitted(self, world, monkeypatch):
        run = _fail(world, monkeypatch, seed=b"p" * 32)
        path = run.records[0]
        valid = json.loads(path.read_bytes())
        attempt = valid["attemptId"]
        stale = {"processId": 1, "orphanSha256": "f" * 64}
        semantic = {
            "armClass": ("control",), "launchSpecSha256": ("d" * 64,),
            "schemaVersion": (2, "1"), "status": ("stopped",),
            "stopDisposition": ("confirmed",), "recordedAt": ("invalid",),
            "errorType": ("", None), "processId": (0, False),
            "processGroupId": (0, False), "pairId": ("f" * 64,),
            "attemptId": ("e" * 64,), "extraField": ("extra",),
        }  # fmt: skip
        cases = [(False, field, value) for field, value in stale.items()]
        cases += [(True, field, value) for field, values in semantic.items() for value in values]  # fmt: skip
        for recommit, field, value in cases:
            changed = dict(valid)
            changed.pop(field) if value is None else changed.__setitem__(field, value)
            _write_record(path, changed, recommit=recommit)
            _reject(lambda: _verify(run.root, run.pair, attempt))

    @pytest.mark.parametrize("pair_state", ["missing-failure", "receipt"])
    def test_orphan_pair_terminal_state(self, world, monkeypatch, pair_state):
        run = _fail(world, monkeypatch, seed=b"q" * 32)
        record = json.loads(run.records[0].read_bytes())
        pair = run.root / "pairs" / run.pair
        (pair / "failure.json").unlink() if pair_state == "missing-failure" else (
            pair / "receipt.json"
        ).write_bytes(b"{}\n")
        _reject(lambda: _verify(run.root, run.pair, record["attemptId"]))

    def test_orphan_replay_attempt_and_pair(self, world, monkeypatch):
        first = _fail(world, monkeypatch, seed=b"r" * 32)
        source = first.records[0]
        record = json.loads(source.read_bytes())
        attempts = (first.root / "attempts").iterdir()
        other_attempt = next(
            path.name for path in attempts if path.name != record["attemptId"]
        )
        target = first.root / _name(first.pair, other_attempt)
        shutil.copyfile(source, target)
        _reject(lambda: _verify(first.root, first.pair, other_attempt))
        replay_root = world.private_root.parent / "private-replay"
        second = _fail(world, monkeypatch, seed=b"s" * 32, root=replay_root)
        second_attempt = json.loads(second.records[0].read_bytes())["attemptId"]
        second.records[0].write_bytes(source.read_bytes())
        _reject(lambda: _verify(second.root, second.pair, second_attempt))

    @pytest.mark.parametrize("bad", ["A" * 64, "a" * 63, "g" * 64, "../attempt"])
    def test_orphan_filename_boundaries(self, bad):
        pair, attempt = "a" * 64, "b" * 64
        assert _name(pair, attempt) == f"orphan-{pair}-{attempt}.json"
        for pair_id, attempt_id in ((bad, attempt), (pair, bad)):
            _reject(lambda: _name(pair_id, attempt_id))

    @pytest.mark.parametrize("replacement", ["root", "pair"])
    def test_orphan_descriptor_identity(self, world, monkeypatch, replacement):
        run = _fail(world, monkeypatch, seed=b"t" * 32)
        record = json.loads(run.records[0].read_bytes())
        if replacement == "root":
            evidence = OfflineEvidence(run.root, run.pair)
            moved = run.root.with_name("private-original")
            run.root.rename(moved)
            run.root.mkdir(mode=0o700)
            name = run.records[0].name
            try:
                _reject(lambda: evidence.load_root(name), _StorageError, "identity")
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
            _reject(lambda: _verify(run.root, run.pair, record["attemptId"]))

    def test_orphan_sealer_attempts_all_after_error(self, world, monkeypatch):
        real_sealer = lifecycle.seal_orphan_authority
        first_error = OSError("first authority write failure")
        attempts: list[str] = []

        def selective(root, orphan):
            attempts.append(orphan.attempt_id)
            if len(attempts) == 1:
                raise first_error
            return real_sealer(root, orphan, recorded_at="2026-09-01T00:00:00Z")

        run = _fail(world, monkeypatch, seed=b"u" * 32, fails=2, sealer=selective)
        assert len(attempts) == 2 and len(set(attempts)) == 2 and len(run.records) == 1
        assert run.error.__cause__.__cause__ is first_error
        assert "authority" in str(run.error)
        assert len(list(run.cells.iterdir())) == 2
        assert not list(run.root.glob("pairs/*/receipt.json"))

    def test_legacy_orphan_name_is_disjoint(self, world, monkeypatch):
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
