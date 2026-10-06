"""Exercise real regex probe JSON under a simulated non-UTF-8 subprocess default."""

import contextlib
import io
import shutil
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parents[1] / "tools"))
import check_regex
import compare_regex_node


class RegexEncodingTests(unittest.TestCase):
    """Raw UTF-8 captures must survive independently of the caller's ANSI locale."""

    def test_independent_gate_with_ansi_default(self):
        """The actual Rust child emits non-ASCII JSON, including an undefined CP1252 byte."""
        cases = [
            (
                f"encoding-{ord(subject)}",
                "[^]",
                subject,
                check_regex.exact(full=True, match=[subject]),
                "exact",
            )
            for subject in ("\u0085", "\U0010ffff")
        ]
        # Control only subprocess's implicit encoding selection. An omitted UTF-8
        # argument recreates the Windows failure while the real child still runs.
        with (
            patch("subprocess._text_encoding", return_value="cp1252"),
            patch.object(check_regex, "cases", return_value=cases),
            contextlib.redirect_stdout(io.StringIO()),
        ):
            check_regex.main()

    @unittest.skipUnless(shutil.which("node"), "supplemental Node oracle unavailable")
    def test_supplemental_comparison_with_ansi_default(self):
        """Both the Rust and Node JSON streams preserve the exact scalar values."""
        cases = [
            {"pattern": "[^]", "subject": subject}
            for subject in ("\u0085", "\U0010ffff")
        ]
        with (
            patch("subprocess._text_encoding", return_value="cp1252"),
            patch.object(compare_regex_node, "requests", return_value=cases),
            patch.object(sys, "argv", ["compare_regex_node.py"]),
            contextlib.redirect_stdout(io.StringIO()),
            self.assertRaises(SystemExit) as result,
        ):
            compare_regex_node.main()
        self.assertEqual(result.exception.code, 0)
