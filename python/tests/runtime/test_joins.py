from __future__ import annotations

import pytest

from yamaa.expressions import (
    DEFAULT_EXPRESSION_OPERATIONS,
    FailedResolution,
    ResolvedValue,
    parse_predicate,
)
from yamaa.io.polars import frame_from_values
from yamaa.models import MISSING, ConditionResult, DateValue, TypedColumn, ValueResult
from yamaa.planning import plan_execution
from yamaa.runtime import (
    ExecutionFailure,
    ExecutionSuccess,
    execute_with_source_provider,
)
from yamaa.runtime.joins import (
    RelationIndex,
    applicable_keys,
    compare_values,
    eligible_records,
    join_scalar,
    order_records,
    partition_records,
    resolution_result,
)
from yamaa.specification.models import (
    Column,
    DatasetSource,
    Expression,
    HandledExpression,
    OrderTerm,
    Output,
    Specification,
)

# R006 normalization expands a bare order term before execution sees it.
ORDER_BY = {"variable": "EX.EXSTDTC", "direction": "asc", "nulls": "last"}


def relation(
    name: str,
    columns: list[tuple[str, str]],
    rows: list[list[object]],
) -> RelationIndex:
    typed = tuple(
        TypedColumn(name=column, type=column_type) for column, column_type in columns
    )
    return RelationIndex(name, frame_from_values(typed, rows))


def ex_relation() -> RelationIndex:
    return relation(
        "EX",
        [
            ("STUDYID", "str"),
            ("USUBJID", "str"),
            ("EXSEQ", "int"),
            ("EXTRT", "str"),
            ("EXSTDTC", "date"),
        ],
        [
            ["CATH", "S1", 1, "VITAMIN D3", DateValue.parse("2025-01-08")],
            ["CATH", "S1", 2, "PLACEBO", DateValue.parse("2025-01-15")],
            ["CATH", "S2", 1, "VITAMIN D3", DateValue.parse("2025-02-08")],
            ["CATH2", "S1", 1, "PLACEBO", DateValue.parse("2025-05-06")],
        ],
    )


def test_a_relation_is_read_once_in_record_order() -> None:
    index = ex_relation()

    assert [record.position for record in index.records] == [0, 1, 2, 3]
    assert index.records[0].values["EXTRT"] == "VITAMIN D3"
    assert index.fields[:2] == ("STUDYID", "USUBJID")


def test_applicable_keys_are_the_output_keys_the_right_side_carries() -> None:
    # REQ-0113 and REQ-0116: the keys, in output `keys` order.
    index = ex_relation()

    assert applicable_keys(["STUDYID", "USUBJID", "LBSEQ"], index) == (
        "STUDYID",
        "USUBJID",
    )
    assert applicable_keys(["USUBJID", "STUDYID"], index) == ("USUBJID", "STUDYID")


def test_a_shared_subject_id_in_another_study_never_combines() -> None:
    index = ex_relation()

    matched = index.matching(("STUDYID", "USUBJID"), ("CATH2", "S1"))

    assert [record.values["EXTRT"] for record in matched] == ["PLACEBO"]


def test_a_record_with_a_missing_key_cannot_match() -> None:
    # REQ-0123: an uncollected identifier is not an identity two rows share.
    index = relation(
        "EX",
        [("USUBJID", "str"), ("EXTRT", "str")],
        [["S1", "A"], [MISSING, "B"]],
    )

    assert index.matching(("USUBJID",), ("S1",)) != ()
    assert index.matching(("USUBJID",), (MISSING,)) == ()


def test_partitions_keep_first_occurrence_order_and_group_missing_together() -> None:
    # REQ-0037 and REQ-0038: missing equals missing for grouping, and a group is
    # ordered by the position of its first record.
    index = relation(
        "LB",
        [("USUBJID", "str"), ("VISIT", "str")],
        [["S2", "V1"], ["S1", MISSING], ["S2", "V1"], ["S1", MISSING]],
    )

    grouped = partition_records(index.records, ("USUBJID", "VISIT"))

    assert list(grouped) == [("S2", "V1"), ("S1", MISSING)]
    assert [record.position for record in grouped[("S1", MISSING)]] == [1, 3]


