"""Parser and evaluator for the closed R010 scalar numeric language.

`yaml/grammar/numeric.yaml` is the single source for the grammar, its closed
function vocabulary, and the spellings it reserves for constructs it refuses.
This module reads that language and nothing else: there is no host `eval`, no
widened vocabulary, and no reassociation of the written expression.
"""

from __future__ import annotations

import math
import re
from collections.abc import Mapping
from functools import lru_cache
from typing import Any, TypeAlias

from pydantic import JsonValue

from yamaa.expressions.core import (
    AbsentValue,
    ExpressionHandler,
    FailedResolution,
    ResolvedValue,
    Resolver,
    expression_condition,
)
from yamaa.models import (
    INT64_MAX,
    INT64_MIN,
    MISSING,
    ConditionResult,
    EvaluationResult,
    RuntimeCondition,
    ValueResult,
    normalize_runtime_value,
    runtime_type_name,
)

NumericAst: TypeAlias = dict[str, Any]
Token: TypeAlias = tuple[str, str, int, int]

# R010-9 closes the function table; `None` is an unbounded maximum.
FUNCTION_ARITIES: dict[str, tuple[int, int | None]] = {
    "ABS": (1, 1),
    "CEIL": (1, 1),
    "FLOOR": (1, 1),
    "TRUNC": (1, 1),
    "SQRT": (1, 1),
    "POWER": (2, 2),
    "EXP": (1, 1),
    "LN": (1, 1),
    "MOD": (2, 2),
    "GREATEST": (2, None),
    "LEAST": (2, None),
    "NULLIF": (2, 2),
    "COALESCE": (1, None),
}

# A spelling R010 reserves for a construct the grammar does not admit, so the
# failure names the construct rather than a stray token.
PROHIBITED_KEYWORDS: dict[str, str] = {
    "AND": "boolean",
    "BETWEEN": "comparison",
    "CASE": "conditional",
    "ELSE": "conditional",
    "END": "conditional",
    "FALSE": "boolean",
    "IN": "comparison",
    "IS": "comparison",
    "LIKE": "comparison",
    "NOT": "boolean",
    "OR": "boolean",
    "OVER": "window",
    "THEN": "conditional",
    "TRUE": "boolean",
    "WHEN": "conditional",
}

_NUMBER = re.compile(r"[0-9]+(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?")
_NAME = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
_OPERATORS = {
    "+": "PLUS",
    "-": "MINUS",
    "*": "STAR",
    "/": "SLASH",
    "(": "LPAREN",
    ")": "RPAREN",
    ",": "COMMA",
}


class NumericError(ValueError):
    """An R010 expression cannot be tokenized, parsed, or closed."""

    def __init__(
        self,
        message: str,
        position: int,
        *,
        condition: str = "invalid_numeric_expression",
        requirement: str = "R010-35",
        context: dict[str, JsonValue] | None = None,
    ) -> None:
        super().__init__(f"{message} at character {position + 1}")
        self.position = position
        self.condition = condition
        self.requirement = requirement
        self.context: dict[str, JsonValue] = dict(context or {})


def _tokenize(text: str) -> list[Token]:
    tokens: list[Token] = []
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
            if end < length and text[end] == ".":
                suffix = _NAME.match(text, end + 1)
                if suffix is None:
                    raise NumericError("invalid qualified identifier", index)
                value += "." + suffix.group(0)
                end = suffix.end()
            construct = PROHIBITED_KEYWORDS.get(value.upper())
            if "." not in value and construct is not None:
                raise _prohibited_construct(construct, index)
            tokens.append(("NAME", value, index, end))
            index = end
            continue

        kind = _OPERATORS.get(character)
        if kind is not None:
            tokens.append((kind, character, index, index + 1))
            index += 1
            continue

        if character in "<>=!":
            raise _prohibited_construct("comparison", index)
        if character == "'":
            raise _prohibited_construct("string", index)
        raise NumericError(f"unexpected character {character!r}", index)

    tokens.append(("EOF", "", length, length))
    return tokens


def _prohibited_construct(construct: str, position: int) -> NumericError:
    return NumericError(
        f"a {construct} construct is not permitted in a numeric expression",
        position,
        condition="prohibited_construct",
        requirement="R010-37",
        context={"construct": construct},
    )


