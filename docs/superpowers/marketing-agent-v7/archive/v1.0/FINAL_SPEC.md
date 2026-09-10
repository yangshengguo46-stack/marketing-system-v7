# Marketing Agent v7：自有内核最终施工规格

**版本：1.0 · 2026-09-10 · 基于已核对的 v7 提交**  
**适用仓库：** `yangshengguo46-stack/marketing-system-v7`  
**核对分支：** `codex/upstream-sync-20260910`  
**核对提交：** `e08f1e1ad5bbc3ea036aa271894d8228e499ed61`  
**性质：** 目标架构、接口合同、迁移和实施规格；不是已实现声明，不是完成验收报告。  
**维护决策：** 允许修改 Codex Core；不再要求持续合并上游，由本项目维护自己的内核。

> 交付目标：不是再造 Marketing OS，不是用聊天调用固定营销工作流，也不是把通用模型裸露给客户。基于已有 Codex 执行循环，使一个 Lead 能理解营销任务、获得可靠材料、选取专业方法、完成作品、处理必要授权，并依据真实结果调整工作。
>
> 本规格把“研发必须按顺序施工”和“产品中的 Agent 不被固定营销顺序控制”分开。工程依赖要严格；营销研究与创作路径由 Lead 决定。

## 阅读方式与交付包

- `FINAL_SPEC.md`：架构与行为的唯一目标规格。
- `IMPLEMENTATION_TASKS.md`：逐工作包施工清单、修改范围、接口、失败测试和交付证据。
- `contracts/`：数据库迁移起稿、配置示例、数据合同与 Core 接口草案。不是可直接覆盖现有仓库的补丁。
- `tests/`：合同和 SQLite 原型验证；不代表 Rust 产品、模型或外部平台已验收。
- `SOURCE_MAP.md`：核对过的源码、历史约定及证据范围。

本规格中的“现有”指所列 SHA 的文件；“新增”指本方案要求实现；“接入验证”指需要运行账户、平台或设备才能确定。不得把新增方法名当作上游已经存在的 API。

---

# 1. 已确认决策与真实起点

## 1.1 不再讨论的产品决定

1. 继续使用 v7 仓库和 Codex 原生模型—工具循环，不新建 v8，不整体迁入 v1–v6 运行时。
2. Core、协议和状态层都允许改；不是为了“证明原生”而修改没有必要改的文件。
3. 只有一个 Lead 拥有营销方向和最终综合判断权。工具、检索、方法和受限审核调用不构成另一套营销调度系统。
4. Chat 是用户主要工作入口。任务卡、证据、作品、媒体预览和授权卡是 Chat 的辅助呈现，不是替代 Chat 的表单工作台。
5. 面向主要在中国大陆使用的客户。按用户已确认约定，生产优先 SeedEvolving，GLM 5.2 是另一主力候选；真实 API 模型 ID、版本、权限和能力必须通过运行配置核验。GPT-6 仅作研发对照，不成为生产 Lead 或审核器必需依赖。[S01]
6. 用户首次使用就应获得可采用的业务成果。客户不负责教系统如何营销、调工具、提供专业方法论或长期“养熟”助手。[S01]
7. 核心营销能力不通过 MCP 串联。外部服务是数据与执行通道，不是第二个大脑。
8. 不恢复庞大盲评平台、固定多 Agent 顾问团、固定行业路由、按案例补关键词的语义分类器。[S02]

## 1.2 不能沿用的先前误判

| 曾经容易混淆的表述 | 本规格采用的准确结论 |
|---|---|
| WorldState 与 ContextContributor 二选一 | 当前 `ContextContributor::contribute_world_state` 就是 WorldState 的原生扩展入口。可以复用；不要绕到错误的 turn-context 路径。[S05/S06] |
| WorldState 本身就是业务数据库 | 它是每步构造的模型可见投影机制；业务真相仍须单独持久化，恢复时重建投影。 |
| Stop hook 已经能验收营销作品 | 它提供阻止停止、继续与停止的控制机制，不提供作品质量判断算法。[S07] |
| v7 已经有上线的事实 guard 和完整评测系统 | 历史计划不等于现有实现；对齐记录记载评测设施于 9/5 删除。必须检验当前代码，不把历史 commit 当现役能力。[S03] |
| 96-token root prompt 就是完整产品主身份 | 当前 `prompt.rs` 围绕 `HeldOutMissionCase` 构造评测输入，不足以证明生产 Lead 已具备完整营销身份。[S04] |
| 三阶段是一个强制执行的后台 Workflow Engine | 当前 `record()` 未强制按顺序提交；问题主要是 `next_stage`、工作单文本和工具描述持续诱导顺序，不应夸大为完整执行器。[S08/S09] |
| 状态里绝不能出现 next_action | 禁止的是程序规定下一营销动作。模型自己写的可修订计划可以保存；现有 `ActionFunnelStep.next_action` 是受众动作，不是 Agent 路由，不能误删。[S10] |
| 源码有 web.run，就能在所有国内模型账户开箱搜索 | 当前实现会使用 provider/auth 发起 SearchClient 请求；工具存在、可用账户和大陆网络可用性是三个不同层面。[S11] |
| Chat 不需要开发 | 不需要再造 Chat 推理引擎；自己的客户端、授权交互、断线恢复和作品展示仍需要实现。 |

## 1.3 现有资产的处理

**继续扩展：** `ai-ip-domain`、`ai-ip-runtime`、App Server、Core 会话与工具循环、原生文件/搜索/Skills/权限能力。保留 `ClaimStatus` 六类及 `ContentPackage` 兼容读取。[S04/S10/S12]

**改为历史兼容：** `WorkChain`、阶段型工作单、评测专用 prompt。旧工作 JSON 不删除，导入新任务与工件后保持只读。

**不是当前已完成：** 本规格的任务存储、生产质量审核、业务动作授权、发布与结果回读、新客户端。必须按工作包逐项落地。

**历史文档处理：** 新增 `ADR-OWNED-CORE-001.md`。只取代“Core 零修改”“必须同步上游”等旧约束，不抹掉旧失败记录、结果和证据。对已经过时的锁定 SHA 与“从未同步”文字加 superseded 注释，不删除历史。

---

# 2. 最终架构

```text
用户
 ↕ 自然语言 / 补资料 / 改方向 / 审阅 / 授权 / 取消
产品客户端（Chat + 作品/证据侧栏；无营销编排）
 ↕ 本产品版本化 App Server 协议
本产品 App Server（会话入口、认证连接、事件投递、依赖装配）
 ↕
自有 Codex Core ─────────────────────────────────────────────
  原生会话/Turn/Step / 模型调用 / 工具执行 / 取消与恢复
  已有 WorldState 管线 ← 有界任务投影 + 实际可用能力目录
  新增完成候选接缝 → 合并停止原因 → 继续/交付/等待/终止
  新增或完善工具观察与交付事件边界
  原有沙箱与审批保留；业务授权不能被普通 shell 绕过
 ───────────────────────────────────────────────────────────
      ↕ 原生编译链接的 ai-ip-runtime；不是 MCP
  任务服务 / 上下文投影 / 能力发现 / 证据入库 / 工件管理
  受限质量审核 / 授权与执行 / 成本记账 / 结果解释
      ↕
  ai-ip-domain           ai-ip-store（新增）
  纯类型、纯校验          主机私有 SQLite + 不可变对象存储
      ↕
  原生工具实现与外部适配器
  已验证的检索 | 文件/媒体理解 | 数据计算 | 生成/剪辑 | 平台发布/回读
```

## 2.1 依赖方向：不允许循环

```text
ai-ip-domain       -> serde / schema / 纯数据依赖
ai-ip-store        -> ai-ip-domain + SQLite + 主机文件能力
ai-ip-runtime      -> ai-ip-domain + ai-ip-store + extension-api + 工具/模型端口
core               -> extension-api + 原有 Codex 依赖
app-server         -> core + ai-ip-runtime + ai-ip-store（装配）
client             -> 生成的协议类型
```

**Core 不直接依赖 `ai-ip-runtime`，`ai-ip-runtime` 不反向调用 Core 私有对象。** 通过既有 `codex-extension-api` 增加通用完成接口和受限模型调用端口。虽然名字是 extension，代码仍在同一 fork 原生编译、由自己的 Core 调度；这不等于外部插件或套壳。[S05/S12]

