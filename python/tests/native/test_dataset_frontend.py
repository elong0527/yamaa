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


@pytest.mark.parametrize(
    "operation,payload",
    [
        ("assert", {"expr": "str_contains(PARAMCD, 'COMP')"}),
        ("implies", {"when": "TRUE", "then": "AVAL = 9223372036854775808"}),
    ],
)
def test_unsupported_predicate_checks_never_read_sources(
    specification, operation, payload
):
    """Whole-run admission refuses unsupported predicate checks before provider effects."""
    doc = specification.model_dump(exclude_unset=True)
    doc["verifications"].append({operation: payload})
    result = execute_with_source_provider(
        Specification.model_validate(doc), lambda _: pytest.fail("source read")
    )
    assert isinstance(result.result, ExecutionUnsupported)
    assert result.result.features[0].operation in {
        "predicate_regex",
        "predicate_wide_integer_literal",
    }


def test_predicate_check_capability_is_required_before_provider(
    specification, monkeypatch
):
    """A row-filter-only native installation cannot start a predicate-check run."""
    doc = specification.model_dump(exclude_unset=True)
    doc["verifications"].append({"assert": {"expr": "TRUE"}})
    monkeypatch.setitem(
        sys.modules,
        "yamaa_native",
        SimpleNamespace(
            execute_dataset=lambda *_: pytest.fail("native execution"),
            dataset_capabilities=lambda: (
                '{"protocol":"dataset/1","features":["row_filter"]}'
            ),
        ),
    )
    result = execute_with_source_provider(
        Specification.model_validate(doc), lambda _: pytest.fail("source read")
    )
    assert isinstance(result.result, ExecutionUnsupported)
    assert [
        (feature.operation, feature.spec_path) for feature in result.result.features
    ] == [("native_predicate_checks", "verifications[2].assert")]


@pytest.mark.parametrize(
    "text,condition,context",
    [
        ("AVAL >", "invalid_predicate", None),
        ("z = a", "unknown_field", {"identifier": "a"}),
        (None, "invalid_declaration", {"reason": "a predicate must be text"}),
    ],
)
def test_later_implication_declaration_retains_native_antecedent_validation(
    specification, text, condition, context
):
    """Lowering defers then-side errors until Rust validates the when declaration."""
    doc = specification.model_dump(exclude_unset=True)
    doc["verifications"].append({"implies": {"when": "AVAL = 'bad'", "then": text}})
    spec = Specification.model_validate(doc)
    admit(spec)
    sources = load_source_tables(spec.input, ProjectResources(CASE))
    plan = plan_execution(spec, sources, supported_operations=OPERATIONS)
    req, error = lower(plan, sources["LB"].table)
    assert error.condition == condition
    assert error.spec_paths == ("verifications[2].implies.then",)
    if context is not None:
        assert error.context == context
    assert len(req["verifications"]) == 3
    checkpoint = req["verifications"][2]
    assert checkpoint["path"] == "verifications[2].implies"
    assert set(checkpoint["check"]) == {"predicate_declaration"}
    assert (
        checkpoint["check"]["predicate_declaration"]["path"]
        == "verifications[2].implies.when"
    )


def key_specification(tmp_path, rows=None):
    """Normalize an independently authored key-only plan with reversed identity order."""
    import yaml

    document = {
        "schema_version": "1.0",
        "domain": "KEYS",
        "keys": ["TAG", "ID"],
        "input": {"SRC": "input.csv"},
        "output": {"path": "out.csv", "columns": ["ID", "TAG", "VALUE"]},
        "columns": [
            {"name": "ID", "type": "int", "label": "Identity", "derivation": "SRC.id"},
            {"name": "TAG", "type": "str", "label": "Tag", "derivation": "SRC.tag"},
            {
                "name": "VALUE",
                "type": "int",
                "label": "Value",
                "derivation": "SRC.value",
            },
        ],
    }
    if rows is not None:
        document["rows"] = rows
    path = tmp_path / "spec.yaml"
    path.write_text(yaml.safe_dump(document), encoding="utf-8")
    return load_specification(path, ROOT / "yaml").specification


