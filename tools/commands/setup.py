"""Install repository tools editable after building the normal development wheel."""

import argparse
import os
import subprocess

from tools.ci.toolchain import channel
from tools.paths import ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--uv", default="uv")
    uv = parser.parse_args().uv
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
