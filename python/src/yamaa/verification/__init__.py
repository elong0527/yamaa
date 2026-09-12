"""Completed-table verification under R005 and R009.

The executor runs :func:`verify_columns` after each column lifecycle and
:func:`verify_dataset` after key validation, so each assertion observes the
artifact its stage guarantees. Both return the table unchanged and raise
:exc:`VerificationError` carrying every failure they found.
"""

from __future__ import annotations

import re
from collections.abc import Mapping, Sequence

from pydantic import BaseModel, ConfigDict, Field, JsonValue

from yamaa.expressions.core import MappingResolver
from yamaa.expressions.predicates import (
    PredicateError,
    TruthValue,
    evaluate_predicate,
    parse_predicate,
)
from yamaa.models import (
    MISSING,
    ConditionResult,
    DateTimeValue,
    DateValue,
    TypedTable,
)
from yamaa.specification.models import Column as SpecColumn
from yamaa.specification.models import Expression

_MAX_REPORTED_KEYS = 5


class _FrozenModel(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)


class VerificationFailure(_FrozenModel):
    """One failed key check or verification with its offending rows."""

    phase: str = Field(pattern=r"^(output|verification)$")
    condition: str = Field(min_length=1)
    spec_paths: tuple[str, ...] = Field(min_length=1)
    requirement: str | None = None
    context: dict[str, JsonValue] = Field(default_factory=dict)


class VerificationError(ValueError):
    """Raised when key validation or verifications fail."""

    def __init__(self, failures: list[VerificationFailure]) -> None:
        if not failures:
            raise ValueError("VerificationError requires at least one failure")
        self.failures = tuple(failures)
        conditions = ", ".join(item.condition for item in failures)
        super().__init__(f"verification failed: {conditions}")


def _missing(value: object) -> bool:
    return value is MISSING or value is None


def _json(value: object) -> JsonValue:
    if _missing(value):
        return None
    if isinstance(value, (DateValue, DateTimeValue)):
        return value.to_text()
    if isinstance(value, (str, int, float, bool)):
        return value
    return type(value).__name__


def _row_keys(row: Mapping[str, object], keys: Sequence[str]) -> dict[str, JsonValue]:
    return {name: _json(row[name]) for name in keys}


def _rows(table: TypedTable) -> list[dict[str, object]]:
    return list(table.frame.iter_rows(named=True))


def _declare(
    failures: list[VerificationFailure],
    *,
    condition: str,
    spec_path: str,
    requirement: str,
    context: dict[str, JsonValue],
    offending: list[dict[str, JsonValue]],
    count_name: str,
) -> None:
    shown = offending[:_MAX_REPORTED_KEYS]
    failures.append(
        VerificationFailure(
            phase="verification",
            condition=condition,
            spec_paths=(spec_path,),
            requirement=requirement,
            context={
                **context,
                count_name: len(offending),
                "keys": shown,
            },
        )
    )


def _operation(expression: Expression) -> tuple[str, Mapping[str, object]]:
    root = expression.root
    keyword = next(iter(root))
    arguments = root[keyword]
    if not isinstance(arguments, dict):
        raise TypeError(f"verification {keyword!r} requires named arguments")
    return keyword, arguments


