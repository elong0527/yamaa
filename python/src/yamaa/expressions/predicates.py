"""Parser and evaluator for the closed R004 predicate language."""

from __future__ import annotations

import re
from collections.abc import Mapping
from enum import Enum
from functools import lru_cache
from typing import Any, Literal, TypeAlias

from pydantic import BaseModel, ConfigDict, JsonValue

from yamaa.expressions.core import (
    AbsentValue,
    FailedResolution,
    ResolvedValue,
    Resolver,
)
from yamaa.models.values import (
    MISSING,
    ConditionResult,
    DateTimeValue,
    DateValue,
    RuntimeCondition,
    RuntimeValue,
    ValueResult,
    normalize_runtime_value,
    runtime_type_name,
    values_comparable,
)

PredicateAst: TypeAlias = dict[str, Any]
Token: TypeAlias = tuple[str, str, int]

RESERVED_NAMES = frozenset(
    {
        "AND",
        "BETWEEN",
        "DATE",
        "DATETIME",
        "ESCAPE",
        "FALSE",
        "IN",
        "IS",
        "LIKE",
        "NOT",
        "NULL",
        "OR",
        "TRUE",
    }
)
COMPARISON_OPERATORS = ("=", "<>", "<", "<=", ">", ">=")
TWO_CHARACTER_OPERATORS = frozenset(
    operator for operator in COMPARISON_OPERATORS if len(operator) == 2
)
ONE_CHARACTER_OPERATORS = frozenset(
    operator for operator in COMPARISON_OPERATORS if len(operator) == 1
)


class PredicateError(ValueError):
    """A portable predicate cannot be tokenized or parsed."""

    condition = "invalid_predicate"

    def __init__(self, message: str, position: int) -> None:
        super().__init__(f"{message} at character {position + 1}")
        self.position = position


def _tokenize(text: str) -> list[Token]:
    tokens: list[Token] = []
    index = 0
    while index < len(text):
        character = text[index]
        if character.isspace():
            index += 1
            continue

        if character == "'":
            start = index
            index += 1
            value: list[str] = []
            while index < len(text):
                if text[index] != "'":
                    value.append(text[index])
                    index += 1
                    continue
                if index + 1 < len(text) and text[index + 1] == "'":
                    value.append("'")
                    index += 2
                    continue
                index += 1
                tokens.append(("STRING", "".join(value), start))
                break
            else:
                raise PredicateError("unterminated string literal", start)
            continue

        number = re.match(
            r"[+-]?[0-9]+(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?",
            text[index:],
        )
        if number is not None:
            value = number.group(0)
            tokens.append(("NUMBER", value, index))
            index += len(value)
            continue

        name = re.match(r"[A-Za-z_][A-Za-z0-9_]*", text[index:])
        if name is not None:
            value = name.group(0)
            end = index + len(value)
            if end < len(text) and text[end] == ".":
                suffix = re.match(r"[A-Za-z_][A-Za-z0-9_]*", text[end + 1 :])
                if suffix is None:
                    raise PredicateError("invalid qualified identifier", index)
                value += f".{suffix.group(0)}"
                end += 1 + len(suffix.group(0))
            tokens.append(("NAME", value, index))
            index = end
            continue

        two_character = text[index : index + 2]
        if two_character in TWO_CHARACTER_OPERATORS:
            tokens.append(("OP", two_character, index))
            index += 2
            continue
        if character in ONE_CHARACTER_OPERATORS:
            tokens.append(("OP", character, index))
            index += 1
            continue
        punctuation = {"(": "LPAREN", ")": "RPAREN", ",": "COMMA"}
        if character in punctuation:
            tokens.append((punctuation[character], character, index))
            index += 1
            continue
        raise PredicateError(f"unexpected character {character!r}", index)

    tokens.append(("EOF", "", len(text)))
    return tokens


def _dangling_escape(pattern: str, escape: str) -> bool:
    escaped = False
    for character in pattern:
        if escaped:
            escaped = False
        elif character == escape:
            escaped = True
    return escaped


