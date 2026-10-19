"""Run the executor equivalence gate on fresh installs of both public hosts.

Rewrite 1 (#2785) replaces the executor and must leave every result
unchanged. The gate builds the Python and R packages from the checkout,
installs both outside it, and runs on those installs:

- public benchmark conformance for each host: every active benchmark
  reproduces its expected bytes, and every negative benchmark its expected
  first diagnostic (phase, condition, requirement and specification paths);
  a repeat pass runs each entry's spec object a second time and requires it
  to reuse the plan its first run compiled, as its private observations
  record, with the same issues, tables and saved bytes (#2821);
- the installed Python suites and `R CMD check`, which carry the cross-host
  diagnostic parity truth and the host-fixture families.

First, until #2843 swaps in the executor over the resolved plan, it runs the
`yamaa-adapters` and `yamaa` test suites with the test-only executor
selector set to `compare`: every plan those suites execute runs on both
executors, and the two attempts must be equal (output, diagnostics, handler
counts in order, retained check records, callback order and observer events;
Q7 of #2840). That is every benchmark except those the `EXEMPTIONS` list of
`memory_resolver_benchmarks.rs` names, which call project functions, expect a
file-ingest diagnostic or judge filesystem paths.

Building fresh means the gate never judges a stale development extension.
"""

import argparse
import os
import subprocess
import sys
import tempfile
from pathlib import Path

from tools.ci.toolchain import channel
from tools.commands import r_tests
from tools.commands.check_r import check
from tools.commands.python_tests import run_installed_suites
from tools.conformance import public_conformance
from tools.paths import ROOT, WORKSPACE

FIXTURES = ROOT / "benchmarks"


def install_python(work, uv):
    """Install wheels built from the checkout, with the locked test
    requirements, into a new environment; return its interpreter."""
    environment = os.environ | {"RUSTUP_TOOLCHAIN": channel()}
    distributions = work / "dist"
    requirements = work / "requirements.txt"

    def run(*arguments):
        subprocess.run(arguments, cwd=ROOT, env=environment, check=True)

    run(
        uv,
        "build",
        "python",
        "--wheel",
        "--out-dir",
        str(distributions),
        "--config-setting",
        "build-args=--profile=dev",
    )
    run(
        uv,
        "build",
        "rust/tests/project-programs/python",
        "--wheel",
        "--out-dir",
        str(distributions),
    )
    run(
        uv,
        "export",
        "--project",
        "python",
        "--locked",
        "--extra",
        "test",
        "--no-dev",
        "--no-emit-project",
        "--no-hashes",
        "--output-file",
        str(requirements),
    )
    run(uv, "venv", "--python", sys.executable, str(work / "venv"))
    python = work / "venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
    run(
        uv,
        "pip",
        "install",
        "--python",
        str(python),
        "--no-deps",
        "-r",
        str(requirements),
        *sorted(map(str, distributions.glob("*.whl"))),
    )
    return python


def compare(work):
    """Judge one host's observations against the independent expected truth."""
    return public_conformance.main(
        [
            "compare",
            "--fixtures",
            str(FIXTURES),
            "--observed",
            str(work / "observed"),
            "--output",
            str(work / "inventory.json"),
        ]
    )


def python_conformance(work, python, plan, revision):
    subprocess.run(
        [
            str(python),
            "-I",
            str(ROOT / "python/tests/installed/public_conformance.py"),
            "--plan",
            str(plan),
            "--fixtures",
            str(FIXTURES),
            "--python-lock",
            str(ROOT / "python/uv.lock"),
            "--output",
            str(work / "observed"),
            "--source-revision",
            revision,
        ],
        cwd=work.parent,
        check=True,
    )
    return compare(work)


def r_conformance(work, environment, plan, revision):
    runner = ROOT / "rust/tests/public_conformance.R"
    subprocess.run(
        [
            "Rscript",
            "--vanilla",
            str(ROOT / "rust/tests/test_public_conformance.R"),
            str(runner),
        ],
        cwd=work.parent,
        env=environment,
        check=True,
    )
    subprocess.run(
        [
            "Rscript",
            "--vanilla",
            str(runner),
            str(plan),
            str(FIXTURES),
            str(work / "observed"),
            revision,
        ],
        cwd=work.parent,
        env=environment | {"PUBLIC_CONFORMANCE_PACKAGE_FORM": "source"},
        check=True,
    )
    return compare(work)


def compare_executors():
    """Run the suites that execute specifications on both executors; a
    differing attempt aborts the test process with both attempts printed."""
    subprocess.run(
        [
            "cargo",
            "test",
            "--locked",
            "-p",
            "yamaa-adapters",
            "-p",
            "yamaa",
            "--features",
            "yamaa-core/executor-comparison",
        ],
        cwd=WORKSPACE,
        env=os.environ | {"YAMAA_EXECUTOR": "compare"},
        check=True,
    )


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--uv", default="uv")
    uv = parser.parse_args(argv).uv
    revision = subprocess.check_output(
        ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True
    ).strip()
    compare_executors()
    with tempfile.TemporaryDirectory(prefix="yamaa-equivalence-") as directory:
        work = Path(directory)
        plan = work / "plan"
        public_conformance.main(
            ["plan", "--fixtures", str(FIXTURES), "--output", str(plan)]
        )
        python = install_python(work / "python-install", uv)
        if python_conformance(work / "python", python, plan, revision):
            raise SystemExit("Python public benchmark conformance failed")
        run_installed_suites(python, work / "python-suites")
        archive, environment = r_tests.install(work / "r-install")
        if r_conformance(work / "r", environment, plan, revision):
            raise SystemExit("R public benchmark conformance failed")
        check(archive, work / "r-check", environment)
    print("equivalence gate passed")


if __name__ == "__main__":
    main()
