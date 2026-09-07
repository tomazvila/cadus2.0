"""Exhaustive expression recipe checks and adversarial reconstruction tests."""
import itertools
import json
import re
import unittest

import sympy as s
import yaml

from expressions import ROOT, build
from expressions_oracle import math, reconstruct


def alpha_signature(expression):
    """Ignore variable renaming while preserving every numerical coefficient."""
    symbols = sorted(expression.free_symbols, key=str)
    names = s.symbols(f"v0:{len(symbols)}")
    return min(str(s.cancel(expression.xreplace(dict(zip(symbols, perm)))))
               for perm in itertools.permutations(names))


def signature(key, problem, answer):
    topic = key.split("/")[0]
    if topic in ("literal-equations", "rearranging-formulas",
                 "translating-phrases-to-expressions", "writing-expressions-from-patterns"):
        # These objectives ask for the complete symbolic rule; equivalent rules collide.
        rhs = answer.split("=", 1)[-1].strip()
        return (key, alpha_signature(math(rhs)))
    if topic == "parts-of-an-expression":
        return (key, answer)
    blocks = re.findall(r"\$([^$]+)\$", problem)
    if topic == "substituting-values":
        expression = math(blocks[0])
        variable, value = blocks[1].split("=")
        return (key, str(expression.subs(s.Symbol(variable.strip()), s.Symbol("v"))),
                str(math(value)))
    return (key, tuple(re.findall(r"\d+", problem)), answer)


def evaluated(row, sample):
    arguments = row["arguments"]
    expression = arguments["answer_expr"]
    bindings = sample["params"]
    expression = re.sub(r"symbol\(([a-z])\)", lambda match: bindings[match[1]], expression)
    if expression.startswith("multipart("):
        names = [part["name"] for part in arguments["answer_contract"]["parts"]]
        values = [math(piece).subs(bindings) for piece in expression[10:-1].split(",")]
        return "; ".join(f"{name} = {value}" for name, value in zip(names, values))
    return str(math(expression).subs(bindings))


def same_answer(left, right):
    if ";" in left or ";" in right:
        return left == right
    return s.simplify(math(left)-math(right)) == 0


class ExpressionsTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.rows = build()

    def test_checked_in_artifact_matches_generator(self):
        path = ROOT / "docs/content-foundations/template36/expressions.json"
        self.assertEqual(json.loads(path.read_text()), self.rows)

    def test_exhaustive_domains_independent_answers_and_material_variation(self):
        all_signatures = set()
        self.assertEqual(len(self.rows), 12)
        for row in self.rows:
            key, args = row["kp_id"], row["arguments"]
            self.assertEqual(set(row), {"kp_id", "kind", "arguments"})
            product = set(itertools.product(*(axis["values"] for axis in args["params"].values())))
            actual = {tuple(sample["params"].values()) for sample in args["samples"]}
            self.assertEqual(product, actual, key)
            self.assertEqual(len(actual), 12, key)
            answers = set()
            for sample in args["samples"]:
                problem = args["statement"].format(**sample["params"])
                for field in [args["statement"], args["solution_sketch"], *args["hints"]]:
                    self.assertEqual(field.format(**sample["params"]).count("$") % 2, 0, key)
                expected = reconstruct(key, problem)
                self.assertTrue(same_answer(expected, evaluated(row, sample)), (key, sample))
                self.assertTrue(same_answer(expected, sample["expected"]), (key, sample))
                fingerprint = signature(key, problem, expected)
                self.assertNotIn(fingerprint, all_signatures, (key, sample))
                all_signatures.add(fingerprint)
                answers.add(expected)
            self.assertGreater(len(answers), 1, key)
        self.assertEqual(len(all_signatures), 144)

    def test_no_authored_semantic_collisions(self):
        tree = yaml.safe_load((ROOT / "curriculum/foundations/03-expressions-equations.yaml").read_text())
        authored = {}
        for topic in tree["topics"]:
            for kp in topic["knowledge_points"]:
                key = topic["id"] + "/" + kp["id"]
                authored[key] = kp["exemplars"]
        for row in self.rows:
            key, args = row["kp_id"], row["arguments"]
            known = {signature(key, e["problem"], e["answer"]) for e in authored[key]}
            for sample in args["samples"]:
                problem = args["statement"].format(**sample["params"])
                self.assertNotIn(signature(key, problem, sample["expected"]), known, (key, sample))

    def test_constraints_from_curriculum(self):
        for row in self.rows:
            for sample in row["arguments"]["samples"]:
                p = sample["params"]
                if row["kp_id"] == "parts-of-an-expression/kp1":
                    self.assertTrue(all(-9 <= value <= 9 for value in p.values()))
                if row["kp_id"] == "substituting-values/kp1":
                    self.assertGreater(p["v"], 0)
                    self.assertLessEqual(p["a"], 9)
                if row["kp_id"] == "translating-sentences-to-equations/kp1":
                    self.assertEqual(math(sample["expected"]).q, 1)

    def test_oracle_detects_wrong_inverse_and_order(self):
        cases = [
            ("literal-equations/kp1", "Solve $y=13b*x+2d-e$ for $x$.", "(y+2*d-e)/(13*b)"),
            ("literal-equations/kp2", "Solve $17b*x+3d*x=y$ for $x$.", "y/(17*b-3*d)"),
            ("literal-equations/kp3", "Solve $y=b*(x+11d)/5$ for $x$.", "y/(5*b)-11*d"),
            ("rearranging-formulas/kp1", "Solve $y=x+21b$ for $x$.", "y+21*b"),
            ("rearranging-formulas/kp2", "Solve $y=17x+11b$ for $x$.", "y/17-11*b"),
            ("translating-phrases-to-expressions/kp1", "The difference of $x$ and $21$.", "21-x"),
            ("translating-phrases-to-expressions/kp2", "The quotient of $x$ and $21$.", "21/x"),
            ("translating-phrases-to-expressions/kp3", "$21$ less than $6$ times $x$.", "21-6*x"),
            ("substituting-values/kp1", "Evaluate $6x+(-8)$ at $x=11$.", "74"),
        ]
        for key, problem, wrong in cases:
            self.assertFalse(same_answer(reconstruct(key, problem), wrong), key)

    def test_changed_premises_change_reconstruction(self):
        for row in self.rows:
            args = row["arguments"]
            first, last = args["samples"][0], args["samples"][-1]
            left = reconstruct(row["kp_id"], args["statement"].format(**first["params"]))
            right = reconstruct(row["kp_id"], args["statement"].format(**last["params"]))
            self.assertFalse(same_answer(left, right), row["kp_id"])

    def test_semantic_signature_catches_paraphrase_and_variable_rename(self):
        key = "translating-phrases-to-expressions/kp3"
        self.assertEqual(signature(key, "", "6*x-21"), signature(key, "", "-21+6*z"))
        self.assertNotEqual(signature(key, "", "6*x-21"), signature(key, "", "6*z-23"))

    def test_cross_key_models_and_evaluation_collisions_are_detected(self):
        from expressions_collisions import compare, linear_evaluation
        own = next(row for row in self.rows if row["kp_id"] == "writing-expressions-from-patterns/kp2")
        example = own["arguments"]["samples"][0]
        item = {"kp_id": "writing-expressions-from-patterns/kp1", "problem": "A renamed affine table."}
        with self.assertRaisesRegex(AssertionError, "semantic collision"):
            compare([item], [own], {item["problem"]: example["expected"].replace("n", "h")})
        self.assertEqual(linear_evaluation("Evaluate $p(x)=6x-8$ at $x=11$."),
                         ("evaluate", "6*v - 8", "11"))
        self.assertIsNone(linear_evaluation("Evaluate $x^2-8$ at $x=11$."))


if __name__ == "__main__":
    unittest.main()
