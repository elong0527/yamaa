"""Read authored installed-host truth and stage the studies the suites share;
never import an engine or evaluate a case.

Each installed suite runs in its own process and may read several families,
so the document is decoded once per process: a suite walks the truth once,
however many families it reads. The families it returns are shared within the
process; a suite reads them and never changes them.
"""

import functools
import json
import shutil
from pathlib import Path


@functools.cache
def _families(root):
    document = json.loads((root / "cases.json").read_text(encoding="utf-8"))
    if document["version"] != "1":
        raise ValueError("unsupported installed-host fixture version")
    return document["families"]


def load(suite, family):
    return _families(Path(suite).resolve().with_name("host-fixtures"))[family]


def function_study(suite, case):
    """Copy negative-function-contract, which the suite's directory holds in
    project-original, to the new directory `case`, with its argument supplied,
    so that a run calls the project function for each row; return `case`."""
    original = (
        Path(suite).resolve().with_name("project-original")
        / "negative-function-contract"
    )
    shutil.copytree(original, case)
    source = Path(case) / "spec.yaml"
    source.write_text(
        source.read_text(encoding="utf-8").replace(
            "        name: project_value\n",
            "        name: project_value\n        args: {x: SOURCE.VALUE}\n",
        ),
        encoding="utf-8",
    )
    return Path(case)