只新增一个主要业务持久化 crate `codex-ai-ip-store`。不要同时建 `marketing-brain`、`marketing-orchestrator`、`marketing-research-service` 或独立任务引擎。

## 2.2 控制权边界

| 问题 | 最终负责者 |
|---|---|
| 任务是什么意思、研究什么、用何种表达、要不要找案例 | Lead |
| 当前账户、项目、已保存内容、授权、费用与执行结果 | 主机服务/数据库 |
| 某个事实的来源、版本与观测范围 | 证据服务 |
| 文件、数据计算、媒体与平台调用 | 原生工具及受控适配器 |
| 已取消、超预算、授权失效、哈希失配是否继续 | Core + 确定性校验 |
| 文案/作品是否切题、可制作、值得采用 | Lead + 有界独立审核；最终业务验收以人工/客户采用与外部结果为准 |
| 修改有无系统增益 | 研发同模型对照，不在客户运行时建立评测平台 |

---

# 3. 范围分层：完整目标，不一次盖完

## 3.1 第一个必须成立的纵向闭环

用户一句真实需求 + 可选材料 → 正确任务理解 → 必要检索或材料阅读 → 完整可采用内容成果 → 自查修订 → 可追溯交付 → 重启可继续。

这个闭环中已有材料充分时允许直接写，必要时允许多次研究、修改方向、重写；不规定工具次数、故事化、行业流程、候选数量。

**首个闭环不是整个最终产品。** 若用户请求成片，只有剧本不能标记成片完成；若请求发布，没有发布回执不能标记已发布。

## 3.2 在架构中预留、按能力逐项启用

媒体理解、视频制作、账号连接、发布、数据回读、长期复盘。每项有真实接入测试之后才出现在“可用能力”中。未实现的服务不提供会返回假成功的工具。

## 3.3 明确不做

自动注册/养号、规避平台风控、批量骚扰式评论/私信、虚假互动、自动执行无限预算媒体生成、客户不知情地把私有材料发到外部、跨客户学习私有资料。

---

# 4. 数据对象：生命周期不是营销工作流

## 4.1 四种 ID 不能混用

- `workspace_id`：本地工作空间及数据隔离边界。
- `project_id`：一个品牌/主体的业务项目；可以关联多平台账号。
- `mission_id`：一次用户希望完成的业务工作，可跨多个 Turn 和 Thread。
- `thread_id / turn_id / call_id`：Codex 执行与对话身份，不自动授予项目或平台权限。

所有 ID 由主机生成。模型不得自由指定其他客户的作用域。工具上下文中由主机绑定 `Scope`；输入仅接受作用域内对象 ID，不接受任意 owner_id。

一条 thread 同时只绑定一个 active mission；一个 mission 可以被多个线程只读观察，但同一时刻只有一个写入 Lead。子 Agent 默认只读任务快照和独立产物目录。

## 4.2 Mission

核心字段：

```rust
// 新合同草案，具体 derive 与 ID 类型见 contracts；不是上游现成接口。
struct Mission {
    id: MissionId,
    scope: Scope,
    revision: u64,
    objective: String,
    original_request_ref: UserMessageRef,
    constraints: Vec<Constraint>,
    expected_outputs: Vec<ExpectedOutput>,
    status: MissionStatus,
    task_understanding: TaskUnderstanding,
    approval_policy_ref: PolicyRef,
    created_at: Timestamp,
    updated_at: Timestamp,
}
```

`ExpectedOutput` 是目标，不是阶段：例如“修改已有脚本开头”“可拍视频方案”“最终视频文件”“某账号发布并返回链接”。Lead 可提出默认交付范围，用户可修订。不能要求客户先填写所有字段才开始工作。

`TaskUnderstanding` 保存四类信息：直接请求、客户提供的事实、Lead 的解释、尚未确定的解释。解释不是用户事实。具体专业创作判断由 Lead 承担，不能全变成追问。

## 4.3 状态机

```text
active → reviewing → delivered
   ↘ waiting_input / waiting_approval / blocked / paused / cancelled / failed
waiting_input/approval/paused/blocked → active（条件满足或用户恢复）
delivered → active（新修订；保留上一份交付与 revision）
```

`delivered` 表示交付了当前约定成果，不表示传播好、不表示发布成功、不表示客户已采用。

状态迁移用事务与 CAS（expected_revision），不允许模型直接写任意 status。取消和暂停由用户/主机优先决定；审核结果不能复活已取消任务。

## 4.4 事实和知识

继续保留现有 `ClaimStatus`：`UserFact / ExternalEvidence / ModelInterpretation / CreativeHypothesis / Unknown / ActualResult`。[S10]

新增独立的 `VerificationState`：`unreviewed / supported / disputed / unsupported / unverifiable`。不要把“来源种类”和“证据可靠性”塞进一个 enum。

- `UserFact`：用户说过，不等于第三方核实；绑定用户消息或已确认资料。
- `ExternalEvidence`：外部来源中观察到，绑定快照与 span；不自动代表来源是真的。
- `ModelInterpretation`：基于证据的解释，保存依据和限定，不升级成事实。
- `CreativeHypothesis`：创意/表达设想，可以是虚构；不得包装成真实客户故事。
- `Unknown`：显式未知，不用编造填满字段。
- `ActualResult`：主机写入，必须绑定运行或平台回执；模型无直接写入权。

## 4.5 工件与版本

`Artifact` 是作品的稳定 ID；`ArtifactVersion` 是不可变版本，包含内容哈希、MIME、存储对象、父版本、任务版本和来源引用。

类型开放但有基础集合：`research_note / strategy / topic / script / copy / storyboard / production_brief / image / audio / video / dataset / analysis / report / other`。

不是所有任务都需要所有类型。视频不必先生成图文文章；纯文案任务不必建立视频计划。

用户修改文件或正文后必须生成新版本。授权与审核绑定确切版本；旧版通过不能让新版自动通过。

## 4.6 决定、依赖与返工

`DecisionRecord` 保存“选择了什么、简短依据、取舍、证据和未知”，不是完整私有思维链。

依赖分成两类：

- 硬依赖：源文件版本、事实依据、授权内容、生产输入。变化时必须标记受影响对象 `needs_review`，既有授权失效。
- 软依赖：风格参考、备选方向、启发材料。变化时提示，不自动作废所有下游。

不得用 research → direction → draft 的位置关系判断所有失效。以实际依赖边做传递失效，使用 visited 集防循环；版本依赖在提交时检查不能形成自依赖。

---

# 5. 存储：SQLite 管状态，文件管正文

## 5.1 路径与真相源

```text
<AppData>/MarketingAgent/workspaces/<workspace_id>/
  state.sqlite3             # 任务、引用、事件、审批、费用等；主机私有
  objects/sha256/ab/<hash>   # 不可变正文/材料/媒体对象
  staging/                  # 待提交对象，不被算作已交付
  receipts/                 # 必要的大回执附件，敏感字段已脱敏
  backups/                  # 一致性备份，按明确保留策略处理

<用户授权的项目目录>/
  materials/                # 用户可读编辑的素材入口
  drafts/                   # 工作稿/导出镜像
  exports/                  # 用户主动导出的正式成果
```

数据库是业务状态唯一真相源；对象库是正式版本字节真相源；workspace 文件是工作材料或可再导入的导出镜像。**禁止同一 work 同时在 SQLite 和旧 JSON 中双写为两份权威状态。**

没有加密实现之前不得声称“已加密本地存储”。首个开发切片可以使用 OS 权限隔离；向客户分发私有材料版本前必须实现并验收敏感数据静态保护、密钥保管与恢复流程，见第 18 章。

## 5.2 表的最小集合

内容闭环目标表：`workspaces / projects / missions / thread_bindings / mission_leases / objects / artifacts / artifact_versions / sources / source_snapshots / source_spans / claims / evidence_links / dependencies / decisions / events / command_dedup / release_candidates / release_items / reviews / budget_accounts / budget_reservations / legacy_imports`。WP01 只建立作用域、任务、对象、版本与事务基础；证据、审核和预算在后续工作包加入，不要求先造齐这些表才跑第一条模型调用。

执行阶段追加：`approvals / action_executions / observations`。数据库 SQL 起稿分布在 `contracts/001_initial.sql` 至 `004_execution.sql`，随相应工作包分期迁入。列中的 JSON 承载版本化细节，不能绕过应用层 enum、权限与大小校验。

