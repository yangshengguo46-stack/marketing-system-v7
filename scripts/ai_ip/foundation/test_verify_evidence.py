import errno
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


def _proof_commitment_vector_fixture() -> dict[str, object]:
    path = (
        Path(__file__).resolve().parents[3]
        / "codex-rs/ai-ip-eval/tests/fixtures/contracts/06b1/proof-commitment-vectors.json"
    )
    return json.loads(path.read_bytes())


def test_proof_commitment_vectors_match_normative_framing() -> None:
    vector = _proof_commitment_vector_fixture()
    key = bytes.fromhex(str(vector["keyHex"]))
    commitments = vector["commitments"]
    assert isinstance(commitments, list)
    for commitment in commitments:
        assert isinstance(commitment, dict)
        assert (
            verifier.proof_commitment_vector(
                key,
                str(commitment["label"]),
                str(commitment["canonicalValue"]).encode("utf-8"),
            )
            == commitment["expectedHex"]
        )
    merkle = vector["merkle"]
    assert isinstance(merkle, dict)
    single = merkle["singleLeaf"]
    assert isinstance(single, dict)
    assert (
        verifier.proof_merkle_root_vector(
            {str(single["name"]): str(single["value"])}
        )
        == single["expectedHex"]
    )
    even_node = merkle["evenNode"]
    assert isinstance(even_node, dict)
    assert (
        verifier.proof_merkle_root_vector(
            {str(single["name"]): str(single["value"])}
        )
        == even_node["leftHex"]
    )
    assert verifier.proof_merkle_root_vector({"é": "b" * 64}) == even_node[
        "rightHex"
    ]
    assert (
        verifier.proof_merkle_root_vector(
            {
                "z": str(single["value"]),
                "é": "b" * 64,
            }
        )
        == even_node["expectedHex"]
    )
    odd_leaves = merkle["oddDuplicationLeaves"]
    assert isinstance(odd_leaves, dict)
    assert verifier.proof_merkle_root_vector(odd_leaves) == merkle["oddDuplicationRootHex"]
    full_leaves = merkle["fullLeaves"]
    assert isinstance(full_leaves, dict)
    assert verifier.proof_merkle_root_vector(full_leaves) == merkle["fullRootHex"]


def test_proof_commitment_vector_helpers_reject_invalid_merkle_inputs() -> None:
    with pytest.raises(verifier.EvidenceError, match="must not be empty"):
        verifier.proof_merkle_root_vector({})
    with pytest.raises(verifier.EvidenceError, match="lowercase 64-hex"):
        verifier.proof_merkle_root_vector({"leaf": "A" * 64})
    with pytest.raises(verifier.EvidenceError, match="proofRootSha256"):
        verifier.proof_merkle_root_vector({"proofRootSha256": "a" * 64})


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
    windows_baseline: Path
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
        foundation_root = tmp_path / "public-foundation"
        baseline = foundation_root / "macos-x86_64" / "baseline"
        post = foundation_root / "macos-x86_64" / "post"
        windows_baseline = foundation_root / "windows-11-x64" / "baseline"
        windows_post = foundation_root / "windows-11-x64" / "post"
        repo.mkdir()
        tools.mkdir()
        baseline.mkdir(parents=True)
        post.mkdir()
        windows_baseline.mkdir(parents=True)
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
                (windows_baseline, "windows-11-x64", "AMD64", "baseline"),
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
            windows_baseline.resolve(),
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
            windows_baseline=mapped(self.windows_baseline),
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


