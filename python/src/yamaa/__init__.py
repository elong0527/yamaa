"""Python helpers for the yamaa clinical data specification."""

from importlib import import_module as _import_module
from importlib.metadata import version as _distribution_version
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from yamaa._reference_domain import DomainRun, DomainRunError, yamaa_domain
    from yamaa.submission import generate_study_document

_EXPORTS = {
    "domain": ("yamaa._domain", "domain"),
    "check": ("yamaa._domain", "check"),
    "DomainError": ("yamaa._domain", "DomainError"),
    "DomainRun": ("yamaa._reference_domain", "DomainRun"),
    "DomainRunError": ("yamaa._reference_domain", "DomainRunError"),
    "yamaa_domain": ("yamaa._reference_domain", "yamaa_domain"),
    "generate_study_document": ("yamaa.submission", "generate_study_document"),
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


__version__ = _distribution_version("yamaa")

__all__ = [
    "domain",
    "check",
    "DomainError",
    "DomainRun",
    "DomainRunError",
    "__version__",
    "generate_study_document",
    "yamaa_domain",
]
