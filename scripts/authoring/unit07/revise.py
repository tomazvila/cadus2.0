"""Apply reviewed unit07 scalar edits without rewriting surrounding YAML."""
from pathlib import Path

import yaml
import sympy as s
from sympy.parsing.sympy_parser import parse_expr, standard_transformations, implicit_multiplication_application

ROOT = Path(__file__).resolve().parents[3]
UNIT = ROOT / "curriculum/foundations/07-polynomials-quadratics.yaml"
EDITS = {}


def put(key, index, problem=None, answer=None, sketch=None):
    row = EDITS.setdefault((key, index), {})
    for field, value in (("problem", problem), ("answer", answer), ("solution_sketch", sketch)):
        if value is not None:
            row[field] = value


def expression(text):
    return parse_expr(text.replace("^", "**"), transformations=standard_transformations + (implicit_multiplication_application,))


def factor_answers(unit):
    prefixes = ("factoring-", "difference-of-squares", "perfect-square-trinomials", "sum-difference-of-cubes", "quadratics-in-form", "choosing-factoring-strategy")
    for topic in unit["topics"]:
        if not topic["id"].startswith(prefixes):
            continue
        for kp in topic["knowledge_points"]:
            for index, item in enumerate(kp["exemplars"]):
                if item["answer"] in ("no", "yes", "GCF") or "(" in item["answer"]:
                    continue
                old = expression(item["answer"])
                result = s.factor(old)
                if topic["id"] == "factoring-gcf" and kp["id"] == "kp1":
                    result = s.factor_terms(s.expand(old))
                assert s.expand(result - old) == 0
                put(topic["id"] + "/" + kp["id"], index, answer=str(result).replace("**", "^"))


def mapping(node):
    return {key.value: value for key, value in node.value}


def apply():
    import arithmetic, factoring, solving, graphing, sketches
    text = UNIT.read_text()
    unit = yaml.safe_load(text)
    factor_answers(unit)
    for module in (arithmetic, factoring, solving, graphing, sketches):
        module.revise(put)
    tree = mapping(yaml.compose(text))
    replacements = []
    found = set()
    for topic in tree["topics"].value:
        t = mapping(topic)
        for kp in t["knowledge_points"].value:
            k = mapping(kp)
            key = t["id"].value + "/" + k["id"].value
            for index, node in enumerate(k["exemplars"].value):
                fields = mapping(node)
                for field, value in EDITS.get((key, index), {}).items():
                    target = fields[field]
                    replacements.append((target.start_mark.index, target.end_mark.index, "'" + value.replace("'", "''") + "'"))
                    found.add((key, index, field))
    assert len(found) == sum(len(row) for row in EDITS.values())
    for start, end, value in sorted(replacements, reverse=True):
        text = text[:start] + value + text[end:]
    UNIT.write_text(text)
    print(f"Updated {len(EDITS)} exemplar rows / {len(found)} scalar fields")


if __name__ == "__main__":
    apply()
