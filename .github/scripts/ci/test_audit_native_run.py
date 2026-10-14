"""CI-discovered independent native evidence audit tests; no candidate execution."""

import copy
import importlib.util
import io
import json
import shutil
import sys
import tarfile
import tempfile
import unittest
import zipfile
from collections import Counter
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
TOOLS = ROOT / "rust/tools"
sys.path.insert(0, str(TOOLS))
spec = importlib.util.spec_from_file_location(
    "audit_native_run", TOOLS / "audit_native_run.py"
)
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)
FIXTURE = Path(__file__).parent / "fixtures/native-audit"


class NativeAudit(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name) / "input"
        shutil.copytree(FIXTURE, self.directory)
        for name in audit.SOURCE_FILES:
            path = self.directory / "source" / name
            path.parent.mkdir(parents=True, exist_ok=True)
            if name.endswith("supplemental.json"):
                audit.write(
                    path,
                    {
                        "suites": [
                            {
                                "id": "python/original-specifications",
                                "runtime": "python",
                            },
                            {"id": "r/check", "runtime": "r"},
                        ]
                    },
                )
            else:
                path.write_text(
                    "examples:\n  independently-authored:\n"
                    if name.endswith("yaml")
                    else "independent source bytes\n"
                )
        self.auditor = audit.Audit(self.directory)

    def artifact(self, name):
        path = self.directory / "artifacts" / name
        path.mkdir(parents=True)
        self.auditor.artifacts.append({"name": name, "expired": False})
        return path

    def package(self):
        root = self.artifact("native-python-macos-15-3.14")
        source = root / "yamaa-0.3.0.tar.gz"
        with tarfile.open(source, "w:gz") as archive:
            for name in (
                "rust/Cargo.lock",
                "rust/Cargo.toml",
                "rust/rust-toolchain.toml",
                "python/uv.lock",
            ):
                data = (self.directory / "source" / name).read_bytes()
                member = tarfile.TarInfo("yamaa-0.3.0/" + name.removeprefix("python/"))
                member.size = len(data)
                archive.addfile(member, io.BytesIO(data))
        wheel = root / "yamaa-0.3.0-cp312-abi3-macosx_11_0_arm64.whl"
        with zipfile.ZipFile(wheel, "w") as archive:
            archive.writestr("yamaa/__init__.py", "# independent fixture\n")
        shutil.copyfile(
            wheel, self.artifact("native-reference-macos-15-3.14") / wheel.name
        )
        shutil.copyfile(
            wheel, self.artifact("native-project-macos-15-3.14") / "project.whl"
        )
        return root, wheel, source

    def installed(self):
        root, wheel, _ = self.package()
        directory = (
            self.artifact("native-qualification-macos-15-3.14-wheel")
            / "wheel/supplemental"
        )
        directory.mkdir(parents=True)
        record = {
            "evidence": {
                "source_revision": self.auditor.source["sha"],
                "artifact_reference": f"github-actions:38161000385/attempt-1/{root.name}/{wheel.name}",
                "package_form": "wheel",
                "run": self.auditor.run["html_url"],
            },
            "installation": {
                "runtime_version": "3.14.7",
                "host_package_version": "0.3.0",
                "core_version": "0.1.0",
                "package_location": "/installed/yamaa/_native.so",
            },
            "platform": "macOS-15.7-arm64",
            "suites": [
                {
                    "id": "python/original-specifications",
                    "result": "pass",
                    "exit_code": 0,
                    "log": "check.log",
                }
            ],
        }
        (directory / "check.log").write_text("independently authored successful log\n")
        audit.write(directory / "supplemental.json", record)
        return directory, record

    def test_discovery_enumerates_every_required_tuple_and_missing_never_passes(self):
        record = self.auditor.collect()
        self.assertEqual(
            Counter(row["kind"] for row in record["lanes"]),
            {
                "shared-crates": 3,
                "packages-python": 3,
                "packages-r": 3,
                "python": 18,
                "r": 3,
                "math": 9,
                "math-portability": 1,
                "public-conformance-python": 18,
                "public-conformance-r": 3,
                "public-cohort": 18,
                "producer": 18,
            },
        )
        self.assertFalse(record["complete"])
        self.assertFalse(any(row["result"] == "pass" for row in record["lanes"]))
        self.assertTrue(
            any(c["result"] == "missing" for r in record["lanes"] for c in r["checks"])
        )

    def test_artifact_directory_without_metadata_is_recorded_as_missing(self):
        directory = Path(self.temporary.name) / "artifact-only"
        directory.mkdir()
        record = audit.Audit(directory).collect()
        self.assertEqual(len(record["lanes"]), 97)
        self.assertFalse(record["complete"])
        self.assertIsNone(record["source"]["sha"])
        self.assertFalse(any(row["result"] == "pass" for row in record["lanes"]))
        self.assertTrue(all(check["result"] == "missing" for check in record["checks"]))

    def test_historical_windows_r_job_failure_cannot_be_upgraded(self):
        record = self.auditor.collect()
        windows = next(
            row
            for row in record["lanes"]
            if row["kind"] == "r" and row["os"] == "windows-latest"
        )
        self.assertEqual(windows["result"], "fail")
        self.assertIn("failure", windows["checks"][0]["detail"])
        self.assertIn("38161000385", audit.markdown(record))

    def test_wrong_source_and_sampled_runs_never_pass(self):
        for key, value in (("head_sha", "0" * 40), ("event", "pull_request")):
            with self.subTest(key=key):
                subject = audit.Audit(self.directory)
                subject.run[key] = value
                checks = []
                subject.check(checks, "source", subject.check_source)
                self.assertEqual(checks[0]["result"], "fail")

    def test_branch_evidence_never_claims_combined_main_scope(self):
        self.auditor.run["head_branch"] = "qualification/issue-1996"
        record = self.auditor.collect()
        self.assertEqual(record["qualification_scope"], "reviewed-branch")
        self.assertIn("Qualification scope: reviewed-branch", audit.markdown(record))
        self.assertFalse(record["complete"])
        record = audit.Audit(self.directory).collect()
        self.assertEqual(record["qualification_scope"], "combined-main")

    def test_same_package_bytes_pass_and_stale_cargo_bytes_fail(self):
        _, _, source = self.package()
        detail = self.auditor.package("macos-15", "python")
        self.assertEqual(detail["canonical_cargo_bytes"], "equal")
        (self.directory / "source/rust/Cargo.lock").write_text("stale lock\n")
        with self.assertRaisesRegex(ValueError, "canonical bytes"):
            self.auditor.package("macos-15", "python")
        self.assertTrue(source.is_file())

    def test_reference_wheel_bytes_and_python_cache_hygiene_are_checked(self):
        _, wheel, _ = self.package()
        reference = (
            self.directory / "artifacts/native-reference-macos-15-3.14" / wheel.name
        )
        reference.write_bytes(reference.read_bytes() + b"different bytes")
        with self.assertRaisesRegex(ValueError, "reference wheel differs"):
            self.auditor.package("macos-15", "python")
        shutil.copyfile(wheel, reference)
        with zipfile.ZipFile(wheel, "a") as archive:
            archive.writestr("yamaa/__pycache__/fixture.pyc", b"authored cache fixture")
        with self.assertRaisesRegex(ValueError, "bytecode|cache"):
            self.auditor.package("macos-15", "python")

    def test_installed_evidence_exact_revision_attempt_runtime_and_denominator(self):
        directory, original = self.installed()
        self.assertEqual(
            self.auditor.supplemental("macos-15", "python", "3.14", "wheel")["suites"],
            1,
        )
        for key, value in (
            ("source_revision", "0" * 40),
            (
                "artifact_reference",
                original["evidence"]["artifact_reference"].replace(
                    "attempt-1", "attempt-2"
                ),
            ),
            ("package_form", "source"),
        ):
            changed = copy.deepcopy(original)
            changed["evidence"][key] = value
            audit.write(directory / "supplemental.json", changed)
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.auditor.supplemental("macos-15", "python", "3.14", "wheel")
        changed = copy.deepcopy(original)
        changed["suites"] = []
        audit.write(directory / "supplemental.json", changed)
        with self.assertRaisesRegex(ValueError, "denominator"):
            self.auditor.supplemental("macos-15", "python", "3.14", "wheel")

    def test_skips_failed_suites_missing_logs_and_unsafe_logs_never_qualify(self):
        directory, original = self.installed()
        for change in ("skip", "failure", "missing", "escape"):
            changed = copy.deepcopy(original)
            log = directory / "check.log"
            log.write_text("OK\n")
            if change == "skip":
                log.write_text("OK (skipped=1)\n")
            elif change == "failure":
                changed["suites"][0]["result"] = "failure"
            elif change == "missing":
                log.unlink()
            else:
                changed["suites"][0]["log"] = "../coverage.json"
            audit.write(directory / "supplemental.json", changed)
            with self.subTest(change=change), self.assertRaises(ValueError):
                self.auditor.supplemental("macos-15", "python", "3.14", "wheel")

    def test_report_only_inventory_keeps_semantic_nonpasses_and_requires_all_records(
        self,
    ):
        root = self.artifact("public-conformance-python-macos-15-3.14-wheel")
        value = {
            "runtime_version": "3.14.7",
            "host_package_version": "0.3.0",
            "engine_version": "0.1.0",
            "platform": "macOS-15.7-arm64",
            "protocol": "public-conformance/1",
            "runtime": "python",
            "source_revision": self.auditor.source["sha"],
            "evidence": self.auditor.run["html_url"],
            "package_form": "wheel",
            "benchmarks": [
                {"example": "independently-authored", "verdict": "unsupported"}
            ],
            "summary": {"unsupported": 1},
            "unsupported_operations": {"authored-operation": 1},
        }
        audit.write(root / "inventory.json", value)
        result = self.auditor.public_inventory("macos-15", "python", "3.14", "wheel")
        self.assertEqual(result["summary"], {"unsupported": 1})
        value["benchmarks"] = []
        audit.write(root / "inventory.json", value)
        with self.assertRaisesRegex(ValueError, "denominator"):
            self.auditor.public_inventory("macos-15", "python", "3.14", "wheel")

    def test_r_public_inventory_accepts_native_platform_names_only_in_their_lane(self):
        platforms = {
            "ubuntu-24.04": "x86_64-pc-linux-gnu",
            "macos-15": "aarch64-apple-darwin23",
            "windows-latest": "x86_64-w64-mingw32",
        }
        for platform, recorded in platforms.items():
            root = self.artifact(f"public-conformance-r-{platform}")
            value = {
                "runtime_version": "4.6.1",
                "host_package_version": "0.3.0",
                "engine_version": "0.1.0",
                "platform": recorded,
                "protocol": "public-conformance/1",
                "runtime": "r",
                "source_revision": self.auditor.source["sha"],
                "evidence": self.auditor.run["html_url"],
                "package_form": "source",
                "benchmarks": [
                    {"example": "independently-authored", "verdict": "unsupported"}
                ],
                "summary": {"unsupported": 1},
                "unsupported_operations": {"authored": 1},
            }
            audit.write(root / "inventory.json", value)
            with self.subTest(platform=platform):
                self.assertEqual(
                    self.auditor.public_inventory(platform, "r", "4.6.1", "source")[
                        "summary"
                    ],
                    {"unsupported": 1},
                )
                value["platform"] = platforms[
                    "macos-15" if platform != "macos-15" else "ubuntu-24.04"
                ]
                audit.write(root / "inventory.json", value)
                with self.assertRaisesRegex(ValueError, "OS/image"):
                    self.auditor.public_inventory(platform, "r", "4.6.1", "source")

    def producer(self):
        root = (
            self.artifact("native-public-cohort")
            / "macos-15/3.14/wheel/producer-cohort"
        )
        root.mkdir(parents=True)
        names = (
            ("python", "python", "reference_run", "native-reference-macos-15-3.14"),
            ("python", "rust", "shared_run", "native-python-macos-15-3.14"),
            ("r", "rust", "shared_run", "native-r-macos-15"),
        )
        rows, batches = [], []
        for runtime, backend, level, artifact in names:
            (self.artifact(artifact) / "authored-package").write_bytes(
                b"package fixture"
            )
            rows.append(
                {
                    "example": "shared-producer-parquet",
                    "runtime": runtime,
                    "backend": backend,
                    "level": level,
                    "result": "pass",
                    "findings": [],
                    "error": None,
                }
            )
            batches.append(
                {
                    "runtime": runtime,
                    "backend": backend,
                    "level": level,
                    "source_revision": self.auditor.source["sha"],
                    "evidence": self.auditor.run["html_url"],
                    "artifact_reference": f"github-actions:38161000385/attempt-1/{artifact}/authored-package",
                    "host_package_version": "0.3.0",
                    "core_version": "0.1.0",
                }
            )
            report = (
                root
                / "reports"
                / runtime
                / f"shared-producer-parquet.{runtime}.{backend}.json"
            )
            audit.write(
                report,
                {
                    "example": "shared-producer-parquet",
                    "runtime": runtime,
                    "backend": backend,
                    "report_version": "0.3.0-draft",
                    "outcome": "success",
                    "error": None,
                    "engine_version": "0.3.0" if backend == "python" else "0.1.0",
                    "nodes": [],
                    "tables": [],
                    "verifications": [],
                    "callbacks": [],
                    "source_reads": [],
                    "artifacts": [],
                    "diagnostics": [],
                    "unsupported": [],
                    "handler_counts": [],
                },
            )
        audit.write(
            root / "coverage.json",
            {
                "source_revision": self.auditor.source["sha"],
                "errors": [],
                "coverage": rows,
                "batches": batches,
                "summary": {"pass": 3},
            },
        )
        audit.write(root / "complete-comparisons.json", [{"passed": True}] * 4)
        for runtime in ("python", "r"):
            owner = root / "raw" / runtime / "shared-producer-parquet"
            audit.write(owner / "before.json", {"authored-observation": [1, 2]})
            for attempt in (1, 2):
                saved = owner / f"after-{attempt}"
                audit.write(
                    saved / "report.json",
                    {
                        "authored-observation": [1, 2],
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
                published = root / "published" / runtime / str(attempt) / "expected"
                published.mkdir(parents=True)
                for name, payload in (
                    ("produced.parquet", b"independent held bytes"),
                    ("out.csv", b"I\n1\n"),
                ):
                    (saved / name).write_bytes(payload)
                    (published / name).write_bytes(payload)
        return root

    def test_producer_requires_both_save_attempts_and_preserves_actual_bytes(self):
        root = self.producer()
        self.assertEqual(
            self.auditor.cohort("macos-15", "3.14", "wheel", True)["required_tuples"], 3
        )
        saved = root / "raw/python/shared-producer-parquet/after-2/out.csv"
        saved.write_bytes(b"I\n2\n")
        with self.assertRaisesRegex(ValueError, "published producer bytes"):
            self.auditor.cohort("macos-15", "3.14", "wheel", True)
        (root / "published/python/2/expected/out.csv").write_bytes(saved.read_bytes())
        with self.assertRaisesRegex(ValueError, "repeat Save"):
            self.auditor.cohort("macos-15", "3.14", "wheel", True)

    def test_cohort_missing_route_or_wrong_batch_never_passes(self):
        root = self.producer()
        value = audit.read(root / "coverage.json")
        original = copy.deepcopy(value)
        value["coverage"].pop()
        audit.write(root / "coverage.json", value)
        with self.assertRaisesRegex(ValueError, "required tuples missing"):
            self.auditor.cohort("macos-15", "3.14", "wheel", True)
        value = original
        value["batches"][2] = copy.deepcopy(value["batches"][1])
        audit.write(root / "coverage.json", value)
        with self.assertRaisesRegex(ValueError, "batch routes differ"):
            self.auditor.cohort("macos-15", "3.14", "wheel", True)

    def test_duplicate_or_expired_artifact_cannot_supply_evidence(self):
        root = self.artifact("authored")
        self.auditor.artifacts[0]["expired"] = True
        with self.assertRaisesRegex(ValueError, "expired"):
            self.auditor.artifact(root.name)
        self.auditor.artifacts[0]["expired"] = False
        self.auditor.artifacts.append(dict(self.auditor.artifacts[0]))
        with self.assertRaisesRegex(ValueError, "duplicate"):
            self.auditor.artifact(root.name)

    def test_check_cli_preserves_json_markdown_and_replays_without_network(self):
        output = Path(self.temporary.name) / "output"
        with patch.object(
            audit, "gh_json", side_effect=AssertionError("offline audit used network")
        ):
            self.assertEqual(
                audit.main(
                    ["--input", str(self.directory), "--output", str(output), "--check"]
                ),
                1,
            )
        record = json.loads((output / "qualification.json").read_text())
        self.assertEqual(len(record["lanes"]), 97)
        self.assertIn("windows-latest", (output / "qualification.md").read_text())
        self.assertNotIn("digest", (output / "qualification.json").read_text())


if __name__ == "__main__":
    unittest.main()
