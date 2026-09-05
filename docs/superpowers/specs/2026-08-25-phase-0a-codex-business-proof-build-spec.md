> **RETIRED — RETIRED_NOT_PASSED (2026-09-05).** This evaluator/proof document is historical and is not executable. All commands, prerequisite gates and successor authorizations below are retired by the approved business-first cleanup; prior results and failures remain historical claims only. Do not resume Phase 0A, 06A, 06B, 07A, 07B or LH1 from this record. Retirement grants no business PASS, model qualification, spending, publication, customer-data access or customer-release authority. Current authority: `docs/superpowers/specs/2026-09-05-business-first-cleanup-design.md` and `docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md`.

# Phase 0A — Codex 原生 Harness 业务证明 Build Specification

> **Document status:** 本文件是 Phase 0A 的规范性构建与验收合同，不是可直接执行的 implementation plan。不得把本文件交给 `superpowers:subagent-driven-development` 或 `superpowers:executing-plans`；只有总台账明确列出的 executable child plan 可以执行。下面的命令均为验收命令形状或接口示例，必须由后续 child plan 复核真实源码后拆成 2–5 分钟 RED/GREEN 步骤，不能直接复制执行。

**Phase outcome:** 锁定 Codex 来源，保存原样基线，并以一个全新真实案例证明最小 Lead Skill 能产生可审计的业务增益信号。

**System boundary:** 根 Thread 是唯一 Lead；Skills、tools、MCP 和临时 subagents 是按需能力；外部 evaluator 只通过原生 App Server V2 驱动 Codex。

**Locked stack:** OpenAI Codex `4ef1d4b89bd419c976b04fefa0fd36844e898340`，Rust 1.95.0，Python 3.11，Bazel，macOS x86_64，Windows 11 x64。

**国产模型决策边界：** 本规格同时受 `docs/superpowers/specs/2026-08-25-domestic-model-adaptation-design.md` 约束。Phase 0A 的 `providerRole=targetVolcengine|approvedReference` 只是本次 paired proof 的证据角色：`targetVolcengine` 保留为首条火山业务链标签，不能进入 Mission/Artifact/Lead 领域模型。第二国产 provider 的 G4p 属于 Plan 06 接缝稳定后的非阻塞 onboarding proof，不进入本规格，也不得把下述 broker 改造成多供应商生产网关。

---

未经最终业务门禁，不得开始 Plan 02。每个新行为依次执行 RED 测试、最小实现、GREEN 测试、提交；对 pinned 上游已有 seam 先做 characterization，若测试直接 GREEN 就保存 test-only 证据，不为追求形式主动制造失败。对 pinned 上游 `AGENTS.md` 要求的 `just fix` 例外：先完成 GREEN，再运行 fix；若 fix 产生语义改动，开启新 RED/GREEN 循环，若只是机械 lint 修正，按上游规则不重复同一测试。

该结论不宣称完整产品已经胜过毫无业务 contract 的任意通用 AI，不宣称跨行业泛化，也不宣称已经产出爆款或获得真实发布效果；这些分别归 Plan 03 的多案例业务门和 Plan 10 的发布—回收闭环证明。

**核心架构：** 根 Thread 就是唯一对结果负责的 Lead。Lead 根据目标动态读取材料、调用 Skill/MCP/工具并按需创建子代理；不增加固定部门拓扑、十三 Agent 名册、线性流水线或第二套编排器。评测器作为原生 App Server V2 客户端启动冻结的 `codex app-server --listen stdio:// --strict-config`：两臂发送字节完全相同、且不泄露内部 Skill 名称的自然业务输入、`additionalContext` 和输出 Schema，唯一处理差异是候选隔离 Home 能发现一个原生 Skill，通用隔离 Home 不能发现它。候选必须依靠 Codex 原生 catalog/description 自主选用能力；未选用就是候选真实失败。`rawResponse/completed` 对根线程及后代逐响应计量。

本次 G2 为了两臂等价隔离，不在评测 Home 安装 MCP 或插件，并关闭工具网络；这只是单案例 eval fixture 的控制变量，不删除 harness 的 MCP/插件/联网能力，也不是客户产品的默认权限。

**凭证边界：** 两次顶层运行由同一个 `live-pair` 协调器和同一个本地 Responses broker 串行执行。broker 从协调器 stdin 读取供应商密钥并注入上游请求；Codex 子进程没有密钥环境变量、`auth.json`、登录状态或凭证文件。broker 只保留脱敏计数、哈希与 usage，不落 Prompt、响应正文或 SSE dump。

**锁定基线：**

- OpenAI Codex：`4ef1d4b89bd419c976b04fefa0fd36844e898340`
- 六版决策导入基线：`58baf3b`；实际实施使用包含最终规格、台账和本计划的干净 `decision tip` 完整 SHA
- Rust：`1.95.0`
- Python：`>=3.11`
- `uv`：`0.11.3`
- `dotslash`：`0.5.8`
- 目标系统：当前开发宿主原生报告的 macOS x86_64、目标机 Windows 11 x64；Rosetta/交叉编译不得冒充 arm64 原生证据

---

## 0. 不变量、范围与停止条件

本计划只交付 G0–G2：来源锁定、Codex 原样基线、最小 Lead 的真实业务证明。

本计划明确不做：Electron、外置 sidecar、localhost 产品 UI、客户数据库、SQLCipher、云端 IAM/积分/账本、生产能力网关、供应商注册表、第二国产 provider onboarding、媒体生成、发布、代理管理、内部后台、安装包或数字人。它们必须等待本计划写出 `PASS_TO_PHASE_0B`。

运行不变量：

1. 只有根 Thread 对最终 `ContentPackage` 负责；子代理可以工作，但不能成为第二个业务真相。
2. 两臂使用相同 Codex 二进制、评测器二进制、broker 二进制、模型、供应商、配置字节、案例、材料、根用户输入、`additionalContext`、Schema、权限、超时和调用上限。
3. 唯一处理差异是：通用 Home 的目标 Skill 数量为 0；候选 Home 的目标 Skill 数量为 1，且内容等于源码锁定资产。
4. “两次运行”是两次顶层业务评测，不是两次上游模型请求。每次运行可由 Lead 发出多个根/子代理 Responses 请求，但 broker 分臂限制请求尝试数、时长和账户硬预算。
5. 评测权限仅用于保护私有案例和凭证：材料可读、文件不可改、工具网络关闭；这不是客户产品的业务权限设计，也不得被复制成日后阉割业务能力的默认策略。
6. 任一审批请求、用户输入请求、空 usage、未知线程请求、第三次顶层运行、重试、超时、仓库变脏、案例材料改变、目录污染、catalog 漂移、晚到子线程或无法绑定的费用，都使本次证明失效；不得“修日志”后判通过。
7. 私有案例、材料、Prompt 展开值、模型输出、线程 ID、盲评映射和评审原文全部留在 Git 外；仓库只提交哈希、聚合分数、成本、结论和无正文证明。
8. 生产 crate 和发布包不得依赖或包含 `codex-ai-ip-eval`、`live-pair`、盲评 rubric、stdin credential path 或 Phase 0A broker CLI。可以复用经独立 characterization 证明的低层 Responses/SSE codec，但不能复用评测状态机；本 broker 不增加 IAM、DeviceRegistration、账本、路由注册表、多租户、持久 ProviderAttempt、回调或供应商对账。

上游 seam 决策：

| Seam | 锁定源码路径 | 决策 |
|---|---|---|
| App Server CLI / stdio | `codex-rs/cli/src/main.rs`, `codex-rs/app-server/src/lib.rs` | **保留**，评测走真实外部二进制 |
| V2 Thread/Turn/Item | `codex-rs/app-server-protocol/src/protocol/v2/` | **保留**，直接使用 typed protocol |
| Skill discovery | `codex-rs/ext/skills/`, `codex-rs/skills/`, `codex-rs/core/src/skills.rs` | **保留**，候选能力作为可删除原生 Skill |
| Subagent | `codex-rs/core/src/agent/`, `codex-rs/core/src/session/multi_agents.rs` | **保留**，Lead 按需调用 |
| Responses proxy | `codex-rs/responses-api-proxy/` | **直接修改**，抽出可复用 server、请求 gate 与脱敏 observer；现有 CLI 行为保持兼容 |
| 业务 contracts | `codex-rs/ai-ip-domain/`, `codex-rs/ai-ip-runtime/` | **新增**，保持独立且可测试 |
| 一次性证明器 | `codex-rs/ai-ip-eval/` | **新增**、`publish = false`，迁移后可删除 |
| 生产模型能力网关 | `codex-rs/ai-ip-provider/` 与云端 Capability Gateway | **本阶段不创建**；Plan 06 才建立统一能力 profile、火山 G4a 和后续 route-scoped G4p |

主要预期触达文件图（executable child plan 才拥有 exact file authority）：

```text
docs/AGENTS.override.md
.ai-ip/AGENTS.override.md
.gitattributes
.ai-ip/upstream.lock.toml
MODIFICATIONS.md
NOTICE
docs/architecture/codex-fork-patch-ledger.md
scripts/ai_ip/foundation/{capture_command.py,test_capture_command.py,verify_evidence.py,test_verify_evidence.py,run_frozen_eval.py,test_run_frozen_eval.py,required_command_matrix.json}
codex-rs/ai-ip-domain/{Cargo.toml,BUILD.bazel,src/*}
codex-rs/ai-ip-runtime/{Cargo.toml,BUILD.bazel,src/*}
ai-ip-assets/skills/deliver-ai-ip-content-package/{SKILL.md,BUILD.bazel}
codex-rs/responses-api-proxy/{BUILD.bazel,src/{lib.rs,broker.rs,read_api_key.rs}}
codex-rs/ai-ip-eval/{Cargo.toml,BUILD.bazel,src/*,tests/fixtures/*}
codex-rs/app-server/tests/suite/v2/{mod.rs,raw_response_subagents.rs,ai_ip_strict_output.rs}
codex-rs/app-server/src/request_processors/thread_processor.rs  # 仅集成测试证实 race 时
ai-ip-evals/rubrics/{content-package-blind-review.json,reviewer-submission.schema.json,BUILD.bazel}
ai-ip-evals/schemas/{held-out-attestation.schema.json,cost-receipt.schema.json,frozen-run-context.schema.json,business-report.schema.json,report-index.schema.json,attempt-index.schema.json,verification.schema.json,retention-closeout.schema.json,BUILD.bazel}
docs/evidence/foundation/*
docs/evidence/business-proof/README.md
docs/evidence/business-proof/index.json
docs/evidence/business-proof/<run-id>/report.json
docs/evidence/business-proof/<run-id>/verification.json
docs/evidence/business-proof/<run-id>/retention-closeout.json
```

---

## Work Package 1：建立 Codex 完整 Git 祖先与 fork 来源台账

**文件：**

- 新建：`docs/AGENTS.override.md`
- 新建：`.ai-ip/AGENTS.override.md`
- 新建：`.gitattributes`
- 新建：`.ai-ip/upstream.lock.toml`
- 新建：`MODIFICATIONS.md`
- 新建：`docs/architecture/codex-fork-patch-ledger.md`
- 修改：`NOTICE`

#### Contract 1：抓取精确源码并建实现 worktree

```bash
set -euo pipefail
git remote add upstream https://github.com/openai/codex.git 2>/dev/null || test "$(git remote get-url upstream)" = "https://github.com/openai/codex.git"
git fetch --no-filter upstream 4ef1d4b89bd419c976b04fefa0fd36844e898340
test "$(git cat-file -t 4ef1d4b89bd419c976b04fefa0fd36844e898340)" = commit
test -z "$(git rev-list --objects --missing=print 4ef1d4b89bd419c976b04fefa0fd36844e898340 | sed -n 's/^?//p')"
test "$(git config --bool --get remote.upstream.promisor 2>/dev/null || printf false)" = false
test -z "$(git status --porcelain=v1 --untracked-files=all)"
ai_ip_decision_tip="$(git rev-parse HEAD)"
test "$(git cat-file -t "$ai_ip_decision_tip")" = commit
git merge-base --is-ancestor 58baf3b "$ai_ip_decision_tip"
git show "$ai_ip_decision_tip:docs/superpowers/specs/2026-08-25-domestic-model-adaptation-design.md" >/dev/null
git show "$ai_ip_decision_tip:docs/superpowers/specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md" >/dev/null
git show "$ai_ip_decision_tip:docs/superpowers/plans/2026-08-25-01a-codex-fork-provenance.md" >/dev/null
git worktree add ../ai-ip-phase-0a -b codex/phase-0a-business-proof 4ef1d4b89bd419c976b04fefa0fd36844e898340
git -C ../ai-ip-phase-0a merge --no-ff --allow-unrelated-histories "$ai_ip_decision_tip" -m "chore: establish Codex-based AI IP fork"
git -C ../ai-ip-phase-0a merge-base --is-ancestor 4ef1d4b89bd419c976b04fefa0fd36844e898340 HEAD
git -C ../ai-ip-phase-0a merge-base --is-ancestor "$ai_ip_decision_tip" HEAD
test -z "$(git -C ../ai-ip-phase-0a status --porcelain=v1 --untracked-files=all)"
```

预期：两个完整历史都可达；锁定 Codex commit 的可达 Git 对象闭包在本地完整存在，没有 partial-clone/promisor 缺失对象；没有复制源码快照，也没有创建虚构 `origin`。G0 机器证据必须明确记录 `productOriginStatus=unconfigured`；真实产品 fork URL 是首次协作推送/发布门，不是本地基础证明中可伪造的值。

#### Contract 2：写入窄范围上游规则例外和来源文件

`docs/AGENTS.override.md` 只允许 fork 自有治理文档：

```markdown
# AI IP fork override

All instructions in upstream `AGENTS.md` remain in force.

This fork may add product-owned provenance documentation under `docs/architecture/codex-fork-patch-ledger.md`, `docs/evidence/`, and `docs/superpowers/`. This narrow exception does not authorize general Codex documentation beside upstream modules.
```

`.ai-ip/AGENTS.override.md` 只允许该目录下的产品来源/机器台账。两个 override 都在子目录，必须继承并追加仓库根的上游 `AGENTS.md`；严禁新建根 `AGENTS.override.md`，因为它会在 discovery 中遮蔽上游根规则，光写“remain in force”不会让其被加载。

`.gitattributes` 固定跨平台字节：

```gitattributes
docs/evidence/**/*.stdout.log -text
docs/evidence/**/*.stderr.log -text
codex-rs/Cargo.lock text eol=lf
MODULE.bazel.lock text eol=lf
pnpm-lock.yaml text eol=lf
```

`.ai-ip/upstream.lock.toml`：

```toml
[openai_codex]
repository = "https://github.com/openai/codex"
sha = "4ef1d4b89bd419c976b04fefa0fd36844e898340"
integration = "git-merge-ancestor"
license = "Apache-2.0"
planning_base = "58baf3b"
decision_tip = "replace-with-the-full-sha-captured-in-step-1"

[product]
name = "AI IP 1.1"
runtime = "codex-app-server"
deerflow_dependency = false
origin_status = "unconfigured"
```

上面的 `decision_tip` 是写入动作说明，不是允许提交的值：实施者必须用 `apply_patch` 把它替换为 Step 1 已捕获的 `ai_ip_decision_tip` 40 位小写 SHA，并在提交前用 TOML parser 断言它与 merge parent 祖先一致；仓库中出现字面量 `replace-with-` 即失败。

`MODIFICATIONS.md` 和 patch ledger 必须声明：这是 Codex 衍生 fork；每个 patch 记录上游 seam、业务理由、测试、同步冲突和删除/回退路径。向 `NOTICE` 追加 AI IP 修改说明，不删除上游版权文本。

#### Contract 3：验证属性与提交

```bash
set -euo pipefail
cd ../ai-ip-phase-0a
test "$(git check-attr text -- codex-rs/Cargo.lock | awk -F': ' '{print $3}')" = set
test "$(git check-attr eol -- codex-rs/Cargo.lock | awk -F': ' '{print $3}')" = lf
test "$(git check-attr text -- docs/evidence/foundation/example.stdout.log | awk -F': ' '{print $3}')" = unset
git diff --check
git add docs/AGENTS.override.md .ai-ip/AGENTS.override.md .gitattributes .ai-ip/upstream.lock.toml MODIFICATIONS.md NOTICE docs/architecture/codex-fork-patch-ledger.md
git commit -m "chore: lock Codex upstream provenance"
```

---

## Work Package 2：在未改 Codex 上采集 macOS 与 Windows 11 基线

**文件：**

- 新建：`scripts/ai_ip/foundation/capture_command.py`
- 新建：`scripts/ai_ip/foundation/test_capture_command.py`
- 新建：`scripts/ai_ip/foundation/verify_evidence.py`
- 新建：`scripts/ai_ip/foundation/test_verify_evidence.py`
- 新建：`scripts/ai_ip/foundation/run_frozen_eval.py`
- 新建：`scripts/ai_ip/foundation/test_run_frozen_eval.py`
- 新建：`scripts/ai_ip/foundation/required_command_matrix.json`
- 新建：`docs/evidence/foundation/README.md`
- 生成：`docs/evidence/foundation/macos-x86_64/*`
- 生成：`docs/evidence/foundation/windows-11-x64/*`

#### Contract 1：先写 recorder 失败测试

测试必须证明 recorder：流式计算 stdout/stderr SHA-256；保留原始退出码；记录 RFC3339 UTC 起止时间、平台、架构、完整 Git SHA、recorder SHA、命令 argv、Cargo/npm/Bazel lock SHA；拒绝 dirty tested tree、相对 worktree/evidence path、evidence 写入 tested tree 和日志路径逃逸。`test_verify_evidence.py` 先覆盖缺文件、matrix ID/argv/expected-exit 漂移、错误 SHA、错误 host/arch、日志篡改、lock 篡改和未知 post failure 的拒绝路径。它还从一开始冻结公开 final 模式的 CLI/fixture：`--candidate-sha --selected-report --report-index --business-verification-receipt --verification-output`，拒绝多选 report、receipt/report/candidate 不匹配、非 exact public fields 和任何 `--private-run-root`/key/attestation 参数。`test_run_frozen_eval.py` 在临时目录分别构造一份合法 `executionMode=replay` 和一份合法 `executionMode=live` context，再覆盖 context 非绝对路径、Schema/平台/后缀错误、mode-specific 必填/禁填字段错误、candidate 不匹配、二进制 hash 漂移、冻结 evaluator worktree 变脏、重复输入变参和子命令未收到 `--frozen-run-context` 的拒绝路径。

```bash
uv run --python 3.11 --with pytest==8.3.5 pytest -q scripts/ai_ip/foundation/test_capture_command.py scripts/ai_ip/foundation/test_verify_evidence.py scripts/ai_ip/foundation/test_run_frozen_eval.py
```

预期：FAIL，`capture_command.py` 尚不存在。

#### Contract 2：实现 recorder 并通过窄测试

`capture_command.py` 使用 `subprocess.Popen` + 两个读取线程逐块写日志和 digest，不把完整日志读入内存；临时文件 `flush` + `fsync` 后 `os.replace` 为最终文件。manifest 使用 `json.dumps(sort_keys=True, separators=(",", ":"))`，时间用 `datetime.now(timezone.utc).isoformat()`。`required_command_matrix.json` 是机器可读的唯一 matrix 权威，其 canonical SHA 写入每份 manifest；下表必须与它逐字段一致。`verify_evidence.py` 只验证 foundation/公开证据，从固定 matrix 读取 expected 项，复算全部字节并把 baseline/post failure ID 精确配对；final 模式只额外绑定已选 public report/index 和 Rust 产生的无正文 live-proof receipt，不重写私有业务验证。它不接触私有案例或 commitment key，不得接受调用方传入“忽略失败”列表。`run_frozen_eval.py` 只接受一个绝对 context 路径与 `--` 后的子命令；它按 context 解析当前平台的精确 evaluator（Windows 必须是已锁定的 `.exe` 路径），并始终复算 context 内冻结的独立 evaluator worktree 的 candidate/clean status 与二进制 hash；它不以已提交 report/evidence 的主取证 worktree HEAD 代替 candidate。验证后用 argv 直接 `exec`，且自动附加 `--frozen-run-context <absolute-path>`；拒绝子命令重复传入已冻结的模型、provider、case、二进制或上限。

```bash
uv run --python 3.11 --with pytest==8.3.5 pytest -q scripts/ai_ip/foundation/test_capture_command.py scripts/ai_ip/foundation/test_verify_evidence.py scripts/ai_ip/foundation/test_run_frozen_eval.py
```

预期：PASS。

#### Contract 3：先提交取证工具，再制作可验证的双 ref bundle

baseline 还没有任何 evidence 时先提交 recorder/verifier/matrix，使 Windows 可以从确定 tools SHA 运行与 macOS 完全相同的工具：

```bash
set -euo pipefail
git add scripts/ai_ip/foundation docs/evidence/foundation/README.md
git commit -m "test: add native evidence recorder"
ai_ip_tools_sha="$(git rev-parse HEAD)"
ai_ip_bundle_dir="$(mktemp -d "${TMPDIR:-/tmp}/ai-ip-baseline-bundle.XXXXXX")"
git update-ref refs/heads/ai-ip-transfer/pinned-codex 4ef1d4b89bd419c976b04fefa0fd36844e898340
git update-ref refs/heads/ai-ip-transfer/foundation-tools "$ai_ip_tools_sha"
git bundle create "$ai_ip_bundle_dir/ai-ip-baseline.bundle" \
  refs/heads/ai-ip-transfer/pinned-codex refs/heads/ai-ip-transfer/foundation-tools
git bundle verify "$ai_ip_bundle_dir/ai-ip-baseline.bundle"
shasum -a 256 "$ai_ip_bundle_dir/ai-ip-baseline.bundle" > "$ai_ip_bundle_dir/ai-ip-baseline.bundle.sha256"
```

