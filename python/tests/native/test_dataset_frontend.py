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


def _native_module(**members):
    """Facade-only double; installed tests separately forbid reference graph algorithms."""
    from yamaa.planning.execution import _find_cycle, _topological_row_order

    def analyze(request):
        """Implement the fake wire service while preserving unrelated facade test truth."""
        graph = json.loads(request)["dependencies"]
        names = [str(index) for index in range(len(graph))]
        dependencies = {
            name: [names[index] for index in edges]
            for name, edges in zip(names, graph, strict=True)
        }
        cycle = _find_cycle(names, dependencies)
        planned = {
            name: SimpleNamespace(column=int(name), dependencies=edges)
            for name, edges in dependencies.items()
        }
        return json.dumps(
            {
                "protocol": "dependency-analysis/1",
                "outcome": {
                    "status": "complete",
                    "cycle": None if cycle is None else [int(name) for name in cycle],
                    "order": [
                        item.column for item in _topological_row_order(planned, names)
                    ],
                },
            }
        )

    def analyze_columns(request):
        """Facade double only; installed rule tests use independent expected diagnostics."""
        payload = json.loads(request)
        graph = payload["dependencies"]
        active = {index for index, edges in enumerate(graph) if edges is not None}
        outcome = json.loads(
            analyze(
                json.dumps(
                    {
                        "dependencies": [
                            [edge for edge in edges or () if edge in active]
                            for edges in graph
                        ]
                    }
                )
            )
        )["outcome"]
        diagnostics = []

        def emit(kind, requirement, location, columns):
            """Encode the wire shape used by unrelated facade tests."""
            diagnostics.append(
                {
                    "condition": kind,
                    "requirement": requirement,
                    "location": location,
                    "columns": columns,
                }
            )

        cycle = outcome["cycle"] or []
        if cycle:
            emit("dependency_cycle", "REQ-0072", "operation", cycle)
        keys = payload["keys"]
        for column, edges in enumerate(graph):
            if column not in cycle:
                for dependency in edges or ():
                    if dependency not in keys and dependency >= column:
                        emit(
                            "forward_reference",
                            "REQ-0071",
                            "operation",
                            [column, dependency],
                        )
        if not payload["has_rows"]:
            for key in keys:
                if graph[key] is None:
                    emit("key_dependency", "REQ-0074", "declaration", [key])
                else:
                    for dependency in graph[key]:
                        if dependency not in keys:
                            emit(
                                "key_dependency",
                                "REQ-0074",
                                "expression",
                                [key, dependency],
                            )
        return json.dumps(
            {
                "protocol": "column-dependencies/1",
                "outcome": {
                    "status": "complete",
                    "order": [n for n in outcome["order"] if n in active],
                    "diagnostics": diagnostics,
                },
            }
        )

    def compile_references(request):
        """Facade-only fake; installed tests require the actual Rust compiler and truth."""
        catalog = json.loads(request)["catalog"]
        outputs = {field["name"]: field["type"] for field in catalog["outputs"]}
        datasets = {
            dataset["name"]: {
                field["name"]: field["type"] for field in dataset["fields"]
            }
            for dataset in catalog["datasets"]
        }
        output_names = tuple(outputs)
        dataset_names = tuple(datasets)

        def qualified(query):
            """Use reference rules only for the facade double, never for native expected truth."""
            from yamaa.models import TypedColumn
            from yamaa.odm.bindings import BindingPlan, DatasetBinding
            from yamaa.planning.execution import (
                _Reference,
                _validate_direct_qualified_reference,
            )
            from yamaa.specification.models import Row

            scope = query["scope"]
            phase = scope["phase"]
            qualifier = query["name"].split(".", 1)[0]
            bindings = BindingPlan(
                domain="OUT",
                output_columns=output_names,
                datasets={
                    name: DatasetBinding(
                        dataset=name,
                        columns=tuple(
                            TypedColumn(name=field, type=kind)
                            for field, kind in fields.items()
                        ),
                    )
                    for name, fields in datasets.items()
                },
            )
            findings = []
            _validate_direct_qualified_reference(
                _Reference(
                    query["name"],
                    "test",
                    expected_type=query["expected"],
                    current_driver=scope["current_driver"],
                    reach=scope["reach"],
                    join_relation=qualifier if scope["joined"] else None,
                ),
                scope["drivers"],
                bindings,
                outputs,
                findings,
                intermediates={},
                row=Row(id="row", derivations={}, group_by=phase["group_by"])
                if phase["kind"] == "row"
                else None,
                grouped_by_driver={qualifier: phase.get("groups", [])},
            )
            results = []
            for finding in findings:
                if finding.condition == "unknown_field":
                    results.append(
                        {
                            "kind": "driver_mismatch"
                            if "drivers" in finding.context
                            else "unknown_field"
                        }
                    )
                elif finding.condition == "phase_boundary":
                    results.append({"kind": "row_phase"})
                elif finding.condition == "ungrouped_driver_field":
                    results.append(
                        {
                            "kind": "row_group"
                            if finding.requirement == "REQ-0067"
                            else "column_group"
                        }
                    )
                else:
                    assert finding.condition == "incompatible_input_type"
                    results.append(
                        {
                            "kind": finding.condition,
                            "expected": finding.context["expected"],
                            "actual": finding.context["actual"],
                        }
                    )
            return {"kind": "qualified_validation", "diagnostics": results}

        def intermediate(query):
            """Use reference rules for the facade double, without producing native expected truth."""
            from yamaa.planning.execution import (
                _IntermediateRead,
                _Reference,
                _reference_intermediate_reads,
                _reference_intermediate_visibility,
            )

            def target(wire):
                if wire is None:
                    return None
                source = wire["source"]
                return SimpleNamespace(
                    dataset="SELF" if source["kind"] == "self" else source["name"],
                    self_fields=tuple(source.get("fields", ())),
                    derived=tuple((name, None) for name in wire["derived"]),
                    readable_columns=tuple(wire["readable"]),
                    dependencies=tuple(wire["dependencies"]),
                )

            findings = []
            if query["kind"] == "validate_intermediate":
                _reference_intermediate_visibility(
                    _Reference("lookup." + query["field"], "test"),
                    target(query["target"]),
                    SimpleNamespace(
                        datasets={
                            name: SimpleNamespace(field_names=tuple(fields))
                            for name, fields in datasets.items()
                        }
                    ),
                    findings,
                )
                diagnostics = [{"kind": "unknown_field"} for _ in findings]
            else:
                read = query["read"]
                value = target(read["target"])
                planned = {read["target_name"]: value} if value is not None else {}
                _reference_intermediate_reads(
                    planned,
                    {
                        read["reader"]: [
                            _IntermediateRead(
                                read["reader"],
                                read["donor_dataset"],
                                read["target_name"],
                                read["field"],
                                "test",
                                frozenset(read["visible"]),
                            )
                        ]
                    },
                    datasets,
                    findings,
                )
                diagnostics = []
                for finding in findings:
                    if finding.condition == "phase_boundary":
                        diagnostics.append({"kind": "self_phase"})
                    elif finding.requirement == "REQ-0125":
                        diagnostics.append({"kind": "unknown_field"})
                    else:
                        diagnostics.append(
                            {
                                "kind": "unavailable_dependency",
                                "dependency": value.dependencies.index(
                                    finding.context["identifier"]
                                ),
                            }
                        )
            return {"kind": "intermediate_validation", "diagnostics": diagnostics}

        def analyze_references(request):
            """Evaluate metadata in the unrelated facade double, never as expected truth."""
            results = []
            for query in json.loads(request)["queries"]:
                if query["kind"] == "bind_relation":
                    name = query["name"]
                    results.append(
                        {
                            "kind": "relation_binding",
                            "dataset": dataset_names.index(name)
                            if name in datasets
                            else None,
                        }
                    )
                    continue
                if query["kind"] == "match_value_type":
                    from yamaa.models import TypedColumn
                    from yamaa.odm.bindings import BindingPlan, DatasetBinding
                    from yamaa.planning.execution import (
                        _reference_match_value_result_type,
                    )
                    from yamaa.specification.models import Expression

                    metadata = query["expression"]
                    if metadata["kind"] == "source":
                        root = {"source": metadata["name"]}
                    elif metadata["kind"] == "literal":
                        root = {
                            "literal": {
                                "str": "x",
                                "int": 1,
                                "float": 1.5,
                                "bool": True,
                                "missing": None,
                                "other": [],
                            }[metadata["scalar"]]
                        }
                    elif metadata["kind"] == "operation":
                        root = {metadata["name"]: None}
                    else:
                        root = {"literal": None}
                    bindings = BindingPlan(
                        domain="OUT",
                        output_columns=tuple(outputs),
                        datasets={
                            name: DatasetBinding(
                                dataset=name,
                                columns=tuple(
                                    TypedColumn(name=field, type=kind)
                                    for field, kind in fields.items()
                                ),
                            )
                            for name, fields in datasets.items()
                        },
                    )
                    results.append(
                        {
                            "kind": "match_value_type",
                            "result_type": _reference_match_value_result_type(
                                Expression(root=root), bindings, outputs
                            ),
                        }
                    )
                    continue
                if query["kind"] == "comparable_types":
                    from yamaa.planning.execution import _reference_comparable_types

                    results.append(
                        {
                            "kind": "comparable_types",
                            "comparable": _reference_comparable_types(
                                query["left"], query["right"]
                            ),
                        }
                    )
                    continue
                if query["kind"] == "infer_keys":
                    from yamaa.planning.execution import _reference_applicable_keys

                    names = tuple(query["keys"])
                    finding = _reference_applicable_keys(
                        names,
                        outputs,
                        {field["name"]: field["type"] for field in query["fields"]},
                    )
                    inference = {"kind": finding.kind}
                    if finding.kind == "keys":
                        inference["keys"] = [names.index(key) for key in finding.keys]
                    elif finding.key is not None:
                        inference["key"] = names.index(finding.key)
                    if finding.kind == "incompatible":
                        inference.update(
                            expected=finding.expected, actual=finding.actual
                        )
                    results.append({"kind": "key_inference", "inference": inference})
                    continue
                if query["kind"] in (
                    "validate_intermediate",
                    "validate_intermediate_read",
                ):
                    results.append(intermediate(query))
                    continue
                name = query["name"]
                if query["kind"] == "validate_qualified":
                    results.append(qualified(query))
                    continue
                if query["kind"] == "bind":
                    bound = None
                    if "." not in name and name in outputs:
                        bound = {
                            "kind": "output",
                            "column": output_names.index(name),
                            "type": outputs[name],
                        }
                    elif "." in name:
                        dataset, field = name.split(".", 1)
                        if dataset in datasets and field in datasets[dataset]:
                            bound = {
                                "kind": "dataset",
                                "dataset": dataset_names.index(dataset),
                                "field": tuple(datasets[dataset]).index(field),
                                "type": datasets[dataset][field],
                            }
                    results.append({"kind": "binding", "binding": bound})
                    continue
                finding = None
                if name not in outputs:
                    candidates = sorted(
                        dataset_names[index]
                        for index in query["candidates"]
                        if name in datasets[dataset_names[index]]
                    )
                    finding = (
                        {
                            "condition": "unresolvable_name",
                            "dataset": dataset_names.index(candidates[0]),
                        }
                        if candidates
                        else {"condition": "unknown_field"}
                    )
                elif (
                    query["available"] is not None
                    and output_names.index(name) not in query["available"]
                ):
                    finding = {
                        "condition": "phase_boundary",
                        "column": output_names.index(name),
                    }
                elif (
                    query["expected"] is not None and outputs[name] != query["expected"]
                ):
                    finding = {
                        "condition": "incompatible_input_type",
                        "expected": query["expected"],
                        "actual": outputs[name],
                    }
                results.append({"kind": "validation", "diagnostic": finding})
            return json.dumps(
                {
                    "protocol": "reference-analysis/1",
                    "outcome": {"status": "complete", "results": results},
                }
            )

        return SimpleNamespace(analyze=analyze_references), json.dumps(
            {
                "protocol": "reference-catalog/1",
                "outcome": {"status": "complete", "results": []},
            }
        )

    def analyze_aggregate(request):
        """Facade-only syntax double; installed tests forbid the reference parser."""
        from yamaa.expressions.aggregate import AggregateError
        from yamaa.planning.aggregate_syntax import analyze_aggregate as analyze_syntax

        text = json.loads(request)["expression"]
        try:
            syntax = analyze_syntax(text)
        except AggregateError as error:
            outcome = {
                "status": "invalid",
                "condition": error.condition,
                "requirement": error.requirement,
                "context": error.context,
                "position": {
                    "character": error.position,
                    "byte": len(text[: error.position].encode()),
                },
            }
        else:
            outcome = {
                "status": "parsed",
                "ast": syntax.ast,
                "identifiers": syntax.identifiers,
                "star_datasets": syntax.star_datasets,
                "ungrouped_identifiers": syntax.ungrouped_identifiers,
            }
        return json.dumps({"protocol": "aggregate-syntax/1", "outcome": outcome})

    def analyze_numeric(request):
        """Facade-only syntax double; installed tests forbid the reference parser."""
        from yamaa.expressions.numeric import NumericError
        from yamaa.planning.numeric_syntax import analyze_numeric as analyze_syntax

        text = json.loads(request)["expression"]
        try:
            syntax = analyze_syntax(text)
        except NumericError as error:
            outcome = {
                "status": "invalid",
                "condition": error.condition,
                "requirement": error.requirement,
                "context": error.context,
                "position": {
                    "character": error.position,
                    "byte": len(text[: error.position].encode()),
                },
            }
        else:
            outcome = {
                "status": "parsed",
                "ast": syntax.ast,
                "identifiers": syntax.identifiers,
            }
        return json.dumps({"protocol": "numeric-syntax/1", "outcome": outcome})

    def analyze_predicate(request):
        """Facade-only syntax double; installed tests forbid the reference parser."""
        from yamaa.expressions import PredicateError
        from yamaa.planning.predicate_syntax import analyze_predicate as analyze_syntax

        text = json.loads(request)["expression"]
        try:
            syntax = analyze_syntax(text)
        except PredicateError as error:
            outcome = {
                "status": "invalid",
                "message": str(error).rsplit(" at character ", 1)[0],
                "condition": error.condition,
                "requirement": error.requirement,
                "context": {},
                "position": {
                    "character": error.position,
                    "byte": len(text[: error.position].encode()),
                },
            }
        else:
            outcome = {
                "status": "parsed",
                "ast": syntax.ast,
                "identifiers": syntax.identifiers,
            }
        return json.dumps({"protocol": "predicate-syntax/1", "outcome": outcome})

    return SimpleNamespace(
        reference_capabilities=lambda: json.dumps(
            {
                "protocol": "reference-analysis/1",
                "features": [
                    "qualified_validation",
                    "intermediate_validation",
                    "key_relations",
                    "match_value_typing",
                    "relation_binding",
                ],
            }
        ),
        analyze_aggregate=analyze_aggregate,
        analyze_numeric=analyze_numeric,
        analyze_predicate=analyze_predicate,
        analyze_dependencies=analyze,
        analyze_column_dependencies=analyze_columns,
        _compile_reference_catalog=compile_references,
        **members,
    )


