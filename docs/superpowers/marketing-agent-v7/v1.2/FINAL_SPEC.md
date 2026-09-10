# Marketing Agent v7：业务与自有内核融合实施规格

**版本：1.2 · 2026-09-10 · 业务与工程融合修订**  
**适用仓库：** `yangshengguo46-stack/marketing-system-v7`  
**核对分支：** `codex/upstream-sync-20260910`  
**核对提交：** `e08f1e1ad5bbc3ea036aa271894d8228e499ed61`  
**性质：** 目标架构、接口合同、迁移和实施规格；不是已实现声明，不是完成验收报告。  
**维护决策：** 允许修改 Codex Core；不再要求持续合并上游，由本项目维护自己的内核。

> 交付目标：不是再造 Marketing OS，不是用聊天调用固定营销工作流，也不是把通用模型裸露给客户。基于已有 Codex 执行循环，使一个 Lead 能理解营销任务、获得可靠材料、选取专业方法、完成作品、处理必要授权，并依据真实结果调整工作。
>
> 本规格把“研发必须按顺序施工”和“产品中的 Agent 不被固定营销顺序控制”分开。工程依赖要严格；营销研究与创作路径由 Lead 决定。

## v1.2 使用顺序与覆盖关系

先读第 B 章“掌管营销：业务能力与架构融合”，再读工程章节。第 B 章将本次用户附件中的 Actor、Transformation、Primitive、Operator、Belief、Candidate、Intervention 等业务设计与仓库现有编剧资产接入同一 Lead；不是第二个 Marketing Host，也不是营销 MCP。

本版替换了第 9.3/9.4 节，补充第 12 章业务验收与第 23 章施工安排。未改写的工程合同继续有效；若旧句子把业务方法推迟到 WP06、把三阶段当必经流程、或暗示只有通用 Agent 身份，按本版业务章节与修订任务单执行。

`contracts/` 仍是 v1.0 工程合同起稿；本版业务类型、资料导航与原生接入尚未写成已编译补丁。`BASELINE_v1_0_VALIDATION_LOG.txt` 是先前合同验证记录，不能用来证明 v1.2 的方法有效、Rust 集成通过或作品质量合格。本次未修改 GitHub 仓库，未运行生产模型和平台动作。

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

# B. 掌管营销：业务能力与架构融合

## 0. 本次决定：保留业务思想，替换旧接法

用户本次重新给出的“掌管营销的神”方案不是无效资料。其中 Actor、Transformation、Trigger、Primitive、Tension、Belief、Operator、Event、Perspective、Candidate、Intervention 和 Learning 共同描述了一套专业工作方式，应成为产品的核心业务设计。[B01]

保留这些概念，不等于恢复附件中较早的 Marketing Host、外置 Orchestrator、营销 MCP、固定候选配额或十五步执行链。用户后来已确认 Chat 是主入口、一个 Lead、v7 原生改造、Core 可动、首轮完整交付。本版按这些新决定调整接法，并明确说明调整，避免把旧方案整个重做或整个否定。

最重要的区分：**不固定工作流程，不等于没有专业立场、方法和作品标准。** 自主权回答“谁选择路径”；业务能力回答“依据什么作出好的选择”。不能为了自主而把专业内容删到只剩一句“请自行完成任务”。

本版取代先前 FINAL_SPEC 中过于概括的方法库、主身份、业务质量维度和方法后置的施工安排。存储、作用域、授权、费用、恢复、原生执行循环和既有安全边界继续沿用；不新建 v8，不整体迁入旧运行时。

## 1. 这份思路为什么像编剧课，又为什么不等于编剧课

课程导读把其中心概括为：观察具体的人，让他为在乎的东西行动；事情检验办法、关系或信念，观众因此产生理解和感受。该概括是项目对课程的整理，不是老师原话。[B02]

附件则进一步要求：明确现实中相关角色的处境、期望与障碍，设计营销干预，并观察结果。[B01]

| 附件的业务概念 | 课程中可以借鉴的能力 | 不能偷换的边界 |
|---|---|---|
| Actor、Situation、Trigger | 理解具体人物、处境和行动起点 | 故事人物不自动等于现实客户或内容观众 |
| Transformation、Motive、Blocker | 欲望、困难、办法和选择 | 课程解释不了某类客户实际上为何购买；客户动机仍是待核对的解释 |
| Tension、Causal Exploration | 行动、回应、后果和关系变化 | 戏剧因果不证明现实因果，更不证明转化因果 |
| Perspective、Belief | 知情位置、理解、期待与信息组织 | 人物说自己改变，不等于观众改变；观众理解不等于品牌偏好 |
| Narrative、Artifact | 写出可呈现的事件、场面、对白和完整作品 | 不能用内容梗概或创作说明代替实际正文 |
| Intervention、Offer、Distribution | 课程可辅助表达和观看体验 | 产品价值、报价、渠道、信任证据和行动承接需要业务材料与验证 |
| Learning | 根据实际稿件返工、辨认误判与修坏 | 客户采用、可拍摄、传播和成交必须分别记录 |

**四个对象分开：**现实业务中的 Actor；作品中的 Character；实际观看的 Audience；提供价值与被识别的 Business/Subject。它们可以重叠，但只能由具体任务建立联系，不能由同一个字符串默认合并。

同样区分三种因果：作品内情节因果、外部事实之间的因果、营销动作对业务结果的因果。一个故事写得自洽，只支持第一种判断。

## 2. 融合后的业务架构

