"""The adapters' copies of core's test support modules stay equal to core's."""

import tempfile
import unittest
from pathlib import Path

from tools.validation import check_test_support as support


class CopiedSupportModules(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory(prefix="yamaa-test-support-")
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        for name in support.SHARED:
            for tree in (support.OWNER, support.COPY):
                path = self.root / tree / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(f"// {name}\n", encoding="utf-8")

    def test_equal_copies_pass(self):
        self.assertEqual(support.check(self.root), [])

    def test_a_changed_copy_names_both_files(self):
        (self.root / support.COPY / "roots.rs").write_text("// drifted\n")
        self.assertEqual(
            support.check(self.root),
            [
                f"{support.COPY / 'roots.rs'} differs from "
                f"{support.OWNER / 'roots.rs'}; make the same change to both"
            ],
        )

    def test_a_missing_module_is_reported(self):
        (self.root / support.OWNER / "document.rs").unlink()
        self.assertEqual(
            support.check(self.root),
            [f"{support.OWNER / 'document.rs'} is missing"],
        )

    def test_the_checkout_keeps_its_copies_equal(self):
        self.assertEqual(support.check(support.ROOT), [])


if __name__ == "__main__":
    unittest.main()
