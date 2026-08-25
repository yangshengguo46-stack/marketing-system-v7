# AI IP 1.1 implementation plans

当前权威文件：

- `2026-08-25-00-codex-ai-ip-master-roadmap.md`：总导航、依赖与门禁；不可直接执行。
- `../specs/2026-08-25-domestic-model-adaptation-design.md`：国产模型 provider-neutral 能力合同与非阻塞第二供应商 onboarding；当前等待用户书面复核。
- `../specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md`：Phase 0A 构建与验收合同；不可直接执行。
- `2026-08-25-01a-codex-fork-provenance.md`：唯一达到 execution depth、当前等待书面批准而 gated 的 child plan。

实施人员必须先读取：

1. `../specs/2026-08-24-ip-agent-saas-design.md`
2. `../specs/2026-08-25-domestic-model-adaptation-design.md`
3. `../../architecture/2026-08-25-legacy-six-version-decision-memory.md`
4. `2026-08-25-00-codex-ai-ip-master-roadmap.md`
5. `../specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md`
6. 当前达到 execution depth、但仍 gated 的 child plan `2026-08-25-01a-codex-fork-provenance.md`

只有第 6 项在解除 gate 后可以交给 `superpowers:subagent-driven-development` 或 `superpowers:executing-plans`；解除条件是第 2 项完成用户书面复核并进入干净 decision tip。构建规格中的 Work Package 不能直接执行。

`archive/2026-08-24-cloud-saas-v1.3/` 中的文件基于已废止架构，只能作为历史证据，禁止执行。
