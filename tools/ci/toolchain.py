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


def minimum():
    """The workspace `rust-version` as a complete release, such as 1.90.0."""
    manifest = tomllib.loads((WORKSPACE / "Cargo.toml").read_text())
    version = manifest["workspace"]["package"]["rust-version"]
    return ".".join((version.split(".") + ["0", "0"])[:3])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install", action="store_true")
    parser.add_argument(
        "--minimum",
        action="store_true",
        help="use the workspace rust-version instead of the pinned channel",
    )
    arguments = parser.parse_args()
    toolchain = declaration()
    if arguments.minimum:
        toolchain["channel"] = minimum()
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
