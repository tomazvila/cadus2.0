#!/usr/bin/env python
"""1.0 answer-checker oracle for Cadus 2.0 milestone M2 (IDs V3, R5).

The script is a long-lived filter (spec section 9.1). It reads one JSON object
per line on stdin and it writes one JSON object per line on stdout. It pays the
SymPy import once, so a caller can ask for many thousands of verdicts.

Usage:
    /home/deploy/dev/cadus/.venv/bin/python check_1_0.py [--timeout SECONDS]

Request fields:
    expected  the authored answer, verbatim
    learner   the learner answer, verbatim
    kind      "numeric", "expression", "multi-step", or "proof"

Response fields:
    equivalent  `answers_equivalent(expected, learner, kind)`, or null on a timeout
    notation    `dot_thousands_variant(expected, learner, kind)`, or null on a timeout
    timeout     true when the wall-clock guard stopped the call

A response keeps the `id` field of its request when the request has one, so a
caller can pair the two. A malformed request gets an `error` field instead of a
verdict; the filter stays open.

# The wall-clock guard (review round 1, finding #21)

1.0 `simplify` has no bound (spec section 3.2), so every call runs under a
wall-clock guard. The default guard is 2 seconds.

The guard runs the 1.0 call in a child process, never in this process. An
earlier version raised a `Timeout` exception from a `SIGALRM` handler, and 1.0
`_sympy_equivalent` swallowed it: the bare `except Exception` handlers of
`/home/deploy/dev/cadus/cadus_web/sympy_check.py` caught the guard and returned
`False`, so a stopped pair was recorded as a decided 1.0 `false`. A child
process cannot swallow a `SIGTERM` that this process sends.

The child is a persistent worker. It starts by `fork`, so it inherits the SymPy
import and costs a few milliseconds. It answers one pair at a time. When the
worker misses the deadline, the parent terminates it, starts a new one, and
writes `{"equivalent": null, "notation": null, "timeout": true}`. A timeout
means the oracle has no opinion on that pair, and the caller must skip it.

Read-only. The script imports the 1.0 package; it never writes into it.
"""

from __future__ import annotations

import argparse
import json
import multiprocessing
import sys

sys.path.insert(0, "/home/deploy/dev/cadus")

from cadus.model import AnswerKind  # noqa: E402
from cadus_web.sympy_check import (  # noqa: E402
    answers_equivalent,
    dot_thousands_variant,
)

DEFAULT_TIMEOUT_S = 2.0

# How long the parent waits for a terminated worker to go away.
REAP_TIMEOUT_S = 5.0


def _worker(connection) -> None:
    """Answer one pair per message until the parent closes the pipe."""
    while True:
        try:
            request = connection.recv()
        except (EOFError, KeyboardInterrupt):
            return
        if request is None:
            return
        expected, learner, kind_name = request
        try:
            kind = AnswerKind(kind_name)
            equivalent = bool(answers_equivalent(expected, learner, kind))
            notation = bool(dot_thousands_variant(expected, learner, kind))
        except BaseException as exc:  # the parent decides what a 1.0 failure means
            try:
                connection.send(("error", f"{type(exc).__name__}: {exc}"))
            except (BrokenPipeError, OSError):
                return
            continue
        try:
            connection.send(("ok", equivalent, notation))
        except (BrokenPipeError, OSError):
            return


class Oracle:
    """One persistent 1.0 worker process, respawned after every timeout."""

    def __init__(self, timeout_s: float) -> None:
        self.timeout_s = timeout_s
        # `fork` inherits the SymPy import, so a respawn costs milliseconds.
        self.context = multiprocessing.get_context("fork")
        self.connection = None
        self.process = None
        self.start()

    def start(self) -> None:
        """Start a new worker process."""
        parent_end, child_end = self.context.Pipe(duplex=True)
        process = self.context.Process(target=_worker, args=(child_end,), daemon=True)
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

    def verdict(self, expected: str, learner: str, kind_name: str) -> dict:
        """Return the 1.0 verdict for one pair, under the wall-clock guard."""
        connection = self.connection
        if connection is None:
            self.start()
            connection = self.connection
        if connection is None:
            return {"error": "the oracle worker did not start"}
        try:
            connection.send((expected, learner, kind_name))
        except (BrokenPipeError, OSError) as exc:
            self.restart()
            return {"error": f"{type(exc).__name__}: {exc}"}
        if not connection.poll(self.timeout_s):
            self.restart()
            return {"equivalent": None, "notation": None, "timeout": True}
        try:
            message = connection.recv()
        except (EOFError, OSError) as exc:
            self.restart()
            return {"error": f"the oracle worker died: {type(exc).__name__}: {exc}"}
        if message[0] == "error":
            return {"error": message[1]}
        return {"equivalent": message[1], "notation": message[2], "timeout": False}


def handle(line: str, oracle: Oracle) -> dict:
    """Read one request line and return one response object."""
    try:
        request = json.loads(line)
    except ValueError as exc:
        return {"error": f"bad json: {exc}"}
    if not isinstance(request, dict):
        return {"error": "request is not an object"}
    expected = request.get("expected")
    learner = request.get("learner")
    kind_name = request.get("kind")
    if not isinstance(expected, str) or not isinstance(learner, str):
        return {"error": "expected and learner must be strings"}
    try:
        AnswerKind(kind_name)
    except ValueError:
        return {"error": f"unknown answer kind: {kind_name!r}"}
    response = oracle.verdict(expected, learner, kind_name)
    if "id" in request:
        response["id"] = request["id"]
    return response


def main() -> int:
    """Run the filter until stdin closes."""
    parser = argparse.ArgumentParser(description="1.0 answer-checker oracle")
    parser.add_argument(
        "--timeout",
        type=float,
        default=DEFAULT_TIMEOUT_S,
        help="wall-clock guard per pair, in seconds (default: 2.0)",
    )
    args = parser.parse_args()
    oracle = Oracle(args.timeout)
    sys.stdout.write(json.dumps({"ready": True, "timeout_s": args.timeout}) + "\n")
    sys.stdout.flush()
    try:
        for line in sys.stdin:
            if not line.strip():
                continue
            response = handle(line, oracle)
            sys.stdout.write(
                json.dumps(response, sort_keys=True, ensure_ascii=False) + "\n"
            )
            sys.stdout.flush()
    finally:
        oracle.stop()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
