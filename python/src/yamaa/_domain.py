"""Public argument and data-frame conversion for the shared Rust lifecycle."""

from __future__ import annotations

from os import PathLike, fspath
from typing import TYPE_CHECKING

import polars as pl

if TYPE_CHECKING:
    import pyarrow as pa

from yamaa._native import DomainError, _check_file, _domain_file
from yamaa._native_results import Diagnostic, diagnostic_records, issues_frame


def _arrow_view(data) -> pa.Table | None:
    if data is None:
        return None
    import pyarrow as pa

    return pa.table(data)


class Domain:
    """An owned native build with independently owned Arrow and Polars views."""

    def __init__(self, native, resolver=None):
        self._native = native
        self._resolver = resolver

    @property
    def output(self) -> pl.DataFrame | None:
        data = self._native.output()
        return None if data is None else pl.DataFrame(data)

    @property
    def output_arrow(self) -> pa.Table | None:
        """Share retained immutable buffers through the Arrow C Stream protocol."""
        return _arrow_view(self._native.output())

    @property
    def issues(self) -> pl.DataFrame:
        """The five-column frame projected from the same records as ``diagnostics``."""
        return issues_frame(self._native.issues())

    @property
    def diagnostics(self) -> list[Diagnostic]:
        """Named records with structured context and optional geometry."""
        return diagnostic_records(self._native.issues())

    @property
    def resource_provenance(self):
        return () if self._resolver is None else self._resolver.provenance

    @property
    def verification_log(self) -> pl.DataFrame | None:
        data = self._native.log(False)
        return None if data is None else pl.DataFrame(data)

    @property
    def verification_log_arrow(self) -> pa.Table | None:
        return _arrow_view(self._native.log(False))

    @property
    def warning_log(self) -> pl.DataFrame | None:
        data = self._native.log(True)
        return None if data is None else pl.DataFrame(data)

    @property
    def warning_log_arrow(self) -> pa.Table | None:
        return _arrow_view(self._native.log(True))

    def save(self) -> bool:
        """Publish held logs and output; a failed build raises DomainError."""
        try:
            return self._native.save()
        finally:
            if self._resolver is not None:
                self._resolver._raise_pending()


def domain(
    specification: str | PathLike[str],
    *,
    environment: str | PathLike[str] | None = None,
    resolver=None,
) -> Domain:
    resolver = None if resolver is None else resolver._session()
    native = _domain_file(
        fspath(specification),
        None if environment is None else fspath(environment),
        None if resolver is None else resolver._dispatch,
    )
    try:
        if resolver is not None:
            resolver._raise_pending()
        return Domain(native, resolver)
    finally:
        # Retained callback tracebacks can include this frame. Keep their
        # native owner in the returned result, never in this frame's locals.
        del native


class Check:
    """Owned static findings: a frame and named records of the same rows."""

    def __init__(self, rows):
        self._rows = rows

    @property
    def issues(self) -> pl.DataFrame:
        return issues_frame(self._rows)

    @property
    def diagnostics(self) -> list[Diagnostic]:
        return diagnostic_records(self._rows)


def check(
    specification: str | PathLike[str],
    *,
    environment: str | PathLike[str] | None = None,
    resolver=None,
) -> Check:
    """Return native preparation and static issues without reading study data."""
    resolver = None if resolver is None else resolver._session()
    result = Check(
        _check_file(
            fspath(specification),
            None if environment is None else fspath(environment),
            None if resolver is None else resolver._dispatch,
        )
    )
    if resolver is not None:
        resolver._raise_pending()
    return result


__all__ = ["DomainError", "check", "domain"]
