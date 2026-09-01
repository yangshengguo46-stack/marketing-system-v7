# AI IP 营销评测实验室 07B-LH1：认证孤儿生命周期权威设计

> 日期：2026-09-01  
> 状态：设计已由产品负责人确认；待独立实施与验证  
> 子阶段：Marketing Evaluation Lab 07B-LH1 / Authenticated Orphan Authority  
> 上游：07B Task 4 在第五轮 breaker 停止，Critical 0 / Important 1 / Ready No  
> 直接后继：07B Task 5 Promptfoo 静态编译适配

## 1. 决策摘要

07B-LH1 是一个单独立项、单独分支、单独验收的生命周期加固阶段，不是 07B Task 4 的第六轮补丁。

它只解决一个负载问题：候选子进程已经由操作系统创建，但完整 `OwnedProcess` 尚未交给 `PairLifecycle` 时，如果后续初始化失败且 TERM/KILL 停止无法确认，系统必须持久记录“可能仍然存活”，保留其隔离单元，并禁止任何完成回执。

采用“预注册进程生命契约”方案：

1. `Popen` 之前，`PairLifecycle` 为本次 attempt 签发唯一的待启动契约。
2. `Popen` 返回的瞬间，守卫把 PID 和进程组绑定到契约；不等待 `OwnedProcess` 构造完成。
3. 完整构造成功后，契约原子升格为已拥有进程。
4. 中途失败且停止确认成功，契约进入已停止失败终态，按原有流程清理。
5. 中途失败且停止无法确认，契约进入不可逆的孤儿终态，使生命周期直接掌握事实，不依赖外层异常类型。

LH1 不修改营销方法、Agent 能力、评委体系、Promptfoo 适配或业务合同。

## 2. 权威和范围

冲突时按以下顺序解释：

1. 本 LH1 规格；
2. `2026-08-31-codex-isolated-batch-runner-design.md`；
3. 07B SDD 台账 R8、R9 和 R10；
4. `task-4-review-round-5.md` 的最终 breaker 结论；
5. 更高层营销评测实验室和六版决策记忆。

本阶段不改变以下已封存决策：

- 一个 Lead 对候选作品结果负责，按需调用业务能力。
- Task 4 的生产边界是不可变 `LaunchSpec` 与控制器拥有的 OS 进程/描述符。
- 两个 arm 都必须完成启动尝试后才观测结果，不得根据第一个 arm 的结果自适应取消第二个。
- POSIX/macOS 是当前生产执行后端；Windows 继续在 R8 下失败关闭。
- 输出、时间、进程组、描述符和证据边界不降级。

## 3. 已证实的根因

失效链路如下：

```text
Popen 返回原始进程
  -> ProcessOwnershipGuard 暂时持有它
  -> post-Popen 初始化故障
  -> guard.abort() 执行 TERM/KILL
  -> 停止无法确认，抛出 FatalSupervisorError
  -> controller 的逐 arm 启动聚合捕获该异常
  -> 统一包装成 BatchControllerError
  -> PairLifecycle.abort() 只检查最外层异常
  -> spawn 没有返回，因此 lifecycle.processes 也是空的
  -> 孤儿事实丢失，隔离单元可能被清理
```

根因不是缺少一个 `isinstance` 分支，而是进程所有权在“OS 已创建”与“完整对象已绑定”之间只存在守卫局部状态，没有进入 pair 生命周期的权威状态机。

## 4. 备选方案

### 4.1 方案 A：预注册进程生命契约（采用）

优点是进程创建前就建立 pair 权威，异常包装不再能丢失孤儿事实；可同时统一未启动、原始进程、已拥有进程和孤儿四种状态。成本是增加一个小型、明确的生命周期类型。

### 4.2 方案 B：递归检查异常链（拒绝）

改动最小，但安全依赖每个中间层正确保留 `__cause__`。任何聚合、序列化或二次包装都可能重新打开空窗。它治疗异常表现，不治疗所有权根因。

### 4.3 方案 C：守卫直接写孤儿文件（拒绝）

能缩短持久化路径，但会使 `ProcessOwnershipGuard` 获得私有根、pair 证据和隔离单元的第二套权威，与 `PairLifecycle` 发生重复和状态冲突。

## 5. 组件边界

### 5.1 ProcessLifecycleLease

新增的私有生命契约只有一个职责：把一次 attempt 的进程事实从启动前连续传递到 pair 终态。

必须支持下列单向转换：

```text
RESERVED
  -> ATTACHED_RAW_PROCESS
       -> PROMOTED_OWNED_PROCESS
       -> STOP_CONFIRMED
       -> ORPHANED
  -> CANCELLED_BEFORE_START
```

约束：

