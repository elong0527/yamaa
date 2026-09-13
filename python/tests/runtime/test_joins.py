from __future__ import annotations

import pytest

from yamaa.expressions import FailedResolution, ResolvedValue, parse_predicate
from yamaa.io.polars import frame_from_values
from yamaa.models import MISSING, ConditionResult, DateValue, TypedColumn, ValueResult
from yamaa.runtime.joins import (
    RelationIndex,
    applicable_keys,
    compare_values,
    eligible_records,
    evaluate_mapping_from,
    join_scalar,
    order_records,
    partition_records,
    resolution_result,
)
from yamaa.specification.models import OrderTerm

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
    # R003-3 and R003-6: the keys, in output `keys` order.
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
    # R003-13: an uncollected identifier is not an identity two rows share.
    index = relation(
        "EX",
        [("USUBJID", "str"), ("EXTRT", "str")],
        [["S1", "A"], [MISSING, "B"]],
    )

    assert index.matching(("USUBJID",), ("S1",)) != ()
    assert index.matching(("USUBJID",), (MISSING,)) == ()


def test_partitions_keep_first_occurrence_order_and_group_missing_together() -> None:
    # R001-7 and R001-8: missing equals missing for grouping, and a group is
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
    # R007-15: `nulls` does not flip with `direction`.
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
    # R003-11 and R003-36: an absent right-side record is ordinary missing.
    assert isinstance(absent, ResolvedValue)
    assert absent.value is MISSING


def test_several_matches_fail_unless_the_specification_answers_for_them() -> None:
    index = ex_relation()

    result = join_scalar(index, ("STUDYID", "USUBJID"), ("CATH", "S1"), "EXTRT")

    assert isinstance(result, FailedResolution)
    assert result.condition.condition == "multiple_matches"
    assert result.condition.requirement == "R003-35"
    assert result.condition.context == {"dataset": "EX", "match_count": 2}


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
    # R008-15: the count reports only the rows where more than one survived.
    index = ex_relation()

    result = join_scalar(
        index,
        ("STUDYID", "USUBJID"),
        ("CATH", "S1"),
        "EXTRT",
        multiple_matches={
            "order_by": [ORDER_BY],
            "keep": "last",
            "filter": "EX.EXSEQ = 1",
        },
    )

    assert isinstance(result, ResolvedValue)
    assert result.value == "VITAMIN D3"
    assert result.handled_by is None


def test_a_selection_filtered_to_nothing_is_an_ordinary_absent_match() -> None:
    # R008-14: a narrow filter produces missing rather than firing a handler.
    index = ex_relation()

    result = join_scalar(
        index,
        ("STUDYID", "USUBJID"),
        ("CATH", "S1"),
        "EXTRT",
        multiple_matches={
            "order_by": [ORDER_BY],
            "keep": "last",
            "filter": "EX.EXSEQ = 9",
        },
    )

    assert isinstance(result, ResolvedValue)
    assert result.value is MISSING


def lbref() -> RelationIndex:
    return relation(
        "LBREF",
        [("LBTESTCD", "str"), ("SEX", "str"), ("ANRHI", "float")],
        [["ALT", "F", 33.0], ["ALT", "M", 41.0], ["AST", "F", 32.0]],
    )


def mapping_from(values: dict[str, object], **extra: object) -> object:
    payload = {
        "source": ["PARAMCD", "SEX"],
        "dataset": "LBREF",
        "key": ["LBTESTCD", "SEX"],
        "value": "ANRHI",
        **extra,
    }
    return evaluate_mapping_from(payload, lbref(), values)


def test_declared_key_pairs_reach_a_right_side_keyed_on_something_else() -> None:
    result = mapping_from({"PARAMCD": "ALT", "SEX": "M"})

    assert isinstance(result, ValueResult)
    assert result.value == 41.0


def test_an_incomplete_key_never_reaches_the_unmapped_handler() -> None:
    # R008-9: with several inputs the two conditions stay disjoint.
    unhandled = mapping_from({"PARAMCD": "ALT", "SEX": MISSING})
    handled = mapping_from({"PARAMCD": "ALT", "SEX": MISSING}, missing=0.0)
    wrong_handler = mapping_from({"PARAMCD": "ALT", "SEX": MISSING}, unmapped=1.0)

    assert isinstance(unhandled, ConditionResult)
    assert unhandled.condition.condition == "missing_input"
    assert unhandled.condition.context["missing_source"] == "SEX"
    assert isinstance(handled, ValueResult)
    assert handled.value == 0.0
    assert handled.handled_by == "missing"
    assert isinstance(wrong_handler, ConditionResult)


def test_a_complete_key_with_no_entry_is_the_unmapped_condition() -> None:
    unhandled = mapping_from({"PARAMCD": "AST", "SEX": "M"})
    handled = mapping_from({"PARAMCD": "AST", "SEX": "M"}, unmapped=None)

    assert isinstance(unhandled, ConditionResult)
    assert unhandled.condition.condition == "unmapped_key"
    assert unhandled.condition.context["lookup_key"] == {
        "LBTESTCD": "AST",
        "SEX": "M",
    }
    assert isinstance(handled, ValueResult)
    assert handled.value is MISSING


def test_a_duplicate_lookup_key_fails_rather_than_taking_file_order() -> None:
    duplicated = relation(
        "LBREF",
        [("LBTESTCD", "str"), ("SEX", "str"), ("ANRHI", "float")],
        [["ALT", "F", 33.0], ["ALT", "F", 35.0]],
    )

    result = evaluate_mapping_from(
        {
            "source": ["PARAMCD", "SEX"],
            "dataset": "LBREF",
            "key": ["LBTESTCD", "SEX"],
            "value": "ANRHI",
        },
        duplicated,
        {"PARAMCD": "ALT", "SEX": "F"},
    )

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "duplicate_lookup_key"
    assert result.condition.context["match_count"] == 2


def test_unequal_source_and_key_lists_name_no_key() -> None:
    result = evaluate_mapping_from(
        {
            "source": ["PARAMCD", "SEX"],
            "dataset": "LBREF",
            "key": ["LBTESTCD"],
            "value": "ANRHI",
        },
        lbref(),
        {"PARAMCD": "ALT", "SEX": "F"},
    )

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "source_key_length_mismatch"
    assert result.condition.requirement == "R007-48"


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
