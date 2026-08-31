# AI IP 营销能力盲评实验室设计规格

- **日期：** 2026-08-31
- **状态：** 用户已批准架构；等待 executable child plan 实施
- **产品关系：** 独立开发期评测系统，不进入 C 端 AI IP Business Server 的回答路径或发布包
- **首个纵切：** 黄金礼品／礼文化品牌 IP

## 1. 决策与目的

先建设一套独立的 **Marketing Capability Evaluation Lab**，再继续扩展 AI IP 业务运行时。它负责回答：

> 爆改后的 Codex 是否比冻结的原版 Codex 更会理解业务、寻找内容方向、形成可执行内容，并且这种改善能否被合格真人、历史结果和后续真实实验共同支持？

实验室不是新的营销总脑、固定多 Agent 流水线、客户知识库或产品外壳。候选 Codex 仍采用“一个 Lead 对结果负责＋按需调用业务能力”；评测系统只在候选回答路径之外出题、盲化、收卷、组织评审和裁决晋级。

本设计改变后续业务证明顺序：

1. 保留当前 `codex-rs/ai-ip-eval` 作为 `Blind Pair Proof Kernel v1`，不把其机械完整性误称为营销判断力。
2. 暂不进入原计划 06B-2 的公开报告扩展；先建立可校准的案例、评委和多案例裁决层。
3. 新实验室产生合格候选后，才使用 06A/06B-1 的强封账能力进行最终晋级证明。
4. `G2`、`PASS_TO_PHASE_0B` 和完整产品业务能力继续保持 OPEN，直到真实多案例评测满足预注册门槛。

## 2. 为什么不能继续沿用现有三人盲评

现有 06A 已经能证明匿名 A/B、材料哈希、独立映射、三份评审、严重失败和费用绑定等机械条件，但仍有四个业务缺口：

1. reviewer 资格主要依赖自我声明，没有用代表案例验证其营销判断力。
2. 固定六维总分和恰好三名 reviewer 不能覆盖不同领域、任务和评委能力组合。
3. 单案例、单次 `Skill vs no Skill` 只能产生一个弱信号，不能说明跨案例营销能力。
4. 历史结果、专家判断和创意偏好尚未分层，容易把爆款数据、名人经验或模型投票误当真理。

因此 06A 保持冻结的高强度证明内核；Marketing Capability Evaluation Lab 在其外部补齐业务测量学，而不是继续向一个已经超过四万行的单案例证明器堆功能。

## 3. 真值模型：没有唯一创意答案，但不是没有标准

每道考题把答案拆成四层，分别裁决：

| 层级 | 权威来源 | 适合裁决 | 明确不能证明 |
| --- | --- | --- | --- |
| A. 硬约束 | 冻结 Brief、来源、权利与确定性规则 | 事实、主体、目标行动、证据、权利、是否遗漏交付、是否严重不可用 | 观众一定喜欢 |
| B. 开放式质量 | 通过校准的目标人群或专业评委的匿名偏好分布 | 哪份判断或内容更适合当前任务，是否接近、是否信息不足 | 普适永恒的创意真理 |
| C. 历史效标 | 交卷后才揭封的真实发布结果 | 预测是否与未来结果保持校准，机制能否在相似条件复现 | 单条内容的因果成功公式 |
| D. 前瞻实验 | 预注册的真实发布、投放或集群 A/B | 某个改动在特定人群、时期和条件下的增量 | 跨账号、跨时代万能规则 |

历史爆款和成功账号是出题素材、效标和假设来源，不是标准答案库。专家多数票、模型分数、播放量、互动率、Bradley–Terry 排名和评委一致性也都只是测量结果，不能独自晋升为真值。

## 4. 总体架构

