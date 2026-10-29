"""Installed original-YAML execution with reference semantic imports forbidden."""

import builtins
import csv
import gc
import json
import os
import shutil
import struct
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import pyarrow as pa
from host_fixtures import first_difference
from host_fixtures import load as load_truth

import yamaa
from yamaa import _native as yamaa_native

TRUTH = load_truth(__file__, "original-specifications")

ROOT = Path(__file__).with_name("specification-original")
CASES = tuple(TRUTH["original_case_names"])
# The public issues table (D28): the five fields the authored findings state
# sit among the severity, the location, the message and the expression
# geometry, which no finding here authors.
FIELDS = ("phase", "condition", "rule", "spec_paths", "context")
GEOMETRY = ("source_span", "operand_route")
ISSUE_COLUMNS = [
    "severity",
    "phase",
    "condition",
    "rule",
    "spec_paths",
    "file",
    "line",
    "column",
    "message",
    "context",
    *GEOMETRY,
]


def issue_schema():
    """The issues table's Polars schema, in column order."""
    import polars as pl

    return {
        "severity": pl.String,
        "phase": pl.String,
        "condition": pl.String,
        "rule": pl.String,
        "spec_paths": pl.List(pl.String),
        "file": pl.String,
        "line": pl.Int64,
        "column": pl.Int64,
        "message": pl.String,
        "context": pl.String,
        "source_span": pl.List(pl.Int64),
        "operand_route": pl.List(pl.String),
    }


def fixture_bytes(value):
    return (
        bytes.fromhex(value["hex"])
        if isinstance(value, dict)
        else value.encode("utf-8")
    )


def without_layer(context):
    """A finding's context without the layer a refused layer names. A schema
    finding names the files the run loaded, `source` and `entry` together."""
    if "entry" in context:
        context = {k: v for k, v in context.items() if k not in ("entry", "source")}
    return context


def error_lines(issue):
    """The lines of an expected/error.yaml that states `issue` exactly, in
    any order: its fields, each path as an item and each context entry, with
    a list in flow style. Read without a YAML parser, as this suite reads
    every original (#3422)."""
    lines = {
        f"phase: {issue['phase']}",
        f"condition: {issue['condition']}",
        f"rule: {issue['rule']}",
        "spec_paths:",
        *(f"  - {path}" for path in issue["spec_paths"]),
    }
    if issue["context"]:
        lines.add("context:")
    for key, value in issue["context"].items():
        text = f"[{', '.join(value)}]" if isinstance(value, list) else value
        lines.add(f"  {key}: {text}")
    return lines


