"""Layout fixes that move and insert whole lines, kept only when proved.

REQ-1286: a fix rearranges the lines of a file and never rewrites what a
value says. Each candidate edit is applied to the text, the text is parsed
again under the engine's YAML 1.2 core rules, and the edit is kept only when
the parsed value, compared with its types, and the comment lines are
unchanged. A refused edit leaves its finding reported.
"""

from __future__ import annotations

from collections import Counter
from collections.abc import Callable, Iterator, Mapping, Sequence
from dataclasses import dataclass

import yaml

from yamaa.specification._yaml import _Yaml12Loader
from yamaa.style._checks import (
    LINE_WIDTH,
    Departure,
    blank_after_openers,
    check,
    header_fields,
    order_violation,
    ordered_mappings,
    suppressed,
)
from yamaa.style._document import Document, last_line, parse, walk

# A continuation aligned after `[` starting past this column leaves too
# little room, so the wrapped list steps in from its key instead.
_ALIGN_LIMIT = 40
_MAX_EDITS = 1000


@dataclass(frozen=True, slots=True)
class _Edit:
    key: tuple[str, ...]
    apply: Callable[[], str | None]


def _typed(value: object) -> object:
    """A comparable form that keeps `1`, `1.0`, and `true` apart."""
    if isinstance(value, dict):
        members = [(_typed(key), _typed(item)) for key, item in value.items()]
        return ("map", tuple(sorted(members, key=repr)))
    if isinstance(value, list):
        return ("seq", tuple(_typed(item) for item in value))
    return (type(value).__name__, value)


def _comments(text: str) -> Counter[str]:
    return Counter(line.strip() for line in text.split("\n") if "#" in line)


def proved(before: str, after: str) -> bool:
    """True when `after` parses to the value of `before` with its comments."""
    try:
        old = yaml.load(before, Loader=_Yaml12Loader)
        new = yaml.load(after, Loader=_Yaml12Loader)
    except yaml.YAMLError:
        return False
    return _typed(old) == _typed(new) and _comments(before) == _comments(after)


def _join(lines: Sequence[str]) -> str:
    return "\n".join(lines) + "\n"


def fix_text(
    text: str,
    orders: Mapping[str, Sequence[str]],
    selected: frozenset[str] | None = None,
) -> str:
    """Apply every proved layout fix to `text` and return the result."""
    result = _fix_once(text, orders, selected)
    if result != text and _fix_once(result, orders, selected) != result:
        # REQ-1286: a fix that a second pass would change again is refused.
        return text
    return result


def _fix_once(
    text: str,
    orders: Mapping[str, Sequence[str]],
    selected: frozenset[str] | None,
) -> str:
    refused: set[tuple[str, ...]] = set()
    for _ in range(_MAX_EDITS):
        try:
            document = parse(text)
        except yaml.YAMLError:
            return text
        kept, _ = suppressed(document, check(document, orders))
        if selected is not None:
            kept = [item for item in kept if item.name in selected]
        applied = False
        for edit in _edits(document, kept, orders):
            if edit.key in refused:
                continue
            candidate = edit.apply()
            if candidate is not None and candidate != text and proved(text, candidate):
                text = candidate
                applied = True
                break
            refused.add(edit.key)
        if not applied:
            return text
    return text


def _edits(
    document: Document,
    found: Sequence[Departure],
    orders: Mapping[str, Sequence[str]],
) -> Iterator[_Edit]:
    names = {item.name for item in found}
    if "field_order" in names:
        wanted = {item.spec_path for item in found if item.name == "field_order"}
        for path, class_name, mapping in ordered_mappings(document):
            violation = order_violation(mapping, orders[class_name])
            if violation is None:
                continue
            field_path = violation[1] if path == "$" else f"{path}.{violation[1]}"
            if field_path not in wanted:
                continue
            yield _Edit(
                ("field_order", path),
                lambda path=path, mapping=mapping, order=orders[class_name]: _reorder(
                    document, mapping, order, path == "$"
                ),
            )
    for item in found:
        if item.name == "line_width":
            yield _Edit(
                ("line_width", item.spec_path, document.lines[item.line].strip()),
                lambda line=item.line: _wrap(document, line),
            )
    starts = sorted(
        {
            document.lead_start(item.line)
            for item in found
            if item.name in ("root_spacing", "entry_spacing")
        }
    )
    if starts:
        yield _Edit(("spacing",), lambda: _insert_blanks(document, starts))
    if "blank_lines" in names:
        yield _Edit(("blank_lines",), lambda: _remove_blanks(document))


def _insert_blanks(document: Document, starts: Sequence[int]) -> str:
    lines = list(document.lines)
    for start in reversed(starts):
        lines.insert(start, "")
    return _join(lines)


