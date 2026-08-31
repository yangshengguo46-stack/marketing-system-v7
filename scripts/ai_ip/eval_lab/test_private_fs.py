import os
import stat
from pathlib import Path
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from contracts import LabContractError
from private_fs import PrivateRoot


REPO_ROOT = Path(__file__).resolve().parents[3]


def _mode(path: Path) -> int:
    return stat.S_IMODE(path.stat(follow_symlinks=False).st_mode)


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
