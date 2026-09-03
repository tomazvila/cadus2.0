#!/usr/bin/env python3
"""Fail on every Rust function over a complexity limit.

Usage: rust_complexity.py <rca-json-dir> [--cyclomatic 22] [--cognitive 22] [--halstead 80]

The input directory holds the per-file JSON that `rust-code-analysis-cli -m -O json`
wrote. Each limit is exclusive: a function passes when its value is below the limit.
"""
import glob
import json
import os
import sys

LIMITS = {"cyclomatic": 22, "cognitive": 22, "halstead": 80}
KINDS = ("function", "closure")


def parse_args(argv):
    limits = dict(LIMITS)
    root = argv[0]
    rest = argv[1:]
    while rest:
        name = rest.pop(0).lstrip("-")
        limits[name] = float(rest.pop(0))
    return root, limits


def value_of(node, metric):
    metrics = node.get("metrics", {})
    if metric == "halstead":
        return metrics.get("halstead", {}).get("difficulty") or 0.0
    return metrics.get(metric, {}).get("sum") or 0.0


def walk(node, path, out):
    if node.get("kind") in KINDS:
        out.append((path, node.get("name"), node.get("start_line"), node))
    for space in node.get("spaces", []):
        walk(space, path, out)


def main() -> int:
    root, limits = parse_args(sys.argv[1:])
    functions = []
    for name in glob.glob(os.path.join(root, "**", "*.json"), recursive=True):
        with open(name) as handle:
            data = json.load(handle)
        walk(data, data.get("name", name), functions)
    failures = []
    for path, name, line, node in functions:
        for metric, limit in limits.items():
            value = value_of(node, metric)
            if value >= limit:
                failures.append((path, line, name, metric, value, limit))
    for path, line, name, metric, value, limit in sorted(failures):
        print(f"{path}:{line} {name} {metric}={value:.0f} (limit {limit:.0f})")
    print(f"rust complexity: {len(functions)} functions, {len(failures)} over a limit")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
