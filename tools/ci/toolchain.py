"""Activate the Rust toolchain declared by the workspace."""

import argparse
import os
import subprocess
import tomllib
from pathlib import Path

from tools.paths import WORKSPACE

# Repository checks lint and format with the first two, and read the third,
# the standard library's sources, for the crates that the third-party notices
# list (tools/packaging/generate_third_party_notices.py). The toolchain file
# names only what a build needs, so rustup adds them only where these checks
# run; no package build or installation needs them.
CHECK_COMPONENTS = ("clippy", "rustfmt", "rust-src")


def declaration(workspace=WORKSPACE):
    return tomllib.loads((workspace / "rust-toolchain.toml").read_text())["toolchain"]


def channel(workspace=WORKSPACE):
    return declaration(workspace)["channel"]


def minimum():
    """The workspace `rust-version` as a complete release, such as 1.82.0."""
    manifest = tomllib.loads((WORKSPACE / "Cargo.toml").read_text())
    version = manifest["workspace"]["package"]["rust-version"]
    return ".".join((version.split(".") + ["0", "0"])[:3])


def install_command(toolchain):
    """The rustup command that installs a toolchain for repository checks."""
    components = dict.fromkeys([*toolchain.get("components", []), *CHECK_COMPONENTS])
    return [
        "rustup",
        "toolchain",
        "install",
        toolchain["channel"],
        "--profile",
        toolchain["profile"],
        "--no-self-update",
        *[
            argument
            for component in components
            for argument in ("--component", component)
        ],
    ]


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
        subprocess.run(install_command(toolchain), check=True)
    if github_environment := os.environ.get("GITHUB_ENV"):
        with Path(github_environment).open("a") as output:
            output.write(f"RUSTUP_TOOLCHAIN={toolchain['channel']}\n")


if __name__ == "__main__":
    main()
