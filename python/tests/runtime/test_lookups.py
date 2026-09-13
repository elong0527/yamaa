from __future__ import annotations

from yamaa.expressions import parse_predicate
from yamaa.io.polars import frame_from_values
from yamaa.models import MISSING, DateValue, TypedColumn
from yamaa.planning import PlannedRecordLookup
from yamaa.runtime.joins import RelationIndex
from yamaa.runtime.lookups import RecordLookupSelector, types_comparable
from yamaa.specification.models import OrderTerm


def relation(
    name: str,
    columns: list[tuple[str, str]],
    rows: list[list[object]],
) -> RelationIndex:
    typed = tuple(
        TypedColumn(name=column, type=column_type) for column, column_type in columns
    )
    return RelationIndex(name, frame_from_values(typed, rows))


def ex() -> RelationIndex:
    return relation(
        "EX",
        [
            ("STUDYID", "str"),
            ("USUBJID", "str"),
            ("EXSEQ", "int"),
            ("EXTRT", "str"),
            ("EXENDTC", "date"),
        ],
        [
            ["CATH", "S1", 1, "VITAMIN D3", DateValue.parse("2025-01-14")],
            ["CATH", "S1", 2, "PLACEBO", DateValue.parse("2025-01-28")],
            ["CATH", "S1", 3, "RESCUE", MISSING],
            ["CATH", "S2", 1, "VITAMIN D3", DateValue.parse("2025-02-20")],
        ],
    )


def selector(plan: PlannedRecordLookup) -> RecordLookupSelector:
    return RecordLookupSelector([plan], {"EX": ex()})


def on_output_keys(**extra: object) -> PlannedRecordLookup:
    return PlannedRecordLookup(
        identifier="LASTEX",
        dataset="EX",
        path="record_lookups[0]",
        match_variables=("STUDYID", "USUBJID"),
        match_fields=("STUDYID", "USUBJID"),
        on_output_keys=True,
        **extra,
    )


def test_a_filter_and_an_ordered_keep_select_one_record() -> None:
    plan = on_output_keys(
        filter_predicate=parse_predicate("EX.EXENDTC IS NOT NULL"),
        order_terms=(
            (OrderTerm(variable="EX.EXENDTC"), "EXENDTC"),
            (OrderTerm(variable="EX.EXSEQ"), "EXSEQ"),
        ),
        keep="last",
    )

    outcome = selector(plan).select("LASTEX", {"STUDYID": "CATH", "USUBJID": "S1"})

    assert outcome.condition is None
    assert outcome.record is not None
    assert outcome.record.values["EXTRT"] == "PLACEBO"


def test_keeping_first_reads_the_other_end_of_the_same_order() -> None:
    plan = on_output_keys(
        filter_predicate=parse_predicate("EX.EXENDTC IS NOT NULL"),
        order_terms=((OrderTerm(variable="EX.EXENDTC"), "EXENDTC"),),
        keep="first",
    )

    outcome = selector(plan).select("LASTEX", {"STUDYID": "CATH", "USUBJID": "S1"})

    assert outcome.record is not None
    assert outcome.record.values["EXTRT"] == "VITAMIN D3"


def test_a_tie_on_every_term_is_resolved_by_record_order() -> None:
    # R015-7: remaining ties are resolved by record order, which makes the
    # selection total rather than dependent on the sort's stability.
    tied = relation(
        "EX",
        [("USUBJID", "str"), ("EXTRT", "str")],
        [["S1", "FIRST"], ["S1", "SECOND"]],
    )
    plan = PlannedRecordLookup(
        identifier="DOSING",
        dataset="EX",
        path="record_lookups[0]",
        match_variables=("USUBJID",),
        match_fields=("USUBJID",),
        on_output_keys=True,
        order_terms=((OrderTerm(variable="EX.USUBJID"), "USUBJID"),),
        keep="first",
    )

    outcome = RecordLookupSelector([plan], {"EX": tied}).select(
        "DOSING", {"USUBJID": "S1"}
    )

    assert outcome.record is not None
    assert outcome.record.values["EXTRT"] == "FIRST"


def test_several_surviving_records_with_no_order_fail() -> None:
    # R015-28: an unhandled multiple match under R003.
    outcome = selector(on_output_keys()).select(
        "LASTEX", {"STUDYID": "CATH", "USUBJID": "S1"}
    )

    assert outcome.condition is not None
    assert outcome.condition.condition.condition == "multiple_matches"
    assert outcome.condition.condition.requirement == "R015-28"
    assert outcome.condition.condition.context["match_count"] == 3
    assert outcome.spec_path == "record_lookups[0]"


