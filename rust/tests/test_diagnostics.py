"""The diagnostic requirement guard fails closed for unknown or missing rules."""

import importlib.util
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "check_diagnostics", Path(__file__).parents[1] / "tools/check_diagnostics.py"
)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


class DiagnosticGuardTests(unittest.TestCase):
    def test_known_requirement_then_unknown_adapter_mapping(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            rules = root / "rules" / "values.md"
            rules.parent.mkdir()
            rules.write_text("**REQ-0001.** A normative requirement.\n")
            source = root / "rust/crates/yamaa-adapters/src/nested/errors.rs"
            source.parent.mkdir(parents=True)
            source.write_text('const REQUIREMENT: &str = "REQ-0001";\n')
            self.assertEqual(guard.violations(root), [])
            source.write_text('const REQUIREMENT: &str = "REQ-999999";\n')
            self.assertEqual(
                guard.violations(root),
                [
                    "rust/crates/yamaa-adapters/src/nested/errors.rs:1: "
                    "undefined requirement REQ-999999"
                ],
            )

    def test_missing_rules_and_sources_do_not_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.assertEqual(
                guard.violations(root), ["no normative requirement definitions found"]
            )
            (root / "rules").mkdir()
            (root / "rules/rule.md").write_text("**REQ-0001.** Required.\n")
            self.assertEqual(guard.violations(root), ["no Rust crate sources found"])

    def test_retired_citation_is_not_a_definition(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "rules").mkdir()
            (root / "rules/rule.md").write_text(
                "**REQ-0001.** Required.\nRetired REQ-0002 is historical.\n"
            )
            source = root / "rust/crates/yamaa-core/src/diagnostic.rs"
            source.parent.mkdir(parents=True)
            source.write_text('const REQUIREMENT: &str = "REQ-0002";\n')
            self.assertIn("undefined requirement REQ-0002", guard.violations(root)[0])


if __name__ == "__main__":
    unittest.main()
