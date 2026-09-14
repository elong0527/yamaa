"""Verify one completed table under R005 and R009.

`check_column`, `check_keys`, and `check_dataset` are the hooks an
executor calls at the R005 stage each belongs to; every one reports the
failures it found and leaves the run's fate to its caller.
`verify_completed_table` runs the three in order and raises.
`build_violation_log` turns non-fatal findings into R009's governed sidecar.
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
    VerificationSeverity,
)
from yamaa.verification.log import (
    VIOLATION_LOG_COLUMNS,
    VIOLATION_LOG_NAMES,
    VIOLATION_LOG_VERSION,
    build_violation_log,
)

__all__ = [
    "VIOLATION_LOG_COLUMNS",
    "VIOLATION_LOG_NAMES",
    "VIOLATION_LOG_VERSION",
    "DeclarationError",
    "VerificationError",
    "VerificationFailure",
    "VerificationSeverity",
    "build_violation_log",
    "check_column",
    "check_dataset",
    "check_keys",
    "verify_completed_table",
]
