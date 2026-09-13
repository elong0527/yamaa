from __future__ import annotations

import pytest

from yamaa.io.polars import frame_from_values
from yamaa.models import TypedColumn
from yamaa.planning import (
    ExecutionPlanningError,
    UnsupportedPlanningError,
    plan_execution,
)
from yamaa.specification.models import (
    Column,
    DatasetSource,
    Expression,
    HandledExpression,
    Output,
    OverrideRule,
    RecordLookup,
    Row,
    Specification,
)


def derivation(expression: dict[str, object]) -> HandledExpression:
    return HandledExpression(value=Expression(root=expression))


def source_table() -> object:
    return frame_from_values(
        (TypedColumn(name="X", type="str"),),
        [["one"], ["two"]],
    )


def specification(
    columns: list[Column], rows: list[Row] | None = None
) -> Specification:
    return Specification(
        schema_version="1.0",
        domain="OUT",
        datasets={"SRC": DatasetSource(path="input/source.csv")},
        base="SRC",
        keys=[columns[0].name],
        output=Output(path="out.csv", columns=[column.name for column in columns]),
        columns=columns,
        rows=rows,
    )


def test_a_key_column_must_not_depend_on_a_non_key_column_without_rows() -> None:
    spec = specification(
        [
            Column(name="B", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(name="K", type="str", derivation=derivation({"source": "B"})),
        ]
    ).model_copy(update={"keys": ["K"]})

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(spec, {"SRC": source_table()})

    (diagnostic,) = raised.value.diagnostics
    assert diagnostic.condition == "key_dependency"
    assert diagnostic.requirement == "R001-43"
    assert diagnostic.context == {"column": "K", "dependency": "B"}


def test_a_later_column_reference_is_not_silently_sorted() -> None:
    spec = specification(
        [
            Column(name="A", type="str", derivation=derivation({"source": "B"})),
            Column(name="B", type="str", derivation=derivation({"source": "SRC.X"})),
        ]
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(spec, {"SRC": source_table()})

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.condition == "dependency_order"
    assert diagnostic.context == {"column": "A", "dependency": "B"}


def test_a_column_cycle_is_reported_as_a_cycle_not_an_ordering_repair() -> None:
    spec = specification(
        [
            Column(name="A", type="str", derivation=derivation({"source": "B"})),
            Column(name="B", type="str", derivation=derivation({"source": "A"})),
        ]
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(spec, {"SRC": source_table()})

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.condition == "dependency_cycle"
    assert diagnostic.context == {"cycle": ["A", "B", "A"]}


def test_a_row_derivation_cannot_read_a_column_phase_value() -> None:
    spec = specification(
        [
            Column(name="A", type="str"),
            Column(name="B", type="str", derivation=derivation({"source": "SRC.X"})),
        ],
        [
            Row(
                id="row",
                derivations={"A": derivation({"source": "B"})},
            )
        ],
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(spec, {"SRC": source_table()})

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.condition == "phase_boundary"
    assert diagnostic.context["identifier"] == "B"
    assert diagnostic.context["required_phase"] == "row_construction"


def test_an_undeclared_row_driver_fails_planning() -> None:
    spec = specification(
        [Column(name="A", type="str")],
        [
            Row(
                id="row",
                dataset="ABSENT",
                derivations={"A": derivation({"literal": "value"})},
            )
        ],
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(spec, {"SRC": source_table()})

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.condition == "driver_unavailable"
    assert diagnostic.context == {"row": "row", "dataset": "ABSENT"}


def test_row_derivations_follow_their_graph_not_yaml_mapping_order() -> None:
    spec = specification(
        [Column(name="A", type="str"), Column(name="B", type="str")],
        [
            Row(
                id="row",
                derivations={
                    "A": derivation({"source": "B"}),
                    "B": derivation({"source": "SRC.X"}),
                },
            )
        ],
    )

    plan = plan_execution(spec, {"SRC": source_table()})

    assert [item.column for item in plan.rows[0].derivations] == ["B", "A"]


def test_grouped_rows_are_explicitly_unsupported() -> None:
    spec = specification(
        [Column(name="A", type="str")],
        [
            Row(
                id="row",
                group_by=["SRC.X"],
                derivations={"A": derivation({"source": "SRC.X"})},
            )
        ],
    )

    with pytest.raises(UnsupportedPlanningError) as raised:
        plan_execution(spec, {"SRC": source_table()})

    assert [feature.model_dump() for feature in raised.value.features] == [
        {"operation": "grouped_rows", "spec_path": "rows[0].group_by"}
    ]


def test_a_grouped_filter_is_not_misread_as_a_record_driven_filter() -> None:
    spec = specification(
        [Column(name="A", type="str")],
        [
            Row(
                id="row",
                group_by=["SRC.X"],
                filter="A = 'one'",
                derivations={"A": derivation({"source": "SRC.X"})},
            )
        ],
    )

    with pytest.raises(UnsupportedPlanningError) as raised:
        plan_execution(spec, {"SRC": source_table()})

    assert raised.value.features[0].operation == "grouped_rows"


def test_record_lookup_references_are_deferred_with_the_lookup() -> None:
    spec = specification(
        [Column(name="A", type="str", derivation=derivation({"source": "LOOK.X"}))]
    ).model_copy(update={"record_lookups": [RecordLookup(id="LOOK", dataset="SRC")]})

    with pytest.raises(UnsupportedPlanningError) as raised:
        plan_execution(spec, {"SRC": source_table()})

    assert raised.value.features[0].operation == "record_lookup"


def test_an_unimplemented_expression_is_not_a_semantic_failure() -> None:
    spec = specification(
        [
            Column(
                name="A",
                type="str",
                derivation=derivation({"str_upper": {"source": "SRC.X"}}),
            )
        ]
    )

    with pytest.raises(UnsupportedPlanningError) as raised:
        plan_execution(spec, {"SRC": source_table()})

    assert raised.value.features[0].operation == "str_upper"


def test_an_override_can_read_the_converted_value_being_derived() -> None:
    spec = specification(
        [
            Column(
                name="A",
                type="str",
                derivation=HandledExpression(
                    value=Expression(root={"source": "SRC.X"}),
                    override=[
                        OverrideRule(
                            when="A = 'one'",
                            value=Expression(root={"source": "A"}),
                        )
                    ],
                ),
            )
        ]
    )

    plan = plan_execution(spec, {"SRC": source_table()})

    assert plan.columns[0].dependencies == ()
