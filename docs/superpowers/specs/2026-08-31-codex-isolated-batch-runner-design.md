# AI IP 营销评测实验室 07B：Codex 隔离批量运行器设计

> 日期：2026-08-31  
> 状态：设计已由产品负责人确认；实现与独立复审进行中
> 子阶段：Marketing Evaluation Lab 07B / Codex Batch Runner  
> 上游依赖：07A Provider-free Lab Foundation、06A/06B1 Native App Server Proof Kernel  
> 直接后继：07C Human Review Workbench、07D Calibrated Pilot + Promotion Bridge

## 1. 决策摘要

07B 建设一条可重复、可审计、默认不付费的候选作品生产线：把同一份真实业务题分别交给一个原版 Codex Agent 和一个爆改版 Codex Agent，在两个完全隔离的 Codex App Server 环境中独立完成，再将作品匿名成 A/B，导入 07A 私有盲评体系。

本阶段的核心不是“再造一个评委”，而是确保评委以后看到的两份作品确实来自公平、独立、未串线的同题运行。

正式决策如下：

1. 主验证采用“原版 Codex vs 爆改版 Codex”的完全隔离同题盲跑。
2. 普通 Codex 协作子 Agent 共享宿主工作区，不满足强隔离要求，不能充当正式候选运行器。
3. 每个候选独占 App Server 进程、`CODEX_HOME`、线程、工作区、缓存、临时目录和日志目录。
4. 自有 Python 薄控制器掌握计划、隔离、证据、匿名化和 07A 导入；锁定版 Promptfoo 仅作为开发期 App Server 执行适配器。
5. Promptfoo 的评分、断言、AI Judge 和数据库均无正式裁决权。
6. 同版本能力开关对照不进入默认主流程，只在爆改版失利、波动异常或需要定位贡献来源时作为诊断实验启动。
7. 07B 的机械验收完全 provider-free；全部通过后，才可凭一次性授权执行一组有硬预算上限的火山 GLM 真实冒烟。
8. 真实 GLM 冒烟只证明运行链可用并产生首批真实作品，不提前宣称爆改版营销能力胜出。
9. 最终晋级与生产证明仍由 06A/06B1 原生 App Server 强证据链和后续 07C/07D 负责。
10. 07B 信任本机操作系统和当前登录用户管理的宿主环境；它防候选 Agent、候选子进程和不可信评测输入作弊，不承担“同一登录用户下已有恶意程序”或宿主已失陷后的系统级防御职责。

## 2. 规格优先级

发生冲突时，07B 按以下顺序解释：

1. 本规格对 07B 的明确规定；
2. `2026-08-31-marketing-capability-evaluation-lab-design.md`；
3. `2026-08-25-phase-0a-codex-business-proof-build-spec.md`；
4. `2026-08-25-legacy-six-version-decision-memory.md`；
5. `codex-fork-patch-ledger.md`；
6. 其他更早的规划文档。

本规格不能覆盖以下更高层不变量：一个 Lead 对业务结果负责、固定五案例仅用于评测、证据分层、确定性边界、业务优先、App Server 单底座、GLM 生产 API 凭据与个人 Coding Plan 分离。

## 3. 为什么现在必须做 07B

07A 已能编译案例、管理私有根、准备匿名评审包、封存批次并执行解锁决策，但它故意不调用模型。缺少 07B 时，候选作品只能手工放入实验室，无法回答：

- 两份作品是否真的收到完全相同的题目和素材；
- 是否使用相同模型、预算、权限和初始工作区；
- 原版与爆改版是否串用了 Home、缓存、线程或历史上下文；
- 是否有人在看到失败后悄悄重跑；
- 输出、轨迹和用量是否被替换；
- 匿名映射是否在交给评委前泄漏。

前六版的主要失败并不是“没有更多提示词或 Skill”，而是大量结构与测试无法证明真实业务结果更好。07B 只补最小、可删除的证据缺口，不再先造大而全平台。

## 4. 目标与非目标

### 4.1 目标

