"""Interpret the repository's language-neutral schema bundle."""

from __future__ import annotations

import copy
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Literal

import regress
from pydantic import JsonValue

from yamaa.specification._yaml import read_yaml_document
from yamaa.specification.diagnostics import (
    SpecificationError,
    ValidationDiagnostic,
)

DefinitionKind = Literal["class", "alias", "registry"]


@dataclass(frozen=True)
class SchemaBundle:
    version: str
    path: Path
    classes: dict[str, list[dict[str, dict[str, Any]]]]
    aliases: dict[str, dict[str, Any]]
    registries: dict[str, dict[str, Any]]


def _diagnostic(
    path: str,
    condition: str,
    context: dict[str, JsonValue],
) -> ValidationDiagnostic:
    return ValidationDiagnostic(
        condition=condition,
        spec_paths=(path or "$",),
        context=context,
    )


def _join(path: str, member: object) -> str:
    if isinstance(member, int):
        return f"{path}[{member}]" if path else f"[{member}]"
    return f"{path}.{member}" if path else str(member)


def _schema_failure(path: Path, reason: str) -> SpecificationError:
    return SpecificationError(
        [
            _diagnostic(
                "$",
                "invalid_schema_bundle",
                {"path": str(path), "reason": reason},
            )
        ]
    )


def load_schema_bundle(schema_root: str | Path) -> SchemaBundle:
    """Load one closed schema bundle rooted at ``schema.yaml``."""
    root = Path(schema_root).resolve()
    entrypoint = root / "schema.yaml"
    if not entrypoint.is_file():
        raise _schema_failure(entrypoint, "schema.yaml is not a regular file")

    classes: dict[str, list[dict[str, dict[str, Any]]]] = {}
    aliases: dict[str, dict[str, Any]] = {}
    registries: dict[str, dict[str, Any]] = {}
    kinds: dict[str, DefinitionKind] = {}
    version: str | None = None
    pending = [entrypoint]
    completed: set[Path] = set()

    while pending:
        current = pending.pop()
        resolved = current.resolve()
        if resolved in completed:
            continue
        completed.add(resolved)
        document = read_yaml_document(current)
        if not isinstance(document, dict):
            raise _schema_failure(current, "schema document must be a mapping")

        document_version = document.get("version")
        if not isinstance(document_version, str) or not document_version:
            raise _schema_failure(current, "version must be a non-empty string")
        if version is None:
            version = document_version
        elif document_version != version:
            raise _schema_failure(
                current,
                f"version {document_version!r} does not match {version!r}",
            )

        includes = document.get("includes", [])
        if not isinstance(includes, list):
            raise _schema_failure(current, "includes must be a list")
        for include in includes:
            if not isinstance(include, str) or not re.fullmatch(
                r"schema_[a-z0-9_]+\.yaml", include
            ):
                raise _schema_failure(current, f"unsafe include {include!r}")
            candidate = current.parent / include
            if candidate.is_symlink() or not candidate.is_file():
                raise _schema_failure(current, f"invalid include {include!r}")
            try:
                candidate.resolve().relative_to(root)
            except ValueError as error:
                raise _schema_failure(current, f"unsafe include {include!r}") from error
            pending.append(candidate)

        for name, definition in document.items():
            if name in {"version", "includes"}:
                continue
            if isinstance(definition, list):
                kind: DefinitionKind = "class"
            elif isinstance(definition, dict) and (
                "type" in definition or "registry" in definition
            ):
                kind = "alias"
            elif isinstance(definition, dict):
                kind = "registry"
            else:
                raise _schema_failure(current, f"unknown declaration {name!r}")

            previous = kinds.get(name)
            if previous is not None and not (previous == kind == "registry"):
                raise _schema_failure(current, f"duplicate declaration {name!r}")
            kinds.setdefault(name, kind)
            if kind == "class":
                classes[name] = definition
            elif kind == "alias":
                aliases[name] = definition
            else:
                registry = registries.setdefault(name, {})
                duplicates = registry.keys() & definition.keys()
                if duplicates:
                    duplicate = min(duplicates)
                    raise _schema_failure(
                        current,
                        f"duplicate registry entry {name}.{duplicate}",
                    )
                registry.update(definition)

    assert version is not None
    if "root_class" not in classes:
        raise _schema_failure(entrypoint, "root_class is not declared")
    return SchemaBundle(
        version=version,
        path=entrypoint,
        classes=classes,
        aliases=aliases,
        registries=registries,
    )


