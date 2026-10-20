"""Text of one YAML document, read at PyYAML marks and edited in place.

The converter never re-serializes a document. It composes the text with
PyYAML's base loader, which keeps every node's marks and interprets no
scalar, and replaces exact character ranges, as `yamaa.style` does. What a
range does not cover, comments, blank lines and the authored layout of every
other value, stays byte for byte. Scalars are typed under the YAML 1.2 core
schema that SCHEMA-4 names.
"""

import bisect
import re
from dataclasses import dataclass

import yaml

LINE_BREAK = re.compile(r"\r\n|\r|\n")
LINE_WIDTH = 79

_NULL = re.compile(r"null|Null|NULL|~|")
_BOOL = re.compile(r"true|True|TRUE|false|False|FALSE")
_INT = re.compile(r"[-+]?[0-9]+|0o[0-7]+|0x[0-9a-fA-F]+")
_FLOAT = re.compile(
    r"[-+]?(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?"
    r"|[-+]?\.(inf|Inf|INF)|\.nan|\.NaN|\.NAN"
)
_SEGMENT = re.compile(r"[A-Za-z0-9_-]+")

try:
    _LOADER = yaml.CBaseLoader
except AttributeError:  # pragma: no cover - libyaml is optional
    _LOADER = yaml.BaseLoader


def compose(text):
    """The node tree of `text`, scalars uninterpreted; None for no document."""
    return yaml.compose(text, Loader=_LOADER)


def plain(node):
    """True for a plain scalar (libyaml reports its style as '', PyYAML as None)."""
    return isinstance(node, yaml.ScalarNode) and node.style in (None, "")


def core_type(node):
    """The YAML 1.2 core-schema type of a scalar node: null, bool, int, float or str."""
    if not isinstance(node, yaml.ScalarNode):
        return None
    if not plain(node):
        return "str"
    value = node.value
    if _NULL.fullmatch(value):
        return "null"
    if _BOOL.fullmatch(value):
        return "bool"
    if _INT.fullmatch(value):
        return "int"
    if _FLOAT.fullmatch(value):
        return "float"
    return "str"


def is_null(node):
    return core_type(node) == "null"


def value_of(node):
    """The plain Python value of a node under the YAML 1.2 core schema."""
    if node is None:
        return None
    if isinstance(node, yaml.ScalarNode):
        kind = core_type(node)
        if kind == "null":
            return None
        if kind == "bool":
            return node.value.lower() == "true"
        if kind == "int":
            text = node.value
            if text.startswith("0o"):
                return int(text[2:], 8)
            if text.startswith("0x"):
                return int(text[2:], 16)
            return int(text)
        if kind == "float":
            text = node.value.lower()
            if text.endswith("inf"):
                return float("-inf") if text.startswith("-") else float("inf")
            if text.endswith("nan"):
                return float("nan")
            return float(node.value)
        return node.value
    if isinstance(node, yaml.SequenceNode):
        return [value_of(item) for item in node.value]
    if isinstance(node, yaml.MappingNode):
        return {value_of(key): value_of(value) for key, value in node.value}
    return None


def member(node, name):
    """The value node of mapping `node` under key `name`, or None."""
    pair = member_pair(node, name)
    return pair[1] if pair else None


def member_pair(node, name):
    if isinstance(node, yaml.MappingNode):
        for key, value in node.value:
            if isinstance(key, yaml.ScalarNode) and key.value == name:
                return key, value
    return None


def double_quoted(value):
    """`value` as a YAML double-quoted scalar."""
    out = []
    for character in value:
        if character == "\\":
            out.append("\\\\")
        elif character == '"':
            out.append('\\"')
        elif character == "\n":
            out.append("\\n")
        elif character == "\t":
            out.append("\\t")
        elif ord(character) < 0x20 or ord(character) == 0x7F:
            out.append(f"\\x{ord(character):02x}")
        else:
            out.append(character)
    return '"' + "".join(out) + '"'


def path_segment(name):
    """One P-CLR-1 spec-path segment: bare when it is a plain segment, else quoted."""
    if _SEGMENT.fullmatch(name):
        return name
    return "['" + name.replace("'", "''") + "']"


@dataclass(frozen=True)
class Edit:
    """Replace text[start:end] with `text`; an insertion has start == end."""

    start: int
    end: int
    text: str


