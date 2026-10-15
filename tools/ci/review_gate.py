"""Verify completed CodeRabbit coverage of the current main pull-request head.

The default command is read-only. Only the trusted default-branch workflow uses
--publish to update the review-completion commit status. No PR code, comments,
artifacts or review-body instructions are executed.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
from dataclasses import dataclass
from pathlib import Path

from tools.ci import review_coverage as coverage

CONTEXT = "review-completion"
REVIEW_CONTEXT = "CodeRabbit"
# Observed on GitHub's authentic CodeRabbit app status; a matching context name
# from another account is not review evidence.
REVIEW_CREATOR = {"login": "coderabbitai[bot]", "id": 136622811, "type": "Bot"}
SHA = re.compile(r"[0-9a-f]{40}\Z")
REPOSITORY = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]*/[A-Za-z0-9][A-Za-z0-9_.-]*\Z")
THREADS = """
query($owner: String!, $name: String!, $number: Int!, $cursor: String) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      number
      headRefOid
      reviewThreads(first: 100, after: $cursor) {
        totalCount
        nodes { id isResolved }
        pageInfo { hasNextPage endCursor }
      }
    }
  }
}
"""


class GateError(ValueError):
    """Review evidence is unavailable, malformed or no longer current."""


def require(condition, message):
    if not condition:
        raise GateError(message)


def positive_integer(value):
    return type(value) is int and value > 0


def authentic(creator):
    return isinstance(creator, dict) and all(
        creator.get(key) == value for key, value in REVIEW_CREATOR.items()
    )


@dataclass(frozen=True)
class Verdict:
    head: str
    state: str
    description: str


class GitHub:
    def __init__(self, repository):
        require(bool(REPOSITORY.fullmatch(repository)), "invalid repository")
        self.repository = repository
        metadata = self.request("GET", f"repos/{repository}")
        require(
            isinstance(metadata, dict)
            and metadata.get("full_name") == repository
            and positive_integer(metadata.get("id"))
            and metadata.get("default_branch") == "main",
            "review gate requires the actual repository with default branch main",
        )
        self.repository_id = metadata["id"]

    def request(self, method, endpoint, body=None):
        command = [
            "gh",
            "api",
            "--hostname",
            "github.com",
            "--method",
            method,
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "X-GitHub-Api-Version: 2022-11-28",
            endpoint,
        ]
        if body is not None:
            command.extend(["--input", "-"])
        try:
            result = subprocess.run(
                command,
                input=json.dumps(body) if body is not None else None,
                text=True,
                capture_output=True,
                check=False,
                timeout=45,
            )
        except (OSError, subprocess.TimeoutExpired) as error:
            raise GateError("GitHub API request could not complete") from error
        # Do not echo API payloads, subprocess output or authentication details.
        require(result.returncode == 0, "GitHub API request failed")
        try:
            return json.loads(result.stdout)
        except (TypeError, json.JSONDecodeError) as error:
            raise GateError("GitHub API did not return JSON") from error

    def pages(self, endpoint):
        rows = []
        page = 1
        previous = None
        separator = "&" if "?" in endpoint else "?"
        while True:
            values = self.request(
                "GET",
                f"repos/{self.repository}/{endpoint}{separator}per_page=100&page={page}",
            )
            require(
                isinstance(values, list) and len(values) <= 100, "invalid GitHub page"
            )
            require(values != previous, "GitHub pagination did not advance")
            rows.extend(values)
            if len(values) < 100:
                return rows
            previous = values
            page += 1

    def pull(self, number):
        return self.request("GET", f"repos/{self.repository}/pulls/{number}")

    def statuses(self, head):
        return self.pages(f"commits/{head}/statuses")

    def threads(self, number, head):
        owner, name = self.repository.split("/")
        cursor = None
        cursors, identifiers, rows = set(), set(), []
        total = None
        while True:
            payload = self.request(
                "POST",
                "graphql",
                {
                    "query": THREADS,
                    "variables": {
                        "owner": owner,
                        "name": name,
                        "number": number,
                        "cursor": cursor,
                    },
                },
            )
            require(
                isinstance(payload, dict) and not payload.get("errors"),
                "review thread query failed",
            )
            data = payload.get("data")
            require(isinstance(data, dict), "missing review thread query data")
            repository = data.get("repository")
            require(isinstance(repository, dict), "missing review thread repository")
            pull = repository.get("pullRequest")
            require(
                isinstance(pull, dict)
                and pull.get("number") == number
                and pull.get("headRefOid") == head,
                "PR head changed while reading review threads",
            )
            connection = pull.get("reviewThreads")
            require(isinstance(connection, dict), "missing review threads")
            count = connection.get("totalCount")
            require(type(count) is int and count >= 0, "invalid review thread count")
            if total is None:
                total = count
            require(count == total, "review threads changed during pagination")
            nodes = connection.get("nodes")
            require(
                isinstance(nodes, list) and len(nodes) <= 100,
                "invalid review thread page",
            )
            for node in nodes:
                require(
                    isinstance(node, dict)
                    and isinstance(node.get("id"), str)
                    and node["id"]
                    and node["id"] not in identifiers
                    and type(node.get("isResolved")) is bool,
                    "invalid or repeated review thread",
                )
                identifiers.add(node["id"])
                rows.append(node)
            info = connection.get("pageInfo")
            require(
                isinstance(info, dict) and type(info.get("hasNextPage")) is bool,
                "invalid thread pagination",
            )
            if not info["hasNextPage"]:
                require(len(rows) == total, "incomplete review thread pagination")
                return rows
            cursor = info.get("endCursor")
            require(
                isinstance(cursor, str) and cursor and cursor not in cursors,
                "nonadvancing thread cursor",
            )
            cursors.add(cursor)

    def publish(self, verdict):
        body = {
            "context": CONTEXT,
            "state": verdict.state,
            "description": verdict.description,
        }
        run = os.environ.get("GITHUB_RUN_ID", "")
        if run.isdigit():
            body["target_url"] = (
                f"https://github.com/{self.repository}/actions/runs/{run}"
            )
        status = self.request(
            "POST", f"repos/{self.repository}/statuses/{verdict.head}", body
        )
        require(
            isinstance(status, dict)
            and status.get("context") == CONTEXT
            and status.get("state") == verdict.state
            and status.get("url") == status_url(self.repository, verdict.head),
            "review status publication was not confirmed",
        )


def status_url(repository, head):
    return f"https://api.github.com/repos/{repository}/statuses/{head}"


def main_pull(api, pull):
    require(isinstance(pull, dict), "invalid pull request response")
    base = pull.get("base") or {}
    repository = base.get("repo") or {}
    return (
        pull.get("state") == "open"
        and base.get("ref") == "main"
        and repository.get("id") == api.repository_id
        and repository.get("full_name") == api.repository
    )


def current_pull(api, number):
    require(positive_integer(number), "invalid pull request number")
    pull = api.pull(number)
    require(isinstance(pull, dict), "invalid pull request response")
    require(pull.get("number") == number, "pull request identity differs")
    if not main_pull(api, pull):
        return None
    head = (pull.get("head") or {}).get("sha")
    require(isinstance(head, str) and SHA.fullmatch(head), "invalid pull request head")
    return head


def latest_review_record(api, head):
    matching = []
    for status in api.statuses(head):
        require(isinstance(status, dict), "invalid commit status")
        if status.get("context") != REVIEW_CONTEXT or not authentic(
            status.get("creator")
        ):
            continue
        require(
            positive_integer(status.get("id")), "invalid CodeRabbit status identity"
        )
        require(
            status.get("url") == status_url(api.repository, head),
            "CodeRabbit status names a different head",
        )
        matching.append(status)
    if not matching:
        return None
    return max(matching, key=lambda status: status["id"])


def review_identity(status):
    if status is None:
        return None
    return status["id"], status.get("state"), status.get("description")


def latest_review(api, head):
    return review_identity(latest_review_record(api, head))


def review_verdict(head, review):
    if review is None:
        return Verdict(
            head, "pending", "CodeRabbit review is missing for the current head"
        )
    _, state, description = review
    if state == "pending":
        return Verdict(
            head, "pending", "CodeRabbit review is pending for the current head"
        )
    if state == "success" and description == "Review completed":
        return Verdict(
            head, "success", "CodeRabbit review completed; all review threads resolved"
        )
    return Verdict(
        head, "failure", "CodeRabbit has not explicitly completed review of this head"
    )


def completed_review(api, number, head):
    # A new review starting/finishing during a multi-page read invalidates
    # that snapshot. Re-read both the status and threads before success.
    for _ in range(3):
        review = latest_review(api, head)
        verdict = review_verdict(head, review)
        if verdict.state != "success":
            return verdict, review
        threads = api.threads(number, head)
        if review != latest_review(api, head):
            continue
        if any(not thread["isResolved"] for thread in threads):
            verdict = Verdict(
                head,
                "failure",
                "Unresolved review threads require triage and resolution",
            )
        return verdict, review
    return (
        Verdict(head, "pending", "Review changed during verification; retry required"),
        None,
    )


def bounded_review(api, number, head, skipped, original):
    try:
        proof = coverage.prove(api, number, head)
        if proof is None:
            return original
        reviews = []
        for chunk in proof.chunks:
            verdict, review = completed_review(api, chunk.number, chunk.head)
            if verdict.state != "success":
                return Verdict(
                    head,
                    verdict.state,
                    f"Review batch #{chunk.number} requires completed review and resolved threads",
                )
            reviews.append((chunk, review))
        if any(not row["isResolved"] for row in api.threads(number, head)):
            return Verdict(
                head, "failure", "The target PR has unresolved review threads"
            )
        # Git objects are immutable, but PR heads, bases, metadata and review
        # statuses are not. Re-read every live reference before publication.
        proof.recheck(api)
        for chunk, review in reviews:
            record = latest_review_record(api, chunk.head)
            if review_identity(record) != review:
                return Verdict(
                    head,
                    "pending",
                    "Review changed before base/head attestation; retry required",
                )
            coverage.attest_completed_range(api, chunk, record, authentic)
        if latest_review(api, head) != skipped or any(
            latest_review(api, chunk.head) != review for chunk, review in reviews
        ):
            return Verdict(
                head,
                "pending",
                "Review changed during bounded verification; retry required",
            )
        return Verdict(
            head,
            "success",
            "Completed bounded reviews cover every exact Git leaf change",
        )
    except coverage.CoverageError as error:
        raise GateError(str(error)) from error


def verify(api, number, *, expected_head=None, publish=False):
    head = current_pull(api, number)
    if head is None:
        return None
    require(
        expected_head is None or expected_head == head, "requested PR head is stale"
    )
    if publish:
        api.publish(
            Verdict(
                head, "pending", "Checking current review completion and review threads"
            )
        )
    try:
        verdict, review = completed_review(api, number, head)
        if coverage.large_skip(review):
            verdict = bounded_review(api, number, head, review, verdict)
        require(
            current_pull(api, number) == head,
            "PR head changed during review verification",
        )
        if publish:
            api.publish(verdict)
        return verdict
    except GateError:
        if publish:
            current = current_pull(api, number)
            if current is not None:
                api.publish(
                    Verdict(
                        current,
                        "failure",
                        "Review verification failed; a fresh check is required",
                    )
                )
        raise


def linked_targets(api, numbers):
    targets = set()
    for pull in api.pages("pulls?state=open&base=main"):
        if not main_pull(api, pull):
            continue
        try:
            batches = coverage.batch_numbers(pull.get("body"))
        except coverage.CoverageError:
            # The target's own refresh will fail closed on malformed metadata.
            continue
        if numbers.intersection(batches):
            require(positive_integer(pull.get("number")), "invalid linked PR number")
            targets.add(pull["number"])
    return targets


def candidates(api, event_name, event):
    repository = event.get("repository") or {}
    require(
        repository.get("full_name") == api.repository
        and repository.get("id") == api.repository_id,
        "event repository differs from the trusted repository",
    )
    if event_name == "status":
        # This excludes our own status events and lookalike status publishers.
        if event.get("context") != REVIEW_CONTEXT or not authentic(event.get("sender")):
            return []
        head = event.get("sha")
        require(
            isinstance(head, str) and SHA.fullmatch(head), "invalid status event head"
        )
        pulls = api.pages(f"commits/{head}/pulls")
        targets, chunks = set(), set()
        for pull in pulls:
            require(isinstance(pull, dict), "invalid associated PR response")
            if (pull.get("head") or {}).get("sha") != head:
                continue
            if main_pull(api, pull):
                targets.add(pull["number"])
            elif coverage.same_repository(api, (pull.get("base") or {}).get("repo")):
                chunks.add(pull["number"])
        require(
            all(positive_integer(number) for number in targets | chunks),
            "invalid associated PR number",
        )
        if chunks:
            targets.update(linked_targets(api, chunks))
        return sorted(targets)
    if event_name == "pull_request_target":
        number = event.get("number")
    elif event_name == "issue_comment":
        issue = event.get("issue") or {}
        if not issue.get("pull_request"):
            return []
        number = issue.get("number")
        require(positive_integer(number), "event has no valid pull request number")
        return sorted({number} | linked_targets(api, {number}))
    elif event_name in {"schedule", "workflow_dispatch"}:
        pulls = api.pages("pulls?state=open&base=main")
        return sorted({pull["number"] for pull in pulls if main_pull(api, pull)})
    else:
        raise GateError("unsupported review gate event")
    require(positive_integer(number), "event has no valid pull request number")
    return [number]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repository",
        default=os.environ.get("GITHUB_REPOSITORY"),
        required=not os.environ.get("GITHUB_REPOSITORY"),
    )
    parser.add_argument("--pr", type=int)
    parser.add_argument(
        "--head",
        help="require this exact final commit in the read-only pre-merge check",
    )
    parser.add_argument(
        "--publish", action="store_true", help="trusted Actions workflow only"
    )
    args = parser.parse_args(argv)
    if args.head is not None:
        require(
            bool(SHA.fullmatch(args.head)) and args.pr is not None,
            "--head requires --pr and a full commit",
        )
    if args.publish:
        require(
            os.environ.get("GITHUB_ACTIONS") == "true"
            and args.repository == os.environ.get("GITHUB_REPOSITORY")
            and os.environ.get("GITHUB_REF") == "refs/heads/main"
            and args.pr is None
            and args.head is None,
            "publication is restricted to the trusted workflow event",
        )
    else:
        require(args.pr is not None, "read-only pre-merge verification requires --pr")
    api = GitHub(args.repository)
    numbers = (
        [args.pr]
        if args.pr is not None
        else candidates(
            api,
            os.environ.get("GITHUB_EVENT_NAME"),
            json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text()),
        )
    )
    passed = True
    for number in numbers:
        verdict = verify(api, number, expected_head=args.head, publish=args.publish)
        if verdict is None:
            require(
                args.publish, "PR is not open against this repository's main branch"
            )
            continue
        print(f"PR #{number} {verdict.head}: {verdict.state}: {verdict.description}")
        passed &= verdict.state == "success"
    # In Actions the published commit context is the gate. A normal pending
    # review is not an infrastructure failure of the updater workflow itself.
    return 0 if args.publish or passed else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (GateError, OSError, KeyError, json.JSONDecodeError) as error:
        print(f"review-completion: {error}")
        raise SystemExit(1) from None
