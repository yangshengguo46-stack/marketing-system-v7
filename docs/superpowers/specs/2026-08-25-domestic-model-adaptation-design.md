# AI IP 1.1 国产模型适配设计

- **日期：** 2026-08-25
- **状态：** 架构 v1.5 已批准；用户已确认方案 B 的书面规格，允许进入 Phase 0A decision tip
- **批准标记：** `APPROVED_FOR_DECISION_TIP`
- **决策：** 单一国产首发模型真实跑通业务链，同时以统一能力合同证明第二国产路线可接入；不把多供应商同时上线设为首版业务证明前置条件
- **首条运营目标：** 火山方舟 / 豆包模型与媒体能力
- **优先 Agent 模型路线：** 火山方舟托管的 GLM 作为优先业务评测路线，通义作为原生 Responses 协议对照；最终实际 model/deployment/revision 仍由能力合同、商业授权和 AI IP 业务盲评决定，跨 provider G4p 另选非火山路线
- **当前可用接入事实：** 用户确认现有火山方舟凭证已包含 GLM 模型访问范围；实现只记录 `provider=Volcengine Ark` 与实际 GLM model/deployment/revision，不记录、复制或提交 API key

本文件是 `2026-08-24-ip-agent-saas-design.md` 的 provider 专项设计输入。国产模型方向继续保留；2026-09-05 已批准的 [business-first cleanup](2026-09-05-business-first-cleanup-design.md) 覆盖本文中的 Phase 0A/proof broker 开发前置要求。这些要求与旧 child 均为 `RETIRED_NOT_PASSED`，不再可执行；旧实施顺序和完成清单仅作历史。本文不授权付费调用，也不把评测退役解释为模型资格、客户激活或客户上线通过。

## 1. 为什么不是“换一个 Base URL”

锁定的 Codex 源码把模型回合建立在 Responses 语义上。请求不仅包含文本，还可能包含工具定义、结构化输出控制、reasoning、stream、include、prompt cache 和 client metadata；流式解析还依赖文本与工具 delta、完成/失败状态和 usage。`ModelProviderInfo` 可以配置名称、地址和认证，但通用 provider 的默认能力声明不能证明真实供应商支持这些语义。

因此，以下结果都不算国产模型适配完成：

- endpoint 返回 HTTP 200；
- 单次聊天能输出中文；
- 只替换 `base_url`、API key 或模型名称；
- 把 provider 伪装成 `OpenAI` 后绕过能力判断；
- 文本流可读，但工具回合、结构化产物、取消、usage 或错误语义失真；
- 机械合同通过，却没有通过相同 Mission 的业务质量回归。

适配的对象是 AI IP 业务需要的能力合同，而不是某个厂商的品牌名称。

## 2. 采用的方案 B

首版按三条不互相阻塞的证据线推进：

1. **首发业务线：** 火山方舟作为第一条真实运营目标，完成平台网关、真实模型调用、冻结加新 unseen 业务回归和内部预算证明。业务盲评先于第二供应商工程。
2. **Agent 模型线：** 现有火山方舟凭证内的 GLM 按 `provider=Volcengine Ark` 建立独立 model route，优先参加 Agent 业务盲评；它能证明同一 provider 下不把业务写死到豆包模型，但不能冒充跨 provider 的 G4p。
3. **跨 provider 可移植性线：** 后续选择一个非火山实际 provider 使用同一产品 seam 完成 G4p，并用通义原生 Responses 路径作为协议对照。该线不自动取得生产路由或故障切换资格，也不阻塞火山托管 GLM 的业务评测与首发链。

方案 B 明确拒绝两个极端：既不只写一个空扩展接口，也不要求火山、通义、DeepSeek 等多家在业务证明前同时完整上线。

## 3. 架构边界

```text
Mission / Lead / Artifact / Business Skill
        │ 只提交 ModelCapabilityRequest
        ▼
codex-ai-ip-runtime
        │ capability key + quality tier + budget/latency/modality constraints
        ▼
codex-ai-ip-provider
        │ signed capability catalog + CapabilityOperation
        ▼
平台模型能力网关（规范化 Responses 子集）
        ├─ 火山方舟原生/近原生 Responses adapter
        ├─ 第二国产供应商 adapter
        └─ fake / recorded conformance adapter（测试专用）
```

