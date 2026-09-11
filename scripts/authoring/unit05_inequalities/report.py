"""Attach source-only audit deltas and current worker evidence, without approval."""
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
from math import log2
from pathlib import Path
import re
import sys

from build import ROOT, OUT, YAML, generate
from test_semantic import material
sys.path.insert(0,str(ROOT/'scripts/review'))
from foundations_content_audit import build_report, pending_templates


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def summary(audit,keys=None):
    rows=[r for r in audit['kps'] if keys is None or r['kp_key'] in keys]
    counts={code:sum(any(i['code']==code for i in r['issues']) for r in rows)
            for code in audit['per_code_affected_kps']}
    return dict(knowledge_points=len(rows),issue_kps=sum(bool(r['issues']) for r in rows),
                per_code_affected_kps=counts)


def unit_keys():
    topic=None
    keys=set()
    for line in YAML.read_text().splitlines():
        if m:=re.match(r'  - id: ([\w-]+)',line): topic=m[1]
        if m:=re.match(r'      - id: (kp\d+)',line): keys.add(f'{topic}/{m[1]}')
    assert len(keys)==75
    return keys


def metrics(recipes):
    result=[]
    for row in recipes:
        args=row['arguments']
        families=material(args)
        counts=Counter(s['expected'] for s in args['samples'])
        total=sum(counts.values())
        result.append(dict(kp_id=row['kp_id'],semantic_families=len(families),
            samples=total,distinct_answers=len(counts),
            answer_entropy_bits=-sum(n/total*log2(n/total) for n in counts.values()),
            numeric_axes=[k for k,v in args['params'].items() if len(v['values'])>1]))
    return result


def main():
    facts_path=Path(sys.argv[1]); gate_path=Path(sys.argv[2])
    facts=json.loads(facts_path.read_text())
    before=json.loads((OUT/'baseline-audit.json').read_text())
    after=build_report(facts,pending_templates(ROOT/'docs/content-foundations'))
    authored,recipes=generate()
    owned=set(json.loads((OUT/'scope.json').read_text()))
    assert all(not r['issues'] for r in after['kps'] if r['kp_key'] in authored)
    gates=json.loads(gate_path.read_text())
    assert len(gates)==27
    for row in gates:
        if 'body' in row: row['body_sha256']=hashlib.sha256(row['body'].encode()).hexdigest()
    (OUT/'production-gates.json').write_text(json.dumps(gates,indent=2)+'\n')
    report=dict(local_baseline='a325d867506af81cd88bb005e1d4d6e6da83af23',
        remote_integration='69e4f650',generated_utc=datetime.now(timezone.utc).isoformat(),
        source_status='pending only; no import or approval',accepted_keys=sorted(authored),
        blocked_keys=sorted(owned-set(authored)),authored_exemplars=sum(map(len,authored.values())),
        exhaustive_instances=sum(len(r['arguments']['samples']) for r in recipes),
        facts_sha256=digest(facts_path),curriculum_sha256=digest(YAML),
        templates_sha256=digest(OUT/'templates.json'),recipe_metrics=metrics(recipes),
        before={name:summary(before,keys) for name,keys in [('foundations',None),('unit05',unit_keys()),('owned',owned)]},
        after={name:summary(after,keys) for name,keys in [('foundations',None),('unit05',unit_keys()),('owned',owned)]},
        selected_after=[r for r in after['kps'] if r['kp_key'] in owned])
    (OUT/'final-audit.json').write_text(json.dumps(report,indent=2)+'\n')
    selected=[r for r in facts['kps'] if r['kp_key'] in authored]
    (OUT/'authored-facts.json').write_text(json.dumps(selected,indent=2)+'\n')
    print(json.dumps({k:report[k] for k in ['authored_exemplars','exhaustive_instances','before','after']},indent=2))


if __name__=='__main__': main()
