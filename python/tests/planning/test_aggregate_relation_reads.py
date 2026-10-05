"""Fieldless reducers still read a relation and its declared match keys."""

import pytest

from yamaa.expressions import DEFAULT_EXPRESSION_OPERATIONS
from yamaa.io.polars import frame_from_values
from yamaa.models import TypedColumn
from yamaa.planning import ExecutionPlanningError, plan_execution
from yamaa.specification.models import (
    Column,
    DatasetSource,
    Expression,
    HandledExpression,
    Output,
    Specification,
)


def derivation(root):
    """Keep authored expression inputs independent of the planner under test."""
    return HandledExpression(value=Expression(root=root))


def count_spec(key, expr="COUNT(OTHER.*)"):
    """Declare the consumer before its key producer to expose missing graph edges."""
    return Specification(
        schema_version="1.0",
        domain="OUT",
        input={
            "SRC": DatasetSource(path="src.csv"),
            "OTHER": DatasetSource(path="other.csv"),
        },
        base="SRC",
        keys=["K"],
        output=Output(path="out.csv", columns=["K", "V"]),
        columns=[
            Column(
                name="V",
                type="int",
                derivation=derivation({"aggregate": {"expr": expr, "key": key}}),
            ),
            Column(name="K", type="int", derivation=derivation({"source": "SRC.N"})),
        ],
    )


def sources():
    """Two independently typed relations with matching and nonmatching donor rows."""
    return {
        "SRC": frame_from_values((TypedColumn(name="N", type="int"),), [[1]]),
        "OTHER": frame_from_values(
            (TypedColumn(name="N", type="int"),), [[1], [1], [2]]
        ),
    }


def plan(spec):
    """Use all ordinary expression contracts, without restricting the result to a prototype."""
    return plan_execution(
        spec, sources(), supported_operations=DEFAULT_EXPRESSION_OPERATIONS
    )


@pytest.mark.parametrize("expr", ["COUNT(OTHER.*)", "SUM(OTHER.N)"])
def test_declared_key_is_a_dependency_and_a_resolved_join(expr):
    """Counting rows depends on the same current-row match value as reducing a field."""
    result = plan(count_spec({"N": "K"}, expr))
    assert [column.column for column in result.columns] == ["K", "V"]
    assert result.columns[1].dependencies == ("K",)
    (join,) = result.resolved_joins
    assert (join.dataset, join.source, join.key, join.inferred, join.spec_path) == (
        "OTHER",
        ("K",),
        ("N",),
        False,
        "columns.V.derivation.aggregate.expr",
    )


@pytest.mark.parametrize("expr", ["COUNT(OTHER.*)", "SUM(OTHER.N)"])
@pytest.mark.parametrize("value,kind", [(True, "bool"), ("x", "str")])
def test_fieldless_count_reports_known_type_mismatch(expr, value, kind):
    """REQ-1259 applies without a reducer field reference; no runtime coercion repairs it."""
    with pytest.raises(ExecutionPlanningError) as caught:
        plan(count_spec({"N": {"literal": value}}, expr))
    assert [
        (d.condition, d.requirement, d.spec_paths, dict(d.context))
        for d in caught.value.diagnostics
    ] == [
        (
            "incompatible_input_type",
            "REQ-0004",
            ("columns.V.derivation.aggregate.expr",),
            {"source": "key[N]", "expected": kind, "actual": "int"},
        )
    ]


@pytest.mark.parametrize(
    "key,identifier", [({"ABSENT": "K"}, "OTHER.ABSENT"), ({"N": "ABSENT"}, "ABSENT")]
)
def test_count_rejects_unknown_donor_or_current_key(key, identifier):
    """Declared key names must exist even when COUNT names no donor field."""
    with pytest.raises(ExecutionPlanningError) as caught:
        plan(count_spec(key))
    assert [
        (d.condition, d.requirement, d.spec_paths, dict(d.context))
        for d in caught.value.diagnostics
    ] == [
        (
            "unknown_field",
            "REQ-0141",
            ("columns.V.derivation.aggregate.expr",),
            {"identifier": identifier},
        )
    ]


