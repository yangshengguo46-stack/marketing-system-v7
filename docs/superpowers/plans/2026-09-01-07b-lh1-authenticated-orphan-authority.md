# 07B-LH1 Authenticated Orphan Authority Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the post-`Popen` ownership gap so an unconfirmed process stop survives exception aggregation as authenticated, offline-verifiable orphan authority and can never produce cleanup or completion.

**Architecture:** `PairLifecycle` pre-registers one `ProcessLifecycleLease` per arm before `Popen`; the launch guard attaches the raw PID/PGID immediately and either promotes the same lease to `OwnedProcess`, confirms its stop, or marks it irreversibly orphaned. A separate orphan-authority module seals one self-committed root record per orphan attempt and verifies it through descriptor-retained `OfflineEvidence`, so exception shape is diagnostic only and no longer owns lifecycle truth.

**Tech Stack:** Python 3.11, pytest 9.0.2, jsonschema 4.25.1, POSIX/macOS `subprocess`/process groups/descriptors, existing canonical JSON and `PrivateRoot` evidence primitives.

**Spec:** `docs/superpowers/specs/2026-09-01-07b-lh1-authenticated-orphan-authority-design.md`

## Global Constraints

- This is standalone `07B-LH1`, not Task 4 fix round 6; it gets its own branch, worktree, SDD ledger, implementation review, and final review.
- Task 5 remains blocked until LH1 reaches Critical 0 / Important 0 / Ready Yes.
- `PairLifecycle` is the only pair terminal-state authority; exception classes and messages are diagnostic, never lifecycle truth.
- Register each arm lease before `Popen`; attach PID/PGID immediately after `Popen` returns; no created process may exist only in a local guard after that point.
- Both arms still receive exactly one launch attempt before results are observed; retry remains zero.
- Any unconfirmed stop preserves every bound cell, seals no pair completion receipt, and cannot be converted to ordinary cleanup by exception wrapping.
- Each orphan attempt gets one exclusive, self-committed root record bound to pair ID, arm class, attempt ID, PID, PGID, and sealed launch-spec commitment.
- Orphan-record sealing or verification failure also preserves cells and fails closed; absence of evidence never proves a process stopped.
- Offline verification must authenticate `PrivateRoot`, retain descriptor identities, enforce exact fields/types/canonical bytes, and reject tamper, filename/content mismatch, cross-pair replay, cross-attempt replay, or a pair completion receipt.
- POSIX/macOS is the supported execution backend; R8 keeps Windows failed closed and the existing ten native-Windows skips are permitted.
- No network, provider, Promptfoo, App Server, paid model, media generation, business scoring, key access, or release operation belongs to LH1.
- No production module may reach 500 lines; non-mechanical total change must remain below 800 lines, with focused lifecycle logic targeted below 500 changed lines.
- Run tests before the final repository formatter. After all Python verification, run `just fmt` from `codex-rs`; do not run tests after `just fmt`.
- Use `apply_patch` for edits, preserve unrelated user changes, and keep the tracked worktree clean at each review handoff.

---

## File Structure

| File | Responsibility |
|---|---|
| `scripts/ai_ip/eval_lab/batch_controller_process_lease.py` | Private one-way process lease state machine and immutable orphan process facts |
| `scripts/ai_ip/eval_lab/batch_controller_lifecycle.py` | Pair-owned lease registry, promotion, abort/complete rules, and cell-preservation decision |
| `scripts/ai_ip/eval_lab/batch_controller_supervision.py` | Attach the raw process immediately, transition the lease on guard/supervisor stop outcomes |
| `scripts/ai_ip/eval_lab/batch_controller_process.py` | Preserve the thin facade while carrying arm and launch commitment into `spawn` |
| `scripts/ai_ip/eval_lab/batch_controller.py` | Compute each immutable launch commitment once and pass exact arm/attempt authority into spawn |
| `scripts/ai_ip/eval_lab/batch_orphan_authority.py` | Seal and offline-verify exact orphan authority records |
| `scripts/ai_ip/eval_lab/batch_contracts.py` | Admit `orphanSha256` to the existing generic self-commitment primitive |
| `scripts/ai_ip/eval_lab/batch_receipts.py` | Expose the already-retained authenticated root directory on `EvidenceLayout` |
| `scripts/ai_ip/eval_lab/batch_receipt_offline.py` | Add bounded descriptor-relative root-record loading for the verifier |
| `scripts/ai_ip/eval_lab/test_batch_controller_lifecycle_hardening.py` | Independent LH1 state, integration, authority, tamper, replay, and failure regressions |

