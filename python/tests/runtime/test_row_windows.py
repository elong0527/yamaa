"""Row-construction windows: issue #659.

Each row template evaluates its own window expressions in a post-emission
pass scoped to the rows that template constructed. These tests cover the
template scoping, the tie-break, the window-on-window rules, and the
unchanged validation guards, at the runtime level.
"""

from __future__ import annotations

import polars as pl
import pytest

from yamaa.models import TypedColumn, TypedTable
from yamaa.runtime import (
    ExecutionFailure,
    ExecutionSuccess,
    execute_specification,
)
from yamaa.specification.models import (
    Column,
    DatasetSource,
    Expression,
    HandledExpression,
    Output,
    Row,
    Specification,
)


def derive(expression):
    return HandledExpression(value=Expression(root=expression))


def make_spec(rows, columns, output_columns, column_derivations=None):
    column_derivations = column_derivations or {}
    return Specification(
        schema_version="1.0",
        domain="OUT",
        input={"SRC": DatasetSource(path="input/source.csv")},
        base="SRC",
        keys=["GRP", "SEQ"],
        output=Output(path="out.csv", columns=output_columns),
        columns=[
            Column(
                name=name,
                type=type_,
                derivation=derive(column_derivations[name])
                if name in column_derivations
                else None,
            )
            for name, type_ in columns
        ],
        rows=rows,
    )


def lag_window(window, source="VAL", offset=-1):
    return {
        "row_value": {
            "source": source,
            "offset": offset,
            "window": window,
        }
    }


def visits_source():
    return {
        "SRC": TypedTable(
            columns=(
                TypedColumn(name="G", type="str"),
                TypedColumn(name="X", type="float"),
                TypedColumn(name="S", type="int"),
            ),
            frame=pl.DataFrame(
                {
                    "G": ["a", "a", "a", "b", "b"],
                    "X": [1.0, 2.0, 3.0, 10.0, 20.0],
                    "S": [1, 2, 3, 1, 2],
                },
                schema={"G": pl.String, "X": pl.Float64, "S": pl.Int64},
            ),
        )
    }


def lag_template(window, extra_derivations=None, row_id="visits", row_filter=None):
    derivations = {
        "GRP": derive({"source": "SRC.G"}),
        "SEQ": derive({"source": "SRC.S"}),
        "VAL": derive({"source": "SRC.X"}),
        "PREV": derive(lag_window(window, source="VAL")),
    }
    derivations.update(extra_derivations or {})
    return Row(id=row_id, filter=row_filter, derivations=derivations)


COLUMNS = [
    ("GRP", "str"),
    ("SEQ", "int"),
    ("VAL", "float"),
    ("PREV", "float"),
]


def test_row_window_lags_within_each_partition() -> None:
    specification = make_spec(
        [lag_template({"group_by": ["GRP"], "order_by": ["SEQ"]})],
        COLUMNS,
        ["GRP", "SEQ", "VAL", "PREV"],
    )

    result = execute_specification(specification, visits_source())

    assert isinstance(result, ExecutionSuccess)
    assert result.artifact.frame.rows() == [
        ("a", 1, 1.0, None),
        ("a", 2, 2.0, 1.0),
        ("a", 3, 3.0, 2.0),
        ("b", 1, 10.0, None),
        ("b", 2, 20.0, 10.0),
    ]


def test_row_window_breaks_order_ties_by_construction_order() -> None:
    # REQ-0301: rows tied on every order term keep their construction
    # order inside the window, so the lag is deterministic.
    specification = make_spec(
        [lag_template({"group_by": ["GRP"], "order_by": ["GRP"]})],
        COLUMNS,
        ["GRP", "SEQ", "VAL", "PREV"],
    )

    result = execute_specification(specification, visits_source())

    assert isinstance(result, ExecutionSuccess)
    assert result.artifact.frame.rows() == [
        ("a", 1, 1.0, None),
        ("a", 2, 2.0, 1.0),
        ("a", 3, 3.0, 2.0),
        ("b", 1, 10.0, None),
        ("b", 2, 20.0, 10.0),
    ]


