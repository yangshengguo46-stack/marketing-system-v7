import os
import sys
from pathlib import Path

import pytest


sys.path.insert(0, str(Path(__file__).resolve().parent))


@pytest.mark.skipif(os.name != "nt", reason="native Windows acquisition journal")
def test_native_identity_failure_retains_duplicate_backed_handle_ownership(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    import batch_isolation_win32 as win32
    from batch_isolation_windows_cleanup import WindowsCleanupError
    from batch_isolation_windows_cleanup import WindowsCleanupJournal

    candidate = tmp_path / "identity-query.txt"
    candidate.write_text("owned", encoding="utf-8")
    handle = win32.open_path(candidate, directory=False)

    class Filesystem:
        marker_handle = None
        root_fd = -1
        base_fd = -1
        directories: dict[str, int] = {}

    filesystem = Filesystem()
    journal = WindowsCleanupJournal()
    original_identity = win32.identity
    failed = False

    def fail_duplicate_query_once(candidate_handle: int) -> tuple[int, int]:
        nonlocal failed
        if candidate_handle != handle and not failed:
            failed = True
            raise win32.Win32SecurityError(5, "injected identity query failure")
        return original_identity(candidate_handle)

    monkeypatch.setattr(win32, "identity", fail_duplicate_query_once)
    with pytest.raises(WindowsCleanupError, match="identity|uncertain"):
        journal.close(filesystem, handle)
    assert journal.creator_pid == os.getpid()
    assert len(journal.pending) == 1
    assert journal.pending[0].state == "identity_unknown"
    assert journal.pending[0].proof_handle is not None

    monkeypatch.setattr(win32, "identity", original_identity)
    journal.retry(filesystem)
    assert journal.pending == []
