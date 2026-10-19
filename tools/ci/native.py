"""Describe the full native matrix and compare its installed-host evidence."""

import itertools
import os
import subprocess
import sys
from pathlib import Path

# Fixed runner images, never a -latest label: GitHub moves those to newer
# images, which can change the system and its toolchains (MSVC on Windows)
# under a lane. tools/tests/test_workflow_runners.py checks every workflow.
PLATFORMS = ("ubuntu-24.04", "macos-15", "windows-2025")
# The Python wheels are also built and qualified for Linux aarch64 (#3030 D49).
PYTHON_PLATFORMS = (*PLATFORMS, "ubuntu-24.04-arm")
PYTHONS = ("3.12", "3.13", "3.14", "3.15")
FORMS = ("wheel", "source")
R_PLATFORMS = PLATFORMS
# Linux aarch64 has no R lane yet, so the complete reports of its Python lanes
# are compared beside the R reports of Linux x86_64.
R_EVIDENCE = {"ubuntu-24.04-arm": "ubuntu-24.04"}
# Each Linux wheel is also installed and qualified on glibc 2.28, the oldest
# glibc that its manylinux_2_28 tag admits.
GLIBC_PLATFORMS = ("ubuntu-24.04", "ubuntu-24.04-arm")
BUILD_PYTHON = "3.14"
# The host whose Python and R source archives a release publishes. Every
# source lane, on every platform, installs those same archives.
SOURCE_PLATFORM = "ubuntu-24.04"
# CRAN's incoming checks of that R source archive: R CMD check --as-cran with
# these R versions on each R platform, with the workspace's minimum Rust, and
# in these R-hub containers, which carry CRAN's check configurations, with
# their distribution's Rust.
CRAN_R_VERSIONS = ("devel", "release", "4.4")
CRAN_CONTAINERS = ("gcc16",)
# The same check with none of the package's suggested packages installed,
# as CRAN's noSuggests check runs it: R-devel on Linux, here with the minimum
# Rust.
CRAN_NO_SUGGESTS_PLATFORMS = ("ubuntu-24.04",)
CRAN_NO_SUGGESTS_R_VERSIONS = ("devel",)


def coverage():
    return {
        "rust": {
            "include": [
                {"os": platform, "python": BUILD_PYTHON} for platform in PLATFORMS
            ]
        },
        "build": {
            "include": [
                {"os": platform, "python": BUILD_PYTHON}
                for platform in PYTHON_PLATFORMS
            ]
        },
        "qualify": {
            "include": [
                {"os": platform, "python": version, "form": form}
                for platform, version, form in itertools.product(
                    PYTHON_PLATFORMS, PYTHONS, FORMS
                )
            ]
        },
        "glibc": {"os": list(GLIBC_PLATFORMS)},
        "r": {"os": list(R_PLATFORMS)},
        "cran": {"os": list(R_PLATFORMS), "r": list(CRAN_R_VERSIONS)},
        "cran_container": {"platform": list(CRAN_CONTAINERS)},
        "cran_no_suggests": {
            "os": list(CRAN_NO_SUGGESTS_PLATFORMS),
            "r": list(CRAN_NO_SUGGESTS_R_VERSIONS),
        },
    }


def compare_cohorts(matrix, root, revision):
    # The pinned download action flattens a single match, including a pattern.
    python_artifacts = len(matrix["include"])
    r_artifacts = len(
        {R_EVIDENCE.get(row["os"], row["os"]) for row in matrix["include"]}
        & set(R_PLATFORMS)
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
        r_platform = R_EVIDENCE.get(platform, platform)
        if r_platform in R_PLATFORMS:
            r_directory = (
                downloaded
                if r_artifacts == 1
                else downloaded / f"native-r-qualification-{r_platform}"
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
