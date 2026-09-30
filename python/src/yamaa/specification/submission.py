"""Column-level submission metadata semantics (REQ-0855..REQ-0925, REQ-0907).

Validates what a specification alone can decide without a study document:
shape-independent field combinations, length binding, origin requirements,
and derivation-graph refutations. Family-dependent requirements (REQ-0908)
-- core vocabularies, mandatory derivation, origin type/source pairs,
role/domain for `adam`, document references, and standard families -- run
when Define-XML composes the specification into a document and are not
checked here.
"""

from __future__ import annotations

from typing import Any

from .diagnostics import ValidationDiagnostic
from .models import HandledExpression, Specification


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


# REQ-0868: default submission type per declared column type.
_DEFAULT_DATA_TYPE: dict[str, str] = {
    "str": "text",
    "int": "integer",
    "float": "float",
    "date": "date",
    "datetime": "datetime",
}

# REQ-0868: `str` admits these in addition to its default.
_STR_ADMITS: frozenset[str] = frozenset(
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
)

# REQ-0872: length is required for these resolved submission types.
_LENGTH_TYPES: frozenset[str] = frozenset({"text", "integer", "float"})

# REQ-0858: governed keys the free-form `metadata` map must not carry.
_GOVERNED_DATASET_KEYS: frozenset[str] = frozenset(
    {
        "label",
        "class",
        "subclass",
        "structure",
        "repeating",
        "reference_data",
        "domain",
        "comment",
    }
)
_GOVERNED_COLUMN_KEYS: frozenset[str] = frozenset(
    {
        "core",
        "mandatory",
        "role",
        "data_type",
        "length",
        "significant_digits",
        "display_format",
        "codelist",
        "inventory_vocabulary",
        "origin",
        "method",
        "comment",
    }
)

# REQ-0897..REQ-0899 applied to one derivation, shared with value_metadata.
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


def _derivation_kind(expression: HandledExpression) -> str:
    operation = expression.value.operation
    if operation == "literal":
        return "literal"
    if operation in ("source", "odm"):
        return "source"
    return "computed"


def _verification_root(verification: Any) -> Any:
    # Column verifications are `Expression` (`.root`); handled expressions
    # carry `.value.root`. Accept both so the same helper serves each.
    value = getattr(verification, "value", None)
    if value is not None and hasattr(value, "root"):
        return value.root
    return getattr(verification, "root", None)


def _max_length_values(verifications: list[Any] | None) -> list[Any]:
    values: list[Any] = []
    for verification in verifications or []:
        root = _verification_root(verification)
        if not isinstance(root, dict) or len(root) != 1:
            continue
        operation, payload = next(iter(root.items()))
        if operation != "max_length" or not isinstance(payload, dict):
            continue
        if "max" in payload:
            values.append(payload["max"])
    return values


def _has_verification(verifications: list[Any] | None, operation: str) -> bool:
    for verification in verifications or []:
        root = _verification_root(verification)
        if isinstance(root, dict) and len(root) == 1 and operation in root:
            return True
    return False


def _resolve_data_type(column_type: str, declared: str | None) -> str:
    if declared is not None:
        return declared
    return _DEFAULT_DATA_TYPE[column_type]


def _admitted_data_types(column_type: str) -> frozenset[str]:
    if column_type == "str":
        return _STR_ADMITS
    return frozenset({_DEFAULT_DATA_TYPE[column_type]})


