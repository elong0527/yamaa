"""Test the aggregate derive step (REQ-1189 through REQ-1192).

The derive step binds per-record computed values before aggregation,
enabling #704 (text-to-number conversion) and #705 (date-to-epoch-day)
without new expression functions.
"""

from __future__ import annotations

from pathlib import Path

from yamaa.expressions import ExpressionDispatcher, MappingResolver
from yamaa.io import ProjectResources, load_source_tables
from yamaa.io.polars import frame_from_values
from yamaa.models import MISSING, TypedColumn
from yamaa.models.values import DateValue, ValueResult
from yamaa.planning import ExecutionPlanningError, plan_execution
from yamaa.runtime import (
    ExecutionFailure,
    ExecutionSuccess,
    execute_specification,
    execute_with_source_provider,
)
from yamaa.specification import load_specification
from yamaa.specification.models import (
    Column as SpecColumn,
)
from yamaa.specification.models import (
    DatasetSource,
    Expression,
    Output,
    Row,
    Specification,
)
from yamaa.specification.models import (
    HandledExpression as SpecHandledExpression,
)

REPOSITORY_ROOT = Path(__file__).parents[3]
SCHEMA_ROOT = REPOSITORY_ROOT / "yaml"


def _run_benchmark(name: str):
    bench = REPOSITORY_ROOT / "benchmarks" / name
    specification = load_specification(bench / "spec.yaml", SCHEMA_ROOT).specification
    resources = ProjectResources(bench)
    return execute_with_source_provider(
        specification,
        lambda datasets: load_source_tables(datasets, resources),
    )


def test_derive_converts_text_to_number_for_aggregation() -> None:
    """#704: text scores convert to float via the binding type, then sum."""
    result = _run_benchmark("adam-adqs-derive-text-score")

    assert isinstance(result, ExecutionSuccess)
    rows = result.artifact.frame.to_dicts()
    assert len(rows) == 1
    assert rows[0]["AVAL"] == 6.0
    assert rows[0]["PARAMCD"] == "PFSCORE"


def test_derive_converts_date_to_epoch_day() -> None:
    """#705: dates convert to integer epoch days via to_epoch_day."""
    result = _run_benchmark("adam-adex-derive-date-epoch")

    assert isinstance(result, ExecutionSuccess)
    rows = result.artifact.frame.to_dicts()
    assert len(rows) == 1
    # 2024-01-15 is 19737 days after 1970-01-01.
    assert rows[0]["EXSTDY"] == 19737


def test_to_epoch_day_epoch_is_zero() -> None:
    """#705: 1970-01-01 is day zero."""
    dispatcher = ExpressionDispatcher()
    expr = Expression.model_validate({"to_epoch_day": {"source": "EX.EXSTDT"}})
    result = dispatcher.evaluate(
        expr,
        MappingResolver({"EX.EXSTDT": DateValue(year=1970, month=1, day=1)}),
    )

    assert result.value == 0


def test_to_epoch_day_before_epoch_is_negative() -> None:
    """#705: dates before 1970-01-01 are negative."""
    dispatcher = ExpressionDispatcher()
    expr = Expression.model_validate({"to_epoch_day": {"source": "EX.EXSTDT"}})
    result = dispatcher.evaluate(
        expr,
        MappingResolver({"EX.EXSTDT": DateValue(year=1969, month=12, day=31)}),
    )

    assert result.value == -1


def test_to_epoch_day_missing_stays_missing() -> None:
    """#705: a missing date returns missing, not an error."""
    dispatcher = ExpressionDispatcher()
    expr = Expression.model_validate({"to_epoch_day": {"source": "EX.EXSTDT"}})
    result = dispatcher.evaluate(expr, MappingResolver({"EX.EXSTDT": MISSING}))

    assert result.value is MISSING


# ---------------------------------------------------------------------------
# Derive-step edge cases: planner diagnostics, conversion failure, extensions.
# ---------------------------------------------------------------------------

_INLINE_SPEC = """\
schema_version: "1.0"
domain: ADQS
keys: [STUDYID, USUBJID]
input:
  QS: {{path: input/qs.csv}}
output:
  path: adqs.csv
  columns: [STUDYID, USUBJID, AVAL]
columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: QS.STUDYID
  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: QS.USUBJID
  - name: AVAL
    type: float
    label: Analysis Value
    derivation:
      aggregate:
        filter: "QS.QSCAT = 'SCALE'"
        key: [STUDYID, USUBJID]
        derive:
{derive_block}
        expr: "{expr}"
rows:
  - id: subscale
    filter: "QS.QSCAT = 'SCALE' AND QS.QSTESTCD = 'PF01'"
    derivations: {{}}
"""

