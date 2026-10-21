"""The host suites' Parquet inputs, written at staging time, read in pyarrow as
the suites state them."""

import datetime
import tempfile
import unittest
from decimal import Decimal
from pathlib import Path

import pyarrow.parquet as pq

from tools.staging import build_parquet_source_inputs as inputs

POINT = bytes.fromhex("0101000000000000000000f03f0000000000000040")
UTC = datetime.timezone.utc
# Each input's fields as pyarrow reads them, and its rows where they decode.
EXPECTED = {
    "text": ([("I", "int64"), ("S", "string")], [(1, ""), (2, "X")]),
    "empty-name": ([("", "int64")], [(1,)]),
    "duplicate": ([("I", "int64"), ("I", "int64")], None),
    "mixed-struct": (
        [
            ("I", "int64"),
            ("S", "struct<left: int64, right: string>"),
            ("AFTER", "int64"),
        ],
        [(1, {"left": 2, "right": "text"}, 1)],
    ),
    "mixed-map": (
        [("I", "int64"), ("M", "map<string, int64 ('M')>"), ("AFTER", "int64")],
        [(1, [("key", 2)], 1)],
    ),
    "mixed-empty": ([("", "int64"), ("M", "map<string, int64 ('M')>")], None),
    "mixed-duplicate": (
        [("I", "int64"), ("I", "int64"), ("M", "map<string, int64 ('M')>")],
        None,
    ),
    "utf8": ([("TEXT", "string")], None),
    "utf8-bool": ([("OTHER", "bool"), ("TEXT", "string")], None),
    "utf8-time": ([("OTHER", "timestamp[us]"), ("TEXT", "string")], None),
    "unsupported-int32": ([("FIELD", "int32")], [(1,)]),
    "unsupported-uint64": ([("FIELD", "uint64")], [(1,)]),
    "unsupported-float32": ([("FIELD", "float")], [(1.25,)]),
    "unsupported-binary": ([("FIELD", "binary")], [(b"x",)]),
    "unsupported-milliseconds": (
        [("FIELD", "timestamp[ms]")],
        [(datetime.datetime(1970, 1, 1, 0, 0, 1),)],
    ),
    "unsupported-timezone": (
        [("FIELD", "timestamp[us, tz=UTC]")],
        [(datetime.datetime(1970, 1, 1, 0, 0, 1, tzinfo=UTC),)],
    ),
    "unsupported-list": ([("FIELD", "list<element: int64>")], [([1],)]),
    "unsupported-fixed-list": (
        [("FIELD", "fixed_size_list<element: int64>[2]")],
        [([1, 2],)],
    ),
    "unsupported-struct": ([("FIELD", "struct<item: int64>")], [({"item": 1},)]),
    "unsupported-decimal": (
        [("FIELD", "decimal128(10, 2)")],
        [(Decimal("1.23"),)],
    ),
    "unsupported-decimal32": ([("FIELD", "decimal32(9, 2)")], [(Decimal("1.23"),)]),
    "unsupported-decimal64": (
        [("FIELD", "decimal64(18, 2)")],
        [(Decimal("1.23"),)],
    ),
    "unsupported-list-view": ([("FIELD", "list_view<element: int64>")], [([1],)]),
    "unsupported-large-list-view": (
        [("FIELD", "large_list_view<element: int64>")],
        [([1],)],
    ),
    "unsupported-geometry": ([("FIELD", "binary")], [(POINT,)]),
    "unsupported-geography": ([("FIELD", "binary")], [(POINT,)]),
    "unsupported-variant": (
        [("FIELD", "struct<metadata: binary not null, value: binary not null>")],
        [({"metadata": b"\x01\x00\x00", "value": b"\x0c\x01"},)],
    ),
}


class StagedParquetInputs(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory(prefix="yamaa-parquet-inputs-")
        cls.root = Path(cls.directory.name) / "pq"
        inputs.build(cls.root)

    @classmethod
    def tearDownClass(cls):
        cls.directory.cleanup()

    def test_each_input_that_a_host_suite_reads_is_staged(self):
        self.assertEqual(
            sorted(path.name for path in self.root.iterdir()),
            sorted(name + ".parquet" for name in [*EXPECTED, "ordered-sum"]),
        )

    def test_pyarrow_reads_each_stated_field_and_row(self):
        for name, (fields, rows) in EXPECTED.items():
            with self.subTest(name=name):
                handle = pq.ParquetFile(self.root / (name + ".parquet"))
                schema = handle.schema_arrow
                self.assertEqual([(f.name, str(f.type)) for f in schema], fields)
                if rows is not None:
                    table = handle.read()
                    columns = [column.to_pylist() for column in table.columns]
                    self.assertEqual(list(zip(*columns)), rows)

    def test_a_text_payload_that_is_not_utf8_follows_the_refused_field(self):
        for name in ("utf8", "utf8-bool", "utf8-time"):
            with self.subTest(name=name):
                table = pq.ParquetFile(self.root / (name + ".parquet")).read()
                text = table.column("TEXT").chunk(0)
                self.assertEqual(text.buffers()[2].to_pybytes(), b"\xff\xff")
        table = pq.ParquetFile(self.root / "utf8-time.parquet").read()
        self.assertEqual(table.column("OTHER").cast("int64").to_pylist(), [1])

    def test_the_ordered_sum_source_keeps_the_original_rows(self):
        original = (
            inputs.ROOT / "benchmarks/adam-adlb-ordered-sum/input/lb.csv"
        ).read_text(encoding="ascii")
        header, *lines = original.splitlines()
        table = pq.ParquetFile(self.root / "ordered-sum.parquet").read()
        self.assertEqual(table.column_names, header.split(","))
        self.assertEqual(str(table.schema.field("LBSTRESN").type), "double")
        rows = [
            ",".join("" if value is None else str(value) for value in row)
            for row in zip(*(column.to_pylist() for column in table.columns))
        ]
        self.assertEqual(rows, lines)

    def test_every_build_writes_the_same_bytes(self):
        with tempfile.TemporaryDirectory(prefix="yamaa-parquet-inputs-") as other:
            again = Path(other) / "pq"
            inputs.build(again)
            for path in self.root.iterdir():
                with self.subTest(name=path.name):
                    self.assertEqual(
                        (again / path.name).read_bytes(), path.read_bytes()
                    )


if __name__ == "__main__":
    unittest.main()
