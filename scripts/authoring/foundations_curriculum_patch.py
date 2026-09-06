"""Add a missing `solution_sketch` to an authored exemplar, line by line.

Follows the convention of `foundations_visuals.py`: the curriculum YAML is
never parsed and rewritten as a whole document (no library, no reordering, no
reformatting of anything this module does not touch) — it is scanned line by
line for the exact indentation this repository's authors already use, and
only a missing `solution_sketch:` line is ever inserted. Every other byte of
every other line is untouched, so a diff shows only the lines this module
adds.

A [`Rejection`] refuses the whole file before any line is written when a
generated sketch is not representable as a plain single-quoted YAML scalar
(it would need escaping this module does not implement), or when a
requested `(topic, kp, exemplar_index)` key is never found.
"""
from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path

_TOPIC_ID = re.compile(r"^  - id: ([a-z0-9-]+)\n$")
_KP_ID = re.compile(r"^      - id: (kp\d+)\n$")
_PROBLEM = re.compile(r"^          - problem: ")
_EXEMPLAR_FIELD = re.compile(r"^            \w+:")
_SOLUTION_SKETCH = re.compile(r"^            solution_sketch:")


class Rejection(ValueError):
    """The patch refuses before any line is written."""


def _quoted(text: str) -> str:
    if "'" in text:
        raise Rejection(f"generated sketch holds a literal quote, unescaped: {text!r}")
    return f"'{text}'"


@dataclass(frozen=True)
class ExemplarKey:
    topic_id: str
    kp_id: str
    exemplar_index: int


def apply_solution_sketches(
    path: Path, sketches: dict[ExemplarKey, str], *, write: bool
) -> tuple[str, list[ExemplarKey]]:
    """Insert a `solution_sketch:` line for every key of `sketches` that lacks one.

    Returns `(new_text, applied_keys)`. Raises [`Rejection`] and touches no
    line at all when any key of `sketches` is absent from the file, or when a
    key that DOES need a sketch already carries one whose text this module
    did not itself write (an author-written sketch is never replaced).
    """
    lines = path.read_text().splitlines(keepends=True)
    topic_id = None
    kp_id = None
    exemplar_index = -1
    output: list[str] = []
    applied: list[ExemplarKey] = []
    remaining = dict(sketches)
    index = 0
    while index < len(lines):
        line = lines[index]
        if match := _TOPIC_ID.match(line):
            topic_id = match.group(1)
            kp_id = None
        elif match := _KP_ID.match(line):
            kp_id = match.group(1)
            exemplar_index = -1
        elif _PROBLEM.match(line):
            exemplar_index += 1
        output.append(line)
        index += 1
        if not _PROBLEM.match(line) or topic_id is None or kp_id is None:
            continue
        key = ExemplarKey(topic_id, kp_id, exemplar_index)
        if key not in remaining:
            continue
        block_has_sketch = False
        cursor = index
        while cursor < len(lines) and _EXEMPLAR_FIELD.match(lines[cursor]):
            if _SOLUTION_SKETCH.match(lines[cursor]):
                block_has_sketch = True
            cursor += 1
        if block_has_sketch:
            raise Rejection(f"{key} already carries an authored solution_sketch")
        while index < cursor:
            output.append(lines[index])
            index += 1
        output.append(f"            solution_sketch: {_quoted(remaining.pop(key))}\n")
        applied.append(key)
    if remaining:
        raise Rejection(f"never found in {path}: {sorted(remaining)}")
    text = "".join(output)
    if write:
        path.write_text(text)
    return text, applied
