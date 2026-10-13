"""Mutation controls for complete public truth; synthetic owners are never evidence."""

import copy
import importlib.util
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOOLS = ROOT / "rust/tools"
sys.path.insert(0, str(TOOLS))


def load(name):
    spec = importlib.util.spec_from_file_location(name, TOOLS / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


public = load("qualify_public_cohort")
producer = load("qualify_producer_cohort")
audit_module = load("audit_native_run")
cohort = public.cohort


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
                        "verification-logs",
                        "producer-cohort",
                    )
                ],
            }
            cohort.write(supplemental / "supplemental.json", record)
            witnesses = [
                name
                + " public domain complete Rust run report and successful save observations passed"
                for name in cohort.COHORT
            ]
            witnesses.append(
                producer.NAME
                + " actual installed public owner and repeated held-byte Save evidence passed"
            )
            (supplemental / "unit.log").write_text("\n".join(witnesses))
            for name in cohort.COHORT:
                raw = cohort.read(ROOT / cohort.TRUTH_PATHS[name])
                raw.update(
                    runtime=host, backend="rust", engine_version="fixture-engine"
                )
                raw["runtime_version"] = record["installation"]["runtime_version"]
                cohort.write(root / "public-reports" / f"{name}.{host}.rust.json", raw)
            self.producer_owner(root, host, record)

    def producer_owner(self, root, host, record):
        source = ROOT / "rust/qualification/producer-cohort" / producer.NAME
        owner = root / "public-reports/producer-cohort" / host / producer.NAME
        case = "/unit-unqualified/" + host + "/" + producer.NAME
        owner.mkdir(parents=True)
        (owner / "case-directory.txt").write_text(case)
        cohort.write(
            owner / "provenance.json",
            {key: record[key] for key in ("scope", "evidence", "installation")},
        )
        raw = cohort.read(source / "reference.json")
        raw.update(
            runtime=host,
            backend="rust",
            engine_version="fixture-engine",
            example="domain",
            entry="spec.yaml",
            activation={"lock": "not_requested", "bindings": [], "tests": []},
            environment={
                "source": f"/unit-unqualified/{host}/environment.yaml",
                "lock_source": f"/unit-unqualified/{host}/"
                + ("uv.lock" if host == "python" else "renv.lock"),
            },
        )
        raw["runtime_version"] = record["installation"]["runtime_version"]
        for group in ("nodes", "tables", "verifications", "callbacks"):
            for observation in raw[group]:
                observation["specification"] = case + "/" + observation["specification"]
        for observation in raw["source_reads"]:
            observation.update(
                specification=case + "/producer.yaml", base_directory=case
            )
        for artifact in raw["artifacts"]:
            artifact["specification"] = (
                case
                + "/"
                + ("producer.yaml" if artifact["name"] == "produced" else "spec.yaml")
            )
        cohort.write(owner / "before.json", raw)
        for attempt in (1, 2):
            saved = owner / f"after-{attempt}"
            cohort.write(
                saved / "report.json",
                {
                    **raw,
                    "publication": {
                        "status": "success",
                        "error": None,
                        "entered": [
                            {"node": 1, "published": True},
                            {"node": 0, "published": True},
                        ],
                    },
                },
            )
            for name in producer.TARGETS:
                shutil.copyfile(source / "expected" / name, saved / name)

    def compare(self, is_producer=False):
        tool = producer if is_producer else public
        fixtures = (
            ROOT / "rust/qualification/producer-cohort"
            if is_producer
            else ROOT / "benchmarks"
        )
        return tool.compare_owners(
            fixtures,
            self.evidence / "python",
            self.evidence / "r",
            self.output,
            self.revision,
        )

    def test_all_complete_authored_cases_and_actual_hosts_survive_without_reference_batches(
        self,
    ):
        result = self.compare()
        self.assertEqual(result["summary"], {"python": 12, "r": 12})
        self.assertEqual(len(result["coverage"]), 24)
        self.assertEqual(len(result["batches"]), 2)
        self.assertTrue(
            all(row["truth"]["kind"] == cohort.TRUTH_KIND for row in result["coverage"])
        )
        self.assertFalse(
            any(batch["level"] == "reference_run" for batch in result["batches"])
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

    def test_exact_scalar_bits_and_missing_empty_are_preserved(self):
        observe = cohort.reports.observe_scalar
        for value in (-(2**63), 2**63 - 1, 2**53 + 1):
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

        report = cohort.reports.read_report(ROOT / cohort.TRUTH_PATHS[producer.NAME])
        artifact = next(a for a in report.artifacts if a.profile == "parquet")
        for value in ("02", 1):
            content = io.BytesIO()
            pq.write_table(pa.table({"ID": [value]}), content)
            findings = cohort.reports._artifact_findings(artifact, content.getvalue())
            self.assertTrue(findings)
            if isinstance(value, int):
                self.assertIn("artifact.types", [finding.kind for finding in findings])

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

    def test_incomplete_producer_owner_and_altered_saved_bytes_are_refused(self):
        owner = (
            self.evidence
            / "python/public-reports/producer-cohort/python"
            / producer.NAME
        )
        before_path = owner / "before.json"
        before = cohort.read(before_path)
        for key in ("callbacks", "environment", "source_reads"):
            with self.subTest(missing=key):
                raw = copy.deepcopy(before)
                del raw[key]
                with self.assertRaises(ValueError):
                    producer.project_report(
                        raw, (owner / "case-directory.txt").read_text()
                    )
        (owner / "after-2/out.csv").write_bytes(b"ID\n02\n")
        with self.assertRaisesRegex(ValueError, "CSV bytes"):
            self.compare(True)

    def test_audit_rechecks_exact_source_truth_and_complete_reports(self):
        result = self.compare(True)
        audit = object.__new__(audit_module.Audit)
        audit.directory = self.directory / "audit"
        audit.source = {"sha": self.revision}
        audit.run = {"id": 7, "run_attempt": 1, "html_url": "unit:unqualified"}
        audit.artifacts = []
        for artifact, filename in (
            ("native-python-ubuntu-24.04-3.14", "unit-owner.whl"),
            ("native-r-ubuntu-24.04", "unit-owner.tar.gz"),
        ):
            path = audit.directory / "artifacts" / artifact / filename
            path.parent.mkdir(parents=True)
            path.write_bytes(b"unit-unqualified bytes; comparator control only")
            audit.artifacts.append({"name": artifact, "expired": False})
        source = audit.directory / "source" / cohort.TRUTH_PATHS[producer.NAME]
        source.parent.mkdir(parents=True)
        shutil.copyfile(ROOT / cohort.TRUTH_PATHS[producer.NAME], source)
        checked = audit.committed_cohort(
            "ubuntu-24.04", "3.12", "wheel", True, self.output, result
        )
        self.assertEqual(checked["required_tuples"], 2)
        retained = self.output / result["coverage"][0]["truth"]["retained_path"]
        retained.write_bytes(retained.read_bytes() + b"\n")
        with self.assertRaisesRegex(ValueError, "exact source bytes"):
            audit.committed_cohort(
                "ubuntu-24.04", "3.12", "wheel", True, self.output, result
            )
        retained.write_bytes(source.read_bytes())
        report_path = self.output / result["coverage"][0]["report"]
        original_report = report_path.read_bytes()
        changed = cohort.read(report_path)
        changed["tables"][0]["rows"][0][0]["value"] = "changed"
        cohort.write(report_path, changed)
        with self.assertRaisesRegex(ValueError, "complete report differs"):
            audit.committed_cohort(
                "ubuntu-24.04", "3.12", "wheel", True, self.output, result
            )
        report_path.write_bytes(original_report)
        before_path = self.output / "raw/python" / producer.NAME / "before.json"
        incomplete = cohort.read(before_path)
        del incomplete["callbacks"]
        cohort.write(before_path, incomplete)
        with self.assertRaisesRegex(ValueError, "incomplete native producer"):
            audit.committed_cohort(
                "ubuntu-24.04", "3.12", "wheel", True, self.output, result
            )

    def test_tools_import_without_any_yamaa_package(self):
        program = """
import importlib.abc, importlib.util, sys
from pathlib import Path
class RefusePackage(importlib.abc.MetaPathFinder):
    def find_spec(self, name, path=None, target=None):
        if name == 'yamaa' or name.startswith('yamaa.'):
            raise AssertionError('qualification imported package/evaluator: ' + name)
sys.meta_path.insert(0, RefusePackage())
for filename in ('qualify_public_cohort.py', 'qualify_producer_cohort.py'):
    spec = importlib.util.spec_from_file_location(filename, Path(sys.argv[1]) / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
"""
        subprocess.run([sys.executable, "-I", "-c", program, str(TOOLS)], check=True)


if __name__ == "__main__":
    unittest.main()
