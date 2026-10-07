"""Installed original-YAML execution with reference semantic imports forbidden."""

import builtins
import json
import os
import platform
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import yamaa_native

ROOT = Path(__file__).with_name("specification-original")
CASES = ("negative-zero-division", "negative-integer-overflow", "adam-adlb-ordered-sum", "schema-window-functions", "schema-lookup")


def modules():
    names = ["schema.yaml"] + sorted(
        p.name for p in (ROOT / "schema").glob("*.yaml") if p.name != "schema.yaml"
    )
    return [(name, (ROOT / "schema" / name).read_bytes()) for name in names]


def prepare(name):
    return yamaa_native._prepare_specification(
        modules(), 0, "spec.yaml", (ROOT / "cases" / name / "spec.yaml").read_bytes()
    )


class OriginalSpecifications(unittest.TestCase):
    def setUp(self):
        original = builtins.__import__

        def reject(name, *args, **kwargs):
            if name.split(".")[0] in {"yamaa", "pydantic", "yaml", "yaml12", "pyarrow"}:
                raise AssertionError("reference semantic import: " + name)
            return original(name, *args, **kwargs)

        guard = patch("builtins.__import__", side_effect=reject)
        guard.start()
        self.addCleanup(guard.stop)

    def test_complete_original_reports_and_cached_capture(self):
        for name in CASES:
            with self.subTest(name=name):
                specification = prepare(name)
                sources = {"DM":"input/dm.csv", "AE":"input/ae.csv", "MEDDRA":"input/meddict.csv"} if name == "schema-lookup" else ({"VS":"input/vs.csv"} if name == "schema-window-functions" else {"LB":"input/lb.csv"})
                self.assertEqual(specification.source(), next(iter(sources.items())))
                state = {"requests": [], "reads": 0, "bytes": {}}

                def capture(dataset, path, maximum, *, state=state, case_name=name):
                    self.assertEqual(sources[dataset], path)
                    state["requests"].append((dataset,path))
                    created = path not in state["bytes"]
                    if created:
                        with (ROOT / "cases" / case_name / path).open("rb") as stream:
                            state["bytes"][path] = stream.read(maximum + 1)
                        self.assertLessEqual(len(state["bytes"][path]), maximum)
                        state["reads"] += 1
                    return state["bytes"][path], created

                metadata = (
                    platform.python_version(),
                    yamaa_native.engine_info()["core_version"],
                    name,
                    "spec.yaml",
                    ".",
                )
                expected = json.loads(
                    (ROOT / "expected" / (name + ".json")).read_text()
                )
                expected.update(
                    runtime="python",
                    runtime_version=metadata[0],
                    engine_version=metadata[1],
                )
                published = []
                with tempfile.TemporaryDirectory() as directory:

                    def publish(path, content, *, name=name, published=published):
                        self.assertIn(name, ("adam-adlb-ordered-sum", "schema-window-functions", "schema-lookup"))
                        self.assertEqual(path, {"schema-lookup":"adsl.csv", "schema-window-functions":"advs.csv", "adam-adlb-ordered-sum":"adlb.csv"}[name])
                        self.assertEqual(
                            content,
                            (ROOT / "cases" / name / "expected" / path).read_bytes(),
                        )
                        pending = Path(directory) / "candidate.csv"
                        pending.write_bytes(content)
                        os.replace(pending, Path(directory) / path)
                        published.append((Path(directory) / path).read_bytes())

                    for created in (1, 0):
                        for source in expected["source_reads"]:
                            source["snapshots_created"] = created
                        actual = json.loads(
                            specification.report(capture, publish, metadata)
                        )
                        self.assertEqual(actual, expected)
                self.assertEqual(
                    len(published), 2 if name in ("adam-adlb-ordered-sum", "schema-window-functions", "schema-lookup") else 0
                )
                self.assertEqual(state["requests"], list(sources.items()) * 2)
                self.assertEqual(state["reads"], len(sources))

    def test_lookup_failures_retain_complete_reference_observations(self):
        name = "schema-lookup"
        original = (ROOT / "cases" / name / "spec.yaml").read_bytes()
        cases = json.loads((ROOT / "expected/lookup-failures.json").read_text())
        for variant in cases:
            with self.subTest(variant=variant["name"]):
                raw = original.replace(variant["before"].encode(), variant["after"].encode())
                spec = yamaa_native._prepare_specification(modules(), 0, "spec.yaml", raw)
                requests = []
                def capture(dataset, path, maximum):
                    requests.append(dataset)
                    return (ROOT / "cases" / name / path).read_bytes(), True
                def publish(*args):
                    self.fail("failed lookup published")
                metadata = ("fixture-runtime", "fixture-engine", name, "spec.yaml", ".")
                actual = json.loads(spec.report(capture, publish, metadata))
                expected = json.loads((ROOT / "expected" / (name + ".json")).read_text())
                expected["outcome"] = expected["nodes"][0]["outcome"] = "failure"
                for field in ("diagnostics", "handler_counts"):
                    expected[field] = expected["nodes"][0][field] = variant[field]
                expected["artifacts"] = expected["verifications"] = []
                expected["tables"] = [table for table in expected["tables"] if table["stage"] == "source"]
                self.assertEqual(actual, expected)
                self.assertEqual(requests, ["DM", "AE", "MEDDRA"])

    def test_window_failures_retain_complete_reference_observations(self):
        name = "schema-window-functions"
        original = (ROOT / "cases" / name / "spec.yaml").read_bytes()
        cases = json.loads((ROOT / "expected/window-failures.json").read_text())
        for variant in cases:
            with self.subTest(variant=variant["name"]):
                raw = original.replace(variant["before"].encode(), variant["after"].encode())
                spec = yamaa_native._prepare_specification(modules(), 0, "spec.yaml", raw)
                requests = []
                def capture(dataset, path, maximum):
                    requests.append(dataset)
                    content = (ROOT / "cases" / name / path).read_bytes()
                    if "input_before" in variant:
                        content = content.replace(variant["input_before"].encode(), variant["input_after"].encode())
                    return content, True
                def publish(*args):
                    self.fail("failed window published")
                metadata = ("fixture-runtime", "fixture-engine", name, "spec.yaml", ".")
                actual = json.loads(spec.report(capture, publish, metadata))
                expected = json.loads((ROOT / "expected" / (name + ".json")).read_text())
                expected["outcome"] = expected["nodes"][0]["outcome"] = "failure"
                for field in ("diagnostics", "handler_counts"):
                    expected[field] = expected["nodes"][0][field] = variant[field]
                expected["artifacts"] = expected["verifications"] = []
                expected["tables"] = [table for table in expected["tables"] if table["stage"] == "source"]
                for row, column, value in variant.get("source_cells", []):
                    expected["tables"][0]["rows"][row][column] = value
                self.assertEqual(actual, expected)
                self.assertEqual(requests, ["VS"])

    def test_single_buffer_rejects_missing_secondary_sources_explicitly(self):
        spec = prepare("schema-lookup")
        source = (ROOT / "cases/schema-lookup/input/dm.csv").read_bytes()
        with self.assertRaises(ValueError) as raised:
            spec.execute_csv(source)
        self.assertEqual(json.loads(str(raised.exception)), {
            "protocol": "specification/prototype",
            "outcome": {"status": "rejected", "stage": "bind", "code": "source_count"},
        })


    def test_original_host_errors_and_interruptions_survive_native_return(self):
        specification = prepare(CASES[0])
        metadata = (
            platform.python_version(),
            yamaa_native.engine_info()["core_version"],
            CASES[0],
            "spec.yaml",
            ".",
        )
        for failure in (
            OSError("retained source failure"),
            KeyboardInterrupt("retained interrupt"),
        ):
            calls = []

            def capture(*args, calls=calls, failure=failure):
                calls.append(args)
                raise failure

            with self.assertRaises(type(failure)) as raised:
                specification.failure_report(capture, metadata)
            self.assertIs(raised.exception, failure)
            self.assertEqual(len(calls), 1)

    def test_publication_retains_original_errors_and_interruptions(self):
        name = "adam-adlb-ordered-sum"
        specification = prepare(name)
        metadata = (
            platform.python_version(),
            yamaa_native.engine_info()["core_version"],
            name,
            "spec.yaml",
            ".",
        )
        content = (ROOT / "cases" / name / "input/lb.csv").read_bytes()
        for failure in (
            OSError("retained publication failure"),
            KeyboardInterrupt("retained publication interrupt"),
        ):
            calls = []

            def publish(*args, calls=calls, failure=failure):
                calls.append(args)
                raise failure

            with self.assertRaises(type(failure)) as raised:
                specification.report(lambda *_: (content, True), publish, metadata)
            self.assertIs(raised.exception, failure)
            self.assertEqual(len(calls), 1)

    def test_lookup_collects_source_findings_and_never_publishes(self):
        name = "schema-lookup"
        raw = (ROOT / "cases" / name / "spec.yaml").read_bytes().replace(
            b"DM: input/dm.csv", b"DM: {path: input/dm.csv, types: {ABSENT: int}}"
        ).replace(b"AE: input/ae.csv", b"AE: {path: input/ae.csv, types: {AEDY: date}}")
        spec = yamaa_native._prepare_specification(modules(), 0, "spec.yaml", raw)
        metadata = ("fixture-runtime", "fixture-engine", name, "spec.yaml", ".")
        requests = []
        def capture(dataset, path, maximum):
            requests.append(dataset)
            return (ROOT / "cases" / name / path).read_bytes(), True
        def publish(*args):
            self.fail("ingestion failure reached publication")
        actual = json.loads(spec.report(capture, publish, metadata))
        expected = json.loads((ROOT / "expected" / (name + ".json")).read_text())
        expected["outcome"] = expected["nodes"][0]["outcome"] = "failure"
        expected["diagnostics"] = [
            {"phase":"validation", "condition":"unknown_field", "requirement":"REQ-0532", "spec_paths":["input.DM.types.ABSENT"], "context":{"dataset":"DM", "field":"ABSENT"}},
            {"phase":"ingest", "condition":"field_parse_failed", "requirement":"REQ-0536", "spec_paths":["input.AE.types.AEDY"], "context":{"dataset":"AE", "field":"AEDY", "type":"date", "value":"50"}},
        ]
        expected["nodes"][0]["diagnostics"] = expected["diagnostics"]
        for field in ("artifacts", "tables", "verifications", "handler_counts"):
            expected[field] = []
        expected["nodes"][0]["handler_counts"] = []
        self.assertEqual(actual, expected)
        self.assertEqual(requests, ["DM", "AE", "MEDDRA"])

    def test_lookup_retains_later_source_exception_identity(self):
        name = "schema-lookup"
        spec = prepare(name)
        metadata = ("fixture-runtime", "fixture-engine", name, "spec.yaml", ".")
        for failure in (OSError("second source failed"), KeyboardInterrupt("second source interrupted")):
            requests = []
            def capture(dataset, path, maximum):
                requests.append(dataset)
                if dataset == "AE":
                    raise failure
                return (ROOT / "cases" / name / path).read_bytes(), True
            with self.assertRaises(type(failure)) as raised:
                spec.failure_report(capture, metadata)
            self.assertIs(raised.exception, failure)
            self.assertEqual(requests, ["DM", "AE"])

    def test_unsupported_preparation_has_no_source_effect(self):
        source = (ROOT / "cases" / CASES[0] / "spec.yaml").read_bytes()
        source = source.replace(b"100 * (AVAL - BASE) / BASE", b"LN(AVAL)")
        with self.assertRaises(ValueError) as raised:
            yamaa_native._prepare_specification(modules(), 0, "spec.yaml", source)
        self.assertEqual(
            json.loads(str(raised.exception))["outcome"]["status"], "unsupported"
        )


if __name__ == "__main__":
    unittest.main()
