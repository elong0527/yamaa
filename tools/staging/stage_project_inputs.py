"""Stage original project studies and the one existing packaging lock."""

from __future__ import annotations

import argparse
import shutil
from pathlib import Path

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


def stage_case(source: Path, destination: Path, *, ignore=None) -> None:
    """Copy a study and supply the canonical packaging lock to Python entries."""
    shutil.copytree(source, destination, symlinks=True, ignore=ignore)
    for call in entries(source)["python"]:
        if call["environment"] is not None:
            lock = destination / Path(call["environment"]).parent / "uv.lock"
            if not lock.exists():
                shutil.copyfile(REPOSITORY / "python/uv.lock", lock)


def stage(destination: Path) -> None:
    destination.mkdir(parents=True)
    for name in CASES:
        case = destination / name
        stage_case(REPOSITORY / "benchmarks" / name, case)
    producer = destination / "shared-producer-parquet"
    producer.mkdir()
    for name in ("seed.csv", "producer.yaml", "spec.yaml"):
        # The native execution fixture has only the original authored inputs.
        # Independent expected reports and artifacts stay in the comparator tree.
        shutil.copyfile(
            REPOSITORY
            / "rust/qualification/producer-cohort/shared-producer-parquet"
            / name,
            producer / name,
        )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    stage(parser.parse_args().destination.resolve())