- 从同一个不可变案例包生成原版/爆改版成对运行计划。
- 在机器可验证的隔离条件下启动两个独立 Codex App Server 候选 Agent。
- 对运行条件做严格同等性预检，差异只能来自预先声明的 treatment。
- 收集输出、轨迹摘要、工具使用、时间、Token、费用和错误证据。
- 以现有 07A 规范进行规范化、哈希、匿名化和私有导入。
- 对超时、崩溃、格式错误、预算超限、串线和篡改执行失败关闭。
- 支持本地假模型端到端测试以及经单次授权的 GLM 冒烟。
- 保持案例、行业、内容形态和未来业务能力可扩展，不把产品锁死在“黄金礼品”上。

### 4.2 非目标

- 不建设真人评审 UI 或评委运营后台；这是 07C。
- 不在本阶段蒸馏营销专家、抖音操盘手或教师人格。
- 不自动宣布胜负、晋级、发布或回写生产配置。
- 不建设知识库、SaaS 租户后台或代理管理系统。
- 不把 Promptfoo 变成长期业务状态、正式评分器或晋级权威。
- 不将 07B 塞入 Codex 产品主路径或继续膨胀现有超大 Rust runner 文件。
- 不使用固定五案例作为 G2 新真实业务案例；07B 初期案例只用于回归和校准。
- 不在没有一次性授权和硬预算时调用任何付费模型。
- 不替代操作系统、终端安全、杀毒或云隔离，不为防御已经取得当前登录用户同等权限的恶意软件而建设专用服务器、虚拟化平台或自有网络基础设施。

### 4.3 信任边界与本机威胁模型

07B 是本地商业产品的评测壳，不是通用恶意软件隔离器。正式证据链必须防御以下不可信输入和执行面：

- Stock/Modified 候选 Agent 及其工具调用、Shell 命令和子进程；
- 案例工作区、模型输出、Promptfoo 输出、轨迹、缓存和下载内容；
- 未登记网络目标、真实凭据泄漏、预算越界、跨臂串线和结果替换；
- 候选在其获准工作区、Home、临时目录和缓存目录内对文件的任意修改。

以下属于宿主前置条件，不属于 07B 自己重新实现的防御范围：

- macOS/Linux 内核、文件系统和当前登录账户的基本完整性；
- 产品负责人管理的同一登录用户下不存在已取得等价权限的恶意并发程序；
- 管理员/root、内核攻击者、宿主恶意软件和物理设备攻击不在本阶段威胁模型内。

因此，Promptfoo 运行闭包仍须放在 Task 4 控制器拥有、候选不可写且不向候选暴露路径的私有运行目录中；启动前与运行结束后必须对实际解包树做完整内容、身份、权限和链接状态核验，任何漂移都使 pair 失败关闭。这里的目标是阻止候选 Agent 篡改或伪造执行证据，并发现宿主异常；不是在同一已失陷用户账户内部再造一个操作系统安全边界。

若发现宿主同用户进程、管理员操作或系统异常改变了私有运行树，当前 batch 标记为宿主完整性失败并在干净宿主重跑，不能把该结果用于盲评。07B 不以此为由降级候选隔离、密钥隔离、网络限制、预算限制或证据核验。

## 5. 核心术语

### 5.1 Candidate Agent

一次候选运行是一个新 App Server 进程中的新线程，拥有独立 Home 和工作区，只接收本轮案例材料及声明允许的业务能力。它不是当前开发线程里的协作子 Agent。

### 5.2 Stock Arm

由登记表冻结的上游原版 Codex 二进制。登记必须包含上游提交、目标平台、构建 profile、工具链版本、构建命令摘要和二进制 SHA-256。

### 5.3 Modified Arm

由登记表冻结的当前爆改版 Codex 二进制。登记信息与 Stock Arm 同级完整，并额外记录 fork 提交和被测能力清单。

### 5.4 Treatment

主实验的 treatment 是“用户实际拿到的原版产品能力”与“用户实际拿到的爆改产品能力”的声明差异，包括对应二进制，以及随产品交付的内建 Skill、系统指令、配置和能力清单。这些差异必须全部写进两个不可变 `TreatmentManifest`，不能在运行时临时加料。题目、外部材料、模型路线、预算、外部权限和业务工作区模板不是 treatment，必须一致。

### 5.5 Diagnostic Run

在主实验异常后才启动的同二进制能力开关实验，用于定位某个 Skill、工具或合同是否产生贡献。Diagnostic Run 不替代主实验结论，必须使用不同的 batch kind 和独立匿名映射。

### 5.6 Paired Attempt

