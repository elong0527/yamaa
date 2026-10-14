"""Replay the locked debug, engine test-support and optimized Rust test suites."""

import subprocess
from pathlib import Path

import tomllib

WORKSPACE = Path(__file__).resolve().parents[3] / "rust"


def main():
    with (WORKSPACE / "rust-toolchain.toml").open("rb") as source:
        toolchain = tomllib.load(source)["toolchain"]["channel"]
    for arguments in (
        [],
        ["-p", "yamaa-engine", "--features", "test-support"],
        [
            "-p",
            "yamaa-core",
            "-p",
            "yamaa-engine",
            "-p",
            "yamaa-adapters",
            "--profile",
            "release-test",
        ],
    ):
        subprocess.run(
            ["cargo", f"+{toolchain}", "test", "--locked", *arguments],
            cwd=WORKSPACE,
            check=True,
        )


if __name__ == "__main__":
    main()
