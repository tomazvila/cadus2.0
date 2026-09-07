"""Independent answers reconstructed from rendered learner-visible premises."""
import re
import string

import sympy as s
from sympy.parsing.sympy_parser import (
    convert_xor, implicit_multiplication_application, parse_expr,
    standard_transformations,
)

TRANSFORMS = standard_transformations + (implicit_multiplication_application, convert_xor)


def math(text):
    letters = {name: s.Symbol(name) for name in string.ascii_letters}
    return parse_expr(text, transformations=TRANSFORMS, local_dict=letters)


def reconstruct(key, problem):
    topic = key.split("/")[0]
    blocks = re.findall(r"\$([^$]+)\$", problem)
    if topic in ("literal-equations", "rearranging-formulas"):
        left, right = blocks[0].split("=")
        answer, = s.solve(math(left) - math(right), s.Symbol("x"))
        return str(s.factor(answer))
    if topic == "parts-of-an-expression":
        expression = math(blocks[0])
        terms = len(s.Add.make_args(expression))
        coefficient = expression.coeff(s.Symbol("y"))
        constant = expression.subs({v: 0 for v in expression.free_symbols})
        return f"terms = {terms}; coefficient = {coefficient}; constant = {constant}"
    if topic == "substituting-values":
        expression = math(blocks[0])
        name, value = blocks[1].split("=")
        return str(expression.subs(s.Symbol(name), math(value)))
    if topic == "translating-phrases-to-expressions":
        return translate_phrase(problem, blocks)
    if topic == "translating-sentences-to-equations":
        multiplier, subtrahend, total = map(math, blocks[1:])
        # Enumerate integers from the visible equation; do not reuse the recipe formula.
        return str(next(n for n in range(-1000, 1001) if multiplier*n-subtrahend == total))
    if topic == "writing-expressions-from-patterns":
        fee, rate = map(int, re.findall(r"€(\d+)", problem))
        variable = math(blocks[0])
        totals = [fee + sum([rate]*n) for n in (0, 1)]
        return str(totals[0] + (totals[1] - totals[0])*variable)
    raise ValueError(f"No independent reconstruction for {key}: {problem}")


def translate_phrase(problem, blocks):
    if "difference" in problem:
        first, second = map(math, blocks)
        return str(first-second)
    if "quotient" in problem:
        numerator, denominator = map(math, blocks)
        return str(numerator/denominator)
    if "less than" in problem:
        amount, multiplier, variable = map(math, blocks)
        return str(s.Add(s.Mul(multiplier, variable), -amount))
    raise ValueError(f"Unsupported phrase: {problem}")
