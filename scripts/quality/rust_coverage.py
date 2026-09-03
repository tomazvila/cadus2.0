#!/usr/bin/env python3
"""Fail when Rust coverage is below 100% or a function has a CRAP score of 25 or more.

Usage: rust_coverage.py <llvm-cov-json> <rca-json-dir> [--crap 25]

CRAP(f) = cc(f)^2 * (1 - cov(f))^3 + cc(f), where cc is the cyclomatic complexity from
`rust-code-analysis` and cov is the covered share of the regions of f from `cargo llvm-cov`.
"""
import glob
import json
import os
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def load_complexity(root):
    by_key = {}

    def walk(node, path):
        if node.get("kind") in ("function", "closure"):
            key = (path, node.get("start_line"))
            cc = node.get("metrics", {}).get("cyclomatic", {}).get("sum") or 1.0
            by_key[key] = max(by_key.get(key, 0.0), cc)
        for space in node.get("spaces", []):
            walk(space, path)

    for name in glob.glob(os.path.join(root, "**", "*.json"), recursive=True):
        with open(name) as handle:
            data = json.load(handle)
        walk(data, os.path.relpath(data.get("name", name), REPO))
    return by_key


def uncovered_files(export):
    out = []
    for item in export["files"]:
        summary = item["summary"]
        short = os.path.relpath(item["filename"], REPO)
        for kind in ("lines", "functions", "regions"):
            if summary[kind]["count"] and summary[kind]["covered"] < summary[kind]["count"]:
                out.append((short, kind, summary[kind]["covered"], summary[kind]["count"]))
    return out


def crap_scores(export, complexity, limit):
    out = []
    for func in export["functions"]:
        regions = func["regions"]
        if not regions or not func["filenames"]:
            continue
        path = os.path.relpath(func["filenames"][0], REPO)
        start = min(region[0] for region in regions)
        cc = complexity.get((path, start), 1.0)
        covered = sum(1 for region in regions if region[4] > 0)
        cov = covered / len(regions)
        crap = cc * cc * (1 - cov) ** 3 + cc
        if crap >= limit:
            out.append((path, start, func["name"][:60], cc, cov, crap))
    return out


def main() -> int:
    cov_path, rca_dir = sys.argv[1], sys.argv[2]
    limit = float(sys.argv[4]) if len(sys.argv) > 4 else 25.0
    with open(cov_path) as handle:
        export = json.load(handle)["data"][0]
    complexity = load_complexity(rca_dir)
    gaps = uncovered_files(export)
    for short, kind, covered, count in sorted(gaps):
        print(f"{short}: {kind} {covered}/{count}")
    crap = crap_scores(export, complexity, limit)
    for path, start, name, cc, cov, score in sorted(crap):
        print(f"{path}:{start} {name} cc={cc:.0f} cov={cov:.2f} crap={score:.1f}")
    totals = export["totals"]
    print(
        "rust coverage: lines {:.2f}% functions {:.2f}% regions {:.2f}%; "
        "{} files below 100%; {} functions with CRAP >= {:.0f}".format(
            totals["lines"]["percent"],
            totals["functions"]["percent"],
            totals["regions"]["percent"],
            len({gap[0] for gap in gaps}),
            len(crap),
            limit,
        )
    )
    return 1 if gaps or crap else 0


if __name__ == "__main__":
    sys.exit(main())
