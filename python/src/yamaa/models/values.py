"""Portable runtime values, conversions, and ordered table contracts."""

from __future__ import annotations

import datetime as dt
import decimal
import math
import re
from enum import Enum
from typing import Annotated, Literal, TypeAlias

import polars as pl
from pydantic import (
    BaseModel,
    ConfigDict,
    Field,
    InstanceOf,
    JsonValue,
    model_validator,
)

from yamaa.specification.models import ColumnType

ConditionPhase: TypeAlias = Literal[
    "validation",
    "ingest",
    "row_construction",
    "derivation",
    "mapping",
    "join",
    "impute",
    "convert",
    "verification",
    "output",
]
HandlerName: TypeAlias = Literal[
    "missing",
    "multiple_matches",
    "unmapped",
    "no_match",
    "invalid",
    "conversion_failure",
    "override",
]

INT64_MIN = -(2**63)
INT64_MAX = 2**63 - 1
_NUMBER = re.compile(r"^[+-]?[0-9]+(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?$")
_NON_FINITE = re.compile(r"^[+-]?\.(?:inf|Inf|INF)$|^\.(?:nan|NaN|NAN)$")
_DATE = re.compile(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$")
_DATETIME = re.compile(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}(?::[0-9]{2})?$")


class MissingValue(Enum):
    """The one explicit runtime missing value."""

    TOKEN = 0


MISSING = MissingValue.TOKEN


class _FrozenModel(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)


class DateValue(_FrozenModel):
    """A complete proleptic Gregorian date with collected precision."""

    year: int = Field(ge=1, le=9999)
    month: int = Field(ge=1, le=12)
    day: int = Field(ge=1, le=31)
    collected_precision: Literal["year", "month", "day"] = "day"

    @model_validator(mode="after")
    def validate_calendar_date(self) -> DateValue:
        dt.date(self.year, self.month, self.day)
        return self

    @classmethod
    def parse(cls, text: str) -> DateValue:
        """Parse the exact R016 date lexical form."""
        if _DATE.fullmatch(text) is None:
            raise ValueError("invalid R016 date text")
        try:
            value = dt.date.fromisoformat(text)
        except ValueError as error:
            raise ValueError("invalid R016 date text") from error
        return cls(year=value.year, month=value.month, day=value.day)

    def to_text(self) -> str:
        return f"{self.year:04d}-{self.month:02d}-{self.day:02d}"

    @property
    def ordering_key(self) -> tuple[int, int, int]:
        return (self.year, self.month, self.day)

    def __eq__(self, other: object) -> bool:
        """Compare two dates by their fields alone.

        R016-35 keeps collected precision out of every comparison, and
        R016-10 makes it a property read off the value rather than part of
        its identity. Equality is that comparison, so a completed date and a
        collected one naming the same day are one value wherever a join
        matches, a partition groups, or a dictionary keys.
        """
        if not isinstance(other, DateValue):
            return NotImplemented
        return self.ordering_key == other.ordering_key

    def __hash__(self) -> int:
        return hash(self.ordering_key)


class DateTimeValue(_FrozenModel):
    """A complete zone-free local civil datetime at whole-second precision."""

    year: int = Field(ge=1, le=9999)
    month: int = Field(ge=1, le=12)
    day: int = Field(ge=1, le=31)
    hour: int = Field(ge=0, le=23)
    minute: int = Field(ge=0, le=59)
    second: int = Field(ge=0, le=59)
    collected_precision: Literal["second"] = "second"

    @model_validator(mode="after")
    def validate_civil_datetime(self) -> DateTimeValue:
        dt.date(self.year, self.month, self.day)
        return self

    @classmethod
    def parse(cls, text: str) -> DateTimeValue:
        """Parse the exact R016 local datetime lexical form."""
        if _DATETIME.fullmatch(text) is None:
            raise ValueError("invalid R016 datetime text")
        try:
            date_text, time_text = text.split("T", 1)
            year, month, day = (int(part) for part in date_text.split("-"))
            time_parts = [int(part) for part in time_text.split(":")]
            hour, minute = time_parts[:2]
            second = time_parts[2] if len(time_parts) == 3 else 0
            return cls(
                year=year,
                month=month,
                day=day,
                hour=hour,
                minute=minute,
                second=second,
            )
        except (ValueError, TypeError) as error:
            raise ValueError("invalid R016 datetime text") from error

    def to_text(self) -> str:
        return (
            f"{self.year:04d}-{self.month:02d}-{self.day:02d}T"
            f"{self.hour:02d}:{self.minute:02d}:{self.second:02d}"
        )

    @property
    def ordering_key(self) -> tuple[int, int, int, int, int, int]:
        return (
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
        )


RuntimeValue: TypeAlias = (
    MissingValue | str | int | float | bool | DateValue | DateTimeValue
)


class RuntimeCondition(_FrozenModel):
    """A structured, vocabulary-aligned non-success result."""

    phase: ConditionPhase
    condition: str = Field(min_length=1)
    context: dict[str, JsonValue] = Field(default_factory=dict)
    applicable_handler: HandlerName | None = None
    requirement: str | None = Field(default=None, pattern=r"^R[0-9]{3}-[0-9]+$")
    # The payload field the condition is about, relative to the operation, so
    # a reported path names `str_extract.group` rather than `str_extract`.
    path_suffix: str | None = Field(default=None, min_length=1)


class HandlerObservation(_FrozenModel):
    """One handler that fired inside a nested expression.

    R008-21 counts every handler path, and R007-3 lets `case` and
    `str_concat` nest an expression that owns handlers of its own. The path
    is relative to the payload of the operation that returned this result,
    so the caller that knows the specification path can complete it.
    """

    path: str = Field(min_length=1)
    handler: HandlerName


class ValueResult(_FrozenModel):
    """A successful evaluation, optionally produced by a local handler."""

    status: Literal["value"] = "value"
    value: RuntimeValue
    handled_by: HandlerName | None = None
    observations: tuple[HandlerObservation, ...] = ()


class ConditionResult(_FrozenModel):
    """A failed evaluation with a structured condition."""

    status: Literal["condition"] = "condition"
    condition: RuntimeCondition


class UnsupportedResult(_FrozenModel):
    """A valid operation outside the component's implemented subset."""

    status: Literal["unsupported"] = "unsupported"
    operation: str = Field(min_length=1)


EvaluationResult: TypeAlias = ValueResult | ConditionResult | UnsupportedResult


def _condition(
    phase: ConditionPhase,
    condition: str,
    context: dict[str, JsonValue],
    applicable_handler: HandlerName | None = None,
    requirement: str | None = None,
) -> ConditionResult:
    return ConditionResult(
        condition=RuntimeCondition(
            phase=phase,
            condition=condition,
            context=context,
            applicable_handler=applicable_handler,
            requirement=requirement,
        )
    )


def runtime_type_name(value: object) -> str | None:
    """Return the closed runtime type name, or ``None`` for missing."""
    if value is MISSING or value is None:
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


def _json_value(value: RuntimeValue | object) -> JsonValue:
    if value is MISSING or value is None:
        return None
    if isinstance(value, (DateValue, DateTimeValue)):
        return value.to_text()
    if isinstance(value, (str, int, float, bool)):
        return value
    return type(value).__name__


def normalize_runtime_value(
    value: object,
) -> EvaluationResult:
    """Normalize one host value at a language boundary."""
    if value is MISSING or value is None:
        return ValueResult(value=MISSING)
    if type(value) is float:
        return ValueResult(value=value if math.isfinite(value) else MISSING)
    if type(value) is int:
        if INT64_MIN <= value <= INT64_MAX:
            return ValueResult(value=value)
        return _condition(
            "derivation",
            "integer_overflow",
            {"value": value, "minimum": INT64_MIN, "maximum": INT64_MAX},
        )
    if isinstance(value, str):
        for offset, character in enumerate(value):
            code_point = ord(character)
            if 0xD800 <= code_point <= 0xDFFF:
                return _condition(
                    "ingest",
                    "invalid_text",
                    {"code_point": f"U+{code_point:04X}", "offset": offset},
                )
        return ValueResult(value=value)
    if type(value) is bool or isinstance(value, (DateValue, DateTimeValue)):
        return ValueResult(value=value)
    return _condition(
        "validation",
        "incompatible_input_type",
        {"actual": type(value).__name__},
    )


def ordering_key(value: RuntimeValue) -> object:
    """Return the key one value orders by."""
    key = getattr(value, "ordering_key", None)
    return key if key is not None else value


def compare_values(left: RuntimeValue, right: RuntimeValue) -> int:
    """Compare two non-missing values in the order their type owns.

    R007-17 gives numeric order to R010, text order to R019, and chronological
    order to R016, so one comparison serves every ordered operation rather
    than each reimplementing its type's order. Two values of types that are
    not mutually comparable raise, because R007-39 refuses to convert an
    operand to make a comparison work.
    """
    if not values_comparable(left, right):
        raise TypeError(
            f"incomparable values {runtime_type_name(left)!r} "
            f"and {runtime_type_name(right)!r}"
        )
    ordered_left = ordering_key(left)
    ordered_right = ordering_key(right)
    if ordered_left < ordered_right:  # type: ignore[operator]
        return -1
    if ordered_left > ordered_right:  # type: ignore[operator]
        return 1
    return 0


def values_comparable(left: RuntimeValue, right: RuntimeValue) -> bool:
    """Return whether two non-missing values are comparable under R007."""
    left_type = runtime_type_name(left)
    right_type = runtime_type_name(right)
    if left_type is None or right_type is None:
        return True
    if {left_type, right_type} <= {"int", "float"}:
        return True
    return left_type == right_type and left_type in {"str", "date", "datetime"}


def _parse_number(text: str) -> RuntimeValue:
    if _NON_FINITE.fullmatch(text):
        return MISSING
    if _NUMBER.fullmatch(text) is None:
        raise ValueError("invalid numeric text")
    if "." not in text and "e" not in text.lower():
        return int(text, 10)
    value = float(text)
    return value if math.isfinite(value) else MISSING


def _float_text(value: float) -> str:
    rendered = format(decimal.Decimal(repr(value)), "f")
    return rendered.removesuffix(".0")


def _conversion_failure(value: RuntimeValue, target: ColumnType) -> ConditionResult:
    return _condition(
        "convert",
        "conversion_failed",
        {
            "from": runtime_type_name(value) or "missing",
            "to": target,
            "value": _json_value(value),
        },
        "conversion_failure",
    )


def convert_value(value: object, target: ColumnType) -> EvaluationResult:
    """Convert one value through the closed R011 conversion table."""
    normalized = normalize_runtime_value(value)
    if not isinstance(normalized, ValueResult):
        return normalized
    source = normalized.value
    if source is MISSING:
        return normalized

    source_type = runtime_type_name(source)
    if source_type == target:
        return ValueResult(value=source)

    if target == "str":
        if type(source) is int:
            return ValueResult(value=str(source))
        if type(source) is float:
            return ValueResult(value=_float_text(source))
        if isinstance(source, (DateValue, DateTimeValue)):
            return ValueResult(value=source.to_text())
        return _conversion_failure(source, target)

    if target == "float" and isinstance(source, str):
        if _NON_FINITE.fullmatch(source):
            return ValueResult(value=MISSING)
        if _NUMBER.fullmatch(source) is None:
            return _conversion_failure(source, target)
        try:
            parsed_float = float(source)
        except OverflowError:
            return ValueResult(value=MISSING)
        return ValueResult(
            value=parsed_float if math.isfinite(parsed_float) else MISSING
        )

    if target == "int" and isinstance(source, str):
        try:
            parsed = _parse_number(source)
        except ValueError:
            return _conversion_failure(source, target)
        if parsed is MISSING:
            return ValueResult(value=MISSING)
        source = parsed

    if target == "int":
        if type(source) is int:
            if INT64_MIN <= source <= INT64_MAX:
                return ValueResult(value=source)
            return _conversion_failure(source, target)
        if (
            type(source) is float
            and source.is_integer()
            and INT64_MIN <= source <= INT64_MAX
        ):
            return ValueResult(value=int(source))
        return _conversion_failure(source, target)

    if target == "float":
        if type(source) is int and INT64_MIN <= source <= INT64_MAX:
            return ValueResult(value=float(source))
        if type(source) is float:
            return ValueResult(value=source)
        return _conversion_failure(source, target)

    if target == "date" and isinstance(source, str):
        try:
            return ValueResult(value=DateValue.parse(source))
        except ValueError:
            return _conversion_failure(source, target)

    if target == "datetime" and isinstance(source, str):
        try:
            return ValueResult(value=DateTimeValue.parse(source))
        except ValueError:
            return _conversion_failure(source, target)

    return _conversion_failure(source, target)


class TypedColumn(_FrozenModel):
    """One declared column in table order."""

    name: str = Field(min_length=1)
    type: ColumnType


class TypedTable(BaseModel):
    """A Polars frame paired with its ordered declared column contract."""

    model_config = ConfigDict(
        strict=True,
        extra="forbid",
        frozen=True,
        arbitrary_types_allowed=True,
    )

    columns: tuple[TypedColumn, ...]
    frame: Annotated[pl.DataFrame, InstanceOf[pl.DataFrame]]

    @model_validator(mode="after")
    def validate_column_order(self) -> TypedTable:
        declared = [column.name for column in self.columns]
        if len(declared) != len(set(declared)):
            raise ValueError("typed table column names must be unique")
        if self.frame.columns != declared:
            raise ValueError("Polars frame columns must match declared column order")
        frame = self.frame
        for column in self.columns:
            series = frame.get_column(column.name)
            if column.type != "float" or series.dtype != pl.Float64:
                continue
            values = series.to_list()
            # R011-9 through R011-15 make this normalization precede every
            # comparison, verification, key check, and artifact operation.
            if any(value is not None and not math.isfinite(value) for value in values):
                frame = frame.with_columns(
                    pl.Series(
                        column.name,
                        [
                            None
                            if value is not None and not math.isfinite(value)
                            else value
                            for value in values
                        ],
                        dtype=pl.Float64,
                        strict=True,
                    )
                )
        if frame is not self.frame:
            object.__setattr__(self, "frame", frame)
        return self