同一个 RunPlan 下 Stock 与 Modified 各一次运行构成一个 pair。任一侧机械无效，整个 pair 无效；另一侧不得获得“技术性胜利”。

## 6. 方案比较与选择

### 6.1 方案 A：Promptfoo 直接掌握全流程

优点是开发最快。缺点是实验计划、执行、评分、结果库和失败语义都被第三方工具绑定，难以保持私有边界和正式证据权威，也难以安全地向后替换。拒绝。

### 6.2 方案 B：自有薄控制器 + 锁定版 Promptfoo 执行适配器

控制器生成类型化计划、建立隔离、调用 Promptfoo、解析不可信结果、生成自有回执并导入 07A。Promptfoo 只负责启动指定 `codex_path_override` 的 App Server 和收集事件。采用。

### 6.3 方案 C：全部扩入现有 Rust `ai-ip-eval`

可最大化复用强证明内核，但会把快速批量实验和最终晋级证明耦合，并继续扩大已经过大的 runner/app_server 模块。07B 不采用；最终 proof 继续复用现有 Rust 路径。

## 7. 总体架构

```text
                    sealed CaseBundle
                           |
                    07B Plan Compiler
                           |
                   immutable RunPlan
                           |
                  Isolation Supervisor
                  /                  \
       Stock isolated cell     Modified isolated cell
       new App Server          new App Server
       new CODEX_HOME          new CODEX_HOME
       new workspace           new workspace
       new thread/cache/tmp    new thread/cache/tmp
                  \                  /
                   untrusted raw results
                           |
                  Receipt / Schema Gate
                           |
                   private arm receipts
                           |
                    Blind Packager
                           |
                  anonymous output A/B
                           |
                    07A Private Root
```

### 7.1 Plan Compiler

读取已封存的 07A CaseBundle、二进制登记、执行 profile 和运行次数，输出类型化 `CandidateRunPlan`。计划一旦封存不可原地修改；任何变更都产生新的 plan ID。

### 7.2 Isolation Supervisor

为每个 attempt 创建隔离根，准备目录、环境白名单、初始工作区快照和 App Server 启动参数。它负责进程生命周期和硬预算中止，不解释营销内容。

### 7.3 Promptfoo Adapter

把已校验计划投影为静态 Promptfoo 配置，调用精确锁定的本地 CLI，并把输出视为不可信输入重新解析。配置不得包含由案例输出、模型输出或任意动态 JavaScript 拼装的可执行代码。

Promptfoo 官方 App Server provider 支持通过 `codex_path_override` 选择具体 Codex 二进制，并能返回线程、turn、item、usage 与 trajectory 元数据；它会自行启动本地 App Server，而不是附着到 Codex Desktop。该能力适合开发批跑，但不能替代本系统的权威回执。

### 7.4 Receipt Gate

对原始结果做 JSON Schema 校验、规范化、哈希、预算核对、同等性核对和失败分类。机械有效且输出不同的完整 pair 才进入匿名化；机械有效但 canonical 输出相同的 pair 直接登记为平局。

### 7.5 Blind Packager

以密码学安全随机源为每个 batch 生成 A/B 映射，把真实臂映射保存在 07A 私有根。交给评审侧的包只包含作品、允许展示的案例上下文和匿名 ID，不含二进制路径、版本、轨迹、开发者标签或运行顺序。

## 8. 类型化合同

具体字段在实施计划中以 JSON Schema 测试先行，至少包含以下合同。

### 8.1 CandidateRunPlan

- `schemaVersion`
- `planId`
- `batchId`
- `batchKind`: `smoke | development_blind | diagnostic`
- `caseBundleId` 与 `caseBundleSha256`
- `caseAnswerSchemaId` 与 schema hash
- `stockTreatmentRef`
- `modifiedTreatmentRef`
- `modelRouteRef`
- `executionProfileRef`
- `workspaceTemplateSha256`
- `promptfooPackageVersion`
- `promptfooLockSha256`
- `replicationCount`
- `retryPolicy`
- `timeoutBudget`
- `tokenBudget`
- `requestBudget`
- `costBudgetCny`
- `networkPolicy`
- `permissionPolicy`
- `toolPolicy`
- `createdAt`
- `sealedAt`
- `planSha256`

计划不得保存真实 API Key、用户登录令牌或可逆凭据。

