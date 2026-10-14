"""Run the dashboard checks when their source files changed."""

import json
import os
import subprocess
from pathlib import Path


def affects_dashboard(path):
    return path.startswith(
        (
            "benchmarks/",
            "evaluations/harbor/prompts/",
            "evaluations/harbor/solutions/",
            ".github/scripts/benchmark-docs/",
            ".github/scripts/ci/",
            ".github/scripts/commands/",
            "docs/assets/",
        )
    ) or path in (
        "Makefile",
        "mkdocs.yml",
        "docs/articles/benchmark.md",
        "rust/tools/public_conformance.py",
        ".github/workflows/yaml-validation.yml",
    )


def changed_since(base):
    # Include old and new paths for renames, and deleted files as well.
    return (
        subprocess.check_output(
            ["git", "diff", "--no-renames", "--name-only", "-z", base, "HEAD"]
        )
        .decode()
        .split("\0")
    )


def push_base(event):
    # A new branch or an unavailable earlier commit checks every dashboard.
    before = event.get("before") or ""
    if not before.strip("0"):
        return None
    available = subprocess.run(
        ["git", "cat-file", "-e", before + "^{commit}"],
        capture_output=True,
        check=False,
    )
    return before if available.returncode == 0 else None


def main():
    changed = True
    name = os.environ["GITHUB_EVENT_NAME"]
    event_path = os.environ.get("GITHUB_EVENT_PATH")
    event = json.loads(Path(event_path).read_text()) if event_path else {}
    if name == "pull_request":
        base = subprocess.check_output(
            ["git", "merge-base", event["pull_request"]["base"]["sha"], "HEAD"],
            text=True,
        ).strip()
        changed = any(affects_dashboard(path) for path in changed_since(base))
    elif name == "push" and (base := push_base(event)):
        changed = any(affects_dashboard(path) for path in changed_since(base))
    with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
        output.write(f"dashboards={str(changed).lower()}\n")


if __name__ == "__main__":
    main()
