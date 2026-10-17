"""Owned, versioned inspection facts from the shared specification preparation."""

import json
from os import PathLike, fspath

import polars as pl

from yamaa._native import (
    _read_resolved_specification,
    _resolve_specification_file,
)
from yamaa._native_results import Diagnostic, diagnostic_records, issues_frame


class ResolvedSpecificationError(ValueError):
    """Resolution refused by the engine, with the original structured issues."""

    def __init__(self, rows):
        super().__init__("cannot resolve specification; inspect issues")
        self._rows = rows

    @property
    def issues(self) -> pl.DataFrame:
        return issues_frame(self._rows)

    @property
    def diagnostics(self) -> list[Diagnostic]:
        return diagnostic_records(self._rows)


class ResolvedSpecification:
    """An owned inspection artifact; it cannot be executed or used to save data.

    JSON loading retains the stored facts. It never resolves paths, opens sources
    or invokes callbacks. Each dictionary view is an independent host copy.
    """

    __slots__ = ("_json",)

    def __init__(self, text: str):
        self._json = _read_resolved_specification(text)

    @classmethod
    def from_json(cls, text: str) -> "ResolvedSpecification":
        """Validate versioned JSON for inspection, without recompilation or IO."""
        return cls(text)

    def to_json(self) -> str:
        """Return deterministic compact JSON with exact integer tokens."""
        return self._json

    def to_dict(self) -> dict:
        """Copy all inspection facts, preserving arbitrary-width JSON integers."""
        return json.loads(self._json)


def resolve_specification(
    specification: str | PathLike[str],
    *,
    environment: str | PathLike[str] | None = None,
) -> ResolvedSpecification:
    """Prepare the original entry and retain its inspection facts.

    Named dictionary references remain in the normalized model. Resources contain
    selected locations and provenance, including each `schema` link's producing
    specification; their presence does not assert existence.
    """
    text, rows = _resolve_specification_file(
        fspath(specification), None if environment is None else fspath(environment)
    )
    if text is None:
        raise ResolvedSpecificationError(rows)
    return ResolvedSpecification(text)
