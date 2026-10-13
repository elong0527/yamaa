"""Public installed SDK qualification against independent original study truth."""

from __future__ import annotations

import copy
import json
import os
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import yamaa
from yamaa import _locked_functions as host

import yaml

ROOT = Path(__file__).parent / "project-original"
CASES = (
    "schema-functions",
    "schema-non-finite",
    "adam-adsl-bmi",
    "adam-advs-percentiles",
)


class PublicEnvironment(unittest.TestCase):
    def test_submission_version_schema_admission_precedes_any_artifact_or_study_read(self):
        os.chdir(self.directory)
        Path("spec.yaml").write_text(
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n"
            "input: {SRC: absent.csv}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\n"
            "output: {path: result.csv, columns: [ID]}\n"
        )
        for family, guide in (("sdtm", "SDTMIG"), ("adam", "ADaMIG"), ("send", "SENDIG")):
            prefix = (
                "schema_version: '1.0'\nlanguage: python\n"
                "study: {id: STUDY, name: Study, description: Study, protocol_name: Protocol}\n"
                f"{family}:\n  standard: {{name: {guide}, version: '1.0', status: Final}}\n"
                "  specs: [absent-spec.yaml]\n  define: absent/define.xml\n"
            )
            with (
                patch.object(host, "verify_versions", side_effect=AssertionError("unexpected activation")),
                patch.object(host, "resolve_callable", side_effect=AssertionError("unexpected binding")),
            ):
                for declared in (None, "'1.0'", "'future package build'"):
                    field = "" if declared is None else f"  source_system_version: {declared}\n"
                    Path("environment.yaml").write_text(prefix + field)
                    checked = yamaa.check("spec.yaml", environment="environment.yaml")
                    self.assertTrue(checked.issues.is_empty(), checked.issues)
                for declared in ("null", "true", "1", "1.5", "''"):
                    Path("environment.yaml").write_text(prefix + f"  source_system_version: {declared}\n")
                    checked = yamaa.check("spec.yaml", environment="environment.yaml")
                    self.assertEqual(checked.issues.height, 1, checked.issues)
                    self.assertIn(f"{family}.source_system_version", checked.issues["spec_paths"][0])
            self.assertEqual(sorted(p.name for p in self.directory.iterdir()), ["environment.yaml", "spec.yaml"])

    def test_selected_functionless_producer_failure_retains_actual_source_prefix(self):
        os.chdir(self.directory)
        (self.directory / "environment.yaml").write_text("schema_version: '1.0'\nlanguage: python\n")
        (self.directory / "producer.yaml").write_text(
            "schema_version: '1.0'\ndomain: PROD\nkeys: [ID]\n"
            "input: {RAW: never-read.csv}\n"
            "columns: [{name: ID, type: int, label: Identifier, derivation: RAW.ID}]\n"
            "output: {path: produced.csv, columns: [ID], decimals: 2}\n"
        )
        (self.directory / "spec.yaml").write_text(
            "schema_version: '1.0'\ndomain: CONS\nkeys: [ID]\n"
            "input: {P: {path: produced.csv, schema: producer.yaml}}\n"
            "columns: [{name: ID, type: int, label: Identifier, derivation: P.ID}]\n"
            "output: {path: consumer.csv, columns: [ID]}\n"
        )
        with (
            patch.object(host, "verify_versions", side_effect=AssertionError("unexpected activation")),
            patch.object(host, "resolve_callable", side_effect=AssertionError("unexpected binding")),
        ):
            checked = yamaa.check("spec.yaml", environment="environment.yaml")
            failed = yamaa.domain("spec.yaml", environment="environment.yaml")
        self.assertTrue(checked.issues.is_empty(), checked.issues)
        self.assertEqual(failed.issues["condition"].to_list(), ["resource_path_missing"])
        self.assertEqual(failed.issues["spec_paths"].to_list(), [["input.RAW.path"]])
        self.assertEqual(failed._native.producer_attempt().entered_nodes(), [1])
        self.assertEqual(self.report(failed)["source_reads"], [])
        self.assertIsNone(failed.output)
        with self.assertRaises(yamaa.DomainError):
            failed.save()
        self.assertFalse((self.directory / "produced.csv").exists())
        self.assertFalse((self.directory / "consumer.csv").exists())

    def test_original_recursive_graph_checks_diagnostics_without_activation_or_data(self):
        os.chdir(self.directory)
        Path("environment.yaml").write_text("schema_version: '1.0'\nlanguage: python\n")
        def document(input, base, output, invalid=False):
            checks = '\n    verifications: [{matches: {pattern: "\\u00e9("}}]' if invalid else ""
            return (
                f"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\nbase: {base}\ninput: {input}\n"
                f"columns:\n  - {{name: ID, type: int, label: Identifier, derivation: {base}.ID}}\n"
                f"  - name: VALUE\n    type: str\n    label: Value\n    derivation: {base}.VALUE{checks}\n"
                f"output: {{path: {output}, columns: [ID, VALUE]}}\n"
            )
        Path("parents").mkdir()
        Path("parents/base.yaml").write_text(document("{RAW: ../never-read.csv}", "RAW", "../leaf.csv", True))
        Path("leaf.yaml").write_text("schema_version: '1.0'\nparents: [parents/base.yaml]\ndomain: LEAF\n")
        Path("left.yaml").write_text(document("{P: {path: leaf.csv, schema: './leaf.yaml'}}", "P", "left.csv"))
        Path("right.yaml").write_text(document("{P: {path: leaf.csv, schema: leaf.yaml}}", "P", "right.csv"))
        Path("spec.yaml").write_text(document("{L: {path: left.csv, schema: left.yaml}, R: {path: right.csv, schema: right.yaml}, Q: {path: right.csv, schema: './right.yaml'}}", "L", "consumer.csv", True))
        with (
            patch.object(host, "verify_versions", side_effect=AssertionError("unexpected activation")),
            patch.object(host, "resolve_callable", side_effect=AssertionError("unexpected binding")),
        ):
            for _ in range(2):
                rows = yamaa.check("spec.yaml", environment="environment.yaml").issues.to_dicts()
                self.assertEqual(len(rows), 2)
                for row, source in zip(rows, ("spec.yaml", "leaf.yaml"), strict=True):
                    context = {"entry": Path("spec.yaml").resolve().as_posix(), "pattern": "\u00e9(", "source": Path(source).resolve().as_posix()}
                    if source == "leaf.yaml":
                        context["declaring_sources"] = [Path("parents/base.yaml").resolve().as_posix()]
                    self.assertEqual(row, {
                        "phase": "validation", "condition": "invalid_regex", "requirement": "REQ-0827",
                        "spec_paths": ["columns.VALUE.verifications[0].matches.pattern"],
                        "context": json.dumps(context, sort_keys=True, ensure_ascii=False, separators=(",", ":")),
                    })
            # The original authored mismatch is admitted independently of any
            # absent serialized artifact. Checking never opens the CSV paths.
            Path("spec.yaml").write_text(document("{P: {path: wrong.csv, schema: leaf.yaml}}", "P", "consumer.csv"))
            rows = yamaa.check("spec.yaml", environment="environment.yaml").issues.to_dicts()
            self.assertEqual([(r["condition"], r["requirement"], r["spec_paths"]) for r in rows],
                [("producer_output_path_mismatch", "REQ-0534", ["input.P.path", "input.P.schema"])])
            Path("spec.yaml").write_text(document("{P: {path: leaf.csv, schema: leaf.yaml, types: {ID: int}}}", "P", "consumer.csv"))
            rows = yamaa.check("spec.yaml", environment="environment.yaml").issues.to_dicts()
            self.assertEqual([(r["condition"], r["requirement"], r["spec_paths"]) for r in rows],
                [("redundant_field_type", "REQ-0523", ["input.P.types.ID"])])
        for name in ("never-read.csv", "leaf.csv", "left.csv", "right.csv", "consumer.csv", "wrong.csv"):
            self.assertFalse(Path(name).exists())

    def test_declaring_file_paths_ignore_configuration_and_save_held_bytes(self):
        os.chdir(self.directory)
        for name in ("entry", "shared", "outputs"):
            Path(name).mkdir()
        Path("yamaa-project.yaml").write_text("[invalid configuration")
        Path("shared/environment.yaml").write_text(
            "schema_version: '1.0'\nlanguage: python\n"
        )
        Path("entry/spec.yaml").write_text(
            "schema_version: '1.0'\nparents: ../shared/base.yaml\ndomain: CHILD\n"
        )
        Path("shared/base.yaml").write_text(
            "schema_version: '1.0'\ndomain: BASE\nkeys: [ID]\ninput: {SRC: data.csv}\n"
            "columns: [{name: ID, type: int, derivation: SRC.ID}]\n"
            "output: {path: ../outputs/held.csv, columns: [ID]}\n"
        )
        Path("entry/data.csv").write_text("ID\n999\n")
        Path("shared/data.csv").write_text("ID\n7\n")
        with (
            patch.object(
                host, "verify_versions", side_effect=AssertionError("unused lock")
            ),
            patch.object(
                host, "resolve_callable", side_effect=AssertionError("unused binding")
            ),
        ):
            checked = yamaa.check(
                "entry/spec.yaml", environment="shared/environment.yaml"
            )
            result = yamaa.domain(
                "entry/spec.yaml", environment="shared/environment.yaml"
            )
        self.assertTrue(checked.issues.is_empty(), checked.issues)
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(result.output["ID"].to_list(), [7])
        self.assertFalse(Path("outputs/held.csv").exists())
        before = self.report(result)
        self.assertNotIn("yamaa-project.yaml", json.dumps(before))
        # Save uses the already serialized bytes and retained output location.
        # No specification, environment or input reread can supply this truth.
        for name in ("entry/spec.yaml", "shared/base.yaml", "shared/environment.yaml"):
            Path(name).unlink()
        Path("shared/data.csv").write_text("ID\n8\n")
        self.assertTrue(result.save())
        self.assertEqual(Path("outputs/held.csv").read_bytes(), b"ID\n7\n")
        self.assertEqual(self.report(result)["source_reads"], before["source_reads"])

    def test_absolute_local_paths_and_missing_declaring_file_input_have_no_fallback(
        self,
    ):
        os.chdir(self.directory)
        for name in ("entry", "shared", "outputs"):
            Path(name).mkdir()
        Path("environment.yaml").write_text("schema_version: '1.0'\nlanguage: python\n")
        Path("shared/data.csv").write_text("ID\n7\n")
        source = (self.directory.resolve() / "shared/../shared/./data.csv").as_posix()
        output = (self.directory.resolve() / "outputs/absolute.csv").as_posix()
        document = (
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n"
            "columns: [{name: ID, type: int, derivation: SRC.ID}]\n"
            "input: {SRC: %s}\noutput: {path: %s, columns: [ID]}\n"
        )
        Path("entry/spec.yaml").write_text(
            document % (json.dumps(source), json.dumps(output))
        )
        result = yamaa.domain("entry/spec.yaml", environment="environment.yaml")
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(result.output["ID"].to_list(), [7])
        self.assertFalse(Path(output).exists())
        self.assertTrue(result.save())
        self.assertEqual(Path(output).read_bytes(), b"ID\n7\n")
        Path(output).unlink()
        # A configured data root and a parent file must never rescue the absent
        # path beside the declaring specification.
        Path("yamaa-project.yaml").write_text("version: '1.0'\ndata_roots: [shared]\n")
        Path("data.csv").write_text("ID\n999\n")
        Path("shared/directory.csv").mkdir()
        for input_path, condition in (
            ("data.csv", "resource_path_missing"),
            ("../shared/directory.csv", "resource_path_not_regular_file"),
        ):
            Path("entry/spec.yaml").write_text(
                document % (json.dumps(input_path), json.dumps(output))
            )
            failed = yamaa.domain("entry/spec.yaml", environment="environment.yaml")
            self.assertIsNone(failed.output)
            self.assertEqual(failed.issues["condition"].to_list(), [condition])
            self.assertEqual(
                failed.issues["spec_paths"].to_list(), [["input.SRC.path"]]
            )
            with self.assertRaises(yamaa.DomainError):
                failed.save()
            self.assertFalse(Path(output).exists())

    def test_external_codelists_keep_environment_locations_and_missing_files_fail_there(self):
        os.chdir(self.directory)
        for name in ("entry", "environment", "terminology", "outputs"):
            Path(name).mkdir()
        Path("yamaa-project.yaml").write_text("[ignored configuration")
        Path("entry/spec.yaml").write_text(
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n"
            "input: {SRC: {path: data.csv, types: {ID: str}}}\n"
            "columns: [{name: ID, type: str, derivation: SRC.ID, "
            "submission: {codelist: IDENTIFIERS, origin: {type: Assigned}}}]\n"
            "output: {path: ../outputs/ids.csv, columns: [ID]}\n"
        )
        Path("entry/data.csv").write_text("ID\n01\n")
        terminology = "codelists: [{id: IDENTIFIERS, name: Identifiers, items: [{value: '01'}]}]\n"
        Path("terminology/ids.yaml").write_text(terminology)
        # An unrelated same-name document cannot change which codelist is used.
        Path("entry/ids.yaml").write_text("[unrelated document")
        for written in ("../terminology/ids.yaml", (self.directory.resolve() / "terminology/ids.yaml").as_posix()):
            with self.subTest(written=written):
                Path("environment/env.yaml").write_text(
                    "schema_version: '1.0'\nlanguage: python\ncodelists: [" + json.dumps(written) + "]\n"
                )
                checked = yamaa.check("entry/spec.yaml", environment="environment/env.yaml")
                result = yamaa.domain("entry/spec.yaml", environment="environment/env.yaml")
                self.assertTrue(checked.issues.is_empty(), checked.issues)
                self.assertTrue(result.issues.is_empty(), result.issues)
                self.assertEqual(result.output["ID"].to_list(), ["01"])
                self.assertTrue(self.report(result)["verifications"])
                self.assertFalse(Path("outputs/ids.csv").exists())
                self.assertTrue(result.save())
                self.assertEqual(Path("outputs/ids.csv").read_bytes(), b"ID\n01\n")
                Path("outputs/ids.csv").unlink()
        Path("environment/env.yaml").write_text(
            "schema_version: '1.0'\nlanguage: python\ncodelists: [ids.yaml]\n"
        )
        # Even a valid codelist at the process/entry location must not rescue it.
        Path("ids.yaml").write_text(terminology)
        Path("entry/ids.yaml").write_text(terminology)
        with patch.object(host, "verify_versions", side_effect=AssertionError("unexpected activation")):
            failed = yamaa.domain("entry/spec.yaml", environment="environment/env.yaml")
        self.assertIsNone(failed.output)
        self.assertFalse(failed.issues.is_empty())
        self.assertIsNone(failed._native.observations())
        self.assertFalse(Path("outputs/ids.csv").exists())

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="yamaa-public-environment-")
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.previous = Path.cwd()
        self.addCleanup(os.chdir, self.previous)

    def study(self, name):
        case = self.directory / name
        shutil.copytree(ROOT / name, case)
        os.chdir(case)
        return case

    def value_study(self):
        case = self.study("negative-function-contract")
        source = case / "spec.yaml"
        source.write_text(
            source.read_text().replace("args: {}", "args: {x: SOURCE.VALUE}")
        )
        return case

    def report(self, result):
        return json.loads(result._native.observations())

    def precision_study(self, decimals="2", output="rounded.csv"):
        os.chdir(self.directory)
        (self.directory / "environment.yaml").write_text(
            "schema_version: '1.0'\nlanguage: python\n"
        )
        (self.directory / "input.csv").write_text(
            "ID,VALUE,INT\n1,0.125,7\n2,-0.125,-7\n3,2.675,0\n"
            "4,-0.004,9\n5,1.234,1\n6,2.345,2\n7,,3\n8,25,4\n"
        )
        precision = "" if decimals is None else ", decimals: " + decimals
        (self.directory / "spec.yaml").write_text(
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n"
            "input: {SRC: {path: input.csv, types: {ID: int, VALUE: float, INT: int}}}\n"
            f"output: {{path: {output}, columns: [ID, VALUE, DOUBLE, EMPTY, INT]{precision}}}\n"
            "columns:\n"
            "  - {name: ID, type: int, derivation: SRC.ID}\n"
            "  - {name: VALUE, type: float, derivation: SRC.VALUE}\n"
            "  - {name: DOUBLE, type: float, derivation: {compute: {expr: 'VALUE + VALUE'}}}\n"
            "  - {name: EMPTY, type: str, derivation: {literal: ''}}\n"
            "  - {name: INT, type: int, derivation: SRC.INT}\n"
            "verifications: [{assert: {when: 'ID = 5', require: 'DOUBLE = 2.468'}}]\n"
        )

    def test_csv_precision_changes_saved_bytes_only_and_preserves_derived_values(self):
        expected = (
            b'ID,VALUE,DOUBLE,EMPTY,INT\n1,0.13,0.25,"",7\n2,-0.13,-0.25,"",-7\n'
            b'3,2.67,5.35,"",0\n4,0.00,-0.01,"",9\n5,1.23,2.47,"",1\n'
            b'6,2.35,4.69,"",2\n7,,,"",3\n8,25.00,50.00,"",4\n'
        )
        self.precision_study()
        with (
            patch.object(host, "verify_versions", side_effect=AssertionError("unused lock")),
            patch.object(host, "resolve_callable", side_effect=AssertionError("unused code")),
        ):
            self.assertTrue(yamaa.check("spec.yaml", environment="environment.yaml").issues.is_empty())
            result = yamaa.domain("spec.yaml", environment="environment.yaml")
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(result.output["VALUE"].to_list(), [0.125, -0.125, 2.675, -0.004, 1.234, 2.345, None, 25.0])
        self.assertEqual(result.output["DOUBLE"].to_list(), [0.25, -0.25, 5.35, -0.008, 2.468, 4.69, None, 50.0])
        before = self.report(result)
        self.assertEqual(before["artifacts"], [])
        self.assertFalse((self.directory / "rounded.csv").exists())
        self.assertTrue(result.save())
        self.assertEqual((self.directory / "rounded.csv").read_bytes(), expected)
        self.assertEqual(self.report(result)["artifacts"][0]["content"].encode(), expected)
        (self.directory / "rounded.csv").unlink()
        self.precision_study(None)
        ordinary = yamaa.domain("spec.yaml", environment="environment.yaml")
        self.assertTrue(ordinary.issues.is_empty(), ordinary.issues)
        self.assertEqual(ordinary.output.to_dicts(), result.output.to_dicts())
        # Metadata and every calculation/check observation retain the same truth.
        self.assertEqual(self.report(ordinary), before)

    def test_csv_precision_has_no_machine_width_limit_and_output_budget_refuses_save(self):
        self.precision_study("5000")
        (self.directory / "input.csv").write_text("ID,VALUE,INT\n1,0.125,7\n")
        result = yamaa.domain("spec.yaml", environment="environment.yaml")
        self.assertTrue(result.issues.is_empty(), result.issues)
        expected = b'ID,VALUE,DOUBLE,EMPTY,INT\n1,0.125' + b"0" * 4997 + b",0.25" + b"0" * 4998 + b',"",7\n'
        self.assertTrue(result.save())
        self.assertEqual((self.directory / "rounded.csv").read_bytes(), expected)
        (self.directory / "rounded.csv").unlink()
        self.precision_study("999999999999999999999999999999")
        self.assertTrue(yamaa.check("spec.yaml", environment="environment.yaml").issues.is_empty())
        failed = yamaa.domain("spec.yaml", environment="environment.yaml")
        self.assertIsNone(failed.output)
        self.assertFalse(failed.issues.is_empty())
        self.assertEqual(failed.issues["condition"].to_list(), ["engine_rejected"])
        with self.assertRaises(yamaa.DomainError):
            failed.save()
        self.assertFalse((self.directory / "rounded.csv").exists())

    def test_csv_precision_validation_and_prior_failure_preserve_public_save_gates(self):
        for precision, output, condition, requirement, context in (
            ("-9223372036854775809", "rounded.csv", "invalid_field_type", "REQ-0744", {"expected": "a non-negative integer", "actual": -9223372036854775809}),
            ("2", "rounded.parquet", "decimals_not_applicable", "REQ-0762", {"path": "rounded.parquet", "profile": "parquet"}),
        ):
            self.precision_study(precision, output)
            result = yamaa.domain("spec.yaml", environment="environment.yaml")
            self.assertIsNone(result.output)
            self.assertEqual(result.issues["condition"].to_list(), [condition])
            self.assertEqual(result.issues["requirement"].to_list(), [requirement])
            self.assertEqual(self.report(result)["diagnostics"][0]["context"], context)
            with self.assertRaises(yamaa.DomainError):
                result.save()
            self.assertFalse((self.directory / output).exists())
        for invalid in ("true", "2.0", "'2'"):
            self.precision_study(invalid)
            result = yamaa.check("spec.yaml", environment="environment.yaml")
            self.assertEqual(result.issues["spec_paths"].to_list(), [["output.decimals"]])
            self.assertFalse(result.issues.is_empty())
        self.precision_study("-1")
        path = self.directory / "spec.yaml"
        path.write_text(path.read_text().replace("VALUE + VALUE", "1 / 0"))
        failed = yamaa.domain("spec.yaml", environment="environment.yaml")
        self.assertIsNone(failed.output)
        self.assertEqual(failed.issues["condition"].to_list(), ["division_by_zero"])
        with self.assertRaises(yamaa.DomainError):
            failed.save()
        self.assertFalse((self.directory / "rounded.csv").exists())

    def test_original_positive_artifacts_and_fresh_activation_on_every_build(self):
        for name in CASES:
            with self.subTest(name=name):
                case = self.study(name)
                expected = next((case / "expected").glob("*.csv"))
                self.assertTrue(
                    yamaa.check(
                        "spec.yaml", environment="python/environment.yaml"
                    ).issues.is_empty()
                )
                for _ in range(2):
                    result = yamaa.domain(
                        "spec.yaml", environment="python/environment.yaml"
                    )
                    self.assertTrue(result.issues.is_empty(), result.issues)
                    self.assertIsNotNone(result.output)
                    before = self.report(result)
                    self.assertEqual(before["outcome"], "success")
                    self.assertEqual(before["activation"]["lock"], "verified")
                    self.assertTrue(before["activation"]["tests"])
                    self.assertTrue(
                        all(
                            case["outcome"] == "passed"
                            for case in before["activation"]["tests"]
                        )
                    )
                    self.assertEqual(before["artifacts"], [])
                    self.assertFalse((case / expected.name).exists())
                    self.assertTrue(result.save())
                    self.assertEqual(
                        (case / expected.name).read_bytes(), expected.read_bytes()
                    )
                    after = self.report(result)
                    self.assertEqual(
                        after["artifacts"][0]["content"].encode(), expected.read_bytes()
                    )
                    (case / expected.name).unlink()

    def test_negative_static_issue_is_exact_and_never_imports_code_or_reads_data(self):
        case = self.study("negative-function-contract")
        shutil.rmtree(case / "input")
        expected = yaml.safe_load((case / "expected/error.yaml").read_text())
        with (
            patch.object(
                host, "verify_versions", side_effect=AssertionError("activation")
            ),
            patch.object(
                host, "resolve_callable", side_effect=AssertionError("project code")
            ),
        ):
            for result in (
                yamaa.check("spec.yaml", environment="python/environment.yaml"),
                yamaa.domain("spec.yaml", environment="python/environment.yaml"),
            ):
                rows = result.issues.to_dicts()
                self.assertEqual(len(rows), 1)
                rows[0]["context"] = json.loads(rows[0]["context"])
                self.assertEqual(rows[0], expected)
        self.assertIsNone(result.output)
        with self.assertRaises(yamaa.DomainError):
            result.save()

    def test_explicit_absolute_metadata_keeps_the_callers_root_spelling(self):
        case = self.value_study()
        os.chdir(self.previous)
        checked = yamaa.check(
            case / "spec.yaml", environment=case / "python/environment.yaml"
        )
        self.assertTrue(checked.issues.is_empty(), checked.issues)

    def test_valid_static_check_has_no_host_effects_even_with_missing_data(self):
        case = self.value_study()
        shutil.rmtree(case / "input")
        with (
            patch.object(
                host, "verify_versions", side_effect=AssertionError("lock execution")
            ),
            patch.object(host, "resolve_callable", side_effect=AssertionError("code")),
        ):
            self.assertTrue(
                yamaa.check(
                    "spec.yaml", environment="python/environment.yaml"
                ).issues.is_empty()
            )

    def test_lock_mismatch_precedes_binding_tests_and_missing_study_data(self):
        case = self.value_study()
        lock = case / "python/uv.lock"
        lock.write_text(
            lock.read_text().replace(
                'name = "yamaa-benchmarks"\nversion = "1.0.0"',
                'name = "yamaa-benchmarks"\nversion = "999999.0"',
            )
        )
        shutil.rmtree(case / "input")
        with patch.object(
            host, "resolve_callable", side_effect=AssertionError("binding")
        ):
            result = yamaa.domain("spec.yaml", environment="python/environment.yaml")
        report = self.report(result)
        self.assertEqual(report["activation"]["lock"], "rejected")
        self.assertEqual(report["source_reads"], [])
        self.assertEqual(report["activation"]["bindings"], [])
        self.assertEqual(
            result.issues["condition"].to_list(), ["runtime_artifact_mismatch"]
        )

    def test_every_failed_case_is_retained_before_data_and_build_repeats_tests(self):
        case = self.value_study()
        shutil.rmtree(case / "input")
        env = case / "python/environment.yaml"
        source = yaml.safe_load(env.read_text())
        tests = source["functions"]["project_value"]["tests"]
        tests[0]["result"] = 9.5
        tests[1]["result"] = 9.0
        env.write_text(yaml.safe_dump(source, sort_keys=False))
        for _ in range(2):
            result = yamaa.domain("spec.yaml", environment="python/environment.yaml")
            report = self.report(result)
            self.assertEqual(report["source_reads"], [])
            self.assertEqual(
                [case["outcome"] for case in report["activation"]["tests"]],
                ["result_mismatch", "result_mismatch", "passed"],
            )
            self.assertEqual(
                result.issues["condition"].to_list(),
                ["function_conformance_failed"] * 2,
            )

    def test_environment_codelists_bind_and_verify_with_independent_static_findings(
        self,
    ):
        case = self.value_study()
        env = case / "python/environment.yaml"
        source = yaml.safe_load(env.read_text())
        source["codelists"] = [
            {
                "codelists": [
                    {
                        "id": "IDENTIFIERS",
                        "name": "Identifiers",
                        "items": [{"value": "01"}, {"value": "S2"}, {"value": "S3"}],
                    }
                ]
            }
        ]
        env.write_text(yaml.safe_dump(source, sort_keys=False))
        spec_path = case / "spec.yaml"
        spec = yaml.safe_load(spec_path.read_text())
        spec["columns"][0]["submission"] = {"codelist": "IDENTIFIERS", "origin": {"type": "Assigned"}}
        spec_path.write_text(yaml.safe_dump(spec, sort_keys=False))
        checked = yamaa.check("spec.yaml", environment=env.resolve())
        self.assertTrue(checked.issues.is_empty(), checked.issues)
        result = yamaa.domain("spec.yaml", environment=env.resolve())
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertTrue(self.report(result)["verifications"])
        spec["columns"][0]["submission"]["codelist"] = "ABSENT"
        spec["columns"][1]["derivation"]["function"]["args"] = {}
        spec_path.write_text(yaml.safe_dump(spec, sort_keys=False))
        shutil.rmtree(case / "input")
        with patch.object(
            host, "verify_versions", side_effect=AssertionError("activation")
        ):
            rows = yamaa.check("spec.yaml", environment=env.resolve()).issues.to_dicts()
        self.assertEqual({row["requirement"] for row in rows}, {"REQ-0700", "REQ-0953"})

    def test_approved_math_is_identical_in_root_row_and_nested_case_compute(self):
        expression = {"compute": {"expr": "EXP(0) + LN(1) + POWER(2, 3)"}}
        for context in ("column", "row", "case"):
            with self.subTest(context=context):
                case = (
                    self.value_study()
                    if context == "column"
                    else self.study(
                        "schema-functions" if context == "row" else "schema-non-finite"
                    )
                )
                path = case / "spec.yaml"
                spec = yaml.safe_load(
                    (ROOT / "negative-function-contract/spec.yaml").read_text()
                )
                spec["columns"][1]["derivation"] = copy.deepcopy(expression)
                if context == "row":
                    del spec["columns"][1]["derivation"]
                    spec["rows"] = [
                        {
                            "id": "records",
                            "dataset": "SOURCE",
                            "derivations": {
                                "RESULT": {"value": copy.deepcopy(expression)}
                            },
                        }
                    ]
                if context == "case":
                    spec["columns"][1]["derivation"] = {
                        "case": [
                            {
                                "when": "ID = '01'",
                                "then": {
                                    "case": [
                                        {
                                            "when": "ID = '01'",
                                            "then": copy.deepcopy(expression),
                                        },
                                        {"otherwise": {"literal": 99.0}},
                                    ]
                                },
                            },
                            {"otherwise": {"literal": 98.0}},
                        ]
                    }
                path.write_text(yaml.safe_dump(spec, sort_keys=False))
                if context != "column":
                    shutil.rmtree(case / "input")
                    shutil.copytree(
                        ROOT / "negative-function-contract/input", case / "input"
                    )
                    shutil.copyfile(
                        ROOT / "negative-function-contract/python/environment.yaml",
                        case / "python/environment.yaml",
                    )
                result = yamaa.domain(
                    "spec.yaml", environment="python/environment.yaml"
                )
                self.assertTrue(result.issues.is_empty(), result.issues)
                self.assertEqual(result.output["RESULT"].to_list(), [9.0])
                result.save()
                self.assertEqual((case / "test.csv").read_bytes(), b"ID,RESULT\n01,9\n")

    def test_original_interrupt_survives_activation(self):
        case = self.value_study()
        path = case / "spec.yaml"
        spec = yaml.safe_load(path.read_text())
        spec["output"]["decimals"] = 2
        path.write_text(yaml.safe_dump(spec, sort_keys=False))
        original = KeyboardInterrupt("original project interruption")
        with (
            patch.object(host, "verify_versions", side_effect=original),
            self.assertRaises(KeyboardInterrupt) as seen,
        ):
            yamaa.domain("spec.yaml", environment="python/environment.yaml")
        self.assertIs(seen.exception, original)

    def test_row_math_binds_stored_fields_and_orders_completed_dependencies(self):
        case = self.value_study()
        path = case / "spec.yaml"
        spec = yaml.safe_load(path.read_text())
        spec["columns"].insert(
            1, {"name": "EARLIER", "type": "float", "label": "Earlier"}
        )
        del spec["columns"][2]["derivation"]
        spec["rows"] = [
            {
                "id": "records",
                "dataset": "SOURCE",
                "derivations": {
                    "RESULT": {
                        "compute": {
                            "expr": "LN(1) + EXP(0) - 1 + EARLIER + SOURCE.VALUE"
                        }
                    },
                    "EARLIER": {"literal": 2.5},
                },
            }
        ]
        path.write_text(yaml.safe_dump(spec, sort_keys=False))
        result = yamaa.domain("spec.yaml", environment="python/environment.yaml")
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(result.output["RESULT"].to_list(), [4.0])
        self.assertTrue(result.save())
        self.assertEqual((case / "test.csv").read_bytes(), b"ID,RESULT\n01,4\n")

    def test_opaque_host_error_is_retained_independently_of_issue_format(self):
        case = self.value_study()
        path = case / "spec.yaml"
        spec = yaml.safe_load(path.read_text())
        spec["output"]["decimals"] = 2
        path.write_text(yaml.safe_dump(spec, sort_keys=False))
        shutil.rmtree(case / "input")
        original = RuntimeError("original lock host error")
        with patch.object(host, "verify_versions", side_effect=original):
            result = yamaa.domain("spec.yaml", environment="python/environment.yaml")
        self.assertIsNone(result.output)
        self.assertIsNone(result._native.observations())
        facts = result._native.project_failures()
        self.assertTrue(any(fact is original for _, fact in facts))

    def test_lock_format_and_host_language_are_static_independent_findings(self):
        case = self.value_study()
        shutil.copyfile(case / "r/renv.lock", case / "python/uv.lock")
        rows = yamaa.check(
            "spec.yaml", environment="python/environment.yaml"
        ).issues.to_dicts()
        self.assertTrue(
            any(row["condition"] == "project_environment_invalid" for row in rows), rows
        )
        env = case / "python/environment.yaml"
        env.write_text(env.read_text().replace("language: python", "language: r"))
        rows = yamaa.check(
            "spec.yaml", environment="python/environment.yaml"
        ).issues.to_dicts()
        self.assertTrue(
            any(row["condition"] == "runner_language_mismatch" for row in rows), rows
        )

    def test_called_only_selection_and_no_code_effects_for_empty_selection(self):
        case = self.value_study()
        env = case / "python/environment.yaml"
        source = yaml.safe_load(env.read_text())
        unused = copy.deepcopy(source["functions"]["project_value"])
        unused["function"] = "absent_package.never_called"
        source["functions"]["unused"] = unused
        env.write_text(yaml.safe_dump(source, sort_keys=False))
        result = yamaa.domain("spec.yaml", environment="python/environment.yaml")
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(
            [b["function"] for b in self.report(result)["activation"]["bindings"]],
            ["project_value"],
        )
        spec = case / "spec.yaml"
        spec.write_text(
            spec.read_text().replace(
                "function:\n        name: project_value\n        args: {x: SOURCE.VALUE}",
                "literal: 1.0",
            )
        )
        with (
            patch.object(host, "verify_versions", side_effect=AssertionError("lock")),
            patch.object(host, "resolve_callable", side_effect=AssertionError("code")),
        ):
            result = yamaa.domain("spec.yaml", environment="python/environment.yaml")
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(self.report(result)["activation"]["tests"], [])

    def test_aggregate_callback_keeps_original_current_types_and_selected_record(self):
        case = self.study("negative-function-contract")
        spec = """schema_version: '1.0'
domain: TEST
input:
  SRC: {path: input.csv, types: {ID: int, N: float}}
  B: {path: input.csv, types: {ID: int, N: str}}
base: SRC
keys: [ID, N]
intermediates:
  - {id: CAP, dataset: SRC, key: {N: SRC.N}, order_by: [SRC.ID], keep: first}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: N, type: float, derivation: SRC.N}
  - name: VALUE
    type: float
    derivation:
      aggregate:
        key: {ID: ID}
        derive:
          - {name: BASE, type: float, derivation: B.N}
          - {name: V, type: float, derivation: {function: {name: project_value, args: {x: CAP.N}}}}
        expr: SUM(V)
output: {path: output.csv, columns: [ID, N, VALUE]}
"""
        (case / "spec.yaml").write_text(spec)
        (case / "input.csv").write_text("ID,N\n1,10\n1,30\n2,40\n")
        result = yamaa.domain("spec.yaml", environment="python/environment.yaml")
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(result.output.rows(), [(1, 10.0, 20.0), (1, 30.0, 60.0), (2, 40.0, 40.0)])
        observed = self.report(result)
        self.assertEqual([row["function"] for row in observed["activation"]["bindings"]], ["project_value"])
        self.assertTrue(result.save())
        self.assertEqual((case / "output.csv").read_bytes(), b"ID,N,VALUE\n1,10,20\n1,30,60\n2,40,40\n")
        self.assertTrue(result.save())
        self.assertEqual(self.report(result)["activation"], observed["activation"])
        (case / "spec.yaml").write_text(spec.replace(", order_by: [SRC.ID], keep: first", ""))
        with (
            patch.object(host, "verify_versions", side_effect=AssertionError("lock")),
            patch.object(host, "resolve_callable", side_effect=AssertionError("code")),
        ):
            issues = yamaa.check("spec.yaml", environment="python/environment.yaml").issues.to_dicts()
        self.assertTrue(any(row["condition"] == "invalid_derive_step" and row["requirement"] == "REQ-1242" for row in issues), issues)


    def test_row_callbacks_join_each_driver_by_completed_keys(self):
        case = self.study("negative-function-contract")
        (case / "a.csv").write_text("ID,N\n1,10\n2,20\n3,30\n")
        (case / "b.csv").write_text("ID,N\n2,40\n1,50\n")
        for driver, other, rows, csv in (
            ("SRC", "B", [(1, 50.0), (2, 40.0), (3, None)], b"ID,VALUE\n1,50\n2,40\n3,\n"),
            ("B", "SRC", [(2, 20.0), (1, 10.0)], b"ID,VALUE\n2,20\n1,10\n"),
        ):
            spec = f"""schema_version: '1.0'
domain: TEST
input:
  SRC: {{path: a.csv, types: {{ID: int, N: float}}}}
  B: {{path: b.csv, types: {{ID: int, N: float}}}}
base: SRC
keys: [ID]
columns:
  - {{name: ID, type: int}}
  - {{name: VALUE, type: float}}
rows:
  - id: records
    dataset: {driver}
    derivations:
      ID: {driver}.ID
      VALUE: {{function: {{name: project_value, args: {{x: {other}.N}}}}}}
output: {{path: output.csv, columns: [ID, VALUE]}}
"""
            (case / "spec.yaml").write_text(spec)
            result = yamaa.domain("spec.yaml", environment="python/environment.yaml")
            self.assertTrue(result.issues.is_empty(), result.issues)
            self.assertEqual(result.output.rows(), rows)
            activation = self.report(result)["activation"]
            self.assertEqual([row["function"] for row in activation["bindings"]], ["project_value"])
            self.assertTrue(result.save())
            self.assertEqual((case / "output.csv").read_bytes(), csv)
            self.assertTrue(result.save())
            self.assertEqual(self.report(result)["activation"], activation)


    def test_intermediate_callbacks_read_prior_values_and_selected_donor_types(self):
        case = self.study("negative-function-contract")
        (case / "a.csv").write_text("ID,N\n1,10\n1,30\n2,40\n")
        (case / "b.csv").write_text("ID,N\n1,one\n2,two\n")
        (case / "spec.yaml").write_text("""schema_version: '1.0'
domain: TEST
input:
  SRC: {path: a.csv, types: {ID: int, N: float}}
  B: {path: b.csv, types: {ID: int, N: str}}
base: SRC
keys: [ID, N]
intermediates:
  - {id: CAP, dataset: SRC, key: [ID], order_by: [SRC.N], keep: first}
  - id: DON
    dataset: B
    key: [ID]
    order_by: [B.ID]
    keep: first
    derivations:
      BASE: CAP.N
      VALUE: {function: {name: project_value, args: {x: BASE}}}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: N, type: float, derivation: SRC.N}
  - {name: VALUE, type: float, derivation: DON.VALUE}
output: {path: output.csv, columns: [ID, N, VALUE]}
""")
        result = yamaa.domain("spec.yaml", environment="python/environment.yaml")
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(result.output.rows(), [(1, 10.0, 10.0), (1, 30.0, 10.0), (2, 40.0, 40.0)])
        activation = self.report(result)["activation"]
        self.assertEqual([row["function"] for row in activation["bindings"]], ["project_value"])
        self.assertTrue(result.save())
        self.assertEqual((case / "output.csv").read_bytes(), b"ID,N,VALUE\n1,10,10\n1,30,10\n2,40,40\n")
        self.assertTrue(result.save())
        self.assertEqual(self.report(result)["activation"], activation)


if __name__ == "__main__":
    unittest.main()
