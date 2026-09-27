"""Adjusted ratio, as this project calculates it.

This module is the callable half of the artifact `environment.yaml` pins by
`runtime.artifact.reference`. It is ordinary project code: it knows nothing
about yamaa, reads no context it was not passed, and returns one scalar for
one subject.
"""


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
