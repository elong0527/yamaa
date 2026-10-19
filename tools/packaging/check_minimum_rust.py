"""Check that the locked graph builds with the workspace's minimum Rust.

`rust-version` in rust/Cargo.toml is the oldest Rust that builds the engine,
and the R and Python source packages build with the caller's toolchain, which
CRAN asks to test with a cargo at least two years old. Cargo reads that field
from each package, so a dependency that declares a newer one breaks those
builds, however recent the toolchain that CI pins.

This fails when a locked package that one of the supported targets compiles,
along the normal, build and development edges of the workspace members,
needs a newer Rust than the workspace declares: a newer `rust-version`, or
an edition that the minimum Rust does not have. It also fails when any
locked package, whatever its target, uses such an edition: a source package
vendors every locked crate, and cargo reads every vendored manifest before
it builds.
"""

import json
import subprocess
import sys
import tomllib

from tools.paths import WORKSPACE

# The targets that the wheels, the command-line binaries and the R package
# are built for, each with the edges that it compiles.
TARGETS = (
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "x86_64-pc-windows-gnu",
)

# The first Rust release of each edition.
EDITIONS = {"2015": "1.0", "2018": "1.31", "2021": "1.56", "2024": "1.85"}


def release(version):
    """A Rust version as a comparable (major, minor, patch)."""
    return tuple(int(part) for part in (version.split(".") + ["0", "0"])[:3])


def minimum(workspace=WORKSPACE):
    """The workspace `rust-version`."""
    manifest = tomllib.loads((workspace / "Cargo.toml").read_text(encoding="utf-8"))
    return manifest["workspace"]["package"]["rust-version"]


def needs(package):
    """The oldest Rust that reads and builds `package`: its declared
    `rust-version`, or its edition's first release when that is newer."""
    edition = EDITIONS[package["edition"]]
    declared = package.get("rust_version") or edition
    return max(declared, edition, key=release)


def locked_metadata(workspace=WORKSPACE):
    """`cargo metadata` for the whole locked graph, every target included."""
    command = ["cargo", "metadata", "--locked", "--all-features"]
    command += ["--format-version", "1"]
    return json.loads(subprocess.check_output(command, cwd=workspace))


def compiled(target, workspace=WORKSPACE):
    """The (name, version) of each package that `target` compiles for the
    workspace members, members included, as Cargo's feature resolver selects
    them. `cargo metadata` would also list the optional dependencies that only
    a weak feature names, which the lock holds but no build compiles."""
    output = subprocess.check_output(
        [
            "cargo",
            "tree",
            "--locked",
            "--workspace",
            "--all-features",
            "--target",
            target,
            "--edges",
            "normal,build,dev",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ],
        cwd=workspace,
        text=True,
    )
    return {
        (line.split()[0], line.split()[1].removeprefix("v"))
        for line in output.splitlines()
        if line.strip()
    }


def label(package):
    return f"{package['name']} {package['version']}"


def problems(declared, every, by_target):
    """Messages for the packages that need a newer Rust than `declared`.

    `every` is the metadata of the whole locked graph and `by_target` maps
    each target to the (name, version) of each package it compiles.
    """
    floor = release(declared)
    packages = {
        (package["name"], package["version"]): package for package in every["packages"]
    }
    newer = {}
    for target, selected in by_target.items():
        for package in map(packages.__getitem__, sorted(selected)):
            if release(needs(package)) > floor:
                newer.setdefault(label(package), (needs(package), []))[1].append(target)
    unreadable = sorted(
        f"{label(package)} (edition {package['edition']})"
        for package in every["packages"]
        if release(EDITIONS[package["edition"]]) > floor
    )
    messages = []
    if newer:
        messages.append(
            f"These locked packages need a newer Rust than the workspace's "
            f"rust-version {declared}: "
            + "; ".join(
                f"{name} needs {version}, built for {', '.join(targets)}"
                for name, (version, targets) in sorted(newer.items())
            )
        )
    if unreadable:
        messages.append(
            f"Cargo {declared} cannot read these locked packages, which a "
            f"source package vendors: {', '.join(unreadable)}"
        )
    return messages


def main():
    declared = minimum()
    every = locked_metadata()
    by_target = {target: compiled(target) for target in TARGETS}
    messages = problems(declared, every, by_target)
    for message in messages:
        print(message, file=sys.stderr)
    if messages:
        raise SystemExit(1)
    print(f"Every locked package builds with Rust {declared} on {len(TARGETS)} targets")


if __name__ == "__main__":
    main()
