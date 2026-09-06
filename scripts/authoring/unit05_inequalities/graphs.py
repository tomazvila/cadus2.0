"""Solve and report a complete number-line graph using two typed fields."""
from itertools import product
from semantic import math, relation

CONTRACT={'kind':'multipart','parts':[
    {'name':'boundary','contract':{'kind':'exact'}},
    {'name':'graph','contract':{'kind':'label','options':[
        ['open left'],['closed left'],['open right'],['closed right']]}}]}
KEY='one-step-inequalities/kp3'


def problem(eq):
    return (f'Solve ${eq}$. Give boundary and graph, where graph combines '
            'open or closed with left or right (for example: boundary = 2; graph = open left).')


def reconstruct(prompt):
    op,bound=relation(math(prompt)[0])
    style=('closed' if '=' in op else 'open')+' '+('left' if '<' in op else 'right')
    return f'boundary = {bound}; graph = {style}'


def sketch(prompt):
    op,bound=relation(math(prompt)[0])
    return (f'Undo the single arithmetic operation to get $x {op} {bound}$. '
            'The boundary comes from equality. A strict sign gives an open point; an inclusive sign gives a closed point. '
            'Values below the boundary shade left; values above it shade right.')


def generate():
    authored=[dict(problem=p,answer=reconstruct(p),answer_contract=CONTRACT,solution_sketch=sketch(p))
              for p in map(problem,['2x>14','x+6<=10','x/3>=5','x-7<2'])]
    statement=problem('x+{a}<={b}')
    samples=[dict(params=dict(a=a,b=b,g='closed left'),expected=reconstruct(statement.format(a=a,b=b)))
             for a,b in product([3,6,9],[34,44,54,64])]
    recipe=dict(kp_id=KEY,kind='template',status='pending',arguments=dict(
        statement=statement,answer_expr='multipart(b-a,g)',answer_contract=CONTRACT,
        params={k:dict(kind='choice',values=v) for k,v in
                [('a',[3,6,9]),('b',[34,44,54,64]),('g',['closed left'])]},
        constraints=[],samples=samples,distractors=[],
        solution_sketch='Subtract {a}: $x <= {b}-{a}$. Equality includes the boundary and smaller values lie left, so the graph is {g}.',
        hints=['First find the boundary by undoing the arithmetic. Then read the comparison to choose the point style and ray direction.']))
    return {KEY:authored},[recipe]


def verify(prompt,answer,solution=None):
    assert answer==reconstruct(prompt),(prompt,answer,reconstruct(prompt))
    if solution is not None:
        op,bound=relation(math(prompt)[0])
        assert relation(math(solution)[0])==(op,bound)
        assert len(solution)>70 and 'boundary' in solution
    return ('graph',math(prompt)[0])
