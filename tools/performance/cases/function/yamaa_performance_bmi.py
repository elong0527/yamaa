"""The Python implementation of the function case's project function."""


def bmi(weight_kg, height_cm):
    """Body mass index, in the order expression.yaml computes it."""
    return weight_kg / ((height_cm / 100) * (height_cm / 100))
