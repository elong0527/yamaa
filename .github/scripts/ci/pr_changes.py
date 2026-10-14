"""Select inexpensive PR smoke checks from the merge-base diff."""

import json
import os
import subprocess
from pathlib import Path


def coverage(paths):
    rust = False
    python = False
    for path in paths:
        # Markdown under tests can be a compiled fixture. Only known prose
        # directories are excluded; unknown runtime files retain smoke coverage.
        if path.startswith(("rust/planning/", "rust/measurements/")) or (
            path.startswith("rust/") and path.count("/") == 1 and path.endswith(".md")
        ):
            continue
        if path.startswith("rust/"):
            rust = python = True
        elif path.startswith(("yaml/", "benchmarks/")):
            if not path.endswith(".md"):
                rust = python = True
        elif path.startswith(("python/", "R/yamaa/", "evaluations/harbor/")):
            if not path.endswith(".md"):
                python = True
        elif path == "Makefile" or path.startswith(
            (".github/workflows/", ".github/scripts/ci/", ".github/scripts/commands/")
        ):
            rust = python = True
        elif path.startswith(".github/scripts/yaml-validation/"):
            python = True
    return {"rust": rust, "python": python}


def main():
    if os.environ["GITHUB_EVENT_NAME"] == "pull_request":
        event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
        base = subprocess.check_output(
            ["git", "merge-base", event["pull_request"]["base"]["sha"], "HEAD"],
            text=True,
        ).strip()
        # Include old and new paths for renames, and deleted files as well.
        paths = (
            subprocess.check_output(
                ["git", "diff", "--no-renames", "--name-only", "-z", base, "HEAD"]
            )
            .decode()
            .split("\0")
        )
        selected = coverage(paths)
    else:
        # Populate compatible caches on main for subsequent PRs.
        selected = {"rust": True, "python": True}
    with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
        output.writelines(
            f"{check}={str(enabled).lower()}\n" for check, enabled in selected.items()
        )
    with Path(os.environ["GITHUB_STEP_SUMMARY"]).open("a", encoding="utf-8") as summary:
        summary.write(
            "PR smoke coverage (Linux, Python 3.12): "
            + ", ".join(
                f"{check}={str(enabled).lower()}" for check, enabled in selected.items()
            )
            + ". Full installed/platform qualification runs separately.\n"
        )


if __name__ == "__main__":
    main()
