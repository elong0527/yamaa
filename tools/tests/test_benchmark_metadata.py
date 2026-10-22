"""Independent benchmark metadata and call-selection refusal controls."""

import copy
import io
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

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

        # The project names its Python lock at python/uv.lock in either
        # language: a 0.5 environment file's `lock: uv.lock` beside it, or a
        # 1.x project's environments.python.lock.
        source = ROOT / "benchmarks/schema-functions"
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
            with self.subTest(name=name):
                self.assertFalse((staged / name).exists())
        self.assertEqual(
            (staged / "input/source.csv").read_bytes(),
            (source / "input/source.csv").read_bytes(),
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

    def test_runtime_staging_supplies_the_lock_a_1_x_project_names(self):
        from tools.paths import ROOT
        from tools.staging.stage_project_inputs import python_lock, stage_case

        source = self.case / "study"
        source.mkdir()
        (source / "spec.yaml").write_text('yamaa: "1.0"\ntype: spec\ndomain: TEST\n')
        (source / "benchmark.yaml").write_text(yaml.safe_dump(self.metadata))
        project = source / "project.yaml"
        # The lock resolves from the project file, in a directory the study
        # does not hold yet (#3030 D8, FUNC-3).
        project.write_text(
            'yamaa: "1.0"\ntype: project\n'
            "environments: {python: {lock: code/py/uv.lock}, r: {lock: r/renv.lock}}\n"
        )
        self.assertEqual(str(python_lock(source, "project.yaml")), "code/py/uv.lock")
        staged = self.case / "staged"
        stage_case(source, staged)
        self.assertEqual(
            (staged / "code/py/uv.lock").read_bytes(),
            (ROOT / "python/uv.lock").read_bytes(),
        )
        self.assertFalse((staged / "r").exists())
        # A host that declares no lock has no entry, and nothing is staged.
        for environments in ("{r: {lock: r/renv.lock}}", "{}"):
            with self.subTest(environments=environments):
                project.write_text(
                    f'yamaa: "1.0"\ntype: project\nenvironments: {environments}\n'
                )
                self.assertIsNone(python_lock(source, "project.yaml"))
                shutil.rmtree(staged)
                stage_case(source, staged)
                self.assertFalse((staged / "code").exists())

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
            {**self.metadata, "validation": {"validator": "schema"}},
        ]
        for metadata in mutations:
            with self.subTest(metadata=metadata):
                (self.case / "benchmark.yaml").write_text(yaml.safe_dump(metadata))
                with self.assertRaises(ValueError):
                    benchmark.entries(self.case)

    def test_define_call_declares_its_project_section_and_created_time(self):
        (self.case / "project.yaml").write_text("schema_version: '1.0'\n")
        define = {
            "environment": "project.yaml",
            "standard": "adam",
            "created": "2026-09-11T00:00:00",
        }
        self.write({**self.metadata, "define": define})
        self.assertEqual(benchmark.load_metadata(self.case)["define"], define)
        for mutation in [
            None,
            "project.yaml",
            {**define, "standard": "SDTM"},
            {**define, "created": "2026-09-11"},
            {**define, "environment": "missing.yaml"},
            {**define, "environment": "../project.yaml"},
            {key: value for key, value in define.items() if key != "created"},
            {**define, "host": "python"},
        ]:
            with self.subTest(define=mutation):
                self.write({**self.metadata, "define": mutation})
                with self.assertRaises(ValueError):
                    benchmark.load_metadata(self.case)

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
        (self.case / "a.yaml").write_text("parent: common.yaml\ndomain: A\n")
        (self.case / "b.yaml").write_text("parent: a.yaml\n")
        entry = self.case / "entry.yaml"
        # Common -> A -> B -> entry: B adds no domain, so A's replaces Common's.
        entry.write_text("parent: b.yaml\n")
        self.assertEqual(benchmark.specification_name(entry), "A")
        entry.write_text("parent: b.yaml\ndomain: NO\n")
        self.assertEqual(benchmark.specification_name(entry), "NO")
        (self.case / "a.yaml").write_text("parent: common.yaml\n")
        entry.write_text("parent: b.yaml\n")
        self.assertEqual(benchmark.specification_name(entry), "COMMON")
        (self.case / "common.yaml").write_text("parent: entry.yaml\n")
        with self.assertRaisesRegex(ValueError, "cyclic parent"):
            benchmark.specification_name(entry)
        (self.case / "lone.yaml").write_text("keys: [ID]\n")
        with self.assertRaisesRegex(ValueError, "domain"):
            benchmark.specification_name(self.case / "lone.yaml")

    def test_a_model_standard_is_the_model_the_last_entries_declare(self):
        """benchmark.yaml keeps `standard` (#3030 plan, question 9); where it
        names a model, the 1.0 entry specification declares that model."""
        header = 'yamaa: "1.0"\ntype: spec\n'
        self.metadata["standard"] = "ADaM"
        self.write()
        for spec, agrees in (
            (header + "model: ADaM\ndomain: TEST\n", True),
            (header + "domain: TEST\n", True),
            ("schema_version: '1.0'\ndomain: TEST\n", True),
            (header + "model: SDTM\ndomain: TEST\n", False),
        ):
            (self.case / "spec.yaml").write_text(spec)
            with self.subTest(spec=spec):
                if agrees:
                    benchmark.check_standard_model(self.case)
                else:
                    with self.assertRaisesRegex(ValueError, "standard ADaM"):
                        benchmark.check_standard_model(self.case)
        # A parent layer may declare the model; the nearest layer wins.
        (self.case / "base.yaml").write_text(header + "model: SDTM\ndomain: TEST\n")
        (self.case / "spec.yaml").write_text(header + "parent: base.yaml\n")
        with self.assertRaisesRegex(ValueError, "model SDTM"):
            benchmark.check_standard_model(self.case)
        (self.case / "spec.yaml").write_text(
            header + "parent: base.yaml\nmodel: ADaM\n"
        )
        benchmark.check_standard_model(self.case)
        # A CDISC or TEST benchmark may build any model, and layers that
        # cannot compose leave a negative's fault to the engine.
        (self.case / "spec.yaml").write_text(header + "model: SEND\ndomain: TEST\n")
        for standard in ("CDISC", "TEST"):
            self.metadata["standard"] = standard
            self.write()
            benchmark.check_standard_model(self.case)
        self.metadata["standard"] = "SDTM"
        self.write()
        (self.case / "spec.yaml").write_text(header + "parent: missing.yaml\n")
        benchmark.check_standard_model(self.case)

    def test_a_project_that_fails_admission_reports_its_issues(self):
        from tools.paths import ROOT

        # The project refuses to name the spec: the run reports the project's
        # issues as a failed run does, not a traceback.
        case = ROOT / "benchmarks/negative-format-name-without-dollar"
        with (
            mock.patch("sys.stderr", new_callable=io.StringIO) as stderr,
            self.assertRaises(SystemExit) as exited,
        ):
            benchmark.main([str(case)])
        self.assertEqual(exited.exception.code, 1)
        self.assertTrue(stderr.getvalue().startswith("spec.yaml: "))
        self.assertIn("CT-12", stderr.getvalue())

    def test_metadata_only_case_cannot_disappear_or_release_without_a_specification(
        self,
    ):
        (self.case / "spec.yaml").unlink()
        self.assertEqual(benchmark.directories(self.case.parent), [self.case])
        with self.assertRaises(ValueError):
            benchmark.entries(self.case)


class PyYAMLRequirement(unittest.TestCase):
    def test_the_repository_yaml_directory_cannot_stand_in_for_pyyaml(self):
        from tools.paths import ROOT

        # Without site-packages there is no PyYAML, so `import yaml` from the
        # repository root finds its yaml/ directory, as in a job that never
        # installed PyYAML. The metadata reader names what it found instead of
        # failing on a missing loader (#3347).
        result = subprocess.run(
            [
                sys.executable,
                "-I",
                "-S",
                "-c",
                "import sys; sys.path.insert(0, sys.argv[1]); import tools.benchmark",
                str(ROOT),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(
            "ImportError: tools.benchmark needs PyYAML, but `import yaml` found "
            f"{ROOT / 'yaml'}, which is not PyYAML",
            result.stderr,
        )
        self.assertNotIn("AttributeError", result.stderr)


if __name__ == "__main__":
    unittest.main()
