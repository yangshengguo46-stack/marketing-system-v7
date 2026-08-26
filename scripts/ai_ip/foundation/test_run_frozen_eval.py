import hashlib
import json
import os
from dataclasses import dataclass, field
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace

import pytest


sys.path.insert(0, str(Path(__file__).resolve().parent))
import run_frozen_eval as frozen


WRAPPER = Path(__file__).with_name("run_frozen_eval.py")
TEST_GIT = Path("/usr/bin/git")
GIT_ENV = {
    "GIT_AUTHOR_NAME": "Frozen evaluator test",
    "GIT_AUTHOR_EMAIL": "frozen-evaluator@example.invalid",
    "GIT_COMMITTER_NAME": "Frozen evaluator test",
    "GIT_COMMITTER_EMAIL": "frozen-evaluator@example.invalid",
    "GIT_CONFIG_NOSYSTEM": "1",
    "GIT_CONFIG_GLOBAL": os.devnull,
    "GIT_TERMINAL_PROMPT": "0",
}
HOST_PLATFORM = "macos-x86_64"


def _git(repo: Path, *args: str) -> str:
    completed = subprocess.run(
        [str(TEST_GIT), "-C", str(repo), *args],
        check=False,
        capture_output=True,
        text=True,
        env=GIT_ENV,
    )
    if completed.returncode != 0:
        raise AssertionError(
            f"git {' '.join(args)} failed ({completed.returncode}): "
            f"{completed.stderr.strip()}"
        )
    return completed.stdout.strip()


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


