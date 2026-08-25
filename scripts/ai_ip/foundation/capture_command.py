import json
import platform
import subprocess
from dataclasses import dataclass
from pathlib import Path


class EvidenceError(ValueError):
    pass


@dataclass(frozen=True)
class CommandSpec:
    id: str
    platforms: tuple[str, ...]
    phase: str
    argv: tuple[str, ...]
    expected_exit: int


@dataclass(frozen=True)
class Matrix:
    schema_version: int
    commands: tuple[CommandSpec, ...]

    def required_for(self, platform_name: str, mode: str) -> tuple[CommandSpec, ...]:
        if mode not in {"baseline", "post"}:
            raise EvidenceError(f"unknown evidence mode: {mode}")
        phases = (
            {"baselineAndPost"}
            if mode == "baseline"
            else {"baselineAndPost", "postOnly"}
        )
        return tuple(
            command
            for command in self.commands
            if platform_name in command.platforms and command.phase in phases
        )


def canonical_json_bytes(value: object) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
        allow_nan=False,
    ).encode("utf-8")


def _reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise EvidenceError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _require_exact_keys(
    value: object, expected: set[str], description: str
) -> dict[str, object]:
    if not isinstance(value, dict):
        raise EvidenceError(f"{description} must be an object")
    keys = set(value)
    if keys != expected:
        unknown = keys - expected
        if unknown:
            raise EvidenceError(
                f"unknown {description} keys: {', '.join(sorted(unknown))}"
            )
        raise EvidenceError(f"invalid {description} keys")
    return value


def load_matrix(path: Path) -> Matrix:
    try:
        parsed = json.loads(
            path.read_text(encoding="utf-8"), object_pairs_hook=_reject_duplicate_keys
        )
    except json.JSONDecodeError as error:
        raise EvidenceError(f"invalid matrix JSON: {error}") from error
    root = _require_exact_keys(parsed, {"schemaVersion", "commands"}, "matrix")
    schema_version = root["schemaVersion"]
    if (
        not isinstance(schema_version, int)
        or isinstance(schema_version, bool)
        or schema_version != 1
    ):
        raise EvidenceError("schemaVersion must be integer 1")
    entries = root["commands"]
    if not isinstance(entries, list):
        raise EvidenceError("commands must be an array")
    commands: list[CommandSpec] = []
    ids: set[str] = set()
    valid_platforms = {"macos-x86_64", "windows-11-x64"}
    valid_phases = {"baselineAndPost", "postOnly"}
    for entry in entries:
        command = _require_exact_keys(
            entry, {"id", "platforms", "phase", "argv", "expectedExit"}, "command"
        )
        command_id = command["id"]
        if not isinstance(command_id, str) or not command_id:
            raise EvidenceError("id must be a non-empty string")
        if command_id in ids:
            raise EvidenceError(f"duplicate command id: {command_id}")
        platforms = command["platforms"]
        if (
            not isinstance(platforms, list)
            or not platforms
            or any(not isinstance(item, str) for item in platforms)
        ):
            raise EvidenceError("platforms must be a non-empty array of strings")
        if any(item not in valid_platforms for item in platforms):
            raise EvidenceError("unknown platform")
        if len(set(platforms)) != len(platforms):
            raise EvidenceError("duplicate platform")
        phase = command["phase"]
        if not isinstance(phase, str) or phase not in valid_phases:
            raise EvidenceError("unknown phase")
        argv = command["argv"]
        if not isinstance(argv, list) or not argv:
            raise EvidenceError("argv must be a non-empty array")
        if any(not isinstance(argument, str) or not argument for argument in argv):
            raise EvidenceError("argv arguments must be non-empty strings")
        expected_exit = command["expectedExit"]
        if not isinstance(expected_exit, int) or isinstance(expected_exit, bool):
            raise EvidenceError("expectedExit must be an integer")
        if expected_exit != 0:
            raise EvidenceError("expectedExit must be zero")
        ids.add(command_id)
        commands.append(
            CommandSpec(command_id, tuple(platforms), phase, tuple(argv), expected_exit)
        )
    return Matrix(schema_version, tuple(commands))


def host_id() -> str:
    system, machine = platform.system(), platform.machine()
    if (system, machine) == ("Darwin", "x86_64"):
        try:
            translated = subprocess.run(
                ["sysctl", "-n", "sysctl.proc_translated"],
                capture_output=True,
                text=True,
                check=False,
            )
        except OSError as error:
            raise EvidenceError("unable to characterize macOS host") from error
        if (
            translated.returncode == 0
            and translated.stdout == "0\n"
            and not translated.stderr
        ):
            return "macos-x86_64"
        if (
            translated.returncode == 1
            and not translated.stdout
            and translated.stderr == "sysctl: unknown oid 'sysctl.proc_translated'\n"
        ):
            return "macos-x86_64"
        raise EvidenceError("unsupported macOS translation state")
    if system == "Windows" and machine in {"AMD64", "x86_64"}:
        version = platform.version()
        if platform.release() == "11" or (
            version.split(".")[-1].isdigit() and int(version.split(".")[-1]) >= 22000
        ):
            return "windows-11-x64"
    raise EvidenceError(f"unsupported evidence host: {system}/{machine}")


def selection_argv(command: CommandSpec) -> tuple[str, ...] | None:
    if command.argv[:2] != ("just", "test") or "-E" not in command.argv:
        return None
    return ("cargo", "nextest", "list", "--message-format", "json", *command.argv[2:])
