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


PAGE = "https://elong0527.github.io/yamaa/benchmark/adam-adae-death.html"
ISSUE = (
    "Benchmark review: rework adam-adae-death",
    f"@reviewer-a did not approve [adam-adae-death]({PAGE}) in "
    f"[their review]({URL}):\n"
    "\n"
    f"> Review of the [adam-adae-death]({PAGE}) benchmark.\n"
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
    f"review [adam-adae-death]({PAGE}) again. The new review replaces the one above: agreeing\n"
    "approves the benchmark, and disagreeing opens a new rework issue.\n",
)


class ReworkIssue(unittest.TestCase):
    def test_a_review_that_does_not_approve_opens_an_issue_naming_its_reviewer(self):
        self.assertEqual(review_rework.rework_issue(discussion()), ISSUE)

    def test_other_discussions_open_no_issue(self):
        cases = (
            ("approval", discussion(overall="Approve")),
            ("skips report", discussion(title="Review: skipped benchmarks")),
            ("unknown benchmark", discussion(title="Review: adam-no-such-benchmark")),
            ("not a review", discussion(title="Question about adam-adae-death")),
            ("deleted account", {**discussion(), "user": None}),
        )
        for label, event in cases:
            with self.subTest(case=label):
                self.assertIsNone(review_rework.rework_issue(event), label)

    def test_only_a_review_newly_not_approving_without_an_issue_opens_one(self):
        approving = BODY.format(overall="Approve")
        not_approving = BODY.format(overall="Not approve")
        cited = [{"body": ISSUE[1]}]
        other = [{"body": ISSUE[1].replace(URL, URL + "1")}]
        cases = (
            ("created", {"action": "created"}, [], ISSUE),
            ("created again", {"action": "created"}, cited, None),
            ("another review's issue", {"action": "created"}, other, ISSUE),
            (
                "edited to not approve",
                {"action": "edited", "changes": {"body": {"from": approving}}},
                [],
                ISSUE,
            ),
            (
                "edited into a review",
                {"action": "edited", "changes": {"title": {"from": "Draft"}}},
                [],
                ISSUE,
            ),
            (
                "edited while not approving",
                {"action": "edited", "changes": {"body": {"from": not_approving}}},
                [],
                None,
            ),
            ("title edited only", {"action": "edited", "changes": {}}, [], None),
        )
        for label, event, existing, expected in cases:
            with self.subTest(case=label):
                self.assertEqual(
                    review_rework.issue_for(
                        {**event, "discussion": discussion()}, existing
                    ),
                    expected,
                    label,
                )

    def test_main_writes_the_issue_files_only_for_rework(self):
        cases = (
            ("rework", "Not approve", {"body.md": ISSUE[1], "title": ISSUE[0]}),
            ("approval", "Approve", {}),
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
                existing = Path(scratch) / "existing.json"
                existing.write_text("[]", encoding="utf-8")
                destination = Path(scratch) / "rework"
                review_rework.main([str(event), str(existing), str(destination)])
                self.assertEqual(
                    {
                        path.name: path.read_text(encoding="utf-8")
                        for path in destination.glob("*")
                    },
                    expected,
                    label,
                )
