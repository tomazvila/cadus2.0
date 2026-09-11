"""Five bounded U07 residual recipes; source-only, pending-only authoring."""
import itertools
import json
from math import isqrt
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[3]
UNIT = ROOT / 'curriculum/foundations/07-polynomials-quadratics.yaml'
OUT = ROOT / 'docs/content-foundations/unit07-complement'
KEYS = ['polynomial-basics/kp2', 'difference-of-squares/kp1',
        'choosing-factoring-strategy/kp1', 'parabola-vertex-form/kp2',
        'quadratic-graphs-vertex/kp3']
EXACT = {'kind': 'exact'}
POINT = {'kind': 'coordinates', 'arity': 2}


def label(*names):
    return {'kind': 'label', 'options': [[n] for n in names]}


def multipart(**parts):
    return {'kind': 'multipart', 'parts': [{'name': n, 'contract': c} for n, c in parts.items()]}


CLASS = label('monomial', 'binomial', 'trinomial')
STRATEGY = label('GCF', 'difference of squares', 'trinomial factoring')
DIRECTION = label('upward', 'downward')
FACTOR = multipart(lower_factor=EXACT, upper_factor=EXACT)
EXTREME = multipart(direction=DIRECTION, extreme_value=EXACT)
GRAPH = multipart(direction=DIRECTION, vertex=POINT, left_intercept=POINT,
                  right_intercept=POINT, y_intercept=POINT)
CONTRACTS = dict(zip(KEYS, [CLASS, FACTOR, STRATEGY, EXTREME, GRAPH]))


def exemplar(problem, answer, sketch):
    return dict(problem=problem, answer=answer, solution_sketch=sketch)


def authored():
    return {
        KEYS[0]: [
            exemplar('Classify $8x^3-11x$ as monomial, binomial, or trinomial.', 'binomial',
                     'The powers 3 and 1 differ, so the two nonzero terms remain separate: a binomial.'),
            exemplar('Combine like terms in $2x^2+7x+3x+6$ and classify the polynomial as monomial, binomial, or trinomial.', 'trinomial',
                     'The linear coefficients add to 10. The result has terms $2x^2$, $10x$, and $6$, so it is a trinomial.'),
            exemplar('An algebra-tile inventory has nine positive cubic pieces and no other pieces. Classify its polynomial as monomial, binomial, or trinomial.', 'monomial',
                     'All nine pieces belong to the same power group, giving the single term $9x^3$: a monomial.'),
            exemplar('A coefficient table in columns $(x^4,x^2,1)$ has row $(5,0,-8)$. Classify the polynomial as monomial, binomial, or trinomial.', 'binomial',
                     'The zero coefficient contributes no term. The two remaining terms $5x^4$ and $-8$ give a binomial.')],
        KEYS[1]: [
            exemplar('Factor $x^2-49$. Report lower_factor and upper_factor, the monic linear factors with smaller and larger constants.', 'lower_factor = x-7; upper_factor = x+7',
                     '$49=7^2$. The conjugates multiply to $x^2-49$ because their linear coefficients sum to zero.'),
            exemplar('A square of side $x$ loses a square of side $9$, with $x>9$. Express the remaining area as two monic linear factors, named lower_factor and upper_factor in increasing constant order.', 'lower_factor = x-9; upper_factor = x+9',
                     'Subtract areas to get $x^2-81$. Since $81=9^2$, the area is $(x-9)(x+9)$; both lengths are positive.'),
            exemplar('A coefficient table in columns $(x^2,x,1)$ has row $(1,0,-121)$. Factor its polynomial into lower_factor and upper_factor, monic linear factors in increasing constant order.', 'lower_factor = x-11; upper_factor = x+11',
                     'The table gives $x^2-121$. The constant is the negative square of 11, so the constants in the conjugate factors are -11 and 11.'),
            exemplar('A learner proposes $(x-13)^2$ for $x^2-169$. Correct the factorization as lower_factor and upper_factor, monic linear factors in increasing constant order.', 'lower_factor = x-13; upper_factor = x+13',
                     'The proposed square has an unwanted linear term. Opposite constants -13 and 13 give sum zero and product -169.')],
        KEYS[2]: [
            exemplar('Choose the first factoring move for $7x^2-63$: GCF, difference of squares, or trinomial factoring. Check a common factor first.', 'GCF',
                     'Both coefficients are divisible by 7. Extract $7(x^2-9)$ before examining the inner square difference.'),
            exemplar('A coefficient table in columns $(x^2,x,1)$ has row $(1,0,-64)$. Choose the first factoring move: GCF, difference of squares, or trinomial factoring.', 'difference of squares',
                     'The coefficients have GCF 1 and the linear coefficient is zero. The polynomial is $x^2-8^2$, so conjugate factors apply.'),
            exemplar('A learner chooses difference of squares for $x^2+9x+20$. Correct the first move: GCF, difference of squares, or trinomial factoring.', 'trinomial factoring',
                     'All three terms are nonzero and the coefficient GCF is 1. Constants 4 and 5 multiply to 20 and add to 9, so use trinomial factoring.'),
            exemplar('A rectangular area is $x^2-100$, with $x>10$. Choose the first move to obtain its linear dimensions: GCF, difference of squares, or trinomial factoring.', 'difference of squares',
                     'There is no nontrivial common factor. Subtracting the square of 10 from the square of x gives $(x-10)(x+10)$.')],
        KEYS[3]: [
            exemplar('For $y=-3(x-4)^2+8$, report direction (upward or downward) and extreme_value.', 'direction = downward; extreme_value = 8',
                     'The squared term is nonnegative and its coefficient is negative. Thus every output is at most 8, attained at $x=4$.'),
            exemplar('Translate $y=2x^2$ left 3 and down 7, giving $y=2(x+3)^2-7$. Report direction (upward or downward) and extreme_value.', 'direction = upward; extreme_value = -7',
                     'Translation preserves the positive leading coefficient. The square vanishes at $x=-3$, where the least output is -7.'),
            exemplar('A learner calls 5 a maximum for $y=4(x-2)^2+5$. Correct the summary using direction (upward or downward) and extreme_value.', 'direction = upward; extreme_value = 5',
                     'The added term is nonnegative, so outputs are at least 5. The graph opens upward and 5 is its minimum, attained at $x=2$.'),
            exemplar('A model is $y=11-2(x+6)^2$. Report direction (upward or downward) and extreme_value using the nonnegativity of a square.', 'direction = downward; extreme_value = 11',
                     'Subtracting twice a nonnegative square keeps the output at most 11. Equality occurs at $x=-6$, giving the maximum.')],
        KEYS[4]: graph_exemplars(),
    }


