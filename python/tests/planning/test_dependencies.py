"""Independent shared graph truth also qualifies the unchanged reference planner."""

import csv
from pathlib import Path
from types import SimpleNamespace

import pytest

from yamaa.planning.execution import _find_cycle, _topological_row_order

TRUTH = (
    Path(__file__).resolve().parents[3]
    / "rust/crates/yamaa-core/tests/fixtures/dependency_analysis.tsv"
)
with TRUTH.open() as stream:
    CASES = list(csv.DictReader(stream, delimiter="\t"))


@pytest.mark.parametrize("case", CASES, ids=lambda case: case["case"])
def test_reference_graph_analysis_against_authored_truth(case):
    names = tuple(str(n) for n in range(int(case["nodes"])))
    graph = dict.fromkeys(names, ())
    if case["edges"] != "-":
        for entry in case["edges"].split(";"):
            node, dependencies = entry.split(":")
            graph[node] = tuple(dependencies.split(","))
    expected_cycle = None if case["cycle"] == "-" else tuple(case["cycle"].split(","))
    expected_order = () if case["order"] == "-" else tuple(case["order"].split(","))
    assert _find_cycle(names, graph) == expected_cycle
    derivations = {
        name: SimpleNamespace(column=name, dependencies=dependencies)
        for name, dependencies in graph.items()
    }
    assert (
        tuple(item.column for item in _topological_row_order(derivations, names))
        == expected_order
    )
