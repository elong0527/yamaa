"""The one reader of Rust source that the repository's tools share.

``rust_tokens`` finds every comment, character literal and string literal of
Rust source, as the Rust reference's lexical grammar defines them: line and
nested block comments, character and byte literals with their escapes, and
plain, byte, C and raw strings. The guards under ``tools/validation`` and the
generators under ``tools/packaging`` all read Rust through it, so a bracket
or a comment marker inside a literal never counts as code in one tool and as
text in another.
"""

import re

RAW_START = re.compile(r'(?:br|cr|r)(#*)"')
CHARACTER = re.compile(r"'(?:[^'\\\r\n]|\\(?:u\{[0-9a-fA-F_]+\}|[^\r\n]))'")
SPACE = re.compile(r"\s*")
LITERAL_STARTS = frozenset("\"'bcr")
ESCAPES = {
    "n": "\n",
    "r": "\r",
    "t": "\t",
    "0": "\0",
    '"': '"',
    "'": "'",
    "\\": "\\",
}


def decode_rust_string(source: str) -> str:
    """Decode ordinary Rust escapes without changing unescaped Unicode."""
    result = []
    offset = 0
    while offset < len(source):
        if source[offset] != "\\" or offset + 1 == len(source):
            result.append(source[offset])
            offset += 1
            continue
        offset += 1
        escaped = source[offset]
        if escaped in ESCAPES:
            result.append(ESCAPES[escaped])
            offset += 1
        elif escaped == "x" and re.fullmatch(
            r"[0-9a-fA-F]{2}", source[offset + 1 : offset + 3]
        ):
            result.append(chr(int(source[offset + 1 : offset + 3], 16)))
            offset += 3
        elif escaped == "u" and source[offset + 1 : offset + 2] == "{":
            end = source.find("}", offset + 2)
            if end < 0:
                result.append(escaped)
                offset += 1
                continue
            try:
                result.append(chr(int(source[offset + 2 : end].replace("_", ""), 16)))
            except ValueError:
                result.append(source[offset : end + 1])
            offset = end + 1
        elif escaped in "\r\n":
            while offset < len(source) and source[offset].isspace():
                offset += 1
        else:
            result.append(escaped)
            offset += 1
    return "".join(result)


def literal(source: str, offset: int) -> tuple[str, int, str | None] | None:
    """The character or string literal that starts at ``offset``, if any.

    The result is ``(kind, end, text)``: ``kind`` is ``character`` or
    ``string``, ``source[offset:end]`` is the literal and ``text`` is a
    string's decoded value (None for a character). A byte or C string is
    found from its quote. An unterminated string is a ValueError.
    """
    first = source[offset]
    if first == "'":
        character = CHARACTER.match(source, offset)
        return None if character is None else ("character", character.end(), None)
    if first == '"':
        end = offset + 1
        while end < len(source) and source[end] != '"':
            end += 2 if source[end] == "\\" else 1
        if end >= len(source):
            raise ValueError("unterminated string literal in Rust source")
        return "string", end + 1, decode_rust_string(source[offset + 1 : end])
    raw = RAW_START.match(source, offset)
    if raw is None or (
        offset and (source[offset - 1].isalnum() or source[offset - 1] == "_")
    ):
        return None
    delimiter = '"' + raw[1]
    end = source.find(delimiter, raw.end())
    if end < 0:
        raise ValueError("unterminated raw string literal in Rust source")
    return "string", end + len(delimiter), source[raw.end() : end]


def literal_end(source: str, offset: int) -> int | None:
    """The offset just past the literal that starts at ``offset``, else None."""
    if source[offset] not in LITERAL_STARTS:
        return None
    found = literal(source, offset)
    return None if found is None else found[1]


