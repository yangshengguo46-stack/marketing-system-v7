import hashlib
import importlib.util
import json
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass, replace
from pathlib import Path
from types import ModuleType, SimpleNamespace

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import verify_evidence as verifier


CAPTURE_SCRIPT = Path(__file__).with_name("capture_command.py")
HEX_64 = "a" * 64
GIT_ENV = {
    "GIT_AUTHOR_NAME": "Evidence Test",
    "GIT_AUTHOR_EMAIL": "evidence@example.invalid",
    "GIT_COMMITTER_NAME": "Evidence Test",
    "GIT_COMMITTER_EMAIL": "evidence@example.invalid",
}
TOOL_OUTPUTS = {
    "python": "Python 3.11.15",
    "uv": "uv 0.11.3 fixture",
    "git": "git version 2.54.0",
    "just": "just 1.58.0",
    "dotslash": "DotSlash 0.5.8",
    "rustc": "rustc 1.95.0 fixture",
    "cargo": "cargo 1.95.0 fixture",
    "cargo-nextest": "cargo-nextest 0.9.103 fixture",
    "cargo-deny": "cargo-deny 0.20.2",
    "node": "v26.4.0",
    "pnpm": "10.34.5",
    "bazelisk": "v1.28.1",
    "bazel": "bazel 9.0.0",
}
REPORT_KEYS = {
    "schemaVersion",
    "publicRunId",
    "decision",
    "forkSha",
    "codexBinarySha256",
    "evaluatorBinarySha256",
    "brokerComponentSha256",
    "modelLabel",
    "providerLabel",
    "providerCompatibilityName",
    "providerRole",
    "frozenRunContextCommitment",
    "executionContextCommitment",
    "attemptIndexRootCommitment",
    "providerEndpointCommitment",
    "privateCaseCommitment",
    "privateMaterialsCommitment",
    "sharedConfigSha256",
    "promptSha256",
    "additionalContextCommitment",
    "outputSchemaSha256",
    "normalizedThreadStartSha256",
    "normalizedTurnStartCommitment",
    "genericCatalogSha256",
    "candidateCatalogSha256",
    "candidateSkillUseVerified",
    "skillUseEvidenceCommitment",
    "pairManifestsCommitment",
    "attestationCommitment",
    "armOrderCommitment",
    "rateCardSha256",
    "reviewSubmissionsCommitment",
    "proofRootSha256",
    "rubricSha256",
    "reviewerCount",
    "experiencedOperatorOrDirectorCount",
    "candidatePreferenceCount",
    "medianGenericScore",
    "medianCandidateScore",
    "medianPairedDelta",
    "candidateSevereFailureCount",
    "genericUsage",
    "candidateUsage",
    "genericCostFen",
    "candidateCostFen",
    "providerRequestAttemptCounts",
    "providerCompletedResponseCounts",
    "usageScope",
    "privateEvidenceRetentionDeadline",
    "capabilityStatus",
    "retentionStatus",
    "sourceMaterialRetention",
    "retentionCloseoutReceiptPath",
    "generatedAt",
}


def _git(repo: Path, *args: str) -> str:
    completed = subprocess.run(
        ["/usr/local/bin/git", "-C", str(repo), *args],
        check=False,
        capture_output=True,
        text=True,
        env={**os.environ, **GIT_ENV},
    )
    if completed.returncode != 0:
        raise AssertionError(completed.stderr)
    return completed.stdout.strip()


def _commit(repo: Path, message: str) -> str:
    _git(repo, "add", "-A")
    _git(repo, "commit", "-q", "-m", message)
    return _git(repo, "rev-parse", "HEAD")


