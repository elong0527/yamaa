"""Resolve R017 specification inheritance before ordinary validation.

The schema interpreter remains the one authority for field validation and
normalization.  This module only supplies the fragment boundary R017 needs,
then composes those normalized fragments, prunes unreachable declarations,
and establishes the stable dependency order of the resolved columns.
"""

from __future__ import annotations

import copy
import os
import re
from collections.abc import Mapping, Sequence
from pathlib import Path
from typing import Literal

from pydantic import BaseModel, ConfigDict, JsonValue, ValidationError

from yamaa.expressions.aggregate import (
    AggregateError,
    aggregate_identifiers,
    parse_aggregate_cached,
)
from yamaa.expressions.numeric import (
    NumericError,
    numeric_identifiers,
    parse_numeric_cached,
)
from yamaa.expressions.predicates import PredicateError, parse_predicate_cached
from yamaa.expressions.strings import (
    TemplateError,
    parse_template_cached,
    template_identifiers,
)
from yamaa.io.project import ProjectResources
from yamaa.schema.parameterized import expand_parameterized_definitions
from yamaa.specification._yaml import read_yaml_document
from yamaa.specification.diagnostics import SpecificationError, ValidationDiagnostic
from yamaa.specification.models import Specification
from yamaa.specification.schema import (
    SchemaBundle,
    class_fields,
    matching_type,
    normalize_descriptor_value,
    split_type_arguments,
    validate_descriptor_value,
    validate_specification,
)

_URI_SCHEME = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")
_DRIVE_ROOT = re.compile(r"^[A-Za-z]:[\\/]")

_KEYED_COLLECTIONS: dict[str, tuple[Literal["mapping", "list"], str | None, str]] = {
    "input": ("mapping", None, "dataset_class"),
    "intermediates": ("list", "id", "intermediate_class"),
    "columns": ("list", "name", "column_class"),
    "rows": ("list", "id", "row_class"),
}

# REQ-0630 composes a matching column member by each field's declared kind.
# Every other keyed collection still replaces a present member field whole.
_COMPOSING_COLLECTIONS = frozenset({"columns"})


class _FrozenModel(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)


class SourceOrigin(_FrozenModel):
    """The layer and authored logical path that supplied one resolved value."""

    file: Path
    spec_path: str


class LayerPath(_FrozenModel):
    """A relative input path as the layer that wrote it spelled it.

    Rebasing states the same location from the entry file, but REQ-0780
    retries a path that reaches no entry in its layer's own spelling, which
    the rebased form no longer shows.
    """

    directory: Path
    written: str


class ResolvedSpecification(_FrozenModel):
    """One complete R017 result together with its diagnostic provenance."""

    specification: Specification
    document: dict[str, JsonValue]
    entry_path: Path
    layers: tuple[Path, ...]
    provenance: dict[str, SourceOrigin]
    # Keyed by logical path, such as ``input.DM.path`` (REQ-0616).
    layer_paths: dict[str, LayerPath] = {}


def _diagnostic(
    condition: str,
    spec_path: str,
    requirement: str,
    context: dict[str, JsonValue],
) -> ValidationDiagnostic:
    return ValidationDiagnostic(
        condition=condition,
        spec_paths=(spec_path,),
        requirement=requirement,
        context=context,
    )


def _model_path(location: tuple[object, ...]) -> str:
    return ".".join(str(member) for member in location) or "$"


def _join(path: str, member: object) -> str:
    if isinstance(member, int):
        return f"{path}[{member}]" if path else f"[{member}]"
    return f"{path}.{member}" if path else str(member)


def _is_nonlocal_parent(value: object) -> bool:
    if not isinstance(value, str) or not value:
        return True
    if _DRIVE_ROOT.match(value):
        return False
    return bool(_URI_SCHEME.match(value))


def _rooted_project_path(value: str) -> bool:
    return value.startswith("/") or bool(re.match(r"^[A-Za-z]:/", value))


def _rebase_path(value: str, layer: Path, entry: Path) -> str:
    """Respell one layer's written path from the entry's directory.

    The spelling is rearranged, never resolved: a written path is decided
    against the filesystem itself, and a path rewritten through a symlink
    would reach the link's target with the link's own condition -- and its
    error text -- already lost. Joining and normalizing lexically leaves a
    path the entry names exactly as its layer wrote it.
    """
    if _rooted_project_path(value):
        return value
    layer_directory = layer.parent.resolve()
    entry_directory = entry.parent.resolve()
    if layer_directory == entry_directory:
        return value
    target = os.path.normpath(os.path.join(layer_directory, value))
    try:
        return Path(os.path.relpath(target, entry_directory)).as_posix()
    except ValueError:
        return str(target)


def _rebase_layer_paths(document: dict[str, object], layer: Path, entry: Path) -> None:
    datasets = document.get("input")
    if isinstance(datasets, dict):
        for source in datasets.values():
            if not isinstance(source, dict):
                continue
            for name in ("path", "schema"):
                value = source.get(name)
                if isinstance(value, str):
                    source[name] = _rebase_path(value, layer, entry)
    output = document.get("output")
    if isinstance(output, dict):
        # REQ-0629: an inherited output publishes where its layer names.
        for name in ("path", "warning_log", "verification_log"):
            value = output.get(name)
            if isinstance(value, str):
                output[name] = _rebase_path(value, layer, entry)
    rows = document.get("rows")
    if isinstance(rows, list):
        for row in rows:
            if not isinstance(row, dict):
                continue
            catalog = row.get("catalog")
            if isinstance(catalog, dict) and isinstance(catalog.get("path"), str):
                catalog["path"] = _rebase_path(catalog["path"], layer, entry)


