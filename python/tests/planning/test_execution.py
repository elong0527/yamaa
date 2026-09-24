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
    IntermediateVerification,
    OrderTerm,
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
    ).model_copy(
        update={
            "intermediates": [
                # REQ-1248: the intermediate projects its dataset; a bare
                # alias would be rejected before key inference runs.
                Intermediate(id="LOOK", dataset="RIGHT", columns=["X", "V"])
            ]
        }
    )

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
    ).model_copy(
        update={
            "intermediates": [Intermediate(id="LOOK", dataset="RIGHT", columns=["V"])]
        }
    )

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


def test_a_lookup_filter_with_an_unqualified_field_suggests_the_qualified_spelling() -> (
    None
):
    spec = two_dataset_specification(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(name="V", type="float", derivation=derivation({"source": "LOOK.V"})),
        ]
    ).model_copy(
        update={
            "intermediates": [
                Intermediate(id="LOOK", dataset="RIGHT", key=["X"], filter="V > 0")
            ]
        }
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": source_table(), "RIGHT": right_table()},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    # REQ-0120: the qualifier stays mandatory; the diagnostic suggests it.
    [diagnostic] = [d for d in raised.value.diagnostics if d.requirement == "REQ-0120"]
    assert diagnostic.requirement == "REQ-0120"
    assert diagnostic.spec_paths == ("intermediates[0].filter",)
    assert diagnostic.context["identifier"] == "V"
    assert diagnostic.context["suggestion"] == "RIGHT.V"


def test_a_lookup_filter_with_a_wrongly_qualified_field_suggests_the_qualified_spelling() -> (
    None
):
    spec = two_dataset_specification(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(name="V", type="float", derivation=derivation({"source": "LOOK.V"})),
        ]
    ).model_copy(
        update={
            "intermediates": [
                Intermediate(id="LOOK", dataset="RIGHT", key=["X"], filter="SRC.V > 0")
            ]
        }
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": source_table(), "RIGHT": right_table()},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    # REQ-0120: naming another dataset's qualifier is the same failure, with
    # the same suggestion.
    [diagnostic] = [d for d in raised.value.diagnostics if d.requirement == "REQ-0120"]
    assert diagnostic.requirement == "REQ-0120"
    assert diagnostic.context["identifier"] == "SRC.V"
    assert diagnostic.context["suggestion"] == "RIGHT.V"


def test_a_lookup_filter_with_a_genuinely_unknown_field_suggests_nothing() -> None:
    spec = two_dataset_specification(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(name="V", type="float", derivation=derivation({"source": "LOOK.V"})),
        ]
    ).model_copy(
        update={
            "intermediates": [
                Intermediate(
                    id="LOOK", dataset="RIGHT", key=["X"], filter="RIGHT.NOPE > 0"
                )
            ]
        }
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": source_table(), "RIGHT": right_table()},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    [diagnostic] = [d for d in raised.value.diagnostics if d.requirement == "REQ-0120"]
    assert diagnostic.requirement == "REQ-0120"
    assert "suggestion" not in diagnostic.context


def test_a_lookup_order_by_with_an_unqualified_field_suggests_the_qualified_spelling() -> (
    None
):
    spec = two_dataset_specification(
        [
            Column(name="X", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(name="V", type="float", derivation=derivation({"source": "LOOK.V"})),
        ]
    ).model_copy(
        update={
            "intermediates": [
                Intermediate(
                    id="LOOK",
                    dataset="RIGHT",
                    key=["X"],
                    order_by=[OrderTerm(variable="V")],
                    keep="first",
                )
            ]
        }
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": source_table(), "RIGHT": right_table()},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    # REQ-0120: order_by carries the same mandatory qualifier.
    [diagnostic] = [d for d in raised.value.diagnostics if d.requirement == "REQ-0120"]
    assert diagnostic.requirement == "REQ-0120"
    assert diagnostic.spec_paths == ("intermediates[0].order_by[0]",)
    assert diagnostic.context["identifier"] == "V"
    assert diagnostic.context["suggestion"] == "RIGHT.V"


def test_an_inline_lookup_filter_with_an_unqualified_field_suggests_the_qualified_spelling() -> (
    None
):
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
                            "key_base": "SRC.X",
                            "key": ["X"],
                            "value": "V",
                            "filter": "V > 0",
                        }
                    }
                ),
            ),
        ]
    )

    # REQ-0120/REQ-0137: the inline filter keeps the mandatory qualifier and
    # suggests it, exactly like the named form.
    assert diagnostic.condition == "unknown_field"
    assert diagnostic.requirement == "REQ-0120"
    assert diagnostic.spec_paths == ("columns.V.derivation.lookup.filter",)
    assert diagnostic.context["identifier"] == "V"
    assert diagnostic.context["suggestion"] == "RIGHT.V"


