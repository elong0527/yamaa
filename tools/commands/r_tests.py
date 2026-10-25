"""Build the R package, run its full installed suites, and check it as CRAN does.

The package is installed twice, by R CMD INSTALL and by R CMD check. When
YAMAA_R_CARGO_TARGET_DIR names a directory, as in CI's r job, both build the
engine there, and the second must find the first one's build complete.
"""

import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

from tools.commands import installed
from tools.commands.check_r import check
from tools.conformance import supplemental_inventory as inventory
from tools.packaging.check_cargo_lock import check_bundle_archive
from tools.paths import ROOT
from tools.staging.stage_r_package import stage

# The line that Cargo prints for each crate that it compiles.
COMPILING = re.compile(r"^\s*Compiling (\S+) v", re.MULTILINE)


def install(work):
    """Build the staged R package in work and install it without benchmark helpers.

    Returns the source archive and the environment whose R_LIBS selects them.
    """
    library = work / "library"
    library.mkdir(parents=True)
    existing = os.environ.get("R_LIBS", os.environ.get("R_LIBS_USER", ""))
    environment = os.environ | {
        "R_LIBS": os.pathsep.join(filter(None, [str(library), existing])),
        "_R_CHECK_CRAN_INCOMING_REMOTE_": "false",
    }
    subprocess.run(
        [
            "Rscript",
            "--vanilla",
            "-e",
            'suggests <- strsplit(read.dcf("R/yamaa/DESCRIPTION")[1,"Suggests"], ",", fixed=TRUE)[[1]]; packages <- trimws(sub("[[:space:]]*[(][^)]*[)]", "", suggests)); stopifnot(all(vapply(packages, requireNamespace, logical(1), quietly=TRUE)))',
        ],
        cwd=ROOT,
        env=environment,
        check=True,
    )
    stage(work / "yamaa")
    subprocess.run(
        ["R", "CMD", "build", "--no-build-vignettes", "--no-manual", "yamaa"],
        cwd=work,
        env=environment,
        check=True,
    )
    archives = list(work.glob("yamaa_*.tar.gz"))
    if len(archives) != 1:
        raise RuntimeError("R build did not produce one source archive")
    # The archive carries the verified R engine bundle and no toolchain file.
    _, kept = check_bundle_archive(archives[0])
    print(f"{archives[0].name}: R engine bundle with {len(kept)} lock packages")
    subprocess.run(
        ["R", "CMD", "INSTALL", f"--library={library}", str(archives[0])],
        cwd=work,
        env=environment,
        check=True,
    )
    return archives[0], environment


def check_shared_build(log):
    """Refuse an installation log in which Cargo compiled a crate.

    `log` is the 00install.out of the installation in R CMD check, which
    shares YAMAA_R_CARGO_TARGET_DIR with the R CMD INSTALL of the same package
    before it.
    """
    crates = COMPILING.findall(log)
    if crates:
        raise RuntimeError(
            "R CMD check's installation compiled "
            + ", ".join(crates)
            + " again instead of reusing the build in YAMAA_R_CARGO_TARGET_DIR: "
            "a build script may watch a directory, whose time unpacking sets, or "
            "an environment variable that differs between the installations"
        )
    print(
        "R CMD check's installation reused the engine build in YAMAA_R_CARGO_TARGET_DIR"
    )


def test_configure():
    # The package's configure step runs in R, so its tests run here, beside
    # the package check, rather than with the Python tests.
    subprocess.run(
        [
            sys.executable,
            "-m",
            "pytest",
            "-q",
            "-rs",
            "tools/tests/test_r_configure.py",
        ],
        cwd=ROOT,
        check=True,
    )


def check_install(archive, environment, work):
    """Check a fresh archive, including the shared engine-build assertion."""
    check(archive, work, environment)
    if os.environ.get("YAMAA_R_CARGO_TARGET_DIR"):
        (log,) = work.glob("*.Rcheck/00install.out")
        check_shared_build(log.read_text(encoding="utf-8"))


def run_installed_suites(work, environment):
    """Run every cataloged R suite on the fresh install, outside the checkout.

    The CRAN archive carries only smoke checks. Ordinary CI and make test-r
    retain the full suite denominator, with the same installed package and
    dependency library that R CMD check uses. Report every failing suite.
    """
    catalog = inventory.load_catalog(
        ROOT, ROOT / "rust/qualification/supplemental.json"
    )
    installed.stage("r", work)
    failed = []
    for suite in catalog["suites"]:
        if suite["runtime"] != "r":
            continue
        path = suite["staged_path"]
        result = subprocess.run(
            ["Rscript", "--vanilla", path],
            cwd=work,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        print(f"== {path}", flush=True)
        print(result.stdout, end="", flush=True)
        if result.returncode:
            failed.append(path)
    if failed:
        raise RuntimeError(f"installed R suites failed: {', '.join(failed)}")


def main():
    test_configure()
    with tempfile.TemporaryDirectory(prefix="yamaa-r-check-") as directory:
        work = Path(directory)
        archive, environment = install(work)
        run_installed_suites(work / "suites", environment)
        check_install(archive, environment, work / "check")


if __name__ == "__main__":
    main()
