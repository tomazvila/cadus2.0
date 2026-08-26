#!/usr/bin/env python
"""Answer corpus dump for Cadus 2.0 milestone M2 (IDs V1-V4).

One JSON line per exemplar answer on a `numeric` or `expression` topic, with the
form 1.0's SymPy checker actually computes for it.

Usage:
    /home/deploy/dev/cadus/.venv/bin/python dump_answers_1_0.py \
        [CURRICULUM_DIR] [OUT.jsonl]

Defaults: /home/deploy/dev/cadus2.0/curriculum and ./answer_corpus.jsonl.

Fields per line:
    topic_id, kp_id ("<diagnostic>" for the topic's diagnostic exemplar),
    exemplar_index (-1 for the diagnostic), answer_kind, answer (verbatim),
    normalized (`sympy_check._normalize`), sympy_source (`to_sympy_source`),
    canonical (`str(parse_expr(...))`, None on failure),
    canonical_srepr (`sympy.srepr`, None on failure),
    parsed (bool), self_equivalent (`_sympy_equivalent(src, src)`),
    shape (the V1 grammar classification), error (parse failure text).

Read-only. It imports the 1.0 package; it never writes into it.
"""

from __future__ import annotations

import json
import re
import sys
from collections import Counter, defaultdict

sys.path.insert(0, "/home/deploy/dev/cadus")

from cadus.graph import load_curriculum  # noqa: E402
from cadus.model import AnswerKind  # noqa: E402
from cadus_web.sympy_check import (  # noqa: E402
    _normalize,
    _parse,
    _sympy_equivalent,
    to_sympy_source,
)

VERIFIABLE = {AnswerKind.numeric, AnswerKind.expression}

# --------------------------------------------------------------------------- #
# Shape classification — the candidate V1 grammar, most specific rule first.
# --------------------------------------------------------------------------- #

_INT = r"[+-]?\d+"
_DEC = r"[+-]?(?:\d+\.\d*|\.\d+)"

_RULES: list[tuple[str, re.Pattern[str]]] = [
    ("integer", re.compile(rf"^{_INT}$")),
    ("decimal", re.compile(rf"^{_DEC}$")),
    ("grouped_integer", re.compile(r"^[+-]?\d{1,3}(?:[,\u00a0\u202f\u2009\u2007 ]\d{3})+$")),
    ("percent", re.compile(rf"^(?:{_DEC}|{_INT})\s*%$")),
    ("fraction", re.compile(rf"^{_INT}\s*/\s*{_INT}$")),
    ("mixed_number", re.compile(rf"^{_INT}\s+\d+\s*/\s*\d+$")),
    ("ordered_tuple", re.compile(r"^\(.*,.*\)$")),
    ("set_or_list", re.compile(r"^[\{\[].*[\}\]]$")),
    ("interval_ineq", re.compile(r"(?:<=|>=|≤|≥|<|>)")),
    ("equation", re.compile(r"^[^<>=]*=[^=].*$")),
    ("comma_list", re.compile(r"^[^(){}\[\]]+,[^(){}\[\]]+$")),
]

#: Multi-letter identifiers that are MATHS, not English. Anything else of three or
#: more letters makes the answer prose (a unit name, a word answer, a hint).
_MATH_WORDS = {
    "sqrt", "cbrt", "abs", "exp", "log", "ln", "sin", "cos", "tan", "sec", "csc",
    "cot", "asin", "acos", "atan", "sinh", "cosh", "tanh", "pi", "oo", "zoo",
    "theta", "alpha", "beta", "gamma", "lamda", "lambda", "phi", "mu", "nu",
    "min", "max", "sum", "prod", "mod", "gcd", "lcm", "det", "deg", "rad",
    "interval", "union", "matrix", "and", "or", "not", "cdot", "times", "frac",
    "left", "right", "text", "circ", "infty", "le", "ge", "pm", "mp",
}
_WORD_RE = re.compile(r"[A-Za-z]{3,}")
_UNICODE_MATH = re.compile(r"[√π∞·−–≤≥θαβλ½⅓⅔¼¾°⁰¹²³⁴⁵⁶⁷⁸⁹×÷]")

#: Whole-answer word replies too short for :data:`_WORD_RE` to see.
_SHORT_WORDS = {
    "no", "up", "in", "on", "ok", "i", "ii", "iii", "iv", "v", "vi", "aa", "sss",
    "sas", "asa", "hl", "ll", "cm", "mm", "km", "ft", "kg", "mg", "ml",
}

#: Unit tokens the curriculum appends to a value ("7 L/min", "2π cm^2").
_UNIT_RE = re.compile(
    r"(?:^|[\s)])(?:cm|mm|km|m|ft|in|yd|mi|kg|g|mg|lb|oz|L|mL|s|min|h|hr|"
    r"cm\^?[23²³]|m\^?[23²³]|km/h|m/s|L/min|mph|°C|°F|°)\s*$"
)

#: `9 R2`, `23 R14`, `x + 2 remainder 3` — a quotient with a remainder.
_QUOT_REM_RE = re.compile(r"(?:\bR\s*\d+\s*$)|(?:\bremainder\b)")


