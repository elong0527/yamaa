"""Value model for the clean-room engine."""

import datetime
import math
import re

INT64_MIN = -(2**63)
INT64_MAX = 2**63 - 1

_MISSING = None


def is_missing(v):
    return v is None


class YDate(datetime.date):
    """A date carrying its collected precision ('D', 'M', or 'Y'). R016."""

    def __new__(cls, year, month, day, precision="D"):
        self = super().__new__(cls, year, month, day)
        self.precision = precision
        return self

    def __repr__(self):
        return f"YDate({str(self)!r}, precision={self.precision!r})"


class YDateTime(datetime.datetime):
    """A local civil datetime (no zone), whole-second resolution. R016."""

    def __new__(
        cls, year, month, day, hour=0, minute=0, second=0, collected_precision="second"
    ):
        self = super().__new__(cls, year, month, day, hour, minute, second)
        self.collected_precision = collected_precision
        return self


def normalize_number(v):
    """R011-9: a non-finite float becomes missing."""
    if isinstance(v, float) and not math.isfinite(v):
        return None
    return v


def _is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def _is_num(v):
    return (_is_int(v) or isinstance(v, float)) and not isinstance(v, bool)


def comparable(a, b):
    """R011-35: int/float mutually comparable; every other type only with itself."""
    if is_missing(a) or is_missing(b):
        return True  # missing never reaches a comparison (UNKNOWN instead)
    if _is_num(a) and _is_num(b):
        return True
    if isinstance(a, str) and isinstance(b, str):
        return True
    return (isinstance(a, YDate) and isinstance(b, YDate)) or (
        isinstance(a, YDateTime) and isinstance(b, YDateTime)
    )


def compare(a, b):
    """Total order for mutually comparable non-missing values. R019-8 for str."""
    if _is_num(a) and _is_num(b):
        fa, fb = float(a), float(b)
        return (fa > fb) - (fa < fb)
    if isinstance(a, str) and isinstance(b, str):
        return (a > b) - (a < b)
    return (a > b) - (a < b)  # dates / datetimes


_DATE_RE = re.compile(r"^(\d{4})-(\d{2})-(\d{2})$")
_DATETIME_RE = re.compile(r"^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2}))?$")
_DATE_PREFIX_RE = re.compile(r"^(\d{4})(?:-(\d{2})(?:-(\d{2}))?)?$")


def parse_date(text):
    """Strict YYYY-MM-DD. Returns YDate or raises ValueError."""
    m = _DATE_RE.match(text)
    if not m:
        raise ValueError(f"not a date: {text!r}")
    return YDate(int(m.group(1)), int(m.group(2)), int(m.group(3)))


def parse_datetime(text):
    """Strict datetime per R016-11/13: omitted ss names second 00."""
    m = _DATETIME_RE.match(text)
    if not m:
        raise ValueError(f"not a datetime: {text!r}")
    y, mo, d, h, mi = (int(m.group(i)) for i in range(1, 6))
    s = int(m.group(6)) if m.group(6) is not None else 0
    return YDateTime(y, mo, d, h, mi, s)


def date_text(d):
    return f"{d.year:04d}-{d.month:02d}-{d.day:02d}"


def datetime_text(d):
    return f"{date_text(d)}T{d.hour:02d}:{d.minute:02d}:{d.second:02d}"


def date_prefix_parts(text):
    """Split an ISO date or date prefix into (year, month|None, day|None)."""
    m = _DATE_PREFIX_RE.match(text)
    if not m:
        return None
    return (
        int(m.group(1)),
        int(m.group(2)) if m.group(2) else None,
        int(m.group(3)) if m.group(3) else None,
    )


_INT_RE = re.compile(r"^[+-]?\d+$")
_FLOAT_RE = re.compile(r"^[+-]?(?:\d+\.\d*|\.\d+|\d+)(?:[eE][+-]?\d+)?$")
_NONFINITE_RE = re.compile(r"^[+-]?\.(?:inf|Inf|INF|nan|NaN|NAN)$")


def parse_int_text(text):
    """R011-21: R010 number with optional sign, no surrounding whitespace."""
    if _INT_RE.match(text):
        return int(text)
    raise ValueError(f"not an int: {text!r}")


def parse_float_text(text):
    if _NONFINITE_RE.match(text):
        return None  # normalized to missing before conversion continues
    if _FLOAT_RE.match(text):
        return normalize_number(float(text))
    raise ValueError(f"not a float: {text!r}")


def float_text(x):
    """R011-26/27 + R020-19: shortest round-trip digits, positional, no exponent."""
    s = repr(float(x))
    if "e" not in s and "E" not in s:
        return s.removesuffix(".0")
    neg = s.startswith("-")
    if neg:
        s = s[1:]
    mant, exp = re.split("[eE]", s)
    exp = int(exp)
    if "." in mant:
        intpart, fracpart = mant.split(".")
    else:
        intpart, fracpart = mant, ""
    digits = intpart + fracpart
    point = len(intpart) + exp  # digits before the decimal point
    if point <= 0:
        out = "0." + "0" * (-point) + digits
    elif point >= len(digits):
        out = digits + "0" * (point - len(digits))
    else:
        out = digits[:point] + "." + digits[point:]
    out = out.rstrip("0").rstrip(".") if "." in out else out
    if out in ("", "-"):
        out = "0"
    return ("-" if neg else "") + out


def ascii_upper(s):
    return "".join(chr(ord(c) - 32) if "a" <= c <= "z" else c for c in s)


def ascii_lower(s):
    return "".join(chr(ord(c) + 32) if "A" <= c <= "Z" else c for c in s)


def ascii_fold(s):
    """R019 ASCII case folding (a-z -> A-Z)."""
    return ascii_upper(s)
