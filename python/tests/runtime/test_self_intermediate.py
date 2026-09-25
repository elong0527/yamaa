"""A SELF intermediate selects records completed by earlier row templates."""

from __future__ import annotations

from pathlib import Path

import pytest
import yaml

from yamaa.io.polars import frame_from_values
from yamaa.models import TypedColumn
from yamaa.runtime import ExecutionFailure, ExecutionSuccess, execute_specification
from yamaa.specification import load_specification
from yamaa.specification.models import (
    Column,
    DatasetSource,
    Expression,
    HandledExpression,
    Intermediate,
    IntermediateVerification,
    OrderTerm,
    Output,
    Row,
    Specification,
)


def derive(value: object) -> HandledExpression:
    if isinstance(value, str):
        value = {"source": value}
    return HandledExpression(value=Expression(root=value))


def specification(*, first_uses_self: bool = False) -> Specification:
    observed = {
        "USUBJID": derive("QS.USUBJID"),
        "AVISITN": derive("QS.AVISITN"),
        "QSSEQ": derive("QS.QSSEQ"),
        "AVAL": derive("DONOR.AVAL" if first_uses_self else "QS.AVAL"),
        "AWRANK": derive(
            {
                "row_number": {
                    "window": {
                        "group_by": ["USUBJID", "AVISITN"],
                        "order_by": ["QSSEQ"],
                    }
                }
            }
        ),
        "ANL01FL": derive({"case": [{"when": "AWRANK = 1", "then": {"literal": "Y"}}]}),
    }
    locf = {
        "USUBJID": derive("QS.USUBJID"),
        "AVISITN": derive({"literal": 8}),
        "QSSEQ": derive("DONOR.QSSEQ"),
        "AVAL": derive("DONOR.AVAL"),
        "AWRANK": derive({"literal": None}),
        "ANL01FL": derive({"literal": None}),
    }
    return Specification(
        schema_version="1.0",
        domain="ADQS",
        input={"QS": DatasetSource(path="input/qs.csv")},
        base="QS",
        keys=["USUBJID", "AVISITN", "QSSEQ"],
        intermediates=[
            Intermediate(
                id="DONOR",
                dataset="SELF",
                key=["USUBJID"],
                filter="ANL01FL = 'Y' AND AVISITN <= 4",
                order_by=[OrderTerm(variable="AVISITN", direction="desc")],
                keep="first",
            )
        ],
        rows=[
            Row(id="obs", dataset="QS", derivations=observed),
            Row(
                id="locf8",
                dataset="QS",
                filter="QS.QSSEQ = 3",
                derivations=locf,
            ),
        ],
        columns=[
            Column(name=name, type=type_)
            for name, type_ in (
                ("USUBJID", "str"),
                ("AVISITN", "int"),
                ("QSSEQ", "int"),
                ("AVAL", "float"),
                ("AWRANK", "int"),
                ("ANL01FL", "str"),
            )
        ],
        output=Output(
            path="adqs.csv",
            columns=["USUBJID", "AVISITN", "QSSEQ", "AVAL", "ANL01FL"],
        ),
    )


def sources():
    columns = (
        TypedColumn(name="USUBJID", type="str"),
        TypedColumn(name="AVISITN", type="int"),
        TypedColumn(name="QSSEQ", type="int"),
        TypedColumn(name="AVAL", type="float"),
    )
    return {
        "QS": frame_from_values(
            columns,
            [["S1", 4, 1, 10.0], ["S1", 4, 2, 20.0], ["S1", 6, 3, 30.0]],
        )
    }


@pytest.mark.parametrize("qualified", [False, True])
def test_self_selects_prior_window_derived_observation(
    qualified: bool, tmp_path: Path
) -> None:
    spec = specification()
    if qualified:
        donor = spec.intermediates[0].model_copy(
            update={
                "filter": "SELF.ANL01FL = 'Y' AND SELF.AVISITN <= 4",
                "order_by": [OrderTerm(variable="SELF.AVISITN", direction="desc")],
            }
        )
        spec = spec.model_copy(update={"intermediates": [donor]})
    path = tmp_path / "spec.yaml"
    path.write_text(yaml.safe_dump(spec.model_dump(mode="json", exclude_none=True)))
    loaded = load_specification(path, Path(__file__).parents[3] / "yaml")
    result = execute_specification(loaded.specification, sources())

    assert isinstance(result, ExecutionSuccess), result
    rows = result.artifact.frame.to_dicts()
    assert [(row["AVISITN"], row["QSSEQ"], row["AVAL"]) for row in rows] == [
        (4, 1, 10.0),
        (4, 2, 20.0),
        (6, 3, 30.0),
        (8, 1, 10.0),
    ]
    assert [row["ANL01FL"] for row in rows] == ["Y", None, "Y", None]


def test_first_template_cannot_read_its_unfinished_self_rows() -> None:
    result = execute_specification(specification(first_uses_self=True), sources())

    assert isinstance(result, ExecutionFailure)
    assert any(
        diagnostic.condition == "phase_boundary"
        and diagnostic.spec_paths == ("rows[0].derivations.AVAL.source",)
        for diagnostic in result.diagnostics
    )


