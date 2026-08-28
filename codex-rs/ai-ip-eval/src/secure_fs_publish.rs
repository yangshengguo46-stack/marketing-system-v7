use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;

use crate::secure_fs::resolve_private_relative;
use crate::secure_fs::validate_private_relative_path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PublishCheckpoint {
    IdentitiesCaptured,
    ParentOpened,
    SourceOpened,
}

pub(crate) fn publish_private_tree_no_replace(
    private_root: &Path,
    staging_relative: &Path,
    final_relative: &Path,
) -> Result<()> {
    publish_private_tree_no_replace_with_hook(
        private_root,
        staging_relative,
        final_relative,
        &mut |_| Ok(()),
    )
}

#[cfg(test)]
pub(crate) fn publish_private_tree_no_replace_for_test(
    private_root: &Path,
    staging_relative: &Path,
    final_relative: &Path,
    hook: &mut dyn FnMut(PublishCheckpoint) -> Result<()>,
) -> Result<()> {
    publish_private_tree_no_replace_with_hook(private_root, staging_relative, final_relative, hook)
}

fn publish_private_tree_no_replace_with_hook(
    private_root: &Path,
    staging_relative: &Path,
    final_relative: &Path,
    hook: &mut dyn FnMut(PublishCheckpoint) -> Result<()>,
) -> Result<()> {
    if staging_relative.parent() != final_relative.parent() {
        bail!("private tree publication requires the same private parent");
    }
    let staging = resolve_private_relative(private_root, staging_relative)?;
    validate_private_relative_path(final_relative)?;
    let unchecked_destination = private_root.join(final_relative);
    match std::fs::symlink_metadata(&unchecked_destination) {
        Ok(_) => bail!("private publication destination already exists"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("inspect private publication destination"),
    }
    let destination = resolve_private_relative(private_root, final_relative)?;
    let expected = platform::capture_identities(&staging)?;
    hook(PublishCheckpoint::IdentitiesCaptured)?;
    platform::publish(&staging, &destination, expected, hook)
}

#[cfg(unix)]
mod platform {
    use std::ffi::CString;
    use std::fs::File;
    use std::os::fd::AsRawFd;
    use std::os::fd::FromRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;

    use super::*;

    pub(super) struct ExpectedIdentities {
        parent: (u64, u64),
        source: (u64, u64),
    }

    fn owned(fd: i32) -> Result<File> {
        if fd < 0 {
            return Err(std::io::Error::last_os_error()).context("open private publish entry");
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    fn open_directory(path: &Path) -> Result<File> {
        if !path.is_absolute() {
            bail!("private publish path must be absolute");
        }
        let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW;
        let mut current = owned(unsafe { libc::open(c"/".as_ptr(), flags) })?;
        for component in path.components() {
            match component {
                std::path::Component::RootDir => {}
                std::path::Component::Normal(name) => {
                    let name = CString::new(name.as_bytes())?;
                    current =
                        owned(unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(), flags) })
                            .with_context(|| {
                            format!("open no-follow private publish path {}", path.display())
                        })?;
                }
                _ => bail!("private publish path is not normalized"),
            }
        }
        let metadata = current.metadata()?;
        if !metadata.is_dir() || metadata.mode() & 0o077 != 0 {
            bail!("private publish directory has unsafe type or permissions");
        }
        Ok(current)
    }

    fn leaf(path: &Path) -> Result<CString> {
        Ok(CString::new(
            path.file_name()
                .context("private publish path has no final component")?
                .as_bytes(),
        )?)
    }

    fn same_identity(left: &File, right: &File) -> Result<bool> {
        Ok(identity(left)? == identity(right)?)
    }

    fn identity(file: &File) -> Result<(u64, u64)> {
        let metadata = file.metadata()?;
        Ok((metadata.dev(), metadata.ino()))
    }

    fn open_child_directory(parent: &File, name: &CString) -> Result<File> {
        let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW;
        let child = owned(unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) })?;
        let metadata = child.metadata()?;
        if !metadata.is_dir() || metadata.mode() & 0o077 != 0 {
            bail!("private staging entry has unsafe type or permissions");
        }
        Ok(child)
    }

    pub(super) fn capture_identities(staging: &Path) -> Result<ExpectedIdentities> {
        let parent_path = staging
            .parent()
            .context("private staging entry has no parent")?;
        let parent = open_directory(parent_path)?;
        let source = open_child_directory(&parent, &leaf(staging)?)
            .context("capture private staging entry identity")?;
        Ok(ExpectedIdentities {
            parent: identity(&parent)?,
            source: identity(&source)?,
        })
    }

    pub(super) fn publish(
        staging: &Path,
        destination: &Path,
        expected: ExpectedIdentities,
        hook: &mut dyn FnMut(PublishCheckpoint) -> Result<()>,
    ) -> Result<()> {
        let parent_path = staging
            .parent()
            .context("private staging entry has no parent")?;
        if destination.parent() != Some(parent_path) {
            bail!("private tree publication requires the same private parent");
        }
        let parent = open_directory(parent_path)?;
        if identity(&parent)? != expected.parent {
            bail!("private publish parent identity changed before publication");
        }
        hook(PublishCheckpoint::ParentOpened)?;
        let before = open_directory(parent_path)?;
        if !same_identity(&parent, &before)? {
            bail!("private publish parent identity changed before publication");
        }
        let staging_name = leaf(staging)?;
        let destination_name = leaf(destination)?;
        let retained_staging = open_child_directory(&parent, &staging_name)
            .context("inspect private staging entry before publication")?;
        if identity(&retained_staging)? != expected.source {
            bail!("private staging identity changed before publication");
        }
        retained_staging
            .sync_all()
            .context("fsync private staging tree")?;
        hook(PublishCheckpoint::SourceOpened)?;
        rename_no_replace(&parent, &staging_name, &destination_name)?;
        let published = open_child_directory(&parent, &destination_name)?;
        if !same_identity(&retained_staging, &published)? {
            bail!("private staging identity changed during publication");
        }
        let after = open_directory(parent_path)?;
        if !same_identity(&parent, &after)? {
            bail!("private publish parent identity changed during publication");
        }
        published
            .sync_all()
            .context("fsync published private tree")?;
        parent.sync_all().context("fsync private publish parent")
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    fn rename_no_replace(parent: &File, source: &CString, destination: &CString) -> Result<()> {
        let result = unsafe {
            libc::renameatx_np(
                parent.as_raw_fd(),
                source.as_ptr(),
                parent.as_raw_fd(),
                destination.as_ptr(),
                libc::RENAME_EXCL,
            )
        };
        rename_result(result)
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn rename_no_replace(parent: &File, source: &CString, destination: &CString) -> Result<()> {
        let result = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                parent.as_raw_fd(),
                source.as_ptr(),
                parent.as_raw_fd(),
                destination.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        rename_result(i32::try_from(result).unwrap_or(-1))
    }

    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "linux",
        target_os = "android"
    )))]
    fn rename_no_replace(_parent: &File, _source: &CString, _destination: &CString) -> Result<()> {
        bail!("no-replace private publication is unsupported on this platform")
    }

    fn rename_result(result: i32) -> Result<()> {
        if result == 0 {
            return Ok(());
        }
        rename_error(std::io::Error::last_os_error())
    }

    fn rename_error(error: std::io::Error) -> Result<()> {
        match error.raw_os_error() {
            Some(code) if code == libc::EEXIST || code == libc::ENOTEMPTY => {
                bail!("private publication destination already exists")
            }
            Some(libc::ENOSYS) => {
                bail!("no-replace private publication is unsupported on this platform")
            }
            _ => Err(error).context("publish private tree without replacement"),
        }
    }

    #[cfg(all(test, any(target_os = "linux", target_os = "android")))]
    pub(super) fn rename_error_for_test(code: i32) -> Result<()> {
        rename_error(std::io::Error::from_raw_os_error(code))
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "android")))]
pub(crate) fn linux_rename_error_for_test(code: i32) -> Result<()> {
    platform::rename_error_for_test(code)
}