def _layer_input_paths(
    document: dict[str, object], layer: Path
) -> dict[tuple[Path, str], str]:
    """Record the relative input paths one layer writes, before rebasing."""
    written: dict[tuple[Path, str], str] = {}
    datasets = document.get("input")
    if isinstance(datasets, dict):
        for dataset, source in datasets.items():
            if not isinstance(source, dict):
                continue
            for name in ("path", "schema"):
                value = source.get(name)
                if isinstance(value, str) and not _rooted_project_path(value):
                    written[(layer, f"input.{dataset}.{name}")] = value
    return written


def _resolved_layer_paths(
    document: dict[str, object],
    provenance: Mapping[str, SourceOrigin],
    written: Mapping[tuple[Path, str], str],
) -> dict[str, LayerPath]:
    """Pair each surviving input path with the layer that supplied it."""
    paths: dict[str, LayerPath] = {}
    datasets = document.get("input")
    if not isinstance(datasets, dict):
        return paths
    for dataset, source in datasets.items():
        if not isinstance(source, dict):
            continue
        for name in ("path", "schema"):
            logical = f"input.{dataset}.{name}"
            origin = provenance.get(logical)
            spelling = written.get((origin.file, logical)) if origin else None
            if origin is not None and spelling is not None:
                paths[logical] = LayerPath(
                    directory=origin.file.parent, written=spelling
                )
    return paths


def _validate_partial_member(
    value: object,
    class_name: str,
    identity: str | None,
    path: str,
    bundle: SchemaBundle,
    fragment: bool = False,
) -> tuple[dict[str, object] | object, list[ValidationDiagnostic]]:
    if not isinstance(value, dict):
        return value, [
            _diagnostic(
                "invalid_field_type",
                path,
                "REQ-0658",
                {"expected": class_name, "actual": type(value).__name__},
            )
        ]

    fields = class_fields(bundle, class_name)
    diagnostics: list[ValidationDiagnostic] = []
    normalized: dict[str, object] = {}
    if identity is not None and identity not in value:
        diagnostics.append(
            _diagnostic(
                "missing_required_field",
                _join(path, identity),
                "REQ-0658",
                {"field": identity, "class": class_name},
            )
        )
    for name in value:
        if name not in fields:
            diagnostics.append(
                _diagnostic(
                    "unknown_field",
                    _join(path, name),
                    "REQ-0658",
                    {"field": str(name), "class": class_name},
                )
            )
    for name, descriptor in fields.items():
        if name not in value:
            continue
        field_path = _join(path, name)
        supplied = value[name]
        if supplied is None:
            if descriptor.get("required") or name == identity:
                diagnostics.append(
                    _diagnostic(
                        "invalid_clear",
                        field_path,
                        "REQ-0660",
                        {"field": name},
                    )
                )
            else:
                normalized[name] = None
            continue
        diagnostics.extend(
            validate_descriptor_value(
                supplied, descriptor, bundle, field_path, fragment=fragment
            )
        )
        normalized[name] = normalize_descriptor_value(
            supplied, descriptor, bundle, fragment=fragment
        )
    return normalized, diagnostics


