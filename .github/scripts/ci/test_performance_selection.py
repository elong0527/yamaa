"""The expensive performance route covers effective code, excluding evidence."""

import ast
import re
import unittest
from pathlib import Path


class PerformanceSelection(unittest.TestCase):
    def test_runtime_protocol_truth_and_admission_changes_require_collection(self):
        root = Path(__file__).resolve().parents[3]
        # This CI selection suite deliberately runs in the setup interpreter,
        # before project dependencies are selected. Read this workflow's explicit
        # quoted scalar list with stdlib only; GitHub validates the whole YAML.
        source = (root / ".github/workflows/performance.yml").read_text()
        push = re.search(r"(?ms)^  push:\n(.*?)(?=^  \S|\Z)", source).group(1)
        branches = re.search(r"(?m)^    branches: \[(.*?)\]$", push).group(1)
        self.assertEqual(
            branches,
            "qualification/issue-1998, qualification/issue-2288-linux-performance",
        )
        patterns = [
            ast.literal_eval(value) for value in re.findall(r"(?m)^      - (.+)$", push)
        ]
        self.assertTrue(patterns)
        self.assertTrue(all(isinstance(pattern, str) for pattern in patterns))

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
            "rust/README.md",
            "python/README.md",
            "R/yamaa/README.md",
            "benchmarks/schema-lookup/README.md",
            "docs/index.md",
        ):
            with self.subTest(path=path):
                self.assertFalse(selected(path), "evidence commit would recollect")

    def test_linux_scope_retains_native_admission_and_excludes_legacy_baselines(self):
        root = Path(__file__).resolve().parents[3]
        source = (root / ".github/workflows/performance.yml").read_text()
        self.assertIn("refs/heads/qualification/issue-2288-linux-performance", source)
        self.assertIn("options: [full-matrix, linux-installed]", source)
        self.assertEqual(source.count('--collection-scope "${COLLECTION_SCOPE}"'), 2)
        self.assertIn("--collection-scope linux-installed --check-collection", source)
        self.assertEqual(source.count("if: env.COLLECTION_SCOPE == 'full-matrix'"), 5)
        self.assertIn("needs: [native, python, r]", source)
        self.assertIn("admit_performance_native.py", source)
        self.assertIn("r-version: '4.6.1'", source)
        self.assertIn("--source-bundle source.bundle", source)
