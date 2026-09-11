"""Publish already worker-gated local rows and retire only unit00's four old drafts."""
import argparse
import json
from pathlib import Path


def read(path):
    return json.loads(path.read_text())


def compact_rows(path,rows):
    path.write_text('[\n'+',\n'.join(json.dumps(r,ensure_ascii=False,separators=(',',':'))
                                  for r in rows)+'\n]\n')


def validate_gate(drafts,stored,evidence):
    assert len(drafts)==len(stored)==len(evidence)
    for draft,row,proof in zip(drafts,stored,evidence):
        assert draft['kp_id']==row['kp_id']==proof['kp_id']
        assert set(draft)=={'kp_id','kind','arguments'} and row['status']=='pending'
        assert row['digest']==proof['digest']
        assert proof['exhaustive'] and proof['distinct_valid_instances']>=12
        assert proof['authored_or_sibling_collisions']==0


def retire_old_drafts(root,old_keys):
    retired=[]
    for name in ('drafts.json','stored-review.json'):
        path=root/'docs/content-foundations/zero-api-completion'/name
        old=read(path)
        removed=[r for r in old if r['kp_id'] in old_keys and r['kind']=='template']
        assert len(removed) in (0,4)
        if name=='stored-review.json':
            retired=[{'kp_key':r['kp_id'],'previous_digest':r['digest']} for r in removed]
        keep=[r for r in old if r not in removed]
        path.write_text(json.dumps(keep,ensure_ascii=False,indent=2)+'\n')
    return retired


def publish(root,candidates,gate):
    drafts=read(candidates)
    stored=read(gate/'stored.json')
    evidence=read(gate/'evidence.json')
    assert not read(gate/'rejected.json'), 'resolve gate rejections before publishing'
    validate_gate(drafts,stored,evidence)
    folder=root/'docs/content-foundations/unit00-templates'
    folder.mkdir(exist_ok=True)
    for name,rows in [('drafts.json',drafts),('stored-review.json',stored),('gate-evidence.json',evidence)]:
        compact_rows(folder/name,rows)
    old_keys={'subtraction-facts/kp2','multiplication-tables/kp1',
              'whole-number-exponents/kp2','expressions-with-parentheses/kp2'}
    retired=retire_old_drafts(root,old_keys)
    if retired:
        compact_rows(root/'docs/reports/unit00-retired-pending-digests.json',retired)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,default=Path.cwd())
    parser.add_argument('--candidates',type=Path,required=True)
    parser.add_argument('--gate',type=Path,required=True)
    args=parser.parse_args()
    publish(args.root,args.candidates,args.gate)
