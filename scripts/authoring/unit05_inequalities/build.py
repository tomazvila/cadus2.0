"""Generate pending-only source and replace only owned exemplar blocks."""
import argparse
from itertools import product
import json
from pathlib import Path
import re

from linear import populate
from sets import populate as populate_sets
import graphs
from rendering import render_all
import semantic_sets
import semantic_labels
from labels import populate as populate_labels, DATA as LABELS
from semantic import reconstruct as linear_answer, sketch as linear_sketch

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT/'docs/content-foundations/unit05-inequalities'
YAML = ROOT/'curriculum/foundations/05-systems-inequalities.yaml'


def is_set(problem):
    problem=problem.replace(r'\le','<=').replace(r'\ge','>=')
    return '|' in problem or len(re.findall(r'<=|>=|<|>',problem))==2


def reconstruct(problem):
    if semantic_labels.is_label(problem): return semantic_labels.reconstruct(problem)
    return semantic_sets.reconstruct(problem) if is_set(problem) else linear_answer(problem)


def sketch(problem):
    if semantic_labels.is_label(problem): return semantic_labels.sketch(problem)
    return semantic_sets.sketch(problem) if is_set(problem) else linear_sketch(problem)


def generate():
    exemplars, recipes = {}, []
    def add(key, authored, statement, expression, aa, bb, contract):
        exemplars[key] = [dict(problem=p, answer=reconstruct(p),
            answer_contract=contract, solution_sketch=sketch(p)) for p in authored]
        domains={'a':aa} if bb is None else {'a':aa,'b':bb}
        bindings=[dict(zip(domains,values)) for values in product(*domains.values())]
        samples=[dict(params=p,expected=reconstruct(statement.format(**p))) for p in bindings]
        recipes.append(dict(kp_id=key,kind='template',status='pending',arguments=dict(
            statement=statement,answer_expr=expression,answer_contract=contract,
            params={k:dict(kind='choice',values=v) for k,v in domains.items()},
            constraints=[],samples=samples,distractors=[],
            solution_sketch=template_sketch(key,statement),
            hints=['Isolate the variable. Check the sign of the coefficient before dividing.'])))
    populate(add)
    populate_sets(add)
    populate_labels(add)
    graph_authored,graph_recipes=graphs.generate()
    exemplars.update(graph_authored)
    recipes.extend(graph_recipes)
    render_all(exemplars,recipes)
    return exemplars, recipes


def template_sketch(key,statement):
    if semantic_labels.is_label(statement): return next(r[-1] for r in LABELS if r[0]==key)
    return set_sketch(statement) if is_set(statement) else recipe_sketch(key)


def set_sketch(statement):
    if '2|3x' in statement:
        return 'Subtract 5 and divide by 2 to get $|3x-{a}| <= ({b}-5)/2$. Bound the inner expression on both sides, add {a} throughout, then divide by positive 3.'
    if '|2x' in statement:
        return 'The absolute-value bound becomes $-{b} <= 2x-{a} <= {b}$. Add {a} throughout, then divide by positive 2. Both endpoints are included.'
    if '|' in statement:
        return 'A magnitude below {a} places x between -{a} and {a}: $-{a} < x < {a}$. Both endpoints are excluded.'
    if 'interval notation' in statement:
        return 'The lower boundary is -{a} and is excluded, so use a round bracket. The upper boundary is {b} and is included, so use a square bracket.'
    if '-3x' in statement:
        return 'Subtract 2 throughout, then divide by -3 and reverse both comparisons: $(2-{b})/3 <= x < ({a}+2)/3$.'
    return 'Subtract 4 from all three parts: $-{a}-4 < x < {b}-4$. Both strict comparisons exclude the endpoints.'


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
    text=''.join(lines)
    text=text.replace('\"shade_toward\":{\"x\":\"0\",\"y\":\"0\"},\"label\":\"y ≤ x - 1\"',
                      '\"shade_toward\":{\"x\":\"0\",\"y\":\"-2\"},\"label\":\"y ≤ x - 1\"')
    text=text.replace('the origin test on y ≤ x - 1 shades the side containing (0,0).',
                      'the origin fails y ≤ x - 1. Shade the side containing (0,-2), which satisfies the inequality.')
    YAML.write_text(text)


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
