# 查理的编剧课：整套课程采集记录

2026-09-06，用户明确要求先把整套课程取得，再进行清洗。当前已完成本次目录快照 **92/92** 分集的下载及文件检查，视频 **11.77 GiB（约 12.63 GB）**，目录总时长 **62:59:42**。

完整说明与文件入口见[材料目录](../../output/course-materials/charlie-screenwriting-20260906/README.md)。

- [catalog.json](catalog.json) / [catalog.csv](catalog.csv)：下载前取得的完整目录；不按标题排除后续新增内容。
- [bilibili-view-direct.json](bilibili-view-direct.json)：公开目录接口原始返回。
- [bilibili-view-final.json](bilibili-view-final.json) / [catalog-recheck.json](catalog-recheck.json)：下载后复查，分集列表未变化。
- [material-index.csv](material-index.csv)：全部材料的本地位置及检查状态。
- [download-verification.json](download-verification.json)：92 集音视频轨、时长、完整数据包读取及文件哈希检查，0 失败。
- [run-record.json](run-record.json)：开源工具版本、准确下载参数、网络路径差异、日志、来源与限制。
- [下载日志](run/full-download.log)：yutto 2.3.1，全量下载 exit 0。
- [画面抽查](run/quality-checks/part-005-at-600s.jpg)：第二课 10:00，可见板书及嵌入式中文字幕；不代表全课视听核验。

本轮成功解决此前目录与视频取得问题：默认 urllib 路径返回 HTTP 412，空 ProxyHandler 直连取得目录；yutto 对自身命令设置 `--proxy no` 完成全量下载。没有改动全局网络设置，没有读取或传输登录 Cookie，没有开发新的采集工具或运行时。

当前所有视频为 480p；独立字幕未取得，原视频内已有字幕保留。全课转录、清洗、板书识别与方法提炼尚未开始；材料齐备不是故事能力通过。