def test_row_window_sees_only_its_own_templates_rows() -> None:
    # Both templates derive the same columns and lag with one global
    # partition. If template B's window pass saw template A's staged rows,
    # B's first row would lag A's last row instead of yielding missing.
    template_a = lag_template(
        {"order_by": ["SEQ"]}, row_id="a_only", row_filter="SRC.G = 'a'"
    )
    template_b = lag_template(
        {"order_by": ["SEQ"]}, row_id="b_only", row_filter="SRC.G = 'b'"
    )
    specification = make_spec(
        [template_a, template_b],
        COLUMNS,
        ["GRP", "SEQ", "VAL", "PREV"],
    )

    result = execute_specification(specification, visits_source())

    assert isinstance(result, ExecutionSuccess)
    assert result.artifact.frame.rows() == [
        ("a", 1, 1.0, None),
        ("a", 2, 2.0, 1.0),
        ("a", 3, 3.0, 2.0),
        ("b", 1, 10.0, None),
        ("b", 2, 20.0, 10.0),
    ]


def test_row_window_on_another_window_result_fails() -> None:
    # REQ-0326: windows evaluate in one pass with no declared order, so a
    # window must not depend on another window's result, even directly.
    specification = make_spec(
        [
            lag_template(
                {"group_by": ["GRP"], "order_by": ["SEQ"]},
                extra_derivations={
                    "PREV2": derive(
                        lag_window(
                            {"group_by": ["GRP"], "order_by": ["SEQ"]},
                            source="PREV",
                        )
                    )
                },
            )
        ],
        COLUMNS + [("PREV2", "float")],
        ["GRP", "SEQ", "VAL", "PREV", "PREV2"],
    )

    result = execute_specification(specification, visits_source())

    assert isinstance(result, ExecutionFailure)
    diagnostic = result.diagnostics[0]
    assert diagnostic.condition == "window_on_window_result"
    assert diagnostic.requirement == "REQ-0326"


def test_row_window_self_reference_is_a_cycle() -> None:
    row = Row(
        id="visits",
        derivations={
            "GRP": derive({"source": "SRC.G"}),
            "SEQ": derive({"source": "SRC.S"}),
            "VAL": derive({"source": "SRC.X"}),
            "PREV": derive(
                lag_window({"group_by": ["GRP"], "order_by": ["SEQ"]}, source="PREV")
            ),
        },
    )
    specification = make_spec([row], COLUMNS, ["GRP", "SEQ", "VAL", "PREV"])

    result = execute_specification(specification, visits_source())

    assert isinstance(result, ExecutionFailure)
    assert any(d.condition == "dependency_cycle" for d in result.diagnostics)


def test_scalar_derivation_may_consume_a_row_window_result() -> None:
    specification = make_spec(
        [
            lag_template(
                {"group_by": ["GRP"], "order_by": ["SEQ"]},
                extra_derivations={
                    "DOUBLED": derive({"compute": {"expr": "PREV * 2"}}),
                },
            )
        ],
        COLUMNS + [("DOUBLED", "float")],
        ["GRP", "SEQ", "VAL", "PREV", "DOUBLED"],
    )

    result = execute_specification(specification, visits_source())

    assert isinstance(result, ExecutionSuccess)
    assert result.artifact.frame.rows() == [
        ("a", 1, 1.0, None, None),
        ("a", 2, 2.0, 1.0, 2.0),
        ("a", 3, 3.0, 2.0, 4.0),
        ("b", 1, 10.0, None, None),
        ("b", 2, 20.0, 10.0, 20.0),
    ]


def test_two_hop_scalar_chain_after_window_pass() -> None:
    # DOUBLED reads CHG which reads the window result PREV. The deferred
    # fixpoint must catch both hops even though DOUBLED is declared first.
    specification = make_spec(
        [
            lag_template(
                {"group_by": ["GRP"], "order_by": ["SEQ"]},
                extra_derivations={
                    "DOUBLED": derive({"compute": {"expr": "CHG * 2"}}),
                    "CHG": derive({"compute": {"expr": "VAL - PREV"}}),
                },
            )
        ],
        COLUMNS + [("CHG", "float"), ("DOUBLED", "float")],
        ["GRP", "SEQ", "VAL", "PREV", "CHG", "DOUBLED"],
    )

    result = execute_specification(specification, visits_source())

    assert isinstance(result, ExecutionSuccess)
    assert result.artifact.frame.rows() == [
        ("a", 1, 1.0, None, None, None),
        ("a", 2, 2.0, 1.0, 1.0, 2.0),
        ("a", 3, 3.0, 2.0, 1.0, 2.0),
        ("b", 1, 10.0, None, None, None),
        ("b", 2, 20.0, 10.0, 10.0, 20.0),
    ]


