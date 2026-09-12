"""The one R022 regular-expression binding every consumer reads through.

R022-4 pins the engine and its version, R022-6 pins the flag set, and R022-29
forbids falling back to a host library. Keeping the binding in one module
means the schema `pattern` descriptor, the `matches` verification, and
`str_extract` cannot drift into three dialects.
"""

from __future__ import annotations

from functools import lru_cache
from typing import Final, TypeAlias

import regress

# R022-3 through R022-6 pin one engine, one version, and one flag set. A
# consumer that cannot provide them fails rather than reading a pattern with
# a host library.
REGEX_ENGINE_CRATE: Final = "regress"
REGEX_ENGINE_CRATE_VERSION: Final = "0.10.4"
REGEX_ENGINE_DISTRIBUTION_VERSION: Final = "2025.10.1"
REGEX_FLAGS: Final = "u"

# The compiled pattern type, so a consumer need not import the engine.
Regex: TypeAlias = regress.Regex


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
    """The pinned engine rejects a pattern."""

    condition = "invalid_regex"
    requirement = "R022-27"

    def __init__(self, pattern: str, reason: str) -> None:
        super().__init__(f"the R022 engine rejects {pattern!r}: {reason}")
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


@lru_cache(maxsize=512)
def compile_pattern(pattern: str) -> regress.Regex:
    """Compile one pattern source with the engine and flags R022 pins."""
    try:
        return regress.Regex(pattern, REGEX_FLAGS)
    except regress.RegressError as error:
        raise RegexError(pattern, str(error)) from error


def full_match(pattern: str, subject: str) -> bool:
    """Return whether an R006 `pattern` descriptor accepts the whole value."""
    return compile_pattern(f"^(?:{pattern})$").find(subject) is not None


def search(pattern: str, subject: str) -> bool:
    """Return whether R009's `matches` finds the pattern anywhere."""
    return compile_pattern(pattern).find(subject) is not None


@lru_cache(maxsize=512)
def capture_group_count(pattern: str) -> int:
    """Count the capturing groups an accepted pattern declares (R022-20).

    The engine reports no count, and it answers an out-of-range group index
    exactly as it answers a group the match did not enter, so the count is
    read from the pattern source the engine has already accepted.
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
    match = compile_pattern(pattern).find(subject)
    if match is None:
        return NO_MATCH
    span = match.group(group)
    if span is None:
        return None
    # The engine reports UTF-8 byte offsets into the subject.
    return subject.encode("utf-8")[span.start : span.stop].decode("utf-8")
