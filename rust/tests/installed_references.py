"""Installed shared reference compiler, independent truth and actual planner ownership."""

import csv
import json
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import yamaa_native
from yamaa.adapters import native_datasets
from yamaa.adapters._native_aggregate_syntax import bind_aggregate_analyzer
from yamaa.adapters._native_dependencies import (
    bind_column_dependency_analyzer,
    bind_dependency_analyzer,
)
from yamaa.adapters._native_numeric_syntax import bind_numeric_analyzer
from yamaa.adapters._native_references import (
    NativeReferenceLimitError,
    bind_reference_compiler,
)
from yamaa.expressions import DEFAULT_EXPRESSION_OPERATIONS
from yamaa.io import ProjectResources, load_source_tables, render_artifact
from yamaa.io.polars import frame_from_values
from yamaa.models import TypedColumn
from yamaa.odm import bindings as reference_bindings
from yamaa.planning import ExecutionPlanningError, plan_execution
from yamaa.planning import execution as reference_planning
from yamaa.specification import load_specification
from yamaa.specification.models import (
    Column,
    DatasetSource,
    Expression,
    HandledExpression,
    Intermediate,
    Output,
    Row,
    Specification,
)

ROOT = Path(__file__).parent
SCHEMA = ROOT / "specification-yaml"


def expression(value):
    """Construct a normalized expression without deriving any expected result."""
    return HandledExpression(value=Expression(root=value))


def specification(columns):
    """Minimal input for reference diagnostics with authored declaration paths."""
    return Specification(
        schema_version="1.0",
        domain="OUT",
        input={"SRC": DatasetSource(path="source.csv")},
        base="SRC",
        keys=[columns[0].name],
        output=Output(path="out.csv", columns=[c.name for c in columns]),
        columns=columns,
    )


