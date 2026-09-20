"""Execute the committed examples this component's relational operations reach.

Every case runs the real public API end to end -- load the committed
specification, read its committed input, execute, and compare what comes out
with the committed artifact or error contract. Reading `expected/` happens
here, in the test, and never in runtime code.
"""

from __future__ import annotations

import textwrap
from pathlib import Path

import polars as pl
import pytest
import yaml

from yamaa.io import render_csv
from yamaa.io.polars import frame_from_values
from yamaa.io.project import ProjectResources
from yamaa.io.source import load_source_tables
from yamaa.models import TypedColumn, TypedTable
from yamaa.runtime import (
    ExecutionFailure,
    ExecutionSuccess,
    execute_specification,
    execute_with_source_provider,
)
from yamaa.specification import SpecificationError, load_specification

REPOSITORY_ROOT = Path(__file__).parents[3]
SCHEMA_ROOT = REPOSITORY_ROOT / "yaml"
EXAMPLES = REPOSITORY_ROOT / "benchmark"

# Committed examples whose join, lookup, aggregate, and grouped-row work this
# component now performs end to end.
ARTIFACT_EXAMPLES = [
    "sdtm-dm-dates",
    "adam-adlb-mean",
    # REQ-0119 matches on the applicable keys as the two sides declare them, so
    # a sequence number joins once both sides say it is one.
    "adam-adae-severity",
    # #217 could only gate this one on `row_number`; PY-14 supplies the
    # window, so its joins and contextual ODM resolution now run end to end.
    "sdtm-lb-multiform",
]

# Committed error contracts this component reproduces field for field.
ERROR_EXAMPLES = [
    "negative-source-duplicate-key",
    "negative-lookup-unmatched-key",
    "negative-lookup-unordered",
    "negative-lookup-unchosen",
    "negative-lookup-mismatched",
    "negative-lookup-unknown-key",
    "negative-lookup-missing-sex",
    "negative-lookup-id-collision",
    "negative-lookup-bad-range",
    # REQ-0143: an unhandled multiple match names the lookup's match under
    # `key` and `lookup_key` and the offending output row under `keys`, the
    # way an unmatched key already did.
    "negative-query-overflow",
    "negative-overlapping-windows",
    "negative-mapping-duplicate-key",
    "negative-mapping-unmapped-key",
    "negative-mapping-partial-key",
    "negative-mapping-unpaired-key",
    "negative-adlb-duplicate-wbc",
]


def _run(directory: Path) -> object:
    specification = load_specification(
        directory / "spec.yaml", SCHEMA_ROOT
    ).specification
    resources = ProjectResources(directory)
    return execute_with_source_provider(
        specification,
        lambda datasets: load_source_tables(datasets, resources),
    )


def _committed_error(directory: Path) -> dict[str, object]:
    return yaml.safe_load((directory / "expected" / "error.yaml").read_text("utf-8"))


@pytest.mark.parametrize("name", ARTIFACT_EXAMPLES)
def test_a_committed_example_reproduces_its_committed_artifact(name: str) -> None:
    directory = EXAMPLES / name
    specification = load_specification(
        directory / "spec.yaml", SCHEMA_ROOT
    ).specification

    result = _run(directory)

    assert isinstance(result, ExecutionSuccess), result
    committed = (directory / "expected" / specification.output.path).read_bytes()
    assert render_csv(result.artifact) == committed


@pytest.mark.parametrize("name", ERROR_EXAMPLES)
def test_a_committed_error_contract_is_reproduced(name: str) -> None:
    directory = EXAMPLES / name
    committed = _committed_error(directory)

    try:
        result = _run(directory)
    except SpecificationError as error:
        # A spec that fails schema validation never reaches execution; the
        # schema diagnostic is the committed contract.
        diagnostic = error.diagnostics[0]
    else:
        assert isinstance(result, ExecutionFailure), result
        diagnostic = result.diagnostics[0]
    assert diagnostic.phase == committed["phase"]
    assert diagnostic.condition == committed["condition"]
    assert list(diagnostic.spec_paths) == committed["spec_paths"]
    assert diagnostic.requirement == committed["requirement"]
    # Every field the contract records is reproduced; a runtime may report
    # more detail beside them, such as the group a reduction failed in.
    assert committed["context"].items() <= diagnostic.context.items()


