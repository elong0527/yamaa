"""The one R022 portable-contract binding every consumer reads through.

R022 pins no engine: the rule text plus the conformance fixtures are the
contract (version 2.0.0), and each consumer normalizes its host library to
it (R022-30 through R022-34). This module is the Python consumer. It checks
every pattern against the portable grammar, translates the pattern onto the
standard library `re` module, and compiles with `re.ASCII` so `\\d`, `\\w`,
and `\\b` keep their ASCII meaning however the host behaves. Keeping the
binding in one module means the schema `pattern` descriptor, the `matches`
verification, and `str_extract` cannot drift into three dialects.
"""

from __future__ import annotations

import re
import string
from functools import lru_cache
from typing import Final, TypeAlias

# R022-5 versions the portable contract with the rule text and the fixtures.
REGEX_CONTRACT_VERSION: Final = "2.0.0"
# The host library this consumer normalizes, for a failure to report.
REGEX_HOST_LIBRARY: Final = "re"

# The compiled pattern type, so a consumer need not import the host library.
Regex: TypeAlias = re.Pattern[str]

# R022-31: the ECMA-262 `WhiteSpace` plus `LineTerminator` set, spelled as
# `re` source. `U+0085` is not in the set, even though some host libraries
# include it in `\\s`.
_WHITESPACE_CLASS: Final = (
    "[\\t\\n\\v\\f\\r \\u00a0\\u1680\\u2000-\\u200a\\u2028\\u2029"
    "\\u202f\\u205f\\u3000\\ufeff]"
)
_WHITESPACE_NEGATION: Final = "[^" + _WHITESPACE_CLASS[1:]
# Inside a character class the same set is spelled without the brackets.
_WHITESPACE_IN_CLASS: Final = (
    "\\t\\n\\v\\f\\r \\u00a0\\u1680\\u2000-\\u200a\\u2028\\u2029"
    "\\u202f\\u205f\\u3000\\ufeff"
)
# R022-32: `.` matches every scalar except these four line terminators.
_DOT_REPLACEMENT: Final = "[^\\r\\n\\u2028\\u2029]"

# Escapes the contract admits after a backslash besides the translations
# above: the single-scalar classes and anchors, the control escapes, and
# NUL. Hex, code point, punctuation, and backreference escapes are checked
# where they are read.
_SIMPLE_ESCAPES: Final = frozenset("dDsSwWbBnrtfv0")
# `\\` plus one of these is an ordinary scalar (R022-34).
_ESCAPABLE_PUNCTUATION: Final = frozenset(string.punctuation)

_HEX_DIGITS: Final = frozenset("0123456789abcdefABCDEF")


class NoMatch:
    """The pattern matched nowhere in the subject.

    R022-23 keeps an empty match distinct from no match, and R019 keeps the
    empty string distinct from missing, so this is its own value rather than
    either of them.
    """

    __slots__ = ()

    def __repr__(self) -> str:
        return "NO_MATCH"


NO_MATCH: Final = NoMatch()


class RegexError(ValueError):
    """A pattern the portable contract rejects."""

    condition = "invalid_regex"
    requirement = "R022-27"

    def __init__(self, pattern: str, reason: str) -> None:
        super().__init__(f"the R022 contract rejects {pattern!r}: {reason}")
        self.pattern = pattern
        self.reason = reason


class RegexGroupError(ValueError):
    """A `str_extract.group` the pattern does not declare."""

    condition = "regex_group_out_of_range"
    requirement = "R022-28"

    def __init__(self, pattern: str, group: int, group_count: int) -> None:
        super().__init__(
            f"group {group} is outside the {group_count} capturing group(s) "
            f"{pattern!r} declares"
        )
        self.pattern = pattern
        self.group = group
        self.group_count = group_count


def _parse_quantifier(pattern: str, index: int) -> int | None:
    """Return the end of the `{n}`, `{n,}`, or `{n,m}` at `index`, if any."""
    end = index + 1
    length = len(pattern)
    digits = 0
    while end < length and pattern[end].isdigit() and pattern[end].isascii():
        digits += 1
        end += 1
    if digits == 0:
        return None
    if end < length and pattern[end] == ",":
        end += 1
        while end < length and pattern[end].isdigit() and pattern[end].isascii():
            end += 1
    if end >= length or pattern[end] != "}":
        return None
    return end + 1


