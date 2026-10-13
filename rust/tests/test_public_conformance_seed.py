"""Independent full-matrix ratchet controls, without any candidate evaluator."""

import contextlib
import copy
import importlib.util
import io
import sys
import tempfile
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
spec = importlib.util.spec_from_file_location(
    "seed_public_conformance", TOOLS / "seed_public_conformance.py"
)
seeder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(seeder)
import public_conformance as judge


class RatchetSeed(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.fixtures = self.root / "fixtures"
        self.artifacts = self.root / "artifacts"
        self.output = self.root / "ratchet.json"
        self.run = {
            "id": 42,
            "run_attempt": 1,
            "status": "completed",
            "conclusion": "success",
            "event": "workflow_dispatch",
            "head_branch": "candidate",
            "head_sha": "f" * 40,
            "path": ".github/workflows/native.yml",
            "html_url": "https://github.com/example/yamaa/actions/runs/42",
        }
        self.run_file = self.root / "run.json"
        judge.write_json(self.run_file, self.run)
        for name in ("stable", "new"):
            case = self.fixtures / name
            (case / "expected").mkdir(parents=True)
            (case / "spec.yaml").write_text(
                "domain: TEST\ninput: {SRC: input.csv}\nkeys: [ID]\n"
                "columns: [{name: ID, type: int, derivation: SRC.ID}]\n"
                "output: {path: output.csv, columns: [ID]}\n"
            )
            (case / "run.py").write_text(
                'import yamaa\nx = yamaa.domain("spec.yaml")\n'
            )
            (case / "run.R").write_text('x <- yamaa_domain("spec.yaml")\n')
            (case / "expected/output.csv").write_bytes(b"ID\n7\n")
        platforms = {
            "python": {
                "ubuntu-24.04": "Linux-x86_64",
                "macos-15": "macOS-15-arm64",
                "windows-latest": "Windows-11-AMD64",
            },
            "r": {
                "ubuntu-24.04": "x86_64-pc-linux-gnu",
                "macos-15": "aarch64-apple-darwin23",
                "windows-latest": "x86_64-w64-mingw32",
            },
        }
        for name, (runtime, platform, version, form) in seeder.lanes():
            root = self.artifacts / name
            observed = root / "observed"
            judge.write_json(
                observed / "run.json",
                {
                    "protocol": judge.PROTOCOL,
                    "runtime": runtime,
                    "runtime_version": version + ".8"
                    if runtime == "python"
                    else version,
                    "host_package_version": "0.4.0",
                    "engine_version": "0.4.0",
                    "source_revision": self.run["head_sha"],
                    "evidence": self.run["html_url"],
                    "platform": platforms[runtime][platform],
                    "package_form": form,
                },
            )
            for case in ("stable", "new"):
                candidate = observed / "work" / case / "output.csv"
                candidate.parent.mkdir(parents=True)
                candidate.write_bytes(b"ID\n7\n")
                judge.write_json(
                    observed / "records" / (case + ".json"),
                    {
                        "builds": [
                            {
                                "specification": "spec.yaml",
                                "environment": None,
                                "issues": [],
                                "saved": True,
                                "tables": {
                                    "output": {
                                        "columns": [["ID", "int"]],
                                        "rows": [["7"]],
                                    },
                                    "verification_log": None,
                                    "warning_log": None,
                                },
                            }
                        ]
                    },
                )
            self.compare(root)

    def compare(self, root):
        with contextlib.redirect_stdout(io.StringIO()):
            judge.compare(self.fixtures, root / "observed", root / "inventory.json")

    def seed(self, previous=None):
        return seeder.seed(
            self.fixtures, self.artifacts, self.run_file, self.output, previous
        )

    def test_intersection_needs_every_lane_of_each_host(self):
        root = self.artifacts / "public-conformance-r-windows-latest"
        (root / "observed/work/new/output.csv").write_bytes(b"ID\n8\n")
        self.compare(root)
        value = self.seed()
        self.assertEqual(value["python"], ["new", "stable"])
        self.assertEqual(value["r"], ["stable"])
        self.assertEqual(len(value["inventories"]), 21)
        self.assertEqual(value["source_revision"], self.run["head_sha"])

    def test_sampled_running_or_wrong_workflow_run_cannot_seed(self):
        for fields in (
            {"event": "pull_request"},
            {"status": "in_progress"},
            {"conclusion": "failure"},
            {"conclusion": "cancelled"},
            {"conclusion": None},
            {"event": "push", "head_branch": "other"},
            {"path": ".github/workflows/python.yml"},
        ):
            judge.write_json(self.run_file, {**self.run, **fields})
            with (
                self.subTest(fields=fields),
                self.assertRaisesRegex(ValueError, "full native"),
            ):
                self.seed()
            self.assertFalse(self.output.exists())

    def test_missing_or_unexpected_lane_cannot_seed(self):
        root = self.artifacts / "public-conformance-r-windows-latest"
        root.rename(self.artifacts / "public-conformance-r-unapproved")
        with self.assertRaisesRegex(ValueError, "lane set"):
            self.seed()
        self.assertFalse(self.output.exists())

    def test_mixed_revision_runtime_form_versions_or_evidence_cannot_seed(self):
        path = (
            self.artifacts
            / "public-conformance-python-windows-latest-3.12-source/inventory.json"
        )
        original = judge.read_json(path)
        for fields in (
            {"source_revision": "a" * 40},
            {"runtime": "r"},
            {"package_form": "wheel"},
            {"runtime_version": "3.14.8"},
            {"evidence": "https://github.com/example/yamaa/actions/runs/43"},
            {"platform": "macOS-15-arm64"},
        ):
            judge.write_json(path, {**original, **fields})
            with self.subTest(fields=fields), self.assertRaises(ValueError):
                self.seed()
            self.assertFalse(self.output.exists())

    def test_forged_pass_or_changed_saved_bytes_cannot_seed(self):
        root = self.artifacts / "public-conformance-r-macos-15"
        (root / "observed/work/stable/output.csv").write_bytes(b"ID\n8\n")
        with self.assertRaisesRegex(ValueError, "independent replay"):
            self.seed()
        self.assertFalse(self.output.exists())

    def test_duplicate_benchmark_cannot_hide_missing_truth(self):
        path = self.artifacts / "public-conformance-r-macos-15/inventory.json"
        value = judge.read_json(path)
        value["benchmarks"][0] = copy.deepcopy(value["benchmarks"][1])
        judge.write_json(path, value)
        with self.assertRaisesRegex(ValueError, "independent replay"):
            self.seed()

    def test_prior_ratchet_never_shrinks_and_existing_output_is_preserved(self):
        previous = self.root / "previous.json"
        judge.write_json(previous, {"python": ["stable"], "r": ["unobserved"]})
        with self.assertRaisesRegex(ValueError, "shrink"):
            self.seed(previous)
        self.assertFalse(self.output.exists())
        self.output.write_text("retained bytes\n")
        with self.assertRaisesRegex(ValueError, "fresh file"):
            self.seed()
        self.assertEqual(self.output.read_text(), "retained bytes\n")


if __name__ == "__main__":
    unittest.main()
