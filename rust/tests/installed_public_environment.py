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
    def test_graph_metadata_does_not_enable_public_producer_execution(self):
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
        for result in (checked, failed):
            self.assertEqual(result.issues["condition"].to_list(), ["unsupported_operation"])
            self.assertEqual(result.issues["spec_paths"].to_list(), [["input.P.schema"]])
        self.assertIsNone(failed.output)
        with self.assertRaises(yamaa.DomainError):
            failed.save()
        self.assertFalse((self.directory / "produced.csv").exists())
        self.assertFalse((self.directory / "consumer.csv").exists())

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
        spec["columns"][0]["submission"] = {"codelist": "IDENTIFIERS"}
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


if __name__ == "__main__":
    unittest.main()
