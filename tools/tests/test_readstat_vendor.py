"""The vendored ReadStat patch check: upstream plus the patch is the tree.

The seam is `tools.ci.readstat`'s comparison, over a scratch upstream and a
scratch vendored tree, so no test fetches the real upstream.
"""

import contextlib
import io
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

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


class ReadStatFetch(unittest.TestCase):
    # The seam is `fetch_upstream`'s handling of a failed git call, which the
    # patch comparison never reaches; git itself is replaced, so nothing is
    # fetched.
    def test_a_failed_fetch_stops_with_one_line_naming_the_upstream_remedy(self):
        failure = subprocess.CompletedProcess(
            ("git", "fetch"), 128, b"", b"fatal: unable to access"
        )
        with (
            tempfile.TemporaryDirectory() as scratch,
            mock.patch.object(readstat.subprocess, "run", return_value=failure),
            contextlib.redirect_stderr(io.StringIO()),
            self.assertRaises(SystemExit) as stopped,
        ):
            readstat.fetch_upstream(
                "https://example.invalid/readstat", "104ba03a", Path(scratch) / "up"
            )
        self.assertEqual(
            stopped.exception.code,
            "::error::vendored ReadStat: cannot fetch upstream "
            "https://example.invalid/readstat at 104ba03a; without network "
            "access, check that commit out and pass --upstream <checkout>",
            "an offline fetch",
        )


if __name__ == "__main__":
    unittest.main()
