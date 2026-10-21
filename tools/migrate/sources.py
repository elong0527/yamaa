"""String literals in Rust, Python and R sources, decoded and written back.

The embedded converter (`tools.migrate.embedded`) reads YAML out of test
sources. This module finds every string a source spells with literals, with
the pieces it is built from, and decodes each piece under its language's
rules:

- Rust: normal, raw and byte strings;
- Python: string and bytes literals through `ast`, with the pieces of an
  implicit concatenation read from `tokenize`;
- R: double- and single-quoted strings, and a `c(...)` vector of literals,
  one line each, as `writeLines` writes it.

A string built by interpolation is reported with its template and never
converted: a format string that holds a brace (`format!` and its family,
`.format()`), an f-string, `%`, `+`, `+=` or `push_str` beside a value,
`paste0`, `paste` and `sprintf`, and a `c(...)` element that is not a
literal. Each piece keeps the style it was written in (quotes, raw
delimiters, prefixes, escapes and line breaks), so a converted text is
written back the same way. A literal a call writes next to a file name
(`study.write("project.yaml", ...)`) carries that name.
"""

import ast
import bisect
import difflib
import io
import re
import tokenize
from dataclasses import dataclass, field

# Where an interpolated template holds a value that is not a literal.
HOLE = "{...}"
HOLE_WORD = "HOLE"
LINE_BREAK = re.compile(r"\r\n|\r|\n")

# A file name a call writes a literal to, or reads an input from.
PATH_LIKE = re.compile(r"[\w.\-/]*\.(?:ya?ml|csv)")

# A literal handed to one of these is a pattern or a probe of some other
# text, never a document of its own: it is not converted.
REPLACE_CALLS = frozenset({"replace", "replacen", "sub", "gsub"})
PROBE_CALLS = frozenset(
    "contains starts_with ends_with find rfind matches split split_once strip_prefix"
    " strip_suffix trim_start_matches trim_end_matches startswith endswith index"
    " count assertIn assertNotIn assertRegex assertNotRegex search match fullmatch"
    " grepl grep regexpr gregexpr regmatches startsWith endsWith".split()
)


@dataclass
class Piece:
    """One literal token: its source range, decoded text and style."""

    start: int
    end: int
    value: str
    style: object
    offsets: list  # the source offset of each decoded character


@dataclass(eq=False)
class Group:
    """A string a source builds from literal pieces, or an interpolated template.

    An R vector of lines (`lines`) holds one piece per line and spells the
    text the lines make with a line break after each. `template` is set when
    a part is not a literal; such a group is listed, never converted.
    """

    language: str
    pieces: list
    start: int
    end: int
    lines: bool = False
    template: str = None
    scope: tuple = ()
    name: str = None
    role: str = "value"
    converted: str = None
    skeleton: str = None
    uses: list = field(default_factory=list)  # scopes that use the name it is bound to
    base: int = 0  # the indent of the line a JSON object starts on
    label: str = None  # the JSON key a fixture value sits under

    @property
    def interpolated(self):
        return self.template is not None

    @property
    def text(self):
        if self.lines:
            return "".join(piece.value + "\n" for piece in self.pieces)
        return "".join(piece.value for piece in self.pieces)

    def offset_of(self, index):
        """The source offset of character `index` of the group's text."""
        position = 0
        for piece in self.pieces:
            size = len(piece.value) + (1 if self.lines else 0)
            if index < position + size:
                local = index - position
                return piece.offsets[local] if local < len(piece.value) else piece.end
            position += size
        return self.end

    def rewrite(self, converted):
        """The source edits that make this group spell `converted`."""
        if self.lines:
            return _rewrite_lines(self, converted)
        return _rewrite_pieces(self, converted)


@dataclass
class Extraction:
    """The groups of one source and its line table."""

    text: str
    groups: list = field(default_factory=list)

    def __post_init__(self):
        self.starts = [0] + [match.end() for match in LINE_BREAK.finditer(self.text)]

    def ordered(self):
        """This extraction with its groups in source order."""
        self.groups.sort(key=lambda group: (group.start, group.end))
        return self

    def line(self, offset):
        """The one-based line holding `offset`."""
        return bisect.bisect_right(self.starts, offset)


def apply(text, edits):
    """Apply non-overlapping (start, end, replacement) edits to `text`."""
    ordered = sorted(edits, key=lambda edit: (edit[0], edit[1]))
    for previous, following in zip(ordered, ordered[1:]):
        if following[0] < previous[1]:
            raise ValueError(f"overlapping edits at {previous[:2]} and {following[:2]}")
    out = text
    for start, end, replacement in reversed(ordered):
        out = out[:start] + replacement + out[end:]
    return out


# -- writing a converted text back ---------------------------------------------


def _line_starts(lines):
    starts = [0]
    for line in lines:
        starts.append(starts[-1] + len(line))
    return starts


def _map_within(original, converted, offset):
    """An offset strictly inside an unchanged run of a changed block, mapped."""
    matcher = difflib.SequenceMatcher(None, original, converted, autojunk=False)
    for i, j, size in matcher.get_matching_blocks():
        if i < offset < i + size:
            return j + offset - i
    return None


