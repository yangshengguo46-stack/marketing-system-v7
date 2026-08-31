"""Complete ctypes ABI declarations used by the isolated-cell Win32 layer."""

import ctypes
from ctypes import wintypes
from dataclasses import dataclass

try:
    from .batch_isolation_win32_policy import StandardInformation
except ImportError:
    from batch_isolation_win32_policy import StandardInformation


kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
advapi32 = ctypes.WinDLL("advapi32", use_last_error=True)
ntdll = ctypes.WinDLL("ntdll")

GENERIC_READ = 0x80000000
GENERIC_WRITE = 0x40000000
DELETE = 0x00010000
FILE_READ_ATTRIBUTES = 0x00000080
READ_CONTROL = 0x00020000
SYNCHRONIZE = 0x00100000
SHARE_READ_WRITE = 0x3
SHARE_DELETE = 0x4
OPEN_EXISTING = 3
CREATE_NEW = 1
BACKUP_SEMANTICS = 0x02000000
OPEN_REPARSE_POINT = 0x00200000
ATTR_REPARSE = 0x400
ATTR_DIRECTORY = 0x10
INVALID_HANDLE = ctypes.c_void_p(-1).value
SDDL_REVISION = 1
SE_FILE_OBJECT = 1
OWNER_SECURITY_INFORMATION = 0x1
DACL_SECURITY_INFORMATION = 0x4
PROTECTED_DACL_SECURITY_INFORMATION = 0x80000000
FILE_DISPOSITION_INFO = 4
FILE_STANDARD_INFO = 1
TOKEN_QUERY = 0x8
TOKEN_USER = 1
FILE_BEGIN = 0
FIND_EX_INFO_BASIC = 1
FIND_EX_SEARCH_NAME_MATCH = 0
FIND_FIRST_EX_LARGE_FETCH = 2
FILE_CREATE = 2
FILE_DIRECTORY_FILE = 0x1
FILE_SYNCHRONOUS_IO_NONALERT = 0x20
OBJ_CASE_INSENSITIVE = 0x40
OBJ_DONT_REPARSE = 0x1000


@dataclass(frozen=True)
class DirectoryEntry:
    name: str
    attributes: int
    size: int

    @property
    def is_directory(self) -> bool:
        return bool(self.attributes & ATTR_DIRECTORY)

    @property
    def is_reparse(self) -> bool:
        return bool(self.attributes & ATTR_REPARSE)


class SecurityAttributes(ctypes.Structure):
    _fields_ = [
        ("nLength", wintypes.DWORD),
        ("lpSecurityDescriptor", wintypes.LPVOID),
        ("bInheritHandle", wintypes.BOOL),
    ]


class FileInformation(ctypes.Structure):
    _fields_ = [
        ("dwFileAttributes", wintypes.DWORD),
        ("ftCreationTime", wintypes.FILETIME),
        ("ftLastAccessTime", wintypes.FILETIME),
        ("ftLastWriteTime", wintypes.FILETIME),
        ("dwVolumeSerialNumber", wintypes.DWORD),
        ("nFileSizeHigh", wintypes.DWORD),
        ("nFileSizeLow", wintypes.DWORD),
        ("nNumberOfLinks", wintypes.DWORD),
        ("nFileIndexHigh", wintypes.DWORD),
        ("nFileIndexLow", wintypes.DWORD),
    ]


class Disposition(ctypes.Structure):
    _fields_ = [("DeleteFile", ctypes.c_ubyte)]


class SidAndAttributes(ctypes.Structure):
    _fields_ = [("Sid", wintypes.LPVOID), ("Attributes", wintypes.DWORD)]


class FindData(ctypes.Structure):
    _fields_ = [
        ("dwFileAttributes", wintypes.DWORD),
        ("ftCreationTime", wintypes.FILETIME),
        ("ftLastAccessTime", wintypes.FILETIME),
        ("ftLastWriteTime", wintypes.FILETIME),
        ("nFileSizeHigh", wintypes.DWORD),
        ("nFileSizeLow", wintypes.DWORD),
        ("dwReserved0", wintypes.DWORD),
        ("dwReserved1", wintypes.DWORD),
        ("cFileName", wintypes.WCHAR * 260),
        ("cAlternateFileName", wintypes.WCHAR * 14),
    ]


class UnicodeString(ctypes.Structure):
    _fields_ = [
        ("Length", wintypes.USHORT),
        ("MaximumLength", wintypes.USHORT),
        ("Buffer", wintypes.LPWSTR),
    ]


class ObjectAttributes(ctypes.Structure):
    _fields_ = [
        ("Length", wintypes.ULONG),
        ("RootDirectory", wintypes.HANDLE),
        ("ObjectName", ctypes.POINTER(UnicodeString)),
        ("Attributes", wintypes.ULONG),
        ("SecurityDescriptor", wintypes.LPVOID),
        ("SecurityQualityOfService", wintypes.LPVOID),
    ]


class IoStatusBlock(ctypes.Structure):
    _fields_ = [("Status", wintypes.LPVOID), ("Information", ctypes.c_size_t)]


