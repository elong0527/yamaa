"""The expensive performance route covers effective code, excluding evidence."""

import ast
import re
import unittest
from pathlib import Path


class PerformanceSelection(unittest.TestCase):
    def test_all_long_producers_wait_for_original_three_platform_r_preflight(self):
        root = Path(__file__).resolve().parents[3]
        source = (root / ".github/workflows/performance.yml").read_text()
        preflight = re.search(r"(?ms)^  r_preflight:\n(.*?)(?=^  \S|\Z)", source).group(
            1
        )
        self.assertIn("os: [ubuntu-24.04, macos-15, windows-latest]", preflight)
        self.assertIn("r-version: '4.6.1'", preflight)
        self.assertIn(
            "R_LIBS: ${{ runner.temp }}/performance-r-preflight-library", preflight
        )
        self.assertIn("--no-emit-project", preflight)
        self.assertIn("jsonlite/tar.gz/refs/tags/2.0.0", preflight)
        self.assertIn("not result.skipped", preflight)
        self.assertIn("rust/tools/check_r_child_peak_rss.py", preflight)
        self.assertNotIn("cargo ", preflight)
        self.assertNotIn("candidate-distribution", preflight)
        for host in ("python", "r"):
            job = re.search(rf"(?ms)^  {host}:\n(.*?)(?=^  \S|\Z)", source).group(1)
            self.assertIn("needs: [native, r_preflight]", job)
        retain = re.search(r"(?ms)^  retain:\n(.*?)(?=^  \S|\Z)", source).group(1)
        self.assertIn("timeout-minutes: 360", retain)

    def test_r_dimension_jobs_keep_all_sizes_and_complete_case_distributions(self):
        root = Path(__file__).resolve().parents[3]
        source = (root / ".github/workflows/performance.yml").read_text()
        r_job = re.search(r"(?ms)^  r:\n(.*?)(?=^  \S|\Z)", source).group(1)
        variants = re.search(r"(?m)^        variant: \[(.*?)\]$", r_job).group(1)
        self.assertEqual(
            [part.strip() for part in variants.split(",")],
            [
                "lookup-1",
                "lookup-4",
                "aggregate-10",
                "aggregate-100",
                "window-10",
                "window-100",
                "callback-1",
                "producer-1",
                "producer-3",
            ],
        )
        self.assertIn("variant_cases(os.environ['PERFORMANCE_VARIANT'])", r_job)
        self.assertIn("pattern='test_public_lifecycle_view_retention.py'", r_job)
        self.assertIn(
            "result.testsRun==expected and result.wasSuccessful() and not result.skipped",
            r_job,
        )
        self.assertIn("library(jsonlite, lib.loc=root)", r_job)
        self.assertIn('packageVersion("jsonlite")) == "2.0.0"', r_job)
        self.assertIn("jsonlite/tar.gz/refs/tags/2.0.0", r_job)
        self.assertIn("r-retention-controls.json", r_job)
        self.assertIn("rust/tools/check_r_child_peak_rss.py", r_job)
        self.assertIn("r-child-rss-controls", r_job)
        self.assertEqual(r_job.count('--cases "${case_args[@]}"'), 3)
        self.assertIn(
            "name: performance-r-${{ matrix.os }}-source-${{ matrix.variant }}", r_job
        )
        self.assertIn(
            "name: performance-r (${{ matrix.os }}, source, ${{ matrix.variant }})",
            r_job,
        )
        python_job = re.search(r"(?ms)^  python:\n(.*?)(?=^  \S|\Z)", source).group(1)
        self.assertIn("variant_cases(os.environ['PERFORMANCE_VARIANT'])", python_job)
        self.assertEqual(python_job.count('--cases "${case_args[@]}"'), 3)
        self.assertIn("${{ matrix.form }}-${{ matrix.variant }}", python_job)
        self.assertIn("${{ matrix.form }}, ${{ matrix.variant }})", python_job)
        self.assertIn("--r-variants --python-variants", source)
        self.assertIn("python: ['3.12', '3.13', '3.14']", python_job)
        self.assertIn("form: [wheel, source]", python_job)

    def test_collection_completion_and_numeric_replay_are_separate_routes(self):
        root = Path(__file__).resolve().parents[3]
        campaign = (root / ".github/workflows/performance.yml").read_text()
        self.assertNotRegex(campaign, r"(?m)^  audit:")
        self.assertIn("needs: [native, r_preflight, python, r, index]", campaign)
        replay = (root / ".github/workflows/performance-replay.yml").read_text()
        push = re.search(r"(?ms)^  push:\n(.*?)(?=^  \S|\Z)", replay).group(1)
        self.assertIn("branches: [evidence/issue-1998-replay]", push)
        self.assertNotIn("evidence/issue-1998-replay", campaign)
        self.assertIn("types: [completed]", replay)
        self.assertIn("github.repository == 'elong0527/yamaa'", replay)
        self.assertIn("github.event.workflow_run.event == 'push'", replay)
        self.assertIn(
            "github.event.workflow_run.head_branch == 'qualification/issue-1998'",
            replay,
        )
        self.assertIn("ref: ${{ github.sha }}", replay)
        self.assertIn("persist-credentials: false", replay)
        self.assertIn("name: performance-replay-unit (${{ matrix.key }})", replay)
        self.assertIn("name: performance-unit-${{ matrix.key }}", replay)
        self.assertIn("include: ${{ fromJSON(needs.prepare.outputs.units) }}", replay)
        self.assertIn(
            "artifact-ids: ${{ needs.prepare.outputs.input-artifact-id }}", replay
        )
        self.assertIn("--mode aggregate", replay)
        self.assertIn("--mode unit", replay)
        self.assertIn("needs.prepare.outputs.mode == 'units'", replay)

    def test_runtime_protocol_truth_and_admission_changes_require_collection(self):
        root = Path(__file__).resolve().parents[3]
        # This CI selection suite deliberately runs in the setup interpreter,
        # before project dependencies are selected. Read this workflow's explicit
        # quoted scalar list with stdlib only; GitHub validates the whole YAML.
        source = (root / ".github/workflows/performance.yml").read_text()
        push = re.search(r"(?ms)^  push:\n(.*?)(?=^  \S|\Z)", source).group(1)
        branches = re.search(r"(?m)^    branches: \[(.*?)\]$", push).group(1)
        self.assertEqual(
            branches, "qualification/issue-1998, qualification/issue-2142-frontier"
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
            "rust/tools/index_public_performance.py",
            "rust/tools/retain_public_performance.py",
            "rust/tools/replay_public_performance.py",
            "rust/tools/audit_public_performance.py",
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
            "rust/qualification/performance-campaign.json",
            "rust/qualification/performance-budgets.json",
            "rust/README.md",
            "python/README.md",
            "R/yamaa/README.md",
            "benchmarks/schema-lookup/README.md",
            "docs/index.md",
        ):
            with self.subTest(path=path):
                self.assertFalse(selected(path), "evidence commit would recollect")
