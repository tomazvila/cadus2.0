#!/usr/bin/env python3
"""Split a digest-bound review packet into deterministic unit cohorts."""
import argparse
import json
from pathlib import Path

from content_review_packet import KINDS, Refused, load_json, sha256, validate_packet

MAX_KPS = 16


def unit_name(value):
    return (
        isinstance(value, str)
        and value
        and value[0].isalnum()
        and value[0].islower()
        and all(ch.islower() or ch.isdigit() or ch == "-" for ch in value)
    )


def cohort_packet(packet, items):
    core = {
        "packet_version": packet["packet_version"],
        "scope": packet["scope"],
        "items": items,
    }
    return core | {"packet_sha256": sha256(core)}


def validate_inputs(packet, units, maximum):
    if type(maximum) is not int or maximum < 1:
        raise Refused("max-kps must be positive")
    if not isinstance(units, dict) or not all(
        isinstance(key, str) and unit_name(value) for key, value in units.items()
    ):
        raise Refused("units map has an invalid unit name")
    indexed = validate_packet(packet)
    if not indexed:
        raise Refused("packet has no items")
    items = list(indexed.values())
    required = {item.get("kp_id") for item in items}
    if None in required or not required <= set(units):
        raise Refused("units map omits a packet knowledge point")
    if any(item.get("kind") not in KINDS for item in items):
        raise Refused("packet has an unsupported kind")
    templates = [item["kp_id"] for item in items if item["kind"] == "template"]
    if len(templates) != len(set(templates)):
        raise Refused("multiple templates for one knowledge point")
    return indexed, items


def grouped_items(items, units):
    groups = {}
    for item in items:
        lane = "templates" if item["kind"] == "template" else "instruction"
        groups.setdefault((units[item["kp_id"]], lane), []).append(item)
    return sorted(
        (unit, lane, sorted(rows, key=lambda row: (row["kp_id"], row["kind"], row["digest"])))
        for (unit, lane), rows in groups.items()
    )


def split(packet, units, maximum=MAX_KPS):
    indexed, items = validate_inputs(packet, units, maximum)
    output, seen = [], set()
    for unit, lane, rows in grouped_items(items, units):
        keys = sorted({item["kp_id"] for item in rows})
        for number, start in enumerate(range(0, len(keys), maximum), 1):
            batch_keys = set(keys[start : start + maximum])
            batch = [item for item in rows if item["kp_id"] in batch_keys]
            digests = {item["digest"] for item in batch}
            if seen.intersection(digests):
                raise Refused("a document appears in multiple cohorts")
            seen.update(digests)
            output.append((unit, lane, number, cohort_packet(packet, batch)))
    if seen != set(indexed):
        raise Refused("cohorts do not partition the packet")
    return output


def write(packet_path, units_path, output, maximum=MAX_KPS):
    packet, units, output = load_json(Path(packet_path)), load_json(Path(units_path)), Path(output)
    if output.exists():
        raise Refused("output directory already exists")
    cohorts = split(packet, units, maximum)
    output.mkdir()
    manifest = {"packet_sha256": packet["packet_sha256"], "cohorts": []}
    for unit, lane, number, cohort in cohorts:
        name = f"{unit}-{lane}-{number:02d}.json"
        (output / name).write_text(json.dumps(cohort, indent=2, sort_keys=True) + "\n")
        manifest["cohorts"].append(
            {"file": name, "unit": unit, "lane": lane, "count": len(cohort["items"]),
             "packet_sha256": cohort["packet_sha256"]}
        )
    manifest["cohort_count"] = len(manifest["cohorts"])
    manifest["manifest_sha256"] = sha256(
        {key: value for key, value in manifest.items() if key != "manifest_sha256"}
    )
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--packet", required=True)
    parser.add_argument("--units-map", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--max-kps", type=int, default=MAX_KPS)
    args = parser.parse_args()
    write(args.packet, args.units_map, args.output, args.max_kps)


if __name__ == "__main__":
    try:
        main()
    except Refused as error:
        raise SystemExit(f"REFUSED: {error}")
