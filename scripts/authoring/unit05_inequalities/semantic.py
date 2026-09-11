"""Independent rational reconstruction of inequality prompts, without answer_expr."""
import ast
from fractions import Fraction as Q
from pathlib import Path
import re
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from exact_arithmetic import binary

OPS = {'<': lambda a,b:a<b, '<=':lambda a,b:a<=b,
       '>':lambda a,b:a>b, '>=':lambda a,b:a>=b}
FLIP = {'<':'>','<=':'>=','>':'<','>=':'<='}


def scalar(text, x=0):
    text = text.replace('\\le', '<=').replace('\\ge', '>=')
    text = text.replace('\\cdot', '*').replace(' ', '')
    text = re.sub(r'(\d|\))(?=[xy(])', r'\1*', text)
    return value(ast.parse(text, mode='eval').body, Q(x))


def value(node, x):
    if isinstance(node, ast.Constant) and type(node.value) is int:
        return Q(node.value)
    if isinstance(node, ast.Name) and node.id in ('x','y','n','k'):
        return x
    if isinstance(node, ast.UnaryOp):
        a = value(node.operand, x)
        if isinstance(node.op, ast.USub): return -a
        if isinstance(node.op, ast.UAdd): return a
    if isinstance(node, ast.BinOp):
        a,b = value(node.left,x),value(node.right,x)
        return binary(node.op,a,b)
    raise ValueError(ast.dump(node))


def affine(text):
    c = scalar(text)
    m = scalar(text,1)-c
    assert all(scalar(text,x)==m*x+c for x in [-3,2,7]), text
    return m,c


def relation(text):
    text = text.replace('\\le','<=').replace('\\ge','>=')
    parts = re.split(r'(<=|>=|<|>)', text)
    assert len(parts)==3, text
    left,op,right = parts
    a,b = affine(left)
    c,d = affine(right)
    assert a!=c, 'Constant/cancelling inequality'
    return (op if a>c else FLIP[op]), (d-b)/(a-c)


def math(problem):
    return [s.replace(r'\le','<=').replace(r'\ge','>=').replace(r'\cdot','*')
            for s in re.findall(r'\$([^$]+)\$',problem)]


def reconstruct(problem):
    """Solve the displayed relation; budget prompts include their domain in prose."""
    equations = [s for s in math(problem) if re.search(r'<=|>=|<|>|\\le|\\ge',s)]
    assert len(equations)==1, problem
    op,bound = relation(equations[0])
    if 'maximum whole' in problem:
        assert op=='<=' and bound>=0
        check_budget(problem,equations[0],bound)
        return str(bound.numerator//bound.denominator)
    return f'x {op} {bound}'


def check_budget(problem,equation,bound):
    context=problem.split('With $x$')[0]
    price,fee,cap=map(Q,re.findall(r'(\d+) euros?',context))
    left,op,right=re.split(r'(<=|>=|<|>)',equation)
    assert affine(left)==(price,fee) and scalar(right)==cap
    whole=bound.numerator//bound.denominator
    assert price*whole+fee<=cap<price*(whole+1)+fee


def signature(problem):
    answer = reconstruct(problem)
    # Solutions are canonical mathematical families, regardless of surface wording.
    return ('budget' if 'maximum whole' in problem else 'ray', answer)


def sketch(problem):
    eq = next(s for s in math(problem) if re.search(r'<=|>=|<|>|\\le|\\ge',s))
    left,op,right = re.split(r'(<=|>=|<|>)', eq)
    a,b = affine(left); c,d = affine(right)
    action = 'reverse' if a<c else 'preserve'
    return (f'Collect variable terms and constants: $({a-c})x {op} {d-b}$. '
            f'Divide by ${a-c}$ and {action} the comparison. '
            f'The boundary is ${(d-b)/(a-c)}$. '
            + ('Take the greatest whole count at or below this bound; larger counts exceed the budget.'
               if 'maximum whole' in problem else 'Equality is included exactly when the comparison is inclusive.'))


def verify(problem, answer, solution=None):
    assert answer == reconstruct(problem), (problem,answer,reconstruct(problem))
    if solution is not None:
        assert len(solution)>70 and 'boundary' in solution
        for eq in math(solution):
            if re.search(r'<=|>=|<|>', eq):
                op,bound = relation(eq)
                source = next(s for s in math(problem) if re.search(r'<=|>=|<|>',s))
                assert (op,bound)==relation(source), (source,eq)
    return signature(problem)
