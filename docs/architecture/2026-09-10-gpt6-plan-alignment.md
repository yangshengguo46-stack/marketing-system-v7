# GPT-6 方案 × 第七版现状对齐

- 日期：2026-09-10
- 来源：用户 2026-09-09 与 GPT-6 的讨论（分享链接 `chatgpt.com/s/t_6aa2440f35b08191993f67149fae02e7`，两份文档：先一版被用户纠正，后一版为最终稿）
- 状态：**对齐分析，不构成执行授权**。本文件不启动任何计划、不授权任何改动、不判定任何门禁。
- 用途：把 GPT-6 的产品定义与架构建议逐条对照第七版已有资产，标出「已实现 / 部分实现 / 冲突 / 新增」，供用户裁决，避免把一份设计文档当成实施计划直接开工。

## 0. 边界说明

GPT-6 的讨论没有第七版上下文，是按"从零建一个产品"写的。因此它读起来像替代方案，实际包含三类内容：

1. 第七版**已经做过并且已由护栏固化**的判断（占多数）；
2. 第七版**已有对应资产的**命名差异（同一件事两个名字）；
3. **真正的新增项**，以及**与第七版现有实现直接冲突**的少数几条。

本文件只做映射，不下结论。第 4 节列出必须由用户裁决的冲突。

## 1. 对齐基准：第七版现有资产

对齐时的实测事实（2026-09-10）：

| 资产 | 位置 | 规模 / 状态 |
|---|---|---|
| Fork 锁定 | `.ai-ip/upstream.lock.toml` | `4ef1d4b89bd419c976b04fefa0fd36844e898340`（2026-08-24），**此后未与上游同步** |
| **Codex Core** | `codex-rs/core/` | **改动文件数 = 0** |
| 非 ai-ip 上游改动 | `codex-rs/` | 仅 13 个文件（`Cargo.toml/lock`、`app-server/*` 6 个、`codex-api/src/sse/responses.rs` 及测试） |
| 业务领域 crate | `codex-rs/ai-ip-domain` | 516 行生产代码 + 670 行测试 |
| 业务运行时 crate | `codex-rs/ai-ip-runtime` | 815 行生产代码 + 704 行测试 |
| Lead Skill | `ai-ip-assets/skills/deliver-ai-ip-content-package/` | 1 份 SKILL.md |
| 原生工具 | `ai-ip-runtime/src/work_tool.rs` | **1 个**：`marketing_work`（`open` / `record`） |
| 工作链 | `ai-ip-runtime/src/work_chain.rs` | 固定三阶段 `Research → Direction → Draft`，支持 `start_at` 跳阶与上游变更后的 `needs_review` |
| 根 Prompt | `ai-ip-runtime/src/prompt.rs` | 96 token；评测上下文上限 900 token |
| 已删除（2026-09-05） | `codex-rs/ai-ip-eval/`、`scripts/ai_ip/eval_lab/`、`ai-ip-evals/` | 整套评测/盲评/跑批设施，`RETIRED_NOT_PASSED` |
| 未构建 | 产品 UI / Chat / 本地存储 / 权限体系 | 规格存在（Plan 02–15），**从未开工**，已冻结 |

`ai-ip-domain` 已有类型：`HeldOutMissionCase`、`MissionMaterial`、`SubjectKind`、`MaterialKind`、`ContentPackage`、`InfluenceRelation`、`ActionFunnelStep`、`PublishableContent`、`Claim`、`ClaimStatus`、`MeasurementPlan`、`Readiness`、`ValidationErrors`。

`ClaimStatus` 六值：`UserFact` / `ExternalEvidence` / `ModelInterpretation` / `CreativeHypothesis` / `Unknown` / `ActualResult`。

### 1.1 底座实际能力盘点（2026-09-10 实测）

对齐第三条轴：GPT-6 要求"新增"或"改造"的东西，Codex 底座是不是已经有了。**每条都给了实际路径。**

