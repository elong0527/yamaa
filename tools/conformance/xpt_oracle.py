"""Encode SAS Transport version 5 files from hand-authored tables.

An independent test oracle written from the public TS-140 record layout: 80-byte
records, 140-byte NAMESTR descriptors and IBM System/360 doubles computed with
exact rational arithmetic. It imports nothing from yamaa and reads no engine
output; the committed ``.xpt`` goldens of the transport benchmarks are written
by ``python -m tools.conformance.xpt_goldens`` from the literal tables there.
"""

import datetime
import struct
from dataclasses import dataclass
from fractions import Fraction

RECORD = 80
NAMESTR = 140
MONTHS = "JAN FEB MAR APR MAY JUN JUL AUG SEP OCT NOV DEC".split()
SAS_ORIGIN = datetime.date(1960, 1, 1)


@dataclass(frozen=True)
class Format:
    """A SAS display format: name, width and decimals (an empty name is w.d)."""

    name: str
    width: int
    decimals: int = 0


@dataclass(frozen=True)
class Variable:
    """One NAMESTR: a character variable has a width; a numeric one is 8 bytes."""

    name: str
    label: str
    character: bool
    width: int = 8
    format: Format | None = None


def record(content: bytes) -> bytes:
    """One record, blank padded; a record never overflows."""
    if len(content) > RECORD:
        raise ValueError("record longer than 80 bytes")
    return content.ljust(RECORD, b" ")


def ascii_field(text: str, width: int) -> bytes:
    """Printable ASCII text left aligned and blank padded to its field."""
    data = text.encode("ascii")
    if len(data) > width or any(not 0x20 <= byte <= 0x7E for byte in data):
        raise ValueError(f"{text!r} does not fit a {width}-byte transport field")
    return data.ljust(width, b" ")


def header(kind: str, counts: str) -> bytes:
    return record(
        b"HEADER RECORD*******"
        + ascii_field(kind, 8)
        + b"HEADER RECORD!!!!!!!"
        + counts.encode("ascii")
    )


def header_time(moment: datetime.datetime) -> bytes:
    """``ddMMMyy:hh:mm:ss`` with an upper-case month."""
    text = (
        f"{moment.day:02d}{MONTHS[moment.month - 1]}{moment.year % 100:02d}:"
        f"{moment.hour:02d}:{moment.minute:02d}:{moment.second:02d}"
    )
    return text.encode("ascii")


def ibm(value) -> bytes:
    """An IBM System/360 double: sign, excess-64 base-16 exponent, 56-bit fraction.

    ``None`` is the ordinary missing value. Any Python int or float converts
    exactly or raises; zero of either sign is eight zero bytes.
    """
    if value is None:
        return b"." + bytes(7)
    exact = Fraction(value)
    if exact == 0:
        return bytes(8)
    sign = 0x80 if exact < 0 else 0
    exact = abs(exact)
    exponent = 0
    while exact >= 1:
        exact /= 16
        exponent += 1
    while exact < Fraction(1, 16):
        exact *= 16
        exponent -= 1
    if not -64 <= exponent <= 63:
        raise ValueError(f"{value!r} has no IBM double")
    fraction = exact * (1 << 56)
    if fraction.denominator != 1:
        raise ValueError(f"{value!r} does not convert exactly")
    return bytes([sign | (exponent + 64)]) + int(fraction).to_bytes(7, "big")


def sas_date(day: datetime.date) -> int:
    """Days since 1960-01-01."""
    return (day - SAS_ORIGIN).days


def sas_datetime(moment: datetime.datetime) -> int:
    """Whole seconds since 1960-01-01T00:00:00, without a zone."""
    elapsed = moment - datetime.datetime(1960, 1, 1)
    return elapsed.days * 86400 + elapsed.seconds


def namestr(index: int, variable: Variable, position: int) -> bytes:
    """One 140-byte NAMESTR; integers are big-endian, unused bytes are zero."""
    form = variable.format or Format("", 0, 0)
    return struct.pack(
        ">hhhh8s40s8shhh2s8shhi52s",
        2 if variable.character else 1,
        0,
        variable.width,
        index + 1,
        ascii_field(variable.name, 8),
        ascii_field(variable.label, 40),
        ascii_field(form.name, 8),
        form.width,
        form.decimals,
        0,
        b"",
        b" " * 8,
        0,
        0,
        position,
        b"",
    )


def encode(
    name: str,
    label: str,
    created: datetime.datetime,
    variables: list[Variable],
    rows: list[list],
) -> bytes:
    """One member: headers, NAMESTRs, the observation header and observations.

    A character cell is text or ``None`` (missing, all blanks); a numeric cell
    is an int, a float or ``None``.
    """
    stamp = header_time(created)
    system = b"SAS     SAS     SASLIB  9.4     yamaa   " + b" " * 24
    member = (
        b"SAS     " + ascii_field(name, 8) + b"SASDATA 9.4     yamaa   " + b" " * 24
    )
    output = bytearray()
    output += header("LIBRARY", "0" * 30)
    output += record(system + stamp)
    output += record(stamp)
    output += header("MEMBER", "000000000000000001600000000140")
    output += header("DSCRPTR", "0" * 30)
    output += record(member + stamp)
    output += record(stamp + b" " * 16 + ascii_field(label, 40) + b" " * 8)
    output += header("NAMESTR", f"000000{len(variables):04d}" + "0" * 20)
    descriptors = bytearray()
    position = 0
    for index, variable in enumerate(variables):
        descriptors += namestr(index, variable, position)
        position += variable.width
    output += descriptors.ljust(-(-len(descriptors) // RECORD) * RECORD, b" ")
    output += header("OBS", "0" * 30)
    observations = bytearray()
    for row in rows:
        if len(row) != len(variables):
            raise ValueError("row width differs from the variables")
        for value, variable in zip(row, variables):
            if variable.character:
                observations += ascii_field(
                    "" if value is None else value, variable.width
                )
            else:
                observations += ibm(value)
    output += observations.ljust(-(-len(observations) // RECORD) * RECORD, b" ")
    return bytes(output)


def file_size(variables: list[Variable], rows: int) -> int:
    """Bytes in a member, computed before encoding."""
    width = sum(variable.width for variable in variables)
    padded = lambda count: -(-count // RECORD) * RECORD  # noqa: E731
    return 8 * RECORD + padded(NAMESTR * len(variables)) + RECORD + padded(rows * width)
