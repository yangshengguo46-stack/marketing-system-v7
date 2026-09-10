# 中文课程 ASR 单样本实测

2026-09-06，真实本地运行成功。**SenseVoiceSmall int8 + sherpa-onnx + Silero VAD 在这台 Intel Mac 上值得继续做少量课程验证，可作为带时间戳的机器初稿方案；这 90 秒不足以支持未经校对直接处理并采用全 92 集。**没有启动批量任务。

## 实测结果

输入为查理编剧课 **p005《第二课-故事的要素2.1》590–680 秒**，即原视频 09:50–11:20。16 kHz 单声道 PCM16，真实 90 秒，不是模拟音频。运行于 Intel i9-9980HK、32 GiB RAM、macOS x86_64；ASR、VAD 均设为 1 线程，依次执行。

| 指标 | 本次结果 |
| --- | --- |
| ASR + VAD 耗时，不含加载 | 13.563 秒 |
| 其中 VAD | 1.167 秒 |
| 模型初始化 | 4.069 秒 |
| 完整 Python 进程墙钟时间 | 21.96 秒 |
| 识别 RTF（耗时/音频长度） | 0.1507，约 6.64 倍于实时 |
| 峰值 RSS | 580,063,232 bytes，约 553 MiB |
| 输出 | 19 个语音段，原视频绝对时间 SRT、JSON、文本 |

完整进程耗时包括 Python/库导入、音频读取、初始化、识别和写盘；13.563 秒只衡量加载完成后的分段与识别。下载和解压不在推理计时内。主任务存在其他工作，因此不是独占机器基准。

本次的旧 Whisper 参考来自主任务已有 p005 全课运行，6 线程处理 1276.679 秒花 352.333 秒。两次输入长度、线程数和系统负载不同，不能据此声称公平的模型速度胜负。

## 术语与否定核查

从原视频提取 9 张帧，agent 目视读取其内嵌字幕，再核对保留的两份机器转写。**这不是人工听写金标，也没有计算人工准确率、CER 或 WER。**帧位置、字幕原文和图片保存在 [review/subtitle-reference.json](review/subtitle-reference.json)。

| 原字幕与位置 | 已有 Whisper base | 本次 SenseVoice |
| --- | --- | --- |
| 灵感，600 秒 | 零感 | 灵感 |
| 出道即巅峰，612 秒 | 出道即天风 | 出道及巅峰，仍有错字 |
| 天才有余，630 秒 | 天才有于 | 天才有余 |
| 技能不足，630 秒 | 技能不足 | 技能不足 |
| 共情，640 秒 | 共情 | 共情 |
| 达不到，640 秒 | 打不到 | 达不到 |
| 不断，644 秒 | 不断 | 不断 |
| 发自内心，648 秒 | 发自内心 | 发自内心 |
| 枯竭，668 秒 | 孤减 | 枯竭 |

“达不到”关键否定保留正确，但不能由一个否定例子推出全课不会漏否定。“出道及巅峰”仍需更正。635–637 秒输出“肉体肉体”，而字幕写“身体”；647–662 秒口语、自我纠正部分也有未判定差异。这些可能包含原说话重复或字幕改写，尚未逐句人工听辨，不能直接标成模型幻觉或正确转写。最后一个语音段因固定 90 秒边界而截断。详见 [review/comparison.json](review/comparison.json)。

## 时间戳与可用边界

- SRT 使用原视频时间，计算式为 VAD 片段采样位置 ÷ 16000 + 590 秒，能回到对应视频位置。
- VAD 产生语音段，不保证一段就是完整语义段落；本次最长段约 15.3 秒。JSON 同时保留未经改写的识别结果、tokens 和相对该语音段的 CTC 时间戳。
- 19 段的时间范围、顺序、token/timestamp 数量及 SRT 条目数已检查，见 [output/validation.json](output/validation.json)。没有用 token 位置假装人工强制对齐。
- 适合继续提取候选规则、制作可回查的课程初稿。正式采用关键规则时仍要复核术语、否定、适用范围；建议先增加几段不同课次和音质样本，再决定是否扩大处理。这里没有开始这一步。
- 仅把当前 RTF 线性乘以全部 62.98 小时原视频，会得到约 9.5 小时识别时间；这是粗略外推，不含下载、逐课初始化和校对，也不是运行承诺。

## 来源、下载与运行时

方案依据 [source/selection.md](source/selection.md)。官方代码、文档、版本、下载 URL、摘要均留存。

- 运行时：Python 3.14.3；`sherpa-onnx==1.13.7`、`sherpa-onnx-core==1.13.7`、`numpy==2.5.2`。隔离 venv 位于本目录，复用已有 uv；无 PyTorch、付费 API、新 Skill 或产品代码变更。
- 模型：官方 2024-07-17 SenseVoice int8 包 163,002,883 bytes；Silero VAD 643,854 bytes。模型、依赖及来源文件的有效下载总量低于 0.2 GB，远低于本次约 1 GB 上限。
- 初始 GitHub 单连接很慢；保留断点，先做本次命令的 `--noproxy '*'`，再对剩余部分用 4 个互不重叠的 HTTP Range 请求。没有改全局代理或更换模型。
- 各段长度、总长和**官方 release API 给出的 SHA-256**完全一致：`7d1efa2138a65b0b488df37f8b89e3d91a60676e416f515b952358d83dfd347e`。校验完成才解压、运行。
- 4 个大 GET 没有留 HTTP 响应头；后续每段起点的 32-byte GET 检查中，1 次取得 HTTP 206、正确 Content-Range 和与完整包一致的内容，3 次超时，原样保留。完整性结论依赖每段长度与官方全包摘要，不依赖超时检查。见 [source/model-verification.json](source/model-verification.json)、[source/model-ranges.json](source/model-ranges.json)、[source/range-get-checks.json](source/range-get-checks.json)。

## 主要交付与复现

- [output/sensevoice.txt](output/sensevoice.txt)：便于阅读的未经校正原始文本。
- [output/sensevoice.source-time.srt](output/sensevoice.source-time.srt)：原视频时间字幕。
- [output/sensevoice.json](output/sensevoice.json)：原始输出、语音段、token 时间与实测性能。
- [output/run.stdout.log](output/run.stdout.log)、[output/run.stderr.log](output/run.stderr.log)：真实运行日志和系统 `/usr/bin/time -l` 数据。
- [runtime/environment.json](runtime/environment.json)、[runtime/requirements.txt](runtime/requirements.txt)、[runtime/wheel-sources.json](runtime/wheel-sources.json)：本机环境与 wheel 来源。
- [audio/clip-record.json](audio/clip-record.json)：输入映射、ffmpeg 截取命令、音频和原视频摘要。
- [run_probe.py](run_probe.py)：固定单个 90 秒样本的运行脚本，包含长度断言，无批量入口。

从仓库根目录复跑，会覆盖本探针的输出；命令为：

```sh
research/2026-09-06-course-to-capability/chinese-asr-probe/runtime/.venv/bin/python research/2026-09-06-course-to-capability/chinese-asr-probe/run_probe.py
```

大模型、音频和隔离环境留在本地并由该目录的 `.gitignore` 排除；小型证据、帧和记录保留供复查。没有提交、合并或推送；没有触碰主任务的现有修改。
