"""PARQUET-7's table against the closed Parquet profile's data file."""

import shutil
import tempfile
import unittest
from pathlib import Path

from tools.commands import repository
from tools.paths import ROOT
from tools.validation import check_parquet_profile as OWNER

CARDS = Path("rules/reference/parquet.md")


class ParquetProfileTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        for relative in (OWNER.PROFILE, CARDS):
            (self.root / relative).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(ROOT / relative, self.root / relative)

    def tearDown(self):
        self.temporary.cleanup()

    def replace(self, relative, old, new):
        path = self.root / relative
        text = path.read_text(encoding="utf-8")
        self.assertEqual(text.count(old), 1, old)
        path.write_text(text.replace(old, new), encoding="utf-8")

    def line(self, text):
        """The line number of the card file line that holds ``text``."""
        lines = (self.root / CARDS).read_text(encoding="utf-8").splitlines()
        return next(number for number, item in enumerate(lines, 1) if text in item)

    def test_the_repository_table_reads_what_the_file_reads(self):
        self.assertEqual(OWNER.check(ROOT), [])
        self.assertEqual(OWNER.check(self.root), [])

    def test_the_repository_checks_run_it(self):
        self.assertIn("tools.validation.check_parquet_profile", repository.CHECKS)

    def test_a_pair_only_the_file_reads_is_reported(self):
        self.replace(
            OWNER.PROFILE, 'logical = ["String"]', 'logical = ["String", "Json"]'
        )
        self.assertEqual(
            OWNER.check(self.root),
            [
                f"ERROR: {OWNER.PROFILE}: reads BYTE_ARRAY annotated Json as str, which "
                "rules/reference/parquet.md PARQUET-7 does not"
            ],
        )

    def test_a_pair_only_the_table_reads_is_reported(self):
        row = "| `DOUBLE` with no annotation | `float` |"
        self.replace(CARDS, row, row + "\n| `FLOAT` with no annotation | `float` |")
        line = self.line("| `FLOAT` with no annotation")
        self.assertEqual(
            OWNER.check(self.root),
            [
                f"ERROR: rules/reference/parquet.md:{line}: PARQUET-7 reads FLOAT "
                f"annotated none as float, which {OWNER.PROFILE} does not"
            ],
        )

    def test_a_listed_annotation_and_a_timestamp_qualifier_each_count(self):
        # One annotation of a list leaves the table, and a timestamp's unit
        # changes: each side then reads a pair the other does not.
        self.replace(CARDS, "`Int(16, signed)` or ", "")
        self.replace(
            CARDS,
            "`Timestamp`, microseconds, no UTC adjustment",
            "`Timestamp`, milliseconds, no UTC adjustment",
        )
        line = self.line("`Timestamp`, milliseconds, no UTC adjustment")
        self.assertEqual(
            OWNER.check(self.root),
            [
                f"ERROR: rules/reference/parquet.md:{line}: PARQUET-7 reads INT64 "
                "annotated Timestamp(MILLIS, isAdjustedToUTC=false) as datetime, which "
                f"{OWNER.PROFILE} does not",
                f"ERROR: {OWNER.PROFILE}: reads INT32 annotated Int(16, signed) as int, "
                "which rules/reference/parquet.md PARQUET-7 does not",
                f"ERROR: {OWNER.PROFILE}: reads INT64 annotated "
                "Timestamp(MICROS, isAdjustedToUTC=false) as datetime, which "
                "rules/reference/parquet.md PARQUET-7 does not",
            ],
        )

    def test_a_row_that_names_no_types_is_reported(self):
        self.replace(
            CARDS, "| `DOUBLE` with no annotation | `float` |", "| a double | float |"
        )
        line = self.line("| a double | float |")
        errors = OWNER.check(self.root)
        self.assertEqual(
            errors[0],
            f"ERROR: rules/reference/parquet.md:{line}: PARQUET-7: the row "
            "['a double', 'float'] names no Parquet and column type",
        )

    def test_a_missing_table_or_unreadable_file_is_reported(self):
        self.replace(
            CARDS, "| Parquet type | Column type |", "| Stored type | Column type |"
        )
        card = self.line("### PARQUET-7.")
        self.assertEqual(
            OWNER.check(self.root),
            [
                f"ERROR: rules/reference/parquet.md:{card}: PARQUET-7: no table "
                "headed | Parquet type | Column type |"
            ],
        )
        (self.root / OWNER.PROFILE).write_text("[[column]\n", encoding="utf-8")
        (error,) = OWNER.check(self.root)
        self.assertTrue(error.startswith(f"ERROR: {OWNER.PROFILE}: "), error)


if __name__ == "__main__":
    unittest.main()
