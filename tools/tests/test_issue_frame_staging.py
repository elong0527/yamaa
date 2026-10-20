"""Literal R issue expectations retain independent JSON truth and Unicode code points."""

import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tools.staging import stage_specification_inputs as stage


class IssueFrameTruth(unittest.TestCase):
    def test_readable_json_fixtures_are_staged_without_reencoding(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            stage.stage_json_fixtures(root)
            sources = list(stage.FIXTURES.glob("*.json"))
            self.assertGreater(len(sources), 25)
            for source in sources:
                self.assertEqual((root / source.name).read_bytes(), source.read_bytes())
            self.assertFalse(list(root.glob("*.tsv")))
            decoder = json.loads((root / "yaml_transport.json").read_text())
            self.assertEqual(len(decoder), 18)
            self.assertTrue(any(isinstance(row["source"], dict) for row in decoder))

    def test_public_output_projection_uses_plain_r_values_and_exact_float_bytes(
        self,
    ):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "expected").mkdir()
            report = {
                "artifacts": [
                    {
                        "columns": ["I", "F", "D", "S"],
                        "types": ["int", "float", "date", "str"],
                    }
                ],
                "tables": [
                    {
                        "stage": "derived",
                        "columns": ["I", "F", "D", "S"],
                        "rows": [
                            [
                                {"type": "int", "value": "-9007199254740992"},
                                {"type": "float", "value": "8000000000000000"},
                                {"type": "date", "value": "1969-12-31"},
                                {"type": "str", "value": "\u00e9\U0001f642"},
                            ],
                            [{"type": "missing", "value": None}] * 4,
                        ],
                    }
                ],
            }
            (root / "expected/case.json").write_text(json.dumps(report))
            (root / "expected/failed.json").write_text('{"artifacts":[]}')
            with patch.object(stage, "CASES", ("case", "failed")):
                stage.stage_public_output_truth(root)
            actual = (root / "public-output-truth.R").read_text()
            self.assertIn('"I"=c(-9007199254740992,NA_real_)', actual)
            self.assertNotIn("yamaa_int64", actual)
            self.assertIn("as.raw(c(0,0,0,0,0,0,0,128))", actual)
            self.assertIn(
                "\"D\"=structure(as.double(c(-1,NA_real_)),class='Date')", actual
            )
            self.assertIn('"S"=c("\\u00e9\\U0001f642",NA_character_)', actual)
            self.assertIn('"failed"=NULL', actual)
            self.assertTrue(actual.isascii())
            # One beyond 2^53 has no exact R double, so no literal can hold it.
            cell = report["tables"][0]["rows"][0][0]
            cell["value"] = "9007199254740993"
            (root / "expected/case.json").write_text(json.dumps(report))
            with patch.object(stage, "CASES", ("case",)):
                with self.assertRaisesRegex(ValueError, "beyond 2\\^53"):
                    stage.stage_public_output_truth(root)

    def test_literal_projection_preserves_nulls_lists_full_integers_and_unicode(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "expected").mkdir()
            diagnostic = dict(
                phase="derivation",
                condition="integer_overflow",
                rule=None,
                spec_paths=["columns.\u00e9\U0001f642", 'quote"slash\\'],
                context={
                    "minimum": -(2**63),
                    "maximum": 2**63 - 1,
                    "text": "\u00e9\U0001f642\n\0",
                },
            )
            (root / "expected/case.json").write_text(
                json.dumps({"diagnostics": [diagnostic]})
            )
            (root / "static_verification_checks.json").write_text(
                json.dumps([{"case": "empty", "expected": []}])
            )
            with patch.object(stage, "CASES", ("case",)):
                stage.stage_issue_frame_truth(root)
            actual = (root / "issue-frame-truth.R").read_text()
            self.assertIn("rule=c(NA_character_)", actual)
            self.assertIn(
                'spec_paths=list(c("columns.\\u00e9\\U0001f642","quote\\"slash\\\\"))',
                actual,
            )
            self.assertIn("-9223372036854775808", actual)
            self.assertIn("9223372036854775807", actual)
            self.assertIn(
                '"static/empty"=list(phase=character(),condition=character(),rule=character(),spec_paths=list(),context=character())',
                actual,
            )
            self.assertNotIn("\\ud83d", actual)
            self.assertTrue(actual.isascii())


if __name__ == "__main__":
    unittest.main()