---

### Task 1: Pre-register process lifecycle leases and close the ownership gap

**Files:**
- Create: `scripts/ai_ip/eval_lab/batch_controller_process_lease.py`
- Create: `scripts/ai_ip/eval_lab/test_batch_controller_lifecycle_hardening.py`
- Modify: `scripts/ai_ip/eval_lab/batch_controller_lifecycle.py:24-135`
- Modify: `scripts/ai_ip/eval_lab/batch_controller_supervision.py:64-224,236-382`
- Modify: `scripts/ai_ip/eval_lab/batch_controller_process.py:38-55`
- Modify: `scripts/ai_ip/eval_lab/batch_controller.py:165-285`

**Interfaces:**
- Consumes: existing `LaunchSpec`/`FrozenLaunchSpec`, `launch_spec_identity(spec) -> tuple[dict[str, object], str]`, `PreparedProcess.request.cell.attempt_id`, `OwnedProcess`, and `PairLifecycle` context ownership.
- Produces:
  - `ProcessLeaseState` values `RESERVED`, `ATTACHED_RAW_PROCESS`, `PROMOTED_OWNED_PROCESS`, `STOP_CONFIRMED`, `ORPHANED`, `CANCELLED_BEFORE_START`.
  - immutable `OrphanedProcess(pair_id, arm_class, attempt_id, process_id, process_group_id, launch_spec_sha256, error_type)`.
  - `ProcessLifecycleLease(pair_id: str, arm_class: str, attempt_id: str, launch_spec_sha256: str)`.
  - `ProcessLifecycleLease.attach_raw_process(process_id: int, process_group_id: int) -> None`.
  - `ProcessLifecycleLease.promote(owned_process: object) -> None`.
  - `ProcessLifecycleLease.cancel_before_start() -> None`.
  - `ProcessLifecycleLease.confirm_stopped() -> None`.
  - `ProcessLifecycleLease.mark_orphaned(error: BaseException) -> OrphanedProcess`.
  - `ProcessLifecycleLease.orphaned_process -> OrphanedProcess | None`.
  - `PairLifecycle.reserve_process(arm_class: str, attempt_id: str, launch_spec_sha256: str) -> ProcessLifecycleLease`.
  - `PairLifecycle.promote_process(lease: ProcessLifecycleLease, process: object) -> None`.
  - `PairLifecycle.orphaned_processes -> tuple[OrphanedProcess, ...]`.
  - `_process.spawn(prepared, limit, lifecycle, *, arm_class: str, launch_spec_sha256: str) -> OwnedProcess`.
- Task 2 consumes `PairLifecycle.orphaned_processes` and the exact immutable `OrphanedProcess` fields; do not rename or add optional placeholders.

- [ ] **Step 1: Write pure state-machine RED tests**

In `test_batch_controller_lifecycle_hardening.py`, import the existing `World`/`world` fixture and round-5 launch helpers rather than duplicating the large child program:

```python
from test_batch_controller import World, world
from test_batch_controller_review_round5 import (
    _BOUND_CHILD,
    _kill_test_processes,
    _launches,
    _with_timeout,
)
```

Add table-driven tests which name the production mutations they catch:

