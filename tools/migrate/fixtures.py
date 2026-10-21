"""JSON and TSV fixtures, and JSON text wherever a test writes it.

A JSON fixture holds YAML documents as strings, specifications as objects,
and the spec paths of expected diagnostics as strings; a TSV fixture holds a
JSON value in a cell. Both read into the groups `tools.migrate.sources`
defines, located by their exact spans, so a converted value replaces only its
own text and the rest of the file stays byte for byte:

- a string is one piece, written back as a JSON string;
- an object that declares `schema_version` (or `yamaa`) is one document,
  written back in the layout it was written in.

A specification written as JSON converts through its block rendering: the file
converter edits block YAML (its insertions write block lines), so the object is
rendered with every string double-quoted and every number, Boolean and null as
written, converted, and written back as JSON in the original layout. An object
whose layout `json.dumps` does not reproduce is listed instead.
"""

import json
import re

from tools.migrate.sources import PATH_LIKE, Extraction, Group, Piece
from tools.migrate.text import compose, double_quoted, value_of

_NUMBER = re.compile(r"-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][-+]?[0-9]+)?")
_WHITE = " \t\r\n"
_PLAIN_KEY = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
_RESERVED = frozenset(
    {"null", "Null", "NULL", "true", "True", "TRUE", "false", "False", "FALSE"}
)
HEADERS = ("schema_version", "yamaa")


class JsonString:
    """A JSON string token."""

    def __init__(self, ascii):
        self.ascii = ascii

    def encode(self, value):
        return json.dumps(value, ensure_ascii=self.ascii)


class Verbatim:
    """A JSON object written as the converted JSON text itself."""

    def encode(self, value):
        return value


def scan(text, start=0, end=None):
    """The JSON value of text[start:end] as nested (kind, start, end, content), or None."""
    end = len(text) if end is None else end

    def skip(position):
        while position < end and text[position] in _WHITE:
            position += 1
        return position

    def value(position):
        position = skip(position)
        if position >= end:
            raise ValueError("value expected")
        character = text[position]
        if character in "{[":
            close = "}" if character == "{" else "]"
            items = []
            cursor = skip(position + 1)
            if text.startswith(close, cursor):
                return (character, position, cursor + 1, items)
            while True:
                if character == "{":
                    key = value(cursor)
                    if key[0] != "string":
                        raise ValueError("key expected")
                    cursor = skip(key[2])
                    if not text.startswith(":", cursor):
                        raise ValueError("colon expected")
                    item = value(cursor + 1)
                    items.append((key, item))
                else:
                    item = value(cursor)
                    items.append(item)
                cursor = skip(item[2])
                if text.startswith(",", cursor):
                    cursor = skip(cursor + 1)
                    continue
                if text.startswith(close, cursor):
                    return (character, position, cursor + 1, items)
                raise ValueError("comma expected")
        if character == '"':
            content, stop = json.decoder.scanstring(text, position + 1)
            return ("string", position, stop, content)
        match = _NUMBER.match(text, position)
        if match and match.end() > position:
            return ("number", position, match.end(), match.group())
        for word in ("true", "false", "null"):
            if text.startswith(word, position):
                return ("literal", position, position + len(word), word)
        raise ValueError("value expected")

    try:
        root = value(start)
    except (ValueError, IndexError):
        return None
    return root if skip(root[2]) == end else None


def _base(text, offset):
    """The indent of the line holding `offset`."""
    line = max(text.rfind("\n", 0, offset), text.rfind("\r", 0, offset)) + 1
    return len(text[line:offset]) - len(text[line:offset].lstrip(" "))


def _collect(text, node, scope, groups, key=None):
    kind, start, end, content = node
    if kind == "string":
        style = JsonString(text[start:end].isascii())
        piece = Piece(start, end, content, style, [start] * len(content))
        group = Group("json", [piece], start, end, scope=scope, label=key)
        if key is not None and PATH_LIKE.fullmatch(key):
            group.name = key
        groups.append(group)
    elif kind == "{":
        keys = {item[3] for item, _ in content}
        if keys & set(HEADERS):
            raw = text[start:end]
            piece = Piece(start, end, raw, Verbatim(), [start] * len(raw))
            base = _base(text, start)
            groups.append(
                Group("json", [piece], start, end, scope=scope, base=base, label=key)
            )
            return
        for name, item in content:
            _collect(text, item, (*scope, start), groups, name[3])
    elif kind == "[":
        for item in content:
            _collect(text, item, (*scope, start), groups, key)


