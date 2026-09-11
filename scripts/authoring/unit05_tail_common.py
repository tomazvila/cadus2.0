"""Deterministic pending-only Unit05 authoring helpers; preserve unrelated YAML."""
import itertools
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]
DEST = ROOT / 'docs/content-foundations/unit05-systems-tail'
COORD = lambda n: {'kind': 'coordinates', 'arity': n}
YESNO = {'kind': 'label', 'options': [['yes'], ['no']]}
EXACT = {'kind': 'exact'}


def multipart(**parts):
    return {'kind': 'multipart', 'parts': [
        {'name': k, 'contract': v} for k, v in parts.items()]}


def exemplar(problem, answer, sketch):
    return dict(problem=problem, answer=answer, solution_sketch=sketch)


def recipe(key, contract, statement, expr, domains, expected, sketch, constraints=(), predicate=None):
    samples = []
    for values in itertools.product(*domains.values()):
        params = dict(zip(domains, values))
        if predicate and not predicate(**params):
            continue
        samples.append(dict(params=params, expected=expected(**params)))
    return dict(kp_id=key, kind='template', status='pending', arguments=dict(
        answer_contract=contract, statement=statement, answer_expr=expr,
        params={k: dict(kind='choice', values=v) for k, v in domains.items()},
        constraints=list(constraints), samples=samples, solution_sketch=sketch,
        hints=['Translate each relationship separately, preserving the requested order.']))


def write_batch(name, rows, authored):
    path = ROOT / 'curriculum/foundations/05-systems-inequalities.yaml'
    text = path.read_text()
    for row in rows:
        key = row['kp_id']
        topic, kp = key.split('/')
        start = text.index(f'  - id: {topic}\n')
        end = text.find('\n  - id:', start+1)
        end = len(text) if end == -1 else end
        section = text[start:end]
        lines = []
        for item in authored[key]:
            item = dict(item, answer_contract=row['arguments']['answer_contract'])
            for i, (field, value) in enumerate(item.items()):
                prefix = '          - ' if i == 0 else '            '
                lines.append(prefix + field + ': ' + json.dumps(value))
        pattern = rf'(      - id: {kp}\n.*?        exemplars:\n).*?(        constraints:)'
        section, count = re.subn(pattern, lambda m: m[1]+'\n'.join(lines)+'\n'+m[2],
                                section, count=1, flags=re.S)
        assert count == 1, key
        text = text[:start]+section+text[end:]
    path.write_text(text)
    DEST.mkdir(exist_ok=True)
    (DEST/f'{name}.json').write_text(json.dumps(rows, indent=2)+'\n')
