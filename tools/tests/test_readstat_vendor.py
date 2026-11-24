"""The vendored ReadStat patch check: upstream plus the patch is the tree.

The seam is `tools.ci.readstat`'s comparison, over a scratch upstream and a
scratch vendored tree, so no test fetches the real upstream.
"""

import tempfile
import unittest
from pathlib import Path

from tools.ci import readstat
from tools.tests import support

UPSTREAM = {
    "LICENSE": "MIT\n",
    "README.md": "upstream's own readme\n",
    "src/readstat.h": "int a;\n",
    "src/sas/readstat_sas.c": "int b;\nint c;\n",
    "src/readstat_writer.c": "not vendored\n",
}
VENDORED = {
    "LICENSE": "MIT\n",
    "README.md": "yamaa's description\n",
    "readstat.h": "int a;\n",
    "sas/readstat_sas.c": "int b;\nint c = 1;\n",
    "yamaa_bridge.c": "int bridge;\n",
}


class ReadStatPatch(unittest.TestCase):
    def test_a_change_after_the_patch_is_written_is_reported_by_its_file(self):
        cases = (
            ("no change", {}, []),
            ("the description", {"README.md": "edited\n"}, []),
            (
                "an upstream file",
                {"readstat.h": "int a2;\n"},
                ["readstat.h: upstream plus the patch differs from the vendored file"],
            ),
            (
                "a local file",
                {"yamaa_bridge.c": "int bridge2;\n"},
                [
                    "yamaa_bridge.c: upstream plus the patch differs from the "
                    "vendored file"
                ],
            ),
            (
                "a file outside the patch",
                {"iconv.h": "int shim;\n"},
                [
                    "iconv.h: in the vendored tree, not in upstream plus the patch",
                ],
            ),
        )
        for label, edits, expected in cases:
            with self.subTest(case=label), tempfile.TemporaryDirectory() as scratch:
                upstream = Path(scratch) / "upstream"
                vendored = Path(scratch) / "vendored"
                for relative, text in UPSTREAM.items():
                    support.write(upstream, relative, text)
                for relative, text in VENDORED.items():
                    support.write(vendored, relative, text)
                findings = readstat.check_patch(upstream, write=True, vendored=vendored)
                self.assertEqual(findings, [], label)
                for relative, text in edits.items():
                    support.write(vendored, relative, text)
                self.assertEqual(
                    readstat.check_patch(upstream, vendored=vendored), expected, label
                )

    def test_the_patch_names_no_blob_digest(self):
        with tempfile.TemporaryDirectory() as scratch:
            upstream = Path(scratch) / "upstream"
            vendored = Path(scratch) / "vendored"
            for relative, text in UPSTREAM.items():
                support.write(upstream, relative, text)
            for relative, text in VENDORED.items():
                support.write(vendored, relative, text)
            patch = readstat.generate(upstream, vendored)
        self.assertEqual(
            [line for line in patch.splitlines() if line.startswith("index ")],
            [],
            "the generated patch",
        )


if __name__ == "__main__":
    unittest.main()
