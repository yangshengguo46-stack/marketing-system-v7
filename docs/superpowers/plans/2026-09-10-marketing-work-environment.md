# 营销工作环境与路径自主 Implementation Plan

> **⚠️ NOT EXECUTABLE — 已知缺陷待修（2026-09-10）。**
>
> 本计划写于底座能力盘点之前。盘点后确认下面六处与 `codex-rs` 实际行为不符，**按现状执行会得到错误实现**。修正前不要开始执行。
>
> | # | 本计划写的 | 实际 | 影响 |
> |---|---|---|---|
> | 1 | 状态存 `thread_store` | `ExtensionData` 是纯内存 `Mutex<HashMap>`，文档明说 "does not provide persistence"；线程重开即空 | **设计缺陷**：重开后首轮无上下文 |
> | 2 | `contribute_turn_context` 的 `PromptSlot` 决定渲染位置 | 该方法的 slot **被忽略**，一律进 `developer_sections`（`core/src/session/mod.rs:4101-4130`） | 断言与语义不符 |
> | 3 | 注入"完整未截断正文" | `MAX_ADDITIONAL_CONTEXT_VALUE_TOKENS = 1_000` 且 `truncate_middle_with_token_budget` 中间截断 | **设计缺陷**：长正文会被砍 |
> | 4 | 用 `ContextContributor` 做注入 | `WorldState`（`core/src/session/world_state.rs`）每步重建、压缩免疫，才是正确机制 | 选错机制：压缩后约束丢失 |
> | 5 | "不新增营销工具" | 底座 `web.run` 已完整覆盖 `search_sources` / `open_source`（含 `ref_id`+`lineno` 引用锚点） | 漏项 |
> | 6 | 未提任务级验收 | Stop hook 的 `StopOutcome{should_block}`（`core/src/session/turn.rs:623`）即"不许停"，配置即用 | 漏项 |
>
> 详细盘点见 `docs/architecture/2026-09-10-gpt6-plan-alignment.md` §1.1。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让营销 Lead 在每一轮都看得见自己的工作任务状态，并自己决定下一步——不再被固定三阶段和预设任务文本牵着走。

**Architecture:** 用上游 `codex-extension-api` 的 `ContextContributor` 在每轮推理前注入营销上下文；工作状态由现有 `marketing_work` 工具在 open/record 时镜像进 `thread_store`，贡献者从同一 store 读取。顺序约束从 `work_chain` 的强制降为建议。**不修改 `codex-rs/core/`，不修改 `codex-rs/app-server-protocol/`。**

**Tech Stack:** Rust 2024 workspace、`codex-extension-api`（`ContextContributor` / `ToolContributor` / `ExtensionData`）、`cargo-nextest`、Bazel lock。

**Spec:** `docs/architecture/2026-09-10-gpt6-plan-alignment.md`（来源与四冲突判定）、`docs/architecture/2026-08-25-legacy-six-version-decision-memory.md`（LG1/LG3/LG6）、`output/business-methods/产品交付与开发验证约定.md`（北极星）。

## Global Constraints

- 上游 base 固定 `bf5ebd98c567931d82e873a4afdac7548bd85979`；不得引入上游新提交。
- **`codex-rs/core/**` 与 `codex-rs/app-server-protocol/**` 改动必须为零。** 本计划只使用上游已暴露的扩展接缝。
- 只允许修改 `codex-rs/ai-ip-runtime/**` 与 `codex-rs/app-server/src/extensions.rs`（仅注册行）。
- 工具对外契约不变：工具名仍是 `marketing_work`，参数 schema 不改，工作文件路径仍是 `output/marketing-work/<workId>.json`。
- 测试一律 `just test -p <crate>`，不得用 `cargo test`；不得运行全 workspace 测试。
- 每步先写失败测试。提交信息用 `feat:` / `fix:` / `test:` 前缀。
- 凡改动 Rust 依赖或 `BUILD.bazel`，必须跑 `just bazel-lock-update`。
- 本计划不调用任何付费模型、不生成媒体、不发布。

---

### Task 1: 可共享的营销工作状态

工作状态必须能被工具（写）和上下文贡献者（读）同时持有。上游 `ExtensionData` 提供 `get_or_init::<T>() -> Arc<T>`，因此状态类型需要内部可变性。

