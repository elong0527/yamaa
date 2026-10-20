"""The selftest case list and each refusal of its repository check."""

import contextlib
import io
import shutil
import tempfile
import unittest
from pathlib import Path

import yaml
from tools.migrate.excepted import EXCEPTED
from tools.paths import ROOT
from tools.validation import check_selftest_cases as SELFTEST

LIST = SELFTEST.LIST
METADATA = {"lifecycle": "draft", "standard": "TEST", "domain": "TEST"}


class CommittedList(unittest.TestCase):
    def test_the_committed_list_passes(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            self.assertEqual(SELFTEST.main([]), 0)
        self.assertIn("not covered: odm, readstat", output.getvalue())

    def test_every_excepted_name_is_a_benchmark(self):
        # A retired or renamed case leaves the shared exception list with it.
        for name in EXCEPTED:
            with self.subTest(name=name):
                self.assertTrue(
                    (ROOT / "benchmarks" / name / "benchmark.yaml").is_file()
                )


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

    def test_a_host_environment_file_marks_a_project_function(self):
        for path in ("python/environment.yaml", "r/environment.yaml"):
            with self.subTest(path=path):
                environment = self.root / "benchmarks/adam-json" / path
                environment.parent.mkdir()
                environment.write_text("schema_version: '1.0'\n")
                self.assertEqual(
                    self.errors(),
                    [
                        f"ERROR: {LIST}.cases: adam-json calls a project function: {path}"
                    ],
                )
                shutil.rmtree(environment.parent)

    def test_a_project_that_links_code_marks_a_project_function(self):
        project = self.root / "benchmarks/adam-json/project.yaml"
        for field, text in (
            ("environments", "environments: {python: code/environment.yaml}\n"),
            ("functions", "type: project\nfunctions: [functions/bmi.yaml]\n"),
        ):
            with self.subTest(field=field):
                project.write_text(text)
                self.assertEqual(
                    self.errors(),
                    [
                        f"ERROR: {LIST}.cases: adam-json calls a project function: "
                        f"project.yaml links {field}"
                    ],
                )

    def test_a_submission_project_that_links_code_marks_a_project_function(self):
        case = self.root / "benchmarks/adam-json"
        (case / "submission-project.yaml").write_text(
            "environments: {r: r/environment.yaml}\n"
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
                "submission-project.yaml links environments"
            ],
        )

    def test_a_case_the_conversion_gate_excepts_is_refused(self):
        for name, reason in EXCEPTED.items():
            with self.subTest(name=name):
                self.case(name, "input/raw.csv", "expected/error.yaml")
                self.document["cases"] = self.listing(name)
                self.assertEqual(
                    self.errors(),
                    [
                        f"ERROR: {LIST}.cases: {name} is excepted from the 1.0 "
                        f"conversion gate ({reason})"
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
