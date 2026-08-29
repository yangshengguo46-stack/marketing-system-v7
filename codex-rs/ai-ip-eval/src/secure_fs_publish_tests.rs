use std::fs;
use std::path::Path;

use pretty_assertions::assert_eq;

use crate::secure_fs::create_owner_only_dir_new;
use crate::secure_fs::read_single_link_regular;
use crate::secure_fs::write_owner_only_new;
use crate::secure_fs_publish::EntryKind;
use crate::secure_fs_publish::PublishCheckpoint;
use crate::secure_fs_publish::publish_entry_with_hook;
use crate::secure_fs_publish::publish_private_file_no_replace;
use crate::secure_fs_publish::publish_private_tree_no_replace;

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

fn staged_tree(root: &Path, relative: &str, bytes: &[u8]) {
    let directory = root.join(relative);
    create_owner_only_dir_new(&directory).unwrap();
    write_owner_only_new(&directory.join("bound.json"), bytes).unwrap();
}

fn publish_standard(root: &Path) -> anyhow::Result<()> {
    publish_private_tree_no_replace(
        root,
        Path::new("coordinator/.staging"),
        Path::new("coordinator/final"),
    )
}

fn staged_file(root: &Path, relative: &str, bytes: &[u8]) {
    write_owner_only_new(&root.join(relative), bytes).unwrap();
}

fn publish_file_standard(root: &Path) -> anyhow::Result<()> {
    publish_private_file_no_replace(
        root,
        Path::new("coordinator/.decision.staging"),
        Path::new("coordinator/decision.json"),
    )
}