所有跨表引用必须约束 workspace/project，防止“ID 有效但属于另一个项目”的错误。作用域检查不能只写在 UI。

## 5.3 并发规则

SQLite 每个连接开启 `foreign_keys=ON`；采用 WAL；写事务短小，不在事务中等待模型/网络。写入遇忙按总截止时间有界重试。WAL 面向本地同机存储，不能把活动数据库放在跨机器网络文件系统共享。[S14–S16]

业务命令：

```text
接收 command_id + expected_revision
→ 认证作用域
→ 开始短事务
→ command_id 已存在且请求 hash 一致：返回历史结果
→ 同 command_id 不同 hash：IDEMPOTENCY_CONFLICT
→ expected_revision 不符：REVISION_CONFLICT
→ 校验命令与权限
→ 修改记录 + revision+1 + 追加事件 + 存去重响应
→ 提交
```

主机使用 mission 写入 lease 和递增 fencing token。数据库 CAS 是最终防线，不依赖“请只用一个 writer”的工具说明。

## 5.4 文件与数据库不能假装原子

对象写入协议：写 staging 临时文件 → 计算 hash/大小 → flush/fsync → 同文件系统原子改名进入不可变对象库 → 事务记录引用。中断后没有被引用的对象可回收；**已经进入对象库但未提交数据库的文件不能显示为已交付**。

数据库提交成功但 UI 事件丢失：从 `events` 序号恢复；不重做模型/媒体调用。删除采用受控流程，不用简单“删路径再删行”。

备份使用 SQLite 在线备份接口或停写的一致性快照，不能单独复制活动 `.db` 而丢掉 WAL。恢复先在独立目录校验版本、引用和字节，再切换。

## 5.5 时间与 ID

持久化时间统一 UTC RFC3339；UI 按用户时区展示。费用使用整数 micro-unit 加 currency，不用 float。操作超时使用单调时钟；跨重启保存截止时间和已使用预算。ID 使用主机生成 UUID；来源内容哈希使用 SHA-256。哈希只证明字节一致，不证明内容真实。

---

# 6. 任务接入、纠偏与作用域

## 6.1 不新增一个“意图分类模型”

仍由当前 Lead 正常理解用户消息。普通问候不创建营销任务；明确工作请求时调用 `marketing_work.create`，把原始消息引用绑定。UI 当前任务为空不意味着立即弹问卷。

首轮可以在同一个模型—工具循环内完成任务建立、必要研究和作品交付。任务解释不足时 Lead 先给出合理且明确的工作理解；只有确实影响目标、事实、预算或授权的歧义才提问。

**不能靠“不调用任务工具”逃过验收。** 产品营销模式在每次最终结束时都触发 CompletionContributor，包含原始请求与输出，即使 mission_id 为空。无 mission 时先做有界的适用性判断：普通聊天返回 NotApplicable、不建库；属于实质工作却未建立任务/保存成果时返回 Continue，由当前 Lead 补齐任务与工件，再进入完整审核。这个判断复用完成审核，不建立前置行业 Router 或第二个 Agent。初版允许这里增加一次小型同生产模型调用，计入成本；要优化成缓存/低成本路径，必须证明不会把复杂工作误当闲聊。不能相信模型自行传入 conversation 标志就免检。


## 6.2 用户纠正

用户说“不是招主播，是找地区合作伙伴”：

1. 主机先记录真实 user message，递增本 turn 输入 epoch；
2. 当前 Lead 接收 steer，修订任务解释并绑定该消息；
3. 已基于旧受众的策略、稿件和候选交付标记需要复核；
4. 旧发布授权不能继续使用；
5. 下一 Step 的 WorldState 显示新目标版本与失效对象；
6. 不把用户纠正塞进仅一次可丢失的对话摘要。

审核开始后出现新用户输入：已有审核结果可以保留作记录，但不能提交为新目标下通过。提交交付时同时 CAS `mission_revision + input_epoch + artifact hashes`。

## 6.3 多账号与切换

项目主体不等于平台账号。任何 publish/read_results 都绑定确切账号；不能靠“上次使用的账号”推断。切换项目时清掉原项目能力授权、任务投影和缓存引用，不清掉原项目持久化记录。

thread fork 默认复制可读历史，不复制有效授权或写 lease；任务绑定默认只读，用户/主机明确派生任务后可写。

---

# 7. Core 修改合同

## 7.1 必改和复用位置

| 文件/目录 | 动作 | 责任 |
|---|---|---|
| `core/src/session/turn.rs` | 修改 | 在正常模型结束位置形成完成候选；接收有界审核结果；合并 cancellation、预算、输入变更和 Stop hooks，不另建循环。[S07] |
| `core/src/stream_events_utils.rs` | 修改/按调用图定位 | 为业务交付的最终文本设置 provisional/released 语义；普通 commentary 仍流式。核对真实 delta 路径后同改 `turn.rs` 对应事件分支。[S17] |
| `ext/extension-api/src/contributors.rs` 与 `contributors/completion.rs`（新增） | 扩展 | 新建通用 `CompletionContributor` 合同，无营销字段。[S05] |
| `ext/extension-api` registry/builder 定义处 | 修改 | 注册新 contributor 与受限 review-model 端口，不能只定义 trait 不接调用链。具体定义文件由符号定位后记录。 |
| `core/src/session/world_state.rs` | 原则上复用 | 已调用 `contribute_world_state`；若预算/retained-fragment 测试暴露缺口再做小补丁。[S06] |
| `core/src/tools/{router.rs,registry.rs,spec_plan.rs}` | 按验证修改 | 捕获实际可见工具与工具结果；并行、审批和取消保持原义。不是写行业路由。 |
| `app-server/src/extensions.rs` | 修改 | 注入同一 store、mission service、审核端口与能力适配器；替换目前无依赖 `install`。[S12] |
| `app-server-protocol` | 增量修改 | 任务、工件、事件、授权接口；新旧客户端能力协商。 |
| `ai-ip-domain` / `ai-ip-runtime` | 扩展 | 产品类型、任务、证据、质量、动作。 |
| `ai-ip-store` | 新增 | 持久化与事务，禁止含 LLM 营销推理。 |

## 7.2 新的通用完成接口（目标签名）

```rust
pub trait CompletionContributor: Send + Sync {
    fn review<'a>(
        &'a self,
        input: CompletionInput<'a>,
    ) -> ExtensionFuture<'a, Result<CompletionDecision, CompletionError>>;
}

pub enum CompletionDecision {
    NotApplicable,
    Allow { report_ref: String },
    Continue { feedback: Vec<CompletionIssue> },
    Wait { kind: WaitKind, message: String },
    Block { reason: String },
}
```

`CompletionInput` 必须包含：`thread_id / turn_id / candidate_id / candidate_digest / input_epoch / remaining_budget / cancellation handle / bounded read-only review context / current provider handle`，以及可空的 mission binding。无 mission 的适用性检查不能伪造 mission_revision=0 塞入正式交付表。不能包含平台密钥，不给审核调用任意工具权限。

这是新增目标 API，不是 `StopOutcome` 的已有字段。具体 trait 的输出、错误与 registry 要在同一个工作包中联通。单独增加 enum 不算完成。

## 7.3 原生循环里的执行次序

```text
1. 接收/合并用户输入；尊重输入 epoch 和取消
2. 固定本 Step 的模型、能力与权限快照
3. 读取 mission 权威状态，构造有界 WorldState
4. 原生模型调用
5. 有工具调用 → 按既有执行器执行 → 记录可靠观察 → 回到下一 Step
6. 有新输入 → 处理输入，旧完成候选作废
7. 没有 follow-up → 形成 completed-output candidate
8. 先处理用户取消、硬预算、硬权限等终止条件
9. 运行既有 Stop hooks，并取得业务 CompletionDecision
10. 合并结果：不能互相绕过；需要继续时给原 Lead 精确缺口
11. 可交付时提交 immutable release + 事件；否则保存草稿与明确状态
```

停止语义优先级：`cancelled > 安全/授权禁止 > 硬预算 > 新用户输入 > 待用户决定 > 需修订 > 允许完成`。实际 Stop hook 的 stop/block 意图需保持兼容；业务“Allow”不能覆盖别的禁止。

**仅 `should_block=true` 但无 continuation feedback 不能依赖上游替你兜底。** 当前实现会忽略缺少提示的 block；新完成接口必须校验反馈非空，或以明确 blocked 状态结束。[S07]

## 7.4 有界继续

