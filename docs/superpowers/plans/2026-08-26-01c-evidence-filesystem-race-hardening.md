> **RETIRED — RETIRED_NOT_PASSED (2026-09-05).** This evaluator/proof document is historical and is not executable. All commands, prerequisite gates and successor authorizations below are retired by the approved business-first cleanup; prior results and failures remain historical claims only. Do not resume Phase 0A, 06A, 06B, 07A, 07B or LH1 from this record. Retirement grants no business PASS, model qualification, spending, publication, customer-data access or customer-release authority. Current authority: `docs/superpowers/specs/2026-09-05-business-first-cleanup-design.md` and `docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md`.

# Phase 0A.2 Task 3B — Evidence Filesystem Race Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the two independently confirmed filesystem-race gaps in the native public-evidence verifier without changing its public formats, CLI, provider boundary, or business behavior.

**Architecture:** Keep the verifier’s existing snapshot/rehash design and add two narrowly scoped defenses. First, all descriptor identity reads flow through one `UnsafeEvidenceError`-normalizing helper. Second, absolute path reads use descriptor-relative, no-follow traversal from the filesystem anchor when the platform supports it; the Windows-compatible fallback performs explicit component check-open-check validation and includes reparse attributes/tags in identity comparisons.

**Tech Stack:** CPython 3.11.15 standard library, pytest 8.3.5, existing `scripts/ai_ip/foundation/verify_evidence.py` verifier and fixtures.

**Spec:** `docs/superpowers/specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md` Work Package 2, constrained by `docs/superpowers/plans/2026-08-25-01b-codex-native-evidence-macos-baseline.md` Task 3 and the Task 3 round-three review verdict recorded in `.superpowers/sdd/2026-08-25-01b-codex-native-evidence-macos-baseline/progress.md`.

## Global Constraints

- Modify only `scripts/ai_ip/foundation/verify_evidence.py` and `scripts/ai_ip/foundation/test_verify_evidence.py` during implementation. This plan document is the only governance addition.
- Preserve every existing CLI option, exit code, canonical JSON schema, evidence layout, byte-regex rule, semantic filter, retained-payload limit, and create-new publication rule.
- Use Python 3.11 standard library only; add no dependency and no `__future__` import.
- Preserve `codex-rs/**` byte-for-byte relative to `4ef1d4b89bd419c976b04fefa0fd36844e898340`.
- `providerMode=not-run` and `paidProviderCost=0`; do not locate, read, print, or use any API key and do not call a provider or model.
- Treat symlink, reparse, hardlink, special-file, component substitution, descriptor-identity, and read-race failures as `UnsafeEvidenceError`. Only existing matrix/evidence/frozen-final CLI boundaries translate that class to compact `INVALID_FORBIDDEN_CONTENT` JSON. Ordinary size, JSON schema, semantic, Git, and business-binding errors remain ordinary `EvidenceError` on stderr.
- On descriptor-relative platforms, absolute paths must be traversed from their filesystem anchor one directory component at a time with no-follow directory opens; a no-follow leaf open alone is insufficient.
- On platforms without descriptor-relative traversal, run a documented check-open-check fallback over every component and reject any symlink/reparse state before or after the open. This fallback is an honest portability boundary, not a claim of POSIX-equivalent race freedom.
- Every new regression test must first be observed failing for its named missing behavior. Tests must exercise real verifier behavior; monkeypatching may inject `OSError` or simulated Windows metadata but assertions target verifier exceptions/CLI output, not mock call counts.
- Use the exact Task 0 environment:
  - `AI_IP_UV_ROOT=/var/folders/vm/g05xtp4x72q6c6365sw3d2sm0000gn/T/ai-ip-native-tools.KIwJgL/uv-0.11.3`
  - `AI_IP_DEV_PATH=/var/folders/vm/g05xtp4x72q6c6365sw3d2sm0000gn/T/ai-ip-native-tools.KIwJgL/uv-0.11.3/bin:/var/folders/vm/g05xtp4x72q6c6365sw3d2sm0000gn/T/ai-ip-native-tools.KIwJgL/cargo-tools/bin:/var/folders/vm/g05xtp4x72q6c6365sw3d2sm0000gn/T/ai-ip-native-tools.KIwJgL/direct-bin:/var/folders/vm/g05xtp4x72q6c6365sw3d2sm0000gn/T/ai-ip-native-tools.KIwJgL/npm-tools/node_modules/.bin:/Users/yangyucheng/.rustup/toolchains/1.95.0-x86_64-apple-darwin/bin:/usr/local/bin:/usr/bin:/bin`
