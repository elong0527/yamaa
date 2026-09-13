from __future__ import annotations

import json
from pathlib import Path

import polars as pl
import pytest

from yamaa import DomainRunError, yamaa_domain

REPOSITORY_ROOT = Path(__file__).parents[2]
EXAMPLES = REPOSITORY_ROOT / "yaml/examples"
DM_EXAMPLE = EXAMPLES / "sdtm-dm-basic"


def test_one_argument_loads_and_executes_a_domain_specification() -> None:
    pilot = yamaa_domain(DM_EXAMPLE / "spec.yaml")

    assert pilot.spec is not None
    assert pilot.spec.domain == "DM"
    assert list(pilot.inputs) == ["ODM"]
    assert isinstance(pilot.inputs["ODM"], pl.DataFrame)
    assert pilot.input["ODM"].equals(pilot.inputs["ODM"])
    assert pilot.output is not None
    assert pilot.output.shape == (4, 8)
    assert pilot.output.get_column("AGE").to_list() == [34, 28, None, None]
    assert pilot.output.get_column("ACTARM").to_list() == [
        "Placebo",
        "Vitamin D3",
        "Unassigned",
        "Unassigned",
    ]
    assert pilot.issues.is_empty()
    assert pilot.issues.schema == {
        "severity": pl.String,
        "phase": pl.String,
        "condition": pl.String,
        "spec_paths": pl.List(pl.String),
        "context": pl.String,
    }


def test_save_uses_the_requested_extension_without_changing_output(
    tmp_path: Path,
) -> None:
    pilot = yamaa_domain(DM_EXAMPLE / "spec.yaml")
    expected = pilot.output
    target = tmp_path / "expected.parquet"

    saved = pilot.save(target)

    assert saved == target
    assert expected is not None
    assert pl.read_parquet(saved).equals(expected)
    assert pilot.output is not None
    assert pilot.output.equals(expected)


def test_invalid_specification_is_available_through_issues() -> None:
    pilot = yamaa_domain(EXAMPLES / "negative-column-type-unknown/spec.yaml")

    assert pilot.spec is None
    assert pilot.inputs == {}
    assert pilot.output is None
    assert pilot.issues.row(0, named=True) == {
        "severity": "error",
        "phase": "validation",
        "condition": "value_not_permitted",
        "spec_paths": ["columns.AVAL.type"],
        "context": (
            '{"permitted":["str","int","float","date","datetime"],"value":"number"}'
        ),
    }

    with pytest.raises(DomainRunError) as raised:
        pilot.save("expected.parquet")

    assert raised.value.issues.equals(pilot.issues)


def test_preflight_issues_prevent_input_loading() -> None:
    pilot = yamaa_domain(EXAMPLES / "negative-source-output-self-reference/spec.yaml")

    assert pilot.spec is not None
    assert pilot.inputs == {}
    assert pilot.output is None
    issue = pilot.issues.row(0, named=True)
    assert issue["condition"] == "duplicate_identifier"
    assert issue["spec_paths"] == ["datasets.ADLB", "domain"]
    assert json.loads(issue["context"]) == {"identifier": "ADLB"}
