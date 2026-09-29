"""The departures the specification style contract names.

Each check reads one parsed file and returns findings with zero-based
positions. None of them reads data, resolves inheritance, or validates the
specification: a style finding describes the text, never its meaning.
"""

from __future__ import annotations

from collections.abc import Iterator, Mapping, Sequence
from dataclasses import dataclass

import yaml

from yamaa.style._document import Document, last_line, member, walk

LINE_WIDTH = 79

FINDINGS: Mapping[str, str] = {
    "field_order": "REQ-1281",
    "root_spacing": "REQ-1282",
    "entry_spacing": "REQ-1283",
    "blank_lines": "REQ-1284",
    "line_width": "REQ-1285",
    "literal_form": "REQ-1250",
    "source_form": "REQ-1255",
    "invalid_suppression": "REQ-1287",
}

# REQ-1283 separates the entries of these root lists.
SPACED_LISTS = ("intermediates", "columns", "rows")

# REQ-1281 orders these mappings by their schema class.
_MEMBER_CLASSES = {
    "output": "output_class",
    "intermediates": "intermediate_class",
    "columns": "column_class",
    "rows": "row_class",
}


@dataclass(frozen=True, slots=True)
class Departure:
    """One finding before it is bound to a file: zero-based position."""

    name: str
    line: int
    column: int
    spec_path: str
    message: str


def ordered_mappings(
    document: Document,
) -> Iterator[tuple[str, str, yaml.MappingNode]]:
    """Yield (spec path, schema class, mapping) for every REQ-1281 mapping."""
    root = document.root
    if not isinstance(root, yaml.MappingNode):
        return
    yield "$", "root_class", root
    datasets = member(root, "input")
    if isinstance(datasets, yaml.MappingNode):
        for key, value in datasets.value:
            if isinstance(value, yaml.MappingNode):
                yield f"input.{key.value}", "dataset_class", value
    for field, class_name in _MEMBER_CLASSES.items():
        value = member(root, field)
        if isinstance(value, yaml.MappingNode):
            yield field, class_name, value
        elif isinstance(value, yaml.SequenceNode):
            for index, item in enumerate(value.value):
                if isinstance(item, yaml.MappingNode):
                    yield f"{field}[{index}]", class_name, item


def order_violation(
    mapping: yaml.MappingNode, order: Sequence[str]
) -> tuple[yaml.Node, str, str] | None:
    """The first field written after a field the schema orders it before."""
    rank = {name: index for index, name in enumerate(order)}
    latest: tuple[int, str] | None = None
    for key, _ in mapping.value:
        position = rank.get(key.value)
        if position is None:
            continue
        if latest is not None and position < latest[0]:
            return key, key.value, latest[1]
        if latest is None or position > latest[0]:
            latest = (position, key.value)
    return None


def header_fields(order: Sequence[str]) -> frozenset[str]:
    """Root fields written before `output`, which REQ-1282 lets stay together."""
    return frozenset(order[: order.index("output")])


def check(document: Document, orders: Mapping[str, Sequence[str]]) -> list[Departure]:
    """Every departure in `document`, before suppression."""
    if document.root is None:
        return []
    paths = _line_paths(document)
    found: list[Departure] = []
    found.extend(_field_order(document, orders))
    found.extend(_root_spacing(document, orders["root_class"]))
    found.extend(_entry_spacing(document))
    found.extend(_blank_lines(document))
    found.extend(_line_width(document, paths))
    found.extend(_literal_form(document))
    found.extend(_source_form(document))
    return sorted(found, key=lambda item: (item.line, item.column, item.name))


def _line_paths(document: Document) -> dict[int, str]:
    paths: dict[int, str] = {}
    for path, node in walk(document.root):
        paths.setdefault(node.start_mark.line, path)
    return paths


def _field_order(
    document: Document, orders: Mapping[str, Sequence[str]]
) -> Iterator[Departure]:
    for path, class_name, mapping in ordered_mappings(document):
        violation = order_violation(mapping, orders[class_name])
        if violation is None:
            continue
        key, name, later = violation
        yield Departure(
            "field_order",
            key.start_mark.line,
            key.start_mark.column,
            name if path == "$" else f"{path}.{name}",
            f"write `{name}` before `{later}`",
        )


