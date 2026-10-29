"""Installed Arrow C Stream handoff with independent values and buffer evidence.

Every result table (output, issues and report) reaches Python through the
Arrow C stream of the native run, never through IPC bytes. The binding's
census of live native handles (installed_handle_lifetime.py) shows that early
release, a failed conversion and repeated exports leave no handle behind.
"""

import gc
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import polars as pl
import pyarrow as pa
import pyarrow.compute as pc

import yamaa
from yamaa import _native

SPEC = """yamaa: "1.0"
type: spec
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

    def run_spec(self):
        result = yamaa.spec(self.root / "spec.yaml").run()
        self.assertEqual(result.issues.num_rows, 0)
        return result

    def test_exact_types_buffer_sharing_and_frames_without_ipc(self):
        run = self.run_spec()
        first = pa.table(run._native.output())
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
        # Each export is a fresh stream over the same held buffers.
        self.assertEqual(addresses(first), addresses(pa.table(run._native.output())))
        # The public tables convert through the same bridge, without IPC.
        with (
            patch.object(
                pl, "read_ipc_stream", side_effect=AssertionError("IPC host handoff")
            ),
            patch.object(
                pl, "read_ipc", side_effect=AssertionError("IPC host handoff")
            ),
        ):
            frame = run.output.to_polars()
            issues = run.issues.to_polars()
            report = run.report.to_polars()
        self.assertEqual(frame["I"].to_list(), [-(2**63), None, 2**63 - 1])
        self.assertEqual(frame["S"].to_list(), ["a\0\U0001f980", None, "\u03b2"])
        self.assertEqual(frame["D"].dtype, pl.Date)
        self.assertIsInstance(frame["T"].dtype, pl.Datetime)
        self.assertEqual(
            pa.table(run._native.issues()).schema.types,
            [
                pa.string(),
                pa.string(),
                pa.string(),
                pa.string(),
                pa.list_(pa.string()),
                pa.string(),
                pa.int64(),
                pa.int64(),
                pa.string(),
                pa.string(),
                pa.list_(pa.int64()),
                pa.list_(pa.string()),
            ],
        )
        self.assertTrue(issues.is_empty())
        held = pa.table(run._native.report())
        self.assertEqual(
            held.schema.types, [pa.string()] * 10 + [pa.int64()] * 2 + [pa.string()] * 2
        )
        self.assertEqual(report.to_dict(as_series=False), held.to_pydict())
        del run
        gc.collect()
        self.assertEqual(first["I"][0].as_py(), -(2**63))

    def test_failed_conversion_unconsumed_capsule_and_early_release(self):
        gc.collect()
        before = _native._live_handles()
        run = self.run_spec()
        exporter = run._native.output()
        for _ in range(30):
            capsule = exporter.__arrow_c_stream__()
            del capsule
            gc.collect()
        with self.assertRaises(yamaa.Error):
            exporter.__arrow_c_stream__(
                pa.schema([("I", pa.int32())]).__arrow_c_schema__()
            )
        reader = pa.RecordBatchReader.from_stream(exporter)
        batch = reader.read_next_batch()
        reader.close()
        del reader, exporter, run
        gc.collect()
        self.assertEqual(_native._live_handles(), before)
        self.assertEqual(batch["S"][0].as_py(), "a\0\U0001f980")
        with self.assertRaises(pa.ArrowInvalid):
            pc.cast(batch["I"], pa.int32(), safe=True)
        self.assertEqual(batch["I"][2].as_py(), 2**63 - 1)

    def test_repeated_chunked_exports_empty_batches_and_empty_result(self):
        gc.collect()
        before = _native._live_handles()
        run = self.run_spec()
        table = pa.table(run._native.output())
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
        empty = self.run_spec()
        empty_table = pa.table(empty._native.output())
        self.assertEqual(empty_table.shape, (0, 6))
        self.assertEqual(
            (empty.output.num_rows, len(empty.output.column_names)), (0, 6)
        )
        self.assertEqual(empty_table.schema, pa.table(run._native.output()).schema)
        del run, empty
        gc.collect()
        self.assertEqual(_native._live_handles(), before)


if __name__ == "__main__":
    unittest.main()
