"""Issue locations (P-ISSUE-1, D28) through the installed public API.

Each case of the shared `issue-locations` truth writes its files, makes one
call and compares its first issue's `file`, `line` and `column` with the
authored location, one case for each rule of
`docs/design/interface-1.0/api.md` at tag `yamaa-v0.6.2`, "Issue locations":
a mapping member at its key, an expanded long form at the written node, a
layered value in the layer whose value took effect, a refusal at the
`parent` or `include` entry of a layer above the entry in that layer, a
project path in the project file or in the file it lists, a data file's
value at its declaration, a file the YAML reader refused at the position it
reported, and an issue without a spec path without a location. R's
issue_locations.R and the command line's parity test check the same truth.
"""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "issue-locations")


def issues(call, root):
    """The issues table the case's call returns, or the refusal carries."""
    path = root / call["path"]
    operation = call["operation"]
    if operation == "check":
        return yamaa.spec(path).check().issues
    if operation == "run":
        return yamaa.spec(path).run().issues
    if operation == "project_check":
        return yamaa.project(path).check().issues
    if operation == "submission":
        return (
            yamaa.project(path)
            .submission(call["section"], created=call["created"])
            .issues
        )
    if operation == "select":
        try:
            yamaa.project(path).spec(call["name"])
        except yamaa.Error as error:
            return error.issues
        raise AssertionError("selecting an unknown name refuses")
    raise ValueError(f"unknown operation: {operation}")


class IssueLocations(unittest.TestCase):
    def test_each_first_issue_is_located_where_the_truth_says(self):
        for case in TRUTH["cases"]:
            with (
                self.subTest(case=case["name"]),
                tempfile.TemporaryDirectory(
                    prefix="yamaa-issue-locations-"
                ) as temporary,
            ):
                # A location names the file by its absolute path as the engine
                # resolves it, with forward slashes.
                root = Path(temporary).resolve()
                for name, text in case["files"].items():
                    (root / name).write_bytes(text.encode("utf-8"))
                row = issues(case["call"], root).to_pylist()[0]
                expected = case["issue"]
                self.assertEqual(row["condition"], expected["condition"])
                path = expected["spec_path"]
                self.assertEqual(row["spec_paths"][:1], [] if path is None else [path])
                file = expected["file"]
                self.assertEqual(
                    (row["file"], row["line"], row["column"]),
                    (
                        None if file is None else (root / file).as_posix(),
                        expected["line"],
                        expected["column"],
                    ),
                )


if __name__ == "__main__":
    unittest.main()
