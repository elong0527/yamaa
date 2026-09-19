from __future__ import annotations

import pytest

from yamaa.expressions import DEFAULT_EXPRESSION_OPERATIONS
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
    Lookup,
    Output,
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
        input={"SRC": DatasetSource(path="input/source.csv")},
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
    assert diagnostic.spec_paths == ("columns.A.derivation.source",)
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
    assert diagnostic.spec_paths == (
        "columns.A.derivation.source",
        "columns.B.derivation.source",
    )
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


def test_a_grouped_template_carries_the_grain_it_partitions_on() -> None:
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

    plan = plan_execution(spec, {"SRC": source_table()})

    assert plan.rows[0].grouped
    assert plan.rows[0].group_variables == ("SRC.X",)
    assert plan.rows[0].group_fields == ("X",)


def test_a_grouped_row_derivation_cannot_read_a_field_outside_the_grain() -> None:
    # R001-36: a driver field that varies within the group has no single
    # value for the candidate, so it is read through an aggregate or not at
    # all.
    table = frame_from_values(
        (TypedColumn(name="X", type="str"), TypedColumn(name="Y", type="str")),
        [["one", "left"], ["one", "right"]],
    )
    spec = specification(
        [Column(name="A", type="str"), Column(name="B", type="str")],
        [
            Row(
                id="row",
                group_by=["SRC.X"],
                derivations={
                    "A": derivation({"source": "SRC.X"}),
                    "B": derivation({"source": "SRC.Y"}),
                },
            )
        ],
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(spec, {"SRC": table})

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.condition == "ungrouped_driver_field"
    assert diagnostic.requirement == "R001-36"
    assert diagnostic.context["identifier"] == "SRC.Y"


def test_a_grouped_filter_reads_the_columns_that_template_derives() -> None:
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

    plan = plan_execution(spec, {"SRC": source_table()})

    assert plan.rows[0].filter_path == "rows[0].filter"
    assert plan.rows[0].filter_predicate is not None


def test_a_grouped_filter_naming_a_qualified_variable_is_rejected() -> None:
    # R001-37: a grouped filter runs after the derivation graph, over the
    # candidate's completed unqualified columns.
    spec = specification(
        [Column(name="A", type="str")],
        [
            Row(
                id="row",
                group_by=["SRC.X"],
                filter="SRC.X = 'one'",
                derivations={"A": derivation({"source": "SRC.X"})},
            )
        ],
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(spec, {"SRC": source_table()})

    assert raised.value.diagnostics[0].condition == "phase_boundary"


def test_a_lookup_contributes_its_match_values_as_dependencies() -> None:
    spec = specification(
        [
            Column(name="A", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(name="B", type="str", derivation=derivation({"source": "LOOK.X"})),
        ]
    ).model_copy(
        update={
            "lookups": [
                Lookup(id="LOOK", dataset="SRC", source=["A"], key=["X"], strict=True)
            ]
        }
    )

    plan = plan_execution(spec, {"SRC": source_table()})

    assert plan.lookups[0].match_variables == ("A",)
    assert plan.lookups[0].match_fields == ("X",)
    # A declared source and key with strict: true makes an unmatched key fatal.
    assert plan.lookups[0].strict is True
    assert dict.fromkeys(plan.columns[1].dependencies) == {"A": None}


def test_a_lookup_defaults_to_missing_on_absence() -> None:
    spec = specification(
        [Column(name="X", type="str", derivation=derivation({"source": "SRC.X"}))]
    ).model_copy(
        update={"lookups": [Lookup(id="LOOK", dataset="SRC", source=["X"], key=["X"])]}
    )

    plan = plan_execution(spec, {"SRC": source_table()})

    # Absence defaults to missing: strict is false and no missing literal.
    assert plan.lookups[0].strict is False
    assert plan.lookups[0].missing is None


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


def two_dataset_specification(columns: list[Column]) -> Specification:
    return Specification(
        schema_version="1.0",
        domain="OUT",
        input={
            "SRC": DatasetSource(path="input/source.csv"),
            "RIGHT": DatasetSource(path="input/right.csv"),
        },
        base="SRC",
        keys=[columns[0].name],
        output=Output(path="out.csv", columns=[column.name for column in columns]),
        columns=columns,
    )


def right_table(column_type: str = "str") -> object:
    return frame_from_values(
        (
            TypedColumn(name="X", type=column_type),
            TypedColumn(name="V", type="float"),
        ),
        [["one", 1.0]] if column_type == "str" else [[1, 1.0]],
    )


def plan_two(columns: list[Column], right: str = "str"):
    return plan_execution(
        two_dataset_specification(columns),
        {"SRC": source_table(), "RIGHT": right_table(right)},
        supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
    )


def first_diagnostic(columns: list[Column], right: str = "str"):
    with pytest.raises(ExecutionPlanningError) as raised:
        plan_two(columns, right)
    return raised.value.diagnostics[0]


def test_an_unwrapped_cross_dataset_source_is_rejected() -> None:
    diagnostic = first_diagnostic(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(
                name="V", type="float", derivation=derivation({"source": "RIGHT.V"})
            ),
        ]
    )

    # R003: a cross-dataset scalar source with no lookup is rejected; the
    # author must wrap it in an explicit lookup: with source/key pairs.
    assert diagnostic.condition == "unknown_lookup"


def test_a_lookup_key_typed_differently_on_each_side_is_reported() -> None:
    # R007-19 performs no implicit conversion, so a type-mismatched
    # explicit lookup key is reported.
    diagnostic = first_diagnostic(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(
                name="V",
                type="float",
                derivation=derivation(
                    {
                        "lookup": {
                            "dataset": "RIGHT",
                            "source": ["X"],
                            "key": ["X"],
                            "value": "V",
                        }
                    }
                ),
            ),
        ],
        right="int",
    )

    assert diagnostic.condition == "incompatible_input_type"


def test_a_declared_key_pair_must_carry_one_comparable_type() -> None:
    diagnostic = first_diagnostic(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(
                name="V",
                type="float",
                derivation=derivation(
                    {
                        "lookup": {
                            "dataset": "RIGHT",
                            "source": ["X"],
                            "key": ["X"],
                            "value": "V",
                        }
                    }
                ),
            ),
        ],
        right="int",
    )

    assert diagnostic.condition == "incompatible_input_type"
    assert diagnostic.requirement == "R007-21"


