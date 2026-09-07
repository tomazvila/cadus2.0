#!/usr/bin/env python3
"""Keep legacy recipe catalogs aligned with the reviewed production-gate cohort.

Run after older unit generators. --check is a pure local drift check: it starts
no worker, database, network endpoint, or model. The production Rust gate remains
responsible for mathematical validation.
"""
import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
COHORT = ROOT / "docs/content-foundations/template19-production-gate"


def render(template, params):
    """Mirror the Rust statement renderer for the JSON scalar domains used here."""
    rendered = {
        name: f"({value})" if isinstance(value, int) and value < 0 else str(value)
        for name, value in params.items()
    }
    return template.format(**rendered)


def synchronize(check=False):
    rows = json.loads((COHORT / "drafts.json").read_text())
    mappings = json.loads((COHORT / "sources.json").read_text())
    expected = {row["kp_id"]: row["arguments"] for row in rows}
    if len(rows) != 19 or len(expected) != 19:
        raise ValueError("the production-gate cohort requires exactly 19 unique keys")
    changes = {}
    for mapping in mappings:
        key = mapping["kp_id"]
        for source in mapping["sources"]:
            path = ROOT / source
            document = changes.setdefault(path, json.loads(path.read_text()))
            matching = [row for row in document if row.get("kp_id") == key
                        and row.get("kind") == "template"]
            if len(matching) != 1:
                raise ValueError(f"{source}: expected one template for {key}")
            matching[0]["arguments"] = expected[key]
    drift = []
    for path, document in changes.items():
        if json.loads(path.read_text()) != document:
            drift.append(str(path.relative_to(ROOT)))
            if not check:
                path.write_text(json.dumps(document, indent=2, ensure_ascii=False) + "\n")
    worked = []
    for row in rows:
        arguments = row["arguments"]
        for sample in arguments["samples"]:
            params = sample["params"]
            worked.append({"kp_id": row["kp_id"], "params": params,
                           "problem": render(arguments["statement"], params),
                           "answer": sample["expected"],
                           "worked_solution": render(arguments["solution_sketch"], params)})
    path = COHORT / "worked-samples.json"
    if not path.exists() or json.loads(path.read_text()) != worked:
        drift.append(str(path.relative_to(ROOT)))
        if not check:
            path.write_text("[\n" + ",\n".join(json.dumps(item, sort_keys=True,
                            separators=(",", ":"), ensure_ascii=False) for item in worked)
                            + "\n]\n")
    return drift


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    changed = synchronize(args.check)
    for name in changed:
        print(("drift: " if args.check else "updated: ") + name)
    raise SystemExit(1 if args.check and changed else 0)
