"""Build the Linux wheel inside the manylinux_2_28 container of this machine.

The container's glibc 2.28 and compilers build the extension, including the
bundled ReadStat and zstd C sources, so the wheel runs on glibc 2.28 and
later, for example RHEL 8. maturin refuses a wheel that needs more than the
manylinux_2_28 policy allows. The checkout is mounted read-only, and Cargo
builds in the container, so no host build output reaches the wheel.
"""

import argparse
import platform
import subprocess
import tomllib
from pathlib import Path

from tools.ci import toolchain
from tools.paths import ROOT

POLICY = "manylinux_2_28"
# Pinned by their dated tags; move both together.
IMAGES = {
    "x86_64": "quay.io/pypa/manylinux_2_28_x86_64:2026.10.16-1",
    "aarch64": "quay.io/pypa/manylinux_2_28_aarch64:2026.10.16-1",
}
# The oldest supported CPython in the image builds the abi3 wheel.
PYTHON = "/opt/python/cp312-cp312/bin/python"

SCRIPT = """\
set -euo pipefail
export CARGO_HOME=/tmp/cargo RUSTUP_HOME=/tmp/rustup CARGO_TARGET_DIR=/tmp/target
export PATH="${CARGO_HOME}/bin:${PATH}"
curl --proto '=https' --tlsv1.2 -sSf -o /tmp/rustup-init \\
  "https://static.rust-lang.org/rustup/dist/$(uname -m)-unknown-linux-gnu/rustup-init"
chmod +x /tmp/rustup-init
/tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain none
rustup toolchain install "${RUST_TOOLCHAIN}" --profile minimal --no-self-update
"${PYTHON}" -m pip install --root-user-action=ignore "${MATURIN}"
cd /io/python
"${PYTHON}" -m maturin build --profile dist --locked --compatibility "${POLICY}" \\
  --interpreter "${PYTHON}" --out /out
"""


def maturin():
    """The maturin requirement that the Python build system pins."""
    [requirement] = tomllib.loads((ROOT / "python/pyproject.toml").read_text())[
        "build-system"
    ]["requires"]
    return requirement


def command(engine, architecture, output, jobs=None):
    """The container command that builds the wheel for `architecture` into `output`."""
    environment = {
        "RUST_TOOLCHAIN": toolchain.channel(),
        "MATURIN": maturin(),
        "POLICY": POLICY,
        "PYTHON": PYTHON,
    }
    if jobs is not None:
        environment["CARGO_BUILD_JOBS"] = str(jobs)
    return [
        engine,
        "run",
        "--rm",
        "--volume",
        f"{ROOT}:/io:ro",
        "--volume",
        f"{output}:/out",
        *[
            argument
            for name, value in environment.items()
            for argument in ("--env", f"{name}={value}")
        ],
        IMAGES[architecture],
        "bash",
        "-c",
        SCRIPT,
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--engine", default="docker")
    parser.add_argument("--jobs", type=int, help="Cargo build jobs in the container")
    args = parser.parse_args()
    architecture = platform.machine()
    if architecture not in IMAGES:
        parser.error(f"no manylinux image for {architecture}")
    # Created here, so the host user can add the source archive beside the wheel.
    args.out.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        command(args.engine, architecture, args.out.resolve(), args.jobs), check=True
    )


if __name__ == "__main__":
    main()
