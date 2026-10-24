"""Check a host with the engine loaded at runtime, as native qualification's
native-loaded jobs do (rust/interface/README.md).

Each check installs the host fresh in the test-only loaded mode and runs the
suites that load each fixture engine of the loading tests: for Python, the
installed suites, on a new wheel environment; for R, R CMD check of the
installed package, whose tests run with it. The repository's tool tests and
the R configure tests load no engine and are left out; CI's loaded lane runs
the complete host checks in the loaded mode on Linux
(tools/commands/host_checks.py). Each check's logs are copied into the
directory that --logs names, whether it passes or fails, for the job to
upload. YAMAA_ENGINE=loaded selects the mode, as for every other check.
"""

import argparse
import os
import shutil
import tempfile
from pathlib import Path

from tools.commands import equivalence, host_checks, loaded, python_tests, r_tests

# The logs that R CMD check leaves in its directory: the check's, the
# package installation's and each test file's output.
R_LOGS = (
    "*.Rcheck/00check.log",
    "*.Rcheck/00install.out",
    "*.Rcheck/tests/*.Rout*",
)


def keep(directory, patterns, logs):
    """Copy each file under `directory` that a pattern matches into `logs`,
    at its path relative to `directory`."""
    for pattern in patterns:
        for path in sorted(directory.glob(pattern)):
            target = logs / path.relative_to(directory)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, target)


def check_python(work, uv, logs):
    """Install a fresh wheel in the loaded mode and run the installed suites
    with the fixture engines; keep their output."""
    python = equivalence.install_python(work / "install", uv)
    fixtures = loaded.build_fixtures(work / "fixture-engines")
    os.environ[loaded.FIXTURES_VARIABLE] = str(fixtures)
    python_tests.run_installed_suites(
        python, work / "suites", log=logs / "installed-suites.log"
    )


def check_r(work, logs):
    """Install the R package in the loaded mode and check it, its tests with
    the fixture engines; keep R CMD check's logs."""
    archive, environment = r_tests.install(work / "install")
    try:
        r_tests.check_install(archive, environment, work / "check")
    finally:
        keep(work / "check", R_LOGS, logs)


def run(host, work, uv, logs):
    """Return the failed check of the selected host, if it failed."""
    failed = []
    if host == "python":
        host_checks.record(
            failed, "Python installed suites", lambda: check_python(work, uv, logs)
        )
    else:
        host_checks.record(failed, "R package check", lambda: check_r(work, logs))
    return failed


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", choices=("python", "r"), required=True)
    parser.add_argument("--uv", default="uv")
    parser.add_argument("--logs", type=Path, required=True)
    args = parser.parse_args(argv)
    if not loaded.loaded():
        parser.error(
            f"these checks are of the loaded mode: set {loaded.VARIABLE}=loaded"
        )
    logs = args.logs.resolve()
    logs.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=f"yamaa-{args.host}-loaded-") as directory:
        failed = run(args.host, Path(directory), args.uv, logs)
    if failed:
        parser.exit(1, f"failed: {'; '.join(failed)}\n")


if __name__ == "__main__":
    main()