class _PredicateParser:
    def __init__(self, text: str) -> None:
        self.tokens = _tokenize(text)
        self.index = 0

    @property
    def token(self) -> Token:
        return self.tokens[self.index]

    def advance(self) -> Token:
        token = self.token
        self.index += 1
        return token

    def keyword(self, value: str) -> bool:
        return self.token[0] == "NAME" and self.token[1].upper() == value

    def take_keyword(self, value: str) -> Token | None:
        return self.advance() if self.keyword(value) else None

    def require(self, kind: str, message: str) -> Token:
        if self.token[0] != kind:
            raise PredicateError(message, self.token[2])
        return self.advance()

    def parse(self) -> PredicateAst:
        node = self.parse_disjunction()
        if self.token[0] != "EOF":
            raise PredicateError("unexpected trailing token", self.token[2])
        return node

    def parse_disjunction(self) -> PredicateAst:
        node = self.parse_conjunction()
        while self.take_keyword("OR") is not None:
            node = {
                "kind": "or",
                "left": node,
                "right": self.parse_conjunction(),
            }
        return node

    def parse_conjunction(self) -> PredicateAst:
        node = self.parse_negation()
        while self.take_keyword("AND") is not None:
            node = {
                "kind": "and",
                "left": node,
                "right": self.parse_negation(),
            }
        return node

    def parse_negation(self) -> PredicateAst:
        if self.take_keyword("NOT") is not None:
            return {"kind": "not", "value": self.parse_negation()}
        return self.parse_boolean()

    def parse_boolean(self) -> PredicateAst:
        if self.token[0] == "LPAREN":
            self.advance()
            node = self.parse_disjunction()
            self.require("RPAREN", "expected ')' to close predicate")
            return node
        if self.take_keyword("TRUE") is not None:
            return {"kind": "boolean", "value": True}
        if self.take_keyword("FALSE") is not None:
            return {"kind": "boolean", "value": False}

        left = self.parse_operand()
        if self.token[0] == "OP":
            return {
                "kind": "comparison",
                "operator": self.advance()[1],
                "left": left,
                "right": self.parse_operand(),
            }
        if self.take_keyword("IS") is not None:
            negated = self.take_keyword("NOT") is not None
            if self.take_keyword("NULL") is None:
                raise PredicateError("expected NULL after IS", self.token[2])
            return {"kind": "null_test", "value": left, "negated": negated}

        negated = self.take_keyword("NOT") is not None
        if self.take_keyword("IN") is not None:
            self.require("LPAREN", "expected '(' after IN")
            values = [self.parse_operand()]
            while self.token[0] == "COMMA":
                self.advance()
                values.append(self.parse_operand())
            self.require("RPAREN", "expected ')' after IN operands")
            return {
                "kind": "in",
                "value": left,
                "values": values,
                "negated": negated,
            }
        if self.take_keyword("BETWEEN") is not None:
            lower = self.parse_operand()
            if self.take_keyword("AND") is None:
                raise PredicateError(
                    "expected AND in BETWEEN predicate",
                    self.token[2],
                )
            return {
                "kind": "between",
                "value": left,
                "lower": lower,
                "upper": self.parse_operand(),
                "negated": negated,
            }
        if self.take_keyword("LIKE") is not None:
            pattern = self.parse_operand()
            escape = None
            if self.take_keyword("ESCAPE") is not None:
                token = self.require("STRING", "ESCAPE requires a string literal")
                if len(token[1]) != 1:
                    raise PredicateError(
                        "ESCAPE requires exactly one code point",
                        token[2],
                    )
                escape = token[1]
            if (
                escape is not None
                and pattern.get("kind") == "literal"
                and pattern.get("type") == "str"
                and _dangling_escape(pattern.get("value", ""), escape)
            ):
                raise PredicateError(
                    "LIKE pattern has a dangling escape",
                    pattern["position"],
                )
            return {
                "kind": "like",
                "value": left,
                "pattern": pattern,
                "escape": escape,
                "negated": negated,
            }
        if negated:
            raise PredicateError(
                "NOT must precede IN, BETWEEN, or LIKE",
                self.token[2],
            )
        raise PredicateError(
            "operand must be followed by a Boolean operator",
            self.token[2],
        )

    def parse_operand(self) -> PredicateAst:
        token = self.token
        if token[0] == "NUMBER":
            self.advance()
            value_type = (
                "float" if "." in token[1] or "e" in token[1].lower() else "int"
            )
            return {
                "kind": "literal",
                "type": value_type,
                "value": token[1],
                "position": token[2],
            }
        if token[0] == "STRING":
            self.advance()
            return {
                "kind": "literal",
                "type": "str",
                "value": token[1],
                "position": token[2],
            }
        if self.take_keyword("NULL") is not None:
            return {
                "kind": "literal",
                "type": None,
                "value": None,
                "position": token[2],
            }
        if self.keyword("DATE") or self.keyword("DATETIME"):
            value_type = self.advance()[1].lower()
            text = self.require(
                "STRING",
                f"{value_type.upper()} requires a string literal",
            )
            try:
                if value_type == "date":
                    DateValue.parse(text[1])
                else:
                    DateTimeValue.parse(text[1])
            except ValueError as error:
                raise PredicateError(
                    f"invalid {value_type} literal",
                    text[2],
                ) from error
            return {
                "kind": "literal",
                "type": value_type,
                "value": text[1],
                "position": token[2],
            }
        if token[0] == "NAME":
            if token[1].upper() in RESERVED_NAMES:
                raise PredicateError("expected operand", token[2])
            self.advance()
            return {
                "kind": "identifier",
                "name": token[1],
                "position": token[2],
            }
        raise PredicateError("expected operand", token[2])


