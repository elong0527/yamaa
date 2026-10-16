"""Public argument and data-frame conversion for the shared Rust lifecycle."""

from os import PathLike, fspath

import polars as pl
import pyarrow as pa

from yamaa._native import DomainError, _check_file, _domain_file
from yamaa._native_results import issues_frame


class Domain:
    """An owned native build with independently owned Arrow and Polars views."""

    def __init__(self, native):
        self._native = native

    @property
    def output(self) -> pl.DataFrame | None:
        data = self._native.output()
        return None if data is None else pl.DataFrame(data)

    @property
    def output_arrow(self) -> pa.Table | None:
        """Share retained immutable buffers through the Arrow C Stream protocol."""
        data = self._native.output()
        return None if data is None else pa.table(data)

    @property
    def issues(self) -> pl.DataFrame:
        return issues_frame(self._native.issues())

    @property
    def verification_log(self) -> pl.DataFrame | None:
        data = self._native.log(False)
        return None if data is None else pl.DataFrame(data)

    @property
    def verification_log_arrow(self) -> pa.Table | None:
        data = self._native.log(False)
        return None if data is None else pa.table(data)

    @property
    def warning_log(self) -> pl.DataFrame | None:
        data = self._native.log(True)
        return None if data is None else pl.DataFrame(data)

    @property
    def warning_log_arrow(self) -> pa.Table | None:
        data = self._native.log(True)
        return None if data is None else pa.table(data)

    def save(self) -> bool:
        """Publish held logs and output; a failed build raises DomainError."""
        return self._native.save()


def domain(
    specification: str | PathLike[str],
    *,
    environment: str | PathLike[str] | None = None,
) -> Domain:
    native = _domain_file(
        fspath(specification), None if environment is None else fspath(environment)
    )
    try:
        native.propagate_interrupt()
        return Domain(native)
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
