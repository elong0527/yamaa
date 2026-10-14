"""Check a standalone R archive outside the checkout and require Status: OK."""

import argparse
import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", required=True, type=Path)
    parser.add_argument("--work-dir", required=True, type=Path)
    args = parser.parse_args()
    archive, work = args.archive.resolve(), args.work_dir.resolve()
    if work.is_relative_to(ROOT):
        parser.error("R package checks must run outside the checkout")
    if not archive.is_file():
        parser.error("--archive must identify the built source archive")
    if not os.environ.get("R_LIBS"):
        parser.error(
            "R_LIBS must identify the installed package and dependency library"
        )
    # Never overwrite previous evidence; an unsuccessful check keeps its logs.
    work.mkdir(parents=True)
    revision = subprocess.check_output(
        ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True
    )
    (work / "source-revision.txt").write_text(revision, encoding="utf-8")
    subprocess.run(
        ["R", "CMD", "check", "--no-manual", str(archive)], cwd=work, check=True
    )
    logs = list(work.glob("*.Rcheck/00check.log"))
    if (
        len(logs) != 1
        or "Status: OK" not in logs[0].read_text(encoding="utf-8").splitlines()
    ):
        raise RuntimeError("R package check has unresolved findings")


if __name__ == "__main__":
    main()
