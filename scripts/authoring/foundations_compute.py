"""Deterministic LaTeX arithmetic: parse, evaluate and re-render.

The module reads one authored `problem` string of the family
`(Compute|Calculate|Evaluate|Simplify) $<expr>$.` and evaluates `<expr>` as
exact rational arithmetic, using only Python's own expression grammar as a
safe intermediate (never `eval`): the LaTeX text is rewritten into a small,
whitelisted Python expression, parsed with `ast.parse`, and walked by a
recursive evaluator that accepts only the node kinds this module writes
itself. No draft this module helps build ever asks a model for an answer;
every value here is exact `fractions.Fraction` arithmetic.

The module never invents an expression outside a KP's own family: every new
operand it proposes reuses the shape of an authored exemplar and only varies
the numbers, so the generated content stays inside the knowledge point's
`constraints` line by construction (`same_shape_new_operands`).
"""
from __future__ import annotations

import ast
import random
import re
from dataclasses import dataclass
from fractions import Fraction
from typing import Callable, Optional

PROBLEM_RE = re.compile(r"^(Compute|Calculate|Evaluate|Simplify) \$(.+)\$\.$")
_LETTER_OK = re.compile(r"\\(frac|dfrac|times|div|cdot|left|right)\b")
_DECIMAL = re.compile(r"(?<![\w.])\d+\.\d+(?![\w.])")
_MIXED = re.compile(r"(\d+)\\frac\{(\d+)\}\{(\d+)\}")
_FRAC = re.compile(r"\\d?frac\{([^{}]+)\}\{([^{}]+)\}")
_BAR = re.compile(r"\|([^|]*)\|")
_CARET_BRACED = re.compile(r"\^\{([^{}]+)\}")


class NotArithmetic(ValueError):
    """The problem text is outside the pure-numeric arithmetic family."""


def match_problem(problem: str) -> Optional[re.Match]:
    """The `(verb, expr)` match of a `Compute $expr$.`-shaped problem, or `None`."""
    return PROBLEM_RE.match(problem.strip())


def is_pure_numeric(expr: str) -> bool:
    """Whether `expr` holds no letters beyond the whitelisted LaTeX commands."""
    return not re.search(r"[a-zA-Z]", _LETTER_OK.sub("", expr))


def _insert_implicit_mult(text: str) -> str:
    """`)(`, `)A`, digit-`(` and digit-`A` each name a product with no operator."""
    return re.sub(r"(?<=[0-9)])\s*(?=[(A])", "*", text)


def to_python_expr(expr: str) -> str:
    """Rewrite one authored LaTeX arithmetic expression into a Python expression.

    The result names only integer and string literals, `+ - * / **`, bare
    parentheses, and three call names this module's evaluator alone reads:
    `D` (an exact decimal literal), `MX` (a mixed number) and `AB` (absolute
    value). Nothing else survives the rewrite, so [`evaluate`] can walk the
    parsed tree without ever running arbitrary code.
    """
    text = expr.replace(r"\left", "").replace(r"\right", "")
    text = text.replace("{,}", "")
    text = _MIXED.sub(r"MX(\1,\2,\3)", text)
    # Run twice: a mixed-number replacement can leave a `\frac` inside the
    # `MX(...)` call unaffected, but a plain `\frac` never nests one more.
    for _ in range(2):
        text = _FRAC.sub(r"((\1)/(\2))", text)
    text = text.replace(r"\times", "*").replace(r"\cdot", "*").replace(r"\div", "/")
    text = _CARET_BRACED.sub(r"**(\1)", text)
    text = text.replace("^", "**")
    text = _DECIMAL.sub(lambda m: f"D('{m.group(0)}')", text)
    text = _BAR.sub(r"AB(\1)", text)
    text = _insert_implicit_mult(text)
    return text


_BINOPS = {
    ast.Add: lambda a, b: a + b,
    ast.Sub: lambda a, b: a - b,
    ast.Mult: lambda a, b: a * b,
    ast.Div: lambda a, b: a / b,
}


def _exact_integer_root(value: int, degree: int) -> int:
    """The exact integer `n`th root of a nonnegative integer, or raise."""
    if value < 0:
        raise NotArithmetic(f"no real {degree}th root of a negative value")
    if value in (0, 1):
        return value
    root = round(value ** (1 / degree))
    for candidate in (root - 1, root, root + 1):
        if candidate >= 0 and candidate**degree == value:
            return candidate
    raise NotArithmetic(f"{value} has no exact integer {degree}th root")