def test_a_right_side_orphan_creates_no_row_and_studies_stay_apart() -> None:
    # REQ-0122 preserves left row count and order, so an exposure record whose
    # subject is absent from DM contributes to no row and creates none, and a
    # subject id reused under a second study reads only its own records.
    result = _run(EXAMPLES / "sdtm-dm-dates")

    assert isinstance(result, ExecutionSuccess)
    rows = result.artifact.frame.to_dicts()
    assert [(row["STUDYID"], row["USUBJID"]) for row in rows] == [
        ("CATH", "CATH-UCSD-0001"),
        ("CATH", "CATH-UCSD-0002"),
        ("CATH", "CATH-UCSD-0003"),
        ("CATH", "CATH-UCSD-0004"),
        ("CATH2", "CATH-UCSD-0001"),
    ]
    reused = {row["STUDYID"]: row["RFXSTDTC"].isoformat() for row in rows[3:]}
    assert reused == {"CATH": "2025-04-05", "CATH2": "2025-05-06"}


def test_repeated_execution_produces_an_identical_artifact() -> None:
    first = _run(EXAMPLES / "sdtm-dm-dates")
    second = _run(EXAMPLES / "sdtm-dm-dates")

    assert isinstance(first, ExecutionSuccess)
    assert isinstance(second, ExecutionSuccess)
    assert render_csv(first.artifact) == render_csv(second.artifact)


def _lb_table(chunks: list[list[list[object]]]) -> TypedTable:
    columns = (
        TypedColumn(name="STUDYID", type="str"),
        TypedColumn(name="USUBJID", type="str"),
        TypedColumn(name="LBSEQ", type="int"),
        TypedColumn(name="LBTESTCD", type="str"),
        TypedColumn(name="LBSTRESN", type="float"),
    )
    frames = [frame_from_values(columns, rows).frame for rows in chunks]
    return TypedTable(columns=columns, frame=pl.concat(frames, how="vertical"))


def test_a_different_input_batch_size_changes_no_value_and_no_row_order() -> None:
    # REQ-0480 folds SUM in relation record order, so a relation delivered as
    # several batches must reduce exactly as the same records delivered as one.
    directory = EXAMPLES / "adam-adlb-mean"
    specification = load_specification(
        directory / "spec.yaml", SCHEMA_ROOT
    ).specification
    rows: list[list[object]] = [
        ["KN189", "KN189-101", 1, "NEUT", 3.1],
        ["KN189", "KN189-101", 2, "NEUT", 2.8],
        ["KN189", "KN189-101", 3, "NEUT", 3.4],
        ["KN189", "KN189-101", 4, "NEUT", None],
        ["KN189", "KN189-102", 1, "NEUT", 4.0],
        ["KN189", "KN189-102", 2, "NEUT", 4.2],
        ["KN189", "KN189-103", 1, "NEUT", None],
    ]

    whole = execute_specification(specification, {"LB": _lb_table([rows])})
    batched = execute_specification(
        specification,
        {"LB": _lb_table([rows[:2], rows[2:5], rows[5:]])},
    )

    assert isinstance(whole, ExecutionSuccess)
    assert isinstance(batched, ExecutionSuccess)
    assert render_csv(batched.artifact) == render_csv(whole.artifact)
    assert (
        render_csv(whole.artifact) == (directory / "expected" / "adlb.csv").read_bytes()
    )


