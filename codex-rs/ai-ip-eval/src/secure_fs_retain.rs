use std::ffi::OsStr;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;

pub(crate) struct RetainedPrivateRoot {
    canonical_path: PathBuf,
    file: File,
    identity: Identity,
}

#[derive(Clone, Copy)]
pub(crate) enum RetainedLeafPermissions {
    RequireOwnerOnly,
    AllowNonOwnerOnlyInsidePrivateDirectory,
}

pub(crate) struct RetainedBoundedFile {
    parent: RetainedPrivateRoot,
    leaf_name: std::ffi::OsString,
    file: File,
    identity: FileIdentity,
    raw_bytes: Vec<u8>,
    cap: u64,
    permissions: RetainedLeafPermissions,
    open_mode: RetainedOpenMode,
}

#[derive(Clone, Copy)]
enum RetainedOpenMode {
    Snapshot,
    Publishable,
}

impl RetainedBoundedFile {
    pub(crate) fn retain(
        path: &Path,
        cap: u64,
        permissions: RetainedLeafPermissions,
    ) -> Result<Self> {
        Self::retain_with_mode(path, cap, permissions, RetainedOpenMode::Snapshot)
    }

    pub(crate) fn retain_publishable(
        path: &Path,
        cap: u64,
        permissions: RetainedLeafPermissions,
    ) -> Result<Self> {
        Self::retain_with_mode(path, cap, permissions, RetainedOpenMode::Publishable)
    }

    pub(crate) fn retain_created_with(
        path: &Path,
        file: File,
        cap: u64,
        permissions: RetainedLeafPermissions,
        before_retain: impl FnOnce() -> Result<()>,
    ) -> Result<Self> {
        let (parent_path, leaf_name) = retained_path_parts(path)?;
        let (identity, raw_bytes) = platform::snapshot(&file, cap, permissions)?;
        before_retain()?;
        let parent = RetainedPrivateRoot::retain(parent_path)?;
        let retained = Self {
            parent,
            leaf_name: leaf_name.to_os_string(),
            file,
            identity,
            raw_bytes,
            cap,
            permissions,
            open_mode: RetainedOpenMode::Publishable,
        };
        retained.reverify_unchanged()?;
        Ok(retained)
    }

    fn retain_with_mode(
        path: &Path,
        cap: u64,
        permissions: RetainedLeafPermissions,
        open_mode: RetainedOpenMode,
    ) -> Result<Self> {
        let (parent_path, leaf_name) = retained_path_parts(path)?;
        let parent = RetainedPrivateRoot::retain(parent_path)?;
        let (file, identity, raw_bytes) =
            platform::open_snapshot(&parent.file, path, leaf_name, cap, permissions, open_mode)?;
        parent.reverify_unchanged()?;
        Ok(Self {
            parent,
            leaf_name: leaf_name.to_os_string(),
            file,
            identity,
            raw_bytes,
            cap,
            permissions,
            open_mode,
        })
    }

    pub(crate) fn raw_bytes(&self) -> &[u8] {
        &self.raw_bytes
    }

    pub(crate) fn reverify_unchanged(&self) -> Result<()> {
        self.parent.reverify_unchanged()?;
        let (retained_identity, retained_bytes) =
            platform::snapshot(&self.file, self.cap, self.permissions)?;
        self.compare(retained_identity, &retained_bytes)?;
        let (current, current_identity, current_bytes) = platform::open_snapshot(
            &self.parent.file,
            &self.parent.canonical_path.join(&self.leaf_name),
            &self.leaf_name,
            self.cap,
            self.permissions,
            self.open_mode,
        )?;
        self.compare(current_identity, &current_bytes)?;
        drop(current);
        let (final_identity, final_bytes) =
            platform::snapshot(&self.file, self.cap, self.permissions)?;
        self.compare(final_identity, &final_bytes)?;
        self.parent.reverify_unchanged()
    }

