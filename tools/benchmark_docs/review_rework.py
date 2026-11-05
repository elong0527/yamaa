"""Write the rework issue for a published review that does not approve.

The Benchmark review workflow runs this module when a `Review:` discussion is
created, so a disagreement reaches the repository as an issue without anyone
filing it. The issue mentions the reviewer, which subscribes them: GitHub
notifies them when the pull request that reworks the benchmark closes it, and
they review the benchmark again. Their new review replaces the one that opened
the issue, so agreeing approves the benchmark and disagreeing opens another.
"""

import json
import sys
from pathlib import Path

from tools.benchmark_docs import review_status
from tools.paths import ROOT

BENCHMARKS = ROOT / "benchmarks"
SITE = "https://elong0527.github.io/yamaa/benchmark/"
NOT_APPROVED = review_status.DECISIONS[1]


def rework_issue(discussion):
    """The (title, body) of the issue a discussion event asks for, or None.

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
    page = f"{SITE}{name}.html"
    quoted = "\n".join(
        "> " + line if line.strip() else ">"
        for line in discussion["body"].strip().splitlines()
    )
    body = f"""\
@{reviewer} did not approve [{name}]({page}) in [their review]({review["url"]}):

{quoted}

### Rework

Change the benchmark as the review asks, and close this issue from the pull
request. A change to `input/` or `expected/` changes a golden file: reproduce
the values independently and say in the pull request which moved and why. When
the benchmark should stay as it is, answer on the review instead and close this
issue as not planned.

### Review again

GitHub notifies @{reviewer} when this issue closes. Once the site has updated,
review [{name}]({page}) again. The new review replaces the one above: agreeing
approves the benchmark, and disagreeing opens a new rework issue.
"""
    return f"Benchmark review: rework {name}", body


def main(argv=None):
    """Read the event at the first path, and write the issue's `title` and
    `body.md` into the directory at the second, or nothing when the review
    needs no rework."""
    event, destination = map(Path, sys.argv[1:] if argv is None else argv)
    issue = rework_issue(json.loads(event.read_text(encoding="utf-8"))["discussion"])
    if issue is None:
        return
    destination.mkdir(parents=True, exist_ok=True)
    (destination / "title").write_text(issue[0], encoding="utf-8")
    (destination / "body.md").write_text(issue[1], encoding="utf-8")


if __name__ == "__main__":
    main()
