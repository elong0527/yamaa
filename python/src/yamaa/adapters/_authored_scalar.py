"""Scalar transport for the retained reference qualification adapters."""

import math
from collections.abc import Mapping

from pydantic import JsonValue

from yamaa.models.values import (
    INT64_MAX,
    INT64_MIN,
    MISSING,
    DateTimeValue,
    DateValue,
    RuntimeValue,
)


class AuthoredValueError(ValueError):
    """An authored default or vector value names no runtime value."""


def runtime_value(authored: JsonValue) -> RuntimeValue:
    """Return the runtime value one authored R018 scalar names.

    REQ-0679 writes a temporal value as a tagged single-key mapping so that
    it stays a typed value rather than becoming text; everything else is
    the YAML scalar itself, with R011's non-finite normalization applied.
    """
    if authored is None:
        return MISSING
    if type(authored) is bool:
        return authored
    if type(authored) is int:
        if not INT64_MIN <= authored <= INT64_MAX:
            raise AuthoredValueError("an integer outside 64 bits names no value")
        return authored
    if type(authored) is float:
        return authored if math.isfinite(authored) else MISSING
    if isinstance(authored, str):
        return authored
    if isinstance(authored, Mapping) and len(authored) == 1:
        kind, text = next(iter(authored.items()))
        if kind in ("date", "datetime") and isinstance(text, str):
            try:
                if kind == "date":
                    return DateValue.parse(text)
                return DateTimeValue.parse(text)
            except ValueError as error:
                raise AuthoredValueError(f"invalid R016 {kind} text") from error
    raise AuthoredValueError("value carries no exact R018 scalar type")
