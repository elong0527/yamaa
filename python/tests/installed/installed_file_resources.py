"""Public filesystem qualification with reference imports forbidden.

The eight capture-stage partial reports and private cache/limit/Changed facts
remain in typed native tests and installed_original_specifications.py. Public
runs inspect all sources first, so physical missing/directory cases use the
unchanged inspection truth and retain zero study reads.
"""

import builtins
import csv
import gc
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from host_fixtures import load as load_truth

import yamaa
from yamaa import _native

TRUTH = load_truth(__file__, "file-resources")


class AnyMessage:
    """Equal to any rendered message: these cases pin what an issue is, and
    the registry's message tests pin its words."""

    def __eq__(self, other):
        return isinstance(other, str) and bool(other)

    def __repr__(self):
        return "<a message>"


class Located:
    """Equal to a location cell holding a `kind`: these cases pin what an
    issue is, and installed_issue_locations pins where it is (P-ISSUE-1)."""

    def __init__(self, kind):
        self.kind = kind

    def __eq__(self, other):
        return isinstance(other, self.kind) and not isinstance(other, bool)

    def __repr__(self):
        return f"<a located {self.kind.__name__}>"


ROOT = Path(__file__).with_name("specification-original")
PATHS = TRUTH["lookup_paths"]


def paths_of(*paths):
    """The paths save() returns: absolute, with forward slashes, in order."""
    return [Path(path).resolve().as_posix() for path in paths]


