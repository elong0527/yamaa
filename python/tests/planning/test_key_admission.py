"""Undeclared output identity must fail before join inference or source access."""

import pytest

from yamaa.adapters.native_datasets import (
    execute_with_source_provider as native_execute,
)
from yamaa.io.polars import frame_from_values
from yamaa.models import TypedColumn
from yamaa.planning import (
    ExecutionPlanningError,
    UnsupportedPlanningError,
    plan_execution,
    preflight_execution,
)
from yamaa.runtime import ExecutionFailure
from yamaa.runtime.executor import execute_with_source_provider
from yamaa.specification.models import (
    Column,
    DatasetSource,
    Expression,
    HandledExpression,
    Output,
    Row,
    Specification,
)


def invalid_key_specification(has_rows=False):
    """Name an absent output key that a secondary source may independently carry."""
    return Specification(
        schema_version="1.0",
        domain="OUT",
        input={
            "SRC": DatasetSource(path="src.csv"),
            "OTHER": DatasetSource(path="other.csv"),
        },
        base="SRC",
        keys=["MISSING"],
        output=Output(path="out.csv", columns=["K", "V"]),
        columns=[
            Column(
                name=name,
                type=kind,
                derivation=HandledExpression(value=Expression(root={"source": source})),
            )
            for name, kind, source in [("K", "str", "SRC.X"), ("V", "int", "OTHER.N")]
        ],
        rows=[Row(id="row", dataset="SRC", derivations={})] if has_rows else None,
    )


def assert_key_diagnostic(diagnostic):
    """Pin the authored declaration location and independent REQ-0220 truth."""
    assert diagnostic.condition == "undeclared_column"
    assert diagnostic.spec_paths == ("keys[0]",)
    assert diagnostic.requirement == "REQ-0220"
    assert diagnostic.context == {"column": "MISSING"}


@pytest.mark.parametrize("has_rows", [False, True])
@pytest.mark.parametrize("source_has_key", [False, True])
def test_unknown_output_key_never_indexes_source_types(has_rows, source_has_key):
    """Direct planning reports invalid identity independently of right-field overlap."""
    spec = invalid_key_specification(has_rows)
    other_columns = [TypedColumn(name="N", type="int")]
    other_values = [1]
    if source_has_key:
        other_columns.insert(0, TypedColumn(name="MISSING", type="str"))
        other_values.insert(0, "x")
    sources = {
        "SRC": frame_from_values((TypedColumn(name="X", type="str"),), [["x"]]),
        "OTHER": frame_from_values(tuple(other_columns), [other_values]),
    }
    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(spec, sources)
    assert_key_diagnostic(raised.value.diagnostics[0])
    assert not any(
        item.condition in {"no_applicable_keys", "incompatible_input_type"}
        for item in raised.value.diagnostics
    )


@pytest.mark.parametrize("has_rows", [False, True])
@pytest.mark.parametrize("execute", [execute_with_source_provider, native_execute])
def test_unknown_output_key_fails_before_provider(execute, has_rows):
    """Both frontends reject invalid identity before any source provider effects."""

    def forbidden_provider(_inputs):
        """Make accidental acquisition observable rather than returning an empty source."""
        pytest.fail("an undeclared output key reached source acquisition")

    result = execute(invalid_key_specification(has_rows), forbidden_provider)
    if execute is native_execute:
        result = result.result
    assert isinstance(result, ExecutionFailure)
    (diagnostic,) = result.diagnostics
    assert_key_diagnostic(diagnostic)


def test_inherited_output_key_waits_for_parent_normalization():
    """A parent may declare the key, so raw inheritance remains explicitly unsupported."""
    spec = invalid_key_specification().model_copy(update={"parents": ["parent.yaml"]})
    with pytest.raises(UnsupportedPlanningError) as raised:
        preflight_execution(spec)
    assert [(item.operation, item.spec_path) for item in raised.value.features] == [
        ("inheritance", "parents")
    ]
