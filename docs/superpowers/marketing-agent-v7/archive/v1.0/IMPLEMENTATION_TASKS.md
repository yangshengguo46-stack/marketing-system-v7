# 可交给 Coding Agent 的施工任务单

基线：`marketing-system-v7@e08f1e1ad5bbc3ea036aa271894d8228e499ed61`。执行依据是同目录 `FINAL_SPEC.md`，本文件不授权付费模型调用、平台发布、删除客户数据或直接改远端默认分支。

## 执行前必须读取

`output/business-methods/产品交付与开发验证约定.md`、`docs/architecture/2026-08-25-legacy-six-version-decision-memory.md`、当前 `work_chain.rs/work_tool.rs/content_package.rs`、`core/src/session/turn.rs/world_state.rs`、`ext/extension-api/src/contributors.rs`。

先检查本地 `git status --short`、HEAD 与分支。若 HEAD 不同，不回退或覆盖用户修改；产出差异清单，按真实符号核对接缝。下面的 NEW 路径是建议新增文件，不声称原仓已有。所有私有凭证保持本地，不打印。

## 全局研发合同

- 每个工作包单独分支/提交，只有定向失败测试存在后才写实现。
- 不创建新的业务运行时，不添加第二个 Lead，不恢复旧 Eval Lab。
- 对 Rust 文件按现仓 import/format/style 规范修改；不得执行会改写全部 output/research 的全仓格式化。
- 用 `just test -p <crate>` 跑实际定向测试。命令为施工时运行，不是本交付包已执行记录。
- 增加 workspace crate 时更新 Cargo workspace/lock 与对应 BUILD；按仓库实际流程更新 Bazel lock。
- 不以旧 Cargo.lock 中偶然已有依赖为理由固定过时安全版本；任何依赖更新要单独记录。
- 固定自己产品的 protocol/schema 版本，不把上游文档的参数原样复制当已验证接口。

---

## WP00：锁基线、记录自有 Core 决策、验证可达能力

**目标：** 后续施工不会误信已删除能力和未验证模型。

**文件：**
- NEW `docs/architecture/ADR-OWNED-CORE-001.md`
- NEW `docs/implementation/marketing-v7-baseline.json`
- MODIFY 直接冲突的旧实施计划，标为 superseded；不修改旧实验结论。
- NEW `scripts/ai_ip/smoke/native_capabilities.ts`（最小诊断客户端，无编排功能）

**必须完成：**
1. 记录 HEAD、工作区脏文件、实际模型配置名称及脱敏能力状态。
2. 区分现有 `ai-ip-domain/runtime`、已删除 evaluator、未开工产品能力。
3. 检查 `web.run` 对目标 provider 是 callable/不可达/权限缺失；不自动花费未批准费用。
4. 检查 `ContextContributor` 的 world-state 路径与 Stop hook 实際执行顺序。
5. 生成 native tool、provider、MCP、Skill、权限的精简 inventory。未验证项是 unknown，不是 available。

**失败测试：** profile 未就绪但能力目录标为 available 时失败；GPT-6 被生产 profile 默认依赖时失败；旧评测目录缺失却“调用成功”时失败。

**完成证据：** baseline.json + 实际可用能力记录 + 旧约束替代清单。此包不宣布作品质量通过。

---

## WP01：任务/作用域/工件合同与存储

**目标：** 一次任务可以可信保存、隔离、恢复，不涉及营销路线。

**文件：**
- NEW `codex-rs/ai-ip-domain/src/{work.rs,artifact.rs,scope.rs,command.rs}`
- MODIFY `ai-ip-domain/src/lib.rs`
- NEW `codex-rs/ai-ip-store/{Cargo.toml,BUILD.bazel}`
- NEW `ai-ip-store/src/{lib.rs,db.rs,mission.rs,artifact.rs,events.rs,objects.rs}`
- NEW `ai-ip-store/migrations/001_initial.sql`
- NEW `ai-ip-store/tests/{transaction.rs,scope.rs,objects.rs}`
- MODIFY Cargo workspace/lock、Bazel 相关配置