def _write_executable(path: Path, body: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(body, encoding="utf-8", newline="\n")
    path.chmod(0o755)


def _strict_json(path: Path) -> dict[str, object]:
    def reject(pairs: list[tuple[str, object]]) -> dict[str, object]:
        value: dict[str, object] = {}
        for key, item in pairs:
            if key in value:
                raise AssertionError(f"duplicate key: {key}")
            value[key] = item
        return value

    parsed = json.loads(path.read_bytes(), object_pairs_hook=reject)
    assert isinstance(parsed, dict)
    return parsed


def _write_json(path: Path, value: object) -> None:
    path.write_bytes(
        json.dumps(
            value,
            ensure_ascii=False,
            sort_keys=True,
            separators=(",", ":"),
            allow_nan=False,
        ).encode("utf-8")
        + b"\n"
    )


def _load_recorder(path: Path) -> ModuleType:
    name = f"fixture_capture_{hash(path)}"
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def _install_tool_path(tool_bin: Path) -> None:
    tool_bin.mkdir()
    for tool, output in TOOL_OUTPUTS.items():
        if tool == "git":
            body = (
                "#!/bin/sh\n"
                f"if test \"$1\" = --version; then printf '%s\\n' {output!r}; "
                'else exec /usr/local/bin/git "$@"; fi\n'
            )
        elif tool == "cargo":
            body = (
                "#!/bin/sh\n"
                f"if test \"$1\" = nextest; then printf '%s\\n' {TOOL_OUTPUTS['cargo-nextest']!r}\n"
                f"elif test \"$1\" = deny; then printf '%s\\n' {TOOL_OUTPUTS['cargo-deny']!r}\n"
                f"else printf '%s\\n' {output!r}; fi\n"
            )
        elif tool == "node":
            body = (
                "#!/bin/sh\n"
                f"if test \"$1\" = -p; then printf '%s\\n' {TOOL_OUTPUTS['bazelisk']!r}; "
                f"else printf '%s\\n' {output!r}; fi\n"
            )
        elif tool == "pnpm":
            body = (
                "#!/bin/sh\n"
                f"if test \"$1\" = install; then printf 'installed\\n'; "
                f"else printf '%s\\n' {output!r}; fi\n"
            )
        else:
            body = f"#!/bin/sh\nprintf '%s\\n' {output!r}\n"
        _write_executable(tool_bin / tool, body)
    (tool_bin / "package.json").write_text(
        '{"name":"@bazel/bazelisk","version":"v1.28.1"}\n', encoding="utf-8"
    )


@dataclass
class ValidEvidenceFixture:
    repo: Path
    tools: Path
    matrix: Path
    baseline: Path
    post: Path
    windows_post: Path
    request: verifier.VerificationRequest
    post_request: verifier.VerificationRequest
    tested_sha: str
    tools_sha: str

    @classmethod
    def create(
        cls, tmp_path: Path, *, blocked_baseline: bool = False
    ) -> "ValidEvidenceFixture":
        tmp_path.mkdir(parents=True, exist_ok=True)
        repo = tmp_path / "tested"
        tools = tmp_path / "tools"
        baseline = tmp_path / "baseline"
        post = tmp_path / "post"
        windows_post = tmp_path / "windows" / "post"
        repo.mkdir()
        tools.mkdir()
        baseline.mkdir()
        post.mkdir()
        windows_post.mkdir(parents=True)
        _git(repo, "init", "-q")
        _git(repo, "config", "user.name", "Evidence Test")
        _git(repo, "config", "user.email", "evidence@example.invalid")
        for relative in ("codex-rs/Cargo.lock", "pnpm-lock.yaml", "MODULE.bazel.lock"):
            path = repo / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"fixture {relative}\n", encoding="utf-8")
        _write_executable(repo / "base-pass", "#!/bin/sh\nprintf 'base pass\\n'\n")
        blocked_exit = 7 if blocked_baseline else 0
        _write_executable(
            repo / "base-paired",
            f"#!/bin/sh\nprintf 'paired\\n'\nexit {blocked_exit}\n",
        )
        _write_executable(repo / "post-extra", "#!/bin/sh\nprintf 'post pass\\n'\n")
        tested_sha = _commit(repo, "tested fixture")
        _git(repo, "switch", "--detach", "-q")

        foundation = tools / "scripts/ai_ip/foundation"
        foundation.mkdir(parents=True)
        shutil.copyfile(CAPTURE_SCRIPT, foundation / "capture_command.py")
        matrix = foundation / "required_command_matrix.json"
        _write_json(
            matrix,
            {
                "schemaVersion": 1,
                "commands": [
                    {
                        "id": "base-pass",
                        "platforms": ["macos-x86_64", "windows-11-x64"],
                        "phase": "baselineAndPost",
                        "argv": ["./base-pass"],
                        "expectedExit": 0,
                    },
                    {
                        "id": "base-paired",
                        "platforms": ["macos-x86_64", "windows-11-x64"],
                        "phase": "baselineAndPost",
                        "argv": ["./base-paired"],
                        "expectedExit": 0,
                    },
                    {
                        "id": "post-extra",
                        "platforms": ["macos-x86_64", "windows-11-x64"],
                        "phase": "postOnly",
                        "argv": ["./post-extra"],
                        "expectedExit": 0,
                    },
                ],
            },
        )
        _git(tools, "init", "-q")
        _git(tools, "config", "user.name", "Evidence Test")
        _git(tools, "config", "user.email", "evidence@example.invalid")
        tools_sha = _commit(tools, "tools fixture")
        _git(tools, "switch", "--detach", "-q")
        recorder = _load_recorder(foundation / "capture_command.py")
        tool_bin = tmp_path / "tool-bin"
        _install_tool_path(tool_bin)
        prior_path = os.environ.get("PATH")
        os.environ["PATH"] = str(tool_bin)
        try:
            for root, platform_id, architecture, mode in (
                (baseline, "macos-x86_64", "x86_64", "baseline"),
                (post, "macos-x86_64", "x86_64", "post"),
                (windows_post, "windows-11-x64", "AMD64", "post"),
            ):
                recorder.host_id = lambda *args, value=platform_id, **kwargs: value
                recorder.platform.machine = lambda value=architecture: value
                context = recorder._resolve_request(str(repo), str(root), str(matrix))
                assert recorder._bootstrap(context) == 0
                if platform_id == "windows-11-x64":
                    bootstrap_path = root / "host-bootstrap.manifest.json"
                    bootstrap = _strict_json(bootstrap_path)
                    bootstrap["administratorToken"] = False
                    _write_json(bootstrap_path, bootstrap)
                loaded = recorder.load_matrix(matrix)
                for command in loaded.required_for(platform_id, mode):
                    result = recorder._capture_command(command, context)
                    assert result == (
                        blocked_exit if command.id == "base-paired" else 0
                    )
        finally:
            if prior_path is None:
                os.environ.pop("PATH", None)
            else:
                os.environ["PATH"] = prior_path
        request = verifier.VerificationRequest(
            repo_root=repo.resolve(),
            matrix_path=matrix.resolve(),
            evidence_root=baseline.resolve(),
            platform="macos-x86_64",
            mode="baseline",
        )
        post_request = replace(request, evidence_root=post.resolve(), mode="post")
        fixture = cls(
            repo.resolve(),
            tools.resolve(),
            matrix.resolve(),
            baseline.resolve(),
            post.resolve(),
            windows_post.resolve(),
            request,
            post_request,
            tested_sha,
            tools_sha,
        )
        if blocked_baseline:
            disposition = verifier.verify_evidence(request)
            _write_json(
                baseline / "baseline-summary.json",
                verifier.disposition_dict(disposition),
            )
        return fixture

    def _manifest(self, root: Path, command_id: str) -> Path:
        return root / f"{command_id}.manifest.json"

    def clone(self, destination: Path) -> "ValidEvidenceFixture":
        source_root = self.repo.parent
        root = destination / "evidence-fixture"
        shutil.copytree(source_root, root)

        def mapped(path: Path) -> Path:
            return root / path.relative_to(source_root)

        request = replace(
            self.request,
            repo_root=mapped(self.repo),
            matrix_path=mapped(self.matrix),
            evidence_root=mapped(self.baseline),
        )
        post_request = replace(
            self.post_request,
            repo_root=mapped(self.repo),
            matrix_path=mapped(self.matrix),
            evidence_root=mapped(self.post),
        )
        return ValidEvidenceFixture(
            repo=mapped(self.repo),
            tools=mapped(self.tools),
            matrix=mapped(self.matrix),
            baseline=mapped(self.baseline),
            post=mapped(self.post),
            windows_post=mapped(self.windows_post),
            request=request,
            post_request=post_request,
            tested_sha=self.tested_sha,
            tools_sha=self.tools_sha,
        )

    def _mutate_json(self, path: Path, key: str, value: object) -> None:
        parsed = _strict_json(path)
        parsed[key] = value
        _write_json(path, parsed)

    def apply_mutation(self, mutation: str) -> None:
        root = self.baseline
        manifest = self._manifest(root, "base-pass")
        if mutation == "missing_bootstrap":
            (root / "host-bootstrap.manifest.json").unlink()
        elif mutation == "missing_manifest":
            manifest.unlink()
        elif mutation == "missing_stdout":
            (root / "base-pass.stdout.log").unlink()
        elif mutation == "missing_stderr":
            (root / "base-pass.stderr.log").unlink()
        elif mutation == "extra_manifest":
            shutil.copyfile(manifest, root / "extra.manifest.json")
        elif mutation == "log_bytes":
            parsed = _strict_json(manifest)
            parsed["stdout"]["bytes"] += 1
            _write_json(manifest, parsed)
        elif mutation == "log_sha":
            parsed = _strict_json(manifest)
            parsed["stdout"]["sha256"] = HEX_64
            _write_json(manifest, parsed)
        elif mutation == "matrix_id":
            self._mutate_json(manifest, "commandId", "base-paired")
        elif mutation == "matrix_argv":
            self._mutate_json(manifest, "argv", ["./other"])
        elif mutation == "matrix_expected_exit":
            self._mutate_json(manifest, "expectedExit", 1)
        elif mutation == "wrong_platform":
            self._mutate_json(manifest, "platform", "windows-11-x64")
        elif mutation == "wrong_arch":
            self._mutate_json(manifest, "architecture", "arm64")
        elif mutation == "tested_sha":
            self._mutate_json(manifest, "testedGitSha", "0" * 40)
        elif mutation == "tools_sha":
            self._mutate_json(manifest, "toolsGitSha", "0" * 40)
        elif mutation == "recorder_sha":
            self._mutate_json(manifest, "recorderSha256", HEX_64)
        elif mutation in {"cargo_lock", "pnpm_lock", "bazel_lock"}:
            names = {
                "cargo_lock": "cargoLockSha256",
                "pnpm_lock": "pnpmLockSha256",
                "bazel_lock": "bazelLockSha256",
            }
            parsed = _strict_json(manifest)
            parsed["locks"][names[mutation]] = HEX_64
            _write_json(manifest, parsed)
        elif mutation == "unknown_status":
            self._mutate_json(manifest, "status", "IGNORED")
        elif mutation == "post_only_in_baseline":
            for suffix in ("manifest.json", "stdout.log", "stderr.log"):
                shutil.copyfile(
                    self.post / f"post-extra.{suffix}", root / f"post-extra.{suffix}"
                )
        elif mutation == "unpaired_post_failure":
            self.request = self.post_request
            post_manifest = self._manifest(self.post, "post-extra")
            parsed = _strict_json(post_manifest)
            parsed["exitCode"] = 9
            parsed["status"] = "BLOCKED_BASELINE"
            _write_json(post_manifest, parsed)
        else:
            raise AssertionError(f"unknown mutation: {mutation}")


