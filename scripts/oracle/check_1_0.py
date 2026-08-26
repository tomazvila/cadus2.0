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

1.0 `simplify` has no bound (spec section 3.2), so every call runs under a
wall-clock guard. The default guard is 2 seconds. A timeout means the oracle has
no opinion on that pair, and the caller must skip it.

Read-only. The script imports the 1.0 package; it never writes into it.
"""

from __future__ import annotations

import argparse
import json
import signal
import sys
from types import FrameType

sys.path.insert(0, "/home/deploy/dev/cadus")

from cadus.model import AnswerKind  # noqa: E402
from cadus_web.sympy_check import (  # noqa: E402
    answers_equivalent,
    dot_thousands_variant,
)

DEFAULT_TIMEOUT_S = 2.0


class Timeout(Exception):
    """The wall-clock guard stopped one call."""


def _on_alarm(signum: int, frame: FrameType | None) -> None:
    """Raise inside the interpreter as soon as the guard fires."""
    raise Timeout


def verdict(expected: str, learner: str, kind: AnswerKind, timeout_s: float) -> dict:
    """Return the 1.0 verdict for one pair, under the wall-clock guard."""
    signal.setitimer(signal.ITIMER_REAL, timeout_s)
    try:
        equivalent = bool(answers_equivalent(expected, learner, kind))
        notation = bool(dot_thousands_variant(expected, learner, kind))
    except Timeout:
        return {"equivalent": None, "notation": None, "timeout": True}
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0.0)
    return {"equivalent": equivalent, "notation": notation, "timeout": False}


def handle(line: str, timeout_s: float) -> dict:
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
        kind = AnswerKind(kind_name)
    except ValueError:
        return {"error": f"unknown answer kind: {kind_name!r}"}
    try:
        response = verdict(expected, learner, kind, timeout_s)
    except Exception as exc:  # the filter stays open on any 1.0 failure
        return {"error": f"{type(exc).__name__}: {exc}"}
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
    signal.signal(signal.SIGALRM, _on_alarm)
    sys.stdout.write(json.dumps({"ready": True, "timeout_s": args.timeout}) + "\n")
    sys.stdout.flush()
    for line in sys.stdin:
        if not line.strip():
            continue
        response = handle(line, args.timeout)
        sys.stdout.write(
            json.dumps(response, sort_keys=True, ensure_ascii=False) + "\n"
        )
        sys.stdout.flush()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
