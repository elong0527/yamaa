"""Stage original unchanged compiler inputs and independent report truth."""

import argparse
import json
import shutil
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[2]
CASES = ("negative-zero-division", "negative-integer-overflow", "adam-adlb-ordered-sum", "schema-window-functions", "schema-inheritance", "schema-lookup")
# The current specification schema closure, not other standalone schema roots.
# Missing/new includes fail shared bundle admission; no runtime parser is used here.
SCHEMA_MODULES = (
    "schema.yaml",
    "schema_shared.yaml",
    "schema_derivation.yaml",
    "schema_verification.yaml",
    "schema_metadata.yaml",
    "schema_function.yaml",
    "schema_expression_core.yaml",
    "schema_expression_aggregate.yaml",
    "schema_expression_numeric.yaml",
    "schema_expression_str.yaml",
    "schema_expression_date.yaml",
    "schema_expression_mapping.yaml",
    "schema_expression_window.yaml",
    "schema_expression_odm.yaml",
)


def stage(destination: Path):
    """Keep original files separate from independently authored expected reports."""
    destination.mkdir(parents=True, exist_ok=False)
    (destination / "schema").mkdir()
    for name in SCHEMA_MODULES:
        shutil.copy2(REPOSITORY / "yaml" / name, destination / "schema" / name)
    for name in CASES:
        shutil.copytree(REPOSITORY / "benchmarks" / name, destination / "cases" / name)
    shutil.copytree(
        REPOSITORY / "rust/crates/yamaa-adapters/tests/fixtures/specifications",
        destination / "expected",
    )

    # R compares complete JSON without adding a JSON-library runtime dependency.
    for path in (destination / "expected").glob("*.json"):
        path.write_text(
            json.dumps(
                json.loads(path.read_text()), sort_keys=True, separators=(",", ":")
            ),
            encoding="utf-8",
        )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    stage(parser.parse_args().destination)
