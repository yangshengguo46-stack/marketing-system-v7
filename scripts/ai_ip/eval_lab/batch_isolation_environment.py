"""Deny-by-default environment projection for isolated attempts."""

import os
import re
from pathlib import Path
from types import MappingProxyType
from typing import Callable, Mapping


LOCALE_NAMES = {
    "LANG",
    "LANGUAGE",
    "LC_ALL",
    "LC_COLLATE",
    "LC_CTYPE",
    "LC_MESSAGES",
    "LC_MONETARY",
    "LC_NUMERIC",
    "LC_TIME",
}
PROFILE_NAMES = {"AWS_PROFILE", "VOLCENGINE_PROFILE"}
APPROVED_PROFILE_NAMES = LOCALE_NAMES | PROFILE_NAMES
_PROFILE_VALUE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}\Z")


class EnvironmentPolicyError(ValueError):
    pass


def windows_overrides(home: Path, cache: Path, temp: Path) -> dict[str, str]:
    home_text = str(home)
    return {
        "USERPROFILE": home_text,
        "HOMEDRIVE": home.drive,
        "HOMEPATH": home_text[len(home.drive) :],
        "APPDATA": str(home / "AppData" / "Roaming"),
        "LOCALAPPDATA": str(cache),
        "TEMP": str(temp),
        "TMP": str(temp),
    }


def build_environment(
    paths: dict[str, Path],
    profile: dict[str, object],
    source: Mapping[str, str],
    windows_projection: Callable[[Path, Path, Path], dict[str, str]],
) -> Mapping[str, str]:
    if not isinstance(source, Mapping):
        raise EnvironmentPolicyError("source environment must be a mapping")
    if os.name == "nt":
        folded = [name.casefold() for name in source if type(name) is str]
        if len(folded) != len(set(folded)):
            raise EnvironmentPolicyError(
                "source environment contains case-insensitive duplicates"
            )
    declared = profile["environmentAllowlist"]
    assert isinstance(declared, list)
    names = {"PATH", "SHELL", *LOCALE_NAMES, *declared}
    environment: dict[str, str] = {}
    for name in names:
        if name not in source:
            continue
        value = source[name]
        if type(value) is not str or "\0" in value:
            raise EnvironmentPolicyError(f"invalid environment value: {name}")
        if name in PROFILE_NAMES and not _PROFILE_VALUE.fullmatch(value):
            raise EnvironmentPolicyError(
                f"profile environment value is not a safe name: {name}"
            )
        environment[name] = value
    environment.update(
        {
            "HOME": str(paths["home"]),
            "CODEX_HOME": str(paths["home"]),
            "TMPDIR": str(paths["temp"]),
            "PROMPTFOO_CACHE_PATH": str(paths["cache"] / "promptfoo"),
            "PROMPTFOO_CONFIG_DIR": str(paths["promptfoo"]),
            "PROMPTFOO_OUTPUT_PATH": str(paths["promptfoo"] / "output.json"),
            "PROMPTFOO_CACHE_ENABLED": "false",
            "FORCE_COLOR": "0",
            "NO_PROXY": "127.0.0.1,localhost,::1",
        }
    )
    if os.name == "nt":
        environment.update(
            windows_projection(paths["home"], paths["cache"], paths["temp"])
        )
    return MappingProxyType(environment)