_INLINE_QS = """\
STUDYID,USUBJID,QSTESTCD,QSCAT,QSORRES
S1,001,PF01,SCALE,1
S1,001,PF02,SCALE,2
S1,001,PF03,SCALE,3
"""

_INLINE_QS_WITH_TEXT = """\
STUDYID,USUBJID,QSTESTCD,QSCAT,QSORRES
S1,001,PF01,SCALE,1
S1,001,PF02,SCALE,N/A
S1,001,PF03,SCALE,3
"""


def _inline_derive(block: str) -> str:
    return "\n".join(
        f"          {line}" if line.strip() else line for line in block.splitlines()
    )


def _run_inline(
    tmp_path,
    derive_block: str,
    csv_text: str = _INLINE_QS,
    expr: str = "SUM(QSNUM)",
    dispatcher=None,
):
    input_dir = tmp_path / "input"
    input_dir.mkdir(exist_ok=True)
    (input_dir / "qs.csv").write_text(csv_text)
    spec_file = tmp_path / "spec.yaml"
    spec_file.write_text(
        _INLINE_SPEC.format(derive_block=_inline_derive(derive_block), expr=expr)
    )
    specification = load_specification(spec_file, SCHEMA_ROOT).specification
    resources = ProjectResources(tmp_path)
    return execute_with_source_provider(
        specification,
        lambda datasets: load_source_tables(datasets, resources),
        dispatcher=dispatcher,
    )


def _qsnum_binding() -> str:
    return "- name: QSNUM\n  type: float\n  derivation: QS.QSORRES"


def test_derive_duplicate_binding_names_fail(tmp_path) -> None:
    """REQ-1189: two bindings may not share a name."""
    result = _run_inline(
        tmp_path,
        _qsnum_binding() + "\n" + _qsnum_binding(),
    )

    assert isinstance(result, ExecutionFailure)
    assert result.diagnostics[0].condition == "invalid_derive_step"
    assert "unique" in result.diagnostics[0].context["reason"]


def test_derive_binding_forward_reference_fails(tmp_path) -> None:
    """REQ-1189: a binding may only read bindings declared before it."""
    result = _run_inline(
        tmp_path,
        "- name: DOUBLED\n  type: float\n  derivation: QSNUM\n" + _qsnum_binding(),
        expr="SUM(DOUBLED)",
    )

    assert isinstance(result, ExecutionFailure)
    assert result.diagnostics[0].condition == "unknown_derive_variable"
    assert result.diagnostics[0].context["variable"] == "QSNUM"


def test_derive_binding_reads_earlier_binding(tmp_path) -> None:
    """REQ-1189: a binding may read a binding declared before it."""
    result = _run_inline(
        tmp_path,
        _qsnum_binding()
        + "\n- name: DOUBLED\n  type: float\n  derivation:\n    compute:\n      expr: 'QSNUM * 2'",
        expr="SUM(DOUBLED)",
    )

    assert isinstance(result, ExecutionSuccess)
    assert result.artifact.frame.to_dicts() == [
        {"STUDYID": "S1", "USUBJID": "001", "AVAL": 12.0}
    ]


def test_derive_conversion_failure_handler_supplies_fallback(tmp_path) -> None:
    """A binding conversion failure falls back to the declared handler value."""
    result = _run_inline(
        tmp_path,
        "- name: QSNUM\n"
        "  type: float\n"
        "  derivation:\n"
        "    value:\n"
        "      source: QS.QSORRES\n"
        "    conversion_failure: 0",
        csv_text=_INLINE_QS_WITH_TEXT,
    )

    assert isinstance(result, ExecutionSuccess)
    # 1 + 0 (the "N/A" fallback) + 3.
    assert result.artifact.frame.to_dicts() == [
        {"STUDYID": "S1", "USUBJID": "001", "AVAL": 4.0}
    ]


def test_derive_conversion_failure_without_handler_fails(tmp_path) -> None:
    """Without a handler, a binding conversion failure fails the run."""
    result = _run_inline(tmp_path, _qsnum_binding(), csv_text=_INLINE_QS_WITH_TEXT)

    assert isinstance(result, ExecutionFailure)
    assert result.diagnostics[0].condition == "conversion_failed"


