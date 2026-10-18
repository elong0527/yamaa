"""Installed public run report, complete independent run truth and held publication."""

import builtins
import csv
import gc
import io
import json
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import polars as pl
from host_fixtures import load as load_truth

import yamaa
from yamaa import _native

TRUTH = load_truth(__file__, "verification-logs")

ROOT = Path(__file__).with_name("specification-original")
CASE = TRUTH["case_name"]
# The case declares `output.report` and `output.path`; the run report is
# written first, then the primary artifact (REQ-0757).
NAMES = ("adsl-checks.csv", "adsl.csv")
REPORT, OUTPUT = NAMES


def kinds(artifact):
    return {
        column: pl.Int64 if kind == "int" else pl.String
        for column, kind in zip(artifact["columns"], artifact["types"], strict=True)
    }


class RunReport(unittest.TestCase):
    def setUp(self):
        original = builtins.__import__
        blocked = (
            "yamaa._reference_domain",
            "yamaa.runtime",
            "yamaa.planning",
            "yamaa.expressions",
            "yamaa.specification",
            "yamaa.schema",
            "yamaa.verification",
            "yamaa.functions",
            "yamaa.models",
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

    def expected(self, entry):
        report = json.loads((ROOT / "expected" / (CASE + ".json")).read_text())
        report.update(
            runtime="python",
            runtime_version=sys.version,
            engine_version=_native._CORE_VERSION,
            example="domain",
        )
        for group in ("nodes", "tables", "verifications", "callbacks"):
            for record in report[group]:
                record["specification"] = str(entry)
        return report

    def retain(self, run, entry):
        destination = os.environ.get("YAMAA_PUBLIC_REPORT_DIR")
        if not destination:
            return
        report = json.loads(run._native.observations())
        self.assertEqual(report["example"], "domain")
        report["example"] = CASE
        for group in ("nodes", "tables", "verifications", "callbacks"):
            for record in report[group]:
                self.assertEqual(record["specification"], str(entry))
                record["specification"] = entry.name
        directory = Path(destination)
        directory.mkdir(parents=True, exist_ok=True)
        (directory / (CASE + ".python.rust.json")).write_text(
            json.dumps(report, sort_keys=True, separators=(",", ":")) + "\n",
            encoding="utf-8",
        )

    def test_original_full_report_typed_views_saved_goldens_and_owned_lifetime(self):
        with tempfile.TemporaryDirectory() as directory:
            case = Path(directory) / "case"
            shutil.copytree(ROOT / "cases" / CASE, case)
            entry = case / "spec.yaml"
            expected = self.expected(entry)
            run = yamaa.spec(entry).run()
            self.assertTrue(run.issues.is_empty())
            self.assertEqual(
                json.loads(run._native.observations()), dict(expected, artifacts=[])
            )
            for index, view in enumerate((run.report, run.output)):
                types = kinds(expected["artifacts"][index])
                golden = pl.read_csv(
                    case / "expected" / NAMES[index], schema_overrides=types
                )
                self.assertEqual(view.schema, types)
                self.assertTrue(view.equals(golden))
            # Each public view is a fresh host copy of privately owned bytes.
            # The report has a row for the run, its input, its rows, two
            # columns and three checks.
            changed = run.report.replace_column(
                0, pl.Series("ARTIFACT", ["changed"] * 8)
            )
            self.assertEqual(changed["ARTIFACT"].to_list(), ["changed"] * 8)
            self.assertEqual(run.report["ARTIFACT"].to_list(), [OUTPUT] * 8)
            for name in NAMES:
                self.assertFalse((case / name).exists())
            entry.unlink()
            (case / "input/dm.csv").unlink()
            gc.collect()
            for _ in range(2):
                run.save()
                self.assertEqual(json.loads(run._native.observations()), expected)
                for name in NAMES:
                    self.assertEqual(
                        (case / name).read_bytes(),
                        (case / "expected" / name).read_bytes(),
                    )
            self.retain(run, entry)
            held = run.report
            del run
            gc.collect()
            self.assertEqual(held.height, 8)
            checks = held.filter(pl.col("KIND") == "check")
            self.assertEqual(
                checks["TOTAL"].to_list(),
                [int(value) for value in TRUTH["evaluated_counts"]],
            )

    def test_fatal_check_saves_only_the_report_and_keeps_original_issues(self):
        with tempfile.TemporaryDirectory() as directory:
            case = Path(directory) / "case"
            shutil.copytree(ROOT / "cases" / CASE, case)
            entry = case / "spec.yaml"
            entry.write_text(
                entry.read_text().replace("severity: warning", "severity: error")
            )
            # The failed run's report, from the independent golden of the
            # successful run: the run row records the failure and has no
            # output count, a failed run has no column rows, and only the
            # checks of the failed stage have rows (REQ-0392, REQ-0393).
            golden = (case / "expected" / REPORT).read_text(encoding="utf-8")
            header, _, source, rows, _, _, not_missing, violated, *_ = (
                golden.splitlines(keepends=True)
            )
            failed_report = (
                header
                + "adsl.csv,run,output,,,,,,failed,range_failed,,,,{}\n"
                + source
                + rows
                + not_missing
                + violated.replace(",warning,", ",error,")
            ).encode("utf-8")
            run = yamaa.spec(entry).run()
            self.assertIsNone(run.output)
            checks = run.report.filter(pl.col("KIND") == "check")
            self.assertEqual(checks["OUTCOME"].to_list(), TRUTH["fatal_outcomes"])
            self.assertTrue(
                run.report.equals(
                    pl.read_csv(
                        io.BytesIO(failed_report), schema_overrides=run.report.schema
                    )
                )
            )
            issues = run.issues.clone()
            self.assertEqual(issues["condition"].to_list(), ["range_failed"])
            for name in NAMES:
                (case / name).write_bytes(b"sentinel")
            entry.unlink()
            (case / "input/dm.csv").unlink()
            for _ in range(2):
                with self.assertRaises(yamaa.Error) as raised:
                    run.save()
                self.assertTrue(raised.exception.issues.equals(issues))
                self.assertTrue(run.issues.equals(issues))
                self.assertEqual((case / REPORT).read_bytes(), failed_report)
                self.assertEqual((case / OUTPUT).read_bytes(), b"sentinel")
                self.assertEqual(
                    len(json.loads(run._native.observations())["artifacts"]), 1
                )

    def test_parquet_report_decodes_to_the_independent_csv_golden(self):
        with tempfile.TemporaryDirectory() as directory:
            case = Path(directory) / "case"
            shutil.copytree(ROOT / "cases" / CASE, case)
            entry = case / "spec.yaml"
            entry.write_text(
                entry.read_text().replace("adsl-checks.csv", "adsl-checks.parquet")
            )
            expected = self.expected(entry)
            run = yamaa.spec(entry).run()
            self.assertTrue(run.issues.is_empty())
            self.assertEqual(
                json.loads(run._native.observations()), dict(expected, artifacts=[])
            )
            types = kinds(expected["artifacts"][0])
            golden = pl.read_csv(case / "expected" / REPORT, schema_overrides=types)
            self.assertEqual(run.report.schema, types)
            self.assertTrue(run.report.equals(golden))
            entry.unlink()
            (case / "input/dm.csv").unlink()
            gc.collect()
            run.save()
            report = json.loads(run._native.observations())
            actual_artifacts = report.pop("artifacts")
            expected.pop("artifacts")
            self.assertEqual(report, expected)
            path = case / "adsl-checks.parquet"
            actual = pl.read_parquet(path)
            self.assertEqual(actual.schema, golden.schema)
            self.assertTrue(actual.equals(golden))
            artifact = actual_artifacts[0]
            self.assertEqual(artifact["name"], path.stem)
            self.assertEqual(artifact["profile"], "parquet")
            self.assertEqual(artifact["columns"], golden.columns)
            self.assertEqual(artifact["row_count"], golden.height)
            self.assertEqual(artifact["byte_length"], path.stat().st_size)
            self.assertEqual(artifact["content"], "")
            self.assertEqual(
                [json.loads(row) for row in artifact["records"][1:]],
                [list(row) for row in golden.rows()],
            )
            self.assertEqual(
                (case / OUTPUT).read_bytes(),
                (case / "expected" / OUTPUT).read_bytes(),
            )

    def test_full_warning_keys_unicode_identity_unchecked_types_and_target_retry(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            entry = root / "spec.yaml"
            raw = """schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: input.csv}
output: {path: absent/out.csv, columns: [ID], report: checks.csv}
columns:
  - name: ID
    type: int
    derivation: SRC.ID
    verifications:
      - range: {max: 0, severity: warning, id: "\\u00e9\\U0001d11e"}
"""
            entry.write_text(raw, encoding="utf-8")
            (root / "input.csv").write_text(
                "ID\n" + "\n".join(map(str, range(1, 9))) + "\n"
            )
            run = yamaa.spec(entry).run()
            self.assertTrue(run.issues.is_empty())
            warning = run.report.filter(pl.col("KIND") == "check")
            self.assertEqual(warning["ID"].to_list(), ["\u00e9\U0001d11e"])
            self.assertEqual(warning["SEVERITY"].to_list(), ["warning"])
            self.assertEqual(warning["OUTCOME"].to_list(), ["violated"])
            self.assertEqual(warning["COUNT"].to_list(), [8])
            self.assertEqual(
                warning["KEYS"].to_list(),
                [
                    json.dumps(
                        [{"ID": i} for i in range(1, 9)],
                        sort_keys=True,
                        separators=(",", ":"),
                    )
                ],
            )
            (root / "checks.csv").write_bytes(b"stale")
            # The missing output directory refuses the save before any write.
            with self.assertRaises(yamaa.Error):
                run.save()
            self.assertEqual((root / "checks.csv").read_bytes(), b"stale")
            (root / "absent").mkdir()
            entry.unlink()
            (root / "input.csv").unlink()
            run.save()
            self.assertTrue(run.issues.is_empty())
            # The ASCII canonical details remain stable independently of host JSON.
            with (root / "checks.csv").open(newline="", encoding="utf-8") as stream:
                row = next(
                    row for row in csv.DictReader(stream) if row["KIND"] == "check"
                )
            self.assertEqual(row["KEYS"], warning["KEYS"][0])
            entry.write_text(raw.split("    verifications:")[0])
            (root / "input.csv").write_text("ID\n1\n")
            unchecked = yamaa.spec(entry).run()
            self.assertEqual(
                unchecked.report["KIND"].to_list(), ["run", "input", "rows", "column"]
            )
            self.assertEqual(unchecked.report.schema["COUNT"], pl.Int64)
            self.assertEqual(unchecked.report.schema["TOTAL"], pl.Int64)
            unchecked.save()
            self.assertEqual((root / "checks.csv").read_bytes().count(b"\n"), 5)
            self.assertEqual((root / "absent/out.csv").read_bytes(), b"ID\n1\n")


if __name__ == "__main__":
    unittest.main()