@dataclass
class FrozenEvalFixture:
    root: Path
    private_root: Path
    worktree: Path
    binary: Path
    context: Path
    candidate_sha: str
    execution_mode: str
    context_argument: str | None = None
    child_argv: tuple[str, ...] = ("replay-pair", "--fixture-safe-flag", "value")
    alternate_child_argv: tuple[tuple[str, ...], ...] = field(default_factory=tuple)

    @classmethod
    def create(cls, tmp_path: Path, *, execution_mode: str) -> "FrozenEvalFixture":
        root = tmp_path.resolve()
        private_root = root / "private-root"
        private_root.mkdir()
        worktree = root / "evaluator-worktree"
        worktree.mkdir()
        _git(worktree, "init", "-q")
        _git(worktree, "config", "user.name", "Frozen evaluator test")
        _git(worktree, "config", "user.email", "frozen-evaluator@example.invalid")
        binary = worktree / "frozen-evaluator"
        binary.write_text(
            "#!" + sys.executable + "\n"
            "import json\n"
            "import os\n"
            "from pathlib import Path\n"
            "import sys\n"
            "Path(sys.argv[0] + '.receipt').write_text(\n"
            "    json.dumps({'argv': sys.argv, 'environment': dict(os.environ)}, sort_keys=True),\n"
            "    encoding='utf-8',\n"
            ")\n",
            encoding="utf-8",
            newline="\n",
        )
        binary.chmod(0o755)
        (worktree / "seed.txt").write_text("clean\n", encoding="utf-8", newline="\n")
        _git(worktree, "add", "-A")
        _git(worktree, "commit", "-q", "-m", "freeze evaluator")
        candidate_sha = _git(worktree, "rev-parse", "HEAD")
        _git(worktree, "switch", "--detach", "-q")

        context = private_root / "coordinator" / "frozen-run-context.json"
        context.parent.mkdir()
        value: dict[str, object] = {
            "schemaVersion": 1,
            "executionMode": execution_mode,
            "candidateSha": candidate_sha,
            "privateRoot": str(private_root),
            "platform": HOST_PLATFORM,
            "futureOwnedRootField": {"permitted": True},
            "frozenEvaluator": {
                "gitSha": candidate_sha,
                "worktree": str(worktree),
                "macosX8664Binary": str(binary),
                "macosX8664BinarySha256": _sha256(binary),
                "windows11X64Binary": r"C:\\frozen-evaluator\\frozen-evaluator.exe",
                "windows11X64BinarySha256": "a" * 64,
            },
        }
        if execution_mode == "replay":
            value["fixtureSetSha256"] = "b" * 64
        elif execution_mode == "live":
            value.update(
                {
                    "providerRole": "targetVolcengine",
                    "providerBudgetEvidenceSha256": "c" * 64,
                    "retentionDeadline": "2030-01-02T03:04:05Z",
                }
            )
        else:
            raise AssertionError(f"unexpected mode: {execution_mode}")
        context.write_text(
            json.dumps(value, sort_keys=True), encoding="utf-8", newline="\n"
        )
        return cls(
            root=root,
            private_root=private_root,
            worktree=worktree,
            binary=binary,
            context=context,
            candidate_sha=candidate_sha,
            execution_mode=execution_mode,
        )

    @property
    def child_receipt(self) -> Path:
        return Path(str(self.binary) + ".receipt")

    def context_value(self) -> dict[str, object]:
        value = json.loads(self.context.read_text(encoding="utf-8"))
        assert isinstance(value, dict)
        return value

    def write_context(self, value: dict[str, object]) -> None:
        self.context.write_text(
            json.dumps(value, sort_keys=True), encoding="utf-8", newline="\n"
        )

    def apply_mutation(self, mutation: str) -> None:
        value = self.context_value()
        evaluator = value["frozenEvaluator"]
        assert isinstance(evaluator, dict)
        if mutation == "relative_context":
            self.context_argument = str(self.context.relative_to(self.root))
            return
        if mutation == "context_symlink":
            target = self.private_root / "coordinator" / "real-context.json"
            self.context.replace(target)
            self.context.symlink_to(target.name)
            return
        if mutation == "bad_schema_version":
            value["schemaVersion"] = True
        elif mutation == "bad_mode":
            value["executionMode"] = "future"
        elif mutation == "cross_mode_field":
            value["providerRole"] = "targetVolcengine"
        elif mutation == "wrong_platform":
            value["platform"] = "windows-11-x64"
        elif mutation == "windows_without_exe":
            evaluator["windows11X64Binary"] = str(self.root / "windows" / "evaluator")
        elif mutation == "candidate_mismatch":
            value["candidateSha"] = "0" * 40
        elif mutation == "worktree_dirty":
            (self.worktree / "untracked.txt").write_text("dirty\n", encoding="utf-8")
        elif mutation == "worktree_head_drift":
            (self.worktree / "drift.txt").write_text("drift\n", encoding="utf-8")
            _git(self.worktree, "add", "drift.txt")
            _git(self.worktree, "commit", "-q", "-m", "drift")
        elif mutation == "binary_hash_drift":
            self.binary.write_text("#!/bin/false\n", encoding="utf-8", newline="\n")
            self.binary.chmod(0o755)
        elif mutation == "binary_symlink":
            replacement = self.worktree / "replacement-evaluator"
            replacement.write_text("#!/bin/false\n", encoding="utf-8", newline="\n")
            replacement.chmod(0o755)
            self.binary.unlink()
            self.binary.symlink_to(replacement.name)
        elif mutation == "alternate_executable_token":
            self.alternate_child_argv = (
                (str(self.root / "other-program"),),
                ("./relative-program",),
                ("folder\\program",),
            )
        elif mutation == "duplicate_frozen_context":
            self.child_argv = ("replay-pair", "--frozen-run-context", "/other")
        elif mutation == "override_model":
            self.child_argv = ("replay-pair", "--model=override")
        elif mutation == "override_provider":
            self.child_argv = ("replay-pair", "--provider", "override")
        elif mutation == "override_case":
            self.child_argv = ("replay-pair", "--case-fixture=override")
        elif mutation == "override_binary":
            self.child_argv = ("replay-pair", "--binary", "/override")
        elif mutation == "override_budget":
            self.child_argv = ("replay-pair", "--budget-fen=1")
        elif mutation == "override_timeout":
            self.child_argv = ("replay-pair", "--timeout", "1")
        elif mutation == "override_limit":
            self.child_argv = ("replay-pair", "--max-output-tokens=1")
        else:
            raise AssertionError(f"unknown mutation: {mutation}")
        self.write_context(value)

    def run_wrapper(
        self, *, environment: dict[str, str] | None = None
    ) -> subprocess.CompletedProcess[str]:
        context_argument = self.context_argument or str(self.context)
        commands = self.alternate_child_argv or (self.child_argv,)
        completed: subprocess.CompletedProcess[str] | None = None
        for child_argv in commands:
            completed = subprocess.run(
                [
                    sys.executable,
                    str(WRAPPER),
                    "--context",
                    context_argument,
                    "--",
                    *child_argv,
                ],
                check=False,
                capture_output=True,
                text=True,
                cwd=self.root,
                env={
                    **os.environ,
                    "AI_IP_TEST_AUTHORITY": "must-not-reach-child",
                    **(environment or {}),
                },
            )
            assert completed.returncode == 1 or not self.alternate_child_argv
        assert completed is not None
        return completed


