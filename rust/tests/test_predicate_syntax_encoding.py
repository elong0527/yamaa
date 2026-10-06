"""The syntax probe must preserve Unicode independently of host locale defaults."""

import contextlib
import io
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parents[1] / "tools"))
import check_predicate_syntax


class PredicateSyntaxEncodingTests(unittest.TestCase):
    """Exercise the actual Rust transport under a simulated Windows ANSI default."""

    def test_reference_gate_with_ansi_default(self):
        """Raw non-ASCII AST strings must retain exact scalars and source positions."""
        with (
            patch("subprocess._text_encoding", return_value="cp1252"),
            contextlib.redirect_stdout(io.StringIO()),
        ):
            check_predicate_syntax.main()
