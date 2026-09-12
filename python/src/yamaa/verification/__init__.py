"""Verify one completed table under R005 and R009.

`check_column`, `check_keys`, and `check_dataset` are the hooks an
executor calls at the R005 stage each belongs to; every one reports the
failures it found and leaves the run's fate to its caller.
`verify_completed_table` runs the three in order and raises.
"""

from yamaa.verification.checks import (
    check_column,
    check_dataset,
    check_keys,
    verify_completed_table,
)
from yamaa.verification.diagnostics import (
    DeclarationError,
    VerificationError,
    VerificationFailure,
)

__all__ = [
    "DeclarationError",
    "VerificationError",
    "VerificationFailure",
    "check_column",
    "check_dataset",
    "check_keys",
    "verify_completed_table",
]
