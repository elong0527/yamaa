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
    diagnostics = validate_specification(document, bundle)
    if diagnostics:
        raise SpecificationError(diagnostics)

    assert isinstance(document, dict)
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

    return LoadedSpecification(
        specification=specification,
        written_path=written_path,
        origin_path=origin_path,
        schema_path=bundle.path,
    )
