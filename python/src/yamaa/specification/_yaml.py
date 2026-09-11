"""Small YAML 1.2 core reader for authored YAMAA documents."""

from __future__ import annotations

import copy
import math
import re
from pathlib import Path
from typing import Any

import yaml
from yaml.composer import ComposerError
from yaml.constructor import ConstructorError
from yaml.events import AliasEvent

from yamaa.specification.diagnostics import (
    SpecificationError,
    ValidationDiagnostic,
)


class _Yaml12Loader(yaml.SafeLoader):
    yaml_implicit_resolvers = copy.deepcopy(yaml.SafeLoader.yaml_implicit_resolvers)

    def compose_node(self, parent: Any, index: Any) -> yaml.Node:
        event = self.peek_event()
        if isinstance(event, AliasEvent):
            raise ComposerError(
                None,
                None,
                "YAML aliases are not allowed",
                event.start_mark,
            )
        if getattr(event, "anchor", None) is not None:
            raise ComposerError(
                None,
                None,
                "YAML anchors are not allowed",
                event.start_mark,
            )
        if getattr(event, "tag", None) is not None:
            raise ComposerError(
                None,
                None,
                "explicit YAML tags are not allowed",
                event.start_mark,
            )
        return super().compose_node(parent, index)

    def flatten_mapping(self, node: yaml.MappingNode) -> None:
        for key_node, _ in node.value:
            if key_node.tag == "tag:yaml.org,2002:merge":
                raise ConstructorError(
                    None,
                    None,
                    "YAML merge keys are not allowed",
                    key_node.start_mark,
                )
        super().flatten_mapping(node)

    def construct_mapping(
        self,
        node: yaml.MappingNode,
        deep: bool = False,
    ) -> dict[object, object]:
        mapping: dict[object, object] = {}
        for key_node, value_node in node.value:
            key = self.construct_object(key_node, deep=deep)
            try:
                hash(key)
            except TypeError as error:
                raise ConstructorError(
                    "while constructing a mapping",
                    node.start_mark,
                    "found unhashable key",
                    key_node.start_mark,
                ) from error
            if key in mapping:
                raise ConstructorError(
                    "while constructing a mapping",
                    node.start_mark,
                    f"found duplicate key {key!r}",
                    key_node.start_mark,
                )
            mapping[key] = self.construct_object(value_node, deep=deep)
        return mapping


for first_character, resolvers in list(_Yaml12Loader.yaml_implicit_resolvers.items()):
    _Yaml12Loader.yaml_implicit_resolvers[first_character] = [
        (tag, pattern)
        for tag, pattern in resolvers
        if tag
        not in {
            "tag:yaml.org,2002:bool",
            "tag:yaml.org,2002:float",
            "tag:yaml.org,2002:int",
            "tag:yaml.org,2002:timestamp",
        }
    ]

_Yaml12Loader.add_implicit_resolver(
    "tag:yaml.org,2002:bool",
    re.compile(r"^(?:true|True|TRUE|false|False|FALSE)$"),
    list("tTfF"),
)
_Yaml12Loader.add_implicit_resolver(
    "tag:yaml.org,2002:int",
    re.compile(r"^(?:[-+]?[0-9]+|0o[0-7]+|0x[0-9a-fA-F]+)$"),
    list("-+0123456789"),
)
_Yaml12Loader.add_implicit_resolver(
    "tag:yaml.org,2002:float",
    re.compile(
        r"^(?:"
        r"[-+]?(?:\.[0-9]+|[0-9]+(?:\.[0-9]*)?)(?:[eE][-+]?[0-9]+)?"
        r"|[-+]?\.(?:inf|Inf|INF)"
        r"|\.(?:nan|NaN|NAN)"
        r")$"
    ),
    list("-+0123456789."),
)


def _construct_int(loader: _Yaml12Loader, node: yaml.Node) -> int:
    value = loader.construct_scalar(node)
    sign = -1 if value.startswith("-") else 1
    unsigned = value[1:] if value.startswith(("-", "+")) else value
    if unsigned.startswith("0o"):
        return int(value, 0)
    if unsigned.startswith("0x"):
        return int(value, 0)
    return sign * int(unsigned, 10)


def _construct_float(loader: _Yaml12Loader, node: yaml.Node) -> float | None:
    value = yaml.SafeLoader.construct_yaml_float(loader, node)
    return value if math.isfinite(value) else None


_Yaml12Loader.add_constructor("tag:yaml.org,2002:int", _construct_int)
_Yaml12Loader.add_constructor("tag:yaml.org,2002:float", _construct_float)


def _yaml_error(error: yaml.YAMLError) -> SpecificationError:
    mark = getattr(error, "problem_mark", None)
    context: dict[str, str | int] = {"reason": str(error).splitlines()[0]}
    if mark is not None:
        context.update(line=mark.line + 1, column=mark.column + 1)
    return SpecificationError(
        [
            ValidationDiagnostic(
                condition="invalid_yaml",
                spec_paths=("$",),
                context=context,
            )
        ]
    )


def _escaped_path_member(member: object) -> str:
    return str(member).encode("unicode_escape").decode("ascii")


def _unicode_scalar_diagnostics(
    value: object,
    path: str = "$",
) -> list[ValidationDiagnostic]:
    if isinstance(value, str):
        for offset, character in enumerate(value):
            code_point = ord(character)
            if 0xD800 <= code_point <= 0xDFFF:
                return [
                    ValidationDiagnostic(
                        condition="invalid_text",
                        spec_paths=(path,),
                        context={
                            "code_point": f"U+{code_point:04X}",
                            "offset": offset,
                        },
                    )
                ]
        return []
    if isinstance(value, list):
        diagnostics: list[ValidationDiagnostic] = []
        for index, item in enumerate(value):
            diagnostics.extend(_unicode_scalar_diagnostics(item, f"{path}[{index}]"))
        return diagnostics
    if isinstance(value, dict):
        diagnostics = []
        for key, item in value.items():
            diagnostics.extend(_unicode_scalar_diagnostics(key, f"{path}.<key>"))
            diagnostics.extend(
                _unicode_scalar_diagnostics(
                    item,
                    f"{path}.{_escaped_path_member(key)}",
                )
            )
        return diagnostics
    return []


def read_yaml_document(path: str | Path) -> object:
    """Read one ASCII YAML document using YAML 1.2 core scalar rules."""
    source_path = Path(path)
    raw = source_path.read_bytes()
    try:
        text = raw.decode("ascii")
    except UnicodeDecodeError as error:
        prefix = raw[: error.start].decode("utf-8", errors="replace")
        line = prefix.count("\n") + 1
        column = len(prefix.rsplit("\n", 1)[-1]) + 1
        raise SpecificationError(
            [
                ValidationDiagnostic(
                    condition="non_ascii_source",
                    spec_paths=("$",),
                    context={
                        "path": str(source_path),
                        "line": line,
                        "column": column,
                    },
                )
            ]
        ) from error

    try:
        document = yaml.load(text, Loader=_Yaml12Loader)
    except yaml.YAMLError as error:
        raise _yaml_error(error) from error
    diagnostics = _unicode_scalar_diagnostics(document)
    if diagnostics:
        raise SpecificationError(diagnostics)
    return document