@pytest.fixture(autouse=True)
def native_syntax_double(monkeypatch):
    """Facade tests need no native installation; installed suites qualify Rust ownership."""
    monkeypatch.setitem(
        sys.modules,
        "yamaa_native",
        _native_module(
            execute_dataset=lambda *_: pytest.fail("unadmitted execution"),
            dataset_capabilities=lambda: json.dumps(
                {
                    "protocol": "dataset/1",
                    "features": [
                        "row_filter",
                        "predicate_checks",
                        "root_filter",
                        "key_grain",
                        "window_numbering",
                        "window_filter",
                    ],
                }
            ),
        ),
    )


@pytest.fixture
def specification():
    """Load the actual schema-validated ADLB document, without reading source data."""
    return load_specification(CASE / "spec.yaml", ROOT / "yaml").specification


def test_older_reference_features_refuse_intermediate_queries_before_io(
    specification, monkeypatch
):
    """A wheel with qualified-field support alone cannot start the new planner path."""
    native = _native_module(execute_dataset=lambda *_: pytest.fail("execution"))
    native.reference_capabilities = lambda: json.dumps(
        {
            "protocol": "reference-analysis/1",
            "features": ["binding", "output_validation", "qualified_validation"],
        }
    )
    monkeypatch.setitem(sys.modules, "yamaa_native", native)
    result = execute_with_source_provider(
        specification, lambda _: pytest.fail("source read")
    )
    assert isinstance(result.result, ExecutionUnsupported)
    assert [(f.operation, f.spec_path) for f in result.result.features] == [
        ("native_intermediate_reference_validation", "$")
    ]
    assert result.result.handler_counts == ()
    assert result.verifications == ()


