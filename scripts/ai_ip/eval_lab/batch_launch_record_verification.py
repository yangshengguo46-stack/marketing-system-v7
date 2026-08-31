"""Offline structural verification for descriptor-bound launch records."""


class LaunchRecordVerificationError(ValueError):
    pass


def _digest(value: object) -> bool:
    return (
        type(value) is str
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
    )


def verify_bound_inputs(
    record: dict[str, object],
    artifacts: dict[str, object],
    environment: dict[str, object],
    launch_spec: dict[str, object],
) -> None:
    """Match every recorded argv/environment reference to one bound input."""
    bound = record.get("boundInputs")
    declared_artifacts = launch_spec.get("artifacts")
    declared_argv = launch_spec.get("argv")
    required = {
        "codex",
        "executable",
        "config",
        "profile",
        "route",
        "protocol",
        "promptfoo",
        "case",
        "schema",
    }
    if type(declared_artifacts) is dict:
        required.update(f"artifact:{name}" for name in declared_artifacts)
    if (
        type(bound) is not dict
        or type(declared_artifacts) is not dict
        or type(declared_argv) is not list
        or set(bound) != required
        or set(artifacts) != required - {"codex"}
    ):
        raise LaunchRecordVerificationError("bound inputs are invalid")
    for name, value in bound.items():
        if (
            type(value) is not dict
            or set(value) != {"childPath", "device", "inode", "mode", "sha256", "size"}
            or type(value["childPath"]) is not str
            or not value["childPath"]
            or any(
                type(value[field]) is not int
                for field in ("device", "inode", "mode", "size")
            )
            or value["size"] < 0
            or not _digest(value["sha256"])
            or (name != "codex" and artifacts.get(name) != value["sha256"])
        ):
            raise LaunchRecordVerificationError("bound input identity is invalid")
    if any(
        artifacts.get(f"artifact:{name}") != digest
        for name, digest in declared_artifacts.items()
    ):
        raise LaunchRecordVerificationError("artifact declaration differs")
    paths = {
        "AI_IP_CASE_PATH": "case",
        "AI_IP_CODEX_PATH": "codex",
        "AI_IP_CONFIG_PATH": "config",
        "AI_IP_PROFILE_PATH": "profile",
        "AI_IP_ROUTE_PATH": "route",
        "AI_IP_PROTOCOL_PATH": "protocol",
        "AI_IP_PROMPTFOO_PATH": "promptfoo",
        "AI_IP_SCHEMA_PATH": "schema",
    }
    if any(
        environment.get(field) != bound[name]["childPath"]
        for field, name in paths.items()
    ):
        raise LaunchRecordVerificationError("environment bypasses bound inputs")
    placeholders = {
        "{codex}": "codex",
        "{config}": "config",
        "{profile}": "profile",
        "{route}": "route",
        "{protocol}": "protocol",
        "{promptfoo}": "promptfoo",
        "{case}": "case",
        "{schema}": "schema",
    }
    placeholders.update(
        {"{artifact:" + name + "}": "artifact:" + name for name in declared_artifacts}
    )
    expanded = [
        bound[placeholders[item]]["childPath"] if item in placeholders else item
        for item in declared_argv
    ]
    if record.get("argv") != [bound["executable"]["childPath"], *expanded]:
        raise LaunchRecordVerificationError("argv bypasses bound inputs")
    if record.get("executableHandoff") not in {
        "darwinPathIdentityVerified",
        "inheritedDescriptorPath",
    }:
        raise LaunchRecordVerificationError("executable handoff is unsupported")