# One compact study that exercises the whole component in one run: a named
# lookup with a declared selection, a cross-dataset aggregate, an inline
# `lookup` declared-key lookup, an output-row reduction, and a grouped row
# template with a grouped filter. It stays scalar-only because
# sdtm-lb-multiform's own end to end run waits on #221.
_SPEC = """\
schema_version: "1.0"
domain: ADLB
input:
  LB: {path: input/lb.csv, types: {AVAL: float}}
  EX: {path: input/ex.csv, types: {EXSEQ: int, EXDOSE: float}}
  REF: {path: input/ref.csv, types: {ANRHI: float}}
keys: [STUDYID, USUBJID, PARAMCD]

intermediates:
  - id: LASTEX
    dataset: EX
    key: [STUDYID, USUBJID]
    order_by: [EX.EXSEQ]
    keep: last

output:
  path: adlb.csv
  columns: [STUDYID, USUBJID, PARAMCD, SEX, NPARAM, AVAL, ANRHI, TRT,
            TOTDOSE, LASTTRT, STUDYMAX]

columns:
  - name: STUDYID
    type: str
  - name: USUBJID
    type: str
  - name: PARAMCD
    type: str
  - name: SEX
    type: str
  - name: NPARAM
    type: int
  - name: AVAL
    type: float
  - name: ANRHI
    type: float
    derivation:
      lookup:
        dataset: REF
        key: [PARAMCD, SEX]
        value: ANRHI
        missing: null
  - name: TRT
    type: str
    derivation:
      lookup:
        dataset: EX
        key: [STUDYID, USUBJID]
        value: EXTRT
        order_by: [EX.EXSEQ]
        keep: first
  - name: TOTDOSE
    type: float
    derivation:
      aggregate:
        key: [STUDYID, USUBJID]
        filter: "EX.EXDOSE > 0"
        expr: "SUM(EX.EXDOSE)"
  - name: LASTTRT
    type: str
    derivation:
      source: LASTEX.EXTRT
  - name: STUDYMAX
    type: float
    derivation:
      aggregate:
        group_by: [STUDYID]
        expr: "MAX(AVAL)"

rows:
  - id: collected
    dataset: LB
    derivations:
      STUDYID: {source: LB.STUDYID}
      USUBJID: {source: LB.USUBJID}
      PARAMCD: {source: LB.PARAMCD}
      SEX: {source: LB.SEX}
      NPARAM: {literal: null}
      AVAL: {source: LB.AVAL}
  - id: total
    dataset: LB
    group_by: [LB.STUDYID, LB.USUBJID, LB.SEX]
    filter: "NPARAM > 1"
    derivations:
      STUDYID: {source: LB.STUDYID}
      USUBJID: {source: LB.USUBJID}
      PARAMCD: {literal: TOTAL}
      SEX: {source: LB.SEX}
      NPARAM:
        aggregate:
          expr: "COUNT(LB.*)"
      AVAL:
        aggregate:
          expr: "SUM(LB.AVAL)"

verifications:
  - unique:
      columns: [STUDYID, USUBJID, PARAMCD]
"""

_LB = """\
STUDYID,USUBJID,PARAMCD,SEX,AVAL
S1,P1,WBC,F,4.0
S1,P1,LYM,F,1.0
S1,P2,WBC,M,6.0
S2,P1,WBC,F,9.0
"""

_EX = """\
STUDYID,USUBJID,EXSEQ,EXTRT,EXDOSE
S1,P1,1,DRUG,10
S1,P1,2,DRUG,20
S1,P2,1,PLACEBO,0
S9,P9,1,ORPHAN,99
"""

_REF = """\
PARAMCD,SEX,ANRHI
WBC,F,11
WBC,M,12
LYM,F,4
"""

_EXPECTED = """\
STUDYID,USUBJID,PARAMCD,SEX,NPARAM,AVAL,ANRHI,TRT,TOTDOSE,LASTTRT,STUDYMAX
S1,P1,WBC,F,,4,11,DRUG,30,DRUG,6
S1,P1,LYM,F,,1,4,DRUG,30,DRUG,6
S1,P2,WBC,M,,6,12,PLACEBO,,PLACEBO,6
S2,P1,WBC,F,,9,11,,,,9
S1,P1,TOTAL,F,2,5,,DRUG,30,DRUG,6
"""