@pytest.mark.parametrize("rows", [None, []])
def test_key_grain_lowering_matches_independent_bound_plan(tmp_path, rows):
    """Absent and empty templates both bind named keys before collected source values."""
    import polars as pl
    import pyarrow as pa

    from yamaa.models import TypedColumn, TypedTable

    spec = key_specification(tmp_path, rows)
    fixtures = ROOT / "rust/crates/yamaa-adapters/tests/fixtures/datasets"
    source = TypedTable(
        columns=tuple(
            TypedColumn(name=name, type="str") for name in ("id", "tag", "value")
        ),
        frame=pl.from_arrow(
            pa.ipc.open_stream(fixtures / "key-grain.arrow").read_all()
        ),
    )
    expected = next(
        case["request"]
        for case in json.loads((fixtures / "expected.json").read_text())
        if case["case"] == "key_grain_converted_identity"
    )
    admit(spec)
    plan = plan_execution(spec, {"SRC": source}, supported_operations=OPERATIONS)
    assert lower(plan, source) == (expected, None)


def test_old_native_package_refuses_key_grain_before_provider(tmp_path, monkeypatch):
    """Earlier installed predicate support must not silently select driver records."""
    spec = key_specification(tmp_path)
    effects = []
    monkeypatch.setitem(
        sys.modules,
        "yamaa_native",
        SimpleNamespace(
            execute_dataset=lambda *_: effects.append("execute"),
            dataset_capabilities=lambda: json.dumps(
                {
                    "protocol": "dataset/1",
                    "features": ["row_filter", "predicate_checks"],
                }
            ),
        ),
    )
    result = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert result.result.status == "unsupported"
    assert [(f.operation, f.spec_path) for f in result.result.features] == [
        ("native_key_grain", "rows")
    ]
    assert effects == []


def numbering_specification(tmp_path, mutate=None):
    """Select only the unchanged rank columns and their source dependencies."""
    import yaml

    document = yaml.safe_load(
        (ROOT / "benchmarks/schema-window-functions/spec.yaml").read_text()
    )
    document["columns"] = [
        column
        for column in document["columns"]
        if column["name"]
        in {
            "STUDYID",
            "USUBJID",
            "VISITN",
            "VISIT",
            "VSDTC",
            "TRTSDT",
            "VSSTRESN",
            "VSEVAL",
            "SEVRANKC",
            "SEVRANKD",
        }
    ]
    document["output"]["columns"] = [column["name"] for column in document["columns"]]
    if mutate is not None:
        mutate(document)
    path = tmp_path / "numbering.yaml"
    path.write_text(yaml.safe_dump(document, sort_keys=False))
    return load_specification(path, ROOT / "yaml").specification


def test_numbering_lowers_expanded_named_windows(tmp_path):
    """Named windows bind to completed outputs with explicit direction and null defaults."""
    spec = numbering_specification(tmp_path)
    admit(spec)
    sources = load_source_tables(
        spec.input, ProjectResources(ROOT / "benchmarks/schema-window-functions")
    )
    plan = plan_execution(spec, sources, supported_operations=OPERATIONS)
    lowered, error = lower(plan, sources["VS"].table)
    assert error is None
    assert [column["expression"] for column in lowered["columns"][-2:]] == [
        {
            "number": {
                "kind": kind,
                "group_by": [1],
                "order_by": [{"column": 6, "descending": True, "nulls_first": False}],
            }
        }
        for kind in ["competition", "dense"]
    ]


@pytest.mark.parametrize("feature", ["regex", "qualified", "rows", "key"])
def test_numbering_unsupported_scope_precedes_provider(tmp_path, feature):
    """Broader valid windows remain explicit unsupported declarations without source effects."""

    def mutate(document):
        """Change one scope feature in the real benchmark declaration."""
        if feature == "regex":
            document["windows"]["RESULT_ORDER"]["filter"] = "str_contains(VSEVAL, 'Y')"
        elif feature == "qualified":
            document["windows"]["RESULT_ORDER"]["order_by"] = ["VS.VSSTRESN"]
        elif feature == "key":
            document["keys"].append("SEVRANKC")
        else:
            document["rows"] = [{"id": "driver", "dataset": "VS", "derivations": {}}]

    spec = numbering_specification(tmp_path, mutate)
    effects = []
    result = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert result.result.status == "unsupported"
    expected = {
        "regex": "predicate_regex",
        "qualified": "window_source",
        "rows": "window_scope",
        "key": "window_key",
    }[feature]
    assert expected in {f.operation for f in result.result.features}
    assert effects == []