def map_offsets(original, converted, offsets):
    """Each offset of `original` at the same place in `converted`, or None.

    Lines are aligned first; an offset inside a changed block maps only when
    the characters on both sides of it are unchanged. An offset where text
    was inserted maps after the insertion, so new lines join the piece
    before them."""
    a_lines = original.splitlines(keepends=True)
    b_lines = converted.splitlines(keepends=True)
    a_starts, b_starts = _line_starts(a_lines), _line_starts(b_lines)
    opcodes = difflib.SequenceMatcher(
        None, a_lines, b_lines, autojunk=False
    ).get_opcodes()
    mapped = []
    for offset in offsets:
        candidates = []
        for tag, i1, i2, j1, j2 in opcodes:
            a1, a2, b1, b2 = a_starts[i1], a_starts[i2], b_starts[j1], b_starts[j2]
            if tag == "equal":
                if a1 <= offset <= a2:
                    candidates.append(b1 + offset - a1)
            elif offset == a1 == a2:
                candidates.append(b2)
            elif offset == a1:
                candidates.append(b1)
            elif offset == a2:
                candidates.append(b2)
            elif a1 < offset < a2:
                inner = _map_within(original[a1:a2], converted[b1:b2], offset - a1)
                if inner is not None:
                    candidates.append(b1 + inner)
        mapped.append(max(candidates) if candidates else None)
    return mapped


def _rewrite_pieces(group, converted):
    original = group.text
    bounds = [0]
    for piece in group.pieces:
        bounds.append(bounds[-1] + len(piece.value))
    inner = map_offsets(original, converted, bounds[1:-1])
    mapped = [0, *inner, len(converted)]
    kept = [0]
    for index in range(1, len(bounds)):
        image = mapped[index]
        if index == len(bounds) - 1 or (
            image is not None and image >= mapped[kept[-1]]
        ):
            kept.append(index)
    edits = []
    for first, last in zip(kept, kept[1:]):
        before = original[bounds[first] : bounds[last]]
        after = converted[mapped[first] : mapped[last]]
        if before == after:
            continue
        head, tail = group.pieces[first], group.pieces[last - 1]
        edits.append((head.start, tail.end, head.style.encode(after)))
    return edits


def _rewrite_lines(group, converted):
    old = [piece.value for piece in group.pieces]
    new = converted.split("\n")
    if new and new[-1] == "":
        new.pop()
    pieces = group.pieces
    edits = []

    def encode(index, line):
        return pieces[min(index, len(pieces) - 1)].style.encode(line)

    def insert_after(index, lines):
        if index < 0:
            at = pieces[0].start
            edits.append((at, at, "".join(encode(0, line) + ", " for line in lines)))
        else:
            at = pieces[index].end
            edits.append(
                (at, at, "".join(", " + encode(index, line) for line in lines))
            )

    def delete(first, last):
        if first > 0:
            edits.append((pieces[first - 1].end, pieces[last - 1].end, ""))
        else:
            edits.append((pieces[first].start, pieces[last].start, ""))

    matcher = difflib.SequenceMatcher(None, old, new, autojunk=False)
    for tag, i1, i2, j1, j2 in matcher.get_opcodes():
        if tag == "equal":
            continue
        if tag == "insert":
            insert_after(i1 - 1, new[j1:j2])
            continue
        if tag == "delete":
            delete(i1, i2)
            continue
        shared = min(i2 - i1, j2 - j1)
        for step in range(shared):
            piece = pieces[i1 + step]
            edits.append((piece.start, piece.end, encode(i1 + step, new[j1 + step])))
        if i2 - i1 > shared:
            delete(i1 + shared, i2)
        if j2 - j1 > shared:
            insert_after(i1 + shared - 1, new[j1 + shared : j2])
    return edits


# -- styles ----------------------------------------------------------------------


def escape(value, quote, raw_breaks=False, wide=None):
    """`value` escaped for a quoted literal: backslashes, `quote`, line breaks
    (kept as they are with `raw_breaks`), tabs and other controls; `wide`
    writes a non-ASCII character, which stays as it is when it is None."""
    out = []
    for index, character in enumerate(value):
        code = ord(character)
        if character == "\\":
            out.append("\\\\")
        elif character == quote[0] and (
            len(quote) == 1
            or value.startswith(quote[0] * 2, index + 1)
            or index == len(value) - 1
        ):
            out.append("\\" + character)
        elif character == "\n":
            out.append("\n" if raw_breaks else "\\n")
        elif character in "\t\r":
            out.append("\\t" if character == "\t" else "\\r")
        elif code < 0x20 or code == 0x7F:
            out.append(f"\\x{code:02x}")
        elif code > 0x7F and wide is not None:
            out.append(wide(code))
        else:
            out.append(character)
    return "".join(out)


@dataclass(frozen=True)
class RustStyle:
    """How a Rust literal is written: its prefix, raw hashes and line breaks."""

    prefix: str = ""
    hashes: int = -1
    multiline: bool = False
    ascii: bool = True

    def encode(self, value):
        if self.hashes >= 0 and (value.isascii() or self.prefix != "b"):
            hashes = self.hashes
            while '"' + "#" * hashes in value:
                hashes += 1
            return f'{self.prefix}r{"#" * hashes}"{value}"{"#" * hashes}'
        if self.prefix == "b":
            wide = "\\x{:02x}".format
        elif self.ascii:
            wide = "\\u{{{:x}}}".format
        else:
            wide = None
        return f'{self.prefix}"{escape(value, '"', self.multiline, wide)}"'


