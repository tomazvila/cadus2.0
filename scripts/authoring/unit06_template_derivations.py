"""Constrain numeric intermediate values so rendered solutions show actual arithmetic."""
from fractions import Fraction
from math import isqrt

FACTORS = {"simplifying-radicals/kp1": 2, "simplifying-radicals/kp2": 5,
           "simplifying-radicals/kp3": 6, "adding-subtracting-radicals/kp2": 2}


def lit(value):
    return {"lit": value}


def mul(*values):
    return {"mul": list(values)}


def power(value, n):
    return mul(*([value]*n))


def enrich(rows):
    """Add a finite, uniquely constrained answer value to numeric root solutions."""
    for row in rows:
        key = row["kp_id"]
        if (relation := relations().get(key)) is None:
            continue
        args = row["arguments"]
        values = []
        for sample in args["samples"]:
            value = intermediate(key, sample)
            value = value.numerator if value.denominator == 1 else str(value)
            sample["params"]["b"] = value
            if value not in values:
                values.append(value)
        args["params"]["b"] = {"kind": "choice", "values": values}
        left, right = relation
        args["constraints"].append({"op": "eq", "left": left, "right": right})
        if key == "cube-roots/kp2":
            # The constraint binds the unique signed cube root. This avoids the
            # evaluator's unevaluated fractional-power spelling in the answer.
            args["answer_expr"] = "b"
        final = {"square-roots/kp2": "{b}/13", "dividing-radicals/kp2": "{b}/5",
                 "rational-exponents/kp2": "1/{b}",
                 "simplifying-radicals/kp1": r"{b}\sqrt2",
                 "simplifying-radicals/kp2": r"3\cdot{b}\sqrt5",
                 "simplifying-radicals/kp3": r"{b}\sqrt6",
                 "adding-subtracting-radicals/kp2": r"({b}+3)\sqrt2"}.get(key, "{b}")
        if key in FACTORS:
            args["solution_sketch"] += " Here $c={b}$, as ${a}=" + str(FACTORS[key]) + r"\cdot{b}^2$."
        args["solution_sketch"] += " The exact result of these steps is $" + final + "$."
    return rows


def intermediate(key, sample):
    a = sample["params"]["a"]
    if key in FACTORS:
        return Fraction(isqrt(a//FACTORS[key]))
    if key == "square-roots/kp2":
        return Fraction(isqrt(a))
    if key == "dividing-radicals/kp2":
        return Fraction(isqrt(a//2))
    if key == "rational-exponents/kp2":
        return Fraction(isqrt(a)**3)
    return Fraction(sample["expected"])


def relations():
    """Polynomial identities pin the intermediate to its independent sample oracle."""
    result = {
        "perfect-square-roots/kp1": ("b", power("a", 2)),
        "perfect-square-roots/kp2": (power("b", 2), "a"),
        "perfect-square-roots/kp3": (power("b", 2), "a"),
        "square-roots/kp1": (power({"sub": ["b", lit(3)]}, 2), "a"),
        "square-roots/kp2": (power("b", 2), "a"),
        "square-roots/kp3": (power("b", 2), mul(lit(9), "a")),
        "cube-roots/kp2": (mul(lit(-1), power("b", 3)), "a"),
        "cube-roots/kp3": (power("b", 4), "a"),
        "estimating-square-roots/kp1": (power("b", 2), {"sub": ["a", lit(1)]}),
        "estimating-square-roots/kp2": (power("b", 2), {"sub": ["a", lit(1)]}),
        "radical-exponent-conversion/kp3": (power("b", 2), "a"),
        "rational-exponents/kp1": (power("b", 2), power("a", 3)),
        "rational-exponents/kp2": (power("b", 2), power("a", 3)),
        "dividing-radicals/kp1": (mul(lit(3), power("b", 2)), "a"),
        "dividing-radicals/kp2": (mul(lit(2), power("b", 2)), "a"),
    }
    result.update({key: (mul(lit(factor), power("b", 2)), "a") for key, factor in FACTORS.items()})
    return result
