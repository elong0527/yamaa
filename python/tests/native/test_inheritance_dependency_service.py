"""Captured inheritance service results preserve ownership and failure semantics."""

import json
from types import SimpleNamespace
from unittest.mock import Mock

import pytest

from yamaa.adapters._native_schema_interpreter import NativeSchemaInterpreter
from yamaa.adapters._native_schema_wire import (
    NativeSchemaLimitError,
    NativeSchemaUnsupportedError,
    encode_tree,
)
from yamaa.specification import SpecificationError


def service(outcome):
    analyze = Mock(
        return_value=json.dumps(
            {
                "protocol": "schema/1",
                "outcome": {"status": "analyzed", "results": [outcome]},
            }
        )
    )
    snapshot = SimpleNamespace(analyze=analyze)
    return (
        NativeSchemaInterpreter(
            snapshot, {"root_class": "root_class", "descriptors": []}, []
        ),
        snapshot,
        analyze,
    )


def test_dependency_query_uses_captured_service_and_returns_owned_document():
    source = {"columns": [{"name": "B"}, {"name": "A"}]}
    expected = {"columns": [{"name": "A"}, {"name": "B"}]}
    interpreter, snapshot, analyze = service(
        {"status": "normalized", "document": encode_tree(expected)}
    )
    snapshot.analyze = lambda *_: pytest.fail("recaptured service")
    result = interpreter.resolve_inheritance_dependencies(source)
    assert result == expected
    assert analyze.call_count == 1
    assert json.loads(analyze.call_args.args[0])["queries"] == [
        {
            "operation": "resolve_inheritance_dependencies",
            "document": encode_tree(source),
        }
    ]
    result["columns"][0]["name"] = "CHANGED"
    assert source["columns"][1]["name"] == "A"


def test_cycle_context_keeps_ordered_names_and_prevents_partial_publication():
    interpreter, _, _ = service(
        {
            "status": "invalid",
            "diagnostics": [
                {
                    "path": "columns",
                    "condition": "dependency_cycle",
                    "requirement": "REQ-0072",
                    "context": [
                        {
                            "name": "cycle",
                            "value": {
                                "kind": "text_list",
                                "value": ["DOWNSTREAM", "A", "B"],
                            },
                        }
                    ],
                }
            ],
        }
    )
    with pytest.raises(SpecificationError) as caught:
        interpreter.resolve_inheritance_dependencies({})
    finding = caught.value.diagnostics[0]
    assert finding.spec_paths == ("columns",)
    assert finding.condition == "dependency_cycle"
    assert finding.context == {"cycle": ["DOWNSTREAM", "A", "B"]}


@pytest.mark.parametrize(
    "outcome,error",
    [
        (
            {
                "status": "resource_limit",
                "phase": "inheritance_numeric",
                "resource": "depth",
                "limit": 64,
            },
            NativeSchemaLimitError,
        ),
        ({"status": "unsupported", "feature": "example"}, NativeSchemaUnsupportedError),
        ({"status": "normalized", "document": encode_tree([])}, TypeError),
        ({"status": "valid", "diagnostics": []}, ValueError),
    ],
)
def test_refusals_and_malformed_results_are_not_published(outcome, error):
    interpreter, _, _ = service(outcome)
    with pytest.raises(error):
        interpreter.resolve_inheritance_dependencies({})
