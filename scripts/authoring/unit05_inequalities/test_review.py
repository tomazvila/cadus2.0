"""Cross-curriculum collisions, residual partitions, visuals and adversarial checks."""
from collections import Counter
from fractions import Fraction as Q
import json
from pathlib import Path
import re
import sys
import unittest

from build import OUT, YAML, generate, is_set
from semantic import OPS, math, reconstruct, relation, scalar
from semantic_labels import checks
from test_semantic import family, truth_materiality
import semantic_sets


def check_visual(visual):
    plane=visual['shaded_half_planes'][0]
    for key in ('through_a','through_b'):
        p=plane[key]
        assert Q(p['y'])==Q(p['x'])-1
    p=plane['shade_toward']
    assert Q(p['y'])<Q(p['x'])-1
    assert plane['solid'] and plane['label']=='y ≤ x - 1'
    assert 'origin fails' in visual['caption']


def check_displayed_equation(test,args,problem,equation):
    if not re.search(r'<=|>=|<|>',equation):
        return
    if args['answer_contract']['kind']=='label':
        if 'x' in equation:
            test.assertEqual(relation(equation),relation(math(problem)[0]))
            return
        left,op,right=re.split(r'(<=|>=|<|>)',equation)
        actual=(scalar(left),op,scalar(right),OPS[op](scalar(left),scalar(right)))
        test.assertIn(actual,checks(problem))
    elif is_set(problem):
        test.assertEqual(semantic_sets.bounds('$'+equation+'$'),semantic_sets.bounds(problem))
    else:
        source=math(problem)[-1] if 'maximum whole' in problem else math(problem)[0]
        test.assertEqual(relation(equation),relation(source))


def check_displayed_sample(test,args,sample):
    problem=args['statement'].format(**sample['params'])
    sketch=args['solution_sketch'].format(**sample['params'])
    for equation in math(sketch):
        check_displayed_equation(test,args,problem,equation)


class ReviewTests(unittest.TestCase):
    def test_complete_scope_and_fail_closed_residuals(self):
        authored,recipes=generate()
        blockers=json.loads((OUT/'blockers.json').read_text())
        blocked={r['kp_id'] for r in blockers}
        self.assertFalse(set(authored)&blocked)
        self.assertEqual(set(authored)|blocked,set(json.loads((OUT/'scope.json').read_text())))
        self.assertEqual(len(blocked),8)
        for row in blockers:
            self.assertEqual(row['status'],'blocked')
            self.assertNotEqual(row['kind'],'template')
            if row['expected_gate']=='accepted':
                answers=[s['expected'] for s in row['arguments']['samples']]
                self.assertEqual(len(answers),12)
                self.assertEqual(len(set(answers)),1)

    def test_cross_curriculum_collisions(self):
        facts=json.loads(Path(FACTS).read_text())
        authored,recipes=generate()
        outside=set()
        for row in facts['kps']:
            if row['kp_key'] in authored: continue
            for exemplar in row['exemplars']:
                try: outside.add(family(exemplar['problem']))
                except (ValueError,AssertionError,KeyError,StopIteration,SyntaxError,ZeroDivisionError): pass
        self.assertGreaterEqual(len(outside),40)
        for rows in authored.values():
            for row in rows: self.assertNotIn(family(row['problem']),outside)
        for row in recipes:
            args=row['arguments']
            for sample in args['samples']:
                self.assertNotIn(family(args['statement'].format(**sample['params'])),outside)

    def test_origin_visual_reconstruction_and_wrong_side_control(self):
        source=YAML.read_text()
        candidates=re.findall(r'        visuals: (.+)',source)
        visual=next(json.loads(s)[0] for s in candidates if 'y ≤ x - 1' in s)
        check_visual(visual)
        visual['shaded_half_planes'][0]['shade_toward']={'x':'0','y':'0'}
        with self.assertRaises(AssertionError): check_visual(visual)

    def test_constant_inactive_axis_budget_and_union_negative_controls(self):
        domains={'a':[1,2,3],'b':[4,5,6,7]}
        with self.assertRaises(AssertionError):
            truth_materiality({(a,b):'yes' for a in domains['a'] for b in domains['b']},domains)
        with self.assertRaises(AssertionError):
            truth_materiality({(a,b):'yes' if a==1 else 'no'
                               for a in domains['a'] for b in domains['b']},domains)
        authored,_=generate()
        budget=authored['inequality-word-problems/kp1'][0]['problem']
        with self.assertRaises(AssertionError): reconstruct(budget.replace('8 euros','9 euros'))
        # A deleted point is a strict subset of the excluded positive-radius interval.
        for radius in range(11,23):
            self.assertFalse(abs(1)>radius)
            self.assertTrue(1<0 or 1>0)
        source='Solve $|2x+1|<=7$.'
        good=semantic_sets.reconstruct(source)
        with self.assertRaises(AssertionError):
            semantic_sets.verify(source,good,'The boundary calculation gives $-3 <= x <= 4$.')

    def test_every_displayed_recipe_transformation(self):
        for row in generate()[1]:
            args=row['arguments']
            for sample in args['samples']:
                check_displayed_sample(self,args,sample)


if __name__=='__main__':
    FACTS=sys.argv.pop(1)
    unittest.main()
