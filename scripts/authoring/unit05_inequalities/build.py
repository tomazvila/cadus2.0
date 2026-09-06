"""Generate pending-only source and replace only owned exemplar blocks."""
import argparse
from itertools import product
import json
from pathlib import Path
import re

from linear import populate
from semantic import reconstruct, sketch

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT/'docs/content-foundations/unit05-inequalities'
YAML = ROOT/'curriculum/foundations/05-systems-inequalities.yaml'


def generate():
    exemplars, recipes = {}, []
    def add(key, authored, statement, expression, aa, bb, contract):
        exemplars[key] = [dict(problem=p, answer=reconstruct(p),
            answer_contract=contract, solution_sketch=sketch(p)) for p in authored]
        samples = [dict(params={'a':a,'b':b}, expected=reconstruct(statement.format(a=a,b=b)))
                   for a,b in product(aa,bb)]
        recipes.append(dict(kp_id=key,kind='template',status='pending',arguments=dict(
            statement=statement,answer_expr=expression,answer_contract=contract,
            params={k:dict(kind='choice',values=v) for k,v in [('a',aa),('b',bb)]},
            constraints=[],samples=samples,distractors=[],
            solution_sketch=recipe_sketch(key),
            hints=['Isolate the variable. Check the sign of the coefficient before dividing.'])))
    populate(add)
    return exemplars, recipes


def recipe_sketch(key):
    return {
      'one-step-inequalities/kp1':'Subtract {a} from both sides to get $x <= {b}-{a}$. The endpoint is included.',
      'one-step-inequalities/kp2':'Multiply both sides by positive {a}: $x >= {a}*{b}$. The endpoint is included.',
      'two-step-inequalities/kp1':'Subtract {a}, then divide by positive 3: $x <= ({b}-{a})/3$. Equality is included.',
      'two-step-inequalities/kp2':'Add 7, then multiply by positive {a}: $x >= {a}*({b}+7)$. Equality is included.',
      'linear-inequalities/kp1':'Subtract {a} and divide by -3, reversing the sign: $x >= ({a}-{b})/3$.',
      'linear-inequalities/kp2':'Distribute, then subtract x: $3x <= 4*{a}+{b}$. Divide by positive 3.',
      'linear-inequalities/kp3':'Move 2x right and the constant left: ${a}+{b} >= 3x$. Divide by positive 3.',
      'inequality-word-problems/kp1':'Remove the delivery fee, then divide by the pack price: $x <= ({b}-{a})/7$. Take the greatest whole count below this boundary.',
    }[key]


def patch(exemplars):
    text = YAML.read_text()
    topic = kp = None
    lines = text.splitlines(keepends=True)
    changes=[]
    for i,line in enumerate(lines):
        if m:=re.match(r'  - id: ([\w-]+)',line): topic=m[1]
        if m:=re.match(r'      - id: (kp\d+)',line): kp=m[1]
        key=f'{topic}/{kp}'
        if line.strip()!='exemplars:' or key not in exemplars: continue
        j=i+1
        while j<len(lines) and (not lines[j].strip() or len(lines[j])-len(lines[j].lstrip())>8): j+=1
        block=''
        for row in exemplars[key]:
            for n,(field,val) in enumerate(row.items()):
                prefix='          - ' if n==0 else '            '
                block+=prefix+field+': '+json.dumps(val,ensure_ascii=False)+'\n'
        changes.append((i+1,j,block))
    assert len(changes)==len(exemplars)
    for i,j,block in reversed(changes): lines[i:j]=[block]
    YAML.write_text(''.join(lines))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write',action='store_true')
    args=parser.parse_args()
    exemplars,recipes=generate()
    if args.write:
        patch(exemplars)
        (OUT/'templates.json').write_text(json.dumps(recipes,indent=2)+'\n')
    print(f'{len(exemplars)} KPs; {sum(map(len,exemplars.values()))} exemplars; pending only')


if __name__=='__main__': main()
