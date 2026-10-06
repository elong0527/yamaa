"""The Python graph bridge grants explicit source access without host traversal."""

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
from yamaa.specification.diagnostics import ValidationDiagnostic


def service(run):
    snapshot = SimpleNamespace(analyze=Mock(), traverse_inheritance=run)
    return NativeSchemaInterpreter(
        snapshot, {"root_class": "root_class", "descriptors": []}, []
    ), snapshot


def message(operation, **values):
    return json.dumps({"protocol": "inheritance/1", "operation": operation, **values})


def result(outcome):
    return json.dumps({"protocol": "inheritance/1", "outcome": outcome})


def test_captured_graph_service_uses_only_the_requested_source_ports():
    entry = {"schema_version": "1.0", "parents": "a"}
    parent = {"schema_version": "1.0"}
    canonicalize = Mock(return_value=("/a", "/written/a"))
    read = Mock(return_value=parent)

    def run(request, dispatch):
        assert json.loads(request) == {
            "protocol": "inheritance/1",
            "entry": {"identity": "/entry", "display_path": "/entry"},
            "document": encode_tree(entry),
        }
        assert json.loads(
            dispatch(message("canonicalize", declaring="/entry", path="a"), 1024)
        ) == {
            "protocol": "inheritance/1",
            "outcome": {
                "status": "resolved",
                "identity": "/a",
                "display_path": "/written/a",
            },
        }
        assert json.loads(
            dispatch(message("read", identity="/a", display_path="/written/a"), 1024)
        )["outcome"] == {"status": "document", "document": encode_tree(parent)}
        return result(
            {
                "status": "traversed",
                "layers": [
                    {"identity": "/a", "document": encode_tree(parent)},
                    {"identity": "/entry", "document": encode_tree(entry)},
                ],
            }
        )

    interpreter, snapshot = service(run)
    snapshot.traverse_inheritance = lambda *_: pytest.fail("recaptured service")
    layers = interpreter.traverse_inheritance("/entry", entry, canonicalize, read)
    assert layers == [("/a", parent), ("/entry", entry)]
    canonicalize.assert_called_once_with("/entry", "a")
    read.assert_called_once_with("/a")
    layers[0][1]["schema_version"] = "changed"
    assert parent["schema_version"] == "1.0"


@pytest.mark.parametrize("operation", ["canonicalize", "read"])
def test_source_os_failures_are_reported_as_unavailable(operation):
    def run(request, dispatch):
        arguments = (
            {"declaring": "/entry", "path": "a"}
            if operation == "canonicalize"
            else {"identity": "/a", "display_path": "/written/a"}
        )
        assert json.loads(dispatch(message(operation, **arguments), 1024)) == {
            "protocol": "inheritance/1",
            "outcome": {"status": "unavailable"},
        }
        return result({"status": "traversed", "layers": []})

    interpreter, _ = service(run)
    interpreter.traverse_inheritance(
        "/entry",
        {},
        Mock(side_effect=OSError("unreadable")),
        Mock(side_effect=OSError("unreadable")),
    )


def test_decoder_failure_is_preserved_without_relabeling_or_fallback():
    failure = SpecificationError(
        [
            ValidationDiagnostic(
                condition="invalid_field_type",
                spec_paths=("$",),
                context={"original": True},
            )
        ]
    )

    def run(request, dispatch):
        return dispatch(message("read", identity="/a", display_path="/a"), 1024)

    interpreter, _ = service(run)
    with pytest.raises(SpecificationError) as caught:
        interpreter.traverse_inheritance(
            "/entry", {}, Mock(), Mock(side_effect=failure)
        )
    assert caught.value is failure


def test_child_diagnostics_use_the_child_context_document():
    child = {"value": "child value"}
    child_tree = encode_tree(child)
    value_node = next(
        i
        for i, node in enumerate(child_tree["nodes"])
        if node == {"kind": "text", "value": "child value"}
    )
    interpreter, _ = service(
        lambda *_: result(
            {
                "status": "invalid",
                "context_document": child_tree,
                "diagnostics": [
                    {
                        "condition": "invalid_field_type",
                        "path": "value",
                        "requirement": None,
                        "context": [
                            {
                                "name": "actual",
                                "value": {"kind": "input_value", "node": value_node},
                            }
                        ],
                    }
                ],
            }
        )
    )
    with pytest.raises(SpecificationError) as caught:
        interpreter.traverse_inheritance("/entry", {"value": "entry"}, Mock(), Mock())
    assert caught.value.diagnostics[0].context == {"actual": "child value"}


def test_cumulative_reply_allowance_is_enforced_before_returning_to_native():
    def run(request, dispatch):
        return dispatch(message("canonicalize", declaring="/entry", path="a"), 1)

    interpreter, _ = service(run)
    read = Mock()
    with pytest.raises(NativeSchemaLimitError) as caught:
        interpreter.traverse_inheritance(
            "/entry", {}, Mock(return_value=("/a", "/a")), read
        )
    assert caught.value.phase == "inheritance"
    assert caught.value.resource == "source_reply_bytes"
    read.assert_not_called()


def test_old_native_snapshots_refuse_traversal_without_reference_fallback():
    interpreter = NativeSchemaInterpreter(
        SimpleNamespace(analyze=Mock()),
        {"root_class": "root_class", "descriptors": []},
        [],
    )
    canonicalize, read = Mock(), Mock()
    with pytest.raises(NativeSchemaUnsupportedError):
        interpreter.traverse_inheritance("/entry", {}, canonicalize, read)
    canonicalize.assert_not_called()
    read.assert_not_called()


def test_unexpected_native_source_operation_has_no_filesystem_authority():
    interpreter, _ = service(
        lambda _, dispatch: dispatch(message("list_directory", path="/"), 1024)
    )
    canonicalize, read = Mock(), Mock()
    with pytest.raises(ValueError, match="source operation"):
        interpreter.traverse_inheritance("/entry", {}, canonicalize, read)
    canonicalize.assert_not_called()
    read.assert_not_called()
