"""Key validation and verifications over one completed table.

Each check reads values that have already finished their R005 lifecycle and
reports failures rather than raising, so an executor can run one at the
stage R005 gives it: a column check after that column's final override, key
validation once every column is complete, and the dataset checks last.
"""

from __future__ import annotations

from collections.abc import Iterable, Mapping, Sequence
from typing import Literal

from pydantic import JsonValue

from yamaa.expressions import (
    MappingResolver,
    PredicateAst,
    PredicateError,
    TruthValue,
    evaluate_predicate,
    parse_predicate,
)
from yamaa.io.polars import runtime_rows, runtime_value
from yamaa.models import (
    MISSING,
    ConditionResult,
    DateTimeValue,
    DateValue,
    RuntimeValue,
    TypedColumn,
    TypedTable,
    ValueResult,
    convert_value,
    normalize_runtime_value,
    runtime_type_name,
)
from yamaa.regex import Regex, RegexError, compile_pattern
from yamaa.specification.models import Column, ColumnType, Expression
from yamaa.verification.diagnostics import (
    REPORTED_KEYS,
    DeclarationError,
    VerificationError,
    VerificationFailure,
)

_NUMERIC: frozenset[ColumnType] = frozenset({"int", "float"})

# The committed `length_failed` fixture names R009-32, the requirement that
# fails a run on any verification failure, where the others name the
# requirement defining their own check. Both identities are reproduced here
# rather than made uniform, because a runtime must report what is committed.
_COLUMN_REQUIREMENTS = {
    "not_missing": ("not_missing_failed", "R009-9"),
    "allowed_values": ("allowed_values_failed", "R009-10"),
    "range": ("range_failed", "R009-11"),
    "max_length": ("length_failed", "R009-32"),
    "matches": ("matches_failed", "R009-13"),
}
_DATASET_REQUIREMENTS = {
    "unique": ("unique_failed", "R009-15"),
    "all_or_none": ("all_or_none_failed", "R009-16"),
    "implies": ("implication_failed", "R009-17"),
    "predicate": ("predicate_failed", "R009-18"),
    "row_count": ("row_count_failed", "R009-19"),
}
_IDENTIFIED = frozenset({"all_or_none", "implies", "predicate"})

KeyMap = dict[str, JsonValue]


def _json(value: RuntimeValue) -> JsonValue:
    if value is MISSING:
        return None
    if isinstance(value, (DateValue, DateTimeValue)):
        return value.to_text()
    return value


def _operation(
    expression: Expression, spec_path: str
) -> tuple[str, Mapping[str, JsonValue]]:
    keyword = expression.operation
    arguments = expression.root[keyword]
    if arguments is None:
        arguments = {}
    if not isinstance(arguments, Mapping):
        raise DeclarationError(
            f"{spec_path}.{keyword}",
            "R009-23",
            "a verification takes named arguments",
        )
    return keyword, arguments


def _values(table: TypedTable, name: str) -> list[RuntimeValue]:
    return [runtime_value(value) for value in table.frame.get_column(name).to_list()]


def _key_maps(table: TypedTable, keys: Sequence[str]) -> list[KeyMap]:
    columns = {name: _values(table, name) for name in keys}
    return [
        {name: _json(columns[name][index]) for name in keys}
        for index in range(table.frame.height)
    ]


def _require_columns(
    table: TypedTable, names: Iterable[str], spec_path: str, requirement: str
) -> None:
    declared = {column.name for column in table.columns}
    for name in names:
        if name not in declared:
            raise DeclarationError(
                spec_path,
                requirement,
                f"unknown column {name!r}",
                condition="unknown_field",
            )


def _failure(
    condition: str,
    spec_path: str,
    requirement: str,
    context: dict[str, JsonValue],
    offending: Sequence[KeyMap],
    *,
    count: int | None = None,
    count_name: str = "failure_count",
    phase: Literal["output", "verification"] = "verification",
    extra: dict[str, JsonValue] | None = None,
) -> VerificationFailure:
    return VerificationFailure(
        phase=phase,
        condition=condition,
        spec_paths=(spec_path,),
        requirement=requirement,
        context={
            **context,
            count_name: len(offending) if count is None else count,
            "keys": list(offending[:REPORTED_KEYS]),
            **(extra or {}),
        },
    )


def _groups(
    values: Sequence[tuple[RuntimeValue, ...]],
) -> dict[tuple[object, ...], list[int]]:
    """Partition row positions by a comparable form of their values."""
    partitions: dict[tuple[object, ...], list[int]] = {}
    for index, combined in enumerate(values):
        partitions.setdefault(tuple(_json(value) for value in combined), []).append(
            index
        )
    return partitions


