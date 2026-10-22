"""Read authored installed-host truth; never import an engine or evaluate a case.

Each installed suite runs in its own process and may read several families,
so the document is decoded once per process: a suite walks the truth once,
however many families it reads. The families it returns are shared within the
process; a suite reads them and never changes them.
"""

import functools
import json
from pathlib import Path


@functools.cache
def _families(root):
    document = json.loads((root / "cases.json").read_text(encoding="utf-8"))
    if document["version"] != "1":
        raise ValueError("unsupported installed-host fixture version")
    return document["families"]


def load(suite, family):
    return _families(Path(suite).resolve().with_name("host-fixtures"))[family]