#[test]
fn secure_publish_moves_one_durable_sibling_without_replacement() {
    let (_temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    staged_tree(&root, "coordinator/.staging", b"first");

    publish_standard(&root).unwrap();
    assert!(!root.join("coordinator/.staging").exists());
    assert_eq!(
        read_single_link_regular(&root.join("coordinator/final/bound.json")).unwrap(),
        b"first"
    );

    staged_tree(&root, "coordinator/.staging", b"replacement");
    let error = publish_standard(&root).unwrap_err();
    assert!(error.to_string().contains("already exists"));
    assert_eq!(
        read_single_link_regular(&root.join("coordinator/final/bound.json")).unwrap(),
        b"first"
    );
    assert_eq!(
        read_single_link_regular(&root.join("coordinator/.staging/bound.json")).unwrap(),
        b"replacement"
    );
}

#[test]
fn secure_publish_rejects_cross_parent_and_missing_sources() {
    let (_temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    staged_tree(&root, ".staging", b"private");

    let error = publish_private_tree_no_replace(
        &root,
        Path::new(".staging"),
        Path::new("coordinator/final"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("same private parent"));
    assert!(root.join(".staging").is_dir());
    assert!(!root.join("coordinator/final").exists());

    let error = publish_private_tree_no_replace(
        &root,
        Path::new("coordinator/missing"),
        Path::new("coordinator/final"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("staging entry"));
}

#[cfg(unix)]
#[test]
fn secure_publish_rejects_link_staging_entries() {
    use std::os::unix::fs::symlink;

    let (temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, root.join("coordinator/.staging")).unwrap();

    assert!(publish_standard(&root).is_err());
    assert!(!root.join("coordinator/final").exists());
}

#[cfg(unix)]
#[test]
fn secure_publish_rejects_unsafe_modes_and_existing_destination_links() {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::fs::symlink;

    let (temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    staged_tree(&root, "coordinator/.staging", b"private");
    let staging = root.join("coordinator/.staging");
    fs::set_permissions(&staging, fs::Permissions::from_mode(0o755)).unwrap();

    let error = publish_standard(&root).unwrap_err();
    assert!(error.to_string().contains("unsafe"));
    assert!(!root.join("coordinator/final").exists());

    fs::set_permissions(&staging, fs::Permissions::from_mode(0o700)).unwrap();
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, root.join("coordinator/final")).unwrap();
    let error = publish_standard(&root).unwrap_err();
    assert!(error.to_string().contains("already exists"));
    assert!(staging.is_dir());
}

#[cfg(unix)]
#[test]
fn secure_publish_rejects_a_parent_identity_change_before_rename() {
    let (_temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    staged_tree(&root, "coordinator/.staging", b"private");
    let mut changed = false;

    let error = publish_entry_with_hook(
        &root,
        Path::new("coordinator/.staging"),
        Path::new("coordinator/final"),
        EntryKind::Tree,
        &mut |checkpoint| {
            if checkpoint == PublishCheckpoint::ParentOpened {
                fs::rename(root.join("coordinator"), root.join("retained-parent"))?;
                create_owner_only_dir_new(&root.join("coordinator"))?;
                changed = true;
            }
            Ok(())
        },
    )
    .unwrap_err();

    assert!(changed);
    assert!(error.to_string().contains("parent identity changed"));
    assert!(root.join("retained-parent/.staging").is_dir());
    assert!(!root.join("coordinator/final").exists());
}

#[cfg(unix)]
#[test]
fn secure_publish_rejects_source_replacement_after_initial_validation() {
    let (_temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    staged_tree(&root, "coordinator/.staging", b"original");
    let mut changed = false;

    let error = publish_entry_with_hook(
        &root,
        Path::new("coordinator/.staging"),
        Path::new("coordinator/final"),
        EntryKind::Tree,
        &mut |checkpoint| {
            if checkpoint == PublishCheckpoint::IdentitiesCaptured {
                fs::rename(
                    root.join("coordinator/.staging"),
                    root.join("coordinator/retained-source"),
                )?;
                staged_tree(&root, "coordinator/.staging", b"replacement");
                changed = true;
            }
            Ok(())
        },
    )
    .unwrap_err();

    assert!(changed);
    assert!(error.to_string().contains("staging identity changed"));
    assert!(!root.join("coordinator/final").exists());
    assert_eq!(
        read_single_link_regular(&root.join("coordinator/retained-source/bound.json")).unwrap(),
        b"original"
    );
    assert_eq!(
        read_single_link_regular(&root.join("coordinator/.staging/bound.json")).unwrap(),
        b"replacement"
    );
}

#[cfg(unix)]
#[test]
fn secure_publish_detects_a_source_name_replacement_at_the_rename_boundary() {
    let (_temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    staged_tree(&root, "coordinator/.staging", b"original");
    let mut changed = false;

    let error = publish_entry_with_hook(
        &root,
        Path::new("coordinator/.staging"),
        Path::new("coordinator/final"),
        EntryKind::Tree,
        &mut |checkpoint| {
            if checkpoint == PublishCheckpoint::SourceOpened {
                fs::rename(
                    root.join("coordinator/.staging"),
                    root.join("coordinator/retained-source"),
                )?;
                staged_tree(&root, "coordinator/.staging", b"replacement");
                changed = true;
            }
            Ok(())
        },
    )
    .unwrap_err();

    assert!(changed);
    assert!(error.to_string().contains("staging identity changed"));
    assert_eq!(
        read_single_link_regular(&root.join("coordinator/retained-source/bound.json")).unwrap(),
        b"original"
    );
    assert_eq!(
        read_single_link_regular(&root.join("coordinator/final/bound.json")).unwrap(),
        b"replacement"
    );
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn secure_publish_linux_rename_enosys_fails_closed() {
    let error = crate::secure_fs_publish::linux_rename_error_for_test(libc::ENOSYS).unwrap_err();
    assert!(error.to_string().contains("unsupported"));
}

#[cfg(windows)]
#[test]
fn secure_publish_windows_rejects_source_and_destination_junctions() {
    use std::process::Command;

    let (temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    let outside = temp.path().join("outside");
    create_owner_only_dir_new(&outside).unwrap();
    let junction = |link: &Path, target: &Path| {
        Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .status()
            .unwrap()
            .success()
    };
    let staging = root.join("coordinator/.staging");
    assert!(junction(&staging, &outside));
    assert!(publish_standard(&root).is_err());
    fs::remove_dir(&staging).unwrap();
    staged_tree(&root, "coordinator/.staging", b"private");
    let destination = root.join("coordinator/final");
    assert!(junction(&destination, &outside));
    assert!(publish_standard(&root).is_err());
    assert!(staging.is_dir());
}

#[test]
fn secure_file_publish_moves_one_durable_file_without_replacement() {
    let (_temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    staged_file(&root, "coordinator/.decision.staging", b"first decision");
    #[cfg(unix)]
    let staging_inode = {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(root.join("coordinator/.decision.staging"))
            .unwrap()
            .ino()
    };
    #[cfg(windows)]
    let staging_identity = {
        let file = fs::File::open(root.join("coordinator/.decision.staging")).unwrap();
        let info = crate::runner::private_existing_windows::info(&file, false).unwrap();
        crate::runner::private_existing_windows::identity(&info)
    };

    publish_file_standard(&root).unwrap();

    assert!(!root.join("coordinator/.decision.staging").exists());
    assert_eq!(
        read_single_link_regular(&root.join("coordinator/decision.json")).unwrap(),
        b"first decision"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            fs::metadata(root.join("coordinator/decision.json"))
                .unwrap()
                .ino(),
            staging_inode
        );
    }
    #[cfg(windows)]
    {
        let file = fs::File::open(root.join("coordinator/decision.json")).unwrap();
        let info = crate::runner::private_existing_windows::info(&file, false).unwrap();
        assert_eq!(
            crate::runner::private_existing_windows::identity(&info),
            staging_identity
        );
    }

    staged_file(
        &root,
        "coordinator/.decision.staging",
        b"replacement decision",
    );
    let error = publish_file_standard(&root).unwrap_err();
    assert!(error.to_string().contains("already exists"));
    assert_eq!(
        read_single_link_regular(&root.join("coordinator/decision.json")).unwrap(),
        b"first decision"
    );
    assert_eq!(
        read_single_link_regular(&root.join("coordinator/.decision.staging")).unwrap(),
        b"replacement decision"
    );
}

#[test]
fn secure_file_publish_rejects_cross_parent_missing_and_directory_sources() {
    let (_temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    staged_file(&root, ".decision.staging", b"decision");

    let error = publish_private_file_no_replace(
        &root,
        Path::new(".decision.staging"),
        Path::new("coordinator/decision.json"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("same private parent"));

    let error = publish_file_standard(&root).unwrap_err();
    assert!(error.to_string().contains("staging entry"));

    create_owner_only_dir_new(&root.join("coordinator/.decision.staging")).unwrap();
    assert!(publish_file_standard(&root).is_err());
    assert!(!root.join("coordinator/decision.json").exists());
}

#[cfg(unix)]
#[test]
fn secure_file_publish_rejects_linked_and_non_private_sources() {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::fs::symlink;

    let (temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    let outside = temp.path().join("outside");
    fs::write(&outside, b"outside").unwrap();
    symlink(&outside, root.join("coordinator/.decision.staging")).unwrap();
    assert!(publish_file_standard(&root).is_err());

    let (_temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    staged_file(&root, "coordinator/.decision.staging", b"decision");
    fs::hard_link(
        root.join("coordinator/.decision.staging"),
        root.join("coordinator/hardlink"),
    )
    .unwrap();
    assert!(publish_file_standard(&root).is_err());

    let (_temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    staged_file(&root, "coordinator/.decision.staging", b"decision");
    fs::set_permissions(
        root.join("coordinator/.decision.staging"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(publish_file_standard(&root).is_err());
}

#[cfg(unix)]
#[test]
fn secure_file_publish_rejects_parent_and_source_replacement() {
    let (_temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    staged_file(&root, "coordinator/.decision.staging", b"original");
    let error = publish_entry_with_hook(
        &root,
        Path::new("coordinator/.decision.staging"),
        Path::new("coordinator/decision.json"),
        EntryKind::File,
        &mut |checkpoint| {
            if checkpoint == PublishCheckpoint::IdentitiesCaptured {
                fs::rename(
                    root.join("coordinator/.decision.staging"),
                    root.join("coordinator/retained-source"),
                )?;
                staged_file(&root, "coordinator/.decision.staging", b"replacement");
            }
            Ok(())
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("staging identity changed"));
    assert!(!root.join("coordinator/decision.json").exists());

    let (_temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    staged_file(&root, "coordinator/.decision.staging", b"original");
    let error = publish_entry_with_hook(
        &root,
        Path::new("coordinator/.decision.staging"),
        Path::new("coordinator/decision.json"),
        EntryKind::File,
        &mut |checkpoint| {
            if checkpoint == PublishCheckpoint::ParentOpened {
                fs::rename(root.join("coordinator"), root.join("retained-parent"))?;
                create_owner_only_dir_new(&root.join("coordinator"))?;
            }
            Ok(())
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("parent identity changed"));
    assert!(!root.join("coordinator/decision.json").exists());
}

#[cfg(unix)]
#[test]
fn secure_file_publish_detects_source_replacement_at_the_rename_boundary() {
    let (_temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    staged_file(&root, "coordinator/.decision.staging", b"original");

    let error = publish_entry_with_hook(
        &root,
        Path::new("coordinator/.decision.staging"),
        Path::new("coordinator/decision.json"),
        EntryKind::File,
        &mut |checkpoint| {
            if checkpoint == PublishCheckpoint::SourceOpened {
                fs::rename(
                    root.join("coordinator/.decision.staging"),
                    root.join("coordinator/retained-source"),
                )?;
                staged_file(&root, "coordinator/.decision.staging", b"replacement");
            }
            Ok(())
        },
    )
    .unwrap_err();

    assert!(error.to_string().contains("staging identity changed"));
    assert_eq!(
        read_single_link_regular(&root.join("coordinator/decision.json")).unwrap(),
        b"replacement"
    );
}

#[test]
fn secure_file_publish_never_overwrites_a_destination_created_at_the_rename_boundary() {
    let (_temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    staged_file(&root, "coordinator/.decision.staging", b"original");

    let error = publish_entry_with_hook(
        &root,
        Path::new("coordinator/.decision.staging"),
        Path::new("coordinator/decision.json"),
        EntryKind::File,
        &mut |checkpoint| {
            if checkpoint == PublishCheckpoint::SourceOpened {
                staged_file(&root, "coordinator/decision.json", b"concurrent winner");
            }
            Ok(())
        },
    )
    .unwrap_err();

    assert!(error.to_string().contains("already exists"));
    assert_eq!(
        read_single_link_regular(&root.join("coordinator/decision.json")).unwrap(),
        b"concurrent winner"
    );
    assert_eq!(
        read_single_link_regular(&root.join("coordinator/.decision.staging")).unwrap(),
        b"original"
    );
}

#[cfg(unix)]
#[test]
fn secure_file_publish_rejects_fifo_without_blocking() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::sync::mpsc;
    use std::time::Duration;

    let (_temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    let fifo = root.join("coordinator/.decision.staging");
    let raw = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(raw.as_ptr(), 0o600) }, 0);
    let worker_root = root;
    let (send, receive) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        send.send(publish_file_standard(&worker_root).is_err())
            .unwrap();
    });

    let result = receive.recv_timeout(Duration::from_secs(1));
    let cleanup = unsafe { libc::open(raw.as_ptr(), libc::O_RDWR | libc::O_NONBLOCK) };
    assert!(cleanup >= 0);
    worker.join().unwrap();
    unsafe { libc::close(cleanup) };
    assert!(result.expect("FIFO publication inspection blocked"));
}

#[cfg(windows)]
#[test]
fn secure_file_publish_windows_rejects_broad_dacls_and_hardlinks() {
    use std::process::Command;

    let (_temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    staged_file(&root, "coordinator/.decision.staging", b"decision");
    assert!(
        Command::new("icacls")
            .arg(root.join("coordinator/.decision.staging"))
            .args(["/grant", "*S-1-1-0:(R)"])
            .status()
            .unwrap()
            .success()
    );
    assert!(publish_file_standard(&root).is_err());

    let (_temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    staged_file(&root, "coordinator/.decision.staging", b"decision");
    fs::write(root.join("coordinator/.decision.staging:stream"), b"hidden").unwrap();
    assert!(publish_file_standard(&root).is_err());

    let (_temp, root) = private_root();
    create_owner_only_dir_new(&root.join("coordinator")).unwrap();
    staged_file(&root, "coordinator/.decision.staging", b"decision");
    fs::hard_link(
        root.join("coordinator/.decision.staging"),
        root.join("coordinator/hardlink"),
    )
    .unwrap();
    assert!(publish_file_standard(&root).is_err());
}
