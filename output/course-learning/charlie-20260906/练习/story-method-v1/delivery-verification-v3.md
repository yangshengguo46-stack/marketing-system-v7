# 交付核对 v3 实际操作与校验报告

## 执行日期
2026-09-09

## 执行目的
修复 v2 自查中的两个实际执行问题：(1) 自查称完整回读三阶段 body，实际只返回首尾片段和关键词检查；(2) draft 仍先公式再场面，与题目要求相反。另需保留 notes-v3-editorial.md 的本地编辑身份，不能将其算作原生模型自行修正。不重写故事。

## 实际操作记录

### 1. marketing_work open
通过 `marketing_work open`（只传 action=open 与 workId=charlie-method-v1-homework34）重新打开已有工作文件，未传新 brief/materials/startAt。返回 results[0/1/2] 的 preview 和 workFile 路径。

### 2. 完整读取原生 JSON 三个 results body
用 Python 逐个读取 `results[0].body`、`results[1].body`、`results[2].body`，每次显式 `max_output_tokens=6000`，未用首尾片段或关键词代替完整读取。

校验：
- results[0] body length=997 chars，以 `## 第二轮来源补读与旧漏读修正` 开头，以 `不为凑三角让人物无理由做反义动作` 结尾。完整无截断标记。
- results[1] body length=1131 chars，以 `## 第二轮返工取舍` 开头，以 `方法为候选副本，未经本轮完整作品检验` 结尾。完整无截断标记。
- results[2] body（record 前的旧 draft）length=1697 chars，以 `# 第34课小三角试作 v2 · 完整场面与公式` 开头，以 `方法为候选副本未经完整作品检验。` 结尾。完整无截断标记。

旧 draft (results[2]) 结构为：先公式（三角公式与关键词）再场面正文——即"先公式再场面"，与题目要求的"先场面再公式"相反。此为本轮修复目标之一。

### 3. 完整读取 scene-v2.md 和 notes-v3-editorial.md
分别用 `cat` 完整读取，`max_output_tokens=6000`。

- scene-v2.md：819 chars，以 `# 收摊` 开头，以 `但每一下都利落得像是在赶一段路。` 结尾。这是原生作者的完整修后正文（场面在前，无公式段）。
- notes-v3-editorial.md：2033 chars，以 `# 小三角公式、关键词与选择说明（v3，本地编辑校正）` 开头，以 `"针扎偏→惦念泄露"的具体效果仍待实际读者检验。` 结尾。文件第一段明确标注：由根Lead从 notes-v2.md 另存，只校正两处说明措辞（读者感受→预期，"不可能再次意外"→缺少新信息依据），未改故事、未再调用原生模型或重新 record，原生 v2、自查与工作文件均保留。

### 4. marketing_work record draft
通过 `marketing_work record`（action=record, workId=charlie-method-v1-homework34, stage=draft）保存 body。

body 构成：
1. scene-v2.md 全文（逐字，819 chars，rstrip 后 818 chars）
2. 分隔线 `\n\n---\n\n`
3. notes-v3-editorial.md 全文（逐字，2033 chars，rstrip 后 2032 chars）

顺序为"先场面后公式"，修正了旧 draft 先公式再场面的顺序问题。

body 保留了 notes-v3-editorial.md 中的本地编辑说明段落（"本文件由根Lead从实际原生 notes-v2.md 另存，只校正两处说明措辞……不能把本文件算作模型自行修正"）。

未添加新剧情、评价或自查胜利宣言。未改变 research/direction 及旧文件历史。

### 5. 校验

第一次 record 后 Python 整串比较发现 2 处差异：index 905 和 913，draft 使用直引号 U+0022，文件使用中文弯引号 U+201C / U+201D。差异落在 notes-v3-editorial.md 第一段"将'不可能再次意外'改为缺少新信息依据"一句。

修复方式：以正确的中文弯引号（"…"，U+201C / U+201D）重新 record draft。

第二次 record 后 Python 整串比较结果：
```
draft_body length: 2857
expected_body length: 2857
EXACT MATCH: True
draft starts with scene: True
draft starts with formula: False
```

- 整串等值：True（body == scene-v2.md rstrip + "\n\n---\n\n" + notes-v3-editorial.md rstrip）
- 长度匹配：2857 == 2857
- 开头为场面（# 收摊）：True
- 开头为公式（# 小三角）：False

校验通过，未用 shell 覆盖原生 JSON。

## 原生 v2 的假自查

v2 自查声称"回读三阶段完整 body"，但实际只返回了首尾约 120 字符的片段和关键词存在检查，未完整读取 results[0/1/2] 的 body 全文。本轮以 Python 逐个完整读取三个 body（997/1131/1697 chars），确认无截断。

v2 draft（results[2] 旧版）结构为先公式再场面，与题目要求的"先场面再公式"相反。本轮 record 修正为"先场面后公式"，经整串比较确认。

## notes-v3-editorial.md 的本地编辑身份

notes-v3-editorial.md 由根Lead（本地编辑）从原生 notes-v2.md 另存，只校正两处说明措辞：
1. 将"读者感受"改为"创作预期"（不把未验证的读者效果断言为已发生）
2. 将"不可能再次意外"改为"缺少新信息依据"（更准确描述逻辑问题）

此文件不改变故事正文，不重新调用原生模型或重新 record 原生 results[0/1]。原生 v2 的 results、自查和工作文件均保留原样。本轮 record 的 draft body 将 scene-v2.md（原生作者正文）与 notes-v3-editorial.md（本地编辑校正）组合保存，notes 中的本地编辑说明段落完整保留在 body 中。

## 仍未知

以下事项未通过或未验证，不能写成已通过：

1. **质量**：故事"收摊"的文学质量未经真人编导或目标读者验收。独立读稿报告是 GPT-6 模型意见，非真人验收。
2. **读者效果**：修正后读者是否实际感受到"惦念"，未经目标读者验证。notes-v3-editorial.md 明确将读者感受改为"创作预期"，不断言已发生。
3. **方法稳定性**：小三角方法为课堂候选副本，课堂示范写法本身也未经读者效果证明。
4. **"针扎偏→惦念泄露"效果**：具体效果仍待实际读者检验。
5. **自主执行稳定性**：本轮修复了 v2 的假自查和顺序问题，但本 Lead 的稳定自主执行能力未经重复验证——首次 record 出现弯引号丢失（2 处差异），第二次 record 后才通过。