**Files:**
- Create: `codex-rs/ai-ip-runtime/src/work_state.rs`
- Modify: `codex-rs/ai-ip-runtime/src/lib.rs`（增加 `mod work_state;` 与 `pub use`）
- Test: `codex-rs/ai-ip-runtime/src/work_state_tests.rs`

**Interfaces:**
- Consumes: `crate::work_chain::WorkChain`
- Produces:
  - `pub struct MarketingWorkState`（`Default`）
  - `MarketingWorkState::set(&self, work_id: String, work: WorkChain)` → `()`
  - `MarketingWorkState::snapshot(&self) -> Option<(String, WorkChain)>`（`WorkChain: Clone` 需要存在）
  - `MarketingWorkState::clear(&self)` → `()`

`work_id` 与 `work` 一起保存：`workId` 不在 `WorkChain` 里，而上下文贡献者需要它来拼工作文件路径（`output/marketing-work/<workId>.json`）。

- [ ] **Step 1: 写失败测试**

创建 `codex-rs/ai-ip-runtime/src/work_state_tests.rs`：

```rust
use crate::WorkChain;
use crate::work_state::MarketingWorkState;

#[test]
fn new_state_has_no_snapshot() {
    let state = MarketingWorkState::default();
    assert!(state.snapshot().is_none());
}

#[test]
fn set_then_snapshot_returns_the_recorded_work_id_and_work() {
    let state = MarketingWorkState::default();
    let mut work = WorkChain::new("brief text".to_string(), vec!["materials/a.md".to_string()]);
    work.record(crate::work_chain::Stage::Research, "research body".to_string());

    state.set("gift-first-work".to_string(), work);

    let (work_id, work) = state.snapshot().expect("snapshot after set");
    assert_eq!(work_id, "gift-first-work");
    assert_eq!(work.brief, "brief text");
    assert_eq!(work.materials, vec!["materials/a.md".to_string()]);
    assert_eq!(
        work.results[0].as_ref().map(|result| result.body.as_str()),
        Some("research body")
    );
}

#[test]
fn clear_removes_the_snapshot() {
    let state = MarketingWorkState::default();
    state.set("work".to_string(), WorkChain::new("brief".to_string(), Vec::new()));

    state.clear();

    assert!(state.snapshot().is_none());
}

#[test]
fn later_set_replaces_the_earlier_snapshot() {
    let state = MarketingWorkState::default();
    state.set("first".to_string(), WorkChain::new("first brief".to_string(), Vec::new()));
    state.set("second".to_string(), WorkChain::new("second brief".to_string(), Vec::new()));

    let (work_id, work) = state.snapshot().expect("snapshot");
    assert_eq!(work_id, "second");
    assert_eq!(work.brief, "second brief");
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `just test -p codex-ai-ip-runtime -E 'test(work_state)'`
Expected: 编译失败，`unresolved import crate::work_state` / `crate::WorkChain`。

- [ ] **Step 3: 最小实现**

创建 `codex-rs/ai-ip-runtime/src/work_state.rs`：

```rust
use std::sync::Mutex;

use crate::work_chain::WorkChain;

/// Thread-scoped mirror of the newest saved `WorkChain`, shared between the
/// `marketing_work` tool (writer) and the context contributor (reader).
///
/// The work file on disk remains the source of truth. This mirror exists so a
/// turn can be assembled before any tool call happens.
#[derive(Debug, Default)]
pub struct MarketingWorkState {
    work: Mutex<Option<(String, WorkChain)>>,
}

impl MarketingWorkState {
    pub fn set(&self, work_id: String, work: WorkChain) {
        *self.work.lock().expect("work state lock") = Some((work_id, work));
    }

    pub fn snapshot(&self) -> Option<(String, WorkChain)> {
        self.work.lock().expect("work state lock").clone()
    }

    pub fn clear(&self) {
        *self.work.lock().expect("work state lock") = None;
    }
}
```

在 `lib.rs` 里增加导出（放在现有 `mod`/`pub use` 区块，保持字母序）：

```rust
mod work_state;
pub use work_state::MarketingWorkState;
```

`WorkChain` 当前未派生 `Clone`。在 `work_chain.rs` 的 `#[derive(...)]` 列表加上 `Clone`（`Stage`、`StageResult`、`WorkChain` 三处都需要，因为 `WorkChain` 含 `[Option<StageResult>; 3]`）。

- [ ] **Step 4: 跑测试确认通过**

Run: `just test -p codex-ai-ip-runtime -E 'test(work_state)'`
Expected: 4 passed。

