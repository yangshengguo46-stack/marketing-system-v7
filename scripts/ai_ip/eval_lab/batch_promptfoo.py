"""Path-neutral Promptfoo configuration and Task 4 launch compilation."""

import hashlib
import ipaddress
import json
from pathlib import Path
from urllib.parse import unquote, urlsplit

try:
    from . import batch_promptfoo_bundle as _bundle
    from . import batch_promptfoo_runner as _runner
    from .batch_launch_spec import CandidateLaunchSet, LaunchArtifact, LaunchSpec
    from .contracts import canonical_json_bytes
except ImportError:
    import batch_promptfoo_bundle as _bundle
    import batch_promptfoo_runner as _runner
    from batch_launch_spec import CandidateLaunchSet, LaunchArtifact, LaunchSpec
    from contracts import canonical_json_bytes


PROMPTFOO_VERSION = "0.122.0"
PromptfooAdapterError = _runner.PromptfooAdapterError
PromptfooResult = _runner.PromptfooResult
parse_promptfoo_result = _runner.parse_promptfoo_result

_REASONING_EFFORTS = {
    "none",
    "minimal",
    "low",
    "medium",
    "high",
    "xhigh",
    "max",
    "ultra",
}
_APPROVAL_POLICIES = {"never", "on-request", "on-failure", "untrusted"}
_SANDBOX_MODES = {"read-only", "workspace-write", "danger-full-access"}
_PUBLIC_DUMMY_BEARER = "ai-ip-public-loopback-dummy"
_PROMPTFOO_ENVIRONMENT = (
    ("PROMPTFOO_DISABLE_SHARING", "1"),
    ("PROMPTFOO_DISABLE_TELEMETRY", "1"),
    ("PROMPTFOO_DISABLE_UPDATE", "1"),
)


def _json_object(payload: object, label: str) -> dict[str, object]:
    if type(payload) is not bytes:
        raise PromptfooAdapterError(f"{label} must be frozen bytes")
    try:
        value = json.loads(payload)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise PromptfooAdapterError(f"{label} must contain JSON") from error
    if type(value) is not dict:
        raise PromptfooAdapterError(f"{label} must contain an object")
    return value


def _required_text(value: object, label: str) -> str:
    if type(value) is not str or not value:
        raise PromptfooAdapterError(f"{label} must be a non-empty string")
    return value


def _loopback_base_url(value: object) -> str:
    base_url = _required_text(value, "base URL")
    try:
        parsed = urlsplit(base_url)
        address = ipaddress.ip_address(parsed.hostname or "")
        port = parsed.port
    except (ValueError, UnicodeError) as error:
        raise PromptfooAdapterError(
            "base URL must be an HTTP loopback IP literal with an explicit port"
        ) from error
    decoded_path = unquote(parsed.path)
    if (
        parsed.scheme != "http"
        or not address.is_loopback
        or port is None
        or parsed.username is not None
        or parsed.password is not None
        or parsed.query
        or parsed.fragment
        or not parsed.path.startswith("/")
        or parsed.path.startswith("//")
        or "\\" in decoded_path
        or any(part in (".", "..") for part in decoded_path.split("/"))
        or any(ord(character) < 0x20 for character in decoded_path)
    ):
        raise PromptfooAdapterError(
            "base URL must be an HTTP loopback IP literal with an explicit port and safe path"
        )
    return base_url