### 3.1 业务层只认能力

Mission、Lead、Artifact、ContentPackage 和业务 Skills 只能引用：

- `ModelCapabilityKey`：需要完成的能力；
- `CapabilityContractVersion`：业务请求所依赖的规范化能力语义；
- `ModelQualityTier`：当前任务允许的质量档位；
- `RequiredFeatures`：输入输出模态、上下文、时延、预算和其他必需特征约束。

这些对象不得包含 `if provider == ...`、具体模型 ID、deployment ID、供应商 endpoint、供应商专属响应字段或易变的 route/evidence eligibility。具体 provider、模型、不可变 revision 和 E0–E4 状态只属于执行 provenance、能力注册表、router、ProviderAttempt、成本与签名回执。

统一合同不是取所有模型能力的最低交集。更强模型的长上下文、多模态、推理或特定工具能力可以作为独立 capability profile 暴露；不支持的能力必须显式为 unavailable，不能伪造成功或静默降级。

### 3.2 Codex 仓内唯一接缝

客户构建仍直接改造 Codex 仓内的真实模型链路：

- `codex-ai-ip-runtime` 负责从业务任务形成能力请求；
- `codex-ai-ip-provider` 负责能力目录、预算、幂等、回执和规范化模型调用；
- `codex-model-provider` / `codex-api` 继续承担 harness 所需的模型 transport，但其生产能力声明来自显式 `ProviderCapabilityProfile`，不能继承通用 provider 的乐观默认值；
- 平台能力网关只负责远程能力、供应商适配、调用真相和结算，不成为第二套 Mission/Artifact 业务后端。

Phase 0A 的本地 proof broker 是一次性评测设施，`publish = false`，只证明 G2 的隔离、公平、用量和成本。它不得演化为客户 provider、商业网关或第二套运行时。

### 3.3 两类供应商 adapter

1. **原生 Responses adapter：** 供应商真实提供所需 Responses 语义时，逐字段验证并透传已证明的子集。
2. **协议翻译 adapter：** 供应商只提供 Chat Completions 或其他原生协议时，由云端能力网关翻译为产品冻结的 Responses 子集。翻译器必须保存工具调用、顺序、结束状态、取消、usage、请求 ID、实际 revision 和错误语义；无法等价表达的能力标记为 unsupported。

协议翻译不进入 Lead、业务 Skills 或 Artifact schema，也不能因为 compatibility name 使用 `OpenAI` 就把供应商标成 OpenAI 或宣称完整 Responses 兼容。

## 4. 冻结的模型能力合同

首版模型 adapter 按 capability profile 声明并验证以下字段。每个字段都有 `supported`、`unsupported` 或带限制的 typed value，不允许缺省为支持：

| 合同面 | 最小证明 |
|---|---|
| 文本输入与流式输出 | 有序增量、最终文本、完成/失败状态一致，可从流重建最终结果 |
| 结构化 Artifact | JSON Schema 或等价严格约束的成功、拒绝和解析失败语义可区分 |
| 工具调用 | 函数/自定义工具定义、参数 delta、tool call ID、结果回注和多轮闭环 |
| 多模态 | 只对真实验证过的图片/文件输入类型声明支持；不因模型名称推断 |
| 终止与取消 | 本地断流、上游取消是否成功、取消后的供应商计费分别记录 |
| Usage | 输入、输出、缓存、reasoning 等供应商可提供的用量字段规范化；空 usage 不能进入付费生产 |
| 模型身份 | 实际 provider、model、deployment/revision；浮动 alias 不能单独成为生产证据 |
| 错误与重试 | 认证、限流、超时、内容拒绝、能力不支持、供应商失败、submission unknown 分开映射 |
| 数据与地域 | 处理地域、留存与删除合同进入签名能力目录，不由模型输出决定 |
| 成本 | 价格版本、预算 hold、ProviderAttempt、供应商账单和最终回执可对账 |

