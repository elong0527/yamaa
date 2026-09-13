"""Parser and reducer engine for the closed R013 aggregate language.

`yaml/grammar/aggregate.yaml` is the single source for this grammar, its
closed reducer vocabulary, and the spellings it reserves for constructs it
refuses. The arithmetic around a reduction is R010's, reused by reference:
every operator, function, promotion, and failure comes from
`yamaa.expressions.numeric` rather than from a second implementation here.

The engine reduces records its caller supplies. It never selects them: which
relation a reduction reads, and which of its records are eligible, belong to
R003 and R007 and stay with the runtime that owns the join.
"""

from __future__ import annotations

import re
from collections.abc import Mapping, Sequence
from functools import lru_cache
from typing import Any, Final, Protocol, TypeAlias

from pydantic import JsonValue

from yamaa.expressions.core import (
    ExpressionHandler,
    MappingResolver,
    Resolver,
    expression_condition,
)
from yamaa.expressions.numeric import (
    FUNCTION_ARITIES,
    PROHIBITED_KEYWORDS,
    evaluate_numeric,
)
from yamaa.models import (
    MISSING,
    ConditionPhase,
    ConditionResult,
    EvaluationResult,
    RuntimeCondition,
    RuntimeValue,
    ValueResult,
    normalize_runtime_value,
    runtime_type_name,
    values_comparable,
)

AggregateAst: TypeAlias = dict[str, Any]
_Token: TypeAlias = tuple[str, str, int, int]

# R013-12 closes the reducer table. The flag says which reducer may take the
# record star, and `COUNT` is the one that does: it counts records where the
# others count values.
REDUCERS: Final[dict[str, bool]] = {
    "SUM": False,
    "COUNT": True,
    "MIN": False,
    "MAX": False,
    "MEAN": False,
    "ONLY": False,
}

_NUMBER = re.compile(r"[0-9]+(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?")
_NAME = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
_OPERATORS: Final[dict[str, str]] = {
    "+": "PLUS",
    "-": "MINUS",
    "*": "STAR",
    "/": "SLASH",
    "(": "LPAREN",
    ")": "RPAREN",
    ",": "COMMA",
}


class AggregateError(ValueError):
    """An R013 expression cannot be tokenized, parsed, or closed."""

    def __init__(
        self,
        message: str,
        position: int,
        *,
        condition: str = "invalid_aggregate_expression",
        requirement: str = "R013-34",
        context: dict[str, JsonValue] | None = None,
    ) -> None:
        super().__init__(f"{message} at character {position + 1}")
        self.position = position
        self.condition = condition
        self.requirement = requirement
        self.context: dict[str, JsonValue] = dict(context or {})


def _prohibited_construct(construct: str, position: int) -> AggregateError:
    return AggregateError(
        f"a {construct} construct is not permitted in an aggregate expression",
        position,
        condition="prohibited_construct",
        requirement="R013-47",
        context={"construct": construct},
    )


def _prohibited_function(
    name: str,
    position: int,
    message: str,
    context: dict[str, JsonValue],
) -> AggregateError:
    return AggregateError(
        message,
        position,
        condition="prohibited_function",
        requirement="R013-35",
        context={"function": name, **context},
    )


def _tokenize(text: str) -> list[_Token]:
    """Scan R013's lexemes, including the record star `COUNT` may take."""
    tokens: list[_Token] = []
    index = 0
    length = len(text)
    while index < length:
        character = text[index]
        if character.isspace():
            index += 1
            continue

        number = _NUMBER.match(text, index)
        if number is not None:
            tokens.append(("NUMBER", number.group(0), index, number.end()))
            index = number.end()
            continue

        name = _NAME.match(text, index)
        if name is not None:
            value = name.group(0)
            end = name.end()
            kind = "NAME"
            if end < length and text[end] == ".":
                if text.startswith(".*", end):
                    value += ".*"
                    end += 2
                    kind = "QUALIFIED_STAR"
                else:
                    suffix = _NAME.match(text, end + 1)
                    if suffix is None:
                        raise AggregateError("invalid qualified identifier", index)
                    value += "." + suffix.group(0)
                    end = suffix.end()
            construct = PROHIBITED_KEYWORDS.get(value.upper())
            if "." not in value and construct is not None:
                raise _prohibited_construct(construct, index)
            tokens.append((kind, value, index, end))
            index = end
            continue

        operator = _OPERATORS.get(character)
        if operator is not None:
            tokens.append((operator, character, index, index + 1))
            index += 1
            continue

        if character in "<>=!":
            raise _prohibited_construct("comparison", index)
        if character == "'":
            raise _prohibited_construct("string", index)
        raise AggregateError(f"unexpected character {character!r}", index)

    tokens.append(("EOF", "", length, length))
    return tokens


