"""Independent exact computation for the 16 retained graph/application examples."""
import json
from pathlib import Path

import sympy as s

from check_math import equal, expr
from check_residual_structured import named

ROOT = Path(__file__).resolve().parents[3]
X = s.Symbol("x")


def graphs(facts):
    axes = [((X-2)**2-1, 0), (-(X+1)**2+4, 1), ((X-1)**2+2, 3), (-(X-2)**2+5, 4)]
    vertices = [X**2-4*X+3, X**2+2*X-8, X**2-6*X+8, X**2-2*X-3]
    for item, (poly, at) in zip(facts["parabola-vertex-form/kp3"], axes):
        if "=" in item["answer"]:
            got = named(item["answer"])
            a, b, _ = s.Poly(poly, X).all_coeffs()
            equal(got["x"], -b/(2*a))
            equal(got["y"], poly.subs(X, at))
        else:
            equal(expr(item["answer"]), poly.subs(X, at))
    for item, poly in zip(facts["quadratic-graphs-vertex/kp2"], vertices):
        got = named(item["answer"])
        roots = sorted(s.solve(poly, X))
        h = sum(roots)/2
        equal(got["vertex"], (h, poly.subs(X, h)))
        if "y_intercept" in got:
            equal(got["y_intercept"], (0, poly.subs(X, 0)))
        else:
            equal(got["left_intercept"], (roots[0], 0))
            equal(got["right_intercept"], (roots[1], 0))


def applications(facts):
    for index, (item, area, gap) in enumerate(zip(facts["quadratic-applications/kp1"], [40, 56, 45, 90], [3, 1, 4, 1])):
        root = next(r for r in s.solve(X*(X+gap)-area, X) if r > 0)
        if index in (0, 2):
            equal(expr(item["answer"]), root)
        else:
            got = named(item["answer"])
            equal(got["smaller"], root)
            equal(got["larger"], root+1)
    for index, (item, b) in enumerate(zip(facts["quadratic-applications/kp2"], [10, 30, 20, 40])):
        poly = -5*X**2+b*X
        value, unit = item["answer"].split()
        if index in (0, 2):
            want = max(s.solve(poly, X))
            assert unit == "s"
        else:
            want = poly.subs(X, s.solve(s.diff(poly, X), X)[0])
            assert unit == "m"
        equal(expr(value), want)


def main():
    rows = json.loads((ROOT / "target/unit07/facts.json").read_text())["kps"]
    facts = {r["kp_key"]: r["exemplars"] for r in rows}
    graphs(facts)
    applications(facts)
    print("Independent structured authored checks: 16 correct answers")


if __name__ == "__main__":
    main()
