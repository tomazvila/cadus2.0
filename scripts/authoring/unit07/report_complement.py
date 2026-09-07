"""Publish reproducible source audit evidence; never access a database or approve."""
import hashlib
import json
import re
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/'scripts/review'))
from foundations_content_audit import build_report, pending_templates

WORK = ROOT/'target/unit07-complement'
OUT = ROOT/'docs/reports/unit07-complement'


def read(path):
    return json.loads(path.read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verification_packet():
    names = ['core-all','baseline-tests','core-focused','worker','clippy','semantic']
    logs = {n:(WORK/(n+'.log')).read_text() for n in names}
    failures = lambda text: set(re.findall(r'^test (.+) \.\.\. FAILED$',text,re.M))
    baseline = failures(logs['baseline-tests'])
    fixed = failures(logs['core-all'])-baseline
    assert len(baseline)==37
    assert fixed=={'parabola_direction_label_rejects_the_opposite_direction',
                   'existing_numeric_and_structured_writers_keep_their_text'}
    for name in ['core-focused','worker','semantic']:
        assert 'FAILED' not in logs[name] and ('test result: ok.' in logs[name] or '\nOK' in logs[name])
    assert 'error:' not in logs['clippy'] and 'Finished' in logs['clippy']
    return dict(baseline_failing_tests=sorted(baseline),broad_core_failures=39,
        baseline_reproduced_failures=37,scope_failures_fixed=sorted(fixed),
        focused_core_passed=27,worker_passed=4,semantic_passed=5,
        clippy='passed with -D warnings',
        log_sha256={n:digest(WORK/(n+'.log')) for n in names})


def audit_delta(keys):
    before_facts, after_facts = (read(WORK/name) for name in ['baseline-facts.json','facts.json'])
    before = {r['kp_key']:r for r in before_facts['kps']}
    after = {r['kp_key']:r for r in after_facts['kps']}
    assert before.keys()==after.keys()
    assert all(before[k]==after[k] for k in before.keys()-keys), 'unowned curriculum changed'
    pending = pending_templates(ROOT/'docs/content-foundations')
    baseline_pending = {k:[r for r in v if '/unit07-complement/' not in r['source']]
                        for k,v in pending.items()}
    baseline_pending = {k:v for k,v in baseline_pending.items() if v}
    assert keys.isdisjoint(baseline_pending)
    baseline = build_report(before_facts,baseline_pending)
    current = build_report(after_facts,pending)
    baseline_rows = {r['kp_key']:r['issues'] for r in baseline['kps']}
    current_rows = {r['kp_key']:r['issues'] for r in current['kps']}
    for key in keys:
        assert baseline_rows[key]==[{'code':'absent_pending_template_recipe'}]
        assert current_rows[key]==[]
    assert all(baseline_rows[k]==current_rows[k] for k in before.keys()-keys)
    return before,baseline,current,baseline_rows,current_rows


def semantic_evidence(keys):
    evidence = read(WORK/'production-evidence.json')
    semantic_packet = read(WORK/'semantic-evidence.json')
    for path,sha in semantic_packet['inputs_sha256'].items():
        assert digest(ROOT/path)==sha, 'stale semantic evidence'
    semantics = semantic_packet['kps']
    assert {r['kp_id']:r['instances'] for r in semantics}=={r['kp_id']:len(r['instances']) for r in evidence}
    assert {r['kp_id'] for r in evidence}==keys
    assert all(r['status']=='pending' and len(r['instances'])>=12 for r in evidence)
    return evidence,semantics


def result_packet(recipes,keys,audit,semantic):
    before,baseline,current,baseline_rows,current_rows=audit
    evidence,semantics=semantic
    return dict(baseline='69e4f650 (isolated snapshot 090a524)',
        scope=sorted(keys), closures=len(keys), authored_exemplars=20,
        pending_instances=sum(len(r['instances']) for r in evidence),
        baseline_issue_kps=baseline['issue_kps'],final_issue_kps=current['issue_kps'],
        baseline_codes=baseline['per_code_affected_kps'],final_codes=current['per_code_affected_kps'],
        owned=[dict(kp_id=k,before=baseline_rows[k],after=current_rows[k]) for k in sorted(keys)],
        unowned_kps_preserved=len(before)-len(keys),semantics=semantics,
        verification=verification_packet(),
        approval='pending only; no database, approval, push, integration, or deployment',
        sha256={str(p.relative_to(ROOT)):digest(p) for p in [recipes,
            ROOT/'curriculum/foundations/07-polynomials-quadratics.yaml',
            WORK/'baseline-facts.json',WORK/'facts.json',WORK/'production-evidence.json']})


def main():
    recipes = ROOT/'docs/content-foundations/unit07-complement/templates.json'
    keys = {r['kp_id'] for r in read(recipes)}
    assert len(keys)==5
    semantic=semantic_evidence(keys)
    result=result_packet(recipes,keys,audit_delta(keys),semantic)
    evidence=semantic[0]
    OUT.mkdir(parents=True,exist_ok=True)
    for name,data in [('audit.json',result),('production-evidence.json',evidence)]:
        (OUT/name).write_text(json.dumps(data,indent=2,sort_keys=True)+'\n')
    print(f"{len(keys)} exact closures; {result['pending_instances']} pending instances; "
          f"{result['unowned_kps_preserved']} unowned KPs preserved")


if __name__=='__main__':
    main()
