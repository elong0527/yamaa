"""Installed original-YAML execution with reference semantic imports forbidden."""

import builtins
import csv
import gc
import json
import os
import platform
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import yamaa_native

ROOT = Path(__file__).with_name("specification-original")
CASES = ("negative-zero-division", "negative-integer-overflow", "adam-adlb-ordered-sum", "schema-window-functions", "schema-inheritance", "schema-lookup")


def modules():
    names = ["schema.yaml"] + sorted(
        p.name for p in (ROOT / "schema").glob("*.yaml") if p.name != "schema.yaml"
    )
    return [(name, (ROOT / "schema" / name).read_bytes()) for name in names]


def specification_name(name):
    return "spec_study.yaml" if name == "schema-inheritance" else "spec.yaml"


def inheritance_callbacks(case):
    state = {"reads": [], "resolutions": []}

    def canonicalize(declaring, written):
        state["resolutions"].append(written)
        candidate = Path(declaring).parent / written
        if not candidate.is_file():
            return None
        return str(candidate.resolve()), str(candidate)

    def capture(identity, display_path, maximum):
        state["reads"].append(Path(identity).name)
        with Path(identity).open("rb") as stream:
            content = stream.read(maximum + 1)
        assert len(content) <= maximum
        return content

    def rebase(layer, entry, written, maximum):
        if Path(written).is_absolute() or Path(layer).parent == Path(entry).parent:
            return written
        target = os.path.normpath(os.path.join(Path(layer).parent, written))
        return Path(os.path.relpath(target, Path(entry).parent)).as_posix()

    return [canonicalize, capture, rebase], state