@pytest.fixture
def relational_study(tmp_path: Path) -> Path:
    (tmp_path / "input").mkdir()
    (tmp_path / "spec.yaml").write_text(textwrap.dedent(_SPEC), encoding="utf-8")
    (tmp_path / "input/lb.csv").write_text(_LB, encoding="utf-8")
    (tmp_path / "input/ex.csv").write_text(_EX, encoding="utf-8")
    (tmp_path / "input/ref.csv").write_text(_REF, encoding="utf-8")
    return tmp_path


def test_one_scalar_study_exercises_every_relational_operation(
    relational_study: Path,
) -> None:
    result = _run(relational_study)

    assert isinstance(result, ExecutionSuccess), result
    assert render_csv(result.artifact).decode("utf-8") == _EXPECTED


def test_that_study_reports_the_selection_handler_only_where_it_chose(
    relational_study: Path,
) -> None:
    # REQ-0356: the handler count reports only the records where more than one
    # match survived the filter, so the single-match subject is not counted.
    result = _run(relational_study)

    assert isinstance(result, ExecutionSuccess)
    counts = {
        (count.spec_path, count.handler): count.count for count in result.handler_counts
    }
    assert (
        counts[("columns.TRT.derivation.lookup.multiple_matches", "multiple_matches")]
        == 3
    )
    assert counts[("columns.ANRHI.derivation.lookup.missing", "missing")] == 1


def test_that_study_is_reproduced_exactly_on_a_second_run(
    relational_study: Path,
) -> None:
    first = _run(relational_study)
    second = _run(relational_study)

    assert isinstance(first, ExecutionSuccess)
    assert isinstance(second, ExecutionSuccess)
    assert render_csv(first.artifact) == render_csv(second.artifact)


# A second compact study for the narrowing the first one does not reach: an
# aggregate `between`, a reduction grouped coarser than the applicable keys,
# and a record lookup read from inside a numeric expression.
_RANGE_SPEC = """\
schema_version: "1.0"
domain: ADVS
input:
  VS: {path: input/vs.csv, types: {VSSEQ: int, ADY: int, AVAL: float}}
  EPOCHS: {path: input/epochs.csv, types: {LO: int, HI: int, LIMIT: int}}
  EX: {path: input/ex.csv, types: {EXSEQ: int, EXDOSE: float, STARTDY: int, ENDDY: int}}
base: VS
keys: [STUDYID, USUBJID, VSSEQ]

intermediates:
  - id: EPOCHDEF
    dataset: EPOCHS
    key: [STUDYID]
    between: {value: ADY, lower: LO, upper: HI}

output:
  path: advs.csv
  columns: [STUDYID, USUBJID, VSSEQ, ADY, AVAL, EXPDOSE, EPOCH, EPOCHLIM, STUDYTOT]

columns:
  - name: STUDYID
    type: str
    derivation: {source: VS.STUDYID}
  - name: USUBJID
    type: str
    derivation: {source: VS.USUBJID}
  - name: VSSEQ
    type: int
    derivation: {source: VS.VSSEQ}
  - name: ADY
    type: int
    derivation: {source: VS.ADY}
  - name: AVAL
    type: float
    derivation: {source: VS.AVAL}
  - name: EXPDOSE
    type: float
    derivation:
      aggregate:
        key: [STUDYID, USUBJID]
        between: {value: ADY, lower: EX.STARTDY, upper: EX.ENDDY}
        expr: "SUM(EX.EXDOSE)"
  - name: EPOCH
    type: str
    derivation: {source: EPOCHDEF.EPOCH}
  - name: EPOCHLIM
    type: float
    derivation:
      compute:
        expr: "2 * EPOCHDEF.LIMIT"
  - name: STUDYTOT
    type: float
    derivation:
      aggregate:
        key: [STUDYID]
        group_by: [EX.STUDYID]
        expr: "SUM(EX.EXDOSE)"
"""

