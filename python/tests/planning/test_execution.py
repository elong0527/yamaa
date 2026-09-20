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
    Intermediate,
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
    assert diagnostic.requirement == "REQ-0074"
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
    assert diagnostic.condition == "forward_reference"
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
    # REQ-0067: a driver field that varies within the group has no single
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
    assert diagnostic.requirement == "REQ-0067"
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
    # REQ-0068: a grouped filter runs after the derivation graph, over the
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
            "intermediates": [
                Intermediate(
                    id="LOOK", dataset="SRC", key_base=["A"], key=["X"], strict=True
                )
            ]
        }
    )

    plan = plan_execution(spec, {"SRC": source_table()})

    assert plan.intermediates[0].match_variables == ("A",)
    assert plan.intermediates[0].match_fields == ("X",)
    # A declared source and key with strict: true makes an unmatched key fatal.
    assert plan.intermediates[0].strict is True
    assert dict.fromkeys(plan.columns[1].dependencies) == {"A": None}


def test_a_lookup_defaults_to_missing_on_absence() -> None:
    spec = specification(
        [Column(name="X", type="str", derivation=derivation({"source": "SRC.X"}))]
    ).model_copy(
        update={"intermediates": [Intermediate(id="LOOK", dataset="SRC", key=["X"])]}
    )

    plan = plan_execution(spec, {"SRC": source_table()})

    # Absence defaults to missing: strict is false and no missing literal.
    assert plan.intermediates[0].strict is False
    assert plan.intermediates[0].missing is None


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
    ).model_copy(update={"intermediates": [Intermediate(id="LOOK", dataset="RIGHT")]})

    plan = plan_execution(
        spec,
        {"SRC": source_table(), "RIGHT": right_table()},
        supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
    )

    # REQ-0153: the omitted key is the applicable output keys; REQ-0154: the
    # omitted source defaults to the key names.
    assert plan.intermediates[0].match_variables == ("X",)
    assert plan.intermediates[0].match_fields == ("X",)


def test_a_named_lookup_with_an_omitted_source_defaults_to_the_key_names() -> None:
    spec = two_dataset_specification(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(name="V", type="float", derivation=derivation({"source": "LOOK.V"})),
        ]
    ).model_copy(
        update={"intermediates": [Intermediate(id="LOOK", dataset="RIGHT", key=["X"])]}
    )

    plan = plan_execution(
        spec,
        {"SRC": source_table(), "RIGHT": right_table()},
        supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
    )

    # REQ-0154: the omitted source defaults to the declared key names.
    assert plan.intermediates[0].match_variables == ("X",)
    assert plan.intermediates[0].match_fields == ("X",)


def test_a_named_lookup_with_an_omitted_key_and_no_applicable_key_fails() -> None:
    right = frame_from_values((TypedColumn(name="V", type="float"),), [[1.0]])
    spec = two_dataset_specification(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(name="V", type="float", derivation=derivation({"source": "LOOK.V"})),
        ]
    ).model_copy(update={"intermediates": [Intermediate(id="LOOK", dataset="RIGHT")]})

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": source_table(), "RIGHT": right},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    # REQ-0153: no output key exists on RIGHT, so the omitted key cannot be
    # inferred and the author must declare it.
    [diagnostic] = [
        d for d in raised.value.diagnostics if d.condition == "no_applicable_keys"
    ]
    assert diagnostic.requirement == "REQ-0153"
    assert diagnostic.spec_paths == ("intermediates[0]",)


