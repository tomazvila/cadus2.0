"""Independent exact reconstruction from rendered equation/half-plane prompts."""
from fractions import Fraction as Q
import re

from test_unit05_residual import linear, scalar
from test_unit05_residual_second import canonical_rows


def point(text):
    values = tuple(scalar(v,{}) for v in text.strip('() ').split(','))
    assert len(values) == 2
    return values


def parse(problem):
    relations, points = [], []
    for chunk in re.findall(r'\$([^$]+)\$',problem):
        matched = re.search(r'<=|>=|<|>|=',chunk)
        if matched:
            left,right = chunk[:matched.start()],chunk[matched.end():]
            relations.append((linear(left+'='+right),matched[0]))
        elif chunk.startswith('(') and not re.fullmatch(r'\(\s*x\s*,\s*y\s*\)',chunk):
            points.append(point(chunk))
    assert len(relations) == 2, problem
    return relations,points


def fields(answer):
    result = {}
    for part in answer.split(';'):
        name,value = (s.strip() for s in part.split('=',1))
        assert name not in result
        result[name] = value
    return result


def residual(row, p):
    a,b,c = row
    x,y = p
    return a*x+b*y-c


def holds(relation,p):
    row,op = relation
    value = residual(row,p)
    return {'=': value == 0,'<': value < 0,'<=':value <= 0,
            '>':value > 0,'>=':value >= 0}[op]


def classify(rows):
    (a,b,c),(d,e,f) = rows
    assert (a or b) and (d or e), 'Degenerate equation excluded'
    if a*e-b*d:
        return 'one'
    return 'infinite' if a*f == c*d and b*f == c*e else 'none'


def halfplane(relation):
    (a,b,c),op = relation
    assert op in ('<','<=','>','>=')
    if op.startswith('>'):
        a,b,c = -a,-b,-c
    scale = abs(next(v for v in (a,b) if v))
    return a/scale,b/scale,c/scale,op in ('<','>')


def reconstructed_region(answer):
    values = fields(answer)
    assert set(values) == {'solid','dashed'}
    planes = []
    for style in ('solid','dashed'):
        m,c,direction = (scalar(v,{}) for v in values[style].strip('() ').split(','))
        assert direction in (Q(-1),Q(1))
        row = (m,Q(-1),-c) if direction == 1 else (-m,Q(1),c)
        planes.append(halfplane((row,'<' if style == 'dashed' else '<=')))
    return tuple(sorted(planes))


def region_signature(relations):
    assert sum(op in ('<','>') for _,op in relations) == 1
    assert all(row[1] for row,_ in relations), 'Nonvertical boundary scope'
    return tuple(sorted(halfplane(r) for r in relations))


def region_checks(relations,answer):
    expected = region_signature(relations)
    supplied = reconstructed_region(answer)
    assert supplied == expected
    # Independently compare exact membership on both boundaries, either side,
    # at the crossing, and outside the overlap. No floating-point geometry.
    (a,b,c,_),(d,e,f,_) = expected
    determinant = a*e-b*d
    assert determinant, 'This family uses two nonparallel boundaries'
    cross_x = (c*e-b*f)/determinant
    x_values = {Q(-20),Q(0),Q(20),cross_x-1,cross_x,cross_x+1}
    decisions = set()
    for x in x_values:
        for aa,bb,cc,_ in expected:
            for delta in (Q(-1),Q(0),Q(1)):
                y = (cc-aa*x)/bb+delta
                direct = all(holds(r,(x,y)) for r in relations)
                record = all((pa*x+pb*y < pc if strict else pa*x+pb*y <= pc)
                             for pa,pb,pc,strict in supplied)
                assert direct == record
                decisions.add(direct)
    assert decisions == {False,True}
    return expected


def special_case_result(key,relations,rows,answer):
    assert all(op == '=' for _,op in relations)
    kind = classify(rows)
    names = {'infinite','none'} if key.endswith('kp1') else {'one','infinite'}
    assert kind in names or (kind == 'none' and len(names) == 2)
    supplied = fields(answer)
    assert set(supplied) == names
    assert supplied == {n:'yes' if n == kind else 'no' for n in names}
    return ('equations',canonical_rows(rows)),kind


def region_result(relations,answer):
    return ('region',region_checks(relations,answer))


def selection_result(key,relations,points,answer):
    valid = [p for p in points if all(holds(r,p) for r in relations)]
    assert len(valid) == 1
    assert point(answer) == valid[0]
    if key.startswith('systems-of-linear-inequalities/'):
        assert all(residual(r,p) for r,_ in relations for p in points)
    signature = ('selection',canonical_relations(relations),tuple(sorted(points)))
    return signature,point(answer)


