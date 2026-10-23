"""Run package/tool tests and standalone public-host suites without release metadata."""

import os
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from tools.commands import loaded
from tools.commands.installed import stage
from tools.conformance.supplemental_inventory import load_catalog
from tools.paths import ROOT


def run_installed_suites(python, work, workers=None):
    """Stage the cataloged Python suites into work and run them with python.

    Each suite is its own process over the staged copy, so they run in
    parallel; their output is printed in catalog order and every failing
    suite is reported.
    """
    catalog = load_catalog(ROOT, ROOT / "rust/qualification/supplemental.json")
    stage("python", work)
    suites = [
        suite["staged_path"]
        for suite in catalog["suites"]
        if suite["runtime"] == "python"
    ]

    def run(staged_path):
        return subprocess.run(
            [str(python), "-E", "-s", str(work / staged_path)],
            cwd=work,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )

    with ThreadPoolExecutor(max_workers=workers or os.cpu_count() or 1) as pool:
        results = list(pool.map(run, suites))
    failed = []
    for path, result in zip(suites, results):
        print(f"== {path}", flush=True)
        print(result.stdout, end="", flush=True)
        if result.returncode:
            failed.append(path)
    if failed:
        raise RuntimeError(f"installed suites failed: {', '.join(failed)}")


def run_tests(python, work):
    """Run pytest and the installed suites on one interpreter; return failures."""
    failed = []
    if loaded.loaded():
        # The suites load each fixture engine beside a copy of the package.
        fixtures = loaded.build_fixtures(work / "fixture-engines")
        os.environ[loaded.FIXTURES_VARIABLE] = str(fixtures)
    with ThreadPoolExecutor(max_workers=1) as background:
        # The suites run beside pytest; one core stays with pytest.
        suites = background.submit(
            run_installed_suites,
            python,
            work,
            max(1, (os.cpu_count() or 2) - 1),
        )
        tests = subprocess.run(
            [str(python), "-m", "pytest", "python/tests", "tools/tests"],
            cwd=ROOT,
        )
        if tests.returncode:
            failed.append("pytest")
        try:
            suites.result()
        except RuntimeError as error:
            failed.append(str(error))
    return failed


def main():
    with tempfile.TemporaryDirectory(prefix="yamaa-python-tests-") as directory:
        failed = run_tests(sys.executable, Path(directory))
    if failed:
        sys.exit(f"failed: {'; '.join(failed)}")


if __name__ == "__main__":
    main()
