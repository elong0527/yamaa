"""Installed original specifications, Parquet family: the ordered-sum case
through Parquet input and output, the closed types and their complete refusal
reports, the empty-string policy and dictionary columns, with reference
semantic imports forbidden. R reads the same truth in
original_specifications_parquet.R."""

import gc
import json
import struct
import sys
import tempfile
import unittest
from pathlib import Path

import pyarrow as pa
from host_fixtures import first_difference
from host_fixtures import load as load_truth

import yamaa
from yamaa import _native as yamaa_native

TRUTH = load_truth(__file__, "original-specifications")

ROOT = Path(__file__).with_name("specification-original")
# The five fields of the public issues table (D28) that the authored findings
# state.
FIELDS = ("phase", "condition", "rule", "spec_paths", "context")


class ParquetOriginalOutput(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="yamaa-public-parquet-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.entry = self.root / "spec.yaml"

    def build(self, source, inputs):
        self.entry.write_bytes(
            json.dumps(source).encode("ascii") if isinstance(source, dict) else source
        )
        for name, content in inputs.items():
            target = self.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(
                content.read_bytes() if isinstance(content, Path) else content
            )
        return yamaa.spec(self.entry).run()

    def observe(self, result):
        return json.loads(result._native.observations())

    def expected_original(self, name, *, artifacts=False):
        expected = json.loads((ROOT / "expected" / (name + ".json")).read_text())
        expected.update(
            runtime="python",
            runtime_version=sys.version,
            engine_version=yamaa_native._CORE_VERSION,
            example="domain",
        )
        for group in ("nodes", "tables", "verifications", "callbacks"):
            for record in expected[group]:
                record["specification"] = str(self.entry)
        if not artifacts:
            expected["artifacts"] = []
        return expected

    def failed_report(self, diagnostics, paths):
        return {
            "artifacts": [],
            "backend": "rust",
            "callbacks": [],
            "diagnostics": diagnostics,
            "engine_version": yamaa_native._CORE_VERSION,
            "error": None,
            "example": "domain",
            "handler_counts": [],
            "nodes": [
                {
                    "diagnostics": diagnostics,
                    "handler_counts": [],
                    "outcome": "failure",
                    "specification": str(self.entry),
                    "unsupported": [],
                }
            ],
            "outcome": "failure",
            "report_version": "0.3.0-draft",
            "runtime": "python",
            "runtime_version": sys.version,
            "source_reads": [
                {
                    "base_directory": ".",
                    "path": path,
                    "outcome": "captured",
                    "condition": None,
                    "snapshots_created": 1,
                }
                for path in paths
            ],
            "tables": [],
            "unsupported": [],
            "verifications": [],
        }

    def test_parquet_empty_string_policy_survives_build_and_later_save(self):
        for policy, expected in (
            ("missing", b"I,S\n1,\n2,X\n"),
            ("present", b'I,S\n1,""\n2,X\n'),
        ):
            with self.subTest(policy=policy):
                source = {
                    "yamaa": "1.0",
                    "type": "spec",
                    "domain": "TEST",
                    "input": {"SRC": {"path": "input.PARQUET", "empty_string": policy}},
                    "keys": ["I"],
                    "columns": [
                        {"name": "I", "type": "int", "derivation": "SRC.I"},
                        {"name": "S", "type": "str", "derivation": "SRC.S"},
                    ],
                    "output": {"path": "output.csv", "columns": ["I", "S"]},
                }
                result = self.build(source, {"input.PARQUET": ROOT / "pq/text.parquet"})
                self.assertEqual(result.issues.num_rows, 0, result.issues)
                self.entry.unlink()
                (self.root / "input.PARQUET").unlink()
                gc.collect()
                for _ in range(2):
                    result.save()
                    self.assertEqual((self.root / "output.csv").read_bytes(), expected)

    def test_parquet_view_and_dictionary_layouts_read_as_text(self):
        # pyarrow string views and dictionaries, Polars Categorical and Enum,
        # and a Polars frame handed to pyarrow through the Arrow PyCapsule
        # interface each store a BYTE_ARRAY String field whose embedded Arrow
        # schema names a view or dictionary layout. A layout is not a type
        # (PARQUET-7), so every field reads as the text pyarrow reads (#2940).
        import base64

        import polars as pl
        import pyarrow.parquet as pq

        text = ["b", None, "", "a", "b", "\u03b2"]
        ids = list(range(1, 7))
        plain = pa.array(text, pa.string())
        indices = (pa.int8(), pa.int16(), pa.int32(), pa.int64())
        indices += (pa.uint8(), pa.uint16(), pa.uint32(), pa.uint64())
        frame = pl.DataFrame(
            {
                "ID": ids,
                "CATEGORICAL": pl.Series(text, dtype=pl.Categorical),
                "ENUM": pl.Series(text, dtype=pl.Enum(["a", "b", "", "\u03b2"])),
            }
        )
        writers = {
            "pyarrow": lambda path: pq.write_table(
                pa.table(
                    {
                        "ID": pa.array(ids, pa.int64()),
                        "VIEW": pa.array(text, pa.string_view()),
                        "LARGE": pa.array(text, pa.large_string()).dictionary_encode(),
                        **{
                            str(index).upper(): plain.dictionary_encode().cast(
                                pa.dictionary(index, pa.string())
                            )
                            for index in indices
                        },
                    }
                ),
                path,
            ),
            "polars": frame.write_parquet,
            "pycapsule": lambda path: pq.write_table(
                pa.table(pl.DataFrame({"ID": ids, "VIEW": text})), path
            ),
        }
        for writer, write in writers.items():
            with self.subTest(writer=writer):
                path = self.root / (writer + ".parquet")
                write(path)
                # Decode the hint as written, independent of how this pyarrow
                # restores it on reading.
                hint = pa.ipc.read_schema(
                    pa.py_buffer(
                        base64.b64decode(
                            pq.ParquetFile(path).metadata.metadata[b"ARROW:schema"]
                        )
                    )
                )
                fields = hint.names[1:]
                stored = pq.read_table(path)
                for name in fields:
                    layout = hint.field(name).type
                    self.assertTrue(
                        pa.types.is_string_view(layout)
                        or pa.types.is_dictionary(layout),
                        f"{name}: {layout}",
                    )
                    self.assertEqual(stored[name].to_pylist(), text, name)
                source = {
                    "yamaa": "1.0",
                    "type": "spec",
                    "domain": "TEST",
                    "input": {"SRC": {"path": path.name, "empty_string": "present"}},
                    "keys": ["ID"],
                    "columns": [
                        {"name": "ID", "type": "int", "derivation": "SRC.ID"},
                        *(
                            {"name": name, "type": "str", "derivation": "SRC." + name}
                            for name in fields
                        ),
                    ],
                    "output": {"path": "output.csv", "columns": ["ID", *fields]},
                }
                result = self.build(source, {})
                self.assertEqual(result.issues.num_rows, 0, result.issues)
                self.assertEqual(result.output.to_polars()["ID"].to_list(), ids)
                for name in fields:
                    self.assertEqual(
                        result.output.to_polars()[name].to_list(), text, name
                    )

    def test_parquet_more_row_groups_than_one_footer_numbers_read_every_record(self):
        # pyarrow and Polars with row_group_size=1 write one row group per
        # record. Past 32,768 row groups, Polars stops writing row-group
        # ordinals. Row-group sizing does not change the delivered records
        # (PARQUET-10), so every record reads in order (#2940).
        import polars as pl
        import pyarrow.parquet as pq

        rows = 32_769
        ids = list(range(rows))
        values = [None if row % 7 == 6 else row for row in ids]
        table = pa.table(
            {"ID": pa.array(ids, pa.int64()), "V": pa.array(values, pa.int64())}
        )
        writers = {
            "pyarrow": lambda path: pq.write_table(table, path, row_group_size=1),
            "polars": lambda path: pl.from_arrow(table).write_parquet(
                path, row_group_size=1
            ),
        }
        for writer, write in writers.items():
            with self.subTest(writer=writer):
                path = self.root / (writer + ".parquet")
                write(path)
                self.assertEqual(pq.ParquetFile(path).metadata.num_row_groups, rows)
                stored = pq.read_table(path)
                # The 32,769 values of each column are compared as a summary of
                # their first difference (#3882).
                self.assertIsNone(first_difference(stored["V"].to_pylist(), values))
                source = {
                    "yamaa": "1.0",
                    "type": "spec",
                    "domain": "TEST",
                    "input": {"SRC": path.name},
                    "keys": ["ID"],
                    "columns": [
                        {"name": "ID", "type": "int", "derivation": "SRC.ID"},
                        {"name": "V", "type": "int", "derivation": "SRC.V"},
                    ],
                    "output": {"path": "output.csv", "columns": ["ID", "V"]},
                }
                result = self.build(source, {})
                self.assertEqual(result.issues.num_rows, 0, result.issues)
                self.assertIsNone(
                    first_difference(result.output.to_polars()["ID"].to_list(), ids)
                )
                self.assertIsNone(
                    first_difference(result.output.to_polars()["V"].to_list(), values)
                )

    def test_parquet_signed_integer_fields_read_as_int(self):
        # pyarrow and Polars store int8 and int16 columns as INT32 annotated
        # Int(8 | 16, signed) and INT_8 | INT_16, and int32 columns as INT32
        # without an annotation. A signed integer field whose width fits its
        # physical type reads as int (PARQUET-7, #2940).
        import polars as pl
        import pyarrow.parquet as pq

        columns = {
            "I8": ([-128, None, 127, 0], pa.int8(), "INT_8"),
            "I16": ([-32768, None, 32767, 0], pa.int16(), "INT_16"),
            "I32": ([-2147483648, None, 2147483647, 0], pa.int32(), "NONE"),
        }
        table = pa.table(
            {
                "ID": pa.array([1, 2, 3, 4], pa.int64()),
                **{
                    name: pa.array(values, kind)
                    for name, (values, kind, _) in columns.items()
                },
            }
        )
        writers = {
            "pyarrow": lambda path: pq.write_table(table, path),
            "polars": lambda path: pl.from_arrow(table).write_parquet(path),
        }
        for writer, write in writers.items():
            with self.subTest(writer=writer):
                path = self.root / (writer + ".parquet")
                write(path)
                schema = pq.ParquetFile(path).schema
                for name, (_, _, converted) in columns.items():
                    column = schema.column(schema.names.index(name))
                    self.assertEqual(column.physical_type, "INT32")
                    self.assertEqual(column.converted_type, converted)
                names = ["ID", *columns]
                source = {
                    "yamaa": "1.0",
                    "type": "spec",
                    "domain": "TEST",
                    "input": {"SRC": path.name},
                    "keys": ["ID"],
                    "columns": [
                        {"name": name, "type": "int", "derivation": "SRC." + name}
                        for name in names
                    ],
                    "output": {"path": "output.csv", "columns": names},
                }
                result = self.build(source, {})
                self.assertEqual(result.issues.num_rows, 0, result.issues)
                for name, (values, _, _) in columns.items():
                    self.assertEqual(
                        result.output.to_polars()[name].to_list(), values, name
                    )
        # The staged pyarrow int32 fixture, which 0.4.0 refused, holds 1.
        source = {
            "yamaa": "1.0",
            "type": "spec",
            "domain": "TEST",
            "input": {"SRC": "input.parquet"},
            "keys": ["FIELD"],
            "columns": [{"name": "FIELD", "type": "int", "derivation": "SRC.FIELD"}],
            "output": {"path": "output.csv", "columns": ["FIELD"]},
        }
        result = self.build(
            source, {"input.parquet": ROOT / "pq" / "unsupported-int32.parquet"}
        )
        self.assertEqual(result.issues.num_rows, 0, result.issues)
        self.assertEqual(result.output.to_polars()["FIELD"].to_list(), [1])

    def test_parquet_semantic_failures_collect_in_source_order(self):
        aliases = tuple(TRUTH["input_aliases"])
        # Every specification is pruned after composition: keep every input live
        # through a row-phase default so ingest failures are still collected
        # instead of pruned away with unused inputs.
        source = {
            "yamaa": "1.0",
            "type": "spec",
            "domain": "TEST",
            "base": "FIRST",
            # A qualified read of an input matches on its declared key (D23,
            # D111): each input declares the output key.
            "input": {
                name: {"path": name + ".parquet", "key": ["ID"]} for name in aliases
            },
            "keys": ["ID"],
            "columns": [
                {
                    "name": "ID",
                    "type": "int",
                    "derivation": {
                        "first_available": {
                            "sources": [f"{name}.ID" for name in aliases]
                        }
                    },
                }
            ],
            "output": {"path": "output.csv", "columns": ["ID"]},
        }
        result = self.build(
            source,
            {
                name + ".parquet": ROOT / "pq" / (fixture + ".parquet")
                for name, fixture in zip(
                    aliases, ("utf8-time", "duplicate", "text"), strict=True
                )
            },
        )
        diagnostics = [
            {
                "phase": "ingest",
                "condition": "source_field_value_invalid",
                "rule": "PARQUET-9",
                "spec_paths": ["input.FIRST.path"],
                "context": {
                    "dataset": "FIRST",
                    "path": "FIRST.parquet",
                    "field": "OTHER",
                    "record": 1,
                    "value": 1,
                },
            },
            {
                "phase": "ingest",
                "condition": "source_field_name_duplicate",
                "rule": "PARQUET-6",
                "spec_paths": ["input.SECOND.path"],
                "context": {
                    "dataset": "SECOND",
                    "path": "SECOND.parquet",
                    "field": "I",
                },
            },
        ]
        self.assertEqual(
            self.observe(result),
            self.failed_report(diagnostics, [name + ".parquet" for name in aliases]),
        )
        self.assertIsNone(result.output)
        with self.assertRaises(yamaa.Error):
            result.save()
        self.assertFalse((self.root / "output.csv").exists())

    def test_original_ordered_sum_reads_parquet_and_preserves_complete_truth(self):
        name = "adam-adlb-ordered-sum"
        case = ROOT / "cases" / name
        source = (
            (case / "spec.yaml")
            .read_bytes()
            .replace(
                TRUTH["floating_input_override"].encode("utf-8"),
                b"input/lb.parquet",
            )
        )
        result = self.build(
            source, {"input/lb.parquet": ROOT / "pq/ordered-sum.parquet"}
        )
        expected = self.expected_original(name, artifacts=True)
        for read in expected["source_reads"]:
            read["path"] = "input/lb.parquet"
        self.assertIsNone(
            first_difference(self.observe(result), dict(expected, artifacts=[]))
        )
        self.assertIsNotNone(result.output)
        self.entry.unlink()
        (self.root / "input/lb.parquet").unlink()
        gc.collect()
        for _ in range(2):
            result.save()
            self.assertEqual(
                (self.root / "adlb.csv").read_bytes(),
                (case / "expected/adlb.csv").read_bytes(),
            )
            self.assertIsNone(first_difference(self.observe(result), expected))

    def test_parquet_declarations_fail_before_any_host_source_authority(self):
        source = (
            (ROOT / "cases/adam-adlb-ordered-sum/spec.yaml")
            .read_bytes()
            .replace(b"input/lb.csv", b"input/lb.parquet")
        )
        self.entry.write_bytes(source)
        expected = [
            {
                "phase": "validation",
                "condition": "redundant_field_type",
                "rule": "INPUT-6",
                "spec_paths": ["input.LB.types.LBSTRESN"],
                "context": {"dataset": "LB", "field": "LBSTRESN", "type": "float"},
            }
        ]
        specification = yamaa.spec(self.entry)
        for result in (specification.check(), specification.run()):
            rows = result.issues.to_polars().select(FIELDS).to_dicts()
            for row in rows:
                row["context"] = json.loads(row["context"])
            self.assertEqual(rows, expected)
        self.assertFalse((self.root / "input").exists())

    def test_parquet_ingestion_failures_retain_exact_conditions_and_never_publish(self):
        # Every specification is pruned after composition: keep the input live
        # via base so the ingest failure is still exercised instead of being
        # pruned away with an unused input.
        entry = (
            b'yamaa: "1.0"\ntype: spec\ndomain: TEST\nbase: SRC\n'
            b"input: {SRC: input.parquet}\nkeys: [ID]\n"
            b'columns:\n  - {name: ID, type: int, derivation: "1"}\n'
            b"output: {path: output.csv, columns: [ID]}\n"
        )
        for name, condition, rule, context in (
            ("utf8", "source_parquet_invalid", "PARQUET-5", {}),
            (
                "utf8-bool",
                "source_field_type_unsupported",
                "PARQUET-7",
                {"field": "OTHER", "stored_type": "bool"},
            ),
            (
                "utf8-time",
                "source_field_value_invalid",
                "PARQUET-9",
                {"field": "OTHER", "record": 1, "value": 1},
            ),
            ("empty-name", "source_field_name_empty", "PARQUET-6", {"field": 1}),
            ("duplicate", "source_field_name_duplicate", "PARQUET-6", {"field": "I"}),
            (
                "mixed-struct",
                "source_field_type_unsupported",
                "PARQUET-7",
                {"field": "S", "stored_type": "struct<left: int64, right: string>"},
            ),
            (
                "mixed-map",
                "source_field_type_unsupported",
                "PARQUET-7",
                {"field": "M", "stored_type": "map<string, int64 ('M')>"},
            ),
            ("mixed-empty", "source_field_name_empty", "PARQUET-6", {"field": 1}),
            (
                "mixed-duplicate",
                "source_field_name_duplicate",
                "PARQUET-6",
                {"field": "I"},
            ),
            *(
                (
                    "unsupported-" + name,
                    "source_field_type_unsupported",
                    "PARQUET-7",
                    {"field": "FIELD", "stored_type": stored},
                )
                for name, stored in (
                    ("uint64", "uint64"),
                    ("binary", "binary"),
                    ("timezone", "timestamp[us, tz=UTC]"),
                    ("list", "list<element: int64>"),
                    ("fixed-list", "fixed_size_list<element: int64>[2]"),
                    ("struct", "struct<item: int64>"),
                    ("decimal", "decimal128(10, 2)"),
                    # Types that the pinned Arrow and Parquet releases do not
                    # know (#3155).
                    ("decimal32", "decimal32(9, 2)"),
                    ("decimal64", "decimal64(18, 2)"),
                    ("list-view", "list_view<element: int64>"),
                    ("large-list-view", "large_list_view<element: int64>"),
                    ("geometry", "binary"),
                    ("geography", "binary"),
                    (
                        "variant",
                        "struct<metadata: binary not null, value: binary not null>",
                    ),
                )
            ),
        ):
            with self.subTest(case=name):
                result = self.build(
                    entry, {"input.parquet": ROOT / "pq" / (name + ".parquet")}
                )
                diagnostics = [
                    {
                        "phase": "ingest",
                        "condition": condition,
                        "rule": rule,
                        "spec_paths": ["input.SRC.path"],
                        "context": dict(dataset="SRC", path="input.parquet", **context),
                    }
                ]
                self.assertEqual(
                    self.observe(result),
                    self.failed_report(diagnostics, ["input.parquet"]),
                )
                self.assertIsNone(result.output)
                with self.assertRaises(yamaa.Error):
                    result.save()
                self.assertFalse((self.root / "output.csv").exists())

    def test_original_build_saves_parquet_with_independent_logical_readback(self):
        name = "adam-adlb-ordered-sum"
        case = ROOT / "cases" / name
        raw = (case / "spec.yaml").read_bytes()
        source = raw.replace(b"path: adlb.csv", b"path: adlb.parquet")
        self.assertNotEqual(source, raw)
        expected = self.expected_original(name, artifacts=True)
        result = self.build(source, {"input/lb.csv": case / "input/lb.csv"})
        self.assertIsNone(
            first_difference(self.observe(result), dict(expected, artifacts=[]))
        )
        self.assertIsNotNone(result.output)
        result.save()
        report = self.observe(result)
        self.assertIsNone(
            first_difference(dict(report, artifacts=[]), dict(expected, artifacts=[]))
        )

        import pyarrow.parquet as pq

        reader = pq.ParquetFile(self.root / "adlb.parquet")
        artifact = expected["artifacts"][0]
        derived = next(t for t in expected["tables"] if t["stage"] == "derived")
        self.assertEqual(reader.schema_arrow.names, artifact["columns"])
        self.assertIsNone(reader.metadata.metadata)
        self.assertEqual(reader.metadata.num_rows, artifact["row_count"])
        expected_types = {"str": ("BYTE_ARRAY", "STRING"), "float": ("DOUBLE", "NONE")}
        for column, kind in enumerate(artifact["types"]):
            physical, logical = expected_types[kind]
            self.assertEqual(reader.schema.column(column).physical_type, physical)
            self.assertEqual(reader.schema.column(column).logical_type.type, logical)
        for column in range(reader.metadata.num_columns):
            self.assertEqual(reader.schema.column(column).max_definition_level, 1)
        actual = reader.read().to_pylist()
        projection = [derived["columns"].index(c) for c in artifact["columns"]]
        for row, truth in zip(actual, derived["rows"], strict=True):
            for column, index in zip(artifact["columns"], projection, strict=True):
                cell = truth[index]
                value = row[column]
                if cell["type"] == "float":
                    value = struct.pack(">d", value).hex()
                elif cell["type"] == "int":
                    value = str(value)
                self.assertEqual(value, cell["value"])
        observed = report["artifacts"][0]
        self.assertEqual(observed["profile"], "parquet")
        self.assertEqual(observed["content"], "")
        self.assertEqual(
            observed["byte_length"], (self.root / "adlb.parquet").stat().st_size
        )
        logical = [json.dumps(artifact["columns"], ensure_ascii=True)]
        logical.extend(
            json.dumps([row[c] for c in artifact["columns"]], ensure_ascii=True)
            for row in actual
        )
        self.assertEqual(observed["records"], logical)

    def test_closed_parquet_types_and_exact_values_with_independent_reader(self):
        kinds = {
            "ID": "str",
            "S": "str",
            "I": "int",
            "F": "float",
            "D": "date",
            "T": "datetime",
        }
        projection = ["T", "I", "S", "F", "D", "EMPTY"]
        specification = {
            "yamaa": "1.0",
            "type": "spec",
            "domain": "TEST",
            "keys": ["I"],
            "input": {"SOURCE": {"path": "input.csv", "types": kinds}},
            "output": {"path": "output.parquet", "columns": projection},
            "columns": [
                {
                    "name": name,
                    "type": kind,
                    "label": name,
                    "derivation": "SOURCE." + name,
                }
                for name, kind in kinds.items()
            ]
            + [{"name": "EMPTY", "type": "str", "label": "Empty text"}],
            "rows": [{"id": "all", "derivations": {"EMPTY": "''"}}],
        }
        data = (
            "ID,S,I,F,D,T\n"
            "a,,\u002d9223372036854775808,-0.0,0001-01-01,1969-12-31T23:59:59\n"
            "b,\u00e9\U0001f642,9223372036854775807,5e-324,9999-12-31,9999-12-31T23:59:59\n"
            "c,text,9007199254740993,1.2345678901234567,,\n"
        ).encode("utf-8")
        result = self.build(specification, {"input.csv": data})
        self.assertIsNotNone(result.output, result.issues)
        result.save()
        report = self.observe(result)
        import datetime

        import pyarrow.parquet as pq

        reader = pq.ParquetFile(self.root / "output.parquet")
        self.assertEqual(
            reader.schema_arrow,
            pa.schema(
                [
                    pa.field("T", pa.timestamp("us")),
                    pa.field("I", pa.int64()),
                    pa.field("S", pa.string()),
                    pa.field("F", pa.float64()),
                    pa.field("D", pa.date32()),
                    pa.field("EMPTY", pa.string()),
                ]
            ),
        )
        self.assertIsNone(reader.metadata.metadata)
        self.assertEqual(reader.metadata.num_rows, 3)
        self.assertEqual(
            [reader.schema.column(i).physical_type for i in range(6)],
            ["INT64", "INT64", "BYTE_ARRAY", "DOUBLE", "INT32", "BYTE_ARRAY"],
        )
        timestamp = json.loads(reader.schema.column(0).logical_type.to_json())
        self.assertEqual(timestamp["Type"], "Timestamp")
        self.assertIs(timestamp["isAdjustedToUTC"], False)
        self.assertEqual(timestamp["timeUnit"], "microseconds")
        for i in range(6):
            self.assertEqual(reader.schema.column(i).max_definition_level, 1)
        rows = reader.read().to_pylist()
        self.assertEqual([row["I"] for row in rows], [-(2**63), 2**63 - 1, 2**53 + 1])
        self.assertEqual([row["S"] for row in rows], [None, "\u00e9\U0001f642", "text"])
        self.assertEqual([row["EMPTY"] for row in rows], ["", "", ""])
        self.assertEqual(
            [struct.pack(">d", row["F"]).hex() for row in rows],
            ["0000000000000000", "0000000000000001", "3ff3c0ca428c59fb"],
        )
        self.assertEqual(
            [row["D"] for row in rows],
            [datetime.date(1, 1, 1), datetime.date(9999, 12, 31), None],
        )
        self.assertEqual(
            [row["T"] for row in rows],
            [
                datetime.datetime(1969, 12, 31, 23, 59, 59),
                datetime.datetime(9999, 12, 31, 23, 59, 59),
                None,
            ],
        )
        logical = [json.dumps(projection)]
        logical.extend(
            json.dumps(
                [row[name] for name in projection],
                default=lambda value: value.isoformat(),
            )
            for row in rows
        )
        self.assertEqual(report["artifacts"][0]["records"], logical)


if __name__ == "__main__":
    unittest.main()
