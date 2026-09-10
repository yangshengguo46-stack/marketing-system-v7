# 存储命令的事务参考

以下是拟实现的 SQL 事务行为，不是已经存在的 Rust 方法。调用者身份、scope 和 fencing token 由主机获得，不信任模型自填。数据库访问必须集中在 store crate。

## 任务更新

```sql
BEGIN IMMEDIATE;
-- 1. 查询 command_dedup，已有同请求 hash 则返回原响应；不同 hash 则回滚冲突。
-- 2. 核对 mission_leases holder/fencing/expiry，若该命令需要 Lead writer。
UPDATE missions
SET objective = :objective,
    contract_json = :contract_json,
    revision = revision + 1,
    updated_at = :now
WHERE workspace_id = :workspace
  AND project_id = :project
  AND id = :mission
  AND revision = :expected_revision
  AND status NOT IN ('cancelled','failed');
-- changes()!=1 → ROLLBACK + REVISION_CONFLICT 或状态错误。
-- 3. 在同一事务取该 mission 下一 event_seq，写入事件。
-- 4. 写入 command_dedup 的 request hash 与确切 response。
COMMIT;
```

不能先更新任务，之后另一个事务补事件；崩溃会造成 UI 永久缺失变化。

## 预算预留

```sql
BEGIN IMMEDIATE;
UPDATE budget_accounts
SET reserved_micros = reserved_micros + :amount,
    revision = revision + 1
WHERE workspace_id=:workspace AND project_id=:project
  AND mission_id=:mission AND currency=:currency
  AND blocked=0
  AND limit_micros - settled_micros - reserved_micros >= :amount;
-- changes()!=1 → ROLLBACK + BUDGET_EXCEEDED。
-- 检查此 preparation 的 approval：作用域、账号、payload hash、有效期、撤销、可用次数。
-- 插入 budget_reservations 与 prepared action_executions；消耗一次授权使用额度。
COMMIT;
-- 事务外调用网络；出错不直接判定外部未执行。
```

外部调用结果不明时保留预留为 unreconciled。真实结算超过估价，真实金额必须入账并 blocked=1 暂停后续花费；不能因余额约束而拒绝记录真实账单。

## 正式交付

```sql
BEGIN IMMEDIATE;
-- 1. 检查任务仍可交付且 mission_revision/input_epoch 未变。
-- 2. 检查所有工件仍对应候选 digest，未受影响依赖变更。
-- 3. 检查 persisted review 报告绑定相同 candidate hash，允许交付且没有安全禁止。
UPDATE release_candidates
SET status='released', released_at=:now
WHERE workspace_id=:workspace AND project_id=:project AND id=:candidate
  AND status='reviewing';
-- changes()!=1 → 回滚；不得重复创建 release。
-- 4. 更新 mission 为 delivered；追加 release.created 事件。
COMMIT;
```

这里的 delivered 不等于用户已经采用或内容已经发布。事件广播在事务之后，可以重试；不能触发重复业务动作。

## SQL 没有单独保证的事情

SQL 起稿约束作用域外键、有限 enum、JSON 语法、去重 key 和部分字段。但下列必须由主机事务服务与测试实现：权限来源、状态迁移合法性、授权与动作的确切账号/digest/mission 一致性、不可变版本写策略、span真实性、依赖无环、完整 hash 校验、模型不得自造 ActualResult、revision和审核间竞态、预算reservation与计数一致性。

不要把迁移能执行或本包21个合同测试通过，解释为这些业务/安全行为已经被实现。