```text
授权材料 / 冻结抖音观察 / MediaKit 时间证据 / 历史结果
                         │
                         ▼
Case Factory ──> Content Packet + Outcome Packet + Reference Dossier
                         │             │                 │
                         │             │交卷前隔离       │只供校准/复核
                         ▼             │                 │
Candidate Runner ──> stock Codex / modified Codex       │
                         │                               │
                         ▼                               │
Blind Controller ──> 匿名、换位、封存、独立 workspace   │
                         │                               │
                         ▼                               │
Reviewer Workbench <── Reviewer Academy                 │
                         │                               │
                         ▼                               │
Decision Engine ──> 分层胜率、严重失败、置信区间、漂移  │
                         │                               │
                         └──────── 锁定后揭封 ──────────┘
                                         │
                                         ▼
                           06A/06B-1 Final Promotion Proof
```

### 4.1 Case Factory

Case Factory 把完整历史材料编译为三个物理隔离的包：

- `ContentPacket`：候选系统和第一阶段真人评委可见，只含当时可获得的业务事实、材料、约束、任务与证据。
- `OutcomePacket`：只含预测时间之后的真实结果及可用性说明；候选交卷并封存前不可访问。
- `ReferenceDossier`：案例编写依据、可成立方向、反例、不可复制条件和争议说明；不作为候选提示词，也不要求候选复述唯一路线。

案例按时间冻结，所有进入 `ContentPacket` 的事实都必须满足 `availableAt <= predictionTime`。同一账号不能只选爆款；应从连续时间窗或明确分层中同时纳入高、中、低表现作品，并保留普通账号、失败案例、时期迁移和未见创作者外推集。

### 4.2 Evidence Compiler

证据编译器采用第五版 `AccountEvidencePack` 与第四版仓颉变种中已经证明有价值的语义：

1. `observation`：何时发生了什么；
2. `function`：该元素可能承担什么内容功能；
3. `interpretation`：为什么可能对该受众有意义；
4. `adaptationVariable`：保留功能时哪些变量可以改变；
5. `nonCopyBoundary`：哪些身份、表达、素材或品牌资产不可复制。

所有字段区分 `observed / inferred / unknown`。模型观察必须回指视频、时间码或来源字段；确定性代码只负责身份、哈希、计数、覆盖、版本和引用完整性，不负责判定爆款原因。

### 4.3 Candidate Runner

候选运行分为两级：

- **研发批跑：** 使用锁定版本的 promptfoo Codex App Server provider，以不同 `codex_path_override` 启动冻结原版和爆改版 Codex。它只承担快速实验、重复运行和轨迹采集，不拥有最终解盲和晋级权。
- **最终证明：** 通过研发批跑和真人评审的候选，进入现有 06A/06B-1 原生 App Server 强封账路径。

两臂必须使用相同模型、provider、基础配置、权限、预算、案例、材料、输出合同和 workspace 初始状态。每臂使用独立的 Home、Thread、cache 和临时 workspace。候选不得访问 scorer、OutcomePacket、ReferenceDossier、另一臂输出或真实臂身份。

promptfoo 的配置文件和脚本按可执行代码处理，只允许来自锁定仓库版本；不接受案例或模型输出生成的动态 JavaScript。

### 4.4 Blind Controller

Blind Controller 是实验室自建的权威薄层，负责：

- 为真实 arm 建立不可猜测的 opaque ID；
- 每题用记录的 CSPRNG seed 随机排列左右位置；
- 对同一评委安排 A/B 与 B/A 换位复测；
- 在评委包中删除 binary、provider、模型家族、Skill、路径、成本和日志标识；
- 分离 reviewer packet、arm key、outcome key 和分析解盲权限；
- 对题集、二进制、模型、权限、预算、随机 seed、rubric、评委版本和输出生成不可变 manifest；
- 只有评审和统计对象封存后才能解盲。

换位后赢家不一致时，开放偏好记为 `nearTie` 或转人工仲裁，不能选择对系统有利的一次结果。`eligibility=neither` 与普通平局必须分开。

### 4.5 Reviewer Workbench

首版使用本地自托管 Label Studio Community Edition 作为真人评审工作台。它只显示 Blind Controller 导出的匿名任务，不保存 arm key、OutcomePacket、供应商密钥或候选 workspace。

每份 submission 至少包含：