| 能力 | 底座实现 | 位置 |
|---|---|---|
| 联网检索 + 打开网页 + **引用定位** | `web.run`：`search_query` / `open(ref_id, lineno)` / `find(pattern)` / `screenshot` | `ext/web-search/src/tool.rs:54` |
| 默认任务身份 | `thread/start.base_instructions` + `developer_instructions`（仅线程未运行时生效） | `app-server/src/request_processors/thread_processor.rs:215,1166,1226` |
| 每轮上下文注入 | `turn/start.additionalContext` + `turn/steer.additionalContext`；`kind` 分 `Untrusted`（→user 消息）/ `Application`（→developer 消息） | `app-server-protocol/.../v2/turn.rs:180,295` |
| 上下文贡献接缝 | `ContextContributor` 三方法：thread / turn / world_state | `ext/extension-api/src/contributors.rs:88` |
| **压缩免疫的上下文** | `WorldState`：每步重建，带 `PreviousWorldStateSection::Known` 做 diff | `core/src/session/world_state.rs` |
| 原生工具注册 | `ToolContributor::tools` / `tools_for_step` | `ext/extension-api/src/contributors.rs:315,322` |
| **任务级验收（不许停）** | Stop hook：`StopOutcome{should_block, should_stop, continuation_fragments}` | `core/src/session/turn.rs:623`；事件名 `hooks/src/lib.rs:23-36` |
| 目标 + token 预算 | `thread_goals` 表（objective/status/token_budget/tokens_used）+ `get_goal`/`create_goal`/`update_goal` | `ext/goal/` |
| 可持久化的线程附件 | `thread_attachments` 表（64KB/条、100 条/线程） | `state/migrations/0055_thread_attachments.sql` |
| 多档审批 | `AskForApproval`：untrusted / on-request / granular（5 子开关）/ never | `protocol/src/protocol.rs:992` |
| 可插拔审批人 | `ApprovalReviewContributor::decide → Allow / Reviewed / AskUser` | `ext/extension-api/.../approval_review.rs` |
| 执行策略引擎 | `execpolicy`：`Allow` / `Prompt` / `Forbidden` 声明式规则 + `host_executable` | `execpolicy/src/decision.rs` |
| 审批 RPC（4 条） | `item/commandExecution` / `item/fileChange` / `item/permissions` / `mcpServer/elicitation` | `app-server-protocol/src/protocol/common.rs:1741-1766` |
| 生物识别验证原语 | `UserVerificationProof{credential_id, signature}`（ECDSA P-256） | `app-server-protocol/.../v2/user_verification.rs` |
| 历史检索（官方压缩恢复手段） | `history.{list_windows,list_items,read_item,search_contents}` | `ext/history-notes/src/tools.rs:70` |
| 笔记虚拟文件系统 | `notes.{list_files_by_prefix,read_file,search_contents,append_to_file,write_file}` | 同上 |
| 沙箱 | `SandboxPolicy`：danger-full-access / read-only / external-sandbox / workspace-write | `protocol/src/protocol.rs:1078` |

**三个硬约束（盘点发现的，必须记住）：**

1. **`ExtensionData` 不落盘。** `Mutex<HashMap<TypeId, ErasedData>>`，文档明说 "does not provide persistence"。`thread_store` 是线程运行期缓存，线程重开即空；扩展必须在 `ThreadLifecycleContributor::on_thread_start` 自行重建。
2. **`contribute_turn_context` 的 `PromptSlot` 被忽略。** 该方法的返回值一律 push 进 `developer_sections`（`core/src/session/mod.rs:4101-4130`）；只有 `contribute_thread_context` 的槽位（`DeveloperPolicy` / `DeveloperCapabilities` / `ContextWindow`）才决定渲染位置。
3. **`additionalContext` 单值上限 1000 token 且中间截断。** `MAX_ADDITIONAL_CONTEXT_VALUE_TOKENS = 1_000`（`context-fragments/src/additional_context.rs:6,95`），用 `truncate_middle_with_token_budget`。任何"把完整正文塞进上下文"的设计都会在中间被砍断。

**对 GPT-6 十个原生工具的覆盖判定：**

| GPT-6 工具 | 底座覆盖 |
|---|---|
| `search_sources` | **已有等价物**：`web.run.search_query` |
| `open_source` | **已有等价物**：`web.run.open(ref_id, lineno)` —— 连引用锚点都有 |
| `edit_artifact` | 已有等价物（偏代码）：`apply_patch` + `notes.write_file` |
| `read_business_context` | 部分：`skills.list/read` + `memories.read/search` + `notes.read_file` |
| `read_work` | 部分：`history.*` + `notes.read_file` + `memories.read` + `get_goal` |
| `record_work` | 部分：`notes.write_file/append_to_file` + `memories.add_ad_hoc_note` + `create_goal/update_goal` |
| `analyze_dataset` | 部分：`exec`（V8 isolate，无 fs/network）太弱；`exec_command`（PTY）强但不受控 |
| `preview_action` | 部分：`request_user_input` / `request_permissions` / 审批 RPC |
| `commit_action` | 部分：`request_permissions`（授权）+ `exec_command`（执行）；**缺业务授权绑定、幂等、审计** |
| `read_results` | **完全没有** |

