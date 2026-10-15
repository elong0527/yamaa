"""Run the complete workspace, including normally linked host-binding tests."""

import os
import subprocess
import sys

from tools.paths import WORKSPACE


def main():
    environment = dict(os.environ)
    environment.setdefault("PYO3_PYTHON", sys.executable)
    subprocess.run(
        ["cargo", "test", "--locked", "--workspace"],
        cwd=WORKSPACE,
        env=environment,
        check=True,
    )


if __name__ == "__main__":
    main()