```text
Chat：目标、材料、纠偏、审阅、授权
  ↕
自己的 App Server / Codex Core
  └─ 唯一 Lead 的原生模型—工具循环
       ├─ 专业身份：默认知道以人的处境、业务价值与成果来判断
       ├─ 工作认知：看到当前目标、角色、假设、候选、证据与工件
       ├─ 专业资源：可读取营销方法、课程全文、完整示范和反例
       ├─ 行动能力：原生搜索、文件、计算、媒体与获准外部操作
       └─ 作品检查：核对正文、商业连接、事实和制作条件
             ↕
      ai-ip-domain / ai-ip-runtime / 已设计的 ai-ip-store
      类型与来源、持久化、方法材料装配、工件、授权、执行与回读
```

这里没有一个先于 Lead 决策的独立 Marketing Kernel 进程。“Marketing Kernel”在本版中保留为**领域能力的统称**：专业立场、可用方法、业务表示、证据、作品标准与反馈共同进入同一个 Lead。

业务知识主要由模型运用。代码负责读取、约束、来源、保存、计算与执行，不用关键词、枚举或硬分数代替营销判断。

## 3. 按附件原有概念逐项落位

| 原业务概念 | 保留的业务职责 | 架构中的位置 |
|---|---|---|
| Business Understanding / Objective | 弄清实际业务、真实能力、客户本次目标；区别长期影响力与当前交付 | 任务解释、产品事实、专业身份；现有 mission + 可选 MarketingContext |
| Actor / Actor Graph | 区分购买、使用、影响、受益、观看等角色，以及彼此关系 | 任务中的稀疏角色记录；不引入图数据库 |
| Transformation / Trigger / Blocker | 解释谁从何种状态向何种状态转变、为什么现在、被什么阻挡 | 带来源和不确定性的机会假设 |
| Human Primitives / Tension / Belief | 提供动机解释、选择张力、当前理解与目标理解的词汇 | 专业词汇与假设，按需使用；不做品类到人性的固定映射 |
| Exploration Operators | 在方向不足或相似时换主体、追因果、换视角、类比或找反例 | 由 Lead 阅读并使用的探索方法，不是每个算子一个 LLM Tool |
| Event Research / Causal Explorer | 从材料和检索中发现命名事件与机制，保留未想到的新入口 | 现有搜索/阅读/证据能力，加研究方法与来源记录 |
| Perspective Exploration | 同一事件可怎样被理解；实际作品让受众看见什么 | CreativeCandidate 的视角与观看假设 |
| Candidate / Ranking / Cliché | 比较真正不同的表达机会，判断与当前目标是否匹配 | 可版本化候选与简短取舍理由；不做固定 Top N 总分器 |
| Narrative / Production | 将方向写成具体且完整的作品，必要时组织视听制作 | 现有编剧方法、课程、工件和媒体能力 |
| Intervention / Offer / Distribution | 判断内容、演示、报价、试用、页面或渠道哪种动作更有用 | InterventionProposal + 既有行动授权与执行 |
| Learning / Marketing Memory | 保留结果、解释与不确定性，调整后续判断 | 结果回读与有来源的学习，不把单次相关性记为规律 |

这些行不是阶段序号。已有稿件只需改开头时，不要求先新建角色、原语或跨域故事。

## 4. 把 MarketingState 变成业务工作记忆，而不是第二个数据库

先前工程规格已有 Mission、Claim、Artifact、Decision、Action 和 Observation 等职责。业务层就在同一持久化体系内增加少量类型化载荷，不复制一套全局 MarketingState Store。

建议只增加四组业务表示，均为新增目标，不是当前源码已有类型：

### 4.1 MarketingContext

包含目标解释、可选角色、重要处境、Transformation 假设和当前业务限制。外壳有 schema_version；scope、revision、来源、用户输入版本沿用已有 work 事务。

角色是当前任务相关角色，不是完整客户画像。每一条重要陈述使用现有六类 ClaimStatus，并分别引用来源。不能只给整份 JSON 一个“verified”标签。

### 4.2 OpportunityHypothesis

当 Lead 认为值得保存时记录：相关角色、具体触发、当前状态、期望或想保住的状态、阻力、可能动机、替代解释、真实产品机制和重要未知。

Transformation 的 WHO/FROM/TO/TRIGGER/BLOCKER/MOTIVE/MECHANISM 是解释视图，不是必须先填完的七列表。某项不适用可以缺省；尚未知晓要注明，不能为过 schema 编造。

### 4.3 CreativeCandidate

它是“值得发展或比較的表达机会”，不等于一段文案，也不必总是故事。建议的内容合同：

```text
candidate_id
purpose                         本候选服务什么目标
opportunity_ref?                从哪个业务处境出发
angle?                         看待事情的视角
premise                         准备用什么事情、观察、解释或演示表达
expected_audience_effect?       希望受众理解/记住/相信什么；只是预测
business_bridge?               为何与此品类、主体和实际能力有关
source_refs[]                  现实事实依据；虚构另标
alternatives_and_limits?       重要替代解释或不成立边界
artifact_refs[]                实际作品版本
selection_note?                Lead 的简短取舍，不是完整推理轨迹
```

`business_bridge` 分开记录：品类相关性、主体/品牌归属、实际产品贡献、证据范围。不能从“这个故事与礼物有关”直接跳到“它证明了这家黄金品牌更好”。

动机词、事件、冲突、故事角色和 before/after 信念都允许缺省，不因缺少它们拒绝演示、说明、报价或局部改稿。

### 4.4 InterventionProposal

记录：目标受众/角色、当前障碍或机会、目标变化、所选手段、实际机制、渠道、内容/页面/试用等工件、可选下一受众动作、结果观察方式。

这是业务提案，不是执行许可。发布、费用、账号和版本授权仍由原工程方案负责；模型写“已同意”“会提高转化”不能制造授权或结果。

### 4.5 复用现有对象的方式

当前 `ContentPackage` 已有 influence_relation、action_funnel、strategic_judgment、publishable_content、claims、measurement_plan 和 readiness。[B06]

