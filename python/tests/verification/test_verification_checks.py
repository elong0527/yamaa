"""Completed-table verification: column checks, keys, and dataset checks."""

from __future__ import annotations

import pytest

from yamaa.io.polars import frame_from_values
from yamaa.models import TypedColumn
from yamaa.specification.models import Column as SpecColumn
from yamaa.specification.models import Expression, OrderTerm, Output
from yamaa.verification import (
    VerificationError,
    finalize_output,
    verify_columns,
    verify_dataset,
)

KEYS = ["STUDYID", "USUBJID"]

BASE_COLUMNS = (
    TypedColumn(name="STUDYID", type="str"),
    TypedColumn(name="USUBJID", type="str"),
    TypedColumn(name="AGE", type="int"),
    TypedColumn(name="SEX", type="str"),
)

BASE_ROWS = [
    ["PILOT7", "P7-731", 34, "M"],
    ["PILOT7", "P7-732", 28, "F"],
]


def _check(error: VerificationError, index: int = 0) -> dict:
    return error.failures[index].model_dump(mode="python")


def test_not_missing_reports_offending_keys() -> None:
    table = frame_from_values(
        BASE_COLUMNS,
        [
            ["PILOT7", "P7-731", 34, "M"],
            ["PILOT7", "P7-732", None, "F"],
        ],
    )
    columns = [
        SpecColumn(
            name="AGE", type="int", verifications=[Expression({"not_missing": {}})]
        )
    ]

    with pytest.raises(VerificationError) as raised:
        verify_columns(table, columns, KEYS)

    assert _check(raised.value) == {
        "phase": "verification",
        "condition": "not_missing_failed",
        "spec_paths": ("columns.AGE.verifications[0].not_missing",),
        "requirement": "R009-9",
        "context": {
            "column": "AGE",
            "failure_count": 1,
            "keys": [{"STUDYID": "PILOT7", "USUBJID": "P7-732"}],
        },
    }


def test_range_failure_matches_the_committed_identity() -> None:
    table = frame_from_values(
        BASE_COLUMNS,
        [
            ["PILOT7", "P7-731", 34, "M"],
            ["PILOT7", "P7-732", 251, "F"],
        ],
    )
    columns = [
        SpecColumn(
            name="AGE",
            type="int",
            verifications=[Expression({"range": {"min": 0, "max": 150}})],
        )
    ]

    with pytest.raises(VerificationError) as raised:
        verify_columns(table, columns, KEYS)

    assert _check(raised.value) == {
        "phase": "verification",
        "condition": "range_failed",
        "spec_paths": ("columns.AGE.verifications[0].range",),
        "requirement": "R009-11",
        "context": {
            "column": "AGE",
            "failure_count": 1,
            "keys": [{"STUDYID": "PILOT7", "USUBJID": "P7-732"}],
        },
    }


def test_allowed_values_lets_missing_pass() -> None:
    table = frame_from_values(BASE_COLUMNS, BASE_ROWS)
    columns = [
        SpecColumn(
            name="SEX",
            type="str",
            verifications=[Expression({"allowed_values": {"values": ["M", "F"]}})],
        )
    ]

    assert verify_columns(table, columns, KEYS).frame.height == 2


def test_max_length_and_matches_use_search_semantics() -> None:
    table = frame_from_values(
        BASE_COLUMNS,
        [
            ["PILOT7", "P7-731", 34, "Male"],
            ["PILOT7", "P7-732", 28, "F"],
        ],
    )
    columns = [
        SpecColumn(
            name="SEX",
            type="str",
            verifications=[Expression({"max_length": {"max": 2}})],
        ),
        SpecColumn(
            name="SEX",
            type="str",
            verifications=[Expression({"matches": {"pattern": "^[MF]$"}})],
        ),
    ]

    with pytest.raises(VerificationError) as raised:
        verify_columns(table, columns, KEYS)

    assert [failure.condition for failure in raised.value.failures] == [
        "length_failed",
        "matches_failed",
    ]
    assert _check(raised.value)["context"] == {
        "column": "SEX",
        "max": 2,
        "failure_count": 1,
        "keys": [{"STUDYID": "PILOT7", "USUBJID": "P7-731"}],
    }


def test_matches_uses_the_pinned_ecma_regex_engine() -> None:
    table = frame_from_values(
        (TypedColumn(name="VALUE", type="str"),),
        [["\u0661"]],
    )
    ascii_digit = [
        SpecColumn(
            name="VALUE",
            type="str",
            verifications=[Expression({"matches": {"pattern": r"^\d$"}})],
        )
    ]

    with pytest.raises(VerificationError) as raised:
        verify_columns(table, ascii_digit, ["VALUE"])
    assert _check(raised.value)["condition"] == "matches_failed"

    unicode_letter = [
        SpecColumn(
            name="VALUE",
            type="str",
            verifications=[Expression({"matches": {"pattern": r"^\p{L}+$"}})],
        )
    ]
    assert (
        verify_columns(
            frame_from_values((TypedColumn(name="VALUE", type="str"),), [["e"]]),
            unicode_letter,
            ["VALUE"],
        ).frame.height
        == 1
    )


