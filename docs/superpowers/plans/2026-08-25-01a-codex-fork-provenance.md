> **HISTORICAL_COMPLETED — NOT EXECUTABLE (2026-09-05).** This completed provenance/business-primitive record is preserved as history. Its commands, follow-on authorizations and proof prerequisites are superseded by the approved [business-first cleanup](../specs/2026-09-05-business-first-cleanup-design.md) and current [roadmap](2026-08-25-00-codex-ai-ip-master-roadmap.md). Retained implementation remains; the old Phase 0A/06A/06B/07A/07B gates are `RETIRED_NOT_PASSED`. No historical child is executable and no business PASS or model/customer-release qualification is claimed.

# Phase 0A.1 — Codex Fork Provenance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 建立同时保留锁定 Codex 上游祖先与当前 AI IP 决策祖先的干净 fork，并让来源、许可、origin 状态和回退边界可机器验证。

**Architecture:** 执行时从锁定的 Codex SHA 创建独立 Git worktree，再以保留双方历史的 merge 接入已提交决策 tip。一个仅依赖 Python 标准库的 verifier 校验 Git 祖先、完整对象闭包、上游 remote、锁文件、原始许可文本和 fork 治理文件；本计划不修改 Codex runtime，也不宣称 G0–G2 已通过。

**Tech Stack:** Git，Python 3.11 标准库，pytest 8.3.5，TOML，OpenAI Codex `4ef1d4b89bd419c976b04fefa0fd36844e898340`。

---

执行前必须使用 `superpowers:using-git-worktrees`。本计划从决策仓库根开始；Task 1 建成 `../ai-ip-phase-0a` 后，Task 2–3 全部在该新 worktree 根执行。不得复制源码快照，不得创建虚构产品 `origin`，不得改动 `codex-rs/`。

## 文件职责图

- `scripts/ai_ip/foundation/verify_upstream_lock.py`：唯一来源锁机器校验器；验证 Git、TOML、许可、NOTICE、治理文件和 origin 状态。
- `scripts/ai_ip/foundation/test_verify_upstream_lock.py`：用临时双祖先 Git 仓覆盖通过与 fail-closed 路径。
- `docs/AGENTS.override.md`：只为 fork 自有规划、架构与证据文档提供窄范围上游规则例外。
- `.ai-ip/AGENTS.override.md`：约束机器来源台账，不允许凭证或业务正文进入 `.ai-ip/`。
- `.gitattributes`：保留上游两条 generated 属性，并追加 AI IP 证据和 lockfile 的跨平台字节规则。
- `.ai-ip/upstream.lock.toml`：记录官方仓、锁定 SHA、决策 tip 和尚未配置产品 origin 的事实。
- `MODIFICATIONS.md`：说明衍生 fork、修改分类、同步与许可策略。
- `NOTICE`：完整保留上游 NOTICE，并追加 AI IP 修改说明。
- `docs/architecture/codex-fork-patch-ledger.md`：建立可追加的 fork patch 台账与首条来源记录。

---

### Task 1：建立保留双祖先的 Codex fork worktree

**Files:**

- Verify only: `docs/superpowers/specs/2026-08-24-ip-agent-saas-design.md`
- Verify only: `docs/superpowers/specs/2026-08-25-domestic-model-adaptation-design.md`
- Verify only: `docs/architecture/2026-08-25-legacy-six-version-decision-memory.md`
- Verify only: `docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md`
- Verify only: `docs/superpowers/specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md`
- Verify only: `docs/superpowers/plans/2026-08-25-01a-codex-fork-provenance.md`

- [ ] **Step 1：确认决策 tip 已提交且当前决策 worktree 干净**

Run from the current decision repository root:

```bash
set -euo pipefail
test "$(git rev-parse --show-toplevel)" = "$PWD"
test "$(git branch --show-current)" = "codex/codex-first-design"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
test "$(git rev-parse 58baf3b)" = "58baf3b21da4768d5f608264c3224660338ef517"
git merge-base --is-ancestor 58baf3b21da4768d5f608264c3224660338ef517 HEAD
git show HEAD:docs/superpowers/specs/2026-08-24-ip-agent-saas-design.md >/dev/null
git show HEAD:docs/superpowers/specs/2026-08-25-domestic-model-adaptation-design.md >/dev/null
git show HEAD:docs/superpowers/specs/2026-08-25-domestic-model-adaptation-design.md \
  | rg -F -x -- '- **批准标记：** `APPROVED_FOR_DECISION_TIP`' >/dev/null
git show HEAD:docs/superpowers/specs/2026-08-25-domestic-model-adaptation-design.md \
  | rg -F -x -- '- **状态：** 架构 v1.5 已批准；用户已确认方案 B 的书面规格，允许进入 Phase 0A decision tip' >/dev/null
test "$(git show HEAD:docs/superpowers/specs/2026-08-25-domestic-model-adaptation-design.md \
  | awk '/^- \[x\] / { count += 1 } END { print count + 0 }')" = "9"
! git show HEAD:docs/superpowers/specs/2026-08-25-domestic-model-adaptation-design.md \
  | rg -n 'PENDING_WRITTEN_REVIEW|^- \[ \] ' >/dev/null
git show HEAD:docs/architecture/2026-08-25-legacy-six-version-decision-memory.md >/dev/null
git show HEAD:docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md >/dev/null
git show HEAD:docs/superpowers/specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md >/dev/null
git show HEAD:docs/superpowers/plans/2026-08-25-01a-codex-fork-provenance.md >/dev/null
if git show-ref --verify --quiet refs/ai-ip/phase-0a-decision-tip; then
  printf '%s\n' 'FAIL: frozen Phase 0A decision ref already exists; inspect the prior attempt' >&2
  exit 1
fi
git update-ref refs/ai-ip/phase-0a-decision-tip HEAD ''
test "$(git rev-parse refs/ai-ip/phase-0a-decision-tip)" = "$(git rev-parse HEAD)"
git rev-parse refs/ai-ip/phase-0a-decision-tip
```

Expected: exit `0`; the final line is one frozen 40-character lowercase decision-tip SHA newer than or equal to planning base `58baf3b21da4768d5f608264c3224660338ef517`. If any file is absent, the tree is dirty, or the import ref already exists, stop and inspect rather than silently moving the ref.

- [ ] **Step 2：配置只读语义的官方 upstream remote**

```bash
set -euo pipefail
if git remote get-url upstream >/dev/null 2>&1; then
  test "$(git remote get-url --all upstream)" = "https://github.com/openai/codex.git"
else
  git remote add upstream https://github.com/openai/codex.git
fi
git remote set-url --push upstream disabled://openai-codex-upstream-read-only
test "$(git remote get-url --all upstream)" = "https://github.com/openai/codex.git"
test "$(git remote get-url --push --all upstream)" = "disabled://openai-codex-upstream-read-only"
if git config --get-regexp '^remote\.origin\.' >/dev/null 2>&1; then
  printf '%s\n' 'FAIL: product origin must remain unconfigured during Phase 0A.1' >&2
  exit 1
fi
```

Expected: exit `0`; `upstream` has exactly one fetch URL (the official HTTPS repository) and one deliberately unusable `disabled://` push URL, and no `remote.origin.*` configuration exists.

- [ ] **Step 3：抓取锁定提交并验证完整对象闭包**

```bash
set -euo pipefail
ai_ip_upstream_sha="4ef1d4b89bd419c976b04fefa0fd36844e898340"
git fetch --no-tags --no-filter upstream "$ai_ip_upstream_sha"
test "$(git cat-file -t "$ai_ip_upstream_sha")" = "commit"
test "$(git rev-parse --is-shallow-repository)" = "false"
test "$(git config --bool --get remote.upstream.promisor 2>/dev/null || printf false)" = "false"
test -z "$(git config --get remote.upstream.partialclonefilter 2>/dev/null || true)"
test -z "$(git config --get extensions.partialclone 2>/dev/null || true)"
test -z "$(git rev-list --objects --missing=print "$ai_ip_upstream_sha" | sed -n 's/^?//p')"
```

