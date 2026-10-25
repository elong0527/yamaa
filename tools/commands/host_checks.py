"""Run a host's tests and optional equivalence on the same fresh installation.

Python's package/tools tests and installed suites use a new wheel environment.
R's full installed suites, package check and public benchmark conformance
use one external source installation. Test failures are collected so that
a failed ordinary suite does not hide the benchmark result, or the reverse.
"""

import argparse
import subprocess
import tempfile
from pathlib import Path

from tools.commands import equivalence, python_tests, r_tests
from tools.conformance import public_conformance
from tools.paths import ROOT


def record(failed, label, check):
    """Record a failed check and allow independent checks to finish."""
    try:
        result = check()
        if result:
            failed.append(label)
            print(f"{label} failed: {result}", flush=True)
    except (subprocess.CalledProcessError, RuntimeError, ValueError) as error:
        failed.append(label)
        print(f"{label} failed: {error}", flush=True)


def benchmark(work, check):
    """Plan and judge benchmarks inside their check, after ordinary tests.

    A malformed benchmark plan must not prevent the package tests from running.
    """
    revision = subprocess.check_output(
        ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True
    ).strip()
    plan = work / "plan"
    status = public_conformance.main(
        ["plan", "--fixtures", str(equivalence.FIXTURES), "--output", str(plan)]
    )
    return status or check(plan, revision)


def run(host, work, uv, qualify):
    """Return every failed check for the selected host."""
    failed = []
    if host == "python":
        python = equivalence.install_python(work / "install", uv, tooling=True)
        record(
            failed,
            "Python tests",
            lambda: python_tests.run_tests(python, work / "suites"),
        )
        if qualify:
            record(
                failed,
                "Python equivalence",
                lambda: benchmark(
                    work,
                    lambda plan, revision: equivalence.python_conformance(
                        work / "equivalence", python, plan, revision
                    ),
                ),
            )
    else:
        record(failed, "R configure tests", r_tests.test_configure)
        archive, environment = r_tests.install(work / "install")
        record(
            failed,
            "R installed suites",
            lambda: r_tests.run_installed_suites(work / "suites", environment),
        )
        record(
            failed,
            "R package check",
            lambda: r_tests.check_install(archive, environment, work / "check"),
        )
        if qualify:
            record(
                failed,
                "R equivalence",
                lambda: benchmark(
                    work,
                    lambda plan, revision: equivalence.r_conformance(
                        work / "equivalence", environment, plan, revision
                    ),
                ),
            )
    return failed


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", choices=("python", "r"), required=True)
    parser.add_argument("--uv", default="uv")
    parser.add_argument("--equivalence", action="store_true")
    args = parser.parse_args(argv)
    with tempfile.TemporaryDirectory(prefix=f"yamaa-{args.host}-checks-") as directory:
        failed = run(args.host, Path(directory), args.uv, args.equivalence)
    if failed:
        parser.exit(1, f"failed: {'; '.join(failed)}\n")


if __name__ == "__main__":
    main()
