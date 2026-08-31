"""Handle-bound operations built on the isolated-cell Win32 ABI."""

import ctypes
from ctypes import wintypes
from pathlib import Path
from typing import Iterator

try:
    from . import batch_isolation_win32_abi as abi
    from .batch_isolation_win32_policy import is_exact_private_sddl
    from .batch_isolation_win32_policy import private_sddl
    from .batch_isolation_windows_handle_ledger import WindowsHandleLedger
except ImportError:
    import batch_isolation_win32_abi as abi
    from batch_isolation_win32_policy import is_exact_private_sddl
    from batch_isolation_win32_policy import private_sddl
    from batch_isolation_windows_handle_ledger import WindowsHandleLedger


advapi32 = abi.advapi32
kernel32 = abi.kernel32
ATTR_DIRECTORY = abi.ATTR_DIRECTORY
ATTR_REPARSE = abi.ATTR_REPARSE
BACKUP_SEMANTICS = abi.BACKUP_SEMANTICS
CREATE_NEW = abi.CREATE_NEW
DACL_SECURITY_INFORMATION = abi.DACL_SECURITY_INFORMATION
DELETE = abi.DELETE
FILE_BEGIN = abi.FILE_BEGIN
FILE_CREATE = abi.FILE_CREATE
FILE_DIRECTORY_FILE = abi.FILE_DIRECTORY_FILE
FILE_DISPOSITION_INFO = abi.FILE_DISPOSITION_INFO
FILE_READ_ATTRIBUTES = abi.FILE_READ_ATTRIBUTES
FILE_STANDARD_INFO = abi.FILE_STANDARD_INFO
FILE_SYNCHRONOUS_IO_NONALERT = abi.FILE_SYNCHRONOUS_IO_NONALERT
FIND_EX_INFO_BASIC = abi.FIND_EX_INFO_BASIC
FIND_EX_SEARCH_NAME_MATCH = abi.FIND_EX_SEARCH_NAME_MATCH
FIND_FIRST_EX_LARGE_FETCH = abi.FIND_FIRST_EX_LARGE_FETCH
GENERIC_READ = abi.GENERIC_READ
GENERIC_WRITE = abi.GENERIC_WRITE
INVALID_HANDLE = abi.INVALID_HANDLE
OPEN_EXISTING = abi.OPEN_EXISTING
OPEN_REPARSE_POINT = abi.OPEN_REPARSE_POINT
OBJ_CASE_INSENSITIVE = abi.OBJ_CASE_INSENSITIVE
OBJ_DONT_REPARSE = abi.OBJ_DONT_REPARSE
OWNER_SECURITY_INFORMATION = abi.OWNER_SECURITY_INFORMATION
PROTECTED_DACL_SECURITY_INFORMATION = abi.PROTECTED_DACL_SECURITY_INFORMATION
READ_CONTROL = abi.READ_CONTROL
SDDL_REVISION = abi.SDDL_REVISION
SE_FILE_OBJECT = abi.SE_FILE_OBJECT
SHARE_DELETE = abi.SHARE_DELETE
SHARE_READ_WRITE = abi.SHARE_READ_WRITE
SYNCHRONIZE = abi.SYNCHRONIZE
TOKEN_QUERY = abi.TOKEN_QUERY
TOKEN_USER = abi.TOKEN_USER
Disposition = abi.Disposition
DirectoryEntry = abi.DirectoryEntry
FileInformation = abi.FileInformation
FindData = abi.FindData
SecurityAttributes = abi.SecurityAttributes
SidAndAttributes = abi.SidAndAttributes
StandardInformation = abi.StandardInformation
IoStatusBlock = abi.IoStatusBlock
ObjectAttributes = abi.ObjectAttributes
UnicodeString = abi.UnicodeString
ntdll = abi.ntdll


class Win32SecurityError(OSError):
    pass


def _raise(message: str) -> None:
    raise Win32SecurityError(ctypes.get_last_error(), message)


def close_raw(handle: int) -> None:
    if handle not in (-1, None) and not kernel32.CloseHandle(handle):  # noqa: F405
        _raise("cannot close retained Windows handle")


def _sid_text(sid: wintypes.LPVOID) -> str:
    text = wintypes.LPWSTR()
    if not advapi32.ConvertSidToStringSidW(sid, ctypes.byref(text)):  # noqa: F405
        _raise("cannot serialize SID")
    try:
        return text.value
    finally:
        kernel32.LocalFree(text)  # noqa: F405


