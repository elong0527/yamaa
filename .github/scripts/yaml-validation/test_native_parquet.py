"""Independent profile controls through public native source admission."""

import datetime
import io
import unittest
from unittest.mock import patch

import _native_parquet as native
import pyarrow as pa
import pyarrow.parquet as pq


def encoded(table):
    buffer = io.BytesIO()
    pq.write_table(table, buffer, compression="none")
    return buffer.getvalue()


class NativeParquet(unittest.TestCase):
    def test_admitted_metadata_preserves_all_carriers_order_and_header_only_fields(
        self,
    ):
        table = pa.table(
            {
                "TEXT": pa.array(["a", None], pa.string()),
                "INTEGER": pa.array(
                    [-9223372036854775808, 9223372036854775807], pa.int64()
                ),
                "FLOAT": pa.array([1.5, None], pa.float64()),
                "DATE": pa.array([datetime.date(2026, 1, 1), None], pa.date32()),
                "TIME": pa.array(
                    [datetime.datetime(2026, 1, 1), None], pa.timestamp("us")
                ),
            }
        )
        expected = (
            ("TEXT", "str"),
            ("INTEGER", "int"),
            ("FLOAT", "float"),
            ("DATE", "date"),
            ("TIME", "datetime"),
        )
        for value in (table, table.slice(0, 0)):
            metadata, fields = native.parquet_source_fields(encoded(value))
            self.assertEqual(fields, expected)
            self.assertEqual(metadata.metadata.num_rows, value.num_rows)

    def test_all_source_fields_are_admitted_before_metadata_even_when_unprojected(self):
        content = encoded(
            pa.table(
                {"GOOD": pa.array([1], pa.int64()), "BAD": pa.array([2], pa.int32())}
            )
        )
        with patch.object(
            native.pq,
            "ParquetFile",
            side_effect=AssertionError("metadata before admission"),
        ):
            with self.assertRaises(native.ParquetProfileFailure) as caught:
                native.parquet_source_fields(content)
        failure = caught.exception
        self.assertEqual(failure.condition, "source_field_type_unsupported")
        self.assertEqual(failure.requirement, "REQ-1040")
        self.assertEqual(
            failure.context,
            {
                "dataset": "SOURCE",
                "path": "source.parquet",
                "field": "BAD",
                "stored_type": "int32",
            },
        )

    def test_temporal_values_are_admitted_without_reading_host_values(self):
        content = encoded(
            pa.table({"TIME": pa.array([1], pa.int64()).cast(pa.timestamp("us"))})
        )
        with self.assertRaises(native.ParquetProfileFailure) as caught:
            native.parquet_source_fields(content)
        self.assertEqual(caught.exception.condition, "source_field_value_invalid")
        self.assertEqual(caught.exception.requirement, "REQ-1041")
        self.assertEqual(
            caught.exception.context,
            {
                "dataset": "SOURCE",
                "path": "source.parquet",
                "field": "TIME",
                "row": 1,
                "value": 1,
            },
        )

    def test_invalid_bytes_and_duplicate_fields_keep_native_refusals(self):
        with self.assertRaises(native.ParquetProfileFailure) as caught:
            native.parquet_source_fields(b"PAR1")
        self.assertEqual(
            (
                caught.exception.condition,
                caught.exception.requirement,
                caught.exception.context,
            ),
            (
                "source_parquet_invalid",
                "REQ-1038",
                {"dataset": "SOURCE", "path": "source.parquet"},
            ),
        )
        duplicate = pa.Table.from_arrays(
            [pa.array([1], pa.int64()), pa.array([2], pa.int64())], names=["ID", "ID"]
        )
        with self.assertRaises(native.ParquetProfileFailure) as caught:
            native.parquet_source_fields(encoded(duplicate))
        self.assertEqual(caught.exception.condition, "source_field_name_duplicate")
        self.assertEqual(
            caught.exception.context,
            {"dataset": "SOURCE", "path": "source.parquet", "field": "ID"},
        )

    def test_generated_fixture_ordinal_collision_preserves_real_stored_name(self):
        content = encoded(pa.table({"ValidatorOrdinal": pa.array([], pa.int64())}))
        _, fields = native.parquet_source_fields(content)
        self.assertEqual(fields, (("ValidatorOrdinal", "int"),))

    def test_generated_field_collision_cannot_bypass_source_value_admission(self):
        content = encoded(
            pa.table(
                {
                    "ValidatorOrdinal": pa.array([1], pa.int64()),
                    "TIME": pa.array([1], pa.int64()).cast(pa.timestamp("us")),
                }
            )
        )
        with self.assertRaises(native.ParquetProfileFailure) as caught:
            native.parquet_source_fields(content)
        self.assertEqual(caught.exception.condition, "source_field_value_invalid")


if __name__ == "__main__":
    unittest.main()
