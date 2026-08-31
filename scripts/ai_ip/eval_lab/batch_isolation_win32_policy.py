"""Portable Win32 ABI layout and exact private-DACL policy."""

import ctypes
import re


BOOLEAN = ctypes.c_ubyte


class StandardInformation(ctypes.Structure):
    _fields_ = [
        ("AllocationSize", ctypes.c_int64),
        ("EndOfFile", ctypes.c_int64),
        ("NumberOfLinks", ctypes.c_uint32),
        ("DeletePending", BOOLEAN),
        ("Directory", BOOLEAN),
    ]


if (
    ctypes.sizeof(StandardInformation) != 24
    or StandardInformation.DeletePending.offset != 20
    or StandardInformation.Directory.offset != 21
):
    raise RuntimeError("FILE_STANDARD_INFO ABI layout is unavailable")


_ACE = re.compile(r"\(([^()]*)\)")


def private_sddl(user_sid: str, *, directory: bool) -> str:
    flags = "OICI" if directory else ""
    return f"O:{user_sid}D:P(A;{flags};FA;;;SY)(A;{flags};FA;;;{user_sid})"


def _flags(value: str) -> frozenset[str] | None:
    if len(value) % 2:
        return None
    result = frozenset(value[index : index + 2] for index in range(0, len(value), 2))
    if len(result) * 2 != len(value):
        return None
    return result


def is_exact_private_sddl(sddl: str, user_sid: str, directory: bool) -> bool:
    prefix = f"O:{user_sid}D:P"
    if not sddl.startswith(prefix):
        return False
    ace_texts = _ACE.findall(sddl[len(prefix) :])
    if len(ace_texts) != 2 or prefix + "".join(f"({ace})" for ace in ace_texts) != sddl:
        return False
    expected_flags = frozenset({"OI", "CI"}) if directory else frozenset()
    principals: set[str] = set()
    for ace in ace_texts:
        fields = ace.split(";")
        if len(fields) != 6:
            return False
        ace_type, flags, rights, object_guid, inherit_guid, principal = fields
        if (
            ace_type != "A"
            or _flags(flags) != expected_flags
            or rights != "FA"
            or object_guid
            or inherit_guid
            or principal not in {"SY", user_sid}
        ):
            return False
        principals.add(principal)
    return principals == {"SY", user_sid}