def _validate_layer(
    document: object,
    bundle: SchemaBundle,
) -> tuple[dict[str, object] | None, list[ValidationDiagnostic]]:
    if not isinstance(document, dict) or not document:
        return None, [
            _diagnostic(
                "invalid_field_type",
                "$",
                "REQ-0658",
                {"expected": "root_class", "actual": type(document).__name__},
            )
        ]

    try:
        document = expand_parameterized_definitions(document)
    except SpecificationError as error:
        return None, list(error.diagnostics)

    fields = class_fields(bundle, "root_class")
    diagnostics: list[ValidationDiagnostic] = []
    normalized: dict[str, object] = {}
    if "schema_version" not in document:
        diagnostics.append(
            _diagnostic(
                "schema_version_mismatch",
                "schema_version",
                "REQ-0656",
                {"expected": bundle.version, "actual": None},
            )
        )
    for name in document:
        if name not in fields:
            diagnostics.append(
                _diagnostic(
                    "unknown_field",
                    str(name),
                    "REQ-0658",
                    {"field": str(name), "class": "root_class"},
                )
            )

    for name, descriptor in fields.items():
        if name not in document:
            continue
        supplied = document[name]
        collection = _KEYED_COLLECTIONS.get(name)
        if name == "parents":
            diagnostics.extend(
                validate_descriptor_value(supplied, descriptor, bundle, name)
            )
            if not diagnostics or all(
                item.spec_paths != (name,) for item in diagnostics
            ):
                normalized[name] = normalize_descriptor_value(
                    supplied, descriptor, bundle
                )
            continue
        if supplied is None:
            if descriptor.get("required"):
                diagnostics.append(
                    _diagnostic("invalid_clear", name, "REQ-0660", {"field": name})
                )
            else:
                normalized[name] = None
            continue
        if collection is None:
            before = len(diagnostics)
            diagnostics.extend(
                validate_descriptor_value(supplied, descriptor, bundle, name)
            )
            if len(diagnostics) == before:
                normalized[name] = normalize_descriptor_value(
                    supplied, descriptor, bundle
                )
            continue

        kind, identity, class_name = collection
        fragment = name in _COMPOSING_COLLECTIONS
        if kind == "mapping":
            if not isinstance(supplied, dict):
                diagnostics.append(
                    _diagnostic(
                        "invalid_field_type",
                        name,
                        "REQ-0658",
                        {"expected": "dict", "actual": type(supplied).__name__},
                    )
                )
                continue
            members: dict[str, object] = {}
            key_descriptor = {"type": "identifier"}
            path_descriptor = {"type": "project_path"}
            for member_id, member in supplied.items():
                member_path = _join(name, member_id)
                diagnostics.extend(
                    validate_descriptor_value(
                        member_id,
                        key_descriptor,
                        bundle,
                        _join(name, f"key({member_id})"),
                    )
                )
                if isinstance(member, str):
                    member_diagnostics = validate_descriptor_value(
                        member, path_descriptor, bundle, member_path
                    )
                    diagnostics.extend(member_diagnostics)
                    if not member_diagnostics:
                        members[str(member_id)] = {"path": member}
                else:
                    normalized_member, member_diagnostics = _validate_partial_member(
                        member, class_name, identity, member_path, bundle, fragment
                    )
                    diagnostics.extend(member_diagnostics)
                    members[str(member_id)] = normalized_member
            normalized[name] = members
            continue

        if not isinstance(supplied, list):
            diagnostics.append(
                _diagnostic(
                    "invalid_field_type",
                    name,
                    "REQ-0658",
                    {"expected": "list", "actual": type(supplied).__name__},
                )
            )
            continue
        members = []
        seen: set[str] = set()
        for index, member in enumerate(supplied):
            member_id = member.get(identity) if isinstance(member, dict) else None
            member_path = (
                _join(name, member_id)
                if isinstance(member_id, str)
                else _join(name, index)
            )
            normalized_member, member_diagnostics = _validate_partial_member(
                member, class_name, identity, member_path, bundle, fragment
            )
            diagnostics.extend(member_diagnostics)
            if isinstance(member_id, str):
                if member_id in seen:
                    diagnostics.append(
                        _diagnostic(
                            "duplicate_identifier",
                            _join(member_path, identity),
                            "REQ-0659",
                            {"identifier": member_id},
                        )
                    )
                seen.add(member_id)
            members.append(normalized_member)
        normalized[name] = members

    return normalized, diagnostics


def _clear_provenance(provenance: dict[str, SourceOrigin], prefix: str) -> None:
    for key in tuple(provenance):
        if key == prefix or key.startswith(f"{prefix}."):
            del provenance[key]


def _record_provenance(
    value: object,
    path: str,
    origin: Path,
    provenance: dict[str, SourceOrigin],
) -> None:
    """Attribute one written value, and every leaf below it, to its layer."""
    provenance[path] = SourceOrigin(file=origin, spec_path=path)
    if isinstance(value, dict):
        for name, nested in value.items():
            _record_provenance(nested, _join(path, name), origin, provenance)


def _replace(
    value: object,
    path: str,
    origin: Path,
    provenance: dict[str, SourceOrigin],
) -> object:
    copied = copy.deepcopy(value)
    _clear_provenance(provenance, path)
    _record_provenance(copied, path, origin, provenance)
    return copied


def _composing_member(
    accumulated: object,
    incoming: object,
    type_value: object,
    bundle: SchemaBundle,
) -> str | None:
    """Return the union member both values compose under, or None to replace."""
    if not isinstance(accumulated, dict) or not isinstance(incoming, dict):
        return None
    member = matching_type(incoming, type_value, bundle, fragment=True)
    if member is None:
        return None
    if member != matching_type(accumulated, type_value, bundle, fragment=True):
        return None
    return member


def _compose_class(
    accumulated: dict[str, object],
    incoming: dict[str, object],
    fields: list[dict[str, dict[str, object]]],
    bundle: SchemaBundle,
    path: str,
    origin: Path,
    provenance: dict[str, SourceOrigin],
) -> dict[str, object]:
    descriptors = {
        name: descriptor for entry in fields for name, descriptor in entry.items()
    }
    composed = dict(accumulated)
    for name, value in incoming.items():
        field_path = _join(path, name)
        descriptor = descriptors.get(name)
        if descriptor is None or name not in composed:
            composed[name] = _replace(value, field_path, origin, provenance)
            continue
        composed[name] = _compose_value(
            composed[name],
            value,
            descriptor["type"],
            bundle,
            field_path,
            origin,
            provenance,
        )
    return composed


def _compose_operation(
    accumulated: dict[str, object],
    incoming: dict[str, object],
    registry_name: str,
    bundle: SchemaBundle,
    path: str,
    origin: Path,
    provenance: dict[str, SourceOrigin],
) -> object:
    """Compose two registry values, which R007 admits one keyword each."""
    if len(accumulated) != 1 or len(incoming) != 1:
        return _replace(incoming, path, origin, provenance)
    operation, payload = next(iter(incoming.items()))
    inherited_operation, inherited_payload = next(iter(accumulated.items()))
    if operation != inherited_operation:
        return _replace(incoming, path, origin, provenance)
    definition = bundle.registries.get(registry_name, {}).get(operation)
    operation_path = _join(path, operation)
    if definition is None:
        return _replace(incoming, path, origin, provenance)
    if isinstance(definition, list):
        if not isinstance(inherited_payload, dict) or not isinstance(payload, dict):
            return _replace(incoming, path, origin, provenance)
        composed = _compose_class(
            inherited_payload,
            payload,
            definition,
            bundle,
            operation_path,
            origin,
            provenance,
        )
    else:
        composed = _compose_value(
            inherited_payload,
            payload,
            definition["type"],
            bundle,
            operation_path,
            origin,
            provenance,
        )
    return {operation: composed}


