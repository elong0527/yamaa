"""Stage original project studies and the one existing packaging lock."""

from __future__ import annotations

import argparse
import shutil
from pathlib import Path, PurePosixPath

import yaml
from tools.benchmark import entries
from tools.paths import ROOT

REPOSITORY = ROOT
CASES = (
    "schema-functions",
    "schema-non-finite",
    "adam-adsl-bmi",
    "adam-advs-percentiles",
    "negative-function-contract",
)


def python_lock(case: Path, project: str) -> PurePosixPath | None:
    """Where the Python packaging lock a project names is staged, relative to
    the case, or None when the project names none.

    A project names its lock in `environments.python.lock`, relative to the
    project file (FUNC-3).
    """
    document = yaml.safe_load((case / project).read_text(encoding="utf-8"))
    environments = document.get("environments") if isinstance(document, dict) else None
    python = environments.get("python") if isinstance(environments, dict) else None
    lock = python.get("lock") if isinstance(python, dict) else None
    # A project's own paths resolve from the project file.
    return None if lock is None else PurePosixPath(project).parent / lock


def stage_case(source: Path, destination: Path, *, ignore=None) -> None:
    """Copy a study and supply the canonical packaging lock to Python entries."""
    shutil.copytree(source, destination, symlinks=True, ignore=ignore)
    for call in entries(source)["python"]:
        if call["environment"] is None:
            continue
        lock = python_lock(source, call["environment"])
        if lock is None:
            continue
        target = destination / lock
        if not target.exists():
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(REPOSITORY / "python/uv.lock", target)


def stage(destination: Path) -> None:
    destination.mkdir(parents=True)
    for name in CASES:
        case = destination / name
        stage_case(REPOSITORY / "benchmarks" / name, case)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    stage(parser.parse_args().destination.resolve())
