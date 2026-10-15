"""Read the workspace's sole Rust toolchain declaration."""

import os
import tomllib
from pathlib import Path

from tools.paths import WORKSPACE


def channel():
    return tomllib.loads((WORKSPACE / "rust-toolchain.toml").read_text())["toolchain"][
        "channel"
    ]


def main():
    with Path(os.environ["GITHUB_ENV"]).open("a") as output:
        output.write(f"RUSTUP_TOOLCHAIN={channel()}\n")


if __name__ == "__main__":
    main()
