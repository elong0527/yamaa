"""Completed-table verification under R005 and R009.

The executor runs :func:`verify_columns` after each column lifecycle and
:func:`verify_dataset` after key validation, so each assertion observes the
artifact its stage guarantees. Both return the table unchanged and raise
:exc:`VerificationError` carrying every failure they found.
"""

from __future__ import annotations

import datetime as dt
from collections.abc import Mapping, Sequence
from functools import cmp_to_key

import regress
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
    ConditionPhase,
    ConditionResult,
    DateTimeValue,
    DateValue,
    TypedTable,
)
from yamaa.specification.models import Column as SpecColumn
from yamaa.specification.models import Expression, OrderTerm, Output

_MAX_REPORTED_KEYS = 5


class _FrozenModel(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)


class VerificationFailure(_FrozenModel):
    """One failed key check or verification with its offending rows."""

    phase: ConditionPhase
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
    if type(value) is dt.datetime:
        return value.isoformat(timespec="seconds")
    if type(value) is dt.date:
        return value.isoformat()
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
        if minimum is not None and maximum is not None and minimum > maximum:
            raise ValueError("range min must not exceed max")
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
                condition="length_failed",
                spec_path=path,
                requirement="R009-32",
                context={"column": column.name, "max": maximum},
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
            expression = regress.Regex(pattern, "u")
        except regress.RegressError as error:
            raise ValueError(f"matches pattern does not compile: {error}") from error
        offending = [
            _row_keys(row, keys)
            for row, value in zip(rows, values)
            if not _missing(value) and expression.find(value) is None
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
    if not keys or len(set(keys)) != len(keys):
        raise ValueError("keys must be non-empty and unique")
    for key in keys:
        if key not in by_name:
            raise ValueError(f"unknown key column {key!r}")
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
    available: set[str],
    failures: list[VerificationFailure],
) -> None:
    if not keys:
        raise ValueError("keys must not be empty")
    if len(set(keys)) != len(keys):
        raise ValueError("keys must not repeat a column")
    for name in keys:
        if name not in available:
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


def _predicate_names(node: object) -> set[str]:
    if isinstance(node, dict):
        names = {node["name"]} if node.get("kind") == "identifier" else set()
        for value in node.values():
            names.update(_predicate_names(value))
        return names
    if isinstance(node, list):
        names: set[str] = set()
        for value in node:
            names.update(_predicate_names(value))
        return names
    return set()


def _parsed_predicate(
    text: object,
    available: set[str],
    path: str,
) -> object:
    if not isinstance(text, str):
        raise TypeError("a predicate must be a non-empty string")
    try:
        parsed = parse_predicate(text)
    except PredicateError as error:
        raise VerificationError(
            [
                VerificationFailure(
                    phase="validation",
                    condition="invalid_predicate",
                    spec_paths=(path,),
                    requirement="R004-31",
                    context={"reason": str(error)},
                )
            ]
        ) from error
    unknown = sorted(_predicate_names(parsed) - available)
    if unknown:
        raise VerificationError(
            [
                VerificationFailure(
                    phase="validation",
                    condition="unknown_field",
                    spec_paths=(path,),
                    requirement="R004-32",
                    context={"identifier": unknown[0]},
                )
            ]
        )
    return parsed


def _predicate_truth(
    ast: object,
    row: Mapping[str, object],
    keys: Sequence[str],
    path: str,
) -> TruthValue:
    result = evaluate_predicate(ast, MappingResolver(dict(row)))
    if isinstance(result, ConditionResult):
        requirement = {
            "unknown_field": "R004-32",
            "incompatible_input_type": "R004-33",
            "invalid_predicate": "R004-34",
        }.get(result.condition.condition)
        raise VerificationError(
            [
                VerificationFailure(
                    phase=result.condition.phase,
                    condition=result.condition.condition,
                    spec_paths=(path,),
                    requirement=requirement,
                    context={
                        **result.condition.context,
                        "failure_count": 1,
                        "keys": [_row_keys(row, keys)],
                    },
                )
            ]
        )
    return result.value


