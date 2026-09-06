"""Independent exact arithmetic/geometry review of authored and served Unit04 content.

Run after the Rust production verifier exports target/unit04-evidence/instances.json.
The oracle checks defining invariants, never evaluates a recipe's answer_expr.
"""
import ast
import itertools
import json
import re
import sys
import unittest
from fractions import Fraction as Q
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/review"))
from foundations_content_audit import family, pending_templates, _markers

DATA = ROOT / "docs/content-foundations/unit04-residual"


def read(path):
    return json.loads(path.read_text())


def arithmetic(text, x=Q(0)):
    """Tiny independent rational evaluator for explicit linear answers only."""
    def walk(node):
        if isinstance(node, ast.Constant) and type(node.value) is int:
            return Q(node.value)
        if isinstance(node, ast.Name) and node.id == "x":
            return x
        if isinstance(node, ast.Tuple):
            return tuple(walk(item) for item in node.elts)
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, (ast.USub, ast.UAdd)):
            return -walk(node.operand) if isinstance(node.op, ast.USub) else walk(node.operand)
        if isinstance(node, ast.BinOp):
            a, b = walk(node.left), walk(node.right)
            if isinstance(node.op, ast.Add):
                return a+b
            if isinstance(node.op, ast.Sub):
                return a-b
            if isinstance(node.op, ast.Mult):
                return a*b
            if isinstance(node.op, ast.Div):
                return a/b
        raise ValueError(f"unsupported arithmetic: {text}")
    return walk(ast.parse(text.strip(), mode="eval").body)


def invariant(answer, premise):
    """Check an answer against displacements, pairs, or line incidence/directions."""
    kind = premise["type"]
    if kind in {"line", "perpendicular"}:
        value = arithmetic(answer)
        if not isinstance(value, tuple) or len(value) != 2:
            return False
        m, c = value
        if any(m*Q(x)+c != Q(y) for x,y in premise["points"]):
            return False
        if kind == "perpendicular":
            dx, dy = premise["reference"]
            return dx+m*dy == 0  # Dot product of direction vectors (1,m) and (dx,dy).
        if "direction" in premise:
            dx, dy = premise["direction"]
            return m*dx == dy
        return True
    value = arithmetic(answer)
    if kind == "position":
        return value == tuple(sum(Q(move[i]) for move in premise["moves"]) for i in [0,1])
    if kind == "ratio":
        return value * Q(premise["input"]) == Q(premise["output"])
    if kind == "slope":
        (x1,y1),(x2,y2) = premise["points"]
        return value * (x2-x1) == y2-y1
    if kind == "coefficients":
        return value == (Q(premise["m"]), Q(premise["b"]))
    raise ValueError(kind)


def signature(answer):
    if answer.startswith("y="):
        c = arithmetic(answer[2:])
        return (arithmetic(answer[2:], Q(1))-c, c)
    return arithmetic(answer)


def corrupt(answer):
    if answer.startswith("y="):
        return answer+"+1"
    value = arithmetic(answer)
    if isinstance(value, tuple):
        return "("+",".join(str(v+1) for v in value)+")"
    return str(value+1)


def distinct(instances):
    assert len(instances) >= 12
    assert len({i["problem"] for i in instances}) == len(instances)
    assert len({signature(i["answer"]) for i in instances}) == len(instances)
    axes = list(instances[0]["params"])
    for axis in axes:
        fibers = {}
        for item in instances:
            fixed = tuple((k,v) for k,v in sorted(item["params"].items()) if k != axis)
            fibers.setdefault(fixed, []).append(signature(item["answer"]))
        assert all(len(v) > 1 and len(set(v)) == len(v) for v in fibers.values()), axis