@pytest.fixture(scope="session")
def valid_evidence_template(
    tmp_path_factory: pytest.TempPathFactory,
) -> ValidEvidenceFixture:
    fixture = ValidEvidenceFixture.create(tmp_path_factory.mktemp("valid-evidence"))
    disposition = verifier.verify_evidence(fixture.request)
    _write_json(
        fixture.baseline / "baseline-summary.json",
        verifier.disposition_dict(disposition),
    )
    return fixture


@pytest.fixture(scope="session")
def blocked_evidence_template(
    tmp_path_factory: pytest.TempPathFactory,
) -> ValidEvidenceFixture:
    return ValidEvidenceFixture.create(
        tmp_path_factory.mktemp("blocked-evidence"), blocked_baseline=True
    )


@pytest.mark.parametrize(
    "mutation",
    [
        "missing_bootstrap",
        "missing_manifest",
        "missing_stdout",
        "missing_stderr",
        "extra_manifest",
        "log_bytes",
        "log_sha",
        "matrix_id",
        "matrix_argv",
        "matrix_expected_exit",
        "wrong_platform",
        "wrong_arch",
        "tested_sha",
        "tools_sha",
        "recorder_sha",
        "cargo_lock",
        "pnpm_lock",
        "bazel_lock",
        "unknown_status",
        "post_only_in_baseline",
        "unpaired_post_failure",
    ],
)
def test_public_evidence_tampering_is_rejected(mutation: str, tmp_path: Path) -> None:
    fixture = ValidEvidenceFixture.create(tmp_path)
    fixture.apply_mutation(mutation)
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_evidence(fixture.request)


def test_valid_evidence_returns_full_immutable_disposition(tmp_path: Path) -> None:
    fixture = ValidEvidenceFixture.create(tmp_path)
    matrix_sha = hashlib.sha256(
        verifier.canonical_json_bytes(_strict_json(fixture.matrix))
    ).hexdigest()
    disposition = verifier.verify_evidence(fixture.request)
    assert disposition == verifier.EvidenceDisposition(
        platform="macos-x86_64",
        mode="baseline",
        tested_git_sha=fixture.tested_sha,
        tools_git_sha=fixture.tools_sha,
        matrix_sha256=matrix_sha,
        command_ids=("base-pass", "base-paired"),
        blocked_ids=(),
    )
    assert disposition.status == "PASS"


def test_post_accepts_only_failures_already_blocked_in_baseline(tmp_path: Path) -> None:
    fixture = ValidEvidenceFixture.create(tmp_path, blocked_baseline=True)
    disposition = verifier.verify_evidence(fixture.post_request)
    assert disposition.status == "BLOCKED_BASELINE"
    assert disposition.blocked_ids == ("base-paired",)


def _block_post_command(fixture: ValidEvidenceFixture, command_id: str) -> None:
    manifest_path = fixture.post / f"{command_id}.manifest.json"
    manifest = _strict_json(manifest_path)
    manifest["exitCode"] = 9
    manifest["status"] = "BLOCKED_BASELINE"
    _write_json(manifest_path, manifest)