def parse_predicate(text: str) -> PredicateAst:
    """Parse one R004 predicate into the validator-compatible AST shape."""
    if not isinstance(text, str) or not text:
        raise PredicateError("predicate must be a non-empty string", 0)
    return _PredicateParser(text).parse()


@lru_cache(maxsize=512)
def _parse_cached(text: str) -> PredicateAst:
    return parse_predicate(text)


def parse_predicate_cached(text: str) -> PredicateAst:
    """Parse one predicate, reusing the parse of a repeated text.

    A `case` branch re-reads its own `when` for every row, and parsing is a
    pure function of the text, so the parse is shared. The returned AST is
    never mutated.
    """
    return _parse_cached(text)


class TruthValue(Enum):
    TRUE = 1
    FALSE = 0
    UNKNOWN = -1


class PredicateValue(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)

    status: Literal["value"] = "value"
    value: TruthValue


PredicateResult: TypeAlias = PredicateValue | ConditionResult
OperandResult: TypeAlias = ValueResult | ConditionResult


def _condition(
    condition: str,
    context: dict[str, JsonValue],
    requirement: str,
) -> ConditionResult:
    return ConditionResult(
        condition=RuntimeCondition(
            phase="validation",
            condition=condition,
            context=context,
            requirement=requirement,
        )
    )


def _operand(node: PredicateAst, resolver: Resolver) -> OperandResult:
    if node["kind"] == "literal":
        value_type = node["type"]
        if value_type is None:
            return ValueResult(value=MISSING)
        if value_type == "str":
            return normalize_runtime_value(node["value"])
        if value_type == "int":
            return normalize_runtime_value(int(node["value"], 10))
        if value_type == "float":
            return normalize_runtime_value(float(node["value"]))
        try:
            temporal: RuntimeValue
            if value_type == "date":
                temporal = DateValue.parse(node["value"])
            elif value_type == "datetime":
                temporal = DateTimeValue.parse(node["value"])
            else:
                return _condition(
                    "invalid_predicate",
                    {"position": node.get("position", 0)},
                    "R004-31",
                )
            return ValueResult(value=temporal)
        except ValueError:
            return _condition(
                "invalid_predicate",
                {"position": node.get("position", 0)},
                "R016-60",
            )

    resolved = resolver.resolve(node["name"])
    if isinstance(resolved, FailedResolution):
        return ConditionResult(condition=resolved.condition)
    if isinstance(resolved, AbsentValue):
        return _condition(
            "unknown_field",
            {"identifier": node["name"]},
            "R004-32",
        )
    assert isinstance(resolved, ResolvedValue)
    return normalize_runtime_value(resolved.value)


def _truth(value: bool) -> PredicateValue:
    return PredicateValue(value=TruthValue.TRUE if value else TruthValue.FALSE)


def _not(value: TruthValue) -> TruthValue:
    if value is TruthValue.UNKNOWN:
        return value
    return TruthValue.FALSE if value is TruthValue.TRUE else TruthValue.TRUE


def _and(left: TruthValue, right: TruthValue) -> TruthValue:
    if left is TruthValue.FALSE or right is TruthValue.FALSE:
        return TruthValue.FALSE
    if left is TruthValue.UNKNOWN or right is TruthValue.UNKNOWN:
        return TruthValue.UNKNOWN
    return TruthValue.TRUE


def _or(left: TruthValue, right: TruthValue) -> TruthValue:
    if left is TruthValue.TRUE or right is TruthValue.TRUE:
        return TruthValue.TRUE
    if left is TruthValue.UNKNOWN or right is TruthValue.UNKNOWN:
        return TruthValue.UNKNOWN
    return TruthValue.FALSE


def _ordered(value: RuntimeValue) -> object:
    if isinstance(value, (DateValue, DateTimeValue)):
        return value.ordering_key
    return value


def _comparison(
    operator: str,
    left: RuntimeValue,
    right: RuntimeValue,
) -> PredicateResult:
    if left is MISSING or right is MISSING:
        return PredicateValue(value=TruthValue.UNKNOWN)
    if not values_comparable(left, right):
        return _condition(
            "incompatible_input_type",
            {
                "left_type": runtime_type_name(left),
                "right_type": runtime_type_name(right),
            },
            "R004-33",
        )
    comparable_left = _ordered(left)
    comparable_right = _ordered(right)
    if {runtime_type_name(left), runtime_type_name(right)} == {"int", "float"}:
        comparable_left = float(left)  # type: ignore[arg-type]
        comparable_right = float(right)  # type: ignore[arg-type]
    operations = {
        "=": lambda: comparable_left == comparable_right,
        "<>": lambda: comparable_left != comparable_right,
        "<": lambda: comparable_left < comparable_right,  # type: ignore[operator]
        "<=": lambda: comparable_left <= comparable_right,  # type: ignore[operator]
        ">": lambda: comparable_left > comparable_right,  # type: ignore[operator]
        ">=": lambda: comparable_left >= comparable_right,  # type: ignore[operator]
    }
    return _truth(operations[operator]())


