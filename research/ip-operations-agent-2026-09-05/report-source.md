# 以 Codex 为底座，做出真正能持续工作的 IP 运营 Agent

第七版北极星下的实现研究

研究日期：2026 年 9 月 5 日  |  面向：产品负责人、业务负责人及研发团队

研究依据：第七版现役规格与计划、前几版历史台账及关键 Git 提交、原始论文、官方开源实现。

本报告回答如何实现完整 IP 运营产品。营销判断、跨行业迁移、创作与持续学习是其中的能力问题。报告提出实现与验证建议；没有修改产品代码、现役计划或发布授权，没有运行新的付费模型实验。

## 01 直接结论：保留第七版，改变能力落地与积累的方式

第七版的北极星是：一站式帮助用户持续产出更可能成为爆款的内容，建立影响力，并带来可衡量的关注、信任或业务行动。其产品范围明确覆盖业务理解、IP 策略、研究、选题、叙事、编导制作、成片、人工发布、数据回收与策略学习。[第七版总体规格，§1](</Users/yangyucheng/Documents/ChatGPT/第七版营销系统/docs/superpowers/specs/2026-08-24-ip-agent-saas-design.md:12>)

**我的建议是继续在第七版 Codex 上做，不启动第八次架构重写。** 第七版已经有一个 Lead、动态调用能力、项目产物、真实证据和发布学习等正确方向。本次研究没有找到一套经过验证、可直接替换进来便能实现该北极星的开源系统。可行的路线，是把这些已有设计变成能在真实 IP 中连续工作的业务能力，并用每次实际交付改进它。

这里的“连续工作”有两个同时成立的要求。运行上，它记得已确认的目标，接得上昨天的工作，能完成成片和版本交付。业务上，它能依据新的材料作出有价值的取舍，内容值得用，后续调整有依据。**完整走过流程不等于会运营；一次好答案也不等于拥有一个 IP 运营 Agent。**

如果由我负责实现，近期会作出五个决定：

- 保留 Codex 原生执行循环，让同一个 Lead 对 Mission 的交付负责；具体业务对象沿用第七版已有命名和边界。
- 把真实项目材料、已采用的判断、内容版本和反馈接到执行现场。模型每次都能取得当前有效资料，而非靠一大段聊天摘要猜测。
- 从旧版提取有证据的局部资产：阅读方法、叙事与证据的传递、素材登记、成片身份、失败恢复及相应反例。逐件迁移，不整代移植。
- 每个近期工作切片同时交付可用内容和质量证据；遇到失败再定位资料、模型、技能、衔接或制作上的具体问题。
- 先把一轮交付接到下一轮运营，再逐步形成项目经验。默认 Skill 的修改继续做有边界的离线实验，自动优化器后置。

这是一条有证据支持的实现路线，不是“某组模块装好就必然得到顶级运营”的证明。历史上已有局部正例，值得继续；目标模型在充分材料下的跨情境质量、成片稳定性和长期业务收益，仍需真实运行证明。

## 02 用户最终应该得到怎样的工作体验

用户告诉系统想建立什么影响力、希望谁采取什么行动，并提供已有材料。系统只问当前会改变决策的关键问题，开始调查、取舍和制作。用户交付的是现实信息、重大方向选择及必要授权；普通研究、选题、编排、制作和修改由 Lead 推进。

一个合格的产品，应能处理下列不同入口，而不强迫用户从头“走流程”。

| 用户当前处境 | Agent 应承担的工作 | 用户看到的结果 |
| --- | --- | --- |
| 不知道账号该做什么 | 识别主体、受众关系、目标和现有素材；形成可验证方向，安排必要的材料获取 | 具体主张、依据、探索内容与最小下一步 |
| 已有定位，要持续更新 | 继承有效定位，从现实事件、素材和受众反馈中找内容；安排能完成的生产组合 | 与账号一致且彼此有差别的可用内容 |
| 已有选题或脚本 | 直接研究缺口、改稿、导演、制作、检查和交付 | 真人拍摄包、成片或任务要求的其他正式产物 |
| 发布后效果不理想 | 核对实际发布版本和数据，区分注意力、留存、受众与行动问题 | 有依据的修订、下一轮内容或待验证解释 |
| 现实条件改变 | 接收新事实，判断哪些旧决定受影响，局部重做 | 清楚的变更、可回退版本及后续执行 |

IP 的长期一致性，也不等于每条都重复一个“内容根”。个人 IP 要保持真实身份、表达和经历的连续性；组织 IP 要保持承诺、责任和不同出镜者之间的一致性。形式、选题和战术可以变化。[第七版总体规格，§7.5、§8](</Users/yangyucheng/Documents/ChatGPT/第七版营销系统/docs/superpowers/specs/2026-08-24-ip-agent-saas-design.md:281>)

以规格中的直播公会为例，系统应理解主体是组织、结果包括合适达人报名与后续激活。它需要取得真实的工作场景、支持方式和成长证据，才能策划内容。换一位运营人员出镜，不必重新建立一个 IP；出现报名多但适配率低的反馈，应调查受众与承诺是否失配，而不能只继续优化播放量。本例解释验收要求，不提供可写入生产提示词的行业答案。

对于只有身份标签的素人，系统要帮助发现其可持续表达的真实材料和兴趣；不能凭标签发明履历或自动指定一个赛道。它可以一边观察和访谈，一边完成不依赖未知事实的小型创作。原规格的五类项目正是为了检验这种差异，不能当作生产模板。[五类验证项目](</Users/yangyucheng/Documents/ChatGPT/第七版营销系统/docs/superpowers/specs/2026-08-24-ip-agent-saas-design.md:63>)

## 03 历史说明了什么：可继承的资产比“上一版输赢”更重要

本次按关键提交核查代码与台账。V1 至 V3 主要由同一 Marketing OS 仓库的演化重建，不能假装存在三个完整、独立冻结的版本库。V4、V5、V6 及早期仓库当前均有未提交改动；下列结论以明确提交为准，不把工作树新增文件计作旧版冻结成果。