@dataclass(frozen=True)
class PythonStyle:
    """How a Python literal is written: prefix, raw, quotes."""

    prefix: str = ""
    raw: bool = False
    quote: str = '"'
    ascii: bool = True

    def encode(self, value):
        quote = self.quote
        if len(quote) == 1:
            # ruff format's choice: double quotes unless the text holds more
            # double quotes than single ones.
            quote = "'" if value.count('"') > value.count("'") else '"'
        if self.raw and quote[0] not in value and not value.endswith("\\"):
            if len(quote) == 3 or "\n" not in value:
                return f"{self.prefix}r{quote}{value}{quote}"
        if "b" in self.prefix:
            wide = "\\x{:02x}".format
        elif self.ascii:
            wide = _python_wide
        else:
            wide = None
        body = escape(value, quote, len(quote) == 3, wide)
        return f"{self.prefix}{quote}{body}{quote}"


def _python_wide(code):
    return f"\\u{code:04x}" if code <= 0xFFFF else f"\\U{code:08x}"


@dataclass(frozen=True)
class RStyle:
    """How an R literal is written: its quote, kept unless that needs escapes."""

    quote: str = '"'
    ascii: bool = True

    def encode(self, value):
        other = "'" if self.quote == '"' else '"'
        quote = other if self.quote in value and other not in value else self.quote
        wide = "\\u{{{:x}}}".format if self.ascii else None
        return f"{quote}{escape(value, quote, False, wide)}{quote}"


# -- a bracket-aware token stream, shared by the Rust and R lexers ----------------


@dataclass
class Token:
    kind: str  # str, ident, punct, open, close, other
    start: int
    end: int
    text: str
    piece: Piece = None


@dataclass
class Frame:
    """One bracket pair: a call, a macro, a block or a plain group."""

    kind: str
    name: str
    open: int
    parent: int = None
    close: int = None
    commas: list = field(default_factory=list)

    def arg_of(self, index):
        return bisect.bisect_left(self.commas, index)

    def arg_range(self, arg):
        first = self.commas[arg - 1] + 1 if arg else self.open + 1
        last = (
            self.commas[arg] - 1
            if arg < len(self.commas)
            else (self.close - 1 if self.close is not None else self.open)
        )
        return first, last

    def arg_count(self):
        if self.close is None or self.close == self.open + 1:
            return 0
        return len(self.commas) + 1


class Stream:
    """Tokens with the frame each one lies directly in."""

    def __init__(self, tokens, opener):
        self.tokens = tokens
        self.frames = []
        self.frame_of = []
        stack = []
        for index, token in enumerate(tokens):
            self.frame_of.append(stack[-1] if stack else None)
            if token.kind == "open":
                kind, name = opener(tokens, index)
                self.frames.append(
                    Frame(kind, name, index, stack[-1] if stack else None)
                )
                stack.append(len(self.frames) - 1)
            elif token.kind == "close":
                if stack:
                    self.frames[stack.pop()].close = index
            elif token.text == "," and stack:
                self.frames[stack[-1]].commas.append(index)

    def frame(self, index):
        found = self.frame_of[index]
        return (found, self.frames[found]) if found is not None else (None, None)

    def scope(self, index, braces=("{",)):
        """The enclosing brace frames of a token, outermost first."""
        chain = []
        current = self.frame_of[index]
        while current is not None:
            frame = self.frames[current]
            if self.tokens[frame.open].text in braces:
                chain.append(current)
            current = frame.parent
        return tuple(reversed(chain))

    def strings_in(self, first, last):
        return [
            token
            for token in self.tokens[first : last + 1]
            if token.kind == "str" and token.piece is not None
        ]

    def lone(self, frame, arg, index, skip=("&",), methods=()):
        """True when argument `arg` of `frame` is token `index` alone, give or
        take a leading `&` and a value-preserving method chain."""
        first, last = frame.arg_range(arg)
        while first < index and self.tokens[first].text in skip:
            first += 1
        if first != index:
            return False
        return self.after_chain(index, methods) == last + 1

    def after_chain(self, index, methods):
        """The first token after a chain of `.method()` calls that keep the value."""
        tokens = self.tokens
        position = index + 1
        while (
            position + 3 < len(tokens)
            and tokens[position].text == "."
            and tokens[position + 1].text in methods
            and tokens[position + 2].text == "("
            and tokens[position + 3].text == ")"
        ):
            position += 4
        return position


def _hint(stream, frame, arg, both_sides):
    """The file name written in the other arguments of a call, if one is."""
    found = None
    for other in range(frame.arg_count()):
        if other == arg or (other > arg and not both_sides):
            continue
        first, last = frame.arg_range(other)
        for token in stream.strings_in(first, last):
            if PATH_LIKE.fullmatch(token.piece.value):
                found = token.piece.value
    return found


# -- Rust ---------------------------------------------------------------------------

_IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
_NUMBER = re.compile(r"[0-9][0-9A-Za-z_]*(?:\.[0-9][0-9A-Za-z_]*)?")
# Each format macro with the position of its format string.
RUST_FORMAT_MACROS = {
    **dict.fromkeys(
        "format format_args print println eprint eprintln panic unreachable todo"
        " unimplemented".split(),
        0,
    ),
    **dict.fromkeys("write writeln assert debug_assert".split(), 1),
    **dict.fromkeys("assert_eq assert_ne debug_assert_eq debug_assert_ne".split(), 2),
}
RUST_VALUE_METHODS = frozenset(
    {"to_owned", "to_string", "into", "as_bytes", "as_str", "as_ref", "to_vec"}
)
RUST_PUSH_METHODS = frozenset({"push_str", "insert_str"})
_RUST_SIMPLE = {
    "n": "\n",
    "r": "\r",
    "t": "\t",
    "\\": "\\",
    "0": "\0",
    "'": "'",
    '"': '"',
}