```python
def test_process_lease_rejects_duplicate_attach_and_terminal_reversal() -> None:
    lease = ProcessLifecycleLease(
        pair_id="a" * 64,
        arm_class="stock",
        attempt_id="b" * 64,
        launch_spec_sha256="c" * 64,
    )
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
```

Also cover `RESERVED -> CANCELLED_BEFORE_START`, `ATTACHED_RAW_PROCESS -> STOP_CONFIRMED`, `ATTACHED_RAW_PROCESS -> PROMOTED_OWNED_PROCESS -> STOP_CONFIRMED`, `ATTACHED_RAW_PROCESS -> PROMOTED_OWNED_PROCESS -> ORPHANED`, invalid arm class, non-positive PID/PGID, malformed IDs/digest, and promotion with a second object. Assertions compare full states/objects, not individual implementation fields.

- [ ] **Step 2: Run the state tests and verify RED**

Run:

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_controller_lifecycle_hardening.py \
  -k 'process_lease'
```

Expected: collection/import failure because `batch_controller_process_lease` and its interfaces do not exist. Save the exact command, expected failure reason, and output in the task report before writing production code.

- [ ] **Step 3: Implement the minimal one-way lease state machine**

Create `batch_controller_process_lease.py` with no OS calls or evidence I/O. Use a private transition check and keep every outward orphan fact immutable:

```python
class ProcessLeaseState(Enum):
    RESERVED = "reserved"
    ATTACHED_RAW_PROCESS = "attachedRawProcess"
    PROMOTED_OWNED_PROCESS = "promotedOwnedProcess"
    STOP_CONFIRMED = "stopConfirmed"
    ORPHANED = "orphaned"
    CANCELLED_BEFORE_START = "cancelledBeforeStart"


@dataclass(frozen=True, slots=True)
class OrphanedProcess:
    pair_id: str
    arm_class: str
    attempt_id: str
    process_id: int
    process_group_id: int
    launch_spec_sha256: str
    error_type: str
```

`mark_orphaned` is idempotent only after it has already produced the same stored `OrphanedProcess`; every other duplicate or backward transition raises `ProcessLeaseError`. Validate exact lower-case 64-hex identifiers/digests, `arm_class in {"stock", "modified"}`, and positive non-boolean integer PID/PGID values at the boundary.

- [ ] **Step 4: Run the state tests and verify GREEN**

Run the Step 2 command.

Expected: all selected lease tests pass with no warnings or skips.

- [ ] **Step 5: Write the post-`Popen` aggregation RED integration test**

Add one real-process regression. Patch `batch_controller_process.OwnedProcess` so only the first construction raises after `Popen`; wrap the real `_terminate_process` so it actually kills/waits but returns `False`, establishing an unconfirmed authority result without leaking the test process. Track both raw `Popen` objects and use `_kill_test_processes` in `finally`.

The test must assert the externally observable contract:

```python
with pytest.raises(BatchControllerError, match="launch|process|lifecycle"):
    batch_controller.run_candidate_pair(
        plan,
        bindings,
        _launches(child_world, _BOUND_CHILD),
        world.private_root,
        seed=b"l" * 32,
    )

assert len(processes) == 2
assert not list(world.private_root.glob("pairs/*/receipt.json"))
assert list(world.private_root.glob("pairs/*/failure.json"))
assert len(list(Path(bindings["attemptBase"]).iterdir())) == 2
```

Instrument `PairLifecycle.abort` only through a test spy around the real method and capture `lifecycle.orphaned_processes`; assert one full `OrphanedProcess` exists after the outer `BatchControllerError` has replaced the guard error. Do not assert on mock call counts except the two real `Popen` results, which are the exact-two-arm contract.

- [ ] **Step 6: Run the integration test and verify RED for the known breaker defect**

Run:

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_controller_lifecycle_hardening.py \
  -k 'aggregation'
```

Expected: FAIL because the outer `BatchControllerError` leaves lifecycle with no bound process/orphan state and the attempt cells are cleaned. The failure must be the Task 4 breaker symptom, not a fixture/import error.

