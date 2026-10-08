"""Installed original-YAML execution with reference semantic imports forbidden."""

import builtins
import csv
import gc
import importlib
import json
import os
import platform
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import yamaa_native

ROOT = Path(__file__).with_name("specification-original")
CASES = ("negative-zero-division", "negative-integer-overflow", "adam-adlb-ordered-sum", "schema-window-functions", "schema-inheritance", "schema-lookup", "negative-formula-flag", "negative-row-aggregate", "negative-row-no-prior", "negative-source-missing-field", "negative-source-trivial-filter", "negative-paired-dates", "negative-not-missing-age", "negative-implausible-age", "negative-invalid-sex", "negative-sex-code", "negative-matches-bad-pattern")


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

    @unittest.skipIf(os.name == "nt", "native file transport is qualified on Unix")
    def test_native_file_preparation_builds_all_original_reports_without_host_ports(self):
        outputs = {"schema-lookup": "adsl.csv", "schema-window-functions": "advs.csv", "adam-adlb-ordered-sum": "adlb.csv", "schema-inheritance": "adlb.csv"}
        for name in CASES:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                case = (ROOT / "cases" / name).resolve()
                entry = specification_name(name)
                specification = yamaa_native._prepare_file_specification(str(case), str(case), entry, [])
                before = 3 if name == "schema-inheritance" else 1
                self.assertEqual(specification.capture_reads(), before)
                self.assertEqual(json.loads(specification.check_issues()), json.loads(prepare(name).check_issues()))
                self.assertEqual(specification.capture_reads(), before)
                metadata = (platform.python_version(), yamaa_native.engine_info()["core_version"], name, entry, ".")
                expected = json.loads((ROOT / "expected" / (name + ".json")).read_text())
                expected.update(runtime="python", runtime_version=metadata[0], engine_version=metadata[1])
                captures = sum(source["snapshots_created"] or 0 for source in expected["source_reads"])
                for created in (1, 0):
                    for source in expected["source_reads"]:
                        if source["snapshots_created"] is not None:
                            source["snapshots_created"] = created
                    result = specification.build(metadata)
                    unsaved = dict(expected, artifacts=[])
                    self.assertEqual(json.loads(result.observations()), unsaved)
                    self.assertEqual(specification.capture_reads(), before + captures)
                    target = Path(directory) / outputs.get(name, "failed.csv")
                    if name.startswith("negative-"):
                        self.assertIsNone(result.output())
                        with self.assertRaisesRegex(ValueError, "cannot save a failed build"):
                            result.save_file("failed.csv", str(target))
                        with self.assertRaisesRegex(ValueError, "cannot save a failed build"):
                            result.save_file("failed.csv", str(Path(directory) / "absent" / "failed.csv"))
                        self.assertFalse(target.exists())
                    else:
                        artifact = expected["artifacts"][0]
                        derived = next(table for table in expected["tables"] if table["stage"] == "derived")
                        projection = [derived["columns"].index(column) for column in artifact["columns"]]
                        snapshot = json.loads(yamaa_native.table_snapshot(result.output()))
                        self.assertEqual(snapshot["columns"], list(map(list, zip(artifact["columns"], artifact["types"]))))
                        self.assertEqual(snapshot["row_count"], str(artifact["row_count"]))
                        def scalar(value):
                            kind, content = value["type"], value["value"]
                            if kind in ("date", "datetime"):
                                return {kind: {"text": content, "precision": "day" if kind == "date" else "second"}}
                            return {kind: content}
                        self.assertEqual(snapshot["rows"], [[scalar(row[column]) for column in projection] for row in derived["rows"]])
                        csv_expected = (case / "expected" / outputs[name]).read_bytes()
                        for _ in range(2):
                            self.assertEqual(json.loads(result.save_file(outputs[name], str(target))), expected)
                            self.assertEqual(target.read_bytes(), csv_expected)
                        self.assertEqual(specification.capture_reads(), before + captures)
                    self.assertEqual(list(Path(directory).glob(".yamaa-output-*")), [])

    @unittest.skipIf(os.name == "nt", "native file transport is qualified on Unix")
    def test_native_file_preparation_retains_complete_early_findings_and_path_authority(self):
        valid = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: {source: SRC.ID}}\n"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            entry = root / "spec.yaml"
            entry.write_text(valid.replace("'1.0'", "'99.0'") + "parents: absent.yaml\n")
            with self.assertRaises(ValueError) as caught:
                yamaa_native._prepare_file_specification(str(root), str(root), "spec.yaml", [])
            self.assertEqual(json.loads(str(caught.exception)), {"protocol": "specification/prototype", "outcome": {"status": "invalid", "diagnostics": [{"phase": "validation", "condition": "schema_version_mismatch", "requirement": "REQ-0245", "spec_paths": ["schema_version"], "context": {"expected": "1.0", "actual": "99.0", "source": str(entry), "entry": str(entry)}}]}})
            entry.write_text(valid + "parents: absent.yaml\n")
            with self.assertRaises(ValueError) as caught:
                yamaa_native._prepare_file_specification(str(root), str(root), "spec.yaml", [])
            self.assertEqual(json.loads(str(caught.exception)), {"protocol": "specification/prototype", "outcome": {"status": "invalid", "diagnostics": [{"phase": "validation", "condition": "parent_not_found", "requirement": "REQ-0654", "spec_paths": ["parents"], "context": {"path": "absent.yaml", "source": str(entry)}}]}})
            entry.write_text(valid)
            (root / "link.yaml").symlink_to(entry)
            with self.assertRaisesRegex(ValueError, "symbolic link"):
                yamaa_native._prepare_file_specification(str(root), str(root), "link.yaml", [])
            with self.assertRaises(ValueError):
                yamaa_native._prepare_file_specification(str(root), str(root), "spec.yaml", [str(root)] * 64)
            with self.assertRaises(TypeError):
                yamaa_native._prepare_file_specification(str(root), str(root), "spec.yaml", [object()])

    @unittest.skipIf(os.name == "nt", "native file transport is qualified on Unix")
    def test_native_file_preparation_cross_directory_parent_rebases_and_holds_model(self):
        valid = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: {source: SRC.ID}}\n"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            (root / "entry").mkdir(); (root / "parent").mkdir()
            (root / "parent/base.yaml").write_text(valid)
            (root / "entry/spec.yaml").write_text("schema_version: '1.0'\nparents: [../parent/base.yaml, ../parent/./base.yaml]\n")
            specification = yamaa_native._prepare_file_specification(str(root), str(root / "entry"), "spec.yaml", [])
            self.assertEqual(specification.source(), ("SRC", "../parent/input.csv"))
            self.assertEqual(specification.capture_reads(), 2)
            self.assertEqual(specification.check_issues(), "[]")
            (root / "entry/spec.yaml").write_text("[changed")
            (root / "parent/base.yaml").write_text("[changed")
            (root / "parent/input.csv").write_bytes(b"ID\n1\n")
            result = specification.build(("fixture-runtime", "fixture-engine", "file-view", "spec.yaml", "."))
            held_output = result.output()
            snapshot = json.loads(yamaa_native.table_snapshot(held_output))
            self.assertEqual(snapshot["columns"], [["ID", "int"]])
            self.assertEqual(snapshot["row_count"], "1")
            self.assertEqual(snapshot["rows"], [[{"int": "1"}]])
            self.assertEqual(specification.capture_reads(), 3)
            (root / "parent/input.csv").write_bytes(b"ID\n2\n")
            with self.assertRaisesRegex(ValueError, "captured resource content changed"):
                specification.build(("fixture-runtime", "fixture-engine", "file-view", "spec.yaml", "."))
            self.assertEqual(result.output(), held_output)
            self.assertEqual(specification.capture_reads(), 3)

    def test_core_preflight_preserves_independent_findings_before_ports(self):
        def no_port(*_):
            self.fail("preflight failure reached a host port")
        with (ROOT / "preflight.tsv").open(encoding="ascii") as stream:
            records = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(len(records), 5)
        self.assertEqual(sum(len(json.loads(r["expected"])["outcome"]["diagnostics"]) for r in records), 16)
        for record in records:
            with self.subTest(case=record["case"]):
                with self.assertRaises(ValueError) as caught:
                    yamaa_native._prepare_document("spec.yaml", record["source"].encode("ascii"), no_port, no_port, no_port)
                self.assertEqual(json.loads(str(caught.exception)), json.loads(record["expected"]))

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

    def test_inherited_pass_failures_resolve_shared_context_and_provenance(self):
        with (ROOT / "inheritance-preparation.tsv").open(encoding="ascii") as stream:
            records = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(len(records), 3)
        for record in records:
            with self.subTest(case=record["case"]):
                counts = [0, 0, 0]
                def canonicalize(declaring, written):
                    self.assertEqual((declaring, written), ("entry.yaml", "parent.yaml"))
                    counts[0] += 1
                    return "parent.yaml", "parent.yaml"
                def capture(identity, display, maximum):
                    self.assertEqual((identity, display), ("parent.yaml", "parent.yaml"))
                    counts[1] += 1
                    raw = record["parent_yaml"].encode()
                    self.assertLessEqual(len(raw), maximum)
                    return raw
                def rebase(layer, entry, written, maximum):
                    counts[2] += 1
                    return written
                with self.assertRaises(ValueError) as caught:
                    yamaa_native._prepare_document("entry.yaml", record["entry_yaml"].encode(), canonicalize, capture, rebase)
                self.assertEqual(json.loads(str(caught.exception)), json.loads(record["expected"]))
                self.assertEqual(counts, [1, 1, int(record["rebases"])])

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

    def test_classified_capture_replies_retain_complete_failed_results(self):
        with (ROOT / "source-capture.tsv").open(encoding="ascii") as stream:
            records = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(len(records), 8)
        for record in records:
            with self.subTest(kind=record["kind"], at=record["fail_at"], cached=record["cached"]):
                spec = prepare("schema-lookup")
                metadata = ("fixture-runtime", "fixture-engine", "schema-lookup", "spec.yaml", ".")
                fail_at, cached = int(record["fail_at"]), bool(int(record["cached"]))
                failure = OSError("opaque host details must not enter findings")
                failure.payload = object()
                requests = []
                def capture(name, path, maximum):
                    index = len(requests)
                    requests.append((name, path))
                    if index == fail_at:
                        return record["kind"], failure
                    content = (ROOT / "cases/schema-lookup" / path).read_bytes()
                    self.assertLessEqual(len(content), maximum)
                    return content, not cached
                result = spec.build(capture, metadata)
                expected = json.loads(record["expected"])
                expected_requests = [("DM", "input/dm.csv"), ("AE", "input/ae.csv")][:fail_at + 1]
                self.assertEqual(requests, expected_requests)
                self.assertEqual(json.loads(result.observations()), expected)
                self.assertIsNone(result.output())
                requests.clear()
                self.assertEqual(json.loads(spec.failure_report(capture, metadata)), expected)
                self.assertEqual(requests, expected_requests)
                requests.clear()
                self.assertEqual(json.loads(spec.report(capture, lambda *_: self.fail("failed capture published"), metadata)), expected)
                self.assertEqual(requests, expected_requests)
                del spec, capture, failure
                gc.collect()
                for _ in range(2):
                    with self.assertRaisesRegex(ValueError, "cannot save a failed build"):
                        result.save(lambda *_: self.fail("failed result reached publisher"))
                self.assertEqual(json.loads(result.observations()), expected)

    def test_metadata_inspection_retains_complete_zero_read_failures(self):
        with (ROOT / "source-inspection.tsv").open(encoding="ascii") as stream:
            records = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(len(records), 7)
        metadata = ("fixture-runtime", "fixture-engine", "schema-lookup", "spec.yaml", ".")
        paths = {"DM": "input/dm.csv", "AE": "input/ae.csv", "MEDDRA": "input/meddict.csv"}
        for record in records:
            with self.subTest(case=record["case"]):
                spec = prepare("schema-lookup")
                failures = json.loads(record["failures"])
                failure = OSError("private filesystem payload")
                failure.payload = object()
                inspections = []
                def inspect(name, path):
                    self.assertEqual(path, paths[name])
                    inspections.append(name)
                    if name in failures:
                        return failures[name], failure
                    return None
                def capture(*_):
                    self.fail("metadata failure reached study capture")
                expected = json.loads(record["expected"])
                for operation in (
                    lambda: spec.build(capture, metadata, inspect=inspect),
                    lambda: spec.failure_report(capture, metadata, inspect=inspect),
                    lambda: spec.report(capture, lambda *_: self.fail("inspection failure published"), metadata, inspect=inspect),
                ):
                    inspections.clear()
                    result = operation()
                    actual = result.observations() if hasattr(result, "observations") else result
                    self.assertEqual(json.loads(actual), expected)
                    self.assertEqual(inspections, list(paths))
                    if hasattr(result, "output"):
                        self.assertIsNone(result.output())
                        with self.assertRaisesRegex(ValueError, "cannot save a failed build"):
                            result.save(lambda *_: self.fail("inspection failure saved"))
                        self.assertEqual(json.loads(result.observations()), expected)

    def test_inspection_stops_opaque_errors_and_preserves_interrupt_identity(self):
        spec = prepare("schema-lookup")
        metadata = ("fixture-runtime", "fixture-engine", "schema-lookup", "spec.yaml", ".")
        def capture(*_):
            self.fail("inspection error reached capture")
        for failure in (OSError("opaque inspection"), KeyboardInterrupt("inspection interrupt"), SystemExit("inspection exit")):
            for returned in (False, True):
                if returned and isinstance(failure, OSError):
                    continue  # Explicit known errors are classified; thrown errors are opaque.
                calls = []
                def inspect(name, path):
                    calls.append((name, path))
                    if name == "DM":
                        return "missing", OSError("earlier known cause")
                    if returned:
                        return "missing", failure
                    raise failure
                for operation in (
                    lambda: spec.build(capture, metadata, inspect=inspect),
                    lambda: spec.failure_report(capture, metadata, inspect=inspect),
                    lambda: spec.report(capture, lambda *_: self.fail("inspection error published"), metadata, inspect=inspect),
                ):
                    calls.clear()
                    with self.assertRaises(type(failure)) as caught:
                        operation()
                    self.assertIs(caught.exception, failure)
                    self.assertEqual(calls, [("DM", "input/dm.csv"), ("AE", "input/ae.csv")])
        for inspector in (False, object(), "missing"):
            with self.assertRaisesRegex(TypeError, "inspect must be callable"):
                spec.build(capture, metadata, inspect=inspector)
        for reply in (True, (b"bytes", False), ("unknown", OSError("opaque")), ("missing", object()), ("missing",)):
            calls = []
            def inspect(*args):
                calls.append(args)
                return reply
            with self.assertRaises((TypeError, ValueError)):
                spec.build(capture, metadata, inspect=inspect)
            self.assertEqual(len(calls), 1)

    def test_successful_inspection_precedes_capture_on_each_build(self):
        spec = prepare("schema-lookup")
        metadata = ("fixture-runtime", "fixture-engine", "schema-lookup", "spec.yaml", ".")
        expected = json.loads((ROOT / "expected/schema-lookup.json").read_text())
        expected.update(runtime_version="fixture-runtime", engine_version="fixture-engine", artifacts=[])
        seen = set()
        events = []
        def inspect(name, path):
            events.append(("inspect", name))
        def capture(name, path, maximum):
            events.append(("capture", name))
            content = (ROOT / "cases/schema-lookup" / path).read_bytes()
            self.assertLessEqual(len(content), maximum)
            created = name not in seen
            seen.add(name)
            return content, created
        for created in (1, 0):
            events.clear()
            result = spec.build(capture, metadata, inspect=inspect)
            self.assertEqual(events, [(kind, name) for kind in ("inspect", "capture") for name in ("DM", "AE", "MEDDRA")])
            for read in expected["source_reads"]:
                read["snapshots_created"] = created
            self.assertEqual(json.loads(result.observations()), expected)
            self.assertIsNotNone(result.output())

    def test_original_filtered_sources_complete_reports_and_exact_csv(self):
        self._source_selection_reports("source-filters.tsv", "source-filter", 13)

    def test_original_first_available_complete_reports_and_exact_csv(self):
        self._source_selection_reports("first-available.tsv", "first-available", 11)

    def test_original_all_or_none_complete_reports_and_exact_csv(self):
        self._source_selection_reports("original-all-or-none.tsv", "all-or-none", 11)

    def test_original_column_not_missing_complete_reports_and_exact_csv(self):
        self._source_selection_reports("original-not-missing.tsv", "column-check", 10)

    def test_row_filter_unsupported_admission_precedes_all_ports(self):
        def no_port(*_):
            self.fail("unsupported row filter reached a parent or study port")
        with (ROOT / "row-filter-admission.tsv").open(encoding="ascii") as stream:
            rows = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(len(rows), 2)
        for row in rows:
            with self.subTest(case=row["case"]), self.assertRaises(ValueError) as caught:
                yamaa_native._prepare_document("spec.yaml", bytes.fromhex(row["source_hex"]), no_port, no_port, no_port)
            self.assertEqual(json.loads(str(caught.exception)), json.loads(row["expected"]))

    def test_row_filters_complete_reports_and_exact_csv(self):
        self._source_selection_reports("row-filters.tsv", "row-filter", 27)

    def test_row_reductions_complete_reports_and_exact_csv(self):
        self._source_selection_reports("row-reductions.tsv", "row-reduction", 25)

    def test_row_column_checks_complete_reports_and_exact_csv(self):
        self._source_selection_reports("row-column-checks.tsv", "row-column", 48)

    def test_static_verification_check_issues_need_no_study_authority(self):
        with (ROOT / "static-verification-checks.tsv").open(encoding="utf-8") as stream:
            cases = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(len(cases), 15)
        def no_port(*args):
            raise AssertionError("static check entered a resource authority")
        for row in cases:
            with self.subTest(case=row["case"]):
                handle = yamaa_native._prepare_document("spec.yaml", bytes.fromhex(row["source_hex"]), no_port, no_port, no_port)
                for _ in range(2):
                    self.assertEqual(handle.check_issues(), row["expected"])
                    expected = json.loads(row["expected"])
                    self.assertEqual(handle.check_issue_rows(), [
                        (issue["phase"],issue["condition"],issue["requirement"],issue["spec_paths"],issue["context"])
                        for issue in expected
                    ])

    def test_owned_build_issue_rows_retain_complete_original_truth_without_more_reads(self):
        result_view = importlib.import_module("yamaa._native_results")
        for name in CASES:
            with self.subTest(case=name):
                handle = prepare(name)
                case = ROOT / "cases" / name
                state = {"reads": 0}
                def capture(dataset, path, maximum):
                    content = (case / path).read_bytes()
                    self.assertLessEqual(len(content), maximum)
                    state["reads"] += 1
                    return content, True
                result = handle.build(capture, (platform.python_version(),yamaa_native.engine_info()["core_version"],name,specification_name(name),"."))
                reads = state["reads"]
                diagnostics = json.loads((ROOT / "expected" / (name + ".json")).read_text())["diagnostics"]
                expected = [(d["phase"],d["condition"],d["requirement"],d["spec_paths"],json.dumps(d["context"],ensure_ascii=False,sort_keys=True,separators=(",", ":"))) for d in diagnostics]
                for _ in range(2):
                    self.assertEqual(result.issues(), expected)
                    frame = result_view.issues_frame(result.issues())
                    self.assertEqual(frame.schema, result_view.ISSUE_SCHEMA)
                    self.assertEqual(frame.rows(), expected)
                    self.assertEqual(frame.columns, ["phase","condition","requirement","spec_paths","context"])
                copy = result.issues()
                if copy:
                    copy[0][3].append("caller mutation")
                self.assertEqual(result.issues(), expected)
                self.assertEqual(state["reads"], reads)
                self.assertEqual(json.loads(result.observations())["diagnostics"], diagnostics)

    def test_original_column_matches_complete_reports_and_exact_csv(self):
        self._source_selection_reports("original-column-matches.tsv", "column-matches", 16)

    def test_original_column_values_complete_reports_and_exact_csv(self):
        self._source_selection_reports("original-column-values.tsv", "column-value", 20)

    def test_original_assertions_complete_reports_and_exact_csv(self):
        self._source_selection_reports("original-assertions.tsv", "assert", 17)

    def _source_selection_reports(self, filename, prefix, cases):
        with (ROOT / filename).open(encoding="utf-8") as stream:
            records = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(len(records), cases)
        for record in records:
            with self.subTest(case=record["case"]):
                raw = bytes.fromhex(record["source_hex"])
                def no_parent(*_):
                    self.fail("standalone filtered source reached parent authority")
                spec = yamaa_native._prepare_document("spec.yaml", raw, no_parent, no_parent, no_parent)
                content = bytes.fromhex(record["input_hex"])
                expected = json.loads(record["expected"])
                metadata = ("fixture-runtime", "fixture-engine", prefix + "-" + record["case"], "spec.yaml", ".")
                calls = []
                def capture(name, path, maximum):
                    self.assertEqual((name, path), ("SRC", "source.csv"))
                    self.assertLessEqual(len(content), maximum)
                    calls.append(path)
                    return content, len(calls) == 1
                for created in (1, 0):
                    expected["source_reads"][0]["snapshots_created"] = created
                    result = spec.build(capture, metadata)
                    unsaved = dict(expected, artifacts=[])
                    self.assertEqual(json.loads(result.observations()), unsaved)
                    if not record["artifact_hex"]:
                        self.assertIsNone(result.output())
                        with self.assertRaisesRegex(ValueError, "cannot save a failed build"):
                            result.save(lambda *_: self.fail("failed source filter published"))
                    else:
                        published = []
                        def publish(path, actual):
                            self.assertEqual(path, "result.csv")
                            self.assertEqual(actual, bytes.fromhex(record["artifact_hex"]))
                            published.append(actual)
                        for _ in range(2):
                            self.assertEqual(json.loads(result.save(publish)), expected)
                        self.assertEqual(len(published), 2)
                self.assertEqual(calls, ["source.csv", "source.csv"])

    def test_capture_failure_reply_validates_transport_and_preserves_interrupts(self):
        spec = prepare("schema-lookup")
        metadata = ("fixture-runtime", "fixture-engine", "schema-lookup", "spec.yaml", ".")
        for reply in (("unknown", OSError("opaque")), ("missing", object()), ("not_regular_file", False), ("missing",)):
            calls = []
            def capture(*args):
                calls.append(args)
                return reply
            with self.assertRaises((TypeError, ValueError)):
                spec.build(capture, metadata)
            self.assertEqual(len(calls), 1)
        for failure in (KeyboardInterrupt("returned interrupt"), SystemExit("returned exit")):
            with self.assertRaises(type(failure)) as caught:
                spec.build(lambda *_: ("missing", failure), metadata)
            self.assertIs(caught.exception, failure)

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

    def test_core_output_findings_preserve_independent_complete_failed_reports(self):
        self._assert_independent_failed_reports("output-declarations.tsv", "output", 5, 8)

    def test_core_grammar_preserves_independent_complete_failed_reports(self):
        self._assert_independent_failed_reports("grammar-diagnostics.tsv", "grammar", 7, 7)

    def test_core_binding_preserves_independent_complete_failed_reports(self):
        self._assert_independent_failed_reports("binding-diagnostics.tsv", "binding", 10, 11, b"ID,V\n1,2\n")

    def test_core_csv_profile_preserves_independent_complete_failed_reports(self):
        self._assert_independent_failed_reports("csv-profile-diagnostics.tsv", "csv", 13, 13)

    def test_core_window_findings_preserve_independent_complete_failed_reports(self):
        self._assert_independent_failed_reports("window-diagnostics.tsv", "window", 3, 3, b"ID,V\n1,2\n")

    def test_core_predicate_preserves_independent_complete_failed_report(self):
        self._assert_independent_failed_reports("predicate-diagnostics.tsv", "predicate", 1, 1, b"ID,V\n1,2\n")

    def test_core_source_typing_preserves_independent_complete_failed_reports(self):
        self._assert_independent_failed_reports("source-typing.tsv", "typing", 4, 4)

    def test_original_column_literals_preserve_reference_reports_and_exact_csv(self):
        self._assert_independent_scalar_reports("column-literals.tsv", "literal", 4)

    def test_original_conversion_handlers_preserve_reference_reports_and_exact_csv(self):
        self._assert_independent_scalar_reports("original-conversion-handlers.tsv", "handler", 5)

    def test_original_row_conversion_handlers_complete_reports_and_exact_csv(self):
        self._assert_independent_scalar_reports("original-row-conversion-handlers.tsv", "row-handler", 8)

    def _assert_independent_scalar_reports(self, fixture, prefix, cases):
        def no_parent(*_):
            self.fail("standalone literal document reached inheritance authority")
        with (ROOT / fixture).open(encoding="utf-8") as stream:
            records = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(len(records), cases)
        for record in records:
            with self.subTest(case=record["case"]):
                reads, saves = [], []
                def capture(dataset, path, maximum):
                    self.assertEqual((dataset, path), ("SRC", "source.csv"))
                    self.assertGreaterEqual(maximum, 5)
                    reads.append(path)
                    return b"ID\n1\n", True
                spec = yamaa_native._prepare_document("spec.yaml", record["source"].encode("ascii"), no_parent, no_parent, no_parent)
                result = spec.build(capture, ("fixture-runtime", "fixture-engine", prefix + "-" + record["case"], "spec.yaml", "."))
                del spec
                gc.collect()
                expected = json.loads(record["expected"])
                unsaved = dict(expected, artifacts=[])
                self.assertEqual(json.loads(result.observations()), unsaved)
                content = bytes.fromhex(record["output_hex"])
                if content:
                    self.assertIsNotNone(result.output())
                    def publish(path, actual):
                        self.assertEqual(path, "result.csv")
                        self.assertEqual(actual, content)
                        saves.append(actual)
                    for _ in range(2):
                        self.assertEqual(json.loads(result.save(publish)), expected)
                    self.assertEqual(len(saves), 2)
                else:
                    self.assertIsNone(result.output())
                    for _ in range(2):
                        with self.assertRaisesRegex(ValueError, "cannot save a failed build"):
                            result.save(lambda *_: self.fail("failed literal build published"))
                    self.assertEqual(saves, [])
                self.assertEqual(json.loads(result.observations()), unsaved)
                self.assertEqual(reads, ["source.csv"])

    def _assert_independent_failed_reports(self, fixture, prefix, cases, findings, content=b"ID\n1\n"):
        def no_parent(*_):
            self.fail("standalone document reached an inheritance port")
        with (ROOT / fixture).open(encoding="ascii") as stream:
            records = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(len(records), cases)
        self.assertEqual(sum(len(json.loads(r["expected"])["diagnostics"]) for r in records), findings)
        for record in records:
            with self.subTest(case=record["case"]):
                held = bytes.fromhex(record["source_hex"]) if "source_hex" in record else content
                reads = []
                def capture(dataset, path, maximum):
                    self.assertEqual((dataset, path), ("SRC", "source.csv"))
                    self.assertGreaterEqual(maximum, len(held))
                    reads.append(path)
                    return held, True
                spec = yamaa_native._prepare_document("spec.yaml", record["source"].encode("ascii"), no_parent, no_parent, no_parent)
                result = spec.build(capture, ("fixture-runtime", "fixture-engine", prefix + "-" + record["case"], "spec.yaml", "."))
                del spec
                gc.collect()
                self.assertEqual(json.loads(result.observations()), json.loads(record["expected"]))
                self.assertIsNone(result.output())
                for _ in range(2):
                    with self.assertRaisesRegex(ValueError, "cannot save a failed build"):
                        result.save(lambda *_: self.fail("failed specification reached publication"))
                    self.assertEqual(json.loads(result.observations()), json.loads(record["expected"]))
                self.assertEqual(reads, [r["path"] for r in json.loads(record["expected"])["source_reads"]])

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
                sources = {"DM":"input/dm.csv", "AE":"input/ae.csv", "MEDDRA":"input/meddict.csv"} if name == "schema-lookup" else ({"VS":"input/vs.csv"} if name in ("schema-window-functions", "negative-row-no-prior") else ({"ODM":"input/odm.csv"} if name in ("negative-source-missing-field", "negative-source-trivial-filter") else ({"DM":"input/dm.csv"} if name in ("negative-paired-dates", "negative-not-missing-age", "negative-implausible-age", "negative-invalid-sex", "negative-sex-code", "negative-matches-bad-pattern") else {"LB":"input/lb.csv"})))
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
        for candidate in (
            source.replace(b"100 * (AVAL - BASE) / BASE", b"LN(AVAL)"),
            source.replace(b"input/lb.csv", b"input/lb.unknown"),
        ):
            with self.subTest(source=candidate):
                self.assertNotEqual(candidate, source)
                with self.assertRaises(ValueError) as raised:
                    yamaa_native._prepare_specification(modules(), 0, "spec.yaml", candidate)
                self.assertEqual(
                    json.loads(str(raised.exception))["outcome"]["status"], "unsupported"
                )