    fn compare(&self, identity: FileIdentity, bytes: &[u8]) -> Result<()> {
        if identity != self.identity {
            bail!("retained private file identity changed");
        }
        if bytes != self.raw_bytes {
            bail!("retained private file bytes changed");
        }
        Ok(())
    }
}

fn retained_path_parts(path: &Path) -> Result<(&Path, &OsStr)> {
    if !path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
    {
        bail!("retained private file path must be normalized and absolute");
    }
    let parent_path = path.parent().context("retained file has no parent")?;
    let leaf_name = path
        .file_name()
        .context("retained file has no final component")?;
    if parent_path.join(leaf_name).as_os_str() != path.as_os_str() {
        bail!("retained private file path must use one exact final component");
    }
    crate::secure_fs::validate_private_relative_path(Path::new(leaf_name))?;
    Ok((parent_path, leaf_name))
}

impl RetainedPrivateRoot {
    pub(crate) fn retain(path: &Path) -> Result<Self> {
        let (file, identity) = platform::open_identity(path)?;
        let retained = Self {
            canonical_path: path.to_path_buf(),
            file,
            identity,
        };
        retained.reverify_unchanged()?;
        Ok(retained)
    }

    pub(crate) fn reverify_unchanged(&self) -> Result<()> {
        platform::validate_retained(&self.file)?;
        let (current, identity) = platform::open_identity(&self.canonical_path)?;
        platform::validate_retained(&current)?;
        if identity != self.identity {
            bail!("private root identity changed during blind bundle transaction");
        }
        Ok(())
    }
}

#[cfg(unix)]
type Identity = (u64, u64);
#[cfg(windows)]
type Identity = u128;
#[cfg(unix)]
type FileIdentity = (u64, u64);
#[cfg(windows)]
type FileIdentity = u128;

#[cfg(unix)]
mod platform {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::fd::FromRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::FileExt;
    use std::os::unix::fs::MetadataExt;

    use super::*;

