"""Exhaustive independent semantics, entropy, materiality, and negative controls."""
from collections import Counter
from copy import deepcopy
from itertools import product
import json
from math import log2
from pathlib import Path
import re
import sys
import unittest

from unit04_complement_common import ROOT, OUT, fields
from unit04_complement_semantics import parsed_answer, reconstruct, signature, verify

HISTORICAL_KEYS={
    'graphing-linear-equations/kp3',
    'graphing-proportional-relationships/kp3',
    'horizontal-vertical-slopes/kp3',
    'proportional-relationships/kp2',
    'slope-as-rate-of-change/kp2',
}
CURRENT_ADDITIONS={
    'coordinate-plane/kp1',
    'horizontal-vertical-slopes/kp1',
    'horizontal-vertical-slopes/kp2',
    'solutions-of-two-variable-equations/kp1',
    'graphing-from-a-table/kp2',
    'slopes-of-parallel-perpendicular-lines/kp3',
}


class ComplementTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.rows=json.loads((OUT/'templates.json').read_text())
        cls.facts=json.loads(Path(FACTS).read_text())
        cls.gate=json.loads(Path(GATE).read_text())
        cls.by_key={r['kp_key']:r for r in cls.facts['kps']}
        cls.keys={r['kp_id'] for r in cls.rows}

    def test_scope_four_material_exemplars_and_production_decidability(self):
        scope=json.loads((OUT/'scope.json').read_text())
        # scope.json is the immutable evidence for the first five-family repair.
        # Six families were completed later; pin that current ownership here
        # without rewriting the historical baseline or its initial statuses.
        self.assertEqual(set(scope['keys']),HISTORICAL_KEYS)
        self.assertEqual(set(scope['initial_status']),HISTORICAL_KEYS)
        self.assertEqual(set(scope['initial_status'].values()),{'absent_pending'})
        self.assertEqual(self.keys,HISTORICAL_KEYS|CURRENT_ADDITIONS)
        for key in self.keys:
            rows=self.by_key[key]['exemplars']
            self.assertEqual(len(rows),4,key)
            structures=set()
            signatures=set()
            for row in rows:
                verify(key,row['problem'],row['answer'],row['solution_sketch'])
                self.assertTrue(row['authored_answer_decidable'])
                structures.add(re.sub(r'[-+]?\d+(?:/\d+)?','#',row['problem']))
                signatures.add(signature(key,row['problem']))
            self.assertEqual(len(structures),4,key)
            self.assertEqual(len(signatures),4,key)

    def test_full_cartesian_domains_materiality_and_entropy(self):
        for row in self.rows:
            key,args=row['kp_id'],row['arguments']
            self.assertEqual(row['status'],'pending')
            self.assertNotIn('space_size',args)
            self.assertEqual(args['constraints'],[])
            domains={k:v['values'] for k,v in args['params'].items()}
            samples={tuple(s['params'][k] for k in domains):s for s in args['samples']}
            wanted=set(product(*domains.values()))
            self.assertEqual(set(samples),wanted)
            self.assertGreaterEqual(len(wanted),12)
            outputs={}
            for params,s in samples.items():
                p=args['statement'].format(**s['params'])
                v=verify(key,p,s['expected'],args['solution_sketch'].format(**s['params']))
                outputs[params]=tuple(v.items())
            counts=Counter(outputs.values())
            self.assertGreaterEqual(len(counts),12,key)
            entropy=-sum(n/len(wanted)*log2(n/len(wanted)) for n in counts.values())
            self.assertGreaterEqual(entropy,log2(12)-1e-12,key)
            for a,b in product(wanted,repeat=2):
                if sum(x!=y for x,y in zip(a,b))==1:
                    self.assertNotEqual(outputs[a],outputs[b],(key,a,b))

    def test_production_rendered_instances_and_collision_scan(self):
        seen={}
        for key in self.keys:
            for ex in self.by_key[key]['exemplars']:
                sig=signature(key,ex['problem'])
                self.assertNotIn(sig,seen,(key,seen.get(sig)))
                seen[sig]=key
        self.assertEqual({r['kp_key'] for r in self.gate},self.keys)
        for row in self.gate:
            key=row['kp_key']
            self.assertTrue(row['exhaustive'])
            for instance in row['instances']:
                verify(key,instance['problem'],instance['answer'],instance['solution_sketch'])
                sig=signature(key,instance['problem'])
                self.assertNotIn(sig,seen,(key,seen.get(sig),instance['problem']))
                seen[sig]=key
        # Exact textual collision scan across all Foundations authored problems.
        authored={ex['problem'] for r in self.facts['kps'] for ex in r['exemplars']}
        generated=[s['problem'] for r in self.gate for s in r['instances']]
        self.assertFalse(authored&set(generated))
        self.assertEqual(len(generated),len(set(generated)))

    def test_semantic_negative_controls_for_every_exemplar_and_instance(self):
        for row in self.rows:
            key=row['kp_id']
            instances=next(r['instances'] for r in self.gate if r['kp_key']==key)
            for ex in self.by_key[key]['exemplars']+instances:
                actual=parsed_answer(ex['answer'])
                for field,value in actual.items():
                    wrong=dict(actual)
                    wrong[field]=not value if isinstance(value,bool) else (value[0]+1,value[1]) if isinstance(value,tuple) else value+1
                    bad=fields(**wrong)
                    with self.assertRaises(AssertionError):
                        verify(key,ex['problem'],bad)
                with self.assertRaises(AssertionError):
                    verify(key,ex['problem'],ex['answer'],'Use the appropriate rule.')
            for i,instance in enumerate(instances):
                with self.assertRaises(AssertionError):
                    verify(key,instance['problem'],instances[(i+1)%len(instances)]['answer'])


if __name__=='__main__':
    FACTS=sys.argv.pop(1)
    GATE=sys.argv.pop(1)
    unittest.main()