def _prohibited_function(
    name: str,
    position: int,
    message: str,
    context: dict[str, JsonValue],
) -> NumericError:
    return NumericError(
        message,
        position,
        condition="prohibited_function",
        requirement="R010-36",
        context={"function": name, **context},
    )


class _Parser:
    """Recursive-descent parser for R010's four productions."""

    def __init__(self, text: str) -> None:
        self._tokens = _tokenize(text)
        self._index = 0

    @property
    def _token(self) -> Token:
        return self._tokens[self._index]

    def _advance(self) -> Token:
        token = self._token
        self._index += 1
        return token

    def parse(self) -> NumericAst:
        node = self._expression()
        if self._token[0] != "EOF":
            raise NumericError("unexpected trailing token", self._token[2])
        return node

    def _expression(self) -> NumericAst:
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

    def _term(self) -> NumericAst:
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

    def _factor(self) -> NumericAst:
        # R010-8 binds a sign to one primary, so `--A` is outside the grammar.
        if self._token[0] in {"PLUS", "MINUS"}:
            operator = self._advance()
            return {
                "kind": "unary",
                "operator": operator[1],
                "value": self._primary(),
            }
        return self._primary()

    def _primary(self) -> NumericAst:
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
            return self._call(token)
        if token[0] == "LPAREN":
            self._advance()
            node = self._expression()
            if self._token[0] != "RPAREN":
                raise NumericError("expected ')' to close expression", self._token[2])
            self._advance()
            return node
        raise NumericError(
            "expected a number, identifier, function, NULL, or parenthesis",
            token[2],
        )

    def _call(self, name_token: Token) -> NumericAst:
        self._advance()
        arguments: list[NumericAst] = []
        if self._token[0] != "RPAREN":
            arguments.append(self._expression())
            while self._token[0] == "COMMA":
                self._advance()
                arguments.append(self._expression())
        if self._token[0] != "RPAREN":
            raise NumericError("expected ')' to close function", self._token[2])
        self._advance()
        # The vocabulary closes only after the call parses, so an unclosed
        # call reports the syntax defect rather than the function name.
        name = name_token[1]
        arity = FUNCTION_ARITIES.get(name.upper())
        if arity is None:
            raise _prohibited_function(
                name,
                name_token[2],
                f"function {name!r} is not permitted by R010",
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


def parse_numeric(text: str) -> NumericAst:
    """Parse one `compute.expr` under the closed R010 grammar."""
    if not isinstance(text, str) or not text:
        raise NumericError("numeric expression must be a non-empty string", 0)
    return _Parser(text).parse()


@lru_cache(maxsize=512)
def _parse_cached(text: str) -> NumericAst:
    return parse_numeric(text)


def parse_numeric_cached(text: str) -> NumericAst:
    """Parse one expression, reusing the parse of a repeated text.

    Parsing is a pure function of the text, so a row loop re-reading one
    `compute.expr` parses it once. The cached AST is never mutated.
    """
    return _parse_cached(text)


def numeric_identifiers(ast: NumericAst) -> tuple[str, ...]:
    """Return every identifier the expression binds, in first-seen order."""
    names: list[str] = []

    def visit(node: NumericAst) -> None:
        kind = node["kind"]
        if kind == "identifier":
            names.append(node["name"])
        elif kind == "unary":
            visit(node["value"])
        elif kind == "binary":
            visit(node["left"])
            visit(node["right"])
        elif kind == "call":
            for argument in node["arguments"]:
                visit(argument)

    visit(ast)
    return tuple(dict.fromkeys(names))


class _Failure(Exception):
    """Carry one structured condition out of a recursive evaluation."""

    def __init__(self, condition: ConditionResult) -> None:
        self.condition = condition
        super().__init__(condition.condition.condition)


def _fail(
    condition: str,
    requirement: str,
    context: dict[str, JsonValue],
    *,
    phase: str = "derivation",
) -> _Failure:
    return _Failure(
        ConditionResult(
            condition=RuntimeCondition(
                phase=phase,  # type: ignore[arg-type]
                condition=condition,
                context=context,
                requirement=requirement,
            )
        )
    )


def _checked_int(value: int, expr: str) -> int:
    if INT64_MIN <= value <= INT64_MAX:
        return value
    raise _fail(
        "integer_overflow",
        "R010-30",
        {
            "expr": expr,
            "value": str(value),
            "minimum": INT64_MIN,
            "maximum": INT64_MAX,
        },
    )


def _finite(value: float) -> float | object:
    # R010-24 applies R011's non-finite normalization after every operator.
    return value if math.isfinite(value) else MISSING


def _is_missing(value: object) -> bool:
    return value is MISSING


def _promote(values: list[object]) -> str:
    return "int" if all(type(value) is int for value in values) else "float"


def _binary(node: NumericAst, expr: str, resolver: Resolver) -> object:
    left = _evaluate(node["left"], expr, resolver)
    right = _evaluate(node["right"], expr, resolver)
    operator = node["operator"]
    if _is_missing(left) or _is_missing(right):
        return MISSING

    if operator == "/":
        # R010-16: division always returns float and never truncates.
        divisor = float(right)  # type: ignore[arg-type]
        if divisor == 0.0:
            raise _fail("division_by_zero", "R010-26", {"expr": expr})
        return _finite(float(left) / divisor)  # type: ignore[arg-type]

    if type(left) is int and type(right) is int:
        if operator == "+":
            return _checked_int(left + right, expr)
        if operator == "-":
            return _checked_int(left - right, expr)
        return _checked_int(left * right, expr)

    left_float = float(left)  # type: ignore[arg-type]
    right_float = float(right)  # type: ignore[arg-type]
    if operator == "+":
        return _finite(left_float + right_float)
    if operator == "-":
        return _finite(left_float - right_float)
    return _finite(left_float * right_float)


def _modulo(left: object, right: object, expr: str) -> object:
    if float(right) == 0.0:  # type: ignore[arg-type]
        raise _fail("division_by_zero", "R010-26", {"expr": expr})
    if type(left) is int and type(right) is int:
        remainder = abs(left) % abs(right)
        return -remainder if left < 0 else remainder
    return _finite(math.fmod(float(left), float(right)))  # type: ignore[arg-type]


def _power(left: object, right: object, expr: str) -> object:
    base = float(left)  # type: ignore[arg-type]
    exponent = float(right)  # type: ignore[arg-type]
    if base == 0.0 and exponent < 0.0:
        raise _fail(
            "invalid_power",
            "R010-29",
            {"expr": expr, "base": base, "exponent": exponent},
        )
    if base < 0.0 and not exponent.is_integer():
        raise _fail(
            "invalid_power",
            "R010-29",
            {"expr": expr, "base": base, "exponent": exponent},
        )
    try:
        return _finite(math.pow(base, exponent))
    except OverflowError:
        return MISSING


def _numbers_equal(left: object, right: object) -> bool:
    """Compare two numbers without widening an int past binary64 precision."""
    if type(left) is int and type(right) is int:
        return left == right
    return float(left) == float(right)  # type: ignore[arg-type]


def _extreme(arguments: list[object], *, largest: bool) -> object:
    """Return the largest or smallest non-NULL argument, promoted (R010-19)."""
    present = [value for value in arguments if not _is_missing(value)]
    if not present:
        return MISSING
    selected = max(present) if largest else min(present)  # type: ignore[type-var]
    if _promote(present) == "float":
        return _finite(float(selected))  # type: ignore[arg-type]
    return selected


def _call(node: NumericAst, expr: str, resolver: Resolver) -> object:
    name = node["name"]
    arguments = [_evaluate(argument, expr, resolver) for argument in node["arguments"]]

    if name == "COALESCE":
        for value in arguments:
            if not _is_missing(value):
                return value
        return MISSING
    if name in {"GREATEST", "LEAST"}:
        return _extreme(arguments, largest=name == "GREATEST")
    if name == "NULLIF":
        left, right = arguments
        if _is_missing(left):
            return MISSING
        if not _is_missing(right) and _numbers_equal(left, right):
            return MISSING
        if (
            _promote([value for value in arguments if not _is_missing(value)])
            == "float"
        ):
            return _finite(float(left))  # type: ignore[arg-type]
        return left

    # R010-22: every remaining function propagates a missing argument.
    if any(_is_missing(value) for value in arguments):
        return MISSING

    if name == "MOD":
        return _modulo(arguments[0], arguments[1], expr)
    if name == "POWER":
        return _power(arguments[0], arguments[1], expr)

    value = arguments[0]
    if name == "ABS":
        if type(value) is int:
            return _checked_int(abs(value), expr)
        return _finite(abs(float(value)))  # type: ignore[arg-type]
    if name == "CEIL":
        return _finite(float(math.ceil(value)))  # type: ignore[arg-type]
    if name == "FLOOR":
        return _finite(float(math.floor(value)))  # type: ignore[arg-type]
    if name == "TRUNC":
        return _finite(float(math.trunc(value)))  # type: ignore[arg-type]
    if name == "SQRT":
        if float(value) < 0.0:  # type: ignore[arg-type]
            raise _fail("sqrt_of_negative", "R010-27", {"expr": expr})
        return _finite(math.sqrt(float(value)))  # type: ignore[arg-type]
    if name == "EXP":
        try:
            return _finite(math.exp(float(value)))  # type: ignore[arg-type]
        except OverflowError:
            return MISSING
    if name == "LN":
        if float(value) <= 0.0:  # type: ignore[arg-type]
            raise _fail("ln_of_nonpositive", "R010-28", {"expr": expr})
        return _finite(math.log(float(value)))  # type: ignore[arg-type]
    raise AssertionError(f"unhandled R010 function {name!r}")


def _identifier(node: NumericAst, expr: str, resolver: Resolver) -> object:
    name = node["name"]
    resolved = resolver.resolve(name)
    if isinstance(resolved, FailedResolution):
        raise _Failure(ConditionResult(condition=resolved.condition))
    if isinstance(resolved, AbsentValue):
        raise _fail(
            "unknown_field",
            "R010-39",
            {"expr": expr, "identifier": name},
            phase="validation",
        )
    assert isinstance(resolved, ResolvedValue)
    normalized = normalize_runtime_value(resolved.value)
    if isinstance(normalized, ConditionResult):
        raise _Failure(normalized)
    assert isinstance(normalized, ValueResult)
    value = normalized.value
    if value is MISSING:
        return MISSING
    actual = runtime_type_name(value)
    if actual not in {"int", "float"}:
        # R010-21 and R007-19: bind a string to a numeric column first.
        raise _fail(
            "incompatible_input_type",
            "R010-40",
            {
                "expr": expr,
                "source": name,
                "expected": "numeric",
                "actual": actual,
            },
            phase="validation",
        )
    return value


def _evaluate(node: NumericAst, expr: str, resolver: Resolver) -> object:
    kind = node["kind"]
    if kind == "number":
        written = node["value"]
        if node["type"] == "int":
            return _checked_int(int(written, 10), expr)
        return _finite(float(written))
    if kind == "null":
        return MISSING
    if kind == "identifier":
        return _identifier(node, expr, resolver)
    if kind == "unary":
        value = _evaluate(node["value"], expr, resolver)
        if _is_missing(value) or node["operator"] == "+":
            return value
        if type(value) is int:
            return _checked_int(-value, expr)
        return _finite(-float(value))  # type: ignore[arg-type]
    if kind == "binary":
        return _binary(node, expr, resolver)
    if kind == "call":
        return _call(node, expr, resolver)
    raise AssertionError(f"unknown numeric AST node {kind!r}")


def evaluate_numeric(
    ast: NumericAst,
    expr: str,
    resolver: Resolver,
) -> EvaluationResult:
    """Evaluate one parsed R010 formula exactly as it is written."""
    try:
        value = _evaluate(ast, expr, resolver)
    except _Failure as failure:
        return failure.condition
    return ValueResult(value=value)  # type: ignore[arg-type]


def _compute(payload: object, resolver: Resolver) -> EvaluationResult:
    if isinstance(payload, str):
        payload = {"expr": payload}
    if not isinstance(payload, Mapping):
        return expression_condition(
            "validation",
            "invalid_field_type",
            {"operation": "compute", "expected": "a mapping"},
            requirement="R007-36",
        )
    expr = payload.get("expr")
    if not isinstance(expr, str):
        return expression_condition(
            "validation",
            "invalid_field_type",
            {"operation": "compute", "expected": "a numeric expression"},
            requirement="R007-36",
        )
    try:
        ast = parse_numeric_cached(expr)
    except NumericError as error:
        return expression_condition(
            "validation",
            error.condition,
            {"expr": expr, **error.context},
            requirement=error.requirement,
            field="expr",
        )
    return evaluate_numeric(ast, expr, resolver)


def numeric_handlers() -> dict[str, ExpressionHandler]:
    """Return the R010 arithmetic operation this component registers."""
    return {"compute": _compute}