- `eligibility = both | aOnly | bOnly | neither`；
- `preference = A | B | nearTie | abstain`；
- 各评测维度的独立判断，不强制过早压成一个总分；
- severe flags；
- 最多三条短理由及对应 evidence refs；
- 评委对信息是否足够的声明。

Label Studio 只负责展示、分配和收集；资格、随机化、换位、解盲与晋级仍由实验室控制。

### 4.6 Reviewer Academy

评委资格按能力域发放，不再使用泛化的“有经验运营”自我声明。一个 reviewer profile 可以拥有多个带有效期的 qualification：

- 业务与品牌 IP 判断；
- 抖音内容操盘；
- 编导与制作可行性；
- 转化与行动漏斗；
- 特定行业知识；
- 事实、权利和证据完整性。

取得资格前必须完成共享校准题；生产批次持续混入隐藏锚点、重复题和位置交换题。资格至少检查：

- 明确硬约束题准确性；
- seeded severe failure 是否漏判；
- 隐藏重复题一致性；
- 位置交换一致性；
- 与合格专家群的系统偏离；
- 在不同题材、人群和创作者类型上的差异偏差。

阈值从首批 pilot 分布中预注册，不伪装成跨行业通用常数。稳定但偏严或偏宽的 reviewer 可以进入后续严厉度校正；随机、漂移、位置偏好、查阅结果、识别作者、串通、共享凭证或未披露利益冲突的 reviewer 必须暂停或取消资格。

每个生产案例默认由两名合格真人独立评审；出现实质分歧、低边际或高风险时由第三名合格 reviewer 仲裁。整个 reviewer pool 和不同案例的 panel 可以大于三人，单题也允许按预注册规则增加评委，不把人数写成领域常数。

### 4.7 AI Judge

世界营销专家、抖音 IP 操盘手和成功博主可以被整理成有来源的方法卡、校准题和证据视角，不能仅凭姓名模拟一个 persona 后获得裁判权。

任何 AI judge 的资格绑定精确模型快照、prompt、rubric、温度、工具和上下文。AI judge 在独立 holdout 上与合格真人完成位置偏差、冗长偏差、自家族偏好和严重失败检出校准前，只输出 shadow diagnostics，不参与晋级票数。同一模型换版本或更改配置后视为新评委，必须重新认证。

### 4.8 Decision Engine

硬约束与开放偏好分开计算：

- severe failure 是非补偿门；创意高分不能抵消事实、主体、权利或完全不可用错误。
- 保留业务理解、策略取舍、证据完整性、内容吸引力、人格/IP 连续性、行动转化适配和制作可行性等维度分布。
- 两系统比较先报告分层 win/tie/abstain、换位一致性和 bootstrap/后验区间；候选超过两套时再使用 Bradley–Terry，并做排序稳定性检查。
- reviewer 质量同时报告原始一致率、重复一致率、换位一致率和按维度的一致性指标。一致性不等于正确性。
- promotion policy 必须在看到隐藏测试结果前冻结，并报告领域、任务、案例时期和评委覆盖；不得用一个总分掩盖某一层严重退化。

当前 06A 的单案例阈值继续属于 `Blind Pair Proof Kernel v1`，不自动成为新实验室的通用门槛。首批 pilot 用来估计分布和设置下一批预注册阈值；pilot 本身不能授权产品业务 PASS。

## 5. 核心数据合同

实验室采用版本化 JSON、内容寻址对象和追加式 ledger，首版不建设向量数据库。最小对象为：

