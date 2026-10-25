"""Project independently authored assertion truth onto the installed-host issue shape."""

import json
import tempfile
import unittest
from pathlib import Path

from tools.paths import ROOT
from tools.staging import stage_specification_inputs as stage


class CompletedAssertionStage(unittest.TestCase):
    def test_truth_identity_and_complete_r_records_survive_staging(self):
        fixture = (
            ROOT
            / "rust/crates/yamaa-application/tests/fixtures/completed_lookup_assertions.json"
        )
        truth = json.loads(fixture.read_text())
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            stage.stage_completed_lookup_assertion_truth(root)
            label = "completed-lookup-assertions"
            staged = json.loads((root / (label + "-truth.json")).read_text())
            self.assertEqual(staged, stage.host_truth(fixture))
            for case in staged["cases"].values():
                for row in case["issues"] + case["check_issues"]:
                    self.assertEqual(set(row), set(stage.HOST_ISSUE_FIELDS))
            paths = json.loads((root / (label + "-case-paths.json")).read_text())
            self.assertEqual(list(paths), list(truth["cases"]))
            self.assertEqual(
                list(paths.values()), [f"{index:04d}" for index in range(1, 22)]
            )
            r_truth = (root / (label + "-truth.R")).read_text()
            for name, case in truth["cases"].items():
                directory = root / (label + "-cases") / paths[name]
                self.assertEqual(
                    (directory / "spec.yaml").read_text(), case["specification"]
                )
                for filename, contents in case["inputs"].items():
                    self.assertEqual(
                        (directory / filename).read_bytes(), contents.encode("ascii")
                    )
                records = [
                    {**record, "specification": "__SPECIFICATION__"}
                    for record in case["verifications"]
                ]
                self.assertIn(
                    json.dumps(
                        json.dumps(records, sort_keys=True, separators=(",", ":"))
                    ),
                    r_truth,
                )
                if "csv" in case:
                    self.assertEqual(
                        (directory / "expected.csv").read_bytes(),
                        case["csv"].encode("ascii"),
                    )
                for file in directory.iterdir():
                    archived = (
                        "yamaa/inst/specification-original/"
                        + file.relative_to(root).as_posix()
                    )
                    self.assertLessEqual(len(archived.encode("ascii")), 100, archived)

    def test_host_projection_drops_adapter_geometry_and_refuses_other_fields(self):
        row = {
            "condition": "finding",
            "context": {"left_type": "int", "right_type": "str"},
            "phase": "validation",
            "rule": "FORMULA-5",
            "spec_paths": ["verifications[1].assert.when"],
        }
        geometry = {"source_span": {"start": 0, "end": 4}, "operand_route": []}
        projected = stage.host_issue_rows([{**row, **geometry}, row])
        self.assertEqual(projected, [row, row])
        # Authored key order survives so geometry-free truth keeps its bytes.
        self.assertEqual(list(projected[0]), list(row))
        with self.assertRaises(ValueError):
            stage.host_issue_rows([{**row, "severity": "error"}])
        with self.assertRaises(ValueError):
            stage.host_issue_rows([{key: row[key] for key in row if key != "context"}])


if __name__ == "__main__":
    unittest.main()
