"""Describe the full native matrix and compare its installed-host evidence."""

import itertools
import os
import subprocess
import sys
from pathlib import Path

PLATFORMS = ("ubuntu-24.04", "macos-15", "windows-latest")
PYTHONS = ("3.12", "3.13", "3.14")
FORMS = ("wheel", "source")
R_PLATFORMS = PLATFORMS
BUILD_PYTHON = "3.14"


def coverage():
    return {
        "build": {
            "include": [
                {"os": platform, "python": BUILD_PYTHON} for platform in PLATFORMS
            ]
        },
        "qualify": {
            "include": [
                {"os": platform, "python": version, "form": form}
                for platform, version, form in itertools.product(
                    PLATFORMS, PYTHONS, FORMS
                )
            ]
        },
        "r": {"os": list(R_PLATFORMS)},
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
        subprocess.run(
            [
                sys.executable,
                "-I",
                "-m",
                "tools.conformance.public_conformance",
                "complete-reports",
                "--fixtures",
                str(root / "benchmarks"),
                "--python-evidence",
                str(python_evidence),
                *r_evidence,
                "--output",
                str(output),
                "--source-revision",
                revision,
            ],
            check=True,
        )


def main():
    root = Path(os.environ["GITHUB_WORKSPACE"])
    revision = subprocess.check_output(
        ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
    ).strip()
    compare_cohorts(coverage()["qualify"], root, revision)


if __name__ == "__main__":
    main()