| 对象 | 责任 |
| --- | --- |
| `CaseFamily` | 一个业务领域及其任务、时期、账户类型和证据覆盖定义 |
| `ExamCase` | 一次冻结考试的身份、时间切点、任务、材料和包引用 |
| `SourceEnvelope` | 来源、权利、采集方式、可用时间、哈希和限制 |
| `EvidencePack` | observed/inferred/unknown 信号、时间码、反例和覆盖 |
| `ContentPacket` | 候选与第一阶段评委可见输入 |
| `OutcomePacket` | 锁定后的历史结果和可用性 |
| `ReferenceDossier` | 出题依据、可成立方向、反例与不可复制条件 |
| `TreatmentSpec` | 原版/爆改版的不可见处理定义和唯一允许差异 |
| `CandidateRun` | 输入、二进制、配置、预算、输出与轨迹 commitment |
| `BlindAssignment` | reviewer、匿名 arm、位置 seed 与换位关联 |
| `ReviewSubmission` | 合格性、偏好、维度、severe flags 和 evidence refs |
| `ReviewerProfile` | 身份 commitment、能力域、利益冲突和资格状态 |
| `CalibrationAttempt` | 锚点、重复、换位、错误检出与资格裁决 |
| `EvaluationBatch` | 题集、处理、评委、分析计划与冻结时间 |
| `BatchDecision` | 解盲前统计、解盲结果、限制和 promotion disposition |

私有正文、真实 reviewer 身份、arm key、outcome key、视频、评论原文和候选输出留在 Git 外；仓库只提交 Schema、rubric、无正文 fixture、哈希、聚合结论和无正文 verification。

## 6. 开源组件选择

### 6.1 采用

- **promptfoo：** 研发批量运行和 Codex App Server adapter；锁定版本与 lockfile，放在外部实验层。
- **Label Studio OSS：** 真人 pairwise 与多媒体证据工作台；本地自托管。
- **现有 06A/06B-1：** 最终候选的原生 App Server 强封账、私有证据与费用绑定。

### 6.2 仅借鉴或后置

- Phoenix 的 shuffle + swap-confirm 协议只借鉴，不因其评测功能再部署一套主 runner。
- Inspect AI 仅在未来出现 Docker/Kubernetes 沙箱、长程多 Agent 或特殊多模态专项时使用，不与 promptfoo 同时争夺当前主 runner。
- DVC 只在案例媒体规模需要对象存储版本时增加；它不负责隐藏集权限。
- MLflow 与 Langfuse 只允许在长期 trace/运营需要明确后二选一，首版不引入。

### 6.3 不作为核心

OpenAI Evals、DeepEval、Giskard、Argilla、Phoenix、MLflow、Langfuse 和 DVC 均不同时进入首版核心。避免多套 runner、schema、缓存、重试、trace 和人评状态互相冲突。

## 7. 旧版资产迁移决策

| 资产 | 决策 | 原因 |
| --- | --- | --- |
| V4 M2 随机映射、双评审、仲裁、封存 | 迁移协议与 fixture，按新合同重写 | 已有测试和真实人工运行；不依赖模型裁判 |
| V5 `AccountEvidencePack` | 复用合同语义与人工复核队列 | 已有真实账号产物和证据/权利边界 |
| V4 `video-pattern-learning` 五层证据 | 薄迁语义，不恢复旧工具/API | 方法有价值，旧编译器已退役 |
| V4 `video-method-distillation` | 仅作为方法资料提取器参考 | 不让视频自动变成线上 Skill 或裁判 |
| V6 同条件实验回执 | 迁移模型、工具、预算、顺序与失败关闭原则 | 已覆盖候选运行的基本可比性 |
| V6 MediaKit 本地安全路由 | 复用本地 trusted I/O 与 receipt | 本地 metadata/trim 有真实验收 |
| V6 社区抖音 MCP | 本地研究可恢复；不打进商业运行时 | 真实跑通过，但源码未留存且 LICENSE/README 冲突 |
| A116 词法 denylist/default root 硬门 | 弃用 | 业务结论被字符串门槛绑死 |
| 单个 LLM judge 决定胜负 | 弃用 | 已出现漏掉人工发现编造的历史反例 |
| `TK爬虫` | 弃用 | 空仓库，无源码、历史、许可或数据能力 |

不迁移任何旧 `.env`、API key、Cookie、StorageState、浏览器配置、代理、验证码材料、评论原始身份或付费恢复凭证。

## 8. 首个纵切：黄金礼品／礼文化

### 8.1 冻结起点

第一批不连接实时平台，先使用第六版已经封存的：