能力合同必须使用真实请求/流 fixture、恶意边界 fixture 和受批准的 live contract run。对一个字段没有证据时，默认状态是 unavailable，而不是“兼容”。

稳定的 `ProviderRouteIdentity` 是一个所有 key 必须出现的 typed object：`capabilityKey`、`capabilityContractVersion`、`qualityTier`、`qualityProfileVersion`、`provider`、`model`、`deployment`、`immutableRevision`、`adapterVersion`、`executionProfileVersion`、`processingRegion`。字符串使用非空 NFC 规范值；只有 `deployment` 可以为 JSON `null`，且仅表示供应商协议确实没有 deployment 维度，未知、缺字段和空字符串一律不同且不能用 null 掩盖。`processingRegion` 必须是已取得合同证据的非空规范值，未解决时不能取得 E2 或更高资格。对象按 RFC 8785 JCS 编码后取 SHA-256，小写 64 位十六进制摘要成为路径安全的 `routeId`；不得拼接带 `/`、`+` 或空值歧义的字符串作 ID。任一 identity 字段变化都产生新 routeId。

代码来源不进入稳定路线身份。每个 E0–E4、G4p、G4b、G7 和 release 阶段分别生成 `ProviderRouteEvidenceRef`，至少绑定 `routeId`、stage、`testedForkSha`、`stageSubjectClosureSha256`、`stageDependencyClosureSha256`、runner/suite version 与 digest、原始 evidence digest 和时间。closure 按阶段定义：E0–E2 覆盖 provider runtime、adapter、gateway 与 contract runner；G4p 另覆盖业务域无 provider 分支检查和签名限制 profile；E3 覆盖实际 Business Runtime、Lead instructions、生产 Skills、Prompt/context/schema 与盲评 suite；G4b/G7 覆盖设备令牌、CapabilityOperation、账本、回执和对账；E4 覆盖真实发布、数据回收和 Retro；release 覆盖目标包、配置、迁移与回滚。后续阶段位于另一 fork SHA 时，verifier 必须证明它所引用的 stage-specific subject/dependency closure 未漂移，或显式重跑受影响阶段；不得把整仓无关变化强迫成业务重测，也不得静默继承已变化的实现证据。route identity、evidence ref 和状态只存在于 registry、router、attempt、receipt 与 qualification record/bundle，不写回 Mission、Artifact 或业务 Skill。

## 5. 路由与业务优先规则

Lead 先根据 Mission 选择能力和质量档位，平台再在已通过相应证据等级的 profile 中路由。价格、时延和可用性可以参与路由，但不能覆盖业务质量门。

自动切换只在以下条件同时满足时允许：

1. 候选 `routeId` 的 profile 覆盖本次请求的全部必需能力；
2. 通过同版本机械合同；
3. 对当前 `QualityProfileVersion` 通过业务质量回归；
4. 数据地域、权利、预算和留存范围不扩大；
5. 实际 provider/model/revision 写入回执并向用户可追溯。

自动切换还必须在 CapabilityOperation 创建时冻结 route policy 和候选集；`submission_unknown`、已取得供应商任务 ID、已经交付部分流、内容安全拒绝、权利限制、无效 Schema、unsupported feature、上下文超限或预算不足都不得跨 provider 自动重试。平台容灾产生的额外供应商尝试不能突破用户已授权 hold；每个 ProviderAttempt 独立保留实际 provider/model/revision、触发原因、usage、成本和回执。

否则返回 typed `CapabilityUnavailable` 或 `ReplanRequired`，由 Lead 选择等待、换能力、降低成本或请求重大方向确认。不得为了高可用把业务质量静默降级。

首版不提供用户模型选择器，不承诺跨供应商自动故障切换，也不把第二供应商纳入默认生产路由。E3 只允许有界内部/邀请 canary；成为客户默认或自动切换候选还需要对应 E4、G4b/G7 与发布门，不能继承火山证据。

## 6. 凭证与客户构建

