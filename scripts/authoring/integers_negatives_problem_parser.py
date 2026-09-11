"""Extract evaluable arithmetic from the reviewed integers-negatives wording."""
from __future__ import annotations

import re

import foundations_compute as compute

_MATH = re.compile(r"\$(.+?)\$")


def _single_term_expression(problem: str, terms: list[str]) -> str | None:
    direct = re.match(r"^(?:Evaluate|Find the value of) \$", problem) or re.match(
        r"^What is \$.+\?", problem
    )
    if direct and len(terms) == 1:
        return terms[0]
    if "absolute value" in problem and len(terms) == 1:
        return f"|{terms[0]}|"
    return None


def _sum_expression(problem: str, terms: list[str]) -> str | None:
    if problem.startswith("Add the absolute values of") and len(terms) == 2:
        return f"|{terms[0]}| + |{terms[1]}|"
    if problem.startswith(("Add $", "Combine $")):
        return f"({terms[0]}) + ({terms[1]})"
    if " plus $" in problem or "sum of $" in problem or "are added together" in problem:
        return " + ".join(f"({term})" for term in terms)
    return None


def _difference_expression(problem: str, terms: list[str]) -> str | None:
    reverse = (
        (problem.startswith("Subtract $") and " from $" in problem)
        or problem.startswith("Find the difference when")
        or (problem.startswith("Take $") and " away from $" in problem)
    )
    if reverse and len(terms) == 2:
        return f"({terms[1]}) - ({terms[0]})"
    if problem.startswith("Find $") and " minus $" in problem:
        return f"({terms[0]}) - ({terms[1]})"
    return None


def _product_or_quotient(problem: str, terms: list[str]) -> str | None:
    if problem.startswith(("Find the product of", "Multiply $")):
        if "then subtract" in problem and len(terms) == 3:
            return f"({terms[0]}) * ({terms[1]}) - ({terms[2]})"
        return f"({terms[0]}) * ({terms[1]})"
    quotient = problem.startswith(("Divide $", "Find the quotient of")) or " divided by $" in problem
    if quotient:
        return f"({terms[0]}) / ({terms[1]})"
    return None


def arithmetic_expression(problem: str) -> str | None:
    """Return the arithmetic stated by a supported problem, independent of its answer."""
    if match := compute.match_problem(problem):
        return match.group(2)
    terms = _MATH.findall(problem)
    if not terms:
        return None
    for parser in (_single_term_expression, _sum_expression, _difference_expression, _product_or_quotient):
        if expression := parser(problem, terms):
            return expression
    if problem.startswith("Find $|") and "and add" in problem:
        return f"({terms[0]}) + ({terms[1]})"
    return None