def _rust_string(text, at):
    """The Rust string literal starting at `at`, as a Token, or None."""
    position = at
    prefix = ""
    if text[position] in "bc" and text[position + 1 : position + 2] in ('"', "r"):
        prefix = text[position]
        position += 1
    if text[position] == '"':
        return _rust_normal(text, at, prefix, position)
    if text[position] == "r":
        hashes = position + 1
        while hashes < len(text) and text[hashes] == "#":
            hashes += 1
        if hashes < len(text) and text[hashes] == '"':
            count = hashes - position - 1
            close = text.find('"' + "#" * count, hashes + 1)
            if close < 0:
                return None
            value = text[hashes + 1 : close]
            end = close + 1 + count
            style = RustStyle(
                prefix, count, multiline="\n" in value, ascii=text[at:end].isascii()
            )
            piece = Piece(at, end, value, style, list(range(hashes + 1, close)))
            return Token("str", at, end, text[at:end], piece)
    return None


def _rust_normal(text, at, prefix, quote):
    value, offsets = [], []
    raw_breaks = escaped_breaks = 0
    position = quote + 1
    size = len(text)
    while True:
        if position >= size:
            return None
        character = text[position]
        if character == '"':
            break
        if character != "\\":
            if character == "\n":
                raw_breaks += 1
            value.append(character)
            offsets.append(position)
            position += 1
            continue
        escape = text[position + 1 : position + 2]
        if escape in _RUST_SIMPLE:
            if escape == "n":
                escaped_breaks += 1
            value.append(_RUST_SIMPLE[escape])
            offsets.append(position)
            position += 2
        elif escape == "x":
            value.append(chr(int(text[position + 2 : position + 4], 16)))
            offsets.append(position)
            position += 4
        elif escape == "u" and text.startswith("{", position + 2):
            close = text.index("}", position)
            value.append(chr(int(text[position + 3 : close].replace("_", ""), 16)))
            offsets.append(position)
            position = close + 1
        elif escape in ("\n", "\r"):
            # A line continuation skips the break and the next line's indent.
            position += 2
            while position < size and text[position] in " \t\r\n":
                position += 1
        else:
            return None
    end = position + 1
    style = RustStyle(
        prefix,
        -1,
        multiline=raw_breaks > escaped_breaks,
        ascii=text[at:end].isascii(),
    )
    piece = Piece(at, end, "".join(value), style, offsets)
    return Token("str", at, end, text[at:end], piece)


def _rust_block_comment(text, at):
    depth, position = 0, at
    while position < len(text):
        if text.startswith("/*", position):
            depth += 1
            position += 2
        elif text.startswith("*/", position):
            depth -= 1
            position += 2
            if depth == 0:
                return position
        else:
            position += 1
    return len(text)


def _rust_char(text, at):
    """The end of a char literal at `at` (after any `b`), or None for a lifetime."""
    if text.startswith("\\", at + 1):
        position = at + 2
        if text.startswith("u{", position):
            position = text.index("}", position) + 1
        elif text.startswith("x", position):
            position += 3
        else:
            position += 1
        return position + 1 if text.startswith("'", position) else None
    if at + 2 < len(text) and text[at + 2] == "'" and text[at + 1] != "\n":
        return at + 3
    return None


def rust_tokens(text):
    tokens = []
    position, size = 0, len(text)
    while position < size:
        character = text[position]
        if character in " \t\r\n":
            position += 1
            continue
        if text.startswith("//", position):
            stop = LINE_BREAK.search(text, position)
            position = size if stop is None else stop.start()
            continue
        if text.startswith("/*", position):
            position = _rust_block_comment(text, position)
            continue
        if character == '"' or (
            character in "bcr" and text[position + 1 : position + 2] in ('"', "#", "r")
        ):
            token = _rust_string(text, position)
            if token is not None:
                tokens.append(token)
                position = token.end
                continue
        if character == "'" or (
            character == "b" and text.startswith("'", position + 1)
        ):
            quote = position + (character == "b")
            stop = _rust_char(text, quote)
            if stop is not None:
                tokens.append(Token("other", position, stop, text[position:stop]))
                position = stop
                continue
            if character == "'":
                match = _IDENT.match(text, position + 1)
                stop = match.end() if match else position + 1
                tokens.append(Token("other", position, stop, text[position:stop]))
                position = stop
                continue
        if character.isalpha() or character == "_":
            match = _IDENT.match(text, position)
            if match is None:
                # A non-ASCII identifier character: one token per character.
                tokens.append(Token("other", position, position + 1, character))
                position += 1
                continue
            tokens.append(Token("ident", position, match.end(), match.group()))
            position = match.end()
            continue
        if character.isdigit():
            match = _NUMBER.match(text, position)
            tokens.append(Token("other", position, match.end(), match.group()))
            position = match.end()
            continue
        kind = (
            "open" if character in "([{" else "close" if character in ")]}" else "punct"
        )
        tokens.append(Token(kind, position, position + 1, character))
        position += 1
    return tokens


def _rust_opener(tokens, index):
    token = tokens[index]
    previous = tokens[index - 1] if index else None
    if (
        previous is not None
        and previous.text == "!"
        and index >= 2
        and tokens[index - 2].kind == "ident"
    ):
        return "macro", tokens[index - 2].text
    if token.text == "{":
        return "block", None
    if token.text == "(" and previous is not None and previous.kind == "ident":
        if index >= 2 and tokens[index - 2].text == ".":
            return "method", previous.text
        return "call", previous.text
    return "group", None


