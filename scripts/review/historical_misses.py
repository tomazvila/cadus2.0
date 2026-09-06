#!/usr/bin/env python3
"""Report missing uncertainty provenance from a complete, read-only tenant export."""
import argparse
import hashlib
import json
import sys
from collections import Counter
from pathlib import Path
from uuid import UUID


def canonical(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"))


def digest(value):
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def invalid_constant(value):
    raise ValueError(f"invalid JSON constant: {value}")


def decode(text):
    return json.loads(text, object_pairs_hook=unique_object, parse_constant=invalid_constant)


def positive_integer(value, label, minimum=1):
    if type(value) is not int or value < minimum:
        raise ValueError(f"{label} must be an integer >= {minimum}")
    return value


def required_text(value, label):
    if not isinstance(value, str) or not value:
        raise ValueError(f"{label} must be nonempty text")
    return value


def parse_record(row, user, through):
    if not isinstance(row, dict) or row.get("record_type") != "event":
        raise ValueError("snapshot must contain event records only")
    if row.get("user_id") != user:
        raise ValueError("event belongs to a different tenant")
    seq = positive_integer(row.get("seq"), "seq")
    if seq > through:
        raise ValueError("out-of-snapshot sequence")
    version = row.get("schema_version")
    if type(version) is not int or version not in (1, 2):
        raise ValueError("unsupported stored schema version")
    raw = required_text(row.get("event_json"), "event_json")
    event = decode(raw)
    if not isinstance(event, dict):
        raise ValueError("event_json must contain an object")
    required_text(event.get("type"), "event type")
    if "v" in event and (type(event["v"]) is not int or event["v"] != version):
        raise ValueError("payload and stored schema versions disagree")
    return seq, version, raw, event


def read_export(lines):
    """Validate complete snapshot coverage before any report can be emitted."""
    documents = [decode(line) for line in lines if line.strip()]
    if not documents or not isinstance(documents[0], dict):
        raise ValueError("a snapshot header is required, including for an empty history")
    header, *rows = documents
    if header.get("record_type") != "snapshot":
        raise ValueError("first record must be the snapshot header")
    user = str(UUID(required_text(header.get("user_id"), "snapshot user_id")))
    expected = positive_integer(header.get("event_count"), "event_count", 0)
    through = positive_integer(header.get("through_seq"), "through_seq", 0)
    if len(rows) != expected or through != expected:
        raise ValueError("incomplete or non-dense snapshot export")
    parsed = [parse_record(row, user, through) for row in rows]
    if len({entry[0] for entry in parsed}) != expected:
        raise ValueError("duplicate sequence")
    # Dense positive unique sequences with the stated count exhaust this prefix.
    return user, sorted(parsed), through


def correction_entries(event, seq, raw):
    entries = event.get("attempts")
    if not isinstance(entries, list):
        raise ValueError("regraded attempts must be a list")
    result = []
    for entry in entries:
        if not isinstance(entry, dict):
            raise ValueError("regraded attempt must be an object")
        attempt_id = required_text(entry.get("attempt_id"), "regraded attempt_id")
        result.append((attempt_id, {
            "seq": seq, "stored_payload_sha256": digest(raw),
            "outcome": entry.get("outcome"),
            "task_id": event.get("task_id"), "topic": event.get("topic"),
            "reason": event.get("reason"),
        }))
    return result


def has_explicit_outcome(event):
    if "outcome" not in event:
        return False
    outcome = event["outcome"]
    if isinstance(outcome, str):
        valid = outcome in ("correct", "incorrect")
    else:
        valid = (isinstance(outcome, dict) and set(outcome) == {"ungraded"}
                 and isinstance(outcome["ungraded"], dict)
                 and set(outcome["ungraded"]) == {"reason"}
                 and isinstance(outcome["ungraded"]["reason"], str))
    if not valid:
        raise ValueError("invalid explicit attempt outcome")
    return True


def review_candidate(user, record, counts):
    seq, version, raw, event = record
    if type(event.get("correct")) is not bool:
        raise ValueError("attempt correct must be a boolean")
    counts["attempts"] += 1
    if version != 1:
        counts["v2_attempts"] += 1
        return None
    counts["v1_attempts"] += 1
    if event["correct"]:
        return None
    counts["v1_incorrect_attempts"] += 1
    if has_explicit_outcome(event):
        counts["v1_incorrect_with_explicit_outcome"] += 1
        return None
    for key in ("task_id", "topic"):
        required_text(event.get(key), key)
    return {
        "user_id": user, "seq": seq, "attempt_id": event["attempt_id"],
        "task_id": event["task_id"], "topic": event["topic"],
        "stored_schema_version": version,
        "payload_version_present": "v" in event,
        "recorded_correct": False,
        "uncertainty_provenance": "not_recorded",
        "review_status": "human_review_required",
        "reason": "The stored v1 miss does not distinguish incorrect from undecidable.",
        "stored_payload_sha256": digest(raw), "stored_payload": raw,
    }


def report(lines):
    user, events, through = read_export(lines)
    candidates, corrections, attempts = [], {}, set()
    counts = Counter()
    for record in events:
        seq, _version, raw, event = record
        if event["type"] == "regraded":
            for attempt_id, evidence in correction_entries(event, seq, raw):
                corrections.setdefault(attempt_id, []).append(evidence)
        if event["type"] != "attempt":
            continue
        attempt_id = required_text(event.get("attempt_id"), "attempt_id")
        if attempt_id in attempts:
            raise ValueError("duplicate attempt identity in tenant snapshot")
        attempts.add(attempt_id)
        candidate = review_candidate(user, record, counts)
        if candidate is not None:
            candidates.append(candidate)
    for candidate in candidates:
        # Corrections are evidence only. Even a decided correction does not prove
        # whether the original checker had uncertainty, or who reviewed it.
        candidate["correction_evidence"] = corrections.get(candidate["attempt_id"], [])
    source = [{"seq": seq, "schema_version": version, "event_json": raw}
              for seq, version, raw, _event in events]
    return {
        "report_version": 1,
        "scope": {"user_id": user, "event_count": len(events), "through_seq": through,
                  "coverage": "complete_exported_snapshot"},
        "source_sha256": digest(canonical({"user_id": user, "events": source})),
        "counts": {name: counts[name] for name in (
            "attempts", "v1_attempts", "v2_attempts", "v1_incorrect_attempts",
            "v1_incorrect_with_explicit_outcome")},
        "review_required": len(candidates),
        "candidates": candidates,
        "limitations": [
            "A candidate is not a recovered ungraded verdict or proof of a grading error.",
            "Original uncertainty and pre-JSONB lexical bytes cannot be reconstructed.",
            "No event, projection, approval, or recovery endpoint is changed by this scan.",
            "The scope is this tenant and exported high-water mark, not the whole deployment.",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("export", type=Path, help="JSONL from historical_misses_export.sql")
    args = parser.parse_args()
    try:
        with args.export.open(encoding="utf-8") as handle:
            result = report(handle)
    except (OSError, ValueError, TypeError) as error:
        print(f"historical scan refused: {error}", file=sys.stderr)
        return 1
    print(canonical(result))
    return 0


if __name__ == "__main__":
    sys.exit(main())
