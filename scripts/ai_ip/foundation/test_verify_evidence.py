import hashlib
import importlib.util
import json
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass, replace
from pathlib import Path
from types import ModuleType

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
        windows_post = tmp_path / "windows-post"
        repo.mkdir()
        tools.mkdir()
        baseline.mkdir()
        post.mkdir()
        windows_post.mkdir()
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
        "thread_id",
        "response_id",
        "reviewer_id",
        "private_root",
        "prompt_body",
        "output_body",
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
    elif mutation in {
        "absolute_path",
        "bearer",
        "credential",
        "thread_id",
        "response_id",
        "reviewer_id",
        "private_root",
        "prompt_body",
        "output_body",
    }:
        value = _strict_json(fixture.receipt)
        key, item = {
            "absolute_path": ("extra", "/private/run"),
            "bearer": ("extra", "Bearer abcdefgh12345678"),
            "credential": ("apiKey", "opaque-value"),
            "thread_id": ("threadId", "thread-123"),
            "response_id": ("responseId", "response-123"),
            "reviewer_id": ("reviewerId", "reviewer-123"),
            "private_root": ("privateRoot", "opaque"),
            "prompt_body": ("prompt", "body"),
            "output_body": ("outputBody", "body"),
        }[mutation]
        value[key] = item
        _write_json(fixture.receipt, value)
    elif mutation in {"nan", "infinity"}:
        payload = fixture.receipt.read_text(encoding="utf-8").rstrip("\n}")
        constant = "NaN" if mutation == "nan" else "Infinity"
        fixture.receipt.write_text(
            payload + f',"extra":{constant}}}\n', encoding="utf-8"
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
