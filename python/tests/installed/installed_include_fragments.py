"""Specification fragments (LAYER-1 to LAYER-4) through the installed API.

The shared `include-fragments` truth writes one study: two layers that a
parent chain and an `include` list compose, and the files of four refusals.
Each check case checks one entry and compares every issue row it returns
with the authored rows, all twelve columns of D28 (#3030); `{root}` stands
for the directory the study is written in, resolved, with forward slashes.
The resolve case compares the layers the entry composes, in contribution
order, the `parent` and `include` resources, the resolved columns, the layer
each named value comes from, and the fields the resolved specification never
holds (LAYER-14). R's include_fragments.R checks the same truth.
"""

from __future__ import annotations

import json
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
            {
                pointer: entry["origins"][pointer]["declaring_source"]
                for pointer in truth["declaring_sources"]
            },
            {
                pointer: located(name)
                for pointer, name in truth["declaring_sources"].items()
            },
        )
        for field in truth["absent_fields"]:
            self.assertNotIn(field, specification)


if __name__ == "__main__":
    unittest.main()