def _gap_has_blank(document: Document, previous: yaml.Node, start: int) -> bool:
    return any(
        document.is_blank(line) for line in range(last_line(previous) + 1, start)
    )


def _root_spacing(document: Document, order: Sequence[str]) -> Iterator[Departure]:
    root = document.root
    if not isinstance(root, yaml.MappingNode) or root.flow_style:
        return
    header = header_fields(order)
    for (_, previous), (key, _) in zip(root.value, root.value[1:]):
        if key.value in header:
            continue
        start = document.lead_start(key.start_mark.line)
        if not _gap_has_blank(document, previous, start):
            yield Departure(
                "root_spacing",
                key.start_mark.line,
                key.start_mark.column,
                key.value,
                f"leave one blank line before `{key.value}`",
            )


def _entry_spacing(document: Document) -> Iterator[Departure]:
    for field in SPACED_LISTS:
        entries = member(document.root, field)
        if not isinstance(entries, yaml.SequenceNode) or entries.flow_style:
            continue
        for index in range(1, len(entries.value)):
            previous, entry = entries.value[index - 1], entries.value[index]
            multiline = (
                last_line(previous) > previous.start_mark.line
                or last_line(entry) > entry.start_mark.line
            )
            start = document.lead_start(entry.start_mark.line)
            if multiline and not _gap_has_blank(document, previous, start):
                yield Departure(
                    "entry_spacing",
                    entry.start_mark.line,
                    entry.start_mark.column,
                    f"{field}[{index}]",
                    f"leave one blank line between `{field}` entries",
                )


def blank_after_openers(document: Document) -> Iterator[int]:
    """Blank lines between a key and the block value that begins below it."""
    for _, node in walk(document.root):
        if not isinstance(node, yaml.MappingNode) or node.flow_style:
            continue
        for key, value in node.value:
            if (
                isinstance(value, (yaml.MappingNode, yaml.SequenceNode))
                and not value.flow_style
                and value.start_mark.line > key.start_mark.line
            ):
                for line in range(key.start_mark.line + 1, value.start_mark.line):
                    if document.is_blank(line):
                        yield line


def _blank_lines(document: Document) -> Iterator[Departure]:
    lines = document.lines
    reported: set[int] = set()
    for line in range(len(lines)):
        if not document.is_blank(line):
            continue
        if line == 0:
            reported.add(line)
            yield Departure("blank_lines", 0, 0, "$", "remove the leading blank line")
        elif line == len(lines) - 1:
            reported.add(line)
            yield Departure(
                "blank_lines", line, 0, "$", "remove the blank line at the end"
            )
        elif document.is_blank(line - 1):
            reported.add(line)
            yield Departure("blank_lines", line, 0, "$", "remove the second blank line")
    for line in blank_after_openers(document):
        if line not in reported:
            yield Departure(
                "blank_lines",
                line,
                0,
                "$",
                "remove the blank line after the key that opens this block",
            )
    if document.text and not document.text.endswith("\n"):
        yield Departure(
            "blank_lines",
            len(lines) - 1,
            len(lines[-1]),
            "$",
            "end the file with a line break",
        )


def _line_width(document: Document, paths: Mapping[int, str]) -> Iterator[Departure]:
    for line, text in enumerate(document.lines):
        if len(text) > LINE_WIDTH:
            yield Departure(
                "line_width",
                line,
                LINE_WIDTH,
                paths.get(line, "$"),
                f"line is {len(text)} characters; wrap it at {LINE_WIDTH}",
            )


def _mapping_nodes(document: Document) -> Iterator[yaml.MappingNode]:
    """Mappings REQ-1250 and REQ-1255 visit: mapping values and list items."""

    def visit(node: yaml.Node) -> Iterator[yaml.MappingNode]:
        yield node
        for _, value in node.value:
            if isinstance(value, yaml.MappingNode):
                yield from visit(value)
            elif isinstance(value, yaml.SequenceNode):
                for item in value.value:
                    if isinstance(item, yaml.MappingNode):
                        yield from visit(item)

    root = document.root
    if isinstance(root, yaml.MappingNode):
        yield from visit(root)
    elif isinstance(root, yaml.SequenceNode):
        for item in root.value:
            if isinstance(item, yaml.MappingNode):
                yield from visit(item)


