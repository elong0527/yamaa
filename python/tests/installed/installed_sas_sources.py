"""Read SAS sources through the installed public API against independent truth."""

import csv
import gc
import json
import tempfile
import unittest
from pathlib import Path

import polars as pl
from host_fixtures import first_difference

import yamaa

FIXTURES = Path(__file__).with_name("host-fixtures")
TRUTH = json.loads((FIXTURES / "sas-cases.json").read_text(encoding="utf-8"))
if TRUTH["version"] != "1":
    raise ValueError("unsupported SAS host fixture version")


class SasSources(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="yamaa-sas-sources-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def stage(self, case):
        root = self.root / case["id"]
        root.mkdir()
        source = root / case["file"]
        payload = (FIXTURES / "sas" / case["file"]).read_bytes()
        mutation = case.get("mutation", {})
        if "bytes_hex" in mutation:
            payload = bytes.fromhex(mutation["bytes_hex"])
        if "truncate" in mutation:
            payload = payload[: mutation["truncate"]]
        if "replace_hex" in mutation:
            before, after = map(bytes.fromhex, mutation["replace_hex"])
            self.assertEqual(payload.count(before), 1)
            payload = payload.replace(before, after, 1)
        changes = mutation.get("set_bytes", [])
        if "set_byte" in mutation:
            changes = [mutation["set_byte"], *changes]
        for offset, value in changes:
            payload = payload[:offset] + bytes([value]) + payload[offset + 1 :]
        source.write_bytes(payload)
        spec = root / "spec.yaml"
        spec.write_text(json.dumps(case["specification"]), encoding="utf-8")
        return root, spec, source

    def assert_types(self, output, fields):
        self.assertEqual(output.column_names, [field["name"] for field in fields])
        expected = {
            "int": pl.Int64,
            "float": pl.Float64,
            "str": pl.String,
            "date": pl.Date,
        }
        for dtype, field in zip(output.to_polars().dtypes, fields, strict=True):
            if field["type"] == "datetime":
                self.assertIsInstance(dtype, pl.Datetime)
                self.assertIsNone(dtype.time_zone)
            else:
                self.assertEqual(dtype, expected[field["type"]])

    def rows(self, output, fields):
        def scalar(value, kind):
            if value is None:
                return None
            if kind == "int":
                return str(value)
            if kind in {"date", "datetime"}:
                return value.isoformat()
            return value

        return [
            [
                scalar(value, field["type"])
                for value, field in zip(row, fields, strict=True)
            ]
            for row in output.to_polars().rows()
        ]

    def assert_output(self, result, case):
        self.assertEqual(result.issues.num_rows, 0, result.issues)
        self.assertIsNotNone(result.output)
        self.assert_types(result.output, case["fields"])
        rows = self.rows(result.output, case["fields"])
        if "rows" in case:
            # The rows are compared as a summary of their first difference
            # (#3882).
            self.assertIsNone(first_difference(rows, case["rows"]))
        else:
            self.assertEqual(rows[0], case["first_row"])
            # The independent haven owner states only the first row. Do not
            # manufacture other golden rows from the candidate parser.
            self.assertEqual(
                [row[0] for row in rows], [str(i) for i in range(1, len(rows) + 1)]
            )

    def assert_failed(self, result, case, root):
        before = result.issues.to_pylist()
        self.assertEqual([row["condition"] for row in before], [case["condition"]])
        self.assertEqual(before[0]["phase"], case.get("phase", "validation"))
        if "context_fields" in case:
            context = json.loads(before[0]["context"])
            for name, value in case["context_fields"].items():
                self.assertEqual(context[name], value)
            self.assertEqual(before[0]["spec_paths"], case["spec_paths"])
        if "causes" in case:
            context = json.loads(before[0]["context"])
            self.assertEqual(context["dataset"], "SRC")
            self.assertIn(context["cause"], case["causes"])
            self.assertEqual(before[0]["spec_paths"], ["input.SRC.path"])
        self.assertIsNone(result.output)
        for _ in range(2):
            with self.assertRaises(yamaa.Error):
                result.save()
            self.assertEqual(result.issues.to_pylist(), before)
            self.assertFalse((root / "result.csv").exists())

    def test_default_types_narrowing_epochs_missing_and_exact_csv(self):
        for case in TRUTH["accepted"]:
            with self.subTest(case=case["id"]):
                root, spec, _ = self.stage(case)
                self.assertEqual(yamaa.spec(spec).check().issues.num_rows, 0)
                self.assertFalse((root / "result.csv").exists())
                result = yamaa.spec(spec).run()
                self.assert_output(result, case)
                result.save()
                if "csv" in case:
                    self.assertEqual(
                        (root / "result.csv").read_bytes(), case["csv"].encode("utf-8")
                    )

    def test_rle_and_rdc_match_every_independent_csv_cell(self):
        owner = TRUTH["compressed"]
        with (FIXTURES / "sas" / owner["truth"]).open(
            encoding="utf-8", newline=""
        ) as stream:
            reader = csv.DictReader(stream)
            names = reader.fieldnames
            records = list(reader)
        fields = [{"name": "RECORD", "type": "int"}] + [
            {"name": name, "type": "str" if name in owner["text_columns"] else "float"}
            for name in names
        ]
        expected = [
            [str(i)]
            + [
                None
                if record[name] == ""
                else record[name]
                if name in owner["text_columns"]
                else float(record[name])
                for name in names
            ]
            for i, record in enumerate(records, 1)
        ]
        for file in owner["files"]:
            with self.subTest(file=file):
                document = {
                    "yamaa": "1.0",
                    "type": "spec",
                    "domain": "TEST",
                    "base": "SRC",
                    "keys": ["RECORD"],
                    "input": {
                        "SRC": {
                            "path": file,
                            "encoding": owner["encodings"][file],
                            "ordinal": "RECORD",
                        }
                    },
                    "columns": [
                        {**f, "label": f["name"], "derivation": "SRC." + f["name"]}
                        for f in fields
                    ],
                    "output": {
                        "path": "result.csv",
                        "columns": [f["name"] for f in fields],
                    },
                }
                case = {
                    "id": file,
                    "file": file,
                    "specification": document,
                    "fields": fields,
                    "rows": expected,
                }
                root, spec, _ = self.stage(case)
                self.assertEqual(yamaa.spec(spec).check().issues.num_rows, 0)
                result = yamaa.spec(spec).run()
                self.assert_output(result, case)
                result.save()
                self.assertTrue((root / "result.csv").is_file())

    def test_static_encoding_contract_is_shared_by_check_and_run(self):
        for case in TRUTH["static_refusals"]:
            with self.subTest(case=case["id"]):
                root, spec, _ = self.stage(case)
                checked = yamaa.spec(spec).check()
                result = yamaa.spec(spec).run()
                self.assertEqual(checked.issues.to_pylist(), result.issues.to_pylist())
                self.assert_failed(result, case, root)

    def test_bad_sources_are_not_read_by_check_and_failed_saves_have_no_output(self):
        for case in TRUTH["refusals"]:
            with self.subTest(case=case["id"]):
                root, spec, _ = self.stage(case)
                self.assertEqual(yamaa.spec(spec).check().issues.num_rows, 0)
                self.assertFalse((root / "result.csv").exists())
                self.assert_failed(yamaa.spec(spec).run(), case, root)

    def test_held_output_and_repeat_saves_survive_source_and_specification_mutation(
        self,
    ):
        case = next(
            case for case in TRUTH["accepted"] if case["id"] == TRUTH["snapshot_case"]
        )
        root, spec, source = self.stage(case)
        result = yamaa.spec(spec).run()
        self.assert_output(result, case)
        held = result.output
        source.write_bytes(b"changed source")
        changed = yamaa.spec(spec).run()
        self.assert_failed(
            changed,
            {
                "condition": "source_sas_invalid",
                "phase": "ingest",
                "causes": ["framing"],
            },
            root,
        )
        spec.unlink()
        source.unlink()
        gc.collect()
        for _ in range(2):
            result.save()
            self.assertEqual(
                (root / "result.csv").read_bytes(), case["csv"].encode("utf-8")
            )
            self.assertTrue(result.output.to_polars().equals(held.to_polars()))
        del result
        gc.collect()
        self.assertEqual(self.rows(held, case["fields"]), case["rows"])


if __name__ == "__main__":
    unittest.main()
