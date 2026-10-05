"""Installed shared reference compiler, independent truth and actual planner ownership."""

import csv
import json
import unittest
from pathlib import Path
from unittest.mock import patch

import yamaa_native
from yamaa.adapters import native_datasets
from yamaa.adapters._native_dependencies import (
    bind_column_dependency_analyzer,
    bind_dependency_analyzer,
)
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

    def test_scope_truth_against_unchanged_reference_rules(self):
        """Replay authored scope outcomes through the separate default Python implementation."""
        with (ROOT / "reference_scope.tsv").open() as stream:
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

    def test_shared_truth_batch_prepared_and_owned(self):
        """Replay authored bindings/conditions through both interfaces after request release."""
        cases = []
        for fixture in ("reference_binding.tsv", "reference_scope.tsv"):
            with (ROOT / fixture).open() as stream:
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

    def plan(self, spec, native=True):
        """Select the actual compiled catalog and forbid reference binding fallback."""
        sources = {
            "SRC": frame_from_values(
                (TypedColumn(name="X", type="str"), TypedColumn(name="N", type="int")),
                [["x", 1]],
            )
        }
        if not native:
            return plan_execution(
                spec, sources, supported_operations=DEFAULT_EXPRESSION_OPERATIONS
            )
        with (
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
                "features": ["binding", "output_validation", "qualified_validation"],
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
