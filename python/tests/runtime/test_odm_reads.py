from __future__ import annotations

from pathlib import Path

import pytest

from yamaa.io import ProjectResources, load_source_tables
from yamaa.runtime import (
    ExecutionFailure,
    ExecutionSuccess,
    execute_with_source_provider,
)
from yamaa.specification import load_specification

SCHEMA_ROOT = Path(__file__).parents[3] / "yaml"

HEADER = (
    "StudyOID,MetaDataVersionOID,SubjectKey,StudyEventOID,StudyEventRepeatKey,"
    "FormOID,FormRepeatKey,ItemGroupOID,ItemGroupRepeatKey,ItemOID,Value"
)

# Subject 001 has a vital-signs form at two visits and a lab form at the
# first, whose date item is collected once per sample.
ODM = f"""{HEADER}
S1,M1,001,SCREENING,1,FO.VS,1,IG.VS,1,IT.VS.DATE,2025-01-02
S1,M1,001,SCREENING,1,FO.VS,1,IG.VS,1,IT.VS.WEIGHT,61
S1,M1,001,WEEK2,1,FO.VS,1,IG.VS,1,IT.VS.DATE,2025-01-16
S1,M1,001,WEEK2,1,FO.VS,1,IG.VS,1,IT.VS.WEIGHT,60
S1,M1,001,SCREENING,1,FO.LB,1,IG.LB,1,IT.LB.DATE,2025-01-03
S1,M1,001,SCREENING,1,FO.LB,1,IG.LB,2,IT.LB.DATE,2025-01-04
"""

PREAMBLE = """schema_version: "1.0"
domain: VS
keys: [STUDYID, USUBJID, VISIT]
input:
  ODM: input/odm.csv
  DM: input/dm.csv
base: ODM
output:
  path: vs.csv
  columns: [STUDYID, USUBJID, VISIT, VSDTC]
"""


def _run(tmp_path: Path, body: str):
    (tmp_path / "input").mkdir(exist_ok=True)
    (tmp_path / "input/odm.csv").write_text(ODM)
    (tmp_path / "input/dm.csv").write_text("STUDYID,USUBJID\nS1,001\n")
    (tmp_path / "spec.yaml").write_text(PREAMBLE + body)
    specification = load_specification(tmp_path / "spec.yaml", SCHEMA_ROOT)
    resources = ProjectResources(tmp_path)
    return execute_with_source_provider(
        specification.specification,
        lambda datasets: load_source_tables(datasets, resources),
    )


def _columns(dtc: str) -> str:
    return f"""columns:
  - {{name: STUDYID, type: str, label: Study Identifier}}
  - {{name: USUBJID, type: str, label: Subject}}
  - {{name: VISIT, type: str, label: Visit}}
  - name: VSDTC
    type: str
    label: Date
    derivation: {dtc}
"""


GROUPED = """rows:
  - id: VISITS
    dataset: ODM
    group_by: [ODM.StudyOID, ODM.SubjectKey, ODM.StudyEventOID]
    derivations:
      STUDYID: ODM.StudyOID
      USUBJID: ODM.SubjectKey
      VISIT: ODM.StudyEventOID
"""


def test_a_grouped_row_reads_the_item_of_its_own_group(tmp_path: Path) -> None:
    result = _run(
        tmp_path,
        GROUPED + _columns("{odm: {item: ODM.IT.VS.DATE, form: [FO.VS, FO.X]}}"),
    )

    assert isinstance(result, ExecutionSuccess)
    assert result.artifact.frame.rows() == [
        ("S1", "001", "SCREENING", "2025-01-02"),
        ("S1", "001", "WEEK2", "2025-01-16"),
    ]


def test_a_grouped_row_fails_when_its_group_holds_two_records(
    tmp_path: Path,
) -> None:
    result = _run(tmp_path, GROUPED + _columns("{odm: ODM.IT.LB.DATE}"))

    assert isinstance(result, ExecutionFailure)
    [diagnostic] = result.diagnostics
    assert diagnostic.condition == "odm_not_unique"
    assert diagnostic.requirement == "REQ-1278"
    assert diagnostic.context["records"] == 2
    assert diagnostic.context["differ"] == {"ItemGroupRepeatKey": ["1", "2"]}
    assert diagnostic.context["repeated"] is False
    assert diagnostic.context["scope"] == {
        "StudyOID": "S1",
        "SubjectKey": "001",
        "StudyEventOID": "SCREENING",
    }


def test_a_filter_on_a_schema_field_identifies_one_record(tmp_path: Path) -> None:
    dtc = """
      odm:
        item: ODM.IT.LB.DATE
        filter: "ODM.ItemGroupRepeatKey = '2'"
"""
    result = _run(tmp_path, GROUPED + _columns(dtc))

    assert isinstance(result, ExecutionSuccess)
    assert result.artifact.frame["VSDTC"].to_list() == ["2025-01-04", None]