def _declared(table: TypedTable) -> dict[str, ColumnType]:
    """The names and types a predicate may read on a completed row."""
    return {column.name: column.type for column in table.columns}


def _identifiers(node: object) -> set[str]:
    """Collect every variable one parsed predicate reads."""
    if isinstance(node, Mapping):
        names = {node["name"]} if node.get("kind") == "identifier" else set()
        for value in node.values():
            names |= _identifiers(value)
        return names
    if isinstance(node, list):
        return set().union(*(_identifiers(value) for value in node)) if node else set()
    return set()


def _predicate(
    text: JsonValue,
    spec_path: str,
    requirement: str,
    available: Mapping[str, ColumnType],
) -> PredicateAst:
    if not isinstance(text, str):
        raise DeclarationError(spec_path, requirement, "a predicate must be text")
    try:
        parsed = parse_predicate(text)
    except PredicateError as error:
        raise DeclarationError(
            spec_path,
            "R004-31",
            str(error),
            condition="invalid_predicate",
        ) from error
    # A name is resolved against the declared columns rather than against a
    # row, so a predicate naming a column the artifact does not have is
    # refused even when no row exists to read it on.
    unknown = sorted(_identifiers(parsed) - available.keys())
    if unknown:
        raise DeclarationError(
            spec_path,
            "R009-31",
            f"predicate names unknown column {unknown[0]!r}",
            condition="unknown_field",
            context={"identifier": unknown[0]},
        )
    # Operand compatibility is a property of the declaration and its field
    # schema, not of whichever values happen to occur in a row. Evaluate the
    # parsed predicate once with a representative non-missing value of every
    # declared type so an empty table, or a short-circuited implication, cannot
    # hide an invalid comparison.
    samples: dict[str, RuntimeValue] = {
        name: _PREDICATE_SAMPLES[column_type] for name, column_type in available.items()
    }
    result = evaluate_predicate(parsed, MappingResolver(samples))
    if isinstance(result, ConditionResult):
        _raise_predicate_condition(result, spec_path)
    return parsed


_PREDICATE_SAMPLES: dict[ColumnType, RuntimeValue] = {
    "str": "sample",
    "int": 0,
    "float": 0.0,
    "date": DateValue(year=2000, month=1, day=1),
    "datetime": DateTimeValue(
        year=2000,
        month=1,
        day=1,
        hour=0,
        minute=0,
        second=0,
    ),
}


def _raise_predicate_condition(result: ConditionResult, spec_path: str) -> None:
    condition = result.condition
    requirement = {
        "unknown_field": "R004-32",
        "incompatible_input_type": "R004-33",
        "invalid_predicate": "R004-34",
    }.get(condition.condition, "R009-23")
    raise DeclarationError(
        spec_path,
        requirement,
        f"predicate cannot be evaluated: {condition.condition}",
        condition=condition.condition,
        context=dict(condition.context),
    )


def _truth(
    ast: PredicateAst, row: Mapping[str, RuntimeValue], spec_path: str
) -> TruthValue:
    """Evaluate one predicate, failing rather than reading a condition as UNKNOWN.

    R004 decides an operand incompatibility and an unknown field before any
    row is read, so neither may be folded into three-valued logic: doing so
    would let an unevaluable antecedent silently satisfy an `implies`.
    """
    result = evaluate_predicate(ast, MappingResolver(dict(row)))
    if isinstance(result, ConditionResult):
        _raise_predicate_condition(result, spec_path)
    return result.value


