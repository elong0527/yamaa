import unittest

from dashboard_changes import affects_dashboard


class DashboardChanges(unittest.TestCase):
    def test_source_changes_and_deletions_need_preview(self):
        for path in (
            "benchmarks/schema-functions/input/data.csv",
            "benchmarks/schema-functions/README.md",
            "evaluations/harbor/prompts/schema-functions/full.md",
            "evaluations/harbor/solutions/schema-functions/result.R",
            ".github/scripts/benchmark-docs/generate.py",
            ".github/scripts/ci/dashboard_changes.py",
            ".github/workflows/yaml-validation.yml",
            "docs/articles/benchmark.md",
        ):
            self.assertTrue(affects_dashboard(path), path)

    def test_runtime_and_unrelated_docs_do_not_need_preview(self):
        for path in ("rust/Cargo.toml", "python/src/yamaa/style.py", "docs/index.md"):
            self.assertFalse(affects_dashboard(path), path)


if __name__ == "__main__":
    unittest.main()
