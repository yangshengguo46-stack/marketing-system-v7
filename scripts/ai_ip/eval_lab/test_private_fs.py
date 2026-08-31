import os
import stat
from pathlib import Path
import subprocess
import sys
import tempfile

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from contracts import LabContractError, canonical_json_bytes, load_exact_json
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


def test_create_new_rejects_group_or_world_writable_parent(tmp_path):
    untrusted_parent = tmp_path / "untrusted"
    untrusted_parent.mkdir(mode=0o777)
    untrusted_parent.chmod(0o777)

    with pytest.raises(LabContractError):
        PrivateRoot.create_new(untrusted_parent / "private")
    assert not (untrusted_parent / "private").exists()


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


def test_first_root_metadata_failure_does_not_delete_unproven_replacement(
    tmp_path, monkeypatch
):
    root_path = tmp_path / "private"
    displaced = tmp_path / "created-by-operation"
    original_lstat_at = private_fs_module._lstat_at
    armed = True

    def replace_then_fail(parent_fd, name):
        nonlocal armed
        if name == root_path.name and armed:
            armed = False
            os.rename(
                name,
                displaced.name,
                src_dir_fd=parent_fd,
                dst_dir_fd=parent_fd,
            )
            os.mkdir(name, 0o700, dir_fd=parent_fd)
            raise OSError("injected first root metadata failure")
        return original_lstat_at(parent_fd, name)

    monkeypatch.setattr(private_fs_module, "_lstat_at", replace_then_fail)

    with pytest.raises((LabContractError, OSError)):
        PrivateRoot.create_new(root_path)
    assert root_path.is_dir()
    assert displaced.is_dir()
    assert not (root_path / private_fs_module._RECEIPT_NAME).exists()
    with pytest.raises(LabContractError):
        PrivateRoot.open_existing(root_path)


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