def rust_tokens(source: str):
    """Yield every comment, character and string literal of Rust source.

    A token is ``(kind, start, end, line, text)``: ``kind`` is ``comment``,
    ``character`` or ``string``, ``source[start:end]`` is the whole token,
    ``line`` is its first source line and ``text`` is a string's decoded value
    (None otherwise). An unterminated string ends the scan.
    """
    offset = 0
    line = 1
    while offset < len(source):
        if source.startswith("//", offset):
            end = source.find("\n", offset)
            end = len(source) if end < 0 else end
            yield "comment", offset, end, line, None
            offset = end
            continue
        if source.startswith("/*", offset):
            start, first = offset, line
            depth = 1
            offset += 2
            while offset < len(source) and depth:
                if source.startswith("/*", offset):
                    depth += 1
                    offset += 2
                elif source.startswith("*/", offset):
                    depth -= 1
                    offset += 2
                else:
                    line += source[offset] == "\n"
                    offset += 1
            yield "comment", start, offset, first, None
            continue
        try:
            found = (
                literal(source, offset) if source[offset] in LITERAL_STARTS else None
            )
        except ValueError:
            return
        if found is not None:
            kind, end, text = found
            yield kind, offset, end, line, text
            line += source[offset:end].count("\n")
            offset = end
            continue
        line += source[offset] == "\n"
        offset += 1


def blank(source: str, tokens: list, kinds: set[str]) -> str:
    """The source with every token of ``kinds`` spaced out, lines kept."""
    pieces = []
    offset = 0
    for kind, start, end, _, _ in tokens:
        if kind in kinds:
            pieces.append(source[offset:start])
            pieces.append(re.sub(r"[^\n]", " ", source[start:end]))
            offset = end
    pieces.append(source[offset:])
    return "".join(pieces)


def strip_comments(source: str) -> str:
    """The source with its comments spaced out and its literals kept."""
    return blank(source, list(rust_tokens(source)), {"comment"})


def strip_literals(source: str) -> str:
    """The source with its comments and literals spaced out, so only real
    brackets and keywords remain."""
    return blank(source, list(rust_tokens(source)), {"comment", "character", "string"})


def balanced(code: str, start: int, opening: str, closing: str) -> int | None:
    """The offset just past the bracket that closes the one at ``start``.

    A literal in ``code`` is skipped whole, so its brackets never count.
    """
    depth = 0
    index = start
    while index < len(code):
        character = code[index]
        if character == opening:
            depth += 1
        elif character == closing:
            depth -= 1
            if not depth:
                return index + 1
        elif character in LITERAL_STARTS:
            end = literal_end(code, index)
            if end is not None:
                index = end
                continue
        index += 1
    return None


def closed(code: str, start: int, opening: str, closing: str) -> int:
    """As ``balanced``, for a reader that refuses an unclosed bracket."""
    end = balanced(code, start, opening, closing)
    if end is None:
        raise ValueError(f"unclosed {opening!r} in Rust source")
    return end


def is_test_gate(attribute: str) -> bool:
    """Whether one attribute compiles its item for tests only: exactly
    ``#[cfg(test)]``, or ``test`` as a top-level operand of ``#[cfg(all(...))]``.
    ``#[cfg(not(test))]`` and feature names that contain ``test`` do not count.
    """
    body = re.sub(r"\s+", "", attribute)
    if body == "#[cfg(test)]":
        return True
    operands = re.fullmatch(r"#\[cfg\(all\((.*)\)\)\]", body)
    if operands is None:
        return False
    depth = 0
    operand = ""
    for character in operands[1] + ",":
        if character == "," and not depth:
            if operand == "test":
                return True
            operand = ""
            continue
        depth += (character == "(") - (character == ")")
        operand += character
    return False


def cfg_test_items(code: str) -> list[tuple[int, int]]:
    """The offsets of every item that a test gate compiles for tests only.

    ``code`` is Rust source with its comments and literals spaced out, so only
    real brackets count. A gated item follows its attributes and ends at its
    closing brace or, without a body, at its semicolon.
    """
    regions = []
    offset = 0
    while (start := code.find("#[", offset)) >= 0:
        end = balanced(code, start + 1, "[", "]")
        if end is None:
            break
        offset = end
        if not is_test_gate(code[start:end]):
            continue
        item = SPACE.match(code, end).end()
        while code.startswith("#[", item):
            item = SPACE.match(
                code, balanced(code, item + 1, "[", "]") or len(code)
            ).end()
        depth = 0
        stop = len(code)
        for index in range(item, len(code)):
            character = code[index]
            if character in "([":
                depth += 1
            elif character in ")]":
                depth -= 1
            elif character == ";" and not depth:
                stop = index + 1
                break
            elif character == "{" and not depth:
                stop = balanced(code, index, "{", "}") or len(code)
                break
        regions.append((start, stop))
        offset = stop
    return regions
