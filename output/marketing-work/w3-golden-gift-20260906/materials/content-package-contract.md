# 现有 ContentPackage 合同

content-package.schema.json 直接通过当前编译的 codex_ai_ip_runtime::content_package_schema() 导出，不是另造结构。
最终 JSON 必须包含完整 publishableContent.body 与具体 productionNotes，不能只给路径或摘要。
readiness=draft 允许保留 openQuestions；readyForHumanReview 不允许未知事实/openQuestions，所以本次资料不全时使用 draft，不能为了通过删除缺口。
UserFact/ExternalEvidence 要有来源；ActualResult 必须有 resultReceiptRef，未实际发生的传播或成交不得标 ActualResult。所有其余 resultReceiptRef=null。
本轮生成和人工编辑记录放 run/，用户作品放本案例目录。作品演绎是 CreativeHypothesis，方向是 ModelInterpretation，不把 V6 生成文本或心理推断写成用户事实。
