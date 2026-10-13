"""Select native CI coverage and compare only the selected installed evidence."""

import argparse
import itertools
import json
import os
import subprocess
import sys
from pathlib import Path

PLATFORMS = ("ubuntu-24.04", "macos-15", "windows-latest")
PYTHONS = ("3.12", "3.13", "3.14")
FORMS = ("wheel", "source")
R_PLATFORMS = PLATFORMS
BUILD_PYTHON = "3.14"
PR_PLATFORM = "ubuntu-24.04"
PR_PYTHONS = ("3.12", "3.14")


def plan(event):
    if event == "pull_request":
        # Keep PR feedback on Linux and exercise both supported Python endpoints.
        platforms = [PR_PLATFORM]
        pythons = PR_PYTHONS
        r_platforms = [PR_PLATFORM]
    elif event in {"push", "workflow_dispatch"}:
        platforms = list(PLATFORMS)
        pythons = PYTHONS
        r_platforms = list(R_PLATFORMS)
    else:
        raise ValueError(f"unsupported native CI event: {event}")
    combinations = [
        {"os": platform, "python": version, "form": form}
        for platform, version, form in itertools.product(platforms, pythons, FORMS)
    ]
    return {
        "build": {
            "include": [
                {"os": platform, "python": BUILD_PYTHON} for platform in platforms
            ]
        },
        "qualify": {"include": combinations},
        "r": {"os": r_platforms},
        "run-r": bool(r_platforms),
        "run-rust": event == "push",
        "full": event in {"push", "workflow_dispatch"},
        "compare-os": platforms[0],
    }


def compare_cohorts(matrix, root, revision):
    # The pinned download action flattens a single match, including a pattern.
    python_artifacts = len(matrix["include"])
    r_artifacts = len(
        {row["os"] for row in matrix["include"] if row["os"] in R_PLATFORMS}
    )
    downloaded = Path("cohort-input")
    for combination in matrix["include"]:
        platform = combination["os"]
        version = combination["python"]
        form = combination["form"]
        python_directory = (
            downloaded
            if python_artifacts == 1
            else downloaded / f"native-qualification-{platform}-{version}-{form}"
        )
        python_evidence = python_directory / form
        output = Path("public-cohort") / platform / version / form
        r_evidence = []
        if platform in R_PLATFORMS:
            r_directory = (
                downloaded
                if r_artifacts == 1
                else downloaded / f"native-r-qualification-{platform}"
            )
            r_evidence = [
                "--r-evidence",
                str(r_directory / "r-source"),
            ]
        for tool, fixtures, destination in (
            ("qualify_public_cohort.py", root / "benchmarks", output),
            (
                "qualify_producer_cohort.py",
                root / "rust/qualification/producer-cohort",
                output / "producer-cohort",
            ),
        ):
            subprocess.run(
                [
                    sys.executable,
                    "-I",
                    str(root / "rust/tools" / tool),
                    "--fixtures",
                    str(fixtures),
                    "--python-evidence",
                    str(python_evidence),
                    *r_evidence,
                    "--output",
                    str(destination),
                    "--source-revision",
                    revision,
                ],
                check=True,
            )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("plan", "compare"))
    args = parser.parse_args()
    if args.command == "compare":
        root = Path(os.environ["GITHUB_WORKSPACE"])
        revision = subprocess.check_output(
            ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
        ).strip()
        compare_cohorts(json.loads(os.environ["QUALIFICATION_MATRIX"]), root, revision)
        return
    coverage = plan(os.environ["GITHUB_EVENT_NAME"])
    with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
        for key, value in coverage.items():
            encoded = (
                value
                if isinstance(value, str)
                else json.dumps(value, separators=(",", ":"))
            )
            output.write(f"{key}={encoded}\n")
    with Path(os.environ["GITHUB_STEP_SUMMARY"]).open("a", encoding="utf-8") as summary:
        if coverage["full"]:
            summary.write(
                "Full installed native coverage: all 18 Python combinations "
                "and all three R hosts.\n"
            )
        else:
            summary.write(
                "PR native coverage: Linux, Python 3.12 and 3.14, "
                "wheel and source installs.\n\n"
                "Full installed platform coverage and complete cohort "
                "comparisons run on main pushes or manual full-matrix dispatches.\n"
            )
        summary.write(
            "Shared Rust checks and cross-platform math run only on pushes to main.\n"
        )


if __name__ == "__main__":
    main()
