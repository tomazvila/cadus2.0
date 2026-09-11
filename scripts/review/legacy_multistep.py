#!/usr/bin/env python3
"""Reproduce the statement-level review from the canonical curriculum JSON on stdin.

Labels encode a reviewed rubric and explicit topic/item decisions, not a keyword
heuristic or a claim that the legacy exemplars satisfy the new integrated schema.
The pinned input digest rejects changed content until its decisions are reviewed.
"""
import hashlib
import json
import sys

MODEL_TOPICS = {
    'angle-of-elevation-depression': 'A physical scene supplies quantities for a trigonometric model.',
    'compound-interest': 'An investment supplies principal, rate, frequency, and duration.',
    'consecutive-integer-problems': 'Verbal number constraints need a model and the requested numbers.',
    'continuous-growth-model': 'A growth or decay context supplies the model and an interpreted quantity.',
    'equation-word-problems': 'A shared situation links unknown quantities through an equation.',
    'exponential-growth-decay': 'A population, asset, or sample changes over stated periods.',
    'fraction-word-problems': 'One situation determines the fraction operation and its interpreted result.',
    'inequality-word-problems': 'A budget, capacity, or score constraint needs a feasible decision.',
    'integer-word-problems': 'Signed changes describe one account, game, journey, or temperature.',
    'interpreting-graphs-qualitatively': 'The graph describes movement, temperature, cost, or height.',
    'interpreting-linear-models': 'A contextual linear model gives a physical or monetary interpretation.',
    'linear-word-problems': 'One situation supplies or requires a linear model and a contextual result.',
    'money-geometry-problems': 'Money or geometric constraints require a model of one situation.',
    'percent-applications': 'A shared price or population undergoes stated percentage changes.',
    'percentages': 'A price or assessment supplies a contextual percentage interpretation.',
    'quadratic-applications': 'A geometric, numeric, or height model determines the requested quantity.',
    'ratio-tables-equivalent-ratios': 'A recipe, map, mixture, or class supplies quantities to compare.',
    'systems-mixture-problems': 'A single mixture links total amount and concentration or price.',
    'systems-money-problems': 'One purchase or coin collection links quantity and total value.',
    'systems-rate-problems': 'A single motion situation links rates, distance, and time.',
    'systems-word-problems': 'Verbal constraints describe the same numbers, geometry, or people.',
    'trig-applications': 'One physical scene links geometric or trigonometric quantities.',
    'unit-rates': 'A real quantity or purchase supplies units for a rate or comparison.',
    'work-rate-problems': 'One job, tank, or journey links rates and elapsed time.',
}
OVERRIDES = {}


def override(topic, kp, indices, category, reason):
    for index in indices:
        OVERRIDES[topic, kp, index] = (category, reason)


for topic, kp, indices in [
    ('continuous-growth-model', 'kp2', [2]),
    ('interpreting-graphs-qualitatively', 'kp3', [1]),
    ('percentages', 'kp1', [0, 1]), ('percentages', 'kp3', [0]),
    ('ratio-tables-equivalent-ratios', 'kp1', [1]),
]:
    override(topic, kp, indices, 'component_exercise', 'The statement asks an isolated mathematical property or calculation.')
for topic, kp, indices in [
    ('linear-word-problems', 'kp1', [1]),
    ('interpreting-linear-models', 'kp1', [1]),
    ('interpreting-linear-models', 'kp2', [0, 1]),
]:
    override(topic, kp, indices, 'context_fragment', 'The model or its contextual variable meanings rely on neighboring material; preserve and repair context before reuse.')
for kp, indices in [('kp3', [0, 1]), ('diagnostic', [0])]:
    override('law-of-sines-cosines', kp, indices, 'coherent_model', 'One ship journey or park supplies a physical triangle model.')
for kp, indices in [('kp1', [0, 1]), ('diagnostic', [0])]:
    override('understanding-ratios', kp, indices, 'coherent_model', 'One fruit, marble, or pupil collection defines the compared quantities.')