_VS = """\
STUDYID,USUBJID,VSSEQ,ADY,AVAL
S1,P1,1,3,120
S1,P1,2,40,130
S1,P2,1,3,110
"""

_EPOCHS = """\
STUDYID,EPOCH,LO,HI,LIMIT
S1,SCREENING,-14,0,5
S1,TREATMENT,1,28,10
S1,FOLLOWUP,29,60,20
"""

_RANGE_EX = """\
STUDYID,USUBJID,EXSEQ,EXDOSE,STARTDY,ENDDY
S1,P1,1,10,1,10
S1,P1,2,20,11,50
S1,P2,1,30,1,10
"""

_RANGE_EXPECTED = """\
STUDYID,USUBJID,VSSEQ,ADY,AVAL,EXPDOSE,EPOCH,EPOCHLIM,STUDYTOT
S1,P1,1,3,120,10,TREATMENT,20,60
S1,P1,2,40,130,20,FOLLOWUP,40,60
S1,P2,1,3,110,30,TREATMENT,20,60
"""


@pytest.fixture
def range_study(tmp_path: Path) -> Path:
    (tmp_path / "input").mkdir()
    (tmp_path / "spec.yaml").write_text(textwrap.dedent(_RANGE_SPEC), encoding="utf-8")
    (tmp_path / "input/vs.csv").write_text(_VS, encoding="utf-8")
    (tmp_path / "input/epochs.csv").write_text(_EPOCHS, encoding="utf-8")
    (tmp_path / "input/ex.csv").write_text(_RANGE_EX, encoding="utf-8")
    return tmp_path


def test_range_narrowing_and_a_coarser_grain_reach_their_own_records(
    range_study: Path,
) -> None:
    result = _run(range_study)

    assert isinstance(result, ExecutionSuccess), result
    assert render_csv(result.artifact).decode("utf-8") == _RANGE_EXPECTED


def test_a_missing_cutoff_never_reduces_the_unrestricted_right_side(
    range_study: Path,
) -> None:
    # REQ-0124: a missing current-row value leaves the lookup with nothing to
    # match, so the absent policy (default missing) answers without narrowing.
    (range_study / "input/vs.csv").write_text(
        "STUDYID,USUBJID,VSSEQ,ADY,AVAL\nS1,P1,1,,120\n", encoding="utf-8"
    )
    (range_study / "input/epochs.csv").write_text(
        "STUDYID,EPOCH,LO,HI,LIMIT\nS1,TREATMENT,1,28,10\n", encoding="utf-8"
    )

    result = _run(range_study)

    assert isinstance(result, ExecutionSuccess), result
    row = result.artifact.frame.to_dicts()[0]
    assert row["EXPDOSE"] is None
    assert row["EPOCH"] is None
    # The coarser keys do not read the cutoff, so it still reduces.
    assert row["STUDYTOT"] == 60.0


@pytest.mark.parametrize(
    ("bound", "expected"),
    [
        ("{value: ADY, lower: EX.STARTDY}", [10.0, 30.0]),
        ("{value: ADY, upper: EX.ENDDY}", [30.0, 20.0]),
    ],
    ids=["lower only", "upper only"],
)
def test_one_stated_bound_narrows_on_that_side_alone(
    tmp_path: Path, bound: str, expected: list[float]
) -> None:
    # REQ-0135 and REQ-0472: omitting one bound makes the match one-sided; it
    # does not exclude the stated endpoint and it does not open both ends.
    (tmp_path / "input").mkdir()
    (tmp_path / "spec.yaml").write_text(
        textwrap.dedent(_OPEN_RANGE_SPEC).replace("{{BETWEEN}}", bound),
        encoding="utf-8",
    )
    (tmp_path / "input/vs.csv").write_text(
        "STUDYID,USUBJID,VSSEQ,ADY\nS1,P1,1,5\nS1,P1,2,25\n", encoding="utf-8"
    )
    (tmp_path / "input/ex.csv").write_text(
        "STUDYID,USUBJID,EXDOSE,STARTDY,ENDDY\nS1,P1,10,1,10\nS1,P1,20,20,30\n",
        encoding="utf-8",
    )

    result = _run(tmp_path)

    assert isinstance(result, ExecutionSuccess), result
    assert [row["EXPDOSE"] for row in result.artifact.frame.to_dicts()] == expected


