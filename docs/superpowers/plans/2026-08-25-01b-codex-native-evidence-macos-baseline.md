# Phase 0A.2 — Codex Native Evidence and macOS Baseline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the fail-closed native evidence tooling and record the unchanged pinned Codex macOS x86_64 focused baseline before any AI IP runtime or business code is modified.

**Architecture:** Three standard-library Python 3.11 CLIs own command capture, public evidence verification, and frozen evaluator dispatch. A committed machine matrix is the only command authority; a detached tools worktree runs the recorder, a separate detached pinned-Codex worktree is tested, and the product worktree receives evidence. This child records macOS evidence and emits a two-ref transfer bundle; Windows 11 evidence is a later independent child on the exact same tools SHA and remains mandatory before G1 can pass.

**Tech Stack:** OpenAI Codex `4ef1d4b89bd419c976b04fefa0fd36844e898340`, AI IP fork base `97cf8fb49df8fa6ac742d1eadd3645a04447dfc3`, CPython 3.11.15, `uv` 0.11.3, Rust/Cargo 1.95.0, cargo-nextest 0.9.103, cargo-deny 0.20.2, pnpm 10.34.5, DotSlash 0.5.8, Bazelisk 1.28.1 with Bazel 9.0.0, Git, pytest 8.3.5.

**Spec:** `docs/superpowers/specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md` Work Package 2, constrained by `docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md` and `docs/superpowers/specs/2026-08-25-domestic-model-adaptation-design.md`.

## Global Constraints

- Preserve `codex-rs/**` byte-for-byte relative to `4ef1d4b89bd419c976b04fefa0fd36844e898340`; this child changes no runtime, App Server, provider, Skill, prompt, model configuration, or business contract.
- Run on native Darwin x86_64 only. Rosetta, cross-compilation, Linux, arm64, or a caller-supplied host label cannot produce `macos-x86_64` evidence.
- Use Python 3.11 standard library only for the three evidence CLIs. Do not add Python dependencies or `__future__` imports.
- All Git subprocesses in evidence tools remove ambient `GIT_*`, set `GIT_NO_LAZY_FETCH=1`, `GIT_NO_REPLACE_OBJECTS=1`, `GIT_OPTIONAL_LOCKS=0`, `GIT_TERMINAL_PROMPT=0`, disable global/system config and fsmonitor, reject replace refs and `info/grafts`, and never prompt or lazy-fetch.
- The tested tree, tools tree, and evidence tree are three distinct absolute canonical directories. The first two remain detached and clean; evidence is written only to the product worktree.
- The command matrix contains exactly 22 `baselineAndPost` entries and 8 `postOnly` entries. macOS baseline executes exactly the 20 `baselineAndPost` entries applicable to macOS and executes no `postOnly` entry.
- Every exact-name nextest filter first runs `cargo nextest list --message-format json` followed by the original `just test` arguments after the first two tokens, and requires JSON `test-count == 1` before the command is executed.
- A nonzero required command is preserved as `BLOCKED_BASELINE`; no caller option may ignore, relabel, delete, or replace it.
- Raw stdout/stderr are captured as bytes with incremental SHA-256. Evidence paths store safe relative basenames only; no API key, bearer, customer content, private case, prompt/response body, reviewer mapping, or generated media may enter evidence or Git.
- `providerMode=not-run` and `paidProviderCost=0`; this child must not locate or read the API key the user authorized for later provider work.
- Windows baseline may run later and in parallel with subsequent approved business work, but no G1 or `PASS_TO_PHASE_0B` claim is legal until verified native Windows 11 x64 baseline and post evidence exist.
- Run `just fmt` after code edits. Use the exact focused pytest commands below. Do not run direct `cargo test`. Ask the user before the complete workspace `just test`, then record `passed`, `failed`, or `not-run/declined` without treating it as a required matrix item.
- Each implementation task ends with a coherent commit, a clean worktree, `python3 scripts/ai_ip/foundation/verify_upstream_lock.py --repo "$PWD"`, `git diff --check`, and `codex-rs` zero-diff verification against the pinned upstream.

---

## Scope Split and Seam Classification

| Seam | Classification | This child | Rollback |
|---|---|---|---|
| `codex-rs/**`, root `AGENTS.md`, `LICENSE`, upstream remote | **preserve** | Read and execute only from a detached pinned worktree | Delete evidence/tool commits; imported ancestors remain untouched |
| `scripts/ai_ip/foundation/{capture_command,verify_evidence,run_frozen_eval}.py` and tests | **add** | Standard-library evidence tools only | Revert the tool commits and delete the detached tools worktree |
| `scripts/ai_ip/foundation/required_command_matrix.json` | **add/freeze** | Single authority for baseline and post commands | Revert only before any evidence exists; after tools SHA, a change invalidates all evidence |
| `docs/evidence/foundation/README.md` and `macos-x86_64/**` | **add** | Public, no-content native evidence | Revert the evidence commit; never edit a recorded manifest/log in place |
| ignored `*.stdout.log` / `*.stderr.log` | **preserve upstream ignore** | Stage only exact evidence log paths with `git add -f --` | Remove only the exact evidence commit; do not broaden `.gitignore` |
| Windows evidence | **defer** | Emit verified two-ref bundle and handoff contract only | Delete the external bundle and local transfer refs; no product code changes |

## Locked Source Facts

- Root recipe file is `justfile`; `just test` delegates to cargo-nextest on both Unix and Windows.
- All 13 filtered tests in the matrix exist at the pinned SHA and aggregate through `tests/all.rs`.
- `source-assets` matches 151 pinned files; `ai-ip-assets/**` correctly matches zero before later work packages.
- The eight `postOnly` entries deliberately reference future AI IP crates/tests/assets; baseline verification must require that they remain unexecuted, not pretend they exist.
- Current host fact before implementation: Darwin x86_64 with 193 GiB free; Rust 1.95.0 is installed outside the ordinary PATH; cargo-nextest, cargo-deny, Bazel/Bazelisk, exact pnpm, exact uv, and exact DotSlash require an isolated bootstrap.

## File Responsibility Map

| Path | Responsibility |
|---|---|
| `scripts/ai_ip/foundation/required_command_matrix.json` | Exact 30-entry, machine-readable command authority |
| `scripts/ai_ip/foundation/capture_command.py` | Strict matrix loader, host detection, bootstrap manifest, streaming command capture, nextest single-match preflight, atomic evidence writes |
| `scripts/ai_ip/foundation/test_capture_command.py` | Matrix, path isolation, streaming, exit-code, nextest, atomicity, bootstrap, and hostile-Git tests |
| `scripts/ai_ip/foundation/verify_evidence.py` | Baseline/post public evidence verifier and frozen final-verification CLI envelope |
| `scripts/ai_ip/foundation/test_verify_evidence.py` | Tamper, completeness, platform, lock, paired-failure, and exact public-final-field tests |
| `scripts/ai_ip/foundation/run_frozen_eval.py` | Frozen context and evaluator binary/worktree guard before direct `exec` |
| `scripts/ai_ip/foundation/test_run_frozen_eval.py` | Replay/live context, platform, binary, Git, duplicate authority, and argv injection tests |
| `docs/evidence/foundation/README.md` | Public evidence layout, claims, Windows transfer protocol, retention, force-add, and rollback |
| `docs/evidence/foundation/macos-x86_64/**` | Generated bootstrap, selection, stdout/stderr, command manifests, and baseline summary |
| `docs/architecture/codex-fork-patch-ledger.md` | Append-only F-0003 tool/evidence entry and baseline disposition; F-0002 separately authorizes this plan |

---

### Task 1: Freeze the exact command matrix and source characterization

**Files:**
- Create: `scripts/ai_ip/foundation/required_command_matrix.json`
- Create: `scripts/ai_ip/foundation/test_capture_command.py`
- Create: `scripts/ai_ip/foundation/capture_command.py`

**Interfaces:**
- Produces: `load_matrix(path: Path) -> Matrix`, `canonical_json_bytes(value: object) -> bytes`, `host_id() -> str`, `selection_argv(command: CommandSpec) -> tuple[str, ...] | None`.
- `Matrix.commands` is an immutable tuple of `CommandSpec(id, platforms, phase, argv, expected_exit)` values.
- Later tasks consume the canonical matrix SHA-256 and the exact host-filtered command set.

- [ ] **Step 1: Write the matrix contract test before the loader exists**

Add tests that construct the complete expected ID/phase/platform map in Python, load the committed JSON through `capture_command.load_matrix`, and compare the whole normalized value rather than individual fields. Also assert the exact counts `30`, `22`, `8`, and macOS baseline applicable count `20`.

