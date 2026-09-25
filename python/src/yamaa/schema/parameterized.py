"""Expand `for:` parameterized intermediates and row templates (REQ-1262).

A definition carrying ``for:`` is instantiated once per listed value before
validation, so the engines only ever see the expanded form. ``{name}``
placeholders in the definition's scalar strings are replaced per instance:
a string that is exactly ``{name}`` takes the bound value with its type
preserved, any other occurrence is replaced with the value's textual form.
"""

from __future__ import annotations

import copy
import re
from typing import Any

from yamaa.specification.diagnostics import SpecificationError, ValidationDiagnostic

_REQUIREMENT = "REQ-1262"

_PLACEHOLDER = re.compile(r"\{([A-Za-z_][A-Za-z0-9_]*)\}")

# Root list keys holding parameterizable definitions, with their identity field.
_TARGETS = (
    ("intermediates", "id"),
    ("rows", "id"),
)


def _is_scalar(value: Any) -> bool:
    """Whether a `for:` item binds the single variable `v`."""
    return isinstance(value, (str, int, float, bool))


def _textual(value: Any) -> str:
    """Render a bound value for substitution inside a larger string."""
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return str(int(value)) if value.is_integer() else repr(value)
    return str(value)


def _environment(
    item: Any, path: str, diagnostics: list[ValidationDiagnostic]
) -> dict[str, Any]:
    """Build the placeholder bindings for one `for:` list item."""
    if isinstance(item, dict):
        return {key: item[key] for key in item}
    if _is_scalar(item):
        return {"v": item}
    diagnostics.append(
        ValidationDiagnostic(
            condition="invalid_for_item",
            spec_paths=(path,),
            requirement=_REQUIREMENT,
            context={"item": str(item)},
        )
    )
    return {}


# Sentinels protecting str_template `{{` / `}}` escapes during substitution.
_ESCAPED_OPEN = "\ue000"
_ESCAPED_CLOSE = "\ue001"


def _substitute_string(
    value: str,
    environment: dict[str, Any],
    path: str,
    diagnostics: list[ValidationDiagnostic],
) -> Any:
    # `{{` and `}}` are literal braces under the str_template grammar, not
    # placeholders: shield them so substitution leaves them untouched.
    protected = value.replace("{{", _ESCAPED_OPEN).replace("}}", _ESCAPED_CLOSE)
    names = _PLACEHOLDER.findall(protected)
    if not names:
        return value
    unknown = sorted({name for name in names if name not in environment})
    if unknown:
        diagnostics.append(
            ValidationDiagnostic(
                condition="unknown_for_variable",
                spec_paths=(path,),
                requirement=_REQUIREMENT,
                context={"variables": unknown},
            )
        )
        return value
    if len(names) == 1 and protected == f"{{{names[0]}}}":
        # A lone placeholder takes the bound value with its type preserved.
        return environment[names[0]]
    substituted = _PLACEHOLDER.sub(
        lambda match: _textual(environment[match.group(1)]), protected
    )
    return substituted.replace(_ESCAPED_OPEN, "{{").replace(_ESCAPED_CLOSE, "}}")


def _substitute(
    value: Any,
    environment: dict[str, Any],
    path: str,
    diagnostics: list[ValidationDiagnostic],
) -> Any:
    if isinstance(value, dict):
        return {
            _substitute(key, environment, path, diagnostics)
            if isinstance(key, str)
            else key: _substitute(item, environment, f"{path}.{key}", diagnostics)
            for key, item in value.items()
        }
    if isinstance(value, list):
        return [
            _substitute(item, environment, f"{path}[{index}]", diagnostics)
            for index, item in enumerate(value)
        ]
    if isinstance(value, str):
        return _substitute_string(value, environment, path, diagnostics)
    return value


def _template_placeholders(text: str) -> set[str]:
    """Placeholder names in a `str_template` template string.

    `{{` and `}}` are literal braces under the str_template grammar, not
    placeholders, so they are removed before scanning.
    """
    unescaped = text.replace("{{", "").replace("}}", "")
    return set(_PLACEHOLDER.findall(unescaped))