def _record_lookup_bindings(
    table: TypedTable,
    columns: Sequence[TypedColumn],
    rows: Sequence[Mapping[str, object]] | None,
) -> tuple[dict[str, ColumnType], list[dict[str, RuntimeValue]]]:
    """Validate and normalize one lookup binding aligned to every output row."""
    lookup_types = {column.name: column.type for column in columns}
    if len(lookup_types) != len(columns):
        raise ValueError("record lookup column names must be unique")
    if any(
        name.count(".") != 1 or not all(part for part in name.split("."))
        for name in lookup_types
    ):
        raise ValueError("record lookup column names must be qualified")
    output_names = {column.name for column in table.columns}
    if output_names.intersection(lookup_types):
        raise ValueError("record lookup columns must not shadow output columns")

    bindings = [{} for _ in range(table.frame.height)] if rows is None else list(rows)
    if len(bindings) != table.frame.height:
        raise ValueError("record lookup rows must align with output rows")
    expected = set(lookup_types)
    normalized_rows: list[dict[str, RuntimeValue]] = []
    for index, binding in enumerate(bindings):
        if not isinstance(binding, Mapping) or set(binding) != expected:
            raise ValueError("record lookup row fields must match their schema")
        normalized: dict[str, RuntimeValue] = {}
        for name, value in binding.items():
            result = normalize_runtime_value(runtime_value(value))
            if not isinstance(result, ValueResult):
                raise TypeError(
                    f"record lookup row {index} field {name!r} is not a runtime value"
                )
            actual = runtime_type_name(result.value)
            if result.value is not MISSING and actual != lookup_types[name]:
                raise ValueError(
                    f"record lookup row {index} field {name!r} must be "
                    f"{lookup_types[name]}, not {actual}"
                )
            normalized[name] = result.value
        normalized_rows.append(normalized)
    return lookup_types, normalized_rows


def check_column(
    table: TypedTable,
    column: Column,
    keys: Sequence[str],
) -> tuple[VerificationFailure, ...]:
    """Run one column's verifications over its completed values (R005 stage 5)."""
    declarations = column.verifications or ()
    if not declarations:
        return ()
    _require_columns(table, [column.name, *keys], f"columns.{column.name}", "R009-31")
    values = _values(table, column.name)
    key_maps = _key_maps(table, keys)
    failures: list[VerificationFailure] = []
    for index, declaration in enumerate(declarations):
        path = f"columns.{column.name}.verifications[{index}]"
        keyword, arguments = _operation(declaration, path)
        if keyword not in _COLUMN_REQUIREMENTS:
            raise DeclarationError(
                f"{path}.{keyword}",
                "R009-23",
                "unknown column verification",
            )
        condition, requirement = _COLUMN_REQUIREMENTS[keyword]
        offending = _column_offenders(
            keyword, arguments, column, values, key_maps, f"{path}.{keyword}"
        )
        if offending:
            context: dict[str, JsonValue] = {"column": column.name}
            if keyword == "max_length":
                context["max"] = arguments["max"]
            failures.append(
                _failure(
                    condition, f"{path}.{keyword}", requirement, context, offending
                )
            )
    return tuple(failures)


def _column_offenders(
    keyword: str,
    arguments: Mapping[str, JsonValue],
    column: Column,
    values: Sequence[RuntimeValue],
    key_maps: Sequence[KeyMap],
    spec_path: str,
) -> list[KeyMap]:
    present = [
        (index, value) for index, value in enumerate(values) if value is not MISSING
    ]
    if keyword == "not_missing":
        return [
            key_maps[index] for index, value in enumerate(values) if value is MISSING
        ]
    if keyword == "allowed_values":
        accepted = _allowed_values(arguments, column, spec_path)
        return [key_maps[index] for index, value in present if value not in accepted]
    if keyword == "range":
        minimum, maximum = _range_bounds(arguments, column, spec_path)
        return [
            key_maps[index]
            for index, value in present
            if (minimum is not None and value < minimum)
            or (maximum is not None and value > maximum)
        ]
    if keyword == "max_length":
        maximum = _max_length(arguments, column, spec_path)
        return [key_maps[index] for index, value in present if len(value) > maximum]
    pattern = _pattern(arguments, column, spec_path)
    return [key_maps[index] for index, value in present if pattern.find(value) is None]


def _require_type(
    column: Column, admitted: frozenset[ColumnType], spec_path: str
) -> None:
    if column.type not in admitted:
        raise DeclarationError(
            spec_path,
            "R009-30",
            f"a {column.type} column does not admit this verification",
        )


def _allowed_values(
    arguments: Mapping[str, JsonValue], column: Column, spec_path: str
) -> list[RuntimeValue]:
    listed = arguments.get("values")
    if not isinstance(listed, list) or not listed:
        raise DeclarationError(spec_path, "R009-23", "allowed_values requires values")
    accepted: list[RuntimeValue] = []
    for value in listed:
        converted = convert_value(value, column.type)
        if not isinstance(converted, ValueResult) or converted.value is MISSING:
            raise DeclarationError(
                spec_path,
                "R009-30",
                f"listed value {value!r} is not a {column.type} value",
            )
        accepted.append(converted.value)
    return accepted