def _compose_value(
    accumulated: object,
    incoming: object,
    type_value: object,
    bundle: SchemaBundle,
    path: str,
    origin: Path,
    provenance: dict[str, SourceOrigin],
) -> object:
    """Compose one written value onto the value it inherits, by declared kind.

    A class composes field by field, a mapping key by key, and a registry
    value only when both name one keyword.  Every other kind, including every
    list, replaces.  A null here is an R006 value, never a clearing marker:
    REQ-0633 keeps the marker at the two composition boundaries above.
    """
    member = _composing_member(accumulated, incoming, type_value, bundle)
    if member is None:
        return _replace(incoming, path, origin, provenance)
    assert isinstance(accumulated, dict) and isinstance(incoming, dict)
    if member in bundle.classes:
        return _compose_class(
            accumulated,
            incoming,
            bundle.classes[member],
            bundle,
            path,
            origin,
            provenance,
        )
    if member.startswith("dict[") and member.endswith("]"):
        _, inner = split_type_arguments(member[5:-1])
        composed = dict(accumulated)
        for key, value in incoming.items():
            key_path = _join(path, key)
            if key not in composed:
                composed[key] = _replace(value, key_path, origin, provenance)
                continue
            composed[key] = _compose_value(
                composed[key], value, inner, bundle, key_path, origin, provenance
            )
        return composed
    alias = bundle.aliases.get(member)
    if alias is None:
        return _replace(incoming, path, origin, provenance)
    registry_name = alias.get("registry")
    if registry_name is not None:
        return _compose_operation(
            accumulated, incoming, registry_name, bundle, path, origin, provenance
        )
    return _compose_value(
        accumulated, incoming, alias["type"], bundle, path, origin, provenance
    )


def _merge_member(
    accumulated: dict[str, object],
    incoming: dict[str, object],
    class_name: str,
    logical_path: str,
    origin: Path,
    bundle: SchemaBundle,
    provenance: dict[str, SourceOrigin],
    diagnostics: list[ValidationDiagnostic],
    compose: bool = False,
) -> None:
    fields = class_fields(bundle, class_name)
    for name, value in incoming.items():
        field_path = _join(logical_path, name)
        if value is None:
            descriptor = fields.get(name, {})
            if descriptor.get("required") or name not in accumulated:
                diagnostics.append(
                    _diagnostic(
                        "invalid_clear", field_path, "REQ-0660", {"field": name}
                    )
                )
                continue
            accumulated.pop(name, None)
            _clear_provenance(provenance, field_path)
            continue
        if compose and name in accumulated and name in fields:
            accumulated[name] = _compose_value(
                accumulated[name],
                value,
                fields[name]["type"],
                bundle,
                field_path,
                origin,
                provenance,
            )
            continue
        accumulated[name] = _replace(value, field_path, origin, provenance)


def _merge_layers(
    contributions: Sequence[tuple[Path, dict[str, object]]],
    bundle: SchemaBundle,
) -> tuple[dict[str, object], dict[str, SourceOrigin], list[ValidationDiagnostic]]:
    resolved: dict[str, object] = {}
    provenance: dict[str, SourceOrigin] = {}
    diagnostics: list[ValidationDiagnostic] = []
    root_fields = class_fields(bundle, "root_class")
    for origin, layer in contributions:
        for name, value in layer.items():
            if name == "parents":
                continue
            if name == "schema_version":
                resolved[name] = copy.deepcopy(value)
                provenance[name] = SourceOrigin(file=origin, spec_path=name)
                continue
            collection = _KEYED_COLLECTIONS.get(name)
            if value is None:
                if root_fields.get(name, {}).get("required") or name not in resolved:
                    diagnostics.append(
                        _diagnostic("invalid_clear", name, "REQ-0660", {"field": name})
                    )
                    continue
                resolved.pop(name, None)
                _clear_provenance(provenance, name)
                continue
            if name == "windows":
                target = resolved.setdefault(name, {})
                assert isinstance(target, dict) and isinstance(value, dict)
                for window, definition in value.items():
                    target[window] = _replace(
                        definition, f"windows.{window}", origin, provenance
                    )
                continue
            if collection is None:
                resolved[name] = copy.deepcopy(value)
                _clear_provenance(provenance, name)
                provenance[name] = SourceOrigin(file=origin, spec_path=name)
                continue

            kind, identity, class_name = collection
            if kind == "mapping":
                target = resolved.setdefault(name, {})
                assert isinstance(target, dict) and isinstance(value, dict)
                for member_id, member in value.items():
                    assert isinstance(member, dict)
                    logical_path = _join(name, member_id)
                    if member_id not in target:
                        target[member_id] = _replace(
                            member, logical_path, origin, provenance
                        )
                    else:
                        _merge_member(
                            target[member_id],
                            member,
                            class_name,
                            logical_path,
                            origin,
                            bundle,
                            provenance,
                            diagnostics,
                        )
                continue

            target = resolved.setdefault(name, [])
            assert isinstance(target, list) and isinstance(value, list)
            positions = {
                member.get(identity): index
                for index, member in enumerate(target)
                if isinstance(member, dict)
            }
            for member in value:
                assert isinstance(member, dict)
                member_id = member.get(identity)
                logical_path = _join(name, member_id)
                if member_id not in positions:
                    positions[member_id] = len(target)
                    target.append(_replace(member, logical_path, origin, provenance))
                else:
                    _merge_member(
                        target[positions[member_id]],
                        member,
                        class_name,
                        logical_path,
                        origin,
                        bundle,
                        provenance,
                        diagnostics,
                        name in _COMPOSING_COLLECTIONS,
                    )
    _materialize_fragments(resolved, bundle)
    return resolved, provenance, diagnostics


