# AI IP 一站式影响力创作系统 1.1 设计规格

- **日期：** 2026-08-24
- **最新修订：** 2026-09-05（业务优先清理；产品架构不变）
- **状态：** 产品范围 v1.1；架构规格 v1.4 与国产模型适配 v1.5 已于 2026-08-25 获用户确认，2026-09-05 已退役旧评测开发前置条件；当前执行以总台账的业务优先修订为准。v1.2 固化前六轮系统审计护栏；v1.3 的 DeerFlow 方案已被 v1.4 明确取代；v1.4 采用 Codex 开源仓深度分叉、本地优先 Business Server 和云端商业能力薄控制面
- **目标市场：** 中国大陆用户为主；首版重点服务抖音、小红书内容，保留服务海外 TikTok 业务题材的能力，但不直接接入 TikTok 发布接口
- **模型与媒体供应商：** 火山引擎 / 火山方舟作为首条真实 provider；用户现有方舟凭证可访问 GLM，故 GLM 作为优先文本 Agent model route，Seedream/Seedance/TTS 继续承担首发媒体路线；跨 provider 候选后续按能力矩阵选择
- **正式客户端：** Windows 11；macOS 同期仅作内部使用和邀请测试

本文件是当前产品与总体架构规格；国产模型 provider 专项边界由 `docs/superpowers/specs/2026-08-25-domestic-model-adaptation-design.md` 细化。产品不再以经典 C 端 SaaS 为前提：C 端是运行于用户电脑的本地优先产品；代理管理和平台内部运营是两套独立云端 Web 系统。v1.3 DeerFlow/云 SaaS 旧计划已归档为历史输入；现役入口是 `docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md`。旧 Phase 0A 与评测 child 已退役，不再构成内部业务开发许可门。禁止继续按 DeerFlow、云端保存客户项目、C 端浏览器直连公网 Agent、固定多 Agent 流水线或外置 Codex 套壳等旧假设实施。

> **2026-09-05 业务优先退役修订：** 已批准的 [business-first cleanup](2026-09-05-business-first-cleanup-design.md) 覆盖本文及被引用旧计划中的内部开发前置要求。Phase 0A、06A、06B、07A、07B（含 LH1）、`PASS_TO_PHASE_0B` 和依赖旧评测 broker 的 proof 完成链为 `RETIRED_NOT_PASSED`，不能阻止继续做真实营销业务切片，也不得恢复为替代评测系统。旧通过、失败和停止记录均只作历史；清理不等于业务 PASS、模型资格或客户版本就绪。真实支出、发布、客户数据和客户上线职责不因此免除。产品架构、来源许可和国产模型方向继续保留。

## 1. 产品定义

本产品是一套面向商家、机构和创作者的本地优先 AI IP 影响力创作系统。它不是单一聊天机器人，也不是只会根据商品关键词生成文案的工具。系统以用户希望获得的影响力或商业结果为 Mission，完成从业务理解、IP 策略、研究、选题、营销叙事、编导制作、AI 成片、人工发布、数据回收，到策略学习的完整闭环。

产品北极星是：一站式帮助用户持续产出更可能成为爆款的内容，建立影响力，并带来可衡量的关注、信任或业务行动。系统不能承诺任何单条内容必然成为爆款；它必须通过更好的业务理解、创意探索、成片质量、发布前预测和真实发布复盘，持续提高结果概率。

系统的核心价值是：

1. 从“业务主体是谁、希望影响谁、希望对方发生什么行动”出发，判断任务属于关注、信任、购买、咨询、报名、加入、激活或其他影响关系，并形成可验证的 ActionFunnel；购买关系只是其中一种，不把所有项目都套成卖货。
2. 像有经验的新媒体运营和编导一样，找到可持续的人物、事件、关系、欲望与冲突。
3. 把内容策划转化为可以真人拍摄、数字人呈现或 AI 情景剧生成的成品。
4. 发布前记录预测，发布后用真实数据和评论修正当前 IP 的经验。
5. 通过一级代理、二级代理和 C 端用户的预充值额度体系完成商业化，同时让代理管理完全独立于客户创作产品。

产品采用“一个 Lead 对结果负责＋按需调用业务能力”的方式，不采用一个大提示词，不采用固定多 Agent 队列，也不让用户管理 Agent。默认由 Lead 把 Mission 做到可交付；专业用户可进入任意业务产物进行修改、重跑或局部委托。

## 2. 首版目标与非目标

### 2.1 首版目标

- Windows 11 用户安装本地产品后，可创建多个相互隔离的 IP 项目；项目、脚本、记忆和素材默认只保存在本机。
- 用户可直接描述一个影响力或商业目标并上传人物、业务、产品、案例和历史内容资料；Lead 只追问当前最影响下一步的缺口，不强迫完成固定时长或固定顺序的问卷。
- 系统维护可版本化的 InfluenceRelation 与 ActionFunnel；在购买型任务中再区分付款者、购买者、使用者、受益者、影响者、推荐者和渠道，在招募型任务中表达看见、信任、报名、审核、加入和激活等行动。
- 根据证据和任务需要形成零个、一个或多个策略候选；方向已经明确时直接执行，不为凑数量制造候选。重大定位由用户确认，普通创作选择由 Lead 推荐并继续执行。
- 主动检索公开网络信息，并分析用户提交的对标链接或材料。
- 从一个 Mission 连续生成完成目标所需的中间产物和可直接拍摄或发布的 ContentPackage；研究、策略、选题、周计划或媒体只在当前任务需要时生成，不要求每次凑齐全套。
- 生成抖音与小红书差异化内容版本。
- 正式支持 Seedream 图片、Seedance AI 情景镜头、豆包标准 TTS 和自动后期。
- 支持真人拍摄生产包、AI 情景剧及混合制作。
- 克隆数字人口播以 Beta 形式存在，受账号权限、授权和稳定性闸门控制。
- 用户手动发布并提交链接、表单或截图；系统提供 T+1、T+3、T+7 默认窗口，也允许 Mission 自定义窗口和事件触发回收，招募/转化任务至少支持 T+14、T+30 及报名、加入、激活等事件。
- 使用发布前预测、数据对照、评论洞察和项目记忆形成学习闭环。
- 支持平台、一级代理、二级代理、C 端用户和创作点账本。
- 云端长期只保存登录、设备、额度、价格、供应商调用凭证和用户主动开启后的端到端加密备份；供应商处理所需的瞬时明文与有期限操作性暂存按第 15.3 节处理，不形成可浏览的客户项目库。

### 2.2 首版明确不做

- 不直接登录或自动发布到抖音、小红书或 TikTok。
- 不提供经典 C 端 Web SaaS；客户浏览器只访问本机 Codex AI IP Business Server。
- 不默认把客户项目、脚本、记忆或素材上传云端，也不提供首版实时多设备协作。
- 不开放用户自带 API Key；首版模型、搜索、图片、视频和语音能力统一通过平台能力网关调用并扣创作点。
- 不承诺纯离线模型推理；断网时允许查看、编辑、整理和导出已有项目，需要云模型或供应商的步骤进入等待联网。
- 不建设大规模平台爬虫。
- 不接 C 端微信、支付宝在线支付和自动分账。
- 不支持三级及更深代理层级。
- 不支持团队协作、项目负责人、编辑和只读成员等团队版角色。
- 不根据少量数据自动微调基础模型。
- 不自动把一个 IP 的私有经验用于另一个 IP。
- 不把传统克隆数字人口播承诺为正式 SLA 能力。
- 不在首版提供全平台内容适配；抖音和小红书为正式适配平台。

## 3. 首批验证项目

固定使用以下五类项目压测系统：

1. **水果**：商品大类、产地、产业链、季节和现实事件。
2. **黄金礼品**：商品材质与送礼行为、人情关系、礼俗和商业承接。
3. **TikTok 直播公会品牌 IP**：主体是公会组织/品牌，不默认把老板或某个主播塑造成唯一 IP；目标是让合适达人看见、产生信任、报名、审核通过、加入并首次开播。系统必须理解机构、达人、运营、平台和家人等多方关系，产出品牌承诺、真实工作场景、成长证据和招募转化内容，不能退化成行业 SOP 或老板个人涨粉方案。
4. **本系统自己营销自己**：平台作为一个真实 IP 项目，分别面向一级代理、二级代理和 C 端用户建立 InfluenceRelation、ActionFunnel 并生产内容；必须使用与客户相同的 Mission、Lead、能力和评测规则，但不要求走固定流程，也不能调用特制宣传模板。
5. **宝妈素人**：人物身份信息不足，系统必须给暂定探索方向并继续访谈，不能自动生成育儿账号方案。

五类项目共同验证：系统能否先识别“业务主体是谁、希望影响谁以及为什么会产生行动”，再决定研究、故事和内容路径，而不是套固定行业公式。第三类专门验证品牌/组织型 IP、多人物出镜和招募漏斗；核心结果包括有效达人报名、加入和首次开播，不只看播放。第四类同时执行内部实战检验：系统必须能够用自己的能力获得关注、信任和业务行动，并用真实发布数据暴露自身问题。固定情景测试另设“只给出身份标签或销售方式”的反例输入，确保系统会继续澄清而不会把经营方式直接当成内容主题；该反例不作为第六个真实项目。

## 4. 用户与商业关系

### 4.1 固定账户层级

```text
平台
└── 一级代理
    ├── 二级代理
    │   └── C 端用户
    └── C 端用户
```

- 一级代理可以创建二级代理和直属 C 端用户。
- 二级代理只能创建直属 C 端用户。
- 首版平台只向一级代理线下收款并充值创作点。
- 一级、二级代理分别向自己的下级线下收款，并在独立代理管理系统内确认凭证和划拨创作点；本地客户产品不提供这些入口。
- 代理属于商业渠道角色，不属于内容协作角色。
- 每个 C 端用户可创建多个 IP 项目，但不能邀请其他成员共同编辑。

### 4.2 内容隐私

代理默认只能读取云端白名单投影：下级账户状态、当前余额、额度流水、聚合消费、设备状态和售后编号。代理管理 API、事件和客户端不得包含 Mission 名称、Prompt、错误原文、Thread、Turn、Memory、Skill、MCP、项目内容或素材字段。

云端本身不持有可读的客户项目，平台和代理因此不存在“后台直接打开客户内容”的普通支持能力。需要协助时，由客户在本地产品中主动生成脱敏、可预览、可撤销、带有效期的 SupportBundle；客户明确选择其中的诊断日志、配置或产物片段后才可上传。原始肖像、声音、儿童素材、恢复密钥和未选择的项目内容默认排除。所有上传、查看和下载均进入追加式审计日志。

### 4.3 平台自营销项目的责任主体

- 平台管理员创建平台内部运营账号、指定自营销项目负责人并设置内部预算，但不代替负责人作内容决策。
- 平台内容运营负责人定义 Mission、提供材料、授权预算、在必要时确认重大方向、人工发布并回收数据；普通选题、脚本、镜头和修改由 Lead 自主推进。
- 真正发布前，如内容使用授权真人、客户案例、敏感原件或重大可验证商业主张，由独立平台审核人确认对应权利和事实；真实但有争议的观点、普通事实不确定和媒体质量问题只提示并提供替代稿，不在创作阶段形成宽泛审批门。
- 这些是平台内部权限，不属于面向 C 端销售的团队版，也不进入一级、二级代理关系树。
- 项目负责人、审核人、预算批准人及其每次操作都写入审计日志；任何人不能通过内部身份绕过预算上限、真人权利、敏感外发、永久删除或实际发布等不可逆边界。
- 平台自营销使用与客户相同的本地 Business Server，在平台持有的专用设备和独立本地工作区运行；`internal-console` 只配置内部账号、预算和能力权限，不保存自营销项目正文。平台项目不能进入普通客户设备，客户项目也不能进入平台运营设备。

## 5. 产品操作流程

```text
用户描述影响力或商业目标并提供现有材料
→ 创建 Mission
→ Lead 判断目标、已有证据、预算和可交付物
→ 动态研究、追问、并行探索、跳步、回退或局部重做
→ 生成 Proposal 与版本化 Artifact
→ 仅在关键事实、重大方向、预算越界或权利边界上请求用户
→ 交付可直接拍摄或发布的 ContentPackage
→ 用户人工发布并回收真实数据
→ Lead 复盘、提出下一轮 Mission 或更新候选经验
```

这是默认的全程委托模式，不是必须逐项完成的固定状态机。用户可以从一句“帮我做三条招募达人并促成报名的视频”开始，也可以从已有脚本、镜头或成片开始；Lead 应复用已有产物，只补完成目标所需的步骤。用户可随时进入创作工作台查看和修改策略、选题、脚本、分镜、媒体、发布与复盘等结构化产物。

Lead 在 Mission 已授权预算内可以连续试错，不逐次弹出付费确认；只有预计超出总预算或单次高成本上限时才再次询问。发布前预测默认由系统自动记录，不成为用户完成任务的额外表单。

每个 IP 项目保存默认营销风险偏好：克制、平衡或激进；默认使用“平衡”，每个 Mission 可以覆盖。该偏好只影响开头强度、悬念和修辞，不降低事实、授权和商品真实性标准。

## 6. 系统总体架构

### 6.1 设计原则

