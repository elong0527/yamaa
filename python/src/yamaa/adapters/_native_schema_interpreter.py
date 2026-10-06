"""Bind an owned Rust schema snapshot to the application schema port."""

from __future__ import annotations

import copy
import json

from yamaa.adapters._native_schema_wire import (
    NativeSchemaUnsupportedError,
    check_outcome,
    decode_nodes,
    encode_tree,
    request,
    response,
)
from yamaa.specification.diagnostics import SpecificationError, ValidationDiagnostic


def descriptor_key(descriptor):
    """Compare retained canonical transport text directly; no digest identity."""
    return json.dumps(encode_tree(descriptor), sort_keys=True, separators=(",", ":"))


class NativeSchemaInterpreter:
    """Keep interpretation and descriptor identities private to one captured service."""

    def __init__(self, snapshot, metadata, modules):
        self._analyze = snapshot.analyze
        self._metadata = copy.deepcopy(metadata)
        self._descriptors = [
            copy.deepcopy(modules[d["module"]][d["source_node"]])
            for d in metadata["descriptors"]
        ]
        self._descriptor_ids = {
            descriptor_key(d): index for index, d in enumerate(self._descriptors)
        }

    def _query(self, value, operation, **arguments):
        tree = encode_tree(value)
        result = response(
            self._analyze(
                request(
                    {
                        "queries": [
                            {"operation": operation, "document": tree, **arguments}
                        ]
                    }
                )
            )
        )
        if result["status"] != "analyzed" or len(result["results"]) != 1:
            raise ValueError("invalid native schema query response")
        return check_outcome(result["results"][0]), tree

    def _diagnostics(self, findings, tree):
        values = decode_nodes(tree)
        result = []
        constraints = {
            "descriptor_values": "values",
            "descriptor_pattern": "pattern",
            "descriptor_minimum": "min_length",
            "descriptor_size": "size",
        }
        for finding in findings:
            context = {}
            for item in finding["context"]:
                ref = item["value"]
                kind = ref["kind"]
                if kind in ("text", "count"):
                    value = ref["value"]
                elif kind == "null":
                    value = None
                elif kind == "input_value":
                    value = values[ref["node"]]
                elif kind in constraints:
                    value = self._descriptors[ref["descriptor"]][constraints[kind]]
                else:
                    raise ValueError("unknown native schema context kind")
                context[item["name"]] = copy.deepcopy(value)
            result.append(
                ValidationDiagnostic(
                    condition=finding["condition"],
                    spec_paths=(finding["path"] or "$",),
                    requirement=finding["requirement"],
                    context=context,
                )
            )
        return result

    def _validate(self, value, operation, **arguments):
        result, tree = self._query(value, operation, **arguments)
        if result["status"] not in ("valid", "invalid"):
            raise ValueError("invalid native schema validation response")
        return self._diagnostics(result["diagnostics"], tree)

    def _normalize(self, value, operation, **arguments):
        result, tree = self._query(value, operation, **arguments)
        if result["status"] == "invalid":
            raise SpecificationError(self._diagnostics(result["diagnostics"], tree))
        if result["status"] != "normalized":
            raise ValueError("invalid native schema normalization response")
        output = result["document"]
        return decode_nodes(output)[output["root"]]

    def _root(self, root_class):
        if root_class != self._metadata["root_class"]:
            raise NativeSchemaUnsupportedError({"feature": "different_root_class"})

    def validate_document(self, document, root_class):
        """Use shared version-priority validation for the captured root class."""
        self._root(root_class)
        return self._validate(document, "validate_document")

    def normalize_document(self, document, root_class):
        """Validate and materialize shared defaults/shorthands."""
        self._root(root_class)
        return self._normalize(document, "normalize_document")

    def expand_windows(self, document, strict):
        """Expand in Rust and retain ordered logical links for host provenance."""
        result, tree = self._query(document, "expand_windows", strict=strict)
        if result["status"] == "invalid":
            raise SpecificationError(self._diagnostics(result["diagnostics"], tree))
        if result["status"] != "expanded":
            raise ValueError("invalid native window expansion response")
        output = result["document"]
        value = decode_nodes(output)[output["root"]]
        if not isinstance(value, dict):
            raise TypeError("native window expansion must return a mapping")
        references = tuple(
            (reference["path"], reference["definition"])
            for reference in result["references"]
        )
        if not all(
            type(path) is str and type(name) is str for path, name in references
        ):
            raise ValueError("invalid native window provenance links")
        return value, references

    def _descriptor(self, descriptor):
        index = self._descriptor_ids.get(descriptor_key(descriptor))
        if index is not None:
            return "descriptor", {"descriptor": index}
        if set(descriptor) == {"type"}:
            return "types", {"types": self._types(descriptor["type"])}
        raise NativeSchemaUnsupportedError({"feature": "uncaptured_descriptor"})

    @staticmethod
    def _types(value):
        if type(value) is str:
            return [value]
        if type(value) is list and all(type(member) is str for member in value):
            return value
        raise TypeError("native schema types must be a string or list of strings")

    def validate_descriptor(self, value, descriptor, path, fragment):
        """Retain captured constraints, or bind an explicitly selected type union."""
        kind, arguments = self._descriptor(descriptor)
        return self._validate(
            value, f"validate_{kind}", path=path, fragment=fragment, **arguments
        )

    def normalize_descriptor(self, value, descriptor, fragment):
        """Keep fragment default suppression inside the shared interpreter."""
        kind, arguments = self._descriptor(descriptor)
        return self._normalize(
            value, f"normalize_{kind}", fragment=fragment, **arguments
        )

    def matching_type(self, value, type_value, fragment):
        """Select the ordered union in Rust, including a cycle-filtered empty union."""
        result, _ = self._query(
            value, "matching_types", types=self._types(type_value), fragment=fragment
        )
        if result["status"] != "matched":
            raise ValueError("invalid native schema matching response")
        return result["member"]

    def metadata(self):
        """Return independent public traversal metadata; the service retains ownership."""

        def fields(value):
            return [
                {field["name"]: copy.deepcopy(self._descriptors[field["descriptor"]])}
                for field in value
            ]

        classes = {c["name"]: fields(c["fields"]) for c in self._metadata["classes"]}
        aliases = {}
        for alias in self._metadata["aliases"]:
            if "descriptor" in alias:
                value = copy.deepcopy(self._descriptors[alias["descriptor"]])
            else:
                value = {
                    "registry": self._metadata["registries"][alias["registry"]]["name"]
                }
            aliases[alias["name"]] = value
        registries = {}
        for registry in self._metadata["registries"]:
            entries = {}
            for entry in registry["entries"]:
                shape = entry["shape"]
                entries[entry["name"]] = (
                    fields(shape["fields"])
                    if shape["kind"] == "class"
                    else copy.deepcopy(self._descriptors[shape["descriptor"]])
                )
            registries[registry["name"]] = entries
        return classes, aliases, registries