def test_missing_key_reports_position_and_count() -> None:
    table = frame_from_values(
        (
            TypedColumn(name="STUDYID", type="str"),
            TypedColumn(name="USUBJID", type="str"),
            TypedColumn(name="PARAMCD", type="str"),
            TypedColumn(name="AVISIT", type="str"),
        ),
        [["CATH", "CATH-UCSD-0002", "SYSBP", None]],
    )

    with pytest.raises(VerificationError) as raised:
        verify_dataset(table, ["STUDYID", "USUBJID", "PARAMCD", "AVISIT"], [])

    assert _check(raised.value) == {
        "phase": "output",
        "condition": "missing_key",
        "spec_paths": ("keys[3]",),
        "requirement": "R005-52",
        "context": {
            "column": "AVISIT",
            "missing_count": 1,
            "keys": [
                {
                    "STUDYID": "CATH",
                    "USUBJID": "CATH-UCSD-0002",
                    "PARAMCD": "SYSBP",
                    "AVISIT": None,
                }
            ],
        },
    }


def test_duplicate_key_reports_each_key_once() -> None:
    table = frame_from_values(
        BASE_COLUMNS,
        [
            ["PILOT7", "P7-722", 34, "M"],
            ["PILOT7", "P7-722", 35, "M"],
        ],
    )

    with pytest.raises(VerificationError) as raised:
        verify_dataset(table, KEYS, [])

    assert _check(raised.value) == {
        "phase": "output",
        "condition": "duplicate_key",
        "spec_paths": ("keys",),
        "requirement": "R005-52",
        "context": {
            "duplicate_count": 1,
            "keys": [{"STUDYID": "PILOT7", "USUBJID": "P7-722"}],
        },
    }


def test_unique_counts_missing_as_a_value() -> None:
    table = frame_from_values(
        BASE_COLUMNS,
        [
            ["PILOT7", "P7-941", 34, None],
            ["PILOT7", "P7-942", 28, None],
        ],
    )

    with pytest.raises(VerificationError) as raised:
        verify_dataset(table, KEYS, [Expression({"unique": {"columns": ["SEX"]}})])

    assert _check(raised.value) == {
        "phase": "verification",
        "condition": "unique_failed",
        "spec_paths": ("verifications[0].unique",),
        "requirement": "R009-15",
        "context": {
            "columns": ["SEX"],
            "failure_count": 1,
            "keys": [
                {"STUDYID": "PILOT7", "USUBJID": "P7-941"},
                {"STUDYID": "PILOT7", "USUBJID": "P7-942"},
            ],
        },
    }


def test_all_or_none_needs_two_distinct_columns() -> None:
    table = frame_from_values(BASE_COLUMNS, BASE_ROWS)

    with pytest.raises(ValueError):
        verify_dataset(
            table, KEYS, [Expression({"all_or_none": {"id": "x", "columns": ["SEX"]}})]
        )


def test_implies_and_predicate_follow_three_valued_logic() -> None:
    table = frame_from_values(
        BASE_COLUMNS,
        [
            ["PILOT7", "P7-731", 34, "M"],
            ["PILOT7", "P7-732", None, "F"],
        ],
    )

    assert (
        verify_dataset(
            table,
            KEYS,
            [
                Expression(
                    {"implies": {"id": "x", "when": "AGE > 100", "then": "SEX = 'X'"}}
                )
            ],
        ).frame.height
        == 2
    )

    with pytest.raises(VerificationError) as raised:
        verify_dataset(
            table,
            KEYS,
            [Expression({"predicate": {"id": "y", "assert": "AGE > 30"}})],
        )
    assert _check(raised.value)["condition"] == "predicate_failed"


def test_row_count_bounds_groups_and_filters() -> None:
    table = frame_from_values(
        BASE_COLUMNS,
        [
            ["PILOT7", "P7-731", 34, "M"],
            ["PILOT7", "P7-732", 35, "M"],
            ["PILOT7", "P7-733", 28, "F"],
        ],
    )

    with pytest.raises(VerificationError) as raised:
        verify_dataset(
            table,
            KEYS,
            [
                Expression(
                    {
                        "row_count": {
                            "id": "one-per-sex",
                            "group_by": ["SEX"],
                            "min": 1,
                            "max": 1,
                        }
                    }
                )
            ],
        )

    failure = _check(raised.value)
    assert failure["condition"] == "row_count_failed"
    assert failure["context"] == {
        "verification_id": "one-per-sex",
        "failure_count": 1,
        "keys": [{"SEX": "M"}],
        "count": 2,
    }


