"""Load validated yamaa specifications."""

from importlib import import_module as _import_module
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from yamaa.specification.diagnostics import (
        SpecificationError,
        ValidationDiagnostic,
    )
    from yamaa.specification.loader import load_specification
    from yamaa.specification.models import LoadedSpecification, Specification

_EXPORTS = {
    "SpecificationError": ("yamaa.specification.diagnostics", "SpecificationError"),
    "ValidationDiagnostic": ("yamaa.specification.diagnostics", "ValidationDiagnostic"),
    "load_specification": ("yamaa.specification.loader", "load_specification"),
    "LoadedSpecification": ("yamaa.specification.models", "LoadedSpecification"),
    "Specification": ("yamaa.specification.models", "Specification"),
}


def __getattr__(name: str):
    """Load a reference export only when that export is requested."""
    if name not in _EXPORTS:
        raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
    module, attribute = _EXPORTS[name]
    value = getattr(_import_module(module), attribute)
    globals()[name] = value
    return value


def __dir__() -> list[str]:
    return sorted(set(globals()) | set(_EXPORTS))


__all__ = [
    "LoadedSpecification",
    "Specification",
    "SpecificationError",
    "ValidationDiagnostic",
    "load_specification",
]
