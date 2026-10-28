"""Specification fragments (LAYER-1 to LAYER-4) through the installed API.

The shared `include-fragments` truth writes one study: two layers that a
parent chain and an `include` list compose, and the files of four refusals.
Each check case checks one entry and compares every issue row it returns
with the authored rows, all twelve columns of D28 (#3030); `{root}` stands
for the directory the study is written in, resolved, with forward slashes.
Each parent case checks a fragment with `check(parent=)` and the child that
names that parent and includes the fragment, and requires both to return its
rows (#3694); each refused parent raises its error text and its rows, where
`{path}` stands for the path the test gave and `{parent}` for the parent it
gave.
The resolve case compares the layers the entry composes, in contribution
order, the `parent` and `include` resources, the resolved columns, the origin
of each named value, its layer and the line and column it was written at, and
the fields the resolved specification never holds (LAYER-14). R's
include_fragments.R checks the same truth.
"""

from __future__ import annotations

import json
import os
import tempfile
import unittest
from pathlib import Path

from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "include-fragments")


def write_study(root):
    """Write the family's files into `root`."""
    for name, text in TRUTH["files"].items():
        (root / name).write_bytes(text.encode("utf-8"))


def expected(rows, root):
    """The case's rows with `{root}` filled and each `file`, written relative
    to the study, as a location names it: the absolute path, resolved, with
    forward slashes (P-ISSUE-1)."""
    spelled = root.as_posix()
    quoted = json.dumps(spelled, ensure_ascii=False)[1:-1]
    return [
        {
            **row,
            "file": None if row["file"] is None else (root / row["file"]).as_posix(),
            "message": row["message"].replace("{root}", spelled),
            "context": row["context"].replace("{root}", quoted),
        }
        for row in rows
    ]


def with_parent(rows, parent):
    """`rows` with `{parent}` filled: the parent path the test gave, as given
    in the message and as JSON text in the context."""
    spelled = os.fspath(parent)
    quoted = json.dumps(spelled, ensure_ascii=False)[1:-1]
    return [
        {
            **row,
            "message": row["message"].replace("{parent}", spelled),
            "context": row["context"].replace("{parent}", quoted),
        }
        for row in rows
    ]


class IncludeFragments(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="yamaa-include-fragments-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        write_study(self.root)

    def test_each_check_returns_the_shared_issue_rows(self):
        for case in TRUTH["checks"]:
            with self.subTest(case=case["name"]):
                checked = yamaa.spec(self.root / case["path"]).check()
                self.assertEqual(
                    checked.issues.to_pylist(),
                    expected(case["issues"], self.root),
                    case["name"],
                )

    def test_a_fragment_on_a_parent_returns_the_rows_of_the_child_that_includes_it(
        self,
    ):
        for case in TRUTH["parent_checks"]:
            with self.subTest(case=case["name"]):
                rows = expected(case["issues"], self.root)
                fragment = yamaa.spec(self.root / case["fragment"]).check(
                    parent=self.root / case["parent"]
                )
                child = yamaa.spec(self.root / case["child"]).check()
                self.assertEqual(
                    (fragment.issues.to_pylist(), child.issues.to_pylist()),
                    (rows, rows),
                    case["name"],
                )

    def test_a_parent_given_for_a_file_that_is_no_fragment_is_refused(self):
        for case in TRUTH["parent_refusals"]:
            with self.subTest(case=case["name"]):
                given = self.root / case["path"]
                parent = self.root / case["parent"]
                with self.assertRaises(yamaa.Error) as caught:
                    yamaa.spec(given).check(parent=parent)
                error = caught.exception
                message = case["message"].replace("{path}", os.fspath(given))
                self.assertEqual(
                    (str(error), error.paths, error.issues.to_pylist()),
                    (
                        message.replace("{parent}", os.fspath(parent)),
                        [],
                        with_parent(expected(case["issues"], self.root), parent),
                    ),
                    case["name"],
                )

    def test_resolve_composes_the_parent_chain_then_the_fragments(self):
        truth = TRUTH["resolve"]
        (entry,) = yamaa.spec(self.root / truth["path"]).resolve()["entries"]
        located = lambda name: (self.root / name).as_posix()  # noqa: E731
        self.assertEqual(entry["layers"], [located(name) for name in truth["layers"]])
        self.assertEqual(
            [
                {
                    "spec_path": resource["spec_path"],
                    "role": resource["role"],
                    "declaring_source": resource["declaring_source"],
                    "written": resource["written"],
                    "location": resource["location"]["path"],
                }
                for resource in entry["resources"]
                if resource["role"]
                in ("parent_specification", "included_specification")
            ],
            [
                {
                    **resource,
                    "declaring_source": located(resource["declaring_source"]),
                    "location": located(resource["location"]),
                }
                for resource in truth["resources"]
            ],
        )
        specification = entry["specification"]
        self.assertEqual(
            [column["name"] for column in specification["columns"]], truth["columns"]
        )
        self.assertEqual(
            {pointer: entry["origins"][pointer] for pointer in truth["origins"]},
            {
                pointer: {
                    **origin,
                    "declaring_source": located(origin["declaring_source"]),
                }
                for pointer, origin in truth["origins"].items()
            },
            "the origin of each named value: its layer, line and column",
        )
        for field in truth["absent_fields"]:
            self.assertNotIn(field, specification)


if __name__ == "__main__":
    unittest.main()
