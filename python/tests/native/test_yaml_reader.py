"""Captured YAML service and host policy failures never fall back to a parser."""

import json
from pathlib import Path
from types import SimpleNamespace

import pytest

from yamaa.adapters import native_specification
from yamaa.adapters._native_schema_wire import NativeSchemaLimitError
from yamaa.adapters._native_yaml import MAX_SOURCE_BYTES, NativeYamlReader
from yamaa.specification import SpecificationError


def reply(outcome, protocol="yaml/1"):
    return json.dumps({"protocol": protocol, "outcome": outcome})


def decoded(value="9007199254740993"):
    return {
        "status": "decoded",
        "document": {"nodes": [{"kind": "integer", "value": value}], "root": 0},
        "locations": [{"line": 1, "column": 1, "offset": 0}],
    }


def test_native_services_are_required_before_any_filesystem_resolution(monkeypatch):
    def forbidden(*args, **kwargs):
        pytest.fail("filesystem accessed before decoder admission")

    monkeypatch.setattr(Path, "resolve", forbidden)
    with pytest.raises(TypeError, match="decode_yaml must be callable"):
        native_specification.load_schema_bundle(
            "unread", native=SimpleNamespace(_compile_schema=lambda _: None)
        )


def test_reader_captures_callable_and_owns_bytes_before_decoding(tmp_path):
    path = tmp_path / "spec.yaml"
    path.write_bytes(b"9007199254740993")
    calls = []

    def decode(raw):
        assert type(raw) is bytes
        calls.append(raw)
        path.unlink()
        return reply(decoded())

    native = SimpleNamespace(decode_yaml=decode)
    reader = NativeYamlReader(native)
    native.decode_yaml = lambda _: pytest.fail("decoder recaptured after IO")
    assert reader.read_document(path) == 9007199254740993
    assert calls == [b"9007199254740993"]
    assert not path.exists()


def test_null_inheritance_snapshot_is_validated_without_reading_it_again(tmp_path):
    from yamaa.schema import resolve_specification
    from yamaa.specification.schema import SchemaBundle

    calls = []

    def read(path):
        calls.append(path)
        if len(calls) != 1:
            pytest.fail("retained null snapshot was reread")

    bundle = SchemaBundle(
        "1.0", tmp_path / "schema.yaml", {}, {}, {}, document_reader=read
    )
    with pytest.raises(SpecificationError) as caught:
        resolve_specification(tmp_path / "entry.yaml", bundle)
    assert caught.value.diagnostics[0].condition == "invalid_field_type"
    assert calls == [(tmp_path / "entry.yaml").resolve()]


def test_original_filename_and_unicode_paths_survive_native_diagnostics():
    findings = [
        {
            "condition": "non_ascii_source",
            "spec_paths": ["$"],
            "context": {"line": 3, "column": 7},
        },
        {
            "condition": "invalid_text",
            "spec_paths": ["$.label[1]"],
            "context": {"code_point": "U+D800", "offset": 2},
        },
    ]
    outcome = {"status": "invalid", "diagnostics": findings}
    reader = NativeYamlReader(SimpleNamespace(decode_yaml=lambda _: reply(outcome)))
    with pytest.raises(SpecificationError) as caught:
        reader.decode_tree(b"retained", Path("parent.yaml"))
    diagnostics = caught.value.diagnostics
    assert diagnostics[0].context == {"line": 3, "column": 7, "path": "parent.yaml"}
    assert diagnostics[1].spec_paths == ("$.label[1]",)
    assert diagnostics[1].context == {"code_point": "U+D800", "offset": 2}
    assert caught.value.native_outcome == outcome
    assert "path" not in outcome["diagnostics"][0]["context"]


def test_source_limits_stop_before_native_call_and_file_read_is_bounded(tmp_path):
    calls = []
    reader = NativeYamlReader(
        SimpleNamespace(decode_yaml=lambda raw: calls.append(raw) or reply(decoded()))
    )
    path = tmp_path / "large.yaml"
    path.write_bytes(b" " * (MAX_SOURCE_BYTES + 20))
    with pytest.raises(NativeSchemaLimitError) as caught:
        reader.read_document(path)
    assert (caught.value.phase, caught.value.resource, caught.value.limit) == (
        "yaml_source",
        "source_bytes",
        MAX_SOURCE_BYTES,
    )
    assert calls == []
    with pytest.raises(TypeError, match="snapshot must be bytes"):
        reader.decode_tree(bytearray(b"1"), path)


def test_native_resource_refusal_is_distinct_from_source_validation():
    outcome = {
        "status": "resource_limit",
        "phase": "yaml_source",
        "resource": "parse_bytes",
        "limit": 33554432,
    }
    reader = NativeYamlReader(SimpleNamespace(decode_yaml=lambda _: reply(outcome)))
    with pytest.raises(NativeSchemaLimitError) as caught:
        reader.decode_tree(b"retained", Path("source.yaml"))
    assert (caught.value.resource, caught.value.limit) == ("parse_bytes", 33554432)


def test_host_integer_digit_policy_is_explicit_and_not_relaxed(monkeypatch):
    import sys

    native = SimpleNamespace(decode_yaml=lambda _: reply(decoded("9" * 641)))
    reader = NativeYamlReader(native)
    monkeypatch.setattr(sys, "get_int_max_str_digits", lambda: 640)
    tree = reader.decode_tree(b"retained", Path("source.yaml"))
    with pytest.raises(NativeSchemaLimitError) as caught:
        reader.values(tree)
    assert (caught.value.phase, caught.value.resource, caught.value.limit) == (
        "yaml_source",
        "host_integer_digits",
        640,
    )
    assert sys.get_int_max_str_digits() == 640
    assert reader.values(decoded()["document"]) == [9007199254740993]


@pytest.mark.parametrize(
    ("text", "error"),
    [
        (b"{}", TypeError),
        (reply(decoded(), protocol="yaml/2"), ValueError),
        (reply({"status": "unknown"}), ValueError),
        (reply({"status": "invalid", "diagnostics": []}), ValueError),
        ("not json", ValueError),
    ],
)
def test_broken_native_responses_are_not_reinterpreted_or_fallbacks(text, error):
    reader = NativeYamlReader(SimpleNamespace(decode_yaml=lambda _: text))
    with pytest.raises(error):
        reader.decode_tree(b"retained", Path("source.yaml"))
