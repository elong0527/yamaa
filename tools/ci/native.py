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
# The R source package is installed and qualified with R-release on every
# platform that a wheel is built for, Linux aarch64 included (#3030 D49), so
# the complete reports of each Python lane are compared beside R reports of
# its own platform (D240 on #3673).
R_PLATFORMS = PYTHON_PLATFORMS
# Each Linux wheel is also installed and qualified on glibc 2.28, the oldest
# glibc that its manylinux_2_28 tag admits.
GLIBC_PLATFORMS = ("ubuntu-24.04", "ubuntu-24.04-arm")
BUILD_PYTHON = "3.14"
# The host whose Python and R source archives a release publishes. Every
# source lane, on every platform, installs those same archives. Its
# native-conformance job also builds the conformance archive that a release
# publishes (#3690), whose committed files are the same for every platform.
SOURCE_PLATFORM = "ubuntu-24.04"
# CRAN's incoming checks of that R source archive: R CMD check --as-cran with
# these R versions on each of these platforms, with the workspace's minimum
# Rust, and in these R-hub containers, which carry CRAN's check
# configurations: gcc16, and the configurations of CRAN's additional checks,
# gcc-asan and clang-asan, whose R compiles packages with AddressSanitizer
# and UndefinedBehaviorSanitizer, and valgrind, where the examples and tests
# run under valgrind's memcheck. The Fedora images, all but clang-asan, build
# with their distribution's Rust, and clang-asan with the minimum Rust from
# its standalone installer. Pin the oldest admitted R patch so the declared
# floor is qualified, not only the last patch of that series. Linux aarch64
# stays at R-release alone (#3030 D198), in its native-r lane.
CRAN_PLATFORMS = PLATFORMS
CRAN_R_VERSIONS = ("devel", "release", "4.2.0")
CRAN_CONTAINERS = ("gcc16", "gcc-asan", "clang-asan", "valgrind")
# The same check with none of the package's suggested packages installed,
# as CRAN's noSuggests check runs it: R-devel on Linux, here with the minimum
# Rust.
CRAN_NO_SUGGESTS_PLATFORMS = ("ubuntu-24.04",)
CRAN_NO_SUGGESTS_R_VERSIONS = ("devel",)
# The same check with the PDF and HTML manuals, which CRAN's incoming checks
# build and validate and the other lanes leave out (--no-manual), since their
# runners have neither LaTeX nor HTML Tidy: R-devel on Linux, with the LaTeX
# packages and fonts of R's Rd.sty, HTML Tidy and the minimum Rust.
CRAN_MANUAL_PLATFORMS = ("ubuntu-24.04",)
CRAN_MANUAL_R_VERSIONS = ("devel",)
# The same check on Windows with the Rust that CRAN's Windows check builds
# with, for the GNU host: R-devel, as CRAN's incoming checks run. A Rust
# release can break the build where neither the minimum nor the pinned Rust
# does: 1.86 to 1.95 ship that host's standard library with empty
# split-DWARF members, which objcopy refused in the static library (#4012).
CRAN_RUST_PLATFORMS = ("windows-2025",)
CRAN_RUST_R_VERSIONS = ("devel",)
CRAN_RUST_VERSIONS = ("1.92.0",)
# The source archives of these hosts that a release publishes, built and
# installed with the network disabled from their vendored crates (#3030 D55)
# on every platform, then run on the conformance corpus: on Linux in a
# container without a network, on macOS and Windows behind a proxy that
# refuses every connection.
OFFLINE_SOURCE_PLATFORMS = PYTHON_PLATFORMS
OFFLINE_SOURCE_HOSTS = ("python", "r")
# The offline toolchain route (#3030 D54), which the restricted-environment
# guide documents for a Linux machine whose packaged Rust is older than the
# workspace's rust-version: that minimum Rust, installed from its standalone
# installer archive of static.rust-lang.org with no rustup and no network,
# builds and installs the source archives of these hosts that a release
# publishes, in a container without a network, and each then runs the
# conformance corpus.
OFFLINE_TOOLCHAIN_PLATFORMS = ("ubuntu-24.04", "ubuntu-24.04-arm")
OFFLINE_TOOLCHAIN_HOSTS = ("python", "r")
# Each wheel that a release publishes, installed as the restricted-environment
# guide installs files carried in, with the oldest and the newest supported
# CPython: the wheels are abi3, so these two bound the supported interpreters
# that each wheel serves. While the network is up, pip downloads the wheel's
# locked runtime dependencies, as wheels, into one directory with it. Then,
# with the network disabled, pip installs the wheel from that directory alone
# into a new virtual environment, building nothing, and the installed package
# runs its selftest: on Linux in a container without a network, from the
# pinned manylinux_2_28 image of the runner's architecture, with that image's
# CPython; on macOS and Windows behind a proxy that refuses every connection.
# The selftest proves that offline installation, and the lane ends with it:
# the conformance corpus runs on the same wheel in the qualify lanes, with
# every supported CPython, and in the glibc lanes, which install it while the
# network is up (D240 on #3673).
OFFLINE_WHEEL_PLATFORMS = PYTHON_PLATFORMS
OFFLINE_WHEEL_PYTHONS = (PYTHONS[0], PYTHONS[-1])
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
# unpacked and run on a fresh runner of its platform with the network
# disabled, on Linux in containers without a network and on macOS and Windows
# behind a proxy that refuses every connection: the binary runs its selftest
# and then the conformance corpus, written as 1.0 (D122), which the job
# prepares while the network is up.
CLI_TARGETS = {
    "ubuntu-24.04": "x86_64-unknown-linux-musl",
    "macos-15": "aarch64-apple-darwin",
    "windows-2025": "x86_64-pc-windows-msvc",
    "ubuntu-24.04-arm": "aarch64-unknown-linux-musl",
}
# Windows network shares (#3030 D15). The runner shares a directory of its own
# over SMB, and the wheel that a release publishes for it, installed with the
# build Python, runs the conformance corpus with every specification, project,
# input and output on that share, reached by one route per lane: the share's
# UNC path, or a drive mapped to the share.
SHARE_PLATFORMS = ("windows-2025",)
SHARE_ROUTES = ("unc-path", "mapped-drive")


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
        "cran": {"os": list(CRAN_PLATFORMS), "r": list(CRAN_R_VERSIONS)},
        "cran_container": {"platform": list(CRAN_CONTAINERS)},
        "cran_no_suggests": {
            "os": list(CRAN_NO_SUGGESTS_PLATFORMS),
            "r": list(CRAN_NO_SUGGESTS_R_VERSIONS),
        },
        "cran_manual": {
            "os": list(CRAN_MANUAL_PLATFORMS),
            "r": list(CRAN_MANUAL_R_VERSIONS),
        },
        "cran_rust": {
            "os": list(CRAN_RUST_PLATFORMS),
            "r": list(CRAN_RUST_R_VERSIONS),
            "rust": list(CRAN_RUST_VERSIONS),
        },
        "offline_source": {
            "os": list(OFFLINE_SOURCE_PLATFORMS),
            "host": list(OFFLINE_SOURCE_HOSTS),
        },
        "offline_toolchain": {
            "os": list(OFFLINE_TOOLCHAIN_PLATFORMS),
            "host": list(OFFLINE_TOOLCHAIN_HOSTS),
        },
        "offline_wheel": {
            "os": list(OFFLINE_WHEEL_PLATFORMS),
            "python": list(OFFLINE_WHEEL_PYTHONS),
        },
        "r_binary": {"os": list(R_BINARY_PLATFORMS), "r": list(R_BINARY_VERSIONS)},
        "cli": {
            "include": [
                {"os": platform, "target": target}
                for platform, target in CLI_TARGETS.items()
            ]
        },
        "share": {"os": list(SHARE_PLATFORMS), "route": list(SHARE_ROUTES)},
    }


