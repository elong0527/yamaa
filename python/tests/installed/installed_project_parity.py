"""Project-run and project-function issue parity through the installed API.

Each case of the shared `project-parity` truth writes its files, makes one
call and compares every issue row it returns with the authored rows, all
twelve columns of D28 (#3030): a listed specification's issues in a project
run, or the run of a specification the project names. A case whose files or
rows depend on the host language names them under `host_files` and under
`issues` by language. `{root}` stands for the directory the case is written
in, resolved, with forward slashes, and `{version}` for the installed yamaa
version. R's project_parity.R checks the same truth.
"""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "project-parity")
HOST = "python"


def issues(call, root):
    """The issues table the case's call returns."""
    path = root / call["path"]
    operation = call["operation"]
    if operation == "project_run":
        return yamaa.project(path).run(call.get("section"))[call["member"]].issues
    if operation == "spec_run":
        return yamaa.project(path).spec(call["name"]).run().issues
    raise ValueError(f"unknown operation: {operation}")


def expected(case, root):
    """The case's rows for this host, with each stand-in filled and each
    `file`, written relative to the case's directory, as a location names
    it: the absolute path, resolved, with forward slashes (P-ISSUE-1)."""
    rows = case["issues"]
    if isinstance(rows, dict):
        rows = rows[HOST]
    values = {"{root}": root.as_posix(), "{version}": yamaa.__version__}

    def filled(text, encode):
        for stand_in, value in values.items():
            text = text.replace(stand_in, encode(value))
        return text

    def as_json(value):
        return json.dumps(value, ensure_ascii=False)[1:-1]

    return [
        {
            **row,
            "file": None if row["file"] is None else (root / row["file"]).as_posix(),
            "message": filled(row["message"], str),
            "context": filled(row["context"], as_json),
        }
        for row in rows
    ]


class ProjectParity(unittest.TestCase):
    def test_issue_rows_equal_the_shared_project_truth(self):
        for case in TRUTH["cases"]:
            with (
                self.subTest(case=case["name"]),
                tempfile.TemporaryDirectory(
                    prefix="yamaa-project-parity-"
                ) as temporary,
            ):
                root = Path(temporary).resolve()
                files = {**case["files"], **case.get("host_files", {}).get(HOST, {})}
                for name, text in files.items():
                    target = root / name
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(text.encode("utf-8"))
                self.assertEqual(
                    issues(case["call"], root).to_pylist(),
                    expected(case, root),
                    case["name"],
                )


if __name__ == "__main__":
    unittest.main()