class _Parser:
    """Recursive-descent parser for R013's productions."""

    def __init__(self, text: str) -> None:
        self._text = text
        self._tokens = _tokenize(text)
        self._index = 0

    @property
    def _token(self) -> _Token:
        return self._tokens[self._index]

    def _advance(self) -> _Token:
        token = self._token
        self._index += 1
        return token

    def parse(self) -> AggregateAst:
        node = self._expression()
        if self._token[0] != "EOF":
            raise AggregateError("unexpected trailing token", self._token[2])
        return node

    def _expression(self) -> AggregateAst:
        node = self._term()
        while self._token[0] in {"PLUS", "MINUS"}:
            operator = self._advance()
            node = {
                "kind": "binary",
                "operator": operator[1],
                "left": node,
                "right": self._term(),
            }
        return node

    def _term(self) -> AggregateAst:
        node = self._factor()
        while self._token[0] in {"STAR", "SLASH"}:
            operator = self._advance()
            node = {
                "kind": "binary",
                "operator": operator[1],
                "left": node,
                "right": self._factor(),
            }
        return node

    def _factor(self) -> AggregateAst:
        # R013-11 keeps R010's unary sign, so a sign binds to one primary.
        if self._token[0] in {"PLUS", "MINUS"}:
            operator = self._advance()
            return {
                "kind": "unary",
                "operator": operator[1],
                "value": self._primary(),
            }
        return self._primary()

    def _primary(self) -> AggregateAst:
        token = self._token
        if token[0] == "NUMBER":
            self._advance()
            written = token[1]
            fractional = "." in written or "e" in written.lower()
            return {
                "kind": "number",
                "type": "float" if fractional else "int",
                "value": written,
            }
        if token[0] == "NAME":
            self._advance()
            if self._token[0] != "LPAREN":
                if token[1].upper() == "NULL":
                    return {"kind": "null"}
                return {"kind": "identifier", "name": token[1]}
            if token[1].upper() in REDUCERS:
                return self._reduction(token)
            return self._call(token)
        if token[0] == "LPAREN":
            self._advance()
            node = self._expression()
            if self._token[0] != "RPAREN":
                raise AggregateError("expected ')' to close expression", self._token[2])
            self._advance()
            return node
        if token[0] == "QUALIFIED_STAR":
            raise AggregateError(
                "the record star is valid only as a COUNT argument",
                token[2],
            )
        raise AggregateError(
            "expected a number, identifier, reducer, function, NULL, or parenthesis",
            token[2],
        )

    def _reduction(self, name_token: _Token) -> AggregateAst:
        name = name_token[1].upper()
        self._advance()
        if self._token[0] == "QUALIFIED_STAR":
            star = self._advance()
            if not REDUCERS[name]:
                raise AggregateError(
                    f"reducer {name_token[1]!r} counts values and takes no star",
                    star[2],
                )
            argument: AggregateAst = {"kind": "star", "dataset": star[1][:-2]}
        else:
            argument = self._expression()
        if self._token[0] != "RPAREN":
            raise AggregateError("expected ')' to close reducer", self._token[2])
        close = self._advance()
        inner = _first_reduction(argument)
        if inner is not None:
            # R013-18: reductions do not nest, and the failure names both.
            raise AggregateError(
                f"reducer {name!r} contains reducer {inner['name']!r}",
                name_token[2],
                condition="nested_reduction",
                requirement="R013-37",
                context={"outer": name, "inner": inner["name"]},
            )
        return {
            "kind": "reduction",
            "name": name,
            "argument": argument,
            "text": self._text[name_token[2] : close[3]],
        }

    def _call(self, name_token: _Token) -> AggregateAst:
        self._advance()
        arguments: list[AggregateAst] = []
        if self._token[0] != "RPAREN":
            arguments.append(self._expression())
            while self._token[0] == "COMMA":
                self._advance()
                arguments.append(self._expression())
        if self._token[0] != "RPAREN":
            raise AggregateError("expected ')' to close function", self._token[2])
        self._advance()
        # The vocabulary closes only after the call parses, so an unclosed
        # call reports the syntax defect rather than the function name.
        name = name_token[1]
        arity = FUNCTION_ARITIES.get(name.upper())
        if arity is None:
            raise _prohibited_function(
                name,
                name_token[2],
                f"function {name!r} is not permitted by R013",
                {},
            )
        minimum, maximum = arity
        count = len(arguments)
        if count < minimum or (maximum is not None and count > maximum):
            expected = str(minimum) if maximum == minimum else f"at least {minimum}"
            raise _prohibited_function(
                name,
                name_token[2],
                f"function {name!r} requires {expected} argument(s), got {count}",
                {"argument_count": count},
            )
        return {"kind": "call", "name": name.upper(), "arguments": arguments}


