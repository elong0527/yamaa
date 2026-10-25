"""Mutation controls for complete public truth; synthetic owners are never evidence."""

import copy
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from tools.conformance import complete_reports as cohort
from tools.paths import ROOT


class CompletePublicTruth(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.revision = "a" * 40
        self.evidence = self.directory / "owners"
        self.output = self.directory / "compared"
        for host in ("python", "r"):
            root = self.evidence / host
            supplemental = root / "supplemental" if host == "python" else root
            supplemental.mkdir(parents=True)
            artifact = (
                "native-python-ubuntu-24.04-3.14"
                if host == "python"
                else "native-r-ubuntu-24.04"
            )
            filename = "unit-owner.whl" if host == "python" else "unit-owner.tar.gz"
            record = {
                "scope": "supplemental_suites",
                "platform": "Linux-unit-unqualified",
                "evidence": {
                    "source_revision": self.revision,
                    "run": "unit:unqualified",
                    "artifact_reference": f"github-actions:7/attempt-1/{artifact}/{filename}",
                    "package_form": "wheel" if host == "python" else "source",
                },
                "installation": {
                    "runtime_version": "3.12.unit" if host == "python" else "4.6.1",
                    "host_package_version": "unit-unqualified",
                    "core_version": "fixture-engine",
                    "package_location": "/unit-unqualified/package",
                },
                "suites": [
                    {
                        "id": host + "/" + suffix,
                        "result": "pass",
                        "exit_code": 0,
                        "log": "unit.log",
                    }
                    for suffix in (
                        "original-specifications",
                        "run-report",
                    )
                ],
            }
            cohort.write(supplemental / "supplemental.json", record)
            witnesses = [
                name
                + " public domain complete Rust run report and successful save observations passed"
                for name in cohort.COHORT
            ]
            (supplemental / "unit.log").write_text("\n".join(witnesses))
            for name in cohort.COHORT:
                raw = cohort.read(ROOT / cohort.TRUTH_PATHS[name])
                raw.update(
                    runtime=host, backend="rust", engine_version="fixture-engine"
                )
                raw["runtime_version"] = record["installation"]["runtime_version"]
                cohort.write(root / "public-reports" / f"{name}.{host}.rust.json", raw)

    def compare(self):
        return cohort.compare_owners(
            ROOT / "benchmarks",
            self.evidence / "python",
            self.evidence / "r",
            self.output,
            self.revision,
        )

    def test_all_complete_authored_cases_and_actual_hosts_survive_without_reference_batches(
        self,
    ):
        result = self.compare()
        count = len(cohort.COHORT)
        self.assertEqual(result["summary"], {"python": count, "r": count})
        self.assertEqual(len(result["coverage"]), 2 * count)
        self.assertEqual(len(result["batches"]), 2)
        self.assertTrue(
            all(row["truth"]["kind"] == cohort.TRUTH_KIND for row in result["coverage"])
        )
        self.assertNotIn(
            "reference_run", [batch["level"] for batch in result["batches"]]
        )
        for row in result["coverage"]:
            self.assertEqual(
                (self.output / row["truth"]["retained_path"]).read_bytes(),
                (ROOT / row["truth"]["path"]).read_bytes(),
            )
            source = (
                self.evidence
                / row["runtime"]
                / "public-reports"
                / Path(row["report"]).name
            )
            self.assertEqual(
                (self.output / row["report"]).read_bytes(), source.read_bytes()
            )

    def test_new_cli_compares_all_authored_reports_together(self):
        # Synthetic reports exercise the real command, never qualification.
        revision = subprocess.check_output(
            ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True
        ).strip()
        for host in ("python", "r"):
            root = self.evidence / host
            supplemental = root / "supplemental" if host == "python" else root
            record_path = supplemental / "supplemental.json"
            record = cohort.read(record_path)
            record["evidence"]["source_revision"] = revision
            cohort.write(record_path, record)
        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.conformance.public_conformance",
                "complete-reports",
                "--fixtures",
                str(ROOT / "benchmarks"),
                "--python-evidence",
                str(self.evidence / "python"),
                "--r-evidence",
                str(self.evidence / "r"),
                "--output",
                str(self.output),
                "--source-revision",
                revision,
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        count = len(cohort.COHORT)
        self.assertEqual(json.loads(result.stdout), {"python": count, "r": count})
        inventory = cohort.read(self.output / "coverage.json")
        self.assertEqual(len(inventory["coverage"]), 2 * count)
        self.assertEqual(len(inventory["batches"]), 2)

    def test_malformed_authored_report_cannot_disappear_from_discovery(self):
        checkout = self.directory / "malformed-truth"
        directory = checkout / "rust/crates/yamaa-application/tests/fixtures/specs"
        directory.mkdir(parents=True)
        cohort.write(directory / "partial.json", [{"condition": "unknown_field"}])
        cohort.write(
            directory / "broken.json",
            {"report_version": "0.3.0-draft", "example": "broken"},
        )
        with self.assertRaises(ValueError):
            cohort.public_cases(checkout)

    def test_missing_complete_fields_and_behavior_drift_never_pass(self):
        truth = cohort.read(ROOT / cohort.TRUTH_PATHS["schema-verification-log"])
        for key in truth:
            with self.subTest(missing=key):
                raw = copy.deepcopy(truth)
                del raw[key]
                with self.assertRaises(ValueError):
                    cohort.reports.parse_report(json.dumps(raw))
        expected = cohort.reports.parse_report(json.dumps(truth))
        for label, change in (
            (
                "source values",
                lambda j: j["tables"][0]["rows"][0][0].update(value="changed"),
            ),
            ("row order", lambda j: j["tables"][0]["rows"].reverse()),
            ("table order", lambda j: j["tables"].reverse()),
            ("log order", lambda j: j["artifacts"].reverse()),
            ("saved CSV", lambda j: j["artifacts"][0].update(content="changed")),
            ("verification order", lambda j: j["verifications"].reverse()),
            (
                "verification count",
                lambda j: j["verifications"][0].update(evaluated_count=999),
            ),
            (
                "source read order/count",
                lambda j: j["source_reads"].append(copy.deepcopy(j["source_reads"][0])),
            ),
        ):
            with self.subTest(change=label):
                raw = copy.deepcopy(truth)
                change(raw)
                self.assertFalse(
                    cohort.reports.compare_reports(
                        expected, cohort.reports.parse_report(json.dumps(raw))
                    ).passed
                )
        raw = copy.deepcopy(truth)
        del raw["nodes"][0]["handler_counts"]
        with self.assertRaises(ValueError):
            cohort.reports.parse_report(json.dumps(raw))
        negative = cohort.read(ROOT / cohort.TRUTH_PATHS["negative-integer-overflow"])
        changed = copy.deepcopy(negative)
        changed["diagnostics"][0]["condition"] = "other"
        self.assertFalse(
            cohort.reports.compare_reports(
                cohort.reports.parse_report(json.dumps(negative)),
                cohort.reports.parse_report(json.dumps(changed)),
            ).passed
        )

    def test_complete_negative_truth_rejects_an_unrelated_output_finding(self):
        original = cohort.read(ROOT / cohort.TRUTH_PATHS["negative-integer-overflow"])
        changed = copy.deepcopy(original)
        changed["diagnostics"].append(
            {
                "phase": "validation",
                "condition": "undeclared_column",
                "spec_paths": ["output.columns"],
                "rule": "OUTPUT-2",
                "context": {"column": "UNRELATED"},
            }
        )
        verdict = cohort.reports.compare_reports(
            cohort.reports.parse_report(json.dumps(original)),
            cohort.reports.parse_report(json.dumps(changed)),
        )
        self.assertFalse(verdict.passed)
        self.assertIn(
            "report.diagnostics.count", [finding.kind for finding in verdict.findings]
        )

    def test_present_null_geometry_is_rejected_while_omitted_geometry_reads(self):
        original = cohort.read(ROOT / cohort.TRUTH_PATHS["negative-integer-overflow"])
        sites = {
            "report": (
                lambda raw: raw["diagnostics"][0],
                lambda report: report.diagnostics[0],
            ),
            "node": (
                lambda raw: raw["nodes"][0]["diagnostics"][0],
                lambda report: report.nodes[0].diagnostics[0],
            ),
        }
        for key in sorted(cohort.reports.GEOMETRY_FIELDS):
            for site, (select, observed) in sites.items():
                with self.subTest(key=key, site=site):
                    self.assertIsNotNone(select(original)[key])
                    omitted = copy.deepcopy(original)
                    del select(omitted)[key]
                    report = cohort.reports.parse_report(json.dumps(omitted))
                    self.assertIsNone(getattr(observed(report), key))
                    nulled = copy.deepcopy(original)
                    select(nulled)[key] = None
                    with self.assertRaises(ValueError):
                        cohort.reports.parse_report(json.dumps(nulled))

    def test_written_reports_omit_unrecorded_geometry_and_read_back(self):
        original = cohort.read(ROOT / cohort.TRUTH_PATHS["negative-integer-overflow"])
        omitted = copy.deepcopy(original)
        diagnostics = (
            omitted["diagnostics"][0],
            omitted["nodes"][0]["diagnostics"][0],
        )
        for diagnostic in diagnostics:
            for key in cohort.reports.GEOMETRY_FIELDS:
                del diagnostic[key]
        unrecorded = cohort.reports.parse_report(json.dumps(omitted))
        path = cohort.reports.write_report(unrecorded, self.directory / "unrecorded")
        written = cohort.read(path)
        for diagnostic in (
            written["diagnostics"][0],
            written["nodes"][0]["diagnostics"][0],
        ):
            self.assertFalse(cohort.reports.GEOMETRY_FIELDS & set(diagnostic))
        self.assertEqual(cohort.reports.read_report(path), unrecorded)
        recorded = cohort.reports.parse_report(json.dumps(original))
        path = cohort.reports.write_report(recorded, self.directory / "recorded")
        self.assertEqual(cohort.reports.read_report(path), recorded)
        self.assertEqual(cohort.read(path)["diagnostics"], original["diagnostics"])
        # Unrecorded geometry stays distinct from a recorded empty route or
        # zero-width span in a parity comparison.
        self.assertFalse(cohort.reports.compare_reports(recorded, unrecorded).passed)

    def test_exact_scalar_bits_and_missing_empty_are_preserved(self):
        observe = cohort.reports.observe_scalar
        for value in (-(2**63), 2**63 - 1, 2**53 + 1):
            with self.subTest(value=value):
                self.assertEqual(observe(value).value, str(value))
        self.assertEqual(observe(0.1).value, "3fb999999999999a")
        self.assertNotEqual(observe(0.0), observe(-0.0))
        self.assertNotEqual(observe(None), observe(""))

    def test_callback_order_count_and_exact_argument_bits_are_compared(self):
        raw = cohort.read(ROOT / cohort.TRUTH_PATHS["schema-verification-log"])
        raw["callbacks"] = [
            {
                "specification": "spec.yaml",
                "function": "unit_protocol",
                "contract_version": "unit",
                "arguments": [
                    ["value", cohort.reports.observe_scalar(value).model_dump()]
                ],
            }
            for value in (0.0, -0.0)
        ]
        expected = cohort.reports.parse_report(json.dumps(raw))
        for change in (
            lambda j: j["callbacks"].pop(),
            lambda j: j["callbacks"].reverse(),
            lambda j: j["callbacks"][0]["arguments"][0][1].update(
                value="3ff0000000000000"
            ),
        ):
            changed = copy.deepcopy(raw)
            change(changed)
            self.assertFalse(
                cohort.reports.compare_reports(
                    expected, cohort.reports.parse_report(json.dumps(changed))
                ).passed
            )

    def test_parquet_truth_compares_logical_types_and_values(self):
        import io

        import pyarrow as pa
        import pyarrow.parquet as pq

        artifact = cohort.reports.ArtifactObservation(
            name="produced",
            profile="parquet",
            columns=("ID",),
            types=("str",),
            row_count=1,
            records=('["ID"]', '["01"]'),
            byte_length=228,
        )
        for value in ("02", 1):
            with self.subTest(value=value):
                content = io.BytesIO()
                pq.write_table(pa.table({"ID": [value]}), content)
                findings = cohort.reports._artifact_findings(
                    artifact, content.getvalue()
                )
                self.assertTrue(findings)
                if isinstance(value, int):
                    self.assertIn(
                        "artifact.types", [finding.kind for finding in findings]
                    )

    def test_held_parquet_truth_reader_finishes_before_process_shutdown(self):
        program = """
import pyarrow as pa
import pyarrow.parquet as pq
from tools.conformance.report_values import _expected_parquet
buffer = pa.BufferOutputStream()
pq.write_table(pa.table({"VALUE": [1, 2, None]}), buffer)
observed, kinds = _expected_parquet(buffer.getvalue().to_pybytes())
assert kinds == ("int",)
assert observed.to_dict(as_series=False) == {"VALUE": [1, 2, None]}
"""
        result = subprocess.run(
            [sys.executable, "-c", program],
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_wrong_truth_tuple_or_missing_actual_lane_is_refused(self):
        result = self.compare()
        for label, change in (
            (
                "truth source",
                lambda j: j["coverage"][0]["truth"].update(source_revision="b" * 40),
            ),
            (
                "truth path",
                lambda j: j["coverage"][0]["truth"].update(path="other.json"),
            ),
            (
                "truth kind",
                lambda j: j["coverage"][0]["truth"].update(kind="reference_run"),
            ),
            ("missing case", lambda j: j["coverage"].pop()),
            (
                "duplicate case",
                lambda j: j["coverage"].append(copy.deepcopy(j["coverage"][0])),
            ),
            ("missing lane", lambda j: j["batches"].pop()),
            (
                "report path",
                lambda j: j["coverage"][0].update(report="../outside.json"),
            ),
        ):
            with self.subTest(change=label):
                raw = copy.deepcopy(result)
                change(raw)
                with self.assertRaises(ValueError):
                    cohort.validate_inventory(raw, self.revision, cohort.COHORT)

    def test_retired_verification_log_suite_cannot_stand_in_for_the_run_report(self):
        for host in ("python", "r"):
            with self.subTest(host=host):
                root = self.evidence / host
                path = (root / "supplemental" if host == "python" else root) / (
                    "supplemental.json"
                )
                original = path.read_bytes()
                record = cohort.read(path)
                for suite in record["suites"]:
                    if suite["id"] == host + "/run-report":
                        suite["id"] = host + "/verification-logs"
                cohort.write(path, record)
                try:
                    with self.assertRaisesRegex(
                        ValueError, f"suite is missing: {host}/run-report"
                    ):
                        cohort.installed_owner(root, host, self.revision)
                finally:
                    path.write_bytes(original)

    def test_tools_import_without_any_yamaa_package(self):
        program = """
import importlib, importlib.abc, sys
class RefusePackage(importlib.abc.MetaPathFinder):
    def find_spec(self, name, path=None, target=None):
        if name == 'yamaa' or name.startswith('yamaa.'):
            raise AssertionError('qualification imported package/evaluator: ' + name)
sys.meta_path.insert(0, RefusePackage())
for name in ('complete_reports', 'report_values', 'public_conformance'):
    importlib.import_module('tools.conformance.' + name)
"""
        subprocess.run([sys.executable, "-c", program], cwd=ROOT, check=True)


class InstalledOwnerReview(unittest.TestCase):
    def setUp(self):
        self.fixture = CompletePublicTruth("runTest")
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)

    def test_qualifier_rejects_nonnull_error_on_actual_complete_public_report(self):
        path = (
            self.fixture.evidence
            / "python/public-reports/schema-lookup.python.rust.json"
        )
        raw = cohort.read(path)
        self.assertEqual(raw["outcome"], "success")
        raw["error"] = "unexpected infrastructure error"
        cohort.write(path, raw)
        with self.assertRaisesRegex(ValueError, "error|report|identity"):
            self.fixture.compare()

    def test_qualifier_binds_complete_public_reports_to_installed_runtime_version(self):
        self.fixture.compare()
        shutil.rmtree(self.fixture.output)
        path = (
            self.fixture.evidence
            / "python/public-reports/schema-lookup.python.rust.json"
        )
        raw = cohort.read(path)
        raw["runtime_version"] = "3.14.9"
        cohort.write(path, raw)
        with self.assertRaisesRegex(ValueError, "runtime|report|owner|version"):
            self.fixture.compare()


class CommandLineCohort(unittest.TestCase):
    """The command line's saved outputs join its platform's comparison, beside
    the owners it ran with; synthetic runs are never evidence."""

    def setUp(self):
        self.fixture = CompletePublicTruth("runTest")
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.cli = self.fixture.directory / "cli"
        cohort.write(
            self.cli / "observed/run.json",
            {
                "protocol": "public-conformance/1",
                "runtime": "cli",
                "version": "yamaa fixture-engine\nlanguage versions: 1.0\n",
                "source_revision": self.fixture.revision,
                "evidence": "unit:unqualified",
            },
        )
        cohort.write(
            self.cli / "inventory.json",
            {
                "runtime": "cli",
                "source_revision": self.fixture.revision,
                "gate": {"mode": "all-active-must-pass", "failing": []},
            },
        )
        # Each run saved what the authored truth publishes, in its order.
        for name in cohort.COHORT:
            truth = cohort.read(ROOT / cohort.TRUTH_PATHS[name])
            paths = []
            for artifact in truth["artifacts"]:
                self.assertEqual(artifact["profile"], "csv")
                path = f"{artifact['name']}.csv"
                saved = self.cli / "observed/work" / name / path
                saved.parent.mkdir(parents=True, exist_ok=True)
                saved.write_bytes(artifact["content"].encode("utf-8"))
                paths.append(path)
            succeeded = truth["outcome"] == "success"
            self.write_run(name, 0 if succeeded else 1, paths)

    def write_run(self, name, exit_status, paths):
        status = "succeeded" if exit_status == 0 else "failed"
        call = {
            "specification": "spec.yaml",
            "environment": None,
            "arguments": ["run", "spec.yaml"],
            "exit_status": exit_status,
            "stderr": "",
            "document": {"status": status, "issues": [], "paths": paths},
        }
        cohort.write(
            self.cli / "observed/records" / f"{name}.json",
            {"example": name, "excluded": None, "calls": [call], "error": None},
        )

    def compare(self):
        self.fixture.compare()
        return cohort.compare_cli_outputs(
            self.cli, self.fixture.output, self.fixture.revision
        )

    def rows(self, name):
        coverage = cohort.read(self.fixture.output / "cli/coverage.json")
        return [row for row in coverage["coverage"] if row["example"] == name]

    def test_saved_outputs_equal_both_hosts_reports_case_by_case(self):
        result = self.compare()
        count = len(cohort.COHORT)
        self.assertEqual(result["summary"], {"python": count, "r": count})
        self.assertEqual(len(result["coverage"]), 2 * count)
        output = self.fixture.output
        self.assertEqual(cohort.read(output / "cli/coverage.json"), result)
        for name in ("observed/run.json", "inventory.json"):
            with self.subTest(name=name):
                self.assertEqual(
                    (output / "cli" / Path(name).name).read_bytes(),
                    (self.cli / name).read_bytes(),
                )
        [python, r] = self.rows("schema-verification-log")
        self.assertEqual((python["compared_with"], r["compared_with"]), ("python", "r"))
        self.assertEqual(
            python["saved"],
            [
                "cli/saved/schema-verification-log/adsl-checks.csv",
                "cli/saved/schema-verification-log/adsl.csv",
            ],
        )
        for saved in python["saved"]:
            source = self.cli / "observed/work" / Path(saved).relative_to("cli/saved")
            self.assertEqual((output / saved).read_bytes(), source.read_bytes())

    def test_a_saved_byte_that_differs_from_either_host_fails(self):
        saved = self.cli / "observed/work/schema-lookup/adsl.csv"
        saved.write_bytes(saved.read_bytes().replace(b"\n01,", b"\n02,"))
        with self.assertRaisesRegex(ValueError, "differ.*: schema-lookup$"):
            self.compare()
        for row in self.rows("schema-lookup"):
            self.assertEqual(row["result"], "mismatch")
            self.assertIn(
                "artifact.record", [finding["kind"] for finding in row["findings"]]
            )

    def test_a_missing_artifact_or_another_outcome_fails(self):
        self.write_run("schema-lookup", 0, [])
        self.write_run("negative-zero-division", 0, [])
        with self.assertRaisesRegex(
            ValueError, ": negative-zero-division, schema-lookup$"
        ):
            self.compare()
        kinds = {
            name: {
                finding["kind"]
                for row in self.rows(name)
                for finding in row["findings"]
            }
            for name in ("schema-lookup", "negative-zero-division")
        }
        self.assertEqual(
            kinds,
            {"schema-lookup": {"artifacts"}, "negative-zero-division": {"outcome"}},
        )

    def test_the_run_names_this_revision_core_and_actions_run_and_passed(self):
        self.fixture.compare()
        for name, key, value, message in (
            ("observed/run.json", "source_revision", "b" * 40, "stale or foreign"),
            ("observed/run.json", "version", "yamaa 0.0.1\n", "engine differs"),
            ("observed/run.json", "evidence", "unit:other", "ran apart"),
            ("inventory.json", "gate", {"failing": ["schema-lookup"]}, "did not pass"),
            ("inventory.json", "source_revision", "b" * 40, "did not pass"),
        ):
            path = self.cli / name
            original = path.read_bytes()
            raw = cohort.read(path)
            raw[key] = value
            cohort.write(path, raw)
            try:
                with self.subTest(file=name, key=key):
                    with self.assertRaisesRegex(ValueError, message):
                        cohort.compare_cli_outputs(
                            self.cli, self.fixture.output, self.fixture.revision
                        )
            finally:
                path.write_bytes(original)
        self.assertFalse((self.fixture.output / "cli").exists())

    def test_an_excluded_case_is_named_with_its_reason(self):
        cohort.write(
            self.cli / "observed/records/schema-lookup.json",
            {
                "example": "schema-lookup",
                "excluded": "excepted by the 1.0 conversion gate: a decided item",
                "calls": [],
                "error": None,
            },
        )
        result = self.compare()
        count = len(cohort.COHORT) - 1
        self.assertEqual(result["summary"], {"python": count, "r": count})
        self.assertEqual(
            {(row["result"], row["reason"]) for row in self.rows("schema-lookup")},
            {("excluded", "excepted by the 1.0 conversion gate: a decided item")},
        )

    def test_the_command_compares_the_command_line_beside_the_owners(self):
        # Synthetic runs exercise the real command, never qualification.
        revision = subprocess.check_output(
            ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True
        ).strip()
        evidence = self.fixture.evidence
        for path in (
            evidence / "python/supplemental/supplemental.json",
            evidence / "r/supplemental.json",
        ):
            record = cohort.read(path)
            record["evidence"]["source_revision"] = revision
            cohort.write(path, record)
        for name in ("observed/run.json", "inventory.json"):
            raw = cohort.read(self.cli / name)
            raw["source_revision"] = revision
            cohort.write(self.cli / name, raw)
        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "tools.conformance.public_conformance",
                "complete-reports",
                "--fixtures",
                str(ROOT / "benchmarks"),
                "--python-evidence",
                str(evidence / "python"),
                "--r-evidence",
                str(evidence / "r"),
                "--cli-evidence",
                str(self.cli),
                "--output",
                str(self.fixture.output),
                "--source-revision",
                revision,
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        count = len(cohort.COHORT)
        self.assertIn(
            json.dumps({"cli": {"python": count, "r": count}}, indent=2), result.stdout
        )
        self.assertEqual(
            cohort.read(self.fixture.output / "cli/coverage.json")["source_revision"],
            revision,
        )


if __name__ == "__main__":
    unittest.main()