def test_ordering_places_missing_where_the_term_declares_and_ties_by_position() -> None:
    # REQ-0300: `nulls` does not flip with `direction`.
    index = relation(
        "EX",
        [("EXSTDTC", "date"), ("EXSEQ", "int")],
        [
            [DateValue.parse("2025-01-08"), 1],
            [MISSING, 2],
            [DateValue.parse("2025-01-08"), 3],
        ],
    )
    ascending = OrderTerm(variable="EX.EXSTDTC")
    descending = OrderTerm(variable="EX.EXSTDTC", direction="desc")

    last = order_records(index.records, [(ascending, "EXSTDTC")])
    first = order_records(
        index.records,
        [(OrderTerm(variable="EX.EXSTDTC", nulls="first"), "EXSTDTC")],
    )
    reverse = order_records(index.records, [(descending, "EXSTDTC")])

    assert [record.position for record in last] == [0, 2, 1]
    assert [record.position for record in first] == [1, 0, 2]
    assert [record.position for record in reverse] == [0, 2, 1]


def test_a_right_side_filter_reads_right_side_records_only() -> None:
    index = ex_relation()

    kept = eligible_records(
        index.records, parse_predicate("EX.EXTRT = 'PLACEBO'"), index
    )
    unreachable = eligible_records(
        index.records, parse_predicate("USUBJID = 'S1'"), index
    )

    assert isinstance(kept, list)
    assert [record.position for record in kept] == [1, 3]
    assert isinstance(unreachable, ConditionResult)
    assert unreachable.condition.condition == "unknown_field"


def test_one_match_is_copied_and_no_match_is_missing() -> None:
    index = ex_relation()

    matched = join_scalar(index, ("STUDYID", "USUBJID"), ("CATH", "S2"), "EXTRT")
    absent = join_scalar(index, ("STUDYID", "USUBJID"), ("CATH", "S9"), "EXTRT")

    assert isinstance(matched, ResolvedValue)
    assert matched.value == "VITAMIN D3"
    # REQ-0121 and REQ-0146: an absent right-side record is ordinary missing.
    assert isinstance(absent, ResolvedValue)
    assert absent.value is MISSING


def test_several_matches_fail_unless_the_specification_answers_for_them() -> None:
    index = ex_relation()

    result = join_scalar(index, ("STUDYID", "USUBJID"), ("CATH", "S1"), "EXTRT")

    assert isinstance(result, FailedResolution)
    assert result.condition.condition == "multiple_matches"
    assert result.condition.requirement == "REQ-0145"
    assert result.condition.context == {"dataset": "EX", "match_count": 2}


def test_a_source_filter_narrows_the_join_without_a_declared_selection() -> None:
    # REQ-0131: a filter that leaves one match answers with it, and one that
    # leaves none is the absent match of REQ-0146, not a multiple match.
    index = ex_relation()

    one = join_scalar(
        index,
        ("STUDYID", "USUBJID"),
        ("CATH", "S1"),
        "EXTRT",
        selector="EX.EXSEQ = 1",
    )
    none = join_scalar(
        index,
        ("STUDYID", "USUBJID"),
        ("CATH", "S1"),
        "EXTRT",
        selector="EX.EXSEQ = 9",
    )
    several = join_scalar(
        index,
        ("STUDYID", "USUBJID"),
        ("CATH", "S1"),
        "EXTRT",
        selector="EX.EXSEQ > 0",
    )

    assert isinstance(one, ResolvedValue)
    assert one.value == "VITAMIN D3"
    assert one.handled_by is None
    assert isinstance(none, ResolvedValue)
    assert none.value is MISSING
    assert isinstance(several, FailedResolution)
    assert several.condition.condition == "multiple_matches"


