"""Main health changes a workflow's issue only for its newest reported main run."""

import json
import re
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from urllib.parse import parse_qs, urlsplit

from tools.ci import main_health
from tools.paths import ROOT

REPOSITORY = "elong0527/yamaa"
WORKFLOW_ID = 205000001
TITLE = "main is red: CI"
OPEN_ISSUE = {"number": 7, "title": TITLE}


def run(number, conclusion="failure", *, event="schedule", attempt=1):
    identity = 39000000000 + number
    return {
        "id": identity,
        "name": "CI",
        "workflow_id": WORKFLOW_ID,
        "run_number": number,
        "run_attempt": attempt,
        "event": event,
        "status": "completed",
        "conclusion": conclusion,
        "head_branch": "main",
        "head_sha": f"{number:040x}",
        "html_url": f"https://github.com/{REPOSITORY}/actions/runs/{identity}",
    }


def body(event_run):
    return (
        f"The latest {event_run['event']} run of CI on main ended in "
        f"{event_run['conclusion']} at {event_run['head_sha']}.\n\n"
        f"Latest failing run: {event_run['html_url']}\n\n"
        "This issue closes itself when a later main run of CI succeeds."
    )


def opened(event_run):
    return [
        (
            "label",
            "create",
            "main-red",
            "--repo",
            REPOSITORY,
            "--color",
            "B60205",
            "--description",
            "A main-branch check is failing",
            "--force",
        ),
        (
            "issue",
            "create",
            "--repo",
            REPOSITORY,
            "--title",
            TITLE,
            "--label",
            "main-red",
            "--body",
            body(event_run),
        ),
    ]


def edited(event_run):
    return [("issue", "edit", "7", "--repo", REPOSITORY, "--body", body(event_run))]


def closed(event_run):
    comment = f"Recovered in {event_run['html_url']} at {event_run['head_sha']}."
    return [("issue", "close", "7", "--repo", REPOSITORY, "--comment", comment)]


class GitHub:
    """Serve completed main runs newest first, as GitHub lists them, and the
    open main-red issues; record every other call as a change."""

    def __init__(self, runs, issues=()):
        self.runs = sorted(runs, key=lambda item: item["run_number"], reverse=True)
        self.issues = list(issues)
        self.pages = []
        self.changes = []

    def __call__(self, *arguments):
        if arguments[0] == "api":
            url = urlsplit(arguments[1])
            query = {key: value for key, [value] in parse_qs(url.query).items()}
            if (
                url.path != f"repos/{REPOSITORY}/actions/workflows/{WORKFLOW_ID}/runs"
                or query["branch"] != "main"
                or query["status"] != "completed"
            ):
                raise AssertionError(f"unexpected request {arguments}")
            size, page = int(query["per_page"]), int(query["page"])
            self.pages.append(page)
            rows = self.runs[(page - 1) * size : page * size]
            return json.dumps({"total_count": len(self.runs), "workflow_runs": rows})
        if arguments[:2] == ("issue", "list"):
            return json.dumps(self.issues)
        self.changes.append(arguments)
        return ""


