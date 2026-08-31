"""Thin facade over descriptor materialization, records, and supervision."""

import os
import subprocess

try:
    from .batch_controller_launch_materialization import (
        LaunchMaterializationError,
        PreparedProcess,
        prepare_process,
    )
    from .batch_controller_launch_record import seal_launch_record
    from .batch_controller_supervision import (
        OwnedProcess,
        close_owned,
        spawn as _spawn,
        supervise_pair,
        terminate_and_wait,
    )
    from .batch_launch_spec import launch_spec_identity
    from .contracts import sha256_json
except ImportError:
    from batch_controller_launch_materialization import (
        LaunchMaterializationError,
        PreparedProcess,
        prepare_process,
    )
    from batch_controller_launch_record import seal_launch_record
    from batch_controller_supervision import (
        OwnedProcess,
        close_owned,
        spawn as _spawn,
        supervise_pair,
        terminate_and_wait,
    )
    from batch_launch_spec import launch_spec_identity
    from contracts import sha256_json


ProcessLaunchError = LaunchMaterializationError
_Popen = subprocess.Popen


def spawn(prepared: PreparedProcess, limit: int, lifecycle: object) -> OwnedProcess:
    return _spawn(
        prepared,
        limit,
        lifecycle,
        popen=_Popen,
        owned_type=OwnedProcess,
        launch_spec_identity_fn=launch_spec_identity,
        sha256_json_fn=sha256_json,
    )
