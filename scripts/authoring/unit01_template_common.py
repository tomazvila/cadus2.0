"""Finite, exact unit01 authoring recipes; every tuple has an authored sample."""
from fractions import Fraction as F
from itertools import product


def recipe(
    key, statement, expression, sketch, hint, domains, solve,
    accept=None, constraints=(), answer_contract=None,
):
    """Build real worker arguments; the worker independently evaluates solve's samples."""
    names = list(domains)
    samples = []
    for values in product(*domains.values()):
        bindings = dict(zip(names, values))
        if accept and not accept(**bindings):
            continue
        expected = solve(**{k: F(v) for k, v in bindings.items()})
        samples.append({'params': bindings, 'expected': str(expected)})
    if len(samples) < 12:
        raise ValueError(f'{key}: only {len(samples)} valid tuples')
    arguments = {
        'statement': statement, 'answer_expr': expression,
        'params': {k: {'kind': 'choice', 'values': list(v)} for k, v in domains.items()},
        'constraints': list(constraints), 'solution_sketch': sketch,
        'hints': [hint], 'distractors': [], 'samples': samples,
    }
    if answer_contract is not None:
        arguments['answer_contract'] = answer_contract
    return {'kp_id': key, 'kind': 'template', 'arguments': arguments}


def cmp(op, left, right):
    return {'op': op, 'left': left, 'right': right}


def plus(a, b):
    return {'op': 'add', 'args': [a, b]}