不允许用裸 40-hex 作 `git bundle create` revision；必须包含上面两个 `refs/heads/ai-ip-transfer/*` 命名 ref。传输 bundle 与独立 SHA-256 文件后，Windows 先校验文件 hash、`git bundle verify` 和 `git bundle list-heads`的两个精确 SHA，再从 `pinned-codex` ref 建 tested tree、从 `foundation-tools` ref 建 tools worktree。两端 manifest 同时绑定 `ai_ip_tools_sha`、recorder 和 matrix SHA。

#### Contract 4：锁定工具链并在未改源码 worktree 采集两平台基线

macOS 和 Windows 分别从 `4ef1…` 的独立、干净 worktree 运行。Windows clone 必须：

```powershell
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
git -c core.autocrlf=false clone $env:AI_IP_BASELINE_BUNDLE_PATH $env:AI_IP_WINDOWS_BASELINE_WORKTREE
Set-Location $env:AI_IP_WINDOWS_BASELINE_WORKTREE
git config core.autocrlf false
git checkout --detach refs/remotes/origin/ai-ip-transfer/pinned-codex
if ((git rev-parse HEAD).Trim() -ne '4ef1d4b89bd419c976b04fefa0fd36844e898340') { throw 'wrong baseline SHA' }
if (@(git status --porcelain=v1 --untracked-files=all).Count -ne 0) { throw 'dirty baseline' }
rustup override set 1.95.0
```

Windows 另以 `refs/remotes/origin/ai-ip-transfer/foundation-tools` 建立只读 tools worktree，`AI_IP_RECORDER`/`AI_IP_COMMAND_MATRIX` 必须指向该 worktree，而不是 pinned tested tree。若 clone 对自定义 ref 不生成 remote-tracking ref，则直接用 `git show-ref` 取 bundle 中的精确 ref 并 `git worktree add --detach <path> <full-sha>`；不根据分支名猜 SHA。

记录器从实施 worktree 以绝对路径运行，把 `--repo-root` 设为未修改的 native worktree，把 `--evidence-dir` 设为 tested tree 之外的实施仓库证据目录。每项的规范调用都是：

```bash
uv run --python 3.11 "$AI_IP_RECORDER" \
  --repo-root "$AI_IP_NATIVE_WORKTREE" \
  --evidence-dir "$AI_IP_EVIDENCE_DIR" \
  --matrix "$AI_IP_COMMAND_MATRIX" \
  --name '<matrix-id>'
```

recorder 必须自己从 matrix 取出 argv 并以 tested tree 为 cwd 直接 `exec`，不经 shell 重解析，不允许调用方另传命令。公共 required matrix 如下，`expectedExit` 全部为 `0`：

| ID | 平台 | 精确 argv |
|---|---|---|
| `fmt-check` | 两者 | `just fmt-check` |
| `app-server-protocol` | 两者 | `just test -p codex-app-server-protocol` |
| `app-server-transport` | 两者 | `just test -p codex-app-server-transport` |
| `state` | 两者 | `just test -p codex-state` |
| `thread-store` | 两者 | `just test -p codex-thread-store` |
| `app-server-process` | 两者 | `just test -p codex-app-server --test all -E 'test(=suite::logging::standalone_app_server_emits_json_info_events)'` |
| `thread-start` | 两者 | `just test -p codex-app-server --test all -E 'test(=suite::v2::thread_start::thread_start_creates_thread_and_emits_started)'` |
| `thread-resume` | 两者 | `just test -p codex-app-server --test all -E 'test(=suite::v2::thread_resume::thread_resume_supports_history_and_overrides)'` |
| `executor-skill` | 两者 | `just test -p codex-app-server --test all -E 'test(=suite::v2::executor_skills::explicit_executor_skill_can_read_referenced_file)'` |
| `mcp-tool` | 两者 | `just test -p codex-app-server --test all -E 'test(=suite::v2::mcp_tool::mcp_server_tool_call_returns_tool_result)'` |
| `process-exec` | 两者 | `just test -p codex-app-server --test all -E 'test(=suite::v2::process_exec::process_spawn_returns_before_exit_and_emits_exit_notification)'` |
| `fs` | 两者 | `just test -p codex-app-server --test all -E 'test(=suite::v2::fs::fs_methods_cover_current_fs_utils_surface)'` |
| `apply-patch` | 两者 | `just test -p codex-apply-patch --test all -E 'test(=suite::cli::test_apply_patch_cli_add_and_update)'` |
| `build-cli-app-server` | 两者 | `cargo build --locked --manifest-path codex-rs/Cargo.toml -p codex-cli -p codex-app-server` |
| `cargo-metadata` | 两者 | `cargo metadata --manifest-path codex-rs/Cargo.toml --locked --format-version 1` |
| `cargo-license-source` | 两者 | `cargo deny --manifest-path codex-rs/Cargo.toml --config codex-rs/deny.toml check licenses sources` |
| `pnpm-dependencies` | 两者 | `pnpm list --recursive --json --depth Infinity` |
| `pnpm-licenses` | 两者 | `pnpm licenses list --json --long` |
| `source-assets` | 两者 | `git ls-files -- codex-rs/**/assets/** codex-rs/**/templates/** codex-rs/**/migrations/** ai-ip-assets/**` |
| `macos-sandbox` | macOS | `just test -p codex-core --test all -E 'test(=suite::exec::write_file_fails_as_sandbox_error)'` |
| `windows-sandbox-restricted` | Windows | `just test -p codex-core --test all -E 'test(=suite::windows_sandbox::windows_restricted_token_rejects_exact_and_glob_deny_read_policy)'` |
| `windows-sandbox-elevated` | Windows | `just test -p codex-core --test all -E 'test(=suite::windows_sandbox::windows_elevated_enforces_deny_read_and_protects_setup_marker)'` |

上表全部为 `baselineAndPost`。同一份 frozen matrix 还必须从 Work Package 2 开始就包含下列 `postOnly`，基线阶段明确不执行，不得到 Work Package 9 才改 matrix SHA：

| ID | 平台 | 精确 argv |
|---|---|---|
| `post-domain` | 两者 | `just test -p codex-ai-ip-domain` |
| `post-runtime` | 两者 | `just test -p codex-ai-ip-runtime` |
| `post-eval` | 两者 | `just test -p codex-ai-ip-eval` |
| `post-responses-proxy` | 两者 | `just test -p codex-responses-api-proxy` |
| `post-descendant-raw` | 两者 | `just test -p codex-app-server --test all -E 'test(=suite::v2::raw_response_subagents::raw_events_cover_root_child_grandchild_and_sibling)'` |
| `post-ai-ip-strict-output` | 两者 | `just test -p codex-app-server --test all -E 'test(=suite::v2::ai_ip_strict_output::turn_start_sends_strict_content_package_schema)'` |
| `post-bazel-rust` | 两者 | `bazel test //codex-rs/ai-ip-domain:ai-ip-domain-unit-tests //codex-rs/ai-ip-runtime:ai-ip-runtime-unit-tests //codex-rs/ai-ip-eval:ai-ip-eval-unit-tests //codex-rs/ai-ip-eval:codex-ai-ip-eval-bin-unit-tests //codex-rs/responses-api-proxy:responses-api-proxy-unit-tests` |
| `post-bazel-assets` | 两者 | `bazel build //ai-ip-assets/skills/deliver-ai-ip-content-package:skill //ai-ip-evals/rubrics:rubrics //ai-ip-evals/schemas:schemas` |

对每个 `test(=...)` filter，recorder 先以相同 package/test target 运行 `cargo nextest list --message-format json -E <filter>`，解析并要求恰好一个 test 匹配，再运行 matrix argv；列表证据与执行证据一起进 manifest，防止任何 filter 静默匹配 0/N 项。

在执行 matrix 前，两平台先在 native worktree 运行 `pnpm install --frozen-lockfile`，确认 `cargo deny --version`，再次断言 Git clean；安装命令、pnpm/Rust/cargo-deny/just/Bazel/OS 版本和可执行文件 SHA 写入 host bootstrap manifest。Windows elevated sandbox 项必须在单独的管理员 PowerShell 7.3+ 进程执行，该 token 与 OS build 写入 manifest。上表的 exact test ID 已在锁定源码确认；实施时 verifier 仍须核对它们存在。

基线 required 项任一非 0 即记录 `BLOCKED_BASELINE`，不得自行从 matrix 删除。已知非 required 上游失败与产品回归仍分栏记录。

完整 workspace `just test` 依上游 `AGENTS.md` 先询问用户；批准则运行，拒绝则记录 `not-run/declined`。已知基线失败与产品回归分栏记录。

业务优先的并行规则：Work Package 2 的 macOS focused baseline 和 tools SHA 必须在 Work Package 3 改码前完成；Windows baseline 可在同一 tools SHA 上独立并行，且只需在 Work Package 9 最终 checkpoint 前回传完成。Windows 机器/工具链暂时阻塞不得阻止已批准的隔离 G2 业务实验，但在 G1 补齐前不得写 `PASS_TO_PHASE_0B`。

#### Contract 5：安全回传、验证 manifest 并提交基线证据

Windows 只在 tools SHA 之上提交 `docs/evidence/foundation/windows-11-x64/`，制作以 tools SHA 为 prerequisite 的增量 bundle 和 SHA-256。macOS 先校验 bundle/prerequisite/提交父链，并确认 diff path 只在 Windows evidence 目录，再 cherry-pick；不接受拷贝来的未提交日志。验证脚本必须逐个复算 matrix/recorder/日志/lock 哈希，检查 argv、host/arch/SHA/exit code，并确认 Windows evidence 只由 Windows 原生运行生成。随后：

```bash
just fmt
git diff --check
git add docs/evidence/foundation
git commit -m "test: capture pinned Codex native baseline"
```

---

## Work Package 3：新增纯业务 `ContentPackage` contract

**文件：**

- 新建：`codex-rs/ai-ip-domain/Cargo.toml`
- 新建：`codex-rs/ai-ip-domain/BUILD.bazel`
- 新建：`codex-rs/ai-ip-domain/src/{lib.rs,mission.rs,content_package.rs,domain_tests.rs}`
- 修改：`codex-rs/Cargo.toml`

`ai-ip-domain` 只依赖 workspace `serde` 与 `schemars`；不得依赖 `codex-core`、App Server、数据库或 provider。

#### Contract 1：写失败测试

覆盖：`SubjectKind::{Person,Brand,Product,Organization,Hybrid}`；案例 ID/目标/材料相对路径/集合大小/字符串字节上限；拒绝 `..`、绝对路径、开发期冻结案例 ID；`ContentPackage` 必须包含 `missionSummary`、`InfluenceRelation`、至少一个 `ActionFunnelStep`、`strategicJudgment`、可直接发布/拍摄的 `PublishableContent`、claims、open questions、measurement plan、readiness；不得声称本 turn 已发布。

`ClaimStatus` 直接冻结六版事实本体：`UserFact`、`ExternalEvidence`、`ModelInterpretation`、`CreativeHypothesis`、`Unknown`、`ActualResult`。`ExternalEvidence` 必须有 source ref；`ActualResult` 必须同时有 source ref 和不可空 `resultReceiptRef`，表示案例材料中已经存在的结果，不允许把本 turn 的草稿伪装成实绩。

`HeldOutMissionCase.materials` 为每份输入声明稳定 `materialId`、相对路径、SHA-256 与 `materialKind = userInput | evidence | actualResultReceipt | other`。`UserFact`、`ExternalEvidence` 和 `ActualResult` 都必须至少绑定一个已声明 source ref；`UserFact` 只能引用 `userInput`，`ExternalEvidence` 只能引用 `evidence`（用户提供的第三方文档/链接必须在 case manifest 入库时明确标为 evidence），`ActualResult.resultReceiptRef` 必须命中 `actualResultReceipt`。`ModelInterpretation`/`CreativeHypothesis` 可以没有外部来源，但不得在 readiness 计算中冒充已支持事实。

关键测试：

```rust
#[test]
fn organization_subject_can_target_a_membership_action() {
    let package = valid_package(SubjectKind::Organization);
    assert_eq!(package.influence_relation.desired_action, "报名参加志愿活动");
    package.validate().unwrap();
}

#[test]
fn external_evidence_without_source_is_rejected() {
    let mut package = valid_package(SubjectKind::Brand);
    package.claims[0].status = ClaimStatus::ExternalEvidence;
    package.claims[0].source_refs.clear();
    assert!(package.validate().is_err());
}

#[test]
fn actual_result_without_receipt_is_rejected() {
    let mut package = valid_package(SubjectKind::Organization);
    package.claims[0].status = ClaimStatus::ActualResult;
    package.claims[0].result_receipt_ref = None;
    assert!(package.validate().is_err());
}

#[test]
fn claim_reference_must_resolve_inside_the_same_case() {
    let case = valid_case();
    let mut package = valid_package(SubjectKind::Brand);
    package.claims[0].source_refs = vec!["another-case:evidence-9".into()];
    assert!(package.validate_against(&case).is_err());
}

#[test]
fn actual_result_receipt_must_have_the_declared_receipt_kind() {
    let case = valid_case();
    let mut package = valid_package(SubjectKind::Organization);
    package.claims[0].status = ClaimStatus::ActualResult;
    package.claims[0].result_receipt_ref = Some("ordinary-material".into());
    assert!(package.validate_against(&case).is_err());
}

#[test]
fn held_out_material_paths_cannot_escape_case_root() {
    let mut case = valid_case();
    case.materials[0].relative_path = "../secret.env".into();
    assert!(case.validate().is_err());
}
```

```bash
just test -p codex-ai-ip-domain
```

预期：FAIL，crate/types 尚不存在。

#### Contract 2：实现封闭、有限 contract

所有 DTO 使用 `#[serde(deny_unknown_fields, rename_all = "camelCase")]`；所有 enum 使用 camelCase；`validate()` 聚合字段错误但不得读取文件。上限常量集中在 `mission.rs`，包括 `MAX_MATERIALS = 128`、目标 8 KiB、单约束 2 KiB、单路径 1 KiB。

`ContentPackage.readiness` 只有 `Draft`、`ReadyForHumanReview`、`BlockedByMissingEvidence`；不能出现 `Published`。`InfluenceRelation` 直接表达 subject/audience/desired action，购买不是默认。`validate_against(&HeldOutMissionCase)` 校验 ref 和 kind；在 runner 中还要对每份 regular file 复算 digest 后才允许最终 package 进入盲评。不存在、跨案例、类型错误或 digest 不符一律失败。

#### Contract 3：验证并提交

```bash
just test -p codex-ai-ip-domain
just bazel-lock-update
bazel test //codex-rs/ai-ip-domain:ai-ip-domain-unit-tests
just bazel-lock-check
just fmt
just fix -p codex-ai-ip-domain
git add codex-rs/Cargo.toml codex-rs/Cargo.lock codex-rs/ai-ip-domain MODULE.bazel.lock
git commit -m "feat: add AI IP content package contracts"
```

按上游规则，`just fix` 后不重复运行同一测试；修正 formatter/lint 产生的差异后直接审阅 diff。

---

## Work Package 4：增加一个可删除的原生 Lead Skill 与有界 turn payload

**文件：**

- 新建：`codex-rs/ai-ip-runtime/Cargo.toml`
- 新建：`codex-rs/ai-ip-runtime/BUILD.bazel`
- 新建：`codex-rs/ai-ip-runtime/src/{lib.rs,prompt.rs,schema.rs,runtime_tests.rs}`
- 新建：`ai-ip-assets/skills/deliver-ai-ip-content-package/{SKILL.md,BUILD.bazel}`
- 修改：`codex-rs/Cargo.toml`

#### Contract 1：先写 payload 与 Schema 失败测试

```rust
#[test]
fn root_and_both_context_layers_are_bounded_before_submission() {
    assert!(approx_token_count(root_prompt()) <= ROOT_PROMPT_MAX_TOKENS);
    let value = evaluation_context(&valid_case()).unwrap();
    assert!(approx_token_count(&value) <= EVALUATION_CONTEXT_MAX_TOKENS);
    let envelope: serde_json::Value = serde_json::from_str(&value).unwrap();
    assert!(envelope.get("task").is_some());
    assert!(envelope.get("missionCase").is_some());
}

#[test]
fn oversized_mission_is_rejected_before_envelope_serialization() {
    let case = mission_over_limit_but_envelope_under_outer_limit();
    assert!(matches!(
        evaluation_context(&case),
        Err(RuntimePromptError::MissionTooLarge { .. })
    ));
}

#[test]
fn generated_schema_is_closed_recursively() {
    let schema = content_package_schema().unwrap();
    assert!(schema.get("$schema").is_none());
    assert_closed_objects(&schema);
}

#[test]
fn strict_schema_requires_every_property_and_uses_null_for_optionals() {
    let schema = content_package_schema().unwrap();
    assert_required_equals_properties_recursively(&schema);
    assert_optional_fields_are_required_nullable(&schema);
    validate_responses_strict_subset(&schema).unwrap();
}

#[test]
fn canonical_skill_asset_is_discoverable_by_the_pinned_parser() {
    let skill = parse_committed_lead_skill().unwrap();
    assert_eq!(skill.name, LEAD_SKILL_NAME);
    assert!(!skill.description.trim().is_empty());
    assert!(skill.enabled);
}
```

```bash
just test -p codex-ai-ip-runtime
```

预期：FAIL。

#### Contract 2：写最小 Skill

`SKILL.md` 以下列 canonical frontmatter 开头，必须被 pinned Codex Skill parser 无错解析：

```yaml
---
name: deliver-ai-ip-content-package
description: Use when the user needs an evidence-aware, publishable AI IP content package tied to a real audience action.
---
```

其精确职责：从 mission 与可核验材料交付一个有行动路径的 `ContentPackage`；判断 subject→audience→desired action，但行动目标只用于适配和验收，不能反向把内容根强制成产品或 CTA。先从真实证据、事件、矛盾或体验中形成值得讲的内容机会；需要命名事实时另行取证；再选择适合的具体选题和表现形式，最后检查商业连接。以上是可跳过/回退的判断原则，不是固定阶段。Lead 可按需读材料、调用工具或创建子代理；事实、推断、创意假设分开；缺证据时保留可用草稿与问题；不得伪造发布或效果。Skill 不含五个冻结案例、黄金礼品、直播公会答案、固定问卷、固定三个策略或固定 Agent 名册。

它只描述决策原则，不重复 JSON Schema，不把材料正文拼入 Skill，也不规定必须调用子代理。

#### Contract 3：实现有界 runtime helpers

`prompt.rs` 权威常量：

```rust
pub const LEAD_SKILL_NAME: &str = "deliver-ai-ip-content-package";
pub const ADDITIONAL_CONTEXT_KEY: &str = "ai_ip_evaluation";
pub const ROOT_PROMPT_MAX_TOKENS: usize = 96;
pub const EVALUATION_CONTEXT_MAX_TOKENS: usize = 900;
const MISSION_CASE_MAX_TOKENS: usize = 640;
const ROOT_PROMPT: &str = "Complete the business task in the supplied additional context. Use any relevant available capabilities as needed and return only the JSON object required by the output schema.";

pub fn root_prompt() -> &'static str { ROOT_PROMPT }

pub fn evaluation_context(case: &HeldOutMissionCase) -> Result<String, RuntimePromptError> {
    case.validate()?;
    let mission = serde_json::to_value(case)?;
    let mission_rendered = serde_json::to_string(&mission)?;
    reject_if_over_limit(&mission_rendered, MISSION_CASE_MAX_TOKENS)?;
    let value = serde_json::json!({
        "task": "基于 missionCase 与其中的相对路径材料，交付一份可直接拍摄或发布的短内容成果。先按需检查材料；只输出符合给定 JSON Schema 的对象；不要声称已经执行发布。",
        "missionCase": mission
    });
    let rendered = serde_json::to_string(&value)?;
    reject_if_over_limit(&rendered, EVALUATION_CONTEXT_MAX_TOKENS)?;
    Ok(rendered)
}
```

`schema.rs` 使用锁定的 `schemars 0.8.22` 的 `schema_for!(ContentPackage)` 后转成 Responses strict subset。该版本生成根 Draft-07 `$schema`、`definitions` 和 `#/definitions/...` ref；converter 必须先移除根 `$schema`，递归处理 `definitions`，再在根上原子改名为 `$defs`，并把全部 local ref 改写为 `#/$defs/...`。之后对 `$defs`/properties/items/`anyOf`/`oneOf`/`allOf` 递归执行：每个 object 的 `additionalProperties=false`，`required` 精确等于所有 `properties` key，Rust `Option<T>` 保留为 required 但允许 `null` 的 union。validator 拒绝根或嵌套 `$schema`、缺 required、额外属性、optional omission、Responses strict 不支持关键字、未改写/悬空 ref；单测必须有根 `$schema` 移除、嵌套 ref 和 ref rewrite。Work Package 4 只做 strict-subset 纯单元测试；发出请求的 `strict=true` 由 Work Package 5 的 evaluator/App Server mock Responses 集成测试证明。运行时不依赖 `codex-core`，也不构造自定义 context fragment。

`ai-ip-runtime` 的 dev-dependencies 必须同时包含 workspace `codex-skills` 与 `codex-utils-cargo-bin`；`codex-skills` 不进入 runtime 产品依赖。`codex-rs/ai-ip-runtime/BUILD.bazel` 的 `codex_rust_crate(...)` 增加 `test_data_extra = ["//ai-ip-assets/skills/deliver-ai-ip-content-package:skill"]`。Skill `filegroup(name = "skill", ...)` 的 visibility 精确为 `['//codex-rs/ai-ip-runtime:__pkg__', '//codex-rs/ai-ip-eval:__pkg__']`。runtime/eval 测试分别通过 `codex_utils_cargo_bin::find_resource!` 的仓库相对参数 `../../ai-ip-assets/skills/deliver-ai-ip-content-package/SKILL.md` 定位资产，禁止依赖 cwd。

#### Contract 4：验证原生 Skill 资产并提交

