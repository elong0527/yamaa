"""The two ASCII scalar substitutions R019 admits.

R019-13 fixes these exactly: they preserve scalar count, have no context
rule, language tailoring, or Unicode-version dependency, and a host casing
routine conforms only when it returns the same result for every input.
R019-15's case-insensitive mapping fold is the same upward substitution.
"""

from __future__ import annotations


def ascii_upper(value: str) -> str:
    """Replace U+0061..U+007A with U+0041..U+005A and keep every other scalar."""
    return "".join(
        chr(ord(character) - 32) if "a" <= character <= "z" else character
        for character in value
    )


def ascii_lower(value: str) -> str:
    """Replace U+0041..U+005A with U+0061..U+007A and keep every other scalar."""
    return "".join(
        chr(ord(character) + 32) if "A" <= character <= "Z" else character
        for character in value
    )
