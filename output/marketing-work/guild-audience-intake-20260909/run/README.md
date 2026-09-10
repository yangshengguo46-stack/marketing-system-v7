# SeedEvolving 原生试用入口

此目录只复用已有 stdio 传输脚本；模型请求和工具执行由本项目 `codex-rs/target/debug/codex-app-server` 负责。不是新的模型代理或运行时。每个 label 创建独立 CODEX_HOME；重复 label 会拒绝启动以免污染旧试验。

从项目根目录执行（需要已构建的原生二进制及已授权第六版 `.env` 中的 `VOLCENGINE_API_KEY`）：

```sh
python3 output/marketing-work/guild-audience-intake-20260909/run/native_client.py my-fresh-intake doubao-seed-evolving output/marketing-work/guild-audience-intake-20260909/candidate-method.txt
```

看到 READY 后，在同一终端输入一行 JSON：

```json
{"id":3,"method":"turn/start","params":{"threadId":"$thread","input":[{"type":"text","text":"我是做直播公会的，想做一个公会品牌账号，让合适的达人看了内容以后愿意咨询并加入我们。这个账号应该怎么起步？"}]}}
```

输入 `exit` 正常关闭。省略最后一个方法文件参数即为本轮通用组。完整原始事件在 label-events.ndjson，终端只显示截短进度。密钥通过进程环境传入，不写配置；不需要复制、粘贴或显示密钥。项目开发任务的模型配置保持原样。

此入口限定首轮接案：不读取旧案例、课程或文件，不联网；未连接正式业务前端。三项自动记忆开关关闭。当前已验证的模型选择符是 `doubao-seed-evolving`，不声称知道服务端隐藏权重版本。其他模型参数虽可传入，但需单独验证，不代表均已接通。
