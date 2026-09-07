"""Semantic and representation checks independent of recipe answer expressions."""
import copy
import json
import re
from pathlib import Path

import sympy as s

from check_math import equal, expr
import check_residual_structured as structured
import check_parameter_math as parameters

ROOT = Path(__file__).resolve().parents[3]
X = s.Symbol("x")
REPAIRED = {
    "factoring-gcf/kp2", "difference-of-squares/kp2", "difference-of-squares/kp3",
    "perfect-square-trinomials/kp3", "quadratics-in-form/kp2",
    "choosing-factoring-strategy/kp2", "choosing-factoring-strategy/kp3",
    "quadratic-formula/kp3",
    "sum-difference-of-cubes/kp1", "sum-difference-of-cubes/kp2",
    "sum-difference-of-cubes/kp3",
}

REPAIRED |= structured.KEYS | parameters.KEYS


def factors(tree):
    if tree.is_Mul:
        return [part for child in tree.args for part in factors(child)]
    if tree.is_Pow and tree.exp.is_Integer and tree.exp > 0:
        return factors(tree.base)
    return [tree] if tree.has(X) else []


def validate_gcf(item,source,answer):
    assert answer.is_Mul and s.prod(f for f in answer.args if f.is_number) < 0, item
    assert answer.is_Mul and len(factors(answer)) > 1, item
    terms = s.Poly(source, X).terms()
    content = s.igcd(*[coefficient for _, coefficient in terms])
    degree = min(power[0] for power, _ in terms)
    quotient = s.cancel(source / (-content * X**degree))
    assert s.Poly(quotient, X).LC() > 0
    assert any(s.expand(factor) == quotient for factor in factors(answer)), item


def validate_factor_shape(key,item,answer):
    if key == "perfect-square-trinomials/kp3":
        assert answer.is_Pow and answer.exp == 2, item
        assert s.Poly(answer.base, X).degree() == 1, item
        return
    pieces = factors(answer)
    assert len(pieces) >= 2, item
    for factor in pieces:
        _, decomposed = s.factor_list(factor, X)
        assert len(decomposed) == 1 and decomposed[0][1] == 1, item


def validate_factoring(key,item,source,answer):
    equal(s.expand(answer), source)
    if key == "factoring-gcf/kp2":
        validate_gcf(item,source,answer)
    else:
        validate_factor_shape(key,item,answer)
    return (s.srepr(s.expand(source)), s.srepr(s.expand(answer)))


def validate(key, item):
    if key in parameters.KEYS:
        return (parameters.validate(key, item), item["answer"])
    if key in structured.KEYS:
        family = structured.validate(key, item)
        return (family, item["answer"])
    math = re.findall(r"\$([^$]*)\$", item["problem"])
    source = expr(math[0].split("=")[0])
    answer = s.sympify(item["answer"], evaluate=False)
    if key == "quadratic-formula/kp3":
        roots = s.solve(source, X)
        equal(answer, sum(root.is_real is True for root in roots))
        return (s.srepr(s.Poly(source, X).monic().as_expr()), str(answer))
    return validate_factoring(key,item,source,answer)


def negative_controls(rows):
    checked = 0
    for row in rows:
        if row["kp_id"] not in REPAIRED:
            continue
        bad = copy.deepcopy(row["instances"][0])
        if row["kp_id"] in parameters.KEYS:
            bad = parameters.negative(bad)
        elif row["kp_id"] in structured.KEYS:
            bad = structured.negative(bad)
        elif row["kp_id"] == "quadratic-formula/kp3":
            bad["answer"] = "0"
        else:
            bad["answer"] = str(s.expand(s.sympify(bad["answer"])))
        try:
            validate(row["kp_id"], bad)
        except AssertionError:
            checked += 1
        else:
            raise AssertionError(("negative control accepted", row["kp_id"]))
    assert checked == len(REPAIRED)
    return checked


def main():
    rows = json.loads((ROOT / "docs/reports/unit07-template-evidence.json").read_text())
    seen = set()
    count = 0
    for row in rows:
        if row["kp_id"] not in REPAIRED:
            continue
        answers = set()
        for item in row["instances"]:
            family = validate(row["kp_id"], item)
            assert family not in seen, (row["kp_id"], family)
            seen.add(family)
            answers.add(family[1])
            count += 1
        assert len(row["instances"]) >= 12 and len(answers) > 1
    controls = negative_controls(rows)
    print(f"Residual semantic checks: {count} instances; {controls} negative controls rejected")


if __name__ == "__main__":
    main()