def test_completion_receipt_is_bound_owner_only_and_reserved(tmp_path):
    root_path = tmp_path / "private"
    private = PrivateRoot.create_new(root_path)
    receipt = root_path / private_fs_module._RECEIPT_NAME
    root_metadata = root_path.stat(follow_symlinks=False)
    receipt_metadata = receipt.stat(follow_symlinks=False)

    assert _mode(receipt) == 0o600
    assert load_exact_json(receipt) == {
        "formatVersion": 1,
        "receiptDevice": receipt_metadata.st_dev,
        "receiptInode": receipt_metadata.st_ino,
        "rootDevice": root_metadata.st_dev,
        "rootInode": root_metadata.st_ino,
    }
    with pytest.raises(FileExistsError):
        descriptor = os.open(receipt, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        os.close(descriptor)
    with pytest.raises(LabContractError):
        private.create_dir(private_fs_module._RECEIPT_NAME)
    with pytest.raises(LabContractError):
        private.write_new_json(private_fs_module._RECEIPT_NAME, {"a": 1})
    with pytest.raises(LabContractError):
        private.read_json(private_fs_module._RECEIPT_NAME)

    with tempfile.TemporaryDirectory(dir=tmp_path) as temporary:
        second = PrivateRoot.create_new(Path(temporary) / "private")
        assert _mode(second.path) == 0o700


def test_open_existing_rejects_invalid_or_replaced_completion_receipt(tmp_path):
    def make_root(name):
        path = tmp_path / name
        PrivateRoot.create_new(path)
        return path, path / private_fs_module._RECEIPT_NAME

    missing_root, missing = make_root("missing")
    missing.unlink()
    with pytest.raises(LabContractError):
        PrivateRoot.open_existing(missing_root)

    malformed_root, malformed = make_root("malformed")
    malformed.write_bytes(b"not-json\n")
    with pytest.raises(LabContractError):
        PrivateRoot.open_existing(malformed_root)

    mismatched_root, mismatched = make_root("mismatched")
    value = load_exact_json(mismatched)
    value["rootInode"] += 1
    mismatched.write_bytes(canonical_json_bytes(value) + b"\n")
    with pytest.raises(LabContractError):
        PrivateRoot.open_existing(mismatched_root)

    replaced_root, replaced = make_root("replaced-receipt")
    retained = replaced.read_bytes()
    replaced.unlink()
    replaced.write_bytes(retained)
    replaced.chmod(0o600)
    with pytest.raises(LabContractError):
        PrivateRoot.open_existing(replaced_root)

    original_root, original_receipt = make_root("replaced-root")
    receipt_bytes = original_receipt.read_bytes()
    displaced = tmp_path / "original-root"
    original_root.rename(displaced)
    original_root.mkdir(mode=0o700)
    replacement_receipt = original_root / private_fs_module._RECEIPT_NAME
    replacement_receipt.write_bytes(receipt_bytes)
    replacement_receipt.chmod(0o600)
    with pytest.raises(LabContractError):
        PrivateRoot.open_existing(original_root)


def test_open_existing_rejects_boolean_receipt_version(tmp_path):
    root_path = tmp_path / "private"
    PrivateRoot.create_new(root_path)
    receipt = root_path / private_fs_module._RECEIPT_NAME
    value = load_exact_json(receipt)
    value["formatVersion"] = True
    receipt.write_bytes(canonical_json_bytes(value) + b"\n")

    with pytest.raises(LabContractError):
        PrivateRoot.open_existing(root_path)


def test_read_second_fstat_failure_closes_opened_file_descriptor(tmp_path, monkeypatch):
    root_path = tmp_path / "private"
    private = PrivateRoot.create_new(root_path)
    private.write_new_json("result.json", {"a": 1})
    target_inode = (root_path / "result.json").stat().st_ino
    original_fstat = os.fstat
    original_close = os.close
    target_calls = 0
    failed_descriptor = None
    closed_after_failure = False

    def fail_second_target_fstat(descriptor):
        nonlocal target_calls, failed_descriptor
        metadata = original_fstat(descriptor)
        if metadata.st_ino == target_inode:
            target_calls += 1
            if target_calls == 2:
                failed_descriptor = descriptor
                raise OSError("injected retained-file fstat failure")
        return metadata

    def record_close(descriptor):
        nonlocal closed_after_failure
        if failed_descriptor == descriptor:
            closed_after_failure = True
        original_close(descriptor)

    monkeypatch.setattr(private_fs_module.os, "fstat", fail_second_target_fstat)
    monkeypatch.setattr(private_fs_module.os, "close", record_close)

    try:
        with pytest.raises(OSError):
            private.read_json("result.json")
        assert closed_after_failure
    finally:
        if failed_descriptor is not None and not closed_after_failure:
            original_close(failed_descriptor)


def test_valid_receipt_survives_close_and_unlink_errors_as_committed(
    tmp_path, monkeypatch
):
    root_path = tmp_path / "private"
    original_close = os.close
    original_unlink = os.unlink
    injected = False

    def fail_first_regular_close(descriptor):
        nonlocal injected
        is_regular = stat.S_ISREG(os.fstat(descriptor).st_mode)
        original_close(descriptor)
        if is_regular and not injected:
            injected = True
            raise OSError("injected precommit receipt close failure")

    def fail_receipt_unlink(name, *args, **kwargs):
        if name == private_fs_module._RECEIPT_NAME:
            raise OSError("injected receipt unlink failure")
        return original_unlink(name, *args, **kwargs)

    monkeypatch.setattr(private_fs_module.os, "close", fail_first_regular_close)
    monkeypatch.setattr(private_fs_module.os, "unlink", fail_receipt_unlink)

    private = PrivateRoot.create_new(root_path)
    assert injected
    assert private.path == root_path
    assert PrivateRoot.open_existing(root_path).path == root_path


def test_ambiguous_receipt_close_is_attempted_once(tmp_path, monkeypatch):
    root_path = tmp_path / "private"
    original_close = os.close
    ambiguous_fd = None
    close_attempts = 0

    def fail_receipt_close_without_closing(descriptor):
        nonlocal ambiguous_fd, close_attempts
        if ambiguous_fd is None and stat.S_ISREG(os.fstat(descriptor).st_mode):
            ambiguous_fd = descriptor
        if descriptor == ambiguous_fd:
            close_attempts += 1
            raise OSError("injected ambiguous receipt close failure")
        original_close(descriptor)

    monkeypatch.setattr(
        private_fs_module.os, "close", fail_receipt_close_without_closing
    )

    try:
        private = PrivateRoot.create_new(root_path)
        assert close_attempts == 1
        assert PrivateRoot.open_existing(root_path).path == private.path
    finally:
        monkeypatch.setattr(private_fs_module.os, "close", original_close)
        if ambiguous_fd is not None:
            original_close(ambiguous_fd)


@pytest.mark.parametrize("directory_close_index", [1, 2])
def test_postcommit_root_or_parent_close_failure_still_returns_valid_root(
    tmp_path, monkeypatch, directory_close_index
):
    root_path = tmp_path / f"private-{directory_close_index}"
    original_close = os.close
    directory_closes = 0

    def fail_selected_directory_close(descriptor):
        nonlocal directory_closes
        is_directory = stat.S_ISDIR(os.fstat(descriptor).st_mode)
        original_close(descriptor)
        if is_directory:
            directory_closes += 1
            if directory_closes == directory_close_index:
                raise OSError("injected postcommit directory close failure")

    monkeypatch.setattr(private_fs_module.os, "close", fail_selected_directory_close)

    private = PrivateRoot.create_new(root_path)
    assert private.path == root_path
    assert PrivateRoot.open_existing(root_path).path == root_path


def test_child_metadata_failure_does_not_delete_unproven_replacement(
    tmp_path, monkeypatch
):
    root_path = tmp_path / "private"
    private = PrivateRoot.create_new(root_path)
    displaced = root_path / "created-child"
    original_lstat_at = private_fs_module._lstat_at
    armed = True

    def replace_then_fail(parent_fd, name):
        nonlocal armed
        if name == "child" and armed:
            armed = False
            os.rename(
                name,
                displaced.name,
                src_dir_fd=parent_fd,
                dst_dir_fd=parent_fd,
            )
            os.mkdir(name, 0o700, dir_fd=parent_fd)
            raise OSError("injected first directory metadata failure")
        return original_lstat_at(parent_fd, name)

    monkeypatch.setattr(private_fs_module, "_lstat_at", replace_then_fail)

    with pytest.raises((LabContractError, OSError)):
        private.create_dir("child")
    assert (root_path / "child").is_dir()
    assert displaced.is_dir()


def test_write_failure_does_not_delete_file_without_proven_identity(
    tmp_path, monkeypatch
):
    root_path = tmp_path / "private"
    private = PrivateRoot.create_new(root_path)
    original_metadata = private_fs_module._created_file_metadata
    armed = True

    def fail_first_file_metadata(descriptor, description):
        nonlocal armed
        if description == "result.json" and armed:
            armed = False
            raise OSError("injected first file metadata failure")
        return original_metadata(descriptor, description)

    monkeypatch.setattr(
        private_fs_module, "_created_file_metadata", fail_first_file_metadata
    )

    with pytest.raises((LabContractError, OSError)):
        private.write_new_json("result.json", {"a": 1})
    assert os.path.lexists(root_path / "result.json")
    with pytest.raises(LabContractError):
        private.read_json("result.json")
    with pytest.raises(LabContractError):
        private.write_new_json("result.json", {"a": 1})


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
