"""Schema-governed transformations applied before ordinary execution."""

from yamaa.schema.inheritance import (
    ResolvedSpecification,
    SourceOrigin,
    resolve_specification,
)

__all__ = ["ResolvedSpecification", "SourceOrigin", "resolve_specification"]
