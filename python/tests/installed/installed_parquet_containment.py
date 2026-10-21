"""Installed Parquet containment: a decoder panic is one finding, and silent.

parquet 55.2.0's metadata decoder panics on an INT96 statistic longer than 12
bytes. The engine contains that panic as `source_parquet_invalid` (PARQUET-5),
prints nothing, and later reads in the same process still work. R reads the
same truth.
"""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "parquet-containment")
# Runs each specification in a fresh interpreter, whose stderr is the one a
# panic hook writes to, saves a run that has output, and prints each run's
# conditions.
CHILD = """
import json, sys
import yamaa
conditions = []
for path in sys.argv[1:]:
    run = yamaa.spec(path).run()
    conditions.append(run.issues.to_polars()["condition"].to_list())
    if run.output is not None:
        run.save()
print(json.dumps(conditions))
"""


class ParquetContainment(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="yamaa-parquet-containment-")
        self.addCleanup(temporary.cleanup)
        self.specs = {}
        for name in ("contained_panic_parquet", "readable_parquet"):
            directory = Path(temporary.name) / name
            directory.mkdir()
            spec = directory / "spec.yaml"
            spec.write_text(TRUTH["specification_yaml"], encoding="utf-8")
            (directory / "input.parquet").write_bytes(bytes.fromhex(TRUTH[name]["hex"]))
            self.specs[name] = spec
        self.saved = self.specs["readable_parquet"].with_name("output.csv")

    def test_a_decoder_panic_is_an_invalid_source_and_later_reads_still_work(self):
        for _ in range(2):
            result = yamaa.spec(self.specs["contained_panic_parquet"]).run()
            self.assertEqual(result.issues.to_pylist(), TRUTH["issues"])
            self.assertIsNone(result.output)
            with self.assertRaises(yamaa.Error):
                result.save()
            result = yamaa.spec(self.specs["readable_parquet"]).run()
            self.assertEqual(result.issues.num_rows, 0)
            result.save()
            self.assertEqual(
                self.saved.read_bytes(), TRUTH["readable_csv"].encode("utf-8")
            )

    def test_a_contained_decoder_panic_prints_nothing(self):
        child = subprocess.run(
            [
                sys.executable,
                "-I",
                "-c",
                CHILD,
                str(self.specs["contained_panic_parquet"]),
                str(self.specs["readable_parquet"]),
            ],
            capture_output=True,
            check=False,
            timeout=600,
        )
        self.assertEqual(child.returncode, 0, child.stderr.decode("utf-8", "replace"))
        self.assertEqual(child.stderr.decode("utf-8", "replace"), "")
        conditions = [issue["condition"] for issue in TRUTH["issues"]]
        self.assertEqual(json.loads(child.stdout), [conditions, []])
        self.assertEqual(self.saved.read_bytes(), TRUTH["readable_csv"].encode("utf-8"))


if __name__ == "__main__":
    unittest.main()
