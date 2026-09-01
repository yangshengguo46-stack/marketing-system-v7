"""Pre-transfer ownership for launch descriptors and raw processes."""

import os
import subprocess
import time

try:
    from .batch_controller_capture import FatalSupervisorError
    from .batch_controller_process_lease import ProcessLifecycleLease
except ImportError:
    from batch_controller_capture import FatalSupervisorError
    from batch_controller_process_lease import ProcessLifecycleLease


_STOP_SECONDS = 1.0


def close_descriptor(descriptor: int) -> None:
    try:
        os.close(descriptor)
    except OSError:
        pass


def close_process_stream(process: subprocess.Popen, name: str) -> int | None:
    stream = getattr(process, name, None)
    if stream is None:
        return None
    try:
        descriptor = stream.fileno()
    except (OSError, ValueError):
        descriptor = None
    try:
        stream.close()
    except OSError:
        pass
    return descriptor


class ProcessOwnershipGuard:
    """Own launch FDs before Popen and raw process authority at assignment."""

    def __init__(
        self,
        prepared: object,
        descriptors: set[int],
        lease: ProcessLifecycleLease,
    ) -> None:
        self.prepared = prepared
        self.descriptors = descriptors
        self.lease = lease
        self.process: subprocess.Popen | None = None
        self.group_id: int | None = None
        self.transferred = False

    def start(self, popen: object, argv: list[str], **kwargs) -> subprocess.Popen:
        self.process = popen(argv, **kwargs)
        self.group_id = self.process.pid
        self.lease.attach_raw_process(self.process.pid, self.group_id)
        return self.process

    def close_descriptor(self, descriptor: int) -> None:
        os.close(descriptor)
        self.descriptors.discard(descriptor)

    def transfer(self) -> None:
        self.transferred = True
        self.descriptors.clear()

    def abort(self, error: BaseException, stop_process: object) -> None:
        confirmed = True
        stop_error = None
        if self.process is not None and self.group_id is not None:
            try:
                confirmed = stop_process(
                    self.process,
                    self.group_id,
                    time.monotonic() + _STOP_SECONDS,
                )
            except BaseException as termination_error:
                confirmed = False
                stop_error = termination_error
            if confirmed:
                self.lease.confirm_stopped()
            else:
                self.lease.mark_orphaned(stop_error or error)
            for name in ("stdout", "stderr"):
                descriptor = close_process_stream(self.process, name)
                if descriptor is not None:
                    self.descriptors.discard(descriptor)
        else:
            self.lease.cancel_before_start()
        for descriptor in tuple(self.descriptors):
            close_descriptor(descriptor)
        self.descriptors.clear()
        self.prepared.close()
        if not confirmed:
            raise FatalSupervisorError(
                "fatal supervisor orphan: post-launch stop was not confirmed"
            ) from (stop_error or error)