def _materialize_fragments(resolved: dict[str, object], bundle: SchemaBundle) -> None:
    """Complete every composed member once composition is finished.

    A composing collection reads each layer as a fragment, so a schema default
    is withheld while the layers compose and cannot overwrite what a parent
    wrote.  One ordinary normalization of the composed value materializes the
    defaults and the shorthand a fragment left unexpanded.
    """
    for name in _COMPOSING_COLLECTIONS:
        _, _, class_name = _KEYED_COLLECTIONS[name]
        members = resolved.get(name)
        if not isinstance(members, list):
            continue
        fields = class_fields(bundle, class_name)
        for member in members:
            if not isinstance(member, dict):
                continue
            for field, descriptor in fields.items():
                if field in member and member[field] is not None:
                    member[field] = normalize_descriptor_value(
                        member[field], descriptor, bundle
                    )


def _ast_identifiers(value: object) -> set[str]:
    names: set[str] = set()
    if isinstance(value, Mapping):
        if value.get("kind") == "identifier" and isinstance(value.get("name"), str):
            names.add(value["name"])
        for nested in value.values():
            names.update(_ast_identifiers(nested))
    elif isinstance(value, list):
        for nested in value:
            names.update(_ast_identifiers(nested))
    return names


def _language_references(value: object, type_name: str) -> set[tuple[str, str]]:
    if not isinstance(value, str):
        return set()
    if type_name == "predicate":
        try:
            names = _ast_identifiers(parse_predicate_cached(value))
        except PredicateError:
            return set()
    elif type_name == "numeric_expression":
        try:
            names = set(numeric_identifiers(parse_numeric_cached(value)))
        except NumericError:
            return set()
    elif type_name == "aggregate_expression":
        try:
            names = set(aggregate_identifiers(parse_aggregate_cached(value)))
        except AggregateError:
            return set()
    elif type_name == "string_template":
        try:
            names = set(template_identifiers(parse_template_cached(value)))
        except TemplateError:
            return set()
    else:
        return set()
    return {("variable", name) for name in names}


# (class or registry keyword, field) positions where an `identifier` string
# names a declared dataset. Replaces the nominal `dataset_id` alias dispatch.
# Mirrors the same tables in .github/scripts/yaml-validation/validate_repository.py.
_IDENTIFIER_DATASET_FIELDS = frozenset(
    {
        ("root_class", "base"),
        ("row_class", "dataset"),
        ("intermediate_class", "dataset"),
        ("lookup", "dataset"),
    }
)

# (class, field) positions where an `identifier` string names a declared
# column. Replaces the nominal `column_name` alias dispatch.
_IDENTIFIER_COLUMN_FIELDS = frozenset(
    {
        ("column_class", "name"),
        ("root_class", "keys"),
        ("output_class", "columns"),
    }
)


def _identifier_reference_kind(scope):
    """Return the reference kind for an `identifier` string at `scope`.

    `scope` is a (class or registry keyword, field) pair, or None. Returns
    "dataset", "variable", or None when the position names no namespace.
    """
    if scope in _IDENTIFIER_DATASET_FIELDS:
        return "dataset"
    if scope in _IDENTIFIER_COLUMN_FIELDS:
        return "variable"
    return None


def _references(
    value: object,
    type_value: object,
    bundle: SchemaBundle,
    scope: object = None,
) -> set[tuple[str, str]]:
    type_name = matching_type(value, type_value, bundle)
    if type_name is None:
        return set()
    if type_name == "variable" and isinstance(value, str):
        return {("variable", value)}
    if type_name == "identifier" and isinstance(value, str):
        kind = _identifier_reference_kind(scope)
        if kind is None:
            return set()
        return {(kind, value)}
    language = _language_references(value, type_name)
    if language:
        return language
    if type_name.startswith("list[") and type_name.endswith("]"):
        if not isinstance(value, list):
            return set()
        inner = type_name[5:-1].strip()
        return set().union(*(_references(item, inner, bundle, scope) for item in value))
    if type_name.startswith("dict[") and type_name.endswith("]"):
        if not isinstance(value, dict):
            return set()
        _, inner = split_type_arguments(type_name[5:-1])
        return set().union(
            *(_references(item, inner, bundle, scope) for item in value.values())
        )
    if type_name in bundle.classes:
        if not isinstance(value, dict):
            return set()
        fields = class_fields(bundle, type_name)
        return set().union(
            *(
                _references(item, fields[name]["type"], bundle, (type_name, name))
                for name, item in value.items()
                if name in fields
            )
        )
    alias = bundle.aliases.get(type_name)
    if alias is None:
        return set()
    registry = alias.get("registry")
    if registry is None:
        return _references(value, alias["type"], bundle, scope)
    if not isinstance(value, dict) or len(value) != 1:
        return set()
    operation, payload = next(iter(value.items()))
    definition = bundle.registries.get(registry, {}).get(operation)
    if isinstance(definition, list):
        if not isinstance(payload, dict):
            return set()
        descriptors = {
            name: descriptor
            for entry in definition
            for name, descriptor in entry.items()
        }
        return set().union(
            *(
                _references(item, descriptors[name]["type"], bundle, (operation, name))
                for name, item in payload.items()
                if name in descriptors
            )
        )
    if isinstance(definition, dict):
        return _references(payload, definition["type"], bundle, scope)
    return set()