- [ ] **Step 7: Thread exact lease authority through controller, guard, and lifecycle**

In `batch_controller.py`, compute the two `launch_spec_identity` tuples once before sealing pair context, reuse their commitments in `identity-context.json`, and pass exact `arm_class` plus commitment to `_process.spawn`. Do not move the existing descriptor-derived launch-record identity call; the round-5 post-`Popen` fault checkpoint must remain post-`Popen`.

In `batch_controller_lifecycle.py`:

```python
def reserve_process(
    self, arm_class: str, attempt_id: str, launch_spec_sha256: str
) -> ProcessLifecycleLease:
    key = (arm_class, attempt_id)
    if key in self._leases:
        raise PairLifecycleError("process lease is already reserved")
    lease = ProcessLifecycleLease(
        self.pair_id, arm_class, attempt_id, launch_spec_sha256
    )
    self._leases[key] = lease
    return lease

def promote_process(
    self, lease: ProcessLifecycleLease, process: object
) -> None:
    if lease not in self._leases.values():
        raise PairLifecycleError("process lease does not belong to this pair")
    lease.promote(process)
    self.processes.append(process)

@property
def orphaned_processes(self) -> tuple[OrphanedProcess, ...]:
    return tuple(
        orphan
        for lease in self._leases.values()
        if (orphan := lease.orphaned_process) is not None
    )
```

Reject duplicate arm/attempt reservations. `complete()` rejects attached or orphaned leases even if `self.processes` is empty. `abort()` decides cell preservation from lease state, not `isinstance(error, FatalSupervisorError)`. For each promoted process, a false/raising `terminate_and_wait` marks that process's lease orphaned before descriptors close. If any lease is orphaned, return without `mark_receipts_sealed` or `cleanup_attempt_cell`.

In `batch_controller_supervision.py`, construct the guard with the reserved lease before `Popen`. `ProcessOwnershipGuard.start` calls `lease.attach_raw_process(pid, pid)` immediately after assigning `process` and `group_id`. On pre-start failure cancel the lease; on confirmed abort call `confirm_stopped`; on unconfirmed abort call `mark_orphaned` before raising `FatalSupervisorError`. Add the lease as a required `OwnedProcess` field; `_begin_terminal_drain` marks it orphaned before raising on an unconfirmed stop. Replace `lifecycle.bind_process(owned)` with `lifecycle.promote_process(lease, owned)` before `guard.transfer()`.

- [ ] **Step 8: Run focused GREEN and Task 4 compatibility**

Run:

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_controller_lifecycle_hardening.py \
  scripts/ai_ip/eval_lab/test_batch_controller_review_round5.py
```

Expected: LH1 tests pass; the ten existing round-5 tests pass; zero skips in these two files. Confirm the aggregation test passes for the right reason: two processes were attempted, no completion exists, and both cells remain.

- [ ] **Step 9: Commit Task 1**

```bash
git add scripts/ai_ip/eval_lab/batch_controller_process_lease.py \
  scripts/ai_ip/eval_lab/batch_controller_lifecycle.py \
  scripts/ai_ip/eval_lab/batch_controller_supervision.py \
  scripts/ai_ip/eval_lab/batch_controller_process.py \
  scripts/ai_ip/eval_lab/batch_controller.py \
  scripts/ai_ip/eval_lab/test_batch_controller_lifecycle_hardening.py