def current_user_sid() -> str:
    token = wintypes.HANDLE()
    if not advapi32.OpenProcessToken(  # noqa: F405
        kernel32.GetCurrentProcess(),
        TOKEN_QUERY,
        ctypes.byref(token),  # noqa: F405
    ):
        _raise("cannot open current-user token")
    try:
        needed = wintypes.DWORD()
        advapi32.GetTokenInformation(  # noqa: F405
            token,
            TOKEN_USER,
            None,
            0,
            ctypes.byref(needed),  # noqa: F405
        )
        buffer = ctypes.create_string_buffer(needed.value)
        if not advapi32.GetTokenInformation(  # noqa: F405
            token,
            TOKEN_USER,
            buffer,
            len(buffer),
            ctypes.byref(needed),  # noqa: F405
        ):
            _raise("cannot read current-user SID")
        sid = ctypes.cast(
            buffer,
            ctypes.POINTER(SidAndAttributes),  # noqa: F405
        ).contents.Sid
        return _sid_text(sid)
    finally:
        close_raw(token)


USER_SID = current_user_sid()


def security_attributes(
    *, directory: bool
) -> tuple[SecurityAttributes, wintypes.LPVOID]:  # noqa: F405
    descriptor = wintypes.LPVOID()
    if not advapi32.ConvertStringSecurityDescriptorToSecurityDescriptorW(  # noqa: F405
        private_sddl(USER_SID, directory=directory),
        SDDL_REVISION,
        ctypes.byref(descriptor),
        None,  # noqa: F405
    ):
        _raise("cannot construct protected DACL")
    attributes = SecurityAttributes(  # noqa: F405
        ctypes.sizeof(SecurityAttributes),
        descriptor,
        False,  # noqa: F405
    )
    return attributes, descriptor


def information(handle: int) -> FileInformation:  # noqa: F405
    result = FileInformation()  # noqa: F405
    if not kernel32.GetFileInformationByHandle(  # noqa: F405
        handle, ctypes.byref(result)
    ):
        _raise("cannot query Windows file identity")
    return result


def identity(handle: int) -> tuple[int, int]:
    info = information(handle)
    return info.dwVolumeSerialNumber, info.nFileIndexHigh << 32 | info.nFileIndexLow


def duplicate_raw(handle: int) -> int:
    process = kernel32.GetCurrentProcess()  # noqa: F405
    result = wintypes.HANDLE()
    if not kernel32.DuplicateHandle(  # noqa: F405
        process, handle, process, ctypes.byref(result), 0, False, 2
    ):
        _raise("cannot duplicate retained Windows handle")
    return result.value


_HANDLE_LEDGER = WindowsHandleLedger((duplicate_raw, identity, close_raw))


def duplicate(handle: int) -> int:
    _HANDLE_LEDGER.retry_uncertain_closes()
    expected_identity = identity(handle)
    duplicate_handle = duplicate_raw(handle)
    return _HANDLE_LEDGER.acquire(duplicate_handle, expected_identity=expected_identity)


def close(handle: int) -> None:
    _HANDLE_LEDGER.close(handle)


def handle_ledger() -> WindowsHandleLedger:
    return _HANDLE_LEDGER


def final_path(handle: int) -> Path:
    needed = kernel32.GetFinalPathNameByHandleW(handle, None, 0, 0)  # noqa: F405
    if not needed:
        _raise("cannot resolve retained Windows handle")
    buffer = ctypes.create_unicode_buffer(needed + 1)
    if not kernel32.GetFinalPathNameByHandleW(  # noqa: F405
        handle, buffer, len(buffer), 0
    ):
        _raise("cannot resolve retained Windows handle")
    text = buffer.value
    if text.startswith("\\\\?\\UNC\\"):
        text = "\\\\" + text[8:]
    elif text.startswith("\\\\?\\"):
        text = text[4:]
    return Path(text)


def open_path_raw(
    path: Path,
    *,
    directory: bool,
    deletable: bool = False,
    security_query: bool = False,
) -> int:
    flags = OPEN_REPARSE_POINT | (BACKUP_SEMANTICS if directory else 0)  # noqa: F405
    handle = kernel32.CreateFileW(  # noqa: F405
        str(path),
        FILE_READ_ATTRIBUTES  # noqa: F405
        | SYNCHRONIZE
        | (DELETE if deletable else 0)
        | (READ_CONTROL if security_query else 0),
        SHARE_READ_WRITE,  # noqa: F405
        None,
        OPEN_EXISTING,  # noqa: F405
        flags,
        None,
    )
    if handle == INVALID_HANDLE:  # noqa: F405
        _raise("cannot retain secure Windows handle")
    return handle


