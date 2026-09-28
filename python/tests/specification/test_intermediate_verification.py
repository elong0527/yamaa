from __future__ import annotations

import pytest
from pydantic import ValidationError

from yamaa.specification.models import Intermediate


def _verification(**fields: object) -> dict[str, object]:
    declaration: dict[str, object] = {
        "id": "ds-eos-unique",
        "unique": {"columns": ["STUDYID", "USUBJID"]},
    }
    declaration.update(fields)
    return {"id": "DS_EOS", "dataset": "DS", "verification": declaration}


def test_intermediate_verification_accepts_a_nonempty_unique_list() -> None:
    intermediate = Intermediate.model_validate(_verification())

    assert intermediate.verification is not None
    assert intermediate.verification.id == "ds-eos-unique"
    assert intermediate.verification.unique.columns == ["STUDYID", "USUBJID"]


def test_intermediate_verification_rejects_an_empty_unique_list() -> None:
    # REQ-1245: the assertion names at least one column.
    with pytest.raises(ValidationError):
        Intermediate.model_validate(_verification(unique={"columns": []}))


def test_intermediate_verification_requires_an_id() -> None:
    # REQ-0374: every verification carries an id.
    with pytest.raises(ValidationError):
        Intermediate.model_validate(
            {
                "id": "DS_EOS",
                "dataset": "DS",
                "verification": {"unique": {"columns": ["STUDYID"]}},
            }
        )


def test_intermediate_verification_forbids_extra_keys() -> None:
    # REQ-1245: the singular syntax carries no severity; a duplicate can
    # never resolve ambiguously, so warning severity is not expressible.
    with pytest.raises(ValidationError):
        Intermediate.model_validate(_verification(severity="warning"))


def test_intermediate_verification_defaults_to_absent() -> None:
    intermediate = Intermediate.model_validate({"id": "DS_EOS", "dataset": "DS"})

    assert intermediate.verification is None
