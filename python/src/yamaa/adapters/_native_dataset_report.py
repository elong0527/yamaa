"""Format native observations without evaluating expressions or checking tables."""

from __future__ import annotations

import struct

from yamaa.planning import ExecutionDiagnostic
from yamaa.verification import VerificationFailure, VerificationRecord
from yamaa.verification.diagnostics import REPORTED_KEYS


def scalar(value):
    """Decode exact diagnostic/key scalars into the public JSON value vocabulary."""
    if not isinstance(value, dict) or len(value) != 1:
        raise ValueError("invalid native scalar observation")
    kind, data = next(iter(value.items()))
    if kind == "missing":
        return None
    if kind == "int":
        return int(data)
    if kind == "integer":
        # The reference retains long conversion integers as text, without
        # constructing an unbounded Python integer or invoking its digit limit.
        return int(data) if len(data.lstrip("-")) <= 19 else data
    if kind == "float":
        return struct.unpack(">d", bytes.fromhex(data))[0]
    if kind in {"date", "datetime"}:
        return data["text"]
    if kind in {"str", "bool"}:
        return data
    raise ValueError("unknown native scalar observation")


def identity(row, keys):
    """Retain declared key order, including completed missing key values."""
    return {name: scalar(value) for name, value in zip(keys, row["keys"], strict=True)}


def condition(observed, keys):
    """Map an engine condition and optional completed identity to public context."""
    diagnostic = observed["diagnostic"]
    context = {name: scalar(value) for name, value in diagnostic["context"].items()}
    if observed["identity"] is not None:
        context["keys"] = [identity(observed["identity"], keys)]
    if observed.get("partition") is not None:
        if observed["identity"] is not None:
            raise ValueError(
                "native condition has competing row and partition identities"
            )
        context["keys"] = [
            {entry["name"]: scalar(entry["value"]) for entry in observed["partition"]}
        ]
    if observed.get("matched_key") is not None:
        pairs = observed["matched_key"]
        context["key"] = [entry["name"] for entry in pairs]
        context["intermediate_key"] = {
            entry["name"]: scalar(entry["value"]) for entry in pairs
        }
    return ExecutionDiagnostic(
        phase=diagnostic["phase"],
        condition=diagnostic["condition"],
        spec_paths=tuple(diagnostic["spec_paths"]),
        # The reference's root/row-filter wrappers omit the primitive requirement.
        # The native diagnostic retains it; the ordinary public result matches
        # that existing site-specific report contract.
        requirement=None
        if all(
            path == "filter" or (path.startswith("rows[") and path.endswith(".filter"))
            for path in diagnostic["spec_paths"]
        )
        else diagnostic["requirement"],
        context=context,
    )


def observations(specification, outcome):
    """Translate all completed check records, preserving failure counts and order."""
    declarations = {
        f"verifications[{index}].{check.operation}": check
        for index, check in enumerate(specification.verifications or ())
    }
    records = []
    failures = []
    for observed in outcome["verifications"]:
        path = observed["spec_path"]
        check = declarations.get(path)
        payload = check.root[check.operation] if check is not None else {}
        if isinstance(payload, list):
            payload = {"columns": payload}
        identifier = payload.get("id")
        failure = None
        if int(observed["failed_count"]):
            context = {"verification_id": identifier} if identifier is not None else {}
            condition = observed["condition"]
            count_name = "failure_count"
            offending = [
                identity(row, specification.keys) for row in observed["offending_rows"]
            ]
            if condition == "missing_key":
                context["column"] = specification.keys[int(path[5:-1])]
                count_name = "missing_count"
            elif condition == "duplicate_key":
                count_name = "duplicate_count"
            elif check is not None and check.operation == "unique":
                context["columns"] = payload["columns"]
            elif check is not None and check.operation == "row_count":
                offending = [{}]
                context["count"] = int(observed["output_rows"])
            elif check is not None and check.operation in {"assert", "implies"}:
                pass  # Predicate failures report counts and keys, without extra fields.
            else:
                raise ValueError("unknown native verification observation")
            context[count_name] = int(observed["failed_count"])
            failure = VerificationFailure(
                phase="output" if check is None else "verification",
                condition=condition,
                spec_paths=(path,),
                requirement=observed["requirement"],
                context={**context, "keys": offending[:REPORTED_KEYS]},
                offending_keys=tuple(offending),
                log_context={**context, "keys": offending},
            )
            failures.append(ExecutionDiagnostic.model_validate(failure.model_dump()))
        if check is not None:
            records.append(
                VerificationRecord(
                    spec_path=path,
                    check=check.operation,
                    requirement=observed["requirement"],
                    verification_id=identifier,
                    evaluated_count=int(observed["evaluated_count"]),
                    failure=failure,
                )
            )
    return tuple(records), tuple(failures)