def _operand_pair(
    left_node: PredicateAst,
    right_node: PredicateAst,
    resolver: Resolver,
) -> tuple[RuntimeValue, RuntimeValue] | ConditionResult:
    left = _operand(left_node, resolver)
    if isinstance(left, ConditionResult):
        return left
    right = _operand(right_node, resolver)
    if isinstance(right, ConditionResult):
        return right
    return left.value, right.value


def _like_pattern(pattern: str, escape: str | None) -> str | None:
    pieces: list[str] = []
    escaped = False
    for character in pattern:
        if escaped:
            pieces.append(re.escape(character))
            escaped = False
        elif escape is not None and character == escape:
            escaped = True
        elif character == "%":
            pieces.append(".*")
        elif character == "_":
            pieces.append(".")
        else:
            pieces.append(re.escape(character))
    return None if escaped else "".join(pieces)


def _evaluate(node: Mapping[str, Any], resolver: Resolver) -> PredicateResult:
    kind = node["kind"]
    if kind == "boolean":
        return _truth(node["value"])
    if kind == "not":
        result = _evaluate(node["value"], resolver)
        if isinstance(result, ConditionResult):
            return result
        return PredicateValue(value=_not(result.value))
    if kind in {"and", "or"}:
        left = _evaluate(node["left"], resolver)
        if isinstance(left, ConditionResult):
            return left
        right = _evaluate(node["right"], resolver)
        if isinstance(right, ConditionResult):
            return right
        operation = _and if kind == "and" else _or
        return PredicateValue(value=operation(left.value, right.value))
    if kind == "comparison":
        operands = _operand_pair(node["left"], node["right"], resolver)
        if isinstance(operands, ConditionResult):
            return operands
        return _comparison(node["operator"], *operands)
    if kind == "null_test":
        operand = _operand(node["value"], resolver)
        if isinstance(operand, ConditionResult):
            return operand
        result = operand.value is MISSING
        return _truth(not result if node["negated"] else result)
    if kind == "in":
        source = _operand(node["value"], resolver)
        if isinstance(source, ConditionResult):
            return source
        truth = TruthValue.FALSE
        for value_node in node["values"]:
            value = _operand(value_node, resolver)
            if isinstance(value, ConditionResult):
                return value
            compared = _comparison("=", source.value, value.value)
            if isinstance(compared, ConditionResult):
                return compared
            truth = _or(truth, compared.value)
        if node["negated"]:
            truth = _not(truth)
        return PredicateValue(value=truth)
    if kind == "between":
        lower = _operand_pair(node["value"], node["lower"], resolver)
        if isinstance(lower, ConditionResult):
            return lower
        upper = _operand_pair(node["value"], node["upper"], resolver)
        if isinstance(upper, ConditionResult):
            return upper
        first = _comparison(">=", *lower)
        second = _comparison("<=", *upper)
        if isinstance(first, ConditionResult):
            return first
        if isinstance(second, ConditionResult):
            return second
        truth = _and(first.value, second.value)
        if node["negated"]:
            truth = _not(truth)
        return PredicateValue(value=truth)
    if kind == "like":
        operands = _operand_pair(node["value"], node["pattern"], resolver)
        if isinstance(operands, ConditionResult):
            return operands
        value, pattern = operands
        if value is MISSING or pattern is MISSING:
            truth = TruthValue.UNKNOWN
        elif not isinstance(value, str) or not isinstance(pattern, str):
            return _condition(
                "incompatible_input_type",
                {
                    "expected": "str",
                    "actual": runtime_type_name(
                        value if not isinstance(value, str) else pattern
                    ),
                },
                "R004-33",
            )
        else:
            compiled = _like_pattern(pattern, node.get("escape"))
            if compiled is None:
                return _condition(
                    "invalid_predicate",
                    {"reason": "LIKE pattern has a dangling escape"},
                    "R004-34",
                )
            truth = (
                TruthValue.TRUE
                if re.fullmatch(compiled, value, flags=re.DOTALL) is not None
                else TruthValue.FALSE
            )
        if node["negated"]:
            truth = _not(truth)
        return PredicateValue(value=truth)
    return _condition("invalid_predicate", {"kind": str(kind)}, "R004-31")


def evaluate_predicate(ast: PredicateAst, resolver: Resolver) -> PredicateResult:
    """Evaluate a parsed predicate with R004 three-valued logic."""
    return _evaluate(ast, resolver)
