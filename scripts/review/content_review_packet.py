#!/usr/bin/env python3
"""Export and apply an auditable, digest-bound content review packet.

The tool uses the existing admin API; it adds no approval path. ``export`` reads
every pending page and each full review document. ``apply`` accepts only decisions
that name a digest in that packet, re-reads the live document, verifies its
fingerprint, and writes only decisions explicitly present in the decision file.
Apply is a dry run unless ``--commit`` is present.
"""
import argparse
import hashlib
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

PACKET_VERSION = 1
DECISION_VERSION = 1
KINDS = {"template", "teach", "hint_ladder", "diagnosis"}
DECISIONS = {"approve", "reject"}


class Refused(ValueError):
    """A fail-closed packet, decision, or live-state refusal."""


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def sha256(value):
    return hashlib.sha256(canonical(value).encode()).hexdigest()


def unique_object(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise Refused(f"duplicate JSON key: {key}")
        value[key] = item
    return value


def load_json(path):
    try:
        return json.loads(path.read_text(), object_pairs_hook=unique_object,
                          parse_constant=lambda value: (_ for _ in ()).throw(
                              Refused(f"invalid JSON constant: {value}")))
    except (OSError, json.JSONDecodeError) as error:
        raise Refused(str(error)) from error


class Api:
    def __init__(self, base_url, cookie):
        self.base = base_url.rstrip("/") + "/api/admin/content"
        self.cookie = cookie.strip()
        if not self.cookie:
            raise Refused("the cookie file is empty")

    def request(self, method, suffix="", body=None):
        headers = {"Accept": "application/json", "Cookie": self.cookie}
        data = None
        if body is not None:
            headers["Content-Type"] = "application/json"
            data = canonical(body).encode()
        request = urllib.request.Request(self.base + suffix, data=data,
                                         headers=headers, method=method)
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                return json.load(response, object_pairs_hook=unique_object)
        except (urllib.error.URLError, json.JSONDecodeError) as error:
            raise Refused(f"{method} {suffix or '/'} failed: {error}") from error

    def list_pending(self):
        rows, page, limit = [], 0, None
        while True:
            answer = self.request("GET", f"?status=pending&page={page}")
            items = answer.get("items")
            current_limit = answer.get("limit")
            if not isinstance(items, list) or type(current_limit) is not int or current_limit < 1:
                raise Refused("the review list response has no valid items/limit")
            if limit is not None and current_limit != limit:
                raise Refused("the review page limit changed during export")
            limit = current_limit
            rows.extend(items)
            if len(items) < limit:
                break
            page += 1
        digests = [row.get("digest") for row in rows if isinstance(row, dict)]
        if len(digests) != len(rows) or len(set(digests)) != len(digests):
            raise Refused("the pending queue contains a missing or duplicate digest")
        return rows

    def document(self, digest):
        return self.request("GET", "/" + urllib.parse.quote(digest, safe=""))

    def decide(self, digest, decision, reason=None):
        suffix = "/" + urllib.parse.quote(digest, safe="") + "/" + decision
        return self.request("POST", suffix, {} if decision == "approve" else {"reason": reason})


def source_index(source_root):
    """Index exact draft bodies by serving key without claiming import lineage."""
    index = {}
    if source_root is None or not source_root.exists():
        return index
    for manifest in sorted(source_root.rglob("manifest.json")):
        index_manifest(index, source_root, manifest)
    return index


def index_manifest(index, source_root, manifest):
    """Add every valid draft file of one manifest to a source index."""
    try:
        document = load_json(manifest)
    except Refused:
        return
    if not isinstance(document, dict) or not isinstance(document.get("files"), list):
        return
    for filename in document["files"]:
        index_source_file(index, source_root, manifest, filename)


def index_source_file(index, source_root, manifest, filename):
    """Add the valid draft rows of one manifest member to a source index."""
    if not isinstance(filename, str) or Path(filename).is_absolute() or ".." in Path(filename).parts:
        return
    path = manifest.parent / filename
    try:
        rows = load_json(path)
    except Refused:
        return
    if not isinstance(rows, list):
        return
    for number, row in enumerate(rows):
        if not isinstance(row, dict) or not {"kp_id", "kind", "arguments"} <= set(row):
            continue
        key = (row["kp_id"], row["kind"], canonical(row["arguments"]))
        index.setdefault(key, []).append({
            "manifest": str(manifest.relative_to(source_root.parent.parent)),
            "file": str(path.relative_to(source_root.parent.parent)),
            "row": number,
        })


def fingerprint(document):
    required = ("digest", "kp_id", "kind", "status", "body", "gate", "instances")
    if not isinstance(document, dict) or any(field not in document for field in required):
        raise Refused("a review document is missing a fingerprint field")
    return sha256({field: document[field] for field in required})


def packet_item(document, sources):
    if document["status"] != "pending" or document["kind"] not in KINDS:
        raise Refused(f"{document.get('digest')}: expected a pending known content kind")
    matches = sources.get((document["kp_id"], document["kind"], canonical(document["body"])), [])
    return {
        "digest": document["digest"], "kp_id": document["kp_id"],
        "kind": document["kind"], "status": document["status"],
        "created_at": document.get("created_at"),
        "authoring_attempts": document.get("authoring_attempts"),
        "authoring_cost_usd": document.get("authoring_cost_usd"),
        "source_matches": matches, "body": document["body"],
        "gate": document["gate"], "instances": document["instances"],
        "instances_note": document.get("instances_note"),
        "fingerprint_sha256": fingerprint(document),
    }


def export_packet(api, output, source_root, workers):
    queue = api.list_pending()
    with ThreadPoolExecutor(max_workers=workers) as pool:
        documents = list(pool.map(lambda row: api.document(row["digest"]), queue))
    after = api.list_pending()
    if [row.get("digest") for row in after] != [row.get("digest") for row in queue]:
        raise Refused("the pending queue changed during export")
    by_digest = {row["digest"]: row for row in queue}
    for document in documents:
        queued = by_digest.get(document.get("digest"))
        if queued is None or any(document.get(key) != queued.get(key)
                                 for key in ("kp_id", "kind", "status")):
            raise Refused("the queue changed while its documents were exported")
    sources = source_index(source_root)
    items = sorted((packet_item(document, sources) for document in documents),
                   key=lambda item: (item["kp_id"], item["kind"], item["digest"]))
    core = {"packet_version": PACKET_VERSION, "scope": "all_pending_content", "items": items}
    packet = core | {"packet_sha256": sha256(core)}
    output.write_text(json.dumps(packet, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    decisions = {"decision_version": DECISION_VERSION,
                 "packet_sha256": packet["packet_sha256"], "decisions": []}
    decision_path = output.with_name(output.stem + ".decisions.json")
    decision_path.write_text(json.dumps(decisions, indent=2, sort_keys=True) + "\n")
    return len(items), decision_path, packet["packet_sha256"]


def validate_packet(packet):
    if not isinstance(packet, dict) or packet.get("packet_version") != PACKET_VERSION:
        raise Refused("unsupported packet version")
    core = {key: value for key, value in packet.items() if key != "packet_sha256"}
    if packet.get("packet_sha256") != sha256(core):
        raise Refused("packet fingerprint mismatch")
    items = packet.get("items")
    if not isinstance(items, list):
        raise Refused("packet items must be a list")
    indexed = {}
    for item in items:
        digest = item.get("digest") if isinstance(item, dict) else None
        if not isinstance(digest, str) or digest in indexed:
            raise Refused("packet contains a missing or duplicate digest")
        indexed[digest] = item
    return indexed


def validate_decisions(document, packet, indexed):
    if not isinstance(document, dict) or document.get("decision_version") != DECISION_VERSION:
        raise Refused("unsupported decision version")
    if document.get("packet_sha256") != packet["packet_sha256"]:
        raise Refused("decisions name a different packet")
    rows = document.get("decisions")
    if not isinstance(rows, list) or not rows:
        raise Refused("no explicit decisions were provided")
    seen, result = set(), []
    for number, row in enumerate(rows):
        digest, decision, reason = validated_decision(row, number, indexed, seen)
        seen.add(digest)
        result.append((indexed[digest], decision, reason))
    return result


def validated_decision(row, number, indexed, seen):
    """Validate one digest-bound human decision and return its normalized fields."""
    if not isinstance(row, dict) or not isinstance(row.get("digest"), str):
        raise Refused(f"decision {number}: digest is required")
    digest, decision = row["digest"], row.get("decision")
    if digest in seen or digest not in indexed:
        raise Refused(f"decision {number}: digest is duplicate or outside the packet")
    if decision not in DECISIONS:
        raise Refused(f"decision {number}: decision must be approve or reject")
    allowed = {"digest", "decision"} if decision == "approve" else {"digest", "decision", "reason"}
    if set(row) != allowed:
        raise Refused(f"decision {number}: unexpected or missing fields")
    reason = row.get("reason")
    if decision == "reject" and (not isinstance(reason, str) or not reason.strip()):
        raise Refused(f"decision {number}: rejection reason is required")
    return digest, decision, reason


def write_receipt(path, receipt):
    """Persist each confirmed write so a later failure has an audit trail."""
    if path is not None:
        temporary = path.with_name(path.name + ".tmp")
        temporary.write_text(json.dumps(receipt, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
        os.replace(temporary, path)


def apply_decisions(api, packet_path, decision_path, commit, receipt_path=None, before_write=None, metadata=None):
    if receipt_path is not None and receipt_path.exists():
        raise Refused("receipt path already exists")
    packet, decisions = load_json(packet_path), load_json(decision_path)
    indexed = validate_packet(packet)
    selected = validate_decisions(decisions, packet, indexed)
    checked = []
    for item, decision, reason in selected:
        live = api.document(item["digest"])
        if live.get("status") != "pending" or fingerprint(live) != item.get("fingerprint_sha256"):
            raise Refused(f"{item['digest']}: live document differs from the reviewed pending document")
        checked.append((item, decision, reason))
    if not commit:
        receipt = {"committed": False, "complete": True, "packet_sha256": packet["packet_sha256"],
                "selected": [{"digest": item["digest"], "decision": decision}
                             for item, decision, _reason in checked]}
        write_receipt(receipt_path, receipt)
        return receipt
    receipt = {"committed": False, "complete": False, **(metadata or {}),
               "packet_sha256": packet["packet_sha256"], "receipts": []}
    for item, decision, reason in checked:
        write_receipt(receipt_path, receipt)
        if before_write is not None:
            try:
                before_write(item)
            except Exception as error:
                receipt["pre_write_refusal"] = {"digest": item["digest"], "error": str(error)}
                write_receipt(receipt_path, receipt)
                raise
        live = api.document(item["digest"])
        if live.get("status") != "pending" or fingerprint(live) != item.get("fingerprint_sha256"):
            raise Refused(f"{item['digest']}: live document changed before its write")
        receipt["in_flight"] = {"digest": item["digest"], "decision": decision}
        write_receipt(receipt_path, receipt)
        try:
            answer = api.decide(item["digest"], decision, reason)
        except Exception as error:
            receipt["uncertain"] = receipt.pop("in_flight") | {"error": str(error)}
            write_receipt(receipt_path, receipt)
            raise
        if decision == "approve" and item["kind"] == "template" and answer.get("rejected_documents") is None:
            receipt["uncertain"] = receipt.pop("in_flight", {"digest": item["digest"]}) | {"error": "template re-gate outcome unavailable"}
            write_receipt(receipt_path, receipt)
            raise Refused(f"{item['digest']}: template re-gate outcome unavailable")
        if answer.get("digest") != item["digest"] or answer.get("status") != (
                "approved" if decision == "approve" else "rejected"):
            receipt["uncertain"] = receipt.pop("in_flight", {"digest": item["digest"]}) | {"error": "malformed decision response"}
            write_receipt(receipt_path, receipt)
            raise Refused(f"{item['digest']}: decision response did not confirm the requested write")
        receipt["receipts"].append(answer)
        receipt.pop("in_flight", None)
        receipt["committed"] = True
        write_receipt(receipt_path, receipt)
    receipt["complete"] = True
    write_receipt(receipt_path, receipt)
    return receipt


def parser():
    root = Path(__file__).resolve().parents[2]
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--base-url", required=True)
    common.add_argument("--cookie-file", required=True, type=Path,
                        help="file containing the admin Cookie header value")
    command = argparse.ArgumentParser(description=__doc__)
    subs = command.add_subparsers(dest="command", required=True)
    export = subs.add_parser("export", parents=[common])
    export.add_argument("--output", required=True, type=Path)
    export.add_argument("--source-root", type=Path, default=root / "docs/content-foundations")
    export.add_argument("--workers", type=int, default=8)
    apply = subs.add_parser("apply", parents=[common])
    apply.add_argument("--packet", required=True, type=Path)
    apply.add_argument("--decisions", required=True, type=Path)
    apply.add_argument("--receipt", required=True, type=Path)
    apply.add_argument("--commit", action="store_true")
    return command


def main(argv=None):
    args = parser().parse_args(argv)
    try:
        api = Api(args.base_url, args.cookie_file.read_text())
        if args.command == "export":
            if not 1 <= args.workers <= 32:
                raise Refused("workers must be from 1 through 32")
            count, decisions, digest = export_packet(api, args.output, args.source_root, args.workers)
            print(f"exported {count} pending documents; packet sha256:{digest}; decisions {decisions}")
        else:
            receipt = apply_decisions(api, args.packet, args.decisions, args.commit, args.receipt)
            print(f"checked {len(receipt.get('selected', receipt.get('receipts', [])))} explicit decisions; committed={receipt['committed']}")
    except (Refused, OSError) as error:
        print(f"content review refused: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
