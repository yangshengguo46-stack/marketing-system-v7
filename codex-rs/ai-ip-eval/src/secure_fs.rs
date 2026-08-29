use std::ffi::OsStr;
use std::fs::File;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;

fn windows_ads_component(name: &OsStr) -> bool {
    name.as_encoded_bytes().contains(&b':')
}

#[cfg(test)]
pub(crate) fn windows_ads_component_for_test(name: &OsStr) -> bool {
    windows_ads_component(name)
}

pub(crate) fn resolve_private_relative(root: &Path, relative: &Path) -> Result<PathBuf> {
    if !root.is_absolute() || root.canonicalize()? != root {
        bail!("private root must be an existing canonical absolute path");
    }
    validate_private_relative_path(relative)?;
    platform::validate_existing(root, EntryKind::Directory)?;
    let resolved = root.join(relative);
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    bail!("private path contains a link: {}", current.display());
                }
                platform::validate_existing(&current, EntryKind::Either)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error).context("inspect contained private path"),
        }
    }
    Ok(resolved)
}

pub(crate) fn validate_private_relative_path(relative: &Path) -> Result<()> {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || cfg!(windows)
            && relative.components().any(|component| {
                matches!(component, Component::Normal(name) if windows_ads_component(name))
            })
    {
        bail!("private path must be a nonempty normalized relative path");
    }
    Ok(())
}

pub(crate) fn create_owner_only_dir_new(path: &Path) -> Result<()> {
    platform::create_directory(path)
}
pub(crate) fn write_owner_only_new(path: &Path, bytes: &[u8]) -> Result<()> {
    platform::create_file(path, bytes)
}
pub(crate) fn create_owner_only_file_new_retained(path: &Path, bytes: &[u8]) -> Result<File> {
    platform::create_file_retained(path, bytes)
}
pub(crate) fn read_single_link_regular(path: &Path) -> Result<Vec<u8>> {
    platform::read_file(path)
}
pub(crate) fn read_single_link_regular_bounded(path: &Path, cap: u64) -> Result<Vec<u8>> {
    platform::read_file_bounded(path, cap)
}
pub(crate) fn fsync_directory(path: &Path) -> Result<()> {
    platform::sync_directory(path)
}
#[cfg(all(test, windows))]
pub(crate) use platform::open_read as windows_open_read_for_test;
#[cfg(windows)]
pub(crate) use platform::validate_directory_handle as validate_private_directory_handle;
#[cfg(windows)]
pub(crate) use platform::validate_file_handle as validate_private_file_handle;

#[derive(Clone, Copy)]
enum EntryKind {
    Either,
    Directory,
    File,
}

#[cfg(unix)]
mod platform {
    use std::ffi::CString;
    use std::io::Write;
    use std::os::fd::AsRawFd;
    use std::os::fd::FromRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::FileExt;
    use std::os::unix::fs::MetadataExt;

    use super::*;

    fn owned(fd: i32) -> Result<File> {
        if fd < 0 {
            return Err(std::io::Error::last_os_error()).context("open secure filesystem entry");
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    fn open_at(parent: &File, name: &CString, flags: i32) -> Result<File> {
        owned(unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) })
    }

    fn open(path: &Path, kind: EntryKind) -> Result<File> {
        if !path.is_absolute() {
            bail!("secure filesystem path must be absolute");
        }
        let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC;
        let mut current =
            owned(unsafe { libc::open(c"/".as_ptr(), flags) }).context("open filesystem root")?;
        let mut components = path.components().peekable();
        while let Some(component) = components.next() {
            match component {
                Component::RootDir => {}
                Component::Normal(name) => {
                    let final_component = components.peek().is_none();
                    let directory = !final_component || matches!(kind, EntryKind::Directory);
                    let name = CString::new(name.as_bytes())?;
                    let flags = libc::O_RDONLY
                        | libc::O_CLOEXEC
                        | libc::O_NOFOLLOW
                        | libc::O_NONBLOCK
                        | if directory { libc::O_DIRECTORY } else { 0 };
                    current = open_at(&current, &name, flags)
                        .with_context(|| format!("open no-follow path {}", path.display()))?;
                }
                _ => bail!("secure filesystem path is not normalized"),
            }
        }
        check(&current, kind)?;
        Ok(current)
    }

