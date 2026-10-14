from __future__ import annotations

from pathlib import Path

import pytest
from pydantic import ValidationError

from yamaa._odm.schema import (
    ODM_ITEM_SCHEMA,
    ClinicalItemRow,
    ParquetWriteResult,
    rows_to_frame,
)


def valid_row() -> dict[str, object]:
    return {
        "ODMVersion": "2.0",
        "FileOID": None,
        "StudyOID": "S",
        "MetaDataVersionOID": "M",
        "SubjectKey": "SUBJ",
        "StudySubjectID": None,
        "SubjectStatus": None,
        "StudyEventOID": "E",
        "StudyEventRepeatKey": None,
        "EventName": None,
        "StartDate": None,
        "EventStatus": None,
        "EventWorkflowStatus": None,
        "FormOID": "F",
        "FormRepeatKey": None,
        "FormName": None,
        "FormLayoutOID": None,
        "FormStatus": None,
        "FormWorkflowStatus": None,
        "OpenQueries": None,
        "ItemGroupOID": "G",
        "ItemGroupRepeatKey": None,
        "ItemGroupName": None,
        "TransactionType": None,
        "ItemOID": "I",
        "ItemName": None,
        "Value": "",
        "ValuePresent": True,
        "IsNull": False,
        "SourceOrdinal": 1,
    }


def test_row_preserves_collected_empty_text() -> None:
    row = ClinicalItemRow.model_validate(valid_row())
    assert row.value == ""
    assert row.value_present is True
    assert row.is_null is False


@pytest.mark.parametrize(
    ("changes", "message"),
    [
        ({"Value": None, "ValuePresent": True}, "present value"),
        ({"Value": "x", "ValuePresent": False}, "absent value"),
        ({"Value": "x", "IsNull": True}, "explicitly null"),
        ({"SourceOrdinal": "1"}, "valid integer"),
        ({"Unexpected": "x"}, "Extra inputs"),
    ],
)
def test_row_rejects_invalid_or_coerced_values(
    changes: dict[str, object], message: str
) -> None:
    candidate = valid_row() | changes
    with pytest.raises(ValidationError, match=message):
        ClinicalItemRow.model_validate(candidate)


REQUIRED_TEXT_FIELDS = [
    "ODMVersion",
    "StudyOID",
    "MetaDataVersionOID",
    "SubjectKey",
    "StudyEventOID",
    "FormOID",
    "ItemGroupOID",
    "ItemOID",
]


@pytest.mark.parametrize("field", REQUIRED_TEXT_FIELDS)
def test_required_text_fields_reject_empty_text(field: str) -> None:
    candidate = valid_row() | {field: ""}
    with pytest.raises(ValidationError):
        ClinicalItemRow.model_validate(candidate)


@pytest.mark.parametrize("field", REQUIRED_TEXT_FIELDS)
def test_required_text_fields_accept_single_character(field: str) -> None:
    row = ClinicalItemRow.model_validate(valid_row() | {field: "x"})
    assert row.polars_row()[field] == "x"


def test_source_ordinal_rejects_zero() -> None:
    with pytest.raises(ValidationError):
        ClinicalItemRow.model_validate(valid_row() | {"SourceOrdinal": 0})


def test_row_can_be_validated_by_field_name() -> None:
    row = ClinicalItemRow.model_validate(valid_row())
    by_field_name = row.model_dump(by_alias=False)
    assert ClinicalItemRow.model_validate(by_field_name) == row


def test_row_is_frozen() -> None:
    row = ClinicalItemRow.model_validate(valid_row())
    with pytest.raises(ValidationError):
        row.value = "changed"


def test_polars_row_uses_public_column_names() -> None:
    row = ClinicalItemRow.model_validate(valid_row())
    assert list(row.polars_row()) == ODM_ITEM_SCHEMA.names()
    assert row.polars_row()["StudyOID"] == "S"
    assert row.polars_row()["SourceOrdinal"] == 1


def test_rows_to_frame_uses_fixed_schema() -> None:
    row = ClinicalItemRow.model_validate(valid_row())
    frame = rows_to_frame([row])
    assert frame.schema == ODM_ITEM_SCHEMA
    assert frame.height == 1
    assert frame["Value"].to_list() == [""]
    empty = rows_to_frame([])
    assert empty.schema == ODM_ITEM_SCHEMA
    assert empty.height == 0


def test_parquet_write_result_bounds_and_config() -> None:
    result = ParquetWriteResult(output_path=Path("out.parquet"), row_count=0)
    assert result.row_count == 0
    with pytest.raises(ValidationError):
        ParquetWriteResult(output_path=Path("out.parquet"), row_count=-1)
    with pytest.raises(ValidationError):
        ParquetWriteResult(output_path=Path("out.parquet"), row_count="0")
    with pytest.raises(ValidationError):
        result.row_count = 1
