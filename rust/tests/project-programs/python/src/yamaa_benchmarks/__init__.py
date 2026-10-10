"""Project calculations used by the original function benchmarks."""


def bmi(weight_kg, height_cm, cm_per_m=100):
    """Return body mass index in kg/m2 from kilograms and centimetres."""
    return weight_kg / (height_cm / cm_per_m) ** 2


def project_ratio(numerator, denominator, decimals=2, adjust=0.0, as_percent=False):
    """Return numerator/denominator, optionally scaled to a percentage.

    The quotient is rounded to `decimals` places and shifted by `adjust`;
    a missing adjustment (None) counts as zero.
    """
    value = numerator / denominator
    if as_percent:
        value *= 100.0
    value = round(value, decimals)
    if adjust is None:
        adjust = 0.0
    return value + adjust


_CONSTANTS = {
    "zero": 0.0,
    "one": 1.0,
    "positive-infinity": float("inf"),
    "negative-infinity": float("-inf"),
    "nan": float("nan"),
}


def numeric_constant(kind):
    """Return the numeric constant `kind` names."""
    return _CONSTANTS[kind]


_LN2 = 0.6931471805599453
_SQRT_2 = 1.4142135623730951
_TWO_OVER_SQRT_PI = 1.1283791670955126
_ONE_OVER_SQRT_PI = 0.5641895835477563

# Beyond this the error-function series alternates in terms far larger than
# its sum, so the continued fraction answers for the tail instead.
_SERIES_LIMIT = 2.0

_TINY = 1e-300


def _exp(x):
    """Return e ** x."""
    doubling = int(x / _LN2 + (0.5 if x >= 0.0 else -0.5))
    remainder = x - doubling * _LN2
    term = 1.0
    total = 1.0
    step = 0
    while True:
        step += 1
        term *= remainder / step
        total += term
        if abs(term) <= 1e-18 * abs(total):
            break
    return total * 2.0**doubling


def _erf_series(z):
    """Return erf(z) from its Maclaurin series, for a small argument."""
    square = z * z
    power = z
    total = z
    step = 0
    while True:
        step += 1
        power *= -square / step
        contribution = power / (2 * step + 1)
        total += contribution
        if abs(contribution) <= 1e-18 * abs(total):
            break
    return _TWO_OVER_SQRT_PI * total


def _erfc_tail(z):
    """Return erfc(z) for a positive argument, by its continued fraction."""
    fraction = _TINY
    numerator = fraction
    denominator = 0.0
    step = 1
    while step < 300:
        partial = 1.0 if step == 1 else (step - 1) / 2.0
        denominator = z + partial * denominator
        if denominator == 0.0:
            denominator = _TINY
        numerator = z + partial / numerator
        if numerator == 0.0:
            numerator = _TINY
        denominator = 1.0 / denominator
        factor = numerator * denominator
        fraction *= factor
        if abs(factor - 1.0) < 1e-17:
            break
        step += 1
    return _ONE_OVER_SQRT_PI * _exp(-z * z) * fraction


def _erfc(z):
    """Return erfc(z) for any argument."""
    if z < -_SERIES_LIMIT:
        return 2.0 - _erfc_tail(-z)
    if z > _SERIES_LIMIT:
        return _erfc_tail(z)
    return 1.0 - _erf_series(z)


def normal_cdf(q):
    """Return the probability a standard normal deviate is at most `q`."""
    return 0.5 * _erfc(-q / _SQRT_2)


def project_value(x):
    """Return the supplied numeric value."""
    return x