def _find_group_end(pattern: str, index: int) -> int:
    """Return the index of the `)` closing the group opening at `index`."""
    depth = 0
    cursor = index
    length = len(pattern)
    in_class = False
    while cursor < length:
        character = pattern[cursor]
        if character == "\\":
            cursor += 2
            continue
        if in_class:
            if character == "]":
                in_class = False
            cursor += 1
            continue
        if character == "[":
            in_class = True
            cursor += 1
            continue
        if character == "(":
            depth += 1
        elif character == ")":
            depth -= 1
            if depth == 0:
                return cursor
        cursor += 1
    raise RegexError(pattern, "an opening parenthesis is never closed")


def _check_fixed_length_lookbehind(pattern: str, start: int, end: int) -> None:
    """Reject a lookbehind body spanning `start` to `end` that can vary."""
    depth = 0
    cursor = start
    in_class = False
    while cursor < end:
        character = pattern[cursor]
        if character == "\\":
            cursor += 2
            continue
        if in_class:
            if character == "]":
                in_class = False
            cursor += 1
            continue
        if character == "[":
            in_class = True
            cursor += 1
            continue
        if character == "(":
            rest = pattern[cursor + 1 : end]
            if rest.startswith(("?=", "?!", "?<=", "?<!")):
                cursor = _find_group_end(pattern, cursor) + 1
                continue
            depth += 1
            cursor += 1
            continue
        if character == ")":
            depth -= 1
            cursor += 1
            continue
        if character == "|":
            raise RegexError(
                pattern, "a lookbehind whose length can vary is not allowed"
            )
        if character in "*+":
            raise RegexError(
                pattern, "a lookbehind whose length can vary is not allowed"
            )
        if character == "?":
            previous = pattern[cursor - 1] if cursor > 0 else ""
            if previous not in "(?*+":
                raise RegexError(
                    pattern,
                    "a lookbehind whose length can vary is not allowed",
                )
            cursor += 1
            continue
        if character == "{":
            close = _parse_quantifier(pattern, cursor)
            if close is not None and close <= end:
                bounds = pattern[cursor + 1 : close - 1]
                minimum, comma, maximum = bounds.partition(",")
                if comma and maximum != minimum or comma and not maximum:
                    raise RegexError(
                        pattern,
                        "a lookbehind whose length can vary is not allowed",
                    )
                cursor = close
                if cursor < end and pattern[cursor] == "?":
                    cursor += 1
                continue
            cursor += 1
            continue
        cursor += 1


def _expand_code_point(pattern: str, index: int) -> tuple[str, int]:
    """Expand the `\\u{...}` escape at `index` to the scalar it names."""
    cursor = index + 3
    length = len(pattern)
    digits = 0
    while cursor < length and pattern[cursor] in _HEX_DIGITS:
        digits += 1
        cursor += 1
    if digits == 0 or digits > 6 or cursor >= length or pattern[cursor] != "}":
        raise RegexError(pattern, "a malformed code point escape")
    scalar = int(pattern[index + 3 : cursor], 16)
    if scalar > 0x10FFFF or 0xD800 <= scalar <= 0xDFFF:
        raise RegexError(pattern, "a malformed code point escape")
    character = chr(scalar)
    if character in _ESCAPABLE_PUNCTUATION:
        return "\\" + character, cursor + 1
    return character, cursor + 1


