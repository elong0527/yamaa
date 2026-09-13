"""Build the governed R009 warning-violation sidecar dataset."""

from __future__ import annotations

import json
from collections.abc import Sequence

from pydantic import JsonValue

from yamaa.io import Artifact, build_artifact
from yamaa.io.polars import frame_from_values
from yamaa.models import MISSING, TypedColumn, ValueResult, convert_value
from yamaa.specification.models import Output
from yamaa.verification.diagnostics import VerificationFailure

VIOLATION_LOG_VERSION = "1.0"
VIOLATION_LOG_COLUMNS: tuple[TypedColumn, ...] = (
    TypedColumn(name="LOG_VERSION", type="str"),
    TypedColumn(name="ARTIFACT", type="str"),
    TypedColumn(name="SEVERITY", type="str"),
    TypedColumn(name="CONDITION", type="str"),
    TypedColumn(name="REQUIREMENT", type="str"),
    TypedColumn(name="SPEC_PATH", type="str"),
    TypedColumn(name="VERIFICATION_ID", type="str"),
    TypedColumn(name="FAILURE_COUNT", type="int"),
    TypedColumn(name="OFFENDING_KEYS", type="str"),
    TypedColumn(name="DETAILS", type="str"),
)
VIOLATION_LOG_NAMES = tuple(column.name for column in VIOLATION_LOG_COLUMNS)


def _canonical_json(value: JsonValue) -> str:
    """Render the compact, ASCII JSON spelling R009 fixes for log fields."""
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=True)
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        rendered = convert_value(value, "str")
        if not isinstance(rendered, ValueResult) or rendered.value is MISSING:
            raise ValueError("canonical JSON does not admit a non-finite number")
        assert isinstance(rendered.value, str)
        return rendered.value
    if isinstance(value, list):
        return "[" + ",".join(_canonical_json(item) for item in value) + "]"
    if isinstance(value, dict):
        return (
            "{"
            + ",".join(
                _canonical_json(name) + ":" + _canonical_json(value[name])
                for name in sorted(value)
            )
            + "}"
        )
    raise TypeError(f"unsupported JSON value {type(value).__name__}")


def build_violation_log(
    violations: Sequence[VerificationFailure], output: Output
) -> Artifact | None:
    """Build a fixed-schema sidecar, including a header-only empty log.

    A log is returned whenever ``output.violation_log`` is declared. Warning
    declarations require that field during preflight, while permitting an
    explicit empty log lets a later successful run replace stale findings.
    """
    if output.violation_log is None:
        if violations:
            raise ValueError("warning violations require output.violation_log")
        return None
    if any(violation.severity != "warning" for violation in violations):
        raise ValueError("a violation log contains warning violations only")

    rows: list[list[object]] = []
    seen: set[str] = set()
    for violation in violations:
        if len(violation.spec_paths) != 1:
            raise ValueError("a logged verification has one stable specification path")
        spec_path = violation.spec_paths[0]
        if spec_path in seen:
            raise ValueError("a violation log has one row per specification path")
        seen.add(spec_path)

        full_context = dict(violation.log_context or violation.context)
        count = full_context.pop("failure_count", None)
        full_context.pop("keys", None)
        verification_id = full_context.pop("verification_id", MISSING)
        if isinstance(count, bool) or not isinstance(count, int) or count < 1:
            raise ValueError("a warning violation has a positive failure_count")
        rows.append(
            [
                VIOLATION_LOG_VERSION,
                output.path,
                "warning",
                violation.condition,
                violation.requirement,
                spec_path,
                verification_id,
                count,
                _canonical_json(list(violation.offending_keys)),
                _canonical_json(full_context),
            ]
        )

    table = frame_from_values(VIOLATION_LOG_COLUMNS, rows)
    declaration = Output(
        path=output.violation_log,
        columns=list(VIOLATION_LOG_NAMES),
    )
    return build_artifact(table, declaration, ["SPEC_PATH"])
