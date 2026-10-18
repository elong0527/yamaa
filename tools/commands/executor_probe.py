"""Time the executor probe on a release build: a measurement, not a test.

Arguments pass through to the probe, for example
`--rows 1000000 --shape composite-key`. `--executor resolved` times the
executor over the resolved plan (#2841) through the test-only selector
instead of today's executor, until #2843 swaps it in. Timings are evidence:
record them on an issue with the printed revision, never in the repository.
"""

import os
import subprocess
import sys

from tools.paths import ROOT, WORKSPACE


def main():
    arguments = sys.argv[1:]
    executor = "rows"
    if "--executor" in arguments:
        index = arguments.index("--executor")
        if index + 1 >= len(arguments) or arguments[index + 1] not in (
            "rows",
            "resolved",
        ):
            raise SystemExit("--executor takes rows or resolved")
        executor = arguments[index + 1]
        del arguments[index : index + 2]
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
        f"revision {revision}{' with local changes' if changed else ''}, {toolchain},"
        f" {executor} executor",
        flush=True,
    )
    features = []
    environment = dict(os.environ)
    if executor == "resolved":
        features = ["--features", "yamaa-core/executor-comparison"]
        environment["YAMAA_EXECUTOR"] = "resolved"
    else:
        environment.pop("YAMAA_EXECUTOR", None)
    subprocess.run(
        [
            "cargo",
            "run",
            "--release",
            "--locked",
            "-p",
            "yamaa",
            *features,
            "--example",
            "executor_probe",
            "--",
            *arguments,
        ],
        cwd=WORKSPACE,
        env=environment,
        check=True,
    )


if __name__ == "__main__":
    main()
