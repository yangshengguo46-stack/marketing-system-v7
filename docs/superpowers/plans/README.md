# AI IP 1.1 implementation plans

当前权威文件：

- `2026-08-25-00-codex-ai-ip-master-roadmap.md`：总导航、依赖与门禁；不可直接执行。
- `../specs/2026-08-25-domestic-model-adaptation-design.md`：已获用户批准的国产模型 provider-neutral 能力合同、火山托管 GLM 路线与非阻塞第二供应商 onboarding。
- `../specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md`：Phase 0A 构建与验收合同；不可直接执行。
- `2026-08-25-01a-codex-fork-provenance.md`：已完成的来源导入与 fork provenance child plan。
- `2026-08-25-01b-codex-native-evidence-macos-baseline.md`：唯一达到 execution depth、当前可执行的证据工具与原生 macOS x86_64 baseline child plan。

实施人员必须先读取：

1. `../specs/2026-08-24-ip-agent-saas-design.md`
2. `../specs/2026-08-25-domestic-model-adaptation-design.md`
3. `../../architecture/2026-08-25-legacy-six-version-decision-memory.md`
4. `2026-08-25-00-codex-ai-ip-master-roadmap.md`
5. `../specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md`
6. 已完成 child plan `2026-08-25-01a-codex-fork-provenance.md` 的来源锁、台账与验收记录
7. 当前达到 execution depth、可从干净 Phase 0A.1 tip 执行的 child plan `2026-08-25-01b-codex-native-evidence-macos-baseline.md`

只有第 7 项可以交给 `superpowers:subagent-driven-development` 或 `superpowers:executing-plans`。它不改 Codex runtime、不调用 provider、不读取 API key；Windows 11 x64 baseline 由后续独立 child 使用同一 tools SHA 完成。构建规格中的 Work Package 不能直接执行。

`archive/2026-08-24-cloud-saas-v1.3/` 中的文件基于已废止架构，只能作为历史证据，禁止执行。
