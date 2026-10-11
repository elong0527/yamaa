"""Select native CI coverage and compare only the selected installed evidence."""

import argparse
import itertools
import json
import os
import random
import subprocess
import sys
from pathlib import Path

PLATFORMS = ("ubuntu-24.04", "macos-15", "windows-latest")
PYTHONS = ("3.12", "3.13", "3.14")
FORMS = ("wheel", "source")
R_PLATFORMS = PLATFORMS
BUILD_PYTHON = "3.14"


def plan(event, run_id):
    combinations = [
        {"os": platform, "python": version, "form": form}
        for platform, version, form in itertools.product(PLATFORMS, PYTHONS, FORMS)
    ]
    if event == "pull_request":
        # A new Actions run samples again; retrying that run keeps its selection.
        combinations = [random.Random(int(run_id)).choice(combinations)]
        platforms = [combinations[0]["os"]]
        r_platforms = [platform for platform in platforms if platform in R_PLATFORMS]
    elif event in {"push", "workflow_dispatch"}:
        platforms = list(PLATFORMS)
        r_platforms = list(R_PLATFORMS)
    else:
        raise ValueError(f"unsupported native CI event: {event}")
    return {
        "build": {
            "include": [
                {"os": platform, "python": BUILD_PYTHON} for platform in platforms
            ]
        },
        "qualify": {"include": combinations},
        "r": {"os": r_platforms},
        "run-r": bool(r_platforms),
        "full": event in {"push", "workflow_dispatch"},
        "compare-os": platforms[0],
    }


def compare_cohorts(matrix, root, revision):
    for combination in matrix["include"]:
        platform = combination["os"]
        version = combination["python"]
        form = combination["form"]
        python_evidence = (
            Path("cohort-input")
            / f"native-qualification-{platform}-{version}-{form}"
            / form
        )
        output = Path("public-cohort") / platform / version / form
        r_evidence = []
        if platform in R_PLATFORMS:
            r_evidence = [
                "--r-evidence",
                str(
                    Path("cohort-input")
                    / f"native-r-qualification-{platform}"
                    / "r-source"
                ),
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
    coverage = plan(os.environ["GITHUB_EVENT_NAME"], os.environ["GITHUB_RUN_ID"])
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
                "Full native coverage: all 18 Python combinations and all three R hosts.\n"
            )
        else:
            selected = coverage["qualify"]["include"][0]
            summary.write(
                f"PR sample for Actions run {os.environ['GITHUB_RUN_ID']}: "
                f"{selected['os']}, Python {selected['python']}, {selected['form']}.\n\n"
                "Retries keep this selection. Full coverage and cross-platform math "
                "comparison run after merge to main.\n"
            )


if __name__ == "__main__":
    main()