class FileResources(unittest.TestCase):
    def test_entry_relative_shared_program_isolates_study_inputs_and_saved_outputs(
        self,
    ):
        self.entry_relative(False)

    def test_entry_relative_schema_links_select_each_study_producer_wrapper(self):
        self.entry_relative(True)

    def entry_relative(self, producer):
        truth = TRUTH["entry_relative"]
        with tempfile.TemporaryDirectory(prefix="yamaa-study-paths-") as directory:
            root = Path(directory)
            shared = root / "shared"
            shared.mkdir()
            for name, content in truth["shared"].items():
                (shared / name).write_text(content, encoding="ascii")
            results = []
            for name, csv in truth["studies"].items():
                study = root / name
                (study / "input").mkdir(parents=True)
                (study / "output").mkdir()
                (study / "input/data.csv").write_bytes(csv.encode("ascii"))
                entry = study / "spec.yaml"
                program = "consumer" if producer else "ordinary"
                entry.write_text(
                    f"schema_version: '1.0'\nparent: ../shared/{program}.yaml\n",
                    encoding="ascii",
                )
                (study / "producer.yaml").write_text(
                    truth["producer_entry"], encoding="ascii"
                )
                if producer:
                    # A link only types the stored artifact: callers build and
                    # save each study's producer before its consumer reads it.
                    produced = yamaa.spec(study / "producer.yaml").run()
                    self.assertTrue(produced.issues.is_empty(), produced.issues)
                    self.assertEqual(
                        produced.save(), paths_of(study / "output/produced.csv")
                    )
                    self.assertEqual(
                        (study / "output/produced.csv").read_bytes(),
                        csv.encode("ascii"),
                    )
                spec = yamaa.spec(entry)
                self.assertTrue(spec.check().issues.is_empty())
                result = spec.run()
                self.assertTrue(result.issues.is_empty(), result.issues)
                self.assertFalse((study / "output/result.csv").exists())
                results.append((study, csv.encode("ascii"), result))
            shutil.rmtree(shared)
            for study, csv, result in results:
                shutil.rmtree(study / "input")
                self.assertEqual(
                    result.save(),
                    paths_of(study / "output/report.csv", study / "output/result.csv"),
                )
                self.assertEqual((study / "output/result.csv").read_bytes(), csv)
                self.assertTrue((study / "output/report.csv").is_file())
                if producer:
                    self.assertEqual((study / "output/produced.csv").read_bytes(), csv)

    def setUp(self):
        original = builtins.__import__
        blocked = (
            "yamaa._reference_domain",
            "yamaa.io",
            "yamaa.models",
            "yamaa.adapters",
            "yamaa.runtime",
            "yamaa.planning",
            "yamaa.expressions",
            "yamaa.regex",
            "yamaa.verification",
            "yamaa.submission",
            "yamaa.specification",
            "yamaa.schema",
        )

        def reject(name, *args, **kwargs):
            if name in {"yaml", "yaml12", "pydantic"} or any(
                name == prefix or name.startswith(prefix + ".") for prefix in blocked
            ):
                raise AssertionError("reference semantic import: " + name)
            return original(name, *args, **kwargs)

        guard = patch("builtins.__import__", side_effect=reject)
        guard.start()
        self.addCleanup(guard.stop)

    def case(self, directory):
        target = Path(directory) / "case"
        shutil.copytree(ROOT / "cases/schema-lookup", target)
        return target, target / "spec.yaml"

    def records(self, name, count):
        with (ROOT / name).open(encoding="ascii", newline="") as stream:
            rows = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(len(rows), count)
        return rows

    def expected(self, report, entry, artifacts=False):
        report = json.loads(report)
        report.update(
            runtime="python",
            runtime_version=sys.version,
            engine_version=_native._CORE_VERSION,
            example="domain",
        )
        for group in ("nodes", "tables", "verifications", "callbacks"):
            for observation in report[group]:
                self.assertEqual(observation["specification"], "spec.yaml")
                observation["specification"] = str(entry)
        if not artifacts:
            report["artifacts"] = []
        return report

    def lookup_expected(self, entry, artifacts=False):
        return self.expected(
            (ROOT / "expected/schema-lookup.json").read_text(), entry, artifacts
        )

    def observe(self, result):
        return json.loads(result._native.observations())

    def fail_files(self, case, failures):
        for name, kind in failures.items():
            path = case / PATHS[name]
            path.unlink()
            if kind == "not_regular_file":
                path.mkdir()

    def failed(self, result, expected, case):
        self.assertEqual(self.observe(result), expected)
        self.assertIsNone(result.output)
        self.assertEqual(
            result.issues.columns,
            TRUTH["issue_columns"],
        )
        self.assertEqual(
            result.issues.to_dicts(),
            [
                {
                    "severity": "error",
                    **finding,
                    "file": Located(str),
                    "line": Located(int),
                    "column": Located(int),
                    "message": AnyMessage(),
                    "context": json.dumps(
                        finding["context"],
                        ensure_ascii=False,
                        sort_keys=True,
                        separators=(",", ":"),
                    ),
                    "source_span": None,
                    "operand_route": None,
                }
                for finding in expected["diagnostics"]
            ],
        )
        gc.collect()
        for _ in range(2):
            with self.assertRaises(yamaa.Error) as caught:
                result.save()
            self.assertTrue(caught.exception.issues.equals(result.issues))
            self.assertEqual(caught.exception.paths, [])
            self.assertEqual(self.observe(result), expected)
            self.assertFalse((case / "adsl.csv").exists())

    def test_original_physical_capture_cases_keep_complete_public_preflight_failures(
        self,
    ):
        inspection = {
            tuple(sorted(json.loads(row["failures"]).items())): row["expected"]
            for row in self.records("source-inspection.tsv", 7)
        }
        for row in self.records("source-capture.tsv", 8):
            # Private capture-stage/cache assertions still consume this exact
            # fixture. The public route owns inspection before byte capture.
            name = "DM" if row["fail_at"] == "0" else "AE"
            failures = {name: row["kind"]}
            with (
                self.subTest(kind=row["kind"], at=row["fail_at"], cached=row["cached"]),
                tempfile.TemporaryDirectory() as directory,
            ):
                case, entry = self.case(directory)
                self.fail_files(case, failures)
                expected = self.expected(inspection[tuple(failures.items())], entry)
                for _ in range(2):
                    self.failed(yamaa.spec(entry).run(), expected, case)

    def test_all_physical_inspection_failures_precede_every_study_read(self):
        for row in self.records("source-inspection.tsv", 7):
            with (
                self.subTest(case=row["case"]),
                tempfile.TemporaryDirectory() as directory,
            ):
                case, entry = self.case(directory)
                failures = json.loads(row["failures"])
                self.fail_files(case, failures)
                # Any premature decode would fail differently. Exact truth
                # requires every metadata cause in order, with zero reads.
                for name in PATHS.keys() - failures.keys():
                    (case / PATHS[name]).write_bytes(b"\xff")
                self.assertTrue(yamaa.spec(entry).check().issues.is_empty())
                expected = self.expected(row["expected"], entry)
                for _ in range(2):
                    self.failed(yamaa.spec(entry).run(), expected, case)

    def test_each_public_run_reinspects_files_without_reusing_prior_source_bytes(
        self,
    ):
        with tempfile.TemporaryDirectory() as directory:
            case, entry = self.case(directory)
            expected = self.lookup_expected(entry)
            spec = yamaa.spec(entry)
            first = spec.run()
            self.assertEqual(self.observe(first), expected)
            content = (case / PATHS["DM"]).read_bytes()
            (case / PATHS["DM"]).unlink()
            missing = next(
                row
                for row in self.records("source-inspection.tsv", 7)
                if row["case"] == "missing_first"
            )
            self.failed(spec.run(), self.expected(missing["expected"], entry), case)
            self.assertEqual(self.observe(first), expected)
            (case / PATHS["DM"]).write_bytes(content)
            self.assertEqual(self.observe(spec.run()), expected)

    def test_changed_files_do_not_replace_owned_values_and_fresh_runs_read_new_bytes(
        self,
    ):
        with tempfile.TemporaryDirectory() as directory:
            case, entry = self.case(directory)
            spec = yamaa.spec(entry)
            first = spec.run()
            expected = self.lookup_expected(entry)
            self.assertEqual(self.observe(first), expected)
            source = case / PATHS["AE"]
            content = source.read_bytes()
            old = b"MYOCARDIAL INFARCTION,FATAL,150\n"
            self.assertEqual(content.count(old), 1)
            source.write_bytes(
                content.replace(old, b"MYOCARDIAL INFARCTION,FATAL,151\n")
            )
            second = spec.run()
            self.assertTrue(second.issues.is_empty(), second.issues)
            self.assertEqual(first.output["DTHDY"].to_list(), [150, 80, None, None])
            self.assertEqual(second.output["DTHDY"].to_list(), [151, 80, None, None])
            self.assertEqual(self.observe(first), expected)
            self.assertEqual(
                [r["snapshots_created"] for r in self.observe(second)["source_reads"]],
                [1, 1, 1],
            )
            self.assertEqual(first.save(), paths_of(case / "adsl.csv"))
            self.assertEqual(
                (case / "adsl.csv").read_bytes(),
                (case / "expected/adsl.csv").read_bytes(),
            )

    def test_original_lookup_keeps_complete_owned_report_and_two_exact_saves(self):
        with tempfile.TemporaryDirectory() as directory:
            case, entry = self.case(directory)
            result = yamaa.spec(entry).run()
            self.assertEqual(self.observe(result), self.lookup_expected(entry))
            self.assertTrue(result.issues.is_empty())
            held = result.output
            self.assertEqual(held["USUBJID"].to_list(), ["01", "02", "03", "04"])
            self.assertEqual(held["DTHDY"].to_list(), [150, 80, None, None])
            golden = (case / "expected/adsl.csv").read_bytes()
            expected = self.lookup_expected(entry, True)
            entry.unlink()
            shutil.rmtree(case / "input")
            gc.collect()
            self.assertEqual(result.save(), paths_of(case / "adsl.csv"))
            self.assertEqual((case / "adsl.csv").read_bytes(), golden)
            self.assertEqual(self.observe(result), expected)
            self.assertTrue(result.output.equals(held))
            del result
            gc.collect()
            self.assertEqual(held["DTHDY"].to_list(), [150, 80, None, None])

    def simple_document(self, source):
        return (
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n"
            f"input: {{SRC: {json.dumps(source)}}}\n"
            "columns: [{name: ID, type: int, derivation: SRC.ID}]\n"
            "output: {path: result.csv, columns: [ID]}\n"
        )

    def test_parent_and_absolute_local_sources_use_approved_public_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            source = root / "outside.csv"
            source.write_bytes(b"ID\n7\n")
            case = root / "case"
            case.mkdir()
            entry = case / "spec.yaml"
            for written in ("../outside.csv", source.as_posix()):
                entry.write_text(self.simple_document(written), encoding="ascii")
                result = yamaa.spec(entry).run()
                self.assertTrue(result.issues.is_empty(), result.issues)
                self.assertEqual(result.output["ID"].to_list(), [7])
                self.assertEqual(result.save(), paths_of(case / "result.csv"))
                self.assertEqual((case / "result.csv").read_bytes(), b"ID\n7\n")

    def test_physical_symlink_refusal_retains_native_condition_without_output(self):
        with tempfile.TemporaryDirectory() as directory:
            case = Path(directory).resolve()
            (case / "source.csv").write_bytes(b"ID\n7\n")
            try:
                (case / "alias.csv").symlink_to("source.csv")
            except OSError as error:
                self.skipTest(
                    "host cannot create physical symlink control: " + str(error)
                )
            entry = case / "spec.yaml"
            entry.write_text(self.simple_document("alias.csv"), encoding="ascii")
            result = yamaa.spec(entry).run()
            self.assertIsNone(result.output)
            self.assertEqual(
                result.issues.to_dicts(),
                [
                    {
                        "severity": "error",
                        "phase": "validation",
                        "condition": "resource_path_symlink",
                        "rule": "FILE-8",
                        "spec_paths": ["input.SRC.path"],
                        "file": Located(str),
                        "line": Located(int),
                        "column": Located(int),
                        "message": AnyMessage(),
                        "context": TRUTH["alias_context_json"],
                        "source_span": None,
                        "operand_route": None,
                    }
                ],
            )
            with self.assertRaises(yamaa.Error):
                result.save()
            self.assertFalse((case / "result.csv").exists())


if __name__ == "__main__":
    unittest.main()