def specification_name(name):
    return "spec_study.yaml" if name == "schema-inheritance" else "spec.yaml"


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
        for group in range(reader.metadata.num_row_groups):
            for column in range(reader.metadata.num_columns):
                self.assertEqual(
                    reader.metadata.row_group(group).column(column).compression,
                    "UNCOMPRESSED",
                )
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
            self.assertEqual(
                reader.metadata.row_group(0).column(i).compression, "UNCOMPRESSED"
            )
        rows = reader.read().to_pylist()
        self.assertEqual([row["I"] for row in rows], [-(2**63), 2**63 - 1, 2**53 + 1])
        self.assertEqual([row["S"] for row in rows], [None, "\u00e9\U0001f642", "text"])
        self.assertEqual([row["EMPTY"] for row in rows], ["", "", ""])
        self.assertEqual(
            [struct.pack(">d", row["F"]).hex() for row in rows],
            ["8000000000000000", "0000000000000001", "3ff3c0ca428c59fb"],
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


class PublicDomains(unittest.TestCase):
    def test_deferred_filter_types_remain_visible_without_selector_observations(self):
        import yamaa

        reference = json.loads(
            (ROOT / "key-reference.json").read_text(encoding="utf-8")
        )
        type_findings = [
            {
                "phase": "validation",
                "condition": "incompatible_input_type",
                "rule": "FORMULA-5",
                "spec_paths": ["intermediates.LOOK.filter"],
                # The right operand's type is the one not accepted (FORMULA-5).
                "context": {"actual": "str", "left_type": "int", "right_type": "str"},
            }
        ]

        def findings(result):
            rows = result.issues.to_polars().select(FIELDS).to_dicts()
            for row in rows:
                row["context"] = json.loads(row["context"])
            return rows

        for variant in (
            "duplicate",
            "resolved",
            "empty_donor",
            "empty_driver",
            "missing_filter_operand",
            "unused",
            "unread_branch",
            "boolean_missing",
        ):
            with (
                self.subTest(variant=variant),
                tempfile.TemporaryDirectory() as directory,
            ):
                case = json.loads(json.dumps(reference["dependency_before_bad_filter"]))
                if variant != "duplicate":
                    case["inputs"]["src.csv"] = "ID,N\na,7\n"
                if variant == "empty_donor":
                    case["inputs"]["tab.csv"] = "K,KINT,KNUM,VAL\n"
                elif variant == "empty_driver":
                    case["inputs"]["src.csv"] = "ID,N\n"
                elif variant in ("missing_filter_operand", "boolean_missing"):
                    case["inputs"]["tab.csv"] = 'K,KINT,KNUM,VAL\n"       7",,7,A\n'
                elif variant == "unused":
                    case["specification"] = case["specification"].replace(
                        "derivation: LOOK.VAL", "derivation: \"'ok'\""
                    )
                elif variant == "unread_branch":
                    case["specification"] = case["specification"].replace(
                        "derivation: LOOK.VAL",
                        "derivation: {case: [{when: 'FALSE', then: LOOK.VAL}, {otherwise: \"'ok'\"}]}",
                    )
                if variant == "boolean_missing":
                    case["specification"] = case["specification"].replace(
                        "TAB.KINT > 'bad'", "FALSE AND TAB.KINT > 'bad'"
                    )
                work = Path(directory)
                entry = work / "spec.yaml"
                entry.write_text(case["specification"], encoding="utf-8")
                for path, content in case["inputs"].items():
                    (work / path).write_text(content, encoding="utf-8")
                result = yamaa.spec(entry).run()
                if variant == "unused":
                    # Every specification is pruned after composition (LAYER-11,
                    # LAYER-12): no column reads LOOK, so pruning discards the
                    # intermediate together with its ill-typed filter and the
                    # specification builds with no issues instead of refusing
                    # the dead declaration.
                    self.assertEqual(findings(result), [])
                    self.assertIsNotNone(result.output)
                    result.save()
                    self.assertEqual(
                        (work / "out.csv").read_bytes(), b"ID,V,V2\na,ok,ok\n"
                    )
                    for path in case["inputs"]:
                        (work / path).unlink()
                    self.assertEqual(findings(yamaa.spec(entry).check()), [])
                    continue
                expected = case["findings"] if variant == "duplicate" else type_findings
                self.assertEqual(findings(result), expected)
                self.assertIsNone(result.output)
                for _ in range(2):
                    with self.assertRaises(yamaa.Error):
                        result.save()
                self.assertFalse((work / "out.csv").exists())
                for path in case["inputs"]:
                    (work / path).unlink()
                # Declared kinds remain available to check with no study files.
                self.assertEqual(findings(yamaa.spec(entry).check()), type_findings)

    def test_public_inheritance_introducing_nulls_and_inherited_clears(self):
        import yamaa

        for case in TRUTH["inheritance_nulls"]:
            with (
                self.subTest(case=case["name"]),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                entry = root / "spec.yaml"
                entry.write_text(case["spec"], encoding="utf-8")
                for name, content in case["parents"].items():
                    (root / name).write_text(content, encoding="utf-8")
                checked = yamaa.spec(entry).check()
                conditions = checked.issues.to_polars()["condition"].to_list()
                if case["check_condition"] is None:
                    self.assertEqual(conditions, [])
                else:
                    self.assertEqual(conditions[0], case["check_condition"])
                for name, content in case["inputs"].items():
                    (root / name).write_text(content, encoding="utf-8")
                result = yamaa.spec(entry).run()
                conditions = result.issues.to_polars()["condition"].to_list()
                if case["run_condition"] is None:
                    self.assertEqual(conditions, [])
                    result.save()
                    self.assertEqual(
                        (root / "result.csv").read_bytes(), case["csv"].encode("utf-8")
                    )
                else:
                    self.assertEqual(conditions[0], case["run_condition"])
                    self.assertIsNone(result.output)
                observations = result._native.observations()
                if observations is not None:
                    # An intermediate's absence counts under `absent` (D202).
                    counts = json.loads(observations)["handler_counts"]
                    self.assertEqual(
                        sum(
                            item["count"]
                            for item in counts
                            if item["handler"] == "absent"
                        ),
                        case["absent_count"],
                    )

    def test_public_temporal_arithmetic_conformance(self):
        import yamaa

        def assert_issues(frame, expected):
            if expected is None:
                self.assertEqual(frame.num_rows, 0)
            else:
                self.assertTrue(
                    any(
                        row["phase"] == expected["phase"]
                        and row["condition"] == expected["condition"]
                        and row["rule"] == expected["rule"]
                        and row["spec_paths"] == [expected["spec_path"]]
                        for row in frame.to_polars().select(FIELDS).to_dicts()
                    ),
                    frame,
                )

        for case in TRUTH["temporal_arithmetic"]:
            with (
                self.subTest(case=case["name"]),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                entry = root / "spec.yaml"
                entry.write_text(case["spec"], encoding="utf-8")
                assert_issues(yamaa.spec(entry).check().issues, case["check_issue"])
                for name, content in case["inputs"].items():
                    (root / name).write_text(content, encoding="utf-8")
                result = yamaa.spec(entry).run()
                assert_issues(result.issues, case["run_issue"])
                if case["csv"] is not None:
                    result.save()
                    self.assertEqual(
                        (root / "result.csv").read_bytes(), case["csv"].encode("utf-8")
                    )
                else:
                    self.assertIsNone(result.output)
                    with self.assertRaises(yamaa.Error):
                        result.save()
                    self.assertFalse((root / "result.csv").exists())
                observations = result._native.observations()
                if observations is not None:
                    counts = json.loads(observations)["handler_counts"]
                    self.assertEqual(
                        sum(
                            item["count"]
                            for item in counts
                            if item["handler"] == "invalid"
                        ),
                        case["invalid_count"],
                    )

    def test_public_text_positions_match_the_shared_truth(self):
        import yamaa

        def rows(frame):
            """The issue rows as the shared truth writes them."""
            found = frame.to_polars().select(FIELDS).to_dicts()
            for row in found:
                row["context"] = json.loads(row["context"])
            return found

        for case in TRUTH["text_positions"]:
            with (
                self.subTest(case=case["name"]),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                entry = root / "spec.yaml"
                entry.write_bytes(case["spec"].encode("utf-8"))
                expected = case["issues"] or []
                checked = yamaa.spec(entry).check()
                self.assertEqual(rows(checked.issues), expected, "the check")
                for name, content in case["inputs"].items():
                    (root / name).write_bytes(content.encode("utf-8"))
                result = yamaa.spec(entry).run()
                self.assertEqual(rows(result.issues), expected, "the run")
                if case["csv"] is None:
                    self.assertIsNone(result.output)
                    with self.assertRaises(yamaa.Error):
                        result.save()
                    self.assertFalse((root / "result.csv").exists())
                else:
                    result.save()
                    self.assertEqual(
                        (root / "result.csv").read_bytes(), case["csv"].encode("utf-8")
                    )
                observations = result._native.observations()
                if observations is not None:
                    counts = json.loads(observations)["handler_counts"]
                    self.assertEqual(
                        sum(
                            item["count"]
                            for item in counts
                            if item["handler"] == "missing"
                        ),
                        case["missing_count"],
                        "the missing answers",
                    )

    def test_public_typed_intermediate_derivations(self):
        import yamaa

        for case in TRUTH["typed_intermediates"]:
            with (
                self.subTest(case=case["name"]),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                entry = root / "spec.yaml"
                entry.write_text(case["spec"], encoding="utf-8")
                # Static findings must precede access to absent study data.
                checked = yamaa.spec(entry).check()
                conditions = checked.issues.to_polars()["condition"].to_list()
                if case["check_condition"] is None:
                    self.assertEqual(conditions, [])
                else:
                    self.assertIn(case["check_condition"], conditions)
                for name, content in case["inputs"].items():
                    (root / name).write_text(content, encoding="utf-8")
                result = yamaa.spec(entry).run()
                conditions = result.issues.to_polars()["condition"].to_list()
                if case["run_condition"] is None:
                    self.assertEqual(conditions, [])
                    result.save()
                    self.assertEqual(
                        (root / "result.csv").read_bytes(), case["csv"].encode("utf-8")
                    )
                else:
                    self.assertIn(case["run_condition"], conditions)
                    self.assertIsNone(result.output)
                observations = result._native.observations()
                if observations is not None:
                    counts = json.loads(observations)["handler_counts"]
                    reached = sum(
                        item["count"]
                        for item in counts
                        if item["handler"] == "unconvertible"
                    )
                    self.assertEqual(reached, case["unconvertible_count"])

    def record_public_report(self, result, name, entry):
        destination = os.environ.get("YAMAA_PUBLIC_REPORT_DIR")
        if not destination:
            return
        report = json.loads(result._native.observations())
        self.assertEqual(report["example"], "domain")
        report["example"] = name
        # Canonicalize report labels only after the original complete report
        # comparison. Diagnostic paths, contexts and resource reads are retained.
        for group in ("nodes", "tables", "verifications", "callbacks"):
            for record in report[group]:
                self.assertEqual(record["specification"], str(entry))
                record["specification"] = entry.name
        path = Path(destination)
        path.mkdir(parents=True, exist_ok=True)
        (path / (name + ".python.rust.json")).write_text(
            json.dumps(report, sort_keys=True, separators=(",", ":")) + "\n",
            encoding="utf-8",
        )

    def setUp(self):
        original = builtins.__import__
        blocked = (
            "yamaa._reference_domain",
            "yamaa.runtime",
            "yamaa.planning",
            "yamaa.expressions",
            "yamaa.specification",
            "yamaa.schema",
            "yamaa.verification",
            "yamaa.functions",
            "yamaa.models",
        )

        def reject(name, *args, **kwargs):
            if name in {"yaml", "yaml12", "pydantic"} or any(
                name == p or name.startswith(p + ".") for p in blocked
            ):
                raise AssertionError("reference semantic import: " + name)
            return original(name, *args, **kwargs)

        guard = patch("builtins.__import__", side_effect=reject)
        guard.start()
        self.addCleanup(guard.stop)

    def test_public_declarative_rounding_original_yaml_and_independent_csv(self):
        import shutil

        import polars as pl
        from polars.testing import assert_frame_equal

        import yamaa

        with tempfile.TemporaryDirectory() as directory:
            case = Path(directory) / "positive"
            shutil.copytree(
                ROOT / "cases" / "schema-round-half-away-from-zero-ties", case
            )
            entry = case / "spec.yaml"
            written = entry.read_bytes()
            source = case / "input" / "adsl.csv"
            captured = source.read_bytes()
            source.unlink()
            self.assertEqual(yamaa.spec(entry).check().issues.num_rows, 0)
            source.write_bytes(captured)
            result = yamaa.spec(entry).run()
            self.assertEqual(result.issues.num_rows, 0)
            types = {name: pl.Float64 for name in ("VAL1", "VAL0", "RND1", "RND0")}
            expected = pl.read_csv(
                case / "expected" / "adsl.csv", schema_overrides=types
            )
            self.assertEqual(expected.height, 16)
            assert_frame_equal(result.output.to_polars(), expected)
            self.assertEqual(entry.read_bytes(), written)
            self.assertFalse((case / "adsl.csv").exists())
            result.save()
            assert_frame_equal(
                pl.read_csv(case / "adsl.csv", schema_overrides=types), expected
            )
            negative = Path(directory) / "negative"
            shutil.copytree(
                ROOT / "cases" / "negative-rounding-source",
                negative,
            )
            shutil.rmtree(negative / "input")
            # This literal is pinned to the independent original error fixture,
            # not a candidate-generated report or formula diagnostic.
            self.assertEqual(
                (negative / "expected" / "error.yaml").read_text(),
                "phase: validation\ncondition: incompatible_input_type\nspec_paths:\n"
                "  - columns.RNDVAL.derivation.round_half_away_from_zero.source\n"
                "rule: FORMULA-16\ncontext:\n  source: SEX\n  expected: numeric\n  actual: str\n",
            )
            expected_issue = {
                "phase": "validation",
                "condition": "incompatible_input_type",
                "rule": "FORMULA-16",
                "spec_paths": [
                    "columns.RNDVAL.derivation.round_half_away_from_zero.source"
                ],
                "context": {"source": "SEX", "expected": "numeric", "actual": "str"},
            }

            def issue(result):
                rows = result.issues.to_polars().select(FIELDS).to_dicts()
                self.assertEqual(len(rows), 1)
                rows[0]["context"] = json.loads(rows[0]["context"])
                return rows[0]

            self.assertEqual(
                issue(yamaa.spec(negative / "spec.yaml").check()), expected_issue
            )
            rejected = yamaa.spec(negative / "spec.yaml").run()
            self.assertIsNone(rejected.output)
            self.assertIsNone(rejected._native.observations())
            self.assertEqual(issue(rejected), expected_issue)
            with self.assertRaises(yamaa.Error):
                rejected.save()
            self.assertFalse((negative / "adsl.csv").exists())

    def test_public_complete_date_operations_keep_original_outputs_and_static_findings(
        self,
    ):
        import shutil

        import polars as pl
        from polars.testing import assert_frame_equal

        import yamaa

        with tempfile.TemporaryDirectory() as directory:
            for name, dates in [
                ("adam-adsl-duration", ["STDT", "ENDT"]),
                ("adam-adsl-analysis-age", ["BRTHDT", "RANDDT"]),
            ]:
                case = Path(directory) / name
                shutil.copytree(ROOT / "cases" / name, case)
                entry = case / "spec.yaml"
                written = entry.read_bytes()
                input_dir = case / "input"
                held = case / "held"
                input_dir.rename(held)
                self.assertEqual(yamaa.spec(entry).check().issues.num_rows, 0)
                held.rename(input_dir)
                result = yamaa.spec(entry).run()
                self.assertEqual(result.issues.num_rows, 0)
                expected = pl.read_csv(
                    case / "expected/adsl.csv",
                    schema_overrides={name: pl.Date for name in dates},
                )
                assert_frame_equal(result.output.to_polars(), expected)
                self.assertEqual(entry.read_bytes(), written)
                self.assertFalse((case / "adsl.csv").exists())
                result.save()
                self.assertEqual(
                    (case / "adsl.csv").read_bytes(),
                    (case / "expected/adsl.csv").read_bytes(),
                )
            for name, rule in [
                ("negative-date-diff-endpoints", "TIME-1"),
                ("negative-study-day-datetime", "TIME-1"),
                ("negative-epoch-day-non-date", "TIME-1"),
                ("negative-date-diff-units", "TIME-5"),
            ]:
                case = Path(directory) / name
                shutil.copytree(ROOT / "cases" / name, case)
                shutil.rmtree(case / "input")
                # Each benchmark states its one complete finding in its
                # expected/error.yaml, in the language it is written in
                # (#3422); its lines are the finding's, in any order.
                stated = set((case / "expected/error.yaml").read_text().splitlines())
                for result in [
                    yamaa.spec(case / "spec.yaml").check(),
                    yamaa.spec(case / "spec.yaml").run(),
                ]:
                    issues = result.issues.to_polars().select(FIELDS).to_dicts()
                    self.assertEqual(len(issues), 1)
                    issues[0]["context"] = json.loads(issues[0]["context"])
                    self.assertEqual(
                        (issues[0]["phase"], issues[0]["rule"]), ("validation", rule)
                    )
                    self.assertEqual(error_lines(issues[0]), stated)
                self.assertIsNone(result.output)
                self.assertIsNone(result._native.observations())
                with self.assertRaises(yamaa.Error):
                    result.save()
                self.assertFalse((case / "input").exists())

    def test_public_temporal_imputation_originals_and_complete_failures(self):
        import shutil

        import polars as pl
        from polars.testing import assert_frame_equal

        import yamaa

        def issues(result):
            records = result.issues.to_polars().select(FIELDS).to_dicts()
            for record in records:
                record["context"] = without_layer(json.loads(record["context"]))
            return records

        with tempfile.TemporaryDirectory() as directory:
            for name, output, dates, integers, datetimes in [
                (
                    "schema-birth-date",
                    "adsl.csv",
                    TRUTH["partial_date_columns"],
                    ["DOBDY"],
                    ["DTM"],
                ),
                ("sdtm-dm-age", "dm.csv", ["RFICDTC"], ["AGE"], []),
            ]:
                case = Path(directory) / name
                shutil.copytree(ROOT / "cases" / name, case)
                entry = case / "spec.yaml"
                written = entry.read_bytes()
                source = case / "input"
                held = case / "held"
                source.rename(held)
                self.assertEqual(issues(yamaa.spec(entry).check()), [])
                held.rename(source)
                result = yamaa.spec(entry).run()
                self.assertEqual(issues(result), [])
                expected = pl.read_csv(case / "expected" / output, infer_schema=False)
                for column in dates:
                    expected = expected.with_columns(pl.col(column).str.to_date())
                for column in datetimes:
                    expected = expected.with_columns(
                        pl.col(column).str.to_datetime(time_unit="ms")
                    )
                for column in integers:
                    expected = expected.with_columns(pl.col(column).cast(pl.Int64))
                assert_frame_equal(result.output.to_polars(), expected)
                self.assertEqual(entry.read_bytes(), written)
                self.assertFalse((case / output).exists())
                result.save()
                self.assertEqual(
                    (case / output).read_bytes(),
                    (case / "expected" / output).read_bytes(),
                )
            with (ROOT / "temporal-imputation-diagnostics.tsv").open() as stream:
                failures = list(csv.DictReader(stream, delimiter="\t"))
            self.assertEqual(len(failures), 8)
            for finding in failures:
                case = Path(directory) / finding["case"]
                shutil.copytree(ROOT / "cases" / finding["case"], case)
                entry = case / "spec.yaml"
                expected = {key: finding[key] for key in ("phase", "condition", "rule")}
                expected.update(
                    spec_paths=[finding["spec_path"]],
                    context=json.loads(finding["context"]),
                )
                static = finding["phase"] == "validation"
                if static:
                    shutil.rmtree(case / "input")
                checked = yamaa.spec(entry).check()
                self.assertEqual(issues(checked), [expected] if static else [])
                result = yamaa.spec(entry).run()
                self.assertEqual(issues(result), [expected])
                self.assertIsNone(result.output)
                self.assertEqual(result._native.observations() is None, static)
                with self.assertRaises(yamaa.Error):
                    result.save()
                self.assertEqual(sorted(p.name for p in case.glob("*.csv")), [])

    def test_public_string_case_modes_original_negatives_and_ordered_original(self):
        import shutil

        import yamaa

        def issues(result):
            records = result.issues.to_polars().select(FIELDS).to_dicts()
            for record in records:
                record["context"] = without_layer(json.loads(record["context"]))
            return records

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = TRUTH["unicode_case_yaml"]
            entry = root / "spec.yaml"
            entry.write_text(source)
            self.assertEqual(issues(yamaa.spec(entry).check()), [])
            (root / "input.csv").write_text(
                "ID,TEXT\n1,FOLLOW-UP\n2,\u00df\u0130\u00e9\U0001f642AbC\n3,\n",
                encoding="utf-8",
                newline="\n",
            )
            result = yamaa.spec(entry).run()
            self.assertEqual(issues(result), [])
            self.assertEqual(
                result.output.to_polars().rows(),
                [
                    ("1", "FOLLOW-UP", "follow-up", "Follow-up", "Follow-Up"),
                    (
                        "2",
                        "\u00df\u0130\u00e9\U0001f642ABC",
                        "\u00df\u0130\u00e9\U0001f642abc",
                        "\u00df\u0130\u00e9\U0001f642abc",
                        "\u00df\u0130\u00e9\U0001f642Abc",
                    ),
                    ("3", "MiXeD", None, None, None),
                ],
            )
            self.assertFalse((root / "output.csv").exists())
            result.save()
            self.assertEqual(
                (root / "output.csv").read_bytes(), TRUTH["unicode_case_csv"].encode()
            )
            source = source.replace("to: upper, missing: MiXeD", "to: upper")
            entry.write_text(source)
            (root / "input.csv").write_bytes(b"ID,TEXT\nrow1,\n")
            # Without `missing`, a missing source gives missing (COLUMN-10).
            answered = yamaa.spec(entry).run()
            expected = json.loads((ROOT / "text-case-reference.json").read_text())[
                "missing"
            ]
            self.assertEqual(issues(answered), expected)
            self.assertEqual(
                answered.output.to_polars().rows(), [("row1", None, None, None, None)]
            )
            with (ROOT / "text-case-diagnostics.tsv").open() as stream:
                failures = list(csv.DictReader(stream, delimiter="\t"))
            self.assertEqual(len(failures), 2)
            for finding in failures:
                case = root / finding["case"]
                shutil.copytree(ROOT / "cases" / finding["case"], case)
                shutil.rmtree(case / "input")
                expected = {key: finding[key] for key in ("phase", "condition", "rule")}
                expected.update(
                    spec_paths=[finding["spec_path"]],
                    context=json.loads(finding["context"]),
                )
                checked = yamaa.spec(case / "spec.yaml").check()
                rejected = yamaa.spec(case / "spec.yaml").run()
                self.assertEqual(issues(checked), [expected])
                self.assertEqual(issues(rejected), [expected])
                self.assertIsNone(rejected.output)
                self.assertIsNone(rejected._native.observations())
                with self.assertRaises(yamaa.Error):
                    rejected.save()
                self.assertFalse((case / "input").exists())
            case = root / "schema-sentence-case"
            shutil.copytree(ROOT / "cases" / case.name, case)
            self.assertEqual(issues(yamaa.spec(case / "spec.yaml").check()), [])
            result = yamaa.spec(case / "spec.yaml").run()
            self.assertEqual(issues(result), [])
            self.assertIsNotNone(result.output)
            self.assertFalse((case / "adlb.csv").exists())
            result.save()
            self.assertEqual(
                (case / "adlb.csv").read_bytes(),
                (case / "expected/adlb.csv").read_bytes(),
            )

    def test_public_source_positions_raw_filter_and_internal_presentation_order(self):
        import yamaa

        def issues(result):
            records = result.issues.to_polars().select(FIELDS).to_dicts()
            for record in records:
                record["context"] = json.loads(record["context"])
            return records

        source = """yamaa: "1.0"
type: spec
domain: TEST
keys: [ID]
input: {SRC: {path: input.csv, ordinal: REC}}
filter: "SRC.KEEP = 'Y'"
output: {path: output.csv, columns: [ID, POSITION], order_by: [SORT]}
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: POSITION, type: int, derivation: SRC.REC}
  - {name: SORT, type: int, derivation: SRC.SORT}
"""
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            entry, data, output = (
                root / "spec.yaml",
                root / "input.csv",
                root / "output.csv",
            )
            entry.write_text(source, encoding="ascii")
            for content, rows, expected in [
                (
                    TRUTH["filtered_multiline_csv"].encode("utf-8"),
                    [("c", 3), ("b", 2)],
                    b"ID,POSITION\nc,3\nb,2\n",
                ),
                (b"ID,SORT,KEEP,NOTE\n", [], b"ID,POSITION\n"),
            ]:
                data.write_bytes(content)
                self.assertEqual(issues(yamaa.spec(entry).check()), [])
                result = yamaa.spec(entry).run()
                self.assertEqual(issues(result), [])
                self.assertEqual(result.output.to_polars().rows(), rows)
                self.assertFalse(output.exists())
                data.unlink()
                for _ in range(2):
                    result.save()
                    self.assertEqual(output.read_bytes(), expected)
                output.unlink()
            data.write_bytes(b"ID,REC,SORT,KEEP\n")
            result = yamaa.spec(entry).run()
            self.assertEqual(
                issues(result),
                [
                    {
                        "phase": "validation",
                        "condition": "ordinal_field_collision",
                        "rule": "INPUT-4",
                        "spec_paths": ["input.SRC.ordinal"],
                        "context": {"dataset": "SRC", "field": "REC"},
                    }
                ],
            )
            self.assertIsNone(result.output)
            with self.assertRaises(yamaa.Error):
                result.save()
            self.assertFalse(output.exists())

    def test_public_string_padding_complete_independent_outputs_findings_and_effects(
        self,
    ):
        import shutil

        import yamaa

        reference = json.loads(
            (ROOT / "text-pad-reference.json").read_text(encoding="utf-8")
        )

        def issues(result):
            records = result.issues.to_polars().select(FIELDS).to_dicts()
            for record in records:
                record["context"] = without_layer(json.loads(record["context"]))
            return records

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, case in reference.items():
                current = root / name
                shutil.copytree(ROOT / "text-pad-cases" / name, current)
                entry = current / "spec.yaml"
                if name == "width-noninteger":
                    (current / "input.csv").unlink()
                result = yamaa.spec(entry).run()
                if "csv" in case:
                    self.assertEqual(issues(result), [], name)
                    self.assertEqual(
                        result.output.to_polars().write_csv().encode(),
                        case["csv"].encode(),
                        name,
                    )
                    self.assertFalse((current / "output.csv").exists())
                    self.assertEqual(
                        json.loads(result._native.observations())["handler_counts"],
                        case["handlers"],
                        name,
                    )
                    result.save()
                    self.assertEqual(
                        (current / "output.csv").read_bytes(),
                        case["csv"].encode(),
                        name,
                    )
                else:
                    expected = json.loads(json.dumps(case["findings"]))
                    if name == "width-noninteger":
                        # Preserve complete loader evidence and expose the existing
                        # schema source/entry gap separately from normative equality.
                        for field in ("source", "entry"):
                            self.assertEqual(
                                expected[0]["context"].pop(field),
                                "/private/tmp/yamaa-root-text-pad-audit/width-noninteger/spec.yaml",
                            )
                        self.assertEqual(issues(yamaa.spec(entry).check()), expected)
                        self.assertIsNone(result._native.observations())
                    else:
                        self.assertEqual(
                            json.loads(result._native.observations())["handler_counts"],
                            case["handlers"],
                            name,
                        )
                    self.assertEqual(issues(result), expected, name)
                    self.assertIsNone(result.output)
                    with self.assertRaises(yamaa.Error):
                        result.save()
                    self.assertFalse((current / "output.csv").exists())
            original = root / "negative-str-pad-width"
            shutil.copytree(ROOT / "cases" / original.name, original)
            shutil.rmtree(original / "input")
            expected = [
                {
                    "phase": "validation",
                    "condition": "invalid_field_type",
                    "rule": "SCHEMA-12",
                    "spec_paths": ["columns.IDVARVAL.derivation.str_pad.width"],
                    "context": {"expected": "int", "actual": "str"},
                }
            ]
            for result in (
                yamaa.spec(original / "spec.yaml").check(),
                yamaa.spec(original / "spec.yaml").run(),
            ):
                self.assertEqual(issues(result), expected)
            self.assertFalse((original / "input").exists())

    def test_public_declarative_flags_complete_original_and_authored_captures(self):
        import shutil

        import yamaa

        reference = json.loads(
            (ROOT / "flag-reference.json").read_text(encoding="utf-8")
        )
        positives = (
            "schema-str-contains",
            "adam-adae-occurrence-flags",
            "adam-adex-dose-reduction",
            "adam-adlb-closest-visit",
            "adam-adsl-completion-flag",
            "sdtm-dm-death",
        )
        negatives = (
            "negative-flag-condition",
            "negative-flag-missing-value",
            "negative-flag-payload",
            "negative-review-arithmetic",
            "negative-review-date-text",
            "negative-review-unknown-date",
        )
        outputs = dict(
            zip(
                positives,
                ("adae.csv", "adae.csv", "adex.csv", "adlb.csv", "adsl.csv", "dm.csv"),
            )
        )
        # Complete captures remain installed for both existing independent gates.
        held = ("nested_case_mixed_declarations", "implicit_raw_ambiguous")
        self.assertEqual(set(held).intersection(reference["witnesses"]), set(held))
        cases = [
            (name, reference["originals"][name], "cases")
            for name in positives + negatives
        ]
        cases += [
            (name, case, "flags")
            for name, case in reference["witnesses"].items()
            if name not in held
        ]
        self.assertEqual(len(cases), 33)

        def issues(result):
            rows = result.issues.to_polars().select(FIELDS).to_dicts()
            for row in rows:
                row["context"] = json.loads(row["context"])
            return rows

        with tempfile.TemporaryDirectory() as directory:
            for name, case, cohort in cases:
                with self.subTest(case=name):
                    work = Path(directory) / name
                    shutil.copytree(ROOT / cohort / name, work)
                    entry = work / "spec.yaml"
                    if cohort == "flags":
                        # A witness is staged from its capture. A benchmark
                        # runs as it is written, in 1.0, whose findings and
                        # handlers are those of its 0.5 capture (#3422) but
                        # for an intermediate's absence, `absent` (D202).
                        self.assertEqual(
                            entry.read_text(encoding="utf-8"), case["specification"]
                        )
                    retained = {
                        path: path.read_bytes()
                        for path in work.rglob("*")
                        if path.is_file()
                    }
                    expected = json.loads(json.dumps(case["findings"]))
                    schema_failure = case["result_type"] == "SpecificationError"
                    if schema_failure:
                        # Full loader provenance remains in the unchanged capture;
                        # only these two existing absent native fields are excepted.
                        for finding in expected:
                            source = finding["context"].pop("source")
                            self.assertEqual(source, finding["context"].pop("entry"))
                            self.assertTrue(source.endswith("/" + name + "/spec.yaml"))
                    output = work / outputs.get(name, "out.csv")
                    for attempt in range(2):
                        with self.subTest(attempt=attempt):
                            self.assertFalse(output.exists())
                            result = yamaa.spec(entry).run()
                            found = issues(result)
                            if schema_failure:
                                # A schema finding names the files this run
                                # loaded.
                                for finding in found:
                                    finding["context"].pop("source", None)
                                    finding["context"].pop("entry", None)
                            self.assertEqual(found, expected)
                            wire = result._native.observations()
                            if wire is None:
                                self.assertEqual(case.get("handlers", []), [])
                            else:
                                self.assertEqual(
                                    json.loads(wire)["handler_counts"],
                                    case.get("handlers", []),
                                )
                            if case["result_type"] == "ExecutionSuccess":
                                expected_path = (
                                    next((work / "expected").glob("*.csv"))
                                    if cohort == "cases"
                                    else work / "expected.csv"
                                )
                                if cohort == "flags":
                                    self.assertEqual(
                                        expected_path.read_bytes(),
                                        case["csv"].encode(),
                                    )
                                    self.assertEqual(
                                        result.output.to_polars().write_csv().encode(),
                                        expected_path.read_bytes(),
                                    )
                                result.save()
                                self.assertEqual(
                                    output.read_bytes(), expected_path.read_bytes()
                                )
                                output.unlink()
                            else:
                                self.assertIsNone(result.output)
                                with self.assertRaises(yamaa.Error):
                                    result.save()
                            self.assertFalse(output.exists())
                            for path, original in retained.items():
                                self.assertEqual(path.read_bytes(), original)

    def test_public_intermediate_keys_preserve_complete_nineteen_captures(self):
        import shutil

        import yamaa

        reference = json.loads(
            (ROOT / "key-reference.json").read_text(encoding="utf-8")
        )
        self.assertEqual(len(reference), 19)
        with tempfile.TemporaryDirectory() as directory:
            for name, case in reference.items():
                with self.subTest(case=name):
                    work = Path(directory) / name
                    shutil.copytree(ROOT / "keys" / name, work)
                    entry = work / "spec.yaml"
                    self.assertEqual(entry.read_bytes(), case["specification"].encode())
                    retained = {
                        path: path.read_bytes()
                        for path in work.rglob("*")
                        if path.is_file()
                    }
                    output = work / "out.csv"
                    for attempt in range(2):
                        with self.subTest(attempt=attempt):
                            self.assertFalse(output.exists())
                            result = yamaa.spec(entry).run()
                            findings = (
                                result.issues.to_polars().select(FIELDS).to_dicts()
                            )
                            for finding in findings:
                                finding["context"] = json.loads(finding["context"])
                            self.assertEqual(findings, case.get("findings", []))
                            wire = result._native.observations()
                            if wire is None:
                                self.assertEqual(case["handlers"], [])
                            else:
                                self.assertEqual(
                                    json.loads(wire)["handler_counts"], case["handlers"]
                                )
                            if "csv" in case:
                                expected = case["csv"].encode()
                                self.assertEqual(
                                    (work / "expected.csv").read_bytes(), expected
                                )
                                self.assertEqual(
                                    result.output.to_polars().write_csv().encode(),
                                    expected,
                                )
                                result.save()
                                self.assertEqual(output.read_bytes(), expected)
                                output.unlink()
                            else:
                                self.assertIsNone(result.output)
                                with self.assertRaises(yamaa.Error):
                                    result.save()
                            self.assertFalse(output.exists())
                            for path, original in retained.items():
                                self.assertEqual(path.read_bytes(), original)

    def test_public_common_row_intermediates_complete_original_and_independent_truth(
        self,
    ):
        import shutil

        import yamaa

        reference = json.loads(
            (ROOT / "row-intermediate-reference.json").read_text(encoding="ascii")
        )
        cases = dict(reference["cases"], **reference["originals"])

        def issues(result):
            records = result.issues.to_polars().select(FIELDS).to_dicts()
            for record in records:
                record["context"] = json.loads(record["context"])
            return records

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, case in cases.items():
                with self.subTest(name=name):
                    current = root / name
                    shutil.copytree(ROOT / "row-cases" / name, current)
                    entry = current / "spec.yaml"
                    expected = json.loads(
                        (current / "findings.json").read_text(encoding="ascii")
                    )
                    if name == "different-row-drivers":
                        # Stored drivers now use the complete historical success witness.
                        self.assertEqual(expected, case["findings"])
                        self.assertEqual(
                            (current / "expected.csv").read_bytes(),
                            case["csv"].encode(),
                        )
                    if name == "adam-adlb-end-of-treatment":
                        self.assertEqual(expected, case["findings"])
                        self.assertEqual(
                            (current / "expected.csv").read_bytes(),
                            case["expected_csv"].encode(),
                        )
                    result = yamaa.spec(entry).run()
                    self.assertEqual(issues(result), expected)
                    observations = result._native.observations()
                    if observations is not None:
                        self.assertEqual(
                            json.loads(observations)["handler_counts"],
                            case.get("handler_counts", []),
                        )
                    output = current / (
                        "adlb.csv" if name in reference["originals"] else "result.csv"
                    )
                    self.assertFalse(output.exists())
                    if not expected:
                        self.assertIsNotNone(result.output)
                        expected_csv = (current / "expected.csv").read_bytes()
                        if name == "adam-adlb-end-of-treatment":
                            expected_columns = (
                                case["expected_csv"].splitlines()[0].split(",")
                            )
                            self.assertEqual(
                                result.output.column_names, expected_columns
                            )
                            self.assertEqual(
                                result.output.to_polars().rows(),
                                [
                                    tuple(row[column] for column in expected_columns)
                                    for row in case["rows"]
                                ],
                            )
                        else:
                            self.assertEqual(
                                result.output.to_polars().write_csv().encode(),
                                expected_csv,
                            )
                        result.save()
                        self.assertEqual(output.read_bytes(), expected_csv)
                        result.save()
                        self.assertEqual(output.read_bytes(), expected_csv)
                    else:
                        self.assertIsNone(result.output)
                        with self.assertRaises(yamaa.Error):
                            result.save()
                        self.assertFalse(output.exists())
                    if name in tuple(TRUTH["row_driver_cases"]):
                        self.assertEqual(issues(yamaa.spec(entry).check()), expected)
            # Public Check admits metadata without inspecting missing study files.
            original = root / "adam-adlb-supplb-padded-key"
            shutil.rmtree(original / "input")
            self.assertEqual(issues(yamaa.spec(original / "spec.yaml").check()), [])
            self.assertFalse((original / "input").exists())

    def test_public_text_template_original_bytes_and_complete_reference_witnesses(self):
        import shutil

        import polars as pl
        from polars.testing import assert_frame_equal

        import yamaa

        truth = json.loads((ROOT / "text-template-reference.json").read_text())
        with (ROOT / "text-template-directories.tsv").open(
            newline="", encoding="ascii"
        ) as stream:
            directories = {
                row["case"]: row["directory"]
                for row in csv.DictReader(stream, delimiter="\t")
            }

        def issues(result):
            records = result.issues.to_polars().select(FIELDS).to_dicts()
            for record in records:
                record["context"] = json.loads(record["context"])
            return records

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            case = root / "adam-adsl-site-parse"
            shutil.copytree(ROOT / "cases" / case.name, case)
            entry = case / "spec.yaml"
            written = entry.read_bytes()
            held = case / "held"
            (case / "input").rename(held)
            self.assertEqual(issues(yamaa.spec(entry).check()), [])
            held.rename(case / "input")
            result = yamaa.spec(entry).run()
            self.assertEqual(issues(result), [])
            expected = pl.read_csv(case / "expected/adsl.csv", infer_schema=False)
            assert_frame_equal(result.output.to_polars(), expected)
            self.assertEqual(
                json.loads(result._native.observations())["handler_counts"],
                truth[case.name]["handler_counts"],
            )
            self.assertEqual(entry.read_bytes(), written)
            self.assertFalse((case / "adsl.csv").exists())
            result.save()
            self.assertEqual(
                (case / "adsl.csv").read_bytes(),
                (case / "expected/adsl.csv").read_bytes(),
            )
            self.assertEqual(
                (case / "adsl.csv").read_bytes(),
                truth[case.name]["csv"].encode("utf-8"),
            )
            negative = root / "negative-subject-reference"
            shutil.copytree(ROOT / "cases" / negative.name, negative)
            shutil.rmtree(negative / "input")
            for result in [
                yamaa.spec(negative / "spec.yaml").check(),
                yamaa.spec(negative / "spec.yaml").run(),
            ]:
                self.assertEqual(issues(result), truth[negative.name]["findings"])
            self.assertIsNone(result.output)
            self.assertIsNone(result._native.observations())
            with self.assertRaises(yamaa.Error):
                result.save()
            for name, value in truth.items():
                if "specification" not in value:
                    continue
                case = root / name
                shutil.copytree(ROOT / "text-template-cases" / directories[name], case)
                result = yamaa.spec(case / "spec.yaml").run()
                # `{entry}` stands for the specification's absolute path as
                # the engine resolves it, with forward slashes.
                resolved = (case / "spec.yaml").resolve().as_posix()
                expected = [
                    {
                        **finding,
                        "context": {
                            key: resolved if item == "{entry}" else item
                            for key, item in finding["context"].items()
                        },
                    }
                    for finding in value.get("findings", [])
                ]
                self.assertEqual(issues(result), expected, name)
                observations = result._native.observations()
                if observations is not None and "handler_counts" in value:
                    self.assertEqual(
                        json.loads(observations)["handler_counts"],
                        value["handler_counts"],
                        name,
                    )
                if "csv" in value:
                    self.assertFalse((case / "output.csv").exists())
                    result.save()
                    self.assertEqual(
                        (case / "output.csv").read_bytes(),
                        value["csv"].encode("utf-8"),
                        name,
                    )
                else:
                    self.assertIsNone(result.output)
                    with self.assertRaises(yamaa.Error):
                        result.save()
                    self.assertFalse((case / "output.csv").exists())

    def test_public_text_extract_original_bytes_and_complete_reference_witnesses(self):
        import shutil

        import polars as pl
        from polars.testing import assert_frame_equal

        import yamaa

        truth = json.loads((ROOT / "text-extract-reference.json").read_text())

        def issues(result):
            records = result.issues.to_polars().select(FIELDS).to_dicts()
            for record in records:
                record["context"] = json.loads(record["context"])
            return records

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            case = root / "adam-adae-text-cleanup"
            shutil.copytree(ROOT / "cases" / case.name, case)
            entry = case / "spec.yaml"
            written = entry.read_bytes()
            held = case / "held"
            (case / "input").rename(held)
            self.assertEqual(issues(yamaa.spec(entry).check()), [])
            held.rename(case / "input")
            result = yamaa.spec(entry).run()
            self.assertEqual(issues(result), [])
            expected = pl.read_csv(
                case / "expected/adae.csv",
                schema_overrides={"AESEQ": pl.Int64, "AEREFNUM": pl.Int64},
            )
            assert_frame_equal(result.output.to_polars(), expected)
            self.assertEqual(
                json.loads(result._native.observations())["handler_counts"],
                truth[case.name]["handler_counts"],
            )
            self.assertEqual(entry.read_bytes(), written)
            self.assertFalse((case / "adae.csv").exists())
            result.save()
            self.assertEqual(
                (case / "adae.csv").read_bytes(),
                (case / "expected/adae.csv").read_bytes(),
            )
            self.assertEqual(
                (case / "adae.csv").read_bytes(), truth[case.name]["csv"].encode()
            )
            negative = root / "negative-str-uncaptured-group"
            shutil.copytree(ROOT / "cases" / negative.name, negative)
            shutil.rmtree(negative / "input")
            for result in [
                yamaa.spec(negative / "spec.yaml").check(),
                yamaa.spec(negative / "spec.yaml").run(),
            ]:
                self.assertEqual(issues(result), truth[negative.name]["findings"])
            self.assertIsNone(result.output)
            self.assertIsNone(result._native.observations())
            with self.assertRaises(yamaa.Error):
                result.save()
            for name in (
                "missing",
                "no-match",
                "non-string",
                "invalid-pattern",
                "negative-group",
                "optional-unentered",
                "entered-empty",
                "whole-empty",
                "leftmost-alternative",
                "last-repeated-capture",
                "literal-null",
                "one-final-conversion",
                "combined-nonstring-group",
                "combined-unknown-group",
                "combined-nonstring-invalid-pattern",
                "combined-unknown-invalid-pattern",
                "nested-case",
                "row-extract",
                "scope-before-pattern",
            ):
                case = root / name
                shutil.copytree(ROOT / "text-extract-cases" / name, case)
                result = yamaa.spec(case / "spec.yaml").run()
                self.assertEqual(issues(result), truth[name].get("findings", []), name)
                observations = result._native.observations()
                if observations is not None and "handler_counts" in truth[name]:
                    self.assertEqual(
                        json.loads(observations)["handler_counts"],
                        truth[name]["handler_counts"],
                        name,
                    )
                if "csv" in truth[name]:
                    self.assertFalse((case / "output.csv").exists())
                    result.save()
                    self.assertEqual(
                        (case / "output.csv").read_bytes(),
                        truth[name]["csv"].encode(),
                        name,
                    )
                    if name == "optional-unentered":
                        self.assertEqual(
                            result.output.to_polars()["RESULT"].to_list(), [None]
                        )
                    if name in ("entered-empty", "whole-empty"):
                        self.assertEqual(
                            result.output.to_polars()["RESULT"].to_list(), [""]
                        )
                else:
                    self.assertIsNone(result.output)
                    with self.assertRaises(yamaa.Error):
                        result.save()

    def test_public_domain_all_original_issues_outputs_and_saved_bytes(self):
        import datetime
        import shutil

        import polars as pl

        import yamaa

        self.assertTrue(callable(yamaa.spec))
        for name in CASES:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                case = Path(directory) / "case"
                shutil.copytree(ROOT / "cases" / name, case)
                entry = case / specification_name(name)
                expected = json.loads(
                    (ROOT / "expected" / (name + ".json")).read_text()
                )
                result = yamaa.spec(entry).run()
                observed_expected = json.loads(json.dumps(expected))
                observed_expected.update(
                    runtime="python",
                    runtime_version=sys.version,
                    engine_version=yamaa_native._CORE_VERSION,
                    example="domain",
                )
                for group in ("nodes", "tables", "verifications", "callbacks"):
                    for record in observed_expected[group]:
                        record["specification"] = str(entry)
                unsaved = dict(observed_expected, artifacts=[])
                self.assertIsNone(
                    first_difference(json.loads(result._native.observations()), unsaved)
                )
                # The issues carry the authored diagnostics whole, geometry
                # included where the failure site recorded it.
                expected_rows = [
                    (
                        r["phase"],
                        r["condition"],
                        r["rule"],
                        r["spec_paths"],
                        json.dumps(
                            r["context"],
                            ensure_ascii=False,
                            sort_keys=True,
                            separators=(",", ":"),
                        ),
                        None
                        if "source_span" not in r
                        else [r["source_span"]["start"], r["source_span"]["end"]],
                        r.get("operand_route"),
                    )
                    for r in expected["diagnostics"]
                ]
                self.assertEqual(
                    result.issues.to_polars().select([*FIELDS, *GEOMETRY]).rows(),
                    expected_rows,
                )
                self.assertEqual(result.issues.to_polars().schema, issue_schema())
                # Every issue is an error with a message, located in a file the
                # case read or, when no file holds its first spec path, not at
                # all (P-ISSUE-1).
                for row in result.issues.to_pylist():
                    self.assertEqual(row["severity"], "error")
                    located = row["file"] is not None
                    self.assertEqual(
                        (located, row["line"] is not None, row["column"] is not None),
                        (located,) * 3,
                    )
                    self.assertTrue(row["message"])
                # The run report records the same outcome as the complete truth.
                self.assertEqual(
                    result.report.to_polars().row(0, named=True)["OUTCOME"],
                    "succeeded" if expected["outcome"] == "success" else "failed",
                )
                if not expected["artifacts"]:
                    self.assertIsNone(result.output)
                    with self.assertRaises(yamaa.Error):
                        result.save()
                    self.assertIsNone(
                        first_difference(
                            json.loads(result._native.observations()), unsaved
                        )
                    )
                    self.record_public_report(result, name, entry)
                    continue
                artifact = expected["artifacts"][0]
                derived = next(t for t in expected["tables"] if t["stage"] == "derived")
                projection = [derived["columns"].index(c) for c in artifact["columns"]]

                def scalar(value):
                    kind, data = value["type"], value["value"]
                    if kind == "missing":
                        return None
                    if kind == "int":
                        return int(data)
                    if kind == "float":
                        return struct.unpack(">d", bytes.fromhex(data))[0]
                    if kind == "date":
                        return datetime.date.fromisoformat(data)
                    if kind == "datetime":
                        return datetime.datetime.fromisoformat(data)
                    return data

                rows = [
                    tuple(scalar(row[column]) for column in projection)
                    for row in derived["rows"]
                ]
                self.assertEqual(result.output.column_names, artifact["columns"])
                self.assertEqual(result.output.to_polars().rows(), rows)
                expected_types = {
                    "str": pl.String,
                    "int": pl.Int64,
                    "float": pl.Float64,
                    "date": pl.Date,
                }
                for column, kind in zip(artifact["columns"], artifact["types"]):
                    self.assertEqual(
                        result.output.to_polars().schema[column], expected_types[kind]
                    )
                output_name = {
                    "schema-lookup": "adsl.csv",
                    "schema-window-functions": "advs.csv",
                    "schema-inheritance": "adlb.csv",
                    "adam-adlb-ordered-sum": "adlb.csv",
                }[name]
                saved = case / output_name
                original = (case / "expected" / output_name).read_bytes()
                for _ in range(2):
                    result.save()
                    self.assertEqual(saved.read_bytes(), original)
                    self.assertIsNone(
                        first_difference(
                            json.loads(result._native.observations()),
                            observed_expected,
                        )
                    )
                self.assertEqual(result.output.to_polars().rows(), rows)
                self.record_public_report(result, name, entry)

    def test_public_check_without_study_and_save_retry(self):
        import yamaa

        source = 'yamaa: "1.0"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: absent/output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n'
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            entry = root / "spec.yaml"
            entry.write_text(source)
            self.assertEqual(yamaa.spec(entry).check().issues.num_rows, 0)
            failed = yamaa.spec(entry).run()
            self.assertIsNone(failed.output)
            self.assertEqual(
                failed.issues.to_polars()["condition"].to_list(),
                ["resource_path_missing"],
            )
            with self.assertRaises(yamaa.Error):
                failed.save()
            (root / "input.csv").write_bytes(
                TRUTH["integer_boundary_csv"].encode("utf-8")
            )
            built = yamaa.spec(entry).run()
            self.assertEqual(
                built.output.to_polars()["ID"].to_list(), [-(2**63), 2**63 - 1]
            )
            (root / "input.csv").write_bytes(b"changed")
            entry.write_bytes(b"[changed")
            with self.assertRaises(yamaa.Error):
                built.save()
            # The output directory does not exist (FILE-7).
            self.assertEqual(
                built.issues.to_polars()["condition"].to_list(),
                ["resource_path_missing"],
            )
            (root / "absent").mkdir()
            built.save()
            self.assertEqual(built.issues.num_rows, 0)
            self.assertEqual(
                (root / "absent/output.csv").read_bytes(),
                TRUTH["integer_boundary_csv"].encode("utf-8"),
            )
        for late in (False, True):
            for existing in (False, True):
                with (
                    tempfile.TemporaryDirectory() as directory,
                    tempfile.TemporaryDirectory() as external,
                ):
                    root, outside = Path(directory), Path(external)
                    entry = root / "spec.yaml"
                    entry.write_text(
                        source.replace("absent/output.csv", "link/output.csv")
                    )
                    (root / "input.csv").write_bytes(b"ID\n1\n")
                    target = outside / "output.csv"
                    if existing:
                        target.write_bytes(b"retained")
                    link = root / "link"
                    if late:
                        link.mkdir()
                    else:
                        link.symlink_to(outside, target_is_directory=True)
                    built = yamaa.spec(entry).run()
                    self.assertEqual(built.output.to_polars()["ID"].to_list(), [1])
                    if late:
                        link.rmdir()
                        link.symlink_to(outside, target_is_directory=True)
                    with self.assertRaises(yamaa.Error):
                        built.save()
                    # A link on the way to the target (FILE-8).
                    self.assertEqual(
                        built.issues.to_polars()["condition"].to_list(),
                        ["resource_path_symlink"],
                    )
                    self.assertEqual(
                        json.loads(built.issues.to_polars()["context"][0]),
                        {"path": "link/output.csv"},
                    )
                    self.assertEqual(
                        sorted(p.name for p in outside.iterdir()),
                        ["output.csv"] if existing else [],
                    )
                    if existing:
                        self.assertEqual(target.read_bytes(), b"retained")
                    link.unlink()
                    link.mkdir()
                    built.save()
                    self.assertEqual(built.issues.num_rows, 0)
                    self.assertEqual((link / "output.csv").read_bytes(), b"ID\n1\n")

    def test_public_project_refusal_and_argument_types(self):
        import yamaa

        project = yamaa.project("absent-project.yaml")
        # The caller's own path, named as given at no field (FILE-7).
        expected = [
            {
                "phase": "validation",
                "condition": "resource_path_missing",
                "rule": "FILE-7",
                "spec_paths": [],
                "context": json.dumps(
                    {"path": "absent-project.yaml"},
                    ensure_ascii=False,
                    separators=(",", ":"),
                ),
            }
        ]
        result = project.run()
        # Nothing ran: no specification and no report rows.
        self.assertEqual(result.names, [])
        self.assertEqual(result.report.num_rows, 0)
        self.assertEqual(result.issues.to_polars().select(FIELDS).to_dicts(), expected)
        self.assertEqual(
            project.check().issues.to_polars().rows(), result.issues.to_polars().rows()
        )
        with self.assertRaises(yamaa.Error) as raised:
            project.spec("TEST")
        self.assertEqual(
            raised.exception.issues.to_polars().select(FIELDS).to_dicts(), expected
        )
        with self.assertRaises(yamaa.Error):
            result.save()
        for function in (yamaa.spec, yamaa.project):
            with self.assertRaises(yamaa.Error):
                function(None)

    def test_public_self_donors_use_only_completed_templates_and_preserve_full_phase_refusal(
        self,
    ):
        import yamaa

        body = TRUTH["section_order_yaml"]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            entry = root / "spec.yaml"
            entry.write_text(body)
            (root / "src.csv").write_text("ID,K,SEQ,V\na,x,2,5\nb,x,1,9\n")
            (root / "other.csv").write_text("ID,K\nc,x\nd,x\n")
            result = yamaa.spec(entry).run()
            self.assertEqual(result.issues.to_polars().rows(), [])
            self.assertEqual(
                json.loads(result._native.observations())["handler_counts"],
                [
                    {
                        "spec_path": "rows.grouped.derivations.V.multiple_matches",
                        "handler": "multiple_matches",
                        "count": 1,
                    },
                    {
                        "spec_path": "rows.grouped.derivations.V.absent",
                        "handler": "absent",
                        "count": 0,
                    },
                    {
                        "spec_path": "rows.added.derivations.V.multiple_matches",
                        "handler": "multiple_matches",
                        "count": 2,
                    },
                    {
                        "spec_path": "rows.added.derivations.V.absent",
                        "handler": "absent",
                        "count": 0,
                    },
                    {
                        "spec_path": "columns.FINAL.derivation.source.multiple_matches",
                        "handler": "multiple_matches",
                        "count": 5,
                    },
                    {
                        "spec_path": "columns.FINAL.derivation.source.absent",
                        "handler": "absent",
                        "count": 0,
                    },
                    {
                        "spec_path": "columns.FIRST.derivation.source.multiple_matches",
                        "handler": "multiple_matches",
                        "count": 5,
                    },
                    {
                        "spec_path": "columns.FIRST.derivation.source.absent",
                        "handler": "absent",
                        "count": 0,
                    },
                ],
            )
            result.save()
            self.assertEqual(
                (root / "result.csv").read_bytes(),
                TRUTH["section_order_csv"].encode("utf-8"),
            )
            for name in tuple(TRUTH["section_case_files"]):
                (root / name).unlink()
            entry.write_text(body.replace("SECTION: \"'first'\"", "SECTION: P.K"))
            for checked in (yamaa.spec(entry).check(), yamaa.spec(entry).run()):
                records = checked.issues.to_polars().select(FIELDS).to_dicts()
                for record in records:
                    record["context"] = json.loads(record["context"])
                self.assertEqual(
                    records,
                    [
                        {
                            "phase": "validation",
                            "condition": "phase_boundary",
                            "rule": "LOOKUP-7",
                            "spec_paths": ["rows.collected.derivations.SECTION.source"],
                            "context": {
                                "identifier": "P.K",
                                "available_phase": "completed_prior_row_template",
                                "required_phase": "row_construction",
                            },
                        }
                    ],
                )
            rejected = yamaa.spec(entry).run()
            self.assertIsNone(rejected.output)
            self.assertIsNone(rejected._native.observations())
            with self.assertRaises(yamaa.Error):
                rejected.save()
            self.assertFalse((root / "src.csv").exists())
            self.assertFalse((root / "result.csv").exists())

    def test_public_intermediate_applicable_keys_and_complete_declaration_findings(
        self,
    ):
        import yamaa

        body = TRUTH["duplicate_lookup_yaml"]

        def findings(result):
            return [
                dict(
                    phase=phase,
                    condition=condition,
                    rule=rule,
                    spec_paths=paths,
                    context=json.loads(context),
                )
                for phase, condition, rule, paths, context in result.issues.to_polars()
                .select(FIELDS)
                .rows()
            ]

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            entry = root / "spec.yaml"
            entry.write_text(body)
            (root / "src.csv").write_text("ID\n2\n1\n3\n")
            (root / "donor.csv").write_text(TRUTH["duplicate_donor_csv"])
            result = yamaa.spec(entry).run()
            self.assertEqual(findings(result), [])
            self.assertEqual(
                json.loads(result._native.observations())["handler_counts"],
                [
                    {
                        "spec_path": "columns.A.derivation.source.multiple_matches",
                        "handler": "multiple_matches",
                        "count": 1,
                    },
                    {
                        "spec_path": "columns.A.derivation.source.absent",
                        "handler": "absent",
                        "count": 1,
                    },
                    {
                        "spec_path": "columns.B.derivation.source.multiple_matches",
                        "handler": "multiple_matches",
                        "count": 1,
                    },
                    {
                        "spec_path": "columns.B.derivation.source.absent",
                        "handler": "absent",
                        "count": 1,
                    },
                ],
            )
            result.save()
            self.assertEqual(
                (root / "result.csv").read_bytes(),
                TRUTH["duplicate_lookup_csv"].encode("utf-8"),
            )
        for remove, condition, context in [
            (
                TRUTH["lookup_first_clause"],
                "rename_only_intermediate",
                {"intermediate": "P", "dataset": "DONOR"},
            ),
            (
                "    keep: first\n",
                "unpaired_fields",
                {"intermediate": "P", "declared": ["order_by"], "missing": ["keep"]},
            ),
            (
                "    order_by: [DONOR.SEQ]\n",
                "unpaired_fields",
                {"intermediate": "P", "declared": ["keep"], "missing": ["order_by"]},
            ),
        ]:
            with (
                self.subTest(condition=condition, context=context),
                tempfile.TemporaryDirectory() as directory,
            ):
                entry = Path(directory) / "spec.yaml"
                entry.write_text(body.replace(remove, ""))
                expected = [
                    {
                        "phase": "validation",
                        "condition": condition,
                        "rule": "LOOKUP-4"
                        if condition == "rename_only_intermediate"
                        else "LOOKUP-10",
                        "spec_paths": ["intermediates.P"],
                        "context": context,
                    }
                ]
                self.assertEqual(findings(yamaa.spec(entry).check()), expected)
                result = yamaa.spec(entry).run()
                self.assertEqual(findings(result), expected)
                self.assertIsNone(result.output)
                self.assertIsNone(result._native.observations())
                with self.assertRaises(yamaa.Error):
                    result.save()
                self.assertFalse((entry.parent / "src.csv").exists())

    def test_public_primary_donor_selection_retains_one_record_and_complete_handlers(
        self,
    ):
        import yamaa

        body = TRUTH["lookup_filter_yaml"]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            entry = root / "spec.yaml"
            entry.write_text(body)
            (root / "src.csv").write_text(TRUTH["filtered_donor_csv"])
            self.assertEqual(yamaa.spec(entry).check().issues.num_rows, 0)
            result = yamaa.spec(entry).run()
            self.assertEqual(result.issues.to_polars().rows(), [])
            self.assertEqual(
                json.loads(result._native.observations())["handler_counts"],
                [
                    {
                        "spec_path": "columns.A.derivation.source.multiple_matches",
                        "handler": "multiple_matches",
                        "count": 2,
                    },
                    {
                        "spec_path": "columns.A.derivation.source.absent",
                        "handler": "absent",
                        "count": 1,
                    },
                    {
                        "spec_path": "columns.B.derivation.source.multiple_matches",
                        "handler": "multiple_matches",
                        "count": 2,
                    },
                    {
                        "spec_path": "columns.B.derivation.source.absent",
                        "handler": "absent",
                        "count": 1,
                    },
                ],
            )
            result.save()
            self.assertEqual(
                (root / "result.csv").read_bytes(),
                TRUTH["filtered_lookup_csv"].encode("utf-8"),
            )

    def test_public_correlated_intermediate_filters_preserve_current_driver_and_complete_handlers(
        self,
    ):
        import yamaa

        inputs = {
            "src.csv": "ID,SEQ,CUT\n1,1,2\n1,2,4\n3,1,2\n1,3,\n",
            "donor.csv": TRUTH["ordered_donor_csv"],
            "alt.csv": "ID,SEQ,CUT\n1,1,4\n3,1,2\n",
        }
        probes = [
            (
                TRUTH["lookup_cutoff_yaml"],
                TRUTH["lookup_cutoff_csv"].encode("utf-8"),
                [
                    {
                        "spec_path": "columns.A.derivation.source.multiple_matches",
                        "handler": "multiple_matches",
                        "count": 1,
                    },
                    {
                        "spec_path": "columns.A.derivation.source.absent",
                        "handler": "absent",
                        "count": 2,
                    },
                    {
                        "spec_path": "columns.B.derivation.source.multiple_matches",
                        "handler": "multiple_matches",
                        "count": 1,
                    },
                    {
                        "spec_path": "columns.B.derivation.source.absent",
                        "handler": "absent",
                        "count": 2,
                    },
                ],
            ),
            (
                TRUTH["lookup_alternative_yaml"],
                b"ID,SEQ,A,B\n1,1,three,\n3,1,,\n",
                [
                    {
                        "spec_path": "rows.alternate.derivations.A.source.multiple_matches",
                        "handler": "multiple_matches",
                        "count": 1,
                    },
                    {
                        "spec_path": "rows.alternate.derivations.A.source.absent",
                        "handler": "absent",
                        "count": 1,
                    },
                    {
                        "spec_path": "rows.alternate.derivations.B.source.multiple_matches",
                        "handler": "multiple_matches",
                        "count": 1,
                    },
                    {
                        "spec_path": "rows.alternate.derivations.B.source.absent",
                        "handler": "absent",
                        "count": 1,
                    },
                ],
            ),
        ]
        for body, expected, ledger in probes:
            with self.subTest(body=body), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                entry = root / "spec.yaml"
                entry.write_text(body)
                for name, content in inputs.items():
                    (root / name).write_text(content)
                result = yamaa.spec(entry).run()
                self.assertEqual(result.issues.to_polars().rows(), [])
                self.assertEqual(
                    json.loads(result._native.observations())["handler_counts"], ledger
                )
                result.save()
                self.assertEqual((root / "result.csv").read_bytes(), expected)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            entry = root / "spec.yaml"
            entry.write_text(
                probes[1][0].replace("D.TIME < ALT.CUT", "D.TIME < SRC.CUT")
            )
            for name, content in inputs.items():
                (root / name).write_text(content)
            result = yamaa.spec(entry).run()
            findings = [
                dict(
                    phase=phase,
                    condition=condition,
                    rule=rule,
                    spec_paths=paths,
                    context=json.loads(context),
                )
                for phase, condition, rule, paths, context in result.issues.to_polars()
                .select(FIELDS)
                .rows()
            ]
            self.assertEqual(
                findings,
                [
                    {
                        "phase": "validation",
                        "condition": "unknown_field",
                        "rule": "LOOKUP-7",
                        "spec_paths": ["intermediates.P.filter"],
                        "context": {"identifier": "SRC.CUT", "intermediate": "P"},
                    }
                ],
            )
            self.assertIsNone(result.output)
            with self.assertRaises(yamaa.Error):
                result.save()
            self.assertFalse((root / "result.csv").exists())

    def test_public_case_intermediate_source_preserves_lazy_shared_selection_and_complete_ledger(
        self,
    ):
        import yamaa

        body = TRUTH["lookup_fallback_yaml"]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            entry = root / "spec.yaml"
            entry.write_text(body)
            (root / "src.csv").write_text("ID,SELECT\n1,Y\n2,N\n3,Y\n")
            (root / "donor.csv").write_text(TRUTH["fallback_donor_csv"])
            result = yamaa.spec(entry).run()
            self.assertEqual(result.issues.to_polars().rows(), [])
            self.assertEqual(
                json.loads(result._native.observations())["handler_counts"],
                [
                    {
                        "spec_path": "columns.A.derivation.case[0].then.source.multiple_matches",
                        "handler": "multiple_matches",
                        "count": 1,
                    },
                    {
                        "spec_path": "columns.A.derivation.case[0].then.source.absent",
                        "handler": "absent",
                        "count": 1,
                    },
                    {
                        "spec_path": "columns.B.derivation.case[0].then.source.multiple_matches",
                        "handler": "multiple_matches",
                        "count": 1,
                    },
                    {
                        "spec_path": "columns.B.derivation.case[0].then.source.absent",
                        "handler": "absent",
                        "count": 1,
                    },
                ],
            )
            result.save()
            self.assertEqual(
                (root / "result.csv").read_bytes(),
                b"ID,A,B\n1,first,\n2,skip,skip\n3,,\n",
            )
            # The intermediate answers the donor without a record, and its
            # answer counts under `absent` at the read, the binding's own row
            # (D202); a variable the lookup does not hold fails (D67,
            # LOOKUP-11).
            entry.write_text(
                body.replace(
                    "then: P.A", "then: {source: {variable: P.A, absent: fallback}}"
                )
            )
            admitted = yamaa.spec(entry).run()
            self.assertEqual(admitted.issues.to_polars().rows(), [])
            admitted.save()
            self.assertEqual(
                (root / "result.csv").read_bytes(),
                b"ID,A,B\n1,first,\n2,skip,skip\n3,,\n",
            )
            handlers = json.loads(admitted._native.observations())["handler_counts"]
            self.assertEqual(
                next(
                    row
                    for row in handlers
                    if row["spec_path"]
                    == "columns.A.derivation.case[0].then.source.absent"
                ),
                {
                    "spec_path": "columns.A.derivation.case[0].then.source.absent",
                    "handler": "absent",
                    "count": 1,
                },
            )
            entry.write_text(
                body.replace(
                    "then: P.A",
                    "then: {source: {variable: P.ABSENT, absent: fallback}}",
                )
            )
            refused = yamaa.spec(entry).run()
            records = refused.issues.to_polars().select(FIELDS).to_dicts()
            for record in records:
                record["context"] = json.loads(record["context"])
            self.assertEqual(
                records,
                [
                    {
                        "phase": "validation",
                        "condition": "unknown_field",
                        "rule": "LOOKUP-11",
                        "spec_paths": ["columns.A.derivation.case[0].then.source"],
                        "context": {"identifier": "P.ABSENT"},
                    }
                ],
            )
            self.assertIsNone(refused.output)

    def test_public_case_predicates_and_filtered_sources_preserve_complete_authored_truth(
        self,
    ):
        import yamaa

        truth = json.loads((ROOT / "case-predicate-truth.json").read_text())
        dispositions = json.loads(
            (ROOT / "case-predicate-dispositions.json").read_text()
        )["cases"]
        for name, case in truth["cases"].items():
            if name in dispositions:
                self.assertEqual(
                    dispositions[name]["classification"],
                    "historical_private_reference_defect",
                )
                self.assertEqual(dispositions[name]["rule"], "LOOKUP-8")
                self.assertEqual(case["csv"], dispositions[name]["historical_csv"])
                self.assertEqual(
                    (
                        ROOT / "case-predicate-history" / name / "expected.csv"
                    ).read_text(),
                    case["csv"],
                )
                continue
            with self.subTest(case=name), tempfile.TemporaryDirectory() as directory:
                destination = Path(directory) / name
                shutil.copytree(ROOT / "case-predicate-cases" / name, destination)
                entry = destination / "spec.yaml"
                result = yamaa.spec(entry).run()
                findings = []
                for row in result.issues.to_polars().select(FIELDS).to_dicts():
                    row["context"] = json.loads(row["context"])
                    findings.append(row)
                self.assertEqual(findings, case.get("findings", []))
                if "csv" in case:
                    result.save()
                    self.assertEqual(
                        (destination / "result.csv").read_bytes(), case["csv"].encode()
                    )
                else:
                    self.assertIsNone(result.output)
                    with self.assertRaises(yamaa.Error):
                        result.save()
                    self.assertFalse((destination / "result.csv").exists())

    def test_public_original_case_predicate_and_branch_filter_csv_truth_is_unchanged(
        self,
    ):
        import yamaa

        names = tuple(TRUTH["source_selection_case_names"])
        for name in names:
            with self.subTest(example=name), tempfile.TemporaryDirectory() as directory:
                destination = Path(directory) / name
                shutil.copytree(ROOT / "cases" / name, destination)
                result = yamaa.spec(destination / "spec.yaml").run()
                self.assertEqual(result.issues.to_pylist(), [])
                result.save()
                for expected in (destination / "expected").glob("*.csv"):
                    self.assertEqual(
                        (destination / expected.name).read_bytes(),
                        expected.read_bytes(),
                    )

    def test_public_source_selection_preserves_authored_types_effect_order_and_save_bytes(
        self,
    ):
        import polars as pl

        import yamaa

        cases = [
            (
                name,
                case,
                ROOT
                / (label + "-cases")
                / json.loads((ROOT / (label + "-case-paths.json")).read_text())[name],
            )
            for label in tuple(TRUTH["source_selection_modes"])
            for name, case in json.loads((ROOT / (label + "-truth.json")).read_text())[
                "cases"
            ].items()
        ]

        def issues(frame):
            rows = frame.to_polars().select(FIELDS).to_dicts()
            for row in rows:
                self.assertIsInstance(row["context"], str)
                row["context"] = json.loads(row["context"])
            return rows

        for name, case, staged in cases:
            for attempt in range(2):
                with (
                    self.subTest(case=name, attempt=attempt),
                    tempfile.TemporaryDirectory() as directory,
                ):
                    destination = Path(directory) / name
                    shutil.copytree(staged, destination)
                    entry = destination / "spec.yaml"
                    self.assertEqual(
                        issues(yamaa.spec(entry).check().issues), case["check_issues"]
                    )
                    result = yamaa.spec(entry).run()
                    self.assertEqual(issues(result.issues), case["issues"])
                    report = json.loads(result._native.observations())
                    self.assertEqual(report["handler_counts"], case["handlers"])
                    self.assertEqual(
                        report["source_reads"],
                        [
                            dict(
                                base_directory=".",
                                condition=None,
                                outcome="captured",
                                path=path,
                                snapshots_created=1,
                            )
                            for path in case["source_reads"]
                        ],
                    )
                    self.assertEqual(report["artifacts"], [])
                    if "table" in case:
                        expected = case["table"]
                        self.assertEqual(
                            result.output.column_names,
                            [column for column, _ in expected["columns"]],
                        )
                        self.assertEqual(
                            result.output.to_polars().dtypes,
                            [
                                pl.String if kind == "str" else pl.Int64
                                for _, kind in expected["columns"]
                            ],
                        )
                        self.assertEqual(
                            [
                                [None if value is None else str(value) for value in row]
                                for row in result.output.to_polars().rows()
                            ],
                            expected["rows"],
                        )
                        result.save()
                        self.assertEqual(
                            (destination / "result.csv").read_bytes(),
                            case["csv"].encode(),
                        )
                        (destination / "input.csv").write_bytes(b"changed after build")
                        result.save()
                        self.assertEqual(
                            (destination / "result.csv").read_bytes(),
                            case["csv"].encode(),
                        )
                    else:
                        self.assertIsNone(result.output)
                        with self.assertRaises(yamaa.Error):
                            result.save()
                        self.assertFalse((destination / "result.csv").exists())

    def test_public_completed_selected_assertions_preserve_complete_records_and_order(
        self,
    ):
        import polars as pl

        import yamaa

        label = "completed-lookup-assertions"
        cases = json.loads((ROOT / (label + "-truth.json")).read_text())["cases"]
        paths = json.loads((ROOT / (label + "-case-paths.json")).read_text())
        self.assertEqual(len(cases), 21)

        def issues(frame):
            rows = frame.to_polars().select(FIELDS).to_dicts()
            for row in rows:
                self.assertIsInstance(row["context"], str)
                row["context"] = json.loads(row["context"])
            return rows

        for name, case in cases.items():
            for attempt in range(2):
                with (
                    self.subTest(case=name, attempt=attempt),
                    tempfile.TemporaryDirectory() as directory,
                ):
                    destination = Path(directory).resolve() / "case"
                    shutil.copytree(
                        ROOT / (label + "-cases") / paths[name], destination
                    )
                    entry = destination / "spec.yaml"
                    self.assertEqual(
                        issues(yamaa.spec(entry).check().issues), case["check_issues"]
                    )
                    result = yamaa.spec(entry).run()
                    self.assertEqual(issues(result.issues), case["issues"])
                    report = json.loads(result._native.observations())
                    expected = [
                        {**record, "specification": str(entry)}
                        for record in case["verifications"]
                    ]
                    self.assertEqual(report["verifications"], expected)
                    self.assertEqual(report["handler_counts"], case["handlers"])
                    self.assertEqual(
                        report["source_reads"],
                        [
                            {
                                "base_directory": ".",
                                "condition": None,
                                "outcome": "captured",
                                "path": path,
                                "snapshots_created": 1,
                            }
                            for path in case["source_reads"]
                        ],
                    )
                    self.assertEqual(report["artifacts"], [])
                    if "table" in case:
                        expected_table = case["table"]
                        self.assertEqual(
                            result.output.column_names,
                            [column for column, _ in expected_table["columns"]],
                        )
                        self.assertEqual(
                            result.output.to_polars().dtypes,
                            [
                                pl.String if kind == "str" else pl.Int64
                                for _, kind in expected_table["columns"]
                            ],
                        )
                        self.assertEqual(
                            [
                                [None if value is None else str(value) for value in row]
                                for row in result.output.to_polars().rows()
                            ],
                            expected_table["rows"],
                        )
                        result.save()
                        self.assertEqual(
                            (destination / "result.csv").read_bytes(),
                            case["csv"].encode(),
                        )
                        (destination / "donor.csv").write_bytes(b"changed after build")
                        result.save()
                        self.assertEqual(
                            (destination / "result.csv").read_bytes(),
                            case["csv"].encode(),
                        )
                    else:
                        self.assertIsNone(result.output)
                        with self.assertRaises(yamaa.Error):
                            result.save()
                        self.assertFalse((destination / "result.csv").exists())


class NamespaceAdmission(unittest.TestCase):
    def test_original_namespace_obligations_through_both_public_routes(self):
        import yamaa

        for case in TRUTH["namespace_admission"]:
            with (
                self.subTest(case=case["name"]),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                entry = root / "spec.yaml"
                entry.write_text(case["specification"], encoding="ascii")
                original = entry.read_bytes()
                # `{entry}` stands for the specification's absolute path as
                # the engine resolves it, with forward slashes.
                resolved = entry.resolve().as_posix()
                expected = [
                    {
                        **issue,
                        "context": {
                            key: resolved if value == "{entry}" else value
                            for key, value in issue["context"].items()
                        },
                    }
                    for issue in case["issues"]
                ]
                # The absent source also proves these are declaration refusals.
                for _ in range(2):
                    checked = yamaa.spec(entry).check()
                    result = yamaa.spec(entry).run()
                    for frame in (checked.issues, result.issues):
                        rows = frame.to_polars().select(FIELDS).to_dicts()
                        for row in rows:
                            row["context"] = json.loads(row["context"])
                        self.assertEqual(rows, expected)
                    self.assertIsNone(result.output)
                    with self.assertRaises(yamaa.Error):
                        result.save()
                    self.assertFalse((root / "out.csv").exists())
                    self.assertEqual(entry.read_bytes(), original)


class PredicateSuggestions(unittest.TestCase):
    def test_original_qualified_suggestions_keep_unknown_and_ambiguous_names(self):
        import yamaa

        for case in TRUTH["predicate_suggestions"]:
            with (
                self.subTest(case=case["name"]),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                for name, content in case["inputs"].items():
                    source = root / name
                    source.parent.mkdir(parents=True, exist_ok=True)
                    source.write_text(content, encoding="ascii")
                entry = root / "spec.yaml"
                entry.write_text(case["specification"], encoding="ascii")
                for _ in range(2):
                    checked = yamaa.spec(entry).check()
                    result = yamaa.spec(entry).run()
                    for frame, expected in (
                        (checked.issues, case["check_issues"]),
                        (result.issues, case["issues"]),
                    ):
                        rows = frame.to_polars().select(FIELDS).to_dicts()
                        for row in rows:
                            row["context"] = json.loads(row["context"])
                        self.assertEqual(rows, expected)
                    if case["csv"] is not None:
                        result.save()
                        self.assertEqual(
                            (root / "out.csv").read_bytes(), case["csv"].encode("ascii")
                        )
                    else:
                        self.assertIsNone(result.output)
                        with self.assertRaises(yamaa.Error):
                            result.save()
                        self.assertFalse((root / "out.csv").exists())


class IntermediateDependencies(unittest.TestCase):
    def test_original_cycles_and_forward_donor_reads_through_both_public_routes(self):
        import yamaa

        for case in TRUTH["intermediate_dependencies"]:
            with (
                self.subTest(case=case["name"]),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                entry = root / "spec.yaml"
                entry.write_text(case["specification"], encoding="ascii")

                def rows(frame):
                    records = frame.to_polars().select(FIELDS).to_dicts()
                    for record in records:
                        record["context"] = json.loads(record["context"])
                    return records

                for inputs_present in (False, True):
                    if inputs_present:
                        for name, content in case["inputs"].items():
                            path = root / name
                            path.parent.mkdir(parents=True, exist_ok=True)
                            path.write_text(content, encoding="ascii")
                    elif not case["issues"]:
                        continue
                    for attempt in range(2):
                        with self.subTest(
                            inputs_present=inputs_present, attempt=attempt
                        ):
                            self.assertEqual(
                                rows(yamaa.spec(entry).check().issues), case["issues"]
                            )
                            result = yamaa.spec(entry).run()
                            self.assertEqual(rows(result.issues), case["issues"])
                            self.assertFalse((root / "out.csv").exists())
                            if case["issues"]:
                                self.assertIsNone(result.output)
                                self.assertIsNone(result._native.observations())
                                with self.assertRaises(yamaa.Error):
                                    result.save()
                            else:
                                if "output_rows" in case:
                                    self.assertEqual(
                                        [
                                            list(row)
                                            for row in result.output.to_polars().rows()
                                        ],
                                        case["output_rows"],
                                    )
                                result.save()
                                self.assertEqual(
                                    (root / "out.csv").read_bytes(),
                                    case["csv"].encode("ascii"),
                                )
                                (root / "out.csv").unlink()
                self.assertEqual(
                    entry.read_text(encoding="ascii"), case["specification"]
                )


class IntermediateRowDrivers(unittest.TestCase):
    def test_source_only_driver_records_preserve_lookup_key_admission(self):
        import yamaa

        for case in TRUTH["intermediate_row_drivers"]:
            with (
                self.subTest(case=case["name"]),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                for name, content in case["inputs"].items():
                    source = root / name
                    source.parent.mkdir(parents=True, exist_ok=True)
                    source.write_text(content, encoding="ascii")
                entry = root / "spec.yaml"
                entry.write_text(case["specification"], encoding="ascii")
                for _ in range(2):
                    checked = yamaa.spec(entry).check()
                    result = yamaa.spec(entry).run()
                    for frame, expected in (
                        (checked.issues, case["check_issues"]),
                        (result.issues, case["issues"]),
                    ):
                        rows = frame.to_polars().select(FIELDS).to_dicts()
                        for row in rows:
                            row["context"] = json.loads(row["context"])
                        self.assertEqual(rows, expected)
                    if case["csv"] is not None:
                        result.save()
                        self.assertEqual(
                            (root / "out.csv").read_bytes(), case["csv"].encode("ascii")
                        )
                    else:
                        self.assertIsNone(result.output)
                        with self.assertRaises(yamaa.Error):
                            result.save()
                        self.assertFalse((root / "out.csv").exists())


class DiagnosticSites(unittest.TestCase):
    def test_original_unknown_references_keep_authored_identifier_and_dataset(self):
        import yamaa

        def issues(frame):
            rows = frame.to_polars().select(FIELDS).to_dicts()
            self.assertEqual(frame.column_names, ISSUE_COLUMNS)
            for row in rows:
                self.assertIsInstance(row["context"], str)
                row["context"] = json.loads(row["context"])
            return rows

        for case in TRUTH["diagnostic_sites"]:
            with (
                self.subTest(case=case["name"]),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                entry = root / "spec.yaml"
                entry.write_text(case["specification"], encoding="utf-8")
                self.assertEqual(
                    issues(yamaa.spec(entry).check().issues), case["check_issues"]
                )
                for name, content in case["inputs"].items():
                    path = root / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text(content, encoding="utf-8")
                for _ in range(2):
                    self.assertEqual(
                        issues(yamaa.spec(entry).check().issues), case["check_issues"]
                    )
                    result = yamaa.spec(entry).run()
                    self.assertEqual(issues(result.issues), case["issues"])
                    self.assertIsNone(result.output)
                    with self.assertRaises(yamaa.Error):
                        result.save()
                    self.assertFalse((root / case["output"]).exists())


class RowCountLookups(unittest.TestCase):
    def test_original_row_count_predicates_resolve_typed_lookup_fields(self):
        import yamaa

        for case in TRUTH["row_count_lookups"]:
            with (
                self.subTest(case=case["name"]),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                for name, content in case["inputs"].items():
                    source = root / name
                    source.parent.mkdir(parents=True, exist_ok=True)
                    source.write_text(content, encoding="ascii")
                entry = root / "spec.yaml"
                entry.write_text(case["specification"], encoding="ascii")
                for _ in range(2):
                    checked = yamaa.spec(entry).check()
                    result = yamaa.spec(entry).run()
                    for frame, expected in (
                        (checked.issues, case["check_issues"]),
                        (result.issues, case["issues"]),
                    ):
                        rows = frame.to_polars().select(FIELDS).to_dicts()
                        for row in rows:
                            row["context"] = json.loads(row["context"])
                        self.assertEqual(rows, expected)
                    report = json.loads(result._native.observations())
                    expected = [
                        {**record, "specification": str(entry)}
                        for record in case["verifications"]
                    ]
                    self.assertEqual(report["verifications"], expected)
                    self.assertEqual(report["handler_counts"], case["handler_counts"])
                    if case["csv"] is not None:
                        result.save()
                        self.assertEqual(
                            (root / "out.csv").read_bytes(), case["csv"].encode("ascii")
                        )
                    else:
                        self.assertIsNone(result.output)
                        with self.assertRaises(yamaa.Error):
                            result.save()
                        self.assertFalse((root / "out.csv").exists())


class BaselineWindowTypes(unittest.TestCase):
    def test_baseline_temporal_types_keep_owned_conditions_and_valid_carriers(self):
        import yamaa

        for case in TRUTH["baseline_window_types"]:
            with (
                self.subTest(case=case["name"]),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                for name, content in case["inputs"].items():
                    source = root / name
                    source.parent.mkdir(parents=True, exist_ok=True)
                    source.write_text(content, encoding="ascii")
                entry = root / "spec.yaml"
                entry.write_text(case["specification"], encoding="ascii")
                for _ in range(2):
                    checked = yamaa.spec(entry).check()
                    result = yamaa.spec(entry).run()
                    for frame, expected in (
                        (checked.issues, case["check_issues"]),
                        (result.issues, case["issues"]),
                    ):
                        rows = frame.to_polars().select(FIELDS).to_dicts()
                        for row in rows:
                            row["context"] = json.loads(row["context"])
                        self.assertEqual(rows, expected)
                    if case["csv"] is not None:
                        result.save()
                        self.assertEqual(
                            (root / "out.csv").read_bytes(), case["csv"].encode("ascii")
                        )
                    else:
                        self.assertIsNone(result.output)
                        with self.assertRaises(yamaa.Error):
                            result.save()
                        self.assertFalse((root / "out.csv").exists())


class RowJoinKeys(unittest.TestCase):
    def test_original_row_join_keys_are_raw_driver_fields(self):
        import yamaa

        def issues(frame):
            self.assertEqual(frame.column_names, ISSUE_COLUMNS)
            rows = frame.to_polars().select(FIELDS).to_dicts()
            for row in rows:
                self.assertIsInstance(row["context"], str)
                row["context"] = json.loads(row["context"])
            return rows

        self.assertEqual(len(TRUTH["row_join_keys"]), 11)
        for case in TRUTH["row_join_keys"]:
            with (
                self.subTest(case=case["name"]),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                entry = root / "spec.yaml"
                entry.write_text(case["specification"], encoding="utf-8")
                self.assertEqual(
                    issues(yamaa.spec(entry).check().issues), case["check_issues"]
                )
                for name, content in case["inputs"].items():
                    path = root / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text(content, encoding="utf-8")
                for _ in range(2):
                    self.assertEqual(
                        issues(yamaa.spec(entry).check().issues), case["check_issues"]
                    )
                    result = yamaa.spec(entry).run()
                    self.assertEqual(issues(result.issues), case["issues"])
                    if case["csv"] is not None:
                        self.assertIsNotNone(result.output)
                        result.save()
                        self.assertEqual(
                            (root / "out.csv").read_bytes(), case["csv"].encode()
                        )
                        (root / "out.csv").unlink()
                    else:
                        self.assertIsNone(result.output)
                        with self.assertRaises(yamaa.Error):
                            result.save()
                        self.assertFalse((root / "out.csv").exists())


class KeyExpressions(unittest.TestCase):
    def test_public_scalar_and_window_keys_keep_scope_comparison_handlers_and_saved_truth(
        self,
    ):
        import yamaa

        def issues(frame):
            records = frame.to_polars().select(FIELDS).to_dicts()
            for record in records:
                self.assertIsInstance(record["context"], str)
                record["context"] = json.loads(record["context"])
            return records

        for case in TRUTH["key_expressions"]:
            with (
                self.subTest(case=case["name"]),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                entry = root / "spec.yaml"
                entry.write_text(case["specification"], encoding="ascii")
                # A refusal binding decides from the declarations is reported
                # by the build only once it has captured the inputs.
                if case["check_issues"] and not case.get("check_after_capture", False):
                    self.assertEqual(
                        issues(yamaa.spec(entry).check().issues), case["check_issues"]
                    )
                    self.assertEqual(
                        issues(yamaa.spec(entry).run().issues), case["check_issues"]
                    )
                for name, content in case["inputs"].items():
                    (root / name).write_text(content, encoding="ascii")
                for attempt in range(2):
                    with self.subTest(attempt=attempt):
                        self.assertEqual(
                            issues(yamaa.spec(entry).check().issues),
                            case["check_issues"],
                        )
                        result = yamaa.spec(entry).run()
                        self.assertEqual(issues(result.issues), case["issues"])
                        self.assertFalse((root / "out.csv").exists())
                        if case["issues"]:
                            self.assertIsNone(result.output)
                            with self.assertRaises(yamaa.Error):
                                result.save()
                            self.assertFalse((root / "out.csv").exists())
                        else:
                            observations = json.loads(result._native.observations())
                            self.assertEqual(
                                observations["handler_counts"], case["handlers"]
                            )
                            result.save()
                            self.assertEqual(
                                (root / "out.csv").read_bytes(),
                                case["csv"].encode("ascii"),
                            )
                            (root / "source.csv").write_bytes(b"changed after build")
                            result.save()
                            self.assertEqual(
                                (root / "out.csv").read_bytes(),
                                case["csv"].encode("ascii"),
                            )
                            (root / "out.csv").unlink()
                            (root / "source.csv").write_text(
                                case["inputs"]["source.csv"], encoding="ascii"
                            )
                self.assertEqual(
                    entry.read_text(encoding="ascii"), case["specification"]
                )


if __name__ == "__main__":
    unittest.main()
