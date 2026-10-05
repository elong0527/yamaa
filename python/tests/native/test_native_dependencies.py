"""Host binding admission and failure propagation without semantic fallback."""

import json
from types import SimpleNamespace

import pytest

from yamaa.adapters._native_dependencies import (
    NativeDependencyLimitError,
    bind_column_dependency_analyzer,
    bind_dependency_analyzer,
)


def test_bound_names_omit_external_and_completed_phase_values():
    """Only dependencies on nodes in this scheduling phase cross the Rust boundary."""
    calls = []

    def invoke(request):
        """Record the bound graph and supply an explicit independently authored order."""
        calls.append(json.loads(request))
        return json.dumps(
            {
                "protocol": "dependency-analysis/1",
                "outcome": {"status": "complete", "cycle": None, "order": [1, 0]},
            }
        )

    analyze = bind_dependency_analyzer(SimpleNamespace(analyze_dependencies=invoke))
    result = analyze(("A", "B"), {"A": ("B", "SRC.X", "ROW_VALUE"), "B": ()})
    assert calls == [{"protocol": "dependency-analysis/1", "dependencies": [[1], []]}]
    assert result.cycle is None
    assert result.order == ("B", "A")


def test_native_resource_limit_retains_exact_policy_counts():
    """Policy failures preserve resource counters rather than becoming cycle diagnostics."""

    def invoke(_):
        """Return a written-edge budget outcome without running a graph algorithm."""
        return json.dumps(
            {
                "protocol": "dependency-analysis/1",
                "outcome": {
                    "status": "limit",
                    "resource": "edges",
                    "limit": "65536",
                    "required": "65537",
                },
            }
        )

    analyze = bind_dependency_analyzer(SimpleNamespace(analyze_dependencies=invoke))
    with pytest.raises(NativeDependencyLimitError) as caught:
        analyze(("A",), {"A": ()})
    assert (caught.value.resource, caught.value.limit, caught.value.required) == (
        "edges",
        65536,
        65537,
    )


def test_transport_failure_propagates_without_host_fallback():
    """Once selected, a failing native service cannot switch to reference scheduling."""
    failure = ValueError("native graph admission failed")

    def invoke(_):
        """Raise the original boundary failure so identity can be verified."""
        raise failure

    analyze = bind_dependency_analyzer(SimpleNamespace(analyze_dependencies=invoke))
    with pytest.raises(ValueError) as caught:
        analyze(("A",), {"A": ()})
    assert caught.value is failure


def test_column_bindings_retain_completed_declarations_and_authored_keys():
    """Completed values participate in rules; source bindings do not cross the boundary."""
    calls = []

    def invoke(request):
        """Record metadata and return an authored diagnostic with its original path kind."""
        calls.append(json.loads(request))
        return json.dumps(
            {
                "protocol": "column-dependencies/1",
                "outcome": {
                    "status": "complete",
                    "order": [2, 0],
                    "diagnostics": [
                        {
                            "condition": "key_dependency",
                            "requirement": "REQ-0074",
                            "location": "expression",
                            "columns": [0, 1],
                        }
                    ],
                },
            }
        )

    native = SimpleNamespace(analyze_column_dependencies=invoke)
    analyze = bind_column_dependency_analyzer(native)
    native.analyze_column_dependencies = lambda _: pytest.fail("service changed")
    result = analyze(
        ("A", "ROW", "B"), {"A": ("ROW", "B", "SRC.X"), "B": ()}, ("B", "A"), False
    )
    assert calls == [
        {
            "protocol": "column-dependencies/1",
            "dependencies": [[1, 2], None, []],
            "keys": [2, 0],
            "has_rows": False,
        }
    ]
    assert result.order == ("B", "A")
    (finding,) = result.diagnostics
    assert (
        finding.condition,
        finding.requirement,
        finding.location,
        finding.columns,
    ) == ("key_dependency", "REQ-0074", "expression", ("A", "ROW"))


def test_column_transport_failure_never_falls_back():
    """A captured compiler failure reaches the caller unchanged."""
    failure = ValueError("column metadata refused")

    def invoke(_):
        """Inject one boundary error without running semantic code."""
        raise failure

    analyze = bind_column_dependency_analyzer(
        SimpleNamespace(analyze_column_dependencies=invoke)
    )
    with pytest.raises(ValueError) as caught:
        analyze(("A",), {"A": ()}, ("A",), False)
    assert caught.value is failure


def test_column_undeclared_key_reports_key_dependency():
    """A key naming no declared column diagnoses instead of failing lookup."""
    calls = []

    def invoke(request):
        """Record the translated keys and return a complete empty analysis."""
        calls.append(json.loads(request))
        return json.dumps(
            {
                "protocol": "column-dependencies/1",
                "outcome": {"status": "complete", "order": [0], "diagnostics": []},
            }
        )

    analyze = bind_column_dependency_analyzer(
        SimpleNamespace(analyze_column_dependencies=invoke)
    )
    result = analyze(("A",), {"A": ()}, ("A", "NOTACOLUMN"), False)
    assert calls == [
        {
            "protocol": "column-dependencies/1",
            "dependencies": [[]],
            "keys": [0],
            "has_rows": False,
        }
    ]
    assert result.order == ("A",)
    (finding,) = result.diagnostics
    assert (
        finding.condition,
        finding.requirement,
        finding.location,
        finding.columns,
    ) == ("key_dependency", "REQ-0074", "declaration", ("NOTACOLUMN",))


def test_column_undeclared_key_exempt_with_row_templates():
    """Row templates exempt undeclared keys the way the reference planner does."""
    calls = []

    def invoke(request):
        """Record the translated keys and return a complete empty analysis."""
        calls.append(json.loads(request))
        return json.dumps(
            {
                "protocol": "column-dependencies/1",
                "outcome": {"status": "complete", "order": [0], "diagnostics": []},
            }
        )

    analyze = bind_column_dependency_analyzer(
        SimpleNamespace(analyze_column_dependencies=invoke)
    )
    result = analyze(("A",), {"A": ()}, ("NOTACOLUMN",), True)
    assert calls[0]["keys"] == []
    assert result.diagnostics == ()