**结论：十个里两个已被完整覆盖（`search_sources`、`open_source`），一个接近覆盖（`edit_artifact`），六个部分覆盖，一个完全没有。** 真正需要自建的核心缺口是三个：`commit_action`（业务动作授权）、`analyze_dataset`（受控数据沙箱）、`read_results`（业务结果回读）。

### 1.2 与第七版既有计划的关系（第三条轴的另一半）

GPT-6 的主张里，哪些已经被第七版写过的计划覆盖——无论那些计划是冻结的、完成的、还是已删除的。

| GPT-6 主张 | 第七版对应 | 状态 |
|---|---|---|
| 一个 Lead 负责结果，不建固定多 Agent 编队 | LG1 + 产品规格 §5 | 已固化 |
| 内容世界 / 表现形式 / 商业承接分层 | LG3 | 已固化 |
| 事实、证据、假设、未知、结果分层 | LG4 + `ClaimStatus` 六值 | **已实现（代码）** |
| 原语是可选词汇，不是分类门槛 | LG7 + V5 教训 | 已固化 |
| 按需 Skill，不做固定流程 | LG9 + 9/5 删除的审计 bullet | 已固化 |
| 权限/预算/幂等/版本由代码执行 | LG6 | 已固化（原则） |
| 营销记忆（Mission / Artifact / Claim / Evidence） | 冻结的 **Plan 02**（Mission/Lead/Artifact 内核） | 冻结未开工 |
| 加密本地项目存储 | 冻结的 **Plan 05** | 冻结未开工 |
| 受控执行、授权绑定、对账 | 冻结的 **Plan 02 + 08** | 冻结未开工 |
| 发布与学习闭环 | 冻结的 **Plan 10** | 冻结未开工 |
| A/B/C 三臂对照（C = 本产品） | 已删除的 paired evaluator（4 个 plan） | **9/5 物理删除** |
| Chat 作为唯一主入口 | GPT-6 独有；第七版无对应计划 | **新增** |
| 产品自描述记录（已验证能力清单） | GPT-6 独有 | **新增** |

**三点结论：**

1. **GPT-6 的多数原则在第七版已经固化**，其中事实分层一条甚至已经落成代码（`ClaimStatus`）。
2. **它的"要做的事"里，有四项落在冻结的 Plan 02/05/08/10 范围内**——也就是说，这些不是新需求，是 9/5 冻结之后无处安放的旧需求。
3. **真正新的只有三条**：Chat 作为主入口（用户已判定为多余）、产品自描述记录、以及 9/5 删除的 C 臂对照。

**因此"以 GPT-6 方案为主"在计划层不产生新工作量，只产生一个重新排序：把冻结的 Plan 02/05 提到前面，把 Plan 10 和 C 臂对照复用 9/5 之前的思路但做小。**

## 2. 逐条对照

判定口径：**已实现** = 第七版有等价实现且语义一致；**护栏覆盖** = 没有实现，但六版决策记忆已把它写成禁止项；**部分** = 方向一致但覆盖不全；**冲突** = 与现有实现或既有治理直接矛盾；**新增** = 第七版没有对应物。

### GPT-6 最终稿（十四节）