def _normalize(pattern: str) -> str:
    """Check `pattern` against the portable grammar and translate it.

    The result is `re` source with the contract semantics: ASCII-only
    classes (R022-30, via the `re.ASCII` flag at compile time), the
    ECMA-262 whitespace set (R022-31), scalar dot and end-only `$`
    (R022-32), and expanded code point escapes (R022-33). Anything the
    grammar excludes (R022-34) fails here with `RegexError`, before the
    host library ever sees the pattern.
    """
    output: list[str] = []
    index = 0
    length = len(pattern)
    in_class = False
    # Whether the previous atom can take a quantifier. Assertions, an
    # opening parenthesis, an alternation bar, and a quantifier itself
    # leave nothing to repeat.
    last_atom = False
    # Whether each open group is a capturing atom. Lookaround opens
    # (`?=`, `?!`, `?<=`, `?<!`) are assertions: quantifying one is a
    # syntax error, so a closing parenthesis restores no atom.
    group_stack: list[bool] = []
    while index < length:
        character = pattern[index]
        if character == "\\":
            if index + 1 >= length:
                raise RegexError(pattern, "a trailing backslash escapes nothing")
            escaped = pattern[index + 1]
            if escaped in _SIMPLE_ESCAPES:
                if escaped == "s":
                    output.append(
                        _WHITESPACE_IN_CLASS if in_class else _WHITESPACE_CLASS
                    )
                    last_atom = True
                elif escaped == "S" and not in_class:
                    output.append(_WHITESPACE_NEGATION)
                    last_atom = True
                elif escaped == "S":
                    output.append("\\S")
                    last_atom = True
                elif escaped in "bB":
                    if in_class and escaped == "B":
                        raise RegexError(pattern, "a malformed escape")
                    output.append(pattern[index : index + 2])
                    # `[\b]` is the backspace scalar in both the contract
                    # and the host library; outside a class both are
                    # zero-width assertions.
                    last_atom = in_class
                else:
                    output.append(pattern[index : index + 2])
                    last_atom = True
                index += 2
                continue
            if escaped == "x":
                digits = pattern[index + 2 : index + 4]
                if len(digits) != 2 or any(
                    digit not in _HEX_DIGITS for digit in digits
                ):
                    raise RegexError(pattern, "a malformed escape")
                output.append(pattern[index : index + 4])
                last_atom = True
                index += 4
                continue
            if escaped == "u":
                if pattern.startswith("u{", index + 1):
                    expanded, index = _expand_code_point(pattern, index)
                    if (
                        len(expanded) == 1
                        and expanded.isdigit()
                        and expanded.isascii()
                        and re.search(r"\\\d+$", "".join(output)) is not None
                    ):
                        # A literal digit would merge with a backreference
                        # the output ends with (`\1` plus `2` reads as group
                        # 12), so spell it as a hex escape instead. The
                        # spelling differs; the scalar it matches does not.
                        expanded = f"\\x{ord(expanded):02x}"
                    output.append(expanded)
                    last_atom = True
                    continue
                digits = pattern[index + 2 : index + 6]
                if len(digits) != 4 or any(
                    digit not in _HEX_DIGITS for digit in digits
                ):
                    raise RegexError(pattern, "a malformed escape")
                output.append(pattern[index : index + 6])
                last_atom = True
                index += 6
                continue
            if escaped == "p" or escaped == "P":
                raise RegexError(pattern, "a property escape is not allowed")
            if escaped.isdigit() and escaped.isascii():
                if escaped == "0":
                    if index + 2 < length and pattern[index + 2].isdigit():
                        raise RegexError(pattern, "a malformed escape")
                    output.append("\\x00")
                    last_atom = True
                    index += 2
                    continue
                if in_class:
                    raise RegexError(pattern, "a malformed escape")
                cursor = index + 1
                while (
                    cursor < length
                    and pattern[cursor].isdigit()
                    and pattern[cursor].isascii()
                ):
                    cursor += 1
                output.append(pattern[index:cursor])
                last_atom = True
                index = cursor
                continue
            if escaped in _ESCAPABLE_PUNCTUATION:
                output.append(pattern[index : index + 2])
                last_atom = True
                index += 2
                continue
            raise RegexError(pattern, "a malformed escape")
        if in_class:
            output.append(character)
            if character == "]":
                in_class = False
                last_atom = True
            index += 1
            continue
        if character == "[":
            in_class = True
            output.append(character)
            last_atom = False
            index += 1
            continue
        if character == ".":
            output.append(_DOT_REPLACEMENT)
            last_atom = True
            index += 1
            continue
        if character == "$":
            output.append("\\Z")
            last_atom = False
            index += 1
            continue
        if character == "^":
            output.append(character)
            last_atom = False
            index += 1
            continue
        if character == "(":
            rest = pattern[index + 1 :]
            if rest.startswith("?P<"):
                raise RegexError(pattern, "a (?P<name> group is not allowed")
            if rest.startswith(("?<=", "?<!")):
                end = _find_group_end(pattern, index)
                _check_fixed_length_lookbehind(pattern, index + 4, end)
                output.append(pattern[index : index + 4])
                index += 4
                last_atom = False
                group_stack.append(False)
                continue
            if rest.startswith("?<"):
                close = rest.find(">", 2)
                if close < 0:
                    raise RegexError(pattern, "a malformed group opening")
                name = rest[2:close]
                if not name:
                    raise RegexError(pattern, "a malformed group opening")
                # The host library spells a named group `(?P<name>`; the
                # spelling differs, the numbering does not.
                output.append(f"(?P<{name}>")
                index += close + 2
                last_atom = False
                group_stack.append(True)
                continue
            if rest.startswith("?"):
                if rest.startswith(("?:", "?=", "?!")):
                    output.append(pattern[index : index + 3])
                    index += 3
                    last_atom = False
                    group_stack.append(rest.startswith("?:"))
                    continue
                if rest[1:2] in ("i", "m", "s", "x", "-"):
                    raise RegexError(pattern, "an inline flag group is not allowed")
                raise RegexError(
                    pattern, "a group extension the contract does not allow"
                )
            output.append(character)
            last_atom = False
            group_stack.append(True)
            index += 1
            continue
        if character in "*+":
            if not last_atom:
                raise RegexError(pattern, "a quantifier repeats nothing")
            output.append(character)
            last_atom = False
            index += 1
            continue
        if character == "?":
            previous = pattern[index - 1] if index > 0 else ""
            if previous in "*+?}":
                output.append(character)
                last_atom = False
                index += 1
                continue
            if not last_atom:
                raise RegexError(pattern, "a quantifier repeats nothing")
            output.append(character)
            last_atom = False
            index += 1
            continue
        if character == "{":
            close = _parse_quantifier(pattern, index)
            if close is None or not last_atom:
                raise RegexError(pattern, "a malformed quantifier")
            output.append(pattern[index:close])
            last_atom = False
            index = close
            continue
        if character == "}":
            raise RegexError(pattern, "a malformed quantifier")
        if character == ")":
            output.append(character)
            last_atom = group_stack.pop() if group_stack else True
            index += 1
            continue
        if character == "|":
            output.append(character)
            last_atom = False
            index += 1
            continue
        output.append(character)
        last_atom = True
        index += 1
    if in_class:
        raise RegexError(pattern, "a character class is never closed")
    return "".join(output)