**目标接口：**
```rust
trait MissionStore {
    fn get(&self, scope: Scope, id: MissionId) -> StoreFuture<MissionSnapshot>;
    fn apply(&self, scope: Scope, command: WorkCommand) -> StoreFuture<CommandReceipt>;
    fn bind_thread(&self, scope: Scope, command: BindThread) -> StoreFuture<ThreadBinding>;
    fn events_after(&self, scope: Scope, mission: MissionId, seq: u64, limit: u32)
        -> StoreFuture<EventPage>;
}
trait ObjectStore {
    fn stage(&self, authorized_input: AuthorizedInput) -> StoreFuture<StagedObject>;
    fn commit(&self, staged: StagedObject) -> StoreFuture<ObjectRef>;
    fn read(&self, scope: Scope, object: ObjectRef, range: ReadRange) -> StoreFuture<Bytes>;
}
```
以上均 NEW；`Scope/AuthorizedInput` 由主机授予，不能从模型自由反序列化后直接信任。

**必须完成：** 外键 scope、CAS revision、command 去重、mission 事件原子写、staging→对象库→DB 引用协议、thread binding。

**失败测试：** 两个 writer 同 revision 只有一个成功；重复 command 不重复事件；同 key 不同请求冲突；跨项目 artifact 引用失败；对象写后 DB 前故障不出现已交付对象；缺失对象读取明确失败。

**验收命令：** `just test -p codex-ai-ip-domain -p codex-ai-ip-store`；按实际仓库格式/锁流程验证。

**回滚：** 新功能开关关闭，保留新 DB 不损坏旧 JSON；不得破坏用户已有工作区。

---

## WP02：旧 WorkChain 导入与中性工作工具

**文件：**
- NEW `ai-ip-store/src/legacy_import.rs`
- NEW `ai-ip-runtime/src/{work_service.rs,artifact_tool.rs}`
- MODIFY `ai-ip-runtime/src/{work_tool.rs,lib.rs}`
- 保留 `work_chain.rs` 为 legacy adapter；不再作为新任务真相。

**接口与实现：**
- `import_work_chain(scope,work_id,original_bytes)->ImportReceipt`
- `marketing_work.create/read/update/add_note/record_decision/propose_delivery`
- `marketing_artifact.create/read/revise/list/export`
- 旧 open/record 参数经 adapter 映射；不返回 nextStage 命令，不要求顺序。
- 新任务写入 SQLite；旧 JSON 只读，不双写。

**失败测试：** draft 可以在没有 research 的情况下创建；资料修正使真实依赖的稿件失效；无关备注修改不废掉所有稿件；重复导入幂等；旧文字“已发布”不产生 ActualResult；旧 Readiness 不升级为审核通过。

**完成证据：** 从真实旧工作 JSON 导入测试副本后的对象清单、hash 对照、缺失材料报告。不是读生产客户目录运行破坏测试。

---

## WP03：任务 WorldState、恢复与能力可见

**文件：**
- NEW `ai-ip-runtime/src/{world_state.rs,context_budget.rs,capabilities.rs,lifecycle.rs}`
- MODIFY `app-server/src/extensions.rs`，注入同一 mission/store 服务
- 必要时 MODIFY `core/src/session/world_state.rs`，只补测试证实的通用机制
- NEW `ai-ip-runtime/tests/{context_projection.rs,capability_inventory.rs}`
- NEW 定向 Core/App Server 集成测试，文件挂到仓库真实测试 suite

**接口：**
- `project_mission(snapshot, capability_snapshot, budget)->MissionProjection`
- `render_projection(previous, retained_fragment_state)->WorldStateSectionContribution`
- `capture_available_capabilities(step_snapshot)->CapabilitySnapshot`
- thread startup/resume 从 DB 恢复；ExtensionData 仅缓存。

**失败测试：** 同 turn 第 2 Step 能看到刚保存工件；compact 后目标/禁止项可見；Known snapshot 但正文不存在时重新渲染；模型/provider/项目切换无串上下文；大材料不被全塞 developer message；动态能力不可用立即撤下可用标志。