_OPEN_RANGE_SPEC = """\
schema_version: "1.0"
domain: ADVS
input:
  VS: {path: input/vs.csv, types: {VSSEQ: int, ADY: int}}
  EX: {path: input/ex.csv, types: {EXDOSE: float, STARTDY: int, ENDDY: int}}
base: VS
keys: [STUDYID, USUBJID, VSSEQ]

output:
  path: advs.csv
  columns: [STUDYID, USUBJID, VSSEQ, ADY, EXPDOSE]

columns:
  - name: STUDYID
    type: str
    derivation: {source: VS.STUDYID}
  - name: USUBJID
    type: str
    derivation: {source: VS.USUBJID}
  - name: VSSEQ
    type: int
    derivation: {source: VS.VSSEQ}
  - name: ADY
    type: int
    derivation: {source: VS.ADY}
  - name: EXPDOSE
    type: float
    derivation:
      aggregate:
        key: [STUDYID, USUBJID]
        between: {{BETWEEN}}
        expr: "SUM(EX.EXDOSE)"
"""


def test_a_contextual_odm_item_is_not_reachable_through_the_join(
    tmp_path: Path,
) -> None:
    # A long-form ODM item is resolved from the current row's complete R002
    # context, never widened to whichever records a key happens to reach. A
    # relation that is not the row driver carries no such context, so the
    # binding refuses rather than answering across item groups.
    (tmp_path / "input").mkdir()
    (tmp_path / "spec.yaml").write_text(textwrap.dedent(_ODM_SPEC), encoding="utf-8")
    (tmp_path / "input/dm.csv").write_text(
        "STUDYID,USUBJID\nST1,P1\n", encoding="utf-8"
    )
    (tmp_path / "input/odm.csv").write_text(
        "StudyOID,SubjectKey,StudyEventOID,ItemGroupOID,ItemOID,Value\n"
        "ST1,P1,SE.A,IG.DM,IT.DM.DIAGGRP,AD\n",
        encoding="utf-8",
    )

    result = _run(tmp_path)

    assert isinstance(result, ExecutionFailure), result
    diagnostic = result.diagnostics[0]
    assert diagnostic.condition == "unknown_field"
    assert diagnostic.context["identifier"] == "ODM.IT.DM.DIAGGRP"


_ODM_SPEC = """\
schema_version: "1.0"
domain: DM
input:
  DM_RAW: input/dm.csv
  ODM: input/odm.csv
base: DM_RAW
keys: [STUDYID, USUBJID]

output:
  path: dm.csv
  columns: [STUDYID, USUBJID, DIAG]

columns:
  - name: STUDYID
    type: str
    derivation: {source: DM_RAW.STUDYID}
  - name: USUBJID
    type: str
    derivation: {source: DM_RAW.USUBJID}
  - name: DIAG
    type: str
    derivation: {source: ODM.IT.DM.DIAGGRP}
"""


