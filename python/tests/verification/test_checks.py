from __future__ import annotations

from pathlib import Path
from typing import Any

import pytest
import yaml

from yamaa.io.polars import frame_from_values
from yamaa.models import DateValue, TypedColumn, TypedTable
from yamaa.specification.models import Column, ColumnType, Expression
from yamaa.verification import (
    DeclarationError,
    VerificationError,
    VerificationFailure,
    check_column,
    check_dataset,
    check_keys,
    verify_completed_table,
)

EXAMPLES = Path(__file__).parents[3] / "yaml" / "examples"


def table(
    declared: list[tuple[str, ColumnType]], rows: list[list[object]]
) -> TypedTable:
    columns = tuple(TypedColumn(name=name, type=kind) for name, kind in declared)
    return frame_from_values(columns, rows)


def column(name: str, kind: ColumnType, *verifications: dict[str, Any]) -> Column:
    return Column(
        name=name,
        type=kind,
        verifications=[Expression(root=item) for item in verifications],
    )


def committed(example: str) -> dict[str, Any]:
    document = yaml.safe_load(
        (EXAMPLES / example / "expected" / "error.yaml").read_text(encoding="ascii")
    )
    return document


def reported(failure: VerificationFailure) -> dict[str, Any]:
    payload = failure.model_dump()
    payload["spec_paths"] = list(payload["spec_paths"])
    return payload


KEYS = ["STUDYID", "USUBJID"]


def test_range_failure_reproduces_the_committed_error_contract() -> None:
    completed = table(
        [("STUDYID", "str"), ("USUBJID", "str"), ("AGE", "int")],
        [["PILOT7", "P7-731", 64], ["PILOT7", "P7-732", 214]],
    )

    failures = check_column(
        completed, column("AGE", "int", {"range": {"min": 18, "max": 100}}), KEYS
    )

    assert len(failures) == 1
    assert reported(failures[0]) == committed("negative-verification-implausible-age")


def test_not_missing_failure_reproduces_the_committed_error_contract() -> None:
    completed = table(
        [("STUDYID", "str"), ("USUBJID", "str"), ("AGE", "int")],
        [["PILOT7", "P7-811", 54], ["PILOT7", "P7-812", None]],
    )

    failures = check_column(completed, column("AGE", "int", {"not_missing": {}}), KEYS)

    assert reported(failures[0]) == committed("negative-not-missing-absent-age")


def test_allowed_values_failure_reproduces_the_committed_error_contract() -> None:
    completed = table(
        [("STUDYID", "str"), ("USUBJID", "str"), ("SEX", "str")],
        [["PILOT7", "P7-951", "M"], ["PILOT7", "P7-952", "X"]],
    )

    failures = check_column(
        completed,
        column("SEX", "str", {"allowed_values": {"values": ["M", "F", "U"]}}),
        KEYS,
    )

    assert reported(failures[0]) == committed("negative-allowed-values-mismatch")


def test_max_length_failure_reproduces_the_committed_error_contract() -> None:
    completed = table(
        [("STUDYID", "str"), ("USUBJID", "str")],
        [
            ["CATH", "CATH-UCSD-0001"],
            ["CATH", "CATH-MCGILL-0002"],
            ["CATH", "CATH-STMARYSMONTREAL-0003"],
        ],
    )

    failures = check_column(
        completed, column("USUBJID", "str", {"max_length": {"max": 20}}), KEYS
    )

    assert reported(failures[0]) == committed("negative-usubjid-exceeds-length")


def test_unique_failure_reports_every_row_carrying_one_repeated_value() -> None:
    completed = table(
        [("STUDYID", "str"), ("USUBJID", "str"), ("SITEID", "str")],
        [["PILOT7", "P7-941", "101"], ["PILOT7", "P7-942", "101"]],
    )

    failures = check_dataset(
        completed, [Expression(root={"unique": {"columns": ["SITEID"]}})], KEYS
    )

    assert reported(failures[0]) == committed("negative-unique-duplicate-values")


def test_all_or_none_failure_reproduces_the_committed_error_contract() -> None:
    completed = table(
        [
            ("STUDYID", "str"),
            ("USUBJID", "str"),
            ("RFSTD", "date"),
            ("DTHDT", "date"),
        ],
        [
            [
                "PILOT7",
                "P7-811",
                DateValue.parse("2023-01-02"),
                DateValue.parse("2023-05-14"),
            ],
            ["PILOT7", "P7-812", DateValue.parse("2023-02-10"), None],
        ],
    )

    failures = check_dataset(
        completed,
        [
            Expression(
                root={
                    "all_or_none": {
                        "id": "paired-death-dates",
                        "columns": ["RFSTD", "DTHDT"],
                    }
                }
            )
        ],
        KEYS,
    )

    assert reported(failures[0]) == committed("negative-all-or-none-partial-row")


