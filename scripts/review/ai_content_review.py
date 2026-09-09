#!/usr/bin/env python3
"""Prepare and apply fail-closed AI evidence for C6 content review.

This tool never calls a model. An independent reviewer supplies evidence; only
validated approve/reject rows are sent through content_review_packet's API path.
"""
import argparse
import hashlib
import json
import sys
from pathlib import Path

import content_review_packet as packet

VERSION = 1
CHECKS = {"independent_math_recomputation", "objective_and_constraints",
          "explanations", "hints_and_leakage"}
DECISIONS = {"approve", "reject", "quarantine"}


def tree_hash(root):
    if not root.is_dir():
        raise packet.Refused("curriculum root is unavailable")
    rows = []
    for path in sorted(item for item in root.rglob("*") if item.is_file()):
        rows.append([str(path.relative_to(root)), hashlib.sha256(path.read_bytes()).hexdigest()])
    return packet.sha256(rows)


def listed(api, **filters):
    rows, page, limit = [], 0, None
    while True:
        query = "&".join(f"{key}={value}" for key, value in sorted(filters.items()))
        answer = api.request("GET", f"?{query}&page={page}")
        items, current = answer.get("items"), answer.get("limit")
        if not isinstance(items, list) or type(current) is not int or current < 1:
            raise packet.Refused("invalid selected-serving-set response")
        if limit is not None and current != limit:
            raise packet.Refused("selected-serving-set page size changed")
        rows.extend(items); limit = current
        if len(items) < limit:
            return rows
        page += 1


def serving_hash(api, item):
    rows = listed(api, kp=item["kp_id"], kind="template")
    docs = [api.document(row["digest"]) for row in rows if row.get("status") == "approved"]
    facts = [{"digest": doc["digest"], "fingerprint_sha256": packet.fingerprint(doc)} for doc in docs]
    if item["kind"] == "template":
        facts.append({"digest": item["digest"], "fingerprint_sha256": item["fingerprint_sha256"]})
    return packet.sha256(sorted(facts, key=lambda row: row["digest"]))


def context(packet_doc, api, curriculum):
    indexed = packet.validate_packet(packet_doc)
    kinds = {item["kind"] for item in indexed.values()}
    if "template" in kinds and len(kinds) != 1:
        raise packet.Refused("template reviews must be a separate batch")
    if "template" in kinds and len({item["kp_id"] for item in indexed.values()}) != len(indexed):
        raise packet.Refused("only one template per knowledge point is allowed per batch")
    if kinds - {"template", "teach", "hint_ladder", "diagnosis"}:
        raise packet.Refused("unsupported content kind")
    if "template" not in kinds:
        for item in indexed.values():
            if not any(row.get("status") == "approved" for row in listed(api, kp=item["kp_id"], kind="template")):
                raise packet.Refused("instruction review requires a refreshed approved template serving set")
    curriculum_hash = tree_hash(curriculum)
    rows = []
    for item in indexed.values():
        live = api.document(item["digest"])
        if live.get("status") != "pending" or packet.fingerprint(live) != item["fingerprint_sha256"]:
            raise packet.Refused(f"{item['digest']}: packet is stale")
        rows.append({"digest": item["digest"], "fingerprint_sha256": item["fingerprint_sha256"],
                     "body_sha256": packet.sha256(item["body"]), "curriculum_sha256": curriculum_hash,
                     "selected_serving_set_sha256": serving_hash(api, item)})
    return sorted(rows, key=lambda row: row["digest"])


def prepare(args):
    doc = packet.load_json(args.packet)
    rows = context(doc, packet.Api(args.base_url, args.cookie_file.read_text()), args.curriculum_root)
    core = {"ai_review_version": VERSION, "packet_sha256": doc["packet_sha256"], "items": rows}
    args.output.write_text(json.dumps(core | {"context_sha256": packet.sha256(core)}, indent=2, sort_keys=True) + "\n")