def _children(node: AggregateAst) -> tuple[AggregateAst, ...]:
    kind = node["kind"]
    if kind == "unary":
        return (node["value"],)
    if kind == "binary":
        return (node["left"], node["right"])
    if kind == "call":
        return tuple(node["arguments"])
    if kind == "reduction":
        return (node["argument"],)
    return ()


def _first_reduction(node: AggregateAst) -> AggregateAst | None:
    if node["kind"] == "reduction":
        return node
    for child in _children(node):
        found = _first_reduction(child)
        if found is not None:
            return found
    return None


def _nodes(ast: AggregateAst) -> list[AggregateAst]:
    """Return every node in written order, so a name is seen where it reads."""
    seen: list[AggregateAst] = []
    pending = [ast]
    while pending:
        node = pending.pop()
        seen.append(node)
        pending.extend(reversed(_children(node)))
    return seen


def parse_aggregate(text: str) -> AggregateAst:
    """Parse one `aggregate.expr` under the closed R013 grammar."""
    if not isinstance(text, str) or not text:
        raise AggregateError("aggregate expression must be a non-empty string", 0)
    return _Parser(text).parse()


@lru_cache(maxsize=512)
def _parse_cached(text: str) -> AggregateAst:
    return parse_aggregate(text)


def parse_aggregate_cached(text: str) -> AggregateAst:
    """Parse one expression, reusing the parse of a repeated text.

    Parsing is a pure function of the text, so a row loop re-reading one
    `aggregate.expr` parses it once. The cached AST is never mutated.
    """
    return _parse_cached(text)


def aggregate_identifiers(ast: AggregateAst) -> tuple[str, ...]:
    """Return every identifier the expression binds, in first-seen order."""
    names = [node["name"] for node in _nodes(ast) if node["kind"] == "identifier"]
    return tuple(dict.fromkeys(names))


def aggregate_star_datasets(ast: AggregateAst) -> tuple[str, ...]:
    """Return the dataset each `COUNT(D.*)` names, in first-seen order."""
    names = [node["dataset"] for node in _nodes(ast) if node["kind"] == "star"]
    return tuple(dict.fromkeys(names))


def ungrouped_identifiers(ast: AggregateAst) -> tuple[str, ...]:
    """Return the identifiers R013-20 requires the enclosing grain to declare.

    A value that varies within the group gives the expression no single
    answer, so every identifier a reduction does not enclose must be grouped
    on. Which columns are grouped on is the context's to know, so this
    reports the names and the caller checks them.
    """
    outside: list[str] = []

    def visit(node: AggregateAst) -> None:
        if node["kind"] == "reduction":
            return
        if node["kind"] == "identifier":
            outside.append(node["name"])
            return
        for child in _children(node):
            visit(child)

    visit(ast)
    return tuple(dict.fromkeys(outside))


def is_single_reduction(ast: AggregateAst) -> bool:
    """Return whether the whole expression is one reduction (R013-22)."""
    return ast["kind"] == "reduction"


class _ReductionFailure(Exception):
    """Carry one structured condition out of a recursive reduction."""

    def __init__(self, condition: ConditionResult) -> None:
        self.condition = condition
        super().__init__(condition.condition.condition)


def _fail(
    condition: str,
    requirement: str,
    context: dict[str, JsonValue],
    *,
    phase: ConditionPhase = "derivation",
) -> _ReductionFailure:
    return _ReductionFailure(
        ConditionResult(
            condition=RuntimeCondition(
                phase=phase,
                condition=condition,
                context=context,
                requirement=requirement,
            )
        )
    )