def aggregate_column(payload: dict[str, object]) -> Column:
    return Column(name="V", type="float", derivation=derivation({"aggregate": payload}))


def aggregate_diagnostic(payload: dict[str, object]):
    return first_diagnostic(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            aggregate_column(payload),
        ]
    )


def test_an_expression_naming_two_relations_is_not_a_join() -> None:
    diagnostic = aggregate_diagnostic({"expr": "SUM(RIGHT.V) + SUM(SRC.X)"})

    assert diagnostic.condition == "mixed_relations"
    assert diagnostic.requirement == "R013-39"
    assert diagnostic.context["relations"] == ["RIGHT", "SRC"]


def test_mixing_a_qualified_identifier_with_an_unqualified_one_fails() -> None:
    diagnostic = aggregate_diagnostic({"expr": "SUM(RIGHT.V) + X"})

    assert diagnostic.condition == "mixed_relations"


def test_an_identifier_outside_a_reduction_must_be_grouped_on() -> None:
    # R013-20: a value that varies within the group gives the expression no
    # single answer.
    diagnostic = aggregate_diagnostic({"expr": "SUM(RIGHT.V) / RIGHT.X"})

    assert diagnostic.condition == "aggregate_identifier_not_grouped"
    assert diagnostic.requirement == "R013-38"
    assert diagnostic.context["identifier"] == "RIGHT.X"


def test_an_output_row_reduction_must_declare_its_partition() -> None:
    diagnostic = aggregate_diagnostic({"expr": "MAX(X)"})

    assert diagnostic.condition == "invalid_aggregate_context"
    assert diagnostic.requirement == "R013-42"


def test_between_narrows_a_qualified_right_side_only() -> None:
    diagnostic = aggregate_diagnostic(
        {
            "group_by": ["X"],
            "expr": "MAX(X)",
            "between": {"value": "X", "lower": "RIGHT.V", "upper": "RIGHT.V"},
        }
    )

    assert diagnostic.condition == "invalid_aggregate_context"
    assert diagnostic.requirement == "R007-44"