def _range_bounds(
    arguments: Mapping[str, JsonValue], column: Column, spec_path: str
) -> tuple[float | int | None, float | int | None]:
    _require_type(column, _NUMERIC, spec_path)
    minimum = arguments.get("min")
    maximum = arguments.get("max")
    for bound in (minimum, maximum):
        if bound is not None and (
            isinstance(bound, bool) or not isinstance(bound, (int, float))
        ):
            raise DeclarationError(spec_path, "R009-23", "a range bound is numeric")
    if minimum is None and maximum is None:
        raise DeclarationError(spec_path, "R009-25", "range requires one bound")
    if minimum is not None and maximum is not None and minimum > maximum:
        raise DeclarationError(spec_path, "R009-25", "range min exceeds max")
    return minimum, maximum


def _max_length(
    arguments: Mapping[str, JsonValue], column: Column, spec_path: str
) -> int:
    _require_type(column, frozenset({"str"}), spec_path)
    maximum = arguments.get("max")
    if isinstance(maximum, bool) or not isinstance(maximum, int):
        raise DeclarationError(spec_path, "R009-23", "max_length requires a max")
    if maximum < 1:
        raise DeclarationError(spec_path, "R009-26", "max_length max is at least one")
    return maximum


def _pattern(
    arguments: Mapping[str, JsonValue], column: Column, spec_path: str
) -> Regex:
    _require_type(column, frozenset({"str"}), spec_path)
    pattern = arguments.get("pattern")
    if not isinstance(pattern, str):
        raise DeclarationError(spec_path, "R009-23", "matches requires a pattern")
    try:
        # `matches` is a search, so the pattern source is compiled as written
        # and is not anchored.
        return compile_pattern(pattern)
    except RegexError as error:
        raise DeclarationError(
            f"{spec_path}.pattern",
            "R022-27",
            error.reason,
            condition="invalid_regex",
            context={"pattern": pattern},
        ) from error


def check_keys(
    table: TypedTable, keys: Sequence[str]
) -> tuple[VerificationFailure, ...]:
    """Validate output identity once every column's lifecycle is complete."""
    if not keys:
        raise DeclarationError("keys", "R005-47", "keys names at least one column")
    if len(set(keys)) != len(keys):
        raise DeclarationError("keys", "R005-47", "a key column is repeated")
    _require_columns(table, keys, "keys", "R005-47")

    columns = {name: _values(table, name) for name in keys}
    key_maps = _key_maps(table, keys)
    failures: list[VerificationFailure] = []
    for position, name in enumerate(keys):
        offending = [
            key_maps[index]
            for index, value in enumerate(columns[name])
            if value is MISSING
        ]
        if offending:
            failures.append(
                _failure(
                    "missing_key",
                    f"keys[{position}]",
                    "R005-52",
                    {"column": name},
                    offending,
                    count_name="missing_count",
                    phase="output",
                )
            )

    combined = [
        tuple(columns[name][index] for name in keys)
        for index in range(table.frame.height)
    ]
    duplicated = [
        key_maps[positions[0]]
        for positions in _groups(combined).values()
        if len(positions) > 1
    ]
    if duplicated:
        failures.append(
            _failure(
                "duplicate_key",
                "keys",
                "R005-52",
                {},
                duplicated,
                count_name="duplicate_count",
                phase="output",
            )
        )
    return tuple(failures)


def check_dataset(
    table: TypedTable,
    verifications: Sequence[Expression],
    keys: Sequence[str],
    *,
    record_lookup_columns: Sequence[TypedColumn] = (),
    record_lookup_rows: Sequence[Mapping[str, object]] | None = None,
) -> tuple[VerificationFailure, ...]:
    """Run dataset verifications with resolved per-row lookup bindings."""
    if not verifications:
        if record_lookup_columns or record_lookup_rows is not None:
            _record_lookup_bindings(table, record_lookup_columns, record_lookup_rows)
        return ()
    _require_columns(table, keys, "keys", "R005-47")
    lookup_types, lookup_rows = _record_lookup_bindings(
        table, record_lookup_columns, record_lookup_rows
    )
    identifiers: dict[str, str] = {}
    failures: list[VerificationFailure] = []
    key_maps = _key_maps(table, keys)
    rows = runtime_rows(table)
    predicate_types = {**_declared(table), **lookup_types}
    predicate_rows = [
        {**row, **lookup_row} for row, lookup_row in zip(rows, lookup_rows, strict=True)
    ]
    for index, declaration in enumerate(verifications):
        path = f"verifications[{index}]"
        keyword, arguments = _operation(declaration, path)
        if keyword not in _DATASET_REQUIREMENTS:
            raise DeclarationError(
                f"{path}.{keyword}", "R009-23", "unknown dataset verification"
            )
        spec_path = f"{path}.{keyword}"
        identifier = _identifier(keyword, arguments, spec_path)
        if identifier is not None:
            if identifier in identifiers:
                raise DeclarationError(
                    spec_path,
                    "R009-24",
                    f"verification id {identifier!r} repeats {identifiers[identifier]}",
                    condition="duplicate_identifier",
                )
            identifiers[identifier] = spec_path
        failure = _dataset_failure(
            keyword,
            arguments,
            table,
            rows,
            predicate_rows,
            predicate_types,
            key_maps,
            identifier,
            spec_path,
        )
        if failure is not None:
            failures.append(failure)
    return tuple(failures)


