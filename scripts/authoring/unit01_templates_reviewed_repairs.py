"""Reviewed Unit01 repairs loaded independently of served JSON outputs."""
import json
from pathlib import Path
REPAIRS = json.loads(Path(__file__).with_suffix(".json").read_text())
