"""Python helpers for the yamaa clinical data specification."""

from importlib import import_module as _import_module
from importlib.metadata import PackageNotFoundError as _PackageNotFoundError
from importlib.metadata import version as _distribution_version
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from yamaa._define import DefineError, define
    from yamaa._domain import DomainError, check, domain
    from yamaa._native_results import Diagnostic, SourceSpan
    from yamaa._odm.parquet import write_odm_parquet
    from yamaa._odm.readers import read_odm
    from yamaa._resolved_specification import (
        ResolvedSpecification,
        ResolvedSpecificationError,
        resolve_specification,
    )

_EXPORTS = {
    "read_odm": ("yamaa._odm.readers", "read_odm"),
    "write_odm_parquet": ("yamaa._odm.parquet", "write_odm_parquet"),
    "define": ("yamaa._define", "define"),
    "DefineError": ("yamaa._define", "DefineError"),
    "domain": ("yamaa._domain", "domain"),
    "check": ("yamaa._domain", "check"),
    "DomainError": ("yamaa._domain", "DomainError"),
    "Diagnostic": ("yamaa._native_results", "Diagnostic"),
    "SourceSpan": ("yamaa._native_results", "SourceSpan"),
    "resolve_specification": ("yamaa._resolved_specification", "resolve_specification"),
    "ResolvedSpecification": ("yamaa._resolved_specification", "ResolvedSpecification"),
    "ResolvedSpecificationError": (
        "yamaa._resolved_specification",
        "ResolvedSpecificationError",
    ),
}


def __getattr__(name: str):
    """Load a requested facade or helper without importing unrelated implementations."""
    if name not in _EXPORTS:
        raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
    module, attribute = _EXPORTS[name]
    value = getattr(_import_module(module), attribute)
    globals()[name] = value
    return value


def __dir__() -> list[str]:
    return sorted(set(globals()) | set(_EXPORTS))


try:
    __version__ = _distribution_version("yamaa")
except _PackageNotFoundError:
    # A vendored copy carries no distribution metadata. The compiled core
    # version is the same release: the shared-release check keeps the
    # Python, R and Rust versions equal.
    from yamaa._native import _CORE_VERSION as __version__

__all__ = [
    "DefineError",
    "Diagnostic",
    "DomainError",
    "ResolvedSpecification",
    "ResolvedSpecificationError",
    "SourceSpan",
    "__version__",
    "check",
    "define",
    "domain",
    "read_odm",
    "resolve_specification",
    "write_odm_parquet",
]
