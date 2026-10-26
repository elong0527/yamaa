"""Activate the Rust toolchain declared by the workspace, or another one.

Without options, the channel that rust/rust-toolchain.toml pins; with
--minimum, the workspace rust-version, the oldest Rust that builds the
engine; with --channel, another rustup channel, such as beta, the next
stable release. With --host, that release built for another host that the
machine runs, such as the GNU host on Windows that CRAN's checks build with.
With --install, rustup installs the selected toolchain with the components
that repository checks use. In CI, the selection is written to GITHUB_ENV as
RUSTUP_TOOLCHAIN for the steps that follow.
"""

import argparse
import os
import subprocess
import tomllib
from pathlib import Path

from tools.paths import WORKSPACE

# Repository checks lint and format with these. The toolchain file names only
# what a build needs, so rustup adds them only where these checks run.
CHECK_COMPONENTS = ("clippy", "rustfmt")
# The third-party notices generator reads the standard library's sources for
# the crates that the notices list (tools/packaging/generate_third_party_notices.py).
# Only the jobs that run the generator add them, with --notices (#3030 D157):
# no package build, installation, test or lint needs them (#3528).
NOTICE_COMPONENTS = ("rust-src",)


def declaration(workspace=WORKSPACE):
    return tomllib.loads((workspace / "rust-toolchain.toml").read_text())["toolchain"]


def channel(workspace=WORKSPACE):
    return declaration(workspace)["channel"]


def minimum():
    """The workspace `rust-version` as a complete release, such as 1.82.0."""
    manifest = tomllib.loads((WORKSPACE / "Cargo.toml").read_text())
    version = manifest["workspace"]["package"]["rust-version"]
    return ".".join((version.split(".") + ["0", "0"])[:3])


def install_command(toolchain, notices=False):
    """The rustup command that installs a toolchain for repository checks, with
    the components that the notices generator reads when `notices` is true."""
    components = dict.fromkeys(
        [
            *toolchain.get("components", []),
            *CHECK_COMPONENTS,
            *(NOTICE_COMPONENTS if notices else ()),
        ]
    )
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


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install", action="store_true")
    parser.add_argument(
        "--minimum",
        action="store_true",
        help="use the workspace rust-version instead of the pinned channel",
    )
    parser.add_argument(
        "--channel",
        metavar="CHANNEL",
        help="use this rustup channel, such as beta, instead of the pinned one",
    )
    parser.add_argument(
        "--host",
        metavar="TRIPLE",
        help="use the toolchain built for this host, such as "
        "x86_64-pc-windows-gnu, instead of rustup's default host",
    )
    parser.add_argument(
        "--notices",
        action="store_true",
        help="with --install, also add the components that the third-party "
        "notices generator reads",
    )
    arguments = parser.parse_args(argv)
    toolchain = declaration()
    if arguments.minimum and arguments.channel:
        parser.error("--minimum and --channel each select the toolchain; give one")
    if arguments.minimum:
        toolchain["channel"] = minimum()
    elif arguments.channel:
        toolchain["channel"] = arguments.channel
    if arguments.host:
        toolchain["channel"] = f"{toolchain['channel']}-{arguments.host}"
    if arguments.install:
        subprocess.run(install_command(toolchain, arguments.notices), check=True)
    if github_environment := os.environ.get("GITHUB_ENV"):
        with Path(github_environment).open("a") as output:
            output.write(f"RUSTUP_TOOLCHAIN={toolchain['channel']}\n")


if __name__ == "__main__":
    main()
