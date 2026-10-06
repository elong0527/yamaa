"""Composition owns semantics in Rust; retained layer paths stay in the host."""

import json
from types import SimpleNamespace
from unittest.mock import Mock

import pytest

from yamaa.adapters._native_schema_interpreter import NativeSchemaInterpreter
from yamaa.adapters._native_schema_wire import NativeSchemaLimitError, encode_tree
from yamaa.schema.inheritance import _merge_layers
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
    return NativeSchemaInterpreter(snapshot, {"descriptors": []}, []), snapshot, analyze


def test_captured_composition_attaches_retained_layer_paths_without_host_merging(
    tmp_path, monkeypatch
):
    expected = {"columns": [{"name": "X", "label": "later"}]}
    interpreter, snapshot, analyze = service(
        {
            "status": "composed",
            "document": encode_tree(expected),
            "provenance": [
                {"path": "columns.X", "layer": 0},
                {"path": "columns.X.label", "layer": 1},
            ],
        }
    )
    first, second = tmp_path / "parent.yaml", tmp_path / "entry.yaml"
    contributions = [
        (first, {"columns": [{"name": "X"}]}),
        (second, {"columns": [{"name": "X", "label": "later"}]}),
    ]
    snapshot.analyze = lambda *_: pytest.fail("recaptured service")
    for name in ["_merge_member", "_compose_value", "_materialize_fragments"]:
        monkeypatch.setattr(
            f"yamaa.schema.inheritance.{name}",
            lambda *_: pytest.fail("host composition"),
        )
    result, origins, diagnostics = _merge_layers(
        contributions, SimpleNamespace(interpreter=interpreter)
    )
    assert result == expected and diagnostics == []
    assert origins["columns.X"].file == first
    assert origins["columns.X.label"].file == second
    assert origins["columns.X.label"].spec_path == "columns.X.label"
    query = json.loads(analyze.call_args.args[0])["queries"][0]
    assert query == {
        "operation": "compose_layers",
        "layers": [encode_tree(value) for _, value in contributions],
    }
    assert contributions[0][1] == {"columns": [{"name": "X"}]}


def test_failure_context_indices_address_the_retained_composed_tree():
    # This node index deliberately differs from either authored layer's value.
    context = encode_tree({"extra": "before", "invalid": ["COMPOSED"]})
    node = next(
        i
        for i, value in enumerate(context["nodes"])
        if value.get("value") == "COMPOSED"
    )
    interpreter, _, _ = service(
        {
            "status": "invalid",
            "context_document": context,
            "diagnostics": [
                {
                    "path": "<normalization>.value",
                    "condition": "invalid_field_type",
                    "requirement": None,
                    "context": [
                        {
                            "name": "actual",
                            "value": {"kind": "input_value", "node": node},
                        }
                    ],
                }
            ],
        }
    )
    with pytest.raises(SpecificationError) as caught:
        interpreter.compose_layers([{"value": "FIRST"}, {"value": "SECOND"}])
    assert caught.value.diagnostics[0].context == {"actual": "COMPOSED"}


@pytest.mark.parametrize(
    "provenance",
    [
        [{"path": "x", "layer": True}],
        [{"path": "x", "layer": -1}],
        [{"path": "x", "layer": 1}],
        [{"path": 1, "layer": 0}],
        [{"path": "x", "layer": 0}, {"path": "x", "layer": 0}],
    ],
)
def test_invalid_origin_links_are_rejected(provenance):
    interpreter, _, _ = service(
        {"status": "composed", "document": encode_tree({}), "provenance": provenance}
    )
    with pytest.raises(ValueError):
        interpreter.compose_layers([{}])


def test_limit_refusal_does_not_fall_back(monkeypatch):
    interpreter, _, _ = service(
        {
            "status": "resource_limit",
            "phase": "normalization",
            "resource": "nodes",
            "limit": 10,
        }
    )
    monkeypatch.setattr(
        "yamaa.schema.inheritance._merge_member", lambda *_: pytest.fail("fallback")
    )
    with pytest.raises(NativeSchemaLimitError):
        _merge_layers([], SimpleNamespace(interpreter=interpreter))