def test_derive_bindings_use_the_configured_dispatcher() -> None:
    """REQ-1189: bindings evaluate with the configured dispatcher (R018).

    The YAML loader only accepts schema-known operations, so this builds the
    specification from models to carry the extension operation through.
    """

    def double(payload, resolver):
        inner = payload.get("source") if isinstance(payload, dict) else None
        resolution = resolver.resolve(inner) if isinstance(inner, str) else None
        value = resolution.value if resolution is not None else None
        return ValueResult(value=2 * float(value))

    dispatcher = ExpressionDispatcher(extensions={"double": double})
    derive = [
        {
            "name": "QSNUM",
            "type": "float",
            "derivation": {"double": {"source": "QS.QSORRES"}},
        }
    ]
    sources = {
        "QS": frame_from_values(
            (
                TypedColumn(name="STUDYID", type="str"),
                TypedColumn(name="USUBJID", type="str"),
                TypedColumn(name="QSTESTCD", type="str"),
                TypedColumn(name="QSCAT", type="str"),
                TypedColumn(name="QSORRES", type="str"),
            ),
            [
                ["S1", "001", "PF01", "SCALE", "1"],
                ["S1", "001", "PF02", "SCALE", "2"],
                ["S1", "001", "PF03", "SCALE", "3"],
            ],
        )
    }
    result = execute_specification(
        _planned_spec(derive), sources, dispatcher=dispatcher
    )

    assert isinstance(result, ExecutionSuccess)
    # (1 + 2 + 3) * 2.
    assert result.artifact.frame.to_dicts() == [
        {"STUDYID": "S1", "USUBJID": "001", "AVAL": 12.0}
    ]


def _planned_spec(derive: list[dict], expr: str = "SUM(QSNUM)") -> Specification:
    def derivation(expression: dict) -> SpecHandledExpression:
        return SpecHandledExpression(value=Expression(root=expression))

    return Specification(
        schema_version="1.0",
        domain="ADQS",
        input={"QS": DatasetSource(path="input/qs.csv")},
        output=Output(path="adqs.csv", columns=["STUDYID", "USUBJID", "AVAL"]),
        keys=["STUDYID", "USUBJID"],
        columns=[
            SpecColumn(
                name="STUDYID",
                type="str",
                derivation=derivation({"source": "QS.STUDYID"}),
            ),
            SpecColumn(
                name="USUBJID",
                type="str",
                derivation=derivation({"source": "QS.USUBJID"}),
            ),
            SpecColumn(
                name="AVAL",
                type="float",
                derivation=derivation(
                    {
                        "aggregate": {
                            "filter": "QS.QSCAT = 'SCALE'",
                            "key": ["STUDYID", "USUBJID"],
                            "derive": derive,
                            "expr": expr,
                        }
                    }
                ),
            ),
        ],
        rows=[
            Row(
                id="subscale",
                filter="QS.QSCAT = 'SCALE' AND QS.QSTESTCD = 'PF01'",
                derivations={},
            )
        ],
    )


def _plan(derive: list[dict], expr: str = "SUM(QSNUM)"):
    sources = {
        "QS": frame_from_values(
            (
                TypedColumn(name="STUDYID", type="str"),
                TypedColumn(name="USUBJID", type="str"),
                TypedColumn(name="QSTESTCD", type="str"),
                TypedColumn(name="QSCAT", type="str"),
                TypedColumn(name="QSORRES", type="str"),
            ),
            [["S1", "001", "PF01", "SCALE", "1"]],
        )
    }
    try:
        plan_execution(
            _planned_spec(derive, expr),
            sources,
            supported_operations=ExpressionDispatcher().supported_operations,
        )
    except ExecutionPlanningError as error:
        return error
    return None


def test_plan_rejects_duplicate_binding_names() -> None:
    """REQ-1189: the planner rejects duplicate derive binding names."""
    derive = [
        {"name": "QSNUM", "type": "float", "derivation": "QS.QSORRES"},
        {"name": "QSNUM", "type": "float", "derivation": "QS.QSORRES"},
    ]

    error = _plan(derive)

    assert error is not None
    assert any(
        diagnostic.condition == "invalid_derive_step"
        for diagnostic in error.diagnostics
    )


def test_plan_rejects_forward_binding_reference() -> None:
    """REQ-1189: the planner rejects a binding read before its declaration."""
    derive = [
        {"name": "DOUBLED", "type": "float", "derivation": "QSNUM"},
        {"name": "QSNUM", "type": "float", "derivation": "QS.QSORRES"},
    ]

    error = _plan(derive, expr="SUM(DOUBLED)")

    assert error is not None
    assert any(
        diagnostic.condition == "unknown_derive_variable"
        for diagnostic in error.diagnostics
    )


def test_plan_rejects_derive_step_naming_two_relations() -> None:
    """REQ-1191: the planner rejects bindings that read two relations."""
    derive = [
        {"name": "A", "type": "float", "derivation": "QS.QSORRES"},
        {"name": "B", "type": "float", "derivation": "EX.EXSEQ"},
    ]

    error = _plan(derive, expr="SUM(A)")

    assert error is not None
    assert any(
        diagnostic.condition == "invalid_derive_step"
        for diagnostic in error.diagnostics
    )
