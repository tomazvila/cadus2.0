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


def _track_topic_kp(line: str, topic_id: str | None, kp_id: str | None) -> tuple[str | None, str | None]:
    """The `(topic_id, kp_id)` state after reading one more line of the file."""
    if match := _TOPIC_ID.match(line):
        return match.group(1), None
    if match := _KP_ID.match(line):
        return topic_id, match.group(1)
    return topic_id, kp_id


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
        previous_kp_id = kp_id
        topic_id, kp_id = _track_topic_kp(line, topic_id, kp_id)
        if kp_id != previous_kp_id:
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


@dataclass(frozen=True)
class KpKey:
    topic_id: str
    kp_id: str


@dataclass(frozen=True)
class NewExemplar:
    """One exemplar block to append; `with_contract` copies the sibling convention.

    `contract_override`, when given, is the exact `answer_contract` JSON
    text to write instead (for a family such as `quotient_remainder` whose
    contract differs exemplar to exemplar, e.g. by its own divisor) —
    additive and optional, so every existing caller that only ever sets
    `with_contract` is untouched.
    """

    problem: str
    answer: str
    solution_sketch: str
    with_contract: bool
    contract_override: str | None = None


def _render_exemplar(exemplar: NewExemplar) -> list[str]:
    for text in (exemplar.problem, exemplar.solution_sketch):
        if "'" in text:
            raise Rejection(f"generated exemplar text holds a literal quote: {text!r}")
    if '"' in exemplar.answer:
        raise Rejection(f"generated answer holds a literal quote: {exemplar.answer!r}")
    lines = [
        f"          - problem: '{exemplar.problem}'\n",
        f'            answer: "{exemplar.answer}"\n',
    ]
    if exemplar.contract_override is not None:
        lines.append(f"            answer_contract: {exemplar.contract_override}\n")
    elif exemplar.with_contract:
        lines.append('            answer_contract: {"kind":"exact"}\n')
    lines.append(f"            solution_sketch: '{exemplar.solution_sketch}'\n")
    return lines


def insert_exemplars(
    path: Path, additions: dict[KpKey, list[NewExemplar]], *, write: bool
) -> tuple[str, list[KpKey]]:
    """Append every one of `additions[key]` right after that KP's last exemplar.

    The KP's own last exemplar decides whether `answer_contract` is written
    for the new ones too (`NewExemplar.with_contract` is set by the caller to
    match). Raises [`Rejection`] and touches no line at all when any key of
    `additions` is never found.
    """
    lines = path.read_text().splitlines(keepends=True)
    topic_id = None
    kp_id = None
    output: list[str] = []
    applied: list[KpKey] = []
    remaining = dict(additions)
    index = 0
    while index < len(lines):
        line = lines[index]
        topic_id, kp_id = _track_topic_kp(line, topic_id, kp_id)
        output.append(line)
        index += 1
        if not _PROBLEM.match(line) or topic_id is None or kp_id is None:
            continue
        cursor = index
        while cursor < len(lines) and _EXEMPLAR_FIELD.match(lines[cursor]):
            cursor += 1
        while index < cursor:
            output.append(lines[index])
            index += 1
        if cursor < len(lines) and _PROBLEM.match(lines[cursor]):
            continue  # not this KP's last exemplar yet
        key = KpKey(topic_id, kp_id)
        if key not in remaining:
            continue
        for exemplar in remaining.pop(key):
            output.extend(_render_exemplar(exemplar))
        applied.append(key)
    if remaining:
        raise Rejection(f"never found in {path}: {sorted(remaining)}")
    text = "".join(output)
    if write:
        path.write_text(text)
    return text, applied
