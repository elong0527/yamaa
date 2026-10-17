"""Installed public API contracts with independent authored inputs and truth."""

from __future__ import annotations

import dataclasses
import gc
import inspect
import json
import shutil
import tempfile
import unittest
import weakref
from pathlib import Path
from unittest.mock import patch

import polars as pl
from host_fixtures import load as load_truth

import yamaa
from yamaa import _locked_functions as host

TRUTH = load_truth(__file__, "public-contract")

SPEC = TRUTH["specification_yaml"]
INPUT = TRUTH["python_input_csv"].encode("utf-8")
VALUES = [
    None if value is None else int(value) for value in TRUTH["python_values_decimal"]
]


class PublicContract(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="yamaa-public-contract-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.spec = self.root / "spec.yaml"
        self.spec.write_text(SPEC, encoding="utf-8")
        (self.root / "input.csv").write_bytes(INPUT)

    def issues(self, result):
        frame = result.issues
        self.assertEqual(
            frame.columns,
            TRUTH["issue_columns"],
        )
        self.assertEqual(
            frame.dtypes,
            [pl.String, pl.String, pl.String, pl.List(pl.String), pl.String],
        )
        records = result.diagnostics
        self.assertIsInstance(records, list)
        self.assertEqual(len(records), frame.height)
        for row, record in zip(frame.to_dicts(), records, strict=True):
            self.assertIsInstance(json.loads(row["context"]), dict)
            self.assertTrue(row["condition"])
            self.assert_projection(row, record)
        return frame

    def assert_projection(self, row, record):
        """One frame row and one named record describe the same finding."""
        self.assertIsInstance(record, yamaa.Diagnostic)
        self.assertEqual(
            [field.name for field in dataclasses.fields(record)],
            TRUTH["diagnostic_record_names"],
        )
        self.assertEqual(
            (record.phase, record.condition, record.requirement, record.spec_paths),
            (row["phase"], row["condition"], row["requirement"], row["spec_paths"]),
        )
        self.assertIsInstance(record.context, dict)
        # The frame context is the canonical JSON text of the record context;
        # Python's JSON reader keeps integer tokens exact, so the two agree
        # value for value.
        self.assertEqual(record.context, json.loads(row["context"]))
        if record.source_span is not None:
            self.assertIsInstance(record.source_span, yamaa.SourceSpan)
            self.assertLessEqual(0, record.source_span.start)
            self.assertLessEqual(record.source_span.start, record.source_span.end)
        if record.operand_route is not None:
            self.assertIsInstance(record.operand_route, list)
            self.assertTrue(all(isinstance(step, str) for step in record.operand_route))
        with self.assertRaises(dataclasses.FrozenInstanceError):
            record.phase = "changed"

    def test_component_endpoints_are_absent_from_the_installed_binding(self):
        from yamaa import _native

        for name in (
            "engine_info",
            "analyze_aggregate",
            "analyze_numeric",
            "analyze_predicate",
            "analyze_dependencies",
            "analyze_column_dependencies",
            "analyze_references",
            "reference_capabilities",
            "dataset_capabilities",
            "scalar_round_trip",
            "evaluate_numeric",
            "evaluate_regex",
            "table_round_trip",
            "table_snapshot",
            "execute_dataset",
            "execute_dataset_sources",
            "execute_dataset_functions",
            "invoke_function",
            "interpret_schema",
            "decode_yaml",
            "_compile_schema",
            "_Schema",
            "_compile_reference_catalog",
            "_ReferenceCatalog",
            "_profile_dataset_functions",
            "_temporal_result",
            "_capture_schema_metadata",
            "_schema_include_names",
            "_SchemaMetadata",
            "_ProducerReport",
            "_prepare_document",
            "_prepare_specification",
            "_prepare_inherited_specification",
            "_prepare_file_specification",
            "_prepare_file_project",
            "_prepare_producer_file",
            "_Specification",
            "_FileSpecification",
            "_ProjectSpecification",
            "_ProjectAttempt",
            "_ProjectResult",
            "_ProducerSpecification",
            "_BuildResult",
        ):
            with self.subTest(name=name):
                self.assertFalse(hasattr(_native, name))
        self.assertIsInstance(_native._CORE_VERSION, str)
        self.assertTrue(_native._CORE_VERSION)
        self.assertEqual(yamaa.__version__, "0.4.0")
        self.assertEqual(_native._CORE_VERSION, yamaa.__version__)

    def test_signatures_argument_types_and_removed_options(self):
        for function in (yamaa.domain, yamaa.check):
            parameters = inspect.signature(function).parameters
            self.assertEqual(
                list(parameters), ["specification", "environment", "resolver"]
            )
            self.assertEqual(
                parameters["environment"].kind, inspect.Parameter.KEYWORD_ONLY
            )
            self.assertIsNone(parameters["environment"].default)
            for value in (None, 7, True, [], object()):
                with self.subTest(
                    function=function.__name__, value=type(value).__name__
                ):
                    with self.assertRaises(TypeError):
                        function(value)
                    if value is not None:
                        with self.assertRaises(TypeError):
                            function(self.spec, environment=value)
            with self.assertRaises(TypeError):
                function(self.spec, None)
            for option in (
                "schema_root",
                "project_root",
                "data_roots",
                "read_project_configuration",
                "dispatcher",
                "study_document",
            ):
                with self.subTest(option=option), self.assertRaises(TypeError):
                    function(self.spec, **{option: None})

    def test_missing_specification_is_an_issue_and_failed_save_is_typed(self):
        absent = self.root / "absent.yaml"
        for function in (yamaa.check, yamaa.domain):
            result = function(absent)
            self.assertGreater(self.issues(result).height, 0)
            for name in ("spec", "specification", "resolved", "resolved_specification"):
                self.assertFalse(hasattr(result, name))
            self.assertEqual(
                [record.condition for record in result.diagnostics],
                result.issues["condition"].to_list(),
            )
            if function is yamaa.domain:
                self.assertIsNone(result.output)
                before = result.issues.to_dicts()
                records = result.diagnostics
                for _ in range(2):
                    with self.assertRaises(yamaa.DomainError):
                        result.save()
                    self.assertEqual(result.issues.to_dicts(), before)
                    self.assertEqual(result.diagnostics, records)
        self.assertFalse((self.root / "output.csv").exists())

    def test_read_only_views_repeated_builds_and_owned_publication(self):
        first, second = yamaa.domain(self.spec), yamaa.domain(self.spec)
        for result in (first, second):
            self.assertTrue(self.issues(result).is_empty())
            self.assertEqual(result.output["VALUE"].to_list(), VALUES)
            self.assertIsNone(result.verification_log)
            self.assertIsNone(result.warning_log)
            for name in tuple(TRUTH["owned_view_names"]):
                with self.subTest(name=name), self.assertRaises(AttributeError):
                    setattr(result, name, None)
            with self.assertRaises(TypeError):
                result.save(self.root / "other.csv")
        self.assertFalse((self.root / "output.csv").exists())
        view = first.output
        view[0, "VALUE"] = 9
        self.assertEqual(first.output["VALUE"].to_list(), VALUES)
        self.spec.unlink()
        (self.root / "input.csv").unlink()
        gc.collect()
        for _ in range(2):
            self.assertIs(first.save(), True)
            self.assertEqual((self.root / "output.csv").read_bytes(), INPUT)
        del first
        gc.collect()
        self.assertEqual(second.output["VALUE"].to_list(), VALUES)
        self.assertIs(second.save(), True)

    def test_exact_nul_text_and_empty_literal_are_distinct_from_missing(self):
        self.spec.write_text(
            SPEC.replace("VALUE, type: int", "VALUE, type: str")
            .replace(
                "output: {",
                TRUTH["empty_text_column"],
            )
            .replace("columns: [ID, VALUE]", "columns: [ID, VALUE, EMPTY]"),
            encoding="utf-8",
        )
        (self.root / "input.csv").write_bytes(b"ID,VALUE\na,a\x00b\nb,\xc3\xa9\nc,\n")
        result = yamaa.domain(self.spec)
        self.assertTrue(self.issues(result).is_empty())
        self.assertEqual(result.output["VALUE"].to_list(), ["a\x00b", "\u00e9", None])
        self.assertEqual(result.output["EMPTY"].to_list(), ["", "", ""])
        result.save()
        self.assertEqual(
            (self.root / "output.csv").read_bytes(),
            b'ID,VALUE,EMPTY\na,a\x00b,""\nb,\xc3\xa9,""\nc,,""\n',
        )

    def test_failed_verification_retains_owned_diagnostic_log(self):
        self.spec.write_text(
            SPEC.replace(
                TRUTH["integer_column_yaml"],
                "  - name: VALUE\n    type: int\n    derivation: SRC.VALUE\n"
                "    verifications:\n"
                "      - range: {max: 0, severity: warning, id: warn}\n"
                "      - range: {max: 0, severity: error, id: fatal}",
            )
            .replace(
                "output: {",
                TRUTH["unreached_row_count_yaml"],
            )
            .replace(
                "columns: [ID, VALUE]}",
                TRUTH["log_output_yaml"],
            ),
            encoding="utf-8",
        )
        (self.root / "input.csv").write_bytes(b"ID,VALUE\na,-3\nb,7\nc,\n")
        for name in ("output.csv", "warnings.csv"):
            (self.root / name).write_bytes(b"sentinel")
        result = yamaa.domain(self.spec)
        issues = self.issues(result).to_dicts()
        self.assertEqual(len(issues), 1)
        self.assertEqual(issues[0]["condition"], "range_failed")
        # The same failure through the named record.
        (record,) = result.diagnostics
        self.assertEqual(record.condition, "range_failed")
        self.assertEqual(record.phase, issues[0]["phase"])
        self.assertEqual(record.requirement, issues[0]["requirement"])
        self.assertEqual(record.spec_paths, issues[0]["spec_paths"])
        self.assertEqual(record.context, json.loads(issues[0]["context"]))
        self.assertIsNone(result.output)
        # REQ-0396 builds the warning artifact only for successful runs;
        # REQ-1177 retains the evaluated diagnostic prefix on failure.
        self.assertIsNone(result.warning_log)
        view = result.verification_log
        self.assertEqual(view["VERIFICATION_ID"].to_list(), ["warn", "fatal"])
        self.assertEqual(view["OUTCOME"].to_list(), ["violated", "violated"])
        self.assertEqual(view["EVALUATED_COUNT"].to_list(), [3, 3])
        self.assertEqual(view["FAILURE_COUNT"].to_list(), [1, 1])
        self.assertEqual(view["REQUIREMENT"].to_list(), ["REQ-0377", "REQ-0377"])
        truth = view.to_dicts()
        view[0, "REPORT_VERSION"] = "changed"
        self.spec.unlink()
        (self.root / "input.csv").unlink()
        gc.collect()
        for _ in range(2):
            with self.assertRaises(yamaa.DomainError):
                result.save()
            self.assertEqual(result.issues.to_dicts(), issues)
            self.assertEqual(result.diagnostics, [record])
            self.assertEqual(result.verification_log.to_dicts(), truth)
            self.assertEqual((self.root / "output.csv").read_bytes(), b"sentinel")
            self.assertEqual((self.root / "warnings.csv").read_bytes(), b"sentinel")
            lines = (self.root / "checks.csv").read_bytes().splitlines()
            self.assertEqual(len(lines), 3)
        held = result.verification_log
        del result
        gc.collect()
        self.assertEqual(held.to_dicts(), truth)

    def function_study(self):
        original = (
            Path(__file__).with_name("project-original") / "negative-function-contract"
        )
        case = self.root / "project"
        shutil.copytree(original, case)
        source = case / "spec.yaml"
        source.write_text(
            source.read_text(encoding="utf-8").replace(
                "args: {}", "args: {x: SOURCE.VALUE}"
            ),
            encoding="utf-8",
        )
        return source, case / "python/environment.yaml"

    def test_each_callback_stage_preserves_original_interrupt_identity(self):
        source, environment = self.function_study()
        verify, resolve = host.verify_versions, host.resolve_callable
        for kind in (KeyboardInterrupt, SystemExit):
            for stage in ("lock", "binding", "conformance", "derivation"):
                with self.subTest(kind=kind.__name__, stage=stage):
                    original = kind(TRUTH["interrupt_message"])
                    calls = []

                    def versions(*arguments):
                        if stage == "lock":
                            raise original
                        return verify(*arguments)

                    def binding(*arguments):
                        if stage == "binding":
                            raise original
                        invoke = resolve(*arguments)

                        def callback(**arguments):
                            calls.append(arguments)
                            if stage == "conformance" or (
                                stage == "derivation" and len(calls) > 2
                            ):
                                raise original
                            return invoke(**arguments)

                        return callback

                    with (
                        patch.object(host, "verify_versions", side_effect=versions),
                        patch.object(host, "resolve_callable", side_effect=binding),
                        self.assertRaises(kind) as seen,
                    ):
                        yamaa.domain(source, environment=environment)
                    self.assertIs(seen.exception, original)
                    self.assertEqual(
                        len(calls), {"conformance": 1, "derivation": 3}.get(stage, 0)
                    )
                    self.assertFalse((source.parent / "test.csv").exists())

    def test_callback_can_reenter_public_check_and_domain(self):
        source, environment = self.function_study()
        resolve = host.resolve_callable
        calls = []

        def resolve_reentrant(reference, parameters):
            invoke = resolve(reference, parameters)

            def callback(**arguments):
                checked = yamaa.check(self.spec)
                nested = yamaa.domain(self.spec)
                self.assertTrue(checked.issues.is_empty())
                self.assertTrue(nested.issues.is_empty())
                self.assertEqual(nested.output["VALUE"].to_list(), VALUES)
                calls.append(nested)
                return invoke(**arguments)

            return callback

        with patch.object(host, "resolve_callable", side_effect=resolve_reentrant):
            result = yamaa.domain(source, environment=environment)
        self.assertTrue(self.issues(result).is_empty(), result.issues)
        self.assertGreaterEqual(len(calls), 3)
        self.spec.unlink()
        gc.collect()
        self.assertEqual(calls[0].output["VALUE"].to_list(), VALUES)
        self.assertFalse((self.root / "output.csv").exists())

    def assert_callback_exception_lifetime(self, reentrant):
        source, environment = self.function_study()
        reentries = []

        class OriginalError(RuntimeError):
            def __str__(error):
                if reentrant:
                    nested = yamaa.domain(self.spec)
                    self.assertTrue(nested.issues.is_empty(), nested.issues)
                    self.assertEqual(nested.output["VALUE"].to_list(), VALUES)
                    reentries.append(nested.output.height)
                return "retained callback failure"

        held = [OriginalError()]
        failure_reference = weakref.ref(held[0])

        def resolve(*_):
            def callback(**_):
                raise held[0]

            return callback

        with patch.object(host, "resolve_callable", side_effect=resolve):
            result = yamaa.domain(source, environment=environment)
        self.assertIsNone(result.output)
        self.assertFalse(result.issues.is_empty())
        failures = result._native.project_failures()
        self.assertTrue(failures)
        self.assertTrue(all(error is failure_reference() for _, error in failures))
        del failures
        held.clear()
        gc.collect()
        self.assertIsNotNone(failure_reference())
        failures = result._native.project_failures()
        self.assertTrue(all(error is failure_reference() for _, error in failures))
        del failures
        with self.assertRaises(yamaa.DomainError):
            result.save()
        del result
        gc.collect()
        self.assertIsNone(failure_reference())
        self.assertFalse((source.parent / "test.csv").exists())
        self.assertEqual(bool(reentries), reentrant)

    def test_callback_exception_lives_exactly_as_long_as_its_public_result(self):
        self.assert_callback_exception_lifetime(False)

    def test_callback_exception_keeps_its_complete_unicode_and_nul_message(self):
        source, environment = self.function_study()
        message = "\u00e9\x00" * 30_000 + "complete failure"
        original = RuntimeError(message)
        resolve = host.resolve_callable
        calls = []

        def binding(*arguments):
            invoke = resolve(*arguments)

            def callback(**arguments):
                calls.append(arguments)
                if len(calls) > 2:
                    raise original
                return invoke(**arguments)

            return callback

        with patch.object(host, "resolve_callable", side_effect=binding):
            result = yamaa.domain(source, environment=environment)
        self.assertEqual(len(calls), 3)
        self.assertIsNone(result.output)
        self.assertEqual(result.issues["condition"].to_list(), ["function_call_failed"])
        context = json.loads(result.issues["context"][0])
        self.assertEqual(context["host_error"], "RuntimeError")
        self.assertEqual(context["host_message"], message)
        self.assertNotIn("host_details_truncated", context)
        # The structured record keeps the complete NUL-bearing text as str.
        (record,) = result.diagnostics
        self.assertEqual(record.condition, "function_call_failed")
        self.assertEqual(record.context["host_error"], "RuntimeError")
        self.assertEqual(record.context["host_message"], message)
        self.assertNotIn("host_details_truncated", record.context)
        self.assertIs(result._native.project_failures()[0][1], original)
        with self.assertRaises(yamaa.DomainError):
            result.save()

    def test_callback_string_larger_than_one_mebibyte_is_owned_and_saved_exactly(self):
        source, environment = self.function_study()
        environment.write_text(
            "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\n"
            "functions:\n  text:\n    function: yamaa_benchmarks.project_value\n"
            "    description: Return text unchanged.\n"
            "    params: [{name: x, type: str}]\n    returns: str\n    tests:\n"
            "      - {id: normal, covers: [normal], args: {x: short}, result: short}\n"
            "      - {id: empty, covers: [boundary], args: {x: ''}, result: ''}\n"
            "      - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n",
            encoding="utf-8",
        )
        source.write_text(
            "schema_version: '1.0'\ndomain: TEXT\nkeys: [ID]\n"
            "input: {SRC: {path: large.csv, types: {ID: int, V: str}}}\n"
            "columns:\n  - {name: ID, type: int, derivation: SRC.ID}\n"
            "  - {name: V, type: str, derivation: {function: {name: text, args: {x: SRC.V}}}}\n"
            "output: {path: held.csv, columns: [ID, V]}\n",
            encoding="utf-8",
        )
        value = "\u00e9" * 524_289 + "complete"
        expected = ("ID,V\n1," + value + "\n").encode("utf-8")
        (source.parent / "large.csv").write_bytes(expected)
        result = yamaa.domain(source, environment=environment)
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(result.output["V"].to_list(), [value])
        source.unlink()
        environment.unlink()
        (source.parent / "large.csv").unlink()
        for _ in range(2):
            self.assertTrue(result.save())
            self.assertEqual((source.parent / "held.csv").read_bytes(), expected)

    def test_reentrant_exception_rendering_keeps_owned_failures_collectable(self):
        self.assert_callback_exception_lifetime(True)

    def test_public_define_signature_and_missing_environment(self):
        parameters = inspect.signature(yamaa.define).parameters
        self.assertEqual(list(parameters), TRUTH["define_argument_names"])
        self.assertEqual(parameters["created"].kind, inspect.Parameter.KEYWORD_ONLY)
        self.assertIsNone(parameters["created"].default)
        result = yamaa.define(
            self.root / "absent-environment.yaml", "sdtm", created="2026-09-11T00:00:00"
        )
        self.assertGreater(self.issues(result).height, 0)
        with self.assertRaises(yamaa.DefineError) as caught:
            result.save()
        self.assertTrue(caught.exception.issues.equals(result.issues))
        self.assertEqual(caught.exception.diagnostics, result.diagnostics)
        with self.assertRaises(TypeError):
            yamaa.define(None, "sdtm")

    def test_records_keep_exact_wide_integers_and_recorded_geometry(self):
        # A literal beyond every 64-bit range is refused before study access;
        # its context carries the signed 64-bit bounds and the exact literal.
        self.spec.write_text(
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\n"
            "columns:\n  - {name: ID, type: str, derivation: SRC.ID}\n"
            "  - {name: N, type: int, derivation: {literal: 18446744073709551616}}\n"
            "output: {path: output.csv, columns: [ID, N]}\n",
            encoding="utf-8",
        )
        checked = yamaa.check(self.spec)
        frame = self.issues(checked)
        (record,) = checked.diagnostics
        self.assertEqual(record.phase, "validation")
        self.assertEqual(record.condition, "integer_overflow")
        self.assertEqual(record.requirement, "REQ-0014")
        self.assertEqual(record.spec_paths, ["columns.N.derivation.literal"])
        self.assertEqual(
            record.context,
            {"minimum": -(2**63), "maximum": 2**63 - 1, "actual": 2**64},
        )
        self.assertTrue(all(type(value) is int for value in record.context.values()))
        self.assertIn('"actual":18446744073709551616', frame["context"][0])
        # A definition finding records no geometry: None, not zero.
        self.assertIsNone(record.source_span)
        self.assertIsNone(record.operand_route)

        # A division by zero fails at one node of the compute expression: the
        # record spans that node's bytes and routes to it from the root, so a
        # failure at the root has a present empty route, not an absent one.
        (self.root / "input.csv").write_bytes(b"ID,VALUE,ZERO\na,2.5,0\n")
        for expression, route in (
            ("1 + VALUE / ZERO", ["right"]),
            ("VALUE / ZERO", []),
        ):
            with self.subTest(expression=expression):
                self.spec.write_text(
                    "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n"
                    "input: {SRC: {path: input.csv, types: {ID: str, VALUE: float, ZERO: float}}}\n"
                    "columns:\n  - {name: ID, type: str, derivation: SRC.ID}\n"
                    "  - {name: VALUE, type: float, derivation: SRC.VALUE}\n"
                    "  - {name: ZERO, type: float, derivation: SRC.ZERO}\n"
                    f"  - {{name: RATIO, type: float, derivation: {{compute: {{expr: '{expression}'}}}}}}\n"
                    "output: {path: output.csv, columns: [ID, VALUE, ZERO, RATIO]}\n",
                    encoding="utf-8",
                )
                result = yamaa.domain(self.spec)
                frame = self.issues(result)
                (record,) = result.diagnostics
                self.assertIsNone(result.output)
                self.assertEqual(record.phase, "derivation")
                self.assertEqual(record.condition, "division_by_zero")
                self.assertEqual(record.requirement, "REQ-0430")
                self.assertEqual(
                    record.spec_paths, ["columns.RATIO.derivation.compute"]
                )
                self.assertEqual(record.context["expr"], expression)
                self.assertEqual(record.context["keys"], [{"ID": "a"}])
                self.assertEqual(record.context, json.loads(frame["context"][0]))
                self.assertIsInstance(record.source_span, yamaa.SourceSpan)
                span = record.source_span
                self.assertEqual(expression[span.start : span.end], "VALUE / ZERO")
                self.assertEqual(record.operand_route, route)
                self.assertIsNotNone(record.operand_route)
                self.assertEqual(result.diagnostics, [record])

    def test_records_equal_the_shared_parity_truth_field_for_field(self):
        # R reads the same cases; both hosts equal one truth, so they equal
        # each other. Numbers are tagged in the truth, because a JSON reader
        # would otherwise choose each host's number types.
        for case in PARITY["cases"]:
            with self.subTest(case=case["name"]):
                spec = self.root / case["name"] / "spec.yaml"
                spec.parent.mkdir()
                if case["specification_yaml"] is not None:
                    spec.write_text(case["specification_yaml"], encoding="utf-8")
                    (spec.parent / "input.csv").write_bytes(
                        fixture_bytes(case["input_csv"])
                    )
                entry = {"check": yamaa.check, "domain": yamaa.domain}[case["entry"]]
                result = entry(spec)
                self.issues(result)
                self.assertEqual(len(result.diagnostics), len(case["records"]))
                for record, truth in zip(
                    result.diagnostics, case["records"], strict=True
                ):
                    span = truth["source_span"]
                    self.assertEqual(
                        (
                            record.phase,
                            record.condition,
                            record.requirement,
                            record.spec_paths,
                            record.source_span,
                            record.operand_route,
                        ),
                        (
                            truth["phase"],
                            truth["condition"],
                            truth["requirement"],
                            truth["spec_paths"],
                            None if span is None else yamaa.SourceSpan(*span),
                            truth["operand_route"],
                        ),
                    )
                    self.assertTrue(
                        same_value(record.context, parity_value(truth["context"])),
                        (record.context, truth["context"]),
                    )


PARITY = load_truth(__file__, "diagnostic-parity")


def fixture_bytes(value):
    if isinstance(value, dict):
        return bytes.fromhex(value["hex"])
    return value.encode("utf-8")


def parity_value(value):
    """Build the Python value a tagged truth value names."""
    if isinstance(value, dict) and len(value) == 1:
        (tag, text), *_ = value.items()
        if tag == "integer":
            return int(text)
        if tag == "float":
            return float(text)
        if tag == "hex":
            return bytes.fromhex(text).decode("utf-8")
    if isinstance(value, dict):
        return {key: parity_value(item) for key, item in value.items()}
    if isinstance(value, list):
        return [parity_value(item) for item in value]
    if isinstance(value, (int, float)):
        raise TypeError("parity truth tags every number")
    return value


def same_value(actual, expected):
    """Equal values of the same type, with object keys in the same order."""
    if type(actual) is not type(expected):
        return False
    if isinstance(expected, dict):
        return list(actual) == list(expected) and all(
            same_value(actual[key], expected[key]) for key in expected
        )
    if isinstance(expected, list):
        return len(actual) == len(expected) and all(map(same_value, actual, expected))
    return actual == expected


class ResolvedSpecificationContract(unittest.TestCase):
    def test_original_origins_survive_inheritance_materialization_and_window_expansion(
        self,
    ):
        with tempfile.TemporaryDirectory(prefix="yamaa-resolved-origins-") as temporary:
            root = Path(temporary).resolve()
            base = root / "base.yaml"
            child = root / "child.yaml"
            base.write_text(
                "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {RAW: missing.csv}\n"
                "columns: [{name: ID, type: str, derivation: {mapping: {source: RAW.ID, dict: {A: Alpha}}}}]\n"
                "output: {path: result.csv, columns: [ID]}\n",
                encoding="utf-8",
            )
            child.write_text(
                "schema_version: '1.0'\nparents: base.yaml\n", encoding="utf-8"
            )
            direct = yamaa.resolve_specification(base).to_dict()["entries"][0][
                "origins"
            ]
            inherited = yamaa.resolve_specification(child).to_dict()["entries"][0][
                "origins"
            ]
            pointer = "/columns/0/derivation/value/mapping/case_sensitive"
            self.assertEqual(direct[pointer]["source_kind"], "schema_default")
            self.assertEqual(inherited[pointer], direct[pointer])
            self.assertTrue(inherited["/input/RAW"]["generated"])
            base.write_text(
                "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {RAW: missing.csv}\n"
                "windows: {W: {order_by: [RAW.ID]}}\n"
                "columns: [{name: ID, type: int, derivation: RAW.ID}]\n"
                "output: {path: result.csv, columns: [ID]}\n",
                encoding="utf-8",
            )
            child.write_text(
                "schema_version: '1.0'\nparents: base.yaml\n"
                "columns: [{name: N, type: int, derivation: {row_number: {window: W}}}]\n"
                "output: {path: result.csv, columns: [ID, N]}\n",
                encoding="utf-8",
            )
            origins = yamaa.resolve_specification(child).to_dict()["entries"][0][
                "origins"
            ]
            prefix = "/columns/1/derivation/value/row_number/window"
            self.assertEqual(origins[prefix]["declaring_source"], child.as_posix())
            self.assertEqual(
                origins[prefix + "/order_by/0/variable"]["declaring_source"],
                base.as_posix(),
            )
            self.assertEqual(
                origins[prefix + "/order_by/0/direction"]["source_kind"],
                "schema_default",
            )
            self.assertTrue(
                origins["/columns/0/derivation/value/source/variable"]["generated"]
            )

    def test_explicit_environment_is_admitted_without_activating_its_callable(self):
        with tempfile.TemporaryDirectory(
            prefix="yamaa-resolved-environment-"
        ) as temporary:
            root = Path(temporary).resolve()
            source = root / "spec.yaml"
            environment = root / "environment.yaml"
            source.write_text(
                "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n"
                "input: {RAW: {path: missing.csv, types: {ID: int, V: str}}}\n"
                "columns: [{name: ID, type: int, derivation: RAW.ID}, "
                "{name: V, type: str, derivation: {function: {name: text, args: {x: RAW.V}}}}]\n"
                "output: {path: output.csv, columns: [ID, V]}\n",
                encoding="utf-8",
            )
            environment.write_text(
                "schema_version: '1.0'\nlanguage: python\nfunctions:\n"
                "  text:\n    function: uninstalled_resolved_fixture.identity\n"
                "    description: Identity.\n    params: [{name: x, type: str}]\n"
                "    returns: str\n    tests:\n"
                "      - {id: normal, covers: [normal], args: {x: a}, result: a}\n"
                "      - {id: empty, covers: [boundary], args: {x: ''}, result: ''}\n"
                "      - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n",
                encoding="utf-8",
            )
            with (
                patch.object(
                    host,
                    "resolve_callable",
                    side_effect=AssertionError("unexpected activation"),
                ),
                patch.object(
                    host,
                    "verify_versions",
                    side_effect=AssertionError("unexpected verification"),
                ),
            ):
                artifact = yamaa.resolve_specification(source, environment=environment)
            self.assertEqual(artifact.to_dict()["environment"], environment.as_posix())

    def test_two_studies_keep_producer_entry_bindings_used_by_domain(self):
        with tempfile.TemporaryDirectory(
            prefix="yamaa-resolved-producers-"
        ) as temporary:
            root = Path(temporary).resolve()
            (root / "shared").mkdir()
            (root / "shared/producer.yaml").write_text(
                "schema_version: '1.0'\ndomain: PRODUCER\nkeys: [ID]\n"
                "input: {RAW: {path: input/raw.csv, relative_to: entry, types: {ID: int}}}\n"
                "columns: [{name: ID, type: int, label: Identifier, derivation: RAW.ID}]\n"
                "output: {path: output/produced.csv, relative_to: entry, columns: [ID]}\n",
                encoding="utf-8",
            )
            (root / "shared/consumer.yaml").write_text(
                "schema_version: '1.0'\ndomain: CONSUMER\nkeys: [ID]\n"
                "input: {P: {path: output/produced.csv, schema: producer.yaml, relative_to: entry}}\n"
                "columns: [{name: ID, type: int, label: Identifier, derivation: P.ID}]\n"
                "output: {path: output/final.csv, relative_to: entry, columns: [ID]}\n",
                encoding="utf-8",
            )
            for wrapper, identifier in (("A", 17), ("B", 29)):
                study = root / wrapper
                (study / "input").mkdir(parents=True)
                (study / "input/raw.csv").write_text(
                    f"ID\n{identifier}\n", encoding="utf-8"
                )
                (study / "spec.yaml").write_text(
                    "schema_version: '1.0'\nparents: ../shared/consumer.yaml\n",
                    encoding="utf-8",
                )
                (study / "producer.yaml").write_text(
                    "schema_version: '1.0'\nparents: ../shared/producer.yaml\n",
                    encoding="utf-8",
                )
                artifact = yamaa.resolve_specification(study / "spec.yaml")
                root_entry, producer = artifact.to_dict()["entries"]
                self.assertEqual(
                    root_entry["producers"], {"P": (study / "producer.yaml").as_posix()}
                )
                resource = next(
                    item
                    for item in producer["resources"]
                    if item["role"] == "input_data"
                )
                self.assertEqual(
                    resource["location"]["path"], (study / "input/raw.csv").as_posix()
                )
                result = yamaa.domain(study / "spec.yaml")
                self.assertTrue(result.issues.is_empty(), result.issues)
                self.assertEqual(result.output["ID"].to_list(), [identifier])
                self.assertFalse((study / "output").exists())

    def test_owned_inspection_and_explicit_round_trip_without_sources_or_callbacks(
        self,
    ):
        with tempfile.TemporaryDirectory(prefix="yamaa-resolved-public-") as temporary:
            root = Path(temporary).resolve()
            spec = root / "spec.yaml"
            spec.write_text(
                "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n"
                "input: {RAW: {path: missing/raw.csv, types: {ID: int}}}\n"
                "columns: [{name: ID, type: int, derivation: RAW.ID}]\n"
                "output: {path: absent/output.csv, columns: [ID]}\n",
                encoding="utf-8",
            )
            with (
                patch.object(
                    host,
                    "resolve_callable",
                    side_effect=AssertionError("unexpected activation"),
                ),
                patch.object(
                    host,
                    "verify_versions",
                    side_effect=AssertionError("unexpected verification"),
                ),
            ):
                artifact = yamaa.resolve_specification(spec)
            facts = artifact.to_dict()
            self.assertEqual(facts["format"], "yamaa.resolved-specification")
            self.assertEqual(facts["artifact_version"], 1)
            self.assertEqual(facts["replay"], "inspection_only")
            self.assertEqual(facts["entry"], spec.as_posix())
            resources = facts["entries"][0]["resources"]
            source = next(item for item in resources if item["role"] == "input_data")
            self.assertEqual(
                source["location"],
                {"kind": "local", "path": (root / "missing/raw.csv").as_posix()},
            )
            self.assertFalse((root / "absent").exists())
            serialized = artifact.to_json()
            facts["entries"][0]["specification"]["domain"] = "MUTATED"
            self.assertEqual(
                artifact.to_dict()["entries"][0]["specification"]["domain"], "TEST"
            )
            spec.unlink()
            previous = Path.cwd()
            try:
                import os

                os.chdir(root.parent)
                restored = yamaa.ResolvedSpecification.from_json(serialized)
                self.assertEqual(restored.to_json(), serialized)
                self.assertEqual(restored.to_dict(), artifact.to_dict())
            finally:
                os.chdir(previous)
            with self.assertRaises(yamaa.ResolvedSpecificationError) as error:
                yamaa.resolve_specification(spec)
            self.assertGreater(error.exception.issues.height, 0)
            self.assertEqual(
                [record.condition for record in error.exception.diagnostics],
                error.exception.issues["condition"].to_list(),
            )
            self.assertTrue(
                all(
                    isinstance(record, yamaa.Diagnostic)
                    for record in error.exception.diagnostics
                )
            )
            invalid = json.loads(serialized)
            invalid["artifact_version"] = 2
            with self.assertRaises(ValueError):
                yamaa.ResolvedSpecification.from_json(json.dumps(invalid))
            with self.assertRaises(ValueError):
                yamaa.ResolvedSpecification.from_json("schema_version: '1.0'")


if __name__ == "__main__":
    unittest.main()