def test_a_declared_selection_chooses_one_match_and_reports_the_handler() -> None:
    index = ex_relation()

    result = join_scalar(
        index,
        ("STUDYID", "USUBJID"),
        ("CATH", "S1"),
        "EXTRT",
        multiple_matches={"order_by": [ORDER_BY], "keep": "last"},
    )

    assert isinstance(result, ResolvedValue)
    assert result.value == "PLACEBO"
    assert result.handled_by == "multiple_matches"


def test_a_selection_filtered_to_one_record_fires_no_handler() -> None:
    # REQ-0356: the count reports only the rows where more than one survived.
    index = ex_relation()

    result = join_scalar(
        index,
        ("STUDYID", "USUBJID"),
        ("CATH", "S1"),
        "EXTRT",
        selector="EX.EXSEQ = 1",
        multiple_matches={"order_by": [ORDER_BY], "keep": "last"},
    )

    assert isinstance(result, ResolvedValue)
    assert result.value == "VITAMIN D3"
    assert result.handled_by is None


def test_a_selection_filtered_to_nothing_is_an_ordinary_absent_match() -> None:
    # REQ-0355: a narrow filter produces missing rather than firing a handler.
    index = ex_relation()

    result = join_scalar(
        index,
        ("STUDYID", "USUBJID"),
        ("CATH", "S1"),
        "EXTRT",
        selector="EX.EXSEQ = 9",
        multiple_matches={"order_by": [ORDER_BY], "keep": "last"},
    )

    assert isinstance(result, ResolvedValue)
    assert result.value is MISSING


def lbref() -> RelationIndex:
    return relation(
        "LBREF",
        [("LBTESTCD", "str"), ("SEX", "str"), ("ANRHI", "float")],
        [["ALT", "F", 33.0], ["ALT", "M", 41.0], ["AST", "F", 32.0]],
    )


def test_an_unnormalized_selection_is_reported_rather_than_raised() -> None:
    result = join_scalar(
        ex_relation(),
        ("STUDYID", "USUBJID"),
        ("CATH", "S1"),
        "EXTRT",
        multiple_matches={"order_by": ["EX.EXSTDTC"], "keep": "last"},
    )

    assert isinstance(result, FailedResolution)
    assert result.condition.condition == "invalid_field_type"


def test_comparison_refuses_two_types_that_are_not_mutually_comparable() -> None:
    assert compare_values(1, 1.0) == 0
    assert compare_values("a", "b") == -1
    with pytest.raises(TypeError):
        compare_values(1, "1")


def test_a_resolution_becomes_the_result_an_expression_returns() -> None:
    assert resolution_result(ResolvedValue(value=3)) == ValueResult(value=3)


# REQ-0150: the implicit join the planner restores for a plain cross-dataset
# scalar source when the applicable keys are clear.


def _implicit_run(
    left_rows: list[list[object]],
    right_rows: list[list[object]],
) -> object:
    left = frame_from_values(
        (
            TypedColumn(name="STUDYID", type="str"),
            TypedColumn(name="USUBJID", type="str"),
        ),
        left_rows,
    )
    right = frame_from_values(
        (
            TypedColumn(name="STUDYID", type="str"),
            TypedColumn(name="USUBJID", type="str"),
            TypedColumn(name="V", type="float"),
        ),
        right_rows,
    )
    specification = Specification(
        schema_version="1.0",
        domain="OUT",
        input={
            "LEFT": DatasetSource(path="input/left.csv"),
            "RIGHT": DatasetSource(path="input/right.csv"),
        },
        base="LEFT",
        keys=["STUDYID", "USUBJID"],
        output=Output(path="out.csv", columns=["STUDYID", "USUBJID", "V"]),
        columns=[
            Column(
                name="STUDYID",
                type="str",
                derivation=HandledExpression(
                    value=Expression(root={"source": "LEFT.STUDYID"})
                ),
            ),
            Column(
                name="USUBJID",
                type="str",
                derivation=HandledExpression(
                    value=Expression(root={"source": "LEFT.USUBJID"})
                ),
            ),
            Column(
                name="V",
                type="float",
                derivation=HandledExpression(
                    value=Expression(root={"source": "RIGHT.V"})
                ),
            ),
        ],
    )
    plan_execution(
        specification,
        {"LEFT": left, "RIGHT": right},
        supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
    )
    return execute_with_source_provider(
        specification, lambda datasets: {"LEFT": left, "RIGHT": right}
    )


