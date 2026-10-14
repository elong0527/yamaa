"""Read authored installed-host truth; never import an engine or evaluate a case."""

import json
from pathlib import Path


def load(suite, family):
    root = Path(suite).resolve().with_name("host-fixtures")
    document = json.loads((root / "cases.json").read_text(encoding="utf-8"))
    if document["version"] != "1":
        raise ValueError("unsupported installed-host fixture version")
    return document["families"][family]