def test_old_native_package_refuses_numbering_before_provider(tmp_path, monkeypatch):
    """Key-grain support alone does not imply window execution capability."""
    spec = numbering_specification(tmp_path)
    effects = []
    monkeypatch.setitem(
        sys.modules,
        "yamaa_native",
        SimpleNamespace(
            execute_dataset=lambda *_: effects.append("execute"),
            dataset_capabilities=lambda: json.dumps(
                {"protocol": "dataset/1", "features": ["key_grain"]}
            ),
        ),
    )
    result = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert result.result.status == "unsupported"
    assert {feature.operation for feature in result.result.features} == {
        "native_window_numbering"
    }
    assert effects == []


def test_window_filter_lowering_keeps_runtime_owner_and_completed_bindings(tmp_path):
    """Runtime predicate failures belong to the window operation, not its filter field."""

    def mutate(document):
        """Attach a valid scalar predicate to the normalized named window."""
        document["windows"]["RESULT_ORDER"]["filter"] = "VSEVAL = 'Y'"

    spec = numbering_specification(tmp_path, mutate)
    admit(spec)
    sources = load_source_tables(
        spec.input, ProjectResources(ROOT / "benchmarks/schema-window-functions")
    )
    request, error = lower(
        plan_execution(spec, sources, supported_operations=OPERATIONS),
        sources["VS"].table,
    )
    assert error is None
    for column in request["columns"][-2:]:
        predicate = column["expression"]["number"]["filter"]
        assert predicate["path"] == column["path"]
        assert predicate["bindings"] == [{"name": "VSEVAL", "read": {"column": 7}}]


def test_old_native_package_refuses_window_filter_before_provider(
    tmp_path, monkeypatch
):
    """Unfiltered numbering support never silently ignores an eligibility filter."""

    def mutate(document):
        """Attach eligibility while preserving the admitted numbering scope."""
        document["windows"]["RESULT_ORDER"]["filter"] = "VSEVAL = 'Y'"

    spec = numbering_specification(tmp_path, mutate)
    effects = []
    monkeypatch.setitem(
        sys.modules,
        "yamaa_native",
        SimpleNamespace(
            execute_dataset=lambda *_: effects.append("execute"),
            dataset_capabilities=lambda: json.dumps(
                {"protocol": "dataset/1", "features": ["key_grain", "window_numbering"]}
            ),
        ),
    )
    result = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert result.result.status == "unsupported"
    assert {feature.operation for feature in result.result.features} == {
        "native_window_filter"
    }
    assert effects == []


@pytest.mark.parametrize("operation", ["row_value", "previous_non_missing", "locf"])
def test_value_windows_lower_completed_donor_and_scope(tmp_path, operation):
    """Value windows retain explicit donor bindings, signed offsets and normalized named scope."""

    def mutate(document):
        """Replace one rank with a supported value read over the same completed data."""
        payload = {"source": "VSSTRESN", "window": "RESULT_ORDER"}
        if operation == "row_value":
            payload["offset"] = -1
        document["columns"][-2]["derivation"] = {operation: payload}

    spec = numbering_specification(tmp_path, mutate)
    admit(spec)
    sources = load_source_tables(
        spec.input, ProjectResources(ROOT / "benchmarks/schema-window-functions")
    )
    request, error = lower(
        plan_execution(spec, sources, supported_operations=OPERATIONS),
        sources["VS"].table,
    )
    assert error is None
    expected = {"column": 6}
    if operation == "row_value":
        expected["offset"] = "-1"
    assert request["columns"][-2]["expression"]["window"] == {
        "kind": {operation: expected},
        "group_by": [1],
        "order_by": [{"column": 6, "descending": True, "nulls_first": False}],
    }