git commit -m "fix(eval): pre-register process lifecycle authority"
```

Before reporting DONE, record RED and GREEN outputs, full changed-file list, `git diff --check`, production line counts, and self-review findings in the task report.

---

### Task 2: Seal and offline-verify per-attempt orphan authority

**Files:**
- Create: `scripts/ai_ip/eval_lab/batch_orphan_authority.py`
- Modify: `scripts/ai_ip/eval_lab/batch_contracts.py:25-37`
- Modify: `scripts/ai_ip/eval_lab/batch_receipts.py:67-125`
- Modify: `scripts/ai_ip/eval_lab/batch_receipt_offline.py:33-125`
- Modify: `scripts/ai_ip/eval_lab/batch_controller_lifecycle.py:24-135`
- Modify: `scripts/ai_ip/eval_lab/test_batch_controller_lifecycle_hardening.py`

**Interfaces:**
- Consumes: Task 1 `OrphanedProcess`, `PairLifecycle.orphaned_processes`, bound `EvidenceLayout`, `OfflineEvidence`, `seal_self_commitment`, `verify_self_commitment`, canonical JSON, and exact `identity-context.json` arm commitments.
- Produces:
  - `EvidenceLayout.root_directory: BoundDirectory` referencing the existing root entry, not a reopened pathname.
  - `OfflineEvidence.load_root(name: str, limit: int = MAX_PRIVATE_FILE_BYTES) -> object`.
  - `orphan_authority_name(pair_id: str, attempt_id: str) -> str`, exactly `orphan-{pair_id}-{attempt_id}.json` after identifier validation.
  - `seal_orphan_authority(root: BoundDirectory, orphan: OrphanedProcess, *, recorded_at: str | None = None) -> dict[str, object]`.
  - `verify_orphan_authority(private_root: Path, pair_id: str, attempt_id: str) -> dict[str, object]`.
- The exact record fields are `schemaVersion`, `pairId`, `armClass`, `attemptId`, `processId`, `processGroupId`, `launchSpecSha256`, `status`, `stopDisposition`, `errorType`, `recordedAt`, and `orphanSha256`; no other fields are accepted.

- [ ] **Step 1: Write authority, tamper, replay, and multi-orphan RED tests**

Extend the LH1 test file with real private-root scenarios:

1. One guard-level orphan yields one `orphan-{pair}-{attempt}.json`, two attempted arms, pair/attempt failure tombstones, no pair receipt, and two retained cells.
2. Two injected `OwnedProcess` construction failures plus unconfirmed stop results yield two distinct records; both verify independently.
3. A confirmed stop after the same post-`Popen` failure yields no per-attempt orphan record and cleans both cells.
4. A pre-`Popen` failure yields no process ID and no orphan record.
5. Parameterized mutation of `processId`, `processGroupId`, `pairId`, `attemptId`, `launchSpecSha256`, `status`, `stopDisposition`, `armClass`, or `orphanSha256` makes verification fail.
6. Renaming/copying a valid record under another attempt filename or another pair root makes verification fail.
7. Replacing the root/layout identity makes verification fail through existing descriptor checks.
8. Patching `batch_controller_lifecycle.seal_orphan_authority` to raise makes the public run fail with orphan/evidence authority wording, retains both cells, and still emits no pair receipt.

The acceptance assertion uses the real verifier:

```python
record = verify_orphan_authority(world.private_root, pair_id, attempt_id)
assert record == json.loads(record_path.read_bytes())
assert record["status"] == "orphaned"
assert record["stopDisposition"] == "unconfirmed"
```

Expected values are literal or derived from sealed test inputs, never by calling the production sealer to build the expected object.

- [ ] **Step 2: Run authority tests and verify RED**

Run:

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_controller_lifecycle_hardening.py \
  -k 'authority or orphan or tamper or replay'
```

Expected: import/behavior failures because `batch_orphan_authority`, per-attempt files, exact verification, and bound-root exposure do not yet exist. Preserve the exact output in the task report.

- [ ] **Step 3: Implement exact bound-root orphan sealing**

Add `orphanSha256` to `_SELF_COMMITMENT_FIELDS` so existing `seal_self_commitment` and `verify_self_commitment` apply without a second commitment algorithm.

Change `EvidenceLayout` to carry the already-opened root `BoundDirectory` explicitly:

```python
@dataclass
class EvidenceLayout:
    root_directory: BoundDirectory
    pair_directory: BoundDirectory
    attempt_directories: tuple[BoundDirectory, BoundDirectory]
    _entries: tuple[BoundDirectory, ...]
    identity_sha256: str
```

