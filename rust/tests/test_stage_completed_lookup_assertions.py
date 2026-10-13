"""Preserve independently authored assertion bytes through portable host staging."""
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).parents[2]
spec = importlib.util.spec_from_file_location(
    "stage_specification_inputs", ROOT / "rust/tools/stage_specification_inputs.py"
)
stage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stage)


class CompletedAssertionStage(unittest.TestCase):
    def test_truth_identity_and_complete_r_records_survive_staging(self):
        fixture = ROOT / "rust/crates/yamaa-adapters/tests/fixtures/completed_lookup_assertions.json"
        truth = json.loads(fixture.read_text())
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            stage.stage_completed_lookup_assertion_truth(root)
            label = "completed-lookup-assertions"
            self.assertEqual((root / (label + "-truth.json")).read_bytes(), fixture.read_bytes())
            paths = json.loads((root / (label + "-case-paths.json")).read_text())
            self.assertEqual(list(paths), list(truth["cases"]))
            self.assertEqual(list(paths.values()), [f"{index:04d}" for index in range(1, 22)])
            r_truth = (root / (label + "-truth.R")).read_text()
            for name, case in truth["cases"].items():
                directory = root / (label + "-cases") / paths[name]
                self.assertEqual((directory / "spec.yaml").read_text(), case["specification"])
                for filename, contents in case["inputs"].items():
                    self.assertEqual((directory / filename).read_bytes(), contents.encode("ascii"))
                records = [{**record, "specification": "__SPECIFICATION__"} for record in case["verifications"]]
                self.assertIn(json.dumps(json.dumps(records, sort_keys=True, separators=(",", ":"))), r_truth)
                if "csv" in case:
                    self.assertEqual((directory / "expected.csv").read_bytes(), case["csv"].encode("ascii"))
                for file in directory.iterdir():
                    archived = "yamaa/inst/specification-original/" + file.relative_to(root).as_posix()
                    self.assertLessEqual(len(archived.encode("ascii")), 100, archived)


if __name__ == "__main__":
    unittest.main()
