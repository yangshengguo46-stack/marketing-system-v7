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
        supervise_pair as _supervise_pair,
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
        supervise_pair as _supervise_pair,
        terminate_and_wait,
    )
    from batch_launch_spec import launch_spec_identity
    from contracts import sha256_json


ProcessLaunchError = LaunchMaterializationError
_Popen = subprocess.Popen


def spawn(
    prepared: PreparedProcess,
    limit: int,
    lifecycle: object,
    *,
    arm_class: str,
    launch_spec_sha256: str,
) -> OwnedProcess:
    return _spawn(
        prepared,
        limit,
        lifecycle,
        arm_class=arm_class,
        launch_spec_sha256=launch_spec_sha256,
        popen=_Popen,
        owned_type=OwnedProcess,
        launch_spec_identity_fn=launch_spec_identity,
        sha256_json_fn=sha256_json,
    )


def supervise_pair(owned: dict[str, OwnedProcess]):
    return _supervise_pair(owned, terminate_and_wait)
