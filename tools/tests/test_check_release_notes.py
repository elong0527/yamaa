"""The unnumbered decisions of released notes: a "D-next" in the notes of the
version tools/packaging/release.toml names, or an earlier one, fails, the
notes of a later version may cite one, and an allowlist entry holds only
while its issue is open and its notes still cite one."""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.commands import repository
from tools.tests.support import run_main, write
from tools.validation import check_release_notes as notes

DECLARATION = Path("tools/packaging/release.toml")
EARLIER = Path("docs/releases/0.8.0.md")
DECLARED = Path("docs/releases/0.9.0.md")
LATER = Path("docs/releases/1.0.0.md")
CITING = "# yamaa\n\n- A change (#1, D-next).\n"
NUMBERED = "# yamaa\n\n- A change (#1, #4064 D300).\n"
ENTRY = f"{DECLARED.as_posix()}: allowed to cite D-next until #4844"


def cited(path, line=3):
    """The check's finding on the citation at `line` of `path`."""
    return (
        f"{path.as_posix()}:{line}: cites D-next, a decision the log has not numbered"
    )


class ReleasedNotes(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory(prefix="yamaa-release-notes-")
        self.addCleanup(directory.cleanup)
        self.directory = directory.name

    def tree(self, files, release='version = "0.9.0"\n'):
        """A fresh repository root that declares `release` and holds `files`,
        a path to its text each."""
        root = Path(tempfile.mkdtemp(dir=self.directory))
        for path, text in {DECLARATION: release, **files}.items():
            write(root, path, text)
        return root

    def test_a_citation_fails_in_the_notes_of_a_released_version(self):
        cases = (
            ("the declared version", {DECLARED: CITING}, [cited(DECLARED)]),
            ("an earlier version", {EARLIER: CITING}, [cited(EARLIER)]),
            ("a later version, in development", {LATER: CITING}, []),
            ("numbered decisions", {EARLIER: NUMBERED, DECLARED: NUMBERED}, []),
            ("a page that names no version", {Path("docs/releases/x.md"): CITING}, []),
            (
                "two citations",
                {DECLARED: CITING + "- Another (D-next).\n"},
                [cited(DECLARED), cited(DECLARED, 4)],
            ),
        )
        for label, files, expected in cases:
            with self.subTest(case=label):
                self.assertEqual(notes.check(self.tree(files), {}), expected, label)

    def test_a_candidate_holds_its_final_version_to_the_rule(self):
        root = self.tree(
            {DECLARED: CITING}, release='version = "0.9.0"\ncandidate = 1\n'
        )
        self.assertEqual(notes.check(root, {}), [cited(DECLARED)])

    def test_an_entry_holds_only_while_its_issue_is_open_and_its_notes_cite(self):
        cases = (
            ("no issue closed", {DECLARED: CITING}, set(), []),
            ("another issue closed", {DECLARED: CITING}, {4800}, []),
            (
                "its issue closed",
                {DECLARED: CITING},
                {4844},
                [
                    f"{ENTRY}, and #4844 is closed; number its decisions, "
                    "or name the open issue that will"
                ],
            ),
            (
                "its notes numbered",
                {DECLARED: NUMBERED},
                set(),
                [f"{ENTRY}, but it cites none; remove its entry"],
            ),
            (
                "its notes gone",
                {},
                set(),
                [f"{ENTRY}, but the file is gone; remove its entry"],
            ),
        )
        for label, files, closed, expected in cases:
            with self.subTest(case=label):
                self.assertEqual(
                    notes.check(self.tree(files), {DECLARED: 4844}, closed),
                    expected,
                    label,
                )

    def test_main_reports_the_allowlisted_notes(self):
        root = self.tree({DECLARED: CITING})
        passed = (
            "PASS: no released notes under docs/releases/ cite D-next, but the "
            f"allowlisted {DECLARED.as_posix()} until #4844.\n"
        )
        cases = (
            ("allowlisted", {DECLARED: 4844}, (0, passed, "")),
            (
                "not allowlisted",
                {},
                (1, f"ERROR: {cited(DECLARED)}\n{notes.HINT}\n", ""),
            ),
        )
        for label, allowlist, expected in cases:
            with self.subTest(case=label):
                with mock.patch.object(notes, "ALLOWLIST", allowlist):
                    result = run_main(notes.main, "--root", str(root))
                self.assertEqual(result, expected, label)

    def test_the_repository_check_runs_it(self):
        self.assertIn("tools.validation.check_release_notes", repository.CHECKS)


if __name__ == "__main__":
    unittest.main()