def _binding(stream, index):
    """(name, mutable) when a `let`, `const` or `static` binds the literal alone."""
    tokens = stream.tokens
    position = index - 1
    while position >= 0 and tokens[position].text not in (";", "{", "}"):
        position -= 1
    words = [token.text for token in tokens[position + 1 : index]]
    mutable = False
    if words[:1] == ["let"]:
        words = words[1:]
        if words[:1] == ["mut"]:
            mutable, words = True, words[1:]
    elif words[:1] in (["const"], ["static"]):
        words = words[1:]
    else:
        return None
    if not words or not _IDENT.fullmatch(words[0]) or "=" not in words:
        return None
    expression = words[words.index("=") + 1 :]
    if expression not in ([], ["&"], ["String", ":", ":", "from", "("]):
        return None
    return words[0], mutable


def _rust_group(stream, index):
    """The group of one literal token, read in its context."""
    tokens = stream.tokens
    token = tokens[index]
    piece = token.piece
    _, frame = stream.frame(index)
    arg = frame.arg_of(index) if frame is not None else None
    call = frame is not None and frame.kind in ("call", "method", "macro")
    group = Group("rust", [piece], token.start, token.end, scope=stream.scope(index))
    formatted = (
        call and frame.kind == "macro" and RUST_FORMAT_MACROS.get(frame.name) == arg
    )
    if (
        formatted
        and stream.lone(frame, arg, index, skip=())
        and re.search(r"[{}]", piece.value)
    ):
        # Doubled braces are text and single ones placeholders: a template.
        return _template(group, piece.value, format_skeleton(piece.value))
    after = stream.after_chain(index, RUST_VALUE_METHODS)
    previous = tokens[index - 1].text if index else ""
    before = (
        previous == "+"
        or (previous == "=" and index >= 2 and tokens[index - 2].text == "+")
        or (call and frame.name in RUST_PUSH_METHODS)
    )
    behind = after < len(tokens) and tokens[after].text == "+"
    if call and frame.kind == "macro" and frame.name == "concat":
        # A piece of concat! lacks the other arguments' document context.
        # Keep its readable text so a changing fragment is listed for manual work.
        return _template(group, piece.value)
    if before or behind:
        return _template(
            group, (HOLE if before else "") + piece.value + (HOLE if behind else "")
        )
    if call and frame.name in REPLACE_CALLS:
        group.role = "replace"
    elif call and frame.name in PROBE_CALLS:
        group.role = "probe"
    elif call and stream.lone(frame, arg, index, methods=RUST_VALUE_METHODS):
        group.name = _hint(stream, frame, arg, both_sides=False)
    return group


def rust_extraction(text):
    """Every string a Rust source spells with literals, as groups."""
    extraction = Extraction(text)
    tokens = rust_tokens(text)
    stream = Stream(tokens, _rust_opener)
    bindings = {}
    for index, token in enumerate(tokens):
        if token.kind != "str":
            continue
        group = _rust_group(stream, index)
        extraction.groups.append(group)
        bound = _binding(stream, index)
        if bound is not None and not group.interpolated:
            bindings.setdefault(bound[0], []).append((index, group, bound[1]))
    _rust_uses(stream, bindings)
    return extraction.ordered()


def _rust_uses(stream, bindings):
    """Read each use of a bound literal: a string a later statement extends is
    interpolated, and one written next to a file name takes that name."""
    tokens = stream.tokens
    hints = {}
    for index, token in enumerate(tokens):
        if token.kind != "ident" or token.text not in bindings:
            continue
        visible = [
            item
            for item in bindings[token.text]
            if item[0] < index and _visible(stream.scope(item[0]), stream.scope(index))
        ]
        if not visible:
            continue
        _, group, mutable = visible[-1]
        group.uses.append(stream.scope(index))
        following = [item.text for item in tokens[index + 1 : index + 3]]
        _, frame = stream.frame(index)
        extended = following in ([".", "push_str"], [".", "push"], ["+", "="]) or (
            frame is not None
            and frame.kind == "macro"
            and frame.name in ("write", "writeln")
            and frame.arg_of(index) == 0
        )
        if mutable and extended:
            if not group.interpolated:
                _template(group, group.text + HOLE)
            continue
        if frame is None or frame.kind not in ("call", "method", "macro"):
            continue
        arg = frame.arg_of(index)
        if stream.lone(frame, arg, index, methods=RUST_VALUE_METHODS):
            hint = _hint(stream, frame, arg, both_sides=False)
            if hint is not None:
                hints.setdefault(id(group), (group, set()))[1].add(hint)
    for group, found in hints.values():
        if len(found) == 1 and group.name is None and not group.interpolated:
            group.name = next(iter(found))


def _visible(declared, used):
    return used[: len(declared)] == declared


def _template(group, template, skeleton=None):
    """Mark a group as interpolated: listed with its template, never converted.

    The skeleton puts a plain word in each hole, so the template can be read as
    YAML to decide whether its conversion would change anything."""
    group.template = template
    group.skeleton = (
        skeleton if skeleton is not None else template.replace(HOLE, HOLE_WORD)
    )
    group.pieces = []
    return group