- 产品采用 OpenAI Codex 开源仓的整仓深度分叉，直接修改 `codex-rs/core`、`app-server`、`app-server-protocol` 和 `state`，把 App Server 变成 AI IP Business Server；不把未改造的 Codex 作为外部 sidecar，也不在外面另造一套 C 端业务后端。
- 业务结果优先于 SaaS 完整度、上游低差异和工程形式。Codex 的安全执行、Thread/Turn/Item、Skills、MCP、Sandbox、工具与临时子 Agent 是基础能力；Project、Mission、Proposal、Artifact、ContentPackage、Prediction 和 Memory 是产品主语。
- C 端客户项目以本地加密 SQLite 和本地项目文件为唯一真相；云端不能远程修改项目、正式版本、记忆或发布状态。
- 云端只承担身份、设备授权、创作点账本、能力资格、供应商路由、调用回执、临时媒体保全和用户主动开启的端到端加密备份。
- 系统默认允许 Lead 联网研究、读写项目文件、运行脚本、使用 Shell、Skills/MCP、FFmpeg 和已授权媒体能力；普通风险以标注、建议和可撤销版本处理，不用大面积硬阻断阉割业务能力。
- 模型、搜索、图片、视频和语音统一通过平台能力网关调用，不开放 BYOK。业务代码引用能力与质量档位，不写死具体模型 ID。
- 模型适配使用显式 `ProviderCapabilityProfile` 和五级证据状态，不把供应商名称、compatibility name 或通用 provider 的默认值当作能力真相。统一合同保留各模型的差异化能力，不把业务压缩成所有供应商的最低能力交集。
- 用户可一次授权 Mission 总预算和单次高成本上限；授权范围内 Lead 自主执行，不逐调用打断。
- 本地客户产品、代理管理系统和平台内部后台是三个独立发布物。代理和平台后台只共享云端账号、账户、账本及调用回执，不拥有客户本地项目的读取通道。

### 6.2 逻辑结构

```text
用户的 Windows 11 电脑
系统浏览器
    ↓ 同源 localhost HTTP / WebSocket
Codex AI IP Business Server（一个本地产品进程）
    ├─ AI IP Domain / Mission Application
    ├─ Codex Lead / Thread / Turn / Skills / MCP / Sandbox
    ├─ 加密 SQLite / 本地项目文件 / 媒体与版本谱系
    ├─ 内置 React/TypeScript 业务界面
    └─ Capability Client / Backup Client
                  ↓ 仅在需要联网能力时
           云端薄控制面与能力网关
    Account / Device / Entitlement / Credit Ledger / Pricing
    CapabilityOperation / ProviderReceipt / Encrypted Backup
                  ↓
 Domestic Model Adapters / Search / Seedream / Seedance / TTS

partner-management                 internal-console
独立云端 Web 系统                    独立云端 Web 系统
          └──────────→ 同一云端商业与能力控制面 ←──────────┘
```

#### 6.2.1 Codex harness 的工作方式与产品映射

Codex harness 是一个持续的回合式执行内核，不是预先画死的多 Agent DAG。一个根 Thread 承载长期上下文，一个 Turn 接收当前目标和状态；Lead 在每一轮判断下一步，可以直接生成结果，也可以调用 Shell、文件、浏览器、Skill、MCP 或创建有边界的临时子 Thread。工具结果和子 Thread 回执成为新的 Item 回到根 Thread，Lead 再判断、纠错、并行、回退或结束。App Server 负责 Thread/Turn/Item 生命周期、流式事件、恢复、配置、批准与外部控制面；它暴露 harness，但不替 Lead 预设固定业务流程。

AI IP 1.1 保留这套根 Thread/Turn/tool loop，并把“对代码任务负责”替换成“对 Mission 的影响力结果负责”：

- 根 Thread 对一个 Mission 的最终 ContentPackage 和下一步行动负责；不会把定位、脚本、镜头分别永久分配给一组互相投票的 Agent。
- Skill 是按需加载的方法与约束，MCP/本地工具/云端 Capability 是执行动作，临时子 Thread 只承担可隔离的研究、媒资或独立审查；它们都不拥有第二套 Mission、Artifact 或 Memory 真相。
- 上下文压缩、暂停、恢复或子任务失败不能改变业务真相；正式状态只由本地 Mission/Artifact 图和追加式 receipt 承载，Thread/Turn 是运行证据。
- 用户只看到目标、产物、版本、成本和下一步，不必管理 harness 里的 Thread、Item、Skill 或子 Agent。深 fork 直接修改 core/App Server/protocol/state 来完成这层映射，不新增 Electron 或外置编排壳。

### 6.3 三个独立产品面

#### C 端本地产品：`ai-ip-desktop`

- 正式支持 Windows 11；macOS 为邀请测试。安装包内含产品化 Codex Business Server、前端静态资源、默认业务 Skills 和本地迁移工具，不依赖 Electron。
- Business Server 启动后只监听随机 `127.0.0.1` 端口，直接提供前端页面、业务 API、文件传输和流式事件，并打开系统浏览器。Windows 启动器只负责进程生命周期、更新和打开页面，不拥有业务逻辑。
- 每次启动生成高熵 bootstrap nonce，页面交换后使用当前系统用户绑定、HttpOnly/SameSite 会话；严格校验 Host、Origin、CSRF 和 WebSocket 握手，拒绝非 loopback、跨 Origin、通配 CORS 和 DNS rebinding。该保护完全自动，不要求用户反复输入密码，也不缩减 Lead 的网络、文件或工具能力。
- 首页以 Mission、ContentPackage 和下一步行动为中心；聊天只是一种输入方式。用户不需要看见 Thread、Turn、MCP 或 Agent 编队。
- C 端产品不出现代理层级、下级客户、转点、代理价格或平台运维入口；账号到期或云端维护不能阻止用户打开、编辑和导出已有本地项目。
- `ai-ip-host` 对每个 Workspace 取得操作系统级独占锁；BusinessTask/RuntimeAttempt claim 带单调递增 fencing generation。重复启动、残留旧进程或更新器不能同时提交 Artifact、确认 outbox 或推进本地状态。

#### 代理管理系统：`partner-management`

- 独立域名、应用、版本、部署和回滚节奏，不进入本地客户产品的路由树。
- 承载一级/二级代理的客户管理、额度划拨、价格权限、限额、线下收款记录、售后授权和报表。
- 只能查看云端客户账户、设备、额度与消费汇总；不能枚举本地 Project、Mission、Thread、Skill、文件或媒体，也不能向本地客户端下发提示词或任务。
- 售后内容只来自客户主动生成的 SupportBundle；代理不能申请后台 break-glass 读取本地目录。

#### 平台内部后台：`internal-console`

- 独立产品面，只允许平台内部身份访问。承载模型与能力注册、内部成本、账本与供应商对账、异常 CapabilityOperation、合规、功能开关、版本发布、设备和脱敏运行诊断。
- 不提供客户项目浏览器，不保存客户 Prompt、脚本、素材、成片或恢复密钥。平台自营销在平台专用设备上使用同一桌面产品完成。
- 平台最小角色固定为：财务录入员只能录单、财务审批员由不同 Principal 批准、支持人员不能改账、运行人员只能读取脱敏诊断、内容运营只能访问平台自有项目、合规审核员只能执行事实/权利/发布闸门。财务录入与审批不得由同一 Principal 完成。

三个发布物使用不同 token audience 和 API allowlist；customer token 只允许调用账户、额度、能力和备份端点，partner/internal token 不能反向连接本地 Business Server。共享云端 API 以 additive 演进并保持 N/N-1 客户端兼容，避免桌面升级被任一后台发布强迫同步。

### 6.4 Codex 仓内改造边界

- `codex-app-server-transport`：承担随机 loopback 端口、同源 HTTP/WebSocket、静态资源和上传/下载 listener；`codex-app-server` 继续承担 composition、消息处理、生命周期和恢复，避免重复创建第二个 listener。
- `codex-app-server-protocol`：增加 `project/*`、`mission/*`、`proposal/*`、`artifact/*`、`generation/*`、`publication/*` 和 `backup/*` 方法与事件；原有 `thread/*`、`turn/*`、`item/*` 保留为底层运行协议。
- `codex-state`：统一管理本地数据库发现、迁移、恢复和索引；AI IP 业务表使用独立 `ai_ip_1.sqlite`，不硬塞进 Codex 现有 state 数据库。
- 新增 `codex-ai-ip-domain`：保存纯业务对象、不变量、版本和依赖图。
- 新增 `codex-ai-ip-runtime`：将 Mission、BusinessTask 与 Codex Thread/Turn/Skill/MCP/临时子 Agent 绑定，负责上下文装配和结构化产物提交。
- 新增 `codex-ai-ip-provider`：连接平台能力网关，处理显式能力 profile、预算、幂等调用、签名回执和 Responses/Search/Seedream/Seedance/TTS 能力；Mission、Artifact 和业务 Skill 不接触 provider/model 分支。
- 新增 `codex-ai-ip-media`：管理镜头、下载、FFmpeg、字幕、质检和本地导出。
- 新增 `codex-ai-ip-backup`：在设备端完成分块加密、清单、上传和恢复。
- Web 界面可以使用仓内 React/TypeScript，但其静态产物由同一个 Business Server 提供；UI 不维护第二套业务状态或决策，因此不是外部套壳。
- 客户构建必须接管 `codex-login`/`AuthManager`、`codex-model-provider-info`、`codex-model-provider` 和 `codex-api` 的真实调用链：删除 OpenAI/ChatGPT 登录与用户 provider override 心智，改用可刷新、可撤销的 DeviceRegistration 凭证；所有 Responses 调用只注册平台能力网关，开发测试旁路只能存在于不可发布的 internal build。
- `codex-model-provider` 的生产能力声明必须来自平台签名目录，不能沿用通用 provider 对工具、图片、搜索或网络能力的乐观默认。原生 Responses 与协议翻译 adapter 都在同一 contract runner 下逐字段证明；无法等价表达的供应商能力明确为 unavailable。
- 新增 Rust `ai-ip-host` 组件管理 Windows 启动、系统托盘、单实例、崩溃监督、签名更新和打开浏览器；它不保存业务状态、不调用模型，也不成为第二套后端。
- `codex-core` 只承载真正通用的业务运行模式、上下文片段、Artifact event 和工具调度；账户、账本、Project 表和具体业务流程不能继续塞入已复杂的 core。

### 6.5 本地数据与版本

- 产品使用独立配置根：Windows 为 `%LOCALAPPDATA%/<product-id>`，macOS 为对应 Application Support 目录；不读取用户现有 `~/.codex` 的 auth、threads、config、plugins、skills 或 AGENTS 规则。需要复用的 Skill/MCP 由用户在产品内显式导入。
- Project、Mission、BusinessTask、Proposal、Decision、Artifact、ContentPackage、Prediction、Publication 和 MemoryRule 进入 SQLCipher 构建的 `ai_ip_1.sqlite`。素材和大文件使用每项目 DEK 的 AEAD 信封加密内容寻址 Blob；用户主动导出时才生成明文副本，FFmpeg/外部工具只使用当前系统用户可读、任务结束即清理的临时工作文件。
- `KeyProtector` 抽象负责包裹 Workspace/Project 密钥：Windows 使用 DPAPI，macOS 邀请测试使用 Keychain。轮换优先重新包裹 DEK，不要求重加密全部大文件；账号到期或云端不可用不影响本地解锁和导出。
- Codex rollout、Thread/Turn 正文、BusinessTaskStepReceipt、工具输出和包含客户内容的诊断记录同样属于项目数据，产品 ThreadStore 必须写入上述加密边界。现有全局 JSONL、`state_*.sqlite`、thread-history/log DB 只能保存脱敏 ID、时间、状态和摘要，或在迁移后停止承载正文。
- Project 维护到 Thread/rollout/Blob 的反向索引；永久删除时清理对应内容、密钥包裹和可识别诊断副本。备份包含项目正式数据与所需 rollout/step receipt，不包含登录令牌、全局缓存和可重新生成的临时日志。
- 每次编辑、重写、生成、转码和局部重做产生不可变版本及派生关系，不覆盖历史原稿。用户可以随时比较、回退或从旧版本建立新分支。
- 文件先写临时文件，完成 checksum 和解码检查后原子改名，再提交数据库引用；禁止出现数据库显示成功但文件不完整。
- 本地更新采用 side-by-side：先暂停写入并保存“旧二进制＋预迁移数据库快照”配对恢复点，在副本上迁移和健康检查后才切换活动版本。切换前失败可以整对回滚；切换并产生新写入后禁止自动退回旧 schema，只能由新版本修复或进入只读导出，不能用恢复旧快照丢弃新数据。
- 用户主动开启备份后，本地先分块加密再上传。首版备份用于灾难恢复，不做静默双向同步；在另一设备恢复同一项目时创建明确分支。

#### 6.5.1 Evidence/provenance 与未来 factual guard 边界

Plan 03 owns the local project Evidence Store/Retrieval Adapter semantics and any future automatic factual-guard business evaluation; Plan 05 owns its durable encrypted local persistence. The substrate is business-led: typed assertions, source identities and spans, provenance, corrections, and artifact bindings. It is not a required document-library, vector-database, or knowledge-base UI; it is not cloud project truth; and it must not impose a fixed user workflow. A UI may later project business artifacts only when a user task justifies it.

