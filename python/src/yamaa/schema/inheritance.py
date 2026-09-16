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
    "record_lookups": ("list", "id", "record_lookup_class"),
    "columns": ("list", "name", "column_class"),
    "rows": ("list", "id", "row_class"),
}


class _FrozenModel(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)


class SourceOrigin(_FrozenModel):
    """The layer and authored logical path that supplied one resolved value."""

    file: Path
    spec_path: str


class ResolvedSpecification(_FrozenModel):
    """One complete R017 result together with its diagnostic provenance."""

    specification: Specification
    document: dict[str, JsonValue]
    entry_path: Path
    layers: tuple[Path, ...]
    provenance: dict[str, SourceOrigin]


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
    if _rooted_project_path(value):
        return value
    target = (layer.parent / value).resolve()
    try:
        return Path(os.path.relpath(target, entry.parent.resolve())).as_posix()
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
    if isinstance(output, dict) and isinstance(output.get("path"), str):
        output["path"] = _rebase_path(output["path"], layer, entry)


def _validate_partial_member(
    value: object,
    class_name: str,
    identity: str | None,
    path: str,
    bundle: SchemaBundle,
) -> tuple[dict[str, object] | object, list[ValidationDiagnostic]]:
    if not isinstance(value, dict):
        return value, [
            _diagnostic(
                "invalid_field_type",
                path,
                "R017-45",
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
                "R017-45",
                {"field": identity, "class": class_name},
            )
        )
    for name in value:
        if name not in fields:
            diagnostics.append(
                _diagnostic(
                    "unknown_field",
                    _join(path, name),
                    "R017-45",
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
                        "R017-47",
                        {"field": name},
                    )
                )
            else:
                normalized[name] = None
            continue
        diagnostics.extend(
            validate_descriptor_value(supplied, descriptor, bundle, field_path)
        )
        normalized[name] = normalize_descriptor_value(supplied, descriptor, bundle)
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
                "R017-45",
                {"expected": "root_class", "actual": type(document).__name__},
            )
        ]

    fields = class_fields(bundle, "root_class")
    diagnostics: list[ValidationDiagnostic] = []
    normalized: dict[str, object] = {}
    if "schema_version" not in document:
        diagnostics.append(
            _diagnostic(
                "schema_version_mismatch",
                "schema_version",
                "R017-43",
                {"expected": bundle.version, "actual": None},
            )
        )
    for name in document:
        if name not in fields:
            diagnostics.append(
                _diagnostic(
                    "unknown_field",
                    str(name),
                    "R017-45",
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
                    _diagnostic("invalid_clear", name, "R017-47", {"field": name})
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
        if kind == "mapping":
            if not isinstance(supplied, dict):
                diagnostics.append(
                    _diagnostic(
                        "invalid_field_type",
                        name,
                        "R017-45",
                        {"expected": "dict", "actual": type(supplied).__name__},
                    )
                )
                continue
            members: dict[str, object] = {}
            key_descriptor = {"type": "dataset_id"}
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
                        member, class_name, identity, member_path, bundle
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
                    "R017-45",
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
                member, class_name, identity, member_path, bundle
            )
            diagnostics.extend(member_diagnostics)
            if isinstance(member_id, str):
                if member_id in seen:
                    diagnostics.append(
                        _diagnostic(
                            "duplicate_identifier",
                            _join(member_path, identity),
                            "R017-46",
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


def _merge_member(
    accumulated: dict[str, object],
    incoming: dict[str, object],
    class_name: str,
    logical_path: str,
    origin: Path,
    bundle: SchemaBundle,
    provenance: dict[str, SourceOrigin],
    diagnostics: list[ValidationDiagnostic],
) -> None:
    fields = class_fields(bundle, class_name)
    for name, value in incoming.items():
        field_path = _join(logical_path, name)
        if value is None:
            descriptor = fields.get(name, {})
            if descriptor.get("required") or name not in accumulated:
                diagnostics.append(
                    _diagnostic("invalid_clear", field_path, "R017-47", {"field": name})
                )
                continue
            accumulated.pop(name, None)
            _clear_provenance(provenance, field_path)
            continue
        accumulated[name] = copy.deepcopy(value)
        _clear_provenance(provenance, field_path)
        provenance[field_path] = SourceOrigin(file=origin, spec_path=field_path)


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
                        _diagnostic("invalid_clear", name, "R017-47", {"field": name})
                    )
                    continue
                resolved.pop(name, None)
                _clear_provenance(provenance, name)
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
                        target[member_id] = copy.deepcopy(member)
                        provenance[logical_path] = SourceOrigin(
                            file=origin, spec_path=logical_path
                        )
                        for field in member:
                            field_path = _join(logical_path, field)
                            provenance[field_path] = SourceOrigin(
                                file=origin, spec_path=field_path
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
                    target.append(copy.deepcopy(member))
                    provenance[logical_path] = SourceOrigin(
                        file=origin, spec_path=logical_path
                    )
                    for field in member:
                        field_path = _join(logical_path, field)
                        provenance[field_path] = SourceOrigin(
                            file=origin, spec_path=field_path
                        )
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
                    )
    return resolved, provenance, diagnostics


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
    if type_name == "sql":
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


def _references(
    value: object,
    type_value: object,
    bundle: SchemaBundle,
) -> set[tuple[str, str]]:
    type_name = matching_type(value, type_value, bundle)
    if type_name is None:
        return set()
    if type_name in {"variable", "column_name"} and isinstance(value, str):
        return {("variable", value)}
    if type_name == "dataset_id" and isinstance(value, str):
        return {("dataset", value)}
    language = _language_references(value, type_name)
    if language:
        return language
    if type_name.startswith("list[") and type_name.endswith("]"):
        if not isinstance(value, list):
            return set()
        inner = type_name[5:-1].strip()
        return set().union(*(_references(item, inner, bundle) for item in value))
    if type_name.startswith("dict[") and type_name.endswith("]"):
        if not isinstance(value, dict):
            return set()
        _, inner = split_type_arguments(type_name[5:-1])
        return set().union(
            *(_references(item, inner, bundle) for item in value.values())
        )
    if type_name in bundle.classes:
        if not isinstance(value, dict):
            return set()
        fields = class_fields(bundle, type_name)
        return set().union(
            *(
                _references(item, fields[name]["type"], bundle)
                for name, item in value.items()
                if name in fields
            )
        )
    alias = bundle.aliases.get(type_name)
    if alias is None:
        return set()
    registry = alias.get("registry")
    if registry is None:
        return _references(value, alias["type"], bundle)
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
                _references(item, descriptors[name]["type"], bundle)
                for name, item in payload.items()
                if name in descriptors
            )
        )
    if isinstance(definition, dict):
        return _references(payload, definition["type"], bundle)
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
            _references(member[name], fields[name]["type"], bundle)
            for name in names
            if name in member and name in fields
        )
    )


