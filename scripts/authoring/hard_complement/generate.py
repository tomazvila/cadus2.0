"""Finite, material paired tasks inside the existing U08/U09 input bounds."""
import itertools
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
DEST = ROOT / 'docs/content-foundations/hard-complement/templates.json'


def recipe(key, statement, expression, sketch, hint, axes, predicate, expected):
    samples = [dict(params=dict(zip(axes, values)), expected=str(expected(*values)))
               for values in itertools.product(*axes.values()) if predicate(*values)]
    return dict(kp_id=key, kind='template', status='pending', arguments=dict(
        statement=statement, answer_expr=expression, solution_sketch=sketch,
        hints=[hint], params={k: dict(kind='choice', values=list(v)) for k, v in axes.items()},
        constraints=[dict(op='lt', left='a', right='b')], samples=samples, distractors=[]))


def logs():
    rows = []
    for kp, base, function, domain in [('kp1', '10', r'\log', range(-4, 7)),
                                       ('kp2', 'e', r'\ln', range(0, 6))]:
        statement = (f'Evaluate each logarithm, then add: ${function}({base}^{{{{{{a}}}}}})$ '
                     f'and ${function}({base}^{{{{{{b}}}}}})$. Give the sum as an integer.')
        rows.append(recipe('common-natural-logarithms/' + kp, statement, 'a+b',
                           f'The logarithm with base ${base}$ returns the exponent of that base. '
                           'The two values are ${a}$ and ${b}$, so add those exponents.',
                           'Which exponent produces each input from the logarithm base?',
                           dict(a=domain, b=domain), lambda a, b: a < b, lambda a, b: a+b))
    return rows


def main():
    rows = logs()
    DEST.parent.mkdir(parents=True, exist_ok=True)
    DEST.write_text(json.dumps(rows, indent=2) + '\n')
    print(f'{len(rows)} pending recipes, {sum(len(r["arguments"]["samples"]) for r in rows)} samples')


if __name__ == '__main__':
    main()
