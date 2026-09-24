"""Resolve complete named windows without changing their caller's scope."""

from __future__ import annotations

import copy
from collections.abc import MutableMapping
from typing import Any

from yamaa.specification.diagnostics import SpecificationError, ValidationDiagnostic
from yamaa.specification.schema import (
    SchemaBundle,
    class_fields,
    matching_type,
    split_type_arguments,
    validate_descriptor_value,
)


def expand_named_windows(
    document: dict[str, object],
    bundle: SchemaBundle,
    *,
    strict: bool = True,
    provenance: MutableMapping[str, Any] | None = None,
) -> dict[str, object]:
    """Copy named settings into schema-declared window positions.

    The non-strict pass exposes dependencies before inheritance pruning while
    leaving unknown names for the strict pass over surviving declarations.
    Definitions are removed only by the strict pass. The schema walk avoids
    interpreting application data or metadata as references.
    """
    result = copy.deepcopy(document)
    definitions = result.get("windows", {})
    if "windows" in result:
        diagnostics = validate_descriptor_value(
            definitions,
            class_fields(bundle, "root_class")["windows"],
            bundle,
            "windows",
        )
        if diagnostics:
            raise SpecificationError(diagnostics)
    diagnostics: list[ValidationDiagnostic] = []

    def fields(value, descriptors, path, logical, *, handled=False):
        if not isinstance(value, dict):
            return value
        for name, descriptor in descriptors.items():
            if name in value:
                child = path if handled and name == "value" else join(path, name)
                value[name] = walk(
                    value[name], descriptor["type"], child, join(logical, name)
                )
        return value

    def join(path, name):
        return f"{path}.{name}" if path else str(name)

    def walk(value, type_value, path, logical, active=frozenset()):
        # Single declared types need no speculative whole-subtree validation.
        kind = (
            type_value
            if isinstance(type_value, str)
            else matching_type(
                value,
                [member for member in type_value if (id(value), member) not in active],
                bundle,
                fragment=True,
            )
        )
        if kind == "window_selection" and isinstance(value, str):
            if value not in definitions:
                if strict:
                    diagnostics.append(
                        ValidationDiagnostic(
                            condition="unknown_window",
                            spec_paths=(path,),
                            requirement="REQ-1253",
                            context={"window": value},
                        )
                    )
                return value
            if provenance is not None:
                prefix = f"windows.{value}"
                for source_path, origin in list(provenance.items()):
                    if source_path.startswith(f"{prefix}."):
                        provenance[logical + source_path[len(prefix) :]] = origin
            return copy.deepcopy(definitions[value])
        if not isinstance(kind, str):
            return value
        if kind.startswith("list[") and isinstance(value, list):
            inner = kind[5:-1]
            identity = {
                "column_class": "name",
                "row_class": "id",
                "intermediate_class": "id",
            }.get(inner)
            return [
                walk(
                    item,
                    inner,
                    f"{path}[{index}]",
                    join(logical, item.get(identity, index))
                    if identity and isinstance(item, dict)
                    else f"{logical}[{index}]",
                )
                for index, item in enumerate(value)
            ]
        if kind.startswith("dict[") and isinstance(value, dict):
            _, inner = split_type_arguments(kind[5:-1])
            return {
                name: walk(item, inner, join(path, name), join(logical, name))
                for name, item in value.items()
            }
        if kind in bundle.classes:
            return fields(
                value,
                class_fields(bundle, kind),
                path,
                logical,
                handled=kind == "handled_expression_class",
            )
        if (id(value), kind) in active:
            return value
        alias = bundle.aliases.get(kind)
        if alias is None:
            return value
        registry = alias.get("registry")
        if registry is None:
            return walk(
                value, alias["type"], path, logical, active | {(id(value), kind)}
            )
        if not isinstance(value, dict) or len(value) != 1:
            return value
        operation, payload = next(iter(value.items()))
        declaration = bundle.registries.get(registry, {}).get(operation)
        if isinstance(declaration, list):
            value[operation] = fields(
                payload,
                {
                    name: descriptor
                    for entry in declaration
                    for name, descriptor in entry.items()
                },
                join(path, operation),
                join(logical, operation),
            )
        elif isinstance(declaration, dict):
            value[operation] = walk(
                payload,
                declaration["type"],
                join(path, operation),
                join(logical, operation),
            )
        return value

    walk(result, "root_class", "", "")
    if diagnostics:
        raise SpecificationError(diagnostics)
    if strict:
        result.pop("windows", None)
    return result