- A113 社区抖音真实搜索证据；
- A115 黄金礼品真实 E2E 失败案例；
- A116 垂直 Skill 诊断案例，仅作为污染与过拟合负例；
- 第五版/第六版已有业务材料和运行回执。

这使实验室骨架能在不读取旧凭证、不触发平台风控和不产生付费调用的情况下完成第一轮 RED/GREEN。骨架稳定后，可恢复 A113 固定提交的社区采集器用于内部研究和补题，但必须放在有界只读 adapter 后，不进入商业发行包。

### 8.2 Case Family

`golden-gift-li-culture-v1` 的业务链为：

```text
黄金礼品
→ 礼品与送/收行为
→ 人情往来和关系变化
→ 中国礼节、人情世故与现代边界
→ 品牌可持续表达与可观察业务行动
```

它不是隐藏的唯一正确内容根。Reference Dossier 至少保留三个可成立方向及其条件：

1. 礼仪知识与传统/现代冲突；
2. 人情往来、关系判断与生活情境；
3. 礼品选择、赠礼风险与品牌商业承接。

后续实时补题采用精准发现再分层抽样，不随机抓若干账号求平均：先按业务主体、受众、内容功能和商业模式发现候选，再覆盖领域权威、人情世故解释者、礼品/消费转化者、挑战者和反例。郭大侠可以成为重要代表样本，但不能成为唯一老师；同账号应同时抽取高、中、低表现作品并记录时期。

### 8.3 能力阶梯

第一套纵切按同一 Case Family 逐级评估：

1. **L1 业务理解：** 主体、受众、目标行动、已有证据和缺口；
2. **L2 孵化判断：** 零个、一个或多个可成立方向、取舍、条件与反例；
3. **L3 内容系统：** 可持续系列、人物/关系、选题空间和商业连接；
4. **L4 ContentPackage：** 一条可拍摄成稿及必要制作说明；
5. **L5 PreflightPrediction：** 预期影响对象、机制、指标、失败信号与替代解释。

首个 executable child 先交付 L1–L2 的完整盲评闭环，同时把 L3–L5 Schema 接缝冻结；不得用空实现宣称整条业务链已经完成。

## 9. 实验与晋级流程

1. 选择 case family、任务层级和时间切点。
2. Case Factory 编译并分别封存 content/outcome/reference 三包。
3. Reviewer Academy 在独立 calibration split 上确定本批合格 reviewer。
4. 冻结 candidate binary、treatment、模型、权限、预算、rubric 和分析计划。
5. Candidate Runner 按承诺顺序运行两臂并生成候选输出。
6. Blind Controller 生成匿名 reviewer tasks 和换位复测任务。
7. 合格真人独立评审；分歧、低边际和高风险任务进入仲裁。
8. Decision Engine 在不解盲状态下封存统计与 reviewer quality。
9. 解封 arm key，生成带限制的 batch decision。
10. 历史回测再解封 OutcomePacket；结果只校准预测，不追溯重写偏好票。
11. 达到预注册 promotion policy 的候选进入 06A/06B-1 最终封账。
12. 通过历史评测仍不等于爆款因果证明；真实发布与数据回收由后续 Publication Learning Loop 完成。

## 10. 失败关闭与业务自由度

下列情况使单题或批次 INVALID，而不是给候选补分：

- 候选在封存前读到 OutcomePacket、ReferenceDossier、scorer 或另一臂输出；
- arm 身份、模型家族、Skill 名、路径或成本泄漏给 reviewer；
- 内容包、二进制、模型配置、权限、预算或 rubric 在运行中漂移；
- reviewer 无对应资格、换位关系缺失、提交身份重复或解盲提前；
- 来源权利、时间切点或关键 evidence ref 无法复核；
- 动态执行未审查的 promptfoo JavaScript 或案例内容成为评测器指令。

这些边界只保护实验有效性，不成为产品业务限制。正式 AI IP Lead 仍可联网研究、读写项目、调用 Skills/MCP、运行工具和在用户预算内试错；不得把评测环境的离线、只读或双臂隔离复制成客户默认能力阉割。

## 11. 隐私、许可与数据边界