def test_matching_on_output_keys_answers_an_absent_record_with_missing() -> None:
    # R015-20: R003 treats an absent right-side record as ordinary missing.
    outcome = selector(on_output_keys()).select(
        "LASTEX", {"STUDYID": "CATH", "USUBJID": "S9"}
    )

    assert outcome.record is None
    assert outcome.condition is None


def declared(unmatched: str = "fail", **extra: object) -> PlannedRecordLookup:
    return PlannedRecordLookup(
        identifier="REFRANGE",
        dataset="EX",
        path="record_lookups[0]",
        match_variables=("SUBJECT",),
        match_fields=("USUBJID",),
        on_output_keys=False,
        unmatched=unmatched,
        **extra,
    )


def test_a_declared_key_with_no_record_is_fatal_and_names_the_key() -> None:
    # R015-21: R007 makes an unmatched lookup key fatal unless the
    # specification answers for it.
    outcome = selector(declared()).select("REFRANGE", {"SUBJECT": "S9"})

    assert outcome.condition is not None
    assert outcome.condition.condition.condition == "unmatched_key"
    assert outcome.condition.condition.requirement == "R015-18"
    assert outcome.condition.condition.context["lookup_key"] == {"USUBJID": "S9"}


def test_a_declared_unmatched_answer_replaces_the_failure() -> None:
    outcome = selector(declared(unmatched="missing")).select(
        "REFRANGE", {"SUBJECT": "S9"}
    )

    assert outcome.condition is None
    assert outcome.record is None


def test_a_missing_match_value_is_answered_before_a_record_is_looked_for() -> None:
    # R015-16 and R015-17: the two absences stay disjoint, and an incomplete
    # value defaults to failing rather than reporting an absent record.
    fatal = selector(declared()).select("REFRANGE", {"SUBJECT": MISSING})
    answered = selector(declared(incomplete="missing")).select(
        "REFRANGE", {"SUBJECT": MISSING}
    )

    assert fatal.condition is not None
    assert fatal.condition.condition.condition == "incomplete_match_value"
    assert fatal.condition.condition.requirement == "R015-17"
    assert fatal.condition.condition.context["missing_source"] == "SUBJECT"
    assert fatal.spec_path == "record_lookups[0].source"
    assert answered.condition is None
    assert answered.record is None


def epochs() -> RelationIndex:
    return relation(
        "EPOCHS",
        [("STUDYID", "str"), ("EPOCH", "str"), ("LO", "int"), ("HI", "int")],
        [
            ["CATH", "SCREENING", -14, 0],
            ["CATH", "TREATMENT", 1, 56],
            ["CATH", "OPEN", 57, MISSING],
        ],
    )


def between(**extra: object) -> PlannedRecordLookup:
    return PlannedRecordLookup(
        identifier="EPOCHDEF",
        dataset="EPOCHS",
        path="record_lookups[0]",
        match_variables=("STUDYID",),
        match_fields=("STUDYID",),
        on_output_keys=False,
        between_value="ADY",
        between_lower="LO",
        between_upper="HI",
        unmatched=extra.pop("unmatched", "missing"),
        **extra,
    )


def epoch_selector() -> RecordLookupSelector:
    return RecordLookupSelector([between()], {"EPOCHS": epochs()})


def test_a_closed_range_includes_both_stated_endpoints() -> None:
    chosen = epoch_selector().select("EPOCHDEF", {"STUDYID": "CATH", "ADY": 56})
    lower = epoch_selector().select("EPOCHDEF", {"STUDYID": "CATH", "ADY": 1})

    assert chosen.record is not None
    assert chosen.record.values["EPOCH"] == "TREATMENT"
    assert lower.record is not None
    assert lower.record.values["EPOCH"] == "TREATMENT"


def test_a_record_missing_a_stated_bound_is_ineligible() -> None:
    # R015-12: an open range is not admitted by omission of the value.
    outcome = epoch_selector().select("EPOCHDEF", {"STUDYID": "CATH", "ADY": 60})

    assert outcome.record is None
    assert outcome.condition is None


def test_a_missing_range_value_is_an_incomplete_match_not_an_absent_record() -> None:
    plan = between(incomplete="fail")

    outcome = RecordLookupSelector([plan], {"EPOCHS": epochs()}).select(
        "EPOCHDEF", {"STUDYID": "CATH", "ADY": MISSING}
    )

    assert outcome.condition is not None
    assert outcome.condition.condition.condition == "incomplete_match_value"
    assert outcome.spec_path == "record_lookups[0].between.value"


def test_declared_types_decide_whether_a_range_can_be_compared() -> None:
    # R015-11: int and float compare through R010's promotion; every other
    # type must match exactly.
    assert types_comparable("int", "float")
    assert types_comparable("date", "date")
    assert not types_comparable("date", "int")
    assert not types_comparable("str", "int")