@pytest.mark.parametrize("analyzer", ["absent", None, False])
@pytest.mark.parametrize(
    "service, operation",
    [
        ("analyze_predicate", "native_predicate_syntax"),
        ("analyze_dependencies", "native_dependency_analysis"),
        ("analyze_column_dependencies", "native_column_dependency_analysis"),
        ("_compile_reference_catalog", "native_reference_binding"),
    ],
)
def test_missing_dependency_service_is_unsupported_before_io(
    specification, monkeypatch, analyzer, service, operation
):
    """A separately installed older wheel cannot abort a run or start source effects."""
    native = _native_module(execute_dataset=lambda *_: pytest.fail("execution"))
    delattr(native, service)
    if analyzer != "absent":
        setattr(native, service, analyzer)
    monkeypatch.setitem(sys.modules, "yamaa_native", native)
    result = execute_with_source_provider(
        specification, lambda _: pytest.fail("source read")
    )
    assert isinstance(result.result, ExecutionUnsupported)
    assert [(f.operation, f.spec_path) for f in result.result.features] == [
        (operation, "$")
    ]
    assert result.result.handler_counts == ()
    assert result.verifications == ()


@pytest.mark.parametrize(
    "capability",
    [
        None,
        False,
        {},
        {"protocol": "reference-analysis/1", "features": []},
        {"protocol": "future/2", "features": ["qualified_validation"]},
    ],
)
def test_qualified_reference_capability_is_checked_before_io(
    monkeypatch, specification, capability
):
    """An older query contract cannot enter source loading before explicit refusal."""
    native = _native_module(execute_dataset=lambda *_: pytest.fail("execution"))
    native.reference_capabilities = (
        (lambda: json.dumps(capability)) if isinstance(capability, dict) else capability
    )
    monkeypatch.setitem(sys.modules, "yamaa_native", native)
    result = execute_with_source_provider(
        specification, lambda _: pytest.fail("source read")
    )
    assert isinstance(result.result, ExecutionUnsupported)
    assert [(f.operation, f.spec_path) for f in result.result.features] == [
        ("native_qualified_reference_validation", "$")
    ]
    assert result.result.handler_counts == ()
    assert result.verifications == ()


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
        "root_regex",
        "predicate_regex",
        "predicate_wide_literal",
        "wide_handler",
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
        "multiple_row_drivers",
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
    if feature == "root_regex":
        doc["filter"] = "str_contains(LB.LBTESTCD, 'COMP')"
        doc["rows"] = None  # The older artifact lacks regex execution capability.
        for column in doc["columns"]:
            if "derivation" not in column:
                column["derivation"] = {"value": {"literal": None}}
    elif feature == "predicate_regex":
        doc["rows"][0]["filter"] = "str_contains(LB.LBTESTCD, 'COMP')"
    elif feature == "predicate_wide_literal":
        doc["rows"][0]["filter"] = "AVAL > 9223372036854775808"
    elif feature == "wide_handler":
        doc["rows"][0]["derivations"]["AVAL"]["unconvertible"] = 9223372036854775808
    elif feature in {"source_filter", "source_handler"}:
        payload = {"variable": "LB.LBSTRESN"}
        payload["filter" if feature == "source_filter" else "no_match"] = (
            "LB.LBSTRESN > 0" if feature == "source_filter" else None
        )
        doc["rows"][0]["derivations"]["AVAL"]["value"] = {"source": payload}
    elif feature == "compute":
        doc["rows"][0]["derivations"]["AVAL"]["value"] = {"compute": "POWER(2, 3)"}
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
    elif feature == "multiple_row_drivers":
        doc["input"]["OTHER"] = {"path": "other.csv"}
        for row in doc["rows"]:
            row["dataset"] = "LB"
        doc["rows"][0]["dataset"] = "OTHER"
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
    monkeypatch.setitem(sys.modules, "yamaa_native", _native_module())
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
    native = _native_module(execute_dataset=lambda *_: pytest.fail("execution"))
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
        _native_module(
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
        "native_predicate_regex",
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
        _native_module(
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
        _native_module(
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
        "regex": "native_predicate_regex",
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
        _native_module(
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
        _native_module(
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
        _native_module(
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
        _native_module(
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


def test_root_filter_lowers_before_key_construction(tmp_path):
    """The root filter binds qualified source fields, with original predicate provenance."""

    def mutate(document):
        document["filter"] = "VS.VISITN > 1"

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
    predicate = request["templates"][0]["filter"]
    assert predicate["path"] == "filter"
    index = next(
        i for i, c in enumerate(sources["VS"].table.columns) if c.name == "VISITN"
    )
    assert predicate["bindings"] == [{"name": "VS.VISITN", "read": {"source": index}}]


def test_old_native_package_refuses_root_filter_before_provider(tmp_path, monkeypatch):
    """A key-grain package with row filters cannot implicitly execute root filtering."""

    def mutate(document):
        document["filter"] = "VS.VISITN > 1"

    spec = numbering_specification(tmp_path, mutate)
    effects = []
    monkeypatch.setitem(
        sys.modules,
        "yamaa_native",
        _native_module(
            execute_dataset=lambda *_: effects.append("execute"),
            dataset_capabilities=lambda: json.dumps(
                {
                    "protocol": "dataset/1",
                    "features": ["row_filter", "key_grain", "window_numbering"],
                }
            ),
        ),
    )
    result = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert result.result.status == "unsupported"
    assert {f.operation for f in result.result.features} == {"native_root_filter"}
    assert effects == []


def test_root_filter_cannot_read_output_before_keys(tmp_path):
    """An output-column root predicate is a phase error before provider effects."""

    def mutate(document):
        document["filter"] = "VISITN > 1"

    spec = numbering_specification(tmp_path, mutate)
    effects = []
    result = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert result.result.status == "failure"
    assert len(result.result.diagnostics) == 1
    diagnostic = result.result.diagnostics[0]
    assert diagnostic.condition == "phase_boundary"
    assert diagnostic.spec_paths == ("filter",)
    assert diagnostic.context == {
        "identifier": "VISITN",
        "available_phase": "column_derivation",
        "required_phase": "row_filter",
    }
    assert effects == []


def source_filter_specification(tmp_path, predicate="VS.VSEVAL = 'Y'"):
    """Filter one non-key source read in the standalone named benchmark."""

    def mutate(document):
        column = next(c for c in document["columns"] if c["name"] == "VSSTRESN")
        column["derivation"] = {
            "source": {"variable": "VS.VSSTRESN", "filter": predicate}
        }

    return numbering_specification(tmp_path, mutate)


def test_source_filter_lowers_source_scope_and_runtime_owner(tmp_path):
    """Reading eligibility uses source fields and the owning source operation path."""
    spec = source_filter_specification(tmp_path)
    admit(spec)
    sources = load_source_tables(
        spec.input, ProjectResources(ROOT / "benchmarks/schema-window-functions")
    )
    request, error = lower(
        plan_execution(spec, sources, supported_operations=OPERATIONS),
        sources["VS"].table,
    )
    assert error is None
    reading = next(c for c in request["columns"] if c["column"] == 6)["expression"][
        "collect"
    ]
    assert reading["identifier"] == "VS.VSSTRESN"
    assert reading["filter"]["path"] == "columns.VSSTRESN.derivation.source"
    index = next(
        i for i, c in enumerate(sources["VS"].table.columns) if c.name == "VSEVAL"
    )
    assert reading["filter"]["bindings"] == [
        {"name": "VS.VSEVAL", "read": {"source": index}}
    ]


@pytest.mark.parametrize("predicate", ["VSEVAL = 'Y'", "OTHER.VSEVAL = 'Y'"])
def test_source_filter_scope_errors_precede_provider(tmp_path, predicate):
    """Source selection never reads output values or fields from another dataset."""
    spec = source_filter_specification(tmp_path, predicate)
    effects = []
    actual = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert actual.result.status == "failure"
    assert [(d.condition, d.requirement) for d in actual.result.diagnostics] == [
        ("unknown_field", "REQ-0132")
    ]
    assert effects == []


def test_old_native_package_refuses_source_filter_before_provider(
    tmp_path, monkeypatch
):
    """Root/row filters do not imply eligibility inside collected source reads."""
    spec = source_filter_specification(tmp_path)
    effects = []
    monkeypatch.setitem(
        sys.modules,
        "yamaa_native",
        _native_module(
            execute_dataset=lambda *_: effects.append("execute"),
            dataset_capabilities=lambda: json.dumps(
                {
                    "protocol": "dataset/1",
                    "features": [
                        "key_grain",
                        "window_numbering",
                        "root_filter",
                        "row_filter",
                    ],
                }
            ),
        ),
    )
    actual = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert actual.result.status == "unsupported"
    assert {f.operation for f in actual.result.features} == {"native_source_filter"}
    assert effects == []


@pytest.mark.parametrize("predicate", ["TRUE", "("])
def test_output_read_rejects_filter_before_parsing_predicate(tmp_path, predicate):
    """A completed output value has no source records to filter, even with invalid syntax."""

    def mutate(document):
        """Replace a source read with an already completed output key."""
        next(c for c in document["columns"] if c["name"] == "VSSTRESN")[
            "derivation"
        ] = {"source": {"variable": "VISITN", "filter": predicate}}

    spec = numbering_specification(tmp_path, mutate)
    effects = []
    actual = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert actual.result.status == "failure"
    assert [(d.condition, d.requirement) for d in actual.result.diagnostics] == [
        ("prohibited_construct", "REQ-0148")
    ]
    assert effects == []


def source_order_specification(tmp_path, mutate=None):
    """Declare qualified source ordering on the benchmark's non-key reading."""

    def change(document):
        column = next(c for c in document["columns"] if c["name"] == "VSSTRESN")
        payload = {
            "variable": "VS.VSSTRESN",
            "order_by": [
                {"variable": "VS.VISITN", "direction": "desc", "nulls": "first"}
            ],
            "keep": "last",
        }
        if mutate is not None:
            mutate(payload)
        column["derivation"] = {"source": payload}

    return numbering_specification(tmp_path, change)


def test_source_order_lowers_qualified_source_coordinates(tmp_path):
    """Source ordering coordinates belong to the input schema, independent of output order."""
    spec = source_order_specification(
        tmp_path, lambda payload: payload.update(filter="VS.VSEVAL = 'Y'")
    )
    admit(spec)
    sources = load_source_tables(
        spec.input, ProjectResources(ROOT / "benchmarks/schema-window-functions")
    )
    source = sources["VS"].table
    request, error = lower(
        plan_execution(spec, sources, supported_operations=OPERATIONS), source
    )
    assert error is None
    reading = next(
        a
        for a in request["columns"]
        if a["path"] == "columns.VSSTRESN.derivation.source"
    )["expression"]["collect"]
    visit = next(i for i, c in enumerate(source.columns) if c.name == "VISITN")
    assert reading["selection"] == {
        "order_by": [{"column": visit, "descending": True, "nulls_first": True}],
        "keep": "last",
    }
    assert reading["filter"]["path"] == "columns.VSSTRESN.derivation.source"


@pytest.mark.parametrize(
    "mutate",
    [
        lambda p: p.pop("keep"),
        lambda p: p.pop("order_by"),
        lambda p: p.update(order_by=["VISITN"]),
        lambda p: p.update(order_by=["OTHER.VISITN"]),
        lambda p: p.update(variable="VISITN"),
    ],
)
def test_unqualified_source_selection_refuses_before_provider(tmp_path, mutate):
    """Unqualified policies remain unsupported without moving lazy reference errors."""
    spec = source_order_specification(tmp_path, mutate)
    effects = []
    actual = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert actual.result.status == "unsupported"
    assert effects == []


def test_old_native_package_refuses_source_selection_before_provider(
    tmp_path, monkeypatch
):
    """Key grain and source filtering do not imply ordered donor choice support."""
    spec = source_order_specification(tmp_path)
    effects = []
    monkeypatch.setitem(
        sys.modules,
        "yamaa_native",
        _native_module(
            execute_dataset=lambda *_: effects.append("execute"),
            dataset_capabilities=lambda: json.dumps(
                {
                    "protocol": "dataset/1",
                    "features": ["key_grain", "window_numbering", "source_filter"],
                }
            ),
        ),
    )
    actual = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert actual.result.status == "unsupported"
    assert {feature.operation for feature in actual.result.features} == {
        "native_source_selection"
    }
    assert effects == []


def multi_source_specification(tmp_path, payload="OTHER.V"):
    """Declare a base listed after its independent lookup relation."""
    import yaml

    document = {
        "schema_version": "1.0",
        "domain": "LOOKUP",
        "keys": ["ID"],
        "base": "SRC",
        "input": {"OTHER": "other.csv", "SRC": "source.csv"},
        "output": {"path": "out.csv", "columns": ["ID", "V"]},
        "columns": [
            {"name": "ID", "type": "int", "label": "ID", "derivation": "SRC.ID"},
            {
                "name": "V",
                "type": "int",
                "label": "V",
                "derivation": payload
                if isinstance(payload, str)
                else {"source": payload},
            },
        ],
    }
    path = tmp_path / "multi.yaml"
    path.write_text(yaml.safe_dump(document, sort_keys=False))
    return load_specification(path, ROOT / "yaml").specification


def named_specification(tmp_path, mutate=None):
    """Load the unchanged benchmark or a schema-valid unsupported selection variant."""
    import yaml

    case = ROOT / "benchmarks/schema-lookup"
    document = yaml.safe_load((case / "spec.yaml").read_text())
    if mutate is not None:
        mutate(document)
    path = tmp_path / "named.yaml"
    path.write_text(yaml.safe_dump(document, sort_keys=False))
    return load_specification(path, ROOT / "yaml").specification


def test_named_intermediate_lowering_preserves_match_and_reading_paths(tmp_path):
    """Different source/output key names and inherited reads stay explicit in the IR."""
    spec = named_specification(tmp_path)
    admit(spec)
    sources = load_source_tables(
        spec.input, ProjectResources(ROOT / "benchmarks/schema-lookup")
    )
    plan = plan_execution(spec, sources, supported_operations=OPERATIONS)
    request, pending = lower(
        plan,
        sources["DM"].table,
        {name: sources[name].table for name in ["AE", "MEDDRA"]},
    )
    assert pending is None
    first, second = request["intermediates"]
    assert first["identifier"] == "DEATHEV"
    assert first["path"] == "intermediates[0]"
    assert first["filter"]["path"] == "intermediates[0].filter"
    assert first["selection"]["keep"] == "last"
    assert first["no_match"] == {"missing": None}
    assert second["keys"] == [{"source_column": 0, "output_column": 2}]
    assert [
        assignment["expression"]["intermediate"]["index"]
        for assignment in request["columns"]
    ] == [0, 0, 1]
    assert request["columns"][0]["path"] == "columns.DTHDY.derivation.source"


def test_completed_column_name_can_match_named_intermediate(tmp_path):
    """A bare name remains an output read even when a named selector has that identifier."""

    def mutate(document):
        """Keep qualified intermediate reads alongside a distinct bare output read."""
        document["columns"].insert(
            1,
            {
                "name": "DEATHEV",
                "type": "int",
                "label": "Output",
                "derivation": {"literal": 17},
            },
        )
        document["columns"].append(
            {"name": "COPY", "type": "int", "label": "Copy", "derivation": "DEATHEV"}
        )

    spec = named_specification(tmp_path, mutate)
    admit(spec)
    sources = load_source_tables(
        spec.input, ProjectResources(ROOT / "benchmarks/schema-lookup")
    )
    plan = plan_execution(spec, sources, supported_operations=OPERATIONS)
    request, pending = lower(
        plan,
        sources["DM"].table,
        {name: sources[name].table for name in ["AE", "MEDDRA"]},
    )
    assert pending is None
    assert request["columns"][-1]["expression"] == {"column": 1}


@pytest.mark.parametrize(
    "mutate",
    [
        lambda doc: doc["intermediates"][0].update(dataset="DM"),
        lambda doc: doc["intermediates"][0].update(filter="AE.AEDY > DTHDY"),
        lambda doc: doc["intermediates"][0].update(order_by=["DM.USUBJID"]),
        lambda doc: doc["intermediates"][0].update(key={"USUBJID": "DM.USUBJID"}),
        lambda doc: doc["intermediates"][0].update(derivations={"X": {"literal": 1}}),
        lambda doc: doc["columns"][1].update(
            derivation={
                "source": {"variable": "DEATHEV.AEDY", "filter": "DEATHEV.AEDY > 0"}
            }
        ),
    ],
)
def test_unqualified_named_selection_refuses_before_provider(tmp_path, mutate):
    """The native frontend refuses broader intermediate semantics before loading tables."""
    spec = named_specification(tmp_path, mutate)
    effects = []
    actual = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert actual.result.status == "unsupported"
    assert effects == []


def test_old_native_package_refuses_named_intermediates_before_provider(
    tmp_path, monkeypatch
):
    """Multi-source support alone does not authorize named selection execution."""
    spec = named_specification(tmp_path)
    effects = []
    monkeypatch.setitem(
        sys.modules,
        "yamaa_native",
        _native_module(
            execute_dataset=lambda *_: effects.append("execute"),
            execute_dataset_sources=lambda *_: effects.append("execute"),
            dataset_capabilities=lambda: json.dumps(
                {"protocol": "dataset/1", "features": ["key_grain", "multi_source"]}
            ),
        ),
    )
    actual = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert actual.result.status == "unsupported"
    assert {feature.operation for feature in actual.result.features} == {
        "native_named_intermediate"
    }
    assert effects == []


def multi_source_tables(kind="int"):
    """Build independently typed snapshots without involving CSV ingestion."""
    from yamaa.io.polars import frame_from_values
    from yamaa.models import TypedColumn

    return {
        "OTHER": frame_from_values(
            (TypedColumn(name="ID", type=kind), TypedColumn(name="V", type="str")),
            [[2.0 if kind == "float" else 2, "7"]],
        ),
        "SRC": frame_from_values((TypedColumn(name="ID", type="str"),), [["02"]]),
    }


def test_completed_column_name_can_match_secondary_source(tmp_path):
    """Only a written dot chooses the secondary namespace; bare names read output columns."""
    import yaml

    multi_source_specification(tmp_path, "OTHER")
    path = tmp_path / "multi.yaml"
    document = yaml.safe_load(path.read_text())
    document["columns"].insert(
        1,
        {
            "name": "OTHER",
            "type": "int",
            "label": "Output",
            "derivation": {"literal": 11},
        },
    )
    path.write_text(yaml.safe_dump(document, sort_keys=False))
    spec = load_specification(path, ROOT / "yaml").specification
    admit(spec)
    sources = multi_source_tables()
    plan = plan_execution(spec, sources, supported_operations=OPERATIONS)
    request, pending = lower(plan, sources["SRC"], {"OTHER": sources["OTHER"]})
    assert pending is None
    assert request["columns"][-1]["expression"] == {"column": 1}


def test_secondary_source_lowering_uses_declared_base_and_inferred_keys(tmp_path):
    """Secondary coordinates and completed-output keys are explicit in the bound request."""
    spec = multi_source_specification(tmp_path)
    admit(spec)
    sources = multi_source_tables()
    request, error = lower(
        plan_execution(spec, sources, supported_operations=OPERATIONS),
        sources["SRC"],
        {"OTHER": sources["OTHER"]},
    )
    assert error is None
    assert request["source"] == [{"name": "ID", "kind": "str"}]
    assert request["secondary"] == [
        {
            "name": "OTHER",
            "schema": [{"name": "ID", "kind": "int"}, {"name": "V", "kind": "str"}],
        }
    ]
    assert request["columns"][0]["expression"] == {
        "lookup": {
            "source": 0,
            "column": 1,
            "keys": [{"source_column": 0, "output_column": 0}],
        }
    }


@pytest.mark.parametrize(
    "payload",
    [
        {"variable": "OTHER.V", "filter": "OTHER.ID > 0"},
        {"variable": "OTHER.V", "order_by": ["OTHER.ID"], "keep": "last"},
    ],
)
def test_secondary_selection_refuses_before_provider(tmp_path, payload):
    """Relation selection has different cardinality rules and remains explicitly unqualified."""
    spec = multi_source_specification(tmp_path, payload)
    effects = []
    actual = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert actual.result.status == "unsupported"
    assert "secondary_source_selection" in {
        feature.operation for feature in actual.result.features
    }
    assert effects == []


def test_old_native_package_refuses_multi_source_before_provider(tmp_path, monkeypatch):
    """Single-source capability never authorizes a multi-table input effect."""
    spec = multi_source_specification(tmp_path)
    effects = []
    monkeypatch.setitem(
        sys.modules,
        "yamaa_native",
        _native_module(
            execute_dataset=lambda *_: effects.append("execute"),
            dataset_capabilities=lambda: json.dumps(
                {"protocol": "dataset/1", "features": ["key_grain"]}
            ),
        ),
    )
    actual = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert actual.result.status == "unsupported"
    assert {feature.operation for feature in actual.result.features} == {
        "native_multi_source"
    }
    assert effects == []


def test_mixed_lookup_key_types_remain_unsupported_after_schema_binding(
    tmp_path, monkeypatch
):
    """Unqualified numeric cross-type equality cannot silently become a failed match."""
    spec = multi_source_specification(tmp_path)
    effects = []
    monkeypatch.setitem(
        sys.modules,
        "yamaa_native",
        _native_module(
            execute_dataset=lambda *_: effects.append("execute"),
            execute_dataset_sources=lambda *_: effects.append("execute_multiple"),
            dataset_capabilities=lambda: json.dumps(
                {"protocol": "dataset/1", "features": ["key_grain", "multi_source"]}
            ),
        ),
    )

    def provider(_):
        """Acquire exactly the declared tables before their key types can be checked."""
        effects.append("provider")
        return multi_source_tables("float")

    actual = execute_with_source_provider(spec, provider)
    assert actual.result.status == "unsupported"
    assert {feature.operation for feature in actual.result.features} == {
        "secondary_source_binding"
    }
    assert effects == ["provider"]


@pytest.mark.parametrize("field", ["keep", "order_by"])
def test_unpaired_named_policy_keeps_preflight_diagnostic(tmp_path, field):
    """Reference declaration errors take precedence over closed-slice refusal."""
    spec = named_specification(tmp_path, lambda doc: doc["intermediates"][0].pop(field))
    effects = []
    actual = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert actual.result.status == "failure"
    diagnostic = actual.result.diagnostics[0]
    assert (diagnostic.condition, diagnostic.requirement, diagnostic.spec_paths) == (
        "unpaired_fields",
        "REQ-0119",
        ("intermediates[0]",),
    )
    assert effects == []


def compute_specification(tmp_path, expression="X + X * 2", row=False):
    """Declare numeric dependencies independently of native compilation or table values."""
    import yaml

    document = {
        "schema_version": "1.0",
        "domain": "NUM",
        "keys": ["ID"],
        "input": {"SRC": "source.csv"},
        "output": {"path": "out.csv", "columns": ["ID", "X", "N"]},
        "columns": [
            {"name": "ID", "type": "int", "label": "ID", "derivation": "SRC.ID"},
            {"name": "X", "type": "int", "label": "X", "derivation": "SRC.X"},
            {
                "name": "N",
                "type": "float",
                "label": "N",
                "derivation": {"compute": {"expr": expression}},
            },
        ],
    }
    if row:
        document["columns"][-1].pop("derivation")
        document["rows"] = [
            {
                "id": "row",
                "dataset": "SRC",
                "derivations": {"N": {"compute": {"expr": expression}}},
            }
        ]
    path = tmp_path / "compute.yaml"
    path.write_text(yaml.safe_dump(document, sort_keys=False))
    return load_specification(path, ROOT / "yaml").specification


@pytest.mark.parametrize(
    "row,expr,expected",
    [(False, "X + X * 2", {"column": 1}), (True, "SRC.X + SRC.X * 2", {"source": 1})],
)
def test_numeric_lowering_preserves_text_and_distinct_bound_names(
    tmp_path, row, expr, expected
):
    """Repeated written occurrences share a binding without evaluating or reassociating syntax."""
    from yamaa.io.polars import frame_from_values
    from yamaa.models import TypedColumn

    spec = compute_specification(tmp_path, expr, row)
    admit(spec)
    source = frame_from_values(
        (TypedColumn(name="ID", type="int"), TypedColumn(name="X", type="int")),
        [[1, 7]],
    )
    plan = plan_execution(spec, {"SRC": source}, supported_operations=OPERATIONS)
    request, pending = lower(plan, source)
    assert pending is None
    items = request["templates"][0]["assignments"] if row else request["columns"]
    item = next(item for item in items if item["column"] == 2)
    assert item["expression"] == {
        "compute": {
            "text": expr,
            "bindings": [{"name": "SRC.X" if row else "X", "read": expected}],
        }
    }


@pytest.mark.parametrize("expr", ["EXP(X)", "LN(X)", "POWER(X, 2)"])
def test_unqualified_math_policy_refuses_before_provider(tmp_path, expr):
    """Dataset computation does not opt into the separate portable-math candidate."""
    spec = compute_specification(tmp_path, expr)
    effects = []
    actual = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert actual.result.status == "unsupported"
    assert effects == []


def test_old_native_package_refuses_computation_before_provider(tmp_path, monkeypatch):
    """A legacy dataset capability cannot silently reinterpret a numeric expression."""
    spec = compute_specification(tmp_path)
    effects = []
    monkeypatch.setitem(
        sys.modules,
        "yamaa_native",
        _native_module(
            execute_dataset=lambda *_: effects.append("execute"),
            dataset_capabilities=lambda: json.dumps(
                {"protocol": "dataset/1", "features": ["key_grain"]}
            ),
        ),
    )
    actual = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert actual.result.status == "unsupported"
    assert {feature.operation for feature in actual.result.features} == {
        "native_numeric_compute"
    }
    assert effects == []


def test_numeric_syntax_error_precedes_provider(tmp_path):
    """Grammar conditions keep their authored expression path before source acquisition."""
    spec = compute_specification(tmp_path, "X +")
    effects = []
    actual = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert actual.result.status == "failure"
    error = actual.result.diagnostics[0]
    assert (error.condition, error.requirement, error.spec_paths) == (
        "invalid_numeric_expression",
        "REQ-0439",
        ("columns.N.derivation.compute.expr",),
    )
    assert effects == []


@pytest.mark.parametrize("replacement", [None, 0, "bad"])
def test_handler_lowering_keeps_presence_and_authored_path(tmp_path, replacement):
    """Explicit null remains a declared fallback and all entries retain planned order."""
    from yamaa.io.polars import frame_from_values
    from yamaa.models import TypedColumn

    spec = compute_specification(tmp_path)
    document = spec.model_dump(exclude_unset=True)
    document["columns"][-1]["derivation"]["unconvertible"] = replacement
    spec = type(spec).model_validate(document)
    admit(spec)
    source = frame_from_values(
        (TypedColumn(name="ID", type="int"), TypedColumn(name="X", type="int")), []
    )
    plan = plan_execution(spec, {"SRC": source}, supported_operations=OPERATIONS)
    request, _ = lower(plan, source)
    assert request["unconvertible"] == [
        {
            "assignment_path": "columns.N.derivation.value.compute",
            "path": "columns.N.derivation.unconvertible",
            "value": {"missing": None}
            if replacement is None
            else {"int": "0"}
            if replacement == 0
            else {"str": "bad"},
        }
    ]


def test_old_native_package_refuses_conversion_handlers_before_provider(
    tmp_path, monkeypatch
):
    """A handler declaration cannot be dropped by a package without this capability."""
    spec = compute_specification(tmp_path)
    document = spec.model_dump(exclude_unset=True)
    document["columns"][-1]["derivation"]["unconvertible"] = None
    spec = type(spec).model_validate(document)
    effects = []
    monkeypatch.setitem(
        sys.modules,
        "yamaa_native",
        _native_module(
            execute_dataset=lambda *_: effects.append("execute"),
            dataset_capabilities=lambda: json.dumps(
                {"protocol": "dataset/1", "features": ["key_grain", "numeric_compute"]}
            ),
        ),
    )
    actual = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert actual.result.status == "unsupported"
    assert {feature.operation for feature in actual.result.features} == {
        "native_unconvertible"
    }
    assert effects == []


def test_key_grain_source_compute_refuses_before_provider(tmp_path):
    """Non-key computations cannot read source rows after key-grain creation."""
    spec = compute_specification(tmp_path, "SRC.X + 1")
    effects = []
    actual = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert actual.result.status == "unsupported"
    assert {feature.operation for feature in actual.result.features} == {
        "compute_binding_scope"
    }
    assert effects == []


@pytest.mark.parametrize("reverse", [False, True])
def test_row_lookup_lowering_uses_raw_driver_keys(reverse):
    """The complete BMI plan uses driver fields even when its input is not listed first."""
    case = ROOT / "benchmarks/adam-advs-bmi"
    spec = load_specification(case / "spec.yaml", ROOT / "yaml").specification
    if reverse:
        spec = spec.model_copy(
            update={"input": dict(reversed(list(spec.input.items())))}
        )
    admit(spec)
    sources = load_source_tables(spec.input, ProjectResources(case))
    plan = plan_execution(spec, sources, supported_operations=OPERATIONS)
    request, pending = lower(plan, sources["VS"].table, {"ADSL": sources["ADSL"].table})
    assert pending is None
    item = next(
        item for item in request["templates"][1]["assignments"] if item["column"] == 5
    )
    driver = {
        column.name: index for index, column in enumerate(sources["VS"].table.columns)
    }
    secondary = {
        column.name: index for index, column in enumerate(sources["ADSL"].table.columns)
    }
    assert item["expression"] == {
        "row_lookup": {
            "source": 0,
            "column": secondary["HEIGHTBL"],
            "keys": [
                {"source_column": secondary[name], "driver_column": driver[name]}
                for name in ["STUDYID", "USUBJID"]
            ],
        }
    }


def test_old_native_package_refuses_row_lookups_before_provider(monkeypatch):
    """Legacy multi-source support cannot imply row-scope raw-key matching."""
    case = ROOT / "benchmarks/adam-advs-bmi"
    spec = load_specification(case / "spec.yaml", ROOT / "yaml").specification
    effects = []
    monkeypatch.setitem(
        sys.modules,
        "yamaa_native",
        _native_module(
            execute_dataset=lambda *_: effects.append("execute"),
            dataset_capabilities=lambda: json.dumps(
                {
                    "protocol": "dataset/1",
                    "features": [
                        "row_filter",
                        "predicate_checks",
                        "multi_source",
                        "numeric_compute",
                    ],
                }
            ),
        ),
    )
    actual = execute_with_source_provider(spec, lambda _: effects.append("provider"))
    assert actual.result.status == "unsupported"
    assert {feature.operation for feature in actual.result.features} == {
        "native_row_source_lookup"
    }
    assert effects == []


@pytest.mark.parametrize(
    "predicate", ["ADSL.HEIGHTBL > 0", "VS.VSSTRESN > 0 AND ADSL.HEIGHTBL > 0"]
)
def test_secondary_row_filter_refuses_before_provider(predicate):
    """Row lookups do not extend the driver-only predicate binder to secondary fields."""
    case = ROOT / "benchmarks/adam-advs-bmi"
    spec = load_specification(case / "spec.yaml", ROOT / "yaml").specification
    document = spec.model_dump(exclude_unset=True)
    document["rows"][0]["filter"] = predicate
    spec = type(spec).model_validate(document)
    effects = []

    def provider(_):
        """Supply valid tables if admission incorrectly reaches acquisition."""
        effects.append("provider")
        return load_source_tables(spec.input, ProjectResources(case))

    actual = execute_with_source_provider(spec, provider)
    assert actual.result.status == "unsupported"
    assert {
        (feature.operation, feature.spec_path) for feature in actual.result.features
    } == {("secondary_row_filter", "rows[0].filter")}
    assert effects == []