- [ ] **Step 5: 提交**

```bash
git add codex-rs/ai-ip-runtime/src/work_state.rs codex-rs/ai-ip-runtime/src/work_state_tests.rs codex-rs/ai-ip-runtime/src/work_chain.rs codex-rs/ai-ip-runtime/src/lib.rs
git commit -m "feat: add thread-scoped marketing work state"
```

---

### Task 2: 工具把工作状态镜像进 thread store

`ToolContributor::tools` 能拿到 `thread_store`，在那里 `get_or_init` 出状态并交给执行器。执行器在每次 open/record 成功后更新它。

**Files:**
- Modify: `codex-rs/ai-ip-runtime/src/work_tool.rs`
- Test: `codex-rs/ai-ip-runtime/src/work_tool_tests.rs`（现有文件，追加）

**Interfaces:**
- Consumes: `MarketingWorkState`（Task 1）
- Produces: 执行器结构体 `MarketingWorkTool { state: Arc<MarketingWorkState> }`；`MarketingWork` 保留为 `ToolContributor`

- [ ] **Step 1: 写失败测试**

在 `codex-rs/ai-ip-runtime/src/work_tool_tests.rs` 追加：

```rust
use std::sync::Arc;

use codex_extension_api::ExtensionData;

use crate::work_state::MarketingWorkState;
use crate::work_tool::MarketingWork;

#[test]
fn tools_registration_seeds_the_work_state_in_the_thread_store() {
    let session_store = ExtensionData::new("session");
    let thread_store = ExtensionData::new("thread");
    let contributor = MarketingWork;

    let executors = contributor.tools(&session_store, &thread_store);

    assert_eq!(executors.len(), 1);
    assert!(
        thread_store.get::<MarketingWorkState>().is_some(),
        "tools() must seed MarketingWorkState so the context contributor can read it"
    );
}

#[test]
fn a_second_tools_call_reuses_the_same_state_instance() {
    let session_store = ExtensionData::new("session");
    let thread_store = ExtensionData::new("thread");
    let contributor = MarketingWork;

    let _first = contributor.tools(&session_store, &thread_store);
    let seeded = thread_store
        .get::<MarketingWorkState>()
        .expect("state seeded by first call");
    seeded.set("kept".to_string(), crate::WorkChain::new("brief".to_string(), Vec::new()));

    let _second = contributor.tools(&session_store, &thread_store);

    let after = thread_store
        .get::<MarketingWorkState>()
        .expect("state present after second call");
    assert_eq!(
        after.snapshot().expect("snapshot").0,
        "kept",
        "a second tools() call must not replace the existing state"
    );
    assert!(Arc::ptr_eq(&seeded, &after));
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `just test -p codex-ai-ip-runtime -E 'test(tools_registration) | test(a_second_tools_call)'`
Expected: FAIL —— `get::<MarketingWorkState>()` 返回 `None`，因为当前 `tools()` 不写 store。

- [ ] **Step 3: 最小实现**

在 `work_tool.rs` 中：

```rust
use crate::work_state::MarketingWorkState;

struct MarketingWork;

pub fn install<C: Sync + 'static>(builder: &mut ExtensionRegistryBuilder<C>) {
    builder.tool_contributor(Arc::new(MarketingWork));
}

impl ToolContributor for MarketingWork {
    fn tools(
        &self,
        _: &ExtensionData,
        thread_store: &ExtensionData,
    ) -> Vec<Arc<dyn for<'call> ToolExecutor<ToolCall<'call>>>> {
        let state = thread_store.get_or_init::<MarketingWorkState>(Default::default);
        vec![Arc::new(MarketingWorkTool { state })]
    }
}

struct MarketingWorkTool {
    state: Arc<MarketingWorkState>,
}