def test_row_window_on_window_derived_value_fails() -> None:
    specification = make_spec(
        [
            lag_template(
                {"group_by": ["GRP"], "order_by": ["SEQ"]},
                extra_derivations={
                    "DOUBLED": derive({"compute": {"expr": "PREV * 2"}}),
                    "BAD": derive(
                        lag_window(
                            {"group_by": ["GRP"], "order_by": ["SEQ"]},
                            source="DOUBLED",
                        )
                    ),
                },
            )
        ],
        COLUMNS + [("DOUBLED", "float"), ("BAD", "float")],
        ["GRP", "SEQ", "VAL", "PREV", "DOUBLED", "BAD"],
    )

    result = execute_specification(specification, visits_source())

    assert isinstance(result, ExecutionFailure)
    diagnostic = result.diagnostics[0]
    assert diagnostic.condition == "window_on_window_result"
    assert diagnostic.requirement == "REQ-0326"


def test_row_window_cannot_read_a_column_another_template_derives() -> None:
    # OTHER is a column-level derivation, so no row template must derive
    # it. The second template's window reads OTHER anyway: the reference
    # is outside the template's row scope, so planning rejects it at the
    # phase boundary.
    first = lag_template({"group_by": ["GRP"], "order_by": ["SEQ"]})
    second = Row(
        id="second",
        derivations={
            "GRP": derive({"source": "SRC.G"}),
            "SEQ": derive({"source": "SRC.S"}),
            "VAL": derive({"source": "SRC.X"}),
            "PREV": derive(
                lag_window({"group_by": ["GRP"], "order_by": ["SEQ"]}, source="OTHER")
            ),
        },
    )
    specification = make_spec(
        [first, second],
        COLUMNS + [("OTHER", "str")],
        ["GRP", "SEQ", "VAL", "PREV"],
        column_derivations={"OTHER": {"literal": "x"}},
    )

    result = execute_specification(specification, visits_source())

    assert isinstance(result, ExecutionFailure)
    assert any(d.condition == "phase_boundary" for d in result.diagnostics)


def test_row_window_still_requires_order_by() -> None:
    specification = make_spec(
        [lag_template({"group_by": ["GRP"]})],
        COLUMNS,
        ["GRP", "SEQ", "VAL", "PREV"],
    )

    result = execute_specification(specification, visits_source())

    assert isinstance(result, ExecutionFailure)
    diagnostic = result.diagnostics[0]
    assert diagnostic.condition == "window_order_by_required"
    assert diagnostic.requirement == "REQ-0340"


def test_row_window_still_rejects_zero_offset() -> None:
    row = Row(
        id="visits",
        derivations={
            "GRP": derive({"source": "SRC.G"}),
            "SEQ": derive({"source": "SRC.S"}),
            "VAL": derive({"source": "SRC.X"}),
            "PREV": derive(
                lag_window({"group_by": ["GRP"], "order_by": ["SEQ"]}, offset=0)
            ),
        },
    )
    specification = make_spec([row], COLUMNS, ["GRP", "SEQ", "VAL", "PREV"])

    result = execute_specification(specification, visits_source())

    assert isinstance(result, ExecutionFailure)
    diagnostic = result.diagnostics[0]
    assert diagnostic.condition == "zero_offset"
    assert diagnostic.requirement == "REQ-0328"


@pytest.mark.parametrize(
    ("operation", "payload", "column_type", "expected"),
    [
        (
            "row_number",
            {"window": {"group_by": ["GRP"], "order_by": ["VAL"]}},
            "int",
            [1, 2, 3, 1, 2],
        ),
        (
            "rank",
            {"window": {"group_by": ["GRP"], "order_by": ["VAL"]}},
            "int",
            [1, 2, 3, 1, 2],
        ),
        (
            "previous_non_missing",
            {
                "source": "VAL",
                "window": {"group_by": ["GRP"], "order_by": ["SEQ"]},
            },
            "float",
            [None, 1.0, 2.0, None, 10.0],
        ),
    ],
)
def test_window_operations_evaluate_during_row_construction(
    operation: str, payload: dict, column_type: str, expected: list
) -> None:
    row = Row(
        id="visits",
        derivations={
            "GRP": derive({"source": "SRC.G"}),
            "SEQ": derive({"source": "SRC.S"}),
            "VAL": derive({"source": "SRC.X"}),
            "COMPUTED": derive({operation: payload}),
        },
    )
    specification = make_spec(
        [row],
        [("GRP", "str"), ("SEQ", "int"), ("VAL", "float"), ("COMPUTED", column_type)],
        ["GRP", "SEQ", "VAL", "COMPUTED"],
    )

    result = execute_specification(specification, visits_source())

    assert isinstance(result, ExecutionSuccess)
    assert [row[3] for row in result.artifact.frame.rows()] == expected