def test_an_aggregate_has_no_context_in_an_ungrouped_row_template() -> None:
    # R007-8 through R007-10 permit exactly three contexts, and a
    # record-driven template is none of them.
    spec = specification(
        [Column(name="A", type="float")],
        [
            Row(
                id="row",
                derivations={"A": derivation({"aggregate": {"expr": "SUM(SRC.X)"}})},
            )
        ],
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": source_table()},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    assert raised.value.diagnostics[0].condition == "invalid_aggregate_context"


def test_a_grouped_row_aggregate_declares_no_grain_of_its_own() -> None:
    spec = specification(
        [Column(name="A", type="float")],
        [
            Row(
                id="row",
                group_by=["SRC.X"],
                derivations={
                    "A": derivation(
                        {"aggregate": {"group_by": ["SRC.X"], "expr": "COUNT(SRC.*)"}}
                    )
                },
            )
        ],
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": source_table()},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    assert raised.value.diagnostics[0].condition == "invalid_aggregate_context"


def test_a_one_field_aggregate_names_the_shared_shorthand_operation() -> None:
    diagnostic = aggregate_diagnostic({"expr": "AVG(RIGHT.V)"})

    assert diagnostic.condition == "prohibited_function"
    assert diagnostic.spec_paths == ("columns.V.derivation.aggregate",)


def test_a_lookup_may_be_read_from_a_numeric_expression() -> None:
    # R003-15: R010 admits a qualified identifier for a record a lookup has
    # already selected, and R010-38 still rejects every other dataset.
    columns = [
        Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
        Column(
            name="V",
            type="float",
            derivation=derivation({"compute": {"expr": "2 * LOOK.V"}}),
        ),
    ]
    spec = two_dataset_specification(columns).model_copy(
        update={
            "lookups": [Lookup(id="LOOK", dataset="RIGHT", source=["X"], key=["X"])]
        }
    )

    plan = plan_execution(
        spec,
        {"SRC": source_table(), "RIGHT": right_table()},
        supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
    )

    assert plan.columns[1].dependencies == ("X",)
    assert (
        first_diagnostic(
            [
                Column(
                    name="X", type="str", derivation=derivation({"source": "SRC.X"})
                ),
                Column(
                    name="V",
                    type="float",
                    derivation=derivation({"compute": {"expr": "2 * RIGHT.V"}}),
                ),
            ]
        ).condition
        == "qualified_identifier"
    )


def filter_diagnostics(spec: Specification) -> list[object]:
    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": source_table()},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )
    return list(raised.value.diagnostics)


def test_a_source_filter_reads_the_stored_fields_of_its_own_right_side() -> None:
    # R003-22: the predicate selects among right-side records, so it names
    # their fields and nothing the output carries.
    spec = specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(
                name="A",
                type="str",
                derivation=derivation(
                    {"source": {"variable": "SRC.X", "filter": "K = 'one'"}}
                ),
            ),
        ]
    )

    diagnostic = filter_diagnostics(spec)[0]

    assert diagnostic.condition == "unknown_field"
    assert diagnostic.spec_paths == ("columns.A.derivation.source.filter",)
    assert diagnostic.requirement == "R003-22"


def test_a_source_filter_names_a_field_the_dataset_carries() -> None:
    spec = specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(
                name="A",
                type="str",
                derivation=derivation(
                    {"source": {"variable": "SRC.X", "filter": "SRC.Y = 'one'"}}
                ),
            ),
        ]
    )

    diagnostic = filter_diagnostics(spec)[0]

    assert diagnostic.condition == "unknown_field"
    assert diagnostic.spec_paths == ("columns.A.derivation.source.filter",)


def test_an_output_column_source_has_no_records_to_filter() -> None:
    # R003-38: the source reads one completed value, not a right side.
    spec = specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(
                name="A",
                type="str",
                derivation=derivation(
                    {"source": {"variable": "K", "filter": "SRC.X = 'one'"}}
                ),
            ),
        ]
    )

    diagnostic = filter_diagnostics(spec)[0]

    assert diagnostic.condition == "prohibited_construct"
    assert diagnostic.spec_paths == ("columns.A.derivation.source.filter",)
    assert diagnostic.requirement == "R003-38"


def test_a_grouped_row_template_source_has_no_records_to_filter() -> None:
    spec = specification(
        [Column(name="K", type="str")],
        [
            Row(
                id="row",
                group_by=["SRC.X"],
                derivations={
                    "K": derivation(
                        {"source": {"variable": "SRC.X", "filter": "SRC.X = 'one'"}}
                    )
                },
            )
        ],
    )

    diagnostic = filter_diagnostics(spec)[0]

    assert diagnostic.condition == "prohibited_construct"
    assert diagnostic.requirement == "R003-38"


def test_a_lookup_source_has_already_chosen_its_record() -> None:
    spec = specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(
                name="A",
                type="str",
                derivation=derivation(
                    {"source": {"variable": "REF.X", "filter": "REF.X = 'one'"}}
                ),
            ),
        ]
    ).model_copy(
        update={"lookups": [Lookup(id="REF", dataset="SRC", source=["K"], key=["X"])]}
    )

    diagnostic = filter_diagnostics(spec)[0]

    assert diagnostic.condition == "prohibited_construct"
    assert diagnostic.requirement == "R003-38"


def test_a_filtered_source_reads_records_without_depending_on_their_keys() -> None:
    spec = specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(
                name="A",
                type="str",
                derivation=derivation(
                    {"source": {"variable": "SRC.X", "filter": "SRC.X = 'one'"}}
                ),
            ),
        ]
    )

    plan = plan_execution(
        spec,
        {"SRC": source_table()},
        supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
    )

    derived = next(column for column in plan.columns if column.column == "A")
    assert derived.dependencies == ()
