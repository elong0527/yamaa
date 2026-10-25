"""The copied test support modules stay equal to their owners' modules."""

import tempfile
import unittest
from pathlib import Path

from tools.validation import check_test_support as support


class CopiedSupportModules(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory(prefix="yamaa-test-support-")
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        for owner, copy, names in support.COPIES:
            for name in names:
                for tree in (owner, copy):
                    path = self.root / tree / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text(f"// {name}\n", encoding="utf-8")

    def test_equal_copies_pass(self):
        self.assertEqual(support.check(self.root), [])

    def test_a_changed_copy_names_both_files(self):
        (self.root / support.APPLICATION / "parquet_files.rs").write_text(
            "// drifted\n"
        )
        self.assertEqual(
            support.check(self.root),
            [
                f"{support.APPLICATION / 'parquet_files.rs'} differs from "
                f"{support.ADAPTERS / 'parquet_files.rs'}; make the same change "
                "to both"
            ],
        )

    def test_a_missing_module_is_reported_for_each_copy(self):
        (self.root / support.CORE / "document.rs").unlink()
        self.assertEqual(
            support.check(self.root),
            [f"{support.CORE / 'document.rs'} is missing"] * 2,
        )

    def test_the_checkout_keeps_its_copies_equal(self):
        self.assertEqual(support.check(support.ROOT), [])


if __name__ == "__main__":
    unittest.main()
