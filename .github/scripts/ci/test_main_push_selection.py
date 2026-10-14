"""Main push filters retain executable inputs while skipping prose-only work."""

import ast
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


def push_selected(workflow, path):
    source = (ROOT / ".github/workflows" / workflow).read_text()
    push = re.search(r"(?ms)^  push:\n(.*?)(?=^  \S|\Z)", source).group(1)
    patterns = [
        ast.literal_eval(value) for value in re.findall(r"(?m)^      - (.+)$", push)
    ]
    selected = False
    for pattern in patterns:
        expression = re.escape(pattern.removeprefix("!"))
        expression = expression.replace(r"\*\*/", "(?:.*/)?")
        expression = expression.replace(r"\*\*", ".*")
        expression = expression.replace(r"\*", "[^/]*")
        if re.fullmatch(expression, path):
            selected = not pattern.startswith("!")
    return selected


class MainPushSelection(unittest.TestCase):
    def test_unit_tests_keep_installed_dependencies_and_authored_test_inputs(self):
        for path in (
            "docs/requirements.txt",
            "docs/assets/giscus-yamaa.css",
            "rules/specification/style.md",
            "rust/crates/yamaa-adapters/tests/fixtures/source.md",
            "rust/tools/public_conformance.py",
            "python/uv.lock",
            "R/yamaa/R/domain.R",
            "yaml/schema/dataset.json",
            "benchmarks/schema-functions/README.md",
            ".github/scripts/ci/test_main_push_selection.py",
            ".github/workflows/unit-tests.yml",
        ):
            with self.subTest(path=path):
                self.assertTrue(push_selected("unit-tests.yml", path))
        for path in (
            "README.md",
            "docs/development.md",
            "rules/README.md",
            "rules/values/types.md",
            "rust/README.md",
            "mkdocs.yml",
        ):
            with self.subTest(path=path):
                self.assertFalse(push_selected("unit-tests.yml", path))

    def test_smoke_cache_refresh_keeps_manifest_toolchain_and_dependency_inputs(self):
        for path in (
            "rust/Cargo.lock",
            "rust/Cargo.toml",
            "rust/crates/yamaa-core/Cargo.toml",
            "rust/rust-toolchain.toml",
            ".github/scripts/ci/requirements-pr.txt",
            ".github/workflows/pr.yml",
        ):
            with self.subTest(path=path):
                self.assertTrue(push_selected("pr.yml", path))
        for path in ("README.md", "rust/crates/yamaa-core/src/lib.rs"):
            with self.subTest(path=path):
                self.assertFalse(push_selected("pr.yml", path))


if __name__ == "__main__":
    unittest.main()
