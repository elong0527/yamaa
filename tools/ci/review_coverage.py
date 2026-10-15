"""Prove that bounded review PRs cover one large PR's exact Git changes.

Only GitHub API data is read. Existing Git object identities identify source;
no source is checked out, interpreted, rebuilt or hashed by this verifier.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from datetime import datetime

MAX_PATHS = 300
SHA = re.compile(r"[0-9a-f]{40}\Z")
MARKER = "yamaa-review-batches"
METADATA = re.compile(r"<!-- yamaa-review-batches:\s*(.*?)\s*-->", re.DOTALL)
LARGE_SKIP = re.compile(
    r"Review skipped: ([1-9][0-9]*) files exceed the limit of 300\Z"
)
MODES = {
    "040000": "tree",
    "100644": "blob",
    "100755": "blob",
    "120000": "blob",
    "160000": "commit",
}
RANGE_PREFIX = "Reviewing files that changed from the base of the PR and between "
REVIEW_RANGE = re.compile(
    re.escape(RANGE_PREFIX) + r"([0-9a-f]{40}) and ([0-9a-f]{40})\."
)
FINAL_MARKER = "final_review_risk_coverage:"
FINAL_REVIEW = re.compile(r"<!-- final_review_risk_coverage:(.*?) -->", re.DOTALL)
SUMMARY = "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->"
WALKTHROUGH = "<!-- walkthrough_start -->"
FORMAL_REVIEW = (
    "<!-- This is an auto-generated comment by CodeRabbit for review status -->"
)
COMMITS_HEADER = "<summary>\U0001f4e5 Commits</summary>"
COMMITS_SECTION = re.compile(
    r"(?m)^<details>\r?\n"
    + re.escape(COMMITS_HEADER)
    + r"\r?\n\r?\n"
    + REVIEW_RANGE.pattern
    + r"\r?\n\r?\n</details>(?=\r?\n|\Z)"
)
FENCE = re.compile(r" {0,3}(`{3,}|~{3,})(.*)\Z")
IN_PROGRESS = re.compile(
    r"<!-- This is an auto-generated comment: review(?: |_)in(?: |_)progress by coderabbit\.ai -->"
)
TIMESTAMP = re.compile(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z\Z")


class CoverageError(ValueError):
    """The supplied review batches do not prove complete source coverage."""


def require(condition, message):
    if not condition:
        raise CoverageError(message)


def object_id(value):
    return isinstance(value, str) and bool(SHA.fullmatch(value))


def positive_integer(value):
    return type(value) is int and value > 0


def large_skip(review):
    """Other successful skips, missing reviews and pending reviews never qualify."""
    if review is None or review[1] != "success" or not isinstance(review[2], str):
        return False
    match = LARGE_SKIP.fullmatch(review[2])
    return match is not None and int(match[1]) > MAX_PATHS


def batch_numbers(body):
    if body is None:
        return ()
    require(isinstance(body, str), "invalid PR body metadata")
    if MARKER not in body:
        return ()
    matches = list(METADATA.finditer(body))
    require(
        body.count(MARKER) == 1 and len(matches) == 1,
        "expected one explicit review-batch JSON marker",
    )
    try:
        numbers = json.loads(matches[0][1])
    except (ValueError, TypeError) as error:
        raise CoverageError("invalid review-batch JSON list") from error
    require(
        isinstance(numbers, list)
        and numbers
        and all(positive_integer(number) for number in numbers)
        and len(set(numbers)) == len(numbers),
        "review-batch IDs must be a nonempty list of distinct positive PR numbers",
    )
    return tuple(numbers)


def path_name(value):
    return (
        isinstance(value, str)
        and bool(value)
        and "\0" not in value
        and all(part not in {"", ".", ".."} for part in value.split("/"))
    )


def same_repository(api, value):
    return (
        isinstance(value, dict)
        and value.get("id") == api.repository_id
        and value.get("full_name") == api.repository
    )


@dataclass(frozen=True)
class Pull:
    number: int
    head: str
    base: str
    head_ref: str
    base_ref: str
    changed_files: int
    batches: tuple[int, ...]


def snapshot(api, number, *, target):
    data = api.pull(number)
    require(isinstance(data, dict), "invalid review-coverage PR response")
    base, head = data.get("base"), data.get("head")
    require(
        data.get("number") == number
        and data.get("state") == "open"
        and data.get("merged") is False
        and isinstance(base, dict)
        and isinstance(head, dict)
        and same_repository(api, base.get("repo")),
        "review coverage requires current open PRs in the trusted repository",
    )
    require(
        object_id(base.get("sha"))
        and object_id(head.get("sha"))
        and isinstance(base.get("ref"), str)
        and base["ref"]
        and isinstance(head.get("ref"), str)
        and head["ref"],
        "invalid review-coverage PR refs",
    )
    require(
        (base["ref"] == "main")
        if target
        else (base["ref"] != "main" and same_repository(api, head.get("repo"))),
        "review batches must use synthetic bases and heads in this repository",
    )
    count = data.get("changed_files")
    require(type(count) is int and count >= 0, "invalid changed-file count")
    return Pull(
        number,
        head["sha"],
        base["sha"],
        head["ref"],
        base["ref"],
        count,
        batch_numbers(data.get("body")) if target else (),
    )


class Objects:
    def __init__(self, api):
        self.api = api
        self.commits, self.trees = {}, {}

    def get(self, endpoint):
        return self.api.request("GET", f"repos/{self.api.repository}/{endpoint}")

    def commit(self, commit):
        if commit not in self.commits:
            data = self.get(f"git/commits/{commit}")
            require(
                isinstance(data, dict)
                and data.get("sha") == commit
                and isinstance(data.get("tree"), dict)
                and object_id(data["tree"].get("sha")),
                "invalid Git commit identity",
            )
            self.commits[commit] = data
        return self.commits[commit]

    def tree_id(self, commit):
        return self.commit(commit)["tree"]["sha"]

    def parents(self, commit):
        parents = self.commit(commit).get("parents")
        require(
            isinstance(parents, list)
            and all(
                isinstance(parent, dict) and object_id(parent.get("sha"))
                for parent in parents
            ),
            "invalid Git commit parents",
        )
        return tuple(parent["sha"] for parent in parents)

    def leaves(self, commit):
        tree = self.tree_id(commit)
        if tree not in self.trees:
            data = self.get(f"git/trees/{tree}?recursive=1")
            require(
                isinstance(data, dict)
                and data.get("sha") == tree
                and data.get("truncated") is False
                and isinstance(data.get("tree"), list),
                "Git tree is missing, malformed or truncated",
            )
            paths, leaves = set(), {}
            for row in data["tree"]:
                require(isinstance(row, dict), "invalid Git tree entry")
                path, mode, kind = row.get("path"), row.get("mode"), row.get("type")
                require(
                    path_name(path)
                    and path not in paths
                    and isinstance(mode, str)
                    and mode in MODES
                    and kind == MODES[mode]
                    and object_id(row.get("sha")),
                    "invalid or repeated Git tree path, mode, type or object",
                )
                paths.add(path)
                if kind != "tree":
                    leaves[path] = (mode, kind, row["sha"])
            self.trees[tree] = leaves
        return self.trees[tree]

    def merge_base(self, base, head):
        # The compare endpoint's files array is capped. Only its merge-base
        # identity is used; complete leaf deltas come from untruncated trees.
        data = self.get(f"compare/{base}...{head}?per_page=1")
        require(
            isinstance(data, dict)
            and isinstance(data.get("base_commit"), dict)
            and data["base_commit"].get("sha") == base
            and isinstance(data.get("merge_base_commit"), dict)
            and object_id(data["merge_base_commit"].get("sha")),
            "invalid Git comparison merge-base identity",
        )
        return data["merge_base_commit"]["sha"]


def delta(old, new):
    return {
        path: (old.get(path), new.get(path))
        for path in old.keys() | new.keys()
        if old.get(path) != new.get(path)
    }


def check_files(api, pull, changes):
    """Match fully paginated PR files to the raw tree paths, including renames."""
    rows = api.pages(f"pulls/{pull.number}/files")
    require(
        isinstance(rows, list) and len(rows) == pull.changed_files,
        "PR file pagination is incomplete or changed",
    )
    names, paths = set(), set()
    for row in rows:
        require(isinstance(row, dict), "invalid PR file entry")
        name, status = row.get("filename"), row.get("status")
        require(
            path_name(name)
            and name not in names
            and isinstance(status, str)
            and status
            in {"added", "removed", "modified", "renamed", "copied", "changed"},
            "invalid or repeated PR file path or status",
        )
        names.add(name)
        paths.add(name)
        require(name in changes, "PR file differs from the exact Git leaf delta")
        old, new = changes[name]
        expected = old if status == "removed" else new
        require(
            expected is not None and row.get("sha") == expected[2],
            "PR file object differs from its Git tree",
        )
        if status == "renamed":
            previous = row.get("previous_filename")
            require(
                path_name(previous)
                and previous != name
                and previous in changes
                and changes[previous][0] is not None,
                "rename has no original Git leaf path",
            )
            paths.add(previous)
        elif status == "removed":
            require(new is None, "removed PR path still exists")
        elif status == "added":
            require(old is None, "added PR path already existed")
        else:
            require(old is not None or status == "copied", "modified PR path is new")
    require(paths == changes.keys(), "PR files do not cover the exact Git leaf delta")


def check_binding(api, pull):
    associated = api.pages(f"commits/{pull.head}/pulls")
    require(
        isinstance(associated, list)
        and len(associated) == 1
        and isinstance(associated[0], dict),
        "review batch head must belong exclusively to its review PR",
    )
    data = associated[0]
    base, head = data.get("base"), data.get("head")
    require(
        data.get("number") == pull.number
        and data.get("state") == "open"
        and isinstance(base, dict)
        and isinstance(head, dict)
        and base.get("sha") == pull.base
        and base.get("ref") == pull.base_ref
        and head.get("sha") == pull.head
        and head.get("ref") == pull.head_ref
        and same_repository(api, base.get("repo"))
        and same_repository(api, head.get("repo")),
        "review completion is associated with a different PR or base",
    )
    check_history(api, pull.number, pull.head)


def check_history(api, number, head):
    events = api.pages(f"issues/{number}/timeline")
    require(isinstance(events, list), "invalid PR review timeline")
    committed = False
    for event in events:
        require(
            isinstance(event, dict)
            and isinstance(event.get("event"), str)
            and event["event"],
            "invalid PR review timeline event",
        )
        kind = event["event"]
        require(
            not kind.startswith(("base_ref_", "automatic_base_change_")),
            "PR base history changed; a fresh review PR and head are required",
        )
        committed |= kind == "committed" and event.get("sha") == head
    require(committed, "PR timeline does not identify its head commit")


def timestamp(value):
    require(
        isinstance(value, str) and TIMESTAMP.fullmatch(value),
        "missing or invalid review evidence timestamp",
    )
    try:
        return datetime.fromisoformat(value.removesuffix("Z") + "+00:00")
    except ValueError as error:
        raise CoverageError("invalid review evidence timestamp") from error


def published_prefix(prefix):
    """Exclude quoted provider sections, tracking only fences and comments."""
    fence, comment = None, False
    for line in prefix.splitlines():
        if fence is not None:
            closing = re.fullmatch(r" {0,3}([`~]+)[ \t]*", line)
            if (
                closing
                and set(closing[1]) == {fence[0]}
                and len(closing[1]) >= fence[1]
            ):
                fence = None
            continue
        if not comment:
            opening = FENCE.fullmatch(line)
            if opening:
                fence = opening[1][0], len(opening[1])
                continue
        if comment:
            end = line.find("-->")
            if end < 0:
                continue
            line, comment = line[end + 3 :], False
        while "<!--" in line:
            start = line.index("<!--")
            end = line.find("-->", start + 4)
            if end < 0:
                line, comment = line[:start], True
                break
            line = line[:start] + line[end + 3 :]
    return fence is None and not comment


def formal_review_ranges(api, number, head, completed_at, authentic):
    """Read the app's typed PR-review receipt when its summary omits the range."""
    candidates, identifiers = [], set()
    for review in api.pages(f"pulls/{number}/reviews"):
        require(isinstance(review, dict), "invalid formal PR review")
        if not authentic(review.get("user")):
            continue
        if review.get("commit_id") != head:
            continue
        identity = review.get("id")
        require(
            positive_integer(identity)
            and identity not in identifiers
            and review.get("pull_request_url")
            == f"https://api.github.com/repos/{api.repository}/pulls/{number}"
            and review.get("html_url")
            == f"https://github.com/{api.repository}/pull/{number}#pullrequestreview-{identity}"
            and review.get("state") in {"COMMENTED", "APPROVED", "CHANGES_REQUESTED"},
            "invalid or conflicting formal review identity, PR or state",
        )
        identifiers.add(identity)
        candidates.append((timestamp(review.get("submitted_at")), identity, review))
    require(candidates, "PR requires its own authentic exact-head formal review range")
    submitted, _, review = max(candidates)
    require(submitted <= completed_at, "completion predates its formal PR review")
    body = review.get("body")
    require(
        isinstance(body, str)
        and body.count(FORMAL_REVIEW) == 1
        and FORMAL_REVIEW in body.splitlines(),
        "formal review is missing its provider review receipt",
    )
    ranges = REVIEW_RANGE.findall(body)
    sections = list(COMMITS_SECTION.finditer(body))
    require(
        len(ranges) == 1
        and body.count(RANGE_PREFIX) == 1
        and body.count(COMMITS_HEADER) == 1
        and len(sections) == 1
        and published_prefix(body[: sections[0].start()]),
        "formal review range is missing, quoted or ambiguous",
    )
    return ranges


