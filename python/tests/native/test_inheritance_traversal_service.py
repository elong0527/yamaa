"""The retired Python callback probe grants no source or encoding authority."""

from types import SimpleNamespace
from unittest.mock import Mock, patch

import pytest

from yamaa.adapters._native_schema_interpreter import NativeSchemaInterpreter
from yamaa.adapters._native_schema_wire import NativeSchemaUnsupportedError


@pytest.mark.parametrize("document", [{}, {"parents": "a"}, {"value": object()}])
def test_retired_inheritance_probe_refuses_before_encoding_or_source_effects(document):
    legacy = Mock(side_effect=AssertionError("legacy JSON callback invoked"))
    snapshot = SimpleNamespace(analyze=Mock(), traverse_inheritance=legacy)
    canonicalize = Mock(side_effect=AssertionError("canonicalize authority"))
    read = Mock(side_effect=AssertionError("source authority"))
    with patch(
        "yamaa.adapters._native_schema_interpreter.encode_tree",
        side_effect=AssertionError("document encoded"),
    ):
        interpreter = NativeSchemaInterpreter(
            snapshot, {"root_class": "root_class", "descriptors": []}, []
        )
        with pytest.raises(NativeSchemaUnsupportedError) as caught:
            interpreter.traverse_inheritance("/entry", document, canonicalize, read)
    assert caught.value.outcome == {"feature": "inheritance_traversal"}
    legacy.assert_not_called()
    snapshot.analyze.assert_not_called()
    canonicalize.assert_not_called()
    read.assert_not_called()


def test_snapshot_inheritance_attribute_is_never_captured():
    class Snapshot:
        analyze = Mock()

        @property
        def traverse_inheritance(self):
            raise AssertionError("retired attribute accessed")

    interpreter = NativeSchemaInterpreter(
        Snapshot(), {"root_class": "root_class", "descriptors": []}, []
    )
    with pytest.raises(NativeSchemaUnsupportedError) as caught:
        interpreter.traverse_inheritance(object(), object(), object(), object())
    assert caught.value.outcome == {"feature": "inheritance_traversal"}


def test_snapshots_without_traversal_have_the_same_explicit_refusal():
    interpreter = NativeSchemaInterpreter(
        SimpleNamespace(analyze=Mock()),
        {"root_class": "root_class", "descriptors": []},
        [],
    )
    canonicalize, read = Mock(), Mock()
    with pytest.raises(NativeSchemaUnsupportedError) as caught:
        interpreter.traverse_inheritance("/entry", {}, canonicalize, read)
    assert caught.value.outcome == {"feature": "inheritance_traversal"}
    canonicalize.assert_not_called()
    read.assert_not_called()
