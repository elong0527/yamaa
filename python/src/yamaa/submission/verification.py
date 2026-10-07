"""Terminology checks at the completed-column verification boundary."""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from typing import Any

from yamaa.models import MISSING, TypedTable
from yamaa.specification.models import Column, Specification
from yamaa.verification import VerificationFailure, VerificationRecord, check_column
from yamaa.verification.checks import (
    _failure,
    _json,
    _key_maps,
    _require_columns,
    _values,
)


def check_terminology(
    table: TypedTable,
    column: Column,
    keys: Sequence[str],
    document: Mapping[str, Any],
    specification: Specification,
    *,
    records: list[VerificationRecord] | None = None,
) -> tuple[VerificationFailure, ...]:
    """Enforce only non-extensible item lists, with row-level overrides.

    Missing values pass. Text uses exact equality and numbers use numeric
    equality, as REQ-0935 fixes. External and extensible lists add no check.
    """
    codelists = {entry["id"]: entry for entry in document.get("codelists") or []}
    metadata = column.submission
    default = metadata.codelist if metadata is not None else None
    discriminator = specification.domain + "TESTCD"
    overrides: dict[str, str | None] = {}
    for row in specification.rows or []:
        override = (row.submission or {}).get(column.name)
        expression = row.derivations.get(discriminator)
        if (
            override is not None
            and override.codelist is not None
            and expression is not None
        ):
            code = expression.value.root.get("literal")
            if isinstance(code, Mapping):
                code = code.get("value")
            if isinstance(code, str):
                overrides[code] = override.codelist
    inventory = metadata is not None and metadata.inventory_vocabulary is True
    if default is None and not overrides and not inventory:
        return ()
    values = _values(table, column.name)
    if overrides:
        # REQ-1163 pins row-level codelist overrides to the domain's
        # <DOMAIN>TESTCD discriminator; reading it without the guard lets a
        # table missing the column escape the verification boundary with an
        # ungoverned polars ColumnNotFoundError instead of a DeclarationError.
        _require_columns(table, [discriminator], f"columns.{column.name}", "REQ-0405")
    codes = _values(table, discriminator) if overrides else [None] * len(values)
    key_maps = _key_maps(table, keys)
    failures: list[VerificationFailure] = []
    bindings = list(dict.fromkeys([default, *overrides.values()]))
    for identifier in bindings:
        if identifier is None:
            continue
        codelist = codelists[identifier]
        if codelist.get("extensible") or codelist.get("items") is None:
            continue
        admitted = {item["value"] for item in codelist["items"]}
        positions = [
            index
            for index, code in enumerate(codes)
            if overrides.get(code, default) == identifier
        ]
        bad = [
            index
            for index in positions
            if values[index] is not MISSING and values[index] not in admitted
        ]
        path = f"columns.{column.name}.submission.codelist"
        failure = None
        if bad:
            failure = _failure(
                "allowed_values_failed",
                path,
                "REQ-0957",
                {"column": column.name, "codelist": identifier},
                [key_maps[index] for index in bad],
                extra={"values": [_json(values[index]) for index in bad[:5]]},
                log_extra={"values": [_json(values[index]) for index in bad]},
            )
            failures.append(failure)
        if records is not None:
            records.append(
                VerificationRecord(
                    spec_path=path,
                    check="allowed_values",
                    target=column.name,
                    requirement="REQ-0957",
                    evaluated_count=len(positions),
                    failure=failure,
                )
            )
    if inventory:
        admitted = {entry["id"] for entry in document["datasets"]}
        bad = [
            index
            for index, value in enumerate(values)
            if value is not MISSING and value not in admitted
        ]
        path = f"columns.{column.name}.submission.inventory_vocabulary"
        failure = None
        if bad:
            failure = _failure(
                "allowed_values_failed",
                path,
                "REQ-1156",
                {"column": column.name},
                [key_maps[index] for index in bad],
                extra={"values": [_json(values[index]) for index in bad[:5]]},
                log_extra={"values": [_json(values[index]) for index in bad]},
            )
            failures.append(failure)
        if records is not None:
            records.append(
                VerificationRecord(
                    spec_path=path,
                    check="allowed_values",
                    target=column.name,
                    requirement="REQ-1156",
                    evaluated_count=len(values),
                    failure=failure,
                )
            )
    return tuple(failures)


def submission_hooks(document: Mapping[str, Any], specification: Specification):
    """Bind study terminology to the existing execution verification hooks."""
    from yamaa.runtime import ExecutionHooks

    def column_check(table, column, keys, *, records=None):
        return (
            *check_column(table, column, keys, records=records),
            *check_terminology(
                table, column, keys, document, specification, records=records
            ),
        )

    return ExecutionHooks(column=column_check)