Expected: exit `0`; the object is a commit, the repository is not shallow or partial, and `rev-list` reports no missing object.

- [ ] **Step 4：确认目标 worktree 和分支尚不存在**

```bash
set -euo pipefail
ai_ip_fork_worktree="$(cd .. && pwd)/ai-ip-phase-0a"
test ! -e "$ai_ip_fork_worktree"
if git show-ref --verify --quiet refs/heads/codex/phase-0a-source-import; then
  printf '%s\n' 'FAIL: branch refs/heads/codex/phase-0a-source-import already exists' >&2
  exit 1
fi
```

Expected: exit `0`. An existing path or branch is an ambiguous prior attempt and must be inspected manually; this plan never deletes it.

- [ ] **Step 5：从锁定上游创建实现 worktree**

```bash
set -euo pipefail
ai_ip_fork_worktree="$(cd .. && pwd)/ai-ip-phase-0a"
git worktree add "$ai_ip_fork_worktree" -b codex/phase-0a-source-import 4ef1d4b89bd419c976b04fefa0fd36844e898340
test "$(git -C "$ai_ip_fork_worktree" rev-parse HEAD)" = "4ef1d4b89bd419c976b04fefa0fd36844e898340"
test -z "$(git -C "$ai_ip_fork_worktree" status --porcelain=v1 --untracked-files=all)"
```

Expected: exit `0`; `../ai-ip-phase-0a` is a clean worktree whose branch starts exactly at the pinned upstream SHA.

- [ ] **Step 6：用 merge commit 接入已审阅决策 tip**

```bash
set -euo pipefail
ai_ip_fork_worktree="$(cd .. && pwd)/ai-ip-phase-0a"
ai_ip_decision_tip="$(git rev-parse refs/ai-ip/phase-0a-decision-tip)"
git -C "$ai_ip_fork_worktree" merge \
  --no-ff \
  --allow-unrelated-histories \
  "$ai_ip_decision_tip" \
  -m "chore: establish Codex-based AI IP fork"
test "$(git -C "$ai_ip_fork_worktree" show -s --format='%P' HEAD)" = "4ef1d4b89bd419c976b04fefa0fd36844e898340 $ai_ip_decision_tip"
git -C "$ai_ip_fork_worktree" merge-base --is-ancestor 4ef1d4b89bd419c976b04fefa0fd36844e898340 HEAD
git -C "$ai_ip_fork_worktree" merge-base --is-ancestor "$ai_ip_decision_tip" HEAD
test -z "$(git -C "$ai_ip_fork_worktree" status --porcelain=v1 --untracked-files=all)"
git update-ref -d refs/ai-ip/phase-0a-decision-tip "$ai_ip_decision_tip"
if git show-ref --verify --quiet refs/ai-ip/phase-0a-decision-tip; then exit 1; fi
```

Expected: exit `0`; the new HEAD is the committed source-import boundary with pinned Codex as first parent and the exact frozen decision tip as second parent. The temporary import ref is deleted only after all merge checks pass; if an earlier command fails it remains as the recovery anchor.

---

### Task 2：以 TDD 增加来源锁机器校验器

**Files:**

- Create: `scripts/ai_ip/foundation/test_verify_upstream_lock.py`
- Create: `scripts/ai_ip/foundation/verify_upstream_lock.py`

- [ ] **Step 1：切换到实现 worktree 并创建 verifier 目录**

```bash
set -euo pipefail
cd "$(cd .. && pwd)/ai-ip-phase-0a"
test "$(git branch --show-current)" = "codex/phase-0a-source-import"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
cat AGENTS.md
mkdir -p scripts/ai_ip/foundation
```

Expected: exit `0`; the executor has read the complete imported root instructions before acting, and only an empty directory structure is created, so Git remains clean.

- [ ] **Step 2：写完整的 verifier 失败测试**

Apply this patch from the implementation worktree root:

