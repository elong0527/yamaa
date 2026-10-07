"""Independent shared-run report truth also matches the original reference path."""

import json
from pathlib import Path

import pytest

from yamaa.adapters.conformance import execute_example

ROOT = Path(__file__).resolve().parents[3]


@pytest.mark.parametrize(
    "name", ["negative-zero-division", "negative-integer-overflow"]
)
def test_authored_reports_match_reference_without_native_execution(name, tmp_path):
    actual = json.loads(
        execute_example(
            ROOT / "benchmarks" / name,
            schema_root=ROOT / "yaml",
            output_dir=tmp_path / name,
            backend="python",
        ).model_dump_json()
    )
    expected = json.loads(
        (
            ROOT
            / "rust/crates/yamaa-adapters/tests/fixtures/specifications"
            / (name + ".json")
        ).read_text()
    )
    for field in ("runtime", "backend", "runtime_version", "engine_version"):
        expected[field] = actual[field]
    assert actual == expected