# R013-15 folds `SUM` with R010's `+`, and R013-14 divides with R010's `/`.
# Both run through the numeric evaluator, so binary64 rounding and integer
# overflow are R010's behavior rather than a second implementation's.
_LEFT: Final[AggregateAst] = {"kind": "identifier", "name": "left"}
_RIGHT: Final[AggregateAst] = {"kind": "identifier", "name": "right"}
_ADD: Final[AggregateAst] = {
    "kind": "binary",
    "operator": "+",
    "left": _LEFT,
    "right": _RIGHT,
}
_DIVIDE: Final[AggregateAst] = {
    "kind": "binary",
    "operator": "/",
    "left": _LEFT,
    "right": _RIGHT,
}


def _arithmetic(
    operation: AggregateAst,
    left: RuntimeValue,
    right: RuntimeValue,
    expr: str,
) -> RuntimeValue:
    result = evaluate_numeric(
        operation,
        expr,
        MappingResolver({"left": left, "right": right}),
    )
    if isinstance(result, ConditionResult):
        raise _ReductionFailure(result)
    assert isinstance(result, ValueResult)
    return result.value


def _numeric_or_fail(
    value: RuntimeValue,
    reducer: str,
    expr: str,
    source: str,
) -> RuntimeValue:
    if runtime_type_name(value) in {"int", "float"}:
        return value
    # R013-45: a non-numeric argument fails rather than being coerced.
    raise _fail(
        "incompatible_input_type",
        "R013-45",
        {
            "expr": expr,
            "reducer": reducer,
            "source": source,
            "expected": "numeric",
            "actual": runtime_type_name(value),
        },
        phase="validation",
    )


def _ordering_key(value: RuntimeValue) -> object:
    ordering = getattr(value, "ordering_key", None)
    return ordering if ordering is not None else value


def _extreme(
    values: Sequence[RuntimeValue],
    expr: str,
    source: str,
    *,
    largest: bool,
) -> RuntimeValue:
    for other in values[1:]:
        if values_comparable(values[0], other):
            continue
        # R013-46: a column mixing incomparable types has no order, and
        # inventing one would make the result implementation-defined.
        raise _fail(
            "incomparable_sources",
            "R013-46",
            {
                "expr": expr,
                "sources": [source],
                "types": sorted(
                    {
                        name
                        for name in (runtime_type_name(value) for value in values)
                        if name is not None
                    }
                ),
            },
            phase="validation",
        )
    chooser = max if largest else min
    return chooser(values, key=_ordering_key)  # type: ignore[return-value]


def _argument_value(
    argument: AggregateAst,
    expr: str,
    record: Mapping[str, object],
) -> RuntimeValue:
    """Read one record's contribution to a reduction.

    A bare identifier keeps whatever type its column carries, because
    R013-24 lets `COUNT`, `MIN`, `MAX`, and `ONLY` reduce any type. An
    argument that computes is R010 arithmetic and fails on a non-numeric
    operand where that arithmetic already lives.
    """
    if argument["kind"] == "identifier":
        name = argument["name"]
        if name not in record:
            raise _fail(
                "unknown_field",
                "R013-41",
                {"expr": expr, "identifier": name},
                phase="validation",
            )
        normalized = normalize_runtime_value(record[name])
        if isinstance(normalized, ConditionResult):
            raise _ReductionFailure(normalized)
        assert isinstance(normalized, ValueResult)
        return normalized.value
    result = evaluate_numeric(argument, expr, MappingResolver(dict(record)))
    if isinstance(result, ConditionResult):
        raise _ReductionFailure(result)
    assert isinstance(result, ValueResult)
    return result.value


def _reduce(
    node: AggregateAst,
    expr: str,
    records: Sequence[Mapping[str, object]],
    phase: ConditionPhase,
    context: Mapping[str, JsonValue],
) -> RuntimeValue:
    """Reduce one relation's records to the value R013-27 pins for it."""
    reducer = node["name"]
    argument = node["argument"]
    source = argument.get("name") or argument.get("dataset") or expr

    if argument["kind"] == "star":
        # R013-19: the record star counts records and names no column.
        return len(records) if records else MISSING
    if not records:
        # R013-27: no record in the group leaves every reducer missing.
        return MISSING

    values = [_argument_value(argument, expr, record) for record in records]

    if reducer == "ONLY":
        # R013-17: `ONLY` counts records rather than values, and R013-29
        # makes more than one a failure rather than a missing-value case.
        if len(records) > 1:
            raise _fail(
                "aggregate_multiple_records",
                "R013-36",
                {
                    "expr": expr,
                    "reducer": reducer,
                    "record_count": len(records),
                    **dict(context),
                },
                phase=phase,
            )
        return values[0]

    present = [value for value in values if value is not MISSING]
    if reducer == "COUNT":
        # R013-27: the records exist, so an all-missing group counts zero.
        return len(present)
    if not present:
        return MISSING
    if reducer in {"MIN", "MAX"}:
        return _extreme(present, expr, str(source), largest=reducer == "MAX")

    total = _numeric_or_fail(present[0], reducer, expr, str(source))
    for value in present[1:]:
        total = _arithmetic(
            _ADD,
            total,
            _numeric_or_fail(value, reducer, expr, str(source)),
            expr,
        )
    if reducer == "SUM":
        return total
    # R013-30: `MEAN` answers missing above before it divides, so an
    # all-missing group never reaches a division by zero.
    return _arithmetic(_DIVIDE, total, len(present), expr)


