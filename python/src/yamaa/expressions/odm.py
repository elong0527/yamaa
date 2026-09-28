"""Registration of the `odm` item read (REQ-1265).

An `odm` expression reads records of an ODM input rather than one bound
value, so, like `aggregate`, it is dispatched to the resolver that owns the
relation; the row resolver decides what comes back (REQ-1269 through
REQ-1272).
"""

from __future__ import annotations

from yamaa.expressions.core import ExpressionHandler, Resolver, relational_handler
from yamaa.models import EvaluationResult

_RELATIONAL = relational_handler("odm")


def _odm(payload: object, resolver: Resolver) -> EvaluationResult:
    # REQ-1265: a bare string is the item. The schema expands it at load;
    # a caller dispatching a raw expression may still hand the string over.
    if isinstance(payload, str):
        payload = {"item": payload}
    return _RELATIONAL(payload, resolver)


def odm_handlers() -> dict[str, ExpressionHandler]:
    """Return the `odm` operation this component registers."""
    return {"odm": _odm}
