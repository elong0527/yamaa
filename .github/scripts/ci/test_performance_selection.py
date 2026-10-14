"""The expensive performance route covers effective code, excluding evidence."""

import re
import unittest
from pathlib import Path

import yaml


class PerformanceSelection(unittest.TestCase):
    def test_runtime_protocol_truth_and_admission_changes_require_collection(self):
        root = Path(__file__).resolve().parents[3]
        workflow = yaml.safe_load(
            (root / ".github/workflows/performance.yml").read_text()
        )
        events = workflow.get("on", workflow.get(True))
        self.assertEqual(events["push"]["branches"], ["qualification/issue-1998"])
        patterns = events["push"]["paths"]

        def selected(path):
            included = False
            for pattern in patterns:
                exclusion = pattern.startswith("!")
                # GitHub's '*' excludes separators; '**' includes them, and
                # '**/' also matches zero intervening directories.
                expression = re.escape(pattern.removeprefix("!"))
                expression = expression.replace(r"\*\*/", "(?:.*/)?")
                expression = expression.replace(r"\*\*", ".*")
                expression = expression.replace(r"\*", "[^/]*")
                if re.fullmatch(expression, path):
                    included = not exclusion
            return included

        for path in (
            "rust/crates/yamaa-core/src/table.rs",
            "rust/crates/yamaa-engine/src/capacity.rs",
            "rust/crates/yamaa-adapters/src/producer_report.rs",
            "rust/crates/yamaa-python/src/domain_entry.rs",
            "rust/crates/yamaa-r/src/domain_entry.rs",
            "rust/crates/yamaa-adapters/tests/fixtures/source.md",
            "rust/Cargo.toml",
            "rust/Cargo.lock",
            "rust/rust-toolchain.toml",
            "rust/tools/collect_public_performance.py",
            "rust/tools/admit_performance_native.py",
            "rust/tools/prepare_public_workloads.py",
            "rust/tools/public_lifecycle_python_worker.py",
            "rust/tools/public_lifecycle_r_worker.R",
            "rust/tools/public_lifecycle_worker_protocol.json",
            "rust/tools/performance_campaign.py",
            "python/src/yamaa/_domain.py",
            "python/pyproject.toml",
            "python/uv.lock",
            "R/yamaa/R/domain.R",
            "R/yamaa/DESCRIPTION",
            "yaml/schema/dataset.json",
            "benchmarks/schema-lookup/spec.yaml",
            ".github/workflows/performance.yml",
            ".github/workflows/native.yml",
            ".github/scripts/ci/native_matrix.py",
        ):
            with self.subTest(path=path):
                self.assertTrue(
                    selected(path), "effective change would skip collection"
                )
        for path in (
            "rust/measurements/accepted-matrix.json",
            "rust/measurements/1998-report.md",
            "rust/planning/maintainer-decisions.md",
            "rust/README.md",
            "python/README.md",
            "R/yamaa/README.md",
            "benchmarks/schema-lookup/README.md",
            "docs/index.md",
        ):
            with self.subTest(path=path):
                self.assertFalse(selected(path), "evidence commit would recollect")
