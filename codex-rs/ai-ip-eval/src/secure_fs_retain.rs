use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Result;
use anyhow::bail;

pub(crate) struct RetainedPrivateRoot {
    canonical_path: PathBuf,
    file: File,
    identity: Identity,
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
mod platform {
    use std::os::unix::fs::MetadataExt;

    use anyhow::Context;

    use super::*;

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
    use std::os::windows::fs::OpenOptionsExt;

    use super::*;
    use windows_sys::Win32::Foundation::READ_CONTROL;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
    use windows_sys::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_WRITE;

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
