use std::fs;
use std::path::Path;

use crate::secure_fs::*;

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

#[test]
fn secure_fs_windows_ads_component_policy_rejects_colons() {
    assert!(crate::secure_fs::windows_ads_component_for_test(
        std::ffi::OsStr::new("proof:stream")
    ));
    assert!(!crate::secure_fs::windows_ads_component_for_test(
        std::ffi::OsStr::new("proof.json")
    ));
}

#[test]
fn secure_fs_modes_create_new_fsync_and_roundtrip() {
    let (_temp, root) = private_root();
    let directory = resolve_private_relative(&root, Path::new("proof")).unwrap();
    create_owner_only_dir_new(&directory).unwrap();
    let file = resolve_private_relative(&root, Path::new("proof/item.json")).unwrap();
    write_owner_only_new(&file, b"private").unwrap();

    assert_eq!(read_single_link_regular(&file).unwrap(), b"private");
    fsync_directory(&directory).unwrap();
    assert!(write_owner_only_new(&file, b"replacement").is_err());
    assert!(create_owner_only_dir_new(&directory).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn secure_fs_bounded_read_accepts_exact_cap_and_rejects_one_byte_over() {
    let (_temp, root) = private_root();
    let file = root.join("bounded.json");
    write_owner_only_new(&file, b"private").unwrap();

    assert_eq!(
        read_single_link_regular_bounded(&file, 7).unwrap(),
        b"private"
    );
    assert!(
        read_single_link_regular_bounded(&file, 6)
            .unwrap_err()
            .to_string()
            .contains("byte cap")
    );
}

#[cfg(unix)]
#[test]
fn secure_fs_rejects_escape_and_symlink_components() {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::fs::symlink;
    let (temp, root) = private_root();
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::set_permissions(&outside, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(outside.join("target"), b"outside").unwrap();
    fs::set_permissions(outside.join("target"), fs::Permissions::from_mode(0o600)).unwrap();
    symlink(&outside, root.join("linked-dir")).unwrap();
    symlink(outside.join("target"), root.join("linked-file")).unwrap();

    assert!(resolve_private_relative(&root, Path::new("../escape")).is_err());
    assert!(resolve_private_relative(&root, &root.join("absolute")).is_err());
    assert!(resolve_private_relative(&root, Path::new("linked-dir/child")).is_err());
    assert!(create_owner_only_dir_new(&root.join("linked-dir/child")).is_err());
    assert!(read_single_link_regular(&root.join("linked-file")).is_err());
    assert!(write_owner_only_new(&root.join("linked-file"), b"replace").is_err());
}

#[cfg(unix)]
#[test]
fn secure_fs_rejects_hardlinks() {
    let (_temp, root) = private_root();
    let file = root.join("single");
    write_owner_only_new(&file, b"private").unwrap();
    fs::hard_link(&file, root.join("second-link")).unwrap();

    assert!(read_single_link_regular(&file).is_err());
}

#[cfg(unix)]
#[test]
fn secure_fs_rejects_fifo_without_blocking() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::sync::mpsc;
    use std::time::Duration;

    let (_temp, root) = private_root();
    let fifo = root.join("fifo");
    let raw = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(raw.as_ptr(), 0o600) }, 0);
    let (send, receive) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        send.send(read_single_link_regular(&fifo).is_err()).unwrap();
    });

    let result = receive.recv_timeout(Duration::from_secs(1));
    let cleanup = unsafe { libc::open(raw.as_ptr(), libc::O_RDWR | libc::O_NONBLOCK) };
    assert!(cleanup >= 0);
    worker.join().unwrap();
    unsafe { libc::close(cleanup) };
    assert!(result.expect("FIFO inspection blocked"));
}

#[cfg(windows)]
mod windows_tests {
    use std::ffi::c_void;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use std::process::Command;

    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::Security::Authorization::*;
    use windows_sys::Win32::Security::*;
    use windows_sys::Win32::Storage::FileSystem::*;

    use super::*;

