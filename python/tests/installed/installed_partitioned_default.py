"""Installed runs beyond the row limit under the default partitions (#3794).

The Python host runs every build over key-bounded partitions by default, so
a CSV input of more records than a table may hold runs when its steps are
safe under those partitions. The truth is the shared `partitioned-default`
family: a study of 10,000,001 records, whose window numbers each subject's
records, and the line count and first and last lines of its saved CSV,
written by hand from its value rule. The R `partitioned_default` suite reads
it too.

The input and the run take minutes and gigabytes, so the suite runs only in
native qualification, whose supplemental inventory sets
`YAMAA_SUPPLEMENTAL_EVIDENCE`; pull request CI keeps the smaller control,
`installed_longitudinal_capacity`, which reads the default partitions from
its run's observations.
"""

import collections
import os
import tempfile
import unittest
from pathlib import Path

from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "partitioned-default")


@unittest.skipUnless(
    os.environ.get("YAMAA_SUPPLEMENTAL_EVIDENCE"),
    "10,000,001 records run only in native qualification's supplemental inventory",
)
class PartitionedDefault(unittest.TestCase):
    def test_a_subject_keyed_window_beyond_the_row_limit_builds_and_saves(self):
        records = TRUTH["records"]
        each = TRUTH["subject_records"]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "spec.yaml").write_text(TRUTH["specification"], encoding="ascii")
            with (root / "input.csv").open("w", encoding="ascii", newline="") as file:
                file.write(TRUTH["header"] + "\n")
                for start in range(0, records, 1_000_000):
                    file.write(
                        "".join(
                            f"S1,{record // each:08},{record % each + 1}\n"
                            for record in range(start, min(records, start + 1_000_000))
                        )
                    )
            built = yamaa.spec(root / "spec.yaml").run()
            self.assertEqual(built.issues.to_pylist(), [])
            built.save()
            del built
            first, last, count = [], collections.deque(maxlen=2), 0
            with (root / "out.csv").open(encoding="ascii", newline="") as saved:
                for line in saved:
                    count += 1
                    if len(first) < 3:
                        first.append(line.rstrip("\n"))
                    last.append(line.rstrip("\n"))
            self.assertEqual(
                (count, first, list(last)),
                (TRUTH["saved_lines"], TRUTH["first_lines"], TRUTH["last_lines"]),
                "the saved file",
            )


if __name__ == "__main__":
    unittest.main()
