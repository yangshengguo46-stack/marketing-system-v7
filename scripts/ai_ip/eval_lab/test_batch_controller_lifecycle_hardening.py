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

_name, _verify = orphan.orphan_authority_name, orphan.verify_orphan_authority
_Lease, _LeaseError, _State = leases.ProcessLifecycleLease, leases.ProcessLeaseError, leases.ProcessLeaseState  # fmt: skip
_StorageError = SecureStorageError
_JSON = dict(ensure_ascii=False, allow_nan=False, sort_keys=True, separators=(",", ":"))


def _lease() -> _Lease:
    return _Lease("a" * 64, "stock", "b" * 64, "c" * 64)


def _attached(owned=None) -> _Lease:
    lease = _lease()
    lease.attach_raw_process(123, 123)
    if owned is not None:
        lease.promote(owned)
    return lease


def _canonical(value: object) -> bytes:
    return json.dumps(value, **_JSON).encode()


def _write_record(path: Path, record: dict[str, object], *, recommit: bool) -> None:
    if recommit:
        semantic = dict(record)
        semantic.pop("orphanSha256")
        record["orphanSha256"] = hashlib.sha256(_canonical(semantic)).hexdigest()
    path.write_bytes(_canonical(record) + b"\n")


def _reject(call, error=orphan.OrphanAuthorityError, match=None):
    with pytest.raises(error, match=match):
        call()


def _fail(world, monkeypatch, *, seed: bytes, **changes):
    options = SimpleNamespace(**(dict(fails=1, lost=True, root=None, pre=False, promote=False, sealer=None, tombstone=None, stop_raises=False) | changes))  # fmt: skip
    plan, bindings = r5._with_timeout(world, 1)
    root = world.private_root if options.root is None else options.root
    script = r5._BOUND_CHILD + (b"\nimport time;time.sleep(10)\n" if options.promote else b"")  # fmt: skip
    launches = r5._launches(World(plan, bindings, root), script)
    process_module = batch_controller._process
    real_owned, real_terminate = (
        process_module.OwnedProcess,
        supervision._terminate_process,
    )
    processes, owned_processes = [], []
    failed: set[int] = set()
    failed_leases: list[_Lease] = []
    stops = 0

    def fail_popen(*args, **kwargs):
        raise OSError("injected pre-Popen failure")

    def owned(*args, **kwargs):
        processes.append(args[0])
        if len(failed) < options.fails:
            failed.add(args[0].pid)
            failed_leases.append(args[1])
            raise RuntimeError("injected OwnedProcess failure")
        result = real_owned(*args, **kwargs)
        owned_processes.append(result)
        return result

    def terminate(process, group_id, deadline):
        nonlocal stops
        stops += 1
        confirmed = real_terminate(process, group_id, deadline)
        if options.stop_raises and process.pid in failed:
            raise OSError("stop failed after real kill and wait")
        if options.promote and stops == 1:
            return False
        return False if options.lost and process.pid in failed else confirmed

    with monkeypatch.context() as patch:
        if options.pre:
            patch.setattr(process_module, "_Popen", fail_popen)
        patch.setattr(process_module, "OwnedProcess", owned)
        patch.setattr(supervision, "_terminate_process", terminate)
        if options.sealer is not None:
            patch.setattr(lifecycle, "seal_orphan_authority", options.sealer)
        if options.tombstone is not None:
            patch.setattr(lifecycle, "seal_failure_tombstone", options.tombstone)
        try:
            with pytest.raises(BatchControllerError) as caught:
                batch_controller.run_candidate_pair(plan, bindings, launches, root, seed=seed)  # fmt: skip
        finally:
            r5._kill_test_processes(processes)
    pair, cells, error = next((root / "pairs").iterdir()).name, Path(bindings["attemptBase"]), caught.value  # fmt: skip
    pids, records = tuple(process.pid for process in processes), sorted(root.glob(f"orphan-{pair}-*.json"))  # fmt: skip
    return SimpleNamespace(**locals())


