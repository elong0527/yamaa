"""Run the workspace tests, leaving out the two host bindings."""

import subprocess

from tools.paths import WORKSPACE

# The bindings have no Rust tests (`test = false` in their manifests); the
# installed Python and R suites test them. Leaving them out keeps PyO3 and
# extendr, whose builds need Python and R, out of the workspace tests.
BINDINGS = ("yamaa-python", "yamaa-r")


def command():
    """The workspace test command, as CI's `rust` job runs it."""
    exclusions = (argument for crate in BINDINGS for argument in ("--exclude", crate))
    return ["cargo", "test", "--locked", "--workspace", *exclusions]


def main():
    subprocess.run(command(), cwd=WORKSPACE, check=True)


if __name__ == "__main__":
    main()