保留其 v1 读取，不原地增加字段破坏 `deny_unknown_fields`。初始实验先将新对象作为带 schema_version 的工件载荷，用原生文件能力和现有保存约定演示；正式服务实现后迁入同一 work/store。后续需要正式对外对象时，再定义版本化 `MarketingDeliveryV2`，引用候选、干预和工件；内容分支可以包含原 ContentPackage，其他任务不必被强行塞成短视频内容包。

`ActionFunnelStep.next_action` 指受众的行动，继续保留；它不是 Agent 的下一阶段。[B06]

## 5. 真正的出厂专业能力：四种东西缺一不可

### 5.1 默认专业立场

不能让用户先说“请使用场景营销方法”。营销产品 profile 在首轮就加载简短专业立场和资源导航。

建议的身份起稿：

```text
你是负责本次营销业务与作品交付的 Lead。用用户的实际目标、事实、产品能力和制作条件作判断。
不要只把品类卖点改写成文案。需要寻找方向时，理解具体的相关角色、处境、触发、选择与障碍；人的动机、信念和受众效果是有依据的假设，不是可凭行业名称断定的事实。
你可以从人、事件、产品机制、数据或已有作品进入。原语与思维算子用于扩展解释和发现机会，不要求每次使用，不强迫所有任务故事化。
区分业务角色、作品人物和观看受众。选择作品视角与具体事件时，同时考虑观众能够实际读到什么，以及它为何与当前主体和业务有关。
专业方法、编剧课程和示范可按实际问题读取。既要创造值得发展的可能，也要检查事实、类比边界和作品本身；不能让安全措辞代替好作品，也不能为了新奇硬接商品。
普通创作选择由你承担，只有客户独有的关键事实、目标冲突和后果性授权才交回用户。用户要求作品时交付完整正文或约定成品，而不是止于分析表、方向列表或创作说明。
使用实际可用能力；保存必要的证据、重要判断和作品版本。发布与付费依主机授权执行。解释效果时区分模型预期、客户采用和真实业务结果。
```

这是目标 profile，不是性能保证，不直接等同于当前 `prompt.rs` 的评测输入。最终长度由实际模型调用和对照调整，不重复堆入旧失败案例。

### 5.2 专业方法与完整示范

方法必须实际说明“怎样观察、怎样取舍、怎样写、怎样发现错误、哪里不能用”，而不仅是名字。第 8 节给出了可实现的首批内容。

### 5.3 真实材料与开放研究

老师的例子不等于客户资料，旧案例清单也不是世界的边界。课程用来学创作判断；材料、检索、用户信息用于了解本次业务。不要先凭闭卷召回列完人物，然后只搜自己已经想到的名字。可以从具体处境和关系问题检索，再从实际阅读发现新角色、新事件与相反解释。

### 5.4 作品标准与生产模型能力

软件架构和课程材料不能替代模型的实际执行能力。生产优先 SeedEvolving、GLM 5.2 的候选分工沿用用户既有约定，实际账户和模型 ID 以运行配置为准。GPT-6 仅研发参考，不暗中成为生产质量补丁。[B08]

客户空历史记忆下仍应得到有用的第一份作品；品牌记忆提升后续贴合度，不承担出厂专业能力。

## 6. 第七版已有编剧资产怎样真正接入

保留这些现有目录和文件，不另写一套自拟课程替代：

```text
output/course-learning/charlie-20260906/全课课件/
output/course-learning/charlie-20260906/清洗讲义/
output/course-learning/charlie-20260906/完整课程/
output/business-methods/story-work-v1/完成与返工故事.md
output/business-methods/story-work-v1/依据正文作创作取舍.md
output/business-methods/story-work-v1/一次完整返工的做法与边界.md
```

本次完整读取了全课导读、前两份方法正文、方法 README；抽读了第 25 课的前段。第 7 课部分内容在上一轮读取；本版不声称重新读完 67 章、听完 63 小时或完成全部课程效果验证。

### 6.1 新增导航而非重建知识平台

建议新增 `ai-ip-assets/business/manifest.json`，索引字段包括：

```text
id / version / question_it_helps / applicability / limitations
source_paths[] / source_anchors[] / complete_context_paths[]
asset_digest / validation_status / deployment_profile
```

状态至少区分 source_reviewed、candidate_method、business_tested。生产部署开关与资料存在性分开。原有 story-work-v1 当前仍是候选方法，不因本次讨论自动改成已验证。

目录不能只叫“编剧课”。应让模型看见可解决的具体问题，如“方向有主题却没有事情”“角色反应重复”“观点依靠附文才成立”“开头抓人却不属于这个业务”。模型再通过既有文件或 Skills 读取相关资料，不需要新建一个会自己思考的 course-research Agent。

### 6.2 课程映射以完整上下文为单位

| 当前作品问题 | 已有资料入口 | 使用后必须落在哪里 |
|---|---|---|
| 只有抽象性格或主题，没有事件 | 第7课、`完成与返工故事` 的事件展开部分 | 正文中的动作、回应和后果 |
| 场面只靠短信或台词解释，人物没有应对 | 第14/34课与 P001，沿现有方法链接回读 | 具体场面；同时保留适用的旁白和简写 |
| 概念不错，却读不出人物为什么这样选 | 人物、欲望、关系与整体修改资料 | 关键选择与结果，不追加说理尾巴 |
| 作者想让观众感动，但正文未建立这种感受 | 第25/27课、`依据正文作创作取舍` | 观众的知情位置、等待与实际可读内容 |
| 改动一处导致后文资源或行为不成立 | 现有返工方法与课堂变体上下文 | 修订后的整稿和必要的连带检查 |

