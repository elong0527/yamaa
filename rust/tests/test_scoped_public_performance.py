"""Scope admission fixtures; raw timing/truth replay keeps its original tests."""

import copy
import importlib.util
import sys
import unittest
from types import SimpleNamespace
from unittest import mock

import test_audit_public_performance as fixtures

auditor = fixtures.auditor
collector = auditor.collector
with mock.patch.object(sys, "path", [str(fixtures.TOOLS), *sys.path]):
    spec = importlib.util.spec_from_file_location(
        "prepare_scoped_performance_audit",
        fixtures.TOOLS / "prepare_scoped_performance_audit.py",
    )
    manifest = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(manifest)


class ScopeCollection(unittest.TestCase):
    def setUp(self):
        self.args = SimpleNamespace(
            collection_scope=collector.LINUX_INSTALLED,
            purpose="characterization",
            evaluator="public",
            scope="full-lifecycle",
            cases=None,
            families=None,
            host="python",
        )
        self.record = {
            "host": "python",
            "system": "Linux",
            "package_form": "wheel",
            "runtime_version": "3.12.12",
        }

    def test_exact_installed_python_and_r_are_admitted(self):
        self.assertEqual(
            collector.validate_collection_scope(self.args, self.record),
            collector.LINUX_INSTALLED,
        )
        self.args.host = "r"
        self.record.update(host="r", package_form="source", runtime_version="4.6.1")
        self.assertEqual(
            collector.validate_collection_scope(self.args, self.record),
            collector.LINUX_INSTALLED,
        )

    def test_scoped_collection_cannot_replace_runtime_package_or_authored_inventory(
        self,
    ):
        for name, value in [
            ("purpose", "capacity-frontier"),
            ("evaluator", "python-reference"),
            ("scope", "producer-root-build"),
            ("cases", ["lookup-1000-1"]),
            ("families", ["lookup"]),
        ]:
            changed = copy.copy(self.args)
            setattr(changed, name, value)
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "all 27"):
                collector.validate_collection_scope(changed, self.record)
        for name, value in [
            ("system", "Darwin"),
            ("package_form", "source"),
            ("runtime_version", "3.13.0"),
        ]:
            changed = dict(self.record, **{name: value})
            with self.subTest(name=name), self.assertRaises(ValueError):
                collector.validate_collection_scope(self.args, changed)
        self.args.host = "r"
        self.record.update(host="r", package_form="source", runtime_version="4.6.0")
        with self.assertRaises(ValueError):
            collector.validate_collection_scope(self.args, self.record)