def _substituted(node: AggregateAst) -> AggregateAst:
    """Rewrite each reduction as the identifier holding its reduced value."""
    kind = node["kind"]
    if kind == "reduction":
        return {"kind": "identifier", "name": node["text"]}
    if kind == "unary":
        return {**node, "value": _substituted(node["value"])}
    if kind == "binary":
        return {
            **node,
            "left": _substituted(node["left"]),
            "right": _substituted(node["right"]),
        }
    if kind == "call":
        return {
            **node,
            "arguments": [_substituted(argument) for argument in node["arguments"]],
        }
    return node


def evaluate_aggregate(
    ast: AggregateAst,
    expr: str,
    records: Sequence[Mapping[str, object]],
    grouped: Mapping[str, object] | None = None,
    *,
    phase: ConditionPhase = "derivation",
    context: Mapping[str, JsonValue] | None = None,
) -> EvaluationResult:
    """Reduce the supplied records to the one value this expression names.

    `records` are the eligible records of one relation, already narrowed and
    already in relation order, because R013-33 makes a reduction read that
    order rather than impose one of its own. Each maps an identifier as the
    expression writes it to that record's value. `grouped` holds the
    identifiers the enclosing grain declares, which R013-21 makes constant
    within the group.
    """
    reported = dict(context or {})
    try:
        reductions = {
            node["text"]: _reduce(node, expr, records, phase, reported)
            for node in _nodes(ast)
            if node["kind"] == "reduction"
        }
    except _ReductionFailure as failure:
        return failure.condition

    if ast["kind"] == "reduction":
        # R013-22: a single reduction retains its own result type, whatever
        # that type is, so no numeric contract applies to the whole result.
        return ValueResult(value=reductions[ast["text"]])

    # R013-23: an expression using any operator or function is numeric, and
    # every reduction and grouped identifier in it must be too, which is
    # exactly what R010's evaluator already requires of an identifier.
    values: dict[str, object] = {**dict(grouped or {}), **reductions}
    return evaluate_numeric(_substituted(ast), expr, MappingResolver(values))


class RelationalResolver(Protocol):
    """Resolver extension for the operations that read a whole relation.

    R003's join and R013's reduction reach records scalar resolution cannot
    see, so the runtime that owns the relation answers for them here rather
    than every expression widening its contract to carry a join engine.
    """

    def resolve_relation(
        self,
        operation: str,
        payload: Mapping[str, object],
    ) -> EvaluationResult: ...


def _relational(operation: str) -> ExpressionHandler:
    def handler(payload: object, resolver: Resolver) -> EvaluationResult:
        if isinstance(payload, str):
            payload = {"expr": payload}
        if not isinstance(payload, Mapping):
            return expression_condition(
                "validation",
                "invalid_field_type",
                {"operation": operation, "expected": "a mapping"},
                requirement="R007-36",
            )
        resolve_relation = getattr(resolver, "resolve_relation", None)
        if not callable(resolve_relation):
            return expression_condition(
                "validation",
                "invalid_field_type",
                {"operation": operation, "reason": "resolver unsupported"},
            )
        return resolve_relation(operation, dict(payload))

    return handler


def aggregate_handlers() -> dict[str, ExpressionHandler]:
    """Return the relational operations this component registers.

    Both read records rather than one bound value, so each is dispatched to
    the resolver that owns the relation. R003, R007, and R013 decide what
    comes back; this map fixes only that the keyword is registered.
    """
    return {
        "aggregate": _relational("aggregate"),
        "mapping_from": _relational("mapping_from"),
    }