def _member_references(
    member: object,
    class_name: str,
    names: Sequence[str],
    bundle: SchemaBundle,
) -> set[tuple[str, str]]:
    if not isinstance(member, dict):
        return set()
    fields = class_fields(bundle, class_name)
    return set().union(
        *(
            _references(member[name], fields[name]["type"], bundle, (class_name, name))
            for name in names
            if name in member and name in fields
        )
    )


def _mark_reference(
    reference: tuple[str, str],
    datasets: Mapping[str, object],
    intermediates: Mapping[str, object],
    live_columns: set[str],
    live_datasets: set[str],
    live_intermediates: set[str],
) -> bool:
    kind, name = reference
    target = live_datasets if kind == "dataset" else live_columns
    if kind == "dataset" or "." not in name:
        before = len(target)
        target.add(name)
        return len(target) != before
    qualifier = name.split(".", 1)[0]
    if qualifier in datasets:
        before = len(live_datasets)
        live_datasets.add(qualifier)
        return len(live_datasets) != before
    if qualifier in intermediates:
        before = len(live_intermediates)
        live_intermediates.add(qualifier)
        return len(live_intermediates) != before
    return False


def _order_variable(term: object) -> str | None:
    if isinstance(term, str):
        return term
    if isinstance(term, dict) and isinstance(term.get("variable"), str):
        return term["variable"]
    return None


def _prune(document: dict[str, object], bundle: SchemaBundle) -> dict[str, object]:
    result = copy.deepcopy(document)
    datasets = result.get("input") if isinstance(result.get("input"), dict) else {}
    columns = result.get("columns") if isinstance(result.get("columns"), list) else []
    intermediates = (
        result.get("intermediates")
        if isinstance(result.get("intermediates"), list)
        else []
    )
    rows = result.get("rows") if isinstance(result.get("rows"), list) else []
    column_map = {
        item.get("name"): item
        for item in columns
        if isinstance(item, dict) and isinstance(item.get("name"), str)
    }
    intermediate_map = {
        item.get("id"): item
        for item in intermediates
        if isinstance(item, dict) and isinstance(item.get("id"), str)
    }

    live_columns: set[str] = set()
    output = result.get("output")
    if isinstance(output, dict):
        if isinstance(output.get("columns"), list):
            live_columns.update(
                item for item in output["columns"] if isinstance(item, str)
            )
        if isinstance(output.get("order_by"), list):
            live_columns.update(
                item
                for item in (_order_variable(term) for term in output["order_by"])
                if item is not None
            )
    if isinstance(result.get("keys"), list):
        live_columns.update(item for item in result["keys"] if isinstance(item, str))
    live_columns.update(
        name for name, item in column_map.items() if "verifications" in item
    )

    live_datasets: set[str] = set()
    if isinstance(result.get("base"), str):
        live_datasets.add(result["base"])
    live_intermediates: set[str] = set()
    root_fields = class_fields(bundle, "root_class")
    initial: set[tuple[str, str]] = set()
    if "verifications" in result:
        initial.update(
            _references(
                result["verifications"], root_fields["verifications"]["type"], bundle
            )
        )
    if "filter" in result:
        initial.update(
            _references(result["filter"], root_fields["filter"]["type"], bundle)
        )
    row_fields = class_fields(bundle, "row_class")
    for row in rows:
        if not isinstance(row, dict):
            continue
        driver = row.get("dataset", result.get("base"))
        if isinstance(driver, str):
            live_datasets.add(driver)
        for name in ("group_by", "filter"):
            if name in row:
                initial.update(_references(row[name], row_fields[name]["type"], bundle))
    for reference in initial:
        _mark_reference(
            reference,
            datasets,
            intermediate_map,
            live_columns,
            live_datasets,
            live_intermediates,
        )

    processed_columns: set[str] = set()
    processed_intermediates: set[str] = set()
    processed_rows: set[tuple[int, str]] = set()
    while True:
        changed = False
        for name in tuple(live_columns - processed_columns):
            processed_columns.add(name)
            for reference in _member_references(
                column_map.get(name),
                "column_class",
                ("derivation", "verifications"),
                bundle,
            ):
                changed |= _mark_reference(
                    reference,
                    datasets,
                    intermediate_map,
                    live_columns,
                    live_datasets,
                    live_intermediates,
                )
        for row_index, row in enumerate(rows):
            derivations = row.get("derivations") if isinstance(row, dict) else None
            if not isinstance(derivations, dict):
                continue
            for target in tuple(live_columns):
                marker = (row_index, target)
                if marker in processed_rows or target not in derivations:
                    continue
                processed_rows.add(marker)
                for reference in _references(derivations[target], "derivation", bundle):
                    changed |= _mark_reference(
                        reference,
                        datasets,
                        intermediate_map,
                        live_columns,
                        live_datasets,
                        live_intermediates,
                    )
        intermediate_fields = class_fields(bundle, "intermediate_class")
        for intermediate_id in tuple(live_intermediates - processed_intermediates):
            processed_intermediates.add(intermediate_id)
            intermediate = intermediate_map.get(intermediate_id)
            if not isinstance(intermediate, dict):
                continue
            if isinstance(intermediate.get("dataset"), str):
                live_datasets.add(intermediate["dataset"])
            for name, value in intermediate.items():
                if name in {"id", "dataset"} or name not in intermediate_fields:
                    continue
                for reference in _references(
                    value, intermediate_fields[name]["type"], bundle
                ):
                    changed |= _mark_reference(
                        reference,
                        datasets,
                        intermediate_map,
                        live_columns,
                        live_datasets,
                        live_intermediates,
                    )
        if not changed:
            break

    if isinstance(result.get("columns"), list):
        result["columns"] = [
            item
            for item in result["columns"]
            if isinstance(item, dict) and item.get("name") in live_columns
        ]
    if isinstance(result.get("input"), dict):
        result["input"] = {
            name: value
            for name, value in result["input"].items()
            if name in live_datasets
        }
    if isinstance(result.get("intermediates"), list):
        result["intermediates"] = [
            item
            for item in result["intermediates"]
            if isinstance(item, dict) and item.get("id") in live_intermediates
        ]
        if not result["intermediates"]:
            del result["intermediates"]
    if isinstance(result.get("rows"), list):
        for row in result["rows"]:
            derivations = row.get("derivations") if isinstance(row, dict) else None
            if isinstance(derivations, dict):
                row["derivations"] = {
                    name: value
                    for name, value in derivations.items()
                    if name in live_columns
                }
    return result