class TestOrphanAuthority:
    def test_process_lease_declared_paths_and_immutable_orphan(self):
        lease = _lease(); lease.cancel_before_start()  # fmt: skip
        assert lease.state is _State.CANCELLED_BEFORE_START
        for promote in (False, True):
            lease = _attached(object() if promote else None); lease.confirm_stopped()  # fmt: skip
            assert lease.state is _State.STOP_CONFIRMED
        owned = object(); lease = _attached(owned)  # fmt: skip
        orphaned = lease.mark_orphaned(RuntimeError("unconfirmed"))
        assert orphaned == leases.OrphanedProcess(
            "a" * 64, "stock", "b" * 64, 123, 123, "c" * 64, "RuntimeError"
        )
        assert (lease.mark_orphaned(ValueError("later")), lease.state, lease._owned_process) == (orphaned, _State.ORPHANED, owned)  # fmt: skip
        assignments = {
            "pair_id": "d" * 64, "arm_class": "modified",
            "attempt_id": "e" * 64, "launch_spec_sha256": "f" * 64,
            "state": _State.STOP_CONFIRMED, "orphaned_process": None,
        }  # fmt: skip
        for name, value in assignments.items():
            _reject(lambda: setattr(lease, name, value), AttributeError)
        assert lease.orphaned_process == orphaned; _reject(lambda: lease.confirm_stopped(), _LeaseError, "transition")  # fmt: skip

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
        lease = _lease(); lease.attach_raw_process(1, 1); _reject(lambda: lease.attach_raw_process(2, 2), _LeaseError, "state")  # fmt: skip
        lease.promote(object())
        _reject(lambda: lease.promote(object()), _LeaseError, "state")

    @pytest.mark.parametrize("mode", ["single", "raised-stop", "double", "confirmed", "pre", "promoted"])  # fmt: skip
    def test_process_orphan_outcomes(self, world, monkeypatch, mode):
        options = {
            "single": {}, "raised-stop": {"stop_raises": True},
            "double": {"fails": 2}, "confirmed": {"fails": 2, "lost": False},
            "pre": {"fails": 0, "lost": False, "pre": True},
            "promoted": {"fails": 0, "promote": True},
        }[mode]  # fmt: skip
        run = _fail(world, monkeypatch, seed=b"l" * 32, **options)
        if mode in {"confirmed", "pre"}:
            assert (bool(run.records), bool(list(run.cells.iterdir())), bool(run.pids)) == (False, False, mode == "confirmed")  # fmt: skip
            return
        assert (len(list(run.cells.iterdir())), bool(list(run.root.glob("pairs/*/receipt.json"))), (run.root / f"orphan-{run.pair}.json").exists()) == (2, False, False)  # fmt: skip
        records = [_verify(run.root, run.pair, path.stem.rsplit("-", 1)[1]) for path in run.records]  # fmt: skip
        assert len(records) == (2 if mode == "double" else 1)
        if mode == "double":
            assert ({item["armClass"] for item in records}, {item["processId"] for item in records}) == ({"stock", "modified"}, set(run.pids))  # fmt: skip
            return
        if mode == "promoted":
            assert run.stops >= 2
            return
        fact, record = run.failed_leases[0].orphaned_process, records[0]
        assert record == {
            "schemaVersion": 1, "pairId": fact.pair_id, "armClass": fact.arm_class,
            "attemptId": fact.attempt_id, "processId": fact.process_id, "processGroupId": fact.process_group_id,
            "launchSpecSha256": fact.launch_spec_sha256, "status": "orphaned", "stopDisposition": "unconfirmed", "errorType": fact.error_type,
            "recordedAt": record["recordedAt"], "orphanSha256": record["orphanSha256"],
        }  # fmt: skip
        assert (len(run.pids), run.failed_leases[0].state, len(list(run.root.glob("attempts/*/failure.json")))) == (2, _State.ORPHANED, 2)  # fmt: skip
        assert list(run.root.glob("pairs/*/failure.json")) and run.owned_processes
        _reject(lambda: setattr(run.owned_processes[0], "lease", _lease()), AttributeError)  # fmt: skip

    def test_orphan_tamper_stale_and_recommitted(self, world, monkeypatch):
        run = _fail(world, monkeypatch, seed=b"p" * 32); path = run.records[0]  # fmt: skip
        valid = json.loads(path.read_bytes()); attempt = valid["attemptId"]  # fmt: skip
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
            changed.pop(field) if value is None else changed.__setitem__(field, value); _write_record(path, changed, recommit=recommit)  # fmt: skip
            _reject(lambda: _verify(run.root, run.pair, attempt))

    @pytest.mark.parametrize("pair_state", ["missing-failure", "receipt"])
    def test_orphan_pair_terminal_state(self, world, monkeypatch, pair_state):
        run = _fail(world, monkeypatch, seed=b"q" * 32); record = json.loads(run.records[0].read_bytes())  # fmt: skip
        pair = run.root / "pairs" / run.pair
        (pair / "failure.json").unlink() if pair_state == "missing-failure" else (pair / "receipt.json").write_bytes(b"{}\n")  # fmt: skip
        _reject(lambda: _verify(run.root, run.pair, record["attemptId"]))

    def test_orphan_replay_attempt_and_pair(self, world, monkeypatch):
        first = _fail(world, monkeypatch, seed=b"r" * 32); source = first.records[0]  # fmt: skip
        record, attempts = json.loads(source.read_bytes()), (first.root / "attempts").iterdir()  # fmt: skip
        other_attempt = next(path.name for path in attempts if path.name != record["attemptId"])  # fmt: skip
        target = first.root / _name(first.pair, other_attempt); shutil.copyfile(source, target)  # fmt: skip
        _reject(lambda: _verify(first.root, first.pair, other_attempt))
        replay_root = world.private_root.parent / "private-replay"; second = _fail(world, monkeypatch, seed=b"s" * 32, root=replay_root)  # fmt: skip
        second_attempt = json.loads(second.records[0].read_bytes())["attemptId"]
        second.records[0].write_bytes(source.read_bytes()); _reject(lambda: _verify(second.root, second.pair, second_attempt))  # fmt: skip

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
            evidence, moved = OfflineEvidence(run.root, run.pair), run.root.with_name("private-original")  # fmt: skip
            run.root.rename(moved); run.root.mkdir(mode=0o700)  # fmt: skip
            name = run.records[0].name
            try:
                _reject(lambda: evidence.load_root(name), _StorageError, "identity")
            finally:
                evidence.close(); run.root.rmdir(); moved.rename(run.root)  # fmt: skip
        else:
            pair = run.root / "pairs" / run.pair; moved = pair.with_name(pair.name + ".original")  # fmt: skip
            pair.rename(moved); shutil.copytree(moved, pair); pair.chmod(0o700)  # fmt: skip
            _reject(lambda: _verify(run.root, run.pair, record["attemptId"]))

    @pytest.mark.parametrize("failure", ["authority", "tombstone"])
    def test_orphan_evidence_attempts_all_and_keeps_first_error(
        self, world, monkeypatch, failure
    ):
        real_sealer, real_tombstone = lifecycle.seal_orphan_authority, lifecycle.seal_failure_tombstone  # fmt: skip
        first_error, attempts = OSError(f"first {failure} evidence failure"), []

        def selective(root, orphan):
            attempts.append(orphan.attempt_id)
            if failure == "authority" and len(attempts) == 1:
                raise first_error
            return real_sealer(root, orphan, recorded_at="2026-09-01T00:00:00Z")

        def tombstone(directory, *args):
            if failure == "tombstone" and directory.path.parent.name == "pairs":
                raise first_error
            return real_tombstone(directory, *args)

        run = _fail(world, monkeypatch, seed=b"u" * 32, fails=2, tombstone=tombstone, sealer=selective)  # fmt: skip
        assert (len(attempts), len(set(attempts)), len(run.records)) == (2, 2, 1 if failure == "authority" else 2)  # fmt: skip
        assert run.error.__cause__.__cause__ is first_error
        assert "evidence" in str(run.error) and len(list(run.cells.iterdir())) == 2
        assert not list(run.root.glob("pairs/*/receipt.json"))

    def test_abort_rejects_foreign_lease_without_erasing_orphan(self, world, monkeypatch):  # fmt: skip
        pair_lifecycle = PairLifecycle(world.private_root, "a" * 64)
        expected = pair_lifecycle.reserve_process("stock", "b" * 64, "c" * 64); expected.attach_raw_process(123, 123)  # fmt: skip
        process, foreign = SimpleNamespace(lease=expected), _lease()
        pair_lifecycle.promote_process(expected, process); process.lease = foreign  # fmt: skip
        pair_lifecycle.layout = SimpleNamespace(pair_directory=object(), attempt_directories=(), root_directory=object())  # fmt: skip
        cleaned: list[object] = []
        monkeypatch.setattr(PairLifecycle, "_tombstone", lambda *args: None)
        monkeypatch.setattr(lifecycle, "seal_orphan_authority", lambda *args: None)
        monkeypatch.setattr(batch_controller._process, "terminate_and_wait", lambda *args: False)  # fmt: skip
        monkeypatch.setattr(batch_controller._process, "close_owned", lambda *args: None)  # fmt: skip
        monkeypatch.setattr(lifecycle, "cleanup_attempt_cell", cleaned.append)
        with pytest.raises(PairLifecycleError, match="authority") as caught:
            pair_lifecycle.abort(RuntimeError("outer failure"))
        assert "binding" in str(caught.value.__cause__)
        assert (expected.state, foreign.state, cleaned) == (_State.ORPHANED, _State.RESERVED, [])  # fmt: skip

    def test_legacy_orphan_name_is_disjoint(self, world, monkeypatch):
        world.private_root.mkdir(mode=0o700); pair = "a" * 64  # fmt: skip
        pair_lifecycle = PairLifecycle(world.private_root, pair); pair_lifecycle.bind_cell(SimpleNamespace(attempt_id="b" * 64))  # fmt: skip
        monkeypatch.setattr(lifecycle, "cleanup_attempt_cell", lambda cell: (_ for _ in ()).throw(OSError("cleanup failed")))  # fmt: skip
        with pytest.raises(PairLifecycleError, match="cleanup"):
            pair_lifecycle.complete()
        assert (world.private_root / f"orphan-{pair}.json").is_file()
        assert not list(world.private_root.glob(f"orphan-{pair}-*.json"))
