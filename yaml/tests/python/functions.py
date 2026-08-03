"""Host functions reachable from the `call` operation.

`call` is the R007 escape hatch. Every function here must exist in the R
implementation under the same name with the same named arguments and the same
result, or a specification using it is not portable. Keeping them in one small
audited file is what makes that checkable.
"""
from __future__ import annotations

from datetime import date

from predicates import DerivationError


def _as_date(v):
    if v in (None, ""):
        return None
    try:
        return date.fromisoformat(str(v)[:10])
    except ValueError:
        raise DerivationError(f"{v!r} is not a complete ISO 8601 date")


def study_day(date_, reference):
    """SDTM --DY / ADaM ADY. Day 1 is the reference day; there is no day 0."""
    d, r = _as_date(date_), _as_date(reference)
    if d is None or r is None:
        return None
    diff = (d - r).days
    return diff + 1 if diff >= 0 else diff


def concat(values, sep=""):
    """Join values in order. Missing anywhere yields missing."""
    if any(v in (None, "") for v in values):
        return None
    return str(sep).join(str(v) for v in values)


HOST_FUNCTIONS = {
    "study_day": study_day,
    "concat": concat,
}
