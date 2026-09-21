"""Verify one completed table under R005 and R009.

`check_column`, `check_keys`, and `check_dataset` are the hooks an
executor calls at the R005 stage each belongs to; every one reports the
failures it found and leaves the run's fate to its caller. Each also
appends one `VerificationRecord` per evaluated check, held or violated,
when the caller passes `records`, so the run can report what ran.
`verify_completed_table` runs the three in order and raises.
`build_warning_log` turns non-fatal findings into R009's governed sidecar,
and `build_verification_log` turns the records into the governed
verification log sidecar.
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
    VerificationRecord,
    VerificationSeverity,
)
from yamaa.verification.log import (
    WARNING_LOG_COLUMNS,
    WARNING_LOG_NAMES,
    WARNING_LOG_VERSION,
    build_warning_log,
)
from yamaa.verification.report import (
    VERIFICATION_LOG_COLUMNS,
    VERIFICATION_LOG_NAMES,
    VERIFICATION_LOG_VERSION,
    build_verification_log,
)

__all__ = [
    "VERIFICATION_LOG_COLUMNS",
    "VERIFICATION_LOG_NAMES",
    "VERIFICATION_LOG_VERSION",
    "WARNING_LOG_COLUMNS",
    "WARNING_LOG_NAMES",
    "WARNING_LOG_VERSION",
    "DeclarationError",
    "VerificationError",
    "VerificationFailure",
    "VerificationRecord",
    "VerificationSeverity",
    "build_verification_log",
    "build_warning_log",
    "check_column",
    "check_dataset",
    "check_keys",
    "verify_completed_table",
]