表中课程是读取入口，不是“出现词语X就执行第Y课”的自动路由。先读取任务和实际稿件，再自主选择。

### 6.3 三层阅读

短导航告诉模型有什么；方法正文说明怎样处理某个具体问题；完整课节、案例变体与例外提供深入核对。长文件按段读取并记录范围，不能把摘要假装成全文。

课堂中的教师试设、学生原稿、真实事件和本次生成的虚构各自标记。来源缺失的学生原稿不补造；不同版本的故事不混成一个“课程标准答案”。课程文字不提升为系统权限指令。[B02–B04]

### 6.4 不把现有阴性结果抹掉

story-work-v1 README 记录：两道新题的首稿比较，两位匿名模型编辑小幅偏好不带课程的 GPT-6；该小样没有显示通用步骤对强模型的额外质量收益。[B05]

这不能证明场景营销无效，也不能证明国内生产模型无收益；同样不能宣布加课就有壁垒。新接入的业务表示、研究方式和方法，应在同模型条件下验证，原稿、外部帮助和阴性结果一起保留。

## 7. Primitive、Operator、Belief 怎样不再停留在名词

### 7.1 Primitive 是解释人的在意，不是商品分类

记录一项动机时，至少能说清它对应哪位角色、哪件事、什么观察，哪些替代解释也可能成立。没有证据时保留为假设。并非每个候选都必须有 primitive。

例如“采购者担心礼物失礼”与“采购者担心实物和确认图不一致”是不同解释。前者可能适合关系表达，后者可能更需要真实样品和交付过程证据。不是黄金天然等于体面。

不沿用附件的 0.87、0.82 等未经校准数字；候选比较可用解释性判词、粗粒度有锚点等级，但不能把模型自评变成测量值。

### 7.2 Operator 是可选择的思考动作，不是 API

| 算子 | 实际动作 | 必须避免 |
|---|---|---|
| 换主体 | 站在付款者、接受者、观看者等不同角色的位置，看同一行为如何被理解 | 只把职业、性别或地点替换，关系结构不变 |
| 追原因 / 看后果 | 检查行为在前后的条件、回应与代价；寻找替代解释 | 把“先发生A后发生B”直接写成A导致B |
| 换尺度 / 跨域类比 | 对应角色、约束、行为和关系结构，寻找可解释的相似处 | 只因两个领域都有“信任”就硬连；把另一个领域的证据当本产品证据 |
| 换视角 | 改变观众跟谁知道事情、什么时候获得哪条信息 | 只更换旁白人称，不改变理解过程 |
| 反事实 / 找反例 | 问某个关键条件不存在时，判断是否还成立 | 为了反转而反转，或用虚构反例否认真实证据 |

“Primitive × Operator”保留为探索机会，不执行全笛卡尔积，不要求先建齐24个词和20个算子，不承诺搜索若干条就必然出现好创意。

### 7.3 Belief 要区分预测和观察

`belief_before` 只能是有来源的观察或当前假设；`belief_after` 是希望内容帮助受众形成的理解。实际变化必须来自受众反馈或可解释的结果证据。不是故事角色顿悟，所以现实受众已经信任品牌。

## 8. 可直接开始撰写与接入的首批业务方法

以下为本次新增目标方法起稿；不是查理老师原话，不是已验证算法。方法没有相互强制调用链，具体案例数量由任务决定。

### M1：从品类表述进入具体处境与选择

**适用：**用户只给业务描述，或现稿只有卖点、人群标签和泛泛痛点。

**做法：**先区分用户实际业务和本次内容任务；读真实能力、限制与材料；辨认当前最相关角色和触发事件；描述他想获得/避免/保住什么，哪些选择互相牵制；留下一两个会改变决定的未知。可以从产品机制或数据反向发现处境，不要求先画完整人群地图。

**产出：**简短可修订的机会假设或直接可用的创作选择。不要求给客户呈现全部内部表格。

**反例：**现有稿件只需要缩短一句时，直接改；商品报价任务不能被改成客户人生故事。

### M2：从处境发现事件、机制与跨域入口

**适用：**当前方向停留在品类常识，或需要值得受众关心的具体材料。

**做法：**把处境写成一个可研究的问题；阅读材料和检索结果，不局限于先想出的名人清单；发现新角色、事件或反例时可以改题；类比只比较确切结构，写清共同关系与不可迁移部分。现实事件缺证可继续作为待研究候选，不能作为已发生事实发布。

**产出：**带来源的事件/观察，或者明确标注的虚构创意；候选说明“为什么这群人值得看”和“为什么属于这项业务”。

**反例：**故事新鲜但受众无关，或与产品只剩同一个词时弃用；已经有明确实物疑虑时可以直接做证明，不继续找历史。

### M3：从事件形成视角与商业连接

**适用：**已有事情，却不知道从哪里讲、看完留下什么。

**做法：**分开真实受众和作品人物；说明希望观众在过程中知道/等待/重新理解什么；选择观众的信息位置；检查从具体事件到当前业务的联系，分别说明品类相关、品牌归属和产品贡献。允许品牌表达不直接销售，但要知道本次承担哪一层目标。

**产出：**CreativeCandidate。多方案的不同应落在关注的问题、理解路径或作品形式，不是同一稿换标题。

**反例：**“删掉产品仍能成故事”不是一票否决；品牌纪录、态度表达可以间接关联。但不能把可替换品牌的一段故事称为已证明该品牌差异。

### M4：把方向写成完整、可呈现的作品

**适用：**已选定或明确要探索一件叙事作品，或已有稿需要展开。

**做法：**优先使用现有 `完成与返工故事.md` 并回读相关课程；把抽象态度变成选择、动作、回应与后果；安排观众知情；写完关键过程而不是用“努力之后终于成功”略过；需要视频时把画面、声音、对白、表演、节奏与资源写到可执行程度。