class Text:
    """A document's text with its line table and mark helpers."""

    def __init__(self, text):
        self.text = text
        self.starts = [0] + [match.end() for match in LINE_BREAK.finditer(text)]
        endings = LINE_BREAK.findall(text)
        self.newline = max(("\n", "\r\n", "\r"), key=endings.count) if endings else "\n"

    def line(self, offset):
        """Zero-based line holding `offset`."""
        return bisect.bisect_right(self.starts, offset) - 1

    def line_start(self, line):
        return self.starts[line]

    def line_end(self, line):
        """Offset of the line break ending `line`, or of the end of text."""
        if line + 1 < len(self.starts):
            following = self.starts[line + 1]
            if self.text.startswith("\r\n", following - 2):
                return following - 2
            return following - 1
        return len(self.text)

    def next_line_start(self, line):
        return self.starts[line + 1] if line + 1 < len(self.starts) else len(self.text)

    def line_text(self, line):
        return self.text[self.line_start(line) : self.line_end(line)]

    def indent(self, line):
        text = self.line_text(line)
        return len(text) - len(text.lstrip(" "))

    def column(self, offset):
        return offset - self.line_start(self.line(offset))

    def line_count(self):
        if self.starts[-1] == len(self.text) and len(self.starts) > 1:
            return len(self.starts) - 1
        return len(self.starts)


def start(node):
    return node.start_mark.index


def end(node, text):
    """Offset just past the last character a node's own text covers."""
    if isinstance(node, yaml.MappingNode) and not node.flow_style:
        if not node.value:
            return node.start_mark.index
        key, value = node.value[-1]
        return max(end(key, text), _pair_end(key, value, text))
    if isinstance(node, yaml.SequenceNode) and not node.flow_style:
        if not node.value:
            return node.start_mark.index
        return end(node.value[-1], text)
    stop = node.end_mark.index
    if isinstance(node, yaml.ScalarNode) and node.style in ("|", ">"):
        written = text[node.start_mark.index : stop].rstrip()
        return node.start_mark.index + len(written)
    return stop


def _pair_end(key, value, text):
    if plain(value) and value.value == "":
        # An empty value ends at the colon that follows its key.
        colon = text.find(":", key.end_mark.index)
        return colon + 1 if colon >= 0 else key.end_mark.index
    return end(value, text)


def pair_end(key, value, text):
    return _pair_end(key, value, text)


def comments(text, root):
    """The (start, end) ranges of every comment in `text`.

    A `#` starts a comment when it begins a line or follows white space and
    lies outside every scalar; a plain scalar never holds ` #`, so only quoted
    and block scalars need excluding.
    """
    spans = []
    if root is not None:
        stack = [root]
        while stack:
            node = stack.pop()
            if isinstance(node, yaml.ScalarNode):
                if not plain(node):
                    spans.append((node.start_mark.index, end(node, text)))
            elif isinstance(node, yaml.SequenceNode):
                stack.extend(node.value)
            elif isinstance(node, yaml.MappingNode):
                for key, value in node.value:
                    stack.append(key)
                    stack.append(value)
    spans.sort()
    starts = [span[0] for span in spans]
    found = []
    for match in re.finditer("#", text):
        offset = match.start()
        if offset and text[offset - 1] not in " \t\r\n":
            continue
        index = bisect.bisect_right(starts, offset) - 1
        if index >= 0 and spans[index][0] <= offset < spans[index][1]:
            continue
        stop = LINE_BREAK.search(text, offset)
        found.append((offset, stop.start() if stop else len(text)))
    return found


def comment_texts(text, root=None):
    """Every comment's text, stripped, in document order."""
    if root is None:
        try:
            root = compose(text)
        except yaml.YAMLError:
            root = None
    return [text[a:b].strip() for a, b in comments(text, root)]


class EditPlan:
    """Non-overlapping edits collected for one pass over one text."""

    def __init__(self):
        self.edits = []

    def fits(self, edit):
        for other in self.edits:
            if edit.start < other.end and other.start < edit.end:
                return False
            if edit.start == edit.end == other.start == other.end:
                return False
            # An insertion strictly inside a replaced range overlaps it.
            if edit.start == edit.end and other.start < edit.start < other.end:
                return False
            if other.start == other.end and edit.start < other.start < edit.end:
                return False
        return True

    def add(self, *edits):
        """Add all of `edits` or none of them; True when they were added."""
        accepted = []
        for edit in edits:
            if not self.fits(edit) or any(
                edit.start < other.end and other.start < edit.end for other in accepted
            ):
                return False
            accepted.append(edit)
        self.edits.extend(accepted)
        return True

    def apply(self, text):
        out = text
        for edit in sorted(
            self.edits, key=lambda item: (item.start, item.end), reverse=True
        ):
            out = out[: edit.start] + edit.text + out[edit.end :]
        return out

    def __bool__(self):
        return bool(self.edits)
