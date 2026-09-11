"""Solve graph and application questions from rendered learner text."""
import re

import sympy as s

from check_math import equal, expr

KEYS = {"parabola-vertex-form/kp3", "quadratic-graphs-vertex/kp2",
        "quadratic-applications/kp1", "quadratic-applications/kp2"}
X = s.Symbol("x")


def named(text):
    return {name.strip(): expr(value) for name, value in
            (field.split("=", 1) for field in text.split(";"))}


def validate(key, item):
    math = re.findall(r"\$([^$]*)\$", item["problem"])
    if key == "quadratic-applications/kp1":
        area = expr(math[0])
        w = s.Symbol("w")
        solutions = s.solve(w*(w+5)-area, w)
        want = next(root for root in solutions if root > 0)
        equal(expr(item["answer"]), want)
        return ("width", str(area), "5")
    poly = expr(math[0].split("=", 1)[1])
    if key == "quadratic-applications/kp2":
        t = s.Symbol("t")
        apex = s.solve(s.diff(poly, t), t)[0]
        number, unit = item["answer"].rsplit(" ", 1)
        assert unit == "m"
        equal(expr(number), poly.subs(t, apex))
        return ("height", s.srepr(poly))
    a, b, _ = s.Poly(poly, X).all_coeffs()
    h = -b/(2*a)
    got = named(item["answer"])
    if key == "parabola-vertex-form/kp3":
        assert set(got) == {"x", "y"}
        equal(got["x"], h)
        equal(got["y"], poly.subs(X, 0))
        assert a != 0
    else:
        assert set(got) == {"vertex", "y_intercept"}
        equal(got["vertex"], (h, poly.subs(X, h)))
        equal(got["y_intercept"], (0, poly.subs(X, 0)))
        assert a == 1 and all(root.is_Integer for root in s.solve(poly, X))
    return (key, s.srepr(s.expand(poly)))


def negative(item):
    bad = dict(item)
    if "=" in bad["answer"]:
        first, *rest = bad["answer"].split(";")
        name, value = first.split("=", 1)
        parsed = expr(value)
        if isinstance(parsed, (tuple, s.Tuple)):
            parsed = (parsed[0]+1, parsed[1])
        else:
            parsed += 1
        bad["answer"] = ";".join([name+" = "+str(parsed), *rest])
    elif bad["answer"].endswith(" m"):
        bad["answer"] = bad["answer"][:-2] + " s"
    else:
        bad["answer"] = str(-expr(bad["answer"]))
    return bad