def check_result(key,relations,rows,points,answer):
    supplied = fields(answer)
    assert set(supplied) == {'residuals','solution'}
    expected = [residual(r,points[0]) for r in rows]
    assert point(supplied['residuals']) == tuple(expected)
    valid = all(holds(r,points[0]) for r in relations)
    assert supplied['solution'] == ('yes' if valid else 'no')
    if key == 'checking-systems-solutions/kp2':
        assert sum(value == 0 for value in expected) == 1
    signature = ('check',canonical_relations(relations),points[0])
    return signature,(*expected,valid)


def check_sketch(sketch,output):
    if sketch is None:
        return
    assert len(sketch) >= 90 and 'must be corrected' not in sketch
    if 'first left-minus-right difference is' in sketch:
        pattern=r'first left-minus-right difference is ([^;]+); the second is ([^.]+)'
        matches = re.search(pattern,sketch)
        assert matches
        assert tuple(Q(value) for value in matches.groups()) == output[:2]


def verify(key,problem,answer,sketch=None):
    relations,points = parse(problem)
    rows = [r for r,_ in relations]
    if key.startswith('systems-special-cases/'):
        assert not points
        signature,output = special_case_result(key,relations,rows,answer)
    elif key == 'systems-of-linear-inequalities/kp2':
        assert not points
        signature = region_result(relations,answer)
        output = signature
    elif len(points) == 2:
        signature,output = selection_result(key,relations,points,answer)
    else:
        assert len(points) == 1
        signature,output = check_result(key,relations,rows,points,answer)
    check_sketch(sketch,output)
    return signature,output


def canonical_relations(relations):
    if all(op == '=' for _,op in relations):
        return ('equations',canonical_rows([r for r,_ in relations]))
    assert all(op != '=' for _,op in relations)
    return ('inequalities',tuple(sorted(halfplane(r) for r in relations)))


def external_signature(problem):
    relations,points = parse(problem)
    canonical = canonical_relations(relations)
    if len(points) == 1:
        return 'check',canonical,points[0]
    if len(points) == 2:
        return 'selection',canonical,tuple(sorted(points))
    if not points and canonical[0] == 'equations':
        return canonical
    if not points:
        return 'region',region_signature(relations)
    raise AssertionError('Unrecognized task signature')


def wrong_answers(answer):
    if ';' not in answer:
        x,y = point(answer)
        return [f'({x+1},{y})',f'({x},{y+1})']
    values = fields(answer)
    result = []
    for name,value in values.items():
        if value in ('yes','no'):
            replacements = ['no' if value == 'yes' else 'yes']
        elif value.startswith('('):
            nums = [scalar(v,{}) for v in value.strip('() ').split(',')]
            replacements = []
            for i in range(len(nums)):
                changed = nums.copy()
                changed[i] += 1
                replacements.append('('+','.join(map(str,changed))+')')
        else:
            replacements = [str(Q(value)+1)]
        for replacement in replacements:
            changed = dict(values,**{name:replacement})
            result.append('; '.join(f'{k} = {v}' for k,v in changed.items()))
    return result


def perturbed_problem(key,problem,answer):
    chunks = re.findall(r'\$([^$]+)\$',problem)
    equations = [c for c in chunks if re.search(r'<=|>=|<|>|=',c)]
    if key.startswith('systems-special-cases/'):
        kind = classify([r for r,_ in parse(problem)[0]])
        change = equations[0]
        if kind == 'infinite':
            left,right = change.split('=')
            change = f'{left}=({right})+1'
        return problem.replace('$'+equations[1]+'$','$'+change+'$',1)
    if key == 'systems-of-linear-inequalities/kp2':
        eq = equations[0]
        match = re.search(r'<=|>=|<|>',eq)
        changed = eq[:match.end()]+'('+eq[match.end():]+')+1'
        return problem.replace('$'+eq+'$','$'+changed+'$',1)
    if key == 'systems-of-linear-inequalities/kp3':
        eq = equations[0]
        swapped = eq.replace('>','<') if '>' in eq else eq.replace('<','>')
        return problem.replace('$'+eq+'$','$'+swapped+'$',1)
    candidate = next(c for c in chunks if c.startswith('('))
    if len(parse(problem)[1]) == 2:
        candidate = next(c for c in chunks if c.startswith('(') and point(c) == point(answer))
    x,y = point(candidate)
    return problem.replace('$'+candidate+'$',f'$({x+1},{y})$',1)
