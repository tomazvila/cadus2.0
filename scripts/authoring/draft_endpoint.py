#!/usr/bin/env python3
"""Serve explicit operator drafts to the normal pending-content pipeline.

Loopback only. This process makes no network request and spends no API credit.
Unknown keys fail closed. The worker still applies its production content gates.
"""
import argparse
import json
import re
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


def answer(request, drafts):
    """Resolve one explicit topic, knowledge point and forced tool."""
    user = next(message["content"] for message in request["messages"] if message["role"] == "user")
    topic = re.search(r"^Topic: .* \(id: ([^)]+)\)$", user, re.MULTILINE)
    kp = re.search(r"^Knowledge point: .* \(id: ([^)]+)\)$", user, re.MULTILINE)
    tool = request["tool_choice"]["function"]["name"]
    kinds = {"emit_template": "template", "emit_teach": "teach", "emit_hint_ladder": "hint_ladder"}
    key = (f"{topic.group(1)}/{kp.group(1)}", kinds[tool])
    arguments = drafts[key]
    return {
        "id": "operator-draft",
        "model": "operator-draft-v1",
        "usage": {"prompt_tokens": 0, "completion_tokens": 0, "cost": 0},
        "choices": [{"finish_reason": "tool_calls", "message": {
            "role": "assistant", "tool_calls": [{"id": "draft", "type": "function", "function": {
                "name": tool, "arguments": json.dumps(arguments)
            }}]
        }}],
    }


def server_for(port, drafts):
    """Build the loopback server; never log headers, keys or request bodies."""
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def do_POST(self):
            status = 400
            result = {"error": "request does not identify one explicit operator draft"}
            try:
                length = int(self.headers.get("Content-Length", "0"))
                if self.path != "/v1/chat/completions" or not 0 < length <= 65_536:
                    raise ValueError("invalid request boundary")
                request = json.loads(self.rfile.read(length))
                result = answer(request, drafts)
                status = 200
            except (ValueError, KeyError, TypeError, AttributeError, StopIteration):
                pass
            payload = json.dumps(result).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

    return ThreadingHTTPServer(("127.0.0.1", port), Handler)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--ready-file", required=True)
    parser.add_argument("--drafts", default=str(Path(__file__).resolve().parents[2] / "docs/content-pilot/arithmetic-instruction-drafts.json"))
    args = parser.parse_args()
    rows = json.loads(Path(args.drafts).read_text())
    drafts = {(row["kp_id"], row["kind"]): row["arguments"] for row in rows}
    if len(drafts) != len(rows):
        raise ValueError("duplicate draft keys")
    with server_for(args.port, drafts) as server:
        url = f"http://127.0.0.1:{server.server_address[1]}/v1"
        Path(args.ready_file).write_text(url + "\n")
        print(url, flush=True)
        server.serve_forever()


if __name__ == "__main__":
    main()
