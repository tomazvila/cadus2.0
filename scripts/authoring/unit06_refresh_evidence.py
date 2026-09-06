"""Replace eight superseded unit06 draft records, preserving other records byte-for-byte."""
import json
from pathlib import Path

from unit06_templates import OUT

LEGACY = Path("docs/content-foundations/zero-api-completion")


def replace_records(text, replacements):
    """Patch top-level JSON array members without rewriting unrelated units."""
    decoder = json.JSONDecoder()
    cursor = text.index("[") + 1
    spans = []
    while cursor < len(text):
        while text[cursor].isspace() or text[cursor] == ",":
            cursor += 1
        if text[cursor] == "]":
            break
        record, end = decoder.raw_decode(text, cursor)
        key = record.get("kp_id")
        if key in replacements and record.get("kind") == "template":
            replacement = dict(replacements[key])
            if "digest" in record:
                previous = record.get("supersedes_digest", record["digest"])
                replacement["supersedes_digest"] = previous
            rendered = json.dumps(replacement, indent=2, ensure_ascii=False).replace("\n", "\n  ")
            spans.append((cursor, end, rendered))
        cursor = end
    for start, end, value in reversed(spans):
        text = text[:start] + value + text[end:]
    assert len(spans) == 8, len(spans)
    return text


def main():
    drafts = {r["kp_id"]: r for r in json.loads((OUT / "drafts.json").read_text())}
    reviews = {r["kp_id"]: {k: r[k] for k in ("kp_id", "kind", "digest", "status", "body", "storage")}
               for r in json.loads((OUT / "pending-review.json").read_text())}
    for filename, rows in (("drafts.json", drafts), ("stored-review.json", reviews)):
        path = LEGACY / filename
        path.write_text(replace_records(path.read_text(), rows))


if __name__ == "__main__":
    main()
