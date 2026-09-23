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
from yamaa.specification.value_metadata import validate_value_metadata


def _pydantic_path(location: tuple[object, ...]) -> str:
    return ".".join(str(member) for member in location) or "$"


def load_specification(
    entry_path: str | Path,
    schema_root: str | Path,
) -> LoadedSpecification:
    """Read, validate, normalize, and model one yamaa specification."""
    written_path = Path(entry_path)
    origin_path = written_path.resolve()
    bundle = load_schema_bundle(schema_root)
    document = read_yaml_document(origin_path)
    if isinstance(document, dict) and "parents" in document:
        # Imported lazily so the schema interpreter remains usable on its own.
        from yamaa.io.project import find_project_configuration
        from yamaa.schema.inheritance import resolve_specification

        # A standalone load discovers the entry's project configuration the
        # same way a run would (REQ-0768); engine calls pass the approved
        # root through plan_workflow instead.
        configuration = find_project_configuration(origin_path)
        resolved = resolve_specification(
            origin_path,
            bundle,
            entry_document=document,
            project_root=(configuration.parent if configuration is not None else None),
        )
        # Row-level (value-level) submission metadata: data-independent checks.
        value_diagnostics = validate_value_metadata(resolved.specification)
        if value_diagnostics:
            raise SpecificationError(value_diagnostics)
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

    # Row-level (value-level) submission metadata: data-independent checks.
    value_diagnostics = validate_value_metadata(specification)
    if value_diagnostics:
        raise SpecificationError(value_diagnostics)

    return LoadedSpecification(
        specification=specification,
        written_path=written_path,
        origin_path=origin_path,
        schema_path=bundle.path,
    )
