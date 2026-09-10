# 来源与核对范围

核对提交：`e08f1e1ad5bbc3ea036aa271894d8228e499ed61`。以下是本次通过 GitHub 连接直接读取的文件/范围。除特别注明，不代表逐行审计了整个仓库，也没有在用户机器运行产品。

GitHub 固定前缀：
`https://github.com/yangshengguo46-stack/marketing-system-v7/blob/e08f1e1ad5bbc3ea036aa271894d8228e499ed61/`

| 编号 | 文件/范围 | 本方案使用它确认什么 |
|---|---|---|
| S01 | `output/business-methods/产品交付与开发验证约定.md` 全文 | 开箱可用、作品质量、国内主力生产模型、GPT-6研发参考、同模型比较、禁止庞大研发平台 |
| S02 | `docs/architecture/2026-08-25-legacy-six-version-decision-memory.md` 1–230 行及本对话既有审计 | 前六轮边界；V1–V3是历史重建，不能当六个正式独立完整版本 |
| S03 | `docs/architecture/2026-09-10-gpt6-plan-alignment.md` 20–40 行，本对话此前读取的完整对齐内容 | 已删除评测设施、未构建产品部分；文中同步/旧SHA陈述存在历史时点问题，不用于覆盖现HEAD |
| S04 | `codex-rs/ai-ip-runtime/src/prompt.rs`、`lib.rs` 与 runtime 目录（本对话前文直接读取） | 评测 prompt 与生产身份不能混淆；现有文件组成 |
| S05 | `codex-rs/ext/extension-api/src/contributors.rs` 1–210；`contributors/world_state.rs` 全文 | contribute_world_state、lifecycle、PreviousWorldStateSection、retained fragment 和角色机制 |
| S06 | `codex-rs/core/src/session/world_state.rs` 全文 | build_world_state_for_step 已遍历并加入扩展 section |
| S07 | `codex-rs/core/src/session/turn.rs` 520–770 | Stop hook 在何处运行、空feedback block会被忽略、原生继续/结束语义 |
| S08 | `codex-rs/ai-ip-runtime/src/work_chain.rs`（前文读取的当前分支） | 三阶段顺序型工作单、record 无前驱阶段强制检查 |
| S09 | `codex-rs/ai-ip-runtime/src/work_tool.rs`（前文读取的当前分支） | 原生工具安装、文件持久化、阶段诱导文本与输入限制 |
| S10 | `codex-rs/ai-ip-domain/src/content_package.rs` 1–170；`lib.rs` 全文 | 六类ClaimStatus、Readiness、受众ActionFunnelStep与现有ContentPackage |
| S11 | `codex-rs/ext/web-search/src/tool.rs` 1–230 | 原生web工具使用provider/auth与SearchClient；存在工具不等于所有账户可用 |
| S12 | `codex-rs/app-server/src/extensions.rs` 1–200 | 当前实际install与依赖装配、goal/skills/web原生注册 |
| S13 | OpenAI Codex App Server官方说明：`https://developers.openai.com/codex/app-server/`，访问时重定向 `https://learn.chatgpt.com/docs/app-server` | stdio、协议类型生成与实验性WebSocket边界；具体参数仍以自有编译版本为准 |
| S14 | SQLite foreign keys：`https://sqlite.org/foreignkeys.html` | 每连接显式开启外键、复合外键约束 |
| S15 | SQLite WAL：`https://sqlite.org/wal.html` | 同机WAL、并发与备份注意事项 |
| S16 | SQLite transactions：`https://sqlite.org/lang_transaction.html` | 事务与写入竞争，不在数据库事务里等待网络 |
| S17 | `codex-rs/core/src/session/turn.rs` 1–110 | 实际使用stream_events_utils与delta事件类型，具体流式修改需沿调用图定位 |

## 额外核对和边界

- 本次 GitHub API 的默认分支 HEAD 返回上述 SHA。新方案应先检查本地 HEAD，不能强制覆盖用户后续提交。
- 用户提供的 `https://chatgpt.com/share/6aa2724f-b848-83e8-bf88-30521c758869` 本次直接抓取未成功。以前面可见对话、仓库归档与已确认产品约定为依据；没有声称该分享页已逐字复核或与仓库另一分享链接完全相同。
- 搜索未获得足以固定 SeedEvolving 生产 API ID 的官方证据，因此没有编造型号、价格或权限。GLM 5.2也按真实账户profile验证，而非凭产品显示名直接接入。
- 新接口、新表结构、新配置键和新协议方法是本交付包设计，不是当前Codex已有API。
- 本交付包的验证是 SQLite/JSON/TOML 合同原型测试。未运行 Rust workspace 编译，未调用付费生产模型，未执行发布、媒体生成、平台授权或客户数据迁移。