| 历史证据 | 实际支持的判断 | 应继承的资产 |
| --- | --- | --- |
| 早期素材与指标循环 | 有视频登记、裁片、来源和媒体检查代码；有到期回收、延期、恢复和学习候选接口。真实平台指标循环仍未完成验收 | 素材身份、时间码、权限、哈希；到期事件与恢复测试 |
| V4 正式成片封存 | 成片需绑定实际文件、制作执行、质量记录和费用；旧执行的 QA 不能批准新执行 | 真实字节交付、版本谱系、局部恢复与“不误报完成”合同 |
| V5 E39 | 相同材料下，阅读方法的人工分从 23/36 到 31/36，事实失败从五例到两例；仍未通过完整门槛 | 阅读理解正反例、对照材料、事实与解释区分 |
| V5 A59、E40 | “雪茄咖”存在词义干扰；改为“雪茄馆”后最终根恢复正确。仍有开放发现、立题与事实问题 | 歧义对照、发现与验证分开、证据到选题的衔接反例 |
| V6 A122 | 已形成的叙事骨架曾未传入成稿输入；修复后，在完整证据下出现真实有效基础稿 | 可选叙事信息与证据引用的连续传递、接口测试 |
| V6 A132 | 公平对照下，一张全局业务注意力卡总体没有带来收益；个案有升有降 | 实际请求清单与逐案比较；不继承该全局注入 |
| V6 A136 | 给定方向仍可能找不到关键一手证据、误写动机，或停住而无替补交付 | 请求不等于事实、方向继承、候选替补需求与错误归因案例 |
| V6 A149 | 通用孵化在工业安全培训、公会中有部分迁移；仍有未支持主张。预算实现为进程内状态 | 受众与业务目标区分、父子共享预算等不变量，重做产品实现 |

证据入口：早期素材提交 `c3ce67266`、指标循环 `42a9978a0`，见 [Marketing OS 执行台账](</Users/yangyucheng/projects/marketing-os-desktop/docs/marketing-os/current/EXECUTION_LEDGER.md:125>)；V4 成片合同 `da5e23eb`，见 [final_artifacts.py](</Users/yangyucheng/Documents/第四版营销系统/backend/packages/harness/deerflow/personal_ip/final_artifacts.py:499>)；V5 E39 结果 `add604c3`、A59 修正 `8d9defec`、E40 结果 `65e5716e`，见 [V5 台账](</Users/yangyucheng/Documents/ChatGPT/第五版营销系统/docs/mcn-incubation-v5/LEDGER.md:1134>)；V6 A122 `085d00fd`、A132 结果 `02c3bacb`、A136 `d14ca51f`、A149 `326eab3b`，见 [V6 台账](</Users/yangyucheng/Documents/ChatGPT/第六版营销系统/docs/content-intelligence-v6/LEDGER.md:2344>)。

三项纠偏直接影响设计。第一，不能用 A58 单题证明通用根选择必然失败，因为 A59 已作词义修正。第二，E40 没找到隐藏目标人物，不等于没有任何有用选题；应分别评估发现能力、事实质量和实际价值。第三，A122 与 A136 不矛盾：前者证明某种充分材料与正确接线能产出好稿，后者揭示自主研究仍可能找错、读错或无法继续。A122 同时修改多个因素，也不能把全部收益只归给一条提示方法。

还有一项不应迁移的“经验”：早期代码曾把互动近似为信任。这是未校准代理指标，不能成为北极星中真实信任或商业行动的测量真值。[旧指标实现](</Users/yangyucheng/projects/marketing-os-desktop/agent/marketing/intelligence/metric_labels.py:88>)

## 04 第七版站在哪里：设计明确，完整运营能力尚待形成

审计快照：主目录 `177d4b6db48b75b6d2658b8337101b02390c5e05`；07B 工作树 `04b3e8bee6c2d0d23b152654eeeadc7011f0a66c`；锁定 Codex 上游 `4ef1d4b89bd419c976b04fefa0fd36844e898340`。两处第七版代码树在本次报告创建前干净。07B 已有实际实现，不能依据过时计划指针说它没开工。

| 北极星需要的能力 | 已有设计或实现 | 本次可见证据的边界 |
| --- | --- | --- |
| 自主执行与工具协作 | Codex 原生 Thread、Turn、Skill、MCP、恢复与临时子任务；V7 有配对执行和记录代码 | 执行机制本身不证明会做 IP |
| 业务理解与内容交付 | Domain 已有 InfluenceRelation、ActionFunnel、ClaimStatus、ContentPackage；有最小 Lead Skill | 当前合同主要描述一次短内容交付 |
| 长期项目与连续运营 | 规格已定义 Project、Mission、Artifact、Prediction、Publication、MemoryRule | 尚无本次可核实的完整持续运营证明 |
| 内容材料与研究 | 规格已有来源与认识状态；实验室有材料投影和隔离 | 当前黄金 fixture 的 knownEvidence 主要是查询回执元数据，没有真实结果正文 |
| 质量验证 | 07A 已实现 synthetic 流程；07B 有隔离与封存代码 | synthetic 强答案及评审不能当真实营销效果 |
| 成片、发布、学习 | 总路线 Plans 09、10 明确拥有 | 不能从前期机械测试推定完成 |

关键代码：[ContentPackage 合同](</Users/yangyucheng/Documents/ChatGPT/第七版营销系统/codex-rs/ai-ip-domain/src/content_package.rs:82>)、[当前 Runtime 提示](</Users/yangyucheng/Documents/ChatGPT/第七版营销系统/codex-rs/ai-ip-runtime/src/prompt.rs:4>)、[case_factory 实际投影](</Users/yangyucheng/Documents/ChatGPT/第七版营销系统/scripts/ai_ip/eval_lab/case_factory.py:234>)、[07B 执行控制器](</Users/yangyucheng/Documents/ChatGPT/第七版营销系统/.worktrees/07b-isolated-batch-runner/scripts/ai_ip/eval_lab/batch_controller.py:270>)。

以上不是对未来设计能力的否定。尤其材料不足结论只针对已检查 fixture，不能推出全部产品研究输入都如此。更准确的判断是：目前证据还不能分清模型能力不足、输入不足、方法不合适、衔接丢失和评价偏差各占多少。

已有隔离与评测工程解决了真实问题，应保留。研发节奏需要调整的是：尽快让它服务于真实业务判断和产物，避免机械证明不断增长，而下一次有信息量的业务实验一直未发生。

## 05 以完整运营周期组织产品，以当前问题组织执行

[[OPERATING_LOOP]]

图中表达业务之间的关系。实际 Mission 可以从任意位置进入，跳过不需要的步骤，也可以回到研究、只重做一个镜头，或直接处理数据反馈。它不规定一个固定 Agent 队列。

