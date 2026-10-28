"""Installed Parquet reading: the types pyarrow writes by default read.

pyarrow 26 wrote this family's file from a table with a timestamp in
nanoseconds, one in milliseconds and a single-precision float, the types
pandas and pyarrow write by default. Each reads as its column type
(PARQUET-7): the timestamps as the whole seconds they count (PARQUET-9) and
the float as the binary64 value it names, so the saved CSV holds that value's
text. R reads the same truth.
"""

import tempfile
import unittest
from pathlib import Path

from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "parquet-writer-defaults")


class ParquetWriterDefaults(unittest.TestCase):
    def test_the_types_pyarrow_writes_by_default_read_and_save(self):
        with tempfile.TemporaryDirectory(prefix="yamaa-parquet-defaults-") as work:
            directory = Path(work)
            spec = directory / "spec.yaml"
            spec.write_text(TRUTH["specification_yaml"], encoding="utf-8")
            (directory / "input.parquet").write_bytes(
                bytes.fromhex(TRUTH["writer_defaults_parquet"]["hex"])
            )
            result = yamaa.spec(spec).run()
            self.assertEqual(result.issues.to_pylist(), [])
            result.save()
            self.assertEqual(
                (directory / "output.csv").read_bytes(),
                TRUTH["saved_csv"].encode("utf-8"),
            )


if __name__ == "__main__":
    unittest.main()
