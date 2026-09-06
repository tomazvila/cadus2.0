"""Check authored parameter answers against independently transcribed source data."""
import json
from pathlib import Path

import sympy as s

from check_math import equal, expr
from check_parameter_math import coefficient_polynomial, negative
from check_residual_structured import named

ROOT = Path(__file__).resolve().parents[3]
X = s.Symbol("x")
ROOTS = {
    "writing-quadratics-from-roots/kp1": [(3, -5), (2, 4), (5, -2), (-4, -7)],
    "writing-quadratics-from-roots/kp2": [(4, 4), (0, -6), (-3, -3), (0, 5)],
    "writing-quadratics-from-roots/kp3": [(s.Rational(1, 2), -3), (s.Rational(2, 3), 1), (s.Rational(1, 4), -2), (s.Rational(3, 5), 1)],
}
POLYS = {
    "converting-to-vertex-form/kp1": [X**2+6*X+5, X**2-4*X+7, X**2+8*X+10, X**2-10*X+21],
    "converting-to-vertex-form/kp2": [2*X**2-8*X+3, -X**2+2*X+3, 3*X**2-12*X+5, -2*X**2-8*X+1],
    "converting-to-vertex-form/kp3": [X**2+10*X+30, 3*X**2+6*X+1, X**2+12*X+40, 2*X**2+4*X+5],
    "applying-the-quadratic-formula/kp1": [3*X**2-5*X+2, X**2-4*X+3, 4*X**2+3*X-1, X**2-3*X+5],
}


def check(key, index, item):
    if key in ROOTS:
        poly = coefficient_polynomial(item["answer"])
        roots = ROOTS[key][index]
        equal(s.Poly(poly, X).monic().as_expr(), s.prod(X-root for root in roots))
        cs = s.Poly(poly, X).all_coeffs()
        assert all(c.is_Integer for c in cs) and s.igcd(*cs) == 1
        if not key.endswith("kp3"):
            assert cs[0] == 1
        return
    if key == "completing-the-square/kp1":
        linear = [8, -6, 10, -4][index]
        got = named(item["answer"])
        equal((X+got["shift"])**2, X**2+linear*X+got["constant"])
        return
    if key == "applying-the-quadratic-formula/kp1":
        equal(coefficient_polynomial(item["answer"]), POLYS[key][index])
        return
    if key.endswith("kp3"):
        got = named(item["answer"])
        a, h, k = got["vertex_parameters"]
        if "minimum" in got:
            assert a > 0
            equal(got["minimum"], k)
        else:
            equal(got["vertex"], (h, k))
    elif key.endswith("kp1"):
        h, k = expr(item["answer"])
        a = 1
    else:
        a, h, k = expr(item["answer"])
    equal(a*(X-h)**2+k, POLYS[key][index])


def main():
    rows = json.loads((ROOT / "target/unit07/facts.json").read_text())["kps"]
    checked = 0
    for row in rows:
        key = row["kp_key"]
        if key not in ROOTS and key not in POLYS and key != "completing-the-square/kp1":
            continue
        assert len(row["exemplars"]) == 4
        for index, item in enumerate(row["exemplars"]):
            check(key, index, item)
            try:
                check(key, index, negative(item))
            except AssertionError:
                checked += 1
            else:
                raise AssertionError(("authored negative control accepted", key, index))
    assert checked == 32
    print("Authored reconstruction: 32 exact answers; 32 perturbed answers rejected")


if __name__ == "__main__":
    main()
