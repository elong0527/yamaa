"""Public argument and data-frame conversion for the shared Rust lifecycle."""

from os import PathLike, fspath

import polars as pl

from yamaa._native import DomainError, _check_file, _domain_file
from yamaa._native_results import issues_frame


class Domain:
    """An owned native build with independently copied host views."""

    def __init__(self, native):
        self._native = native

    @property
    def output(self) -> pl.DataFrame | None:
        data = self._native.output()
        return None if data is None else pl.read_ipc_stream(data)

    @property
    def issues(self) -> pl.DataFrame:
        return issues_frame(self._native.issues())

    @property
    def verification_log(self) -> pl.DataFrame | None:
        return None

    @property
    def warning_log(self) -> pl.DataFrame | None:
        return None

    def save(self) -> bool:
        """Publish the declared artifact; a failed build raises DomainError."""
        return self._native.save()


def domain(
    specification: str | PathLike[str],
    *,
    environment: str | PathLike[str] | None = None,
) -> Domain:
    return Domain(
        _domain_file(
            fspath(specification), None if environment is None else fspath(environment)
        )
    )


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
