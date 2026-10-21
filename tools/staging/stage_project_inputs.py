"""Stage original project studies and the one existing packaging lock."""

from __future__ import annotations

import argparse
import shutil
from pathlib import Path, PurePosixPath

import yaml
from tools.benchmark import entries
from tools.paths import ROOT
from tools.surface import declares_header

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

    A 1.x project names its lock in `environments.python.lock`, relative to
    the project file (FUNC-3). A 0.5 project links a Python environment file,
    whose own relative spellings start at that file, and its lock is staged
    as `uv.lock` beside it.
    """
    document = yaml.safe_load((case / project).read_text(encoding="utf-8"))
    environments = document.get("environments") if isinstance(document, dict) else None
    linked = environments.get("python") if isinstance(environments, dict) else None
    if linked is None:
        return None
    # A project's own paths resolve from the project file.
    base = PurePosixPath(project).parent
    if declares_header(document):
        lock = linked.get("lock") if isinstance(linked, dict) else None
        return None if lock is None else base / lock
    return base / PurePosixPath(linked).parent / "uv.lock"


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