```python
EXPECTED_IDS = {
    "fmt-check",
    "app-server-protocol",
    "app-server-transport",
    "state",
    "thread-store",
    "app-server-process",
    "thread-start",
    "thread-resume",
    "executor-skill",
    "mcp-tool",
    "process-exec",
    "fs",
    "apply-patch",
    "build-cli-app-server",
    "cargo-metadata",
    "cargo-license-source",
    "pnpm-dependencies",
    "pnpm-licenses",
    "source-assets",
    "macos-sandbox",
    "windows-sandbox-restricted",
    "windows-sandbox-elevated",
    "post-domain",
    "post-runtime",
    "post-eval",
    "post-responses-proxy",
    "post-descendant-raw",
    "post-ai-ip-strict-output",
    "post-bazel-rust",
    "post-bazel-assets",
}

def test_required_matrix_is_exact_and_frozen() -> None:
    matrix = capture.load_matrix(MATRIX_PATH)
    assert {item.id for item in matrix.commands} == EXPECTED_IDS
    assert len(matrix.commands) == 30
    assert sum(item.phase == "baselineAndPost" for item in matrix.commands) == 22
    assert sum(item.phase == "postOnly" for item in matrix.commands) == 8
    assert len(matrix.required_for("macos-x86_64", "baseline")) == 20
    assert matrix.required_for("macos-x86_64", "baseline") == tuple(
        item
        for item in matrix.commands
        if item.phase == "baselineAndPost" and "macos-x86_64" in item.platforms
    )
```

Add parameterized rejection for duplicate JSON keys, duplicate command IDs, unknown keys, unknown platform/phase, an `argv` encoded as one command string instead of a string array, empty arguments, non-integer/boolean `expectedExit`, nonzero expected exit, and any entry whose field set differs from exactly `{id, platforms, phase, argv, expectedExit}`. Do not reject punctuation inside individual argv elements: exact nextest filters and Git pathspecs legitimately contain parentheses, equals signs, and `**`.

- [ ] **Step 2: Run the focused RED**

Run:

```bash
PYTHONDONTWRITEBYTECODE=1 uv run --python 3.11 --with pytest==8.3.5 \
  pytest -q -p no:cacheprovider \
  scripts/ai_ip/foundation/test_capture_command.py::test_required_matrix_is_exact_and_frozen
```

Expected: FAIL with `ModuleNotFoundError` for `capture_command` or `FileNotFoundError` for `required_command_matrix.json`. A syntax/import failure in the test is not an accepted RED.

- [ ] **Step 3: Create the complete 30-entry matrix**

Use the exact argv from Work Package 2. Store JSON as UTF-8 with a trailing LF; its semantic digest is the SHA-256 of `json.dumps(parsed, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")`.

```json
{
  "schemaVersion": 1,
  "commands": [
    {"id":"fmt-check","platforms":["macos-x86_64","windows-11-x64"],"phase":"baselineAndPost","argv":["just","fmt-check"],"expectedExit":0},
    {"id":"app-server-protocol","platforms":["macos-x86_64","windows-11-x64"],"phase":"baselineAndPost","argv":["just","test","-p","codex-app-server-protocol"],"expectedExit":0},
    {"id":"app-server-transport","platforms":["macos-x86_64","windows-11-x64"],"phase":"baselineAndPost","argv":["just","test","-p","codex-app-server-transport"],"expectedExit":0},
    {"id":"state","platforms":["macos-x86_64","windows-11-x64"],"phase":"baselineAndPost","argv":["just","test","-p","codex-state"],"expectedExit":0},
    {"id":"thread-store","platforms":["macos-x86_64","windows-11-x64"],"phase":"baselineAndPost","argv":["just","test","-p","codex-thread-store"],"expectedExit":0},
    {"id":"app-server-process","platforms":["macos-x86_64","windows-11-x64"],"phase":"baselineAndPost","argv":["just","test","-p","codex-app-server","--test","all","-E","test(=suite::logging::standalone_app_server_emits_json_info_events)"],"expectedExit":0},
    {"id":"thread-start","platforms":["macos-x86_64","windows-11-x64"],"phase":"baselineAndPost","argv":["just","test","-p","codex-app-server","--test","all","-E","test(=suite::v2::thread_start::thread_start_creates_thread_and_emits_started)"],"expectedExit":0},
    {"id":"thread-resume","platforms":["macos-x86_64","windows-11-x64"],"phase":"baselineAndPost","argv":["just","test","-p","codex-app-server","--test","all","-E","test(=suite::v2::thread_resume::thread_resume_supports_history_and_overrides)"],"expectedExit":0},
    {"id":"executor-skill","platforms":["macos-x86_64","windows-11-x64"],"phase":"baselineAndPost","argv":["just","test","-p","codex-app-server","--test","all","-E","test(=suite::v2::executor_skills::explicit_executor_skill_can_read_referenced_file)"],"expectedExit":0},
    {"id":"mcp-tool","platforms":["macos-x86_64","windows-11-x64"],"phase":"baselineAndPost","argv":["just","test","-p","codex-app-server","--test","all","-E","test(=suite::v2::mcp_tool::mcp_server_tool_call_returns_tool_result)"],"expectedExit":0},
    {"id":"process-exec","platforms":["macos-x86_64","windows-11-x64"],"phase":"baselineAndPost","argv":["just","test","-p","codex-app-server","--test","all","-E","test(=suite::v2::process_exec::process_spawn_returns_before_exit_and_emits_exit_notification)"],"expectedExit":0},
    {"id":"fs","platforms":["macos-x86_64","windows-11-x64"],"phase":"baselineAndPost","argv":["just","test","-p","codex-app-server","--test","all","-E","test(=suite::v2::fs::fs_methods_cover_current_fs_utils_surface)"],"expectedExit":0},
    {"id":"apply-patch","platforms":["macos-x86_64","windows-11-x64"],"phase":"baselineAndPost","argv":["just","test","-p","codex-apply-patch","--test","all","-E","test(=suite::cli::test_apply_patch_cli_add_and_update)"],"expectedExit":0},
    {"id":"build-cli-app-server","platforms":["macos-x86_64","windows-11-x64"],"phase":"baselineAndPost","argv":["cargo","build","--locked","--manifest-path","codex-rs/Cargo.toml","-p","codex-cli","-p","codex-app-server"],"expectedExit":0},
    {"id":"cargo-metadata","platforms":["macos-x86_64","windows-11-x64"],"phase":"baselineAndPost","argv":["cargo","metadata","--manifest-path","codex-rs/Cargo.toml","--locked","--format-version","1"],"expectedExit":0},
    {"id":"cargo-license-source","platforms":["macos-x86_64","windows-11-x64"],"phase":"baselineAndPost","argv":["cargo","deny","--manifest-path","codex-rs/Cargo.toml","--config","codex-rs/deny.toml","check","licenses","sources"],"expectedExit":0},
    {"id":"pnpm-dependencies","platforms":["macos-x86_64","windows-11-x64"],"phase":"baselineAndPost","argv":["pnpm","list","--recursive","--json","--depth","Infinity"],"expectedExit":0},
    {"id":"pnpm-licenses","platforms":["macos-x86_64","windows-11-x64"],"phase":"baselineAndPost","argv":["pnpm","licenses","list","--json","--long"],"expectedExit":0},
    {"id":"source-assets","platforms":["macos-x86_64","windows-11-x64"],"phase":"baselineAndPost","argv":["git","ls-files","--","codex-rs/**/assets/**","codex-rs/**/templates/**","codex-rs/**/migrations/**","ai-ip-assets/**"],"expectedExit":0},
    {"id":"macos-sandbox","platforms":["macos-x86_64"],"phase":"baselineAndPost","argv":["just","test","-p","codex-core","--test","all","-E","test(=suite::exec::write_file_fails_as_sandbox_error)"],"expectedExit":0},
    {"id":"windows-sandbox-restricted","platforms":["windows-11-x64"],"phase":"baselineAndPost","argv":["just","test","-p","codex-core","--test","all","-E","test(=suite::windows_sandbox::windows_restricted_token_rejects_exact_and_glob_deny_read_policy)"],"expectedExit":0},
    {"id":"windows-sandbox-elevated","platforms":["windows-11-x64"],"phase":"baselineAndPost","argv":["just","test","-p","codex-core","--test","all","-E","test(=suite::windows_sandbox::windows_elevated_enforces_deny_read_and_protects_setup_marker)"],"expectedExit":0},
    {"id":"post-domain","platforms":["macos-x86_64","windows-11-x64"],"phase":"postOnly","argv":["just","test","-p","codex-ai-ip-domain"],"expectedExit":0},
    {"id":"post-runtime","platforms":["macos-x86_64","windows-11-x64"],"phase":"postOnly","argv":["just","test","-p","codex-ai-ip-runtime"],"expectedExit":0},
    {"id":"post-eval","platforms":["macos-x86_64","windows-11-x64"],"phase":"postOnly","argv":["just","test","-p","codex-ai-ip-eval"],"expectedExit":0},
    {"id":"post-responses-proxy","platforms":["macos-x86_64","windows-11-x64"],"phase":"postOnly","argv":["just","test","-p","codex-responses-api-proxy"],"expectedExit":0},
    {"id":"post-descendant-raw","platforms":["macos-x86_64","windows-11-x64"],"phase":"postOnly","argv":["just","test","-p","codex-app-server","--test","all","-E","test(=suite::v2::raw_response_subagents::raw_events_cover_root_child_grandchild_and_sibling)"],"expectedExit":0},
    {"id":"post-ai-ip-strict-output","platforms":["macos-x86_64","windows-11-x64"],"phase":"postOnly","argv":["just","test","-p","codex-app-server","--test","all","-E","test(=suite::v2::ai_ip_strict_output::turn_start_sends_strict_content_package_schema)"],"expectedExit":0},
    {"id":"post-bazel-rust","platforms":["macos-x86_64","windows-11-x64"],"phase":"postOnly","argv":["bazel","test","//codex-rs/ai-ip-domain:ai-ip-domain-unit-tests","//codex-rs/ai-ip-runtime:ai-ip-runtime-unit-tests","//codex-rs/ai-ip-eval:ai-ip-eval-unit-tests","//codex-rs/ai-ip-eval:codex-ai-ip-eval-bin-unit-tests","//codex-rs/responses-api-proxy:responses-api-proxy-unit-tests"],"expectedExit":0},
    {"id":"post-bazel-assets","platforms":["macos-x86_64","windows-11-x64"],"phase":"postOnly","argv":["bazel","build","//ai-ip-assets/skills/deliver-ai-ip-content-package:skill","//ai-ip-evals/rubrics:rubrics","//ai-ip-evals/schemas:schemas"],"expectedExit":0}
  ]
}
```

