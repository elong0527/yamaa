"""Private installed whole-graph builds against separately authored study truth."""

import gc
import json
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import yamaa_benchmarks as program
from yamaa import _locked_functions as host
from yamaa import _native as native

ROOT = Path(__file__).parent / "project-original"
RAW = b"ID,VALUE\n1,1.234\n2,2.345\n"
PRODUCED = b"ID,VALUE\n1,1.23\n2,2.35\n"
CONSUMED = b"ID,VALUE\n1,2.46\n2,4.70\n"
PRODUCER = """schema_version: '1.0'
domain: TEST
base: RAW
keys: [ID]
input: {RAW: {path: ../../raw/raw.csv, types: {ID: int, VALUE: float}}}
columns:
  - {name: ID, type: int, label: Identifier, derivation: RAW.ID}
  - {name: VALUE, type: float, label: Value, derivation: {function: {name: id_float, args: {x: RAW.VALUE}}}}
output: {path: ../../generated/producer.csv, columns: [ID, VALUE], decimals: 2}
"""
CONSUMER = """schema_version: '1.0'
domain: TEST
base: FIRST
keys: [ID]
input:
  FIRST: {path: ../generated/producer.csv, schema: ../producer/p.yaml}
  SECOND: {path: ../generated/producer.csv, schema: ../producer/./p.yaml}
intermediates: [{id: SECOND_REFERENCE, dataset: SECOND, no_match: null}]
columns:
  - {name: ID, type: int, label: Identifier, derivation: FIRST.ID}
  - {name: FIRST_VALUE, type: float, label: First value, derivation: FIRST.VALUE}
  - {name: SECOND_VALUE, type: float, label: Second value, derivation: SECOND_REFERENCE.VALUE}
  - {name: VALUE, type: float, label: Value, derivation: {compute: {expr: 'FIRST_VALUE + SECOND_VALUE'}}}
output: {path: ../generated/root.csv, columns: [ID, VALUE], decimals: 2}
"""
FUNCTION = """function: yamaa_benchmarks.project_value
description: Installed numeric identity.
params: [{name: x, type: float}]
returns: float
tests:
  - {id: normal, covers: [normal, numeric-comparison], args: {x: 7.25}, result: 7.25}
  - {id: boundary, covers: [boundary], args: {x: 0.0}, result: 0.0}
  - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}
"""
METADATA = (
    "installed-python",
    "0.1.0",
    "independent-producer",
    "consumer/root.yaml",
    ".",
)


def raising_value(failure, original, trigger):
    def value(x):
        if x == trigger:
            raise failure
        return original(x)

    return value


