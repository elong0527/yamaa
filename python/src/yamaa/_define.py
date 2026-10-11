"""Public argument and issues conversion for shared native Define generation."""
from importlib.metadata import version
from os import PathLike, fspath

from yamaa._native import DefineError, _define_file
from yamaa._native_results import issues_frame


class Define:
    """An owned metadata snapshot whose rendered outputs are published by Save."""

    def __init__(self, native):
        self._native = native

    @property
    def issues(self):
        return issues_frame(self._native.issues())

    def save(self) -> bool:
        """Publish all preflighted outputs; failure raises DefineError."""
        try:
            return self._native.save()
        except DefineError as error:
            error.issues = self.issues
            raise


def define(environment: str | PathLike[str], standard: str, *, created: str | None = None) -> Define:
    """Compose submission metadata without running a specification."""
    return Define(_define_file(fspath(environment), standard, created, version("yamaa")))


__all__ = ["DefineError", "define"]
