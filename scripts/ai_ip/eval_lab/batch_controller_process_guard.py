import os
import time

try:
    from .batch_controller_capture import FatalSupervisorError
    from .batch_controller_process_lease import ProcessLifecycleLease
except ImportError:
    from batch_controller_capture import FatalSupervisorError
    from batch_controller_process_lease import ProcessLifecycleLease


class ProcessOwnershipGuard:
    def __init__(self, prepared, descriptors, lease: ProcessLifecycleLease):
        self.prepared = prepared
        self.descriptors = descriptors
        self.lease = lease
        self.process = None
        self.group_id = None

    def start(self, popen: object, argv: list[str], **kwargs):
        self.process = popen(argv, **kwargs)
        self.group_id = self.process.pid
        self.lease.attach_raw_process(self.process.pid, self.group_id)
        return self.process

    def close_descriptor(self, descriptor: int) -> None:
        os.close(descriptor)
        self.descriptors.discard(descriptor)

    def transfer(self) -> None:
        self.descriptors.clear()

    def abort(self, error: BaseException, stop, close):
        confirmed, cause = True, error
        if self.process is None:
            self.lease.cancel_before_start()
        else:
            try:
                confirmed = stop(self.process, self.group_id, time.monotonic() + 1.0)
            except BaseException as cause:
                confirmed = False
            if confirmed:
                self.lease.confirm_stopped()
            else:
                self.lease.mark_orphaned(cause)
            for name in ("stdout", "stderr"):
                descriptor = close(self.process, name)
                if descriptor is not None:
                    self.descriptors.discard(descriptor)
        for descriptor in tuple(self.descriptors):
            try:
                os.close(descriptor)
            except OSError:
                pass
        self.descriptors.clear()
        self.prepared.close()
        if not confirmed:
            raise FatalSupervisorError(
                "fatal supervisor orphan: post-launch stop was not confirmed"
            ) from cause