An automatic guard is not implied by this data model. Before a Plan 03 child proposes one, it must freeze its factual surface over the real `ContentPackage`, the handling of labeled inference outside `Claim`, the relation to `Readiness`, a guard evaluation set (faithful paraphrase positives and unseen negatives), false-negative/false-positive thresholds, quality/token/latency/cost measurements, deletion, live commitments, and byte-replay semantics. A semantic model verdict is not deterministic truth merely because deterministic code records it; replay verifies recorded bytes and does not re-query a model. The current [OpenAI Guardrails documentation](https://developers.openai.com/api/docs/guides/agents/guardrails-approvals) is rationale for layering checks and human oversight, not this product's implementation contract.

### 6.6 业务与执行状态的边界

- Mission 和 Artifact 图决定业务进展；Thread、Turn、Item、durable rollout、BusinessTaskStepReceipt 和 RuntimeAttempt 只提供重建执行所需的历史与副作用凭证，不等于业务完成，也不承诺从任意工具指令中点续跑。
- Lead、Skill、MCP 和临时子 Agent 可以创建候选、草稿和新版本；重大定位变更由用户确认。扣费、权限、版本完整性、幂等和外部不可逆动作由代码执行。
- 每个有副作用的业务步骤在继续前先提交 StepReceipt、Artifact 或 CapabilityOperation。RuntimeAttempt 失败时从 durable rollout 和已提交业务产物重建一个新 attempt；不得盲目重放已完成副作用，也不得要求用户重新完成访谈或整条内容重做。
- 云端 `CapabilityOperation` 只决定远程能力执行和结算；供应商成功不等于本地文件已经落地。本地下载完整、checksum 一致且可解码后即可作为草稿查看、导出和修改；业务质量检查只影响 `quality_score`、`readiness` 和 Lead 的改进建议，不把低分产物变成不可访问。
- 只有出现超出已授权预算、使用未授权真人肖像/声音、向项目外发送未授权敏感原件、永久删除或执行发布等不可逆动作时硬阻断。普通事实不确定、品牌风险、争议度和媒体瑕疵可以继续生成草稿，并以状态和替代方案提示用户。

## 7. Lead 与业务能力系统

### 7.1 一个 Lead 对结果负责

每个 Mission 只有一个稳定的 Lead。Lead 理解用户目标、决定当前最有价值的下一步、管理预算、组合能力、比较候选、维护产物谱系并交付 ContentPackage。它可以在可逆范围内直接作业务判断，不需要通过固定投票，也不能把责任推给一串互相对话的 Agent。

Lead 只有在以下情况打断用户：缺少会实质改变结果的关键事实；存在多个不可兼容的重大定位；预计超出已授权预算；需要新的真人权利授权；将执行发布、永久删除或敏感外发。普通选题、表达、镜头和工具选择由 Lead 继续推进，并保留可撤销版本。

### 7.2 按需能力目录

以下是可以合并、跳过、并行或扩展的能力，不是固定 Agent 编队：

1. **业务理解**：材料解析、缺口追问、业务主体、购买/影响关系、用户画像、证据和现实限制。
2. **影响力策略**：个人、品牌、产品、组织或复合主体的定位、长期追问、内容世界、栏目、品牌声音和商业连接。
3. **内容情报**：公开网络研究、趋势、对标、平台语境、社会情绪、事实核验和证据冲突。
4. **创意生产**：选题、开头、叙事构型、口播、纪实、剧情、产品展示、标题、正文及抖音/小红书差异化版本。
5. **导演与媒体**：表演、场面、分镜、故事板、Seedream、Seedance、TTS、FFmpeg、字幕、包装、质检和导出。
6. **增长学习**：发布前预测、实验设计、指标回收、评论洞察、项目记忆和下一轮 Mission。

能力以 Skills、MCP、内置工具或临时子 Agent 实现。Codex 的搜索、Shell、代码和文件处理能力保留为幕后执行手段；系统可为一次业务任务临时写脚本、整理数据或处理媒体，不能因为它们带有 coding 属性而删除。

### 7.3 动态计划与创意竞争

- Lead 根据 Mission 创建可变 MissionPlan，可以新增、跳过、重排、回退和并行 BusinessTask；计划服务于结果，不是用户必须完成的流程。
- 不确定度较高时，Lead 可以并行探索多个实质不同的策略、选题或脚本，并调用独立反方审查，再综合证据、目标、平台、可拍性和业务连接选择主方案。
- 候选数量由真实不确定性决定，可以为零、一或多个；不得为了界面整齐固定输出三个，也不得用语气变化伪造差异。
- 外部网页只作为不可信数据，不执行其中的提示词或操作指令；研究能力学习结构和证据，不复制具体文案或特定创作者的个人语言。
- 新 Skill 或临时编排可先作为可删除实验进入内部评测；只有准备成为长期默认能力或新增硬闸门时，才必须提供 held-out 失败、基线对照、成本和删除路径。

### 7.4 业务产物，而不是聊天答案

每次有价值的执行必须形成一个或多个版本化 Artifact，例如 BusinessBrief、Persona、Strategy、ResearchPack、TopicCard、CreativeBrief、Script、Storyboard、Shot、MediaAsset、ContentPackage、Prediction 或 Retro。聊天文本本身不能替代正式产物。

Lead 必须说明主要判断依据、仍属假设的部分、用户需要做的最小行动和最终交付位置。对显式假设、普通事实不确定或媒体瑕疵，可以继续形成草稿和替代版本；不得把虚构客户故事、收益、产品能力或人物经历标记为真实。

### 7.5 主体与表达一致性

`Subject` 支持 `PERSON`、`BRAND`、`PRODUCT`、`ORGANIZATION` 和 `HYBRID`。个人 IP 维护人物语言、价值、经历和镜头表现；品牌/组织 IP 维护品牌承诺、组织责任、表达边界和不同出镜代表之间的一致性。直播公会可以由达人、招募官、运营和管理者轮换出镜，不因更换出镜者就被误判为多个项目。

## 8. 核心策划推理

### 8.1 BusinessContext 的五个商业视角

系统不从“这个行业能讲什么”开始。Lead 持续维护以下五个视角，但它们不是固定问卷、固定顺序或开工门禁；已有材料足够时自动填充，未知项显式保留，只追问当前最影响结果的一项：

1. **业务主体是谁？** 支持个人、品牌、产品、组织或复合主体；明确业务阶段、经历、立场、可调用资源、独家素材和现实限制，判断哪些话能被可信、长期地表达。
2. **希望影响谁，发生什么行动？** 建立 InfluenceRelation 与 ActionFunnel。行动可以是停留、关注、相信、咨询、购买、报名、加入、开播或持续行动；购买型任务再细分付款、购买、使用、受益、影响、推荐和渠道关系。
3. **对方为什么会行动，又为什么可能不行动？** 找到具体情境、触发事件、想获得的改变、隐性动机、机会成本、风险和反对理由。
4. **对方为什么相信、选择、关注或加入这个主体？** 找到可展示的证据、差异、关系基础、过程透明度、案例、权威来源和长期一致性，不能用自我评价代替信任依据。
5. **什么内容能让对方从陌生走到行动？** 分别设计停留、理解、关注、信任、咨询或购买等阶段目标；一条内容优先聚焦一个主要的可观察行动，但不把真实传播和转化强行简化成单变量实验。

前两项确定“谁在影响谁”，后三项把宽泛受众收敛为可创作、可验证、可转化的关系。状态要版本化，并关联用户材料、基础核验或明确标记的假设及信心等级。Lead 可以在部分未知时先完成不依赖该未知的研究、草稿或低成本试验，不能把推断静默写成事实，也不能因缺少不相关字段拒绝工作。

### 8.2 受众与行动假设不是人口标签

一个项目可以有零个、一个或多个 AudiencePersona；直接脚本、活动响应或探索型 Mission 不因缺少完整画像而停工。Lead 只填写与当前目标相关的字段，可记录角色、与主体的关系、进入行动状态的情境、期望结果、阻力、信任证据、表达方式、平台、行动路径、来源、信心和推翻条件；未知字段保留未知，不要求凑齐模板。

“25—40 岁女性”“宝妈”“老板”只能作为线索，不能单独构成受众判断。黄金礼品中的送礼者与收礼者、本系统中的一级代理与 C 端用户、直播公会中的公会品牌与潜在达人等关系必须拆开处理。人物定位、品牌承诺、核心受众或目标行动发生重大变化时由用户确认；普通细节允许 Lead 形成新候选版本并继续可逆工作，不能静默覆盖已确认版本。

### 8.3 从用户画像进入内容世界

系统研究该个人、品牌或组织及其真实业务生态中反复发生的事件、相互影响的人、各自的欲望与损失、必须作出的选择，以及这些选择怎样连接产品或行动目标。画像与研究可以互相补充，不要求先完成一张“完美画像”才开始。系统据此寻找可持续的叙事发动机，例如身份变化、隐藏机制、利益冲突、小人物与大趋势、承诺代价和关系误解。

### 8.4 本系统自己营销自己

平台建立一个平台所有的内部 IP 项目，由第 4.3 节定义的平台内容运营负责人在平台专用设备上操作，使用与客户相同的 Mission、Lead、业务能力、媒体和复盘机制。该项目至少区分一级代理、二级代理和 C 端用户三个受众轨道；系统不得把渠道购买者、渠道经营者和最终使用者混成同一个画像。

系统根据不确定性生成必要数量的总方向并推荐主方案。三个轨道可以共享品牌主体和长期承诺，但分别维护信任证据、栏目组合、转化动作、禁用表达、预算和效果基线；如果主体责任、品牌承诺或账号受众发生实质冲突，应另建项目，而不是用固定三选一掩盖冲突。

自营销内容只能使用平台自身真实能力、公开资料、内部实测和取得明确授权的客户案例，不能读取或暗用客户私有项目。它与客户项目共用同一不可逆边界和 AI 标识规则；普通事实、营销争议和媒体质量以标注、评分及替代版本处理，不建立额外审批流水线。该项目不得拥有硬编码高分、特殊提示词或专用宣传模板；发布结果进入独立项目记忆，不能污染任何客户项目的画像、基线或经验。

### 8.5 候选策略比较

候选策略按以下维度比较：

- 是否准确对应当前 InfluenceRelation、ActionFunnel 和已有受众证据。
- 是否持续有真实事件发生。
- 商家是否拥有第一手素材或可靠研究能力。
- 是否有鲜明的人物、组织关系、机制或矛盾。
- 是否可连续生产一年以上。
- 是否适合目标平台。
- 是否保持业务联想。
- 是否真实、合规且可以拍摄或生成。

## 9. 内容和媒体生产

### 9.1 创作母版

Lead 为内容维护可变 CreativeBrief，可包含核心问题、主体、变化、矛盾、观众认知变化、事实素材和业务连接。不同题材不要求每项齐全：机制拆解、品牌说明、产品演示或招募内容可以没有固定主人公。只有用户主动锁版、进入高成本不可逆生成或形成发布前预测时才冻结对应版本；创作早期可以边研究边修改。

### 9.2 正式媒体能力

- 火山方舟 Responses API：首条真实文本 Agent、工具调用和多模态理解链路。
- Seedream：角色、场景、商品参考、封面和故事板。
- Seedance：AI 情景镜头、视频编辑和延展；所有结果必须质检。
- 豆包标准 TTS：口播、旁白和角色声音。

### 9.3 有条件能力

- 声音复刻：只有完成服务开通、音色购买/授权和商业合同核对后开放。
- Seedance 授权真人肖像：只能使用火山认可的本人认证授权资产链路。

### 9.4 Beta 能力

客户克隆数字人、单图音频驱动、OmniHuman 和奇美拉相关能力保持独立功能开关。只有同时完成以下条件才能升级为正式能力：

1. 实际生产火山账号具备接口权限、配额和稳定调用能力。
2. 完成一轮真实客户肖像与声音授权流程。
3. 企业合同书面确认平台代客户生成及商业分发权利。
4. 完成批量稳定性、费用、失败恢复和媒体质检测试。
5. 完成数字人标识、停用、撤回和删除流程。

### 9.5 商品真实性

带货内容中的商品外形、包装、颜色和功能以真实素材为准。真实产品图片或视频可与 AI 人物合成；视频模型不能凭空创造商品能力、演示结果或前后对比。

### 9.6 成本控制路径

```text
角色与声音
→ 故事板
→ 低清动态预览
→ 台词与节奏
→ 高清镜头
→ 最终剪辑
```

这是 Lead 可选择的成本控制路径，不是所有内容必须走完的确认流水线。用户先授予 Mission 预算；预算范围内可以自动完成故事板、预览和局部试错。只有预计超出总预算、超过单次高成本上限或改变重大创作方向时才请求确认。单个镜头失败时只重做该镜头。

## 10. 数据对象与状态

### 10.1 核心对象

本地业务库保存：

- Workspace、Subject、IPProject、Mission、MissionPlan、BusinessTask、RuntimeAttempt、ThreadBinding。
- BusinessContextVersion、InfluenceRelationVersion、ActionFunnelVersion、PurchaseRoleRelationVersion、AudienceTrack、AudiencePersonaVersion、BrandVoiceVersion、OrganizationProfileVersion、EpistemicAssertion。
- Proposal、ProposalAcceptance、Decision、Evidence、ResearchClaim、BenchmarkAnalysis。
- StrategyVersion、TopicCard、CreativeBrief、ContentVersion、PlatformVariant、ContentPackage。
- CharacterProfile、VoiceAsset、ConsentRecord、Scene、Shot、MediaGenerationTask、MediaAsset、QCReport。
- Prediction、Publication、MetricSnapshot、CommentInsight、Retro、MemoryRule。
- CapabilityOperation、DisclosureManifest、SignedProviderReceipt、LocalAuditEvent、LocalOutboxEvent、SupportBundle。

云端商业库只保存：

- UserCredential、PrincipalProfile、AuthSession、DeviceRegistration、Account、ResellerRelation、CUser、PlatformOperator、PlatformRoleGrant。
- CreditWallet、LedgerEntry、TransferOrder、PricingVersion、MissionSpendGrant、CapabilityHold。
- CloudCapabilityOperation、ProviderAttempt、ProviderCostEntry、InternalCostCenter、InternalBudgetHold。
- ModelRegistryEntry、CapabilityEntitlement、CloudAuditEvent、CloudOutboxEvent、EncryptedBackupManifest 和密文块索引。

`Subject(kind = PERSON | BRAND | PRODUCT | ORGANIZATION | HYBRID)` 是项目责任主体，`CharacterProfile` 只是可选内容资产。直播公会固定案例必须使用 BRAND/ORGANIZATION，并记录达人看见、报名、审核、加入、首次开播和持续开播等结果，不能映射成老板个人画像。

`Mission` 是顶层业务目标，`BusinessTask` 是 Lead 可动态重排的工作单元，`RuntimeAttempt` 是一次 Codex 执行尝试。`CapabilityOperation` 表示一次本地远程能力意图；云端同 ID 的 `CloudCapabilityOperation` 负责额度和供应商执行。它们互相映射但不共享项目正文。

Strategy、Persona、Topic、CreativeBrief 等都是可选类型化 Artifact。任何 Artifact 都能从用户输入、其他 Artifact 或一次临时 Mission 派生；不要求形成“BusinessContext → Persona → Strategy → Topic”的强制外键链。每次修订生成新版本和 provenance 边，历史内容不会自动指向新版本。

本地 Project 的所有者可以是客户账号或平台内部运营账号；二者使用相同本地领域模型但位于不同设备和工作区。代理关系只存在云端商业库，不能成为本地项目父级。

### 10.2 项目状态

```text
ACTIVE / PAUSED / ARCHIVED / READ_ONLY_RECOVERY
```

项目状态只表达生命周期，不编码业务流程。画像是否完整、是否已有策略、当前在写脚本还是做视频都由 MissionPlan 和 Artifact 图表达，不能成为创建合理产物的前置锁。

### 10.3 Mission 与产物状态

```text
Mission: RUNNING / WAITING_USER / WAITING_NETWORK / COMPLETED / CANCELLED
BusinessTask: PLANNED / RUNNABLE / RUNNING / BLOCKED / COMPLETED / SKIPPED / FAILED
Artifact: DRAFT / CANDIDATE / ACCEPTED / SUPERSEDED / PUBLISHED
```

执行状态、计费状态和交付就绪度必须分开。RuntimeAttempt 失败不等于已有草稿失效，费用进入对账中不等于 ContentPackage 不可查看，普通风险警告也不应把整个 Mission 标成失败。

### 10.4 媒体任务状态

媒体同时记录三个正交状态，不能串成一条固定流水线：

```text
execution_state: PLANNED / SUBMITTED / RUNNING / SUCCEEDED / FAILED / CANCELLED
delivery_state: PENDING / DOWNLOADED / VERIFIED / DELIVERED / REDO_RECOMMENDED
billing_state: UNHELD / HELD / SETTLED / RELEASED / RECONCILIATION_REQUIRED
```

低清预览、故事板和高清生成是 Lead 可选的成本路径。`VERIFIED` 只表示文件完整且可解码；业务质量记录为独立评分、问题和可选局部重做建议。供应商回调采用 `provider + provider_task_id` 唯一约束；回调和轮询进入同一状态归并器，重复、乱序和延迟回调不能重复扣点或重复创建下游任务。

### 10.5 本地与云端真相边界

| 数据 | 所有者 | 能否决定业务结果 |
|---|---|---|
| Project、Mission、业务上下文、Artifact、ContentPackage、本地权利记录、Prediction、Publication、Metric、Memory | 本地 AI IP 业务库和项目目录 | 可以，是内容业务的唯一真相 |
| Codex Thread、Turn、Item、durable rollout、StepReceipt、RuntimeAttempt | 本地 Codex runtime | 只负责执行重建、去重副作用和观测，不能静默改正式版本 |
| Account、Device、Entitlement、CreditWallet、Ledger、Pricing、Hold | 云端商业控制面 | 只决定账号、远程能力资格和创作点真相 |
| CloudCapabilityOperation、ProviderAttempt、ProviderCostEntry、SignedProviderReceipt | 云端能力网关 | 只决定远程调用、费用和对账真相 |
| 临时 TOS 对象 | 云端操作性暂存 | 只用于供应商异步结果恢复，不是项目资产或备份 |
| 加密 Backup 块 | 云端备份存储 | 只有客户端持有密钥并恢复校验后才成为本地项目数据 |

模型或工具输出可以立即保存为草稿 Artifact；结构错误、普通事实不确定或质量警告不应让整个 Mission 失败。只有用户接受、Lead 在授权范围内选择或用户主动锁版后，产物才成为正式版本。RuntimeAttempt 的 `completed`、供应商的 `succeeded` 和云端的 `settled` 均不能单独证明 ContentPackage 已交付。

### 10.6 Mission、执行与远程调用映射

```text
Mission
└─ BusinessTask
   ├─ RuntimeAttempt
   └─ CapabilityOperation
      └─ ProviderAttempt
```

- 一个 BusinessTask 可以有多个 RuntimeAttempt；失败后创建新 attempt，并复用已有 Artifact。临时子 Agent 归属于当前 attempt，不另建顶层业务任务。
- 一个 CapabilityOperation 对用户具有固定最高扣点和稳定 `operation_id`；平台为完成它可以创建多个 ProviderAttempt，但平台重试、切换供应商或崩溃恢复不能再次向用户收费。
- `operation_id` 在云端单独全局唯一，`request_digest` 是该行不可变字段。重复 ID 时摘要相同则返回已有状态或结果，摘要不同则返回冲突；禁止把两者做成允许同 ID 多行的联合唯一键。
- 本地 Runtime 可以进行不涉及平台密钥的读取、写作、脚本和媒体处理；所有平台模型或供应商能力必须通过本地 CapabilityOperation 与 outbox，不能由 Skill、MCP 或 Shell 绕过计费入口。
- 生成草稿、执行成功、费用结算和发布就绪分别记录；任何一个维度不能替代其他维度。
- 云端余额是创作点真相；本地保存签名回执和最近余额用于展示与恢复，不允许离线凭缓存发起新的付费调用。

### 10.7 本地 Mission 与云端调用黄金链路

```text
用户提出目标并授权 Mission 预算
→ Lead 在本地创建动态计划并执行可逆工作
→ 形成 Proposal / Artifact / DisclosureManifest
→ 需要远程能力时，本地事务写 CapabilityOperation + LocalOutboxEvent
→ 云端在同一 PostgreSQL 事务中校验资格并写 hold + CloudCapabilityOperation + Audit + Outbox
→ 能力 Worker 幂等调用 Responses / Search / Seedream / Seedance / TTS
→ 云端保存费用证据和签名回执，异步结果仅进入临时保全区
→ 本地按 operation_id 恢复、下载、校验并形成新的 Artifact
→ 云端结算实际费用并释放未使用额度
→ Lead 继续 Mission，最终交付本地 ContentPackage
```

文本请求默认流式中转，云端持久日志不得保存原始 Prompt、上下文或模型正文。异步媒体允许进入按 operation 隔离的加密临时区；完成或终止后按配置自动删除，最短满足断网恢复，最长不得变成无限期项目存储。

杀死浏览器、Business Server、RuntimeAttempt、网络、云端 Worker、供应商回调或下载进程后，恢复不得创建重复用户扣点、重复有效供应商任务、丢失已落地 Artifact，或要求用户重新完成已保存的业务理解。

## 11. 研究、营销与真实性平衡

系统把内容分为四类：

1. **个人判断**：可以尖锐，明确属于立场。
2. **商家经验**：必须真实发生，并表明是自身观察。
3. **公共事实**：后台核实，成片不要求展示论文式引用。
4. **重大因果、功效、收益和政策主张**：需要更强依据，不能靠修辞越过。

研究事实账本记录陈述、来源、日期、适用范围、类型、冲突来源、可信程度和重新核实时间。

上述内容类别之外，每一条会影响画像、策略、选题、叙事或发布判断的陈述还必须标记一种认识状态：用户原话、外部证据、模型解释、创意假设、未知或真实结果。模型解释和创意假设不能静默升级为事实；未知不能写成零、否定或模型自行补齐的经历、能力、案例、资源和地域判断。

认识状态用于帮助 Lead 在不确定中继续工作，而不是把未知全部变成禁区。模型解释和创意假设可以进入明确标记的草稿、剧情重构和测试版本；当内容准备被当作真实商业主张、真实经历或真人指控发布时，才要求相应证据或改写为清楚的观点、假设、匿名化或虚构表达。

允许选择性叙事、延迟揭示和观点先行，但整个内容结束后不能留下虚假事实。真实可识别人物不得因悬念设计受到不必要污名。

## 12. 发布预测和学习闭环

### 12.1 发布前预测

系统在 ContentPackage 锁版时自动记录相关策略、选题、营销结构、形式、平台、主要变量、预期指标变化、预期评论、失败原因和信心等级，不要求用户再填一张表。单一主要变量是便于学习的建议，不是复杂真实内容的硬限制。预测冻结后只能追加说明，不能篡改。

### 12.2 数据回收

用户手动提交链接、表单或后台截图。系统保存原始截图和提取结果；识别不确定的数字必须由用户确认。

用户必须同时登记实际发布版本。复盘以最终成片、标题、正文和发布时间为准，不能用 Agent 原稿解释用户修改后的内容表现。

按平台保存播放/曝光、观看、完播、互动、收藏、分享、涨粉、主页访问和可获得的商业指标。T+1、T+3、T+7 是内容传播的默认窗口；每个 Mission 还要预注册适合自身结果的归因窗口和事件，招募型默认增加 T+14、T+30，并记录达人报名、审核通过、加入、首次开播和持续开播。

### 12.3 诊断层级

- 注意力：是否点开或停下。
- 留存：开头承诺是否被内容接住。
- 共鸣：是否互动、收藏、转发和表达立场。
- 行动：是否关注、进入主页、咨询或成交。

经验依次经过：单次观察、候选假设、多次重复、项目规则、反例降级。Lead 可以自动调整开头、节奏、平台包装和选题角度等可逆战术并保留 diff/undo；主体身份、品牌承诺、核心受众或长期定位发生重大变化时才形成用户待确认的新版本。

## 13. 创作点与代理结算

### 13.1 线下收款

- 一级代理线下向平台付款，平台确认后充值创作点。
- 二级代理线下向一级代理付款，一级代理确认后划拨创作点。
- C 端线下向所属代理付款，代理确认后充值创作点。
- 平台和代理的凭证确认、充值、划拨、退款与对账均在 `partner-management` 或 `internal-console` 完成；本地客户产品只显示当前客户钱包、消费明细和待处理结果。
- 付款截图仅为凭证，不能自动触发充值。
- 首版不接在线支付，不做自动分账。

### 13.2 两本账

1. 现金订单记录：谁向谁支付了多少；线下真实金额未录入时只能显示估算。
2. 创作点账本：发行、购买、划拨、冻结、结算、释放、补偿和退款。

创作点账本采用追加式分录，禁止修改旧记录。余额是可重算的汇总，不是唯一事实。

余额不得为负。下级充值或代理间划拨必须在一个数据库事务中同时扣减上级、增加下级并写入关联分录；并发请求不得造成超额划拨。未使用额度向上退回时走反向申请和审批，不能直接修改余额。

### 13.3 Mission 预算与远程能力扣费

```text
用户一次授权 Mission 总预算与可选单次上限
→ 每次远程操作返回价格版本和最高扣点
→ 冻结创作点并创建唯一 CloudCapabilityOperation
→ 调用供应商
→ 记录供应商实际消耗
→ 按同一 operation 结算
→ 未使用部分释放
```

在已授权 Mission 预算内，Lead 可以连续调用文本、搜索、图片、视频和语音能力，不重复弹出付款确认。只有预计超过 Mission 总预算、单次高成本上限或改变重大任务目标时才请求新的授权。

云端 `MissionSpendGrant` 是 Mission 总预算权威；创建 CapabilityHold 的同一事务必须锁定 Grant，并验证 `settled + active_holds + new_hold <= authorized_limit`。本地预算只是规划与展示缓存，多个并发操作不能各自绕过总预算。

云端冻结、CloudCapabilityOperation、CloudAuditEvent 和 CloudOutboxEvent 必须在同一个 PostgreSQL 事务中完成。`operation_id` 单独全局唯一，`request_digest` 在该行不可变；平台内部重试、供应商切换和崩溃恢复不得产生第二次用户收费。用户主动要求额外版本、额外镜头或改变目标时才创建新的计费操作。

取消已被供应商接受的任务时，只结算已有证据且不可撤回的实际消耗，不得超过冻结上限。供应商成本暂时不明确时进入“对账中”，不能假装零成本或再次扣款；七十二小时仍无法确认时释放用户冻结额度，由平台承担后续迟到成本。

平台、代理和 C 端分别具有单任务、单日和单月创作点上限。达到上限时暂停新付费任务，不能在后台继续产生未展示费用。

平台自营销项目使用独立内部成本中心，不进入创作点钱包。其流程为：按人民币分授权 Mission 预算；每次远程调用创建 `InternalBudgetHold`、CloudCapabilityOperation 和 outbox；任务结束后按 `ProviderCostEntry` 结算实际成本并释放余额。供应商原生用量和账单币种同时保留，人民币换算引用当时的成本价格版本。该流程不消耗可转售给代理或 C 端的创作点，也不计为代理收入；内部项目同样受单任务、单日和单月预算上限约束。

### 13.4 价格权限

- 平台设置一级进货价、最低转售价、能力消耗价和活动规则。
- 一级代理设置二级进货价和直属 C 端售价。
- 二级代理设置直属 C 端售价。
- 价格变更只影响未来订单。

## 14. 权利、隐私和合规

### 14.1 数据地域

- 客户项目、人物资产、数据库和项目文件默认保存在用户位于中国大陆的本地设备；云端账号、账本、能力操作、临时媒体和加密备份部署在中国大陆。
- 每次远程调用由 Lead 生成本地可查的 DisclosureManifest，列出为完成任务发送的字段、产物和素材。发送范围以业务效果所需为准，但不得夹带无关项目内容；普通调用不逐次弹窗。
- 文本请求默认流式中转，云端持久日志不保存原始 Prompt、上下文或模型正文。异步媒体只进入按 operation 隔离并有自动删除期限的临时区，不形成云端项目库。
- 首版不由系统向 TikTok 或境外服务发送个人信息和人物素材。
- 用户自行下载成片并发布海外平台。

### 14.2 数字身份授权中心

肖像许可、声音许可和敏感个人信息同意分别记录，不使用一个笼统勾选框替代。记录授权人、用途、平台、项目、地域、期限、次数、商业使用、是否允许建模、是否允许复用、是否允许转交火山处理、撤回和历史成片处理规则。

权利记录保存在本地项目。声音或肖像本人应通过可验证方式直接确认高风险授权；调用云端能力时只提交完成本次处理所需的授权声明、范围和校验摘要。撤回后停止新生成并冻结相关数字资产，但不删除依法需要保留的历史记录。

### 14.3 未成年人

- 注册用户限定十八周岁以上。
- 上传儿童素材必须记录年龄和监护人授权。
- 儿童肖像和声音建模默认关闭，进入单独高风险流程。
- 自动提醒隐藏学校、住址、定位、车牌、作息和医疗信息。
- 代理和平台云端后台不能查看儿童素材；SupportBundle 默认排除儿童原件。

### 14.4 AI 内容标识

- 文本、图片、音频、视频和虚拟场景进入统一标识流水线。
- 数字人口播和 AI 情景剧添加显式标识。
- 文件元数据写入生成合成属性、服务信息和内容编号等隐式标识。
- 每次转码、加字幕和导出后重新检查标识。
- 首版不开放去除显式标识的例外功能。

### 14.5 上线前外部确认

产品在外部确认完成前也必须具备输入审核、生成结果审核、用户申诉、公众投诉、违法滥用处置、内容删除和审计留存功能。模型名称、模型备案信息、本应用登记或上线编号及适用功能进入“模型合规注册表”，并按主管要求在产品显著位置展示。

以下事项必须在公开商业上线前由律师、客户经理或主管部门形成书面结论：

- 应用登记、算法备案、安全评估的具体组合。
- ICP 备案之外是否需要增值电信业务经营许可。
- 平台是否构成广告经营者及广告档案义务。
- 数字人、声音复刻和真人人像的企业商业授权。
- 代理预充值模式的合同、税务、发票、消费者权益与层级销售风险。
- 网络安全等级保护定级。
- 未成年人商业数字分身和亲子带货的具体边界。

### 14.6 创作自由与硬边界

- 事实不确定、品牌争议、叙事张力、普通媒体瑕疵和平台风格风险默认给出标签、理由及替代版本，不阻止继续生成草稿或导出工作文件。
- 只有超出已授权预算、使用未授权真人肖像/声音、向项目外发送未授权敏感原件、永久删除或执行发布等不可逆动作由代码硬阻断。
- 硬边界触发时，系统必须同时提供能继续推进业务的合法替代路径，例如匿名化、改成明确虚构、改用合成角色、降低成本档位或保存为未发布草稿；不能只说“无法完成”。

## 15. Codex 本地产品、国产模型能力与云端薄控制面

### 15.1 能力注册表

云端后台保存能力、完整模型 ID、不可变 deployment/revision、生命周期、账号可用状态、输入输出模态、参数范围、价格版本、质量版本、处理地域、供应商留存条款和回归测试结果。本地产品只缓存已签名、带有效期的 `ProviderCapabilityProfile`，不依赖写死的模型名称。稳定 `ProviderRouteIdentity` 精确包含 `capabilityKey`、`capabilityContractVersion`、`qualityTier`、`qualityProfileVersion`、`provider`、`model`、`deployment`、`immutableRevision`、`adapterVersion`、`executionProfileVersion`、`processingRegion`，按 typed RFC 8785 JCS 取 SHA-256 得到路径安全 `routeId`。所有 key 必须存在、字符串非空 NFC；只有无 deployment 维度时该字段可为 JSON null，`processingRegion` 未取得合同证据时不能进入 E2。每个 E0–E4、G4p、G4b、G7 和 release 证据另以 `ProviderRouteEvidenceRef` 绑定 routeId、stage、`testedForkSha`、stage-specific subject/dependency closure、runner/suite 和原始证据 digest；E3 closure 必须包含实际 Business Runtime、Lead instructions、生产 Skills、Prompt/context/schema，不能只看 adapter。跨 fork 只在相应阶段 closure 被证明未漂移时引用旧证据，否则重跑受影响阶段。低级状态不能覆盖高级状态，route identity、evidence ref 和状态只属于 registry/router/attempt/receipt，不写回 Mission 或 Artifact。

新模型不能自动进入生产。必须先运行固定测试集，再由平台管理员切换。Retiring 模型提前迁移；Shutdown 模型禁止创建新任务。

Plan 06 的 provider contract owner 在创建 routeId 后持续承担该路线的 qualification ownership。首发火山路线和后续非火山 G4p 路线都在所需 runner 实际存在后，由同一 owner 校验各阶段 closure 并签署 `ProviderRoutePreReleaseRecord`；Plan 14 只消费该记录并为目标 customer/default/advertised release 或自动切换签发最终 `ProviderRouteQualificationBundle`。内部/邀请 evidence canary 不等待 pre-release/final bundle，而由签名 `ProviderRouteCanaryAuthorization` 精确限制 routeId、目标阶段、允许用户/构建、预算、时限、地域和权利范围。

### 15.2 能力网关协议

- Codex 文本模型通过平台提供的规范化 Responses 子集调用。火山方舟作为首条真实 provider；用户确认其现有方舟凭证包含 GLM 访问范围，因此先把 `provider=Volcengine Ark` 的 GLM 建成独立 Agent model route 并参加业务盲评。该路线不能冒充跨 provider G4p；通义作为原生 Responses 协议对照，后续非火山 provider 仍按能力矩阵选择。
- 原生 Responses adapter 与 Chat Completions/厂商原生协议翻译 adapter 使用同一合同 runner，逐项验证流式文本、结构化 Artifact、工具闭环、多模态声明、取消、usage、实际 revision 和错误语义；不能因 URL 或 compatibility name 看似兼容就宣布可用。
- customer build 的模型调用顺序固定为：用 DeviceRegistration 刷新短期设备令牌 → 创建/恢复 CloudCapabilityOperation 与 hold → `codex-model-provider` 经 `codex-ai-ip-provider` 调用平台 Responses endpoint → 持久化签名用量与结算回执。OpenAI/ChatGPT OAuth、环境 API key 和本地 provider override 在该构建中不可达。
- Web Search、Seedream、Seedance 和 TTS 作为有版本的业务工具能力暴露；每次调用都绑定 CloudCapabilityOperation、价格版本和最终供应商回执。
- 平台密钥只存在云端。任何本地 Skill、MCP、脚本或浏览器页面都不能取得供应商密钥。
- 首版不承诺跨供应商自动故障切换。未来只有 routeId 独立取得 E4、G4p、对应 G4b/G7，并由 Plan 14 签发最终 `ProviderRouteQualificationBundle`，且地域、权利、留存与预算范围不扩大时，才可进入自动切换候选集；`submission_unknown`、供应商任务已受理或部分流已交付等不可安全重放状态不得跨 provider 自动重试。价格超预算、权利范围改变或输出语义明显变化时必须返回 Lead 重新决策。
- GLM Coding Plan 等限定个人或指定工具使用的订阅额度不能进入产品网关、转售或代理调用；正式适配必须使用允许产品集成的开放平台 API 或单独企业合同，并在能力 profile 中保存合同范围证据。

### 15.3 媒体结果保全与本地落盘

Seedream、Seedance 和 TTS 返回地址均视为临时地址。云端先按 operation 隔离保全，再由本地客户端下载、校验 checksum、验证可解码性并写入项目目录；供应商 URL 或 TOS 临时对象不能直接成为项目资产。

- 文本和同步输入的明文只在隔离的供应商适配进程内存及实际供应商处理中短暂出现，不进入数据库和持久日志。异步供应商若要求对象输入，使用每 operation 独立、由服务处理层管理的 DEK 加密暂存；平台在处理期间技术上需要解密转交供应商，因此必须如实披露，但不提供后台读取接口。供应商终态后二十四小时删除，绝对不超过七天。
- 输出由隔离 Worker 取得并完成 checksum 后，立即使用 DeviceRegistration 中的设备公钥重加密；云端不保存设备私钥或可展开输出的密钥。收到本地下载 ACK 后二十四小时删除，未 ACK 时绝对不超过三十天。
- 端到端备份是另一条数据路径：密钥只在客户端，平台从始至终不能解密。操作性暂存不能改名成备份，也不能通过延长 TTL 形成客户项目库。

### 15.4 实施技术选择

- 代码底座：锁定提交的 OpenAI Codex Apache-2.0 整仓 fork；本地业务能力直接进入 Codex Rust workspace，不通过外部 SaaS 壳调用通用 App Server。
- 本地服务：Rust `codex-app-server` 是唯一 composition root，直接提供同源 HTTP、WebSocket、静态资源、文件和流式协议。
- 本地前端：React、TypeScript 和 Vite 产出静态资源并嵌入/随 Business Server 分发；无 SSR、无 Electron、无第二套业务 API。
- 本地数据：Codex `state` 生命周期下的加密 SQLite、不可变业务版本和项目私有文件目录；本地全文检索优先，不引入独立向量数据库。
- 本地媒体：捆绑并版本锁定 FFmpeg；命令由受控参数构造，但 Lead 可以通过高层工具完成剪辑、字幕和转码，不要求用户接触命令。
- 云端控制面：Rust/Axum 服务与 SQLx/PostgreSQL 保存 IAM、账户、账本、价格、能力操作和回执；RocketMQ 只用于异步供应商任务通知，Redis 只在真实需要时承担限流或短期协调。
- 云端 Web：`partner-management` 与 `internal-console` 是两个独立 React/TypeScript 发布物，只调用各自 allowlist API，不共享路由树。
- API 契约：本地浏览器只访问 localhost Business Server；Business Server 使用设备绑定凭证调用云端 customer allowlist。partner/internal token 不能调用 customer device API，customer token 不能调用代理或内部管理端点。
- 测试：保留 Codex 原生 Rust 测试；新增本地领域、Mission/Runtime 映射、恢复、Windows 文件语义、云端账本、能力幂等、供应商回调和三个发布物的契约与端到端测试。

### 15.5 云端推荐部署

云端薄控制面优先在华北 2（北京）主地域部署；购买前确认目标规格和各产品地域可用性。

- APIG 只暴露账号、设备、额度、能力、备份、partner 和 internal allowlist。
- VKE 跨可用区运行控制面、能力 Worker 和两个 Web 后台。
- PostgreSQL 一主一备是账号、账本、价格和远程能力操作的唯一云端真相。
- RocketMQ 多可用区承担异步供应商任务通知、重试和死信；所有消息可重复、延迟和乱序。
- TOS 使用独立的操作性暂存桶和端到端加密备份桶；前者强制生命周期删除，后者只保存客户端密文。
- TLS、日志、指标和告警不得记录原始 Prompt、客户项目正文、人物原件或备份密钥。

### 15.6 Windows 分发与更新

- Windows 11 使用代码签名安装包；安装器包含固定 fork 版本、前端资源、默认 Skills、FFmpeg 和迁移工具，并验证每个组件摘要。
- 更新器等待旧 Business Server 释放 Workspace 锁，并按第 6.5 节建立 side-by-side 配对恢复点。切换前失败回到旧二进制和旧快照；切换后已有新写入时不得自动回旧 schema。高风险更新先进入小流量渠道；强制升级窗口只能暂停新的云端能力调用，不能把仍兼容的数据锁在本地或阻止导出。
- 浏览器关闭不等于任务取消；Business Server 可以继续已授权 Mission，并在系统托盘或重新打开页面后显示进度。永久后台常驻和开机自启默认由用户选择。
- macOS 使用相同协议和业务测试，但 v1.1 只作为内部与邀请测试，不阻塞 Windows 正式版。

## 16. 失败恢复和审计

### 16.1 通用错误信息

需要用户处理或影响最终结果的失败，应说明发生了什么、哪些内容已保存、用户需要做什么、系统将如何继续。已被系统自动恢复且不影响结果的瞬时错误只进入可查看运行记录，不反复打断用户。

### 16.2 必须可恢复的异常

- 模型、搜索或供应商暂时不可用。
- 网页无法读取。
- 上传文件损坏。
- AI 输出不符合结构。
- 用户重复点击。
- 两个本地任务同时修改同一对象。
- 截图数据识别不确定。
- 回调重复、乱序或丢失。
- 媒体成功但临时链接即将过期。
- 生成内容安全拦截。
- 浏览器关闭、Business Server 崩溃、Windows 重启或网络中断。
- 本地数据库、文件、云端账本、队列或单可用区短暂故障。

所有任务应从最近成功的 Artifact 或 StepReceipt 继续，不要求用户重新完成访谈或整条视频重新生成。在第 15.3 节操作性保留期内直接恢复；结果尚未被本地确认就因平台暂存丢失或过期时，由平台承担同等质量重生成或退还相应创作点，不能要求用户重复付费。

### 16.3 删除与恢复

- 普通删除进入回收站。
- 永久删除需要二次确认。
- 本地数据库引用与项目文件使用可恢复删除；只有用户明确永久删除后才清理。本地模型缓存和临时文件按计划回收，不能误删正式 Artifact。
- 用户删除项目时分别选择“仅删除本机”或“同时删除端到端加密备份”；云端返回可验证删除回执。操作性媒体暂存按强制生命周期删除。

### 16.4 审计

本地追加记录操作者、Lead/Skill、提示词/规则版本、模型版本、输入来源、修改、用户决策、披露清单、费用回执、重试和结果。云端只记录账号、设备、能力、费用、供应商尝试和脱敏故障字段；不得把本地完整审计日志自动复制到云端。任何日志都隐藏密钥、证件、地址、音色训练原件和恢复密钥。

### 16.5 本地启动恢复

取得 Workspace 独占锁和新 fencing generation 后，Business Server 每次启动执行恢复扫描：

1. 释放失效的本地执行 lease。
2. 重发尚未得到云端确认的 LocalOutboxEvent。
3. 查询所有非终态 CapabilityOperation 的云端状态和费用。
4. 下载并校验已完成但尚未落地的远程结果。
5. 根据 durable rollout、StepReceipt 和已提交 Artifact 重建新 RuntimeAttempt；不假设能从崩溃中的任意工具步骤原地续跑。
6. 清理、续传或隔离未完成的临时文件。
7. 将恢复结果作为新事件追加，不能改写历史。

断网时用户仍能打开、搜索、编辑、版本化和导出全部本地项目，并运行不依赖云端的文件与媒体操作。未被云端接受的任务进入 `WAITING_NETWORK`，不冻结创作点；已经提交的异步任务继续在云端执行，重连后按 `operation_id` 恢复。

### 16.6 可选端到端加密备份

- 备份默认关闭，定位为灾难恢复，不是多设备实时同步。
- 备份单位固定为单项目只读一致性 `ProjectBundle`，不直接上传正在写入的共享 SQLite。客户端先取得 SQLite 一致性快照和 Artifact 高水位，导出该 Project 的领域行、所需 rollout/StepReceipt、版本关系和 Blob 清单，再在离开设备前分块加密。
- 上传顺序固定为内容寻址密文块在先、加密 Manifest 最后原子发布；未发布完整 Manifest 的批次不可见、不可恢复。平台只保存密文、版本和完整性摘要。
- 用户必须保存恢复口令或恢复文件；平台不能代替用户解密。备份不包含登录令牌、供应商密钥和可重新取得的操作性暂存。
- 增量备份可以中断续传，快照取得后不阻塞 Lead 工作。新设备恢复同时需要账号身份和恢复密钥，逐块校验后生成新设备密钥。
- v1.1 不执行双向自动合并；另一设备恢复同一项目时创建明确分支，避免静默覆盖。

### 16.7 云端灾难恢复

- 已确认的 Ledger/Hold/CloudCapabilityOperation 事务使用同步高可用，目标 RPO=0；备份与分析副本可以有独立 RPO，但不能覆盖账本承诺。灾难恢复后先用本地签名回执、云端 outbox、供应商请求清单和最终账单完成缺口核对，再开放新的扣费调用；核心商业服务目标四小时内恢复。
- TOS 备份桶保存客户端密文并校验完整性；操作性暂存桶只在固定保留窗口内跨可用区保存密文，不做长期备份。确认下载前对象不可恢复时按平台责任重生成或退款，不能声称仅凭任务状态和回执重建媒体字节。
- 每月至少完成一次账本、供应商任务或加密备份恢复演练，每季度完成一次完整云端灾难恢复演练；只有实际恢复记录和 checksum 一致才算通过。

## 17. 测试与上线标准

### 17.1 测试层级

1. 锁定 Codex 原样提交并通过上游 Rust、App Server、Sandbox、Skill/MCP、Thread/Turn 和 Windows 基线测试。
2. 文本模型统一合同测试：火山 Responses 完成首条真实账号 E1/E2 合同，火山托管 GLM 作为独立 model route 使用同一 runner 并生成自己的 route evidence，但不计作 G4p；后续非火山国产 Agent 只在独立 G4p lane 被实例化时完成其 exact route 的 E1/E2，失败只让该 route 保持 disabled，不能继承火山结果或阻塞其他 child plan。
3. 火山首发非文本工具合同测试：Search route 在 E1/E2 后复用 Plan 03 research/evidence suite 取得 E3；Seedream、Seedance、TTS 分别完成真实账号、临时结果保全、usage/成本和错误语义 E1/E2，再由 Plans 09–10 取得媒体 E3/E4。不能把这些能力要求强加给纯文本第二 Agent route。
4. 最小真实 Mission 纵切：一句业务目标和现有素材直接形成可发布 ContentPackage；不以完成固定表单或固定 Agent 流程代替结果。
5. 五类冻结项目情景测试；直播公会必须按品牌/组织 IP 完成达人招募漏斗，不得答成老板个人 IP。
6. 冻结方案后才揭晓的跨行业 unseen 测试；五类项目不得进入生产 Prompt、Skill 示例、检索金答案、few-shot 或关键词补丁。
7. 运营、编导和目标行业从业者对普通通用 AI 对照版进行盲评。
8. 全程委托、自主预算、动态跳步/回退/并行和不必要打断率测试。
9. 媒体质量、连续性、局部重做、权利和最终导出测试。
10. 本地所有权、独立产品配置根、SQLCipher/Blob 加密、断网编辑、Windows 重启、Business Server/Runtime 崩溃和 side-by-side 更新测试。
11. 账本随机交易、能力幂等、供应商清单、最终账单和故障注入对账测试。
12. 本地明文副本扫描、恶意网页/Origin/CSRF/WS 测试、云端最小留存、SupportBundle、提示词注入、设备隔离、端到端加密备份与恢复测试。
13. 真实商家封闭发布和平台自营销测试。

### 17.2 营销质量

每个 Mission 生成普通通用 AI/Codex 对照版和本系统版，由至少两名有经验的运营/编导盲评：目标理解、观看欲望、兑现程度、主体与受众匹配、人物或组织情境、观点、表达一致性、可拍摄/可生成性、完整交付度、业务连接和标题党风险。

固定案例库和 unseen 中，本系统版本至少在 70% 的盲评配对中优于通用 AI 对照版；存在把虚构标记为真实、明显不可执行、个人/品牌主体误判或与目标业务行动断裂的结果直接判为失败，不得用其他维度高分抵消。

### 17.3 严重错误零容忍

数据、账本和不可逆外部动作始终适用本节；内容类条款只在系统把内容标记为真实/发布就绪、调用外部真人能力或真正执行外发时适用。明确标记的内部创意假设、风险草稿和低质量候选可以保存、查看和修改。

- 项目或商家数据泄漏。
- 越权访问。
- 篡改冻结预测或历史数据。
- 把虚构标记为真实或发布就绪。
- 未经授权把肖像、声音或儿童素材发送给外部供应商或用于发布。
- 把重大商品事实、收益、医疗或政策虚假主张标记为真实或发布就绪。
- 重复扣点、重复退款或账本不平。
- 系统失败却显示成功。
- AI 标识在最终导出中丢失。
- 云端持久数据库、日志或运营后台出现客户项目正文、Prompt、人物原件、可浏览媒体明文或备份解密密钥；第 15.3 节隔离进程中的瞬时处理和有期限加密暂存除外。
- 代理或平台后台能枚举、读取、修改客户本地 Project、Mission、Thread、Memory、Skill、文件或流事件。
- Codex Skill、MCP、Shell 或 Runtime 绕过 CapabilityOperation、hold 和 outbox 使用平台供应商密钥。
- RuntimeAttempt 或供应商成功直接触发正式接受、发布或重复扣点，而未形成相应业务版本和费用证据。
- 本地升级、迁移或应用回滚造成项目、版本谱系、素材或恢复密钥丢失。
- 代理管理系统或平台内部后台复制 IAM、Account、Ledger 或权限真相。
- 上游升级、进程恢复或网络重试造成重复用户扣点、重复有效供应商任务或未核清 hold 被错误释放。

### 17.4 工程验收基线

- localhost 非 AI 普通操作在目标 Windows 11 设备上 95% 两秒内完成；打开已有项目和基础编辑不依赖云端可用性。
- 远程长任务三秒内返回本地可查询的 operation ID 和预计状态，不阻塞页面。
- 固定测试集中所有关键结构化输出在有限重试后成功率达到 99% 以上。
- 对至少 10,000 组随机充值、划拨、冻结、结算、失败和退款序列进行账本不变量测试，差异为零。
- 重复、乱序回调测试中不得重复扣点或推进错误状态。
- 在本地事务前后、云端冻结前后、供应商接受前后、回调前后、结果保全后和下载中逐点杀进程；恢复后恰好一次用户结算，已完成产物不丢失。
- Windows 安装、首次启动、浏览器关闭、重启、断网、更新失败、磁盘不足和只读恢复均完成真实演练。
- customer build 不出现 OpenAI/ChatGPT 登录、环境 API key、用户 provider override 或读取既有 `~/.codex` 内容的路径；所有真实模型调用都能关联唯一 CloudCapabilityOperation。
- 每个进入 customer/default/advertised release 或自动切换集的 routeId 都有完整 `ProviderRouteIdentity`、各阶段 `ProviderRouteEvidenceRef` 和目标发布物的最终 `ProviderRouteQualificationBundle`；内部预算下为取得 E3/E4 而运行的 evidence canary 使用独立签名 `ProviderRouteCanaryAuthorization`，不能冒充客户生产资格。未通过业务质量门的后续非火山 G4p 路线保持 disabled，不能因 E1/E2 通过进入默认路由或自动切换。
- 产品根、全局 state/log、回收站和永久删除后执行明文与孤儿 Blob 扫描；客户正文只能存在于明确的加密项目存储、用户主动导出或任务级临时文件。
- 来自任意外部网页、另一本地用户、错误 Host/Origin、缺失 CSRF 或未认证 WebSocket 的调用都不能操作 Business Server；正常用户无额外登录步骤且业务工具能力不受损。
- `ai-ip-desktop`、`partner-management`、`internal-console` 分别构建、发布和回滚；N/N-1 API 测试保证任一云端后台升级不强迫桌面同步升级。
- 每个发布物记录 `upstream_sha`、`fork_sha`、Cargo/npm 锁摘要、安装包或镜像 digest、本地业务迁移版本和云端迁移版本。
- 上游同步后必须重新通过业务 Mission、Windows、本地迁移、能力网关、账本幂等、云端最小留存和独立发布测试。

### 17.5 封闭 Beta

首批招募至少四个不依赖五类冻结案例答案的真实客户项目，并运行一个平台自营销项目。测试周期至少六周，每个项目至少发布十二条内容，总量不少于六十条。平台自营销项目中，一级代理、二级代理和 C 端用户三个受众轨道各至少发布三条内容。每条内容保留当时使用的业务上下文、关键来源、制作版本、发布记录、默认与 Mission 自定义归因窗口的数据和复盘；不要求先完成固定入驻流程。

每个 Mission 在发布前登记主要结果、归因窗口、历史匹配基线或对照方法，以及最小有意义变化：关注型看传播、停留和涨粉；信任型看收藏、评论质量和主页访问；获客型看有效线索、转化与单位成本；品牌招募型看有效报名、加入、激活和持续行动。没有历史基线时使用匹配对照或预先登记的绝对漏斗阈值，不能伪造“提升”。进入公开 Beta 前，至少三个客户项目达到各自预注册的最小有意义变化，且没有通过虚构、权利伤害或不可持续投流换取结果；真实且有证据的争议表达不因“有争议”自动作废。辅助目标为用户接受主方案或仅作局部修改的比例达到 60%，最终 ContentPackage 无需结构性返工的比例达到 70%。

平台自营销项目不计入上述“三个客户项目达标”数量。一级代理、二级代理和 C 端用户三个受众轨道分别预注册目标、归因窗口、对照方法和最小有效行动阈值，分别统计曝光、主页访问、有效咨询、代理申请、注册或试用；冷启动使用绝对阈值，不宣称提升。封闭 Beta 期间每个轨道都必须达到自己的阈值，不能用单个偶然行动宣布通过。任一必需指标未达到时继续封闭测试，不以用户主观好评替代。

## 18. 分阶段建设

该系统不得写成一个单体实施计划，也不得先花数月把 SaaS、代理和云基础设施全部搭齐后才验证内容能力。各阶段围绕可真实使用的 Mission 纵切推进；会造成真实扣费、数据损坏或权利事故的底线必须先证明，其余工程能力与业务纵切并行演进。现有实施台账在全部重写前不可执行。

### 子项目 0：Codex 深度分叉与真实业务切片

来源锁与现有 Codex 执行底座已经保留；旧 0A/0B proof 依赖为 `RETIRED_NOT_PASSED`。下一步围绕一个真实营销任务，用现有 Mission、Lead 和 ContentPackage，完成“一句目标＋现有材料→可直接拍摄或发布的草稿、制作说明和待确认事实”，依据实际缺口改进业务能力。无需先恢复 paired evaluator、隔离评测 broker 或其他替代证明设施。

页面、线程、工具或账本能运行不能替代业务成果；机械清理与保留测试通过也不证明内容质量或真实模型可达。真实模型调用仍须有适用授权和预算，客户数据、发布和客户上线继续承担各自职责。后续产品化按具体业务需要推进，本次不新建功能或放宽正式客户启用条件。

### 子项目 1：一站式内容业务能力

完成一个 Lead、动态 MissionPlan、业务主体、InfluenceRelation、ActionFunnel、按缺口追问、研究、策略、选题、脚本、平台版本、ContentPackage 和本地项目记忆。接入首批业务 Skills，并建立普通通用 AI/Codex 对照、冻结案例和 unseen 盲评；固定三个策略、固定问卷和固定 Agent 编队不得进入实现。

### 子项目 2：最小商业控制面与可信创作点

交付账号、设备授权、能力目录、MissionSpendGrant、创作点账本、内部成本中心、价格版本、CapabilityOperation、供应商回执和对账；先服务真实业务纵切，再扩展一级/二级代理商业规则。所有客户 Prompt 和项目正文继续只在本地。

### 子项目 3：生成式媒体工作室

Seedream、Seedance、标准 TTS、故事板、镜头任务、可选低清预览、高清生成、临时结果保全、本地 FFmpeg 后期、字幕、AI 标识、媒体质检、局部重做和成片交付。

### 子项目 4：发布与学习闭环

自动发布前预测、人工发布记录、截图/表单数据回收、按 Mission 目标计算结果、评论洞察、复盘、项目记忆和下一轮 Mission。

### 子项目 5：代理运营后台

作为独立 `partner-management` 系统交付线下收款记录、额度划拨、价格权限、代理客户管理、异常退款、设备停用迁移和对账报表。它使用共享云端 IAM、Account 和 Ledger，但拥有独立域名、发布物、版本、部署与回滚节奏，不进入本地客户产品，也不能读取项目内容。

### 子项目 6：数字人 Beta 与公开上线

端到端加密备份与全新设备恢复、生产账号 PoC、肖像/声音授权、声音复刻、真人人像资产、传统数字人 Beta、签名更新、备案登记、合规审核、云端灾备和公开 Beta 上线闸门。

### 推荐第一条端到端切片

选择一个未进入五类冻结测试、未出现在 Prompt/Skill 示例中的真实商家：

```text
用户给出一个可衡量的影响力或业务目标并提供现有材料
→ Lead 创建 Mission 并只追问最关键缺口
→ 自主完成必要研究和与不确定度相称的探索；方向明确时不强制多方案
→ 交付一条可直接拍摄或发布的 ContentPackage
→ 在 Mission 预算内生成至少一个真实媒体产物并完成本地落盘
→ 用户人工发布
→ 回收至少一次真实数据并形成下一轮建议
```

验收重点是用户是否拿到愿意发布、无需结构性返工、与目标业务行动相连的产物，以及系统能否在崩溃或断网后继续；不验收用户是否按预设顺序点击了全部页面。

## 19. 已决定延后但不模糊的事项

- **准确模型 ID：** 在实施时从生产账号实时目录解析并锁定，经回归测试后写入模型注册表，不在规格中写死。
- **克隆数字人正式化：** 仅在第 9.4 节全部闸门通过后升级。
- **在线支付：** 首版不做；以后作为独立支付与结算子项目评审。
- **TikTok API 和跨境数据：** 首版不接；以后单独进行个人信息出境和平台 API 评审。
- **团队版：** 首版不做；未来必须单独设计权限和协作数据模型。
- **大规模平台爬虫：** 首版不做；研究只使用公开网络和用户提交材料。
- **BYOK 与纯离线模型：** 首版不开放；只有平台能力网关稳定后，再独立评估本地模型、用户自带密钥与新的商业模式。
- **多设备实时同步：** 首版只有端到端加密备份/恢复和显式项目分支，不做自动双向合并。
- **其他客户端：** Windows 10、Linux、移动端和原生 Rust GUI 不进入 v1.1；macOS 仅为邀请测试。内置 localhost Web UI 是正式产品界面，不另加 Electron。
- **C 端云端 SaaS：** 首版不做；未来若出现明确的跨设备或团队需求，必须证明不会把客户本地内容默认迁回云端。

## 20. 规格确认补充条款（2026-08-25）

本节记录初版规格完成后的实现边界。20.1 至 20.4 形成产品 v1.1 的方法与供应商护栏；20.5 固化前六轮审计中仍有效的经验。第 6、10、15、17、18、21、22 节形成架构 v1.4：以 Codex 本地 Business Server、Mission 全程委托和业务能力优先取代 v1.3 DeerFlow 云端 SaaS 假设。国产模型的 v1.5 已批准边界由 `2026-08-25-domestic-model-adaptation-design.md` 定义并覆盖相应 v1.4 provider 条款。后续计划、代码和验收报告若与已批准版本冲突，以最新已批准规格为准并提交显式变更记录。

### 20.1 所有行业共用的内容世界扩展

Lead 可以按任务需要使用以下六个内容世界视角，既不要求 BusinessContext 五项全部齐全，也不要求在生成选题前固定执行：

1. 对象分类与细分。
2. 历史与文化。
3. 地理与环境。
4. 生产与流通。
5. 人物、关系、选择与代价。
6. 受众、影响关系、目标行动与决策路径。

六个视角是可选研究镜头，不是必须凑齐的分类表，也不按分支数量机械选择内容根。扩展结果可以先作为待核验研究问题、检索方向或创意假设；地域趋势、历史故事、产业判断及人物经历在被当作真实商业主张或真实经历发布前必须取得可追溯来源，不能因模型表达肯定而升级为事实。

### 20.2 真实主体与不可逆动作确认

Lead 在创作过程中自动识别是否涉及真实可识别人物、严重指控、私有证据和必须恢复的关键真相，并把风险绑定到当前 TopicCard/Artifact 版本。缺少证据时仍可生成明确标记的内部草稿、匿名化方案、虚构重构或替代表达，营销叙事入口不能因此整体关闭。

只有当内容将使用受权利约束的真人原件调用外部供应商、向项目外发送敏感原件或真正执行发布时，才硬要求相应授权。用户在本地锁版、查看或导出草稿是可逆动作；系统记录事实/权利警告和 readiness，但不拒绝形成 ContentPackage。确认记录绑定具体 Artifact、Subject、用途和权利范围；内容变化到足以改变风险时生成新记录，精确重放返回原记录。

### 20.3 营销张力与可避免污名的固定回归

营销可以使用选择性叙事、观点先行、悬念和延迟揭示，但真正执行发布时最终留给观众的信念必须真实且范围准确。对于真实且可识别的人，系统不得为了开头张力先制造足以造成可避免污名的严重结论，再依靠结尾反转免责；本地锁版只记录显著警告和替代稿，实际外发/发布时再根据主体声明、私有证据和研究事实执行硬校验。模型即使谎报“无风险”也不能覆盖该边界。

固定回归案例是：“这个男人弄死了他养了十年的小狗”作为先行定罪，而完整事实是小狗患有绝症、持续痛苦，主人只能选择安乐处置以解除痛苦。若主人可识别，该结构必须阻断。只有去标识或明确披露为虚构/重构时才可采用延迟揭示，并且结束前必须完整恢复“绝症”“持续痛苦”“处置目的为解除痛苦”三项真相；少一项、只在说明区补充或让观众最终仍相信主人残忍，均不得发布。

### 20.4 真实供应商启用条件

子项目 0 可以使用平台内部预算完成火山 Responses 和媒体能力 PoC，但任何客户创作点环境在开启真实供应商前，必须完成本地 CapabilityOperation 预写、云端 hold/operation/audit/outbox 同事务、供应商请求清单、最终账单逐笔匹配、零调用证明和杀进程演练。每个新增国产模型路线都必须生成独立 `ProviderRouteIdentity`/routeId 和分阶段 `ProviderRouteEvidenceRef`，不能继承火山的可达、业务质量或结算结果；受个人或指定工具使用限制的 Coding Plan 额度不得进入产品网关。该条件只阻止未经计量的真实付费调用，不阻止使用 fake provider、录制回放或本地工具继续开发和评测业务能力；第二供应商 onboarding 失败也不阻止已通过门禁的火山业务纵切继续。

### 20.5 前六轮系统审计护栏

前六轮系统的完整证据、弃用原因和复用清单，以 docs/architecture/2026-08-25-legacy-six-version-decision-memory.md 为当前决策记忆。本补充不增加产品板块，而是约束实现方法：

- 五个商业视角是可由已有资料填充、允许显式未知的 BusinessContext，不是固定逐题问卷或业务就绪门；只追问当前最决定下一步的一项未知。
- 六视角扩展只能生成候选研究问题，不要求六类齐全，不按分支数量选择内容根，也不能成为硬编码的行业答案。
- 内容世界、命名人物或事件的证据发现、具体选题、表现形式和商业承接分层处理；商业回流不能反向决定内容根。
- 五个固定项目只用于回归和盲评，不能进入生产提示词、Skill 示例、检索上下文或关键词规则；泛化结论必须另有冻结 unseen。
- 新增永久默认的 Agent 编排、硬闸门或长期上下文前，必须先记录一个真实 held-out 失败、未改基线、最小候选、质量/事实/Token/时延/成本对照和删除路径。可回滚的 Skill、Prompt 和工具实验可以先进入隔离评测，不因缺少长期证据停止探索。
- Lead 可以决定选题、表达、工具、镜头、低成本试错和其他可逆业务动作；权限、版本完整性、哈希、重大方向确认、预算上限、调用次数、幂等、扣费、真人权利和供应商对账由代码执行。
- 能力状态分别标记为代码存在、机械合同通过、真实调用可达、业务盲评通过、真实发布复盘通过，低级状态不能替代高级状态。
- 当前系统永久不依赖或调用 map-marketing-content-world；相关旧材料只可作为历史证据。

## 21. Codex 整仓深度分叉与基础证明

### 21.1 来源锁定

- 官方上游：`https://github.com/openai/codex`。
- 首选锁定基线：`4ef1d4b89bd419c976b04fefa0fd36844e898340`，提交时间 2026-08-24。构建、测试和发布输入必须使用完整 SHA，禁止使用浮动 `main`、`latest` 或仅凭本机安装版本代替源码锁定。
- 采用方式是保留 Git 祖先关系的整仓深度 fork。`upstream` 只读指向 OpenAI 官方仓；当前本地决策仓未配置产品 `origin`，G0 必须记录 `productOriginStatus=unconfigured`。首次协作推送或发布前必须配置并验证产品 fork URL，此后 `origin` 才指向产品 fork。DeerFlow 不进入依赖图或回退链，第六版和其他旧实现只作为方法证据，不能整仓移植。
- 每个发布物记录 `upstream_sha`、`fork_sha`、Cargo/npm 锁摘要、Windows 安装包或镜像 digest、本地与云端迁移版本、默认 Skill 清单和 fork patch ledger 摘要。
- Codex 开源部分使用 Apache-2.0；必须保留 LICENSE/NOTICE、版权声明和修改说明。IDE 扩展与 Codex 云端产品不属于本次开源复用范围；新增 Rust、JavaScript、FFmpeg、字体、图标、Skills、模型与媒体资产另行生成 SBOM 和许可证清单。

### 21.2 Foundation Proof

G0–G2/Phase 0A 的 mandatory proof 完成前置已退役，状态为 `RETIRED_NOT_PASSED`。下表 G0–G2 保存原先的证明目标和判断标准，仅作历史，不再是内部开发许可门。来源锁、许可与准确报告测试的责任保留；真实客户能力的产品化/发布责任仍按适用范围承担，不能以清理冒充完成。

| 证明 | 最小证据 | 通过标准 |
|---|---|---|
| G0 来源、许可与锁定（历史：RETIRED_NOT_PASSED） | 校验完整 SHA、干净工作树、Cargo/npm 锁文件、Apache-2.0、源码依赖/许可/资产清单、NOTICE 和产品 origin 状态 | 来源可复现，源码阶段清单/NOTICE 完整，不依赖浮动分支；可分发物完整 SBOM 在 Plan 14 按具体发布物生成 |
| G1 Codex 原样基线（历史：RETIRED_NOT_PASSED） | 在目标 Windows 11 和开发 macOS 上运行上游 build/test、App Server、Thread 恢复、Skill/MCP、Sandbox 与基本工具 | 原样失败被记录并与产品改动区分；关键运行路径可重复 |
| G2 非编码业务可控性（历史：RETIRED_NOT_PASSED） | 使用最小 business instructions、结构化 Artifact 和临时 Skill 完成一个非五类固定案例的真实任务 | 能形成有业务价值的 ContentPackage，不退化成 coding assistant 或聊天答案 |
| G3 localhost 产品入口 | App Server 同进程提供静态 UI、同源 RPC/stream、上传下载；验证随机端口、bootstrap 会话、Host/Origin/CSRF/WS 和单实例 fencing | 不依赖 Electron/云端 C 网关，状态和流可恢复；恶意网页和旧进程无权操作 |
| G4a 首发国产模型内部能力兼容 | 以火山为首条目标，用隔离内部凭证经产品 provider seam 真实验证 Responses 流式、工具、多模态、取消、用量及 Search/Seedream/Seedance/TTS；客户设备令牌和客户点数保持关闭 | Plan 06 的 aggregate gate 只要求：文本 Agent routeId 独立取得 E1/E2，并在相同 product adapter/execution profile 上重跑完整冻结集加新 unseen 取得 E3；Search route 取得 E1/E2 并复用 Plan 03 research suite 取得 E3；Seedream、Seedance、TTS 分别取得 E1/E2。媒体 E3/E4 不属于 G4a，通过后由 Plans 09–10 独立取得。不得继承 evaluation-broker route 资格，只授权有预算上限的内部/邀请使用 |
| G4b 首发模型客户激活 | 在 G4a 之上验证客户设备令牌、客户点数、产品网关和无旁路配置 | customer build 无 OpenAI 登录/BYOK 旁路；交易、回执、对账、幂等和供应商合同测试同时通过 |
| G4p 跨 provider 可移植性 | 一个非火山实际 provider 的 `ProviderRouteIdentity` 使用相同 contract runner 独立完成机械和受预算约束的真实可达证明；火山托管 GLM 不计作本门 | 该 routeId 分别取得 E1/E2 refs 和独立 G4p `ProviderRouteEvidenceRef`；后者证明业务域无 provider 分支与限制 profile 完整，不能由 E1/E2 推导。route 默认 disabled，失败不阻塞 G2/G6/G4a 或首发火山链，低阶证据不能冒充生产质量资格 |
| G5 本地真相与迁移 | 产品专用根、SQLCipher、加密 Blob、rollout/StepReceipt、原子提交、断网、崩溃和 side-by-side 更新 | 无正文明文旁路，已保存产物不丢失；切换后新写入不被旧快照覆盖 |
| G6 业务纵切与盲评 | 从一句目标到可发布 ContentPackage；普通 AI/Codex 对照、冻结案例、unseen 和真人盲评 | 业务结果达到第 17 节标准，流程完成率不能代替内容质量 |
| G7 能力调用与账本 | fake/real provider 执行 `local operation/outbox → cloud hold/operation/audit/outbox → provider → receipt → settle` 并逐点杀进程 | 不重复扣点、不重复有效任务、不丢成本；不明费用进入对账 |
| G8 云端最小留存与备份 | 检查数据库/日志/TOS；全新设备使用恢复密钥还原项目 | 云端无项目正文与明文媒体长期库；平台人员无法解密备份；checksum 一致 |
| G9 分发与独立产品 | 签名 Windows 安装/更新/回滚；desktop、partner、internal 分别发布 | 本地项目可恢复，三个发布物不绑定版本，customer 内容不进入后台 |

### 21.3 裁决规则

- Codex 是已批准的底座，不因需要修改 core/app-server/protocol/state 就自动回退；“需要深改”本身不是失败。若某个原生机制妨碍业务能力，应先建立未改基线，再在 fork 中替换并用业务盲评、可靠性和成本证明收益。
- 旧 G0–G2 与依赖旧 proof/broker 的内部开发先后门禁已退役，不再要求完成评测设施才做业务。G4a 只证明首发模型内部兼容，不等于客户可用；外部 customer build 必须再过 G4b。G4p 是按 provider 绑定的非阻塞 onboarding proof：未通过时后续非火山 provider route 保持 disabled，不阻止已通过 G4a/G4b 的首发火山链；只有要启用、宣传或自动切换到该 route 时，才必须继续通过对应业务、发布、账本与客户激活门。G3–G5、G4b、G7–G9 在对应功能交付真实客户前成为产品化/发布门。任何工程门都不能阻止使用 fake provider、录制回放或经单独批准的隔离实验继续提高业务能力。
- 只有来源/许可不可接受、目标 Windows 无法运行、平台模型完全无法适配，或 Codex 在真实业务盲评中经多轮最小改造仍显著劣于可用替代方案时，才重新发起底座决策；不得静默回到 DeerFlow。
- 证明结果保存命令、日志、JUnit、Windows 环境、迁移前后校验和、故障注入记录、盲评原始表、供应商合同结果和对应阶段的来源证据：G0 保存源码依赖/许可/资产清单，Plan 14 对每个实际可分发物生成 SBOM。页面能打开、线程能聊天或单次漂亮样例不构成业务通过。

## 22. 爆改所有权、上游同步与回退

### 22.1 改动所有权

| 区域 | 策略 | 典型内容 |
|---|---|---|
| AI IP 产品核心 | 高差异，业务效果优先 | app-server 产品入口、业务协议、Mission/Artifact、Lead、动态计划、业务 Skills、本地状态、媒体、备份和业务 UI |
| Codex 通用能力 | 可以直接修改，但每处保留对照和回归 | Thread/Turn/Item、上下文装配、工具调度、子 Agent、模型 provider、审批和事件协议 |
| 优先吸收上游区 | 尽量保持可识别边界 | Sandbox、MCP/Skill 发现、通用执行、安全修复、平台兼容、性能和依赖更新 |
| 云端商业薄层 | 产品自有、不能反向拥有内容 | IAM、Device、Account、Ledger、Pricing、CapabilityOperation、Provider Gateway、partner/internal 和加密备份 |
| 禁止移植区 | 不进入新产品 | DeerFlow runtime/Gateway、旧巨型提示词、固定多 Agent 拓扑、固定内容根 gate、云端客户内容库、旧密钥和无许可证抓取代码 |

本项目接受与上游 Codex 产生较大差异；维护上游同步成本不能成为削弱业务能力的理由。但领域对象应放在独立 crate，通用执行改动应有测试和 patch ledger，避免所有逻辑堆入 `codex-core` 后无法理解。

### 22.2 上游同步策略

- 不自动追踪、合并或部署 `upstream/main`。每次升级在独立 `integration/upstream-<sha>` 分支完成。
- 同步前记录旧/新 SHA、Breaking Changes、安全与 Sandbox 修复、App Server 协议、state/migration、模型 provider、依赖变化和 patch 冲突；同步后更新 patch ledger。
- 保留 merge ancestry。由于产品是深度 fork，可以选择完整合并、范围清晰的带出处 cherry-pick 或重新实现；每次必须说明为何选择该方式，不能无记录地复制代码。
- 必跑上游原生测试、Windows 安装与更新、本地 Mission/Artifact、business protocol、state/migration、统一模型合同、火山首发能力、所有启用的第二 provider route、媒体落盘、账本黄金链路、云端最小留存、固定案例/unseen 盲评和三个发布物独立回滚。
- 每个同步必须人工审阅；不得自动合入产品主干或自动部署生产。已被上游覆盖或不再需要的 fork patch 必须删除。

### 22.3 运行期降级与回退

- 新上游同步失败时停留在当前已验收 SHA，不阻塞业务域开发。
- Business Server 严重故障时优先进入本地只读恢复，使用户仍能查看和导出项目；新的 Agent、研究或媒体调用可以暂停，但已有文件和版本不能被账号或云端开关锁死。
- 远程能力故障时，CapabilityOperation 和 hold 保持可审计状态并进入自动/人工对账；不得伪造失败、成功或零费用。本地可逆工作继续运行，任务进入 `WAITING_NETWORK` 或能力降级。
- 桌面更新在切换前可以回到“上一签名版本＋预迁移快照”；切换并产生新写入后不能自动回旧 schema，只能前向修复或只读导出。任何路径都禁止破坏性 downgrade。
- desktop、partner 和 internal 可以独立回滚；云端 IAM、Account 和 Ledger 权威不能随任一前端回滚成分叉状态。
- 锁定 Codex 提交的改造失败时保留失败证据并选择新的确切 Codex 提交或修正 fork；不得静默接回 DeerFlow 或第六版 runtime。

### 22.4 开源复用与不采用项

- 复用 Codex Apache-2.0 的 Thread/Turn/Item、App Server、Skills/MCP、Sandbox、工具、子 Agent、state 和通用执行能力；复用 PostgreSQL 事务、SQLx、RocketMQ 与 TOS 作为云端商业和供应商执行基础。
- 1.1 不引入 Formance Ledger 或 TigerBeetle。前者增加独立账本服务与部署边界，后者增加第二种金融数据库；两者都会破坏 `hold + capability_operation + audit + outbox` 同一 PostgreSQL 事务，当前规模也不需要其复杂度。
- 1.1 不引入 Celery 或 Dramatiq。它们以 Redis/RabbitMQ 等作为另一套 broker/任务语义，与已决定的 RocketMQ、CloudCapabilityOperation、lease 和 outbox 重叠。
- 1.1 不引入 Debezium Outbox Event Router。它适合 CDC/Kafka 体系；本项目使用窄 PostgreSQL outbox dispatcher 和 RocketMQ adapter，先保持一套交付语义。
- 被拒绝的开源方案不是永久禁用；只有出现真实规模、可靠性或运维证据，并能保持业务事务真相时才重新评审。

### 22.5 实施台账重写义务

本规格已于 2026-08-25 获用户复核，并使用 superpowers `writing-plans` 重写总台账、Phase 0A build specification 与首个 executable child plan。后续每个 child plan 仍必须逐项删除或替换：

- DeerFlow、LangGraph、FastAPI C 端 Gateway、云端客户 Project/PostgreSQL、Redis stream bridge 和三个云端 Web 产品的旧路径。
- Electron、外置 Codex sidecar、未改造的通用 App Server 套壳和第二套 C 端业务后端。
- 固定二十分钟问卷、固定三个策略、固定十三 Agent、固定线性状态机和黄金礼品首条纵切。
- 把 Thread/Turn/RuntimeAttempt 当作 Mission、正式内容、发布或付费真相。
- 客户素材默认进入 TOS、平台后台 break-glass 读取客户内容或代理能查看项目的任何路径。
- 先完成完整 SaaS/代理基础设施才开始验证内容业务能力的顺序。
- 任何没有 Codex upstream path、现有测试、保留/修改/替换决定、业务收益证明和删除路径的抽象层。

现役台账执行 2026-09-05 业务优先清理修订，随后围绕真实营销任务形成可用业务切片；旧 master §9 与所有旧 child 均不再可执行。继续保留锁定 Codex 的真实源码接缝、最小改动和删除/回退路径，不能把历史参考答案带入生产提示词或伪装成真实业务结果。

## 23. 官方依据索引

- [OpenAI Codex 官方仓库](https://github.com/openai/codex)
- [Codex Apache-2.0 License](https://github.com/openai/codex/blob/main/LICENSE)
- [OpenAI 开源组件说明](https://learn.chatgpt.com/docs/open-source)
- [Codex App Server 官方文档](https://learn.chatgpt.com/docs/app-server)
- [Codex SDK 官方文档](https://learn.chatgpt.com/docs/codex-sdk)
- [Codex as a platform](https://learn.chatgpt.com/blog/codex-as-a-platform)
- [智谱 GLM 长程 Agent 模型说明](https://docs.bigmodel.cn/cn/guide/models/text/glm-5.2)
- [Z.AI Chat Completion API](https://docs.z.ai/api-reference/llm/chat-completion)
- [GLM Coding Plan 使用限制](https://docs.z.ai/legal-agreement/subscription-terms)
- [阿里云百炼 Responses API](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-responses)
- [Apache RocketMQ Clients](https://github.com/apache/rocketmq-clients)
- [Formance Ledger](https://github.com/formancehq/ledger)
- [TigerBeetle](https://github.com/tigerbeetle/tigerbeetle)
- [Debezium Outbox Event Router](https://debezium.io/documentation/reference/stable/transformations/outbox-event-router.html)

- [火山方舟 Responses API 工具调用](https://www.volcengine.com/docs/82379/1958524?lang=zh)
- [火山方舟多模态理解](https://www.volcengine.com/docs/82379/1958521?lang=zh)
- [Seedream 图片生成](https://www.volcengine.com/docs/82379/1824121?lang=zh)
- [Seedance 视频任务 API](https://www.volcengine.com/docs/82379/1520757?lang=zh)
- [Seedance 真人肖像方案](https://www.volcengine.com/docs/82379/2608626?lang=zh)
- [豆包大模型语音合成](https://www.volcengine.com/docs/6561/1257543?lang=zh)
- [声音复刻用户协议](https://www.volcengine.com/docs/6561/1136414?lang=zh)
- [火山引擎 VKE](https://www.volcengine.com/docs/6460?lang=zh)
- [火山引擎 PostgreSQL 高可用](https://www.volcengine.com/docs/6438/79222)
- [火山引擎 RocketMQ](https://www.volcengine.com/docs/6410?lang=zh)
- [火山引擎 TOS 版本控制](https://www.volcengine.com/docs/6349/74830?lang=en)
- [人工智能生成合成内容标识办法](https://www.cac.gov.cn/2025-03/14/c_1743654684782215.htm)
- [生成式人工智能服务管理暂行办法](https://www.cac.gov.cn/2023-07/13/c_1690898327029107.htm)
- [互联网信息服务深度合成管理规定](https://www.cac.gov.cn/2022-12/11/c_1672221949354811.htm)
- [中华人民共和国个人信息保护法](https://www.cac.gov.cn/2021-08/20/c_1631050028355286.htm)
- [互联网广告管理办法](https://www.samr.gov.cn/zw/zfxxgk/fdzdgknr/fgs/art/2023/art_d93a579afd45413e8576e4623fab348f.html)
