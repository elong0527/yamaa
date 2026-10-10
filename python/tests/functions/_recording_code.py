"""Shared recording variant of the BMI project code.

This module exists so test modules can import ``RECORDING_CODE`` without
going through ``conftest``: two ``conftest.py`` files (``functions/`` and
``odm/``) share the top-level ``conftest`` module name under pytest's
prepend import mode, so ``from conftest import ...`` resolves to whichever
conftest loaded last and breaks collection depending on invocation order.
A uniquely named private module sidesteps that collision entirely.
"""

from __future__ import annotations

# The same arithmetic as BMI_CODE, with every invocation recorded where a
# test can read it. REQ-0691 is an ordering rule, and an order is only
# observable from inside the code that gets called.
RECORDING_CODE = """
CALLS = []


def bmi(weight_kg, height_cm, cm_per_m=100):
    CALLS.append((weight_kg, height_cm, cm_per_m))
    return weight_kg / (height_cm / cm_per_m) ** 2
"""