def test_an_inline_lookup_filter_with_a_genuinely_unknown_field_suggests_nothing() -> (
    None
):
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
                            "key_base": "SRC.X",
                            "key": ["X"],
                            "value": "V",
                            "filter": "NOPE > 0",
                        }
                    }
                ),
            ),
        ]
    )

    # REQ-0120: a field the dataset does not have gets no suggestion.
    assert diagnostic.condition == "unknown_field"
    assert diagnostic.requirement == "REQ-0120"
    assert diagnostic.spec_paths == ("columns.V.derivation.lookup.filter",)
    assert diagnostic.context["identifier"] == "NOPE"
    assert "suggestion" not in diagnostic.context


def test_an_inline_lookup_order_by_with_an_unqualified_field_suggests_the_qualified_spelling() -> (
    None
):
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
                            "key_base": "SRC.X",
                            "key": ["X"],
                            "value": "V",
                            "order_by": ["V"],
                            "keep": "first",
                        }
                    }
                ),
            ),
        ]
    )

    # REQ-0120/REQ-0137: order_by carries the same mandatory qualifier.
    assert diagnostic.condition == "unknown_field"
    assert diagnostic.requirement == "REQ-0120"
    assert diagnostic.spec_paths == ("columns.V.derivation.lookup.order_by[0]",)
    assert diagnostic.context["identifier"] == "V"
    assert diagnostic.context["suggestion"] == "RIGHT.V"


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


def test_a_grouped_row_aggregate_declares_no_key_pairs() -> None:
    # REQ-0142: the group is the match. A key pair would be ignored and the
    # aggregate would still reduce the current group, so it is refused.
    spec = specification(
        [Column(name="A", type="int")],
        [
            Row(
                id="row",
                group_by=["SRC.X"],
                derivations={
                    "A": derivation(
                        {"aggregate": {"key_base": ["SRC.X"], "expr": "COUNT(SRC.*)"}}
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

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.condition == "invalid_aggregate_context"
    assert diagnostic.requirement == "REQ-0142"
    assert diagnostic.spec_paths == ("rows[0].derivations.A.aggregate.key_base",)


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


def test_a_grouped_row_lookup_keyed_on_group_keys_is_planned() -> None:
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
                derivations={"V": derivation({"source": "LOOK.V"})},
            )
        ],
    ).model_copy(
        update={
            "intermediates": [
                Intermediate(id="LOOK", dataset="RIGHT", key_base=["SRC.K"], key=["K"])
            ]
        }
    )

    # Issue #711: group keys are known while grouped rows are built, so the
    # lookup key_base needs no template derivation.
    plan_execution(
        spec,
        row_tables(),
        supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
    )


def test_an_ungrouped_row_lookup_keyed_on_driver_fields_is_planned() -> None:
    spec = row_two_dataset_specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.K"})),
            Column(name="V", type="float"),
        ],
        [
            Row(
                id="r",
                dataset="SRC",
                derivations={"V": derivation({"source": "LOOK.V"})},
            )
        ],
    ).model_copy(
        update={
            "intermediates": [
                Intermediate(id="LOOK", dataset="RIGHT", key_base=["SRC.K"], key=["K"])
            ]
        }
    )

    # Issue #711: an ungrouped template reads its driver record 1:1, so
    # driver fields are available at row construction.
    plan_execution(
        spec,
        row_tables(),
        supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
    )