def _members(type_value: object) -> list[str]:
    values = type_value if isinstance(type_value, list) else [type_value]
    return [str(value).strip() for value in values]


def _split_arguments(value: str) -> tuple[str, str]:
    depth = 0
    for index, character in enumerate(value):
        if character == "[":
            depth += 1
        elif character == "]":
            depth -= 1
        elif character == "," and depth == 0:
            return value[:index].strip(), value[index + 1 :].strip()
    raise ValueError(f"invalid dictionary type arguments {value!r}")


def _actual_type(value: object) -> str:
    if isinstance(value, dict):
        return "mapping"
    if isinstance(value, list):
        return "sequence"
    if value is None:
        return "null"
    return type(value).__name__


def _invalid_type(path: str, expected: str, value: object) -> ValidationDiagnostic:
    return _diagnostic(
        path,
        "invalid_field_type",
        {"expected": expected, "actual": _actual_type(value)},
    )


def _class_fields(
    bundle: SchemaBundle,
    name: str,
) -> dict[str, dict[str, Any]]:
    return {
        field_name: descriptor
        for entry in bundle.classes.get(name, [])
        for field_name, descriptor in entry.items()
    }


def _outer_matches(value: object, type_name: str, bundle: SchemaBundle) -> bool:
    if type_name == "str":
        return isinstance(value, str)
    if type_name == "int":
        return type(value) is int
    if type_name == "float":
        return type(value) in {int, float}
    if type_name == "bool":
        return type(value) is bool
    if type_name == "null":
        return value is None
    if type_name == "list" or type_name.startswith("list["):
        return isinstance(value, list)
    if type_name == "dict" or type_name.startswith("dict["):
        return isinstance(value, dict)
    if type_name in bundle.classes:
        return isinstance(value, dict)
    alias = bundle.aliases.get(type_name)
    if alias is None:
        return False
    if "registry" in alias:
        return isinstance(value, dict)
    return any(
        _outer_matches(value, member, bundle) for member in _members(alias.get("type"))
    )


def _validate_constraints(
    value: object,
    descriptor: dict[str, Any],
    path: str,
) -> list[ValidationDiagnostic]:
    diagnostics: list[ValidationDiagnostic] = []
    permitted = descriptor.get("values")
    if permitted is not None and value not in permitted:
        diagnostics.append(
            _diagnostic(
                path,
                "value_not_permitted",
                {"value": value, "permitted": permitted},
            )
        )
    pattern = descriptor.get("pattern")
    if pattern is not None and isinstance(value, str):
        try:
            matched = regress.Regex(f"^(?:{pattern})$", "u").find(value)
        except regress.RegressError as error:
            diagnostics.append(
                _diagnostic(
                    path,
                    "invalid_regex",
                    {"pattern": pattern, "reason": str(error)},
                )
            )
        else:
            if matched is None:
                diagnostics.append(
                    _diagnostic(
                        path,
                        "pattern_mismatch",
                        {"value": value, "pattern": pattern},
                    )
                )
    minimum = descriptor.get("min_length")
    if minimum is not None and (
        not hasattr(value, "__len__") or len(value) < minimum  # type: ignore[arg-type]
    ):
        diagnostics.append(_diagnostic(path, "minimum_length", {"minimum": minimum}))
    size = descriptor.get("size")
    if size is not None and (
        not hasattr(value, "__len__") or len(value) != size  # type: ignore[arg-type]
    ):
        diagnostics.append(_diagnostic(path, "invalid_size", {"size": size}))
    return diagnostics


def _validate_descriptor(
    value: object,
    descriptor: dict[str, Any],
    bundle: SchemaBundle,
    path: str,
) -> list[ValidationDiagnostic]:
    diagnostics = _validate_type(value, descriptor["type"], bundle, path)
    if diagnostics:
        return diagnostics
    return _validate_constraints(value, descriptor, path)


def _validate_inline_class(
    value: object,
    fields: list[dict[str, dict[str, Any]]],
    bundle: SchemaBundle,
    path: str,
    class_name: str,
) -> list[ValidationDiagnostic]:
    if not isinstance(value, dict):
        return [_invalid_type(path, class_name, value)]
    descriptors = {
        field_name: descriptor
        for entry in fields
        for field_name, descriptor in entry.items()
    }
    diagnostics: list[ValidationDiagnostic] = []
    for field_name, descriptor in descriptors.items():
        field_path = _join(path, field_name)
        if descriptor.get("required") and field_name not in value:
            diagnostics.append(
                _diagnostic(
                    field_path,
                    "missing_required_field",
                    {"field": field_name, "class": class_name},
                )
            )
        elif field_name in value:
            diagnostics.extend(
                _validate_descriptor(value[field_name], descriptor, bundle, field_path)
            )
    for field_name in value:
        if field_name not in descriptors:
            diagnostics.append(
                _diagnostic(
                    _join(path, field_name),
                    "unknown_field",
                    {"field": str(field_name), "class": class_name},
                )
            )
    return diagnostics