def compare_cohorts(coverage, root, revision):
    """Compare the complete reports of every installed host with their truth.

    Each Python lane is compared beside the R source installation of its own
    platform, and a platform with Python lanes and no native-r lane is
    refused. Each R binary installation is compared beside the wheel lane of
    its platform with the build Python: on Windows the binary links through
    Rtools' GNU toolchain, where the wheel uses MSVC. Each command-line
    archive's saved outputs join the comparison of that wheel lane, beside
    its R reports.
    """
    matrix = coverage["qualify"]
    r_platforms = coverage["r"]["os"]
    unpaired = {row["os"] for row in matrix["include"]} - set(r_platforms)
    if unpaired:
        raise ValueError(
            "no native-r lane runs on the platform of Python lanes: "
            + ", ".join(sorted(unpaired))
        )
    binaries = list(
        itertools.product(coverage["r_binary"]["os"], coverage["r_binary"]["r"])
    )
    targets = {lane["os"]: lane["target"] for lane in coverage["cli"]["include"]}
    # The pinned download action flattens a single match, including a pattern.
    python_artifacts = len(matrix["include"])
    r_artifacts = len(r_platforms)
    cli_artifacts = len(targets)
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
        r_directory = evidence(f"native-r-qualification-{platform}", r_artifacts)
        beside = ["--r-evidence", str(r_directory / "r-source")]
        if (version, form) == (BUILD_PYTHON, "wheel") and platform in targets:
            target = targets.pop(platform)
            beside += [
                "--cli-evidence",
                str(evidence(f"public-conformance-cli-{target}", cli_artifacts)),
            ]
        comparisons.append((python_evidence, beside, output))
    if targets:
        raise ValueError(
            "no wheel lane with the build Python and R reports beside it runs on "
            "the platform of command-line targets: " + ", ".join(sorted(targets))
        )
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
    for python_evidence, beside, output in comparisons:
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
                *beside,
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