def prepare(name):
    case = ROOT / "cases" / name
    path = case / specification_name(name)
    callbacks, state = inheritance_callbacks(case)
    prepared = yamaa_native._prepare_document(
        str(path.resolve()), path.read_bytes(), *callbacks
    )
    if name == "schema-inheritance":
        assert state["reads"] == ["spec_organization.yaml", "spec_compound.yaml"]
        assert state["resolutions"] == ["spec_organization.yaml", "spec_compound.yaml", "spec_organization.yaml"]
    else:
        assert state["reads"] == state["resolutions"] == []
    return prepared


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

    def test_shipped_schema_ignores_ambient_files_and_rejects_versions_before_ports(self):
        path = ROOT / "cases/adam-adlb-ordered-sum/spec.yaml"
        raw = path.read_bytes()
        def no_parent(*_):
            self.fail("standalone or invalid version reached parent authority")
        previous = Path.cwd()
        with tempfile.TemporaryDirectory() as directory:
            try:
                os.chdir(directory)
                Path("schema.yaml").write_bytes(b"not: the shipped schema\n")
                prepared = yamaa_native._prepare_document("spec.yaml", raw, no_parent, no_parent, no_parent)
                self.assertEqual(prepared.source(), ("LB", "input/lb.csv"))
                wrong = raw.replace(b'"1.0"', b'"99.0"', 1)
                self.assertNotEqual(wrong, raw)
                with self.assertRaises(ValueError) as caught:
                    yamaa_native._prepare_document("spec.yaml", wrong, no_parent, no_parent, no_parent)
                self.assertEqual(json.loads(str(caught.exception))["outcome"], {
                    "status": "invalid", "diagnostics": [{
                        "phase": "validation", "condition": "schema_version_mismatch",
                        "requirement": None, "spec_paths": ["schema_version"],
                        "context": {"expected": "1.0", "actual": "99.0"},
                    }],
                })
            finally:
                os.chdir(previous)

    def test_original_source_failures_match_independent_decoder_truth(self):
        def no_port(*_):
            self.fail("source decoding failure reached a host port")
        with (ROOT / "decode-replay.tsv").open(encoding="ascii") as stream:
            records = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(len(records), 8)
        for record in records:
            with self.subTest(case=record["id"]):
                with self.assertRaises(ValueError) as caught:
                    yamaa_native._prepare_document("source.yaml", bytes.fromhex(record["source_hex"]), no_port, no_port, no_port)
                self.assertEqual(json.loads(str(caught.exception)), json.loads(record["expected"]))

    def test_schema_context_preserves_exact_integers_and_constraint_values(self):
        def no_port(*_):
            self.fail("invalid entry reached parent or study authority")
        for literal, expected in [("true", True), ("123456789012345678901234567890", 123456789012345678901234567890), ("null", None)]:
            with self.subTest(literal=literal):
                with self.assertRaises(ValueError) as caught:
                    yamaa_native._prepare_document("spec.yaml", ("schema_version: " + literal).encode(), no_port, no_port, no_port)
                self.assertEqual(json.loads(str(caught.exception))["outcome"], {
                    "status": "invalid", "diagnostics": [{"phase": "validation", "condition": "schema_version_mismatch", "requirement": None, "spec_paths": ["schema_version"], "context": {"expected": "1.0", "actual": expected}}],
                })
        path = ROOT / "cases/adam-adlb-ordered-sum/spec.yaml"
        text = path.read_bytes().replace(b"domain: ADLB", b"domain: bad-name", 1)
        self.assertNotEqual(text, path.read_bytes())
        with self.assertRaises(ValueError) as caught:
            yamaa_native._prepare_document("spec.yaml", text, no_port, no_port, no_port)
        self.assertEqual(json.loads(str(caught.exception))["outcome"], {
            "status": "invalid", "diagnostics": [{"phase": "validation", "condition": "pattern_mismatch", "requirement": "REQ-0287", "spec_paths": ["domain"], "context": {"value": "bad-name", "pattern": "^[A-Za-z_][A-Za-z0-9_]*$"}}],
        })

    def test_owned_result_retains_projected_output_and_save_never_reexecutes(self):
        for name in CASES:
            with self.subTest(name=name):
                specification = prepare(name)
                requests = []
                sealed = False
                def capture(dataset, path, maximum):
                    self.assertFalse(sealed, "save recaptured study data")
                    requests.append(path)
                    content = (ROOT / "cases" / name / path).read_bytes()
                    self.assertLessEqual(len(content), maximum)
                    return content, True
                metadata = ("fixture-runtime", "fixture-engine", name, specification_name(name), ".")
                result = specification.build(capture, metadata)
                sealed = True
                del specification
                gc.collect()
                expected = json.loads((ROOT / "expected" / (name + ".json")).read_text())
                unsaved = dict(expected, artifacts=[])
                self.assertEqual(json.loads(result.observations()), unsaved)
                if expected["outcome"] != "success":
                    self.assertIsNone(result.output())
                    with self.assertRaisesRegex(ValueError, "cannot save a failed build"):
                        result.save(lambda *_: self.fail("failed result reached publisher"))
                    continue
                artifact = expected["artifacts"][0]
                derived = [t for t in expected["tables"] if t["stage"] == "derived"][0]
                projection = [derived["columns"].index(c) for c in artifact["columns"]]
                snapshot = json.loads(yamaa_native.table_snapshot(result.output()))
                self.assertEqual(snapshot["columns"], list(map(list, zip(artifact["columns"], artifact["types"]))))
                self.assertEqual(snapshot["row_count"], str(artifact["row_count"]))
                def scalar(v):
                    kind, value = v["type"], v["value"]
                    if kind in ("date", "datetime"):
                        return {kind: {"text": value, "precision": "day" if kind == "date" else "second"}}
                    return {kind: value}
                self.assertEqual(snapshot["rows"], [[scalar(row[c]) for c in projection] for row in derived["rows"]])
                captured = list(requests)
                for failure in (OSError("save failed"), KeyboardInterrupt("save interrupted")):
                    calls = []
                    def fail(*args):
                        calls.append(args)
                        raise failure
                    with self.assertRaises(type(failure)) as caught:
                        result.save(fail)
                    self.assertIs(caught.exception, failure)
                    self.assertEqual(len(calls), 1)
                    self.assertEqual(json.loads(result.observations()), unsaved)
                with tempfile.TemporaryDirectory() as directory:
                    published = []
                    def publish(path, content):
                        expected_bytes = (ROOT / "cases" / name / "expected" / path).read_bytes()
                        self.assertEqual(content, expected_bytes)
                        pending = Path(directory) / "pending"
                        pending.write_bytes(content)
                        os.replace(pending, Path(directory) / path)
                        published.append((Path(directory) / path).read_bytes())
                    for _ in range(2):
                        self.assertEqual(json.loads(result.save(publish)), expected)
                    self.assertEqual(len(published), 2)
                self.assertEqual(requests, captured)

    def test_build_capture_preserves_original_host_failure(self):
        spec = prepare("schema-lookup")
        metadata = ("fixture-runtime", "fixture-engine", "schema-lookup", "spec.yaml", ".")
        for failure in (OSError("capture failed"), KeyboardInterrupt("capture interrupted"), SystemExit("capture stopped")):
            calls = []
            def capture(*args):
                calls.append(args)
                raise failure
            with self.assertRaises(type(failure)) as caught:
                spec.build(capture, metadata)
            self.assertIs(caught.exception, failure)
            self.assertEqual(len(calls), 1)

    def test_output_rejection_discards_table_and_blocks_save(self):
        name = "adam-adlb-ordered-sum"
        source = (ROOT / "cases" / name / "spec.yaml").read_bytes()
        source = source.replace(b"columns: [STUDYID,", b"columns: [STUDYID, STUDYID,", 1)
        spec = yamaa_native._prepare_document("spec.yaml", source, lambda *_: None, lambda *_: None, lambda *_: "")
        result = spec.build(
            lambda _, path, maximum: ((ROOT / "cases" / name / path).read_bytes(), True),
            ("fixture-runtime", "fixture-engine", name, "spec.yaml", "."),
        )
        self.assertIsNone(result.output())
        self.assertEqual(json.loads(result.observations())["diagnostics"], [{
            "phase": "validation", "condition": "duplicate_identifier", "requirement": "REQ-0234",
            "spec_paths": ["output.columns[1]"], "context": {"column": "STUDYID"},
        }])
        with self.assertRaisesRegex(ValueError, "cannot save a failed build"):
            result.save(lambda *_: self.fail("rejected output reached publication"))

    def test_inherited_raw_loader_replays_existing_complete_failure_contracts(self):
        with (ROOT / "inheritance-replay.tsv").open(newline="", encoding="ascii") as stream:
            records = list(csv.DictReader(stream, delimiter="\t"))
        names = list(dict.fromkeys(record["case"] for record in records))
        self.assertEqual(len(names), 7)
        for name in names:
            with self.subTest(case=name):
                case = [row for row in records if row["case"] == name]
                expected = [row for row in case if row["operation"]]
                calls = []

                def event(operation):
                    self.assertLess(len(calls), len(expected))
                    row = expected[len(calls)]
                    self.assertEqual(operation, row["operation"])
                    calls.append(row)
                    return row

                def canonicalize(declaring, written):
                    row = event("canonicalize")
                    self.assertEqual((declaring, written), (row["declaring"], row["written"]))
                    return (row["identity"], row["display_path"]) if row["identity"] else None

                def capture(identity, display_path, maximum):
                    row = event("read")
                    self.assertEqual((identity, display_path), (row["identity"], row["display_path"]))
                    content = row["source_yaml"].encode()
                    self.assertLessEqual(len(content), maximum)
                    return content or None

                with self.assertRaises(ValueError) as caught:
                    yamaa_native._prepare_document(
                        case[0]["entry"], case[0]["entry_yaml"].encode(),
                        canonicalize, capture, lambda *_: self.fail("failed traversal reached rebasing")
                    )
                self.assertEqual(json.loads(str(caught.exception)), json.loads(case[0]["expected"]))
                self.assertEqual(calls, expected)

    def test_inherited_callbacks_preserve_original_failures(self):
        case = ROOT / "cases/schema-inheritance"
        path = case / "spec_study.yaml"
        for operation in range(3):
            for kind in (RuntimeError, KeyboardInterrupt, SystemExit):
                with self.subTest(operation=operation, kind=kind):
                    callbacks, _ = inheritance_callbacks(case)
                    failure = kind("original inherited callback failure")
                    failure.marker = object()
                    calls = []

                    def fail(*args):
                        calls.append(args)
                        raise failure

                    callbacks[operation] = fail
                    with self.assertRaises(kind) as caught:
                        yamaa_native._prepare_document(
                            str(path.resolve()), path.read_bytes(), *callbacks
                        )
                    self.assertIs(caught.exception, failure)
                    self.assertEqual(len(calls), 1)

    def test_inherited_missing_parent_and_cycle_keep_shared_diagnostics(self):
        case = ROOT / "cases/schema-inheritance"
        path = case / "spec_study.yaml"
        identity = str(path.resolve())
        for cycle, condition, requirement in [(False, "parent_not_found", "REQ-0654"), (True, "inheritance_cycle", "REQ-0655")]:
            callbacks, state = inheritance_callbacks(case)
            callbacks[0] = lambda *_: (identity, identity) if cycle else None
            with self.assertRaises(ValueError) as caught:
                yamaa_native._prepare_inherited_specification(modules(), 0, identity, path.read_bytes(), *callbacks)
            outcome = json.loads(str(caught.exception))["outcome"]
            self.assertEqual(outcome["status"], "invalid")
            self.assertEqual(outcome["diagnostics"][0]["condition"], condition)
            self.assertEqual(outcome["diagnostics"][0]["requirement"], requirement)
            self.assertEqual(state["reads"], [])

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
                    specification_name(name),
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
                        self.assertIn(name, ("adam-adlb-ordered-sum", "schema-window-functions", "schema-inheritance", "schema-lookup"))
                        self.assertEqual(path, {"schema-lookup":"adsl.csv", "schema-window-functions":"advs.csv", "adam-adlb-ordered-sum":"adlb.csv", "schema-inheritance":"adlb.csv"}[name])
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
                    len(published), 2 if name in ("adam-adlb-ordered-sum", "schema-window-functions", "schema-inheritance", "schema-lookup") else 0
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
