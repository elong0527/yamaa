"""Expand a fixed CSV catalog into ordinary row templates."""

from __future__ import annotations

import copy
import math
import re
from pathlib import Path

from yamaa.io.csv import CsvProfileFailure, scan_records
from yamaa.io.project import ProjectResources, ResourceFailure
from yamaa.specification.diagnostics import SpecificationError, ValidationDiagnostic

_PLACEHOLDER = re.compile(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}")
_IDENTIFIER = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")


def _fail(path: str, condition: str, **context: object) -> None:
    raise SpecificationError(
        [
            ValidationDiagnostic(
                condition=condition,
                spec_paths=(path,),
                requirement="REQ-1249",
                context=context,
            )
        ]
    )


def _catalog_records(
    catalog: object, resources: ProjectResources, path: str
) -> list[dict[str, object]]:
    if not isinstance(catalog, dict) or set(catalog) - {
        "path",
        "id_column",
        "types",
        "unique_columns",
    }:
        _fail(path, "invalid_row_catalog", reason="invalid_declaration")
    written = catalog.get("path")
    if not isinstance(written, str) or not written:
        _fail(path, "invalid_row_catalog", reason="invalid_path")
    id_column = catalog.get("id_column")
    if not isinstance(id_column, str) or not _IDENTIFIER.fullmatch(id_column):
        _fail(path, "invalid_row_catalog", reason="invalid_id_column")
    types = catalog.get("types", {})
    if not isinstance(types, dict) or any(
        not isinstance(name, str)
        or not _IDENTIFIER.fullmatch(name)
        or kind not in ("str", "int", "float")
        for name, kind in types.items()
    ):
        _fail(path, "invalid_row_catalog", reason="invalid_types")
    unique_columns = catalog.get("unique_columns", [])
    if not isinstance(unique_columns, list) or any(
        not isinstance(name, str) or not _IDENTIFIER.fullmatch(name)
        for name in unique_columns
    ):
        _fail(path, "invalid_row_catalog", reason="invalid_unique_columns")
    try:
        snapshot = resources.capture(written)
        resources.verify(snapshot)
    except ResourceFailure as error:
        _fail(path, error.condition, resource=written)
    try:
        parsed = scan_records(snapshot.content.decode("utf-8"))
    except (UnicodeError, CsvProfileFailure) as error:
        _fail(path, "invalid_row_catalog", reason=str(error))
    if any(byte > 0x7F for byte in snapshot.content):
        _fail(path, "invalid_row_catalog", reason="non_ascii_source")
    if not parsed:
        _fail(path, "invalid_row_catalog", reason="empty_catalog")
    header, *records = parsed
    if not header or any(
        not isinstance(name, str) or not _IDENTIFIER.fullmatch(name) for name in header
    ):
        _fail(path, "invalid_row_catalog", reason="invalid_header")
    if len(header) != len(set(header)):
        _fail(path, "invalid_row_catalog", reason="duplicate_header")
    if not records:
        _fail(path, "invalid_row_catalog", reason="empty_catalog")
    if id_column not in header:
        _fail(path, "invalid_row_catalog", reason="missing_id_column")
    if set(types) - set(header):
        _fail(path, "invalid_row_catalog", reason="unknown_typed_column")
    if len(unique_columns) != len(set(unique_columns)) or set(unique_columns) - set(
        header
    ):
        _fail(path, "invalid_row_catalog", reason="invalid_unique_columns")
    unique_values: dict[str, set[object]] = {name: set() for name in unique_columns}
    converted: list[dict[str, object]] = []
    seen: set[str] = set()
    for line, record in enumerate(records, 2):
        if len(record) != len(header):
            _fail(path, "invalid_row_catalog", reason="field_count", line=line)
        item: dict[str, object] = dict(zip(header, record, strict=True))
        if any(value is None for value in item.values()):
            _fail(path, "invalid_row_catalog", reason="missing_cell", line=line)
        identifier = item[id_column]
        if not isinstance(identifier, str) or not _IDENTIFIER.fullmatch(identifier):
            _fail(path, "invalid_row_catalog", reason="invalid_id", line=line)
        if identifier in seen:
            _fail(path, "invalid_row_catalog", reason="duplicate_id", line=line)
        seen.add(identifier)
        for name, kind in types.items():
            if kind == "str":
                continue
            try:
                item[name] = int(item[name]) if kind == "int" else float(item[name])
            except (TypeError, ValueError):
                _fail(path, "invalid_row_catalog", reason="invalid_type", line=line)
            if kind == "float" and not math.isfinite(item[name]):
                _fail(path, "invalid_row_catalog", reason="invalid_type", line=line)
        for name, values in unique_values.items():
            if item[name] in values:
                _fail(path, "invalid_row_catalog", reason="duplicate_value", line=line)
            values.add(item[name])
        converted.append(item)
    return converted


def _substitute(
    value: object, record: dict[str, object], path: str, key: str = ""
) -> object:
    if isinstance(value, dict):
        return {
            name: _substitute(item, record, path, name) for name, item in value.items()
        }
    if isinstance(value, list):
        return [_substitute(item, record, path, key) for item in value]
    if not isinstance(value, str) or "${" not in value:
        return value
    match = _PLACEHOLDER.fullmatch(value)
    if match and key not in ("filter", "when"):
        name = match.group(1)
        if name not in record:
            _fail(path, "unknown_row_catalog_column", column=name)
        return record[name]
    if key not in ("filter", "when"):
        _fail(path, "invalid_row_catalog_placeholder", field=key)

    def replacement(found: re.Match[str]) -> str:
        name = found.group(1)
        if name not in record:
            _fail(path, "unknown_row_catalog_column", column=name)
        item = record[name]
        if isinstance(item, str):
            return "'" + item.replace("'", "''") + "'"
        return str(item)

    expanded = _PLACEHOLDER.sub(replacement, value)
    if "${" in expanded:
        _fail(path, "invalid_row_catalog_placeholder", field=key)
    return expanded


def expand_row_catalogs(
    document: dict[str, object], entry: Path, resources: ProjectResources | None = None
) -> dict[str, object]:
    """Expand catalog rows before final schema and execution validation."""
    rows = document.get("rows")
    if not isinstance(rows, list) or not any(
        isinstance(row, dict) and "catalog" in row for row in rows
    ):
        return document
    view = resources or ProjectResources(entry.parent)
    view = view.with_base_directory(entry.parent)
    expanded_document = copy.deepcopy(document)
    expanded_rows: list[object] = []
    seen_ids = {
        row["id"]
        for row in expanded_document["rows"]
        if isinstance(row, dict)
        and "catalog" not in row
        and isinstance(row.get("id"), str)
    }
    for index, row in enumerate(expanded_document["rows"]):
        if not isinstance(row, dict) or "catalog" not in row:
            expanded_rows.append(row)
            continue
        path = f"rows[{index}].catalog"
        row_id = row.get("id")
        if not isinstance(row_id, str) or not _IDENTIFIER.fullmatch(row_id):
            _fail(path, "invalid_row_catalog", reason="invalid_template_id")
        catalog = row.pop("catalog")
        for record in _catalog_records(catalog, view, path):
            variant = _substitute(row, record, path)
            if not isinstance(variant, dict):
                _fail(path, "invalid_row_catalog", reason="invalid_template")
            variant["id"] = f"{row_id}_{record[catalog['id_column']]}"
            if variant["id"] in seen_ids:
                _fail(path, "invalid_row_catalog", reason="duplicate_generated_id")
            seen_ids.add(variant["id"])
            expanded_rows.append(variant)
    expanded_document["rows"] = expanded_rows
    return expanded_document