**长期 IP 项目与单次 Mission 是两个时间尺度。** 项目保存已确认的主体、承诺、受众与历史；Mission 是当前要完成的工作。一次 Mission 结束不等于 IP 运营结束。新的真实材料、用户要求、到期回收和异常结果可以成为下一次工作的入口。

这不要求再造一个独立“总运营 Agent”给 Lead 下指令。项目中只需保留待处理事项及其触发条件，由既有本地运行服务唤起同一业务入口。每次 Lead 判断当前值得做什么；不能因“持续运营”就默认全天运行、自动发布或不断消耗预算。触发服务负责时间和状态，Lead 负责内容与取舍。

运行证据与业务权威必须区别。Thread 记录执行过程；有效业务状态来自已提交、带版本与来源的项目产物。新会话需要取回当前版本、相关材料和未完成事项，不是把全部历史聊天和方法手册重新灌入上下文。[第七版 Codex 映射与数据边界](</Users/yangyucheng/Documents/ChatGPT/第七版营销系统/docs/superpowers/specs/2026-08-24-ip-agent-saas-design.md:167>)

长期 Agent 的工程研究也发现，单靠压缩上下文不足以保证持续完成任务；清楚的进度产物和可观察验证有帮助。不过 Anthropic 的相关演示针对软件开发，不能直接当 IP 运营能力证明。[Justin Young，Anthropic，2025-11-26](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents)

建议第一条连续工作验证直接使用现成文件与版本保存能力，安排一次真实中断和续做。只有观察到具体状态丢失、重复生成或恢复失败，才扩展相应运行时能力。正式客户产品仍按既有计划建设加密存储、更新和恢复，不能以研发样本替代发布要求。

## 06 持续理解一个 IP：保存现实，而不是固化模型猜测

沿用现有领域对象即可，近期无需新增庞大的知识图谱平台。关键是让每条影响决策的信息有明确身份：已确认约束、可核验事实、待验证假设、观察结果。这是阅读视图，底层继续使用第七版已有六类 ClaimStatus；用户原话有来源，也不自动等于外部事实已经核验。

| 需要持续保存的内容 | 权威与更新方式 | 给当前任务的用途 |
| --- | --- | --- |
| 主体与已确认承诺 | 用户确认的版本；重大变化另形成候选 | 判断谁能说什么，保持个人或组织身份 |
| 受众与行动关系 | 一部分是事实，一部分是有依据的假设 | 确定当前内容希望改变谁的认知与行为 |
| 现实材料与来源 | 原文件、原话、时间码、来源片段、时间及限制 | 提供能研究、能创作、能拍摄的内容 |
| 采用和拒绝的决定 | 记录当时条件、依据、结果及必要解释 | 避免重复争论，允许新证据推翻旧判断 |
| 栏目与内容历史 | 已完成、待完成、实际发布版本与依赖 | 连续生产，识别重复和承诺未兑现 |
| 结果与经验 | 原始观察保留；经验带适用条件和反例 | 支持下一轮假设与低风险调整 |

资料获取本身应该是一项运营能力。缺少内容时，系统要判断需要用户提供哪一次真实经历、哪段现场记录、哪种客户疑问或哪个可观察过程，并把获取要求变成简单可执行的行动。对外部公共议题则主动调查。它不能把公开行业常识当成这个主体的独家经历。

**信息采集的目标是改变下一次决策。** 已有素材足以做一条演示，不必继续追问完整人物生平；某条商业主张缺证据，则先补该证据或改用可成立的表达。不同未知项的优先级不同，这比做一张“全字段完美画像”更接近运营工作。

