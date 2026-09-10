# Marketing Agent v7 业务与架构融合包 v1.2

1. `BUSINESS_ARCHITECTURE.md`：先看这份。以本次附件的业务概念与v7编剧资产为主，说明业务如何参与同一个原生Lead。
2. `FINAL_SPEC.md`：业务章节已合并进原工程规格，并替换主身份、方法库，补充质量标准与施工优先级。
3. `IMPLEMENTATION_TASKS.md`：业务任务B00–B06与原WP00–WP10对齐。
4. `contracts/`、`tests/`、`SOURCE_MAP.md`：沿用v1.0工程起稿，不是已编译集成代码。本次没有补写业务类型的Rust实现。
5. `BASELINE_v1_0_VALIDATION_LOG.txt`：仅历史原型记录；不证明本版业务增益。

本版不修改GitHub仓库，不自动调用付费模型，不发起发布或数据迁移。所有NEW接口是目标合同，实施时以实际源码适配。当前课程方法的候选状态和阴性结果保持不变。

本次只检查了文档生成、必要章节存在、代码围栏闭合、可解析文件与包完整性；没有运行Rust集成、生产模型或客户业务验收。
