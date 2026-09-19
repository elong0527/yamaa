from __future__ import annotations

import pytest

from yamaa.expressions import DEFAULT_EXPRESSION_OPERATIONS
from yamaa.io.polars import frame_from_values
from yamaa.models import TypedColumn
from yamaa.planning import (
    ExecutionPlanningError,
    ImplicitJoin,
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
                Lookup(
                    id="LOOK", dataset="SRC", key_source=["A"], key=["X"], strict=True
                )
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
    ).model_copy(update={"lookups": [Lookup(id="LOOK", dataset="SRC", key=["X"])]})

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


def test_a_named_lookup_with_an_omitted_key_infers_the_applicable_keys() -> None:
    spec = two_dataset_specification(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(name="V", type="float", derivation=derivation({"source": "LOOK.V"})),
        ]
    ).model_copy(update={"lookups": [Lookup(id="LOOK", dataset="RIGHT")]})

    plan = plan_execution(
        spec,
        {"SRC": source_table(), "RIGHT": right_table()},
        supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
    )

    # R003-43: the omitted key is the applicable output keys; R003-44: the
    # omitted source defaults to the key names.
    assert plan.lookups[0].match_variables == ("X",)
    assert plan.lookups[0].match_fields == ("X",)


def test_a_named_lookup_with_an_omitted_source_defaults_to_the_key_names() -> None:
    spec = two_dataset_specification(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(name="V", type="float", derivation=derivation({"source": "LOOK.V"})),
        ]
    ).model_copy(update={"lookups": [Lookup(id="LOOK", dataset="RIGHT", key=["X"])]})

    plan = plan_execution(
        spec,
        {"SRC": source_table(), "RIGHT": right_table()},
        supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
    )

    # R003-44: the omitted source defaults to the declared key names.
    assert plan.lookups[0].match_variables == ("X",)
    assert plan.lookups[0].match_fields == ("X",)


def test_a_named_lookup_with_an_omitted_key_and_no_applicable_key_fails() -> None:
    right = frame_from_values((TypedColumn(name="V", type="float"),), [[1.0]])
    spec = two_dataset_specification(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(name="V", type="float", derivation=derivation({"source": "LOOK.V"})),
        ]
    ).model_copy(update={"lookups": [Lookup(id="LOOK", dataset="RIGHT")]})

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": source_table(), "RIGHT": right},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    # R003-43: no output key exists on RIGHT, so the omitted key cannot be
    # inferred and the author must declare it.
    [diagnostic] = [
        d for d in raised.value.diagnostics if d.condition == "no_applicable_keys"
    ]
    assert diagnostic.requirement == "R003-43"
    assert diagnostic.spec_paths == ("lookups[0]",)


def test_a_named_lookup_with_mismatched_source_and_key_lengths_fails() -> None:
    spec = two_dataset_specification(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(name="V", type="float", derivation=derivation({"source": "LOOK.V"})),
        ]
    ).model_copy(
        update={
            "lookups": [
                Lookup(id="LOOK", dataset="RIGHT", key_source=["X", "X"], key=["X"])
            ]
        }
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": source_table(), "RIGHT": right_table()},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    # R003-5: explicit pairs must pair by position after inference.
    [diagnostic] = [
        d
        for d in raised.value.diagnostics
        if d.condition == "source_key_length_mismatch"
    ]
    assert diagnostic.requirement == "R003-5"
    assert diagnostic.spec_paths == ("lookups[0]",)


def test_an_inline_lookup_with_an_omitted_key_infers_the_applicable_keys() -> None:
    plan = plan_two(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(
                name="V",
                type="float",
                derivation=derivation({"lookup": {"dataset": "RIGHT", "value": "V"}}),
            ),
        ]
    )

    # R003-43/R003-44: the inline lookup omits both lists. The inferred
    # source becomes a dependency of the column.
    [derived] = [column for column in plan.columns if column.column == "V"]
    assert "X" in derived.dependencies


def test_a_qualified_aggregate_with_an_omitted_key_infers_the_applicable_keys() -> None:
    plan = plan_two(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            aggregate_column({"expr": "SUM(RIGHT.V)"}),
        ]
    )

    # R003-43/R003-44: the aggregate omits both lists and groups on X.
    [join] = [
        join
        for join in plan.resolved_joins
        if join.spec_path == "columns.V.derivation.aggregate.expr"
    ]
    assert join.source == ("X",)
    assert join.key == ("X",)
    assert join.inferred is True


def test_an_inferred_lookup_key_typed_differently_on_each_side_is_reported() -> None:
    diagnostic = first_diagnostic(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(
                name="V",
                type="float",
                derivation=derivation({"lookup": {"dataset": "RIGHT", "value": "V"}}),
            ),
        ],
        right="int",
    )

    # R003-41: an inferred key must compare equal on both sides.
    assert diagnostic.condition == "incompatible_input_type"


def test_a_cross_dataset_source_with_clear_keys_uses_the_implicit_join() -> None:
    plan = plan_two(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(
                name="V", type="float", derivation=derivation({"source": "RIGHT.V"})
            ),
        ]
    )

    # R003-40: the output key X exists on RIGHT, so the read joins on it.
    [derived] = [column for column in plan.columns if column.column == "V"]
    assert derived.implicit_joins == (ImplicitJoin(dataset="RIGHT", keys=("X",)),)
    assert "X" in derived.dependencies
    [resolved] = [
        join
        for join in plan.resolved_joins
        if join.spec_path == "columns.V.derivation.source"
    ]
    assert resolved.inferred is True
    assert resolved.source == ("X",)
    assert resolved.key == ("X",)


def test_a_cross_dataset_source_without_applicable_keys_requires_a_lookup() -> None:
    right = frame_from_values((TypedColumn(name="V", type="float"),), [[1.0]])
    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            two_dataset_specification(
                [
                    Column(
                        name="X", type="str", derivation=derivation({"source": "SRC.X"})
                    ),
                    Column(
                        name="V",
                        type="float",
                        derivation=derivation({"source": "RIGHT.V"}),
                    ),
                ]
            ),
            {"SRC": source_table(), "RIGHT": right},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    # R003-42: no output key exists on RIGHT, so the intended match is
    # unclear and the author must declare it with an explicit `lookup:`.
    [diagnostic] = raised.value.diagnostics
    assert diagnostic.condition == "no_applicable_keys"
    assert diagnostic.requirement == "R003-42"
    assert diagnostic.spec_paths == ("columns.V.derivation.source",)


def test_an_implicit_join_key_typed_differently_on_each_side_is_reported() -> None:
    diagnostic = first_diagnostic(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(
                name="V", type="float", derivation=derivation({"source": "RIGHT.V"})
            ),
        ],
        right="int",
    )

    # R003-41: an inferred key must compare equal on both sides.
    assert diagnostic.condition == "incompatible_input_type"
    assert diagnostic.requirement == "R003-41"


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
                            "key_source": ["X"],
                            "key": ["V"],
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
                            "key_source": ["X"],
                            "key": ["V"],
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
        update={"lookups": [Lookup(id="LOOK", dataset="RIGHT", key=["X"])]}
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
        update={
            "lookups": [Lookup(id="REF", dataset="SRC", key_source=["K"], key=["X"])]
        }
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