- [ ] **Step 4: Implement the strict matrix value types and loader**

The implementation must reject duplicate JSON keys before normal deserialization and must not accept `bool` where an integer is required.

```python
@dataclass(frozen=True)
class CommandSpec:
    id: str
    platforms: tuple[str, ...]
    phase: str
    argv: tuple[str, ...]
    expected_exit: int

@dataclass(frozen=True)
class Matrix:
    schema_version: int
    commands: tuple[CommandSpec, ...]

    def required_for(self, platform: str, mode: str) -> tuple[CommandSpec, ...]:
        if mode not in {"baseline", "post"}:
            raise EvidenceError(f"unknown evidence mode: {mode}")
        phases = {"baselineAndPost"} if mode == "baseline" else {"baselineAndPost", "postOnly"}
        return tuple(
            command
            for command in self.commands
            if platform in command.platforms and command.phase in phases
        )

def canonical_json_bytes(value: object) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
        allow_nan=False,
    ).encode("utf-8")

def selection_argv(command: CommandSpec) -> tuple[str, ...] | None:
    if command.argv[:2] != ("just", "test") or "-E" not in command.argv:
        return None
    return ("cargo", "nextest", "list", "--message-format", "json", *command.argv[2:])
```

`host_id()` must return only `macos-x86_64` for `(platform.system(), platform.machine()) == ("Darwin", "x86_64")` and `windows-11-x64` for native 64-bit Windows 11. It rejects every other tuple. On macOS it also requires `sysctl -n sysctl.proc_translated` to be absent or `0`; value `1` rejects Rosetta.

- [ ] **Step 5: Run the Task 1 GREEN and full existing foundation tests**

Run:

```bash
PYTHONDONTWRITEBYTECODE=1 uv run --python 3.11 --with pytest==8.3.5 \
  pytest -q -p no:cacheprovider \
  scripts/ai_ip/foundation/test_capture_command.py \
  scripts/ai_ip/foundation/test_verify_upstream_lock.py
```

Expected: all tests pass; the upstream-lock suite remains `25 passed` within the combined total.

- [ ] **Step 6: Format, verify scope, and commit the frozen matrix boundary**

```bash
PATH="/Users/yangyucheng/.rustup/toolchains/1.95.0-x86_64-apple-darwin/bin:$PATH" just fmt
git diff --check
test -z "$(git diff --name-only 4ef1d4b89bd419c976b04fefa0fd36844e898340..HEAD -- codex-rs)"
git add -- \
  scripts/ai_ip/foundation/capture_command.py \
  scripts/ai_ip/foundation/test_capture_command.py \
  scripts/ai_ip/foundation/required_command_matrix.json
git diff --cached --check
git commit -m "test: freeze native Codex command matrix"
python3 scripts/ai_ip/foundation/verify_upstream_lock.py --repo "$PWD"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
```

Expected: clean commit containing exactly the three paths above.

---

### Task 2: Implement the streaming command recorder and bootstrap manifest

**Files:**
- Modify: `scripts/ai_ip/foundation/capture_command.py`
- Modify: `scripts/ai_ip/foundation/test_capture_command.py`

**Interfaces:**
- Consumes: `Matrix`, `CommandSpec`, `selection_argv`, and the committed matrix.
- Produces CLI modes:
  - command capture: `capture_command.py --repo-root ABS --evidence-dir ABS --matrix ABS --name ID`
  - bootstrap capture: `capture_command.py --repo-root ABS --evidence-dir ABS --matrix ABS --bootstrap-tools-json ABS`
- Produces `<id>.{stdout,stderr}.log`, optional `<id>.selection.{stdout,stderr}.log`, `<id>.manifest.json`, `dependency-install.{stdout,stderr}.log`, and `host-bootstrap.manifest.json` using create-new semantics.

- [ ] **Step 1: Add recorder RED tests for the full failure surface**

Use temporary Git repositories and executable Python fixtures. Tests must cover these exact cases:

```python
@dataclass
class RecorderHarness:
    repo: Path
    evidence: Path
    matrix: Path

    @classmethod
    def create(cls, tmp_path: Path) -> "RecorderHarness":
        repo = initialize_clean_fixture_repo(tmp_path / "tested")
        evidence = (tmp_path / "evidence").resolve()
        evidence.mkdir()
        matrix = write_fixture_matrix(tmp_path / "matrix.json")
        return cls(repo=repo, evidence=evidence, matrix=matrix)

    def run(self, *extra: str) -> subprocess.CompletedProcess[bytes]:
        return subprocess.run(
            [sys.executable, str(CAPTURE_SCRIPT), "--repo-root", str(self.repo),
             "--evidence-dir", str(self.evidence), "--matrix", str(self.matrix), *extra],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )

@pytest.mark.parametrize(
    "invalid_input",
    ["relative_repo", "relative_evidence", "relative_matrix", "dirty_repo", "evidence_inside_repo", "repo_inside_evidence", "symlink_parent", "unknown_command", "wrong_platform", "preexisting_output"],
)
def test_capture_rejects_invalid_boundaries(invalid_input: str, tmp_path: Path) -> None:
    harness = RecorderHarness.create(tmp_path)
    apply_invalid_boundary(harness, invalid_input)
    completed = harness.run("--name", "fixture-command")
    assert completed.returncode == 1
    assert not list(harness.evidence.glob("fixture-command.*"))

def test_capture_streams_binary_stdout_and_stderr_and_preserves_exit_code(tmp_path: Path) -> None:
    harness = RecorderHarness.create(tmp_path)
    expected_stdout, expected_stderr = install_binary_stream_fixture(harness.repo, exit_code=7)
    completed = harness.run("--name", "fixture-command")
    manifest = load_strict_json(harness.evidence / "fixture-command.manifest.json")
    assert completed.returncode == 7
    assert manifest["exitCode"] == 7
    assert manifest["stdout"]["bytes"] == len(expected_stdout)
    assert manifest["stdout"]["sha256"] == hashlib.sha256(expected_stdout).hexdigest()
    assert manifest["stderr"]["bytes"] == len(expected_stderr)
    assert manifest["stderr"]["sha256"] == hashlib.sha256(expected_stderr).hexdigest()

@pytest.mark.parametrize("test_count", [0, 2])
def test_filtered_command_requires_exactly_one_listed_test(test_count: int, tmp_path: Path) -> None:
    harness = RecorderHarness.create(tmp_path)
    marker = install_filtered_fixture(harness.repo, test_count=test_count)
    completed = harness.run("--name", "filtered-fixture")
    assert completed.returncode == 1
    assert not marker.exists()

def test_filtered_command_records_selection_before_execution(tmp_path: Path) -> None:
    harness = RecorderHarness.create(tmp_path)
    marker = install_filtered_fixture(harness.repo, test_count=1)
    completed = harness.run("--name", "filtered-fixture")
    manifest = load_strict_json(harness.evidence / "filtered-fixture.manifest.json")
    assert completed.returncode == 0
    assert marker.read_text(encoding="utf-8") == "executed\n"
    assert manifest["selection"]["testCount"] == 1

def test_capture_rejects_dirty_tree_after_command(tmp_path: Path) -> None:
    harness = RecorderHarness.create(tmp_path)
    install_dirtying_fixture(harness.repo)
    completed = harness.run("--name", "fixture-command")
    assert completed.returncode == 1
    assert b"tested tree became dirty" in completed.stderr

def test_capture_never_overwrites_or_leaves_final_partial_files(tmp_path: Path) -> None:
    harness = RecorderHarness.create(tmp_path)
    install_success_fixture(harness.repo)
    assert harness.run("--name", "fixture-command").returncode == 0
    first_manifest = (harness.evidence / "fixture-command.manifest.json").read_bytes()
    assert harness.run("--name", "fixture-command").returncode == 1
    assert (harness.evidence / "fixture-command.manifest.json").read_bytes() == first_manifest
    assert not list(harness.evidence.glob("*.tmp-*"))

def test_bootstrap_manifest_requires_exact_tool_set_and_records_hashes(tmp_path: Path) -> None:
    harness = RecorderHarness.create(tmp_path)
    tools_json = write_exact_fake_tools(tmp_path / "tools.json")
    completed = harness.run("--bootstrap-tools-json", str(tools_json))
    manifest = load_strict_json(harness.evidence / "host-bootstrap.manifest.json")
    assert completed.returncode == 0
    assert set(manifest["tools"]) == REQUIRED_TOOL_NAMES
    assert all("executableSha256" in receipt for receipt in manifest["tools"].values())
    assert manifest["dependencyInstall"]["argv"] == ["pnpm", "install", "--frozen-lockfile"]
    assert manifest["dependencyInstall"]["exitCode"] == 0
```

