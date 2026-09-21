"""Interpret the repository's language-neutral schema bundle."""

from __future__ import annotations

import copy
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Literal

from pydantic import JsonValue

from yamaa.regex import RegexError, compile_pattern, full_match
from yamaa.specification._yaml import read_yaml_document
from yamaa.specification.diagnostics import (
    SpecificationError,
    ValidationDiagnostic,
)

DefinitionKind = Literal["class", "alias", "registry"]
ActiveTypes = frozenset[tuple[int, str]]


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
    requirement: str | None = None,
) -> ValidationDiagnostic:
    return ValidationDiagnostic(
        condition=condition,
        spec_paths=(path or "$",),
        requirement=requirement,
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


def _is_alias_declaration(definition: dict[object, object]) -> bool:
    type_value = definition.get("type")
    return (
        isinstance(type_value, str)
        or (
            isinstance(type_value, list)
            and all(isinstance(member, str) for member in type_value)
        )
        or isinstance(definition.get("registry"), str)
    )


def load_schema_bundle(
    schema_root: str | Path,
    *,
    entry_name: str = "schema.yaml",
    root_class: str = "root_class",
) -> SchemaBundle:
    """Load one closed schema bundle rooted at an entry document.

    The repository publishes two entry points over the same shared
    declarations: `schema.yaml` for a specification and, under REQ-0664,
    `schema_environment.yaml` for a project environment validated
    independently of any specification.
    """
    root = Path(schema_root).resolve()
    entrypoint = root / entry_name
    if not entrypoint.is_file():
        raise _schema_failure(entrypoint, f"{entry_name} is not a regular file")

    classes: dict[str, list[dict[str, dict[str, Any]]]] = {}
    aliases: dict[str, dict[str, Any]] = {}
    registries: dict[str, dict[str, Any]] = {}
    kinds: dict[str, DefinitionKind] = {}
    version: str | None = None
    pending = [(entrypoint, ())]
    completed: set[Path] = set()

    while pending:
        current, ancestors = pending.pop()
        resolved = current.resolve()
        if resolved in ancestors:
            raise _schema_failure(current, f"schema include cycle at {current.name}")
        if resolved in completed:
            continue
        completed.add(resolved)
        descendants = ancestors + (resolved,)
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
            pending.append((candidate, descendants))

        for name, definition in document.items():
            if name in {"version", "includes"}:
                continue
            if not isinstance(name, str) or not name:
                raise _schema_failure(current, "declaration names must be strings")
            if isinstance(definition, list):
                kind: DefinitionKind = "class"
            elif isinstance(definition, dict) and _is_alias_declaration(definition):
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
                    duplicate = min(
                        duplicates,
                        key=lambda value: (type(value).__name__, repr(value)),
                    )
                    raise _schema_failure(
                        current,
                        f"duplicate registry entry {name}.{duplicate}",
                    )
                registry.update(definition)

    assert version is not None
    if root_class not in classes:
        raise _schema_failure(entrypoint, f"{root_class} is not declared")
    bundle = SchemaBundle(
        version=version,
        path=entrypoint,
        classes=classes,
        aliases=aliases,
        registries=registries,
    )
    _validate_schema_bundle(bundle)
    return bundle


def _members(type_value: object) -> list[str]:
    values = type_value if isinstance(type_value, list) else [type_value]
    return [str(value).strip() for value in values]


def _split_arguments(value: str) -> tuple[str, str]:
    depth = 0
    separator: int | None = None
    for index, character in enumerate(value):
        if character == "[":
            depth += 1
        elif character == "]":
            depth -= 1
            if depth < 0:
                break
        elif character == "," and depth == 0:
            if separator is not None:
                break
            separator = index
    if depth == 0 and separator is not None:
        key_type = value[:separator].strip()
        value_type = value[separator + 1 :].strip()
        if key_type and value_type:
            return key_type, value_type
    raise ValueError(f"invalid dictionary type arguments {value!r}")


_BUILTIN_TYPES = frozenset({"str", "int", "float", "bool", "null", "list", "dict"})
_DESCRIPTOR_KEYS = frozenset(
    {"type", "description", "values", "pattern", "min_length", "size", "default"}
)
_TYPE_NAME = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")


def _parse_type_expression(expression: str) -> list[str]:
    expression = expression.strip()
    if _TYPE_NAME.fullmatch(expression):
        return [expression]
    if expression.startswith("list[") and expression.endswith("]"):
        inner = expression[5:-1].strip()
        if inner:
            return _parse_type_expression(inner)
    if expression.startswith("dict[") and expression.endswith("]"):
        key_type, value_type = _split_arguments(expression[5:-1])
        return _parse_type_expression(key_type) + _parse_type_expression(value_type)
    raise ValueError(f"invalid type expression {expression!r}")


def _descriptor_issues(
    descriptor: object,
    *,
    class_field: bool,
    path: str,
) -> tuple[list[str], list[str]]:
    if not isinstance(descriptor, dict):
        return [f"{path}: descriptor must be a mapping"], []

    issues: list[str] = []
    allowed = _DESCRIPTOR_KEYS | ({"required"} if class_field else set())
    for keyword in descriptor:
        if keyword not in allowed:
            issues.append(f"{path}: invalid descriptor keyword {keyword!r}")

    type_value = descriptor.get("type")
    if "type" not in descriptor:
        issues.append(f"{path}: missing 'type'")
        return issues, []
    if isinstance(type_value, str):
        type_members = [type_value.strip()]
    elif (
        isinstance(type_value, list)
        and type_value
        and all(isinstance(item, str) for item in type_value)
    ):
        type_members = [item.strip() for item in type_value]
    else:
        issues.append(f"{path}: type must be a string or non-empty list of strings")
        type_members = []

    references: list[str] = []
    for member in type_members:
        try:
            references.extend(_parse_type_expression(member))
        except ValueError as error:
            issues.append(f"{path}.type: {error}")

    required = descriptor.get("required")
    if "required" in descriptor and type(required) is not bool:
        issues.append(f"{path}: required must be a boolean")
    if required is True and "default" in descriptor:
        issues.append(f"{path}: a required field cannot declare a default")

    description = descriptor.get("description")
    if "description" in descriptor and (
        not isinstance(description, str) or not description.strip()
    ):
        issues.append(f"{path}: description must be a non-empty string")

    string_only = type_members == ["str"]
    sized_only = bool(type_members) and all(
        member in {"list", "dict"} or member.startswith(("list[", "dict["))
        for member in type_members
    )

    pattern = descriptor.get("pattern")
    if "pattern" in descriptor:
        if not string_only:
            issues.append(f"{path}: pattern is allowed only for type str")
        if not isinstance(pattern, str):
            issues.append(f"{path}: pattern must be a string")
        else:
            try:
                compile_pattern(f"^(?:{pattern})$")
            except RegexError as error:
                issues.append(f"{path}: invalid pattern {pattern!r}: {error.reason}")

    minimum = descriptor.get("min_length")
    if "min_length" in descriptor:
        if not string_only:
            issues.append(f"{path}: min_length is allowed only for type str")
        if type(minimum) is not int or minimum < 0:
            issues.append(f"{path}: min_length must be a non-negative integer")

    size = descriptor.get("size")
    if "size" in descriptor:
        if not sized_only:
            issues.append(f"{path}: size is allowed only for list or dict")
        if type(size) is not int or size < 0:
            issues.append(f"{path}: size must be a non-negative integer")

    values = descriptor.get("values")
    if "values" in descriptor:
        if not string_only:
            issues.append(f"{path}: values is allowed only for type str")
        if not isinstance(values, list) or not all(
            isinstance(value, str) for value in values
        ):
            issues.append(f"{path}: values must be a list of strings")

    return issues, references


def _field_issues(
    fields: object,
    *,
    path: str,
) -> tuple[list[str], list[str], list[tuple[object, dict[str, Any], str]]]:
    if not isinstance(fields, list):
        return [f"{path}: class must be a list"], [], []
    issues: list[str] = []
    references: list[str] = []
    defaults: list[tuple[object, dict[str, Any], str]] = []
    names: set[str] = set()
    for index, entry in enumerate(fields):
        entry_path = f"{path}[{index}]"
        if not isinstance(entry, dict) or len(entry) != 1:
            issues.append(f"{entry_path}: class fields must be one-entry mappings")
            continue
        name, descriptor = next(iter(entry.items()))
        if not isinstance(name, str) or not name:
            issues.append(f"{entry_path}: field name must be a non-empty string")
            continue
        field_path = f"{path}.{name}"
        if name in names:
            issues.append(f"{field_path}: duplicate class field")
        names.add(name)
        descriptor_errors, descriptor_references = _descriptor_issues(
            descriptor,
            class_field=True,
            path=field_path,
        )
        issues.extend(descriptor_errors)
        references.extend(descriptor_references)
        if isinstance(descriptor, dict) and "default" in descriptor:
            defaults.append(
                (descriptor["default"], descriptor, f"{field_path}.default")
            )
    return issues, references, defaults


def _validate_schema_bundle(bundle: SchemaBundle) -> None:
    issues: list[str] = []
    references: list[str] = []
    defaults: list[tuple[object, dict[str, Any], str]] = []
    known_types = _BUILTIN_TYPES | bundle.classes.keys() | bundle.aliases.keys()

    for name, fields in bundle.classes.items():
        field_errors, field_references, field_defaults = _field_issues(
            fields,
            path=name,
        )
        issues.extend(field_errors)
        references.extend(field_references)
        defaults.extend(field_defaults)

    referenced_registries: set[str] = set()
    for name, alias in bundle.aliases.items():
        path = name
        if "registry" in alias:
            if set(alias) != {"registry"}:
                issues.append(
                    f"{path}: registry-backed type must contain only 'registry'"
                )
            registry_name = alias.get("registry")
            if not isinstance(registry_name, str) or not registry_name:
                issues.append(f"{path}: registry name must be a non-empty string")
            else:
                referenced_registries.add(registry_name)
            continue
        descriptor_errors, descriptor_references = _descriptor_issues(
            alias,
            class_field=False,
            path=path,
        )
        issues.extend(descriptor_errors)
        references.extend(descriptor_references)
        if "default" in alias:
            defaults.append((alias["default"], alias, f"{path}.default"))

    for registry_name, entries in bundle.registries.items():
        if not entries:
            issues.append(f"{registry_name}: registry is empty")
        if registry_name not in referenced_registries:
            issues.append(f"{registry_name}: registry is unreferenced")
        for entry_name, definition in entries.items():
            entry_path = f"{registry_name}.{entry_name}"
            if not isinstance(entry_name, str) or not entry_name:
                issues.append(f"{entry_path}: entry name must be a non-empty string")
                continue
            if isinstance(definition, list):
                field_errors, field_references, field_defaults = _field_issues(
                    definition,
                    path=entry_path,
                )
                issues.extend(field_errors)
                references.extend(field_references)
                defaults.extend(field_defaults)
            elif isinstance(definition, dict):
                descriptor_errors, descriptor_references = _descriptor_issues(
                    definition,
                    class_field=False,
                    path=entry_path,
                )
                issues.extend(descriptor_errors)
                references.extend(descriptor_references)
                if "default" in definition:
                    defaults.append(
                        (
                            definition["default"],
                            definition,
                            f"{entry_path}.default",
                        )
                    )
            else:
                issues.append(f"{entry_path}: entry must be a class or descriptor")

    for registry_name in referenced_registries:
        if registry_name not in bundle.registries:
            issues.append(f"registry {registry_name!r} is not declared")
    for reference in references:
        if reference not in known_types:
            issues.append(f"unknown schema type {reference!r}")

    if issues:
        raise _schema_failure(bundle.path, "; ".join(issues))

    for value, descriptor, path in defaults:
        diagnostics = _validate_descriptor(value, descriptor, bundle, path)
        if diagnostics:
            conditions = ", ".join(item.condition for item in diagnostics)
            issues.append(f"{path}: invalid default ({conditions})")
    if issues:
        raise _schema_failure(bundle.path, "; ".join(issues))


def _actual_type(value: object) -> str:
    if isinstance(value, dict):
        return "mapping"
    if isinstance(value, list):
        return "sequence"
    if value is None:
        return "null"
    return type(value).__name__


def _invalid_type(
    path: str,
    expected: str,
    value: object,
    requirement: str | None = "REQ-0287",
) -> ValidationDiagnostic:
    return _diagnostic(
        path,
        "invalid_field_type",
        {"expected": expected, "actual": _actual_type(value)},
        requirement,
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


def _outer_matches(
    value: object,
    type_name: str,
    bundle: SchemaBundle,
    active: ActiveTypes = frozenset(),
) -> bool:
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
    validation_key = (id(value), type_name)
    if validation_key in active:
        return False
    if "registry" in alias:
        return isinstance(value, dict)
    nested_active = active | {validation_key}
    return any(
        _outer_matches(value, member, bundle, nested_active)
        for member in _members(alias.get("type"))
    )


def _validate_constraints(
    value: object,
    descriptor: dict[str, Any],
    path: str,
    requirement: str = "REQ-0287",
) -> list[ValidationDiagnostic]:
    diagnostics: list[ValidationDiagnostic] = []
    permitted = descriptor.get("values")
    if permitted is not None and value not in permitted:
        diagnostics.append(
            _diagnostic(
                path,
                "value_not_permitted",
                {"value": value, "permitted": permitted},
                requirement,
            )
        )
    pattern = descriptor.get("pattern")
    if pattern is not None and isinstance(value, str):
        try:
            matched = full_match(pattern, value)
        except RegexError as error:
            diagnostics.append(
                _diagnostic(
                    path,
                    "invalid_regex",
                    {"pattern": pattern, "reason": error.reason},
                    "REQ-0827",
                )
            )
        else:
            if not matched:
                diagnostics.append(
                    _diagnostic(
                        path,
                        "pattern_mismatch",
                        {"value": value, "pattern": pattern},
                        requirement,
                    )
                )
    minimum = descriptor.get("min_length")
    if minimum is not None and (
        not hasattr(value, "__len__") or len(value) < minimum  # type: ignore[arg-type]
    ):
        diagnostics.append(
            _diagnostic(path, "minimum_length", {"minimum": minimum}, requirement)
        )
    size = descriptor.get("size")
    if size is not None and (
        not hasattr(value, "__len__") or len(value) != size  # type: ignore[arg-type]
    ):
        diagnostics.append(
            _diagnostic(path, "invalid_size", {"size": size}, requirement)
        )
    return diagnostics


def _validate_descriptor(
    value: object,
    descriptor: dict[str, Any],
    bundle: SchemaBundle,
    path: str,
    active: ActiveTypes = frozenset(),
    fragment: bool = False,
) -> list[ValidationDiagnostic]:
    diagnostics = _validate_type(
        value, descriptor["type"], bundle, path, active, fragment
    )
    if diagnostics:
        return diagnostics
    return _validate_constraints(value, descriptor, path)


def _validate_inline_class(
    value: object,
    fields: list[dict[str, dict[str, Any]]],
    bundle: SchemaBundle,
    path: str,
    class_name: str,
    active: ActiveTypes = frozenset(),
    fragment: bool = False,
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
        if descriptor.get("required") and not fragment and field_name not in value:
            diagnostics.append(
                _diagnostic(
                    field_path,
                    "missing_required_field",
                    {"field": field_name, "class": class_name},
                )
            )
        elif field_name in value:
            diagnostics.extend(
                _validate_descriptor(
                    value[field_name],
                    descriptor,
                    bundle,
                    field_path,
                    active,
                    fragment,
                )
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
    active: ActiveTypes = frozenset(),
    fragment: bool = False,
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
            diagnostics.extend(
                _validate_type(
                    item, inner, bundle, _join(path, suffix), active, fragment
                )
            )
        return diagnostics

    if type_name.startswith("dict[") and type_name.endswith("]"):
        if not isinstance(value, dict):
            return [_invalid_type(path, "dict", value)]
        key_type, value_type = _split_arguments(type_name[5:-1])
        diagnostics = []
        for key, item in value.items():
            diagnostics.extend(
                _validate_type(
                    key,
                    key_type,
                    bundle,
                    _join(path, f"key({key})"),
                    active,
                    fragment,
                )
            )
            diagnostics.extend(
                _validate_type(
                    item, value_type, bundle, _join(path, key), active, fragment
                )
            )
        return diagnostics

    if type_name in bundle.classes:
        return _validate_inline_class(
            value,
            bundle.classes[type_name],
            bundle,
            path,
            type_name,
            active,
            fragment,
        )

    alias = bundle.aliases.get(type_name)
    if alias is not None:
        validation_key = (id(value), type_name)
        if validation_key in active:
            return [_invalid_type(path, type_name, value)]
        nested_active = active | {validation_key}
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
                    nested_active,
                    fragment,
                )
            return _validate_descriptor(
                payload,
                definition,
                bundle,
                operation_path,
                nested_active,
                fragment,
            )

        diagnostics = _validate_type(
            value,
            alias["type"],
            bundle,
            path,
            nested_active,
            fragment,
        )
        if (
            type_name == "derivation"
            and diagnostics
            and not isinstance(value, (str, dict))
        ):
            # REQ-0320: a bare derivation is either a string source reference
            # or a mapping. Name the dict form so the fix is obvious.
            return [
                _diagnostic(
                    path,
                    "bare_derivation_scalar",
                    {
                        "actual": _actual_type(value),
                        "hint": (
                            "a bare derivation must be a string column "
                            "reference such as ADSL.AGE; write a fixed value "
                            "as {literal: ...}"
                        ),
                    },
                    "REQ-0320",
                )
            ]
        has_matching_outer_type = any(
            _outer_matches(value, member, bundle, nested_active)
            for member in _members(alias["type"])
        )
        if (
            diagnostics
            and not has_matching_outer_type
            and all(item.condition == "invalid_field_type" for item in diagnostics)
        ):
            requirement = "REQ-0322" if type_name == "variable" else "REQ-0287"
            return [_invalid_type(path, type_name, value, requirement)]
        if diagnostics:
            return diagnostics
        requirement = {
            "column_type": "REQ-0012",
            "day_rule": "REQ-0609",
            "time_rule": "REQ-1184",
        }.get(type_name, "REQ-0287")
        return _validate_constraints(value, alias, path, requirement)

    return [_diagnostic(path, "unknown_schema_type", {"type": type_name})]


def _validate_type(
    value: object,
    type_value: object,
    bundle: SchemaBundle,
    path: str,
    active: ActiveTypes = frozenset(),
    fragment: bool = False,
) -> list[ValidationDiagnostic]:
    attempted: list[tuple[str, list[ValidationDiagnostic]]] = []
    for type_name in _members(type_value):
        if type_name in bundle.aliases and (id(value), type_name) in active:
            continue
        diagnostics = _validate_single(value, type_name, bundle, path, active, fragment)
        if not diagnostics:
            return []
        attempted.append((type_name, diagnostics))
    for type_name, diagnostics in attempted:
        if _outer_matches(value, type_name, bundle, active):
            return diagnostics
    if attempted:
        return attempted[0][1]
    expected = " | ".join(_members(type_value))
    return [_invalid_type(path, expected, value)]


def validate_document(
    document: object,
    bundle: SchemaBundle,
    root_class: str,
) -> list[ValidationDiagnostic]:
    """Validate a raw document against one named class of the bundle."""
    if not isinstance(document, dict) or not document:
        return [_invalid_type("$", root_class, document)]
    version = document.get("schema_version")
    if version != bundle.version:
        context: dict[str, JsonValue] = {"expected": bundle.version}
        if version is None or isinstance(version, (str, int, float, bool)):
            context["actual"] = version
        else:
            context["actual_type"] = _actual_type(version)
        return [
            _diagnostic(
                "schema_version",
                "schema_version_mismatch",
                context,
            )
        ]
    return _validate_single(document, root_class, bundle, "")


def validate_specification(
    document: object,
    bundle: SchemaBundle,
) -> list[ValidationDiagnostic]:
    """Validate a raw document against the bundle's root class."""
    return validate_document(document, bundle, "root_class")


def _matches(
    value: object,
    type_name: str,
    bundle: SchemaBundle,
    active: ActiveTypes,
    fragment: bool = False,
) -> bool:
    return not _validate_single(
        value,
        type_name,
        bundle,
        "<normalization>",
        active,
        fragment,
    )


def _normalize_descriptor(
    value: object,
    descriptor: dict[str, Any],
    bundle: SchemaBundle,
    active: ActiveTypes,
    fragment: bool = False,
) -> object:
    return _normalize_type(value, descriptor["type"], bundle, active, fragment)


def _normalize_inline_class(
    value: object,
    fields: list[dict[str, dict[str, Any]]],
    bundle: SchemaBundle,
    active: ActiveTypes,
    fragment: bool = False,
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
                value[field_name], descriptor, bundle, active, fragment
            )
        elif "default" in descriptor and not fragment:
            normalized[field_name] = _normalize_descriptor(
                descriptor["default"],
                descriptor,
                bundle,
                active,
            )
    return normalized


_DERIVATION_CALL = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*)\(\s*([^()]*?)\s*\)$")


def _derivation_shorthand(value: str) -> dict[str, object]:
    """Expand a bare derivation string to its dict form.

    A plain name stays the REQ-0319 source shorthand. `operation(argument)`
    is the one-argument operation call shorthand, so `to_number(IDVARVAL)`
    reads as `{to_number: {source: IDVARVAL}}`.
    """
    match = _DERIVATION_CALL.match(value.strip())
    if match is None:
        return {"source": value}
    return {match.group(1): {"source": match.group(2)}}


def _case_result_shorthand(value: str) -> dict[str, object]:
    """Expand a bare `case` result variable to its source expression."""
    return {"source": value}


def _normalize_single(
    value: object,
    type_name: str,
    bundle: SchemaBundle,
    active: ActiveTypes,
    fragment: bool = False,
) -> object:
    if type_name.startswith("list[") and type_name.endswith("]"):
        inner = type_name[5:-1].strip()
        return [
            _normalize_type(item, inner, bundle, active, fragment)
            for item in value  # type: ignore[union-attr]
        ]
    if type_name.startswith("dict[") and type_name.endswith("]"):
        key_type, value_type = _split_arguments(type_name[5:-1])
        return {
            _normalize_type(key, key_type, bundle, active): _normalize_type(
                item, value_type, bundle, active, fragment
            )
            for key, item in value.items()  # type: ignore[union-attr]
        }
    if type_name in bundle.classes:
        return _normalize_inline_class(
            value,
            bundle.classes[type_name],
            bundle,
            active,
            fragment,
        )
    alias = bundle.aliases.get(type_name)
    if alias is not None:
        normalization_key = (id(value), type_name)
        if normalization_key in active:
            return copy.deepcopy(value)
        nested_active = active | {normalization_key}
        if type_name == "derivation":
            # REQ-0319: a bare derivation string is the source shorthand.
            # Expand it to the dict form, then dispatch on the remaining
            # union members so the registry and the REQ-0266
            # handled-expression expansion apply unchanged. The "str" member
            # exists for validation; normalization never dispatches on it.
            # `operation(argument)` is the one-argument operation call
            # shorthand, so `to_number(IDVARVAL)` reads as
            # `{to_number: {source: IDVARVAL}}`. Parentheses cannot appear in
            # a variable name (REQ-1057), so no valid source shorthand
            # changes meaning.
            if isinstance(value, str):
                value = _derivation_shorthand(value)
            return _normalize_type(
                value,
                [member for member in _members(alias["type"]) if member != "str"],
                bundle,
                nested_active,
                fragment,
            )
        if type_name == "case_result":
            # REQ-0319: then/otherwise use the same bare-variable source
            # shorthand as a derivation, but do not admit derivation call
            # shorthand or top-level handlers.
            if isinstance(value, str):
                value = _case_result_shorthand(value)
            return _normalize_type(
                value,
                [member for member in _members(alias["type"]) if member != "str"],
                bundle,
                nested_active,
                fragment,
            )
        registry_name = alias.get("registry")
        if registry_name is not None:
            operation, payload = next(iter(value.items()))  # type: ignore[union-attr]
            definition = bundle.registries[registry_name][operation]
            if isinstance(definition, list):
                payload = _normalize_inline_class(
                    payload,
                    definition,
                    bundle,
                    nested_active,
                    fragment,
                )
            else:
                payload = _normalize_descriptor(
                    payload,
                    definition,
                    bundle,
                    nested_active,
                    fragment,
                )
            return {operation: payload}
        return _normalize_type(value, alias["type"], bundle, nested_active, fragment)
    return copy.deepcopy(value)


def _normalize_type(
    value: object,
    type_value: object,
    bundle: SchemaBundle,
    active: ActiveTypes,
    fragment: bool = False,
) -> object:
    members = _members(type_value)
    for member in members:
        if not (member.startswith("list[") and member.endswith("]")):
            continue
        inner = member[5:-1].strip()
        if len(members) != 2 or inner not in members:
            continue
        if _matches(value, member, bundle, active, fragment):
            return _normalize_single(value, member, bundle, active, fragment)
        if _matches(value, inner, bundle, active, fragment):
            return [_normalize_type(value, inner, bundle, active, fragment)]

    for class_name in members:
        if len(members) != 2:
            continue
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
            if (
                member == class_name
                or member in bundle.classes
                or field_types != [member]
            ):
                continue
            if _matches(value, member, bundle, active, fragment):
                expanded = {
                    field_name: _normalize_descriptor(
                        value,
                        descriptor,
                        bundle,
                        active,
                        fragment,
                    )
                }
                for name, other in fields.items():
                    if name not in expanded and "default" in other and not fragment:
                        expanded[name] = _normalize_descriptor(
                            other["default"],
                            other,
                            bundle,
                            active,
                        )
                return expanded

    for member in members:
        if _matches(value, member, bundle, active, fragment):
            return _normalize_single(value, member, bundle, active, fragment)
    return copy.deepcopy(value)


def normalize_document(
    document: dict[object, object],
    bundle: SchemaBundle,
    root_class: str,
) -> object:
    """Materialize defaults and R006 shorthands for one named class."""
    return _normalize_single(document, root_class, bundle, frozenset())


def normalize_specification(
    document: dict[object, object], bundle: SchemaBundle
) -> object:
    """Materialize defaults and R006 collection/class shorthands."""
    return normalize_document(document, bundle, "root_class")


# R017 validates schema-shaped fragments instead of complete root objects.  Keep
# that component behind this deliberately small adapter rather than teaching it
# the schema interpreter's private representation.
def class_fields(bundle: SchemaBundle, class_name: str) -> dict[str, dict[str, Any]]:
    """Return one class's fields in schema order."""
    return _class_fields(bundle, class_name)


def validate_descriptor_value(
    value: object,
    descriptor: dict[str, Any],
    bundle: SchemaBundle,
    path: str,
    *,
    fragment: bool = False,
) -> list[ValidationDiagnostic]:
    """Validate one field value against its descriptor.

    ``fragment`` defers requiredness at every depth, which is what R017 needs
    to read a layer's ``columns`` member field as a patch of the value it
    composes onto.  Every other check still applies to what the layer wrote.
    """
    return _validate_descriptor(value, descriptor, bundle, path, frozenset(), fragment)


def normalize_descriptor_value(
    value: object,
    descriptor: dict[str, Any],
    bundle: SchemaBundle,
    *,
    fragment: bool = False,
) -> object:
    """Normalize one already validated field value.

    ``fragment`` expands R006 shorthand without materializing a default, so a
    patch carries only what its layer wrote and cannot silently replace an
    inherited value with this schema's default.
    """
    return _normalize_descriptor(value, descriptor, bundle, frozenset(), fragment)


def matching_type(
    value: object,
    type_value: object,
    bundle: SchemaBundle,
    *,
    fragment: bool = False,
) -> str | None:
    """Return the first schema union member a value satisfies."""
    return next(
        (
            member
            for member in _members(type_value)
            if _matches(value, member, bundle, frozenset(), fragment)
        ),
        None,
    )


def split_type_arguments(value: str) -> tuple[str, str]:
    """Split the two members of a validated dictionary type expression."""
    return _split_arguments(value)
