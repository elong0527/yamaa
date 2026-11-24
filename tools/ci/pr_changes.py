"""Select the work of the four PR lanes from the exact head's changed paths.

Only known independent areas narrow the checks. An unrecognized path runs
every lane, so adding a build input cannot accidentally bypass qualification.
Each job runs this cheap selection before installing its tools; no fifth
planning job or extra required status is needed.

A pull request that moves a lock, a manifest or the advisory policy also
checks both locks against the advisory databases in the repository lane, so
a dependency with a known advisory fails a required check before it merges.
Main's scheduled runs qualify both installed hosts against the benchmark
goldens, so two pull requests that are each green and wrong together are
caught within one schedule interval.
"""

import argparse
import os
import subprocess
from pathlib import Path

from tools.paths import ROOT

FLAGS = (
    "rust",
    "minimum",
    "python",
    "r",
    "equivalence_python",
    "equivalence_r",
    "workflow_lint",
    "advisories",
)
DOCUMENTS = {"README.md", "CONTRIBUTING.md", "AGENTS.md"}
DOCUMENT_SUFFIXES = {".md", ".qmd", ".css", ".svg", ".png", ".jpg", ".jpeg", ".gif"}
# These tools change repository validation or documentation, not the installed
# hosts. Their unit tests remain in the Python lane.
REPOSITORY_TOOLS = (
    "tools/tests/",
    "tools/docs/",
    "tools/benchmark_docs/",
    "tools/rules/",
    "tools/validation/",
    "evaluations/harbor/",
)
SHARED_WORKFLOW = {
    ".github/workflows/ci.yml",
    "tools/ci/pr_changes.py",
    "python/pyproject.toml",
    "python/uv.lock",
}
# Inputs to the advisory check: the two locks it scans, the manifests they
# are resolved from, its ignore policies, and the action that runs it.
ADVISORY_INPUTS = {
    "rust/Cargo.lock",
    "python/uv.lock",
    "python/pyproject.toml",
    "rust/deny.toml",
    "osv-scanner.toml",
}
ADVISORY_ACTION = ".github/actions/advisories/"
ENGINE_CHECKS = {
    "rust": True,
    "minimum": True,
    "python": True,
    "r": True,
    "equivalence_python": True,
    "equivalence_r": True,
}


def documentation(path):
    """Whether a path is prose or a site asset, without executable code."""
    return path in DOCUMENTS or (
        path.startswith("docs/") and Path(path).suffix in DOCUMENT_SUFFIXES
    )


def advisory_input(path):
    """Whether a path can change what the advisory check reports."""
    return (
        path in ADVISORY_INPUTS
        or path.startswith(ADVISORY_ACTION)
        or (path.startswith("rust/") and Path(path).name == "Cargo.toml")
    )


def select(paths):
    """Return the complete lane decision for the given changed paths."""
    paths = tuple(paths)
    result = dict.fromkeys(FLAGS, False) | {"repository": "docs"}
    for path in paths:
        if documentation(path):
            continue
        result["repository"] = "full"
        if path in SHARED_WORKFLOW or path.startswith(".github/actions/"):
            result.update(ENGINE_CHECKS)
        elif path.startswith(".github/"):
            result["python"] = True
        elif path.startswith(REPOSITORY_TOOLS):
            result["python"] = True
        elif path.startswith("python/"):
            result.update(python=True, equivalence_python=True)
        elif path.startswith("R/"):
            result.update(r=True, equivalence_r=True)
        else:
            result.update(ENGINE_CHECKS)
        if path.startswith(".github/"):
            result["workflow_lint"] = True
        if advisory_input(path):
            result["advisories"] = True
    # Empty diffs cannot justify skipping checks (for example after a force
    # push that leaves no changes against the base).
    if not paths:
        result.update(ENGINE_CHECKS, repository="full")
    return result


def changed_paths(base, head, root=ROOT):
    """Read the PR's own diff, refusing missing refs or a synthetic merge.

    --no-renames includes a renamed file's old path too, and -z keeps spaces,
    tabs and newlines in a filename from changing the selection.
    """
    if not base or not head:
        raise ValueError("a pull request needs both base and head revisions")

    def git(*arguments):
        return subprocess.check_output(["git", "-C", str(root), *arguments])

    if git("rev-parse", "HEAD").decode().strip() != head:
        raise ValueError("PR checks must run on the pull-request head")
    ancestor = git("merge-base", base, head).decode().strip()
    return (
        git("diff", "--no-renames", "--name-only", "-z", ancestor, head, "--")
        .decode("utf-8", errors="surrogateescape")
        .split("\0")[:-1]
    )


def decision(event, base="", head="", root=ROOT):
    """Select a PR's work, or the complete development work on other events.

    Main's schedule and a dispatch qualify both installed hosts; a push to a
    release branch runs the development work, as its candidate is qualified
    by the native run. The weekly Dependency graphs run checks main's locks
    for new advisories, so only a pull request that moves them runs the
    advisory check here.
    """
    if event == "pull_request":
        return select(changed_paths(base, head, root))
    if event not in ("push", "schedule", "workflow_dispatch"):
        raise ValueError(f"unsupported CI event: {event!r}")
    result = dict.fromkeys(FLAGS, True) | {"repository": "full", "advisories": False}
    gates = event != "push"
    result.update(equivalence_python=gates, equivalence_r=gates)
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--event",
        choices=("pull_request", "push", "schedule", "workflow_dispatch"),
        default=os.environ.get("EVENT_NAME"),
        required="EVENT_NAME" not in os.environ,
    )
    parser.add_argument("--base", default=os.environ.get("BASE_SHA", ""))
    parser.add_argument("--head", default=os.environ.get("HEAD_SHA", ""))
    args = parser.parse_args(argv)
    try:
        selected = decision(args.event, args.base, args.head)
    except (ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"::error::{error}\n")
    for name, value in selected.items():
        print(f"{name}={str(value).lower()}")


if __name__ == "__main__":
    main()
