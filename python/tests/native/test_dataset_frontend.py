"""Source-independent admission and exact lowering of the native specification bridge."""

import json
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

from yamaa.adapters._native_dataset_plan import OPERATIONS, admit, lower
from yamaa.adapters.native_datasets import execute_with_source_provider
from yamaa.io import ProjectResources, load_source_tables
from yamaa.planning import ExecutionPlanningError, plan_execution
from yamaa.runtime import ExecutionUnsupported
from yamaa.specification import load_specification
from yamaa.specification.models import Specification

ROOT = Path(__file__).parents[3]
CASE = ROOT / "benchmarks/adam-adlb-ordered-sum"


@pytest.fixture
def specification():
    """Load the actual schema-validated ADLB document, without reading source data."""
    return load_specification(CASE / "spec.yaml", ROOT / "yaml").specification


def test_adlb_lowering_matches_independent_bound_plan(specification):
    """The frontend recovers every expression, scope, index and original path."""
    admit(specification)
    sources = load_source_tables(specification.input, ProjectResources(CASE))
    plan = plan_execution(specification, sources, supported_operations=OPERATIONS)
    expected = json.loads(
        (
            ROOT / "rust/crates/yamaa-adapters/tests/fixtures/datasets/adlb-plan.json"
        ).read_text()
    )
    assert lower(plan, sources["LB"].table) == (expected, None)


@pytest.mark.parametrize(
    "feature",
    [
        "filter",
        "predicate_regex",
        "predicate_wide_literal",
        "handler",
        "null_handler",
        "source_filter",
        "source_handler",
        "compute",
        "aggregate_expression",
        "aggregate_filter",
        "aggregate_reducer",
        "aggregate_column",
        "column_check",
        "warning",
        "fraction",
        "grouped_count",
        "no_rows",
        "multiple_sources",
        "source_schema",
        "wide_literal",
    ],
)
def test_unsupported_run_never_reads_sources(specification, feature):
    """Every unimplemented feature is rejected as a whole run before the provider."""
    doc = specification.model_dump(exclude_unset=True)
    # Keep the literal handler's authored presence distinct from an omitted default.
    for column in doc["columns"]:
        declaration = column.get("derivation")
        if declaration is not None:
            declaration.pop("unconvertible", None)
    for row in doc["rows"]:
        for declaration in row["derivations"].values():
            declaration.pop("unconvertible", None)
    if feature == "filter":
        doc["filter"] = "LB.LBSTRESN > 0"
        doc["rows"] = None  # valid root-filter mode, still outside this bridge
        for column in doc["columns"]:
            if "derivation" not in column:
                column["derivation"] = {"value": {"literal": None}}
    elif feature == "predicate_regex":
        doc["rows"][0]["filter"] = "str_contains(LB.LBTESTCD, 'COMP')"
    elif feature == "predicate_wide_literal":
        doc["rows"][0]["filter"] = "AVAL > 9223372036854775808"
    elif feature in {"handler", "null_handler"}:
        doc["rows"][0]["derivations"]["AVAL"]["unconvertible"] = (
            0 if feature == "handler" else None
        )
    elif feature in {"source_filter", "source_handler"}:
        payload = {"variable": "LB.LBSTRESN"}
        payload["filter" if feature == "source_filter" else "no_match"] = (
            "LB.LBSTRESN > 0" if feature == "source_filter" else None
        )
        doc["rows"][0]["derivations"]["AVAL"]["value"] = {"source": payload}
    elif feature == "compute":
        doc["rows"][0]["derivations"]["AVAL"]["value"] = {"compute": "1 + 2"}
    elif feature.startswith("aggregate_"):
        payload = {"expr": "SUM(LB.LBSTRESN)"}
        if feature == "aggregate_expression":
            payload["expr"] = "SUM(LB.LBSTRESN + 1)"
        elif feature == "aggregate_filter":
            payload["filter"] = "LB.LBSTRESN > 0"
        elif feature == "aggregate_reducer":
            payload["expr"] = "MAX(LB.LBSTRESN)"
        if feature == "aggregate_column":
            doc["columns"][5]["derivation"] = {"value": {"aggregate": payload}}
            for row in doc["rows"]:
                row["derivations"].pop("AVAL")
        else:
            doc["rows"][1]["derivations"]["AVAL"]["value"] = {"aggregate": payload}
    elif feature == "column_check":
        doc["columns"][0]["verifications"] = [{"not_null": {}}]
    elif feature == "warning":
        doc["verifications"][0]["unique"]["severity"] = "warning"
        doc["output"]["warning_log"] = "warnings.csv"
    elif feature == "fraction":
        doc["verifications"][1]["row_count"]["min_fraction"] = 0.5
    elif feature == "grouped_count":
        doc["verifications"][1]["row_count"]["group_by"] = ["STUDYID"]
    elif feature == "no_rows":
        doc["rows"] = None
        for column in doc["columns"]:
            if "derivation" not in column:
                column["derivation"] = {"value": {"literal": None}}
    elif feature == "multiple_sources":
        doc["input"]["OTHER"] = {"path": "other.csv"}
        for row in doc["rows"]:
            row["dataset"] = "LB"
    elif feature == "source_schema":
        doc["input"]["LB"]["schema_path"] = "producer.yaml"
    else:
        doc["rows"][0]["derivations"]["AVAL"]["value"] = {"literal": 2**80}
    # DatasetSource's schema field uses an alias in model validation.
    if feature == "source_schema":
        doc["input"]["LB"]["schema"] = doc["input"]["LB"].pop("schema_path")
    spec = Specification.model_validate(doc)
    effects = []
    result = execute_with_source_provider(spec, lambda _: effects.append("read"))
    assert isinstance(result.result, ExecutionUnsupported), result
    assert effects == []


