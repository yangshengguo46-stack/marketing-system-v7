"""Construction and sealing of descriptor-derived process launch records."""

import sys

try:
    from .batch_receipt_storage import write_entry
    from .contracts import canonical_json_bytes
except ImportError:
    from batch_receipt_storage import write_entry
    from contracts import canonical_json_bytes


def build_launch_record(
    prepared: object,
    *,
    argv: list[str],
    environment: dict[str, str],
    pid: int,
    started_at: str,
    launch_spec_identity_fn: object,
    sha256_json_fn: object,
) -> dict[str, object]:
    """Build evidence only from descriptor reads and the exact OS handoff values."""
    prepared.verify_handoff()
    bound = {name: item.evidence() for name, item in sorted(prepared.inputs.items())}
    bound["executable"]["childPath"] = argv[0]
    artifacts = {
        name: evidence["sha256"] for name, evidence in bound.items() if name != "codex"
    }
    launch_spec = launch_spec_identity_fn(prepared.spec)[0]
    executable = bound["executable"]
    record = {
        "argv": list(argv),
        "artifactSha256": artifacts,
        "boundInputs": bound,
        "candidateBinarySha256": bound["codex"]["sha256"],
        "copiedExecutableSha256": executable["sha256"],
        "environment": dict(sorted(environment.items())),
        "environmentSha256": sha256_json_fn(dict(sorted(environment.items()))),
        "executableHandoff": (
            "darwinPathIdentityVerified"
            if sys.platform == "darwin"
            else "inheritedDescriptorPath"
        ),
        "executableSha256": executable["sha256"],
        "launchSpec": launch_spec,
        "pid": pid,
        "startedAt": started_at,
    }
    if (
        record["executableSha256"] != prepared.spec.executable_sha256
        or launch_spec.get("executableSha256") != record["executableSha256"]
    ):
        raise ValueError("launch record executable identity differs")
    return record


def seal_launch_record(owned: object, directory: object) -> None:
    write_entry(
        directory,
        "launch-record.json",
        canonical_json_bytes(owned.launch_record) + b"\n",
    )