@pytest.mark.parametrize("layout_error", ["mode_leaf", "platform_parent"])
def test_library_rejects_non_authoritative_evidence_leaf_layout(
    layout_error: str,
    tmp_path: Path,
    valid_evidence_template: ValidEvidenceFixture,
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    if layout_error == "mode_leaf":
        relocated = fixture.baseline.parent / "relocated"
    else:
        relocated = fixture.baseline.parent.parent / "wrong-platform" / "baseline"
    shutil.copytree(fixture.baseline, relocated)
    request = replace(fixture.request, evidence_root=relocated)
    with pytest.raises(verifier.EvidenceError, match="evidence root.*layout"):
        verifier.verify_evidence(request)


def test_post_library_requires_exact_post_leaf_with_baseline_sibling(
    tmp_path: Path, valid_evidence_template: ValidEvidenceFixture
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    relocated = fixture.post.parent / "relocated"
    shutil.copytree(fixture.post, relocated)
    with pytest.raises(verifier.EvidenceError, match="evidence root.*layout"):
        verifier.verify_evidence(replace(fixture.post_request, evidence_root=relocated))


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
    matches = verifier._EvidenceSnapshot.matches

    def matches_then_swap(
        snapshot: verifier._EvidenceSnapshot,
    ) -> tuple[verifier.ForbiddenMatch, ...]:
        result = matches(snapshot)
        manifest = snapshot.root / "host-bootstrap.manifest.json"
        replacement = snapshot.root / "replacement.tmp"
        replacement.write_bytes(manifest.read_bytes())
        replacement.replace(manifest)
        return result

    monkeypatch.setattr(verifier._EvidenceSnapshot, "matches", matches_then_swap)
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_evidence(fixture.request)


def test_recursive_scanner_rejects_identical_entry_swap_after_child_scan(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    root = tmp_path / "evidence"
    root.mkdir()
    path = root / "safe.log"
    path.write_bytes(b"safe\n")
    capture_file = verifier._FileSnapshot.capture.__func__

    def capture_then_swap(
        cls: type[verifier._FileSnapshot],
        target: Path,
        display: str,
        **options: object,
    ) -> verifier._FileSnapshot:
        snapshot = capture_file(cls, target, display, **options)
        replacement = target.with_name("replacement.tmp")
        replacement.write_bytes(target.read_bytes())
        replacement.replace(target)
        return snapshot

    monkeypatch.setattr(
        verifier._FileSnapshot, "capture", classmethod(capture_then_swap)
    )
    with pytest.raises(verifier.EvidenceError):
        verifier.scan_forbidden_evidence(root)


def test_recursive_scanner_rejects_empty_root_directory_swap(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    root = tmp_path / "evidence"
    root.mkdir()
    detached = tmp_path / "detached"
    scandir = os.scandir
    swapped = False

    def scandir_then_swap(path: object) -> object:
        nonlocal swapped
        entries = list(scandir(path))
        if Path(path) == root and not swapped:
            swapped = True
            root.rename(detached)
            root.mkdir()
        return iter(entries)

    monkeypatch.setattr(os, "scandir", scandir_then_swap)
    with pytest.raises(verifier.EvidenceError):
        verifier.scan_forbidden_evidence(root)


def test_file_snapshot_scans_hashes_and_retains_the_same_bytes(tmp_path: Path) -> None:
    path = tmp_path / "public.json"
    payload = b'{"safe":"Bearer abcdefgh12345678"}\n'
    path.write_bytes(payload)
    snapshot = verifier._FileSnapshot.capture(path, "public.json", keep_payload=True)
    assert snapshot.payload == payload
    assert snapshot.sha256 == hashlib.sha256(payload).hexdigest()
    assert snapshot.matches == (verifier.ForbiddenMatch("public.json", "bearer-token"),)


@pytest.mark.parametrize("keep_payload", [True, False], ids=["json", "log"])
@pytest.mark.parametrize("phase", ["capture", "final-rehash"])
def test_file_snapshot_rejects_continual_growth_with_size_plus_one_reads(
    keep_payload: bool,
    phase: str,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    path = tmp_path / ("public.json" if keep_payload else "public.log")
    path.write_bytes(b"abc")
    snapshot = None
    if phase == "final-rehash":
        snapshot = verifier._FileSnapshot.capture(
            path, path.name, keep_payload=keep_payload
        )
    real_read = os.read
    requested: list[int] = []

    def continually_growing_read(descriptor: int, byte_count: int) -> bytes:
        if len(requested) == 3:
            return b""
        requested.append(byte_count)
        with path.open("ab", buffering=0) as output:
            output.write(b"x")
        return real_read(descriptor, byte_count)

    monkeypatch.setattr(verifier.os, "read", continually_growing_read)
    with pytest.raises(verifier.EvidenceError):
        if snapshot is None:
            verifier._FileSnapshot.capture(path, path.name, keep_payload=keep_payload)
        else:
            snapshot.validate_final_state()
    assert requested == [3, 1]


def test_evidence_snapshot_uses_descriptor_relative_nofollow_capture(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    root = tmp_path / "baseline"
    root.mkdir()
    (root / "safe.json").write_bytes(b"{}\n")
    open_regular = verifier._open_stable_regular
    observed: list[tuple[int | None, str | None]] = []

    def observe_open(path: Path, **options: object) -> tuple[int, os.stat_result]:
        observed.append((options.get("dir_fd"), options.get("name")))
        return open_regular(path, **options)

    monkeypatch.setattr(verifier, "_open_stable_regular", observe_open)
    snapshot = verifier._EvidenceSnapshot(root)
    try:
        snapshot.require_exact({"safe.json"}, set())
        snapshot.validate_final_state()
    finally:
        snapshot.close()
    assert any(dir_fd is not None and name == "safe.json" for dir_fd, name in observed)


def test_evidence_snapshot_windows_fallback_rehashes_same_inode_rewrite(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    root = tmp_path / "baseline"
    root.mkdir()
    path = root / "safe.json"
    path.write_bytes(b"{}\n")
    monkeypatch.setattr(
        verifier, "_descriptor_relative_traversal_available", lambda: False
    )
    snapshot = verifier._EvidenceSnapshot(root)
    try:
        snapshot.require_exact({"safe.json"}, set())
        assert snapshot.root_fd is None
        _rewrite_same_inode_same_size_and_restore_mtime(path)
        with pytest.raises(verifier.EvidenceError):
            snapshot.validate_final_state()
    finally:
        snapshot.close()


def _rewrite_same_inode_same_size_and_restore_mtime(path: Path) -> None:
    before = path.stat(follow_symlinks=False)
    payload = bytearray(path.read_bytes())
    assert payload
    payload[0] ^= 1
    with path.open("r+b", buffering=0) as output:
        output.write(payload)
        os.fsync(output.fileno())
    os.utime(path, ns=(before.st_atime_ns, before.st_mtime_ns))
    after = path.stat(follow_symlinks=False)
    assert (before.st_ino, before.st_size, before.st_mtime_ns) == (
        after.st_ino,
        after.st_size,
        after.st_mtime_ns,
    )


def test_verify_rehashes_manifest_after_same_inode_rewrite(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    valid_evidence_template: ValidEvidenceFixture,
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    matches = verifier._EvidenceSnapshot.matches

    def matches_then_rewrite(
        snapshot: verifier._EvidenceSnapshot,
    ) -> tuple[verifier.ForbiddenMatch, ...]:
        result = matches(snapshot)
        _rewrite_same_inode_same_size_and_restore_mtime(
            snapshot.root / "host-bootstrap.manifest.json"
        )
        return result

    monkeypatch.setattr(verifier._EvidenceSnapshot, "matches", matches_then_rewrite)
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_evidence(fixture.request)


def test_verify_rehashes_log_after_same_inode_rewrite(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    valid_evidence_template: ValidEvidenceFixture,
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    validate_command = verifier._validate_command

    def validate_then_rewrite(*args: object, **kwargs: object) -> bool:
        result = validate_command(*args, **kwargs)
        command = args[2]
        if command.id == "base-pass":
            _rewrite_same_inode_same_size_and_restore_mtime(
                fixture.baseline / "base-pass.stdout.log"
            )
        return result

    monkeypatch.setattr(verifier, "_validate_command", validate_then_rewrite)
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_evidence(fixture.request)


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


def test_verify_rehashes_matrix_after_same_inode_rewrite(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    valid_evidence_template: ValidEvidenceFixture,
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    matrix_from_value = verifier._matrix_from_value

    def parse_then_rewrite(parsed: object) -> object:
        matrix = matrix_from_value(parsed)
        _rewrite_same_inode_same_size_and_restore_mtime(fixture.matrix)
        return matrix

    monkeypatch.setattr(verifier, "_matrix_from_value", parse_then_rewrite)
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_evidence(fixture.request)


def test_unexpected_json_is_rejected_before_payload_retention(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    root = tmp_path / "baseline"
    root.mkdir()
    (root / "expected.json").write_bytes(b"{}\n")
    (root / "unexpected.json").write_bytes(b"12345")
    snapshot = verifier._EvidenceSnapshot(root)
    capture_file = verifier._FileSnapshot.capture.__func__
    captured: list[str] = []

    def observe_capture(
        cls: type[verifier._FileSnapshot],
        path: Path,
        display: str,
        **options: object,
    ) -> verifier._FileSnapshot:
        captured.append(display)
        return capture_file(cls, path, display, **options)

    monkeypatch.setattr(verifier._FileSnapshot, "capture", classmethod(observe_capture))
    try:
        with pytest.raises(verifier.EvidenceError, match="completeness"):
            snapshot.require_exact({"expected.json"}, set())
        assert captured == []
        assert snapshot.files == {}
    finally:
        snapshot.close()


def test_evidence_snapshot_rejects_per_json_limit(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    root = tmp_path / "baseline"
    root.mkdir()
    (root / "expected.json").write_bytes(b"{}\n")
    snapshot = verifier._EvidenceSnapshot(root)
    monkeypatch.setattr(verifier, "MAX_PUBLIC_JSON_BYTES", 2, raising=False)
    monkeypatch.setattr(verifier, "MAX_TOTAL_PUBLIC_JSON_BYTES", 100, raising=False)
    try:
        with pytest.raises(verifier.EvidenceError, match="JSON.*limit"):
            snapshot.require_exact({"expected.json"}, set())
        assert snapshot.files == {}
    finally:
        snapshot.close()


def test_evidence_snapshot_rejects_total_json_limit(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    root = tmp_path / "baseline"
    root.mkdir()
    (root / "first.json").write_bytes(b"{}\n")
    (root / "second.json").write_bytes(b"[]\n")
    snapshot = verifier._EvidenceSnapshot(root)
    monkeypatch.setattr(verifier, "MAX_PUBLIC_JSON_BYTES", 4, raising=False)
    monkeypatch.setattr(verifier, "MAX_TOTAL_PUBLIC_JSON_BYTES", 5, raising=False)
    try:
        with pytest.raises(verifier.EvidenceError, match="total public JSON.*limit"):
            snapshot.require_exact({"first.json", "second.json"}, set())
        assert snapshot.files == {}
    finally:
        snapshot.close()


def test_matrix_snapshot_rejects_public_json_limit(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    valid_evidence_template: ValidEvidenceFixture,
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    monkeypatch.setattr(verifier, "MAX_PUBLIC_JSON_BYTES", 1, raising=False)
    bootstrap = _strict_json(fixture.baseline / "host-bootstrap.manifest.json")
    with pytest.raises(verifier.EvidenceError, match="JSON.*limit"):
        verifier._bindings(
            fixture.repo,
            fixture.matrix,
            bootstrap["testedGitSha"],
            bootstrap["toolsGitSha"],
        )


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
        windows_baseline = verifier.verify_evidence(
            replace(
                evidence.request,
                evidence_root=evidence.windows_baseline,
                platform="windows-11-x64",
            )
        )
        _write_json(
            evidence.windows_baseline / "baseline-summary.json",
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
            windows_baseline=mapped(original.windows_baseline),
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


def test_frozen_final_rehashes_report_after_same_inode_rewrite(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    final_fixture_template: FinalFixture,
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    validate_report = verifier._validate_report

    def validate_then_rewrite(
        path: Path, repo: Path, candidate_sha: str, payload: bytes
    ) -> dict[str, object]:
        report = validate_report(path, repo, candidate_sha, payload)
        _rewrite_same_inode_same_size_and_restore_mtime(path)
        return report

    monkeypatch.setattr(verifier, "_validate_report", validate_then_rewrite)
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_frozen_final(fixture.request)
    assert not fixture.output.exists()


def test_frozen_final_publication_is_last_irreversible_action(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    final_fixture_template: FinalFixture,
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    publish = verifier._publish_create_new

    def publish_then_mutate(path: Path, payload: bytes, repo: Path) -> None:
        publish(path, payload, repo)
        replacement = fixture.report.with_name("post-publication-replacement.tmp")
        replacement.write_bytes(fixture.report.read_bytes())
        replacement.replace(fixture.report)

    monkeypatch.setattr(verifier, "_publish_create_new", publish_then_mutate)
    try:
        result = verifier.verify_frozen_final(fixture.request)
    except verifier.EvidenceError:
        assert not fixture.output.exists()
    else:
        assert result["foundationDecision"] == "PASS_TO_PHASE_0B"
        assert fixture.output.exists()


def test_frozen_final_adapter_failure_leaves_no_output_or_temp(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    final_fixture_template: FinalFixture,
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    monkeypatch.setattr(
        verifier.capture,
        "_publish_create_new",
        lambda temp, final: (_ for _ in ()).throw(OSError("publish failed")),
    )
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_frozen_final(fixture.request)
    assert not fixture.output.exists()
    assert not list(fixture.output.parent.glob("*.tmp-*"))


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
    "private_key",
    [
        "apiKey",
        "api_key",
        "api-key",
        "accessToken",
        "access_token",
        "access-token",
        "secretKey",
        "secret_key",
        "secret-key",
        "secret",
    ],
)
def test_final_semantic_filter_rejects_nested_credential_key(
    private_key: str,
) -> None:
    with pytest.raises(verifier.EvidenceError):
        verifier._safe_public_value(
            {"allowedEnvelope": {private_key: "opaque"}}, "public value"
        )


@pytest.mark.parametrize(
    "absolute_path",
    [r"\\server\share", r"\\?\C:\private", r"\\.\pipe\private"],
)
def test_final_semantic_filter_rejects_unc_and_device_paths(
    absolute_path: str,
) -> None:
    with pytest.raises(verifier.EvidenceError):
        verifier._safe_public_value(
            {"allowedEnvelope": {"safeField": absolute_path}}, "public value"
        )


@pytest.mark.parametrize(
    "rooted_path",
    [r"\Users\alice\private", r"\??\C:\private"],
)
def test_final_semantic_filter_rejects_single_backslash_rooted_paths(
    rooted_path: str,
) -> None:
    with pytest.raises(verifier.EvidenceError):
        verifier._safe_public_value(
            {"allowedEnvelope": {"safeField": rooted_path}}, "public value"
        )


def test_final_semantic_filter_allows_internal_backslashes() -> None:
    verifier._safe_public_value(
        {"allowedEnvelope": {"safeField": r"ordinary\text"}}, "public value"
    )


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


def test_final_rejects_boolean_selected_attempt_ordinal(
    tmp_path: Path, final_fixture_template: FinalFixture
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    receipt = _strict_json(fixture.receipt)
    receipt["selectedAttemptOrdinal"] = True
    _write_json(fixture.receipt, receipt)
    with pytest.raises(verifier.EvidenceError):
        verifier.verify_frozen_final(fixture.request)


def test_final_input_rejects_public_json_limit(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    final_fixture_template: FinalFixture,
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    mac = verifier.verify_evidence(
        replace(fixture.evidence.post_request, evidence_root=fixture.evidence.post)
    )
    windows = verifier.verify_evidence(
        replace(
            fixture.evidence.post_request,
            evidence_root=fixture.evidence.windows_post,
            platform="windows-11-x64",
        )
    )
    monkeypatch.setattr(
        verifier,
        "verify_evidence",
        lambda request: mac if request.platform == "macos-x86_64" else windows,
    )
    monkeypatch.setattr(verifier, "MAX_PUBLIC_JSON_BYTES", 1, raising=False)
    with pytest.raises(verifier.EvidenceError, match="JSON.*limit"):
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


def test_baseline_rejects_boolean_manifest_expected_exit(
    tmp_path: Path, valid_evidence_template: ValidEvidenceFixture
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    manifest_path = fixture.baseline / "base-pass.manifest.json"
    manifest = _strict_json(manifest_path)
    manifest["expectedExit"] = False
    _write_json(manifest_path, manifest)
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
    expected_error: str | None = None
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
        _refresh_final_bindings(fixture)
        expected_error = "select exactly one report"
    elif mutation in {"report_candidate", "index_candidate", "receipt_candidate"}:
        target = {
            "report_candidate": fixture.report,
            "index_candidate": fixture.index,
            "receipt_candidate": fixture.receipt,
        }[mutation]
        value = _strict_json(target)
        if mutation == "report_candidate":
            value["forkSha"] = "0" * 40
            expected_error = "selected report candidate binding differs"
        elif mutation == "index_candidate":
            value["attempts"][0]["candidateSha"] = "0" * 40
            expected_error = "selected report/index binding differs"
        else:
            value["candidateSha"] = "0" * 40
            expected_error = "business receipt candidateSha binding differs"
        _write_json(target, value)
        if mutation in {"report_candidate", "index_candidate"}:
            _refresh_final_bindings(fixture)
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
    with pytest.raises(verifier.EvidenceError, match=expected_error):
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


def _assert_forbidden_cli_result(
    result: int, capsys: pytest.CaptureFixture[str]
) -> None:
    output = capsys.readouterr()
    assert result == 1
    assert json.loads(output.out)["status"] == "INVALID_FORBIDDEN_CONTENT"
    assert output.err == ""


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


def _replace_parent_with_symlink_to_original(parent: Path) -> Path:
    detached = parent.with_name(f"{parent.name}-detached")
    parent.rename(detached)
    parent.symlink_to(detached, target_is_directory=True)
    return detached


def test_baseline_cli_rejects_parent_substitution_between_inspection_and_canonicalization(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
    valid_evidence_template: ValidEvidenceFixture,
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    unsafe_path_component = verifier._unsafe_path_component

    def inspect_then_swap(path: Path) -> str | None:
        result = unsafe_path_component(path)
        if path == fixture.baseline:
            _replace_parent_with_symlink_to_original(path.parent)
        return result

    monkeypatch.setattr(verifier, "_unsafe_path_component", inspect_then_swap)
    _assert_forbidden_cli_result(
        verifier.main(
            [
                "--repo-root",
                str(fixture.repo),
                "--matrix",
                str(fixture.matrix),
                "--evidence-root",
                str(fixture.baseline),
                "--platform",
                "macos-x86_64",
                "--mode",
                "baseline",
            ]
        ),
        capsys,
    )


def test_frozen_final_rejects_dotted_output_parent_inside_repo(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
    final_fixture_template: FinalFixture,
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    _stub_final_native_evidence(monkeypatch, fixture)
    sibling = fixture.evidence.repo.parent / "containment-sibling"
    sibling.mkdir()
    inside_output = fixture.evidence.repo / "verification.next.json"
    dotted_output = sibling / ".." / fixture.evidence.repo.name / inside_output.name
    assert dotted_output != inside_output
    assert dotted_output.parent != fixture.evidence.repo
    assert fixture.evidence.repo not in dotted_output.parent.parents
    args = _final_fixture_cli_args(fixture)
    args[args.index("--verification-output") + 1] = str(dotted_output)
    assert verifier.main(args) == 1
    output = capsys.readouterr()
    assert output.out == ""
    assert "must not contain '..'" in output.err
    assert not inside_output.exists()


def test_open_stable_directory_anchor_fstat_failure_closes_owned_descriptor(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    anchor = Path(Path.cwd().anchor)
    safe_fstat = verifier._safe_fstat
    real_open = os.open
    real_fstat = os.fstat
    descriptors: list[int] = []
    fstat_calls = 0

    def record_anchor_open(
        path_value: object, flags: int, *args: object, **kwargs: object
    ) -> int:
        descriptor = real_open(path_value, flags, *args, **kwargs)
        if Path(path_value) == anchor:
            descriptors.append(descriptor)
        return descriptor

    def fail_second_fstat(descriptor: int, label: str) -> os.stat_result:
        nonlocal fstat_calls
        fstat_calls += 1
        if fstat_calls == 2:
            raise verifier.UnsafeEvidenceError("injected anchor fstat failure")
        return safe_fstat(descriptor, label)

    monkeypatch.setattr(
        verifier, "_descriptor_relative_traversal_available", lambda: True
    )
    monkeypatch.setattr(verifier.os, "open", record_anchor_open)
    monkeypatch.setattr(verifier, "_safe_fstat", fail_second_fstat)
    try:
        with pytest.raises(verifier.UnsafeEvidenceError):
            verifier._open_stable_directory(anchor)
        assert len(descriptors) == 1
        with pytest.raises(OSError) as error:
            real_fstat(descriptors[0])
        assert error.value.errno == errno.EBADF
    finally:
        for descriptor in descriptors:
            try:
                real_fstat(descriptor)
            except OSError:
                continue
            os.close(descriptor)


def test_fallback_component_failure_after_leaf_open_closes_descriptor(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    path = tmp_path / "public.json"
    path.write_bytes(b"{}\n")
    fallback_component_states = verifier._fallback_component_states
    real_open = os.open
    real_fstat = os.fstat
    leaf_descriptors: list[int] = []
    component_calls = 0

    def record_leaf_open(
        path_value: object, flags: int, *args: object, **kwargs: object
    ) -> int:
        descriptor = real_open(path_value, flags, *args, **kwargs)
        if Path(path_value) == path:
            leaf_descriptors.append(descriptor)
        return descriptor

    def fail_post_open_component_check(
        checked_path: Path, label: str
    ) -> tuple[os.stat_result, ...]:
        nonlocal component_calls
        component_calls += 1
        if component_calls == 2:
            raise verifier.UnsafeEvidenceError("injected fallback component failure")
        return fallback_component_states(checked_path, label)

    monkeypatch.setattr(
        verifier, "_descriptor_relative_traversal_available", lambda: False
    )
    monkeypatch.setattr(verifier.os, "open", record_leaf_open)
    monkeypatch.setattr(
        verifier, "_fallback_component_states", fail_post_open_component_check
    )
    try:
        with pytest.raises(verifier.UnsafeEvidenceError):
            verifier._open_stable_regular(path)
        assert len(leaf_descriptors) == 1
        with pytest.raises(OSError) as error:
            real_fstat(leaf_descriptors[0])
        assert error.value.errno == errno.EBADF
    finally:
        for descriptor in leaf_descriptors:
            try:
                real_fstat(descriptor)
            except OSError:
                continue
            os.close(descriptor)


def test_windows_fallback_without_nofollow_rejects_post_open_leaf_reparse(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    path = tmp_path / "public.json"
    path.write_bytes(b"{}\n")
    entry_state = verifier._entry_state
    real_open = os.open
    descriptor: int | None = None
    reparse_armed = False

    def open_then_arm_reparse(
        path_value: object, flags: int, *args: object, **kwargs: object
    ) -> int:
        nonlocal reparse_armed
        opened = real_open(path_value, flags, *args, **kwargs)
        if Path(path_value) == path:
            reparse_armed = True
        return opened

    def post_open_reparse_state(
        checked_path: Path, dir_fd: int | None, name: str | None
    ) -> object:
        value = entry_state(checked_path, dir_fd, name)
        if reparse_armed and checked_path == path:
            return SimpleNamespace(
                st_dev=value.st_dev,
                st_ino=value.st_ino,
                st_mode=value.st_mode,
                st_nlink=value.st_nlink,
                st_size=value.st_size,
                st_mtime_ns=value.st_mtime_ns,
                st_file_attributes=0x400,
                st_reparse_tag=1,
            )
        return value

    monkeypatch.setattr(
        verifier, "_descriptor_relative_traversal_available", lambda: False
    )
    monkeypatch.setattr(verifier.os, "O_NOFOLLOW", 0)
    monkeypatch.setattr(verifier.os, "open", open_then_arm_reparse)
    monkeypatch.setattr(verifier, "_entry_state", post_open_reparse_state)
    try:
        with pytest.raises(verifier.UnsafeEvidenceError):
            descriptor, _ = verifier._open_stable_regular(path)
    finally:
        if descriptor is not None:
            os.close(descriptor)


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
                "--repo-root",
                str(fixture.repo),
                "--matrix",
                str(fixture.matrix),
                "--evidence-root",
                str(fixture.baseline),
                "--platform",
                "macos-x86_64",
                "--mode",
                "baseline",
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


def test_baseline_cli_rejects_matrix_parent_symlink_to_original(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
    valid_evidence_template: ValidEvidenceFixture,
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    matrix_from_value = verifier._matrix_from_value

    def parse_then_swap(parsed: object) -> verifier.capture.Matrix:
        matrix = matrix_from_value(parsed)
        _replace_parent_with_symlink_to_original(fixture.matrix.parent)
        return matrix

    monkeypatch.setattr(verifier, "_matrix_from_value", parse_then_swap)
    _assert_forbidden_cli_result(
        verifier.main(
            [
                "--repo-root",
                str(fixture.repo),
                "--matrix",
                str(fixture.matrix),
                "--evidence-root",
                str(fixture.baseline),
                "--platform",
                "macos-x86_64",
                "--mode",
                "baseline",
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
                "--repo-root",
                str(fixture.repo),
                "--matrix",
                str(fixture.matrix),
                "--evidence-root",
                str(fixture.baseline),
                "--platform",
                "macos-x86_64",
                "--mode",
                "baseline",
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


def test_file_snapshot_rejects_reparse_after_read(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    path = tmp_path / "public.json"
    path.write_bytes(b"{}\n")
    snapshot = verifier._FileSnapshot.capture(path, path.name, keep_payload=True)
    real_read = os.read
    real_fstat = os.fstat
    read_descriptors: set[int] = set()

    def track_read(descriptor: int, byte_count: int) -> bytes:
        payload = real_read(descriptor, byte_count)
        read_descriptors.add(descriptor)
        return payload

    def add_reparse_after_read(descriptor: int) -> object:
        value = real_fstat(descriptor)
        if descriptor not in read_descriptors:
            return value
        return SimpleNamespace(
            st_dev=value.st_dev,
            st_ino=value.st_ino,
            st_mode=value.st_mode,
            st_nlink=value.st_nlink,
            st_size=value.st_size,
            st_mtime_ns=value.st_mtime_ns,
            st_file_attributes=0x400,
            st_reparse_tag=1,
        )

    monkeypatch.setattr(verifier.os, "read", track_read)
    monkeypatch.setattr(verifier.os, "fstat", add_reparse_after_read)
    with pytest.raises(verifier.UnsafeEvidenceError):
        snapshot.validate_final_state()


@pytest.mark.parametrize("inject_failure", [False, True], ids=["pass", "failure"])
def test_file_snapshot_descriptor_lifecycle_closes_owned_descriptors(
    inject_failure: bool,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    parent = tmp_path / "parent"
    parent.mkdir()
    path = parent / "public.json"
    path.write_bytes(b"{}\n")
    real_open = os.open
    real_stat = os.stat
    real_fstat = os.fstat
    descriptors: list[int] = []
    recording = False

    def record_open(
        path_value: object, flags: int, *args: object, **kwargs: object
    ) -> int:
        descriptor = real_open(path_value, flags, *args, **kwargs)
        if recording:
            descriptors.append(descriptor)
        return descriptor

    def fail_parent_component(
        path_value: object, *args: object, **kwargs: object
    ) -> os.stat_result:
        if (
            inject_failure
            and path_value == "parent"
            and kwargs.get("dir_fd") is not None
            and kwargs.get("follow_symlinks") is False
        ):
            raise OSError("injected parent component failure")
        return real_stat(path_value, *args, **kwargs)

    monkeypatch.setattr(verifier.os, "open", record_open)
    monkeypatch.setattr(verifier.os, "stat", fail_parent_component)
    monkeypatch.setattr(
        verifier, "_descriptor_relative_traversal_available", lambda: True
    )
    recording = True
    try:
        if inject_failure:
            with pytest.raises(verifier.UnsafeEvidenceError):
                verifier._FileSnapshot.capture(path, path.name, keep_payload=True)
        else:
            snapshot = verifier._FileSnapshot.capture(
                path, path.name, keep_payload=True
            )
            snapshot.validate_final_state()
    finally:
        recording = False
    assert descriptors
    for descriptor in descriptors:
        with pytest.raises(OSError) as error:
            real_fstat(descriptor)
        assert error.value.errno == errno.EBADF


def _final_fixture_cli_args(fixture: FinalFixture) -> list[str]:
    foundation = fixture.evidence.baseline.parent.parent
    return [
        "--repo-root",
        str(fixture.evidence.repo),
        "--matrix",
        str(fixture.evidence.matrix),
        "--evidence-root",
        str(foundation),
        "--candidate-sha",
        fixture.evidence.tested_sha,
        "--selected-report",
        str(fixture.report),
        "--report-index",
        str(fixture.index),
        "--business-verification-receipt",
        str(fixture.receipt),
        "--verification-output",
        str(fixture.output),
    ]


def _stub_final_native_evidence(
    monkeypatch: pytest.MonkeyPatch, fixture: FinalFixture
) -> None:
    matrix_sha = hashlib.sha256(
        verifier.canonical_json_bytes(_strict_json(fixture.evidence.matrix))
    ).hexdigest()

    def disposition(
        request: verifier.VerificationRequest,
    ) -> verifier.EvidenceDisposition:
        return verifier.EvidenceDisposition(
            platform=request.platform,
            mode="post",
            tested_git_sha=fixture.evidence.tested_sha,
            tools_git_sha=fixture.evidence.tools_sha,
            matrix_sha256=matrix_sha,
            command_ids=("base-pass", "base-paired", "post-extra"),
            blocked_ids=(),
        )

    monkeypatch.setattr(verifier, "verify_evidence", disposition)


def test_baseline_cli_reports_symlink_evidence_root_as_forbidden_only(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    valid_evidence_template: ValidEvidenceFixture,
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    unsafe_parent = tmp_path / "unsafe" / "macos-x86_64"
    unsafe_parent.mkdir(parents=True)
    unsafe_root = unsafe_parent / "baseline"
    unsafe_root.symlink_to(fixture.baseline, target_is_directory=True)
    _assert_forbidden_cli_result(
        verifier.main(
            [
                "--repo-root",
                str(fixture.repo),
                "--matrix",
                str(fixture.matrix),
                "--evidence-root",
                str(unsafe_root),
                "--platform",
                "macos-x86_64",
                "--mode",
                "baseline",
            ]
        ),
        capsys,
    )


def test_baseline_cli_reports_symlink_matrix_as_forbidden_only(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    valid_evidence_template: ValidEvidenceFixture,
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    real_matrix = fixture.matrix.with_name("matrix-real.json")
    fixture.matrix.rename(real_matrix)
    fixture.matrix.symlink_to(real_matrix)
    _assert_forbidden_cli_result(
        verifier.main(
            [
                "--repo-root",
                str(fixture.repo),
                "--matrix",
                str(fixture.matrix),
                "--evidence-root",
                str(fixture.baseline),
                "--platform",
                "macos-x86_64",
                "--mode",
                "baseline",
            ]
        ),
        capsys,
    )


def test_baseline_cli_reports_hardlinked_matrix_as_forbidden_only(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    valid_evidence_template: ValidEvidenceFixture,
) -> None:
    fixture = valid_evidence_template.clone(tmp_path)
    real_matrix = fixture.matrix.with_name("matrix-real.json")
    fixture.matrix.rename(real_matrix)
    os.link(real_matrix, fixture.matrix)
    _assert_forbidden_cli_result(
        verifier.main(
            [
                "--repo-root",
                str(fixture.repo),
                "--matrix",
                str(fixture.matrix),
                "--evidence-root",
                str(fixture.baseline),
                "--platform",
                "macos-x86_64",
                "--mode",
                "baseline",
            ]
        ),
        capsys,
    )


@pytest.mark.parametrize("target_name", ["report", "index", "receipt"])
def test_final_cli_reports_symlink_consumed_input_as_forbidden_only(
    target_name: str,
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    final_fixture_template: FinalFixture,
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    target = {
        "report": fixture.report,
        "index": fixture.index,
        "receipt": fixture.receipt,
    }[target_name]
    real_target = target.with_name(f"{target.stem}-real.json")
    target.rename(real_target)
    target.symlink_to(real_target)
    foundation = fixture.evidence.baseline.parent.parent
    _assert_forbidden_cli_result(
        verifier.main(
            [
                "--repo-root",
                str(fixture.evidence.repo),
                "--matrix",
                str(fixture.evidence.matrix),
                "--evidence-root",
                str(foundation),
                "--candidate-sha",
                fixture.evidence.tested_sha,
                "--selected-report",
                str(fixture.report),
                "--report-index",
                str(fixture.index),
                "--business-verification-receipt",
                str(fixture.receipt),
                "--verification-output",
                str(fixture.output),
            ]
        ),
        capsys,
    )


@pytest.mark.parametrize("target_name", ["report", "index", "receipt"])
def test_final_cli_reports_hardlinked_consumed_input_as_forbidden_only(
    target_name: str,
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
    final_fixture_template: FinalFixture,
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    _stub_final_native_evidence(monkeypatch, fixture)
    target = {
        "report": fixture.report,
        "index": fixture.index,
        "receipt": fixture.receipt,
    }[target_name]
    real_target = target.with_name(f"{target.stem}-real.json")
    target.rename(real_target)
    os.link(real_target, target)
    _assert_forbidden_cli_result(
        verifier.main(_final_fixture_cli_args(fixture)), capsys
    )


@pytest.mark.parametrize("race_kind", ["symlink", "reparse"])
def test_final_cli_reports_post_check_path_race_as_forbidden_only(
    race_kind: str,
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
    final_fixture_template: FinalFixture,
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    _stub_final_native_evidence(monkeypatch, fixture)
    validate_receipt = verifier._validate_receipt
    entry_state = verifier._entry_state
    reparse_armed = False

    def validate_then_race(*args: object, **kwargs: object) -> dict[str, object]:
        nonlocal reparse_armed
        receipt = validate_receipt(*args, **kwargs)
        if race_kind == "symlink":
            real_report = fixture.report.with_name("report-real.json")
            fixture.report.rename(real_report)
            fixture.report.symlink_to(real_report)
        else:
            reparse_armed = True
        return receipt

    def reparse_entry_state(path: Path, dir_fd: int | None, name: str | None) -> object:
        value = entry_state(path, dir_fd, name)
        if reparse_armed and path == fixture.report:
            return SimpleNamespace(
                st_dev=value.st_dev,
                st_ino=value.st_ino,
                st_mode=value.st_mode,
                st_nlink=value.st_nlink,
                st_size=value.st_size,
                st_mtime_ns=value.st_mtime_ns,
                st_file_attributes=0x400,
            )
        return value

    monkeypatch.setattr(verifier, "_validate_receipt", validate_then_race)
    monkeypatch.setattr(verifier, "_entry_state", reparse_entry_state)
    _assert_forbidden_cli_result(
        verifier.main(_final_fixture_cli_args(fixture)), capsys
    )


def test_final_cli_keeps_ordinary_json_schema_error_out_of_forbidden_channel(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
    final_fixture_template: FinalFixture,
) -> None:
    fixture = final_fixture_template.clone(tmp_path)
    _stub_final_native_evidence(monkeypatch, fixture)
    report = _strict_json(fixture.report)
    report["unexpected"] = "public"
    _write_json(fixture.report, report)
    assert verifier.main(_final_fixture_cli_args(fixture)) == 1
    output = capsys.readouterr()
    assert output.out == ""
    assert "keys differ" in output.err


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
