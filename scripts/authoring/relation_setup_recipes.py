"""Generate pending setup-preserving translation recipes for two exact KPs."""
from itertools import product
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CONTRACT = {"kind": "relation_setup"}


def recipe(key, statement, expression, sketch, domains, solve):
    samples = []
    for values in product(*domains.values()):
        params = dict(zip(domains, values, strict=True))
        samples.append({"params": params, "expected": solve(**params)})
    assert len(samples) >= 12
    assert len({row["expected"] for row in samples}) == len(samples)
    return {
        "kp_id": key,
        "kind": "template",
        "status": "pending",
        "arguments": {
            "statement": statement,
            "answer_expr": expression,
            "answer_contract": CONTRACT,
            "params": {name: {"kind": "choice", "values": values}
                       for name, values in domains.items()},
            "constraints": [],
            "solution_sketch": sketch,
            "hints": ["Preserve the stated operations and grouping before choosing the relation symbol."],
            "samples": samples,
            "distractors": [],
        },
    }


def equation_recipes():
    key = "translating-sentences-to-equations/kp2"
    return [
        recipe(
            key,
            "Write an equation: ${b}$ times a number $x$ plus ${a}$ is ${c}$. Preserve the setup; do not solve.",
            "relationform((b*x+a,c),0)",
            "Translate '${b}$ times $x$' as ${b}*x$, then add ${a}$. The word 'is' supplies equality with ${c}$. Keep that unsolved operation structure.",
            {"a": [4, 7], "b": [2, 3, 5], "c": [19, 31]},
            lambda a, b, c: f"{b}*x + {a} = {c}",
        ),
        recipe(
            key,
            "Write an equation: ${a}$ less than the quotient of $x$ and ${b}$ is ${c}$. Preserve the setup; do not solve.",
            "relationform((x/b-a,c),0)",
            "First form the quotient $x/{b}$. Subtract ${a}$ from that quotient, then use 'is' to set the unchanged expression equal to ${c}$.",
            {"a": [3, 6], "b": [2, 4, 5], "c": [7, 13]},
            lambda a, b, c: f"x/{b} - {a} = {c}",
        ),
        recipe(
            key,
            "Write an equation: ${a}$ times the sum of $x$ and ${b}$ is ${c}$. Preserve the setup; do not solve.",
            "relationform((a*(x+b),c),0)",
            "The named sum is $x+{b}$, so keep it grouped. Multiply that whole sum by ${a}$ and set the resulting setup equal to ${c}$.",
            {"a": [2, 4, 6], "b": [3, 8], "c": [24, 42]},
            lambda a, b, c: f"{a}*(x + {b}) = {c}",
        ),
    ]


def inequality_recipes():
    key = "writing-inequalities-from-statements/kp2"
    return [
        recipe(
            key,
            "Write an inequality: ${a}$ more than ${b}$ times $x$ is at most ${c}$. Preserve the setup; do not solve.",
            "relationform((b*x+a,c),-1)",
            "Multiply $x$ by ${b}$ and add ${a}$. 'At most' means less than or equal to, so retain the setup as ${b}*x+{a}\\le {c}$.",
            {"a": [4, 7], "b": [2, 3, 5], "c": [18, 29]},
            lambda a, b, c: f"{b}*x + {a} <= {c}",
        ),
        recipe(
            key,
            "Write an inequality: the quotient of $x$ and ${a}$ exceeds ${c}$. Preserve the setup; do not solve.",
            "relationform((x/a,c),2)",
            "The quotient in the stated order is $x/{a}$. 'Exceeds' is a strict greater-than comparison with ${c}$; leave the quotient unsolved.",
            {"a": [2, 3, 4], "c": [5, 7, 9, 11]},
            lambda a, c: f"x/{a} > {c}",
        ),
    ]


def main():
    groups = [
        ("unit03-translation", equation_recipes()),
        ("unit05-relation-translation", inequality_recipes()),
    ]
    for directory, rows in groups:
        output = ROOT / "docs/content-foundations" / directory / "templates.json"
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(rows, indent=2) + "\n")
    print("5 pending recipes; 60 exhaustive samples")


if __name__ == "__main__":
    main()
