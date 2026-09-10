"""One-case stdio client; App Server owns every model request and tool execution.
Input lines are JSON-RPC requests, with $thread replaced by the started thread id.
No response fixtures, tool emulation, provider proxy, or agent orchestration.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time
import termios

if sys.stdin.isatty():
    terminal_settings = termios.tcgetattr(sys.stdin)
    terminal_settings[3] &= ~(termios.ICANON | termios.ECHO)
    termios.tcsetattr(sys.stdin, termios.TCSANOW, terminal_settings)

root = Path(__file__).resolve().parents[4]
run = Path(__file__).resolve().parent
label = sys.argv[1]
model = sys.argv[2]
state = run / (label + "-state")
state.mkdir(exist_ok=False)
secret = None
for line in Path("/Users/yangyucheng/Documents/ChatGPT/第六版营销系统/.env").read_text().splitlines():
    if line.startswith("VOLCENGINE_API_KEY="):
        secret = line.split("=", 1)[1].strip().strip("\"'")
assert secret, "Authorized credential unavailable"
env = dict(os.environ)
env["GUILD_TRIAL_ARK_API_KEY"] = secret
env["CODEX_HOME"] = str(state)
# Restrict model-executed shell inheritance without changing host environment.
config = f'''model = "{model}"
model_provider = "story_ark"
approval_policy = "never"
sandbox_mode = "danger-full-access"
web_search = "disabled"
project_doc_max_bytes = 0
model_reasoning_effort = "medium"
model_reasoning_summary = "none"
[features]
code_mode = false
memories = false
[memories]
generate_memories = false
use_memories = false
[orchestrator.skills]
enabled = false
[orchestrator.mcp]
enabled = false
[shell_environment_policy]
inherit = "core"
exclude = ["*KEY*", "*TOKEN*", "*SECRET*"]
[model_providers.story_ark]
name = "Guild intake authorized Ark"
base_url = "https://ark.cn-beijing.volces.com/api/v3"
env_key = "GUILD_TRIAL_ARK_API_KEY"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
stream_idle_timeout_ms = 180000
supports_websockets = false
'''
(state / "config.toml").write_text(config)
log = (run / (label + "-events.ndjson")).open("a", buffering=1)
stderr = (run / (label + "-stderr.log")).open("a")
proc = subprocess.Popen([str(root / "codex-rs/target/debug/codex-app-server"), "-c", "features.memories=false", "-c", "memories.generate_memories=false", "-c", "memories.use_memories=false"], cwd=root, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True, bufsize=1)
thread_id = None
ready = threading.Event()
write_lock = threading.Lock()

def send(obj):
    data = json.dumps(obj, ensure_ascii=False)
    with write_lock:
        log.write(json.dumps({"at":time.time(), "direction":"client", "message":obj}, ensure_ascii=False) + "\n")
        proc.stdin.write(data + "\n")
        proc.stdin.flush()

def read():
    global thread_id
    for line in proc.stdout:
        safe = line.replace(secret, "[REDACTED]")
        try:
            obj = json.loads(safe)
        except ValueError:
            continue
        log.write(json.dumps({"at":time.time(), "direction":"server", "message":obj}, ensure_ascii=False) + "\n")
        if obj.get("id") == 1:
            send({"method":"initialized","params":{}})
            send({"id":2,"method":"thread/start","params":{"model":model,"cwd":str(root),"approvalPolicy":"never","sandbox":"danger-full-access","selectedCapabilityRoots":[],"experimentalRawEvents":True,"baseInstructions":"You are a capable assistant helping the user with an actual marketing assignment.","developerInstructions":'你正在接待一位提出营销需求的用户。直接用中文回应，提供你认为有用的专业判断。用户没有提供的企业、地区、服务、案例和结果都是未知事实。不要读取本地文件、其他任务、密钥或历史资料，不调用子代理、Skill或媒体工具，不修改文件。当前任务是第一轮需求接待；无需为回答而制造工具调用。用户另行要求保存时，可以使用原生 marketing_work；新建 open 只传 action、workId、brief、materials，record 只传 action、workId、stage、body。' + (Path(sys.argv[3]).read_text() if len(sys.argv)>3 else "")}})
        elif obj.get("id") == 2 and "result" in obj:
            thread_id = obj["result"]["thread"]["id"]
            (run / (label + "-thread.json")).write_text(json.dumps({"threadId":thread_id,"model":model},ensure_ascii=False))
            print("READY", thread_id, model, flush=True)
            ready.set()
        elif "error" in obj:
            print("RPC_ERROR", json.dumps(obj,ensure_ascii=False)[:1800], flush=True)
        elif obj.get("method") in ["error","turn/completed","thread/tokenUsage/updated"]:
            print(json.dumps(obj,ensure_ascii=False)[:2200],flush=True)
        elif obj.get("method") == "rawResponseItem/completed" and obj.get("params",{}).get("item",{}).get("type") in ["function_call", "function_call_output"]:
            item = obj["params"]["item"]
            print("RAW_TOOL", json.dumps(item,ensure_ascii=False)[:600],flush=True)
        elif obj.get("method") == "item/completed":
            item = obj.get("params",{}).get("item",{})
            if item.get("type") != "reasoning":
                print("ITEM", item.get("type"), json.dumps(item,ensure_ascii=False)[:850],flush=True)
        elif "id" in obj and "method" in obj:
            print("UNEXPECTED_SERVER_REQUEST", obj["method"],flush=True)
    print("SERVER_EXIT", proc.poll(), flush=True)

threading.Thread(target=read,daemon=True).start()
send({"id":1,"method":"initialize","params":{"clientInfo":{"name":"guild_intake_stdio","version":"0.1"},"capabilities":{"experimentalApi":True}}})
if not ready.wait(90):
    raise RuntimeError("Native thread did not initialize; inspect saved events")
for line in sys.stdin:
    if line.strip() == "exit":
        break
    if line.strip():
        send(json.loads(line.replace("$thread",thread_id)))
proc.stdin.close()
proc.wait(timeout=30)
