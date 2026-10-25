"""Installed Parquet refusal: an invalid file is one finding, and silent.

Two files of this family are invalid. In one, an INT96 column statistic holds
13 bytes where the type's statistics hold 12. In the other, two row groups
name one page whose header holds fields that parquet 55 could not skip, which
its path refused and the native reader still refuses. The engine refuses each
as `source_parquet_invalid` (PARQUET-5), prints nothing, and later reads in
the same process still work. R reads the same truth.
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
INVALID = ("long_int96_statistic_parquet", "shared_presented_page_parquet")
# Runs each specification in a fresh interpreter, whose stderr is the one a
# panic hook or a warning would write to, saves a run that has output, and
# prints each run's conditions.
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
        for name in (*INVALID, "readable_parquet"):
            directory = Path(temporary.name) / name
            directory.mkdir()
            spec = directory / "spec.yaml"
            spec.write_text(TRUTH["specification_yaml"], encoding="utf-8")
            (directory / "input.parquet").write_bytes(bytes.fromhex(TRUTH[name]["hex"]))
            self.specs[name] = spec
        self.saved = self.specs["readable_parquet"].with_name("output.csv")

    def test_an_invalid_file_is_one_finding_and_later_reads_still_work(self):
        for name in INVALID:
            with self.subTest(case=name):
                self.assert_invalid_then_readable(self.specs[name])

    def assert_invalid_then_readable(self, spec):
        # A truth row names its file relative to the specification's
        # directory; a location names it by its absolute path (P-ISSUE-1).
        directory = spec.parent.resolve()
        expected = [
            {**row, "file": (directory / row["file"]).as_posix()}
            for row in TRUTH["issues"]
        ]
        for _ in range(2):
            result = yamaa.spec(spec).run()
            self.assertEqual(result.issues.to_pylist(), expected)
            self.assertIsNone(result.output)
            with self.assertRaises(yamaa.Error):
                result.save()
            result = yamaa.spec(self.specs["readable_parquet"]).run()
            self.assertEqual(result.issues.num_rows, 0)
            result.save()
            self.assertEqual(
                self.saved.read_bytes(), TRUTH["readable_csv"].encode("utf-8")
            )

    def test_refusing_the_invalid_files_prints_nothing(self):
        child = subprocess.run(
            [
                sys.executable,
                "-I",
                "-c",
                CHILD,
                *(str(self.specs[name]) for name in INVALID),
                str(self.specs["readable_parquet"]),
            ],
            capture_output=True,
            check=False,
            timeout=600,
        )
        self.assertEqual(child.returncode, 0, child.stderr.decode("utf-8", "replace"))
        self.assertEqual(child.stderr.decode("utf-8", "replace"), "")
        conditions = [issue["condition"] for issue in TRUTH["issues"]]
        self.assertEqual(json.loads(child.stdout), [conditions, conditions, []])
        self.assertEqual(self.saved.read_bytes(), TRUTH["readable_csv"].encode("utf-8"))


if __name__ == "__main__":
    unittest.main()
