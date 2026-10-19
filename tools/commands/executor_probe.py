"""Time the executor probe on a release build: a measurement, not a test.

Arguments pass through to the probe, for example
`--rows 1000000 --shape composite-key`. Timings are evidence: record them on
an issue with the printed revision, never in the repository.
"""

import subprocess
import sys

from tools.paths import ROOT, WORKSPACE


def main():
    arguments = sys.argv[1:]
    revision = subprocess.check_output(
        ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True
    ).strip()
    changed = subprocess.check_output(
        ["git", "-C", str(ROOT), "status", "--porcelain", "--untracked-files=no"],
        text=True,
    ).strip()
    toolchain = subprocess.check_output(
        ["rustc", "--version"], cwd=WORKSPACE, text=True
    ).strip()
    print(
        f"revision {revision}{' with local changes' if changed else ''}, {toolchain}",
        flush=True,
    )
    subprocess.run(
        [
            "cargo",
            "run",
            "--release",
            "--locked",
            "-p",
            "yamaa",
            "--example",
            "executor_probe",
            "--",
            *arguments,
        ],
        cwd=WORKSPACE,
        check=True,
    )


if __name__ == "__main__":
    main()
