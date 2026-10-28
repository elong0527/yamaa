"""Independent public-run truth and comparator failure-mode tests."""

import contextlib
import copy
import functools
import importlib.util
import io
import os
import shutil
import struct
import tempfile
import unittest
from pathlib import Path

from tools.conformance import public_conformance as judge
from tools.paths import ROOT
from tools.tests.support import write

SPEC = """yamaa: "1.0"
type: spec
domain: TEST
keys: [ID]
input: {SRC: input.csv}
output: {path: result.csv, columns: [ID, VALUE]}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: VALUE, type: str, derivation: SRC.VALUE}
"""

METADATA = "lifecycle: draft\nstandard: TEST\ndomain: TEST\n"


@functools.cache
def corpus_plan():
    """The plan of this repository's benchmarks, made once for the tests that
    read it; none of them changes it."""
    return judge.plan(ROOT / "benchmarks")


def installed_runner():
    """The installed Python conformance runner, loaded from its file; it
    imports the package only when it runs."""
    spec = importlib.util.spec_from_file_location(
        "installed_public_conformance",
        ROOT / "python/tests/installed/public_conformance.py",
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def repeated(build):
    """The build as the runners record it: its object compiled a plan, and its
    second run reused it, equals the first and changed no saved file (#2821)."""
    repeat = copy.deepcopy(
        {key: build[key] for key in ("issues", "tables", "saved", "paths")}
    )
    return {
        **build,
        "plan": "compiled",
        "repeat": {**repeat, "plan": "reused", "changed_files": []},
    }


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
            # save() returns the declared file's absolute path (D38), which
            # the runners record relative to the case's root.
            "paths": ["result.csv"],
            "tables": {
                "output": {
                    "columns": [["ID", "int"], ["VALUE", "str"]],
                    "rows": [["9223372036854775807", ""], ["2", None], ["3", "612c62"]],
                },
                "report": None,
            },
        }
        self.example = judge.plan(self.fixtures)["examples"][0]

    def saved(self, *names):
        """The build's save wrote `names` in this order beside the candidate."""
        self.build["paths"] = list(names)

    def verdict(self):
        return judge.compare_example(
            self.example,
            "python",
            {"builds": [repeated(self.build)]},
            self.fixtures,
            self.observed,
        )

    def test_exact_saved_bytes_and_lossless_typed_public_view(self):
        self.assertEqual(self.verdict()["verdict"], "pass")
        self.build["tables"]["output"]["columns"][0][1] = "float"
        self.assertEqual(self.verdict()["verdict"], "mismatch")

    def test_a_second_run_must_reuse_the_plan_and_equal_the_first(self):
        def verdict(build):
            return judge.compare_example(
                self.example,
                "python",
                {"builds": [build]},
                self.fixtures,
                self.observed,
            )

        self.assertEqual(verdict(repeated(self.build))["verdict"], "pass")
        missing = verdict(self.build)
        self.assertEqual(missing["verdict"], "mismatch")
        self.assertIn("spec.yaml was not run a second time", missing["findings"])
        # A refused compilation keeps nothing: the second run is refused too.
        refused = repeated(self.build)
        refused.update(plan="refused")
        refused["repeat"].update(plan="refused")
        self.assertEqual(verdict(refused)["verdict"], "pass")
        for first, second in (("compiled", "compiled"), ("refused", "reused")):
            with self.subTest(first=first, second=second):
                build = repeated(self.build)
                build.update(plan=first)
                build["repeat"].update(plan=second)
                result = verdict(build)
                self.assertEqual(result["verdict"], "mismatch")
                self.assertIn(
                    {
                        "repeat": "spec.yaml",
                        "first_run_plan": first,
                        "second_run_plan": second,
                        "expected_second_run_plan": judge.REPEATED_PLAN[first],
                    },
                    result["findings"],
                )
        changes = {
            "issues": (
                lambda repeat: repeat["issues"].append(
                    {"condition": "division_by_zero"}
                ),
                "issues[0]",
                None,
                {"condition": "division_by_zero"},
            ),
            "tables": (
                lambda repeat: repeat["tables"]["output"]["rows"][1].__setitem__(
                    0, "7"
                ),
                "tables.output.rows[1][0]",
                "2",
                "7",
            ),
            "saved": (lambda repeat: repeat.update(saved=False), "saved", True, False),
            "paths": (
                lambda repeat: repeat["paths"].append("/elsewhere.csv"),
                "paths[1]",
                None,
                "/elsewhere.csv",
            ),
        }
        for field, (change, at, first, second) in changes.items():
            with self.subTest(field=field):
                build = repeated(self.build)
                change(build["repeat"])
                result = verdict(build)
                self.assertEqual(result["verdict"], "mismatch")
                self.assertIn(
                    {
                        "repeat": "spec.yaml",
                        "differs_from_first_run": field,
                        "at": at,
                        "first_run": first,
                        "second_run": second,
                    },
                    result["findings"],
                )
        build = repeated(self.build)
        build["repeat"]["changed_files"] = ["result.csv"]
        result = verdict(build)
        self.assertEqual(result["verdict"], "mismatch")
        self.assertIn(
            {
                "repeat": "spec.yaml",
                "differs_from_first_run": "saved files",
                "changed_files": ["result.csv"],
            },
            result["findings"],
        )

    def test_a_successful_save_returns_the_authored_paths_in_order(self):
        path = self.build["paths"][0]
        # A record keeps no machine's root, so the absolute path that save()
        # returned is refused on every lane, as is any other spelling.
        absolute = (self.observed / "work/simple/result.csv").resolve().as_posix()
        for paths in (
            None,
            [],
            [path, path],
            ["other.csv"],
            [absolute],
            [absolute.replace("/", "\\")],
            ["./result.csv"],
            ["Result.csv"],
        ):
            with self.subTest(paths=paths):
                self.build["paths"] = paths
                result = self.verdict()
                self.assertEqual(result["verdict"], "mismatch")
                self.assertIn(
                    {"saved_paths": "spec.yaml", "expected": [path], "actual": paths},
                    result["findings"],
                )

    def test_saved_paths_are_declared_beside_the_entry_from_the_case_root(self):
        # An entry below the case's root declares its files beside itself;
        # a parent segment resolves as text, as the engine's canonical path
        # does. The report comes first.
        self.assertEqual(
            judge.declared_paths(
                "specs/spec.yaml",
                {"report": "checks.csv", "path": "../out/result.csv"},
            ),
            ["specs/checks.csv", "out/result.csv"],
        )
        self.assertEqual(judge.declared_paths("spec.yaml", {}), [])

    def test_the_python_runner_names_a_listed_layer_by_the_domain_it_inherits(self):
        domain_of = installed_runner().domain_of
        (self.case / "spec_child.yaml").write_text(
            'yamaa: "1.0"\ntype: spec\nparent: spec.yaml\n'
        )
        self.assertEqual(domain_of(self.case / "spec.yaml"), "TEST")
        self.assertEqual(domain_of(self.case / "spec_child.yaml"), "TEST")
        # The layer nearest the entry that writes the domain supplies it.
        (self.case / "spec_child.yaml").write_text(
            'yamaa: "1.0"\ntype: spec\nparent: spec.yaml\ndomain: CHILD\n'
        )
        self.assertEqual(domain_of(self.case / "spec_child.yaml"), "CHILD")

    def test_the_python_runner_stages_parent_locks_from_their_declaring_layers(self):
        # Seam: inject the installation's lock before engine admission, keeping
        # authored locks and leaving malformed parent chains for the engine.
        lock = self.root / "installed.lock"
        lock.write_bytes(b"installed lock\n")
        stage_lock = installed_runner().stage_lock
        for label, files, expected in (
            (
                "inherited",
                {
                    "package/project.yaml": "parent: ../study/project.yaml\n",
                    "study/project.yaml": "parent: ../program/project.yaml\n",
                    "program/project.yaml": "environments: {python: {lock: locks/uv.lock}}\n",
                },
                {"program/locks/uv.lock": b"installed lock\n"},
            ),
            (
                "overridden",
                {
                    "package/project.yaml": "parent: ../study/project.yaml\nenvironments: {python: {lock: uv.lock}}\n",
                    "study/project.yaml": "environments: {python: {lock: uv.lock}}\n",
                    "study/uv.lock": "authored lock\n",
                },
                {
                    "package/uv.lock": b"installed lock\n",
                    "study/uv.lock": b"authored lock\n",
                },
            ),
            (
                "cycle",
                {
                    "package/project.yaml": "parent: ../study/project.yaml\n",
                    "study/project.yaml": "parent: ../package/project.yaml\nenvironments: {python: {lock: uv.lock}}\n",
                },
                {"study/uv.lock": b"installed lock\n"},
            ),
            ("missing", {"package/project.yaml": "parent: missing.yaml\n"}, {}),
            (
                "malformed",
                {
                    "package/project.yaml": "parent: broken.yaml\n",
                    "package/broken.yaml": "[",
                },
                {},
            ),
            (
                "two-parents",
                {"package/project.yaml": "parent: [one.yaml, two.yaml]\n"},
                {},
            ),
        ):
            with self.subTest(case=label):
                case = self.case / label
                for name, text in files.items():
                    write(case, name, text)
                stage_lock(case, "package/project.yaml", lock)
                actual = {
                    path.relative_to(case).as_posix(): path.read_bytes()
                    for path in case.rglob("*.lock")
                }
                self.assertEqual(actual, expected)

    def test_the_python_runner_records_saved_paths_below_the_case_root(self):
        relative = installed_runner().relative
        case = (self.observed / "work/simple").resolve()
        root = case.as_posix()
        self.assertEqual(
            relative([f"{root}/checks.csv", f"{root}/out/result.csv"], case),
            ["checks.csv", "out/result.csv"],
        )
        self.assertIsNone(relative(None, case))
        self.assertEqual(relative([], case), [])
        # A path outside the case, or with a backslash, stays as returned.
        outside = [
            f"{root}2/result.csv",
            f"{case.parent.as_posix()}/result.csv",
            f"{root}/out\\result.csv",
        ]
        self.assertEqual(relative(outside, case), outside)

    def test_the_python_runner_names_its_output_through_a_path_that_reaches_it(self):
        # --output-as names the output directory through another path to it,
        # as a drive mapped to a network share reaches the share (#3030 D15);
        # a linked directory stands in for one here. The path stays as
        # written, never resolved to the directory that it reaches, and the
        # record already written in the output must read back through it.
        named_output = installed_runner().named_output
        output = self.root / "output"
        output.mkdir()
        (output / "run.json").write_text('{"protocol": "public-conformance/1"}\n')
        alias = self.root / "alias"
        os.symlink(output, alias, target_is_directory=True)
        self.assertEqual(named_output(output, None), output)
        self.assertEqual(named_output(output, alias), alias)
        elsewhere = self.root / "elsewhere"
        elsewhere.mkdir()
        (elsewhere / "run.json").write_text('{"protocol": "another run"}\n')
        for written in (elsewhere, self.root / "absent"):
            with (
                self.subTest(written=written),
                self.assertRaisesRegex(
                    ValueError, f"--output-as {written} does not reach --output"
                ),
            ):
                named_output(output, written)

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
            self.assertFalse((tree / "benchmark.yaml").exists())
            self.assertFalse((tree / "run.py").exists())
            self.assertFalse((tree / "run.R").exists())
            self.assertEqual(
                (tree / "__child_spec.yaml").read_text(),
                'yamaa: "1.0"\ntype: spec\nparent: spec.yaml\n',
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

    def test_an_empty_child_declares_the_1_x_header(self):
        """An empty child layer declares the 1.x header (LAYER-4)."""
        self.assertEqual(
            judge.empty_child("spec.yaml"),
            'yamaa: "1.0"\ntype: spec\nparent: spec.yaml\n',
        )

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
            "phase: derivation\ncondition: test_failure\nspec_paths: [columns.VALUE]\nrule: REQ-test\ncontext: {keys: [{ID: 2}]}\n"
        )
        issue = {
            "phase": "derivation",
            "condition": "test_failure",
            "spec_paths": ["columns.VALUE"],
            "rule": "REQ-test",
            "context": {"keys": [{"ID": 2}], "extra": "retained"},
        }
        self.build.update(issues=[issue], saved=False, tables={"output": None})
        self.assertEqual(self.verdict()["verdict"], "pass")
        issue["context"]["keys"][0]["ID"] = 3
        self.assertEqual(self.verdict()["verdict"], "mismatch")
        self.build["issues"].insert(0, {**issue, "condition": "earlier_failure"})
        self.assertEqual(self.verdict()["verdict"], "mismatch")

    def test_validation_truth_requires_every_path_and_allows_independent_findings(self):
        (self.case / "expected/error.yaml").write_text(
            "phase: validation\ncondition: dependency_cycle\n"
            "spec_paths: [columns.A.derivation, columns.B.derivation]\n"
            "rule: COLUMN-6\ncontext: {cycle: [A, B, A]}\n"
        )
        cycle = {
            "phase": "validation",
            "condition": "dependency_cycle",
            "spec_paths": ["columns.A.derivation", "columns.B.derivation"],
            "rule": "COLUMN-6",
            "context": {"cycle": ["A", "B", "A"]},
        }
        # Original c7b88a4f reports this independent forward reference beside
        # the cycle in negative-adlb-computed-param. A partial error contract
        # cannot stand in for exhaustive independently authored report truth.
        independent = {
            "phase": "validation",
            "condition": "forward_reference",
            "spec_paths": ["columns.C.derivation"],
            "rule": "COLUMN-5",
            "context": {"column": "C", "dependency": "B"},
        }
        self.build.update(
            issues=[cycle, independent], saved=False, tables={"output": None}
        )
        self.assertEqual(self.verdict()["verdict"], "pass")
        cycle["spec_paths"].pop()
        result = self.verdict()
        self.assertEqual(result["verdict"], "mismatch")
        self.assertIn(
            {"missing_validation_paths": ["columns.B.derivation"]},
            result["findings"],
        )

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

    def test_undeclared_report_is_held_but_uncompared_truth_cannot_pass(self):
        # Every run that began holds its report; without output.report there
        # is no independent truth to compare it with.
        self.build["tables"]["report"] = {
            "columns": [["KIND", "str"], ["COUNT", "int"]],
            "rows": [["72756e", "3"]],
        }
        self.assertEqual(self.verdict()["verdict"], "pass")
        (self.case / "expected/forgotten.csv").write_text("ID\n1\n")
        self.assertEqual(self.verdict()["verdict"], "mismatch")

    def test_define_and_resolved_spec_have_explicit_dispositions(self):
        (self.case / "submission-project.yaml").write_text(
            'yamaa: "1.0"\ntype: project\n'
        )
        (self.case / "benchmark.yaml").write_text(
            METADATA
            + "define:\n  environment: submission-project.yaml\n  standard: sdtm\n"
            "  created: '2026-09-11T00:00:00'\n"
        )
        self.example = judge.plan(self.fixtures)["examples"][0]
        for name in [
            "define.xml",
            "dm.json",
            "dm.ndjson",
            "dm.xpt",
            "spec_resolved.yaml",
        ]:
            (self.case / "expected" / name).write_text("independent truth")
        verdict = self.verdict()
        self.assertEqual(verdict["verdict"], "pass")
        self.assertEqual(
            {r["artifact"] for r in verdict["not_compared"]},
            {"define.xml", "dm.json", "dm.ndjson", "dm.xpt", "spec_resolved.yaml"},
        )
        # A Define benchmark's expected failure belongs to its Define call: the
        # domain build must still pass and save, and the error is not compared.
        (self.case / "expected/error.yaml").write_text(
            "phase: validation\ncondition: value_not_permitted\n"
            "spec_paths: [sdtm.define]\n"
        )
        verdict = self.verdict()
        self.assertEqual(verdict["verdict"], "pass", verdict)
        self.assertIn("error.yaml", {r["artifact"] for r in verdict["not_compared"]})
        self.build["issues"] = [
            {"phase": "validation", "condition": "value_not_permitted"}
        ]
        self.assertEqual(self.verdict()["verdict"], "mismatch")

    def test_define_files_without_a_define_call_are_not_dispositioned(self):
        (self.case / "expected/define.xml").write_text("independent truth")
        verdict = self.verdict()
        self.assertEqual(verdict["verdict"], "pass")
        self.assertEqual(verdict["not_compared"], [])

    def test_missing_record_errors_and_deliberately_broken_output_fails_gate(self):
        inventory = self.root / "inventory.json"
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(judge.compare(self.fixtures, self.observed, inventory), 1)
            self.assertEqual(judge.read_json(inventory)["summary"]["error"], 1)
            judge.write_json(
                self.observed / "records/simple.json",
                {"builds": [repeated(self.build)]},
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

    def test_report_types_are_independent_and_saved_reports_are_compared(self):
        (self.case / "spec.yaml").write_text(
            SPEC.replace("path: result.csv,", "path: result.csv, report: checks.csv,")
        )
        self.saved("checks.csv", "result.csv")
        truth = b"KIND,COUNT,TOTAL,DETAILS\nrun,3,,{}\n"
        (self.case / "expected/checks.csv").write_bytes(truth)
        (self.observed / "work/simple/checks.csv").write_bytes(truth)
        self.build["tables"]["report"] = {
            "columns": [
                ["KIND", "str"],
                ["COUNT", "int"],
                ["TOTAL", "int"],
                ["DETAILS", "str"],
            ],
            "rows": [["72756e", "3", None, "7b7d"]],
        }
        self.assertEqual(self.verdict()["verdict"], "pass")
        self.build["tables"]["report"]["columns"][1][1] = "str"
        self.assertEqual(self.verdict()["verdict"], "mismatch")
        self.build["tables"]["report"]["columns"][1][1] = "int"
        (self.observed / "work/simple/checks.csv").write_bytes(
            b"KIND,COUNT,TOTAL,DETAILS\nrun,4,,{}\n"
        )
        self.assertEqual(self.verdict()["verdict"], "mismatch")
        (self.observed / "work/simple/checks.csv").unlink()
        self.assertEqual(self.verdict()["verdict"], "mismatch")

    def test_r_plain_doubles_must_hold_each_declared_integer_exactly(self):
        truth = b'ID,VALUE\n9007199254740992,""\n2,\n3,"a,b"\n'
        (self.case / "expected/result.csv").write_bytes(truth)
        (self.observed / "work/simple/result.csv").write_bytes(truth)

        def double(value):
            return struct.pack(">d", value).hex()

        output = self.build["tables"]["output"]
        output["columns"][0][1] = "float"
        output["rows"] = [
            [double(2.0**53), ""],
            [double(2.0), None],
            [double(3.0), "612c62"],
        ]

        def verdict(runtime):
            record = {"builds": [repeated(self.build)]}
            return judge.compare_example(
                self.example, runtime, record, self.fixtures, self.observed
            )["verdict"]

        self.assertEqual(verdict("r"), "pass")
        # Python keeps integers as integers; a float view is a type mismatch.
        self.assertEqual(verdict("python"), "mismatch")
        output["rows"][1][0] = double(2.5)
        self.assertEqual(verdict("r"), "mismatch")
        output["rows"][1][0] = double(2.0)
        output["rows"][0][0] = double(2.0**53 + 2)
        self.assertEqual(verdict("r"), "mismatch")

    def test_parquet_logical_comparison_ignores_metadata_and_serialization(self):
        import pyarrow as pa
        import pyarrow.parquet as pq

        (self.case / "spec.yaml").write_text(
            SPEC.replace("result.csv", "result.parquet")
        )
        self.saved("result.parquet")
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

    def test_original_parquet_primary_and_report_mutations_are_refused(self):
        import pyarrow as pa
        import pyarrow.parquet as pq

        # Retain the retired TestParquetGolden primary and its AGE=99,
        # renamed-column, float-type and empty-table probes, and apply the
        # same probes, COUNT=2 included, to an independently written report.
        primary_truth = ROOT / "benchmarks/schema-parquet/expected/adsl.parquet"
        (self.case / "spec.yaml").write_text(
            'yamaa: "1.0"\ntype: spec\n'
            "domain: ADSL\n"
            "keys: [STUDYID, USUBJID]\n"
            "input: {DM: input/dm.parquet}\n"
            "output: {path: adsl.parquet, columns: [STUDYID, USUBJID, AGE],"
            " report: adsl-checks.parquet}\n"
            "columns:\n"
            "  - {name: STUDYID, type: str, derivation: DM.STUDYID}\n"
            "  - {name: USUBJID, type: str, derivation: DM.USUBJID}\n"
            "  - {name: AGE, type: int, derivation: DM.AGE}\n"
        )
        self.saved("adsl-checks.parquet", "adsl.parquet")
        (self.case / "expected/result.csv").unlink()
        report = pa.table(
            {
                "ARTIFACT": ["adsl.parquet", "adsl.parquet"],
                "KIND": ["run", "check"],
                "SPEC_PATH": ["output", "columns.AGE.verifications[0].range"],
                "COUNT": pa.array([2, 1], pa.int64()),
                "TOTAL": pa.array([None, 2], pa.int64()),
            }
        )
        sink = pa.BufferOutputStream()
        pq.write_table(report, sink)
        originals = {
            "adsl.parquet": primary_truth.read_bytes(),
            "adsl-checks.parquet": sink.getvalue().to_pybytes(),
        }
        tables = {}
        for name, payload in originals.items():
            (self.case / "expected" / name).write_bytes(payload)
            (self.observed / "work/simple" / name).write_bytes(payload)
            tables[name] = pq.read_table(self.case / "expected" / name)

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

        self.build["tables"] = {
            "output": public_view(tables["adsl.parquet"]),
            "report": public_view(tables["adsl-checks.parquet"]),
        }
        judge.write_json(
            self.observed / "records/simple.json", {"builds": [repeated(self.build)]}
        )
        self.assertEqual(verdict()["verdict"], "pass")
        for name, table in tables.items():
            primary = name == "adsl.parquet"
            field = "AGE" if primary else "COUNT"
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
                "row order": table.take(
                    pa.array(list(reversed(range(table.num_rows))))
                ),
            }
            for directory in (self.case / "expected", self.observed / "work/simple"):
                target = directory / name
                for mutation, changed in mutations.items():
                    with self.subTest(
                        artifact=name, target=directory, mutation=mutation
                    ):
                        try:
                            pq.write_table(changed, target)
                            result = verdict()
                            malformed_truth = directory == self.case / "expected" and (
                                mutation == "logical type"
                                or (primary and mutation == "column name")
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
                    artifact=name, target=directory, mutation="renamed artifact"
                ):
                    try:
                        target.rename(removed)
                        self.assertEqual(verdict()["verdict"], "mismatch")
                    finally:
                        removed.rename(target)
        self.assertEqual(verdict()["verdict"], "pass")

    def test_parquet_report_row_order_is_compared(self):
        import pyarrow as pa
        import pyarrow.parquet as pq

        (self.case / "spec.yaml").write_text(
            SPEC.replace(
                "path: result.csv,", "path: result.csv, report: report.parquet,"
            )
        )
        self.saved("report.parquet", "result.csv")
        table = pa.table(
            {
                "COUNT": pa.array([1, 2], pa.int64()),
                "ARTIFACT": ["first.csv", "second.csv"],
            }
        )
        self.build["tables"]["report"] = {
            "columns": [["COUNT", "int"], ["ARTIFACT", "str"]],
            "rows": [["1", "66697273742e637376"], ["2", "7365636f6e642e637376"]],
        }
        truth = self.case / "expected/report.parquet"
        candidate = self.observed / "work/simple/report.parquet"
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
        # 2.675 is stored 1.8e-14 below the scaled tie, inside the 2^-26
        # interval, so it reports 2.68; 1.004999999 is 1.0e-7 below and
        # reports 1.00.
        self.assertEqual(
            judge.reported_table(
                {
                    "columns": [["N", "float"]],
                    "rows": [["4005666666666666"], ["3ff0147ae102f5e5"]],
                },
                2,
            ),
            {
                "columns": [["N", "float"]],
                "rows": [["400570a3d70a3d71"], ["3ff0000000000000"]],
            },
        )

    def test_metadata_entries_and_explicit_projects_are_preserved(self):
        for name in ("spec_one.yaml", "spec_two.yaml"):
            (self.case / name).write_text(SPEC)
        for host in ("python", "r"):
            (self.case / host).mkdir()
            (self.case / host / "project.yaml").write_text("project fixture")
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
                            "environment": f"{host}/project.yaml",
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
                    "environment": "python/project.yaml",
                },
                {"specification": "spec_two.yaml", "environment": None},
            ],
        )
        self.assertEqual(entries["r"][0]["environment"], "r/project.yaml")
        (self.case / "benchmark.yaml").write_text(METADATA)
        with self.assertRaisesRegex(ValueError, "explicit entries"):
            judge.plan(self.fixtures)

    def test_root_project_is_the_only_automatic_argument_for_both_hosts(self):
        (self.case / "environment.yaml").write_text("retired root environment")
        (self.case / "python").mkdir()
        (self.case / "python/environment.yaml").write_text("host fixture")
        entries = judge.plan(self.fixtures)["examples"][0]["entries"]
        self.assertIsNone(entries["python"][0]["environment"])
        self.assertIsNone(entries["r"][0]["environment"])
        (self.case / "project.yaml").write_text("project fixture")
        entries = judge.plan(self.fixtures)["examples"][0]["entries"]
        self.assertEqual(entries["python"][0]["environment"], "project.yaml")
        self.assertEqual(entries["r"][0]["environment"], "project.yaml")

    def command_line(self):
        return judge.plan(self.fixtures)["examples"][0]["cli"]

    def test_command_line_runs_each_call_and_names_a_project_spec(self):
        self.assertEqual(
            self.example["cli"],
            {
                "excluded": None,
                "calls": [
                    {
                        "specification": "spec.yaml",
                        "environment": None,
                        "arguments": ["run", "spec.yaml"],
                    }
                ],
            },
        )
        # With a project, the call names its specification by `domain` (D68).
        (self.case / "project.yaml").write_text(
            'yamaa: "1.0"\ntype: project\nsdtm:\n  specs: [spec.yaml]\n'
        )
        self.assertEqual(
            self.command_line()["calls"][0]["arguments"],
            ["run", "project.yaml", "--spec", "TEST"],
        )
        # A specification that names no domain gives its call no command line.
        (self.case / "spec.yaml").write_text(SPEC.replace("domain: TEST\n", ""))
        self.assertIsNone(self.command_line()["calls"][0]["arguments"])
        # A layer the project lists is named by the domain it inherits.
        (self.case / "spec.yaml").write_text(SPEC)
        (self.case / "spec_child.yaml").write_text(
            'yamaa: "1.0"\ntype: spec\nparent: spec.yaml\n'
        )
        (self.case / "project.yaml").write_text(
            'yamaa: "1.0"\ntype: project\nsdtm:\n  specs: [spec_child.yaml]\n'
        )
        (self.case / "benchmark.yaml").write_text(
            METADATA
            + "entries:\n"
            + "".join(
                f"  {host}:\n  - specification: spec_child.yaml\n"
                for host in ("python", "r")
            )
        )
        self.assertEqual(
            self.command_line()["calls"][0]["arguments"],
            ["run", "project.yaml", "--spec", "TEST"],
        )

    def test_command_line_excludes_benchmarks_whose_project_supplies_functions(self):
        project = self.case / "project.yaml"
        # A project lists the function files that declare the functions (D8).
        project.write_text(
            'yamaa: "1.0"\ntype: project\nfunctions: [functions/bmi.yaml]\n'
        )
        self.assertEqual(
            self.command_line(),
            {"excluded": judge.CLI_FUNCTION_EXCLUSION, "calls": []},
        )
        # A project's environments hold only locks, and an unreadable
        # project supplies nothing; the hosts report why it cannot be read.
        for text in (
            'yamaa: "1.0"\ntype: project\nenvironments:\n  python: {lock: uv.lock}\n',
            "sdtm: [unclosed\n",
        ):
            with self.subTest(text=text):
                project.write_text(text)
                self.assertIsNone(self.command_line()["excluded"])
        # A benchmark's own exclusion holds for the command line too.
        (self.case / "benchmark.yaml").write_text(
            METADATA + "conformance:\n  excluded: a reviewed reason\n"
        )
        self.assertEqual(
            self.command_line(), {"excluded": "a reviewed reason", "calls": []}
        )

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
            {"builds": [repeated(self.build)], "variants": variants},
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
                    "rule": "LAYER-9",
                    "spec_paths": ["intermediates.X.absent"],
                    "context": {},
                }
            ],
            saved=False,
            tables={"output": None, "report": None},
        )
        verdict = self.verdict_with_variants([failing])
        self.assertEqual(verdict["verdict"], "pass")
        self.assertEqual(len(verdict["metamorphic"]["expected_failures"]), 1)
        verdict = self.verdict_with_variants([self.child_variant()])
        self.assertEqual(verdict["verdict"], "mismatch")
        self.assertEqual(len(verdict["metamorphic"]["stale"]), 1)

    def test_key_order_texts_move_key_columns_to_the_front_reversed(self):
        document = {
            "yamaa": "1.0",
            "type": "spec",
            "keys": ["ID", "VALUE"],
            "columns": [
                {"name": "OTHER", "derivation": "SRC.OTHER"},
                {"name": "ID", "derivation": "SRC.ID"},
                {"name": "VALUE", "derivation": "'x'"},
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
        # A null parent names no parent, so the document stays standalone.
        self.assertIsNotNone(judge.key_order_texts({**document, "parent": None}))
        for ineligible in (
            {**document, "rows": []},
            {**document, "parent": "base.yaml"},
            {**document, "include": ["fragment.yaml"]},
            {**document, "keys": ["ID"]},
            {
                **document,
                "columns": [
                    {"name": "ID", "derivation": "SRC.ID + 1"},
                    {"name": "VALUE", "derivation": "SRC.VALUE"},
                ],
            },
        ):
            self.assertIsNone(judge.key_order_texts(ineligible))

    def test_key_order_texts_read_a_1_x_document_in_its_language(self):
        """A 1.x key column reads one qualified field, or is a literal written
        as quoted text or a YAML scalar (EXPR-5); the texts keep the header."""
        document = {
            "yamaa": "1.0",
            "type": "spec",
            "keys": ["DOMAIN", "ID", "SEQ", "FLAG"],
            "columns": [
                {"name": "OTHER", "derivation": "SRC.OTHER"},
                {"name": "DOMAIN", "derivation": "'DM'"},
                {"name": "ID", "derivation": "SRC.ID"},
                {"name": "SEQ", "derivation": 1},
                {"name": "FLAG", "derivation": None},
            ],
        }
        control, permuted = judge.key_order_texts(document)
        self.assertEqual(judge.json.loads(control)["yamaa"], "1.0")
        self.assertEqual(
            [c["name"] for c in judge.json.loads(permuted)["columns"]],
            ["FLAG", "SEQ", "ID", "DOMAIN", "OTHER"],
        )
        for derivation in ("ID", "SRC.ID + 1", "UPPER(SRC.ID)", {"literal": "x"}):
            with self.subTest(derivation=derivation):
                ineligible = copy.deepcopy(document)
                ineligible["columns"][2]["derivation"] = derivation
                self.assertIsNone(judge.key_order_texts(ineligible), derivation)

    def test_declarations_follow_the_single_parent_chain(self):
        (self.case / "layers").mkdir()
        (self.case / "layers/base.yaml").write_text(SPEC)
        (self.case / "middle.yaml").write_text(
            'yamaa: "1.0"\ntype: spec\nparent: layers/base.yaml\n'
            "columns:\n  - {name: VALUE, type: int}\n"
        )
        (self.case / "entry.yaml").write_text(
            'yamaa: "1.0"\ntype: spec\nparent: middle.yaml\noutput: {decimals: 2}\n'
        )
        types, output = judge.declarations(self.case / "entry.yaml")
        self.assertEqual(types, {"ID": "int", "VALUE": "int"})
        self.assertEqual((output["path"], output["decimals"]), ("result.csv", 2))
        (self.case / "layers/base.yaml").write_text(
            'yamaa: "1.0"\ntype: spec\nparent: ../entry.yaml\n'
        )
        with self.assertRaisesRegex(ValueError, "cyclic declarations"):
            judge.declarations(self.case / "entry.yaml")

    def test_declarations_read_fragments_after_the_parent_chain(self):
        """A layer's fragments contribute after its parent chain and before
        its own fields, in list order (LAYER-3)."""
        (self.case / "base.yaml").write_text(SPEC)
        (self.case / "first.yaml").write_text(
            'yamaa: "1.0"\ntype: spec\ncolumns:\n  - {name: VALUE, type: int}\n'
        )
        (self.case / "second.yaml").write_text(
            'yamaa: "1.0"\ntype: spec\ncolumns:\n  - {name: VALUE, type: float}\n'
        )
        (self.case / "entry.yaml").write_text(
            'yamaa: "1.0"\ntype: spec\nparent: base.yaml\n'
            "include: [first.yaml, second.yaml]\noutput: {decimals: 2}\n"
        )
        layers = [path.name for path, _ in judge.chain(self.case / "entry.yaml")]
        self.assertEqual(
            layers, ["base.yaml", "first.yaml", "second.yaml", "entry.yaml"]
        )
        types, output = judge.declarations(self.case / "entry.yaml")
        self.assertEqual(types, {"ID": "int", "VALUE": "float"})
        self.assertEqual(output["decimals"], 2)

    def test_a_1_x_child_clears_an_inherited_output_field(self):
        """A 1.x child removes an inherited field by listing it in `clear`
        (LAYER-9); its own value applies after the clear."""
        (self.case / "base.yaml").write_text(
            'yamaa: "1.0"\ntype: spec\ndomain: TEST\nkeys: [ID]\n'
            "output: {path: result.csv, report: checks.csv, decimals: 2}\n"
            "columns:\n  - {name: ID, type: int}\n"
        )
        (self.case / "entry.yaml").write_text(
            'yamaa: "1.0"\ntype: spec\nparent: base.yaml\n'
            "clear: [output.report, output.decimals, columns.ID.label]\n"
            "output: {decimals: 3}\n"
        )
        types, output = judge.declarations(self.case / "entry.yaml")
        self.assertEqual(types, {"ID": "int"})
        self.assertEqual(output, {"path": "result.csv", "decimals": 3})
        self.assertEqual(judge.declared_paths("entry.yaml", output), ["result.csv"])

    def test_a_copy_entry_declares_the_types_its_producer_declares(self):
        """A 1.x copy entry declares each column it lists with its producer's
        type, read through the input's `schema` link from the layer that
        declares it, and from a producer that copies in turn (SPEC-9)."""
        (self.case / "producers").mkdir()
        (self.case / "producers/dm.yaml").write_text(
            'yamaa: "1.0"\ntype: spec\ndomain: DM\nkeys: [ID]\n'
            "output: {path: dm.csv, columns: [ID, AGE]}\n"
            "columns:\n  - {name: ID, type: int}\n  - {name: AGE, type: float}\n"
        )
        (self.case / "producers/adsl.yaml").write_text(
            'yamaa: "1.0"\ntype: spec\ndomain: ADSL\nkeys: [ID]\n'
            "input: {DM: {path: dm.csv, schema: dm.yaml}}\n"
            "output: {path: adsl.csv, columns: [ID, AGE]}\n"
            "columns:\n  - copy: {dataset: DM, columns: [ID, AGE]}\n"
        )
        (self.case / "base.yaml").write_text(
            'yamaa: "1.0"\ntype: spec\ndomain: ADAE\nkeys: [ID]\n'
            "input: {ADSL: {path: adsl.csv, schema: producers/adsl.yaml}}\n"
            "output: {path: adae.csv, columns: [ID, AGE, SEQ]}\n"
            "columns:\n  - copy: {dataset: ADSL, columns: [ID, AGE]}\n"
        )
        (self.case / "entry.yaml").write_text(
            'yamaa: "1.0"\ntype: spec\nparent: base.yaml\n'
            "columns:\n  - {name: SEQ, type: int}\n"
        )
        types, _ = judge.declarations(self.case / "entry.yaml")
        self.assertEqual(types, {"ID": "int", "AGE": "float", "SEQ": "int"})

    def test_an_inherited_copy_reads_the_link_the_resolved_chain_keeps(self):
        """A parent's copy entry takes its types from the producer that the
        input's final `schema` link names, from the base its final
        `relative_to` selects, whichever layer writes them (SPEC-9)."""
        producer = (
            'yamaa: "1.0"\ntype: spec\ndomain: ADSL\nkeys: [ID]\n'
            "output: {{path: adsl.csv, columns: [ID, AGE]}}\n"
            "columns:\n  - {{name: ID, type: int}}\n  - {{name: AGE, type: {age}}}\n"
        )
        (self.case / "layers/producers").mkdir(parents=True)
        (self.case / "producers").mkdir()
        (self.case / "old.yaml").write_text(producer.format(age="int"))
        (self.case / "new.yaml").write_text(producer.format(age="float"))
        (self.case / "layers/producers/adsl.yaml").write_text(
            producer.format(age="int")
        )
        (self.case / "producers/adsl.yaml").write_text(producer.format(age="float"))
        base = (
            'yamaa: "1.0"\ntype: spec\ndomain: ADAE\nkeys: [ID]\n'
            "input: {{ADSL: {link}}}\n"
            "output: {{path: adae.csv, columns: [ID, AGE]}}\n"
            "columns:\n  - {{name: ID, type: int}}\n"
            "  - copy: {{dataset: ADSL, columns: [AGE]}}\n"
        )
        child = 'yamaa: "1.0"\ntype: spec\nparent: {parent}\ninput: {{ADSL: {link}}}\n'
        for parent, inherited, written in (
            # The child replaces the inherited link.
            ("base.yaml", "{path: adsl.csv, schema: old.yaml}", "{schema: new.yaml}"),
            # Only the child links the input.
            ("base.yaml", "{path: adsl.csv}", "{schema: new.yaml}"),
            # The child resolves the inherited link from the entry.
            (
                "layers/base.yaml",
                "{path: adsl.csv, schema: producers/adsl.yaml}",
                "{relative_to: entry}",
            ),
        ):
            with self.subTest(parent=parent, written=written):
                (self.case / parent).write_text(base.format(link=inherited))
                (self.case / "entry.yaml").write_text(
                    child.format(parent=parent, link=written)
                )
                types, _ = judge.declarations(self.case / "entry.yaml")
                self.assertEqual(types, {"ID": "int", "AGE": "float"})

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
        record = {"builds": [repeated(self.build)], "variants": variants}
        verdict = judge.compare_example(
            example, "python", record, fixtures, observed, {}
        )
        self.assertEqual(verdict["verdict"], "pass")
        # Check rows follow column declaration order (CHECK-23): permuting
        # the key columns moves their rows, which compare as a multiset.
        report = {
            "columns": [["SPEC_PATH", "str"], ["COUNT", "int"]],
            "rows": [["ID", "0"], ["VALUE", "0"]],
        }
        for build in (self.build, *variants):
            build["tables"]["report"] = copy.deepcopy(report)
        variants[2]["tables"]["report"]["rows"].reverse()
        record = {"builds": [repeated(self.build)], "variants": variants}
        verdict = judge.compare_example(
            example, "python", record, fixtures, observed, {}
        )
        self.assertEqual(verdict["verdict"], "pass")
        variants[2]["tables"]["report"]["rows"][0][1] = "1"
        verdict = judge.compare_example(
            example, "python", record, fixtures, observed, {}
        )
        self.assertEqual(verdict["verdict"], "mismatch")
        self.assertEqual(verdict["metamorphic"]["differences"][0]["check"], "key_order")
        # An empty child composes the same declarations in the same order.
        variants[2]["tables"]["report"] = copy.deepcopy(report)
        variants[0]["tables"]["report"]["rows"].reverse()
        verdict = judge.compare_example(
            example, "python", record, fixtures, observed, {}
        )
        self.assertEqual(verdict["verdict"], "mismatch")
        self.assertEqual(
            verdict["metamorphic"]["differences"][0]["check"], "empty_child"
        )
        variants[0]["tables"]["report"] = copy.deepcopy(report)
        variants[2]["tables"]["output"]["rows"][0][0] = "1"
        verdict = judge.compare_example(
            example, "python", record, fixtures, observed, {}
        )
        self.assertEqual(verdict["verdict"], "mismatch")
        self.assertEqual(verdict["metamorphic"]["differences"][0]["check"], "key_order")

    def test_corpus_inventory_has_all_entries_and_approved_exclusions(self):
        fixtures = ROOT / "benchmarks"
        corpus = corpus_plan()
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
        self.assertEqual(
            names["sdtm-dm-metadata"]["entries"]["python"][0]["environment"],
            "project.yaml",
        )
        # The case runs its completed submission layer with the project
        # that declares the codelists it binds (CT-13).
        self.assertEqual(
            names["sdtm-lb-metadata"]["entries"]["python"],
            [
                {
                    "specification": "submission-spec.yaml",
                    "environment": "submission-project.yaml",
                }
            ],
        )
        self.assertIsNone(names["schema-inheritance-project-root"]["excluded"])
        self.assertEqual({n for n, e in names.items() if e["excluded"]}, set())

    def test_corpus_metamorphic_plan_and_expectations_follow_each_entry(self):
        corpus = corpus_plan()
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
        # forward-reference ordering exceptions remain permanent: LAYER-13
        # composes the empty child's columns in dependency order. So does the
        # link to its consuming specification, which from the empty child
        # names the parent, whose output file the child writes (OUTPUT-9).
        # The tracked differences under #3593 and #3594 are fixed.
        permanent = {
            ("negative-forward-key-reference", "empty_child"),
            ("negative-forward-reference", "empty_child"),
            ("negative-link-to-itself", "empty_child"),
        }
        self.assertEqual(set(expectations), permanent)
        for key in permanent:
            self.assertTrue(expectations[key]["permanent"])
        for (benchmark, check), item in expectations.items():
            self.assertIn(benchmark, names)
            self.assertIn(check, {"empty_child", "key_order"})
            self.assertTrue(item.get("issue") or item.get("permanent"))

    def test_corpus_command_line_plan_excludes_only_project_function_benchmarks(self):
        corpus = corpus_plan()
        names = {example["name"]: example for example in corpus["examples"]}
        for name in ("adam-adsl-bmi", "schema-functions"):
            with self.subTest(name=name):
                self.assertEqual(
                    names[name]["cli"],
                    {"excluded": judge.CLI_FUNCTION_EXCLUSION, "calls": []},
                )
        for name, example in names.items():
            entries = example["entries"]
            if example["cli"]["excluded"]:
                # Only a project supplies project functions.
                self.assertNotEqual(
                    {call["environment"] for call in entries["python"]} - {None, ""},
                    set(),
                    name,
                )
                continue
            # The command line makes the hosts' calls, so they must agree.
            self.assertEqual(entries["python"], entries["r"], name)
            calls = example["cli"]["calls"]
            self.assertEqual(
                [
                    {key: call[key] for key in ("specification", "environment")}
                    for call in calls
                ],
                entries["python"],
                name,
            )
            for call in calls:
                self.assertIsNotNone(call["arguments"], name)
                self.assertEqual(
                    call["arguments"][:2],
                    ["run", call["environment"] or call["specification"]],
                    name,
                )


class CommandLineJudgement(unittest.TestCase):
    """The comparator judges a command-line record by what its runs saved and
    the issues they printed, as the command line's runner records them."""

    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.fixtures = self.root / "fixtures"
        self.case = self.fixtures / "simple"
        (self.case / "expected").mkdir(parents=True)
        (self.case / "spec.yaml").write_text(SPEC)
        (self.case / "benchmark.yaml").write_text(METADATA)
        (self.case / "expected/result.csv").write_bytes(b'ID,VALUE\n1,""\n2,\n')
        self.observed = self.root / "observed"
        self.work = self.observed / "work/simple"
        self.work.mkdir(parents=True)
        shutil.copyfile(self.case / "expected/result.csv", self.work / "result.csv")
        judge.write_json(
            self.observed / "run.json",
            {"protocol": judge.PROTOCOL, "runtime": "cli", "source_revision": "test"},
        )
        # Written independently, as the runner records one run of a binary.
        self.call = {
            "specification": "spec.yaml",
            "environment": None,
            "arguments": ["run", "spec.yaml"],
            "exit_status": 0,
            "stderr": "",
            "document": {"status": "succeeded", "issues": [], "paths": ["result.csv"]},
        }
        self.record = {
            "example": "simple",
            "excluded": None,
            "calls": [self.call],
            "error": None,
        }

    def verdict(self):
        example = judge.plan(self.fixtures)["examples"][0]
        return judge.compare_cli_example(
            example, self.record, self.fixtures, self.observed
        )

    def fails(self, finding, verdict="mismatch"):
        result = self.verdict()
        self.assertEqual(result["verdict"], verdict)
        self.assertIn(finding, result["findings"])

    def fail_negative(self):
        """Make the benchmark a negative whose one run failed as expected."""
        (self.case / "expected/result.csv").unlink()
        (self.work / "result.csv").unlink()
        (self.case / "expected/error.yaml").write_text(
            "phase: derivation\ncondition: test_failure\nspec_paths: [columns.VALUE]\n"
            "rule: REQ-test\ncontext: {keys: [{ID: 2}]}\n"
        )
        issue = {
            "phase": "derivation",
            "condition": "test_failure",
            "spec_paths": ["columns.VALUE"],
            "rule": "REQ-test",
            "severity": "error",
            "context": {"keys": [{"ID": 2}], "extra": "retained"},
        }
        self.call.update(exit_status=1)
        self.call["document"].update(status="failed", issues=[issue], paths=[])
        return issue

    def test_a_run_passes_on_its_status_saved_bytes_and_saved_paths(self):
        self.assertEqual(self.verdict()["verdict"], "pass")
        self.call["document"]["paths"] = ["out/result.csv"]
        self.fails(
            {
                "saved_paths": "spec.yaml",
                "expected": ["result.csv"],
                "actual": ["out/result.csv"],
            }
        )
        self.call["document"]["paths"] = ["result.csv"]
        self.call.update(exit_status=1)
        self.fails(
            {
                "run": ["run", "spec.yaml"],
                "expected": [0, "succeeded"],
                "actual": [1, "succeeded"],
            }
        )
        self.call.update(exit_status=0)
        (self.work / "result.csv").write_bytes(b"ID,VALUE\n1,\n2,\n")
        self.fails("saved CSV bytes differ: result.csv")
        (self.work / "result.csv").unlink()
        self.fails("save did not publish result.csv")

    def test_a_saved_parquet_output_compares_logically(self):
        import pyarrow as pa
        import pyarrow.parquet as pq

        (self.case / "spec.yaml").write_text(
            SPEC.replace("result.csv", "result.parquet")
        )
        (self.case / "expected/result.csv").unlink()
        self.call["document"]["paths"] = ["result.parquet"]
        table = pa.table({"ID": pa.array([1, 2], type=pa.int64()), "VALUE": ["", None]})
        pq.write_table(table, self.case / "expected/result.parquet")
        pq.write_table(table, self.work / "result.parquet", compression="snappy")
        self.assertEqual(self.verdict()["verdict"], "pass")
        pq.write_table(table.slice(1), self.work / "result.parquet")
        self.fails("saved Parquet logical table differs: result.parquet")

    def test_a_negative_needs_its_first_issue_and_saves_no_output(self):
        issue = self.fail_negative()
        self.assertEqual(self.verdict()["verdict"], "pass")
        issue["context"]["keys"][0]["ID"] = 3
        self.assertEqual(self.verdict()["verdict"], "mismatch")
        issue["context"]["keys"][0]["ID"] = 2
        self.call.update(exit_status=0)
        self.fails(
            {"run": ["run", "spec.yaml"], "expected_exit_status": 1, "exit_status": 0}
        )
        self.call.update(exit_status=1)
        self.call["document"]["paths"] = ["result.csv"]
        self.fails(
            {"negative_run_saved": ["run", "spec.yaml"], "paths": ["result.csv"]}
        )
        # An output on disk that the run did not list was saved all the same.
        self.call["document"]["paths"] = []
        (self.work / "result.csv").write_text("ID,VALUE\n")
        self.fails(
            {"negative_run_saved": ["run", "spec.yaml"], "paths": ["result.csv"]}
        )

    def test_a_failed_run_may_save_its_declared_report(self):
        # PROJECT-12: a failed run writes its report, then refuses.
        self.fail_negative()
        (self.case / "spec.yaml").write_text(
            SPEC.replace("output: {", "output: {report: result-checks.csv, ")
        )
        self.call["document"]["paths"] = ["result-checks.csv"]
        self.assertEqual(self.verdict()["verdict"], "pass")
        self.call["document"]["paths"] = ["result-checks.csv", "other.csv"]
        self.fails({"negative_run_saved": ["run", "spec.yaml"], "paths": ["other.csv"]})

    def test_a_validation_negative_needs_every_declared_path(self):
        issue = self.fail_negative()
        (self.case / "expected/error.yaml").write_text(
            "phase: validation\ncondition: test_failure\n"
            "spec_paths: [columns.ID, columns.VALUE]\n"
        )
        issue.update(phase="validation", spec_paths=["columns.ID", "columns.VALUE"])
        # An independent finding beside the expected one is allowed.
        self.call["document"]["issues"].append(
            {**issue, "condition": "other_failure", "spec_paths": ["columns.ID"]}
        )
        self.assertEqual(self.verdict()["verdict"], "pass")
        issue["spec_paths"].pop()
        self.fails({"missing_validation_paths": ["columns.VALUE"]})

    def test_the_record_must_follow_the_plan_and_run_every_call(self):
        self.call["arguments"] = ["run", "other.yaml"]
        self.fails("observed calls do not match the runner plan", "error")
        self.call["arguments"] = ["run", "spec.yaml"]
        self.record.update(error="ValueError: yamaa printed no single JSON line")
        self.fails("ValueError: yamaa printed no single JSON line", "error")
        self.record.update(error=None, excluded="a reason the plan does not give")
        self.fails("observed calls do not match the runner plan", "error")

    def test_engine_unsupported_and_unexpected_issues_never_pass(self):
        issue = {
            "phase": "engine",
            "condition": "internal",
            "spec_paths": ["columns.VALUE"],
            "context": {},
        }
        self.call["document"]["issues"] = [issue]
        self.fails({"engine_issues": [issue]}, "error")
        issue.update(
            phase="validation",
            condition="unsupported_operation",
            context={"operation": "a later feature"},
        )
        result = self.verdict()
        self.assertEqual(
            (result["verdict"], result["unsupported_operations"]),
            ("unsupported", ["a later feature"]),
        )
        issue.update(condition="test_failure", context={})
        self.fails({"unexpected_issues": [issue]})

    def test_the_gate_requires_what_the_command_line_runs(self):
        # A benchmark whose project supplies functions is excluded for the
        # command line alone (#3030 section 2.3), and named with its reason.
        functions = self.fixtures / "functions"
        shutil.copytree(self.case, functions)
        (functions / "project.yaml").write_text(
            'yamaa: "1.0"\ntype: project\nfunctions: [functions/bmi.yaml]\n'
        )
        judge.write_json(self.observed / "records/simple.json", self.record)
        judge.write_json(
            self.observed / "records/functions.json",
            {
                "example": "functions",
                "excluded": judge.CLI_FUNCTION_EXCLUSION,
                "calls": [],
                "error": None,
            },
        )
        inventory = self.root / "inventory.json"
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(judge.compare(self.fixtures, self.observed, inventory), 0)
        written = judge.read_json(inventory)
        self.assertEqual(
            {(r["example"], r["verdict"]) for r in written["benchmarks"]},
            {("functions", "excluded"), ("simple", "pass")},
        )
        self.assertEqual(
            [r.get("reason") for r in written["benchmarks"]],
            [judge.CLI_FUNCTION_EXCLUSION, None],
        )
        (self.work / "result.csv").write_text("broken\n")
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(judge.compare(self.fixtures, self.observed, inventory), 1)
        self.assertEqual(judge.read_json(inventory)["gate"]["failing"], ["simple"])


if __name__ == "__main__":
    unittest.main()