@pytest.mark.parametrize("execution_mode", ["replay", "live"])
def test_frozen_eval_executes_only_the_context_selected_evaluator(
    execution_mode: str, tmp_path: Path
) -> None:
    fixture = FrozenEvalFixture.create(tmp_path, execution_mode=execution_mode)

    completed = fixture.run_wrapper()

    assert completed.returncode == 0, completed.stderr
    receipt = json.loads(fixture.child_receipt.read_text(encoding="utf-8"))
    assert receipt["argv"] == [
        str(fixture.binary),
        "replay-pair",
        "--fixture-safe-flag",
        "value",
        "--frozen-run-context",
        str(fixture.context),
    ]
    assert "AI_IP_TEST_AUTHORITY" not in receipt["environment"]


def test_git_verification_uses_fixed_executable_and_environment(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    hostile_bin = tmp_path / "hostile-bin"
    hostile_bin.mkdir()
    hostile_receipt = tmp_path / "hostile-git.json"
    trusted_receipt = tmp_path / "trusted-git.json"
    hostile_git = hostile_bin / "git"
    hostile_git.write_text(
        "#!" + sys.executable + "\n"
        "from pathlib import Path\n"
        "import json, os\n"
        f"Path({str(hostile_receipt)!r}).write_text(json.dumps(dict(os.environ)))\n",
        encoding="utf-8",
        newline="\n",
    )
    trusted_git = tmp_path / "trusted-git"
    trusted_git.write_text(
        "#!" + sys.executable + "\n"
        "from pathlib import Path\n"
        "import json, os\n"
        f"Path({str(trusted_receipt)!r}).write_text(json.dumps(dict(os.environ)))\n",
        encoding="utf-8",
        newline="\n",
    )
    hostile_git.chmod(0o755)
    trusted_git.chmod(0o755)
    monkeypatch.setenv("PATH", str(hostile_bin))
    monkeypatch.setenv("AI_IP_GIT_SECRET_SENTINEL", "must-not-reach-git")
    monkeypatch.setattr(frozen, "TRUSTED_GIT_EXECUTABLE", trusted_git, raising=False)

    completed = frozen._run_git(tmp_path, "rev-parse", "HEAD")

    assert completed.returncode == 0
    assert not hostile_receipt.exists()
    trusted_environment = json.loads(trusted_receipt.read_text(encoding="utf-8"))
    assert "AI_IP_GIT_SECRET_SENTINEL" not in trusted_environment
    assert "PATH" not in trusted_environment


def test_wrapper_rejects_hostile_path_without_leaking_secret_to_child(
    tmp_path: Path,
) -> None:
    fixture = FrozenEvalFixture.create(tmp_path, execution_mode="replay")
    hostile_bin = tmp_path / "hostile-bin"
    hostile_bin.mkdir()
    hostile_receipt = tmp_path / "hostile-git-ran"
    hostile_git = hostile_bin / "git"
    hostile_git.write_text(
        "#!" + sys.executable + "\n"
        "from pathlib import Path\n"
        f"Path({str(hostile_receipt)!r}).write_text('ran')\n",
        encoding="utf-8",
        newline="\n",
    )
    hostile_git.chmod(0o755)

    completed = fixture.run_wrapper(
        environment={
            "PATH": str(hostile_bin),
            "AI_IP_GIT_SECRET_SENTINEL": "must-not-reach-git-or-child",
        }
    )

    assert not hostile_receipt.exists()
    assert completed.returncode == 0, completed.stderr
    receipt = json.loads(fixture.child_receipt.read_text(encoding="utf-8"))
    assert "AI_IP_GIT_SECRET_SENTINEL" not in receipt["environment"]


@pytest.mark.parametrize(
    "prefix",
    [
        ("--cont",),
        ("--context", "first", "--context", "second"),
        ("--context", "first", "unexpected"),
    ],
)
def test_wrapper_requires_exact_single_context_prefix(
    prefix: tuple[str, ...], tmp_path: Path
) -> None:
    fixture = FrozenEvalFixture.create(tmp_path, execution_mode="replay")
    arguments = list(prefix)
    if "first" in arguments:
        arguments[arguments.index("first")] = str(fixture.context)
    if "second" in arguments:
        arguments[arguments.index("second")] = str(fixture.context)
    if prefix == ("--cont",):
        arguments.append(str(fixture.context))
    completed = subprocess.run(
        [sys.executable, str(WRAPPER), *arguments, "--", "replay-pair"],
        check=False,
        capture_output=True,
        text=True,
        cwd=fixture.root,
    )

    assert completed.returncode == 1
    assert not fixture.child_receipt.exists()


def test_wrapper_preserves_evaluator_context_argument(tmp_path: Path) -> None:
    fixture = FrozenEvalFixture.create(tmp_path, execution_mode="replay")
    fixture.child_argv = ("replay-pair", "--context", "evaluator-owned-context")

    completed = fixture.run_wrapper()

    assert completed.returncode == 0, completed.stderr
    receipt = json.loads(fixture.child_receipt.read_text(encoding="utf-8"))
    assert receipt["argv"] == [
        str(fixture.binary),
        "replay-pair",
        "--context",
        "evaluator-owned-context",
        "--frozen-run-context",
        str(fixture.context),
    ]


def test_context_accepts_genuine_foreign_windows_path(tmp_path: Path) -> None:
    fixture = FrozenEvalFixture.create(tmp_path, execution_mode="replay")

    completed = fixture.run_wrapper()

    assert completed.returncode == 0, completed.stderr


@pytest.mark.parametrize(
    ("machine", "version", "expected"),
    [
        ("AMD64", SimpleNamespace(major=10, build=19045, product_type=1), None),
        ("AMD64", SimpleNamespace(major=10, build=22631, product_type=3), None),
        ("arm64", SimpleNamespace(major=10, build=22631, product_type=1), None),
        (
            "AMD64",
            SimpleNamespace(major=10, build=22631, product_type=1),
            "windows-11-x64",
        ),
    ],
)
def test_windows_host_selection_requires_windows_11_x64_workstation(
    machine: str,
    version: SimpleNamespace,
    expected: str | None,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(frozen.sys, "platform", "win32")
    monkeypatch.setattr(frozen.platform, "machine", lambda: machine)
    monkeypatch.setattr(frozen.sys, "getwindowsversion", lambda: version, raising=False)

    if expected is None:
        with pytest.raises(frozen.FrozenEvalError):
            frozen._host_platform()
    else:
        assert frozen._host_platform() == expected


def test_verified_dispatch_rejects_a_path_swap_when_fd_exec_is_unavailable(
    tmp_path: Path,
) -> None:
    fixture = FrozenEvalFixture.create(tmp_path, execution_mode="replay")
    digest = _sha256(fixture.binary)
    verified = frozen._open_verified_executable(fixture.binary, digest)
    replacement = fixture.worktree / "unverified-evaluator"
    replacement.write_text("#!/bin/false\n", encoding="utf-8", newline="\n")
    replacement.chmod(0o755)
    replacement.replace(fixture.binary)

    if os.execve not in os.supports_fd:
        with pytest.raises(frozen.FrozenEvalError, match="changed after verification"):
            frozen._exec(verified, ("replay-pair",), fixture.context)
        assert not fixture.child_receipt.exists()
        return
    raise AssertionError("add a platform fd-exec dispatch assertion for this host")


@pytest.mark.parametrize(
    "mutation",
    [
        "relative_context",
        "context_symlink",
        "bad_schema_version",
        "bad_mode",
        "cross_mode_field",
        "wrong_platform",
        "windows_without_exe",
        "candidate_mismatch",
        "worktree_dirty",
        "worktree_head_drift",
        "binary_hash_drift",
        "binary_symlink",
        "alternate_executable_token",
        "duplicate_frozen_context",
        "override_model",
        "override_provider",
        "override_case",
        "override_binary",
        "override_budget",
        "override_timeout",
        "override_limit",
    ],
)
def test_frozen_eval_rejects_authority_drift(mutation: str, tmp_path: Path) -> None:
    fixture = FrozenEvalFixture.create(tmp_path, execution_mode="replay")
    fixture.apply_mutation(mutation)

    completed = fixture.run_wrapper()

    assert completed.returncode == 1
    assert not fixture.child_receipt.exists()


def test_frozen_eval_rejects_unknown_frozen_evaluator_key(tmp_path: Path) -> None:
    fixture = FrozenEvalFixture.create(tmp_path, execution_mode="replay")
    value = fixture.context_value()
    evaluator = value["frozenEvaluator"]
    assert isinstance(evaluator, dict)
    evaluator["futureBinary"] = "/not-authorized"
    fixture.write_context(value)

    completed = fixture.run_wrapper()

    assert completed.returncode == 1
    assert not fixture.child_receipt.exists()


def test_frozen_eval_rejects_non_rfc3339_live_retention_deadline(
    tmp_path: Path,
) -> None:
    fixture = FrozenEvalFixture.create(tmp_path, execution_mode="live")
    value = fixture.context_value()
    value["retentionDeadline"] = "2030-01-02T03:04:05+0000"
    fixture.write_context(value)

    completed = fixture.run_wrapper()

    assert completed.returncode == 1
    assert not fixture.child_receipt.exists()