class InstalledProducerBuilds(unittest.TestCase):
    def setUp(self):
        self.work = tempfile.TemporaryDirectory()
        self.addCleanup(self.work.cleanup)
        self.root = Path(self.work.name).resolve()
        self.write("yamaa-project.yaml", "version: '1.0'\n")
        self.write("producer/layers/base.yaml", PRODUCER)
        self.write(
            "producer/p.yaml",
            "schema_version: '1.0'\nparents: [layers/base.yaml]\ndomain: PRODUCER\n",
        )
        self.write("consumer/root.yaml", CONSUMER)
        self.write("raw/raw.csv", RAW)
        self.write(
            "env/environment.yaml",
            "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {id_float: float.yaml}\n",
        )
        self.write("env/float.yaml", FUNCTION)
        self.write(
            "env/uv.lock", (ROOT / "schema-functions/python/uv.lock").read_bytes()
        )

    def write(self, name, value):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(value.encode() if isinstance(value, str) else value)

    def prepare(self):
        prepared = native._prepare_producer_file(
            (self.root / "consumer/root.yaml").as_posix(),
            str(self.root / "env/environment.yaml"),
        )
        self.assertEqual(prepared.preparation_status(), "ready")
        return prepared

    def report(self, attempt):
        result = attempt.result()
        self.assertEqual(result.report_status(), "complete")
        return result, json.loads(result.observations())

    def test_rounded_aliases_repeat_real_union_and_keep_complete_report_after_owner_deletion(
        self,
    ):
        prepared = self.prepare()
        self.write("raw/raw.csv", b"ID,VALUE\n1,99.99\n")
        events = []
        original = program.project_value
        verify = host.verify_versions
        resolve = host.resolve_callable

        def versions(lock, calls):
            events.append(("lock", tuple(calls)))
            self.assertEqual(lock, (self.root / "env/uv.lock").read_bytes())
            return verify(lock, calls)

        def binding(call, params):
            events.append(("bind", call))
            return resolve(call, params)

        def value(x):
            events.append(("invoke", x))
            if x == 0.0:
                self.write("raw/raw.csv", RAW)
            return original(x)

        with (
            patch.object(host, "verify_versions", versions),
            patch.object(host, "resolve_callable", binding),
            patch.object(program, "project_value", value),
        ):
            attempts = [prepared.build(METADATA), prepared.build(METADATA)]
        self.assertEqual(
            events,
            [
                ("lock", ("yamaa_benchmarks.project_value",)),
                ("bind", "yamaa_benchmarks.project_value"),
                ("invoke", 7.25),
                ("invoke", 0.0),
                ("invoke", 1.234),
                ("invoke", 2.345),
            ]
            * 2,
        )
        self.assertFalse((self.root / "generated").exists())
        del prepared
        shutil.rmtree(self.root)
        for attempt in attempts:
            self.assertTrue(attempt.accepted())
            self.assertEqual(attempt.entered_nodes(), [1, 0])
            self.assertEqual(attempt.artifact(1), PRODUCED)
            self.assertEqual(attempt.artifact(0), CONSUMED)
            sources = dict(attempt.retained_sources())
            self.assertEqual(
                next(
                    value
                    for name, value in sources.items()
                    if name.endswith("/raw/raw.csv")
                ),
                RAW,
            )
            with (
                patch.object(
                    host,
                    "verify_versions",
                    side_effect=AssertionError("report activation"),
                ),
                patch.object(
                    host,
                    "resolve_callable",
                    side_effect=AssertionError("report binding"),
                ),
                patch.object(
                    program,
                    "project_value",
                    side_effect=AssertionError("report replay"),
                ),
            ):
                result, observed = self.report(attempt)
                self.assertEqual(self.report(attempt)[1], observed)
            self.assertEqual(observed["outcome"], "success")
            self.assertEqual(
                [artifact["content"].encode() for artifact in observed["artifacts"]],
                [PRODUCED, CONSUMED],
            )
            self.assertEqual(
                [case["invoked"] for case in observed["activation"]["tests"]],
                [True, True, False],
            )
            tables = observed["tables"]
            self.assertEqual(
                [table["name"] for table in tables],
                ["RAW", "output", "FIRST", "SECOND", "output"],
            )
            self.assertEqual(
                [row[1]["value"] for row in tables[1]["rows"]],
                ["3ff3be76c8b43958", "4002c28f5c28f5c3"],
            )
            for table in tables[2:4]:
                self.assertEqual(
                    [row[1]["value"] for row in table["rows"]],
                    ["3ff3ae147ae147ae", "4002cccccccccccd"],
                )
            self.assertEqual(
                [row[3]["value"] for row in tables[4]["rows"]],
                ["4003ae147ae147ae", "4012cccccccccccd"],
            )
            self.assertFalse(hasattr(result, "save"))
            self.assertFalse(hasattr(attempt, "save"))
            self.assertTrue(result.attempt().accepted())

    def test_original_derivation_error_and_interrupt_identity_survive_reports_and_deletion(
        self,
    ):
        prepared = self.prepare()
        attempts = []
        for failure in (
            ValueError('actual "host" cause'),
            KeyboardInterrupt("original interrupt"),
        ):
            original = program.project_value
            value = raising_value(failure, original, 1.234)
            with patch.object(program, "project_value", value):
                attempts.append((failure, prepared.build(METADATA)))
        del prepared
        shutil.rmtree(self.root)
        while attempts:
            failure, attempt = attempts.pop(0)
            with self.subTest(type=type(failure)):
                self.assertFalse(attempt.accepted())
                self.assertEqual(attempt.entered_nodes(), [1])
                self.assertEqual(attempt.host_failures(), [("derivation", failure)])
                self.assertIs(attempt.host_failures()[0][1], failure)
                result = attempt.result()
                observed = json.loads(result.observations())
                self.assertEqual(observed["outcome"], "failure")
                self.assertEqual(observed["artifacts"], [])
                if isinstance(failure, Exception):
                    self.assertEqual(result.report_status(), "complete")
                    context = observed["diagnostics"][0]["context"]
                    self.assertEqual(
                        context["source"], (self.root / "producer/p.yaml").as_posix()
                    )
                    self.assertEqual(
                        context["entry"], (self.root / "consumer/root.yaml").as_posix()
                    )
                    self.assertEqual(
                        context["declaring_sources"],
                        [(self.root / "producer/layers/base.yaml").as_posix()],
                    )
                    self.assertEqual(
                        context["host_message"],
                        str(failure),
                    )
                else:
                    self.assertEqual(result.report_status(), "original_boundary")
                    with self.assertRaises(KeyboardInterrupt) as caught:
                        result.attempt().propagate_interrupt()
                    self.assertIs(caught.exception, failure)
                del attempt
                gc.collect()
                self.assertIs(result.attempt().host_failures()[0][1], failure)

    def test_binding_and_case_failure_keep_external_authored_origins_before_missing_data(
        self,
    ):
        (self.root / "raw/raw.csv").unlink()
        for stage in ("binding", "conformance"):
            failure = ValueError("original " + stage)
            prepared = self.prepare()
            original = program.project_value

            value = raising_value(failure, original, 7.25)

            with (
                patch.object(host, "resolve_callable", side_effect=failure)
                if stage == "binding"
                else patch.object(program, "project_value", value)
            ):
                attempt = prepared.build(METADATA)
            result, observed = self.report(attempt)
            self.assertEqual(attempt.entered_nodes(), [])
            self.assertEqual(observed["source_reads"], [])
            issue = observed["diagnostics"][0]
            self.assertTrue(issue["context"]["source"].endswith("/env/float.yaml"))
            self.assertTrue(issue["context"]["entry"].endswith("/env/environment.yaml"))
            self.assertEqual(
                issue["spec_paths"], ["function" if stage == "binding" else "tests[0]"]
            )
            self.assertIs(result.attempt().host_failures()[0][1], failure)

    def test_native_report_gate_and_whole_reporting_limits_keep_original_attempt(self):
        prepared = self.prepare()
        refused = prepared.build(METADATA, report_bytes=1)
        self.assertFalse(refused.accepted())
        self.assertEqual(refused.entered_nodes(), [1])
        self.assertIsNone(refused.artifact(1))
        self.assertIsNone(refused.artifact(0))
        self.assertEqual(
            json.loads(refused.result().observations())["error"]["code"], "output_limit"
        )
        complete = prepared.build(METADATA)
        bounded = complete.result(maximum=1)
        self.assertEqual(bounded.report_status(), "report_limit")
        self.assertTrue(bounded.attempt().accepted())
        self.assertEqual(bounded.attempt().artifact(0), CONSUMED)
        self.assertFalse(hasattr(bounded, "save"))

    def test_invalid_consumer_only_binding_admits_the_whole_union_before_any_study_effect(
        self,
    ):
        self.write(
            "env/environment.yaml",
            "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {id_float: float.yaml, consumer_float: consumer.yaml}\n",
        )
        self.write(
            "env/consumer.yaml",
            FUNCTION.replace(
                "yamaa_benchmarks.project_value", "yamaa_benchmarks.absent_function"
            ),
        )
        text = CONSUMER.replace(
            "  - {name: VALUE,",
            "  - {name: SUM_VALUE, type: float, label: Sum, derivation: {compute: {expr: 'FIRST_VALUE + SECOND_VALUE'}}}\n  - {name: VALUE,",
        )
        self.write(
            "consumer/root.yaml",
            text.rsplit("{compute: {expr: 'FIRST_VALUE + SECOND_VALUE'}}", 1)[0]
            + "{function: {name: consumer_float, args: {x: SUM_VALUE}}}"
            + text.rsplit("{compute: {expr: 'FIRST_VALUE + SECOND_VALUE'}}", 1)[1],
        )
        (self.root / "raw/raw.csv").unlink()
        prepared = self.prepare()
        calls = []
        original_verify = host.verify_versions

        def verify(lock, selected):
            calls.append(tuple(selected))
            return original_verify(lock, selected)

        def forbidden(x):
            raise AssertionError("data or conformance invocation before full binding")

        with (
            patch.object(host, "verify_versions", verify),
            patch.object(program, "project_value", forbidden),
        ):
            attempt = prepared.build(METADATA)
        result, observed = self.report(attempt)
        self.assertEqual(
            set(calls[0]),
            {"yamaa_benchmarks.project_value", "yamaa_benchmarks.absent_function"},
        )
        self.assertEqual(attempt.entered_nodes(), [])
        self.assertEqual(observed["source_reads"], [])
        self.assertEqual(observed["activation"]["tests"], [])
        self.assertEqual(len(observed["activation"]["bindings"]), 2)
        self.assertEqual(
            observed["diagnostics"][0]["context"]["function"], "consumer_float"
        )
        self.assertIsInstance(result.attempt().host_failures()[0][1], AttributeError)

    def test_diamond_executes_shared_producer_once_through_actual_installed_identity(
        self,
    ):
        for branch in ("left", "right"):
            self.write(
                f"{branch}.yaml",
                "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {LEAF: {path: generated/producer.csv, schema: producer/p.yaml}}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: LEAF.ID}\n  - {name: VALUE, type: float, label: Value, derivation: LEAF.VALUE}\n"
                + f"output: {{path: generated/{branch}.csv, columns: [ID, VALUE], decimals: 2}}\n",
            )
        self.write(
            "consumer/root.yaml",
            CONSUMER.replace(
                "../generated/producer.csv, schema: ../producer/p.yaml",
                "../generated/left.csv, schema: ../left.yaml",
            ).replace(
                "../generated/producer.csv, schema: ../producer/./p.yaml",
                "../generated/right.csv, schema: ../right.yaml",
            ),
        )
        calls = []
        original = program.project_value

        def value(x):
            calls.append(x)
            return original(x)

        with patch.object(program, "project_value", value):
            prepared = self.prepare()
            attempt = prepared.build(METADATA)
        _result, observed = self.report(attempt)
        self.assertTrue(attempt.accepted())
        self.assertEqual(attempt.entered_nodes(), [3, 1, 2, 0])
        self.assertEqual(calls, [7.25, 0.0, 1.234, 2.345])
        self.assertEqual(
            [artifact["content"].encode() for artifact in observed["artifacts"]],
            [PRODUCED, PRODUCED, PRODUCED, CONSUMED],
        )
        self.assertFalse((self.root / "generated").exists())

    def test_rendering_refusal_and_reentry_keep_original_failure_and_held_derived_origin(
        self,
    ):
        class Reentrant(ValueError):
            attempt = None

            def __str__(self):
                self.attempt.accepted()
                return "unreachable"

        failure = Reentrant("original")
        prepared = self.prepare()
        original = program.project_value

        def value(x):
            if x == 1.234:
                raise failure
            return original(x)

        with patch.object(program, "project_value", value):
            attempt = prepared.build(METADATA)
        failure.attempt = attempt
        result = attempt.result()
        self.assertEqual(result.report_status(), "original_boundary")
        self.assertIs(result.attempt().host_failures()[0][1], failure)
        failure.attempt = None
        oversized = ValueError("x" * 65537)
        with patch.object(
            program,
            "project_value",
            lambda x: (_ for _ in ()).throw(oversized) if x == 1.234 else original(x),
        ):
            other = prepared.build(METADATA)
        bounded = other.result()
        self.assertEqual(bounded.report_status(), "report_limit")
        self.assertIs(bounded.attempt().host_failures()[0][1], oversized)

    def test_early_raw_rejection_and_later_ingestion_failure_keep_original_bytes(self):
        self.write("env/environment.yaml", "schema_version: '1.0'\nlanguage: [\n")
        with patch.object(
            host, "verify_versions", side_effect=AssertionError("rejected activation")
        ):
            prepared = native._prepare_producer_file(
                (self.root / "consumer/root.yaml").as_posix(),
                str(self.root / "env/environment.yaml"),
            )
        self.assertEqual(prepared.preparation_status(), "environment_failure")
        sources = prepared.rejected_sources()
        self.assertEqual(len(sources), 2)
        self.assertEqual(
            next(
                bytes
                for name, bytes in sources
                if name.endswith("/env/environment.yaml")
            ),
            b"schema_version: '1.0'\nlanguage: [\n",
        )
        self.assertEqual(
            next(
                bytes for name, bytes in sources if name.endswith("/yamaa-project.yaml")
            ),
            b"version: '1.0'\n",
        )
        with self.assertRaises(ValueError):
            prepared.build(METADATA)
        self.write(
            "env/environment.yaml",
            "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {id_float: float.yaml}\n",
        )
        malformed = b"ID,VALUE\n1,not-a-float\n"
        self.write("raw/raw.csv", malformed)
        prepared = self.prepare()
        attempt = prepared.build(METADATA)
        result, observed = self.report(attempt)
        self.assertFalse(attempt.accepted())
        self.assertEqual(attempt.entered_nodes(), [1])
        self.assertEqual(observed["artifacts"], [])
        self.assertTrue(observed["diagnostics"])
        self.assertEqual(
            next(
                bytes
                for name, bytes in result.attempt().retained_sources()
                if name.endswith("/raw/raw.csv")
            ),
            malformed,
        )

    def test_consumer_verification_failure_keeps_prior_artifact_as_evidence_without_save(
        self,
    ):
        self.write(
            "consumer/root.yaml",
            CONSUMER + "verifications: [{assert: {require: 'VALUE < 0'}}]\n",
        )
        prepared = self.prepare()
        attempt = prepared.build(METADATA)
        self.assertFalse(attempt.accepted())
        self.assertEqual(attempt.entered_nodes(), [1, 0])
        self.assertEqual(attempt.artifact(1), PRODUCED)
        self.assertIsNone(attempt.artifact(0))
        result, observed = self.report(attempt)
        del prepared, attempt
        shutil.rmtree(self.root)
        self.assertEqual(observed["outcome"], "failure")
        self.assertEqual(
            [artifact["content"].encode() for artifact in observed["artifacts"]],
            [PRODUCED],
        )
        self.assertEqual(len(observed["nodes"]), 2)
        self.assertTrue(observed["diagnostics"])
        self.assertTrue(observed["verifications"])
        self.assertFalse(result.attempt().accepted())
        self.assertFalse(hasattr(result, "save"))

    def test_typed_graph_rejection_reports_the_actual_declaration_after_files_drop(
        self,
    ):
        self.write(
            "consumer/root.yaml",
            CONSUMER.replace(
                "../generated/producer.csv, schema: ../producer/p.yaml",
                "../generated/wrong.csv, schema: ../producer/p.yaml",
            ),
        )
        with patch.object(
            host,
            "verify_versions",
            side_effect=AssertionError("rejected graph activation"),
        ):
            prepared = native._prepare_producer_file(
                (self.root / "consumer/root.yaml").as_posix(),
                str(self.root / "env/environment.yaml"),
            )
        self.assertEqual(prepared.preparation_status(), "graph_failure")
        shutil.rmtree(self.root)
        issues = json.loads(prepared.rejection_issues())
        self.assertEqual([issue["requirement"] for issue in issues], ["REQ-0534"])
        self.assertEqual(
            issues[0]["spec_paths"], ["input.FIRST.path", "input.FIRST.schema"]
        )
        self.assertIn("wrong.csv", issues[0]["context"])
        self.assertFalse(
            any(name.endswith(".csv") for name, _ in prepared.rejected_sources())
        )
        with self.assertRaises(ValueError):
            prepared.rejection_issues(maximum=1)
        self.assertEqual(json.loads(prepared.rejection_issues()), issues)


if __name__ == "__main__":
    unittest.main()
