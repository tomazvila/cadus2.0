#!/usr/bin/env python3
"""Per-KP recipes for the 8 mastery-floor-root topics of `00-arithmetic-core`.

Each recipe below names ONE knowledge point, encodes numbers that satisfy
THAT knowledge point's own authored `constraints` text, and computes both the
answer and the solution sketch from the real numbers (`foundations_compute`,
`foundations_number_theory`, `foundations_arith_sketch`) — never a hand-typed
value. `test_apply_arithmetic_core_recipes_facts.py` re-derives every value
independently from the recipe's own problem text and checks it against that
knowledge point's own constraint.

Default is a dry run: reports what it would change and writes nothing. Pass
`--write` to edit `curriculum/foundations/00-arithmetic-core.yaml` in place.
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

import foundations_arith_sketch as sk
import foundations_number_theory as nt
from foundations_curriculum_patch import (
    ExemplarKey,
    KpKey,
    NewExemplar,
    Rejection,
    apply_solution_sketches,
    insert_exemplars,
)

UNIT_FILE = "curriculum/foundations/00-arithmetic-core.yaml"


def _sum(a: int, b: int) -> NewExemplar:
    return NewExemplar(f"Compute ${a} + {b}$.", str(a + b), sk.make_ten_sketch(a, b), True)


def _diff(a: int, b: int) -> NewExemplar:
    return NewExemplar(f"Compute ${a} - {b}$.", str(a - b), sk.count_back_sketch(a, b), True)


def _bridge_diff(a: int, b: int) -> NewExemplar:
    return NewExemplar(f"Compute ${a} - {b}$.", str(a - b), sk.bridge_back_sketch(a, b), True)


def _times(a: int, b: int) -> NewExemplar:
    sketch = sk.partial_products_sketch(a, b, decompose="second" if b > a else "first")
    return NewExemplar(f"Compute ${a} \\times {b}$.", str(a * b), sketch, True)


def _div(a: int, b: int) -> NewExemplar:
    q = a // b
    return NewExemplar(
        f"Compute ${a} \\div {b}$.", str(q), f"Undo multiplication: ${b} \\times {q} = {a}$.", True
    )


def _missing_factor(known: int, product: int) -> NewExemplar:
    other = product // known
    return NewExemplar(
        f"Solve: $\\square \\times {known} = {product}$.",
        str(other),
        f"The missing factor is ${product} \\div {known} = {other}$.",
        False,
    )


def _square(n: int) -> NewExemplar:
    return NewExemplar(f"Compute ${n}^2$.", str(n * n), f"${n}^2 = {n} \\times {n} = {n * n}$.", True)


def _square_check(n: int, is_square: bool) -> NewExemplar:
    value = n * n if is_square else n
    label = "yes" if is_square else "no"
    if is_square:
        sketch = f"${n} \\times {n} = {value}$."
    else:
        lo = int(n**0.5)
        while (lo + 1) ** 2 <= n:
            lo += 1
        hi = lo + 1
        sketch = f"${lo}^2={lo * lo}$ and ${hi}^2={hi * hi}$; ${n}$ lies strictly between them, so it is not a perfect square."
    problem = f"Is ${value}$ a perfect square? (yes/no)"
    return NewExemplar(problem, label, sketch, False, '{"kind":"label","options":[["yes"],["no"]]}')


def _digit_value(n: int, place_index: int) -> NewExemplar:
    digit, value = nt.digit_at_place(n, place_index)
    place = nt.place_name(place_index)
    problem = f"What is the value of the ${digit}$ in ${nt.format_grouped(n)}$?"
    sketch = f"The ${digit}$ sits in the {place} place, so its value is ${digit} \\times {10 ** place_index} = {value}$."
    return NewExemplar(problem, str(value), sketch, False)


def _expanded(n: int) -> NewExemplar:
    problem = f"Write ${nt.format_grouped(n)}$ in expanded form."
    answer = nt.expanded_form(n)
    return NewExemplar(problem, answer, f"Add the value of each nonzero digit: ${answer} = {n}$.", False)


def _compare(a: int, b: int) -> NewExemplar:
    winner = max(a, b)
    sa, sb = str(a), str(b)
    if len(sa) != len(sb):
        sketch = f"${nt.format_grouped(winner)}$ has more digits than ${nt.format_grouped(min(a, b))}$, so it is larger."
    else:
        pos = next(i for i in range(len(sa)) if sa[i] != sb[i])
        sketch = f"Digits match up to position {pos + 1}; there ${sa[pos]} {'>' if sa[pos] > sb[pos] else '<'} {sb[pos]}$ decides it."
    return NewExemplar(f"Which is larger, ${nt.format_grouped(a)}$ or ${nt.format_grouped(b)}$?", str(winner), sketch, False)


def _order(values: list[int]) -> NewExemplar:
    ordered = sorted(values)
    listed = ", ".join(f"${nt.format_grouped(v)}$" for v in values)
    problem = f"Order from least to greatest: {listed}."
    answer = ", ".join(str(v) for v in ordered)
    chain = " < ".join(nt.format_grouped(v) for v in ordered)
    return NewExemplar(problem, answer, f"Comparing place by place: ${chain}$.", False)


def _round(n: int, place_index: int) -> NewExemplar:
    rounded = nt.round_to(n, place_index)
    place = nt.place_name(place_index)
    digit_below, _ = nt.digit_at_place(n, place_index - 1)
    direction = "up" if digit_below >= 5 else "down (keep the digit)"
    problem = f"Round ${nt.format_grouped(n)}$ to the nearest {place[:-1] if place != 'ones' else place}."
    sketch = (
        f"The digit to the right of the {place} place is ${digit_below}$, so round {direction}: "
        f"${nt.format_grouped(rounded)}$."
    )
    return NewExemplar(problem, str(rounded), sketch, False)


MISSING_SKETCHES: dict[ExemplarKey, str] = {
    ExemplarKey("single-digit-addition", "kp1", 0): sk.make_ten_sketch(3, 4),
    ExemplarKey("single-digit-addition", "kp1", 1): sk.make_ten_sketch(6, 2),
    ExemplarKey("single-digit-addition", "kp1", 2): sk.make_ten_sketch(5, 4),
    ExemplarKey("single-digit-addition", "kp2", 1): sk.make_ten_sketch(9, 6),
    ExemplarKey("single-digit-addition", "kp2", 2): sk.make_ten_sketch(7, 5),
    ExemplarKey("subtraction-facts", "kp1", 0): sk.count_back_sketch(9, 4),
    ExemplarKey("subtraction-facts", "kp1", 1): sk.count_back_sketch(7, 3),
    ExemplarKey("subtraction-facts", "kp1", 2): sk.count_back_sketch(10, 6),
    ExemplarKey("subtraction-facts", "kp2", 1): sk.bridge_back_sketch(13, 6),
    ExemplarKey("subtraction-facts", "kp2", 2): sk.bridge_back_sketch(14, 6),
    ExemplarKey("multiplication-tables", "kp1", 0): sk.partial_products_sketch(6, 7, decompose="second"),
    ExemplarKey("multiplication-tables", "kp1", 1): sk.partial_products_sketch(8, 4, decompose="second"),
    ExemplarKey("multiplication-tables", "kp1", 2): sk.partial_products_sketch(9, 6, decompose="second"),
    ExemplarKey("multiplication-tables", "kp2", 0): sk.partial_products_sketch(12, 7, decompose="first"),
    ExemplarKey("multiplication-tables", "kp2", 1): sk.partial_products_sketch(11, 8, decompose="first"),
    ExemplarKey("division-facts", "kp1", 0): "Undo multiplication: $6 \\times 7 = 42$.",
    ExemplarKey("division-facts", "kp1", 1): "Undo multiplication: $9 \\times 7 = 63$.",
    ExemplarKey("division-facts", "kp1", 2): "Undo multiplication: $8 \\times 4 = 32$.",
    ExemplarKey("division-facts", "kp2", 1): "Undo multiplication: $7 \\times 8 = 56$.",
    ExemplarKey("division-facts", "kp2", 2): "The missing factor is $54 \\div 9 = 6$.",
    ExemplarKey("perfect-squares", "kp1", 0): "$7^2 = 7 \\times 7 = 49$.",
    ExemplarKey("perfect-squares", "kp1", 1): "$9^2 = 9 \\times 9 = 81$.",
    ExemplarKey("perfect-squares", "kp1", 2): "$12^2 = 12 \\times 12 = 144$.",
    ExemplarKey("perfect-squares", "kp2", 1): "$11^2 = 121$.",
    ExemplarKey("perfect-squares", "kp2", 2): "$7^2=49$ and $8^2=64$; $50$ lies strictly between them, so it is not a perfect square.",
    ExemplarKey("place-value", "kp1", 0): "The $7$ sits in the tens place, so its value is $7 \\times 10 = 70$.",
    ExemplarKey("place-value", "kp1", 1): "Reading left to right, $3852$ has digits thousands=$3$, hundreds=$8$, tens=$5$, ones=$2$; the hundreds digit is $8$.",
    ExemplarKey("place-value", "kp2", 0): "The $5$ sits in the thousands place, so its value is $5 \\times 1000 = 5000$.",
    ExemplarKey("comparing-ordering-whole-numbers", "kp1", 1): "Same digit count (three); compare hundreds: $3 < 4$, so $389 < 401$.",
    ExemplarKey("comparing-ordering-whole-numbers", "kp2", 0): "Comparing hundreds then tens: $153 < 315 < 351$.",
    ExemplarKey("comparing-ordering-whole-numbers", "kp2", 1): "All three share the thousands digit $2$; comparing hundreds digits ($0$, $8$, $0$), $2{,}870$ has the largest and is greatest.",
    ExemplarKey("rounding-whole-numbers", "kp1", 0): "The ones digit is $7 (\\geq 5)$, so round up to $50$.",
    ExemplarKey("rounding-whole-numbers", "kp1", 1): "The ones digit is $3 (< 5)$, so round down to $80$.",
    ExemplarKey("rounding-whole-numbers", "kp2", 0): "The hundreds digit is $6 (\\geq 5)$, so round up to $3{,}000$.",
    ExemplarKey("rounding-whole-numbers", "kp2", 2): "The hundreds digit is $5 (\\geq 5)$, so round up to $5{,}000$.",
}


RECIPES: dict[KpKey, list[NewExemplar]] = {
    KpKey("single-digit-addition", "kp1"): [_sum(2, 8)],
    KpKey("single-digit-addition", "kp2"): [_sum(9, 9)],
    KpKey("subtraction-facts", "kp1"): [_diff(8, 8)],
    KpKey("subtraction-facts", "kp2"): [_bridge_diff(17, 9)],
    KpKey("multiplication-tables", "kp1"): [_times(10, 10)],
    KpKey("multiplication-tables", "kp2"): [_times(12, 12), _times(11, 2)],
    KpKey("division-facts", "kp1"): [_div(100, 10)],
    KpKey("division-facts", "kp2"): [_missing_factor(8, 64)],
    KpKey("perfect-squares", "kp1"): [_square(1)],
    KpKey("perfect-squares", "kp2"): [_square_check(12, True)],
    KpKey("place-value", "kp1"): [_digit_value(196, 1), _digit_value(7249, 3)],
    KpKey("place-value", "kp2"): [_digit_value(63041, 3), _expanded(30208)],
    KpKey("comparing-ordering-whole-numbers", "kp1"): [_compare(6284, 6248), _compare(512, 4999)],
    KpKey("comparing-ordering-whole-numbers", "kp2"): [
        _order([5316, 5361, 5136]),
        _order([712, 89, 1205, 698]),
    ],
    KpKey("rounding-whole-numbers", "kp1"): [_round(267, 2)],
    KpKey("rounding-whole-numbers", "kp2"): [_round(9500, 3)],
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