    fn icacls(path: &Path, args: &[&str]) {
        assert!(
            Command::new("icacls")
                .arg(path)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }

    fn dacl_mask(path: &Path) -> u32 {
        let mut options = std::fs::OpenOptions::new();
        let file = options
            .access_mode(READ_CONTROL)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .unwrap();
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
        assert_eq!(status, ERROR_SUCCESS);
        let mut ace: *mut c_void = std::ptr::null_mut();
        assert_ne!(unsafe { GetAce(dacl, 0, &mut ace) }, 0);
        let mask = unsafe { (*ace.cast::<ACCESS_ALLOWED_ACE>()).Mask };
        unsafe { LocalFree(descriptor as HLOCAL) };
        mask
    }

    #[test]
    fn secure_fs_windows_atomic_dacl_replaces_broad_acl_and_rejects_foreign_ace() {
        let temp = tempfile::tempdir().unwrap();
        icacls(temp.path(), &["/grant", "*S-1-1-0:(OI)(CI)(R)"]);
        let root = temp.path().join("private");
        create_owner_only_dir_new(&root).unwrap();
        let root = root.canonicalize().unwrap();
        assert_eq!(FILE_ALL_ACCESS, 0x001f_01ff);
        assert_eq!(dacl_mask(&root), FILE_ALL_ACCESS);
        fsync_directory(&root).unwrap();
        let file = root.join("proof.json");
        write_owner_only_new(&file, b"private").unwrap();
        assert_eq!(dacl_mask(&file), FILE_ALL_ACCESS);
        assert_eq!(read_single_link_regular(&file).unwrap(), b"private");

        icacls(&file, &["/grant", "*S-1-1-0:(R)"]);
        assert!(read_single_link_regular(&file).is_err());
    }

    #[test]
    fn secure_fs_windows_final_read_handle_shares_read_only() {
        let (_temp, root) = private_root();
        let file = root.join("proof.json");
        write_owner_only_new(&file, b"private").unwrap();
        let held = crate::secure_fs::windows_open_read_for_test(&file).unwrap();

        let mut reader = std::fs::OpenOptions::new();
        reader
            .access_mode(FILE_GENERIC_READ)
            .share_mode(FILE_SHARE_READ)
            .open(&file)
            .unwrap();
        let mut writer = std::fs::OpenOptions::new();
        assert!(
            writer
                .access_mode(FILE_GENERIC_WRITE)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .open(&file)
                .is_err()
        );
        assert!(fs::remove_file(&file).is_err());
        drop(held);
    }

    #[test]
    fn secure_fs_windows_rejects_ads_in_every_path_seam() {
        let (_temp, root) = private_root();
        assert!(resolve_private_relative(&root, Path::new("proof:stream")).is_err());
        assert!(create_owner_only_dir_new(&root.join("directory:stream")).is_err());
        assert!(write_owner_only_new(&root.join("new:stream"), b"hidden").is_err());

        let file = root.join("proof.json");
        write_owner_only_new(&file, b"private").unwrap();
        let stream = root.join("proof.json:stream");
        fs::write(&stream, b"hidden").unwrap();
        assert!(resolve_private_relative(&root, Path::new("proof.json:stream")).is_err());
        assert!(read_single_link_regular(&stream).is_err());
        assert!(fsync_directory(&stream).is_err());
    }

    #[test]
    fn secure_fs_windows_rejects_junction_components_and_hardlinks() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("private");
        let outside = temp.path().join("outside");
        create_owner_only_dir_new(&root).unwrap();
        create_owner_only_dir_new(&outside).unwrap();
        let junction = root.join("junction");
        assert!(
            Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&junction)
                .arg(&outside)
                .status()
                .unwrap()
                .success()
        );
        assert!(fsync_directory(&junction).is_err());
        assert!(write_owner_only_new(&junction.join("escaped"), b"no").is_err());

        let file = root.join("single");
        write_owner_only_new(&file, b"private").unwrap();
        fs::hard_link(&file, root.join("second-link")).unwrap();
        assert!(read_single_link_regular(&file).is_err());
    }
}