def _exact_rational_power(base: Fraction, exponent: Fraction) -> Fraction:
    """`base ** exponent` for a rational exponent, only when the result is exact.

    A radical the curriculum authors as a rational exponent (`8^{2/3}`,
    `4^{-1/2}`) always resolves to an exact rational value in this KP family,
    so a root that leaves a remainder is outside the grammar, not rounded.
    """
    degree = exponent.denominator
    power = abs(exponent.numerator)
    rooted = Fraction(
        _exact_integer_root(base.numerator**power, degree),
        _exact_integer_root(base.denominator**power, degree),
    )
    return 1 / rooted if exponent < 0 else rooted


def _eval_node(node: ast.AST) -> Fraction:
    if isinstance(node, ast.Expression):
        return _eval_node(node.body)
    if isinstance(node, ast.Constant) and isinstance(node.value, int) and not isinstance(node.value, bool):
        return Fraction(node.value)
    if isinstance(node, ast.BinOp):
        if isinstance(node.op, ast.Pow):
            base = _eval_node(node.left)
            exponent = _eval_node(node.right)
            if exponent.denominator != 1:
                return _exact_rational_power(base, exponent)
            return base ** exponent.numerator
        handler = _BINOPS.get(type(node.op))
        if handler is None:
            raise NotArithmetic(f"unsupported operator: {ast.dump(node.op)}")
        return handler(_eval_node(node.left), _eval_node(node.right))
    if isinstance(node, ast.UnaryOp):
        value = _eval_node(node.operand)
        if isinstance(node.op, ast.USub):
            return -value
        if isinstance(node.op, ast.UAdd):
            return value
        raise NotArithmetic(f"unsupported unary operator: {ast.dump(node.op)}")
    if isinstance(node, ast.Call) and isinstance(node.func, ast.Name):
        name = node.func.id
        if node.keywords:
            raise NotArithmetic("keyword arguments are not arithmetic")
        args = node.args
        if name == "D" and len(args) == 1 and isinstance(args[0], ast.Constant) and isinstance(args[0].value, str):
            return Fraction(args[0].value)
        if name == "AB" and len(args) == 1:
            return abs(_eval_node(args[0]))
        if name == "MX" and len(args) == 3:
            whole, num, den = (_eval_node(a) for a in args)
            return whole + num / den
        raise NotArithmetic(f"unsupported call: {name}")
    raise NotArithmetic(f"unsupported syntax: {ast.dump(node)}")


def evaluate(expr: str) -> Fraction:
    """The exact value of one authored LaTeX arithmetic expression.

    Raises [`NotArithmetic`] for anything outside the whitelisted grammar —
    never guesses, never falls back to a float.
    """
    python_expr = to_python_expr(expr)
    try:
        tree = ast.parse(python_expr, mode="eval")
    except SyntaxError as error:
        raise NotArithmetic(f"{expr!r} -> {python_expr!r}: {error}") from error
    return _eval_node(tree)


def _terminating_decimal(value: Fraction) -> Optional[str]:
    """The exact fixed-point decimal text of `value`, or `None` if it never terminates.

    A denominator with a prime factor other than 2 or 5 never reaches a
    terminating decimal (e.g. `1/3`), so this returns `None` there — never a
    rounded guess.
    """
    sign = "-" if value < 0 else ""
    magnitude = abs(value)
    denominator = magnitude.denominator
    factor_counts = []
    for factor in (2, 5):
        count = 0
        while denominator % factor == 0:
            denominator //= factor
            count += 1
        factor_counts.append(count)
    if denominator != 1:
        return None
    places = max(factor_counts)
    scaled = magnitude * (10**places)
    digits = str(scaled.numerator).rjust(places + 1, "0")
    if places == 0:
        return f"{sign}{digits}"
    return f"{sign}{digits[:-places]}.{digits[-places:]}"


def render_answer(value: Fraction, *, prefer_decimal: bool = False) -> str:
    """The canonical answer text of an exact value: a decimal, an integer, or `a/b`.

    `prefer_decimal` renders a terminating value as a fixed-point decimal
    (e.g. `2.65`, matching the authored style of a knowledge point whose own
    problem already carries a decimal point) instead of `a/b` — an
    already-authored decimal exercise never wants its own generated answer
    to surface as an unreduced improper fraction.
    """
    if prefer_decimal:
        decimal = _terminating_decimal(value)
        if decimal is not None:
            return decimal
    if value.denominator == 1:
        return str(value.numerator)
    return f"{value.numerator}/{value.denominator}"