### 8.2 BinaryManifest

- `binaryId`
- `armClass`: `stock | modified`
- `sourceRepository`
- `sourceCommit`
- `forkBaseCommit`（Modified 必需）
- `targetTriple`
- `buildProfile`
- `toolchainVersions`
- `buildReceiptSha256`
- `binarySha256`
- `declaredCapabilities`

运行前必须重新计算二进制哈希。路径只是定位信息，不是身份依据。

### 8.3 TreatmentManifest

- `treatmentId`
- `binaryManifestRef`
- `codexHomeSeedSha256`
- `systemInstructionSha256`
- `capabilityBundleSha256`
- `effectiveCodexConfigSha256`
- `declaredCapabilities`
- `prohibitedCaseSpecificMaterial`
- `treatmentManifestSha256`

Stock 与 Modified 可以拥有不同的产品能力种子，但两者都必须在 SEALED 前冻结。能力种子不得包含本案例答案、评委 rubric、对标结果或只为当前题人工编写的隐藏提示。两个隔离舱从各自 treatment seed 初始化后，运行期不再读取开发仓库中的活动 Skill 或用户真实 `CODEX_HOME`。

### 8.4 ExecutionProfile

ExecutionProfile 是业务可扩展的能力声明，不把所有案例永久限制为只读或断网：

- `sandboxMode`
- `approvalPolicy`
- `networkMode`
- `networkAllowlist`
- `writablePaths`
- `externalTools`
- `environmentAllowlist`
- `maxWallClockSeconds`
- `maxOutputBytes`

L1/L2 固定材料案例默认断网和最小写权限。需要实时研究、MCP、媒体解析或文件生成的未来案例可声明更丰富 profile，但两个主实验臂必须获得同等外部条件。若网络结果会漂移，应先独立采集并封存为同一证据包，而不是让两臂在不同时间随机搜索。

### 8.5 ArmAttemptReceipt

- `attemptId`
- `pairId`
- `privateArmId`
- `treatmentManifestSha256`
- `binaryManifestSha256`
- `effectiveConfigSha256`
- `inputSha256`
- `workspaceBeforeSha256`
- `workspaceAfterSha256`
- `appServerProtocolSchemaSha256`
- `promptfooConfigSha256`
- `startedAt` / `finishedAt`
- `threadIdCommitment`
- `turnIdCommitment`
- `trajectorySha256`
- `outputSha256`
- `usage`
- `costEvidence`
- `exitClassification`
- `failureDetails`
- `receiptSha256`

### 8.6 PairedRunReceipt

- `pairId`
- `planSha256`
- 两个 `ArmAttemptReceipt` 的 commitment
- 条件同等性检查结果
- 顺序随机化 commitment
- pair 有效性
- 无效原因
- `outputRelation`: `distinct | canonicallyIdentical`
- 匿名映射 commitment

`outputRelation` 只在机械有效的 pair 上有意义：有效 pair 必须包含，无效 pair 必须省略。两个通过 schema 校验的 `CaseAnswer` 先规范化为 canonical JSON；只有 canonical bytes 与 SHA-256 同时一致时才记为 `canonicallyIdentical`。相同输出是有效实验结果，不得伪装成执行失败。匿名映射 commitment 只在 `distinct` 时存在；identical pair 不制造虚假映射。

### 8.7 LiveRunAuthorization

真实模型调用必须额外提供一次性授权：

- 精确 plan ID
- 模型与路由
- 最大请求数、Token、人民币预算和墙钟时间
- 授权创建时间、失效时间
- 一次性 nonce
- 授权状态

授权不包含密钥；使用一次后不可复用。

### 8.8 CandidatePairImportReceipt

导入发生在 `PairedRunReceipt` 封存之后，不能反写或替换已封存回执。独立导入回执至少包含：

- `importId`
- `pairId`
- `pairedRunReceiptSha256`
- `stockOutputSha256`
- `modifiedOutputSha256`
- `outputRelation`
- `blindDisposition`: `readyForBlindReview | identicalTieNoReview`
- `privateDestinationCommitment`
- `importedAt`
- `importReceiptSha256`

同一个 pair 只能成功导入一次。重复导入、目标已存在或源回执验证失败都必须失败关闭。

## 9. 完全隔离规则

