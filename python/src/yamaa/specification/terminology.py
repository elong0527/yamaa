"""Controlled terminology validation (REQ-0926..REQ-0958, REQ-1076..REQ-1079).

Validates what the study document declares and what column bindings claim:

- define-alone: codelist shape, duplicate identifiers and values, value
  types, uniform decode/rank, extension admission, standard references.
- composition: a column's ``submission.codelist`` naming a declared
  codelist, type agreement with the bound column, agreement with
  ``allowed_values``, and every declared codelist being used.

Runtime enforcement over the completed column (REQ-0937, REQ-0940,
REQ-0957, REQ-1156) runs where verification runs and is not checked here.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from yamaa.specification._yaml import read_yaml_document
from yamaa.specification.diagnostics import (
    SpecificationError,
    ValidationDiagnostic,
)
from yamaa.specification.models import Specification
from yamaa.specification.schema import (
    SchemaBundle,
    load_schema_bundle,
    normalize_document,
    validate_document,
)


def _diagnostic(
    condition: str,
    spec_paths: tuple[str, ...],
    requirement: str,
    context: dict[str, Any],
) -> ValidationDiagnostic:
    return ValidationDiagnostic(
        condition=condition,
        spec_paths=spec_paths,
        requirement=requirement,
        context=context,
    )


# REQ-0941: a text codelist binds to a str column, integer to int, float
# to float.
_CODELIST_TO_COLUMN: dict[str, str] = {
    "text": "str",
    "integer": "int",
    "float": "float",
}


def _codelists(define: dict[str, Any]) -> list[dict[str, Any]]:
    items = define.get("codelists") or []
    return [item for item in items if isinstance(item, dict)]


def _standards_by_id(define: dict[str, Any]) -> dict[str, dict[str, Any]]:
    standards: dict[str, dict[str, Any]] = {}
    for entry in define.get("standards") or []:
        if isinstance(entry, dict) and isinstance(entry.get("id"), str):
            standards[entry["id"]] = entry
    return standards


def _codelists_by_id(
    define: dict[str, Any],
) -> dict[str, dict[str, Any]]:
    by_id: dict[str, dict[str, Any]] = {}
    for entry in _codelists(define):
        identifier = entry.get("id")
        if isinstance(identifier, str) and identifier not in by_id:
            by_id[identifier] = entry
    return by_id


def _is_int(value: object) -> bool:
    return type(value) is int


def _is_float_like(value: object) -> bool:
    return type(value) in (int, float)


def _value_matches_data_type(value: object, data_type: str) -> bool:
    if data_type == "text":
        return isinstance(value, str)
    if data_type == "integer":
        return _is_int(value)
    if data_type == "float":
        return _is_float_like(value)
    return False


def _equality_key(value: object, data_type: str) -> tuple[str, object]:
    """Normalize one coded value for the equality REQ-0935 uses.

    Text compares by exact scalar sequence (Python string equality, with
    no normalization); numeric types compare by numeric equality, so
    1 and 1.0 are the same value. Anything else keys by its
    representation so shape validation can still report duplicates.
    """
    if data_type == "text":
        if isinstance(value, str):
            return ("text", value)
        return ("other", repr(value))
    if _is_float_like(value):
        return ("num", float(value))  # type: ignore[arg-type]
    if isinstance(value, str):
        return ("text", value)
    return ("other", repr(value))


def _item_values(
    codelist: dict[str, Any],
) -> list[Any]:
    values: list[Any] = []
    for item in codelist.get("items") or []:
        if isinstance(item, dict) and "value" in item:
            values.append(item["value"])
    return values


def validate_define_codelists(
    define: dict[str, Any],
) -> list[ValidationDiagnostic]:
    """Validate codelists declared by one study document. See REQ-0926..REQ-0952."""
    diagnostics: list[ValidationDiagnostic] = []
    codelists = _codelists(define)
    standards = _standards_by_id(define)

    seen_ids: set[str] = set()
    seen_names: set[str] = set()
    for codelist in codelists:
        identifier = codelist.get("id")
        name = codelist.get("name")
        path = f"codelists.{identifier}" if isinstance(identifier, str) else "codelists"
        data_type = codelist.get("data_type", "text")
        if not isinstance(data_type, str) or data_type not in (
            "text",
            "integer",
            "float",
        ):
            data_type = "text"

        # REQ-0948: duplicate codelist id or name.
        if isinstance(identifier, str):
            if identifier in seen_ids:
                diagnostics.append(
                    _diagnostic(
                        "duplicate_define_identifier",
                        (path,),
                        "REQ-0948",
                        {"codelist": identifier, "field": "id"},
                    )
                )
            seen_ids.add(identifier)
        if isinstance(name, str):
            if name in seen_names:
                diagnostics.append(
                    _diagnostic(
                        "duplicate_define_identifier",
                        (path,),
                        "REQ-0948",
                        {"codelist": identifier, "field": "name", "name": name},
                    )
                )
            seen_names.add(name)

        # REQ-0958: a standard reference names a declared CT standard.
        standard = codelist.get("standard")
        if standard is not None:
            referenced = standards.get(standard) if isinstance(standard, str) else None
            if referenced is None or referenced.get("type") != "CT":
                diagnostics.append(
                    _diagnostic(
                        "codelist_shape_invalid",
                        (f"{path}.standard",)
                        if isinstance(identifier, str)
                        else (path,),
                        "REQ-0958",
                        {"codelist": identifier, "standard": standard},
                    )
                )

        # REQ-0947: either items or external, never both and never neither.
        has_items = codelist.get("items") is not None
        has_external = codelist.get("external") is not None
        if has_items == has_external:
            diagnostics.append(
                _diagnostic(
                    "codelist_shape_invalid",
                    (path,),
                    "REQ-0947",
                    {"codelist": identifier},
                )
            )
            continue

        if has_external:
            continue

        items = codelist.get("items") or []
        if not isinstance(items, list):
            continue

        # REQ-0950: every coded value is of the codelist's data_type.
        for index, item in enumerate(items):
            if not isinstance(item, dict) or "value" not in item:
                continue
            if not _value_matches_data_type(item["value"], data_type):
                diagnostics.append(
                    _diagnostic(
                        "codelist_shape_invalid",
                        (f"{path}.items[{index}]",),
                        "REQ-0950",
                        {
                            "codelist": identifier,
                            "value": item["value"],
                            "data_type": data_type,
                        },
                    )
                )

        # REQ-0949: coded values are unique under REQ-0935 equality.
        seen: set[tuple[str, object]] = set()
        for index, item in enumerate(items):
            if not isinstance(item, dict) or "value" not in item:
                continue
            key = _equality_key(item["value"], data_type)
            if key in seen:
                diagnostics.append(
                    _diagnostic(
                        "codelist_duplicate_value",
                        (f"{path}.items[{index}]",),
                        "REQ-0949",
                        {"codelist": identifier, "value": item["value"]},
                    )
                )
            seen.add(key)

        # REQ-0932/REQ-0933: decode and rank are uniform within one codelist.
        decodes = [
            isinstance(item, dict) and item.get("decode") is not None for item in items
        ]
        ranks = [
            isinstance(item, dict) and item.get("rank") is not None for item in items
        ]
        if any(decodes) and not all(decodes):
            diagnostics.append(
                _diagnostic(
                    "codelist_partial_item_field",
                    (path,),
                    "REQ-0951",
                    {"codelist": identifier, "field": "decode"},
                )
            )
        if any(ranks) and not all(ranks):
            diagnostics.append(
                _diagnostic(
                    "codelist_partial_item_field",
                    (path,),
                    "REQ-0951",
                    {"codelist": identifier, "field": "rank"},
                )
            )

        # REQ-0934: extended only on an extensible codelist with a standard.
        extensible = codelist.get("extensible") is True
        has_standard = codelist.get("standard") is not None
        for index, item in enumerate(items):
            if (
                isinstance(item, dict)
                and item.get("extended") is True
                and not (extensible and has_standard)
            ):
                diagnostics.append(
                    _diagnostic(
                        "codelist_extension_not_admitted",
                        (f"{path}.items[{index}]",),
                        "REQ-0952",
                        {"codelist": identifier},
                    )
                )

    return diagnostics


def _allowed_values(column: Any) -> list[Any] | None:
    """Collect every allowed_values entry on one column, if any."""
    collected: list[Any] = []
    found = False
    for verification in column.verifications or []:
        root = getattr(verification, "root", None)
        if not isinstance(root, dict) or len(root) != 1:
            continue
        operation, payload = next(iter(root.items()))
        if operation != "allowed_values" or not isinstance(payload, dict):
            continue
        values = payload.get("values")
        if isinstance(values, list):
            found = True
            collected.extend(values)
    return collected if found else None


def _binding_diagnostics_for_column(
    column: Any,
    field_path: str,
    codelist_id: str,
    codelists: dict[str, dict[str, Any]],
) -> list[ValidationDiagnostic]:
    """Check one codelist binding against its declaration.

    Covers REQ-0953 (unknown), REQ-0954 (type agreement), and REQ-0955
    (agreement with allowed_values). Callers pass column-level paths for
    shared declarations and row-level paths for per-value overrides.
    """
    diagnostics: list[ValidationDiagnostic] = []
    name = column.name if hasattr(column, "name") else column
    column_name = name if isinstance(name, str) else str(name)
    codelist = codelists.get(codelist_id)
    if codelist is None:
        diagnostics.append(
            _diagnostic(
                "unknown_codelist",
                (field_path,),
                "REQ-0953",
                {"column": column_name, "codelist": codelist_id},
            )
        )
        return diagnostics

    data_type = codelist.get("data_type", "text")
    if not isinstance(data_type, str):
        data_type = "text"
    expected_column = _CODELIST_TO_COLUMN.get(data_type, "str")
    actual_column = column.type if hasattr(column, "type") else None
    if actual_column is not None and actual_column != expected_column:
        diagnostics.append(
            _diagnostic(
                "codelist_type_mismatch",
                (field_path,),
                "REQ-0954",
                {
                    "column": column_name,
                    "codelist": codelist_id,
                    "codelist_data_type": data_type,
                    "column_type": actual_column,
                },
            )
        )

    # REQ-0942/REQ-0944: a non-extensible items list beside allowed_values
    # must hold exactly the same values. Extensible and external bindings
    # leave the verification to stand alone.
    if hasattr(column, "verifications"):
        allowed = _allowed_values(column)
        items = codelist.get("items")
        extensible = codelist.get("extensible") is True
        if allowed is not None and isinstance(items, list) and not extensible:
            bound = {
                _equality_key(value, data_type) for value in _item_values(codelist)
            }
            declared = {_equality_key(value, data_type) for value in allowed}
            if bound != declared:
                diagnostics.append(
                    _diagnostic(
                        "codelist_values_conflict",
                        (field_path,),
                        "REQ-0955",
                        {"column": column_name, "codelist": codelist_id},
                    )
                )

    return diagnostics


def validate_spec_codelist_bindings(
    specification: Specification,
    define: dict[str, Any],
) -> list[ValidationDiagnostic]:
    """Validate one specification's codelist bindings against a study document."""
    diagnostics: list[ValidationDiagnostic] = []
    codelists = _codelists_by_id(define)
    columns_by_name = {column.name: column for column in specification.columns}

    for column in specification.columns:
        submission = column.submission
        if submission is None or submission.codelist is None:
            continue
        field_path = f"columns.{column.name}.submission"
        diagnostics.extend(
            _binding_diagnostics_for_column(
                column, field_path, submission.codelist, codelists
            )
        )

    for row in specification.rows or []:
        if not row.submission:
            continue
        row_path = f"rows.{row.id}"
        for column_name, entry in row.submission.items():
            if entry.codelist is None:
                continue
            column = columns_by_name.get(column_name)
            if column is None:
                continue
            field_path = f"{row_path}.submission.{column_name}"
            diagnostics.extend(
                _binding_diagnostics_for_column(
                    column, field_path, entry.codelist, codelists
                )
            )

    return diagnostics


