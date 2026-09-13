#!/usr/bin/env python3
"""Source-bound semantic finite/bank overlay. ROOT must be an isolated checkout.
Usage: python3 apply-finite11.py ROOT --review INDEPENDENT-ACCEPTANCE.json
Acceptance evidence must name all target KP IDs under accepted_kp_ids, or as
objects with kp_id and decision=accept/accepted. No production publication.
"""
import argparse,copy,hashlib,json,re
from pathlib import Path
D=Path(__file__).resolve().parent
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('root',type=Path);p.add_argument('--review',type=Path)
p.add_argument('--preflight',action='store_true',help='Prepare temporary cache workspace for native gates before independent acceptance; never publication authority.')
p.add_argument('--plan',type=Path,default=D/'FINITE-POLICY-IMPLEMENTATION-v6.json')
p.add_argument('--prior-plan',type=Path,help='Permit exact semantic policy replacement from this prior immutable candidate in an existing stage.')
x=p.parse_args();root=x.root.resolve();report=json.loads(x.plan.read_text())
prior={e['kp_id']:e for e in json.loads(x.prior_plan.read_text())['targets']} if x.prior_plan else {}
def policy_content(v):
    return {k:value for k,value in v.items() if k!='review_ref'} if v else None
def sha(v):return 'sha256:'+hashlib.sha256(v).hexdigest()
if x.preflight:
    assert '/.cache/' in str(root) and root.name=='workspace','Preflight requires the isolated cache workspace'
    review={'candidate_sha256':sha(x.plan.read_bytes()),'status':'technical_preflight_only_not_independent_acceptance','accepted_kp_ids':[]}
    review_bytes=(json.dumps(review,indent=2)+'\n').encode()
else:
    assert x.review,'--review is required for accepted application'
    review_bytes=x.review.read_bytes();review=json.loads(review_bytes)
assert review.get('candidate_sha256')==sha(x.plan.read_bytes()),'Independent review must bind the exact candidate_sha256'
def canon(v):return json.dumps(v,sort_keys=True,separators=(',',':'),ensure_ascii=False)
accepted=set()
def accept_walk(obj):
    if isinstance(obj,list):
        for v in obj:accept_walk(v)
    elif isinstance(obj,dict):
        accepted.update(obj.get('accepted_kp_ids',[]))
        if obj.get('decision') in ('accept','accepted') and obj.get('kp_id'):accepted.add(obj['kp_id'])
        for v in obj.values():accept_walk(v)
accept_walk(review)
entries={e['kp_id']:copy.deepcopy(e) for e in report['targets']}
assert x.preflight or set(entries)<=accepted,'Independent evidence does not explicitly accept every target: '+str(sorted(set(entries)-accepted))
ref=sha(review_bytes);updates={};counts={};mirror_mismatches=[];fixture_skips=[]
def read(path):return updates.get(path,path.read_text())
def put(path,obj):updates[path]=json.dumps(obj,indent=2,ensure_ascii=True)+'\n'
def bound(text,key):
    topic,kp=key.split('/')
    tm=re.search(r'^  - id: '+re.escape(topic)+r'\s*$',text,re.M)
    if not tm:
        fm=re.search(r'^  \{"id": "'+re.escape(topic)+r'"',text,re.M);assert fm,key
        start=fm.start()+2;_,size=json.JSONDecoder().raw_decode(text[start:])
        km=re.search(r'^    \{"id": "'+kp+r'"',text[start:start+size],re.M);assert km,key
        lo=start+km.start()+4;_,size=json.JSONDecoder().raw_decode(text[lo:]);return lo,lo+size,True
    tn=re.search(r'^  - id: ',text[tm.end():],re.M);end=tm.end()+tn.start() if tn else len(text)
    km=re.search(r'^      - id: '+kp+r'\s*$',text[tm.start():end],re.M);assert km,key
    lo=tm.start()+km.start();kn=re.search(r'^      - id: ',text[tm.start()+km.end():end],re.M)
    hi=tm.start()+km.end()+kn.start() if kn else end
    return lo,hi,False
for key,e in entries.items():
    policy=e['finite_objective_domain']
    if policy is None:continue
    policy['review_ref']=ref
    path=root/e['source']['curriculum_path'];text=read(path);lo,hi,flow=bound(text,key)
    old_policy=json.loads(text[lo:hi]).get('finite_objective_domain') if flow else None
    if flow and old_policy==policy:continue
    if not flow:
        existing=re.search(r'^        finite_objective_domain: (.+)$',text[lo:hi],re.M)
        if existing and json.loads(existing.group(1))==policy:continue
        old_policy=json.loads(existing.group(1)) if existing else None
    if old_policy and key in prior:
        assert policy_content(old_policy)==policy_content(prior[key]['finite_objective_domain']),'Prior policy drift: '+key
        section=text[lo:hi]
        if flow:
            marker=re.search(r'"finite_objective_domain"\s*:\s*',section);assert marker
            _,size=json.JSONDecoder().raw_decode(section[marker.end():]);start=marker.end();stop=start+size
        else:
            start=existing.start(1);stop=existing.end(1)
        updates[path]=text[:lo]+section[:start]+json.dumps(policy,ensure_ascii=True)+section[stop:]+text[hi:]
        continue
    assert sha(text[lo:hi].encode())==e['source']['original_stanza_sha256'],'Curriculum target drift: '+key
    if flow:
        kp=json.loads(text[lo:hi]);assert 'finite_objective_domain' not in kp
        # Add one property to the existing JSON-shaped YAML object, preserving its content.
        replacement=text[lo:hi-1]+', "finite_objective_domain": '+json.dumps(policy,ensure_ascii=True)+'}'
    else:
        assert 'finite_objective_domain:' not in text[lo:hi]
        line_end=text.index('\n',lo)+1
        replacement=text[lo:line_end]+'        finite_objective_domain: '+json.dumps(policy,ensure_ascii=True)+'\n'+text[line_end:hi]
    updates[path]=text[:lo]+replacement+text[hi:]
