"""Run all shared workspace tests with debug and release code generation."""

import subprocess

from tools.paths import WORKSPACE


def main():
    for profile in ([], ["--profile", "release-test"]):
        subprocess.run(
            ["cargo", "test", "--locked", "--all-targets", "--all-features", *profile],
            cwd=WORKSPACE,
            check=True,
        )


if __name__ == "__main__":
    main()
