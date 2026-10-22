"""The rule cards' tables of CDISC vocabulary values against the data file."""

import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import yaml
from tools.commands import repository
from tools.paths import ROOT
from tools.tests import support
from tools.validation import check_cdisc_tables as OWNER

CARDS = Path("rules/profiles/submission.md")
JSON_CARDS = Path("rules/profiles/dataset-json.md")
SPELLED = "yaml/vocabulary/cdisc.yaml.vocabularies"


class CdiscTablesTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        for relative in (OWNER.VOCABULARY, CARDS, JSON_CARDS):
            (self.root / relative).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(ROOT / relative, self.root / relative)

    def tearDown(self):
        self.temporary.cleanup()

    def vocabulary(self):
        return support.load_yaml(self.root / OWNER.VOCABULARY)

    def write(self, document):
        (self.root / OWNER.VOCABULARY).write_text(
            yaml.safe_dump(document, sort_keys=False), encoding="utf-8"
        )

    def add(self, name, value):
        document = self.vocabulary()
        document["vocabularies"][name]["values"].append(
            {"value": value, "since": "1.0"}
        )
        self.write(document)

    def replace(self, old, new, relative=CARDS):
        path = self.root / relative
        text = path.read_text(encoding="utf-8")
        self.assertEqual(text.count(old), 1, old)
        path.write_text(text.replace(old, new), encoding="utf-8")

    def at(self, text, card, relative=CARDS):
        """The label of the line of a card file that holds ``text``."""
        lines = (self.root / relative).read_text(encoding="utf-8").splitlines()
        line = next(number for number, item in enumerate(lines, 1) if text in item)
        return f"ERROR: {relative}:{line}: {card}"

    def test_the_repository_tables_list_the_values_of_the_file(self):
        self.assertEqual(OWNER.check(ROOT), [])
        self.assertEqual(OWNER.check(self.root), [])

    def test_the_repository_checks_run_it_after_checking_the_file(self):
        checks = repository.CHECKS
        self.assertLess(
            checks.index("tools.validation.check_cdisc_vocabulary"),
            checks.index("tools.validation.check_cdisc_tables"),
        )

    def test_a_value_the_file_adds_fails_each_table_that_places_its_vocabulary(self):
        sub3 = self.at("| Model | Standard names |", "SUB-3")
        sub9 = self.at("| Role | The column |", "SUB-9")
        sub10 = self.at("| Group | Classes | Models |", "SUB-10")
        sub15 = self.at("| Type | Admitted sources for `SDTM` |", "SUB-15")
        sub26 = self.at("| `data_type` | Form | Examples |", "SUB-26")
        json10 = self.at("| Submission data type |", "JSON-10", JSON_CARDS)
        for name, value, expected in (
            (
                "standard_name",
                "SENDIG-XYZ",
                [f"{sub3}: Standard names does not list `SENDIG-XYZ`"],
            ),
            ("role", "Analysis", [f"{sub9}: Role does not list `Analysis`"]),
            ("dataset_class", "NEW CLASS", [f"{sub10}: Classes does not list"]),
            ("origin_type", "Imputed", [f"{sub15}: Type does not list `Imputed`"]),
            (
                "model",
                "CDASH",
                [
                    f"{sub3}: Model does not list `CDASH`",
                    f"{sub10}: Models does not list `CDASH`",
                ],
            ),
            (
                "submission_data_type",
                "partialDuration",
                [
                    f"{sub26}: `data_type` does not list `partialDuration`",
                    f"{json10}: Submission data type does not list `partialDuration`",
                ],
            ),
            # SDTM admits any source for Other, so no column places sources.
            ("origin_source", "Device", []),
        ):
            with self.subTest(name):
                shutil.copy(ROOT / OWNER.VOCABULARY, self.root / OWNER.VOCABULARY)
                self.add(name, value)
                errors = OWNER.check(self.root)
                self.assertEqual(len(errors), len(expected), errors)
                for error, start in zip(errors, expected):
                    self.assertTrue(error.startswith(start), (error, start))
                    self.assertTrue(
                        error.endswith(f"which {SPELLED}.{name} lists"), error
                    )

    def test_a_table_lists_only_values_of_the_file(self):
        self.replace(
            "| `SDTM` | `SDTMIG`, `SDTMIG-AP`, `SDTMIG-MD` |",
            "| `SDTM` | `SDTMIG`, `SDTMIG-AP`, `SDTMIG-MX` |",
        )
        self.replace("| `Protocol` | `Sponsor` |", "| `Protocol` | `Patient` |")
        self.replace("| `URI` | `URI` |", "| `Uri` | `URI` |", JSON_CARDS)
        self.assertEqual(
            OWNER.check(self.root),
            [
                f"{self.at('`SDTMIG-MX`', 'SUB-3')}: Standard names lists "
                f"`SDTMIG-MX`, which {SPELLED}.standard_name does not",
                f"{self.at('| Model |', 'SUB-3')}: Standard names does not list "
                f"`SDTMIG-MD`, which {SPELLED}.standard_name lists",
                f"{self.at('`Patient`', 'SUB-15')}: Admitted sources for `SDTM` "
                f"lists `Patient`, which {SPELLED}.origin_source does not",
                f"{self.at('| `Uri` |', 'JSON-10', JSON_CARDS)}: Submission data "
                f"type lists `Uri`, which {SPELLED}.submission_data_type does not",
                f"{self.at('| Submission data type |', 'JSON-10', JSON_CARDS)}: "
                "Submission data type does not list `URI`, which "
                f"{SPELLED}.submission_data_type lists",
            ],
        )

    def test_a_value_is_placed_once_in_a_row_it_belongs_in(self):
        self.replace(
            "| analysis | `ADAM OTHER`,", "| analysis | `EVENTS`, `ADAM OTHER`,"
        )
        self.replace("`SENDIG-GENETOX` |", "`SENDIG-GENETOX`, `BIMO` |")
        # A column that places nothing repeats a value in several rows, as
        # `Vendor` is, but not in one cell.
        self.replace(
            "| `Derived` | `Vendor`, `Sponsor` |",
            "| `Derived` | `Vendor`, `Sponsor`, `Vendor` |",
        )
        self.replace("| `time` | `hh:mm:ss`", "| `time`, `text` | `hh:mm:ss`")
        self.assertEqual(
            OWNER.check(self.root),
            [
                f"{self.at('`SENDIG-GENETOX`', 'SUB-3')}: Standard names lists "
                "`BIMO`, which belongs in no row",
                f"{self.at('| analysis |', 'SUB-10')}: Classes lists `EVENTS` twice",
                f"{self.at('| `Derived` |', 'SUB-15')}: Admitted sources for "
                "`SDTM` lists `Vendor` twice",
                f"{self.at('| `time`, `text` |', 'SUB-26')}: `data_type` lists "
                "`text`, which belongs in no row",
            ],
        )

    def test_a_placing_cell_lists_code_spans_and_a_row_has_every_column(self):
        self.replace("| `ADaM` | `ADaMIG`,", "| ADaM | `ADaMIG`,")
        self.replace("| `Protocol` | `Sponsor` |", "| `Protocol` |")
        self.assertEqual(
            OWNER.check(self.root),
            [
                f"{self.at('| ADaM |', 'SUB-3')}: Model cell 'ADaM' is not a list "
                "of code spans",
                f"{self.at('| Model |', 'SUB-3')}: Model does not list `ADaM`, "
                f"which {SPELLED}.model lists",
                f"{self.at('| `Protocol` |', 'SUB-15')}: the row and the header "
                "have 1 and 2 cells",
                f"{self.at('| Type |', 'SUB-15')}: Type does not list `Protocol`, "
                f"which {SPELLED}.origin_type lists",
            ],
        )

    def test_a_card_table_or_vocabulary_that_is_gone_is_reported(self):
        sub3 = self.at("### SUB-3. ", "SUB-3")
        sub10 = self.at("| Group | Classes | Models |", "SUB-10")
        self.replace("| Model | Standard names |", "| Model | Guides |")
        # A heading without an ID heads no card.
        self.replace("### SUB-15. ", "### ")
        document = self.vocabulary()
        del document["vocabularies"]["dataset_class"]
        self.write(document)
        self.assertEqual(
            OWNER.check(self.root),
            [
                f"{sub3}: no table headed | Model | Standard names |",
                f"{sub10}: Classes lists {SPELLED}.dataset_class, which does not exist",
                "ERROR: rules: no card SUB-15, whose table this check reads",
            ],
        )

    def test_a_value_that_belongs_in_no_row_is_a_value_of_the_file(self):
        table = OWNER.Table(
            "SUB-3",
            ("Model", "Standard names"),
            (
                OWNER.Column("model"),
                OWNER.Column(
                    "standard_name", unplaced=("BIMO", "CDISC/NCI", "SDTMIG-PGX")
                ),
            ),
        )
        with mock.patch.object(OWNER, "TABLES", (table,)):
            self.assertEqual(
                OWNER.check(self.root),
                [
                    f"{self.at('| Model |', 'SUB-3')}: `SDTMIG-PGX` belongs in no "
                    f"row of Standard names, but {SPELLED}.standard_name does not "
                    "list it"
                ],
            )

    def test_an_unreadable_file_is_reported_before_any_table(self):
        (self.root / OWNER.VOCABULARY).write_text("a: 1\n", encoding="utf-8")
        errors = OWNER.check(self.root)
        self.assertTrue(errors)
        self.assertTrue(
            all(
                error.startswith("ERROR: yaml/vocabulary/cdisc.yaml.")
                for error in errors
            ),
            errors,
        )


if __name__ == "__main__":
    unittest.main()
