from __future__ import annotations

import runpy
from pathlib import Path

import polars as pl
import pytest
from polars.testing import assert_frame_equal

EXAMPLES = Path(__file__).parents[2] / "yaml/examples"


def positive_runners() -> tuple[Path, ...]:
    return tuple(
        runner
        for runner in sorted(EXAMPLES.glob("*/run.py"))
        if not (runner.parent / "expected/error.yaml").exists()
    )


@pytest.mark.parametrize(
    "runner",
    positive_runners(),
    ids=lambda runner: runner.parent.name,
)
def test_positive_example_outputs_match_expected_csvs(
    runner: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    example = runner.parent
    expected = {
        path.name: path for path in sorted((example / "expected").glob("*.csv"))
    }
    assert expected, f"{example.name} has run.py but no expected CSV"

    monkeypatch.chdir(example)
    namespace = runpy.run_path(runner.name)
    outputs = namespace["df"]

    assert isinstance(outputs, dict)
    assert set(outputs) == set(expected)
    for filename, expected_path in expected.items():
        actual = outputs[filename]
        assert isinstance(actual, pl.DataFrame)
        committed = pl.read_csv(expected_path, schema=actual.schema)
        assert_frame_equal(actual, committed, check_exact=True)
