#!/usr/bin/env python
"""Rational-rewrite spelling generator for Cadus 2.0 milestone M2 (IDs V3, R5).

The script is a long-lived filter, and it reads and writes the same way
`check_1_0.py` does: one JSON object per line on stdin, one JSON object per line
on stdout. It pays the SymPy import once.

Usage:
    /home/deploy/dev/cadus/.venv/bin/python rewrite_1_0.py [--timeout SECONDS]

# What the script is for, and what it is NOT for

M2 review 3, finding #14: no generator family of `crates/core/tests/answer_oracle.rs`
put a rational expression over a common denominator or split one, so the class-3
agreement measured the generators that were written and not the parity of the two
checkers. This script writes the spellings that family needs.

SymPy GENERATES a spelling here. SymPy never JUDGES a pair. Every verdict of the
harness still comes from `scripts/oracle/check_1_0.py`, which calls the 1.0
checker itself. A spelling this script writes is a learner variant, and 1.0
grades it like any other learner variant.

The one other job is `op: "difference"`, and it is evidence and not a verdict: it
records whether SymPy `cancel` or `radsimp` of `expected - learner` is zero. The
harness needs that fact to name the two documented divergence classes with a
specific predicate, because "the two answers differ" alone names nothing.

# Requests

    {"op": "rewrite", "answer": "2/x + 1/(x + 1)", "id": 7}
    {"op": "difference", "expected": "...", "learner": "...", "id": 8}

`rewrite` answers with the spellings of the six SymPy rules, each printed with
`str()`:

    {"id": 7, "spellings": [{"rule": "together", "text": "(3*x + 2)/(x*(x + 1))"}]}

A rule that raises, or that SymPy refuses for the shape, writes no entry. The
rule order is fixed, so the output is reproducible.

`difference` answers with two flags:

    {"id": 8, "cancel_zero": true, "radsimp_zero": true}

A response keeps the `id` field of its request. A malformed request gets an
`error` field instead, and the filter stays open.

# The wall-clock guard

`factor` and `apart` have no bound on a large polynomial, so every call runs in a
child process under a wall-clock guard, exactly as `check_1_0.py` runs the 1.0
checker. The default guard is 5 seconds. When the worker misses the deadline, the
parent terminates it, starts a new one, and writes `{"timeout": true}`.

The answer text reaches SymPy through the 1.0 rewrite `to_sympy_source` and the
1.0 parser `_parse`, so a spelling starts from the tree 1.0 itself reads.

Read-only. The script imports the 1.0 package; it never writes into it.
"""

from __future__ import annotations

import argparse
import json
import sys

from _filter import WorkerHost, send_error, serve

sys.path.insert(0, "/home/deploy/dev/cadus")

from cadus_web.sympy_check import _parse, to_sympy_source  # noqa: E402

DEFAULT_TIMEOUT_S = 5.0

#: The rewrite rules, in a fixed order. Every one of them is a step a learner
#: performs by hand: put the fractions over one denominator, split them again,
#: cancel a common factor, factor, multiply out, rationalize a radical.
RULES = ("together", "apart", "cancel", "factor", "expand", "radsimp")


def _spellings(text: str) -> list[dict]:
    """Return the SymPy spellings of one answer, under the rules of `RULES`."""
    import sympy

    expression = _parse(to_sympy_source(text))
    out: list[dict] = []
    for rule in RULES:
        function = getattr(sympy, rule)
        try:
            if rule == "apart":
                # `apart` needs one variable, and it raises on anything else.
                symbols = sorted(expression.free_symbols, key=str)
                if len(symbols) != 1:
                    continue
                value = function(expression, symbols[0])
            else:
                value = function(expression)
        except Exception:  # noqa: BLE001 — a rule that refuses a shape writes nothing
            continue
        spelling = str(value)
        if not spelling.strip():
            continue
        out.append({"rule": rule, "text": spelling})
    return out


def _difference(expected: str, learner: str) -> dict:
    """Return whether `cancel` or `radsimp` of the difference is zero."""
    import sympy

    left = _parse(to_sympy_source(expected))
    right = _parse(to_sympy_source(learner))
    difference = left - right
    flags = {"cancel_zero": False, "radsimp_zero": False}
    try:
        flags["cancel_zero"] = bool(sympy.cancel(difference) == 0)
    except Exception:  # noqa: BLE001 — a shape `cancel` refuses is not a zero
        pass
    try:
        flags["radsimp_zero"] = bool(sympy.radsimp(difference) == 0)
    except Exception:  # noqa: BLE001 — a shape `radsimp` refuses is not a zero
        pass
    return flags


def _job(request: dict) -> dict:
    """Run one request in the worker process."""
    operation = request.get("op", "rewrite")
    if operation == "rewrite":
        return {"spellings": _spellings(request["answer"]), "timeout": False}
    if operation == "difference":
        result = _difference(request["expected"], request["learner"])
        result["timeout"] = False
        return result
    raise ValueError(f"unknown op: {operation!r}")


def _worker(connection) -> None:
    """Answer one request per message until the parent closes the pipe."""
    while True:
        try:
            request = connection.recv()
        except (EOFError, KeyboardInterrupt):
            return
        if request is None:
            return
        try:
            result = _job(request)
        except BaseException as exc:  # the parent decides what a failure means
            if not send_error(connection, exc):
                return
            continue
        try:
            connection.send(("ok", result))
        except (BrokenPipeError, OSError):
            return


class Rewriter(WorkerHost):
    """One persistent SymPy worker process, respawned after every timeout."""

    def __init__(self, timeout_s: float) -> None:
        super().__init__(timeout_s, _worker)

    def run(self, request: dict) -> dict:
        """Return the result of one request, under the wall-clock guard."""
        connection = self.connection
        if connection is None:
            self.start()
            connection = self.connection
        if connection is None:
            return {"error": "the rewrite worker did not start"}
        try:
            connection.send(request)
        except (BrokenPipeError, OSError) as exc:
            self.restart()
            return {"error": f"{type(exc).__name__}: {exc}"}
        if not connection.poll(self.timeout_s):
            self.restart()
            return {"spellings": [], "timeout": True}
        try:
            message = connection.recv()
        except (EOFError, OSError) as exc:
            self.restart()
            return {"error": f"the rewrite worker died: {type(exc).__name__}: {exc}"}
        if message[0] == "error":
            return {"error": message[1]}
        return message[1]


def handle(line: str, rewriter: Rewriter) -> dict:
    """Read one request line and return one response object."""
    try:
        request = json.loads(line)
    except ValueError as exc:
        return {"error": f"bad json: {exc}"}
    if not isinstance(request, dict):
        return {"error": "request is not an object"}
    operation = request.get("op", "rewrite")
    if operation == "rewrite" and not isinstance(request.get("answer"), str):
        return {"error": "answer must be a string"}
    if operation == "difference" and not (
        isinstance(request.get("expected"), str)
        and isinstance(request.get("learner"), str)
    ):
        return {"error": "expected and learner must be strings"}
    response = rewriter.run(request)
    if "id" in request:
        response["id"] = request["id"]
    return response


def main() -> int:
    """Run the filter until stdin closes."""
    parser = argparse.ArgumentParser(description="1.0 rational-rewrite generator")
    parser.add_argument(
        "--timeout",
        type=float,
        default=DEFAULT_TIMEOUT_S,
        help="wall-clock guard per request, in seconds (default: 5.0)",
    )
    args = parser.parse_args()
    return serve(Rewriter(args.timeout), handle)


if __name__ == "__main__":
    raise SystemExit(main())
