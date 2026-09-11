#!/usr/bin/env python3
"""Per-KP recipes for the multi-digit add/subtract/multiply topics of
`00-arithmetic-core` (addition-with-carrying, subtraction-with-borrowing,
multi-digit-addition-subtraction, addition-subtraction-word-problems,
multiplying-by-one-digit, multi-digit-multiplication).

Every recipe names ONE knowledge point and picks numbers that satisfy that
knowledge point's own authored `constraints` text (a carry count, a borrow
count, "no zeros in the minuend", digit counts); the answer and the sketch
are computed from those numbers by `foundations_arith_sketch`, never typed
by hand. `test_apply_arithmetic_core_recipes_multidigit.py` re-derives every
value independently.

Default is a dry run: reports what it would change and writes nothing. Pass
`--write` to edit `curriculum/foundations/00-arithmetic-core.yaml` in place.
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

import foundations_arith_sketch as sk
from foundations_curriculum_patch import (
    ExemplarKey,
    KpKey,
    NewExemplar,
    Rejection,
    apply_solution_sketches,
    insert_exemplars,
)

UNIT_FILE = "curriculum/foundations/00-arithmetic-core.yaml"


def _add(a: int, b: int) -> NewExemplar:
    return NewExemplar(f"Compute ${a} + {b}$.", str(a + b), sk.column_add_sketch(a, b), True)


def _sub(a: int, b: int) -> NewExemplar:
    return NewExemplar(f"Compute ${a} - {b}$.", str(a - b), sk.column_subtract_sketch(a, b), True)


def _check_sub(a: int, b: int) -> NewExemplar:
    result = a - b
    sketch = f"${a} - {b} = {result}$; check: ${b} + {result} = {a}$."
    return NewExemplar(f"Compute ${a} - {b}$, then verify by addition.", str(result), sketch, False)


def _three_sum(a: int, b: int, c: int) -> NewExemplar:
    ab = a + b
    total = ab + c
    sketch = f"${a} + {b} = {ab}$; ${ab} + {c} = {total}$."
    return NewExemplar(f"Compute ${a} + {b} + {c}$.", str(total), sketch, True)


def _add_then_sub(a: int, b: int, c: int) -> NewExemplar:
    step = a - b
    total = step + c
    sketch = f"${a} - {b} = {step}$; ${step} + {c} = {total}$."
    return NewExemplar(f"Compute ${a} - {b} + {c}$.", str(total), sketch, True)


def _sub_group(a: int, b: int, c: int) -> NewExemplar:
    group = b + c
    total = a - group
    sketch = f"${b} + {c} = {group}$; ${a} - {group} = {total}$."
    return NewExemplar(f"Compute ${a} - ({b} + {c})$.", str(total), sketch, True)


def _one_step_word(prompt: str, answer: int, sketch: str) -> NewExemplar:
    return NewExemplar(prompt, str(answer), sketch, False)


def _mult1(a: int, b: int, *, decompose: str = "first") -> NewExemplar:
    sketch = sk.partial_products_sketch(a, b, decompose=decompose)
    return NewExemplar(f"Compute ${a} \\times {b}$.", str(a * b), sketch, True)


def _near_round(a: int, b: int, *, a_near: int) -> NewExemplar:
    delta = a_near - a
    assert delta != 0
    near_total = a_near * b
    adjust = delta * b
    total = near_total - adjust if delta > 0 else near_total + abs(adjust)
    assert total == a * b
    sign = "minus" if delta > 0 else "plus"
    sketch = f"${a_near} \\times {b} = {near_total}$, {sign} ${abs(delta)} \\times {b} = {abs(adjust)}$: ${total}$."
    return NewExemplar(f"Compute ${a} \\times {b}$ mentally.", str(total), sketch, False)


def _mult2(a: int, b: int) -> NewExemplar:
    sketch = sk.partial_products_sketch(a, b, decompose="second")
    return NewExemplar(f"Compute ${a} \\times {b}$.", str(a * b), sketch, True)


def _mult2_estimate(a: int, b: int, a_round: int, b_round: int) -> NewExemplar:
    total = a * b
    estimate = a_round * b_round
    sketch = f"Estimate ${a_round} \\times {b_round} = {estimate}$; exact: ${a} \\times {b} = {total}$."
    prompt = f"Compute ${a} \\times {b}$ and check that the answer is close to ${a_round} \\times {b_round}$."
    return NewExemplar(prompt, str(total), sketch, False)


MISSING_SKETCHES: dict[ExemplarKey, str] = {
    ExemplarKey("addition-with-carrying", "kp1", 1): sk.column_add_sketch(56, 27),
    ExemplarKey("addition-with-carrying", "kp2", 1): sk.column_add_sketch(385, 247),
    ExemplarKey("addition-with-carrying", "kp3", 0): "$25 + 37 = 62$; $62 + 18 = 80$.",
    ExemplarKey("addition-with-carrying", "kp3", 1): "$123 + 45 = 168$; $168 + 206 = 374$.",
    ExemplarKey("subtraction-with-borrowing", "kp1", 1): sk.column_subtract_sketch(61, 28),
    ExemplarKey("subtraction-with-borrowing", "kp2", 0): sk.column_subtract_sketch(435, 187),
    ExemplarKey("subtraction-with-borrowing", "kp2", 1): sk.column_subtract_sketch(612, 345),
    ExemplarKey("subtraction-with-borrowing", "kp3", 1): sk.column_subtract_sketch(725, 468),
    ExemplarKey("multi-digit-addition-subtraction", "kp1", 0): sk.column_add_sketch(1867, 3589),
    ExemplarKey("multi-digit-addition-subtraction", "kp1", 1): sk.column_add_sketch(4675, 2848),
    ExemplarKey("multi-digit-addition-subtraction", "kp2", 1): sk.column_subtract_sketch(700, 356),
    ExemplarKey("multi-digit-addition-subtraction", "kp2", 2): sk.column_subtract_sketch(4003, 1567),
    ExemplarKey("multi-digit-addition-subtraction", "kp3", 0): "$350 + 275 = 625$; $625 - 125 = 500$.",
    ExemplarKey("addition-subtraction-word-problems", "kp1", 0):
        "$1{,}240 + 385 = 1{,}625$.",
    ExemplarKey("addition-subtraction-word-problems", "kp1", 1):
        "$4{,}500 - 3{,}862 = 638$.",
    ExemplarKey("addition-subtraction-word-problems", "kp2", 1):
        "$88 + 95 = 183$; $364 - 183 = 181$.",
    ExemplarKey("addition-subtraction-word-problems", "kp3", 1):
        "$3{,}776 - 2{,}917 = 859$.",
    ExemplarKey("multiplying-by-one-digit", "kp1", 1): sk.partial_products_sketch(57, 8, decompose="first"),
    ExemplarKey("multiplying-by-one-digit", "kp2", 0): sk.partial_products_sketch(234, 4, decompose="first"),
    ExemplarKey("multi-digit-multiplication", "kp1", 1): sk.partial_products_sketch(68, 24, decompose="second"),
    ExemplarKey("multi-digit-multiplication", "kp2", 0): sk.partial_products_sketch(125, 12, decompose="second"),
    ExemplarKey("multi-digit-multiplication", "kp2", 2): sk.partial_products_sketch(342, 27, decompose="second"),
}


RECIPES: dict[KpKey, list[NewExemplar]] = {
    KpKey("addition-with-carrying", "kp1"): [_add(29, 43), _add(38, 54)],
    KpKey("addition-with-carrying", "kp2"): [_add(469, 357), _add(578, 296)],
    KpKey("addition-with-carrying", "kp3"): [_three_sum(46, 72, 139), _three_sum(208, 356, 94)],
    KpKey("subtraction-with-borrowing", "kp1"): [_sub(74, 38), _sub(93, 56)],
    KpKey("subtraction-with-borrowing", "kp2"): [_sub(542, 378), _sub(651, 284)],
    KpKey("subtraction-with-borrowing", "kp3"): [_check_sub(83, 56), _check_sub(604, 378)],
    KpKey("multi-digit-addition-subtraction", "kp1"): [_add(2986, 4757), _add(3458, 2967)],
    KpKey("multi-digit-addition-subtraction", "kp2"): [_sub(800, 347)],
    KpKey("multi-digit-addition-subtraction", "kp3"): [
        _add_then_sub(960, 275, 140),
        _sub_group(1250, 480, 275),
    ],
    KpKey("addition-subtraction-word-problems", "kp1"): [
        _one_step_word(
            "A warehouse stores $3{,}450$ boxes and ships $1{,}275$. How many boxes remain?",
            3450 - 1275, "$3{,}450 - 1{,}275 = 2{,}175$.",
        ),
        _one_step_word(
            "A concert hall seats $2{,}800$ people; $1{,}950$ tickets are sold. How many seats are still available?",
            2800 - 1950, "$2{,}800 - 1{,}950 = 850$.",
        ),
    ],
    KpKey("addition-subtraction-word-problems", "kp2"): [
        _one_step_word(
            "Ben had $180$ marbles, lost $45$, then won $60$ more. How many does he have now?",
            180 - 45 + 60, "$180 - 45 = 135$; $135 + 60 = 195$.",
        ),
        _one_step_word(
            "A shop has $340$ shirts. It sells $128$ on Monday and receives a delivery of $75$. How many shirts does it have now?",
            340 - 128 + 75, "$340 - 128 = 212$; $212 + 75 = 287$.",
        ),
    ],
    KpKey("addition-subtraction-word-problems", "kp3"): [
        _one_step_word(
            "Nora has $246$ stickers, which is $59$ fewer than Elena has. How many stickers does Elena have?",
            246 + 59, "$59$ fewer than Elena means Elena has $246 + 59 = 305$.",
        ),
        _one_step_word(
            "River A is $1{,}885$ km long and River B is $2{,}340$ km long. How much shorter is River A?",
            2340 - 1885, "$2{,}340 - 1{,}885 = 455$.",
        ),
    ],
    KpKey("multiplying-by-one-digit", "kp1"): [_mult1(46, 7), _mult1(83, 9)],
    KpKey("multiplying-by-one-digit", "kp2"): [_mult1(326, 5), _mult1(408, 7)],
    KpKey("multiplying-by-one-digit", "kp3"): [
        _near_round(69, 4, a_near=70),
        _near_round(199, 8, a_near=200),
    ],
    KpKey("multi-digit-multiplication", "kp1"): [_mult2(57, 36), _mult2(82, 19)],
    KpKey("multi-digit-multiplication", "kp2"): [_mult2(463, 18)],
    KpKey("multi-digit-multiplication", "kp3"): [
        NewExemplar(
            "Compute $37 \\times 52$ using partial products.",
            str(37 * 52),
            sk.partial_products_sketch(37, 52, decompose="second"),
            False,
        ),
        _mult2_estimate(63, 48, 60, 50),
    ],
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--curriculum", default=UNIT_FILE)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    path = Path(args.curriculum)

    try:
        _, sketched = apply_solution_sketches(path, MISSING_SKETCHES, write=args.write)
    except Rejection as error:
        print(f"REFUSED (solution sketches): {error}", file=sys.stderr)
        return 1
    verb = "written" if args.write else "planned"
    print(f"{len(sketched)} missing solution_sketch line(s) {verb}")

    try:
        _, applied = insert_exemplars(path, RECIPES, write=args.write)
    except Rejection as error:
        print(f"REFUSED (new exemplars): {error}", file=sys.stderr)
        return 1
    count = sum(len(RECIPES[key]) for key in applied)
    print(f"{len(applied)} knowledge point(s), {count} new exemplar(s) {verb}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