默认每个实质交付候选最多 2 次自动修复，初次审核 1 次；总费用与 token 仍由 mission budget 管理。数值是可调工程默认值，不是“营销必须迭代两次”。

相同候选 hash + 相同缺口 hash 不再次无限审核；发生相同失败两次进入“保留草稿、说明阻塞/请求必要输入”。耗尽预算不能伪造通过，也不能丢弃已完成工作。

已有 goal 自动续跑与本次 turn 内返工共用一个 continuation owner。等待授权、用户取消和已交付任务不能被 idle callback 重新拉起。

## 7.5 流式输出与交付边界

Stop 是在模型输出之后触发的，不能把它当成未审核文本从未展示过的保证。

本产品把输出分成：

- `progress`：正常流式工作更新；不宣布已发布、已通过。
- `draft`：可预览草稿，明确“尚未完成检查”；不能由客户端当正式交付导出。
- `release`：绑定审核、任务版本与确切工件版本的交付事件。

实质任务最终回答的业务正文先暂存在候选缓冲/对象库；通过后转为 release。普通简短聊天不建立业务任务，也不做完整的作品/来源审核；仍经过轻量的完成适用性检查。不要通过暂停所有 tool/commentary 流制造长时间黑屏。供应商若不提供可靠的 final/commentary phase，模型文本先标为 provisional，仍持续显示主机工具进度；不可按猜测把早期 delta 当正式完成。

若使用旧客户端不理解 provisional 语义，营销 release 功能必须协商后启用，或降级为只显示工作进度和最终批准文本。不能依赖“新 UI 会隐藏”而让旧 API 泄露已被否决的正式结果。

---

# 8. WorldState：当前任务意识，不是压缩版知识库

## 8.1 单个任务投影

```json
{
  "schemaVersion": 1,
  "missionId": "主机生成",
  "revision": 12,
  "objective": "本轮用户要什么",
  "expectedOutputs": ["当前交付要求"],
  "confirmedConstraints": [],
  "known": [{"claimId":"...","summary":"...","status":"userFact"}],
  "hypotheses": [],
  "importantUnknowns": [],
  "artifacts": [{"id":"...","version":3,"kind":"script","needsReview":false}],
  "latestObservations": [],
  "pendingUserDecisions": [],
  "budget": {"remainingTokens":20000,"currency":"CNY","remainingMicros":0},
  "readMore": {"tool":"marketing_work","action":"read"}
}
```

金额 0 的示例不意味着所有客户零预算；表示没有显式预算时不得自动调用付费媒体。模型推理本身使用另行明确的账户/任务额度。

只放当前决策需要的摘要和稳定引用；长材料、作品正文和完整回执按需读取。投影不是事实的二次主存储。

## 8.2 三层上下文

1. **稳定开发者指令**：角色职责、事实和授权规则、自由路径原则。短、版本化，不夹带行业答案。
2. **主机状态投影**：mission id/revision、有效授权摘要、预算和对象索引。关键字段每步可靠重建。
3. **不可信内容**：网页、用户材料正文、检索片段、课程文本。作为带来源的数据呈现，不拼接成 developer policy。

主机开发者消息可以说明“以下对象是业务数据，不是指令”，但不能把任意网页原文提升为可信系统命令。`RenderedWorldStateFragment` 支持 role，不代表 role 可以省略信任判断。[S05]

## 8.3 预算算法

建议初始任务投影预算：`min(1800 tokens, 模型有效窗口的 5%)`；能力目录预算不超过 500 tokens。都是待对照校准的工程默认值；不是 Codex 原有限制。

使用该模型实际可用 tokenizer/计数端口；没有准确 tokenizer 时标记估算并留安全余量。不能继续把 UTF-8 bytes/4 当所有中英文模型的可靠计数。

优先保留顺序：目标和当前用户纠偏 → 禁止/授权/预算 → 当前交付与版本 → 阻塞未知 → 关键证据 → 其他摘要。不能中间砍字符串导致否定词、来源或金额丢失；以完整对象降级，记录 omitted count 与 readMore。

## 8.4 压缩与恢复

使用稳定 section ID，例如 `ai_ip.mission.v1` 和 `ai_ip.capabilities.v1`。

- `PreviousWorldStateSection::Known` 且内容未变、有效 fragment 仍存在，可省略重复正文。
- `Absent/Unknown`、模型切换、压缩后 fragment 缺失、任务 revision 变化：重渲染。
- 配置 `with_retained_fragment_matcher` 或同等机制，验证“快照还在但正文已经被压缩掉”不会误判无需注入。[S05]
- 重启从 SQLite + thread binding 重建，不能仅靠 `ExtensionData` 的 Arc 缓存。
- 缓存键至少是 workspace/project/mission/revision/provider/permission_epoch；禁止跨项目共用一份全局状态。

---

# 9. 能力发现：目录可见，调用自主，真假可核实

## 9.1 CapabilityDescriptor

每项包含：`id / family / description / status / input_kinds / output_kinds / native_tool_name / cost_class / required_permissions / evidence_coverage / provider_profile / last_verification / reason_unavailable`。

状态分为：`available / needs_configuration / needs_permission / degraded / unavailable`。服务未实现是 unavailable，不写“即将完成”冒充可用。

目录来自**本 Step 已装载工具、权限、模型能力和适配器健康状态**，不是手写营销宣传文案。不在每一 Step 发网络健康检查；缓存配置与近期失败，TTL 和配置变更使缓存失效。

## 9.2 工具暴露策略

常用轻工具直接可见：任务读写、已有原生搜索（可用时）、文件读取、工件保存/检查。媒体和平台等大 schema 按需发现。

只隐藏大 schema，不隐藏存在性。不能重演“所有能力都藏在 tool_search 后，模型根本没想到搜索工具”的失败。

能力发现按相关性提供候选，但不能强制执行，不能根据“TikTok/黄金礼品/雪茄”关键词决定路线。相关性不足允许直接探索与读取。

## 9.3 可复用方法库

方法按能力组织，而不是每行业一个 workflow：理解交易与受众、研究和证据阅读、内容世界、直接销售/证明/演示/解释/故事、视听表达、发布与指标解释。

每个方法有：适用条件、可解决的问题、输入材料、局限、反例、可选操作建议、输出例子。**没有“必须调用下一个方法”的引用链**。

原语/动机词表只是启发词汇，允许扩展和不使用。不得写成完成任务必填的人性分类，也不得强制从产品跳到宏大故事。方法变更须同模型留出对照，无增益就删。

## 9.4 生产主身份起稿

```text
你是对本次营销工作成果负责的 Lead。以用户的真实目标、材料和约束为准。
你可以自主研究、选择表达方式、制作和修订成果；不要把所有任务套入同一流程。
对于已有材料足够的局部任务，直接完成；对于关键事实或方法不足的任务，主动使用实际可用能力补齐。
用户负责客户特有事实和关键业务决定，你负责专业判断，不要用问卷转嫁专业工作。
用户事实、外部观察、你的解释、创意假设、未知和实际结果必须区分。
创意可以虚构，但不得冒充真实客户、实验结果或发布回执。
保存重要成果和简短决策依据；不要输出或保存完整私有推理过程。
发布、付费和其他有后果操作以主机给出的有效授权为准，不得自行声称获得授权。
交付可采用的完整成果；无法完成的部分保留已完成内容并明确说明，不以“以后会更好”代替结果。
```

这是待验证的最小产品职责文本，不是效果保证；不得无限往里面堆旧失败案例。生产输出是自然语言与工件，不是每句话都强制 JSON。

---

# 10. 原生工具合同

不要把“理解人性、提炼原语、想选题、评战略”等每种思考包装成一个内部再调 LLM 的工具。

## 10.1 `marketing_work`（替换阶段语义，保留兼容入口）

目标 action：`create / read / update / add_note / record_decision / bind / propose_delivery`。

共同入参：`action / commandId / missionId? / expectedRevision? / payload`；workspace/project/thread scope 由主机注入。写 action 必须携带 commandId 与必要的 revision；read 分页读取。

- create：原请求 ref、任务解释、目标、可选约束/预期输出；不能直接接受 `delivered`。
- update：字段级 typed patch；不允许更改 owner、费用或结果回执。
- add_note：观点、未知或方法说明；显式 claim 状态。
- record_decision：选择和简短依据，可保存 Lead 提出的下一步计划；计划可随时改变，不是主机指令。
- propose_delivery：提交候选，不等于通过。可以在没有该工具调用时由 Core 捕获实质 final；不能用“没调 propose”逃过完成检查。