### 9.1 文件系统隔离

每个 arm attempt 必须拥有不同的：

- `CODEX_HOME`
- workspace root
- thread store
- cache root
- temp root
- log root
- Promptfoo output root
- process working directory

所有目录必须位于本次 batch 的受控临时根或 07A 私有根下，不允许默认回落到用户真实 Home。不得用符号链接把一个臂的可写路径指向另一臂。

### 9.2 进程与线程隔离

- 每个 attempt 启动新 App Server 进程；禁止跨臂或跨 attempt 复用。
- 每个 attempt 新建线程；禁止 resume 旧线程。
- 一个进程退出后才可封存该 attempt 的完整运行状态。
- 执行顺序随机化，真实顺序只写私有回执。

### 9.3 环境隔离

- `inherit_process_env` 必须关闭或等价实现。
- 只注入 ExecutionProfile 白名单中的非秘密变量。
- 真实 provider key 不进入候选进程、Agent shell、Promptfoo 配置或 `cli_env`。
- 本地凭据代理向 App Server 暴露的只能是受限 loopback 地址与非生产占位令牌。

Promptfoo 官方文档明确提醒，放入 CLI 环境的凭据可能被 Agent shell 看到，因此 07B 的 GLM live path 不得直接用该便利配置传递真实密钥。

### 9.4 信息隔离

候选不得接收：

- 评审 rubric、评委资料或评分提示；
- 参考答案、发布后结果或历史胜负；
- 另一臂的输入之外状态、输出、轨迹或身份；
- `stock`、`modified`、`baseline`、`candidate` 等实验标签；
- 真实 A/B 映射。

二进制可能通过自身行为感知其能力差异，07B 不声称能消除这种自省；它保证不额外注入实验身份或对手信息。

### 9.5 条件同等性

主实验下列项目必须相同：

- CaseBundle bytes
- 输出 schema
- 模型路由、模型名和 reasoning effort
- 预算与超时
- sandbox、approval、网络和外部工具权限
- 初始工作区快照
- 运行宿主类别与资源上限
- Promptfoo 版本和适配器版本

Stock/Modified 的 `TreatmentManifest` 按设计不同，但其每一项差异必须在 SEALED 前声明并进入回执。未声明的有效配置差异视为污染。不满足任一同等条件或发现未声明差异，pair 在运行前拒绝或在运行后标记无效。

## 10. 运行状态机

```text
DRAFT
  -> SEALED
  -> PREFLIGHT_PASSED
  -> ISOLATION_READY
  -> RUNNING
  -> RAW_CAPTURED
  -> RECEIPTS_SEALED
       |-> [distinct] ANONYMIZED -> IMPORTED_TO_07A
       |-> [canonicallyIdentical] IDENTICAL_TIE_RECORDED -> IMPORTED_TO_07A
```

任何阶段均可进入相应失败终态：

- `FAILED_PLAN`
- `FAILED_PREFLIGHT`
- `FAILED_ISOLATION`
- `FAILED_EXECUTION`
- `FAILED_SCHEMA`
- `FAILED_BUDGET`
- `FAILED_EVIDENCE`
- `FAILED_IMPORT`

失败状态不可被原地改写为成功。修复后重跑必须使用新 attempt ID；原失败记录继续保留。

## 11. 重试、超时与无效组

- 默认不自动重试。
- 若某类基础设施允许重试，策略必须在 SEALED 前声明，并对两个臂一致。
- 每次重试生成独立 receipt，不能覆盖前一次 stdout、轨迹或输出。
- 任一臂超时、崩溃、被预算中止、输出不合 schema 或证据不完整，整个 pair 无效。
- 无效 pair 不进入作品胜负统计，也不能让成功的一臂获得胜场。
- 候选输出内容质量差不是机械失败；只要合同有效，仍应匿名交给评委。
- 两臂 canonical 输出完全相同是有效平局，不创建供真人比较的 A/B 包，不得计为任何一臂获胜，也不得单独支撑晋级。
- 评委不得因“工具调用多”“推理长”或“成本高”直接推断臂身份；成本与轨迹不进入普通作品包。

## 12. 运行次数

### 12.1 机械冒烟

每臂一次。只验证链路、隔离、证据和导入，不判断营销能力。

### 12.2 开发盲评