LongMemEval 把长期记忆拆成提取、多会话推理、时间推理、知识更新和适当不回答。这些维度适合转成项目回归题：能否区分旧承诺与新承诺、不同账号的材料、上次草稿与实际发布版。它的 500 道问答不是长期 IP 经营实验，因此只借其诊断维度。[Wu 等，ICLR 2025，LongMemEval](https://arxiv.org/abs/2410.10813)

## 07 让 Lead 做有依据的取舍，而不是只完成表格

Lead 的核心工作是从当前目标和现实材料中决定下一项有价值的动作。研究、访谈、类比、生成多个创意、独立审查和直接制作，都是可选动作。默认只保留简短的决策说明：采用什么、依据什么、什么还未知、出现什么新信息应重看。无需要求保存模型的隐藏思维链，也不能把一段流畅解释当成真实因果证明。

判断可以借助下列问题，但它们不是每次都要填写的硬表单：

- 这条内容为何值得目标受众看完，它承诺并兑现什么？
- 这个主体是否有可信的立场、证据与资源来表达？
- 它怎样积累关注、信任或行动，而不只是临时制造播放？
- 现有时间、素材、表演和制作条件是否能支撑？
- 再研究或多做一个候选，是否可能改变选择，值得额外成本？

不确定性高时，候选之间应在选题机制、受众认知变化、证据或表达形式上有实质差别。明确任务不必强制多个候选。评价应允许多个方向成立，比较具体取舍；不能把负责人脑中某个未公开的人名或答案当作唯一正确解。

研究的改进重点是“发现新信息”。先记录当前可能影响选择的未知，读资料后更新问题；从真实来源中发现新人物、事件和反例，而不是只搜索模型已经想到的人名。读到的证据应带着原始片段、指向对象和限制进入 TopicCard、CreativeBrief 与 Script，不能在层层摘要中消失。

STORM 的方法可帮助组织调查，Co-STORM 则尝试从未用资料中引出新问题。其研究对象是知识探索和长文；这支持方法候选，不能证明会选营销题。源码只按 URL 合并并对片段精确去重，也不能证明多个网站是独立证据。[Shao 等，NAACL 2024](https://aclanthology.org/2024.naacl-long.347/)、[Jiang 等，Co-STORM，2024](https://arxiv.org/abs/2408.15232)、[STORM 去重源码](https://github.com/stanford-oval/storm/blob/fb951af7744dab086e34962e9bc6fe878e145f83/knowledge_storm/storm_wiki/modules/storm_dataclass.py#L66)

为避免“越研究越停不下来”，建议研究产物同时交付：可使用的发现、关键冲突、对当前选择的影响、可继续追的线索。当主要选择已不再受剩余未知影响时停止；某个候选证据不足，换有根据的候选或交付明确的材料获取任务。停止编造与继续推进可以同时成立。

## 08 跨行业泛化：迁移有条件的方法，并在现实中重新落地

用户需要跨行业能力，这一点不应缩减。但“跨行业”至少包含六种变化：个人与组织主体变化、受众角色变化、目标行动变化、材料充分度变化、制作资源变化、平台与时间变化。只换行业名而保持同一模板，不能充分验证泛化。

旧版内容地图可保留为一种临时研究产物：帮助展开可能的事件、关系、问题与材料。它不能成为所有任务必须先通过的唯一选根器，也不应自动成为账号的永久承诺。本建议不恢复第七版明确禁止调用的旧内容地图 Skill。[第七版历史经验边界](</Users/yangyucheng/Documents/ChatGPT/第七版营销系统/docs/superpowers/specs/2026-08-24-ip-agent-saas-design.md:924>)

可迁移经验应同时保留两部分：具体案例提供事实与执行细节；方法说明指出关系结构、成立条件、不可照搬之处。比如“客户事前难判断质量，因此用真实过程降低不确定性”可以成为跨领域候选方法；具体证明材料、承诺和拍摄方式必须重新适配。该例是设计解释，不是已验证的通用营销定律。

认知研究提供了有限但有用的依据。Loewenstein、Thompson、Gentner 的人类谈判实验中，比较相同案例使后续采用条件契约的比例由 16% 到 48%；但总体谈判收益差异并不显著。2003 年的后续研究继续支持结构比较帮助原则迁移。它们不证明 LLM 已学会开放营销判断，却提醒我们分别检验“方法被采用”和“业务有收益”。[1999 作者原文](https://groups.psych.northwestern.edu/gentner/papers/LoewensteinThompsonGentner99.pdf)、[2003 作者原文](https://groups.psych.northwestern.edu/gentner/papers/GentnerLoewensteinThompson03.pdf)

近期 LLM 研究也发现，按子任务提炼的文字经验平均比整项任务轨迹更容易迁移；但研究使用英文 AppWorld、OfficeBench、KramaBench，有模型例外，且目前是 2026 年 8 月预印本。本项目可试验“小范围方法经验”，不能照搬其效果量或把代码工具视为无用。[Feng 等，2026-08-20](https://arxiv.org/html/2608.20274v1)

因此，V5 的 Skill 资产应重组为按任务需要可找到的短方法与参考材料。例如查证一个动机、保留叙事证据、对齐口播与镜头、校对字幕、核实发布版本。对“整个行业如何起号”的大答案保持审慎。技能目录需要检验实际选择与使用，而非以文件数量评估能力；另一项新预印本也提示技能可能稳定程序执行，但会受检索困难和不相容假设影响。[Jiang 等，2026-08-14](https://arxiv.org/abs/2608.14036)

训练材料、开发诊断和最终未见测试必须分开，最好按项目与来源家族隔离，再加入时间变化。已经公开的旧案例可用于理解和回归，不能继续宣称是未见案例。不得自动跨客户使用私有经验；通用方法库只来自公开可用材料或有明确授权的提炼。

## 09 创作与媒体：让内容意图真正进入画面和声音

从战略到好内容之间，还隔着具体选题、证据组织、叙事或论证、语言、表演、画面与剪辑。增加营销规则不会自动填满这些能力。第六版 A122 提醒我们：即使上游已经想对，下游也可能没有得到关键材料。

建议以已有 CreativeBrief 为贯穿对象。它按任务保留要表达的变化、关键证据、观点和生产条件；需要故事时带上人物行动与结果，演示或解释型内容允许采用其他结构。每个镜头应说明它提供什么信息、情绪或证据，避免最终成片变成与口播无关的装饰画面。

| 制作路径 | 应交付的东西 | 质量判断的对象 |
| --- | --- | --- |
| 真人拍摄 | 可执行脚本、动作、场景、镜头、收声、素材缺口与剪辑指令 | 人能否完成，实际素材是否兑现意图 |
| AI 情景制作 | 参考资产、镜头意图、连续性依赖、候选片段、声音、时间线和成片 | 实际画面、声音、连贯性与表达效果 |
| 混合制作 | 真实商品/现场与生成素材的边界、合成与剪辑关系 | 真实性、一致性及信息是否被误导 |

交付层要区分“生成成功”“文件已到本地”“草稿质量”“用户可使用”和“实际已发布”。图片或视频供应商返回成功不等于成片可用。单个镜头需要重做时，应复用未变化的参考、声音与其他片段；改变脚本后，只重做受影响的部分。V4 的文件身份与 QA 绑定合同可作为这一层的回归资产。[V4 成片封存](</Users/yangyucheng/Documents/第四版营销系统/backend/packages/harness/deerflow/personal_ip/final_artifacts.py:499>)

ViMax 为分镜、参考图和跨镜头依赖提供了可研究实现，其论文有 35 个视频案例的偏好评价。它没有验证品牌真实主张、IP 增长或本项目目标供应商。本次源码还发现，图像评审返回非法索引时会退到候选 0；这种运行兜底不能被解释为质量已通过。[Huang 等，ViMax，2026-06-02](https://arxiv.org/html/2606.07649v1)、[候选选择源码](https://github.com/HKUDS/ViMax/blob/05a48943878312d88fe5a016c12a9654940ecc43/agents/best_image_selector.py#L141)

生产上可借它的参考关联和局部制作思路，保留 Codex 调度与第七版媒体合同。FFmpeg 继续负责确定性后期；OpenTimelineIO 可在需要编辑器交换时评估，它表达剪辑信息和媒体引用，本身不渲染影片，也不负责完整业务谱系。[OTIO 官方文档](https://opentimelineio.readthedocs.io/en/latest/index.html)、[FFmpeg 官方许可说明](https://ffmpeg.org/legal.html)

创意质量还需要实际观看和听感。技术探针可查时长、解码、音轨和字幕边界；视觉模型可辅助找错；负责人和目标受众评价是否想看、是否兑现、是否值得信任。它们解决不同问题，不应合成一个掩盖事实错误的平均分。

Doshi、Hauser 的 293 篇八句英文故事实验同时观察到个体创意评价提高和作品相似性上升。因此，长期 IP 还应检查题材、关系、证据和表达是否重复；该研究不能推成专业短视频或商业转化增益。[Science Advances，2024-07-12](https://pmc.ncbi.nlm.nih.gov/articles/PMC11244532/)

## 10 运营与学习：让下一轮确实利用上一轮的结果

第一步是记录实际发生了什么。锁版时保存 Prediction，记录目标事件、观察窗口、比较基线、预期变化与失败信号。发布后登记实际标题、正文、成片和时间；用户改过内容，就依据实际版本复盘。数据回收保持首版规定的人工链接、表单或截图入口，不新增自动登录发布承诺。[第七版发布与学习规格](</Users/yangyucheng/Documents/ChatGPT/第七版营销系统/docs/superpowers/specs/2026-08-24-ip-agent-saas-design.md:498>)

播放、完播、互动、收藏、主页访问、有效线索与后续行动各有含义。评论能揭示受众如何理解内容，但评论者不代表全部受众；互动也不自动等于信任。应优先确认原始观察、曝光条件和行动质量，再形成解释。

一个可执行的复盘至少回答：原先想验证什么；实际发生什么；哪个差异有证据；还有哪些解释；下一次调整什么，哪些不变。这里的学习可以很小：修改一个镜头的表达、换一个可验证的选题角度、补充某种现场材料。无需每次都升级整个账号定位。

| 信息 | 可以如何使用 | 不能直接得出的结论 |
| --- | --- | --- |
| 用户喜欢一版稿件 | 更新该项目的表达偏好 | 该稿会爆或所有客户都喜欢 |
| 某类内容多次留存较好 | 形成带条件的局部候选规律 | 内容形式单独造成全部提升 |
| 高播放但合适线索很少 | 检查受众、承诺、承接和线索定义 | 单凭播放宣布业务成功 |
| 一次上线后数据上涨 | 保存观察并继续检验 | 已证明 Agent 的增量效果 |
| 新材料推翻旧判断 | 追加修正并使旧经验降级 | 删除原始预测来制造正确记录 |

人类预测研究说明，有可结算事件、持续反馈和概率训练，才能检验校准与辨别力。可借鉴其预测记录方法，但不能把地缘政治预测结果外推为内容运营。[Mellers 等，Psychological Science，2014](https://faculty.wharton.upenn.edu/wp-content/uploads/2015/07/2014---psychological-strategies-for-winning-a-tournament.pdf)

真实业务归因更难。Gordon 等在 Facebook 随机广告实验中发现，观察性方法常无法复原随机实验效果；2019 年正式论文使用 15 个实验，不能与其 2016 年白皮书的 12 个实验混用。这里借其识别偏差的结论，不用展示广告样本规模规定自然流量 IP 的试验规模。[Marketing Science，2019](https://pubsonline.informs.org/doi/10.1287/mksc.2018.1135)

持续学习分为两件工作。项目内，更新带条件、有反例、可失效的 MemoryRule，下一次取回相关少量条目。产品研发中，只有同类失败反复出现，才提出默认 Skill 修改并用新案例比较。前者不自动改系统提示词，后者不自动取得客户私有经验。

早期离线优化就是一次有边界的修改实验。不要先建自动训练、评委管理和经验晋级平台。判断标准稳定后，再考虑 GEPA 等优化器。

## 11 开源取舍：提取机制，保留一个运行底座

以下为 2026 年 9 月 5 日可见源码快照，采用静态阅读，未安装或运行。提交不等于论文实验的精确版本，源码许可证也不代替模型、素材或供应商条款。

| 项目与快照 | 可提取能力 | 本项目建议 |
| --- | --- | --- |
| STORM / Co-STORM；fb951af7744d；MIT | 视角驱动调查、追问、未用证据发现、引用结构 | 优先试验研究方法；不移植整套对话与报告运行时 |
| ACE；82709de050e1；Apache-2.0 | 带 ID 的增量经验、反馈关联 | 借条目思想；不直接当长期项目记忆系统 |
| GEPA；0632cdb5dcc0；MIT | 根据外部评估与反馈优化文字组件 | 有可靠反馈后用于离线、小范围优化 |
| ViMax；05a489438783；MIT | 分镜、参考图选择、跨镜头关系 | 按需提取到媒体能力；不接第二个总控 |
| OpenTimelineIO；bc5fe2d78dc3；Apache-2.0 | 剪辑时间线与媒体引用交换 | 需要编辑器互通时再引入，不阻塞初始成片 |

来源：[STORM 固定快照](https://github.com/stanford-oval/storm/tree/fb951af7744dab086e34962e9bc6fe878e145f83)、[ACE 固定快照](https://github.com/ace-agent/ace/tree/82709de050e1db6e6ef2f07bcb0393560b94992a)、[GEPA 固定快照](https://github.com/gepa-ai/gepa/tree/0632cdb5dcc052e690eab439e1b4a7e3e9cfe407)、[ViMax 固定快照](https://github.com/HKUDS/ViMax/tree/05a48943878312d88fe5a016c12a9654940ecc43)、[OTIO 固定快照](https://github.com/AcademySoftwareFoundation/OpenTimelineIO/tree/bc5fe2d78dc3f8b2a8feb7e04483d85a12e80072)。

ACE 的限制需要具体看源码。当前默认生成路径把整本 playbook 拼入提示，条目更新函数只实现 ADD，UPDATE、MERGE、DELETE 仍是 TODO；另有语义合并器，但默认关闭。因此，本项目要自行保证项目隔离、版本、撤销、过期和有界读取，不能宣称“装了 ACE 就有成熟记忆”。其论文的正面结果来自 AppWorld 和财务类基准，依赖可用反馈，不等于能从营销噪声中自动学对。[ACE 论文](https://arxiv.org/abs/2510.04618)、[条目操作](https://github.com/ace-agent/ace/blob/82709de050e1db6e6ef2f07bcb0393560b94992a/playbook_utils.py#L96)、[生成器](https://github.com/ace-agent/ace/blob/82709de050e1db6e6ef2f07bcb0393560b94992a/ace/core/generator.py#L59)

GEPA 的适配接口可让 Codex 正常执行，由外部提供评估、产物与反馈；不需要把生产系统换成 DSPy。风险是默认未给 valset 时会复用训练集，官方也说明可能复制训练词语或过拟合评分。建议一次只优化一个可辨识的文字组件，明确分开训练、验证和最终测试。其六项任务结果不证明中文 IP 运营增益。[GEPA 论文，ICLR 2026](https://arxiv.org/abs/2507.19457)、[适配接口](https://github.com/gepa-ai/gepa/blob/0632cdb5dcc052e690eab439e1b4a7e3e9cfe407/src/gepa/core/adapter.py#L83)、[验证集默认](https://github.com/gepa-ai/gepa/blob/0632cdb5dcc052e690eab439e1b4a7e3e9cfe407/src/gepa/api.py#L231)

没有足够证据支持把“多 Agent”“自我反思”单独当成解决方案。自我纠错研究的正反结果随任务和验证方法而变，不能用旧模型结论断言当前模型永远做不到，也不能假定多问几遍就会更好。[Huang 等，ICLR 2024](https://arxiv.org/abs/2310.01798)、[Wu 等，EMNLP 2024](https://arxiv.org/abs/2405.14092)

## 12 怎样知道它在变好：把检验嵌入真实工作

研究方法需要各司其职。历史 Git 能定位变化和已知失败；受控比较能判断某种处理是否改善当前任务；人员评价能判断内容是否有用；真实发布能提供受众和行动反馈。任何一种都不能单独证明北极星已经实现。

| 方法 | 能回答的问题 | 不能替代什么 |
| --- | --- | --- |
| Git、台账与原始产物对照 | 当时改了什么，哪些信息在链路中丢失 | 不能从代码存在推定模型效果 |
| 同材料、同模型的局部对照 | 一项 Skill 或上下文处理是否改善结果 | 不能证明市场增量或所有行业泛化 |
| 业务负责人示范与修订 | 哪些取舍有价值，什么修改才可用 | 其个人偏好不等于受众总体偏好 |
| 匿名内容与成片比较 | 理解、创作、可执行性和交付质量差异 | 主观评分不等于业务结果 |
| 前瞻发布与数据回收 | 真实接受度、行动、生产成本和长期变化 | 非随机观察不能自动给出因果结论 |

诊断失败时，可用“给定正确输入”的局部实验，而不是一直重写系统。先给模型同一份经过整理的真实材料，看能否作出有用选择；能做到再比较其自主研究。给定已接受选题，检查写作；给定脚本与参考资产，检查制作；给定实际数据，检查下一步建议。哪里在补入信息后明显恢复，哪里就值得优先修复。这里的人工辅助结果是诊断条件，不能算 Agent 独立完成。

如充分材料下仍然普遍平庸，再比较目标路线与另一个具备资格的强模型，控制工具、输入和预算，确认模型能力与交互适配问题。不能从更换模型的旧实验就断言所有新模型无用；也不应把某次更强输出直接写成永久默认路由。

每条研发切片同时保留两个验收：运行验收和业务验收。比如中断后能打开稿件，是运行验收；新事实出现后能适当调整且无需结构性返工，是业务验收。二者都从第一轮开始记录，不能等“全闭环都做完”才问内容值不值得用。

至少覆盖：跨主体、跨受众行动、跨资料条件、跨生产条件，以及同一项目跨时间的变化。评阅中分别记录严重事实问题、主观胜负与弃权、实际修改量、操作时间、模型费用和制作费用。不要把这些全部压成一个不可解释总分。

第七版已有 70% 配对胜率与封闭 Beta 目标；它们是验收要求，当前尚未证明达到。六周、至少四个客户项目加自营销、合计不少于六十条发布，也不自动等于完成严格因果识别。小规模研发诊断只指导下一次修改，不授予 G2 或客户发布资格。[现役质量与 Beta 标准](</Users/yangyucheng/Documents/ChatGPT/第七版营销系统/docs/superpowers/specs/2026-08-24-ip-agent-saas-design.md:765>)

## 13 接到现有 Codex：近期应该补什么

以下是实现建议，不是当前代码已经存在的 API。保持一个 Business Server、一个 Codex 执行底座及现有业务对象，优先把缺失行为放在所属模块；不要把具体营销流程继续塞进 codex-core。

| 所属位置 | 近期最小补充 | 直接检验的行为 |
| --- | --- | --- |
| ai-ip-domain | 从一次 ContentPackage 向有效项目版本、决定与产物引用逐步扩展；沿用既有状态 | 已确认约束不被假设覆盖，关键内容依据不丢失 |
| ai-ip-runtime | 当前任务装配有效版本、相关材料、待办与预算；允许提交局部产物再继续 | 不必重访谈，能跳步、回退、中断续做 |
| Skills 与工具目录 | 从旧版挑选有对应失败证据的小能力，参考资料按需读取 | 方法在该用时被使用，不强迫每次执行 |
| 研究与 Evidence Store | 保存实际来源片段、对象、时间和冲突，贯穿到内容版本 | 找到与读懂证据，并能解释其对选择的影响 |
| ai-ip-media 与能力网关 | 参考资产、镜头依赖、供应商任务与本地文件身份相连 | 局部重做、实际成片可用、不重复计费 |
| Publication / Prediction / MemoryRule | 实际版本登记、到期回收、观察与解释分开、相关经验读取 | 下一轮确实使用新结果，错误经验可降级 |
| 研发评测 | 复用现有记录与匿名比较，加入局部诊断条件 | 变化解决了哪种问题，是否损伤其他任务 |

已读 Codex App Server 支持 thread/start、thread/resume、turn/start、Skill 与 MCP 接口；无需为这些能力另造运行时。业务 Project 的长期权威不能直接等同于上游通用工作区 Project。正式协议扩展仍走 v2，状态管理按原规格使用独立业务数据库及产品配置根。[本地 App Server 文档](</Users/yangyucheng/Documents/ChatGPT/第七版营销系统/codex-rs/app-server/README.md:296>)、[现役仓内改造边界](</Users/yangyucheng/Documents/ChatGPT/第七版营销系统/docs/superpowers/specs/2026-08-24-ip-agent-saas-design.md:204>)

上下文扩展应按现有约束定义类型、有界装配和按需读取；不能直接照搬 ACE 默认整本注入。能力网关、费用回执、权限与不可逆动作继续由代码负责，Lead 负责语义判断；普通低质量草稿仍可保存查看，不能把所有质量意见升级成阻止创作的硬闸门。

## 14 建设顺序：每个切片都朝北极星交付

建议用以下顺序安排近期研发。具体时长和费用需由目标模型、素材、操作者与制作条件确定，本次研究没有依据承诺“几天做完”或虚构精确预算。

**切片一：一次值得继续的真实交付。** 从已有授权且材料、操作者和使用场景齐备的项目选 Mission。业务负责人负责现实信息与质量判断；Lead 完成研究、取舍和任务要求的成品或生产包。复用原生 Codex、当前材料读取和产物保存，只补眼前妨碍内容价值的缺口。运行验收是实际产物可访问，业务验收是是否愿意使用、返工在哪里。若整理材料后的原生基线已足够，就不加新的能力组件。

**切片二：在新信息下继续同一个 IP。** 让操作者带来真实的新材料或修改，安排一次中断续做。复用文件与版本能力，检查是否记得有效目标、是否更新受影响内容、是否保留未变化部分。停止条件是没有新增质量收益却增加用户补充与维护负担；发生具体丢失再补运行时。不能先把正式长期记忆平台全部建完。

**切片三：完成实际媒体与一次结果回收。** 由明确的操作者提供或取得拍摄/制作条件并人工发布。可先用已有媒体工具或人工协作完成研发样本，分别注明哪一部分由 Agent 完成；不把人工补完算自动成片。保存实际发布版本、到期数据和下一轮修改，检验从“交付”到“继续运营”是否发生。客户产品的媒体、网关、加密与权限仍按正式 gate 验收。

**切片四：在结构不同的项目中验证迁移。** 沿用同一核心与能力版本，更换主体、行动、资料和资源条件，加入新的未见项目。研发可以逐个完成深样本，不将产品范围收窄成一个行业。若新项目必须不断添加行业特判，先检查旧经验的适用范围、材料和检索，不立即重写整个 Agent。

**切片五：把重复有效的改进变成产品能力。** 观察到稳定的工作量与质量收益后，再完成对应正式状态、恢复、媒体和学习能力，扩大前瞻运营观察。此时才考虑自动离线优化。若低成本核心已经做到相当质量，保留简单实现；复杂模块必须持续说明自己的收益。

这与现有计划有需要明确处理的先后关系。Plan 01 负责最小业务证明，Plans 02、03 负责业务内核和内容能力，Plans 09、10 负责媒体与发布学习；总路线要求 Plan 03 通过后才启动 Plans 04 至 15。**若采用本报告建议，提前做切片二、三的研发验证，必须把试验范围和材料、费用、调用边界明确写入现役 child 的调整，不能默默视为正式 Plans 05、09、10 已获准开工。** 现有 G2、客户发布、财务和供应商资格没有被本报告变更。[总路线与依赖](</Users/yangyucheng/Documents/ChatGPT/第七版营销系统/docs/superpowers/plans/2026-08-25-00-codex-ai-ip-master-roadmap.md:65>)

其中可以立刻复用的研发习惯，是每次只留下六样证据：真实任务与材料、当时有效状态、系统实际输出、使用者修改或评价、花费与耗时、保留或删除决定。已有封存记录可以承载这些内容，不必为“六样证据”另建一套平台。

## 15 最终取舍与研究边界

要实现用户期待的效果，系统需要把通用模型的能力放进真实运营工作：有现实材料可看，有当前有效决定可继承，有专业制作工具可执行，有具体反馈可纠正。它对用户的价值应体现在逐渐减少用户亲自研究、选题、编排、制作和重复解释的工作，同时提高内容可用性和结果概率。拥有更多文件、更多 Agent 或更长分析本身都不构成价值。

因此，保留第七版北极星与 Codex 底座，停止把架构命名当作下一次突破；用真实 IP 的交付与继续运营来带动能力实现。从旧版搬来经过核查的资产和反例，从开源方案抽取局部方法，再通过不同主体和条件下的工作检验它。这个方向兼顾跨领域目标与现实的建设顺序。

本研究的高置信结论是：现役规格已经有完整产品方向；旧版存在可继承的正例、接口与错误材料；开源方法可提供局部机制，不能直接证明北极星；第七版当前可见实现与证据尚不能证明完整持续运营效果。中等置信建议是上述实现优先序及有条件经验机制。尚未解决的实证问题包括目标模型在充分材料下的质量上限、自动研究的稳定性、各媒体路线的实际成本、跨项目迁移及长期业务增量。

范围说明：本次核查历史台账和关键 Git 差异，并非逐行读完所有旧代码或复跑全部历史模型实验；早期素材回执没有重新观看。外部研究以原始论文、作者材料和官方源码为主；2026 年新预印本尚不能当独立复现结论。源码仅静态检查，未验证在本机、目标模型或实际供应商账户中可运行。没有确认新的真实运营账号、素材授权和业务数据，不能报告新的市场效果。

停止继续检索的理由是：北极星各主要环节已获得本地依据、相关外部方法或明确的未证实边界。进一步选择路线最需要新增真实项目证据，继续搜索另一套 Agent 框架预计不能替代它。

报告依据的完整来源记录和检索说明保留在本地研究目录。PDF 交付前执行文字、引用目标与分页检查，并对关键及高风险页面作抽样视觉检查；抽样不等于逐页完整视觉审阅。


## 16 文献与源码来源注记

下列记录供核对原始研究对象、发表时间和源码快照。网络材料访问于 2026 年 9 月 5 日。报告不把相关论文的效果量外推为本产品的实际收益；本地历史依据及提交位于第 03、04 节。

[Effective harnesses for long-running agents](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents)。Justin Young / Anthropic；2025-11-26。完整官方文章；软件开发演示，不是IP研究。

[LongMemEval: Benchmarking Chat Assistants on Long-Term Interactive Memory](https://arxiv.org/abs/2410.10813)。Di Wu, Hongwei Wang, Wenhao Yu, Yuwei Zhang, Kai-Wei Chang, Dong Yu；2024-10-14；v2 2025-03-04。原论文HTML和摘要；ICLR2025；500道记忆问答。

[Assisting in Writing Wikipedia-like Articles From Scratch with Large Language Models](https://aclanthology.org/2024.naacl-long.347/)。Yijia Shao et al. / NAACL；2024。原论文全文；100篇FreshWiki，10位编辑；方法与边界已核查。

[Into the Unknown Unknowns: Engaged Human Learning through Participation in Language Model Agent Conversations](https://arxiv.org/abs/2408.15232)。Yucheng Jiang, Yijia Shao, Dekun Ma, Sina J. Semnani, Monica S. Lam；2024；v2 2024-10-17。原论文全文；20名招募，19份有效评估，不是营销评价。

[Analogical encoding facilitates knowledge transfer in negotiation](https://groups.psych.northwestern.edu/gentner/papers/LoewensteinThompsonGentner99.pdf)。Jeffrey Loewenstein, Leigh Thompson, Dedre Gentner；1999-12。作者PDF全文；已核对 p.591 的不显著收益反证；未把人类效果外推LLM。

[Learning and Transfer: A General Role for Analogical Encoding](https://groups.psych.northwestern.edu/gentner/papers/GentnerLoewensteinThompson03.pdf)。Dedre Gentner, Jeffrey Loewenstein, Leigh Thompson；2003。已读取作者 PDF 全文，实验2与3；人类谈判迁移。

[Break It Down, Pass It On: Cross-Task Skill Transfer in LLM Agents](https://arxiv.org/html/2608.20274v1)。Yiyang Feng, Biddut Sarker Bijoy, Niranjan Balasubramanian, Jiawei Zhou；2026-08-20。预印本；全文方法、模型、数据及局限核查；英文任务。

[Demystifying Agent Skills: Why They Work-Until They Don't](https://arxiv.org/abs/2608.14036)。Zhiyuan Jiang et al.；2026-08-14。预印本；HTML方法及摘要，未复现研究。

[ViMax: Agentic Video Generation](https://arxiv.org/html/2606.07649v1)。Lingxuan Huang, Sizhe He, Hengji Zhou, Liqiang Nie, Lianghao Xia, Chao Huang；2026-06-02。原论文全文；35个视频样本；不等同营销与目标供应商效果。

[OpenTimelineIO Documentation](https://opentimelineio.readthedocs.io/en/latest/index.html)。Academy Software Foundation / OpenTimelineIO contributors；访问2026-09-05；页面未给单一发布日期。官方文档；时间线交换，非渲染器。

[FFmpeg License and Legal Considerations](https://ffmpeg.org/legal.html)。FFmpeg project；访问2026-09-05；页面未给单一发布日期。官方许可页面；具体发行二进制仍需按启用组件核查。

[Generative AI enhances individual creativity but reduces the collective diversity of novel content](https://pmc.ncbi.nlm.nih.gov/articles/PMC11244532/)。Anil R. Doshi, Oliver P. Hauser / Science Advances；2024-07-12。原文全文；293个作者，600个评阅者；8句英文故事，非专业IP运营。

[Psychological Strategies for Winning a Geopolitical Forecasting Tournament](https://faculty.wharton.upenn.edu/wp-content/uploads/2015/07/2014---psychological-strategies-for-winning-a-tournament.pdf)。Barbara Mellers et al. / Psychological Science；2014-03-21。作者全文PDF；训练与校准；人类地缘政治预测不是内容干预。

[A Comparison of Approaches to Advertising Measurement: Evidence from Big Field Experiments at Facebook](https://pubsonline.informs.org/doi/10.1287/mksc.2018.1135)。Brett R. Gordon, Florian Zettelmeyer, Neha Bhargava, Dan Chapsky / Marketing Science；2019-04-04 online；38(2):193-225。正式期刊摘要；15实验500百万用户实验观察；与2016白皮书12实验口径区分。

[Agentic Context Engineering: Evolving Contexts for Self-Improving Language Models](https://arxiv.org/abs/2510.04618)。Qizheng Zhang et al.；2025-10-06；v3 2026-03-29。已核对 v3 摘要及 v2 方法与限制；官方源码独立核验，未声称精确复现实验。

[GEPA: Reflective Prompt Evolution Can Outperform Reinforcement Learning](https://arxiv.org/abs/2507.19457)。Lakshya A Agrawal et al.；2025-07-25；v2 2026-02-14。ICLR2026；论文和官方代码；结果非中文营销。

[Large Language Models Cannot Self-Correct Reasoning Yet](https://arxiv.org/abs/2310.01798)。Jie Huang et al.；2023-10-03；ICLR2024。摘要及原始研究记录；只作限定任务反证，非2026模型能力上限。

[Large Language Models Can Self-Correct with Key Condition Verification](https://arxiv.org/abs/2405.14092)。Zhenyu Wu, Qingkai Zeng, Zhihan Zhang, Zhaoxuan Tan, Chao Shen, Meng Jiang；2024-05-23；v3 2024-10-03。EMNLP2024；摘要原论文记录；任务与验证条件不同。

[stanford-oval/storm 官方仓库](https://github.com/stanford-oval/storm/tree/fb951af7744dab086e34962e9bc6fe878e145f83)。维护者：stanford-oval/storm 项目团队；快照 2025-09-30；commit `fb951af7744dab086e34962e9bc6fe878e145f83`；许可 MIT。仅静态阅读关键实现与许可证，未安装运行。

[ace-agent/ace 官方仓库](https://github.com/ace-agent/ace/tree/82709de050e1db6e6ef2f07bcb0393560b94992a)。维护者：ace-agent/ace 项目团队；快照 2026-08-24；commit `82709de050e1db6e6ef2f07bcb0393560b94992a`；许可 Apache-2.0。仅静态阅读关键实现与许可证，未安装运行。

[gepa-ai/gepa 官方仓库](https://github.com/gepa-ai/gepa/tree/0632cdb5dcc052e690eab439e1b4a7e3e9cfe407)。维护者：gepa-ai/gepa 项目团队；快照 2026-09-01 UTC；commit `0632cdb5dcc052e690eab439e1b4a7e3e9cfe407`；许可 MIT。仅静态阅读关键实现与许可证，未安装运行。

[HKUDS/ViMax 官方仓库](https://github.com/HKUDS/ViMax/tree/05a48943878312d88fe5a016c12a9654940ecc43)。维护者：HKUDS/ViMax 项目团队；快照 2026-07-29；commit `05a48943878312d88fe5a016c12a9654940ecc43`；许可 MIT。仅静态阅读关键实现与许可证，未安装运行。

[AcademySoftwareFoundation/OpenTimelineIO 官方仓库](https://github.com/AcademySoftwareFoundation/OpenTimelineIO/tree/bc5fe2d78dc3f8b2a8feb7e04483d85a12e80072)。维护者：AcademySoftwareFoundation/OpenTimelineIO 项目团队；快照 2026-08-07；commit `bc5fe2d78dc3f8b2a8feb7e04483d85a12e80072`；许可 Apache-2.0。仅静态阅读关键实现与许可证，未安装运行。