Implement the five named fixture helpers above in the same test file; each accepts only its declared path/value inputs and returns the concrete paths or bytes asserted by the test. The binary-stream fixture writes at least 1 MiB plus non-UTF-8 bytes to each stream, exits `7`, and asserts byte count, SHA-256, and `exitCode == 7`. It must demonstrate with a patched reader that every `read()` request is at most 1 MiB, so the recorder never decodes or requests the complete streams at once. The bootstrap fixture supplies exact absolute executables for `python`, `uv`, `git`, `just`, `dotslash`, `rustc`, `cargo`, `cargo-nextest`, `cargo-deny`, `node`, `pnpm`, `bazelisk`, and `bazel` through an exact JSON object; separate tests remove one tool and add one unknown tool and require exit `1` before any final manifest exists.

- [ ] **Step 2: Run recorder RED**

```bash
PYTHONDONTWRITEBYTECODE=1 uv run --python 3.11 --with pytest==8.3.5 \
  pytest -q -p no:cacheprovider scripts/ai_ip/foundation/test_capture_command.py \
  -k 'capture or bootstrap or filtered'
```

Expected: FAIL because command execution, streaming logs, atomic publication, and bootstrap mode are not implemented. `DID NOT RAISE` for an intended rejection is an accepted RED; fixture syntax failure is not.

- [ ] **Step 3: Implement path, Git, and tools-tree isolation**

Reuse the hardened Git environment shape from `verify_upstream_lock.py`, but keep helpers private to this file. Before any capture:

```python
def require_absolute_directory(raw: str, label: str) -> Path:
    lexical = Path(raw)
    if not lexical.is_absolute():
        raise EvidenceError(f"{label} must be absolute")
    resolved = lexical.resolve(strict=True)
    if not resolved.is_dir() or lexical.is_symlink():
        raise EvidenceError(f"{label} must be a real directory")
    return resolved

def paths_are_disjoint(left: Path, right: Path) -> bool:
    return left != right and left not in right.parents and right not in left.parents
```

Require `repo-root` to be a Git top-level with a clean index/worktree, 40-lowercase-hex HEAD, no shallow/promisor/replace/graft state, and exact lock files. Resolve the tools repository from the non-symlink `__file__`, require it clean, require `matrix` to be a tracked stage-0 `100644` blob inside that tools repository, and require tools tree, tested tree, and evidence tree to be pairwise disjoint.

- [ ] **Step 4: Implement binary streaming and atomic create-new publication**

Use `subprocess.Popen(..., stdout=PIPE, stderr=PIPE, shell=False, cwd=repo_root, env=sanitized_env)`. Each reader thread repeatedly reads `1024 * 1024` byte chunks, writes to a same-directory exclusive temporary file, updates `hashlib.sha256`, counts bytes, flushes, and `os.fsync`s. Join both readers, wait for the real process, fsync the evidence directory, then publish logs followed by the manifest. No final destination may pre-exist; on any exception remove only this invocation's temp files and never overwrite evidence.

```python
@dataclass(frozen=True)
class StreamReceipt:
    path: str
    sha256: str
    bytes: int

@dataclass(frozen=True)
class ProcessReceipt:
    argv: tuple[str, ...]
    exit_code: int
    started_at: str
    ended_at: str
    stdout: StreamReceipt
    stderr: StreamReceipt

def canonical_rfc3339_utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="microseconds").replace("+00:00", "Z")
```

For selection preflight derive `(cargo, nextest, list, --message-format, json, *command.argv[2:])`, record both selection streams, parse stdout with `json.loads`, require exact root keys containing integer `test-count`, and require `type(test_count) is int and test_count == 1`. If selection fails, publish a command manifest with `status="BLOCKED_SELECTION"`, do not run the main command, and exit nonzero.

- [ ] **Step 5: Implement exact command and bootstrap manifests**

Command manifests have exactly these top-level keys:

```python
COMMAND_MANIFEST_KEYS = {
    "schemaVersion", "commandId", "phase", "argv", "expectedExit", "exitCode",
    "status", "startedAt", "endedAt", "platform", "architecture", "testedGitSha",
    "toolsGitSha", "recorderSha256", "matrixSha256", "locks", "stdout", "stderr",
    "selection",
}
```

`status` is `PASS` only when exit code equals expected; otherwise `BLOCKED_BASELINE`. `selection` is JSON null for ordinary commands or an exact object containing `argv`, `exitCode`, `testCount`, `stdout`, and `stderr`. Lock keys are exactly `cargoLockSha256`, `pnpmLockSha256`, and `bazelLockSha256`.

`host-bootstrap.manifest.json` has exactly `schemaVersion`, `platform`, `architecture`, `osVersion`, `osBuild`, `testedGitSha`, `toolsGitSha`, `recorderSha256`, `matrixSha256`, `locks`, `tools`, `dependencyInstall`, `createdAt`, `administratorToken`. Each tool value contains only `versionArgv`, `versionExitCode`, `versionStdoutSha256`, `versionStderrSha256`, `executableSha256`, and `executableBytes`; no absolute executable path is stored. `dependencyInstall` contains exactly `argv`, `exitCode`, `startedAt`, `endedAt`, `stdout`, and `stderr`, where the two stream receipts use the same shape as command manifests. Bootstrap mode itself runs exactly `pnpm install --frozen-lockfile` with the supplied pinned `pnpm` executable, records both logs, and rejects a nonzero exit or any tested-tree dirt before publishing the manifest. `administratorToken` is JSON null on macOS.

- [ ] **Step 6: Run recorder GREEN and all foundation Python tests**

```bash
PYTHONDONTWRITEBYTECODE=1 uv run --python 3.11 --with pytest==8.3.5 \
  pytest -q -p no:cacheprovider \
  scripts/ai_ip/foundation/test_capture_command.py \
  scripts/ai_ip/foundation/test_verify_upstream_lock.py
```

Expected: PASS, including byte-for-byte stream receipts and all fail-closed path cases.

- [ ] **Step 7: Format and commit the recorder**

