"""Semantic isolation against the exhaustive production-enumerated corpus."""
import json
import re

import yaml
import sympy as s

from expressions import ROOT, build
from expressions_oracle import math, reconstruct
from expressions_test import signature


def authored_answers():
    answers = {}
    for path in (ROOT / "curriculum").rglob("*.yaml"):
        collect(yaml.safe_load(path.read_text()), answers)
    return answers


def collect(value, answers):
    if isinstance(value, list):
        for child in value:
            collect(child, answers)
    if isinstance(value, dict):
        if isinstance(value.get("problem"), str) and isinstance(value.get("answer"), str):
            answers[value["problem"]] = value["answer"]
        for child in value.values():
            collect(child, answers)


def family(key, problem):
    topic = key.split("/")[0]
    if topic in ("literal-equations", "rearranging-formulas"):
        return "isolate"
    if topic == "translating-phrases-to-expressions":
        return "translate"
    if topic in ("writing-expressions-from-patterns", "linear-word-problems"):
        return "model"
    if topic in ("substituting-values", "evaluating-expressions", "evaluating-polynomials"):
        return "evaluate"
    return key


def fingerprint(key, problem, answer):
    group = family(key, problem)
    surrogate = {
        "isolate": "literal-equations/kp1", "translate": "translating-phrases-to-expressions/kp1",
        "model": "writing-expressions-from-patterns/kp2", "evaluate": "substituting-values/kp1",
    }.get(group, key)
    return (group, *signature(surrogate, problem, answer)[1:])


def compare(corpus, rows, answers):
    candidates, owned = {}, {row["kp_id"] for row in rows}
    for row in rows:
        for sample in row["arguments"]["samples"]:
            problem = row["arguments"]["statement"].format(**sample["params"])
            key = fingerprint(row["kp_id"], problem, sample["expected"])
            assert key not in candidates, ("candidate collision", key)
            candidates[key] = problem
    checked = 0
    for item in corpus:
        key, problem = item["kp_id"], item["problem"]
        if key not in owned:
            if family(key, problem) == "evaluate":
                found = linear_evaluation(problem)
                if found is not None:
                    assert found not in candidates, ("linear evaluation collision", item)
                    checked += 1
                continue
            # Table and word-model KPs can ask for the identical affine rule.
            if key not in {"writing-expressions-from-patterns/kp1", "linear-word-problems/kp1"}:
                continue
        expected = answers.get(problem)
        if expected is None:
            expected = reconstruct_pending(key, problem)
        found = fingerprint(key, problem, expected)
        assert found not in candidates, ("semantic collision", item, candidates.get(found))
        checked += 1
    return checked


def linear_evaluation(problem):
    blocks = re.findall(r"\$([^$]+)\$", problem)
    assignments = [re.fullmatch(r"([a-z])\s*=\s*(\(?-?\d+\)?)", block) for block in blocks]
    assignments = [item for item in assignments if item]
    if len(assignments) != 1:
        return None
    variable, value = assignments[0].groups()
    for block in blocks:
        if re.fullmatch(r"[a-z]\s*=.*", block):
            continue
        text = re.sub(r"^[a-z]\([a-z]\)=", "", block).replace("{", "(").replace("}", ")")
        if not re.fullmatch(r"[a-zA-Z0-9+*/^(). -]+", text):
            continue
        expression = math(text)
        symbol = s.Symbol(variable)
        if expression.free_symbols == {symbol} and expression.is_polynomial(symbol):
            if s.degree(expression, symbol) == 1:
                return ("evaluate", str(expression.subs(symbol, s.Symbol("v"))), str(math(value)))
    return None


def reconstruct_pending(key, problem):
    if key == "writing-expressions-from-patterns/kp1":
        pair = re.search(r"\(n,v\)=\(1,(\d+)\)", problem)
        step = re.search(r"adds \$(\d+)\$", problem)
        if pair and step:
            first, delta = int(pair[1]), int(step[1])
            return f"{delta}*n+{first-delta}"
    # Owned unmatched statements are refused by the independent premise parser.
    return reconstruct(key, problem)


def main():
    corpus = json.loads((ROOT / "target/template36-corpus.json").read_text())
    rows = build()
    checked = compare(corpus, rows, authored_answers())
    # Cross-lane fee models are mathematically the same family across different KPs.
    other = json.loads((ROOT / "docs/content-foundations/template36/graphs.json").read_text())
    models = set()
    own = next(row for row in rows if row["kp_id"] == "writing-expressions-from-patterns/kp2")
    for sample in own["arguments"]["samples"]:
        models.add(fingerprint(own["kp_id"], "", sample["expected"]))
    for row in other:
        if row["kp_id"] != "linear-word-problems/kp1":
            continue
        for sample in row["arguments"]["samples"]:
            problem = row["arguments"]["statement"].format(**sample["params"])
            fee, rate = map(int, re.findall(r"EUR \$(\d+)\$", problem))
            assert fingerprint(row["kp_id"], problem, f"{rate}*h+{fee}") not in models
            checked += 1
    print(f"Scanned {len(corpus)} corpus items; semantically compared {checked} related premises.")


if __name__ == "__main__":
    main()