for topic, kp, indices in [
    ('applying-the-quadratic-formula', 'kp1', [1]),
    ('completing-the-square', 'kp1', [0]),
    ('converting-to-vertex-form', 'kp3', [0, 1]),
    ('domain-range', 'diagnostic', [0]),
    ('elimination-with-addition', 'kp3', [0]),
    ('graphing-linear-equations', 'kp1', [0, 1]),
    ('graphing-linear-inequalities', 'diagnostic', [0]),
    ('graphing-proportional-relationships', 'diagnostic', [0]),
    ('law-of-sines', 'kp3', [0]),
    ('law-of-sines-cosines', 'kp2', [0]),
    ('parabola-vertex-form', 'kp2', [0, 1]),
    ('parabola-vertex-form', 'kp3', [0]),
    ('parabola-vertex-form', 'diagnostic', [0]),
    ('point-slope-form', 'kp2', [0]),
    ('point-slope-form', 'diagnostic', [0]),
    ('pythagorean-converse', 'kp3', [0]),
    ('quadratic-graphs-vertex', 'kp2', [0, 1]),
    ('quadratic-graphs-vertex', 'kp3', [0, 1]),
    ('quadratic-graphs-vertex', 'diagnostic', [0]),
    ('radical-equations-basic', 'kp3', [1]),
    ('substitution-with-isolated-variable', 'kp3', [0]),
    ('trig-graphs-basic', 'kp1', [1]),
    ('trig-graphs-basic', 'kp3', [0, 1]),
    ('trig-graphs-basic', 'diagnostic', [0]),
    ('trig-graphs-midline', 'kp2', [0, 1]),
    ('trig-graphs-midline', 'kp3', [0, 1]),
    ('trig-graphs-midline', 'diagnostic', [0]),
]:
    override(topic, kp, indices, 'linked_outputs', 'Several requested outputs or a check refer to the same mathematical object; preserve that internal link.')


def encoding(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':'))


def statements(dump):
    for topic in dump['topics']:
        if topic['course'] != 'foundations' or topic['answer_kind'] != 'multi-step':
            continue
        for kp in topic['knowledge_points']:
            for index, item in enumerate(kp['exemplars']):
                yield dict(topic_id=topic['id'], kp_id=kp['id'], exemplar_index=index,
                           problem=item['problem'], answer=item['answer'])
        if topic['diagnostic_exemplar']:
            item = topic['diagnostic_exemplar']
            yield dict(topic_id=topic['id'], kp_id='diagnostic', exemplar_index=0,
                       problem=item['problem'], answer=item['answer'])


rows = sorted(statements(json.load(sys.stdin)), key=lambda r: (r['topic_id'], r['kp_id'], r['exemplar_index']))
assert len(rows) == 542
assert len({r['topic_id'] for r in rows}) == 78
source_hash = hashlib.sha256(encoding(rows).encode()).hexdigest()
EXPECTED_SOURCE = '432d59e238c00ffe8d83bac6e3e0e9277689c55f21818249577c96d6476ab00d'
assert source_hash == EXPECTED_SOURCE, ('Statement content changed; review before repinning', source_hash)
assert set(OVERRIDES) <= {(r['topic_id'], r['kp_id'], r['exemplar_index']) for r in rows}
for row in rows:
    default = ('coherent_model', MODEL_TOPICS[row['topic_id']]) if row['topic_id'] in MODEL_TOPICS else (
        'component_exercise', 'A standalone symbolic, numeric, geometric, or conceptual exercise; no shared application scenario is stated.')
    category, reason = OVERRIDES.get((row['topic_id'], row['kp_id'], row['exemplar_index']), default)
    print(encoding(row | {'classification': category, 'review_reason': reason}))
print(f'statements={len(rows)} topics=78 source_sha256={source_hash}', file=sys.stderr)