def _mark_reference(
    reference: tuple[str, str],
    datasets: Mapping[str, object],
    lookups: Mapping[str, object],
    live_columns: set[str],
    live_datasets: set[str],
    live_lookups: set[str],
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
    if qualifier in lookups:
        before = len(live_lookups)
        live_lookups.add(qualifier)
        return len(live_lookups) != before
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
    lookups = (
        result.get("record_lookups")
        if isinstance(result.get("record_lookups"), list)
        else []
    )
    rows = result.get("rows") if isinstance(result.get("rows"), list) else []
    column_map = {
        item.get("name"): item
        for item in columns
        if isinstance(item, dict) and isinstance(item.get("name"), str)
    }
    lookup_map = {
        item.get("id"): item
        for item in lookups
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
    live_lookups: set[str] = set()
    root_fields = class_fields(bundle, "root_class")
    initial: set[tuple[str, str]] = set()
    if "verifications" in result:
        initial.update(
            _references(
                result["verifications"], root_fields["verifications"]["type"], bundle
            )
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
            reference, datasets, lookup_map, live_columns, live_datasets, live_lookups
        )

    processed_columns: set[str] = set()
    processed_lookups: set[str] = set()
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
                    lookup_map,
                    live_columns,
                    live_datasets,
                    live_lookups,
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
                        lookup_map,
                        live_columns,
                        live_datasets,
                        live_lookups,
                    )
        lookup_fields = class_fields(bundle, "record_lookup_class")
        for lookup_id in tuple(live_lookups - processed_lookups):
            processed_lookups.add(lookup_id)
            lookup = lookup_map.get(lookup_id)
            if not isinstance(lookup, dict):
                continue
            if isinstance(lookup.get("dataset"), str):
                live_datasets.add(lookup["dataset"])
            for name, value in lookup.items():
                if name in {"id", "dataset"} or name not in lookup_fields:
                    continue
                for reference in _references(
                    value, lookup_fields[name]["type"], bundle
                ):
                    changed |= _mark_reference(
                        reference,
                        datasets,
                        lookup_map,
                        live_columns,
                        live_datasets,
                        live_lookups,
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
    if isinstance(result.get("record_lookups"), list):
        result["record_lookups"] = [
            item
            for item in result["record_lookups"]
            if isinstance(item, dict) and item.get("id") in live_lookups
        ]
        if not result["record_lookups"]:
            del result["record_lookups"]
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
    lookups: Mapping[str, object],
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
        lookup = lookups.get(reference.split(".", 1)[0])
        for lookup_kind, lookup_reference in _member_references(
            lookup,
            "record_lookup_class",
            ("source", "between", "filter", "order_by"),
            bundle,
        ):
            if lookup_kind == "variable" and "." not in lookup_reference:
                dependencies.add(lookup_reference)
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
    lookup_items = (
        document.get("record_lookups")
        if isinstance(document.get("record_lookups"), list)
        else []
    )
    lookups = {
        item.get("id"): item
        for item in lookup_items
        if isinstance(item, dict) and isinstance(item.get("id"), str)
    }
    dependencies = {
        item["name"]: _column_dependencies(item, rows, lookups, bundle)
        for item in columns
        if isinstance(item, dict) and isinstance(item.get("name"), str)
    }
    diagnostics = [
        _diagnostic(
            "unknown_reference",
            _join(_join("columns", name), "derivation"),
            "R001-39",
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
                "R001-41",
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
        "record_lookups": "record_lookup_class",
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
) -> ResolvedSpecification:
    """Resolve one local R017 graph into a complete specification.

    ``entry_document`` lets a workflow pass the immutable R021 snapshot it
    already parsed; ordinary callers simply pass the entry path.
    """
    entry_path = Path(entry).resolve()
    contributions: list[tuple[Path, dict[str, object]]] = []
    completed: set[Path] = set()
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
                        "R017-42",
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
                        "R017-41",
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
                            "R006-4",
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
                        "R017-43",
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
                            "R017-40",
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
                            "R017-41",
                            {"path": parent},
                        )
                    ]
                )
            visit(candidate)
        active.pop()
        completed.add(canonical)
        _rebase_layer_paths(normalized, canonical, entry_path)
        contributions.append((canonical, normalized))

    raw_entry = (
        entry_document if entry_document is not None else read_yaml_document(entry_path)
    )
    visit(entry_path, raw_entry)
    if not isinstance(raw_entry, dict):
        raise TypeError("validated entry is a mapping")
    if "output" not in raw_entry:
        inherited_columns: list[str] = []
        for _, layer in contributions[:-1]:
            output = layer.get("output")
            if isinstance(output, dict) and isinstance(output.get("columns"), list):
                inherited_columns = [
                    item for item in output["columns"] if isinstance(item, str)
                ]
        raise SpecificationError(
            [
                _diagnostic(
                    "missing_entry_output",
                    "parents",
                    "R017-44",
                    {"inherited_columns": inherited_columns},
                )
            ]
        )

    resolved, provenance, diagnostics = _merge_layers(contributions, schema_bundle)
    if diagnostics:
        raise SpecificationError(diagnostics)
    if "parents" in raw_entry:
        resolved = _prune(resolved, schema_bundle)
        resolved, diagnostics = _order_columns(resolved, schema_bundle)
        if diagnostics:
            raise SpecificationError(diagnostics)
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
    )
