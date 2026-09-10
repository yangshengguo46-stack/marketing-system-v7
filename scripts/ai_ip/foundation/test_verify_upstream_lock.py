import os
from pathlib import Path
import subprocess
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import verify_upstream_lock as verifier


GIT_ENV = {
    "GIT_AUTHOR_NAME": "AI IP Test",
    "GIT_AUTHOR_EMAIL": "ai-ip-test@example.invalid",
    "GIT_COMMITTER_NAME": "AI IP Test",
    "GIT_COMMITTER_EMAIL": "ai-ip-test@example.invalid",
}


def _git(repo: Path, *args: str) -> str:
    completed = subprocess.run(
        ["git", "-C", str(repo), *args],
        check=False,
        capture_output=True,
        text=True,
        env={**os.environ, **GIT_ENV},
    )
    if completed.returncode != 0:
        raise AssertionError(
            f"git {' '.join(args)} failed ({completed.returncode}): "
            f"{completed.stderr.strip()}"
        )
    return completed.stdout.strip()


def _write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8", newline="\n")


def _commit(repo: Path, message: str) -> str:
    _git(repo, "add", "-A")
    _git(repo, "commit", "-m", message)
    return _git(repo, "rev-parse", "HEAD")


def _replace_once(path: Path, old: str, new: str) -> None:
    content = path.read_text(encoding="utf-8")
    assert content.count(old) == 1
    path.write_text(content.replace(old, new, 1), encoding="utf-8", newline="\n")