def test_key_validation_reproduces_the_committed_missing_and_duplicate_contracts() -> (
    None
):
    missing = table(
        [
            ("STUDYID", "str"),
            ("USUBJID", "str"),
            ("PARAMCD", "str"),
            ("AVISIT", "str"),
        ],
        [
            ["CATH", "CATH-UCSD-0001", "SYSBP", "Day 1"],
            ["CATH", "CATH-UCSD-0002", "SYSBP", None],
        ],
    )
    duplicate = table(
        [("STUDYID", "str"), ("USUBJID", "str")],
        [["PILOT7", "P7-721"], ["PILOT7", "P7-722"], ["PILOT7", "P7-722"]],
    )

    missing_failures = check_keys(missing, ["STUDYID", "USUBJID", "PARAMCD", "AVISIT"])
    duplicate_failures = check_keys(duplicate, KEYS)

    assert reported(missing_failures[0]) == committed("negative-keys-missing-value")
    assert reported(duplicate_failures[0]) == committed(
        "negative-output-duplicate-subject"
    )


def test_a_collected_empty_string_is_a_value_rather_than_a_missing_one() -> None:
    completed = table(
        [("STUDYID", "str"), ("USUBJID", "str"), ("CMNT", "str")],
        [["S", "", "kept"], ["S", "S-2", ""], ["S", "S-3", None]],
    )

    # An empty key value was collected, so output identity holds.
    assert check_keys(completed, KEYS) == ()

    failures = check_column(completed, column("CMNT", "str", {"not_missing": {}}), KEYS)
    assert [failure.context["keys"] for failure in failures] == [
        [{"STUDYID": "S", "USUBJID": "S-3"}]
    ]
    # An empty string and a missing value are two values, not one repeated.
    assert (
        check_dataset(
            completed, [Expression(root={"unique": {"columns": ["CMNT"]}})], KEYS
        )
        == ()
    )


def test_implies_and_predicate_report_their_business_rule_identity() -> None:
    completed = table(
        [("STUDYID", "str"), ("USUBJID", "str"), ("CMNT", "str"), ("CMNTFL", "str")],
        [
            ["CTX", "CTX-01", "kept", "Y"],
            ["CTX", "CTX-02", "present", "N"],
        ],
    )

    failures = check_dataset(
        completed,
        [
            Expression(
                root={
                    "implies": {
                        "id": "flag-follows-collection",
                        "when": "CMNTFL = 'N'",
                        "then": "CMNT IS NULL",
                    }
                }
            ),
            Expression(
                root={
                    "predicate": {
                        "id": "flag-is-yes-or-no",
                        "assert": "CMNTFL IN ('Y')",
                    }
                }
            ),
        ],
        KEYS,
    )

    assert [failure.condition for failure in failures] == [
        "implication_failed",
        "predicate_failed",
    ]
    assert failures[0].context["verification_id"] == "flag-follows-collection"
    assert failures[0].context["keys"] == [{"STUDYID": "CTX", "USUBJID": "CTX-02"}]
    assert failures[1].context["verification_id"] == "flag-is-yes-or-no"
    assert failures[1].spec_paths == ("verifications[1].predicate",)


def test_grouped_row_count_keeps_a_group_whose_filter_admits_no_row() -> None:
    completed = table(
        [("STUDYID", "str"), ("USUBJID", "str"), ("ABLFL", "str")],
        [
            ["CATH", "CATH-UCSD-0001", "Y"],
            ["CATH", "CATH-UCSD-0001", "Y"],
            ["CATH", "CATH-UCSD-0002", None],
        ],
    )

    failures = check_dataset(
        completed,
        [
            Expression(
                root={
                    "row_count": {
                        "id": "one-baseline-record-per-subject",
                        "group_by": ["STUDYID", "USUBJID"],
                        "filter": "ABLFL = 'Y'",
                        "min": 1,
                        "max": 1,
                    }
                }
            )
        ],
        KEYS,
    )

    assert failures[0].condition == "row_count_failed"
    assert failures[0].context["failure_count"] == 2
    assert failures[0].context["keys"] == [
        {"STUDYID": "CATH", "USUBJID": "CATH-UCSD-0001"},
        {"STUDYID": "CATH", "USUBJID": "CATH-UCSD-0002"},
    ]
    assert failures[0].context["count"] == 2