@dataclass(frozen=True)
class Operand:
    """One integer leaf of an expression template, with its textual span."""

    start: int
    end: int
    text: str
    value: int


_OPERAND = re.compile(r"(?<![\d.])-?\d+(?!\d*\.\d)")


def integer_operands(expr: str) -> list[Operand]:
    """Every bare integer literal of `expr`, left to right.

    A decimal's whole part is excluded by the trailing lookahead, and a
    mixed number's whole part is excluded because [`_MIXED_SPAN`] masks it
    before the scan — a generator that only varies bare integers never
    corrupts a decimal or a mixed number by construction.
    """
    masked = _DECIMAL.sub(lambda m: "#" * len(m.group(0)), expr)
    return [
        Operand(m.start(), m.end(), m.group(0), int(m.group(0)))
        for m in _OPERAND.finditer(masked)
    ]


def same_shape_new_operands(
    expr: str,
    rng: random.Random,
    *,
    forbid_zero_result: bool = False,
    forbid_values: frozenset[Fraction] = frozenset(),
    extra_ok: Optional[Callable[[str, Fraction], bool]] = None,
    operand_ceiling: Optional[int] = None,
    attempts: int = 500,
) -> tuple[str, Fraction]:
    """One new expression of the identical shape as `expr`, with fresh operands.

    Every bare integer literal is redrawn, independently, from a range no
    more than three times the original operand's own size (so the search has
    room to satisfy `extra_ok` below), and the sign is kept when the
    original operand carried a leading `-`. The rewrite touches only bare
    integers: a decimal's digits are left untouched by [`integer_operands`].

    `extra_ok`, when given, is a further caller-supplied check on the
    candidate text and its value — the caller's tool for a rule this module
    cannot see on its own, such as "the value must stay inside the range
    this knowledge point already serves" or "this shape divides evenly". It
    is this module's real defense against a bigger or smaller item than the
    knowledge point's author intended, NOT the operand range, which stays
    wide on purpose so a tight `extra_ok` still has room to be satisfied.
    `operand_ceiling`, when given, additionally caps every drawn operand's
    SIZE (never its sign) — the caller's way to stop, for example, a
    fraction's denominator from drifting past every denominator its
    knowledge point's own exemplars ever used, a shape `extra_ok` alone
    cannot see since it only reads the final value. Retries up to `attempts`
    times for a value that parses (never raising
    `ZeroDivisionError` out of this function; a redraw that divides by zero
    is just one more failed attempt), is not in `forbid_values` (so a
    generated worked example never lands on an answer this knowledge point
    already serves), optionally avoids a zero result, and satisfies
    `extra_ok`.
    """
    operands = integer_operands(expr)
    if not operands:
        raise NotArithmetic("no bare integer operand to vary")
    for _ in range(attempts):
        pieces = []
        cursor = 0
        for operand in operands:
            pieces.append(expr[cursor : operand.start])
            magnitude = max(2, abs(operand.value) * 3)
            if operand_ceiling is not None:
                magnitude = min(magnitude, max(2, operand_ceiling))
            draw = rng.randint(1, magnitude)
            if operand.value < 0:
                draw = -draw
            pieces.append(str(draw))
            cursor = operand.end
        pieces.append(expr[cursor:])
        candidate = "".join(pieces)
        if candidate == expr:
            continue
        try:
            value = evaluate(candidate)
        except (NotArithmetic, ZeroDivisionError):
            continue
        if forbid_zero_result and value == 0:
            continue
        if value in forbid_values:
            continue
        if extra_ok is not None and not extra_ok(candidate, value):
            continue
        return candidate, value
    raise NotArithmetic(f"no fresh operand set found for {expr!r} after {attempts} draws")


def parse_answer_text(text: str) -> Fraction:
    """Read an authored `answer` field of the numeric family: int, `a/b`, `w n/d`, or a decimal."""
    text = text.strip()
    if " " in text and "/" in text:
        whole, frac = text.split(" ", 1)
        num, den = frac.split("/", 1)
        sign = -1 if whole.startswith("-") else 1
        return sign * (abs(Fraction(whole)) + Fraction(int(num), int(den)))
    if "/" in text:
        num, den = text.split("/", 1)
        return Fraction(int(num), int(den))
    return Fraction(text)
