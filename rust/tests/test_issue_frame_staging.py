"""Literal R issue expectations retain independent JSON truth and Unicode code points."""

import csv
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "stage_specification_inputs", Path(__file__).resolve().parents[1] / "tools/stage_specification_inputs.py"
)
stage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stage)


class IssueFrameTruth(unittest.TestCase):
    def test_literal_projection_preserves_nulls_lists_full_integers_and_unicode(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "expected").mkdir()
            diagnostic = dict(phase="derivation",condition="integer_overflow",requirement=None,
                              spec_paths=["columns.é🙂", "quote\"slash\\"],
                              context={"minimum": -(2**63), "maximum": 2**63-1, "text": "é🙂\n\0"})
            (root / "expected/case.json").write_text(json.dumps({"diagnostics": [diagnostic]}))
            with (root / "static-verification-checks.tsv").open("w", newline="") as stream:
                writer = csv.DictWriter(stream,fieldnames=["case","expected"],delimiter="\t")
                writer.writeheader()
                writer.writerow(dict(case="empty",expected="[]"))
            with patch.object(stage,"CASES",("case",)):
                stage.stage_issue_frame_truth(root)
            actual = (root / "issue-frame-truth.R").read_text()
            self.assertIn('requirement=c(NA_character_)', actual)
            self.assertIn('spec_paths=list(c("columns.\\u00e9\\U0001f642","quote\\"slash\\\\"))', actual)
            self.assertIn('-9223372036854775808', actual)
            self.assertIn('9223372036854775807', actual)
            self.assertIn('"static/empty"=list(phase=character(),condition=character(),requirement=character(),spec_paths=list(),context=character())', actual)
            self.assertNotIn('\\ud83d', actual)
            self.assertTrue(actual.isascii())


if __name__ == "__main__":
    unittest.main()
