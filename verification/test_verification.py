"""Boundary tests; mocked infrastructure never supplies formal proof."""
from fractions import Fraction
from pathlib import Path
import unittest
from contracts import Unsupported,evaluate,parse_expression,parse_factors,prepare_claim,prepare_problem
from lean_engine import Engine,checked_axioms,source_for
from verify_service import verify_request

class DecliningEngine:
    def metadata(self):
        return {"name":"test-double","available":False}
    def verify(self,claim,deadline):
        return False,"Proof not checked.",{"statement":claim.statement}

class Parsers(unittest.TestCase):
    def test_exact_arithmetic(self):
        self.assertEqual(evaluate(parse_expression("0.1+0.2"),{}),Fraction(3,10))
        self.assertEqual(evaluate(parse_expression("-2^2"),{}),-4)
        self.assertEqual(evaluate(parse_expression("(1/3+1/6)^2"),{}),Fraction(1,4))
    def test_unsafe_and_unsupported_input(self):
        for text in ("__import__('os')","by sorry","1; print(2)","x.y","f(x)","1/0","x/x","2^-1","2^100","2x"):
            with self.subTest(text=text),self.assertRaises(Unsupported):
                parse_expression(text,("x",))
    def test_factor_spellings(self):
        for text in ("1x12,2X6,3*4","1 times 12 and 2 times 6 and 3 times 4","1\u00d712,2\u00d76,3\u00d74",r"$1\times12,2\times6,3\times4$","(1,12),(2,6),(3,4)"):
            with self.subTest(text=text):
                result=parse_factors(text)
                self.assertEqual(set(result.members),{1,2,3,4,6,12})
                self.assertEqual(len(result.pairs),3)
        self.assertEqual(parse_factors("{1,2,3,6}").members,(1,2,3,6))
        self.assertEqual(parse_factors("1 2 3 6").members,(1,2,3,6))
    def test_pair_claims_are_preserved(self):
        claim=prepare_claim(prepare_problem({"kind":"factor_list","target":6}),"1x2,3x6")
        self.assertEqual(claim.status,"disproved")
        self.assertIn("1 * 2 = 6",claim.statement)
        self.assertTrue(claim.statement.startswith("Not "))
        self.assertEqual(claim.tactic,"decide +kernel")
    def test_bad_factor_syntax_is_not_a_disproof(self):
        for text in ("","1,,2","1x2x3","1; import Mathlib","(1,2","-1,1"):
            with self.subTest(text=text),self.assertRaises(Unsupported):
                parse_factors(text)
    def test_polynomial_negation_uses_a_checked_witness(self):
        p=prepare_problem({"kind":"polynomial_identity","expression":"(x+1)^2","variables":["x"]})
        claim=prepare_claim(p,"x^2+1")
        self.assertEqual(claim.status,"disproved")
        self.assertIn("Not (forall (x : Rat)",claim.statement)
        self.assertIn("counterexample",claim.interpretation)
        self.assertIn("norm_num at h_at",claim.tactic)
    def test_grid_miss_does_not_establish_truth(self):
        p=prepare_problem({"kind":"polynomial_identity","expression":"0","variables":["x"]})
        claim=prepare_claim(p,"(x+2)*(x+1)*x*(x-1)*(x-2)")
        self.assertIn("ring",claim.tactic)

class Boundaries(unittest.TestCase):
    def request(self):
        return {"source_hash":"example","problem":{"kind":"numeric_expression","expression":"1/2"},"learner_answer":"0.5","candidate_answer":"3"}
    def test_failure_and_missing_engine_fail_closed(self):
        for engine in (DecliningEngine(),Engine(Path("/missing/lean"),Path("/missing/path"))):
            result=verify_request(self.request(),engine)
            self.assertTrue(result["supported"])
            self.assertEqual(result["status"],"unresolved")
            self.assertEqual(result["learner"]["status"],"unresolved")
            self.assertEqual(result["candidate"]["status"],"unresolved")
    def test_unsupported_request(self):
        req=self.request()
        req["problem"]={"kind":"run_code","code":"anything"}
        self.assertFalse(verify_request(req,DecliningEngine())["supported"])
        req=self.request()
        req["command"]="anything"
        self.assertFalse(verify_request(req,DecliningEngine())["supported"])
    def test_axioms(self):
        self.assertEqual(checked_axioms("'CadusClaim' does not depend on any axioms","CadusClaim"),[])
        self.assertEqual(checked_axioms("'CadusClaim' depends on axioms: [propext, Quot.sound]","CadusClaim"),["Quot.sound","propext"])
        for name in ("sorryAx","Lean.trustCompiler","Lean.ofReduceBool","customAxiom"):
            with self.subTest(name=name),self.assertRaises(ValueError):
                checked_axioms("'CadusClaim' depends on axioms: ["+name+"]","CadusClaim")
        with self.assertRaises(ValueError):
            checked_axioms("success","CadusClaim")
    def test_fixed_template(self):
        c=prepare_claim(prepare_problem({"kind":"factor_list","target":6}),"1,2,3,6")
        source=source_for(c)
        self.assertIn("decide +kernel",source)
        self.assertNotIn("native_decide",source)
        self.assertNotIn("sorry",source)
        self.assertIn("#print axioms CadusClaim",source)

if __name__=="__main__":
    unittest.main()
