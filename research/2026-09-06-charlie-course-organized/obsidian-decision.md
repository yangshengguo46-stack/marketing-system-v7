# 是否采用 Obsidian

核查日期：2026-09-06。结论：本次资料按兼容 Obsidian 的普通 Markdown 目录整理，推荐把它作为人的课程阅读与作业记录界面。暂不增加系统运行依赖。

## 官方事实和本项目判断

| 官方资料 | 能确认的功能 | 本项目的用法判断 |
| --- | --- | --- |
| [How Obsidian stores data](https://obsidian.md/help/data-storage) | Vault 就是本地文件夹，笔记是 Markdown，外部编辑变化会被刷新 | Codex 和人可以读写同一份课程文件，不需要把素材迁进另一个数据库。 |
| [Internal links](https://obsidian.md/help/links) | 支持 Markdown 链接、Wiki 链接与标题链接 | 课程→课堂案例→实际作业→自己的提交稿可以串起来。优先用普通 Markdown 链接，保持其他编辑器也能读。 |
| [Properties](https://obsidian.md/help/properties) | YAML 属性支持文本、列表、数字、日期等；不支持嵌套属性的常规编辑 | 每课只加课程号、类型、整理状态、来源等少量平面属性。详细证据留在正文和原始 JSON。 |
| [Bases syntax](https://obsidian.md/help/bases/syntax) | 表格视图读取笔记属性和文件信息，视图另存 `.base` | 未来需要筛选课程和作业时可试其原生视图；当前 Markdown 目录和作业表已足够，不先写插件。 |

这些是功能与实施判断，不能据此声称 Obsidian 会自动教会模型编剧。软件提供资料整理界面，讲解是否忠实、作业是否有用，仍取决于实际课程整理和练习。

## 三者各做什么

另实际读了 [kepano/obsidian-skills](https://github.com/kepano/obsidian-skills) 的 README、`obsidian-markdown/SKILL.md` 和 `obsidian-cli/SKILL.md`。它提供 Markdown、Bases、Canvas、CLI 等格式与操作指引，适合作为以后使用 Obsidian 的工具知识；其中没有替我们核读编剧课程、判断作业或保证作品质量的内容。本轮仅研究，不安装或执行其中命令。

[官方 CLI 文档](https://obsidian.md/help/cli)说明需要支持它的安装版本，并与运行中的 Obsidian 应用配合。当前只需生成课程文件，直接文件读写已经够用，不为这一步引入应用常驻或额外的自动化调用。

| 形式 | 保存什么 | 当前决定 |
| --- | --- | --- |
| 课程资料 / 知识资料 | 按原课编排的讲解、案例、时间点、作业及疑点 | 先完整整理，这是主工作。 |
| Obsidian | 阅读这些文件、通过链接跳转、记自己的理解和作业进度 | 目录保持兼容，可作为阅读器打开；不要求系统依赖它。 |
| Skill | 遇到某个明确创作问题时的步骤、要读取哪些课程材料、应交付什么 | 课程和练习整理后再提炼小范围流程；本轮不安装营销 Skill。 |

## 文件边界

课程资料单独放一份目录，避免把整个 Codex 仓库、构建缓存、密钥或原生任务状态当作 Vault 索引。原视频留在已有媒体目录，不为笔记再复制约 12.6 GB 的视频。课程页记录来源链接及时间点；内部课程、案例与作业链接位于资料目录内。

本机常见安装位置 `/Applications/Obsidian.app` 和 `~/Applications/Obsidian.app` 未发现 Obsidian。此项只核查了这两个位置，不等于穷尽全盘安装位置。本轮未安装应用、插件、同步服务或发布服务；资料链接与文件检查不等于在 Obsidian 界面中验收过。
