"""Independent public-run truth and comparator failure-mode tests."""

import contextlib
import importlib.util
import io
import os
import shutil
import tempfile
import unittest
from pathlib import Path

SPEC = """schema_version: "1.0"
domain: TEST
keys: [ID]
input: {SRC: input.csv}
output: {path: result.csv, columns: [ID, VALUE]}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: VALUE, type: str, derivation: SRC.VALUE}
"""
ROOT = Path(__file__).parents[1]
spec = importlib.util.spec_from_file_location(
    "public_conformance", ROOT / "tools/public_conformance.py"
)
judge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(judge)


class PublicConformance(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.fixtures = self.root / "fixtures"
        self.case = self.fixtures / "simple"
        (self.case / "expected").mkdir(parents=True)
        (self.case / "spec.yaml").write_text(SPEC)
        (self.case / "run.py").write_text(
            'import yamaa\nx = yamaa.yamaa_domain("spec.yaml").output\n'
        )
        (self.case / "run.R").write_text('x <- yamaa_domain("spec.yaml")$output\n')
        (self.case / "expected/result.csv").write_bytes(
            b'ID,VALUE\n9223372036854775807,""\n2,\n3,"a,b"\n'
        )
        self.observed = self.root / "observed"
        candidate = self.observed / "work/simple/result.csv"
        candidate.parent.mkdir(parents=True)
        shutil.copyfile(self.case / "expected/result.csv", candidate)
        judge.write_json(
            self.observed / "run.json",
            {
                "protocol": judge.PROTOCOL,
                "runtime": "python",
                "source_revision": "test",
            },
        )
        # Written independently rather than produced by an engine or oracle helper.
        self.build = {
            "specification": "spec.yaml",
            "environment": None,
            "issues": [],
            "saved": True,
            "tables": {
                "output": {
                    "columns": [["ID", "int"], ["VALUE", "str"]],
                    "rows": [["9223372036854775807", ""], ["2", None], ["3", "612c62"]],
                },
                "verification_log": None,
                "warning_log": None,
            },
        }
        self.example = judge.plan(self.fixtures)["examples"][0]

    def verdict(self):
        return judge.compare_example(
            self.example,
            "python",
            {"builds": [self.build]},
            self.fixtures,
            self.observed,
        )

    def test_exact_saved_bytes_and_lossless_typed_public_view(self):
        self.assertEqual(self.verdict()["verdict"], "pass")
        self.build["tables"]["output"]["columns"][0][1] = "float"
        self.assertEqual(self.verdict()["verdict"], "mismatch")

    def test_r_input_staging_preserves_native_links_and_excludes_expected_truth(self):
        inputs = self.case / "input"
        inputs.mkdir()
        (inputs / "target.csv").write_bytes(b"ID\n1\n")
        (inputs / "directory").mkdir()
        os.symlink("target.csv", inputs / "linked.csv")
        os.symlink("absent.csv", inputs / "broken.csv")
        os.symlink("directory", inputs / "linked-directory", target_is_directory=True)
        output = self.root / "plan"
        with contextlib.redirect_stdout(io.StringIO()):
            judge.emit_plan(self.fixtures, output)
        staged = output / "r-inputs/simple"
        for name, target in (("linked.csv", "target.csv"), ("broken.csv", "absent.csv"), ("linked-directory", "directory")):
            self.assertTrue((staged / "input" / name).is_symlink())
            self.assertEqual(os.readlink(staged / "input" / name), target)
        self.assertEqual((staged / "input/target.csv").read_bytes(), b"ID\n1\n")
        self.assertFalse((staged / "expected").exists())
        self.assertFalse((staged / "run.py").exists())
        self.assertFalse((staged / "run.R").exists())
        moved = self.root / "execution/simple"
        moved.parent.mkdir()
        staged.rename(moved)
        self.assertTrue((moved / "input/linked.csv").is_symlink())
        self.assertEqual(os.readlink(moved / "input/linked.csv"), "target.csv")

    def test_quoted_empty_is_distinct_from_missing_and_csv_bytes_are_exact(self):
        self.build["tables"]["output"]["rows"][0][1] = None
        self.assertEqual(self.verdict()["verdict"], "mismatch")
        self.build["tables"]["output"]["rows"][0][1] = ""
        (self.observed / "work/simple/result.csv").write_bytes(
            b'ID,VALUE\r\n9223372036854775807,""\r\n2,\r\n3,"a,b"\r\n'
        )
        self.assertEqual(self.verdict()["verdict"], "mismatch")

    def test_negative_uses_first_issue_and_context_subset_without_reference(self):
        (self.case / "expected/error.yaml").write_text(
            "phase: derivation\ncondition: test_failure\nspec_paths: [columns.VALUE]\nrequirement: REQ-test\ncontext: {keys: [{ID: 2}]}\n"
        )
        issue = {
            "phase": "derivation",
            "condition": "test_failure",
            "spec_paths": ["columns.VALUE"],
            "requirement": "REQ-test",
            "context": {"keys": [{"ID": 2}], "extra": "retained"},
        }
        self.build.update(issues=[issue], saved=False, tables={"output": None})
        self.assertEqual(self.verdict()["verdict"], "pass")
        issue["context"]["keys"][0]["ID"] = 3
        self.assertEqual(self.verdict()["verdict"], "mismatch")
        self.build["issues"].insert(0, {**issue, "condition": "earlier_failure"})
        self.assertEqual(self.verdict()["verdict"], "mismatch")

    def test_unsupported_and_engine_error_can_never_pass_negative_truth(self):
        (self.case / "expected/error.yaml").write_text(
            "phase: planning\ncondition: unsupported_operation\nspec_paths: [columns.VALUE]\n"
        )
        self.build.update(
            issues=[
                {
                    "phase": "planning",
                    "condition": "unsupported_operation",
                    "spec_paths": ["columns.VALUE"],
                    "context": {"operation": "mapping"},
                }
            ],
            saved=False,
            tables={"output": None},
        )
        verdict = self.verdict()
        self.assertEqual(verdict["verdict"], "unsupported")
        self.assertEqual(verdict["unsupported_operations"], ["mapping"])
        self.build["issues"].append(
            {"phase": "engine", "condition": "engine_rejected", "context": {}}
        )
        self.assertEqual(self.verdict()["verdict"], "error")

    def test_undeclared_or_uncompared_table_cannot_pass(self):
        self.build["tables"]["warning_log"] = {"columns": [], "rows": []}
        self.assertEqual(self.verdict()["verdict"], "mismatch")
        self.build["tables"]["warning_log"] = None
        (self.case / "expected/forgotten.csv").write_text("ID\n1\n")
        self.assertEqual(self.verdict()["verdict"], "mismatch")

    def test_define_and_resolved_spec_have_explicit_dispositions(self):
        for name in ["define.xml", "dm.json", "spec_resolved.yaml"]:
            (self.case / "expected" / name).write_text("independent truth")
        verdict = self.verdict()
        self.assertEqual(verdict["verdict"], "pass")
        self.assertEqual(
            {r["artifact"] for r in verdict["not_compared"]},
            {"define.xml", "dm.json", "spec_resolved.yaml"},
        )

    def test_missing_record_errors_and_deliberately_broken_output_fails_ratchet(self):
        ratchet = self.root / "ratchet.json"
        judge.write_json(ratchet, {"python": ["simple"], "r": []})
        inventory = self.root / "inventory.json"
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(judge.compare(self.fixtures, self.observed, inventory), 0)
            self.assertEqual(judge.read_json(inventory)["summary"]["error"], 1)
            judge.write_json(
                self.observed / "records/simple.json", {"builds": [self.build]}
            )
            self.assertEqual(
                judge.compare(self.fixtures, self.observed, inventory, ratchet), 0
            )
            (self.observed / "work/simple/result.csv").write_text("broken\n")
            self.assertEqual(
                judge.compare(self.fixtures, self.observed, inventory, ratchet), 1
            )
        self.assertEqual(
            judge.read_json(inventory)["ratchet"]["regressions"], ["simple"]
        )

    def test_incomplete_multi_entry_run_errors(self):
        self.example["entries"]["python"].append(
            {"specification": "other.yaml", "environment": None}
        )
        self.assertEqual(self.verdict()["verdict"], "error")

    def test_log_types_are_independent_and_saved_logs_are_compared(self):
        with (self.case / "spec.yaml").open("a") as stream:
            stream.write("  # unused comment\n")
        (self.case / "spec.yaml").write_text(
            SPEC.replace(
                "path: result.csv,", "path: result.csv, warning_log: warnings.csv,"
            )
        )
        truth = b"FAILURE_COUNT,CONDITION\n1,test_warning\n"
        (self.case / "expected/warnings.csv").write_bytes(truth)
        (self.observed / "work/simple/warnings.csv").write_bytes(truth)
        self.build["tables"]["warning_log"] = {
            "columns": [["FAILURE_COUNT", "int"], ["CONDITION", "str"]],
            "rows": [["1", "746573745f7761726e696e67"]],
        }
        self.assertEqual(self.verdict()["verdict"], "pass")
        self.build["tables"]["warning_log"]["columns"][0][1] = "str"
        self.assertEqual(self.verdict()["verdict"], "mismatch")

    def test_parquet_logical_comparison_ignores_metadata_and_serialization(self):
        import pyarrow as pa
        import pyarrow.parquet as pq

        (self.case / "spec.yaml").write_text(
            SPEC.replace("result.csv", "result.parquet")
        )
        (self.case / "expected/result.csv").unlink()
        table = pa.table(
            {
                "ID": pa.array([9223372036854775807, 2, 3], type=pa.int64()),
                "VALUE": ["", None, "a,b"],
            }
        )
        truth = self.case / "expected/result.parquet"
        candidate = self.observed / "work/simple/result.parquet"
        pq.write_table(table, truth, compression="none")
        pq.write_table(
            table.replace_schema_metadata({b"producer": b"independent"}),
            candidate,
            compression="snappy",
        )
        self.assertNotEqual(truth.read_bytes(), candidate.read_bytes())
        self.assertEqual(self.verdict()["verdict"], "pass")
        pq.write_table(table.slice(1), candidate)
        self.assertEqual(self.verdict()["verdict"], "mismatch")
        pq.write_table(
            table.set_column(0, "ID", pa.array([1, 2, 3], type=pa.int32())), candidate
        )
        self.assertEqual(self.verdict()["verdict"], "mismatch")

    def test_declared_reported_precision_rounds_exactly_without_tolerance(self):
        self.assertEqual(
            judge.reported_table(
                {"columns": [["N", "float"]], "rows": [["4001000000000000"]]}, 2
            ),
            {"columns": [["N", "float"]], "rows": [["40010a3d70a3d70a"]]},
        )
        self.assertEqual(
            judge.reported_table(
                {"columns": [["N", "float"]], "rows": [["c001000000000000"]]}, 2
            ),
            {"columns": [["N", "float"]], "rows": [["c0010a3d70a3d70a"]]},
        )
        self.assertEqual(
            judge.reported_table(
                {"columns": [["N", "float"]], "rows": [["bfa999999999999a"]]}, 0
            ),
            {"columns": [["N", "float"]], "rows": [["0000000000000000"]]},
        )

    def test_literal_entries_and_host_environments_are_preserved(self):
        (self.case / "run.py").write_text(
            'a = yamaa.domain("one.yaml", environment="python/environment.yaml").output\nb = yamaa.domain("two.yaml").output\n'
        )
        (self.case / "run.R").write_text(
            'a <- yamaa_domain("one.yaml", environment="r/environment.yaml")$output\nb <- yamaa_domain("two.yaml")$output\n'
        )
        entries = judge.plan(self.fixtures)["examples"][0]["entries"]
        self.assertEqual(
            entries["python"],
            [
                {"specification": "one.yaml", "environment": "python/environment.yaml"},
                {"specification": "two.yaml", "environment": None},
            ],
        )
        self.assertEqual(entries["r"][0]["environment"], "r/environment.yaml")
        (self.case / "run.py").write_text("a = yamaa.domain(variable).output")
        with self.assertRaises(ValueError):
            judge.plan(self.fixtures)

    def test_negative_host_environment_and_no_automatic_root_environment(self):
        (self.case / "run.py").unlink()
        (self.case / "run.R").unlink()
        (self.case / "environment.yaml").write_text("define fixture")
        (self.case / "python").mkdir()
        (self.case / "python/environment.yaml").write_text("host fixture")
        entries = judge.plan(self.fixtures)["examples"][0]["entries"]
        self.assertEqual(entries["python"][0]["environment"], "python/environment.yaml")
        self.assertIsNone(entries["r"][0]["environment"])

    def test_corpus_inventory_has_all_entries_and_approved_exclusions(self):
        corpus = judge.plan(ROOT.parent / "benchmarks")
        names = {e["name"]: e for e in corpus["examples"]}
        self.assertEqual(len(names), 318)
        self.assertEqual(len(names["sdtm-dm-race-ethnicity"]["entries"]["python"]), 2)
        self.assertEqual(
            names["schema-inheritance"]["entries"]["r"][0]["specification"],
            "spec_study.yaml",
        )
        self.assertIsNone(
            names["sdtm-dm-metadata"]["entries"]["python"][0]["environment"]
        )
        self.assertIsNone(names["schema-inheritance-project-root"]["excluded"])
        self.assertEqual(
            {n for n, e in names.items() if e["excluded"]}, set(judge.EXCLUDED)
        )


if __name__ == "__main__":
    unittest.main()