def _column_dependencies(
    column: dict[str, object],
    rows: Sequence[object],
    intermediates: Mapping[str, object],
    bundle: SchemaBundle,
) -> set[str]:
    references: set[tuple[str, str]] = set()
    if "derivation" in column:
        references.update(_references(column["derivation"], "derivation", bundle))
    name = column.get("name")
    for row in rows:
        derivations = row.get("derivations") if isinstance(row, dict) else None
        if isinstance(derivations, dict) and name in derivations:
            references.update(_references(derivations[name], "derivation", bundle))
    dependencies: set[str] = set()
    for kind, reference in references:
        if kind != "variable":
            continue
        if "." not in reference:
            dependencies.add(reference)
            continue
        intermediate = intermediates.get(reference.split(".", 1)[0])
        for intermediate_kind, intermediate_reference in _member_references(
            intermediate,
            "intermediate_class",
            ("key_base", "between", "filter", "order_by"),
            bundle,
        ):
            if intermediate_kind == "variable" and "." not in intermediate_reference:
                dependencies.add(intermediate_reference)
    return dependencies


def _order_columns(
    document: dict[str, object], bundle: SchemaBundle
) -> tuple[dict[str, object], list[ValidationDiagnostic]]:
    columns = document.get("columns")
    if not isinstance(columns, list):
        return document, []
    names = [item.get("name") if isinstance(item, dict) else None for item in columns]
    if not all(isinstance(name, str) for name in names):
        return document, []
    typed_names = [name for name in names if isinstance(name, str)]
    positions = {name: index for index, name in enumerate(typed_names)}
    rows = document.get("rows") if isinstance(document.get("rows"), list) else []
    intermediate_items = (
        document.get("intermediates")
        if isinstance(document.get("intermediates"), list)
        else []
    )
    intermediates = {
        item.get("id"): item
        for item in intermediate_items
        if isinstance(item, dict) and isinstance(item.get("id"), str)
    }
    dependencies = {
        item["name"]: _column_dependencies(item, rows, intermediates, bundle)
        for item in columns
        if isinstance(item, dict) and isinstance(item.get("name"), str)
    }
    diagnostics = [
        _diagnostic(
            "unknown_reference",
            _join(_join("columns", name), "derivation"),
            "REQ-0070",
            {"column": name, "dependency": dependency},
        )
        for name, required in dependencies.items()
        for dependency in sorted(required)
        if dependency not in positions
    ]
    if diagnostics:
        return document, diagnostics

    dependents = {name: set() for name in typed_names}
    indegree = {name: 0 for name in typed_names}
    for name, required in dependencies.items():
        for dependency in required:
            dependents[dependency].add(name)
            indegree[name] += 1
    ready = sorted(
        (name for name in typed_names if indegree[name] == 0), key=positions.get
    )
    ordered: list[str] = []
    while ready:
        name = ready.pop(0)
        ordered.append(name)
        for dependent in sorted(dependents[name], key=positions.get):
            indegree[dependent] -= 1
            if indegree[dependent] == 0:
                ready.append(dependent)
                ready.sort(key=positions.get)
    if len(ordered) != len(typed_names):
        cycle = [name for name in typed_names if indegree[name] > 0]
        return document, [
            _diagnostic(
                "dependency_cycle",
                "columns",
                "REQ-0072",
                {"cycle": cycle},
            )
        ]
    by_name = {item["name"]: item for item in columns if isinstance(item, dict)}
    result = copy.deepcopy(document)
    result["columns"] = [by_name[name] for name in ordered]
    return result, []


def _schema_order(
    document: dict[str, object], bundle: SchemaBundle
) -> dict[str, object]:
    root = class_fields(bundle, "root_class")
    classes = {
        "input": "dataset_class",
        "intermediates": "intermediate_class",
        "columns": "column_class",
        "rows": "row_class",
    }
    ordered: dict[str, object] = {}
    for name in root:
        if name not in document or name == "parents":
            continue
        value = document[name]
        class_name = classes.get(name)
        if class_name is None:
            ordered[name] = value
            continue
        fields = class_fields(bundle, class_name)
        if name == "input" and isinstance(value, dict):
            ordered[name] = {
                member_id: {
                    field: member[field]
                    for field in fields
                    if isinstance(member, dict) and field in member
                }
                for member_id, member in value.items()
            }
        elif isinstance(value, list):
            ordered[name] = [
                {
                    field: member[field]
                    for field in fields
                    if isinstance(member, dict) and field in member
                }
                for member in value
            ]
        else:
            ordered[name] = value
    return ordered