@pytest.mark.parametrize("expr", ["COUNT(OTHER.*)", "SUM(OTHER.N)"])
@pytest.mark.parametrize(
    "root,deps",
    [
        ({"literal": 1}, ()),
        ({"source": "K"}, ("K",)),
        ({"greatest": {"sources": ["K", "K"]}}, ("K",)),
    ],
)
def test_expression_key_dependencies_never_include_synthetic_match_names(
    expr, root, deps
):
    """The key alias is diagnostic metadata; only actual inner reads order derivations."""
    result = plan(count_spec({"N": root}, expr))
    consumer = next(column for column in result.columns if column.column == "V")
    assert consumer.dependencies == deps
    assert all(not name.startswith("key[") for name in consumer.dependencies)
    assert len(result.resolved_joins) == 1


@pytest.mark.parametrize(
    "key", [{"N": "K"}, {"N": {"source": "K"}}, {"N": {"literal": 1}}]
)
def test_count_executes_after_its_key_and_returns_authored_csv(key):
    """Actual reference execution counts two matching donors, never all three or no rows."""
    from yamaa.io import render_artifact
    from yamaa.runtime.executor import execute_with_source_provider

    result = execute_with_source_provider(count_spec(key), lambda _: sources())
    assert result.status == "success"
    assert render_artifact(result.artifact) == b"K,V\n1,2\n"


def test_unknown_relation_is_not_a_stored_wildcard_field():
    """A fieldless read reports the authored relation token once, without invented key errors."""
    with pytest.raises(ExecutionPlanningError) as caught:
        plan(count_spec({"N": "K"}, "COUNT(ABSENT.*)"))
    assert [
        (d.condition, d.requirement, d.spec_paths, dict(d.context))
        for d in caught.value.diagnostics
    ] == [
        (
            "unknown_field",
            "REQ-0103",
            ("columns.V.derivation.aggregate.expr",),
            {"identifier": "ABSENT.*"},
        )
    ]


@pytest.mark.parametrize(
    "expr", ["COUNT(OTHER.*) + COUNT(OTHER.*)", "COUNT(OTHER.*) + SUM(OTHER.N)"]
)
def test_repeated_or_mixed_reductions_preserve_one_join(expr):
    """A field read can carry the pairing already; COUNT must not duplicate its key finding."""
    with pytest.raises(ExecutionPlanningError) as caught:
        plan(count_spec({"N": {"literal": True}}, expr))
    assert len(caught.value.diagnostics) == 1
    assert caught.value.diagnostics[0].requirement == "REQ-0004"
    result = plan(count_spec({"N": "K"}, expr))
    assert len(result.resolved_joins) == 1
    assert [column.column for column in result.columns] == ["K", "V"]


def test_fieldless_count_infers_keys_before_recording_dependencies():
    """Omitted keys retain inference provenance and put their producer before COUNT."""
    spec = count_spec({"N": "K"})
    spec = spec.model_copy(
        update={
            "keys": ["N"],
            "output": Output(path="out.csv", columns=["N", "V"]),
            "columns": [
                spec.columns[0].model_copy(
                    update={
                        "derivation": derivation(
                            {"aggregate": {"expr": "COUNT(OTHER.*)"}}
                        )
                    }
                ),
                spec.columns[1].model_copy(update={"name": "N"}),
            ],
        }
    )
    result = plan(spec)
    assert [column.column for column in result.columns] == ["N", "V"]
    assert result.columns[1].dependencies == ("N",)
    (join,) = result.resolved_joins
    assert (join.source, join.key, join.inferred) == (("N",), ("N",), True)


def test_existing_filter_reference_retains_diagnostic_path():
    """A COUNT with a donor filter already carried the pairing; preserve its provenance."""
    spec = count_spec({"N": {"literal": True}})
    declaration = derivation(
        {
            "aggregate": {
                "expr": "COUNT(OTHER.*)",
                "key": {"N": {"literal": True}},
                "filter": "OTHER.N > 0",
            }
        }
    )
    spec = spec.model_copy(
        update={
            "columns": [
                spec.columns[0].model_copy(update={"derivation": declaration}),
                spec.columns[1],
            ]
        }
    )
    with pytest.raises(ExecutionPlanningError) as caught:
        plan(spec)
    assert [(d.requirement, d.spec_paths) for d in caught.value.diagnostics] == [
        ("REQ-0004", ("columns.V.derivation.aggregate.filter",))
    ]