def attest_review(api, number, head, status, authentic, *, base=None):
    """Bind a completed status to this PR's own reviewed head.

    Commit statuses alone do not name a PR or base. Use the authentic app's
    latest walkthrough and final reviewed footer. Ordinary reviews may be
    incremental; bounded batches additionally require their exact base.
    A status predating the walkthrough cannot complete its newer review.
    """
    completed_at = timestamp(status.get("created_at"))
    candidates, identifiers = [], set()
    for comment in api.pages(f"issues/{number}/comments"):
        require(isinstance(comment, dict), "invalid review walkthrough comment")
        if not authentic(comment.get("user")):
            continue
        body = comment.get("body")
        require(isinstance(body, str), "invalid authentic review comment body")
        # App-authored discussion replies can quote ranges and footers. Only
        # the provider's own summary/walkthrough comment is review evidence.
        if not body.startswith(SUMMARY):
            continue
        identity = comment.get("id")
        require(
            positive_integer(identity)
            and identity not in identifiers
            and comment.get("issue_url")
            == f"https://api.github.com/repos/{api.repository}/issues/{number}",
            "invalid or repeated review walkthrough identity",
        )
        identifiers.add(identity)
        created, updated = (
            timestamp(comment.get("created_at")),
            timestamp(comment.get("updated_at")),
        )
        require(created <= updated, "invalid review walkthrough chronology")
        candidates.append((updated, identity, body))
    require(
        candidates,
        "PR requires its own authentic completed review walkthrough",
    )
    updated, _, body = max(candidates)
    require(IN_PROGRESS.search(body) is None, "PR review is still in progress")
    require(WALKTHROUGH in body, "missing provider review walkthrough")
    require(
        updated <= completed_at, "completion predates the latest review walkthrough"
    )
    markers = list(FINAL_REVIEW.finditer(body))
    require(
        body.count(FINAL_MARKER) == 1 and len(markers) == 1,
        "PR review needs one explicit final reviewed footer",
    )
    try:
        footer = json.loads(markers[0][1])
    except (ValueError, TypeError) as error:
        raise CoverageError("invalid final reviewed footer JSON") from error
    require(
        isinstance(footer, dict)
        and footer.get("sourceCommitId") == head
        and footer.get("coveredCommitId") == head
        and footer.get("kind") == "reviewed",
        "final review footer does not cover the exact current head",
    )
    ranges = REVIEW_RANGE.findall(body)
    require(
        body.count(RANGE_PREFIX) == len(ranges),
        "review walkthrough contains a malformed or conflicting range",
    )
    if not ranges:
        ranges = formal_review_ranges(api, number, head, completed_at, authentic)
    require(
        len(ranges) == 1
        and ranges[0][1] == head
        and (base is None or ranges[0][0] == base),
        "completed review walkthrough names a different base or head",
    )


