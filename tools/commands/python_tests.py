"""Run package/tool tests and standalone public-host suites without release metadata."""

import os
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from tools.commands.installed import stage
from tools.conformance.supplemental_inventory import load_catalog
from tools.paths import ROOT


def run_suite(work, staged_path):
    return subprocess.run(
        [sys.executable, "-E", "-s", str(work / staged_path)],
        cwd=work,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )


def main():
    catalog = load_catalog(ROOT, ROOT / "rust/qualification/supplemental.json")
    suites = [
        suite["staged_path"]
        for suite in catalog["suites"]
        if suite["runtime"] == "python"
    ]
    failed = []
    with tempfile.TemporaryDirectory(prefix="yamaa-python-tests-") as directory:
        work = Path(directory)
        stage("python", work)
        # Each standalone suite is its own process over the staged copy, so
        # they run beside pytest; one core stays with pytest.
        workers = max(1, (os.cpu_count() or 2) - 1)
        with ThreadPoolExecutor(max_workers=workers) as pool:
            runs = [pool.submit(run_suite, work, path) for path in suites]
            tests = subprocess.run(
                [sys.executable, "-m", "pytest", "python/tests", "tools/tests"],
                cwd=ROOT,
            )
            if tests.returncode:
                failed.append("pytest")
            # Report in catalog order once each suite has finished.
            for path, run in zip(suites, runs):
                result = run.result()
                print(f"== {path}", flush=True)
                print(result.stdout, end="", flush=True)
                if result.returncode:
                    failed.append(path)
    if failed:
        sys.exit(f"failed: {', '.join(failed)}")


if __name__ == "__main__":
    main()
