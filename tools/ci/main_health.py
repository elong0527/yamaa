"""Keep one main-red issue per watched workflow in step with its newest main run.

The Main health workflow runs this once for each completed main run of a
watched workflow. A failed run opens or edits the workflow's issue and a
successful run closes it, but only when no newer main run of that workflow has
one of those conclusions. Runs can complete out of order: a re-run of an older
run can finish after a newer run, and its result then says nothing about main.
"""

import argparse
import json
import subprocess
from pathlib import Path

LABEL = "main-red"
# Cancelled and skipped runs say nothing about main. The job's condition in
# .github/workflows/main-health.yml lists the same conclusions.
REPORTED = frozenset({"success", "failure", "timed_out", "startup_failure"})
PAGE_SIZE = 100


def gh(*arguments):
    """Run the GitHub CLI and return its standard output."""
    return subprocess.run(
        ["gh", *arguments], check=True, stdout=subprocess.PIPE, text=True
    ).stdout


def reported(run):
    # A pull request from a fork's branch named main is not a main run.
    return run["event"] != "pull_request" and run["conclusion"] in REPORTED


def newer_runs(repository, run):
    """Yield the completed main runs of the run's workflow that are newer than it.

    Newer means a higher run number. A re-run keeps its run id and run number
    and gets only a new attempt with new attempt timestamps, so the run number
    orders a workflow's runs by when each run was created, and a re-run is
    exactly as new as the run it repeats. GitHub lists runs newest first in that
    order, so the scan stops at the first run that is not newer.
    """
    page = 1
    while True:
        runs = json.loads(
            gh(
                "api",
                f"repos/{repository}/actions/workflows/{run['workflow_id']}/runs"
                "?branch=main&status=completed&exclude_pull_requests=true"
                f"&per_page={PAGE_SIZE}&page={page}",
            )
        )["workflow_runs"]
        for other in runs:
            if other["run_number"] <= run["run_number"]:
                return
            yield other
        if len(runs) < PAGE_SIZE:
            return
        page += 1


def report(repository, run):
    """Open, edit or close the workflow's issue for one completed main run."""
    name, number = run["name"], run["run_number"]
    if not reported(run):
        print(f"{name} run {number} ended in {run['conclusion']}; nothing to report.")
        return
    newer = next(
        (other for other in newer_runs(repository, run) if reported(other)), None
    )
    if newer is not None:
        print(
            f"{name} run {number} was overtaken by run {newer['run_number']} "
            f"({newer['html_url']}); the issue is unchanged."
        )
        return
    title = f"main is red: {name}"
    issues = json.loads(
        gh(
            "issue",
            "list",
            "--repo",
            repository,
            "--state",
            "open",
            "--label",
            LABEL,
            "--limit",
            "100",
            "--json",
            "number,title",
        )
    )
    issue = next((item["number"] for item in issues if item["title"] == title), None)
    url, revision = run["html_url"], run["head_sha"]
    if run["conclusion"] == "success":
        if issue is None:
            print(f"{name} run {number} succeeded and no issue is open.")
            return
        gh(
            "issue",
            "close",
            str(issue),
            "--repo",
            repository,
            "--comment",
            f"Recovered in {url} at {revision}.",
        )
        print(f"Closed #{issue}: {name} run {number} succeeded.")
        return
    body = (
        f"The latest {run['event']} run of {name} on main ended in "
        f"{run['conclusion']} at {revision}.\n\n"
        f"Latest failing run: {url}\n\n"
        f"This issue closes itself when a later main run of {name} succeeds."
    )
    if issue is not None:
        # Editing instead of commenting keeps a two-hourly failure from
        # notifying watchers on every run.
        gh("issue", "edit", str(issue), "--repo", repository, "--body", body)
        print(f"Updated #{issue} for {name} run {number}.")
        return
    gh(
        "label",
        "create",
        LABEL,
        "--repo",
        repository,
        "--color",
        "B60205",
        "--description",
        "A main-branch check is failing",
        "--force",
    )
    gh(
        "issue",
        "create",
        "--repo",
        repository,
        "--title",
        title,
        "--label",
        LABEL,
        "--body",
        body,
    )
    print(f"Opened an issue for {name} run {number}.")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", required=True)
    parser.add_argument(
        "--event", type=Path, required=True, help="workflow_run event payload"
    )
    args = parser.parse_args(argv)
    event = json.loads(args.event.read_text(encoding="utf-8"))
    report(args.repository, event["workflow_run"])


if __name__ == "__main__":
    main()
