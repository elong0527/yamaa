from __future__ import annotations

from pathlib import Path

import pytest

from yamaa.io.polars import frame_from_values
from yamaa.models import TypedColumn
from yamaa.specification import load_specification
from yamaa.submission.verification import check_terminology

ROOT = Path(__file__).parents[3]


@pytest.mark.parametrize(
    "extensible,external", [(False, False), (True, False), (False, True)]
)
def test_missing_passes_and_only_nonextensible_lists_constrain(extensible, external):
    spec = load_specification(
        ROOT / "benchmarks/sdtm-dm-metadata/spec.yaml", ROOT / "yaml"
    ).specification
    column = next(column for column in spec.columns if column.name == "SEX")
    table = frame_from_values(
        tuple(
            TypedColumn(name=name, type="str") for name in ["STUDYID", "USUBJID", "SEX"]
        ),
        [["S", "1", None], ["S", "2", "X"], ["S", "3", "F"]],
    )
    code = {"id": "SEX", "extensible": extensible}
    if not external:
        code["items"] = [{"value": "F"}]
    records = []
    failures = check_terminology(
        table, column, spec.keys, {"codelists": [code]}, spec, records=records
    )
    if extensible or external:
        assert failures == () and records == []
    else:
        assert failures[0].context["keys"] == [{"STUDYID": "S", "USUBJID": "2"}]
        assert failures[0].context["values"] == ["X"]
        assert records[0].evaluated_count == 3


def test_numeric_equality_and_exact_text_equality():
    spec = load_specification(
        ROOT / "benchmarks/sdtm-dm-metadata/spec.yaml", ROOT / "yaml"
    ).specification
    age = next(column for column in spec.columns if column.name == "AGE")
    age = age.model_copy(
        update={"submission": age.submission.model_copy(update={"codelist": "AGE"})}
    )
    table = frame_from_values(
        (
            TypedColumn(name="STUDYID", type="str"),
            TypedColumn(name="USUBJID", type="str"),
            TypedColumn(name="AGE", type="int"),
        ),
        [["S", "1", 1], ["S", "2", None]],
    )
    assert (
        check_terminology(
            table,
            age,
            spec.keys,
            {"codelists": [{"id": "AGE", "items": [{"value": 1.0}]}]},
            spec,
        )
        == ()
    )
    sex = next(column for column in spec.columns if column.name == "SEX")
    table = frame_from_values(
        tuple(
            TypedColumn(name=name, type="str") for name in ["STUDYID", "USUBJID", "SEX"]
        ),
        [["S", "1", "f"]],
    )
    assert check_terminology(
        table,
        sex,
        spec.keys,
        {"codelists": [{"id": "SEX", "items": [{"value": "F"}]}]},
        spec,
    )[0].context["values"] == ["f"]


def test_row_override_replaces_shared_codelist_and_reports_only_its_rows():
    spec = load_specification(
        ROOT / "benchmarks/sdtm-lb-metadata/spec.yaml", ROOT / "yaml"
    ).specification
    column = next(column for column in spec.columns if column.name == "LBORRESU")
    columns = tuple(
        TypedColumn(name=name, type=kind)
        for name, kind in [
            ("STUDYID", "str"),
            ("USUBJID", "str"),
            ("LBSEQ", "int"),
            ("LBTESTCD", "str"),
            ("LBORRESU", "str"),
        ]
    )
    table = frame_from_values(
        columns,
        [
            ["S", "1", 1, "GLUC", "g/L"],
            ["S", "1", 2, "CREAT", "mg/dL"],
            ["S", "1", 3, "OTHER", "g/L"],
        ],
    )
    document = {
        "codelists": [
            {"id": "CL_LBORRESU", "items": [{"value": "g/L"}]},
            {"id": "CL_GLUCOSE", "items": [{"value": "mg/dL"}]},
            {"id": "CL_CREATININE", "items": [{"value": "mg/dL"}]},
        ]
    }
    records = []
    (failure,) = check_terminology(
        table, column, spec.keys, document, spec, records=records
    )
    assert failure.context["codelist"] == "CL_GLUCOSE"
    assert failure.context["keys"][0]["LBSEQ"] == 1
    assert sum(record.evaluated_count for record in records) == 3