def format_skeleton(value):
    """A `format!` or `str.format` template with doubled braces undone and a
    word in each placeholder."""
    return re.sub(
        r"\{\{|\}\}|\{[^{}]*\}",
        lambda match: match.group()[0] if match.group() in ("{{", "}}") else HOLE_WORD,
        value,
    )


# -- Python -----------------------------------------------------------------------

_PY_PREFIX = re.compile(r"[A-Za-z]*")
PY_WRITES = frozenset({"write_text", "write_bytes"})


def _python_piece(first, last, token_text):
    prefix = _PY_PREFIX.match(token_text).group()
    body = token_text[len(prefix) :]
    quote = body[:3] if body[:3] in ('"""', "'''") else body[0]
    lowered = prefix.lower()
    style = PythonStyle(
        prefix=lowered.replace("r", ""),
        raw="r" in lowered,
        quote=quote,
        ascii=token_text.isascii(),
    )
    value = ast.literal_eval(token_text)
    if isinstance(value, bytes):
        value = value.decode("latin-1")
    # Offsets are approximate: a triple-quoted token's lines map to its lines.
    breaks = [first] + [
        first + match.end() for match in LINE_BREAK.finditer(token_text)
    ]
    offsets, line = [], 0
    for character in value:
        offsets.append(breaks[min(line, len(breaks) - 1)] if len(quote) == 3 else first)
        line += character == "\n"
    return Piece(first, last, value, style, offsets)


def _python_path(node, assignments, depth=0):
    """A file name an expression spells, directly or through its last assignment."""
    for inner in ast.walk(node):
        if (
            isinstance(inner, ast.Constant)
            and isinstance(inner.value, str)
            and PATH_LIKE.fullmatch(inner.value)
        ):
            return inner.value
    if depth == 0 and isinstance(node, (ast.Name, ast.Attribute)):
        assigned = assignments.get(ast.unparse(node))
        if assigned is not None and len(assigned) == 1:
            return _python_path(assigned[0], assignments, 1)
    return None


def _call_name(call):
    if isinstance(call.func, ast.Attribute):
        return call.func.attr
    if isinstance(call.func, ast.Name):
        return call.func.id
    return None


def python_extraction(text):
    """Every string a Python source spells with literals, as groups."""
    extraction = Extraction(text)
    try:
        tree = ast.parse(text)
        tokens = list(
            tokenize.generate_tokens(io.StringIO(text, newline=None).readline)
        )
    except (SyntaxError, tokenize.TokenError, ValueError):
        return extraction.ordered()
    lines = text.splitlines(keepends=True)
    starts = extraction.starts

    def offset(row, column, in_bytes=False):
        line = lines[row - 1] if row - 1 < len(lines) else ""
        if in_bytes and not line.isascii():
            column = len(line.encode("utf-8")[:column].decode("utf-8", "replace"))
        return starts[row - 1] + column

    strings = []
    for token in tokens:
        if token.type == tokenize.STRING:
            first, last = offset(*token.start), offset(*token.end)
            strings.append((first, last, text[first:last]))
    string_starts = [item[0] for item in strings]
    parents, scopes, assignments = {}, {}, {}
    for node in ast.walk(tree):
        for child in ast.iter_child_nodes(node):
            parents[child] = node
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            scopes[node] = len(scopes)
        if isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, (ast.Name, ast.Attribute)):
                    assignments.setdefault(ast.unparse(target), []).append(node.value)

    def scope(node):
        chain = []
        current = parents.get(node)
        while current is not None:
            if current in scopes:
                chain.append(scopes[current])
            current = parents.get(current)
        return tuple(reversed(chain))

    def span(node):
        return (
            offset(node.lineno, node.col_offset, True),
            offset(node.end_lineno, node.end_col_offset, True),
        )

    groups = {}
    for node in ast.walk(tree):
        holder = parents.get(node)
        if isinstance(node, ast.JoinedStr):
            if not isinstance(holder, (ast.JoinedStr, ast.FormattedValue)):
                template = "".join(
                    value.value
                    if isinstance(value, ast.Constant)
                    else "{" + ast.unparse(value.value) + "}"
                    for value in node.values
                )
                skeleton = "".join(
                    value.value if isinstance(value, ast.Constant) else HOLE_WORD
                    for value in node.values
                )
                group = Group("python", [], *span(node), scope=scope(node))
                extraction.groups.append(_template(group, template, skeleton))
            continue
        if not (
            isinstance(node, ast.Constant) and isinstance(node.value, (str, bytes))
        ):
            continue
        if isinstance(holder, (ast.JoinedStr, ast.FormattedValue)) or _docstring(
            node, parents
        ):
            continue
        start, end = span(node)
        pieces = []
        index = bisect.bisect_left(string_starts, start)
        while index < len(strings) and strings[index][1] <= end:
            pieces.append(_python_piece(*strings[index]))
            index += 1
        value = node.value
        if isinstance(value, bytes):
            value = value.decode("latin-1")
        if not pieces or "".join(piece.value for piece in pieces) != value:
            continue
        group = Group("python", pieces, start, end, scope=scope(node))
        groups[node] = group
        extraction.groups.append(group)
        _python_context(node, holder, parents, group, assignments)
    _python_uses(tree, groups, assignments, scope)
    return extraction.ordered()