# Patch only active authoring inputs and declared current mirrors. Archives, reviews,
# prior gates and proposal files are intentionally outside this exact path list.
for rel in report['active_template_sources']:
    path=root/rel
    if not path.exists():
        if rel.endswith('/inputs/templates.json'):raise FileNotFoundError(path)
        continue
    obj=json.loads(read(path));changed=[0]
    def replace_templates(node):
        if isinstance(node,list):
            for value in node:replace_templates(value)
        elif isinstance(node,dict):
            key=node.get('kp_id');e=entries.get(key)
            if e and node.get('kind')=='template':
                mirror=next((m for m in report['active_template_mirrors'] if m['path']==rel and m['kp_id']==key),None)
                assert mirror,'Unreviewed mirror: '+rel+' '+key
                before=mirror['before'];after=mirror['after']
                if node==after:
                    counts[key]=counts.get(key,0)+1
                    return
                if node!=before:
                    mirror_mismatches.append({'path':rel,'kp_id':key,'reason':'current mirror differs from reviewed selected template; must resolve before applying'})
                    return
                node.clear();node.update(copy.deepcopy(after));changed[0]+=1;counts[key]=counts.get(key,0)+1
                return
            for value in node.values():replace_templates(value)
    replace_templates(obj)
    if changed[0]:put(path,obj)
assert not mirror_mismatches,json.dumps(mirror_mismatches,indent=2)
# Every changed selected template must have been found in current source.
for key,e in entries.items():
    assert e['replacement_template']==e['original_template'] or counts.get(key,0), 'Missing active template: '+key
found=set()
for path in sorted((root/'docs/content-foundations/whole-course-teach/drafts').glob('part-*.json')):
    obj=json.loads(read(path));changed=False
    for row in obj:
        key=row.get('kp_id');e=entries.get(key)
        if not e:continue
        found.add(key)
        if row==e['replacement_teach']:continue
        assert row==e['original_teach'],'Teach target drift: '+key
        row.clear();row.update(copy.deepcopy(e['replacement_teach']));changed=True
    if changed:put(path,obj)
assert set(entries)<=found,'Missing Teach targets: '+str(sorted(set(entries)-found))
fixture=root/'scripts/authoring/testdata/foundations_topics.json'
if fixture.exists():
    obj=json.loads(read(fixture));touched=False
    for topic in obj:
        for kp in topic.get('knowledge_points',[]):
            e=entries.get(topic['id']+'/'+kp['id'])
            if e and e['finite_objective_domain']:
                variants=[v for c in e['finite_objective_domain']['cases'] for v in c['variants']]
                if not all(any(ex['problem']==v['problem'] and ex['answer']==v['answer'] and ex.get('answer_contract')==v.get('answer_contract') for v in variants) for ex in kp.get('exemplars',[])):
                    fixture_skips.append(e['kp_id'])
                    continue
                if kp.get('finite_objective_domain')==e['finite_objective_domain']:continue
                if kp.get('finite_objective_domain'):
                    assert e['kp_id'] in prior and policy_content(kp['finite_objective_domain'])==policy_content(prior[e['kp_id']]['finite_objective_domain']),'Existing fixture policy requires review'
                kp['finite_objective_domain']=e['finite_objective_domain'];touched=True
    if touched:put(fixture,obj)
overlay=root/'scripts/authoring/companion_template_repairs.json'
if overlay.exists():
    obj=json.loads(read(overlay));assert isinstance(obj,dict)
    for key,e in entries.items():
        if e['replacement_template']==e['original_template']:continue
        args=e['replacement_template']['arguments']
        assert key not in obj or obj[key]==args,'Conflicting generator overlay: '+key
        obj[key]=args
    put(overlay,obj)
evidence_dir=root/'docs/content-foundations/finite-objective-domains/adoption-2026-09-13'
updates[evidence_dir/('technical-preflight.json' if x.preflight else 'independent-review.json')]=review_bytes.decode()
updates[evidence_dir/'repair-plan.json']=x.plan.read_text()
updates[evidence_dir/'apply-finite11.py']=Path(__file__).read_text()
updates[evidence_dir/'REGENERATE.md']='# Finite teaching reservations\nAfter regenerating any active template source, replay the source-bound `apply-finite11.py` overlay using this repair plan and independent review. Run the instruction and template gates before authoring/import. Original historical evidence stays immutable.\n\nSeven finite policies group exact variants by mathematical case. Each case declares its teaching and practice role. Three policies include explicitly declared rehearsal cases. Fresh and reserved cases remain protected. Eight templates exclude exact worked bindings; two also have finite policies, leaving six bank-only targets.\n'
# All preconditions have passed before the first mutation.
for path,text in updates.items():
    path.parent.mkdir(parents=True,exist_ok=True);path.write_text(text)
print(json.dumps({'applied_paths':[str(p.relative_to(root)) for p in updates],'template_replacements':counts,'historical_fixture_policy_skips':fixture_skips,'review_ref':ref,'native_gates':'not run by this applicator'},indent=2))