#[cfg(windows)]
mod platform {
    use std::fs::File;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;

    use super::*;
    use windows_sys::Win32::Foundation::ERROR_ALREADY_EXISTS;
    use windows_sys::Win32::Foundation::ERROR_FILE_EXISTS;
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::*;

    pub(super) struct ExpectedIdentities {
        parent: (u32, u32, u32),
        source: (u32, u32, u32),
    }

    fn open(path: &Path, access: u32, share: u32) -> Result<File> {
        let mut options = std::fs::OpenOptions::new();
        options
            .access_mode(access)
            .share_mode(share)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
            .with_context(|| format!("open private publish path {}", path.display()))
    }

    fn open_retained(path: &Path, access: u32) -> Result<File> {
        open(path, access, FILE_SHARE_READ | FILE_SHARE_WRITE)
    }

    fn open_observer(path: &Path, access: u32) -> Result<File> {
        open(
            path,
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        )
    }

    fn identity(file: &File) -> Result<(u32, u32, u32)> {
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        if unsafe { GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &mut info) } == 0 {
            return Err(std::io::Error::last_os_error())
                .context("inspect private publish identity");
        }
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            bail!("private publish path contains a reparse point");
        }
        Ok((
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
        ))
    }

    pub(super) fn capture_identities(staging: &Path) -> Result<ExpectedIdentities> {
        let parent_path = staging
            .parent()
            .context("private staging entry has no parent")?;
        let parent = open_retained(
            parent_path,
            FILE_GENERIC_READ | FILE_GENERIC_WRITE | READ_CONTROL,
        )?;
        crate::secure_fs::validate_private_directory_handle(&parent)?;
        let source = open_retained(
            staging,
            DELETE | FILE_READ_ATTRIBUTES | SYNCHRONIZE | READ_CONTROL,
        )?;
        crate::secure_fs::validate_private_directory_handle(&source)?;
        let parent_after = open_observer(parent_path, FILE_READ_ATTRIBUTES | READ_CONTROL)?;
        crate::secure_fs::validate_private_directory_handle(&parent_after)?;
        let expected = ExpectedIdentities {
            parent: identity(&parent)?,
            source: identity(&source)?,
        };
        if identity(&parent_after)? != expected.parent {
            bail!("private publish parent identity changed while capturing source");
        }
        Ok(expected)
    }

    pub(super) fn publish(
        staging: &Path,
        destination: &Path,
        expected: ExpectedIdentities,
        hook: &mut dyn FnMut(PublishCheckpoint) -> Result<()>,
    ) -> Result<()> {
        let parent_path = staging
            .parent()
            .context("private staging entry has no parent")?;
        if destination.parent() != Some(parent_path) {
            bail!("private tree publication requires the same private parent");
        }
        let parent = open_retained(
            parent_path,
            FILE_GENERIC_READ | FILE_GENERIC_WRITE | READ_CONTROL,
        )?;
        crate::secure_fs::validate_private_directory_handle(&parent)?;
        let retained_parent = identity(&parent)?;
        if retained_parent != expected.parent {
            bail!("private publish parent identity changed before publication");
        }
        hook(PublishCheckpoint::ParentOpened)?;
        let staging_file = open_retained(
            staging,
            DELETE | FILE_READ_ATTRIBUTES | SYNCHRONIZE | READ_CONTROL,
        )?;
        crate::secure_fs::validate_private_directory_handle(&staging_file)?;
        let retained_staging = identity(&staging_file)?;
        if retained_staging != expected.source {
            bail!("private staging identity changed before publication");
        }
        let parent_before = open_retained(parent_path, FILE_READ_ATTRIBUTES | READ_CONTROL)?;
        crate::secure_fs::validate_private_directory_handle(&parent_before)?;
        if identity(&parent_before)? != retained_parent {
            bail!("private publish parent identity changed before publication");
        }
        hook(PublishCheckpoint::SourceOpened)?;
        let destination_name = destination
            .file_name()
            .context("private publication destination has no name")?
            .encode_wide()
            .collect::<Vec<_>>();
        let header_size = size_of::<FILE_RENAME_INFO>() - size_of::<u16>();
        let byte_size = header_size + destination_name.len() * size_of::<u16>();
        let word_size = size_of::<usize>();
        let mut storage = vec![0_usize; byte_size.div_ceil(word_size)];
        let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        unsafe {
            (*info).Anonymous.ReplaceIfExists = false;
            (*info).RootDirectory = parent.as_raw_handle() as HANDLE;
            (*info).FileNameLength = u32::try_from(destination_name.len() * size_of::<u16>())?;
            std::ptr::copy_nonoverlapping(
                destination_name.as_ptr(),
                (*info).FileName.as_mut_ptr(),
                destination_name.len(),
            );
        }
        let renamed = unsafe {
            SetFileInformationByHandle(
                staging_file.as_raw_handle() as HANDLE,
                FileRenameInfo,
                info.cast(),
                u32::try_from(byte_size)?,
            )
        };
        if renamed == 0 {
            let code = unsafe { GetLastError() };
            if code == ERROR_ALREADY_EXISTS || code == ERROR_FILE_EXISTS {
                bail!("private publication destination already exists");
            }
            return Err(std::io::Error::last_os_error())
                .context("publish private tree without replacement");
        }
        let published = open_observer(
            destination,
            FILE_GENERIC_READ | FILE_GENERIC_WRITE | READ_CONTROL,
        )?;
        crate::secure_fs::validate_private_directory_handle(&published)?;
        if identity(&published)? != retained_staging {
            bail!("private staging identity changed during publication");
        }
        let parent_after = open_retained(parent_path, FILE_GENERIC_READ | READ_CONTROL)?;
        crate::secure_fs::validate_private_directory_handle(&parent_after)?;
        if identity(&parent_after)? != retained_parent {
            bail!("private publish parent identity changed during publication");
        }
        published
            .sync_all()
            .context("fsync published private tree")?;
        parent.sync_all().context("fsync private publish parent")
    }
}
