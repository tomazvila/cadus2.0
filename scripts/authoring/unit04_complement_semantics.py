"""Independent reconstruction from learner-visible statements using rational arithmetic.

This module never imports the author or evaluates a recipe answer_expr.
"""
import ast
from fractions import Fraction as Q
import re

from exact_arithmetic import binary


def scalar(source, variables=None):
    source = source.replace(' ', '')
    source = re.sub(r'(\d|\))(?=[xy(])',r'\1*',source)
    return evaluate(ast.parse(source,mode='eval').body,variables or {})


def evaluate(node, variables):
    if isinstance(node,ast.Constant) and type(node.value) is int:
        return Q(node.value)
    if isinstance(node,ast.Name):
        return Q(variables[node.id])
    if isinstance(node,ast.UnaryOp):
        value=evaluate(node.operand,variables)
        if isinstance(node.op,ast.USub):
            return -value
        if isinstance(node.op,ast.UAdd):
            return value
    if isinstance(node,ast.BinOp):
        a,b=evaluate(node.left,variables),evaluate(node.right,variables)
        return binary(node.op,a,b)
    raise ValueError(f'Unsupported scalar: {ast.dump(node)}')


def coordinate_tuples(math):
    found=[]
    for start,ch in enumerate(math):
        if ch!='(':
            continue
        depth=0
        for end in range(start,len(math)):
            depth+=(math[end]=='(')-(math[end]==')')
            if depth==0:
                try:
                    node=ast.parse(math[start:end+1],mode='eval').body
                    if isinstance(node,ast.Tuple) and len(node.elts)==2:
                        found.append(tuple(evaluate(n,{}) for n in node.elts))
                except (ValueError,SyntaxError,KeyError):
                    pass
                break
    return found


def coordinate_expressions(math):
    """Return coordinate-pair expressions, including pairs containing ``t``."""
    found=[]
    for start,ch in enumerate(math):
        if ch!='(':
            continue
        depth=0
        for end in range(start,len(math)):
            depth+=(math[end]=='(')-(math[end]==')')
            if depth==0:
                try:
                    node=ast.parse(math[start:end+1],mode='eval').body
                    if isinstance(node,ast.Tuple) and len(node.elts)==2:
                        found.append(tuple(ast.unparse(n) for n in node.elts))
                except SyntaxError:
                    pass
                break
    return found


def points(problem):
    return [point for math in re.findall(r'\$([^$]+)\$',problem)
            for point in coordinate_tuples(math)]


def equations(problem):
    return [x for x in re.findall(r'\$([^$]+)\$',problem)
            if re.match(r'^[xy]=',x)]


def coordinate_expression_pairs(problem):
    return [point for math in re.findall(r'\$([^$]+)\$',problem)
            for point in coordinate_expressions(math)]


def all_equations(problem):
    return [x for x in re.findall(r'\$([^$]+)\$',problem)
            if '=' in x and any(variable in x for variable in 'xy')]


def solve_equal(left,right,variable='t'):
    """Solve two affine scalar expressions for one variable."""
    left0,right0=scalar(left,{variable:0}),scalar(right,{variable:0})
    left1,right1=scalar(left,{variable:1}),scalar(right,{variable:1})
    left_rate,right_rate=left1-left0,right1-right0
    assert scalar(left,{variable:2})==left0+2*left_rate
    assert scalar(right,{variable:2})==right0+2*right_rate
    assert left_rate!=right_rate
    return (right0-left0)/(left_rate-right_rate)


def quadrant_code(point):
    x,y=point
    if x==0 and y==0:
        return Q(7)
    if y==0:
        return Q(5)
    if x==0:
        return Q(6)
    if x>0:
        return Q(1 if y>0 else 4)
    return Q(2 if y>0 else 3)


def slope(p,q):
    assert p[0]!=q[0], 'Vertical slope is undefined'
    return (q[1]-p[1])/(q[0]-p[0])


def coefficient(equation):
    right=equation.split('=')[1]
    intercept=scalar(right,{'x':0})
    rate=scalar(right,{'x':1})-intercept
    assert scalar(right,{'x':3})==3*rate+intercept
    return rate


