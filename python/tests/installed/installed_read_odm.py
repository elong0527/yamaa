"""Installed read_odm: the clinical-item table and its refusals.

The reader's complete suite is native (odm_xml.rs); this checks what the
installed host returns for the same two documents, which R reads too.
"""

import io
import os
import tarfile
import tempfile
import unittest
from pathlib import Path

import polars as pl
from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "read-odm")
DOCUMENTS = Path(__file__).resolve().with_name("host-fixtures") / "odm"
ISSUE_COLUMNS = load_truth(__file__, "public-contract")["issue_columns"]


def expected_rows(rows):
    """Integer columns are decimal text in the truth; every other cell is text."""
    return [
        {
            name: int(value)
            if name in TRUTH["integer_columns"] and value is not None
            else value
            for name, value in row.items()
        }
        for row in rows
    ]


class ReadOdm(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="yamaa-read-odm-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        archive = TRUTH["archive"]
        with tarfile.open(self.root / archive["name"], "w") as stream:
            for member in archive["members"]:
                data = (DOCUMENTS / member).read_bytes()
                info = tarfile.TarInfo(member)
                info.size = len(data)
                stream.addfile(info, io.BytesIO(data))

    def source(self, name):
        if name is None:
            return self.root / "absent.xml"
        if name == TRUTH["archive"]["name"]:
            return self.root / name
        return DOCUMENTS / name

    def test_documents_read_into_the_fixed_clinical_item_table(self):
        types = [
            pl.Int64 if name in TRUTH["integer_columns"] else pl.String
            for name in TRUTH["columns"]
        ]
        for read in TRUTH["reads"]:
            with self.subTest(document=read["document"]):
                frame = yamaa.read_odm(self.source(read["document"]))
                self.assertIsInstance(frame, pl.DataFrame)
                self.assertEqual(frame.columns, TRUTH["columns"])
                self.assertEqual(frame.dtypes, types)
                self.assertEqual(frame.to_dicts(), expected_rows(read["rows"]))

    def test_an_archive_member_reads_like_its_document(self):
        archive = TRUTH["archive"]
        selected = next(
            read for read in TRUTH["reads"] if read["document"] == archive["selected"]
        )
        frame = yamaa.read_odm(
            self.root / archive["name"], archive_member=archive["selected"]
        )
        self.assertEqual(frame.to_dicts(), expected_rows(selected["rows"]))

    def test_refusals_raise_yamaa_error_naming_the_reader_code(self):
        for refusal in TRUTH["refusals"]:
            with self.subTest(refusal=refusal["name"]):
                options = {
                    name: value if name == "archive_member" else int(value)
                    for name, value in refusal["options"].items()
                }
                source = self.source(refusal["document"])
                with self.assertRaises(yamaa.Error) as caught:
                    yamaa.read_odm(source, **options)
                error = caught.exception
                self.assertIs(type(error), yamaa.Error)
                # D39: the call names the source as given; a reader failure
                # carries no issue, so its own text follows.
                self.assertTrue(
                    str(error).startswith(
                        f"cannot read {os.fspath(source)}: "
                        f"ODM read failure ({refusal['code']}): "
                    ),
                    str(error),
                )
                # A reader failure is not a specification finding.
                self.assertEqual(error.issues.columns, ISSUE_COLUMNS)
                self.assertTrue(error.issues.is_empty())


if __name__ == "__main__":
    unittest.main()
