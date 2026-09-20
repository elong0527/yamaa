"""Named numeric constants, as this project spells them.

This module is the callable half of the artifact `environment.yaml` pins by
digest. It is ordinary project code: it knows nothing about yamaa, reads no
context it was not passed, and returns one scalar for one subject.
"""

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