- 每个 attempt 只能预留一个契约。
- `attach_raw_process` 只能从 `RESERVED` 进入，并保存 PID/PGID。
- `promote` 只能从 `ATTACHED_RAW_PROCESS` 进入，并将同一个进程转交给现有 `OwnedProcess` 集合。
- `STOP_CONFIRMED`、`ORPHANED` 和 `CANCELLED_BEFORE_START` 都是终态。
- 重复、跨 attempt、跨 pair 或状态逆转必须失败关闭。
- 契约不拥有营销输出、提示词或 provider 信息。

### 5.2 ProcessOwnershipGuard

守卫继续拥有启动描述符和原始 `Popen` 对象，但必须同时持有本 attempt 的契约。

- `Popen` 成功返回后立即绑定契约。
- 转交成功时，先将契约升格为同一 `OwnedProcess`，再释放守卫局部所有权。
- abort 停止确认成功时标记 `STOP_CONFIRMED`。
- abort 停止无法确认时标记 `ORPHANED`，再抛出结构化的 fatal 错误。
- 结构化错误仍保留诊断价值，但孤儿真值的权威来源是契约状态，不是异常链。

### 5.3 PairLifecycle

`PairLifecycle` 成为唯一的 pair 终态权威：

- 签发和持有本 pair 的所有契约。
- 任一契约为 `ORPHANED` 时，不清理任何已绑定隔离单元。
- 任一契约仍处于 `ATTACHED_RAW_PROCESS` 时，不允许进入完成。
- abort 时仍对已升格的 `OwnedProcess` 执行现有停止确认。
- 只有所有进程均被证明停止且没有孤儿契约时，才能清理单元或返回完成。
- 异常聚合仍可对外使用 `BatchControllerError`，但不得改写契约事实。

### 5.4 OrphanAuthorityRecord

每个孤儿 attempt 产生一份独立、不可覆盖的权威记录，避免两个 arm 同时孤儿时必须覆写 pair 级文件。文件名由已验证的 pair ID 与 attempt ID 确定，通过已绑定的私有根描述符排他写入。

记录至少包含：

- `schemaVersion`: `1`
- `pairId`
- `attemptId`
- `processId`
- `processGroupId`
- `launchSpecSha256`
- `status`: `orphaned`
- `stopDisposition`: `unconfirmed`
- `errorType`
- `recordedAt`
- `orphanSha256`: 对除本字段外全部语义字段的 canonical JSON 自承诺

不记录完整环境变量、API Key、候选输出、对标素材或业务私密。

## 6. 认证与离线验证

“认证孤儿”不表示伪造一个“进程一定存活”的结论；它证明的是控制器曾启动指定进程，并且在规定停止协议内无法证明它已停止。

离线验证必须：

1. 重新认证 `PrivateRoot` 身份，并绑定根描述符。
2. 从已封存 layout authority 验证 pair 和 attempt 归属。
3. 以有界、no-follow 方式读取孤儿记录。
4. 验证 canonical bytes、精确字段类型、`orphanSha256` 和已封存 `launchSpecSha256`。
5. 拒绝缺字段、额外字段、篡改、文件名/内容不匹配、跨 pair 与跨 attempt 重放。
6. 验证孤儿 pair 不存在有效 `receipt.json`，并存在失败 tombstone。

只有成功完成上述验证的记录才能被报告为“已认证孤儿”。未能写入或验证孤儿记录时，系统仍必须保留隔离单元并失败关闭，不得以“记录失败”推导“进程已停止”。

## 7. 精确数据流

```text
PairLifecycle.reserve_process(attempt, launchSpecSha256)
  -> ProcessLifecycleLease(RESERVED)
  -> ProcessOwnershipGuard(lease)
  -> Popen
  -> lease.attach_raw_process(pid, pgid)
  -> post-Popen materialization
       | success
       |   -> lease.promote(OwnedProcess)
       |   -> existing pair supervision
       |
       | failure
           -> TERM/KILL + bounded positive wait
                | confirmed
                |   -> lease.confirm_stopped()
                |   -> normal pair failure cleanup
                |
                | unconfirmed
                    -> lease.mark_orphaned(error)
                    -> controller may aggregate errors
                    -> PairLifecycle observes ORPHANED directly
                    -> seal failure tombstones
                    -> seal per-attempt OrphanAuthorityRecord
                    -> preserve every bound cell
                    -> no pair receipt, no completion
```

两个 arm 的启动尝试顺序规则保持不变。第一个 arm 形成孤儿契约时，控制器仍按已封存顺序完成第二个启动尝试；在离开 pair 生命周期前统一执行孤儿终态。

## 8. 失败规则

