"""R020 csv profile writer: completed tables to exact bytes.

Missing is always written bare. A collected empty string keeps its two
quote characters under the exact quoting condition below.
"""

from __future__ import annotations

import math
from collections.abc import Mapping, Sequence
from fractions import Fraction

from yamaa.models import (
    MISSING,
    DateTimeValue,
    DateValue,
    TypedTable,
    ValueResult,
    convert_value,
)

_MISSING = MISSING


class ArtifactError(ValueError):
    """A value the artifact profile cannot carry, or a failed publication."""

    def __init__(
        self, condition: str, target: str, context: Mapping[str, object] | None = None
    ) -> None:
        self.condition = condition
        self.target = target
        self.context = dict(context or {})
        super().__init__(f"{condition}: {target}")


def _missing(value: object) -> bool:
    return value is MISSING or value is None


def _plain_text(value: object) -> str | None:
    if _missing(value):
        return None
    if isinstance(value, str):
        return value
    if isinstance(value, bool):
        raise ArtifactError("unwritable_value", "", {"value": repr(value)})
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        if not math.isfinite(value):
            return None
        converted = convert_value(value, "str")
        assert isinstance(converted, ValueResult)
        text = converted.value
        assert isinstance(text, str)
        return text
    if isinstance(value, (DateValue, DateTimeValue)):
        return value.to_text()
    raise ArtifactError("unwritable_value", "", {"value": type(value).__name__})


def _fixed_point(value: float, decimals: int) -> str:
    scaled = Fraction(value) * (10**decimals)
    numerator = abs(scaled.numerator)
    quotient, remainder = divmod(numerator, scaled.denominator)
    if 2 * remainder >= scaled.denominator:
        quotient += 1
    sign = "-" if scaled < 0 and quotient > 0 else ""
    digits = str(quotient).rjust(decimals + 1, "0")
    if decimals == 0:
        return f"{sign}{digits}"
    return f"{sign}{digits[:-decimals]}.{digits[-decimals:]}"


def _field_text(value: object, column_type: str, decimals: int | None) -> str | None:
    if _missing(value):
        return None
    if column_type == "float" and decimals is not None:
        assert isinstance(value, float)
        if not math.isfinite(value):
            return None
        return _fixed_point(value, decimals)
    return _plain_text(value)


def _quote(text: str) -> str:
    if text == "" or any(character in text for character in '",\r\n'):
        return '"' + text.replace('"', '""') + '"'
    return text


def render_csv(
    table: TypedTable,
    columns: Sequence[str],
    keys: Sequence[str],
    decimals: int | None,
) -> bytes:
    """Render the artifact's columns to the exact R020 csv bytes."""
    if decimals is not None and decimals < 0:
        raise ArtifactError("invalid_decimals", "", {"decimals": decimals})
    by_name = {column.name: column for column in table.columns}
    for name in columns:
        if name not in by_name:
            raise ArtifactError("unknown_output_column", name)
    for name in keys:
        if name not in by_name:
            raise ArtifactError("unknown_key_column", name)
    lines = [",".join(_quote(name) for name in columns)]
    for record in table.frame.iter_rows(named=True):
        fields = []
        for name in columns:
            try:
                text = _field_text(record[name], by_name[name].type, decimals)
            except ArtifactError as error:
                context = {
                    "column": name,
                    "value": repr(record[name]),
                    "keys": {key: record[key] for key in keys},
                }
                raise ArtifactError(
                    error.condition or "unwritable_value", name, context
                ) from error
            fields.append("" if text is None else _quote(text))
        lines.append(",".join(fields))
    return ("\n".join(lines) + "\n").encode("utf-8")