def test_a_named_lookup_with_mismatched_source_and_key_lengths_fails() -> None:
    spec = two_dataset_specification(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(name="V", type="float", derivation=derivation({"source": "LOOK.V"})),
        ]
    ).model_copy(
        update={
            "intermediates": [
                Intermediate(id="LOOK", dataset="RIGHT", key_base=["X", "X"], key=["X"])
            ]
        }
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": source_table(), "RIGHT": right_table()},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    # REQ-0115: explicit pairs must pair by position after inference.
    [diagnostic] = [
        d
        for d in raised.value.diagnostics
        if d.condition == "source_key_length_mismatch"
    ]
    assert diagnostic.requirement == "REQ-0115"
    assert diagnostic.spec_paths == ("intermediates[0]",)


def test_a_named_lookup_pairing_a_key_base_against_an_inferred_key_fails() -> None:
    source = frame_from_values(
        (TypedColumn(name="X", type="str"), TypedColumn(name="Y", type="str")),
        [["one", "a"]],
    )
    spec = Specification(
        schema_version="1.0",
        domain="OUT",
        input={
            "SRC": DatasetSource(path="input/source.csv"),
            "RIGHT": DatasetSource(path="input/right.csv"),
        },
        base="SRC",
        keys=["X"],
        output=Output(path="out.csv", columns=["X", "Y", "V"]),
        columns=[
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(name="Y", type="str", derivation=derivation({"source": "SRC.Y"})),
            Column(name="V", type="float", derivation=derivation({"source": "LOOK.V"})),
        ],
    ).model_copy(
        update={
            "intermediates": [
                Intermediate(id="LOOK", dataset="RIGHT", key_base=["X", "Y"])
            ]
        }
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": source, "RIGHT": right_table()},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    # REQ-0115: the two declared key_base names pair with one inferred key, so
    # the pairing is reported rather than the intermediate silently vanishing
    # and its readers failing as unknown fields.
    [diagnostic] = [
        d
        for d in raised.value.diagnostics
        if d.condition == "source_key_length_mismatch"
    ]
    assert diagnostic.requirement == "REQ-0115"
    assert diagnostic.spec_paths == ("intermediates[0]",)
    assert diagnostic.context["key_base"] == ["X", "Y"]
    assert diagnostic.context["key"] == ["X"]


def test_an_inline_lookup_with_a_key_naming_no_identifiers_is_reported() -> None:
    diagnostic = first_diagnostic(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(
                name="V",
                type="float",
                derivation=derivation(
                    {"lookup": {"dataset": "RIGHT", "key": 5, "value": "V"}}
                ),
            ),
        ]
    )

    # REQ-0321: a written key that names no identifiers is not an omitted key,
    # so it is reported here instead of reaching the runtime unvalidated.
    assert diagnostic.condition == "invalid_field_type"
    assert diagnostic.requirement == "REQ-0321"
    assert diagnostic.spec_paths == ("columns.V.derivation.lookup",)


def test_an_aggregate_with_a_key_naming_no_identifiers_is_reported() -> None:
    diagnostic = first_diagnostic(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(
                name="V",
                type="float",
                derivation=derivation(
                    {"aggregate": {"expr": "max(RIGHT.V)", "key": 5}}
                ),
            ),
        ]
    )

    # REQ-0140: a written key that names no identifiers must not silently
    # reduce over the whole relation unkeyed.
    assert diagnostic.condition == "missing_aggregate_keys"
    assert diagnostic.requirement == "REQ-0140"


def test_an_inline_lookup_with_an_incomplete_between_is_reported() -> None:
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
                            "key": ["X"],
                            "value": "V",
                            "between": {"value": "X"},
                        }
                    }
                ),
            ),
        ]
    )

    # REQ-0321: the named form requires value, lower and upper together, so a
    # partial inline range is reported before it reaches the runtime.
    assert diagnostic.condition == "invalid_field_type"
    assert diagnostic.requirement == "REQ-0321"
    assert diagnostic.spec_paths == ("columns.V.derivation.lookup.between",)


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

    # REQ-0153/REQ-0154: the inline lookup omits both lists. The inferred
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

    # REQ-0153/REQ-0154: the aggregate omits both lists and groups on X.
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

    # REQ-0151: an inferred key must compare equal on both sides.
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

    # REQ-0150: the output key X exists on RIGHT, so the read joins on it.
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

    # REQ-0152: no output key exists on RIGHT, so the intended match is
    # unclear and the author must declare it with an explicit `lookup:`.
    [diagnostic] = raised.value.diagnostics
    assert diagnostic.condition == "no_applicable_keys"
    assert diagnostic.requirement == "REQ-0152"
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

    # REQ-0151: an inferred key must compare equal on both sides.
    assert diagnostic.condition == "incompatible_input_type"
    assert diagnostic.requirement == "REQ-0151"


def test_a_lookup_key_typed_differently_on_each_side_is_reported() -> None:
    # REQ-0004 performs no implicit conversion, so a type-mismatched
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
                            "key_base": ["X"],
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
                            "key_base": ["X"],
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
    assert diagnostic.requirement == "REQ-0305"


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
    assert diagnostic.requirement == "REQ-0504"
    assert diagnostic.context["relations"] == ["RIGHT", "SRC"]


def test_mixing_a_qualified_identifier_with_an_unqualified_one_fails() -> None:
    diagnostic = aggregate_diagnostic({"expr": "SUM(RIGHT.V) + X"})

    assert diagnostic.condition == "mixed_relations"


def test_an_identifier_outside_a_reduction_must_be_grouped_on() -> None:
    # REQ-0485: a value that varies within the group gives the expression no
    # single answer.
    diagnostic = aggregate_diagnostic({"expr": "SUM(RIGHT.V) / RIGHT.X"})

    assert diagnostic.condition == "aggregate_identifier_not_grouped"
    assert diagnostic.requirement == "REQ-0503"
    assert diagnostic.context["identifier"] == "RIGHT.X"


