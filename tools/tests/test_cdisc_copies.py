"""The lists of CDISC vocabulary values outside the cards' tables against the
data file."""

import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import yaml
from tools.benchmark_docs import mapping_doc
from tools.commands import repository
from tools.paths import ROOT
from tools.tests import support
from tools.validation import check_cdisc_copies as OWNER

SUBMISSION = Path("rules/profiles/submission.md")
TERMINOLOGY = Path("rules/profiles/terminology.md")
DEFINE = Path("rules/profiles/define-xml.md")
EXCEL = Path("docs/articles/excel.md")
TERMINOLOGY_PAGE = Path("docs/articles/terminology.md")
SPELLED = "yaml/vocabulary/cdisc.yaml.vocabularies"
TEMPORAL = "ERROR: tools/benchmark_docs/mapping_doc.py: TEMPORAL_SUBMISSION_TYPES"


class CdiscCopiesTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        for relative in (
            OWNER.VOCABULARY,
            SUBMISSION,
            TERMINOLOGY,
            DEFINE,
            EXCEL,
            TERMINOLOGY_PAGE,
        ):
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

    def replace(self, old, new, relative=SUBMISSION):
        path = self.root / relative
        text = path.read_text(encoding="utf-8")
        self.assertEqual(text.count(old), 1, old)
        path.write_text(text.replace(old, new), encoding="utf-8")

    def at(self, text, relative=SUBMISSION, card=None):
        """The label of the first line of a file that holds ``text``."""
        lines = (self.root / relative).read_text(encoding="utf-8").splitlines()
        line = next(number for number, item in enumerate(lines, 1) if text in item)
        return f"ERROR: {relative}:{line}" + (f": {card}" if card else "")

    def missing(self, label, start, value, name):
        return (
            f"{label}: the list after {start!r} does not name `{value}`, which "
            f"{SPELLED}.{name} lists"
        )

    def test_the_repository_lists_name_the_values_of_the_file(self):
        self.assertEqual(OWNER.check(ROOT), [])
        self.assertEqual(OWNER.check(self.root), [])

    def test_the_repository_checks_run_it_after_checking_the_file(self):
        checks = repository.CHECKS
        self.assertLess(
            checks.index("tools.validation.check_cdisc_vocabulary"),
            checks.index("tools.validation.check_cdisc_copies"),
        )

    def test_a_value_the_file_adds_fails_each_list_of_its_vocabulary(self):
        sub6 = self.at("`str` admits", card="SUB-6")
        sub10 = self.at("is declared for every model:", card="SUB-10")
        meanings = self.at("For `SDTM` and `SEND`, `Req`", card="SUB-10")
        types = self.at("Each origin's `type` is one of", card="SUB-14")
        sources = self.at("and its `source` one of", card="SUB-14")
        ct2 = self.at("(`ADaM`, `CDASH`", TERMINOLOGY, "CT-2")
        define6 = self.at("`status`, `Draft`", DEFINE, "DEFINE-6")
        item = self.at("(`Req` / `Exp` / `Perm` for SDTM and SEND;", EXCEL)
        row = self.at("| `Core` (", EXCEL)
        names = self.at("in their exact case:", TERMINOLOGY_PAGE)
        sets = self.at("`publishing_set` is one of", TERMINOLOGY_PAGE)
        # Each added value, the passages that must then name it, and what else
        # must hold it.
        for name, value, passages, others in (
            (
                "submission_data_type",
                "partialDuration",
                [(sub6, "`str` admits")],
                [
                    f"{TEMPORAL} lacks `partialDuration`, a date or time type of "
                    f"{SPELLED}.submission_data_type (SUB-26)"
                ],
            ),
            (
                "core_designation",
                "Rec",
                [
                    (sub10, "is declared for every model:"),
                    (meanings, "For `SDTM` and `SEND`,"),
                    (item, "It travels in `submission.core` ("),
                    (row, "| `Core` ("),
                ],
                [],
            ),
            (
                "origin_type",
                "Imputed",
                [(types, "Each origin's `type` is one of")],
                [],
            ),
            ("origin_source", "Device", [(sources, "and its `source` one of")], []),
            (
                "publishing_set",
                "QRS",
                [(ct2, "its publishing set ("), (sets, "`publishing_set` is one of")],
                [],
            ),
            ("standard_status", "Withdrawn", [(define6, "A guide's `status`,")], []),
            ("standard_name", "SENDIG-XYZ", [(names, "in their exact case:")], []),
            # SUB-10 names models beside the designations, but not each of them.
            ("model", "CDASH", [], []),
        ):
            with self.subTest(name):
                document = support.load_yaml(ROOT / OWNER.VOCABULARY)
                document["vocabularies"][name]["values"].append(
                    {"value": value, "since": "1.0"}
                )
                self.write(document)
                self.assertEqual(
                    OWNER.check(self.root),
                    [
                        *(
                            self.missing(label, start, value, name)
                            for label, start in passages
                        ),
                        *others,
                    ],
                )

    def test_a_list_names_only_values_of_its_vocabularies(self):
        self.replace("`str` admits `date`,", "`str` admits `text`, `date`,")
        self.replace(
            "for `SDTM` and `SEND`, and `Req`", "for `SDTM` and `SENDIG`, and `Req`"
        )
        self.replace("`Vendor`. Both vocabularies", "`Device`. Both vocabularies")
        self.assertEqual(
            OWNER.check(self.root),
            [
                f"{self.at('`str` admits', card='SUB-6')}: the list after "
                "'`str` admits' names `text`, which it leaves out",
                f"{self.at('`SENDIG`, and', card='SUB-10')}: the list after "
                "'is declared for every model:' names `SENDIG`, which "
                f"{SPELLED}.core_designation does not",
                f"{self.at('`Device`. Both', card='SUB-14')}: the list after "
                f"'and its `source` one of' names `Device`, which "
                f"{SPELLED}.origin_source does not",
                self.missing(
                    self.at("and its `source` one of", card="SUB-14"),
                    "and its `source` one of",
                    "Vendor",
                    "origin_source",
                ),
            ],
        )

    def test_a_passage_that_is_gone_or_opens_twice_is_reported(self):
        self.replace("A `str` column admits temporal", "So a `str` admits temporal")
        self.replace(", and its `source` one of", ", and each `source` one of")
        self.replace(
            "`CDISC/NCI`, its publishing set", "`CDISC/NCI`, its set", TERMINOLOGY
        )
        # A heading without an ID heads no card.
        self.replace("### DEFINE-6. ", "### ", DEFINE)
        (self.root / TERMINOLOGY_PAGE).unlink()
        missing_page = (
            "ERROR: docs/articles/terminology.md: no such page, whose list this "
            "check reads"
        )
        self.assertEqual(
            OWNER.check(self.root),
            [
                f"{self.at('### SUB-6. ', card='SUB-6')}: 2 passages open with "
                "'`str` admits'",
                f"{self.at('Each origin', card='SUB-14')}: the list after "
                '"Each origin\'s `type` is one of" does not end with '
                "'and its `source`'",
                f"{self.at('### SUB-14. ', card='SUB-14')}: 0 passages open with "
                "'and its `source` one of'",
                f"{self.at('### CT-2. ', TERMINOLOGY, 'CT-2')}: 0 passages open "
                "with 'its publishing set ('",
                "ERROR: rules: no card DEFINE-6, whose list this check reads",
                missing_page,
                missing_page,
            ],
        )

    def test_a_vocabulary_a_list_names_or_a_value_it_leaves_out_is_of_the_file(self):
        passages = (
            OWNER.Passage(
                "SUB-6",
                "`str` admits",
                "submission_data_type",
                omitted=("text", "integer", "float", "decimal"),
            ),
            OWNER.Passage("CT-2", "its publishing set (", "publishing_set", end=")"),
        )
        document = self.vocabulary()
        del document["vocabularies"]["publishing_set"]
        self.write(document)
        with mock.patch.object(OWNER, "PASSAGES", passages):
            self.assertEqual(
                OWNER.check(self.root),
                [
                    f"{self.at('`str` admits', card='SUB-6')}: the list after "
                    "'`str` admits' leaves out `decimal`, but "
                    f"{SPELLED}.submission_data_type does not list it",
                    f"{self.at('### CT-2. ', TERMINOLOGY, 'CT-2')}: the list after "
                    f"'its publishing set (' names {SPELLED}.publishing_set, which "
                    "does not exist",
                ],
            )

    def test_the_mapping_doc_holds_exactly_the_date_and_time_types(self):
        held = (mapping_doc.TEMPORAL_SUBMISSION_TYPES - {"time"}) | {"text"}
        with mock.patch.object(mapping_doc, "TEMPORAL_SUBMISSION_TYPES", held):
            self.assertEqual(
                OWNER.check(self.root),
                [
                    f"{TEMPORAL} lacks `time`, a date or time type of "
                    f"{SPELLED}.submission_data_type (SUB-26)",
                    f"{TEMPORAL} holds `text`, which is no date or time type of "
                    f"{SPELLED}.submission_data_type (SUB-26)",
                ],
            )

    def test_an_unreadable_file_is_reported_before_any_list(self):
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