def _remove_blanks(document: Document) -> str:
    drop = set(blank_after_openers(document))
    lines = document.lines
    for line in range(len(lines)):
        if document.is_blank(line) and (
            line == 0 or line == len(lines) - 1 or document.is_blank(line - 1)
        ):
            drop.add(line)
    kept = [text for line, text in enumerate(lines) if line not in drop]
    while kept and not kept[0].strip():
        kept.pop(0)
    while kept and not kept[-1].strip():
        kept.pop()
    return _join(kept)


def _reorder(
    document: Document,
    mapping: yaml.MappingNode,
    order: Sequence[str],
    is_root: bool,
) -> str | None:
    """Move whole fields of a block mapping into schema order."""
    if mapping.flow_style or len(mapping.value) < 2:
        return None
    lines = document.lines
    pairs = mapping.value
    rank = {name: index for index, name in enumerate(order)}
    sequence = sorted(
        range(len(pairs)),
        key=lambda index: (rank.get(pairs[index][0].value, len(order)), index),
    )
    if sequence == list(range(len(pairs))):
        return None
    key_lines = [key.start_mark.line for key, _ in pairs]
    ends = [last_line(value) for _, value in pairs]
    starts = [key_lines[0]] + [document.lead_start(line) for line in key_lines[1:]]
    if any(starts[index] <= ends[index - 1] for index in range(1, len(pairs))):
        return None
    segments = [lines[starts[index] : ends[index] + 1] for index in range(len(pairs))]
    gaps = [[]] + [
        lines[ends[index - 1] + 1 : starts[index]] for index in range(1, len(pairs))
    ]
    if is_root:
        return _reorder_root(
            document, pairs, sequence, starts, ends, segments, gaps, order
        )
    if any(gap for gap in gaps):
        # Blank lines or detached comments between the fields of one entry
        # have no unambiguous owner once the fields move.
        return None
    column = pairs[0][0].start_mark.column
    prefix = lines[key_lines[0]][:column]
    first = sequence[0]
    if first != 0:
        if (
            starts[first] != key_lines[first]
            or lines[key_lines[first]][:column].strip()
        ):
            return None
        segments = [list(segment) for segment in segments]
        segments[first][0] = prefix + segments[first][0][column:]
        segments[0][0] = " " * column + segments[0][0][column:]
    body = [text for index in sequence for text in segments[index]]
    return _join(lines[: starts[0]] + body + lines[ends[-1] + 1 :])


def _reorder_root(
    document: Document,
    pairs: Sequence[tuple[yaml.Node, yaml.Node]],
    sequence: Sequence[int],
    starts: Sequence[int],
    ends: Sequence[int],
    segments: Sequence[list[str]],
    gaps: Sequence[list[str]],
    order: Sequence[str],
) -> str:
    lines = document.lines
    header = header_fields(order)
    names = [key.value for key, _ in pairs]

    def blanks(gap: Sequence[str]) -> int:
        return sum(1 for text in gap if not text.strip())

    def detached(gap: Sequence[str]) -> list[str]:
        content = list(gap)
        while content and not content[-1].strip():
            content.pop()
        while len(content) > 1 and not content[0].strip() and not content[1].strip():
            content.pop(0)
        return content if any(text.strip() for text in content) else []

    # A comment separated from the next field by a blank line stays with
    # the field it follows, keeping the blank line written above it.
    trailing = [detached(gaps[index + 1]) for index in range(len(pairs) - 1)]
    trailing.append(detached(lines[ends[-1] + 1 :]))
    header_style = any(
        blanks(gaps[index]) and names[index] in header and names[index - 1] in header
        for index in range(1, len(pairs))
    )
    out = list(lines[: starts[0]])
    previous: int | None = None
    for index in sequence:
        if previous is not None:
            if names[index] not in header:
                separation = 1
            elif index == previous + 1:
                separation = min(1, blanks(gaps[index]))
            else:
                separation = int(header_style)
            out.extend([""] * separation)
        out.extend(segments[index])
        out.extend(trailing[index])
        previous = index
    return _join(out)


def _block_context(document: Document) -> Iterator[tuple[yaml.Node, int, str]]:
    """Yield (node, parent column, role) for values written in block context."""
    for _, node in walk(document.root):
        if isinstance(node, yaml.MappingNode) and not node.flow_style:
            for key, value in node.value:
                if value.start_mark.line == key.start_mark.line:
                    yield value, key.start_mark.column, "value"
        elif isinstance(node, yaml.SequenceNode) and not node.flow_style:
            for item in node.value:
                yield item, item.start_mark.column, "item"


def _source(document: Document, node: yaml.Node) -> str:
    return document.text[node.start_mark.pointer : node.end_mark.pointer]