**关键验收：** 连续两次 tool 调用的真实模型请求或可验证输入快照必须不同且 revision 正确，不以“contributor 被注册”代替真的注入。

---

## WP04：一个真实任务的自主研究与创作

**文件：**
- NEW `ai-ip-runtime/src/{evidence.rs,evidence_tool.rs,research_adapter.rs}`
- NEW `ai-ip-store/migrations/002_evidence.sql`
- NEW `ai-ip-store/src/evidence.rs`
- MODIFY 当前搜索/文件工具观察接缝（先定位实际 ToolFinish/结果对象，不直接猜签名）
- NEW 小型业务测试文件夹 `tests/business_cases/`，普通 JSON/Markdown

**实现：**
- 真实搜索/读取返回结果捕获为来源，限定 coverage。
- Lead 读取材料、选方法、生成 artifact；不强制研究→方向→草稿。
- 有预算许可后分别用真实国内 profile 验证，不自动调用 GPT-6 修稿。

**失败测试：** snippet 不被记为完整视频；模型传 URL+“verified”不能自证；来源中注入命令不改变授权；无工具可达不谎称研究完成。

**业务验证：** 一个材料充分的局部改稿任务、一个确实需补证据的陌生任务。二者不要求相同工具轨迹。记录首次稿和客户介入。

**停止条件：** 仍不能获得可用正文时，不转去建设几十个平台或完整 UI；先用本包证据定位任务理解、资料供给或模型质量问题。

---

## WP05：Core 完成接缝与有界质量审核

**文件：**
- NEW `ext/extension-api/src/contributors/completion.rs`
- MODIFY contributors 导出及 registry/builder/getter 所在实际文件
- NEW `core/src/session/completion.rs` 与定向 tests
- MODIFY `core/src/session/turn.rs`
- MODIFY `core/src/stream_events_utils.rs` 和真实 delta 分派位置
- NEW `ai-ip-runtime/src/{completion.rs,review.rs,release.rs}`
- NEW `ai-ip-store/migrations/003_review_budget.sql`
- NEW `ai-ip-store/src/{release.rs,budget.rs}`

**接口：** 采用 `contracts/completion_api.rs` 的目标签名，以现有 `ExtensionFuture` 适配。Core 对象不外泄，审核端口只有 `review_once`，不创建独立 tool loop。

**必须完成：**
1. 所有营销模式的最终输出进入完成接缝；无 mission 也需判断适用性，实质任务不能因没调 work 工具免检。正式 candidate 与 mission/input epoch/artifacts 绑定。
2. 先机械检查，再一个有界国内模型审核；审核输出严格解析。
3. Continue 把缺口交给当前 Lead；Wait/Block 保留草稿且停止自动续跑。
4. 取消、预算、用户新输入、Stop hook 与质量结果统一合并。
5. release 事务最终再次验证 revision/digest，不用旧审核放行新稿。
6. 正式交付与 provisional streaming 分离；兼容客户端协商。

**失败测试：** 审核过程中用户改目标；审核模型异常；相同缺陷循环；模型漏填 claims 但正文有错误事实；合法虚构不被拒；工具直接输出虚假发布状态不能放行；旧客户端不得显示被拒内容为正式成果；Stop block 无反馈报错而非无限继续。

**回归：** 非营销模式的原生 Codex 会话不受新业务机制影响；营销模式无 mission 的闲聊经轻量 NotApplicable 后正常结束；已有 hooks 和目标机制不能出现双重续跑；cancel latency 不因审核阻塞。

---

## WP06：内容闭环业务验收与有限方法库

**文件：**
- MODIFY/整理 `ai-ip-assets/skills/` 中当前有效业务方法
- NEW 普通文件 `tests/business_cases/{development,heldout}/`
- NEW `scripts/ai_ip/smoke/paired_business_runs.ts`（轻量，不新建平台）
- NEW `docs/verification/` 原始稿、评阅与结论索引

**任务：** 拆开生产身份和 `HeldOutMissionCase` 的历史评测 prompt；普通聊天不用 ContentPackage JSON；方法按需读取，有不适用条件。