def test_a_grouped_identifier_beside_a_reduction_is_admitted() -> None:
    plan = plan_two(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            aggregate_column({"group_by": ["RIGHT.X"], "expr": "SUM(RIGHT.V)"}),
        ]
    )

    # REQ-0140: the join matches on the declared keys instead of the applicable keys.
    assert plan.columns[1].dependencies == ("X",)


def test_an_output_row_reduction_must_declare_its_partition() -> None:
    diagnostic = aggregate_diagnostic({"expr": "MAX(X)"})

    assert diagnostic.condition == "invalid_aggregate_context"
    assert diagnostic.requirement == "REQ-0507"


def test_between_narrows_a_qualified_right_side_only() -> None:
    diagnostic = aggregate_diagnostic(
        {
            "group_by": ["X"],
            "expr": "MAX(X)",
            "between": {"value": "X", "lower": "RIGHT.V", "upper": "RIGHT.V"},
        }
    )

    assert diagnostic.condition == "invalid_aggregate_context"
    assert diagnostic.requirement == "REQ-0329"


def test_an_aggregate_has_no_context_in_an_ungrouped_row_template() -> None:
    # REQ-0295 through REQ-0467 permit exactly three contexts, and a
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
    # REQ-0125: R010 admits a qualified identifier for a record a lookup has
    # already selected, and REQ-0442 still rejects every other dataset.
    columns = [
        Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
        Column(
            name="V",
            type="float",
            derivation=derivation({"compute": {"expr": "2 * LOOK.V"}}),
        ),
    ]
    spec = two_dataset_specification(columns).model_copy(
        update={"intermediates": [Intermediate(id="LOOK", dataset="RIGHT", key=["X"])]}
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
    # REQ-0132: the predicate selects among right-side records, so it names
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
    assert diagnostic.requirement == "REQ-0132"


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
    # REQ-0148: the source reads one completed value, not a right side.
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
    assert diagnostic.requirement == "REQ-0148"


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
    assert diagnostic.requirement == "REQ-0148"


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
            "intermediates": [
                Intermediate(id="REF", dataset="SRC", key_base=["K"], key=["X"])
            ]
        }
    )

    diagnostic = filter_diagnostics(spec)[0]

    assert diagnostic.condition == "prohibited_construct"
    assert diagnostic.requirement == "REQ-0148"


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


def row_two_dataset_specification(
    columns: list[Column], rows: list[Row]
) -> Specification:
    return Specification(
        schema_version="1.0",
        domain="OUT",
        input={
            "SRC": DatasetSource(path="input/source.csv"),
            "RIGHT": DatasetSource(path="input/right.csv"),
        },
        keys=["K"],
        output=Output(path="out.csv", columns=[column.name for column in columns]),
        columns=columns,
        rows=rows,
    )


def row_tables() -> dict[str, object]:
    return {
        "SRC": frame_from_values(
            (
                TypedColumn(name="K", type="str"),
                TypedColumn(name="W", type="float"),
            ),
            [["a", 1.0], ["b", 2.0]],
        ),
        "RIGHT": frame_from_values(
            (
                TypedColumn(name="K", type="str"),
                TypedColumn(name="V", type="float"),
            ),
            [["a", 10.0]],
        ),
    }


def test_a_row_source_to_another_dataset_joins_on_the_driver_record() -> None:
    spec = row_two_dataset_specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.K"})),
            Column(name="V", type="float"),
        ],
        [
            Row(
                id="r",
                dataset="SRC",
                derivations={"V": derivation({"source": "RIGHT.V"})},
            )
        ],
    )
    plan = plan_execution(spec, row_tables())

    # REQ-0156: the applicable key matches the driver record's field, so the
    # planned join states driver-qualified match variables.
    [row] = plan.rows
    [derived] = [item for item in row.derivations if item.column == "V"]
    assert derived.implicit_joins == (
        ImplicitJoin(dataset="RIGHT", keys=("K",), match_variables=("SRC.K",)),
    )
    assert "SRC.K" in derived.dependencies
    [resolved] = [
        join
        for join in plan.resolved_joins
        if join.spec_path == "rows[0].derivations.V.source"
    ]
    assert resolved.dataset == "RIGHT"
    assert resolved.source == ("SRC.K",)
    assert resolved.key == ("K",)
    assert resolved.inferred is True


def test_a_row_source_without_applicable_keys_reports_only_no_applicable_keys() -> None:
    tables = row_tables()
    tables["RIGHT"] = frame_from_values(
        (TypedColumn(name="V", type="float"),),
        [[10.0]],
    )
    spec = row_two_dataset_specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.K"})),
            Column(name="V", type="float"),
        ],
        [
            Row(
                id="r",
                dataset="SRC",
                derivations={"V": derivation({"source": "RIGHT.V"})},
            )
        ],
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(spec, tables)

    # REQ-0152 stands alone: no knock-on phase error follows it.
    [diagnostic] = raised.value.diagnostics
    assert diagnostic.condition == "no_applicable_keys"
    assert diagnostic.requirement == "REQ-0152"
    assert diagnostic.spec_paths == ("rows[0].derivations.V.source",)


