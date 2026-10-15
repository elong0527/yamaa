"""Build, install and check the R package in a fresh external directory."""

import os
import subprocess
import tempfile
from pathlib import Path

from tools.commands.check_r import check
from tools.paths import ROOT
from tools.staging.stage_r_package import stage


def main():
    with tempfile.TemporaryDirectory(prefix="yamaa-r-check-") as directory:
        work = Path(directory)
        library = work / "library"
        library.mkdir()
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
                'packages <- trimws(strsplit(read.dcf("R/yamaa/DESCRIPTION")[1,"Suggests"], ",", fixed=TRUE)[[1]]); stopifnot(all(vapply(packages, requireNamespace, logical(1), quietly=TRUE)))',
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
        for package in (ROOT / "rust/tests/project-programs/r", archives[0]):
            subprocess.run(
                ["R", "CMD", "INSTALL", f"--library={library}", str(package)],
                cwd=work,
                env=environment,
                check=True,
            )
        check(archives[0], work / "check", environment)


if __name__ == "__main__":
    main()