def test_a_grouped_row_lookup_keyed_on_a_varying_driver_field_still_fails() -> None:
    tables = {
        "SRC": frame_from_values(
            (
                TypedColumn(name="K", type="str"),
                TypedColumn(name="S", type="str"),
            ),
            [["a", "one"], ["a", "two"]],
        ),
        "RIGHT": frame_from_values(
            (
                TypedColumn(name="K", type="str"),
                TypedColumn(name="V", type="float"),
            ),
            [["one", 10.0]],
        ),
    }
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
                derivations={"V": derivation({"source": "LOOK.V"})},
            )
        ],
    ).model_copy(
        update={
            "intermediates": [
                Intermediate(id="LOOK", dataset="RIGHT", key_base=["SRC.S"], key=["K"])
            ]
        }
    )

    # REQ-0126 still holds for a driver field that varies within the group.
    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            tables,
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    diagnostics = [
        diagnostic
        for diagnostic in raised.value.diagnostics
        if diagnostic.condition == "phase_boundary"
    ]
    assert diagnostics
    assert diagnostics[0].context["identifier"] == "SRC.S"
    assert diagnostics[0].context["required_phase"] == "row_construction"


def test_a_root_filter_is_the_filter_only_row_template() -> None:
    spec = specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.X"})),
        ]
    ).model_copy(update={"filter": "SRC.X <> 'two'"})

    plan = plan_execution(spec, {"SRC": source_table()})

    (row_plan,) = plan.rows
    assert row_plan.declaration is None
    assert row_plan.driver == "SRC"
    assert row_plan.filter_path == "filter"
    assert row_plan.filter_predicate is not None


def test_a_root_filter_declared_with_rows_fails() -> None:
    spec = specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.X"})),
        ],
        rows=[Row(id="all", derivations={})],
    ).model_copy(update={"filter": "SRC.X <> 'two'"})

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(spec, {"SRC": source_table()})

    (diagnostic,) = raised.value.diagnostics
    assert diagnostic.condition == "conflicting_row_construction"
    assert diagnostic.requirement == "REQ-1171"
    assert diagnostic.spec_paths == ("filter", "rows")


def test_a_root_filter_naming_an_unqualified_variable_fails() -> None:
    spec = specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.X"})),
        ]
    ).model_copy(update={"filter": "X <> 'two'"})

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(spec, {"SRC": source_table()})

    (diagnostic,) = raised.value.diagnostics
    assert diagnostic.condition == "phase_boundary"
    assert diagnostic.spec_paths == ("filter",)


