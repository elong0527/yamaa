"""String expressions: R012 templates, R019 casing, and R022 extraction.

`yaml/grammar/string-template.yaml` is the single source for the template
grammar. Every regular expression is read by the one engine R022 pins, so no
pattern reaches Python `re`, and casing is the exact ASCII substitution R019
defines rather than a host or Unicode case table.
"""

from __future__ import annotations

import re
from collections.abc import Mapping, Sequence
from functools import lru_cache
from typing import Any, TypeAlias

from pydantic import JsonValue

from yamaa.expressions.core import (
    AbsentValue,
    ExpressionHandler,
    FailedResolution,
    NestedDispatcher,
    ResolvedValue,
    Resolver,
    evaluate_nested,
    expression_condition,
    handler_value,
)
from yamaa.expressions.text import ascii_lower, ascii_upper
from yamaa.models import (
    MISSING,
    ConditionResult,
    EvaluationResult,
    HandlerObservation,
    RuntimeValue,
    UnsupportedResult,
    ValueResult,
    normalize_runtime_value,
    runtime_type_name,
)
from yamaa.regex import (
    NO_MATCH,
    RegexError,
    capture_group_count,
    regex_extract,
)

TemplatePart: TypeAlias = dict[str, Any]

_PLACEHOLDER = re.compile(r"[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*")


class TemplateError(ValueError):
    """A string template is outside the closed R012 grammar."""

    condition = "invalid_string_template"
    requirement = "R012-16"

    def __init__(self, reason: str, position: int, placeholder: str | None) -> None:
        super().__init__(f"{reason} at character {position + 1}")
        self.reason = reason
        self.position = position
        self.placeholder = placeholder

    @property
    def context(self) -> dict[str, JsonValue]:
        context: dict[str, JsonValue] = {"reason": self.reason}
        if self.placeholder is not None:
            context["placeholder"] = self.placeholder
        return context


def parse_template(text: str) -> tuple[TemplatePart, ...]:
    """Scan one template left to right under the closed R012 grammar."""
    if not isinstance(text, str):
        raise TemplateError("string template must be a string", 0, None)
    parts: list[TemplatePart] = []
    literal: list[str] = []
    index = 0
    length = len(text)

    def flush() -> None:
        if literal:
            parts.append({"kind": "text", "value": "".join(literal)})
            literal.clear()

    while index < length:
        # R012-8: a brace pair takes precedence over a single brace.
        if text.startswith("{{", index) or text.startswith("}}", index):
            literal.append(text[index])
            index += 2
            continue
        character = text[index]
        if character == "}":
            raise TemplateError("unmatched_brace", index, None)
        if character != "{":
            literal.append(character)
            index += 1
            continue
        end = text.find("}", index + 1)
        if end < 0:
            raise TemplateError("unmatched_brace", index, None)
        name = text[index + 1 : end]
        if "{" in name or _PLACEHOLDER.fullmatch(name) is None:
            raise TemplateError("invalid_placeholder", index, name)
        flush()
        parts.append({"kind": "placeholder", "name": name})
        index = end + 1

    flush()
    return tuple(parts)


@lru_cache(maxsize=512)
def _parse_template_cached(text: str) -> tuple[TemplatePart, ...]:
    return parse_template(text)


def parse_template_cached(text: str) -> tuple[TemplatePart, ...]:
    """Scan one template, reusing the scan of a repeated text."""
    return _parse_template_cached(text)


def template_identifiers(parts: Sequence[TemplatePart]) -> tuple[str, ...]:
    """Return each placeholder once, in first-seen order (R012-11)."""
    return tuple(
        dict.fromkeys(part["name"] for part in parts if part["kind"] == "placeholder")
    )


def _invalid_payload(operation: str, expected: str) -> ConditionResult:
    return expression_condition(
        "validation",
        "invalid_field_type",
        {"operation": operation, "expected": expected},
        requirement="R007-36",
    )


def _resolve_string(
    variable: object,
    resolver: Resolver,
    operation: str,
    field: str = "source",
) -> ValueResult | ConditionResult:
    """Resolve one `variable` input and hold R007-24 to a string."""
    if not isinstance(variable, str):
        return _invalid_payload(operation, "a variable name")
    resolved = resolver.resolve(variable)
    if isinstance(resolved, FailedResolution):
        return ConditionResult(condition=resolved.condition)
    if isinstance(resolved, AbsentValue):
        return expression_condition(
            "validation",
            "unknown_field",
            {"identifier": variable},
            requirement="R002-27",
            field=field,
        )
    assert isinstance(resolved, ResolvedValue)
    normalized = normalize_runtime_value(resolved.value)
    if not isinstance(normalized, ValueResult):
        assert isinstance(normalized, ConditionResult)
        return normalized
    value = normalized.value
    if value is MISSING or isinstance(value, str):
        return ValueResult(value=value)
    return expression_condition(
        "validation",
        "incompatible_input_type",
        {
            "source": variable,
            "expected": "str",
            "actual": runtime_type_name(value),
        },
        requirement="R007-24",
        field=field,
    )


def _missing_input(payload: Mapping[object, object], variable: str) -> EvaluationResult:
    if "missing" in payload:
        return handler_value(payload, "missing")
    return expression_condition(
        "mapping",
        "missing_input",
        {"variable": variable},
        applicable_handler="missing",
        requirement="R007-49",
    )


