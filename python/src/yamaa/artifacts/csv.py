"""The R020 csv profile: the exact bytes a reviewable artifact becomes.

The reader for the delimited *source* profile lives in `yamaa.io.csv` and
imports the standard library alone, because the repository validator loads
it by path. This module writes instead, and writing has its own contract:
R020 fixes every byte, quotes an exact set of fields, keeps a collected
empty string apart from a missing value, and rounds a float once, here.
"""

from __future__ import annotations

import decimal

from yamaa.artifacts.output import Artifact
from yamaa.io.polars import runtime_value
from yamaa.models import (
    MISSING,
    DateTimeValue,
    DateValue,
    RuntimeValue,
    ValueResult,
    convert_value,
)

# R020-14 states the quoting condition exactly rather than as a minimum, so
# two runtimes quote the same fields.
_QUOTED = ('"', ",", "\r", "\n")


def _quote(text: str) -> str:
    if text == "" or any(character in text for character in _QUOTED):
        return '"' + text.replace('"', '""') + '"'
    return text


def fixed_point(value: float, decimals: int) -> str:
    """Round one binary64 value to `decimals` places, exactly and once.

    R020-33 rounds the exact decimal value every binary64 is, with a tie
    going away from zero. `Decimal(value)` is that exact value, and
    `ROUND_HALF_UP` is the away-from-zero tie both `round` builtins and the
    C formatting beneath them decide the other way. The context is widened
    to the digits this value and this precision need, so a large declared
    precision stays exact rather than reaching a host limit.
    """
    exact = decimal.Decimal(value)
    with decimal.localcontext() as context:
        context.prec = max(exact.adjusted(), 0) + decimals + 3
        context.Emin = decimal.MIN_EMIN
        context.Emax = decimal.MAX_EMAX
        quantized = exact.quantize(
            decimal.Decimal(1).scaleb(-decimals),
            rounding=decimal.ROUND_HALF_UP,
        )
    text = format(quantized, "f")
    # R020-33: a value that rounds to zero is written without a sign.
    return text[1:] if quantized == 0 and text.startswith("-") else text


def _text(value: RuntimeValue, decimals: int | None) -> str | None:
    """Return the field's text, or None for a missing value (R020-17, R020-18)."""
    if value is MISSING:
        return None
    if isinstance(value, str):
        return value
    if isinstance(value, (DateValue, DateTimeValue)):
        return value.to_text()
    if isinstance(value, float) and decimals is not None:
        return fixed_point(value, decimals)
    # R011 owns an `int`'s digits and a `float`'s shortest round-tripping
    # spelling; this profile writes that text rather than a second one.
    converted = convert_value(value, "str")
    assert isinstance(converted, ValueResult)
    assert isinstance(converted.value, str)
    return converted.value


def render_csv(artifact: Artifact) -> bytes:
    """Render one artifact to the exact bytes R020's csv profile fixes."""
    names = [column.name for column in artifact.columns]
    records = [",".join(_quote(name) for name in names)]
    for row in artifact.frame.iter_rows():
        fields = [
            "" if text is None else _quote(text)
            for text in (
                _text(runtime_value(value), artifact.decimals) for value in row
            )
        ]
        records.append(",".join(fields))
    # R020-9: U+000A terminates every record, including the last, and
    # R020-13 keeps the header record of an artifact that holds no row.
    return "".join(f"{record}\n" for record in records).encode("utf-8")
