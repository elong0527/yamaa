"""Host codec preserves decoded scalar identity before Rust admission."""

import math
import struct

import pytest

from yamaa.adapters._native_schema_wire import (
    NativeSchemaLimitError,
    NativeSchemaUnsupportedError,
    decode_nodes,
    encode_tree,
    request,
    response,
)


def test_codec_preserves_scalar_types_order_wide_integers_and_negative_zero():
    value = {"b": [None, True, False, 10**100, -0.0, "\\\n\u00e9"], "a": {2: 3.0}}
    tree = encode_tree(value)
    decoded = decode_nodes(tree)[tree["root"]]
    assert list(decoded) == ["b", "a"]
    assert [type(item) for item in decoded["b"]] == [
        type(None),
        bool,
        bool,
        int,
        float,
        str,
    ]
    assert decoded["b"][3] == 10**100
    assert struct.pack(">d", decoded["b"][4]).hex() == "8000000000000000"
    assert decoded == value
    value["b"].clear()
    assert len(decode_nodes(tree)[tree["root"]]["b"]) == 6


@pytest.mark.parametrize("value", [math.inf, -math.inf, math.nan, object(), (1,)])
def test_unsupported_host_values_never_coerce_to_schema_values(value):
    with pytest.raises(NativeSchemaUnsupportedError):
        encode_tree(value)


def test_depth_and_cycles_reach_explicit_policies():
    value = []
    value.append(value)
    with pytest.raises(NativeSchemaLimitError) as error:
        encode_tree(value)
    assert (error.value.phase, error.value.resource, error.value.limit) == (
        "decoded_document",
        "depth",
        64,
    )
    with pytest.raises(NativeSchemaLimitError) as error:
        request({"text": "x" * 8_388_609})
    assert (error.value.phase, error.value.resource) == ("transport", "request_bytes")


def test_policy_and_unsupported_responses_stay_separate_from_language_failures():
    with pytest.raises(NativeSchemaLimitError) as error:
        response(
            '{"protocol":"schema/1","outcome":{"status":"resource_limit","phase":"type","resource":"bytes","limit":65536}}'
        )
    assert (error.value.phase, error.value.resource, error.value.limit) == (
        "type",
        "bytes",
        65536,
    )
    with pytest.raises(NativeSchemaUnsupportedError) as error:
        response(
            '{"protocol":"schema/1","outcome":{"status":"unsupported","feature":"normalized_mapping_key"}}'
        )
    assert error.value.feature == "normalized_mapping_key"
