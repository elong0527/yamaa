"""Install repository tools editable after building the normal development wheel.

`--without-package` installs the same locked environment and tools without the
development package, for a CI job that never imports it.
"""

import argparse
import os
import shutil
import subprocess

from tools.ci.toolchain import channel, declaration, install_command
from tools.paths import ROOT


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--uv", default="uv")
    parser.add_argument(
        "--without-package",
        action="store_true",
        help="leave out the development package, and so its engine build",
    )
    arguments = parser.parse_args(argv)
    uv = arguments.uv
    # The toolchain file names only what a build needs, so `make quick` needs
    # Clippy and rustfmt for its lint, and rust-src for its notice check,
    # added to the pinned toolchain where rustup runs.
    if shutil.which("rustup"):
        subprocess.run(install_command(declaration()), cwd=ROOT, check=True)
    environment = os.environ | {"RUSTUP_TOOLCHAIN": channel()}
    subprocess.run(
        [
            uv,
            "sync",
            "--project",
            "python",
            "--extra",
            "test",
            "--group",
            "benchmark",
            "--group",
            "tooling",
            "--locked",
            "--no-editable",
            "--config-setting",
            "build-args=--profile=dev",
            # The docs workflow leaves the package out of its sync the same way.
            *(("--no-install-package", "yamaa") if arguments.without_package else ()),
        ],
        cwd=ROOT,
        env=environment,
        check=True,
    )
    interpreter = subprocess.check_output(
        [
            uv,
            "run",
            "--project",
            "python",
            "--no-sync",
            "python",
            "-c",
            "import sys; print(sys.executable)",
        ],
        cwd=ROOT,
        text=True,
    ).strip()
    subprocess.run(
        [
            uv,
            "pip",
            "install",
            "--python",
            interpreter,
            "--no-deps",
            "--editable",
            str(ROOT),
        ],
        cwd=ROOT,
        check=True,
    )


if __name__ == "__main__":
    main()
