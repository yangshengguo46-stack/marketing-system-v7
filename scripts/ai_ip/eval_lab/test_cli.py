import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import cli
from cli import main
from contracts import canonical_json_bytes, load_exact_json


REPO_ROOT = Path(__file__).resolve().parents[3]
LAB_ROOT = REPO_ROOT / "ai-ip-evals" / "lab"
POLICY_PATH = LAB_ROOT / "rubrics" / "fixture-qualification-policy.json"


def _write_json(path: Path, value: object) -> None:
    path.write_bytes(canonical_json_bytes(value) + b"\n")


def _profile(reviewer_id: str = "reviewer-business-1") -> dict[str, object]:
    return {
        "schemaVersion": 1,
        "objectKind": "ReviewerProfile",
        "reviewerId": reviewer_id,
        "capabilityDomains": ["businessIpJudgment", "evidenceIntegrity"],
        "conflictDisclosure": "accepted",
        "requestedQualifications": [
            "businessIpJudgment",
            "evidenceIntegrity",
        ],
    }


def _attempt(reviewer_id: str = "reviewer-business-1") -> dict[str, object]:
    return {
        "schemaVersion": 1,
        "objectKind": "CalibrationAttempt",
        "reviewerId": reviewer_id,
        "policyId": "fixture-reviewer-qualification-v1",
        "anchorCorrect": 4,
        "severeMisses": 0,
        "repeatAgreement": 1000,
        "swapAgreement": 1000,
        "completedAt": "2026-08-31T00:00:00Z",
    }


def _prepare_arguments() -> list[str]:
    return [
        "prepare-blind-batch",
        "--private-root",
        "private",
        "--case-receipt",
        "case.json",
        "--stock-answer",
        "stock.json",
        "--modified-answer",
        "modified.json",
        "--rubric",
        "rubric.json",
        "--batch-id",
        "batch-1",
        "--analysis-frozen-at",
        "2026-09-01T00:00:00Z",
    ]


def _seal_arguments() -> list[str]:
    return [
        "seal-blind-statistics",
        "--private-root",
        "private",
        "--blind-pack-receipt",
        "pack.json",
        "--base-qualification-receipt",
        "base-1.json",
        "--base-qualification-receipt",
        "base-2.json",
        "--sealed-at",
        "2026-09-01T00:00:00Z",
    ]


def _complete_arguments() -> list[list[str]]:
    prepare = _prepare_arguments() + [
        "--base-qualification-receipt",
        "base-1.json",
        "--base-qualification-receipt",
        "base-2.json",
    ]
    seal = _seal_arguments() + [
        item
        for index in range(4)
        for item in ("--submission", f"submission-{index}.json")
    ]
    return [
        [
            "compile-golden-gift",
            "--source-root",
            "source",
            "--source-manifest",
            "manifest.json",
            "--blueprint",
            "blueprint.json",
            "--private-root",
            "private",
            "--compiled-at",
            "2026-09-01T00:00:00Z",
        ],
        [
            "qualify-reviewer",
            "--profile",
            "profile.json",
            "--attempt",
            "attempt.json",
            "--policy",
            "policy.json",
            "--evaluated-at",
            "2026-09-01T00:00:00Z",
            "--output-root",
            "qualification",
        ],
        prepare,
        seal,
        [
            "unlock-fixture-pilot",
            "--private-root",
            "private",
            "--blind-statistics",
            "statistics.json",
            "--arm-key",
            "arm-key.json",
            "--case-receipt",
            "case.json",
            "--outcome-packet",
            "outcome.json",
            "--unlocked-at",
            "2026-09-01T00:00:00Z",
        ],
    ]


@pytest.mark.parametrize("arguments", _complete_arguments())
def test_unknown_flags_are_generic_and_never_echo_values(arguments, capsys):
    secret = "DO-NOT-ECHO-unknown-path-or-reviewer"

    result = main(arguments + ["--unknown-sensitive-flag", secret])

    captured = capsys.readouterr()
    assert result == 2
    assert captured.out == ""
    assert captured.err == "invalid arguments\n"
    assert secret not in captured.err


def test_abbreviated_flags_are_rejected_without_echoing_path(capsys):
    secret = "DO-NOT-ECHO-abbreviated-source-root"
    arguments = _complete_arguments()[0]
    source_index = arguments.index("--source-root")
    arguments[source_index] = "--source-ro"
    arguments[source_index + 1] = secret

    result = main(arguments)

    captured = capsys.readouterr()
    assert result == 2
    assert captured.out == ""
    assert captured.err == "invalid arguments\n"
    assert secret not in captured.err


