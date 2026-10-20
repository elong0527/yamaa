"""Build, install and check the R package as CRAN does, in a fresh external directory."""

import os
import subprocess
import sys
import tempfile
from pathlib import Path

from tools.commands.check_r import check
from tools.packaging.check_cargo_lock import check_bundle_archive
from tools.paths import ROOT
from tools.staging.stage_r_package import stage


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


def main():
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
    with tempfile.TemporaryDirectory(prefix="yamaa-r-check-") as directory:
        work = Path(directory)
        archive, environment = install(work)
        check(archive, work / "check", environment)


if __name__ == "__main__":
    main()
