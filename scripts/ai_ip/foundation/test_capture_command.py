import hashlib
import json
from pathlib import Path

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