当前 `open/record` 保留到迁移完成；进入 legacy adapter，不继续发送 `follow the next work order`。`startAt/stage` 只映射旧对象标签，不控制新 Lead。

## 10.2 `marketing_artifact`

action：`create / read / revise / list / export`。

create/revise 输入：任务、kind、MIME、内容或授权相对路径、父版本、claim/evidence refs；主机验证路径、哈希、文件类型、大小和版本连续性后写不可变对象。

不允许模型直接把 `readiness` 设为 passed。formal export 必须引用已交付 release；draft export 允许，但返回 `draft` 标记，不能伪装正式批准。

## 10.3 `marketing_evidence`

action：`list / read / inspect / link_claim`。

source 的 URL、观测时间、摘要和快照由实际读取/适配器生成。`link_claim` 只提交“这个 span 可能支持这个主张”，不能由模型设置 `source_verified=true`。

读取返回完整且有界 span 与 provenance；缺失正文要明确是 snippet-only，不把搜索摘要当成看过视频或整篇文章。

## 10.4 `marketing_capabilities`

action：`list / describe`；只反映当前 scope/provider 的实际能力。不能返回账号密钥、签名 URL 或内部网络信息。

## 10.5 `marketing_action`（后续真实执行包启用）

action：`prepare / execute / read_status / read_results`。

`prepare` 生成可审阅动作预览和内容 digest；`execute` 只接受 preparation ID，主机自行查有效 approval 与预算，不接受模型传 `approved=true`。

已存在原生搜索、文件编辑和 shell 不重新造营销版同义工具。`analysis` 和媒体适配器可独立作为真正执行工具，不伪装成专业推理工具。

## 10.6 错误协议

统一返回：`code / message / retryable / retryAfterMs? / currentRevision? / requiredPermission? / partialArtifactRefs[]`。

主要错误码：`SCOPE_DENIED / REVISION_CONFLICT / IDEMPOTENCY_CONFLICT / SOURCE_UNAVAILABLE / SOURCE_COVERAGE_INSUFFICIENT / CAPABILITY_UNAVAILABLE / BUDGET_EXCEEDED / APPROVAL_REQUIRED / APPROVAL_STALE / EXECUTION_UNKNOWN / CANCELLED / VALIDATION_FAILED`。

语义错误返回模型可修复错误；秘密、内部堆栈不进入模型。网络重试与业务返工分开记账。

---

# 11. 证据系统：来源、内容与解释分开

## 11.1 Source / Snapshot / Span

`SourceRecord` 保存来源身份与类别；`SourceSnapshot` 保存这次实际看到的字节、抓取时间、内容 hash、解析器版本、覆盖状态；`SourceSpan` 保存稳定定位。

定位联合类型：`text(byte_start,byte_end) / page(page,region?) / media(t_start_ms,t_end_ms,frame_ids?) / table(sheet,row,column) / tool_receipt(receipt_id,field_path)`。

UTF-8 偏移必须落在字符边界；不得把 JavaScript UTF-16 字符位置直接当 Rust 字节偏移。网页内容更新后创建新快照，旧 claim 继续绑定原快照；不能静默改写证据。

## 11.2 原生搜索引用不是永久证据 ID

一次 `web.run` 的 ref_id 和行号是当前工具会话的引用句柄；产品保存长期证据时，需要记录 URL/来源标识、观测时间、当次实际返回文本/快照与可复现 span。

若服务不提供原始正文，只能存 returned_excerpt 与 coverage=partial；不能声称保存了完整网页。失效 ref 不直接重用，重新打开后得到新版本。

## 11.3 媒体理解

视频必须区分：元数据、帧采样、画面观察、OCR、ASR、音频、镜头边界与模型推测。抽了 12 帧不等于看完全部视频；无音频分析不能判定音乐与口播。

材料记录包含：原文件 hash、duration、actual sampled intervals、transcript provenance、识别置信/已知局限、模型版本、处理参数、处理时间与费用。

对标分析：身份线索 ≠ 代表作品；代表作品 ≠ 因果成功证明。可以提出迁移假设，但不把“播放高”自动解释为某个叙事机制导致成功。

## 11.4 事实审核不是神谕

确定性部分：引用存在、scope 一致、span 越界、哈希、来源版本、结果回执、有效授权、数值计算可重放。

语义部分：语句是否被来源支持、是否把时间关系写成因果、是否扩大数量范围、是否错误跨来源拼接、是否把计划写成完成。它需要模型/规则辅助核对，存在误判；只能报告 `supported / unsupported / unclear` 与依据，不声称形式化证明现实真相。

不能只校验模型自己填写的 `claims[]`，还要检查公开正文、标题、字幕、语音稿和关键视觉文字中的事实断言。数字、实体关系、证言、效果承诺、时间、价格等优先抽取。漏检率要在专门测试集中测量。

## 11.5 虚构与推断

虚构角色、示意场景、拍摄设想和情节冲突允许存在。审核关注是否会被合理理解成真实客户、真实资历、真实结果，而不是“素材库里没有这个角色所以禁止创作”。

合理跨来源推理允许：建立 derived 节点，明确支持路径和限定词。不能要求一切推导必须被单个 span 原样写出；单一 span 原样支持只适用于声称来源直接给出的事实。

---

# 12. 作品质量与完成验收

## 12.1 先锁定用户要的成果

通用完成合同不能硬编码所有任务都需要“定位、受众、选题、完整脚本”。局部改开头只验局部目标与保留约束；起号方案检查对应完整范围；成片请求检查可播放成片而不把脚本冒充完成。

`AcceptanceContract` = 原始请求 + 已确认范围 + 用户纠偏 + 当前明确交付清单。Lead 可以提出合同，但不能在交付失败后单方面降低目标以通过检查。

## 12.2 三层检查，一次有界审核

**A. 机械与安全检查（代码）**：文件真实存在、scope、格式、可读性、哈希、版本、引用、关键禁止、授权和费用。确定性失败不能被模型打分覆盖。

**B. 任务与事实检查（受限生产模型）**：是否答对问题、事实和假设是否混淆、是否漏关键交付、是否把客户必须的资源编出来。审核输入只含任务、候选正文、相关来源与已知限制；不能调用 shell、发布、改数据库或任意搜索。

**C. 作品与制作质量检查（同一次审核中的独立维度）**：表达是否具体、有受众价值、值得采用；形式与任务是否相配；是否只有方法名称没有作品；画面/声音/节奏/资源是否能落实；是否把专业问题甩回客户。

B/C 默认由与 Lead 相同的已验证国内生产模型做一个无原生成推理历史的受限调用。不默认引入 GPT-6、三评委投票或固定专业多模型团队。[S01]

## 12.3 审核输出

```json
{
  "schemaVersion":1,
  "verdict":"revise",
  "candidateDigest":"...",
  "missionRevision":12,
  "inputEpoch":3,
  "findings":[
    {
      "id":"q_001",
      "category":"task_fit",
      "severity":"blocking",
      "artifactVersionId":"...",
      "location":{"kind":"text","start":0,"end":48},
      "problem":"交付稿面向消费者，但当前任务面向合作伙伴",
      "basisRefs":["user_message:..."],
      "acceptanceCondition":"保留用户确认的受众与招募目标"
    }
  ],
  "quality":{"specificity":"weak","shootability":"acceptable"},
  "uncertainty":["传播效果尚无外部验证"]
}
```

审核器给出问题和应满足的条件，不下发一条固定营销工具路线，不自己替 Lead 写新稿。具体改法仍由 Lead 决定。

普通风格偏好不作无限 hard gate；无法可靠判断的主观质量记为建议/不确定。明确答非所问、无法制作、缺交付物和来源冲突可以阻止正式交付。用户的内容偏好优先于模型审美。

## 12.4 交付提交

`release_candidate` 绑定 `mission_revision / input_epoch / artifact_version_ids / content_digests / scope / acceptance_contract_digest`。

审核完成 → 再取当前 revision → 核对未被新输入或修改作废 → 事务写 release status、report ref 和 release event。

用户只说“采用这个方向”不等于发布授权。`delivered / user_accepted / exported / published / measured_effective` 是不同事实。

## 12.5 超时或审核不可用