def validate_opened_handle(
    handle: int, *, directory: bool, allow_reparse: bool = False
) -> tuple[int, int]:
    info = information(handle)
    is_directory = bool(info.dwFileAttributes & ATTR_DIRECTORY)  # noqa: F405
    if (info.dwFileAttributes & ATTR_REPARSE and not allow_reparse) or (  # noqa: F405
        not allow_reparse and is_directory != directory
    ):
        raise Win32SecurityError("reparse point or wrong entry type is forbidden")
    return info.dwVolumeSerialNumber, info.nFileIndexHigh << 32 | info.nFileIndexLow


def open_path(
    path: Path,
    *,
    directory: bool,
    allow_reparse: bool = False,
    deletable: bool = False,
    security_query: bool = False,
) -> int:
    _HANDLE_LEDGER.retry_uncertain_closes()
    handle = _HANDLE_LEDGER.acquire(
        open_path_raw(
            path,
            directory=directory,
            deletable=deletable,
            security_query=security_query,
        )
    )
    try:
        validate_opened_handle(
            handle, directory=directory, allow_reparse=allow_reparse
        )
    except BaseException:
        close(handle)
        raise
    return handle


def child_path(parent: int, name: str) -> Path:
    if name in {"", ".", ".."} or "\\" in name or "/" in name:
        raise Win32SecurityError("unsafe Windows child name")
    return final_path(parent) / name


def open_child(
    parent: int, name: str, *, directory: bool, deletable: bool = False
) -> int:
    return open_path(child_path(parent, name), directory=directory, deletable=deletable)


def open_child_raw(
    parent: int,
    name: str,
    *,
    directory: bool,
    deletable: bool = False,
    security_query: bool = False,
) -> int:
    return open_path_raw(
        child_path(parent, name),
        directory=directory,
        deletable=deletable,
        security_query=security_query,
    )


def create_directory(parent: int, name: str) -> int:
    if name in {"", ".", ".."} or "\\" in name or "/" in name:
        raise Win32SecurityError("unsafe Windows child name")
    _, descriptor = security_attributes(directory=True)
    buffer = ctypes.create_unicode_buffer(name)
    object_name = UnicodeString(
        len(name.encode("utf-16-le")),
        len(name.encode("utf-16-le")) + 2,
        ctypes.cast(buffer, wintypes.LPWSTR),
    )
    attributes = ObjectAttributes(
        ctypes.sizeof(ObjectAttributes),
        parent,
        ctypes.pointer(object_name),
        OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
        descriptor,
        None,
    )
    status = IoStatusBlock()
    handle = wintypes.HANDLE()
    try:
        result = ntdll.NtCreateFile(
            ctypes.byref(handle),
            GENERIC_READ | DELETE | SYNCHRONIZE,
            ctypes.byref(attributes),
            ctypes.byref(status),
            None,
            ATTR_DIRECTORY,
            SHARE_READ_WRITE,
            FILE_CREATE,
            FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | OPEN_REPARSE_POINT,
            None,
            0,
        )
        if result < 0:
            raise Win32SecurityError(
                result, "cannot atomically create Windows directory"
            )
    finally:
        kernel32.LocalFree(descriptor)  # noqa: F405
    return handle.value


def create_file(parent: int, name: str, payload: bytes) -> int:
    attributes, descriptor = security_attributes(directory=False)
    try:
        handle = kernel32.CreateFileW(  # noqa: F405
            str(child_path(parent, name)),
            GENERIC_READ | GENERIC_WRITE | DELETE,  # noqa: F405
            SHARE_READ_WRITE,  # noqa: F405
            ctypes.byref(attributes),
            CREATE_NEW,  # noqa: F405
            OPEN_REPARSE_POINT,  # noqa: F405
            None,
        )
    finally:
        kernel32.LocalFree(descriptor)  # noqa: F405
    if handle == INVALID_HANDLE:  # noqa: F405
        _raise("cannot create protected Windows file")
    try:
        written = wintypes.DWORD()
        buffer = ctypes.create_string_buffer(payload)
        if not kernel32.WriteFile(  # noqa: F405
            handle, buffer, len(payload), ctypes.byref(written), None
        ) or written.value != len(payload):
            _raise("short secure Windows write")
        if not kernel32.FlushFileBuffers(handle):  # noqa: F405
            _raise("cannot flush secure Windows file")
        require_private_acl(handle)
        return handle
    except BaseException:
        try:
            mark_delete(handle)
        finally:
            close(handle)
        raise


