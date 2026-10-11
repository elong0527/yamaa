"""Stage original project studies and the one existing packaging lock."""

from __future__ import annotations

import argparse
import shutil
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[2]
CASES = (
    "schema-functions",
    "schema-non-finite",
    "adam-adsl-bmi",
    "adam-advs-percentiles",
    "negative-function-contract",
)


def stage(destination: Path) -> None:
    destination.mkdir(parents=True)
    for name in CASES:
        case = destination / name
        shutil.copytree(REPOSITORY / "benchmarks" / name, case)
        # The sole repository Python lock is packaging-tool-owned. Its exact
        # bytes become local study metadata; no second lock is committed.
        shutil.copyfile(REPOSITORY / "python/uv.lock", case / "python/uv.lock")
    producer = destination / "shared-producer-parquet"
    producer.mkdir()
    for name in ("seed.csv", "producer.yaml", "spec.yaml"):
        # The native execution fixture has only the original authored inputs.
        # Independent expected reports and artifacts stay in the comparator tree.
        shutil.copyfile(
            REPOSITORY / "rust/qualification/producer-cohort/shared-producer-parquet" / name,
            producer / name,
        )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    stage(parser.parse_args().destination.resolve())