Construct it with `entries[0]`; do not reopen the root by pathname. Add `OfflineEvidence.load_root` as a thin call to its existing descriptor-retained `_load(self.root, name, limit)`.

In `batch_orphan_authority.py`, validate identifiers before forming the filename, build the exact record with `orphanSha256` initialized to 64 zeroes, seal it with `seal_self_commitment`, and write canonical JSON plus one LF using `write_entry(root, name, payload)`. Default `recordedAt` uses UTC ISO-8601 with a trailing `Z`; a supplied value exists only to make exact tests deterministic and must pass the same validation.

- [ ] **Step 4: Implement offline verification and lifecycle sealing**

`verify_orphan_authority` must open one `OfflineEvidence`, then in a `try/finally`:

- open the requested attempt from the sealed layout to prove membership;
- load the root record descriptor-relative and require its exact 12-field set/types;
- require canonical bytes through `OfflineEvidence.load_root`;
- verify `orphanSha256`;
- require filename parameters to equal record `pairId`/`attemptId`;
- load `identity-context.json`, require `armClass in {"stock", "modified"}`, and match `launchSpecSha256` to that arm's sealed value;
- require positive non-boolean PID/PGID;
- require `status == "orphaned"` and `stopDisposition == "unconfirmed"`;
- require pair `failure.json` and reject pair `receipt.json`;
- call `evidence.verify()` after the final read;
- always close descriptors.

In `PairLifecycle.abort`, seal one record for every `orphaned_processes` item through `self.layout.root_directory`. Do this after terminal tombstones and before returning without cell cleanup. If any exclusive write fails, keep all cells and raise `PairLifecycleError("authenticated orphan authority sealing failed")` from the first sealing error; do not fall through to ordinary cleanup. Retain the existing generic resource-orphan path only for non-process cell-cleanup failures and keep its filename disjoint from per-attempt process authority.

- [ ] **Step 5: Run focused GREEN tests**

Run the Step 2 command, then:

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 \
  pytest -q scripts/ai_ip/eval_lab/test_batch_controller_lifecycle_hardening.py \
  scripts/ai_ip/eval_lab/test_batch_controller_review.py \
  scripts/ai_ip/eval_lab/test_batch_controller_review_round2.py \
  scripts/ai_ip/eval_lab/test_batch_controller_review_round3.py \
  scripts/ai_ip/eval_lab/test_batch_controller_review_round4.py \
  scripts/ai_ip/eval_lab/test_batch_controller_review_round5.py
```

Expected: all LH1 and all five Task 4 review suites pass, with no skips in these files. Inspect every `orphan-*.json` assertion in historical tests to confirm the new attempt-level records preserve its behavioral meaning rather than weakening the assertion.

- [ ] **Step 6: Run the exact Task 1–4 plus LH1 compatibility suite**

Run:

```bash
uv run --python 3.11 --with pytest==9.0.2 --with jsonschema==4.25.1 pytest -q \
  scripts/ai_ip/eval_lab/test_batch_contracts.py \
  scripts/ai_ip/eval_lab/test_contract_assets.py \
  scripts/ai_ip/eval_lab/test_batch_plan.py \
  scripts/ai_ip/eval_lab/test_batch_isolation.py \
  scripts/ai_ip/eval_lab/test_batch_isolation_security.py \
  scripts/ai_ip/eval_lab/test_batch_isolation_security_round2.py \
  scripts/ai_ip/eval_lab/test_batch_isolation_security_round3.py \
  scripts/ai_ip/eval_lab/test_batch_isolation_security_round4.py \
  scripts/ai_ip/eval_lab/test_batch_isolation_security_round5.py \
  scripts/ai_ip/eval_lab/test_batch_isolation_windows_round4.py \
  scripts/ai_ip/eval_lab/test_batch_isolation_windows_round5.py \
  scripts/ai_ip/eval_lab/test_batch_controller.py \
  scripts/ai_ip/eval_lab/test_batch_controller_review.py \
  scripts/ai_ip/eval_lab/test_batch_controller_review_round2.py \
  scripts/ai_ip/eval_lab/test_batch_controller_review_round3.py \
  scripts/ai_ip/eval_lab/test_batch_controller_review_round4.py \
  scripts/ai_ip/eval_lab/test_batch_controller_review_round5.py \
  scripts/ai_ip/eval_lab/test_batch_controller_lifecycle_hardening.py