def _row_id(arguments: Mapping[str, object], keyword: str) -> str | None:
    identifier = arguments.get("id")
    if identifier is not None and not isinstance(identifier, str):
        raise ValueError(f"{keyword} id must be a string")
    if identifier == "":
        raise ValueError(f"{keyword} id must not be empty")
    return identifier


def _check_dataset(
    keyword: str,
    arguments: Mapping[str, object],
    rows: Sequence[Mapping[str, object]],
    available: set[str],
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
            if name not in available:
                raise ValueError(f"unknown column {name!r}")
        members: dict[tuple[object, ...], list[dict[str, JsonValue]]] = {}
        for row in rows:
            combined = _key_tuple(row, names)
            members.setdefault(combined, []).append(_row_keys(row, keys))
        duplicated = [group for group in members.values() if len(group) > 1]
        if duplicated:
            failures.append(
                VerificationFailure(
                    phase="verification",
                    condition="unique_failed",
                    spec_paths=(path,),
                    requirement="R009-15",
                    context={
                        "columns": list(names),
                        "failure_count": len(duplicated),
                        "keys": [key for group in duplicated for key in group][
                            :_MAX_REPORTED_KEYS
                        ],
                    },
                )
            )
    elif keyword == "all_or_none":
        identifier = _row_id(arguments, keyword)
        if identifier is None:
            raise ValueError("all_or_none requires an id")
        names = arguments.get("columns")
        if not isinstance(names, list) or len(set(names)) < 2:
            raise ValueError("all_or_none requires at least two distinct columns")
        for name in names:
            if name not in available:
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
                context={"verification_id": identifier},
                offending=offending,
                count_name="failure_count",
            )
    elif keyword == "implies":
        identifier = _row_id(arguments, keyword)
        if identifier is None:
            raise ValueError("implies requires an id")
        when_path = f"{path}.when"
        then_path = f"{path}.then"
        when = _parsed_predicate(arguments.get("when"), available, when_path)
        then = _parsed_predicate(arguments.get("then"), available, then_path)
        offending = [
            _row_keys(row, keys)
            for row in rows
            if _predicate_truth(when, row, keys, when_path) is TruthValue.TRUE
            and _predicate_truth(then, row, keys, then_path) is not TruthValue.TRUE
        ]
        if offending:
            _declare(
                failures,
                condition="implication_failed",
                spec_path=path,
                requirement="R009-17",
                context={"verification_id": identifier},
                offending=offending,
                count_name="failure_count",
            )
    elif keyword == "predicate":
        identifier = _row_id(arguments, keyword)
        if identifier is None:
            raise ValueError("predicate requires an id")
        assertion_path = f"{path}.assert"
        assertion = _parsed_predicate(
            arguments.get("assert"), available, assertion_path
        )
        offending = [
            _row_keys(row, keys)
            for row in rows
            if _predicate_truth(assertion, row, keys, assertion_path)
            is not TruthValue.TRUE
        ]
        if offending:
            _declare(
                failures,
                condition="predicate_failed",
                spec_path=path,
                requirement="R009-18",
                context={"verification_id": identifier},
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
                if name not in available:
                    raise ValueError(f"unknown column {name!r}")
        minimum = arguments.get("min")
        maximum = arguments.get("max")
        if minimum is None and maximum is None:
            raise ValueError("row_count requires at least one bound")
        if minimum is not None and maximum is not None and minimum > maximum:
            raise ValueError("row_count min must not exceed max")
        predicate = None
        filter_path = f"{path}.filter"
        if arguments.get("filter") is not None:
            predicate = _parsed_predicate(
                arguments.get("filter"), available, filter_path
            )
        partitions: dict[tuple[object, ...], int] = {} if groups else {(): 0}
        for row in rows:
            combined = _key_tuple(row, groups or ())
            partitions.setdefault(combined, 0)
            if predicate is None or (
                _predicate_truth(predicate, row, keys, filter_path) is TruthValue.TRUE
            ):
                partitions[combined] += 1
        offending = [
            (combined, count)
            for combined, count in partitions.items()
            if (minimum is not None and count < minimum)
            or (maximum is not None and count > maximum)
        ]
        if offending:
            context: dict[str, JsonValue] = {
                "failure_count": len(offending),
            }
            if identifier is not None:
                context["verification_id"] = identifier
            if groups is not None:
                context["keys"] = [
                    {
                        name: _json(value)
                        for name, value in zip(groups, combined, strict=True)
                    }
                    for combined, _ in offending[:_MAX_REPORTED_KEYS]
                ]
            if len(offending) == 1:
                context["count"] = offending[0][1]
            else:
                context["counts"] = [
                    count for _, count in offending[:_MAX_REPORTED_KEYS]
                ]
            failures.append(
                VerificationFailure(
                    phase="verification",
                    condition="row_count_failed",
                    spec_paths=(path,),
                    requirement="R009-19",
                    context=context,
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
    available = {column.name for column in table.columns}
    failures: list[VerificationFailure] = []
    _check_keys(rows, list(keys), available, failures)
    if failures:
        raise VerificationError(failures)

    seen_ids: set[str] = set()
    operations = []
    for index, verification in enumerate(verifications):
        keyword, arguments = _operation(verification)
        identifier = _row_id(arguments, keyword)
        if identifier is not None:
            if identifier in seen_ids:
                raise ValueError(f"duplicate dataset verification id {identifier!r}")
            seen_ids.add(identifier)
        operations.append((index, keyword, arguments))

    for index, keyword, arguments in operations:
        _check_dataset(
            keyword,
            arguments,
            rows,
            available,
            list(keys),
            index,
            failures,
        )
    if failures:
        raise VerificationError(failures)
    return table


def _ordered_value(value: object) -> object:
    if isinstance(value, (DateValue, DateTimeValue)):
        return value.ordering_key
    return value


def _compare_rows(
    left: Mapping[str, object],
    right: Mapping[str, object],
    terms: Sequence[OrderTerm],
) -> int:
    for term in terms:
        left_value = left[term.variable]
        right_value = right[term.variable]
        left_missing = _missing(left_value)
        right_missing = _missing(right_value)
        if left_missing or right_missing:
            if left_missing and right_missing:
                continue
            left_first = term.nulls == "first"
            return -1 if left_missing == left_first else 1

        ordered_left = _ordered_value(left_value)
        ordered_right = _ordered_value(right_value)
        if ordered_left == ordered_right:
            continue
        result = -1 if ordered_left < ordered_right else 1
        return -result if term.direction == "desc" else result
    return 0


def order_table(table: TypedTable, terms: Sequence[OrderTerm]) -> TypedTable:
    """Apply R005 presentation ordering with stable construction-order ties."""
    available = {column.name for column in table.columns}
    names = [term.variable for term in terms]
    if len(names) != len(set(names)):
        raise ValueError("output order terms must not repeat a column")
    for name in names:
        if name not in available:
            raise ValueError(f"unknown output order column {name!r}")
    if not terms or table.frame.height < 2:
        return table

    rows = _rows(table)
    indices = sorted(
        range(len(rows)),
        key=cmp_to_key(
            lambda left, right: _compare_rows(rows[left], rows[right], terms)
        ),
    )
    return TypedTable(columns=table.columns, frame=table.frame.gather(indices))


def finalize_output(
    table: TypedTable,
    output: Output,
    keys: Sequence[str],
    verifications: Sequence[Expression],
) -> TypedTable:
    """Validate output membership and keys, verify, then order for writing."""
    available = {column.name for column in table.columns}
    if not output.columns:
        raise ValueError("output columns must not be empty")
    if len(output.columns) != len(set(output.columns)):
        raise ValueError("output columns must not repeat a column")
    for name in output.columns:
        if name not in available:
            raise ValueError(f"unknown output column {name!r}")
    for key in keys:
        if key not in output.columns:
            raise ValueError(f"key column {key!r} is not an output column")

    verified = verify_dataset(table, keys, verifications)
    return order_table(verified, output.order_by or ())


__all__ = [
    "VerificationError",
    "VerificationFailure",
    "finalize_output",
    "order_table",
    "verify_columns",
    "verify_dataset",
]