def test_invalid_aggregate_grammar_is_validation_before_sources(specification):
    """Malformed syntax is never mislabeled as valid unsupported syntax."""
    doc = specification.model_dump(exclude_unset=True)
    doc["rows"][1]["derivations"]["AVAL"]["value"] = {
        "aggregate": {"expr": "SUM(LB.*)"}
    }
    spec = Specification.model_validate(doc)
    with pytest.raises(ExecutionPlanningError) as raised:
        admit(spec)
    assert raised.value.diagnostics[0].phase == "validation"
    effects = []
    result = execute_with_source_provider(spec, lambda _: effects.append("read"))
    assert result.result.status == "failure"
    assert effects == []


def test_missing_native_entrypoint_precedes_sources(specification, monkeypatch):
    """An old installed native package cannot trigger IO before failing."""
    monkeypatch.setitem(sys.modules, "yamaa_native", SimpleNamespace())
    effects = []
    with pytest.raises(AttributeError):
        execute_with_source_provider(specification, lambda _: effects.append("read"))
    assert effects == []


@pytest.mark.parametrize(
    "declared,values", [("float", [1]), ("str", [1]), ("int", [True])]
)
def test_source_storage_cannot_silently_coerce_logical_types(declared, values):
    """Caller-provided typed tables must actually carry their declared logical types."""
    import polars as pl

    from yamaa.adapters.native_datasets import _source_ipc
    from yamaa.models import TypedColumn, TypedTable

    table = TypedTable(
        columns=(TypedColumn(name="x", type=declared),),
        frame=pl.DataFrame({"x": values}),
    )
    with pytest.raises(ValueError, match="incompatible source storage"):
        _source_ipc(table)


def test_invalid_filter_grammar_fails_before_provider(specification):
    """A malformed predicate is a validation failure, not unsupported execution."""
    spec = specification.model_copy(deep=True)
    spec.rows[0] = spec.rows[0].model_copy(update={"filter": "AVAL != 0"})
    effects = []
    result = execute_with_source_provider(spec, lambda _: effects.append("read"))
    assert result.result.status == "failure"
    assert result.result.diagnostics[0].condition == "invalid_predicate"
    assert result.result.diagnostics[0].spec_paths == ("rows[0].filter",)
    assert effects == []


def test_filter_lowering_retains_scopes_and_association(specification):
    """Bind source and candidate names explicitly in an independently authored tree."""
    spec = specification.model_copy(deep=True)
    spec.rows[0] = spec.rows[0].model_copy(
        update={"filter": "AVAL > 0 AND LB.LBTESTCD LIKE 'COMP%'"}
    )
    admit(spec)
    sources = load_source_tables(spec.input, ProjectResources(CASE))
    plan = plan_execution(spec, sources, supported_operations=OPERATIONS)
    lowered, error = lower(plan, sources["LB"].table)
    assert error is None
    predicate = lowered["templates"][0]["filter"]
    assert predicate["root"] == 2
    assert predicate["nodes"] == [
        {
            "compare": {
                "operator": "gt",
                "left": {"identifier": "AVAL"},
                "right": {"literal": {"int": "0"}},
            }
        },
        {
            "like": {
                "value": {"identifier": "LB.LBTESTCD"},
                "pattern": {"literal": {"str": "COMP%"}},
                "negated": False,
                "escape": None,
            }
        },
        {"and": [0, 1]},
    ]
    assert predicate["bindings"] == [
        {"name": "AVAL", "read": {"column": 5}},
        {
            "name": "LB.LBTESTCD",
            "read": {
                "source": next(
                    i
                    for i, c in enumerate(sources["LB"].table.columns)
                    if c.name == "LBTESTCD"
                )
            },
        },
    ]