@pytest.mark.parametrize(
    "arguments",
    [
        _prepare_arguments() + ["--base-qualification-receipt", "only-one.json"],
        _prepare_arguments()
        + [
            "--base-qualification-receipt",
            "base-1.json",
            "--base-qualification-receipt",
            "base-2.json",
            "--arbitrator-qualification-receipt",
            "arb-1.json",
            "--arbitrator-qualification-receipt",
            "arb-2.json",
        ],
        _seal_arguments()
        + [
            item
            for index in range(5)
            for item in ("--submission", f"submission-{index}.json")
        ],
        _seal_arguments()
        + [
            item
            for index in range(4)
            for item in ("--submission", f"submission-{index}.json")
        ]
        + [
            "--arbitrator-qualification-receipt",
            "arb-1.json",
            "--arbitrator-qualification-receipt",
            "arb-2.json",
        ],
    ],
)
def test_cardinality_errors_return_two_before_domain_dispatch(arguments, capsys):
    result = main(arguments)

    captured = capsys.readouterr()
    assert result == 2
    assert captured.out == ""
    assert captured.err == "invalid arguments\n"


def test_qualification_accepts_relative_paths_but_emits_only_safe_projection(
    tmp_path, monkeypatch, capsys
):
    profile_path = tmp_path / "profile.json"
    attempt_path = tmp_path / "attempt.json"
    policy_path = tmp_path / "policy.json"
    output_parent = tmp_path / "qualification-private"
    output_parent.mkdir(mode=0o700)
    output_parent.chmod(0o700)
    _write_json(profile_path, _profile())
    _write_json(attempt_path, _attempt())
    policy_path.write_bytes(POLICY_PATH.read_bytes())
    monkeypatch.chdir(tmp_path)

    result = main(
        [
            "qualify-reviewer",
            "--profile",
            "profile.json",
            "--attempt",
            "attempt.json",
            "--policy",
            "policy.json",
            "--evaluated-at",
            "2026-08-31T00:00:00Z",
            "--output-root",
            "qualification-private/reviewer-1",
        ]
    )

    captured = capsys.readouterr()
    assert result == 0
    assert captured.err == ""
    assert captured.out == (
        '{"diagnosticOnly":true,"objectKind":"QualificationReceipt",'
        '"status":"qualified"}\n'
    )
    receipt = load_exact_json(
        output_parent / "reviewer-1" / "qualification-receipt.json"
    )
    assert receipt["reviewerId"] == "reviewer-business-1"
    assert "reviewer-business-1" not in captured.out
    assert str(output_parent) not in captured.out


def test_domain_errors_are_generic_and_do_not_leak_input_body(tmp_path, capsys):
    secret = "DO-NOT-ECHO-private-reviewer-identity"
    profile_path = tmp_path / "profile.json"
    attempt_path = tmp_path / "attempt.json"
    output_parent = tmp_path / "qualification-private"
    output_parent.mkdir(mode=0o700)
    output_parent.chmod(0o700)
    _write_json(profile_path, _profile(secret))
    _write_json(attempt_path, _attempt("another-reviewer"))

    result = main(
        [
            "qualify-reviewer",
            "--profile",
            str(profile_path),
            "--attempt",
            str(attempt_path),
            "--policy",
            str(POLICY_PATH),
            "--evaluated-at",
            "2026-08-31T00:00:00Z",
            "--output-root",
            str(output_parent / "reviewer-1"),
        ]
    )

    captured = capsys.readouterr()
    assert result == 1
    assert captured.out == ""
    assert captured.err == "lab command failed\n"
    assert secret not in captured.err
    assert not (output_parent / "reviewer-1").exists()


def test_path_canonicalization_completes_before_any_domain_action(
    monkeypatch, capsys
):
    calls = {
        "create_new": 0,
        "open_existing": 0,
        "compile": 0,
        "qualify": 0,
        "prepare": 0,
        "seal": 0,
        "unlock": 0,
    }

    def record(name):
        def operation(*_args, **_kwargs):
            calls[name] += 1
            return object()

        return operation

    original_resolve = Path.resolve

    def fail_late(self, *, strict=False):
        if self.name == "canonicalization-failure.json":
            raise OSError("DO-NOT-ECHO-canonicalization-path")
        return original_resolve(self, strict=strict)

    monkeypatch.setattr(Path, "resolve", fail_late)
    monkeypatch.setattr(cli.PrivateRoot, "create_new", record("create_new"))
    monkeypatch.setattr(cli.PrivateRoot, "open_existing", record("open_existing"))
    monkeypatch.setattr(cli, "compile_golden_gift_case", record("compile"))
    monkeypatch.setattr(cli, "evaluate_calibration", record("qualify"))
    monkeypatch.setattr(cli, "prepare_blind_batch", record("prepare"))
    monkeypatch.setattr(cli, "seal_blind_statistics", record("seal"))
    monkeypatch.setattr(cli, "unlock_fixture_pilot", record("unlock"))
    arguments = _complete_arguments()[0]
    blueprint_index = arguments.index("--blueprint") + 1
    arguments[blueprint_index] = "canonicalization-failure.json"

    try:
        result = main(arguments)
    except OSError:
        result = "raw path error escaped"

    captured = capsys.readouterr()
    assert result == 1
    assert captured.out == ""
    assert captured.err == "lab command failed\n"
    assert calls == {name: 0 for name in calls}
