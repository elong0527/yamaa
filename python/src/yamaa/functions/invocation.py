"""Cross the host boundary once, for one logical row.

R018-24 invokes a binding once per logical row with nothing but its declared
arguments, and R018-25 fixes what may come back. Both directions of that
boundary live here: an authored value becomes a runtime value, a runtime
value becomes a host scalar, and whatever the host returns is checked
against the contract before any other stage sees it. A short-circuit under
R018-20 never reaches the host at all.
"""

from __future__ import annotations

import datetime as dt
import math
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from typing import TypeAlias

from pydantic import JsonValue

from yamaa.functions.models import FunctionContract, FunctionParameter
from yamaa.io.csv import fixed_point
from yamaa.models.values import (
    INT64_MAX,
    INT64_MIN,
    MISSING,
    ConditionResult,
    DateTimeValue,
    DateValue,
    EvaluationResult,
    RuntimeCondition,
    RuntimeValue,
    ValueResult,
)

HostValue: TypeAlias = object


class AuthoredValueError(ValueError):
    """An authored default or vector value names no runtime value."""


def runtime_value(authored: JsonValue) -> RuntimeValue:
    """Return the runtime value one authored R018 scalar names.

    R018-18 writes a temporal value as a tagged single-key mapping so that
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


def value_type(value: RuntimeValue) -> str | None:
    """Return the exact R018 type of one runtime value, or None for missing."""
    if value is MISSING:
        return None
    if type(value) is bool:
        return "bool"
    if type(value) is int:
        return "int"
    if type(value) is float:
        return "float"
    if isinstance(value, str):
        return "str"
    if isinstance(value, DateValue):
        return "date"
    if isinstance(value, DateTimeValue):
        return "datetime"
    return "unsupported"


def _host_argument(value: RuntimeValue) -> HostValue:
    """Return the host scalar one runtime value is passed to a binding as."""
    if value is MISSING:
        # R018-20 passes the host runtime's canonical missing scalar.
        return None
    if isinstance(value, DateValue):
        return dt.date(value.year, value.month, value.day)
    if isinstance(value, DateTimeValue):
        # R016 fixes a zone-free local civil datetime, so the host scalar
        # this becomes carries no zone either.
        return dt.datetime(  # noqa: DTZ001
            value.year,
            value.month,
            value.day,
            value.hour,
            value.minute,
            value.second,
        )
    return value


class _ResultRejected(ValueError):
    def __init__(self, reason: str, **context: JsonValue) -> None:
        self.reason = reason
        self.context = context
        super().__init__(reason)


def _host_result(value: HostValue) -> RuntimeValue:
    """Normalize one returned host scalar, or say why it is not one.

    R018-25 runs R011's non-finite normalization here, immediately after the
    host returns and before the contract's result checks, so a returned
    infinity is a missing result that a contract must have declared rather
    than a float that slips through.
    """
    if value is None:
        return MISSING
    if type(value) is bool:
        raise _ResultRejected("a binding returned a Boolean", returned="bool")
    if type(value) is int:
        if not INT64_MIN <= value <= INT64_MAX:
            raise _ResultRejected("a returned integer exceeds 64 bits")
        return value
    if type(value) is float:
        return value if math.isfinite(value) else MISSING
    if isinstance(value, str):
        return value
    if isinstance(value, (DateValue, DateTimeValue)):
        return value
    if type(value) is dt.date:
        return DateValue(year=value.year, month=value.month, day=value.day)
    if type(value) is dt.datetime:
        if value.tzinfo is not None or value.microsecond:
            raise _ResultRejected(
                "a returned datetime carries a zone or a fraction of a second"
            )
        return DateTimeValue(
            year=value.year,
            month=value.month,
            day=value.day,
            hour=value.hour,
            minute=value.minute,
            second=value.second,
        )
    raise _ResultRejected(
        "a binding returned a value of no scalar type",
        returned=type(value).__name__,
    )


def _condition(
    condition: str,
    requirement: str,
    context: dict[str, JsonValue],
) -> ConditionResult:
    """Return one fatal R018 condition; R018-40 and R018-41 admit no handler."""
    return ConditionResult(
        condition=RuntimeCondition(
            phase="derivation",
            condition=condition,
            context=context,
            applicable_handler=None,
            requirement=requirement,
        )
    )


def results_match(actual: RuntimeValue, expected: RuntimeValue, decimals: int) -> bool:
    """Compare one result with its expectation under R018-31.

    A float compares through temporary decimal copies at the contract's
    precision; R018-32 keeps that comparison off the value itself, so the
    caller still holds the unrounded result. Every other type compares by
    its own equality, which for text is R019's.
    """
    if actual is MISSING or expected is MISSING:
        return actual is MISSING and expected is MISSING
    if value_type(actual) != value_type(expected):
        return False
    if type(actual) is float and type(expected) is float:
        return fixed_point(actual, decimals) == fixed_point(expected, decimals)
    return bool(actual == expected)


@dataclass(frozen=True, slots=True)
class BoundFunction:
    """One activated contract and the callable resolved inside its artifact."""

    name: str
    contract: FunctionContract
    target: Callable[..., object]

    def _identity(self) -> dict[str, JsonValue]:
        """Return what R018-43 requires every failure of this call to name."""
        return {
            "function": self.name,
            "contract_version": self.contract.contract_version,
            "implementation_version": self.contract.implementation_version,
        }

    def _argument(
        self,
        parameter: FunctionParameter,
        supplied: Mapping[str, RuntimeValue],
    ) -> RuntimeValue | ConditionResult:
        if parameter.name not in supplied:
            if parameter.required:
                return _condition(
                    "invalid_function_argument",
                    "R018-39",
                    {
                        **self._identity(),
                        "parameter": parameter.name,
                        "reason": "a required argument was not supplied",
                    },
                )
            # R018-20: omitting an optional argument selects its default.
            return runtime_value(parameter.default)
        value = supplied[parameter.name]
        declared = value_type(value)
        if declared is not None and declared != parameter.type:
            return _condition(
                "invalid_function_argument",
                "R018-39",
                {
                    **self._identity(),
                    "parameter": parameter.name,
                    "expected": parameter.type,
                    "actual": declared,
                },
            )
        return value

    def invoke(self, supplied: Mapping[str, RuntimeValue]) -> EvaluationResult:
        """Apply R018-20, then invoke this binding once for one logical row."""
        unknown = sorted(set(supplied) - set(self.contract.parameters))
        if unknown:
            return _condition(
                "invalid_function_argument",
                "R018-39",
                {**self._identity(), "unknown": unknown},
            )

        arguments: dict[str, HostValue] = {}
        for parameter in self.contract.params:
            value = self._argument(parameter, supplied)
            if isinstance(value, ConditionResult):
                return value
            if value is MISSING and not parameter.accepts_missing:
                # R018-20 and R018-21: the binding is not invoked and the
                # result is missing, which is not a returned missing value.
                return ValueResult(value=MISSING)
            host_name = self.contract.binding.args[parameter.name]
            arguments[host_name] = _host_argument(value)

        try:
            returned = self.target(**arguments)
        # R018-40 is exactly this: whatever the host raised is fatal, and a
        # runner that let one class of host failure through would be wrong.
        except Exception as error:  # noqa: BLE001
            return _condition(
                "function_call_failed",
                "R018-40",
                {
                    **self._identity(),
                    "call": self.contract.binding.call,
                    "host_error": type(error).__name__,
                    "host_message": str(error),
                },
            )
        return self._result(returned)

    def _result(self, returned: HostValue) -> EvaluationResult:
        """Check one returned scalar against the contract it must satisfy."""
        try:
            value = _host_result(returned)
        except _ResultRejected as rejected:
            return _condition(
                "invalid_function_result",
                "R018-41",
                {**self._identity(), "reason": rejected.reason, **rejected.context},
            )
        if value is MISSING:
            if not self.contract.may_return_missing:
                return _condition(
                    "invalid_function_result",
                    "R018-41",
                    {
                        **self._identity(),
                        "reason": "an invoked binding returned an undeclared missing",
                    },
                )
            return ValueResult(value=MISSING)
        declared = value_type(value)
        if declared != self.contract.returns:
            return _condition(
                "invalid_function_result",
                "R018-41",
                {
                    **self._identity(),
                    "expected": self.contract.returns,
                    "actual": declared,
                },
            )
        return ValueResult(value=value)


__all__ = [
    "AuthoredValueError",
    "BoundFunction",
    "results_match",
    "runtime_value",
    "value_type",
]
