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
# Linux aarch64 has no native-r lane yet, so the complete reports of its Python
# lanes are compared beside the R reports of Linux x86_64.
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
# The source archives of these hosts that a release publishes, built and
# installed with the network disabled from their vendored crates (#3030 D55)
# on every platform, then run on the conformance corpus: on Linux in a
# container without a network, on macOS and Windows behind a proxy that
# refuses every connection.
OFFLINE_SOURCE_PLATFORMS = PYTHON_PLATFORMS
OFFLINE_SOURCE_HOSTS = ("python", "r")
# R-release and R-oldrel, the two R series that CRAN builds binary packages
# for, each at its latest patch release. The other native R lanes use
# R-release.
R_RELEASE = "4.6.1"
R_OLDREL = "4.5.3"
# The R binary packages that a release publishes: one per platform and series
# (#3030 D48), and none for Linux, which builds from source (D54). Each is
# built from the published R source archive, then installed on a clean runner
# of its platform with no Rust, no Rtools and no network, where it runs the
# conformance corpus and the installed R suites.
R_BINARY_PLATFORMS = ("macos-15", "windows-2025")
R_BINARY_VERSIONS = (R_RELEASE, R_OLDREL)
# The command-line binaries that a release publishes (#3030 D12, D49): the
# yamaa executable of rust/crates/yamaa-cli, built for each target on a runner
# of its own platform, statically with musl on Linux and with the static C
# runtime on Windows, and archived with its notices. Each archive is then
# unpacked on a fresh runner of its platform, where the binary runs its
# selftest.
CLI_TARGETS = {
    "ubuntu-24.04": "x86_64-unknown-linux-musl",
    "macos-15": "aarch64-apple-darwin",
    "windows-2025": "x86_64-pc-windows-msvc",
    "ubuntu-24.04-arm": "aarch64-unknown-linux-musl",
}


def r_series(version):
    """Return the R series of a version, such as 4.6 for 4.6.1."""
    return ".".join(version.split(".")[:2])


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
        "offline_source": {
            "os": list(OFFLINE_SOURCE_PLATFORMS),
            "host": list(OFFLINE_SOURCE_HOSTS),
        },
        "r_binary": {"os": list(R_BINARY_PLATFORMS), "r": list(R_BINARY_VERSIONS)},
        "cli": {
            "include": [
                {"os": platform, "target": target}
                for platform, target in CLI_TARGETS.items()
            ]
        },
    }


def compare_cohorts(coverage, root, revision):
    """Compare the complete reports of every installed host with their truth.

    Each Python lane is compared beside the R source installation of its
    platform. Each R binary installation is compared beside the wheel lane of
    its platform with the build Python: on Windows the binary links through
    Rtools' GNU toolchain, where the wheel uses MSVC.
    """
    matrix = coverage["qualify"]
    binaries = list(
        itertools.product(coverage["r_binary"]["os"], coverage["r_binary"]["r"])
    )
    # The pinned download action flattens a single match, including a pattern.
    python_artifacts = len(matrix["include"])
    r_artifacts = len(
        {R_EVIDENCE.get(row["os"], row["os"]) for row in matrix["include"]}
        & set(R_PLATFORMS)
    )
    downloaded = Path("cohort-input")

    def evidence(name, count):
        return downloaded if count == 1 else downloaded / name

    comparisons = []
    for combination in matrix["include"]:
        platform = combination["os"]
        version = combination["python"]
        form = combination["form"]
        python_evidence = (
            evidence(
                f"native-qualification-{platform}-{version}-{form}", python_artifacts
            )
            / form
        )
        output = Path("public-cohort") / platform / version / form
        r_evidence = []
        r_platform = R_EVIDENCE.get(platform, platform)
        if r_platform in R_PLATFORMS:
            r_directory = evidence(f"native-r-qualification-{r_platform}", r_artifacts)
            r_evidence = [
                "--r-evidence",
                str(r_directory / "r-source"),
            ]
        comparisons.append((python_evidence, r_evidence, output))
    for platform, version in binaries:
        python_evidence = (
            evidence(
                f"native-qualification-{platform}-{BUILD_PYTHON}-wheel",
                python_artifacts,
            )
            / "wheel"
        )
        r_directory = evidence(
            f"native-r-binary-qualification-{platform}-{version}", len(binaries)
        )
        output = Path("public-cohort") / platform / f"r-binary-{version}"
        comparisons.append(
            (python_evidence, ["--r-evidence", str(r_directory / "r-binary")], output)
        )
    for python_evidence, r_evidence, output in comparisons:
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
    compare_cohorts(coverage(), root, revision)


if __name__ == "__main__":
    main()