impl<'call> ToolExecutor<ToolCall<'call>> for MarketingWorkTool {
    // tool_name / spec 与现状逐字相同，不改
    fn handle<'a>(&'a self, call: ToolCall<'call>) -> ToolExecutorFuture<'a>
    where
        'call: 'a,
    {
        Box::pin(async move {
            // ... 现有 handle 主体逐字保留 ...
            // 在 `if changed {` 写入文件成功之后、构造 result 之前，追加：
            self.state.set(args.work_id.clone(), work.clone());
            // ...
        })
    }
}
```

**注意**：`handle` 里 `work` 在写文件前仍归本地所有；`self.state.set(work.clone())` 必须放在 `fs.write_file(...)` 成功之后，使镜像只在文件确实落盘时更新。`JsonToolOutput` 的构造与错误映射逐字不变。

- [ ] **Step 4: 跑测试确认通过**

Run: `just test -p codex-ai-ip-runtime`
Expected: 全部通过（Task 1 的 4 个 + 本任务 2 个 + 既有测试）。

- [ ] **Step 5: 提交**

```bash
git add codex-rs/ai-ip-runtime/src/work_tool.rs codex-rs/ai-ip-runtime/src/work_tool_tests.rs
git commit -m "feat: mirror saved marketing work into the thread store"
```

---

### Task 3: 每轮注入营销上下文

**Files:**
- Create: `codex-rs/ai-ip-runtime/src/context.rs`
- Modify: `codex-rs/ai-ip-runtime/src/lib.rs`
- Test: `codex-rs/ai-ip-runtime/src/context_tests.rs`

**Interfaces:**
- Consumes: `MarketingWorkState`（Task 1）、`WorkChain` / `Stage`（既有）
- Produces:
  - `pub const MARKETING_CONTEXT_KIND: &str = "marketing.work_state";`
  - `pub const MARKETING_POLICY_KIND: &str = "marketing.judgment_policy";`
  - `pub struct MarketingContextContributor`
  - `pub fn marketing_policy_fragment() -> PromptFragment`
  - `pub fn work_state_fragment(work: &WorkChain, file: &str) -> PromptFragment`

- [ ] **Step 1: 写失败测试**

创建 `codex-rs/ai-ip-runtime/src/context_tests.rs`：

```rust
use codex_extension_api::PromptSlot;

use crate::WorkChain;
use crate::context::MARKETING_CONTEXT_KIND;
use crate::context::MARKETING_POLICY_KIND;
use crate::context::marketing_policy_fragment;
use crate::context::work_state_fragment;
use crate::work_chain::Stage;

#[test]
fn policy_fragment_states_that_the_lead_chooses_the_path() {
    let fragment = marketing_policy_fragment();

    assert_eq!(fragment.slot(), PromptSlot::DeveloperPolicy);
    assert_eq!(fragment.content_kind().0, MARKETING_POLICY_KIND);
    let text = fragment.text();
    assert!(text.contains("choose the next step"), "policy must grant path choice");
    assert!(text.contains("three stages"), "policy must say the stages are not mandatory");
    assert!(text.contains("unknown"), "policy must require unknowns stay unknown");
}

#[test]
fn work_state_fragment_carries_brief_stage_and_open_questions() {
    let mut work = WorkChain::new(
        "卖黄金礼品，想讲人情".to_string(),
        vec!["materials/brand.md".to_string()],
    );
    work.record(Stage::Research, "已确认：品牌无自有工厂".to_string());

    let fragment = work_state_fragment(&work, "output/marketing-work/gift.json");

    assert_eq!(fragment.slot(), PromptSlot::ContextWindow);
    assert_eq!(fragment.content_kind().0, MARKETING_CONTEXT_KIND);
    let text = fragment.text();
    assert!(text.contains("卖黄金礼品，想讲人情"), "must carry the brief");
    assert!(text.contains("output/marketing-work/gift.json"), "must carry the work file path");
    assert!(text.contains("materials/brand.md"), "must carry the material paths");
    assert!(text.contains("research"), "must name the completed stage");
    assert!(text.contains("已确认：品牌无自有工厂"), "must carry the recorded body");
    assert!(text.contains("direction"), "must show a stage that is still open");
}

