"""The R018-10 contract identity two projects must agree on.

A logical contract is claimed by name and version, but a name is not an
agreement: two projects claim the same contract only when the fingerprint
calculated here is identical. R018-14 keeps the language, the artifact, the
binding, the description, and the implementation version out of that
payload, so an R project and a Python project implementing one behavior
agree while a project that quietly changed a parameter does not.
"""

from __future__ import annotations

import hashlib
import json
import math
import struct
from collections.abc import Mapping
from typing import TypeAlias

from yamaa.functions.models import FunctionContract, FunctionParameter
from yamaa.models.values import INT64_MAX, INT64_MIN, DateTimeValue, DateValue

CanonicalValue: TypeAlias = dict[str, object]


class ContractValueError(ValueError):
    """A declared value carries no exact R018 scalar type."""


def function_value_type(value: object) -> str | None:
    """Return the exact R018 type of one authored value, or None for missing.

    R018-12 runs R011's non-finite normalization first, which is why a
    non-finite float is missing here rather than a float.
    """
    if type(value) is float and not math.isfinite(value):
        return None
    if value is None:
        return None
    if type(value) is bool:
        return "bool"
    if type(value) is int:
        # R011 stores an integer in 64 bits, so a wider literal names no
        # value this vocabulary can carry.
        return "int" if INT64_MIN <= value <= INT64_MAX else "<invalid>"
    if type(value) is float:
        return "float"
    if isinstance(value, str):
        return "str"
    if isinstance(value, Mapping) and len(value) == 1:
        kind, text = next(iter(value.items()))
        if kind in ("date", "datetime") and isinstance(text, str):
            try:
                _temporal_text(kind, text)
            except ValueError:
                return "<invalid>"
            return kind
    return "<invalid>"


def _temporal_text(kind: str, text: str) -> str:
    """Return the R016 canonical text of one authored temporal literal.

    R018-11 encodes a temporal value as its R016 canonical text, so the
    encoding goes through the same parser every other temporal value in the
    package does rather than trusting the authored spelling.
    """
    parsed = DateValue.parse(text) if kind == "date" else DateTimeValue.parse(text)
    return parsed.to_text()


def canonical_function_value(value: object, declared_type: str) -> CanonicalValue:
    """Encode one value under R018-11 without losing its logical type."""
    actual = function_value_type(value)
    if actual is None:
        return {"type": "missing"}
    if actual != declared_type:
        raise ContractValueError(f"expected {declared_type!r}, got {actual!r}")
    if actual == "float":
        assert isinstance(value, float)
        return {"type": actual, "value": struct.pack(">d", value).hex()}
    if actual == "int":
        assert isinstance(value, int)
        return {"type": actual, "value": str(value)}
    if actual == "bool":
        return {"type": actual, "value": value}
    if actual in ("date", "datetime"):
        assert isinstance(value, Mapping)
        return {"type": actual, "value": _temporal_text(actual, value[actual])}
    return {"type": actual, "value": value}


def _canonical_parameter(parameter: FunctionParameter) -> dict[str, object]:
    """Encode one parameter under R018-11 in its declared position."""
    default: dict[str, object] = {"present": parameter.has_default}
    if parameter.has_default:
        default["value"] = canonical_function_value(parameter.default, parameter.type)
    return {
        "name": parameter.name,
        "type": parameter.type,
        "required": parameter.required,
        "accepts_missing": parameter.accepts_missing,
        "default": default,
    }


def contract_fingerprint(name: str, contract: FunctionContract) -> str:
    """Return the `sha256:`-prefixed R018-10 identity of one contract."""
    payload = {
        "format": "yamaa-r018-contract-v1",
        "name": name,
        "contract_version": contract.contract_version,
        "params": [_canonical_parameter(parameter) for parameter in contract.params],
        "returns": contract.returns,
        "may_return_missing": contract.may_return_missing,
        # R018-13 spells the comparison precision as a base-10 string, which
        # is also why the payload below carries no JSON number at all.
        "comparison_decimals": str(contract.comparison_decimals),
    }
    # RFC 8785 fixes member order and UTF-8 encoding. With no numeric value
    # left in the payload, sorted compact JSON is that canonical form, so no
    # host number formatting reaches the hash.
    canonical = json.dumps(
        payload,
        ensure_ascii=False,
        allow_nan=False,
        sort_keys=True,
        separators=(",", ":"),
    )
    return "sha256:" + hashlib.sha256(canonical.encode("utf-8")).hexdigest()


__all__ = [
    "ContractValueError",
    "canonical_function_value",
    "contract_fingerprint",
    "function_value_type",
]
