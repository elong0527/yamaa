"""The captured Rust service owns layer traversal and returns original-path findings."""

import json
from types import SimpleNamespace
from unittest.mock import Mock

import pytest

from yamaa.adapters._native_schema_interpreter import NativeSchemaInterpreter
from yamaa.adapters._native_schema_wire import NativeSchemaLimitError, encode_tree
from yamaa.schema.inheritance import _validate_layer


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
    interpreter = NativeSchemaInterpreter(
        snapshot, {"root_class": "root_class", "descriptors": []}, []
    )
    return interpreter, snapshot, analyze


def test_layer_delegates_once_without_host_traversal(monkeypatch):
    source = {"schema_version": "1.0", "input": {"SRC": "data.csv"}}
    expected = {"schema_version": "1.0", "input": {"SRC": {"path": "data.csv"}}}
    interpreter, snapshot, analyze = service(
        {
            "status": "normalized",
            "document": encode_tree(expected),
            "origins": [],
        }
    )
    snapshot.analyze = lambda *_: pytest.fail("recaptured service")
    for name in (
        "class_fields",
        "_validate_partial_member",
        "validate_descriptor_value",
        "normalize_descriptor_value",
    ):
        monkeypatch.setattr(
            f"yamaa.schema.inheritance.{name}", lambda *_: pytest.fail("host traversal")
        )
    result, findings = _validate_layer(source, SimpleNamespace(interpreter=interpreter))
    assert result == expected and findings == []
    assert analyze.call_count == 1
    assert json.loads(analyze.call_args.args[0])["queries"] == [
        {"operation": "normalize_layer", "document": encode_tree(source)}
    ]
    result["input"]["SRC"]["path"] = "changed"
    assert source["input"]["SRC"] == "data.csv"


def test_invalid_result_has_no_partial_document_and_retains_input_context():
    source = {"schema_version": "1.0", "columns": [{"name": "X", "type": "BAD"}]}
    encoded = encode_tree(source)
    node = next(i for i, n in enumerate(encoded["nodes"]) if n.get("value") == "BAD")
    interpreter, _, _ = service(
        {
            "status": "invalid",
            "diagnostics": [
                {
                    "path": "columns.X.type",
                    "condition": "value_not_permitted",
                    "requirement": "REQ-0287",
                    "context": [
                        {
                            "name": "value",
                            "value": {"kind": "input_value", "node": node},
                        }
                    ],
                }
            ],
        }
    )
    value, findings = _validate_layer(source, SimpleNamespace(interpreter=interpreter))
    assert value is None
    assert len(findings) == 1
    assert findings[0].spec_paths == ("columns.X.type",)
    assert findings[0].context == {"value": "BAD"}


def test_limit_refusal_does_not_fall_back(monkeypatch):
    interpreter, _, _ = service(
        {
            "status": "resource_limit",
            "phase": "validation",
            "resource": "work",
            "limit": 1,
        }
    )
    monkeypatch.setattr(
        "yamaa.schema.inheritance.class_fields", lambda *_: pytest.fail("fallback")
    )
    with pytest.raises(NativeSchemaLimitError):
        _validate_layer({}, SimpleNamespace(interpreter=interpreter))


@pytest.mark.parametrize(
    "outcome,error",
    [
        ({"status": "valid", "diagnostics": []}, ValueError),
        ({"status": "normalized", "document": encode_tree([])}, TypeError),
    ],
)
def test_malformed_success_is_not_published(outcome, error):
    interpreter, _, _ = service(outcome)
    with pytest.raises(error):
        interpreter.normalize_layer({"schema_version": "1.0"})