- `Popen` 之前失败：契约取消，无孤儿，按原路径清理。
- `Popen` 后失败、停止确认：记失败 tombstone，无孤儿，允许清理。
- `Popen` 后失败、停止未确认：必须封存孤儿权威，保留所有已绑定隔离单元，禁止完成回执。
- 孤儿记录写入失败：仍保留单元，向上报告证据失败，不降级为普通失败。
- 已升格 `OwnedProcess` 停止未确认：复用同一权威记录和保留规则，不维护第二套孤儿格式。
- 外层异常包装、聚合或文案变更：不影响上述状态。
- 不存在从 `ORPHANED` 自动恢复为完成的路径。后续人工清理属于另一个明示授权的运维能力，不在 LH1 内。

## 9. 测试策略

实施必须严格按 RED -> GREEN -> REFACTOR 进行，并保留失败证据。

### 9.1 核心回归

- 在已覆盖的六个 post-`Popen` 故障点中注入至少一个故障，同时使停止确认返回 false。
- 证明外层最终仍可抛出 `BatchControllerError`，但契约为 `ORPHANED`。
- 证明不存在 pair `receipt.json`，并且 pair/attempt 失败 tombstone 存在。
- 证明两个隔离单元都未被清理，孤儿 attempt 的权威记录存在。
- 证明两个 arm 均完成且仅完成一次启动尝试。

### 9.2 多孤儿和正常分支

- 两个 arm 同时停止未确认时，产生两份不冲突的 attempt 级记录。
- 一个 arm 孤儿、一个 arm 正常启动时，整个 pair 仍无完成回执并保留全部单元。
- 停止可确认时，不产生孤儿记录，且原有清理行为不变。
- `Popen` 之前失败时，契约不伪造 PID 或孤儿。

### 9.3 权威和篡改测试

- 离线验证接受原始 canonical 孤儿记录。
- 修改 PID、PGID、pair ID、attempt ID、launch spec commitment 或状态任意一项后必须拒绝。
- 把记录复制到另一个 pair/attempt 后必须拒绝。
- 根路径、layout 目录或记录文件被替换时必须失败关闭。
- 孤儿记录写入被注入失败时，单元仍然保留。

### 9.4 回归范围

- 新增 LH1 聚焦测试。
- Task 4 全部历史评审测试。
- Task 1–4 的精确 17 文件合并回归。
- 已存在的 10 个 Windows 平台跳过可保留；不允许新增 POSIX/macOS 跳过。
- 不运行 provider、网络、Promptfoo、App Server、付费模型或媒体生成。

## 10. 实施边界

预期只修改或新增下列职责边界，精确文件名由实施计划根据现有模块固化：

- pair 生命周期与进程契约；
- post-`Popen` 所有权守卫；
- 孤儿权威记录的封存与离线验证；
- 一个独立 LH1 对抗测试文件。

非机械改动总量必须保持在 800 行以下，复杂生命周期逻辑目标在 500 行以下；任一生产模块目标低于 500 行，不向现有 477 行的高触达 supervision 模块堆入独立权威逻辑。

## 11. 完成标准

LH1 只在以下条件同时满足时记为完成：

1. 失败的 post-`Popen` 停止无法确认时，pair 生命周期直接保有孤儿事实。
2. 异常被任意外层聚合后，孤儿语义、单元保留和无完成回执规则仍然成立。
3. 每个孤儿 attempt 都有独立、不可覆盖、可离线验证的权威记录。
4. 无法封存记录时仍保留隔离单元，不产生假阳性完成。
5. 停止已确认和 `Popen` 前失败的原有行为没有退化。
6. 两 arm 精确启动尝试和零重试规则没有退化。
7. 新增聚焦测试、Task 4 回归和 Task 1–4 合并回归全部通过，只保留 R8 中已知的 Windows 跳过。
8. 独立任务评审和最终全分支评审均为 Critical 0 / Important 0 / Ready Yes。
9. 代码规模、模块规模、格式化、diff 卫生和工作树清洁符合仓库规则。
10. LH1 通过后才允许解除 07B Task 5 阻塞；通过不等于营销能力获证或 07B 全部完成。

## 12. 非目标与可删除路径

LH1 不实现：

- 孤儿进程的自动扫描、重试终止或后台运维系统；
- Windows 进程权威后端；
- Promptfoo 或 Codex App Server 接入；
- provider 路由、密钥、付费模型或媒体生成；
- 营销评分、对标分析、教师系统或 IP 作品生成。

若后续被更强的进程管理内核替代，可整体删除新契约和孤儿验证模块，并将 `PairLifecycle` 的调用点恢复为单步进程绑定；07A、Codex 产品路径和营销业务合同不依赖 LH1 的具体类名。
