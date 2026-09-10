#!/usr/bin/env python3
import argparse
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib
from typing import Any


OFFICIAL_UPSTREAM_REPOSITORY = "https://github.com/openai/codex.git"
DISABLED_UPSTREAM_PUSH_URL = "disabled://openai-codex-upstream-read-only"
PINNED_UPSTREAM_SHA = "4ef1d4b89bd419c976b04fefa0fd36844e898340"
# Latest upstream commit merged into the fork. Distinct from PINNED_UPSTREAM_SHA,
# which stays the original fork point because source_import's first parent must
# remain exactly that commit. Upstream-owned files are compared against this one.
CURRENT_UPSTREAM_SHA = "bf5ebd98c567931d82e873a4afdac7548bd85979"
PLANNING_BASE_SHA = "58baf3b21da4768d5f608264c3224660338ef517"
SHA_PATTERN = re.compile(r"[0-9a-f]{40}")

DOCS_OVERRIDE_TEXT = """# AI IP fork documentation override

The repository-root `AGENTS.md` remains in force.

This fork may add product-owned provenance, architecture, evidence, specifications, and implementation plans only under `docs/architecture/`, `docs/evidence/`, and `docs/superpowers/`. This narrow exception does not authorize unrelated product documentation beside upstream Codex modules or weaken any code, test, formatting, or review rule inherited from the repository root.
"""

AI_IP_OVERRIDE_TEXT = """# AI IP machine-governance override

The repository-root `AGENTS.md` remains in force.

Files under `.ai-ip/` may contain only source locks and machine-readable fork governance. Never store credentials, provider tokens, private cases, prompt or response bodies, reviewer mappings, customer content, or generated media here. Source-lock changes must preserve exact Git ancestry and must be verified by `scripts/ai_ip/foundation/verify_upstream_lock.py`.
"""

NOTICE_SUFFIX_TEXT = """
AI IP 1.1 modifications

This distribution includes modifications to OpenAI Codex for the AI IP 1.1 product.
Modification provenance is documented in MODIFICATIONS.md and docs/architecture/codex-fork-patch-ledger.md.
"""

ATTRIBUTES_SUFFIX_TEXT = """
# AI IP evidence and reproducible lockfiles
docs/evidence/**/*.stdout.log -text
docs/evidence/**/*.stderr.log -text
codex-rs/Cargo.lock text eol=lf
MODULE.bazel.lock text eol=lf
pnpm-lock.yaml text eol=lf
"""

REQUIRED_DECISION_PATHS = (
    "docs/superpowers/specs/2026-08-24-ip-agent-saas-design.md",
    "docs/superpowers/specs/2026-08-25-domestic-model-adaptation-design.md",
    "docs/architecture/2026-08-25-legacy-six-version-decision-memory.md",
    "docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md",
    "docs/superpowers/specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md",
    "docs/superpowers/plans/2026-08-25-01a-codex-fork-provenance.md",
)
DOMESTIC_MODEL_DECISION_PATH = (
    "docs/superpowers/specs/2026-08-25-domestic-model-adaptation-design.md"
)
DOMESTIC_MODEL_APPROVAL_MARKER = "- **批准标记：** `APPROVED_FOR_DECISION_TIP`"
DOMESTIC_MODEL_PENDING_MARKER = "- **批准标记：** `PENDING_WRITTEN_REVIEW`"
DOMESTIC_MODEL_APPROVED_STATUS_LINE = "- **状态：** 架构 v1.5 已批准；用户已确认方案 B 的书面规格，允许进入 Phase 0A decision tip"
DOMESTIC_MODEL_ACCEPTANCE_ITEM_COUNT = 9

REQUIRED_PATHS = (
    ".ai-ip/AGENTS.override.md",
    ".ai-ip/upstream.lock.toml",
    ".gitattributes",
    "AGENTS.md",
    "LICENSE",
    "MODIFICATIONS.md",
    "NOTICE",
    "docs/AGENTS.override.md",
    "docs/architecture/codex-fork-patch-ledger.md",
)


