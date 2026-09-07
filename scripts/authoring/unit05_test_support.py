"""Shared fixture and materiality checks for Unit05 authoring tests."""
import json
from pathlib import Path


def load_facts_recipes(facts_path,recipe_paths):
    facts=json.loads(Path(facts_path).read_text())
    kps={row['kp_key']:row for row in facts['kps']}
    recipes=[]
    for path in recipe_paths:
        recipes.extend(json.loads(Path(path).read_text()))
    return facts,kps,recipes


def assert_pending_audit(test,report,keys,*,key_message=False):
    expected=['pending_template_production_gate_declined']
    for row in report['kps']:
        if row['kp_key'] in keys:
            message=row['kp_key'] if key_message else None
            test.assertEqual([issue['code'] for issue in row['issues']],expected,message)


def collect_recipe_outputs(test,key,args,verify,seen):
    domains={name:domain['values'] for name,domain in args['params'].items()}
    outputs={}
    for sample in args['samples']:
        params=tuple(sample['params'][name] for name in domains)
        test.assertNotIn(params,outputs)
        problem=args['statement'].format(**sample['params'])
        signature,output=verify(key,problem,sample['expected'])
        test.assertNotIn(signature,seen,(key,seen.get(signature),problem))
        seen[signature]=key
        outputs[params]=output
    return domains,outputs


def extend_pending_signatures(test,seen,pending,keys,reader):
    count=0
    for key,rows in pending.items():
        if key in keys:
            continue
        for row in rows:
            args=row['document'].get('arguments',{})
            for sample in args.get('samples',[]):
                problem=args['statement'].format(**sample['params'])
                signature=reader(key,problem,sample['expected'])
                if signature is not None:
                    test.assertNotIn(seen.get(signature),keys)
                    seen[signature]=key
                    count+=1
    test.assertGreaterEqual(count,96)


def assert_pairwise_active(test,tuples,domains,outputs):
    for a,b in tuples:
        for other in domains['a']:
            if other != a:
                test.assertNotEqual(outputs[a,b],outputs[other,b])
        for other in domains['b']:
            if other != b:
                test.assertNotEqual(outputs[a,b],outputs[a,other])