def render_promptfoo_config(request: object) -> dict[str, object]:
    """Render one shared config without embedding any attempt-local path."""
    route = _json_object(request.model_route_json, "model route")
    profile = _json_object(request.execution_profile_json, "execution profile")
    output_schema = _json_object(request.case_answer_schema_json, "output schema")
    model = _required_text(route.get("model"), "model")
    base_url = _loopback_base_url(route.get("baseUrl"))
    reasoning = route.get("reasoningEffort", "medium")
    sandbox = profile.get("sandboxMode")
    approval = profile.get("approvalPolicy")
    timeout_seconds = profile.get("maxWallClockSeconds")
    if reasoning not in _REASONING_EFFORTS:
        raise PromptfooAdapterError(
            "reasoning effort is unsupported by Promptfoo 0.122.0"
        )
    if sandbox not in _SANDBOX_MODES:
        raise PromptfooAdapterError("sandbox mode is unsupported by Promptfoo 0.122.0")
    if approval not in _APPROVAL_POLICIES:
        raise PromptfooAdapterError(
            "approval policy is unsupported by Promptfoo 0.122.0"
        )
    if type(timeout_seconds) is not int or not 1 <= timeout_seconds <= 3600:
        raise PromptfooAdapterError(
            "Promptfoo timeout must be between 1 and 3600 seconds"
        )
    timeout_ms = timeout_seconds * 1000
    cli_env = {
        name: "{{ env." + name + " }}"
        for name in (
            "AI_IP_CASE_PATH",
            "AI_IP_CONFIG_PATH",
            "AI_IP_PROFILE_PATH",
            "AI_IP_PROTOCOL_PATH",
            "AI_IP_ROUTE_PATH",
            "AI_IP_SCHEMA_PATH",
            "CODEX_HOME",
            "HOME",
        )
    }
    cli_env["TMPDIR"] = "{{ env.AI_IP_CANDIDATE_TMP }}"
    cli_env["OPENAI_API_KEY"] = _PUBLIC_DUMMY_BEARER
    return {
        "prompts": [
            "Read the sealed case at $AI_IP_CASE_PATH and return only JSON that "
            "conforms to the schema at $AI_IP_SCHEMA_PATH."
        ],
        "providers": [
            {
                "id": "openai:codex-app-server",
                "config": {
                    "approval_policy": approval,
                    "apiKey": _PUBLIC_DUMMY_BEARER,
                    "base_url": base_url,
                    "cli_env": cli_env,
                    "codex_path_override": "{{ env.AI_IP_CODEX_PATH }}",
                    "ephemeral": True,
                    "include_raw_events": True,
                    "inherit_process_env": False,
                    "model": model,
                    "model_provider": "openai",
                    "model_reasoning_effort": reasoning,
                    "output_schema": output_schema,
                    "request_timeout_ms": timeout_ms,
                    "reuse_server": False,
                    "sandbox_mode": sandbox,
                    "startup_timeout_ms": timeout_ms,
                    "turn_timeout_ms": timeout_ms,
                    "working_dir": "{{ env.AI_IP_WORKSPACE }}",
                },
            }
        ],
        "tests": [{"vars": {}}],
    }


def _shared_requests(stock: object, modified: object) -> None:
    fields = (
        "execution_profile_json",
        "model_route_json",
        "app_server_protocol_schema",
        "promptfoo_config",
    )
    if any(getattr(stock, field) != getattr(modified, field) for field in fields):
        raise PromptfooAdapterError(
            "stock and modified Promptfoo launch material differs"
        )
    expected = canonical_json_bytes(render_promptfoo_config(stock))
    if stock.promptfoo_config != expected:
        raise PromptfooAdapterError(
            "frozen Promptfoo config differs from static rendering"
        )


def compile_promptfoo_launch_set(
    stock_request: object,
    modified_request: object,
    *,
    promptfoo_runtime_root: Path,
    portable_node_path: Path,
) -> CandidateLaunchSet:
    """Compile sealed adapter bytes; Task 4 remains the only host process owner."""
    _shared_requests(stock_request, modified_request)
    try:
        runtime = _bundle.seal_committed_promptfoo_runtime(
            promptfoo_runtime_root, portable_node_path
        )
    except _bundle.PromptfooBundleError as error:
        raise PromptfooAdapterError(
            f"committed Promptfoo runtime is unavailable: {error}"
        ) from error
    source_artifacts = []
    for name in ("batch_promptfoo_runner.js", "batch_promptfoo_result.js"):
        try:
            payload = _bundle.read_bounded_regular(
                Path(__file__).with_name(name), _bundle.MAX_SOURCE_BYTES, name
            )
        except _bundle.PromptfooFilesystemError as error:
            raise PromptfooAdapterError(f"sealed runner source is unavailable: {error}") from error
        source_artifacts.append(
            LaunchArtifact(name, payload, hashlib.sha256(payload).hexdigest())
        )
    artifacts = (*source_artifacts, *runtime.artifacts)
    argv = (
        "{artifact:batch_promptfoo_runner.js}",
        "{artifact:batch_promptfoo_result.js}",
        *runtime.artifact_arguments,
    )

    def launch(request: object) -> LaunchSpec:
        return LaunchSpec(
            executable_path=runtime.node_path,
            executable_sha256=runtime.node_sha256,
            argv=argv,
            environment=_PROMPTFOO_ENVIRONMENT,
            artifacts=artifacts,
            effective_config=request.effective_config,
            execution_profile_json=request.execution_profile_json,
            model_route_json=request.model_route_json,
            app_server_protocol_schema=request.app_server_protocol_schema,
            promptfoo_config=request.promptfoo_config,
        )

    return CandidateLaunchSet(launch(stock_request), launch(modified_request))
