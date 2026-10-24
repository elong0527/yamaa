"""Install repository tools editable after building the normal development wheel.

`--without-package` installs the same locked environment and tools without the
development package, for a CI job that never imports it. `--keep-toolchain`
adds no components to the pinned toolchain, which CI's setup action installs
first with the components that its job needs.
"""

import argparse
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

from tools.ci.toolchain import channel, declaration, install_command
from tools.commands import loaded
from tools.paths import ROOT


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--uv", default="uv")
    parser.add_argument(
        "--without-package",
        action="store_true",
        help="leave out the development package, and so its engine build",
    )
    parser.add_argument(
        "--keep-toolchain",
        action="store_true",
        help="leave the pinned toolchain's components as the caller installed them",
    )
    arguments = parser.parse_args(argv)
    uv = arguments.uv
    # The toolchain file names only what a build needs, so `make quick` needs
    # Clippy and rustfmt for its lint, and rust-src for its notice check,
    # added to the pinned toolchain where rustup runs. CI's setup action adds
    # rust-src only for the jobs that run the notices generator
    # (tools.ci.toolchain --notices), and passes --keep-toolchain.
    if shutil.which("rustup") and not arguments.keep_toolchain:
        subprocess.run(
            install_command(declaration(), notices=True), cwd=ROOT, check=True
        )
    environment = os.environ | {"RUSTUP_TOOLCHAIN": channel()}
    sync = [
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
    ]
    subprocess.run(sync, cwd=ROOT, env=environment, check=True)
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
    if not arguments.without_package:
        engine_mode(uv, interpreter, environment, sync)


def engine_mode(uv, interpreter, environment, sync):
    """Install the package in the link mode YAMAA_ENGINE selects. The locked
    environment holds the linked build; the loaded mode replaces it with a
    build that links no engine, and places the engine library beside its
    module (tools/commands/loaded.py). A linked setup after a loaded one,
    whose engine library is still beside the module, syncs the linked build
    in again and takes that library away."""
    library = loaded.package_directory(interpreter) / loaded.LIBRARY
    if not loaded.loaded():
        if library.exists() or library.is_symlink():
            subprocess.run(
                [*sync, "--reinstall-package", "yamaa"],
                cwd=ROOT,
                env=environment,
                check=True,
            )
            library.unlink()
        return
    with tempfile.TemporaryDirectory(prefix="yamaa-loaded-wheel-") as directory:
        subprocess.run(
            [
                uv,
                "build",
                "python",
                "--wheel",
                "--out-dir",
                directory,
                "--config-setting",
                f"build-args=--profile=dev {loaded.WHEEL_ARGUMENTS}",
            ],
            cwd=ROOT,
            env=environment,
            check=True,
        )
        subprocess.run(
            [
                uv,
                "pip",
                "install",
                "--python",
                interpreter,
                "--no-deps",
                "--reinstall",
                *map(str, Path(directory).glob("*.whl")),
            ],
            cwd=ROOT,
            check=True,
        )
    loaded.install_beside(interpreter, loaded.build_engine())


if __name__ == "__main__":
    main()