class NewestMainRun(unittest.TestCase):
    def report(self, event_run, runs, issues=()):
        github = GitHub(runs, issues)
        with patch.object(main_health, "gh", side_effect=github):
            main_health.report(REPOSITORY, event_run)
        return github

    def test_newest_failure_opens_the_issue_or_edits_the_open_one(self):
        for conclusion in ("failure", "timed_out", "startup_failure"):
            newest = run(12, conclusion)
            runs = [newest, run(11, "success")]
            with self.subTest(conclusion=conclusion):
                self.assertEqual(self.report(newest, runs).changes, opened(newest))
                self.assertEqual(
                    self.report(newest, runs, [OPEN_ISSUE]).changes, edited(newest)
                )

    def test_newest_success_closes_the_open_issue(self):
        newest = run(12, "success")
        runs = [newest, run(11, "failure")]
        self.assertEqual(
            self.report(newest, runs, [OPEN_ISSUE]).changes, closed(newest)
        )
        self.assertEqual(self.report(newest, runs).changes, [])
        other = {"number": 8, "title": "main is red: Docs"}
        self.assertEqual(self.report(newest, runs, [other]).changes, [])

    def test_overtaken_older_success_changes_nothing(self):
        for newer in ("failure", "timed_out", "startup_failure"):
            older = run(11, "success", attempt=2)
            with self.subTest(newer=newer):
                github = self.report(older, [run(12, newer), older], [OPEN_ISSUE])
                self.assertEqual(github.changes, [])

    def test_overtaken_older_failure_changes_nothing(self):
        older = run(11, "failure", attempt=2)
        for issues in ((), [OPEN_ISSUE]):
            with self.subTest(issues=issues):
                github = self.report(older, [run(12, "success"), older], issues)
                self.assertEqual(github.changes, [])

    def test_a_rerun_of_the_newest_run_counts(self):
        # The re-run keeps run number 12, so it is still the newest run and
        # its result is the one that counts.
        recovered = run(12, "success", attempt=2)
        self.assertEqual(
            self.report(
                recovered, [recovered, run(11, "failure")], [OPEN_ISSUE]
            ).changes,
            closed(recovered),
        )
        broken = run(12, "failure", attempt=2)
        self.assertEqual(
            self.report(broken, [broken, run(11, "success")]).changes, opened(broken)
        )
        # A further attempt that is still running takes the run out of the
        # completed list; no newer run has overtaken it.
        self.assertEqual(
            self.report(broken, [run(11, "success")]).changes, opened(broken)
        )

    def test_cancelled_and_skipped_runs_are_ignored(self):
        for conclusion in ("cancelled", "skipped", "neutral", "action_required"):
            with self.subTest(event_run=conclusion):
                github = self.report(run(12, conclusion), [run(12, conclusion)])
                self.assertEqual((github.pages, github.changes), ([], []))
        recovered = run(11, "success")
        runs = [run(13, "cancelled"), run(12, "skipped"), recovered]
        self.assertEqual(
            self.report(recovered, runs, [OPEN_ISSUE]).changes, closed(recovered)
        )

    def test_pull_request_runs_from_a_branch_named_main_neither_report_nor_overtake(
        self,
    ):
        fork = run(12, "failure", event="pull_request")
        github = self.report(fork, [fork])
        self.assertEqual((github.pages, github.changes), ([], []))
        recovered = run(11, "success")
        self.assertEqual(
            self.report(recovered, [fork, recovered], [OPEN_ISSUE]).changes,
            closed(recovered),
        )

    def test_scan_reads_pages_until_a_newer_reported_run_or_the_event_run(self):
        # A full page of newer cancelled runs hides a newer failure on page 2.
        older = run(101, "success", attempt=2)
        runs = [run(number, "cancelled") for number in range(103, 203)]
        runs += [run(102, "failure"), older]
        github = self.report(older, runs, [OPEN_ISSUE])
        self.assertEqual((github.pages, github.changes), ([1, 2], []))
        # Runs listed after the event's own run are older; their pages are
        # never read.
        newest = run(150, "failure")
        runs = [run(number, "cancelled") for number in range(151, 200)]
        runs += [newest] + [run(number, "success") for number in range(1, 150)]
        github = self.report(newest, runs)
        self.assertEqual((github.pages, github.changes), ([1], opened(newest)))

    def test_main_reads_the_workflow_run_event(self):
        newest = run(12, "failure")
        github = GitHub([newest, run(11, "success")])
        with tempfile.TemporaryDirectory() as directory:
            event = Path(directory) / "event.json"
            event.write_text(json.dumps({"workflow_run": newest}), encoding="utf-8")
            with patch.object(main_health, "gh", side_effect=github):
                main_health.main(["--repository", REPOSITORY, "--event", str(event)])
        self.assertEqual(github.changes, opened(newest))

    def test_job_condition_reports_the_same_conclusions(self):
        workflow = (ROOT / ".github/workflows/main-health.yml").read_text(
            encoding="utf-8"
        )
        conclusions = re.findall(r"fromJSON\('(\[[^']*\])'\)", workflow)
        self.assertEqual(len(conclusions), 1)
        self.assertEqual(set(json.loads(conclusions[0])), main_health.REPORTED)


if __name__ == "__main__":
    unittest.main()