def test_a_record_driven_row_reads_its_own_item_group_occurrence(
    tmp_path: Path,
) -> None:
    body = """rows:
  - id: WEIGHTS
    dataset: ODM
    filter: "ODM.ItemOID = 'IT.VS.WEIGHT'"
    derivations:
      STUDYID: ODM.StudyOID
      USUBJID: ODM.SubjectKey
      VISIT: ODM.StudyEventOID
""" + _columns("{odm: ODM.IT.VS.DATE}")

    result = _run(tmp_path, body)

    assert isinstance(result, ExecutionSuccess)
    assert result.artifact.frame["VSDTC"].to_list() == ["2025-01-02", "2025-01-16"]


def test_a_row_derivation_can_read_an_odm_item(tmp_path: Path) -> None:
    body = GROUPED.replace(
        "      VISIT: ODM.StudyEventOID\n",
        "      VISIT: ODM.StudyEventOID\n      VSDTC: {odm: ODM.IT.VS.DATE}\n",
    ) + _columns("{literal: X}").replace("    derivation: {literal: X}\n", "")

    result = _run(tmp_path, body)

    assert isinstance(result, ExecutionSuccess)
    assert result.artifact.frame["VSDTC"].to_list() == [
        "2025-01-02",
        "2025-01-16",
    ]


def test_a_column_odm_default_can_be_overridden_and_inherited(
    tmp_path: Path,
) -> None:
    rows = """rows:
  - id: screening
    dataset: ODM
    group_by: [ODM.StudyOID, ODM.SubjectKey, ODM.StudyEventOID]
    filter: "VISIT = 'SCREENING'"
    derivations:
      STUDYID: ODM.StudyOID
      USUBJID: ODM.SubjectKey
      VISIT: ODM.StudyEventOID
      VSDTC: {literal: X}
  - id: week2
    dataset: ODM
    group_by: [ODM.StudyOID, ODM.SubjectKey, ODM.StudyEventOID]
    filter: "VISIT = 'WEEK2'"
    derivations:
      STUDYID: ODM.StudyOID
      USUBJID: ODM.SubjectKey
      VISIT: ODM.StudyEventOID
"""
    result = _run(tmp_path, rows + _columns("{odm: ODM.IT.VS.DATE}"))

    assert isinstance(result, ExecutionSuccess)
    assert result.artifact.frame.rows() == [
        ("S1", "001", "SCREENING", "X"),
        ("S1", "001", "WEEK2", "2025-01-16"),
    ]


def test_a_column_odm_override_needs_no_odm_row_scope(tmp_path: Path) -> None:
    rows = """rows:
  - id: other_source
    dataset: DM
    derivations:
      STUDYID: DM.STUDYID
      USUBJID: DM.USUBJID
      VISIT: {literal: SCREENING}
      VSDTC: {literal: X}
"""
    result = _run(tmp_path, rows + _columns("{odm: ODM.IT.VS.DATE}"))

    assert isinstance(result, ExecutionSuccess)
    assert result.artifact.frame.rows() == [("S1", "001", "SCREENING", "X")]


@pytest.mark.parametrize(
    ("body", "path", "location"),
    [
        (
            """rows:
  - id: SUBJECTS
    dataset: DM
    derivations:
      STUDYID: DM.STUDYID
      USUBJID: DM.USUBJID
      VISIT: {literal: SCREENING}
      VSDTC: {odm: ODM.IT.VS.DATE}
"""
            + _columns("{literal: X}").replace("    derivation: {literal: X}\n", ""),
            "rows[0].derivations.VSDTC.odm",
            "row",
        ),
        (
            """intermediates:
  - id: DATES
    dataset: ODM
    derivations:
      DATE: {odm: ODM.IT.VS.DATE}
"""
            + GROUPED
            + _columns("ODM.StudyOID"),
            "intermediates[0].derivations.DATE.odm",
            "intermediate",
        ),
        (
            GROUPED
            + _columns(
                """
      aggregate:
        derive:
          - name: D
            type: str
            derivation: {odm: ODM.IT.VS.DATE}
        expr: "MAX(D)"
"""
            ),
            "columns.VSDTC.derivation.aggregate.derive[0].derivation.odm",
            "derive",
        ),
    ],
)
def test_a_read_with_no_odm_scope_fails_before_any_source_is_read(
    tmp_path: Path, body: str, path: str, location: str
) -> None:
    result = _run(tmp_path, body)

    assert isinstance(result, ExecutionFailure)
    [diagnostic] = [
        found
        for found in result.diagnostics
        if found.condition == "invalid_odm_context"
    ]
    assert diagnostic.spec_paths == (path,)
    assert diagnostic.context == {"dataset": "ODM", "location": location}


def test_a_filter_naming_a_vendor_field_is_unknown(tmp_path: Path) -> None:
    dtc = """
      odm:
        item: ODM.IT.VS.DATE
        filter: "ODM.SITEID = '9'"
"""
    result = _run(tmp_path, GROUPED + _columns(dtc))

    assert isinstance(result, ExecutionFailure)
    [diagnostic] = result.diagnostics
    assert diagnostic.condition == "unknown_field"
    assert diagnostic.spec_paths == ("columns.VSDTC.derivation.odm.filter",)
    assert diagnostic.context == {"identifier": "ODM.SITEID", "dataset": "ODM"}