- 客户构建不开放 BYOK、OpenAI/ChatGPT OAuth、环境 API key、`auth.json`、用户 `model_providers` 或本地 endpoint/provider override。
- customer build 只用 DeviceRegistration 刷新短期设备令牌访问平台能力网关；供应商密钥只存在云端隔离适配进程。
- 开发旁路只允许存在于不可发布的 internal build，并在构建测试中证明 customer artifact 不包含或不可达该路径。
- 本地 Skill、MCP、Shell 和浏览器不能获得供应商凭证；它们只能创建受预算与幂等约束的 CapabilityOperation。
- GLM 的个人 Coding Plan 或仅允许指定工具使用的订阅额度不得用于产品 API、代理调用、聚合或转售。GLM adapter 必须使用允许产品集成的开放平台 API 或单独企业合同；若通过其他云平台托管的 GLM Responses 服务接入，证据中的实际 provider 必须记录该云平台，不能只写模型品牌。

这条边界不限制 Lead 的业务工具能力，只限制供应商凭证和付费调用真相的旁路。

## 7. 证据等级与门禁

模型能力继续使用五个互不替代的状态：

| 等级 | 含义 | 不能宣称 |
|---|---|---|
| E0 `code-present` | adapter 或路径存在 | 不能宣称合同通过 |
| E1 `mechanically-conformant` | fake/recorded/contract fixture 通过 | 不能宣称真实 provider 可达 |
| E2 `live-provider-reachable` | 受预算约束的真实调用通过并有 usage/revision/回执 | 不能宣称业务质量成立 |
| E3 `business-regression-passed` | 冻结集加新 unseen 真人盲评无质量回退 | 不能宣称真实传播效果 |
| E4 `publication-retro-passed` | 真实发布、数据回收和复盘通过 | 不能承诺单条必爆 |

### 7.1 首发火山链路

- Plan 01/G2 可以在 `targetVolcengine` 或获批准的 reference provider 上证明业务方法；reference 结果不能冒充首发链路。
- Plan 03 必须让指定首发火山模型经 isolated evaluation broker 完成完整冻结集和新 unseen 业务门，形成 `LaunchModelBusinessBaseline`；它证明模型/方法值得进入产品接缝，但不是某个 product routeId 的 E3。
- Plan 06/G4a 经产品 provider seam 取得 E1、E2，并在该火山 routeId 上复用 Plan 03 runner 重跑完整获授权回归加新 unseen，独立取得产品路线 E3；Plan 03 的 `LaunchModelBusinessBaseline` 只是方法基线，不能被 product adapter/execution profile 继承。
- G4b 再证明客户设备令牌、创作点、回执、对账、幂等和 customer build 无旁路；G4a 不能代替 G4b。

G4a 是一个 aggregate gate，不是假装所有火山能力属于同一 routeId。它的 Plan 06 最低通过线到此为止：文本 Agent route 独立取得 E1–E3；Search route 取得 E1/E2，并复用 Plan 03 research/evidence suite 取得 E3；Seedream、Seedance、TTS 按各自 `capabilityKey` 拥有独立 routeId，只取得 E1/E2。媒体 E3/E4 不是 G4a entry/exit 依赖：Plan 09 后续对每条拟资格化媒体 route 运行冻结媒体集加新 unseen 的真人盲评/QC 并签发 E3 ref；Plan 10 只为真实发布实际使用的各 route 产生 E4。Plan 06 的 provider contract owner 同时保存 `VolcengineG4aRecord` 并持续作为这些首发 route 的 qualification owner；Plan 08、09、10 追加各阶段 EvidenceRef 后，由该 owner 校验 stage closure 并签发每条拟 customer/default/advertised route 的 `ProviderRoutePreReleaseRecord`，Plan 14 再签发最终 bundle。为取得 E3/E4，内部预算下的证据收集和邀请 canary 不需要先拥有 pre-release/final bundle，而是使用签名 `ProviderRouteCanaryAuthorization`，精确绑定 routeId、目标阶段、允许用户/构建、预算、时限、地域和权利范围；它不能授权客户生产路由。

### 7.2 第二国产供应商 route-scoped 可移植性门 G4p