def _is_prose(body: str) -> bool:
    """True when the answer carries an English word rather than only maths."""
    stripped = re.sub(r"\\[a-zA-Z]+", " ", body)
    if any(w.lower() not in _MATH_WORDS for w in _WORD_RE.findall(stripped)):
        return True
    return body.strip().lower() in _SHORT_WORDS


def classify(raw: str, source: str | None, value: object) -> str:
    """Bucket one authored answer string by surface shape.

    ``source``/``value`` are the ``to_sympy_source`` rewrite and the parsed SymPy
    object, used only to split a bare expression into "numeric-valued" (no free
    symbols) and "has variables".
    """
    s = raw.strip()
    if not s:
        return "empty"
    body = s
    if body.startswith("$") and body.endswith("$") and len(body) > 1:
        body = body[1:-1].strip()
    body = body.rstrip(".").strip()
    for name, pat in _RULES[:6]:
        if pat.match(body):
            return name
    if _QUOT_REM_RE.search(body):
        return "quotient_remainder"
    if _UNIT_RE.search(body) and re.search(r"\d", body):
        return "value_with_unit"
    for name, pat in _RULES[6:]:
        hit = pat.search(body) if name == "interval_ineq" else pat.match(body)
        if hit:
            return name
    if _is_prose(body):
        return "prose_or_words"
    free = getattr(value, "free_symbols", None)
    if free is not None:
        return "expression_symbolic" if free else "expression_numeric"
    if _UNICODE_MATH.search(body) or re.search(r"[A-Za-z]", body):
        return "expression_symbolic"
    return "other"


def rows(curriculum: str):
    graph = load_curriculum(curriculum)
    for tid in sorted(graph.topics):
        topic = graph.topics[tid]
        if topic.answer_kind not in VERIFIABLE:
            continue
        items = [
            (kp.id, i, ex.answer)
            for kp in topic.knowledge_points
            for i, ex in enumerate(kp.exemplars)
        ]
        if topic.diagnostic_exemplar is not None:
            items.append(("<diagnostic>", -1, topic.diagnostic_exemplar.answer))
        for kp_id, idx, answer in items:
            raw = str(answer or "")
            row: dict[str, object] = {
                "topic_id": tid,
                "kp_id": kp_id,
                "exemplar_index": idx,
                "answer_kind": str(topic.answer_kind),
                "answer": raw,
            }
            try:
                src = to_sympy_source(raw)
            except Exception as exc:  # noqa: BLE001
                row |= {
                    "normalized": None, "sympy_source": None, "canonical": None,
                    "canonical_srepr": None, "parsed": False, "self_equivalent": False,
                    "shape": classify(raw, None, None),
                    "error": f"{type(exc).__name__}: {exc}",
                }
                yield row
                continue
            row["normalized"] = _normalize(raw)
            row["sympy_source"] = src
            value: object = None
            try:
                from sympy import srepr

                value = _parse(src)
                row["canonical"] = str(value)
                try:
                    row["canonical_srepr"] = srepr(value)
                except Exception:  # noqa: BLE001
                    row["canonical_srepr"] = None
                row["parsed"] = True
                row["error"] = None
            except Exception as exc:  # noqa: BLE001
                row |= {
                    "canonical": None, "canonical_srepr": None, "parsed": False,
                    "error": f"{type(exc).__name__}: {exc}",
                }
            row["shape"] = classify(raw, src, value)
            try:
                row["self_equivalent"] = bool(_sympy_equivalent(src, src))
            except Exception:  # noqa: BLE001
                row["self_equivalent"] = False
            yield row


def main(argv: list[str]) -> int:
    curriculum = argv[1] if len(argv) > 1 else "/home/deploy/dev/cadus2.0/curriculum"
    out = argv[2] if len(argv) > 2 else "answer_corpus.jsonl"
    shapes: Counter[str] = Counter()
    examples: dict[str, list[str]] = defaultdict(list)
    total = parsed = self_eq = 0
    failures: list[dict[str, object]] = []
    topics: set[str] = set()
    with open(out, "w", encoding="utf-8") as fh:
        for row in rows(curriculum):
            total += 1
            topics.add(str(row["topic_id"]))
            shapes[str(row["shape"])] += 1
            if len(examples[str(row["shape"])]) < 5:
                examples[str(row["shape"])].append(str(row["answer"]))
            if row["parsed"]:
                parsed += 1
            else:
                failures.append(row)
            if row["self_equivalent"]:
                self_eq += 1
            fh.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n")

    print(f"topics: {len(topics)}   answers: {total}", file=sys.stderr)
    print(f"parsed: {parsed}   failed: {total - parsed}", file=sys.stderr)
    print(f"self_equivalent (reaches the SymPy rung): {self_eq}", file=sys.stderr)
    print("\nshape counts:", file=sys.stderr)
    for shape, n in shapes.most_common():
        pct = 100.0 * n / total
        print(f"  {shape:24s} {n:5d}  {pct:5.1f}%", file=sys.stderr)
        for ex in examples[shape]:
            print(f"      {ex!r}", file=sys.stderr)
    print("\nparse failures:", file=sys.stderr)
    for row in failures[:60]:
        print(
            f"  {row['topic_id']}.{row['kp_id']}[{row['exemplar_index']}] "
            f"{row['answer']!r} -> {row['error']}",
            file=sys.stderr,
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
