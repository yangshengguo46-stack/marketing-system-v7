use std::fs;

use pretty_assertions::assert_eq;

use crate::secure_fs::create_owner_only_dir_new;
use crate::secure_fs::write_owner_only_new;
use crate::secure_fs_retain::RetainedBoundedFile;
use crate::secure_fs_retain::RetainedLeafPermissions;

const REVIEW_CAP: u64 = 64 * 1024;

fn private_root() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("private");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    }
    #[cfg(windows)]
    create_owner_only_dir_new(&root).unwrap();
    (temp, root.canonicalize().unwrap())
}

fn review_file(root: &std::path::Path, bytes: &[u8]) -> std::path::PathBuf {
    let reviews = root.join("reviews");
    create_owner_only_dir_new(&reviews).unwrap();
    let file = reviews.join("reviewer-1.json");
    write_owner_only_new(&file, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    }
    file
}

fn retain_review(path: &std::path::Path, cap: u64) -> anyhow::Result<RetainedBoundedFile> {
    RetainedBoundedFile::retain(
        path,
        cap,
        RetainedLeafPermissions::AllowNonOwnerOnlyInsidePrivateDirectory,
    )
}

fn retain_review_error(path: &std::path::Path, cap: u64) -> anyhow::Error {
    retain_review(path, cap).err().unwrap()
}

#[test]
fn retained_bounded_file_accepts_a_readable_leaf_inside_an_owner_only_directory() {
    let (_temp, root) = private_root();
    let bytes = vec![b'r'; usize::try_from(REVIEW_CAP).unwrap()];
    let file = review_file(&root, &bytes);

    let retained = retain_review(&file, REVIEW_CAP).unwrap();

    assert_eq!(retained.raw_bytes(), bytes);
    retained.reverify_unchanged().unwrap();
}

#[test]
fn retained_bounded_file_rejects_one_byte_over_the_cap() {
    let (_temp, root) = private_root();
    let bytes = vec![b'r'; usize::try_from(REVIEW_CAP + 1).unwrap()];
    let file = review_file(&root, &bytes);

    let error = retain_review_error(&file, REVIEW_CAP);

    assert!(error.to_string().contains("byte cap"));
}

#[cfg(unix)]
#[test]
fn retained_bounded_file_owner_only_policy_does_not_waive_leaf_mode() {
    use std::os::unix::fs::PermissionsExt;

    let (_temp, root) = private_root();
    let file = review_file(&root, b"review");
    let error =
        RetainedBoundedFile::retain(&file, REVIEW_CAP, RetainedLeafPermissions::RequireOwnerOnly)
            .err()
            .unwrap();
    assert!(error.to_string().contains("permissions"));

    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    RetainedBoundedFile::retain(&file, REVIEW_CAP, RetainedLeafPermissions::RequireOwnerOnly)
        .unwrap();
}

#[test]
fn retained_bounded_file_accepts_an_empty_regular_leaf() {
    let (_temp, root) = private_root();
    let file = review_file(&root, b"");

    let retained = retain_review(&file, REVIEW_CAP).unwrap();

    assert_eq!(retained.raw_bytes(), b"");
    retained.reverify_unchanged().unwrap();
}

#[test]
fn retained_bounded_file_rejects_relative_and_noncanonical_parent_spellings() {
    let (_temp, root) = private_root();
    let file = review_file(&root, b"review");
    assert!(retain_review(std::path::Path::new("reviews/reviewer-1.json"), REVIEW_CAP).is_err());

    let dotted = root.join("reviews/./reviewer-1.json");
    assert!(retain_review(&dotted, REVIEW_CAP).is_err());

    assert!(retain_review(file.parent().unwrap(), REVIEW_CAP).is_err());
}

#[cfg(unix)]
#[test]
fn retained_bounded_file_rejects_hardlinks_and_symlinks() {
    use std::os::unix::fs::symlink;

    let (_temp, root) = private_root();
    let file = review_file(&root, b"review");
    let reviews = file.parent().unwrap();
    fs::hard_link(&file, reviews.join("hardlink.json")).unwrap();
    assert!(retain_review(&file, REVIEW_CAP).is_err());

    let symlink_path = reviews.join("symlink.json");
    symlink(&file, &symlink_path).unwrap();
    assert!(retain_review(&symlink_path, REVIEW_CAP).is_err());
}

#[cfg(unix)]
#[test]
fn retained_bounded_file_rejects_fifo_without_blocking() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::sync::mpsc;
    use std::time::Duration;

    let (_temp, root) = private_root();
    let reviews = root.join("reviews");
    create_owner_only_dir_new(&reviews).unwrap();
    let fifo = reviews.join("fifo.json");
    let raw = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(raw.as_ptr(), 0o644) }, 0);
    let (send, receive) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        send.send(retain_review(&fifo, REVIEW_CAP).is_err())
            .unwrap();
    });

    let result = receive.recv_timeout(Duration::from_secs(1));
    let cleanup = unsafe { libc::open(raw.as_ptr(), libc::O_RDWR | libc::O_NONBLOCK) };
    assert!(cleanup >= 0);
    worker.join().unwrap();
    unsafe { libc::close(cleanup) };
    assert!(result.expect("FIFO retained read blocked"));
}

#[cfg(unix)]
#[test]
fn retained_bounded_file_rejects_same_byte_path_replacement() {
    let (_temp, root) = private_root();
    let file = review_file(&root, b"review");
    let retained = retain_review(&file, REVIEW_CAP).unwrap();
    fs::rename(&file, file.with_extension("retained")).unwrap();
    fs::write(&file, b"review").unwrap();

    let error = retained.reverify_unchanged().unwrap_err();

    assert!(error.to_string().contains("identity changed"));
}