G4p 的目标是证明产品接缝可移植，不是宣布第二供应商已可自动接管生产流量。它必须：

1. 使用与火山 adapter 相同的 capability contract 和测试 runner；
2. 通过 E1，且至少对首版必需的文本流、结构化 Artifact、工具闭环、usage、revision、取消与错误映射完成一次受批准的 E2 live contract run；
3. 证明 Mission、Lead、Artifact 和业务 Skill 无供应商分支；
4. 将不支持的能力与限制写入签名 profile；
5. 以稳定 `routeId` 和本阶段 `ProviderRouteEvidenceRef` 保存完整 identity、tested fork/closure/runner digests、请求计数、usage、费用、已知限制和回退方法；
6. 不使用第二供应商的 E1/E2 结果冒充 E3；
7. 单独签发 G4p `ProviderRouteEvidenceRef`，证明业务域无 provider 分支与限制 profile 完整；G4p 不能从 E1/E2 自动推导。

G4p 不阻塞 Plan 01、Plan 03、G4a、G4b、首发火山 customer release、内部/邀请测试、真实媒体纵切或发布学习闭环；失败只让该后续非火山 provider route 保持 disabled。只有发布物要启用或宣传该非火山 provider/model，或把它加入自动切换候选集时，G4p 及对应更高业务、发布、账本与客户激活证据才成为该 route 的门禁。

### 7.3 后续非火山 G4p route 的单一资格 owner

Plan 06 的 provider 合同 owner 只负责在真实接缝上生成可重复的 `provider-onboarding-e0-e2/<route-id>` child-plan 模板并完成 G4p/E0–E2；它不能预写尚不存在的 Plan 08/10/14 runner。只有产品决定继续资格化某条后续非火山 G4p route，且所需 runner 已由前序计划真实建立后，同一个 qualification owner 才使用 `superpowers:writing-plans` 为该 routeId 编写非阻塞 `provider-qualification/<route-id>` child plan，并汇总：

1. 复用 Plan 03 的冻结加新 unseen 盲评 runner，独立取得该 route 的 E3；
2. 复用 Plan 08 的 customer token、hold、ProviderAttempt、回执、对账和故障注入 runner，独立取得该 route 的 G4b/G7；
3. 复用 Plan 10 的真实发布与复盘合同，独立取得 E4；
4. 上述阶段各自的 `ProviderRouteEvidenceRef` 与 closure-drift 判定。

资格子计划先输出签名 `ProviderRoutePreReleaseRecord`，以同一稳定 routeId 引用 E0–E4、G4p、G4b/G7 各阶段原始证据，禁止复制火山状态或把“同一模型品牌、另一托管 provider”视为同一路线。Plan 14 消费该 pre-release record，在目标发布物上复核 route disabled-by-default、构建内容和回滚证据，成功后才签发最终 `ProviderRouteQualificationBundle`。Plan 11 的内部模型注册表可以展示 canary authorization、阶段 refs 和 pre-release 状态；只有最终 bundle 才能授权 customer/default/advertised route 或自动切换，后台不能制造或升级证据。为了取得 E3/E4，受签名 `ProviderRouteCanaryAuthorization` 约束的内部/邀请 evidence canary 可以在 pre-release/final bundle 前运行；它不能被宣传成客户生产资格。所需 runner 尚未存在、资格失败或暂不执行时，仅该 route 保持 disabled，不增加火山主线的 entry gate。

## 8. 实施顺序

1. Phase 0A.1 先保存包含本设计的决策祖先，不修改 Codex runtime。
2. Plan 01 继续完成来源、上游基线和最小业务证明；不在 proof broker 中建设多供应商产品网关。
3. Plans 02–03 先深化 Mission/Lead/Artifact 和内容质量。
4. Plan 06 建立统一 `ProviderCapabilityProfile`、明确能力默认拒绝、平台网关 contract runner，并完成火山 G4a。
5. 不阻塞 Plans 07–10 或首发火山发布的前提下，为火山托管 GLM 增加独立 model-route 证据；它不计作 G4p。后续再选择非火山 provider 完成 G4p，并用通义 Responses 路径校验原生协议 profile。
6. 只有要在某个发布物中启用、宣传或自动切换到后续非火山 G4p route 时，该发布物才同时要求 G4p、对应 G4b/G7 和更高业务/发布证据。

