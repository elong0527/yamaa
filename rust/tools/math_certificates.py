"""Independent, bounded binary64 certificates for EXP, LN and POWER samples.

This helper never calls platform math or libm and never uses a candidate result
to select expected truth. It does not qualify a function universally. Every
returned certificate applies only to its exact input bits. Domain, missing and
identity observations are explicitly language evidence, not accuracy evidence.

Decimal EXP/LN are documented correctly rounded with ROUND_HALF_EVEN. POWER
deliberately uses the pure Python _pydecimal Context.power: the documented
guarantee for that implementation is correctly rounded, whereas the C version
is only almost always correctly rounded. See:
https://docs.python.org/3/library/decimal.html#decimal.Decimal.exp
https://docs.python.org/3/library/decimal.html#decimal.Decimal.ln
https://docs.python.org/3/library/decimal.html#decimal.Context.power

A 160-digit result encloses the mathematical result within half one decimal
ULP. Exact Fraction comparisons must place that entire closed enclosure in one
binary64 nearest/ties-even rounding cell, or the sample is excluded. Bounded
integral powers instead use exact rational arithmetic, including underflow ties.
The source of these numerical guarantees is the documented Decimal contract;
the helper is independent of the candidate, not a formal proof of Decimal.
"""

import _pydecimal
import decimal
import re
import struct
from fractions import Fraction

PRECISION = 160
MAX_EXP_INPUT = 1000
MAX_POWER_EXPONENT = 2048
MAX_POWER_LOG2_RANGE = 4096
MAX_RATIONAL_BITS = 16384
SIGN = 1 << 63
MAX_FINITE = 0x7FEFFFFFFFFFFFFF
POSITIVE_INFINITY = 0x7FF0000000000000
MIN_SUBNORMAL = Fraction(1, 1 << 1074)
OVERFLOW_THRESHOLD = Fraction((1 << 1024) - (1 << 970))
_BITS = re.compile(r"[0-9a-f]{16}\Z")


def _decode(bits):
    """Decode canonical finite bits with struct; missing is a language sentinel."""
    if bits == "missing":
        return None
    if not isinstance(bits, str) or not _BITS.fullmatch(bits):
        raise ValueError("binary64 bits must be sixteen lowercase hexadecimal digits")
    value = struct.unpack(">d", bytes.fromhex(bits))[0]
    if int(bits, 16) & POSITIVE_INFINITY == POSITIVE_INFINITY:
        raise ValueError("nonfinite bits must be represented by missing")
    return value


def _fraction(bits):
    """Obtain an exact rational from a finite binary64 encoding."""
    return Fraction.from_float(struct.unpack(">d", int(bits).to_bytes(8, "big"))[0])


def _power_of_two(exponent):
    """Construct an exact binary scale without floating-point arithmetic."""
    return Fraction(1 << exponent) if exponent >= 0 else Fraction(1, 1 << -exponent)


def _floor_log2(value):
    """Compute floor(log2(value)) by exact integer comparisons for positive rationals."""
    exponent = value.numerator.bit_length() - value.denominator.bit_length()
    if value < _power_of_two(exponent):
        exponent -= 1
    return exponent


def _round_integer(value):
    """Round a nonnegative rational to an integer with ties to even."""
    whole, remainder = divmod(value.numerator, value.denominator)
    twice = remainder * 2
    return whole + (
        twice > value.denominator or (twice == value.denominator and whole % 2)
    )


def _nearest_bits(value, negative_zero=False):
    """Round exactly to binary64, including signed zero and overflow ties."""
    negative = value < 0 or (value == 0 and negative_zero)
    magnitude = abs(value)
    sign = SIGN if negative else 0
    if magnitude == 0:
        return sign
    exponent = _floor_log2(magnitude)
    if exponent < -1022:
        significand = _round_integer(magnitude / MIN_SUBNORMAL)
        return sign | significand
    if exponent > 1023:
        return sign | POSITIVE_INFINITY
    significand = _round_integer(magnitude / _power_of_two(exponent - 52))
    if significand == 1 << 53:
        exponent += 1
        significand = 1 << 52
    if exponent > 1023:
        return sign | POSITIVE_INFINITY
    return sign | ((exponent + 1023) << 52) | (significand - (1 << 52))


