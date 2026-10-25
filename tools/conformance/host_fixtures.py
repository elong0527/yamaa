"""Read authored installed-host truth, stage the studies the suites share, and
summarize where an observed value first differs from its truth; never import
an engine or evaluate a case.

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


class _Absent:
    """What a summary shows where one value holds an item the other lacks."""

    def __repr__(self):
        return "<absent>"


ABSENT = _Absent()


def _shown(value):
    """`value`, or its type and length when it is a container of more than
    ten items, so that a summary stays short."""
    if isinstance(value, (list, tuple, dict, set, frozenset)) and len(value) > 10:
        return f"<{type(value).__name__} of {len(value)} items>"
    return value


def first_difference(observed, expected):
    """Where `observed` first differs from `expected`, or None when they are
    equal.

    A suite asserts this summary to be None instead of comparing a value that
    pretty-prints to 250 lines or more. When two such values differ, unittest
    builds the whole difflib diff of their pretty-printed forms before it
    truncates its message, and CPython before 3.14 does that in cubic time
    (python/cpython#119105). One failing assertion could then hold its job
    until the job's time limit rather than report (#3882).

    The summary walks both values together, into the dictionaries and the
    lists or tuples of one type they share. Each step follows the first key
    (in `expected`'s order, then `observed`'s) or index whose items differ.
    It returns `at`, that path, with `observed` and `expected`, the two items
    the walk ends on. An item one side lacks shows as `<absent>`.
    """
    if observed == expected:
        return None
    at = []
    while True:
        if isinstance(observed, dict) and isinstance(expected, dict):
            keys = [*expected, *(key for key in observed if key not in expected)]
            step = next(
                key
                for key in keys
                if observed.get(key, ABSENT) != expected.get(key, ABSENT)
            )
            observed, expected = observed.get(step, ABSENT), expected.get(step, ABSENT)
        elif isinstance(observed, (list, tuple)) and type(observed) is type(expected):
            step = next(
                (
                    index
                    for index, (item, authored) in enumerate(zip(observed, expected))
                    if item != authored
                ),
                min(len(observed), len(expected)),
            )
            observed = observed[step] if step < len(observed) else ABSENT
            expected = expected[step] if step < len(expected) else ABSENT
        else:
            return {
                "at": at,
                "observed": _shown(observed),
                "expected": _shown(expected),
            }
        at.append(step)