def _check_column(
    keyword: str,
    arguments: Mapping[str, object],
    column: SpecColumn,
    values: Sequence[object],
    rows: Sequence[Mapping[str, object]],
    keys: Sequence[str],
    index: int,
    failures: list[VerificationFailure],
) -> None:
    path = f"columns.{column.name}.verifications[{index}].{keyword}"
    if keyword == "not_missing":
        offending = [
            _row_keys(row, keys) for row, value in zip(rows, values) if _missing(value)
        ]
        if offending:
            _declare(
                failures,
                condition="not_missing_failed",
                spec_path=path,
                requirement="R009-9",
                context={"column": column.name},
                offending=offending,
                count_name="failure_count",
            )
    elif keyword == "allowed_values":
        accepted = arguments.get("values")
        if not isinstance(accepted, list):
            raise ValueError("allowed_values requires a values list")
        offending = [
            _row_keys(row, keys)
            for row, value in zip(rows, values)
            if not _missing(value) and value not in accepted
        ]
        if offending:
            _declare(
                failures,
                condition="allowed_values_failed",
                spec_path=path,
                requirement="R009-10",
                context={"column": column.name},
                offending=offending,
                count_name="failure_count",
            )
    elif keyword == "range":
        if column.type not in ("int", "float"):
            raise ValueError(f"range applies to numeric columns, not {column.type}")
        minimum = arguments.get("min")
        maximum = arguments.get("max")
        if minimum is None and maximum is None:
            raise ValueError("range requires at least one bound")
        offending = [
            _row_keys(row, keys)
            for row, value in zip(rows, values)
            if not _missing(value)
            and (
                (minimum is not None and value < minimum)
                or (maximum is not None and value > maximum)
            )
        ]
        if offending:
            _declare(
                failures,
                condition="range_failed",
                spec_path=path,
                requirement="R009-11",
                context={"column": column.name},
                offending=offending,
                count_name="failure_count",
            )
    elif keyword == "max_length":
        if column.type != "str":
            raise ValueError(f"max_length applies to str columns, not {column.type}")
        maximum = arguments.get("max")
        if not isinstance(maximum, int) or maximum < 1:
            raise ValueError("max_length requires a max of at least one")
        offending = [
            _row_keys(row, keys)
            for row, value in zip(rows, values)
            if not _missing(value) and len(value) > maximum
        ]
        if offending:
            _declare(
                failures,
                condition="max_length_failed",
                spec_path=path,
                requirement="R009-12",
                context={"column": column.name},
                offending=offending,
                count_name="failure_count",
            )
    elif keyword == "matches":
        if column.type != "str":
            raise ValueError(f"matches applies to str columns, not {column.type}")
        pattern = arguments.get("pattern")
        if not isinstance(pattern, str):
            raise ValueError("matches requires a pattern string")
        try:
            expression = re.compile(pattern)
        except re.error as error:
            raise ValueError(f"matches pattern does not compile: {error}") from error
        offending = [
            _row_keys(row, keys)
            for row, value in zip(rows, values)
            if not _missing(value) and expression.search(value) is None
        ]
        if offending:
            _declare(
                failures,
                condition="matches_failed",
                spec_path=path,
                requirement="R009-13",
                context={"column": column.name},
                offending=offending,
                count_name="failure_count",
            )
    else:
        raise ValueError(f"unknown column verification {keyword!r}")


def verify_columns(
    table: TypedTable,
    columns: Sequence[SpecColumn],
    keys: Sequence[str],
) -> TypedTable:
    """Run each column's verifications over its completed values."""
    rows = _rows(table)
    by_name = {column.name: column for column in table.columns}
    failures: list[VerificationFailure] = []
    for declared in columns:
        column = by_name.get(declared.name)
        if column is None:
            raise ValueError(f"unknown column {declared.name!r}")
        values = table.frame.get_column(column.name).to_list()
        for index, verification in enumerate(declared.verifications or ()):
            keyword, arguments = _operation(verification)
            _check_column(
                keyword, arguments, declared, values, rows, keys, index, failures
            )
    if failures:
        raise VerificationError(failures)
    return table


def _key_tuple(row: Mapping[str, object], names: Sequence[str]) -> tuple[object, ...]:
    return tuple(None if _missing(row[name]) else row[name] for name in names)


