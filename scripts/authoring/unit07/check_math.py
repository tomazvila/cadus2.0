"""Independently solve the rendered production instances, using their question text."""
import json
import re
from functools import reduce
from pathlib import Path

import sympy as s
from sympy.parsing.sympy_parser import parse_expr, standard_transformations, implicit_multiplication_application

ROOT = Path(__file__).resolve().parents[3]
X = s.Symbol("x")


def expr(text):
    text = text.strip().replace("^", "**")
    return parse_expr(text, transformations=standard_transformations + (implicit_multiplication_application,))


def polynomial(text):
    if "=" not in text:
        return expr(text)
    left, right = text.split("=", 1)
    if left in ("p(x)", "y", "C"):
        return expr(right)
    return expr(left) - expr(right)


def equal(got, want):
    if isinstance(want, (tuple, s.Tuple)):
        assert isinstance(got, (tuple, s.Tuple)) and len(got) == len(want), (got, want)
        for left, right in zip(got, want):
            equal(left, right)
    else:
        assert s.simplify(got - want) == 0, (got, want)


def basics(key, math):
    if key == "polynomial-basics/kp1":
        poly = s.Poly(polynomial(math[0]), X)
        return (poly.degree(), poly.LC())
    if key == "polynomial-basics/kp3":
        return sum(map(expr, math[:3]))
    if key.startswith("evaluating-polynomials/"):
        if key.endswith("kp3"):
            return polynomial(math[0]).subs(s.Symbol("n"), expr(math[-1].split("=")[1]))
        return polynomial(math[1]).subs(X, expr(math[0]))
    return None


def arithmetic(key, math):
    topic, kp = key.split("/")
    if topic == "adding-polynomials":
        return sum(map(expr, math[:2])) if kp != "kp3" else expr(math[0])
    if topic == "polynomial-addition-subtraction":
        return sum(map(expr, math[:2])) - expr(math[2]) if kp == "kp3" else expr(math[1])-expr(math[0])
    if topic == "dividing-polynomials-by-monomials":
        return expr(math[0])
    if key == "special-products/kp1":
        return expr(math[0])**2
    if topic in ("multiplying-monomials-polynomials", "multiplying-binomials", "polynomial-multiplication", "special-products", "binomial-cubes"):
        return s.prod(map(expr, math))
    if topic == "polynomial-division":
        raw = parse_expr(math[0].replace("^", "**"), evaluate=False, transformations=standard_transformations + (implicit_multiplication_application,))
        numerator, denominator = s.fraction(raw)
        quotient, remainder = s.div(numerator, denominator, X)
        if kp != "kp2":
            assert remainder == 0
        return (quotient, remainder) if kp == "kp2" else quotient
    if topic == "synthetic-division":
        quotient, remainder = s.div(polynomial(math[0]), expr(math[1]), X)
        return remainder if kp == "kp3" else (quotient, remainder) if kp == "kp2" else quotient
    if topic == "gcf-of-monomials":
        return reduce(s.gcd, map(expr, math))
    return None


def solve(key, math):
    topic = key.split("/")[0]
    root_topics = ("zero-product-property", "quadratic-equations-factoring", "square-root-property", "completing-the-square", "completing-square-leading-coefficient", "applying-the-quadratic-formula", "quadratic-formula")
    if topic in root_topics:
        poly = polynomial(math[0])
        if key == "quadratic-formula/kp3":
            return len(s.polys.polytools.intervals(poly, eps=s.Rational(1,100)))
        roots = s.solve(poly, X)
        return tuple(sorted(roots, key=lambda root: float(root)))
    if topic == "discriminant":
        value = s.discriminant(polynomial(math[0]), X)
        return value if key.endswith("kp1") else 1+s.sign(value)
    if topic in ("parabola-vertex-form", "quadratic-graphs-vertex"):
        poly = s.Poly(polynomial(math[0]), X)
        a, b, _ = poly.all_coeffs()
        h = -b/(2*a)
        return (h, poly.eval(h))
    return None


def check(row, item):
    key = row["kp_id"]
    math = re.findall(r"\$([^$]*)\$", item["problem"])
    got = s.sympify(item["answer"])
    factoring = ("factoring-", "difference-of-squares", "perfect-square-trinomials", "sum-difference-of-cubes", "quadratics-in-form", "choosing-factoring-strategy")
    if key.startswith(factoring):
        equal(s.expand(got), expr(math[0]))
        assert got.is_Mul or got.is_Pow, (key, got)
        return
    want = basics(key, math)
    if want is None:
        want = arithmetic(key, math)
    if want is None:
        want = solve(key, math)
    assert want is not None, f"No independent verifier for {key}"
    equal(got, want)


def main():
    rows = json.loads((ROOT / "docs/reports/unit07-template-evidence.json").read_text())
    checked = 0
    for row in rows:
        for item in row["instances"]:
            try:
                check(row, item)
            except Exception as error:
                raise AssertionError((row["kp_id"], item["problem"], item["answer"])) from error
            checked += 1
    print(f"Independently solved {checked} rendered instances across {len(rows)} templates")


if __name__ == "__main__":
    main()
