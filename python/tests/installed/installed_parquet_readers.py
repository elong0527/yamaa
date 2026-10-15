"""Independent installed readers must decode the same public Save bytes."""

import tempfile
import unittest
from pathlib import Path

import polars as pl
import pyarrow as pa
import pyarrow.parquet as pq

import yamaa

SPEC = """schema_version: '1.0'
domain: TEST
keys: [ID]
input:
  SRC: input.csv
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: COUNT, type: int, derivation: SRC.COUNT}
  - {name: VALUE, type: float, derivation: SRC.VALUE}
output:
  path: output.parquet
  columns: [ID, COUNT, VALUE]
"""
INPUT = b"ID,COUNT,VALUE\nA,1,1.25\nB,-2,-2.5\nC,0,0.0\nD,,\n"
SCHEMA = {"ID": pl.String, "COUNT": pl.Int64, "VALUE": pl.Float64}
ARROW_SCHEMA = pa.schema(
    [
        pa.field("ID", pa.string()),
        pa.field("COUNT", pa.int64()),
        pa.field("VALUE", pa.float64()),
    ]
)
VALUES = {
    "ID": ["A", "B", "C", "D"],
    "COUNT": [1, -2, 0, None],
    "VALUE": [1.25, -2.5, 0.0, None],
}


class ParquetReaders(unittest.TestCase):
    def test_public_save_is_readable_with_float_and_missing_values(self):
        with tempfile.TemporaryDirectory(prefix="yamaa-parquet-readers-") as directory:
            root = Path(directory)
            specification = root / "spec.yaml"
            specification.write_text(SPEC, encoding="ascii")
            source = root / "input.csv"
            source.write_bytes(INPUT)
            output = root / "output.parquet"

            built = yamaa.domain(specification)
            self.assertTrue(built.issues.is_empty(), built.issues)
            self.assertEqual(built.output.schema, SCHEMA)
            self.assertEqual(built.output.to_dict(as_series=False), VALUES)
            self.assertFalse(output.exists())
            specification.unlink()
            source.unlink()

            self.assertTrue(built.save())
            saved = output.read_bytes()
            for _ in range(2):
                frame = pl.read_parquet(output)
                self.assertEqual(frame.schema, SCHEMA)
                self.assertEqual(frame.to_dict(as_series=False), VALUES)
                table = pq.read_table(pa.BufferReader(saved))
                self.assertEqual(table.schema, ARROW_SCHEMA)
                self.assertEqual(table.to_pydict(), VALUES)
                self.assertTrue(built.save())
                self.assertEqual(output.read_bytes(), saved)


if __name__ == "__main__":
    unittest.main()
