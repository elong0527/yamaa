"""Build the governed verification log sidecar dataset (REQ-1173..1178)."""

from __future__ import annotations

from collections.abc import Sequence

from yamaa.io import Artifact, build_artifact
from yamaa.io.polars import frame_from_values
from yamaa.models import MISSING, TypedColumn
from yamaa.specification.models import Output
from yamaa.verification.diagnostics import VerificationRecord
from yamaa.verification.log import _canonical_json

VERIFICATION_LOG_VERSION = "1.0"
VERIFICATION_LOG_COLUMNS: tuple[TypedColumn, ...] = (
    TypedColumn(name="REPORT_VERSION", type="str"),
    TypedColumn(name="ARTIFACT", type="str"),
    TypedColumn(name="SPEC_PATH", type="str"),
    TypedColumn(name="VERIFICATION_ID", type="str"),
    TypedColumn(name="CHECK", type="str"),
    TypedColumn(name="TARGET", type="str"),
    TypedColumn(name="REQUIREMENT", type="str"),
    TypedColumn(name="SEVERITY", type="str"),
    TypedColumn(name="OUTCOME", type="str"),
    TypedColumn(name="CONDITION", type="str"),
    TypedColumn(name="EVALUATED_COUNT", type="int"),
    TypedColumn(name="FAILURE_COUNT", type="int"),
    TypedColumn(name="DETAILS", type="str"),
)
VERIFICATION_LOG_NAMES = tuple(column.name for column in VERIFICATION_LOG_COLUMNS)


def build_verification_log(
    records: Sequence[VerificationRecord], output: Output
) -> Artifact | None:
    """Build the fixed-schema report of every evaluated check, in order.

    One row is returned per record: a held check reports ``OUTCOME`` ``held``
    with a missing ``CONDITION``, a zero ``FAILURE_COUNT``, and empty
    ``DETAILS``; a violated check reports ``violated`` with its condition,
    count, and remaining detail context. The offending keys stay out of the
    report: the warning log carries them for the same ``SPEC_PATH``
    (REQ-1176). A header-only log is returned when no check was evaluated,
    so a later run replaces a stale report (REQ-1173).
    """
    if output.verification_log is None:
        if records:
            raise ValueError("evaluated checks require output.verification_log")
        return None

    rows: list[list[object]] = []
    seen: set[str] = set()
    for record in records:
        if record.spec_path in seen:
            raise ValueError("a verification log has one row per specification path")
        seen.add(record.spec_path)

        failure = record.failure
        if failure is None:
            outcome = "held"
            condition: object = MISSING
            failure_count = 0
            details = "{}"
        else:
            outcome = "violated"
            condition = failure.condition
            full_context = dict(failure.log_context or failure.context)
            count = full_context.pop("failure_count", None)
            full_context.pop("keys", None)
            full_context.pop("verification_id", None)
            if (
                isinstance(count, bool)
                or not isinstance(count, int)
                or count < 0
                or count > record.evaluated_count
            ):
                raise ValueError(
                    "a violated check reports a failure count within its evaluated count"
                )
            failure_count = count
            details = _canonical_json(full_context)

        rows.append(
            [
                VERIFICATION_LOG_VERSION,
                output.path,
                record.spec_path,
                record.verification_id
                if record.verification_id is not None
                else MISSING,
                record.check,
                record.target if record.target is not None else MISSING,
                record.requirement,
                record.severity,
                outcome,
                condition,
                record.evaluated_count,
                failure_count,
                details,
            ]
        )

    table = frame_from_values(VERIFICATION_LOG_COLUMNS, rows)
    declaration = Output(
        path=output.verification_log,
        columns=list(VERIFICATION_LOG_NAMES),
    )
    return build_artifact(table, declaration, ["SPEC_PATH"])
