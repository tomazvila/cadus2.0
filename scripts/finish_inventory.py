#!/usr/bin/env python3
"""FINISH Wave 0 inventory — one row per KP.

Writes docs/FINISH-INVENTORY.json with, for each KP: course, topic, kp id,
declared contract (exemplar-level answer_contract kinds, else the topic's
answer_kind), exemplar count, approved template count, approved teach count,
tier (S3 of FINISH-CADUS.md). All later waves take their work lists from here.

Run: nix shell nixpkgs#python3Packages.pyyaml -c python3 scripts/finish_inventory.py
DB: docker exec cadus2-db psql -U cadus_admin -d cadus (read-only).
"""
import glob
import json
import subprocess
import sys
from pathlib import Path

import yaml

REPO = Path(__file__).resolve().parent.parent
OUT = REPO / "docs" / "FINISH-INVENTORY.json"

TIER1 = {"foundations", "geometry", "probability-statistics", "precalculus",
         "discrete-mathematics"}
# S4: verdict only if the answer has an AnswerContract variant with a
# deterministic check. Every AnswerContract variant in contract.rs is
# deterministic EXCEPT `None` (22 variants: Exact, RequiredAssignment, Approx,
# Tolerance, Unit, QuotientRemainder, Coordinates, Set, RequiredForm, List,
# InequalityUnion, RequiredInequalityNotation, RequiredSinglePower,
# RequiredNormalizedScientificNotation, RequiredSimplestRadical, ReducedRatio,
# AscendingChain, PolynomialRelation, RelationSetup, Label, Multipart, None).
# Undeclared contracts fall back to the runtime answer kinds that check.rs can
# decide: numeric and expression. multi-step and label-only are not decidable.
DECIDABLE_ANSWER_KINDS = {"numeric", "expression"}


def approved_counts():
    sql = ("SELECT kp_id, kind, count(*) FROM content_store "
           "WHERE status = 'approved' GROUP BY 1, 2")
    r = subprocess.run(["docker", "exec", "cadus2-db", "psql", "-U", "cadus_admin",
                        "-d", "cadus", "-tA", "-F", "\t", "-c", sql],
                       capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"psql failed: {r.stderr[:300]}")
    counts = {}
    for line in r.stdout.splitlines():
        kp_id, kind, n = line.split("\t")
        counts.setdefault(kp_id, {})[kind] = int(n)
    return counts


def contracts_of(kp):
    kinds = set()
    for ex in kp.get("exemplars") or []:
        c = (ex.get("answer_contract") or {}).get("kind")
        if c:
            kinds.add(c)
    return sorted(kinds)


def main():
    counts = approved_counts()
    rows = []
    for path in sorted(glob.glob(str(REPO / "curriculum" / "*" / "*.yaml"))):
        course = Path(path).parent.name
        if course == "courses":
            continue
        tier = 1 if course in TIER1 else 2
        doc = yaml.safe_load(open(path))
        for topic in doc.get("topics") or []:
            tid = topic["id"]
            akind = topic.get("answer_kind")
            for kp in topic.get("knowledge_points") or []:
                kp_id = f"{tid}/{kp['id']}"
                cc = counts.get(kp_id, {})
                contracts = contracts_of(kp)
                has_contract_decl = bool(contracts)
                verdict_kind = (has_contract_decl and any(k != "none" for k in contracts)) or \
                               (not has_contract_decl and akind in DECIDABLE_ANSWER_KINDS)
                rows.append({
                    "course": course,
                    "tier": tier,
                    "topic": tid,
                    "kp": kp["id"],
                    "answer_kind": akind,
                    "declared_contracts": contracts,
                    "verdict_capable": verdict_kind,
                    "exemplars": len(kp.get("exemplars") or []),
                    "templates_approved": cc.get("template", 0),
                    "teach_approved": cc.get("teach", 0),
                    "hint_ladders_approved": cc.get("hint_ladder", 0),
                    "diagnoses_approved": cc.get("diagnosis", 0),
                })
    OUT.write_text(json.dumps(rows, indent=1))
    by_course = {}
    for r in rows:
        c = by_course.setdefault(r["course"], {"kps": 0, "teach": 0, "verdict": 0,
                                               "templates": 0, "exemplars_lt6": 0})
        c["kps"] += 1
        c["teach"] += 1 if r["teach_approved"] else 0
        c["verdict"] += 1 if r["verdict_capable"] else 0
        c["templates"] += 1 if r["templates_approved"] else 0
        c["exemplars_lt6"] += 1 if r["exemplars"] < 6 else 0
    print(json.dumps(by_course, indent=1))
    print(f"\nwrote {len(rows)} KP rows to {OUT}")


if __name__ == "__main__":
    main()
