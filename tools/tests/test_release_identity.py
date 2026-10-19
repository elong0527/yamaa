"""One release identity, written once, spelled for each package format."""

import tempfile
import unittest
from pathlib import Path

from tools.packaging import release_identity
from tools.packaging.release_identity import Identity
from tools.paths import ROOT


class Spellings(unittest.TestCase):
    def test_a_final_release_has_one_spelling_everywhere(self):
        identity = Identity("1.0.0")
        self.assertEqual(
            (identity.python, identity.cargo, identity.r, identity.tag),
            ("1.0.0", "1.0.0", "1.0.0", "yamaa-v1.0.0"),
        )
        self.assertIsNone(identity.previous())
        self.assertEqual(str(identity), "1.0.0")

    def test_a_candidate_keeps_the_final_r_version(self):
        # #3030 D53: an R version cannot mark a candidate.
        identity = Identity("1.0.0", 2)
        self.assertEqual(
            (identity.python, identity.cargo, identity.r, identity.tag),
            ("1.0.0rc2", "1.0.0-rc.2", "1.0.0", "yamaa-v1.0.0-rc.2"),
        )
        self.assertEqual(identity.previous(), Identity("1.0.0", 1))
        self.assertIsNone(Identity("1.0.0", 1).previous())
        self.assertEqual(str(identity), "1.0.0 release candidate 2")

    def test_only_x_y_z_and_positive_candidates_are_identities(self):
        for version, candidate in [
            ("1.0", None),
            ("1.0.0.0", None),
            ("01.0.0", None),
            ("1.0.0rc1", None),
            ("1.0.0-rc.1", None),
            (" 1.0.0", None),
            (100, None),
            ("1.0.0", 0),
            ("1.0.0", -1),
            ("1.0.0", True),
            ("1.0.0", 1.0),
            ("1.0.0", "1"),
        ]:
            with (
                self.subTest(version=version, candidate=candidate),
                self.assertRaises(ValueError),
            ):
                Identity(version, candidate)


class Tags(unittest.TestCase):
    def test_final_and_candidate_tags_name_their_identity(self):
        for tag, identity in [
            ("yamaa-v1.0.0", Identity("1.0.0")),
            ("yamaa-v1.0.0-rc.1", Identity("1.0.0", 1)),
            ("yamaa-v10.20.30-rc.12", Identity("10.20.30", 12)),
        ]:
            with self.subTest(tag=tag):
                self.assertEqual(release_identity.from_tag(tag), identity)
                self.assertEqual(identity.tag, tag)

    def test_every_other_name_is_refused(self):
        for tag in [
            "v1.0.0",
            "yamaa-1.0.0",
            "yamaa-v1.0",
            "yamaa-v01.0.0",
            "yamaa-v1.0.0rc1",
            "yamaa-v1.0.0-rc1",
            "yamaa-v1.0.0-rc.0",
            "yamaa-v1.0.0-rc.01",
            "yamaa-v1.0.0-beta.1",
            "yamaa-v1.0.0-rc.1-extra",
            "yamaa-py-0.3.0",
            "",
            None,
        ]:
            with (
                self.subTest(tag=tag),
                self.assertRaisesRegex(ValueError, "is not a release tag"),
            ):
                release_identity.from_tag(tag)


class Declaration(unittest.TestCase):
    def read(self, text):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            if text is not None:
                path = root / release_identity.DECLARATION
                path.parent.mkdir(parents=True)
                path.write_text(text, encoding="utf-8")
            return release_identity.read(root)

    def test_the_declaration_names_a_final_release_or_a_candidate(self):
        self.assertEqual(self.read('version = "1.0.0"\n'), Identity("1.0.0"))
        self.assertEqual(
            self.read('version = "1.0.0"\ncandidate = 3\n'), Identity("1.0.0", 3)
        )

    def test_the_repository_declares_a_coherent_identity(self):
        self.assertIsInstance(release_identity.read(ROOT), Identity)

    def test_a_missing_unreadable_or_other_shaped_declaration_is_refused(self):
        for text, error in [
            (None, "unreadable"),
            ("version = ", "unreadable"),
            ("candidate = 1\n", "declare `version`"),
            ('version = "1.0.0"\nname = "yamaa"\n', "nothing else"),
            ('version = "1.0.0rc1"\n', "must be X.Y.Z"),
            ('version = "1.0.0"\ncandidate = 0\n', "positive integer"),
            ('version = "1.0.0"\ncandidate = "1"\n', "positive integer"),
        ]:
            with self.subTest(text=text), self.assertRaisesRegex(ValueError, error):
                self.read(text)


if __name__ == "__main__":
    unittest.main()