def _declare() -> None:
    declarations = {
        kernel32.CreateFileW: (
            wintypes.HANDLE,
            [
                wintypes.LPCWSTR,
                wintypes.DWORD,
                wintypes.DWORD,
                wintypes.LPVOID,
                wintypes.DWORD,
                wintypes.DWORD,
                wintypes.HANDLE,
            ],
        ),
        kernel32.CloseHandle: (wintypes.BOOL, [wintypes.HANDLE]),
        kernel32.GetFileInformationByHandle: (
            wintypes.BOOL,
            [wintypes.HANDLE, ctypes.POINTER(FileInformation)],
        ),
        kernel32.GetCurrentProcess: (wintypes.HANDLE, []),
        kernel32.DuplicateHandle: (
            wintypes.BOOL,
            [
                wintypes.HANDLE,
                wintypes.HANDLE,
                wintypes.HANDLE,
                ctypes.POINTER(wintypes.HANDLE),
                wintypes.DWORD,
                wintypes.BOOL,
                wintypes.DWORD,
            ],
        ),
        kernel32.LocalFree: (wintypes.LPVOID, [wintypes.LPVOID]),
        kernel32.GetFinalPathNameByHandleW: (
            wintypes.DWORD,
            [wintypes.HANDLE, wintypes.LPWSTR, wintypes.DWORD, wintypes.DWORD],
        ),
        kernel32.CreateDirectoryW: (
            wintypes.BOOL,
            [wintypes.LPCWSTR, ctypes.POINTER(SecurityAttributes)],
        ),
        kernel32.WriteFile: (
            wintypes.BOOL,
            [
                wintypes.HANDLE,
                wintypes.LPCVOID,
                wintypes.DWORD,
                ctypes.POINTER(wintypes.DWORD),
                wintypes.LPVOID,
            ],
        ),
        kernel32.FlushFileBuffers: (wintypes.BOOL, [wintypes.HANDLE]),
        kernel32.SetFilePointerEx: (
            wintypes.BOOL,
            [
                wintypes.HANDLE,
                ctypes.c_longlong,
                ctypes.POINTER(ctypes.c_longlong),
                wintypes.DWORD,
            ],
        ),
        kernel32.ReadFile: (
            wintypes.BOOL,
            [
                wintypes.HANDLE,
                wintypes.LPVOID,
                wintypes.DWORD,
                ctypes.POINTER(wintypes.DWORD),
                wintypes.LPVOID,
            ],
        ),
        kernel32.SetFileInformationByHandle: (
            wintypes.BOOL,
            [wintypes.HANDLE, ctypes.c_int, wintypes.LPVOID, wintypes.DWORD],
        ),
        kernel32.GetFileInformationByHandleEx: (
            wintypes.BOOL,
            [wintypes.HANDLE, ctypes.c_int, wintypes.LPVOID, wintypes.DWORD],
        ),
        kernel32.FindFirstFileExW: (
            wintypes.HANDLE,
            [
                wintypes.LPCWSTR,
                ctypes.c_int,
                wintypes.LPVOID,
                ctypes.c_int,
                wintypes.LPVOID,
                wintypes.DWORD,
            ],
        ),
        kernel32.FindNextFileW: (
            wintypes.BOOL,
            [wintypes.HANDLE, ctypes.POINTER(FindData)],
        ),
        kernel32.FindClose: (wintypes.BOOL, [wintypes.HANDLE]),
        advapi32.OpenProcessToken: (
            wintypes.BOOL,
            [wintypes.HANDLE, wintypes.DWORD, ctypes.POINTER(wintypes.HANDLE)],
        ),
        advapi32.GetTokenInformation: (
            wintypes.BOOL,
            [
                wintypes.HANDLE,
                ctypes.c_int,
                wintypes.LPVOID,
                wintypes.DWORD,
                ctypes.POINTER(wintypes.DWORD),
            ],
        ),
        advapi32.ConvertSidToStringSidW: (
            wintypes.BOOL,
            [wintypes.LPVOID, ctypes.POINTER(wintypes.LPWSTR)],
        ),
        advapi32.ConvertStringSecurityDescriptorToSecurityDescriptorW: (
            wintypes.BOOL,
            [
                wintypes.LPCWSTR,
                wintypes.DWORD,
                ctypes.POINTER(wintypes.LPVOID),
                ctypes.POINTER(wintypes.DWORD),
            ],
        ),
        advapi32.GetSecurityInfo: (
            wintypes.DWORD,
            [
                wintypes.HANDLE,
                ctypes.c_int,
                wintypes.DWORD,
                ctypes.POINTER(wintypes.LPVOID),
                ctypes.POINTER(wintypes.LPVOID),
                ctypes.POINTER(wintypes.LPVOID),
                ctypes.POINTER(wintypes.LPVOID),
                ctypes.POINTER(wintypes.LPVOID),
            ],
        ),
        advapi32.ConvertSecurityDescriptorToStringSecurityDescriptorW: (
            wintypes.BOOL,
            [
                wintypes.LPVOID,
                wintypes.DWORD,
                wintypes.DWORD,
                ctypes.POINTER(wintypes.LPWSTR),
                ctypes.POINTER(wintypes.DWORD),
            ],
        ),
        ntdll.NtCreateFile: (
            ctypes.c_long,
            [
                ctypes.POINTER(wintypes.HANDLE),
                wintypes.DWORD,
                ctypes.POINTER(ObjectAttributes),
                ctypes.POINTER(IoStatusBlock),
                ctypes.POINTER(ctypes.c_longlong),
                wintypes.DWORD,
                wintypes.DWORD,
                wintypes.DWORD,
                wintypes.DWORD,
                wintypes.LPVOID,
                wintypes.DWORD,
            ],
        ),
    }
    for function, (restype, argtypes) in declarations.items():
        function.restype = restype
        function.argtypes = argtypes


_declare()
