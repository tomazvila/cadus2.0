"""Offline, scoped Unit04 source authoring helpers; never touches a database."""
from fractions import Fraction as Q
from itertools import product
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'docs/content-foundations/unit04-complement'
EXACT = {'kind': 'exact'}
YESNO = {'kind': 'label', 'options': [['yes'], ['no']]}


def multipart(*names):
    return {'kind': 'multipart', 'parts': [
        {'name': name, 'contract': {'kind':'coordinates','arity':2} if label=='pair' else YESNO if label else EXACT}
        for name, label in names]}


def text(value):
    if isinstance(value, tuple):
        return '('+', '.join(text(v) for v in value)+')'
    if isinstance(value, bool):
        return 'yes' if value else 'no'
    return str(Q(value))


def fields(**values):
    return '; '.join(f'{name} = {text(value)}' for name, value in values.items())


def example(problem, answer, sketch, contract=EXACT):
    return dict(problem=problem, answer=answer, solution_sketch=sketch,
                answer_contract=contract)


def recipe(key, statement, expression, sketch, domains, solve, contract=EXACT):
    samples = []
    for values in product(*domains.values()):
        params = dict(zip(domains, values))
        samples.append({'params': params, 'expected': solve(**params)})
    return {'kp_id': key, 'kind': 'template', 'status': 'pending', 'arguments': {
        'statement': statement, 'answer_expr': expression,
        'answer_contract': contract,
        'params': {k: {'kind': 'choice', 'values': v} for k, v in domains.items()},
        'constraints': [], 'solution_sketch': sketch,
        'hints': ['Which coordinate or ratio determines the requested property?'],
        'samples': samples, 'distractors': []}}


def publish(exemplars, recipes):
    path = ROOT / 'curriculum/foundations/04-linear-graphs.yaml'
    source = path.read_text()
    for key, rows in exemplars.items():
        topic, kp = key.split('/')
        pattern = r'(^  - id: '+topic+r'\n.*?^      - id: '+kp+r'\n.*?)'
        pattern += r'^        exemplars:\n.*?(?=^        constraints:)'
        assert len(rows) == 4
        block = '        exemplars:\n'
        for row in rows:
            block += '          - problem: '+json.dumps(row['problem'])+'\n'
            for name in ('answer', 'answer_contract', 'solution_sketch'):
                block += '            '+name+': '+json.dumps(row[name])+'\n'
        source, count = re.subn(pattern, lambda m: m[1]+block, source,
                                flags=re.M | re.S)
        assert count == 1, key
    path.write_text(source)
    dest = OUT / 'templates.json'
    previous = json.loads(dest.read_text()) if dest.exists() else []
    merged = {r['kp_id']: r for r in previous}
    merged.update({r['kp_id']: r for r in recipes})
    dest.write_text(json.dumps(list(merged.values()), indent=2)+'\n')