    fn check(file: &File, kind: EntryKind) -> Result<()> {
        let metadata = file.metadata()?;
        let type_matches = match kind {
            EntryKind::Either => metadata.is_file() || metadata.is_dir(),
            EntryKind::Directory => metadata.is_dir(),
            EntryKind::File => metadata.is_file(),
        };
        if !type_matches
            || (metadata.is_file() && metadata.nlink() != 1)
            || metadata.mode() & 0o077 != 0
        {
            bail!("private entry has unsafe type, links, or permissions");
        }
        Ok(())
    }

    pub(super) fn validate_existing(path: &Path, kind: EntryKind) -> Result<()> {
        open(path, kind).map(drop)
    }

    fn leaf(path: &Path) -> Result<CString> {
        let name = path.file_name().context("private path has no name")?;
        Ok(CString::new(name.as_bytes())?)
    }

    pub(super) fn create_directory(path: &Path) -> Result<()> {
        let parent_path = path.parent().context("private directory has no parent")?;
        let parent = open(parent_path, EntryKind::Directory)?;
        let name = leaf(path)?;
        if unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
            return Err(std::io::Error::last_os_error()).context("create private directory");
        }
        let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW;
        let created = open_at(&parent, &name, flags).context("open created private directory")?;
        if unsafe { libc::fchmod(created.as_raw_fd(), 0o700) } != 0 {
            return Err(std::io::Error::last_os_error()).context("chmod private directory");
        }
        check(&created, EntryKind::Directory)?;
        created.sync_all()?;
        parent.sync_all().context("fsync private directory parent")
    }

    pub(super) fn create_file(path: &Path, bytes: &[u8]) -> Result<()> {
        let parent_path = path.parent().context("private file has no parent")?;
        let parent = open(parent_path, EntryKind::Directory)?;
        let name = leaf(path)?;
        let mut created = owned(unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_WRONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_CREAT | libc::O_EXCL,
                0o600,
            )
        })
        .context("create private file")?;
        if unsafe { libc::fchmod(created.as_raw_fd(), 0o600) } != 0 {
            return Err(std::io::Error::last_os_error()).context("chmod private file");
        }
        created.write_all(bytes)?;
        created.sync_all()?;
        check(&created, EntryKind::File)?;
        let flags = libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK;
        let reopened = open_at(&parent, &name, flags).context("reopen created private file")?;
        check(&reopened, EntryKind::File)?;
        let created_identity = created.metadata()?;
        let reopened_identity = reopened.metadata()?;
        if (created_identity.dev(), created_identity.ino())
            != (reopened_identity.dev(), reopened_identity.ino())
        {
            bail!("private file identity changed after creation");
        }
        parent.sync_all().context("fsync private file parent")
    }

    pub(super) fn create_file_retained(path: &Path, bytes: &[u8]) -> Result<File> {
        let parent_path = path.parent().context("private file has no parent")?;
        let parent = open(parent_path, EntryKind::Directory)?;
        let name = leaf(path)?;
        let mut created = owned(unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDWR | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_CREAT | libc::O_EXCL,
                0o600,
            )
        })
        .context("create private file")?;
        if unsafe { libc::fchmod(created.as_raw_fd(), 0o600) } != 0 {
            return Err(std::io::Error::last_os_error()).context("chmod private file");
        }
        created.write_all(bytes)?;
        created.sync_all()?;
        check(&created, EntryKind::File)?;
        Ok(created)
    }

    pub(super) fn read_file(path: &Path) -> Result<Vec<u8>> {
        read_file_bounded(path, u64::MAX)
    }

    pub(super) fn read_file_bounded(path: &Path, cap: u64) -> Result<Vec<u8>> {
        let file = open(path, EntryKind::File)?;
        let before = file.metadata()?;
        if before.len() > cap {
            bail!("private file exceeds its byte cap");
        }
        let mut bytes = vec![0_u8; usize::try_from(before.len())?];
        file.read_exact_at(&mut bytes, 0)?;
        let mut eof_probe = [0_u8; 1];
        if file.read_at(&mut eof_probe, before.len())? != 0 {
            bail!("private file grew during bounded read");
        }
        let after = file.metadata()?;
        if before.dev() != after.dev()
            || before.ino() != after.ino()
            || before.len() != after.len()
            || after.nlink() != 1
        {
            bail!("private file changed during read");
        }
        Ok(bytes)
    }

    pub(super) fn sync_directory(path: &Path) -> Result<()> {
        let directory = open(path, EntryKind::Directory)?;
        directory.sync_all().context("fsync directory")
    }
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::io::Read;
    use std::io::Write;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::io::FromRawHandle;

    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::Security::Authorization::*;
    use windows_sys::Win32::Security::*;
    use windows_sys::Win32::Storage::FileSystem::*;
    use windows_sys::Win32::System::Threading::*;

    use super::*;

    const OPEN_FLAGS: u32 = FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS;

    fn validate_chain(path: &Path) -> Result<()> {
        if !path.is_absolute() {
            bail!("secure Windows filesystem path must be absolute");
        }
        let mut current = PathBuf::new();
        for component in path.components() {
            current.push(component.as_os_str());
            match component {
                Component::Prefix(_) | Component::RootDir => {}
                Component::Normal(name) => {
                    if windows_ads_component(name) {
                        bail!("Windows alternate data streams are forbidden");
                    }
                    open(&current, FILE_READ_ATTRIBUTES).with_context(|| {
                        format!("reject reparse component in {}", path.display())
                    })?;
                }
                _ => bail!("secure Windows filesystem path is not normalized"),
            }
        }
        Ok(())
    }

    fn open(path: &Path, access: u32) -> Result<File> {
        let mut options = std::fs::OpenOptions::new();
        options.access_mode(access).custom_flags(OPEN_FLAGS);
        let file = options.open(path)?;
        checked_info(&file)?;
        Ok(file)
    }

    pub(crate) fn open_read(path: &Path) -> Result<File> {
        let mut options = std::fs::OpenOptions::new();
        options.access_mode(FILE_GENERIC_READ | READ_CONTROL);
        options.share_mode(FILE_SHARE_READ).custom_flags(OPEN_FLAGS);
        Ok(options.open(path)?)
    }

    fn checked_info(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION> {
        let mut info = unsafe { std::mem::zeroed::<BY_HANDLE_FILE_INFORMATION>() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &mut info) } == 0
            || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0 && info.nNumberOfLinks != 1)
        {
            bail!("private Windows entry is a reparse point or hardlink");
        }
        Ok(info)
    }

    fn user_sid() -> Result<Vec<usize>> {
        let mut token: HANDLE = std::ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(std::io::Error::last_os_error()).context("open current process token");
        }
        let mut length = 0;
        unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut length) };
        let words = usize::try_from(length)?.div_ceil(std::mem::size_of::<usize>());
        let mut bytes = vec![0_usize; words];
        let buffer = bytes.as_mut_ptr().cast();
        let read = length != 0
            && unsafe { GetTokenInformation(token, TokenUser, buffer, length, &mut length) } != 0;
        unsafe { CloseHandle(token) };
        if !read {
            return Err(std::io::Error::last_os_error()).context("read current user SID");
        }
        Ok(bytes)
    }

    struct PrivateSecurity(SECURITY_DESCRIPTOR, Vec<usize>);

    impl PrivateSecurity {
        fn new() -> Result<Self> {
            let user = user_sid()?;
            let sid = unsafe { (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid };
            let acl_len = std::mem::size_of::<ACL>() + std::mem::size_of::<ACCESS_ALLOWED_ACE>()
                - std::mem::size_of::<u32>()
                + usize::try_from(unsafe { GetLengthSid(sid) })?;
            let word = std::mem::size_of::<usize>();
            let mut acl = vec![0_usize; acl_len.div_ceil(word)];
            let acl_ptr = acl.as_mut_ptr().cast::<ACL>();
            if unsafe { InitializeAcl(acl_ptr, u32::try_from(acl.len() * word)?, ACL_REVISION) }
                == 0
                || unsafe { AddAccessAllowedAce(acl_ptr, ACL_REVISION, FILE_ALL_ACCESS, sid) } == 0
            {
                return Err(std::io::Error::last_os_error()).context("build protected DACL");
            }
            let descriptor = SECURITY_DESCRIPTOR {
                Revision: 1,
                Sbz1: 0,
                Control: SE_DACL_PRESENT | SE_DACL_PROTECTED,
                Owner: std::ptr::null_mut(),
                Group: std::ptr::null_mut(),
                Sacl: std::ptr::null_mut(),
                Dacl: acl_ptr,
            };
            Ok(Self(descriptor, acl))
        }

        fn attributes(&mut self) -> SECURITY_ATTRIBUTES {
            let _keep_acl_alive = &self.1;
            SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: (&mut self.0 as *mut SECURITY_DESCRIPTOR).cast(),
                bInheritHandle: 0,
            }
        }
    }

    fn verify_dacl(file: &File) -> Result<()> {
        let user = user_sid()?;
        let sid = unsafe { (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid };
        let mut dacl = std::ptr::null_mut();
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        let status = unsafe {
            GetSecurityInfo(
                file.as_raw_handle() as HANDLE,
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut dacl,
                std::ptr::null_mut(),
                &mut descriptor,
            )
        };
        let valid = status == ERROR_SUCCESS
            && !dacl.is_null()
            && unsafe {
                (*(descriptor as *const SECURITY_DESCRIPTOR_RELATIVE)).Control & SE_DACL_PROTECTED
                    != 0
            }
            && unsafe { (*dacl).AceCount == 1 };
        let mut ace: *mut c_void = std::ptr::null_mut();
        let valid = valid
            && unsafe { GetAce(dacl, 0, &mut ace) } != 0
            && unsafe {
                let allowed = &*ace.cast::<ACCESS_ALLOWED_ACE>();
                allowed.Header.AceType == 0
                    && allowed.Header.AceFlags == 0
                    && EqualSid((&raw const allowed.SidStart).cast_mut().cast(), sid) != 0
                    && allowed.Mask == FILE_ALL_ACCESS
            };
        if !descriptor.is_null() {
            unsafe { LocalFree(descriptor as HLOCAL) };
        }
        if !valid {
            bail!("private Windows entry lacks an exact protected current-user DACL");
        }
        Ok(())
    }

    pub(super) fn validate_existing(path: &Path, kind: EntryKind) -> Result<()> {
        validate_chain(path)?;
        let file = open(path, FILE_GENERIC_READ | READ_CONTROL)?;
        let metadata = file.metadata()?;
        if matches!(kind, EntryKind::Directory) && !metadata.is_dir()
            || matches!(kind, EntryKind::File) && !metadata.is_file()
        {
            bail!("private Windows entry has the wrong type");
        }
        verify_dacl(&file)
    }

    pub(crate) fn validate_directory_handle(file: &File) -> Result<()> {
        let info = checked_info(file)?;
        if info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
            bail!("private Windows entry is not a directory");
        }
        verify_dacl(file)
    }

    pub(crate) fn validate_file_handle(file: &File) -> Result<()> {
        let info = checked_info(file)?;
        if info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
            bail!("private Windows entry is not a regular file");
        }
        verify_dacl(file)
    }

    fn create(path: &Path, directory: bool, share_mode: u32) -> Result<File> {
        if path.file_name().is_none_or(windows_ads_component) {
            bail!("Windows alternate data streams are forbidden");
        }
        let parent_path = path.parent().context("private path has no parent")?;
        validate_chain(parent_path)?;
        let parent = open(parent_path, FILE_READ_ATTRIBUTES)?;
        let mut security = PrivateSecurity::new()?;
        let attributes = security.attributes();
        let path_wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let created = if directory {
            if unsafe { CreateDirectoryW(path_wide.as_ptr(), &attributes) } == 0 {
                return Err(std::io::Error::last_os_error()).context("create private directory");
            }
            open(path, FILE_GENERIC_READ | FILE_GENERIC_WRITE | READ_CONTROL)?
        } else {
            let handle = unsafe {
                CreateFileW(
                    path_wide.as_ptr(),
                    FILE_GENERIC_READ | FILE_GENERIC_WRITE | READ_CONTROL,
                    share_mode,
                    &attributes,
                    CREATE_NEW,
                    FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
                    std::ptr::null_mut(),
                )
            };
            if handle == INVALID_HANDLE_VALUE {
                return Err(std::io::Error::last_os_error()).context("create private file");
            }
            unsafe { File::from_raw_handle(handle) }
        };
        checked_info(&created)?;
        verify_dacl(&created)?;
        let parent_after = open(parent_path, FILE_READ_ATTRIBUTES)?;
        let before = checked_info(&parent)?;
        let after = checked_info(&parent_after)?;
        if before.dwVolumeSerialNumber != after.dwVolumeSerialNumber
            || before.nFileIndexHigh != after.nFileIndexHigh
            || before.nFileIndexLow != after.nFileIndexLow
        {
            bail!("private parent identity changed during creation");
        }
        Ok(created)
    }

    pub(super) fn create_directory(path: &Path) -> Result<()> {
        let created = create(path, true, 0)?;
        created.sync_all().context("fsync private directory")
    }

    pub(super) fn create_file(path: &Path, bytes: &[u8]) -> Result<()> {
        let mut file = create(path, false, 0)?;
        file.write_all(bytes)?;
        file.sync_all().context("fsync private file")
    }

    pub(super) fn create_file_retained(path: &Path, bytes: &[u8]) -> Result<File> {
        let mut file = create(path, false, FILE_SHARE_READ | FILE_SHARE_DELETE)?;
        file.write_all(bytes)?;
        file.sync_all().context("fsync private file")?;
        Ok(file)
    }

    pub(super) fn read_file(path: &Path) -> Result<Vec<u8>> {
        read_file_bounded(path, u64::MAX)
    }

    pub(super) fn read_file_bounded(path: &Path, cap: u64) -> Result<Vec<u8>> {
        validate_chain(path)?;
        let mut file = open_read(path)?;
        let before = checked_info(&file)?;
        verify_dacl(&file)?;
        let accepted_size = u64::from(before.nFileSizeHigh) << 32 | u64::from(before.nFileSizeLow);
        if accepted_size > cap {
            bail!("private file exceeds its byte cap");
        }
        let mut bytes = vec![0_u8; usize::try_from(accepted_size)?];
        file.read_exact(&mut bytes)?;
        let mut eof_probe = [0_u8; 1];
        if file.read(&mut eof_probe)? != 0 {
            bail!("private file grew during bounded read");
        }
        let after = checked_info(&file)?;
        verify_dacl(&file)?;
        if before.dwVolumeSerialNumber != after.dwVolumeSerialNumber
            || before.nFileIndexHigh != after.nFileIndexHigh
            || before.nFileIndexLow != after.nFileIndexLow
            || before.nFileSizeHigh != after.nFileSizeHigh
            || before.nFileSizeLow != after.nFileSizeLow
        {
            bail!("private Windows file changed during read");
        }
        Ok(bytes)
    }

    pub(super) fn sync_directory(path: &Path) -> Result<()> {
        validate_chain(path)?;
        let file = open(path, FILE_GENERIC_READ | FILE_GENERIC_WRITE | READ_CONTROL)?;
        if !file.metadata()?.is_dir() {
            bail!("private Windows entry is not a directory");
        }
        verify_dacl(&file)?;
        file.sync_all().context("fsync private directory")
    }
}