def test_implicit_join_matches_on_the_applicable_keys_in_left_row_order() -> None:
    result = _implicit_run(
        [["S", "b"], ["S", "a"]],
        [["S", "a", 1.0], ["S", "b", 2.0]],
    )

    assert isinstance(result, ExecutionSuccess)
    assert result.table.frame.to_dicts() == [
        {"STUDYID": "S", "USUBJID": "b", "V": 2.0},
        {"STUDYID": "S", "USUBJID": "a", "V": 1.0},
    ]


def test_implicit_join_yields_missing_when_no_right_side_matches() -> None:
    result = _implicit_run(
        [["S", "a"]],
        [["S", "b", 1.0]],
    )

    assert isinstance(result, ExecutionSuccess)
    assert result.table.frame.to_dicts() == [
        {"STUDYID": "S", "USUBJID": "a", "V": None}
    ]


def test_implicit_join_reports_duplicate_right_side_matches() -> None:
    result = _implicit_run(
        [["S", "a"]],
        [["S", "a", 1.0], ["S", "a", 2.0]],
    )

    assert isinstance(result, ExecutionFailure)
    assert result.diagnostics[0].condition == "multiple_matches"


def test_a_structured_implicit_source_filters_and_selects_before_reading() -> None:
    # REQ-0111: a structured cross-dataset source keeps its filter and
    # multiple_matches on the implicit join.
    left = frame_from_values(
        (
            TypedColumn(name="STUDYID", type="str"),
            TypedColumn(name="USUBJID", type="str"),
        ),
        [["S", "a"]],
    )
    right = frame_from_values(
        (
            TypedColumn(name="STUDYID", type="str"),
            TypedColumn(name="USUBJID", type="str"),
            TypedColumn(name="V", type="float"),
            TypedColumn(name="FLAG", type="str"),
        ),
        [["S", "a", 1.0, "N"], ["S", "a", 2.0, "Y"], ["S", "a", 3.0, "Y"]],
    )
    specification = Specification(
        schema_version="1.0",
        domain="OUT",
        input={
            "LEFT": DatasetSource(path="input/left.csv"),
            "RIGHT": DatasetSource(path="input/right.csv"),
        },
        base="LEFT",
        keys=["STUDYID", "USUBJID"],
        output=Output(path="out.csv", columns=["STUDYID", "USUBJID", "V"]),
        columns=[
            Column(
                name="STUDYID",
                type="str",
                derivation=HandledExpression(
                    value=Expression(root={"source": "LEFT.STUDYID"})
                ),
            ),
            Column(
                name="USUBJID",
                type="str",
                derivation=HandledExpression(
                    value=Expression(root={"source": "LEFT.USUBJID"})
                ),
            ),
            Column(
                name="V",
                type="float",
                derivation=HandledExpression(
                    value=Expression(
                        root={
                            "source": {
                                "variable": "RIGHT.V",
                                "filter": "RIGHT.FLAG = 'Y'",
                                "multiple_matches": {
                                    "order_by": ["RIGHT.V"],
                                    "keep": "last",
                                },
                            }
                        }
                    )
                ),
            ),
        ],
    )
    plan_execution(
        specification,
        {"LEFT": left, "RIGHT": right},
        supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
    )
    result = execute_with_source_provider(
        specification, lambda datasets: {"LEFT": left, "RIGHT": right}
    )

    assert isinstance(result, ExecutionSuccess)
    assert result.table.frame.to_dicts() == [{"STUDYID": "S", "USUBJID": "a", "V": 3.0}]
