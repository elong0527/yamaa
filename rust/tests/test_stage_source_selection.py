"""Portable staged paths retain exact independently authored source truth."""

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


class PortableSourceSelectionStage(unittest.TestCase):
    def test_each_identity_has_one_portable_directory_and_unchanged_truth(self):
        for fixture, label in (
            ("source_selection", "source-selection"),
            ("source_filter_scope", "source-filter-scope"),
            ("source_filter_mixed", "source-filter-mixed"),
            ("completed_lookup_assertions", "completed-lookup-assertions"),
        ):
            with (
                self.subTest(fixture=fixture),
                tempfile.TemporaryDirectory() as temporary,
            ):
                destination = Path(temporary)
                original = (
                    ROOT
                    / "rust/crates/yamaa-adapters/tests/fixtures"
                    / (fixture + ".json")
                )
                truth = json.loads(original.read_text())
                stage.stage_source_selection_truth(destination, fixture, label)
                self.assertEqual(
                    (destination / (label + "-truth.json")).read_bytes(),
                    original.read_bytes(),
                )
                paths = json.loads(
                    (destination / (label + "-case-paths.json")).read_text()
                )
                self.assertEqual(list(paths), list(truth["cases"]))
                self.assertEqual(
                    list(paths.values()),
                    [f"{index:04d}" for index in range(1, len(paths) + 1)],
                )
                self.assertEqual(len(set(paths.values())), len(paths))
                r_truth = (destination / (label + "-truth.R")).read_text()
                for name, case in truth["cases"].items():
                    directory = destination / (label + "-cases") / paths[name]
                    self.assertIn(
                        json.dumps(name) + "=list(directory=" + json.dumps(paths[name]),
                        r_truth,
                    )
                    self.assertEqual(
                        (directory / "spec.yaml").read_text(), case["specification"]
                    )
                    for path, content in case["inputs"].items():
                        self.assertEqual(
                            (directory / path).read_bytes(), content.encode("ascii")
                        )
                    if "csv" in case:
                        self.assertEqual(
                            (directory / "expected.csv").read_bytes(),
                            case["csv"].encode("ascii"),
                        )
                    for file in directory.iterdir():
                        archive_path = (
                            "yamaa/inst/specification-original/"
                            + file.relative_to(destination).as_posix()
                        )
                        self.assertLessEqual(
                            len(archive_path.encode("ascii")), 100, archive_path
                        )


if __name__ == "__main__":
    unittest.main()