保存草稿并返回 `review_unavailable`，允许用户明确接受未核验草稿；不能把审核异常视为 pass。发布路径仍然要求有效授权和不可绕过的安全校验。

拒绝/返工原因要展示可读摘要，不显示完整审核模型内部思维。审核 token、费用和返工次数进入任务总账。

---

# 13. 研究与数据计算

## 13.1 搜索后端接入

首个研究工作包必须在 SeedEvolving 与 GLM 5.2 的真实运行配置下分别测试：发现工具 → 调用 → 打开来源 → 读取正文 → 引用回传 → 超时/限流恢复。

如果原生 `web.run` 后端不能用于某账户，保持原生工具界面与模型循环，替换或增加**同级原生研究适配器**。不是换成一套 MCP 营销流水线。

适配器合同：`search(query,limits)->SearchResult[]`、`open(handle_or_url,limits)->SourceSnapshot`、`find(snapshot,pattern)->Span[]`。支持常规网页工具所需的域名、时间和预算限制；不把未验证的 provider 特性复制进 schema。

生产可用能力列表由真实 profile 决定。无可用研究服务时可以完成明确不依赖外部证据的任务；对必须研究的任务说明缺口，不暗中退化为凭记忆编造。

## 13.2 受控数据执行

不用字符串黑名单判断 Python/shell 安全。数据工作以只读输入目录、独立输出目录、禁网络、无凭据、CPU/内存/时间/输出限额运行。使用已验证的 Codex 沙箱环境；不能满足限制的平台不启用“安全数据执行”能力。

代码由 Lead 编写，执行器只执行。记录脚本 hash、输入 hash、环境版本、退出码、输出文件和统计摘要。任何计算结果能从输入与脚本复现；模型不得捏造数据作为实际结果。

CSV/Excel 导出要处理公式注入；日志和异常不泄露完整私有表格。空样本、缺失、零分母、时区、去重口径必须显式处理。

---

# 14. 视频、图片、音频与成片

## 14.1 统一对象，不建第二套视频总控

所有媒体都属于同一个 mission 和 artifact version 体系。媒体异步 job 是工具执行状态，不成为第二个带战略权的 Agent 或视频数据库。

`ProductionSpec` 的基础内容：目标与语气、画幅/分辨率/帧率/时长、镜头/节奏、画面元素、口播/表演、音乐/环境声、字幕、安全区、素材与权利、预算和交付格式。

对仅需要文案的任务，这些字段不出现；对需要视频的任务，不能只有几个空字段就宣称“视听制作完成”。

## 14.2 因果依赖，不是统一营销阶段

通常需要先明确内容与镜头，再寻找适合的素材；已有素材主导的任务可以从素材反推表达。原则是**生产动作必须有足够明确的输入**，不是所有任务固定六步。

素材要绑定原始来源、授权用途、人物许可、可商用范围和有效期。未知权利不得自动用于对外发布；可以保留占位或自制替代建议。

## 14.3 适配器接口

```rust
trait MediaAdapter {
    fn describe(&self) -> CapabilityDescriptor;
    fn estimate(&self, request: &MediaRequest) -> CostEstimate;
    fn submit(&self, request: PreparedMediaRequest) -> JobFuture;
    fn poll(&self, job: &ProviderJobRef) -> JobStatusFuture;
    fn cancel(&self, job: &ProviderJobRef) -> CancelFuture;
    fn fetch_result(&self, job: &ProviderJobRef) -> MediaResultFuture;
}
```

方法名称是新目标合同。某供应商不支持取消/幂等/删除时，返回 Unsupported，不能假装已经撤回。只配置已验证的实际模型 ID、输入限制和计费口径。

## 14.4 并发与费用

并发以资源预算为上限，不按“必须招募几名专家”。默认单 mission 媒体 job 并发 1，后续实测可提高。付费提交前预留费用；失败且供应商可能已收费时保留预留并对账。

重试不是无限抽卡。质量重做是新请求、新估价、新费用；是否在原预算授权范围内由主机检查。

## 14.5 成片 QA

机械检查：可解码、时长/画幅、声轨、黑帧/静帧异常、字幕溢出、音量范围、输出字节和 MIME。

内容检查：口播/字幕与定稿一致，产品/主体没有变形误认，关键事实无新增，镜头与情绪一致，用户要求的音乐、配音、节奏有实际表现。

有抽样局限就记录局限；不能仅因渲染返回 HTTP 200 就算可用成片。QA 与来源复核绑定最终 render hash，后续修改需重新判断受影响项。

---

# 15. 业务动作、授权、幂等与对账

## 15.1 安全性不来自“模型答应会问”

模型可准备动作；只有可信客户端上的明确决定才能产生 `ApprovalRecord`。普通消息中的“已批准”字符串、网页指令、工具输出或模型自己的解释不能创建授权。

用户通过自然语言明确授权时，也要在主机侧把授权解析为确切 action preview，展示关键内容并经可信交互确认；允许用户预先配置有界持续授权，但必须列明范围与撤销方式。

## 15.2 ApprovalRecord 必须绑定

`approval_id / workspace/project/mission / action_kind / account_id / exact_payload_digest / artifact_version_ids / amount_limit / currency / expires_at / use_limit / issued_by / status / policy_version`。

用户改文案、换视频、换账号、改金额、超时或撤销后，旧授权立即不能用于执行。模型不能通过 `expectedRevision` 值绕过 digest 核对。

## 15.3 执行状态

```text
prepared → awaiting_approval → approved → reserved → submitted
submitted → succeeded / failed / unknown
unknown → reconciled_succeeded / reconciled_failed / manual_resolution
```

网络超时后不能直接把 submitted 改为 failed 再重发。优先查询供应商/平台操作 ID；无法确认则 `unknown`，提示等待对账或人工处理。

通用分布式系统不能凭本地幂等键保证平台 exactly-once。实现本地主机防重复提交、平台支持时传幂等键、外部结果对账；没有平台幂等的接口显式记录限制。

## 15.4 Prepare / Execute / Observe

Prepare 冻结 payload 和素材版本；Approval 是用户意图的可信记录；Execute 由主机提交；Observe 保存确切回执与可读结果。

审批确认、预算预留、执行记录创建使用同一个事务；网络在事务外执行。提交结果与 outbox/event 原子记录。崩溃恢复按 action 状态处理，不重新执行 succeeded/unknown。

## 15.5 原生 shell 与浏览器不能变成后门

“发布工具要求审批”而 shell 可以拿密钥 `curl` 发布，是无效的权限设计。

生产营销环境：主机密钥不进入模型、工作区或执行子进程；有外部写权限的浏览器会话和平台凭据由受控执行通道持有；默认不给通用 shell 任意认证网络写能力。公共研究与数据执行采用低权限环境。

跨平台沙箱、浏览器已登录会话、脚本、HTTP、第三方工具都要纳入同一权限测试。若开放不受限 shell 或共享登录浏览器，则必须标为开发者不受限模式，不能继续声称业务审批不可绕过。

---

# 16. 结果回读与长期学习

## 16.1 发布成功和营销有效是两回事

`PublicationReceipt`：平台账号、内容 ID、确切版本、提交时间、状态、回读 URL/内容一致性。

`Observation`：来源接口、采集时间、统计窗口、指标名称、值、单位、口径、账号与内容范围、缺失或延迟说明。

播放、完播、评论、询盘与成交不随意互换；没有归因证据就不能说“该内容造成成交”。用户手动输入结果标为 user supplied，而不是平台回读。

## 16.2 学习记录

Lead 可根据结果提出：继续/调整/停止某个方向、受众假设变化、值得重试的表达。写成版本化 DecisionRecord，引用观察与反例。

长期记忆保存客户明确偏好、已确认业务事实、资产、采用/拒绝及理由。不自动把单次高播放提升为通用规律，不把不同客户资料混成方法库。

每条可更新记忆有来源、作用域、last_verified、有效期/失效条件。没有明确证据不能把“推测用户有主播资源”写成事实。

---

# 17. 模型与协议兼容

## 17.1 配置不是模型名字列表

`ProviderProfile` 保存实际 API endpoint、auth secret ref、真实 model ID、可得 revision、transport、tool calling、streaming、context window、tokenizer/估算方式、图片/音频支持、原生 web 可用性、最大输出、费用表版本与最近验收。

SeedEvolving 是产品选型标签，不能因此凭空填写官方 API 型号；GLM 5.2 也必须核对当前账号能调用的具体 ID。型号或权限未知时保持 profile 未就绪，不偷偷改用别的模型。

