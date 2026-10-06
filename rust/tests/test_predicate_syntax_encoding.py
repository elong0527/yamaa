"""The syntax probe must preserve Unicode independently of host locale defaults."""

import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parents[1] / "tools"))
import check_predicate_syntax


class PredicateSyntaxEncodingTests(unittest.TestCase):
    """Exercise the actual Rust transport without requiring the reference package."""

    def test_probe_with_ansi_default(self):
        """Raw non-ASCII AST strings retain exact scalars and original character sites."""
        subjects = ["\u0085", "\u2028", "\u2029", "\U0010ffff"]
        with patch("subprocess._text_encoding", return_value="cp1252"):
            responses = check_predicate_syntax.probe(
                [f"'{subject}' = 'e\u0301'" for subject in subjects]
            )
        for subject, response in zip(subjects, responses, strict=True):
            self.assertEqual(
                response,
                {
                    "protocol": "predicate-syntax/1",
                    "outcome": {
                        "status": "parsed",
                        "identifiers": [],
                        "ast": {
                            "kind": "comparison",
                            "operator": "=",
                            "left": {
                                "kind": "literal",
                                "type": "str",
                                "value": subject,
                                "position": 0,
                            },
                            "right": {
                                "kind": "literal",
                                "type": "str",
                                "value": "e\u0301",
                                "position": 6,
                            },
                        },
                    },
                },
            )
