# AI IP implementation plans

当前执行依据：

- [Business-first cleanup 已批准范围](../specs/2026-09-05-business-first-cleanup-design.md)：本次唯一实施范围；清理任务完成后该计划也只作完成记录。
- [当前总路线图](2026-08-25-00-codex-ai-ip-master-roadmap.md)：业务优先的下一步与仍适用的职责边界。
- [主产品规格](../specs/2026-08-24-ip-agent-saas-design.md)、[国产模型方向](../specs/2026-08-25-domestic-model-adaptation-design.md)与[六版决策记忆](../../architecture/2026-08-25-legacy-six-version-decision-memory.md)：保留产品方向，涉及旧 proof 前置要求的内容由本次退役决定覆盖。

Phase 0A、06A、06B、07A、07B（含 LH1）及其 capture、attestation、private proof、Promptfoo、评分与盲评基础设施计划均为 `RETIRED_NOT_PASSED`。它们不再是内部业务开发的前置条件，也没有被判定为业务通过。旧 child 的“当前唯一可执行”“下一步”“必须先完成”文字仅保留为历史，不能据此重启任务。

来源导入 01a、ContentPackage 合同 02、Lead Skill/runtime 03 是保留实现的历史完成记录，不可重新执行。其他 evaluator-specific plans/specs 已在文首标为 RETIRED；DeerFlow/SaaS archive 继续不可执行。

下一步是围绕一个真实营销任务，用现有 Mission、Lead、ContentPackage 产出可直接拍摄或发布的草稿、必要制作说明和待确认事实，再按具体缺口做小改动。无需先恢复旧评测设施或新建替代门禁。这项清理本身不实现该业务切片、不调用模型、不授权支出、发布、客户数据操作或客户版本上线。