def json_extraction(text):
    """The strings and specification objects of a JSON file, as groups."""
    extraction = Extraction(text)
    root = scan(text)
    if root is not None:
        _collect(text, root, (), extraction.groups)
    return extraction.ordered()


def tsv_extraction(text):
    """The strings and specification objects of each JSON cell of a TSV file."""
    extraction = Extraction(text)
    for row, match in enumerate(re.finditer(r"([^\r\n]*)(?:\r\n|\r|\n|$)", text)):
        cell_start = match.start()
        for cell in match[1].split("\t"):
            stripped = cell.strip()
            if stripped[:1] in ("{", "[", '"'):
                node = scan(text, cell_start, cell_start + len(cell))
                if node is not None:
                    _collect(text, node, (row,), extraction.groups)
            cell_start += len(cell) + 1
    return extraction.ordered()


# -- the JSON route ------------------------------------------------------------------


def is_json(text):
    """True for a JSON object or array, the forms a specification is written in."""
    stripped = text.strip()
    if stripped[:1] not in ("{", "["):
        return False
    try:
        json.loads(stripped)
    except ValueError:
        return False
    return True


def _dump(value, layout):
    indent, separators, ascii, base = layout
    out = json.dumps(value, indent=indent, separators=separators, ensure_ascii=ascii)
    if base and indent is not None:
        out = out.replace("\n", "\n" + " " * base)
    return out


def layout_of(text, base=0):
    """The `json.dumps` arguments that reproduce `text` exactly, or None."""
    stripped = text.strip()
    value = json.loads(stripped)
    ascii = stripped.isascii()
    for indent, separators in (
        (None, (",", ":")),
        (None, (", ", ": ")),
        (2, (",", ": ")),
        (4, (",", ": ")),
        (1, (",", ": ")),
    ):
        layout = (indent, separators, ascii, base)
        if _dump(value, layout) == stripped:
            return layout
    return None


def _key(name):
    if _PLAIN_KEY.fullmatch(name) and name not in _RESERVED:
        return name
    return double_quoted(name)


def _scalar(value):
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, (int, float)):
        return repr(value)
    if isinstance(value, dict):
        return "{}"
    if isinstance(value, list):
        return "[]"
    return double_quoted(value)


def block(value, indent=0):
    """A JSON value as block YAML: strings double-quoted, every other scalar
    plain, nested mappings and lists in block style."""
    pad = " " * indent
    lines = []
    if isinstance(value, dict) and value:
        for name, item in value.items():
            if isinstance(item, (dict, list)) and item:
                lines.append(f"{pad}{_key(name)}:")
                lines.append(block(item, indent + 2))
            else:
                lines.append(f"{pad}{_key(name)}: {_scalar(item)}")
    elif isinstance(value, list) and value:
        for item in value:
            if isinstance(item, (dict, list)) and item:
                inner = block(item, indent + 2)
                lines.append(f"{pad}- {inner[indent + 2 :]}")
            else:
                lines.append(f"{pad}- {_scalar(item)}")
    else:
        lines.append(pad + _scalar(value))
    return "\n".join(lines)


def to_json(root):
    """The JSON value of a YAML node tree read under the YAML 1.2 core schema."""
    return value_of(root)


def rendered(text):
    """A JSON text's block rendering, the form the file converter edits."""
    return block(json.loads(text.strip())) + "\n"


def written_back(text, base, converted):
    """The JSON text of a converted block rendering, in the layout `text` has:
    (JSON text, None), or (None, reason) when the value changes and that layout
    cannot be written."""
    value = json.loads(text.strip())
    new = to_json(compose(converted))
    if new == value:
        return text, None
    layout = layout_of(text, base)
    if layout is None:
        return None, "in a JSON layout json.dumps does not write"
    leading = text[: len(text) - len(text.lstrip())]
    trailing = text[len(text.rstrip()) :]
    return leading + _dump(new, layout) + trailing, None
