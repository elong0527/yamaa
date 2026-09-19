"""Public specification loading helper."""

from __future__ import annotations

from pathlib import Path

from pydantic import ValidationError

from yamaa.specification._yaml import read_yaml_document
from yamaa.specification.diagnostics import (
    SpecificationError,
    ValidationDiagnostic,
)
from yamaa.specification.models import LoadedSpecification, Specification
from yamaa.specification.schema import (
    load_schema_bundle,
    normalize_specification,
    validate_specification,
)
from yamaa.specification.submission import (
    has_value_level_metadata,
    validate_value_level_metadata,
    warn_value_level_define_deferred,
)


def _pydantic_path(location: tuple[object, ...]) -> str:
    return ".".join(str(member) for member in location) or "$"


def load_specification(
    entry_path: str | Path,
    schema_root: str | Path,
) -> LoadedSpecification:
    """Read, validate, normalize, and model one YAMAA specification."""
    written_path = Path(entry_path)
    origin_path = written_path.resolve()
    bundle = load_schema_bundle(schema_root)
    document = read_yaml_document(origin_path)
    if isinstance(document, dict) and "parents" in document:
        # Imported lazily so the schema interpreter remains usable on its own.
        from yamaa.schema.inheritance import resolve_specification

        resolved = resolve_specification(
            origin_path,
            bundle,
            entry_document=document,
        )
        submission_diagnostics = validate_value_level_metadata(resolved.document)
        if submission_diagnostics:
            raise SpecificationError(submission_diagnostics)
        if has_value_level_metadata(resolved.document):
            warn_value_level_define_deferred()
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
    submission_diagnostics = validate_value_level_metadata(document)
    if submission_diagnostics:
        raise SpecificationError(submission_diagnostics)

    normalized = normalize_specification(document, bundle)
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

    if has_value_level_metadata(document):
        warn_value_level_define_deferred()

    return LoadedSpecification(
        specification=specification,
        written_path=written_path,
        origin_path=origin_path,
        schema_path=bundle.path,
    )
