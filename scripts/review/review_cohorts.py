#!/usr/bin/env python3
"""Split a digest-bound review packet into deterministic unit cohorts."""
import argparse
import json
from pathlib import Path
from content_review_packet import KINDS, Refused, load_json, sha256, validate_packet

MAX_KPS = 16

def unit_name(value):
    return isinstance(value, str) and value and value[0].isalnum() and value[0].islower() and all(ch.islower() or ch.isdigit() or ch == "-" for ch in value)

def cohort_packet(packet, items):
    core = {"packet_version": packet["packet_version"], "scope": packet["scope"], "items": items}
    return core | {"packet_sha256": sha256(core)}

def split(packet, units, maximum=MAX_KPS):
    if type(maximum) is not int or maximum < 1: raise Refused("max-kps must be positive")
    indexed = validate_packet(packet)
    if not indexed: raise Refused("packet has no items")
    required = {item.get("kp_id") for item in indexed.values()}
    if None in required or not required <= set(units): raise Refused("units map omits a packet knowledge point")
    if any(item.get("kind") not in KINDS for item in indexed.values()): raise Refused("packet has an unsupported kind")
    if any(sum(item["kind"] == "template" and item["kp_id"] == key for item in indexed.values()) > 1 for key in required): raise Refused("multiple templates for one knowledge point")
    groups = {}
    for item in indexed.values():
        lane = "templates" if item["kind"] == "template" else "instruction"
        groups.setdefault((units[item["kp_id"]], lane), []).append(item)
    output, seen = [], set()
    for (unit, lane), items in sorted(groups.items()):
        for index in range(0, len({item["kp_id"] for item in items}), maximum):
            keys = set(sorted({item["kp_id"] for item in items})[index:index + maximum])
            batch = [item for item in items if item["kp_id"] in keys]
            if seen.intersection(item["digest"] for item in batch): raise Refused("a document appears in multiple cohorts")
            seen.update(item["digest"] for item in batch)
            output.append((unit, lane, index // maximum + 1, cohort_packet(packet, batch)))
    if seen != set(indexed): raise Refused("cohorts do not partition the packet")
    return output

def write(packet_path, units_path, output, maximum=MAX_KPS):
    packet, units, output = load_json(Path(packet_path)), load_json(Path(units_path)), Path(output)
    if output.exists(): raise Refused("output directory already exists")
    if not isinstance(units, dict) or not all(isinstance(key, str) and unit_name(value) for key, value in units.items()): raise Refused("units map has an invalid unit name")
    cohorts = split(packet, units, maximum)
    output.mkdir()
    manifest = {"packet_sha256": packet["packet_sha256"], "cohorts": []}
    for unit, lane, number, cohort in cohorts:
        name = f"{unit}-{lane}-{number:02d}.json"
        (output / name).write_text(json.dumps(cohort, indent=2, sort_keys=True) + "\n")
        manifest["cohorts"].append({"file": name, "unit": unit, "lane": lane, "count": len(cohort["items"]), "packet_sha256": cohort["packet_sha256"]})
    manifest["cohort_count"] = len(manifest["cohorts"])
    manifest["manifest_sha256"] = sha256({key: value for key, value in manifest.items() if key != "manifest_sha256"})
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--packet", required=True); parser.add_argument("--units-map", required=True)
    parser.add_argument("--output", required=True); parser.add_argument("--max-kps", type=int, default=MAX_KPS)
    args = parser.parse_args()
    write(args.packet, args.units_map, args.output, args.max_kps)
if __name__ == "__main__":
    try: main()
    except Refused as error: raise SystemExit(f"REFUSED: {error}")