默认每臂三次。replication count 在运行前封存，用于观察模型波动和结果稳定性，不能看完结果再修改样本量。

07B 必须按已封存 `replicationCount` 一次性派生并执行恰好对应数量的 pair；每个 replication 有独立 pair/attempt ID、顺序随机化和回执。执行器不得因早期胜负、平局或失败自适应增减样本。

### 12.3 晋级证明

样本量和停止规则由 07D 在看到结果前预注册。07B 只执行已封存计划，无权自行扩大或缩小样本。

## 13. Promptfoo 边界

### 13.1 允许职责

- 根据静态配置启动精确二进制；
- 发送题目和输出 schema；
- 收集 final output、usage 和 App Server trajectory 元数据；
- 输出机器可解析的原始结果。

### 13.2 禁止职责

- 保存正式 arm mapping；
- 访问 07A rubric、参考案例或真实业务结果；
- 使用 AI Judge 产生正式分数；
- 决定 batch PASS、解锁或晋级；
- 动态执行来自案例或模型输出的 JavaScript；
- 直接持有真实火山 API Key；
- 作为唯一证据存储。

### 13.3 版本锁定

实施时必须从官方包元数据和官方 App Server provider 文档核对兼容版本，再以 exact version 写入 package manifest 与 pnpm lock。禁止范围版本、`latest`、`pnpm dlx promptfoo` 或未锁定的全局安装。

Codex App Server 协议仍随版本演进，计划必须同时锁定两侧二进制和协议 schema hash。不得假设任意 Promptfoo 版本可兼容任意 Codex 二进制。

## 14. Provider 与 GLM 接入

### 14.1 Provider-free 默认路径

机械验收使用本地确定性 Responses 假服务或现有测试代理：

- 不访问公网模型；
- 不读取用户真实凭据；
- 两侧接收可验证的等价响应；
- 实际启动 App Server，而不是只 mock Python 函数；
- 能制造超时、断流、坏 schema、预算超限和部分轨迹等故障。

### 14.2 真实 GLM 冒烟

只有 provider-free 全绿后才能准备 LiveRunAuthorization。执行前必须向产品负责人展示：模型、运行次数、最大 Token、最大请求数、预算上限和预计输出。

真实密钥通过现有 06B1 本地凭据/预算代理边界或经等价强度审查的新适配层读取，绝不复制到仓库、日志或候选环境。若现有代理无法安全承载 Promptfoo App Server path，live gate 保持关闭；不得为赶进度降低密钥边界。

首个 live smoke 使用黄金礼品 L1/L2 同题 pair，只生成匿名作品和证据，不产生营销胜负结论。火山 API 中的 GLM 可作为优先真实路线，但不能被记为独立跨 provider 证据。

## 15. 业务不被套死的设计

隔离约束只防串线、篡改、偷看和无限付费，不规定营销方法本身。07B 不硬编码内容根、内容地图、语义跃迁、固定 Agent 数量或固定提示词链。

业务扩展通过版本化合同完成：

- CaseBundle 可承载不同行业、账号阶段、目标人群和真实素材；
- ExecutionProfile 可开放研究、MCP、媒体解析、文件产出等能力；
- 输出 schema 可从 L1/L2 演进到 L3-L5；
- Modified Arm 可按产品演进携带新的 Lead Skill 和按需业务工具；
- 每项新增永久复杂度仍需遵守六版决策记忆的 held-out failure、baseline、最小候选、盲比和删除路径要求。

安全边界不评价“什么内容一定会爆”，也不以固定规则替代模型涌现和教师系统。

## 16. CLI 表面

计划新增独立 batch CLI，避免继续把所有命令堆入现有 07A CLI。最终命名可在实施计划中微调，语义固定为：

```text
batch plan        生成并校验 DRAFT 计划
batch seal        冻结计划并生成 plan hash
batch preflight   校验二进制、依赖、目录、模型路线和预算
batch run         执行 provider-free 计划
batch run-live    只接受一次性授权文件，不接受 API Key 参数
batch verify      重算全部 commitment 并验证 pair
batch anonymize   生成 A/B 包和私有映射
batch import      导入 07A PrivateRoot
batch status      显示机械状态，不显示营销胜负
```

危险默认值：