```bash
PATH="/Users/yangyucheng/.rustup/toolchains/1.95.0-x86_64-apple-darwin/bin:$PATH" just fmt
git diff --check
git add -- scripts/ai_ip/foundation/capture_command.py scripts/ai_ip/foundation/test_capture_command.py
git diff --cached --check
git commit -m "test: add native Codex evidence recorder"
python3 scripts/ai_ip/foundation/verify_upstream_lock.py --repo "$PWD"
test -z "$(git diff --name-only 4ef1d4b89bd419c976b04fefa0fd36844e898340..HEAD -- codex-rs)"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
```

---

### Task 3: Implement baseline/post and frozen-final public evidence verification

**Files:**
- Create: `scripts/ai_ip/foundation/verify_evidence.py`
- Create: `scripts/ai_ip/foundation/test_verify_evidence.py`

**Interfaces:**
- Consumes: committed matrix plus bootstrap/command/log evidence.
- Produces baseline/post CLI: `verify_evidence.py --repo-root ABS --matrix ABS --evidence-root ABS --platform macos-x86_64|windows-11-x64 --mode baseline|post`.
- Produces frozen final CLI with exactly: `--candidate-sha`, `--selected-report`, `--report-index`, `--business-verification-receipt`, `--verification-output` plus the public foundation inputs above.
- Writes final output with create-new semantics; baseline/post modes are read-only.

- [ ] **Step 1: Write tamper and completeness RED tests**

Build valid synthetic evidence through the real recorder, then parameterize one mutation per test:

```python
@pytest.mark.parametrize(
    "mutation",
    [
        "missing_bootstrap", "missing_manifest", "missing_stdout", "missing_stderr",
        "extra_manifest", "log_bytes", "log_sha", "matrix_id", "matrix_argv",
        "matrix_expected_exit", "wrong_platform", "wrong_arch", "tested_sha",
        "tools_sha", "recorder_sha", "cargo_lock", "pnpm_lock", "bazel_lock",
        "unknown_status", "post_only_in_baseline", "unpaired_post_failure",
    ],
)
def test_public_evidence_tampering_is_rejected(mutation: str, tmp_path: Path) -> None:
    fixture = ValidEvidenceFixture.create(tmp_path)
    fixture.apply_mutation(mutation)
    with pytest.raises(EvidenceError):
        verify_evidence(fixture.request)
```

`ValidEvidenceFixture.create` must invoke the real recorder for every required synthetic command and return a valid `VerificationRequest`; `apply_mutation` implements each listed mutation as one exact filesystem or JSON change and rejects unknown mutation names. Add a separate positive test asserting the full `EvidenceDisposition` value and verifier CLI exit codes `0`, `1`, and `2`.

Positive baseline requires the exact platform command set and permits recorded `BLOCKED_BASELINE` while returning an overall `blocked` disposition; it never silently passes a nonzero required item. Post mode requires all `baselineAndPost + postOnly` items and accepts a post failure only if its ID exactly exists in baseline disposition; any new or unknown post failure is rejected.

- [ ] **Step 2: Write frozen final CLI RED tests**

Tests freeze exact CLI names and public fields. They must reject repeated/multiple selected reports, report/index/candidate mismatch, receipt fields outside the allowlist, any field or value containing an absolute path, bearer/key/secret, thread/response/reviewer ID, private root, prompt/output body, NaN/Infinity, or duplicate JSON key. `argparse` must reject `--private-run-root`, key, attestation, and ignore-failure options because they are not defined.

```python
FINAL_OUTPUT_KEYS = {
    "schemaVersion", "publicRunId", "candidateSha", "reportPath", "reportSha256",
    "reportIndexSha256", "selectedAttemptOrdinal", "proofRootSha256",
    "frozenRunContextCommitment", "executionContextCommitment",
    "attemptIndexRootCommitment", "brokerReceiptCommitment", "costReceiptsCommitment",
    "requiredMatrixSha256", "macPostEvidenceCommitment", "windowsPostEvidenceCommitment",
    "candidateAllowedDiffSha256", "liveProofVerificationSha256", "G0", "G1", "G2",
    "capabilityStatus", "foundationDecision", "verifierSha256", "verifiedAt",
    "verifiedBeforeRetentionDeadline",
}
```

The Python verifier treats the Rust business verification receipt as opaque-but-exact public evidence: it verifies the allowlisted object, digests, candidate/report/index bindings, and copies only the fields above. It does not read a private run root, commitment key, case, transcript, provider secret, or review material and does not recompute business quality.

- [ ] **Step 3: Run verifier RED**

```bash
PYTHONDONTWRITEBYTECODE=1 uv run --python 3.11 --with pytest==8.3.5 \
  pytest -q -p no:cacheprovider scripts/ai_ip/foundation/test_verify_evidence.py
```

Expected: FAIL with `ModuleNotFoundError` or missing verifier entry points, not fixture syntax failure.

- [ ] **Step 4: Implement exact baseline/post verification**

Implement strict duplicate-key JSON loading, whole-object key allowlists, lower-hex digest validation, RFC3339 UTC ordering, relative safe-basename checks, and chunked log/hash recomputation. Recompute matrix semantic SHA, recorder blob from `toolsGitSha:scripts/ai_ip/foundation/capture_command.py`, lock blobs from `testedGitSha`, and require every tools/tested SHA to be a real commit without replacements/grafts.

The returned receipt is an immutable value:

```python
@dataclass(frozen=True)
class EvidenceDisposition:
    platform: str
    mode: str
    tested_git_sha: str
    tools_git_sha: str
    matrix_sha256: str
    command_ids: tuple[str, ...]
    blocked_ids: tuple[str, ...]

    @property
    def status(self) -> str:
        return "PASS" if not self.blocked_ids else "BLOCKED_BASELINE"
```

CLI stdout prints one compact JSON object containing this disposition and exits `0` only for `PASS`; it exits `2` for valid but blocked evidence and `1` for invalid evidence. No `--ignore-*` argument exists.

- [ ] **Step 5: Implement create-new frozen final output**

Require all five final arguments together or none. Final mode first verifies macOS and Windows post evidence, then verifies a single report selected by an append-only report index and an exact public Rust receipt. The final output contains exactly `FINAL_OUTPUT_KEYS`, is canonical JSON plus LF, and is written to an absolute path outside the repository by exclusive temp/create-new + fsync + atomic publication. An existing output, symlink, path inside the repository, or parent reparse/symlink is rejected.

- [ ] **Step 6: Run verifier GREEN and regression tests**

```bash
PYTHONDONTWRITEBYTECODE=1 uv run --python 3.11 --with pytest==8.3.5 \
  pytest -q -p no:cacheprovider \
  scripts/ai_ip/foundation/test_verify_evidence.py \
  scripts/ai_ip/foundation/test_capture_command.py \
  scripts/ai_ip/foundation/test_verify_upstream_lock.py
```

Expected: PASS.

- [ ] **Step 7: Format and commit the public verifier**

```bash
PATH="/Users/yangyucheng/.rustup/toolchains/1.95.0-x86_64-apple-darwin/bin:$PATH" just fmt
git diff --check
git add -- scripts/ai_ip/foundation/verify_evidence.py scripts/ai_ip/foundation/test_verify_evidence.py
git diff --cached --check
git commit -m "test: verify native Codex evidence"
python3 scripts/ai_ip/foundation/verify_upstream_lock.py --repo "$PWD"
test -z "$(git diff --name-only 4ef1d4b89bd419c976b04fefa0fd36844e898340..HEAD -- codex-rs)"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
```

---

### Task 4: Freeze evaluator dispatch without implementing business evaluation

**Files:**
- Create: `scripts/ai_ip/foundation/run_frozen_eval.py`
- Create: `scripts/ai_ip/foundation/test_run_frozen_eval.py`

**Interfaces:**
- Consumes CLI: `run_frozen_eval.py --context ABS -- SUBCOMMAND [args...]`.
- Consumes the frozen wrapper projection from the larger future context: root `schemaVersion`, `executionMode`, `candidateSha`, `privateRoot`, and `frozenEvaluator`; replay additionally requires `fixtureSetSha256`, while live additionally requires `providerRole`, `providerBudgetEvidenceSha256`, and `retentionDeadline`.
- Produces no manifest and no business result; after verification it calls direct `os.execve` with one appended `--frozen-run-context ABS`.

- [ ] **Step 1: Write replay/live and authority RED tests**

Create one valid replay and one valid live context in temporary directories. Each uses a detached clean evaluator worktree and a small executable child whose SHA is frozen. Cover:

