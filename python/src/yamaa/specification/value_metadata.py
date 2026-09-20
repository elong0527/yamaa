"""Value-level submission metadata declared on row templates.

A ``rows`` entry may carry a ``submission`` map keyed by column name. Each
entry describes the submission metadata for one value of the row's
discriminator (``<DOMAIN>TESTCD`` for findings domains): its codelist,
origin, and any other column-level submission field, overriding the shared
column-level declaration for that value. Validation is data-independent: it
checks the declarations against each other and against the row derivations,
never against input data.
"""

from __future__ import annotations

from typing import Any

from .diagnostics import ValidationDiagnostic
from .models import HandledExpression, Specification, SubmissionColumn


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


def _derivation_kind(expression: HandledExpression) -> str:
    """Classify a row entry's derivation the way REQ-0897..REQ-0899 do."""
    operation = expression.value.operation
    if operation == "literal":
        return "literal"
    if operation == "source":
        return "source"
    return "computed"


# REQ-0897..REQ-0899 applied to one row entry's derivation.
_ADMITTED_ORIGINS: dict[str, frozenset[str]] = {
    "computed": frozenset({"Derived", "Assigned", "Other"}),
    "literal": frozenset({"Assigned", "Protocol", "Other"}),
    "source": frozenset(
        {
            "Collected",
            "Assigned",
            "Protocol",
            "Predecessor",
            "Other",
            "Not Available",
        }
    ),
}


def validate_value_metadata(
    specification: Specification,
) -> list[ValidationDiagnostic]:
    """Validate row-level ``submission`` maps. See REQ-1162..REQ-1168."""
    diagnostics: list[ValidationDiagnostic] = []
    rows = specification.rows or []
    if not any(row.submission for row in rows):
        return diagnostics

    output_columns = set(specification.output.columns)
    discriminator = f"{specification.domain}TESTCD"
    has_discriminator = discriminator in {c.name for c in specification.columns}
    column_derivations = {
        column.name: column.derivation for column in specification.columns
    }

    # (column, testcd) -> first submission seen, for conflict detection.
    seen: dict[tuple[str, str], SubmissionColumn] = {}

    for row in rows:
        if not row.submission:
            continue
        row_path = f"rows.{row.id}"
        if not has_discriminator:
            diagnostics.append(
                _diagnostic(
                    "value_metadata_requires_testcd",
                    (f"{row_path}.submission",),
                    "REQ-1163",
                    {"domain": specification.domain},
                )
            )
            continue

        testcd: str | None = None
        declaration = row.derivations.get(discriminator)
        if declaration is not None:
            payload = declaration.value.root.get("literal")
            if isinstance(payload, str):
                testcd = payload
        if testcd is None:
            diagnostics.append(
                _diagnostic(
                    "value_metadata_requires_literal_testcd",
                    (f"{row_path}.submission",),
                    "REQ-1164",
                    {"discriminator": discriminator},
                )
            )
            continue

        for column_name, submission in row.submission.items():
            field_path = f"{row_path}.submission.{column_name}"
            effective = row.derivations.get(column_name) or column_derivations.get(
                column_name
            )
            if effective is None:
                diagnostics.append(
                    _diagnostic(
                        "value_metadata_unknown_column",
                        (field_path,),
                        "REQ-1165",
                        {"column": column_name, "row": row.id},
                    )
                )
                continue
            if column_name not in output_columns:
                diagnostics.append(
                    _diagnostic(
                        "value_metadata_not_output_column",
                        (field_path,),
                        "REQ-1166",
                        {"column": column_name},
                    )
                )

            key = (column_name, testcd)
            first = seen.get(key)
            if first is None:
                seen[key] = submission
            elif first != submission:
                diagnostics.append(
                    _diagnostic(
                        "conflicting_value_metadata",
                        (field_path,),
                        "REQ-1167",
                        {"column": column_name, "testcd": testcd},
                    )
                )
                continue

            kind = _derivation_kind(effective)
            if submission.origin.type not in _ADMITTED_ORIGINS[kind]:
                diagnostics.append(
                    _diagnostic(
                        "refuted_value_origin",
                        (field_path,),
                        "REQ-1168",
                        {
                            "column": column_name,
                            "testcd": testcd,
                            "derivation_kind": kind,
                            "origin_type": submission.origin.type,
                        },
                    )
                )

    return diagnostics