def _check_keys(
    rows: Sequence[Mapping[str, object]],
    keys: Sequence[str],
    failures: list[VerificationFailure],
) -> None:
    for name in keys:
        if not rows or name not in rows[0]:
            raise ValueError(f"unknown key column {name!r}")
    for position, name in enumerate(keys):
        offending = [_row_keys(row, keys) for row in rows if _missing(row[name])]
        if offending:
            shown = offending[:_MAX_REPORTED_KEYS]
            failures.append(
                VerificationFailure(
                    phase="output",
                    condition="missing_key",
                    spec_paths=(f"keys[{position}]",),
                    requirement="R005-52",
                    context={
                        "column": name,
                        "missing_count": len(offending),
                        "keys": shown,
                    },
                )
            )
    seen: dict[tuple[object, ...], dict[str, JsonValue]] = {}
    duplicated: list[dict[str, JsonValue]] = []
    for row in rows:
        combined = _key_tuple(row, keys)
        if combined in seen:
            if seen[combined] is not None:
                duplicated.append(seen[combined])
                seen[combined] = None
        else:
            seen[combined] = _row_keys(row, keys)
    if duplicated:
        failures.append(
            VerificationFailure(
                phase="output",
                condition="duplicate_key",
                spec_paths=("keys",),
                requirement="R005-52",
                context={
                    "duplicate_count": len(duplicated),
                    "keys": duplicated[:_MAX_REPORTED_KEYS],
                },
            )
        )


def _parsed_predicate(text: object) -> object:
    if not isinstance(text, str):
        raise TypeError("a predicate must be a non-empty string")
    try:
        return parse_predicate(text)
    except PredicateError as error:
        raise ValueError(f"invalid predicate: {error}") from error


def _predicate_truth(ast: object, row: Mapping[str, object]) -> TruthValue | None:
    result = evaluate_predicate(ast, MappingResolver(dict(row)))
    if isinstance(result, ConditionResult):
        if result.condition.condition == "unknown_field":
            raise ValueError(
                f"predicate names an unknown field: {result.condition.context}"
            )
        return None
    return result.value


def _row_id(arguments: Mapping[str, object], keyword: str) -> str | None:
    identifier = arguments.get("id")
    if identifier is not None and not isinstance(identifier, str):
        raise ValueError(f"{keyword} id must be a string")
    return identifier


