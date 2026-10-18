"""Independent benchmark metadata and call-selection refusal controls."""

import copy
import shutil
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

    def test_runtime_staging_supplies_the_actual_function_environment_lock(self):
        from tools.paths import ROOT
        from tools.staging.stage_project_inputs import stage_case

        source = ROOT / "benchmarks/adam-adsl-bmi"
        staged = self.case / "runtime"
        stage_case(
            source,
            staged,
            ignore=shutil.ignore_patterns("expected", "README.md", "benchmark.yaml"),
        )
        self.assertEqual(
            (staged / "python/uv.lock").read_bytes(),
            (ROOT / "python/uv.lock").read_bytes(),
        )
        self.assertFalse((source / "python/uv.lock").exists())
        for name in ("expected", "README.md", "benchmark.yaml"):
            self.assertFalse((staged / name).exists())
        self.assertEqual(
            (staged / "input/adsl.csv").read_bytes(),
            (source / "input/adsl.csv").read_bytes(),
        )

    def test_runtime_staging_follows_the_python_environment_the_project_links(self):
        from tools.paths import ROOT
        from tools.staging.stage_project_inputs import stage_case

        source = self.case / "study"
        (source / "code/py").mkdir(parents=True)
        (source / "spec.yaml").write_text("domain: TEST\n")
        (source / "benchmark.yaml").write_text(yaml.safe_dump(self.metadata))
        (source / "project.yaml").write_text(
            "schema_version: '1.0'\n"
            "environments: {python: code/py/environment.yaml, r: r/environment.yaml}\n"
        )
        (source / "code/py/environment.yaml").write_text("schema_version: '1.0'\n")
        staged = self.case / "staged"
        stage_case(source, staged)
        self.assertEqual(
            (staged / "code/py/uv.lock").read_bytes(),
            (ROOT / "python/uv.lock").read_bytes(),
        )
        self.assertFalse((staged / "uv.lock").exists())
        self.assertFalse((staged / "r").exists())
        (source / "project.yaml").write_text("schema_version: '1.0'\n")
        shutil.rmtree(staged)
        stage_case(source, staged)
        self.assertFalse((staged / "code/py/uv.lock").exists())

    def test_singleton_and_root_project_defaults_are_explicit_in_plan(self):
        (self.case / "environment.yaml").write_text("root is never automatic\n")
        (self.case / "r").mkdir()
        (self.case / "r/environment.yaml").write_text("host fixture\n")
        self.assertEqual(
            benchmark.entries(self.case),
            {
                "python": [{"specification": "spec.yaml", "environment": None}],
                "r": [{"specification": "spec.yaml", "environment": None}],
            },
        )
        (self.case / "project.yaml").write_text("study project\n")
        self.assertEqual(
            benchmark.entries(self.case),
            {
                host: [{"specification": "spec.yaml", "environment": "project.yaml"}]
                for host in ("python", "r")
            },
        )

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

    def test_explicit_null_disables_an_existing_root_project(self):
        (self.case / "project.yaml").write_text("study project\n")
        self.metadata["entries"] = {
            "python": [{"specification": "spec.yaml", "environment": None}],
            "r": [{"specification": "spec.yaml"}],
        }
        self.write()
        entries = benchmark.entries(self.case)
        self.assertIsNone(entries["python"][0]["environment"])
        self.assertEqual(entries["r"][0]["environment"], "project.yaml")

    def test_metadata_is_required_and_legacy_prose_or_scripts_cannot_supply_it(self):
        (self.case / "benchmark.yaml").unlink()
        (self.case / "README.md").write_text("**Standard:** TEST | **Domain:** TEST\n")
        (self.case / "run.py").write_text('yamaa.spec("spec.yaml").run()\n')
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
            {**self.metadata, "harbor": {}},
            {**self.metadata, "harbor": {"opensas_exclusion": " "}},
            {**self.metadata, "harbor": {"opensas_exclusion": True}},
            {**self.metadata, "harbor": {"skip": "unregistered exclusion"}},
            {**self.metadata, "conformance": {"excluded": ""}},
            {**self.metadata, "conformance": {"metamorphic": []}},
            {
                **self.metadata,
                "conformance": {"metamorphic": {"unknown": {"reason": "by design"}}},
            },
            {
                **self.metadata,
                "conformance": {"metamorphic": {"empty_child": {"permanent": True}}},
            },
            {
                **self.metadata,
                "conformance": {
                    "metamorphic": {
                        "empty_child": {"reason": "by design", "permanent": "yes"}
                    }
                },
            },
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

    def test_project_name_is_the_domain_the_layers_compose(self):
        (self.case / "common.yaml").write_text("domain: COMMON\n")
        (self.case / "a.yaml").write_text("parents: common.yaml\ndomain: A\n")
        (self.case / "b.yaml").write_text("parents: common.yaml\n")
        entry = self.case / "entry.yaml"
        # Common -> A -> B -> entry: B adds no domain, so A's replaces Common's.
        entry.write_text("parents: [a.yaml, b.yaml]\n")
        self.assertEqual(benchmark.specification_name(entry), "A")
        entry.write_text("parents: [a.yaml, b.yaml]\ndomain: NO\n")
        self.assertEqual(benchmark.specification_name(entry), "NO")
        (self.case / "common.yaml").write_text("parents: entry.yaml\n")
        with self.assertRaisesRegex(ValueError, "cyclic parents"):
            benchmark.specification_name(entry)
        (self.case / "lone.yaml").write_text("keys: [ID]\n")
        with self.assertRaisesRegex(ValueError, "domain"):
            benchmark.specification_name(self.case / "lone.yaml")

    def test_metadata_only_case_cannot_disappear_or_release_without_a_specification(
        self,
    ):
        (self.case / "spec.yaml").unlink()
        self.assertEqual(benchmark.directories(self.case.parent), [self.case])
        with self.assertRaises(ValueError):
            benchmark.entries(self.case)


if __name__ == "__main__":
    unittest.main()
