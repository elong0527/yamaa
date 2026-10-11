"""Installed public logs, complete independent run truth and held publication."""

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
import yamaa
from yamaa import _native

ROOT = Path(__file__).with_name("specification-original")
CASE = "schema-verification-log"
NAMES = ("adsl-checks.csv", "adsl-warnings.csv", "adsl.csv")


class VerificationLogs(unittest.TestCase):
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
            engine_version=_native.engine_info()["core_version"],
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
            for index, view in enumerate(
                (result.verification_log, result.warning_log, result.output)
            ):
                artifact = expected["artifacts"][index]
                kinds = {
                    column: pl.Int64 if kind == "int" else pl.String
                    for column, kind in zip(artifact["columns"], artifact["types"])
                }
                golden = pl.read_csv(
                    case / "expected" / NAMES[index], schema_overrides=kinds
                )
                self.assertEqual(view.schema, kinds)
                self.assertTrue(view.equals(golden))
            # Each public view is a fresh host copy of privately owned bytes.
            result.verification_log.replace_column(
                0, pl.Series("REPORT_VERSION", ["changed"] * 4)
            )
            result.warning_log.replace_column(0, pl.Series("LOG_VERSION", ["changed"]))
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
            held = result.verification_log
            del result
            gc.collect()
            self.assertEqual(held.height, 4)
            self.assertEqual(held["EVALUATED_COUNT"].to_list(), [3, 3, 3, 1])

    def test_fatal_checks_publish_only_verification_and_keep_original_issues(self):
        with tempfile.TemporaryDirectory() as directory:
            case = Path(directory) / "case"
            shutil.copytree(ROOT / "cases" / CASE, case)
            entry = case / "spec.yaml"
            entry.write_text(
                entry.read_text().replace("severity: warning", "severity: error")
            )
            result = yamaa.domain(entry)
            self.assertIsNone(result.output)
            self.assertIsNone(result.warning_log)
            self.assertEqual(
                result.verification_log["OUTCOME"].to_list(), ["held", "violated"]
            )
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
                self.assertEqual((case / NAMES[2]).read_bytes(), b"sentinel")
                self.assertEqual(
                    len(json.loads(result._native.observations())["artifacts"]), 1
                )

    def test_parquet_logs_decode_to_independent_csv_goldens(self):
        with tempfile.TemporaryDirectory() as directory:
            case = Path(directory) / "case"
            shutil.copytree(ROOT / "cases" / CASE, case)
            entry = case / "spec.yaml"
            entry.write_text(
                entry.read_text()
                .replace("adsl-checks.csv", "adsl-checks.parquet")
                .replace("adsl-warnings.csv", "adsl-warnings.parquet")
            )
            expected = self.expected(entry)
            result = yamaa.domain(entry)
            self.assertTrue(result.issues.is_empty())
            self.assertEqual(
                json.loads(result._native.observations()), dict(expected, artifacts=[])
            )
            goldens = []
            for index, view in enumerate((result.verification_log, result.warning_log)):
                artifact = expected["artifacts"][index]
                kinds = {
                    column: pl.Int64 if kind == "int" else pl.String
                    for column, kind in zip(artifact["columns"], artifact["types"])
                }
                golden = pl.read_csv(
                    case / "expected" / NAMES[index], schema_overrides=kinds
                )
                self.assertEqual(view.schema, kinds)
                self.assertTrue(view.equals(golden))
                goldens.append(golden)
            entry.unlink()
            (case / "input/dm.csv").unlink()
            gc.collect()
            self.assertTrue(result.save())
            report = json.loads(result._native.observations())
            actual_artifacts = report.pop("artifacts")
            expected.pop("artifacts")
            self.assertEqual(report, expected)
            for index, golden in enumerate(goldens):
                path = case / NAMES[index].replace(".csv", ".parquet")
                actual = pl.read_parquet(path)
                self.assertEqual(actual.schema, golden.schema)
                self.assertTrue(actual.equals(golden))
                artifact = actual_artifacts[index]
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
                (case / NAMES[2]).read_bytes(),
                (case / "expected" / NAMES[2]).read_bytes(),
            )

    def test_full_warning_keys_unicode_identity_empty_types_and_target_retry(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            entry = root / "spec.yaml"
            raw = """schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: input.csv}
output: {path: absent/out.csv, columns: [ID], warning_log: warnings.csv, verification_log: checks.csv}
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
            warning = result.warning_log
            self.assertEqual(warning["VERIFICATION_ID"].to_list(), ["\u00e9\U0001d11e"])
            self.assertEqual(warning["FAILURE_COUNT"].to_list(), [8])
            self.assertEqual(
                warning["OFFENDING_KEYS"].to_list(),
                [
                    json.dumps(
                        [{"ID": i} for i in range(1, 9)],
                        sort_keys=True,
                        separators=(",", ":"),
                    )
                ],
            )
            for name in ("checks.csv", "warnings.csv"):
                (root / name).write_bytes(b"stale")
            self.assertFalse(result.save())
            for name in ("checks.csv", "warnings.csv"):
                self.assertEqual((root / name).read_bytes(), b"stale")
            (root / "absent").mkdir()
            entry.unlink()
            (root / "input.csv").unlink()
            self.assertTrue(result.save())
            self.assertTrue(result.issues.is_empty())
            # The ASCII canonical details remain stable independently of host JSON.
            with (root / "warnings.csv").open(newline="") as stream:
                row = next(csv.DictReader(stream))
            self.assertEqual(row["OFFENDING_KEYS"], warning["OFFENDING_KEYS"][0])
            entry.write_text(raw.split("    verifications:")[0])
            (root / "input.csv").write_text("ID\n1\n")
            empty = yamaa.domain(entry)
            self.assertEqual(empty.verification_log.height, 0)
            self.assertEqual(empty.warning_log.height, 0)
            self.assertEqual(empty.verification_log.schema["EVALUATED_COUNT"], pl.Int64)
            self.assertEqual(empty.warning_log.schema["FAILURE_COUNT"], pl.Int64)
            self.assertTrue(empty.save())
            self.assertEqual((root / "checks.csv").read_bytes().count(b"\n"), 1)
            self.assertEqual((root / "warnings.csv").read_bytes().count(b"\n"), 1)


if __name__ == "__main__":
    unittest.main()