def resolve_specification(
    entry: str | Path,
    schema_bundle: SchemaBundle,
    *,
    entry_document: object | None = None,
    resources: ProjectResources | None = None,
) -> ResolvedSpecification:
    """Resolve one local R017 graph into a complete specification.

    ``entry_document`` lets a workflow pass the immutable R021 snapshot it
    already parsed; ordinary callers simply pass the entry path.
    """
    entry_path = Path(entry).resolve()
    contributions: list[tuple[Path, dict[str, object]]] = []
    completed: set[Path] = set()
    written_paths: dict[tuple[Path, str], str] = {}
    active: list[Path] = []
    entry_version: object | None = None

    def visit(path: Path, supplied: object | None = None) -> None:
        nonlocal entry_version
        canonical = path.resolve()
        if canonical in active:
            raise SpecificationError(
                [
                    _diagnostic(
                        "inheritance_cycle",
                        "parents",
                        "REQ-0655",
                        {"reason": "parent_chain_returns_to_entry"},
                    )
                ]
            )
        if canonical in completed:
            return
        try:
            raw = supplied if supplied is not None else read_yaml_document(canonical)
        except (OSError, SpecificationError) as error:
            if isinstance(error, SpecificationError):
                raise
            raise SpecificationError(
                [
                    _diagnostic(
                        "parent_not_found",
                        "parents",
                        "REQ-0654",
                        {"path": str(path)},
                    )
                ]
            ) from error
        normalized, diagnostics = _validate_layer(raw, schema_bundle)
        if diagnostics or normalized is None:
            raise SpecificationError(diagnostics)
        version = normalized.get("schema_version")
        if canonical == entry_path:
            entry_version = version
            if version != schema_bundle.version:
                raise SpecificationError(
                    [
                        _diagnostic(
                            "schema_version_mismatch",
                            "schema_version",
                            "REQ-0245",
                            {"expected": schema_bundle.version, "actual": version},
                        )
                    ]
                )
        elif version != schema_bundle.version or (
            entry_version is not None and version != entry_version
        ):
            raise SpecificationError(
                [
                    _diagnostic(
                        "schema_version_mismatch",
                        "parents",
                        "REQ-0656",
                        {
                            "entry_version": entry_version,
                            "parent_version": version,
                        },
                    )
                ]
            )
        active.append(canonical)
        for parent in normalized.get("parents", []):
            if _is_nonlocal_parent(parent):
                raise SpecificationError(
                    [
                        _diagnostic(
                            "invalid_parent_path",
                            "parents",
                            "REQ-0653",
                            {"reason": "remote_reference"},
                        )
                    ]
                )
            candidate = Path(parent)
            if not candidate.is_absolute():
                candidate = canonical.parent / candidate
            if not candidate.is_file():
                raise SpecificationError(
                    [
                        _diagnostic(
                            "parent_not_found",
                            "parents",
                            "REQ-0654",
                            {"path": parent},
                        )
                    ]
                )
            visit(candidate)
        active.pop()
        completed.add(canonical)
        written_paths.update(_layer_input_paths(normalized, canonical))
        _rebase_layer_paths(normalized, canonical, entry_path)
        contributions.append((canonical, normalized))

    raw_entry = (
        entry_document if entry_document is not None else read_yaml_document(entry_path)
    )
    visit(entry_path, raw_entry)
    if not isinstance(raw_entry, dict):
        raise TypeError("validated entry is a mapping")

    resolved, provenance, diagnostics = _merge_layers(contributions, schema_bundle)
    if diagnostics:
        raise SpecificationError(diagnostics)
    from yamaa.schema.row_catalog import expand_row_catalogs

    resolved = expand_row_catalogs(resolved, entry_path, resources)
    from yamaa.schema.windows import expand_named_windows

    resolved = expand_named_windows(
        resolved, schema_bundle, strict=False, provenance=provenance
    )
    if "parents" in raw_entry:
        resolved = _prune(resolved, schema_bundle)
        resolved, diagnostics = _order_columns(resolved, schema_bundle)
        if diagnostics:
            raise SpecificationError(diagnostics)
    resolved = expand_named_windows(resolved, schema_bundle)
    resolved = _schema_order(resolved, schema_bundle)
    diagnostics = validate_specification(resolved, schema_bundle)
    if diagnostics:
        raise SpecificationError(diagnostics)
    try:
        specification = Specification.model_validate(resolved, strict=True)
    except ValidationError as error:
        raise SpecificationError(
            [
                ValidationDiagnostic(
                    condition="model_contract_mismatch",
                    spec_paths=(_model_path(item["loc"]),),
                    context={"reason": item["msg"]},
                )
                for item in error.errors(include_url=False, include_input=False)
            ]
        ) from error
    return ResolvedSpecification(
        specification=specification,
        document=resolved,  # type: ignore[arg-type]
        entry_path=entry_path,
        layers=tuple(path for path, _ in contributions),
        provenance=provenance,
        layer_paths=_resolved_layer_paths(resolved, provenance, written_paths),
    )
