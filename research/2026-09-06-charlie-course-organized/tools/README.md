# 离线全课转录工具

`transcribe_course.py` 使用已验证的本地 SenseVoiceSmall int8 与 Silero VAD，读取下载验收清单中 92 个 MP4。没有网络调用、付费服务或新的运行时安装。FFmpeg 将音频流式转换成 16 kHz 单声道 PCM；每个 VAD 窗口输入后立即消费已完成语音段，不缓存整课音频或输出大 WAV。

从仓库根目录运行：

```sh
research/2026-09-06-course-to-capability/chinese-asr-probe/runtime/.venv/bin/python research/2026-09-06-charlie-course-organized/tools/transcribe_course.py --workers 6
```

只处理指定分 P 可添加 `--parts 2,3`。默认先处理 P2/3/4/7/8/40，接着 P41/5/6/9/10/1，然后将前半主课、后半主课与补充内容交错安排，支持并行阅读。每个 worker 的识别模型与 VAD 都只用一个 CPU 线程。

逐 P 输出为：

- `transcripts/pNNN.json`：完成标记、来源 CID/URL/标题/路径/SHA256、解码覆盖、VAD 设置、时间与用量、原始模型结果与分段文本。
- `transcripts/pNNN.source-time.txt`：机器稿说明、来源与源时间戳全文。
- `transcripts/pNNN.source-time.srt`：相同语音段的源时间轴字幕；质量说明保存在同行 JSON/TXT。
- `processing/pNNN.status.json`：当前处理状态，大约每 10 秒更新。
- `processing/pNNN.segments.jsonl`：每段识别完成后立即写入，仅用作处理中查看及排错。
- `processing/pNNN.ffmpeg.log`：音频解码警告或错误。
- `processing/batch-status.json`：该次批处理队列、结果、完成情况。

JSON/TXT/SRT 全部写好后才原子发布完成版 JSON。阅读者应以 `transcripts/pNNN.json` 的 `state == "completed"` 为准。再次运行会校验模型标识、源 SHA256 与 TXT/SRT 哈希并跳过已完成分 P；中断或报错分 P 会从头重跑，以免拼接造成遗漏或重复。不要同时运行两个重叠队列。

输入 SHA256 引用先前 `download-verification.json` 的验收值，识别启动时核对文件大小与纳秒修改时间。全音频解码覆盖与 VAD 识别覆盖分别记录：前者完整并不意味着每个声音都保留为文字。

检查已有结果：

```sh
research/2026-09-06-course-to-capability/chinese-asr-probe/runtime/.venv/bin/python research/2026-09-06-charlie-course-organized/tools/verify_transcripts.py
```

需要严格验收全 92P 时添加 `--require-all`。检查报告写到 `processing/transcript-verification.json`，覆盖文件一致性、时间范围与全音频解码元数据。该检查不评价听写准确性或作业是否找全。

所有结果都是机器转录，未人工听校；时间戳是 VAD 语音边界，不能等同于逐字精确对齐。专名、术语、引文、数字及作业细节应回到课程源片对应时间核对。禁止仅凭关键词扫描声称完成全套课程作业核查。