def test_filtered_row_count_keeps_zero_count_groups() -> None:
    table = frame_from_values(
        BASE_COLUMNS,
        [
            ["PILOT7", "P7-731", 34, "M"],
            ["PILOT7", "P7-732", 35, "M"],
            ["PILOT7", "P7-733", 28, "F"],
        ],
    )

    with pytest.raises(VerificationError) as raised:
        verify_dataset(
            table,
            KEYS,
            [
                Expression(
                    {
                        "row_count": {
                            "id": "older-subject-per-sex",
                            "group_by": ["SEX"],
                            "filter": "AGE > 100",
                            "min": 1,
                        }
                    }
                )
            ],
        )

    assert _check(raised.value)["context"] == {
        "verification_id": "older-subject-per-sex",
        "failure_count": 2,
        "keys": [{"SEX": "M"}, {"SEX": "F"}],
        "counts": [0, 0],
    }


def test_ungrouped_row_count_checks_an_empty_table() -> None:
    table = frame_from_values(BASE_COLUMNS, [])

    with pytest.raises(VerificationError) as raised:
        verify_dataset(
            table,
            KEYS,
            [Expression({"row_count": {"min": 1}})],
        )

    assert _check(raised.value)["context"] == {
        "failure_count": 1,
        "count": 0,
    }


def test_empty_table_uses_its_declared_schema_for_keys_and_references() -> None:
    table = frame_from_values(BASE_COLUMNS, [])

    assert (
        verify_dataset(
            table,
            KEYS,
            [Expression({"unique": {"columns": ["SEX"]}})],
        ).frame.height
        == 0
    )

    with pytest.raises(VerificationError) as raised:
        verify_dataset(
            table,
            KEYS,
            [Expression({"predicate": {"id": "known-field", "assert": "ABSENT = 1"}})],
        )
    assert _check(raised.value)["condition"] == "unknown_field"


def test_predicate_evaluation_conditions_are_not_treated_as_unknown() -> None:
    table = frame_from_values(BASE_COLUMNS, BASE_ROWS)

    with pytest.raises(VerificationError) as raised:
        verify_dataset(
            table,
            KEYS,
            [Expression({"predicate": {"id": "typed-rule", "assert": "SEX > 1"}})],
        )

    failure = _check(raised.value)
    assert failure["phase"] == "validation"
    assert failure["condition"] == "incompatible_input_type"
    assert failure["spec_paths"] == ("verifications[0].predicate.assert",)
    assert failure["requirement"] == "R004-33"


def test_rejects_reversed_range_and_duplicate_verification_ids() -> None:
    table = frame_from_values(BASE_COLUMNS, [])
    columns = [
        SpecColumn(
            name="AGE",
            type="int",
            verifications=[Expression({"range": {"min": 10, "max": 1}})],
        )
    ]
    with pytest.raises(ValueError, match="range min"):
        verify_columns(table, columns, KEYS)

    duplicate_ids = [
        Expression({"predicate": {"id": "same", "assert": "TRUE"}}),
        Expression({"implies": {"id": "same", "when": "TRUE", "then": "TRUE"}}),
    ]
    with pytest.raises(ValueError, match="duplicate dataset verification id"):
        verify_dataset(table, KEYS, duplicate_ids)


def test_failure_reports_the_dataset_verification_id() -> None:
    table = frame_from_values(BASE_COLUMNS, BASE_ROWS)

    with pytest.raises(VerificationError) as raised:
        verify_dataset(
            table,
            KEYS,
            [Expression({"predicate": {"id": "adult-only", "assert": "AGE > 30"}})],
        )

    assert _check(raised.value)["context"] == {
        "verification_id": "adult-only",
        "failure_count": 1,
        "keys": [{"STUDYID": "PILOT7", "USUBJID": "P7-732"}],
    }


def test_finalize_output_validates_membership_and_orders_stably() -> None:
    columns = (
        TypedColumn(name="ID", type="str"),
        TypedColumn(name="SORT", type="int"),
        TypedColumn(name="VALUE", type="str"),
    )
    table = frame_from_values(
        columns,
        [
            ["a", 2, "b"],
            ["b", None, "a"],
            ["c", 2, "a"],
            ["e", 2, "a"],
            ["d", 1, "z"],
        ],
    )
    output = Output(
        path="values.csv",
        columns=["ID", "VALUE"],
        order_by=[
            OrderTerm(variable="SORT", direction="desc", nulls="first"),
            OrderTerm(variable="VALUE"),
        ],
    )

    ordered = finalize_output(table, output, ["ID"], [])

    assert ordered.frame["ID"].to_list() == ["b", "c", "e", "a", "d"]
    with pytest.raises(ValueError, match="not an output column"):
        finalize_output(table, output, ["SORT"], [])


def test_declaration_violations_raise_before_evaluation() -> None:
    table = frame_from_values(BASE_COLUMNS, BASE_ROWS)

    with pytest.raises(ValueError):
        verify_dataset(table, KEYS, [Expression({"unique": {"columns": ["ABSENT"]}})])
    with pytest.raises(ValueError):
        verify_dataset(table, ["STUDYID", "ABSENT"], [])