def test_to_date_accepts_iso_text_at_planning() -> None:
    # Issue #703: the planner no longer pins `to_date.source` to `datetime`;
    # ISO text is answered at evaluation under REQ-0607.
    spec = specification(
        [
            Column(name="DTC", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(
                name="TRTSDT",
                type="date",
                derivation=derivation({"to_date": {"source": "DTC"}}),
            ),
        ]
    )

    plan = plan_execution(
        spec,
        {"SRC": source_table()},
        supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
    )

    assert [column.column for column in plan.columns] == ["DTC", "TRTSDT"]


def test_to_date_still_accepts_a_datetime_source_at_planning() -> None:
    datetime_table = frame_from_values(
        (TypedColumn(name="X", type="datetime"),),
        [["2025-01-12T14:30:05"]],
    )
    spec = specification(
        [
            Column(
                name="DTM", type="datetime", derivation=derivation({"source": "SRC.X"})
            ),
            Column(
                name="DT",
                type="date",
                derivation=derivation({"to_date": {"source": "DTM"}}),
            ),
        ]
    )

    plan = plan_execution(
        spec,
        {"SRC": datetime_table},
        supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
    )

    assert [column.column for column in plan.columns] == ["DTM", "DT"]


def _intermediate_spec(
    derivations: dict[str, dict[str, object]] | None,
    key: list[str],
    key_base: list[str] | None = None,
    read_field: str = "QVAL",
    **intermediate_fields: object,
) -> Specification:
    columns = [
        Column(
            name="STUDYID", type="str", derivation=derivation({"source": "SRC.STUDYID"})
        ),
        Column(
            name="LBSEQ", type="int", derivation=derivation({"source": "SRC.LBSEQ"})
        ),
        Column(
            name="EPFLAG",
            type="str",
            derivation=derivation({"source": f"SUP_EP.{read_field}"}),
        ),
    ]
    spec = specification(columns).model_copy(
        update={
            "input": {
                "SRC": DatasetSource(path="input/source.csv"),
                "SUPP": DatasetSource(path="input/supp.csv"),
            },
            "intermediates": [
                Intermediate(
                    id="SUP_EP",
                    dataset="SUPP",
                    derivations={
                        name: derivation(expression)
                        for name, expression in (derivations or {}).items()
                    }
                    or None,
                    key=key,
                    key_base=key_base,
                    **intermediate_fields,
                )
            ],
        }
    )
    return spec


def _supp_table() -> object:
    return frame_from_values(
        (
            TypedColumn(name="STUDYID", type="str"),
            TypedColumn(name="USUBJID", type="str"),
            TypedColumn(name="IDVARVAL", type="str"),
            TypedColumn(name="QVAL", type="str"),
        ),
        [["S1", "U1", "  259", "Y"]],
    )


def _src_table() -> object:
    return frame_from_values(
        (
            TypedColumn(name="STUDYID", type="str"),
            TypedColumn(name="USUBJID", type="str"),
            TypedColumn(name="LBSEQ", type="int"),
        ),
        [["S1", "U1", 259]],
    )


def test_an_intermediate_derivation_may_feed_a_target_side_key() -> None:
    # REQ-1185: the derived name is a legal target-side key field.
    spec = _intermediate_spec(
        {"IDVARVAL_U": {"str_upper": {"source": "IDVARVAL"}}},
        key=["STUDYID", "USUBJID", "IDVARVAL_U"],
        key_base=["SRC.STUDYID", "SRC.USUBJID", "SRC.STUDYID"],
    )

    plan = plan_execution(
        spec,
        {"SRC": _src_table(), "SUPP": _supp_table()},
        supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
    )

    assert plan.intermediates[0].match_fields == ("STUDYID", "USUBJID", "IDVARVAL_U")
    assert plan.intermediates[0].derived[0][0] == "IDVARVAL_U"


def test_intermediate_clauses_may_read_a_derived_name() -> None:
    spec = _intermediate_spec(
        {"IDVARVAL_U": {"str_upper": {"source": "IDVARVAL"}}},
        key=["STUDYID", "USUBJID"],
        key_base=["SRC.STUDYID", "SRC.USUBJID"],
        read_field="IDVARVAL_U",
        filter="SUPP.IDVARVAL_U IS NOT NULL",
        order_by=[OrderTerm(variable="SUPP.IDVARVAL_U")],
        keep="first",
        columns=["IDVARVAL_U"],
    )

    plan = plan_execution(
        spec,
        {"SRC": _src_table(), "SUPP": _supp_table()},
        supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
    )

    intermediate = plan.intermediates[0]
    assert intermediate.filter_predicate is not None
    assert intermediate.order_terms[0][1] == "IDVARVAL_U"
    assert intermediate.readable_columns == ("IDVARVAL_U",)


def test_an_unqualified_derived_order_field_suggests_its_qualified_name() -> None:
    spec = _intermediate_spec(
        {"QVAL_U": {"str_upper": {"source": "QVAL"}}},
        key=["STUDYID"],
        order_by=[OrderTerm(variable="QVAL_U")],
        keep="first",
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": _src_table(), "SUPP": _supp_table()},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    [diagnostic] = [
        d
        for d in raised.value.diagnostics
        if d.spec_paths == ("intermediates[0].order_by[0]",)
    ]
    assert diagnostic.condition == "unknown_field"
    assert diagnostic.context["suggestion"] == "SUPP.QVAL_U"


def test_a_failed_derivation_does_not_repeat_as_an_unknown_order_field() -> None:
    spec = _intermediate_spec(
        {"QVAL_U": {"str_upper": {"source": "NOPE"}}},
        key=["STUDYID"],
        order_by=[OrderTerm(variable="SUPP.QVAL_U")],
        keep="first",
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": _src_table(), "SUPP": _supp_table()},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    assert any(
        d.spec_paths == ("intermediates[0].derivations.QVAL_U.str_upper.source",)
        for d in raised.value.diagnostics
    )
    assert all(
        d.spec_paths != ("intermediates[0].order_by[0]",)
        for d in raised.value.diagnostics
    )


def _derivation_diagnostic(
    derivations: dict[str, dict[str, object]], condition: str
) -> object:
    spec = _intermediate_spec(derivations, key=["STUDYID"])
    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": _src_table(), "SUPP": _supp_table()},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )
    diagnostics = [
        diagnostic
        for diagnostic in raised.value.diagnostics
        if diagnostic.condition == condition
    ]
    assert diagnostics, raised.value.diagnostics
    return diagnostics[0]


def test_an_intermediate_derivation_rejects_a_driver_reference() -> None:
    # REQ-1185: the driver is out of scope for an intermediate derivation.
    diagnostic = _derivation_diagnostic(
        {"IDVARVAL_U": {"str_upper": {"source": "SRC.LBSEQ"}}}, "unknown_field"
    )

    assert diagnostic.requirement == "REQ-1185"
    assert diagnostic.spec_paths == (
        "intermediates[0].derivations.IDVARVAL_U.str_upper.source",
    )


def test_an_intermediate_derivation_rejects_another_dataset_reference() -> None:
    # REQ-1185: a qualified name must name the intermediate's own dataset.
    diagnostic = _derivation_diagnostic(
        {"IDVARVAL_U": {"str_upper": {"source": "SRC.IDVARVAL"}}}, "unknown_field"
    )

    assert diagnostic.requirement == "REQ-1185"


def test_an_intermediate_derivation_rejects_a_sibling_derivation() -> None:
    # REQ-1185: derivations read stored columns only, not each other.
    diagnostic = _derivation_diagnostic(
        {
            "FIRST_N": {"str_upper": {"source": "IDVARVAL"}},
            "SECOND_N": {"str_upper": {"source": "FIRST_N"}},
        },
        "unknown_field",
    )

    assert diagnostic.requirement == "REQ-1185"
    assert diagnostic.spec_paths == (
        "intermediates[0].derivations.SECOND_N.str_upper.source",
    )


def test_an_intermediate_derivation_rejects_a_stored_column_shadow() -> None:
    # REQ-1185: the derived name would hide the stored column.
    diagnostic = _derivation_diagnostic(
        {"IDVARVAL": {"str_upper": {"source": "IDVARVAL"}}}, "duplicate_derivation"
    )

    assert diagnostic.requirement == "REQ-1185"
    assert diagnostic.spec_paths == ("intermediates[0].derivations.IDVARVAL",)


def test_a_key_an_intermediate_matches_on_is_derived_before_its_reader() -> None:
    # REQ-0050 makes the intermediate's match values dependencies of the
    # reading column; the forward_reference check exempts keys (REQ-0074),
    # so the planner orders the key before its reader instead of failing.
    spec = specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(name="V", type="str", derivation=derivation({"source": "LOOK.X"})),
            Column(name="J", type="str", derivation=derivation({"source": "SRC.X"})),
        ]
    ).model_copy(
        update={
            "keys": ["K", "J"],
            "intermediates": [
                Intermediate(id="LOOK", dataset="SRC", key_base=["J"], key=["X"])
            ],
        }
    )

    plan = plan_execution(spec, {"SRC": source_table()})

    assert [planned.column for planned in plan.columns] == ["K", "J", "V"]