def _valid_repository(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    *,
    domestic_model_approved: bool = True,
    decision_symlink_path: str | None = None,
) -> tuple[Path, str, str, str]:
    repo = tmp_path / "fork"
    repo.mkdir()
    _git(repo, "init", "-q")
    _git(repo, "config", "user.name", "AI IP Test")
    _git(repo, "config", "user.email", "ai-ip-test@example.invalid")

    upstream_notice = "OpenAI Codex\nCopyright 2025 OpenAI\n"
    upstream_attributes = (
        "codex-rs/app-server-protocol/schema/** linguist-generated\n"
        "codex-rs/hooks/schema/generated/** linguist-generated\n"
    )
    _write(repo / "LICENSE", "Apache License\nVersion 2.0\n")
    _write(repo / "NOTICE", upstream_notice)
    _write(repo / ".gitattributes", upstream_attributes)
    _write(repo / "AGENTS.md", "# Upstream rules\n")
    upstream_sha = _commit(repo, "upstream")

    # A later upstream commit the fork re-syncs onto. upstream_sha stays the
    # original fork point because source_import's first parent must be exactly
    # it; upstream-owned files are compared against this later commit instead.
    upstream_license_synced = "Apache License\nVersion 2.0\nSynced\n"
    upstream_notice_synced = upstream_notice + "Synced notice line.\n"
    upstream_attributes_synced = upstream_attributes + "synced/path text eol=lf\n"
    upstream_agents_synced = "# Upstream rules\n\nSynced rules\n"
    _write(repo / "LICENSE", upstream_license_synced)
    _write(repo / "NOTICE", upstream_notice_synced)
    _write(repo / ".gitattributes", upstream_attributes_synced)
    _write(repo / "AGENTS.md", upstream_agents_synced)
    current_upstream_sha = _commit(repo, "upstream sync target")

    _git(repo, "switch", "--orphan", "decisions")
    _git(repo, "rm", "-rf", "--ignore-unmatch", ".")
    _write(repo / "docs/decision.md", "planning base\n")
    planning_base_sha = _commit(repo, "planning base")
    _write(repo / "docs/decision.md", "planning base\ndecision tip\n")
    for relative_path in verifier.REQUIRED_DECISION_PATHS:
        content = f"decision input: {relative_path}\n"
        if relative_path == verifier.DOMESTIC_MODEL_DECISION_PATH:
            if domestic_model_approved:
                content += f"{verifier.DOMESTIC_MODEL_APPROVED_STATUS_LINE}\n"
                content += f"{verifier.DOMESTIC_MODEL_APPROVAL_MARKER}\n"
                content += "## 10. 书面验收清单\n"
                content += "\n".join(
                    f"- [x] approved decision item {index}" for index in range(1, 10)
                )
                content += "\n## 11. 锁定源码依据\n"
            else:
                content += "- **状态：** 架构 v1.5 候选；等待书面复核\n"
                content += f"{verifier.DOMESTIC_MODEL_APPROVAL_MARKER}\n"
                content += f"{verifier.DOMESTIC_MODEL_PENDING_MARKER}\n"
                content += "## 10. 书面验收清单\n"
                content += "\n".join(
                    f"- [ ] pending decision item {index}" for index in range(1, 10)
                )
                content += "\n## 11. 锁定源码依据\n"
        _write(repo / relative_path, content)
    if decision_symlink_path is None:
        decision_tip_sha = _commit(repo, "decision tip")
    else:
        _git(repo, "add", "-A")
        link_payload = tmp_path / "decision-link-payload"
        _write(link_payload, "missing-target")
        link_blob = _git(repo, "hash-object", "-w", str(link_payload))
        _git(
            repo,
            "update-index",
            "--add",
            "--cacheinfo",
            f"120000,{link_blob},{decision_symlink_path}",
        )
        _git(repo, "commit", "-m", "decision tip")
        decision_tip_sha = _git(repo, "rev-parse", "HEAD")
        _git(
            repo,
            "restore",
            "--worktree",
            "--source=HEAD",
            "--",
            decision_symlink_path,
        )

    _git(repo, "switch", "-c", "product", upstream_sha)
    _git(
        repo,
        "merge",
        "--no-ff",
        "--allow-unrelated-histories",
        decision_tip_sha,
        "-m",
        "source import",
    )
    source_import_sha = _git(repo, "rev-parse", "HEAD")
    _git(
        repo,
        "merge",
        "--no-ff",
        current_upstream_sha,
        "-m",
        "upstream sync",
    )
    _git(repo, "remote", "add", "upstream", verifier.OFFICIAL_UPSTREAM_REPOSITORY)
    _git(
        repo,
        "remote",
        "set-url",
        "--push",
        "upstream",
        verifier.DISABLED_UPSTREAM_PUSH_URL,
    )

    monkeypatch.setattr(verifier, "PINNED_UPSTREAM_SHA", upstream_sha)
    monkeypatch.setattr(verifier, "CURRENT_UPSTREAM_SHA", current_upstream_sha)
    monkeypatch.setattr(verifier, "PLANNING_BASE_SHA", planning_base_sha)

    lock = f'''[openai_codex]
repository = "{verifier.OFFICIAL_UPSTREAM_REPOSITORY}"
sha = "{upstream_sha}"
integration = "git-merge-ancestor"
license = "Apache-2.0"
planning_base = "{planning_base_sha}"
decision_tip = "{decision_tip_sha}"
source_import = "{source_import_sha}"
current_upstream = "{current_upstream_sha}"

[product]
name = "AI IP 1.1"
runtime = "codex-app-server"
deerflow_dependency = false
origin_status = "unconfigured"
'''
    _write(repo / ".ai-ip/upstream.lock.toml", lock)
    _write(repo / "docs/AGENTS.override.md", verifier.DOCS_OVERRIDE_TEXT)
    _write(repo / ".ai-ip/AGENTS.override.md", verifier.AI_IP_OVERRIDE_TEXT)
    _write(
        repo / ".gitattributes",
        upstream_attributes_synced + verifier.ATTRIBUTES_SUFFIX_TEXT,
    )
    _write(repo / "NOTICE", upstream_notice_synced + verifier.NOTICE_SUFFIX_TEXT)
    _write(
        repo / "MODIFICATIONS.md",
        "\n".join(verifier.modifications_required_markers()) + "\n",
    )
    _write(
        repo / "docs/architecture/codex-fork-patch-ledger.md",
        "\n".join(verifier.ledger_required_markers()) + "\n",
    )
    _commit(repo, "governance")
    return repo, upstream_sha, planning_base_sha, decision_tip_sha