def test_self_uniqueness_checks_completed_derived_rows() -> None:
    spec = specification()
    donor = spec.intermediates[0].model_copy(
        update={
            "filter": None,
            "verification": IntermediateVerification(unique=["USUBJID", "AVISITN"]),
        }
    )
    spec = spec.model_copy(update={"intermediates": [donor]})

    result = execute_specification(spec, sources())

    assert isinstance(result, ExecutionFailure)
    assert any(
        diagnostic.condition == "duplicate_intermediate_records"
        and diagnostic.spec_paths == ("intermediates[0].verification",)
        for diagnostic in result.diagnostics
    )


# Issue #1005: AVISITN and AWTDIFF are each derived once, at column level,
# although row-phase machinery reads both: the obs window partitions by
# AVISITN and orders by AWTDIFF, both templates match the visit-window table
# on AVISITN, and the LOCF donor filters and orders by it.
HOISTED_ROW_PHASE_READS = """
schema_version: "1.0"
domain: ADQS
keys: [USUBJID, AVISITN, ADY]
input:
  QS: input/qs.csv
  WIN: input/win.csv
intermediates:
  - id: TARGETS
    dataset: WIN
    key: [AVISITN]
  - id: DONOR
    dataset: SELF
    key: [USUBJID]
    filter: "ANL01FL = 'Y' AND AVISITN <= 6"
    order_by:
      - {variable: AVISITN, direction: desc}
    keep: first
columns:
  - {name: USUBJID, type: str}
  - {name: AVISIT, type: str}
  - name: AVISITN
    type: int
    derivation:
      mapping:
        source: AVISIT
        dict: {"Week 4": 4, "Week 6": 6, "Week 8": 8}
  - {name: ADY, type: int}
  - {name: AWTARGET, type: int}
  - name: AWTDIFF
    type: int
    derivation:
      compute:
        expr: "ABS(AWTARGET - ADY)"
  - {name: AVAL, type: float}
  - {name: AWRANK, type: int}
  - {name: ANL01FL, type: str}
output:
  path: adqs.csv
  columns: [USUBJID, AVISIT, AVISITN, ADY, AWTARGET, AWTDIFF, AVAL, ANL01FL]
rows:
  - id: obs
    dataset: QS
    derivations:
      USUBJID: QS.USUBJID
      AVISIT: QS.AVISIT
      ADY: QS.ADY
      AWTARGET: TARGETS.TARGET
      AVAL: QS.AVAL
      AWRANK:
        row_number:
          window:
            group_by: [USUBJID, AVISITN]
            order_by: [AWTDIFF]
      ANL01FL:
        case:
          - when: "AWRANK = 1"
            then: {literal: "Y"}
  - id: locf8
    dataset: QS
    filter: "QS.ADY = 41"
    derivations:
      USUBJID: QS.USUBJID
      AVISIT: {literal: "Week 8"}
      ADY: DONOR.ADY
      AWTARGET: TARGETS.TARGET
      AVAL: DONOR.AVAL
      AWRANK: {literal: null}
      ANL01FL: {literal: null}
"""


# The same records with the window partitioned by AVISIT and the donor
# chosen by ADY, so the TARGETS match is AVISITN's only row-phase reader.
MATCH_ONLY = {
    "group_by: [USUBJID, AVISITN]": "group_by: [USUBJID, AVISIT]",
    "AND AVISITN <= 6": "AND ADY <= 41",
    "{variable: AVISITN, direction: desc}": "{variable: ADY, direction: desc}",
}


@pytest.mark.parametrize("replacements", [{}, MATCH_ONLY], ids=["all", "match-only"])
def test_row_phase_consumers_read_column_level_derivations(
    replacements: dict[str, str], tmp_path: Path
) -> None:
    text = HOISTED_ROW_PHASE_READS
    for old, new in replacements.items():
        assert old in text
        text = text.replace(old, new)
    path = tmp_path / "spec.yaml"
    path.write_text(text)
    loaded = load_specification(path, Path(__file__).parents[3] / "yaml")
    qs = frame_from_values(
        (
            TypedColumn(name="USUBJID", type="str"),
            TypedColumn(name="AVISIT", type="str"),
            TypedColumn(name="ADY", type="int"),
            TypedColumn(name="AVAL", type="float"),
        ),
        [
            ["S1", "Week 4", 20, 10.0],
            ["S1", "Week 4", 29, 20.0],
            ["S1", "Week 6", 41, 30.0],
        ],
    )
    win = frame_from_values(
        (
            TypedColumn(name="AVISITN", type="int"),
            TypedColumn(name="TARGET", type="int"),
        ),
        [[4, 28], [6, 42], [8, 56]],
    )

    result = execute_specification(loaded.specification, {"QS": qs, "WIN": win})

    assert isinstance(result, ExecutionSuccess), result
    assert result.artifact.frame.rows() == [
        ("S1", "Week 4", 4, 20, 28, 8, 10.0, None),
        ("S1", "Week 4", 4, 29, 28, 1, 20.0, "Y"),
        ("S1", "Week 6", 6, 41, 42, 1, 30.0, "Y"),
        ("S1", "Week 8", 8, 41, 56, 15, 30.0, None),
    ]