def graph_answer(a, left, right):
    h = (left + right) / 2
    k = a * (h-left) * (h-right)
    return (f'direction = {"upward" if a > 0 else "downward"}; vertex = ({h:g}, {k:g}); '
            f'left_intercept = ({left}, 0); right_intercept = ({right}, 0); '
            f'y_intercept = (0, {a*left*right})')


def graph_exemplars():
    request = (' Report direction (upward or downward), vertex, left_intercept, '
               'right_intercept, and y_intercept; use coordinate pairs for points.')
    cases = [
        ('Sketch $y=x^2-4x-12$.', 1, -2, 6,
         'Factor as $(x+2)(x-6)$: the zeros are -2 and 6. Their midpoint is 2; evaluation gives -16. The constant -12 is the vertical intercept ordinate.'),
        ('A parabolic arch has signed-height model $y=-x^2+10x$.', -1, 0, 10,
         'The negative coefficient opens downward. The zeros of $-x(x-10)$ are 0 and 10; their midpoint is 5, with height 25. At input zero the height is zero.'),
        ('A coefficient table in columns $(x^2,x,1)$ has row $(1,6,-7)$. Sketch its graph.', 1, -7, 1,
         'The polynomial factors as $(x+7)(x-1)$. Its zeros have midpoint -3; evaluation gives -16. The positive leading coefficient opens upward; at zero the output is -7.'),
        ('A learner draws $y=-x^2+2x+15$ opening upward. Correct the complete graph summary.', -1, -3, 5,
         'The negative leading coefficient opens downward. Factor as $-(x+3)(x-5)$ for zeros -3 and 5; midpoint 1 gives height 16. At zero the output is 15.')]
    return [exemplar(p+request, graph_answer(a,l,r), s) for p,a,l,r,s in cases]


def recipe(key, statement, expr, sketch, hint, axes, expected, constraints=None):
    samples = []
    for values in itertools.product(*axes.values()):
        p = dict(zip(axes, values))
        if key == KEYS[0] and not 1 <= sum(p[v] != 0 for v in 'abcd') <= 3:
            continue
        if key == KEYS[2] and p['b'] not in (0, p['c']-1):
            continue
        samples.append({'params': p, 'expected': expected(p)})
    return {'kp_id': key, 'kind': 'template', 'status': 'pending', 'arguments': {
        'statement': statement, 'answer_expr': expr, 'solution_sketch': sketch,
        'hints': [hint], 'params': {k: {'kind': 'choice', 'values': list(v)} for k,v in axes.items()},
        'constraints': constraints or [], 'samples': samples, 'distractors': []}}