```bash
apply_patch <<'PATCH'
*** Begin Patch
*** Add File: scripts/ai_ip/foundation/test_verify_upstream_lock.py
+import os
+from pathlib import Path
+import subprocess
+import sys
+
+import pytest
+
+sys.path.insert(0, str(Path(__file__).resolve().parent))
+import verify_upstream_lock as verifier
+
+
+GIT_ENV = {
+    "GIT_AUTHOR_NAME": "AI IP Test",
+    "GIT_AUTHOR_EMAIL": "ai-ip-test@example.invalid",
+    "GIT_COMMITTER_NAME": "AI IP Test",
+    "GIT_COMMITTER_EMAIL": "ai-ip-test@example.invalid",
+}
+
+
+def _git(repo: Path, *args: str) -> str:
+    completed = subprocess.run(
+        ["git", "-C", str(repo), *args],
+        check=False,
+        capture_output=True,
+        text=True,
+        env={**os.environ, **GIT_ENV},
+    )
+    if completed.returncode != 0:
+        raise AssertionError(
+            f"git {' '.join(args)} failed ({completed.returncode}): "
+            f"{completed.stderr.strip()}"
+        )
+    return completed.stdout.strip()
+
+
+def _write(path: Path, content: str) -> None:
+    path.parent.mkdir(parents=True, exist_ok=True)
+    path.write_text(content, encoding="utf-8", newline="\n")
+
+
+def _commit(repo: Path, message: str) -> str:
+    _git(repo, "add", "-A")
+    _git(repo, "commit", "-m", message)
+    return _git(repo, "rev-parse", "HEAD")
+
+
+def _replace_once(path: Path, old: str, new: str) -> None:
+    content = path.read_text(encoding="utf-8")
+    assert content.count(old) == 1
+    path.write_text(content.replace(old, new, 1), encoding="utf-8", newline="\n")
+
+
+def _valid_repository(
+    tmp_path: Path,
+    monkeypatch: pytest.MonkeyPatch,
+    *,
+    domestic_model_approved: bool = True,
+) -> tuple[Path, str, str, str]:
+    repo = tmp_path / "fork"
+    repo.mkdir()
+    _git(repo, "init", "-q")
+    _git(repo, "config", "user.name", "AI IP Test")
+    _git(repo, "config", "user.email", "ai-ip-test@example.invalid")
+
+    upstream_notice = "OpenAI Codex\nCopyright 2025 OpenAI\n"
+    upstream_attributes = (
+        "codex-rs/app-server-protocol/schema/** linguist-generated\n"
+        "codex-rs/hooks/schema/generated/** linguist-generated\n"
+    )
+    _write(repo / "LICENSE", "Apache License\nVersion 2.0\n")
+    _write(repo / "NOTICE", upstream_notice)
+    _write(repo / ".gitattributes", upstream_attributes)
+    _write(repo / "AGENTS.md", "# Upstream rules\n")
+    upstream_sha = _commit(repo, "upstream")
+
+    _git(repo, "switch", "--orphan", "decisions")
+    _git(repo, "rm", "-rf", "--ignore-unmatch", ".")
+    _write(repo / "docs/decision.md", "planning base\n")
+    planning_base_sha = _commit(repo, "planning base")
+    _write(repo / "docs/decision.md", "planning base\ndecision tip\n")
+    for relative_path in verifier.REQUIRED_DECISION_PATHS:
+        content = f"decision input: {relative_path}\n"
+        if relative_path == verifier.DOMESTIC_MODEL_DECISION_PATH:
+            if domestic_model_approved:
+                content += f"{verifier.DOMESTIC_MODEL_APPROVED_STATUS_LINE}\n"
+                content += f"{verifier.DOMESTIC_MODEL_APPROVAL_MARKER}\n"
+                content += "## 10. 书面验收清单\n"
+                content += "\n".join(
+                    f"- [x] approved decision item {index}" for index in range(1, 10)
+                )
+                content += "\n## 11. 锁定源码依据\n"
+            else:
+                content += "- **状态：** 架构 v1.5 候选；等待书面复核\n"
+                content += f"{verifier.DOMESTIC_MODEL_APPROVAL_MARKER}\n"
+                content += f"{verifier.DOMESTIC_MODEL_PENDING_MARKER}\n"
+                content += "## 10. 书面验收清单\n"
+                content += "\n".join(
+                    f"- [ ] pending decision item {index}" for index in range(1, 10)
+                )
+                content += "\n## 11. 锁定源码依据\n"
+        _write(repo / relative_path, content)
+    decision_tip_sha = _commit(repo, "decision tip")
+
+    _git(repo, "switch", "-c", "product", upstream_sha)
+    _git(
+        repo,
+        "merge",
+        "--no-ff",
+        "--allow-unrelated-histories",
+        decision_tip_sha,
+        "-m",
+        "source import",
+    )
+    source_import_sha = _git(repo, "rev-parse", "HEAD")
+    _git(repo, "remote", "add", "upstream", verifier.OFFICIAL_UPSTREAM_REPOSITORY)
+    _git(
+        repo,
+        "remote",
+        "set-url",
+        "--push",
+        "upstream",
+        verifier.DISABLED_UPSTREAM_PUSH_URL,
+    )
+
+    monkeypatch.setattr(verifier, "PINNED_UPSTREAM_SHA", upstream_sha)
+    monkeypatch.setattr(verifier, "PLANNING_BASE_SHA", planning_base_sha)
+
+    lock = f'''[openai_codex]
+repository = "{verifier.OFFICIAL_UPSTREAM_REPOSITORY}"
+sha = "{upstream_sha}"
+integration = "git-merge-ancestor"
+license = "Apache-2.0"
+planning_base = "{planning_base_sha}"
+decision_tip = "{decision_tip_sha}"
+source_import = "{source_import_sha}"
+
+[product]
+name = "AI IP 1.1"
+runtime = "codex-app-server"
+deerflow_dependency = false
+origin_status = "unconfigured"
+'''
+    _write(repo / ".ai-ip/upstream.lock.toml", lock)
+    _write(repo / "docs/AGENTS.override.md", verifier.DOCS_OVERRIDE_TEXT)
+    _write(repo / ".ai-ip/AGENTS.override.md", verifier.AI_IP_OVERRIDE_TEXT)
+    _write(
+        repo / ".gitattributes",
+        upstream_attributes + verifier.ATTRIBUTES_SUFFIX_TEXT,
+    )
+    _write(repo / "NOTICE", upstream_notice + verifier.NOTICE_SUFFIX_TEXT)
+    _write(
+        repo / "MODIFICATIONS.md",
+        "\n".join(verifier.modifications_required_markers()) + "\n",
+    )
+    _write(
+        repo / "docs/architecture/codex-fork-patch-ledger.md",
+        "\n".join(verifier.ledger_required_markers()) + "\n",
+    )
+    _commit(repo, "governance")
+    return repo, upstream_sha, planning_base_sha, decision_tip_sha
+
+
+def test_valid_fork_passes(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
+    repo, upstream_sha, _, decision_tip_sha = _valid_repository(tmp_path, monkeypatch)
+
+    result = verifier.verify_repository(repo)
+
+    assert result == {
+        "decisionTip": decision_tip_sha,
+        "head": _git(repo, "rev-parse", "HEAD"),
+        "originStatus": "unconfigured",
+        "upstreamSha": upstream_sha,
+    }
+
+
+def test_dirty_tree_is_rejected(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
+    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
+    _write(repo / "untracked.txt", "dirty\n")
+
+    with pytest.raises(verifier.VerificationError, match="working tree is not clean"):
+        verifier.verify_repository(repo)
+
+
+def test_required_path_rejects_git_symlink_mode(
+    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
+) -> None:
+    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
+    link_payload = tmp_path / "license-link-payload"
+    _write(link_payload, "outside-license")
+    link_blob = _git(repo, "hash-object", "-w", str(link_payload))
+    _git(
+        repo,
+        "update-index",
+        "--add",
+        "--cacheinfo",
+        f"120000,{link_blob},LICENSE",
+    )
+
+    with pytest.raises(
+        verifier.VerificationError, match="required path must use regular Git mode 100644"
+    ):
+        verifier.verify_repository(repo, require_clean=False)
+
+
+def test_broken_root_override_symlink_cannot_look_absent(
+    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
+) -> None:
+    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
+    link_payload = tmp_path / "override-link-payload"
+    _write(link_payload, "missing-target")
+    link_blob = _git(repo, "hash-object", "-w", str(link_payload))
+    _git(
+        repo,
+        "update-index",
+        "--add",
+        "--cacheinfo",
+        f"120000,{link_blob},AGENTS.override.md",
+    )
+
+    with pytest.raises(
+        verifier.VerificationError, match="root AGENTS.override.md would shadow"
+    ):
+        verifier.verify_repository(repo, require_clean=False)
+
+
+def test_wrong_locked_upstream_sha_is_rejected(
+    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
+) -> None:
+    repo, upstream_sha, _, _ = _valid_repository(tmp_path, monkeypatch)
+    _replace_once(repo / ".ai-ip/upstream.lock.toml", upstream_sha, "0" * 40)
+
+    with pytest.raises(verifier.VerificationError, match="openai_codex.sha"):
+        verifier.verify_repository(repo, require_clean=False)
+
+
+def test_unconfigured_origin_rejects_an_origin_remote(
+    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
+) -> None:
+    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
+    _git(repo, "remote", "add", "origin", "https://example.invalid/ai-ip.git")
+
+    with pytest.raises(
+        verifier.VerificationError,
+        match="origin_status is unconfigured but remote origin exists",
+    ):
+        verifier.verify_repository(repo)
+
+
+def test_unconfigured_origin_rejects_push_only_configuration(
+    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
+) -> None:
+    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
+    _git(repo, "config", "remote.origin.pushurl", "https://example.invalid/push.git")
+
+    with pytest.raises(
+        verifier.VerificationError,
+        match="origin_status is unconfigured but remote origin exists",
+    ):
+        verifier.verify_repository(repo)
+
+
+@pytest.mark.parametrize(
+    ("config_key", "error_match"),
+    (
+        ("remote.upstream.url", "upstream fetch URLs"),
+        ("remote.upstream.pushurl", "upstream push URLs"),
+    ),
+)
+def test_upstream_remote_rejects_additional_urls(
+    tmp_path: Path,
+    monkeypatch: pytest.MonkeyPatch,
+    config_key: str,
+    error_match: str,
+) -> None:
+    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
+    _git(repo, "config", "--add", config_key, "https://example.invalid/extra.git")
+
+    with pytest.raises(verifier.VerificationError, match=error_match):
+        verifier.verify_repository(repo)
+
+
+def test_notice_must_preserve_upstream_bytes_and_exact_suffix(
+    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
+) -> None:
+    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
+    _write(repo / "NOTICE", "replacement notice\n" + verifier.NOTICE_SUFFIX_TEXT)
+
+    with pytest.raises(verifier.VerificationError, match="NOTICE must equal"):
+        verifier.verify_repository(repo, require_clean=False)
+
+
+def test_required_git_attribute_cannot_be_removed(
+    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
+) -> None:
+    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
+    attributes = repo / ".gitattributes"
+    _replace_once(attributes, "MODULE.bazel.lock text eol=lf\n", "")
+
+    with pytest.raises(
+        verifier.VerificationError,
+        match=".gitattributes must equal pinned upstream bytes plus the exact AI IP suffix",
+    ):
+        verifier.verify_repository(repo, require_clean=False)
+
+
+def test_later_git_attribute_override_is_rejected(
+    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
+) -> None:
+    repo, _, _, _ = _valid_repository(tmp_path, monkeypatch)
+    attributes = repo / ".gitattributes"
+    _write(
+        attributes,
+        attributes.read_text(encoding="utf-8") + "MODULE.bazel.lock -text\n",
+    )
+
+    with pytest.raises(
+        verifier.VerificationError,
+        match=".gitattributes must equal pinned upstream bytes plus the exact AI IP suffix",
+    ):
+        verifier.verify_repository(repo, require_clean=False)
+
+
+def test_source_import_commit_must_bind_both_exact_parents(
+    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
+) -> None:
+    repo, upstream_sha, _, _ = _valid_repository(tmp_path, monkeypatch)
+    source_import_sha = _git(
+        repo, "rev-list", "--first-parent", "--merges", f"{upstream_sha}..HEAD"
+    ).splitlines()[0]
+    _replace_once(
+        repo / ".ai-ip/upstream.lock.toml",
+        f'source_import = "{source_import_sha}"',
+        f'source_import = "{upstream_sha}"',
+    )
+
+    with pytest.raises(verifier.VerificationError, match="source-import parents"):
+        verifier.verify_repository(repo, require_clean=False)
+
+
+def test_decision_tip_must_contain_all_reviewed_inputs(
+    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
+) -> None:
+    repo, _, planning_base_sha, decision_tip_sha = _valid_repository(
+        tmp_path, monkeypatch
+    )
+    _replace_once(
+        repo / ".ai-ip/upstream.lock.toml",
+        f'decision_tip = "{decision_tip_sha}"',
+        f'decision_tip = "{planning_base_sha}"',
+    )
+
+    with pytest.raises(
+        verifier.VerificationError, match="decision tip missing required file"
+    ):
+        verifier.verify_repository(repo, require_clean=False)
+
+
+def test_domestic_model_decision_must_be_fully_approved(
+    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
+) -> None:
+    repo, _, _, _ = _valid_repository(
+        tmp_path, monkeypatch, domestic_model_approved=False
+    )
+
+    with pytest.raises(
+        verifier.VerificationError,
+        match="domestic-model decision is not fully approved for the decision tip",
+    ):
+        verifier.verify_repository(repo, require_clean=False)
+
+
+def test_decision_tip_must_descend_from_planning_base(
+    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
+) -> None:
+    repo, upstream_sha, _, decision_tip_sha = _valid_repository(tmp_path, monkeypatch)
+    _replace_once(
+        repo / ".ai-ip/upstream.lock.toml",
+        f'decision_tip = "{decision_tip_sha}"',
+        f'decision_tip = "{upstream_sha}"',
+    )
+
+    with pytest.raises(
+        verifier.VerificationError,
+        match="planning base is not an ancestor of decision tip",
+    ):
+        verifier.verify_repository(repo, require_clean=False)
+
+
+def test_placeholder_decision_tip_is_rejected(
+    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
+) -> None:
+    repo, _, _, decision_tip_sha = _valid_repository(tmp_path, monkeypatch)
+    _replace_once(
+        repo / ".ai-ip/upstream.lock.toml",
+        f'decision_tip = "{decision_tip_sha}"',
+        'decision_tip = "replace-with-decision-tip"',
+    )
+
+    with pytest.raises(verifier.VerificationError, match="decision_tip is a placeholder"):
+        verifier.verify_repository(repo, require_clean=False)
*** End Patch
PATCH
```

