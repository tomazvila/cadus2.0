"""Render comparison and multiplication symbols inside learner-facing math."""
import re


def latex(text):
    def convert(match):
        body=match[1].replace('<=',r'\le ').replace('>=',r'\ge ')
        body=body.replace('*',r'\cdot ')
        return '$'+body+'$'
    return re.sub(r'\$([^$]+)\$',convert,text)


def render_all(exemplars,recipes):
    for rows in exemplars.values():
        for row in rows:
            for key in ['problem','solution_sketch']: row[key]=latex(row[key])
    for row in recipes:
        args=row['arguments']
        for key in ['statement','solution_sketch']: args[key]=latex(args[key])
        args['hints']=list(map(latex,args['hints']))