def _death_source_table() -> object:
    return frame_from_values(
        (
            TypedColumn(name="X", type="str"),
            TypedColumn(name="DTHFL2", type="str"),
        ),
        [["one", "Y"], ["two", "N"]],
    )


def _case_flag(when: str) -> HandledExpression:
    return derivation(
        {
            "case": [
                {"when": when, "then": {"literal": "Y"}},
                {"otherwise": {"literal": "N"}},
            ]
        }
    )


def test_a_column_case_predicate_naming_a_source_field_suggests_the_qualified_spelling() -> (
    None
):
    # REQ-0189 / issue #780: an unqualified predicate identifier that names a
    # source field is unresolvable as written (REQ-0106), not an unknown field.
    spec = specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(name="DTHFL", type="str", derivation=_case_flag("DTHFL2 = 'Y'")),
        ]
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": _death_source_table()},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    [diagnostic] = [d for d in raised.value.diagnostics if d.requirement == "REQ-0189"]
    assert diagnostic.condition == "unresolvable_name"
    assert diagnostic.spec_paths == ("columns.DTHFL.derivation.case[0].when",)
    assert diagnostic.context == {
        "identifier": "DTHFL2",
        "suggestion": "SRC.DTHFL2",
    }


def test_a_column_case_predicate_with_a_genuinely_unknown_name_stays_unknown_field() -> (
    None
):
    # REQ-0189: a name no dataset carries is still unknown_field.
    spec = specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(name="DTHFL", type="str", derivation=_case_flag("NOPE = 'Y'")),
        ]
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": _death_source_table()},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    [diagnostic] = [d for d in raised.value.diagnostics if d.requirement == "REQ-0189"]
    assert diagnostic.condition == "unknown_field"
    assert diagnostic.spec_paths == ("columns.DTHFL.derivation.case[0].when",)
    assert diagnostic.context == {"identifier": "NOPE"}