class ParquetOriginalOutput(unittest.TestCase):
    @staticmethod
    def failed_report(name, diagnostics, paths):
        return {
            "artifacts": [], "backend": "rust", "callbacks": [],
            "diagnostics": diagnostics, "engine_version": "fixture-engine", "error": None,
            "example": name, "handler_counts": [],
            "nodes": [{"diagnostics": diagnostics, "handler_counts": [], "outcome": "failure",
                       "specification": "spec.yaml", "unsupported": []}],
            "outcome": "failure", "report_version": "0.3.0-draft", "runtime": "python",
            "runtime_version": "fixture-runtime",
            "source_reads": [{"base_directory": ".", "path": path, "outcome": "captured",
                              "condition": None, "snapshots_created": 1} for path in paths],
            "tables": [], "unsupported": [], "verifications": [],
        }

    def test_parquet_empty_string_policy_survives_build_and_later_save(self):
        for policy, expected in (("missing", b'I,S\n1,\n2,X\n'), ("present", b'I,S\n1,""\n2,X\n')):
            with self.subTest(policy=policy):
                source = json.dumps({"schema_version": "1.0", "domain": "TEST",
                    "input": {"SRC": {"path": "input.PARQUET", "empty_string": policy}},
                    "keys": ["I"], "columns": [
                        {"name": "I", "type": "int", "derivation": "SRC.I"},
                        {"name": "S", "type": "str", "derivation": "SRC.S"}],
                    "output": {"path": "output.csv", "columns": ["I", "S"]}}).encode()
                def no_port(*_):
                    self.fail("unexpected source authority")
                spec = yamaa_native._prepare_document("spec.yaml", source, no_port, no_port, no_port)
                requests, saved = [], []
                def capture(dataset, path, maximum):
                    requests.append((dataset, path))
                    content = (ROOT / "pq/text.parquet").read_bytes()
                    self.assertLessEqual(len(content), maximum)
                    return content, True
                result = spec.build(capture, ("fixture-runtime", "fixture-engine", policy, "spec.yaml", "."))
                del spec
                gc.collect()
                def publish(path, content):
                    self.assertEqual(path, "output.csv")
                    self.assertEqual(content, expected)
                    saved.append(content)
                for _ in range(2):
                    self.assertEqual(json.loads(result.save(publish))["outcome"], "success")
                self.assertEqual(requests, [("SRC", "input.PARQUET")])
                self.assertEqual(saved, [expected, expected])

    def test_parquet_semantic_failures_collect_in_source_order(self):
        source = json.dumps({"schema_version": "1.0", "domain": "TEST", "base": "FIRST",
            "input": {name: {"path": name + ".parquet"} for name in ("FIRST", "SECOND", "THIRD")},
            "keys": ["ID"], "columns": [{"name": "ID", "type": "int", "derivation": {"compute": {"expr": "1"}}}],
            "output": {"path": "output.csv", "columns": ["ID"]}}).encode()
        def no_port(*_):
            self.fail("failed build reached source or publication authority")
        spec = yamaa_native._prepare_document("spec.yaml", source, no_port, no_port, no_port)
        requests = []
        def capture(dataset, path, maximum):
            requests.append((dataset, path))
            fixture = {"FIRST": "utf8-time", "SECOND": "duplicate", "THIRD": "text"}[dataset]
            content = (ROOT / "pq" / (fixture + ".parquet")).read_bytes()
            self.assertLessEqual(len(content), maximum)
            return content, True
        result = spec.build(capture, ("fixture-runtime", "fixture-engine", "collection", "spec.yaml", "."))
        diagnostics = [
            {"phase": "ingest", "condition": "source_field_value_invalid", "requirement": "REQ-1041",
             "spec_paths": ["input.FIRST.path"], "context": {"dataset": "FIRST", "path": "FIRST.parquet", "field": "OTHER", "row": 1, "value": 1}},
            {"phase": "ingest", "condition": "source_field_name_duplicate", "requirement": "REQ-1039",
             "spec_paths": ["input.SECOND.path"], "context": {"dataset": "SECOND", "path": "SECOND.parquet", "field": "I"}},
        ]
        paths = [name + ".parquet" for name in ("FIRST", "SECOND", "THIRD")]
        self.assertEqual(json.loads(result.observations()), self.failed_report("collection", diagnostics, paths))
        self.assertIsNone(result.output())
        with self.assertRaisesRegex(ValueError, "cannot save a failed build"):
            result.save(no_port)
        self.assertEqual(requests, list(zip(("FIRST", "SECOND", "THIRD"), paths)))

    def test_original_ordered_sum_reads_parquet_and_preserves_complete_truth(self):
        name = "adam-adlb-ordered-sum"
        case = ROOT / "cases" / name
        source = (case / "spec.yaml").read_bytes().replace(
            b"path: input/lb.csv, types: {LBSTRESN: float}", b"path: input/lb.parquet"
        )
        expected = json.loads((ROOT / "expected" / (name + ".json")).read_text())
        for read in expected["source_reads"]:
            read["path"] = "input/lb.parquet"
        unsaved = dict(expected, artifacts=[])
        original_import = builtins.__import__
        def reject(module, *args, **kwargs):
            if module.split(".")[0] in {"yamaa", "pydantic", "yaml", "yaml12", "pyarrow"}:
                self.fail("host semantics during native Parquet ingestion: " + module)
            return original_import(module, *args, **kwargs)
        def no_parent(*_):
            self.fail("standalone preparation invoked a parent")
        requests, saved = [], []
        def capture(dataset, path, maximum):
            requests.append((dataset, path))
            content = (ROOT / "pq" / "ordered-sum.parquet").read_bytes()
            self.assertLessEqual(len(content), maximum)
            return content, True
        def publish(path, content):
            self.assertEqual(path, "adlb.csv")
            self.assertEqual(content, (case / "expected/adlb.csv").read_bytes())
            saved.append(content)
        with patch("builtins.__import__", side_effect=reject):
            spec = yamaa_native._prepare_document("spec.yaml", source, no_parent, no_parent, no_parent)
            result = spec.build(capture, ("fixture-runtime", "fixture-engine", name, "spec.yaml", "."))
            del spec
            gc.collect()
            self.assertEqual(json.loads(result.observations()), unsaved)
            self.assertIsNotNone(result.output())
            for _ in range(2):
                self.assertEqual(json.loads(result.save(publish)), expected)
            self.assertEqual(json.loads(result.observations()), unsaved)
        self.assertEqual(requests, [("LB", "input/lb.parquet")])
        self.assertEqual(len(saved), 2)

    def test_parquet_declarations_fail_before_any_host_source_authority(self):
        source = (ROOT / "cases/adam-adlb-ordered-sum/spec.yaml").read_bytes().replace(b"input/lb.csv", b"input/lb.parquet")
        def no_port(*_):
            self.fail("redundant type declaration reached a host port")
        with self.assertRaises(ValueError) as caught:
            yamaa_native._prepare_document("spec.yaml", source, no_port, no_port, no_port)
        self.assertEqual(json.loads(str(caught.exception)), {"protocol":"specification/prototype", "outcome":{"status":"invalid", "diagnostics":[{
            "phase":"validation", "condition":"redundant_field_type", "requirement":"REQ-0533",
            "spec_paths":["input.LB.types.LBSTRESN"], "context":{"dataset":"LB", "field":"LBSTRESN", "type":"float"},
        }]}})

    def test_parquet_ingestion_failures_retain_exact_conditions_and_never_publish(self):
        source = json.dumps({"schema_version":"1.0", "domain":"TEST", "input":{"SRC":{"path":"input.parquet"}}, "keys":["ID"],
            "columns":[{"name":"ID", "type":"int", "derivation":{"compute":{"expr":"1"}}}],
            "output":{"path":"output.csv", "columns":["ID"]}}).encode()
        def no_port(*_):
            self.fail("standalone preparation invoked a host port")
        spec = yamaa_native._prepare_document("spec.yaml", source, no_port, no_port, no_port)
        for name, condition, requirement, context in (
            ("utf8", "source_parquet_invalid", "REQ-1038", {}),
            ("utf8-bool", "source_field_type_unsupported", "REQ-1040", {"field":"OTHER","stored_type":"bool"}),
            ("utf8-time", "source_field_value_invalid", "REQ-1041", {"field":"OTHER","row":1,"value":1}),
            ("empty-name", "source_field_name_empty", "REQ-1039", {"field":1}),
            ("duplicate", "source_field_name_duplicate", "REQ-1039", {"field":"I"}),
            ("mixed-struct", "source_field_type_unsupported", "REQ-1040", {"field":"S", "stored_type":"struct<left: int64, right: string>"}),
            ("mixed-map", "source_field_type_unsupported", "REQ-1040", {"field":"M", "stored_type":"map<string, int64 ('M')>"}),
            ("mixed-empty", "source_field_name_empty", "REQ-1039", {"field":1}),
            ("mixed-duplicate", "source_field_name_duplicate", "REQ-1039", {"field":"I"}),
            *(("unsupported-" + name, "source_field_type_unsupported", "REQ-1040",
               {"field": "FIELD", "stored_type": stored}) for name, stored in (
                ("int32", "int32"), ("uint64", "uint64"), ("float32", "float"), ("binary", "binary"),
                ("milliseconds", "timestamp[ms]"), ("timezone", "timestamp[us, tz=UTC]"),
                ("list", "list<element: int64>"), ("fixed-list", "fixed_size_list<element: int64>[2]"),
                ("struct", "struct<item: int64>"), ("decimal", "decimal128(10, 2)"))),
        ):
            with self.subTest(case=name):
                requests=[]
                def capture(dataset, path, maximum):
                    requests.append((dataset,path))
                    return (ROOT / "pq" / (name + ".parquet")).read_bytes(), True
                result=spec.build(capture, ("fixture-runtime", "fixture-engine", name, "spec.yaml", "."))
                report=json.loads(result.observations())
                self.assertIsNone(result.output())
                diagnostics = [{"phase":"ingest", "condition":condition, "requirement":requirement,
                    "spec_paths":["input.SRC.path"], "context":dict(dataset="SRC",path="input.parquet",**context)}]
                self.assertEqual(report, self.failed_report(name, diagnostics, ["input.parquet"]))
                with self.assertRaisesRegex(ValueError, "cannot save a failed build"):
                    result.save(no_port)
                self.assertEqual(requests, [("SRC","input.parquet")])

    def test_original_build_saves_parquet_with_independent_logical_readback(self):
        # Only the declared output container changes; inputs and committed truth
        # remain unchanged. No reference module participates in native execution.
        name = "adam-adlb-ordered-sum"
        case = ROOT / "cases" / name
        raw = (case / "spec.yaml").read_bytes()
        source = raw.replace(b"path: adlb.csv", b"path: adlb.parquet")
        self.assertNotEqual(source, raw)
        expected = json.loads((ROOT / "expected" / (name + ".json")).read_text())
        unsaved = dict(expected, artifacts=[])
        original_import = builtins.__import__
        def reject(module, *args, **kwargs):
            if module.split(".")[0] in {"yamaa", "pydantic", "yaml", "yaml12", "pyarrow"}:
                self.fail("reference semantic import during native build/save: " + module)
            return original_import(module, *args, **kwargs)
        def no_parent(*_):
            self.fail("standalone preparation invoked a parent")
        requests, saved = [], []
        def capture(dataset, path, maximum):
            requests.append((dataset, path))
            data = (case / path).read_bytes()
            self.assertLessEqual(len(data), maximum)
            return data, True
        def publish(path, data):
            self.assertEqual(path, "adlb.parquet")
            saved.append(data)
        with patch("builtins.__import__", side_effect=reject):
            spec = yamaa_native._prepare_document("spec.yaml", source, no_parent, no_parent, no_parent)
            result = spec.build(capture, ("fixture-runtime", "fixture-engine", name, "spec.yaml", "."))
            del spec
            gc.collect()
            self.assertEqual(json.loads(result.observations()), unsaved)
            self.assertIsNotNone(result.output())
            report = json.loads(result.save(publish))
            self.assertEqual(json.loads(result.observations()), unsaved)
            self.assertEqual(dict(report, artifacts=[]), unsaved)
        self.assertEqual(requests, [("LB", "input/lb.csv")])
        self.assertEqual(len(saved), 1)

        # This independent reader runs only after the native build/save has
        # completed under the import guard. It is an oracle, never a fallback.
        import io
        import struct
        import pyarrow.parquet as pq
        reader = pq.ParquetFile(io.BytesIO(saved[0]))
        artifact = expected["artifacts"][0]
        derived = next(t for t in expected["tables"] if t["stage"] == "derived")
        self.assertEqual(reader.schema_arrow.names, artifact["columns"])
        self.assertIsNone(reader.metadata.metadata)
        self.assertEqual(reader.metadata.num_rows, artifact["row_count"])
        expected_types = {"str": ("BYTE_ARRAY", "STRING"), "float": ("DOUBLE", "NONE")}
        for column, kind in enumerate(artifact["types"]):
            physical, logical = expected_types[kind]
            self.assertEqual(reader.schema.column(column).physical_type, physical)
            self.assertEqual(reader.schema.column(column).logical_type.type, logical)
        for group in range(reader.metadata.num_row_groups):
            for column in range(reader.metadata.num_columns):
                self.assertEqual(reader.metadata.row_group(group).column(column).compression, "UNCOMPRESSED")
                self.assertEqual(reader.schema.column(column).max_definition_level, 1)
        actual = reader.read().to_pylist()
        projection = [derived["columns"].index(c) for c in artifact["columns"]]
        for row, truth in zip(actual, derived["rows"], strict=True):
            for column, index in zip(artifact["columns"], projection, strict=True):
                cell = truth[index]
                value = row[column]
                if cell["type"] == "float":
                    value = struct.pack(">d", value).hex()
                elif cell["type"] == "int":
                    value = str(value)
                self.assertEqual(value, cell["value"])
        observed = report["artifacts"][0]
        self.assertEqual(observed["profile"], "parquet")
        self.assertEqual(observed["content"], "")
        self.assertEqual(observed["byte_length"], len(saved[0]))
        logical = [json.dumps(artifact["columns"], ensure_ascii=True)]
        logical.extend(json.dumps([row[c] for c in artifact["columns"]], ensure_ascii=True) for row in actual)
        self.assertEqual(observed["records"], logical)

    def test_closed_parquet_types_and_exact_values_with_independent_reader(self):
        # Hand-authored boundary values qualify the codec independently of the
        # existing original-document cohort and its unchanged expected reports.
        kinds = {"ID": "str", "S": "str", "I": "int", "F": "float", "D": "date", "T": "datetime"}
        projection = ["T", "I", "S", "F", "D", "EMPTY"]
        specification = {
            "schema_version": "1.0", "domain": "TEST", "keys": ["I"],
            "input": {"SOURCE": {"path": "input.csv", "types": kinds}},
            "output": {"path": "output.parquet", "columns": projection},
            "columns": [
                {"name": name, "type": kind, "label": name, "derivation": "SOURCE." + name}
                for name, kind in kinds.items()
            ] + [{"name": "EMPTY", "type": "str", "label": "Empty text"}],
            "rows": [{"id": "all", "derivations": {"EMPTY": {"literal": ""}}}],
        }
        data = (
            "ID,S,I,F,D,T\n"
            "a,,\u002d9223372036854775808,-0.0,0001-01-01,1969-12-31T23:59:59\n"
            "b,\u00e9\U0001f642,9223372036854775807,5e-324,9999-12-31,9999-12-31T23:59:59\n"
            "c,text,9007199254740993,1.2345678901234567,,\n"
        ).encode("utf-8")
        original_import = builtins.__import__
        def reject(module, *args, **kwargs):
            if module.split(".")[0] in {"yamaa", "pydantic", "yaml", "yaml12", "pyarrow"}:
                self.fail("reference semantic import during native build/save: " + module)
            return original_import(module, *args, **kwargs)
        def no_parent(*_):
            self.fail("standalone preparation invoked a parent")
        requests, saved = [], []
        def capture(dataset, path, maximum):
            requests.append((dataset, path))
            self.assertLessEqual(len(data), maximum)
            return data, True
        def publish(path, content):
            self.assertEqual(path, "output.parquet")
            saved.append(content)
        with patch("builtins.__import__", side_effect=reject):
            prepared = yamaa_native._prepare_document(
                "spec.yaml", json.dumps(specification).encode("ascii"), no_parent, no_parent, no_parent
            )
            result = prepared.build(capture, ("fixture-runtime", "fixture-engine", "parquet-types", "spec.yaml", "."))
            self.assertIsNotNone(result.output(), result.observations())
            report = json.loads(result.save(publish))
        self.assertEqual(requests, [("SOURCE", "input.csv")])
        self.assertEqual(len(saved), 1)

        import datetime
        import io
        import struct
        import pyarrow as pa
        import pyarrow.parquet as pq
        reader = pq.ParquetFile(io.BytesIO(saved[0]))
        self.assertEqual(reader.schema_arrow, pa.schema([
            pa.field("T", pa.timestamp("us")), pa.field("I", pa.int64()),
            pa.field("S", pa.string()), pa.field("F", pa.float64()),
            pa.field("D", pa.date32()), pa.field("EMPTY", pa.string()),
        ]))
        self.assertIsNone(reader.metadata.metadata)
        self.assertEqual(reader.metadata.num_rows, 3)
        self.assertEqual([reader.schema.column(i).physical_type for i in range(6)],
                         ["INT64", "INT64", "BYTE_ARRAY", "DOUBLE", "INT32", "BYTE_ARRAY"])
        timestamp = json.loads(reader.schema.column(0).logical_type.to_json())
        self.assertEqual(timestamp["Type"], "Timestamp")
        self.assertIs(timestamp["isAdjustedToUTC"], False)
        self.assertEqual(timestamp["timeUnit"], "microseconds")
        for i in range(6):
            self.assertEqual(reader.schema.column(i).max_definition_level, 1)
            self.assertEqual(reader.metadata.row_group(0).column(i).compression, "UNCOMPRESSED")
        rows = reader.read().to_pylist()
        self.assertEqual([row["I"] for row in rows], [-(2**63), 2**63 - 1, 2**53 + 1])
        self.assertEqual([row["S"] for row in rows], [None, "\u00e9\U0001f642", "text"])
        self.assertEqual([row["EMPTY"] for row in rows], ["", "", ""])
        self.assertEqual([struct.pack(">d", row["F"]).hex() for row in rows],
                         ["8000000000000000", "0000000000000001", "3ff3c0ca428c59fb"])
        self.assertEqual([row["D"] for row in rows],
                         [datetime.date(1, 1, 1), datetime.date(9999, 12, 31), None])
        self.assertEqual([row["T"] for row in rows],
                         [datetime.datetime(1969, 12, 31, 23, 59, 59),
                          datetime.datetime(9999, 12, 31, 23, 59, 59), None])
        logical = [json.dumps(projection)]
        logical.extend(json.dumps([row[name] for name in projection], default=lambda value: value.isoformat()) for row in rows)
        self.assertEqual(report["artifacts"][0]["records"], logical)


if __name__ == "__main__":
    unittest.main()