**产出：**完整作品正文及必要制作说明。不是新增一份“剧情生成完成”JSON。

**反例：**不强制三幕比例、所有人物成长或情绪必须更惨；已确定写演示、说明或报价的任务，不因课程存在改成故事。

### M5：从完整正文判断采用或返工

**适用：**已有完整稿件，需要决定是否交付或怎样改。

**做法：**使用 `依据正文作创作取舍.md`。先看任务与正文，记录实际能读到什么，再看作者意图；把事实、因果表达、风格适配与实际效果分开。反馈指向具体位置、影响、证据、改法和代价。只改重要问题，保留成立部分，并回查连带变化。

**产出：**可采用的稿件，或带具体依据的完整修订稿。作者解释漂亮不替正文得分。

**反例：**模型意见不等于真实观众效果；不为了让审核器显得有用而强行返工。

### M6：从内容走向营销干预

**适用：**用户要获客、提升信任、增加咨询，或已有数据提示单纯增加内容可能无效。

**做法：**读取当前目标与实际观察，比较“理解不清、缺少证据、承诺不明确、行动步骤受阻、渠道/受众不匹配”等解释；选择有实际材料支持的优先动作。手段可以是内容、演示、比较、报价、试用、页面、服务流程或渠道实验。重要业务条件由用户确认，外部行动经过授权。

**产出：**InterventionProposal，以及它实际需要的作品、页面修改或执行方案。通过既有动作和结果机制闭环。

**反例：**咨询少不自动说明内容无聊；高播放不自动说明带来成交；没有流量或分母不能从零成交判断说服力失败。

## 9. 在一轮原生会话里如何工作

以下是一个可能的实际轨迹，不是运行时编码的统一步骤。

客户：“我是做企业纪念礼品的，不想只讲材质，给我一条能用的品牌短片。”

1. 主身份、资源导航和当前真实能力已可见。Lead 区分业务事实与本次交付，读取产品资料；只有真阻塞的问题才问用户。
2. Lead 提出与任务相关的处境假设，可能使用 M1；发现采购者、受礼者、观众的关注不同。
3. Lead 根据目的选择探索材料、写一场或做演示。若需事件研究，使用现有检索/文件能力，返回的外部材料进入证据记录。
4. 值得保留的选择形成稀疏 Candidate。随后可直接写稿，也可以回读课程、继续验证或换形式。
5. 选用叙事后，按 M4 使用已有编剧方法；完成稿件保存为原有 Artifact。Candidate 引用该版本，Intervention 说明它希望承担的业务作用。
6. 完成审核先读任务和正文，不靠 Candidate 的“很感人”评价替稿件打分；发现关键缺口，回给同一个 Lead 修订。
7. 用户纠偏时更新实际业务解释和相关候选依赖，不抹掉已读取的真实资料；授权仍只绑定确切版本和动作。

必要的短决策记录可保存“为什么选/不选”；不收集或输出模型完整私有推理。

## 10. 一份贯穿示例：作品中的人物、真实观众和商业目标怎样对应

**全部为本设计的虚构演示，不是用户产品事实、客户研究结果或营销效果证据。**假设某品牌确有可展示的定制纪念牌样品，能刻录经客户确认的短文字。没有这项能力时不可采用该产品表现。

### 10.1 业务处境与选择

客户任务：一条企业退休纪念的品牌短片，重在表达理解与纪念，不追求立即成交。

业务付款角色：企业采购或组织者；受礼角色：退休同事；观看人群：可能包括组织者和经历过此类场合的普通观众。不能将三者自动合并。

机会假设：当集体准备统一的“感谢付出”文字时，某个具体的人可能更在意自己曾经真实参与、帮助过的事情是否被记得。这个假设只服务示例，不能当普遍人性结论。

Transformation：从“礼品完成了一场仪式”，到“礼品指向这个人的具体贡献”。这是表达目标，不是已发生的受众变化。

Operator：由组织者视角换到受礼者，再检查组织统一表达与个人具体经历之间的差异。无需跨国历史故事，也无需给“认可”打0.92分。

### 10.2 候选与取舍

- 候选A：材质和包装展示。适合品质说明，但未回答本次想要的纪念表达。
- 候选B：夸张演绎主角拯救公司。缺少真实材料且容易让品牌表达失真，不采用。
- 候选C：一份惯常感谢词，被一句双方记得的具体小事替换。能够形成简短场面，选作虚构创作方向。

这不是固定三候选制度，只是本示例把取舍展开给施工者看。

### 10.3 完整短片起稿《这一句》

时长只是创作预估，须排练核定。

| 片段 | 画面与行动 | 对白/声音 |
|---|---|---|
| 起点 | 办公室里，年轻同事对着纪念牌确认稿。屏幕上是一段整齐的感谢词。他删掉一个词，又换回去。 | 鼠标和键盘声。 |
| 事情进入 | 退休同事走过，伸手扶住桌边一杯快被纸袋带倒的水。年轻人立刻把电脑往里挪。 | 年轻人：“刚来的时候，你也救过我一次电脑。” |
| 关系具体化 | 对方笑着把水杯放到另一边，顺手整理松动的插线。 | 对方：“你那时杯子也放这里。” |
| 选择 | 年轻人看向屏幕，没有再改“卓越”“奉献”这些词。他另开确认稿，写下一句，停住看了一会。 | 不用旁白解释他受了感动。 |
| 交付场面 | 告别时，对方从盒中取出纪念牌，背面刻着：“谢谢你，替我接住了很多第一次。”他抬头，看见年轻人已经把水杯摆到了远离电脑的一侧。 | 对方：“这回放对了。” |
| 收束 | 镜头落到牌上经确认的文字，再回到两人继续收拾桌面的动作。品牌标识适度出现。 | 保留自然声，音乐与节奏按品牌气质另行确定。 |

