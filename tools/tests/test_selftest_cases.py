"""The selftest case list and each refusal of its repository check."""

import contextlib
import io
import tempfile
import unittest
from pathlib import Path

import yaml
from tools.validation import check_selftest_cases as SELFTEST

LIST = SELFTEST.LIST
METADATA = {"lifecycle": "draft", "standard": "TEST", "domain": "TEST"}


class CommittedList(unittest.TestCase):
    def test_the_committed_list_passes(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            self.assertEqual(SELFTEST.main([]), 0)
        # A listed case covers every adapter.
        self.assertIn("not covered: none", output.getvalue())


class Refusals(unittest.TestCase):
    """Three cases that cover every adapter but `odm` and `readstat`, one of
    them negative; each test breaks one condition."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.case(
            "adam-xpt",
            "input/raw.csv",
            "expected/adsl.csv",
            "expected/adsl.xpt",
            "expected/define.xml",
        )
        self.case(
            "adam-json",
            "input/raw.csv",
            "expected/adae.csv",
            "expected/adae.json",
            "expected/define.xml",
        )
        self.case("negative-parquet", "input/raw.parquet", "expected/error.yaml")
        self.document = {
            "cases": ["adam-json", "adam-xpt", "negative-parquet"],
            "uncovered": {
                "odm": "no case reads ODM XML",
                "readstat": "no case reads SAS files",
            },
        }

    def case(self, name, *files):
        directory = self.root / "benchmarks" / name
        for path in ("spec.yaml", *files):
            (directory / path).parent.mkdir(parents=True, exist_ok=True)
            (directory / path).write_text("domain: TEST\n")
        (directory / "benchmark.yaml").write_text(yaml.safe_dump(METADATA))
        return directory

    def errors(self):
        (self.root / LIST).write_text(yaml.safe_dump(self.document))
        return SELFTEST.check(self.root)

    def listing(self, name):
        return sorted(["adam-json", "adam-xpt", "negative-parquet", name])

    def test_the_tree_passes(self):
        self.assertEqual(self.errors(), [])

    def test_a_name_that_is_not_a_benchmark_is_refused(self):
        # `./adam-json` reaches a case directory, but a listed name is bare.
        for name in ("adam-absent", "./adam-json"):
            with self.subTest(name=name):
                self.document["cases"] = self.listing(name)
                self.assertEqual(
                    self.errors(),
                    [f"ERROR: {LIST}.cases: no benchmark named {name!r}"],
                )

    def test_a_project_that_links_code_marks_a_project_function(self):
        project = self.root / "benchmarks/adam-json/project.yaml"
        project.write_text(
            'yamaa: "1.0"\ntype: project\nfunctions: [functions/bmi.yaml]\n'
        )
        self.assertEqual(
            self.errors(),
            [
                f"ERROR: {LIST}.cases: adam-json calls a project function: "
                "project.yaml links functions"
            ],
        )

    def test_a_submission_project_that_links_code_marks_a_project_function(self):
        case = self.root / "benchmarks/adam-json"
        (case / "submission-project.yaml").write_text(
            'yamaa: "1.0"\ntype: project\nfunctions: [functions/bmi.yaml]\n'
        )
        define = {
            "environment": "submission-project.yaml",
            "standard": "adam",
            "created": "2026-09-11T00:00:00",
        }
        (case / "benchmark.yaml").write_text(
            yaml.safe_dump({**METADATA, "define": define})
        )
        self.assertEqual(
            self.errors(),
            [
                f"ERROR: {LIST}.cases: adam-json calls a project function: "
                "submission-project.yaml links functions"
            ],
        )

    def test_a_list_without_a_negative_case_is_refused(self):
        self.document["cases"].remove("negative-parquet")
        self.document["uncovered"]["parquet"] = "no case reads Parquet"
        self.assertEqual(
            self.errors(),
            [
                f"ERROR: {LIST}.cases: no negative case (expected/error.yaml) "
                "runs the issue path"
            ],
        )

    def test_an_adapter_no_listed_case_covers_must_be_named(self):
        del self.document["uncovered"]["readstat"]
        self.assertEqual(
            self.errors(),
            [
                f"ERROR: {LIST}.uncovered: no listed case covers readstat; "
                "list one that does or name it here"
            ],
        )

    def test_an_adapter_a_listed_case_covers_cannot_be_named(self):
        # An expected .xpt shows the XPT writer; a SAS input shows readstat.
        (self.root / "benchmarks/adam-xpt/input/raw.sas7bdat").write_text("")
        self.assertEqual(
            self.errors(),
            [f"ERROR: {LIST}.uncovered.readstat: adam-xpt covers it"],
        )

    def test_an_unknown_adapter_or_a_reason_off_one_line_is_refused(self):
        self.document["uncovered"]["odm"] = "no case reads\nODM XML"
        self.document["uncovered"]["sas"] = "no case reads SAS files"
        self.assertEqual(
            self.errors(),
            [
                f"ERROR: {LIST}.uncovered.odm: expected a one-line reason",
                f"ERROR: {LIST}.uncovered.sas: unknown adapter, expected one of "
                "csv, dataset-json, define-xml, odm, parquet, readstat, xpt",
            ],
        )

    def test_a_malformed_list_is_refused(self):
        unsorted = ["adam-xpt", "adam-json", "negative-parquet"]
        repeated = ["adam-json", "adam-json", "adam-xpt", "negative-parquet"]
        for document, error in (
            (["adam-json"], f"ERROR: {LIST}: expected a mapping"),
            ({"cases": []}, f"ERROR: {LIST}.cases: expected a list of benchmark names"),
            ({**self.document, "extra": 1}, f"ERROR: {LIST}.extra: unknown field"),
            (
                {**self.document, "cases": unsorted},
                f"ERROR: {LIST}.cases: expected unique names in sorted order",
            ),
            (
                {**self.document, "cases": repeated},
                f"ERROR: {LIST}.cases: expected unique names in sorted order",
            ),
        ):
            with self.subTest(document=document):
                self.document = document
                self.assertIn(error, self.errors())


if __name__ == "__main__":
    unittest.main()
