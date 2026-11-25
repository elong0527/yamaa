"""The case table behind the ordinal case-insensitive comparison of locations
(OUTPUT-9), generated from the pinned Unicode source beside it."""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.commands import repository
from tools.packaging import generate_case_table as generator
from tools.tests import support

# Three rows of a published UnicodeData.txt: one that states a simple
# uppercase, one that states none, and the dotless i, which the table leaves
# unmapped.
PUBLISHED = (
    "0061;LATIN SMALL LETTER A;Ll;0;L;;;;;N;;;0041;;0041\n"
    "0041;LATIN CAPITAL LETTER A;Lu;0;L;;;;;N;;;;0061;\n"
    "0131;LATIN SMALL LETTER DOTLESS I;Ll;0;L;;;;;N;;;0049;;0049\n"
    "10428;DESERET SMALL LETTER LONG I;Ll;0;L;;;;;N;;;10400;;10400\n"
)


# The table's Cyrillic rows under U+0422 and the Windows record of them: one
# class of three, recorded pairwise, as the oracle printed it.
TE = [(0x0442, 0x0422), (0x1C84, 0x0422), (0x1C85, 0x0422)]
TE_RECORD = (
    "# a comment\n"
    "Windows separates U+0422/U+0442 from U+1C84\n"
    "Windows separates U+1C84 from U+1C85\n"
)


class CaseTableTests(unittest.TestCase):
    def test_the_committed_files_are_the_pinned_source(self):
        self.assertEqual(
            {path: path.read_bytes() for path in generator.expected()},
            generator.expected(),
        )

    def test_the_repository_checks_run_it(self):
        self.assertIn("tools.packaging.generate_case_table", repository.CHECKS)

    def test_the_cut_keeps_each_stated_uppercase_and_the_table_drops_the_unmapped(
        self,
    ):
        source = generator.cut(PUBLISHED)
        self.assertEqual(
            (source[len(generator.SOURCE_HEADER) :], generator.table(source)),
            ("0061;0041\n0131;0049\n10428;10400\n", [(0x61, 0x41), (0x10428, 0x10400)]),
            "the cut and the table of three published rows",
        )

    def test_each_recorded_class_without_the_shared_uppercase_is_excepted(self):
        self.assertEqual(
            generator.windows_unmapped(TE, TE_RECORD),
            [0x1C84, 0x1C85],
            "a class of three under U+0422, recorded pairwise",
        )

    def test_the_derivation_refuses_a_record_the_table_does_not_answer(self):
        for label, record in (
            ("a joined class", "Windows joins U+0422/U+1C84\n"),
            ("two uppercases", "Windows separates U+0422/U+0442 from U+0041\n"),
        ):
            with self.subTest(case=label):
                with self.assertRaises(ValueError, msg=label):
                    generator.windows_unmapped(TE, record)

    def test_a_hand_edit_of_the_windows_exceptions_fails_the_check(self):
        with tempfile.TemporaryDirectory() as scratch:
            committed = generator.WINDOWS_TARGET.read_text(encoding="ascii")
            target = support.write(
                scratch, "windows-unmapped.txt", committed.replace("03C2\n", "")
            )
            with mock.patch.object(generator, "WINDOWS_TARGET", Path(target)):
                status, _, errors = support.run_main(generator.main)
            self.assertEqual(
                (status, "windows-unmapped.txt differs from Unicode" in errors),
                (1, True),
                "a deleted Windows exception",
            )

    def test_a_hand_edit_of_the_table_fails_the_check(self):
        with tempfile.TemporaryDirectory() as scratch:
            committed = generator.TARGET.read_text(encoding="ascii")
            for label, edited in (
                ("a changed mapping", committed.replace("0061 0041\n", "0061 0042\n")),
                ("a deleted line", committed.replace("0062 0042\n", "")),
            ):
                with self.subTest(case=label):
                    target = support.write(scratch, "uppercase.txt", edited)
                    with mock.patch.object(generator, "TARGET", Path(target)):
                        status, _, errors = support.run_main(generator.main)
                    self.assertEqual(
                        (status, "uppercase.txt differs from Unicode 15.0.0" in errors),
                        (1, True),
                        label,
                    )


if __name__ == "__main__":
    unittest.main()