def _identifier(
    keyword: str, arguments: Mapping[str, JsonValue], spec_path: str
) -> str | None:
    identifier = arguments.get("id")
    grouped = keyword == "row_count" and arguments.get("group_by") is not None
    if identifier is None:
        if keyword in _IDENTIFIED or grouped:
            raise DeclarationError(
                spec_path,
                "R009-28" if grouped else "R009-8",
                f"{keyword} requires a verification id",
                condition="missing_verification_id",
            )
        return None
    if not isinstance(identifier, str) or not identifier:
        raise DeclarationError(spec_path, "R009-8", "a verification id is text")
    return identifier


def _implication_fails(
    when: PredicateAst,
    then: PredicateAst,
    row: Mapping[str, RuntimeValue],
    when_path: str,
    then_path: str,
) -> bool:
    """Evaluate both predicates before applying implication truth semantics."""
    when_truth = _truth(when, row, when_path)
    then_truth = _truth(then, row, then_path)
    return when_truth is TruthValue.TRUE and then_truth is not TruthValue.TRUE


def _dataset_failure(
    keyword: str,
    arguments: Mapping[str, JsonValue],
    table: TypedTable,
    rows: Sequence[Mapping[str, RuntimeValue]],
    predicate_rows: Sequence[Mapping[str, RuntimeValue]],
    predicate_types: Mapping[str, ColumnType],
    key_maps: Sequence[KeyMap],
    identifier: str | None,
    spec_path: str,
) -> VerificationFailure | None:
    condition, requirement = _DATASET_REQUIREMENTS[keyword]
    context: dict[str, JsonValue] = {}
    if identifier is not None:
        context["verification_id"] = identifier

    if keyword == "row_count":
        return _row_count_failure(
            arguments,
            table,
            rows,
            predicate_rows,
            predicate_types,
            condition,
            requirement,
            context,
            spec_path,
        )

    if keyword == "unique":
        names = _column_list(arguments, "columns", table, spec_path)
        combined = [tuple(row[name] for name in names) for row in rows]
        repeated = [
            positions for positions in _groups(combined).values() if len(positions) > 1
        ]
        if not repeated:
            return None
        context["columns"] = list(names)
        # A repeated combination is one failure, and every row carrying it
        # is reported, because the offending rows differ in their own keys.
        return _failure(
            condition,
            spec_path,
            requirement,
            context,
            [key_maps[position] for positions in repeated for position in positions],
            count=len(repeated),
        )

    if keyword == "all_or_none":
        names = _column_list(arguments, "columns", table, spec_path)
        if len(set(names)) < 2:
            raise DeclarationError(
                spec_path, "R009-27", "all_or_none names two distinct columns"
            )
        offending = [
            key_maps[index]
            for index, row in enumerate(rows)
            if len({row[name] is MISSING for name in names}) > 1
        ]
    elif keyword == "implies":
        when_path = f"{spec_path}.when"
        then_path = f"{spec_path}.then"
        when = _predicate(arguments.get("when"), when_path, "R009-23", predicate_types)
        then = _predicate(arguments.get("then"), then_path, "R009-23", predicate_types)
        offending = [
            key_maps[index]
            for index, row in enumerate(predicate_rows)
            if _implication_fails(when, then, row, when_path, then_path)
        ]
    else:
        assertion_path = f"{spec_path}.assert"
        assertion = _predicate(
            arguments.get("assert"), assertion_path, "R009-23", predicate_types
        )
        offending = [
            key_maps[index]
            for index, row in enumerate(predicate_rows)
            if _truth(assertion, row, assertion_path) is not TruthValue.TRUE
        ]

    if not offending:
        return None
    return _failure(condition, spec_path, requirement, context, offending)


