from __future__ import annotations

import pytest
from pydantic import ValidationError

from yamaa.specification.models import Intermediate


def test_intermediate_verification_accepts_a_nonempty_unique_list() -> None:
    intermediate = Intermediate.model_validate(
        {
            "id": "DS_EOS",
            "dataset": "DS",
            "verification": {"unique": ["STUDYID", "USUBJID"]},
        }
    )

    assert intermediate.verification is not None
    assert intermediate.verification.unique == ["STUDYID", "USUBJID"]


def test_intermediate_verification_rejects_an_empty_unique_list() -> None:
    # REQ-1243: the assertion names at least one column.
    with pytest.raises(ValidationError):
        Intermediate.model_validate(
            {"id": "DS_EOS", "dataset": "DS", "verification": {"unique": []}}
        )


def test_intermediate_verification_forbids_extra_keys() -> None:
    # REQ-1243: the singular syntax carries no severity; a duplicate can
    # never resolve ambiguously, so warning severity is not expressible.
    with pytest.raises(ValidationError):
        Intermediate.model_validate(
            {
                "id": "DS_EOS",
                "dataset": "DS",
                "verification": {"unique": ["STUDYID"], "severity": "warning"},
            }
        )


def test_intermediate_verification_defaults_to_absent() -> None:
    intermediate = Intermediate.model_validate({"id": "DS_EOS", "dataset": "DS"})

    assert intermediate.verification is None