def review_rows(batch, expected):
    if batch.get("ai_review_version") != VERSION or not isinstance(batch.get("reviewer"), dict):
        raise packet.Refused("AI reviewer identity, model, and policy version are required")
    reviewer = batch["reviewer"]
    if set(reviewer) != {"identity", "model", "policy_version"} or not all(isinstance(value, str) and value for value in reviewer.values()):
        raise packet.Refused("AI reviewer identity, model, and policy version are required")
    rows = batch.get("decisions")
    if not isinstance(rows, list) or len(rows) != len(expected): raise packet.Refused("one AI decision per packet item is required")
    result, seen = [], set()
    for row in rows:
        digest = row.get("digest") if isinstance(row, dict) else None
        if digest in seen or digest not in expected: raise packet.Refused("duplicate or unsupported AI evidence")
        seen.add(digest); verdict, reason, checks = row.get("decision"), row.get("reason"), row.get("checks")
        if verdict not in DECISIONS or not isinstance(reason, str) or not reason.strip() or not isinstance(checks, dict) or set(checks) != CHECKS:
            raise packet.Refused(f"{digest}: missing decision, reason, or substantive checks")
        statuses = []
        for name in CHECKS:
            check = checks[name]
            if not isinstance(check, dict) or set(check) != {"status", "evidence"} or check["status"] not in {"pass", "fail", "unsupported"} or not isinstance(check["evidence"], str) or not check["evidence"].strip():
                raise packet.Refused(f"{digest}: {name} evidence is invalid")
            statuses.append(check["status"])
        if verdict == "approve" and set(statuses) != {"pass"}: raise packet.Refused(f"{digest}: approval has unresolved evidence")
        result.append((digest, verdict, reason))
    return result


def apply(args):
    source, batch = packet.load_json(args.packet), packet.load_json(args.review)
    if batch.get("packet_sha256") != source.get("packet_sha256"): raise packet.Refused("review names a different packet")
    core = {key: batch[key] for key in ("ai_review_version", "packet_sha256", "items") if key in batch}
    if batch.get("context_sha256") != packet.sha256(core): raise packet.Refused("AI review context fingerprint mismatch")
    api = packet.Api(args.base_url, args.cookie_file.read_text())
    actual = context(source, api, args.curriculum_root)
    if actual != batch["items"]: raise packet.Refused("AI review evidence is stale against live curriculum or serving set")
    decisions = review_rows(batch, {row["digest"]: row for row in actual})
    by_digest = packet.validate_packet(source)
    for digest, verdict, _reason in decisions:
        if verdict == "approve" and by_digest[digest]["kind"] == "template":
            gate = api.document(digest).get("gate")
            if not isinstance(gate, dict) or gate.get("gated") is not True or gate.get("rejected") or type(gate.get("instances_checked")) is not int or gate["instances_checked"] < 1:
                raise packet.Refused(f"{digest}: template gate does not support approval")
    out = {"decision_version": 1, "packet_sha256": source["packet_sha256"], "decisions": []}
    for digest, verdict, reason in decisions:
        if verdict != "quarantine": out["decisions"].append({"digest": digest, "decision": verdict} if verdict == "approve" else {"digest": digest, "decision": verdict, "reason": reason})
    if not out["decisions"]:
        packet.write_receipt(args.receipt, {"committed": False, "complete": True, "packet_sha256": source["packet_sha256"], "ai_reviewer": batch["reviewer"], "review_sha256": hashlib.sha256(args.review.read_bytes()).hexdigest(), "quarantined": [d for d, _v, _r in decisions]})
        return
    args.decisions.write_text(json.dumps(out, indent=2, sort_keys=True) + "\n")
    def before_write(current):
        one = {"packet_version": source["packet_version"], "scope": source["scope"], "items": [current]}
        one["packet_sha256"] = packet.sha256(one)
        expected = next(row for row in batch["items"] if row["digest"] == current["digest"])
        if context(one, api, args.curriculum_root) != [expected]:
            raise packet.Refused("AI review context changed before write")
    metadata = {"ai_reviewer": batch["reviewer"], "review_sha256": hashlib.sha256(args.review.read_bytes()).hexdigest()}
    receipt = packet.apply_decisions(api, args.packet, args.decisions, args.commit, args.receipt, before_write, metadata)
    receipt["ai_reviewer"] = batch["reviewer"]; receipt["quarantined"] = [d for d, v, _ in decisions if v == "quarantine"]
    packet.write_receipt(args.receipt, receipt)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__); subs = parser.add_subparsers(dest="command", required=True)
    for name in ("prepare", "apply"):
        cmd = subs.add_parser(name); cmd.add_argument("--base-url", required=True); cmd.add_argument("--cookie-file", required=True, type=Path); cmd.add_argument("--packet", required=True, type=Path); cmd.add_argument("--curriculum-root", required=True, type=Path)
        if name == "prepare": cmd.add_argument("--output", required=True, type=Path)
        else: cmd.add_argument("--review", required=True, type=Path); cmd.add_argument("--decisions", required=True, type=Path); cmd.add_argument("--receipt", required=True, type=Path); cmd.add_argument("--commit", action="store_true")
    args = parser.parse_args(argv)
    try: prepare(args) if args.command == "prepare" else apply(args)
    except (packet.Refused, OSError) as error: print(f"AI content review refused: {error}", file=sys.stderr); return 2
    return 0


if __name__ == "__main__": raise SystemExit(main())
