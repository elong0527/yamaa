"""Installed read_odm: the clinical-item table and its refusals.

The reader's complete suite is native (odm_xml.rs); this checks what the
installed host returns for the same two documents, which R reads too.
"""

import io
import json
import os
import sys
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


def fixture_bytes(value):
    """A truth file is UTF-8 text, or its bytes written as hex."""
    if isinstance(value, dict):
        return bytes.fromhex(value["hex"])
    return value.encode("utf-8")


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
        # Sources the refusals read that are neither document nor archive.
        for name, value in TRUTH["files"].items():
            (self.root / name).write_bytes(fixture_bytes(value))

    def source(self, name):
        if name is None:
            return self.root / "absent.xml"
        if name == TRUTH["archive"]["name"] or name in TRUTH["files"]:
            return self.root / name
        return DOCUMENTS / name

    def test_documents_read_into_the_fixed_clinical_item_table(self):
        types = [
            pl.Int64 if name in TRUTH["integer_columns"] else pl.String
            for name in TRUTH["columns"]
        ]
        for read in TRUTH["reads"]:
            with self.subTest(document=read["document"]):
                table = yamaa.read_odm(self.source(read["document"]))
                self.assertIsInstance(table, yamaa.Table)
                self.assertEqual(table.column_names, TRUTH["columns"])
                self.assertEqual(table.to_polars().dtypes, types)
                self.assertEqual(table.to_pylist(), expected_rows(read["rows"]))

    def test_an_archive_member_reads_like_its_document(self):
        archive = TRUTH["archive"]
        selected = next(
            read for read in TRUTH["reads"] if read["document"] == archive["selected"]
        )
        table = yamaa.read_odm(
            self.root / archive["name"], archive_member=archive["selected"]
        )
        self.assertEqual(table.to_pylist(), expected_rows(selected["rows"]))

    def test_refusals_raise_yamaa_error_carrying_their_issue(self):
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
                # `{source}` stands for the source as given (#3030 D73, D158).
                given = os.fspath(source)
                expected = refusal["issue"]
                message = expected["message"].replace("{source}", given)
                # D39: the call names the source as given, then the issue.
                self.assertEqual(
                    str(error),
                    f"cannot read {given}: {message} "
                    f"[{expected['condition']}, {expected['rule']}]",
                )
                self.assertEqual(error.issues.column_names, ISSUE_COLUMNS)
                [row] = error.issues.to_pylist()
                self.assertEqual(
                    {
                        name: row[name]
                        for name in ("severity", "phase", "condition", "rule")
                    },
                    {
                        "severity": "error",
                        "phase": expected["phase"],
                        "condition": expected["condition"],
                        "rule": expected["rule"],
                    },
                )
                self.assertEqual(row["spec_paths"], expected["spec_paths"])
                self.assertEqual(row["message"], message)
                context = {
                    name: value.replace("{source}", given)
                    if isinstance(value, str)
                    else value
                    for name, value in expected["context"].items()
                }
                self.assertEqual(json.loads(row["context"]), context)

    def test_oversized_integer_limits_refuse_with_the_original_value(self):
        # Python's guarded integer-to-native conversion cannot be a JSON fixture.
        threshold = sys.int_info.str_digits_check_threshold
        previous = sys.get_int_max_str_digits()
        digits = "1" + "0" * threshold
        source = self.root / "unread.xml"
        argument = "max_xml_depth"
        truth = TRUTH["invalid_limits"]
        minimum = truth["arguments"][argument]
        try:
            sys.set_int_max_str_digits(threshold)
            for value, shown, label in [
                (10**threshold, digits, "positive"),
                (-(10**threshold), "-" + digits, "negative"),
            ]:
                with self.subTest(case=label):
                    message = (
                        f"{argument} has the value {shown}, which is not permitted"
                    )
                    expected = truth["issue"] | {
                        "spec_paths": [argument],
                        "message": message,
                        "context": f'{{"minimum":{minimum},"value":{shown}}}',
                    }
                    with self.assertRaises(yamaa.Error) as caught:
                        yamaa.read_odm(source, **{argument: value})
                    error = caught.exception
                    self.assertEqual(
                        (
                            type(error),
                            str(error),
                            error.issues.column_names,
                            error.issues.to_pylist(),
                            error.paths,
                            sys.get_int_max_str_digits(),
                        ),
                        (
                            yamaa.Error,
                            f"cannot read {source}: {message} [value_not_permitted, ODM-1]",
                            ISSUE_COLUMNS,
                            [expected],
                            [],
                            threshold,
                        ),
                    )
        finally:
            sys.set_int_max_str_digits(previous)

    def test_invalid_limits_refuse_at_the_argument_before_reading_the_source(self):
        truth = TRUTH["invalid_limits"]
        source = self.root / "unread.xml"
        for argument, minimum in truth["arguments"].items():
            for case in truth["values"]:
                with self.subTest(argument=argument, value=case["name"]):
                    value = case["value"]
                    shown = value if isinstance(value, str) else json.dumps(value)

                    def fill(text):
                        return (
                            text.replace("{argument}", argument)
                            .replace("{value}", shown)
                            .replace("{source}", str(source))
                        )

                    expected = {
                        name: fill(item)
                        if isinstance(item, str)
                        else [fill(part) for part in item]
                        if isinstance(item, list)
                        else item
                        for name, item in truth["issue"].items()
                    }
                    expected["context"] = json.dumps(
                        {"minimum": minimum, "value": value},
                        sort_keys=True,
                        separators=(",", ":"),
                    )
                    with self.assertRaises(yamaa.Error) as caught:
                        yamaa.read_odm(source, **{argument: value})
                    error = caught.exception
                    self.assertEqual(
                        (
                            type(error),
                            str(error),
                            error.issues.column_names,
                            error.issues.to_pylist(),
                            error.paths,
                        ),
                        (
                            yamaa.Error,
                            fill(truth["error"]),
                            ISSUE_COLUMNS,
                            [expected],
                            [],
                        ),
                    )


if __name__ == "__main__":
    unittest.main()
