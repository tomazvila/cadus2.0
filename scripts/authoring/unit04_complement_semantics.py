"""Independent reconstruction from learner-visible statements using rational arithmetic.

This module never imports the author or evaluates a recipe answer_expr.
"""
import ast
from fractions import Fraction as Q
import re


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
        if isinstance(node.op,ast.Add):
            return a+b
        if isinstance(node.op,ast.Sub):
            return a-b
        if isinstance(node.op,ast.Mult):
            return a*b
        if isinstance(node.op,ast.Div):
            return a/b
    raise ValueError(f'Unsupported scalar: {ast.dump(node)}')


def points(problem):
    found=[]
    for math in re.findall(r'\$([^$]+)\$',problem):
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


def equations(problem):
    return [x for x in re.findall(r'\$([^$]+)\$',problem)
            if re.match(r'^[xy]=',x)]


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
    if topic=='horizontal-vertical-slopes':
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
    raise ValueError(f'No independent reconstruction for {key}')


def answer_value(value):
    if value in ('yes','no'):
        return value=='yes'
    if value.startswith('('):
        return tuple(scalar(x) for x in value.strip('()').split(','))
    return scalar(value)


def parsed_answer(answer):
    if '=' not in answer:
        return {'value':scalar(answer)}
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
    if topic in ('horizontal-vertical-slopes','graphing-linear-equations'):
        coords=(result['v'],result['h']) if 'h' in result else (result['x'],result['y'])
        return ('axis-lines',coords)
    return (topic,tuple(result.items()))
