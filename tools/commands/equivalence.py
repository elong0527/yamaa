"""Run the executor equivalence gate on fresh installs of both public hosts.

Rewrite 1 (#2785) replaces the executor and must leave every result
unchanged. The gate builds the Python and R packages from the checkout,
installs both outside it, and runs on those installs:

- public benchmark conformance for each host: every active benchmark
  reproduces its expected bytes, and every negative benchmark its expected
  first diagnostic (phase, condition, rule and specification paths);
  a repeat pass runs each entry's spec object a second time and requires it
  to reuse the plan its first run compiled, as its private observations
  record, with the same issues, tables and saved bytes (#2821);
- the full installed Python and R suites, which carry the cross-host
  diagnostic parity truth and the host-fixture families, and `R CMD check`
  of the small smoke suite that the source package ships.

Building fresh means the gate never judges a stale development extension.
`--hosts python` runs the Python half alone: its build, its conformance and
the installed suites; `--hosts r` the R half: its build, its conformance,
its full installed suites and `R CMD check`. Together the halves are the
whole gate, which runs by default.
CI runs each half in its corresponding host lane, sharing the fresh install
and installed suites with the ordinary host tests.
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
from tools.paths import ROOT

FIXTURES = ROOT / "benchmarks"
HOSTS = ("python", "r")


def install_python(work, uv, *, tooling=False):
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
        *(
            ("--group", "tooling", "--no-emit-package", "yamaa-tools")
            if tooling
            else ()
        ),
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
    if tooling:
        # Pytest's repository-tool tests need the locked tooling dependencies
        # and editable tools. The host itself remains the freshly built wheel.
        run(uv, "pip", "install", "--python", str(python), "--no-deps", "-e", str(ROOT))
    return python


def compare(work, fixtures=FIXTURES):
    """Judge one host's observations against the independent expected truth."""
    return public_conformance.main(
        [
            "compare",
            "--fixtures",
            str(fixtures),
            "--observed",
            str(work / "observed"),
            "--output",
            str(work / "inventory.json"),
        ]
    )


def python_conformance(work, python, plan, revision, fixtures=FIXTURES):
    subprocess.run(
        [
            str(python),
            "-I",
            str(ROOT / "python/tests/installed/public_conformance.py"),
            "--plan",
            str(plan),
            "--fixtures",
            str(fixtures),
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
    return compare(work, fixtures)


def r_conformance(work, environment, plan, revision, fixtures=FIXTURES):
    # Only full benchmark qualification needs the authored project programs.
    subprocess.run(
        [
            "R",
            "CMD",
            "INSTALL",
            f"--library={environment['R_LIBS'].split(os.pathsep)[0]}",
            str(ROOT / "rust/tests/project-programs/r"),
        ],
        cwd=work.parent,
        env=environment,
        check=True,
    )
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
            str(fixtures),
            str(work / "observed"),
            revision,
        ],
        cwd=work.parent,
        env=environment | {"PUBLIC_CONFORMANCE_PACKAGE_FORM": "source"},
        check=True,
    )
    return compare(work, fixtures)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--uv", default="uv")
    parser.add_argument(
        "--hosts",
        default="python,r",
        help="comma-separated hosts to run: python, r (default: python,r)",
    )
    args = parser.parse_args(argv)
    hosts = [host.strip() for host in args.hosts.split(",") if host.strip()]
    if not hosts or any(host not in HOSTS for host in hosts):
        parser.error(f"--hosts names python, r or both, not {args.hosts!r}")
    revision = subprocess.check_output(
        ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True
    ).strip()
    with tempfile.TemporaryDirectory(prefix="yamaa-equivalence-") as directory:
        work = Path(directory)
        plan = work / "plan"
        public_conformance.main(
            ["plan", "--fixtures", str(FIXTURES), "--output", str(plan)]
        )
        if "python" in hosts:
            python = install_python(work / "python-install", args.uv)
            if python_conformance(work / "python", python, plan, revision):
                raise SystemExit("Python public benchmark conformance failed")
            run_installed_suites(python, work / "python-suites")
        if "r" in hosts:
            archive, environment = r_tests.install(work / "r-install")
            r_tests.run_installed_suites(work / "r-suites", environment)
            if r_conformance(work / "r", environment, plan, revision):
                raise SystemExit("R public benchmark conformance failed")
            check(archive, work / "r-check", environment)
    print(f"equivalence gate passed on {', '.join(hosts)}")


if __name__ == "__main__":
    main()
