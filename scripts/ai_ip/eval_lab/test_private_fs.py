import os
import stat
from pathlib import Path
import subprocess
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from contracts import LabContractError
import private_fs as private_fs_module
from private_fs import PrivateRoot


REPO_ROOT = Path(__file__).resolve().parents[3]


def _mode(path: Path) -> int:
    return stat.S_IMODE(path.stat(follow_symlinks=False).st_mode)


def _git(repo: Path, *arguments: str) -> None:
    subprocess.run(
        ["git", "-C", str(repo), *arguments],
        check=True,
        capture_output=True,
    )


def test_create_new_rejects_relative_existing_and_in_repo_roots(tmp_path):
    with pytest.raises(LabContractError):
        PrivateRoot.create_new(Path("relative-private-root"))

    existing = tmp_path / "existing"
    existing.mkdir()
    with pytest.raises(LabContractError):
        PrivateRoot.create_new(existing)

    with pytest.raises(LabContractError):
        PrivateRoot.create_new(REPO_ROOT / ".task-2-private-root-must-not-exist")


def test_create_new_rejects_symlink_ancestor(tmp_path):
    actual_parent = tmp_path / "actual"
    actual_parent.mkdir()
    linked_parent = tmp_path / "linked"
    linked_parent.symlink_to(actual_parent, target_is_directory=True)

    with pytest.raises(LabContractError):
        PrivateRoot.create_new(linked_parent / "private")
    assert not (actual_parent / "private").exists()


def test_create_new_rejects_root_inside_secondary_linked_worktree(
    tmp_path, monkeypatch
):
    repository = tmp_path / "repository"
    repository.mkdir()
    _git(repository, "init", "-q")
    _git(repository, "config", "user.name", "Private FS test")
    _git(repository, "config", "user.email", "private-fs@example.invalid")
    (repository / "tracked.txt").write_text("tracked\n", encoding="utf-8")
    _git(repository, "add", "tracked.txt")
    _git(repository, "commit", "-q", "-m", "initial")
    linked = tmp_path / "linked"
    _git(repository, "worktree", "add", "--detach", "-q", str(linked), "HEAD")
    monkeypatch.setattr(private_fs_module, "_MODULE_REPO_ROOT", repository)

    with pytest.raises(LabContractError):
        PrivateRoot.create_new(linked / "private")
    assert not (linked / "private").exists()


def test_create_new_cleans_root_when_first_metadata_read_fails(tmp_path, monkeypatch):
    root_path = tmp_path / "private"
    original_lstat = Path.lstat
    armed = True

    def fail_first_post_create_lstat(path):
        nonlocal armed
        metadata = original_lstat(path)
        if path == root_path and armed:
            armed = False
            raise OSError("injected first root metadata failure")
        return metadata

    monkeypatch.setattr(Path, "lstat", fail_first_post_create_lstat)

    with pytest.raises((LabContractError, OSError)):
        PrivateRoot.create_new(root_path)
    assert not os.path.lexists(root_path)


def test_root_and_new_json_are_owner_only_and_created_once(tmp_path):
    root_path = tmp_path / "private"
    private = PrivateRoot.create_new(root_path)
    private.create_dir("cases")
    private.write_new_json("cases/result.json", {"礼": "人情", "a": 1})

    assert _mode(root_path) == 0o700
    assert _mode(root_path / "cases") == 0o700
    assert _mode(root_path / "cases" / "result.json") == 0o600
    assert (root_path / "cases" / "result.json").read_bytes() == (
        '{"a":1,"礼":"人情"}\n'.encode("utf-8")
    )
    assert private.read_json("cases/result.json") == {"a": 1, "礼": "人情"}

    with pytest.raises(LabContractError):
        private.write_new_json("cases/result.json", {"replacement": True})
    assert private.read_json("cases/result.json") == {"a": 1, "礼": "人情"}


def test_create_dir_cleans_directory_when_first_metadata_read_fails(
    tmp_path, monkeypatch
):
    root_path = tmp_path / "private"
    private = PrivateRoot.create_new(root_path)
    original_lstat_at = private_fs_module._lstat_at
    armed = True

    def fail_first_child_metadata(parent_fd, name):
        nonlocal armed
        if name == "child" and armed:
            armed = False
            raise OSError("injected first directory metadata failure")
        return original_lstat_at(parent_fd, name)

    monkeypatch.setattr(private_fs_module, "_lstat_at", fail_first_child_metadata)

    with pytest.raises((LabContractError, OSError)):
        private.create_dir("child")
    assert not os.path.lexists(root_path / "child")


