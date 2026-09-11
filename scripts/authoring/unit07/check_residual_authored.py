"""Independent equations reconstructed from the retained authored questions."""
import json
import re
from pathlib import Path

import sympy as s

from check_math import equal, expr

ROOT = Path(__file__).resolve().parents[3]
X = s.Symbol("x")
SOURCES = {
    "factoring-gcf/kp2": ["-4*x**2-8*x", "14*x**4+21*x**3-7*x**2", "-6*x**4+9*x**3-3*x**2", "12*x**4-8*x**3+4*x**2"],
    "difference-of-squares/kp2": ["4*x**2-25", "9*x**4-16", "16*x**2-9", "25*x**4-4"],
    "difference-of-squares/kp3": ["2*x**2-18", "x**4-16", "3*x**4-48", "x**4-81"],
    "perfect-square-trinomials/kp3": ["4*x**2-12*x+9", "9*x**2+30*x+25", "16*x**2-24*x+9", "25*x**2+20*x+4"],
    "quadratics-in-form/kp2": ["x**4-5*x**2+4", "x**4-13*x**2+36", "x**4-17*x**2+16", "x**4-25*x**2+144"],
    "choosing-factoring-strategy/kp2": ["2*x**3-8*x", "3*x**2+12*x+12", "3*x**3-12*x", "2*x**2+20*x+50"],
    "choosing-factoring-strategy/kp3": ["2*x**4+8*x**3-18*x**2-72*x", "x**3+3*x**2-4*x-12", "3*x**3+6*x**2-12*x-24", "x**3+2*x**2-9*x-18"],
    "sum-difference-of-cubes/kp1": ["x**3-8", "(x+1)**3-27", "x**3-64", "(x+2)**3-125"],
    "sum-difference-of-cubes/kp2": ["x**3+1", "(x-1)**3+64", "x**3+8", "(x-2)**3+125"],
    "sum-difference-of-cubes/kp3": ["8*x**3-27", "27*x**3+1", "8*x**3-1", "64*x**3+27"],
    "quadratic-formula/kp3": ["2*x**2+6*x+3", "x**2+2*x+5", "2*x**2+4*x-1", "x**2+4*x+8"],
}


def root_answer(text):
    text = re.sub(r"√(\d+)", r"sqrt(\1)", text)
    return [expr(part.split("=")[1]) for part in text.split(" or ")]


def check(facts):
    checked = 0
    for kp in facts["kps"]:
        key = kp["kp_key"]
        if key not in SOURCES:
            continue
        assert len(kp["exemplars"]) == 4
        sources = list(map(s.sympify, SOURCES[key]))
        assert len(set(sources)) == 4
        for item, source in zip(kp["exemplars"], sources):
            assert item["authored_answer_decidable"] and item["solution_sketch"]
            if key == "quadratic-formula/kp3":
                roots = [r for r in s.solve(source, X) if r.is_real]
                if item["answer"] == "0":
                    assert not roots
                else:
                    assert set(root_answer(item["answer"])) == set(roots)
            else:
                equal(expr(item["answer"]), source)
            checked += 1
    assert checked == 4*len(SOURCES)
    print(f"Independent authored equations: {checked} correct answers")


if __name__ == "__main__":
    check(json.loads((ROOT / "target/unit07/facts.json").read_text()))