- 首个纵切只用现有冻结证据，不定位或读取第五/第六版 API key。
- 社区抖音实现即使继续用于本地研发，也不因 MIT 文件自动获得平台数据、视频内容和商业发行权。
- MediaKit 首版只允许已经验收的本地能力；云能力保持关闭，逐能力另行授权和验收。
- 视频、评论、账号身份和 reviewer 身份按最小披露处理；公开报告不得包含原文或可逆个人标识。
- `analysis_only` 素材不能自动进入训练集、Skill 或商业案例库。
- 候选输出、真人理由和 arm mapping 在 retention 到期后按实验清单关闭；用户原始项目材料不冒充已经删除。

## 12. 实施分解与完成标准

本设计拆为四个可独立验收的 executable child：

1. **Lab Foundation + Golden Gift L1–L2**：核心 Schema、三包隔离、Reviewer Academy、Blind Controller、决策合同和冻结 fixture 的离线闭环。
2. **Codex Batch Runner**：promptfoo 锁定、原版/爆改版 App Server adapter、独立 workspace、相同条件和轨迹 receipt。
3. **Human Review Workbench**：Label Studio 本地部署、匿名任务导入导出、双评审/仲裁和 reviewer 资格同步。
4. **Calibrated Pilot + Promotion Bridge**：黄金礼品多案例 pilot、换位/漂移统计、预注册 promotion policy 和 06A/06B-1 final bridge。

实验室首个可用里程碑必须真实做到：

- 从冻结黄金礼品材料生成物理隔离的三包；
- 对两份无 treatment 标识的候选输出生成独立随机盲包；
- 运行 reviewer 校准并拒绝无资格 submission；
- 收集两人独立评审并在分歧时进入仲裁；
- 在解盲前输出无正文统计，在锁定后才解盲；
- 用 A115 已知负例证明严重错误不能被创意分抵消；
- 全流程不调用 provider、不读取凭证、不访问实时抖音、不声称业务 PASS。

只有第四个 child 完成，才允许讨论新的 G2 真实业务门；在此之前，任何离线 fixture、模型自评或漂亮 UI 都不能被称为“营销能力已经通过”。

## 13. 研究与本地证据索引

开源与方法研究以官方文档、论文和仓库为准：

- promptfoo Codex App Server provider：<https://www.promptfoo.dev/docs/providers/openai-codex-app-server/>
- Label Studio generative pairwise human preference：<https://labelstud.io/templates/generative-pairwise-human-preference>
- Chatbot Arena 匿名 pairwise 与统计排名：<https://arxiv.org/abs/2403.04132>
- LLM-as-a-Judge 位置、冗长和自增强偏差：<https://arxiv.org/abs/2306.05685>
- PoLL 多样模型评委面板：<https://arxiv.org/abs/2404.18796>
- 时间序列 rolling-origin 评测：<https://otexts.com/fpp3/tscv.html>

本地旧版证据以只读方式审计：

- V4 M2 盲评引擎：`/Users/yangyucheng/Documents/第四版营销系统/scripts/ip_agent_m2_method_attribution.py`
- V4 视频模式 Skill：`/Users/yangyucheng/Documents/第四版营销系统/skills/public/video-pattern-learning/SKILL.md`
- V5 AccountEvidencePack：`/Users/yangyucheng/Documents/ChatGPT/第五版营销系统/backend/experiments/e15_account_evidence/`
- V5 A33：`/Users/yangyucheng/Documents/ChatGPT/第五版营销系统/docs/mcn-incubation-v5/audits/A33-account-structured-extraction-feasibility.md`
- V6 A97：`/Users/yangyucheng/Documents/ChatGPT/第六版营销系统/docs/content-intelligence-v6/audits/A97-skill-evidence-self-evolving-system.md`
- V6 A113/A114：`/Users/yangyucheng/Documents/ChatGPT/第六版营销系统/docs/content-intelligence-v6/audits/`
- V6 黄金礼品 A115/A116 冻结 evidence：`/Users/yangyucheng/Documents/ChatGPT/第六版营销系统/docs/content-intelligence-v6/evidence/`
