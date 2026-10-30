"""Installed status and printing of every result and lazy object (D36, D37).

Every printed line comes from the shared truth, which R reads too: its lines
were written from the forms of docs/design/interface-1.0/api.md at tag
yamaa-v0.6.2 and the message templates of yaml/conditions.yaml before the
engine ran. `{path}` stands for the path the test gave, and `{root}` for the
study directory as an issue location names it. Rust renders the text, so
`repr()` and `str()` return it unchanged; the facade's own tests cover each
form further (rust/crates/yamaa/tests/printed_text.rs).
"""

import os
import shutil
import tempfile
import unittest
from pathlib import Path

from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "printing")
STATUSES = load_truth(__file__, "selftest")["statuses"]
CREATED = load_truth(__file__, "public-define")["created"]
DEFINE = Path(__file__).with_name("define-fixtures")


class Printing(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="yamaa-printing-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for name, text in TRUTH["files"].items():
            (self.root / name).write_text(text, encoding="utf-8")

    def printed(self, value, lines, path):
        """`value` prints `lines`, filling `{path}` and the study's `{root}`."""
        text = (
            "\n".join(lines)
            .replace("{path}", os.fspath(path))
            .replace("{root}", self.root.resolve().as_posix())
        )
        self.assertEqual(repr(value), text)
        self.assertEqual(str(value), text)
        self.assertTrue(text.isascii())

    def result(self, case, path):
        if case["call"] == "check":
            return yamaa.spec(path).check()
        if case["call"] == "project_check":
            return yamaa.project(path).check()
        if case["call"] == "run":
            return yamaa.spec(path).run()
        return yamaa.project(path).run(case["section"])

    def test_a_lazy_object_prints_the_path_it_was_given(self):
        for kind, function in (("spec", yamaa.spec), ("project", yamaa.project)):
            with self.subTest(kind=kind):
                lazy = TRUTH["lazy"][kind]
                path = self.root / lazy["path"]
                # A path-like object prints as the text it stands for.
                for given in (path, os.fspath(path)):
                    self.printed(function(given), lazy["lines"], path)
                # It reads nothing, so a file that does not exist prints too.
                absent = self.root / "absent.yaml"
                self.printed(function(absent), lazy["lines"], absent)

    def test_every_result_has_its_status_and_prints_its_summary(self):
        for case in TRUTH["cases"]:
            with self.subTest(case=case["name"]):
                path = self.root / case["path"]
                result = self.result(case, path)
                self.assertIn(result.status, STATUSES)
                self.assertEqual(result.status, case["status"])
                self.printed(result, case["lines"], path)
                with self.assertRaises(AttributeError):
                    result.status = "succeeded"

    def test_a_project_run_member_prints_as_a_run_under_its_name(self):
        [case] = [case for case in TRUTH["cases"] if "members" in case]
        runs = yamaa.project(self.root / case["path"]).run(case["section"])
        self.assertEqual(runs.names, [member["name"] for member in case["members"]])
        for member in case["members"]:
            with self.subTest(member=member["name"]):
                run = runs[member["name"]]
                self.assertEqual(run.status, member["status"])
                self.printed(run, member["lines"], self.root)

    def test_a_submission_has_its_status_and_prints_its_summary(self):
        truth = TRUTH["submission"]
        directory = self.root / "submission"
        shutil.copytree(DEFINE / truth["case"], directory)
        shutil.copy2(directory / "expected/dm.csv", directory / "dm.csv")
        path = directory / "project.yaml"
        submission = yamaa.project(path).submission(truth["section"], created=CREATED)
        self.assertEqual(submission.status, truth["status"])
        self.printed(submission, truth["lines"], path)


if __name__ == "__main__":
    unittest.main()
