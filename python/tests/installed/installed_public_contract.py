"""Installed public API contracts with independent authored inputs and truth."""

from __future__ import annotations

import datetime
import gc
import importlib.util
import inspect
import json
import math
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
PARITY = load_truth(__file__, "diagnostic-parity")

SPEC = TRUTH["specification_yaml"]
INPUT = TRUTH["python_input_csv"].encode("utf-8")
VALUES = [
    None if value is None else int(value) for value in TRUTH["python_values_decimal"]
]
MESSAGES = TRUTH["messages"]
ISSUE_TYPES = [
    pl.String,
    pl.String,
    pl.String,
    pl.List(pl.String),
    pl.String,
    pl.List(pl.Int64),
    pl.List(pl.String),
]
REMOVED = (
    "domain",
    "check",
    "define",
    "resolve_specification",
    "write_odm_parquet",
    "ResolvedSpecification",
    "Diagnostic",
    "SourceSpan",
    "DomainError",
    "DefineError",
    "ResolvedSpecificationError",
    "resources",
)


def public_names(value):
    return sorted(name for name in dir(value) if not name.startswith("_"))


def arguments(function):
    return list(inspect.signature(function).parameters)


class PublicContract(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="yamaa-public-contract-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.spec = self.root / "spec.yaml"
        self.spec.write_text(SPEC, encoding="utf-8")
        (self.root / "input.csv").write_bytes(INPUT)

    def issues(self, result):
        """The one findings view: seven typed columns, context as JSON text."""
        frame = result.issues
        self.assertIsInstance(frame, pl.DataFrame)
        self.assertEqual(frame.columns, TRUTH["issue_columns"])
        self.assertEqual(frame.dtypes, ISSUE_TYPES)
        for row in frame.to_dicts():
            self.assertTrue(row["condition"])
            self.assertIsInstance(json.loads(row["context"]), dict)
            span = row["source_span"]
            if span is not None:
                self.assertEqual(len(span), 2)
                self.assertLessEqual(0, span[0])
                self.assertLessEqual(span[0], span[1])
        return frame

    def refusal(self, call):
        """Run `call`, require a yamaa.Error, and return it."""
        with self.assertRaises(yamaa.Error) as caught:
            call()
        error = caught.exception
        self.assertIs(type(error), yamaa.Error)
        self.assertIsNone(error.__cause__)
        self.assertTrue(error.__suppress_context__)
        self.issues(error)
        return error

    def test_exports_native_surface_and_version(self):
        from yamaa import _native

        self.assertEqual(
            yamaa.__all__, ["Error", "__version__", "project", "read_odm", "spec"]
        )
        self.assertTrue(issubclass(yamaa.Error, Exception))
        for name in REMOVED:
            with self.subTest(name=name):
                self.assertFalse(hasattr(yamaa, name))
        for module in ("yamaa.resources", "yamaa._odm"):
            self.assertIsNone(importlib.util.find_spec(module))
        self.assertEqual(
            sorted(name for name in dir(_native) if not name.startswith("__")),
            [
                "_ArrowTable",
                "_CORE_VERSION",
                "_Error",
                "_ProjectRun",
                "_Run",
                "_Submission",
                "_project_check",
                "_project_run",
                "_project_spec",
                "_project_submission",
                "_read_odm",
                "_spec_check",
                "_spec_resolve",
                "_spec_run",
                "_style_same_value",
            ],
        )
        self.assertEqual(yamaa.__version__, "0.4.0")
        self.assertEqual(_native._CORE_VERSION, yamaa.__version__)

    def test_signatures_members_and_argument_types(self):
        expected = TRUTH["arguments"]
        project = yamaa.project(self.root / "project.yaml")
        spec = yamaa.spec(self.spec)
        callables = {
            "spec": yamaa.spec,
            "project": yamaa.project,
            "read_odm": yamaa.read_odm,
            "spec.check": spec.check,
            "spec.resolve": spec.resolve,
            "spec.run": spec.run,
            "project.spec": project.spec,
            "project.check": project.check,
            "project.run": project.run,
            "project.submission": project.submission,
        }
        for name, function in callables.items():
            with self.subTest(name=name):
                self.assertEqual(arguments(function), expected[name])
        options = inspect.signature(yamaa.read_odm).parameters
        self.assertEqual(
            options["source"].kind, inspect.Parameter.POSITIONAL_OR_KEYWORD
        )
        self.assertIsNone(options["archive_member"].default)
        for name, default in TRUTH["read_odm_limits"].items():
            self.assertEqual(options[name].default, int(default))
        for name in expected["read_odm"][1:]:
            self.assertEqual(options[name].kind, inspect.Parameter.KEYWORD_ONLY)
        self.assertIsNone(inspect.signature(project.run).parameters["section"].default)
        submission = inspect.signature(project.submission).parameters
        self.assertIsNone(submission["section"].default)
        self.assertIsNone(submission["created"].default)
        self.assertEqual(submission["created"].kind, inspect.Parameter.KEYWORD_ONLY)

        members = TRUTH["members"]
        self.assertEqual(public_names(spec), members["spec"])
        self.assertEqual(public_names(project), members["project"])
        self.assertEqual(public_names(spec.check()), members["check"])
        run = spec.run()
        self.assertEqual(public_names(run), members["run"])
        self.assertEqual(arguments(run.save), expected["run.save"])

        for value in (None, 7, True, [], object()):
            with self.subTest(value=type(value).__name__):
                for function in (yamaa.spec, yamaa.project, yamaa.read_odm):
                    with self.assertRaises(TypeError):
                        function(value)
                with self.assertRaises(TypeError):
                    project.spec(value)
                if value is not None:
                    for call in (
                        lambda: project.run(value),
                        lambda: project.submission(value),
                        lambda: project.submission(created=value),
                        lambda: yamaa.read_odm(self.spec, archive_member=value),
                    ):
                        with self.assertRaises(TypeError):
                            call()
        # The removed options are gone, not ignored.
        for option in ("environment", "resolver"):
            with self.subTest(option=option):
                with self.assertRaises(TypeError):
                    yamaa.spec(self.spec, **{option: None})
                for method in (spec.check, spec.resolve, spec.run, project.check):
                    with self.assertRaises(TypeError):
                        method(**{option: None})
        with self.assertRaises(TypeError):
            run.save(self.root / "other.csv")
        self.assertFalse((self.root / "output.csv").exists())

    def test_missing_specification_is_an_issue_and_failed_save_raises(self):
        spec = yamaa.spec(self.root / "absent.yaml")
        checked = self.issues(spec.check())
        self.assertEqual(checked["condition"].to_list(), ["engine_rejected"])
        self.assertEqual(
            json.loads(checked["context"][0]),
            {"code": "resource_missing", "stage": "prepare"},
        )
        run = spec.run()
        self.assertIsNone(run.output)
        self.assertIsNone(run.report)
        self.assertTrue(self.issues(run).equals(checked))
        for _ in range(2):
            error = self.refusal(run.save)
            self.assertEqual(str(error), MESSAGES["failed_run_save"])
            self.assertTrue(error.issues.equals(checked))
            self.assertTrue(run.issues.equals(checked))
        error = self.refusal(spec.resolve)
        self.assertEqual(str(error), MESSAGES["resolve"])
        self.assertTrue(error.issues.equals(checked))
        self.assertFalse((self.root / "output.csv").exists())

    def test_results_are_snapshots_and_save_publishes_held_bytes(self):
        first, second = yamaa.spec(self.spec).run(), yamaa.spec(self.spec).run()
        for result in (first, second):
            self.assertTrue(self.issues(result).is_empty())
            self.assertEqual(result.output["VALUE"].to_list(), VALUES)
            self.assertEqual(result.report.columns, TRUTH["report_columns"])
            self.assertEqual(
                result.report["KIND"].to_list(),
                ["run", "input", "rows", "column", "column"],
            )
            self.assertEqual(result.report["OUTCOME"][0], "succeeded")
            self.assertEqual(result.report["COUNT"][0], 3)
            for name in TRUTH["read_only_views"]["run"]:
                with self.subTest(name=name), self.assertRaises(AttributeError):
                    setattr(result, name, None)
        self.assertFalse((self.root / "output.csv").exists())
        view = first.output
        view[0, "VALUE"] = 9
        self.assertEqual(first.output["VALUE"].to_list(), VALUES)
        self.spec.unlink()
        (self.root / "input.csv").unlink()
        gc.collect()
        for _ in range(2):
            self.assertIsNone(first.save())
            self.assertEqual((self.root / "output.csv").read_bytes(), INPUT)
        del first
        gc.collect()
        self.assertEqual(second.output["VALUE"].to_list(), VALUES)
        self.assertIsNone(second.save())

    def test_typed_columns_keep_their_exact_values(self):
        self.spec.write_text(TRUTH["typed_specification_yaml"], encoding="utf-8")
        (self.root / "input.csv").write_bytes(TRUTH["typed_input_csv"].encode("utf-8"))
        result = yamaa.spec(self.spec).run()
        self.assertTrue(self.issues(result).is_empty(), result.issues)
        frame = result.output
        self.assertEqual(
            frame.dtypes[:5], [pl.Int64, pl.String, pl.Int64, pl.Float64, pl.Date]
        )
        self.assertIsInstance(frame.dtypes[5], pl.Datetime)
        self.assertIsNone(frame.dtypes[5].time_zone)
        truth = TRUTH["typed_values"]
        epoch = datetime.datetime(1970, 1, 1)
        for name, convert in (
            ("ID", int),
            ("S", str),
            ("I", int),
            ("F", float),
            ("D", lambda days: epoch.date() + datetime.timedelta(days=int(days))),
            ("T", lambda seconds: epoch + datetime.timedelta(seconds=int(seconds))),
        ):
            with self.subTest(column=name):
                self.assertEqual(
                    frame[name].to_list(),
                    [
                        None if value is None else convert(value)
                        for value in truth[name]
                    ],
                )
        self.assertEqual(math.copysign(1.0, frame["F"][0]), -1.0)

    def test_text_with_nul_and_empty_literal_are_distinct_from_missing(self):
        self.spec.write_text(
            SPEC.replace("VALUE, type: int", "VALUE, type: str")
            .replace("output: {", TRUTH["empty_text_column"])
            .replace("columns: [ID, VALUE]", "columns: [ID, VALUE, EMPTY]"),
            encoding="utf-8",
        )
        (self.root / "input.csv").write_bytes(b"ID,VALUE\na,a\x00b\nb,\xc3\xa9\nc,\n")
        result = yamaa.spec(self.spec).run()
        self.assertTrue(self.issues(result).is_empty())
        self.assertEqual(result.output["VALUE"].to_list(), ["a\x00b", "é", None])
        self.assertEqual(result.output["EMPTY"].to_list(), ["", "", ""])
        self.assertIsNone(result.save())
        self.assertEqual(
            (self.root / "output.csv").read_bytes(),
            b'ID,VALUE,EMPTY\na,a\x00b,""\nb,\xc3\xa9,""\nc,,""\n',
        )

    def test_failed_run_writes_its_report_and_raises(self):
        self.spec.write_text(
            SPEC.replace(
                TRUTH["integer_column_yaml"], TRUTH["fatal_integer_column_yaml"]
            )
            .replace("output: {", TRUTH["unreached_row_count_yaml"])
            .replace("columns: [ID, VALUE]}", TRUTH["report_output_yaml"]),
            encoding="utf-8",
        )
        (self.root / "input.csv").write_bytes(TRUTH["failed_input_csv"].encode())
        for name in ("output.csv", "checks.csv"):
            (self.root / name).write_bytes(b"sentinel")
        result = yamaa.spec(self.spec).run()
        issues = self.issues(result).to_dicts()
        self.assertEqual(
            [(row["phase"], row["condition"], row["requirement"]) for row in issues],
            [("verification", "range_failed", "REQ-0377")],
        )
        self.assertEqual(
            json.loads(issues[0]["context"]),
            {
                "column": "VALUE",
                "failure_count": 1,
                "keys": [{"ID": "b"}],
                "verification_id": "fatal",
            },
        )
        self.assertIsNone(result.output)
        report = result.report
        self.assertEqual(report.columns, TRUTH["report_columns"])
        self.assertEqual(
            report["KIND"].to_list(), ["run", "input", "rows", "check", "check"]
        )
        checks = report.filter(pl.col("KIND") == "check")
        self.assertEqual(checks["ID"].to_list(), ["warn", "fatal"])
        self.assertEqual(checks["OUTCOME"].to_list(), ["violated", "violated"])
        self.assertEqual(checks["COUNT"].to_list(), [1, 1])
        self.assertEqual(checks["TOTAL"].to_list(), [3, 3])
        self.assertEqual(checks["RULE"].to_list(), ["REQ-0377", "REQ-0377"])
        truth = report.to_dicts()
        report[0, "OUTCOME"] = "changed"
        self.spec.unlink()
        (self.root / "input.csv").unlink()
        gc.collect()
        for _ in range(2):
            error = self.refusal(result.save)
            self.assertEqual(str(error), MESSAGES["failed_run_save"])
            self.assertEqual(error.issues.to_dicts(), issues)
            self.assertEqual(result.issues.to_dicts(), issues)
            self.assertEqual(result.report.to_dicts(), truth)
            self.assertEqual((self.root / "output.csv").read_bytes(), b"sentinel")
            self.assertEqual(
                (self.root / "checks.csv").read_bytes(),
                TRUTH["failed_report_csv"].encode("utf-8"),
            )
        held = result.report
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
        return yamaa.project(case / "project.yaml"), case

    def test_each_callback_stage_preserves_original_interrupt_identity(self):
        project, case = self.function_study()
        verify, resolve = host.verify_versions, host.resolve_callable
        entries = {
            "spec": lambda: project.spec("TEST").run(),
            "project": lambda: project.run(),
        }
        for kind in (KeyboardInterrupt, SystemExit):
            for stage in ("lock", "binding", "conformance", "derivation"):
                for entry, call in entries.items():
                    with self.subTest(kind=kind.__name__, stage=stage, entry=entry):
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
                            call()
                        self.assertIs(seen.exception, original)
                        self.assertEqual(
                            len(calls),
                            {"conformance": 1, "derivation": 3}.get(stage, 0),
                        )
                        self.assertFalse((case / "test.csv").exists())

    def test_callback_can_reenter_public_check_and_run(self):
        project, case = self.function_study()
        resolve = host.resolve_callable
        calls = []

        def resolve_reentrant(reference, parameters):
            invoke = resolve(reference, parameters)

            def callback(**arguments):
                checked = yamaa.spec(self.spec).check()
                nested = yamaa.spec(self.spec).run()
                self.assertTrue(checked.issues.is_empty())
                self.assertTrue(nested.issues.is_empty())
                self.assertEqual(nested.output["VALUE"].to_list(), VALUES)
                calls.append(nested)
                return invoke(**arguments)

            return callback

        with patch.object(host, "resolve_callable", side_effect=resolve_reentrant):
            result = project.spec("TEST").run()
        self.assertTrue(self.issues(result).is_empty(), result.issues)
        self.assertGreaterEqual(len(calls), 3)
        self.spec.unlink()
        gc.collect()
        self.assertEqual(calls[0].output["VALUE"].to_list(), VALUES)
        self.assertFalse((self.root / "output.csv").exists())
        self.assertFalse((case / "test.csv").exists())

    def assert_callback_exception_lifetime(self, reentrant):
        project, case = self.function_study()
        reentries = []

        class OriginalError(RuntimeError):
            def __str__(error):
                if reentrant:
                    nested = yamaa.spec(self.spec).run()
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
            result = project.spec("TEST").run()
        self.assertIsNone(result.output)
        self.assertEqual(
            set(self.issues(result)["condition"]), {"function_conformance_failed"}
        )
        for row in result.issues.to_dicts():
            failure = json.loads(row["context"])["failure"]
            self.assertEqual(failure["context"]["host_error"], "OriginalError")
            self.assertEqual(
                failure["context"]["host_message"], "retained callback failure"
            )
        held.clear()
        gc.collect()
        # The run owns the original failure as long as the run itself lives.
        self.assertIsNotNone(failure_reference())
        self.refusal(result.save)
        del result
        gc.collect()
        self.assertIsNone(failure_reference())
        self.assertFalse((case / "test.csv").exists())
        self.assertEqual(bool(reentries), reentrant)

    def test_callback_exception_lives_exactly_as_long_as_its_public_result(self):
        self.assert_callback_exception_lifetime(False)

    def test_reentrant_exception_rendering_keeps_owned_failures_collectable(self):
        self.assert_callback_exception_lifetime(True)

    def test_callback_exception_keeps_its_complete_unicode_and_nul_message(self):
        project, _ = self.function_study()
        message = "é\x00" * 30_000 + "complete failure"
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
            result = project.spec("TEST").run()
        self.assertEqual(len(calls), 3)
        self.assertIsNone(result.output)
        self.assertEqual(
            self.issues(result)["condition"].to_list(), ["function_call_failed"]
        )
        context = json.loads(result.issues["context"][0])
        self.assertEqual(context["host_error"], "RuntimeError")
        self.assertEqual(context["host_message"], message)
        self.assertNotIn("host_details_truncated", context)
        self.refusal(result.save)

    def test_callback_string_larger_than_one_mebibyte_is_owned_and_saved_exactly(self):
        _, case = self.function_study()
        environment = case / "python/environment.yaml"
        environment.write_text(
            "schema_version: '1.0'\nlock: uv.lock\n"
            "functions:\n  text:\n    function: yamaa_benchmarks.project_value\n"
            "    description: Return text unchanged.\n"
            "    params: [{name: x, type: str}]\n    returns: str\n    tests:\n"
            "      - {id: normal, covers: [normal], args: {x: short}, result: short}\n"
            "      - {id: empty, covers: [boundary], args: {x: ''}, result: ''}\n"
            "      - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n",
            encoding="utf-8",
        )
        source = case / "spec.yaml"
        source.write_text(
            "schema_version: '1.0'\ndomain: TEXT\nkeys: [ID]\n"
            "input: {SRC: {path: large.csv, types: {ID: int, V: str}}}\n"
            "columns:\n  - {name: ID, type: int, derivation: SRC.ID}\n"
            "  - {name: V, type: str, derivation: {function: {name: text, args: {x: SRC.V}}}}\n"
            "output: {path: held.csv, columns: [ID, V]}\n",
            encoding="utf-8",
        )
        value = "é" * 524_289 + "complete"
        expected = ("ID,V\n1," + value + "\n").encode("utf-8")
        (case / "large.csv").write_bytes(expected)
        result = yamaa.project(case / "project.yaml").spec("TEXT").run()
        self.assertTrue(self.issues(result).is_empty(), result.issues)
        self.assertEqual(result.output["V"].to_list(), [value])
        for name in (
            "spec.yaml",
            "project.yaml",
            "python/environment.yaml",
            "large.csv",
        ):
            (case / name).unlink()
        for _ in range(2):
            self.assertIsNone(result.save())
            self.assertEqual((case / "held.csv").read_bytes(), expected)

    def test_project_refusals_are_issues_and_unknown_sections_raise(self):
        project = yamaa.project(self.root / "absent-project.yaml")
        error = self.refusal(lambda: project.spec("TEST"))
        self.assertEqual(str(error), MESSAGES["unread_project"])
        unread = error.issues.to_dicts()
        self.assertEqual(
            [(row["condition"], row["spec_paths"]) for row in unread],
            [("project_environment_missing", ["environment"])],
        )
        checked = self.issues(project.check())
        self.assertEqual(checked.to_dicts(), unread)
        runs = project.run()
        self.assertEqual(runs.names, [])
        self.assertEqual(self.issues(runs).to_dicts(), unread)
        self.assertEqual(runs.report.columns, TRUTH["report_columns"])
        self.assertTrue(runs.report.is_empty())
        self.assertEqual(public_names(runs), TRUTH["members"]["project_run"])
        error = self.refusal(runs.save)
        self.assertEqual(str(error), MESSAGES["refused_project_run_save"])
        self.assertEqual(error.issues.to_dicts(), unread)
        error = self.refusal(lambda: runs["ADSL"])
        self.assertEqual(str(error), MESSAGES["no_member_built"])
        self.assertTrue(error.issues.is_empty())
        submission = project.submission("sdtm", created="2026-09-11T00:00:00")
        self.assertEqual(public_names(submission), TRUTH["members"]["submission"])
        self.assertEqual(self.issues(submission).to_dicts(), unread)
        for _ in range(2):
            error = self.refusal(submission.save)
            self.assertEqual(str(error), MESSAGES["refused_submission_save"])
            self.assertTrue(error.issues.equals(submission.issues))
        for name in TRUTH["read_only_views"]["project_run"]:
            with self.subTest(name=name), self.assertRaises(AttributeError):
                setattr(runs, name, None)
        with self.assertRaises(AttributeError):
            submission.issues = None
        for call, message in (
            (lambda: project.run("bogus"), MESSAGES["unknown_section_run"]),
            (
                lambda: project.submission("bogus"),
                MESSAGES["unknown_section_submission"],
            ),
        ):
            error = self.refusal(call)
            self.assertEqual(str(error), message)
            (row,) = error.issues.to_dicts()
            self.assertEqual(
                (row["phase"], row["condition"]), ("engine", "engine_rejected")
            )
            self.assertEqual(
                json.loads(row["context"]),
                {
                    "code": "unknown_section",
                    "section": "bogus",
                    "sections": ["sdtm", "adam", "send"],
                    "stage": "project",
                },
            )
        self.assertEqual(
            sorted(path.name for path in self.root.iterdir()),
            ["input.csv", "spec.yaml"],
        )

    def test_issue_rows_keep_exact_wide_integers_and_recorded_geometry(self):
        # A literal beyond every 64-bit range is refused before study access;
        # its context carries the signed 64-bit bounds and the exact literal.
        self.spec.write_text(
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\n"
            "columns:\n  - {name: ID, type: str, derivation: SRC.ID}\n"
            "  - {name: N, type: int, derivation: {literal: 18446744073709551616}}\n"
            "output: {path: output.csv, columns: [ID, N]}\n",
            encoding="utf-8",
        )
        (row,) = self.issues(yamaa.spec(self.spec).check()).to_dicts()
        self.assertEqual(
            (row["phase"], row["condition"], row["requirement"], row["spec_paths"]),
            (
                "validation",
                "integer_overflow",
                "REQ-0014",
                ["columns.N.derivation.literal"],
            ),
        )
        context = json.loads(row["context"])
        self.assertEqual(
            context, {"minimum": -(2**63), "maximum": 2**63 - 1, "actual": 2**64}
        )
        self.assertTrue(all(type(value) is int for value in context.values()))
        # A definition finding records no geometry: None, not an empty list.
        self.assertIsNone(row["source_span"])
        self.assertIsNone(row["operand_route"])

        # A division by zero fails at one node of the compute expression: the
        # span covers that node's bytes and the route leads to it from the
        # root, so a failure at the root has a present empty route.
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
                result = yamaa.spec(self.spec).run()
                self.assertIsNone(result.output)
                (row,) = self.issues(result).to_dicts()
                self.assertEqual(row["condition"], "division_by_zero")
                start, end = row["source_span"]
                self.assertEqual(expression[start:end], "VALUE / ZERO")
                self.assertEqual(row["operand_route"], route)
                self.assertIsNotNone(row["operand_route"])
                self.assertEqual(json.loads(row["context"])["keys"], [{"ID": "a"}])

    def test_issue_rows_equal_the_shared_parity_truth(self):
        # R reads the same cases; both hosts equal one truth, so they equal
        # each other. The context is compared as its canonical JSON text, so
        # no JSON reader chooses a number type.
        for case in PARITY["cases"]:
            with self.subTest(case=case["name"]):
                spec = self.root / case["name"] / "spec.yaml"
                spec.parent.mkdir()
                if case["specification_yaml"] is not None:
                    spec.write_text(case["specification_yaml"], encoding="utf-8")
                    (spec.parent / "input.csv").write_bytes(
                        fixture_bytes(case["input_csv"])
                    )
                call = getattr(yamaa.spec(spec), case["call"])
                self.assertEqual(self.issues(call()).to_dicts(), case["issues"])


def fixture_bytes(value):
    if isinstance(value, dict):
        return bytes.fromhex(value["hex"])
    return value.encode("utf-8")


class Resolve(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="yamaa-resolve-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()

    def test_original_origins_survive_inheritance_materialization_and_window_expansion(
        self,
    ):
        base = self.root / "base.yaml"
        child = self.root / "child.yaml"
        base.write_text(
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {RAW: missing.csv}\n"
            "columns: [{name: ID, type: str, derivation: {mapping: {source: RAW.ID, dict: {A: Alpha}}}}]\n"
            "output: {path: result.csv, columns: [ID]}\n",
            encoding="utf-8",
        )
        child.write_text(
            "schema_version: '1.0'\nparents: base.yaml\n", encoding="utf-8"
        )
        direct = yamaa.spec(base).resolve()["entries"][0]["origins"]
        inherited = yamaa.spec(child).resolve()["entries"][0]["origins"]
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
        origins = yamaa.spec(child).resolve()["entries"][0]["origins"]
        prefix = "/columns/1/derivation/value/row_number/window"
        self.assertEqual(origins[prefix]["declaring_source"], child.as_posix())
        self.assertEqual(
            origins[prefix + "/order_by/0/variable"]["declaring_source"],
            base.as_posix(),
        )
        self.assertEqual(
            origins[prefix + "/order_by/0/direction"]["source_kind"], "schema_default"
        )
        self.assertTrue(
            origins["/columns/0/derivation/value/source/variable"]["generated"]
        )

    def test_project_spec_resolves_without_activating_its_callable(self):
        source = self.root / "spec.yaml"
        source.write_text(
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n"
            "input: {RAW: {path: missing.csv, types: {ID: int, V: str}}}\n"
            "columns: [{name: ID, type: int, derivation: RAW.ID}, "
            "{name: V, type: str, derivation: {function: {name: text, args: {x: RAW.V}}}}]\n"
            "output: {path: output.csv, columns: [ID, V]}\n",
            encoding="utf-8",
        )
        (self.root / "environment.yaml").write_text(
            "schema_version: '1.0'\nfunctions:\n"
            "  text:\n    function: uninstalled_resolved_fixture.identity\n"
            "    description: Identity.\n    params: [{name: x, type: str}]\n"
            "    returns: str\n    tests:\n"
            "      - {id: normal, covers: [normal], args: {x: a}, result: a}\n"
            "      - {id: empty, covers: [boundary], args: {x: ''}, result: ''}\n"
            "      - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n",
            encoding="utf-8",
        )
        project = self.root / "project.yaml"
        project.write_text(
            "schema_version: '1.0'\n"
            "environments: {python: environment.yaml, r: environment.yaml}\n"
            "adam: {specs: [spec.yaml]}\n",
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
            facts = yamaa.project(project).spec("TEST").resolve()
        self.assertEqual(facts["project"], project.as_posix())
        self.assertEqual(facts["entry"], source.as_posix())
        # The project supplies the function: the same file alone names an
        # unknown one.
        with self.assertRaises(yamaa.Error) as caught:
            yamaa.spec(source).resolve()
        self.assertEqual(
            caught.exception.issues.select(
                "condition", "requirement", "spec_paths", "context"
            ).rows(),
            [
                (
                    "unknown_project_function",
                    "REQ-0698",
                    ["columns.V.derivation.function.name"],
                    '{"function":"text"}',
                )
            ],
        )

    def test_two_studies_keep_producer_entry_bindings_used_by_run(self):
        (self.root / "shared").mkdir()
        (self.root / "shared/producer.yaml").write_text(
            "schema_version: '1.0'\ndomain: PRODUCER\nkeys: [ID]\n"
            "input: {RAW: {path: input/raw.csv, relative_to: entry, types: {ID: int}}}\n"
            "columns: [{name: ID, type: int, label: Identifier, derivation: RAW.ID}]\n"
            "output: {path: output/produced.csv, relative_to: entry, columns: [ID]}\n",
            encoding="utf-8",
        )
        (self.root / "shared/consumer.yaml").write_text(
            "schema_version: '1.0'\ndomain: CONSUMER\nkeys: [ID]\n"
            "input: {P: {path: output/produced.csv, schema: producer.yaml, relative_to: entry}}\n"
            "columns: [{name: ID, type: int, label: Identifier, derivation: P.ID}]\n"
            "output: {path: output/final.csv, relative_to: entry, columns: [ID]}\n",
            encoding="utf-8",
        )
        for wrapper, identifier in (("A", 17), ("B", 29)):
            study = self.root / wrapper
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
            spec = yamaa.spec(study / "spec.yaml")
            root_entry, producer = spec.resolve()["entries"]
            self.assertEqual(
                root_entry["producers"], {"P": (study / "producer.yaml").as_posix()}
            )
            resource = next(
                item for item in producer["resources"] if item["role"] == "input_data"
            )
            self.assertEqual(
                resource["location"]["path"], (study / "input/raw.csv").as_posix()
            )
            result = spec.run()
            self.assertTrue(result.issues.is_empty(), result.issues)
            self.assertEqual(result.output["ID"].to_list(), [identifier])
            self.assertFalse((study / "output").exists())

    def test_resolve_returns_owned_facts_and_raises_for_an_unreadable_spec(self):
        spec = self.root / "spec.yaml"
        spec.write_text(
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n"
            "input: {RAW: {path: missing/raw.csv, types: {ID: int}}}\n"
            "columns: [{name: ID, type: int, derivation: RAW.ID}]\n"
            "output: {path: absent/output.csv, columns: [ID]}\n",
            encoding="utf-8",
        )
        handle = yamaa.spec(spec)
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
            facts = handle.resolve()
        self.assertIsInstance(facts, dict)
        self.assertEqual(facts["format"], "yamaa.resolved-specification")
        self.assertEqual(facts["artifact_version"], 1)
        self.assertEqual(facts["replay"], "inspection_only")
        self.assertEqual(facts["entry"], spec.as_posix())
        self.assertIsNone(facts["project"])
        resources = facts["entries"][0]["resources"]
        source = next(item for item in resources if item["role"] == "input_data")
        self.assertEqual(
            source["location"],
            {"kind": "local", "path": (self.root / "missing/raw.csv").as_posix()},
        )
        self.assertFalse((self.root / "absent").exists())
        facts["entries"][0]["specification"]["domain"] = "MUTATED"
        self.assertEqual(
            handle.resolve()["entries"][0]["specification"]["domain"], "TEST"
        )
        spec.unlink()
        with self.assertRaises(yamaa.Error) as caught:
            handle.resolve()
        self.assertEqual(str(caught.exception), MESSAGES["resolve"])
        self.assertEqual(
            caught.exception.issues["condition"].to_list(), ["engine_rejected"]
        )


if __name__ == "__main__":
    unittest.main()
