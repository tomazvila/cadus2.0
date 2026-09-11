"""Fail-closed source acceptance: real worker gate plus independent semantic checks."""
import argparse
import ast
from collections import Counter
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

from unit05_tail_common import DEST, ROOT
from unit05_tail_oracle import verify as verify_system
from test_unit05_tail_mixtures import verify as verify_mixture
sys.path.insert(0,str(ROOT/'scripts/review'))
from foundations_content_audit import build_report, pending_templates

BASELINE = '47f2532'
BATCHES = ('mixtures','checking','special','regions')


def run(*args,**kwargs):
    subprocess.run(args,cwd=ROOT,check=True,**kwargs)


def verify_source_scope(keys):
    path = 'curriculum/foundations/05-systems-inequalities.yaml'
    baseline = subprocess.check_output(['git','show',f'{BASELINE}:{path}'],cwd=ROOT,text=True)
    def strip_exemplars(text):
        for key in keys:
            topic,kp = key.split('/')
            start = text.index(f'  - id: {topic}\n')
            end = text.find('\n  - id:',start+1)
            end = len(text) if end < 0 else end
            section = text[start:end]
            pattern = rf'(      - id: {kp}\n.*?        exemplars:\n).*?(        constraints:)'
            section,count = re.subn(pattern,lambda m:m[1]+m[2],section,count=1,flags=re.S)
            assert count == 1
            text = text[:start]+section+text[end:]
        return text
    assert strip_exemplars(baseline) == strip_exemplars((ROOT/path).read_text())
    changed_core = subprocess.check_output(['git','diff',BASELINE,'--','crates/core/src'],cwd=ROOT)
    assert not changed_core, 'The current answer evaluator and gate thresholds must remain unchanged'


def limits():
    paths = list((ROOT/'scripts/authoring').glob('*unit05_tail*.py'))
    paths.append(ROOT/'crates/worker/tests/unit05_systems_tail.rs')
    largest_file,largest_function = 0,0
    for path in paths:
        source = path.read_text()
        length = len(source.splitlines())
        assert length < 500,(path,length)
        largest_file = max(largest_file,length)
        if path.suffix == '.py':
            tree = ast.parse(source)
            sizes = [n.end_lineno-n.lineno+1 for n in ast.walk(tree)
                     if isinstance(n,(ast.FunctionDef,ast.AsyncFunctionDef))]
        else:
            starts = [i for i,line in enumerate(source.splitlines()) if line.startswith('fn ')]
            lines = source.splitlines()
            sizes = [next(i for i in range(start,len(lines)) if lines[i] == '}')-start+1
                     for start in starts]
        assert all(n <= 70 for n in sizes),(path,sizes)
        largest_function = max(largest_function,max(sizes,default=0))
    return dict(largest_code_file_lines=largest_file,largest_function_lines=largest_function)


def summary(facts,rows):
    report = build_report(facts,pending_templates(ROOT/'docs/content-foundations'))
    keys = {r['kp_id'] for r in rows}
    selected = [r for r in report['kps'] if r['kp_key'] in keys]
    assert len(selected) == 13 and all(not r['issues'] for r in selected)
    before = json.loads((DEST/'baseline-audit.json').read_text())
    assert {r['kp_key'] for r in before['kps']} == keys
    assert all(any(i['code'] == 'absent_pending_template_recipe' for i in r['issues'])
               for r in before['kps'])
    assert report['issue_kps'] == before['issue_kps']-13
    metrics = []
    for row in rows:
        key,args = row['kp_id'],row['arguments']
        oracle = verify_mixture if key.startswith('systems-mixture-problems/') else verify_system
        outputs = [oracle(key,args['statement'].format(**s['params']),s['expected'])[1]
                   for s in args['samples']]
        counts = Counter(outputs)
        entropy = -sum(n/len(outputs)*math.log2(n/len(outputs)) for n in counts.values())
        metrics.append(dict(kp_key=key,exemplars=4,pending_instances=len(outputs),
                            distinct_answers=len(counts),answer_entropy_bits=round(entropy,6)))
    return dict(source_baseline=BASELINE,live_mapping='7bebe2aa',pending_only=True,
                closed=sorted(keys),issue_kps=report['issue_kps'],
                per_code_affected_kps=report['per_code_affected_kps'],kps=metrics,
                total_exemplars=52,total_pending_instances=sum(r['pending_instances'] for r in metrics))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--ignore-rust-version',action='store_true',
                        help='Explicitly record use of an older available compiler')
    args = parser.parse_args()
    compat = ['--ignore-rust-version'] if args.ignore_rust_version else []
    cargo = os.environ.get('CARGO','cargo')
    rows = [r for name in BATCHES for r in json.loads((DEST/f'{name}.json').read_text())]
    verify_source_scope({r['kp_id'] for r in rows})
    with tempfile.TemporaryDirectory(prefix='unit05-tail-') as temp:
        facts_path = Path(temp)/'facts.json'
        with facts_path.open('w') as out:
            run(cargo,'run',*compat,'--quiet','-p','cadus-core','--bin','content_audit_facts',
                '--','curriculum',stdout=out)
        for test in ('test_unit05_tail_mixtures','test_unit05_tail_systems',
                     'test_unit05_residual','test_unit05_residual_second'):
            run(sys.executable,str(ROOT/f'scripts/authoring/{test}.py'),str(facts_path))
        run(sys.executable,'-m','unittest','discover','-s','scripts/review',
            '-p','test_foundations_content_audit.py')
        run(cargo,'test',*compat,'-p','cadus-worker','--test','unit05_systems_tail',
            '--test','unit05_residual','--test','unit05_residual_second',
            env=dict(os.environ,SQLX_OFFLINE='true'))
        run(cargo,'fmt','--all','--check')
        run(cargo,'clippy',*compat,'-p','cadus-worker','--test','unit05_systems_tail','--','-D','warnings',
            env=dict(os.environ,SQLX_OFFLINE='true'))
        result = summary(json.loads(facts_path.read_text()),rows)
        result.update(limits())
        result['facts_sha256'] = hashlib.sha256(facts_path.read_bytes()).hexdigest()
    result['rustc'] = subprocess.check_output(['rustc','--version'],text=True).strip()
    result['rust_version_override'] = args.ignore_rust_version
    (DEST/'final-audit.json').write_text(json.dumps(result,indent=2)+'\n')
    run('git','diff','--check')
    print(f"UNIT05 SOURCE ACCEPTANCE OK: 13 closed; {result['total_pending_instances']} pending; "
          f"{result['issue_kps']} course residuals. No content approved.")


if __name__ == '__main__':
    main()