def test_valid_fork_passes(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    repo, upstream_sha, _, decision_tip_sha = _valid_repository(tmp_path, monkeypatch)

    result = verifier.verify_repository(repo)

    assert result == {
        "decisionTip": decision_tip_sha,
        "head": _git(repo, "rev-parse", "HEAD"),
        "originStatus": "unconfigured",
        "upstreamSha": upstream_sha,
    }


def test_all_git_subprocesses_disable_lazy_fetch_and_optional_locks(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo = tmp_path / "fork"
    repo.mkdir()
    _git(repo, "init", "-q")
    monkeypatch.setenv("AI_IP_TEST_SENTINEL", "preserved")
    monkeypatch.setenv("GIT_OBJECT_DIRECTORY", str(tmp_path / "ambient-objects"))
    monkeypatch.setenv("git_namespace", "ambient-lowercase-namespace")
    captured_environments: list[dict[str, str] | None] = []
    run = subprocess.run

    def capture_environment(
        *args: object, **kwargs: object
    ) -> subprocess.CompletedProcess:
        captured_environments.append(kwargs.get("env"))
        return run(*args, **kwargs)

    monkeypatch.setattr(verifier.subprocess, "run", capture_environment)

    completed = verifier._run_git(repo, "rev-parse", "--git-dir")

    assert completed.returncode == 0
    assert len(captured_environments) == 1
    environment = captured_environments[0]
    assert environment is not None
    assert environment["AI_IP_TEST_SENTINEL"] == "preserved"
    assert {
        key: value
        for key, value in environment.items()
        if key.upper().startswith("GIT_")
    } == {
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


def test_packed_replace_ref_for_locked_commit_is_rejected(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, upstream_sha, _, _ = _valid_repository(tmp_path, monkeypatch)
    upstream_tree = _git(repo, "show", "-s", "--format=%T", upstream_sha)
    replacement_commit = _git(
        repo, "commit-tree", upstream_tree, "-m", "substitute locked upstream"
    )
    _git(repo, "replace", upstream_sha, replacement_commit)
    replace_ref = f"refs/replace/{upstream_sha}"
    loose_replace_ref_path = Path(
        _git(
            repo,
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            replace_ref,
        )
    )
    packed_refs_path = Path(
        _git(
            repo,
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "packed-refs",
        )
    )
    packed_refs_before = (
        packed_refs_path.read_text(encoding="utf-8")
        if packed_refs_path.is_file()
        else ""
    )
    assert loose_replace_ref_path.is_file()
    assert replace_ref not in packed_refs_before
    assert _git(repo, "for-each-ref", "--format=%(refname)", "refs/replace/") == (
        replace_ref
    )

    with pytest.raises(
        verifier.VerificationError, match="Git replace refs are forbidden"
    ):
        verifier.verify_repository(repo)

    _git(repo, "pack-refs", "--all", "--prune")
    assert not os.path.lexists(os.fspath(loose_replace_ref_path))
    assert replace_ref in packed_refs_path.read_text(encoding="utf-8")
    assert _git(repo, "for-each-ref", "--format=%(refname)", "refs/replace/") == (
        replace_ref
    )

    with pytest.raises(
        verifier.VerificationError, match="Git replace refs are forbidden"
    ):
        verifier.verify_repository(repo)


def test_common_git_grafts_entry_is_rejected(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, upstream_sha, _, _ = _valid_repository(tmp_path, monkeypatch)
    linked_worktree = tmp_path / "linked-worktree"
    _git(repo, "worktree", "add", "--detach", str(linked_worktree), "HEAD")
    source_import_sha = _git(
        repo, "rev-list", "--first-parent", "--merges", f"{upstream_sha}..HEAD"
    ).splitlines()[0]
    source_import_parents = _git(repo, "show", "-s", "--format=%P", source_import_sha)
    common_git_dir = Path(
        _git(
            linked_worktree,
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
        )
    )
    grafts_path = common_git_dir / "info/grafts"
    _write(grafts_path, f"{source_import_sha} {source_import_parents}\n")

    with pytest.raises(
        verifier.VerificationError, match="Git info/grafts entries are forbidden"
    ):
        verifier.verify_repository(linked_worktree)

    _write(grafts_path, "")
    with pytest.raises(
        verifier.VerificationError, match="Git info/grafts entries are forbidden"
    ):
        verifier.verify_repository(linked_worktree)

    grafts_path.unlink()
    try:
        grafts_path.symlink_to(tmp_path / "missing-grafts-target")
    except OSError:
        return
    assert grafts_path.is_symlink()
    assert not grafts_path.exists()

    with pytest.raises(
        verifier.VerificationError, match="Git info/grafts entries are forbidden"
    ):
        verifier.verify_repository(linked_worktree)


def test_git_subprocess_ignores_ambient_repository_redirection(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo_a = tmp_path / "repo-a"
    repo_b = tmp_path / "repo-b"
    repo_a.mkdir()
    repo_b.mkdir()
    _git(repo_a, "init", "-q")
    _git(repo_b, "init", "-q")
    monkeypatch.setenv("GIT_DIR", str(repo_b / ".git"))
    monkeypatch.setenv("GIT_WORK_TREE", str(repo_b))

    top_level = verifier._git_text(repo_a, "rev-parse", "--show-toplevel")

    assert Path(top_level).resolve() == repo_a.resolve()


def test_decision_tip_required_input_rejects_git_symlink_mode(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    symlink_path = verifier.REQUIRED_DECISION_PATHS[0]
    repo, _, _, _ = _valid_repository(
        tmp_path,
        monkeypatch,
        decision_symlink_path=symlink_path,
    )

    with pytest.raises(
        verifier.VerificationError,
        match="decision tip required file must be a single 100644 blob",
    ):
        verifier.verify_repository(repo)


def test_dirty_tree_is_rejected(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
    _write(repo / "untracked.txt", "dirty\n")

    with pytest.raises(verifier.VerificationError, match="working tree is not clean"):
        verifier.verify_repository(repo)


def test_required_path_rejects_redirecting_parent_directory(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
    ai_ip_directory = repo / ".ai-ip"
    external_directory = tmp_path / "external-ai-ip"
    _git(
        repo,
        "update-index",
        "--skip-worktree",
        ".ai-ip/AGENTS.override.md",
        ".ai-ip/upstream.lock.toml",
    )
    _write(repo / ".git/info/exclude", ".ai-ip\n")
    ai_ip_directory.rename(external_directory)
    try:
        ai_ip_directory.symlink_to(external_directory, target_is_directory=True)
    except OSError as error:
        external_directory.rename(ai_ip_directory)
        pytest.skip(f"directory symlink unavailable on this platform: {error}")
    assert _git(repo, "status", "--porcelain=v1", "--untracked-files=all") == ""

    with pytest.raises(
        verifier.VerificationError,
        match="required path parent must not be a symlink or redirection",
    ):
        verifier.verify_repository(repo)


def test_required_path_rejects_git_symlink_mode(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
    link_payload = tmp_path / "license-link-payload"
    _write(link_payload, "outside-license")
    link_blob = _git(repo, "hash-object", "-w", str(link_payload))
    _git(
        repo,
        "update-index",
        "--add",
        "--cacheinfo",
        f"120000,{link_blob},LICENSE",
    )

    with pytest.raises(
        verifier.VerificationError,
        match="required path must use regular Git mode 100644",
    ):
        verifier.verify_repository(repo, require_clean=False)


def test_allow_dirty_required_path_rejects_nonzero_index_stage(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
    license_blob = _git(repo, "rev-parse", "HEAD:LICENSE")
    _git(repo, "rm", "--cached", "--", "LICENSE")
    completed = subprocess.run(
        ["git", "-C", str(repo), "update-index", "-z", "--index-info"],
        check=False,
        capture_output=True,
        input=f"100644 {license_blob} 1\tLICENSE\0".encode(),
        env={**os.environ, **GIT_ENV},
    )
    assert completed.returncode == 0, completed.stderr.decode("utf-8", errors="replace")

    with pytest.raises(
        verifier.VerificationError,
        match="required path must be a single stage-0 100644 blob",
    ):
        verifier.verify_repository(repo, require_clean=False)


def test_broken_root_override_symlink_cannot_look_absent(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
    link_payload = tmp_path / "override-link-payload"
    _write(link_payload, "missing-target")
    link_blob = _git(repo, "hash-object", "-w", str(link_payload))
    _git(
        repo,
        "update-index",
        "--add",
        "--cacheinfo",
        f"120000,{link_blob},AGENTS.override.md",
    )

    with pytest.raises(
        verifier.VerificationError, match="root AGENTS.override.md would shadow"
    ):
        verifier.verify_repository(repo, require_clean=False)


def test_wrong_locked_upstream_sha_is_rejected(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, upstream_sha, _, _ = _valid_repository(tmp_path, monkeypatch)
    _replace_once(repo / ".ai-ip/upstream.lock.toml", upstream_sha, "0" * 40)

    with pytest.raises(verifier.VerificationError, match="openai_codex.sha"):
        verifier.verify_repository(repo, require_clean=False)


def test_deerflow_dependency_rejects_integer_zero(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
    _replace_once(
        repo / ".ai-ip/upstream.lock.toml",
        "deerflow_dependency = false",
        "deerflow_dependency = 0",
    )

    with pytest.raises(
        verifier.VerificationError,
        match="product.deerflow_dependency must be boolean false",
    ):
        verifier.verify_repository(repo, require_clean=False)


def test_unconfigured_origin_rejects_an_origin_remote(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
    _git(repo, "remote", "add", "origin", "https://example.invalid/ai-ip.git")

    with pytest.raises(
        verifier.VerificationError,
        match="origin_status is unconfigured but remote origin exists",
    ):
        verifier.verify_repository(repo)


def test_unconfigured_origin_rejects_push_only_configuration(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
    _git(repo, "config", "remote.origin.pushurl", "https://example.invalid/push.git")

    with pytest.raises(
        verifier.VerificationError,
        match="origin_status is unconfigured but remote origin exists",
    ):
        verifier.verify_repository(repo)


@pytest.mark.parametrize(
    ("config_key", "error_match"),
    (
        ("remote.upstream.url", "upstream fetch URLs"),
        ("remote.upstream.pushurl", "upstream push URLs"),
    ),
)
def test_upstream_remote_rejects_additional_urls(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    config_key: str,
    error_match: str,
) -> None:
    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
    _git(repo, "config", "--add", config_key, "https://example.invalid/extra.git")

    with pytest.raises(verifier.VerificationError, match=error_match):
        verifier.verify_repository(repo)


def test_notice_must_preserve_upstream_bytes_and_exact_suffix(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
    _write(repo / "NOTICE", "replacement notice\n" + verifier.NOTICE_SUFFIX_TEXT)

    with pytest.raises(verifier.VerificationError, match="NOTICE must equal"):
        verifier.verify_repository(repo, require_clean=False)


def test_required_git_attribute_cannot_be_removed(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
    attributes = repo / ".gitattributes"
    _replace_once(attributes, "MODULE.bazel.lock text eol=lf\n", "")

    with pytest.raises(
        verifier.VerificationError,
        match=".gitattributes must equal pinned upstream bytes plus the exact AI IP suffix",
    ):
        verifier.verify_repository(repo, require_clean=False)


def test_later_git_attribute_override_is_rejected(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
    attributes = repo / ".gitattributes"
    _write(
        attributes,
        attributes.read_text(encoding="utf-8") + "MODULE.bazel.lock -text\n",
    )

    with pytest.raises(
        verifier.VerificationError,
        match=".gitattributes must equal pinned upstream bytes plus the exact AI IP suffix",
    ):
        verifier.verify_repository(repo, require_clean=False)


def test_source_import_commit_must_bind_both_exact_parents(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, upstream_sha, _, _ = _valid_repository(tmp_path, monkeypatch)
    # rev-list lists newest first; the source import is the oldest merge after
    # the fork point, followed by any later upstream sync merges.
    source_import_sha = _git(
        repo, "rev-list", "--first-parent", "--merges", f"{upstream_sha}..HEAD"
    ).splitlines()[-1]
    _replace_once(
        repo / ".ai-ip/upstream.lock.toml",
        f'source_import = "{source_import_sha}"',
        f'source_import = "{upstream_sha}"',
    )

    with pytest.raises(verifier.VerificationError, match="source-import parents"):
        verifier.verify_repository(repo, require_clean=False)


def test_decision_tip_must_contain_all_reviewed_inputs(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, _, planning_base_sha, decision_tip_sha = _valid_repository(
        tmp_path, monkeypatch
    )
    _replace_once(
        repo / ".ai-ip/upstream.lock.toml",
        f'decision_tip = "{decision_tip_sha}"',
        f'decision_tip = "{planning_base_sha}"',
    )

    with pytest.raises(
        verifier.VerificationError, match="decision tip missing required file"
    ):
        verifier.verify_repository(repo, require_clean=False)


def test_domestic_model_decision_must_be_fully_approved(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, _, _, _ = _valid_repository(
        tmp_path, monkeypatch, domestic_model_approved=False
    )

    with pytest.raises(
        verifier.VerificationError,
        match="domestic-model decision is not fully approved for the decision tip",
    ):
        verifier.verify_repository(repo, require_clean=False)


def test_decision_tip_must_descend_from_planning_base(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, upstream_sha, _, decision_tip_sha = _valid_repository(tmp_path, monkeypatch)
    _replace_once(
        repo / ".ai-ip/upstream.lock.toml",
        f'decision_tip = "{decision_tip_sha}"',
        f'decision_tip = "{upstream_sha}"',
    )

    with pytest.raises(
        verifier.VerificationError,
        match="planning base is not an ancestor of decision tip",
    ):
        verifier.verify_repository(repo, require_clean=False)


def test_placeholder_decision_tip_is_rejected(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo, _, _, decision_tip_sha = _valid_repository(tmp_path, monkeypatch)
    _replace_once(
        repo / ".ai-ip/upstream.lock.toml",
        f'decision_tip = "{decision_tip_sha}"',
        'decision_tip = "replace-with-decision-tip"',
    )

    with pytest.raises(
        verifier.VerificationError, match="decision_tip is a placeholder"
    ):
        verifier.verify_repository(repo, require_clean=False)