def test_a_column_case_predicate_with_a_qualified_source_field_plans() -> None:
    spec = specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(name="DTHFL", type="str", derivation=_case_flag("SRC.DTHFL2 = 'Y'")),
        ]
    )

    plan = plan_execution(
        spec,
        {"SRC": _death_source_table()},
        supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
    )

    assert [planned.column for planned in plan.columns] == ["K", "DTHFL"]


def _flag_field(condition: str) -> HandledExpression:
    return derivation({"flag": {"condition": condition, "false_value": "N"}})


def test_a_column_flag_predicate_naming_a_source_field_suggests_the_qualified_spelling() -> (
    None
):
    # REQ-0189 / REQ-1255: the flag condition names predicate identifiers
    # like a case when does.
    spec = specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(name="DTHFL", type="str", derivation=_flag_field("DTHFL2 = 'Y'")),
        ]
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": _death_source_table()},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    [diagnostic] = [d for d in raised.value.diagnostics if d.requirement == "REQ-0189"]
    assert diagnostic.condition == "unresolvable_name"
    assert diagnostic.spec_paths == ("columns.DTHFL.derivation.flag.condition",)
    assert diagnostic.context == {
        "identifier": "DTHFL2",
        "suggestion": "SRC.DTHFL2",
    }


def test_a_bare_string_flag_condition_names_predicate_identifiers() -> None:
    # REQ-1255: a bare predicate string is the condition; its identifiers
    # are reported at the flag itself, not at flag.condition.
    spec = specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(
                name="DTHFL",
                type="str",
                derivation=derivation({"flag": "DTHFL2 = 'Y'"}),
            ),
        ]
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": _death_source_table()},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    [diagnostic] = [d for d in raised.value.diagnostics if d.requirement == "REQ-0189"]
    assert diagnostic.condition == "unresolvable_name"
    assert diagnostic.spec_paths == ("columns.DTHFL.derivation.flag",)
    assert diagnostic.context == {
        "identifier": "DTHFL2",
        "suggestion": "SRC.DTHFL2",
    }


def test_a_column_flag_with_a_qualified_source_field_plans() -> None:
    spec = specification(
        [
            Column(name="K", type="str", derivation=derivation({"source": "SRC.X"})),
            Column(
                name="DTHFL", type="str", derivation=_flag_field("SRC.DTHFL2 = 'Y'")
            ),
        ]
    )

    plan = plan_execution(
        spec,
        {"SRC": _death_source_table()},
        supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
    )

    assert [planned.column for planned in plan.columns] == ["K", "DTHFL"]