class LinuxAudit(unittest.TestCase):
    def setUp(self):
        fixtures.CollectionGates.setUp(self)
        self.payload["collection_scope"] = collector.LINUX_INSTALLED
        lanes = auditor.tuples(collector.LINUX_INSTALLED)
        self.payload["entries"] = [
            entry for entry in self.payload["entries"] if entry["tuple"] in lanes
        ]
        for entry in self.payload["entries"]:
            entry["collections"] = [
                value for value in entry["collections"] if value["role"] == "candidate"
            ]
            path = self.root / entry["collections"][0]["path"] / "collection.json"
            value = auditor.read(path)
            value["collection_scope"] = collector.LINUX_INSTALLED
            fixtures.write(path, value)

    def admitted_values(self, *args):
        return fixtures.CollectionGates.admitted_values(self, *args)

    def invoke(self, name, *, scope=collector.LINUX_INSTALLED):
        fixtures.write(self.root / "input.json", self.payload)
        status = auditor.main(
            [
                "--input",
                str(self.root / "input.json"),
                "--output",
                str(self.root / name),
                "--collection-scope",
                scope,
                "--check-collection",
            ]
        )
        return status, auditor.read(self.root / name / "performance-audit.json")

    def test_exact_54_cells_pass_without_baselines_or_numeric_acceptance(self):
        with mock.patch.object(
            auditor, "admit_collection", side_effect=self.admitted_values
        ) as replay:
            status, report = self.invoke("linux-report")
        self.assertEqual(status, 0)
        self.assertEqual(
            report["coverage"],
            {"status": "pass", "required": 54, "passed": 54, "failed": 0, "missing": 0},
        )
        self.assertEqual(report["comparator_coverage"]["required"], 0)
        self.assertEqual(report["collection_gate"]["status"], "pass")
        self.assertEqual(report["budgets"]["status"], "not_evaluated")
        self.assertFalse(report["accepted"])
        self.assertEqual(replay.call_count, 2)
        self.assertTrue(
            all(call.args[2] == "candidate" for call in replay.call_args_list)
        )
        self.assertEqual(len(auditor.tuples()), 21)

    def test_default_full_audit_cannot_silently_consume_a_scoped_manifest(self):
        status, report = self.invoke("wrong-default", scope=collector.FULL_MATRIX)
        self.assertEqual(status, 1)
        self.assertEqual(report["coverage"]["required"], 567)
        self.assertIn("scope", str(report["harness_correctness"]["errors"]))

    def test_missing_one_candidate_cell_keeps_54_denominator(self):
        def missing(*args):
            values = self.admitted_values(*args)
            if args[1]["host"] == "r":
                values.pop(auditor.case_names()[-1])
            return values

        with mock.patch.object(auditor, "admit_collection", side_effect=missing):
            status, report = self.invoke("one-missing")
        self.assertEqual(status, 1)
        self.assertEqual(report["coverage"]["required"], 54)
        self.assertEqual(report["coverage"]["passed"], 53)
        self.assertEqual(report["coverage"]["missing"], 1)

    def test_unexpected_tuple_duplicate_and_scope_relabel_are_refused(self):
        original = copy.deepcopy(self.payload)
        for index, mutate in enumerate(
            [
                lambda: self.payload["entries"][0]["tuple"].update(python="3.13"),
                lambda: self.payload["entries"].append(
                    copy.deepcopy(self.payload["entries"][0])
                ),
            ]
        ):
            self.payload = copy.deepcopy(original)
            mutate()
            with mock.patch.object(
                auditor, "admit_collection", side_effect=self.admitted_values
            ):
                status, report = self.invoke("bad-tuple-" + str(index))
            self.assertEqual(status, 1)
            self.assertTrue(report["harness_correctness"]["errors"])
        self.payload = original
        entry = self.payload["entries"][0]
        path = self.root / entry["collections"][0]["path"] / "collection.json"
        value = auditor.read(path)
        value.pop("collection_scope")
        fixtures.write(path, value)
        with mock.patch.object(
            auditor, "admit_collection", side_effect=self.admitted_values
        ):
            status, report = self.invoke("relabelled")
        self.assertEqual(status, 1)
        self.assertIn("scope", str(report["harness_correctness"]["errors"]))

    def test_final_budget_and_fragment_flags_cannot_change_scope_acceptance(self):
        for flags in [
            ["--check"],
            ["--budgets", "budgets.json"],
            ["--catalog"],
            ["--unit", "unit.json"],
        ]:
            with (
                self.subTest(flags=flags),
                self.assertRaises(SystemExit),
                mock.patch.object(sys, "stderr"),
            ):
                auditor.main(
                    [
                        "--input",
                        str(self.root / "input.json"),
                        "--output",
                        str(self.root / "forbidden"),
                        "--collection-scope",
                        collector.LINUX_INSTALLED,
                        *flags,
                    ]
                )

    def test_recorded_optional_comparator_failure_still_refuses_collection(self):
        entry = self.payload["entries"][0]
        entry["collections"].append(
            {"role": "baseline", "path": entry["artifact_name"] + "/baseline-evidence"}
        )

        def failed(*args):
            values = self.admitted_values(*args)
            if args[2] == "baseline":
                values[auditor.role_case_names("baseline")[0]] = {
                    "status": "fail",
                    "reason": "retained raw comparator failed",
                }
            return values

        with mock.patch.object(auditor, "admit_collection", side_effect=failed):
            status, report = self.invoke("optional-comparator-failure")
        self.assertEqual(status, 1)
        self.assertEqual(report["coverage"]["passed"], 54)
        self.assertEqual(report["comparator_coverage"]["required"], 0)
        self.assertEqual(report["harness_correctness"]["status"], "fail")

    def test_scope_manifest_binds_original_artifact_attempt_without_rewriting_metadata(
        self,
    ):
        before = {
            name: (self.root / name).read_bytes()
            for name in ["run.json", "jobs.json", "artifacts.json"]
        }
        value = manifest.prepare(
            self.root, self.payload["repository"], self.payload["source_revision"]
        )
        self.assertEqual(
            [entry["tuple"] for entry in value["entries"]],
            auditor.tuples(collector.LINUX_INSTALLED),
        )
        self.assertTrue(
            all(
                [collection["role"] for collection in entry["collections"]]
                == ["candidate"]
                for entry in value["entries"]
            )
        )
        self.assertEqual(value["collection_scope"], collector.LINUX_INSTALLED)
        self.assertEqual(
            before, {name: (self.root / name).read_bytes() for name in before}
        )
        artifacts = auditor.read(self.root / "artifacts.json")
        if isinstance(artifacts, dict):
            artifacts = artifacts["artifacts"]
        duplicate = copy.deepcopy(artifacts[0])
        duplicate["id"] += 100000
        artifacts.append(duplicate)
        fixtures.write(self.root / "artifacts.json", {"artifacts": artifacts})
        with self.assertRaisesRegex(ValueError, "ambiguous"):
            manifest.prepare(
                self.root, self.payload["repository"], self.payload["source_revision"]
            )


if __name__ == "__main__":
    unittest.main()
