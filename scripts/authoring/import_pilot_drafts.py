#!/usr/bin/env python3
"""Gate explicit operator drafts and store them pending with zero API spend.

DATABASE_URL selects the target database. This helper never approves content.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import threading
from draft_endpoint import server_for


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worker", required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    rows = json.loads((root / "docs/content-pilot/arithmetic-instruction-drafts.json").read_text())
    drafts = {(row["kp_id"], row["kind"]): row["arguments"] for row in rows}
    server = server_for(0, drafts)
    thread = threading.Thread(target=server.serve_forever)
    thread.start()
    try:
        environment = os.environ.copy()
        environment.update({
            "OPENAI_BASE_URL": f"http://127.0.0.1:{server.server_address[1]}/v1",
            "OPENAI_API_KEY": "operator-draft-local-only",
            "OPENAI_MODEL": "operator-draft-v1",
            "CADUS_CURRICULUM": str(root / "curriculum"),
        })
        command = [args.worker, "author", "--kind", "teach", "--kind", "hint_ladder",
                   "--budget-usd", "5", "--request-reserve-usd", "0.50", "--concurrency", "4"]
        for key in dict.fromkeys(row["kp_id"] for row in rows):
            command.extend(["--kp", key])
        return subprocess.run(command, env=environment, cwd=root, check=False).returncode
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


if __name__ == "__main__":
    raise SystemExit(main())