class VerificationError(RuntimeError):
    pass


def modifications_required_markers() -> tuple[str, ...]:
    return (
        "# AI IP 1.1 modifications to OpenAI Codex",
        f"Locked upstream commit: `{CURRENT_UPSTREAM_SHA}`.",
        "Product origin status at this checkpoint: `unconfigured`.",
        "DeerFlow is not a runtime dependency or fallback.",
        "All fork-specific patches are recorded in `docs/architecture/codex-fork-patch-ledger.md`.",
    )


def ledger_required_markers() -> tuple[str, ...]:
    return (
        "# Codex fork patch ledger",
        "## F-0001 — Establish source provenance",
        f"- Upstream base: `{PINNED_UPSTREAM_SHA}`.",
        "- Product origin status: `unconfigured`.",
        "- Rollback: remove only the AI IP provenance commit after first returning to the source-import merge commit; never rewrite or discard either imported ancestor.",
    )


def _run_git(repo: Path, *args: str) -> subprocess.CompletedProcess[bytes]:
    environment = {
        key: value
        for key, value in os.environ.items()
        if not key.upper().startswith("GIT_")
    }
    environment.update(
        {
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
    )
    return subprocess.run(
        ["git", "-C", str(repo), *args],
        check=False,
        capture_output=True,
        env=environment,
    )


def _git_bytes(repo: Path, *args: str) -> bytes:
    completed = _run_git(repo, *args)
    if completed.returncode != 0:
        detail = completed.stderr.decode("utf-8", errors="replace").strip()
        raise VerificationError(
            f"git {' '.join(args)} failed ({completed.returncode}): {detail}"
        )
    return completed.stdout


def _git_text(repo: Path, *args: str) -> str:
    return _git_bytes(repo, *args).decode("utf-8").strip()


def _optional_git_text(repo: Path, *args: str) -> str | None:
    completed = _run_git(repo, *args)
    if completed.returncode == 0:
        return completed.stdout.decode("utf-8").strip()
    if completed.returncode in (1, 2):
        return None
    detail = completed.stderr.decode("utf-8", errors="replace").strip()
    raise VerificationError(
        f"git {' '.join(args)} failed ({completed.returncode}): {detail}"
    )


def _is_ancestor(repo: Path, ancestor: str, descendant: str) -> bool:
    completed = _run_git(repo, "merge-base", "--is-ancestor", ancestor, descendant)
    if completed.returncode == 0:
        return True
    if completed.returncode == 1:
        return False
    detail = completed.stderr.decode("utf-8", errors="replace").strip()
    raise VerificationError(
        f"git merge-base --is-ancestor failed ({completed.returncode}): {detail}"
    )


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise VerificationError(message)


def _absolute_git_path(repo: Path, option: str, label: str) -> Path:
    path = Path(_git_text(repo, "rev-parse", "--path-format=absolute", option))
    _require(path.is_absolute(), f"{label} is not absolute: {path}")
    _require(path.is_dir(), f"{label} is not a directory: {path}")
    return path


def _reject_git_object_substitution(repo: Path) -> None:
    replace_refs = _git_text(
        repo, "for-each-ref", "--format=%(refname)", "refs/replace/"
    ).splitlines()
    _require(not replace_refs, f"Git replace refs are forbidden: {replace_refs}")

    git_admin_dir = _absolute_git_path(repo, "--git-dir", "Git admin directory")
    git_common_dir = _absolute_git_path(
        repo, "--git-common-dir", "Git common directory"
    )
    git_grafts_path = Path(
        _git_text(
            repo,
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "info/grafts",
        )
    )
    _require(
        git_grafts_path.is_absolute(),
        f"Git info/grafts path is not absolute: {git_grafts_path}",
    )
    grafts_paths = {
        git_admin_dir / "info/grafts",
        git_common_dir / "info/grafts",
        git_grafts_path,
    }
    forbidden_grafts = [
        path for path in grafts_paths if os.path.lexists(os.fspath(path))
    ]
    _require(
        not forbidden_grafts,
        f"Git info/grafts entries are forbidden: {sorted(map(str, forbidden_grafts))}",
    )


def _read_lock(repo: Path) -> tuple[dict[str, Any], str]:
    path = repo / ".ai-ip/upstream.lock.toml"
    try:
        raw = path.read_text(encoding="utf-8")
        parsed = tomllib.loads(raw)
    except (OSError, tomllib.TOMLDecodeError) as exc:
        raise VerificationError(f"cannot read upstream lock: {exc}") from exc
    _require(isinstance(parsed, dict), "upstream lock root must be a TOML table")
    return parsed, raw


def _exact_section(
    lock: dict[str, Any], name: str, required_keys: set[str]
) -> dict[str, Any]:
    section = lock.get(name)
    _require(isinstance(section, dict), f"missing TOML table [{name}]")
    actual_keys = set(section)
    _require(
        actual_keys == required_keys,
        f"[{name}] keys differ: expected {sorted(required_keys)}, got {sorted(actual_keys)}",
    )
    return section


def _require_equal(actual: Any, expected: Any, label: str) -> None:
    _require(actual == expected, f"{label}: expected {expected!r}, got {actual!r}")


def _require_commit(repo: Path, sha: str, label: str) -> None:
    _require(isinstance(sha, str), f"{label} must be a string")
    _require(
        SHA_PATTERN.fullmatch(sha) is not None, f"{label} must be lowercase 40-hex"
    )
    _require_equal(_git_text(repo, "cat-file", "-t", sha), "commit", label)


def _require_markers(path: Path, markers: tuple[str, ...]) -> None:
    content = path.read_text(encoding="utf-8")
    for marker in markers:
        _require(marker in content, f"{path.name} missing marker: {marker}")


def verify_repository(repo: Path, *, require_clean: bool = True) -> dict[str, str]:
    repo = repo.resolve(strict=True)
    top_level = Path(_git_text(repo, "rev-parse", "--show-toplevel")).resolve()
    _require(top_level == repo, f"--repo must be the worktree root: {top_level}")
    _reject_git_object_substitution(repo)
    _require_equal(
        _git_text(repo, "rev-parse", "--is-shallow-repository"),
        "false",
        "repository shallowness",
    )
    if require_clean:
        status = _git_text(repo, "status", "--porcelain=v1", "--untracked-files=all")
        _require(not status, f"working tree is not clean:\n{status}")

    for relative_path in REQUIRED_PATHS:
        relative = Path(relative_path)
        parent = repo
        for component in relative.parts[:-1]:
            parent /= component
            try:
                resolved_parent = parent.resolve(strict=True)
            except OSError as exc:
                raise VerificationError(
                    f"cannot resolve required path parent: {relative_path}: {exc}"
                ) from exc
            lexical_parent = Path(os.path.abspath(parent))
            _require(
                not parent.is_symlink()
                and os.path.normcase(str(resolved_parent))
                == os.path.normcase(str(lexical_parent)),
                "required path parent must not be a symlink or redirection: "
                f"{relative_path}",
            )
        required_path = repo / relative_path
        _require(required_path.is_file(), f"missing required file: {relative_path}")
        try:
            resolved_path = required_path.resolve(strict=True)
        except OSError as exc:
            raise VerificationError(
                f"cannot resolve required path: {relative_path}: {exc}"
            ) from exc
        lexical_path = Path(os.path.abspath(required_path))
        _require(
            not required_path.is_symlink()
            and os.path.normcase(str(resolved_path))
            == os.path.normcase(str(lexical_path)),
            f"required path must not be a symlink or redirection: {relative_path}",
        )
        index_entries = [
            entry
            for entry in _git_bytes(
                repo, "ls-files", "--stage", "-z", "--", relative_path
            ).split(b"\0")
            if entry
        ]
        if index_entries:
            _require(
                len(index_entries) == 1,
                f"required path must be a single stage-0 100644 blob: {relative_path}",
            )
            metadata, separator, index_path = index_entries[0].partition(b"\t")
            metadata_fields = metadata.split(b" ")
            _require(
                separator == b"\t"
                and len(metadata_fields) == 3
                and index_path == relative_path.encode("utf-8"),
                f"required path must be a single stage-0 100644 blob: {relative_path}",
            )
            mode, object_id_bytes, stage = metadata_fields
            _require(
                mode == b"100644",
                f"required path must use regular Git mode 100644: {relative_path}",
            )
            try:
                object_id = object_id_bytes.decode("ascii")
            except UnicodeDecodeError as exc:
                raise VerificationError(
                    "required path must be a single stage-0 100644 blob: "
                    f"{relative_path}"
                ) from exc
            _require(
                stage == b"0"
                and SHA_PATTERN.fullmatch(object_id) is not None
                and _git_text(repo, "cat-file", "-t", object_id) == "blob",
                f"required path must be a single stage-0 100644 blob: {relative_path}",
            )
        else:
            _require(
                not require_clean,
                f"required path must be a single stage-0 100644 blob: {relative_path}",
            )
    root_override = repo / "AGENTS.override.md"
    tracked_root_override = _git_text(
        repo, "ls-files", "--stage", "--", "AGENTS.override.md"
    )
    _require(
        not root_override.exists()
        and not root_override.is_symlink()
        and not tracked_root_override,
        "root AGENTS.override.md would shadow the imported upstream rules",
    )

    lock, lock_text = _read_lock(repo)
    _require("replace-with" not in lock_text, "decision_tip is a placeholder")
    _require(
        set(lock) == {"openai_codex", "product"}, "unexpected top-level lock table"
    )
    openai = _exact_section(
        lock,
        "openai_codex",
        {
            "decision_tip",
            "integration",
            "license",
            "planning_base",
            "repository",
            "sha",
            "source_import",
            "current_upstream",
        },
    )
    product = _exact_section(
        lock,
        "product",
        {"deerflow_dependency", "name", "origin_status", "runtime"},
    )

    _require_equal(
        openai["repository"], OFFICIAL_UPSTREAM_REPOSITORY, "openai_codex.repository"
    )
    _require_equal(openai["sha"], PINNED_UPSTREAM_SHA, "openai_codex.sha")
    _require_equal(
        openai["integration"], "git-merge-ancestor", "openai_codex.integration"
    )
    _require_equal(openai["license"], "Apache-2.0", "openai_codex.license")
    _require_equal(
        openai["planning_base"], PLANNING_BASE_SHA, "openai_codex.planning_base"
    )
    _require_equal(product["name"], "AI IP 1.1", "product.name")
    _require_equal(product["runtime"], "codex-app-server", "product.runtime")
    deerflow_dependency = product["deerflow_dependency"]
    _require(
        type(deerflow_dependency) is bool and deerflow_dependency is False,
        "product.deerflow_dependency must be boolean false",
    )
    _require_equal(product["origin_status"], "unconfigured", "product.origin_status")

    upstream_sha = openai["sha"]
    current_upstream = openai["current_upstream"]
    planning_base = openai["planning_base"]
    decision_tip = openai["decision_tip"]
    source_import = openai["source_import"]
    _require(
        isinstance(decision_tip, str), "openai_codex.decision_tip must be a string"
    )
    _require_commit(repo, upstream_sha, "openai_codex.sha")
    _require_commit(repo, current_upstream, "openai_codex.current_upstream")
    _require_commit(repo, planning_base, "openai_codex.planning_base")
    _require_commit(repo, decision_tip, "openai_codex.decision_tip")
    _require_commit(repo, source_import, "openai_codex.source_import")
    _require(
        _is_ancestor(repo, upstream_sha, current_upstream),
        "original fork point is not an ancestor of the current upstream",
    )
    head = _git_text(repo, "rev-parse", "HEAD")
    _require_commit(repo, head, "HEAD")
    _require(
        _is_ancestor(repo, planning_base, decision_tip),
        "planning base is not an ancestor of decision tip",
    )
    for relative_path in REQUIRED_DECISION_PATHS:
        decision_entries = [
            entry
            for entry in _git_bytes(
                repo, "ls-tree", "-z", decision_tip, "--", relative_path
            ).split(b"\0")
            if entry
        ]
        _require(
            decision_entries,
            f"decision tip missing required file: {relative_path}",
        )
        metadata, separator, entry_path = decision_entries[0].partition(b"\t")
        metadata_fields = metadata.split(b" ")
        _require(
            len(decision_entries) == 1
            and separator == b"\t"
            and metadata_fields[:2] == [b"100644", b"blob"]
            and len(metadata_fields) == 3
            and entry_path == relative_path.encode("utf-8"),
            f"decision tip required file must be a single 100644 blob: {relative_path}",
        )
    domestic_model_decision = _git_bytes(
        repo, "show", f"{decision_tip}:{DOMESTIC_MODEL_DECISION_PATH}"
    ).decode("utf-8")
    domestic_model_lines = domestic_model_decision.splitlines()
    _require(
        domestic_model_lines.count(DOMESTIC_MODEL_APPROVAL_MARKER) == 1,
        "domestic-model decision is not fully approved for the decision tip",
    )
    status_lines = [
        line for line in domestic_model_lines if line.startswith("- **状态：** ")
    ]
    _require(
        status_lines == [DOMESTIC_MODEL_APPROVED_STATUS_LINE],
        "domestic-model decision is not fully approved for the decision tip",
    )
    _require(
        DOMESTIC_MODEL_PENDING_MARKER not in domestic_model_lines,
        "domestic-model decision is not fully approved for the decision tip",
    )
    try:
        checklist_start = domestic_model_lines.index("## 10. 书面验收清单")
        checklist_end = domestic_model_lines.index("## 11. 锁定源码依据")
    except ValueError as error:
        raise VerificationError(
            "domestic-model decision is not fully approved for the decision tip"
        ) from error
    checklist_lines = [
        line
        for line in domestic_model_lines[checklist_start + 1 : checklist_end]
        if re.fullmatch(r"- \[(?: |x|X)\] .+", line)
    ]
    _require(
        len(checklist_lines) == DOMESTIC_MODEL_ACCEPTANCE_ITEM_COUNT
        and all(line.startswith("- [x] ") for line in checklist_lines),
        "domestic-model decision is not fully approved for the decision tip",
    )
    source_import_parents = _git_text(
        repo, "show", "-s", "--format=%P", source_import
    ).split()
    _require_equal(
        source_import_parents,
        [upstream_sha, decision_tip],
        "source-import parents",
    )
    _require(
        _is_ancestor(repo, source_import, head),
        "source-import commit is not an ancestor of HEAD",
    )
    _require(
        _is_ancestor(repo, upstream_sha, head),
        "upstream SHA is not an ancestor of HEAD",
    )
    _require(
        _is_ancestor(repo, current_upstream, head),
        "current upstream SHA is not an ancestor of HEAD",
    )
    _require(
        _is_ancestor(repo, decision_tip, head),
        "decision tip is not an ancestor of HEAD",
    )

    missing_output = _git_text(
        repo,
        "rev-list",
        "--objects",
        "--missing=print",
        upstream_sha,
        decision_tip,
        source_import,
        head,
    )
    missing_objects = [
        line for line in missing_output.splitlines() if line.startswith("?")
    ]
    _require(
        not missing_objects, f"Git object closure is incomplete: {missing_objects}"
    )
    promisor = _optional_git_text(
        repo, "config", "--bool", "--get", "remote.upstream.promisor"
    )
    _require(
        promisor in (None, "false"), f"upstream remote is promisor-backed: {promisor}"
    )
    _require(
        _optional_git_text(
            repo, "config", "--get", "remote.upstream.partialclonefilter"
        )
        is None,
        "upstream remote has a partial-clone filter",
    )
    _require(
        _optional_git_text(repo, "config", "--get", "extensions.partialclone") is None,
        "repository has extensions.partialclone",
    )

    _require_equal(
        _git_text(repo, "remote", "get-url", "--all", "upstream").splitlines(),
        [OFFICIAL_UPSTREAM_REPOSITORY],
        "upstream fetch URLs",
    )
    _require_equal(
        _git_text(
            repo, "remote", "get-url", "--push", "--all", "upstream"
        ).splitlines(),
        [DISABLED_UPSTREAM_PUSH_URL],
        "upstream push URLs",
    )
    origin = _optional_git_text(repo, "config", "--get-regexp", r"^remote\.origin\.")
    _require(
        origin is None,
        f"origin_status is unconfigured but remote origin exists or is configured: {origin}",
    )

    upstream_license = _git_bytes(repo, "show", f"{current_upstream}:LICENSE")
    _require(
        (repo / "LICENSE").read_bytes() == upstream_license,
        "LICENSE must remain byte-identical to pinned upstream",
    )
    upstream_agents = _git_bytes(repo, "show", f"{current_upstream}:AGENTS.md")
    _require(
        (repo / "AGENTS.md").read_bytes() == upstream_agents,
        "AGENTS.md must remain byte-identical to pinned upstream",
    )
    upstream_notice = _git_bytes(repo, "show", f"{current_upstream}:NOTICE")
    _require(
        (repo / "NOTICE").read_bytes()
        == upstream_notice + NOTICE_SUFFIX_TEXT.encode("utf-8"),
        "NOTICE must equal pinned upstream bytes plus the exact AI IP suffix",
    )
    _require_equal(
        (repo / "docs/AGENTS.override.md").read_text(encoding="utf-8"),
        DOCS_OVERRIDE_TEXT,
        "docs/AGENTS.override.md",
    )
    _require_equal(
        (repo / ".ai-ip/AGENTS.override.md").read_text(encoding="utf-8"),
        AI_IP_OVERRIDE_TEXT,
        ".ai-ip/AGENTS.override.md",
    )

    upstream_attributes = _git_bytes(
        repo, "show", f"{current_upstream}:.gitattributes"
    )
    _require(
        (repo / ".gitattributes").read_bytes()
        == upstream_attributes + ATTRIBUTES_SUFFIX_TEXT.encode("utf-8"),
        ".gitattributes must equal pinned upstream bytes plus the exact AI IP suffix",
    )

    _require_markers(repo / "MODIFICATIONS.md", modifications_required_markers())
    _require_markers(
        repo / "docs/architecture/codex-fork-patch-ledger.md",
        ledger_required_markers(),
    )
    return {
        "decisionTip": decision_tip,
        "head": head,
        "originStatus": product["origin_status"],
        "upstreamSha": upstream_sha,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Verify the AI IP Codex source lock")
    parser.add_argument("--repo", required=True, type=Path)
    parser.add_argument("--allow-dirty", action="store_true")
    args = parser.parse_args(argv)
    try:
        result = verify_repository(args.repo, require_clean=not args.allow_dirty)
    except (OSError, UnicodeError, VerificationError) as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1
    print(
        "PASS: Codex upstream lock verified "
        f"(upstream={result['upstreamSha']}, decision={result['decisionTip']}, "
        f"head={result['head']}, origin={result['originStatus']})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