def reconstruct(key, problem):
    topic=key.split('/')[0]
    ps=points(problem)
    if topic=='proportional-relationships':
        assert len(ps)==2
        r,s=(y/x for x,y in ps)
        return dict(ratios=(r,s),proportional=r==s)
    if topic=='graphing-proportional-relationships':
        a=ps[0][1]/ps[0][0]
        b=ps[1][1]/ps[1][0] if len(ps)==2 else coefficient(equations(problem)[0])
        return dict(rates=(a,b),faster=a>b)
    if key=='coordinate-plane/kp1':
        assert len(ps)==2
        return dict(A=quadrant_code(ps[0]),B=quadrant_code(ps[1]))
    if key in ('horizontal-vertical-slopes/kp1','horizontal-vertical-slopes/kp2'):
        pairs=coordinate_expression_pairs(problem)
        coordinate=1 if key.endswith('/kp1') else 0
        expressions=[pair[coordinate] for pair in pairs]
        axis='y' if coordinate==1 else 'x'
        axis_equations=[eq for eq in all_equations(problem) if eq.startswith(f'{axis}=')]
        if axis_equations:
            expressions.append(axis_equations[0].split('=',1)[1])
        assert len(expressions)==2
        return dict(value=solve_equal(*expressions))
    if key=='horizontal-vertical-slopes/kp3':
        assert len(ps)==2
        return dict(h=ps[0][1],v=ps[1][0])
    if topic=='slope-as-rate-of-change':
        assert len(ps)==2 and ps[1][0]>ps[0][0]
        rate=slope(*ps)
        return dict(rate=rate,increasing=rate>0)
    if topic=='graphing-linear-equations':
        eqs={eq[0]:scalar(eq.split('=')[1]) for eq in equations(problem)}
        assert set(eqs)=={'x','y'}
        return dict(x=eqs['x'],y=eqs['y'])
    if key=='solutions-of-two-variable-equations/kp1':
        assert len(ps)==1
        eqs=all_equations(problem)
        assert len(eqs)==1
        left,right=eqs[0].split('=',1)
        variables={'x':ps[0][0],'y':ps[0][1]}
        values=(scalar(left,variables),scalar(right,variables))
        return dict(values=values,valid=values[0]==values[1])
    if key=='graphing-from-a-table/kp2':
        assert len(ps)==2 and ps[0][0]!=ps[1][0]
        intercept=ps[0][1]-ps[0][0]*slope(*ps)
        return dict(value=(Q(0),intercept))
    if key=='slopes-of-parallel-perpendicular-lines/kp3':
        eqs=equations(problem)
        assert len(eqs)==2
        rates=tuple(coefficient(eq) for eq in eqs)
        relation=Q(1 if rates[0]==rates[1] else 2 if rates[0]*rates[1]==-1 else 3)
        return dict(rates=rates,relation=relation)
    raise ValueError(f'No independent reconstruction for {key}')


def answer_value(value):
    if value in ('yes','no'):
        return value=='yes'
    if value.startswith('('):
        return tuple(scalar(x) for x in value.strip('()').split(','))
    return scalar(value)


def parsed_answer(answer):
    if '=' not in answer:
        return {'value':answer_value(answer)}
    fields={}
    for part in answer.split(';'):
        name,value=(s.strip() for s in part.split('='))
        assert name not in fields
        fields[name]=answer_value(value)
    return fields


def verify(key, problem, answer, sketch=None):
    expected=reconstruct(key,problem)
    actual=parsed_answer(answer)
    assert actual==expected,(key,problem,expected,actual)
    if sketch is not None:
        assert len(sketch)>=80, 'A worked explanation is required'
        assert not any(x in sketch.lower() for x in ('appropriate rule','work through the steps','answer is'))
        for name,value in re.findall(r'\b([a-z]+)=(-?\d+(?:/\d+)?)',sketch):
            if name in expected:
                assert scalar(value)==expected[name], (name,value,expected)
    return expected


def signature(key,problem):
    result=reconstruct(key,problem)
    topic=key.split('/')[0]
    if key=='horizontal-vertical-slopes/kp3' or topic=='graphing-linear-equations':
        coords=(result['v'],result['h']) if 'h' in result else (result['x'],result['y'])
        return ('axis-lines',coords)
    return (key,tuple(result.items()))
