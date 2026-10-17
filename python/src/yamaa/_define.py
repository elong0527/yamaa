"""Public argument and issues conversion for shared native Define generation."""

from os import PathLike, fspath

from yamaa._native import _CORE_VERSION, DefineError, _define_file
from yamaa._native_results import Diagnostic, diagnostic_records, issues_frame


class Define:
    """An owned metadata snapshot whose rendered outputs are published by Save."""

    def __init__(self, native):
        self._native = native

    @property
    def issues(self):
        """The five-column frame projected from the same records as ``diagnostics``."""
        return issues_frame(self._native.issues())

    @property
    def diagnostics(self) -> list[Diagnostic]:
        """Named records with structured context and optional geometry."""
        return diagnostic_records(self._native.issues())

    def save(self) -> bool:
        """Publish all preflighted outputs; failure raises DefineError.

        The error carries both views of the same retained diagnostics.
        """
        try:
            return self._native.save()
        except DefineError as error:
            rows = self._native.issues()
            error.issues = issues_frame(rows)
            error.diagnostics = diagnostic_records(rows)
            raise


def define(
    environment: str | PathLike[str], standard: str, *, created: str | None = None
) -> Define:
    """Compose submission metadata without running a specification."""
    return Define(_define_file(fspath(environment), standard, created, _CORE_VERSION))


__all__ = ["DefineError", "define"]
