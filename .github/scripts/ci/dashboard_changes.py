"""Run the PR dashboard preview when its source files changed."""

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
        )
    ) or path in ("docs/articles/benchmark.md", ".github/workflows/yaml-validation.yml")


def main():
    changed = False
    if os.environ["GITHUB_EVENT_NAME"] == "pull_request":
        event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
        base = subprocess.check_output(
            ["git", "merge-base", event["pull_request"]["base"]["sha"], "HEAD"],
            text=True,
        ).strip()
        paths = (
            subprocess.check_output(["git", "diff", "--name-only", "-z", base, "HEAD"])
            .decode()
            .split("\0")
        )
        changed = any(affects_dashboard(path) for path in paths)
    with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
        output.write(f"dashboards={str(changed).lower()}\n")


if __name__ == "__main__":
    main()
