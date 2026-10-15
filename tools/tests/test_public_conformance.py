"""Independent public-run truth and comparator failure-mode tests."""

import contextlib
import copy
import io
import os
import shutil
import tempfile
import unittest
from pathlib import Path

from tools.conformance import public_conformance as judge
from tools.paths import ROOT

SPEC = """schema_version: "1.0"
domain: TEST
keys: [ID]
input: {SRC: input.csv}
output: {path: result.csv, columns: [ID, VALUE]}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: VALUE, type: str, derivation: SRC.VALUE}
"""

METADATA = "lifecycle: draft\nstandard: TEST\ndomain: TEST\n"


class PublicConformance(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.fixtures = self.root / "fixtures"
        self.case = self.fixtures / "simple"
        (self.case / "expected").mkdir(parents=True)
        (self.case / "spec.yaml").write_text(SPEC)
        (self.case / "benchmark.yaml").write_text(METADATA)
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

    def test_retired_adapter_csv_mutations_cannot_pass_public_conformance(self):
        # Preserve the independent corruption controls from the retired
        # adapters/test_conformance.py: cell, header, row order and row count.
        # Mutating either saved output or expected truth must fail even when
        # the separately observed public table is otherwise unchanged.
        truth = self.case / "expected/result.csv"
        candidate = self.observed / "work/simple/result.csv"
        original = truth.read_bytes()
        records = original.splitlines(keepends=True)
        mutations = {
            "cell": original.replace(b"9223372036854775807", b"9223372036854775806"),
            "header and columns": b'VALUE,ID\n"",9223372036854775807\n,2\n"a,b",3\n',
            "row order": records[0] + b"".join(reversed(records[1:])),
            "row count": b"".join(records[:-1]),
        }
        for target in (truth, candidate):
            for name, changed in mutations.items():
                with self.subTest(target=target.name, mutation=name):
                    try:
                        target.write_bytes(changed)
                        self.assertEqual(self.verdict()["verdict"], "mismatch")
                    finally:
                        target.write_bytes(original)
        self.assertEqual(self.verdict()["verdict"], "pass")

    def test_retired_adapter_missing_or_renamed_artifact_cannot_pass(self):
        truth = self.case / "expected/result.csv"
        renamed = truth.with_name("renamed.csv")
        truth.rename(renamed)
        self.assertEqual(self.verdict()["verdict"], "mismatch")
        renamed.rename(truth)
        candidate = self.observed / "work/simple/result.csv"
        candidate.unlink()
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
        variants = output / "r-variant-inputs/simple"
        for tree in (staged, variants):
            for name, target in (
                ("linked.csv", "target.csv"),
                ("broken.csv", "absent.csv"),
                ("linked-directory", "directory"),
            ):
                self.assertTrue((tree / "input" / name).is_symlink())
                self.assertEqual(os.readlink(tree / "input" / name), target)
            self.assertEqual((tree / "input/target.csv").read_bytes(), b"ID\n1\n")
            self.assertFalse((tree / "expected").exists())
            self.assertFalse((tree / "run.py").exists())
            self.assertFalse((tree / "run.R").exists())
            self.assertEqual(
                (tree / "__child_spec.yaml").read_text(),
                'schema_version: "1.0"\nparents: spec.yaml\n',
            )
        (staged / "result.csv").write_bytes(b"original\n")
        (variants / "result.csv").write_bytes(b"variant\n")
        (variants / "input/target.csv").write_bytes(b"ID\n2\n")
        self.assertEqual((staged / "result.csv").read_bytes(), b"original\n")
        self.assertEqual((staged / "input/target.csv").read_bytes(), b"ID\n1\n")
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

    def test_missing_record_errors_and_deliberately_broken_output_fails_gate(self):
        inventory = self.root / "inventory.json"
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(judge.compare(self.fixtures, self.observed, inventory), 1)
            self.assertEqual(judge.read_json(inventory)["summary"]["error"], 1)
            judge.write_json(
                self.observed / "records/simple.json", {"builds": [self.build]}
            )
            self.assertEqual(judge.compare(self.fixtures, self.observed, inventory), 0)
            (self.observed / "work/simple/result.csv").write_text("broken\n")
            self.assertEqual(judge.compare(self.fixtures, self.observed, inventory), 1)
        self.assertEqual(judge.read_json(inventory)["gate"]["failing"], ["simple"])

    def test_incomplete_multi_entry_run_errors(self):
        self.example["entries"]["python"].append(
            {"specification": "other.yaml", "environment": None}
        )
        self.assertEqual(self.verdict()["verdict"], "error")

    def test_new_directory_is_required_on_both_hosts_without_a_registry_edit(self):
        inventory = self.root / "inventory.json"
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(judge.compare(self.fixtures, self.observed, inventory), 1)
        self.assertEqual(judge.read_json(inventory)["gate"]["failing"], ["simple"])
        metadata = judge.read_json(self.observed / "run.json")
        metadata["runtime"] = "r"
        judge.write_json(self.observed / "run.json", metadata)
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(judge.compare(self.fixtures, self.observed, inventory), 1)
        self.assertEqual(judge.read_json(inventory)["gate"]["failing"], ["simple"])

    def test_unexpected_runner_record_is_rejected(self):
        judge.write_json(self.observed / "records/unexpected.json", {})
        with self.assertRaisesRegex(ValueError, "unexpected installed runner"):
            judge.compare(self.fixtures, self.observed, self.root / "inventory.json")

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

    def test_original_parquet_primary_and_review_log_mutations_are_refused(self):
        import pyarrow as pa
        import pyarrow.parquet as pq

        # Retain the retired TestParquetGolden inputs and its AGE=99,
        # FAILURE_COUNT=2, renamed-column, float-type and empty-table probes.
        original_case = ROOT / "benchmarks/schema-parquet"
        specification = (original_case / "spec.yaml").read_text()
        (self.case / "expected/result.csv").unlink()
        originals = {}
        tables = {}
        for name in ("adsl.parquet", "adsl-violations.parquet"):
            source = original_case / "expected" / name
            originals[name] = source.read_bytes()
            tables[name] = pq.read_table(source)
            (self.case / "expected" / name).write_bytes(originals[name])
            (self.observed / "work/simple" / name).write_bytes(originals[name])

        def public_view(table):
            # Freeze the observed view from authored fixture values before
            # mutating either side; no engine produces these expectations.
            return {
                "columns": [
                    [field.name, "int" if pa.types.is_int64(field.type) else "str"]
                    for field in table.schema
                ],
                "rows": [
                    [
                        None
                        if value is None
                        else str(value)
                        if isinstance(value, int)
                        else value.encode("utf-8").hex()
                        for value in row.values()
                    ]
                    for row in table.to_pylist()
                ],
            }

        inventory = self.root / "inventory.json"

        def verdict():
            with contextlib.redirect_stdout(io.StringIO()):
                judge.compare(self.fixtures, self.observed, inventory)
            return judge.read_json(inventory)["benchmarks"][0]

        # The original benchmark uses warning_log; the same independent
        # Parquet checks must also cover the public verification_log slot.
        for log_slot in ("warning_log", "verification_log"):
            (self.case / "spec.yaml").write_text(
                specification.replace("warning_log:", f"{log_slot}:")
            )
            self.build["tables"] = {
                "output": public_view(tables["adsl.parquet"]),
                "warning_log": None,
                "verification_log": None,
            }
            self.build["tables"][log_slot] = public_view(
                tables["adsl-violations.parquet"]
            )
            judge.write_json(
                self.observed / "records/simple.json", {"builds": [self.build]}
            )
            self.assertEqual(verdict()["verdict"], "pass")
            for name, table in tables.items():
                primary = name == "adsl.parquet"
                field = "AGE" if primary else "FAILURE_COUNT"
                index = table.schema.get_field_index(field)
                renamed = "AGE" if primary else "ARTIFACT"
                mutations = {
                    "cell": table.set_column(
                        index,
                        field,
                        pa.array([99 if primary else 2] * table.num_rows, pa.int64()),
                    ),
                    "column name": table.rename_columns(
                        [
                            f"{column}_X" if column == renamed else column
                            for column in table.column_names
                        ]
                    ),
                    "logical type": table.set_column(
                        index, field, table[field].cast(pa.float64())
                    ),
                    "row count": table.slice(0, 0),
                }
                if table.num_rows > 1:
                    mutations["row order"] = table.take(
                        pa.array(list(reversed(range(table.num_rows))))
                    )
                for directory in (
                    self.case / "expected",
                    self.observed / "work/simple",
                ):
                    target = directory / name
                    for mutation, changed in mutations.items():
                        with self.subTest(
                            slot=log_slot,
                            artifact=name,
                            target=directory,
                            mutation=mutation,
                        ):
                            try:
                                pq.write_table(changed, target)
                                result = verdict()
                                malformed_truth = (
                                    directory == self.case / "expected"
                                    and (
                                        mutation == "logical type"
                                        or (primary and mutation == "column name")
                                    )
                                )
                                self.assertEqual(
                                    result["verdict"],
                                    "error" if malformed_truth else "mismatch",
                                )
                                self.assertTrue(result["findings"])
                            finally:
                                target.write_bytes(originals[name])
                    removed = target.with_name("removed-" + target.name)
                    with self.subTest(
                        slot=log_slot,
                        artifact=name,
                        target=directory,
                        mutation="renamed artifact",
                    ):
                        try:
                            target.rename(removed)
                            self.assertEqual(verdict()["verdict"], "mismatch")
                        finally:
                            removed.rename(target)
            self.assertEqual(verdict()["verdict"], "pass")

    def test_parquet_verification_log_row_order_is_compared(self):
        import pyarrow as pa
        import pyarrow.parquet as pq

        (self.case / "spec.yaml").write_text(
            SPEC.replace(
                "path: result.csv,", "path: result.csv, verification_log: log.parquet,"
            )
        )
        table = pa.table(
            {
                "FAILURE_COUNT": pa.array([1, 2], pa.int64()),
                "ARTIFACT": ["first.csv", "second.csv"],
            }
        )
        self.build["tables"]["verification_log"] = {
            "columns": [["FAILURE_COUNT", "int"], ["ARTIFACT", "str"]],
            "rows": [["1", "66697273742e637376"], ["2", "7365636f6e642e637376"]],
        }
        truth = self.case / "expected/log.parquet"
        candidate = self.observed / "work/simple/log.parquet"
        pq.write_table(table, truth)
        pq.write_table(table, candidate)
        self.assertEqual(self.verdict()["verdict"], "pass")
        original = truth.read_bytes()
        for target in (truth, candidate):
            try:
                pq.write_table(table.take(pa.array([1, 0])), target)
                self.assertEqual(self.verdict()["verdict"], "mismatch")
            finally:
                target.write_bytes(original)
        self.assertEqual(self.verdict()["verdict"], "pass")

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

    def test_metadata_entries_and_host_environments_are_preserved(self):
        for name in ("spec_one.yaml", "spec_two.yaml"):
            (self.case / name).write_text(SPEC)
        for host in ("python", "r"):
            (self.case / host).mkdir()
            (self.case / host / "environment.yaml").write_text("host fixture")
        judge.write_json(
            self.case / "benchmark.yaml",
            {
                "lifecycle": "draft",
                "standard": "TEST",
                "domain": "TEST",
                "entries": {
                    host: [
                        {
                            "specification": "spec_one.yaml",
                            "environment": f"{host}/environment.yaml",
                        },
                        {"specification": "spec_two.yaml", "environment": None},
                    ]
                    for host in ("python", "r")
                },
            },
        )
        entries = judge.plan(self.fixtures)["examples"][0]["entries"]
        self.assertEqual(
            entries["python"],
            [
                {
                    "specification": "spec_one.yaml",
                    "environment": "python/environment.yaml",
                },
                {"specification": "spec_two.yaml", "environment": None},
            ],
        )
        self.assertEqual(entries["r"][0]["environment"], "r/environment.yaml")
        (self.case / "benchmark.yaml").write_text(METADATA)
        with self.assertRaisesRegex(ValueError, "explicit entries"):
            judge.plan(self.fixtures)

    def test_negative_host_environment_and_no_automatic_root_environment(self):
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
        judge.write_json(
            self.case / "benchmark.yaml",
            {
                "lifecycle": "draft",
                "standard": "TEST",
                "domain": "TEST",
                "conformance": {
                    "metamorphic": {
                        "empty_child": {
                            "issue": "https://github.com/elong0527/yamaa/issues/2190",
                        }
                    }
                },
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
        (case / "benchmark.yaml").write_text(METADATA)
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
        fixtures = ROOT / "benchmarks"
        corpus = judge.plan(fixtures)
        names = {e["name"]: e for e in corpus["examples"]}
        # Every authored directory is planned; no count to edit per addition.
        self.assertEqual(
            set(names),
            {
                p.name
                for p in fixtures.iterdir()
                if p.is_dir() and not p.name.startswith(".")
            },
        )
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
            {n for n, e in names.items() if e["excluded"]},
            {"negative-path-absolute", "negative-path-parent-escape"},
        )

    def test_corpus_metamorphic_plan_and_expectations_follow_each_entry(self):
        corpus = judge.plan(ROOT / "benchmarks")
        pairs = 0
        for example in corpus["examples"]:
            variants = example["variants"]["python"]
            self.assertEqual(variants, example["variants"]["r"])
            entries = 0 if example["excluded"] else len(example["entries"]["python"])
            kinds = {}
            for variant in variants:
                kinds.setdefault(variant["entry"], []).append(variant["kind"])
            self.assertEqual(set(kinds), set(range(entries)), example["name"])
            # One empty child per entry; a key-order-eligible entry adds one
            # control/permuted pair. Derived per entry, so adding a benchmark
            # needs no count edit here.
            for entry_kinds in kinds.values():
                self.assertIn(
                    sorted(entry_kinds),
                    (
                        ["empty_child"],
                        ["empty_child", "key_order_control", "key_order_permuted"],
                    ),
                    example["name"],
                )
                pairs += "key_order_control" in entry_kinds
        self.assertTrue(pairs, "no benchmark entry exercises the key-order check")
        expectations = judge.load_metamorphic_expectations(ROOT / "benchmarks")
        names = {e["name"] for e in corpus["examples"]}
        # #2190 fixes the 40 missing-handler empty children. The reviewed
        # forward-reference ordering exception remains permanent.
        self.assertEqual(len(expectations), 1)
        self.assertTrue(
            expectations[("negative-forward-reference", "empty_child")]["permanent"]
        )
        for (benchmark, check), item in expectations.items():
            self.assertIn(benchmark, names)
            self.assertIn(check, {"empty_child", "key_order"})
            self.assertTrue(item.get("issue") or item.get("permanent"))


if __name__ == "__main__":
    unittest.main()
