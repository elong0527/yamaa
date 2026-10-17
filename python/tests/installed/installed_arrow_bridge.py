"""Installed Arrow C Stream handoff with independent values and buffer evidence."""

import gc
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import polars as pl
import pyarrow as pa
import pyarrow.compute as pc

import yamaa

SPEC = """schema_version: '1.0'
domain: TEST
base: SRC
keys: [ID]
input:
  SRC:
    path: input.csv
    types: {ID: int, S: str, I: int, F: float, D: date, T: datetime}
columns:
  - {name: ID, type: int, label: Identifier, derivation: SRC.ID}
  - {name: S, type: str, label: Text, derivation: SRC.S}
  - {name: I, type: int, label: Integer, derivation: SRC.I}
  - {name: F, type: float, label: Float, derivation: SRC.F}
  - {name: D, type: date, label: Date, derivation: SRC.D}
  - {name: T, type: datetime, label: Datetime, derivation: SRC.T}
output: {path: output.csv, columns: [ID, S, I, F, D, T]}
"""
INPUT = (
    "ID,S,I,F,D,T\n"
    "1,a\0\U0001f980,-9223372036854775808,-0.0,0001-01-01,0001-01-01T00:00:00\n"
    "2,,,,,\n"
    "3,\u03b2,9223372036854775807,1.25,9999-12-31,9999-12-31T23:59:59\n"
)


def addresses(table):
    return [
        [None if buffer is None else buffer.address for buffer in chunk.buffers()]
        for column in table.columns
        for chunk in column.chunks
    ]


class ArrowBridge(unittest.TestCase):
    def setUp(self):
        work = tempfile.TemporaryDirectory(prefix="yamaa-arrow-bridge-")
        self.addCleanup(work.cleanup)
        self.root = Path(work.name)
        (self.root / "spec.yaml").write_text(SPEC, encoding="utf-8")
        (self.root / "input.csv").write_text(INPUT, encoding="utf-8")

    def build(self):
        result = yamaa.domain(self.root / "spec.yaml")
        self.assertEqual(result.issues.height, 0)
        return result

    def test_exact_types_buffer_sharing_and_owned_default_views(self):
        built = self.build()
        first = built.output_arrow
        self.assertEqual(
            first.schema.types,
            [
                pa.int64(),
                pa.string(),
                pa.int64(),
                pa.float64(),
                pa.date32(),
                pa.timestamp("s"),
            ],
        )
        self.assertEqual(first["S"].to_pylist(), ["a\0\U0001f980", None, "\u03b2"])
        self.assertEqual(first["I"].to_pylist(), [-(2**63), None, 2**63 - 1])
        self.assertEqual(first["F"].to_pylist(), [-0.0, None, 1.25])
        self.assertEqual(
            pc.cast(first["D"], pa.int32()).to_pylist(), [-719162, None, 2932896]
        )
        self.assertEqual(
            pc.cast(first["T"], pa.int64()).to_pylist(),
            [-62135596800, None, 253402300799],
        )
        self.assertEqual(addresses(first), addresses(built.output_arrow))
        # The run report crosses the same bridge with its two int64 counts.
        report = built.report_arrow
        self.assertEqual(
            report["KIND"].to_pylist(), ["run", "input", "rows"] + ["column"] * 6
        )
        self.assertEqual(report.schema.field("COUNT").type, pa.int64())
        self.assertEqual(report.schema.field("TOTAL").type, pa.int64())
        self.assertEqual(report["COUNT"].to_pylist(), [3, 3, 3, 0, 1, 1, 1, 1, 1])
        self.assertEqual(report["DETAILS"][4].as_py(), '{"max_length":3}')
        self.assertEqual(addresses(report), addresses(built.report_arrow))
        # The default public result traverses the same bridge, without IPC.
        with patch.object(
            pl, "read_ipc_stream", side_effect=AssertionError("IPC host handoff")
        ):
            frame = built.output
        self.assertEqual(frame["I"].to_list(), [-(2**63), None, 2**63 - 1])
        self.assertEqual(frame["S"].to_list(), ["a\0\U0001f980", None, "\u03b2"])
        self.assertEqual(frame["D"].dtype, pl.Date)
        self.assertIsInstance(frame["T"].dtype, pl.Datetime)
        del built
        gc.collect()
        self.assertEqual(first["I"][0].as_py(), -(2**63))

    def test_failed_conversion_unconsumed_capsule_and_early_release(self):
        built = self.build()
        exporter = built._native.output()
        for _ in range(30):
            capsule = exporter.__arrow_c_stream__()
            del capsule
            gc.collect()
        with self.assertRaises(NotImplementedError):
            exporter.__arrow_c_stream__(
                pa.schema([("I", pa.int32())]).__arrow_c_schema__()
            )
        reader = pa.RecordBatchReader.from_stream(exporter)
        batch = reader.read_next_batch()
        reader.close()
        del reader, exporter, built
        gc.collect()
        self.assertEqual(batch["S"][0].as_py(), "a\0\U0001f980")
        with self.assertRaises(pa.ArrowInvalid):
            pc.cast(batch["I"], pa.int32(), safe=True)
        self.assertEqual(batch["I"][2].as_py(), 2**63 - 1)

    def test_repeated_chunked_exports_empty_batches_and_empty_result(self):
        built = self.build()
        table = built.output_arrow
        expected = table.to_pydict()
        first = table.to_batches()[0]
        for _ in range(30):
            source = pa.RecordBatchReader.from_batches(
                first.schema,
                [
                    first.slice(0, 0),
                    first.slice(0, 1),
                    first.slice(1, 0),
                    first.slice(1),
                ],
            )
            table = pa.table(source)
            self.assertEqual(table.to_pydict(), expected)
            # Subsequent cycles preserve chunk boundaries as well as values.
            table = pa.table(
                pa.RecordBatchReader.from_batches(table.schema, table.to_batches())
            )
        (self.root / "input.csv").write_text("ID,S,I,F,D,T\n", encoding="utf-8")
        empty = self.build()
        self.assertEqual(empty.output_arrow.shape, (0, 6))
        self.assertEqual(empty.output.shape, (0, 6))
        self.assertEqual(empty.output_arrow.schema, built.output_arrow.schema)


if __name__ == "__main__":
    unittest.main()
