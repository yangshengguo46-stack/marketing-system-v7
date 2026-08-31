"""Win32 secure-filesystem backend for isolated candidate cells."""

import ctypes
import os
import secrets
import stat
from ctypes import wintypes
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


class SecureFilesystemError(OSError):
    pass


if os.name == "nt":
    _kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    _advapi32 = ctypes.WinDLL("advapi32", use_last_error=True)
    _GENERIC_READ = 0x80000000
    _GENERIC_WRITE = 0x40000000
    _DELETE = 0x00010000
    _FILE_SHARE_READ = 1
    _FILE_SHARE_WRITE = 2
    _OPEN_EXISTING = 3
    _CREATE_NEW = 1
    _FILE_FLAG_BACKUP_SEMANTICS = 0x02000000
    _FILE_FLAG_OPEN_REPARSE_POINT = 0x00200000
    _FILE_ATTRIBUTE_REPARSE_POINT = 0x400
    _FILE_ATTRIBUTE_DIRECTORY = 0x10
    _INVALID_HANDLE_VALUE = ctypes.c_void_p(-1).value
    _SDDL_REVISION_1 = 1
    _DACL_SECURITY_INFORMATION = 0x00000004
    _PROTECTED_DACL_SECURITY_INFORMATION = 0x80000000
    _FILE_DISPOSITION_INFO = 4
    _TOKEN_QUERY = 0x0008
    _TOKEN_USER = 1
    _MAX_DELETE_ENTRIES = 100_000

    _kernel32.CreateFileW.restype = wintypes.HANDLE
    _kernel32.CreateFileW.argtypes = [
        wintypes.LPCWSTR,
        wintypes.DWORD,
        wintypes.DWORD,
        wintypes.LPVOID,
        wintypes.DWORD,
        wintypes.DWORD,
        wintypes.HANDLE,
    ]
    _kernel32.GetCurrentProcess.restype = wintypes.HANDLE
    _kernel32.LocalFree.restype = wintypes.LPVOID
    _advapi32.GetNamedSecurityInfoW.restype = wintypes.DWORD

    class _SecurityAttributes(ctypes.Structure):
        _fields_ = [
            ("nLength", wintypes.DWORD),
            ("lpSecurityDescriptor", wintypes.LPVOID),
            ("bInheritHandle", wintypes.BOOL),
        ]

    class _FileInformation(ctypes.Structure):
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

    class _Disposition(ctypes.Structure):
        _fields_ = [("DeleteFile", wintypes.BOOL)]

    class _SidAndAttributes(ctypes.Structure):
        _fields_ = [("Sid", wintypes.LPVOID), ("Attributes", wintypes.DWORD)]

    def _raise(message: str) -> None:
        raise SecureFilesystemError(ctypes.get_last_error(), message)

    def _current_user_sid() -> str:
        token = wintypes.HANDLE()
        if not _advapi32.OpenProcessToken(
            _kernel32.GetCurrentProcess(), _TOKEN_QUERY, ctypes.byref(token)
        ):
            _raise("cannot open current-user token")
        try:
            required = wintypes.DWORD()
            _advapi32.GetTokenInformation(
                token, _TOKEN_USER, None, 0, ctypes.byref(required)
            )
            buffer = ctypes.create_string_buffer(required.value)
            if not _advapi32.GetTokenInformation(
                token,
                _TOKEN_USER,
                buffer,
                len(buffer),
                ctypes.byref(required),
            ):
                _raise("cannot read current-user SID")
            sid = ctypes.cast(buffer, ctypes.POINTER(_SidAndAttributes)).contents.Sid
            text = wintypes.LPWSTR()
            if not _advapi32.ConvertSidToStringSidW(sid, ctypes.byref(text)):
                _raise("cannot serialize current-user SID")
            try:
                return text.value
            finally:
                _kernel32.LocalFree(text)
        finally:
            _kernel32.CloseHandle(token)

    _USER_SID = _current_user_sid()
    _PRIVATE_SDDL = f"D:P(A;;FA;;;SY)(A;;FA;;;{_USER_SID})"

    def _security_attributes() -> tuple[_SecurityAttributes, wintypes.LPVOID]:
        descriptor = wintypes.LPVOID()
        if not _advapi32.ConvertStringSecurityDescriptorToSecurityDescriptorW(
            _PRIVATE_SDDL, _SDDL_REVISION_1, ctypes.byref(descriptor), None
        ):
            _raise("cannot construct protected DACL")
        attributes = _SecurityAttributes(
            ctypes.sizeof(_SecurityAttributes), descriptor, False
        )
        return attributes, descriptor

    def _open(
        path: Path, access: int, directory: bool, *, allow_reparse: bool = False
    ) -> int:
        flags = _FILE_FLAG_OPEN_REPARSE_POINT
        if directory:
            flags |= _FILE_FLAG_BACKUP_SEMANTICS
        handle = _kernel32.CreateFileW(
            str(path),
            access,
            _FILE_SHARE_READ | _FILE_SHARE_WRITE,
            None,
            _OPEN_EXISTING,
            flags,
            None,
        )
        if handle == _INVALID_HANDLE_VALUE:
            _raise("cannot retain secure Windows handle")
        information = _information(handle)
        is_directory = bool(information.dwFileAttributes & _FILE_ATTRIBUTE_DIRECTORY)
        if (
            information.dwFileAttributes & _FILE_ATTRIBUTE_REPARSE_POINT
            and not allow_reparse
        ) or (not allow_reparse and is_directory != directory):
            _kernel32.CloseHandle(handle)
            raise SecureFilesystemError(
                "reparse point or wrong entry type is forbidden"
            )
        return handle

    def _information(handle: int) -> _FileInformation:
        information = _FileInformation()
        if not _kernel32.GetFileInformationByHandle(handle, ctypes.byref(information)):
            _raise("cannot query Windows file identity")
        return information

    def handle_identity(handle: int) -> tuple[int, int]:
        information = _information(handle)
        index = information.nFileIndexHigh << 32 | information.nFileIndexLow
        return information.dwVolumeSerialNumber, index

    def close_handle(handle: int) -> None:
        if not _kernel32.CloseHandle(handle):
            _raise("cannot close retained Windows handle")

    def identity(metadata: os.stat_result) -> tuple[int, int]:
        return metadata.st_dev, metadata.st_ino

    def open_directory(path: Path) -> int:
        candidate = Path(path)
        if not candidate.is_absolute():
            raise SecureFilesystemError("absolute Windows directory path required")
        current = _open(Path(candidate.anchor), _GENERIC_READ | _DELETE, True)
        built = Path(candidate.anchor)
        try:
            for part in candidate.parts[1:]:
                built /= part
                child = _open(built, _GENERIC_READ | _DELETE, True)
                _kernel32.CloseHandle(current)
                current = child
            result = current
            current = -1
            return result
        finally:
            if current not in (-1, None):
                _kernel32.CloseHandle(current)

    def _final_path(handle: int) -> Path:
        required = _kernel32.GetFinalPathNameByHandleW(handle, None, 0, 0)
        if not required:
            _raise("cannot resolve retained Windows handle")
        buffer = ctypes.create_unicode_buffer(required + 1)
        if not _kernel32.GetFinalPathNameByHandleW(handle, buffer, len(buffer), 0):
            _raise("cannot resolve retained Windows handle")
        text = buffer.value
        if text.startswith("\\\\?\\UNC\\"):
            text = "\\\\" + text[8:]
        elif text.startswith("\\\\?\\"):
            text = text[4:]
        return Path(text)

    def ancestor_identities(handle: int) -> frozenset[tuple[int, int]]:
        path = _final_path(handle)
        found: set[tuple[int, int]] = set()
        for candidate in (path, *path.parents):
            parent = open_directory(candidate)
            try:
                found.add(handle_identity(parent))
            finally:
                _kernel32.CloseHandle(parent)
        return frozenset(found)

    def _create_directory(path: Path) -> int:
        attributes, descriptor = _security_attributes()
        try:
            if not _kernel32.CreateDirectoryW(str(path), ctypes.byref(attributes)):
                _raise("cannot create protected Windows directory")
        finally:
            _kernel32.LocalFree(descriptor)
        if not _windows_path_has_private_acl(path):
            raise SecureFilesystemError("protected DACL verification failed")
        return open_directory(path)

    def _create_file(path: Path, payload: bytes) -> None:
        attributes, descriptor = _security_attributes()
        try:
            handle = _kernel32.CreateFileW(
                str(path),
                _GENERIC_WRITE | _DELETE,
                0,
                ctypes.byref(attributes),
                _CREATE_NEW,
                _FILE_FLAG_OPEN_REPARSE_POINT,
                None,
            )
        finally:
            _kernel32.LocalFree(descriptor)
        if handle == _INVALID_HANDLE_VALUE:
            _raise("cannot create protected Windows file")
        try:
            written = wintypes.DWORD()
            buffer = ctypes.create_string_buffer(payload)
            if not _kernel32.WriteFile(
                handle, buffer, len(payload), ctypes.byref(written), None
            ) or written.value != len(payload):
                _raise("short secure Windows write")
            if not _kernel32.FlushFileBuffers(handle):
                _raise("cannot flush secure Windows file")
        finally:
            _kernel32.CloseHandle(handle)

    def create_private_directory(path: Path) -> int:
        return _create_directory(path)

    def create_private_file_handle(path: Path, payload: bytes) -> int:
        _create_file(path, payload)
        return _open(path, _GENERIC_READ | _DELETE, False)

    def read_handle(handle: int, limit: int) -> bytes:
        if not _kernel32.SetFilePointer(handle, 0, None, 0):
            error = ctypes.get_last_error()
            if error:
                _raise("cannot rewind secure Windows file")
        buffer = ctypes.create_string_buffer(limit + 1)
        read = wintypes.DWORD()
        if not _kernel32.ReadFile(handle, buffer, limit + 1, ctypes.byref(read), None):
            _raise("cannot read secure Windows file")
        if read.value > limit:
            raise SecureFilesystemError("secure Windows file exceeds safety bound")
        return buffer.raw[: read.value]

    def _windows_path_has_private_acl(path: Path) -> bool:
        descriptor = wintypes.LPVOID()
        result = _advapi32.GetNamedSecurityInfoW(
            str(path),
            1,
            _DACL_SECURITY_INFORMATION,
            None,
            None,
            None,
            None,
            ctypes.byref(descriptor),
        )
        if result:
            return False
        text = wintypes.LPWSTR()
        try:
            if not _advapi32.ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                _SDDL_REVISION_1,
                _DACL_SECURITY_INFORMATION | _PROTECTED_DACL_SECURITY_INFORMATION,
                ctypes.byref(text),
                None,
            ):
                return False
            sddl = text.value or ""
            return (
                "D:P" in sddl
                and ";;;SY)" in sddl
                and f";;;{_USER_SID})" in sddl
                and all(broad not in sddl for broad in (";;;WD)", ";;;AU)", ";;;BU)"))
            )
        finally:
            if text:
                _kernel32.LocalFree(text)
            _kernel32.LocalFree(descriptor)

    def _mark_delete(handle: int) -> None:
        disposition = _Disposition(True)
        if not _kernel32.SetFileInformationByHandle(
            handle,
            _FILE_DISPOSITION_INFO,
            ctypes.byref(disposition),
            ctypes.sizeof(disposition),
        ):
            _raise("handle-bound Windows deletion failed")

    @dataclass
    class WindowsCellFilesystem:
        base_path: Path
        root_name: str
        base_fd: int
        root_fd: int
        base_identity: tuple[int, int]
        root_identity: tuple[int, int]
        directories: dict[str, int]
        directory_identities: dict[str, tuple[int, int]]
        marker_handle: int | None = None

        @classmethod
        def create(cls, base: Path, root_name: str) -> "WindowsCellFilesystem":
            base_fd = open_directory(base)
            root = base / root_name
            root_fd = _create_directory(root)
            directories: dict[str, int] = {}
            for name in ("home", "workspace", "cache", "temp", "logs", "promptfoo"):
                directories[name] = _create_directory(root / name)
            directories["cache/promptfoo"] = _create_directory(
                root / "cache" / "promptfoo"
            )
            return cls(
                base,
                root_name,
                base_fd,
                root_fd,
                handle_identity(base_fd),
                handle_identity(root_fd),
                directories,
                {name: handle_identity(fd) for name, fd in directories.items()},
            )

        def validate(self, *, cleanup: bool = False) -> None:
            if (
                handle_identity(self.base_fd) != self.base_identity
                or handle_identity(self.root_fd) != self.root_identity
            ):
                raise SecureFilesystemError("attempt root identity was substituted")
            for name, expected in self.directory_identities.items():
                if handle_identity(self.directories[name]) != expected:
                    raise SecureFilesystemError(f"required layout {name} changed")
            current = open_directory(self.base_path / self.root_name)
            try:
                if handle_identity(current) != self.root_identity:
                    raise SecureFilesystemError("attempt root identity was substituted")
            finally:
                _kernel32.CloseHandle(current)

        def base_is_still_bound(self) -> bool:
            try:
                current = open_directory(self.base_path)
            except SecureFilesystemError:
                return False
            try:
                return handle_identity(current) == self.base_identity
            finally:
                _kernel32.CloseHandle(current)

        def write_snapshot(
            self, files: Iterable[tuple[str, bytes]], target: str
        ) -> None:
            root = self.base_path / self.root_name / target
            retained: dict[Path, int] = {root: self.directories[target]}
            try:
                for relative, payload in sorted(files):
                    path = root / relative
                    current = root
                    for part in Path(relative).parts[:-1]:
                        current /= part
                        if current not in retained:
                            retained[current] = (
                                open_directory(current)
                                if current.exists()
                                else _create_directory(current)
                            )
                    _create_file(path, payload)
            finally:
                for path, handle in retained.items():
                    if path != root:
                        _kernel32.CloseHandle(handle)

        def create_file(self, name: str, payload: bytes) -> None:
            if self.marker_handle is not None:
                raise FileExistsError(name)
            self.marker_handle = create_private_file_handle(
                self.base_path / self.root_name / name, payload
            )

        def read_file(self, name: str, limit: int) -> bytes:
            if self.marker_handle is None:
                raise FileNotFoundError(name)
            return read_handle(self.marker_handle, limit)

        def scan_regular_identities(self) -> frozenset[tuple[int, int]]:
            identities: set[tuple[int, int]] = set()
            root = self.base_path / self.root_name
            for directory, names, files in os.walk(root, followlinks=False):
                for name in names:
                    if (Path(directory) / name).is_symlink():
                        raise SecureFilesystemError("cell contains a reparse point")
                for name in files:
                    path = Path(directory) / name
                    metadata = path.lstat()
                    if stat.S_ISLNK(metadata.st_mode) or metadata.st_nlink != 1:
                        raise SecureFilesystemError("cell contains a link")
                    identities.add(identity(metadata))
            return frozenset(identities)

        def delete_exact(self) -> None:
            self.validate()
            root = self.base_path / self.root_name
            if self.marker_handle is not None:
                _kernel32.CloseHandle(self.marker_handle)
                self.marker_handle = None
            retained = {
                root / Path(name): handle
                for name, handle in self.directories.items()
                if name != "cache/promptfoo"
            }
            retained[root / "cache" / "promptfoo"] = self.directories["cache/promptfoo"]
            entries = 0
            for directory, names, files in os.walk(
                root, topdown=False, followlinks=False
            ):
                for name in files:
                    entries += 1
                    if entries > _MAX_DELETE_ENTRIES:
                        raise SecureFilesystemError(
                            "cleanup reached its bounded progress limit; retry is required"
                        )
                    handle = _open(
                        Path(directory) / name,
                        _DELETE,
                        False,
                        allow_reparse=True,
                    )
                    _mark_delete(handle)
                    _kernel32.CloseHandle(handle)
                for name in names:
                    entries += 1
                    if entries > _MAX_DELETE_ENTRIES:
                        raise SecureFilesystemError(
                            "cleanup reached its bounded progress limit; retry is required"
                        )
                    path = Path(directory) / name
                    handle = retained.get(path)
                    if handle is None:
                        handle = _open(
                            path,
                            _DELETE,
                            not path.is_symlink(),
                            allow_reparse=path.is_symlink(),
                        )
                    _mark_delete(handle)
                    _kernel32.CloseHandle(handle)
            self.directories.clear()
            _mark_delete(self.root_fd)
            _kernel32.CloseHandle(self.root_fd)
            _kernel32.CloseHandle(self.base_fd)
            self.root_fd = -1
            self.base_fd = -1

        def close(self) -> None:
            handles = [*self.directories.values(), self.root_fd, self.base_fd]
            if self.marker_handle is not None:
                handles.append(self.marker_handle)
            for handle in handles:
                if handle not in (-1, None):
                    _kernel32.CloseHandle(handle)