## 17.2 验收矩阵

对每个生产 profile 测：纯文本、多个连续工具调用、严格/非严格结构化工具参数、中文长上下文、流式断线、重复 event、usage 缺失、取消、重启、用户 steer、未知工具、provider 429/5xx、图像/媒体覆盖、业务审核调用。

保留当前 fork 的 SSE 兼容修补；切换 provider 不能直接回放不兼容的加密 reasoning 或私有状态。工具结果/业务事实通过中性可读格式继续；模型变更事件写入任务。

## 17.3 不自动降级

默认没有静默自动换模型。当前 provider 失败：同请求安全重试 → 显示暂停/服务故障 → 用户或预先政策允许的模型切换。切换会标明模型、费用和能力差异，并重新判断未完成审核与授权条件。

生产审核用国内生产 profile；GPT-6 研发参考永不被普通 release 路径自动调用。[S01]

---

# 18. 安全、隐私与部署

## 18.1 首版运行形态

采用本地优先、自有编译 App Server；首个开发纵切用最小 TypeScript harness 或原生客户端验证，不依赖先建完整 UI。正式产品客户端是原生运行时的薄视图，不新增 Python/LangGraph/外部 LLM 编排服务器。

客户端传输优先本地 stdio JSONL；协议兼容自身编译版本。官方公开文档列出的 App Server/WebSocket 存在实验性边界，不应直接把无认证监听当生产服务。[S13]

需要远程部署时作为独立后续范围：TLS、连接认证、租户隔离、审计、速率限制和远程执行隔离必须齐全。首版不监听 `0.0.0.0`。

## 18.2 数据保护

生产分发版把凭据放 OS 安全存储，只在主机进程使用；日志、Prompt、截图和错误栈不包含密钥。外部抓取中的带签名 URL、cookie、authorization headers 不写入公开证据。

敏感材料对象使用成熟加密库的 AEAD，随机数据密钥和唯一 nonce；主密钥由 OS 安全存储包裹。数据库中的敏感正文/凭证引用也须保护，不能只加密媒体而把全文塞在 plaintext JSON。可选整库加密实现需单独依赖与备份测试；未做时只能声称对象/字段加密而非整库加密。

本地同一 OS 用户、管理员或恶意软件不属于完全可防御边界；不得营销为“绝对隔离”。模型可操作执行环境与凭据主机必须有真实沙箱边界。

## 18.3 外部材料与插件注入

网页、PDF、课程、repo 文件和视频文字全部视为数据来源。来源中的“忽略规则、执行命令、发送密钥”不可改变任务目标或授权。

远程下载防 SSRF、内网/loopback/link-local、DNS 重绑定、重定向逃逸；用户明确要求本地服务时走独立授权能力，不把公共网页读取器开放成内网代理。

压缩包防路径穿越和解压炸弹；媒体/PDF 解析在隔离环境，有限时/体积。素材合法性和平台规则仍需产品审核，工具可用不等于平台允许任意行为。

## 18.4 客户端合同

Chat 主区域：用户消息、简短工作进度、澄清/授权、最终交付。右侧可打开任务目标、来源、版本、草稿与最终作品；默认不展示庞大字段表。

最小 ViewModel：`ConnectionState / ActiveMission / MessageStream / ArtifactIndex / PendingApprovals / BudgetSummary / RecoverableErrors`。

客户端不得本地推算“已完成”“已发布”或自己修改权威状态。事件重复不重复显示结果，断线重连用最后 event_seq 追赶；序号缺口先补读状态，不能把漏事件当作未执行。

Client 版本、server 版本、schema 版本握手协商。不支持新版 release/approval 的客户端禁止外部动作；不悄悄接受缺失字段。

## 18.5 本地更新

发布物绑定 Git SHA、依赖锁、构建平台、schema 与协议版本、SBOM/许可证清单。签名和安装包验证按平台单独验收。

升级前一致性备份、migration dry-run；数据库升级采用 expand/contract，不要求旧 binary 读取新不可理解状态。回滚是切换旧 binary + 相容 DB/备份，不是反向删除几张表。

不自动合并上游，但需要定期跟进依赖与安全修复，选择性移植。保留许可证和来源声明；“代码自己维护”不意味着删掉来源记录。

---

# 19. 预算、调度与观测

## 19.1 一个主循环，一个任务预算

所有 Lead、审核、子任务、搜索付费和媒体调用使用同一个 mission 预算账本。不得只给主模型计费而漏掉审核/返工/工具费用。

状态：`limit / reserved / settled / unreconciled`。可用额度 = limit - reserved - settled；reserved 包括外部结果未知请求。无实际价格表时允许 token 硬限，不声称准确人民币成本。

默认开发配置：自动完成修复≤2、受限审核≤初次+2、普通读网络安全重试≤2、写动作未知后不盲重试。wall-time/token/费用具体限额由产品配置和运行证据调整，不是写死行业流程。

## 19.2 不记录完整思维链

记录 task/turn/call IDs、工具名、参数摘要/hash、来源、工件、时间、usage、费用、错误类别和简短 decision summary。模型原生隐藏 reasoning 不复制进业务日志、客户 UI 或评测公开材料。

诊断原始输入/输出只在显式授权开发模式下保存，默认脱敏、限定期限与访问；不得把完整用户素材和密钥提交 Git。

## 19.3 运行状态指标

至少观测：首次可采用成果率、首次稿人工介入量、任务误解、无依据断言、真实资料读取率、返工次数、无效重复调用、恢复成功率、重复发布数、p50/p95 时延、token 与费用、工具可用性。

“调用搜索次数高”“输出文件多”“过了 N 个测试”不作为营销能力指标。

---

# 20. 对外 App Server 协议（新增目标）

保留原生 thread/turn 接口，新增 namespaced `marketing/*`，不把 HTTP REST 服务器变成另一个运行时。

| 方法 | 最小参数 | 返回 |
|---|---|---|
| `marketing/project/create` | name, authorizedPath? | project |
| `marketing/project/read` | projectId | project + capabilities |
| `marketing/mission/read` | missionId, sinceRevision? | snapshot |
| `marketing/mission/bind` | threadId, missionId, expectedBindingRevision | binding |
| `marketing/mission/pause` | missionId, commandId | snapshot |
| `marketing/mission/resume` | missionId, commandId | snapshot；不重发未知外部动作 |
| `marketing/mission/cancel` | missionId, commandId | cancellation receipt |
| `marketing/artifact/read` | artifactId, version?, offset?, limit? | metadata + bounded content/ref |
| `marketing/artifact/export` | releaseId/versionId, destination grant | export receipt |
| `marketing/approval/decide` | approvalId, previewDigest, decision, client nonce | trusted decision receipt |
| `marketing/events/list` | missionId, afterSeq, limit | events + next cursor |

方法名是拟新增协议，不代表当前 App Server 已支持。协议 types、Rust handler、TS 生成文件、端到端测试必须一起提交。

事件 envelope：`eventId / eventSeq / workspaceId / projectId / missionId / missionRevision / type / createdAt / payload`。

事件类型：`mission.updated / mission.paused / mission.blocked / artifact.versionCreated / evidence.observed / approval.required / approval.revoked / action.submitted / action.unknown / action.completed / review.started / review.completed / release.created / budget.updated`。

权威事件与状态在同一事务中落库；广播是可重试副作用。序号按 mission 单调递增，不承诺跨项目全局顺序。

---

# 21. 旧数据迁移

## 21.1 旧 WorkChain JSON → 新任务

`output/marketing-work/<workId>.json`：

- brief → mission objective 与 legacy original material；不能伪造原始用户消息 ref。
- materials → imported Source records；文件不存在时记录 missing，不创建假快照。
- results[0/1/2] → research_note / strategy / copy 工件首版本。
- needs_review 原样保留。
- start_at 只存 legacy metadata，不变成任务状态。
- 旧 body 中的“已发布/已完成”只能作为旧文本；没有回执不升级为 ActualResult。

导入键 = workspace + legacy workId + 原始 JSON hash。重复导入同一文件返回已导入结果；同 workId 不同 hash 进入新导入版本/冲突处理，不覆盖。

## 21.2 ContentPackage 兼容

保留现有六种 ClaimStatus 和旧 Readiness 读取。旧 `ReadyForHumanReview` 不映射为 verified/accepted。`ContentPackage` 作为一种兼容导出视图，不作为所有产品回复的总 schema。[S10]

