"""Named key identity remains separate from dependency evaluation order."""

import polars as pl
import pytest

from yamaa.models import TypedColumn, TypedTable
from yamaa.runtime import ExecutionFailure, ExecutionSuccess, execute_specification
from yamaa.specification.models import (
    Column,
    DatasetSource,
    Expression,
    HandledExpression,
    Output,
    Specification,
)


def run(columns, keys, source, source_types):
    """Use real planning and execution without assuming any key evaluation order."""
    spec = Specification(
        schema_version="1.0",
        domain="OUT",
        input={"SRC": DatasetSource(path="input.csv")},
        keys=keys,
        output=Output(path="out.csv", columns=[c[0] for c in columns]),
        columns=[
            Column(
                name=name,
                type=kind,
                derivation=HandledExpression(value=Expression(root=expression)),
            )
            for name, kind, expression in columns
        ],
    )
    typed = tuple(
        TypedColumn(name=name, type=kind) for name, kind in source_types.items()
    )
    frame = pl.DataFrame(
        source,
        schema={
            name: {"str": pl.String, "int": pl.Int64}[kind]
            for name, kind in source_types.items()
        },
    )
    return execute_specification(spec, {"SRC": TypedTable(columns=typed, frame=frame)})


@pytest.mark.parametrize(
    "second_type,seconds", [("str", ["second", "third", "second"]), ("int", [2, 3, 2])]
)
def test_key_names_keep_values_when_identity_order_differs_from_column_order(
    second_type, seconds
):
    """Different identity order must never exchange typed column values."""
    result = run(
        [("A", "str", {"source": "SRC.X"}), ("B", second_type, {"source": "SRC.Y"})],
        ["B", "A"],
        {"X": ["first", "next", "first"], "Y": seconds},
        {"X": "str", "Y": second_type},
    )
    assert isinstance(result, ExecutionSuccess)
    assert result.table.frame.rows() == [("first", seconds[0]), ("next", seconds[1])]


def test_dependent_keys_keep_dependency_order_and_named_values():
    """Reordering identity must preserve prerequisite evaluation as well as values."""
    result = run(
        [
            ("A", "int", {"source": "SRC.X"}),
            ("B", "int", {"compute": {"expr": "A + 1"}}),
        ],
        ["B", "A"],
        {"X": [4, 2, 4]},
        {"X": "int"},
    )
    assert isinstance(result, ExecutionSuccess)
    assert result.table.frame.rows() == [(4, 5), (2, 3)]


def test_missing_key_diagnostic_names_original_column():
    """A reordered identity reports the missing field and correctly named complete keys."""
    result = run(
        [("A", "str", {"source": "SRC.X"}), ("B", "str", {"source": "SRC.Y"})],
        ["B", "A"],
        {"X": [None], "Y": ["present"]},
        {"X": "str", "Y": "str"},
    )
    assert isinstance(result, ExecutionFailure)
    (diagnostic,) = result.diagnostics
    assert diagnostic.condition == "missing_key"
    assert diagnostic.spec_paths == ("keys[1]",)
    assert diagnostic.context == {
        "column": "A",
        "missing_count": 1,
        "keys": [{"B": "present", "A": None}],
    }


def test_valid_key_cannot_conceal_a_missing_key_record():
    """User text and integers cannot impersonate the internal missing-row identity."""
    result = run(
        [("A", "int", {"source": "SRC.X"}), ("B", "str", {"source": "SRC.Y"})],
        ["B", "A"],
        {"X": [1, None], "Y": ["__missing_key__", "present"]},
        {"X": "int", "Y": "str"},
    )
    assert isinstance(result, ExecutionFailure)
    (diagnostic,) = result.diagnostics
    assert diagnostic.condition == "missing_key"
    assert diagnostic.context == {
        "column": "A",
        "missing_count": 1,
        "keys": [{"B": "present", "A": None}],
    }


@pytest.mark.parametrize("missing_position", [0, 1])
def test_missing_key_token_does_not_combine_unrelated_feeding_records(missing_position):
    """Both arrival orders keep present and missing rows' non-key readings separate."""
    rows = [(missing_position, "__missing_key__", "valid")]
    rows.insert(missing_position, (None, "present", "missing"))
    result = run(
        [
            ("A", "int", {"source": "SRC.X"}),
            ("B", "str", {"source": "SRC.Y"}),
            ("VALUE", "str", {"source": "SRC.Z"}),
        ],
        ["B", "A"],
        {name: [row[i] for row in rows] for i, name in enumerate(("X", "Y", "Z"))},
        {"X": "int", "Y": "str", "Z": "str"},
    )
    assert isinstance(result, ExecutionFailure)
    (diagnostic,) = result.diagnostics
    assert diagnostic.condition == "missing_key"
    assert diagnostic.spec_paths == ("keys[1]",)
    assert diagnostic.context == {
        "column": "A",
        "missing_count": 1,
        "keys": [{"B": "present", "A": None}],
    }