```bash
just test -p codex-ai-ip-runtime
if rg -n '(黄金礼品|直播公会|宝妈|creator sees|passes review|first live|map-marketing-content-world|ContentRootLab|固定内容根)' ai-ip-assets/skills/deliver-ai-ip-content-package/SKILL.md codex-rs/ai-ip-runtime/src codex-rs/ai-ip-runtime/Cargo.toml codex-rs/ai-ip-runtime/BUILD.bazel; then exit 1; fi
just bazel-lock-update
bazel test //codex-rs/ai-ip-runtime:ai-ip-runtime-unit-tests
bazel build //ai-ip-assets/skills/deliver-ai-ip-content-package:skill
just bazel-lock-check
just fmt
just fix -p codex-ai-ip-runtime
git add codex-rs/Cargo.toml codex-rs/Cargo.lock codex-rs/ai-ip-runtime ai-ip-assets/skills MODULE.bazel.lock
git commit -m "feat: add removable AI IP Lead skill"
```

#### Contract 5：七轮 programmatic prompt-optimization stop decision 与 Work Package 4 完成边界

七轮经审阅的 Skill wording cycle 支持一个工程上的 programmatic prompt-optimization stop decision：不再以改写 `SKILL.md` 追求 factual-fidelity acceptance。它不是“任何 prompt 都不可能通过”的理论证明，也不是 live-model 或泛化估计；Cycle 1–7 是 `providerMode=not-run` 的 fresh-context 诊断。Cycle 7 在三份冻结 packet 均已核对 digest 后只完成第一份样本：original 与新 held-out 各为 18/18；cycle-6 regression 的 maker-03 为 16/18。其 `contentPremise` 把 `project-card` 中的小册子和 `design-note` 中独立的黑白草稿/下周三种封面测试写成同一项目的 `实际进度`，没有单一 source span 蕴含该跨来源连接，因此 `E` 与 `F` 均失败。控制器在这个 RED 后立即取消其余十二个 dispatch，不为未执行的输出补推结论。

此结果是诚实的 diagnostic RED，而不是对 Skill 的继续改词授权：raw Skill 是有用的生成指令，而 isolated evaluation of its outputs 继续暴露事实保真 RED，不能作为事实保真的业务验收 guard。Work Package 4 可以在完整有序的 Cargo、validator/gate、Bazel、lock、fmt/fix 验证与新 scoped review 后，只完成其机械范围（可删除原生 Skill、pinned parser/resource、bounded payload、strict Schema 和构建闭环）。该完成不得声称 all-sample factual fidelity、business acceptance、G2、Phase 0B readiness 或 live-model quality。Phase 0A factual acceptance 仍是 Work Package 6 的 source-visible factual severe-failure human blind-review gate：它不是 all-sample acceptance，也不声称自动发布安全。production/persistent Evidence Store 不属于 Work Package 4；Plan 03 先定义和评测任何将来的 automatic guard，Plan 05 才拥有 durable encrypted local persistence。

---

## Work Package 5：把原生 App Server、凭证 broker 与整树计量做成原子 paired evaluator

**文件：**

- 修改：`codex-rs/responses-api-proxy/src/lib.rs`
- 新建：`codex-rs/responses-api-proxy/src/broker.rs`
- 新建：`codex-rs/responses-api-proxy/src/broker_tests.rs`
- 修改：`codex-rs/responses-api-proxy/src/read_api_key.rs`
- 修改：`codex-rs/responses-api-proxy/Cargo.toml`
- 修改：`codex-rs/responses-api-proxy/BUILD.bazel`
- 新建：`codex-rs/app-server/tests/suite/v2/raw_response_subagents.rs`
- 新建：`codex-rs/app-server/tests/suite/v2/ai_ip_strict_output.rs`
- 修改：`codex-rs/app-server/tests/suite/v2/mod.rs`
- 修改：`codex-rs/app-server/{Cargo.toml,BUILD.bazel}`
- 条件修改：`codex-rs/app-server/src/request_processors/thread_processor.rs`（仅 pinned 集成测试证明 listener attach race 时）
- 新建：`codex-rs/ai-ip-eval/Cargo.toml`
- 新建：`codex-rs/ai-ip-eval/BUILD.bazel`
- 新建：`codex-rs/ai-ip-eval/src/{lib.rs,main.rs,model.rs,app_server.rs,catalog.rs,broker_gate.rs,runner.rs,evidence.rs,eval_tests.rs}`
- 新建：`codex-rs/ai-ip-eval/tests/fixtures/{replay-fixture-set.json,replay-case.json,replay-transcript.jsonl,replay-attestation.json,replay-review-1.json,replay-review-2.json,replay-review-3.json}`
- 修改：`codex-rs/Cargo.toml`

### 5.1 先冻结证据模型和失败测试

#### Contract 1：注册 evaluator，写真实 wire replay fixture

`codex-rs/ai-ip-eval/Cargo.toml`：

先在根 `codex-rs/Cargo.toml` 的 `[workspace.dependencies]` 增加当前 lock 已存在的精确版本：

```toml
windows-sys = { version = "0.61.2", features = ["Win32_Foundation", "Win32_Security", "Win32_Security_Authorization", "Win32_Storage_FileSystem", "Win32_System_Threading"] }
```

然后 crate manifest 为：

```toml
[package]
name = "codex-ai-ip-eval"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false

[lib]
name = "codex_ai_ip_eval"
path = "src/lib.rs"
doctest = false

[[bin]]
name = "codex-ai-ip-eval"
path = "src/main.rs"

[lints]
workspace = true

[dependencies]
anyhow = { workspace = true }
chrono = { workspace = true }
clap = { workspace = true, features = ["derive"] }
codex-ai-ip-domain = { workspace = true }
codex-ai-ip-runtime = { workspace = true }
codex-app-server-protocol = { workspace = true }
codex-process-hardening = { workspace = true }
codex-responses-api-proxy = { workspace = true }
codex-skills = { workspace = true }
ctor = { workspace = true }
hmac = { workspace = true }
rand = { workspace = true }
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
sha2 = { workspace = true }
tokio = { workspace = true, features = ["io-util", "process", "rt-multi-thread", "sync", "time"] }
toml = { workspace = true }

[target.'cfg(windows)'.dependencies]
windows-sys = { workspace = true }

[dev-dependencies]
codex-utils-cargo-bin = { workspace = true }
pretty_assertions = { workspace = true }
tempfile = { workspace = true }
```

`main.rs` 在解析参数或读 stdin 前保留与原 proxy 二进制相同的 pre-main hardening：

```rust
#[ctor::ctor]
fn pre_main() {
    codex_process_hardening::pre_main_hardening();
}
```

Phase 0A 的付费 G2 live 只在已审批 macOS 开发宿主执行；Windows 本阶段只跑编译/测试/回放，不读 provider secret。Windows ACL 代码仍用 `windows-sys` 的 token、SID、ACL 与 filesystem API 建立当前用户唯一可读的 seed/context，并有原生 Windows 失败测试实际重新读取 DACL，验证只有当前 user SID（加不可避免的 SYSTEM/owner control）可读，而不是只验证文件可创建或只编译。`Win32_System_Threading` 是 `GetCurrentProcess`/`OpenProcessToken` 所需 feature；未完成 `ReadFile` + `VirtualLock` + zeroize + process mitigation 前不得扩展 Windows live。

Work Package 5 初始 `BUILD.bazel` 的 `test_data_extra` 包含 `glob(["tests/fixtures/**"])` 和 `//ai-ip-assets/skills/deliver-ai-ip-content-package:skill`；测试统一用 `codex_utils_cargo_bin::find_resource!`。Work Package 6 创建 rubric/schema filegroup 后再把 `//ai-ip-evals/rubrics:rubrics` 和 `//ai-ip-evals/schemas:schemas` 加入同一 `test_data_extra`，不允许任何 contract test 依赖 cwd。

Replay transcript 不再伪造 `codex exec` 事件，也不另传 package 文件。它至少包含：

```json
{"method":"rawResponse/completed","params":{"threadId":"root-thread","turnId":"root-turn","responseId":"resp-1","usage":{"totalTokens":150,"inputTokens":100,"cachedInputTokens":10,"cacheWriteInputTokens":0,"outputTokens":50,"reasoningOutputTokens":20}}}
{"method":"item/completed","params":{"threadId":"root-thread","turnId":"root-turn","completedAtMs":1787616000000,"item":{"type":"agentMessage","id":"final-1","text":"{\"missionSummary\":\"...valid fixture JSON...\"}"}}}
{"method":"turn/completed","params":{"threadId":"root-thread","turn":{"id":"root-turn","items":[{"type":"agentMessage","id":"final-1","text":"{\"missionSummary\":\"...same valid fixture JSON...\"}"}],"status":"completed","error":null,"itemsView":"full","startedAt":null,"completedAt":null,"durationMs":null}}}
```

每行 fixture 必须先反序列化为 pinned `ServerNotification`；其中的完整 package JSON 必须能反序列化为真实 `ContentPackage`，`item/completed` 与 `turn/completed` 文本字节相同。

#### Contract 2：写模型与 collector 失败测试

