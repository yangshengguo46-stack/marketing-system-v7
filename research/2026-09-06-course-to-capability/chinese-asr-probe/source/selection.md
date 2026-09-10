# 方案选择与证据

选用 **SenseVoiceSmall int8（2024-07-17）+ sherpa-onnx 1.13.7 + Silero VAD**。只实测这一种模型，不安装完整 PyTorch/FunASR 训练或导出环境。

1. [SenseVoice 官方仓库](https://github.com/QwenAudio/SenseVoice)的 README 将 sherpa-onnx 列为部署实现；本地留存 `sensevoice-repository-readme.md`。这说明所选运行方式有上游出处，不代表本课程准确率已被验证。
2. [sherpa-onnx 官方预训练模型文档](https://k2-fsa.github.io/sherpa/onnx/sense-voice/pretrained.html)列出 2024-07-17 int8 模型、普通话 `zh`、CPU 线程数、ITN 标点输出及 token timestamps。官方模型压缩包为 **163,002,883 bytes**；未选择同包包含约 894 MB float32 模型的完整版。
3. [官方 Python API 文档](https://k2-fsa.github.io/sherpa/onnx/sense-voice/python-api.html)与[固定 v1.13.7 的字幕示例](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.7/python-api-examples/generate-subtitles.py)展示 `OfflineRecognizer.from_sense_voice` 和 VAD 的分段起点。保存原文件，以便复查本地小脚本的 API 依据。VAD 段落时间来自采样位置，不靠语言模型猜测。
4. [PyPI sherpa-onnx 1.13.7](https://pypi.org/project/sherpa-onnx/1.13.7/)提供 CPython 3.14/macOS x86_64 wheel；其依赖为 `sherpa-onnx-core==1.13.7`。本机真实成功导入两者及 NumPy，无需编译或新 Python 下载。

限制与设置：

- 只取 p005 原视频 590–680 秒，16 kHz、单声道 PCM16，共 90 秒。来源 SHA-256 和原视频映射见 `../audio/clip-record.json`。
- ASR/VAD 各 1 线程，CPU，中文 `zh`，开启 ITN。顺序推理，配合主任务已有 CPU 工作，不并行跑第二样本。
- Silero threshold 0.5、min silence 0.5 s、min speech 0.25 s、max speech 20 s，采用运行时默认值并显式记录。官方通用字幕示例采用更短切段；这里保留较长语音上下文。
- VAD 起止是语音段边界，未宣称等同语义段落或精确字词边界；token timestamps 是 CTC 模型位置。原视频时间由片段相对时间加 590 秒计算。
- 9 张原视频帧由 agent 目视读取字幕；不是人工听写金标。字幕本身可能压缩口语、替换词汇或把词写成字母，例如 668 秒的“R体”。因此不计算整段 CER/WER，不把 OCR/ASR 置信度当人工准确率。
- 引用课程关键规则时，仍应点回原视频并核查术语、否定、范围条件。

下载全部来自公开官方仓库、官方 release 及 PyPI；记录下载 URL、字节数和 SHA-256。初始 GitHub 下载速度慢，按主任务建议仅给同一次模型续传增加 `curl --noproxy '*'`；未改全局代理，未换模型。细节见 `downloads.json`、`model-resume.json` 及日志。

模型权重和代码分开记录来源；模型包自带 LICENSE、README 一并保留。这里不对模型或课程的再分发权利作推断。