def _check_dataset(
    keyword: str,
    arguments: Mapping[str, object],
    rows: Sequence[Mapping[str, object]],
    keys: Sequence[str],
    index: int,
    failures: list[VerificationFailure],
) -> None:
    path = f"verifications[{index}].{keyword}"
    if keyword == "unique":
        names = arguments.get("columns")
        if not isinstance(names, list) or not names:
            raise ValueError("unique requires a non-empty columns list")
        for name in names:
            if not rows or name not in rows[0]:
                raise ValueError(f"unknown column {name!r}")
        seen: dict[tuple[object, ...], dict[str, JsonValue]] = {}
        duplicated: list[dict[str, JsonValue]] = []
        for row in rows:
            combined = _key_tuple(row, names)
            if combined in seen:
                if seen[combined] is not None:
                    duplicated.append(seen[combined])
                    seen[combined] = None
            else:
                seen[combined] = _row_keys(row, keys)
        if duplicated:
            _declare(
                failures,
                condition="unique_failed",
                spec_path=path,
                requirement="R009-15",
                context={"columns": list(names)},
                offending=duplicated,
                count_name="failure_count",
            )
    elif keyword == "all_or_none":
        identifier = _row_id(arguments, keyword)
        if identifier is None:
            raise ValueError("all_or_none requires an id")
        names = arguments.get("columns")
        if not isinstance(names, list) or len(set(names)) < 2:
            raise ValueError("all_or_none requires at least two distinct columns")
        for name in names:
            if not rows or name not in rows[0]:
                raise ValueError(f"unknown column {name!r}")
        offending = [
            _row_keys(row, keys)
            for row in rows
            if any(_missing(row[name]) for name in names)
            and not all(_missing(row[name]) for name in names)
        ]
        if offending:
            _declare(
                failures,
                condition="all_or_none_failed",
                spec_path=path,
                requirement="R009-16",
                context={"columns": list(names)},
                offending=offending,
                count_name="failure_count",
            )
    elif keyword == "implies":
        identifier = _row_id(arguments, keyword)
        if identifier is None:
            raise ValueError("implies requires an id")
        when = _parsed_predicate(arguments.get("when"))
        then = _parsed_predicate(arguments.get("then"))
        offending = [
            _row_keys(row, keys)
            for row in rows
            if _predicate_truth(when, row) is TruthValue.TRUE
            and _predicate_truth(then, row) is not TruthValue.TRUE
        ]
        if offending:
            _declare(
                failures,
                condition="implication_failed",
                spec_path=path,
                requirement="R009-17",
                context={},
                offending=offending,
                count_name="failure_count",
            )
    elif keyword == "predicate":
        identifier = _row_id(arguments, keyword)
        if identifier is None:
            raise ValueError("predicate requires an id")
        assertion = _parsed_predicate(arguments.get("assert"))
        offending = [
            _row_keys(row, keys)
            for row in rows
            if _predicate_truth(assertion, row) is not TruthValue.TRUE
        ]
        if offending:
            _declare(
                failures,
                condition="predicate_failed",
                spec_path=path,
                requirement="R009-18",
                context={},
                offending=offending,
                count_name="failure_count",
            )
    elif keyword == "row_count":
        identifier = _row_id(arguments, keyword)
        groups = arguments.get("group_by")
        if groups is not None:
            if identifier is None:
                raise ValueError("a grouped row_count requires an id")
            if (
                not isinstance(groups, list)
                or not groups
                or len(set(groups)) != len(groups)
            ):
                raise ValueError("row_count group_by must be non-empty and unique")
            for name in groups:
                if not rows or name not in rows[0]:
                    raise ValueError(f"unknown column {name!r}")
        minimum = arguments.get("min")
        maximum = arguments.get("max")
        if minimum is None and maximum is None:
            raise ValueError("row_count requires at least one bound")
        if minimum is not None and maximum is not None and minimum > maximum:
            raise ValueError("row_count min must not exceed max")
        admitted = rows
        if arguments.get("filter") is not None:
            predicate = _parsed_predicate(arguments.get("filter"))
            admitted = [
                row
                for row in rows
                if _predicate_truth(predicate, row) is TruthValue.TRUE
            ]
        partitions: dict[tuple[object, ...], list[Mapping[str, object]]] = {}
        for row in admitted:
            partitions.setdefault(_key_tuple(row, groups or ()), []).append(row)
        offending = [
            {
                "values": {
                    name: _json(value) for name, value in zip(groups or (), combined)
                },
                "count": len(members),
            }
            for combined, members in partitions.items()
            if (minimum is not None and len(members) < minimum)
            or (maximum is not None and len(members) > maximum)
        ]
        if offending:
            context: dict[str, JsonValue] = {}
            if groups is not None:
                context["group_by"] = list(groups)
            if minimum is not None:
                context["min"] = minimum
            if maximum is not None:
                context["max"] = maximum
            failures.append(
                VerificationFailure(
                    phase="verification",
                    condition="row_count_failed",
                    spec_paths=(path,),
                    requirement="R009-19",
                    context={
                        **context,
                        "failure_count": len(offending),
                        "groups": offending[:_MAX_REPORTED_KEYS],
                    },
                )
            )
    else:
        raise ValueError(f"unknown dataset verification {keyword!r}")


def verify_dataset(
    table: TypedTable,
    keys: Sequence[str],
    verifications: Sequence[Expression],
) -> TypedTable:
    """Validate keys, then run dataset verifications over completed rows."""
    rows = _rows(table)
    failures: list[VerificationFailure] = []
    _check_keys(rows, list(keys), failures)
    for index, verification in enumerate(verifications):
        keyword, arguments = _operation(verification)
        _check_dataset(keyword, arguments, rows, list(keys), index, failures)
    if failures:
        raise VerificationError(failures)
    return table


__all__ = [
    "VerificationError",
    "VerificationFailure",
    "verify_columns",
    "verify_dataset",
]
