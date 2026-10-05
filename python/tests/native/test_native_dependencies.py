"""Host binding admission and failure propagation without semantic fallback."""

import json
from types import SimpleNamespace

import pytest

from yamaa.adapters._native_dependencies import (
    NativeDependencyLimitError,
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
