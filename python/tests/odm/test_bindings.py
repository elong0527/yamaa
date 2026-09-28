from __future__ import annotations

from pathlib import Path

import pytest

from yamaa.io import ProjectResources, load_source_tables
from yamaa.models import TypedColumn
from yamaa.odm import BindingFailure, BoundReference, build_binding_plan
from yamaa.specification import load_specification

REPOSITORY = Path(__file__).parents[3]


def _fixture_plan():
    root = REPOSITORY / "benchmarks/schema-odm-form-items"
    loaded_spec = load_specification(root / "spec.yaml", REPOSITORY / "yaml")
    sources = load_source_tables(
        loaded_spec.specification.input,
        ProjectResources(root),
    )
    return build_binding_plan(loaded_spec.specification, sources), sources


def test_plan_resolves_output_and_dataset_names() -> None:
    plan, _ = _fixture_plan()

    assert plan.bind("LBDTC") == BoundReference(
        name="LBDTC",
        kind="output",
        field="LBDTC",
    )
    assert plan.bind("ODM.StudyOID") == BoundReference(
        name="ODM.StudyOID",
        kind="dataset",
        dataset="ODM",
        field="StudyOID",
    )


@pytest.mark.parametrize(
    "name",
    # REQ-1265: an ODM item is read with `odm`, so an ItemOID suffix on a
    # long-form relation names no field.
    ["UNKNOWN", "OTHER.Value", "ODM.IT.LB.LBDTC", "ODM.AGE"],
)
def test_plan_rejects_unknown_names(name: str) -> None:
    plan, _ = _fixture_plan()

    result = plan.bind(name)

    assert isinstance(result, BindingFailure)
    assert result.condition.condition == "unknown_field"
    assert result.condition.context == {"identifier": name}


def test_plan_requires_every_normalized_source_table() -> None:
    plan, sources = _fixture_plan()
    del plan

    loaded_spec = load_specification(
        REPOSITORY / "benchmarks/schema-odm-form-items/spec.yaml",
        REPOSITORY / "yaml",
    )
    with pytest.raises(ValueError, match="exactly match"):
        build_binding_plan(loaded_spec.specification, {})

    assert sources["ODM"].table.columns[0] == TypedColumn(name="StudyOID", type="str")