#[cfg(unix)]
#[test]
fn retained_bounded_file_rejects_in_place_byte_drift() {
    let (_temp, root) = private_root();
    let file = review_file(&root, b"review");
    let retained = retain_review(&file, REVIEW_CAP).unwrap();
    fs::write(&file, b"edited").unwrap();

    let error = retained.reverify_unchanged().unwrap_err();

    assert!(error.to_string().contains("bytes changed"));
}

#[cfg(unix)]
#[test]
fn retained_bounded_file_rejects_growth_past_the_cap_on_reverify() {
    let (_temp, root) = private_root();
    let bytes = vec![b'r'; usize::try_from(REVIEW_CAP).unwrap()];
    let file = review_file(&root, &bytes);
    let retained = retain_review(&file, REVIEW_CAP).unwrap();
    fs::write(&file, vec![b'r'; usize::try_from(REVIEW_CAP + 1).unwrap()]).unwrap();

    let error = retained.reverify_unchanged().unwrap_err();

    assert!(error.to_string().contains("byte cap"));
}

#[cfg(unix)]
#[test]
fn retained_bounded_file_rechecks_strict_leaf_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let (_temp, root) = private_root();
    let file = review_file(&root, b"review");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    let retained =
        RetainedBoundedFile::retain(&file, REVIEW_CAP, RetainedLeafPermissions::RequireOwnerOnly)
            .unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();

    let error = retained.reverify_unchanged().unwrap_err();

    assert!(error.to_string().contains("permissions"));
}

#[cfg(unix)]
#[test]
fn retained_bounded_file_rejects_parent_replacement() {
    let (_temp, root) = private_root();
    let file = review_file(&root, b"review");
    let retained = retain_review(&file, REVIEW_CAP).unwrap();
    let reviews = root.join("reviews");
    fs::rename(&reviews, root.join("retained-reviews")).unwrap();
    create_owner_only_dir_new(&reviews).unwrap();
    write_owner_only_new(&reviews.join("reviewer-1.json"), b"review").unwrap();

    let error = retained.reverify_unchanged().unwrap_err();

    assert!(error.to_string().contains("identity changed"));
}

#[cfg(unix)]
#[test]
fn retained_bounded_file_rejects_a_non_private_parent() {
    use std::os::unix::fs::PermissionsExt;

    let (_temp, root) = private_root();
    let file = review_file(&root, b"review");
    fs::set_permissions(file.parent().unwrap(), fs::Permissions::from_mode(0o755)).unwrap();

    let error = retain_review_error(&file, REVIEW_CAP);

    assert!(error.to_string().contains("owner-only directory"));
}
#[cfg(unix)]
#[test]
fn retained_bounded_file_rechecks_parent_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let (_temp, root) = private_root();
    let file = review_file(&root, b"review");
    let retained = retain_review(&file, REVIEW_CAP).unwrap();
    fs::set_permissions(file.parent().unwrap(), fs::Permissions::from_mode(0o755)).unwrap();

    let error = retained.reverify_unchanged().unwrap_err();

    assert!(error.to_string().contains("owner-only directory"));
}

#[cfg(windows)]
mod windows_tests {
    use std::os::windows::fs::OpenOptionsExt;
    use std::process::Command;

    use windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_WRITE;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_DELETE;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_WRITE;

    use super::*;

    fn icacls(path: &std::path::Path, args: &[&str]) {
        assert!(
            Command::new("icacls")
                .arg(path)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }

    #[test]
    fn retained_bounded_file_windows_never_waives_the_leaf_dacl() {
        let (_temp, root) = private_root();
        let file = review_file(&root, b"review");
        icacls(&file, &["/grant", "*S-1-1-0:(R)"]);

        assert!(retain_review(&file, REVIEW_CAP).is_err());
        assert!(
            RetainedBoundedFile::retain(
                &file,
                REVIEW_CAP,
                RetainedLeafPermissions::RequireOwnerOnly,
            )
            .is_err()
        );
    }

    #[test]
    fn retained_bounded_file_windows_rejects_a_broad_parent_dacl() {
        let (_temp, root) = private_root();
        let file = review_file(&root, b"review");

        icacls(file.parent().unwrap(), &["/grant", "*S-1-1-0:(R)"]);
        assert!(retain_review(&file, REVIEW_CAP).is_err());
    }

    #[test]
    fn retained_bounded_file_windows_rechecks_the_leaf_dacl() {
        let (_temp, root) = private_root();
        let file = review_file(&root, b"review");
        let retained = retain_review(&file, REVIEW_CAP).unwrap();
        icacls(&file, &["/grant", "*S-1-1-0:(R)"]);

        assert!(retained.reverify_unchanged().is_err());
    }

    #[test]
    fn retained_bounded_file_windows_rejects_hardlinks_and_named_streams() {
        let (_temp, root) = private_root();
        let file = review_file(&root, b"review");
        fs::hard_link(&file, file.with_extension("hardlink")).unwrap();
        assert!(retain_review(&file, REVIEW_CAP).is_err());

        let (_temp, root) = private_root();
        let file = review_file(&root, b"review");
        fs::write(file.with_file_name("reviewer-1.json:stream"), b"hidden").unwrap();
        assert!(retain_review(&file, REVIEW_CAP).is_err());
    }

    #[test]
    fn retained_bounded_file_windows_blocks_write_and_delete() {
        let (_temp, root) = private_root();
        let file = review_file(&root, b"review");
        let retained = retain_review(&file, REVIEW_CAP).unwrap();
        let mut writer = std::fs::OpenOptions::new();
        assert!(
            writer
                .access_mode(FILE_GENERIC_WRITE)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .open(&file)
                .is_err()
        );
        assert!(fs::remove_file(&file).is_err());
        drop(retained);
        fs::write(&file, b"edited").unwrap();
    }
}