Expected: `scripts/ai_ip/foundation/test_verify_upstream_lock.py` exists with the complete 17-case suite above; no implementation file exists yet.

- [ ] **Step 3：运行测试并确认 RED 来自缺失实现**

```bash
PYTHONDONTWRITEBYTECODE=1 uv run --python 3.11 --with pytest==8.3.5 pytest -q -p no:cacheprovider scripts/ai_ip/foundation/test_verify_upstream_lock.py
```

Expected: exit `2`; collection fails with `ModuleNotFoundError: No module named 'verify_upstream_lock'`. A dependency-installation or Git error is not the required RED.

- [ ] **Step 4：实现完整、fail-closed 的来源锁 verifier**

Apply this patch from the implementation worktree root:

```bash
apply_patch <<'PATCH'
*** Begin Patch
*** Add File: scripts/ai_ip/foundation/verify_upstream_lock.py
+#!/usr/bin/env python3
+import argparse
+from pathlib import Path
+import re
+import subprocess
+import sys
+import tomllib
+from typing import Any
+
+
+OFFICIAL_UPSTREAM_REPOSITORY = "https://github.com/openai/codex.git"
+DISABLED_UPSTREAM_PUSH_URL = "disabled://openai-codex-upstream-read-only"
+PINNED_UPSTREAM_SHA = "4ef1d4b89bd419c976b04fefa0fd36844e898340"
+PLANNING_BASE_SHA = "58baf3b21da4768d5f608264c3224660338ef517"
+SHA_PATTERN = re.compile(r"[0-9a-f]{40}")
+
+DOCS_OVERRIDE_TEXT = """# AI IP fork documentation override
+
+The repository-root `AGENTS.md` remains in force.
+
+This fork may add product-owned provenance, architecture, evidence, specifications, and implementation plans only under `docs/architecture/`, `docs/evidence/`, and `docs/superpowers/`. This narrow exception does not authorize unrelated product documentation beside upstream Codex modules or weaken any code, test, formatting, or review rule inherited from the repository root.
+"""
+
+AI_IP_OVERRIDE_TEXT = """# AI IP machine-governance override
+
+The repository-root `AGENTS.md` remains in force.
+
+Files under `.ai-ip/` may contain only source locks and machine-readable fork governance. Never store credentials, provider tokens, private cases, prompt or response bodies, reviewer mappings, customer content, or generated media here. Source-lock changes must preserve exact Git ancestry and must be verified by `scripts/ai_ip/foundation/verify_upstream_lock.py`.
+"""
+
+NOTICE_SUFFIX_TEXT = """
+AI IP 1.1 modifications
+
+This distribution includes modifications to OpenAI Codex for the AI IP 1.1 product.
+Modification provenance is documented in MODIFICATIONS.md and docs/architecture/codex-fork-patch-ledger.md.
+"""
+
+ATTRIBUTES_SUFFIX_TEXT = """
+# AI IP evidence and reproducible lockfiles
+docs/evidence/**/*.stdout.log -text
+docs/evidence/**/*.stderr.log -text
+codex-rs/Cargo.lock text eol=lf
+MODULE.bazel.lock text eol=lf
+pnpm-lock.yaml text eol=lf
+"""
+
+REQUIRED_DECISION_PATHS = (
+    "docs/superpowers/specs/2026-08-24-ip-agent-saas-design.md",
+    "docs/superpowers/specs/2026-08-25-domestic-model-adaptation-design.md",
+    "docs/architecture/2026-08-25-legacy-six-version-decision-memory.md",
+    "docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md",
+    "docs/superpowers/specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md",
+    "docs/superpowers/plans/2026-08-25-01a-codex-fork-provenance.md",
+)
+DOMESTIC_MODEL_DECISION_PATH = (
+    "docs/superpowers/specs/2026-08-25-domestic-model-adaptation-design.md"
+)
+DOMESTIC_MODEL_APPROVAL_MARKER = (
+    "- **批准标记：** `APPROVED_FOR_DECISION_TIP`"
+)
+DOMESTIC_MODEL_PENDING_MARKER = (
+    "- **批准标记：** `PENDING_WRITTEN_REVIEW`"
+)
+DOMESTIC_MODEL_APPROVED_STATUS_LINE = (
+    "- **状态：** 架构 v1.5 已批准；用户已确认方案 B 的书面规格，允许进入 Phase 0A decision tip"
+)
+DOMESTIC_MODEL_ACCEPTANCE_ITEM_COUNT = 9
+
+REQUIRED_PATHS = (
+    ".ai-ip/AGENTS.override.md",
+    ".ai-ip/upstream.lock.toml",
+    ".gitattributes",
+    "AGENTS.md",
+    "LICENSE",
+    "MODIFICATIONS.md",
+    "NOTICE",
+    "docs/AGENTS.override.md",
+    "docs/architecture/codex-fork-patch-ledger.md",
+)
+
+
+class VerificationError(RuntimeError):
+    pass
+
+
+def modifications_required_markers() -> tuple[str, ...]:
+    return (
+        "# AI IP 1.1 modifications to OpenAI Codex",
+        f"Locked upstream commit: `{PINNED_UPSTREAM_SHA}`.",
+        "Product origin status at this checkpoint: `unconfigured`.",
+        "DeerFlow is not a runtime dependency or fallback.",
+        "All fork-specific patches are recorded in `docs/architecture/codex-fork-patch-ledger.md`.",
+    )
+
+
+def ledger_required_markers() -> tuple[str, ...]:
+    return (
+        "# Codex fork patch ledger",
+        "## F-0001 — Establish source provenance",
+        f"- Upstream base: `{PINNED_UPSTREAM_SHA}`.",
+        "- Product origin status: `unconfigured`.",
+        "- Rollback: remove only the AI IP provenance commit after first returning to the source-import merge commit; never rewrite or discard either imported ancestor.",
+    )
+
+
+def _run_git(repo: Path, *args: str) -> subprocess.CompletedProcess[bytes]:
+    return subprocess.run(
+        ["git", "-C", str(repo), *args],
+        check=False,
+        capture_output=True,
+    )
+
+
+def _git_bytes(repo: Path, *args: str) -> bytes:
+    completed = _run_git(repo, *args)
+    if completed.returncode != 0:
+        detail = completed.stderr.decode("utf-8", errors="replace").strip()
+        raise VerificationError(
+            f"git {' '.join(args)} failed ({completed.returncode}): {detail}"
+        )
+    return completed.stdout
+
+
+def _git_text(repo: Path, *args: str) -> str:
+    return _git_bytes(repo, *args).decode("utf-8").strip()
+
+
+def _optional_git_text(repo: Path, *args: str) -> str | None:
+    completed = _run_git(repo, *args)
+    if completed.returncode == 0:
+        return completed.stdout.decode("utf-8").strip()
+    if completed.returncode in (1, 2):
+        return None
+    detail = completed.stderr.decode("utf-8", errors="replace").strip()
+    raise VerificationError(
+        f"git {' '.join(args)} failed ({completed.returncode}): {detail}"
+    )
+
+
+def _is_ancestor(repo: Path, ancestor: str, descendant: str) -> bool:
+    completed = _run_git(repo, "merge-base", "--is-ancestor", ancestor, descendant)
+    if completed.returncode == 0:
+        return True
+    if completed.returncode == 1:
+        return False
+    detail = completed.stderr.decode("utf-8", errors="replace").strip()
+    raise VerificationError(
+        f"git merge-base --is-ancestor failed ({completed.returncode}): {detail}"
+    )
+
+
+def _require(condition: bool, message: str) -> None:
+    if not condition:
+        raise VerificationError(message)
+
+
+def _read_lock(repo: Path) -> tuple[dict[str, Any], str]:
+    path = repo / ".ai-ip/upstream.lock.toml"
+    try:
+        raw = path.read_text(encoding="utf-8")
+        parsed = tomllib.loads(raw)
+    except (OSError, tomllib.TOMLDecodeError) as exc:
+        raise VerificationError(f"cannot read upstream lock: {exc}") from exc
+    _require(isinstance(parsed, dict), "upstream lock root must be a TOML table")
+    return parsed, raw
+
+
+def _exact_section(
+    lock: dict[str, Any], name: str, required_keys: set[str]
+) -> dict[str, Any]:
+    section = lock.get(name)
+    _require(isinstance(section, dict), f"missing TOML table [{name}]")
+    actual_keys = set(section)
+    _require(
+        actual_keys == required_keys,
+        f"[{name}] keys differ: expected {sorted(required_keys)}, got {sorted(actual_keys)}",
+    )
+    return section
+
+
+def _require_equal(actual: Any, expected: Any, label: str) -> None:
+    _require(actual == expected, f"{label}: expected {expected!r}, got {actual!r}")
+
+
+def _require_commit(repo: Path, sha: str, label: str) -> None:
+    _require(isinstance(sha, str), f"{label} must be a string")
+    _require(SHA_PATTERN.fullmatch(sha) is not None, f"{label} must be lowercase 40-hex")
+    _require_equal(_git_text(repo, "cat-file", "-t", sha), "commit", label)
+
+
+def _require_markers(path: Path, markers: tuple[str, ...]) -> None:
+    content = path.read_text(encoding="utf-8")
+    for marker in markers:
+        _require(marker in content, f"{path.name} missing marker: {marker}")
+
+
+def verify_repository(repo: Path, *, require_clean: bool = True) -> dict[str, str]:
+    repo = repo.resolve(strict=True)
+    top_level = Path(_git_text(repo, "rev-parse", "--show-toplevel")).resolve()
+    _require(top_level == repo, f"--repo must be the worktree root: {top_level}")
+    _require_equal(
+        _git_text(repo, "rev-parse", "--is-shallow-repository"),
+        "false",
+        "repository shallowness",
+    )
+    if require_clean:
+        status = _git_text(repo, "status", "--porcelain=v1", "--untracked-files=all")
+        _require(not status, f"working tree is not clean:\n{status}")
+
+    for relative_path in REQUIRED_PATHS:
+        required_path = repo / relative_path
+        _require(required_path.is_file(), f"missing required file: {relative_path}")
+        _require(
+            not required_path.is_symlink(),
+            f"required path must not be a symlink: {relative_path}",
+        )
+        index_entry = _git_text(repo, "ls-files", "--stage", "--", relative_path)
+        if index_entry:
+            index_lines = index_entry.splitlines()
+            _require(len(index_lines) == 1, f"ambiguous Git index entry: {relative_path}")
+            mode = index_lines[0].split(maxsplit=1)[0]
+            _require(
+                mode == "100644",
+                f"required path must use regular Git mode 100644: {relative_path}",
+            )
+    root_override = repo / "AGENTS.override.md"
+    tracked_root_override = _git_text(
+        repo, "ls-files", "--stage", "--", "AGENTS.override.md"
+    )
+    _require(
+        not root_override.exists()
+        and not root_override.is_symlink()
+        and not tracked_root_override,
+        "root AGENTS.override.md would shadow the imported upstream rules",
+    )
+
+    lock, lock_text = _read_lock(repo)
+    _require("replace-with" not in lock_text, "decision_tip is a placeholder")
+    _require(set(lock) == {"openai_codex", "product"}, "unexpected top-level lock table")
+    openai = _exact_section(
+        lock,
+        "openai_codex",
+        {
+            "decision_tip",
+            "integration",
+            "license",
+            "planning_base",
+            "repository",
+            "sha",
+            "source_import",
+        },
+    )
+    product = _exact_section(
+        lock,
+        "product",
+        {"deerflow_dependency", "name", "origin_status", "runtime"},
+    )
+
+    _require_equal(
+        openai["repository"], OFFICIAL_UPSTREAM_REPOSITORY, "openai_codex.repository"
+    )
+    _require_equal(openai["sha"], PINNED_UPSTREAM_SHA, "openai_codex.sha")
+    _require_equal(
+        openai["integration"], "git-merge-ancestor", "openai_codex.integration"
+    )
+    _require_equal(openai["license"], "Apache-2.0", "openai_codex.license")
+    _require_equal(
+        openai["planning_base"], PLANNING_BASE_SHA, "openai_codex.planning_base"
+    )
+    _require_equal(product["name"], "AI IP 1.1", "product.name")
+    _require_equal(product["runtime"], "codex-app-server", "product.runtime")
+    _require_equal(product["deerflow_dependency"], False, "product.deerflow_dependency")
+    _require_equal(product["origin_status"], "unconfigured", "product.origin_status")
+
+    upstream_sha = openai["sha"]
+    planning_base = openai["planning_base"]
+    decision_tip = openai["decision_tip"]
+    source_import = openai["source_import"]
+    _require(isinstance(decision_tip, str), "openai_codex.decision_tip must be a string")
+    _require_commit(repo, upstream_sha, "openai_codex.sha")
+    _require_commit(repo, planning_base, "openai_codex.planning_base")
+    _require_commit(repo, decision_tip, "openai_codex.decision_tip")
+    _require_commit(repo, source_import, "openai_codex.source_import")
+    head = _git_text(repo, "rev-parse", "HEAD")
+    _require_commit(repo, head, "HEAD")
+    _require(
+        _is_ancestor(repo, planning_base, decision_tip),
+        "planning base is not an ancestor of decision tip",
+    )
+    for relative_path in REQUIRED_DECISION_PATHS:
+        decision_object = _run_git(
+            repo, "cat-file", "-t", f"{decision_tip}:{relative_path}"
+        )
+        _require(
+            decision_object.returncode == 0 and decision_object.stdout.strip() == b"blob",
+            f"decision tip missing required file: {relative_path}",
+        )
+    domestic_model_decision = _git_bytes(
+        repo, "show", f"{decision_tip}:{DOMESTIC_MODEL_DECISION_PATH}"
+    ).decode("utf-8")
+    domestic_model_lines = domestic_model_decision.splitlines()
+    _require(
+        domestic_model_lines.count(DOMESTIC_MODEL_APPROVAL_MARKER) == 1,
+        "domestic-model decision is not fully approved for the decision tip",
+    )
+    status_lines = [
+        line for line in domestic_model_lines if line.startswith("- **状态：** ")
+    ]
+    _require(
+        status_lines == [DOMESTIC_MODEL_APPROVED_STATUS_LINE],
+        "domestic-model decision is not fully approved for the decision tip",
+    )
+    _require(
+        DOMESTIC_MODEL_PENDING_MARKER not in domestic_model_lines,
+        "domestic-model decision is not fully approved for the decision tip",
+    )
+    try:
+        checklist_start = domestic_model_lines.index("## 10. 书面验收清单")
+        checklist_end = domestic_model_lines.index("## 11. 锁定源码依据")
+    except ValueError as error:
+        raise VerificationError(
+            "domestic-model decision is not fully approved for the decision tip"
+        ) from error
+    checklist_lines = [
+        line
+        for line in domestic_model_lines[checklist_start + 1 : checklist_end]
+        if re.fullmatch(r"- \[(?: |x|X)\] .+", line)
+    ]
+    _require(
+        len(checklist_lines) == DOMESTIC_MODEL_ACCEPTANCE_ITEM_COUNT
+        and all(line.startswith("- [x] ") for line in checklist_lines),
+        "domestic-model decision is not fully approved for the decision tip",
+    )
+    source_import_parents = _git_text(
+        repo, "show", "-s", "--format=%P", source_import
+    ).split()
+    _require_equal(
+        source_import_parents,
+        [upstream_sha, decision_tip],
+        "source-import parents",
+    )
+    _require(
+        _is_ancestor(repo, source_import, head),
+        "source-import commit is not an ancestor of HEAD",
+    )
+    _require(_is_ancestor(repo, upstream_sha, head), "upstream SHA is not an ancestor of HEAD")
+    _require(_is_ancestor(repo, decision_tip, head), "decision tip is not an ancestor of HEAD")
+
+    missing_output = _git_text(
+        repo,
+        "rev-list",
+        "--objects",
+        "--missing=print",
+        upstream_sha,
+        decision_tip,
+        source_import,
+        head,
+    )
+    missing_objects = [line for line in missing_output.splitlines() if line.startswith("?")]
+    _require(not missing_objects, f"Git object closure is incomplete: {missing_objects}")
+    promisor = _optional_git_text(repo, "config", "--bool", "--get", "remote.upstream.promisor")
+    _require(promisor in (None, "false"), f"upstream remote is promisor-backed: {promisor}")
+    _require(
+        _optional_git_text(repo, "config", "--get", "remote.upstream.partialclonefilter")
+        is None,
+        "upstream remote has a partial-clone filter",
+    )
+    _require(
+        _optional_git_text(repo, "config", "--get", "extensions.partialclone") is None,
+        "repository has extensions.partialclone",
+    )
+
+    _require_equal(
+        _git_text(repo, "remote", "get-url", "--all", "upstream").splitlines(),
+        [OFFICIAL_UPSTREAM_REPOSITORY],
+        "upstream fetch URLs",
+    )
+    _require_equal(
+        _git_text(
+            repo, "remote", "get-url", "--push", "--all", "upstream"
+        ).splitlines(),
+        [DISABLED_UPSTREAM_PUSH_URL],
+        "upstream push URLs",
+    )
+    origin = _optional_git_text(repo, "config", "--get-regexp", r"^remote\.origin\.")
+    _require(
+        origin is None,
+        f"origin_status is unconfigured but remote origin exists or is configured: {origin}",
+    )
+
+    upstream_license = _git_bytes(repo, "show", f"{upstream_sha}:LICENSE")
+    _require(
+        (repo / "LICENSE").read_bytes() == upstream_license,
+        "LICENSE must remain byte-identical to pinned upstream",
+    )
+    upstream_agents = _git_bytes(repo, "show", f"{upstream_sha}:AGENTS.md")
+    _require(
+        (repo / "AGENTS.md").read_bytes() == upstream_agents,
+        "AGENTS.md must remain byte-identical to pinned upstream",
+    )
+    upstream_notice = _git_bytes(repo, "show", f"{upstream_sha}:NOTICE")
+    _require(
+        (repo / "NOTICE").read_bytes()
+        == upstream_notice + NOTICE_SUFFIX_TEXT.encode("utf-8"),
+        "NOTICE must equal pinned upstream bytes plus the exact AI IP suffix",
+    )
+    _require_equal(
+        (repo / "docs/AGENTS.override.md").read_text(encoding="utf-8"),
+        DOCS_OVERRIDE_TEXT,
+        "docs/AGENTS.override.md",
+    )
+    _require_equal(
+        (repo / ".ai-ip/AGENTS.override.md").read_text(encoding="utf-8"),
+        AI_IP_OVERRIDE_TEXT,
+        ".ai-ip/AGENTS.override.md",
+    )
+
+    upstream_attributes = _git_bytes(repo, "show", f"{upstream_sha}:.gitattributes")
+    _require(
+        (repo / ".gitattributes").read_bytes()
+        == upstream_attributes + ATTRIBUTES_SUFFIX_TEXT.encode("utf-8"),
+        ".gitattributes must equal pinned upstream bytes plus the exact AI IP suffix",
+    )
+
+    _require_markers(repo / "MODIFICATIONS.md", modifications_required_markers())
+    _require_markers(
+        repo / "docs/architecture/codex-fork-patch-ledger.md",
+        ledger_required_markers(),
+    )
+    return {
+        "decisionTip": decision_tip,
+        "head": head,
+        "originStatus": product["origin_status"],
+        "upstreamSha": upstream_sha,
+    }
+
+
+def main(argv: list[str] | None = None) -> int:
+    parser = argparse.ArgumentParser(description="Verify the AI IP Codex source lock")
+    parser.add_argument("--repo", required=True, type=Path)
+    parser.add_argument("--allow-dirty", action="store_true")
+    args = parser.parse_args(argv)
+    try:
+        result = verify_repository(args.repo, require_clean=not args.allow_dirty)
+    except (OSError, UnicodeError, VerificationError) as exc:
+        print(f"FAIL: {exc}", file=sys.stderr)
+        return 1
+    print(
+        "PASS: Codex upstream lock verified "
+        f"(upstream={result['upstreamSha']}, decision={result['decisionTip']}, "
+        f"head={result['head']}, origin={result['originStatus']})"
+    )
+    return 0
+
+
+if __name__ == "__main__":
+    raise SystemExit(main())
*** End Patch
PATCH
```

