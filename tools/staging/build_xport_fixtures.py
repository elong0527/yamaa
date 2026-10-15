"""Author small XPT binary controls directly from the documented transport layout.

This fixture writer does not import or invoke ReadStat or the yamaa reader.
The accompanying literal expected tables are the independent test oracle.
"""

import struct
from fractions import Fraction

from tools.paths import ROOT

FIXTURES = ROOT / "rust/crates/yamaa-adapters/tests/fixtures/sas"


def record(value):
    return value.ljust(80, b" ")


def header(name, variables=0):
    return record(
        b"HEADER RECORD*******"
        + name.encode().ljust(8)
        + b"HEADER RECORD!!!!!!!"
        + f"{0:05}{variables:05}{0:05}{0:05}{0:05}{0:05}".encode()
    )


def ibm(value):
    if isinstance(value, str):
        return value.encode() + bytes(7)
    value = Fraction(value)
    if not value:
        return bytes(8)
    sign = 0x80 if value < 0 else 0
    value = abs(value)
    exponent = 64
    while value >= 1:
        value /= 16
        exponent += 1
    while value < Fraction(1, 16):
        value *= 16
        exponent -= 1
    mantissa = value * (1 << 56)
    assert mantissa.denominator == 1
    return bytes([sign | exponent]) + int(mantissa).to_bytes(7, "big")


def xport(version, rows, columns):
    names = {
        5: ["LIBRARY", "MEMBER", "DSCRPTR", "NAMESTR", "OBS"],
        8: ["LIBV8", "MEMBV8", "DSCPTV8", "NAMSTV8", "OBSV8"],
    }[version]
    output = header(names[0]) + record(b"SAS     SAS     SASLIB  6.06    fixture")
    output += record(b"01JAN60:00:00:00")
    output += header(names[1]) + header(names[2])
    output += record(b"SAS     FIXTURE ") + record(b"01JAN60:00:00:00")
    output += header(names[3], len(columns))
    descriptors = bytearray()
    position = 0
    for index, (name, text, width) in enumerate(columns):
        descriptor = bytearray(140)
        struct.pack_into(">HHHH", descriptor, 0, 2 if text else 1, 0, width, index + 1)
        descriptor[8:16] = name.encode()[:8].ljust(8)
        descriptor[16:56] = b"Independent authored fixture".ljust(40)
        # Deliberately misleading display format: it must not infer date typing.
        descriptor[56:64] = b"DATE    " if not text else b"        "
        descriptor[72:80] = b"        "
        struct.pack_into(">I", descriptor, 84, position)
        if version == 8:
            descriptor[88:120] = name.encode().ljust(32)
        descriptors.extend(descriptor)
        position += width
    output += bytes(descriptors).ljust((len(descriptors) + 79) // 80 * 80)
    output += header(names[4])
    for row in rows:
        for value, (_, text, width) in zip(row, columns, strict=True):
            output += value.ljust(width, b" ") if text else ibm(value)[:width]
    return output.ljust((len(output) + 79) // 80 * 80, b" ")


if __name__ == "__main__":
    columns = [
        ("ID", False, 8),
        ("TEXT", True, 16),
        ("NUM", False, 8),
        ("DAY", False, 8),
        ("STAMP", False, 8),
    ]
    rows = [
        [2, b" caf\xe9", Fraction(3, 2), 0, 0],
        [1, b"", "Z", 3653, -1],
        [3, b".", -2, -1, 1],
    ]
    for version in [5, 8]:
        actual = (
            columns
            if version == 5
            else [("ID", False, 8), ("LONG_TEXT_NAME", True, 16), *columns[2:]]
        )
        payload = xport(version, rows, actual)
        (FIXTURES / f"v{version}.xpt").write_bytes(payload)
        (FIXTURES / f"multi-v{version}.xpt").write_bytes(payload + payload[240:])
    (FIXTURES / "precision.xpt").write_bytes(
        xport(5, [[2**53 + 1]], [("NUM", False, 8)])
    )
    (FIXTURES / "decimal.xpt").write_bytes(
        xport(5, [[Fraction(0.1)]], [("NUM", False, 8)])
    )
    (FIXTURES / "member-text.xpt").write_bytes(
        xport(5, [[header("MEMBER"), 1]], [("TEXT", True, 80), ("NUM", False, 8)])
    )
    (FIXTURES / "fractional.xpt").write_bytes(
        xport(5, [[Fraction(3, 2)]], [("NUM", False, 8)])
    )
    (FIXTURES / "missing.xpt").write_bytes(
        xport(
            5,
            [[value] for value in [".", "_", *"ABCDEFGHIJKLMNOPQRSTUVWXYZ"]],
            [("NUM", False, 8)],
        )
    )
    utf8_rows = [
        [2, " caf\xe9".encode(), Fraction(3, 2), 0, 0],
        [1, b"", "Z", 3653, -1],
        [3, b".", -2, -1, 1],
    ]
    (FIXTURES / "utf8.xpt").write_bytes(xport(5, utf8_rows, columns))
    (FIXTURES / "integer-bounds.xpt").write_bytes(
        xport(
            5,
            [[value] for value in [-(2**63), 0, 2**53, 2**63 - 1024]],
            [("NUM", False, 8)],
        )
    )
    (FIXTURES / "integer-overflow.xpt").write_bytes(
        xport(5, [[2**63]], [("NUM", False, 8)])
    )
    if (FIXTURES / "rle.sas7bdat").exists():
        sas = (FIXTURES / "rle.sas7bdat").read_bytes()
        assert b"SASYZCRL" in sas
        (FIXTURES / "unsupported-compression.sas7bdat").write_bytes(
            sas.replace(b"SASYZCRL", b"UNKNOWN!", 1)
        )