def _validate_single(
    value: object,
    type_name: str,
    bundle: SchemaBundle,
    path: str,
) -> list[ValidationDiagnostic]:
    if type_name == "str":
        return [] if isinstance(value, str) else [_invalid_type(path, "str", value)]
    if type_name == "int":
        return [] if type(value) is int else [_invalid_type(path, "int", value)]
    if type_name == "float":
        return (
            [] if type(value) in {int, float} else [_invalid_type(path, "float", value)]
        )
    if type_name == "bool":
        return [] if type(value) is bool else [_invalid_type(path, "bool", value)]
    if type_name == "null":
        return [] if value is None else [_invalid_type(path, "null", value)]
    if type_name == "list":
        return [] if isinstance(value, list) else [_invalid_type(path, "list", value)]
    if type_name == "dict":
        return [] if isinstance(value, dict) else [_invalid_type(path, "dict", value)]

    if type_name.startswith("list[") and type_name.endswith("]"):
        if not isinstance(value, list):
            return [_invalid_type(path, "list", value)]
        inner = type_name[5:-1].strip()
        diagnostics: list[ValidationDiagnostic] = []
        for index, item in enumerate(value):
            suffix: object = index
            if isinstance(item, dict):
                suffix = item.get("name", item.get("id", index))
            diagnostics.extend(_validate_type(item, inner, bundle, _join(path, suffix)))
        return diagnostics

    if type_name.startswith("dict[") and type_name.endswith("]"):
        if not isinstance(value, dict):
            return [_invalid_type(path, "dict", value)]
        key_type, value_type = _split_arguments(type_name[5:-1])
        diagnostics = []
        for key, item in value.items():
            diagnostics.extend(
                _validate_type(key, key_type, bundle, _join(path, f"key({key})"))
            )
            diagnostics.extend(
                _validate_type(item, value_type, bundle, _join(path, key))
            )
        return diagnostics

    if type_name in bundle.classes:
        return _validate_inline_class(
            value,
            bundle.classes[type_name],
            bundle,
            path,
            type_name,
        )

    alias = bundle.aliases.get(type_name)
    if alias is not None:
        registry_name = alias.get("registry")
        if registry_name is not None:
            if not isinstance(value, dict):
                return [_invalid_type(path, type_name, value)]
            if len(value) != 1:
                return [
                    _diagnostic(
                        path,
                        "invalid_operation_count",
                        {"registry": registry_name, "count": len(value)},
                    )
                ]
            operation, payload = next(iter(value.items()))
            definition = bundle.registries.get(registry_name, {}).get(operation)
            if definition is None:
                return [
                    _diagnostic(
                        _join(path, operation),
                        "unknown_operation",
                        {"registry": registry_name, "operation": str(operation)},
                    )
                ]
            operation_path = _join(path, operation)
            if isinstance(definition, list):
                return _validate_inline_class(
                    payload,
                    definition,
                    bundle,
                    operation_path,
                    str(operation),
                )
            return _validate_descriptor(payload, definition, bundle, operation_path)

        diagnostics = _validate_type(value, alias["type"], bundle, path)
        has_matching_outer_type = any(
            _outer_matches(value, member, bundle) for member in _members(alias["type"])
        )
        if (
            diagnostics
            and not has_matching_outer_type
            and all(item.condition == "invalid_field_type" for item in diagnostics)
        ):
            return [_invalid_type(path, type_name, value)]
        if diagnostics:
            return diagnostics
        return _validate_constraints(value, alias, path)

    return [_diagnostic(path, "unknown_schema_type", {"type": type_name})]


def _validate_type(
    value: object,
    type_value: object,
    bundle: SchemaBundle,
    path: str,
) -> list[ValidationDiagnostic]:
    attempted: list[tuple[str, list[ValidationDiagnostic]]] = []
    for type_name in _members(type_value):
        diagnostics = _validate_single(value, type_name, bundle, path)
        if not diagnostics:
            return []
        attempted.append((type_name, diagnostics))
    for type_name, diagnostics in attempted:
        if _outer_matches(value, type_name, bundle):
            return diagnostics
    return attempted[0][1] if attempted else []


