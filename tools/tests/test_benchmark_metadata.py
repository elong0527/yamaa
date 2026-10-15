"""Independent benchmark metadata and call-selection refusal controls."""

import copy
import tempfile
import unittest
from pathlib import Path

import yaml
from tools import benchmark


class BenchmarkMetadata(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.case = Path(temporary.name) / "case"
        self.case.mkdir()
        (self.case / "spec.yaml").write_text("domain: TEST\n")
        self.metadata = {"lifecycle": "draft", "standard": "TEST", "domain": "TEST"}
        self.write()

    def write(self, metadata=None):
        (self.case / "benchmark.yaml").write_text(
            yaml.safe_dump(self.metadata if metadata is None else metadata)
        )

    def test_singleton_and_host_environment_defaults_are_explicit_in_plan(self):
        (self.case / "environment.yaml").write_text("root is never automatic\n")
        self.assertEqual(
            benchmark.entries(self.case),
            {
                "python": [{"specification": "spec.yaml", "environment": None}],
                "r": [{"specification": "spec.yaml", "environment": None}],
            },
        )
        (self.case / "r").mkdir()
        (self.case / "r/environment.yaml").write_text("host fixture\n")
        self.assertEqual(
            benchmark.entries(self.case)["r"],
            [{"specification": "spec.yaml", "environment": "r/environment.yaml"}],
        )
        self.assertIsNone(benchmark.entries(self.case)["python"][0]["environment"])

    def test_authored_order_and_environment_override_are_preserved_per_host(self):
        (self.case / "other.yaml").write_text("domain: OTHER\n")
        (self.case / "custom.yaml").write_text("parameters: {cutoff: 42}\n")
        self.metadata["entries"] = {
            "python": [
                {"specification": "other.yaml", "environment": "custom.yaml"},
                {"specification": "spec.yaml", "environment": None},
            ],
            "r": [
                {"specification": "spec.yaml", "environment": None},
                {"specification": "other.yaml", "environment": "custom.yaml"},
            ],
        }
        self.write()
        self.assertEqual(benchmark.entries(self.case), self.metadata["entries"])
        self.assertEqual(
            (self.case / "custom.yaml").read_text(), "parameters: {cutoff: 42}\n"
        )

    def test_explicit_null_disables_an_existing_host_environment(self):
        (self.case / "python").mkdir()
        (self.case / "python/environment.yaml").write_text("host fixture\n")
        self.metadata["entries"] = {
            "python": [{"specification": "spec.yaml", "environment": None}],
            "r": [{"specification": "spec.yaml"}],
        }
        self.write()
        self.assertIsNone(benchmark.entries(self.case)["python"][0]["environment"])

    def test_metadata_is_required_and_legacy_prose_or_scripts_cannot_supply_it(self):
        (self.case / "benchmark.yaml").unlink()
        (self.case / "README.md").write_text("**Standard:** TEST | **Domain:** TEST\n")
        (self.case / "run.py").write_text('yamaa.domain("spec.yaml")\n')
        self.assertEqual(benchmark.directories(self.case.parent), [self.case])
        with self.assertRaisesRegex(ValueError, "benchmark.yaml"):
            benchmark.entries(self.case)

    def test_multifile_cases_cannot_guess_an_entry_or_drop_a_host(self):
        (self.case / "spec_parent.yaml").write_text("domain: PARENT\n")
        with self.assertRaisesRegex(ValueError, "explicit entries"):
            benchmark.entries(self.case)
        self.metadata["entries"] = {"python": [{"specification": "spec.yaml"}]}
        self.write()
        with self.assertRaisesRegex(ValueError, "Python and R"):
            benchmark.entries(self.case)

    def test_invalid_metadata_is_rejected_instead_of_defaulting(self):
        mutations = [
            None,
            [],
            {},
            {**self.metadata, "lifecycle": "released"},
            {**self.metadata, "lifecycle": ["draft"]},
            {**self.metadata, "domain": ""},
            {**self.metadata, "standard": 1},
            {**self.metadata, "run": "run.py"},
            {**self.metadata, "covers": ["flag", "flag"]},
            {**self.metadata, "covers": ["Flag"]},
            {**self.metadata, "covers": [None]},
            {**self.metadata, "validation": {"rule": "R007"}},
            {
                **self.metadata,
                "validation": {
                    "rule": "R007",
                    "validator": "schema",
                    "blocked_by": "#1",
                },
            },
            {**self.metadata, "validation": {"rule": "R7", "validator": "schema"}},
            {**self.metadata, "validation": {"rule": "R007", "blocked_by": "closed"}},
        ]
        for metadata in mutations:
            with self.subTest(metadata=metadata):
                (self.case / "benchmark.yaml").write_text(yaml.safe_dump(metadata))
                with self.assertRaises(ValueError):
                    benchmark.entries(self.case)

    def test_duplicate_metadata_keys_and_calls_are_rejected(self):
        (self.case / "benchmark.yaml").write_text(
            "lifecycle: draft\nstandard: TEST\ndomain: TEST\ndomain: OTHER\n"
        )
        with self.assertRaisesRegex(ValueError, "unique string keys"):
            benchmark.entries(self.case)
        self.metadata["entries"] = {
            "python": [{"specification": "spec.yaml"}] * 2,
            "r": [{"specification": "spec.yaml"}],
        }
        self.write()
        with self.assertRaisesRegex(ValueError, "duplicate python entry"):
            benchmark.entries(self.case)

    def test_entries_reject_missing_or_escaping_files_and_unknown_call_fields(self):
        for call in (
            {"specification": "missing.yaml"},
            {"specification": "../spec.yaml"},
            {"specification": "/spec.yaml"},
            {"specification": "spec.yaml", "environment": "missing.yaml"},
            {"specification": "spec.yaml", "parameters": {}},
            {"specification": 2},
        ):
            with self.subTest(call=call):
                self.metadata["entries"] = {
                    host: [copy.deepcopy(call)] for host in benchmark.HOSTS
                }
                self.write()
                with self.assertRaises(ValueError):
                    benchmark.entries(self.case)

    def test_metadata_only_case_cannot_disappear_or_release_without_a_specification(
        self,
    ):
        (self.case / "spec.yaml").unlink()
        self.assertEqual(benchmark.directories(self.case.parent), [self.case])
        with self.assertRaises(ValueError):
            benchmark.entries(self.case)


if __name__ == "__main__":
    unittest.main()
