# AI IP 1.1 implementation plans

当前权威文件：

- `2026-08-25-00-codex-ai-ip-master-roadmap.md`：总导航、依赖与门禁；不可直接执行。
- `../specs/2026-08-25-domestic-model-adaptation-design.md`：已获用户批准的国产模型 provider-neutral 能力合同、火山托管 GLM 路线与非阻塞第二供应商 onboarding。
- `../specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md`：Phase 0A 构建与验收合同；不可直接执行。
- `2026-08-25-01a-codex-fork-provenance.md`：已完成 provenance boundary。
- `2026-08-25-01b-codex-native-evidence-macos-baseline.md`：部分执行；最终 macOS baseline handoff 仍未解决。
- `2026-08-26-01c-evidence-filesystem-race-hardening.md`：已完成其 filesystem-race repair boundary。
- `2026-08-26-01d-nextest-selection-loopback-recapture.md`：Task 3 以 `exit 1` / `INVALID_FORBIDDEN_CONTENT` 停止并作为 invalid capture attempt 关闭；未产出 baseline PASS/BLOCKED summary，也未运行 Task 4 或 F-0003 handoff。
- `2026-08-26-02-content-package-contract.md`：以其 documented business-priority exception 下的 pure-domain contract 完成；不弥补或关闭 baseline gap。
- 上述任一 disposition 都不关闭 G0–G2、不授权 provider call 或产品 Plan 02。
- `2026-08-26-03-native-lead-skill-runtime.md`：Work Package 4 已完成其机械范围与 fresh scoped review。Cycle 1–7 为 `providerMode=not-run` 的 diagnostic RED，不能充当事实保真业务验收、live-model 质量或 `PASS_TO_PHASE_0B`。
- `2026-08-27-04-native-atomic-paired-evaluator.md`：已完成 Work Package 5 的 proxy、typed evaluator/collector、App Server characterization 与 atomic paired runner 机械边界；只产生 replay/mock-upstream 证明。
- `2026-08-27-05-paired-evaluator-semantic-closure.md`：已完成 Plan 04 的五项语义闭环；最终 evaluator `91/91`、proxy `22/22`、两个 Bazel 目标通过，fresh scoped review 无 Critical/Important。该结果仍为 `providerMode=not-run`，不构成 live/G2 业务验收。
- `2026-08-28-06a-blind-review-and-score.md`：当前唯一可执行 child；实现 Work Package 6 的第一个独立边界——冻结 rubric/schema、重验双臂、三人独立盲评包与确定性私有评分。它不实现成本/公开报告/retention，不运行 provider，也不关闭 G0–G2。

实施人员必须先读取：

1. `../specs/2026-08-24-ip-agent-saas-design.md`
2. `../specs/2026-08-25-domestic-model-adaptation-design.md`
3. `../../architecture/2026-08-25-legacy-six-version-decision-memory.md`
4. `2026-08-25-00-codex-ai-ip-master-roadmap.md`
5. `../specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md`
6. 前序 children 的来源锁、台账、验收或停止记录（01a complete；01b handoff unresolved；01c repair complete；01d invalid capture closed without summary/F-0003；02 pure-domain exception complete）
7. `2026-08-26-03-native-lead-skill-runtime.md` 及其 Cycle 1–7 diagnostic RED、最终机械验证和复审证据
8. `2026-08-27-04-native-atomic-paired-evaluator.md` 与其语义闭环 child `2026-08-27-05-paired-evaluator-semantic-closure.md`
9. 当前 child `2026-08-28-06a-blind-review-and-score.md`

第 9 项仅可在其既有 plan 的限制内执行 Work Package 6A；本 child 只实现 provider-free 的盲评与评分合同，不运行真实 provider、不读取 API key。Work Package 6B/6C、真实双臂执行、`PASS_TO_PHASE_0B` 与产品 Plan 02 保持 blocked。构建规格中的其他 Work Package 不能直接执行。

`archive/2026-08-24-cloud-saas-v1.3/` 中的文件基于已废止架构，只能作为历史证据，禁止执行。
