"""Run package/tool tests and standalone public-host suites without release metadata."""

import subprocess
import sys
import tempfile
from pathlib import Path

from tools.commands.installed import stage
from tools.conformance.supplemental_inventory import load_catalog
from tools.paths import ROOT


def run_installed_suites(python, work):
    """Stage the cataloged Python suites into work and run them with python."""
    catalog = load_catalog(ROOT, ROOT / "rust/qualification/supplemental.json")
    stage("python", work)
    for suite in catalog["suites"]:
        if suite["runtime"] == "python":
            subprocess.run(
                [str(python), "-E", "-s", str(work / suite["staged_path"])],
                cwd=work,
                check=True,
            )


def main():
    subprocess.run(
        [sys.executable, "-m", "pytest", "python/tests", "tools/tests"],
        cwd=ROOT,
        check=True,
    )
    with tempfile.TemporaryDirectory(prefix="yamaa-python-tests-") as directory:
        run_installed_suites(sys.executable, Path(directory))


if __name__ == "__main__":
    main()
