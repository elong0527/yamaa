"""Render completed typed tables to the exact R020 CSV profile."""

from __future__ import annotations

import datetime as dt
import math
from collections.abc import Sequence

from yamaa.io.publish import ArtifactError
from yamaa.models import MISSING, TypedTable, ValueResult, convert_value
from yamaa.specification.models import ColumnType


def _value_text(
    value: object,
    column_type: ColumnType,
    decimals: int | None,
) -> str | None:
    if value is None or value is MISSING:
        return None
    if column_type == "str" and isinstance(value, str):
        return value
    if column_type == "int" and type(value) is int:
        return str(value)
    if column_type == "float" and type(value) is float:
        if not math.isfinite(value):
            return None
        if decimals is not None:
            return _fixed_point(value, decimals)
        converted = convert_value(value, "str")
        assert isinstance(converted, ValueResult)
        text = converted.value
        assert isinstance(text, str)
        return text
    if column_type == "date" and type(value) is dt.date:
        return value.isoformat()
    if (
        column_type == "datetime"
        and type(value) is dt.datetime
        and value.tzinfo is None
        and value.microsecond == 0
    ):
        return value.isoformat(timespec="seconds")
    raise ArtifactError("unwritable_value", value)


def _fixed_point(value: float, decimals: int) -> str:
    numerator, denominator = value.as_integer_ratio()
    binary_places = denominator.bit_length() - 1
    exact_digits = abs(numerator) * (5**binary_places)

    if decimals >= binary_places:
        digits = str(exact_digits) + ("0" * (decimals - binary_places))
        rounded_nonzero = exact_digits != 0
    else:
        divisor = 10 ** (binary_places - decimals)
        quotient, remainder = divmod(exact_digits, divisor)
        if 2 * remainder >= divisor:
            quotient += 1
        digits = str(quotient)
        rounded_nonzero = quotient != 0

    sign = "-" if numerator < 0 and rounded_nonzero else ""
    digits = digits.rjust(decimals + 1, "0")
    if decimals == 0:
        return f"{sign}{digits}"
    return f"{sign}{digits[:-decimals]}.{digits[-decimals:]}"


def _quote_field(text: str) -> str:
    if text == "" or any(character in text for character in ('"', ",", "\r", "\n")):
        return '"' + text.replace('"', '""') + '"'
    return text


def render_csv(
    table: TypedTable,
    columns: Sequence[str],
    keys: Sequence[str],
    decimals: int | None,
) -> bytes:
    """Render output columns in order to the exact R020 CSV bytes."""
    if decimals is not None and (type(decimals) is not int or decimals < 0):
        raise ArtifactError("invalid_output_decimals", decimals)
    by_name = {column.name: column for column in table.columns}
    for name in columns:
        if name not in by_name:
            raise ArtifactError("unknown_output_column", name)
    lines = [",".join(_quote_field(name) for name in columns)]
    for row in table.frame.iter_rows(named=True):
        fields = []
        for name in columns:
            try:
                text = _value_text(row[name], by_name[name].type, decimals)
            except ArtifactError as error:
                error.column = name
                error.keys = dict(
                    zip(keys, (row.get(key) for key in keys), strict=True)
                )
                raise
            fields.append("" if text is None else _quote_field(text))
        lines.append(",".join(fields))
    return ("\n".join(lines) + "\n").encode()
