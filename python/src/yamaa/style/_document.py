"""One specification file as written: its lines, nodes, and suppressions.

The style contract judges the text a person reads, so this module keeps what
the engine loader discards: the YAML node of every value with its marks, the
comment lines, and the lines that belong to a multi-line scalar and are
therefore content rather than layout.
"""

from __future__ import annotations

import re
from collections.abc import Iterator
from dataclasses import dataclass, field

import yaml

from yamaa.specification._yaml import _Yaml12Loader

_SUPPRESSION = re.compile(
    r"^#\s*yamaa-style:\s*allow\s+(?P<names>[a-z_, ]+?)\s*(?:--\s*(?P<reason>.*))?$"
)


@dataclass(frozen=True, slots=True)
class Suppression:
    """One `# yamaa-style: allow` comment and the lines it covers."""

    line: int
    names: tuple[str, ...]
    reason: str
    first: int
    last: int


@dataclass(slots=True)
class Document:
    """A parsed specification file.

    Line numbers are zero-based here; findings report them one-based.
    """

    text: str
    lines: list[str]
    root: yaml.Node | None
    content_lines: frozenset[int] = field(default_factory=frozenset)
    suppressions: tuple[Suppression, ...] = ()

    def is_blank(self, line: int) -> bool:
        return line not in self.content_lines and not self.lines[line].strip()

    def is_comment(self, line: int) -> bool:
        return line not in self.content_lines and self.lines[line].lstrip().startswith(
            "#"
        )

    def lead_start(self, line: int) -> int:
        """First line of the comment block written directly above `line`."""
        start = line
        while start > 0 and self.is_comment(start - 1):
            start -= 1
        return start


def parse(text: str) -> Document:
    """Compose `text` under the engine's YAML 1.2 core rules, keeping marks."""
    root = yaml.compose(text, Loader=_Yaml12Loader)
    lines = text.split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    document = Document(text=text, lines=lines, root=root)
    if root is not None:
        document.content_lines = frozenset(_content_lines(root))
    document.suppressions = tuple(_suppressions(document))
    return document


def last_line(node: yaml.Node) -> int:
    """Zero-based line holding the last character of `node`."""
    if isinstance(node, yaml.MappingNode) and not node.flow_style and node.value:
        return last_line(node.value[-1][1])
    if isinstance(node, yaml.SequenceNode) and not node.flow_style and node.value:
        return last_line(node.value[-1])
    end = node.end_mark
    if isinstance(node, yaml.ScalarNode) and node.style in ("|", ">"):
        # A block scalar's end mark follows its trailing blank lines; the
        # scalar ends on its last written line.
        written = end.buffer[: end.pointer].rstrip()
        return max(node.start_mark.line, written.count("\n"))
    if end.column == 0 and end.line > node.start_mark.line:
        return end.line - 1
    return end.line


def _content_lines(node: yaml.Node) -> Iterator[int]:
    """Lines after the first of every multi-line scalar or flow collection."""
    first, final = node.start_mark.line, last_line(node)
    if isinstance(node, yaml.ScalarNode) or node.flow_style:
        yield from range(first + 1, final + 1)
        return
    children: list[yaml.Node] = []
    if isinstance(node, yaml.MappingNode):
        for key, value in node.value:
            children.extend((key, value))
    else:
        children.extend(node.value)
    for child in children:
        yield from _content_lines(child)


def walk(node: yaml.Node, path: str = "$") -> Iterator[tuple[str, yaml.Node]]:
    """Yield every node with its positional spec path (`columns[3].name`)."""
    yield path, node
    if isinstance(node, yaml.MappingNode):
        for key, value in node.value:
            yield from walk(value, _join(path, key.value))
    elif isinstance(node, yaml.SequenceNode):
        for index, item in enumerate(node.value):
            yield from walk(item, f"{path}[{index}]")


def _join(path: str, key: object) -> str:
    return str(key) if path == "$" else f"{path}.{key}"


def member(node: yaml.Node | None, name: str) -> yaml.Node | None:
    """The value of mapping `node` under `name`, if it has one."""
    if isinstance(node, yaml.MappingNode):
        for key, value in node.value:
            if isinstance(key, yaml.ScalarNode) and key.value == name:
                return value
    return None


def _spans(document: Document) -> dict[int, int]:
    """Map each line an entry or field starts on to the last line it covers."""
    spans: dict[int, int] = {}
    if document.root is None:
        return spans

    def record(start: int, end: int) -> None:
        spans[start] = max(end, spans.get(start, start))

    for _, node in walk(document.root):
        if isinstance(node, yaml.MappingNode):
            for key, value in node.value:
                record(key.start_mark.line, last_line(value))
        elif isinstance(node, yaml.SequenceNode):
            for item in node.value:
                record(item.start_mark.line, last_line(item))
    return spans


def _suppressions(document: Document) -> Iterator[Suppression]:
    spans = _spans(document)
    for line, text in enumerate(document.lines):
        if not document.is_comment(line):
            continue
        match = _SUPPRESSION.match(text.strip())
        if match is None:
            if "yamaa-style:" in text:
                yield Suppression(line, (), "", line, line)
            continue
        names = tuple(
            name.strip() for name in match["names"].split(",") if name.strip()
        )
        target = line + 1
        while target < len(document.lines) and document.is_comment(target):
            target += 1
        last = spans.get(target, target)
        yield Suppression(line, names, (match["reason"] or "").strip(), target, last)