- `run` 默认 provider-free；
- 不存在自动 fallback 到 live provider；
- `run-live` 没有有效授权立即退出；
- CLI stdout 不输出密钥、真实 arm mapping 或私有绝对路径；
- 所有写入目标必须先通过 07A PrivateRoot 边界校验。

## 17. 建议代码边界

生产模块目标保持在 500 行以内，优先新增小模块而不是扩大 `codex-rs/ai-ip-eval/src/runner.rs` 或 `app_server.rs`。

建议落点：

```text
scripts/ai_ip/eval_lab/
  batch_contracts.py
  batch_plan.py
  batch_isolation.py
  batch_promptfoo.py
  batch_receipts.py
  batch_controller.py
  batch_cli.py

ai-ip-evals/lab/schemas/
  candidate-run-plan.schema.json
  binary-manifest.schema.json
  treatment-manifest.schema.json
  execution-profile.schema.json
  arm-attempt-receipt.schema.json
  paired-run-receipt.schema.json
  candidate-pair-import-receipt.schema.json
  live-run-authorization.schema.json

ai-ip-evals/lab/promptfoo/
  package.json
  README.md
  locked static templates or generated-config schema
```

Promptfoo 加入现有 pnpm workspace 并使用根 lockfile；不引入第二个业务后端或常驻 Node 服务。

## 18. 测试策略

### 18.1 Schema 与合同测试

- 每个新 schema 都有最小合法 fixture、全字段合法 fixture 和非法 fixture。
- 缺 ID、错版本、预算为负、未知枚举、错误哈希、额外秘密字段必须拒绝。
- 规范化 JSON 与 SHA-256 在不同键顺序下保持稳定。

### 18.2 隔离单元测试

- 两臂目录绝不重合；
- 符号链接逃逸被拒绝；
- 环境变量白名单有效；
- 用户真实 Home 不可见；
- 重用 server/thread 被拒绝；
- 一侧不能读取另一侧 sentinel 文件；
- arm label 不进入候选输入或评审包。

### 18.3 失败语义测试

- 一侧超时、崩溃、坏 JSON、错误 schema、缺轨迹或预算超限使 pair 无效；
- 失败不能被成功 attempt 覆盖；
- 未声明重试被拒绝；
- 计划、二进制、配置、输出或回执任意一字节变化都会导致 verify 失败；
- Promptfoo exit 0 但证据不完整仍然失败关闭；
- canonical 输出相同时形成有效 `identicalTieNoReview`，不进入真人 A/B 包，也不计胜场；
- `replicationCount=3` 时恰好产生三个独立 pair，不能提前停止或追加第四个。

### 18.4 Provider-free 真实进程测试

- 用真实 Stock/Modified App Server 进程连接本地假 Responses 服务；
- 精确同题产生两份合 schema 输出；
- 捕获 usage、trajectory 与过程状态；
- 成对回执封存成功；
- 匿名包可导入 07A，并通过现有 07A 验证。

### 18.5 Live smoke

- 必须在独立命令中显式执行；
- 预算与授权测试先于网络调用；
- 真实 key 不出现在进程列表、配置、stdout、日志、轨迹或 artifacts；
- 完成后授权变为 consumed；
- 输出只进入私有根与匿名评审包。

## 19. 完成标准

07B 分成两个不混淆的完成状态。

`IMPLEMENTATION_COMPLETE_LIVE_PENDING_AUTHORIZATION` 必须同时满足：

1. 精确登记并校验 Stock/Modified 两个二进制。
2. 同一 RunPlan 能启动两个真正独立的 App Server 候选 Agent。
3. 除声明 treatment 外，机器证明其他条件一致。
4. 输出、轨迹、用量、错误和哈希形成完整私有回执。
5. 任何篡改都能被 `verify` 发现。
6. 评审包中不存在真实 arm 身份泄漏。
7. 无效 pair、重试与预算规则全部失败关闭。
8. Promptfoo 与 pnpm 依赖精确锁定，无动态 JS 配置入口。
9. provider-free 真实进程端到端测试通过。
10. distinct 匿名输出无损导入 07A；canonical identical 输出以有效平局导入私有根但不生成伪 A/B 评审包。
11. 现有 07A 测试继续全绿。
12. 代码审查 Critical/Important 为零，生产模块没有不必要的大文件增长。

在此基础上，只有再满足以下条件，状态才可升级为 `07B_COMPLETE`：

