"""Path-neutral Promptfoo configuration and Task 4 launch compilation."""

import hashlib
import json
import os
import stat
import sys
from pathlib import Path

try:
    from . import batch_promptfoo_runner as _runner
    from .batch_launch_spec import CandidateLaunchSet, LaunchArtifact, LaunchSpec
    from .contracts import canonical_json_bytes
except ImportError:
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
_MAX_LAUNCH_EXECUTABLE_BYTES = 128 * 1024 * 1024


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


def render_promptfoo_config(request: object) -> dict[str, object]:
    """Render one shared config without embedding any attempt-local path."""
    route = _json_object(request.model_route_json, "model route")
    profile = _json_object(request.execution_profile_json, "execution profile")
    output_schema = _json_object(request.case_answer_schema_json, "output schema")
    model = _required_text(route.get("model"), "model")
    base_url = _required_text(route.get("baseUrl"), "base URL")
    reasoning = route.get("reasoningEffort", "medium")
    sandbox = profile.get("sandboxMode")
    approval = profile.get("approvalPolicy")
    timeout_seconds = profile.get("maxWallClockSeconds")
    if reasoning not in _REASONING_EFFORTS:
        raise PromptfooAdapterError("reasoning effort is unsupported by Promptfoo 0.122.0")
    if sandbox not in _SANDBOX_MODES:
        raise PromptfooAdapterError("sandbox mode is unsupported by Promptfoo 0.122.0")
    if approval not in _APPROVAL_POLICIES:
        raise PromptfooAdapterError("approval policy is unsupported by Promptfoo 0.122.0")
    if type(timeout_seconds) is not int or not 1 <= timeout_seconds <= 3600:
        raise PromptfooAdapterError("Promptfoo timeout must be between 1 and 3600 seconds")
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
            "TMPDIR",
        )
    }
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
                    "working_dir": ".",
                },
            }
        ],
        "tests": [{}],
    }


def _regular_file_sha256(path: Path) -> str:
    descriptor = -1
    try:
        before = path.lstat()
        if (
            not stat.S_ISREG(before.st_mode)
            or stat.S_ISLNK(before.st_mode)
            or before.st_size > _MAX_LAUNCH_EXECUTABLE_BYTES
        ):
            raise PromptfooAdapterError("runner Python must be a bounded regular file")
        descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
        opened = os.fstat(descriptor)
        if (opened.st_dev, opened.st_ino) != (before.st_dev, before.st_ino):
            raise PromptfooAdapterError("runner Python changed while opened")
        digest = hashlib.sha256()
        remaining = _MAX_LAUNCH_EXECUTABLE_BYTES + 1
        while remaining:
            chunk = os.read(descriptor, min(64 * 1024, remaining))
            if not chunk:
                break
            digest.update(chunk)
            remaining -= len(chunk)
        after = os.fstat(descriptor)
        if before.st_size > _MAX_LAUNCH_EXECUTABLE_BYTES or (
            opened.st_size,
            opened.st_mtime_ns,
        ) != (after.st_size, after.st_mtime_ns):
            raise PromptfooAdapterError("runner Python changed while measured")
        return digest.hexdigest()
    except OSError as error:
        raise PromptfooAdapterError("runner Python is unavailable") from error
    finally:
        if descriptor >= 0:
            os.close(descriptor)


def _validate_promptfoo_cli(path: object) -> Path:
    if not isinstance(path, Path) or not path.is_absolute():
        raise PromptfooAdapterError("Promptfoo CLI path must be an absolute Path")
    try:
        resolved = path.resolve(strict=True)
    except OSError as error:
        raise PromptfooAdapterError("Promptfoo CLI path is unavailable") from error
    package_root = None
    package = None
    for parent in (resolved.parent, *resolved.parents):
        manifest = parent / "package.json"
        if not manifest.is_file():
            continue
        try:
            candidate = json.loads(manifest.read_bytes())
        except (OSError, UnicodeDecodeError, json.JSONDecodeError):
            continue
        if type(candidate) is dict and candidate.get("name") == "promptfoo":
            package_root = parent
            package = candidate
            break
    if package_root is None or package is None:
        raise PromptfooAdapterError("Promptfoo CLI is not from a local promptfoo package")
    if package.get("version") != PROMPTFOO_VERSION:
        raise PromptfooAdapterError("Promptfoo CLI must be exactly 0.122.0")
    binary = package.get("bin")
    if type(binary) is not dict or type(binary.get("promptfoo")) is not str:
        raise PromptfooAdapterError("Promptfoo package has no declared CLI")
    if (package_root / binary["promptfoo"]).resolve(strict=True) != resolved:
        raise PromptfooAdapterError("Promptfoo CLI differs from its package declaration")
    return path


def _shared_requests(stock: object, modified: object) -> None:
    fields = (
        "execution_profile_json",
        "model_route_json",
        "app_server_protocol_schema",
        "promptfoo_config",
    )
    if any(getattr(stock, field) != getattr(modified, field) for field in fields):
        raise PromptfooAdapterError("stock and modified Promptfoo launch material differs")
    expected = canonical_json_bytes(render_promptfoo_config(stock))
    if stock.promptfoo_config != expected:
        raise PromptfooAdapterError("frozen Promptfoo config differs from static rendering")


def compile_promptfoo_launch_set(
    stock_request: object,
    modified_request: object,
    *,
    promptfoo_cli_path: Path,
    runner_python_path: Path | None = None,
) -> CandidateLaunchSet:
    """Compile sealed adapter bytes; Task 4 remains the only host process owner."""
    _shared_requests(stock_request, modified_request)
    cli = _validate_promptfoo_cli(promptfoo_cli_path)
    executable = (
        Path(sys.executable).resolve()
        if runner_python_path is None
        else runner_python_path
    )
    if not isinstance(executable, Path) or not executable.is_absolute():
        raise PromptfooAdapterError("runner Python path must be an absolute Path")
    executable_sha256 = _regular_file_sha256(executable)
    runner_path = Path(_runner.__file__).resolve()
    runner_payload = runner_path.read_bytes()
    artifact = LaunchArtifact(
        "promptfoo_runner.py",
        runner_payload,
        hashlib.sha256(runner_payload).hexdigest(),
    )

    def launch(request: object) -> LaunchSpec:
        return LaunchSpec(
            executable_path=executable,
            executable_sha256=executable_sha256,
            argv=("{artifact:promptfoo_runner.py}",),
            environment=(("AI_IP_PROMPTFOO_CLI", str(cli)),),
            artifacts=(artifact,),
            effective_config=request.effective_config,
            execution_profile_json=request.execution_profile_json,
            model_route_json=request.model_route_json,
            app_server_protocol_schema=request.app_server_protocol_schema,
            promptfoo_config=request.promptfoo_config,
        )

    return CandidateLaunchSet(launch(stock_request), launch(modified_request))
