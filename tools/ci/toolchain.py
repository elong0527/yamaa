"""Activate the Rust toolchain declared by the workspace."""

import argparse
import os
import subprocess
import tomllib
from pathlib import Path

from tools.paths import WORKSPACE


def declaration():
    return tomllib.loads((WORKSPACE / "rust-toolchain.toml").read_text())["toolchain"]


def channel():
    return declaration()["channel"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install", action="store_true")
    arguments = parser.parse_args()
    toolchain = declaration()
    if arguments.install:
        subprocess.run(
            [
                "rustup",
                "toolchain",
                "install",
                toolchain["channel"],
                "--profile",
                toolchain["profile"],
                "--no-self-update",
                *[
                    argument
                    for component in toolchain.get("components", [])
                    for argument in ("--component", component)
                ],
            ],
            check=True,
        )
    if github_environment := os.environ.get("GITHUB_ENV"):
        with Path(github_environment).open("a") as output:
            output.write(f"RUSTUP_TOOLCHAIN={toolchain['channel']}\n")


if __name__ == "__main__":
    main()