这是可继续审阅的示例，不宣布已达商业发布质量。潜在问题也要保留：表达可能过于轻，杯子事件是否足以支撑“很多第一次”，是否需要额外但不过度的前文，文字是否显得作者代言。应先读实际稿件判断，而不是靠附文宣布其感动成立。

### 10.4 产品与品牌没有被“自动证明”

该稿让定制文字承担具体的纪念作用，但它主要支持品类意义和定制表达；没有独自证明黄金优于其他材料，更没有证明这家品牌优于其他供应商。要承担这些命题，需另有真实材料。

若客户改成“采购者只担心实物与确认图不一致”，同一 Lead 应改做真实确认图—样品—成品的对照，不再拍上面的情境片。这正是 Intervention 层高于故事层的意义。

## 11. 冷启动具体会看到什么

每步可见内容分层：

1. 稳定的专业立场与短资源目录：产品出厂资产，不依赖客户记忆。
2. 当前任务 WorldState：本次目标、角色/假设、已选表达、相关工件、证据缺口、授权与预算。
3. 已读取的方法与材料：按需进入，带来源和范围，不无上限复制课程正文。
4. 实际成果与反馈：具体正文、检查结果和用户纠偏，直接影响下一个动作。

先前工程投影如果只注入 missionId、状态和文件路径，专业上下文仍然贫乏。本版要求将当前已经形成、对下一步重要的业务解释投影进去；没有形成的不能用主机固定字段替模型编出。

示例投影：

```text
目标：完成企业纪念品牌短片，不是采购报价。
已知：产品定制能力来自资料S1，尚无真实客户故事可引用。
角色：付款者与受礼者不同；观众假设尚未验证。
机会：仪式性感谢与具体记忆之间的差异（假设）。
选择：写虚构场面；不声称真实客户经历。
作品：artifact-7@revision-2。
待处理：正文中的具体事情是否足以承载“被记得”；品牌归属是否成立。
资源：可读取“完成与返工故事”及完整课节；不强制调用。
```

不包含程序指定的 next_stage；模型提出的下一步计划可保存并随时修订。

## 12. 代码该做什么、不能做什么

### 12.1 建议文件映射

| 文件/位置 | 状态 | 本次职责 |
|---|---|---|
| `ai-ip-assets/skills/deliver-ai-ip-content-package/SKILL.md` | 已有 | 保留已有 judgment loop，不再另造相反总流程；补充专业资料导航与版本化兼容说明 |
| `ai-ip-assets/business/lead-context.md` | 新增目标 | 首轮专业立场；不夹带行业金标答案 |
| `ai-ip-assets/business/manifest.json` | 新增目标 | 现有课程/方法和新增营销方法的精简导航、来源、版本与试用状态 |
| `ai-ip-assets/business/marketing-judgment.md` | 新增目标 | M1/M2/M3/M6 的具体业务方法、反例与限制 |
| `codex-rs/ai-ip-domain/src/marketing_context.rs` | 新增目标 | 角色、处境和Transformation等可选载荷，不是前置必填大schema |
| `codex-rs/ai-ip-domain/src/creative_candidate.rs` | 新增目标 | Candidate、观看假设、商业连接、工件引用 |
| `codex-rs/ai-ip-domain/src/intervention.rs` | 新增目标 | 干预提案，执行仍引用已有Action与授权 |
| `codex-rs/ai-ip-runtime/src/business_context.rs` | 新增目标 | 装配专业立场、当前业务投影与资源目录；不调用模型做前置营销路由 |
| 原计划中的 `world_state.rs`、`review.rs`、`evidence.rs`、`work_service.rs` | 待按原工程包实现/扩展 | 分别承载当前业务认知、作品检查、来源与原子保存；不另建服务 |
| `codex-rs/app-server/src/extensions.rs` | 已有 | 装配同一业务服务与原生循环，具体函数签名以当前源码实施 |
| Core WorldState 与 completion 接缝 | 已有机制/拟扩展接口 | 通用上下文和完成控制；不编码黄金、TikTok、原语匹配等策略 |

当前 Lead Skill 实际已包含主体/受众/动作、真实机会、具体主题与形式、诚实商业连接，以及允许跳过/回访的原则。[B07] 这是可继续的骨架，不应声称毫无业务能力。欠缺的是本次补充的具体方法供给、可保留业务对象和被验证的运用。

### 12.2 工具不增加“思考按钮”

不新增 `infer_primitive()`、`generate_tension()`、`score_creativity()` 这类各自再次调用LLM的业务工具。先复用原生阅读/检索和原计划的 work/artifact/evidence 写接口。

运行时可以增加纯数据命令，如保存候选或干预；这些操作只校验、存储和返回引用，不自行决定谁是目标客户、哪种视角最好。

方法选择也是 Lead 的工作。将来若做检索推荐，它只能提供相关资源候选，不能变成必经营销路径。

### 12.3 最小保存接口的概念合同

```text
save_business_context(expected_revision, patch_with_provenance)
save_candidate(expected_revision, candidate_payload)
record_selection(candidate_refs, selected_ref, short_rationale)
propose_intervention(expected_revision, intervention_payload)
```

这些是新增操作语义，不是声称当前 native tool 已有此命名。可合并进既有 work 命令协议；由主机注入 scope，禁止模型指定 owner。开发者需按现有实现适配，不新增第二套授权/事件/数据库。

## 13. 业务验收：不是查十五个字段有没有填

审核依据当前任务、完整作品和相关来源，并保留“作者意图”与“正文实际效果”的分离。[B04]

