"""Build finite, exact worker-template candidates; no imports or approvals."""
import itertools
import json
from pathlib import Path

import sympy as s

ROOT = Path(__file__).resolve().parents[3]
ROWS = []


def term(expr):
    if expr.is_Rational:
        return {"lit": int(expr) if expr.is_Integer else str(expr)}
    if expr.is_Symbol:
        return str(expr)
    if expr.is_Add or expr.is_Mul:
        return {"add" if expr.is_Add else "mul": [term(arg) for arg in expr.args]}
    if expr.is_Pow and expr.exp.is_Integer and expr.exp > 0:
        return {"mul": [term(expr.base)] * int(expr.exp)}
    raise ValueError(f"Unsupported constraint term: {expr}")


def exact_text(value):
    if isinstance(value, (tuple, s.Tuple)):
        return "(" + ", ".join(exact_text(item) for item in value) + ")"
    if isinstance(value, set):
        return "{" + ", ".join(exact_text(item) for item in sorted(value, key=str)) + "}"
    return str(s.simplify(value))


def evaluate_answer(answer, bound):
    deferred_gcd = s.Function("gcd")
    parsed = s.sympify(answer, locals={"gcd": deferred_gcd, "max": s.Max, "abs": s.Abs})
    if isinstance(parsed, tuple):
        return tuple(evaluate_answer(str(item), bound) for item in parsed)
    result = parsed.subs(bound)
    return result.replace(lambda node: node.func == deferred_gcd, lambda node: s.gcd(*node.args))


def add(key, statement, answer, sketch, hint, axes=None, derived=None, contract=None, unit=None):
    axes = axes or {"a": range(2, 14)}
    derived = derived or {}
    params = {name: {"kind": "choice", "values": list(values)} for name, values in axes.items()}
    formulas = {name: s.sympify(expr) for name, expr in derived.items()}
    samples = []
    for values in itertools.product(*axes.values()):
        bound = dict(zip(axes, values))
        for name, formula in formulas.items():
            bound[name] = int(formula.subs(bound))
        if key == "applying-the-quadratic-formula/kp1":
            expected = f"a = 2; b = -3; c = {bound['a']}"
        else:
            evaluated = evaluate_answer(answer, bound)
            expected = exact_text(evaluated) + (" " + unit if unit else "")
        samples.append({"params": bound, "expected": expected})
    for name in formulas:
        params[name] = {"kind": "choice", "values": sorted({row["params"][name] for row in samples})}
    constraints = [{"op": "eq", "left": name, "right": term(formula)} for name, formula in formulas.items()]
    answer_expr = f"({answer}) {unit}" if unit else answer
    arguments = dict(statement=statement, answer_expr=answer_expr, solution_sketch=sketch,
                     hints=[hint], params=params, constraints=constraints, samples=samples, distractors=[])
    row = {"kp_id": key, "kind": "template", "status": "pending", "arguments": arguments}
    if contract is not None:
        row["requested_contract"] = contract
    ROWS.append(row)


def main():
    import template_arithmetic, template_factoring, template_solving, template_graphing
    for module in (template_arithmetic, template_factoring, template_solving, template_graphing):
        module.build(add)
    keys = [row["kp_id"] for row in ROWS]
    assert len(set(keys)) == len(keys)
    target = ROOT / "target/unit07/candidates.json"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(ROWS, indent=2) + "\n")
    print(f"Wrote {len(ROWS)} candidates for the production worker gate")


if __name__ == "__main__":
    main()
