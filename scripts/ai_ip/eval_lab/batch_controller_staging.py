"""Controller-owned materialization of immutable preflight snapshots."""

import shutil
from dataclasses import dataclass
from pathlib import Path

try:
    from .batch_controller_snapshots import materialize_file
    from .batch_controller_support import ValidatedBindings
except ImportError:
    from batch_controller_snapshots import materialize_file
    from batch_controller_support import ValidatedBindings


@dataclass(frozen=True)
class StagedInputs:
    root: Path
    stock_seed: Path
    modified_seed: Path
    workspace_seed: Path
    schema_path: Path

    def cleanup(self) -> None:
        shutil.rmtree(self.root)


def stage_inputs(bindings: ValidatedBindings, pair_id: str) -> StagedInputs:
    root = Path(bindings.attempt_base) / f".batch-inputs-{pair_id}"
    try:
        root.mkdir(mode=0o700)
        stock = bindings.stock.codex_home_seed.materialize(root / "seed-a")
        modified = bindings.modified.codex_home_seed.materialize(root / "seed-b")
        workspace = bindings.workspace_seed.materialize(root / "workspace")
        schema = materialize_file(
            root / "case-answer-schema.json", bindings.case_answer_schema_file, 0o600
        )
        return StagedInputs(root, stock, modified, workspace, schema)
    except BaseException:
        shutil.rmtree(root, ignore_errors=True)
        raise