13. 获得一次性授权后，GLM 黄金礼品冒烟生成第一组真实匿名作品。
14. live 授权、预算、凭据隔离和匿名导入证据全部验证通过。

未获得授权不是代码失败，但必须保留 `IMPLEMENTATION_COMPLETE_LIVE_PENDING_AUTHORIZATION`，不能伪装已经完成 live 证明。

07B 完成不等于 Phase 0A G2 PASS，也不等于爆改版营销能力已获证明。它只表示候选作品生产链可信可用。

## 20. 失败与降级策略

- Promptfoo provider 与当前 Codex 协议不兼容：锁定兼容组合或实现极薄兼容适配，不改弱证据合同。
- Stock Codex 无法运行同一模型路线：本 batch 无效；不得偷偷换模型。另开“产品兼容性”事实记录，不计内容质量胜负。
- GLM 凭据代理无法保证密钥隔离：只完成 provider-free 07B，live gate 保持关闭。
- trajectory 缺少必要字段：保留原始输出用于诊断，但不得导入正式盲评 batch。
- 运行磁盘占用较大：保留封存证据，允许删除可再生 workspace/cache；不得以节省空间为由删除唯一证据。
- 业务能力需要联网：先封存共同证据包；确需 live network 时使用同等 allowlist 和预注册时间窗，并把外部漂移标为限制。

## 21. 可删除路径

07B 是外置评测壳，不进入产品主业务状态。若它未能提高验证可信度，可删除：

- `scripts/ai_ip/eval_lab/batch_*`
- 07B 新增 schemas
- `ai-ip-evals/lab/promptfoo` package
- 对应测试与 fixtures

删除后 07A provider-free lab、06A/06B1 原生强证明和 Codex 产品运行路径继续存在。07B 不能把不可逆生产迁移作为前置条件。

## 22. 六版复杂度准入自审

1. **具体 held-out failure 是什么？** 手工候选运行无法证明同题、隔离、同预算和未重跑；前六版测试绿也无法证明业务比较可信。
2. **最简单解释是什么？** 候选生成链缺少可验证实验合同，导致评审输入可能已被污染。
3. **最小候选机制是什么？** 自有薄控制器 + 两个新 App Server 进程 + 规范化回执；不造新业务平台。
4. **可测结果是什么？** provider-free 条件下能生成并验证一个完整匿名 pair，任一污染与篡改都失败关闭。
5. **更简单基线是什么？** 人工打开两个任务复制同一问题。它无法证明 Home、缓存、线程、配置和重试独立，因此只可用于探索，不可作为正式证据。
6. **固定案例如何使用？** 黄金礼品 L1/L2 只用于机械回归与后续校准，不充当 G2 新真实案例。
7. **如何删除？** 07B 文件可整体移除，不影响产品 App Server 和 06A/06B1 核心。
8. **何时允许升级永久复杂度？** 只有真实盲评暴露明确失败、简单 baseline 不足、候选改动能被盲比验证且仍有删除路径时。

## 23. 实施顺序约束

实施计划必须遵循测试驱动顺序：

1. schemas 与非法 fixtures；
2. plan sealing 与哈希；
3. 隔离目录和环境边界；
4. 假执行器下的 paired state machine；
5. Promptfoo exact pin 与静态配置适配；
6. 真实 App Server + 本地假 Responses 端到端；
7. 07A 匿名导入；
8. 文档、台账和全量 07A 回归；
9. 单次授权后的 GLM live smoke。

任何任务不得以“以后再补”为由绕过 schema、隔离、预算或回执测试。实现期间若发现设计与当前 Promptfoo/Codex 协议冲突，应先修改本规格并重新确认，不得在代码中静默形成第二套合同。

## 24. 参考资料

- [Promptfoo Codex App Server provider 官方文档](https://www.promptfoo.dev/docs/providers/openai-codex-app-server/)
- [Promptfoo 官方仓库](https://github.com/promptfoo/promptfoo)
- [OpenAI Codex App Server README](https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md)
- `docs/superpowers/specs/2026-08-31-marketing-capability-evaluation-lab-design.md`
- `docs/superpowers/specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md`
- `docs/architecture/2026-08-25-legacy-six-version-decision-memory.md`
- `docs/architecture/codex-fork-patch-ledger.md`