| # | GPT-6 主张 | 第七版现状 | 判定 |
|---|---|---|---|
| 一 | 营销入口应从"商品属性/人口标签"推进到"具体的人、具体处境、具体事情" | LG3 第 1 层"确认业务主体、交易角色和受众" | 部分 |
| 二 | 保留：用户场景、事件因果、视角、原语、思维算子、信念变化、营销干预、状态记忆、反馈学习 | 无实现；LG2/LG3/LG7 已把这些写成原则 | 护栏覆盖 |
| 二 | 推翻 ①：不再要求所有任务走同一条十五步链路 | LG3 原文"下列是逻辑依赖，不是必须线性执行的流程" | 已实现（原则） |
| 二 | 推翻 ②：不再强制"必须有故事、必须有冲突、必须走远" | LG3"非叙事内容不强迫讲故事"（V4 教训） | 已实现（原则） |
| 二 | 推翻 ③：不再把原语当万能根 | LG7 五案例不得进入生产 Prompt；V5 语义地图过拟合教训 | 已实现（原则） |
| 二 | 推翻 ④：不再把通用模型等同于必然平庸 | 与 LG8 一致（能力声明需五级证据） | 已实现（原则） |
| 二 | 推翻 ⑤：不再把每种思考包成一个工具 | **2026-09-05 实际删除**了 Lead Skill 里五条重复审计 bullet 与整套评测工具 | 已实现（代码） |
| 三 | 产品定义：以 Chat 为主入口、基于 Codex 原生改造的营销 Agent | 产品规格 §5 业务流一致；产品面定位为本地优先桌面端 | 部分（定义一致，**无实现**） |
| 四 | Chat 必须是主体，承载交代目标/持续纠偏/审阅成果/授权行动 | 无 Chat。App Server 已有 `turn/steer`、`turn/interrupt` 可用 | 新增 |
| 五 | 营销概念作为共同语言（业务产品/角色/场景触发/动机/障碍/信念/事件证据/视角/营销动作/结果） | `ContentPackage` 字段覆盖其中一部分；LG3 六层是逻辑依赖 | 部分 |
| 六 | Universal Human Primitives 降级为可扩展动机词表 | 无实现；LG7 已禁止把它做成标准答案 | 护栏覆盖 |
| 七 | 创意从人/事件/产品机制/数据/已有内容多入口进入 | LG3"Lead 可依现有证据跳过、并行或回退" | 已实现（原则） |
| 八 | **自主主循环**：判断最缺什么 → 自选下一步 → 检查权限预算 → 执行观察 → 更新认知 → 继续或交付或说明阻塞 | **WorkChain 是固定三阶段 `Research → Direction → Draft`** | **冲突** |
| 九 | 原生领域改造：改默认身份、上下文装配、原生工具、任务生命周期 | **Codex Core 改动数 = 0**；现有扩展点只有 App Server extension registry | **冲突** |
| 十 | 营销记忆独立于聊天记录；区分用户提供/外部观察/模型假设/已批准决定；品牌/项目/任务/聊天分开 | `ClaimStatus` 六值**比四分更细**；`HeldOutMissionCase` 已有；**无品牌工作空间、无项目层级** | 部分（分层超出，层级缺失） |
| 十一 | "让它自己卖自己"作为完整测试（五关） | 系统自营销是 LG7 五类回归之一；**产品自描述记录是新增** | 部分 + 新增 |
| 十二 | Chat 技术实现与权限；授权绑定账号/对象/版本/金额；失败不盲目重试 | LG5（普通聊天不等于不可逆授权）、LG6（确定性边界由代码执行）、LG4（六类认识状态） | 已实现（护栏） |
| 十三 | 用 A/B/C 三臂对照证明不是换皮（直接 Prompt / 固定流程 / 原生 Agent） | 曾有三臂能力的 paired evaluator，**已于 2026-09-05 删除**；现用 GPT-6 / SeedEvolving 子智能体做 A/B | 部分（C 臂缺失） |
| 十四 | 五阶段开发顺序（原生 Chat 闭环 → 营销理解 → 完整交付 → 受控执行 → 结果修正） | 等价于冻结的 Plan 02–15；当前执行口径是"小营销切片" | **冲突** |

### GPT-6 前稿独有内容（最终稿未展开，但更具体）

| 项 | 内容 | 第七版现状 | 判定 |
|---|---|---|---|
| 源码入口表 | 12 条路径：`core/src/session/turn.rs`、`turn_context.rs`、`step_context.rs`、`tools/registry.rs`、`router.rs`、`spec_plan.rs`、`context_manager/`、`compact.rs`、`client.rs`、`app-server/`、`app-server-protocol/` | **12 条全部存在**（已实测） | 事实成立 |
| 工具清单 | 10 个：`read_business_context`、`search_sources`、`open_source`、`analyze_dataset`、`read_work`、`record_work`、`edit_artifact`、`preview_action`、`commit_action`、`read_results` | 只有 `marketing_work`。`read_work`/`record_work` 与之等价；其余 8 个缺失 | 新增 |
| 目录结构 | `marketing-domain`、`marketing-store`、`marketing-research`、`marketing-evals`、`core/src/marketing/` | `marketing-domain` ≈ `ai-ip-domain`（已有）；`marketing-evals` ≈ 已删除的 `ai-ip-evals`；其余 3 个不存在 | 部分 + 新增 |
| 上下文装配 | 每轮按需装配目标/事实/成果/证据/未决/预算，不塞全量词典与历史 | `prompt.rs` 已是此思路（96 token + 900 token 上限） | 已实现（原则） |
| 模型接入边界 | 换供应商后工具调用/流式/上下文恢复需逐项验证 | F-0012 已实测并修复方舟 SSE `added` 事件解析 | 已实现 |

## 3. 判定汇总

