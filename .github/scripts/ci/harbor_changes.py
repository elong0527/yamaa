"""Select the Harbor benchmarks affected by a PR, or full scheduled coverage."""

import argparse
import json
import os
import subprocess
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[3]
SHARED_FILES = {
    "python/pyproject.toml",
    "python/uv.lock",
    ".github/workflows/harbor-oracle.yml",
    ".github/scripts/ci/harbor_changes.py",
}


def select_benchmarks(event, paths, available):
    """Map benchmark data, prompts, and references to their current Harbor cases."""
    available = set(available)
    if event in {"schedule", "workflow_dispatch"}:
        return {"benchmarks": sorted(available), "full": True, "shared": []}
    if event != "pull_request":
        raise ValueError(f"unsupported Harbor Oracle event: {event}")
    selected, shared = set(), set()
    for path in paths:
        parts = PurePosixPath(path).parts
        if len(parts) >= 3 and parts[0] == "benchmarks":
            selected.add(parts[1])
        elif len(parts) >= 4 and parts[:2] == ("evaluations", "harbor"):
            if parts[2] in {"prompts", "solutions"}:
                selected.add(parts[3])
            elif not path.endswith(".md"):
                shared.add(path)
        elif path in SHARED_FILES or (
            path.startswith("evaluations/harbor/")
            and (not path.endswith(".md") or parts[-1].startswith("system-"))
        ):
            shared.add(path)
    return {
        "benchmarks": sorted(available if shared else selected & available),
        "full": bool(shared),
        "shared": sorted(shared),
    }


def changed_paths(root, base, head):
    """Diff the PR head against its merge base, retaining both sides of renames."""
    merge_base = subprocess.check_output(
        ["git", "-C", str(root), "merge-base", base, head], text=True
    ).strip()
    return (
        subprocess.check_output(
            [
                "git",
                "-C",
                str(root),
                "diff",
                "--no-renames",
                "--name-only",
                "-z",
                merge_base,
                head,
            ]
        )
        .decode()
        .split("\0")[:-1]
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    event_name = os.environ["GITHUB_EVENT_NAME"]
    available = [
        path.parent.name
        for path in (ROOT / "evaluations/harbor/prompts").glob("*/full.md")
        if (ROOT / "benchmarks" / path.parent.name).is_dir()
    ]
    paths = []
    if event_name == "pull_request":
        event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
        pr = event["pull_request"]
        paths = changed_paths(ROOT, pr["base"]["sha"], pr["head"]["sha"])
    selection = select_benchmarks(event_name, paths, available)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(selection["benchmarks"], indent=2) + "\n")
    with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
        output.write(f"run={str(bool(selection['benchmarks'])).lower()}\n")
        output.write(f"full={str(selection['full']).lower()}\n")
    with Path(os.environ["GITHUB_STEP_SUMMARY"]).open("a", encoding="utf-8") as summary:
        scope = "full" if selection["full"] else "impacted"
        summary.write(
            f"Harbor Oracle: {scope} coverage, "
            f"{len(selection['benchmarks'])} benchmark(s).\n\n"
        )
        if selection["shared"]:
            summary.write("Shared changes affect every benchmark:\n")
            summary.writelines(f"- `{path}`\n" for path in selection["shared"])
            summary.write("\n")
        if selection["benchmarks"]:
            summary.writelines(f"- `{name}`\n" for name in selection["benchmarks"])
        else:
            summary.write(
                "No affected Harbor benchmarks; Docker verification is skipped.\n"
            )


if __name__ == "__main__":
    main()