Expected: the verifier contains no non-stdlib runtime dependency and performs no network or write operation.

- [ ] **Step 5：运行窄测试并确认 GREEN**

```bash
PYTHONDONTWRITEBYTECODE=1 uv run --python 3.11 --with pytest==8.3.5 pytest -q -p no:cacheprovider scripts/ai_ip/foundation/test_verify_upstream_lock.py
```

Expected: exit `0` and `17 passed`.

- [ ] **Step 6：检查 diff 并提交 verifier 边界**

```bash
set -euo pipefail
(cd codex-rs && just fmt)
git diff --check
git add scripts/ai_ip/foundation/test_verify_upstream_lock.py scripts/ai_ip/foundation/verify_upstream_lock.py
git diff --cached --check
git commit -m "test: add Codex source lock verifier"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
```

Expected: exit `0`; commit contains exactly the verifier and its test, and the worktree is clean.

---

### Task 3：写入精确 fork 治理文件并封存来源边界

**Files:**

- Create: `docs/AGENTS.override.md`
- Create: `.ai-ip/AGENTS.override.md`
- Modify: `.gitattributes`
- Create: `.ai-ip/upstream.lock.toml`
- Create: `MODIFICATIONS.md`
- Modify: `NOTICE`
- Create: `docs/architecture/codex-fork-patch-ledger.md`
- Verify only: `LICENSE`
- Verify only: `AGENTS.md`