def validate_submission_metadata(
    specification: Specification,
) -> list[ValidationDiagnostic]:
    """Validate family-independent submission semantics. See REQ-0907."""
    diagnostics: list[ValidationDiagnostic] = []
    output_columns = set(specification.output.columns)
    columns_by_name = {column.name: column for column in specification.columns}
    has_root_submission = specification.submission is not None

    # REQ-0858: the free-form map must not carry a governed key.
    if specification.metadata:
        for key in specification.metadata:
            if key in _GOVERNED_DATASET_KEYS:
                diagnostics.append(
                    _diagnostic(
                        "reserved_metadata_key",
                        (f"metadata.{key}",),
                        "REQ-0910",
                        {"key": key, "level": "root"},
                    )
                )

    # REQ-0925: reference data never repeats.
    if specification.submission is not None:
        dataset = specification.submission
        if dataset.reference_data and dataset.repeating:
            diagnostics.append(
                _diagnostic(
                    "reference_data_repeating_conflict",
                    ("submission",),
                    "REQ-0925",
                    {"reference_data": True, "repeating": True},
                )
            )

    # Collect row derivations per column for REQ-0900.
    row_derivations: dict[str, list[HandledExpression]] = {}
    for row in specification.rows or []:
        for name, derivation in (row.derivations or {}).items():
            row_derivations.setdefault(name, []).append(derivation)

    for column in specification.columns:
        name = column.name
        submission = column.submission
        column_path = f"columns.{name}"

        if column.metadata:
            for key in column.metadata:
                if key in _GOVERNED_COLUMN_KEYS:
                    diagnostics.append(
                        _diagnostic(
                            "reserved_metadata_key",
                            (f"{column_path}.metadata.{key}",),
                            "REQ-0910",
                            {"key": key, "level": "column", "column": name},
                        )
                    )

        if submission is None:
            continue

        # REQ-0856 / REQ-0909: submission only for output columns.
        if name not in output_columns:
            diagnostics.append(
                _diagnostic(
                    "submission_not_output_column",
                    (f"{column_path}.submission",),
                    "REQ-0909",
                    {"column": name},
                )
            )

        field_path = f"{column_path}.submission"
        resolved = _resolve_data_type(column.type, submission.data_type)

        # REQ-0868 / REQ-0914: declared submission type admitted by column type.
        if resolved not in _admitted_data_types(column.type):
            diagnostics.append(
                _diagnostic(
                    "submission_data_type_not_admitted",
                    (field_path,),
                    "REQ-0914",
                    {
                        "column": name,
                        "declared_type": column.type,
                        "data_type": resolved,
                    },
                )
            )

        # REQ-0871 / REQ-0911: length positive, significant_digits non-negative.
        if submission.length is not None and not (
            isinstance(submission.length, int)
            and not isinstance(submission.length, bool)
            and submission.length > 0
        ):
            diagnostics.append(
                _diagnostic(
                    "submission_length_invalid",
                    (f"{field_path}.length",),
                    "REQ-0911",
                    {"column": name, "length": submission.length},
                )
            )
        if submission.significant_digits is not None and not (
            isinstance(submission.significant_digits, int)
            and not isinstance(submission.significant_digits, bool)
            and submission.significant_digits >= 0
        ):
            diagnostics.append(
                _diagnostic(
                    "submission_length_invalid",
                    (f"{field_path}.significant_digits",),
                    "REQ-0911",
                    {
                        "column": name,
                        "significant_digits": submission.significant_digits,
                    },
                )
            )

        max_values = _max_length_values(column.verifications)

        # REQ-0875 / REQ-0913: declared length agrees with max_length.
        if submission.length is not None and max_values:
            for max_value in max_values:
                if max_value != submission.length:
                    diagnostics.append(
                        _diagnostic(
                            "declared_length_conflict",
                            (field_path,),
                            "REQ-0913",
                            {
                                "column": name,
                                "length": submission.length,
                                "max_length": max_value,
                            },
                        )
                    )
                    break

        # REQ-0872 / REQ-0873 / REQ-0912: required and prohibited combinations.
        # Scoped to specs opting into submission (root.submission present);
        # see issue #1514. A spec without root submission may carry partial
        # column metadata (e.g. value-level fixtures) that composition
        # completes.
        if has_root_submission:
            length_required = resolved in _LENGTH_TYPES and not max_values
            if length_required and submission.length is None:
                diagnostics.append(
                    _diagnostic(
                        "submission_length_missing",
                        (field_path,),
                        "REQ-0912",
                        {"column": name, "data_type": resolved},
                    )
                )
            # Length prohibited otherwise (REQ-0872); when a max_length
            # verification derives it on a str column, absence is the
            # derived form and presence must agree (checked above).
            if (
                not length_required
                and resolved not in _LENGTH_TYPES
                and submission.length is not None
                and not max_values
            ):
                diagnostics.append(
                    _diagnostic(
                        "submission_length_not_applicable",
                        (field_path,),
                        "REQ-0912",
                        {"column": name, "data_type": resolved},
                    )
                )
            float_type = resolved == "float"
            if float_type and submission.significant_digits is None:
                diagnostics.append(
                    _diagnostic(
                        "submission_length_missing",
                        (field_path,),
                        "REQ-0912",
                        {"column": name, "field": "significant_digits"},
                    )
                )
            if not float_type and submission.significant_digits is not None:
                diagnostics.append(
                    _diagnostic(
                        "submission_length_not_applicable",
                        (field_path,),
                        "REQ-0912",
                        {"column": name, "field": "significant_digits"},
                    )
                )

            # REQ-0882/REQ-0884: core required whenever a spec opts into
            # submission. Family-specific vocabularies (Exp for adam, Cond
            # for sdtm/send) and mandatory derivation are REQ-0908
            # composition checks.
            if submission.core is None:
                diagnostics.append(
                    _diagnostic(
                        "core_mandatory_conflict",
                        (field_path,),
                        "REQ-0915",
                        {"column": name, "reason": "core_missing"},
                    )
                )

        # REQ-0885 / REQ-0916: mandatory:true is proven by not_missing or keys.
        if (
            submission.mandatory is True
            and not _has_verification(column.verifications, "not_missing")
            and name not in (specification.keys or [])
        ):
            diagnostics.append(
                _diagnostic(
                    "mandatory_not_enforced",
                    (field_path,),
                    "REQ-0916",
                    {"column": name},
                )
            )

        # REQ-0887 / REQ-0918: origin required for every column with metadata.
        origin = submission.origin
        if origin is None:
            diagnostics.append(
                _diagnostic(
                    "origin_missing",
                    (field_path,),
                    "REQ-0918",
                    {"column": name},
                )
            )
            continue

        # REQ-0892 / REQ-0920: description required for incomplete origins.
        if (
            origin.type in ("Predecessor", "Other", "Not Available")
            and not origin.description
        ):
            diagnostics.append(
                _diagnostic(
                    "origin_description_missing",
                    (field_path,),
                    "REQ-0920",
                    {"column": name, "origin_type": origin.type},
                )
            )

        # REQ-0895 / REQ-0923: a Derived origin claims an algorithm.
        if origin.type == "Derived" and submission.method is None:
            diagnostics.append(
                _diagnostic(
                    "method_missing",
                    (field_path,),
                    "REQ-0923",
                    {"column": name},
                )
            )

        # REQ-0897..REQ-0900 / REQ-0922: the graph refutes contradicting origins.
        admitted = _admitted_for_column(column, row_derivations.get(name, []))
        if admitted is not None:
            if not admitted:
                diagnostics.append(
                    _diagnostic(
                        "origin_contradicts_derivation",
                        (field_path,),
                        "REQ-0922",
                        {"column": name, "reason": "row_derivations_disagree"},
                    )
                )
            elif origin.type not in admitted:
                diagnostics.append(
                    _diagnostic(
                        "origin_contradicts_derivation",
                        (field_path,),
                        "REQ-0922",
                        {
                            "column": name,
                            "origin_type": origin.type,
                            "admitted": sorted(admitted),
                        },
                    )
                )

    # Row-level origin presence mirrors REQ-0887 per value; graph refutation
    # for values stays in value_metadata (REQ-1168).
    for row in specification.rows or []:
        if not row.submission:
            continue
        row_path = f"rows.{row.id}"
        for column_name, entry in row.submission.items():
            if column_name not in columns_by_name:
                continue
            if entry.origin is None:
                diagnostics.append(
                    _diagnostic(
                        "origin_missing",
                        (f"{row_path}.submission.{column_name}",),
                        "REQ-0918",
                        {"column": column_name, "row": row.id},
                    )
                )
                continue
            if (
                entry.origin.type in ("Predecessor", "Other", "Not Available")
                and not entry.origin.description
            ):
                diagnostics.append(
                    _diagnostic(
                        "origin_description_missing",
                        (f"{row_path}.submission.{column_name}",),
                        "REQ-0920",
                        {
                            "column": column_name,
                            "row": row.id,
                            "origin_type": entry.origin.type,
                        },
                    )
                )
            if entry.origin.type == "Derived":
                column = columns_by_name[column_name]
                effective_method = entry.method if entry.method is not None else None
                # Inherit the shared declaration when the value leaves it absent.
                if effective_method is None and column.submission is not None:
                    effective_method = column.submission.method
                if effective_method is None:
                    diagnostics.append(
                        _diagnostic(
                            "method_missing",
                            (f"{row_path}.submission.{column_name}",),
                            "REQ-0923",
                            {"column": column_name, "row": row.id},
                        )
                    )

    return diagnostics


def _admitted_for_column(
    column: Any,
    row_derivations: list[HandledExpression],
) -> frozenset[str] | None:
    """Return origins the derivation graph admits, or None when unknown.

    REQ-0900: a row-derived column keeps only the types all entries admit.
    """
    if row_derivations:
        admitted: frozenset[str] | None = None
        for derivation in row_derivations:
            kinds = _ADMITTED_ORIGINS[_derivation_kind(derivation)]
            admitted = kinds if admitted is None else admitted & kinds
        return admitted if admitted is not None else frozenset()
    derivation = getattr(column, "derivation", None)
    if derivation is None:
        return None
    return _ADMITTED_ORIGINS[_derivation_kind(derivation)]


__all__ = ["validate_submission_metadata"]
