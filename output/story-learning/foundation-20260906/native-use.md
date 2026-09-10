# 在现有作业链中怎样使用这个工作包

一个 Lead 根据当前创作问题，给原生作业附上题目与相关的一份方法材料；把实际故事保存到原有研究、方向、正文作业中。只给文件路径后，要实际读取完整相关文件，工具的预览不是正文。

本轮已成功使用同一 App Server、glm-5-2-260617 和 marketing_work，保留了模型请求事件、材料读取、三阶段正文、作品附件。普通工作资料通过原生文件工具显式读取；当前没有安装自动调用的营销 Skill。

## 已实跑的调用方式

新建时 open 只传 `action`、`workId`、`brief`、`materials`；不要混入保存时使用的 `stage` 或 `body`。成功返回应有实际 `workFile`；错误说明不能解释成保存成功。已有作业重新打开只需 `action`、`workId`，局部返工按原工具支持的 `startAt` 处理。

保存时 record 只传 `action`、`workId`、`stage`、`body`。先确认作业创建成功，再保存真实研究、方向和完整正文。完整原稿和修订稿另存附件，方便比较。工具建议的通用视频包装不扩张本次只写故事的任务。

出现参数错误时先按返回修正原调用，不能用 Shell 模拟 marketing_work，也不能去遍历旧任务寻找可抄的工作文件。试用中确实出现过这一失败，见[执行记录](../../../research/2026-09-06-course-to-capability/trial-verification.json)和其中列出的排除记录。

本轮补清的是试用客户端的调用说明；产品工具仍是原实现，不能据此宣称所有模型都会稳定正确调用。后续若相同误用继续出现，再以实际错误为依据补最小工具接口说明或结构约束。

## 查看真实成果

- [完整对照结果记录](../../../research/2026-09-06-course-to-capability/trial-verification.json)
- [独立匿名故事评阅](../../../research/2026-09-06-course-to-capability/independent-story-review.md)
- [原生试用客户端](run/native_client.py)：仅启动既有 App Server 并交换 JSON-RPC；模型请求和工具执行仍归 App Server。密钥从已授权本机资料读取，不写入记录。

这是用于复现研究过程的客户端记录，不是新的产品运行时。不要直接反复执行旧试验标签覆盖已有记录；后续任务使用新的标签和 workId。