def _column_list(
    arguments: Mapping[str, JsonValue],
    field: str,
    table: TypedTable,
    spec_path: str,
) -> list[str]:
    names = arguments.get(field)
    if not isinstance(names, list) or not names:
        raise DeclarationError(
            spec_path, "R009-23", f"{field} names at least one column"
        )
    if any(not isinstance(name, str) for name in names):
        raise DeclarationError(spec_path, "R009-23", f"{field} names are text")
    _require_columns(table, names, f"{spec_path}.{field}", "R009-31")
    return names


def _row_count_failure(
    arguments: Mapping[str, JsonValue],
    table: TypedTable,
    rows: Sequence[Mapping[str, RuntimeValue]],
    predicate_rows: Sequence[Mapping[str, RuntimeValue]],
    predicate_types: Mapping[str, ColumnType],
    condition: str,
    requirement: str,
    context: dict[str, JsonValue],
    spec_path: str,
) -> VerificationFailure | None:
    minimum = arguments.get("min")
    maximum = arguments.get("max")
    for bound in (minimum, maximum):
        if bound is not None and (
            isinstance(bound, bool) or not isinstance(bound, int)
        ):
            raise DeclarationError(spec_path, "R009-23", "a row_count bound is an int")
    if minimum is None and maximum is None:
        raise DeclarationError(spec_path, "R009-25", "row_count requires one bound")
    if minimum is not None and maximum is not None and minimum > maximum:
        raise DeclarationError(spec_path, "R009-25", "row_count min exceeds max")

    grouped = arguments.get("group_by") is not None
    if grouped:
        names = _column_list(arguments, "group_by", table, spec_path)
        if len(set(names)) != len(names):
            raise DeclarationError(
                spec_path, "R009-29", "row_count group_by repeats a column"
            )
        partitions = _groups([tuple(row[name] for name in names) for row in rows])
    else:
        names = []
        # R009-19 bounds the whole output, so the ungrouped count exists even
        # when no row does: an empty artifact must still meet a `min`.
        partitions = {(): list(range(len(rows)))}

    admitted = set(range(len(rows)))
    if arguments.get("filter") is not None:
        filter_path = f"{spec_path}.filter"
        predicate = _predicate(
            arguments["filter"], filter_path, "R009-23", predicate_types
        )
        admitted = {
            index
            for index, row in enumerate(predicate_rows)
            if _truth(predicate, row, filter_path) is TruthValue.TRUE
        }

    # R009-20 partitions the artifact rather than the counted rows, so a
    # group whose filter admits nothing still exists and still fails a `min`.
    offending: list[tuple[KeyMap, int]] = []
    for combined, positions in partitions.items():
        count = sum(1 for position in positions if position in admitted)
        if (minimum is not None and count < minimum) or (
            maximum is not None and count > maximum
        ):
            offending.append(
                ({name: combined[order] for order, name in enumerate(names)}, count)
            )
    if not offending:
        return None
    shown = offending[:REPORTED_KEYS]
    counts: dict[str, JsonValue]
    if len(offending) == 1:
        counts = {"count": offending[0][1]}
    else:
        # Each count is aligned to the group at the same position in `keys`.
        # Keeping only the first count would make every later group ambiguous.
        counts = {"counts": [count for _, count in shown]}
    return _failure(
        condition,
        spec_path,
        requirement,
        context,
        [group for group, _ in offending],
        extra=counts,
    )


def verify_completed_table(
    table: TypedTable,
    columns: Sequence[Column],
    keys: Sequence[str],
    verifications: Sequence[Expression] = (),
    *,
    record_lookup_columns: Sequence[TypedColumn] = (),
    record_lookup_rows: Sequence[Mapping[str, object]] | None = None,
) -> TypedTable:
    """Run every stage in R005 order and raise at the first that fails.

    Column verifications run first, then output-key validation, then the
    dataset verifications R009 runs last. A stage that fails stops the run
    with every failure it found, because a later stage asserts over values
    an earlier one has already refused.
    """
    failures = [
        failure for column in columns for failure in check_column(table, column, keys)
    ]
    if failures:
        raise VerificationError(failures)

    failures = list(check_keys(table, keys))
    if failures:
        raise VerificationError(failures)

    failures = list(
        check_dataset(
            table,
            verifications,
            keys,
            record_lookup_columns=record_lookup_columns,
            record_lookup_rows=record_lookup_rows,
        )
    )
    if failures:
        raise VerificationError(failures)
    return table