def test_post_rejects_forged_baseline_summary_inside_post_root(
    tmp_path: Path, valid_evidence_template: ValidEvidenceFixture
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    forged = {
        "schemaVersion": 1,
        "platform": "macos-x86_64",
        "mode": "baseline",
        "testedGitSha": fixture.tested_sha,
        "toolsGitSha": fixture.tools_sha,
        "matrixSha256": hashlib.sha256(
            verifier.canonical_json_bytes(_strict_json(fixture.matrix))
        ).hexdigest(),
        "commandIds": ["base-pass", "base-paired", "post-extra"],
        "blockedIds": ["post-extra"],
        "status": "BLOCKED_BASELINE",
    }
    _write_json(fixture.post / "baseline-summary.json", forged)
    _block_post_command(fixture, "post-extra")
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_evidence(fixture.post_request)


def test_post_rejects_post_root_baseline_summary_even_when_post_passes(
    tmp_path: Path, valid_evidence_template: ValidEvidenceFixture
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    _write_json(
        fixture.post / "baseline-summary.json",
        {
            "schemaVersion": 1,
            "platform": "macos-x86_64",
            "mode": "baseline",
            "testedGitSha": fixture.tested_sha,
            "toolsGitSha": fixture.tools_sha,
            "matrixSha256": hashlib.sha256(
                verifier.canonical_json_bytes(_strict_json(fixture.matrix))
            ).hexdigest(),
            "commandIds": ["base-pass", "base-paired"],
            "blockedIds": [],
            "status": "PASS",
        },
    )
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_evidence(fixture.post_request)


def test_post_rejects_tampered_sibling_baseline_evidence(
    tmp_path: Path, blocked_evidence_template: ValidEvidenceFixture
) -> None:
    fixture = blocked_evidence_template.clone(tmp_path)
    with (fixture.baseline / "base-paired.stdout.log").open("ab") as output:
        output.write(b"tampered\n")
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_evidence(fixture.post_request)


@pytest.mark.parametrize("summary_state", ["missing", "mismatched"])
def test_post_requires_exact_recomputed_sibling_baseline_summary(
    summary_state: str,
    tmp_path: Path,
    blocked_evidence_template: ValidEvidenceFixture,
) -> None:
    fixture = blocked_evidence_template.clone(tmp_path)
    summary = fixture.baseline / "baseline-summary.json"
    if summary_state == "missing":
        summary.unlink()
    else:
        value = _strict_json(summary)
        value["blockedIds"] = []
        value["status"] = "PASS"
        _write_json(summary, value)
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_evidence(fixture.post_request)


def test_post_pairs_only_against_recomputed_historical_baseline(
    tmp_path: Path, blocked_evidence_template: ValidEvidenceFixture
) -> None:
    fixture = blocked_evidence_template.clone(tmp_path)
    disposition = verifier.verify_evidence(fixture.post_request)
    assert disposition.blocked_ids == ("base-paired",)


def test_verify_rejects_identical_manifest_inode_swap_after_scan(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    valid_evidence_template: ValidEvidenceFixture,
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    real_scan = verifier.scan_forbidden_evidence

    def scan_then_swap(root: Path) -> tuple[verifier.ForbiddenMatch, ...]:
        matches = real_scan(root)
        manifest = root / "host-bootstrap.manifest.json"
        replacement = root / "replacement.tmp"
        replacement.write_bytes(manifest.read_bytes())
        replacement.replace(manifest)
        return matches

    monkeypatch.setattr(verifier, "scan_forbidden_evidence", scan_then_swap)
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_evidence(fixture.request)


def test_recursive_scanner_rejects_identical_entry_swap_after_child_scan(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    root = tmp_path / "evidence"
    root.mkdir()
    path = root / "safe.log"
    path.write_bytes(b"safe\n")
    scan_one = verifier._scan_one

    def scan_then_swap(
        target: Path, display: str
    ) -> tuple[verifier.ForbiddenMatch, ...]:
        matches = scan_one(target, display)
        replacement = target.with_name("replacement.tmp")
        replacement.write_bytes(target.read_bytes())
        replacement.replace(target)
        return matches

    monkeypatch.setattr(verifier, "_scan_one", scan_then_swap)
    with pytest.raises(verifier.EvidenceError):
        verifier.scan_forbidden_evidence(root)


def test_log_verification_streams_without_whole_file_materialization(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    valid_evidence_template: ValidEvidenceFixture,
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    stable_file_bytes = verifier._stable_file_bytes

    def reject_log_materialization(path: Path) -> bytes:
        if path.suffix == ".log":
            raise AssertionError("log was materialized")
        return stable_file_bytes(path)

    monkeypatch.setattr(verifier, "_stable_file_bytes", reject_log_materialization)
    assert verifier.verify_evidence(fixture.request).status == "PASS"


def test_verify_rejects_identical_matrix_swap_after_snapshot(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    valid_evidence_template: ValidEvidenceFixture,
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    capture_file = verifier._FileSnapshot.capture.__func__

    def capture_then_swap(
        cls: type[verifier._FileSnapshot],
        path: Path,
        display: str,
        *,
        keep_payload: bool,
    ) -> verifier._FileSnapshot:
        snapshot = capture_file(cls, path, display, keep_payload=keep_payload)
        if display == "required-command-matrix.json":
            replacement = path.with_name("matrix-replacement.tmp")
            replacement.write_bytes(path.read_bytes())
            replacement.replace(path)
        return snapshot

    monkeypatch.setattr(
        verifier._FileSnapshot, "capture", classmethod(capture_then_swap)
    )
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_evidence(fixture.request)


def test_cli_exits_zero_one_and_two_and_publishes_exact_summary(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    passing = ValidEvidenceFixture.create(tmp_path / "passing")
    args = [
        "--repo-root",
        str(passing.repo),
        "--matrix",
        str(passing.matrix),
        "--evidence-root",
        str(passing.baseline),
        "--platform",
        "macos-x86_64",
        "--mode",
        "baseline",
        "--summary-output",
        str(passing.baseline / "baseline-summary.json"),
    ]
    assert verifier.main(args) == 0
    stdout = capsys.readouterr().out.encode()
    assert stdout == (passing.baseline / "baseline-summary.json").read_bytes()
    assert verifier.main(args[:-2]) == 0
    capsys.readouterr()

    invalid = ValidEvidenceFixture.create(tmp_path / "invalid")
    invalid.apply_mutation("missing_manifest")
    assert (
        verifier.main(
            args[:0]
            + [
                "--repo-root",
                str(invalid.repo),
                "--matrix",
                str(invalid.matrix),
                "--evidence-root",
                str(invalid.baseline),
                "--platform",
                "macos-x86_64",
                "--mode",
                "baseline",
            ]
        )
        == 1
    )
    assert capsys.readouterr().out == ""

    blocked = ValidEvidenceFixture.create(tmp_path / "blocked", blocked_baseline=True)
    assert (
        verifier.main(
            [
                "--repo-root",
                str(blocked.repo),
                "--matrix",
                str(blocked.matrix),
                "--evidence-root",
                str(blocked.baseline),
                "--platform",
                "macos-x86_64",
                "--mode",
                "baseline",
            ]
        )
        == 2
    )
    assert json.loads(capsys.readouterr().out)["status"] == "BLOCKED_BASELINE"


FORBIDDEN_SAMPLES = {
    "unix-user-home": b"/Users/alice/project",
    "windows-user-home": b"C:\\Users\\alice\\project",
    "authorization-header": b"Authorization: opaque-value",
    "bearer-token": b"Bearer abcdefgh12345678",
    "credential-assignment": b"api_key=opaque-value",
    "codex-auth-path": b".codex/auth.json",
    "private-json-body-key": b'"privateCase":',
}


@pytest.mark.parametrize(("rule_id", "sample"), FORBIDDEN_SAMPLES.items())
@pytest.mark.parametrize("boundary", [False, True], ids=["single-chunk", "boundary"])
def test_scanner_reports_exact_rule_without_matched_bytes(
    rule_id: str, sample: bytes, boundary: bool, tmp_path: Path
) -> None:
    root = tmp_path / "evidence"
    root.mkdir()
    prefix = b"safe\n"
    if boundary:
        split = max(1, len(sample) // 2)
        prefix = b"x" * (verifier.CHUNK_SIZE - split - 1) + b" "
    (root / "public.log").write_bytes(prefix + sample + b"\n")
    matches = verifier.scan_forbidden_evidence(root)
    assert matches == (verifier.ForbiddenMatch("public.log", rule_id),)
    assert sample.decode("ascii", errors="ignore") not in repr(matches)


@pytest.mark.parametrize(
    "safe",
    [
        b"prompt engineering",
        b"response status",
        b"prompt",
        b"a" * 64,
        b"test(=suite::v2::safe_name)",
        b"docs/evidence/foundation/public.log",
    ],
)
def test_scanner_allows_public_nonsecret_text(safe: bytes, tmp_path: Path) -> None:
    root = tmp_path / "evidence"
    root.mkdir()
    (root / "safe.log").write_bytes(safe)
    assert verifier.scan_forbidden_evidence(root) == ()


@pytest.mark.parametrize("unsafe_kind", ["symlink", "hardlink", "fifo"])
def test_scanner_rejects_unsafe_filesystem_entries(
    unsafe_kind: str, tmp_path: Path
) -> None:
    root = tmp_path / "evidence"
    root.mkdir()
    target = tmp_path / "target"
    target.write_bytes(b"safe")
    unsafe = root / "unsafe.log"
    if unsafe_kind == "symlink":
        unsafe.symlink_to(target)
    elif unsafe_kind == "hardlink":
        os.link(target, unsafe)
    else:
        if not hasattr(os, "mkfifo"):
            pytest.skip("FIFO is unavailable")
        os.mkfifo(unsafe)
    with pytest.raises(verifier.EvidenceError):
        verifier.scan_forbidden_evidence(root)


def test_scanner_rejects_symlink_path_escape(tmp_path: Path) -> None:
    outside = tmp_path / "outside"
    outside.mkdir()
    (outside / "safe.log").write_bytes(b"safe")
    root = tmp_path / "evidence"
    root.mkdir()
    (root / "escape").symlink_to(outside, target_is_directory=True)
    with pytest.raises(verifier.EvidenceError):
        verifier.scan_forbidden_evidence(root)


def test_scanner_rejects_file_that_changes_while_read(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    root = tmp_path / "evidence"
    root.mkdir()
    path = root / "race.log"
    path.write_bytes(b"safe")
    real_fstat = verifier.os.fstat
    calls = 0

    def changing_fstat(descriptor: int) -> os.stat_result:
        nonlocal calls
        calls += 1
        result = real_fstat(descriptor)
        if calls >= 2:
            values = list(result)
            values[6] += 1
            return os.stat_result(values)
        return result

    monkeypatch.setattr(verifier.os, "fstat", changing_fstat)
    with pytest.raises(verifier.EvidenceError):
        verifier.scan_forbidden_evidence(root)


def _business_report(candidate_sha: str) -> dict[str, object]:
    capability = {
        "codePresent": True,
        "mechanicalContracts": "pendingFoundationVerification",
        "liveProviderReachable": True,
        "businessBlindReview": "passed",
        "publicationRetro": "notRun",
    }
    report: dict[str, object] = {key: HEX_64 for key in REPORT_KEYS}
    report.update(
        {
            "schemaVersion": 1,
            "publicRunId": "public-run-1234567890abcdef",
            "decision": "BUSINESS_SIGNAL_PASS_PENDING_FOUNDATION",
            "forkSha": candidate_sha,
            "modelLabel": "approved-model-revision",
            "providerLabel": "approved-provider",
            "providerCompatibilityName": "OpenAI",
            "providerRole": "targetVolcengine",
            "candidateSkillUseVerified": True,
            "reviewerCount": 3,
            "experiencedOperatorOrDirectorCount": 2,
            "candidatePreferenceCount": 2,
            "medianGenericScore": 7,
            "medianCandidateScore": 9,
            "medianPairedDelta": 2,
            "candidateSevereFailureCount": 0,
            "genericUsage": {},
            "candidateUsage": {},
            "genericCostFen": 12,
            "candidateCostFen": 13,
            "providerRequestAttemptCounts": [1, 1],
            "providerCompletedResponseCounts": [1, 1],
            "usageScope": "rootSessionTree",
            "privateEvidenceRetentionDeadline": "2030-01-02T00:00:00.000000Z",
            "capabilityStatus": capability,
            "retentionStatus": "pending",
            "sourceMaterialRetention": "userOwnedOriginalsOutsideProofCopyScope",
            "retentionCloseoutReceiptPath": "retention-closeout.json",
            "generatedAt": "2030-01-01T00:00:00.000000Z",
        }
    )
    assert set(report) == REPORT_KEYS
    return report


@dataclass
class FinalFixture:
    root: Path
    evidence: ValidEvidenceFixture
    report: Path
    index: Path
    receipt: Path
    output: Path
    request: verifier.FinalVerificationRequest

    @classmethod
    def create(cls, tmp_path: Path) -> "FinalFixture":
        evidence = ValidEvidenceFixture.create(tmp_path / "foundation")
        mac_baseline = verifier.verify_evidence(evidence.request)
        _write_json(
            evidence.baseline / "baseline-summary.json",
            verifier.disposition_dict(mac_baseline),
        )
        windows_baseline_root = evidence.windows_post.parent / "baseline"
        windows_baseline_root.mkdir()
        for name in (
            "host-bootstrap.manifest.json",
            "base-pass.manifest.json",
            "base-pass.stdout.log",
            "base-pass.stderr.log",
            "base-paired.manifest.json",
            "base-paired.stdout.log",
            "base-paired.stderr.log",
        ):
            shutil.copyfile(evidence.windows_post / name, windows_baseline_root / name)
        for path in evidence.windows_post.glob("*.version.*.log"):
            shutil.copyfile(path, windows_baseline_root / path.name)
        for path in evidence.windows_post.glob("dependency-install.*.log"):
            shutil.copyfile(path, windows_baseline_root / path.name)
        windows_baseline = verifier.verify_evidence(
            replace(
                evidence.request,
                evidence_root=windows_baseline_root,
                platform="windows-11-x64",
            )
        )
        _write_json(
            windows_baseline_root / "baseline-summary.json",
            verifier.disposition_dict(windows_baseline),
        )
        report = (
            evidence.repo
            / "docs/evidence/business-proof/public-run-1234567890abcdef/report.json"
        )
        report.parent.mkdir(parents=True)
        report_value = _business_report(evidence.tested_sha)
        _write_json(report, report_value)
        report_sha = hashlib.sha256(report.read_bytes()).hexdigest()
        report_path = report.relative_to(evidence.repo).as_posix()
        index = evidence.repo / "docs/evidence/business-proof/index.json"
        index_value = {
            "schemaVersion": 1,
            "attempts": [
                {
                    "attemptOrdinal": 1,
                    "publicRunId": report_value["publicRunId"],
                    "reportPath": report_path,
                    "reportSha256": report_sha,
                    "candidateSha": evidence.tested_sha,
                    "decision": report_value["decision"],
                    "supersedes": None,
                    "candidateDiffSha256": HEX_64,
                    "caseCommitment": "b" * 64,
                    "materialsCommitment": "c" * 64,
                    "selectedForCheckpoint": True,
                }
            ],
        }
        _write_json(index, index_value)
        index_sha = hashlib.sha256(index.read_bytes()).hexdigest()
        receipt = tmp_path / "live-proof-verification.json"
        receipt_value = {
            "schemaVersion": 1,
            "publicRunId": report_value["publicRunId"],
            "candidateSha": evidence.tested_sha,
            "reportPath": report_path,
            "reportSha256": report_sha,
            "reportIndexSha256": index_sha,
            "selectedAttemptOrdinal": 1,
            "proofRootSha256": report_value["proofRootSha256"],
            "frozenRunContextCommitment": report_value["frozenRunContextCommitment"],
            "executionContextCommitment": report_value["executionContextCommitment"],
            "attemptIndexRootCommitment": report_value["attemptIndexRootCommitment"],
            "brokerReceiptCommitment": "d" * 64,
            "costReceiptsCommitment": "e" * 64,
            "candidateAllowedDiffSha256": HEX_64,
            "G2": "PASS",
            "capabilityStatus": report_value["capabilityStatus"],
            "verifiedAt": "2030-01-01T12:00:00.000000Z",
            "verifiedBeforeRetentionDeadline": True,
        }
        _write_json(receipt, receipt_value)
        output_parent = tmp_path / "private-output"
        output_parent.mkdir()
        output = output_parent / "verification.next.json"
        request = verifier.FinalVerificationRequest(
            repo_root=evidence.repo,
            matrix_path=evidence.matrix,
            mac_post_root=evidence.post,
            windows_post_root=evidence.windows_post,
            candidate_sha=evidence.tested_sha,
            selected_report=report,
            report_index=index,
            business_verification_receipt=receipt,
            verification_output=output,
        )
        return cls(tmp_path, evidence, report, index, receipt, output, request)

    def clone(self, destination: Path) -> "FinalFixture":
        root = destination / "final-fixture"
        shutil.copytree(self.root, root)

        def mapped(path: Path) -> Path:
            return root / path.relative_to(self.root)

        original = self.evidence
        evidence = ValidEvidenceFixture(
            repo=mapped(original.repo),
            tools=mapped(original.tools),
            matrix=mapped(original.matrix),
            baseline=mapped(original.baseline),
            post=mapped(original.post),
            windows_post=mapped(original.windows_post),
            request=replace(
                original.request,
                repo_root=mapped(original.repo),
                matrix_path=mapped(original.matrix),
                evidence_root=mapped(original.baseline),
            ),
            post_request=replace(
                original.post_request,
                repo_root=mapped(original.repo),
                matrix_path=mapped(original.matrix),
                evidence_root=mapped(original.post),
            ),
            tested_sha=original.tested_sha,
            tools_sha=original.tools_sha,
        )
        report = mapped(self.report)
        index = mapped(self.index)
        receipt = mapped(self.receipt)
        output = mapped(self.output)
        request = replace(
            self.request,
            repo_root=evidence.repo,
            matrix_path=evidence.matrix,
            mac_post_root=evidence.post,
            windows_post_root=evidence.windows_post,
            selected_report=report,
            report_index=index,
            business_verification_receipt=receipt,
            verification_output=output,
        )
        return FinalFixture(root, evidence, report, index, receipt, output, request)


def _refresh_final_bindings(fixture: FinalFixture) -> None:
    report_sha = hashlib.sha256(fixture.report.read_bytes()).hexdigest()
    index = _strict_json(fixture.index)
    index["attempts"][0]["reportSha256"] = report_sha
    _write_json(fixture.index, index)
    receipt = _strict_json(fixture.receipt)
    report = _strict_json(fixture.report)
    receipt["reportSha256"] = report_sha
    receipt["reportIndexSha256"] = hashlib.sha256(
        fixture.index.read_bytes()
    ).hexdigest()
    receipt["capabilityStatus"] = report["capabilityStatus"]
    _write_json(fixture.receipt, receipt)


@pytest.fixture(scope="session")
def final_fixture_template(tmp_path_factory: pytest.TempPathFactory) -> FinalFixture:
    return FinalFixture.create(tmp_path_factory.mktemp("final-template"))


def test_frozen_final_verifies_public_bindings_and_writes_exact_allowlist(
    tmp_path: Path, final_fixture_template: FinalFixture
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    result = verifier.verify_frozen_final(fixture.request)
    assert set(result) == verifier.FINAL_OUTPUT_KEYS
    assert fixture.output.read_bytes() == verifier.canonical_json_bytes(result) + b"\n"
    assert result["candidateSha"] == fixture.evidence.tested_sha
    assert result["G0"] == result["G1"] == result["G2"] == "PASS"
    assert result["foundationDecision"] == "PASS_TO_PHASE_0B"
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_frozen_final(fixture.request)


def test_frozen_final_rejects_identical_report_swap_before_publication(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    final_fixture_template: FinalFixture,
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    validate_report = verifier._validate_report

    def validate_then_swap(
        path: Path, repo: Path, candidate_sha: str, payload: bytes
    ) -> dict[str, object]:
        report = validate_report(path, repo, candidate_sha, payload)
        replacement = path.with_name("report-replacement.tmp")
        replacement.write_bytes(path.read_bytes())
        replacement.replace(path)
        return report

    monkeypatch.setattr(verifier, "_validate_report", validate_then_swap)
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_frozen_final(fixture.request)
    assert not fixture.output.exists()


def test_final_publication_failure_removes_reserved_temp(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo = tmp_path / "repo"
    output_parent = tmp_path / "output"
    repo.mkdir()
    output_parent.mkdir()
    output = output_parent / "verification.next.json"

    def lose_publication_race(temp: Path, final: Path) -> None:
        raise verifier.EvidenceError(f"already exists: {final.name}")

    monkeypatch.setattr(verifier.capture, "_publish_create_new", lose_publication_race)
    with pytest.raises(verifier.EvidenceError, match="already exists"):
        verifier._publish_create_new(output, b"payload\n", repo)
    assert not list(output_parent.glob("*.tmp-*"))


@pytest.mark.parametrize(
    "private_value",
    [
        "thread-123",
        "response-123",
        "reviewer-123",
        "private material",
        "prompt body",
        "output body",
    ],
)
def test_final_rejects_private_material_in_allowed_nested_value(
    private_value: str,
    tmp_path: Path,
    final_fixture_template: FinalFixture,
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    report = _strict_json(fixture.report)
    report["capabilityStatus"]["mechanicalContracts"] = private_value
    _write_json(fixture.report, report)
    _refresh_final_bindings(fixture)
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_frozen_final(fixture.request)


@pytest.mark.parametrize(
    "private_key",
    [
        "threadId",
        "responseId",
        "reviewerId",
        "privateRoot",
        "prompt",
        "outputBody",
        "reviewerMapping",
        "privateCase",
        "caseBody",
    ],
)
def test_final_rejects_private_material_in_allowed_nested_key(
    private_key: str,
    tmp_path: Path,
    final_fixture_template: FinalFixture,
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    report = _strict_json(fixture.report)
    report["genericUsage"] = {private_key: "opaque"}
    _write_json(fixture.report, report)
    _refresh_final_bindings(fixture)
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_frozen_final(fixture.request)


def test_final_allows_benign_prompt_and_response_terms(
    tmp_path: Path, final_fixture_template: FinalFixture
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    report = _strict_json(fixture.report)
    report["capabilityStatus"]["mechanicalContracts"] = (
        "prompt engineering response status"
    )
    report["genericUsage"] = {"promptTokens": 12, "responseStatus": "complete"}
    _write_json(fixture.report, report)
    _refresh_final_bindings(fixture)
    assert verifier.verify_frozen_final(fixture.request)["G2"] == "PASS"


@pytest.mark.parametrize(
    ("target", "field"),
    [
        ("report", "schemaVersion"),
        ("index", "schemaVersion"),
        ("receipt", "schemaVersion"),
        ("attempt", "attemptOrdinal"),
    ],
)
def test_final_rejects_boolean_integer_primitives(
    target: str,
    field: str,
    tmp_path: Path,
    final_fixture_template: FinalFixture,
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    path = {
        "report": fixture.report,
        "index": fixture.index,
        "receipt": fixture.receipt,
        "attempt": fixture.index,
    }[target]
    value = _strict_json(path)
    if target == "attempt":
        value["attempts"][0][field] = True
    else:
        value[field] = True
    _write_json(path, value)
    if target in {"report", "index", "attempt"}:
        _refresh_final_bindings(fixture)
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_frozen_final(fixture.request)


def test_baseline_rejects_boolean_schema_version(
    tmp_path: Path, valid_evidence_template: ValidEvidenceFixture
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    bootstrap_path = fixture.baseline / "host-bootstrap.manifest.json"
    bootstrap = _strict_json(bootstrap_path)
    bootstrap["schemaVersion"] = True
    _write_json(bootstrap_path, bootstrap)
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_evidence(fixture.request)


@pytest.mark.parametrize(
    "timestamp",
    [
        "2030-01-01T12:00:00Z",
        "2030-01-01 12:00:00.000000Z",
        "2030-01-01T12:00:00.000000z",
        "2030-01-01T12:00:00.000000+00:00",
        "2030-01-01T12:00:00.0000000Z",
    ],
)
def test_rfc3339_utc_requires_exact_canonical_grammar(timestamp: str) -> None:
    with pytest.raises(verifier.EvidenceError):
        verifier._rfc3339_utc(timestamp, "timestamp")


def test_final_requires_report_generation_before_live_verification(
    tmp_path: Path, final_fixture_template: FinalFixture
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    receipt = _strict_json(fixture.receipt)
    receipt["verifiedAt"] = "2029-12-31T23:59:59.000000Z"
    _write_json(fixture.receipt, receipt)
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_frozen_final(fixture.request)


@pytest.mark.parametrize(
    "mutation",
    [
        "multiple_selected",
        "report_candidate",
        "index_candidate",
        "receipt_candidate",
        "receipt_report_sha",
        "receipt_index_sha",
        "receipt_extra",
        "absolute_path",
        "bearer",
        "credential",
        "nan",
        "infinity",
        "duplicate_key",
    ],
)
def test_frozen_final_rejects_tampering_and_private_fields(
    mutation: str, tmp_path: Path, final_fixture_template: FinalFixture
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    if mutation == "multiple_selected":
        index = _strict_json(fixture.index)
        duplicate = dict(index["attempts"][0])
        duplicate["attemptOrdinal"] = 2
        duplicate["publicRunId"] = "second-public-run-1234567890"
        duplicate["reportPath"] = (
            "docs/evidence/business-proof/second-public-run-1234567890/report.json"
        )
        duplicate["supersedes"] = index["attempts"][0]["publicRunId"]
        duplicate["caseCommitment"] = "f" * 64
        duplicate["materialsCommitment"] = "1" * 64
        index["attempts"].append(duplicate)
        _write_json(fixture.index, index)
    elif mutation in {"report_candidate", "index_candidate", "receipt_candidate"}:
        target = {
            "report_candidate": fixture.report,
            "index_candidate": fixture.index,
            "receipt_candidate": fixture.receipt,
        }[mutation]
        value = _strict_json(target)
        if mutation == "report_candidate":
            value["forkSha"] = "0" * 40
        elif mutation == "index_candidate":
            value["attempts"][0]["candidateSha"] = "0" * 40
        else:
            value["candidateSha"] = "0" * 40
        _write_json(target, value)
    elif mutation in {"receipt_report_sha", "receipt_index_sha", "receipt_extra"}:
        value = _strict_json(fixture.receipt)
        key = {
            "receipt_report_sha": "reportSha256",
            "receipt_index_sha": "reportIndexSha256",
            "receipt_extra": "extra",
        }[mutation]
        value[key] = HEX_64 if mutation != "receipt_extra" else True
        _write_json(fixture.receipt, value)
    elif mutation in {"absolute_path", "bearer", "credential"}:
        report = _strict_json(fixture.report)
        report["genericUsage"] = {
            "safeField": {
                "absolute_path": "/private/run",
                "bearer": "Bearer abcdefgh12345678",
                "credential": "api_key=opaque-value",
            }[mutation]
        }
        _write_json(fixture.report, report)
        _refresh_final_bindings(fixture)
    elif mutation in {"nan", "infinity"}:
        payload = fixture.report.read_text(encoding="utf-8")
        constant = "NaN" if mutation == "nan" else "Infinity"
        fixture.report.write_text(
            payload.replace(
                '"genericUsage":{}', f'"genericUsage":{{"value":{constant}}}', 1
            ),
            encoding="utf-8",
        )
    elif mutation == "duplicate_key":
        payload = fixture.receipt.read_text(encoding="utf-8")
        fixture.receipt.write_text(
            payload.replace('{"G2":"PASS",', '{"G2":"PASS","G2":"PASS",', 1),
            encoding="utf-8",
        )
    else:
        raise AssertionError(mutation)
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_frozen_final(fixture.request)


@pytest.mark.parametrize(
    "forbidden_option",
    [
        "--private-run-root",
        "--commitment-key-file",
        "--attestation",
        "--ignore-failure",
    ],
)
def test_cli_does_not_define_private_or_ignore_options(
    forbidden_option: str, capsys: pytest.CaptureFixture[str]
) -> None:
    with pytest.raises(SystemExit) as error:
        verifier.main([forbidden_option, "value"])
    assert error.value.code == 2
    assert "unrecognized arguments" in capsys.readouterr().err


def test_final_cli_requires_all_five_final_arguments(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    with pytest.raises(SystemExit) as error:
        verifier.main(
            [
                "--candidate-sha",
                "0" * 40,
                "--selected-report",
                str(tmp_path / "report.json"),
            ]
        )
    assert error.value.code == 2
    assert "required together" in capsys.readouterr().err


@pytest.mark.parametrize(
    ("option", "value"),
    [
        ("--repo-root", "/repo"),
        ("--matrix", "/matrix.json"),
        ("--evidence-root", "/evidence"),
        ("--platform", "macos-x86_64"),
        ("--mode", "baseline"),
        ("--summary-output", "/summary.json"),
        ("--candidate-sha", "0" * 40),
        ("--selected-report", "/report.json"),
        ("--report-index", "/index.json"),
        ("--business-verification-receipt", "/receipt.json"),
        ("--verification-output", "/verification.json"),
    ],
)
def test_cli_rejects_repeated_option_occurrences(
    option: str, value: str, capsys: pytest.CaptureFixture[str]
) -> None:
    with pytest.raises(SystemExit) as error:
        verifier._parser().parse_args([option, value, option, value])
    assert error.value.code == 2
    assert "may not be repeated" in capsys.readouterr().err


def _baseline_cli_args() -> list[str]:
    return [
        "--repo-root",
        "/repo",
        "--matrix",
        "/matrix.json",
        "--evidence-root",
        "/evidence",
        "--platform",
        "macos-x86_64",
        "--mode",
        "baseline",
    ]


def _final_cli_args() -> list[str]:
    return [
        "--repo-root",
        "/repo",
        "--matrix",
        "/matrix.json",
        "--evidence-root",
        "/evidence",
        "--candidate-sha",
        "0" * 40,
        "--selected-report",
        "/report.json",
        "--report-index",
        "/index.json",
        "--business-verification-receipt",
        "/receipt.json",
        "--verification-output",
        "/verification.json",
    ]


@pytest.mark.parametrize("option", ["--repo-root", "--matrix", "--evidence-root"])
def test_baseline_cli_rejects_relative_public_paths(
    option: str, capsys: pytest.CaptureFixture[str]
) -> None:
    args = _baseline_cli_args()
    args[args.index(option) + 1] = "relative"
    with pytest.raises(SystemExit) as error:
        verifier.main(args)
    assert error.value.code == 2
    assert "must be absolute" in capsys.readouterr().err


@pytest.mark.parametrize(
    "option",
    [
        "--repo-root",
        "--matrix",
        "--evidence-root",
        "--selected-report",
        "--report-index",
        "--business-verification-receipt",
        "--verification-output",
    ],
)
def test_final_cli_rejects_relative_public_paths(
    option: str, capsys: pytest.CaptureFixture[str]
) -> None:
    args = _final_cli_args()
    args[args.index(option) + 1] = "relative"
    with pytest.raises(SystemExit) as error:
        verifier.main(args)
    assert error.value.code == 2
    assert "must be absolute" in capsys.readouterr().err


def test_final_cli_has_no_public_foundation_defaults(
    capsys: pytest.CaptureFixture[str],
) -> None:
    args = _final_cli_args()[6:]
    with pytest.raises(SystemExit) as error:
        verifier.main(args)
    assert error.value.code == 2
    assert "public foundation arguments" in capsys.readouterr().err


def test_final_cli_resolves_exact_sibling_paired_post_roots(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    requests: list[verifier.FinalVerificationRequest] = []
    monkeypatch.setattr(
        verifier,
        "verify_frozen_final",
        lambda request: requests.append(request) or {},
    )
    assert verifier.main(_final_cli_args()) == 0
    assert requests[0].mac_post_root == Path("/evidence/macos-x86_64/post")
    assert requests[0].windows_post_root == Path("/evidence/windows-11-x64/post")


@pytest.mark.parametrize(
    "baseline_only",
    [
        ["--platform", "macos-x86_64"],
        ["--mode", "post"],
        ["--summary-output", "/summary.json"],
    ],
)
def test_final_cli_rejects_baseline_only_arguments(
    baseline_only: list[str], capsys: pytest.CaptureFixture[str]
) -> None:
    with pytest.raises(SystemExit) as error:
        verifier.main(_final_cli_args() + baseline_only)
    assert error.value.code == 2
    assert "baseline-only" in capsys.readouterr().err


def test_baseline_cli_rejects_final_only_arguments(
    capsys: pytest.CaptureFixture[str],
) -> None:
    with pytest.raises(SystemExit) as error:
        verifier.main(_baseline_cli_args() + _final_cli_args()[6:])
    assert error.value.code == 2
    assert "final-only" in capsys.readouterr().err


def test_absolute_directory_rejects_windows_reparse_component(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    parent = tmp_path / "junction"
    child = parent / "child"
    child.mkdir(parents=True)
    lstat = Path.lstat

    def reparse_lstat(path: Path) -> object:
        metadata = lstat(path)
        if path == parent:
            return SimpleNamespace(
                st_mode=metadata.st_mode,
                st_file_attributes=0x400,
            )
        return metadata

    monkeypatch.setattr(Path, "lstat", reparse_lstat)
    with pytest.raises(verifier.EvidenceError, match="reparse"):
        verifier._absolute_directory(child, "directory")


@pytest.mark.parametrize("failure", [FileExistsError("race"), OSError("filesystem")])
def test_summary_temp_reservation_errors_are_normalized(
    failure: OSError, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    root = tmp_path / "evidence"
    root.mkdir()
    monkeypatch.setattr(
        verifier.capture,
        "_write_temp_bytes",
        lambda path, payload: (_ for _ in ()).throw(failure),
    )
    with pytest.raises(verifier.EvidenceError):
        verifier._write_summary(
            root / "baseline-summary.json", b"{}\n", root, "baseline"
        )


def test_summary_publish_oserror_is_normalized_and_temp_is_removed(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    root = tmp_path / "evidence"
    root.mkdir()
    monkeypatch.setattr(
        verifier.capture,
        "_publish_create_new",
        lambda temp, final: (_ for _ in ()).throw(OSError("race")),
    )
    with pytest.raises(verifier.EvidenceError):
        verifier._write_summary(
            root / "baseline-summary.json", b"{}\n", root, "baseline"
        )
    assert not list(root.glob("*.tmp-*"))