def read_file(handle: int, limit: int) -> bytes:
    if not kernel32.SetFilePointerEx(handle, 0, None, FILE_BEGIN):  # noqa: F405
        _raise("cannot rewind secure Windows file")
    buffer = ctypes.create_string_buffer(limit + 1)
    count = wintypes.DWORD()
    if not kernel32.ReadFile(  # noqa: F405
        handle, buffer, limit + 1, ctypes.byref(count), None
    ):
        _raise("cannot read secure Windows file")
    if count.value > limit:
        raise Win32SecurityError("secure Windows file exceeds safety bound")
    return buffer.raw[: count.value]


def require_private_acl(handle: int) -> None:
    owner = wintypes.LPVOID()
    dacl = wintypes.LPVOID()
    descriptor = wintypes.LPVOID()
    result = advapi32.GetSecurityInfo(  # noqa: F405
        handle,
        SE_FILE_OBJECT,  # noqa: F405
        OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,  # noqa: F405
        ctypes.byref(owner),
        None,
        ctypes.byref(dacl),
        None,
        ctypes.byref(descriptor),
    )
    if result:
        raise Win32SecurityError(result, "cannot query protected DACL")
    text = wintypes.LPWSTR()
    try:
        if not owner or not dacl or _sid_text(owner) != USER_SID:
            raise Win32SecurityError("protected object has the wrong owner")
        if not advapi32.ConvertSecurityDescriptorToStringSecurityDescriptorW(  # noqa: F405
            descriptor,
            SDDL_REVISION,  # noqa: F405
            OWNER_SECURITY_INFORMATION  # noqa: F405
            | DACL_SECURITY_INFORMATION  # noqa: F405
            | PROTECTED_DACL_SECURITY_INFORMATION,  # noqa: F405
            ctypes.byref(text),
            None,
        ):
            _raise("cannot serialize protected DACL")
        sddl = text.value or ""
        directory = bool(information(handle).dwFileAttributes & ATTR_DIRECTORY)
        if not is_exact_private_sddl(sddl, USER_SID, directory):
            raise Win32SecurityError("protected DACL verification failed")
    finally:
        if text:
            kernel32.LocalFree(text)  # noqa: F405
        kernel32.LocalFree(descriptor)  # noqa: F405


def iter_directory(handle: int) -> Iterator[DirectoryEntry]:
    data = FindData()  # noqa: F405
    search = str(final_path(handle) / "*")
    find = kernel32.FindFirstFileExW(  # noqa: F405
        search,
        FIND_EX_INFO_BASIC,  # noqa: F405
        ctypes.byref(data),
        FIND_EX_SEARCH_NAME_MATCH,  # noqa: F405
        None,
        FIND_FIRST_EX_LARGE_FETCH,  # noqa: F405
    )
    if find == INVALID_HANDLE:  # noqa: F405
        if ctypes.get_last_error() == 2:
            return
        _raise("cannot stream Windows directory")
    try:
        while True:
            name = data.cFileName
            if name not in {".", ".."}:
                yield DirectoryEntry(
                    name,
                    data.dwFileAttributes,
                    data.nFileSizeHigh << 32 | data.nFileSizeLow,
                )
            if not kernel32.FindNextFileW(find, ctypes.byref(data)):  # noqa: F405
                if ctypes.get_last_error() == 18:
                    break
                _raise("cannot continue Windows directory stream")
    finally:
        if not kernel32.FindClose(find):  # noqa: F405
            _raise("cannot close Windows directory stream")


def mark_delete(handle: int) -> None:
    disposition = Disposition(True)  # noqa: F405
    if not kernel32.SetFileInformationByHandle(  # noqa: F405
        handle,
        FILE_DISPOSITION_INFO,  # noqa: F405
        ctypes.byref(disposition),
        ctypes.sizeof(disposition),
    ):
        _raise("handle-bound Windows deletion failed")
    if not delete_pending(handle):
        raise Win32SecurityError("Windows deletion was not made pending")


def delete_pending(handle: int) -> bool:
    standard = StandardInformation()  # noqa: F405
    if not kernel32.GetFileInformationByHandleEx(  # noqa: F405
        handle,
        FILE_STANDARD_INFO,  # noqa: F405
        ctypes.byref(standard),
        ctypes.sizeof(standard),
    ):
        _raise("cannot query Windows deletion disposition")
    return bool(standard.DeletePending)
