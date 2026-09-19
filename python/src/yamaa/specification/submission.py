"""R024 value-level submission metadata validation (issue #565).

`values` inside a column's `submission` block declares per-test-code
submission metadata for findings datasets. These checks run on the
normalized specification document after structural schema validation and
before the strict Pydantic model contract, so a rejected declaration
reports its own condition instead of a generic contract mismatch.
"""

from __future__ import annotations

import warnings
from typing import Any

from yamaa.specification.diagnostics import ValidationDiagnostic

# R024-14: each declared column type admits a closed set of submission types.
_ADMITTED_DATA_TYPES: dict[str, frozenset[str]] = {
    "str": frozenset(
        {
            "text",
            "date",
            "datetime",
            "time",
            "partialDate",
            "partialTime",
            "partialDatetime",
            "incompleteDate",
            "incompleteTime",
            "incompleteDatetime",
            "durationDatetime",
            "intervalDatetime",
            "URI",
        }
    ),
    "int": frozenset({"integer"}),
    "float": frozenset({"float"}),
    "date": frozenset({"date"}),
    "datetime": frozenset({"datetime"}),
}

# R024-43 through R024-45: origin types the derivation graph admits.
_ORIGINS_SOURCE_COPY = frozenset(
    {"Assigned", "Collected", "Protocol", "Predecessor", "Not Available", "Other"}
)
_ORIGINS_LITERAL = frozenset({"Assigned", "Protocol", "Other"})
_ORIGINS_COMPUTED = frozenset({"Derived", "Assigned", "Other"})


def _diagnostic(
    path: str,
    condition: str,
    requirement: str,
    context: dict[str, Any],
) -> ValidationDiagnostic:
    return ValidationDiagnostic(
        condition=condition,
        spec_paths=(path,),
        requirement=requirement,
        context=context,
    )


def _inner_operation(derivation: Any) -> str | None:
    """Name the single expression operation a derivation performs.

    Returns None when there is no derivation or its shape is not a single
    operation, in which case no graph fact is claimed.
    """
    if not isinstance(derivation, dict):
        return None
    inner = derivation.get("value", derivation)
    if not isinstance(inner, dict) or len(inner) != 1:
        return None
    return next(iter(inner))


def _admitted_origins(derivation: Any) -> frozenset[str] | None:
    """Origin types the derivation graph admits for one derivation."""
    operation = _inner_operation(derivation)
    if operation is None:
        return None
    if operation == "source":
        return _ORIGINS_SOURCE_COPY
    if operation == "literal":
        return _ORIGINS_LITERAL
    return _ORIGINS_COMPUTED


def _column_admitted_origins(
    column: dict[str, Any], rows: list[Any]
) -> frozenset[str] | None:
    """Admitted origin types for a column, intersecting row entries (R024-46)."""
    derivation = column.get("derivation")
    if derivation is not None:
        return _admitted_origins(derivation)
    admitted: frozenset[str] | None = None
    for row in rows:
        if not isinstance(row, dict):
            continue
        derivations = row.get("derivations")
        if not isinstance(derivations, dict):
            continue
        if column.get("name") not in derivations:
            continue
        entry_admitted = _admitted_origins(derivations[column["name"]])
        if entry_admitted is None:
            return None
        admitted = entry_admitted if admitted is None else admitted & entry_admitted
        if not admitted:
            break
    return admitted


def _check_entry(
    column: dict[str, Any],
    entry: dict[str, Any],
    index: int,
    admitted_origins: frozenset[str] | None,
    diagnostics: list[ValidationDiagnostic],
) -> None:
    name = column["name"]
    base = f"columns.{name}.submission.values[{index}]"
    column_submission = column.get("submission") or {}

    data_type = entry.get("data_type")
    if data_type is not None:
        admitted_types = _ADMITTED_DATA_TYPES.get(column.get("type"), frozenset())
        if data_type not in admitted_types:
            diagnostics.append(
                _diagnostic(
                    f"{base}.data_type",
                    "submission_data_type_not_admitted",
                    "R024-79",
                    {
                        "column": name,
                        "declared_type": column.get("type"),
                        "submission_type": data_type,
                        "testcd": entry.get("testcd"),
                    },
                )
            )

    entry_origin = entry.get("origin") or column_submission.get("origin")
    entry_method = entry.get("method", column_submission.get("method"))
    origin_type = entry_origin.get("type") if isinstance(entry_origin, dict) else None
    if origin_type == "Derived" and entry_method is None:
        diagnostics.append(
            _diagnostic(
                f"{base}.origin",
                "method_missing",
                "R024-79",
                {"column": name, "testcd": entry.get("testcd")},
            )
        )
    if (
        origin_type is not None
        and admitted_origins is not None
        and origin_type not in admitted_origins
    ):
        diagnostics.append(
            _diagnostic(
                f"{base}.origin",
                "origin_contradicts_derivation",
                "R024-79",
                {
                    "column": name,
                    "testcd": entry.get("testcd"),
                    "declared_origin": origin_type,
                    "admitted_origins": sorted(admitted_origins),
                },
            )
        )