def test_grouped_row_count_failure_reproduces_the_committed_error_contract() -> None:
    completed = table(
        [
            ("STUDYID", "str"),
            ("USUBJID", "str"),
            ("PARAMCD", "str"),
            ("ADT", "date"),
            ("ABLFL", "str"),
        ],
        [
            ["CATH", "CATH-UCSD-0001", "ALT", DateValue.parse("2025-01-01"), "Y"],
            ["CATH", "CATH-UCSD-0001", "ALT", DateValue.parse("2025-01-02"), "Y"],
            ["CATH", "CATH-UCSD-0001", "ALT", DateValue.parse("2025-01-08"), None],
            ["CATH", "CATH-UCSD-0002", "ALT", DateValue.parse("2025-01-01"), "Y"],
            ["CATH", "CATH-UCSD-0002", "ALT", DateValue.parse("2025-01-08"), None],
        ],
    )

    failures = check_dataset(
        completed,
        [
            Expression(
                root={"unique": {"columns": ["STUDYID", "USUBJID", "PARAMCD", "ADT"]}}
            ),
            Expression(
                root={
                    "row_count": {
                        "id": "one-baseline-record-per-subject-and-parameter",
                        "group_by": ["STUDYID", "USUBJID", "PARAMCD"],
                        "filter": "ABLFL = 'Y'",
                        "min": 1,
                        "max": 1,
                    }
                }
            ),
        ],
        ["STUDYID", "USUBJID", "PARAMCD", "ADT"],
    )

    assert len(failures) == 1
    assert reported(failures[0]) == committed("negative-adlb-multiple-baseline-records")


def test_ungrouped_row_count_fails_a_minimum_on_an_empty_artifact() -> None:
    empty = table([("STUDYID", "str"), ("USUBJID", "str")], [])

    failures = check_dataset(empty, [Expression(root={"row_count": {"min": 1}})], KEYS)

    assert failures[0].condition == "row_count_failed"
    assert failures[0].context["count"] == 0
    assert failures[0].context["keys"] == [{}]


def test_header_only_artifact_passes_key_validation_and_column_checks() -> None:
    empty = table([("STUDYID", "str"), ("USUBJID", "str"), ("AGE", "int")], [])

    assert check_keys(empty, KEYS) == ()
    assert check_column(empty, column("AGE", "int", {"not_missing": {}}), KEYS) == ()


def test_matches_searches_with_the_pinned_engine_rather_than_a_host_dialect() -> None:
    completed = table(
        [("STUDYID", "str"), ("USUBJID", "str"), ("CODE", "str")],
        [["S", "S-1", "\u0665"], ["S", "S-2", "5"]],
    )

    failures = check_column(
        completed, column("CODE", "str", {"matches": {"pattern": r"^\d$"}}), KEYS
    )

    assert failures[0].condition == "matches_failed"
    assert failures[0].requirement == "R009-13"
    assert failures[0].context["keys"] == [{"STUDYID": "S", "USUBJID": "S-1"}]


def test_matches_admits_a_unicode_property_pattern_and_searches_anywhere() -> None:
    completed = table(
        [("STUDYID", "str"), ("USUBJID", "str"), ("TEXT", "str")],
        [["S", "S-1", "12ab"], ["S", "S-2", "1234"]],
    )

    failures = check_column(
        completed, column("TEXT", "str", {"matches": {"pattern": r"\p{L}+"}}), KEYS
    )

    assert failures[0].context["keys"] == [{"STUDYID": "S", "USUBJID": "S-2"}]


def test_max_length_counts_one_supplementary_plane_scalar_once() -> None:
    completed = table(
        [("STUDYID", "str"), ("USUBJID", "str"), ("TEXT", "str")],
        [["S", "S-1", "\U0001d400\U0001d401"]],
    )

    assert (
        check_column(completed, column("TEXT", "str", {"max_length": {"max": 2}}), KEYS)
        == ()
    )
    assert (
        len(
            check_column(
                completed, column("TEXT", "str", {"max_length": {"max": 1}}), KEYS
            )
        )
        == 1
    )


def test_allowed_values_and_missing_values_follow_their_declared_type() -> None:
    completed = table(
        [("STUDYID", "str"), ("USUBJID", "str"), ("FLAG", "int")],
        [["S", "S-1", 1], ["S", "S-2", None]],
    )

    assert (
        check_column(
            completed, column("FLAG", "int", {"allowed_values": {"values": [1]}}), KEYS
        )
        == ()
    )
    with pytest.raises(DeclarationError) as raised:
        check_column(
            completed,
            column("FLAG", "int", {"allowed_values": {"values": [True]}}),
            KEYS,
        )
    assert raised.value.requirement == "R009-30"