def _rounding_cell(bits):
    """Return exact bounds and tie inclusion for one signed binary64 rounding cell."""
    magnitude = bits & (SIGN - 1)
    if magnitude == POSITIVE_INFINITY:
        lower, upper = OVERFLOW_THRESHOLD, None
        lower_included, upper_included = True, False
    elif magnitude == 0:
        lower, upper = Fraction(0), MIN_SUBNORMAL / 2
        lower_included = upper_included = True
    else:
        center = _fraction(magnitude)
        lower = (_fraction(magnitude - 1) + center) / 2
        upper = (
            OVERFLOW_THRESHOLD
            if magnitude == MAX_FINITE
            else (center + _fraction(magnitude + 1)) / 2
        )
        lower_included = upper_included = magnitude % 2 == 0
    if bits & SIGN:
        lower, upper = -upper if upper is not None else None, -lower
        lower_included, upper_included = upper_included, lower_included
    return lower, upper, lower_included, upper_included


def _inside_cell(lower, upper, bits):
    """Admit only a closed enclosure wholly inside the exact rounding cell."""
    cell_lower, cell_upper, include_lower, include_upper = _rounding_cell(bits)
    lower_ok = (
        cell_lower is None
        or lower > cell_lower
        or (include_lower and lower == cell_lower)
    )
    upper_ok = (
        cell_upper is None
        or upper < cell_upper
        or (include_upper and upper == cell_upper)
    )
    return lower <= upper and lower_ok and upper_ok


def _ratio(value):
    """Serialize exact rational proof bounds without decimal integer digit limits."""
    if value is None:
        return None
    return {
        "numerator_hex": format(value.numerator, "x"),
        "denominator_hex": format(value.denominator, "x"),
    }


def _result(bits):
    """Apply the engine's stated nonfinite-to-missing result convention."""
    return (
        "missing" if bits & POSITIVE_INFINITY == POSITIVE_INFINITY else f"{bits:016x}"
    )


def _language(evidence, expected, reason):
    """Keep semantic expectations separate from a numerical nearest-value claim."""
    evidence.update(
        status="language",
        method="language-semantics",
        language_expected=expected,
        reason=reason,
    )
    if expected == "missing" or _BITS.fullmatch(expected):
        evidence["comparison"] = (
            "matches" if evidence["candidate"] == expected else "differs"
        )
    return evidence


def _excluded(evidence, reason):
    """Retain an explicit exclusion instead of treating an undecided case as correct."""
    evidence.update(status="excluded", reason=reason)
    return evidence


def _certified(evidence, lower, upper, method, details):
    """Choose expected bits from independent bounds, then compare the candidate."""
    center = (lower + upper) / 2
    bits = _nearest_bits(center)
    if not _inside_cell(lower, upper, bits):
        return _excluded(
            evidence, "independent enclosure crosses a binary64 rounding boundary"
        )
    cell_lower, cell_upper, include_lower, include_upper = _rounding_cell(bits)
    nearest = _result(bits)
    evidence.update(
        status="certified",
        method=method,
        nearest_binary64=nearest,
        comparison="matches" if evidence["candidate"] == nearest else "differs",
        proof={
            **details,
            "rounding": "nearest, ties to even",
            "enclosure": {"lower": _ratio(lower), "upper": _ratio(upper)},
            "rounding_cell": {
                "lower": _ratio(cell_lower),
                "upper": _ratio(cell_upper),
                "lower_included": include_lower,
                "upper_included": include_upper,
            },
            "rounded_bits_before_nonfinite_normalization": f"{bits:016x}",
        },
    )
    return evidence


def _decimal_certificate(evidence, function, values):
    """Enclose one documented correctly rounded Decimal operation at fixed precision."""
    implementation = _pydecimal if function == "POWER" else decimal
    context = implementation.Context(
        prec=PRECISION,
        rounding=implementation.ROUND_HALF_EVEN,
        Emin=-10000,
        Emax=10000,
    )
    operands = [implementation.Decimal.from_float(value) for value in values]
    try:
        rounded = (
            context.exp(operands[0])
            if function == "EXP"
            else context.ln(operands[0])
            if function == "LN"
            else context.power(operands[0], operands[1])
        )
    except implementation.DecimalException as error:
        return _excluded(evidence, f"Decimal operation signaled {type(error).__name__}")
    if (
        not rounded.is_finite()
        or rounded.is_zero()
        or context.flags[implementation.Subnormal]
    ):
        return _excluded(
            evidence, "Decimal result is outside the admitted normal nonzero range"
        )
    value = Fraction(*rounded.as_integer_ratio())
    ulp_exponent = rounded.adjusted() - PRECISION + 1
    radius = (
        Fraction(10**ulp_exponent, 2)
        if ulp_exponent >= 0
        else Fraction(1, 2 * 10**-ulp_exponent)
    )
    return _certified(
        evidence,
        value - radius,
        value + radius,
        "python-decimal-power-enclosure"
        if function == "POWER"
        else "decimal-enclosure",
        {
            "precision_decimal_digits": PRECISION,
            "input_decimals": [str(value) for value in operands],
            "rounded_decimal": str(rounded),
            "half_decimal_ulp": _ratio(radius),
            "decimal_implementation": "_pydecimal"
            if function == "POWER"
            else "decimal",
        },
    )