def _wrap(document: Document, line: int) -> str | None:
    """Rewrite one overlong line by the first strategy that applies."""
    for node, column, role in _block_context(document):
        start, end = node.start_mark, node.end_mark
        if not isinstance(node, yaml.ScalarNode) and not node.flow_style:
            continue
        if not start.line <= line <= end.line:
            continue
        if document.lines[end.line][end.column :].strip():
            continue
        if isinstance(node, yaml.SequenceNode) and node.flow_style:
            return _wrap_sequence(document, node, column)
        if start.line != line or end.line != line:
            continue
        if isinstance(node, yaml.MappingNode) and node.flow_style:
            return _block_mapping(document, node, column, role, line)
        if isinstance(node, yaml.ScalarNode) and node.style in ('"', "'", None):
            indent = column + 2 if role == "value" else start.column
            return _fold_scalar(document, node, indent, line)
    return None


def _replace(document: Document, line: int, new: Sequence[str]) -> str:
    lines = document.lines
    return _join(lines[:line] + list(new) + lines[line + 1 :])


def _wrap_sequence(
    document: Document, node: yaml.SequenceNode, column: int
) -> str | None:
    """Continue a flow list of scalars on following lines."""
    if not node.value or not all(
        isinstance(item, yaml.ScalarNode) for item in node.value
    ):
        return None
    first, final = node.start_mark.line, node.end_mark.line
    if any("#" in document.lines[line] for line in range(first, final + 1)):
        return None
    opening = node.start_mark.column + 1
    indent = opening if opening <= _ALIGN_LIMIT else column + 4
    items = [_source(document, item) for item in node.value]
    out: list[str] = []
    current = document.lines[first][:opening]
    filled = False
    for index, item in enumerate(items):
        piece = item + ("]" if index == len(items) - 1 else ",")
        if filled and len(current) + 1 + len(piece) > LINE_WIDTH:
            out.append(current)
            current = " " * indent + piece
        else:
            current += (" " if filled else "") + piece
        filled = True
    out.append(current)
    if any(len(part) > LINE_WIDTH for part in out):
        return None
    lines = document.lines
    return _join(lines[:first] + out + lines[final + 1 :])


def _block_mapping(
    document: Document,
    node: yaml.MappingNode,
    column: int,
    role: str,
    line: int,
) -> str | None:
    """Write a one-line flow mapping as a block mapping, member by member."""
    if not node.value:
        return None
    if len(node.value) == 1 and node.value[0][0].value in ("literal", "source"):
        # REQ-1250 keeps `{literal: X}` in flow form; REQ-1255 wants
        # `{source: X}` rewritten by its author, not moved to block form.
        return None
    text = document.lines[line]
    members = []
    for key, value in node.value:
        written = _source(document, value)
        members.append(
            f"{_source(document, key)}:" + (f" {written}" if written else "")
        )
    if role == "value":
        indent = column + 2
        out = [text[: node.start_mark.column].rstrip()]
        out.extend(" " * indent + entry for entry in members)
    else:
        indent = node.start_mark.column
        out = [text[:indent] + members[0]]
        out.extend(" " * indent + entry for entry in members[1:])
    return _replace(document, line, out)


def _fold_scalar(
    document: Document, node: yaml.ScalarNode, indent: int, line: int
) -> str | None:
    """Continue a quoted or plain scalar on following lines at single spaces.

    YAML folds a line break inside a flow scalar to one space, so breaking
    at a space between two other characters keeps the value. The proof in
    `proved` still decides whether the rewritten line is kept.
    """
    text = document.lines[line]
    written = _source(document, node)
    head = text[: node.start_mark.column]
    out: list[str] = []
    width = LINE_WIDTH - len(head)
    rest = written
    following = LINE_WIDTH - indent
    while len(rest) > width:
        breaks = [
            index
            for index in range(1, min(width + 1, len(rest) - 1))
            if rest[index] == " "
            and rest[index - 1] not in " \\"
            and rest[index + 1] not in " #"
        ]
        if not breaks:
            return None
        if node.style == '"':
            # Keep a predicate's quoted text literal on one line when a
            # break outside it exists.
            quoted, inside = set(), False
            for index, character in enumerate(rest):
                if character == "'":
                    inside = not inside
                elif inside:
                    quoted.add(index)
            breaks = [index for index in breaks if index not in quoted] or breaks
        # Prefer a break after a comma or before a Boolean connective, so a
        # predicate continues at a clause rather than inside a comparison.
        clauses = [
            index
            for index in breaks
            if index >= width // 2
            and (rest[index - 1] == "," or rest.startswith(("AND ", "OR "), index + 1))
        ]
        breaks = clauses or breaks
        cut = breaks[-1]
        closing = [index for index in breaks if len(rest) - index - 1 <= following]
        if closing:
            # The rest fits on one more line: balance the two lines rather
            # than leave a word or two alone on the last one.
            cut = min(closing, key=lambda index: max(index, len(rest) - index - 1))
        out.append(head + rest[:cut])
        rest = rest[cut + 1 :]
        head = " " * indent
        width = following
    out.append(head + rest)
    return _replace(document, line, out)