def validate_specification(
    document: object,
    bundle: SchemaBundle,
) -> list[ValidationDiagnostic]:
    """Validate a raw document against the bundle's root class."""
    if not isinstance(document, dict) or not document:
        return [_invalid_type("$", "root_class", document)]
    version = document.get("schema_version")
    if version != bundle.version:
        return [
            _diagnostic(
                "schema_version",
                "schema_version_mismatch",
                {"expected": bundle.version, "actual": version},
            )
        ]
    return _validate_single(document, "root_class", bundle, "")


def _matches(value: object, type_name: str, bundle: SchemaBundle) -> bool:
    return not _validate_single(value, type_name, bundle, "<normalization>")


def _normalize_descriptor(
    value: object,
    descriptor: dict[str, Any],
    bundle: SchemaBundle,
) -> object:
    return _normalize_type(value, descriptor["type"], bundle)


def _normalize_inline_class(
    value: object,
    fields: list[dict[str, dict[str, Any]]],
    bundle: SchemaBundle,
) -> object:
    if not isinstance(value, dict):
        return copy.deepcopy(value)
    descriptors = {
        field_name: descriptor
        for entry in fields
        for field_name, descriptor in entry.items()
    }
    normalized: dict[object, object] = {}
    for field_name, descriptor in descriptors.items():
        if field_name in value:
            normalized[field_name] = _normalize_descriptor(
                value[field_name], descriptor, bundle
            )
        elif "default" in descriptor:
            normalized[field_name] = copy.deepcopy(descriptor["default"])
    return normalized


def _normalize_single(
    value: object,
    type_name: str,
    bundle: SchemaBundle,
) -> object:
    if type_name.startswith("list[") and type_name.endswith("]"):
        inner = type_name[5:-1].strip()
        return [_normalize_type(item, inner, bundle) for item in value]  # type: ignore[union-attr]
    if type_name.startswith("dict[") and type_name.endswith("]"):
        key_type, value_type = _split_arguments(type_name[5:-1])
        return {
            _normalize_type(key, key_type, bundle): _normalize_type(
                item, value_type, bundle
            )
            for key, item in value.items()  # type: ignore[union-attr]
        }
    if type_name in bundle.classes:
        return _normalize_inline_class(value, bundle.classes[type_name], bundle)
    alias = bundle.aliases.get(type_name)
    if alias is not None:
        registry_name = alias.get("registry")
        if registry_name is not None:
            operation, payload = next(iter(value.items()))  # type: ignore[union-attr]
            definition = bundle.registries[registry_name][operation]
            if isinstance(definition, list):
                payload = _normalize_inline_class(payload, definition, bundle)
            else:
                payload = _normalize_descriptor(payload, definition, bundle)
            return {operation: payload}
        return _normalize_type(value, alias["type"], bundle)
    return copy.deepcopy(value)


def _normalize_type(
    value: object,
    type_value: object,
    bundle: SchemaBundle,
) -> object:
    members = _members(type_value)
    for member in members:
        if not (member.startswith("list[") and member.endswith("]")):
            continue
        inner = member[5:-1].strip()
        if (
            inner in members
            and not isinstance(value, list)
            and _matches(value, inner, bundle)
        ):
            return [_normalize_type(value, inner, bundle)]

    for class_name in members:
        fields = _class_fields(bundle, class_name)
        required = [
            (name, descriptor)
            for name, descriptor in fields.items()
            if descriptor.get("required")
        ]
        if len(required) != 1:
            continue
        field_name, descriptor = required[0]
        field_types = _members(descriptor["type"])
        for member in members:
            if member == class_name or member not in field_types:
                continue
            if _matches(value, member, bundle):
                expanded = {
                    field_name: _normalize_descriptor(value, descriptor, bundle)
                }
                for name, other in fields.items():
                    if name not in expanded and "default" in other:
                        expanded[name] = copy.deepcopy(other["default"])
                return expanded

    for member in members:
        if _matches(value, member, bundle):
            return _normalize_single(value, member, bundle)
    return copy.deepcopy(value)


def normalize_specification(
    document: dict[object, object], bundle: SchemaBundle
) -> object:
    """Materialize defaults and R006 collection/class shorthands."""
    return _normalize_single(document, "root_class", bundle)