**对照：** 同模型同底座通用 Agent vs 系统 Agent。主力模型分别验证；GPT-6 参考单列、不暗中改 B 稿。

**验收：** 先看首次可采用程度、事实与制作问题、客户介入、耗时/成本。以完整作品为单位，所有原始稿保留，不用“测试全绿”宣布作品优质。

---

## WP07：媒体理解、制作与成片 QA

**文件：**
- NEW `ai-ip-domain/src/media.rs`
- NEW `ai-ip-runtime/src/media/{mod.rs,understand.rs,render.rs,qa.rs}`
- NEW 供应商适配器文件，只按已确认 API 建立
- NEW 媒体任务与内容 hash 的集成测试

**接口：** `MediaAdapter::estimate/submit/poll/cancel/fetch_result`；能力限制来自实测，不填猜测版本号。

**测试：** 无音频能力不宣称听过声音；源素材失效；供应商超时后扣费未知；取消不保证远端任务取消；渲染成功但静音/黑帧；字幕与稿件不一致；最终 render 改动后原 QA 失效。

**验收：** 一份真实请求到真实可播放媒体，权利、成本、参数、QA 与局限可追溯。模拟媒体不能替代。

---

## WP08：业务动作授权、预算与第一个平台

**文件：**
- NEW `ai-ip-domain/src/{approval.rs,execution.rs}`
- NEW `ai-ip-store/migrations/004_execution.sql`
- NEW `ai-ip-store/src/{approval.rs,execution.rs}`
- NEW `ai-ip-runtime/src/{action_tool.rs,execution_service.rs}`
- NEW `ai-ip-runtime/src/platforms/` 下第一个真实平台适配器
- MODIFY App Server 协议/可信审批 handler

**实现：** prepare 冻结内容，可信 UI 决定产生 approval，短事务预留费用并创建 execution，网络提交，回执/unknown 对账。用过期/撤销/版本/账号绑定阻止误执行。

**安全测试：** 普通 shell/脚本/登录浏览器能否绕过审批，必须实测；能绕过则不能发布“受控外部动作”版本。

**验收：** 仅在专用测试账号与明确授权下完成一次发布/回读。没有平台权限时交付接口和测试证据，标记平台接入未完成，不用 mock 解锁能力目录。

---

## WP09：回读指标与有来源的学习

**文件：**
- NEW `ai-ip-runtime/src/{observations.rs,learning.rs}`
- NEW 平台 read_results adapter 与指标口径测试
- MODIFY decision/memory 投影

**测试：** 观测窗口/时区错误；零分母；重复平台结果；用户手填与平台数据混淆；少量高播放被写成因果规律；跨项目记忆泄漏。

**验收：** 至少一次真实已有内容的指标回读，Lead 形成有来源、有不确定性的调整建议；不是保证增长。

---

## WP10：正式客户端、更新与客户试用

**新增代码位置：** 产品客户端放独立 `apps/` 子目录，与 Rust runtime 通过版本化协议连接；具体桌面打包器必须在目标 OS/现有前端约束检查后锁版本，不让本规格猜未核验的系统支持。

**客户端必须有：** Chat、任务/作品侧栏、provisional/正式成果区别、授权卡、取消/暂停/恢复、预算提示、断线追赶、素材导入/导出、可理解的错误。

**打包安全：** 私有状态路径、OS 密钥存储、子进程环境清理、签名、版本清单、migration backup、回滚及客户数据导出/删除。不要把 App Server 无认证 WebSocket 开到公网。

**验收：** 目标 Windows/macOS 各自真实设备验证。开发机能跑不等于两平台均发布就绪；无法验收的架构/系统不写支持。

---

## 工作包提交模板

```text
工作包：WPxx
基线/结束SHA：
修改职责：
触及 Core 接缝：
新增/改变的外部合同：
先失败的测试：
运行的命令与结果：
真实模型/工具证据：
首次业务成果与人工介入：
未验证项：
数据/授权/费用风险：
回滚操作：
下一步允许施工范围：
```

禁止填“全部完成”而省略真实模型调用、客户采用与外部平台验收的边界。
