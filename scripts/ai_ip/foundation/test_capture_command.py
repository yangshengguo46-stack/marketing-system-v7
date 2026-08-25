import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from unittest.mock import Mock

import pytest

import capture_command as capture


MATRIX_PATH = Path(__file__).with_name("required_command_matrix.json")
EXPECTED_MATRIX_SHA256 = (
    "04deab6a2769e491b2b313a1f3b44bd11b1ce7f58b5f6893b12e8cb8105c140a"
)
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

CAPTURE_SCRIPT = Path(__file__).with_name("capture_command.py")
REQUIRED_TOOL_NAMES = {
    "python",
    "uv",
    "git",
    "just",
    "dotslash",
    "rustc",
    "cargo",
    "cargo-nextest",
    "cargo-deny",
    "node",
    "pnpm",
    "bazelisk",
    "bazel",
}
COMMAND_MANIFEST_KEYS = {
    "schemaVersion",
    "commandId",
    "phase",
    "argv",
    "expectedExit",
    "exitCode",
    "status",
    "startedAt",
    "endedAt",
    "platform",
    "architecture",
    "testedGitSha",
    "toolsGitSha",
    "recorderSha256",
    "matrixSha256",
    "locks",
    "stdout",
    "stderr",
    "selection",
}
BOOTSTRAP_MANIFEST_KEYS = {
    "schemaVersion",
    "platform",
    "architecture",
    "osVersion",
    "osBuild",
    "testedGitSha",
    "toolsGitSha",
    "recorderSha256",
    "matrixSha256",
    "locks",
    "tools",
    "dependencyInstall",
    "createdAt",
    "administratorToken",
}
TOOL_RECEIPT_KEYS = {
    "versionArgv",
    "versionExitCode",
    "versionStdout",
    "versionStderr",
    "executableSha256",
    "executableBytes",
}
STREAM_RECEIPT_KEYS = {"path", "sha256", "bytes"}
GIT_ENV = {
    "GIT_AUTHOR_NAME": "Recorder Test",
    "GIT_AUTHOR_EMAIL": "recorder@example.invalid",
    "GIT_COMMITTER_NAME": "Recorder Test",
    "GIT_COMMITTER_EMAIL": "recorder@example.invalid",
}


def _git(repo: Path, *args: str) -> str:
    completed = subprocess.run(
        ["/usr/local/bin/git", "-C", str(repo), *args],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
        env={**os.environ, **GIT_ENV},
    )
    if completed.returncode != 0:
        raise AssertionError(
            f"git {' '.join(args)} failed ({completed.returncode}): "
            f"{completed.stderr.strip()}"
        )
    return completed.stdout.strip()


def _commit(repo: Path, message: str = "fixture") -> None:
    _git(repo, "add", "-A")
    _git(repo, "commit", "-q", "-m", message)


def _write_executable(path: Path, body: str) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(body, encoding="utf-8", newline="\n")
    path.chmod(0o755)
    return path


def initialize_clean_fixture_repo(path: Path) -> Path:
    path.mkdir(parents=True)
    _git(path, "init", "-q")
    _git(path, "config", "user.name", "Recorder Test")
    _git(path, "config", "user.email", "recorder@example.invalid")
    for relative in ("codex-rs/Cargo.lock", "pnpm-lock.yaml", "MODULE.bazel.lock"):
        lock = path / relative
        lock.parent.mkdir(parents=True, exist_ok=True)
        lock.write_text(f"fixture {relative}\n", encoding="utf-8", newline="\n")
    (path / "seed.txt").write_text("clean\n", encoding="utf-8", newline="\n")
    _commit(path, "initialize tested tree")
    _git(path, "switch", "--detach", "-q")
    return path.resolve()


def write_fixture_matrix(path: Path) -> Path:
    tools = path.parents[3]
    path.parent.mkdir(parents=True)
    shutil.copyfile(CAPTURE_SCRIPT, path.with_name("capture_command.py"))
    matrix = {
        "schemaVersion": 1,
        "commands": [
            {
                "id": "fixture-command",
                "platforms": ["macos-x86_64"],
                "phase": "baselineAndPost",
                "argv": ["./fixture-command"],
                "expectedExit": 0,
            },
            {
                "id": "filtered-fixture",
                "platforms": ["macos-x86_64"],
                "phase": "baselineAndPost",
                "argv": ["just", "test", "-E", "test(=fixture)"],
                "expectedExit": 0,
            },
        ],
    }
    path.write_text(json.dumps(matrix), encoding="utf-8", newline="\n")
    _git(tools, "init", "-q")
    _git(tools, "config", "user.name", "Recorder Test")
    _git(tools, "config", "user.email", "recorder@example.invalid")
    _commit(tools, "initialize tools tree")
    _git(tools, "switch", "--detach", "-q")
    return path.resolve()


def load_strict_json(path: Path) -> dict[str, object]:
    value = load_json_without_duplicate_keys(path)
    assert isinstance(value, dict)
    return value