@dataclass(frozen=True)
class Coverage:
    target: Pull
    chunks: tuple[Pull, ...]

    def recheck(self, api):
        for pull in (self.target, *self.chunks):
            require(
                snapshot(api, pull.number, target=pull == self.target) == pull,
                "review-coverage PR head, base or batch metadata changed",
            )
            if pull != self.target:
                check_binding(api, pull)


def prove(api, number, head):
    """Return exact bounded coverage, or None when no batch metadata is supplied."""
    target = snapshot(api, number, target=True)
    require(target.head == head, "target head changed before bounded verification")
    if not target.batches:
        return None
    require(number not in target.batches, "a PR cannot be its own review batch")
    objects = Objects(api)
    original_base = objects.merge_base(target.base, head)
    target_tree = objects.tree_id(head)
    changes = delta(objects.leaves(original_base), objects.leaves(head))
    require(
        len(changes) > MAX_PATHS and target.changed_files > MAX_PATHS,
        "bounded review fallback requires an actual large diff",
    )
    check_files(api, target, changes)
    chunks, seen, heads = [], set(), {head}
    for batch in target.batches:
        chunk = snapshot(api, batch, target=False)
        require(
            chunk.head not in heads, "review batches must have distinct head commits"
        )
        heads.add(chunk.head)
        require(
            objects.tree_id(chunk.head) == target_tree,
            "review batch head does not have the complete target source tree",
        )
        require(
            objects.parents(chunk.head) == (chunk.base,)
            and objects.parents(chunk.base) == (head,),
            "review batch must be one synthetic base child of the target and its single head child",
        )
        require(
            objects.merge_base(chunk.base, chunk.head) == chunk.base,
            "review batch base must be its actual review merge base",
        )
        subset = delta(objects.leaves(chunk.base), objects.leaves(chunk.head))
        require(
            0 < len(subset) <= MAX_PATHS,
            "each review batch must change between 1 and 300 raw leaf paths",
        )
        require(not seen.intersection(subset), "review batch deltas overlap")
        require(
            all(
                path in changes and change == changes[path]
                for path, change in subset.items()
            ),
            "review batch changes do not match original old/new Git leaves",
        )
        check_files(api, chunk, subset)
        check_binding(api, chunk)
        seen.update(subset)
        chunks.append(chunk)
    require(seen == changes.keys(), "review batches leave original changes unreviewed")
    return Coverage(target, tuple(chunks))