@pytest.mark.parametrize(
    "metadata",
    [
        None,
        {"protocol": "dataset/2", "features": ["row_filter"]},
        {"protocol": "dataset/1", "features": []},
    ],
)
def test_filter_capability_refusal_precedes_source_provider(
    specification, monkeypatch, metadata
):
    """Older or incompatible native installations cannot start filter-run IO."""
    doc = specification.model_dump(exclude_unset=True)
    doc["rows"][0]["filter"] = "AVAL > 0"
    native = SimpleNamespace(execute_dataset=lambda *_: pytest.fail("execution"))
    if metadata is not None:
        native.dataset_capabilities = lambda: json.dumps(metadata)
    monkeypatch.setitem(sys.modules, "yamaa_native", native)
    result = execute_with_source_provider(
        Specification.model_validate(doc), lambda _: pytest.fail("source read")
    )
    assert isinstance(result.result, ExecutionUnsupported)
    assert [
        (feature.operation, feature.spec_path) for feature in result.result.features
    ] == [("native_row_filter", "rows[0].filter")]


def test_filter_capability_allows_source_provider(specification, monkeypatch):
    """A compatible installation proceeds to the authorized provider exactly once."""
    doc = specification.model_dump(exclude_unset=True)
    doc["rows"][0]["filter"] = "AVAL > 0"
    monkeypatch.setitem(
        sys.modules,
        "yamaa_native",
        SimpleNamespace(
            execute_dataset=lambda *_: pytest.fail("execution"),
            dataset_capabilities=lambda: (
                '{"protocol":"dataset/1","features":["row_filter"]}'
            ),
        ),
    )
    effects = []

    def provider(_):
        effects.append("read")
        raise RuntimeError("provider sentinel")

    with pytest.raises(RuntimeError, match="provider sentinel"):
        execute_with_source_provider(Specification.model_validate(doc), provider)
    assert effects == ["read"]


@pytest.mark.parametrize(
    "expression", ["AVAL = '\ud800'", "PARAMCD LIKE '%' ESCAPE '\ud800'"]
)
def test_non_scalar_predicate_text_is_unsupported_before_io(specification, expression):
    """Unrepresentable Unicode text never reaches a source or native byte boundary."""
    doc = specification.model_dump(exclude_unset=True)
    doc["rows"][0]["filter"] = expression
    result = execute_with_source_provider(
        Specification.model_validate(doc), lambda _: pytest.fail("source read")
    )
    assert isinstance(result.result, ExecutionUnsupported)
    assert result.result.features[0].operation == "predicate_non_scalar_text"


@pytest.mark.parametrize(
    "index,expression",
    [
        (0, "ABSENT > 0"),
        (1, "ABSENT IS NULL"),
        (1, "LB.STUDYID = 'S'"),
        (1, "ABSENT IS NULL AND LB.STUDYID IS NOT NULL"),
    ],
)
def test_filter_phase_errors_match_planner_before_provider(
    specification, index, expression
):
    """Known phase/scope failures retain planner diagnostics without starting IO."""
    doc = specification.model_dump(exclude_unset=True)
    doc["rows"][index]["filter"] = expression
    spec = Specification.model_validate(doc)
    sources = load_source_tables(spec.input, ProjectResources(CASE))
    with pytest.raises(ExecutionPlanningError) as expected:
        plan_execution(spec, sources, supported_operations=OPERATIONS)
    result = execute_with_source_provider(spec, lambda _: pytest.fail("source read"))
    assert result.result.status == "failure"
    assert result.result.diagnostics == expected.value.diagnostics


@pytest.mark.parametrize("index", [0, 1])
def test_filter_scope_allows_promoted_column_defaults(specification, index):
    """REQ-1260 defaults read by filters become row-local in either template mode."""
    doc = specification.model_dump(exclude_unset=True)
    doc["rows"][index]["filter"] = "STUDYID IS NOT NULL"
    spec = Specification.model_validate(doc)
    admit(spec)
    sources = load_source_tables(spec.input, ProjectResources(CASE))
    plan = plan_execution(spec, sources, supported_operations=OPERATIONS)
    req, error = lower(plan, sources["LB"].table)
    assert error is None
    assert req["templates"][index]["filter"]["bindings"] == [
        {"name": "STUDYID", "read": {"column": 0}}
    ]
