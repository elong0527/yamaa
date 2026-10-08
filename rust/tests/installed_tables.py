"""Run installed native and Python facade table checks outside the checkout."""

import json
import struct
import unittest
from pathlib import Path

import polars as pl
import pyarrow as pa
import yamaa
from yamaa import _native as yamaa_native
from yamaa.adapters.native_tables import ipc_from_polars, polars_from_ipc

ROOT = Path(__file__).with_name("tables")


class InstalledTables(unittest.TestCase):
    """Cross actual installed Python/Arrow/Polars boundaries without candidate truth."""

    def test_packages_are_installed(self):
        """The facade must be a wheel install, not an editable source checkout."""
        self.assertIn("site-packages", str(Path(yamaa.__file__).resolve()))
        self.assertIn("site-packages", str(Path(yamaa_native.__file__).resolve()))

    def test_arrow_and_polars_round_trip(self):
        """Pin exact values/precision, nulls, row order and direct IPC chunk order."""
        expected = json.loads((ROOT / "expected.json").read_text(encoding="utf-8"))
        for name in ("mixed", "empty", "schema_only"):
            request = (ROOT / f"{name}.arrow").read_bytes()
            normalized = yamaa_native.table_round_trip(request)
            reader = pa.ipc.open_stream(normalized)
            schema = reader.schema
            batches = list(reader)
            self.assertEqual(
                [str(b.num_rows) for b in batches], expected[name]["chunks"]
            )
            if name == "mixed":
                table = pa.Table.from_batches(batches, schema=schema)
                self.assertEqual(
                    table.column("I").to_pylist(),
                    [-(2**63), 2**63 - 1, 2**53 + 1, None, 0],
                )
                values = table.column("F").to_pylist()
                self.assertEqual(struct.pack(">d", values[0]).hex(), "8000000000000000")
                self.assertEqual(values[1:4], [None] * 3)
                self.assertEqual(struct.pack(">d", values[4]).hex(), "0000000000000001")
                self.assertEqual(
                    table.column("S").to_pylist(),
                    ["", "a\0\U0001f980", None, "last", "x"],
                )
            frame = polars_from_ipc(request)
            del request
            output = ipc_from_polars(frame, schema)
            truth = json.loads(yamaa_native.table_snapshot(output))
            for field in ("protocol", "columns", "row_count", "rows"):
                self.assertEqual(truth[field], expected[name][field], (name, field))
            self.assertEqual(pa.ipc.open_stream(output).schema, schema)

    def test_zero_column_rows_require_lossless_ipc(self):
        """Reject a host representation that cannot preserve the input row count."""
        with self.assertRaisesRegex(ValueError, "zero-column"):
            polars_from_ipc((ROOT / "zero_columns.arrow").read_bytes())

    def test_physical_restoration_does_not_coerce_logical_types(self):
        """Schema restoration cannot parse text or silently promote numeric columns."""
        for values, target in [
            (["1"], pa.int64()),
            ([1], pa.float64()),
            ([1.0], pa.int64()),
            ([True], pa.int64()),
        ]:
            with self.assertRaisesRegex(ValueError, "incompatible logical"):
                ipc_from_polars(
                    pl.DataFrame({"A": values}), pa.schema([pa.field("A", target)])
                )
        temporal = pa.schema(
            [
                pa.field(
                    "T",
                    pa.struct(
                        [
                            pa.field("value", pa.timestamp("s"), False),
                            pa.field("precision", pa.uint8(), False),
                        ]
                    ),
                )
            ]
        )
        bad = pl.DataFrame({"T": [{"value": 0, "precision": 1}]}).with_columns(
            pl.col("T").struct.with_fields(pl.field("value").cast(pl.Datetime("ms")))
        )
        with self.assertRaisesRegex(ValueError, "precision must use UInt8"):
            ipc_from_polars(bad, temporal)

    def test_visible_precision_and_fractional_seconds_are_not_repaired(self):
        """Invalid visible children and fractional seconds fail instead of truncating."""
        schema = pa.ipc.open_stream((ROOT / "mixed.arrow").read_bytes()).schema
        frame = polars_from_ipc((ROOT / "mixed.arrow").read_bytes())
        with self.assertRaises(ValueError):
            ipc_from_polars(
                frame.select(pl.all().reverse()).select(pl.all().exclude("I")), schema
            )
        # Force visible fractional milliseconds and retain the explicit precision child.
        field = schema.field("T")
        bad = pl.DataFrame({"T": [{"value": 1, "precision": 1}]}).with_columns(
            pl.col("T").struct.with_fields(
                pl.field("value").cast(pl.Datetime("ms")),
                pl.field("precision").cast(pl.UInt8),
            )
        )
        with self.assertRaises(pa.ArrowInvalid):
            ipc_from_polars(bad, pa.schema([field]))
        for child in ("value", "precision"):
            bad = (
                pl.DataFrame({"T": [{"value": 0, "precision": 1}]})
                .with_columns(
                    pl.col("T").struct.with_fields(
                        pl.field("value").cast(pl.Datetime("ms")),
                        pl.field("precision").cast(pl.UInt8),
                    )
                )
                .with_columns(
                    pl.col("T").struct.with_fields(
                        pl.lit(None)
                        .cast(pl.Datetime("ms") if child == "value" else pl.UInt8)
                        .alias(child)
                    )
                )
            )
            with self.assertRaisesRegex(ValueError, "visible temporal"):
                ipc_from_polars(bad, pa.schema([field]))


if __name__ == "__main__":
    unittest.main()
