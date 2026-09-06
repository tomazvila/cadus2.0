"""Extract evaluable arithmetic from the reviewed integers-negatives wording."""
from __future__ import annotations

import re

import foundations_compute as compute

_MATH = re.compile(r"\$(.+?)\$")


def arithmetic_expression(problem: str) -> str | None:
    """Return the arithmetic stated by a supported problem, independent of its answer."""
    if match := compute.match_problem(problem):
        return match.group(2)
    terms = _MATH.findall(problem)
    if not terms:
        return None
    if re.match(r"^(?:Evaluate|Find the value of) \$", problem) or re.match(r"^What is \$.+\?", problem):
        if len(terms) == 1:
            return terms[0]
    if "absolute value" in problem and len(terms) == 1:
        return f"|{terms[0]}|"
    if problem.startswith("Add the absolute values of") and len(terms) == 2:
        return f"|{terms[0]}| + |{terms[1]}|"
    if problem.startswith("Subtract $") and " from $" in problem and len(terms) == 2:
        return f"({terms[1]}) - ({terms[0]})"
    if problem.startswith("Add $") or problem.startswith("Combine $"):
        return f"({terms[0]}) + ({terms[1]})"
    if " plus $" in problem or "sum of $" in problem:
        return " + ".join(f"({term})" for term in terms)
    if "are added together" in problem:
        return " + ".join(f"({term})" for term in terms)
    if problem.startswith("Find the difference when") and len(terms) == 2:
        return f"({terms[1]}) - ({terms[0]})"
    if problem.startswith("Find $") and " minus $" in problem:
        return f"({terms[0]}) - ({terms[1]})"
    if problem.startswith("Take $") and " away from $" in problem:
        return f"({terms[1]}) - ({terms[0]})"
    if problem.startswith("Find the product of") or problem.startswith("Multiply $"):
        if "then subtract" in problem and len(terms) == 3:
            return f"({terms[0]}) * ({terms[1]}) - ({terms[2]})"
        return f"({terms[0]}) * ({terms[1]})"
    if problem.startswith("Divide $") or " divided by $" in problem or problem.startswith("Find the quotient of"):
        return f"({terms[0]}) / ({terms[1]})"
    if problem.startswith("Find $|") and "and add" in problem:
        return f"({terms[0]}) + ({terms[1]})"
    return None