@dataclass
class RecorderHarness:
    repo: Path
    evidence: Path
    matrix: Path
    script: Path

    @classmethod
    def create(cls, tmp_path: Path) -> "RecorderHarness":
        repo = initialize_clean_fixture_repo(tmp_path / "tested")
        evidence = (tmp_path / "evidence").resolve()
        evidence.mkdir()
        matrix = write_fixture_matrix(
            tmp_path / "tools/scripts/ai_ip/foundation/matrix.json"
        )
        return cls(
            repo=repo,
            evidence=evidence,
            matrix=matrix,
            script=matrix.with_name("capture_command.py"),
        )

    @property
    def capture_script(self) -> Path:
        return self.script

    def run(self, *extra: str) -> subprocess.CompletedProcess[bytes]:
        environment = os.environ.copy()
        environment["PYTHONDONTWRITEBYTECODE"] = "1"
        test_path = self.repo / ".git/recorder-test-path"
        if test_path.is_file():
            environment["PATH"] = test_path.read_text(encoding="utf-8").strip()
        hostile = self.repo / ".git/recorder-hostile-env.json"
        if hostile.is_file():
            seeded = json.loads(hostile.read_text(encoding="utf-8"))
            assert isinstance(seeded, dict)
            environment.update(seeded)
        return subprocess.run(
            [
                sys.executable,
                str(self.capture_script),
                "--repo-root",
                str(self.repo),
                "--evidence-dir",
                str(self.evidence),
                "--matrix",
                str(self.matrix),
                *extra,
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            env=environment,
        )


def _rewrite_matrix_platform(harness: RecorderHarness, platform_name: str) -> None:
    matrix = load_strict_json(harness.matrix)
    commands = matrix["commands"]
    assert isinstance(commands, list)
    for command in commands:
        assert isinstance(command, dict)
        command["platforms"] = [platform_name]
    harness.matrix.write_text(json.dumps(matrix), encoding="utf-8", newline="\n")
    _commit(harness.matrix.parents[3], "change fixture platform")


def apply_invalid_boundary(harness: RecorderHarness, invalid_input: str) -> None:
    if invalid_input == "relative_repo":
        harness.repo = Path("tested")
    elif invalid_input == "relative_evidence":
        harness.evidence = Path("evidence")
    elif invalid_input == "relative_matrix":
        harness.matrix = Path("matrix.json")
    elif invalid_input == "dirty_repo":
        (harness.repo / "dirty.txt").write_text("dirty\n", encoding="utf-8")
    elif invalid_input == "evidence_inside_repo":
        harness.evidence = harness.repo / "evidence"
        harness.evidence.mkdir()
    elif invalid_input == "repo_inside_evidence":
        harness.evidence = harness.repo.parent
    elif invalid_input == "symlink_parent":
        target = harness.evidence
        link = target.parent / "linked-evidence"
        link.symlink_to(target, target_is_directory=True)
        harness.evidence = link / "."
    elif invalid_input == "unknown_command":
        return
    elif invalid_input == "wrong_platform":
        _rewrite_matrix_platform(harness, "windows-11-x64")
    elif invalid_input == "preexisting_output":
        (harness.evidence / "fixture-command.stdout.log").write_bytes(b"existing")
    else:
        raise AssertionError(f"unknown invalid boundary: {invalid_input}")


def install_success_fixture(repo: Path) -> None:
    _write_executable(repo / "fixture-command", "#!/bin/sh\nprintf success\n")
    _commit(repo, "install success command")


def install_binary_stream_fixture(repo: Path, exit_code: int) -> tuple[bytes, bytes]:
    expected_stdout = b"\x00\xff" + (b"A" * (1024 * 1024 + 17))
    expected_stderr = b"\x80\xfe" + (b"B" * (1024 * 1024 + 29))
    script = f"""#!/usr/bin/env python3
import os
os.write(1, b"\\x00\\xff" + b"A" * {1024 * 1024 + 17})
os.write(2, b"\\x80\\xfe" + b"B" * {1024 * 1024 + 29})
raise SystemExit({exit_code})
"""
    _write_executable(repo / "fixture-command", script)
    _commit(repo, "install binary stream command")
    return expected_stdout, expected_stderr


def install_dirtying_fixture(repo: Path) -> None:
    _write_executable(
        repo / "fixture-command",
        "#!/bin/sh\nprintf changed > seed.txt\n",
    )
    _commit(repo, "install dirtying command")


def install_barrier_fixture(repo: Path) -> None:
    _write_executable(
        repo / "fixture-command",
        "#!/bin/sh\nsleep 0.2\nprintf winner\nprintf error >&2\n",
    )
    _commit(repo, "install barrier command")


def install_filtered_fixture(repo: Path, test_count: int) -> Path:
    tool_bin = repo / "tool-bin"
    marker = repo / ".git/executed"
    _write_executable(
        tool_bin / "cargo",
        "#!/bin/sh\nprintf '%s\\n' "
        + repr(
            json.dumps(
                {"rust-build-meta": {}, "test-count": test_count, "rust-suites": {}}
            )
        )
        + "\n",
    )
    _write_executable(
        tool_bin / "just",
        f"#!/bin/sh\nprintf 'executed\\n' > {str(marker)!r}\n",
    )
    _commit(repo, "install filtered command")
    (repo / ".git/recorder-test-path").write_text(
        f"{tool_bin}:/usr/local/bin:/usr/bin:/bin\n", encoding="utf-8"
    )
    return marker


def run_two_captures_concurrently(
    harness: RecorderHarness, command_id: str
) -> tuple[subprocess.CompletedProcess[bytes], subprocess.CompletedProcess[bytes]]:
    argv = [
        sys.executable,
        str(harness.capture_script),
        "--repo-root",
        str(harness.repo),
        "--evidence-dir",
        str(harness.evidence),
        "--matrix",
        str(harness.matrix),
        "--name",
        command_id,
    ]
    environment = {**os.environ, "PYTHONDONTWRITEBYTECODE": "1"}
    first = subprocess.Popen(
        argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=environment
    )
    second = subprocess.Popen(
        argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=environment
    )
    first_stdout, first_stderr = first.communicate(timeout=15)
    second_stdout, second_stderr = second.communicate(timeout=15)
    return (
        subprocess.CompletedProcess(argv, first.returncode, first_stdout, first_stderr),
        subprocess.CompletedProcess(
            argv, second.returncode, second_stdout, second_stderr
        ),
    )


def verify_complete_fixture_evidence(evidence: Path, command_id: str) -> None:
    expected = {
        f"{command_id}.stdout.log",
        f"{command_id}.stderr.log",
        f"{command_id}.manifest.json",
    }
    assert {path.name for path in evidence.iterdir()} == expected
    manifest = load_strict_json(evidence / f"{command_id}.manifest.json")
    assert set(manifest) == COMMAND_MANIFEST_KEYS
    for stream_name in ("stdout", "stderr"):
        receipt = manifest[stream_name]
        assert isinstance(receipt, dict)
        assert set(receipt) == STREAM_RECEIPT_KEYS
        payload = (evidence / str(receipt["path"])).read_bytes()
        assert receipt["bytes"] == len(payload)
        assert receipt["sha256"] == hashlib.sha256(payload).hexdigest()


TOOL_OUTPUTS = {
    "python": "Python 3.11.15",
    "uv": "uv 0.11.3 (fixture metadata)",
    "git": "git version 2.54.0",
    "just": "just 1.58.0",
    "dotslash": "DotSlash 0.5.8",
    "rustc": "rustc 1.95.0 (fixture)",
    "cargo": "cargo 1.95.0 (fixture)",
    "cargo-nextest": "cargo-nextest 0.9.103 (fixture)\\nrelease: fixture",
    "cargo-deny": "cargo-deny 0.20.2",
    "node": "v26.4.0",
    "pnpm": "10.34.5",
    "bazelisk": "v1.28.1",
    "bazel": "bazel 9.0.0",
}


def install_exact_fake_tool_path(harness: RecorderHarness, tool_bin: Path) -> None:
    tool_bin.mkdir()
    for tool, output in TOOL_OUTPUTS.items():
        if tool == "git":
            body = f"""#!/bin/sh
if test "$1" = --version; then printf '%s\\n' {output!r}; else exec /usr/local/bin/git "$@"; fi
"""
        elif tool == "cargo":
            body = f"""#!/bin/sh
if test "$1" = nextest; then printf '%b\\n' {TOOL_OUTPUTS["cargo-nextest"]!r}
elif test "$1" = deny; then printf '%s\\n' {TOOL_OUTPUTS["cargo-deny"]!r}
else printf '%s\\n' {output!r}; fi
"""
        elif tool == "node":
            body = f"""#!/bin/sh
if test "$1" = -p; then printf '%s\\n' {TOOL_OUTPUTS["bazelisk"]!r}; else printf '%s\\n' {output!r}; fi
"""
        elif tool == "pnpm":
            body = f"""#!/bin/sh
if test "$1" = install; then printf 'installed\\n'; else printf '%s\\n' {output!r}; fi
"""
        else:
            body = f"#!/bin/sh\nprintf '%b\\n' {output!r}\n"
        _write_executable(tool_bin / tool, body)
    (tool_bin / "package.json").write_text(
        '{"name":"@bazel/bazelisk","version":"v1.28.1"}\n', encoding="utf-8"
    )
    (harness.repo / ".git/recorder-test-path").write_text(
        f"{tool_bin}\n", encoding="utf-8"
    )


def _final_evidence_snapshot(evidence: Path, command_id: str) -> dict[str, bytes]:
    return {
        path.name: path.read_bytes()
        for path in evidence.glob(f"{command_id}.*")
        if ".tmp-" not in path.name
    }


def load_json_without_duplicate_keys(path: Path) -> object:
    def reject_duplicates(pairs: list[tuple[str, object]]) -> dict[str, object]:
        result: dict[str, object] = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate JSON key: {key}")
            result[key] = value
        return result

    return json.loads(
        path.read_text(encoding="utf-8"), object_pairs_hook=reject_duplicates
    )


def test_required_matrix_is_exact_and_frozen() -> None:
    parsed = load_json_without_duplicate_keys(MATRIX_PATH)
    assert (
        hashlib.sha256(capture.canonical_json_bytes(parsed)).hexdigest()
        == EXPECTED_MATRIX_SHA256
    )
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


@pytest.mark.parametrize(
    ("payload", "message"),
    [
        ('{"schemaVersion":1,"schemaVersion":1,"commands":[]}', "duplicate JSON key"),
        (
            '{"schemaVersion":1,"commands":[{"id":"one","platforms":["macos-x86_64"],"phase":"baselineAndPost","argv":["just"],"expectedExit":0},{"id":"one","platforms":["macos-x86_64"],"phase":"baselineAndPost","argv":["just"],"expectedExit":0}]}',
            "duplicate command id",
        ),
        (
            '{"schemaVersion":1,"commands":[{"id":"one","platforms":["macos-x86_64"],"phase":"baselineAndPost","argv":["just"],"expectedExit":0,"extra":true}]}',
            "unknown command keys",
        ),
        (
            '{"schemaVersion":1,"commands":[{"id":"one","platforms":["linux"],"phase":"baselineAndPost","argv":["just"],"expectedExit":0}]}',
            "unknown platform",
        ),
        (
            '{"schemaVersion":1,"commands":[{"id":"one","platforms":["macos-x86_64"],"phase":"later","argv":["just"],"expectedExit":0}]}',
            "unknown phase",
        ),
        (
            '{"schemaVersion":1,"commands":[{"id":"one","platforms":["macos-x86_64"],"phase":[],"argv":["just"],"expectedExit":0}]}',
            "unknown phase",
        ),
        (
            '{"schemaVersion":1,"commands":[{"id":"one","platforms":["macos-x86_64"],"phase":"baselineAndPost","argv":"just test","expectedExit":0}]}',
            "argv must be a non-empty array",
        ),
        (
            '{"schemaVersion":1,"commands":[{"id":"one","platforms":["macos-x86_64"],"phase":"baselineAndPost","argv":["just", ""],"expectedExit":0}]}',
            "argv arguments must be non-empty strings",
        ),
        (
            '{"schemaVersion":1,"commands":[{"id":"one","platforms":["macos-x86_64"],"phase":"baselineAndPost","argv":["just"],"expectedExit":true}]}',
            "expectedExit must be an integer",
        ),
        (
            '{"schemaVersion":1,"commands":[{"id":"one","platforms":["macos-x86_64"],"phase":"baselineAndPost","argv":["just"],"expectedExit":1}]}',
            "expectedExit must be zero",
        ),
        (
            '{"schemaVersion":1,"commands":[{"id":"one","platforms":["macos-x86_64"],"phase":"baselineAndPost","argv":["just"],"expectedExit":0,"missing":null}]}',
            "unknown command keys",
        ),
        (
            '{"schemaVersion":1,"commands":[{"id":"one","platforms":["macos-x86_64"],"phase":"baselineAndPost","argv":["just"]}]}',
            "invalid command keys",
        ),
        (
            '{"schemaVersion":1,"commands":[],"unknown":true}',
            "unknown matrix keys",
        ),
    ],
)
def test_load_matrix_rejects_invalid_entries(
    tmp_path: Path, payload: str, message: str
) -> None:
    path = tmp_path / "matrix.json"
    path.write_text(payload, encoding="utf-8")
    with pytest.raises(capture.EvidenceError, match=message):
        capture.load_matrix(path)


def test_selection_argv_only_converts_nextest_filters() -> None:
    command = capture.CommandSpec(
        "selected",
        ("macos-x86_64",),
        "baselineAndPost",
        ("just", "test", "-p", "crate", "-E", "test(=x)"),
        0,
    )
    assert capture.selection_argv(command) == (
        "cargo",
        "nextest",
        "list",
        "--message-format",
        "json",
        "-p",
        "crate",
        "-E",
        "test(=x)",
    )
    unselected = capture.CommandSpec(
        "other", ("macos-x86_64",), "baselineAndPost", ("just", "fmt-check"), 0
    )
    assert capture.selection_argv(unselected) is None


def test_required_for_rejects_unknown_mode() -> None:
    matrix = capture.Matrix(1, ())
    with pytest.raises(capture.EvidenceError, match="unknown evidence mode: invalid"):
        matrix.required_for("macos-x86_64", "invalid")


def test_host_id_accepts_native_darwin_with_exact_zero(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(capture.platform, "system", lambda: "Darwin")
    monkeypatch.setattr(capture.platform, "machine", lambda: "x86_64")
    run = Mock(return_value=subprocess.CompletedProcess([], 0, "0\n", ""))
    monkeypatch.setattr(capture.subprocess, "run", run)
    assert capture.host_id() == "macos-x86_64"
    run.assert_called_once_with(
        ["sysctl", "-n", "sysctl.proc_translated"],
        capture_output=True,
        text=True,
        check=False,
    )


def test_host_id_accepts_genuine_absent_darwin_oid(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(capture.platform, "system", lambda: "Darwin")
    monkeypatch.setattr(capture.platform, "machine", lambda: "x86_64")
    monkeypatch.setattr(
        capture.subprocess,
        "run",
        Mock(
            return_value=subprocess.CompletedProcess(
                [], 1, "", "sysctl: unknown oid 'sysctl.proc_translated'\n"
            )
        ),
    )
    assert capture.host_id() == "macos-x86_64"


@pytest.mark.parametrize(
    "result",
    [
        subprocess.CompletedProcess([], 0, "1\n", ""),
        subprocess.CompletedProcess([], 0, "2\n", ""),
        subprocess.CompletedProcess([], 1, "", "permission denied\n"),
    ],
)
def test_host_id_rejects_non_native_darwin_results(
    monkeypatch: pytest.MonkeyPatch, result: subprocess.CompletedProcess[str]
) -> None:
    monkeypatch.setattr(capture.platform, "system", lambda: "Darwin")
    monkeypatch.setattr(capture.platform, "machine", lambda: "x86_64")
    monkeypatch.setattr(capture.subprocess, "run", Mock(return_value=result))
    with pytest.raises(capture.EvidenceError):
        capture.host_id()


def test_host_id_rejects_missing_sysctl(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(capture.platform, "system", lambda: "Darwin")
    monkeypatch.setattr(capture.platform, "machine", lambda: "x86_64")
    monkeypatch.setattr(capture.subprocess, "run", Mock(side_effect=FileNotFoundError))
    with pytest.raises(capture.EvidenceError):
        capture.host_id()


def test_host_id_accepts_windows_11_x64(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(capture.platform, "system", lambda: "Windows")
    monkeypatch.setattr(capture.platform, "machine", lambda: "AMD64")
    monkeypatch.setattr(capture.platform, "release", lambda: "10")
    monkeypatch.setattr(capture.platform, "version", lambda: "10.0.22631")
    assert capture.host_id() == "windows-11-x64"


def test_host_id_rejects_unsupported_host(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(capture.platform, "system", lambda: "Linux")
    monkeypatch.setattr(capture.platform, "machine", lambda: "x86_64")
    with pytest.raises(
        capture.EvidenceError, match="unsupported evidence host: Linux/x86_64"
    ):
        capture.host_id()


@pytest.mark.parametrize(
    "invalid_input",
    [
        "relative_repo",
        "relative_evidence",
        "relative_matrix",
        "dirty_repo",
        "evidence_inside_repo",
        "repo_inside_evidence",
        "symlink_parent",
        "unknown_command",
        "wrong_platform",
        "preexisting_output",
    ],
)
def test_capture_rejects_invalid_boundaries(invalid_input: str, tmp_path: Path) -> None:
    harness = RecorderHarness.create(tmp_path)
    apply_invalid_boundary(harness, invalid_input)
    before = _final_evidence_snapshot(harness.evidence, "fixture-command")
    name = (
        "missing-command" if invalid_input == "unknown_command" else "fixture-command"
    )
    completed = harness.run("--name", name)
    assert completed.returncode == 1
    assert _final_evidence_snapshot(harness.evidence, "fixture-command") == before
    assert not list(harness.evidence.glob("*.tmp-*"))


def test_capture_streams_binary_stdout_and_stderr_and_preserves_exit_code(
    tmp_path: Path,
) -> None:
    harness = RecorderHarness.create(tmp_path)
    expected_stdout, expected_stderr = install_binary_stream_fixture(
        harness.repo, exit_code=7
    )
    completed = harness.run("--name", "fixture-command")
    manifest = load_strict_json(harness.evidence / "fixture-command.manifest.json")
    assert completed.returncode == 7
    assert set(manifest) == COMMAND_MANIFEST_KEYS
    assert manifest["exitCode"] == 7
    assert manifest["status"] == "BLOCKED_BASELINE"
    assert manifest["stdout"]["bytes"] == len(expected_stdout)
    assert manifest["stdout"]["sha256"] == hashlib.sha256(expected_stdout).hexdigest()
    assert manifest["stderr"]["bytes"] == len(expected_stderr)
    assert manifest["stderr"]["sha256"] == hashlib.sha256(expected_stderr).hexdigest()
    assert (
        harness.evidence / "fixture-command.stdout.log"
    ).read_bytes() == expected_stdout
    assert (
        harness.evidence / "fixture-command.stderr.log"
    ).read_bytes() == expected_stderr


def test_stream_reader_never_requests_more_than_one_mib(tmp_path: Path) -> None:
    class RecordingReader(io.BytesIO):
        def __init__(self, payload: bytes) -> None:
            super().__init__(payload)
            self.requests: list[int] = []

        def read(self, size: int = -1) -> bytes:
            self.requests.append(size)
            return super().read(size)

    payload = b"x" * (1024 * 1024 + 3)
    reader = RecordingReader(payload)
    temp = tmp_path / "stream.tmp"
    digest, count = capture._stream_reader(reader, temp)
    assert reader.requests
    assert all(0 < request <= 1024 * 1024 for request in reader.requests)
    assert temp.read_bytes() == payload
    assert (digest, count) == (hashlib.sha256(payload).hexdigest(), len(payload))


@pytest.mark.parametrize("test_count", [0, 2])
def test_filtered_command_requires_exactly_one_listed_test(
    test_count: int, tmp_path: Path
) -> None:
    harness = RecorderHarness.create(tmp_path)
    marker = install_filtered_fixture(harness.repo, test_count=test_count)
    completed = harness.run("--name", "filtered-fixture")
    assert completed.returncode == 1
    assert not marker.exists()
    manifest = load_strict_json(harness.evidence / "filtered-fixture.manifest.json")
    assert manifest["status"] == "BLOCKED_SELECTION"
    assert manifest["selection"]["testCount"] == test_count


@pytest.mark.parametrize(
    "selection_output",
    [
        b"not-json\n",
        b'{"test-count":true,"rust-build-meta":{},"rust-suites":{}}\n',
        b'{"test-count":1,"rust-build-meta":{}}\n',
        b'{"test-count":1,"rust-build-meta":{},"rust-suites":{},"extra":0}\n',
        b'{"test-count":1,"test-count":1,"rust-build-meta":{},"rust-suites":{}}\n',
    ],
)
def test_filtered_command_rejects_malformed_selection_json(
    selection_output: bytes, tmp_path: Path
) -> None:
    harness = RecorderHarness.create(tmp_path)
    marker = install_filtered_fixture(harness.repo, test_count=1)
    cargo = harness.repo / "tool-bin/cargo"
    encoded = selection_output.hex()
    _write_executable(
        cargo,
        f"#!/usr/bin/env python3\nimport os\nos.write(1, bytes.fromhex({encoded!r}))\n",
    )
    _commit(harness.repo, "change selection output")
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
    assert set(manifest["selection"]) == {
        "argv",
        "exitCode",
        "testCount",
        "stdout",
        "stderr",
    }
    assert manifest["selection"]["argv"] == [
        "cargo",
        "nextest",
        "list",
        "--message-format",
        "json",
        "-E",
        "test(=fixture)",
    ]


def test_capture_rejects_dirty_tree_after_command(tmp_path: Path) -> None:
    harness = RecorderHarness.create(tmp_path)
    install_dirtying_fixture(harness.repo)
    completed = harness.run("--name", "fixture-command")
    assert completed.returncode == 1
    assert b"tested tree became dirty" in completed.stderr
    assert not list(harness.evidence.glob("fixture-command.*"))
    assert not list(harness.evidence.glob("*.tmp-*"))


def test_capture_never_overwrites_or_leaves_final_partial_files(
    tmp_path: Path,
) -> None:
    harness = RecorderHarness.create(tmp_path)
    install_success_fixture(harness.repo)
    assert harness.run("--name", "fixture-command").returncode == 0
    first = _final_evidence_snapshot(harness.evidence, "fixture-command")
    assert harness.run("--name", "fixture-command").returncode == 1
    assert _final_evidence_snapshot(harness.evidence, "fixture-command") == first
    assert not list(harness.evidence.glob("*.tmp-*"))


def test_concurrent_same_id_has_one_complete_winner_and_no_overwrite(
    tmp_path: Path,
) -> None:
    harness = RecorderHarness.create(tmp_path)
    install_barrier_fixture(harness.repo)
    first, second = run_two_captures_concurrently(harness, "fixture-command")
    assert sorted([first.returncode, second.returncode]) == [0, 1]
    verify_complete_fixture_evidence(harness.evidence, "fixture-command")
    assert not list(harness.evidence.glob("*.tmp-*"))


def test_posix_publish_adapter_is_create_new_and_fsyncs_directory(
    tmp_path: Path,
) -> None:
    temp = tmp_path / "value.tmp"
    final = tmp_path / "value.log"
    temp.write_bytes(b"first")
    events: list[tuple[str, object]] = []

    def link(source: Path, target: Path) -> None:
        events.append(("link", (source, target)))
        os.link(source, target)

    def open_directory(path: Path, flags: int) -> int:
        events.append(("open", (path, flags)))
        return os.open(path, flags)

    capture._publish_create_new(
        temp,
        final,
        platform_name="posix",
        link=link,
        open_directory=open_directory,
        fsync=lambda descriptor: events.append(("fsync", descriptor)),
        close=os.close,
        unlink=os.unlink,
    )
    assert final.read_bytes() == b"first"
    assert not temp.exists()
    assert [event[0] for event in events] == ["link", "open", "fsync"]
    replacement = tmp_path / "replacement.tmp"
    replacement.write_bytes(b"second")
    with pytest.raises(capture.EvidenceError, match="already exists"):
        capture._publish_create_new(replacement, final, platform_name="posix")
    assert final.read_bytes() == b"first"


@pytest.mark.parametrize("error_code", [80, 183])
def test_windows_publish_adapter_maps_exists_without_replace(
    error_code: int, tmp_path: Path
) -> None:
    temp = tmp_path / "value.tmp"
    final = tmp_path / "value.log"
    temp.write_bytes(b"new")
    calls: list[tuple[str, str, int]] = []

    def move_file(source: str, target: str, flags: int) -> int:
        calls.append((source, target, flags))
        return 0

    with pytest.raises(capture.EvidenceError, match="already exists"):
        capture._publish_create_new(
            temp,
            final,
            platform_name="nt",
            move_file=move_file,
            get_last_error=lambda: error_code,
        )
    assert calls == [(str(temp), str(final), 8)]
    assert temp.read_bytes() == b"new"


def test_windows_publish_adapter_reports_other_safe_error(tmp_path: Path) -> None:
    temp = tmp_path / "value.tmp"
    final = tmp_path / "value.log"
    temp.write_bytes(b"new")
    with pytest.raises(capture.EvidenceError, match="error 5"):
        capture._publish_create_new(
            temp,
            final,
            platform_name="nt",
            move_file=lambda _source, _target, _flags: 0,
            get_last_error=lambda: 5,
        )


def test_sanitized_child_environment_has_exact_allowlist_and_hardened_git() -> None:
    ambient = {
        "PATH": "/safe/bin",
        "HOME": "/safe/home",
        "TMPDIR": "/safe/tmp",
        "CARGO_HOME": "/safe/cargo",
        "npm_config_store_dir": "/safe/pnpm",
        "GIT_DIR": "/hostile/repo",
        "git_namespace": "hostile",
        "OPENAI_API_KEY": "secret",
        "ACCESS_TOKEN": "secret",
        "DYLD_INSERT_LIBRARIES": "/hostile/library",
        "LD_PRELOAD": "/hostile/library",
        "PYTHONPATH": "/hostile/python",
        "PYTHONHOME": "/hostile/python",
        "NPM_TOKEN": "secret",
        "npm_config_auth": "secret",
        "CARGO_REGISTRIES_CRATES_IO_TOKEN": "secret",
    }
    child = capture._sanitized_child_environment(ambient, platform_name="posix")
    assert child == {
        "PATH": "/safe/bin",
        "HOME": "/safe/home",
        "TMPDIR": "/safe/tmp",
        "CARGO_HOME": "/safe/cargo",
        "npm_config_store_dir": "/safe/pnpm",
        "GIT_CONFIG_COUNT": "1",
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_CONFIG_KEY_0": "core.fsmonitor",
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_CONFIG_VALUE_0": "false",
        "GIT_NO_LAZY_FETCH": "1",
        "GIT_NO_REPLACE_OBJECTS": "1",
        "GIT_OPTIONAL_LOCKS": "0",
        "GIT_TERMINAL_PROMPT": "0",
    }


@pytest.mark.parametrize(
    ("ambient", "platform_name"),
    [
        ({"HOME": "/home"}, "posix"),
        ({"PATH": "/bin"}, "posix"),
        ({"PATH": "/bin", "HOME": "bad\x00value"}, "posix"),
        ({"PATH": "C:\\bin", "PATHEXT": ".EXE"}, "nt"),
    ],
)
def test_sanitized_child_environment_rejects_missing_or_nul_os_values(
    ambient: dict[str, str], platform_name: str
) -> None:
    with pytest.raises(capture.EvidenceError):
        capture._sanitized_child_environment(ambient, platform_name=platform_name)


def test_hostile_ambient_values_never_reach_command_child(tmp_path: Path) -> None:
    harness = RecorderHarness.create(tmp_path)
    _write_executable(
        harness.repo / "fixture-command",
        """#!/usr/bin/env python3
import json
import os
print(json.dumps(dict(sorted(os.environ.items()))))
""",
    )
    _commit(harness.repo, "install environment command")
    hostile = {
        "GIT_DIR": "/hostile",
        "GITHUB_TOKEN": "secret",
        "OPENAI_API_KEY": "secret",
        "PYTHONPATH": "/hostile",
        "npm_config_auth": "secret",
    }
    (harness.repo / ".git/recorder-hostile-env.json").write_text(
        json.dumps(hostile), encoding="utf-8"
    )
    completed = harness.run("--name", "fixture-command")
    assert completed.returncode == 0
    child = json.loads(
        (harness.evidence / "fixture-command.stdout.log").read_text(encoding="utf-8")
    )
    assert all(key not in child for key in hostile)
    assert child["GIT_NO_LAZY_FETCH"] == "1"
    assert child["GIT_NO_REPLACE_OBJECTS"] == "1"


@pytest.mark.parametrize("git_state", ["shallow", "replace", "graft", "promisor"])
def test_capture_rejects_unsafe_git_object_state(
    git_state: str, tmp_path: Path
) -> None:
    harness = RecorderHarness.create(tmp_path)
    install_success_fixture(harness.repo)
    git_dir = Path(_git(harness.repo, "rev-parse", "--absolute-git-dir"))
    if git_state == "shallow":
        (git_dir / "shallow").write_text(
            _git(harness.repo, "rev-parse", "HEAD") + "\n", encoding="ascii"
        )
    elif git_state == "replace":
        head = _git(harness.repo, "rev-parse", "HEAD")
        _git(harness.repo, "replace", head, head)
    elif git_state == "graft":
        info = git_dir / "info"
        info.mkdir(exist_ok=True)
        (info / "grafts").write_text("", encoding="ascii")
    elif git_state == "promisor":
        _git(harness.repo, "config", "remote.origin.promisor", "true")
    completed = harness.run("--name", "fixture-command")
    assert completed.returncode == 1
    assert not list(harness.evidence.glob("fixture-command.*"))


@pytest.mark.parametrize("attached_tree", ["tested", "tools"])
def test_capture_requires_detached_tested_and_tools_trees(
    attached_tree: str, tmp_path: Path
) -> None:
    harness = RecorderHarness.create(tmp_path)
    install_success_fixture(harness.repo)
    tree = harness.repo if attached_tree == "tested" else harness.matrix.parents[3]
    _git(tree, "switch", "-q", "-c", "attached-fixture")
    completed = harness.run("--name", "fixture-command")
    assert completed.returncode == 1
    assert not list(harness.evidence.glob("fixture-command.*"))


def test_command_manifest_has_exact_keys_and_safe_relative_receipts(
    tmp_path: Path,
) -> None:
    harness = RecorderHarness.create(tmp_path)
    install_success_fixture(harness.repo)
    completed = harness.run("--name", "fixture-command")
    assert completed.returncode == 0
    manifest = load_strict_json(harness.evidence / "fixture-command.manifest.json")
    assert set(manifest) == COMMAND_MANIFEST_KEYS
    assert set(manifest["locks"]) == {
        "cargoLockSha256",
        "pnpmLockSha256",
        "bazelLockSha256",
    }
    assert manifest["selection"] is None
    assert manifest["argv"] == ["./fixture-command"]
    assert manifest["status"] == "PASS"
    for stream_name in ("stdout", "stderr"):
        assert set(manifest[stream_name]) == STREAM_RECEIPT_KEYS
        assert Path(manifest[stream_name]["path"]).name == manifest[stream_name]["path"]


def test_bootstrap_manifest_requires_exact_tool_set_and_records_hashes(
    tmp_path: Path,
) -> None:
    harness = RecorderHarness.create(tmp_path)
    install_exact_fake_tool_path(harness, tmp_path / "tool-bin")
    completed = harness.run("--bootstrap")
    manifest = load_strict_json(harness.evidence / "host-bootstrap.manifest.json")
    assert completed.returncode == 0, completed.stderr.decode(errors="replace")
    assert set(manifest) == BOOTSTRAP_MANIFEST_KEYS
    assert set(manifest["tools"]) == REQUIRED_TOOL_NAMES
    assert all(
        set(receipt) == TOOL_RECEIPT_KEYS for receipt in manifest["tools"].values()
    )
    assert all("executableSha256" in receipt for receipt in manifest["tools"].values())
    assert all(
        set(receipt[stream]) == STREAM_RECEIPT_KEYS
        for receipt in manifest["tools"].values()
        for stream in ("versionStdout", "versionStderr")
    )
    assert set(manifest["dependencyInstall"]) == {
        "argv",
        "exitCode",
        "startedAt",
        "endedAt",
        "stdout",
        "stderr",
    }
    assert manifest["dependencyInstall"]["argv"] == [
        "pnpm",
        "install",
        "--frozen-lockfile",
    ]
    assert manifest["dependencyInstall"]["exitCode"] == 0
    assert manifest["administratorToken"] is None


@pytest.mark.parametrize("missing_tool", sorted(REQUIRED_TOOL_NAMES))
def test_bootstrap_rejects_each_missing_required_tool(
    missing_tool: str, tmp_path: Path
) -> None:
    harness = RecorderHarness.create(tmp_path)
    tool_bin = tmp_path / "tool-bin"
    install_exact_fake_tool_path(harness, tool_bin)
    (tool_bin / missing_tool).unlink()
    completed = harness.run("--bootstrap")
    assert completed.returncode == 1
    assert not (harness.evidence / "host-bootstrap.manifest.json").exists()
    assert not list(harness.evidence.glob("*.tmp-*"))


def test_bootstrap_rejects_duplicate_canonical_tool_resolution(
    tmp_path: Path,
) -> None:
    harness = RecorderHarness.create(tmp_path)
    tool_bin = tmp_path / "tool-bin"
    install_exact_fake_tool_path(harness, tool_bin)
    (tool_bin / "cargo-deny").unlink()
    (tool_bin / "cargo-deny").symlink_to(tool_bin / "cargo-nextest")
    completed = harness.run("--bootstrap")
    assert completed.returncode == 1
    assert not (harness.evidence / "host-bootstrap.manifest.json").exists()


@pytest.mark.parametrize("failure", ["version", "dependency", "dirty"])
def test_bootstrap_rejects_probe_install_and_tree_failures(
    failure: str, tmp_path: Path
) -> None:
    harness = RecorderHarness.create(tmp_path)
    tool_bin = tmp_path / "tool-bin"
    install_exact_fake_tool_path(harness, tool_bin)
    if failure == "version":
        _write_executable(tool_bin / "uv", "#!/bin/sh\nprintf 'uv 9.9.9\\n'\n")
    elif failure == "dependency":
        _write_executable(
            tool_bin / "pnpm",
            "#!/bin/sh\nif test \"$1\" = install; then exit 9; fi\nprintf '10.34.5\\n'\n",
        )
    else:
        _write_executable(
            tool_bin / "pnpm",
            "#!/bin/sh\nif test \"$1\" = install; then printf dirty > seed.txt; else printf '10.34.5\\n'; fi\n",
        )
    completed = harness.run("--bootstrap")
    assert completed.returncode == 1
    assert not (harness.evidence / "host-bootstrap.manifest.json").exists()
    assert not list(harness.evidence.glob("*.tmp-*"))


def test_bootstrap_second_capture_never_overwrites(tmp_path: Path) -> None:
    harness = RecorderHarness.create(tmp_path)
    install_exact_fake_tool_path(harness, tmp_path / "tool-bin")
    assert harness.run("--bootstrap").returncode == 0
    first = {path.name: path.read_bytes() for path in harness.evidence.iterdir()}
    assert harness.run("--bootstrap").returncode == 1
    assert {
        path.name: path.read_bytes() for path in harness.evidence.iterdir()
    } == first
    assert not list(harness.evidence.glob("*.tmp-*"))
