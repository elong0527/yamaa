"""Installed yamaa.lint: the P-LINT-2 table, fixes, a partial restatement of
an inherited mapping and values restated as they are inherited, which read
the parent layer, and refusals.

The lint's complete suite is native (the adapters' lint tests); this checks
what the installed host returns for the shared truth, which R reads too.
"""

import os
import tempfile
import unittest
from pathlib import Path

import polars as pl
from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "lint")


def expected(rows):
    """Integer columns are decimal text in the truth."""
    return [
        {
            name: int(value) if name in TRUTH["integer_columns"] else value
            for name, value in row.items()
        }
        for row in rows
    ]


class Lint(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="yamaa-lint-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.path = self.root / TRUTH["file_name"]
        self.path.write_bytes(TRUTH["specification_yaml"].encode("utf-8"))

    def check_table(self, result, rows, path=None):
        table = result.findings
        self.assertIsInstance(table, yamaa.Table)
        self.assertEqual(table.column_names, TRUTH["columns"])
        table = table.to_polars()
        self.assertEqual(
            table.dtypes,
            [pl.String, pl.Int64, pl.Int64, pl.String, pl.String, pl.String],
        )
        absolute = os.path.abspath(path or self.path).replace(os.sep, "/")
        self.assertEqual(table["file"].to_list(), [absolute] * len(rows))
        self.assertEqual(table.drop("file").to_dicts(), expected(rows))

    def check_result(self, result, key):
        expected = TRUTH["result"][key]
        # A rewritten path is the file named, absolute, with forward slashes,
        # as the findings' file column is: not resolved through links or
        # short names.
        paths = (
            [os.path.abspath(self.path).replace(os.sep, "/")]
            if expected["rewritten"]
            else []
        )
        self.assertEqual(
            (
                type(result),
                sorted(name for name in dir(result) if not name.startswith("_")),
                result.status,
                result.paths,
                repr(result),
            ),
            (
                yamaa.Lint,
                sorted(TRUTH["result"]["members"]),
                expected["status"],
                paths,
                expected["summary"],
            ),
        )

    def test_findings_come_as_the_six_column_table(self):
        result = yamaa.lint(self.path)
        self.check_result(result, "before")
        self.check_table(result, TRUTH["rows"])
        self.check_table(yamaa.lint([str(self.path)], fix=False), TRUTH["rows"])
        self.assertEqual(
            self.path.read_bytes().decode("utf-8"), TRUTH["specification_yaml"]
        )

    def test_fix_rewrites_the_file_and_lists_what_remains(self):
        before = yamaa.lint(self.path)
        fixed = yamaa.lint([self.path], fix=True)
        self.check_result(fixed, "fixed")
        self.check_table(fixed, TRUTH["fixed_rows"])
        self.check_result(before, "before")
        self.check_table(before, TRUTH["rows"])
        self.assertEqual(self.path.read_bytes().decode("utf-8"), TRUTH["fixed_yaml"])
        self.check_table(yamaa.lint(self.path), TRUTH["fixed_rows"])

    def test_a_layer_is_compared_with_its_parent_and_fix_leaves_it(self):
        for key in ("partial_restatement", "restated_inherited_value"):
            case = TRUTH[key]
            for name, text in case["files"].items():
                (self.root / name).write_bytes(text.encode("utf-8"))
            path = self.root / case["file_name"]
            for fix in (False, True):
                with self.subTest(case=key, fix=fix):
                    self.check_table(yamaa.lint(path, fix=fix), case["rows"], path)
                    self.assertEqual(
                        path.read_bytes().decode("utf-8"),
                        case["files"][case["file_name"]],
                    )

    def test_a_clean_table_keeps_its_columns(self):
        self.path.write_bytes(b"")
        result = yamaa.lint(self.path)
        self.check_result(result, "clean")
        table = result.findings
        self.assertEqual(table.column_names, TRUTH["columns"])
        self.assertEqual(table.num_rows, 0)

    def test_refusals_raise_yamaa_error_naming_the_condition(self):
        for refusal in TRUTH["refusals"]:
            with self.subTest(condition=refusal["condition"]):
                path = self.root / "refused.yaml"
                if refusal["yaml"] is not None:
                    path.write_bytes(refusal["yaml"].encode("utf-8"))
                with self.assertRaises(yamaa.Error) as raised:
                    yamaa.lint([self.path, path], fix=True)
                issues = raised.exception.issues
                self.assertEqual(
                    issues.to_polars()["condition"].to_list(), [refusal["condition"]]
                )
                # A refused call writes nothing.
                self.assertEqual(
                    self.path.read_bytes().decode("utf-8"),
                    TRUTH["specification_yaml"],
                )
                path.unlink(missing_ok=True)


if __name__ == "__main__":
    unittest.main()
