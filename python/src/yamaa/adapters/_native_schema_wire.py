"""Bounded decoded-value transport for the shared schema interpreter."""

from __future__ import annotations

import json
import math
import struct


class NativeSchemaLimitError(RuntimeError):
    """A resource policy refusal is distinct from an invalid specification."""

    def __init__(self, outcome):
        self.phase = outcome["phase"]
        self.resource = outcome["resource"]
        self.limit = outcome["limit"]
        super().__init__(f"native schema {self.phase} {self.resource} limit exceeded")


class NativeSchemaUnsupportedError(RuntimeError):
    """A host value or normalization outside the admitted transport vocabulary."""

    def __init__(self, outcome):
        self.feature = outcome["feature"]
        self.outcome = dict(outcome)
        super().__init__(f"unsupported native schema feature: {self.feature}")


def limit(phase: str, resource: str, maximum: int):
    """Retain the same resource envelope for host-side admission limits."""
    raise NativeSchemaLimitError(
        {"phase": phase, "resource": resource, "limit": maximum}
    )


def check_outcome(outcome):
    """Raise policy and unsupported outcomes without attempting another backend."""
    if outcome["status"] == "resource_limit":
        raise NativeSchemaLimitError(outcome)
    if outcome["status"] == "unsupported":
        raise NativeSchemaUnsupportedError(outcome)
    return outcome


def request(value: object) -> str:
    """Bound serialized bytes before joining the entire native request."""
    chunks = []
    size = 0
    for chunk in json.JSONEncoder(separators=(",", ":"), allow_nan=False).iterencode(
        {"protocol": "schema/1", **value}
    ):
        size += len(chunk)  # ensure_ascii defaults to true.
        if size > 8_388_608:
            limit("transport", "request_bytes", 8_388_608)
        chunks.append(chunk)
    return "".join(chunks)


def response(text: str):
    """Decode the captured service's closed response envelope."""
    if len(text) > 16_777_216 or len(text.encode("utf-8")) > 16_777_216:
        limit("transport", "response_bytes", 16_777_216)
    envelope = json.loads(text)
    if envelope["protocol"] != "schema/1":
        raise ValueError("unsupported native schema response protocol")
    return check_outcome(envelope["outcome"])


def encode_tree(value: object) -> dict:
    """Copy builtin decoded values, preserving scalar types and all finite float bits."""
    nodes = []
    text_bytes = 0
    edges = 0
    reserved = 0

    def push(value, depth):
        nonlocal text_bytes, edges, reserved
        if depth > 64:
            limit("decoded_document", "depth", 64)
        reserved += 1
        if reserved > 131_072:
            limit("decoded_document", "nodes", 131_072)
        kind = type(value)
        if value is None:
            node = {"kind": "null"}
        elif kind is bool:
            node = {"kind": "boolean", "value": value}
        elif kind is int:
            decimal = str(value)
            text_bytes += len(decimal)
            node = {"kind": "integer", "value": decimal}
        elif kind is float:
            if not math.isfinite(value):
                raise NativeSchemaUnsupportedError({"feature": "non_finite_host_float"})
            node = {"kind": "float", "bits": struct.pack(">d", value).hex()}
        elif kind is str:
            text_bytes += len(value.encode("utf-8"))
            node = {"kind": "text", "value": value}
        elif kind in (list, dict):
            edges += len(value) * (2 if kind is dict else 1)
            if edges > 262_144:
                limit("decoded_document", "edges", 262_144)
            if kind is list:
                node = {
                    "kind": "sequence",
                    "items": [push(v, depth + 1) for v in value],
                }
            else:
                entries = []
                for key, child in value.items():
                    if key is not None and type(key) not in (str, int, float, bool):
                        raise NativeSchemaUnsupportedError(
                            {"feature": "host_mapping_key"}
                        )
                    entries.append([push(key, depth + 1), push(child, depth + 1)])
                node = {"kind": "mapping", "entries": entries}
        else:
            raise NativeSchemaUnsupportedError({"feature": "host_value_type"})
        if text_bytes > 8_388_608:
            limit("decoded_document", "text_bytes", 8_388_608)
        index = len(nodes)
        nodes.append(node)
        return index

    root = push(value, 0)
    return {"nodes": nodes, "root": root}


def decode_nodes(tree: dict) -> list:
    """Decode a native-owned admitted arena without numeric JSON coercion."""
    values = []
    for node in tree["nodes"]:
        kind = node["kind"]
        if kind == "null":
            value = None
        elif kind in ("boolean", "text"):
            value = node["value"]
        elif kind == "integer":
            value = int(node["value"])
        elif kind == "float":
            value = struct.unpack(">d", bytes.fromhex(node["bits"]))[0]
        elif kind == "sequence":
            value = [values[index] for index in node["items"]]
        elif kind == "mapping":
            value = {values[key]: values[child] for key, child in node["entries"]}
        else:
            raise ValueError("unknown native schema node kind")
        values.append(value)
    return values