| 交付类型 | 主要业务检查 | 不应强求 |
|---|---|---|
| 叙事/场景短片 | 具体事情、人物选择与回应、观众知情、可呈现性、商业连接 | 所有人物成长、每段反转、必须悲伤、固定三幕比例 |
| 演示/证明 | 所承诺的机制与演示条件是否一致，证据是否真实，顾虑是否被回答 | 历史故事、远距离联想、人性标签 |
| 商品解释/报价 | 决策信息、实际条件、差异与不确定性、行动清晰度 | 对常见但有用的材质/价格词自动判俗套 |
| 品牌/IP方向 | 长期能讲的问题、受众价值、主体资格、与已有资产的联系 | 每个候选都承担立即成交，或交付一堆主题词即算完成 |
| 数据诊断/转化修改 | 观察口径、替代解释、动作与障碍的匹配、可回读结果 | 默认新增视频，或把模型归因当实验结论 |

`semantic_distance`、`story`、`tension`只能按任务作为取舍维度，不是统一总分公式。“足够远、能回来”保留为探索类创意的启发，不定义所有营销质量。

审核把缺口返回当前 Lead；不另设拥有策略权的 Writer Brain。可计算和硬安全错误由代码检查；风格、创意和有效性依任务判断，真实受众与业务结果另行验证。

## 14. Intervention 才是“掌管营销”而非“掌管编剧”

同一套业务表示也应允许下面这类任务：

用户：“最近有浏览，但咨询少。”

Lead 读实际数据和页面后，可以把“受众没有理解提供什么”“不信承诺”“下一步太麻烦”“来的不是目标人群”等保留为竞争解释；证据不足时不先定诊断。它选择的动作可能是改页面、展示真实过程、调整试用或报价说明，而非追加一条故事视频。

Offer：把真实价值、适用对象、价格/服务/交付条件讲清楚，不擅自承诺折扣和服务。

Distribution：选择与当前目标、受众和素材条件相配的渠道/形式，平台最新规则另行核实，不把历史课程或旧运营经验当当前政策。

Learning：保存“观察到了什么”与“猜测为何”两条线。没有对照，不能从一条高播放内容推导“这个原语有效”“45秒后植入最好”。附件的Global/Vertical/Brand三层经验保留为作用域，但生产资产、来源事实、候选解释、客户私有经验不可混同。[B01]

## 15. 施工优先级修订：业务能力不是等WP06再整理

工程依赖要有顺序，产品的营销动作不固定。新增业务工作与原WP按下面方式交叉推进：

| 工作 | 何时推进 | 交付 |
|---|---|---|
| B00 业务与课程资产核对 | 与WP00同时 | 旧方案保留/替换清单、现有课程方法导航、候选状态和已知阴性结果 |
| B01 专业身份与方法实体 | 在WP01–03的最小条件上尽早试用 | 真正写好的lead-context、marketing-judgment与manifest，不留空占位 |
| B02 业务对象与记忆投影 | 并入WP01–03 | 稀疏角色/机会/候选/干预，与已有工件和claim相连；无需等全部平台 |
| B03 首次完整作品 | WP04主验收 | 同一Lead从真实简报产出可采用正文或约定交付；没有客户历史记忆也能做 |
| B04 业务审核与返工 | 并入WP05 | 先看正文、再看意图，因果/商业/事实分开，返工写回同一工件体系 |
| B05 增益与删减 | WP06，早期小样也可前置 | 同模型首轮作品比较，删掉无用资料和步骤，不恢复大型Eval Lab |
| B06 制作、外部动作与结果 | WP07–09 | 媒体、授权和结果机制承接Intervention，不另设业务总控 |

无需等自有Core所有改造完成才测试业务思想。可先用现有原生会话、已可达工具和显式文件加载作研发试验；这是验证，不得冒充自动生产集成已经完成。

每个工作包至少提交一份实际作品/业务记录和一份技术证据。纯schema/数据库测试通过，不能宣布业务工作包通过。

## 16. 证明第一轮的差异，而不是证明工程很复杂

沿同一生产模型、相同简报、相同原始资料和正常基础工具机会，区分：

A：通用Agent；B：工程框架组；C：工程框架+本次业务立场、方法与已确认专业材料。

这是隔离贡献的研发设计，不是生产中的三Agent团队。系统组多出的课程/方法本来就是处理变量，须记录；不能额外得到金标答案、客户暗示或隐藏GPT-6修改。首稿和后续返工分别比较。

观察：首次可采用程度、具体场景与角色是否相关、表达是否成立、商业连接、制作可执行性、事实错误、客户介入、耗时与成本。使用目标任务相关的判断，不要求每题都有原语和跨域故事。

黄金、榴莲、自营销已经讨论过，属于开发/回归，不再作为未见题。新业务、新表达类型、局部修改、非叙事、错误线索和用户纠偏应单独保留。小样先判断是否值得继续，不把少数模型偏好包装成稳定优势。

## 17. 当前证据边界与最终结论

**现有资产：**课程导读、讲义、故事创作/判断方法、试用结果；已有ContentPackage和简短Lead Skill。

**本次新增设计：**默认业务认知、四组稀疏业务表示、现有课程的专业导航、具体营销判断方法、业务与工程工作包对齐。

**尚未完成：**Rust代码接入、真实国内模型同条件试验、客户采用与营销效果验收。本次未修改远端仓库、未调用付费生产模型、未发起平台动作。

结论：业务不是“外挂一个编剧课”，也不是“重新建一个Marketing OS”。同一个原生Lead，以Actor/Transformation等理解业务，以Operator/Event/Perspective发现机会，以编剧和视听方法形成作品，以Intervention确定商业作用，以真实结果修正认知。

## 来源索引

