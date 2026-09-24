"""Body mass index, as this project calculates it.

This module is the callable half of the artifact `environment.yaml` pins by
`runtime.artifact.reference`. It is ordinary project code: it knows nothing about yamaa, reads no
context it was not passed, and returns one scalar for one subject.
"""


def bmi(weight_kg, height_cm, cm_per_m=100):
    """Return body mass index in kg/m2 from kilograms and centimetres."""
    return weight_kg / (height_cm / cm_per_m) ** 2