## 9. 明确延后

- 跨 provider G4p 的实际 provider、模型 ID、endpoint、region 和 deployment/revision；火山托管 GLM 只冻结为优先 Agent 评测路线，不代表第二 provider 已落定；
- 三家以上同时上线；
- 用户自选模型、BYOK 或用户 endpoint；
- 跨供应商自动故障切换；
- 基于价格的动态路由 UI；
- 对第二供应商的完整 E3/E4 业务资格；
- 纯离线国产模型推理和本地 GPU 调度。

延后不代表永远不做。新增供应商沿同一能力合同和证据等级进入，不得修改业务对象来迎合某个厂商。

## 10. 书面验收清单

本设计只有在以下项目同时成立时才可标记为用户批准并进入来源冻结：

- [x] 用户确认方案 B 的书面边界与 G4p 时序；
- [x] 主规格明确火山是首条运营目标，业务层是 provider-neutral；
- [x] 总台账把第二供应商可移植性证明放在业务门之后的非阻塞 onboarding lane，且只在启用或宣传该 route 前成为门禁；
- [x] Phase 0A 规格明确 proof broker 不是生产多供应商网关；
- [x] Phase 0A.1 provenance plan 把本文件列为 required decision input；
- [x] 所有文档继续禁止 customer BYOK 和 provider override；
- [x] 不把第二供应商适配变成 Plan 01/03 业务证明的阻塞项；
- [x] 不把 E1/E2 宣称成 E3/E4。
- [x] 稳定 routeId、阶段 EvidenceRef、pre-release record 与 Plan 14 最终 bundle 不发生 fork-SHA 或门禁自循环。

## 11. 锁定源码依据

本设计基于 OpenAI Codex `4ef1d4b89bd419c976b04fefa0fd36844e898340` 的以下真实接缝：

- `codex-rs/model-provider-info/src/lib.rs`：`WireApi`、provider 地址/认证/传输配置；
- `codex-rs/model-provider/src/provider.rs`：`ModelProvider` 和能力声明；
- `codex-rs/model-provider/src/models_endpoint.rs`：模型目录与认证路径；
- `codex-rs/core/src/client.rs`：Responses 请求装配；
- `codex-rs/codex-api/src/common.rs`：Responses 请求类型；
- `codex-rs/codex-api/src/sse/responses.rs`：流式事件、工具 delta、终态和 usage 解析。

实现 child plan 必须先为这些 seam 写 characterization test，再以 TDD 增加 AI IP 的显式能力 profile；不得凭本文猜测未来上游文件仍然相同。

## 12. 当前官方供应商依据

这些链接只支持“优先评估”的顺序，不冻结未来模型 revision。实际 onboarding 前必须重新获取官方 API、价格、地域、留存与合同快照：

- [智谱 GLM 长程 Agent 模型说明](https://docs.bigmodel.cn/cn/guide/models/text/glm-5.2)：当前旗舰系列强调长任务、Function Calling、流式输出和结构化输出；这使 GLM 值得优先进入 AI IP Agent 盲评，但供应商自报基准不能代替本产品业务证据。
- [Z.AI Chat Completion API](https://docs.z.ai/api-reference/llm/chat-completion)：直连接口公开工具、工具流、reasoning、JSON 输出和模型身份字段；它不是 Codex Responses 完整等价证明。
- [阿里云百炼 Responses API](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-responses)：提供 Responses 形状、函数调用和结构化事件，可作为原生协议 profile 的实现对照；接入第三方模型时实际 provider 仍是对应托管平台。
- [GLM Coding Plan 使用条款](https://docs.z.ai/legal-agreement/subscription-terms)：个人或指定工具套餐默认不允许用于自有应用、SaaS、代理或转售；正式产品必须使用获授权的开放 API 或书面企业合同。