def test_a_row_join_key_missing_from_the_driver_is_reported() -> None:
    tables = {
        "SRC": frame_from_values(
            (TypedColumn(name="W", type="float"),),
            [[1.0]],
        ),
        "RIGHT": row_tables()["RIGHT"],
    }
    spec = row_two_dataset_specification(
        [
            Column(name="K", type="str", derivation=derivation({"literal": "k"})),
            Column(name="V", type="float"),
        ],
        [
            Row(
                id="r",
                dataset="SRC",
                derivations={"V": derivation({"source": "RIGHT.V"})},
            )
        ],
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(spec, tables)

    [diagnostic] = raised.value.diagnostics
    assert diagnostic.condition == "unknown_field"
    assert diagnostic.requirement == "REQ-0103"
    assert diagnostic.context == {"identifier": "SRC.K"}


def test_a_grouped_row_join_matches_the_group_keys() -> None:
    spec = row_two_dataset_specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.K"})),
            Column(name="V", type="float"),
        ],
        [
            Row(
                id="g",
                dataset="SRC",
                group_by=["SRC.K"],
                derivations={"V": derivation({"source": "RIGHT.V"})},
            )
        ],
    )
    plan = plan_execution(spec, row_tables())

    # REQ-0157: the applicable key is a group key, so the join matches it.
    [row] = plan.rows
    [derived] = [item for item in row.derivations if item.column == "V"]
    assert derived.implicit_joins == (
        ImplicitJoin(dataset="RIGHT", keys=("K",), match_variables=("SRC.K",)),
    )


def test_a_grouped_row_join_needs_group_keys() -> None:
    spec = row_two_dataset_specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.K"})),
            Column(name="V", type="float"),
        ],
        [
            Row(
                id="g",
                dataset="SRC",
                group_by=["SRC.W"],
                derivations={"V": derivation({"source": "RIGHT.V"})},
            )
        ],
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(spec, row_tables())

    # REQ-0157: a key the group does not carry varies within it.
    # REQ-0107: the column-level key echo is not a group key either.
    assert [
        (diagnostic.condition, diagnostic.requirement)
        for diagnostic in raised.value.diagnostics
    ] == [
        ("ungrouped_driver_field", "REQ-0067"),
        ("ungrouped_driver_field", "REQ-0107"),
    ]
    assert raised.value.diagnostics[0].context["identifier"] == "SRC.K"
    assert raised.value.diagnostics[1].context["identifier"] == "SRC.K"


def test_a_column_level_non_key_source_on_a_grouped_row_fails() -> None:
    source = frame_from_values(
        (
            TypedColumn(name="K", type="str"),
            TypedColumn(name="W", type="float"),
        ),
        [["a", 1.0], ["a", 2.0]],
    )
    spec = specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.K"})),
            Column(name="W", type="float", derivation=derivation({"source": "SRC.W"})),
        ],
        [
            Row(
                id="g",
                group_by=["SRC.K"],
                derivations={},
            )
        ],
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(spec, {"SRC": source})

    # REQ-0107: a column-level derivation reads every constructed row, so a
    # scalar source of a grouped driver must name a group key.
    [diagnostic] = raised.value.diagnostics
    assert diagnostic.condition == "ungrouped_driver_field"
    assert diagnostic.requirement == "REQ-0107"
    assert diagnostic.context["identifier"] == "SRC.W"


def test_a_column_level_group_key_echo_on_a_grouped_row_plans() -> None:
    source = frame_from_values(
        (
            TypedColumn(name="K", type="str"),
            TypedColumn(name="W", type="float"),
        ),
        [["a", 1.0], ["a", 2.0]],
    )
    spec = specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.K"})),
        ],
        [
            Row(
                id="g",
                group_by=["SRC.K"],
                derivations={},
            )
        ],
    )

    plan = plan_execution(spec, {"SRC": source})

    assert [planned.column for planned in plan.columns] == ["K"]


def test_a_row_inline_lookup_matching_driver_fields_is_planned() -> None:
    spec = row_two_dataset_specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.K"})),
            Column(name="V", type="float"),
        ],
        [
            Row(
                id="r",
                dataset="SRC",
                derivations={
                    "V": derivation(
                        {
                            "lookup": {
                                "dataset": "RIGHT",
                                "key_base": ["SRC.K"],
                                "key": ["K"],
                                "value": "V",
                            }
                        }
                    )
                },
            )
        ],
    )

    # REQ-0156: an explicit lookup states the same driver-side match.
    plan_execution(
        spec,
        row_tables(),
        supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
    )