    fn owned(fd: i32) -> Result<File> {
        if fd < 0 {
            return Err(std::io::Error::last_os_error()).context("open retained private file");
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    fn validate(file: &File, permissions: RetainedLeafPermissions) -> Result<std::fs::Metadata> {
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.nlink() != 1
            || matches!(permissions, RetainedLeafPermissions::RequireOwnerOnly)
                && metadata.mode() & 0o077 != 0
        {
            bail!("retained private file has unsafe type, links, or permissions");
        }
        Ok(metadata)
    }

    pub(super) fn open_snapshot(
        parent: &File,
        _path: &Path,
        leaf_name: &OsStr,
        cap: u64,
        permissions: RetainedLeafPermissions,
        _open_mode: RetainedOpenMode,
    ) -> Result<(File, FileIdentity, Vec<u8>)> {
        let leaf_name = CString::new(leaf_name.as_bytes())?;
        let flags = libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK;
        let file = owned(unsafe { libc::openat(parent.as_raw_fd(), leaf_name.as_ptr(), flags) })?;
        let (identity, bytes) = snapshot(&file, cap, permissions)?;
        Ok((file, identity, bytes))
    }

    pub(super) fn snapshot(
        file: &File,
        cap: u64,
        permissions: RetainedLeafPermissions,
    ) -> Result<(FileIdentity, Vec<u8>)> {
        let before = validate(file, permissions)?;
        if before.len() > cap {
            bail!("retained private file exceeds its byte cap");
        }
        let mut bytes = vec![0_u8; usize::try_from(before.len())?];
        file.read_exact_at(&mut bytes, 0)?;
        let mut eof_probe = [0_u8; 1];
        if file.read_at(&mut eof_probe, before.len())? != 0 {
            bail!("retained private file grew during bounded read");
        }
        let after = validate(file, permissions)?;
        let before_identity = (before.dev(), before.ino());
        if before_identity != (after.dev(), after.ino()) || before.len() != after.len() {
            bail!("retained private file changed during bounded read");
        }
        Ok((before_identity, bytes))
    }

    pub(super) fn open_identity(path: &Path) -> Result<(File, Identity)> {
        if !path.is_absolute() || path.canonicalize()?.as_os_str() != path.as_os_str() {
            bail!("private root must remain canonical during blind bundle transaction");
        }
        let file = File::open(path).context("retain private root directory")?;
        validate_retained(&file)?;
        let metadata = file.metadata()?;
        Ok((file, (metadata.dev(), metadata.ino())))
    }

    pub(super) fn validate_retained(file: &File) -> Result<()> {
        let metadata = file.metadata()?;
        if !metadata.is_dir() || metadata.mode() & 0o077 != 0 {
            bail!("retained private root is not an owner-only directory");
        }
        Ok(())
    }
}

#[cfg(windows)]
mod platform {
    use std::io::Read;
    use std::io::Seek;
    use std::io::SeekFrom;
    use std::os::windows::fs::OpenOptionsExt;

    use super::*;
    use windows_sys::Win32::Foundation::READ_CONTROL;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
    use windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_READ;
    use windows_sys::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_DELETE;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_WRITE;

    pub(super) fn open_snapshot(
        _parent: &File,
        path: &Path,
        _leaf_name: &OsStr,
        cap: u64,
        permissions: RetainedLeafPermissions,
        open_mode: RetainedOpenMode,
    ) -> Result<(File, FileIdentity, Vec<u8>)> {
        let share_mode = match open_mode {
            RetainedOpenMode::Snapshot => FILE_SHARE_READ,
            RetainedOpenMode::Publishable => FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        };
        let mut options = std::fs::OpenOptions::new();
        options
            .access_mode(FILE_GENERIC_READ | READ_CONTROL)
            .share_mode(share_mode)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        let file = options.open(path)?;
        let (identity, bytes) = snapshot(&file, cap, permissions)?;
        Ok((file, identity, bytes))
    }

    pub(super) fn snapshot(
        file: &File,
        cap: u64,
        _permissions: RetainedLeafPermissions,
    ) -> Result<(FileIdentity, Vec<u8>)> {
        let before = crate::runner::private_existing_windows::info(file, false)?;
        crate::secure_fs::validate_private_file_handle(file)?;
        let accepted_size = u64::from(before.nFileSizeHigh) << 32 | u64::from(before.nFileSizeLow);
        if accepted_size > cap {
            bail!("retained private file exceeds its byte cap");
        }
        let mut reader = file.try_clone()?;
        reader.seek(SeekFrom::Start(0))?;
        let mut bytes = vec![0_u8; usize::try_from(accepted_size)?];
        reader.read_exact(&mut bytes)?;
        let mut eof_probe = [0_u8; 1];
        if reader.read(&mut eof_probe)? != 0 {
            bail!("retained private file grew during bounded read");
        }
        let after = crate::runner::private_existing_windows::info(file, false)?;
        crate::secure_fs::validate_private_file_handle(file)?;
        let identity = crate::runner::private_existing_windows::identity(&before);
        if identity != crate::runner::private_existing_windows::identity(&after)
            || before.nFileSizeHigh != after.nFileSizeHigh
            || before.nFileSizeLow != after.nFileSizeLow
        {
            bail!("retained private file changed during bounded read");
        }
        Ok((identity, bytes))
    }

    pub(super) fn open_identity(path: &Path) -> Result<(File, Identity)> {
        if !path.is_absolute() || path.canonicalize()?.as_os_str() != path.as_os_str() {
            bail!("private root must remain canonical during blind bundle transaction");
        }
        let mut options = std::fs::OpenOptions::new();
        options
            .access_mode(FILE_READ_ATTRIBUTES | READ_CONTROL)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS);
        let file = options.open(path)?;
        crate::secure_fs::validate_private_directory_handle(&file)?;
        let info = crate::runner::private_existing_windows::info(&file, true)?;
        Ok((
            file,
            crate::runner::private_existing_windows::identity(&info),
        ))
    }

    pub(super) fn validate_retained(file: &File) -> Result<()> {
        crate::secure_fs::validate_private_directory_handle(file)
    }
}
