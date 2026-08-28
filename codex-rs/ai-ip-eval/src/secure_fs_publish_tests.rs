use std::fs;
use std::path::Path;

use pretty_assertions::assert_eq;

use crate::secure_fs::create_owner_only_dir_new;
use crate::secure_fs::read_single_link_regular;
use crate::secure_fs::write_owner_only_new;
use crate::secure_fs_publish::PublishCheckpoint;
use crate::secure_fs_publish::publish_private_tree_no_replace;
use crate::secure_fs_publish::publish_private_tree_no_replace_for_test;

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

    let error = publish_private_tree_no_replace_for_test(
        &root,
        Path::new("coordinator/.staging"),
        Path::new("coordinator/final"),
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

    let error = publish_private_tree_no_replace_for_test(
        &root,
        Path::new("coordinator/.staging"),
        Path::new("coordinator/final"),
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

    let error = publish_private_tree_no_replace_for_test(
        &root,
        Path::new("coordinator/.staging"),
        Path::new("coordinator/final"),
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