def certify(function: str, inputs: list[str], result: str) -> dict:
    """Certify one exact-input sample or return an explicit semantic/range exclusion.

    Unary inputs may have one operand or the assessment's two-operand form with
    an unused +0 second operand. POWER requires two operands. All bits are finite
    canonical binary64 encodings; the literal missing denotes a missing operand
    or result. Candidate results are inspected only after independent truth is
    established. Malformed evidence raises ValueError rather than losing a case.
    """
    if function not in ("EXP", "LN", "POWER"):
        raise ValueError(f"unsupported certificate function {function!r}")
    if not isinstance(inputs, list) or len(inputs) not in (
        (2,) if function == "POWER" else (1, 2)
    ):
        raise ValueError("incorrect certificate operand count")
    if function != "POWER" and len(inputs) == 2 and inputs[1] != "0000000000000000":
        raise ValueError("the unused unary assessment operand must be +0")
    values = [_decode(bits) for bits in inputs]
    _decode(result)
    evidence = {
        "schema_version": 1,
        "function": function,
        "inputs": list(inputs),
        "candidate": result,
        "status": "excluded",
        "method": None,
        "nearest_binary64": None,
        "comparison": "not-assessed",
    }
    if any(value is None for value in values):
        return _language(
            evidence, "missing", "missing propagates after all arguments evaluate"
        )
    base = values[0]
    if function == "EXP":
        if base == 0:
            return _language(evidence, "3ff0000000000000", "EXP of either zero is one")
        if abs(base) > MAX_EXP_INPUT:
            return _excluded(
                evidence,
                f"EXP input magnitude exceeds bounded certificate range {MAX_EXP_INPUT}",
            )
    elif function == "LN":
        if base <= 0:
            return _language(
                evidence, "REQ-0432", "LN rejects nonpositive finite inputs"
            )
        if base == 1:
            return _language(evidence, "0000000000000000", "LN of one is positive zero")
    else:
        exponent = values[1]
        integral = exponent.is_integer()
        if (base == 0 and exponent < 0) or (base < 0 and not integral):
            return _language(
                evidence,
                "REQ-0433",
                "POWER rejects zero-negative and negative-fractional domains",
            )
        if exponent == 0 or base == 1:
            return _language(
                evidence,
                "3ff0000000000000",
                "POWER zero exponent or unit base is one, including zero to zero",
            )
        if exponent == 1:
            return _language(
                evidence,
                inputs[0],
                "POWER unit exponent preserves its finite base and zero sign",
            )
        if base == 0:
            negative_zero = (
                bool(int(inputs[0], 16) & SIGN) and integral and int(exponent) % 2 == 1
            )
            return _language(
                evidence,
                "8000000000000000" if negative_zero else "0000000000000000",
                "POWER positive exponent of zero follows integral odd-exponent zero sign",
            )
        if base == -1:
            return _language(
                evidence,
                "bff0000000000000" if int(exponent) % 2 else "3ff0000000000000",
                "POWER negative unit base with an integral exponent is exactly signed one",
            )
        if abs(exponent) > MAX_POWER_EXPONENT:
            return _excluded(
                evidence,
                f"POWER exponent magnitude exceeds work bound {MAX_POWER_EXPONENT}",
            )
        rational_base = Fraction.from_float(base)
        if integral:
            exponent = int(exponent)
            projected_bits = abs(exponent) * max(
                abs(rational_base.numerator).bit_length(),
                rational_base.denominator.bit_length(),
            )
            if projected_bits > MAX_RATIONAL_BITS:
                return _excluded(
                    evidence,
                    f"exact POWER rational exceeds work bound {MAX_RATIONAL_BITS} bits",
                )
            exact = rational_base**exponent
            return _certified(
                evidence,
                exact,
                exact,
                "exact-rational-power",
                {"integral_exponent": exponent},
            )
        binary_exponent = _floor_log2(rational_base)
        exact_exponent = Fraction.from_float(exponent)
        if (
            max(
                abs(binary_exponent * exact_exponent),
                abs((binary_exponent + 1) * exact_exponent),
            )
            > MAX_POWER_LOG2_RANGE
        ):
            return _excluded(
                evidence,
                f"POWER result log2 enclosure exceeds work bound {MAX_POWER_LOG2_RANGE}",
            )
    return _decimal_certificate(
        evidence, function, values[:2] if function == "POWER" else values[:1]
    )
