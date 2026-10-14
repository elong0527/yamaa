"""Public filesystem qualification with reference imports forbidden.

The eight capture-stage partial reports and private cache/limit/Changed facts
remain in typed native tests and installed_original_specifications.py. Public
builds inspect all sources first, so physical missing/directory cases use the
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

import yamaa
from yamaa import _native

ROOT = Path(__file__).with_name("specification-original")
PATHS = {"DM": "input/dm.csv", "AE": "input/ae.csv", "MEDDRA": "input/meddict.csv"}


class FileResources(unittest.TestCase):
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
            ["phase", "condition", "requirement", "spec_paths", "context"],
        )
        self.assertEqual(
            result.issues.to_dicts(),
            [
                {
                    **finding,
                    "context": json.dumps(
                        finding["context"],
                        ensure_ascii=False,
                        sort_keys=True,
                        separators=(",", ":"),
                    ),
                }
                for finding in expected["diagnostics"]
            ],
        )
        gc.collect()
        for _ in range(2):
            with self.assertRaises(yamaa.DomainError):
                result.save()
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
                    self.failed(yamaa.domain(entry), expected, case)

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
                self.assertTrue(yamaa.check(entry).issues.is_empty())
                expected = self.expected(row["expected"], entry)
                for _ in range(2):
                    self.failed(yamaa.domain(entry), expected, case)

    def test_each_public_build_reinspects_files_without_reusing_prior_source_bytes(
        self,
    ):
        with tempfile.TemporaryDirectory() as directory:
            case, entry = self.case(directory)
            expected = self.lookup_expected(entry)
            first = yamaa.domain(entry)
            self.assertEqual(self.observe(first), expected)
            content = (case / PATHS["DM"]).read_bytes()
            (case / PATHS["DM"]).unlink()
            missing = next(
                row
                for row in self.records("source-inspection.tsv", 7)
                if row["case"] == "missing_first"
            )
            self.failed(
                yamaa.domain(entry), self.expected(missing["expected"], entry), case
            )
            self.assertEqual(self.observe(first), expected)
            (case / PATHS["DM"]).write_bytes(content)
            self.assertEqual(self.observe(yamaa.domain(entry)), expected)

    def test_changed_files_do_not_replace_owned_values_and_fresh_builds_read_new_bytes(
        self,
    ):
        with tempfile.TemporaryDirectory() as directory:
            case, entry = self.case(directory)
            first = yamaa.domain(entry)
            expected = self.lookup_expected(entry)
            self.assertEqual(self.observe(first), expected)
            source = case / PATHS["AE"]
            content = source.read_bytes()
            old = b"MYOCARDIAL INFARCTION,FATAL,150\n"
            self.assertEqual(content.count(old), 1)
            source.write_bytes(
                content.replace(old, b"MYOCARDIAL INFARCTION,FATAL,151\n")
            )
            second = yamaa.domain(entry)
            self.assertTrue(second.issues.is_empty(), second.issues)
            self.assertEqual(first.output["DTHDY"].to_list(), [150, 80, None, None])
            self.assertEqual(second.output["DTHDY"].to_list(), [151, 80, None, None])
            self.assertEqual(self.observe(first), expected)
            self.assertEqual(
                [r["snapshots_created"] for r in self.observe(second)["source_reads"]],
                [1, 1, 1],
            )
            self.assertTrue(first.save())
            self.assertEqual(
                (case / "adsl.csv").read_bytes(),
                (case / "expected/adsl.csv").read_bytes(),
            )

    def test_original_lookup_keeps_complete_owned_report_and_two_exact_saves(self):
        with tempfile.TemporaryDirectory() as directory:
            case, entry = self.case(directory)
            result = yamaa.domain(entry)
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
            for _ in range(2):
                self.assertTrue(result.save())
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
                result = yamaa.domain(entry)
                self.assertTrue(result.issues.is_empty(), result.issues)
                self.assertEqual(result.output["ID"].to_list(), [7])
                self.assertTrue(result.save())
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
            result = yamaa.domain(entry)
            self.assertIsNone(result.output)
            self.assertEqual(
                result.issues.to_dicts(),
                [
                    {
                        "phase": "validation",
                        "condition": "resource_path_symlink",
                        "requirement": "REQ-0783",
                        "spec_paths": ["input.SRC.path"],
                        "context": '{"dataset":"SRC","path":"alias.csv"}',
                    }
                ],
            )
            with self.assertRaises(yamaa.DomainError):
                result.save()
            self.assertFalse((case / "result.csv").exists())


if __name__ == "__main__":
    unittest.main()
