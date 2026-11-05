"""Save the published benchmark reviews, and count them for the review page.

GitHub's Discussions API needs a token, so the static page cannot ask it. The
Docs workflow runs this module before the build, and the generator reads the
file it saves. The dashboards write each review's title and Overall row, and
these patterns read them back.
"""

import json
import os
import re
import sys
import urllib.request
from datetime import UTC, datetime

from tools.paths import ROOT

API = "https://api.github.com/graphql"
DESTINATION = ROOT / "docs/benchmark/review/reviews.json"
TITLE = re.compile(r"Review: ([a-z0-9-]+)")
# A reviewer's decision, as the Overall row of the review states it.
DECISIONS = ("Approve", "Not approve")
# A reviewer who cannot judge a benchmark publishes a skip in the same row. The
# review page counts skips, and they decide nothing.
SKIP = "Skip"
# GitHub can store a submitted form's line breaks as CRLF.
OVERALL = re.compile(
    rf"^\| \*\*Overall\*\* \| ({'|'.join((*DECISIONS, SKIP))}) \|\s*$", re.MULTILINE
)
REQUIRED_REVIEWS = 2
# The UTC date each later round opens, oldest first. The coordinator adds one
# when the rework of the round before it is done.
ROUND_STARTS = ()
APPROVED = "Approved"
NEEDS_REWORK = "Needs rework"
IN_REVIEW = "In review"
NOT_REVIEWED = "Not reviewed"
STATUSES = (APPROVED, NEEDS_REWORK, IN_REVIEW, NOT_REVIEWED)
QUERY = """
query ($cursor: String) {
  repository(owner: "elong0527", name: "yamaa") {
    discussions(first: 100, after: $cursor) {
      pageInfo { hasNextPage endCursor }
      nodes { title body createdAt url author { login } }
    }
  }
}
"""


def read_review(discussion):
    """One review from a discussion, or None for any other discussion."""
    title = TITLE.fullmatch(discussion["title"])
    overall = OVERALL.search(discussion["body"])
    if not title or not overall or not discussion["author"]:
        return None
    return {
        "benchmark": title.group(1),
        "reviewer": discussion["author"]["login"],
        "posted": discussion["createdAt"],
        "overall": overall.group(1),
        "url": discussion["url"],
    }


def status(verdicts):
    """A benchmark settles only once enough people have reviewed it."""
    if not verdicts:
        return NOT_REVIEWED
    if len(verdicts) < REQUIRED_REVIEWS:
        return IN_REVIEW
    return APPROVED if set(verdicts) == {"Approve"} else NEEDS_REWORK


def rounds(reviews, starts):
    """A benchmark's (reviews, status) in each round it took part in, with each
    reviewer's latest review in the order they first reviewed it. Only a
    person's latest review counts until approval. An approval stands for good,
    rework starts the next round with no reviews, and other reviews carry over."""
    history, latest = [], {}
    pending = sorted(reviews, key=lambda review: review["posted"])
    for end in (*starts, None):
        while pending and (end is None or pending[0]["posted"] < end):
            review = pending.pop(0)
            latest[review["reviewer"]] = review
            if status([item["overall"] for item in latest.values()]) == APPROVED:
                return history + [(tuple(latest.values()), APPROVED)]
        found = status([item["overall"] for item in latest.values()])
        history.append((tuple(latest.values()), found))
        if found == NEEDS_REWORK:
            latest = {}
    return history


def tally(names, reviews):
    """Each benchmark's rounds, from the decisions on the benchmarks in `names`."""
    found = {name: [] for name in names}
    for review in reviews:
        if review["benchmark"] in found and review["overall"] in DECISIONS:
            found[review["benchmark"]].append(review)
    return {name: rounds(items, ROUND_STARTS) for name, items in found.items()}


def skips(names, reviews):
    """How many reviewers skipped each benchmark in `names`."""
    found = {name: set() for name in names}
    for review in reviews:
        if review["benchmark"] in found and review["overall"] == SKIP:
            found[review["benchmark"]].add(review["reviewer"])
    return {name: len(reviewers) for name, reviewers in found.items()}


def fetch(token):
    """Every review published in the repository's discussions."""
    reviews, cursor = [], None
    while True:
        request = urllib.request.Request(
            API,
            data=json.dumps({"query": QUERY, "variables": {"cursor": cursor}}).encode(),
            headers={"Authorization": f"Bearer {token}", "User-Agent": "yamaa-docs"},
        )
        with urllib.request.urlopen(request, timeout=30) as response:
            answer = json.load(response)
        if "errors" in answer:
            raise RuntimeError(answer["errors"])
        discussions = answer["data"]["repository"]["discussions"]
        found = (read_review(item) for item in discussions["nodes"])
        reviews.extend(review for review in found if review)
        page = discussions["pageInfo"]
        if not page["hasNextPage"]:
            return reviews
        if page["endCursor"] in (None, cursor):
            raise RuntimeError("the discussions cursor did not advance")
        cursor = page["endCursor"]


def main():
    token = os.environ.get("GITHUB_TOKEN")
    if not token:
        sys.exit("GITHUB_TOKEN is required to read the discussions")
    saved = {
        "updated": datetime.now(UTC).strftime("%Y-%m-%d %H:%M UTC"),
        "reviews": fetch(token),
    }
    DESTINATION.parent.mkdir(parents=True, exist_ok=True)
    DESTINATION.write_text(json.dumps(saved, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
