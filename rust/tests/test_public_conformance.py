"""Independent public-run truth and comparator failure-mode tests."""

import contextlib
import copy
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
        for name, target in (
            ("linked.csv", "target.csv"),
            ("broken.csv", "absent.csv"),
            ("linked-directory", "directory"),
        ):
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

    def test_new_directory_is_required_on_both_hosts_without_a_registry_edit(self):
        ratchet = self.root / "ratchet.json"
        judge.write_json(ratchet, {"python": [], "r": []})
        inventory = self.root / "inventory.json"
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(
                judge.compare(self.fixtures, self.observed, inventory, ratchet), 1
            )
        self.assertEqual(
            judge.read_json(inventory)["ratchet"]["regressions"], ["simple"]
        )
        metadata = judge.read_json(self.observed / "run.json")
        metadata["runtime"] = "r"
        judge.write_json(self.observed / "run.json", metadata)
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(
                judge.compare(self.fixtures, self.observed, inventory, ratchet), 1
            )
        self.assertEqual(
            judge.read_json(inventory)["ratchet"]["regressions"], ["simple"]
        )

    def test_unexpected_runner_record_and_duplicate_observations_are_rejected(self):
        judge.write_json(self.observed / "records/unexpected.json", {})
        with self.assertRaisesRegex(ValueError, "unexpected installed runner"):
            judge.compare(self.fixtures, self.observed, self.root / "inventory.json")
        (self.observed / "records/unexpected.json").unlink()
        ratchet = self.root / "ratchet.json"
        judge.write_json(ratchet, {"python": ["simple", "simple"]})
        with self.assertRaisesRegex(ValueError, "duplicate observed"):
            judge.compare(
                self.fixtures, self.observed, self.root / "inventory.json", ratchet
            )

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

    def child_variant(self, **overrides):
        variant = copy.deepcopy(self.build)
        variant.update(
            kind="empty_child",
            entry=0,
            specification="__child_spec.yaml",
        )
        variant.update(overrides)
        return variant

    def verdict_with_variants(self, variants):
        return judge.compare_example(
            self.example,
            "python",
            {"builds": [self.build], "variants": variants},
            self.fixtures,
            self.observed,
        )

    def test_metamorphic_empty_child_must_reproduce_its_parent(self):
        self.assertEqual(
            self.example["variants"]["python"],
            [{"kind": "empty_child", "entry": 0, "specification": "__child_spec.yaml"}],
        )
        verdict = self.verdict_with_variants([self.child_variant()])
        self.assertEqual(verdict["verdict"], "pass")
        self.assertTrue(verdict["metamorphic"]["judged"])
        changed = self.child_variant()
        changed["tables"]["output"]["rows"][0][0] = "1"
        verdict = self.verdict_with_variants([changed])
        self.assertEqual(verdict["verdict"], "mismatch")
        self.assertEqual(len(verdict["metamorphic"]["differences"]), 1)
        self.assertEqual(
            verdict["metamorphic"]["differences"][0]["check"], "empty_child"
        )
        # A record predating the metamorphic checks is reported, not judged.
        self.assertFalse(self.verdict()["metamorphic"]["judged"])
        self.assertEqual(self.verdict()["verdict"], "pass")

    def test_metamorphic_expected_failure_is_listed_and_goes_stale(self):
        path = (
            self.root / "rust" / "qualification" / "public-conformance-metamorphic.json"
        )
        judge.write_json(
            path,
            {
                "protocol": judge.METAMORPHIC_PROTOCOL,
                "expected_failures": [
                    {
                        "benchmark": "simple",
                        "check": "empty_child",
                        "issue": "https://github.com/elong0527/yamaa/issues/2190",
                    }
                ],
            },
        )
        failing = self.child_variant(
            issues=[
                {
                    "phase": "validation",
                    "condition": "invalid_clear",
                    "requirement": "REQ-0660",
                    "spec_paths": ["intermediates.X.no_match"],
                    "context": {},
                }
            ],
            saved=False,
            tables={"output": None, "verification_log": None, "warning_log": None},
        )
        verdict = self.verdict_with_variants([failing])
        self.assertEqual(verdict["verdict"], "pass")
        self.assertEqual(len(verdict["metamorphic"]["expected_failures"]), 1)
        verdict = self.verdict_with_variants([self.child_variant()])
        self.assertEqual(verdict["verdict"], "mismatch")
        self.assertEqual(len(verdict["metamorphic"]["stale"]), 1)

    def test_key_order_texts_move_key_columns_to_the_front_reversed(self):
        document = {
            "keys": ["ID", "VALUE"],
            "columns": [
                {"name": "OTHER", "derivation": "SRC.OTHER"},
                {"name": "ID", "derivation": "SRC.ID"},
                {"name": "VALUE", "derivation": {"literal": "x"}},
            ],
        }
        control, permuted = judge.key_order_texts(document)
        self.assertEqual(
            [c["name"] for c in judge.json.loads(control)["columns"]],
            ["OTHER", "ID", "VALUE"],
        )
        self.assertEqual(
            [c["name"] for c in judge.json.loads(permuted)["columns"]],
            ["VALUE", "ID", "OTHER"],
        )
        for ineligible in (
            {**document, "rows": []},
            {**document, "parents": ["base.yaml"]},
            {**document, "keys": ["ID"]},
            {
                **document,
                "columns": [
                    {"name": "ID", "derivation": {"compute": {"expr": "1"}}},
                    {"name": "VALUE", "derivation": "SRC.VALUE"},
                ],
            },
        ):
            self.assertIsNone(judge.key_order_texts(ineligible))

    def test_metamorphic_key_order_permutation_compares_against_control(self):
        fixtures = self.root / "fixtures2"
        case = fixtures / "ordered"
        (case / "expected").mkdir(parents=True)
        (case / "spec.yaml").write_text(SPEC.replace("keys: [ID]", "keys: [ID, VALUE]"))
        (case / "run.py").write_text(
            'import yamaa\nx = yamaa.yamaa_domain("spec.yaml").output\n'
        )
        (case / "run.R").write_text('x <- yamaa_domain("spec.yaml")$output\n')
        (case / "expected/result.csv").write_bytes(
            b'ID,VALUE\n9223372036854775807,""\n2,\n3,"a,b"\n'
        )
        observed = self.root / "observed2"
        candidate = observed / "work/ordered/result.csv"
        candidate.parent.mkdir(parents=True)
        shutil.copyfile(case / "expected/result.csv", candidate)
        example = judge.plan(fixtures)["examples"][0]
        kinds = [v["kind"] for v in example["variants"]["python"]]
        self.assertEqual(
            kinds, ["empty_child", "key_order_control", "key_order_permuted"]
        )

        def variant(kind, specification):
            build = copy.deepcopy(self.build)
            build.update(kind=kind, entry=0, specification=specification)
            return build

        variants = [
            variant("empty_child", "__child_spec.yaml"),
            variant("key_order_control", "__keyorder_control_spec.yaml"),
            variant("key_order_permuted", "__keyorder_permuted_spec.yaml"),
        ]
        record = {"builds": [self.build], "variants": variants}
        verdict = judge.compare_example(
            example, "python", record, fixtures, observed, {}
        )
        self.assertEqual(verdict["verdict"], "pass")
        variants[2]["tables"]["output"]["rows"][0][0] = "1"
        verdict = judge.compare_example(
            example, "python", record, fixtures, observed, {}
        )
        self.assertEqual(verdict["verdict"], "mismatch")
        self.assertEqual(verdict["metamorphic"]["differences"][0]["check"], "key_order")

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

    def test_corpus_metamorphic_plan_and_expectations_match_the_reviewed_counts(self):
        corpus = judge.plan(ROOT.parent / "benchmarks")
        kinds = {"empty_child": 0, "key_order_control": 0, "key_order_permuted": 0}
        for example in corpus["examples"]:
            self.assertEqual(example["variants"]["python"], example["variants"]["r"])
            for variant in example["variants"]["python"]:
                kinds[variant["kind"]] += 1
        # The counts reviewed in #2291: one empty child per active entry, and
        # 99 key-order-eligible positive entries.
        self.assertEqual(
            kinds,
            {"empty_child": 317, "key_order_control": 99, "key_order_permuted": 99},
        )
        expectations = judge.load_metamorphic_expectations(ROOT.parent / "benchmarks")
        names = {e["name"] for e in corpus["examples"]}
        self.assertEqual(len(expectations), 47)
        for (benchmark, check), item in expectations.items():
            self.assertIn(benchmark, names)
            self.assertIn(check, {"empty_child", "key_order"})
            self.assertTrue(item.get("issue") or item.get("permanent"))


if __name__ == "__main__":
    unittest.main()
