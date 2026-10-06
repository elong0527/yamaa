"""Public specification loading helper."""

from __future__ import annotations

from pathlib import Path

from pydantic import ValidationError

from yamaa.specification.diagnostics import (
    SpecificationError,
    ValidationDiagnostic,
)
from yamaa.specification.models import LoadedSpecification, Specification
from yamaa.specification.schema import (
    SchemaBundle,
    load_schema_bundle,
    normalize_specification,
    read_bundle_document,
    validate_specification,
)
from yamaa.specification.submission import validate_submission_metadata
from yamaa.specification.value_metadata import validate_value_metadata


def _pydantic_path(location: tuple[object, ...]) -> str:
    return ".".join(str(member) for member in location) or "$"


def load_specification(
    entry_path: str | Path,
    schema_root: str | Path,
) -> LoadedSpecification:
    """Read, validate, normalize, and model one yamaa specification."""
    return load_specification_with_bundle(entry_path, load_schema_bundle(schema_root))


def load_specification_with_bundle(
    entry_path: str | Path,
    bundle: SchemaBundle,
) -> LoadedSpecification:
    """Use one captured schema service throughout loading, inheritance and windows."""
    written_path = Path(entry_path)
    origin_path = written_path.resolve()
    document = read_bundle_document(origin_path, bundle)
    if isinstance(document, dict) and "parents" in document:
        # Imported lazily so the schema interpreter remains usable on its own.
        from yamaa.schema.inheritance import resolve_specification

        resolved = resolve_specification(
            origin_path,
            bundle,
            entry_document=document,
        )
        # resolve_specification already ran submission and value-metadata
        # validation; reaching here means both passed.
        return LoadedSpecification(
            specification=resolved.specification,
            written_path=written_path,
            origin_path=origin_path,
            schema_path=bundle.path,
        )
    diagnostics = validate_specification(document, bundle)
    if diagnostics:
        raise SpecificationError(diagnostics)

    assert isinstance(document, dict)
    normalized = normalize_specification(document, bundle)
    from yamaa.schema.windows import expand_named_windows

    normalized = expand_named_windows(normalized, bundle)
    try:
        specification = Specification.model_validate(normalized, strict=True)
    except ValidationError as error:
        diagnostics = [
            ValidationDiagnostic(
                condition="model_contract_mismatch",
                spec_paths=(_pydantic_path(item["loc"]),),
                context={"reason": item["msg"]},
            )
            for item in error.errors(include_url=False, include_input=False)
        ]
        raise SpecificationError(diagnostics) from error

    # Column-level (REQ-0907) and row-level (REQ-1162..REQ-1168) checks.
    submission_diagnostics = validate_submission_metadata(specification)
    value_diagnostics = validate_value_metadata(specification)
    diagnostics = [*submission_diagnostics, *value_diagnostics]
    if diagnostics:
        raise SpecificationError(diagnostics)

    return LoadedSpecification(
        specification=specification,
        written_path=written_path,
        origin_path=origin_path,
        schema_path=bundle.path,
    )