```python
@pytest.mark.parametrize(
    "mutation",
    [
        "relative_context", "context_symlink", "bad_schema_version", "bad_mode",
        "cross_mode_field", "wrong_platform", "windows_without_exe", "candidate_mismatch",
        "worktree_dirty", "worktree_head_drift", "binary_hash_drift", "binary_symlink",
        "duplicate_frozen_context", "override_model", "override_provider", "override_case",
        "override_binary", "override_budget", "override_timeout", "override_limit",
    ],
)
def test_frozen_eval_rejects_authority_drift(mutation: str, tmp_path: Path) -> None:
    fixture = FrozenEvalFixture.create(tmp_path, execution_mode="replay")
    fixture.apply_mutation(mutation)
    completed = fixture.run_wrapper()
    assert completed.returncode == 1
    assert not fixture.child_receipt.exists()
```

`FrozenEvalFixture.create` initializes the detached clean evaluator worktree, writes the fully keyed replay/live context, and installs the executable child; `apply_mutation` performs one listed authority drift and rejects unknown names. The two positive tests spawn the wrapper as a subprocess and assert the child receives its original argv plus exactly one `--frozen-run-context` and the canonical absolute context path.

- [ ] **Step 2: Run wrapper RED**

```bash
PYTHONDONTWRITEBYTECODE=1 uv run --python 3.11 --with pytest==8.3.5 \
  pytest -q -p no:cacheprovider scripts/ai_ip/foundation/test_run_frozen_eval.py
```

Expected: FAIL because the wrapper does not exist.

- [ ] **Step 3: Implement the exact context and forbidden-flag contracts**

The wrapper accepts no environment-derived authority. It clears ambient Git variables for worktree verification and compares raw SHA-256 for the platform-selected evaluator. It requires `schemaVersion == 1` with integer-not-boolean type and rejects unknown keys inside the exact `frozenEvaluator` object, but it permits additional root fields because Work Package 6 owns and will strictly validate the larger typed context. `executionMode="replay"` requires `fixtureSetSha256` and forbids `providerRole`, `providerBudgetEvidenceSha256`, and `retentionDeadline`; live requires those three and forbids `fixtureSetSha256`. `providerRole` is exactly `targetVolcengine` or `approvedReference`; the wrapper validates only the required string/digest/RFC3339 shapes and never interprets provider semantics.

```python
FROZEN_EVALUATOR_KEYS = {
    "gitSha", "worktree",
    "macosX8664Binary", "macosX8664BinarySha256",
    "windows11X64Binary", "windows11X64BinarySha256",
}
```

All six keys are present. The non-current platform path and hash remain frozen strings but are not opened; the current platform pair is canonicalized, hashed, and executed. This projection is the compatibility seam that the later Rust frozen-context schema must embed unchanged.

```python
FORBIDDEN_CHILD_FLAGS = {
    "--frozen-run-context", "--model", "--model-id", "--provider", "--provider-role",
    "--endpoint", "--case", "--case-fixture", "--binary", "--evaluator", "--codex-bin",
    "--budget", "--budget-fen", "--limit", "--attempt-limit", "--timeout",
    "--max-output-tokens",
}

def reject_authority_overrides(argv: tuple[str, ...]) -> None:
    for token in argv:
        name = token.split("=", 1)[0]
        if name in FORBIDDEN_CHILD_FLAGS:
            raise FrozenEvalError(f"child argv overrides frozen authority: {name}")
```

Require `privateRoot`, evaluator worktree, evaluator binary, and context to be absolute, canonical, pairwise correctly contained, regular non-symlink paths. Reject dirty evaluator Git state, wrong HEAD, shallow/replace/graft metadata, non-`.exe` Windows evaluator, and host mismatch. Call `os.execve(executable, final_argv, sanitized_environment)` directly; never use a shell.

- [ ] **Step 4: Run wrapper GREEN and all foundation tests**

```bash
PYTHONDONTWRITEBYTECODE=1 uv run --python 3.11 --with pytest==8.3.5 \
  pytest -q -p no:cacheprovider \
  scripts/ai_ip/foundation/test_run_frozen_eval.py \
  scripts/ai_ip/foundation/test_verify_evidence.py \
  scripts/ai_ip/foundation/test_capture_command.py \
  scripts/ai_ip/foundation/test_verify_upstream_lock.py
```

Expected: PASS.

- [ ] **Step 5: Write the evidence README**

Document exact layout, immutable/create-new rule, three-tree isolation, the 30-entry matrix, `PASS` versus `BLOCKED_BASELINE`, tools/tested SHA binding, Windows two-ref transfer and later return child, forced staging of only ignored stdout/stderr logs, absence of provider calls, and rollback. State explicitly that manifest host labels plus bootstrap hashes establish a reproducible provenance claim but cannot cryptographically prove that an untrusted remote machine is physical Windows.

- [ ] **Step 6: Format and commit the frozen wrapper/documentation boundary**

```bash
PATH="/Users/yangyucheng/.rustup/toolchains/1.95.0-x86_64-apple-darwin/bin:$PATH" just fmt
git diff --check
git add -- \
  scripts/ai_ip/foundation/run_frozen_eval.py \
  scripts/ai_ip/foundation/test_run_frozen_eval.py \
  docs/evidence/foundation/README.md
git diff --cached --check
git commit -m "test: freeze native evaluation evidence boundary"
python3 scripts/ai_ip/foundation/verify_upstream_lock.py --repo "$PWD"
test -z "$(git diff --name-only 4ef1d4b89bd419c976b04fefa0fd36844e898340..HEAD -- codex-rs)"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
```

---

### Task 5: Seal the tools SHA and create the verified two-ref transfer bundle

**Files:**
- No tracked file changes.
- Create outside Git: `ai-ip-baseline.bundle`, `ai-ip-baseline.bundle.sha256` in a fresh `mktemp -d` directory.
- Create local refs: `refs/heads/ai-ip-transfer/pinned-codex`, `refs/heads/ai-ip-transfer/foundation-tools`.

**Interfaces:**
- Produces immutable `AI_IP_TOOLS_SHA` consumed by every baseline/post manifest and the later Windows child.
- Produces a bundle containing exactly the pinned Codex ref and tools ref.

- [ ] **Step 1: Run the complete Python foundation suite on the candidate tools SHA**

```bash
PYTHONDONTWRITEBYTECODE=1 uv run --python 3.11 --with pytest==8.3.5 \
  pytest -q -p no:cacheprovider scripts/ai_ip/foundation/test_*.py
PATH="/Users/yangyucheng/.rustup/toolchains/1.95.0-x86_64-apple-darwin/bin:$PATH" just fmt-check
python3 scripts/ai_ip/foundation/verify_upstream_lock.py --repo "$PWD"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
```

Expected: all foundation tests and both verifiers pass; tree remains clean.

- [ ] **Step 2: Capture tools SHA and make detached tools/tested worktrees**

```bash
set -euo pipefail
AI_IP_TOOLS_SHA="$(git rev-parse HEAD)"
AI_IP_NATIVE_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/ai-ip-native-macos.XXXXXX")"
AI_IP_TOOLS_WORKTREE="$AI_IP_NATIVE_ROOT/tools"
AI_IP_NATIVE_WORKTREE="$AI_IP_NATIVE_ROOT/pinned-codex"
git worktree add --detach "$AI_IP_TOOLS_WORKTREE" "$AI_IP_TOOLS_SHA"
git worktree add --detach "$AI_IP_NATIVE_WORKTREE" 4ef1d4b89bd419c976b04fefa0fd36844e898340
test "$(git -C "$AI_IP_TOOLS_WORKTREE" rev-parse HEAD)" = "$AI_IP_TOOLS_SHA"
test "$(git -C "$AI_IP_NATIVE_WORKTREE" rev-parse HEAD)" = 4ef1d4b89bd419c976b04fefa0fd36844e898340
test -z "$(git -C "$AI_IP_TOOLS_WORKTREE" status --porcelain=v1 --untracked-files=all)"
test -z "$(git -C "$AI_IP_NATIVE_WORKTREE" status --porcelain=v1 --untracked-files=all)"
```

Expected: two disjoint detached, clean worktrees.

- [ ] **Step 3: Create and verify the named-ref bundle**

