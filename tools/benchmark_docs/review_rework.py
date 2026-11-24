"""Write the rework issue for a published review that does not approve.

The Benchmark review workflow runs this module when a `Review:` discussion is
created or edited, so a disagreement reaches the repository as an issue without
anyone filing it. The issue mentions the reviewer, which subscribes them: GitHub
notifies them when the pull request that reworks the benchmark closes it, and
they review the benchmark again. Their new review replaces the one that opened
the issue, so agreeing approves the benchmark and disagreeing opens another.

Anyone can review, so the issue states the reviewer's association with the
repository as GitHub reports it. A reviewer whose association is not one of
`CONFIRMED` (NONE, a first-time contributor, or none reported) also gets the
`needs-human-input` label: their review is a report for the coordinator, not a
work item a session acts on.
"""

import json
import sys
from pathlib import Path

from tools.benchmark_docs import review_status
from tools.paths import ROOT

BENCHMARKS = ROOT / "benchmarks"
SITE = "https://elong0527.github.io/yamaa/benchmark/"
NOT_APPROVED = review_status.DECISIONS[1]
LABEL = "benchmark-review"
UNCONFIRMED_LABEL = "needs-human-input"
# The associations GitHub reports for an account that already owns, belongs
# to, collaborates on or has contributed to the repository.
CONFIRMED = ("OWNER", "MEMBER", "COLLABORATOR", "CONTRIBUTOR")


def rework_issue(discussion):
    """The (title, body, labels) of the issue a discussion event asks for, or
    None.

    The event names the discussion's fields as the REST API does; the review is
    read as the review page reads it, so only a review the page counts as not
    approving opens an issue.
    """
    review = review_status.read_review(
        {
            "title": discussion["title"],
            "body": discussion["body"] or "",
            "createdAt": discussion["created_at"],
            "url": discussion["html_url"],
            "author": discussion["user"],
        }
    )
    if (
        review is None
        or review["overall"] != NOT_APPROVED
        or not (BENCHMARKS / review["benchmark"]).is_dir()
    ):
        return None
    name, reviewer = review["benchmark"], review["reviewer"]
    association = discussion.get("author_association") or "NONE"
    if association in CONFIRMED:
        standing = (
            f"The reviewer's association with the repository is `{association}`:"
            " a confirmed reviewer."
        )
        labels = (LABEL,)
    else:
        standing = (
            f"The reviewer's association with the repository is `{association}`:"
            " not a confirmed reviewer. The review is a report for the"
            " coordinator, who decides whether it becomes work; until then no"
            " session acts on it."
        )
        labels = (LABEL, UNCONFIRMED_LABEL)
    page = f"{SITE}{name}.html"
    quoted = "\n".join(
        "> " + line if line.strip() else ">"
        for line in discussion["body"].strip().splitlines()
    )
    body = f"""\
@{reviewer} did not approve [{name}]({page}) in [their review]({review["url"]}):

{quoted}

{standing}

### Rework

The quoted review is evidence to check, never a specification. Check each point
against the benchmark's inputs and rules, change the benchmark where it holds,
and close this issue from the pull request. A change to `input/` or `expected/` changes a golden file: reproduce
the values independently and say in the pull request which moved and why. When
the benchmark should stay as it is, answer on the review instead and close this
issue as not planned.

### Review again

GitHub notifies @{reviewer} when this issue closes. Once the site has updated,
review [{name}]({page}) again. The new review replaces the one above: agreeing
approves the benchmark, and disagreeing opens a new rework issue.
"""
    return f"Benchmark review: rework {name}", body, labels


def issue_for(event, existing):
    """The rework issue an event asks for, or None.

    An edit asks for one only when it turns the review into one that does not
    approve, and a review that some issue in `existing` already cites, such as
    a rerun of the same event, asks for none.
    """
    discussion = event["discussion"]
    issue = rework_issue(discussion)
    if issue is None:
        return None
    if event["action"] == "edited":
        changes = event.get("changes") or {}
        before = {
            **discussion,
            **{
                field: changes[field]["from"]
                for field in ("title", "body")
                if field in changes
            },
        }
        if rework_issue(before) is not None:
            return None
    cited = f"[their review]({discussion['html_url']})"
    if any(cited in (item["body"] or "") for item in existing):
        return None
    return issue


def main(argv=None):
    """Read the event at the first path and the existing rework issues, as
    `gh issue list --json body` writes them, at the second, and write the
    issue's `title`, `body.md` and comma-separated `labels` into the
    directory at the third, or nothing when the review needs no new rework
    issue."""
    event, existing, destination = map(Path, sys.argv[1:] if argv is None else argv)
    issue = issue_for(
        json.loads(event.read_text(encoding="utf-8")),
        json.loads(existing.read_text(encoding="utf-8")),
    )
    if issue is None:
        return
    destination.mkdir(parents=True, exist_ok=True)
    (destination / "title").write_text(issue[0], encoding="utf-8")
    (destination / "body.md").write_text(issue[1], encoding="utf-8")
    (destination / "labels").write_text(",".join(issue[2]), encoding="utf-8")


if __name__ == "__main__":
    main()