def test_declaration_defects_are_refused_rather_than_reported_as_data_failures() -> (
    None
):
    completed = table(
        [("STUDYID", "str"), ("USUBJID", "str"), ("AGE", "int")],
        [["S", "S-1", 50]],
    )
    empty = table([("STUDYID", "str"), ("USUBJID", "str"), ("AGE", "int")], [])

    for candidate in (completed, empty):
        with pytest.raises(DeclarationError) as reversed_range:
            check_column(
                candidate,
                column("AGE", "int", {"range": {"min": 100, "max": 18}}),
                KEYS,
            )
        assert reversed_range.value.requirement == "R009-25"

    with pytest.raises(DeclarationError) as no_bound:
        check_dataset(completed, [Expression(root={"row_count": {}})], KEYS)
    assert no_bound.value.requirement == "R009-25"

    with pytest.raises(DeclarationError) as grouped:
        check_dataset(
            completed,
            [Expression(root={"row_count": {"group_by": ["STUDYID"], "min": 1}})],
            KEYS,
        )
    assert grouped.value.condition == "missing_verification_id"
    assert grouped.value.requirement == "R009-28"

    with pytest.raises(DeclarationError) as unknown:
        check_dataset(
            completed, [Expression(root={"unique": {"columns": ["SITEID"]}})], KEYS
        )
    assert unknown.value.condition == "unknown_field"

    with pytest.raises(DeclarationError) as untyped:
        check_column(
            completed, column("AGE", "int", {"matches": {"pattern": "^1$"}}), KEYS
        )
    assert untyped.value.requirement == "R009-30"

    with pytest.raises(DeclarationError) as unreadable:
        check_column(
            completed,
            column("STUDYID", "str", {"matches": {"pattern": "(?P<code>[MFU])"}}),
            KEYS,
        )
    assert unreadable.value.condition == "invalid_regex"
    assert unreadable.value.context == {"pattern": "(?P<code>[MFU])"}


def test_a_predicate_naming_an_absent_column_is_refused_on_an_empty_artifact() -> None:
    empty = table([("STUDYID", "str"), ("USUBJID", "str")], [])
    declaration = Expression(
        root={"predicate": {"id": "names-a-ghost", "assert": "ABSENT = 1"}}
    )

    with pytest.raises(DeclarationError) as raised:
        check_dataset(empty, [declaration], KEYS)

    assert raised.value.condition == "unknown_field"
    assert raised.value.context == {"identifier": "ABSENT"}


def test_duplicate_verification_identifiers_are_refused() -> None:
    completed = table([("STUDYID", "str"), ("USUBJID", "str")], [["S", "S-1"]])
    declaration = {"predicate": {"id": "one-rule", "assert": "USUBJID IS NOT NULL"}}

    with pytest.raises(DeclarationError) as raised:
        check_dataset(
            completed,
            [Expression(root=declaration), Expression(root=declaration)],
            KEYS,
        )

    assert raised.value.condition == "duplicate_identifier"
    assert raised.value.requirement == "R009-24"


def test_an_unevaluable_predicate_fails_instead_of_satisfying_a_verification() -> None:
    completed = table(
        [("STUDYID", "str"), ("USUBJID", "str"), ("AGE", "int")],
        [["S", "S-1", 50]],
    )

    with pytest.raises(DeclarationError) as raised:
        check_dataset(
            completed,
            [
                Expression(
                    root={
                        "implies": {
                            "id": "compares-text-with-a-number",
                            "when": "STUDYID = 1",
                            "then": "AGE > 0",
                        }
                    }
                )
            ],
            KEYS,
        )

    assert raised.value.condition == "incompatible_input_type"


def test_verify_completed_table_stops_at_the_first_failing_stage() -> None:
    completed = table(
        [("STUDYID", "str"), ("USUBJID", "str"), ("AGE", "int")],
        [["S", "S-1", 214], ["S", "S-1", 30]],
    )
    columns = [
        Column(name="STUDYID", type="str"),
        Column(name="USUBJID", type="str"),
        column("AGE", "int", {"range": {"min": 18, "max": 100}}),
    ]

    with pytest.raises(VerificationError) as raised:
        verify_completed_table(
            completed,
            columns,
            KEYS,
            [Expression(root={"row_count": {"min": 5}})],
        )

    assert [failure.condition for failure in raised.value.failures] == ["range_failed"]

    passing = table(
        [("STUDYID", "str"), ("USUBJID", "str"), ("AGE", "int")],
        [["S", "S-1", 30], ["S", "S-2", 40]],
    )
    assert (
        verify_completed_table(
            passing, columns, KEYS, [Expression(root={"row_count": {"min": 1}})]
        )
        is passing
    )