def test_write_new_json_cleans_file_when_first_fstat_fails(tmp_path, monkeypatch):
    root_path = tmp_path / "private"
    private = PrivateRoot.create_new(root_path)
    original_fstat = os.fstat
    armed = True

    def fail_first_file_fstat(descriptor):
        nonlocal armed
        metadata = original_fstat(descriptor)
        if stat.S_ISREG(metadata.st_mode) and armed:
            armed = False
            raise OSError("injected first file metadata failure")
        return metadata

    monkeypatch.setattr(private_fs_module.os, "fstat", fail_first_file_fstat)

    with pytest.raises((LabContractError, OSError)):
        private.write_new_json("result.json", {"a": 1})
    assert not os.path.lexists(root_path / "result.json")


def test_write_new_json_rejects_retained_inode_replacement(tmp_path, monkeypatch):
    root_path = tmp_path / "private"
    private = PrivateRoot.create_new(root_path)
    original_read = PrivateRoot._read_checked_file
    replaced = False

    def replace_before_reopen(self, parent_fd, name, *args, **kwargs):
        nonlocal replaced
        if not replaced:
            replaced = True
            os.rename(
                name,
                "created-away.json",
                src_dir_fd=parent_fd,
                dst_dir_fd=parent_fd,
            )
            descriptor = os.open(
                name,
                os.O_CREAT | os.O_EXCL | os.O_WRONLY,
                0o600,
                dir_fd=parent_fd,
            )
            try:
                os.write(descriptor, b'{"a":1}\n')
                os.fsync(descriptor)
            finally:
                os.close(descriptor)
        return original_read(self, parent_fd, name, *args, **kwargs)

    monkeypatch.setattr(PrivateRoot, "_read_checked_file", replace_before_reopen)

    with pytest.raises(LabContractError):
        private.write_new_json("result.json", {"a": 1})


@pytest.mark.parametrize("relative", ["../escaped", "cases/../../escaped", "/absolute"])
def test_relative_operations_reject_traversal_and_absolute_paths(tmp_path, relative):
    private = PrivateRoot.create_new(tmp_path / "private")

    with pytest.raises(LabContractError):
        private.create_dir(relative)
    with pytest.raises(LabContractError):
        private.write_new_json(relative, {"a": 1})
    with pytest.raises(LabContractError):
        private.read_json(relative)


def test_operations_reject_symlink_directory_and_file_paths(tmp_path):
    root_path = tmp_path / "private"
    private = PrivateRoot.create_new(root_path)
    private.create_dir("actual")
    (root_path / "linked").symlink_to("actual", target_is_directory=True)

    with pytest.raises(LabContractError):
        private.write_new_json("linked/result.json", {"a": 1})

    private.write_new_json("actual/result.json", {"a": 1})
    (root_path / "linked.json").symlink_to("actual/result.json")
    with pytest.raises(LabContractError):
        private.read_json("linked.json")


def test_read_json_rejects_hardlinked_retained_file(tmp_path):
    root_path = tmp_path / "private"
    private = PrivateRoot.create_new(root_path)
    private.write_new_json("retained.json", {"a": 1})
    os.link(root_path / "retained.json", tmp_path / "second-link.json")

    with pytest.raises(LabContractError):
        private.read_json("retained.json")


def test_open_and_read_reject_modes_broader_than_owner_only(tmp_path):
    root_path = tmp_path / "private"
    private = PrivateRoot.create_new(root_path)
    private.write_new_json("result.json", {"a": 1})
    (root_path / "result.json").chmod(0o640)

    with pytest.raises(LabContractError):
        private.read_json("result.json")

    root_path.chmod(0o750)
    with pytest.raises(LabContractError):
        PrivateRoot.open_existing(root_path)


def test_open_existing_rechecks_root_and_reads_retained_json(tmp_path):
    root_path = tmp_path / "private"
    PrivateRoot.create_new(root_path).write_new_json("result.json", {"a": 1})

    reopened = PrivateRoot.open_existing(root_path)
    assert reopened.repo_root == REPO_ROOT
    assert reopened.read_json("result.json") == {"a": 1}


def test_private_fs_supports_package_import_from_scripts_root(tmp_path):
    scripts_root = Path(__file__).resolve().parents[2]
    completed = subprocess.run(
        [
            sys.executable,
            "-c",
            (
                "import sys; "
                f"sys.path.insert(0, {str(scripts_root)!r}); "
                "from ai_ip.eval_lab.private_fs import PrivateRoot"
            ),
        ],
        cwd=tmp_path,
        check=False,
        capture_output=True,
        text=True,
    )

    assert completed.returncode == 0, completed.stderr