def _python_context(node, holder, parents, group, assignments):
    """Read a literal's context: a template, a pattern, a probe or a write."""
    if isinstance(holder, ast.Attribute) and holder.attr == "format":
        _template(group, group.text, format_skeleton(group.text))
    elif isinstance(holder, ast.BinOp) and (
        isinstance(holder.op, ast.Add)
        or (isinstance(holder.op, ast.Mod) and holder.left is node)
    ):
        other = holder.right if holder.left is node else holder.left
        hole = "{" + ast.unparse(other) + "}"
        text = group.text
        if isinstance(holder.op, ast.Mod):
            skeleton = re.sub(
                r"%(?:\([^)]*\))?[-#0 +]*[0-9*]*(?:\.[0-9*]+)?[a-zA-Z%]",
                HOLE_WORD,
                text,
            )
            _template(group, text, skeleton)
        elif holder.left is node:
            _template(group, text + hole, text + HOLE_WORD)
        else:
            _template(group, hole + text, HOLE_WORD + text)
    elif (
        isinstance(holder, ast.Compare)
        and holder.left is node
        and any(isinstance(op, (ast.In, ast.NotIn)) for op in holder.ops)
    ):
        group.role = "probe"
    elif isinstance(holder, ast.Call) and node in holder.args:
        name = _call_name(holder)
        if name in REPLACE_CALLS:
            group.role = "replace"
        elif name in PROBE_CALLS:
            group.role = "probe"
        elif name in PY_WRITES and isinstance(holder.func, ast.Attribute):
            group.name = _python_path(holder.func.value, assignments)
        else:
            for earlier in holder.args[: holder.args.index(node)]:
                group.name = _python_path(earlier, {}) or group.name


def _python_uses(tree, groups, assignments, scope):
    """A literal bound to a name takes the file name the name is written to."""
    bound = {}
    for node in ast.walk(tree):
        if (
            isinstance(node, ast.Assign)
            and node.value in groups
            and len(node.targets) == 1
            and isinstance(node.targets[0], (ast.Name, ast.Attribute))
        ):
            bound.setdefault(ast.unparse(node.targets[0]), []).append(
                groups[node.value]
            )
    for node in ast.walk(tree):
        if isinstance(node, (ast.Name, ast.Attribute)) and isinstance(
            getattr(node, "ctx", None), ast.Load
        ):
            for group in bound.get(ast.unparse(node), ()):
                group.uses.append(scope(node))
    hints = {}
    for node in ast.walk(tree):
        if (
            isinstance(node, ast.Call)
            and _call_name(node) in PY_WRITES
            and isinstance(node.func, ast.Attribute)
            and node.args
            and isinstance(node.args[0], (ast.Name, ast.Attribute))
        ):
            key = ast.unparse(node.args[0])
            found = _python_path(node.func.value, assignments)
            if found and len(bound.get(key, ())) == 1:
                hints.setdefault(key, set()).add(found)
    for key, found in hints.items():
        group = bound[key][0]
        if len(found) == 1 and group.name is None and not group.interpolated:
            group.name = next(iter(found))


def _docstring(node, parents):
    holder = parents.get(node)
    if not isinstance(holder, ast.Expr):
        return False
    owner = parents.get(holder)
    return (
        isinstance(
            owner, (ast.Module, ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)
        )
        and bool(owner.body)
        and owner.body[0] is holder
    )


# -- R --------------------------------------------------------------------------------

_R_IDENT = re.compile(r"(?:[A-Za-z]|\.(?![0-9]))[A-Za-z0-9._]*")
_R_NUMBER = re.compile(
    r"0[xX][0-9A-Fa-f]+[Li]?|[0-9]*\.?[0-9]+(?:[eE][+-]?[0-9]+)?[Li]?"
)
_R_SIMPLE = dict(zip("nrtbafv\\'\"` ", "\n\r\t\b\a\f\v\\'\"` "))
R_INTERPOLATING = frozenset(
    {"paste0", "paste", "sprintf", "gettextf", "glue", "format"}
)


def _r_string(text, at):
    quote = text[at]
    value, offsets = [], []
    position = at + 1
    size = len(text)
    while True:
        if position >= size:
            return None
        character = text[position]
        if character == quote:
            break
        if character != "\\":
            value.append(character)
            offsets.append(position)
            position += 1
            continue
        escape = text[position + 1 : position + 2]
        if escape in _R_SIMPLE:
            value.append(_R_SIMPLE[escape])
            offsets.append(position)
            position += 2
            continue
        if escape == "x":
            match = re.match(r"[0-9A-Fa-f]{1,2}", text[position + 2 :])
            if not match:
                return None
            value.append(chr(int(match.group(), 16)))
            offsets.append(position)
            position += 2 + match.end()
            continue
        if escape in ("u", "U"):
            if text.startswith("{", position + 2):
                close = text.index("}", position)
                code = text[position + 3 : close]
                stop = close + 1
            else:
                match = re.match(
                    r"[0-9A-Fa-f]{1,%d}" % (4 if escape == "u" else 8),
                    text[position + 2 :],
                )
                if not match:
                    return None
                code = match.group()
                stop = position + 2 + match.end()
            value.append(chr(int(code, 16)))
            offsets.append(position)
            position = stop
            continue
        match = re.match(r"[0-7]{1,3}", text[position + 1 :])
        if match:
            value.append(chr(int(match.group(), 8)))
            offsets.append(position)
            position += 1 + match.end()
            continue
        return None
    end = position + 1
    style = RStyle(quote, ascii=text[at:end].isascii())
    piece = Piece(at, end, "".join(value), style, offsets)
    return Token("str", at, end, text[at:end], piece)