# Row-phase reads reach other datasets through the same join the column
# phase uses: a bare source joins on the applicable keys against the
# driver record (REQ-0156), including in a grouped template whose group
# keys carry the match (REQ-0157), and an explicit lookup states
# driver-side match variables.
_ROW_JOIN_SPEC = """\
schema_version: "1.0"
domain: OUT
input:
  MAIN: {path: input/main.csv, types: {W: float}}
  AUX: {path: input/aux.csv, types: {H: float}}
keys: [ID, KIND]
output:
  path: out.csv
  columns: [ID, KIND, VAL]
columns:
  - name: ID
    type: str
    label: Identifier
    derivation:
      source: MAIN.ID
  - name: KIND
    type: str
    label: Kind
  - name: VAL
    type: float
    label: Value
rows:
  - id: carried
    dataset: MAIN
    derivations:
      KIND: {source: MAIN.KIND}
      VAL: {source: MAIN.W}
  - id: looked
    dataset: MAIN
    filter: "MAIN.KIND = 'B'"
    derivations:
      KIND: {literal: LK}
      VAL:
        lookup:
          dataset: AUX
          key_base: [MAIN.ID]
          key: [ID]
          value: H
  - id: joined
    dataset: MAIN
    filter: "MAIN.KIND = 'A'"
    derivations:
      KIND: {literal: JN}
      VAL: {source: AUX.H}
  - id: grouped
    dataset: MAIN
    group_by: [MAIN.ID]
    derivations:
      KIND: {literal: GJ}
      VAL: {source: AUX.H}
verifications:
  - unique:
      columns: [ID, KIND]
"""

_ROW_JOIN_MAIN = """\
ID,KIND,W
s1,A,10
s1,B,20
s2,A,30
"""

_ROW_JOIN_AUX = """\
ID,H
s1,100
s2,
"""

_ROW_JOIN_EXPECTED = """\
ID,KIND,VAL
s1,A,10
s1,B,20
s2,A,30
s1,LK,100
s1,JN,100
s2,JN,
s1,GJ,100
s2,GJ,
"""


@pytest.fixture
def row_join_study(tmp_path: Path) -> Path:
    (tmp_path / "input").mkdir()
    (tmp_path / "spec.yaml").write_text(
        textwrap.dedent(_ROW_JOIN_SPEC), encoding="utf-8"
    )
    (tmp_path / "input/main.csv").write_text(_ROW_JOIN_MAIN, encoding="utf-8")
    (tmp_path / "input/aux.csv").write_text(_ROW_JOIN_AUX, encoding="utf-8")
    return tmp_path


def test_row_phase_reads_reach_other_datasets(row_join_study: Path) -> None:
    result = _run(row_join_study)

    assert isinstance(result, ExecutionSuccess), result
    assert render_csv(result.artifact).decode("utf-8") == _ROW_JOIN_EXPECTED


@pytest.mark.parametrize("predicate", ["AVAL >= 0", "TRUE"])
def test_grouped_filter_precedes_whole_column_verification(
    tmp_path: Path, predicate: str
) -> None:
    # The lifecycle completes expression/conversion before the grouped filter;
    # whole-column verification sees only candidates retained by that filter.
    (tmp_path / "dm.csv").write_text("USUBJID,AGE\nS1,-1\nS2,7\n")
    (tmp_path / "spec.yaml").write_text(
        textwrap.dedent(
            f"""\
            schema_version: "1.0"
            domain: ADSL
            keys: [USUBJID]
            input:
              DM: {{path: dm.csv, types: {{AGE: int}}}}
            output:
              path: adsl.csv
              columns: [USUBJID, AVAL]
            rows:
              - id: subject
                dataset: DM
                group_by: [DM.USUBJID]
                filter: "{predicate}"
                derivations:
                  USUBJID: {{source: DM.USUBJID}}
                  AVAL: {{aggregate: "SUM(DM.AGE)"}}
            columns:
              - name: USUBJID
                type: str
              - name: AVAL
                type: int
                verifications:
                  range: {{min: 0}}
            """
        )
    )

    result = _run(tmp_path)

    if predicate == "TRUE":
        assert isinstance(result, ExecutionFailure), result
        assert result.diagnostics[0].phase == "verification"
    else:
        assert isinstance(result, ExecutionSuccess), result
        assert result.artifact.frame.to_dicts() == [{"USUBJID": "S2", "AVAL": 7}]