- **B01** 用户本次附件《粘贴的 markdown (1)。md》：Actor与Transformation（第4–5节），Primitive/Operator（第6–8节），Candidate与评分（第18–20节），Objective/Intervention（第21–22节），Learning/Memory（第23–24节）。其MCP、外置Host、固定数量和候选列表即交付等接法，本版明确按后续用户决定调整。
- **B02** `output/course-learning/charlie-20260906/全课课件/README.md`，完整读取；blob `0f0e55d4d45dd5b95591f1cd55743d17ba7ba4b3`。
- **B03** `output/business-methods/story-work-v1/完成与返工故事.md`，完整读取；blob `6e001ab3533d8f91409f40fe3e3169cda208b6ad`。
- **B04** `output/business-methods/story-work-v1/依据正文作创作取舍.md`，完整读取；blob `ea486d753febfc6bd983b028aaa3a21c91d72189`。
- **B05** `output/business-methods/story-work-v1/README.md`，完整读取；blob `521a7b7ec588e7b865873ed810ff57ee12e1a534`。
- **B06** `codex-rs/ai-ip-domain/src/content_package.rs`，本次读取1–110行；blob `f72b7f68d453221b3fa8732c46d4ad5002ff883d`。
- **B07** `ai-ip-assets/skills/deliver-ai-ip-content-package/SKILL.md`，完整读取；blob `271e90415c934b03e37a6f3bf02e54e5812eff0e`。
- **B08** `output/business-methods/产品交付与开发验证约定.md`，前序对话已核读；国内主力、首轮价值、同模型比较和GPT-6参考边界。
- **B09** `output/course-learning/charlie-20260906/清洗讲义/第25课.md`，本次仅抽读前130行所请求的内容；不能据此声称读完本课。
- **B10** 前序交付的 `FINAL_SPEC.md` 与 `IMPLEMENTATION_TASKS.md`。本版保留其工程边界，修订业务方法深度与建设优先级。


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

## 9.3 专业资料、业务方法与课程接入

按第 B 章第 5–8 节实现，不再把业务能力留在“整理一个方法库”的占位层。保留 `output/course-learning/charlie-20260906/` 全课课件与讲义，以及 `output/business-methods/story-work-v1/` 的已有创作和判断方法；新增精简来源导航和面向当前任务的营销方法，不复制一套课程或建立知识平台。

首轮存在性可见的内容至少包括业务角色与处境、事件研究与探索、视角与商业连接、完整作品与返工、营销干预。详细方法按需读取；每个条目有来源、适用问题、限制、版本和验证状态。课程与候选方法被读取不等于已经证明业务能力。

在同一个 Lead 中使用方法，不将 Actor/Primitive/Operator 等拆成各自再调 LLM 的工具。不强制数量、顺序、原语标签、故事或CTA。客户只补独有事实与关键决定，不需要自己选课程和教方法。

## 9.4 生产主身份起稿

以下替换此前过薄的通用职责表述；它仍是待验证的业务 profile，不是模型效果保证。详细材料另按需装配，不把完整课程永久塞入请求。

```text
你是负责本次营销业务与作品交付的 Lead。用用户的实际目标、事实、产品能力和制作条件作判断。
不要只把品类卖点改写成文案。需要寻找方向时，理解具体的相关角色、处境、触发、选择与障碍；人的动机、信念和受众效果是有依据的假设，不是可凭行业名称断定的事实。
你可以从人、事件、产品机制、数据或已有作品进入。原语与思维算子用于扩展解释和发现机会，不要求每次使用，不强迫所有任务故事化。
区分业务角色、作品人物和观看受众。选择作品视角与具体事件时，同时考虑观众能够实际读到什么，以及它为何与当前主体和业务有关。
专业方法、编剧课程和示范可按实际问题读取。既要创造值得发展的可能，也要检查事实、类比边界和作品本身；不能让安全措辞代替好作品，也不能为了新奇硬接商品。
普通创作选择由你承担，只有客户独有的关键事实、目标冲突和后果性授权才交回用户。用户要求作品时交付完整正文或约定成品，而不是止于分析表、方向列表或创作说明。
使用实际可用能力；保存必要的证据、重要判断和作品版本。发布与付费依主机授权执行。解释效果时区分模型预期、客户采用和真实业务结果。
```

明确分离评测专用 `HeldOutMissionCase` prompt 与产品主身份；普通聊天正常响应，不强制生成完整业务 JSON。Core 负责正确提供专业上下文，不代替 Lead 自动推断客户动机。

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

## 12.9 业务质量补充：按交付任务检查，不按概念字段计数

实施第 B 章第 13 节。叙事检查具体事件、人物选择、观看位置和商业连接；证明/演示检查机制和证据；报价/解释检查真实决策信息；方向与品牌内容检查长期受众价值与主体归属；诊断检查数据口径和替代解释。

先读任务与实际完整正文，留下正文支持的简短理解，再比较作者意图。作者说明、Candidate里的主题和自评分不能替正文得分。事实因果、剧情因果、营销效果因果分开。偏好、可采用、可制作、传播与转化分开。

业务方法是生成能力，审核是修订边界；不能用增加审核次数代替缺失的专业能力，不因风格平静、没有反转或没有故事就阻止交付。

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

## 23.0 v1.2 业务工作前置

第 B 章第 15 节与修订后的任务单优先。B00/B01（资产与专业方法）和 B02（业务对象）应与 WP00–WP03 并行准备；WP04 的核心验收是它们共同支撑的首份完整作品；WP05 要检查业务而不只验schema；WP06 是增益与删减，不是第一次开始写业务方法。

尚无完整新内核时，可以用现有原生会话、可达工具和显式文件加载开展研发对照，不能把该试验称为生产自动接入。不要以等待数据库、所有Core改造、所有媒体渠道为理由推迟作品检验。


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
