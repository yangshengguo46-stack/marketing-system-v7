# W3 原生实跑断点

2026-09-06，原分支与未提交修改上使用现有 debug/codex-app-server，薄 stdio 客户端只转发公共 JSON-RPC，未模拟模型或工具。

1. attempt-1，GLM glm-5-2-260617：原生 turn 开始后，core/src/util.rs 的 debug 断言 `ReasoningSummaryPartAdded without active item` 中断模型工作；没有 marketing_work 成果。
2. attempt-2，doubao-seed-evolving：124.784 秒后返回 HTTP 429，turn failed，未生成成果；request id 在 events 中。没有自动反复付费重试。
3. 独立最小接口诊断（不是业务产出）：第一次对 summary 字段返回 400 unknown field；第二次仅要求 Reply OK，真实输出事件显示 reasoning 起始项没有 summary，message 起始项没有 content，而 done 项才补正文。第二次供应商报告 15 input + 69 output = 84 tokens。
4. 根因：SSE added 项被按完整 ResponseItem 反序列化，必填正文数组尚不存在，解析器静默丢弃 added；之后原生 turn 没有 active item 可接收 delta。需要仅对 added 的 reasoning.summary / message.content 缺省为空数组，不改 done、不覆盖已有数组、不伪造供应商输出。

首条失败调用没有完成 usage 回执；实际账单未知。HTTP 错误和结构诊断均不能计为营销业务通过。

## 回归记录

- RED：`just test -p codex-api --lib -E 'test(partial_item)' --retries 0`，nextest `bcac04d0-1944-4de7-8237-e1a5f30a57c7`；2 项中1通过1失败，失败为缺省正文的 added 项没有发出。
- 首轮 GREEN 范围：`just test -p codex-api --lib --retries 0 --test-threads 1`，nextest `dfb645bb-059f-416a-ad09-fc4ecb3d1a8e`；3 项新增用例通过，整体156/162，6项既有文件上传测试受 loopback502影响。
- 后续使用原台账中的测试进程专属 NO_PROXY 设置复跑；不修改全局代理。最终结果见 api-tests.log 与 app-server-tests.log。