def validate_study_terminology(
    define: dict[str, Any],
    specifications: dict[str, Specification],
) -> list[ValidationDiagnostic]:
    """Validate terminology across one study document and its datasets.

    ``specifications`` maps the study document's dataset ``id`` to the
    loaded specification that entry represents. Checks define-alone
    codelist semantics, every binding each specification declares, and
    that every declared codelist is used (REQ-0956).
    """
    diagnostics: list[ValidationDiagnostic] = []
    diagnostics.extend(validate_define_codelists(define))

    used: set[str] = set()
    for dataset_id, specification in specifications.items():
        del dataset_id
        for column in specification.columns:
            submission = column.submission
            if submission is not None and submission.codelist is not None:
                used.add(submission.codelist)
        for row in specification.rows or []:
            for entry in (row.submission or {}).values():
                if entry.codelist is not None:
                    used.add(entry.codelist)
        diagnostics.extend(validate_spec_codelist_bindings(specification, define))

    # REQ-0946/REQ-0956: every declared codelist is named by a binding.
    for codelist in _codelists(define):
        identifier = codelist.get("id")
        if isinstance(identifier, str) and identifier not in used:
            diagnostics.append(
                _diagnostic(
                    "unreferenced_codelist",
                    (f"codelists.{identifier}",),
                    "REQ-0956",
                    {"codelist": identifier},
                )
            )

    return diagnostics