def validate_value_level_metadata(document: Any) -> list[ValidationDiagnostic]:
    """Validate every column `submission.values` declaration (R024-76..81)."""
    diagnostics: list[ValidationDiagnostic] = []
    if not isinstance(document, dict):
        return diagnostics
    output = document.get("output")
    output_columns = output.get("columns") if isinstance(output, dict) else []
    output_set = set(output_columns) if isinstance(output_columns, list) else set()
    dataset_submission = document.get("submission")
    dataset_submission = (
        dataset_submission if isinstance(dataset_submission, dict) else {}
    )
    dataset_class = dataset_submission.get("class")
    domain = dataset_submission.get("domain") or document.get("domain")
    rows = document.get("rows")
    rows = rows if isinstance(rows, list) else []

    columns = document.get("columns")
    if not isinstance(columns, list):
        return diagnostics
    for column in columns:
        if not isinstance(column, dict) or "name" not in column:
            continue
        name = column["name"]

        metadata = column.get("metadata")
        if isinstance(metadata, dict) and "values" in metadata:
            diagnostics.append(
                _diagnostic(
                    f"columns.{name}.metadata.values",
                    "reserved_metadata_key",
                    "R024-81",
                    {"column": name, "key": "values"},
                )
            )

        submission = column.get("submission")
        if not isinstance(submission, dict):
            continue
        values = submission.get("values")
        if values is None:
            continue
        values_path = f"columns.{name}.submission.values"
        if not isinstance(values, list):
            continue

        if name not in output_set:
            diagnostics.append(
                _diagnostic(
                    values_path,
                    "values_outside_output_columns",
                    "R024-76",
                    {"column": name},
                )
            )
            continue

        if dataset_class != "FINDINGS":
            diagnostics.append(
                _diagnostic(
                    values_path,
                    "values_requires_findings_class",
                    "R024-77",
                    {"column": name, "dataset_class": dataset_class},
                )
            )

        testcd_column = (
            f"{domain}TESTCD" if isinstance(domain, str) and domain else None
        )
        if testcd_column is not None and testcd_column not in output_set:
            diagnostics.append(
                _diagnostic(
                    values_path,
                    "value_testcd_column_missing",
                    "R024-80",
                    {"column": name, "testcd_column": testcd_column},
                )
            )

        admitted_origins = _column_admitted_origins(column, rows)
        seen: dict[Any, int] = {}
        for index, entry in enumerate(values):
            if not isinstance(entry, dict):
                continue
            testcd = entry.get("testcd")
            if testcd in seen:
                diagnostics.append(
                    _diagnostic(
                        f"{values_path}[{index}].testcd",
                        "duplicate_value_testcd",
                        "R024-78",
                        {"column": name, "testcd": testcd},
                    )
                )
            else:
                seen[testcd] = index
            _check_entry(column, entry, index, admitted_origins, diagnostics)

    return diagnostics


def has_value_level_metadata(document: Any) -> bool:
    """Whether any column declares `submission.values`."""
    if not isinstance(document, dict):
        return False
    columns = document.get("columns")
    if not isinstance(columns, list):
        return False
    return any(
        isinstance(column, dict)
        and isinstance(column.get("submission"), dict)
        and column["submission"].get("values") is not None
        for column in columns
    )


def warn_value_level_define_deferred() -> None:
    """Emit the R024-75 loud notice: value-level define.xml is deferred.

    The declaration is retained and valid; only the R026 document generation
    is not yet implemented. This warning fires at validation time so the gap
    is never a silent drop.
    """
    warnings.warn(
        "value-level submission metadata (submission.values) is declared but "
        "define.xml value-level generation (R026) is not yet implemented; the "
        "declaration is retained and no value-level document is emitted",
        UserWarning,
        stacklevel=3,
    )
