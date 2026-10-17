"""Installed public run report, complete independent run truth and held publication."""

import builtins
import csv
import gc
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

TRUTH = load_truth(__file__, "run-report")

ROOT = Path(__file__).with_name("specification-original")
CASE = TRUTH["case_name"]
NAMES = tuple(TRUTH["artifact_names"])


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

    def retain(self, result, entry):
        destination = os.environ.get("YAMAA_PUBLIC_REPORT_DIR")
        if not destination:
            return
        report = json.loads(result._native.observations())
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

    def golden(self, case, artifact, name, extension=".csv"):
        kinds = {
            column: pl.Int64 if kind == "int" else pl.String
            for column, kind in zip(artifact["columns"], artifact["types"])
        }
        return kinds, pl.read_csv(
            case / "expected" / name.replace(extension, ".csv"),
            schema_overrides=kinds,
        )

    def test_original_full_report_typed_views_saved_goldens_and_owned_lifetime(self):
        with tempfile.TemporaryDirectory() as directory:
            case = Path(directory) / "case"
            shutil.copytree(ROOT / "cases" / CASE, case)
            entry = case / "spec.yaml"
            expected = self.expected(entry)
            result = yamaa.domain(entry)
            self.assertTrue(result.issues.is_empty())
            self.assertEqual(
                json.loads(result._native.observations()), dict(expected, artifacts=[])
            )
            for index, view in enumerate((result.report, result.output)):
                kinds, golden = self.golden(
                    case, expected["artifacts"][index], NAMES[index]
                )
                self.assertEqual(view.schema, kinds)
                self.assertTrue(view.equals(golden))
            # Each public view is a fresh host copy of privately owned bytes.
            result.report.replace_column(0, pl.Series("ARTIFACT", ["changed"] * 8))
            self.assertEqual(result.report["ARTIFACT"].to_list(), ["adsl.csv"] * 8)
            for name in NAMES:
                self.assertFalse((case / name).exists())
            entry.unlink()
            (case / "input/dm.csv").unlink()
            gc.collect()
            for _ in range(2):
                self.assertTrue(result.save())
                self.assertEqual(json.loads(result._native.observations()), expected)
                for name in NAMES:
                    self.assertEqual(
                        (case / name).read_bytes(),
                        (case / "expected" / name).read_bytes(),
                    )
            self.retain(result, entry)
            held = result.report
            del result
            gc.collect()
            self.assertEqual(held.height, 8)
            self.assertEqual(
                held.filter(pl.col("KIND") == "check")["TOTAL"].to_list(),
                [int(value) for value in TRUTH["check_totals"]],
            )

    def test_fatal_checks_publish_only_the_report_and_keep_original_issues(self):
        with tempfile.TemporaryDirectory() as directory:
            case = Path(directory) / "case"
            shutil.copytree(ROOT / "cases" / CASE, case)
            entry = case / "spec.yaml"
            entry.write_text(
                entry.read_text().replace("severity: warning", "severity: error")
            )
            result = yamaa.domain(entry)
            self.assertIsNone(result.output)
            report = result.report
            self.assertEqual(report["KIND"].to_list(), TRUTH["fatal_kinds"])
            self.assertEqual(
                report.filter(pl.col("KIND") == "check")["OUTCOME"].to_list(),
                TRUTH["fatal_outcomes"],
            )
            self.assertEqual(report.row(0, named=True)["OUTCOME"], "failed")
            self.assertEqual(report.row(0, named=True)["CONDITION"], "range_failed")
            issues = result.issues.clone()
            self.assertEqual(issues["condition"].to_list(), ["range_failed"])
            for name in NAMES:
                (case / name).write_bytes(b"sentinel")
            entry.unlink()
            (case / "input/dm.csv").unlink()
            for _ in range(2):
                with self.assertRaises(yamaa.DomainError):
                    result.save()
                self.assertTrue(result.issues.equals(issues))
                self.assertEqual((case / NAMES[1]).read_bytes(), b"sentinel")
                self.assertEqual(
                    (case / NAMES[0]).read_bytes().count(b"\n"),
                    1 + len(TRUTH["fatal_kinds"]),
                )
                self.assertEqual(
                    len(json.loads(result._native.observations())["artifacts"]), 1
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
            result = yamaa.domain(entry)
            self.assertTrue(result.issues.is_empty())
            self.assertEqual(
                json.loads(result._native.observations()), dict(expected, artifacts=[])
            )
            kinds, golden = self.golden(case, expected["artifacts"][0], NAMES[0])
            self.assertEqual(result.report.schema, kinds)
            self.assertTrue(result.report.equals(golden))
            entry.unlink()
            (case / "input/dm.csv").unlink()
            gc.collect()
            self.assertTrue(result.save())
            report = json.loads(result._native.observations())
            actual_artifacts = report.pop("artifacts")
            expected.pop("artifacts")
            self.assertEqual(report, expected)
            path = case / NAMES[0].replace(".csv", ".parquet")
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
                (case / NAMES[1]).read_bytes(),
                (case / "expected" / NAMES[1]).read_bytes(),
            )

    def test_full_warning_keys_unicode_identity_empty_types_and_target_retry(self):
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
            result = yamaa.domain(entry)
            self.assertTrue(result.issues.is_empty())
            check = result.report.filter(pl.col("KIND") == "check")
            self.assertEqual(check["ID"].to_list(), ["\u00e9\U0001d11e"])
            self.assertEqual(check["COUNT"].to_list(), [8])
            self.assertEqual(check["TOTAL"].to_list(), [8])
            self.assertEqual(
                check["KEYS"].to_list(),
                [
                    json.dumps(
                        [{"ID": i} for i in range(1, 9)],
                        sort_keys=True,
                        separators=(",", ":"),
                    )
                ],
            )
            (root / "checks.csv").write_bytes(b"stale")
            self.assertFalse(result.save())
            self.assertEqual((root / "checks.csv").read_bytes(), b"stale")
            (root / "absent").mkdir()
            entry.unlink()
            (root / "input.csv").unlink()
            self.assertTrue(result.save())
            self.assertTrue(result.issues.is_empty())
            # The ASCII canonical keys remain stable independently of host JSON.
            with (root / "checks.csv").open(newline="", encoding="utf-8") as stream:
                (row,) = [
                    row for row in csv.DictReader(stream) if row["KIND"] == "check"
                ]
            self.assertEqual(row["KEYS"], check["KEYS"][0])
            self.assertEqual(row["ID"], "\u00e9\U0001d11e")
            entry.write_text(raw.split("    verifications:")[0])
            (root / "input.csv").write_text("ID\n1\n")
            empty = yamaa.domain(entry)
            self.assertEqual(
                empty.report["KIND"].to_list(), ["run", "input", "rows", "column"]
            )
            self.assertEqual(empty.report.schema["COUNT"], pl.Int64)
            self.assertEqual(empty.report.schema["TOTAL"], pl.Int64)
            self.assertTrue(empty.save())
            self.assertEqual((root / "checks.csv").read_bytes().count(b"\n"), 5)

    def test_report_collisions_and_unknown_profiles_precede_study_access(self):
        for report, condition in (
            ("out.csv", "artifact_path_collision"),
            ("checks.arrow", "unknown_artifact_profile"),
        ):
            with (
                self.subTest(report=report),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                entry = root / "spec.yaml"
                entry.write_text(
                    "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n"
                    "input: {SRC: input.csv}\n"
                    f"output: {{path: out.csv, columns: [ID], report: {report}}}\n"
                    "columns:\n  - {name: ID, type: int, derivation: SRC.ID}\n",
                    encoding="utf-8",
                )
                # The input is absent: the refusal precedes any study access.
                result = yamaa.domain(entry)
                self.assertIsNone(result.output)
                self.assertIsNone(result.report)
                self.assertEqual(result.issues["condition"].to_list(), [condition])
                self.assertEqual(
                    result.issues["spec_paths"].to_list()[0][-1], "output.report"
                )
                with self.assertRaises(yamaa.DomainError):
                    result.save()
                for name in ("out.csv", report):
                    self.assertFalse((root / name).exists())


if __name__ == "__main__":
    unittest.main()