class Unit04ResidualReview(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.premises = {r["kp_id"]: r for r in read(DATA/"semantic-premises.json")}
        cls.drafts = {r["kp_id"]: r for r in read(DATA/"drafts.json")}
        cls.instances = {r["kp_id"]: r["instances"] for r in
                         read(ROOT/"target/unit04-evidence/instances.json")}
        facts = read(ROOT/"target/unit04-evidence/after-facts.json")
        cls.facts = {r["kp_key"]: r for r in facts["kps"]}

    def test_exact_scope_pending_status_and_complete_domains(self):
        self.assertEqual(len(self.drafts), 10)
        self.assertEqual(self.drafts.keys(), self.premises.keys())
        self.assertEqual(self.drafts.keys(), self.instances.keys())
        pending = pending_templates(ROOT/"docs/content-foundations")
        for key, draft in self.drafts.items():
            self.assertEqual(draft["status"], "pending")
            self.assertEqual(len(pending[key]), 1, key)
            args = draft["arguments"]
            self.assertEqual(args["constraints"], [])
            self.assertNotIn("space_size", args)
            domains = [v["values"] for v in args["params"].values()]
            self.assertEqual(len(list(itertools.product(*domains))), 12)
            self.assertEqual(len(args["samples"]), 12)
            self.assertGreater(len(args["solution_sketch"]), 70)
            self.assertFalse(_markers(args["solution_sketch"], args["answer_expr"]))

    def test_authored_examples_against_geometric_and_arithmetic_invariants(self):
        for key, evidence in self.premises.items():
            live = self.facts[key]["exemplars"]
            self.assertEqual(len(live), 4)
            families = set()
            for actual, source in zip(live, evidence["authored"]):
                for field in ["problem", "answer", "solution_sketch"]:
                    self.assertEqual(actual[field], source[field], (key,field))
                self.assertTrue(invariant(actual["answer"], source["premise"]), key)
                self.assertFalse(invariant(corrupt(actual["answer"]), source["premise"]), key)
                self.assertTrue(actual["authored_answer_decidable"], key)
                self.assertFalse(_markers(actual["solution_sketch"], actual["answer"]))
                families.add((family(actual["problem"]), family(actual["answer"])))
            self.assertEqual(len(families), 4, key)

    def test_served_answers_and_parameters_exhaustively(self):
        for key, instances in self.instances.items():
            distinct(instances)
            cases = {tuple(sorted((k,str(v)) for k,v in c["params"].items())): c["premise"]
                     for c in self.premises[key]["cases"]}
            seen = set()
            for item in instances:
                params = tuple(sorted(item["params"].items()))
                seen.add(params)
                self.assertTrue(invariant(item["answer"], cases[params]), (key,item))
                self.assertFalse(invariant(corrupt(item["answer"]), cases[params]), key)
                self.assertNotIn(item["answer"], item["problem"], (key,"answer exposed"))
            self.assertEqual(seen, cases.keys())

    def test_no_cross_kp_problem_answer_collisions(self):
        # Check authored material across all Foundations, plus new rendered tasks.
        rows = [(key,e["problem"],e["answer"]) for key,k in self.facts.items()
                for e in k["exemplars"]]
        rows += [(key,e["problem"],e["answer"]) for key,items in self.instances.items() for e in items]
        seen = {}
        for key, problem, answer in rows:
            normalized = re.sub(r"\s+", " ", problem.casefold()).strip(" .")
            previous = seen.setdefault(normalized, key)
            if previous != key and (key in self.drafts or previous in self.drafts):
                self.fail(f"cross-KP collision: {previous} / {key}: {problem}")
        recipe_families = {}
        for key,draft in self.drafts.items():
            template = draft["arguments"]["statement"]
            fingerprint = family(re.sub(r"\{[a-z]+\}", "1", template))
            self.assertNotIn(fingerprint, recipe_families, key)
            recipe_families[fingerprint] = key

    def test_semantic_negative_controls_reject_collapsed_or_inert_families(self):
        instances = self.instances["constant-of-proportionality/kp1"]
        for wrong in ["0", "1", "2-2", "7/7"]:
            mutant = [dict(i, answer=wrong) for i in instances]
            with self.assertRaises(AssertionError):
                distinct(mutant)
        mutant = [dict(i, answer=str(i["params"]["a"])) for i in instances]
        with self.assertRaises(AssertionError):
            distinct(mutant)
        mutant = [dict(i, problem="Same task") for i in instances]
        with self.assertRaises(AssertionError):
            distinct(mutant)

    def test_semantic_negative_controls_detect_sign_and_reciprocal_errors(self):
        premise = {"type": "perpendicular", "points": [[2,3]], "reference": [1,4]}
        self.assertTrue(invariant("(-1/4,7/2)", premise))
        self.assertFalse(invariant("(1/4,5/2)", premise))
        self.assertFalse(invariant("(-4,11)", premise))
        self.assertFalse(invariant("(-1/4,3)", premise))
        self.assertFalse(invariant("(5,3)", {"type": "position", "moves": [[3,5]]}))


if __name__ == "__main__":
    unittest.main()