`model.rs` 的私有 `RunManifest` 至少包含：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub cache_write_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderRole {
    TargetVolcengine,
    ApprovedReference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProofBrokerCompatibilityName {
    #[serde(rename = "OpenAI")]
    OpenAi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "executionMode", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ModeEvidence {
    Replay {
        fixture_set_sha256: String,
    },
    Live {
        attestation_sha256: String,
        provider_budget_evidence_sha256: String,
        approval_commitment: String,
        provider_endpoint_commitment: String,
        provider_role: ProviderRole,
        arm_order_commitment: String,
        rate_card_sha256: String,
        billing_policy_sha256: String,
        fx_policy_sha256: Option<String>,
        authorized_pair_cost_fen: u64,
        retention_deadline: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunManifest {
    pub schema_version: u32,
    pub pair_id: String,
    pub frozen_run_context_sha256: String,
    pub execution_context_sha256: String,
    pub run_ordinal: u8,
    pub condition: EvaluationCondition,
    pub fork_sha: String,
    pub case_sha256: String,
    pub source_materials_sha256: String,
    pub prompt_sha256: String,
    pub additional_context_sha256: String,
    pub output_schema_sha256: String,
    pub thread_start_request_sha256: String,
    pub turn_start_request_sha256: String,
    pub shared_config_sha256: String,
    pub effective_config_sha256: String,
    pub config_layers_sha256: String,
    pub native_skill_sha256: Option<String>,
    pub pre_skill_catalog_sha256: String,
    pub post_skill_catalog_sha256: String,
    pub normalized_base_catalog_sha256: String,
    pub skill_use_evidence_sha256: Option<String>,
    pub codex_binary_sha256: String,
    pub evaluator_binary_sha256: String,
    pub broker_component_sha256: String,
    pub first_root_provider_request_commitment: String,
    pub normalized_first_root_base_commitment: String,
    pub first_root_treatment_diff_commitment: Option<String>,
    pub app_server_transcript_sha256: String,
    pub broker_attempt_ledger_sha256: String,
    pub attempt_index_root_sha256: String,
    pub content_package_sha256: String,
    pub root_thread_id: String,
    pub root_turn_id: String,
    pub session_id: String,
    pub provider_request_attempt_count: u64,
    pub provider_completed_response_count: u64,
    pub raw_response_count: u64,
    pub usage_scope: String,
    pub usage: Usage,
    pub model_label: String,
    pub actual_model_revision: String,
    pub deployment_or_fingerprint_commitment: Option<String>,
    pub provider_label: String,
    pub provider_compatibility_name: ProofBrokerCompatibilityName,
    pub authorized_evaluation_run_cost_fen: u64,
    pub max_provider_request_attempts: u64,
    pub max_total_tokens: i64,
    pub max_elapsed_seconds: u64,
    pub elapsed_ms: u128,
    pub tree_closed: bool,
    pub execution_mode: ExecutionMode,
    pub mode_evidence: ModeEvidence,
}
```

测试必须覆盖：

- 对 `(threadId, turnId, responseId)` 去重；相同重复只计一次，usage 冲突重复失败；
- `usage: null`、负数、`checked_add` overflow、未知 thread、缺少 root terminal、多个 root final message、item/turn final 不一致均失败；
- `Usage.total_tokens` 等于所有唯一 raw completion 的 `totalTokens` 之和，不用 root 累积 usage 代替；
- 每个 raw completion 必须满足 `totalTokens = inputTokens + outputTokens`、`cachedInputTokens + cacheWriteInputTokens <= inputTokens`、`reasoningOutputTokens <= outputTokens`；任一负数或内部不一致在聚合前失败。
- replay 只验证机械流程，manifest 明确 `executionMode: replay`，永远不能进入 G2 report；
- replay 的 case boundary 规则由 test 自建临时 `.git` 目录；committed fixture 本身不被误当 live private case。
- `ModeEvidence` 从类型上分开 replay/live。`ProviderRole` 只存在于 `Live` variant，并且只接受 `targetVolcengine` 或 `approvedReference`；顶层 `RunManifest` 不重复 providerRole，Replay 没有该字段且不能伪造一个 live token。两个 live manifest 必须绑定同一 attestation、provider budget evidence、approval commitment、provider endpoint commitment、arm-order commitment、pair 总上限与 retention deadline；broker receipt 通过 exact `frozenRunContextSha256`/`executionContextSha256` 绑定这些冻结字段，后处理必须重算 context hash，不能把间接绑定误写成 receipt 的重复明文字段，也不得替换审批证据。Replay 只能绑定 committed fixture set SHA，不伪造审批/预算证据，`execution_mode` 与 enum variant 不一致即拒绝。typed round-trip/JSON Schema 测试必须拒绝未知 role、Replay 携带 role、Live 缺 role、顶层重复 role 及内外不一致 fixture。
- `ProofBrokerCompatibilityName` 是 pinned Codex proof broker 的单值 compatibility switch，序列化只能是精确 `"OpenAI"`；manifest 和 business-report Schema 使用 `const`/等价单值约束，测试拒绝 `"openai"`、真实供应商名和任意其他值，并证明它不能满足 `providerRole`、target evidence 或真实 `providerLabel`。
- evaluator request canonicalization 集成测试直接比较两臂完整 typed `TurnStartParams`，证明 root text、Schema 和唯一 `additionalContext` entry 的 key/value/kind 逐字节相同，且 kind 为 `Untrusted`；不为静态常量写重复值测试，也不让 runtime crate 依赖 App Server protocol。
- 真实 App Server → mock Responses 的集成测试放在 `codex-rs/app-server/tests/suite/v2/ai_ip_strict_output.rs`，复用该 package 已有 `TestAppServer`/mock Responses，解析实际 provider-visible request，断言 output format 使用 Work Package 4 转换后的 Schema 且 `strict=true`。app-server 只以 dev-dependency/Bazel test dependency 引入 `codex-ai-ip-runtime`；evaluator 的 Cargo/Bazel 单测只用 fake stdio server 验证客户端和 typed request deep equality，不偷偷依赖 target 目录里“恰好已构建”的 `codex` binary。
- evaluator/App Server mock Responses 集成测试还必须解析实际 provider-visible request，断言 output format 使用 Work Package 4 转换后的 Schema 且 `strict=true`；Work Package 4 本身不伪造该集成层。
- 候选臂 typed item 记录必须有一个在 root 或 descendant 中、final package 之前完成的成功本地工具访问；使用 pinned `codex-skills` implicit-access 解析识别其确实读取 canonical `SKILL.md`，并对工具返回的完整 Skill 字节复算源资产 SHA。只出现 catalog、只打印路径或读取失败不算使用；该归一化证据写入 `skill_use_evidence_sha256`。候选未实际使用 Skill 是有效业务试验的 `ITERATE_SMALLEST_LEAD_CHANGE`，不得把 catalog treatment 误报成 Skill 内容增益。

```bash
just test -p codex-ai-ip-eval
```

预期：FAIL，collector 尚不存在。

### 5.2 把现有 Responses proxy 抽成可门控的库服务

#### Contract 3：先写 proxy gate/observer 失败测试

在 `responses-api-proxy` 增加测试：只接受 `POST /v1/responses`；删除调用方的 Authorization/Host 后注入锁内 bearer；gate 拒绝时不上游；上游失败只记 attempt 不记 completion；SSE parser 只提取 `response.completed` 的 response ID/usage 并不保留正文；permit deadline 会设置单请求 timeout；redirect 被拒绝；shutdown 有界等待 in-flight 清零；`wait()` 不会抢先 shutdown；现有 `--server-info`/`--http-shutdown`/`--dump-dir` 参数和行为不回归。还要用 mock upstream 先 RED 证明：evaluator-only transform 在有界 JSON 中拒绝调用方已带/非整数 `max_output_tokens`，插入冻结 u64，重建 Content-Length，upstream 收到精确值，且 observer 只看到 post-transform digest/长度而看不到 body。

```bash
just test -p codex-responses-api-proxy
```

预期：FAIL，新接口不存在。

#### Contract 4：实现最小可复用 server surface

`broker.rs` 导出同步、线程安全接口；新增 public trait 必须按上游 `AGENTS.md` 写明角色、线程/时限约束和调用顺序的 doc comments。禁止把 request/response body 传给 observer：

`codex-rs/responses-api-proxy/Cargo.toml` 增加 `sha2 = { workspace = true }`，用于 post-transform digest；新测试放在 sibling `broker_tests.rs`，由 `lib.rs` 的 `#[cfg(test)] mod broker_tests;` 登记，不在新 `broker.rs` 内塞 inline test module。

```rust
pub struct ProxyConfig {
    pub listen_port: Option<u16>,
    pub upstream_url: reqwest::Url,
    pub dump_dir: Option<std::path::PathBuf>,
    pub http_shutdown: bool,
    pub default_request_timeout: Option<std::time::Duration>,
    pub request_transform: Option<RequestTransformConfig>,
}

pub struct RequestTransformConfig {
    pub max_output_tokens: u64,
    pub max_body_bytes: usize,
    pub inspector: std::sync::Arc<dyn RequestInspector>,
}

pub struct TransformedRequestMetadata {
    pub content_length: usize,
    pub sha256: [u8; 32],
    pub evidence: TransformedRequestEvidence,
}

pub struct TransformedRequestEvidence {
    pub raw_sha256: [u8; 32],
    pub normalized_sha256: [u8; 32],
    pub normalized_base_commitment: [u8; 32],
    pub treatment_diff_commitment: Option<[u8; 32]>,
}

pub trait RequestInspector: Send + Sync + 'static {
    fn inspect(
        &self,
        post_injection_body: &serde_json::Value,
    ) -> anyhow::Result<TransformedRequestEvidence>;
}

pub struct RequestMetadata {
    pub method: String,
    pub path: String,
    pub window_id: Option<String>,
    pub parent_thread_id: Option<String>,
    pub is_subagent: bool,
}

pub struct ResponseCompletedMetadata {
    pub response_id: String,
    pub usage: Option<ObservedUsage>,
    pub actual_model: Option<String>,
    pub deployment_or_fingerprint: Option<String>,
}

pub trait RequestGate: Send + Sync + 'static {
    fn before_forward(
        &self,
        request: &RequestMetadata,
        transformed: &TransformedRequestMetadata,
    ) -> anyhow::Result<RequestPermit>;
    fn after_forward(&self, permit: RequestPermit, result: &ForwardResult);
}

pub trait ExchangeObserver: Send + Sync + 'static {
    fn response_completed(&self, permit: &RequestPermit, event: &ResponseCompletedMetadata);
}

pub struct RunningProxy {
    addr: std::net::SocketAddr,
    shutdown_tx: Option<std::sync::mpsc::SyncSender<()>>,
    completion_rx: std::sync::mpsc::Receiver<anyhow::Result<()>>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl RunningProxy {
    pub fn addr(&self) -> std::net::SocketAddr { self.addr }

    pub fn wait(mut self) -> anyhow::Result<()> {
        let result = self.completion_rx.recv()?;
        self.join_thread()?;
        result
    }

    pub fn shutdown_with_timeout(mut self, timeout: std::time::Duration) -> anyhow::Result<()> {
        if let Some(tx) = self.shutdown_tx.take() {
            tx.send(())?;
        }
        let result = self.completion_rx.recv_timeout(timeout)?;
        self.join_thread()?;
        result
    }

    fn join_thread(&mut self) -> anyhow::Result<()> {
        let Some(join) = self.join.take() else {
            anyhow::bail!("proxy thread already joined");
        };
        join.join().map_err(|_| anyhow::anyhow!("proxy thread panicked"))
    }
}

pub fn bind(config: &ProxyConfig) -> anyhow::Result<BoundProxy>;

pub fn activate(
    bound: BoundProxy,
    config: ProxyConfig,
    auth: LockedAuthHeader,
    gate: std::sync::Arc<dyn RequestGate>,
    observer: std::sync::Arc<dyn ExchangeObserver>,
) -> anyhow::Result<RunningProxy>;
```

把 `read_auth_header_from_stdin()` 与其锁内返回类型公开给同 workspace 的 evaluator。`RequestPermit` 携带不可延长的 absolute deadline，forwarder 用 remaining duration 设置 `RequestBuilder::timeout`；过期前不能发请求，挂起 SSE 也必须在 deadline 后退出。reqwest Client 固定 redirect policy `none` 且 `.no_proxy()`；evaluator 启动时拒绝或清除 `HTTP_PROXY`/`HTTPS_PROXY`/`ALL_PROXY` 及小写异体，测试用恶意 proxy env 证明 bearer/请求永不到达代理。这里严格区分两层路径：Codex 访问本地 broker 的 ingress 仍只接受 `POST /v1/responses`；live upstream 则接受审批/冻结的 exact HTTPS origin + path，拒绝 userinfo/query/fragment。`providerRole=targetVolcengine` 时，host/region 与 attestation 的 Ark 合同证据一致且 canonical path 为 `/api/v3/responses`；`approvedReference` 时必须精确等于 attestation 中的 endpoint。只有单元测试/本地 mock 可用 loopback HTTP。URL 只以私有 SHA/公开 HMAC commitment 绑定，不公开泄露。

legacy CLI 的 `request_transform=None`；evaluator 在 `activate` 时才传冻结的 `RequestTransformConfig`。transform 在 gate 之前仅在有界内存解析/改写，不发网络；插入 `max_output_tokens` 后，proxy 以短生命 `&serde_json::Value` 调用同进程 `RequestInspector`。evaluator inspector 在不保留正文的前提下对 ID/时间/Home 前缀归一化，只在内存副本中删除 candidate 的目标 Skill catalog fragment，并返回 typed digest/commitment evidence；额外 system prompt/tool spec/metadata 差异直接报错。gate 以 `TransformedRequestMetadata` 写入 pre-forward attempt，不接收 body；upstream send 完成后原始/归一化 buffer 立即 zeroize，observer 永远只看 evidence。RED 测试必须证明 dynamic ID 被归一、唯一 Skill fragment 可删、多一个字段就 poison。这一能力属于 Step 3/4 和 4A commit，paired runner commit 不再偷改 proxy API。

SSE tee 最多保留当前一条 event 的有界缓冲，识别结束后立即清零；live proof 固定 `dump_dir=None` 且 `http_shutdown=false`。parser 同时提取非正文 `response.model` 与 provider 可用的 deployment/system fingerprint，要求整 pair 实际 revision 一致；若 provider 不回传，attestation 只能使用有 provider 合同证据的不可变 revision ID，浮动 alias 无资格 PASS。

broker 在内存中解析每臂第一个 root `/v1/responses` request，在不存储 body 的前提下对 ID/时间/Home 前缀做规范化，并用私有 commitment key 计算全请求 commitment。然后用 pinned Skill catalog parser 只从 candidate 删除目标 Skill 的 catalog metadata/path 片段；删除后两臂 provider-visible root payload 必须字节相同，structural diff 必须只有该片段。三个 commitment 写入 manifest，raw body/diff 立即 zeroize；后续请求可因 treatment 产生轨迹差异，但仍受 gate/usage 绑定。测试要求除 target Skill 外任一 system prompt/tool spec/metadata 差异都 poison。

`bind` 只绑定 loopback listener，不接收请求；完成 config/Home/sandbox preflight 后才从 stdin 读 key 并 `activate`。`run_main(Args)` 走 `bind → activate → wait`，使用 allow-all gate/no-op observer，并保留旧 CLI 的 server-info、HTTP shutdown 和 dump 语义。`shutdown_with_timeout` 通过 completion channel 有界等待后才 join，不出现 workspace 禁止的 `expect()`。

```bash
just test -p codex-responses-api-proxy
```

预期：PASS。

#### Contract 4A：封存 proxy 兼容边界

```bash
just bazel-lock-update
just test -p codex-responses-api-proxy
bazel test //codex-rs/responses-api-proxy:responses-api-proxy-unit-tests
just bazel-lock-check
just fmt
just fix -p codex-responses-api-proxy
git add codex-rs/Cargo.toml codex-rs/Cargo.lock codex-rs/responses-api-proxy MODULE.bazel.lock docs/architecture/codex-fork-patch-ledger.md
git commit -m "refactor: expose gated Responses proxy server"
```

`fmt/fix` 后只审阅 diff 并提交，不重跑同一测试。这个 commit 不得包含 evaluator/App Server 变更。

### 5.3 实现外部 pinned App Server stdio 客户端

#### Contract 5：写握手、catalog、事件和关闭失败测试

`app_server.rs` 只复用 `codex-app-server-protocol`，不依赖 in-process App Server 或不支持 stdio child 的 `codex-app-server-client`。测试用一个假 stdio server 验证：请求 ID 路由；超时；未知 server request 返回 JSON-RPC `-32601` 并 poison；stderr 单独保存；stdin EOF 后有界等待；超时 kill；每行一个 JSON；任一 JSON 解析错误 fail closed。

精确握手：

1. 启动 `codex app-server --listen stdio:// --strict-config`；
2. `initialize`，`clientInfo = { name: "codex_ai_ip_eval", title: None, version: env!("CARGO_PKG_VERSION").to_owned() }`，capability `experimentalApi = true`；
3. 校验 response `codexHome` 等于该臂 canonical Home；
4. 发送 `initialized` notification；
5. 立即调用 typed `config/read { includeLayers: true, cwd: Some(canonical_eval_tree.to_str().ok_or_else(|| anyhow::anyhow!("non-UTF-8 eval tree"))?.to_owned()) }`，校验有效配置和层来源；`ConfigReadParams.cwd` 是 `Option<String>`，非 UTF-8 路径必须拒绝，不能 lossy 转换；
6. 立即调用 `configRequirements/read` （params 为 `None`），要求 response `requirements == null`；
7. 此前不允许其他 request。

```bash
just test -p codex-ai-ip-eval
```

预期：FAIL。

`--strict-config` 只保证语法严格，不禁用 Unix `/etc/codex/*`、Windows ProgramData、macOS MDM 或 enterprise managed layers。`config/read` 返回的 layers 会过滤 `PackagedDefaults`，而 evaluator 生成的 `$CODEX_HOME/config.toml` 本身是一个 `ConfigLayerSource::User { file, profile: None }`。因此每臂在 `skills/list`/`thread/start` 前的精确规则是：`layers.is_some()`；恰好一个 User layer，`file` 必须等于 canonical `$CODEX_HOME/config.toml`、`profile == None`、`disabledReason == None`；任何第二个 User 或 `profile: Some(_)` 一律失败，即便其 config 为空。磁盘 config bytes 要单独重读并与 typed builder 的 UTF-8/LF bytes 相等；`ConfigLayer.config` 是 JSON Value，只与 builder parse 后的 canonical JSON 深比较，不能拿 TOML bytes 直接比较。其他平台占位 layer 仅在 `config == {}` 且无 origin 指向时可接受；任何非空 Mdm/System/EnterpriseManaged/Project/SessionFlags/Legacy 均失败。每个 origin 的完整 `(name, version)` 必须深等唯一 User layer metadata。由于 requirements 在 effective config 中已被 apply 且可从 origins 过滤，还必须单独要求 `configRequirements/read.requirements == None`。最后将 effective config 与 expected typed config 加明确允许的内建派生默认做深比较，递归拒绝额外 provider/profile、MCP、plugin/app、developer/model instruction、approval/permission override、网络开启或 auth 来源。路径归一化后的 effective config/layer/requirements provenance 在两臂必须字节相同，并绑入 `effective_config_sha256`/`config_layers_sha256`。假 server 和 pinned App Server 集成测试覆盖恶意 system layer、空/非空额外 User/profile、错误 origin metadata 与 requirements 被拒绝。

#### Contract 6：实现 `skills/list` 全字段归一化

每臂在 turn 前后各调用一次：

```rust
SkillsListParams {
    cwds: vec![case_dir.to_path_buf()],
    force_reload: true,
}
```

要求 response 仅一条 canonical cwd、`errors` 为空。对 `SkillMetadata` 全字段 `name, description, short_description, interface, dependencies, path, scope, enabled` 做 canonical JSON；路径先 canonicalize，再把不同实体前缀替换为 `$CODEX_HOME`、`$HOST_HOME`、`$CASE`，Windows 分隔符转 `/`，按 `(scope,name,path)` 排序后 SHA-256。

通用臂目标 Skill 为 0；候选臂目标 Skill 恰好 1、`scope=user`、`enabled=true`、路径 `$CODEX_HOME/skills/deliver-ai-ip-content-package/SKILL.md`，文件字节与源码资产相同。候选剔除此项后必须与通用 catalog 逐字节相同；每臂 pre/post hash 必须相同。

#### Contract 7：实现原生 thread/turn 流程与整树静止判定

`thread/start` 固定：model/provider/cwd；`approvalPolicy=never`；`permissions="ai-ip-eval"`；`ephemeral=false`；`experimentalRawEvents=true`。不能使用 ephemeral：上游对子线程的持久 spawn edge 会在 ephemeral 模式跳过，无法用 ancestor 查询证明整树完整。Home 位于 Git 外、单次可销毁，retention 到期再处理。

`turn/start` 固定：

```rust
TurnStartParams {
    thread_id: root_id.clone(),
    input: vec![UserInput::Text {
        text: root_prompt().to_string(),
        text_elements: vec![],
    }],
    additional_context: Some(HashMap::from([(
        ADDITIONAL_CONTEXT_KEY.to_string(),
        AdditionalContextEntry {
            value: evaluation_context(&mission_case)?,
            kind: AdditionalContextKind::Untrusted,
        },
    )])),
    output_schema: Some(content_package_schema()?),
    permissions: Some("ai-ip-eval".into()),
    approval_policy: Some(AskForApproval::Never),
    ..Default::default()
}
```

权威输出是匹配 root thread/turn 的 `turn/completed`：status 必须 `completed`，items 中恰好一个非空 `AgentMessage.text`，反序列化后依次调用 `ContentPackage::validate()` 和 `ContentPackage::validate_against(&mission_case)`；runner 已在运行前对 mission case 中的 regular-file digest 复算并锁定。同 root turn 的最后 `item/completed` AgentMessage 必须同字节。`blind-pack` 重读时必须对私有 case/material 再次执行同一 `validate_against` 与 digest 校验，不信任 manifest 自报。

整树事件集维护 `thread/started`、`turn/started`、`turn/completed`、`thread/status/changed`、`rawResponse/completed`。根完成后分页调用 `thread/list`，设置 `ancestorThreadId=root`、`limit=100`，跟随全部 cursor；`thread/loaded/list` 也设 `limit=100` 并跟完全部 cursor，再以 `thread/read(includeTurns=false)` 交叉检查相同 `sessionId` 的非根线程全部在 ancestor 集中。任一列表页重复 cursor、超过有界页数或中途变更均 poison。集成 fixture 必须形成 root→child→grandchild，加一个 root sibling，证明 ancestor 查询覆盖任意深度而不是只测一代 child。

pinned App Server 明确不把 GuardianReview 或不参与 persisted spawn-edge lifecycle 的异常 Subagent 线程放入 ancestor 集，因此 Phase 0A 的共享 eval config 显式设置 `approvals_reviewer = "user"`、`features.guardian_approval = false`、`features.guardianv2.enabled = false`，typed evaluator 不暴露 `review/start`。config audit 必须证明这三个有效值没有被 managed/model config 改回。若仍出现 `ThreadSource::GuardianReview`、Guardian lifecycle 通知，或 parent 不在 known set 的 Subagent，collector 标记本次不可封账；broker 的 pre-forward poison 行为与 upstream-count 测试归 Contract 9/12，而不塞进 App Server seam commit。这是 paired eval 的计量控制变量，不删除或限制正式产品的 Review/Guardian 能力。

只有以下条件同时成立才封账：每个已见 turn terminal；descendant 无 `Active`/`SystemError`；所有 raw event thread 属于根或 descendant；broker in-flight 为 0；做完一次完整查询后经历 2 秒无相关事件，再做第二次完整查询，两个 `(id,parent,session,status)` 集完全相同。封账前还必须在私有内存中按 `responseId` 对 broker `response.completed` 与 App Server `rawResponse/completed` 做双向一一对应，比较每项全部 usage 字段；不只比 count。有缺失、多出、重复冲突或 usage 不等均 poison，匹配完成后才丢弃 ID 并保留集合 commitment。否则 interrupt 所有 active turn、关闭 stdin、判失败。

正常关闭没有 shutdown RPC：drop child stdin，让 stdio EOF 触发 App Server 清理并有界等待进程退出；超时才 kill。

#### Contract 7A：封存 typed stdio client、config audit 与 collector

```bash
just bazel-lock-update
just test -p codex-ai-ip-eval
bazel test //codex-rs/ai-ip-eval:ai-ip-eval-unit-tests //codex-rs/ai-ip-eval:codex-ai-ip-eval-bin-unit-tests
just bazel-lock-check
just fmt
just fix -p codex-ai-ip-eval
git add codex-rs/Cargo.toml codex-rs/Cargo.lock codex-rs/ai-ip-eval MODULE.bazel.lock
git commit -m "test: add typed App Server evaluation client"
```

此 commit 只包含模型、stdio/config/catalog/tree collector 与它们的测试；不包含 live pair 协调器。

#### Contract 8：增加 pinned App Server 的 descendant raw-event 与 strict-output 集成测试

使用现有 App Server mock Responses test server：root 模型产生两个 `spawn_agent` tool call；第一个 child 再创建 grandchild，第二个是 root sibling，四个线程分别得到独立 mocked `response.completed` usage，root 收回全部结果后完成。断言同一外部连接收到 root、child、grandchild、sibling 的全部 `rawResponse/completed`，每条 raw notification 只断言其真实字段 `(threadId,turnId,responseId,usage)`。parent edge 从 `thread/started.thread.parentThreadId` 与 typed `thread/read` 两处交叉断言；root `turn/completed` 必须在三个 descendant terminal 之后。

`ai_ip_strict_output.rs` 用 runtime 生成的 `ContentPackage` Schema 发真实 V2 `turn/start`；mock upstream 解析实际 `text.format.schema`，与同次 `content_package_schema()` 返回值做整对象 deep equality，断言 `strict=true`、根没有 `$schema`/`definitions`、全部 local ref 只允许 `#/$defs/...` 且无悬空 ref，再返回有效 package。这是 strict schema 的唯一真实 App Server 集成归属。

```bash
just test -p codex-app-server --test all -E 'test(=suite::v2::raw_response_subagents::raw_events_cover_root_child_grandchild_and_sibling)'
just test -p codex-app-server --test all -E 'test(=suite::v2::ai_ip_strict_output::turn_start_sends_strict_content_package_schema)'
```

先运行这两条 focused 测试，把它们当作 pinned App Server seam 的 characterization：若两条直接 GREEN，记录为 test-only seam 证据并继续，绝不能为了制造 RED 主动破坏上游正确行为。只有测试复现 descendant listener race、strict wire 漂移或 parent edge 错误时，才进入 RED→最小修复；“测试尚未登记”、依赖安装失败或无关编译错误都不是有效 RED。若 pinned Codex 实现真的暴露 attach race，只在 `thread_processor.rs::try_attach_thread_listener` 写最小修复和回归，并在 patch ledger 标为 **直接修改**；不得在 evaluator 猜补 usage。没有证实 race 时不得修改该文件。实现后的 package/focused 测试和 `fix` 统一放在 Contract 8A，避免 `fix` 后再跑同一测试。

#### Contract 8A：封存 descendant raw-event seam

```bash
just bazel-lock-update
just test -p codex-app-server --test all -E 'test(=suite::v2::raw_response_subagents::raw_events_cover_root_child_grandchild_and_sibling)'
just test -p codex-app-server --test all -E 'test(=suite::v2::ai_ip_strict_output::turn_start_sends_strict_content_package_schema)'
if ! git diff --quiet -- codex-rs/app-server/src/request_processors/thread_processor.rs; then just test -p codex-app-server; fi
bazel test //codex-rs/app-server:app-server-all-test
just bazel-lock-check
just fmt
just fix -p codex-app-server
git add codex-rs/Cargo.lock MODULE.bazel.lock codex-rs/app-server/Cargo.toml codex-rs/app-server/BUILD.bazel codex-rs/app-server/tests/suite/v2/mod.rs codex-rs/app-server/tests/suite/v2/raw_response_subagents.rs codex-rs/app-server/tests/suite/v2/ai_ip_strict_output.rs codex-rs/app-server/src/request_processors/thread_processor.rs docs/architecture/codex-fork-patch-ledger.md
git commit -m "test: cover descendant raw response usage"
```

若 production seam 未改，`git add` 不会把该 path 放入 commit；若改了，patch ledger 必须指向先前失败的精确测试。pinned BUILD 已生成稳定 `//codex-rs/app-server:app-server-all-test`，不得降级成 Cargo-only 后声称 Bazel 边界已验证。

### 5.4 原子 live-pair、隔离 Home 与 broker gate

#### Contract 9：写 gate 状态机失败测试

状态必须只有：

```text
Created
→ OrderCommitted(first=CSPRNG(generic|candidate), second=other)
→ Active(run=1, first, root, knownThreads)
→ Sealed1
→ Active(run=2, second, root, knownThreads)
→ Finished
```

审批且 candidate 冻结后，`live-pair` 在读 bearer/发请求前由 OS CSPRNG 生成 32-byte `arm-order-seed.bin`，以 `create_new` + owner-only 权限写入私有 coordinator 目录，由其均匀选择第一臂。live 不接受调用方指定 seed/order；seed commitment 同时绑入两臂 manifest 和 broker receipt。这不把单 pair 升格为因果结论，但避免 treatment 与永久第二次运行完全共线。

任一非法转移进入 `Poisoned`。测试拒绝：order commitment 前、arm 前、两臂之间、结束后或第三臂请求；两臂 condition 重复；非 `POST /v1/responses`；超过每臂 attempt cap；deadline 后；无法解析 `x-codex-window-id`；root 不匹配；child 的 `x-codex-parent-thread-id` 不在 known set。还要注入 `ThreadSource::GuardianReview`、Guardian lifecycle 与 parent 不在 known set 的异常 Subagent 请求，断言先写 poison、forward 不发生且 upstream request count 保持 0。合法 child 请求把自己的 thread ID 加入本臂树。每次在转发前原子递增 attempt；Codex provider retries 固定 0，失败 attempt 直接 poison；这些 gate 测试到 Contract 12 的 paired-runner commit 才 GREEN。

私有 attempt index 不是一个最终 count。每次 HTTP 转发前必须先 append + `fsync` 一条 request record，exact 字段为 `{schemaVersion,recordType:"request",status:"forwarding",pairId,frozenRunContextSha256,executionContextSha256,globalAttemptIndex,runOrdinal,condition,armAttemptIndex,requestStartedAt,requestCommitment,normalizedRequestCommitment,normalizedBaseCommitment,treatmentDiffCommitment:string|null,threadCommitment,windowCommitment,parentThreadCommitment:string|null,deadline,maxOutputTokens}`。其中三个 normalized/treatment commitment 是已评审 Plan 05 为证明两臂归一化基线相同且只存在授权 treatment 差异而追加的 wire 修订；generic 的 `treatmentDiffCommitment=null`，candidate 的值必须与候选 treatment 证据绑定。completion/failure/timeout 必须再 append + `fsync` 一条 terminal record，exact 字段为 `{schemaVersion,recordType:"terminal",globalAttemptIndex,requestRecordSha256,endedAt,status:"completed|failed|timeout",responseIdCommitment:string|null,actualModelRevision:string|null,deploymentCommitment:string|null,usage:Usage|null,failureClass:string|null}`；三种状态不适用的字段写 JSON `null` 而不是省略。terminal 的 `globalAttemptIndex` 与 `requestRecordSha256` 必须唯一反向绑定先前 request，不能凭数组位置猜测。全局 request index 严格为 `0..N-1`，每臂 index 严格为 `0..n-1`，每条 request 恰好一条 terminal，无 gap/重复/改写。

每臂 sealed broker receipt 的 exact 字段为 `{schemaVersion,pairId,frozenRunContextSha256,executionContextSha256,runOrdinal,condition,firstCondition,secondCondition,attemptIndexFileSha256,attemptIndexMerkleRoot,globalAttemptStartInclusive,globalAttemptEndExclusive,attemptCount,completionCount,failureCount,timeoutCount,inFlight,sealedAt,previousArmReceiptSha256}`；第一臂 `previousArmReceiptSha256=null`，第二臂必须等于第一臂 receipt SHA，`inFlight=0`。pair final receipt 再绑定两臂 receipt SHA、总 attempt/completion/failure/timeout counts、arm-order commitment 和最终 attempt-index root。两份 execution manifest、cost receipt 和 private verifier都从 append-only index 重算所有 receipt 字段。

pinned `ResponsesApiRequest` 不会自动发 `max_output_tokens`，所以不能只在 manifest 写一个上限就称为硬闸。loopback broker 在 forward 前解析有界 JSON，拒绝调用方已携带、非整数或冲突的 `max_output_tokens`，再插入 attestation 冻结的 u64 值；以插入后的 upstream bytes 做两臂 parity/commitment。mock upstream 必须断言确实收到该值；上限/第三臂/超 attempt 被拒时 upstream request count 不增加。

#### Contract 10：生成一次并复制的共享 config

`live-pair` 接收 model、provider upstream URL 与 limits，broker 绑定 `127.0.0.1:0` 后只生成一次下列 UTF-8/LF config bytes，再复制到两个 Home：

```toml
model = "${APPROVED_MODEL_LABEL}"
model_provider = "ai-ip-proof-broker"
approval_policy = "never"
approvals_reviewer = "user"
default_permissions = "ai-ip-eval"
project_doc_max_bytes = 0

[model_providers.ai-ip-proof-broker]
name = "OpenAI"
base_url = "http://127.0.0.1:${BROKER_PORT}/v1"
wire_api = "responses"
requires_openai_auth = false
request_max_retries = 0
stream_max_retries = 0
supports_websockets = false

[agents]
enabled = true
max_concurrent_threads_per_session = 4

[features]
guardian_approval = false

[features.multi_agent_v2]
enabled = true

[features.guardianv2]
enabled = false

[permissions.ai-ip-eval.filesystem]
":minimal" = "read"
":workspace_roots" = "read"
"~/.codex/skills" = "read"

[permissions.ai-ip-eval.network]
enabled = false

[shell_environment_policy]
inherit = "core"
ignore_default_excludes = false

[skills.bundled]
enabled = false
```

`${...}` 由 evaluator 内的 typed builder 写入真实已批准值；模板本身不传给 shell。配置必须拒绝 `env_key`、bearer、headers、MCP、plugins、developer instructions、model instruction file、profile 和 `auth.json`。

provider id 可以是 `ai-ip-proof-broker`，但 friendly `name` 必须精确为 `OpenAI`：pinned `ModelProviderInfo::is_openai()` 依赖该名字开启 Codex 原有的 OpenAI capability/metadata 行为；它只是 compatibility switch，不是实际供应商声明。私有 manifest 和公开 report 必须另记 approved sanitized `providerLabel`/`modelLabel`，并明确 `providerCompatibilityName=OpenAI`。本地 broker 只改变凭证和计量路径，不得无意把 Codex 降级成 generic provider。

所有派生私有路径都必须在 attestation 记录的 canonical `privateRoot` 内由 coordinator 以 owner-only 权限新建：两臂 host Home、`CODEX_HOME=$HOME/.codex`、`skills`、App Server stderr/transcript/rollout/state/cache/tmp、broker ledger、reviewer/mapping/reviews 和 commitment keys。不允许 symlink、hardlink（regular file `nlink` 必须 1）、Windows junction/reparse point 或路径竞态；每次打开重做 containment 检查。App Server child 的 HOME/TMP/TEMP/TMPDIR 全指向本臂子目录，不得把 `ephemeral=false` rollout 或正文留在系统 temp/privateRoot 外。

两臂使用字节相同的 `"~/.codex/skills" = "read"`；generic 该目录存在但为空，candidate 只有 `skills/<name>/SKILL.md`。preflight 必须证明 candidate 能完整读取 Skill、generic 无目标 Skill，且两臂均不能读取 skills 根外的 credential sentinel。App Server child 使用 `env_clear()`，只恢复当前平台启动所需的非秘密 OS 变量、该臂 `HOME/USERPROFILE`、`CODEX_HOME` 与冻结 `PATH`；不得继承 `*KEY*`、`*TOKEN*`、`*SECRET*`、代理或云凭证变量。

#### Contract 11：实现唯一 live surface

CLI 只有以下模式：

```text
codex-ai-ip-eval replay-pair ...
codex-ai-ip-eval freeze-run-context ...
codex-ai-ip-eval live-pair ...
codex-ai-ip-eval blind-pack ...
codex-ai-ip-eval score ...
codex-ai-ip-eval make-cost-receipt ...
codex-ai-ip-eval annotate-cost ...
codex-ai-ip-eval summarize ...
codex-ai-ip-eval verify-report ...
codex-ai-ip-eval publish-report ...
codex-ai-ip-eval verify-live-proof ...
codex-ai-ip-eval finalize-checkpoint ...
codex-ai-ip-eval retention-closeout ...
```

不得暴露独立 live single-arm `run`。`freeze-run-context` 必须用 Clap nested subcommand/typed enum 表达互斥模式，不能把 live 字段做成一组可遗漏的 `Option`：

```rust
#[derive(Debug, clap::Subcommand)]
enum FreezeRunContextArgs {
    Replay(ReplayFreezeArgs),
    Live(LiveFreezeArgs),
}
```

`ReplayFreezeArgs` 只允许 frozen evaluator repo/SHA、private root、Codex binary、case fixture、transcript fixture、`replay-fixture-set.json` 和 output；程序复算 fixture-set manifest 及其列出的每个字节，context 写入 `executionMode=replay` 与 `fixtureSetSha256`。它拒绝 attestation、approval、provider、endpoint、budget、rate/billing/FX、money/token/time 等 live 参数。`LiveFreezeArgs` 则要求下面 Work Package 8 列出的全部审批/供应商/预算/限制字段并拒绝 transcript/fixture-set。`frozen-run-context.schema.json` 用 `oneOf` + `executionMode` const 冻结两支；Rust typed round-trip、JSON Schema 和 Python wrapper 各有合法 replay/live fixture，并测试交叉字段必然失败。

`freeze-run-context` 是不读 bearer、不发网络的静态冻结步骤。两种模式都以 OS CSPRNG + `create_new` + owner-only + `fsync` 生成 `coordinator/commitment-key.bin`，由 key 派生 pair/public run ID 与 case/material commitments，再原子写私有 `frozen-run-context.json`。共同字段至少绑定 `schemaVersion/executionMode/createdAt/publicRunId/pairId/candidateSha`、case/material commitments、key commitment、Prompt/additionalContext/Schema、Codex/evaluator/broker/Skill/config/source/lock hashes、精确二进制绝对路径与平台后缀和 private root。live 分支另外绑定 candidate freeze/case-selected/approval timestamps、requested model/provider/providerRole/endpoint commitment、attestation/approval/budget/rate-card/billing-policy/FX-policy commitments、pair/per-arm money、attempt/token/time/每请求 `maxOutputTokens` 上限与 retention deadline。

live context 本身是 owner-only 私有文件；其所有运行专用 attestation/budget/rate-card/billing-policy/FX-policy 引用必须位于受 inventory 管理的 `privateRoot/inputs`，case/material 的运行副本也由 freeze 以 `create_new` 导入该目录。context 只保存这些私有副本和 frozen evaluator/main evidence worktree 的 canonical absolute path + bytes digest，以及已批准的明文 HTTPS endpoint 与 commitment。`live-pair` 按 path 打开后重算字节，不接受同名命令行覆盖。未来才会出现的 supplier statement 不进 frozen context；context 只能冻结其允许的 private-root drop-directory policy，运行后由 cost receipt 绑定 statement 字节与 SHA。context 不包含 bearer、材料正文或动态 broker port。原始用户项目素材仍由用户在其项目中保留，不属于 proof-copy retention；该外部保留事实和 owner 必须在私有 source-retention disclosure 与最终用户报告中明示。

`live-pair` 只接受 `--frozen-run-context`，不再重复接收 model/provider/case/binary/limits 以防止两份权威。它在单一进程内按 `bind(no accept) → 重验 static context/冻结引用/投影/Home/config/catalog/sandbox → 用 OS CSPRNG 创建 arm-order seed → create_new+fsync execution-context.json 绑定 frozen SHA/key commitment/动态 port/config/order commitment/first+second/startedAt/deadline → 再读 bearer → activate`执行。bound listener 不跨进程重绑；`execution-context.json` 与 pair marker 同时绑定 frozen context SHA。然后严格按已承诺顺序各运行 generic/candidate 一次。每臂在 `thread/start` 返回 root ID 后、`turn/start` 前 arm gate；每臂完成整树静止与 App Server clean exit 后 seal；第二臂 receipt 链接第一臂 receipt hash。

源码、case、材料在每臂前后复算；每臂前后运行：

```bash
git rev-parse HEAD
git status --porcelain=v1 --untracked-files=all
```

必须等于冻结 SHA 且完全为空。`current_exe()`、Codex binary、broker component source、共享 config、Skill asset、request canonical JSON 和 transcript 全部哈希。任何一项变化即使输出漂亮也作废。

#### Contract 12：通过 paired runner 测试并提交第四个 GREEN 边界

```bash
just test -p codex-ai-ip-eval
just test -p codex-responses-api-proxy
just bazel-lock-update
bazel test //codex-rs/ai-ip-eval:ai-ip-eval-unit-tests //codex-rs/ai-ip-eval:codex-ai-ip-eval-bin-unit-tests
just bazel-lock-check
just fmt
just fix -p codex-ai-ip-eval
git add codex-rs/Cargo.toml codex-rs/Cargo.lock codex-rs/ai-ip-eval MODULE.bazel.lock docs/architecture/codex-fork-patch-ledger.md
git commit -m "test: add atomic Codex AI IP paired runner"
```

不要在 `fix/fmt` 后重复以上测试；下一任务用新测试继续前进。Work Package 5 必须得到四个可独立审查的 GREEN commit：proxy，typed stdio/config/collector，descendant seam，paired runner；不得 squash 成一个巨型 commit。

---

## Work Package 6：冻结盲评、成本绑定与无正文结论

**文件：**

- 新建：`ai-ip-evals/rubrics/content-package-blind-review.json`
- 新建：`ai-ip-evals/rubrics/reviewer-submission.schema.json`
- 新建：`ai-ip-evals/rubrics/BUILD.bazel`
- 新建：`ai-ip-evals/schemas/held-out-attestation.schema.json`
- 新建：`ai-ip-evals/schemas/cost-receipt.schema.json`
- 新建：`ai-ip-evals/schemas/{frozen-run-context.schema.json,business-report.schema.json,report-index.schema.json,attempt-index.schema.json,verification.schema.json,retention-closeout.schema.json}`
- 新建：`ai-ip-evals/schemas/BUILD.bazel`
- 新建：`codex-rs/ai-ip-eval/src/{blind.rs,score.rs,cost.rs,commitment.rs,report.rs,verify.rs,publish.rs,checkpoint.rs,retention.rs}`
- 修改：`codex-rs/ai-ip-eval/src/{lib.rs,main.rs,model.rs,eval_tests.rs}`
- 修改：`codex-rs/ai-ip-eval/BUILD.bazel`
- 新建：`docs/evidence/business-proof/README.md`

### 6.1 冻结 reviewer contract

#### Contract 1：先写篡改与错误决策的失败测试

测试至少覆盖：

- 任一 bound file 被改、缺失或换目录，`blind-pack` 拒绝；
- 两臂 fork/case/material/prompt/additionalContext/schema/thread request/shared config/Codex/evaluator/broker component/model/provider/limits 任一不等，拒绝；
- generic 有目标 Skill、candidate 没有或不止一个、候选剔除目标项后的 catalog 不等，拒绝；
- mapping 出现在 reviewer dir，拒绝；
- 任一 ContentPackage 正文包含目标 Skill 名/路径、`candidate`/`generic`、Home 前缀或其他 treatment-only 标识，拒绝且不发给 reviewer；
- 三名 reviewer 共用同一 A/B mapping 或 seed，拒绝；每人必须有独立 CSPRNG permutation 和独立私有 mapping；
- 少于/多于 3 份 review、reviewer ID 重复、Schema 外字段、分数越界，拒绝；
- 少于 2 名 reviewer 在私有 attestation/submission 中声明并签名 `experiencedOperatorOrDirector=true`，不能 PASS；公共 report 只记合格人数，不记身份或经历正文；
- candidate 只有总分漂亮但存在严重事实/主体/权利失败，不能 PASS；
- evaluator 当前二进制哈希与 manifest 不同，所有后处理命令拒绝。

```bash
just test -p codex-ai-ip-eval
```

预期：FAIL。

#### Contract 2：创建冻结 rubric

`content-package-blind-review.json` 固定 6 个 0–4 分维度：

1. `businessOutcomeClarity`：是否明确真实结果而不是泛流量；
2. `subjectAudienceActionFit`：主体、受众、影响关系和行动漏斗是否成立；
3. `strategicJudgment`：是否做了与材料相关的取舍，而非套模板；
4. `publishableUsability`：内容能否直接拍摄/发布，hook/body/CTA 是否连贯；
5. `evidenceIntegrity`：事实、推断、创意假设和未知是否诚实；
6. `measurementUsefulness`：主结果、观察信号和归因窗口是否可执行。

reviewer 还必须提交：`preferred = A|B|tie`、`readyForHumanReview.A/B`、每臂最多三条具体理由，以及四个 severe flags：`fabricatedFactualClaim`、`wrongSubjectOrDesiredAction`、`notActuallyUsable`、`rightsOrPrivacyViolation`。

`fabricatedFactualClaim=true` 的定义是：任何未标注的、没有来源支持的 subject/relation/object/modifier 附着，跨来源 entity/event bridge，actor 或 ordinal substitution，time/aspect drift，sequence 被提升为 cause/result，forecast 被提升为 completion，或 event/check-in count 被提升为 unique-person/set count。reviewer 从冻结 case/materials 对两臂评估事实完整性和业务可用性，但不得重写 package；完整理由仅留在私有 review submission，公开 report 不含正文。

自动决策规则冻结为：

```text
VALID：三名独立真人 reviewer，其中至少两名为有经验的运营/编导，所有输入/receipt/hash 验证通过。
PASS：
  - 至少 2/3 reviewer 选择 candidate；且
  - candidate 相对 generic 的 reviewer 总分差中位数 >= 3（满分 24）；且
  - candidate 总分中位数 >= 18；且
  - 至少 2/3 reviewer 标记 candidate readyForHumanReview=true；且
  - candidate 四类 severe flag 全部为 false，且三份 submission 的 `fabricatedFactualClaim` 均为 false。
任一 candidate `fabricatedFactualClaim=true`：ITERATE_SMALLEST_LEAD_CHANGE，属于有效业务失败而非 INVALID_PROOF。
其他有效结果：ITERATE_SMALLEST_LEAD_CHANGE。
任一证据/预算/隔离/成本绑定无效：INVALID_PROOF，不参与业务判断。
```

这就是 Phase 0A 的 **source-visible factual severe-failure gate**，不是 all-sample factual-fidelity acceptance。Lead/generator 负责选择题材、形式、业务连接和有用草稿，并标注 claims、hypotheses、unknowns 与 open questions；人类 reviewer 只基于冻结来源判断事实和业务质量，不重写 package。若将来证据支持 automatic guard，它只检查 factual support、只发出 violations，绝不选择 topic、CTA、team、production stage 或 workflow，创意输出仅由 Lead 修复。确定性代码负责 hashes、source identity、byte ranges、caps、retry count、release state 与 receipts；没有测量过的 guard evidence 时，semantic-model verdict 不得表述为确定性真相。

成本与 token/时延比率必须展示，但在批准硬上限内不得抵消内容业务 PASS；业务优先。若 candidate 更贵，进入后续优化项，而不是自动否决更好的业务能力。

### 6.2 实现 blind pack 与成对验证

#### Contract 3：绑定所有输入并隐藏条件

`blind-pack` 重新读取每臂目录，不信任 manifest 自报：

- 重建 case/material digest、`root_prompt()`、`evaluation_context()` 和 Schema；
- 解析 typed App Server transcript，重新提取唯一 final package与整树 usage；
- 重新计算 catalogs、request JSON、二进制、broker ledger 和所有文件 hash；
- 检查 `promptSha256` 与 `additionalContextSha256` 两臂相等；
- 对 `threadStartRequest`/`turnStartRequest` canonical JSON，在哈希前只把 root/turn/session ID 换成固定 token；归一化结果两臂相等；
- 唯一允许不同的是 `condition`、由共享 arm-order commitment 决定的 run ordinal、Home 路径归一化后的目标 Skill entry/hash、treatment-derived `skillUseEvidence` 以及输出正文和由输出产生的计量。generic 的 skill use 必须为 `None`；candidate 为 `Some` 才有资格 PASS，`None` 是有效 `ITERATE`。Skill tool aggregated output 必须未截断，去掉协议包装后的净 payload 必须与 asset bytes 精确一致，命令噪声或部分输出不得误判。

对三名 reviewer 分别从 OS CSPRNG 生成 32-byte seed，各自做 Fisher–Yates permutation，生成 `reviewer/reviewer-{1,2,3}/`：`A.json`、`B.json`、`case.json`、`materials/`、`materials-manifest.json`、`rubric.json`、`reviewer-submission.schema.json`、`review-bundle.json`。对输出 JSON 值做 fail-closed treatment-marker 扫描后才复制。材料只允许来自 case 中已经校验的相对路径、必须是 regular file、拒绝 symlink；复制后逐文件复算并要求 aggregate digest 与两臂 `sourceMaterialsSha256` 相同。三份 `mapping/reviewer-{1,2,3}.private.json` 必须写入另一个 coordinator-only 绝对目录；它与 reviewer 目录不能互为父子，且都在 Git 外。`score` 用 reviewer ID 只打开对应 mapping，不用一个共享顺序解盲。

review bundle 不包含 condition、Home、Skill、耗时、token、成本、线程 ID、日志文件名或可推断 mapping 的顺序字段。

`blind-pack` 的 live 模式使用 `--seed-dir`：从 OS CSPRNG 生成三个 32-byte seed，以 `create_new` 和 owner-only 权限（Unix `0600`；Windows 当前用户 ACL）写入 coordinator 目录；任一路径已存在即失败。Replay 不用 `--seed-dir`，而要求恰好三次重复的 `--replay-seed <value>`（Clap `Vec<String>`），三值必须显式不同；该参数在 live 拒绝。不得要求调用方预先提供一个不存在的 live seed 文件。两种模式的 `blind-pack` 还必须从 frozen context 解析 canonical privateRoot，做 containment/reparse/hardlink 检查后，以 owner-only 权限原子创建空的 `reviews/` drop-dir 并立即写入 inventory；已存在或非空即失败。操作员不得用另一个 shell 自己拼 `$PRIVATE_ROOT/reviews`。

`codex-rs/ai-ip-eval/BUILD.bazel` 必须把 `tests/fixtures/**` 作为 test runfiles（`test_data_extra = glob(["tests/fixtures/**"])`），并同时保留 Skill、rubric 和 schema 三个 filegroup。`ai-ip-evals/rubrics/BUILD.bazel` 的 `filegroup(name = "rubrics", ...)` 与 `ai-ip-evals/schemas/BUILD.bazel` 的 `filegroup(name = "schemas", ...)` 都使用精确 `visibility = ["//codex-rs/ai-ip-eval:__pkg__"]`，不能依赖 Bazel 默认 private visibility；schemas filegroup 必须显式包含本 Work Package 列出的每个 schema，包括 `report-index.schema.json` 与 `retention-closeout.schema.json`。eval 测试的 `find_resource!` 参数精确使用 `tests/fixtures/<file>`、`../../ai-ip-assets/skills/deliver-ai-ip-content-package/SKILL.md`、`../../ai-ip-evals/rubrics/<file>.json` 和 `../../ai-ip-evals/schemas/<file>.json`；禁止依赖 cwd。

完成 blind/score 边界后先独立验证并提交，不把整个 Work Package 6 塞进一个超大 commit：

```bash
just bazel-lock-update
just test -p codex-ai-ip-eval
bazel test //codex-rs/ai-ip-eval:ai-ip-eval-unit-tests //codex-rs/ai-ip-eval:codex-ai-ip-eval-bin-unit-tests
bazel build //ai-ip-evals/rubrics:rubrics //ai-ip-evals/schemas:schemas
just bazel-lock-check
just fmt
just fix -p codex-ai-ip-eval
git add ai-ip-evals codex-rs/ai-ip-eval codex-rs/Cargo.lock MODULE.bazel.lock
git commit -m "test: freeze AI IP blind review contract"
```

### 6.3 绑定成本、账户硬上限和 retention

#### Contract 4：实现成本 receipt

先增加 cost/live-only 与四个发布 verifier 的失败测试：replay、空 cost receipt、未绑定 broker receipt、broker/App Server `(responseId, usage)` 多重集不等、过期/倒置 RFC3339 retention、未来 `signedAt`、attestation/budget/endpoint/order commitment 不一致均不能 PASS；`verify-report` 拒绝额外字段、secret/path/ID 和错误 proof root；`verify-live-proof` 拒绝 attempt/cost/mapping/usage/timeline 任一篡改；`publish-report` 拒绝错误 destination/CAS/bytes，但在私有 transaction marker 精确匹配时能从第一文件已写的 crash 恢复；`finalize-checkpoint` 拒绝手工 PASS、过期、candidate 后 shipping code 改动、origin 伪造和不同 bytes 半写，只允许相同 marker/HEAD/bytes 的恢复。shipping-boundary fixtures 还必须有三条恶意闭环：customer Cargo target 把 `codex-ai-ip-eval` 变成 normal dependency、release Bazel filegroup 纳入 evaluator binary/private fixtures、installer/package manifest 纳入 `codex-ai-ip-eval` 或 `codex-responses-api-proxy` 二进制；三者都先 RED 后被 verifier 拒绝。另有 positive control：customer target 只复用经 allowlist 的 `codex_responses_api_proxy` 低层 library codec/server API、未打包 proxy binary、stdin credential surface 或 evaluator state 时必须通过，防止隔离测试误伤可复用底层能力。写完这些具体 fixture/test 后先运行下列 focused 命令观察 RED；RED 必须是对应 subcommand/行为不存在或返回错误结果，不能只是测试未登记。随后实现、重跑 focused/Cargo/Bazel 全部 GREEN，最后再与实现一起提交；不得提交一个故意失败的 RED commit。

```bash
just test -p codex-ai-ip-eval
```

预期：FAIL，cost/report 逻辑尚未实现。

私有 `cost-receipt.json` 只能由 `make-cost-receipt` 生成，必须包含：`frozenRunContextSha256`、`executionManifestSha256`、broker receipt SHA、condition/run ordinal、attempt-index root/range、供应商/实际模型 revision、事前冻结 rate-card SHA、billing-policy/source commitment、如需换汇时的 FX-policy/evidence SHA、供应商预算上限证据 SHA、attempt/completion counts、`usageScope=rootSessionTree`、完整 usage、计算时间、计算式、`estimatedFen`、可选的运行后 `supplierStatementSha256/supplierActualFen`、`chargedFen=max(estimatedFen,supplierActualFen)` 和 `withinCeilings`。两臂必须使用 frozen context/attestation/broker 事前绑定的同一 rate card/billing policy/FX policy，事后不得换价。最终供应商账单只能运行后进 receipt，不可能在 attestation 中预先写它的 SHA。

Rust 与 source input 可保留 `u64`，但所有写入 exact RFC 8785/JCS `CostReceiptV1` 的非负整数必须在 `0..=9_007_199_254_740_991`；超界在 publication 前失败，禁止取整或静默变更 wire 值。供应商 statement 的 arm/pair/provider/model 不一致由 receipt authority 依据预期执行臂拒绝，不能仅因其自身 JSON wire 有效而接受。

冻结费率单位为“每百万 token 的人民币分”：`uncachedInput`、`cachedInput`、`cacheWriteInput`、`output`。reasoning token 只做诊断；若供应商已把它包含在 output，不得二次计费。计算：

```rust
let uncached = input
    .checked_sub(cached)
    .and_then(|value| value.checked_sub(cache_write))
    .ok_or_else(|| anyhow::anyhow!("input token subtraction overflow"))?;
ensure!(uncached >= 0 && cached >= 0 && cache_write >= 0 && output >= 0);
let uncached = u128::try_from(uncached)?;
let cached = u128::try_from(cached)?;
let cache_write = u128::try_from(cache_write)?;
let output = u128::try_from(output)?;
let uncached_cost = uncached
    .checked_mul(u128::from(rate.uncached_input))
    .ok_or_else(|| anyhow::anyhow!("uncached cost overflow"))?;
let cached_cost = cached
    .checked_mul(u128::from(rate.cached_input))
    .ok_or_else(|| anyhow::anyhow!("cached cost overflow"))?;
let cache_write_cost = cache_write
    .checked_mul(u128::from(rate.cache_write_input))
    .ok_or_else(|| anyhow::anyhow!("cache-write cost overflow"))?;
let output_cost = output
    .checked_mul(u128::from(rate.output))
    .ok_or_else(|| anyhow::anyhow!("output cost overflow"))?;
let numerator = uncached_cost
    .checked_add(cached_cost)
    .and_then(|value| value.checked_add(cache_write_cost))
    .and_then(|value| value.checked_add(output_cost))
    .ok_or_else(|| anyhow::anyhow!("total cost overflow"))?;
let rounded = numerator
    .checked_add(999_999)
    .ok_or_else(|| anyhow::anyhow!("cost rounding overflow"))?;
let reported_cost_fen = u64::try_from(rounded / 1_000_000)?;
```

若供应商账单提供更高的权威实际金额，receipt 使用更高值并保留两者；不能用估算值覆盖真实费用。provider attempt cap、deadline、每请求固定 `max_output_tokens` 与隔离账户/预付总额是事前硬闸；单臂 total-token/成本是事后 proof-validity ceiling，不伪称能在未知输入 usage 前阻止所有超额。两个 receipt 分别超过已批准单臂 validity ceiling、总和超过批准总上限，都使 proof INVALID；供应商隔离账户的总额必须不高于用户批准总额。

`held-out-attestation.json` 在 candidate 冻结且新案例已选定后、任何 provider 请求前签署，因为审批必须针对实际案例与材料而非空白授权。字段至少为：`candidateSha`、`candidateFrozenAt`、`caseSelectedAt`、`caseSha256`、`sourceMaterialsSha256`、canonical `privateRoot`、case class 声明、`notOneOfFiveFrozenClasses=true`、材料授权范围、provider/model disclosure、`providerRole=targetVolcengine|approvedReference`、target provider/model evidence commitment、三名 reviewer disclosure、每人私有的 qualification class/经验签名（至少两人 `experiencedOperatorOrDirector=true`）、用户 `approvalId`、approved total/per-run fen、attempt/token/time ceilings、每请求 `maxOutputTokens`、`signedAt`、`retentionDeadline`、provider budget evidence SHA、rate-card SHA、billing-policy/source commitment、FX policy/evidence SHA、费率币种/生效时间。运行时用 `DateTime::parse_from_rfc3339`，要求 `candidateSha == forkSha`、`candidateFrozenAt < caseSelectedAt <= signedAt <= runStart < retentionDeadline`，并要求 case/material/privateRoot 与 frozen context、两臂 manifest 精确一致。`approvedReference` 可产生 Plan 01 业务方法信号，但不能被标成 target，也不能满足 Plan 03 的火山门或 G4a；本 schema 不表达第二国产 provider 的 G4p，后续 onboarding proof 必须使用产品能力合同另建证据，不能复用或改名本次结果。

### 6.4 只提交无正文 report

#### Contract 5：实现 score / annotate-cost / summarize

`score` 校验三份 submission 后才按 reviewer ID 读取三份私有 mapping，输出私有 decision。execution manifest 封账后永不改写，避免“receipt 含 manifest SHA、manifest 又含 receipt SHA”的哈希环。`annotate-cost` 的精确语义是以 `create_new` 原子生成 `cost-binding.json { executionManifestSha256, costReceiptSha256, brokerReceiptSha256, condition, boundAt }`；已存在、hash 不符或字段不全拒绝。`summarize` 同时重算 immutable manifest、receipt 与 binding，输出仓库内唯一业务证据 `report.json`：

```json
{
  "schemaVersion": 1,
  "publicRunId": "hmac-derived-high-entropy-id",
  "decision": "BUSINESS_SIGNAL_PASS_PENDING_FOUNDATION",
  "forkSha": "40-hex",
  "codexBinarySha256": "64-hex",
  "evaluatorBinarySha256": "64-hex",
  "brokerComponentSha256": "64-hex",
  "modelLabel": "approved-sanitized-model-revision",
  "providerLabel": "approved-actual-provider",
  "providerCompatibilityName": "OpenAI",
  "providerRole": "targetVolcengine",
  "frozenRunContextCommitment": "salted-hmac-sha256",
  "executionContextCommitment": "salted-hmac-sha256",
  "attemptIndexRootCommitment": "salted-hmac-sha256",
  "providerEndpointCommitment": "salted-hmac-sha256",
  "privateCaseCommitment": "salted-hmac-sha256",
  "privateMaterialsCommitment": "salted-hmac-sha256",
  "sharedConfigSha256": "64-hex",
  "promptSha256": "64-hex",
  "additionalContextCommitment": "salted-hmac-sha256",
  "outputSchemaSha256": "64-hex",
  "normalizedThreadStartSha256": "64-hex",
  "normalizedTurnStartCommitment": "salted-hmac-sha256",
  "genericCatalogSha256": "64-hex",
  "candidateCatalogSha256": "64-hex",
  "candidateSkillUseVerified": true,
  "skillUseEvidenceCommitment": "salted-hmac-sha256",
  "pairManifestsCommitment": "salted-hmac-sha256",
  "attestationCommitment": "salted-hmac-sha256",
  "armOrderCommitment": "salted-hmac-sha256",
  "rateCardSha256": "64-hex",
  "reviewSubmissionsCommitment": "salted-hmac-sha256",
  "proofRootSha256": "64-hex",
  "rubricSha256": "64-hex",
  "reviewerCount": 3,
  "experiencedOperatorOrDirectorCount": 2,
  "candidatePreferenceCount": 2,
  "candidateReadyForHumanReviewCount": 2,
  "medianGenericScore": 0,
  "medianCandidateScore": 0,
  "medianPairedDelta": 0,
  "candidateSevereFailureCount": 0,
  "genericUsage": {},
  "candidateUsage": {},
  "genericCostFen": 0,
  "candidateCostFen": 0,
  "providerRequestAttemptCounts": [0, 0],
  "providerCompletedResponseCounts": [0, 0],
  "usageScope": "rootSessionTree",
  "privateEvidenceRetentionDeadline": "RFC3339",
  "capabilityStatus": {
    "codePresent": true,
    "mechanicalContracts": "pendingFoundationVerification",
    "liveProviderReachable": true,
    "businessBlindReview": "passed|iterate|invalid",
    "publicationRetro": "notRun"
  },
  "retentionStatus": "pending",
  "sourceMaterialRetention": "userOwnedOriginalsOutsideProofCopyScope",
  "retentionCloseoutReceiptPath": "retention-closeout.json",
  "generatedAt": "RFC3339"
}
```

实际数值不得用 0 占位。`publicRunId` 由 pair ID 的 HMAC 派生，在 frozen context 时确定，不用 score 后的墙钟目录名猜测本次运行。低熵或敏感对象不用公开裸 SHA，而由 coordinator 在读 bearer 前生成 32-byte 私有 commitment key；key 只留在私有 coordinator 目录直至 retention closeout。非敏感 Schema/通用 Prompt/catalog 可公开裸 SHA。

编码算法是协议的一部分：canonical JSON 用 RFC 8785/JCS UTF-8，禁止 NaN/Infinity 和重复 key。敏感值 commitment 为 `HMAC-SHA256(key, domain || u32be(label_len) || label_utf8 || u64be(value_len) || canonical_value_bytes)`，其中 Rust domain 必须是 `b"AI-IP-PROOF-V1\0"`，即字节 hex `41 49 2d 49 50 2d 50 52 4f 4f 46 2d 56 31 00`，末字节是真实 NUL，不是反斜杠和字符 `0`；输出小写 hex。Merkle leaf 为 `SHA256(0x00 || u32be(name_len) || name_utf8 || u32be(value_len) || value_ascii)`，内部节点为 `SHA256(0x01 || left32 || right32)`；leaf 按 UTF-8 字段名字节排序，奇数层复制最后一个，空集拒绝。Rust fixture 必须冻结至少一组 key/label/value/leaf/node/root 期望 hex，并由 Python public verifier 使用同一公开 vector 交叉校验。`verify-live-proof`、`summarize` 和 report verifier 共享同一 Rust 实现而不是各自解释。`proofRootSha256` 对全部公开 digest/commitment 做上述 Merkle root，leaf set 明确排除 `proofRootSha256` 自身，避免哈希环。

report 禁止 case ID、正文、材料路径、输出、理由、reviewer 身份、mapping、线程/响应 ID、upstream URL 或 bearer。README 解释 `PASS`、`ITERATE`、`INVALID`，并声明 report 只是最小 Skill treatment 与本次较优内容相关的单案例首个信号，不是因果效应估计、跨行业泛化、爆款或发布效果证明。

Work Package 6 同时以 RED/GREEN 实现四个不能到 Work Package 9 临时补的 verifier：

- `verify-report`：对 public report 执行 JSON Schema、exact allowlist、JCS/proof-root 重算、secret/path/ID 扫描；失败 fixture 包含 bearer 形式、绝对路径、thread/response ID、case 正文和额外字段。成功时必须以 `--output` 在私有 staging 生成 exact、无正文的 `report-verification.json`，至少绑定 `schemaVersion`、frozen run context SHA、report SHA、next-index SHA、`publicRunId`、验证器二进制 SHA、`verifiedAt` 和 `valid=true`；已存在输出、额外字段或任一输入不匹配都失败。
- `verify-live-proof`：在 retention deadline 前从明确传入的 frozen/execution context、commitment key、attempt index、broker receipt、两份 immutable execution manifest、cost receipt/binding、attestation、三份 mapping/review 与 public report 重算完整私有证明，并只输出无正文 `live-proof-verification.json`。它必须验证 providerRole/target evidence、timeline、candidate/case/material/privateRoot、request parity、Skill use、attempt 无 gap、broker/App Server usage 多重集、cost ceiling、review mapping 与 HMAC/Merkle；不信任 `summarize` 的 decision。
- `publish-report`：必须接收 `--verification-receipt` 和该 receipt 绑定的私有 staging report/index；发布前重算 frozen context、report 与 next-index 的实际 SHA，并与 receipt 做 exact equality，避免 verify/publish 之间的 TOCTOU。destination report 必须是 context 中 `publicRunId` 的精确仓库路径并以 `create_new` 写入；destination index 必须以 summarize 读取的 prior state 做 CAS，next 只能 append 一条。首次调用要求主取证 repo clean，并在私有 `coordinator/publish-report.transaction.json` 以 `create_new` 记录 HEAD、原始 status commitment、两个 destination、prior state、验证 receipt SHA 和目标 bytes SHA，再写仓库。若中断，重跑允许 repo 的 dirty/untracked 集合仅是 marker 列出的 destination 子集，HEAD 不变且已存在文件字节等于 marker；随后幂等完成缺失文件。marker 不在仓库；任何其他 dirt、path escape、symlink/hardlink、已存不同 report、receipt 漂移或旧记录改写都失败。
- `finalize-checkpoint`：只读无正文 foundation verification、selected report/index 和现有 ledger，重验 deadline、G0/G1/G2、candidate allowed-diff、`origin_status`/实际 remote 和 public path allowlist。首次调用要求主取证 repo clean，并在私有 coordinator 写 HEAD/destination/prior/target SHA transaction marker；恢复调用只允许 marker 精确列出的 verification/ledger 半写 bytes，不能被外层 clean gate 抢先拒绝。它以 CAS 同时发布 verification 和 ledger checkpoint；只有所有机器结论为 PASS 才能写 `PASS_TO_PHASE_0B`，其他情况必须写 ITERATE/INVALID。测试要求手工 PASS、过期、候选后代码改动、origin 状态伪造、相同 bytes 一半写入可恢复和已存不同输出失败。

`report-index.schema.json` 冻结 `{schemaVersion,attempts}` 的 exact 结构和每条 attempt 字段。`summarize --existing-report-index` 只接受仓库内精确的 `docs/evidence/business-proof/index.json`。首个 run 时仅允许该 exact path 不存在，并把 next index 的私有构建元数据记为 `priorIndexState="absent", priorIndexSha256=null`；后续 run 要求 existing index 通过 schema/JCS/append-only 校验并记录真实 SHA。测试覆盖 absent 首次创建、伪空文件、旧条目改写、第四条 attempt 和第二个 selected 条目。`publish-report` 的 transaction marker 对 absent state 也做 CAS：发布瞬间 destination index 仍必须不存在；若已出现任何未绑定文件即失败。

Work Package 2 的 Python `verify_evidence.py` 保持 foundation/公开证据 verifier；Work Package 9 只让它绑定上述无正文 Rust verification receipt，不突然向它传私有 root 或 key。

完成 cost/report 后立即做第二个可审查提交：

```bash
just bazel-lock-update
just test -p codex-ai-ip-eval
bazel test //codex-rs/ai-ip-eval:ai-ip-eval-unit-tests //codex-rs/ai-ip-eval:codex-ai-ip-eval-bin-unit-tests
just bazel-lock-check
just fmt
just fix -p codex-ai-ip-eval
git add codex-rs/ai-ip-eval ai-ip-evals docs/evidence/business-proof codex-rs/Cargo.lock MODULE.bazel.lock
git commit -m "test: bind AI IP proof cost and report"
```

#### Contract 6：实现 retention closeout，而不假装 SSD 安全擦除

先增加并运行 destructive closeout 失败测试：拒绝仓库/Home/磁盘根/系统临时根或其祖先路径、marker/pair/proof mismatch、未到期且无新 approval、有 in-flight、删除失败和重复调用；失败 receipt 必须诚实且不得伪造物理安全擦除。`retention-closeout.schema.json` 使用 exact allowlist/`additionalProperties=false` 和相互一致的 status/boolean 约束；negative fixtures 覆盖未知字段、缺 proof-root/inventory commitment、成功状态却 `proofCopiesDeleted=false`、伪造 `physicalSecureErasureGuaranteed=true` 与失败状态缺原因。

```bash
just test -p codex-ai-ip-eval
```

预期：FAIL，closeout 尚未实现。

CLI 增加 `retention-closeout`。它只接受 attestation 中记录的 canonical private root，并要求目录内 pair marker 与 `pairId`/proof root 匹配；拒绝仓库、`/`、用户 Home、系统临时根本身及其任何祖先。到期自动授权逻辑删除；提前删除必须有新的 approval ID。

每个 pair marker 必须在运行期间 append + `fsync` 出完整派生私有路径 inventory：`inputs/` 内的 attestation/budget/rate/billing/FX 运行专用原件、导入的 case/material proof copies、source-retention disclosure、Home/CODEX_HOME、rollout/state/cache/tmp、transcript/stderr、eval-tree、broker/attempt/cost、reviewer/mapping/reviews、seed/key/context 全部列出。closeout 重做 canonical containment，拒绝 symlink/junction/reparse/hardlink，并在删除前要求实际整树 exact 等于 inventory：有未列文件或漏列文件都失败，不做选择性删除。删除前确认无 App Server/broker/evaluator in-flight，然后删整个 canonical private root；删除后确认 root 不存在，在 root 外写通过 `retention-closeout.schema.json` exact 校验的无正文 `retention-closeout.json`：pair/report/proof-root commitments、inventory commitment、scheduled/deleted RFC3339、status、operator、`proofCopiesDeleted=true|false`、`externalUserSourcesRetained=true`、方法 `logical-filesystem-delete`、`physicalSecureErasureGuaranteed=false` 和失败原因。receipt 不含外部源路径；用户可读报告必须直说用户原始项目素材仍保留、删除完成仅指 proof copies 和运行专用证明输入，不能笼统宣称“全部私有源数据已删除”。

无法安全删除时不得伪造成功：保留失败 receipt、立即通知用户并阻止任何 retention-complete 声明。公开 business report 初始为 `retentionStatus=pending` 并记录 closeout receipt 的预期相对位置；到期后只提交无正文 receipt。协调器台账必须在 deadline 前创建明确到期任务，但本计划不擅自创建外部自动化。

`retentionDeadline` 必须在审批时覆盖预计的三人 review、macOS/Windows Work Package 9 全 matrix、安全回传/复核以及明确 buffer。Work Package 9 的 private recomputation 与 checkpoint 必须在 deadline 前完成；若 deadline 先到，先按授权 closeout，但本 pair 不再有资格授权 `PASS_TO_PHASE_0B`，必须用新审批和新 held-out case 重做，不得为了验收延期保留私有数据。

#### Contract 7：验证并单独提交 retention closeout

```bash
just test -p codex-ai-ip-eval
just bazel-lock-update
bazel test //codex-rs/ai-ip-eval:ai-ip-eval-unit-tests //codex-rs/ai-ip-eval:codex-ai-ip-eval-bin-unit-tests
just bazel-lock-check
just fmt
just fix -p codex-ai-ip-eval
git add codex-rs/ai-ip-eval codex-rs/Cargo.lock MODULE.bazel.lock docs/evidence/business-proof
git commit -m "feat: add private proof retention closeout"
```

---

## Work Package 7：零预算机械排练与污染门

**文件：**

- 读取：`codex-rs/ai-ip-eval/tests/fixtures/*`
- 读取：`ai-ip-assets/skills/deliver-ai-ip-content-package/SKILL.md`
- 只在 Git 外生成：系统临时目录下的 replay pair/reviewer/mapping/report 尝试

#### Contract 1：构建一次、冻结二进制并运行 focused set

```bash
set -euo pipefail
ai_ip_repo_root="$(git rev-parse --show-toplevel)"
ai_ip_candidate_sha="$(git rev-parse HEAD)"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
just test -p codex-ai-ip-domain
just test -p codex-ai-ip-runtime
just test -p codex-ai-ip-eval
just test -p codex-responses-api-proxy
just test -p codex-app-server --test all -E 'test(=suite::v2::raw_response_subagents::raw_events_cover_root_child_grandchild_and_sibling)'
just test -p codex-app-server --test all -E 'test(=suite::v2::ai_ip_strict_output::turn_start_sends_strict_content_package_schema)'
just bazel-lock-check
cargo build --locked --manifest-path codex-rs/Cargo.toml -p codex-cli -p codex-ai-ip-eval -p codex-responses-api-proxy
if test -n "${CARGO_TARGET_DIR:-}"; then
  ai_ip_target_root="$(python3 -c 'from pathlib import Path; import os; print(Path(os.environ["CARGO_TARGET_DIR"]).resolve(strict=True))')"
else
  ai_ip_target_root="$ai_ip_repo_root/codex-rs/target"
fi
ai_ip_eval_bin="$(python3 -c 'from pathlib import Path; import sys; print(Path(sys.argv[1]).resolve(strict=True))' "$ai_ip_target_root/debug/codex-ai-ip-eval")"
ai_ip_codex_bin="$(python3 -c 'from pathlib import Path; import sys; print(Path(sys.argv[1]).resolve(strict=True))' "$ai_ip_target_root/debug/codex")"
ai_ip_eval_sha="$(shasum -a 256 "$ai_ip_eval_bin" | awk '{print $1}')"
ai_ip_codex_sha="$(shasum -a 256 "$ai_ip_codex_bin" | awk '{print $1}')"
test "${#ai_ip_eval_sha}" = 64
test "${#ai_ip_codex_sha}" = 64
export AI_IP_EVAL_BIN="$ai_ip_eval_bin"
export AI_IP_CODEX_BIN="$ai_ip_codex_bin"
export AI_IP_CANDIDATE_SHA="$ai_ip_candidate_sha"
```

禁止 `cargo run`。本 shell 导出三个绝对/锁定值；若换 shell，必须显式重新提供 `AI_IP_EVAL_BIN`、`AI_IP_CODEX_BIN`、`AI_IP_CANDIDATE_SHA`，不能期待局部变量跨 code fence 存活。Work Package 7 先用冻结二进制直接创建 replay context；从该 context 产生后起，所有 evaluator 子命令都经 `run_frozen_eval.py`。

#### Contract 2：在独立 Git boundary 运行 replay pair

```bash
set -euo pipefail
: "${AI_IP_EVAL_BIN:?set absolute frozen evaluator binary}"
: "${AI_IP_CODEX_BIN:?set absolute frozen codex binary}"
: "${AI_IP_CANDIDATE_SHA:?set frozen candidate SHA}"
ai_ip_repo_root="$(git rev-parse --show-toplevel)"
ai_ip_replay_base="$(mktemp -d "${TMPDIR:-/tmp}/ai-ip-phase0a-replay.XXXXXX")"
ai_ip_replay_root="$ai_ip_replay_base/private-run"
ai_ip_replay_case_root="$ai_ip_replay_base/input-case"
mkdir -p "$ai_ip_replay_root" "$ai_ip_replay_case_root"
git -C "$ai_ip_replay_case_root" init -q
cp "$ai_ip_repo_root/codex-rs/ai-ip-eval/tests/fixtures/replay-case.json" "$ai_ip_replay_case_root/case.json"
AI_IP_FROZEN_RUN_CONTEXT="$ai_ip_replay_root/coordinator/frozen-run-context.json"
"$AI_IP_EVAL_BIN" freeze-run-context replay \
  --repo-root "$ai_ip_repo_root" \
  --fork-sha "$AI_IP_CANDIDATE_SHA" \
  --case "$ai_ip_replay_case_root/case.json" \
  --transcript "$ai_ip_repo_root/codex-rs/ai-ip-eval/tests/fixtures/replay-transcript.jsonl" \
  --fixture-set-manifest "$ai_ip_repo_root/codex-rs/ai-ip-eval/tests/fixtures/replay-fixture-set.json" \
  --codex-bin "$AI_IP_CODEX_BIN" \
  --private-root "$ai_ip_replay_root" \
  --output "$AI_IP_FROZEN_RUN_CONTEXT"
python3 scripts/ai_ip/foundation/run_frozen_eval.py --context "$AI_IP_FROZEN_RUN_CONTEXT" -- replay-pair
python3 scripts/ai_ip/foundation/run_frozen_eval.py --context "$AI_IP_FROZEN_RUN_CONTEXT" -- \
  blind-pack \
  --reviewer-root reviewer \
  --mapping-dir coordinator/mappings \
  --replay-seed one \
  --replay-seed two \
  --replay-seed three
for ai_ip_reviewer in 1 2 3; do
  test -f "$ai_ip_replay_root/reviewer/reviewer-$ai_ip_reviewer/A.json"
  test -f "$ai_ip_replay_root/reviewer/reviewer-$ai_ip_reviewer/B.json"
  test -f "$ai_ip_replay_root/coordinator/mappings/reviewer-$ai_ip_reviewer.private.json"
done
test ! -e "$ai_ip_replay_root/reviewer/mapping.private.json"
test -d "$ai_ip_replay_root/reviews"
cp "$ai_ip_repo_root/codex-rs/ai-ip-eval/tests/fixtures/replay-review-1.json" "$ai_ip_replay_root/reviews/reviewer-1.json"
cp "$ai_ip_repo_root/codex-rs/ai-ip-eval/tests/fixtures/replay-review-2.json" "$ai_ip_replay_root/reviews/reviewer-2.json"
cp "$ai_ip_repo_root/codex-rs/ai-ip-eval/tests/fixtures/replay-review-3.json" "$ai_ip_replay_root/reviews/reviewer-3.json"
python3 scripts/ai_ip/foundation/run_frozen_eval.py --context "$AI_IP_FROZEN_RUN_CONTEXT" -- score \
  --mapping-dir coordinator/mappings \
  --reviews-dir "$ai_ip_replay_root/reviews" \
  --output "$ai_ip_replay_root/coordinator/decision.private.json"
if python3 scripts/ai_ip/foundation/run_frozen_eval.py --context "$AI_IP_FROZEN_RUN_CONTEXT" -- summarize \
  --mapping-dir coordinator/mappings \
  --reviews-dir "$ai_ip_replay_root/reviews" \
  --decision "$ai_ip_replay_root/coordinator/decision.private.json" \
  --attestation "$ai_ip_repo_root/codex-rs/ai-ip-eval/tests/fixtures/replay-attestation.json" \
  --output "$ai_ip_replay_root/replay-report-must-not-exist.json" \
  2>"$ai_ip_replay_root/summarize.stderr"; then
  exit 1
fi
rg -n 'live evidence required' "$ai_ip_replay_root/summarize.stderr"
test ! -e "$ai_ip_replay_root/replay-report-must-not-exist.json"
```

Replay 可以证明 hash/盲化/决策机械正确，永远不能产生 `BUSINESS_SIGNAL_PASS_PENDING_FOUNDATION` 或 `PASS_TO_PHASE_0B`。Work Package 8 的 live report 最多写前者；唯一 `PASS_TO_PHASE_0B` 由 Work Package 9 在 G0/G1/G2 全通过后写入 checkpoint ledger。

#### Contract 3：扫描生产候选污染和仓库泄漏

```bash
set -euo pipefail
if rg -n '(水果|黄金礼品|直播公会|宝妈|本系统自己营销自己|creator sees|passes review|first live|map-marketing-content-world|ContentRootLab|固定内容根)' \
  ai-ip-assets/skills/deliver-ai-ip-content-package/SKILL.md \
  codex-rs/ai-ip-runtime/src \
  codex-rs/ai-ip-runtime/Cargo.toml \
  codex-rs/ai-ip-runtime/BUILD.bazel; then
  exit 1
fi
rg -n '^publish = false$' codex-rs/ai-ip-eval/Cargo.toml
test -z "$(git status --porcelain=v1 --untracked-files=all)"
```

预期：无固定案例污染；无 replay case/output/review/mapping/receipt/transcript 进入工作树。任何失败先增加失败测试、修复、提交并重新冻结 candidate，绝不带病进入 live。

---

## Work Package 8：执行一次真实、原子、双臂盲评业务证明

**私有输入：**

- 一个在 candidate 冻结后才选择、且不属于五类冻结案例的新真实 `case.json` 与相对材料；
- `held-out-attestation.json`；
- 隔离/预付 provider API key；
- provider 当前 rate card、billing policy/source、FX policy（人民币不换汇也要明记 `notApplicable`）与账户硬上限证据；
- 三名互不协商的真人 reviewer submission。

**仓库输出：** 本次 live/score 初始只写 `docs/evidence/business-proof/<public-run-id>/report.json` 并更新无正文 `docs/evidence/business-proof/index.json`；到 retention deadline 后，另允许同一 run 目录增加无正文 `retention-closeout.json`。

#### Contract 1：冻结 candidate，之后才选择并揭示案例

```bash
set -euo pipefail
: "${AI_IP_FROZEN_EVAL_WORKTREE:?set a new absolute worktree path outside the evidence repo}"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
AI_IP_CANDIDATE_SHA="$(git rev-parse HEAD)"
git worktree add --detach "$AI_IP_FROZEN_EVAL_WORKTREE" "$AI_IP_CANDIDATE_SHA"
test "$(git -C "$AI_IP_FROZEN_EVAL_WORKTREE" rev-parse HEAD)" = "$AI_IP_CANDIDATE_SHA"
test -z "$(git -C "$AI_IP_FROZEN_EVAL_WORKTREE" status --porcelain=v1 --untracked-files=all)"
git show --stat --oneline "$AI_IP_CANDIDATE_SHA"
```

协调器在此命令完成后才选案例。独立 detached evaluator worktree 一直保持 candidate SHA 与 clean，Work Package 8 report 和 Work Package 9 证据提交发生在另一个主取证 worktree；这使后处理 wrapper 仍能验证原 candidate，不需放宽成“任意 descendant HEAD”。换 shell 时实施者必须重新提供 `AI_IP_CANDIDATE_SHA`、`AI_IP_FROZEN_EVAL_WORKTREE` 的精确值，不依赖上一个 code block 的 export。直到 blind score 完成，不得改 Skill、runtime、Schema、rubric、decision rule、evaluator、broker、模型、provider 或 Codex binary。案例一旦用于任何臂，就不再是下次迭代的 held-out case。

#### Contract 2：在任何上游调用前取得一次明确审批

向用户展示并记录：candidate SHA；模型/实际 provider；两次顶层 evaluation run 由 live-pair 在审批后、任一请求前 CSPRNG 承诺顺序且两臂各一次；每臂最大 provider request attempts、总 token、时长和人民币分；两臂总人民币分；provider 隔离账户/预付硬上限；案例/材料向 provider 和三名 reviewer 披露范围；私有证据 retention deadline；实际发布不在授权内。

必须获得可归属 `approvalId` 的本次明确批准。此前对架构的“可以”、沉默或普通技术授权均不能替代该消费/披露批准。

审批后，coordinator 先建立 owner-only `AI_IP_PRIVATE_RUN_ROOT/inputs/`。本次运行专用的 `held-out-attestation.json`、provider budget evidence、rate-card snapshot、billing-policy snapshot 和 FX-policy snapshot 必须直接写在该目录的五个固定文件名下；不得先放在普通下载目录/Home 后又只复制一份而把原件遗留在 retention 范围外。外部 `case.json` 及其材料属于用户原始项目输入，freeze 会导入一份受 retention 管理的只读副本；原始用户素材继续由用户持有，最终报告必须明确它不在 proof-copy 删除承诺内。

#### Contract 3：一次冻结全部静态输入，不读 bearer

```bash
set -euo pipefail
: "${AI_IP_CANDIDATE_SHA:?set exact SHA frozen in Step 1}"
: "${AI_IP_FROZEN_EVAL_WORKTREE:?set exact detached candidate worktree}"
: "${AI_IP_EVAL_BIN:?set absolute frozen evaluator binary}"
: "${AI_IP_CODEX_BIN:?set absolute frozen Codex binary}"
: "${AI_IP_PRIVATE_CASE:?set absolute private case.json path}"
: "${AI_IP_PRIVATE_ATTESTATION:?set privateRoot/inputs/held-out-attestation.json}"
: "${AI_IP_PROVIDER_BUDGET_EVIDENCE:?set privateRoot/inputs/provider-budget-evidence.json}"
: "${AI_IP_RATE_CARD:?set privateRoot/inputs/rate-card.json}"
: "${AI_IP_BILLING_POLICY:?set privateRoot/inputs/billing-policy.json}"
: "${AI_IP_FX_POLICY:?set privateRoot/inputs/fx-policy.json, including notApplicable CNY policy}"
: "${AI_IP_PRIVATE_RUN_ROOT:?set absolute owner-only private run root containing only inputs}"
: "${AI_IP_FROZEN_RUN_CONTEXT:?set absolute output path under private root/coordinator}"
: "${AI_IP_MODEL_LABEL:?set approved model label}"
: "${AI_IP_PROVIDER_LABEL:?set approved provider label}"
: "${AI_IP_PROVIDER_ROLE:?set targetVolcengine or approvedReference}"
: "${AI_IP_PROVIDER_UPSTREAM_URL:?set approved exact Responses endpoint (Ark: /api/v3/responses)}"
: "${AI_IP_AUTHORIZED_TOTAL_COST_FEN:?set approved integer}"
: "${AI_IP_AUTHORIZED_PER_RUN_COST_FEN:?set approved integer}"
: "${AI_IP_MAX_PROVIDER_ATTEMPTS_PER_RUN:?set approved integer}"
: "${AI_IP_MAX_TOTAL_TOKENS_PER_RUN:?set approved integer}"
: "${AI_IP_MAX_ELAPSED_SECONDS_PER_RUN:?set approved integer}"
: "${AI_IP_MAX_OUTPUT_TOKENS_PER_REQUEST:?set approved integer}"
ai_ip_evidence_repo_root="$(git rev-parse --show-toplevel)"
ai_ip_repo_root="$(python3 -c 'from pathlib import Path; import sys; print(Path(sys.argv[1]).resolve(strict=True))' "$AI_IP_FROZEN_EVAL_WORKTREE")"
ai_ip_eval_bin="$(python3 -c 'from pathlib import Path; import sys; print(Path(sys.argv[1]).resolve(strict=True))' "$AI_IP_EVAL_BIN")"
ai_ip_codex_bin="$(python3 -c 'from pathlib import Path; import sys; print(Path(sys.argv[1]).resolve(strict=True))' "$AI_IP_CODEX_BIN")"
test "$(git -C "$ai_ip_repo_root" rev-parse HEAD)" = "$AI_IP_CANDIDATE_SHA"
test -z "$(git -C "$ai_ip_repo_root" status --porcelain=v1 --untracked-files=all)"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
test -x "$ai_ip_eval_bin"
test -x "$ai_ip_codex_bin"
python3 - "$AI_IP_PRIVATE_RUN_ROOT" "$AI_IP_PRIVATE_ATTESTATION" "$AI_IP_PROVIDER_BUDGET_EVIDENCE" "$AI_IP_RATE_CARD" "$AI_IP_BILLING_POLICY" "$AI_IP_FX_POLICY" <<'PY'
from pathlib import Path
import os
import sys

root = Path(sys.argv[1]).resolve(strict=True)
if not root.is_dir() or root in {Path("/"), Path.home(), Path(os.getenv("TMPDIR", "/tmp")).resolve()}:
    raise SystemExit("unsafe private root")
inputs = root / "inputs"
expected = [
    inputs / "held-out-attestation.json",
    inputs / "provider-budget-evidence.json",
    inputs / "rate-card.json",
    inputs / "billing-policy.json",
    inputs / "fx-policy.json",
]
supplied = [Path(value).resolve(strict=True) for value in sys.argv[2:]]
if supplied != expected or set(root.iterdir()) != {inputs} or set(inputs.iterdir()) != set(expected):
    raise SystemExit("private root input allowlist mismatch")
for path in expected:
    if not path.is_file() or path.is_symlink() or path.stat().st_nlink != 1:
        raise SystemExit(f"unsafe proof input: {path.name}")
PY
"$ai_ip_eval_bin" freeze-run-context live \
  --repo-root "$ai_ip_repo_root" \
  --evidence-repo-root "$ai_ip_evidence_repo_root" \
  --fork-sha "$AI_IP_CANDIDATE_SHA" \
  --case "$AI_IP_PRIVATE_CASE" \
  --attestation "$AI_IP_PRIVATE_ATTESTATION" \
  --provider-budget-evidence "$AI_IP_PROVIDER_BUDGET_EVIDENCE" \
  --rate-card "$AI_IP_RATE_CARD" \
  --billing-policy "$AI_IP_BILLING_POLICY" \
  --fx-policy "$AI_IP_FX_POLICY" \
  --codex-bin "$ai_ip_codex_bin" \
  --lead-skill "$ai_ip_repo_root/ai-ip-assets/skills/deliver-ai-ip-content-package/SKILL.md" \
  --model-label "$AI_IP_MODEL_LABEL" \
  --provider-label "$AI_IP_PROVIDER_LABEL" \
  --provider-role "$AI_IP_PROVIDER_ROLE" \
  --provider-upstream-url "$AI_IP_PROVIDER_UPSTREAM_URL" \
  --authorized-total-cost-fen "$AI_IP_AUTHORIZED_TOTAL_COST_FEN" \
  --authorized-per-run-cost-fen "$AI_IP_AUTHORIZED_PER_RUN_COST_FEN" \
  --max-provider-request-attempts-per-run "$AI_IP_MAX_PROVIDER_ATTEMPTS_PER_RUN" \
  --max-total-tokens-per-run "$AI_IP_MAX_TOTAL_TOKENS_PER_RUN" \
  --max-elapsed-seconds-per-run "$AI_IP_MAX_ELAPSED_SECONDS_PER_RUN" \
  --max-output-tokens-per-request "$AI_IP_MAX_OUTPUT_TOKENS_PER_REQUEST" \
  --private-root "$AI_IP_PRIVATE_RUN_ROOT" \
  --output "$AI_IP_FROZEN_RUN_CONTEXT"
test -f "$AI_IP_FROZEN_RUN_CONTEXT"
```

`freeze-run-context live` 用 `create_new` 一次生成 commitment key 与 context；它先把外部 user-owned case/regular materials 流式复制到 `privateRoot/inputs/case/`，逐文件 `create_new`、owner-only、`fsync`、复算 hash，并生成私有 `source-retention-disclosure.json`，列明外部原始素材仍由用户保留而 proof copy 将在 deadline 删除。随后它只绑定 privateRoot 内副本，验证 attestation 的 candidate/case/material/privateRoot/timeline/providerRole/预算与全部输入，但不连接 provider。`live-pair` 再从受管副本投影为 `privateRoot/eval-tree/`：新建无历史、无 remote 的 Git repo，只复制 canonical `case.json`、manifest 声明的 regular materials 和固定 marker，用本地假身份做唯一初始 commit 后设为只读。逐目录要求 exact allowlist，拒绝未声明文件、symlink、hardlink、junction/reparse point、submodule、alternate object store、hook、remote或额外 Git config；两臂 cwd 都只指向该投影，原始素材目录不在 App Server 权限内。`.git` 中除了这些已授权字节和固定本地 metadata 外不得有其他内容。

随后 `live-pair` 以 `bind` 取得不接收请求的 loopback port，再生成最终 config/Home。在读取 provider bearer、`activate` broker 或发模型请求前必须自动完成：所有派生路径 canonical containment/inode/reparse 检查；eval-tree 无 `AGENTS*`/`.codex`/`.agents`；attestation RFC3339/approval/预算验证；源码和投影材料流式 hash；两个 Home config bytes/effective layer/catalog parity；用冻结 Codex 的 `sandbox -P ai-ip-eval -C eval-tree` 证明声明材料可读、写 eval-tree 失败、读原始 case/privateRoot 其他目录/credential sentinel 失败、candidate Skill 可完整读取、工具网络失败。preflight 失败时 provider attempt 必须仍为 0，bound listener 立即关闭。

#### Contract 4：一个命令按已承诺随机顺序完成两臂各一次

从操作系统凭证库读取隔离 API key，通过 stdin 交给 evaluator。macOS 示例：

```bash
set -euo pipefail
: "${AI_IP_FROZEN_RUN_CONTEXT:?set absolute frozen context created in Step 3}"
: "${AI_IP_KEYCHAIN_SERVICE:?set keychain service}"
: "${AI_IP_KEYCHAIN_ACCOUNT:?set keychain account}"
security find-generic-password -s "$AI_IP_KEYCHAIN_SERVICE" -a "$AI_IP_KEYCHAIN_ACCOUNT" -w | \
python3 scripts/ai_ip/foundation/run_frozen_eval.py \
  --context "$AI_IP_FROZEN_RUN_CONTEXT" -- live-pair
```

`security` 的 stdout 只接 evaluator stdin；禁止 `set -x`、环境变量 API key、命令行 key、`auth.json`、shell history key、log/dump。若不是 macOS，使用平台安全凭证命令向 stdin 输出，不能降级成参数/env。

预期：单一进程持有 bound listener，preflight 全通过后先写 `execution-context.json`，再读 bearer/激活 broker。只有两个顶层 root，顺序由已承诺 seed 决定，generic/candidate 各一次；每臂可按需有子代理；所有上游 attempt 已 pre-forward append+fsync 并有 terminal record；broker/App Server 的 `(responseId, usage)` 私有多重集精确一致；两个 App Server 均 clean exit；broker final receipt 显示 `inFlight=0` 且绑定 attempt root。任一臂失败时另一臂不补跑、任一条件不重跑，本 pair 整体 INVALID。

#### Contract 5：绑定费用并生成盲包

协调器在 broker final seal 后、不改写 execution manifest，先生成两个私有 cost receipt、再生成 sidecar binding，最后生成三份独立盲包。若运行后已有供应商 statement，将其复制到 private root 且通过 `--supplier-statement` 传入；没有则省略，不伪造空账单。以下示例是无最终 statement 的精确路径：

```bash
set -euo pipefail
: "${AI_IP_FROZEN_RUN_CONTEXT:?set absolute frozen context}"
python3 scripts/ai_ip/foundation/run_frozen_eval.py --context "$AI_IP_FROZEN_RUN_CONTEXT" -- \
  make-cost-receipt --condition generic
python3 scripts/ai_ip/foundation/run_frozen_eval.py --context "$AI_IP_FROZEN_RUN_CONTEXT" -- \
  make-cost-receipt --condition candidate
python3 scripts/ai_ip/foundation/run_frozen_eval.py --context "$AI_IP_FROZEN_RUN_CONTEXT" -- \
  annotate-cost --condition generic
python3 scripts/ai_ip/foundation/run_frozen_eval.py --context "$AI_IP_FROZEN_RUN_CONTEXT" -- \
  annotate-cost --condition candidate
python3 scripts/ai_ip/foundation/run_frozen_eval.py --context "$AI_IP_FROZEN_RUN_CONTEXT" -- \
  blind-pack --reviewer-root reviewer --mapping-dir coordinator/mappings --seed-dir coordinator/blind-seeds
```

`make-cost-receipt` 从 frozen context 取 rate card/billing/FX/budget 的冻结 path+digest，从 execution context/manifest/broker receipt 取实际 model/provider/attempt/usage；产物至少有 `{schemaVersion,pair/context/condition/runOrdinal,executionManifestSha,brokerReceiptSha,attemptLedgerSha/root/range,actualProvider/model,rateCardSha,billingPolicySha,fxPolicySha,budgetEvidenceSha,attempt/completionCounts,usageScope,fullUsage,estimatedFen,supplierActualFen|null,chargedFen,currency,fx,effectiveAt,ceilings,withinCeilings,calculatedAt}`。

三名 reviewer 各自只查看 `reviewer/reviewer-N/` 并把各自 JSON 放入 `blind-pack` 已经以 owner-only 权限创建并登记的私有 `reviews/` drop-dir；不得看到 mapping、条件、Skill、成本、token、日志、其他 reviewer 的 A/B 顺序或答案。该目录只接收三份 reviewer submission；它与只读盲包 `reviewer/`、coordinator mapping 目录分离。不得在新 shell 中直接展开未验证的 `$AI_IP_PRIVATE_RUN_ROOT/reviews`。

#### Contract 6：自动 score 并生成唯一可提交 report

```bash
set -euo pipefail
: "${AI_IP_FROZEN_RUN_CONTEXT:?set absolute frozen context}"
ai_ip_repo_root="$(git rev-parse --show-toplevel)"
ai_ip_public_run_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["publicRunId"])' "$AI_IP_FROZEN_RUN_CONTEXT")"
case "$ai_ip_public_run_id" in (*[!a-zA-Z0-9_-]*|'') exit 1;; esac
ai_ip_private_public_dir="$(python3 -c 'from pathlib import Path; import sys; print(Path(sys.argv[1]).resolve(strict=True).parent / "public-staging")' "$AI_IP_FROZEN_RUN_CONTEXT")"
python3 scripts/ai_ip/foundation/run_frozen_eval.py --context "$AI_IP_FROZEN_RUN_CONTEXT" -- \
  score --mapping-dir coordinator/mappings --reviews-dir reviews --output coordinator/decision.private.json
ai_ip_report_dir="$ai_ip_repo_root/docs/evidence/business-proof/$ai_ip_public_run_id"
python3 scripts/ai_ip/foundation/run_frozen_eval.py --context "$AI_IP_FROZEN_RUN_CONTEXT" -- \
  summarize \
  --mapping-dir coordinator/mappings \
  --reviews-dir reviews \
  --decision coordinator/decision.private.json \
  --commitment-key-file coordinator/commitment-key.bin \
  --existing-report-index "$ai_ip_repo_root/docs/evidence/business-proof/index.json" \
  --report-index-output "$ai_ip_private_public_dir/index.next.json" \
  --output "$ai_ip_private_public_dir/report.json"
python3 scripts/ai_ip/foundation/run_frozen_eval.py --context "$AI_IP_FROZEN_RUN_CONTEXT" -- \
  verify-report \
  --report "$ai_ip_private_public_dir/report.json" \
  --report-index "$ai_ip_private_public_dir/index.next.json" \
  --output "$ai_ip_private_public_dir/report-verification.json"
python3 scripts/ai_ip/foundation/run_frozen_eval.py --context "$AI_IP_FROZEN_RUN_CONTEXT" -- \
  publish-report \
  --verification-receipt "$ai_ip_private_public_dir/report-verification.json" \
  --report "$ai_ip_private_public_dir/report.json" \
  --report-index "$ai_ip_private_public_dir/index.next.json" \
  --destination-report "$ai_ip_report_dir/report.json" \
  --destination-index "$ai_ip_repo_root/docs/evidence/business-proof/index.json"
```

不要在 `publish-report` 前加外层 clean 断言：首次 clean 校验与 crash-recovery 的 exact dirty allowlist 都由同一个 frozen publisher 按私有 transaction marker 裁决，否则第一次半写后会永远无法重入。

机器校验通过后再人工检查 report/index 无正文/路径/ID，然后先 stage 再做 diff check（`git diff` 不检查 untracked file）：

```bash
git add "$ai_ip_report_dir/report.json" docs/evidence/business-proof/index.json
git diff --cached --check
git commit -m "test: record held-out AI IP business proof"
```

`index.json` 是 append-only 试验索引，每条固定 `{attemptOrdinal,publicRunId,reportPath,reportSha256,candidateSha,decision,supersedes,candidateDiffSha256,caseCommitment,materialsCommitment,selectedForCheckpoint}`。最多三条；case/material commitments 必须每次不同；最多一条 `selectedForCheckpoint=true`。

第二/三次 candidate 必然在前一次 report/index commit 之后，所以不对两个 candidate commit 的全 diff 盲目应用业务路径白名单。verifier 先将中间 bookkeeping partition 为前一个 `publicRunId/report.json` 的精确新增和 `index.json` 的 append-only 变更，重算其 SHA/schema 且禁止删除/改写旧记录；剩余 treatment projection 只允许 `ai-ip-assets/skills/deliver-ai-ip-content-package/`、`codex-rs/ai-ip-runtime/`、`codex-rs/ai-ip-domain/`。`candidateDiffSha256` 是对 `git diff --binary <previous-candidate>..<candidate> -- <three-treatment-roots>` 的字节 SHA，而不包含已验证 bookkeeping。proof harness、rubric、decision rule、provider 和上限改动必须将当前 pair 作废并重新走 Plan，不伪装成“最小业务迭代”。

若结论为 `ITERATE_SMALLEST_LEAD_CHANGE`，提交的 report 仍诚实记录失败；下一次只改上述白名单内的最小变更、重新冻结，并使用另一个新案例。连续三次最小迭代仍失败时暂停并向用户发起底座/业务方法复审；不得静默接回 DeerFlow。

---

## Work Package 9：补齐 native post-change 证据并裁决 G0–G2

**文件：**

- 生成：`docs/evidence/foundation/macos-x86_64/post-*`
- 在目标机生成并回传：`docs/evidence/foundation/windows-11-x64/post-*`
- 修改：`docs/evidence/foundation/workspace-suite-disposition.md`
- 修改：`docs/architecture/codex-fork-patch-ledger.md`
- 生成：`docs/evidence/business-proof/<run-id>/verification.json`（无正文）

#### Contract 1：从冻结 candidate SHA 建 macOS/Windows 原生测试树

不得在已产生 report 的新 HEAD 上冒充 candidate 测试。macOS 直接使用 Work Package 8 保留的 detached frozen evaluator worktree，再断言 candidate/clean；recorder 的 evidence-dir 指向主取证 worktree，不写 tested tree。Windows 同样必须有两棵独立树：immutable/clean tested candidate clone 与只承载 evidence 的 evidence clone；不能在被测树里生成或提交日志。传输必须先建可广告的命名 ref，不用裸 SHA 冒充 bundle tip：

```bash
set -euo pipefail
: "${AI_IP_CANDIDATE_SHA:?set frozen candidate SHA}"
: "${AI_IP_FROZEN_EVAL_WORKTREE:?set detached candidate worktree}"
: "${AI_IP_WINDOWS_CANDIDATE_BUNDLE:?set absolute output bundle path}"
test "$(git -C "$AI_IP_FROZEN_EVAL_WORKTREE" rev-parse HEAD)" = "$AI_IP_CANDIDATE_SHA"
test -z "$(git -C "$AI_IP_FROZEN_EVAL_WORKTREE" status --porcelain=v1 --untracked-files=all)"
git update-ref refs/heads/ai-ip-transfer/candidate "$AI_IP_CANDIDATE_SHA"
git bundle create "$AI_IP_WINDOWS_CANDIDATE_BUNDLE" refs/heads/ai-ip-transfer/candidate
git bundle list-heads "$AI_IP_WINDOWS_CANDIDATE_BUNDLE"
shasum -a 256 "$AI_IP_WINDOWS_CANDIDATE_BUNDLE" > "$AI_IP_WINDOWS_CANDIDATE_BUNDLE.sha256"
```

Windows 先校验传输 SHA-256 和 `git bundle list-heads` 中 `refs/heads/ai-ip-transfer/candidate` 的精确 SHA，再 clone；`git bundle verify` 需要 Git repository context，所以必须在 clone 后以 `git -C` 运行，不在空目录裸跑。Windows PowerShell 开头和核心命令固定：

```powershell
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
if ($PSVersionTable.PSVersion -lt [Version]'7.3') { throw 'PowerShell 7.3+ required' }
foreach ($name in @('AI_IP_WINDOWS_CANDIDATE_BUNDLE','AI_IP_WINDOWS_CANDIDATE_WORKTREE','AI_IP_WINDOWS_EVIDENCE_WORKTREE','AI_IP_CANDIDATE_SHA')) {
  if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($name))) { throw "missing $name" }
}
git -c core.autocrlf=false clone $env:AI_IP_WINDOWS_CANDIDATE_BUNDLE $env:AI_IP_WINDOWS_CANDIDATE_WORKTREE
git -c core.autocrlf=false clone $env:AI_IP_WINDOWS_CANDIDATE_BUNDLE $env:AI_IP_WINDOWS_EVIDENCE_WORKTREE
git -C $env:AI_IP_WINDOWS_CANDIDATE_WORKTREE bundle verify $env:AI_IP_WINDOWS_CANDIDATE_BUNDLE
git -C $env:AI_IP_WINDOWS_CANDIDATE_WORKTREE checkout --detach $env:AI_IP_CANDIDATE_SHA
git -C $env:AI_IP_WINDOWS_EVIDENCE_WORKTREE checkout -B ai-ip-windows-evidence $env:AI_IP_CANDIDATE_SHA
if ((git -C $env:AI_IP_WINDOWS_CANDIDATE_WORKTREE rev-parse HEAD).Trim() -ne $env:AI_IP_CANDIDATE_SHA) { throw 'wrong candidate SHA' }
if (@(git -C $env:AI_IP_WINDOWS_CANDIDATE_WORKTREE status --porcelain=v1 --untracked-files=all).Count -ne 0) { throw 'dirty candidate tree' }
if ((git -C $env:AI_IP_WINDOWS_EVIDENCE_WORKTREE rev-parse HEAD).Trim() -ne $env:AI_IP_CANDIDATE_SHA) { throw 'wrong evidence parent' }
if (@(git -C $env:AI_IP_WINDOWS_EVIDENCE_WORKTREE status --porcelain=v1 --untracked-files=all).Count -ne 0) { throw 'dirty evidence tree before capture' }
```

#### Contract 2：两平台运行与 baseline 同构的完整 required matrix

对每个平台不改 matrix，逐项用 `capture_command.py` 执行 Work Package 2 已冻结的全部本平台 `baselineAndPost` 和 `postOnly`。后者精确包含 domain/runtime/eval/responses-proxy、descendant raw event、AI IP strict-output App Server test、Rust Bazel test labels 和 Bazel asset build labels；以 machine matrix 为唯一数量权威，不使用“四个新 target”这类模糊计数。Windows 每次 recorder 调用的 `--repo-root` 必须是 `$env:AI_IP_WINDOWS_CANDIDATE_WORKTREE`，`--evidence-dir` 必须是 `$env:AI_IP_WINDOWS_EVIDENCE_WORKTREE/docs/evidence/foundation/windows-11-x64`；recorder 对两者 canonicalize 后要求不相同且互不包含。matrix 结束后被测 clone 的 HEAD/status 仍精确 candidate/clean。

每个 post manifest 绑定 `AI_IP_CANDIDATE_SHA`、host、arch、lock SHA 和原始日志 hash。新失败或无法与 baseline 精确对应的失败阻断 G1。完整 workspace `just test` 仍需依上游规则询问用户；批准/拒绝和 baseline/post failing test ID 写入 disposition，不能把拒绝伪装成通过。

#### Contract 3：安全回传并先提交全部 post evidence

Windows 只在独立 evidence clone 提交 `docs/evidence/foundation/windows-11-x64/`；被测 candidate clone 永远不写、不提交。evidence commit 的唯一 parent 必须是 `AI_IP_CANDIDATE_SHA`，diff 只能是该 Windows evidence 目录。PowerShell 精确封包：

```powershell
if (@(git -C $env:AI_IP_WINDOWS_CANDIDATE_WORKTREE status --porcelain=v1 --untracked-files=all).Count -ne 0) { throw 'tested tree changed during capture' }
git -C $env:AI_IP_WINDOWS_EVIDENCE_WORKTREE add -- docs/evidence/foundation/windows-11-x64
$changed = @(git -C $env:AI_IP_WINDOWS_EVIDENCE_WORKTREE diff --cached --name-only)
if ($changed.Count -eq 0 -or @($changed | Where-Object { $_ -notlike 'docs/evidence/foundation/windows-11-x64/*' }).Count -ne 0) { throw 'Windows evidence diff escaped allowlist' }
git -C $env:AI_IP_WINDOWS_EVIDENCE_WORKTREE diff --cached --check
git -C $env:AI_IP_WINDOWS_EVIDENCE_WORKTREE commit -m 'test: capture Windows native post-change evidence'
$evidenceSha = (git -C $env:AI_IP_WINDOWS_EVIDENCE_WORKTREE rev-parse HEAD).Trim()
if ((git -C $env:AI_IP_WINDOWS_EVIDENCE_WORKTREE rev-parse "$evidenceSha^").Trim() -ne $env:AI_IP_CANDIDATE_SHA) { throw 'wrong evidence parent' }
git -C $env:AI_IP_WINDOWS_EVIDENCE_WORKTREE update-ref refs/heads/ai-ip-transfer/windows-post $evidenceSha
git -C $env:AI_IP_WINDOWS_EVIDENCE_WORKTREE bundle create $env:AI_IP_WINDOWS_POST_BUNDLE refs/heads/ai-ip-transfer/windows-post "^$($env:AI_IP_CANDIDATE_SHA)"
(Get-FileHash -Algorithm SHA256 $env:AI_IP_WINDOWS_POST_BUNDLE).Hash.ToLowerInvariant() | Set-Content -NoNewline "$($env:AI_IP_WINDOWS_POST_BUNDLE).sha256"
```

macOS 必须在已含 candidate prerequisite 的 repo 里运行 `git bundle verify`，校验 `list-heads` 的精确命名 ref/SHA、唯一父链和 diff allowlist后才 cherry-pick。不得移动、reset 或覆盖任一原生 tested tree。

macOS post evidence 由 recorder 直接写入主取证 worktree 的 `docs/evidence/foundation/macos-x86_64/`，所以不需反向 bundle。Windows cherry-pick 完成后，更新 `workspace-suite-disposition.md` 和 patch ledger 的“post evidence 已采集/未通过”状态，此时严禁写最终 checkpoint。然后精确 stage 与提交：

```bash
set -euo pipefail
git add docs/evidence/foundation/macos-x86_64 docs/evidence/foundation/windows-11-x64 \
  docs/evidence/foundation/workspace-suite-disposition.md docs/architecture/codex-fork-patch-ledger.md
git diff --cached --check
git commit -m "test: capture native post-change foundation evidence"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
```

#### Contract 4：运行最终机器校验

最终 verifier 必须：

- 逐个复算 baseline/post manifest、stdout/stderr 与 lock hashes；
- 校验正确 platform/arch/tested SHA；
- 校验 exact smoke 名称来自 pinned source；
- 校验 license/NOTICE/完整 upstream ancestry，并读取 `.ai-ip/upstream.lock.toml` 的 `origin_status`：本计划值为 `unconfigured` 时实际 `origin` 必须不存在，不能伪造产品 URL；`upstream` 仍必须是锁定官方 URL。未来配置 origin 时必须先更新 lock/发布证据；
- 在 retention deadline 前由冻结 Rust `verify-live-proof` 从 private run root 重建 business report，重做整树 `(responseId,usage)`、cost/attempt/time/审批上限与 HMAC/Merkle 校验；不得只信公开 report 字段；
- 校验仓库没有 case、材料、Prompt 展开值、package、mapping、review、private provider/broker/cost receipt、transcript、thread/response ID 或 secret pattern；唯一 receipt 例外是到期后按 schema allowlist 的无正文 `retention-closeout.json`，verifier 必须检查其字段和 proof-root commitment；
- 做 shipping dependency/package negative scan：customer shipping target 的 normal dependency 不得包含 `codex-ai-ip-eval`；安装包与发布 filegroup 不得包含 evaluator/private fixtures、`codex-ai-ip-eval` 或 `codex-responses-api-proxy` 二进制和 stdin credential surface。`codex-ai-ip-eval` 保持 `publish=false`，proof-only broker/evaluator targets 保持 private/non-shipping，也不得实现生产 Capability Gateway、provider registry 或 route selection；经 positive control 证明的 `codex_responses_api_proxy` 低层 library codec/server API 可以复用，不能把整个上游 crate 误判为不可发布；
- 校验 shipping Skill/runtime/Cargo/BUILD 不引用六版永久禁用的 `map-marketing-content-world`、`ContentRootLab` 或固定内容根选择器；
- 校验主取证工作树 clean，独立 evaluator worktree 仍精确为 candidate/clean；并对 `candidate..HEAD` 做 exact partition：除已选 report/index bookkeeping、macOS/Windows native evidence、workspace disposition 和 checkpoint 前 patch-ledger evidence 外不允许任何路径，shipping code 必须与 candidate 字节一致。

```bash
set -euo pipefail
: "${AI_IP_FROZEN_RUN_CONTEXT:?set selected run frozen context}"
ai_ip_repo_root="$(git rev-parse --show-toplevel)"
ai_ip_candidate_sha="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["candidateSha"])' "$AI_IP_FROZEN_RUN_CONTEXT")"
ai_ip_public_run_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["publicRunId"])' "$AI_IP_FROZEN_RUN_CONTEXT")"
ai_ip_private_root="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["privateRoot"])' "$AI_IP_FROZEN_RUN_CONTEXT")"
ai_ip_business_report="$ai_ip_repo_root/docs/evidence/business-proof/$ai_ip_public_run_id/report.json"
ai_ip_report_index="$ai_ip_repo_root/docs/evidence/business-proof/index.json"
ai_ip_live_verification="$ai_ip_private_root/coordinator/live-proof-verification.json"
ai_ip_verification_next="$ai_ip_private_root/coordinator/verification.next.json"
test -f "$ai_ip_business_report"
test -f "$ai_ip_report_index"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
git merge-base --is-ancestor 4ef1d4b89bd419c976b04fefa0fd36844e898340 HEAD
git merge-base --is-ancestor 58baf3b HEAD
git merge-base --is-ancestor "$ai_ip_candidate_sha" HEAD
git diff --check
just fmt-check
just bazel-lock-check
python3 scripts/ai_ip/foundation/run_frozen_eval.py --context "$AI_IP_FROZEN_RUN_CONTEXT" -- \
  verify-live-proof \
  --report "$ai_ip_business_report" \
  --report-index "$ai_ip_report_index" \
  --commitment-key-file "$ai_ip_private_root/coordinator/commitment-key.bin" \
  --output "$ai_ip_live_verification"
python3 scripts/ai_ip/foundation/verify_evidence.py \
  --candidate-sha "$ai_ip_candidate_sha" \
  --selected-report "$ai_ip_business_report" \
  --report-index "$ai_ip_report_index" \
  --business-verification-receipt "$ai_ip_live_verification" \
  --verification-output "$ai_ip_verification_next"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
```

`verification.next.json` 仍在 private root，不会让“写仓库后又要求 clean”自我失败。它至少包含 `{schemaVersion,publicRunId,candidateSha,reportPath,reportSha256,reportIndexSha256,selectedAttemptOrdinal,proofRootSha256,frozenRunContextCommitment,executionContextCommitment,attemptIndexRootCommitment,brokerReceiptCommitment,costReceiptsCommitment,requiredMatrixSha256,macPostEvidenceCommitment,windowsPostEvidenceCommitment,candidateAllowedDiffSha256,liveProofVerificationSha256,G0,G1,G2,capabilityStatus,foundationDecision,verifierSha256,verifiedAt,verifiedBeforeRetentionDeadline}`，不含私有路径、正文、thread/response/reviewer ID 或 key。Python 只消费无正文 Rust receipt 和公开证据；若 private evidence 已按 retention 删除，Rust 步骤必须失败且本 pair 不能通过 checkpoint。

#### Contract 5：用 finalizer 发布 verification 并生成唯一 checkpoint

首次调用从 clean 主仓库进入；若上次调用发生已绑定半写，则由 Work Package 6 已 RED/GREEN 实现的 `finalize-checkpoint` 按私有 transaction marker 恢复。它重读 `verification.next.json`、report/index 选择、retention deadline、candidate allowed diff、origin status 和现有 patch ledger；只有 G0 来源/许可、G1 两平台 required matrix、G2 live blind rule 与 `BUSINESS_SIGNAL_PASS_PENDING_FOUNDATION` 全部通过时，才同时以 `create_new`/原子替换发布 `<publicRunId>/verification.json` 并在 patch ledger 写：

```text
Foundation checkpoint: PASS_TO_PHASE_0B
Authorized next plan: 02 — Mission, Lead, and Artifact kernel
```

否则 finalizer 只能写 `ITERATE_PHASE_0A` 或 `INVALID_PROOF` 与最小下一步；不得启动 localhost UI、SaaS、代理管理或媒体基础设施。手工向 ledger 粘贴 PASS 不构成有效 checkpoint。

```bash
set -euo pipefail
: "${AI_IP_FROZEN_RUN_CONTEXT:?set selected run frozen context}"
ai_ip_repo_root="$(git rev-parse --show-toplevel)"
ai_ip_public_run_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["publicRunId"])' "$AI_IP_FROZEN_RUN_CONTEXT")"
ai_ip_private_root="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["privateRoot"])' "$AI_IP_FROZEN_RUN_CONTEXT")"
ai_ip_business_report="$ai_ip_repo_root/docs/evidence/business-proof/$ai_ip_public_run_id/report.json"
ai_ip_verification_next="$ai_ip_private_root/coordinator/verification.next.json"
ai_ip_verification_output="$ai_ip_repo_root/docs/evidence/business-proof/$ai_ip_public_run_id/verification.json"
python3 scripts/ai_ip/foundation/run_frozen_eval.py --context "$AI_IP_FROZEN_RUN_CONTEXT" -- \
  finalize-checkpoint \
  --foundation-verification "$ai_ip_verification_next" \
  --report "$ai_ip_business_report" \
  --report-index "$ai_ip_repo_root/docs/evidence/business-proof/index.json" \
  --verification-output "$ai_ip_verification_output" \
  --patch-ledger "$ai_ip_repo_root/docs/architecture/codex-fork-patch-ledger.md"
git add "$ai_ip_verification_output" docs/architecture/codex-fork-patch-ledger.md
git diff --cached --check
git commit -m "test: close Codex AI IP foundation checkpoint"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
```

不要在 finalizer 前加外层 clean 断言：首次 clean 与 recovery 的 exact verification/ledger dirty allowlist 由 finalizer 一处裁决。finalizer 成功返回后才精确 stage、检查、提交并重新要求 clean。

#### Contract 6：向用户交付 checkpoint 报告并等待继续/纠正决策

本步不再生成一份可能泄露私有内容的仓库文件，而是从已验证 report/verification/ledger 渲染用户可读交付，必须逐项显示：业务结果与其严格限定；working/stubbed 能力；五级 capability status；macOS/Windows 测试与未运行项；两臂实际 token/时延/费用；私有数据位置、retention deadline/closeout 状态；已知风险；候选 Skill/证明 harness 的删除或回滚路径；以及“继续 Plan 02”或“纠正/重试”的明确决策点。只有 ledger 为 `PASS_TO_PHASE_0B` 时才能向用户推荐 Plan 02。

---

## Phase 0A acceptance contract

Phase 0A 完成必须同时满足：

- Codex 完整 ancestor、license/NOTICE、fork patch ledger 可复现；
- 未改与改后 native macOS/Windows 证据可区分；
- 没有 `codex exec` 评测桥、第二套编排器、固定 Agent DAG 或禁用子代理；
- App Server typed `additionalContext`、原生 Skill discovery、root/descendant raw usage 和 broker gate 均有测试；
- provider secret 从未进入 Codex 子进程、配置、环境导出、日志或 Git；
- `codex-ai-ip-eval` 保持 `publish=false`，proof-only broker/evaluator targets 保持 private/non-shipping；三类恶意 dependency/filegroup/package fixture 证明 evaluator 与 proxy binary 不进入 customer 发布物，同时 positive control 证明允许复用低层 proxy library，且整个 Phase 0A surface 没有演化为 Capability Gateway/registry/router；
- 一个真正新案例完成三人盲评和费用绑定；
- 仓库只有无正文 report + verification，以及到期后允许的无正文 retention closeout receipt；
- checkpoint 明确为 `PASS_TO_PHASE_0B` 才允许 Plan 02。

实施时若真实 pinned 源码与本计划接口不一致，先新增源码事实记录和失败测试，再用 `superpowers:writing-plans` 修订本计划；不得用临时 wrapper 绕开 Codex harness。
