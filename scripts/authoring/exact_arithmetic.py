"""Shared closed arithmetic for independent authoring oracles."""
import ast


def binary(operator,left,right):
    operations={ast.Add:lambda:left+right,ast.Sub:lambda:left-right,
                ast.Mult:lambda:left*right,ast.Div:lambda:left/right}
    try:
        return operations[type(operator)]()
    except KeyError as error:
        raise ValueError(f"unsupported binary operator: {type(operator).__name__}") from error
