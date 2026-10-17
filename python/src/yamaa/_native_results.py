"""Named diagnostic records and their data-frame projection for native results.

The native boundary hands back one row per diagnostic: the five frame fields
(``context`` as the engine's canonical JSON text), the structured context
tree, and the optional geometry. Both public views are projections of those
rows, so the ``issues`` frame and the ``diagnostics`` records can never
disagree, and neither parses JSON text in Python.
"""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass

import polars as pl

NativeRow = tuple[
    str,
    str,
    str | None,
    list[str],
    str,
    dict[str, object],
    tuple[int, int] | None,
    list[str] | None,
]
ISSUE_SCHEMA = {
    "phase": pl.String,
    "condition": pl.String,
    "requirement": pl.String,
    "spec_paths": pl.List(pl.String),
    "context": pl.String,
}


@dataclass(frozen=True, slots=True)
class SourceSpan:
    """Half-open byte offsets into the original expression text.

    A present ``SourceSpan(0, 0)`` is a recorded zero-width span; a record
    whose failure site recorded no geometry carries ``None`` instead.
    """

    start: int
    end: int


@dataclass(frozen=True, slots=True)
class Diagnostic:
    """One finding as the shared engine recorded it.

    ``context`` is the structured context: integer tokens are ``int`` with
    their exact value whatever their width, other numbers ``float``, objects
    ``dict`` and arrays ``list``. ``source_span`` and ``operand_route`` are
    ``None`` when the failure site recorded no geometry; a present empty
    route is a failure at the expression root, not an absent route.
    """

    phase: str
    condition: str
    requirement: str | None
    spec_paths: list[str]
    context: dict[str, object]
    source_span: SourceSpan | None
    operand_route: list[str] | None


def diagnostic_records(rows: Sequence[NativeRow]) -> list[Diagnostic]:
    """Copy the native rows into frozen records; every value is already owned."""
    return [
        Diagnostic(
            phase=phase,
            condition=condition,
            requirement=requirement,
            spec_paths=list(spec_paths),
            context=context,
            source_span=None if span is None else SourceSpan(*span),
            operand_route=None if route is None else list(route),
        )
        for phase, condition, requirement, spec_paths, _, context, span, route in rows
    ]


def issues_frame(rows: Sequence[NativeRow]) -> pl.DataFrame:
    """Project the five frame columns from the same native rows.

    Empty and all-missing columns keep their schema; ``context`` stays the
    engine's canonical JSON text.
    """
    return pl.DataFrame([row[:5] for row in rows], schema=ISSUE_SCHEMA, orient="row")
