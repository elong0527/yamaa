"""The rework issue a published benchmark review opens.

The seam is the discussion event GitHub hands the Benchmark review workflow,
which no fixture home holds.
"""

import json
import tempfile
import unittest
from pathlib import Path

from tools.benchmark_docs import review_rework

URL = "https://github.com/elong0527/yamaa/discussions/4400"
BODY = (
    "Review of the [adam-adae-death](https://elong0527.github.io/yamaa/benchmark/"
    "adam-adae-death.html) benchmark.\r\n"
    "\r\n"
    "| Question | Answer |\r\n"
    "| --- | --- |\r\n"
    "| **Task:** Would you agree? | Disagree |\r\n"
    "| **Overall** | {overall} |\r\n"
    "\r\n"
    "### Comments\r\n"
    "\r\n"
    "Use the demographics death date.\r\n"
)


def discussion(title="Review: adam-adae-death", overall="Not approve"):
    return {
        "title": title,
        "body": BODY.format(overall=overall),
        "created_at": "2026-11-05T10:00:00Z",
        "html_url": URL,
        "user": {"login": "reviewer-a"},
    }


class ReworkIssue(unittest.TestCase):
    def test_a_review_that_does_not_approve_opens_an_issue_naming_its_reviewer(self):
        page = "https://elong0527.github.io/yamaa/benchmark/adam-adae-death.html"
        self.assertEqual(
            review_rework.rework_issue(discussion()),
            (
                "Benchmark review: rework adam-adae-death",
                f"@reviewer-a did not approve [adam-adae-death]({page}) in "
                f"[their review]({URL}):\n"
                "\n"
                f"> Review of the [adam-adae-death]({page}) benchmark.\n"
                ">\n"
                "> | Question | Answer |\n"
                "> | --- | --- |\n"
                "> | **Task:** Would you agree? | Disagree |\n"
                "> | **Overall** | Not approve |\n"
                ">\n"
                "> ### Comments\n"
                ">\n"
                "> Use the demographics death date.\n"
                "\n"
                "### Rework\n"
                "\n"
                "Change the benchmark as the review asks, and close this issue from the pull\n"
                "request. A change to `input/` or `expected/` changes a golden file: reproduce\n"
                "the values independently and say in the pull request which moved and why. When\n"
                "the benchmark should stay as it is, answer on the review instead and close this\n"
                "issue as not planned.\n"
                "\n"
                "### Review again\n"
                "\n"
                "GitHub notifies @reviewer-a when this issue closes. Once the site has updated,\n"
                f"review [adam-adae-death]({page}) again. The new review replaces the one above: agreeing\n"
                "approves the benchmark, and disagreeing opens a new rework issue.\n",
            ),
        )

    def test_other_discussions_open_no_issue(self):
        cases = (
            ("approval", discussion(overall="Approve")),
            ("skip", discussion(overall="Skip")),
            ("unknown benchmark", discussion(title="Review: adam-no-such-benchmark")),
            ("not a review", discussion(title="Question about adam-adae-death")),
            ("deleted account", {**discussion(), "user": None}),
        )
        for label, event in cases:
            with self.subTest(case=label):
                self.assertIsNone(review_rework.rework_issue(event), label)

    def test_main_writes_the_issue_files_only_for_rework(self):
        cases = (
            ("rework", "Not approve", ["body.md", "title"]),
            ("approval", "Approve", []),
        )
        for label, overall, expected in cases:
            with self.subTest(case=label), tempfile.TemporaryDirectory() as scratch:
                event = Path(scratch) / "event.json"
                event.write_text(
                    json.dumps(
                        {"action": "created", "discussion": discussion(overall=overall)}
                    ),
                    encoding="utf-8",
                )
                destination = Path(scratch) / "rework"
                review_rework.main([str(event), str(destination)])
                self.assertEqual(
                    sorted(path.name for path in destination.glob("*")), expected, label
                )
