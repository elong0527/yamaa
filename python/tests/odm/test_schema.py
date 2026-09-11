from __future__ import annotations

import pytest
from pydantic import ValidationError
from yamaa.odm.schema import ClinicalItemRow


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