```bash
set -euo pipefail
AI_IP_BUNDLE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/ai-ip-baseline-bundle.XXXXXX")"
git update-ref refs/heads/ai-ip-transfer/pinned-codex 4ef1d4b89bd419c976b04fefa0fd36844e898340
git update-ref refs/heads/ai-ip-transfer/foundation-tools "$AI_IP_TOOLS_SHA"
git bundle create "$AI_IP_BUNDLE_DIR/ai-ip-baseline.bundle" \
  refs/heads/ai-ip-transfer/pinned-codex \
  refs/heads/ai-ip-transfer/foundation-tools
git bundle verify "$AI_IP_BUNDLE_DIR/ai-ip-baseline.bundle"
test "$(git bundle list-heads "$AI_IP_BUNDLE_DIR/ai-ip-baseline.bundle" | wc -l | tr -d ' ')" = 2
git bundle list-heads "$AI_IP_BUNDLE_DIR/ai-ip-baseline.bundle" | \
  grep -F "4ef1d4b89bd419c976b04fefa0fd36844e898340 refs/heads/ai-ip-transfer/pinned-codex"
git bundle list-heads "$AI_IP_BUNDLE_DIR/ai-ip-baseline.bundle" | \
  grep -F "$AI_IP_TOOLS_SHA refs/heads/ai-ip-transfer/foundation-tools"
shasum -a 256 "$AI_IP_BUNDLE_DIR/ai-ip-baseline.bundle" > \
  "$AI_IP_BUNDLE_DIR/ai-ip-baseline.bundle.sha256"
```

Expected: bundle verifies and advertises exactly two named refs. Record the absolute external bundle directory, bundle SHA-256, and `AI_IP_TOOLS_SHA` in the execution handoff; do not commit the bundle.

---

### Task 6: Bootstrap an isolated, exact macOS x86_64 toolchain and record it

**Files:**
- Generate: `docs/evidence/foundation/macos-x86_64/host-bootstrap.manifest.json`
- Generate: tool-version stdout/stderr logs beneath the same directory.

**Interfaces:**
- Consumes detached tools/tested worktrees and `AI_IP_TOOLS_SHA` from Task 5.
- Produces exact tool paths JSON for recorder bootstrap mode and a validated host manifest.

- [ ] **Step 1: Create a task-specific external tool root and expose Rust 1.95.0**

Do not repurpose `$HOME`, `$CODEX_HOME`, or global PATH persistently. The already installed host `uv` is only a bootstrap downloader; capture its version and executable SHA in the execution handoff, but do not treat it as an evidence tool. All evidence CLIs and pytest run with the isolated CPython 3.11.15 and pinned `uv` 0.11.3 created below.

```bash
set -euo pipefail
AI_IP_TOOL_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/ai-ip-native-tools.XXXXXX")"
AI_IP_CARGO_ROOT="$AI_IP_TOOL_ROOT/cargo-tools"
AI_IP_NPM_ROOT="$AI_IP_TOOL_ROOT/npm-tools"
AI_IP_PYTHON_ROOT="$AI_IP_TOOL_ROOT/python-3.11.15"
AI_IP_UV_ROOT="$AI_IP_TOOL_ROOT/uv-0.11.3"
AI_IP_RUST_BIN="/Users/yangyucheng/.rustup/toolchains/1.95.0-x86_64-apple-darwin/bin"
test "$("$AI_IP_RUST_BIN/rustc" --version | awk '{print $2}')" = 1.95.0
UV_PYTHON_INSTALL_DIR="$AI_IP_PYTHON_ROOT" uv python install 3.11.15
UV_PYTHON_INSTALL_DIR="$AI_IP_PYTHON_ROOT" uv venv --python 3.11.15 "$AI_IP_UV_ROOT"
UV_PYTHON_INSTALL_DIR="$AI_IP_PYTHON_ROOT" uv pip install \
  --python "$AI_IP_UV_ROOT/bin/python" uv==0.11.3
"$AI_IP_RUST_BIN/cargo" install --root "$AI_IP_CARGO_ROOT" --locked cargo-nextest --version 0.9.103
"$AI_IP_RUST_BIN/cargo" install --root "$AI_IP_CARGO_ROOT" --locked cargo-deny --version 0.20.2
npm install --prefix "$AI_IP_NPM_ROOT" --ignore-scripts --package-lock=false \
  pnpm@10.34.5 fb-dotslash@0.5.8 @bazel/bazelisk@1.28.1
```

Expected: exact versions install under the task-specific temporary root. Installation network/cache failures are bootstrap failures, not Codex baseline failures.

- [ ] **Step 2: Set the process-local PATH and verify every exact version**

```bash
set -euo pipefail
export PATH="$AI_IP_UV_ROOT/bin:$AI_IP_CARGO_ROOT/bin:$AI_IP_NPM_ROOT/node_modules/.bin:$AI_IP_RUST_BIN:/usr/local/bin:/usr/bin:/bin"
test "$(uv --version)" = "uv 0.11.3"
test "$(python --version)" = "Python 3.11.15"
test "$(cargo --version | awk '{print $2}')" = 1.95.0
test "$(rustc --version | awk '{print $2}')" = 1.95.0
test "$(cargo nextest --version | awk '{print $2}')" = 0.9.103
test "$(cargo deny --version | awk '{print $2}')" = 0.20.2
test "$(pnpm --version)" = 10.34.5
test "$(dotslash --version | awk '{print $2}')" = 0.5.8
test "$(bazelisk version | sed -n 's/^Bazelisk version: v//p')" = 1.28.1
test "$(bazel --version | awk '{print $2}')" = 9.0.0
test "$(just --version | awk '{print $2}')" = 1.58.0
test "$(node --version)" = v26.4.0
```

Every comparison above is exact. If any command prints a different value, stop as `BLOCKED_BOOTSTRAP`, retain only the external install logs, and amend this reviewed plan before changing a parser or accepted version.

- [ ] **Step 3: Prepare isolated build roots without mutating the tested tree**

```bash
set -euo pipefail
export CARGO_TARGET_DIR="$AI_IP_NATIVE_ROOT/cargo-target"
export CARGO_HOME="$AI_IP_NATIVE_ROOT/cargo-home"
mkdir -p "$CARGO_TARGET_DIR" "$CARGO_HOME"
test -z "$(git -C "$AI_IP_NATIVE_WORKTREE" status --porcelain=v1 --untracked-files=all)"
```

Expected: build/cache roots are outside the tested tree and the tested tree is clean. The recorder's bootstrap mode owns the exact dependency-install command and its receipt in the next step.

- [ ] **Step 4: Generate the exact bootstrap-tools JSON outside Git and capture the host manifest**

The JSON contains exactly the absolute paths for the 13 tools listed in Task 2. Keep `CARGO_TARGET_DIR` and `CARGO_HOME` pointed at the two external directories from Step 3, then invoke:

```bash
"$AI_IP_UV_ROOT/bin/python" "$AI_IP_TOOLS_WORKTREE/scripts/ai_ip/foundation/capture_command.py" \
  --repo-root "$AI_IP_NATIVE_WORKTREE" \
  --evidence-dir "$PWD/docs/evidence/foundation/macos-x86_64" \
  --matrix "$AI_IP_TOOLS_WORKTREE/scripts/ai_ip/foundation/required_command_matrix.json" \
  --bootstrap-tools-json "$AI_IP_BOOTSTRAP_TOOLS_JSON"
```

Expected: bootstrap mode runs and records exact `pnpm install --frozen-lockfile`, creates the manifest with `platform=macos`, `architecture=x86_64`, exact pinned/tool SHAs and exact tool hashes, and proves tested and tools trees remain clean.

---

### Task 7: Capture the unchanged macOS focused baseline

**Files:**
- Generate: `docs/evidence/foundation/macos-x86_64/<command-id>.manifest.json`
- Generate: matching stdout/stderr and selection logs.
- Generate: `docs/evidence/foundation/macos-x86_64/baseline-summary.json`

**Interfaces:**
- Consumes exact matrix/tools/bootstrap from Tasks 1–6.
- Produces 20 command manifests and one verifier disposition; no runtime changes.

- [ ] **Step 1: Run each macOS baseline command only through the recorder**

```bash
set -euo pipefail
AI_IP_RECORDER="$AI_IP_TOOLS_WORKTREE/scripts/ai_ip/foundation/capture_command.py"
AI_IP_COMMAND_MATRIX="$AI_IP_TOOLS_WORKTREE/scripts/ai_ip/foundation/required_command_matrix.json"
AI_IP_EVIDENCE_DIR="$PWD/docs/evidence/foundation/macos-x86_64"
for AI_IP_MATRIX_ID in \
  fmt-check app-server-protocol app-server-transport state thread-store \
  app-server-process thread-start thread-resume executor-skill mcp-tool process-exec fs \
  apply-patch build-cli-app-server cargo-metadata cargo-license-source \
  pnpm-dependencies pnpm-licenses source-assets macos-sandbox
do
  "$AI_IP_UV_ROOT/bin/python" "$AI_IP_RECORDER" \
    --repo-root "$AI_IP_NATIVE_WORKTREE" \
    --evidence-dir "$AI_IP_EVIDENCE_DIR" \
    --matrix "$AI_IP_COMMAND_MATRIX" \
    --name "$AI_IP_MATRIX_ID" || true
  test -z "$(git -C "$AI_IP_NATIVE_WORKTREE" status --porcelain=v1 --untracked-files=all)"
  test -z "$(git -C "$AI_IP_TOOLS_WORKTREE" status --porcelain=v1 --untracked-files=all)"
done
```