def test_a_row_case_predicate_naming_a_driver_field_suggests_the_qualified_spelling() -> (
    None
):
    # REQ-0106: the same unresolvable-name diagnostic applies in row
    # derivations, where the row's driver is the in-scope dataset.
    spec = specification(
        [
            Column(name="K", type="str"),
            Column(name="DTHFL", type="str"),
        ],
        [
            Row(
                id="row",
                derivations={
                    "K": derivation({"source": "SRC.X"}),
                    "DTHFL": _case_flag("DTHFL2 = 'Y'"),
                },
            )
        ],
    )

    with pytest.raises(ExecutionPlanningError) as raised:
        plan_execution(
            spec,
            {"SRC": _death_source_table()},
            supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
        )

    [diagnostic] = [d for d in raised.value.diagnostics if d.requirement == "REQ-0189"]
    assert diagnostic.condition == "unresolvable_name"
    assert diagnostic.spec_paths == ("rows[0].derivations.DTHFL.case[0].when",)
    assert diagnostic.context == {
        "identifier": "DTHFL2",
        "suggestion": "SRC.DTHFL2",
    }


def _verification_source_table() -> object:
    return frame_from_values(
        (
            TypedColumn(name="STUDYID", type="str"),
            TypedColumn(name="USUBJID", type="str"),
        ),
        [["S1", "P01"]],
    )


def _verification_ds_table() -> object:
    return frame_from_values(
        (
            TypedColumn(name="STUDYID", type="str"),
            TypedColumn(name="USUBJID", type="str"),
            TypedColumn(name="DSCAT", type="str"),
            TypedColumn(name="DSDECOD", type="str"),
        ),
        [["S1", "P01", "DISPOSITION EVENT", "COMPLETED"]],
    )


def _verification_spec(**intermediate_fields: object) -> Specification:
    columns = [
        Column(
            name="STUDYID",
            type="str",
            derivation=derivation({"source": "SRC.STUDYID"}),
        ),
        Column(
            name="USUBJID",
            type="str",
            derivation=derivation({"source": "SRC.USUBJID"}),
        ),
    ]
    return specification(columns).model_copy(
        update={
            "input": {
                "SRC": DatasetSource(path="input/source.csv"),
                "DS": DatasetSource(path="input/ds.csv"),
            },
            "intermediates": [
                Intermediate(
                    id="DS_EOS",
                    dataset="DS",
                    key=["STUDYID", "USUBJID"],
                    key_base=["SRC.STUDYID", "SRC.USUBJID"],
                    **intermediate_fields,
                )
            ],
        }
    )


def _plan_verification_spec(**intermediate_fields: object) -> object:
    return plan_execution(
        _verification_spec(**intermediate_fields),
        {"SRC": _verification_source_table(), "DS": _verification_ds_table()},
        supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
    )


def test_intermediate_verification_unique_columns_reach_the_plan() -> None:
    # REQ-1245: the declared uniqueness columns ride into the plan.
    plan = _plan_verification_spec(
        filter="DS.DSCAT = 'DISPOSITION EVENT'",
        verification=IntermediateVerification(unique=["STUDYID", "USUBJID"]),
    )

    assert plan.intermediates[0].unique_columns == ("STUDYID", "USUBJID")


def test_intermediate_verification_rejects_an_unknown_column() -> None:
    # REQ-1245: a unique column must name a stored field.
    with pytest.raises(ExecutionPlanningError) as raised:
        _plan_verification_spec(
            verification=IntermediateVerification(unique=["STUDYID", "NOPE"]),
        )

    (diagnostic,) = [
        diagnostic
        for diagnostic in raised.value.diagnostics
        if diagnostic.condition == "unknown_field"
    ]
    assert diagnostic.requirement == "REQ-1245"
    assert diagnostic.spec_paths == ("intermediates[0].verification.unique[1]",)
    assert diagnostic.context["identifier"] == "DS.NOPE"


def test_intermediate_verification_rejects_a_correlated_filter() -> None:
    # REQ-1245: a correlated filter admits no single run-wide donor set.
    with pytest.raises(ExecutionPlanningError) as raised:
        _plan_verification_spec(
            filter="DS.DSCAT = 'DISPOSITION EVENT' AND DS.USUBJID = SRC.USUBJID",
            verification=IntermediateVerification(unique=["STUDYID", "USUBJID"]),
        )

    (diagnostic,) = raised.value.diagnostics
    assert diagnostic.condition == "correlated_filter_with_unique_verification"
    assert diagnostic.requirement == "REQ-1245"
    assert diagnostic.spec_paths == ("intermediates[0].verification",)