- Run `just fmt` after code edits. Do not run the complete workspace `just test`; that remains the later Task 7 user decision point.
- End with one coherent commit, upstream-lock PASS, `git diff --check` PASS, pinned-upstream `codex-rs` zero diff, and a clean worktree.

## Root-Cause Record

| Finding | Confirmed root cause | Required correction |
|---|---|---|
| Post-read identity failures escape the compact unsafe channel | `_stable_file_bytes`, `_scan_one`, `_FileSnapshot.capture`, and `_FileSnapshot.validate_final_state` call `os.fstat()` after reading without translating `OSError`; raw exceptions can reach the CLI. | Route all descriptor metadata reads in the affected verifier paths through one safe helper that raises `UnsafeEvidenceError` with a public label and never includes an OS path or original exception text. |
| Parent-component substitution can survive final rehash | Path-based capture checks ancestors once, then final reopen protects only the leaf with `O_NOFOLLOW`; renaming a parent and replacing it with a symlink to the original directory preserves the leaf inode/digest. `_same_file_state` also omits Windows reparse attributes/tags. | Traverse absolute components descriptor-relative from the anchor on POSIX. For the fallback, validate every component before and after open/read. Include reparse state in identity comparison and reject a reparse bit/tag at every checkpoint. |

## File Responsibility Map

| Path | Responsibility in Task 3B |
|---|---|
| `scripts/ai_ip/foundation/verify_evidence.py` | Safe descriptor metadata helper, anchored no-follow path traversal, fallback component validation, reparse-aware state comparison, and lifecycle-safe descriptor cleanup. |
| `scripts/ai_ip/foundation/test_verify_evidence.py` | Behavioral RED/GREEN coverage for post-read `fstat` errors, matrix/final/evidence-root parent substitution, fallback behavior, Windows reparse mutation, compact CLI classification, and descriptor cleanup. |

---

### Task 1: Close descriptor and parent-component races

**Files:**
- Modify: `scripts/ai_ip/foundation/verify_evidence.py`
- Modify: `scripts/ai_ip/foundation/test_verify_evidence.py`

**Interfaces:**
- Consumes unchanged: `_FileSnapshot.capture`, `_FileSnapshot.validate_final_state`, `_EvidenceSnapshot`, `_capture_matrix`, `scan_forbidden_evidence`, `verify_evidence`, `verify_frozen_final`, and `main`.
- Produces private helper: `_safe_fstat(descriptor: int, label: str) -> os.stat_result`.
- Produces private helper `_open_absolute_parent_nofollow(path: Path) -> int`, returning an owned descriptor for the absolute path's stable parent after descriptor-relative no-follow traversal from the filesystem anchor.
- Produces private helper `_fallback_component_states(path: Path, label: str)`, returning an immutable tuple of reparse-checked `lstat` states for the anchor-through-parent component chain on platforms without descriptor-relative traversal.
- Preserves all public Python and CLI interfaces byte-for-byte at their boundaries.

- [ ] **Step 1: Add post-read `fstat` RED tests**

Add a test helper that records descriptors only after a real `os.read`, then raises `OSError("injected post-read fstat")` when the verifier next inspects one of those descriptors. The production change that makes these tests fail is removing `OSError -> UnsafeEvidenceError` normalization after a bounded read.

```python
def _inject_post_read_fstat_failure(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    real_read = os.read
    real_fstat = os.fstat
    read_descriptors: set[int] = set()

    def track_read(descriptor: int, byte_count: int) -> bytes:
        payload = real_read(descriptor, byte_count)
        read_descriptors.add(descriptor)
        return payload

    def fail_after_read(descriptor: int) -> os.stat_result:
        if descriptor in read_descriptors:
            raise OSError("injected post-read fstat")
        return real_fstat(descriptor)

    monkeypatch.setattr(verifier.os, "read", track_read)
    monkeypatch.setattr(verifier.os, "fstat", fail_after_read)
```

