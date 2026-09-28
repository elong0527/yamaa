from __future__ import annotations

import pytest
from pydantic import ValidationError

from yamaa.specification.models import Intermediate


def test_intermediate_accepts_short_and_named_unique_checks() -> None:
    intermediate = Intermediate.model_validate(
        {
            "id": "DS_EOS",
            "dataset": "DS",
            "verifications": [
                {"unique": ["STUDYID", "USUBJID"]},
                {"unique": {"id": "one-row-per-subject", "columns": ["USUBJID"]}},
            ],
        }
    )

    assert intermediate.verifications is not None
    assert intermediate.verifications[0].unique == ["STUDYID", "USUBJID"]
    assert intermediate.verifications[1].unique.id == "one-row-per-subject"


@pytest.mark.parametrize(
    "verification",
    [
        {"unique": []},
        {"unique": {"columns": []}},
        {"unique": {"columns": ["STUDYID"], "severity": "warning"}},
        {"unique": {"id": None, "columns": ["STUDYID"]}},
        {"assert": {"expr": "TRUE"}},
    ],
)
def test_intermediate_rejects_invalid_unique_checks(verification: dict) -> None:
    with pytest.raises(ValidationError):
        Intermediate.model_validate(
            {"id": "DS_EOS", "dataset": "DS", "verifications": [verification]}
        )


def test_intermediate_rejects_the_old_singular_field() -> None:
    with pytest.raises(ValidationError):
        Intermediate.model_validate(
            {"id": "DS_EOS", "dataset": "DS", "verification": {"unique": ["USUBJID"]}}
        )


def test_intermediate_verifications_default_to_absent() -> None:
    intermediate = Intermediate.model_validate({"id": "DS_EOS", "dataset": "DS"})

    assert intermediate.verifications is None