- [ ] **Step 1：创建 `.ai-ip` 目录**

```bash
mkdir -p .ai-ip
```

Expected: exit `0`; Git remains clean because an empty directory is not tracked.

- [ ] **Step 2：增加窄范围 docs 规则例外**

```bash
apply_patch <<'PATCH'
*** Begin Patch
*** Add File: docs/AGENTS.override.md
+# AI IP fork documentation override
+
+The repository-root `AGENTS.md` remains in force.
+
+This fork may add product-owned provenance, architecture, evidence, specifications, and implementation plans only under `docs/architecture/`, `docs/evidence/`, and `docs/superpowers/`. This narrow exception does not authorize unrelated product documentation beside upstream Codex modules or weaken any code, test, formatting, or review rule inherited from the repository root.
*** End Patch
PATCH
```

Expected: `docs/AGENTS.override.md` exactly matches `DOCS_OVERRIDE_TEXT` in the verifier; no root `AGENTS.override.md` is created.

- [ ] **Step 3：增加 `.ai-ip` 机器治理规则**

```bash
apply_patch <<'PATCH'
*** Begin Patch
*** Add File: .ai-ip/AGENTS.override.md
+# AI IP machine-governance override
+
+The repository-root `AGENTS.md` remains in force.
+
+Files under `.ai-ip/` may contain only source locks and machine-readable fork governance. Never store credentials, provider tokens, private cases, prompt or response bodies, reviewer mappings, customer content, or generated media here. Source-lock changes must preserve exact Git ancestry and must be verified by `scripts/ai_ip/foundation/verify_upstream_lock.py`.
*** End Patch
PATCH
```