```

Expected: at least the existing `250 passed, 10 skipped` plus every new LH1 test; the only skips are the ten R8 native-Windows guards. Any new skip, xfail, warning, hang, or reduced historical pass count is a failure.

- [ ] **Step 7: Verify caps and run the mandatory final formatter**

Before formatting, run:

```bash
git diff --check
wc -l scripts/ai_ip/eval_lab/batch_controller_process_lease.py \
  scripts/ai_ip/eval_lab/batch_orphan_authority.py \
  scripts/ai_ip/eval_lab/batch_controller_lifecycle.py \
  scripts/ai_ip/eval_lab/batch_controller_supervision.py
lh1_base_commit=$(git merge-base codex/07b-isolated-batch-runner HEAD)
git diff --stat "$lh1_base_commit"
```

Expected: no whitespace errors; every production module below 500 lines; the full non-mechanical LH1 implementation below 800 changed lines. If Task 1 contains multiple commits, measure from the LH1 branch base rather than trusting `HEAD~1` and include the exact base in the report.

After all tests have passed, run from `codex-rs`:

```bash
just fmt
```

If `cargo` is absent from inherited `PATH`, prepend the installed stable Rust toolchain path and rerun the same `just fmt`. Do not run tests after the successful formatter. Inspect formatter changes and preserve only intended LH1 formatting; never discard unrelated user work.

- [ ] **Step 8: Commit Task 2 and formatter output**

```bash
git add scripts/ai_ip/eval_lab/batch_controller_process_lease.py \
  scripts/ai_ip/eval_lab/batch_orphan_authority.py \
  scripts/ai_ip/eval_lab/batch_contracts.py \
  scripts/ai_ip/eval_lab/batch_receipts.py \
  scripts/ai_ip/eval_lab/batch_receipt_offline.py \
  scripts/ai_ip/eval_lab/batch_controller_lifecycle.py \
  scripts/ai_ip/eval_lab/batch_controller_supervision.py \
  scripts/ai_ip/eval_lab/batch_controller_process.py \
  scripts/ai_ip/eval_lab/batch_controller.py \
  scripts/ai_ip/eval_lab/test_batch_controller_lifecycle_hardening.py
git commit -m "feat(eval): authenticate orphan process authority"
```

Before reporting DONE, record RED/GREEN evidence, exact combined pass/skip counts, formatter result, line/change caps, `git diff --check`, final HEAD, and self-review findings in the task report. Do not claim LH1 Ready; independent task and whole-branch reviewers own that verdict.

---

## Final Review Gate

After both tasks pass their individual spec-and-quality reviews:

1. Generate one whole-branch review package from the LH1 branch base to final HEAD.
2. Dispatch a fresh strongest-available reviewer with the spec, plan, LH1 ledger rulings, and review package.
3. Require explicit Critical 0 / Important 0 / Ready Yes, including proof that exception wrapping no longer controls lifecycle truth, per-attempt records verify offline, cells survive evidence-write failure, and the exact compatibility suite retains only the ten R8 skips.
4. If final review finds issues, use exactly one consolidated fix dispatch and one scoped re-review as required by Subagent-Driven Development; adjudicate residuals in the LH1 ledger.
5. Only after a clean final review may the branch be merged locally into `codex/07b-isolated-batch-runner`, R10 be closed by an append-only ledger entry, and Task 5 become unblocked.