The `|| true` is allowed only in this outer evidence-collection loop because the recorder must persist a nonzero required result as `BLOCKED_BASELINE`; the verifier, not the shell loop, decides the final disposition. It is forbidden inside the recorder, matrix, or verification command.

- [ ] **Step 2: Verify baseline completeness and write the summary from verified output**

```bash
set +e
"$AI_IP_UV_ROOT/bin/python" "$AI_IP_TOOLS_WORKTREE/scripts/ai_ip/foundation/verify_evidence.py" \
  --repo-root "$PWD" \
  --matrix "$AI_IP_COMMAND_MATRIX" \
  --evidence-root "$AI_IP_EVIDENCE_DIR" \
  --platform macos-x86_64 \
  --mode baseline > "$AI_IP_EVIDENCE_DIR/baseline-summary.json"
AI_IP_BASELINE_VERIFY_EXIT=$?
set -e
test "$AI_IP_BASELINE_VERIFY_EXIT" -eq 0 -o "$AI_IP_BASELINE_VERIFY_EXIT" -eq 2
```

Expected: exit `0` and summary `status=PASS`, or exit `2` and exact nonempty `blockedIds`. Exit `1`, missing IDs, unexpected IDs, tampering, or verifier crash invalidates capture and must be fixed before continuing.

- [ ] **Step 3: Request the upstream-required complete workspace suite decision**

Ask the user explicitly whether to run the complete workspace `just test`. If approved, run the exact command from the detached tested tree with the isolated environment, stream stdout/stderr to an external task-specific directory, preserve its real exit code, and record `status` as `passed` or `failed` plus the computed 64-character lowercase SHA-256 strings of both logs in the ledger; the external logs are not committed and the frozen required matrix is unchanged. If declined, record `workspaceSuite={"status":"not-run/declined"}`. The required focused baseline result is independent and cannot be upgraded or downgraded by this optional suite.

- [ ] **Step 4: Scan evidence for forbidden content before staging**

Run a byte-safe scanner that rejects the current user home path, `Authorization:`, `Bearer `, common API-key assignments, `.codex/auth.json`, prompt/response body field names, and any private case/reviewer marker. The scanner may print file names and rule IDs only, never matching secret bytes. Add its exact implementation and tests to `verify_evidence.py` before using it if the verifier does not already own this scan.

Expected: zero forbidden matches. A match blocks staging until the producing command/evidence path is corrected and recaptured; never edit logs in place.

---

### Task 8: Commit verified baseline evidence and close the child boundary

**Files:**
- Generate/track: `docs/evidence/foundation/macos-x86_64/**`
- Modify: `docs/architecture/codex-fork-patch-ledger.md`

**Interfaces:**
- Consumes verified PASS macOS baseline.
- Produces the Phase 0A.2 handoff and authorizes only the next business-contract child plus an independent Windows-baseline child; it does not close G0–G2.

- [ ] **Step 1: Stop if the macOS required baseline is blocked**

Read `baseline-summary.json`. If status is not `PASS`, append an F-0003 `BLOCKED_BASELINE` disposition listing exact IDs and no removal/ignore mechanism, commit the honest evidence only if review confirms it contains useful reproducible failures, and stop. Do not authorize Work Package 3 until the required macOS baseline is recaptured as PASS.

- [ ] **Step 2: Append the F-0003 ledger entry for a passing baseline**

The ledger entry records: owner and plan path; upstream/tested SHA; exact tools SHA; classification `preserve/add`; paths; business reason; Python test command; matrix SHA; macOS evidence summary SHA; bundle SHA; optional workspace suite disposition; `providerMode=not-run`; `paidProviderCost=0`; Windows `pending-independent-child`; sync risk; and rollback. It must say that only macOS focused baseline is proven and that G0/G1/G2/`PASS_TO_PHASE_0B` remain open.

- [ ] **Step 3: Stage exact evidence paths, including ignored logs, and commit**

```bash
set -euo pipefail
git add -- \
  docs/evidence/foundation/macos-x86_64/*.json \
  docs/architecture/codex-fork-patch-ledger.md
git add -f -- \
  docs/evidence/foundation/macos-x86_64/*.stdout.log \
  docs/evidence/foundation/macos-x86_64/*.stderr.log
test -z "$(git diff --cached --name-only | sed -n '\#^docs/evidence/foundation/macos-x86_64/#d;\#^docs/architecture/codex-fork-patch-ledger.md$#d;p')"
git diff --cached --check
git commit -m "test: capture pinned Codex macOS baseline"
```

Expected: commit contains only verified macOS evidence and the ledger entry. No tested/tools tree is changed.

- [ ] **Step 4: Run final machine verification on the clean evidence commit**

```bash
set -euo pipefail
python3 scripts/ai_ip/foundation/verify_upstream_lock.py --repo "$PWD"
PYTHONDONTWRITEBYTECODE=1 uv run --python 3.11 --with pytest==8.3.5 \
  pytest -q -p no:cacheprovider scripts/ai_ip/foundation/test_*.py
uv run --python 3.11 scripts/ai_ip/foundation/verify_evidence.py \
  --repo-root "$PWD" \
  --matrix scripts/ai_ip/foundation/required_command_matrix.json \
  --evidence-root docs/evidence/foundation/macos-x86_64 \
  --platform macos-x86_64 \
  --mode baseline
test -z "$(git diff --name-only 4ef1d4b89bd419c976b04fefa0fd36844e898340..HEAD -- codex-rs)"
git diff --check 4ef1d4b89bd419c976b04fefa0fd36844e898340 HEAD
git fsck --full --strict --no-reflogs --no-dangling
test -z "$(git status --porcelain=v1 --untracked-files=all)"
```

Expected: both verifiers PASS, all Python tests pass, macOS baseline disposition is PASS, strict fsck succeeds, `codex-rs` is unchanged, and the worktree is clean.

- [ ] **Step 5: Deliver the checkpoint handoff**

Report in Chinese: business result and strict limitation; working/stubbed capabilities; tools/tested/final SHAs; matrix and evidence digests; macOS command totals and failures; Windows status; optional workspace suite decision; paid cost; local/external data locations; known risks; bundle path/digest; rollback; and explicit continue/correct decision. State exactly:

```text
providerMode=not-run
paidProviderCost=0
WindowsBaseline=pending-independent-child
G0=OPEN
G1=OPEN
G2=OPEN
PASS_TO_PHASE_0B=false
```

---

## Work Package 2 Coverage

| Work Package 2 contract | Owning task(s) | Child disposition |
|---|---|---|
| Recorder/verifier/wrapper RED tests | Tasks 1–4 | Implemented before each corresponding behavior; focused commands and accepted failure modes are explicit |
| Streaming recorder, fixed matrix, public verifier, frozen wrapper | Tasks 1–4 | Fully implemented and sealed into the tools SHA without runtime/business changes |
| Tools-first commit and named two-ref bundle | Task 5 | Fully implemented; bundle advertises only pinned Codex and foundation tools refs |
| Exact toolchain and unchanged native baseline | Tasks 6–7 | This child completes the 20-command native macOS x86_64 half only |
| Native Windows 11 x64 baseline and evidence return | Later independent child | Required on the exact Task 5 tools SHA; Work Package 2 remains open until it returns and verifies |
| Full workspace `just test` disposition | Task 7 | User decision recorded separately; never changes or substitutes for the frozen required matrix |
| Evidence commit and machine verification | Task 8 | macOS evidence only; G0/G1/G2 remain open |

The plan intentionally splits the two native hosts at the business-approved seam in Work Package 2 Contract 4. It does not reinterpret a partial macOS result as completion of the parent Work Package.

---

## Completion Boundary

This plan completes only when the evidence tools are committed, the exact tools SHA is bundled with the pinned Codex ref, the unchanged native macOS x86_64 20-command baseline is verifiably PASS, evidence contains no prohibited content, `codex-rs` remains byte-identical to pinned upstream, and all reviews/machine checks pass.

The plan does not prove Windows, G0, G1, G2, business quality, a provider route, or a product. The next executable work may be (a) the first pure `ContentPackage` business-contract child after the macOS baseline PASS, and/or (b) the independent Windows baseline child on this exact tools SHA. Windows may not block approved isolated business work, but it remains mandatory before the final Phase 0A checkpoint.