class LoadedStudyDocument:
    """A validated study document with its dataset specifications."""

    def __init__(
        self,
        document: dict[str, Any],
        written_path: Path,
        specifications: dict[str, Specification],
    ) -> None:
        self.document = document
        self.written_path = written_path
        self.specifications = specifications


def load_study_document(
    define_path: str | Path,
    schema_root: str | Path,
) -> LoadedStudyDocument:
    """Load one study document and validate its terminology composition.

    Reads ``define.yaml`` against ``schema_define.yaml``, loads every
    dataset entry's specification with the specification loader, then
    checks controlled terminology (REQ-0926..REQ-0958) across the two.
    Raises SpecificationError when any stage fails.
    """
    from yamaa.specification.loader import load_specification

    written_path = Path(define_path)
    origin_path = written_path.resolve()
    bundle: SchemaBundle = load_schema_bundle(
        schema_root, entry_name="schema_define.yaml", root_class="define_class"
    )
    raw = read_yaml_document(origin_path)
    shape = validate_document(raw, bundle, "define_class")
    if shape:
        raise SpecificationError(shape)
    assert isinstance(raw, dict)
    normalized = normalize_document(raw, bundle, "define_class")
    assert isinstance(normalized, dict)
    define: dict[str, Any] = dict(normalized)

    specifications: dict[str, Specification] = {}
    for entry in define.get("datasets") or []:
        if not isinstance(entry, dict):
            continue
        dataset_id = entry.get("id")
        spec_ref = entry.get("spec")
        if not isinstance(dataset_id, str) or not isinstance(spec_ref, str):
            continue
        spec_path = (origin_path.parent / spec_ref).resolve()
        loaded = load_specification(spec_path, schema_root)
        specifications[dataset_id] = loaded.specification

    diagnostics = validate_study_terminology(define, specifications)
    if diagnostics:
        raise SpecificationError(diagnostics)
    return LoadedStudyDocument(
        document=define,
        written_path=written_path,
        specifications=specifications,
    )


__all__ = [
    "LoadedStudyDocument",
    "load_study_document",
    "validate_define_codelists",
    "validate_spec_codelist_bindings",
    "validate_study_terminology",
]