Add four behavioral cases:

```python
def test_stable_file_bytes_normalizes_post_read_fstat_failure(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    path = tmp_path / "public.json"
    path.write_bytes(b"{}\n")
    _inject_post_read_fstat_failure(monkeypatch)
    with pytest.raises(verifier.UnsafeEvidenceError):
        verifier._stable_file_bytes(path)

def test_recursive_scanner_normalizes_post_read_fstat_failure(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    root = tmp_path / "evidence"
    root.mkdir()
    (root / "safe.log").write_bytes(b"safe\n")
    _inject_post_read_fstat_failure(monkeypatch)
    with pytest.raises(verifier.UnsafeEvidenceError):
        verifier.scan_forbidden_evidence(root)

def test_baseline_cli_reports_post_read_matrix_fstat_failure_as_forbidden_only(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
    valid_evidence_template: ValidEvidenceFixture,
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    _inject_post_read_fstat_failure(monkeypatch)
    _assert_forbidden_cli_result(
        verifier.main(
            [
                "--repo-root", str(fixture.repo),
                "--matrix", str(fixture.matrix),
                "--evidence-root", str(fixture.baseline),
                "--platform", "macos-x86_64",
                "--mode", "baseline",
            ]
        ),
        capsys,
    )

def test_final_cli_reports_post_read_input_fstat_failure_as_forbidden_only(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
    final_fixture_template: FinalFixture,
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    _stub_final_native_evidence(monkeypatch, fixture)
    _inject_post_read_fstat_failure(monkeypatch)
    _assert_forbidden_cli_result(
        verifier.main(_final_fixture_cli_args(fixture)), capsys
    )
```

The tests must not assert the raw injected message, and CLI stderr must remain empty.

- [ ] **Step 2: Add parent-component and reparse RED tests**

Use real temporary directories for parent substitution. Capture the target first, rename its parent to a detached sibling, and replace the original parent path with a directory symlink pointing to the detached original. The leaf inode, size, mtime, bytes, and digest remain unchanged; rejection must therefore come from component safety, not content mismatch.

```python
def _replace_parent_with_symlink_to_original(parent: Path) -> Path:
    detached = parent.with_name(f"{parent.name}-detached")
    parent.rename(detached)
    parent.symlink_to(detached, target_is_directory=True)
    return detached

@pytest.mark.parametrize("force_fallback", [False, True], ids=["posix", "fallback"])
def test_file_snapshot_rejects_parent_symlink_to_original_on_final_rehash(
    force_fallback: bool,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    parent = tmp_path / "parent"
    parent.mkdir()
    path = parent / "public.json"
    path.write_bytes(b"{}\n")
    if force_fallback:
        monkeypatch.setattr(
            verifier, "_descriptor_relative_traversal_available", lambda: False
        )
    snapshot = verifier._FileSnapshot.capture(path, path.name, keep_payload=True)
    _replace_parent_with_symlink_to_original(path.parent)
    with pytest.raises(verifier.UnsafeEvidenceError):
        snapshot.validate_final_state()
```

Add public-boundary variants for all load-bearing roots. Use these hooks so the race occurs after initial capture but before the existing final rehash:

