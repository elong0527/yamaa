"""Retain complete actual public producer owners for independent M1 comparison."""

import builtins
import json
import os
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from yamaa import domain

ROOT = Path(__file__).with_name("project-original")
NAME = "shared-producer-parquet"


class ProducerCohort(unittest.TestCase):
    def test_original_shared_parquet_build_and_repeated_save_without_reference_semantics(self):
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary).resolve()
            case = work / NAME
            case.mkdir()
            for name in ("seed.csv", "producer.yaml", "spec.yaml"):
                shutil.copyfile(ROOT / NAME / name, case / name)
            (work / "yamaa-project.yaml").write_text("version: '1.0'\n", encoding="ascii")
            (work / "environment.yaml").write_text(
                "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {}\n",
                encoding="ascii",
            )
            shutil.copyfile(ROOT / "schema-functions/python/uv.lock", work / "uv.lock")
            output = os.environ.get("YAMAA_PUBLIC_REPORT_DIR")
            evidence = Path(output) / "producer-cohort/python" / NAME if output else work / "evidence"
            evidence.mkdir(parents=True)
            (evidence / "case-directory.txt").write_text(case.as_posix(), encoding="utf-8")
            provenance = os.environ.get("YAMAA_SUPPLEMENTAL_EVIDENCE")
            if provenance:
                (evidence / "provenance.json").write_text(provenance, encoding="utf-8")

            original = builtins.__import__

            def reject(name, *args, **kwargs):
                if name in {"yaml", "yaml12", "pydantic", "pyarrow"} or name.startswith(
                    ("yamaa.planning", "yamaa.runtime", "yamaa.specification", "yamaa.io", "yamaa.adapters")
                ):
                    raise AssertionError("reference semantic import: " + name)
                return original(name, *args, **kwargs)

            with patch("builtins.__import__", side_effect=reject):
                result = domain(case / "spec.yaml", environment=work / "environment.yaml")
                self.assertTrue(result.issues.is_empty(), result.issues)
                self.assertEqual(result.output.to_dicts(), [{"ID": "01"}])
                before = result._native.observations()
                (evidence / "before.json").write_text(before, encoding="utf-8")
                self.assertFalse((case / "produced.parquet").exists())
                self.assertFalse((case / "out.csv").exists())
                for name in ("seed.csv", "producer.yaml", "spec.yaml"):
                    (case / name).unlink()
                saved = None
                for attempt in (1, 2):
                    self.assertTrue(result.save())
                    directory = evidence / f"after-{attempt}"
                    directory.mkdir()
                    observed = result._native.observations()
                    (directory / "report.json").write_text(observed, encoding="utf-8")
                    actual = {}
                    for name in ("produced.parquet", "out.csv"):
                        actual[name] = (case / name).read_bytes()
                        (directory / name).write_bytes(actual[name])
                    self.assertEqual(actual["out.csv"], b"ID\n01\n")
                    self.assertEqual(actual["produced.parquet"][:4], b"PAR1")
                    self.assertEqual(actual["produced.parquet"][-4:], b"PAR1")
                    if saved is not None:
                        self.assertEqual(actual, saved)
                    saved = actual
                    report = json.loads(observed)
                    self.assertEqual(report.pop("publication")["status"], "success")
                    self.assertEqual(report, json.loads(before))
                    self.assertEqual(result.output.to_dicts(), [{"ID": "01"}])


if __name__ == "__main__":
    unittest.main()
