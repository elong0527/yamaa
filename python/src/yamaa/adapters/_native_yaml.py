"""Captured byte decoder for opt-in loading; filesystem authority stays in Python."""

from __future__ import annotations

import json
import sys
from pathlib import Path

from yamaa.adapters._native_schema_wire import check_outcome, decode_nodes, limit
from yamaa.specification.diagnostics import SpecificationError, ValidationDiagnostic

MAX_SOURCE_BYTES = 8_388_608
MAX_RESPONSE_BYTES = 16_777_216


class NativeYamlReader:
    """Retain one decoder callable before reading any schema or specification."""

    def __init__(self, native):
        decode = getattr(native, "decode_yaml", None)
        if not callable(decode):
            raise TypeError("native decode_yaml must be callable")
        self._decode = decode

    def decode_tree(self, raw: bytes, path: Path) -> dict:
        """Admit an immutable byte snapshot without a host YAML parser."""
        if type(raw) is not bytes:
            raise TypeError("YAML snapshot must be bytes")
        if len(raw) > MAX_SOURCE_BYTES:
            limit("yaml_source", "source_bytes", MAX_SOURCE_BYTES)
        text = self._decode(raw)
        if type(text) is not str:
            raise TypeError("native YAML response must be text")
        if (
            len(text) > MAX_RESPONSE_BYTES
            or len(text.encode("utf-8")) > MAX_RESPONSE_BYTES
        ):
            limit("transport", "response_bytes", MAX_RESPONSE_BYTES)
        envelope = json.loads(text)
        if envelope["protocol"] != "yaml/1":
            raise ValueError("unsupported native YAML response protocol")
        outcome = check_outcome(envelope["outcome"])
        if outcome["status"] == "invalid":
            diagnostics = []
            for finding in outcome["diagnostics"]:
                context = dict(finding["context"])
                if finding["condition"] == "non_ascii_source":
                    context["path"] = str(path)
                diagnostics.append(
                    ValidationDiagnostic(
                        condition=finding["condition"],
                        spec_paths=tuple(finding["spec_paths"]),
                        context=context,
                    )
                )
            if not diagnostics:
                raise ValueError("native YAML failure lacks diagnostics")
            error = SpecificationError(diagnostics)
            error.native_outcome = outcome
            raise error
        if outcome["status"] != "decoded":
            raise ValueError("invalid native YAML response status")
        return outcome["document"]

    def values(self, tree: dict) -> list:
        """Honor the host's decimal conversion policy without changing it globally."""
        maximum = sys.get_int_max_str_digits()
        if maximum:
            for node in tree["nodes"]:
                if node["kind"] == "integer":
                    digits = len(node["value"].removeprefix("-"))
                    if digits > maximum:
                        limit("yaml_source", "host_integer_digits", maximum)
        return decode_nodes(tree)

    def read_document(self, path: Path) -> object:
        """Read a bounded source snapshot and publish only a fully admitted value."""
        with path.open("rb") as stream:
            raw = stream.read(MAX_SOURCE_BYTES + 1)
        tree = self.decode_tree(raw, path)
        return self.values(tree)[tree["root"]]
