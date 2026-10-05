"""Installed shared graph analysis and actual native planner ownership."""

import csv
import json
import unittest
from pathlib import Path
from unittest.mock import patch

import yamaa_native
from yamaa.adapters import native_datasets
from yamaa.adapters._native_dependencies import (
    NativeDependencyLimitError,
    bind_dependency_analyzer,
)
from yamaa.io import ProjectResources, load_source_tables, render_artifact
from yamaa.io.polars import frame_from_values
from yamaa.models import TypedColumn
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


def source(name):
    """Construct a source expression without fixture rewriting or derived expectations."""
    return HandledExpression(value=Expression(root={"source": name}))


def specification(columns, rows=None):
    """A minimal current-schema input for planner ownership and diagnostic tests."""
    return Specification(
        schema_version="1.0",
        domain="OUT",
        input={"SRC": DatasetSource(path="source.csv")},
        base="SRC",
        keys=[columns[0].name],
        output=Output(path="out.csv", columns=[column.name for column in columns]),
        columns=columns,
        rows=rows,
    )


class InstalledDependencies(unittest.TestCase):
    """The optional frontend must use installed Rust, with independent expected results."""

    def test_shared_graph_truth(self):
        """Replay authored cycles and orders without relying on another graph algorithm."""
        with (ROOT / "dependency_analysis.tsv").open() as stream:
            cases = list(csv.DictReader(stream, delimiter="\t"))
        for case in cases:
            with self.subTest(case=case["case"]):
                graph = [[] for _ in range(int(case["nodes"]))]
                if case["edges"] != "-":
                    for entry in case["edges"].split(";"):
                        node, edges = entry.split(":")
                        graph[int(node)] = [int(n) for n in edges.split(",")]
                expected = {
                    "protocol": "dependency-analysis/1",
                    "outcome": {
                        "status": "complete",
                        "cycle": None
                        if case["cycle"] == "-"
                        else [int(n) for n in case["cycle"].split(",")],
                        "order": []
                        if case["order"] == "-"
                        else [int(n) for n in case["order"].split(",")],
                    },
                }
                request = json.dumps(
                    {"protocol": "dependency-analysis/1", "dependencies": graph}
                )
                for _ in range(2):
                    self.assertEqual(
                        json.loads(yamaa_native.analyze_dependencies(request)), expected
                    )

    def test_native_planning_forbids_reference_graphs_and_captures_service(self):
        """Original CSVs survive real Rust scheduling, even if the provider swaps the API."""
        for name in (
            "specification-adlb",
            "specification-windows",
            "specification-lookup",
            "specification-functions",
        ):
            with self.subTest(case=name):
                case = ROOT / name
                spec = load_specification(case / "spec.yaml", SCHEMA).specification
                original = yamaa_native.analyze_dependencies
                effects = []

                def invoke(request, effects=effects, original=original):
                    """Record an actual installed Rust call, without producing expected truth."""
                    effects.append("graph")
                    return original(request)

                def provider(declarations, effects=effects, case=case):
                    """The chosen compiler implementation is captured before source effects."""
                    effects.append("source")
                    yamaa_native.analyze_dependencies = lambda _: self.fail(
                        "provider replaced the captured compiler"
                    )
                    return load_source_tables(declarations, ProjectResources(case))

                with (
                    patch.object(yamaa_native, "analyze_dependencies", invoke),
                    patch.object(
                        reference_planning,
                        "_find_cycle",
                        side_effect=AssertionError("Python cycle algorithm"),
                    ),
                    patch.object(
                        reference_planning,
                        "_topological_row_order",
                        side_effect=AssertionError("Python scheduling algorithm"),
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
                self.assertEqual(effects[0], "source")
                self.assertEqual(effects.count("source"), 1)
                self.assertGreater(effects.count("graph"), 0)
                expected = case / "expected" / Path(spec.output.path).name
                self.assertEqual(
                    render_artifact(actual.result.artifact), expected.read_bytes()
                )

    def test_missing_compiler_precedes_activation_and_data(self):
        """An older native installation cannot silently fall back after host effects."""
        case = ROOT / "specification-functions"
        spec = load_specification(case / "spec.yaml", SCHEMA).specification
        with (
            patch.object(yamaa_native, "analyze_dependencies", None),
            patch.object(
                native_datasets,
                "activate_project",
                side_effect=AssertionError("activation"),
            ),
            self.assertRaisesRegex(TypeError, "analyze_dependencies"),
        ):
            native_datasets.execute_with_project_functions(
                spec, lambda _: self.fail("source read"), case / "python", SCHEMA
            )

    def plan(self, spec):
        """Run real planning with the installed service and prohibit reference algorithms."""
        with (
            patch.object(reference_planning, "_find_cycle", side_effect=AssertionError),
            patch.object(
                reference_planning, "_topological_row_order", side_effect=AssertionError
            ),
        ):
            return plan_execution(
                spec,
                {
                    "SRC": frame_from_values(
                        (TypedColumn(name="X", type="str"),), [["x"]]
                    )
                },
                dependency_analyzer=bind_dependency_analyzer(yamaa_native),
            )

    def test_newly_ready_row_precedes_waiting_row_and_completed_values_are_external(
        self,
    ):
        """Stable order is recomputed after each node, retaining later column-phase reads."""
        spec = specification(
            [Column(name=name, type="str") for name in ("A", "B", "C", "D")]
            + [Column(name="AFTER", type="str", derivation=source("A"))],
            [
                Row(
                    id="r",
                    derivations={
                        "D": source("SRC.X"),
                        "B": source("C"),
                        "A": source("C"),
                        "C": source("SRC.X"),
                    },
                )
            ],
        )
        plan = self.plan(spec)
        self.assertEqual(
            [d.column for d in plan.rows[0].derivations], ["C", "A", "B", "D"]
        )
        self.assertEqual([d.column for d in plan.columns], ["AFTER"])

    def test_column_cycle_retains_canonical_paths_and_context(self):
        """The installed Rust result feeds existing REQ-0072 diagnostic construction."""
        spec = specification(
            [
                Column(name="A", type="str", derivation=source("B")),
                Column(name="B", type="str", derivation=source("A")),
            ]
        )
        with self.assertRaises(ExecutionPlanningError) as caught:
            self.plan(spec)
        diagnostic = caught.exception.diagnostics[0]
        self.assertEqual(diagnostic.condition, "dependency_cycle")
        self.assertEqual(diagnostic.requirement, "REQ-0072")
        self.assertEqual(diagnostic.context, {"cycle": ["A", "B", "A"]})
        self.assertEqual(
            diagnostic.spec_paths,
            (
                "columns.A.derivation.source",
                "columns.B.derivation.source",
            ),
        )

    def test_forward_reference_is_not_repaired_by_rust_order(self):
        """A sortable column graph still obeys the language's declaration restriction."""
        spec = specification(
            [
                Column(name="K", type="str", derivation=source("SRC.X")),
                Column(name="A", type="str", derivation=source("B")),
                Column(name="B", type="str", derivation=source("SRC.X")),
            ]
        )
        with self.assertRaises(ExecutionPlanningError) as caught:
            self.plan(spec)
        (diagnostic,) = caught.exception.diagnostics
        self.assertEqual(diagnostic.condition, "forward_reference")
        self.assertEqual(diagnostic.context, {"column": "A", "dependency": "B"})
        self.assertEqual(diagnostic.spec_paths, ("columns.A.derivation.source",))

    def test_intermediate_cycle_uses_shared_analysis_and_existing_provenance(self):
        """Intermediate graph edges use the same Rust service without changing REQ-1263."""
        spec = specification(
            [
                Column(name="K", type="str", derivation=source("SRC.X")),
            ]
        ).model_copy(
            update={
                "intermediates": [
                    Intermediate(
                        id="A",
                        dataset="SRC",
                        key={"X": "SRC.X"},
                        derivations={"Y": source("B.X")},
                    ),
                    Intermediate(
                        id="B",
                        dataset="SRC",
                        key={"X": "SRC.X"},
                        derivations={"Y": source("A.X")},
                    ),
                ]
            }
        )
        with self.assertRaises(ExecutionPlanningError) as caught:
            self.plan(spec)
        (diagnostic,) = [
            d for d in caught.exception.diagnostics if d.condition == "dependency_cycle"
        ]
        self.assertEqual(diagnostic.requirement, "REQ-1263")
        self.assertEqual(diagnostic.context, {"cycle": ["A", "B", "A"]})
        self.assertEqual(
            diagnostic.spec_paths,
            (
                "intermediates[0].derivations.Y.source",
                "intermediates[1].derivations.Y.source",
            ),
        )

    def test_installed_admission_and_resource_failures_then_reuse(self):
        """Reject malformed indices and excessive graphs without poisoning later calls."""
        for graph in ([[1]], [[True]], [[-1]], [[0.5]]):
            with self.subTest(graph=graph), self.assertRaises(ValueError):
                yamaa_native.analyze_dependencies(
                    json.dumps(
                        {
                            "protocol": "dependency-analysis/1",
                            "dependencies": graph,
                        }
                    )
                )
        analyze = bind_dependency_analyzer(yamaa_native)
        with self.assertRaises(NativeDependencyLimitError) as caught:
            analyze(tuple(str(n) for n in range(4097)), {})
        self.assertEqual(
            (
                caught.exception.resource,
                caught.exception.limit,
                caught.exception.required,
            ),
            ("nodes", 4096, 4097),
        )
        self.assertEqual(analyze(("A",), {"A": ("A",)}).cycle, ("A", "A"))


if __name__ == "__main__":
    unittest.main()