class InstalledReferences(unittest.TestCase):
    """Both native entrypoints and the actual optional frontend must use the shared core."""

    def test_intermediate_truth_against_unchanged_reference_rules(self):
        """Authored wire findings also match the separate default visibility and donor rules."""
        with (ROOT / "reference_intermediate.tsv").open(encoding="utf-8") as stream:
            cases = list(csv.DictReader(stream, delimiter="\t", quoting=csv.QUOTE_NONE))
        for case in cases:
            with self.subTest(case=case["case"]):
                data = json.loads(case["request"])
                (query,) = data["queries"]
                datasets = {
                    dataset["name"]: {
                        field["name"]: field["type"] for field in dataset["fields"]
                    }
                    for dataset in data["catalog"]["datasets"]
                }
                is_read = query["kind"] == "validate_intermediate_read"
                read = query.get("read", {})
                wire = read.get("target") if is_read else query["target"]
                target = (
                    None
                    if wire is None
                    else SimpleNamespace(
                        dataset="SELF"
                        if wire["source"]["kind"] == "self"
                        else wire["source"]["name"],
                        self_fields=tuple(wire["source"].get("fields", ())),
                        derived=tuple((name, None) for name in wire["derived"]),
                        readable_columns=tuple(wire["readable"]),
                        dependencies=tuple(wire["dependencies"]),
                    )
                )
                findings = []
                if is_read:
                    reference_planning._reference_intermediate_reads(
                        {} if target is None else {read["target_name"]: target},
                        {
                            read["reader"]: [
                                reference_planning._IntermediateRead(
                                    read["reader"],
                                    read["donor_dataset"],
                                    read["target_name"],
                                    read["field"],
                                    "authored.path",
                                    frozenset(read["visible"]),
                                )
                            ]
                        },
                        datasets,
                        findings,
                    )
                else:
                    reference_planning._reference_intermediate_visibility(
                        reference_planning._Reference(
                            "lookup." + query["field"], "authored.path"
                        ),
                        target,
                        SimpleNamespace(
                            datasets={
                                name: SimpleNamespace(field_names=tuple(fields))
                                for name, fields in datasets.items()
                            }
                        ),
                        findings,
                    )
                expected = []
                for finding in json.loads(case["expected"])["outcome"]["results"][0][
                    "diagnostics"
                ]:
                    context = (
                        {"identifier": "lookup." + query["field"]}
                        if not is_read
                        else {
                            "intermediate": read["reader"],
                            "identifier": read["target_name"] + "." + read["field"],
                        }
                    )
                    if finding["kind"] == "unknown_field":
                        condition, requirement = "unknown_field", "REQ-0125"
                    elif finding["kind"] == "self_phase":
                        condition, requirement = "phase_boundary", "REQ-1263"
                    else:
                        self.assertEqual(finding["kind"], "unavailable_dependency")
                        condition, requirement = "unknown_field", "REQ-1263"
                        context["identifier"] = wire["dependencies"][
                            finding["dependency"]
                        ]
                        context["read"] = read["target_name"]
                    expected.append(
                        (condition, requirement, ("authored.path",), context)
                    )
                self.assertEqual(
                    [
                        (d.condition, d.requirement, d.spec_paths, dict(d.context))
                        for d in findings
                    ],
                    expected,
                )

    def test_scope_truth_against_unchanged_reference_rules(self):
        """Replay authored scope outcomes through the separate default Python implementation."""
        with (ROOT / "reference_scope.tsv").open(encoding="utf-8") as stream:
            cases = list(csv.DictReader(stream, delimiter="\t", quoting=csv.QUOTE_NONE))
        for case in cases:
            with self.subTest(case=case["case"]):
                data = json.loads(case["request"])
                (query,) = data["queries"]
                scope, name = query["scope"], query["name"]
                phase, qualifier = scope["phase"], name.split(".", 1)[0]
                bindings = reference_bindings.BindingPlan(
                    domain="OUT",
                    output_columns=(),
                    datasets={
                        dataset["name"]: reference_bindings.DatasetBinding(
                            dataset=dataset["name"],
                            columns=tuple(
                                TypedColumn(name=field["name"], type=field["type"])
                                for field in dataset["fields"]
                            ),
                        )
                        for dataset in data["catalog"]["datasets"]
                    },
                )
                diagnostics = []
                reference_planning._validate_direct_qualified_reference(
                    reference_planning._Reference(
                        name,
                        "authored.path",
                        expected_type=query["expected"],
                        current_driver=scope["current_driver"],
                        reach=scope["reach"],
                        join_relation=qualifier if scope["joined"] else None,
                    ),
                    scope["drivers"],
                    bindings,
                    {},
                    diagnostics,
                    intermediates={},
                    row=Row(id="r", group_by=phase["group_by"], derivations={})
                    if phase["kind"] == "row"
                    else None,
                    grouped_by_driver={qualifier: phase.get("groups", [])},
                )
                actual = []
                for diagnostic in diagnostics:
                    if diagnostic.condition == "unknown_field":
                        actual.append(
                            {
                                "kind": "driver_mismatch"
                                if "drivers" in diagnostic.context
                                else "unknown_field"
                            }
                        )
                    elif diagnostic.condition == "phase_boundary":
                        actual.append({"kind": "row_phase"})
                    elif diagnostic.condition == "ungrouped_driver_field":
                        actual.append(
                            {
                                "kind": "row_group"
                                if diagnostic.requirement == "REQ-0067"
                                else "column_group"
                            }
                        )
                    else:
                        self.assertEqual(
                            diagnostic.condition, "incompatible_input_type"
                        )
                        actual.append(
                            {
                                "kind": diagnostic.condition,
                                "expected": diagnostic.context["expected"],
                                "actual": diagnostic.context["actual"],
                            }
                        )
                self.assertEqual(
                    actual,
                    json.loads(case["expected"])["outcome"]["results"][0][
                        "diagnostics"
                    ],
                )

    def test_aggregate_match_typing_keeps_diagnostic_provenance(self):
        """Aggregate keys use the same core table and retain their own authored diagnostic."""
        for value in (1, True):
            spec = specification(
                [
                    Column(
                        name="K", type="str", derivation=expression({"source": "SRC.X"})
                    ),
                    Column(
                        name="V",
                        type="int",
                        derivation=expression(
                            {
                                "aggregate": {
                                    "dataset": "OTHER",
                                    "key": {"N": {"literal": value}},
                                    "expr": "SUM(OTHER.N)",
                                }
                            }
                        ),
                    ),
                ]
            )
            spec = spec.model_copy(
                update={
                    "input": {**spec.input, "OTHER": DatasetSource(path="other.csv")}
                }
            )
            sources = {
                "OTHER": frame_from_values((TypedColumn(name="N", type="int"),), [[1]])
            }
            for native in (False, True):
                with self.subTest(value=value, native=native):
                    if value is True:
                        with self.assertRaises(ExecutionPlanningError) as caught:
                            self.plan(spec, native, sources)
                        self.assertEqual(
                            [
                                (
                                    d.condition,
                                    d.requirement,
                                    d.spec_paths,
                                    dict(d.context),
                                )
                                for d in caught.exception.diagnostics
                            ],
                            [
                                (
                                    "incompatible_input_type",
                                    "REQ-0004",
                                    ("columns.V.derivation.aggregate.expr",),
                                    {
                                        "source": "key[N]",
                                        "expected": "bool",
                                        "actual": "int",
                                    },
                                )
                            ],
                        )
                    else:
                        self.assertEqual(
                            self.plan(spec, native, sources).columns[-1].column, "V"
                        )

    def test_match_value_truth_and_literal_projection(self):
        """Independent REQ-1259 truth covers the real port and the retained default policy."""
        with (ROOT / "reference_match_values.tsv").open(encoding="utf-8") as stream:
            cases = list(csv.DictReader(stream, delimiter="\t", quoting=csv.QUOTE_NONE))
        for case in cases:
            request = json.loads(case["request"])
            catalog = request["catalog"]
            columns = {field["name"]: field["type"] for field in catalog["outputs"]}
            bindings = reference_bindings.BindingPlan(
                domain="OUT",
                output_columns=tuple(columns),
                datasets={
                    dataset["name"]: reference_bindings.DatasetBinding(
                        dataset=dataset["name"],
                        columns=tuple(
                            TypedColumn(name=field["name"], type=field["type"])
                            for field in dataset["fields"]
                        ),
                    )
                    for dataset in catalog["datasets"]
                },
            )
            compiler = bind_reference_compiler(yamaa_native)(bindings, columns)
            metadata = request["queries"][0]["expression"]
            kind = metadata["kind"]
            if kind == "source":
                roots = [
                    {"source": metadata["name"]},
                    {"source": {"variable": metadata["name"], "missing": "fallback"}},
                ]
            elif kind == "literal":
                values = {
                    "str": ["", "\u00e9", "2026-10-05"],
                    "int": [0, -1, 10**100],
                    "float": [0.0, -0.0, float("nan"), float("inf"), -float("inf")],
                    "bool": [True, False],
                    "missing": [None],
                    "other": [[], [1]],
                }[metadata["scalar"]]
                roots = [{"literal": value} for value in values] + [
                    {
                        "literal": {
                            "value": value,
                            "missing": "replacement",
                            "invalid": 1,
                        }
                    }
                    for value in values
                ]
            elif kind == "operation":
                roots = [
                    {metadata["name"]: None},
                    {
                        metadata["name"]: {
                            "missing": True,
                            "invalid": 1,
                            "no_match": "replacement",
                        }
                    },
                ]
            else:
                roots = [{"source": None}, {"source": {}}, {"source": 12}]
            expected = json.loads(case["expected"])["outcome"]["results"][0][
                "result_type"
            ]
            for root in roots:
                with self.subTest(case=case["case"], root=root):
                    value = Expression(root=root)
                    self.assertEqual(compiler.match_value_type(value), expected)
                    self.assertEqual(
                        reference_planning._reference_match_value_result_type(
                            value, bindings, columns
                        ),
                        expected,
                    )

    def test_match_value_query_errors_and_limits_do_not_fall_back(self):
        """A failed query publishes no result, and an explicit later attempt can retry."""
        bindings = reference_bindings.BindingPlan(
            domain="OUT", output_columns=(), datasets={}
        )
        compiler = bind_reference_compiler(yamaa_native)(bindings, {})
        for operation, resource in (
            ("source", "reference_bytes"),
            ("\u00e9" * 32769, "operation_bytes"),
        ):
            value = Expression(root={operation: "\u00e9" * 32769})
            with (
                self.subTest(resource=resource),
                self.assertRaises(NativeReferenceLimitError) as caught,
            ):
                compiler.match_value_type(value)
            self.assertEqual(
                (
                    caught.exception.resource,
                    caught.exception.limit,
                    caught.exception.required,
                ),
                (resource, 65536, 65538),
            )
        self.assertEqual(
            compiler.match_value_type(Expression(root={"literal": True})), "bool"
        )
        original = yamaa_native._compile_reference_catalog
        failure = ValueError("match type query failed")

        def fail_typing(request):
            """Inject only at the new query, allowing real catalog and preceding queries."""
            catalog, status = original(request)

            def analyze(request):
                """Fail only typing while all other metadata uses the actual native catalog."""
                if any(
                    query["kind"] == "match_value_type"
                    for query in json.loads(request)["queries"]
                ):
                    raise failure
                return catalog.analyze(request)

            return SimpleNamespace(analyze=analyze), status

        spec = specification(
            [
                Column(
                    name="K", type="str", derivation=expression({"source": "SRC.X"})
                ),
                Column(
                    name="V", type="int", derivation=expression({"source": "LOOK.N"})
                ),
            ]
        ).model_copy(
            update={
                "intermediates": [
                    Intermediate(id="LOOK", dataset="SRC", key={"N": {"literal": 1}})
                ]
            }
        )
        with (
            patch.object(yamaa_native, "_compile_reference_catalog", fail_typing),
            self.assertRaises(ValueError) as caught,
        ):
            self.plan(spec)
        self.assertIs(caught.exception, failure)
        self.assertEqual(self.plan(spec).intermediates[0].match_fields, ("N",))
        # Shared planning metadata does not enable expression keys in the bounded executor.
        result = native_datasets.execute_with_source_provider(
            spec, lambda _: self.fail("source")
        )
        self.assertEqual(result.result.status, "unsupported")
        self.assertIn(
            ("intermediate_key", "intermediates[0].key"),
            [(f.operation, f.spec_path) for f in result.result.features],
        )

    def test_match_value_capability_precedes_activation_and_data(self):
        """The previous installed query set refuses both frontends before side effects."""
        case = ROOT / "specification-functions"
        project_spec = load_specification(case / "spec.yaml", SCHEMA).specification
        ordinary_spec = load_specification(
            ROOT / "specification-adlb" / "spec.yaml", SCHEMA
        ).specification
        with (
            patch.object(
                yamaa_native,
                "reference_capabilities",
                lambda: (
                    '{"protocol":"reference-analysis/1","features":["binding","output_validation","qualified_validation","intermediate_validation","key_relations"]}'
                ),
            ),
            patch.object(
                native_datasets,
                "activate_project",
                side_effect=AssertionError("activation"),
            ),
        ):
            project = native_datasets.execute_with_project_functions(
                project_spec, lambda _: self.fail("source"), case / "python", SCHEMA
            )
            ordinary = native_datasets.execute_with_source_provider(
                ordinary_spec, lambda _: self.fail("source")
            )
        for actual in (ordinary, project):
            self.assertEqual(actual.result.status, "unsupported")
            self.assertEqual(
                [(f.operation, f.spec_path) for f in actual.result.features],
                [("native_match_value_typing", "$")],
            )
            self.assertEqual(actual.result.handler_counts, ())
            self.assertEqual(actual.verifications, ())

    def test_key_truth_against_default_rules(self):
        """Replay independent comparison/inference truth through the retained default rules."""
        with (ROOT / "reference_keys.tsv").open(encoding="utf-8") as stream:
            for case in csv.DictReader(stream, delimiter="\t", quoting=csv.QUOTE_NONE):
                data = json.loads(case["request"])
                query = data["queries"][0]
                expected = json.loads(case["expected"])["outcome"]["results"][0]
                with self.subTest(case=case["case"]):
                    if query["kind"] == "comparable_types":
                        self.assertEqual(
                            reference_planning._reference_comparable_types(
                                query["left"], query["right"]
                            ),
                            expected["comparable"],
                        )
                        continue
                    keys = tuple(query["keys"])
                    finding = reference_planning._reference_applicable_keys(
                        keys,
                        {
                            field["name"]: field["type"]
                            for field in data["catalog"]["outputs"]
                        },
                        {field["name"]: field["type"] for field in query["fields"]},
                    )
                    expected = expected["inference"]
                    self.assertEqual(finding.kind, expected["kind"])
                    self.assertEqual(
                        finding.keys, tuple(keys[i] for i in expected.get("keys", []))
                    )
                    self.assertEqual(
                        finding.key,
                        keys[expected["key"]] if "key" in expected else None,
                    )
                    self.assertEqual(finding.expected, expected.get("expected"))
                    self.assertEqual(finding.actual, expected.get("actual"))

    def test_range_bound_compatibility_keeps_diagnostic_provenance(self):
        """Range type comparison uses the same Rust relation rule with REQ-0121 paths."""
        for value in ("K", "SRC.X"):
            spec = specification(
                [
                    Column(
                        name="K", type="int", derivation=expression({"source": "SRC.N"})
                    ),
                    Column(
                        name="V",
                        type="int",
                        derivation=expression({"source": "SRC.N"}),
                    ),
                ]
            ).model_copy(
                update={
                    "intermediates": [
                        Intermediate(
                            id="LOOK",
                            dataset="SRC",
                            key={"N": "K"},
                            between={"value": value, "lower": "N", "upper": "N"},
                        )
                    ]
                }
            )
            for native in (False, True):
                with self.subTest(value=value, native=native):
                    if value == "K":
                        self.assertEqual(
                            self.plan(spec, native).intermediates[0].identifier, "LOOK"
                        )
                        continue
                    with self.assertRaises(ExecutionPlanningError) as caught:
                        self.plan(spec, native)
                    (diagnostic,) = caught.exception.diagnostics
                    self.assertEqual(
                        (
                            diagnostic.condition,
                            diagnostic.requirement,
                            diagnostic.spec_paths,
                            dict(diagnostic.context),
                        ),
                        (
                            "incomparable_range_types",
                            "REQ-0121",
                            ("intermediates[0].between",),
                            {
                                "intermediate": "LOOK",
                                "value_type": "str",
                                "lower_type": "int",
                                "upper_type": "int",
                            },
                        ),
                    )

    def test_boolean_expression_keys_report_language_type_mismatch(self):
        """Known boolean expressions remain distinct from int keys in both compiler paths."""
        for value in (
            {"literal": True},
            {"literal": False},
            {"str_contains": {"source": "K", "pattern": "x"}},
        ):
            spec = specification(
                [
                    Column(
                        name="K", type="str", derivation=expression({"source": "SRC.X"})
                    ),
                ]
            ).model_copy(
                update={
                    "intermediates": [
                        Intermediate(
                            id="LOOK",
                            dataset="SRC",
                            key={"N": value},
                        )
                    ]
                }
            )
            for native in (False, True):
                with self.subTest(value=value, native=native):
                    with self.assertRaises(ExecutionPlanningError) as caught:
                        self.plan(spec, native)
                    (diagnostic,) = caught.exception.diagnostics
                    self.assertEqual(
                        (
                            diagnostic.condition,
                            diagnostic.requirement,
                            diagnostic.spec_paths,
                            dict(diagnostic.context),
                        ),
                        (
                            "incompatible_input_type",
                            "REQ-0323",
                            ("intermediates[0].key",),
                            {
                                "intermediate": "LOOK",
                                "source": "key[N]",
                                "expected": "bool",
                                "actual": "int",
                            },
                        ),
                    )

    def test_unknown_static_key_type_defers_without_comparison(self):
        """Mapping keys retain REQ-1259 runtime typing in both compiler paths."""
        spec = specification(
            [
                Column(
                    name="K", type="str", derivation=expression({"source": "SRC.X"})
                ),
                Column(
                    name="V", type="int", derivation=expression({"source": "LOOK.N"})
                ),
            ]
        ).model_copy(
            update={
                "intermediates": [
                    Intermediate(
                        id="LOOK",
                        dataset="SRC",
                        key={"N": {"mapping": {"source": "K", "dict": {"x": 1}}}},
                    )
                ]
            }
        )
        for native in (False, True):
            with self.subTest(native=native):
                plan = self.plan(spec, native)
                self.assertEqual(plan.intermediates[0].match_fields, ("N",))
                self.assertEqual(plan.intermediates[0].match_variables, ("key[N]",))

    def test_inferred_key_paths_types_and_order(self):
        """Actual optional planning owns inference; failures retain authored join provenance."""
        for kind in ("float", "str", None):
            spec = specification(
                [
                    Column(
                        name="K", type="int", derivation=expression({"source": "SRC.N"})
                    ),
                    Column(
                        name="V",
                        type="int",
                        derivation=expression({"source": "OTHER.N"}),
                    ),
                ]
            ).model_copy(
                update={
                    "input": {
                        "SRC": DatasetSource(path="src.csv"),
                        "OTHER": DatasetSource(path="other.csv"),
                    }
                }
            )
            columns = [TypedColumn(name="N", type="int")]
            values = [1]
            if kind is not None:
                columns.insert(0, TypedColumn(name="K", type=kind))
                values.insert(0, 1.0 if kind == "float" else "x")
            sources = {"OTHER": frame_from_values(tuple(columns), [values])}
            for native in (False, True):
                with self.subTest(kind=kind, native=native):
                    if kind == "float":
                        plan = self.plan(spec, native, sources)
                        self.assertEqual(plan.columns[1].implicit_joins[0].keys, ("K",))
                        continue
                    with self.assertRaises(ExecutionPlanningError) as caught:
                        self.plan(spec, native, sources)
                    (diagnostic,) = caught.exception.diagnostics
                    self.assertEqual(
                        diagnostic.spec_paths, ("columns.V.derivation.source",)
                    )
                    if kind == "str":
                        self.assertEqual(
                            diagnostic.condition, "incompatible_input_type"
                        )
                        self.assertEqual(diagnostic.requirement, "REQ-0151")
                        self.assertEqual(
                            dict(diagnostic.context),
                            {"source": "K", "expected": "int", "actual": "str"},
                        )
                    else:
                        self.assertEqual(diagnostic.condition, "no_applicable_keys")
                        self.assertEqual(diagnostic.requirement, "REQ-0152")
                        self.assertEqual(
                            dict(diagnostic.context),
                            {
                                "dataset": "OTHER",
                                "keys": ["K"],
                                "hint": "declare an explicit `intermediate:` with `source`/`key` pairs",
                            },
                        )

    def test_shared_truth_batch_prepared_and_owned(self):
        """Replay authored bindings/conditions through both interfaces after request release."""
        cases = []
        for fixture in (
            "reference_binding.tsv",
            "reference_scope.tsv",
            "reference_intermediate.tsv",
            "reference_keys.tsv",
            "reference_match_values.tsv",
            "reference_relations.tsv",
        ):
            with (ROOT / fixture).open(encoding="utf-8") as stream:
                cases.extend(
                    csv.DictReader(stream, delimiter="\t", quoting=csv.QUOTE_NONE)
                )
        for case in cases:
            with self.subTest(case=case["case"]):
                expected = json.loads(case["expected"])
                self.assertEqual(
                    json.loads(yamaa_native.analyze_references(case["request"])),
                    expected,
                )
                request = json.loads(case["request"])
                catalog, status = yamaa_native._compile_reference_catalog(
                    json.dumps(
                        {
                            "protocol": "reference-catalog/1",
                            "catalog": request["catalog"],
                        }
                    )
                )
                self.assertEqual(json.loads(status)["outcome"]["status"], "complete")
                query = json.dumps(
                    {"protocol": "reference-queries/1", "queries": request["queries"]}
                )
                request["catalog"] = None
                del request
                for _ in range(2):
                    self.assertEqual(json.loads(catalog.analyze(query)), expected)
                with self.assertRaises(AttributeError):
                    catalog.changed = True
        with self.assertRaises(TypeError):
            yamaa_native._ReferenceCatalog()

    def plan(self, spec, native=True, extra_sources=None):
        """Select the actual compiled catalog and forbid reference binding fallback."""
        sources = {
            "SRC": frame_from_values(
                (TypedColumn(name="X", type="str"), TypedColumn(name="N", type="int")),
                [["x", 1]],
            )
        }
        sources.update(extra_sources or {})
        if not native:
            return plan_execution(
                spec, sources, supported_operations=DEFAULT_EXPRESSION_OPERATIONS
            )
        with (
            patch.object(
                reference_planning,
                "analyze_numeric",
                side_effect=AssertionError("reference numeric syntax"),
            ),
            patch.object(
                reference_planning,
                "analyze_aggregate",
                side_effect=AssertionError("reference aggregate syntax"),
            ),
            patch.object(
                reference_bindings,
                "_bind_reference",
                side_effect=AssertionError("reference binder"),
            ),
            patch.object(
                reference_planning,
                "_unresolvable_reference_diagnostic",
                side_effect=AssertionError("reference bare-name rules"),
            ),
            patch.object(
                reference_planning,
                "_validate_direct_qualified_reference",
                side_effect=AssertionError("reference qualified rules"),
            ),
            patch.object(
                reference_planning,
                "_reference_intermediate_visibility",
                side_effect=AssertionError("reference intermediate visibility"),
            ),
            patch.object(
                reference_planning,
                "_reference_comparable_types",
                side_effect=AssertionError("reference key rules"),
            ),
            patch.object(
                reference_planning,
                "_reference_match_value_result_type",
                side_effect=AssertionError("reference match value typing"),
            ),
            patch.object(
                reference_planning,
                "_reference_applicable_keys",
                side_effect=AssertionError("reference key rules"),
            ),
            patch.object(
                reference_planning,
                "_reference_intermediate_reads",
                side_effect=AssertionError("reference donor scope"),
            ),
            patch.object(
                reference_planning,
                "_find_cycle",
                side_effect=AssertionError("reference cycle analysis"),
            ),
        ):
            return plan_execution(
                spec,
                sources,
                supported_operations=DEFAULT_EXPRESSION_OPERATIONS,
                dependency_analyzer=bind_dependency_analyzer(yamaa_native),
                column_dependency_analyzer=bind_column_dependency_analyzer(
                    yamaa_native
                ),
                reference_compiler_factory=bind_reference_compiler(yamaa_native),
                aggregate_analyzer=bind_aggregate_analyzer(yamaa_native),
                numeric_analyzer=bind_numeric_analyzer(yamaa_native),
            )

    def test_suggestion_and_unknown_keep_exact_paths(self):
        """Native and unchanged reference planning both match explicit diagnostic truth."""
        for name, condition, context in (
            ("X", "unresolvable_name", {"identifier": "X", "suggestion": "SRC.X"}),
            ("ABSENT", "unknown_field", {"identifier": "ABSENT"}),
        ):
            spec = specification(
                [
                    Column(
                        name="K", type="str", derivation=expression({"source": "SRC.X"})
                    ),
                    Column(
                        name="V", type="str", derivation=expression({"source": name})
                    ),
                ]
            )
            for native in (False, True):
                with (
                    self.subTest(name=name, native=native),
                    self.assertRaises(ExecutionPlanningError) as caught,
                ):
                    self.plan(spec, native)
                (diagnostic,) = caught.exception.diagnostics
                self.assertEqual(
                    (
                        diagnostic.condition,
                        diagnostic.spec_paths,
                        dict(diagnostic.context),
                    ),
                    (condition, ("columns.V.derivation.source",), context),
                )

    def test_intermediate_visibility_and_donor_diagnostic_provenance(self):
        """Actual planning retains hidden-field-before-donor-dependency findings and paths."""
        for hidden, unavailable in (
            (False, False),
            (True, False),
            (False, True),
            (True, True),
        ):
            target = Intermediate(
                id="LOOK",
                dataset="SRC",
                key={"X": "K" if unavailable else "SRC.X"},
                columns=["X"] if hidden else None,
            )
            reader = Intermediate(
                id="READER",
                dataset="SRC",
                key={"X": "K"},
                derivations={"D": expression({"source": "LOOK.N"})},
            )
            spec = specification(
                [
                    Column(
                        name="K", type="str", derivation=expression({"source": "SRC.X"})
                    ),
                    Column(
                        name="V",
                        type="int",
                        derivation=expression({"source": "READER.D"}),
                    ),
                ]
            ).model_copy(update={"intermediates": [target, reader]})
            expected = []
            if hidden:
                expected.append(
                    (
                        "unknown_field",
                        "REQ-0125",
                        ("intermediates[1].derivations.D.source",),
                        {"intermediate": "READER", "identifier": "LOOK.N"},
                    )
                )
            if unavailable:
                expected.append(
                    (
                        "unknown_field",
                        "REQ-1263",
                        ("intermediates[1].derivations.D.source",),
                        {"intermediate": "READER", "identifier": "K", "read": "LOOK"},
                    )
                )
            for native in (False, True):
                with self.subTest(
                    hidden=hidden, unavailable=unavailable, native=native
                ):
                    if expected:
                        with self.assertRaises(ExecutionPlanningError) as caught:
                            self.plan(spec, native)
                        self.assertEqual(
                            [
                                (
                                    d.condition,
                                    d.requirement,
                                    d.spec_paths,
                                    dict(d.context),
                                )
                                for d in caught.exception.diagnostics
                            ],
                            expected,
                        )
                    else:
                        plan = self.plan(spec, native)
                        self.assertEqual(
                            tuple(item.identifier for item in plan.intermediates),
                            ("LOOK", "READER"),
                        )

    def test_direct_intermediate_visibility_and_self_donor_phase(self):
        """REQ-0125 direct reads and REQ-1263 SELF donor reads keep distinct provenance."""
        direct = specification(
            [
                Column(
                    name="K", type="str", derivation=expression({"source": "SRC.X"})
                ),
                Column(
                    name="V", type="int", derivation=expression({"source": "LOOK.N"})
                ),
            ]
        ).model_copy(
            update={
                "intermediates": [
                    Intermediate(
                        id="LOOK", dataset="SRC", key={"X": "K"}, columns=["X"]
                    )
                ]
            }
        )
        own = specification(
            [
                Column(name="K", type="str"),
                Column(
                    name="V", type="str", derivation=expression({"source": "READER.D"})
                ),
            ]
        ).model_copy(
            update={
                "rows": [
                    Row(id="r", derivations={"K": expression({"source": "SRC.X"})})
                ],
                "intermediates": [
                    Intermediate(id="DONOR", dataset="SELF", key=["K"]),
                    Intermediate(
                        id="READER",
                        dataset="SRC",
                        key={"X": "K"},
                        derivations={"D": expression({"source": "DONOR.K"})},
                    ),
                ],
            }
        )
        for spec, expected in [
            (
                direct,
                (
                    "unknown_field",
                    "REQ-0125",
                    ("columns.V.derivation.source",),
                    {"identifier": "LOOK.N"},
                ),
            ),
            (
                own,
                (
                    "phase_boundary",
                    "REQ-1263",
                    ("intermediates[1].derivations.D.source",),
                    {"intermediate": "READER", "identifier": "DONOR.K"},
                ),
            ),
        ]:
            for native in (False, True):
                with (
                    self.subTest(native=native, expected=expected),
                    self.assertRaises(ExecutionPlanningError) as caught,
                ):
                    self.plan(spec, native)
                self.assertEqual(
                    [
                        (d.condition, d.requirement, d.spec_paths, dict(d.context))
                        for d in caught.exception.diagnostics
                    ],
                    [expected],
                )

    def test_intermediate_cycle_deferral_retains_authored_paths(self):
        """Shared visibility defers self-reads and preserves mutual-cycle diagnosis afterward."""
        for mutual in (False, True):
            reader = Intermediate(
                id="READER",
                dataset="SRC",
                key={"X": "SRC.X"},
                derivations={
                    "D": expression({"source": "LOOK.N" if mutual else "READER.N"})
                },
            )
            intermediates = (
                [
                    Intermediate(
                        id="LOOK",
                        dataset="SRC",
                        key={"X": "SRC.X"},
                        derivations={"BACK": expression({"source": "READER.N"})},
                    )
                ]
                if mutual
                else []
            ) + [reader]
            spec = specification(
                [
                    Column(
                        name="K", type="str", derivation=expression({"source": "SRC.X"})
                    ),
                    Column(
                        name="V",
                        type="int",
                        derivation=expression({"source": "READER.D"}),
                    ),
                ]
            ).model_copy(update={"intermediates": intermediates})
            expected_cycle = (
                ["LOOK", "READER", "LOOK"] if mutual else ["READER", "READER"]
            )
            expected_paths = (
                (
                    "intermediates[0].derivations.BACK.source",
                    "intermediates[1].derivations.D.source",
                )
                if mutual
                else ("intermediates[0].derivations.D.source",)
            )
            for native in (False, True):
                with (
                    self.subTest(mutual=mutual, native=native),
                    self.assertRaises(ExecutionPlanningError) as caught,
                ):
                    self.plan(spec, native)
                self.assertEqual(
                    [
                        (d.condition, d.requirement, d.spec_paths, dict(d.context))
                        for d in caught.exception.diagnostics
                    ],
                    [
                        (
                            "dependency_cycle",
                            "REQ-1263",
                            expected_paths,
                            {"cycle": expected_cycle},
                        )
                    ],
                )

    def test_qualified_scope_priority_and_provenance(self):
        """Real planning preserves unknown/group/type ordering and authored context."""
        plain = specification(
            [
                Column(
                    name="K", type="str", derivation=expression({"source": "SRC.X"})
                ),
                Column(
                    name="V",
                    type="str",
                    derivation=expression({"source": "SRC.ABSENT"}),
                ),
            ]
        )
        grouped = specification(
            [Column(name="K", type="str"), Column(name="V", type="str")]
        ).model_copy(
            update={
                "rows": [
                    Row(
                        id="g",
                        group_by=["SRC.X"],
                        derivations={
                            "K": expression({"source": "SRC.X"}),
                            "V": expression(
                                {"str_case": {"source": "SRC.N", "to": "upper"}}
                            ),
                        },
                    )
                ]
            }
        )
        column = specification(
            [
                Column(
                    name="K", type="str", derivation=expression({"source": "SRC.X"})
                ),
                Column(
                    name="V", type="int", derivation=expression({"source": "SRC.N"})
                ),
            ]
        ).model_copy(update={"rows": [Row(id="g", group_by=["SRC.X"], derivations={})]})
        for spec, expected in [
            (
                plain,
                [
                    (
                        "unknown_field",
                        "REQ-0103",
                        ("columns.V.derivation.source",),
                        {"identifier": "SRC.ABSENT"},
                    )
                ],
            ),
            (
                grouped,
                [
                    (
                        "ungrouped_driver_field",
                        "REQ-0067",
                        ("rows[0].derivations.V.str_case.source",),
                        {"identifier": "SRC.N", "row": "g", "dataset": "SRC"},
                    ),
                    (
                        "incompatible_input_type",
                        "REQ-0308",
                        ("rows[0].derivations.V.str_case.source",),
                        {"source": "SRC.N", "expected": "str", "actual": "int"},
                    ),
                ],
            ),
            (
                column,
                [
                    (
                        "ungrouped_driver_field",
                        "REQ-0107",
                        ("columns.V.derivation.source",),
                        {"identifier": "SRC.N", "dataset": "SRC"},
                    )
                ],
            ),
        ]:
            for native in (False, True):
                with (
                    self.subTest(native=native, expected=expected),
                    self.assertRaises(ExecutionPlanningError) as caught,
                ):
                    self.plan(spec, native)
                self.assertEqual(
                    [
                        (d.condition, d.requirement, d.spec_paths, dict(d.context))
                        for d in caught.exception.diagnostics
                    ],
                    expected,
                )

    def test_qualified_capability_precedes_activation_and_data(self):
        """Missing or older query advertisements refuse before host effects."""
        self.assertEqual(
            json.loads(yamaa_native.reference_capabilities()),
            {
                "protocol": "reference-analysis/1",
                "features": [
                    "binding",
                    "output_validation",
                    "qualified_validation",
                    "intermediate_validation",
                    "key_relations",
                    "match_value_typing",
                    "relation_binding",
                ],
            },
        )
        case = ROOT / "specification-functions"
        spec = load_specification(case / "spec.yaml", SCHEMA).specification
        for capability in (
            None,
            lambda: (
                '{"protocol":"reference-analysis/1","features":["binding","output_validation"]}'
            ),
        ):
            with (
                patch.object(yamaa_native, "reference_capabilities", capability),
                patch.object(
                    native_datasets,
                    "activate_project",
                    side_effect=AssertionError("activation"),
                ),
            ):
                actual = native_datasets.execute_with_project_functions(
                    spec, lambda _: self.fail("source"), case / "python", SCHEMA
                )
            self.assertEqual(actual.result.status, "unsupported")
            self.assertEqual(
                [(f.operation, f.spec_path) for f in actual.result.features],
                [("native_qualified_reference_validation", "$")],
            )
            self.assertEqual(actual.result.handler_counts, ())
            self.assertEqual(actual.verifications, ())

    def test_expected_type_diagnostic_keeps_requirement(self):
        """A bound bare numeric output does not satisfy a string operation's expected type."""
        spec = specification(
            [
                Column(
                    name="K", type="str", derivation=expression({"source": "SRC.X"})
                ),
                Column(
                    name="N", type="int", derivation=expression({"source": "SRC.N"})
                ),
                Column(
                    name="V",
                    type="str",
                    derivation=expression({"str_case": {"source": "N", "to": "upper"}}),
                ),
            ]
        )
        for native in (False, True):
            with (
                self.subTest(native=native),
                self.assertRaises(ExecutionPlanningError) as caught,
            ):
                self.plan(spec, native)
            (diagnostic,) = caught.exception.diagnostics
            self.assertEqual(
                (
                    diagnostic.condition,
                    diagnostic.requirement,
                    diagnostic.spec_paths,
                    dict(diagnostic.context),
                ),
                (
                    "incompatible_input_type",
                    "REQ-0308",
                    ("columns.V.derivation.str_case.source",),
                    {"source": "N", "expected": "str", "actual": "int"},
                ),
            )

    def test_intermediate_shadow_keeps_language_diagnostic(self):
        """Invalid repeated visible fields cannot become a native catalog metadata error."""
        spec = specification(
            [
                Column(
                    name="K", type="str", derivation=expression({"source": "SRC.X"})
                ),
            ]
        ).model_copy(
            update={
                "intermediates": [
                    Intermediate(
                        id="I",
                        dataset="SRC",
                        key={"X": "SRC.X"},
                        derivations={
                            "X": expression(
                                {"str_case": {"source": "X", "to": "upper"}}
                            )
                        },
                    )
                ]
            }
        )
        for native in (False, True):
            with (
                self.subTest(native=native),
                self.assertRaises(ExecutionPlanningError) as caught,
            ):
                self.plan(spec, native)
            (diagnostic,) = caught.exception.diagnostics
            self.assertEqual(
                (diagnostic.condition, diagnostic.requirement, diagnostic.spec_paths),
                (
                    "duplicate_derivation",
                    "REQ-1185",
                    ("intermediates[0].derivations.X",),
                ),
            )

    def test_row_phase_precedes_expected_type(self):
        """A column-phase integer is unavailable before a row string type check."""
        spec = specification(
            [
                Column(name="K", type="str"),
                Column(
                    name="D",
                    type="int",
                    derivation=expression(
                        {"row_number": {"window": {"order_by": ["K"]}}}
                    ),
                ),
                Column(name="V", type="str"),
            ]
        ).model_copy(
            update={
                "rows": [
                    Row(
                        id="r",
                        derivations={
                            "K": expression({"source": "SRC.X"}),
                            "V": expression(
                                {"str_case": {"source": "D", "to": "upper"}}
                            ),
                        },
                    )
                ]
            }
        )
        for native in (False, True):
            with (
                self.subTest(native=native),
                self.assertRaises(ExecutionPlanningError) as caught,
            ):
                self.plan(spec, native)
            (diagnostic,) = caught.exception.diagnostics
            self.assertEqual(
                (
                    diagnostic.condition,
                    diagnostic.requirement,
                    diagnostic.spec_paths,
                    dict(diagnostic.context),
                ),
                (
                    "phase_boundary",
                    None,
                    ("rows[0].derivations.V.str_case.source",),
                    {
                        "identifier": "D",
                        "row": "r",
                        "available_phase": "column_derivation",
                        "required_phase": "row_construction",
                    },
                ),
            )

    def test_actual_frontend_captures_catalog_compiler_before_source(self):
        """Original CSVs pass with reference binding disabled and module mutation during IO."""
        for name in (
            "specification-adlb",
            "specification-windows",
            "specification-lookup",
            "specification-functions",
        ):
            with self.subTest(case=name):
                case = ROOT / name
                spec = load_specification(case / "spec.yaml", SCHEMA).specification
                compile_catalog = yamaa_native._compile_reference_catalog
                events = []

                def compile_once(request, original=compile_catalog, events=events):
                    """Observe actual compilation without substituting results."""
                    events.append("compile")
                    return original(request)

                def provider(declarations, case=case, events=events):
                    """Source effects cannot replace the already selected compiler service."""
                    events.append("source")
                    yamaa_native._compile_reference_catalog = lambda _: self.fail(
                        "compiler replaced during source IO"
                    )
                    return load_source_tables(declarations, ProjectResources(case))

                with (
                    patch.object(
                        yamaa_native, "_compile_reference_catalog", compile_once
                    ),
                    patch.object(
                        reference_bindings,
                        "_bind_reference",
                        side_effect=AssertionError("reference binder"),
                    ),
                    patch.object(
                        reference_planning,
                        "_unresolvable_reference_diagnostic",
                        side_effect=AssertionError("reference output rules"),
                    ),
                    patch.object(
                        reference_planning,
                        "_validate_direct_qualified_reference",
                        side_effect=AssertionError("reference qualified rules"),
                    ),
                    patch.object(
                        reference_planning,
                        "_reference_intermediate_visibility",
                        side_effect=AssertionError("reference intermediate visibility"),
                    ),
                    patch.object(
                        reference_planning,
                        "_reference_comparable_types",
                        side_effect=AssertionError("reference key rules"),
                    ),
                    patch.object(
                        reference_planning,
                        "_reference_match_value_result_type",
                        side_effect=AssertionError("reference match value typing"),
                    ),
                    patch.object(
                        reference_planning,
                        "_reference_applicable_keys",
                        side_effect=AssertionError("reference key rules"),
                    ),
                    patch.object(
                        reference_planning,
                        "_reference_intermediate_reads",
                        side_effect=AssertionError("reference donor scope"),
                    ),
                ):
                    if name == "specification-functions":
                        actual = native_datasets.execute_with_project_functions(
                            spec, provider, case / "python", SCHEMA, cache=None
                        )
                    else:
                        actual = native_datasets.execute_with_source_provider(
                            spec, provider
                        )
                self.assertEqual(actual.result.status, "success")
                self.assertEqual(events, ["source", "compile"])
                self.assertEqual(
                    render_artifact(actual.result.artifact),
                    (case / "expected" / Path(spec.output.path).name).read_bytes(),
                )

    def test_missing_service_precedes_activation_and_data(self):
        """Older installations refuse explicitly before any host execution effects."""
        case = ROOT / "specification-functions"
        spec = load_specification(case / "spec.yaml", SCHEMA).specification
        with (
            patch.object(yamaa_native, "_compile_reference_catalog", None),
            patch.object(
                native_datasets,
                "activate_project",
                side_effect=AssertionError("activation"),
            ),
        ):
            actual = native_datasets.execute_with_project_functions(
                spec, lambda _: self.fail("source"), case / "python", SCHEMA
            )
        self.assertEqual(actual.result.status, "unsupported")
        self.assertEqual(
            [(f.operation, f.spec_path) for f in actual.result.features],
            [("native_reference_binding", "$")],
        )
        self.assertEqual(actual.result.handler_counts, ())
        self.assertEqual(actual.verifications, ())

    def test_intermediate_query_limit_prevents_execution_and_allows_explicit_retry(
        self,
    ):
        """A failed metadata query has no fallback; an explicit retry rereads source data."""
        case = ROOT / "specification-lookup"
        spec = load_specification(case / "spec.yaml", SCHEMA).specification
        compile_catalog = yamaa_native._compile_reference_catalog
        inject_limit = True
        sources = []

        def compile_with_limit(request):
            """Keep the real catalog and inject one admitted-query policy outcome."""
            catalog, status = compile_catalog(request)

            def analyze(query):
                """Only the new intermediate query fails; all other calls use native Rust."""
                nonlocal inject_limit
                if inject_limit and any(
                    item["kind"] == "validate_intermediate"
                    for item in json.loads(query)["queries"]
                ):
                    inject_limit = False
                    return json.dumps(
                        {
                            "protocol": "reference-analysis/1",
                            "outcome": {
                                "status": "limit",
                                "resource": "intermediate_entries",
                                "limit": "65536",
                                "required": "65537",
                            },
                        }
                    )
                return catalog.analyze(query)

            return SimpleNamespace(analyze=analyze), status

        def provider(declarations):
            """Source ownership remains with the caller on each explicitly requested attempt."""
            sources.append("source")
            return load_source_tables(declarations, ProjectResources(case))

        with (
            patch.object(
                yamaa_native, "_compile_reference_catalog", compile_with_limit
            ),
            patch.object(
                yamaa_native, "execute_dataset", wraps=yamaa_native.execute_dataset
            ) as execute,
            patch.object(
                yamaa_native,
                "execute_dataset_sources",
                wraps=yamaa_native.execute_dataset_sources,
            ) as execute_sources,
            patch.object(
                reference_planning,
                "_reference_intermediate_visibility",
                side_effect=AssertionError("reference fallback"),
            ),
        ):
            with self.assertRaises(NativeReferenceLimitError) as caught:
                native_datasets.execute_with_source_provider(spec, provider)
            self.assertEqual(
                (
                    caught.exception.resource,
                    caught.exception.limit,
                    caught.exception.required,
                ),
                ("intermediate_entries", 65536, 65537),
            )
            self.assertFalse(inject_limit)
            self.assertFalse(execute.called)
            self.assertFalse(execute_sources.called)
            self.assertEqual(sources, ["source"])
            actual = native_datasets.execute_with_source_provider(spec, provider)
            self.assertTrue(execute.called or execute_sources.called)
        self.assertEqual(sources, ["source", "source"])
        self.assertEqual(actual.result.status, "success")
        self.assertEqual(
            render_artifact(actual.result.artifact),
            (case / "expected" / Path(spec.output.path).name).read_bytes(),
        )

    def test_key_query_limit_prevents_execution_and_allows_explicit_retry(
        self,
    ):
        """A failed metadata query has no fallback; an explicit retry rereads source data."""
        case = ROOT / "specification-lookup"
        spec = load_specification(case / "spec.yaml", SCHEMA).specification
        compile_catalog = yamaa_native._compile_reference_catalog
        inject_limit = True
        sources = []

        def compile_with_limit(request):
            """Keep the real catalog and inject one admitted-query policy outcome."""
            catalog, status = compile_catalog(request)

            def analyze(query):
                """Only the key comparison query fails; all other calls use native Rust."""
                nonlocal inject_limit
                if inject_limit and any(
                    item["kind"] == "comparable_types"
                    for item in json.loads(query)["queries"]
                ):
                    inject_limit = False
                    return json.dumps(
                        {
                            "protocol": "reference-analysis/1",
                            "outcome": {
                                "status": "limit",
                                "resource": "key_entries",
                                "limit": "65536",
                                "required": "65537",
                            },
                        }
                    )
                return catalog.analyze(query)

            return SimpleNamespace(analyze=analyze), status

        def provider(declarations):
            """Source ownership remains with the caller on each explicitly requested attempt."""
            sources.append("source")
            return load_source_tables(declarations, ProjectResources(case))

        with (
            patch.object(
                yamaa_native, "_compile_reference_catalog", compile_with_limit
            ),
            patch.object(
                yamaa_native, "execute_dataset", wraps=yamaa_native.execute_dataset
            ) as execute,
            patch.object(
                yamaa_native,
                "execute_dataset_sources",
                wraps=yamaa_native.execute_dataset_sources,
            ) as execute_sources,
            patch.object(
                reference_planning,
                "_reference_comparable_types",
                side_effect=AssertionError("reference fallback"),
            ),
        ):
            with self.assertRaises(NativeReferenceLimitError) as caught:
                native_datasets.execute_with_source_provider(spec, provider)
            self.assertEqual(
                (
                    caught.exception.resource,
                    caught.exception.limit,
                    caught.exception.required,
                ),
                ("key_entries", 65536, 65537),
            )
            self.assertFalse(inject_limit)
            self.assertFalse(execute.called)
            self.assertFalse(execute_sources.called)
            self.assertEqual(sources, ["source"])
            actual = native_datasets.execute_with_source_provider(spec, provider)
            self.assertTrue(execute.called or execute_sources.called)
        self.assertEqual(sources, ["source", "source"])
        self.assertEqual(actual.result.status, "success")
        self.assertEqual(
            render_artifact(actual.result.artifact),
            (case / "expected" / Path(spec.output.path).name).read_bytes(),
        )

    def test_intermediate_capability_precedes_activation_and_data(self):
        """The previous query set cannot enter intermediate planning in either frontend."""
        case = ROOT / "specification-functions"
        project_spec = load_specification(case / "spec.yaml", SCHEMA).specification
        ordinary_spec = load_specification(
            ROOT / "specification-adlb" / "spec.yaml", SCHEMA
        ).specification
        with (
            patch.object(
                yamaa_native,
                "reference_capabilities",
                lambda: (
                    '{"protocol":"reference-analysis/1","features":["binding","output_validation","qualified_validation"]}'
                ),
            ),
            patch.object(
                native_datasets,
                "activate_project",
                side_effect=AssertionError("activation"),
            ),
        ):
            project = native_datasets.execute_with_project_functions(
                project_spec, lambda _: self.fail("source"), case / "python", SCHEMA
            )
            ordinary = native_datasets.execute_with_source_provider(
                ordinary_spec, lambda _: self.fail("source")
            )
        for actual in (ordinary, project):
            self.assertEqual(actual.result.status, "unsupported")
            self.assertEqual(
                [(f.operation, f.spec_path) for f in actual.result.features],
                [("native_intermediate_reference_validation", "$")],
            )
            self.assertEqual(actual.result.handler_counts, ())
            self.assertEqual(actual.verifications, ())

    def test_key_capability_precedes_activation_and_data(self):
        """The previous query set cannot enter key planning in either frontend."""
        case = ROOT / "specification-functions"
        project_spec = load_specification(case / "spec.yaml", SCHEMA).specification
        ordinary_spec = load_specification(
            ROOT / "specification-adlb" / "spec.yaml", SCHEMA
        ).specification
        with (
            patch.object(
                yamaa_native,
                "reference_capabilities",
                lambda: (
                    '{"protocol":"reference-analysis/1","features":["binding","output_validation","qualified_validation","intermediate_validation"]}'
                ),
            ),
            patch.object(
                native_datasets,
                "activate_project",
                side_effect=AssertionError("activation"),
            ),
        ):
            project = native_datasets.execute_with_project_functions(
                project_spec, lambda _: self.fail("source"), case / "python", SCHEMA
            )
            ordinary = native_datasets.execute_with_source_provider(
                ordinary_spec, lambda _: self.fail("source")
            )
        for actual in (ordinary, project):
            self.assertEqual(actual.result.status, "unsupported")
            self.assertEqual(
                [(f.operation, f.spec_path) for f in actual.result.features],
                [("native_key_relations", "$")],
            )
            self.assertEqual(actual.result.handler_counts, ())
            self.assertEqual(actual.verifications, ())

    def test_compile_failure_then_explicit_retry(self):
        """Compiler errors are propagated once and cannot poison a later planning attempt."""
        spec = specification(
            [Column(name="K", type="str", derivation=expression({"source": "SRC.X"}))]
        )
        failure = ValueError("reference catalog failed")
        with (
            patch.object(
                yamaa_native, "_compile_reference_catalog", side_effect=failure
            ),
            self.assertRaises(ValueError) as caught,
        ):
            self.plan(spec)
        self.assertIs(caught.exception, failure)
        self.assertEqual([column.column for column in self.plan(spec).columns], ["K"])

    def test_compiler_limits_remain_explicit(self):
        """Over-budget catalogs never become language diagnostics or partial handles."""
        fields = [{"name": str(n), "type": "str"} for n in range(4097)]
        catalog, status = yamaa_native._compile_reference_catalog(
            json.dumps(
                {
                    "protocol": "reference-catalog/1",
                    "catalog": {"outputs": fields, "datasets": []},
                }
            )
        )
        self.assertIsNone(catalog)
        self.assertEqual(
            json.loads(status)["outcome"],
            {
                "status": "limit",
                "resource": "outputs",
                "limit": "4096",
                "required": "4097",
            },
        )
        bindings = reference_bindings.BindingPlan(
            domain="OUT", datasets={}, output_columns=tuple(str(n) for n in range(4097))
        )
        with self.assertRaises(NativeReferenceLimitError) as caught:
            bind_reference_compiler(yamaa_native)(
                bindings, dict.fromkeys(bindings.output_columns, "str")
            )
        self.assertEqual(
            (
                caught.exception.resource,
                caught.exception.limit,
                caught.exception.required,
            ),
            ("outputs", 4096, 4097),
        )


if __name__ == "__main__":
    unittest.main()
