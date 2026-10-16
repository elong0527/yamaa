"""Public argument and data-frame conversion for the shared Rust lifecycle."""

from os import PathLike, fspath

import polars as pl

from yamaa._native import DomainError, _check_file, _domain_file
from yamaa._native_results import issues_frame


class Domain:
    """An owned native build with independently copied host views."""

    def __init__(self, native, resolver=None):
        self._native = native
        self._resolver = resolver

    @property
    def output(self) -> pl.DataFrame | None:
        data = self._native.output()
        return None if data is None else pl.read_ipc_stream(data)

    @property
    def issues(self) -> pl.DataFrame:
        return issues_frame(self._native.issues())

    @property
    def verification_log(self) -> pl.DataFrame | None:
        data = self._native.log(False)
        return None if data is None else pl.read_ipc_stream(data)

    @property
    def warning_log(self) -> pl.DataFrame | None:
        data = self._native.log(True)
        return None if data is None else pl.read_ipc_stream(data)

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
    native = _domain_file(
        fspath(specification), None if environment is None else fspath(environment),
        None if resolver is None else resolver._dispatch,
    )
    try:
        native.propagate_interrupt()
        if resolver is not None:
            resolver._raise_pending()
        return Domain(native, resolver)
    finally:
        # Retained callback tracebacks can include this frame. Keep their
        # native owner in the returned result, never in this frame's locals.
        del native


class Check:
    def __init__(self, rows):
        self._rows = rows

    @property
    def issues(self) -> pl.DataFrame:
        return issues_frame(self._rows)


def check(
    specification: str | PathLike[str],
    *,
    environment: str | PathLike[str] | None = None,
) -> Check:
    """Return native preparation and static issues without reading study data."""
    return Check(
        _check_file(
            fspath(specification), None if environment is None else fspath(environment)
        )
    )


__all__ = ["DomainError", "check", "domain"]
