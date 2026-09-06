"""Check repaired algebra against authored and sibling semantic families."""
import json
from pathlib import Path

import sympy as s

from check_math import expr
from check_residual_math import REPAIRED, validate

ROOT = Path(__file__).resolve().parents[3]
FACTORING = ("factoring-", "difference-of-squares/", "perfect-square-trinomials/",
             "sum-difference-of-cubes/", "quadratics-in-form/", "choosing-factoring-strategy/")


def signature(key, item):
    if key.startswith(FACTORING):
        try:
            value = expr(item["answer"])
            if not value.has(s.Symbol("x")):
                return None
            return ("factor", s.srepr(s.expand(value)))
        except (TypeError, SyntaxError, AttributeError):
            return None
    if key in {"quadratic-formula/kp3", "discriminant/kp2"}:
        if not item["answer"].isdigit():
            return None
        import re
        math = re.findall(r"\$([^$]*)\$", item["problem"])
        try:
            expression = math[0].split("=", 1)
            raw = expression[-1] if expression[0] == "y" else expression[0]
            poly = expr(raw)
            if isinstance(poly, (tuple, s.Tuple)) and len(poly) == 3:
                a, b, c = poly
                poly = a*s.Symbol("x")**2+b*s.Symbol("x")+c
            if not poly.has(s.Symbol("x")) or not item["answer"].isdigit():
                return None
            return ("count", s.srepr(s.Poly(poly, s.Symbol("x")).monic().as_expr()))
        except (TypeError, SyntaxError, IndexError):
            return None
    return None


def main():
    facts = json.loads((ROOT / "target/unit07/facts.json").read_text())
    rows = json.loads((ROOT / "docs/reports/unit07-template-evidence.json").read_text())
    existing = {}
    for kp in facts["kps"]:
        for item in kp["exemplars"]:
            sig = signature(kp["kp_key"], item)
            if sig:
                existing.setdefault(sig, []).append(kp["kp_key"])
    for row in rows:
        if row["kp_id"] in REPAIRED:
            continue
        for item in row["instances"]:
            sig = signature(row["kp_id"], item)
            if sig:
                existing.setdefault(sig, []).append(row["kp_id"])
    checked = 0
    for row in rows:
        if row["kp_id"] not in REPAIRED:
            continue
        for item in row["instances"]:
            sig = signature(row["kp_id"], item)
            if sig:
                assert sig not in existing, (row["kp_id"], item["problem"], existing.get(sig))
                existing[sig] = [row["kp_id"]]
                checked += 1
    print(f"Semantic algebra collision checks: {checked} repaired instances, no collisions")


if __name__ == "__main__":
    main()
