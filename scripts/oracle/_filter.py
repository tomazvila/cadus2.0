"""The long-lived filter shape of `check_1_0.py` and `rewrite_1_0.py`.

Both scripts read one JSON object per line on stdin, write one JSON object per
line on stdout, and run every request in a persistent worker process under a
wall-clock guard. This module holds the worker host and the stdin loop once.
The scripts keep what differs: the worker function, the request check, and the
answer on a timeout.
"""

from __future__ import annotations

import json
import multiprocessing
import sys

#: How long the parent waits for a terminated worker to go away.
REAP_TIMEOUT_S = 5.0


class WorkerHost:
    """One persistent worker process, respawned after every timeout.

    `target` is the worker function. It receives the child end of a duplex pipe
    and answers one message per request until the parent closes the pipe.
    """

    def __init__(self, timeout_s: float, target) -> None:
        self.timeout_s = timeout_s
        self.target = target
        # `fork` inherits the SymPy import, so a respawn costs milliseconds.
        self.context = multiprocessing.get_context("fork")
        self.connection = None
        self.process = None
        self.start()

    def start(self) -> None:
        """Start a new worker process."""
        parent_end, child_end = self.context.Pipe(duplex=True)
        process = self.context.Process(target=self.target, args=(child_end,), daemon=True)
        process.start()
        # The parent drops its copy of the child end, so a dead worker gives EOF.
        child_end.close()
        self.connection = parent_end
        self.process = process

    def stop(self) -> None:
        """Terminate the worker and close the pipe."""
        process = self.process
        connection = self.connection
        self.process = None
        self.connection = None
        if connection is not None:
            try:
                connection.close()
            except OSError:
                pass
        if process is None:
            return
        process.terminate()
        process.join(REAP_TIMEOUT_S)
        if process.is_alive():
            process.kill()
            process.join(REAP_TIMEOUT_S)

    def restart(self) -> None:
        """Terminate the worker and start a fresh one."""
        self.stop()
        self.start()


def send_error(connection, exc: BaseException) -> bool:
    """Send one error message. Return False when the pipe is gone."""
    try:
        connection.send(("error", f"{type(exc).__name__}: {exc}"))
    except (BrokenPipeError, OSError):
        return False
    return True


def serve(host: WorkerHost, handle) -> int:
    """Run the filter until stdin closes.

    `handle(line, host)` turns one request line into one response object.
    """
    sys.stdout.write(json.dumps({"ready": True, "timeout_s": host.timeout_s}) + "\n")
    sys.stdout.flush()
    try:
        for line in sys.stdin:
            if not line.strip():
                continue
            response = handle(line, host)
            sys.stdout.write(
                json.dumps(response, sort_keys=True, ensure_ascii=False) + "\n"
            )
            sys.stdout.flush()
    finally:
        host.stop()
    return 0
