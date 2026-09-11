"""Record reproducible audit deltas and source-only gate evidence."""
import argparse
import ast
import hashlib
import json
from pathlib import Path
import sys

from unit04_complement_common import OUT, ROOT
sys.path.insert(0,str(ROOT/'scripts/review'))
from foundations_content_audit import build_report, pending_templates


def read(path):
    return json.loads(Path(path).read_text())


def summary(report, keys=None):
    rows=[r for r in report['kps'] if keys is None or r['kp_key'] in keys]
    return {'knowledge_points':len(rows),'issue_kps':sum(bool(r['issues']) for r in rows),
            'per_code_affected_kps':{c:sum(any(i['code']==c for i in r['issues']) for r in rows)
                                     for c in report['per_code_affected_kps']}}


def python_function_sizes(path,source):
    functions=[]
    for node in ast.walk(ast.parse(source)):
        if isinstance(node,(ast.FunctionDef,ast.AsyncFunctionDef)):
            size=node.end_lineno-node.lineno+1
            assert size<=70,(path,node.name,size)
            functions.append(size)
    return functions


def rust_function_sizes(source):
    # Rustfmt places top-level closing braces at column zero.
    functions=[]
    start=None
    for i,line in enumerate(source.splitlines()):
        if line.startswith('fn '):
            start=i
        if line=='}' and start is not None:
            functions.append(i-start+1)
            start=None
    assert max(functions)<=70
    return functions


def limits():
    result=[]
    files=list((ROOT/'scripts/authoring').glob('*unit04_complement*.py'))
    files.append(ROOT/'crates/worker/examples/unit04_complement_gate.rs')
    for path in files:
        source=path.read_text()
        assert len(source.splitlines())<500,path
        functions=(python_function_sizes(path,source) if path.suffix=='.py'
                   else rust_function_sizes(source))
        result.append({'file':str(path.relative_to(ROOT)),'lines':len(source.splitlines()),
                       'max_function_lines':max(functions,default=0)})
    return result


def unit_keys():
    topics={line.strip().split(': ')[-1]
            for line in (ROOT/'curriculum/foundations/04-linear-graphs.yaml').read_text().splitlines()
            if line.startswith('  - id: ')}
    return {r['kp_key'] for r in read(ROOT/'.tooling/before-facts.json')['kps']
            if r['kp_key'].split('/')[0] in topics}


def assert_audit_delta(before,after,keys):
    assert all(not r['issues'] for r in after['kps'] if r['kp_key'] in keys)
    untouched_before={r['kp_key']:r for r in before['kps'] if r['kp_key'] not in keys}
    untouched_after={r['kp_key']:r for r in after['kps'] if r['kp_key'] not in keys}
    assert untouched_before==untouched_after


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('stage',choices=['checkpoint','final'])
    args=parser.parse_args()
    before=read(ROOT/'.tooling/before-audit.json')
    facts=read(ROOT/'.tooling/after-facts.json')
    after=build_report(facts,pending_templates(ROOT/'docs/content-foundations'))
    recipes=read(OUT/'templates.json')
    keys={r['kp_id'] for r in recipes}
    unit=unit_keys()
    assert_audit_delta(before,after,keys)
    gate=read(ROOT/'.tooling/gate.json')
    assert {r['kp_key'] for r in gate}==keys
    report={'baseline':'d0be8747 (current integration)','stage':args.stage,
            'closed_source_kps':sorted(keys),'exemplars':4*len(keys),
            'production_instances':sum(len(r['instances']) for r in gate),
            'before':{'foundations':summary(before),'unit04':summary(before,unit)},
            'after':{'foundations':summary(after),'unit04':summary(after,unit)},
            'selected_before':[r for r in before['kps'] if r['kp_key'] in keys],
            'selected_after':[r for r in after['kps'] if r['kp_key'] in keys],
            'outside_scope_audit_unchanged':True,'limits':limits(),
            'sha256':{p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in
                      ['curriculum/foundations/04-linear-graphs.yaml',
                       'docs/content-foundations/unit04-complement/templates.json']},
            'approval':'pending only; no database writes, integration, push, or deployment'}
    (OUT/f'audit-{args.stage}.json').write_text(json.dumps(report,indent=2)+'\n')
    (OUT/f'gate-{args.stage}.json').write_text(json.dumps(gate,indent=2)+'\n')
    print(json.dumps({k:report[k] for k in ('closed_source_kps','before','after')},indent=2))


if __name__=='__main__':
    main()