```python
def test_baseline_cli_rejects_matrix_parent_symlink_to_original(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
    valid_evidence_template: ValidEvidenceFixture,
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    matrix_from_value = verifier._matrix_from_value

    def parse_then_swap(parsed: object) -> capture.Matrix:
        matrix = matrix_from_value(parsed)
        _replace_parent_with_symlink_to_original(fixture.matrix.parent)
        return matrix

    monkeypatch.setattr(verifier, "_matrix_from_value", parse_then_swap)
    _assert_forbidden_cli_result(
        verifier.main(
            [
                "--repo-root", str(fixture.repo),
                "--matrix", str(fixture.matrix),
                "--evidence-root", str(fixture.baseline),
                "--platform", "macos-x86_64",
                "--mode", "baseline",
            ]
        ),
        capsys,
    )

def test_baseline_cli_rejects_evidence_parent_symlink_to_original(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
    valid_evidence_template: ValidEvidenceFixture,
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    require_exact = verifier._EvidenceSnapshot.require_exact

    def require_then_swap(
        snapshot: verifier._EvidenceSnapshot,
        required_names: set[str],
        optional_names: set[str],
    ) -> None:
        require_exact(snapshot, required_names, optional_names)
        if snapshot.root == fixture.baseline:
            _replace_parent_with_symlink_to_original(snapshot.root.parent)

    monkeypatch.setattr(verifier._EvidenceSnapshot, "require_exact", require_then_swap)
    _assert_forbidden_cli_result(
        verifier.main(
            [
                "--repo-root", str(fixture.repo),
                "--matrix", str(fixture.matrix),
                "--evidence-root", str(fixture.baseline),
                "--platform", "macos-x86_64",
                "--mode", "baseline",
            ]
        ),
        capsys,
    )

def test_final_cli_rejects_report_parent_symlink_to_original(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
    final_fixture_template: FinalFixture,
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    _stub_final_native_evidence(monkeypatch, fixture)
    validate_receipt = verifier._validate_receipt

    def validate_then_swap(*args: object, **kwargs: object) -> dict[str, object]:
        receipt = validate_receipt(*args, **kwargs)
        _replace_parent_with_symlink_to_original(fixture.report.parent)
        return receipt

    monkeypatch.setattr(verifier, "_validate_receipt", validate_then_swap)
    _assert_forbidden_cli_result(
        verifier.main(_final_fixture_cli_args(fixture)), capsys
    )
```

The three variants prove:

- Baseline CLI: substitute the matrix parent after matrix capture and require compact forbidden-only JSON.
- Baseline CLI: substitute the evidence platform parent after `_EvidenceSnapshot` capture and require compact forbidden-only JSON.
- Frozen-final CLI: substitute the selected-report parent after validation and require compact forbidden-only JSON.

Add one simulated Windows metadata mutation where `os.fstat` returns the same core identity after the bounded read but changes `st_file_attributes` to include `0x400` and sets a nonzero `st_reparse_tag`. `_FileSnapshot.validate_final_state()` must raise `UnsafeEvidenceError`. The production mutation caught by this test is deleting reparse attributes/tags from `_same_file_state` or failing to reject them after the read.

Add a descriptor-lifecycle test that wraps the real `os.open`, records only descriptors returned during `_FileSnapshot.capture()` plus `validate_final_state()`, and calls the real `os.fstat` on each recorded descriptor after both a passing run and an injected component failure. Every recorded descriptor must raise `OSError` with `errno.EBADF`; the test therefore checks verifier-owned descriptors directly and does not depend on `/dev/fd` process-wide counts.

- [ ] **Step 3: Run the new focused tests and record RED**

```bash
PATH="$AI_IP_DEV_PATH" PYTHONDONTWRITEBYTECODE=1 \
  "$AI_IP_UV_ROOT/bin/uv" run --python "$AI_IP_UV_ROOT/bin/python" --with pytest==8.3.5 \
  pytest -q -p no:cacheprovider scripts/ai_ip/foundation/test_verify_evidence.py \
  -k 'post_read_fstat or parent_symlink_to_original or parent_component or reparse_after_read or descriptor_lifecycle'
```

Expected: the new tests fail because raw `OSError`, path-based parent traversal, or reparse-insensitive identity comparison remains; existing control cases in the selection pass. Syntax/import/fixture errors do not count as RED.

- [ ] **Step 4: Implement safe metadata reads and anchored traversal**

Implement `_safe_fstat` and replace the post-read raw calls plus existing duplicated open-time `fstat` wrappers where doing so preserves their public labels:

```python
def _safe_fstat(descriptor: int, label: str) -> os.stat_result:
    try:
        return os.fstat(descriptor)
    except OSError as error:
        raise UnsafeEvidenceError(
            f"unable to inspect opened public evidence: {label}"
        ) from error
```

Extend `_same_file_state` so its compared tuple includes normalized reparse state:

```python
def _reparse_state(value: os.stat_result) -> tuple[int, int]:
    return (
        getattr(value, "st_file_attributes", 0) & 0x400,
        getattr(value, "st_reparse_tag", 0),
    )
```

The comparison must retain every existing POSIX field and append `_reparse_state(value)`. Any opened or post-read state with the reparse bit or a nonzero reparse tag is unsafe even when all core fields match.