def test_zero_window_offset_keeps_validation_condition_before_provider(tmp_path):
    """Zero is a language validation failure, not a native request error or missing read."""

    def mutate(document):
        """Declare a syntactically valid but forbidden zero offset."""
        document["columns"][-2]["derivation"] = {
            "row_value": {"source": "VSSTRESN", "offset": 0, "window": "RESULT_ORDER"}
        }

    spec = numbering_specification(tmp_path, mutate)
    effects = []
    result = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert result.result.status == "failure"
    assert [(d.condition, d.requirement) for d in result.result.diagnostics] == [
        ("zero_offset", "REQ-0328")
    ]
    assert effects == []


def test_old_native_package_refuses_value_windows_before_provider(
    tmp_path, monkeypatch
):
    """Numbering and filtering capabilities do not imply donor-value operations."""

    def mutate(document):
        """Select an admitted completed-output donor."""
        document["columns"][-2]["derivation"] = {
            "locf": {"source": "VSSTRESN", "window": "RESULT_ORDER"}
        }

    spec = numbering_specification(tmp_path, mutate)
    effects = []
    monkeypatch.setitem(
        sys.modules,
        "yamaa_native",
        SimpleNamespace(
            execute_dataset=lambda *_: effects.append("execute"),
            dataset_capabilities=lambda: json.dumps(
                {
                    "protocol": "dataset/1",
                    "features": ["key_grain", "window_numbering", "window_filter"],
                }
            ),
        ),
    )
    result = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert result.result.status == "unsupported"
    assert {feature.operation for feature in result.result.features} == {
        "native_window_values"
    }
    assert effects == []


@pytest.mark.parametrize("global_scope", [False, True])
def test_baseline_lowers_temporal_completed_dependencies(tmp_path, global_scope):
    """Baseline has no ordering and can use a named partition or the global output."""

    def mutate(document):
        payload = {"date": "VSDTC", "reference_date": "TRTSDT"}
        if not global_scope:
            payload["window"] = {"group_by": ["USUBJID"]}
        document["columns"][-2]["derivation"] = {"baseline_flag": payload}

    spec = numbering_specification(tmp_path, mutate)
    admit(spec)
    sources = load_source_tables(
        spec.input, ProjectResources(ROOT / "benchmarks/schema-window-functions")
    )
    request, error = lower(
        plan_execution(spec, sources, supported_operations=OPERATIONS),
        sources["VS"].table,
    )
    assert error is None
    assert request["columns"][-2]["expression"]["window"] == {
        "kind": {"baseline_flag": {"date": 4, "reference_date": 5}},
        "group_by": [] if global_scope else [1],
        "order_by": [],
    }


@pytest.mark.parametrize("scenario", ["types", "qualified", "ordered", "capability"])
def test_baseline_admission_precedes_provider(tmp_path, monkeypatch, scenario):
    """Unsupported temporal scope and ordering validation never consume source input."""

    def mutate(document):
        payload = {"date": "VSDTC", "reference_date": "TRTSDT"}
        if scenario == "types":
            payload.update(date="VISITN", reference_date="VISITN")
        elif scenario == "qualified":
            payload["date"] = "VS.VSDTC"
        elif scenario == "ordered":
            payload["window"] = {"order_by": ["VISITN"]}
        document["columns"][-2]["derivation"] = {"baseline_flag": payload}

    spec = numbering_specification(tmp_path, mutate)
    effects = []
    monkeypatch.setitem(
        sys.modules,
        "yamaa_native",
        SimpleNamespace(
            execute_dataset=lambda *_: effects.append("execute"),
            dataset_capabilities=lambda: json.dumps(
                {
                    "protocol": "dataset/1",
                    "features": [
                        "key_grain",
                        "window_numbering",
                        "window_filter",
                        "window_values",
                    ],
                }
            ),
        ),
    )
    result = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    if scenario == "ordered":
        assert result.result.status == "failure"
        assert [(d.condition, d.requirement) for d in result.result.diagnostics] == [
            ("window_order_by_forbidden", "REQ-0341")
        ]
    else:
        assert result.result.status == "unsupported"
        expected = {
            "types": "baseline_temporal_types",
            "qualified": "window_source",
            "capability": "native_window_baseline",
        }[scenario]
        assert expected in {f.operation for f in result.result.features}
    assert effects == []