@lru_cache(maxsize=512)
def _compile_translated(translated: str) -> re.Pattern[str]:
    """Compile already-normalized source with the contract flag set."""
    return re.compile(translated, re.ASCII)


@lru_cache(maxsize=512)
def compile_pattern(pattern: str) -> re.Pattern[str]:
    """Compile one pattern source through the portable contract (R022-30..34)."""
    try:
        return _compile_translated(_normalize(pattern))
    except re.error as error:
        raise RegexError(pattern, str(error)) from error


def full_match(pattern: str, subject: str) -> bool:
    """Return whether an R006 `pattern` descriptor accepts the whole value."""
    try:
        translated = f"^(?:{_normalize(pattern)})\\Z"
    except re.error as error:  # pragma: no cover - _normalize raises first
        raise RegexError(pattern, str(error)) from error
    try:
        return _compile_translated(translated).search(subject) is not None
    except re.error as error:
        raise RegexError(pattern, str(error)) from error


def search(pattern: str, subject: str) -> bool:
    """Return whether R009's `matches` finds the pattern anywhere."""
    return compile_pattern(pattern).search(subject) is not None


@lru_cache(maxsize=512)
def capture_group_count(pattern: str) -> int:
    """Count the capturing groups an accepted pattern declares (R022-20).

    The host library answers an out-of-range group index exactly as it
    answers a group the match did not enter, so the count is read from the
    pattern source the contract has already accepted.
    """
    compile_pattern(pattern)
    count = 0
    index = 0
    length = len(pattern)
    in_class = False
    while index < length:
        character = pattern[index]
        if character == "\\":
            index += 2
            continue
        if in_class:
            if character == "]":
                in_class = False
            index += 1
            continue
        if character == "[":
            in_class = True
            index += 1
            continue
        if character == "(":
            if not pattern.startswith("(?", index):
                count += 1
            else:
                rest = pattern[index + 2 :]
                if rest.startswith("<") and not rest.startswith(("<=", "<!")):
                    count += 1
        index += 1
    return count


def regex_extract(pattern: str, subject: str, group: int = 0) -> object:
    """Return the R022 `str_extract` result for one pattern and subject.

    ``NO_MATCH`` when the pattern matched nowhere, ``None`` when the match did
    not enter the requested group, and the matched text otherwise.
    """
    declared = capture_group_count(pattern)
    if type(group) is not int or group < 0 or group > declared:
        raise RegexGroupError(pattern, group, declared)
    match = compile_pattern(pattern).search(subject)
    if match is None:
        return NO_MATCH
    return match.group(group)