Expected: `.ai-ip/AGENTS.override.md` exactly matches `AI_IP_OVERRIDE_TEXT` in the verifier.

- [ ] **Step 4：在上游现有 `.gitattributes` 后追加字节规则**

```bash
apply_patch <<'PATCH'
*** Begin Patch
*** Update File: .gitattributes
@@
 codex-rs/app-server-protocol/schema/** linguist-generated
 codex-rs/hooks/schema/generated/** linguist-generated
+
+# AI IP evidence and reproducible lockfiles
+docs/evidence/**/*.stdout.log -text
+docs/evidence/**/*.stderr.log -text
+codex-rs/Cargo.lock text eol=lf
+MODULE.bazel.lock text eol=lf
+pnpm-lock.yaml text eol=lf
*** End Patch
PATCH
```

Expected: the two pinned upstream `linguist-generated` lines remain byte-for-byte present; five AI IP rules are appended.

- [ ] **Step 5：用已提交决策分支的真实 SHA 写来源锁**

The heredoc is intentionally unquoted so the already validated 40-hex Git value is expanded before `apply_patch` writes the file; no placeholder is written.

```bash
set -euo pipefail
ai_ip_upstream_sha="4ef1d4b89bd419c976b04fefa0fd36844e898340"
ai_ip_source_import="$(git rev-list --first-parent --merges "$ai_ip_upstream_sha"..HEAD)"
test "$(printf '%s\n' "$ai_ip_source_import" | awk 'NF { count += NF } END { print count + 0 }')" = "1"
set -- $(git show -s --format='%P' "$ai_ip_source_import")
test "$#" = "2"
test "$1" = "$ai_ip_upstream_sha"
ai_ip_decision_tip="$2"
test "$(git cat-file -t "$ai_ip_decision_tip")" = "commit"
git merge-base --is-ancestor 58baf3b21da4768d5f608264c3224660338ef517 "$ai_ip_decision_tip"
apply_patch <<PATCH
*** Begin Patch
*** Add File: .ai-ip/upstream.lock.toml
+[openai_codex]
+repository = "https://github.com/openai/codex.git"
+sha = "4ef1d4b89bd419c976b04fefa0fd36844e898340"
+integration = "git-merge-ancestor"
+license = "Apache-2.0"
+planning_base = "58baf3b21da4768d5f608264c3224660338ef517"
+decision_tip = "$ai_ip_decision_tip"
+source_import = "$ai_ip_source_import"
+
+[product]
+name = "AI IP 1.1"
+runtime = "codex-app-server"
+deerflow_dependency = false
+origin_status = "unconfigured"
*** End Patch
PATCH
```

Expected: `.ai-ip/upstream.lock.toml` contains two exact tables, and `decision_tip` is read from the source-import merge's immutable second parent rather than a mutable branch name or author-time placeholder.

- [ ] **Step 6：写完整的 fork 修改说明**

```bash
apply_patch <<'PATCH'
*** Begin Patch
*** Add File: MODIFICATIONS.md
+# AI IP 1.1 modifications to OpenAI Codex
+
+AI IP 1.1 is a modified derivative of OpenAI Codex. The fork preserves the complete upstream Git ancestor and records each product-owned patch instead of treating a copied source snapshot as provenance.
+
+Locked upstream commit: `4ef1d4b89bd419c976b04fefa0fd36844e898340`.
+
+Product origin status at this checkpoint: `unconfigured`.
+
+DeerFlow is not a runtime dependency or fallback.
+
+## Source and license policy
+
+- The fetch remote named `upstream` points to `https://github.com/openai/codex.git`; its configured push URL is deliberately unusable.
+- `origin` remains absent until a real product fork repository is selected and verified. A local placeholder URL is not provenance.
+- The imported `LICENSE`, root `AGENTS.md`, and upstream portion of `NOTICE` remain intact.
+- This source-import checkpoint records source governance only. A distributable SBOM and artifact-specific third-party notices are generated later for each actual release candidate.
+
+## Patch policy
+
+Every fork-specific patch is classified as preserve, directly modify, replace, or add. Each entry states the business reason, upstream seam, tests, sync risk, and deletion or rollback path. Business capability may justify a deep fork, but undocumented edits to the execution substrate are not allowed.
+
+All fork-specific patches are recorded in `docs/architecture/codex-fork-patch-ledger.md`.
+
+## Current boundary
+
+Phase 0A.1 adds only provenance verification and governance files. It does not modify the Codex runtime, close G0, prove the native macOS or Windows baseline, prove business quality, or authorize Plan 02.
*** End Patch
PATCH
```

Expected: `MODIFICATIONS.md` explicitly distinguishes source provenance from later binary SBOM and business proof.

- [ ] **Step 7：在保留上游 NOTICE 后追加精确修改说明**

```bash
apply_patch <<'PATCH'
*** Begin Patch
*** Update File: NOTICE
@@
 Copyright (c) 2016-2022 Florian Dehau
 Copyright (c) 2023-2025 The Ratatui Developers