def r_tokens(text):
    tokens = []
    position, size = 0, len(text)
    while position < size:
        character = text[position]
        if character in " \t\r\n":
            position += 1
            continue
        if character == "#":
            stop = LINE_BREAK.search(text, position)
            position = size if stop is None else stop.start()
            continue
        if character in "\"'":
            token = _r_string(text, position)
            if token is None:
                tokens.append(Token("other", position, size, text[position:]))
                break
            tokens.append(token)
            position = token.end
            continue
        if character == "`":
            stop = text.find("`", position + 1)
            stop = size if stop < 0 else stop + 1
            tokens.append(Token("ident", position, stop, text[position:stop]))
            position = stop
            continue
        if character in "rR" and text[position + 1 : position + 2] in ('"', "'"):
            raw = re.match(r"[rR]([\"'])(-*)([(\[{])", text[position:])
            if raw:
                closer = {"(": ")", "[": "]", "{": "}"}[raw.group(3)]
                closer += raw.group(2) + raw.group(1)
                stop = text.find(closer, position + raw.end())
                stop = size if stop < 0 else stop + len(closer)
                tokens.append(Token("other", position, stop, text[position:stop]))
                position = stop
                continue
        match = _R_IDENT.match(text, position)
        if match:
            tokens.append(Token("ident", position, match.end(), match.group()))
            position = match.end()
            continue
        match = _R_NUMBER.match(text, position)
        if match and match.end() > position:
            tokens.append(Token("other", position, match.end(), match.group()))
            position = match.end()
            continue
        if text.startswith("<-", position):
            tokens.append(Token("punct", position, position + 2, "<-"))
            position += 2
            continue
        kind = (
            "open" if character in "([{" else "close" if character in ")]}" else "punct"
        )
        tokens.append(Token(kind, position, position + 1, character))
        position += 1
    return tokens


def _r_opener(tokens, index):
    token = tokens[index]
    previous = tokens[index - 1] if index else None
    if token.text == "{":
        return "block", None
    if token.text == "(" and previous is not None and previous.kind == "ident":
        return "call", previous.text
    return "group", None


def _r_vector(stream, frame):
    """A `c(...)` of literals as one document of lines, or its template."""
    tokens = stream.tokens
    parts = []
    for arg in range(frame.arg_count()):
        first, last = frame.arg_range(arg)
        if last > first and tokens[first + 1].text == "=":
            return None
        parts.append(
            tokens[first] if first == last and tokens[first].kind == "str" else None
        )
    literal = [part for part in parts if part is not None]
    if not literal or any("\n" in part.piece.value for part in literal):
        return None
    group = Group(
        "r",
        [part.piece for part in literal],
        tokens[frame.open - 1].start,
        tokens[frame.close].end,
        lines=True,
        scope=stream.scope(frame.open),
    )
    if not all(parts):
        _template(
            group,
            "".join((part.piece.value if part else HOLE) + "\n" for part in parts),
        )
    return group


def _r_paste(stream, frame):
    """A `paste0`, `paste` or `sprintf` with a literal part: always a template."""
    tokens = stream.tokens
    parts = []
    for arg in range(frame.arg_count()):
        first, last = frame.arg_range(arg)
        if last > first and tokens[first + 1].text == "=":
            continue
        parts.append(
            tokens[first] if first == last and tokens[first].kind == "str" else None
        )
    if not any(parts):
        return None
    group = Group(
        "r",
        [],
        tokens[frame.open - 1].start,
        tokens[frame.close].end,
        scope=stream.scope(frame.open),
    )
    return _template(
        group, "".join(part.piece.value if part else HOLE for part in parts)
    )


def r_extraction(text):
    """Every string an R source spells with literals, as groups."""
    extraction = Extraction(text)
    tokens = r_tokens(text)
    stream = Stream(tokens, _r_opener)

    def hint(frame_index, arg):
        """A file name in the other arguments of the call, or of its callers."""
        for _ in range(3):
            frame = stream.frames[frame_index]
            if frame.kind == "call":
                found = _hint(stream, frame, arg, both_sides=True)
                if found:
                    return found
            if frame.parent is None:
                return None
            arg = stream.frames[frame.parent].arg_of(frame.open)
            frame_index = frame.parent
        return None

    used = set()
    for frame_index, frame in enumerate(stream.frames):
        if frame.kind != "call" or frame.close is None:
            continue
        if frame.name == "c":
            group = _r_vector(stream, frame)
        elif frame.name in R_INTERPOLATING:
            group = _r_paste(stream, frame)
        else:
            continue
        if group is None:
            continue
        # Every string inside the call belongs to the group, a nested
        # template's parts included.
        used.update(
            index
            for index in range(frame.open, frame.close)
            if tokens[index].kind == "str"
        )
        if not group.interpolated and frame.parent is not None:
            group.name = hint(
                frame.parent, stream.frames[frame.parent].arg_of(frame.open)
            )
        extraction.groups.append(group)

    for index, token in enumerate(tokens):
        if token.kind != "str" or index in used:
            continue
        frame_index, frame = stream.frame(index)
        group = Group(
            "r", [token.piece], token.start, token.end, scope=stream.scope(index)
        )
        if frame is not None and frame.kind == "call":
            if frame.name in REPLACE_CALLS:
                group.role = "replace"
            elif frame.name in PROBE_CALLS:
                group.role = "probe"
            else:
                group.name = hint(frame_index, frame.arg_of(index))
        extraction.groups.append(group)
    return extraction.ordered()