#[test]
fn work_state_fragment_never_truncates_the_recorded_body() {
    let body = "证".repeat(3000);
    let mut work = WorkChain::new("brief".to_string(), Vec::new());
    work.record(Stage::Research, body.clone());

    let fragment = work_state_fragment(&work, "output/marketing-work/x.json");

    assert!(
        fragment.text().contains(&body),
        "the whole recorded body must reach the model; previews are what the model mistook for reading"
    );
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `just test -p codex-ai-ip-runtime -E 'test(policy_fragment) | test(work_state_fragment)'`
Expected: 编译失败，`unresolved import crate::context`。

- [ ] **Step 3: 最小实现**

创建 `codex-rs/ai-ip-runtime/src/context.rs`：

```rust
use codex_extension_api::ContentItemKind;
use codex_extension_api::ContextContributor;
use codex_extension_api::ExtensionData;
use codex_extension_api::ExtensionFuture;
use codex_extension_api::PromptFragment;
use codex_extension_api::PromptSlot;
use codex_extension_api::TurnContextContributionInput;

use crate::work_chain::Stage;
use crate::work_state::MarketingWorkState;

pub const MARKETING_CONTEXT_KIND: &str = "marketing.work_state";
pub const MARKETING_POLICY_KIND: &str = "marketing.judgment_policy";

const POLICY_TEXT: &str = "\
This session does marketing work for one business objective.
- You choose the next step. The three stages (research, direction, draft) are places to keep work, not a required order. Skip, repeat, or work outside them when the task calls for it.
- Keep user-provided facts, external evidence, model interpretation, creative hypotheses, unknowns, and real results distinct. Never promote a hypothesis to a fact.
- When evidence is missing, keep the most useful draft you can and state what stays unknown. Do not invent business facts, capabilities, customers, or results.
- Stop and report when the work is done, when a decisive input is missing, when an external action needs authorization, or when further work would add nothing.";

pub fn marketing_policy_fragment() -> PromptFragment {
    PromptFragment::developer_policy(
        POLICY_TEXT,
        ContentItemKind(MARKETING_POLICY_KIND.to_string()),
    )
}

pub fn work_state_fragment(work: &WorkChain, file: &str) -> PromptFragment {
    let mut lines = vec![
        format!("Work file: {file}"),
        format!("Objective: {}", work.brief),
    ];
    if !work.materials.is_empty() {
        lines.push(format!("Materials: {}", work.materials.join(", ")));
    }
    for (index, stage) in [Stage::Research, Stage::Direction, Stage::Draft]
        .into_iter()
        .enumerate()
    {
        let name = match stage {
            Stage::Research => "research",
            Stage::Direction => "direction",
            Stage::Draft => "draft",
        };
        match work.results[index].as_ref() {
            Some(result) => lines.push(format!(
                "Saved {name}{}:\n{}",
                if result.needs_review { " (needs review)" } else { "" },
                result.body
            )),
            None => lines.push(format!("Not yet saved: {name}")),
        }
    }
    PromptFragment::new(
        PromptSlot::ContextWindow,
        lines.join("\n"),
        ContentItemKind(MARKETING_CONTEXT_KIND.to_string()),
    )
}

#[derive(Debug, Default)]
pub struct MarketingContextContributor;

impl ContextContributor for MarketingContextContributor {
    fn contribute_thread_context<'a>(
        &'a self,
        _session_store: &'a ExtensionData,
        _thread_store: &'a ExtensionData,
    ) -> ExtensionFuture<'a, Vec<PromptFragment>> {
        Box::pin(async move { vec![marketing_policy_fragment()] })
    }

    fn contribute_turn_context<'a>(
        &'a self,
        input: TurnContextContributionInput<'a>,
    ) -> ExtensionFuture<'a, Vec<PromptFragment>> {
        Box::pin(async move {
            let Some(state) = input.thread_store.get::<MarketingWorkState>() else {
                return Vec::new();
            };
            let Some(work) = state.snapshot() else {
                return Vec::new();
            };
            vec![work_state_fragment(&work, &work_file_path(&work))]
        })
    }
}
```

`work_file_path` 是纯函数，单独放在 `context.rs` 里：

```rust
pub fn work_file_path(work_id: &str) -> String {
    format!("output/marketing-work/{work_id}.json")
}
```

`MarketingWorkState::snapshot()` 返回 `(String, WorkChain)`（Task 1 已定义该签名），调用处解构：

```rust
let Some((work_id, work)) = state.snapshot() else {
    return Vec::new();
};
vec![work_state_fragment(&work, &work_file_path(&work_id))]
```

- [ ] **Step 4: 跑测试确认通过**

Run: `just test -p codex-ai-ip-runtime`
Expected: 全部通过。

- [ ] **Step 5: 提交**

```bash
git add codex-rs/ai-ip-runtime/src/context.rs codex-rs/ai-ip-runtime/src/context_tests.rs codex-rs/ai-ip-runtime/src/lib.rs codex-rs/ai-ip-runtime/src/work_state.rs codex-rs/ai-ip-runtime/src/work_state_tests.rs
git commit -m "feat: contribute marketing policy and work state to the prompt"
```

---

### Task 4: 注册贡献者（含 App Server 集成验证）

**Files:**
- Modify: `codex-rs/ai-ip-runtime/src/work_tool.rs`（`install` 内增加一行注册）
- Test: `codex-rs/app-server/tests/suite/v2/ai_ip_work_chain.rs`（追加一个测试）

**Interfaces:**
- Consumes: `MarketingContextContributor`（Task 3）
- Produces: `install()` 同时注册工具贡献者与 prompt 贡献者

- [ ] **Step 1: 写失败测试**

在 `codex-rs/app-server/tests/suite/v2/ai_ip_work_chain.rs` 追加。断言只用两条，避免依赖"每轮贡献一次还是每次模型请求贡献一次"：

```rust
#[tokio::test]
async fn marketing_policy_and_saved_work_reach_the_model() -> Result<()> {
    skip_if_wine_exec!(
        Ok(()),
        "Native ToolEnvironment currently omits foreign-platform cwd values"
    );
    let server = responses::start_mock_server().await;
    let body = format!(
        "{}CONTEXT_FULL_BODY_MARKER",
        "已确认：品牌无自有工厂，主要客户是企业采购。".repeat(20)
    );
    let events = vec![
        responses::sse(vec![
            responses::ev_function_call(
                "open-context",
                "marketing_work",
                &json!({"action":"open","workId":"ctx","brief":"卖黄金礼品，想讲人情"}).to_string(),
            ),
            responses::ev_completed("open-context"),
        ]),
        responses::sse(vec![
            responses::ev_function_call(
                "record-context",
                "marketing_work",
                &json!({"action":"record","workId":"ctx","stage":"research","body":body})
                    .to_string(),
            ),
            responses::ev_completed("record-context"),
        ]),
        // A later turn that calls no tool at all: the injected context must persist.
        responses::sse(vec![
            responses::ev_assistant_message("plain-context", "Noted"),
            responses::ev_completed("plain-context"),
        ]),
        responses::sse(vec![
            responses::ev_assistant_message("plain-context-2", "Noted"),
            responses::ev_completed("plain-context-2"),
        ]),
    ];
    let mock = responses::mount_sse_sequence(&server, events).await;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri())
        .with_sandbox_mode("workspace-write")
        .write(codex_home.path())?;
    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(std::time::Duration::from_secs(60))
        .await?;
    let thread = app.start_thread(ThreadStartParams::default()).await?.thread;
    let _: TurnStartResponse = app
        .request(|request_id| codex_app_server_protocol::ClientRequest::TurnStart {
            request_id,
            params: TurnStartParams {
                thread_id: thread.id,
                input: vec![UserInput::Text {
                    text: "Complete this marketing mission".into(),
                    text_elements: vec![],
                }],
                ..Default::default()
            },
        })
        .await?;
    timeout(
        std::time::Duration::from_secs(60),
        app.read_notification::<TurnCompletedNotification>("turn/completed"),
    )
    .await??;

    let requests = mock.requests();
    let first = requests.first().expect("first model request");
    let last = requests.last().expect("last model request");
    // Thread-scoped policy is present before any work exists.
    assert!(first.body_contains_text("You choose the next step"));
    assert!(!first.body_contains_text("CONTEXT_FULL_BODY_MARKER"));
    // Turn-scoped state carries the objective and the complete, untruncated body.
    assert!(last.body_contains_text("You choose the next step"));
    assert!(last.body_contains_text("卖黄金礼品，想讲人情"));
    assert!(last.body_contains_text("CONTEXT_FULL_BODY_MARKER"));
    assert!(
        last.body_contains_text(&body),
        "the whole recorded body must reach the model, not a 90-byte preview"
    );
    Ok(())
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `just test -p codex-app-server --test all -E 'test(marketing_policy_and_saved_work)' --retries 0 --test-threads 1`
Expected: FAIL —— 请求体内缺少 policy 与 body 标记。

- [ ] **Step 3: 最小实现**

在 `work_tool.rs` 的 `install` 中：

```rust
pub fn install<C: Sync + 'static>(builder: &mut ExtensionRegistryBuilder<C>) {
    builder.tool_contributor(Arc::new(MarketingWork));
    builder.prompt_contributor(Arc::new(crate::context::MarketingContextContributor));
}
```

需要 `use codex_extension_api::...` 中已有的 `Arc`；`prompt_contributor` 接受 `Arc<dyn ContextContributor>`。

- [ ] **Step 4: 跑测试确认通过**

Run: `just test -p codex-app-server --test all -E 'test(ai_ip_work_chain) | test(ai_ip_strict_output)' --retries 0 --test-threads 1`
Expected: 既有的 2 个 + 新的 1 个，全部通过。回环测试需要 `NO_PROXY=127.0.0.1,localhost,::1`，只对测试进程生效。

- [ ] **Step 5: 收尾检查并提交**

```bash
just fix -p codex-ai-ip-runtime -p codex-app-server
just fmt
git diff --check
git add codex-rs/ai-ip-runtime/src/work_tool.rs codex-rs/app-server/tests/suite/v2/ai_ip_work_chain.rs
git commit -m "feat: register the marketing context contributor"
```

**注意**：`just fmt` 会格式化所有被 git 跟踪的文件，包括 `output/` 与 `research/`。执行后必须 `git checkout -- output research` 还原，除非本次确实要改那两个目录。

---

### Task 5: 把顺序约束降为建议

`work_chain` 现在用 `next_stage()` 强制"先 research 再 direction 再 draft"，并用 `work_order()` 给模型下达预设任务文本。这与 LG3「逻辑依赖，不是必须线性执行的流程」冲突，也是 W3/W4 机械跟随的直接来源。

**Files:**
- Modify: `codex-rs/ai-ip-runtime/src/work_chain.rs`
- Modify: `codex-rs/ai-ip-runtime/src/work_chain_tests.rs`
- Modify: `codex-rs/app-server/tests/suite/v2/ai_ip_work_chain.rs`（既有断言随契约变更同步改写）

**Interfaces:**
- Consumes: 无新增
- Produces: `WorkChain::suggested_stage(&self) -> Option<Stage>`（替代 `next_stage`）；`WorkChain::can_record(&self, stage: Stage) -> bool`；`WorkChain::state_report(&self, work_file: &str) -> Value`（替代 `work_order`）

**已确认会破坏的既有断言**（`app-server/tests/suite/v2/ai_ip_work_chain.rs`，本任务必须同步改写）：

- 第 167–179 行断言 `results[i]["nextStage"]` —— 新报告用 `suggestedStage`，且不再有 `nextStage`。
- 第 183–186 行断言 `reopened["results"][2] == {"preview":..., "needsReview":true}` —— 新报告用 `stages[]`，不再有 `results`/`preview`。
- 第 187–190 行断言 `results[1]["results"][0]["preview"]` 等于正文的前 90 字节 —— 预览被移除；完整正文改由上下文注入提供，该断言由 Task 4 的新测试覆盖。
- 第 192 行断言 `reopened.to_string().len() <= 900` —— 新报告不再有 900 字节上限约束（它只是状态，不含正文）。

改写后该测试仍须覆盖它原本证明的行为：写入落盘、跨线程重开、上游变更后下游标 `needsReview`、以及 `listener`/材料读取。只替换报告形状相关的断言，不削弱其余覆盖。

- [ ] **Step 1: 改写现有测试为期望的新行为**

先改 `work_chain_tests.rs`。

```rust
#[test]
fn a_fresh_work_may_enter_at_any_stage() {
    let work = WorkChain::new("brief".to_string(), Vec::new());
    // 三阶段都允许直接写入，没有任何一个被拒绝。
    assert_eq!(work.suggested_stage(), Some(Stage::Research));
    assert!(work.can_record(Stage::Draft));
    assert!(work.can_record(Stage::Direction));
}