| 判定 | 条数 | 说明 |
|---|---|---|
| 已实现（原则或代码） | 9 | 多数已被 LG1–LG10 覆盖，其中"不把思考包成工具"是 9/5 的实际删除动作 |
| 护栏覆盖 | 2 | 无实现，但已有明令禁止 |
| 部分 | 5 | 方向一致，覆盖不全（主要是缺 Chat、缺品牌/项目层级、缺 8 个工具） |
| **冲突** | **4** | 见第 4 节 |
| 新增 | 4 | Chat 主体、产品自描述记录、8 个原生工具、存储/研究 crate |

**核心观察：GPT-6 的方案与第七版已有判断的重合度很高，真正分歧只有四条，而且都集中在"改不改 Core"和"固定不固定路线"这两个点上。**

## 4. 必须由用户裁决的四个冲突

### 冲突 1：改不改 Codex Core（最高影响）

- GPT-6 要求改 `session/turn.rs`、`turn_context.rs`、`compact.rs`、`tools/spec_plan.rs` 等核心 seam。
- 第七版事实：**Core 改动数 = 0**；F-0011 原文 `Keep core, protocol, model/provider, state, login and sandbox implementations unchanged.`
- 影响：patch ledger 要求每个上游改动写明 seam / 业务理由 / 回归证明 / sync 风险 / rollback。改 `turn.rs` 的 rollback 几乎不可能干净执行，且上游同步冲突面从"可忽略"变为"高"。
- 需要裁决：接受改 Core，还是坚持只用扩展点（extension registry + App Server）？

### 冲突 2：固定三阶段 vs 模型自选路径

- GPT-6 第八节：不固定研究路线，模型判断最缺什么再选下一步。
- 第七版 `work_chain.rs`：`Research → Direction → Draft` 三阶段固定骨架（有 `start_at` 跳阶与返工标记）。
- 影响：W3/W4 的失败记录显示模型会机械跟随该骨架。但完全取消骨架会让"研究/方向/成果"的产物边界消失，返工与审阅失去锚点。
- 需要裁决：保留骨架、改为可选建议、还是完全移除？

### 冲突 3：开发顺序 —— 五阶段 vs 冻结的 Plan 02–15

- GPT-6 第十四节：五阶段从"原生 Chat 任务闭环"开始，第一阶段就要 Chat + 工作区 + 任务状态 + 证据记录 + 权限边界。
- 第七版：Plan 02–15 自 2026-09-05 起冻结，当前执行口径是"一个真实 mission → 可用的 ContentPackage"小切片。
- 影响：五阶段 ≈ Plan 02 + 04 + 05 三合一。第七版从 8/25 至今这三样一个都没做。
- 需要裁决：解冻并接受大切片，还是维持小切片？

### 冲突 4：评测 —— C 臂缺失

- GPT-6 第十三节要求 A/B/C 三臂（直接 Prompt / 固定流程 / 原生 Agent），**C 臂就是本产品本身**。
- 第七版：具备三臂能力的 paired evaluator 已于 9/5 删除；当前只有 GPT-6 / SeedEvolving 的 A/B（通用 Agent vs 加方法）。
- 影响：没有 C 臂，就无法回答"这个系统比通用 Agent 强在哪"——而这正是 9/9 产品约定里定的主要对照问题。
- 需要裁决：是否重建 C 臂对照，以及用什么最小形式。

## 5. 双方都没解决的那一关

GPT-6 第十三节的八条测试（换行业、信息不足、错误线索、用户纠偏、不适合叙事、长任务恢复、自我营销、外部执行）**全部是行为正确性**；文档第 1765 行明确写"核心测试不是'哪篇文案更漂亮'"。

第七版七轮的实际卡点不是行为错误，是**走到终点了作品不被接受**（W3 用户否定、W4 不通过、两组对照阴性）。LG8 已规定营销质量需要五级证据（代码存在 → 机械合同 → 真实调用 → 业务盲评 → 真实发布复盘），但"业务盲评"的实现随 9/5 清理一起删除，**当前这个位置上没有任何替代品**。

这一条不属于 GPT-6 方案的缺陷（它是一份架构文档，不是验收标准），但它是把方案转成实施计划时**必须先补的前置**：没有它，五阶段做完仍然无法回答"这次做出来的东西够不够好"。

## 6. 下一步

按用户 2026-09-10 决定：先对齐（本文件），再转实施计划。

转实施计划前需要用户先裁决第 4 节的四个冲突，其中冲突 1（改不改 Core）决定整个工程形状，冲突 2 决定现有 `work_chain.rs` 的去留。

第 5 节所述的"作品质量验收标准"需要用户给出，或明确授权采用某个可测的替代形式。