def recipes():
    rows = []
    term_count = {'add': [{'mul': [{'lit':scale},name]} for scale,name in
                         [('1/2','a'),('1/3','b'),('1/5','c'),('-1/9','d')]]}
    rows.append(recipe(KEYS[0],
        'Classify ${a}x^4+({b})x^3+({c})x+({d})$ after omitting zero terms. Choose {f}, {g}, or {h}.',
        'signcase(a/2+b/3+c/5-d/9-2,[f,g,h])',
        'The four power groups have coefficients ${a}$, ${b}$, ${c}$, and ${d}$. Each nonzero coefficient contributes one term; a zero coefficient contributes none. Distinct powers cannot be combined.',
        'Count nonzero power groups, including the constant when it is nonzero.',
        dict(a=[0,2], b=[0,3], c=[0,5], d=[0,-9], f=['monomial'], g=['binomial'], h=['trinomial']),
        lambda p: ['monomial','binomial','trinomial'][sum(p[v] != 0 for v in 'abcd')-1],
        [{'op':'ge','left':term_count,'right':{'lit':1}},
         {'op':'le','left':term_count,'right':{'lit':3}}]))
    rows.append(recipe(KEYS[1],
        'Factor $x^2-{a}$. Report lower_factor and upper_factor, the monic linear factors in increasing constant order.',
        'multipart(x-sqrt(a),x+sqrt(a))',
        'The positive square root of ${a}$ gives the magnitude of both constants. Choose opposite signs: their sum is zero and their product is $-{a}$, reconstructing the original polynomial.',
        'The two monic factors are conjugates; their constant product must be negative.',
        dict(a=[v*v for v in range(17,29)]),
        lambda p: f'lower_factor = x-{isqrt(p["a"])}; upper_factor = x+{isqrt(p["a"])}'))
    rows.append(recipe(KEYS[2],
        'For ${g}x^2+({g}*{b})x-({g}*{c})$, choose the first factoring move: {f}, {s}, or {h}. Evaluate coefficient products and check a common factor first.',
        'signcase(g-1,[f,signcase(b,[s,s,h]),f])',
        'The coefficient products share ${g}$. If ${g}>1$, extract it first. Otherwise, when ${b}=0$ the two remaining terms are squares; when ${b}>0$, find constants with sum ${b}$ and product $-{c}$.',
        'Check the integer GCF, then count the nonzero terms before choosing a pattern.',
        dict(g=[1,2], b=[0,48,80,120], c=[49,81,121], f=['GCF'], s=['difference of squares'], h=['trinomial factoring']),
        lambda p: 'GCF' if p['g']>1 else ('difference of squares' if p['b']==0 else 'trinomial factoring'),
        [{'op':'eq','left':{'mul':['b',{'add':['b',{'mul':[{'lit':-1},'c']},{'lit':1}]}]},'right':{'lit':0}}]))
    rows.append(recipe(KEYS[3],
        'For $y={a}(x-2)^2+({k})$, report direction ({d} or {u}) and extreme_value.',
        'multipart(signcase(a,[d,u,u]),k)',
        'The square is zero at $x=2$ and nonnegative elsewhere. Multiplying by ${a}$ makes the vertex output ${k}$ a minimum for a positive coefficient or a maximum for a negative coefficient.',
        'Use the sign of the leading coefficient and the value when the square vanishes.',
        dict(a=[-2,3], k=[-8,-5,-2,7,10,12], d=['downward'], u=['upward']),
        lambda p: f'direction = {"upward" if p["a"]>0 else "downward"}; extreme_value = {p["k"]}'))
    rows.append(recipe(KEYS[4],
        'Sketch $y={a}x^2-({a}*({l}+{r}))x+({a}*{l}*{r})$. Report direction ({d} or {u}), vertex, left_intercept, right_intercept, and y_intercept; use points for all intercepts.',
        'multipart(signcase(a,[d,u,u]),((l+r)/2,-a*(r-l)^2/4),(l,0),(r,0),(0,a*l*r))',
        'The polynomial factors as ${a}(x-({l}))(x-({r}))$. The two zeros are ${l}$ and ${r}$; evaluate their midpoint for the vertex. The coefficient ${a}$ controls opening, and substituting zero gives ${a}*{l}*{r}$ for the vertical intercept ordinate.',
        'Find the zeros, average their inputs, then evaluate the midpoint and zero.',
        dict(a=[-1,1], l=[-8,-6], r=[4,8,12], d=['downward'], u=['upward']),
        lambda p: graph_answer(p['a'],p['l'],p['r'])))
    return rows


def patch_curriculum():
    text = UNIT.read_text()
    data = authored()
    root = yaml.compose(text)
    fields = lambda n: {k.value: v for k,v in n.value}
    edits = []
    for topic in fields(root)['topics'].value:
        t = fields(topic)
        for kp in t['knowledge_points'].value:
            k = fields(kp)
            key = t['id'].value + '/' + k['id'].value
            if key not in data:
                continue
            items = [{**item, 'answer_contract': CONTRACTS[key]} for item in data[key]]
            block = ''.join('          - ' + '\n            '.join(
                name + ': ' + json.dumps(value, ensure_ascii=False) for name,value in item.items())
                + '\n' for item in items)
            old = k['exemplars']
            start = text.rfind('\n', 0, old.start_mark.index) + 1
            end = text.rfind('\n', 0, old.end_mark.index) + 1
            edits.append((start, end, block))
    assert len(edits) == 5
    for start,end,block in sorted(edits, reverse=True):
        text = text[:start] + block + text[end:]
    text = text.replace('monic square minus a perfect-square constant up to 100;',
                        'monic square minus a perfect-square constant up to 784;')
    UNIT.write_text(text)


if __name__ == '__main__':
    patch_curriculum()
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT/'templates.json').write_text(json.dumps(recipes(), indent=2)+'\n')