def _str_template_placeholders(node: object) -> set[str]:
    """Collect every `str_template` placeholder name in a definition."""
    found: set[str] = set()
    if isinstance(node, dict):
        for key, value in node.items():
            if key == "str_template":
                if isinstance(value, str):
                    found |= _template_placeholders(value)
                elif isinstance(value, dict):
                    template = value.get("template")
                    if isinstance(template, str):
                        found |= _template_placeholders(template)
            else:
                found |= _str_template_placeholders(value)
    elif isinstance(node, list):
        for item in node:
            found |= _str_template_placeholders(item)
    return found


def expand_parameterized_definitions(
    document: dict[str, object],
) -> dict[str, object]:
    """Instantiate every definition declaring `for:` once per listed value.

    The definition keeps its position; instances are validated exactly as if
    written out. Raises SpecificationError on an empty value list, an invalid
    item, a placeholder with no bound variable, a `for` variable colliding
    with a `str_template` placeholder in the same definition, a placeholder
    id on a definition without `for:`, or duplicate ids among the expanded
    instances of one list.
    """
    result = copy.deepcopy(document)
    diagnostics: list[ValidationDiagnostic] = []
    for list_key, identity in _TARGETS:
        items = result.get(list_key)
        if not isinstance(items, list):
            continue
        expanded: list[object] = []
        from_expansion: set[int] = set()
        for index, item in enumerate(items):
            template_id = item.get(identity) if isinstance(item, dict) else None
            base_path = (
                f"{list_key}.{template_id}"
                if isinstance(template_id, str)
                else f"{list_key}[{index}]"
            )
            if not isinstance(item, dict) or "for" not in item:
                if isinstance(template_id, str) and _PLACEHOLDER.search(template_id):
                    diagnostics.append(
                        ValidationDiagnostic(
                            condition="placeholder_without_for",
                            spec_paths=(f"{base_path}.id",),
                            requirement=_REQUIREMENT,
                            context={"id": template_id},
                        )
                    )
                else:
                    expanded.append(item)
                continue
            values = item["for"]
            if not isinstance(values, list) or not values:
                diagnostics.append(
                    ValidationDiagnostic(
                        condition="empty_for_list",
                        spec_paths=(f"{base_path}.for",),
                        requirement=_REQUIREMENT,
                        context={},
                    )
                )
                continue
            template = {key: item[key] for key in item if key != "for"}
            bound_names: set[str] = set()
            for for_item in values:
                if isinstance(for_item, dict):
                    bound_names.update(
                        name for name in for_item if isinstance(name, str)
                    )
                elif _is_scalar(for_item):
                    bound_names.add("v")
            collisions = _str_template_placeholders(template) & bound_names
            if collisions:
                diagnostics.append(
                    ValidationDiagnostic(
                        condition="for_str_template_collision",
                        spec_paths=(base_path,),
                        requirement=_REQUIREMENT,
                        context={"variables": sorted(collisions)},
                    )
                )
                continue
            for position, for_item in enumerate(values):
                environment = _environment(
                    for_item, f"{base_path}.for[{position}]", diagnostics
                )
                instance = _substitute(template, environment, base_path, diagnostics)
                from_expansion.add(len(expanded))
                expanded.append(instance)
        seen: dict[str, int] = {}
        for index, item in enumerate(expanded):
            instance_id = item.get(identity) if isinstance(item, dict) else None
            if not isinstance(instance_id, str):
                continue
            first = seen.get(instance_id)
            if first is None:
                seen[instance_id] = index
            elif index in from_expansion or first in from_expansion:
                # Only duplicates an expansion produced are reported here;
                # literal duplicates keep their existing diagnostics.
                diagnostics.append(
                    ValidationDiagnostic(
                        condition="duplicate_identifier",
                        spec_paths=(f"{list_key}[{index}].{identity}",),
                        requirement=_REQUIREMENT,
                        context={"identifier": instance_id},
                    )
                )
        result[list_key] = expanded
    if diagnostics:
        raise SpecificationError(diagnostics)
    return result