#[test]
fn the_state_report_does_not_prescribe_the_work() {
    let mut work = WorkChain::new("brief".to_string(), Vec::new());
    work.record(Stage::Research, "body".to_string());

    let report = work.state_report("output/marketing-work/x.json");

    let task = report["task"].as_str().expect("task is a string");
    assert!(
        !task.contains("Read full brief/materials"),
        "the report must not hand the model a prescribed task"
    );
    assert!(
        task.contains("choose"),
        "the report must state that the model chooses the next step"
    );
    assert_eq!(report["stages"][0]["name"], "research");
    assert_eq!(report["stages"][0]["saved"], true);
    assert_eq!(report["stages"][1]["saved"], false);
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `just test -p codex-ai-ip-runtime -E 'test(a_fresh_work_may_enter_at_any_stage) | test(the_state_report_does_not_prescribe)'`
Expected: 编译失败——`can_record` / `suggested_stage` / `state_report` 尚不存在。

- [ ] **Step 3: 最小实现**

在 `work_chain.rs` 中：

```rust
impl WorkChain {
    /// Reports which stage is most likely useful next. Ordering is a hint:
    /// every stage stays writable at any time (LG3).
    pub fn suggested_stage(&self) -> Option<Stage> {
        [Stage::Research, Stage::Direction, Stage::Draft]
            .into_iter()
            .find(|stage| match &self.results[stage.index()] {
                Some(result) => result.needs_review,
                None => true,
            })
    }

    pub fn can_record(&self, _stage: Stage) -> bool {
        true
    }

    pub fn state_report(&self, work_file: &str) -> Value {
        json!({
            "workFile": work_file,
            "suggestedStage": self.suggested_stage(),
            "task": "Choose the next step that most reduces the biggest gap in this work. Saving a stage does not oblige you to continue in order. Return only the JSON object required by the output schema.",
            "stages": [Stage::Research, Stage::Direction, Stage::Draft]
                .into_iter()
                .map(|stage| json!({
                    "name": stage_name(stage),
                    "saved": self.results[stage.index()].is_some(),
                    "needsReview": self.results[stage.index()]
                        .as_ref()
                        .is_some_and(|result| result.needs_review),
                }))
                .collect::<Vec<_>>(),
        })
    }
}

fn stage_name(stage: Stage) -> &'static str {
    match stage {
        Stage::Research => "research",
        Stage::Direction => "direction",
        Stage::Draft => "draft",
    }
}
```

`record()` 的既有语义保持不变（写下游标 `needs_review`）。删除 `next_stage()` 与 `work_order()`；`work_tool.rs` 中 `work.work_order(&work_file)` 改为 `work.state_report(&work_file)`。

- [ ] **Step 4: 跑测试确认通过**

Run: `just test -p codex-ai-ip-runtime`
Expected: 全部通过。

- [ ] **Step 5: 同步改写 App Server 集成测试的既有断言**

`just test -p codex-app-server --test all -E 'test(ai_ip_work_chain)'` 现在必然失败。按本任务开头列出的四处逐条改写 `app-server/tests/suite/v2/ai_ip_work_chain.rs`，然后：

Run: `just test -p codex-app-server --test all -E 'test(ai_ip_work_chain) | test(ai_ip_strict_output)' --retries 0 --test-threads 1`
Expected: 3 passed（既有 2 个 + Task 4 新增 1 个）。回环测试需 `NO_PROXY=127.0.0.1,localhost,::1`，只对测试进程生效。

- [ ] **Step 6: 收尾检查并提交**

```bash
just fix -p codex-ai-ip-runtime -p codex-app-server
just fmt
git diff --check
git checkout -- output research
git add codex-rs/ai-ip-runtime/src/work_chain.rs codex-rs/ai-ip-runtime/src/work_chain_tests.rs codex-rs/ai-ip-runtime/src/work_tool.rs codex-rs/app-server/tests/suite/v2/ai_ip_work_chain.rs
git commit -m "refactor: make the marketing stage order advisory"
```

`git checkout -- output research` 是必须的：`just fmt` 会格式化所有被 git 跟踪的文件，包括那两个目录下的用户资料。

---

## 本计划不做的事

- 不建 Chat UI、不设计消息形态（用户判定为多余：产品本身就是 agent）。
- 不新增营销工具（现在 1 个，P3 按真实任务缺口再加）。
- 不建品牌工作空间、不建营销记忆层级（P2）。
- 不建动作预览/授权执行（P5）。
- 不建同模型对照（P6）。
- 不修改 `codex-rs/core/**`。若压缩后上下文丢失营销约束确实成为阻塞，再单独立项并单点突破。
- 不调用付费模型、不生成媒体、不发布。

## 验收

本计划完成的标志是**行为**，不是测试数：

1. 任意一轮发给模型的请求都包含当前目标、已保存阶段正文（完整、未截断）、材料路径与判断策略。
2. 模型可以在没有 research 的情况下直接写 direction 或 draft，且不会被工具拒绝。
3. 工具返回不再给模型下达预设任务文本。
4. 既有 `marketing_work` 契约不变：工具名、参数 schema、工作文件路径与格式逐字不变。

以上四条不构成作品质量通过，也不构成营销效果证据。作品质量仍需 P6 的真实交付与对照。
