"""Reconstruct full equations from parameter responses and compare exact math."""
import re

import sympy as s

from check_math import equal, expr
from check_residual_structured import named

X = s.Symbol("x")
KEYS = {"writing-quadratics-from-roots/kp1", "writing-quadratics-from-roots/kp2",
        "writing-quadratics-from-roots/kp3", "completing-the-square/kp1",
        "applying-the-quadratic-formula/kp1", "converting-to-vertex-form/kp1",
        "converting-to-vertex-form/kp2", "converting-to-vertex-form/kp3"}


def coefficient_polynomial(answer):
    values = expr(answer)
    assert isinstance(values, (tuple, s.Tuple)) and len(values) == 3
    a, b, c = values
    assert a > 0
    return a*X**2+b*X+c


def root_equation(key, item, math):
    poly = coefficient_polynomial(item["answer"])
    roots = list(expr(math[0])) if key.endswith("kp2") else [expr(math[0]), expr(math[1])]
    want = s.prod(X-root for root in roots)
    equal(s.Poly(poly, X).monic().as_expr(), want)
    coefficients = s.Poly(poly, X).all_coeffs()
    if key.endswith("kp3"):
        assert all(c.is_Integer for c in coefficients) and s.igcd(*coefficients) == 1
    else:
        assert coefficients[0] == 1
    return ("root-equation", s.srepr(s.expand(poly)))


def vertex_components(key,item):
    if key.endswith("kp3"):
        got=named(item["answer"])
        a,h,k=got["vertex_parameters"]
        return a,h,k,got
    if key.endswith("kp1"):
        h,k=expr(item["answer"])
        return s.Integer(1),h,k,None
    a,h,k=expr(item["answer"])
    return a,h,k,None


def vertex_form(key, item, math):
    poly = expr(math[0].split("=", 1)[1])
    a,h,k,got=vertex_components(key,item)
    if got is not None:
        assert a > 0
        equal(got["minimum"], k)
    equal(a*(X-h)**2+k, poly)
    equal(s.diff(poly, X).subs(X, h), 0)
    equal(poly.subs(X, h), k)
    return ("vertex-conversion", s.srepr(s.expand(poly)))


def validate(key, item):
    math = re.findall(r"\$([^$]*)\$", item["problem"])
    if key.startswith("writing-quadratics-from-roots/"):
        return root_equation(key, item, math)
    if key.startswith("converting-to-vertex-form/"):
        return vertex_form(key, item, math)
    if key == "completing-the-square/kp1":
        poly = expr(math[0].split("=")[0])
        got = named(item["answer"])
        assert set(got) == {"constant", "shift"}
        equal((X+got["shift"])**2, poly+got["constant"])
        return (key, s.srepr(poly))
    left, right = math[0].split("=")
    source = expr(left)-expr(right)
    equal(coefficient_polynomial(item["answer"]), source)
    return (key, s.srepr(s.expand(source)))


def negative(item):
    bad = dict(item)
    if "=" in item["answer"]:
        got = named(item["answer"])
        key = next(iter(got))
        value = got[key]
        got[key] = tuple([value[0]+1, *value[1:]]) if isinstance(value, (tuple, s.Tuple)) else value+1
        bad["answer"] = "; ".join(f"{name} = {value}" for name, value in got.items())
    else:
        value = list(expr(item["answer"]))
        value[-1] += 1
        bad["answer"] = str(tuple(value))
    return bad