`ActionFunnelStep.next_action` 保留为受众动作，不因去掉 Agent 固定路线而删除它。

## 21.3 切换顺序

备份 → dry-run 输出数量/丢失材料/冲突 → 导入事务 → 核对引用与正文 hash → 启用新读写 → 旧 JSON 只读。

旧 adapter 与新 service 不同时写两份状态。迁移失败可以回到旧只读界面查看，但不能假装新功能已生效。

---

# 22. 测试矩阵与上线门槛

## 22.1 机械测试

| 类别 | 必测场景 |
|---|---|
| store | CAS 冲突、重复 command、异项目外键、取消与审核竞争、lease 失效、事件原子性 |
| 对象库 | 写文件中途崩溃、改名后 DB 前崩溃、hash 失配、丢失文件、备份恢复 |
| WorldState | 首轮、同 turn 多 Step、compact、resume、模型切换、scope 切换、Known 但 fragment 缺失、超预算摘要 |
| 工具 | 参数未知字段、越界路径、读写作用域、错误恢复、并行结果、重复 event |
| review | 旧 revision、缺 sources、合法虚构、跨来源推断、隐藏正文断言、审核超时、反复同一缺陷 |
| 执行 | 错账号、旧视频、金额变化、撤销、过期、断线未知、重复提交、取消后已在外部执行 |
| 安全 | prompt injection、SSRF、压缩路径穿越、日志密钥、shell/浏览器绕审批 |
| 客户端 | 断线重播、旧版本协议、provisional 文本、事件缺口、正确显示 paused/unknown |

## 22.2 行为回归：场景而非固定工具轨迹

至少覆盖以下 12 类（研发用案例，不写进生产 Prompt）：

1. 普通“你好”：正常回应，不分析营销数据。
2. 已有脚本只改开头：保留其余约束，不重做全部定位。
3. 陌生且时效性强的行业：识别真正任务并补必要信息，不凭名词编数值。
4. 普通且材料充分的业务：可以不搜索，直接做出成果。
5. 黄金礼品：允许礼赠关系等合适视角，不强制只能谈工艺或必须讲人性故事。
6. 雪茄/其他命名题：探索开放候选，不背金标人物清单。
7. 品牌展示/产品演示：不强迫虚构冲突。
8. 用户中途换受众或目标：旧方案失效范围合理，新稿确实改变。
9. 矛盾和伪造来源：标记矛盾，不让格式与 hash 冒充可信。
10. 长任务 compact + 重启：目标/事实/作品/授权连续。
11. 产品自营销：只能宣传经过验证的真实功能，未实现能力明确不承诺。
12. 发布/媒体付费：未经有效授权不执行，未知结果不盲重试。

原 TikTok MENA/CCA 案例作为历史回归，不能用它微调硬路由后宣称泛化。

## 22.3 主要产品比较

对 SeedEvolving 和 GLM 5.2 分别做：

- A：同模型、同底座的通用 Agent，具有正常研究、写作、自查与工具机会。
- B：同模型 + 本产品运行时/方法/状态与审核。

相同简报、初始资料、任务范围、工具可得性、预算，单列任何额外材料与人工帮助。GPT-6 作为第三种研发参考，不是强制生产竞争门槛。[S01]

首批开发集与最终留出集分开；最终集在实现前冻结，后续看过答案的案例不继续冒称 unseen。保存所有首次稿和失败稿，返工另记。

## 22.4 质量判定

客户/熟悉业务的评阅者只看作品和简报，隐藏系统标签并随机顺序。评：是否采用、是否答对任务、事实风险、制作可执行性、需要多少客户修改、表达具体度和专业质量。

研发首批建议 24 个跨任务案例做机械/行为回归，另 12 个未见任务做盲评发现问题；这是诊断样本，不足以自动证明市场普遍优势。报告逐案结果、配对差异、样本量与不确定性，不用一个平均分掩盖重大失败。

进入客户小范围试用的必要条件：没有已知 P0 数据/授权问题；恢复/取消/幂等测试通过；两种拟发布 profile 的真实调用路径通过；作品达到可采用门槛。**“B 比 A 稍好”但都不可用，不算成功。**

大规模营销效果与付费意愿需要后续真实采用/发布/结果验证，不是本地测试可以宣布的。

---

# 23. 施工顺序与停止条件

完整逐包清单在 `IMPLEMENTATION_TASKS.md`，本节给依赖：

```text
WP00 基线/旧约束裁决/能力探针
  ↓
WP01 任务与工件合同 + 事务存储
  ↓
WP02 legacy 导入与中性工作工具
  ↓
WP03 WorldState + 恢复 + 实际能力目录
  ↓
WP04 单真实任务研究/创作/交付 + 同模型初步对照
  ↓
WP05 Core 完成接口 + 流式交付边界 + 受限质量审核
  ↓
WP06 完整内容纵切与首次可采用验收
  ↓
WP07 媒体理解/制作/成片QA
  ↓
WP08 业务动作授权/预算/幂等 + 首个平台真实接入
  ↓
WP09 结果回读/用户反馈/学习
  ↓
WP10 最小正式客户端/打包/安全测试/小范围试用
```

客户端协议和 UI 可在 WP03 后独立实现，但不能先做漂亮 mock 作为业务完成证据。检索/媒体适配器可并行开发；Core 接缝、作用域、schema migration 和同一存储模块保持单一负责人。

每个工作包必须有：失败测试 → 最小实现 → 定向回归 → 实际结果/局限 → 独立 diff review → 可回滚提交。

任何工作包需要新服务、向量库或另一个执行引擎，先给出目前架构不能满足的具体失败测试；“以后可能用得上”不是理由。

---

# 24. 不能被实现 Agent 自行改变的约定

- 不更换仓库、底座或生产主模型角色。
- 不因 Core 可修改就重写整个 Codex。
- 不把业务状态库放进模型可随意编辑的项目目录。
- 不把工具输出、网页、课程和用户聊天文本当作执行授权。
- 不把所有输出强制塞回旧 ContentPackage。
- 不把三阶段换成十五阶段，也不把审核器变成第二个总调度 Lead。
- 不把旧评测 commit 当作当前可运行工具，不恢复大型 Eval Lab。
- 不为了通过测试硬编码行业名、案例答案、搜索次数或固定角色团。
- 不把审核器的一次 Pass 称为真实营销有效；不把 Token 减少单独当成功。
- 不用价格未知、无预算授权的真实媒体接口跑测试。
- 不读取/上传真实凭据来补示例配置，不在 Git 中提交 output 大媒体和用户原始材料。
- 不删除许可证、NOTICE、历史负面结果和前六版审计。

---

# 25. 最终 Definition of Done

## 内核与工程完成

一个真实 mission 能在原生 Codex 循环内创建、研究、工作、受限审核、交付、取消与恢复；状态、证据和工件作用域一致；Core 完成边界不能被重复事件、新输入或旧授权绕过；所有消耗有预算和记录。

## 内容产品完成

国内主力模型在无客户专业指导的首轮，能在约定范围交付客户可采用成果；不以任意 JSON、工件数量或分析文章代替作品；公开事实和创意边界可理解；失败可以明确定位，不需要客户调试系统。

## 执行产品完成

需要媒体时有真实可用媒体；需要发布时有经过授权的确切平台回执；需要效果分析时有来源明确的数据。目标平台未接入时，该范围不得标为完成。

## 自有 Core 维护完成

维护负责人能从固定 SHA 构建、运行定向回归、迁移和回滚；发布物可追溯；有依赖和安全修复的选择性移植流程。无需保持上游零改动，但必须能解释每个修改的失败用例、职责和回归证据。

---

# 26. 最终决策摘要

**把 v7 作为自有产品继续建设。** 让代码保存世界、保护边界、执行动作；让 Lead 保持营销判断与专业创作；让有界审核发现实质问题，但不接管路线；让真实作品与客户采用决定是否成功。

不是“只改 Prompt”，不是“为了原生全部写进 Core”，也不是“先造齐所有模块再等效果出现”。第一条短闭环必须同时证明：任务理解正确、资料使用合理、作品可采用、成本可承担、出错可恢复。

这份文件冻结的是可施工的接口和责任边界，**不宣称尚未跑过的模型质量、供应商权限、平台发布和设备兼容已经成立**。这些验证被明确放进对应工作包，不能用猜测替代。