def _literal_form(document: Document) -> Iterator[Departure]:
    """REQ-1250: a literal written in block form under its parent key."""
    paths = {id(node): path for path, node in walk(document.root)}
    for mapping in _mapping_nodes(document):
        for key, value in mapping.value:
            if not (
                isinstance(key, yaml.ScalarNode)
                and isinstance(value, yaml.MappingNode)
                and len(value.value) == 1
            ):
                continue
            literal_key, literal_value = value.value[0]
            if (
                isinstance(literal_key, yaml.ScalarNode)
                and literal_key.value == "literal"
                and isinstance(literal_value, yaml.ScalarNode)
                and literal_value.start_mark.line == literal_key.start_mark.line
                and key.start_mark.line < literal_key.start_mark.line
            ):
                yield Departure(
                    "literal_form",
                    key.start_mark.line,
                    key.start_mark.column,
                    paths.get(id(value), "$"),
                    "write the literal on one line as {literal: ...}",
                )


def _is_plain_source(value: yaml.Node) -> bool:
    return (
        isinstance(value, yaml.MappingNode)
        and len(value.value) == 1
        and isinstance(value.value[0][0], yaml.ScalarNode)
        and value.value[0][0].value == "source"
        and isinstance(value.value[0][1], yaml.ScalarNode)
    )


def _source_form(document: Document) -> Iterator[Departure]:
    """REQ-1255: a plain source written `{source: X}` where shorthand applies.

    The shorthand positions are a `derivation` value, a `derivations` entry,
    and a `case` branch `then` or `otherwise`. A filtered source, a nested
    expression argument, the `value` of a handled expression, and a
    verification's `then` keep the mapping form.
    """
    paths = {id(node): path for path, node in walk(document.root)}
    found: list[Departure] = []

    def report(key: yaml.Node, value: yaml.Node) -> None:
        found.append(
            Departure(
                "source_form",
                key.start_mark.line,
                key.start_mark.column,
                paths.get(id(value), "$"),
                "write the source as the bare string",
            )
        )

    def visit(mapping: yaml.MappingNode, in_case: bool = False) -> None:
        for key, value in mapping.value:
            name = key.value if isinstance(key, yaml.ScalarNode) else None
            if name == "case" and isinstance(value, yaml.SequenceNode):
                for item in value.value:
                    if isinstance(item, yaml.MappingNode):
                        visit(item, True)
                continue
            shorthand = name == "derivation" or (
                in_case and name in ("then", "otherwise")
            )
            if shorthand and _is_plain_source(value):
                report(key, value)
            elif name == "derivations" and isinstance(value, yaml.MappingNode):
                for entry_key, entry_value in value.value:
                    if _is_plain_source(entry_value):
                        report(entry_key, entry_value)
            if isinstance(value, yaml.MappingNode):
                visit(value, in_case)
            elif isinstance(value, yaml.SequenceNode):
                for item in value.value:
                    if isinstance(item, yaml.MappingNode):
                        visit(item, in_case)

    root = document.root
    if isinstance(root, yaml.MappingNode):
        visit(root)
    elif isinstance(root, yaml.SequenceNode):
        for item in root.value:
            if isinstance(item, yaml.MappingNode):
                visit(item)
    return iter(found)


def suppressed(
    document: Document, found: Sequence[Departure]
) -> tuple[list[Departure], list[Departure]]:
    """Split `found` into (kept, suppressed) and add REQ-1287 findings."""
    kept: list[Departure] = []
    hidden: list[Departure] = []
    valid = []
    for suppression in document.suppressions:
        unknown = [
            name
            for name in suppression.names
            if name not in FINDINGS or name == "invalid_suppression"
        ]
        if not suppression.names or unknown or not suppression.reason:
            problem = (
                f"unknown finding `{unknown[0]}`"
                if unknown
                else "name the findings and give a reason after `--`"
            )
            kept.append(
                Departure(
                    "invalid_suppression",
                    suppression.line,
                    document.lines[suppression.line].index("#"),
                    "$",
                    problem,
                )
            )
            continue
        valid.append(suppression)
    for item in found:
        if any(
            item.name in suppression.names
            and suppression.first <= item.line <= suppression.last
            for suppression in valid
        ):
            hidden.append(item)
        else:
            kept.append(item)
    kept.sort(key=lambda item: (item.line, item.column, item.name))
    return kept, hidden
