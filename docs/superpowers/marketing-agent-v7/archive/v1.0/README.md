# Marketing Agent v7 最终施工包

日期：2026-09-10  
基线：`yangshengguo46-stack/marketing-system-v7@e08f1e1ad5bbc3ea036aa271894d8228e499ed61`

## 从这里开始

先读 `FINAL_SPEC.md`，再按 `IMPLEMENTATION_TASKS.md` 工作包执行。`SOURCE_MAP.md` 说明哪些现状已核对、哪些是新增设计。

该方案允许修改自有 Codex Core，不要求持续同步上游；仍保留现有原生循环、App Server 和领域资产。一个 Lead 负责结果，业务路径自主；确定性权限、成本、状态和回执由代码保护。

## 文件

- FINAL_SPEC.md：26章产品/架构/接口/安全/质量/迁移/测试规格。
- IMPLEMENTATION_TASKS.md：WP00–WP10共11个可提交工作包。
- contracts/001_initial.sql～004_execution.sql：分期的26张表起稿。
- contracts/completion_api.rs：新的原生完成接口草案。
- contracts/STORE_TRANSACTION_REFERENCE.md：CAS、交付与费用事务参考及应用层责任。
- contracts/review.schema.json、release_candidate.schema.json、review.example.json：严格JSON合同与样例。
- contracts/defaults.example.toml：拟新增配置，默认关闭未授权外部执行。
- tests/test_contracts.py：21个SQLite/JSON/TOML合同原型测试。
- VALIDATION_LOG.txt：本次本地验证输出。

## 已实际验证

Python unittest：21项通过。检查包括复合作用域外键、条件CAS、重复键、事务回滚、字段约束、费用预留条件、JSON Schema与安全默认配置。

## 没有执行

没有修改用户GitHub仓库；没有在v7中合并任何补丁；没有运行Rust workspace编译或产品端到端测试；没有调用付费模型/生成媒体/授权平台发布；没有执行真实客户数据迁移。Rust接口草案和配置键是拟新增设计，需要施工时接入真实符号、依赖和协议。

## 运行包内测试

需要Python 3.11+；JSON Schema测试使用jsonschema，未安装时该组会明确跳过。当前交付日志中该组已运行并通过。

```bash
python -m unittest discover -s tests -v
```

此命令只验证合同起稿，不是产品业务验收命令。生产验收仍要求真实国内模型、首次可采用作品、恢复/授权/执行安全，以及适用平台与设备实测。