+
+AI IP 1.1 modifications
+
+This distribution includes modifications to OpenAI Codex for the AI IP 1.1 product.
+Modification provenance is documented in MODIFICATIONS.md and docs/architecture/codex-fork-patch-ledger.md.
*** End Patch
PATCH
```

Expected: the original upstream NOTICE bytes remain the complete prefix, followed by exactly the suffix frozen in `NOTICE_SUFFIX_TEXT`.

- [ ] **Step 8：建立首条 fork patch ledger 记录**

```bash
apply_patch <<'PATCH'
*** Begin Patch
*** Add File: docs/architecture/codex-fork-patch-ledger.md
+# Codex fork patch ledger
+
+This append-only ledger explains why AI IP 1.1 diverges from the locked OpenAI Codex ancestor. A patch is not accepted merely because it compiles: it must identify the upstream seam, business reason, regression proof, sync risk, and removal or rollback path.
+
+## Entry contract
+
+Each later entry contains:
+
+- stable entry ID and owning implementation-plan task;
+- upstream base and exact fork commit;
+- classification: **preserve**, **directly modify**, **replace**, or **add**;
+- touched files and externally visible contracts;
+- business result that requires the change;
+- focused and integration tests with evidence locations;
+- upstream-sync conflict surface;
+- deletion or rollback procedure.
+
+No entry may contain credentials, private cases, prompt or response bodies, reviewer mappings, or customer content.
+
+## F-0001 — Establish source provenance
+
+- Owner: Phase 0A.1 Codex Fork Provenance.
+- Upstream base: `4ef1d4b89bd419c976b04fefa0fd36844e898340`.
+- Product origin status: `unconfigured`.
+- Classification: **preserve** upstream Git ancestry, `LICENSE`, root `AGENTS.md`, and upstream NOTICE; **modify** `.gitattributes` and `NOTICE`; **add** source-lock verification and fork governance.
+- Touched seams: repository metadata and documentation only; `codex-rs/` is unchanged.
+- Business reason: permit a deep business-first Codex fork without losing reproducible source attribution or silently falling back to DeerFlow.
+- Regression proof: `scripts/ai_ip/foundation/test_verify_upstream_lock.py` and a clean-tree run of `scripts/ai_ip/foundation/verify_upstream_lock.py`.
+- Upstream-sync risk: later upstream changes to `LICENSE`, `NOTICE`, root `AGENTS.md`, or `.gitattributes` require an explicit lock update and ledger entry; the verifier fails closed on silent drift.
+- Rollback: remove only the AI IP provenance commit after first returning to the source-import merge commit; never rewrite or discard either imported ancestor.
*** End Patch
PATCH
```

Expected: the ledger has one complete, reviewable entry and no runtime patch claim.

- [ ] **Step 9：运行 verifier 的 dirty-tree 模式检查待提交治理文件**

```bash
python3 scripts/ai_ip/foundation/verify_upstream_lock.py --repo "$PWD" --allow-dirty
```

Expected: exit `0` and one line beginning `PASS: Codex upstream lock verified`, ending with `origin=unconfigured)`.

- [ ] **Step 10：验证 `.gitattributes` 的实际 Git 语义**

```bash
set -euo pipefail
test "$(git check-attr linguist-generated -- codex-rs/app-server-protocol/schema/example.json | awk -F': ' '{print $3}')" = "set"
test "$(git check-attr linguist-generated -- codex-rs/hooks/schema/generated/example.json | awk -F': ' '{print $3}')" = "set"
test "$(git check-attr text -- docs/evidence/foundation/example.stdout.log | awk -F': ' '{print $3}')" = "unset"
test "$(git check-attr text -- docs/evidence/foundation/example.stderr.log | awk -F': ' '{print $3}')" = "unset"
test "$(git check-attr text -- codex-rs/Cargo.lock | awk -F': ' '{print $3}')" = "set"
test "$(git check-attr eol -- codex-rs/Cargo.lock | awk -F': ' '{print $3}')" = "lf"
test "$(git check-attr text -- MODULE.bazel.lock | awk -F': ' '{print $3}')" = "set"
test "$(git check-attr eol -- MODULE.bazel.lock | awk -F': ' '{print $3}')" = "lf"
test "$(git check-attr text -- pnpm-lock.yaml | awk -F': ' '{print $3}')" = "set"
test "$(git check-attr eol -- pnpm-lock.yaml | awk -F': ' '{print $3}')" = "lf"
```

Expected: exit `0`; both upstream generated rules and all five appended byte rules have the intended Git interpretation.

- [ ] **Step 11：重跑 verifier 单测并扫描治理文件中的占位符**

```bash
set -euo pipefail
PYTHONDONTWRITEBYTECODE=1 uv run --python 3.11 --with pytest==8.3.5 pytest -q -p no:cacheprovider scripts/ai_ip/foundation/test_verify_upstream_lock.py
if rg -n '(replace-with|TBD|TODO)' \
  .ai-ip/upstream.lock.toml \
  .ai-ip/AGENTS.override.md \
  docs/AGENTS.override.md \
  MODIFICATIONS.md \
  NOTICE \
  docs/architecture/codex-fork-patch-ledger.md; then
  exit 1
fi
git diff --check
```

Expected: exit `0`, `17 passed`, no `rg` match, and no whitespace error.

- [ ] **Step 12：提交来源治理边界**

```bash
set -euo pipefail
git add \
  docs/AGENTS.override.md \
  .ai-ip/AGENTS.override.md \
  .gitattributes \
  .ai-ip/upstream.lock.toml \
  MODIFICATIONS.md \
  NOTICE \
  docs/architecture/codex-fork-patch-ledger.md
git diff --cached --check
git commit -m "chore: lock Codex fork provenance"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
```

Expected: exit `0`; the governance commit contains exactly the seven listed paths and the worktree is clean.

- [ ] **Step 13：在干净提交上执行最终机器验证**

```bash
set -euo pipefail
python3 scripts/ai_ip/foundation/verify_upstream_lock.py --repo "$PWD"
PYTHONDONTWRITEBYTECODE=1 uv run --python 3.11 --with pytest==8.3.5 pytest -q -p no:cacheprovider scripts/ai_ip/foundation/test_verify_upstream_lock.py
test -z "$(git diff --name-only 4ef1d4b89bd419c976b04fefa0fd36844e898340..HEAD -- codex-rs)"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
```

Expected: verifier prints `PASS`, pytest prints `17 passed`, no path under `codex-rs/` differs from the pinned ancestor, and the worktree remains clean.

---

## 完成边界

本计划完成时只允许声称：Codex 上游和 AI IP 决策历史均为当前 fork 的可达祖先；来源、许可、NOTICE、上游 remote、未配置产品 origin 和治理文档已由测试与干净树 verifier 绑定。

本来源专用 child 不生成会对自身最终 commit 形成哈希环的独立 evidence manifest；`.ai-ip/upstream.lock.toml`、fork patch ledger、clean-tree verifier/pytest 输出和执行 handoff 共同构成证据记录。handoff 必须明确写 `providerMode=not-run`、`paidProviderCost=0`、所有已知失败或“无”，以及 ledger 中的 rollback 方法；任一未解决失败都不得写成本计划完成。

不得声称 G0 已关闭，因为源码依赖/许可/资产清单仍由后续 Phase 0A child plan 交付；不得声称 G1 原样跨平台基线、G2 真实业务证明或 `PASS_TO_PHASE_0B`。后续计划也不得改写本计划的 merge commit 或用复制源码替代其祖先关系。