def _cased(operation: str, transform: object) -> ExpressionHandler:
    def handler(payload: object, resolver: Resolver) -> EvaluationResult:
        if not isinstance(payload, Mapping):
            return _invalid_payload(operation, "a mapping")
        variable = payload.get("source")
        resolved = _resolve_string(variable, resolver, operation)
        if not isinstance(resolved, ValueResult):
            return resolved
        if resolved.value is MISSING:
            return _missing_input(payload, str(variable))
        assert isinstance(resolved.value, str)
        return ValueResult(value=transform(resolved.value))  # type: ignore[operator]

    return handler


def _str_extract(payload: object, resolver: Resolver) -> EvaluationResult:
    if not isinstance(payload, Mapping):
        return _invalid_payload("str_extract", "a mapping")
    variable = payload.get("source")
    pattern = payload.get("pattern")
    group = payload.get("group", 0)
    if not isinstance(pattern, str) or type(group) is not int:
        return _invalid_payload("str_extract", "a pattern and an int group")

    try:
        declared = capture_group_count(pattern)
    except RegexError as error:
        return expression_condition(
            "validation",
            error.condition,
            {"pattern": error.pattern, "reason": error.reason},
            requirement=error.requirement,
            field="pattern",
        )
    if group < 0 or group > declared:
        return expression_condition(
            "validation",
            "regex_group_out_of_range",
            {"group": group, "group_count": declared, "pattern": pattern},
            requirement="R022-28",
            field="group",
        )

    resolved = _resolve_string(variable, resolver, "str_extract")
    if not isinstance(resolved, ValueResult):
        return resolved
    if resolved.value is MISSING:
        return _missing_input(payload, str(variable))
    subject = resolved.value
    assert isinstance(subject, str)

    extracted = regex_extract(pattern, subject, group)
    if extracted is NO_MATCH:
        if "no_match" in payload:
            return handler_value(payload, "no_match")
        # R008-6: the subject is present but the pattern reached nothing.
        return expression_condition(
            "mapping",
            "unmatched_pattern",
            {"value": subject, "pattern": pattern},
            applicable_handler="no_match",
            requirement="R007-49",
        )
    if extracted is None:
        # R022-22: the match did not enter a declared group, so the result is
        # missing and `no_match` deliberately does not apply.
        return ValueResult(value=MISSING)
    assert isinstance(extracted, str)
    return ValueResult(value=extracted)


def _template(payload: object, resolver: Resolver) -> EvaluationResult:
    if isinstance(payload, str):
        payload = {"template": payload}
    if not isinstance(payload, Mapping):
        return _invalid_payload("str_template", "a template or a mapping")
    template = payload.get("template")
    if not isinstance(template, str):
        return _invalid_payload("str_template", "a template string")
    try:
        parts = parse_template_cached(template)
    except TemplateError as error:
        return expression_condition(
            "validation",
            error.condition,
            error.context,
            requirement=error.requirement,
        )

    rendered: list[str] = []
    for part in parts:
        if part["kind"] == "text":
            rendered.append(part["value"])
            continue
        name = part["name"]
        resolved = _resolve_string(name, resolver, "str_template", "template")
        if not isinstance(resolved, ValueResult):
            return resolved
        if resolved.value is MISSING:
            # R012-14: one missing placeholder answers the whole template.
            return _missing_input(payload, name)
        assert isinstance(resolved.value, str)
        rendered.append(resolved.value)
    return ValueResult(value="".join(rendered))


def _concat(dispatcher: NestedDispatcher) -> ExpressionHandler:
    def handler(payload: object, resolver: Resolver) -> EvaluationResult:
        if not isinstance(payload, Mapping):
            return _invalid_payload("str_concat", "a mapping")
        sources = payload.get("sources")
        if not isinstance(sources, Sequence) or isinstance(sources, (str, bytes)):
            return _invalid_payload("str_concat", "a list of expressions")

        rendered: list[str] = []
        observations: list[HandlerObservation] = []
        for index, source in enumerate(sources):
            result, nested = evaluate_nested(
                dispatcher, source, resolver, f"sources[{index}]"
            )
            if isinstance(result, (ConditionResult, UnsupportedResult)):
                return result
            observations.extend(nested)
            value: RuntimeValue = result.value
            if value is MISSING:
                missing = _missing_input(payload, f"sources[{index}]")
                if isinstance(missing, ValueResult):
                    return missing.model_copy(
                        update={"observations": tuple(observations)}
                    )
                return missing
            if not isinstance(value, str):
                return expression_condition(
                    "validation",
                    "incompatible_input_type",
                    {
                        "source": f"sources[{index}]",
                        "expected": "str",
                        "actual": runtime_type_name(value),
                    },
                    requirement="R007-24",
                    field=f"sources[{index}]",
                )
            rendered.append(value)
        return ValueResult(
            value="".join(rendered),
            observations=tuple(observations),
        )

    return handler


def string_handlers(dispatcher: NestedDispatcher) -> dict[str, ExpressionHandler]:
    """Return the R007 string operations this component registers."""
    return {
        "str_extract": _str_extract,
        "str_concat": _concat(dispatcher),
        "str_template": _template,
        "str_upper": _cased("str_upper", ascii_upper),
        "str_lower": _cased("str_lower", ascii_lower),
    }
