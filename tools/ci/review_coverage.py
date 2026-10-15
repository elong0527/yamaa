"""Prove that bounded review PRs cover one large PR's exact Git changes.

Only GitHub API data is read. Existing Git object identities identify source;
no source is checked out, interpreted, rebuilt or hashed by this verifier.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass

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

    def tree_id(self, commit):
        if commit not in self.commits:
            data = self.get(f"git/commits/{commit}")
            require(
                isinstance(data, dict)
                and data.get("sha") == commit
                and isinstance(data.get("tree"), dict)
                and object_id(data["tree"].get("sha")),
                "invalid Git commit identity",
            )
            self.commits[commit] = data["tree"]["sha"]
        return self.commits[commit]

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
        seen.update(subset)
        chunks.append(chunk)
    require(seen == changes.keys(), "review batches leave original changes unreviewed")
    return Coverage(target, tuple(chunks))
