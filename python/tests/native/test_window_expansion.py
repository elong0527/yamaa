"""Native expansion owns semantics; the host retains its layer-origin objects."""

import json
from types import SimpleNamespace
from unittest.mock import Mock

import pytest

from yamaa.adapters._native_schema_interpreter import NativeSchemaInterpreter
from yamaa.adapters._native_schema_wire import NativeSchemaLimitError, encode_tree
from yamaa.schema.windows import expand_named_windows
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
    interpreter = NativeSchemaInterpreter(snapshot, {"descriptors": []}, [])
    return interpreter, analyze, snapshot


def test_captured_service_transfers_only_definition_descendants_and_preserves_use_origin(
    monkeypatch,
):
    expected = {"first": {"order_by": ["SEQ"]}, "second": {"order_by": ["SEQ"]}}
    interpreter, analyze, snapshot = service(
        {
            "status": "expanded",
            "document": encode_tree(expected),
            "references": [
                {"path": "first", "definition": "VISITS"},
                {"path": "second", "definition": "VISITS"},
            ],
        }
    )
    source = {
        "windows": {"VISITS": {"order_by": ["SEQ"]}},
        "first": "VISITS",
        "second": "VISITS",
    }
    use_origin, definition_origin, child_origin, unrelated = (
        object(),
        object(),
        object(),
        object(),
    )
    provenance = {
        "first": use_origin,
        "windows.VISITS": definition_origin,
        "windows.VISITS.order_by": child_origin,
        "windows.VISITS2.order_by": unrelated,
    }
    monkeypatch.setattr(
        "yamaa.schema.windows.class_fields", lambda *_: pytest.fail("host traversal")
    )
    snapshot.analyze = lambda *_: pytest.fail("recaptured service")
    result = expand_named_windows(
        source,
        SimpleNamespace(interpreter=interpreter),
        strict=False,
        provenance=provenance,
    )
    assert result == expected
    assert result["first"] is not result["second"]
    assert provenance["first"] is use_origin
    assert provenance["first.order_by"] is child_origin
    assert provenance["second.order_by"] is child_origin
    assert provenance["windows.VISITS2.order_by"] is unrelated
    assert "second" not in provenance
    assert source["first"] == "VISITS"
    query = json.loads(analyze.call_args.args[0])["queries"][0]
    assert query["operation"] == "expand_windows" and query["strict"] is False


@pytest.mark.parametrize(
    "outcome, exception",
    [
        (
            {
                "status": "invalid",
                "diagnostics": [
                    {
                        "path": "columns.X.derivation.row_number.window",
                        "condition": "unknown_window",
                        "requirement": "REQ-1253",
                        "context": [
                            {
                                "name": "window",
                                "value": {"kind": "text", "value": "MISSING"},
                            }
                        ],
                    }
                ],
            },
            SpecificationError,
        ),
        (
            {
                "status": "resource_limit",
                "phase": "normalization",
                "resource": "nodes",
                "limit": 10,
            },
            NativeSchemaLimitError,
        ),
        ({"status": "normalized"}, ValueError),
        (
            {
                "status": "expanded",
                "document": encode_tree({}),
                "references": [{"path": 1, "definition": "W"}],
            },
            ValueError,
        ),
    ],
)
def test_failed_native_expansion_never_publishes_provenance_or_falls_back(
    outcome, exception, monkeypatch
):
    interpreter, _, _ = service(outcome)
    origin = object()
    provenance = {"windows.W.order_by": origin}
    monkeypatch.setattr(
        "yamaa.schema.windows.class_fields", lambda *_: pytest.fail("fallback")
    )
    with pytest.raises(exception):
        expand_named_windows(
            {}, SimpleNamespace(interpreter=interpreter), provenance=provenance
        )
    assert provenance == {"windows.W.order_by": origin}