Implement `_open_absolute_parent_nofollow(path)` for platforms satisfying `_descriptor_relative_traversal_available()`:

1. Require an absolute path and open its anchor as a no-follow directory.
2. For each parent component, `os.stat(component, dir_fd=current_fd, follow_symlinks=False)`, reject non-directory/symlink/reparse state, `os.open(component, O_RDONLY|O_DIRECTORY|O_NOFOLLOW, dir_fd=current_fd)`, compare the opened state through `_safe_fstat`, then close the previous descriptor.
3. Open/stat the final leaf relative to the stable parent descriptor. Close the parent descriptor after obtaining the leaf descriptor; the leaf descriptor remains valid for the bounded read.
4. Re-run the same anchored traversal for every post-read and final-rehash path-state check. Persistent ancestor substitution therefore fails even when it points to the original directory.
5. Handle the filesystem anchor and single-component paths explicitly. Every intermediate descriptor is closed in `finally` on all exceptions.

Implement `_fallback_component_states(path, label)` and use it for the fallback where descriptor-relative traversal is unavailable:

1. Capture each absolute component with `lstat` before open, rejecting symlink/reparse/non-directory parents.
2. Open the leaf without shell involvement, validate it with `_safe_fstat`, then capture the complete component chain again.
3. Require before/after component identities to match under the reparse-aware comparison and reject any reparse state observed at either checkpoint.
4. Repeat the check-open-check sequence during final rehash and evidence-root final validation.

Do not broaden the forbidden exception translation. Keep ordinary `EvidenceError` behavior unchanged.

- [ ] **Step 5: Run focused GREEN and preserved boundary controls**

```bash
PATH="$AI_IP_DEV_PATH" PYTHONDONTWRITEBYTECODE=1 \
  "$AI_IP_UV_ROOT/bin/uv" run --python "$AI_IP_UV_ROOT/bin/python" --with pytest==8.3.5 \
  pytest -q -p no:cacheprovider scripts/ai_ip/foundation/test_verify_evidence.py \
  -k 'post_read_fstat or parent_symlink_to_original or parent_component or reparse_after_read or descriptor_lifecycle or hardlinked or post_check_path_race or continual_growth or windows_fallback or recursive_scanner'
```

Expected: all selected tests pass with zero warnings. Verify the new regression tests by reverting only the production diff, observing them fail, restoring the diff, and observing them pass again before commit.

- [ ] **Step 6: Run the complete foundation Python regression**

```bash
PATH="$AI_IP_DEV_PATH" PYTHONDONTWRITEBYTECODE=1 \
  "$AI_IP_UV_ROOT/bin/uv" run --python "$AI_IP_UV_ROOT/bin/python" --with pytest==8.3.5 \
  pytest -q -p no:cacheprovider \
  scripts/ai_ip/foundation/test_verify_evidence.py \
  scripts/ai_ip/foundation/test_capture_command.py \
  scripts/ai_ip/foundation/test_verify_upstream_lock.py
```

Expected: PASS with zero failures. Do not infer this result from Step 5.

- [ ] **Step 7: Format, verify, and commit Task 3B**

```bash
PATH="$AI_IP_DEV_PATH" just fmt
"$AI_IP_UV_ROOT/bin/python" -m py_compile \
  scripts/ai_ip/foundation/verify_evidence.py \
  scripts/ai_ip/foundation/test_verify_evidence.py
git diff --check
git add -- \
  scripts/ai_ip/foundation/verify_evidence.py \
  scripts/ai_ip/foundation/test_verify_evidence.py
git diff --cached --check
test "$(git diff --cached --name-only | LC_ALL=C sort)" = "$(printf '%s\n' \
  scripts/ai_ip/foundation/test_verify_evidence.py \
  scripts/ai_ip/foundation/verify_evidence.py | LC_ALL=C sort)"
git commit -m "fix: close native evidence path races"
"$AI_IP_UV_ROOT/bin/python" scripts/ai_ip/foundation/verify_upstream_lock.py --repo "$PWD"
test -z "$(git diff --name-only 4ef1d4b89bd419c976b04fefa0fd36844e898340..HEAD -- codex-rs)"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
```

Expected: one cohesive implementation commit after the separately committed plan, upstream-lock PASS, no `codex-rs` path, and a clean worktree.
